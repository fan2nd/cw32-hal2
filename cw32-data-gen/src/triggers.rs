//! Curated, destination-specific CW32 trigger writes. No universal ITR enum.
use anyhow::{Context, Result, ensure};
use chiptool::ir;
use cw32_data_serde::chip::{
    Core,
    core::peripheral::{Trigger, TriggerRegister},
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::Path};

const PATH: &str = "cw32-data/triggers/cw32l010.yaml";
const QUALIFIED_SHA256: &str = "3c4cca3860897cbf1ca1a38e26e885887657011aeafa69ac2cb1af72bfa058ea";
const L011_PATH: &str = "cw32-data/triggers/cw32l011.yaml";
const L011_QUALIFIED_SHA256: &str =
    "0418698396e4582dd6d521f550d34db752c3b7e934f641b1f2dd2286b6c3a503";
#[derive(Clone, Deserialize)]
struct Route {
    destination: String,
    signal: String,
    source: String,
    source_event: String,
    registers: Vec<TriggerRegister>,
    status: String,
    conflict_ids: Vec<String>,
}
#[derive(Clone, Deserialize)]
struct Authoring {
    schema_version: u32,
    family: String,
    register_versions: BTreeMap<String, String>,
    status: String,
    routes: Vec<Route>,
}

/// Project only a hash-pinned reviewed sidecar, after validating all real fields.
/// Original manual/SDK bytes are independently checked by the evidence verifier.
pub fn apply(
    root: &Path,
    path: &str,
    profile: &str,
    core: &mut Core,
    registers: &BTreeMap<String, ir::IR>,
) -> Result<usize> {
    let qualified_sha256 = match (path, profile) {
        (PATH, "CW32L010") => QUALIFIED_SHA256,
        (L011_PATH, "CW32L011") => L011_QUALIFIED_SHA256,
        _ => anyhow::bail!("unqualified trigger family/path"),
    };
    let bytes = fs::read(root.join(path))?;
    ensure!(
        format!("{:x}", Sha256::digest(&bytes)) == qualified_sha256,
        "trigger authoring changed; requalify own-family sources"
    );
    project(
        &crate::parse_yaml(Path::new(path), &bytes)?,
        profile,
        core,
        registers,
    )
}
fn project(
    file: &Authoring,
    profile: &str,
    core: &mut Core,
    registers: &BTreeMap<String, ir::IR>,
) -> Result<usize> {
    ensure!(
        file.schema_version == 1
            && matches!(file.family.as_str(), "CW32L010" | "CW32L011")
            && profile == file.family
            && file.status == "verified-own-manual-sdk",
        "unqualified trigger profile"
    );
    let adc_version = match file.family.as_str() {
        "CW32L010" => "cw32l010_v1",
        "CW32L011" => "cw32l011_v1",
        _ => unreachable!(),
    };
    let version_for = |name: &str| {
        if name == "ADC" {
            adc_version
        } else {
            "cw32l010_v1"
        }
    };
    ensure!(file.routes.len() == 2, "unexpected qualified trigger set");
    ensure!(
        file.register_versions.len() == 3
            && ["BTIM1", "BTIM2", "ADC"].iter().all(|name| file
                .register_versions
                .get(*name)
                .is_some_and(|v| v == version_for(name))),
        "incomplete or unqualified trigger register identities"
    );
    for (name, version) in &file.register_versions {
        let reference = core
            .peripherals
            .iter()
            .find(|p| p.name == *name)
            .context("trigger peripheral absent")?
            .registers
            .as_ref()
            .context("trigger register identity absent")?;
        ensure!(
            reference.version == *version && version == version_for(name),
            "trigger hardware version differs"
        );
    }
    for route in &file.routes {
        ensure!(
            route.status == "documented" && route.conflict_ids.is_empty(),
            "conflicted or reserved trigger cannot bind"
        );
        ensure!(
            route.source == "BTIM1_TRGO" && route.source_event == "UPDATE",
            "unqualified source event"
        );
        let (signal, register, field, role, offset, size, value) = match route.destination.as_str()
        {
            "ADC" => (
                "START_CONVERSION",
                "TRIGGER",
                "BTIM1TRGO",
                "independent_enable",
                13,
                1,
                1,
            ),
            "BTIM2" => ("TRGI", "SMCR", "TRGISRC", "selector", 7, 4, 8),
            _ => anyhow::bail!("unqualified destination or self-feedback"),
        };
        ensure!(
            route.signal == signal && route.registers.len() == 2,
            "destination signal differs"
        );
        let expected = [
            TriggerRegister {
                peripheral: "BTIM1".into(),
                register: "CR2".into(),
                field: "MMS".into(),
                role: "source_event".into(),
                bit_offset: 4,
                bit_size: 3,
                value: 2,
            },
            TriggerRegister {
                peripheral: route.destination.clone(),
                register: register.into(),
                field: field.into(),
                role: role.into(),
                bit_offset: offset,
                bit_size: size,
                value,
            },
        ];
        ensure!(
            route.registers == expected,
            "destination-local selector/enable or source event differs"
        );
        for write in &route.registers {
            validate_field(write, core, registers)?;
        }
    }
    ensure!(
        file.routes
            .iter()
            .filter(|r| r.destination == "ADC")
            .count()
            == 1
            && file
                .routes
                .iter()
                .filter(|r| r.destination == "BTIM2")
                .count()
                == 1,
        "duplicate/missing destination"
    );
    // Transactional: never leave a partial legal binding on any rejected input.
    let mut projected = core.clone();
    for route in &file.routes {
        let destination = projected
            .peripherals
            .iter_mut()
            .find(|p| p.name == route.destination)
            .context("destination absent")?;
        ensure!(
            destination.triggers.is_empty(),
            "duplicate trigger projection"
        );
        destination.triggers.push(Trigger {
            signal: route.signal.clone(),
            source: route.source.clone(),
            registers: route.registers.clone(),
        });
    }
    *core = projected;
    Ok(file.routes.len())
}
fn validate_field(
    write: &TriggerRegister,
    core: &Core,
    registers: &BTreeMap<String, ir::IR>,
) -> Result<()> {
    let peripheral = core
        .peripherals
        .iter()
        .find(|p| p.name == write.peripheral)
        .context("trigger field owner absent")?;
    let reference = peripheral
        .registers
        .as_ref()
        .context("trigger register identity absent")?;
    let ir = registers
        .get(&reference.kind)
        .context("trigger IR absent")?;
    let block = ir
        .blocks
        .get(&reference.block)
        .context("trigger block absent")?;
    let item = block
        .items
        .iter()
        .find(|i| i.name == write.register)
        .context("trigger register absent")?;
    let ir::BlockItemInner::Register(register) = &item.inner else {
        anyhow::bail!("nested trigger register");
    };
    ensure!(
        register.access == ir::Access::ReadWrite && register.bit_size == 32,
        "trigger register must be 32-bit RW"
    );
    let fieldset = ir
        .fieldsets
        .get(
            register
                .fieldset
                .as_ref()
                .context("trigger fieldset identity absent")?,
        )
        .context("trigger fieldset absent")?;
    let field = fieldset
        .fields
        .iter()
        .find(|f| f.name == write.field)
        .context("trigger field absent/reserved")?;
    ensure!(
        field.bit_offset == ir::BitOffset::Regular(write.bit_offset)
            && field.bit_size == write.bit_size
            && write.bit_size < 32
            && write.value < (1 << write.bit_size),
        "trigger field layout/value differs"
    );
    ensure!(
        write.role != "independent_enable" || (write.bit_size == 1 && write.value == 1),
        "ADC trigger is an independent one-bit enable"
    );
    Ok(())
}
#[cfg(test)]
mod tests;
