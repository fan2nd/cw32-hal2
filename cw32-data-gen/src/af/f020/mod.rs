//! F020-only, source-qualified ADC mux and GTIM CH1–4 route projection.
//! Source signal labels are retained separately from hardware mux encodings.
use super::*;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const DATASHEET: &str = "1e330d800f10114c654b97cbd45b64d56a99c3cb39138d4ef20de3c12968dab0";
const MANUAL: &str = "279521eaf7d32b241e614a9553044a4d99c1e7ca2566efa3b0fdd2b66c106eed";
const ADC_HEADER: &str = "e3b36174dea21de3d8d74c248c18c59450056728dbebce25751a55c92f3dd0cf";
const GPIO_HEADER: &str = "eccc3bb68452d2e3218397b2a795c4330e4a3d9b5a76481de128655e4602874d";
const SDK: &str = "1d77fece47a0c615c8ae51374ea946b17ab489042222f33e38d93e6969945b5d";
const ANALOG_PINS: [&str; 13] = [
    "PA0", "PA1", "PA2", "PA3", "PA4", "PA5", "PA6", "PA7", "PB0", "PB1", "PB2", "PB10", "PB11",
];

fn array<'a>(value: &'a Value, field: &str) -> Result<&'a Vec<Value>> {
    value[field]
        .as_array()
        .with_context(|| format!("F020 {field} missing"))
}

fn pinned_yaml(root: &Path, source: &Value, expected_path: &str) -> Result<Value> {
    ensure!(source["file"] == expected_path, "F020 source path mismatch");
    let bytes = fs::read(root.join(expected_path))?;
    ensure!(
        source["sha256"] == format!("{:x}", Sha256::digest(&bytes)),
        "F020 source hash mismatch"
    );
    crate::parse_yaml(Path::new(expected_path), &bytes)
}

fn validate_sources(input: &Value, kind: &str) -> Result<()> {
    ensure!(
        input["schema_version"] == 1 && input["profile"] == "CW32F020" && input["kind"] == kind,
        "F020 route profile/kind mismatch"
    );
    ensure!(
        input["status"]
            == if kind == "analog" {
                "verified-sdk-datasheet-reference-manual"
            } else {
                "verified-sdk-and-datasheet"
            },
        "F020 routes not reviewed"
    );
    ensure!(
        input["alias_pin_policy"] == "common-package-intersection",
        "F020 alias policy mismatch"
    );
    for (key, file, hash) in [
        (
            "datasheet",
            "current-datasheets/CW32F020_DataSheet_CN_V1.3.pdf",
            DATASHEET,
        ),
        (
            "reference_manual",
            "CW32F020_UserManual_CN_V1.4.pdf",
            MANUAL,
        ),
        (
            "sdk_archive",
            "CW32F020_StandardPeripheralLib_V1.2.zip",
            SDK,
        ),
        (
            "adc_header",
            "cw32f020/Libraries/inc/cw32f020_adc.h",
            ADC_HEADER,
        ),
        (
            "gpio_header",
            "cw32f020/Libraries/inc/cw32f020_gpio.h",
            GPIO_HEADER,
        ),
    ] {
        ensure!(
            input["sources"][key]["file"] == file && input["sources"][key]["sha256"] == hash,
            "F020 qualified source changed: {key}"
        );
    }
    if kind == "pwm" {
        ensure!(
            input["datasheet"]["sha256"] == DATASHEET,
            "F020 PWM datasheet mismatch"
        );
    }
    Ok(())
}

fn validate_pin(route: &Value, pinouts: &Value) -> Result<()> {
    let pin = route["pin"].as_str().context("F020 route pin missing")?;
    let row = array(pinouts, "table_rows")?
        .iter()
        .find(|r| {
            r["signals"]
                .as_array()
                .is_some_and(|signals| signals.iter().any(|s| s == pin))
        })
        .context("F020 route pin absent from own datasheet")?;
    let signals = array(row, "signals")?;
    ensure!(
        row["pin_type"] == "I/O"
            && !signals.iter().any(|s| ["SWCLK", "SWDIO", "NRST", "BOOT"]
                .iter()
                .any(|bad| s == bad)),
        "F020 debug/reset/input-only route forbidden"
    );
    let page = row["pdf_page_index"]
        .as_u64()
        .context("F020 pin page missing")?;
    let expected_cell = json!({"pin":pin,"source_name":row["source_name"],"table":"5-2",
        "pdf_page":page+1,"printed_page":page,"positions":row["positions"],
        "pin_type":row["pin_type"],"sha256":DATASHEET});
    ensure!(
        crate::without_source_aliases(&route["pin_cell"]) == expected_cell,
        "F020 pin provenance mismatch"
    );
    let mut package_pins = serde_json::Map::new();
    for package in array(pinouts, "packages")? {
        let name = package["name"]
            .as_str()
            .context("F020 package name missing")?;
        let column = package["table_column"]
            .as_str()
            .context("F020 package column missing")?;
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
                "F020 package route unbonded or restricted"
            );
        }
        package_pins.insert(name.into(), position.clone());
    }
    ensure!(
        route["package_pins"] == Value::Object(package_pins),
        "F020 route package positions mismatch"
    );
    let oscillator: Vec<_> = signals
        .iter()
        .filter(|s| s.as_str().is_some_and(|s| s.starts_with("OSC")))
        .cloned()
        .collect();
    ensure!(
        route["oscillator_aliases"] == json!(oscillator),
        "F020 oscillator ownership evidence mismatch"
    );
    Ok(())
}

fn validate(input: &Value, kind: &str, pinouts: &Value, candidates: &Value) -> Result<()> {
    validate_sources(input, kind)?;
    ensure!(
        pinouts["family"] == "CW32F020" && pinouts["source"]["sha256"] == DATASHEET,
        "F020 own-package source mismatch"
    );
    ensure!(
        candidates["profile"] == "CW32F020"
            && candidates["source"]["header_sha256"] == GPIO_HEADER
            && candidates["status"] == "candidate-sdk-and-register-verified",
        "F020 SDK candidate source mismatch"
    );
    let routes = array(input, "routes")?;
    ensure!(
        routes.len() == if kind == "analog" { 13 } else { 46 },
        "F020 route count mismatch"
    );
    let mut seen = BTreeSet::new();
    for route in routes {
        validate_pin(route, pinouts)?;
        if kind == "analog" {
            let mux = route["mux"]
                .as_u64()
                .context("F020 ADC hardware mux missing")?;
            ensure!(
                mux < 13
                    && route["pin"] == ANALOG_PINS[mux as usize]
                    && route["channel"] == mux
                    && route["peripheral"] == "ADC"
                    && route["af"].is_null()
                    && route["signal"] == format!("IN{mux}")
                    && route["source_signal"] == format!("ADC_IN{mux}")
                    && seen.insert(format!("{mux}")),
                "F020 ADC pin/channel/mux mismatch"
            );
            ensure!(
                route["source_macro"] == format!("ADC_ExInputCH{mux}")
                    && route["source_line"] == 206 + mux
                    && route["sdk_pin_comment_line"] == 189 + mux
                    && route["source_sha256"] == ADC_HEADER,
                "F020 ADC SDK provenance mismatch"
            );
            ensure!(
                crate::without_source_aliases(&route["manual_cell"])
                    == json!({"table":"21-5","pdf_page":378,"printed_page":377,
                "source_signal":format!("AIN{mux}"),"mux_bits":format!("{mux:04b}"),"pin":route["pin"],"sha256":MANUAL}),
                "F020 ADC manual mux evidence mismatch"
            );
        } else {
            let peripheral = route["peripheral"]
                .as_str()
                .context("F020 PWM peripheral missing")?;
            let signal = route["signal"]
                .as_str()
                .context("F020 PWM signal missing")?;
            let pin = route["pin"].as_str().context("F020 PWM pin missing")?;
            ensure!(
                ["GTIM1", "GTIM2", "GTIM3", "GTIM4"].contains(&peripheral)
                    && ["CH1", "CH2", "CH3", "CH4"].contains(&signal)
                    && route["source_signal"] == signal
                    && route["channel"] == u64::from(signal.as_bytes()[2] - b'0')
                    && route["source_kind"] == "sdk-and-datasheet"
                    && route["source_sha256"] == GPIO_HEADER,
                "F020 unsupported PWM peripheral/signal"
            );
            let candidate = array(candidates, "routes")?
                .iter()
                .find(|r| r["pin"] == route["pin"] && r["af"] == route["af"])
                .context("F020 PWM SDK coordinate missing")?;
            for key in [
                "pin",
                "af",
                "function",
                "source_macro",
                "source_line",
                "gpio_register",
                "gpio_field",
                "peripheral",
                "signal",
            ] {
                ensure!(
                    route[key] == candidate[key],
                    "F020 PWM SDK tuple mismatch: {key}"
                );
            }
            ensure!(
                seen.insert(format!("{pin}:{peripheral}:{signal}")),
                "duplicate F020 PWM route"
            );
            let (table, page) = match pin.as_bytes().get(1) {
                Some(b'A') => ("5-3", 26),
                Some(b'B') => ("5-4", 27),
                Some(b'C') => ("5-5", 27),
                Some(b'F') => ("5-6", 27),
                _ => anyhow::bail!("F020 PWM invalid port"),
            };
            ensure!(
                crate::without_source_aliases(&route["datasheet_cell"])
                    == json!({"pin":pin,"af":route["af"],"table":table,
                "pdf_page":page+1,"printed_page":page,"function":format!("{peripheral}_{signal}"),"sha256":DATASHEET}),
                "F020 PWM datasheet tuple mismatch"
            );
        }
    }
    for (part, count) in [
        ("CW32F020F6U7", if kind == "analog" { 9 } else { 17 }),
        ("CW32F020K6U7", if kind == "analog" { 11 } else { 32 }),
        ("CW32F020C6U7", if kind == "analog" { 13 } else { 46 }),
    ] {
        ensure!(
            routes
                .iter()
                .filter(|r| !r["package_pins"][part].is_null())
                .count()
                == count,
            "F020 exact-package route count mismatch"
        );
    }
    Ok(())
}

pub(super) fn apply(
    root: &Path,
    path: &str,
    profile: &str,
    kind: &str,
    core: &mut chip::Core,
    registers: Option<&BTreeMap<String, ir::IR>>,
) -> Result<usize> {
    ensure!(
        profile == "CW32F020" && ["analog", "pwm"].contains(&kind),
        "unqualified analog/PWM profile"
    );
    let input: Value = crate::read_yaml(root.join(path))?;
    let pinouts = pinned_yaml(
        root,
        &input["sources"]["pinouts"],
        "cw32-data/pinouts/cw32f020.yaml",
    )?;
    let candidates = pinned_yaml(
        root,
        &input["sources"]["sdk_candidates"],
        "cw32-data/af/cw32f020.yaml",
    )?;
    validate(&input, kind, &pinouts, &candidates)?;
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
        "F020 routes require exact-package or common-intersection pins"
    );
    for name in if kind == "analog" {
        vec!["ADC"]
    } else {
        vec!["GTIM1", "GTIM2", "GTIM3", "GTIM4"]
    } {
        let peripheral = core
            .peripherals
            .iter()
            .find(|p| p.name == name)
            .context("F020 peripheral absent")?;
        let reference = peripheral
            .registers
            .as_ref()
            .context("F020 register identity missing")?;
        ensure!(
            reference.kind == if kind == "analog" { "adc" } else { "gtim" }
                && reference.version
                    == if kind == "analog" {
                        "cw32f020_v1"
                    } else {
                        "v1"
                    },
            "F020 analog/PWM register identity differs"
        );
        ensure!(
            peripheral.pins.is_empty(),
            "F020 analog/PWM already projected"
        );
    }
    let mut projected = core.clone();
    let count = if kind == "pwm" {
        super::project(
            &serde_json::from_value(input.clone())?,
            profile,
            &mut projected,
            registers.context("F020 PWM needs actual GPIO registers")?,
        )?
    } else {
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
        adc.pins.len()
    };
    let expected = array(&input, "routes")?
        .iter()
        .filter(|r| pins.contains(r["pin"].as_str().unwrap()))
        .count();
    ensure!(
        count == expected && count > 0,
        "F020 route projection mismatch"
    );
    for p in &mut projected.peripherals {
        p.pins
            .sort_by(|a, b| (&a.pin, &a.signal, a.af).cmp(&(&b.pin, &b.signal, b.af)));
    }
    *core = projected;
    Ok(count)
}

#[cfg(test)]
mod tests;
