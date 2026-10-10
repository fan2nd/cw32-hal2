//! Own-source factory-LSI SYSCLK qualification and complete direct-consumer facts.
use anyhow::{Context, Result, ensure};
use chiptool::ir;
use cw32_data_serde::chip::Core;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::Path};

fn parse_policy(bytes: &[u8]) -> Result<Value> {
    // serde_yaml::Value rejects duplicate mapping keys recursively before
    // conversion to JSON Value, including arbitrary family/field mappings.
    let value: serde_yaml::Value = serde_yaml::from_slice(bytes)?;
    Ok(serde_json::to_value(value)?)
}

pub fn validate(
    root: &Path,
    line: &str,
    core: &Core,
    registers: &BTreeMap<String, ir::IR>,
) -> Result<()> {
    let qualified = matches!(line, "CW32F020" | "CW32F030" | "CW32A030");
    let Some(clock) = core
        .peripherals
        .iter()
        .find_map(|p| p.clock_limits.as_ref())
    else {
        // A source-import fixture without any clock metadata has no source
        // capability. The three qualified real families must never take this exit.
        ensure!(
            !qualified,
            "qualified factory-LSI family lacks clock metadata"
        );
        return Ok(());
    };
    let path = "cw32-data/lsi-sysclk-qualified.yaml";
    let bytes = fs::read(root.join(path))?;
    let electrical = parse_policy(&fs::read(root.join("cw32-data/electrical.yaml"))?)?;
    ensure!(
        electrical["policies"][path] == format!("{:x}", Sha256::digest(&bytes)),
        "factory-LSI source policy changed"
    );
    let policy = parse_policy(&bytes)?;
    ensure!(
        policy["schema_version"] == 1
            && policy["source_authority"] == "sources/evidence-sources.json",
        "unknown factory-LSI authority"
    );
    let families = policy["families"]
        .as_object()
        .context("missing LSI family roster")?;
    ensure!(
        families.keys().map(String::as_str).collect::<Vec<_>>()
            == ["CW32A030", "CW32F020", "CW32F030"],
        "factory-LSI family scope changed"
    );
    let own = &policy["families"][line];
    ensure!(
        clock.lsi_sysclk.is_some() == qualified,
        "qualified family requires factory-LSI facts; other families must omit them"
    );
    ensure!(
        own["lsi_sysclk"].is_object() == qualified,
        "qualified family policy requires nonnull factory-LSI facts"
    );
    ensure!(
        serde_json::to_value(&clock.lsi_sysclk)? == own["lsi_sysclk"],
        "factory-LSI presence or facts differ from own source"
    );
    let Some(lsi) = &clock.lsi_sysclk else {
        return Ok(());
    };
    ensure!(
        own["watchdog_source"] == "independent_rc10k"
            && own["gpio_inspection"] == "whole_bank_functional_handover"
            && own["factory_read_width_bits"] == 16
            && own["trim_write"] == "stopped_field_only"
            && own["waitcycle_preserved"] == true
            && own["cycle_timing_qualified"] == false,
        "factory-LSI operating contract changed"
    );
    let lock: Value =
        serde_json::from_slice(&fs::read(root.join("sources/evidence-sources.json"))?)?;
    let sources = own["sources"]
        .as_array()
        .context("missing LSI own sources")?;
    let expected = match line {
        "CW32F020" => [
            "vendor:CW32F020_UserManual_CN_V1.4.pdf",
            "vendor:current-datasheets/CW32F020_DataSheet_CN_V1.3.pdf",
        ],
        "CW32F030" => [
            "vendor:CW32x030_UserManual_CN_V2.5.pdf",
            "vendor:CW32F030_DataSheet_CN_V1.9.pdf",
        ],
        "CW32A030" => [
            "vendor:CW32x030_UserManual_CN_V2.5.pdf",
            "vendor:CW32A030_DataSheet_CN_V1.1.pdf",
        ],
        _ => anyhow::bail!("unqualified factory-LSI family"),
    };
    ensure!(
        sources.len() == expected.len(),
        "incomplete LSI source roster"
    );
    for (source, id) in sources.iter().zip(expected) {
        ensure!(source["source_ref"] == id, "wrong own LSI source selection");
        let original = lock["artifacts"]
            .as_array()
            .context("missing source lock")?
            .iter()
            .find(|a| a["id"] == id)
            .context("LSI source is not canonical")?;
        ensure!(
            source["sha256"] == original["sha256"]
                && original["provenance"]["status"] == "selected"
                && original["provenance"]["chip_scope"]
                    .as_array()
                    .context("missing scope")?
                    .iter()
                    .any(|s| s == line),
            "LSI source is not selected own-family evidence"
        );
        let pages = source["pdf_pages_1_based"]
            .as_array()
            .context("missing pages")?;
        let printed = source["printed_pages"]
            .as_array()
            .context("missing printed pages")?;
        let count = original["provenance"]["pdf_page_count"]
            .as_u64()
            .context("missing page count")?;
        ensure!(
            !pages.is_empty()
                && pages.len() == printed.len()
                && pages.iter().zip(printed).all(|(p, q)| p
                    .as_u64()
                    .is_some_and(|p| p > 0 && p <= count && q.as_u64() == Some(p - 1))),
            "invalid LSI evidence pages"
        );
    }
    let peripheral = |name: &str| {
        core.peripherals
            .iter()
            .find(|p| p.name == name)
            .with_context(|| format!("missing LSI consumer {name}"))
    };
    let register = |name: &str, reg: &str, offset: u32| -> Result<&ir::FieldSet> {
        let p = peripheral(name)?
            .registers
            .as_ref()
            .context("missing LSI registers")?;
        let ir = registers.get(&p.kind).context("missing LSI register IR")?;
        let block = ir.blocks.get(&p.block).context("missing LSI block")?;
        let item = block
            .items
            .iter()
            .find(|r| r.name == reg && r.byte_offset == offset)
            .with_context(|| format!("wrong LSI register {name}.{reg}"))?;
        ensure!(item.array.is_none(), "unexpected LSI register array");
        let ir::BlockItemInner::Register(reg) = &item.inner else {
            anyhow::bail!("LSI object is not a register")
        };
        ensure!(
            reg.bit_size == 32 && matches!(reg.access, ir::Access::Read | ir::Access::ReadWrite),
            "LSI register is not readable u32"
        );
        ir.fieldsets
            .get(reg.fieldset.as_ref().context("missing LSI fields")?)
            .context("missing LSI fieldset")
    };
    let field = |fields: &ir::FieldSet, name: &str, bit: u32, size: u32| -> Result<()> {
        ensure!(
            fields.fields.iter().any(|f| f.name == name
                && f.bit_offset == ir::BitOffset::Regular(bit)
                && f.bit_size == size
                && f.array.is_none()),
            "LSI field mismatch: {name}"
        );
        Ok(())
    };
    let text = |v: &Value| v.as_str().map(str::to_owned).context("missing LSI text");
    let number = |v: &Value| {
        v.as_u64()
            .and_then(|x| u32::try_from(x).ok())
            .context("missing LSI number")
    };
    let addresses = own["peripheral_addresses"]
        .as_object()
        .context("missing LSI peripheral addresses")?;
    let expected_names: Vec<_> = ["SYSCTRL", "RTC", "AWT"]
        .into_iter()
        .chain(lsi.uarts.iter().map(String::as_str))
        .chain(lsi.gpio_banks.iter().map(String::as_str))
        .collect();
    ensure!(
        addresses.len() == expected_names.len(),
        "incomplete LSI address roster"
    );
    for name in expected_names {
        ensure!(
            addresses[name].as_u64() == Some(u64::from(peripheral(name)?.address)),
            "LSI peripheral address mismatch: {name}"
        );
    }
    // Require complete unique identities before checking their source-bound IR
    // positions. Merely iterating supplied entries would accept omissions.
    let mut required_fields = vec![
        ("SYSCTRL", "CR0", "SYSCLK"),
        ("SYSCTRL", "CR0", "KEY"),
        ("SYSCTRL", "CR1", "HSIEN"),
        ("SYSCTRL", "CR1", "HSEEN"),
        ("SYSCTRL", "CR1", "PLLEN"),
        ("SYSCTRL", "CR1", "LSIEN"),
        ("SYSCTRL", "CR1", "LSEEN"),
        ("SYSCTRL", "CR1", "LSECCS"),
        ("SYSCTRL", "CR1", "HSECCS"),
        ("SYSCTRL", "CR1", "CLKCCS"),
        ("SYSCTRL", "CR1", "KEY"),
        ("SYSCTRL", "IER", "LSIRDY"),
        ("SYSCTRL", "ISR", "LSIRDY"),
        ("SYSCTRL", "ISR", "HSESTABLE"),
        ("SYSCTRL", "ISR", "LSISTABLE"),
        ("SYSCTRL", "ISR", "LSESTABLE"),
        ("SYSCTRL", "LSI", "TRIM"),
        ("SYSCTRL", "LSI", "WAITCYCLE"),
        ("SYSCTRL", "LSI", "STABLE"),
        ("SYSCTRL", "HSE", "STABLE"),
        ("SYSCTRL", "LSE", "STABLE"),
        ("SYSCTRL", "MCO", "SOURCE"),
        ("RTC", "CR1", "SOURCE"),
        ("AWT", "CR", "SRC"),
        ("GPIOB", "AFRH", "AFR11"),
    ];
    required_fields.extend(
        lsi.uarts
            .iter()
            .map(|name| (name.as_str(), "CR2", "SOURCE")),
    );
    required_fields.extend(
        lsi.gpio_banks
            .iter()
            .map(|name| (name.as_str(), "FILTER", "FLTCLK")),
    );
    let fields = own["register_fields"]
        .as_array()
        .context("missing LSI field roster")?;
    let mut actual_fields = fields
        .iter()
        .map(|f| {
            (
                f["peripheral"].as_str().unwrap_or(""),
                f["register"].as_str().unwrap_or(""),
                f["field"].as_str().unwrap_or(""),
            )
        })
        .collect::<Vec<_>>();
    required_fields.sort();
    actual_fields.sort();
    ensure!(
        actual_fields == required_fields && actual_fields.len() == 32,
        "LSI field roster must be exact, complete and unique"
    );
    let mut required_gates = ["RTC", "AWT"]
        .into_iter()
        .chain(lsi.uarts.iter().map(String::as_str))
        .chain(lsi.gpio_banks.iter().map(String::as_str))
        .collect::<Vec<_>>();
    let gates = own["consumer_gates"]
        .as_array()
        .context("missing LSI gate roster")?;
    let mut actual_gates = gates
        .iter()
        .map(|g| g["peripheral"].as_str().unwrap_or(""))
        .collect::<Vec<_>>();
    required_gates.sort();
    actual_gates.sort();
    ensure!(
        actual_gates == required_gates && actual_gates.len() == 9,
        "LSI gate roster must be exact, complete and unique"
    );
    for fact in own["register_fields"]
        .as_array()
        .context("missing LSI fields")?
    {
        let fields = register(
            &text(&fact["peripheral"])?,
            &text(&fact["register"])?,
            number(&fact["byte_offset"])?,
        )?;
        field(
            fields,
            &text(&fact["field"])?,
            number(&fact["bit_offset"])?,
            number(&fact["bit_size"])?,
        )?;
    }
    for fact in own["consumer_gates"]
        .as_array()
        .context("missing LSI gates")?
    {
        let name = text(&fact["peripheral"])?;
        let gate = peripheral(&name)?
            .rcc_control
            .as_ref()
            .context("missing qualified LSI consumer gate")?;
        let reset = gate.reset.as_ref().context("missing LSI consumer reset")?;
        ensure!(
            gate.controller == "SYSCTRL"
                && gate.enable.register == text(&fact["register"])?
                && gate.enable.field == name
                && reset.register == text(&fact["reset_register"])?
                && reset.field == name
                && gate.enable_active_value
                && gate.reset_asserted_value == Some(false)
                && gate.enable_write_key.is_none()
                && fact["keyed"] == false
                && fact["enable_active_value"] == true
                && fact["reset_asserted_value"] == false
                && fact["gate_controls_work"] == name.starts_with("GPIO"),
            "LSI gate protocol differs from own source"
        );
        let en = register(
            "SYSCTRL",
            &gate.enable.register,
            number(&fact["byte_offset"])?,
        )?;
        let rst = register(
            "SYSCTRL",
            &reset.register,
            number(&fact["reset_byte_offset"])?,
        )?;
        field(en, &name, number(&fact["bit_offset"])?, 1)?;
        field(rst, &name, number(&fact["bit_offset"])?, 1)?;
        ensure!(
            !en.fields.iter().any(|f| f.name == "KEY"),
            "classic LSI gate must be unkeyed"
        );
    }
    for (kind, expected) in [("uart", &lsi.uarts), ("gpio", &lsi.gpio_banks)] {
        let actual: Vec<_> = core
            .peripherals
            .iter()
            .filter(|p| {
                p.registers.as_ref().is_some_and(|r| {
                    if kind == "gpio" {
                        matches!(r.kind.as_ref(), "gpio" | "gpioc" | "gpiof")
                    } else {
                        r.kind == kind
                    }
                })
            })
            .map(|p| p.name.as_str())
            .collect();
        ensure!(
            actual == expected.iter().map(String::as_str).collect::<Vec<_>>(),
            "incomplete LSI {kind} roster"
        );
    }
    let rtc = peripheral("RTC")?
        .rtc_calendar
        .as_ref()
        .context("missing factory-LSI RTC facts")?;
    ensure!(
        lsi.nominal_hz == rtc.nominal_hz
            && lsi.minimum_hz == rtc.minimum_hz
            && lsi.maximum_hz == rtc.maximum_hz
            && lsi.supply_mv == rtc.supply_mv
            && lsi.temperature_c == rtc.temperature_c
            && lsi.factory_trim_address == rtc.factory_trim_address,
        "same-source LSI/RTC qualification diverges"
    );
    ensure!(
        core.interrupts
            .iter()
            .any(|i| i.name == "RCC" && u16::from(i.number) == lsi.rcc_irq),
        "RCC pending observer IRQ changed"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_lsi_policy_keys_are_rejected_at_every_depth() {
        for input in [
            "families: {}\nfamilies: {}\n",
            "families:\n  CW32F020: {}\n  CW32F020: {}\n",
            "families:\n  CW32F020:\n    lsi_sysclk: {nominal_hz: 32800, nominal_hz: 1}\n",
        ] {
            assert!(
                parse_policy(input.as_bytes())
                    .unwrap_err()
                    .to_string()
                    .contains("duplicate")
            );
        }
    }
}
