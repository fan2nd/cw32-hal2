//! CW32L012 dual-ADC routes, qualified from its own manual, datasheet and SDK.
//! A pad can belong to both ADCs, with a different mux for each instance.
use super::*;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
mod sources;

fn array<'a>(value: &'a Value, field: &str) -> Result<&'a Vec<Value>> {
    value[field]
        .as_array()
        .with_context(|| format!("L012 ADC {field} missing"))
}

fn validate(input: &Value, pinouts: &Value) -> Result<()> {
    ensure!(
        input["schema_version"] == 1
            && input["profile"] == "CW32L012"
            && input["kind"] == "analog"
            && input["register_version"] == "cw32l012_v1",
        "L012 ADC profile/kind/register mismatch"
    );
    ensure!(
        input["status"] == "verified-sdk-datasheet-reference-manual"
            && input["alias_pin_policy"] == "common-package-intersection",
        "L012 ADC route review/policy missing"
    );
    for &(key, file, hash) in sources::SOURCES {
        ensure!(
            input["sources"][key]["file"] == file && input["sources"][key]["sha256"] == hash,
            "L012 ADC qualified source changed: {key}"
        );
    }
    ensure!(
        pinouts["family"] == "CW32L012"
            && pinouts["source"]["sha256"] == sources::source("datasheet").1
            && pinouts["status"] == "verified-from-official-datasheet",
        "L012 ADC own pinout source mismatch"
    );
    let routes = array(input, "routes")?;
    ensure!(
        routes.len() == 24,
        "L012 ADC requires 12 external routes per ADC"
    );
    let mut seen_pins = BTreeSet::new();
    let mut seen_muxes = BTreeSet::new();
    for (index, route) in routes.iter().enumerate() {
        let instance = index / 12;
        let channel = index % 12;
        let (peripheral, pins, comment_start) = sources::ROUTES[instance];
        let pin = pins[channel];
        let source_signal = format!("{peripheral}_IN{channel}");
        ensure!(
            route["peripheral"] == peripheral
                && route["af"].is_null()
                && route["pin"] == pin
                && route["signal"] == format!("IN{channel}")
                && route["source_signal"] == source_signal
                && route["channel"] == channel
                && route["mux"] == channel
                && seen_pins.insert((peripheral, pin))
                && seen_muxes.insert((peripheral, channel)),
            "L012 ADC instance/pin/source-channel/hardware-mux mismatch"
        );
        ensure!(
            route["source_macro"] == format!("ADC_InputCH{channel}")
                && route["source_line"] == 222 + channel
                && route["sdk_pin_comment_line"] == comment_start + channel
                && route["sdk_comment_pin"] == pin
                && route["source_sha256"] == sources::source("adc_header").1
                && route["source_kind"] == "sdk-mux-and-datasheet-reference-manual",
            "L012 ADC SDK provenance mismatch"
        );
        ensure!(
            crate::without_source_aliases(&route["manual_cell"])
                == json!({"table":"25-4","pdf_page":604,
                "printed_page":578,"instance":peripheral,
                "source_signal":format!("ADCx_IN{channel}"),
                "mux_bits":format!("{channel:04b}"),"pin":pin,
                "sha256":sources::source("reference_manual").1}),
            "L012 ADC manual mux evidence mismatch"
        );
        let row = array(pinouts, "table_rows")?
            .iter()
            .find(|r| {
                r["signals"]
                    .as_array()
                    .is_some_and(|s| s.iter().any(|s| s == pin))
            })
            .context("L012 ADC pin absent from own datasheet")?;
        let signals = array(row, "signals")?;
        ensure!(
            row["pin_type"] == "I/O"
                && !signals.iter().any(|s| ["SWCLK", "SWDIO", "NRST", "BOOT"]
                    .iter()
                    .any(|bad| s == bad)),
            "L012 ADC debug/reset/input-only route forbidden"
        );
        let page = row["pdf_page_index"]
            .as_u64()
            .context("L012 ADC pin page missing")?;
        ensure!(page >= 3, "L012 ADC invalid pin page");
        ensure!(
            crate::without_source_aliases(&route["pin_cell"])
                == json!({"pin":pin,"source_name":row["source_name"],
                "source_signal":source_signal,"table":"5-2","pdf_page":page+1,
                "printed_page":page-2,"positions":row["positions"],"pin_type":row["pin_type"],
                "sha256":sources::source("datasheet").1}),
            "L012 ADC datasheet pin/source-signal provenance mismatch"
        );
        let mut packages = serde_json::Map::new();
        for package in array(pinouts, "packages")? {
            let name = package["name"]
                .as_str()
                .context("L012 ADC package name missing")?;
            let column = package["table_column"]
                .as_str()
                .context("L012 ADC package column missing")?;
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
                    "L012 ADC package route unbonded or restricted"
                );
            }
            packages.insert(name.into(), position.clone());
        }
        ensure!(
            route["package_pins"] == Value::Object(packages),
            "L012 ADC package positions mismatch"
        );
        let oscillator: Vec<_> = signals
            .iter()
            .filter(|s| s.as_str().is_some_and(|s| s.starts_with("OSC")))
            .cloned()
            .collect();
        ensure!(
            route["oscillator_aliases"] == json!(oscillator),
            "L012 ADC oscillator evidence mismatch"
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
    ensure!(
        profile == "CW32L012" && path == "cw32-data/af/cw32l012-analog.yaml",
        "L012 ADC sidecar path/profile mismatch"
    );
    let input: Value = crate::read_yaml(root.join(path))?;
    let (pinout_path, pinout_hash) = sources::source("pinouts");
    let bytes = fs::read(root.join(pinout_path))?;
    ensure!(
        format!("{:x}", Sha256::digest(&bytes)) == pinout_hash,
        "L012 ADC pinned pinout changed"
    );
    let pinouts: Value = crate::parse_yaml(Path::new(pinout_path), &bytes)?;
    validate(&input, &pinouts)?;
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
        pins.len() == core.pins.len() && (package_sets.contains(&pins) || pins == common),
        "L012 ADC requires exact-package or common-intersection pins"
    );
    for (name, address) in [("ADC1", 0x4000_0000), ("ADC2", 0x4000_0100)] {
        let mut instances = core.peripherals.iter().filter(|p| p.name == name);
        let adc = instances.next().context("L012 ADC instance absent")?;
        ensure!(
            instances.next().is_none() && adc.address == address,
            "L012 ADC instance identity differs"
        );
        let reference = adc
            .registers
            .as_ref()
            .context("L012 ADC register identity absent")?;
        ensure!(
            reference.kind == "adc"
                && reference.version == "cw32l012_v1"
                && reference.block == "ADC",
            "L012 ADC register identity differs"
        );
        ensure!(adc.pins.is_empty(), "L012 ADC routes already projected");
    }
    // Commit only after both ADC identities and all package/source checks pass.
    let mut projected = core.clone();
    let mut count = 0;
    for route in array(&input, "routes")? {
        let pin = route["pin"].as_str().unwrap();
        if pins.contains(pin) {
            let adc = projected
                .peripherals
                .iter_mut()
                .find(|p| p.name == route["peripheral"])
                .unwrap();
            adc.pins.push(chip::core::peripheral::Pin {
                pin: pin.into(),
                signal: route["signal"].as_str().unwrap().into(),
                af: None,
                adc_mux: Some(route["mux"].as_u64().unwrap() as u8),
                comparator_mux: None,
            });
            count += 1;
        }
    }
    ensure!(count > 0, "L012 ADC empty package projection");
    for adc in projected
        .peripherals
        .iter_mut()
        .filter(|p| matches!(p.name.as_str(), "ADC1" | "ADC2"))
    {
        adc.pins
            .sort_by(|a, b| (&a.pin, &a.signal).cmp(&(&b.pin, &b.signal)));
    }
    *core = projected;
    Ok(count)
}

#[cfg(test)]
mod tests;
