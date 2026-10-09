//! Direct classic GTIM inputs independently qualified from own-family sources.
use anyhow::{Context, Result, ensure};
use chiptool::ir;
use cw32_data_serde::chip::{self, core::peripheral::ClassicTimerInput};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::Path};

#[derive(Deserialize)]
struct Catalog {
    schema_version: u32,
    evidence: String,
    profiles: BTreeMap<String, Profile>,
}
#[derive(Deserialize)]
struct Profile {
    register_version: String,
    route_source: String,
    route_source_sha256: String,
    encoder_fixed_reload: Option<u16>,
    instances: Vec<String>,
    routes: Vec<Route>,
}
#[derive(Deserialize)]
struct Route {
    peripheral: String,
    pin: String,
    af: u8,
    channel: u8,
}

pub fn apply(
    root: &Path,
    line: &str,
    core: &mut chip::Core,
    registers: &BTreeMap<String, ir::IR>,
) -> Result<()> {
    if !core.peripherals.iter().any(|p| p.name.starts_with("GTIM")) {
        return Ok(());
    }
    let catalog: Catalog = crate::read_yaml(root.join("cw32-data/classic-timer-input.yaml"))?;
    ensure!(catalog.schema_version == 1, "unknown classic input catalog");
    let Some(profile) = catalog.profiles.get(line) else {
        return Ok(());
    };
    let evidence: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join(&catalog.evidence))?)?;
    ensure!(
        evidence["families"].get(line).is_some(),
        "own-family input evidence missing"
    );
    ensure!(
        evidence["common_semantics"]["qei"]["ARR_required"].as_u64()
            == profile.encoder_fixed_reload.map(u64::from),
        "QEI reload differs from own-source review"
    );
    ensure!(
        format!(
            "{:x}",
            Sha256::digest(fs::read(root.join(&profile.route_source))?)
        ) == profile.route_source_sha256,
        "classic input route-source changed; requalify physical routes"
    );
    let gtim = &registers["gtim"];
    for (register, field, offset, size) in [
        ("CR0", "ENCMODE", 15, 2),
        ("CR1", "CH1FLT", 0, 3),
        ("CMMR", "CC1M", 0, 4),
        ("ISR", "UD", 2, 1),
        ("ICR", "DIRCHANGE", 9, 1),
        ("ISR", "DIR", 10, 1),
    ] {
        ensure!(
            gtim.fieldsets[register]
                .fields
                .iter()
                .any(|f| f.name == field
                    && f.bit_offset == ir::BitOffset::Regular(offset)
                    && f.bit_size == size),
            "classic input register contract differs"
        );
    }
    let found: Vec<_> = core
        .peripherals
        .iter()
        .filter(|p| p.name.starts_with("GTIM"))
        .map(|p| p.name.clone())
        .collect();
    ensure!(
        found == profile.instances,
        "classic input instance set differs"
    );
    for peripheral in core
        .peripherals
        .iter_mut()
        .filter(|p| p.name.starts_with("GTIM"))
    {
        ensure!(
            peripheral
                .registers
                .as_ref()
                .context("missing classic GTIM registers")?
                .version
                == profile.register_version,
            "classic input register version differs"
        );
        let mux = format!("{}CAP", peripheral.name);
        let sys = &registers["sysctrl"];
        ensure!(
            sys.blocks["SYSCTRL"].items.iter().any(|i| i.name == mux),
            "classic capture mux absent"
        );
        for channel in 1..=4 {
            ensure!(
                sys.fieldsets[&mux]
                    .fields
                    .iter()
                    .any(|f| f.name == format!("CH{channel}")
                        && f.bit_offset == ir::BitOffset::Regular((channel - 1) * 4)
                        && f.bit_size == 3),
                "classic external mux contract differs"
            );
        }
        peripheral.classic_timer_input = Some(ClassicTimerInput {
            capture_mux: mux,
            encoder_fixed_reload: profile.encoder_fixed_reload,
        });
        for route in profile
            .routes
            .iter()
            .filter(|r| r.peripheral == peripheral.name)
        {
            ensure!(
                (1..=4).contains(&route.channel),
                "invalid classic input channel"
            );
            if !core.pins.iter().any(|p| p.name == route.pin) {
                continue;
            }
            let source = peripheral
                .pins
                .iter()
                .find(|p| {
                    p.pin == route.pin
                        && p.af == Some(route.af)
                        && p.signal == format!("CH{}", route.channel)
                })
                .context("qualified physical CH route absent")?;
            let mut pin = source.clone();
            pin.signal = format!("CAP{}", route.channel);
            peripheral.pins.push(pin);
        }
        peripheral
            .pins
            .sort_by(|a, b| (&a.pin, &a.signal, a.af).cmp(&(&b.pin, &b.signal, b.af)));
    }
    Ok(())
}
