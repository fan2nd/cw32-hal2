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

/// Validate the separate live native-LSI RTC capability without projecting or
/// changing any calendar, clock, peripheral, register or metadata model.
pub fn validate_native_live_lsi(
    root: &Path,
    chip: &crate::ChipInput,
    line: &str,
    core: &chip::Core,
    registers: &BTreeMap<String, ir::IR>,
) -> Result<()> {
    use serde_json::json;
    let value: serde_yaml::Value =
        crate::read_yaml(root.join("cw32-data/rtc-live-lsi-qualified.yaml"))?;
    let policy = serde_json::to_value(value)?;
    let qualification = json!({
        "source": "LSI", "source_encoding": 2,
        "prescaler_first": 1, "prescaler_second": 16400, "calendar_divisor": 32800,
        "admission": "live_factory_reuse_only", "source_identity": "independent_of_sysclk",
        "factory_read_width_bits": 16, "factory_erased_raw_halfword": 65535,
        "factory_zero_valid": true, "software_lsien_required": true,
        "stable_required": true, "masked_factory_trim_match_required": true,
        "oscillator_writes": "forbidden", "rate_only": true, "cycle_timing_qualified": false,
        "attach_compensation_enabled": false, "initialization_compensation": "all_zero",
        "initialization": "unset_and_stopped_only",
        "l010_running_set_datetime": "UnqualifiedWriteTiming",
        "async_alarm_wait": "UnsupportedAsyncAlarmSource",
        "rtc_domain_loss": "ClockDomainUnavailable",
        "domain_loss_cleanup": "suppress_further_key_writes",
        "source_switch": "verify_source_before_reducing_first_divisor"
    });
    ensure!(
        policy.as_object().is_some_and(|p| p.len() == 6)
            && policy["schema_version"] == 1
            && policy["source_authority"] == "sources/evidence-sources.json"
            && policy["evidence"] == "docs/native-live-factory-lsi-rtc-qualification.json"
            && policy["envelope_policy"] == "cw32-data/lsi-sysclk-qualified.yaml"
            && policy["qualification"] == qualification,
        "unqualified native live-LSI RTC policy"
    );
    let families = policy["families"].as_object().context("missing RTC family roster")?;
    ensure!(
        families.keys().map(String::as_str).collect::<Vec<_>>() == ["CW32L010", "CW32L011", "CW32L012"],
        "native live-LSI RTC family scope changed"
    );
    let roster = [
        ("CW32L010", "cw32l010_v1", 10, vec![("CW32L010F8P6", "TSSOP20"), ("CW32L010F8U6", "QFN20"), ("CW32L010Y8M6", "SOP16")]),
        ("CW32L011", "cw32l011_v1", 10, vec![("CW32L011K8T6", "LQFP32"), ("CW32L011K8U6", "QFN32")]),
        ("CW32L012", "cw32l012_v1", 9, vec![("CW32L012C8T6", "LQFP48"), ("CW32L012C8U6", "QFN48")]),
    ];
    for (family, version, trim, parts) in &roster {
        let parts: Vec<_> = parts.iter().map(|(name, package)| json!({"name": name, "package": package})).collect();
        ensure!(families[*family] == json!({
            "exact_parts": parts, "rtc_version": version, "sysctrl_version": version,
            "envelope_pointer": format!("/families/{family}/lsi_sysclk"), "trim_bit_size": trim
        }), "native live-LSI RTC own-family policy mismatch: {family}");
    }
    let proof: serde_json::Value = serde_json::from_slice(&fs::read(root.join(
        "docs/native-live-factory-lsi-rtc-qualification.json",
    ))?)?;
    ensure!(proof["schema_version"] == 1 && proof["policy"] == policy, "RTC policy/proof mismatch");
    let exact_identity = roster.iter().find(|(_, _, _, parts)| parts.iter().any(|(name, _)| *name == chip.name));
    let native_ip = core.peripherals.iter().filter_map(|p| p.registers.as_ref()).any(|r|
        matches!(r.kind.as_str(), "rtc" | "sysctrl")
            && matches!(r.version.as_str(), "cw32l010_v1" | "cw32l011_v1" | "cw32l012_v1"));
    let family = roster.iter().find(|(family, _, _, _)| *family == line);
    if family.is_none() && exact_identity.is_none() && !native_ip {
        return Ok(());
    }
    let (_, version, trim, parts) = family.context("native RTC IP/part has inconsistent family")?;
    ensure!(native_ip && exact_identity.is_none_or(|v| v.0 == line), "native RTC family/name/IP mismatch");
    let expected_package = parts.iter().find(|(name, _)| *name == chip.name).map(|(_, p)| *p);
    ensure!(expected_package.is_some() || chip.name == line, "unqualified native RTC part");
    if let Some(package) = expected_package {
        ensure!(chip.packages.len() == 1 && chip.packages[0].name == chip.name
            && chip.packages[0].package == package, "native RTC exact package mismatch");
    } else {
        ensure!(chip.packages.is_empty(), "generic native RTC must remain package-neutral");
    }
    let owner = |name: &str, kind: &str, block: &str, address: u32| -> Result<&chip::core::Peripheral> {
        let owners: Vec<_> = core.peripherals.iter().filter(|p| p.name == name).collect();
        ensure!(owners.len() == 1, "native RTC owner must be unique: {name}");
        let p = owners[0];
        let r = p.registers.as_ref().context("missing native RTC registers")?;
        ensure!(p.address == address && r.kind == kind && r.block == block && r.version == *version,
            "native RTC owner identity mismatch: {name}");
        Ok(p)
    };
    let rtc = owner("RTC", "rtc", "RTC", 0x40004400)?;
    let sysctrl = owner("SYSCTRL", "sysctrl", "SYSCTRL", 0x40004000)?;
    ensure!(core.peripherals.iter().filter(|p| p.clock_limits.is_some()).count() == 1,
        "native RTC clock owner must be unique");
    let clock = sysctrl.clock_limits.as_ref().context("missing native RTC oscillator facts")?;
    let exact = expected_package.is_some();
    ensure!(clock.lsi_sysclk.is_some() == exact && clock.lse_configuration.is_some() == exact,
        "native RTC exact/generic capability boundary mismatch");
    let old: Catalog = crate::read_yaml(root.join("cw32-data/rtc-calendar.yaml"))?;
    let old = old.profiles.get(line).context("missing native HSIOSC RTC profile")?;
    ensure!(rtc.rtc_calendar.as_ref() == Some(old) && old.source == "HSIOSC"
        && old.source_encoding == 3 && old.factory_trim_address == clock.factory_hsi_trim_address,
        "native live-LSI must preserve the independent HSIOSC RTC profile");
    let field = |kind: &str, block: &str, register: &str, offset: u32,
                 name: &str, bit: u32, width: u32, access: ir::Access| -> Result<()> {
        let ir = registers.get(kind).context("missing native RTC register IR")?;
        let block = ir.blocks.get(block).context("missing native RTC register block")?;
        let items: Vec<_> = block.items.iter().filter(|r| r.name == register).collect();
        ensure!(items.len() == 1 && items[0].byte_offset == offset && items[0].array.is_none(),
            "native RTC register mismatch: {kind}.{register}");
        let ir::BlockItemInner::Register(r) = &items[0].inner else { anyhow::bail!("native RTC control must be a register") };
        ensure!(r.bit_size == 32 && r.access == access, "native RTC register access mismatch");
        let fs = ir.fieldsets.get(r.fieldset.as_ref().context("missing native RTC fieldset")?).context("unknown native RTC fieldset")?;
        let fields: Vec<_> = fs.fields.iter().filter(|f| f.name == name).collect();
        ensure!(fields.len() == 1 && fields[0].bit_offset == ir::BitOffset::Regular(bit)
            && fields[0].bit_size == width && fields[0].array.is_none(),
            "native RTC field mismatch: {kind}.{register}.{name}");
        if kind == "rtc" && register == "CR1" && name == "SOURCE" {
            ensure!(fields[0].enumm.is_none(), "native RTC SOURCE must remain raw u8");
        }
        Ok(())
    };
    for (reg, offset, name, bit, width) in [
        ("KEY", 0, "KEY", 0, 8), ("CR0", 4, "START", 7, 1), ("CR0", 4, "H24", 3, 1),
        ("CR1", 8, "SOURCE", 8, 3), ("CR1", 8, "WAIT", 2, 1),
        ("PSC", 64, "PSC1", 20, 8), ("PSC", 64, "PSC2", 0, 20),
        ("COMPCFR1", 16, "EN", 15, 1),
    ] {
        field("rtc", "RTC", reg, offset, name, bit, width,
            if reg == "KEY" { ir::Access::Write } else { ir::Access::ReadWrite })?;
    }
    let cr1 = &registers["rtc"].fieldsets["CR1"];
    for (name, bit) in [("ACCESS", 0), ("WINDOW", 1)] {
        ensure!(cr1.fields.iter().any(|f| f.name == name) == (line == "CW32L010"),
            "native RTC ACCESS/WINDOW family distinction mismatch");
        if line == "CW32L010" { field("rtc", "RTC", "CR1", 8, name, bit, 1, ir::Access::ReadWrite)?; }
    }
    for (reg, offset, name, bit, width) in [
        ("CR1", 4, "LSIEN", 3, 1), ("LSI", 32, "TRIM", 0, *trim),
        ("LSI", 32, "WAITCYCLE", 10, 2), ("LSI", 32, "STABLE", 15, 1),
    ] { field("sysctrl", "SYSCTRL", reg, offset, name, bit, width, ir::Access::ReadWrite)?; }
    if let Some(lsi) = &clock.lsi_sysclk {
        let (minimum, maximum, supply) = match line {
            "CW32L010" => (31816, 33784, (1620, 5500)),
            "CW32L011" => (29520, 41000, (1700, 5500)),
            "CW32L012" => (29520, 36080, (1700, 5500)),
            _ => unreachable!(),
        };
        ensure!((lsi.nominal_hz, lsi.minimum_hz, lsi.maximum_hz, lsi.supply_mv,
                 lsi.temperature_c, lsi.factory_trim_address) ==
            (32800, minimum, maximum, supply, (-40, 85), 0x001007c2), "native RTC factory envelope mismatch");
        let shared: serde_yaml::Value = crate::read_yaml(root.join("cw32-data/lsi-sysclk-qualified.yaml"))?;
        let shared = serde_json::to_value(shared)?;
        ensure!(serde_json::to_value(lsi)? == shared["families"][line]["lsi_sysclk"],
            "native RTC envelope reference mismatch");
        ensure!(lsi.maximum_hz <= 1_000_000 && lsi.nominal_hz == 1 * 16400 * 2,
            "native RTC programmable division mismatch");
    }
    Ok(())
}
