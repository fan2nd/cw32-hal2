//! Own-L083 AES/TRNG geometry and capability qualification before projection.
use anyhow::{Context, Result, ensure};
use chiptool::ir;
use cw32_data_serde::chip::{
    self,
    core::peripheral::{Aes, Trng},
};
use serde::Deserialize;
use std::{collections::BTreeMap, fs, path::Path};

#[derive(Deserialize, PartialEq)]
struct Catalog {
    schema_version: u32,
    family: String,
    source_ref: String,
    evidence: String,
    aes: AesProfile,
    trng: TrngProfile,
}
#[derive(Deserialize, PartialEq)]
struct Common {
    register_version: String,
    word_bits: u8,
    word_order: String,
    irq: bool,
    dma: bool,
    start_abort_documented: bool,
}
#[derive(Deserialize, PartialEq)]
struct AesProfile {
    #[serde(flatten)]
    geometry: Aes,
    #[serde(flatten)]
    common: Common,
    mode: String,
}
#[derive(Deserialize, PartialEq)]
struct TrngProfile {
    #[serde(flatten)]
    geometry: Trng,
    #[serde(flatten)]
    common: Common,
    health_status_documented: bool,
    analog_ready_documented: bool,
}
#[derive(Deserialize)]
struct Evidence {
    facts: Catalog,
}

pub(crate) fn apply(
    root: &Path,
    line: &str,
    core: &mut chip::Core,
    registers: &BTreeMap<String, ir::IR>,
) -> Result<()> {
    if !core
        .peripherals
        .iter()
        .any(|p| p.name == "AES" || p.name == "TRNG")
    {
        return Ok(());
    }
    let data: Catalog = crate::read_yaml(root.join("cw32-data/crypto.yaml"))?;
    ensure!(
        data.schema_version == 1 && data.family == line,
        "unqualified crypto family/schema"
    );
    let evidence: Evidence = serde_json::from_slice(&fs::read(root.join(&data.evidence))?)?;
    ensure!(
        data == evidence.facts,
        "crypto facts differ from own-source review"
    );
    for (name, common) in [("AES", &data.aes.common), ("TRNG", &data.trng.common)] {
        let p = core
            .peripherals
            .iter_mut()
            .find(|p| p.name == name)
            .context("missing crypto peripheral")?;
        let regs = p.registers.as_ref().context("missing crypto registers")?;
        ensure!(
            regs.version == common.register_version
                && common.word_bits == 32
                && common.word_order == "least-significant-register-first"
                && !common.irq
                && !common.dma
                && !common.start_abort_documented
                && p.interrupts.is_empty()
                && p.dma_channels.is_empty(),
            "unreviewed crypto interface"
        );
        let control = p.rcc_control.as_ref().context("missing crypto RCC")?;
        ensure!(
            control.shared_enable_group.is_none()
                && control.shared_reset_group.is_none()
                && control.bus_clock == "HCLK"
                && control.reset_asserted_value == Some(false),
            "unreviewed crypto clock/reset"
        );
        let ir = &registers[&regs.kind];
        let groups: &[(&str, u8, u32, ir::Access)] = if name == "AES" {
            ensure!(
                data.aes.geometry
                    == Aes {
                        block_words: 4,
                        key_words_128: 4,
                        key_words_192: 6,
                        key_words_256: 8
                    }
                    && data.aes.mode == "ECB",
                "unreviewed AES geometry/mode"
            );
            p.aes = Some(data.aes.geometry.clone());
            &[
                ("DATA", 4, 16, ir::Access::ReadWrite),
                ("KEY", 8, 32, ir::Access::Write),
            ]
        } else {
            ensure!(
                data.trng.geometry.output_words == 2
                    && !data.trng.health_status_documented
                    && !data.trng.analog_ready_documented,
                "unreviewed TRNG geometry/status"
            );
            p.trng = Some(data.trng.geometry.clone());
            &[("DATA", 2, 12, ir::Access::Read)]
        };
        for (prefix, count, offset, access) in groups {
            for index in 0..*count {
                let item = ir.blocks[&regs.block]
                    .items
                    .iter()
                    .find(|r| r.name == format!("{prefix}{index}"))
                    .context("missing crypto word")?;
                let ir::BlockItemInner::Register(register) = &item.inner else {
                    anyhow::bail!("crypto word is not register");
                };
                ensure!(
                    item.array.is_none()
                        && item.byte_offset == offset + u32::from(index) * 4
                        && register.bit_size == 32
                        && register.access == *access,
                    "crypto register geometry differs"
                );
            }
        }
    }
    Ok(())
}
