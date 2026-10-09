use super::*;
use std::path::PathBuf;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_owned()
}
fn load(path: &str) -> Value {
    if path.starts_with("cw32-data/data/") || path.starts_with("docs/") {
        serde_json::from_slice(&fs::read(root().join(path)).unwrap()).unwrap()
    } else {
        crate::read_yaml(root().join(path)).unwrap()
    }
}
fn input() -> Value {
    load("cw32-data/af/cw32l012-analog.yaml")
}
fn valid(input: &Value) -> bool {
    validate(input, &load(sources::source("pinouts").0)).is_ok()
}
fn core(part: &str) -> chip::Core {
    let mut value = load(&format!("cw32-data/data/chips/{part}.json"));
    let mut core: chip::Core = serde_json::from_value(value["cores"][0].take()).unwrap();
    for adc in core
        .peripherals
        .iter_mut()
        .filter(|p| matches!(p.name.as_str(), "ADC1" | "ADC2"))
    {
        adc.pins.clear();
    }
    core
}
fn project(core: &mut chip::Core) -> Result<usize> {
    apply(
        &root(),
        "cw32-data/af/cw32l012-analog.yaml",
        "CW32L012",
        core,
    )
}
fn routes(core: &chip::Core) -> BTreeSet<(String, String, String, u8)> {
    core.peripherals
        .iter()
        .filter(|p| matches!(p.name.as_str(), "ADC1" | "ADC2"))
        .flat_map(|adc| {
            adc.pins.iter().map(|p| {
                (
                    adc.name.clone(),
                    p.pin.clone(),
                    p.signal.clone(),
                    p.adc_mux.unwrap(),
                )
            })
        })
        .collect()
}
fn fails_without_mutation(mut core: chip::Core) {
    let before = serde_json::to_value(&core).unwrap();
    assert!(project(&mut core).is_err());
    assert_eq!(serde_json::to_value(&core).unwrap(), before);
}

#[test]
fn own_sources_and_both_exact_packages_and_alias_validate() {
    assert!(valid(&input()));
    let expected: BTreeSet<_> = array(&input(), "routes")
        .unwrap()
        .iter()
        .map(|r| {
            (
                r["peripheral"].as_str().unwrap().into(),
                r["pin"].as_str().unwrap().into(),
                r["signal"].as_str().unwrap().into(),
                r["mux"].as_u64().unwrap() as u8,
            )
        })
        .collect();
    let mut sets: Vec<BTreeSet<(String, String, String, u8)>> = Vec::new();
    for part in ["CW32L012C8T6", "CW32L012C8U6", "CW32L012"] {
        let mut core = core(part);
        assert_eq!(project(&mut core).unwrap(), 24);
        let got = routes(&core);
        assert_eq!(got, expected, "{part}");
        for name in ["ADC1", "ADC2"] {
            let adc = core.peripherals.iter().find(|p| p.name == name).unwrap();
            assert_eq!(adc.pins.len(), 12);
            assert!(
                adc.pins
                    .iter()
                    .all(|p| p.af.is_none() && p.adc_mux.unwrap() < 12)
            );
        }
        if part == "CW32L012" {
            assert_eq!(got, sets[0].intersection(&sets[1]).cloned().collect());
        } else {
            sets.push(got);
        }
    }
}

#[test]
fn shared_physical_pads_keep_instance_specific_muxes() {
    let mut core = core("CW32L012");
    project(&mut core).unwrap();
    let got = routes(&core);
    for (pin, mux1, mux2) in [
        ("PA5", 5, 0),
        ("PA6", 6, 1),
        ("PA7", 7, 2),
        ("PB0", 8, 3),
        ("PB1", 9, 4),
        ("PB10", 10, 10),
        ("PB2", 11, 11),
    ] {
        assert!(got.contains(&("ADC1".into(), pin.into(), format!("IN{mux1}"), mux1)));
        assert!(got.contains(&("ADC2".into(), pin.into(), format!("IN{mux2}"), mux2)));
    }
    assert!(
        !got.iter()
            .any(|(_, pin, _, _)| matches!(pin.as_str(), "PB11" | "PA13" | "PA14"))
    );
    assert!(
        !got.iter()
            .any(|(adc, pin, _, _)| adc == "ADC1" && pin == "PA12")
    );
    assert!(
        !got.iter()
            .any(|(adc, pin, _, _)| adc == "ADC2" && pin == "PA0")
    );
}

#[test]
fn instance_source_mux_and_sdk_mutations_fail_closed() {
    let base = input();
    for index in [0, 12] {
        for (key, bad) in [
            ("pin", json!("PA13")),
            ("pin", json!("PB11")),
            ("signal", json!("IN12")),
            ("source_signal", json!("ADC_IN0")),
            ("channel", json!(12)),
            ("mux", json!(12)),
            ("mux", json!(13)),
            ("af", json!(0)),
            ("peripheral", json!("ADC")),
            ("source_macro", json!("ADC_ExInputCH0")),
            ("source_line", json!(1)),
            ("sdk_pin_comment_line", json!(1)),
            ("sdk_comment_pin", json!("PA13")),
            ("source_kind", json!("candidate")),
            ("source_sha256", json!("bad")),
        ] {
            let mut bad_input = base.clone();
            bad_input["routes"][index][key] = bad;
            assert!(!valid(&bad_input), "accepted route {index} {key}");
        }
        for field in [
            "manual_cell",
            "pin_cell",
            "package_pins",
            "oscillator_aliases",
        ] {
            let mut bad = base.clone();
            bad["routes"][index][field] = Value::Null;
            assert!(!valid(&bad), "accepted route {index} {field}");
        }
    }
    let mut bad = base.clone();
    bad["routes"][0]["peripheral"] = json!("ADC2");
    bad["routes"][12]["peripheral"] = json!("ADC1");
    assert!(!valid(&bad));
    let mut bad = base.clone();
    bad["routes"][12] = bad["routes"][0].clone();
    assert!(!valid(&bad));
    let mut bad = base.clone();
    bad["routes"].as_array_mut().unwrap().pop();
    assert!(!valid(&bad));
    let mut bad = base;
    let mut internal = bad["routes"][0].clone();
    internal["mux"] = json!(12);
    internal["source_signal"] = json!("DAC_OUT2");
    bad["routes"].as_array_mut().unwrap().push(internal);
    assert!(!valid(&bad));
}

#[test]
fn source_identity_and_package_provenance_cannot_drift() {
    let base = input();
    for (key, value) in [
        ("profile", json!("CW32L011")),
        ("kind", json!("pwm")),
        ("status", json!("candidate")),
        ("alias_pin_policy", json!("largest-package")),
        ("register_version", json!("cw32l011_v1")),
    ] {
        let mut bad = base.clone();
        bad[key] = value;
        assert!(!valid(&bad));
    }
    for (source, _, _) in sources::SOURCES {
        for field in ["file", "sha256"] {
            let mut bad = base.clone();
            bad["sources"][source][field] = json!("wrong-family");
            assert!(!valid(&bad));
        }
    }
    for (field, key, value) in [
        ("manual_cell", "instance", json!("ADC2")),
        ("manual_cell", "pdf_page", json!(579)),
        ("manual_cell", "mux_bits", json!("1100")),
        ("pin_cell", "source_signal", json!("ADC2_IN0")),
        ("pin_cell", "printed_page", json!(34)),
        ("package_pins", "CW32L012C8T6", json!("99")),
    ] {
        let mut bad = base.clone();
        bad["routes"][0][field][key] = value;
        assert!(!valid(&bad));
    }
}

#[test]
fn rejects_bad_core_without_projecting_either_adc() {
    for name in ["ADC1", "ADC2"] {
        let mut bad = core("CW32L012");
        bad.peripherals.retain(|p| p.name != name);
        fails_without_mutation(bad);
        let mut bad = core("CW32L012");
        let duplicate = bad
            .peripherals
            .iter()
            .find(|p| p.name == name)
            .unwrap()
            .clone();
        bad.peripherals.push(duplicate);
        fails_without_mutation(bad);
        let mut bad = core("CW32L012");
        bad.peripherals
            .iter_mut()
            .find(|p| p.name == name)
            .unwrap()
            .address += 4;
        fails_without_mutation(bad);
        let mut bad = core("CW32L012");
        bad.peripherals
            .iter_mut()
            .find(|p| p.name == name)
            .unwrap()
            .registers
            .as_mut()
            .unwrap()
            .version = "v1".into();
        fails_without_mutation(bad);
    }
    let mut bad = core("CW32L012");
    bad.pins.push(chip::core::Pin {
        name: "PD15".into(),
    });
    fails_without_mutation(bad);
    let mut bad = core("CW32L012");
    bad.pins.push(bad.pins[0].clone());
    fails_without_mutation(bad);
    let mut bad = core("CW32L012");
    bad.pins.retain(|p| p.name != "PA0");
    fails_without_mutation(bad);
    let mut bad = core("CW32L012");
    project(&mut bad).unwrap();
    fails_without_mutation(bad);
}
