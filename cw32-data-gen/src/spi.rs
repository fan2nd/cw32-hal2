//! Project-authored SPI chip-fact projection; no register-layout policy lives here.
use std::{collections::BTreeMap, fs, path::Path};

use anyhow::{Context, Result, ensure};
use cw32_data_serde::chip::core::{Peripheral, peripheral::Spi};
use serde::Deserialize;

#[derive(Deserialize)]
struct Catalog {
    schema_version: u32,
    policy: String,
    profiles: BTreeMap<String, Profile>,
}
#[derive(Deserialize)]
struct Profile {
    policy_ref: String,
    peripherals: BTreeMap<String, Spi>,
}

pub fn apply(root: &Path, path: &str, line: &str, peripherals: &mut [Peripheral]) -> Result<()> {
    let catalog: Catalog = crate::read_yaml(root.join(path))?;
    ensure!(
        catalog.schema_version == 1,
        "unsupported SPI metadata schema"
    );
    let profile = catalog
        .profiles
        .get(line)
        .context("missing SPI chip profile")?;
    // The retained policy owns the own-family electrical/manual citations and
    // qualified conservative choices. Do not inherit limits from register aliases.
    ensure!(
        profile.policy_ref == format!("#/families/{line}"),
        "SPI source is not this family"
    );
    let policy: serde_json::Value = serde_json::from_slice(&fs::read(root.join(&catalog.policy))?)?;
    let evidence = policy
        .pointer(&profile.policy_ref[1..])
        .context("missing SPI source policy")?;
    for source in ["datasheet", "manual"] {
        let source = &evidence["sources"][source];
        ensure!(
            source["sha256"].as_str().is_some_and(|s| s.len() == 64)
                && source["url"]
                    .as_str()
                    .is_some_and(|s| s.starts_with("https://"))
                && source["pages"].as_array().is_some_and(|p| !p.is_empty()),
            "missing SPI source citation"
        );
    }
    let mut projected = 0;
    for peripheral in peripherals {
        if peripheral
            .registers
            .as_ref()
            .is_none_or(|r| r.kind != "spi")
        {
            continue;
        }
        let limits = profile
            .peripherals
            .get(&peripheral.name)
            .context("missing SPI instance limits")?;
        ensure!(
            limits.maximum_frequency != 0 && limits.minimum_divisor >= 2,
            "invalid SPI clock limits"
        );
        ensure!(
            Some(u64::from(limits.maximum_frequency)) == evidence["maximum_sck_hz"].as_u64()
                && Some(u64::from(limits.minimum_divisor)) == evidence["minimum_divisor"].as_u64(),
            "SPI instance limits disagree with reviewed own-family source policy"
        );
        peripheral.spi = Some(limits.clone());
        projected += 1;
    }
    ensure!(
        projected == profile.peripherals.len(),
        "SPI metadata names a missing or non-SPI instance"
    );
    Ok(())
}
