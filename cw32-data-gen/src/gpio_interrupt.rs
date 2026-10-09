//! Own-manual command domains and explicitly retained serviced-source policy.
use std::{collections::BTreeMap, fs, path::Path};

use anyhow::{Context, Result, ensure};
use cw32_data_serde::chip::core::{Peripheral, peripheral::GpioInterrupt};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Catalog {
    schema_version: u32,
    command_evidence: String,
    profiles: BTreeMap<String, Profile>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Profile {
    command_source: serde_json::Value,
    serviced_policy: serde_json::Value,
    level_trigger_source: String,
    peripherals: BTreeMap<String, GpioInterrupt>,
}

pub fn apply(root: &Path, path: &str, line: &str, peripherals: &mut [Peripheral]) -> Result<()> {
    let catalog: Catalog = crate::read_yaml(root.join(path))?;
    ensure!(
        catalog.schema_version == 1,
        "unsupported GPIO interrupt metadata schema"
    );
    let profile = catalog
        .profiles
        .get(line)
        .context("missing GPIO interrupt profile")?;
    let evidence: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join(&catalog.command_evidence))?)?;
    let source = evidence["families"]
        .as_array()
        .context("missing GPIO command sources")?
        .iter()
        .find(|s| s["family"].as_str() == Some(line))
        .context("missing own-family GPIO command source")?;
    ensure!(
        crate::without_source_aliases(&profile.command_source) == *source,
        "GPIO command citation differs from own-family evidence"
    );
    let bits = source["command_bits"]
        .as_u64()
        .context("missing GPIO command width")?;
    ensure!(
        (1..=16).contains(&bits),
        "GPIO command width exceeds halfword domain"
    );
    let command_mask = ((1u32 << bits) - 1) as u16;
    ensure!(
        source["sha256"].as_str().is_some_and(|s| s.len() == 64)
            && source["url"]
                .as_str()
                .is_some_and(|s| s.starts_with("https://"))
            && source["section"].as_str().is_some_and(|s| !s.is_empty()),
        "incomplete GPIO own-manual citation"
    );
    ensure!(
        profile.serviced_policy["prior_source_sha256"]
            .as_str()
            .is_some_and(|s| s.len() == 64)
            && profile.serviced_policy["basis"]
                .as_str()
                .is_some_and(|s| !s.is_empty())
            && root
                .join(profile.level_trigger_source.split('#').next().unwrap())
                .is_file(),
        "missing distinct serviced-mask/trigger policy"
    );
    let mut projected = 0;
    for peripheral in peripherals {
        if !peripheral.name.starts_with("GPIO") {
            continue;
        }
        let facts = profile
            .peripherals
            .get(&peripheral.name)
            .context("missing GPIO interrupt bank policy")?;
        ensure!(
            facts.serviced_mask != 0 && facts.serviced_mask & !command_mask == 0,
            "GPIO serviced mask outside documented command domain"
        );
        ensure!(
            facts.clear_noop_mask == command_mask,
            "GPIO command mask differs from own-manual fields"
        );
        ensure!(
            peripheral
                .interrupts
                .iter()
                .filter(|i| i.signal == "GLOBAL")
                .count()
                == 1,
            "GPIO bank must have exactly one GLOBAL link"
        );
        peripheral.gpio_interrupt = Some(facts.clone());
        projected += 1;
    }
    ensure!(
        projected == profile.peripherals.len(),
        "GPIO interrupt metadata names an absent bank"
    );
    Ok(())
}
