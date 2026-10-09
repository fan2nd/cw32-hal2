//! Own-family, source-qualified CH1–4 output routes for buffered GTIM variants.
//! No general SDK candidate, oscillator pad, or input-only pad is promoted implicitly.
use super::*;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
mod profiles;

struct Spec {
    profile: &'static str,
    version: &'static str,
    peripherals: &'static [&'static str],
    sources: &'static [(&'static str, &'static str, &'static str)],
    route_count: usize,
    // Canonical, sorted-key UTF-8 JSON of the complete independently qualified
    // route array. This prevents adding a coherent but SDK-only route or
    // substituting sibling-family cells. The source verifier rebuilds the array
    // from the original own-family PDF and exact SDK macro assignments.
    routes_sha256: &'static str,
}

fn array<'a>(value: &'a Value, field: &str) -> Result<&'a Vec<Value>> {
    value[field]
        .as_array()
        .with_context(|| format!("buffered PWM {field} missing"))
}

fn source(spec: &Spec, key: &str) -> (&'static str, &'static str) {
    let (_, path, hash) = spec.sources.iter().find(|s| s.0 == key).unwrap();
    (path, hash)
}

fn pinned_yaml(root: &Path, spec: &Spec, key: &str) -> Result<Value> {
    let (path, hash) = source(spec, key);
    let bytes = fs::read(root.join(path))?;
    ensure!(
        format!("{:x}", Sha256::digest(&bytes)) == hash,
        "buffered PWM pinned {key} changed"
    );
    crate::parse_yaml(Path::new(path), &bytes)
}

fn validate(input: &Value, spec: &Spec, pinouts: &Value, candidates: &Value) -> Result<()> {
    ensure!(
        input["schema_version"] == 1
            && input["profile"] == spec.profile
            && input["kind"] == "pwm"
            && input["register_version"] == spec.version
            && input["status"] == "verified-sdk-and-datasheet"
            && input["alias_pin_policy"] == "common-package-intersection",
        "buffered PWM profile/register/review mismatch"
    );
    for &(key, file, hash) in spec.sources {
        ensure!(
            input["sources"][key]["file"] == file && input["sources"][key]["sha256"] == hash,
            "buffered PWM qualified source changed: {key}"
        );
    }
    let (_, datasheet) = source(spec, "datasheet");
    let (_, header) = source(spec, "gpio_header");
    ensure!(
        pinouts["family"] == spec.profile
            && pinouts["status"] == "verified-from-official-datasheet"
            && pinouts["source"]["sha256"] == datasheet
            && input["datasheet"]["sha256"] == datasheet,
        "buffered PWM own datasheet/pinout identity mismatch"
    );
    ensure!(
        candidates["profile"] == spec.profile
            && candidates["status"] == "candidate-sdk-and-register-verified"
            && candidates["source"]["header_sha256"] == header,
        "buffered PWM own SDK identity mismatch"
    );
    let routes = array(input, "routes")?;
    ensure!(
        routes.len() == spec.route_count
            && format!("{:x}", Sha256::digest(qualified_route_bytes(routes)?))
                == spec.routes_sha256,
        "buffered PWM qualified route set changed; independently requalify original sources"
    );
    for route in routes {
        let pin = route["pin"].as_str().context("buffered PWM pin missing")?;
        let peripheral = route["peripheral"]
            .as_str()
            .context("buffered PWM timer missing")?;
        let signal = route["signal"]
            .as_str()
            .context("buffered PWM channel missing")?;
        ensure!(
            spec.peripherals.contains(&peripheral)
                && ["CH1", "CH2", "CH3", "CH4"].contains(&signal)
                && route["source_signal"] == signal
                && route["source_kind"] == "sdk-and-datasheet"
                && route["source_sha256"] == header,
            "buffered PWM unsupported timer/channel"
        );
        let row = array(pinouts, "table_rows")?
            .iter()
            .find(|row| {
                row["signals"]
                    .as_array()
                    .is_some_and(|s| s.iter().any(|s| s == pin))
            })
            .context("buffered PWM pin absent from own pinout")?;
        let signals = array(row, "signals")?;
        ensure!(
            row["pin_type"] == "I/O"
                && !signals.iter().any(|s| s.as_str().is_some_and(|s| [
                    "SWDIO", "SWCLK", "NRST", "BOOT"
                ]
                .contains(&s)
                    || s.starts_with("OSC")))
                && route["oscillator_aliases"] == json!([]),
            "buffered PWM debug/reset/input-only/oscillator pin forbidden"
        );
        let candidate = array(candidates, "routes")?
            .iter()
            .chain(array(candidates, "unresolved")?.iter())
            .find(|r| r["pin"] == route["pin"] && r["af"] == route["af"])
            .context("buffered PWM SDK coordinate absent")?;
        for key in [
            "pin",
            "af",
            "function",
            "source_macro",
            "source_line",
            "gpio_register",
            "gpio_field",
        ] {
            ensure!(
                route[key] == candidate[key],
                "buffered PWM SDK tuple differs: {key}"
            );
        }
        // L010 names the sole 0x40001800 timer GTIM in its datasheet and
        // manual, GTIM1 in CMSIS/SVD. Only PA3 AF6's raw SDK macro omits "1".
        // Preserve the original label and qualify this exact exception only.
        let l010_pa3 = spec.profile == "CW32L010" && pin == "PA3";
        let source_peripheral = if l010_pa3 { "GTIM" } else { peripheral };
        ensure!(
            route["source_peripheral"] == source_peripheral
                && route["function"] == format!("{source_peripheral}{signal}"),
            "buffered PWM source instance mismatch"
        );
        if l010_pa3 {
            ensure!(
                route["af"] == 6
                    && peripheral == "GTIM1"
                    && signal == "CH4"
                    && route["source_macro"] == "PA03_AFx_GTIMCH4"
                    && candidate["peripheral"].is_null()
                    && candidate["signal"].is_null(),
                "buffered PWM L010 singleton alias changed"
            );
        } else {
            ensure!(
                route["peripheral"] == candidate["peripheral"]
                    && route["signal"] == candidate["signal"],
                "buffered PWM SDK instance/channel changed"
            );
        }
        let cell_peripheral = if spec.profile == "CW32L010" {
            "GTIM"
        } else {
            peripheral
        };
        let (_, manual) = source(spec, "reference_manual");
        ensure!(
            route["datasheet_cell"]["function"] == format!("{cell_peripheral}_{signal}")
                && route["manual_cell"]["function"] == route["datasheet_cell"]["function"]
                && route["manual_cell"]["pin"] == pin
                && route["manual_cell"]["af"] == route["af"]
                && route["manual_cell"]["sha256"] == manual,
            "buffered PWM own manual/datasheet channel mismatch"
        );
        for package in array(pinouts, "packages")? {
            let name = package["name"]
                .as_str()
                .context("buffered PWM package absent")?;
            let column = package["table_column"]
                .as_str()
                .context("buffered PWM column absent")?;
            let position = &row["positions"][column];
            ensure!(
                route["package_pins"][name] == *position,
                "buffered PWM package position differs"
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
                    "buffered PWM route is not a bonded safe output"
                );
            }
        }
    }
    Ok(())
}

pub(super) fn apply(
    root: &Path,
    path: &str,
    profile: &str,
    core: &mut chip::Core,
    registers: &BTreeMap<String, ir::IR>,
) -> Result<usize> {
    let spec = profiles::spec(profile).context("unreviewed buffered PWM family")?;
    ensure!(
        path == format!("cw32-data/af/{}-pwm.yaml", profile.to_lowercase()),
        "buffered PWM sidecar path mismatch"
    );
    let input: Value = crate::read_yaml(root.join(path))?;
    let pinouts = pinned_yaml(root, &spec, "pinouts")?;
    let candidates = pinned_yaml(root, &spec, "sdk_candidates")?;
    validate(&input, &spec, &pinouts, &candidates)?;
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
        "buffered PWM requires exact-package or common-intersection pins"
    );
    for name in spec.peripherals {
        let timer = core
            .peripherals
            .iter()
            .find(|p| p.name == *name)
            .context("buffered PWM timer absent")?;
        let reference = timer
            .registers
            .as_ref()
            .context("buffered PWM register identity absent")?;
        ensure!(
            reference.kind == "gtim"
                && reference.version == spec.version
                && reference.block == "GTIM",
            "buffered PWM real register identity differs"
        );
        ensure!(
            timer.pins.is_empty(),
            "buffered PWM routes already projected"
        );
    }
    // Project only after every source and package check succeeds. The shared
    // digital projector validates the actual selected GPIO register/field IR.
    let mut projected = core.clone();
    let count = super::project(
        &serde_json::from_value(input.clone())?,
        profile,
        &mut projected,
        registers,
    )?;
    let expected = array(&input, "routes")?
        .iter()
        .filter(|r| pins.contains(r["pin"].as_str().unwrap()))
        .count();
    ensure!(
        count == expected && count > 0,
        "buffered PWM projection differs"
    );
    *core = projected;
    Ok(count)
}

#[cfg(test)]
mod tests;
