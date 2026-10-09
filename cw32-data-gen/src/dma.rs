//! Reviewed DMA projections. The sidecars retain physical ordinals, shared IRQ
//! ownership and request evidence without extending the upstream static schema.
use anyhow::{Context, Result, ensure};
use chiptool::ir;
use cw32_data_serde::chip::{
    self,
    core::{DmaChannels, peripheral::DmaChannel},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    schema_version: u32,
    family: String,
    presence: String,
    coverage: Value,
    hardware_validated: bool,
    sources: BTreeMap<String, Value>,
    evidence: BTreeMap<String, Value>,
    core_dma_channels: Vec<DmaChannels>,
    channel_details: Vec<Value>,
    peripheral_dma_channels: BTreeMap<String, Vec<DmaChannel>>,
    requests: Vec<Value>,
    excluded_sdk_requests: Vec<Value>,
    unknowns: Vec<Value>,
    #[serde(default)]
    source_discrepancies: Vec<Value>,
    request_register: Option<Value>,
    normalization: Option<Value>,
}
#[derive(Debug, Default, Serialize)]
pub struct DmaReport {
    pub physical_channels: usize,
    pub hardware_requests: usize,
    pub excluded_sdk_requests: usize,
    pub unresolved_semantics: usize,
}
fn string<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key]
        .as_str()
        .with_context(|| format!("missing DMA string {key}"))
}
fn evidence(file: &File, value: &Value) -> Result<()> {
    let refs = value["evidence"]
        .as_array()
        .context("DMA fact lacks evidence")?;
    ensure!(!refs.is_empty(), "DMA fact has empty evidence");
    for reference in refs {
        ensure!(
            file.evidence
                .contains_key(reference.as_str().context("invalid DMA evidence ref")?),
            "unknown DMA evidence ref"
        );
    }
    Ok(())
}
fn validate_field(
    registers: &BTreeMap<String, ir::IR>,
    peripheral: &chip::core::Peripheral,
    register: &str,
    field: &str,
    offset: u32,
    width: u32,
) -> Result<()> {
    let reference = peripheral
        .registers
        .as_ref()
        .context("DMA reference has no registers")?;
    let ir = registers
        .get(&reference.kind)
        .context("DMA register IR missing")?;
    let block = ir
        .blocks
        .get(&reference.block)
        .context("DMA block missing")?;
    let item = block
        .items
        .iter()
        .find(|i| i.name == register)
        .context("DMA register missing")?;
    let ir::BlockItemInner::Register(reg) = &item.inner else {
        anyhow::bail!("DMA field is not a register")
    };
    let fields = ir
        .fieldsets
        .get(reg.fieldset.as_ref().context("DMA fieldset missing")?)
        .context("DMA fieldset unresolved")?;
    let field = fields
        .fields
        .iter()
        .find(|f| f.name == field)
        .context("DMA field missing")?;
    ensure!(
        field.bit_offset == ir::BitOffset::Regular(offset) && field.bit_size == width,
        "DMA field layout differs from reviewed source"
    );
    Ok(())
}
pub fn apply(
    root: &Path,
    path: &str,
    profile: &str,
    core: &mut chip::Core,
    registers: &BTreeMap<String, ir::IR>,
) -> Result<DmaReport> {
    let file: File =
        crate::read_yaml(root.join(path)).with_context(|| format!("parse DMA metadata {path}"))?;
    apply_file(&file, profile, core, registers)
}
fn apply_file(
    file: &File,
    profile: &str,
    core: &mut chip::Core,
    registers: &BTreeMap<String, ir::IR>,
) -> Result<DmaReport> {
    ensure!(
        file.schema_version == 1 && file.family == profile,
        "DMA profile mismatch"
    );
    ensure!(
        !file.hardware_validated,
        "DMA metadata must not claim hardware validation"
    );
    for kind in ["channels", "interrupts", "requests"] {
        ensure!(file.coverage[kind] == "complete", "unreviewed DMA coverage");
    }
    ensure!(
        !file.sources.is_empty() && !file.evidence.is_empty(),
        "DMA sources or evidence missing"
    );
    for source in file.sources.values() {
        let hash = string(source, "sha256")?;
        let path = Path::new(string(source, "path")?);
        ensure!(
            hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()),
            "invalid DMA source hash"
        );
        ensure!(
            !path.is_absolute()
                && !path
                    .components()
                    .any(|c| matches!(c, std::path::Component::ParentDir)),
            "nonportable DMA source path"
        );
        ensure!(
            source
                .get("url")
                .or_else(|| source.get("archive_url"))
                .and_then(Value::as_str)
                .is_some_and(|url| url.starts_with("https://www.whxy.com/")),
            "DMA source lacks official URL"
        );
    }
    for item in file.evidence.values() {
        ensure!(
            file.sources.contains_key(string(item, "source")?)
                && item
                    .get("section")
                    .or_else(|| item.get("interpretation"))
                    .and_then(Value::as_str)
                    .is_some_and(|s| !s.is_empty()),
            "unresolved DMA evidence source or section"
        );
        let pages = item["pdf_page_indices"]
            .as_array()
            .context("DMA evidence missing pages")?;
        ensure!(
            !pages.is_empty() && pages.iter().all(|p| p.as_u64().is_some()),
            "invalid DMA evidence page"
        );
    }
    let peripherals: BTreeMap<_, _> = core
        .peripherals
        .iter()
        .map(|p| (p.name.as_str(), p))
        .collect();
    let irqs: BTreeMap<_, _> = core
        .interrupts
        .iter()
        .map(|i| (i.name.as_str(), i.number))
        .collect();
    let channel_names: Vec<_> = file
        .core_dma_channels
        .iter()
        .map(|c| c.name.clone())
        .collect();
    let count = channel_names.len();
    ensure!(
        file.channel_details.len() == count,
        "DMA channel evidence count differs"
    );
    let inventory: BTreeSet<_> = peripherals
        .keys()
        .filter(|n| {
            n.strip_prefix("DMACHANNEL")
                .is_some_and(|s| s.parse::<u8>().is_ok())
        })
        .copied()
        .collect();
    let expected_inventory: BTreeSet<_> = (1..=count).map(|n| format!("DMACHANNEL{n}")).collect();
    ensure!(
        inventory == expected_inventory.iter().map(String::as_str).collect(),
        "DMA physical channel inventory differs"
    );
    if count == 0 {
        ensure!(
            file.presence == "absent"
                && file.evidence.contains_key("absence")
                && file.requests.is_empty()
                && file.peripheral_dma_channels.is_empty()
                && !peripherals.keys().any(|n| n.contains("DMA")),
            "unverified DMA absence"
        );
        ensure!(
            file.request_register.is_none() && file.normalization.is_none(),
            "absent DMA has routing facts"
        );
    } else {
        ensure!(
            file.presence == "present" && peripherals.contains_key("DMA"),
            "DMA controller missing"
        );
        ensure!(
            file.request_register.as_ref()
                == Some(
                    &serde_json::json!({"peripheral_kind":"dmachannel","register":"TRIG","field":"HARDSRC","bit_offset":2,"bit_size":6})
                ),
            "unknown DMA selector layout"
        );
        ensure!(
            file.normalization.is_some(),
            "DMA normalization not documented"
        );
    }
    for (index, (channel, detail)) in file
        .core_dma_channels
        .iter()
        .zip(&file.channel_details)
        .enumerate()
    {
        let n = index + 1;
        ensure!(
            serde_json::to_value(channel)?
                == serde_json::json!({"name":format!("DMA_CH{n}"),"dma":"DMA","channel":index}),
            "unreviewed DMA channel projection"
        );
        let register_name = format!("DMACHANNEL{n}");
        ensure!(
            detail["name"] == channel.name
                && detail["physical_channel"] == n
                && detail["register_peripheral"] == register_name,
            "DMA physical ordinal mismatch"
        );
        let irq = string(detail, "interrupt")?;
        ensure!(
            irqs.get(irq)
                .is_some_and(|n| detail["interrupt_number"] == *n),
            "DMA IRQ reference mismatch"
        );
        ensure!(
            detail["cmsis_symbol"] == format!("{irq}_IRQn")
                && detail["cmsis_line"].as_u64().is_some_and(|line| line > 0),
            "DMA CMSIS evidence missing"
        );
        let peripheral = peripherals[register_name.as_str()];
        ensure!(
            peripheral
                .interrupts
                .iter()
                .any(|i| i.signal == "GLOBAL" && i.interrupt == irq),
            "DMA channel shared IRQ association mismatch"
        );
        evidence(file, detail)?;
        validate_field(registers, peripheral, "TRIG", "HARDSRC", 2, 6)?;
        for register in ["ISR", "ICR"] {
            validate_field(
                registers,
                peripherals["DMA"],
                register,
                &format!("TC{n}"),
                4 * index as u32,
                1,
            )?;
            validate_field(
                registers,
                peripherals["DMA"],
                register,
                &format!("TE{n}"),
                4 * index as u32 + 1,
                1,
            )?;
        }
    }
    let mut projection = BTreeMap::<String, Vec<DmaChannel>>::new();
    let mut requests = BTreeSet::new();
    let mut signals = BTreeSet::new();
    for route in &file.requests {
        let peripheral = string(route, "peripheral")?;
        let signal = string(route, "signal")?;
        let request = route["request"].as_u64().context("DMA request missing")?;
        ensure!(
            peripherals.contains_key(peripheral) && route["dma"] == "DMA",
            "DMA request references absent peripheral/controller"
        );
        ensure!(
            request < 64 && requests.insert(request),
            "duplicate or oversized DMA request"
        );
        ensure!(
            !signal.is_empty()
                && signal
                    .bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
                && signals.insert((peripheral, signal)),
            "invalid or duplicate DMA signal"
        );
        ensure!(
            route["channels"] == serde_json::to_value(&channel_names)?,
            "DMA request has unreviewed channel restriction/expansion"
        );
        ensure!(
            u64::from_str_radix(string(&route["manual"], "binary")?, 2)? == request,
            "DMA manual request number mismatch"
        );
        ensure!(
            file.evidence["request_table"]["pdf_page_indices"]
                .as_array()
                .context("DMA request table pages missing")?
                .contains(&route["manual"]["pdf_page_index"]),
            "DMA request page outside cited table"
        );
        ensure!(
            !string(&route["manual"], "description")?.is_empty(),
            "DMA request description missing"
        );
        let sdk = &route["sdk"];
        ensure!(
            sdk["line"].as_u64().is_some_and(|n| n > 0),
            "DMA SDK line missing"
        );
        match string(sdk, "encoding")? {
            "shifted_left_2_define" => ensure!(
                sdk["raw_value"] == request << 2
                    && string(sdk, "symbol")?.starts_with("DMA_HardTrig_"),
                "DMA shifted SDK encoding mismatch"
            ),
            "unshifted_enum" => ensure!(
                sdk["raw_value"] == request
                    && string(sdk, "symbol")?.starts_with("DMA_TRIGGER_SRC_"),
                "DMA SDK enum encoding mismatch"
            ),
            _ => anyhow::bail!("unknown DMA SDK encoding"),
        }
        evidence(file, route)?;
        projection
            .entry(peripheral.into())
            .or_default()
            .push(DmaChannel {
                signal: signal.into(),
                dma: Some("DMA".into()),
                channel: None,
                dmamux: None,
                remap: vec![],
                request: Some(request as u8),
            });
    }
    ensure!(
        projection == file.peripheral_dma_channels,
        "DMA projection differs from reviewed requests"
    );
    for excluded in &file.excluded_sdk_requests {
        if let Some(value) = excluded["request"].as_u64() {
            ensure!(
                !requests.contains(&value),
                "excluded SDK request was emitted"
            );
        }
    }
    // Supplemental semantic facts remain in the sidecar; these cannot alter the projection.
    let _ = &file.source_discrepancies;
    core.dma_channels = file.core_dma_channels.clone();
    for peripheral in &mut core.peripherals {
        peripheral.dma_channels = projection.remove(&peripheral.name).unwrap_or_default();
    }
    Ok(DmaReport {
        physical_channels: count,
        hardware_requests: requests.len(),
        excluded_sdk_requests: file.excluded_sdk_requests.len(),
        unresolved_semantics: file.unknowns.len(),
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    fn fixture() -> (File, chip::Core, BTreeMap<String, ir::IR>) {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let file = crate::read_yaml(root.join("cw32-data/dma/cw32f020.yaml")).unwrap();
        let chip: cw32_data_serde::Chip = serde_json::from_slice(
            &fs::read(root.join("cw32-data/data/chips/CW32F020.json")).unwrap(),
        )
        .unwrap();
        let core = chip.cores[0].clone();
        let registers = core
            .peripherals
            .iter()
            .map(|p| {
                let r = p.registers.as_ref().unwrap();
                let ir = serde_yaml::from_slice(
                    &fs::read(
                        root.join("cw32-data/registers")
                            .join(format!("{}_{}.yaml", r.kind, r.version)),
                    )
                    .unwrap(),
                )
                .unwrap();
                (r.kind.clone(), ir)
            })
            .collect();
        (file, core, registers)
    }
    #[test]
    fn physical_dma_and_shared_irq_evidence_are_required() {
        let (mut file, mut core, ir) = fixture();
        file.core_dma_channels[0].channel = 1;
        assert!(apply_file(&file, "CW32F020", &mut core, &ir).is_err());
        let (mut file, mut core, ir) = fixture();
        file.channel_details[1]["interrupt"] = Value::String("DMACH23".into());
        assert!(apply_file(&file, "CW32F020", &mut core, &ir).is_err());
    }
    #[test]
    fn request_projection_and_source_codes_must_match() {
        let (mut file, mut core, ir) = fixture();
        file.peripheral_dma_channels.get_mut("UART1").unwrap()[0].request = Some(63);
        assert!(apply_file(&file, "CW32F020", &mut core, &ir).is_err());
        let (mut file, mut core, ir) = fixture();
        file.requests[0]["manual"]["binary"] = Value::String("111111".into());
        assert!(apply_file(&file, "CW32F020", &mut core, &ir).is_err());
        let (mut file, mut core, ir) = fixture();
        file.requests[0]["evidence"] = serde_json::json!(["absent-evidence"]);
        assert!(apply_file(&file, "CW32F020", &mut core, &ir).is_err());
    }
}
