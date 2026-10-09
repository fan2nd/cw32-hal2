//! Own-family qualification for the classic ADC's common-timing ordered scan.
use anyhow::{Context, Result, ensure};
use chiptool::ir;
use cw32_data_serde::chip::{
    self,
    core::peripheral::{AdcSequence, ClassicAdcScan},
};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::Path};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Catalog {
    schema_version: u32,
    evidence: String,
    evidence_sha256: String,
    electrical_policy: String,
    electrical_policy_sha256: String,
    startup_policy: String,
    startup_policy_sha256: String,
    profiles: BTreeMap<String, Profile>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Profile {
    register_version: String,
    facts: ClassicAdcScan,
    sequence: AdcSequence,
}

fn locked_json(root: &Path, path: &str, expected: &str) -> Result<Value> {
    let bytes = fs::read(root.join(path))?;
    ensure!(
        format!("{:x}", Sha256::digest(&bytes)) == expected,
        "classic ADC source review changed: {path}"
    );
    Ok(serde_json::from_slice(&bytes)?)
}

pub(crate) fn apply(
    root: &Path,
    path: &str,
    line: &str,
    core: &mut chip::Core,
    registers: &BTreeMap<String, ir::IR>,
) -> Result<()> {
    let catalog: Catalog = crate::read_yaml(root.join(path))?;
    ensure!(
        catalog.schema_version == 1,
        "unknown classic ADC scan catalog"
    );
    let profile = catalog
        .profiles
        .get(line)
        .context("missing own-family classic ADC scan profile")?;
    let proof = locked_json(root, &catalog.evidence, &catalog.evidence_sha256)?;
    let own = proof["families"]
        .as_array()
        .context("missing reviewed classic ADC families")?
        .iter()
        .find(|family| family["family"] == line)
        .context("missing own-family classic ADC review")?;
    let facts = &profile.facts;
    let sequence = &profile.sequence;
    // Evidence is reviewed separately for each selected line, including the
    // R031 mux numbering and x030 shared manual's explicit family scope.
    let channels = &own["channels"];
    let constraints = &own["conservative_policy"];
    ensure!(
        sequence.programmable_order
            && !sequence.per_slot_sample_time
            && sequence.per_slot_result
            && own["sequence"]["ordered_slots"] == sequence.programmable_order
            && own["common_controls"]["per_slot_controls"] == sequence.per_slot_sample_time
            && own["results"]["count"] == sequence.maximum_length
            && own["sequence"]["registers"]
                .as_array()
                .is_some_and(
                    |registers| registers.iter().all(|register| register["slot_indices"]
                        .as_array()
                        .is_some_and(
                            |slots| slots.len() == usize::from(facts.slots_per_sequence_register)
                        ))
                )
            && own["family"] == line
            && own["register_version"] == profile.register_version
            && own["slots"] == sequence.maximum_length
            && channels["supply_div3"] == facts.supply_channel
            && channels["temperature"].as_u64() == facts.temperature_channel.map(u64::from)
            && channels["vrefint_1v2"].as_u64() == facts.bandgap_channel.map(u64::from)
            && constraints["buffered_requires_single_channel_single_shot"]
                == facts.buffered_requires_single_channel
            && constraints["internal_requires_single_channel_single_shot"]
                == facts.internal_requires_single_channel,
        "classic ADC scan facts differ from own-source review"
    );
    let lock: Value =
        serde_json::from_slice(&fs::read(root.join("sources/evidence-sources.json"))?)?;
    ensure!(
        lock["artifacts"]
            .as_array()
            .context("missing original sources")?
            .iter()
            .any(|source| source["kind"] == "pdf"
                && source["provenance"]["status"] == "selected"
                && source["provenance"]["chip_scope"]
                    .as_array()
                    .is_some_and(|scope| scope.iter().any(|family| family == line))
                && source["path"] == own["source_path"]
                && source["sha256"] == own["source_sha256"]),
        "classic ADC manual is outside the own-family selected-source lock"
    );
    ensure!(
        matches!(sequence.maximum_length, 4 | 8)
            && facts.buffered_requires_single_channel
            && facts.internal_requires_single_channel
            && facts.first_internal_channel == facts.supply_channel
            && facts.temperature_channel.is_some() == facts.bandgap_channel.is_some(),
        "unsupported classic ADC operating constraints"
    );
    let electrical = locked_json(
        root,
        &catalog.electrical_policy,
        &catalog.electrical_policy_sha256,
    )?;
    let own_electrical = &electrical["families"][line]["adc"];
    let startup_policy = fs::read(root.join(&catalog.startup_policy))?;
    ensure!(
        format!("{:x}", Sha256::digest(&startup_policy)) == catalog.startup_policy_sha256,
        "classic ADC startup review changed"
    );
    if facts.temperature_channel.is_some() {
        // 50 us and 25 us are deliberately conservative software guards. The
        // source temperature maximum is 45 us; BGR startup is only approximate
        // (20 us), so its guard does not establish a characterized maximum.
        ensure!(
            own_electrical["temperature_startup_max_us"]
                .as_u64()
                .is_some_and(
                    |max| max.checked_add(5) == Some(u64::from(facts.temperature_startup_us))
                )
                && facts.bandgap_startup_us == 25,
            "classic ADC source guards are not qualified"
        );
        if let Some(supplement) = proof["supplementary_startup_evidence"]
            .as_array()
            .and_then(|all| all.iter().find(|source| source["family"] == line))
        {
            ensure!(
                supplement["software_temperature_guard_us"] == facts.temperature_startup_us
                    && supplement["software_bgr_guard_us"] == facts.bandgap_startup_us,
                "classic ADC supplementary startup review differs"
            );
        } else {
            ensure!(
                std::str::from_utf8(&startup_policy)?.contains(line.trim_start_matches("CW32")),
                "missing own-family startup review"
            );
        }
    } else {
        ensure!(
            facts.temperature_startup_us == 0 && facts.bandgap_startup_us == 0,
            "absent source has a startup guard"
        );
    }
    let adc = core
        .peripherals
        .iter_mut()
        .find(|p| p.name == "ADC")
        .context("missing classic ADC")?;
    let selected = adc
        .registers
        .as_ref()
        .context("missing classic ADC registers")?;
    ensure!(
        selected.kind == "adc" && selected.version == profile.register_version,
        "classic ADC selected register version mismatch"
    );
    let ir = registers
        .get("adc")
        .context("missing classic ADC register IR")?;
    let block = &ir.blocks["ADC"];
    let result = block
        .items
        .iter()
        .find(|i| i.name == "RESULT")
        .context("missing normalized ADC results")?;
    let ir::BlockItemInner::Register(result_register) = &result.inner else {
        anyhow::bail!("ADC result must be a register")
    };
    ensure!(
        result.byte_offset == 0x20
            && result.array
                == Some(ir::Array::Regular(ir::RegularArray {
                    len: u32::from(sequence.maximum_length),
                    stride: 4
                }))
            && result_register.access == ir::Access::Read
            && result_register.fieldset.as_deref() == Some("RESULT"),
        "classic ADC result span differs from qualified scan length"
    );
    let first = if sequence.maximum_length == 4 {
        "SQR"
    } else {
        "SQR0"
    };
    for (name, offset) in if sequence.maximum_length == 4 {
        vec![("SQR", 0x0c)]
    } else {
        vec![("SQR0", 0x50), ("SQR1", 0x54)]
    } {
        let item = block
            .items
            .iter()
            .find(|i| i.name == name)
            .context("missing classic ADC sequence register")?;
        ensure!(
            item.byte_offset == offset && item.array.is_none(),
            "classic ADC sequence register offset mismatch"
        );
        let ir::BlockItemInner::Register(register) = &item.inner else {
            anyhow::bail!("ADC sequence must be a register")
        };
        ensure!(
            register.access == ir::Access::ReadWrite && register.fieldset.as_deref() == Some(name),
            "classic ADC sequence access mismatch"
        );
        let fs = &ir.fieldsets[name];
        let slots = fs
            .fields
            .iter()
            .find(|f| f.name == "SQR")
            .context("missing typed ADC slot fields")?;
        ensure!(
            slots.bit_offset == ir::BitOffset::Regular(0)
                && slots.bit_size == 4
                && slots.array
                    == Some(ir::Array::Regular(ir::RegularArray {
                        len: u32::from(facts.slots_per_sequence_register),
                        stride: 4
                    })),
            "classic ADC slot mapping differs"
        );
        if name == first {
            let count = fs
                .fields
                .iter()
                .find(|f| f.name == "ENS")
                .context("missing ADC scan length")?;
            ensure!(
                count.bit_offset == ir::BitOffset::Regular(16)
                    && count.bit_size == sequence.maximum_length.ilog2()
                    && count.array.is_none(),
                "classic ADC scan length differs"
            );
        } else {
            ensure!(
                fs.fields.len() == 1,
                "SQR1 must not expose first-register count or reserved bits"
            );
        }
    }
    for (register, field, offset, width) in [
        ("CR0", "MODE", 1, 3),
        ("CR0", "SAM", 11, 2),
        ("CR0", "BUF", 13, 1),
        ("CR1", "CHMUX", 0, 4),
        ("ISR", "EOC", 0, 1),
        ("ISR", "EOS", 1, 1),
        ("START", "START", 0, 1),
    ] {
        ensure!(
            ir.fieldsets[register].fields.iter().any(|f| f.name == field
                && f.bit_offset == ir::BitOffset::Regular(offset)
                && f.bit_size == width
                && f.array.is_none()),
            "classic ADC protocol field mismatch: {register}.{field}"
        );
    }
    let mode = ir.fieldsets["CR0"]
        .fields
        .iter()
        .find(|f| f.name == "MODE")
        .unwrap();
    ensure!(
        mode.enumm.as_deref() == Some("Mode"),
        "classic ADC mode must be typed"
    );
    let mode = &ir.enums["Mode"];
    ensure!(
        mode.bit_size == 3
            && mode.variants.len() == 2
            && mode
                .variants
                .iter()
                .any(|v| v.name == "SINGLE"
                    && own["single_mode"].as_u64() == Some(u64::from(v.value)))
            && mode
                .variants
                .iter()
                .any(|v| v.name == "SCAN" && own["scan_mode"].as_u64() == Some(u64::from(v.value))),
        "unqualified classic ADC conversion modes"
    );
    let cr0 = &ir.fieldsets["CR0"];
    ensure!(
        cr0.fields.iter().any(|f| f.name == "TSEN") == facts.temperature_channel.is_some(),
        "reserved or absent temperature gate"
    );
    ensure!(
        cr0.fields.iter().any(|f| f.name == "BGREN")
            == (sequence.maximum_length == 4 && facts.bandgap_channel.is_some()),
        "reserved or absent bandgap gate"
    );
    let limits = adc
        .adc_limits
        .as_mut()
        .context("missing classic ADC electrical facts")?;
    ensure!(
        limits.sample_cycles == vec![5, 6, 8, 10] && limits.comparison_cycles == 19,
        "classic ADC common timing facts differ"
    );
    ensure!(
        limits.classic_scan.is_none(),
        "duplicate classic ADC scan projection"
    );
    ensure!(
        limits.sequence.is_none(),
        "duplicate ADC sequence projection"
    );
    limits.sequence = Some(sequence.clone());
    limits.classic_scan = Some(facts.clone());
    Ok(())
}
