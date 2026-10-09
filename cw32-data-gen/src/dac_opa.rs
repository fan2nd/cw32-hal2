//! L012 analog-output facts, qualified against its own manual and package tables.
use anyhow::{Context, Result, ensure};
use cw32_data_serde::chip::{
    self,
    core::peripheral::{DacLimits, OpaLimits},
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

#[derive(Deserialize)]
struct Catalog {
    schema_version: u32,
    profile: String,
    sources: serde_json::Value,
    limits: Limits,
    routes: Vec<Route>,
}
#[derive(Deserialize)]
struct Limits {
    dac: DacLimits,
    opa: OpaLimits,
}
#[derive(Deserialize)]
struct Route {
    peripheral: String,
    pin: String,
    signal: String,
    af: Option<u8>,
}

pub fn apply(root: &Path, line: &str, core: &mut chip::Core) -> Result<()> {
    if line != "CW32L012" {
        return Ok(());
    }
    let bytes = fs::read(root.join("cw32-data/dac-opa.yaml"))?;
    ensure!(
        format!("{:x}", Sha256::digest(&bytes)) == CATALOG_HASH,
        "DAC/OPA own-source qualification changed; review before accepting"
    );
    let c: Catalog = crate::parse_yaml(Path::new("cw32-data/dac-opa.yaml"), &bytes)?;
    ensure!(
        c.schema_version == 1 && c.profile == line,
        "DAC/OPA identity mismatch"
    );
    let lock: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("sources/evidence-sources.json"))?)?;
    for key in ["datasheet", "reference_manual", "sdk_archive"] {
        let source = &c.sources[key];
        let entry = lock["artifacts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["path"] == source["file"])
            .context("analog output source not locked")?;
        ensure!(
            entry["sha256"] == source["sha256"] && entry["url"] == source["url"],
            "analog output source mismatch"
        );
    }
    let pin_source = &c.sources["pinouts"];
    let bytes = fs::read(root.join(pin_source["file"].as_str().unwrap()))?;
    ensure!(
        format!("{:x}", Sha256::digest(bytes)) == pin_source["sha256"].as_str().unwrap(),
        "analog output package source changed"
    );
    for p in &mut core.peripherals {
        if p.name == "DAC" {
            p.dac_limits = Some(c.limits.dac.clone());
        }
        if matches!(p.name.as_str(), "OPA1" | "OPA2") {
            p.opa_limits = Some(c.limits.opa.clone());
        }
    }
    for route in c.routes {
        ensure!(
            route.af.is_none(),
            "analog output route must not set a digital AF"
        );
        if !core.pins.iter().any(|p| p.name == route.pin) {
            continue;
        }
        let p = core
            .peripherals
            .iter_mut()
            .find(|p| p.name == route.peripheral)
            .context("analog output instance missing")?;
        ensure!(
            p.registers.as_ref().is_some_and(
                |r| r.version == "cw32l012_v1" && matches!(r.kind.as_str(), "dac" | "opa")
            ),
            "analog output register identity mismatch"
        );
        ensure!(
            p.rcc_control.is_some(),
            "analog output clock gate not qualified"
        );
        ensure!(
            !p.pins
                .iter()
                .any(|p| p.pin == route.pin && p.signal == route.signal),
            "duplicate analog output route"
        );
        p.pins.push(chip::core::peripheral::Pin {
            pin: route.pin,
            signal: route.signal,
            af: None,
            adc_mux: None,
            comparator_mux: None,
        });
        p.pins
            .sort_by(|a, b| (&a.pin, &a.signal).cmp(&(&b.pin, &b.signal)));
    }
    Ok(())
}
const CATALOG_HASH: &str = "182308f8d959135cd8094e9617b5f2e948669e2ca580c3ea9b184fa1193b30e3";
