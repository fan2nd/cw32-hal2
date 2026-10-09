//! Reviewed digital and analog routing; no pin or function inference.
mod atim_complementary;
mod atim_pwm;
pub use atim_complementary::apply as apply_atim_complementary;
mod buffered_pwm;
mod classic_adc;
mod classic_pwm;
mod f020;
mod halltim;
pub use halltim::apply as apply_halltim;
mod l012_adc;
mod low_adc;
use anyhow::{Context, Result, ensure};
use chiptool::ir;
use cw32_data_serde::chip;
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

// Retain the established route fingerprint while canonical IDs annotate the
// same SHA-qualified evidence cells.
fn qualified_route_bytes(routes: &[serde_json::Value]) -> Result<Vec<u8>> {
    Ok(serde_json::to_vec(&crate::without_source_aliases(
        &serde_json::Value::Array(routes.to_vec()),
    ))?)
}

#[derive(Deserialize)]
struct Profile {
    schema_version: u32,
    profile: String,
    status: String,
    datasheet: serde_json::Value,
    selector_capability: Option<SelectorCapability>,
    routes: Vec<Route>,
}
#[derive(Deserialize)]
struct Route {
    pin: String,
    af: u8,
    peripheral: String,
    signal: String,
    source_macro: Option<String>,
    source_line: Option<u32>,
    source_kind: Option<String>,
    datasheet_cell: Option<DatasheetCell>,
    gpio_register: String,
    gpio_field: String,
}

#[derive(Deserialize)]
struct DatasheetCell {
    pin: String,
    af: u8,
    table: String,
    pdf_page: u32,
    printed_page: u32,
    function: String,
}

#[derive(Deserialize)]
struct SelectorCapability {
    min: u8,
    max: u8,
    pdf_printed_page_offset: u32,
    evidence: String,
}

#[derive(Clone, Copy)]
struct AfCapabilities {
    max_selector: u8,
    pdf_printed_page_offset: u32,
}

fn capabilities(profile: &str) -> Result<AfCapabilities> {
    // These are source-reviewed family capabilities, not register-width guesses.
    // New-family source hashes, actual numbered table columns and PDF page
    // coordinates are pinned in docs/final-serial-af-evidence.json. The older
    // profiles retain their own reviewed datasheet evidence in cw32-data/af/.
    let (max_selector, pdf_printed_page_offset) = match profile {
        // L012 DS V1.0 Tables 5-3..6 explicitly document selectors 8 and 9.
        "CW32L012" => (9, 3),
        // L011 DS V1.1 uses 1..7; its PDF has three front-matter pages.
        "CW32L011" => (7, 3),
        "CW32F002" | "CW32F003" | "CW32L010" | "CW32F020" | "CW32F030" | "CW32A030"
        | "CW32L031" | "CW32R031" | "CW32W031" | "CW32L052" | "CW32L083" => (7, 1),
        _ => anyhow::bail!("AF family capability not reviewed"),
    };
    Ok(AfCapabilities {
        max_selector,
        pdf_printed_page_offset,
    })
}

fn validate_profile_capabilities(source: &Profile) -> Result<AfCapabilities> {
    let capability = capabilities(&source.profile)?;
    // Newly promoted profiles must also carry the source-backed declaration.
    if matches!(
        source.profile.as_str(),
        "CW32F002" | "CW32F003" | "CW32L010" | "CW32L011" | "CW32L012"
    ) {
        ensure!(
            source.selector_capability.is_some(),
            "AF selector evidence missing"
        );
    }
    if let Some(declared) = &source.selector_capability {
        ensure!(
            declared.min == 1
                && declared.max == capability.max_selector
                && declared.pdf_printed_page_offset == capability.pdf_printed_page_offset
                && !declared.evidence.is_empty(),
            "AF selector capability differs from reviewed family"
        );
    }
    Ok(capability)
}

fn validate_route_evidence(route: &Route, capability: AfCapabilities) -> Result<()> {
    // Four bits of register storage do not authorize undocumented selectors.
    ensure!(
        (1..=capability.max_selector).contains(&route.af),
        "undocumented AF selector"
    );
    let sdk = route.source_macro.as_ref().is_some_and(|s| !s.is_empty())
        && route.source_line.is_some_and(|line| line > 0);
    match route.source_kind.as_deref() {
        // Existing reviewed profiles retain their original SDK evidence schema.
        None => ensure!(sdk, "AF route evidence missing"),
        Some(kind) => {
            let cell = route
                .datasheet_cell
                .as_ref()
                .context("AF datasheet cell missing")?;
            ensure!(
                cell.pin == route.pin
                    && cell.af == route.af
                    && cell.table.starts_with("5-")
                    && cell.table[2..]
                        .parse::<u32>()
                        .is_ok_and(|n| (3..=8).contains(&n))
                    && cell.pdf_page > capability.pdf_printed_page_offset
                    && cell
                        .printed_page
                        .checked_add(capability.pdf_printed_page_offset)
                        == Some(cell.pdf_page)
                    && !cell.function.is_empty(),
                "AF datasheet coordinate invalid"
            );
            match kind {
                "sdk-and-datasheet" | "datasheet-corrected-sdk" => {
                    ensure!(sdk, "AF SDK evidence missing")
                }
                // The official datasheet is authoritative. Do not fabricate a
                // vendor macro when its SDK omits a documented route.
                "datasheet" => ensure!(
                    route.source_macro.is_none() && route.source_line.is_none(),
                    "datasheet-only route claims SDK evidence"
                ),
                _ => anyhow::bail!("unknown AF evidence kind"),
            }
        }
    }
    Ok(())
}

pub fn apply(
    root: &Path,
    path: &str,
    profile: &str,
    core: &mut chip::Core,
    registers: &BTreeMap<String, ir::IR>,
) -> Result<usize> {
    let source: Profile = crate::read_yaml(root.join(path))?;
    project(&source, profile, core, registers)
}

fn project(
    source: &Profile,
    profile: &str,
    core: &mut chip::Core,
    registers: &BTreeMap<String, ir::IR>,
) -> Result<usize> {
    ensure!(
        source.schema_version == 1 && source.profile == profile,
        "AF profile mismatch"
    );
    ensure!(
        source.status == "verified-sdk-and-datasheet",
        "AF candidates are not reviewed metadata"
    );
    ensure!(
        source.datasheet["sha256"]
            .as_str()
            .is_some_and(|s| s.len() == 64)
            && source.datasheet["af_evidence"]
                .as_str()
                .is_some_and(|s| !s.is_empty()),
        "AF evidence missing"
    );
    let capability = validate_profile_capabilities(source)?;
    let pins: BTreeSet<_> = core.pins.iter().map(|p| p.name.clone()).collect();
    let mut seen = BTreeSet::new();
    let mut projected = 0;
    for route in &source.routes {
        validate_route_evidence(route, capability)?;
        ensure!(
            route.pin.len() >= 3 && route.pin.starts_with('P'),
            "invalid AF pin"
        );
        let n: u32 = route.pin[2..].parse()?;
        ensure!(n < 16, "invalid AF pin number");
        let port = format!("GPIO{}", &route.pin[1..2]);
        let gpio = core
            .peripherals
            .iter()
            .find(|p| p.name == port)
            .context("AF GPIO missing")?;
        let reference = gpio
            .registers
            .as_ref()
            .context("AF GPIO registers missing")?;
        let ir = registers
            .get(&reference.kind)
            .context("AF GPIO IR missing")?;
        let block = ir
            .blocks
            .get(&reference.block)
            .context("AF GPIO block missing")?;
        let register = block
            .items
            .iter()
            .find(|i| i.name == route.gpio_register)
            .context("AF register missing")?;
        let ir::BlockItemInner::Register(register) = &register.inner else {
            anyhow::bail!("AF target not a register");
        };
        let fields = ir
            .fieldsets
            .get(register.fieldset.as_ref().context("AF fieldset missing")?)
            .context("AF fieldset missing")?;
        let field = fields
            .fields
            .iter()
            .find(|f| f.name == route.gpio_field)
            .context("AF field missing")?;
        ensure!(
            field.bit_offset == ir::BitOffset::Regular((n % 8) * 4)
                && u32::from(route.af) < (1u32 << field.bit_size),
            "AF field/value mismatch"
        );
        ensure!(
            seen.insert((
                route.peripheral.clone(),
                route.pin.clone(),
                route.signal.clone()
            )),
            "conflicting AF route"
        );
        let peripheral = core
            .peripherals
            .iter_mut()
            .find(|p| p.name == route.peripheral)
            .context("AF peripheral missing")?;
        // Die routes may be valid but unbonded on this exact physical package.
        if pins.contains(&route.pin) {
            peripheral.pins.push(chip::core::peripheral::Pin {
                pin: route.pin.clone(),
                signal: route.signal.clone(),
                af: Some(route.af),
                adc_mux: None,
                comparator_mux: None,
            });
            projected += 1;
        }
    }
    for peripheral in &mut core.peripherals {
        peripheral
            .pins
            .sort_by(|a, b| (&a.pin, &a.signal, a.af).cmp(&(&b.pin, &b.signal, b.af)));
    }
    Ok(projected)
}

/// Project reviewed ADC analog routes; unlike digital AFs they have no selector.
pub fn apply_analog(
    root: &Path,
    path: &str,
    profile: &str,
    core: &mut chip::Core,
) -> Result<usize> {
    if matches!(profile, "CW32L010" | "CW32L011") {
        return low_adc::apply(root, path, profile, core);
    }
    if profile == "CW32L012" {
        return l012_adc::apply(root, path, profile, core);
    }
    if profile == "CW32F020" {
        return f020::apply(root, path, profile, "analog", core, None);
    }
    if matches!(
        profile,
        "CW32F002" | "CW32F003" | "CW32L031" | "CW32R031" | "CW32W031" | "CW32L052" | "CW32L083"
    ) {
        return classic_adc::apply(root, path, profile, core);
    }
    ensure!(
        matches!(profile, "CW32F030" | "CW32A030"),
        "unreviewed analog family"
    );
    let input: serde_json::Value = crate::read_yaml(root.join(path))?;
    ensure!(
        input["schema_version"] == 1
            && input["status"] == "verified-sdk-and-datasheets-and-reference-manuals",
        "analog metadata not reviewed"
    );
    ensure!(
        input["profiles"]
            .as_array()
            .context("analog profiles missing")?
            .iter()
            .any(|p| p == profile),
        "analog profile mismatch"
    );
    let sources = input["sources"]
        .as_array()
        .context("analog evidence missing")?;
    ensure!(
        !sources.is_empty()
            && sources
                .iter()
                .all(|s| s["sha256"].as_str().is_some_and(|h| h.len() == 64)
                    && s["section"].as_str().is_some_and(|s| !s.is_empty())),
        "analog evidence invalid"
    );
    let pins: BTreeSet<_> = core.pins.iter().map(|p| p.name.clone()).collect();
    let mut channels = BTreeSet::new();
    let mut pads = BTreeSet::new();
    let mut count = 0;
    for route in input["routes"]
        .as_array()
        .context("analog routes missing")?
    {
        ensure!(
            route["peripheral"] == "ADC" && route["af"].is_null(),
            "unexpected analog peripheral/AF"
        );
        let channel = route["channel"]
            .as_u64()
            .context("analog channel missing")?;
        let pin = route["pin"].as_str().context("analog pin missing")?;
        let signal = route["signal"].as_str().context("analog signal missing")?;
        ensure!(
            channel <= 12
                && signal == format!("IN{channel}")
                && route["mux"] == channel
                && route["source_signal"] == format!("ADC_IN{channel}")
                && channels.insert(channel)
                && pads.insert(pin),
            "invalid/duplicate analog channel"
        );
        if pins.contains(pin) {
            let adc = core
                .peripherals
                .iter_mut()
                .find(|p| p.name == "ADC")
                .context("ADC absent")?;
            adc.pins.push(chip::core::peripheral::Pin {
                pin: pin.into(),
                signal: signal.into(),
                af: None,
                adc_mux: Some(channel as u8),
                comparator_mux: None,
            });
            count += 1;
        }
    }
    ensure!(
        channels.len() == 13,
        "reviewed x030 analog map must cover13 external channels"
    );
    for p in &mut core.peripherals {
        p.pins
            .sort_by(|a, b| (&a.pin, &a.signal, a.af).cmp(&(&b.pin, &b.signal, b.af)));
    }
    Ok(count)
}

#[cfg(test)]
mod tests;

/// Apply only independently qualified PWM output routes, separate from serial AFs.
pub fn apply_pwm(
    root: &Path,
    path: &str,
    profile: &str,
    core: &mut chip::Core,
    registers: &BTreeMap<String, ir::IR>,
) -> Result<usize> {
    if profile == "CW32F020" {
        return f020::apply(root, path, profile, "pwm", core, Some(registers));
    }
    if matches!(profile, "CW32L010" | "CW32L011" | "CW32L012") {
        return buffered_pwm::apply(root, path, profile, core, registers);
    }
    classic_pwm::apply(root, path, profile, core, registers)
}

/// Project only independently qualified main-output ATIM routes.
pub fn apply_atim_pwm(
    root: &Path,
    path: &str,
    profile: &str,
    core: &mut chip::Core,
    registers: &BTreeMap<String, ir::IR>,
) -> Result<usize> {
    atim_pwm::apply(root, path, profile, core, registers)
}
