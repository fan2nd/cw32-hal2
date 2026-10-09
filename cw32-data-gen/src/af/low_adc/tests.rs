use super::*;
use std::path::PathBuf;
const FAMILIES: [&str; 2] = ["CW32L010", "CW32L011"];
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
fn input(profile: &str) -> Value {
    load(&format!(
        "cw32-data/af/{}-analog.yaml",
        profile.to_lowercase()
    ))
}
fn valid(profile: &str, input: &Value) -> bool {
    let spec = profiles::spec(profile).unwrap();
    validate(input, &spec, &load(spec.source("pinouts").0)).is_ok()
}
fn core(part: &str) -> chip::Core {
    let mut value = load(&format!("cw32-data/data/chips/{part}.json"));
    let mut core: chip::Core = serde_json::from_value(value["cores"][0].take()).unwrap();
    core.peripherals
        .iter_mut()
        .find(|p| p.name == "ADC")
        .unwrap()
        .pins
        .clear();
    core
}
fn project(profile: &str, core: &mut chip::Core) -> Result<usize> {
    apply(
        &root(),
        &format!("cw32-data/af/{}-analog.yaml", profile.to_lowercase()),
        profile,
        core,
    )
}
#[test]
fn all_own_sources_and_exact_package_projections_validate() {
    for profile in FAMILIES {
        let input = input(profile);
        assert!(valid(profile, &input), "{profile}");
        let spec = profiles::spec(profile).unwrap();
        let pinouts = load(spec.source("pinouts").0);
        let mut parts: Vec<_> = array(&pinouts, "packages")
            .unwrap()
            .iter()
            .map(|p| p["name"].as_str().unwrap())
            .collect();
        parts.push(profile);
        let mut route_sets: Vec<BTreeSet<(String, String, u8)>> = Vec::new();
        for part in parts {
            let mut core = core(part);
            let count = project(profile, &mut core).unwrap();
            let adc = core.peripherals.iter().find(|p| p.name == "ADC").unwrap();
            let got: BTreeSet<_> = adc
                .pins
                .iter()
                .map(|p| (p.pin.clone(), p.signal.clone(), p.adc_mux.unwrap()))
                .collect();
            let expected: BTreeSet<_> = array(&input, "routes")
                .unwrap()
                .iter()
                .filter(|r| core.pins.iter().any(|p| p.name == r["pin"]))
                .map(|r| {
                    (
                        r["pin"].as_str().unwrap().to_owned(),
                        r["signal"].as_str().unwrap().to_owned(),
                        r["mux"].as_u64().unwrap() as u8,
                    )
                })
                .collect();
            assert_eq!(got, expected, "{part}");
            assert_eq!(count, expected.len());
            assert!(adc.pins.iter().all(|p| p.af.is_none()));
            if part == profile {
                assert_eq!(
                    got,
                    route_sets.iter().skip(1).fold(
                        route_sets[0].clone(),
                        |a: BTreeSet<_>, b: &BTreeSet<_>| a.intersection(b).cloned().collect()
                    ),
                    "{profile} alias must be common intersection"
                );
            } else {
                route_sets.push(got);
            }
        }
    }
}
#[test]
fn source_signal_mux_and_sdk_mutations_fail_closed() {
    for profile in FAMILIES {
        let base = input(profile);
        for (key, bad) in [
            ("pin", json!("PA13")),
            ("pin", json!("PC5")),
            ("pin", json!("PF3")),
            ("signal", json!("IN15")),
            ("source_signal", json!("ADC_IN15")),
            ("channel", json!(15)),
            ("mux", json!(15)),
            ("af", json!(0)),
            ("peripheral", json!("ADC2")),
            ("source_macro", json!("ADC_InputCH15")),
            ("sdk_pin_comment_column", json!(3)),
            ("source_line", json!(1)),
            ("sdk_pin_comment_line", json!(1)),
            ("sdk_comment_pin", json!("PA13")),
            ("source_kind", json!("candidate")),
            ("source_sha256", json!("bad")),
        ] {
            let mut bad_input = base.clone();
            bad_input["routes"][0][key] = bad;
            assert!(!valid(profile, &bad_input), "{profile} accepted {key}");
        }
        for field in [
            "manual_cell",
            "pin_cell",
            "package_pins",
            "oscillator_aliases",
        ] {
            let mut bad = base.clone();
            bad["routes"][0][field] = Value::Null;
            assert!(!valid(profile, &bad), "{profile} accepted {field}");
        }
        let mut bad = base.clone();
        bad["routes"][1] = bad["routes"][0].clone();
        assert!(!valid(profile, &bad));
        let mut bad = base.clone();
        bad["routes"].as_array_mut().unwrap().pop();
        assert!(!valid(profile, &bad));
    }
}
#[test]
fn family_sources_alias_policy_and_register_identity_cannot_drift() {
    for profile in FAMILIES {
        let base = input(profile);
        for (key, value) in [
            ("profile", json!("CW32F020")),
            ("kind", json!("pwm")),
            ("status", json!("candidate")),
            ("alias_pin_policy", json!("largest-package")),
            ("register_version", json!("v1")),
        ] {
            let mut bad = base.clone();
            bad[key] = value;
            assert!(!valid(profile, &bad));
        }
        for source in [
            "datasheet",
            "reference_manual",
            "sdk_archive",
            "adc_header",
            "pinouts",
        ] {
            for field in ["file", "sha256"] {
                let mut bad = base.clone();
                bad["sources"][source][field] = json!("wrong-family");
                assert!(!valid(profile, &bad));
            }
        }
        let mut bad = core(profile);
        bad.peripherals
            .iter_mut()
            .find(|p| p.name == "ADC")
            .unwrap()
            .registers
            .as_mut()
            .unwrap()
            .version = "v1".into();
        let before = serde_json::to_value(&bad).unwrap();
        assert!(project(profile, &mut bad).is_err());
        assert_eq!(serde_json::to_value(&bad).unwrap(), before);
        let mut bad = core(profile);
        bad.pins.push(chip::core::Pin {
            name: "PD15".into(),
        });
        let before = serde_json::to_value(&bad).unwrap();
        assert!(project(profile, &mut bad).is_err());
        assert_eq!(serde_json::to_value(&bad).unwrap(), before);
        let mut bad = core(profile);
        project(profile, &mut bad).unwrap();
        let before = serde_json::to_value(&bad).unwrap();
        assert!(project(profile, &mut bad).is_err());
        assert_eq!(serde_json::to_value(&bad).unwrap(), before);
    }
}

#[test]
fn family_maps_and_source_columns_cannot_be_interchanged() {
    let mut l011 = input("CW32L011");
    let l010 = input("CW32L010");
    for i in 7..14 {
        let original = l011["routes"][i].clone();
        l011["routes"][i]["pin"] = l010["routes"][i]["pin"].clone();
        assert!(!valid("CW32L011", &l011));
        l011["routes"][i] = original;
    }
    l011["routes"][7]["sdk_pin_comment_column"] = json!(0);
    assert!(!valid("CW32L011", &l011));
}

#[test]
fn safe_package_counts_and_reserved_pins_are_explicit() {
    for (profile, part, count, forbidden) in [
        ("CW32L010", "CW32L010", 10, &["PA7", "PA8", "PB7"][..]),
        (
            "CW32L010",
            "CW32L010Y8M6",
            10,
            &["PA7", "PA8", "PB7", "PA2", "PB2", "PB3", "PB4"][..],
        ),
        ("CW32L010", "CW32L010F8P6", 14, &["PA7", "PA8", "PB7"][..]),
        ("CW32L010", "CW32L010F8U6", 14, &["PA7", "PA8", "PB7"][..]),
        ("CW32L011", "CW32L011", 14, &["PA13", "PA14"][..]),
        ("CW32L011", "CW32L011K8T6", 14, &["PA13", "PA14"][..]),
        ("CW32L011", "CW32L011K8U6", 14, &["PA13", "PA14"][..]),
    ] {
        let mut core = core(part);
        assert_eq!(project(profile, &mut core).unwrap(), count, "{part}");
        let adc = core.peripherals.iter().find(|p| p.name == "ADC").unwrap();
        assert!(
            adc.pins
                .iter()
                .all(|p| !forbidden.contains(&p.pin.as_str()))
        );
        assert!(adc.pins.iter().all(|p| p.adc_mux.is_some_and(|m| m < 14)));
    }
}

#[test]
fn internal_sources_are_provenance_only_never_gpio_routes() {
    for profile in FAMILIES {
        let base = input(profile);
        for (key, value) in [
            ("mux", json!(13)),
            ("signal", json!("VDD_DIV3")),
            ("source_macro", json!("ADC_InputVDD")),
            ("source_line", json!(1)),
            ("source_sha256", json!("wrong")),
            ("manual_cell", Value::Null),
            ("pin", json!("PA0")),
        ] {
            let mut bad = base.clone();
            bad["internal_sources"][0][key] = value;
            assert!(!valid(profile, &bad), "{profile} internal {key}");
        }
        let mut bad = base.clone();
        bad["internal_sources"].as_array_mut().unwrap().pop();
        assert!(!valid(profile, &bad));
        let mut bad = base.clone();
        bad["routes"]
            .as_array_mut()
            .unwrap()
            .push(base["internal_sources"][0].clone());
        assert!(!valid(profile, &bad));
    }
}

#[test]
fn l011_datasheet_front_matter_offset_is_not_the_manual_offset() {
    let mut bad = input("CW32L011");
    assert_eq!(bad["routes"][0]["pin_cell"]["pdf_page"], 29);
    assert_eq!(bad["routes"][0]["pin_cell"]["printed_page"], 26);
    assert_eq!(bad["routes"][0]["manual_cell"]["pdf_page"], 509);
    assert_eq!(bad["routes"][0]["manual_cell"]["printed_page"], 508);
    bad["routes"][0]["pin_cell"]["printed_page"] = json!(28);
    assert!(!valid("CW32L011", &bad));
}
