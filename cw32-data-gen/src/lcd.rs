//! Own-family LCD register domains and physical COM/SEG/VLCD routes.
use anyhow::{Context, Result, ensure};
use chiptool::ir;
use cw32_data_serde::chip::{
    self,
    core::peripheral::{Lcd, Pin},
};
use serde::Deserialize;
use std::{collections::BTreeMap, fs, path::Path};
#[derive(Deserialize)]
struct Catalog {
    schema_version: u32,
    evidence: String,
    profiles: BTreeMap<String, Profile>,
}
#[derive(Deserialize)]
struct Profile {
    facts: Lcd,
    routes: Vec<Route>,
}
#[derive(Deserialize)]
struct Route {
    pin: String,
    signal: String,
    table: String,
    pdf_page: u32,
    analog_cell: String,
}
pub fn apply(
    root: &Path,
    line: &str,
    core: &mut chip::Core,
    registers: &BTreeMap<String, ir::IR>,
) -> Result<()> {
    let data: Catalog = crate::read_yaml(root.join("cw32-data/lcd.yaml"))?;
    ensure!(data.schema_version == 1, "unknown LCD schema");
    let profile = data
        .profiles
        .get(line)
        .context("missing own-family LCD facts")?;
    let proof: serde_json::Value = serde_json::from_slice(&fs::read(root.join(&data.evidence))?)?;
    ensure!(
        serde_json::to_value(&profile.facts)? == proof["families"][line]["facts"],
        "LCD facts differ from own-source review"
    );
    let peripheral = core
        .peripherals
        .iter_mut()
        .find(|p| p.name == "LCD")
        .unwrap();
    let ir = &registers["lcd"];
    for n in &profile.facts.ram_registers {
        ensure!(
            ir.blocks["LCD"]
                .items
                .iter()
                .any(|r| r.name == format!("RAM{n}") && r.byte_offset == 64 + u32::from(*n) * 4),
            "LCD RAM layout mismatch"
        );
    }
    let actual: Vec<_> = ir.blocks["LCD"]
        .items
        .iter()
        .filter(|r| r.name.starts_with("RAM"))
        .map(|r| r.name[3..].parse::<u8>().unwrap())
        .collect();
    let mut expected = profile.facts.ram_registers.clone();
    expected.sort();
    let mut actual = actual;
    actual.sort();
    ensure!(actual == expected, "LCD unsupported RAM register exposed");
    ensure!(
        profile
            .facts
            .segments
            .iter()
            .all(|n| profile.facts.ram_registers.contains(&(n / 4))),
        "LCD segment without RAM"
    );
    for (reg, field, enumm) in [
        ("CR0", "DUTY", "Duty"),
        ("CR0", "BIAS", "Bias"),
        ("CR0", "INRS", "BiasSource"),
        ("CR1", "CLKCS", "ClockSource"),
        ("CR1", "LCDFS", "ScanFrequency"),
    ] {
        ensure!(
            ir.fieldsets[reg]
                .fields
                .iter()
                .any(|f| f.name == field && f.enumm.as_deref() == Some(enumm)),
            "LCD typed selector missing"
        );
    }
    peripheral.lcd = Some(profile.facts.clone());
    for route in &profile.routes {
        ensure!(
            route.table == "5-2" && route.pdf_page > 0 && route.analog_cell.contains(&route.signal),
            "LCD route source cell missing"
        );
        if core.pins.iter().any(|p| p.name == route.pin) {
            peripheral.pins.push(Pin {
                pin: route.pin.clone(),
                signal: route.signal.clone(),
                af: None,
                adc_mux: None,
                comparator_mux: None,
            });
        }
    }
    peripheral
        .pins
        .sort_by(|a, b| (&a.pin, &a.signal).cmp(&(&b.pin, &b.signal)));
    Ok(())
}
