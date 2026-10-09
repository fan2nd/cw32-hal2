//! Own-family LVD facts and the documented IR register subfunction.
//! No fabricated register block or implicit timer/UART clock ownership.
use anyhow::{Context, Result, ensure};
use chiptool::ir;
use cw32_data_serde::chip::{
    self,
    core::peripheral::{Ir, Lvd},
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
    lvd: Lvd,
    ir: Ir,
    ir_controller: String,
    ir_routes: Vec<Route>,
}
#[derive(Deserialize)]
struct Route {
    pin: String,
    af: u8,
    gpio_register: String,
    gpio_field: String,
    source_macro: String,
    source_line: u32,
    datasheet_cell: serde_json::Value,
}

pub fn apply(
    root: &Path,
    line: &str,
    core: &mut chip::Core,
    registers: &BTreeMap<String, ir::IR>,
) -> Result<()> {
    let c: Catalog = crate::read_yaml(root.join("cw32-data/lvd-ir.yaml"))?;
    ensure!(c.schema_version == 1, "unknown LVD/IR schema");
    let p = c
        .profiles
        .get(line)
        .context("missing own-family LVD/IR facts")?;
    let e: serde_json::Value = serde_json::from_slice(&fs::read(root.join(c.evidence))?)?;
    let e = &e["families"][line];
    ensure!(
        serde_json::to_value(&p.lvd)? == e["lvd"] && serde_json::to_value(&p.ir)? == e["ir"],
        "LVD/IR facts differ from own-source audit"
    );
    for source in ["manual", "datasheet"] {
        let s = &e["sources"][source];
        ensure!(
            s["sha256"].as_str().is_some_and(|s| s.len() == 64)
                && s["url"].as_str().is_some_and(|s| s.starts_with("https://")),
            "LVD/IR source citation missing"
        );
    }
    let lvd = core
        .peripherals
        .iter_mut()
        .find(|p| p.name == "LVD")
        .context("missing LVD")?;
    let r = lvd.registers.as_ref().context("missing LVD registers")?;
    let low = r.version == "cw32l010_v1";
    let ir = &registers[&r.kind];
    let f = &ir.fieldsets["CR0"].fields;
    let source = f.iter().find(|f| f.name == "SOURCE").unwrap();
    let threshold = f.iter().find(|f| f.name == "VTH").unwrap();
    ensure!(
        source.bit_offset == ir::BitOffset::Regular(2)
            && source.bit_size == if low { 1 } else { 2 },
        "LVD SOURCE layout changed"
    );
    ensure!(
        threshold.bit_offset == ir::BitOffset::Regular(4)
            && threshold.bit_size == if low { 3 } else { 4 },
        "LVD VTH layout changed"
    );
    ensure!(
        p.lvd.thresholds_mv.len() == (1 << threshold.bit_size),
        "incomplete LVD threshold domain"
    );
    ensure!(
        lvd.rcc_control.is_none(),
        "LVD unexpectedly acquired a controller gate; review required"
    );
    let mut facts = p.lvd.clone();
    for input in &facts.inputs {
        ensure!(
            input.selector > 0 && u32::from(input.selector) < (1 << source.bit_size),
            "invalid LVD input selector"
        );
    }
    facts
        .inputs
        .retain(|input| core.pins.iter().any(|pin| pin.name == input.pin));
    lvd.lvd = Some(facts);

    // SYSCTRL owns IRMOD on classic families. Existing F003 IR is the same
    // address view and supplies the HAL token, never a second generated owner.
    let index = core
        .peripherals
        .iter()
        .position(|r| r.name == p.ir_controller)
        .context("missing IR register controller")?;
    let controller = &core.peripherals[index];
    let r = controller
        .registers
        .as_ref()
        .context("missing IR register block")?;
    let ir = &registers[&r.kind];
    let item = ir.blocks[&r.block]
        .items
        .iter()
        .find(|r| r.name == p.ir.register)
        .context("missing real IR register")?;
    let expected_address = if p.ir_controller == "SYSCTRL" {
        0x4001_0074
    } else {
        0x4000_4080
    };
    ensure!(
        controller.address + item.byte_offset == expected_address,
        "IR subfunction address changed"
    );
    let ir::BlockItemInner::Register(reg) = &item.inner else {
        anyhow::bail!("IR control is not a register")
    };
    ensure!(
        reg.access == ir::Access::ReadWrite,
        "IR control must be readable and writable"
    );
    let f = &ir.fieldsets[reg.fieldset.as_ref().unwrap()].fields;
    ensure!(
        f.iter().any(|f| f.name == "MOD"
            && f.bit_offset == ir::BitOffset::Regular(0)
            && f.bit_size == 4),
        "IR selector layout changed"
    );
    ensure!(
        f.iter().any(|f| f.name == "IRSW") == p.ir.software_control
            && f.iter().any(|f| f.name == "INV") == p.ir.invert,
        "IR control capabilities changed"
    );
    if p.ir.mode_configurable {
        ensure!(
            f.iter().find(|f| f.name == "MOD").unwrap().enumm.as_deref() == Some("IrMode"),
            "IR needs its typed destination-specific selector"
        );
    }
    if let Some(owner) = core.peripherals.iter().find(|r| r.name == p.ir.owner) {
        ensure!(
            owner.address == expected_address,
            "IR ownership token aliases a different function"
        );
    } else {
        ensure!(
            p.ir.owner == "IR" && p.ir_controller == "SYSCTRL",
            "only the documented SYSCTRL IR subfunction may add a HAL singleton"
        );
    }
    let mut pins = vec![];
    for route in &p.ir_routes {
        ensure!(
            !route.source_macro.is_empty()
                && route.source_line > 0
                && route.datasheet_cell["pin"] == route.pin
                && route.datasheet_cell["af"] == route.af
                && route.datasheet_cell["function"] == "IR_OUT",
            "unverified IR pin route"
        );
        let n: u32 = route.pin[2..].parse()?;
        let gpio = core
            .peripherals
            .iter()
            .find(|r| r.name == format!("GPIO{}", &route.pin[1..2]))
            .context("missing IR GPIO")?;
        let r = gpio.registers.as_ref().unwrap();
        let ir = &registers[&r.kind];
        let field = ir.fieldsets[&route.gpio_register]
            .fields
            .iter()
            .find(|f| f.name == route.gpio_field)
            .context("missing IR AF field")?;
        ensure!(
            field.bit_offset == ir::BitOffset::Regular((n % 8) * 4)
                && u32::from(route.af) < (1 << field.bit_size),
            "IR AF layout/value changed"
        );
        if core.pins.iter().any(|p| p.name == route.pin) {
            pins.push(chip::core::peripheral::Pin {
                pin: route.pin.clone(),
                signal: "IR_OUT".into(),
                af: Some(route.af),
                adc_mux: None,
                comparator_mux: None,
            });
        }
    }
    core.peripherals[index].ir = Some(p.ir.clone());
    core.peripherals[index].pins.extend(pins);
    core.peripherals[index]
        .pins
        .sort_by(|a, b| (&a.pin, &a.signal, a.af).cmp(&(&b.pin, &b.signal, b.af)));
    Ok(())
}
