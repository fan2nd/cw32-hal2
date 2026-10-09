//! Own-family classic ADC routing. Labels are never parsed into hardware muxes.
use super::*;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
mod profiles;

struct Spec {
    profile: &'static str,
    version: &'static str,
    manual_table: &'static str,
    manual_page: u64,
    sources: &'static [(&'static str, &'static str, &'static str)],
    // Pin, hardware mux, SDK macro line, SDK comment line, SDK comment pin.
    routes: &'static [(&'static str, u8, u64, u64, &'static str)],
}
impl Spec {
    fn source_signal(&self, channel: usize) -> String {
        let prefix = if matches!(
            self.profile,
            "CW32F002" | "CW32F003" | "CW32L052" | "CW32L083"
        ) {
            "ADC_AIN"
        } else {
            "ADC_IN"
        };
        format!("{prefix}{channel}")
    }
    fn source(&self, key: &str) -> (&str, &str) {
        let (_, path, hash) = self.sources.iter().find(|s| s.0 == key).unwrap();
        (path, hash)
    }
}
fn array<'a>(value: &'a Value, field: &str) -> Result<&'a Vec<Value>> {
    value[field]
        .as_array()
        .with_context(|| format!("classic ADC {field} missing"))
}
fn validate(input: &Value, spec: &Spec, pinouts: &Value) -> Result<()> {
    ensure!(
        input["schema_version"] == 1
            && input["profile"] == spec.profile
            && input["kind"] == "analog"
            && input["register_version"] == spec.version,
        "classic ADC profile/kind/register mismatch"
    );
    ensure!(
        input["status"] == "verified-sdk-datasheet-reference-manual"
            && input["alias_pin_policy"] == "common-package-intersection",
        "classic ADC route review/policy missing"
    );
    for (key, file, hash) in spec.sources {
        ensure!(
            input["sources"][key]["file"] == *file && input["sources"][key]["sha256"] == *hash,
            "classic ADC qualified source changed: {key}"
        );
    }
    let (_, datasheet) = spec.source("datasheet");
    let (_, manual) = spec.source("reference_manual");
    let (_, header) = spec.source("adc_header");
    ensure!(
        pinouts["family"] == spec.profile
            && pinouts["source"]["sha256"] == datasheet
            && pinouts["status"] == "verified-from-official-datasheet",
        "classic ADC own pinout source mismatch"
    );
    let routes = array(input, "routes")?;
    ensure!(
        routes.len() == spec.routes.len(),
        "classic ADC route count mismatch"
    );
    let mut seen_pins = BTreeSet::new();
    let mut seen_muxes = BTreeSet::new();
    for (channel, (route, &(pin, mux, macro_line, comment_line, comment_pin))) in
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
            "classic ADC pin/source-channel/hardware-mux mismatch"
        );
        ensure!(
            route["source_macro"] == format!("ADC_ExInputCH{channel}")
                && route["source_line"] == macro_line
                && route["sdk_pin_comment_line"] == comment_line
                && route["sdk_comment_pin"] == comment_pin
                && route["source_sha256"] == header
                && route["source_kind"]
                    == if pin == comment_pin {
                        "sdk-mux-and-datasheet-reference-manual"
                    } else {
                        "datasheet-reference-manual-corrected-sdk-pin-comment"
                    },
            "classic ADC SDK provenance mismatch"
        );
        ensure!(
            crate::without_source_aliases(&route["manual_cell"])
                == json!({"table":spec.manual_table,"pdf_page":spec.manual_page,
            "printed_page":spec.manual_page-1,"source_signal":format!("AIN{channel}"),
            "mux_bits":format!("{mux:04b}"),"pin":pin,"sha256":manual}),
            "classic ADC manual mux evidence mismatch"
        );
        let row = array(pinouts, "table_rows")?
            .iter()
            .find(|r| {
                r["signals"]
                    .as_array()
                    .is_some_and(|s| s.iter().any(|s| s == pin))
            })
            .context("classic ADC pin absent from own datasheet")?;
        let signals = array(row, "signals")?;
        ensure!(
            row["pin_type"] == "I/O"
                && !signals.iter().any(|s| ["SWCLK", "SWDIO", "NRST", "BOOT"]
                    .iter()
                    .any(|bad| s == bad)),
            "classic ADC debug/reset/input-only route forbidden"
        );
        let page = row["pdf_page_index"]
            .as_u64()
            .context("classic ADC pin page missing")?;
        ensure!(
            crate::without_source_aliases(&route["pin_cell"])
                == json!({"pin":pin,"source_name":row["source_name"],
            "source_signal":spec.source_signal(channel),"table":"5-2","pdf_page":page+1,
            "printed_page":page,"positions":row["positions"],"pin_type":row["pin_type"],"sha256":datasheet}),
            "classic ADC datasheet pin/source-signal provenance mismatch"
        );
        let mut packages = serde_json::Map::new();
        for package in array(pinouts, "packages")? {
            let name = package["name"]
                .as_str()
                .context("classic ADC package name missing")?;
            let column = package["table_column"]
                .as_str()
                .context("classic ADC package column missing")?;
            let position = &row["positions"][column];
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
                    "classic ADC package route unbonded or restricted"
                );
            }
            packages.insert(name.into(), position.clone());
        }
        ensure!(
            route["package_pins"] == Value::Object(packages),
            "classic ADC package positions mismatch"
        );
        let oscillator: Vec<_> = signals
            .iter()
            .filter(|s| s.as_str().is_some_and(|s| s.starts_with("OSC")))
            .cloned()
            .collect();
        ensure!(
            route["oscillator_aliases"] == json!(oscillator),
            "classic ADC oscillator evidence mismatch"
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
    let spec = profiles::spec(profile).context("unreviewed classic ADC family")?;
    ensure!(
        path == format!("cw32-data/af/{}-analog.yaml", profile.to_lowercase()),
        "classic ADC sidecar path mismatch"
    );
    let input: Value = crate::read_yaml(root.join(path))?;
    let (pinout_path, pinout_hash) = spec.source("pinouts");
    let bytes = fs::read(root.join(pinout_path))?;
    ensure!(
        format!("{:x}", Sha256::digest(&bytes)) == pinout_hash,
        "classic ADC pinned pinout changed"
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
        "classic ADC requires exact-package or common-intersection pins"
    );
    let adc = core
        .peripherals
        .iter()
        .find(|p| p.name == "ADC")
        .context("classic ADC absent")?;
    let reference = adc
        .registers
        .as_ref()
        .context("classic ADC register identity absent")?;
    ensure!(
        reference.kind == "adc" && reference.version == spec.version && reference.block == "ADC",
        "classic ADC register identity differs"
    );
    ensure!(adc.pins.is_empty(), "classic ADC routes already projected");
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
    ensure!(count > 0, "classic ADC empty package projection");
    adc.pins
        .sort_by(|a, b| (&a.pin, &a.signal).cmp(&(&b.pin, &b.signal)));
    *core = projected;
    Ok(count)
}
#[cfg(test)]
mod tests;
