//! Evidence-backed clock identities and independently verified control facts.
//!
//! The sidecars retain facts that upstream Rcc cannot express (peripheral-local
//! muxes, active-low reset semantics, and unresolved sources). Only explicitly
//! supported identities enter Rcc; verified controls also enter RccControl.
//! Unresolved kernels remain unresolved. StopMode is a declared build
//! policy, never a statement that CW32 implements STM32 Stop1/Stop2 modes.
use anyhow::{Context, Result, ensure};
use chiptool::ir;
use cw32_data_serde::chip::core::{
    Peripheral,
    peripheral::{Rcc, RccControl, RccWriteKey, rcc},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    #[serde(default, rename = "source_ref")]
    _source_ref: Option<String>,
    url: String,
    sha256: String,
    artifact: String,
    derived_from: Option<String>,
    conversion: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Evidence {
    source_id: String,
    locator: String,
    claim: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Deserialize)]
#[serde(deny_unknown_fields)]
struct Field {
    peripheral: String,
    register: String,
    field: String,
}
impl Field {
    fn upstream(&self) -> rcc::Field {
        rcc::Field {
            register: self.register.clone(),
            field: self.field.clone(),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
enum Kernel {
    Clock(Clock),
    Mux(Mux),
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Clock {
    clock: String,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Mux {
    mux: Field,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WriteKey {
    field: Field,
    value: u32,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RelatedField {
    role: String,
    field: Field,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StopPolicy {
    value: rcc::StopMode,
    basis: String,
    reason: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    name: String,
    status: String,
    bus_clock: Option<String>,
    kernel_clock: Option<Kernel>,
    #[serde(default)]
    kernel_sources: Vec<String>,
    enable: Option<Field>,
    reset: Option<Field>,
    reset_status: String,
    enable_active_value: Option<u8>,
    reset_asserted_value: Option<u8>,
    enable_write_key: Option<WriteKey>,
    #[serde(default)]
    related_fields: Vec<RelatedField>,
    shared_enable_group: Option<String>,
    shared_reset_group: Option<String>,
    #[serde(default)]
    evidence: BTreeMap<String, Vec<Evidence>>,
    #[serde(default)]
    blockers: Vec<String>,
    #[serde(default)]
    notes: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ClockFile {
    schema_version: u32,
    profile: String,
    controller: String,
    sources: BTreeMap<String, Source>,
    stop_mode_policy: StopPolicy,
    peripherals: Vec<Record>,
    limitations: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct ClockReport {
    pub peripheral_records: usize,
    pub supported_records: usize,
    pub control_records: usize,
    pub partial_records: usize,
    pub shared_enable_groups: usize,
    pub shared_reset_groups: usize,
    pub limitations: Vec<String>,
}

/// Apply one explicitly configured family sidecar after the curated register IR
/// and all peripheral instances have been loaded. A missing sidecar is an error.
pub fn apply(
    root: &Path,
    path: &str,
    profile: &str,
    peripherals: &mut [Peripheral],
    registers: &BTreeMap<String, ir::IR>,
) -> Result<ClockReport> {
    let file: ClockFile = crate::read_yaml(root.join(path))
        .with_context(|| format!("parse clock metadata {path}"))?;
    validate_source_lock(root, &file)?;
    apply_file(&file, profile, peripherals, registers)
}

// Match exact artifact identity against the canonical external-source lock.
// Generation requires no redistributable copy of the vendor documents.
#[derive(Deserialize)]
struct EvidenceLock {
    artifacts: Vec<LockedArtifact>,
}
#[derive(Deserialize)]
struct LockedArtifact {
    path: String,
    url: String,
    sha256: String,
    #[serde(default)]
    members: Vec<LockedMember>,
    text: Option<LockedMember>,
}
#[derive(Deserialize)]
struct LockedMember {
    path: String,
    sha256: String,
}
fn validate_source_lock(root: &Path, file: &ClockFile) -> Result<()> {
    let lock: EvidenceLock =
        serde_json::from_slice(&fs::read(root.join("sources/evidence-sources.json"))?)?;
    for (id, source) in &file.sources {
        ensure!(
            lock.artifacts.iter().any(|artifact| {
                artifact.url == source.url
                    && ((artifact.path == source.artifact && artifact.sha256 == source.sha256)
                        || artifact
                            .members
                            .iter()
                            .chain(artifact.text.iter())
                            .any(|member| {
                                member.path == source.artifact && member.sha256 == source.sha256
                            }))
            }),
            "{}.{} source URL, artifact or SHA-256 differs from canonical lock",
            file.profile,
            id
        );
    }
    Ok(())
}

fn evidence(file: &ClockFile, record: &Record, fact: &str) -> Result<()> {
    let references = record
        .evidence
        .get(fact)
        .with_context(|| format!("{}.{} lacks evidence", record.name, fact))?;
    ensure!(
        !references.is_empty(),
        "{}.{} has empty evidence",
        record.name,
        fact
    );
    for item in references {
        ensure!(
            file.sources.contains_key(&item.source_id),
            "unknown clock evidence source {}",
            item.source_id
        );
        ensure!(
            !item.locator.trim().is_empty() && !item.claim.trim().is_empty(),
            "clock evidence lacks locator or claim"
        );
    }
    Ok(())
}

fn field_width(
    field: &Field,
    peripherals: &[Peripheral],
    registers: &BTreeMap<String, ir::IR>,
) -> Result<u32> {
    let peripheral = peripherals
        .iter()
        .find(|p| p.name == field.peripheral)
        .with_context(|| format!("clock field has unknown peripheral {}", field.peripheral))?;
    let reference = peripheral
        .registers
        .as_ref()
        .context("clock field peripheral has no registers")?;
    let ir = registers
        .get(&reference.kind)
        .context("clock peripheral IR missing")?;
    let block = ir
        .blocks
        .get(&reference.block)
        .context("clock peripheral block missing")?;
    let item = block
        .items
        .iter()
        .find(|item| item.name == field.register)
        .with_context(|| {
            format!(
                "unknown clock register {}.{}",
                field.peripheral, field.register
            )
        })?;
    let ir::BlockItemInner::Register(register) = &item.inner else {
        anyhow::bail!("clock field refers to nested block")
    };
    ensure!(
        register.access == ir::Access::ReadWrite,
        "clock control requires readable and writable register"
    );
    let fieldset = ir
        .fieldsets
        .get(
            register
                .fieldset
                .as_ref()
                .context("clock register has no fieldset")?,
        )
        .context("clock fieldset missing")?;
    let member = fieldset
        .fields
        .iter()
        .find(|member| member.name == field.field)
        .with_context(|| {
            format!(
                "unknown clock field {}.{}.{}",
                field.peripheral, field.register, field.field
            )
        })?;
    Ok(member.bit_size)
}

fn validate_shared(file: &ClockFile, enable: bool) -> Result<usize> {
    let mut fields = BTreeMap::<&Field, Vec<&Record>>::new();
    let mut groups = BTreeMap::<&str, &Field>::new();
    for record in &file.peripherals {
        let (field, group) = if enable {
            (&record.enable, &record.shared_enable_group)
        } else {
            (&record.reset, &record.shared_reset_group)
        };
        ensure!(
            group.is_none() || field.is_some(),
            "shared clock group has no field"
        );
        if let Some(field) = field {
            fields.entry(field).or_default().push(record);
            if let Some(group) = group {
                ensure!(!group.trim().is_empty(), "empty shared clock group");
                if let Some(old) = groups.insert(group, field) {
                    ensure!(
                        old == field,
                        "shared clock group {group} maps to different fields"
                    );
                }
            }
        }
    }
    for records in fields.values() {
        let groups: BTreeSet<_> = records
            .iter()
            .map(|record| {
                if enable {
                    record.shared_enable_group.as_deref()
                } else {
                    record.shared_reset_group.as_deref()
                }
            })
            .collect();
        let active_values: BTreeSet<_> = records
            .iter()
            .filter_map(|record| {
                if enable {
                    record.enable_active_value
                } else {
                    record.reset_asserted_value
                }
            })
            .collect();
        ensure!(
            active_values.len() <= 1,
            "shared clock field has conflicting active polarity"
        );
        if records.len() > 1 {
            ensure!(
                groups.len() == 1 && !groups.contains(&None),
                "shared clock field requires one explicit shared group"
            );
            for record in records {
                evidence(
                    file,
                    record,
                    if enable {
                        "shared_enable_group"
                    } else {
                        "shared_reset_group"
                    },
                )?;
            }
        } else {
            ensure!(
                groups.contains(&None),
                "shared clock group has only one member"
            );
        }
    }
    Ok(groups.len())
}

fn apply_file(
    file: &ClockFile,
    profile: &str,
    peripherals: &mut [Peripheral],
    registers: &BTreeMap<String, ir::IR>,
) -> Result<ClockReport> {
    ensure!(file.schema_version == 1, "unsupported clock schema");
    ensure!(file.profile == profile, "clock profile mismatch");
    ensure!(
        peripherals.iter().any(|p| p.name == file.controller),
        "clock controller missing"
    );
    ensure!(!file.sources.is_empty(), "clock metadata requires sources");
    for source in file.sources.values() {
        ensure!(
            source
                .conversion
                .as_ref()
                .is_none_or(|value| !value.trim().is_empty()),
            "empty clock source conversion"
        );
        if let Some(parent) = &source.derived_from {
            ensure!(
                file.sources.contains_key(parent),
                "derived clock source parent missing"
            );
        }
        ensure!(
            source.sha256.len() == 64 && source.sha256.bytes().all(|b| b.is_ascii_hexdigit()),
            "invalid clock source hash"
        );
        ensure!(
            source.url.starts_with("https://") && !source.artifact.trim().is_empty(),
            "clock source needs URL and artifact"
        );
    }
    ensure!(
        file.stop_mode_policy.basis == "conservative-build-policy",
        "StopMode must be explicit software policy"
    );
    ensure!(
        file.stop_mode_policy.value == rcc::StopMode::Stop1
            && !file.stop_mode_policy.reason.trim().is_empty(),
        "unreviewed CW32 stop policy"
    );
    let inventory: BTreeSet<_> = peripherals.iter().map(|p| p.name.as_str()).collect();
    let records: BTreeSet<_> = file.peripherals.iter().map(|p| p.name.as_str()).collect();
    ensure!(
        records.len() == file.peripherals.len(),
        "duplicate clock peripheral record"
    );
    ensure!(
        records == inventory,
        "clock inventory differs from peripheral inventory"
    );
    let mut report = ClockReport {
        peripheral_records: records.len(),
        limitations: file.limitations.clone(),
        ..Default::default()
    };
    // Validate everything before mutating generated metadata. Partial records
    // also resolve their fields; stale provenance cannot hide behind a blocker.
    let mut output = BTreeMap::new();
    let mut controls = BTreeMap::new();
    for record in &file.peripherals {
        ensure!(
            record.status == "supported" || record.status == "partial",
            "unknown clock support status"
        );
        ensure!(
            record
                .blockers
                .iter()
                .chain(&record.notes)
                .all(|note| !note.trim().is_empty()),
            "empty clock blocker/note"
        );
        for refs in record.evidence.values() {
            for reference in refs {
                ensure!(
                    file.sources.contains_key(&reference.source_id)
                        && !reference.locator.trim().is_empty()
                        && !reference.claim.trim().is_empty(),
                    "invalid clock evidence"
                );
            }
        }
        if let Some(clock) = &record.bus_clock {
            ensure!(!clock.trim().is_empty(), "empty bus clock");
            evidence(file, record, "bus_clock")?;
        }
        ensure!(
            record
                .kernel_sources
                .iter()
                .all(|source| !source.trim().is_empty()),
            "empty kernel source option"
        );
        ensure!(
            record.kernel_sources.is_empty() || record.kernel_clock.is_some(),
            "kernel options lack source fact"
        );
        if let Some(kernel) = &record.kernel_clock {
            evidence(file, record, "kernel_clock")?;
            match kernel {
                Kernel::Clock(clock) => {
                    ensure!(!clock.clock.trim().is_empty(), "empty kernel clock")
                }
                Kernel::Mux(mux) => {
                    field_width(&mux.mux, peripherals, registers)?;
                }
            }
        }
        for (name, field) in [("enable", &record.enable), ("reset", &record.reset)] {
            if let Some(field) = field {
                evidence(file, record, name)?;
                ensure!(
                    field_width(field, peripherals, registers)? == 1,
                    "clock gate/reset is not one bit"
                );
            }
        }
        match record.reset_status.as_str() {
            "verified-field" => {
                ensure!(record.reset.is_some(), "verified reset lacks field");
            }
            "no-controller-reset" => {
                ensure!(record.reset.is_none(), "absent reset has field");
                evidence(file, record, "reset_status")?;
            }
            "not-established" => {
                ensure!(
                    record.status == "partial",
                    "supported reset not established"
                );
            }
            _ => anyhow::bail!("invalid reset status"),
        }
        for (name, value, field) in [
            (
                "enable_active_value",
                record.enable_active_value,
                &record.enable,
            ),
            (
                "reset_asserted_value",
                record.reset_asserted_value,
                &record.reset,
            ),
        ] {
            if let Some(value) = value {
                ensure!(
                    value <= 1 && field.is_some(),
                    "invalid clock control active level"
                );
                evidence(file, record, name)?;
            }
        }
        if let Some(key) = &record.enable_write_key {
            let width = field_width(&key.field, peripherals, registers)?;
            ensure!(
                width <= 32 && (width == 32 || key.value < (1u32 << width)),
                "clock write key does not fit field"
            );
            let enable = record
                .enable
                .as_ref()
                .context("clock write key has no enable field")?;
            ensure!(
                key.field.peripheral == enable.peripheral && key.field.register == enable.register,
                "clock key must accompany enable in same register"
            );
            evidence(file, record, "enable_write_key")?;
        }
        for related in &record.related_fields {
            ensure!(
                !related.role.trim().is_empty(),
                "related clock field lacks role"
            );
            field_width(&related.field, peripherals, registers)?;
            evidence(file, record, &related.role)?;
        }
        // A kernel may remain partial while its gate, bus and reset controls
        // are established. Keep these facts independent; never infer a kernel.
        if let (Some(bus_clock), Some(enable), Some(active)) = (
            &record.bus_clock,
            &record.enable,
            record.enable_active_value,
        ) {
            ensure!(
                enable.peripheral == file.controller,
                "RccControl enable belongs to another controller"
            );
            if let Some(reset) = &record.reset {
                ensure!(
                    reset.peripheral == file.controller,
                    "RccControl reset belongs to another controller"
                );
                ensure!(
                    record.reset_asserted_value.is_some(),
                    "RccControl reset lacks asserted polarity"
                );
            }
            controls.insert(
                record.name.clone(),
                RccControl {
                    controller: file.controller.clone(),
                    bus_clock: bus_clock.clone(),
                    enable: enable.upstream(),
                    enable_active_value: active != 0,
                    enable_write_key: record.enable_write_key.as_ref().map(|key| RccWriteKey {
                        field: key.field.upstream(),
                        value: key.value,
                    }),
                    reset: record.reset.as_ref().map(Field::upstream),
                    reset_asserted_value: record.reset_asserted_value.map(|value| value != 0),
                    shared_enable_group: record.shared_enable_group.clone(),
                    shared_reset_group: record.shared_reset_group.clone(),
                },
            );
        }
        if record.status == "partial" {
            ensure!(!record.blockers.is_empty(), "partial clock needs blocker");
            report.partial_records += 1;
            continue;
        }
        ensure!(record.blockers.is_empty(), "supported clock has blockers");
        ensure!(
            record.enable_active_value.is_some(),
            "supported clock lacks enable polarity"
        );
        ensure!(
            record.reset.is_none() || record.reset_asserted_value.is_some(),
            "supported clock lacks reset polarity"
        );
        let bus_clock = record
            .bus_clock
            .clone()
            .context("supported clock lacks bus source")?;
        let enable = record
            .enable
            .as_ref()
            .context("supported clock lacks enable")?;
        ensure!(
            enable.peripheral == file.controller,
            "upstream Rcc cannot scope enable peripheral"
        );
        let kernel_clock = match record
            .kernel_clock
            .as_ref()
            .context("supported clock lacks kernel source")?
        {
            Kernel::Clock(clock) => rcc::KernelClock::Clock(clock.clock.clone()),
            Kernel::Mux(mux) => {
                ensure!(
                    mux.mux.peripheral == file.controller,
                    "upstream Rcc cannot scope peripheral-local kernel mux"
                );
                rcc::KernelClock::Mux(mux.mux.upstream())
            }
        };
        if let Some(reset) = &record.reset {
            ensure!(
                reset.peripheral == file.controller,
                "upstream Rcc cannot scope reset peripheral"
            );
        }
        output.insert(
            record.name.clone(),
            Rcc {
                bus_clock,
                kernel_clock,
                enable: enable.upstream(),
                reset: record.reset.as_ref().map(Field::upstream),
                stop_mode: file.stop_mode_policy.value.clone(),
            },
        );
        report.supported_records += 1;
    }
    report.control_records = controls.len();
    report.shared_enable_groups = validate_shared(file, true)?;
    report.shared_reset_groups = validate_shared(file, false)?;
    report.limitations.push(format!("Clock metadata: {} complete identities, {} independently verified controls; {} partial identities remain in the factual sidecar. RccControl carries actual reset polarity, write keys and shared groups without inferring a kernel. Stop1 is only a conservative build policy, not a CW32 hardware mode.", report.supported_records, report.control_records, report.partial_records));
    ensure!(
        peripherals
            .iter()
            .all(|peripheral| peripheral.rcc.is_none() && peripheral.rcc_control.is_none()),
        "clock adapter would overwrite existing RCC"
    );
    for peripheral in peripherals {
        peripheral.rcc = output.remove(&peripheral.name);
        peripheral.rcc_control = controls.remove(&peripheral.name);
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn fixture() -> (ClockFile, Vec<Peripheral>, BTreeMap<String, ir::IR>) {
        let fields = json!({"fields":[{"name":"CLOCK","bit_offset":0,"bit_size":1},{"name":"SELECT","bit_offset":1,"bit_size":2},{"name":"KEY","bit_offset":16,"bit_size":16}]});
        let ir: ir::IR = serde_json::from_value(json!({
            "block/SYSCTRL":{"items":[{"name":"EN","byte_offset":0,"fieldset":"CONTROL"},{"name":"RST","byte_offset":4,"fieldset":"CONTROL"},{"name":"SEL","byte_offset":8,"fieldset":"CONTROL"}]},
            "fieldset/CONTROL":fields,
        })).unwrap();
        let local: ir::IR = serde_json::from_value(json!({"block/DEVICE":{"items":[{"name":"LOCAL","byte_offset":0,"fieldset":"CONTROL"}]},"fieldset/CONTROL":fields})).unwrap();
        let peripherals: Vec<Peripheral> = serde_json::from_value(json!([
            {"name":"SYSCTRL","address":0,"registers":{"kind":"sysctrl","version":"fixture","block":"SYSCTRL"}},
            {"name":"DEVICE","address":16,"registers":{"kind":"device","version":"fixture","block":"DEVICE"}}
        ])).unwrap();
        let ev =
            json!([{"source_id":"source","locator":"table 1","claim":"Synthetic unit test fact"}]);
        let file: ClockFile = serde_json::from_value(json!({
            "schema_version":1,"profile":"TEST","controller":"SYSCTRL",
            "sources":{"source":{"url":"https://example.invalid/fixture","artifact":"fixture","sha256":"0000000000000000000000000000000000000000000000000000000000000000"}},
            "stop_mode_policy":{"value":"Stop1","basis":"conservative-build-policy","reason":"Synthetic policy, not hardware fact"},
            "peripherals":[
                {"name":"SYSCTRL","status":"partial","reset_status":"not-established","blockers":["Controller has no self gate."]},
                {"name":"DEVICE","status":"supported","bus_clock":"PCLK","kernel_clock":{"clock":"PCLK"},"enable":{"peripheral":"SYSCTRL","register":"EN","field":"CLOCK"},"reset":{"peripheral":"SYSCTRL","register":"RST","field":"CLOCK"},"reset_status":"verified-field","enable_active_value":1,"reset_asserted_value":0,"evidence":{"bus_clock":ev,"kernel_clock":ev,"enable":ev,"reset":ev,"enable_active_value":ev,"reset_asserted_value":ev}}
            ],"limitations":["Synthetic metadata."]
        })).unwrap();
        (
            file,
            peripherals,
            BTreeMap::from([("sysctrl".into(), ir), ("device".into(), local)]),
        )
    }
    #[test]
    fn all_authored_family_sidecars_validate_against_curated_ir() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let mut count = 0;
        for entry in fs::read_dir(root.join("cw32-data/clock")).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().and_then(|v| v.to_str()) != Some("yaml") {
                continue;
            }
            let file: ClockFile = serde_yaml::from_slice(&fs::read(&path).unwrap()).unwrap();
            let chip: cw32_data_serde::Chip = serde_json::from_slice(
                &fs::read(
                    root.join("cw32-data/data/chips")
                        .join(format!("{}.json", file.profile)),
                )
                .unwrap(),
            )
            .unwrap();
            let mut peripherals = chip.cores[0].peripherals.clone();
            for peripheral in &mut peripherals {
                peripheral.rcc = None;
                peripheral.rcc_control = None;
            }
            let mut registers = BTreeMap::new();
            for peripheral in &peripherals {
                if let Some(reference) = &peripheral.registers {
                    let path = root
                        .join("cw32-data/registers")
                        .join(format!("{}_{}.yaml", reference.kind, reference.version));
                    registers.insert(
                        reference.kind.clone(),
                        serde_yaml::from_slice(&fs::read(path).unwrap()).unwrap(),
                    );
                }
            }
            validate_source_lock(root, &file).unwrap();
            apply_file(&file, &file.profile, &mut peripherals, &registers)
                .with_context(|| format!("validate {}", file.profile))
                .unwrap();
            count += 1;
        }
        assert_eq!(count, 13);
    }
    #[test]
    fn supported_identity_preserves_upstream_shape_and_explicit_policy() {
        let (file, mut peripherals, registers) = fixture();
        let report = apply_file(&file, "TEST", &mut peripherals, &registers).unwrap();
        assert_eq!(report.supported_records, 1);
        assert_eq!(report.partial_records, 1);
        assert_eq!(report.control_records, 1);
        let control = peripherals[1].rcc_control.as_ref().unwrap();
        assert_eq!(control.bus_clock, "PCLK");
        assert!(control.enable_active_value);
        assert_eq!(control.reset_asserted_value, Some(false));
        assert!(peripherals[0].rcc.is_none());
        let metadata = serde_json::to_value(peripherals[1].rcc.as_ref().unwrap()).unwrap();
        assert_eq!(
            metadata,
            json!({"bus_clock":"PCLK","kernel_clock":"PCLK","enable":{"register":"EN","field":"CLOCK"},"reset":{"register":"RST","field":"CLOCK"}})
        );
        assert_eq!(file.peripherals[1].reset_asserted_value, Some(0));
    }
    #[test]
    fn missing_required_clock_is_not_filled_from_bus() {
        let (mut file, mut peripherals, registers) = fixture();
        file.peripherals[1].kernel_clock = None;
        assert!(
            apply_file(&file, "TEST", &mut peripherals, &registers)
                .unwrap_err()
                .to_string()
                .contains("lacks kernel")
        );
        assert!(peripherals.iter().all(|p| p.rcc.is_none()));
    }
    #[test]
    fn peripheral_local_mux_stays_partial() {
        let (mut file, mut peripherals, registers) = fixture();
        file.peripherals[1].kernel_clock = Some(Kernel::Mux(Mux {
            mux: Field {
                peripheral: "DEVICE".into(),
                register: "LOCAL".into(),
                field: "SELECT".into(),
            },
        }));
        assert!(
            apply_file(&file, "TEST", &mut peripherals, &registers)
                .unwrap_err()
                .to_string()
                .contains("peripheral-local")
        );
        file.peripherals[1].status = "partial".into();
        file.peripherals[1]
            .blockers
            .push("Upstream schema cannot scope local mux.".into());
        let report = apply_file(&file, "TEST", &mut peripherals, &registers).unwrap();
        assert_eq!(report.supported_records, 0);
        assert!(peripherals[1].rcc.is_none());
        assert_eq!(report.control_records, 1);
        assert_eq!(
            peripherals[1].rcc_control.as_ref().unwrap().bus_clock,
            "PCLK"
        );
    }
    #[test]
    fn partial_control_without_kernel_remains_independent() {
        let (mut file, mut peripherals, registers) = fixture();
        file.peripherals[1].status = "partial".into();
        file.peripherals[1].kernel_clock = None;
        file.peripherals[1]
            .blockers
            .push("Kernel source not established.".into());
        let report = apply_file(&file, "TEST", &mut peripherals, &registers).unwrap();
        assert_eq!(report.control_records, 1);
        assert!(peripherals[1].rcc.is_none());
        assert!(peripherals[1].rcc_control.is_some());
    }
    #[test]
    fn unscoped_partial_control_and_missing_polarity_are_rejected() {
        let (mut file, mut peripherals, registers) = fixture();
        file.peripherals[1].status = "partial".into();
        file.peripherals[1]
            .blockers
            .push("Kernel source not established.".into());
        file.peripherals[1].reset_asserted_value = None;
        assert!(
            apply_file(&file, "TEST", &mut peripherals, &registers)
                .unwrap_err()
                .to_string()
                .contains("polarity")
        );
        assert!(peripherals.iter().all(|p| p.rcc_control.is_none()));
        file.peripherals[1].reset_asserted_value = Some(0);
        file.peripherals[1].enable = Some(Field {
            peripheral: "DEVICE".into(),
            register: "LOCAL".into(),
            field: "CLOCK".into(),
        });
        assert!(
            apply_file(&file, "TEST", &mut peripherals, &registers)
                .unwrap_err()
                .to_string()
                .contains("another controller")
        );
    }
    #[test]
    fn authored_source_identity_is_locked() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let mut file: ClockFile =
            crate::read_yaml(root.join("cw32-data/clock/cw32l012.yaml")).unwrap();
        validate_source_lock(root, &file).unwrap();
        file.sources.values_mut().next().unwrap().sha256 = "0".repeat(64);
        assert!(
            validate_source_lock(root, &file)
                .unwrap_err()
                .to_string()
                .contains("canonical lock")
        );
    }
    #[test]
    fn invalid_partial_field_is_rejected() {
        let (mut file, mut peripherals, registers) = fixture();
        file.peripherals[1].status = "partial".into();
        file.peripherals[1]
            .blockers
            .push("Pending kernel review.".into());
        file.peripherals[1].enable.as_mut().unwrap().field = "MISSING".into();
        assert!(
            apply_file(&file, "TEST", &mut peripherals, &registers)
                .unwrap_err()
                .to_string()
                .contains("unknown clock field")
        );
    }
    #[test]
    fn missing_evidence_and_invented_stop_semantics_are_rejected() {
        let (mut file, mut peripherals, registers) = fixture();
        file.peripherals[1].evidence.remove("kernel_clock");
        assert!(apply_file(&file, "TEST", &mut peripherals, &registers).is_err());
        let (mut file, mut peripherals, registers) = fixture();
        file.stop_mode_policy.basis = "vendor-hardware-mode".into();
        assert!(apply_file(&file, "TEST", &mut peripherals, &registers).is_err());
    }
    #[test]
    fn shared_gate_requires_explicit_group_and_evidence() {
        let (mut file, mut peripherals, registers) = fixture();
        let mut alias = peripherals[1].clone();
        alias.name = "ALIAS".into();
        peripherals.push(alias);
        let mut record = file.peripherals[1].clone();
        record.name = "ALIAS".into();
        file.peripherals.push(record);
        assert!(
            apply_file(&file, "TEST", &mut peripherals, &registers)
                .unwrap_err()
                .to_string()
                .contains("shared clock field")
        );
        for record in &mut file.peripherals[1..] {
            record.shared_enable_group = Some("shared-device".into());
            record.shared_reset_group = Some("shared-device".into());
            record.evidence.insert(
                "shared_enable_group".into(),
                record.evidence["enable"].clone(),
            );
            record.evidence.insert(
                "shared_reset_group".into(),
                record.evidence["reset"].clone(),
            );
        }
        let report = apply_file(&file, "TEST", &mut peripherals, &registers).unwrap();
        assert_eq!(report.shared_enable_groups, 1);
        assert_eq!(report.shared_reset_groups, 1);
        for peripheral in &peripherals[1..] {
            let control = peripheral.rcc_control.as_ref().unwrap();
            assert_eq!(
                control.shared_enable_group.as_deref(),
                Some("shared-device")
            );
            assert_eq!(control.shared_reset_group.as_deref(), Some("shared-device"));
        }
    }
    #[test]
    fn write_key_field_and_value_are_validated() {
        let (mut file, mut peripherals, registers) = fixture();
        file.peripherals[1].enable_write_key = Some(WriteKey {
            field: Field {
                peripheral: "SYSCTRL".into(),
                register: "EN".into(),
                field: "KEY".into(),
            },
            value: 0x5a5a,
        });
        let ev = file.peripherals[1].evidence["enable"].clone();
        file.peripherals[1]
            .evidence
            .insert("enable_write_key".into(), ev);
        apply_file(&file, "TEST", &mut peripherals, &registers).unwrap();
        let control = peripherals[1].rcc_control.as_ref().unwrap();
        assert_eq!(control.enable_write_key.as_ref().unwrap().value, 0x5a5a);
        peripherals.iter_mut().for_each(|p| {
            p.rcc = None;
            p.rcc_control = None;
        });
        file.peripherals[1].enable_write_key.as_mut().unwrap().value = 0x10000;
        assert!(
            apply_file(&file, "TEST", &mut peripherals, &registers)
                .unwrap_err()
                .to_string()
                .contains("does not fit")
        );
    }
    #[test]
    fn wrong_profile_or_missing_inventory_is_rejected() {
        let (mut file, mut peripherals, registers) = fixture();
        assert!(apply_file(&file, "OTHER", &mut peripherals, &registers).is_err());
        file.peripherals.remove(0);
        assert!(apply_file(&file, "TEST", &mut peripherals, &registers).is_err());
    }
}
