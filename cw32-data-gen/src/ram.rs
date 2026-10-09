//! Validate RAM parity facts against each family's normalized controller and IRQ.
use anyhow::{Context, Result, ensure};
use chiptool::ir;
use cw32_data_serde::chip;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::Path};
#[derive(Deserialize)]
struct Catalog {
    schema_version: u32,
    evidence: String,
    common: Common,
    profiles: BTreeMap<String, Profile>,
}
#[derive(Deserialize, Serialize)]
struct Common {
    always_enabled: bool,
    software_parity_enable: bool,
    software_parity_disable: bool,
    initialization_command: bool,
    injection_command: bool,
    reset_command: bool,
    independent_clock_gate: bool,
    register_offsets: BTreeMap<String, u32>,
    status_bit: u8,
    clear_bit: u8,
    clear_value: u8,
    clear_noop: u8,
    address_reset: u32,
    status_reset: u32,
    parity_bits_per_data_byte: u8,
}
#[derive(Deserialize, Serialize)]
struct Profile {
    register_version: String,
    base_address: u32,
    ier_parity_bit: u32,
    enable_status: bool,
    address_bits: u32,
    interrupt: String,
    interrupt_number: u8,
}
pub(crate) fn apply(
    root: &Path,
    line: &str,
    core: &mut chip::Core,
    registers: &BTreeMap<String, ir::IR>,
) -> Result<()> {
    let data: Catalog = crate::read_yaml(root.join("cw32-data/ram-parity.yaml"))?;
    ensure!(data.schema_version == 1, "unsupported RAM facts schema");
    let p = data
        .profiles
        .get(line)
        .context("missing own-family RAM profile")?;
    let evidence: serde_json::Value = serde_json::from_slice(&fs::read(root.join(data.evidence))?)?;
    let own = &evidence["families"][line];
    ensure!(
        serde_json::to_value(&data.common)? == evidence["common"]
            && serde_json::to_value(p)? == own["profile"],
        "RAM diagnostics differ from own-source review"
    );
    for kind in ["manual", "datasheet", "sdk"] {
        ensure!(
            own["sources"][kind]["sha256"]
                .as_str()
                .is_some_and(|s| s.len() == 64),
            "RAM source pin missing"
        );
    }
    let ram = core
        .peripherals
        .iter()
        .find(|x| x.name == "RAM")
        .context("RAM peripheral missing")?;
    let r = ram.registers.as_ref().context("RAM registers missing")?;
    ensure!(
        ram.address == p.base_address && r.kind == "ram" && r.version == p.register_version,
        "RAM profile disagrees with normalized map"
    );
    ensure!(
        ram.rcc.is_none() && ram.rcc_control.is_none(),
        "RAM has no independent documented RCC gate or reset"
    );
    ensure!(
        ram.interrupts.len() == 1
            && ram.interrupts[0].interrupt == p.interrupt
            && ram.interrupts[0].signal == "GLOBAL",
        "RAM shared IRQ association missing"
    );
    ensure!(
        core.interrupts
            .iter()
            .any(|i| i.name == p.interrupt && i.number == p.interrupt_number),
        "RAM IRQ vector mismatch"
    );
    let ir = &registers["ram"];
    for (name, offset, access) in [
        ("IER", 0, ir::Access::ReadWrite),
        ("ADDR", 4, ir::Access::Read),
        ("ISR", 8, ir::Access::Read),
        ("ICR", 12, ir::Access::ReadWrite),
    ] {
        let item = ir.blocks["RAM"]
            .items
            .iter()
            .find(|i| i.name == name)
            .context("missing RAM register")?;
        let ir::BlockItemInner::Register(r) = &item.inner else {
            anyhow::bail!("RAM item not register")
        };
        ensure!(
            item.byte_offset == offset
                && r.bit_size == 32
                && r.access == access
                && r.fieldset.as_deref() == Some(name),
            "RAM register layout/access mismatch"
        );
    }
    for (reg, name, bit, width) in [
        ("IER", "PARITY", p.ier_parity_bit, 1),
        ("ISR", "PARITY", 0, 1),
        ("ICR", "PARITY", 0, 1),
        ("ADDR", "ADDR", 0, p.address_bits),
    ] {
        let f = ir.fieldsets[reg]
            .fields
            .iter()
            .find(|f| f.name == name)
            .context("missing RAM field")?;
        ensure!(
            f.bit_offset == ir::BitOffset::Regular(bit) && f.bit_size == width,
            "RAM field layout mismatch"
        );
    }
    ensure!(
        ir.fieldsets["IER"].fields.iter().any(|f| f.name == "EN") == p.enable_status,
        "RAM EN status availability mismatch"
    );
    core.peripherals
        .iter_mut()
        .find(|x| x.name == "RAM")
        .unwrap()
        .ram_parity = Some(chip::core::peripheral::RamParity {
        enable_status: p.enable_status,
    });
    Ok(())
}
