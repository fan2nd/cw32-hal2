//! Native classic ATIM A/B inputs qualified independently from output routes.
use anyhow::{Context, Result, ensure};
use chiptool::ir;
use cw32_data_serde::{
    chip::{
        self,
        core::peripheral::{ClassicAtimInput, Pin},
    },
    register_write::RegisterWrites,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Catalog {
    schema_version: u32,
    evidence: String,
    evidence_sha256: String,
    profiles: BTreeMap<String, Profile>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Profile {
    register_version: String,
    protocol: ClassicAtimInput,
    pinout: String,
    pinout_sha256: String,
    routes: Vec<Route>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Route {
    pin: String,
    af: u8,
    signal: String,
    evidence_pointer: String,
}
#[derive(Deserialize)]
struct Pinouts {
    family: String,
    packages: Vec<Package>,
}
#[derive(Deserialize)]
struct Package {
    name: String,
    family_part: String,
    table_column: String,
    pins: Vec<chip::PackagePin>,
}

fn field(ir: &ir::IR, register: &str, name: &str, bit: u32, width: u32) -> Result<()> {
    let block = ir.blocks.get("ATIM").context("native ATIM block missing")?;
    let item = block
        .items
        .iter()
        .find(|i| i.name == register)
        .with_context(|| format!("native ATIM register {register} missing"))?;
    let ir::BlockItemInner::Register(reg) = &item.inner else {
        anyhow::bail!("native ATIM item must be a register");
    };
    ensure!(
        reg.fieldset.as_deref() == Some(register),
        "native ATIM fieldset differs"
    );
    let fields = &ir
        .fieldsets
        .get(register)
        .context("native ATIM fieldset missing")?
        .fields;
    ensure!(
        fields.iter().any(|f| f.name == name
            && f.bit_offset == ir::BitOffset::Regular(bit)
            && f.bit_size == width
            && f.array.is_none()),
        "native ATIM {register}.{name} differs"
    );
    Ok(())
}

fn register_contract(root: &Path, version: &str, ir: &ir::IR) -> Result<()> {
    let block = ir.blocks.get("ATIM").context("native ATIM block missing")?;
    for (name, offset) in [
        ("ARR", 0),
        ("CNT", 4),
        ("CR", 12),
        ("ISR", 16),
        ("ICR", 20),
        ("MSCR", 24),
        ("FLTR", 28),
        ("CH1CR", 36),
        ("CH2CR", 40),
        ("CH3CR", 44),
        ("RCR", 52),
        ("CH1CCRA", 60),
        ("CH1CCRB", 64),
        ("CH2CCRA", 68),
        ("CH2CCRB", 72),
        ("CH3CCRA", 76),
        ("CH3CCRB", 80),
    ] {
        let item = block
            .items
            .iter()
            .find(|i| i.name == name)
            .context("native ATIM register absent")?;
        let ir::BlockItemInner::Register(register) = &item.inner else {
            anyhow::bail!("native ATIM item must be a register");
        };
        ensure!(
            item.byte_offset == offset
                && item.array.is_none()
                && register.bit_size == 32
                && (name != "ISR" || register.access == ir::Access::Read),
            "native ATIM register offset, width or access differs"
        );
    }
    for (register, name, bit, width) in [
        ("CR", "EN", 0, 1),
        ("CR", "CT", 2, 1),
        ("CR", "PRS", 4, 3),
        ("CR", "BUFPEN", 7, 1),
        ("CR", "UIE", 10, 1),
        ("CR", "MODE", 12, 2),
        ("CR", "ONESHOT", 14, 1),
        ("CR", "URS", 17, 1),
        ("CR", "UG", 25, 1),
        ("CR", "DIR", 27, 1),
        ("MSCR", "SMS", 8, 3),
        ("MSCR", "IA1S", 11, 1),
        ("MSCR", "IB1S", 12, 1),
        ("ARR", "ARR", 0, 16),
        ("CNT", "CNT", 0, 16),
        ("RCR", "RCR", 0, 8),
        ("RCR", "OV", 8, 1),
        ("RCR", "UD", 9, 1),
        ("ISR", "UIF", 0, 1),
        ("ICR", "UIF", 0, 1),
    ] {
        field(ir, register, name, bit, width)?;
    }
    for channel in 1..=3 {
        let control = format!("CH{channel}CR");
        for (name, bit, width) in [
            ("BKSA", 0, 2),
            ("BKSB", 2, 2),
            ("CSA", 4, 1),
            ("CSB", 5, 1),
            ("BUFEA", 6, 1),
            ("BUFEB", 7, 1),
            ("CIEA", 8, 1),
            ("CIEB", 9, 1),
            ("CISB", 12, 2),
            ("CCGA", 14, 1),
            ("CCGB", 15, 1),
        ] {
            field(ir, &control, name, bit, width)?;
        }
        // F003 has no DMA fields. Do not alias or normalize that register contract.
        for (name, bit) in [("CDEA", 10), ("CDEB", 11)] {
            if version == "cw32f003_v1" {
                ensure!(
                    !ir.fieldsets[&control].fields.iter().any(|f| f.name == name),
                    "F003 native channel unexpectedly has DMA fields"
                );
            } else {
                field(ir, &control, name, bit, 1)?;
            }
        }
        for (half, flag, filter) in [
            ("A", channel + 1, (channel - 1) * 8),
            ("B", channel + 4, (channel - 1) * 8 + 4),
        ] {
            field(
                ir,
                "FLTR",
                &format!("OCM{channel}{half}FLT{channel}{half}"),
                filter,
                3,
            )?;
            field(ir, "FLTR", &format!("CCP{channel}{half}"), filter + 3, 1)?;
            field(
                ir,
                &format!("CH{channel}CCR{half}"),
                &format!("CCR{channel}{half}"),
                0,
                16,
            )?;
            for register in ["ISR", "ICR"] {
                field(ir, register, &format!("C{channel}{half}F"), flag, 1)?;
                field(ir, register, &format!("C{channel}{half}E"), flag + 6, 1)?;
            }
        }
    }
    // L052's imported MSM bit4 is reserved by its own manual; it is not an input capability.
    let writes: RegisterWrites = crate::read_yaml(root.join("cw32-data/register-writes.yaml"))?;
    ensure!(
        writes.schema_version == 1,
        "unknown native ATIM command policy"
    );
    let entries = writes
        .registers
        .get(&format!("atim_{version}"))
        .context("native ATIM command policy missing")?;
    let command = entries
        .iter()
        .find(|e| e.block == "ATIM" && e.register == "ICR")
        .context("native ATIM ICR policy missing")?;
    let expected: BTreeSet<_> = ir.fieldsets["ICR"]
        .fields
        .iter()
        .map(|f| f.name.clone())
        .collect();
    ensure!(
        command.fieldset == "ICR"
            && command.reset_value == 0x7ffff
            && command.write_noop == 0x7ffff
            && command
                .zero_to_clear_fields
                .iter()
                .cloned()
                .collect::<BTreeSet<_>>()
                == expected
            && command.zero_to_clear_fields.len() == expected.len(),
        "native ATIM ICR policy must preserve reserved reset-one bit1 and unrelated flags"
    );
    Ok(())
}

pub fn apply(
    root: &Path,
    line: &str,
    selection: &str,
    core: &mut chip::Core,
    registers: &BTreeMap<String, ir::IR>,
) -> Result<()> {
    if core.peripherals.is_empty() {
        return Ok(());
    }
    let catalog: Catalog = crate::read_yaml(root.join("cw32-data/classic-atim-input.yaml"))?;
    ensure!(
        catalog.schema_version == 1,
        "unknown native ATIM input catalog"
    );
    let Some(profile) = catalog.profiles.get(line) else {
        return Ok(());
    };
    ensure!(
        profile.protocol == ClassicAtimInput::ClassicPairedV1
            && matches!(
                profile.register_version.as_str(),
                "v1" | "cw32f003_v1" | "cw32l052_v1"
            ),
        "unqualified native ATIM protocol/register combination"
    );
    let bytes = fs::read(root.join(&catalog.evidence))?;
    ensure!(
        format!("{:x}", Sha256::digest(&bytes)) == catalog.evidence_sha256,
        "native ATIM evidence changed; requalify the catalog"
    );
    let proof: serde_json::Value = serde_json::from_slice(&bytes)?;
    let own = &proof["classic_atim_input"]["families"][line];
    ensure!(
        proof["schema_version"] == 1
            && own["protocol"] == "ClassicPairedV1"
            && own["register_variant"] == profile.register_version
            && own["pinout"]["path"] == profile.pinout
            && own["pinout"]["sha256"] == profile.pinout_sha256,
        "native ATIM own-family qualification differs"
    );
    ensure!(
        format!(
            "{:x}",
            Sha256::digest(fs::read(root.join(&profile.pinout))?)
        ) == profile.pinout_sha256,
        "native ATIM package source changed"
    );
    let pinouts: Pinouts = crate::read_yaml(root.join(&profile.pinout))?;
    ensure!(pinouts.family == line, "native ATIM package family differs");
    let exact = pinouts.packages.iter().find(|p| p.name == selection);
    let packages: Vec<_> = pinouts
        .packages
        .iter()
        .filter(|p| {
            exact.map_or(selection == line || selection == p.family_part, |e| {
                p.name == e.name
            })
        })
        .collect();
    ensure!(
        !packages.is_empty(),
        "native ATIM selection lacks own package evidence"
    );
    let peripheral = core
        .peripherals
        .iter_mut()
        .find(|p| p.name == "ATIM")
        .context("qualified native ATIM is absent")?;
    let identity = peripheral
        .registers
        .as_ref()
        .context("native ATIM registers absent")?;
    ensure!(
        identity.kind == "atim"
            && identity.block == "ATIM"
            && identity.version == profile.register_version,
        "native ATIM actual register identity differs"
    );
    let register_source = &proof["counter_events"]["families"][line]["banks"]["ATIM"];
    let path = register_source["register_file"]
        .as_str()
        .context("own ATIM register source absent")?;
    ensure!(
        register_source["register_variant"] == identity.version
            && register_source["register_file_sha256"]
                == format!("{:x}", Sha256::digest(fs::read(root.join(path))?)),
        "native ATIM canonical register source changed"
    );
    register_contract(
        root,
        &identity.version,
        registers.get("atim").context("native ATIM IR absent")?,
    )?;
    ensure!(
        peripheral.classic_atim_input.is_none(),
        "duplicate native ATIM fact projection"
    );
    let mut seen = BTreeSet::new();
    let mut additions = Vec::new();
    for route in &profile.routes {
        ensure!(
            matches!(
                route.signal.as_str(),
                "CH1A" | "CH1B" | "CH2A" | "CH2B" | "CH3A" | "CH3B"
            ) && route.af < 8
                && seen.insert((route.pin.clone(), route.signal.clone(), route.af)),
            "invalid or duplicate native ATIM input route"
        );
        ensure!(
            route
                .evidence_pointer
                .starts_with(&format!("/classic_atim_input/families/{line}/routes/")),
            "native ATIM route evidence belongs to another family"
        );
        let evidence = proof
            .pointer(&route.evidence_pointer)
            .context("native input route evidence absent")?;
        ensure!(
            evidence["pin"] == route.pin
                && evidence["af"] == route.af
                && evidence["signal"] == route.signal
                && evidence["input_source_qualified"] == true
                && evidence["withheld_reasons"]
                    .as_array()
                    .is_some_and(|a| a.is_empty()),
            "native ATIM route lacks positive own-source qualification"
        );
        for key in ["own_datasheet_cell", "own_manual_cell", "own_sdk_macro"] {
            ensure!(
                evidence[key]["pin"] == route.pin
                    && evidence[key]["af"] == route.af
                    && evidence[key]["signal"] == route.signal,
                "native input source cell differs"
            );
        }
        if !core.pins.iter().any(|p| p.name == route.pin) {
            continue;
        }
        let mut bonded = true;
        for package in &packages {
            let pin = package.pins.iter().find(|p| p.signals.contains(&route.pin));
            let position = evidence["pin_definition"]["positions"][&package.table_column].as_str();
            ensure!(
                pin.map(|p| p.position.as_str()) == position
                    && evidence["package_pins"][&package.name].as_str() == position,
                "native input package position differs from own-source evidence"
            );
            bonded &= position.is_some();
        }
        if !bonded {
            continue;
        }
        let signal = format!("CAP{}", &route.signal[2..]);
        ensure!(
            !peripheral
                .pins
                .iter()
                .any(|p| p.pin == route.pin && p.signal == signal && p.af == Some(route.af)),
            "native ATIM capture route already exists"
        );
        additions.push(Pin {
            pin: route.pin.clone(),
            signal,
            af: Some(route.af),
            adc_mux: None,
            comparator_mux: None,
        });
    }
    let qualified: BTreeSet<_> = own["routes"]
        .as_array()
        .context("native input source routes absent")?
        .iter()
        .filter(|r| r["input_source_qualified"] == true)
        .map(|r| {
            Ok((
                r["pin"].as_str().context("source pin missing")?.to_owned(),
                r["signal"]
                    .as_str()
                    .context("source signal missing")?
                    .to_owned(),
                u8::try_from(r["af"].as_u64().context("source AF missing")?)?,
            ))
        })
        .collect::<Result<_>>()?;
    ensure!(
        qualified == seen,
        "native ATIM catalog differs from the complete qualified source routes"
    );
    peripheral.classic_atim_input = Some(ClassicAtimInput::ClassicPairedV1);
    peripheral.pins.extend(additions);
    peripheral
        .pins
        .sort_by(|a, b| (&a.pin, &a.signal, a.af).cmp(&(&b.pin, &b.signal, b.af)));
    Ok(())
}
