//! Calendar clocks qualified from each device's own manual and datasheet.
use anyhow::{Context, Result, ensure};
use chiptool::ir;
use cw32_data_serde::chip::{self, core::peripheral::RtcCalendar};
use serde::Deserialize;
use std::{collections::BTreeMap, fs, path::Path};
#[derive(Deserialize)]
struct Catalog {
    schema_version: u32,
    evidence: String,
    profiles: BTreeMap<String, RtcCalendar>,
}
pub fn apply(
    root: &Path,
    line: &str,
    core: &mut chip::Core,
    registers: &BTreeMap<String, ir::IR>,
) -> Result<()> {
    let Some(peripheral) = core.peripherals.iter_mut().find(|p| p.name == "RTC") else {
        return Ok(());
    };
    let catalog: Catalog = crate::read_yaml(root.join("cw32-data/rtc-calendar.yaml"))?;
    ensure!(catalog.schema_version == 1, "unknown RTC calendar catalog");
    let facts = catalog
        .profiles
        .get(line)
        .context("missing own-family RTC facts")?;
    let proof: serde_json::Value = serde_json::from_slice(&fs::read(root.join(catalog.evidence))?)?;
    ensure!(
        serde_json::to_value(facts)? == proof["families"][line]["facts"],
        "RTC facts differ from own-source review"
    );
    let ir = &registers["rtc"];
    for (name, offset) in [
        ("KEY", 0),
        ("CR0", 4),
        ("CR1", 8),
        ("DATE", 20),
        ("TIME", 24),
    ] {
        ensure!(
            ir.blocks["RTC"]
                .items
                .iter()
                .any(|r| r.name == name && r.byte_offset == offset),
            "RTC register layout mismatch"
        );
    }
    ensure!(
        facts.minimum_hz > 0
            && facts.minimum_hz <= facts.nominal_hz
            && facts.nominal_hz <= facts.maximum_hz,
        "invalid source envelope"
    );
    if facts.source == "HSIOSC" {
        ensure!(
            facts.source_encoding == 3
                && facts.prescaler_first > 0
                && facts.prescaler_first <= 256
                && facts.prescaler_second <= 1 << 20,
            "invalid HSIOSC prescaler"
        );
        ensure!(
            u64::from(facts.maximum_hz) <= u64::from(facts.prescaler_first) * 1_000_000,
            "RTC intermediate electrical ceiling"
        );
        ensure!(
            facts.nominal_hz == u32::from(facts.prescaler_first) * facts.prescaler_second * 2
                && facts.calendar_divisor == facts.nominal_hz,
            "RTC nominal divisor mismatch"
        );
        ensure!(
            ir.blocks["RTC"]
                .items
                .iter()
                .any(|r| r.name == "PSC" && r.byte_offset == 64),
            "missing RTC prescaler"
        );
    } else {
        ensure!(
            facts.source == "LSI"
                && facts.source_encoding == 2
                && facts.nominal_hz == 32800
                && facts.calendar_divisor == 32768
                && facts.prescaler_first == 0
                && facts.prescaler_second == 0,
            "unqualified classic RTC source"
        );
        ensure!(
            !ir.blocks["RTC"].items.iter().any(|r| r.name == "PSC"),
            "classic RTC cannot invent a prescaler"
        );
    }
    peripheral.rtc_calendar = Some(facts.clone());
    apply_alarms(root, line, peripheral, ir)
}

#[derive(Deserialize)]
struct AlarmCatalog {
    schema_version: u32,
    evidence: String,
    profiles: BTreeMap<String, AlarmProfile>,
}
#[derive(serde::Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AlarmProfile {
    register_version: String,
    alarm_register_access: String,
    alarm_mask_fields: Vec<String>,
    icr_preserve_seed: u32,
    irq: String,
    async_wait: bool,
    alarm_a_ignore_bit: u8,
    alarm_b_mask_polarity: String,
    alarm_b_configuration_supported: bool,
}

fn apply_alarms(
    root: &Path,
    line: &str,
    peripheral: &mut chip::core::Peripheral,
    ir: &ir::IR,
) -> Result<()> {
    use cw32_data_serde::{chip::core::peripheral::RtcAlarms, register_write::RegisterWrites};
    let catalog: AlarmCatalog = crate::read_yaml(root.join("cw32-data/rtc-alarms.yaml"))?;
    ensure!(catalog.schema_version == 1, "unknown RTC alarm catalog");
    let profile = catalog
        .profiles
        .get(line)
        .context("missing own-family RTC alarm facts")?;
    let proof: serde_json::Value = serde_json::from_slice(&fs::read(root.join(catalog.evidence))?)?;
    ensure!(
        serde_json::to_value(profile)? == proof["families"][line]["profile"],
        "RTC alarm facts differ from own-source review"
    );
    ensure!(
        peripheral.registers.as_ref().unwrap().version == profile.register_version,
        "RTC alarm register version mismatch"
    );
    let direct = match profile.alarm_register_access.as_str() {
        "direct" => true,
        "window_access" => false,
        _ => anyhow::bail!("unqualified RTC alarm register access"),
    };
    ensure!(
        profile.async_wait == direct,
        "unqualified RTC alarm async access"
    );
    ensure!(
        profile.alarm_a_ignore_bit == 1
            && !profile.alarm_b_configuration_supported
            && profile.alarm_b_mask_polarity == "unresolved_manual_sdk_conflict",
        "unqualified alarm comparator semantics"
    );
    ensure!(
        profile.irq == "RTC"
            && peripheral.interrupts.len() == 1
            && peripheral.interrupts[0].interrupt == profile.irq
            && peripheral.interrupts[0].signal == "GLOBAL",
        "RTC alarm interrupt route mismatch"
    );
    ensure!(
        profile.alarm_mask_fields.len() == 4,
        "RTC alarm mask shape mismatch"
    );
    for register in ["ALARMA", "ALARMB"] {
        for (name, (offset, width)) in
            profile
                .alarm_mask_fields
                .iter()
                .zip([(7, 1), (15, 1), (23, 1), (24, 7)])
        {
            ensure!(
                ir.fieldsets[register].fields.iter().any(|f| f.name == *name
                    && f.bit_offset == ir::BitOffset::Regular(offset)
                    && f.bit_size == width),
                "RTC alarm field mismatch: {register}.{name}"
            );
        }
    }
    let writes: RegisterWrites = crate::read_yaml(root.join("cw32-data/register-writes.yaml"))?;
    let key = format!("rtc_{}", profile.register_version);
    let command = writes
        .registers
        .get(&key)
        .and_then(|v| v.iter().find(|r| r.register == "ICR"))
        .context("missing RTC ICR command policy")?;
    ensure!(
        u64::from(profile.icr_preserve_seed) == command.write_noop
            && command.write_noop == command.reset_value
            && command.write_noop == 0x7f,
        "RTC ICR command policy differs from own-source review"
    );
    peripheral.rtc_alarms = Some(RtcAlarms {
        direct_register_access: direct,
        async_wait: profile.async_wait,
        alarm_a_ignore_bit: profile.alarm_a_ignore_bit,
        alarm_b_configuration_supported: profile.alarm_b_configuration_supported,
    });
    Ok(())
}
