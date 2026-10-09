//! Own-family shared comparator-divider facts; no inferred standalone RCC domain.
use anyhow::{Context, Result, ensure};
use cw32_data_serde::chip::{self, core::peripheral::ReferenceDivider};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

#[derive(Deserialize)]
struct Catalog {
    schema_version: u32,
    profiles: Vec<Profile>,
}
#[derive(Deserialize)]
struct Profile {
    line: String,
    sources: Value,
    register_version: String,
    register_kind: String,
    legal_div_range: [u8; 2],
    ratio_denominator: u8,
    instances: Vec<Instance>,
}
#[derive(Deserialize)]
struct Instance {
    name: String,
    facts: ReferenceDivider,
}

pub fn apply(root: &Path, line: &str, core: &mut chip::Core) -> Result<()> {
    let path = Path::new("cw32-data/reference-dividers.yaml");
    let bytes = fs::read(root.join(path))?;
    ensure!(
        format!("{:x}", Sha256::digest(&bytes)) == CATALOG_HASH,
        "reference-divider own-source qualification changed; review before accepting"
    );
    let catalog: Catalog = crate::parse_yaml(path, &bytes)?;
    ensure!(
        catalog.schema_version == 1,
        "unknown reference-divider schema"
    );
    let Some(profile) = catalog.profiles.into_iter().find(|p| p.line == line) else {
        return Ok(());
    };
    ensure!(
        matches!(line, "CW32L010" | "CW32L011" | "CW32L012")
            && profile.legal_div_range == [0, 7]
            && profile.ratio_denominator == 8,
        "unqualified comparator reference family or ratio"
    );
    let lock: Value =
        serde_json::from_slice(&fs::read(root.join("sources/evidence-sources.json"))?)?;
    for source in profile
        .sources
        .as_object()
        .context("missing divider sources")?
        .values()
    {
        let id = source["source_ref"].as_str().context("source ID missing")?;
        let mut found = false;
        for artifact in lock["artifacts"].as_array().unwrap() {
            if artifact["id"] == id {
                ensure!(
                    artifact["path"] == source["file"]
                        && artifact["sha256"] == source["sha256"]
                        && artifact["url"] == source["url"],
                    "divider source lock mismatch"
                );
                found = true;
            }
            for member in artifact["members"].as_array().into_iter().flatten() {
                if id == format!("member:{}", member["path"].as_str().unwrap()) {
                    ensure!(
                        member["path"] == source["file"] && member["sha256"] == source["sha256"],
                        "divider SDK member lock mismatch"
                    );
                    found = true;
                }
            }
        }
        ensure!(found, "unlocked divider source: {id}");
    }
    for instance in profile.instances {
        let facts = &instance.facts;
        ensure!(
            facts.consumers.len() == 2
                && facts.consumers[0] != facts.consumers[1]
                && facts.consumers.contains(&facts.clock_owner)
                && facts.negative_mux == 3,
            "invalid divider ownership group"
        );
        let owner = core
            .peripherals
            .iter()
            .find(|p| p.name == facts.clock_owner)
            .context("divider clock owner absent")?;
        let clock = owner
            .rcc_control
            .as_ref()
            .context("divider owner clock unqualified")?;
        ensure!(
            clock.shared_enable_group.is_some() && clock.shared_reset_group.is_some(),
            "divider requires shared comparator clock control"
        );
        for consumer in &facts.consumers {
            let p = core
                .peripherals
                .iter()
                .find(|p| &p.name == consumer)
                .context("divider consumer absent")?;
            let limits = p
                .comparator_limits
                .as_ref()
                .context("divider consumer limits absent")?;
            ensure!(
                p.registers.as_ref().is_some_and(|r| r.kind == "vc")
                    && p.rcc_control.as_ref() == Some(clock)
                    && limits.supply_mv == facts.supply_mv
                    && limits.input_uses_vdda == facts.input_uses_vdda,
                "divider own-family consumer/clock/electrical mismatch"
            );
        }
        let p = core
            .peripherals
            .iter_mut()
            .find(|p| p.name == instance.name)
            .context("divider instance absent")?;
        ensure!(
            p.registers.as_ref().is_some_and(
                |r| r.kind == profile.register_kind && r.version == profile.register_version
            ),
            "divider register identity mismatch"
        );
        ensure!(
            p.rcc.is_none() && p.rcc_control.is_none(),
            "divider helper must not invent its own RCC domain"
        );
        p.reference_divider = Some(instance.facts);
    }
    Ok(())
}

const CATALOG_HASH: &str = "eb319effab1def2f79f72d65a5d5a4cf190f3076761eff2082e968952bb20b3f";
