//! Own-family single-slot sequence ADC routing. Hardware muxes are explicit.
use super::*;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
mod profiles;

struct Spec {
    profile: &'static str,
    version: &'static str,
    manual_page: u64,
    datasheet_page_offset: u64,
    sdk_comment_column: u64,
    internal_macro_start: u64,
    sources: &'static [(&'static str, &'static str, &'static str)],
    // Pin, hardware mux, SDK macro line, own-family SDK comment line.
    routes: &'static [(&'static str, u8, u64, u64)],
}
impl Spec {
    fn source_signal(&self, channel: usize) -> String {
        format!("ADC_IN{channel}")
    }
    fn source(&self, key: &str) -> (&str, &str) {
        let (_, path, hash) = self.sources.iter().find(|s| s.0 == key).unwrap();
        (path, hash)
    }
}
fn array<'a>(value: &'a Value, field: &str) -> Result<&'a Vec<Value>> {
    value[field]
        .as_array()
        .with_context(|| format!("low ADC {field} missing"))
}
fn validate(input: &Value, spec: &Spec, pinouts: &Value) -> Result<()> {
    ensure!(
        input["schema_version"] == 1
            && input["profile"] == spec.profile
            && input["kind"] == "analog"
            && input["register_version"] == spec.version,
        "low ADC profile/kind/register mismatch"
    );
    ensure!(
        input["status"] == "verified-sdk-datasheet-reference-manual"
            && input["alias_pin_policy"] == "common-package-intersection",
        "low ADC route review/policy missing"
    );
    for (key, file, hash) in spec.sources {
        ensure!(
            input["sources"][key]["file"] == *file && input["sources"][key]["sha256"] == *hash,
            "low ADC qualified source changed: {key}"
        );
    }
    let (_, datasheet) = spec.source("datasheet");
    let (_, manual) = spec.source("reference_manual");
    let (_, header) = spec.source("adc_header");
    ensure!(
        pinouts["family"] == spec.profile
            && pinouts["source"]["sha256"] == datasheet
            && pinouts["status"] == "verified-from-official-datasheet",
        "low ADC own pinout source mismatch"
    );
    let internal = array(input, "internal_sources")?;
    ensure!(
        internal.len() == 2,
        "low ADC internal source count mismatch"
    );
    for (index, (signal, source_macro, source_signal)) in [
        ("TEMPERATURE", "ADC_InputTs", "TS 内置温度传感器"),
        ("BGR1P2", "ADC_InputVref1P2", "1.2V 内核电压基准源"),
    ]
    .into_iter()
    .enumerate()
    {
        let mux = index + 14;
        ensure!(
            crate::without_source_aliases(&internal[index])
                == json!({"signal":signal,"mux":mux,
            "source_macro":source_macro,"source_line":spec.internal_macro_start+index as u64,
            "source_sha256":header,"manual_cell":{"table":"20-4",
            "pdf_page":spec.manual_page,"printed_page":spec.manual_page-1,
            "mux_bits":format!("{mux:04b}"),"source_signal":source_signal,
            "gpio":"-","sha256":manual}}),
            "low ADC internal source provenance mismatch"
        );
    }
    let routes = array(input, "routes")?;
    ensure!(
        routes.len() == spec.routes.len(),
        "low ADC route count mismatch"
    );
    let mut seen_pins = BTreeSet::new();
    let mut seen_muxes = BTreeSet::new();
    for (channel, (route, &(pin, mux, macro_line, comment_line))) in
        routes.iter().zip(spec.routes).enumerate()
    {
        ensure!(
            route["peripheral"] == "ADC"
                && route["af"].is_null()
                && route["pin"] == pin
                && route["signal"] == format!("IN{channel}")
                && route["source_signal"] == spec.source_signal(channel)
                && route["channel"] == channel
                && route["mux"] == mux
                && seen_pins.insert(pin)
                && seen_muxes.insert(mux),
            "low ADC pin/source-channel/hardware-mux mismatch"
        );
        ensure!(
            route["source_macro"] == format!("ADC_InputCH{channel}")
                && route["source_line"] == macro_line
                && route["sdk_pin_comment_line"] == comment_line
                && route["sdk_pin_comment_column"] == spec.sdk_comment_column
                && route["sdk_comment_pin"] == pin
                && route["source_sha256"] == header
                && route["source_kind"] == "sdk-mux-and-datasheet-reference-manual",
            "low ADC SDK provenance mismatch"
        );
        ensure!(
            crate::without_source_aliases(&route["manual_cell"])
                == json!({"table":"20-4","pdf_page":spec.manual_page,
            "printed_page":spec.manual_page-1,"source_signal":spec.source_signal(channel),
            "mux_bits":format!("{mux:04b}"),"pin":pin,"sha256":manual}),
            "low ADC manual mux evidence mismatch"
        );
        let row = array(pinouts, "table_rows")?
            .iter()
            .find(|r| {
                r["signals"]
                    .as_array()
                    .is_some_and(|s| s.iter().any(|s| s == pin))
            })
            .context("low ADC pin absent from own datasheet")?;
        let signals = array(row, "signals")?;
        ensure!(
            row["pin_type"] == "I/O"
                && !signals.iter().any(|s| ["SWCLK", "SWDIO", "NRST", "BOOT"]
                    .iter()
                    .any(|bad| s == bad)),
            "low ADC debug/reset/input-only route forbidden"
        );
        let page = row["pdf_page_index"]
            .as_u64()
            .context("low ADC pin page missing")?;
        ensure!(
            crate::without_source_aliases(&route["pin_cell"])
                == json!({"pin":pin,"source_name":row["source_name"],
            "source_signal":spec.source_signal(channel),"table":"5-2","pdf_page":page+1,
            "printed_page":page+1-spec.datasheet_page_offset,"positions":row["positions"],"pin_type":row["pin_type"],"sha256":datasheet}),
            "low ADC datasheet pin/source-signal provenance mismatch"
        );
        let mut packages = serde_json::Map::new();
        for package in array(pinouts, "packages")? {
            let name = package["name"]
                .as_str()
                .context("low ADC package name missing")?;
            let column = package["table_column"]
                .as_str()
                .context("low ADC package column missing")?;
            let position = &row["positions"][column];
            let bonded = array(package, "pins")?.iter().any(|p| {
                p["signals"]
                    .as_array()
                    .is_some_and(|s| s.iter().any(|s| s == pin))
            });
            ensure!(
                bonded == !position.is_null(),
                "low ADC package bonding mismatch"
            );
            if !position.is_null() {
                ensure!(
                    array(package, "gpio_pins_preserving_swd")?
                        .iter()
                        .any(|s| s == pin)
                        && array(package, "pins")?
                            .iter()
                            .any(|p| p["position"] == *position
                                && p["signals"]
                                    .as_array()
                                    .is_some_and(|s| s.iter().any(|s| s == pin))),
                    "low ADC package route unbonded or restricted"
                );
            }
            packages.insert(name.into(), position.clone());
        }
        ensure!(
            route["package_pins"] == Value::Object(packages),
            "low ADC package positions mismatch"
        );
        let oscillator: Vec<_> = signals
            .iter()
            .filter(|s| s.as_str().is_some_and(|s| s.starts_with("OSC")))
            .cloned()
            .collect();
        ensure!(
            route["oscillator_aliases"] == json!(oscillator),
            "low ADC oscillator evidence mismatch"
        );
    }
    Ok(())
}

pub(super) fn apply(
    root: &Path,
    path: &str,
    profile: &str,
    core: &mut chip::Core,
) -> Result<usize> {
    let spec = profiles::spec(profile).context("unreviewed low ADC family")?;
    ensure!(
        path == format!("cw32-data/af/{}-analog.yaml", profile.to_lowercase()),
        "low ADC sidecar path mismatch"
    );
    let input: Value = crate::read_yaml(root.join(path))?;
    let (pinout_path, pinout_hash) = spec.source("pinouts");
    let bytes = fs::read(root.join(pinout_path))?;
    ensure!(
        format!("{:x}", Sha256::digest(&bytes)) == pinout_hash,
        "low ADC pinned pinout changed"
    );
    let pinouts: Value = crate::parse_yaml(Path::new(pinout_path), &bytes)?;
    validate(&input, &spec, &pinouts)?;
    let pins: BTreeSet<_> = core.pins.iter().map(|p| p.name.as_str()).collect();
    let package_sets: Vec<BTreeSet<_>> = array(&pinouts, "packages")?
        .iter()
        .map(|p| {
            p["pins"]
                .as_array()
                .unwrap()
                .iter()
                .flat_map(|p| p["signals"].as_array().unwrap())
                .filter_map(|s| s.as_str())
                .filter(|s| crate::pinouts::is_gpio(s))
                .collect()
        })
        .collect();
    let common = package_sets
        .iter()
        .skip(1)
        .fold(package_sets[0].clone(), |a, b| {
            a.intersection(b).copied().collect()
        });
    ensure!(
        package_sets.contains(&pins) || pins == common,
        "low ADC requires exact-package or common-intersection pins"
    );
    let adc = core
        .peripherals
        .iter()
        .find(|p| p.name == "ADC")
        .context("low ADC absent")?;
    let reference = adc
        .registers
        .as_ref()
        .context("low ADC register identity absent")?;
    ensure!(
        reference.kind == "adc" && reference.version == spec.version && reference.block == "ADC",
        "low ADC register identity differs"
    );
    ensure!(adc.pins.is_empty(), "low ADC routes already projected");
    let mut projected = core.clone();
    let adc = projected
        .peripherals
        .iter_mut()
        .find(|p| p.name == "ADC")
        .unwrap();
    for route in array(&input, "routes")? {
        let pin = route["pin"].as_str().unwrap();
        if pins.contains(pin) {
            adc.pins.push(chip::core::peripheral::Pin {
                pin: pin.into(),
                signal: route["signal"].as_str().unwrap().into(),
                af: None,
                adc_mux: Some(route["mux"].as_u64().unwrap() as u8),
                comparator_mux: None,
            });
        }
    }
    let count = adc.pins.len();
    ensure!(count > 0, "low ADC empty package projection");
    adc.pins
        .sort_by(|a, b| (&a.pin, &a.signal).cmp(&(&b.pin, &b.signal)));
    *core = projected;
    Ok(count)
}
#[cfg(test)]
mod tests;
