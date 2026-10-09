//! Own-source qualification for programmable, per-slot ADC sequences.
use anyhow::{Context, Result, ensure};
use chiptool::ir;
use cw32_data_serde::chip::{self, core::peripheral::AdcSequence};
use serde::Deserialize;
use serde_json::Value;
use std::{collections::BTreeMap, fs, path::Path};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Catalog {
    schema_version: u32,
    evidence: String,
    profiles: BTreeMap<String, Profile>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Profile {
    register_version: String,
    facts: AdcSequence,
}

pub(crate) fn apply(
    root: &Path,
    path: &str,
    line: &str,
    core: &mut chip::Core,
    registers: &BTreeMap<String, ir::IR>,
) -> Result<()> {
    let catalog: Catalog = crate::read_yaml(root.join(path))?;
    ensure!(catalog.schema_version == 1, "unknown ADC sequence catalog");
    let profile = catalog
        .profiles
        .get(line)
        .context("missing own-family ADC sequence profile")?;
    let proof: Value = serde_json::from_slice(&fs::read(root.join(catalog.evidence))?)?;
    let own = &proof["families"][line];
    ensure!(
        own["register_version"] == profile.register_version
            && own["facts"] == serde_json::to_value(&profile.facts)?,
        "ADC sequence facts differ from own-source review"
    );
    let lock: Value =
        serde_json::from_slice(&fs::read(root.join("sources/evidence-sources.json"))?)?;
    ensure!(
        lock["artifacts"]
            .as_array()
            .context("missing source lock")?
            .iter()
            .any(|source| own["family"] == line
                && source["kind"] == "pdf"
                && source["provenance"]["status"] == "selected"
                && source["provenance"]["chip_scope"]
                    .as_array()
                    .is_some_and(|scope| scope.iter().any(|family| family == line))
                && source["path"] == own["manual"]["path"]
                && source["sha256"] == own["manual"]["sha256"]
                && source["url"] == own["manual"]["url"]),
        "ADC sequence manual is outside the own-family source lock"
    );
    let adcs: Vec<_> = core
        .peripherals
        .iter_mut()
        .filter(|p| p.registers.as_ref().is_some_and(|r| r.kind == "adc"))
        .collect();
    ensure!(!adcs.is_empty(), "missing sequence ADC");
    for adc in &adcs {
        let selected = adc.registers.as_ref().context("missing ADC registers")?;
        ensure!(
            selected.version == profile.register_version,
            "ADC sequence version mismatch"
        );
    }
    // Layout comes from the selected family's authored source review, never
    // from a family-name switch or another peripheral's compatible shape.
    let shape = &own["register_shape"];
    let fact = |section: &str, key: &str| -> Result<u32> {
        u32::try_from(
            shape[section][key]
                .as_u64()
                .with_context(|| format!("missing ADC sequence {section}.{key}"))?,
        )
        .context("ADC sequence fact exceeds u32")
    };
    let facts = &profile.facts;
    ensure!(
        facts.maximum_length == 8
            && facts.programmable_order
            && facts.per_slot_sample_time
            && facts.per_slot_result,
        "unsupported ADC sequence architecture"
    );
    let ir = registers.get("adc").context("missing ADC register IR")?;
    let block = ir.blocks.get("ADC").context("missing ADC register block")?;
    for (name, offset) in [
        ("SAMPLE", fact("sample", "offset")?),
        ("SQRCFR", fact("sqrcfr", "offset")?),
        ("RESULT", fact("result", "offset")?),
    ] {
        let item = block
            .items
            .iter()
            .find(|r| r.name == name)
            .context("missing ADC sequence register")?;
        ensure!(
            item.byte_offset == offset,
            "ADC sequence register offset mismatch"
        );
        let ir::BlockItemInner::Register(register) = &item.inner else {
            anyhow::bail!("ADC sequence target is not a register")
        };
        ensure!(
            register.fieldset.as_deref() == Some(name),
            "ADC sequence fieldset mismatch"
        );
        if name == "RESULT" {
            ensure!(
                register.access == ir::Access::Read
                    && item.array
                        == Some(ir::Array::Regular(ir::RegularArray { len: 8, stride: 4 })),
                "ADC sequence result array mismatch"
            );
        } else {
            let field = &ir.fieldsets[name].fields;
            ensure!(
                field.len() == 1
                    && field[0].name == "SQRCH"
                    && field[0].bit_size == 4
                    && field[0].bit_offset == ir::BitOffset::Regular(0)
                    && field[0].array
                        == Some(ir::Array::Regular(ir::RegularArray { len: 8, stride: 4 })),
                "ADC sequence slot field array mismatch"
            );
        }
    }
    let result = &ir.fieldsets["RESULT"].fields;
    ensure!(
        result.len() == 1
            && result[0].name == "RESULT"
            && result[0].bit_offset == ir::BitOffset::Regular(0)
            && result[0].bit_size == fact("result", "field_width_bits")?,
        "ADC sequence result field mismatch"
    );
    for (name, offset) in [
        ("START", fact("start", "byte_offset")?),
        ("ICR", fact("clear", "byte_offset")?),
        ("ISR", fact("status", "byte_offset")?),
    ] {
        let item = block
            .items
            .iter()
            .find(|r| r.name == name)
            .context("missing ADC sequence status/control")?;
        ensure!(
            item.byte_offset == offset && item.array.is_none(),
            "ADC sequence status/control offset mismatch"
        );
        let ir::BlockItemInner::Register(register) = &item.inner else {
            anyhow::bail!("ADC status/control is not a register")
        };
        ensure!(
            register.fieldset.as_deref() == Some(name),
            "ADC status/control fieldset mismatch"
        );
        if name == "ISR" {
            ensure!(
                register.access == ir::Access::Read,
                "ADC status must be read-only"
            );
        }
    }
    for name in ["ISR", "ICR"] {
        for (flag, offset) in [("EOC", 0), ("EOS", 1)] {
            let field = ir.fieldsets[name]
                .fields
                .iter()
                .find(|f| f.name == flag)
                .context("missing ADC sequence completion flag")?;
            ensure!(
                field.bit_offset == ir::BitOffset::Regular(offset) && field.bit_size == 1,
                "ADC completion flag encoding mismatch"
            );
        }
    }
    let ens = ir.fieldsets["CR"]
        .fields
        .iter()
        .find(|f| f.name == "ENS")
        .context("missing ADC sequence length")?;
    ensure!(
        ens.bit_offset == ir::BitOffset::Regular(fact("ens", "bit_offset")?)
            && ens.bit_size == fact("ens", "bit_width")?,
        "ADC sequence length encoding mismatch"
    );
    for adc in adcs {
        adc.adc_limits
            .as_mut()
            .context("missing ADC electrical limits")?
            .sequence = Some(facts.clone());
    }
    Ok(())
}
