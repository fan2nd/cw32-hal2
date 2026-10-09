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
fn inputs(kind: &str) -> (Value, Value, Value) {
    (
        load(&format!("cw32-data/af/cw32f020-{kind}.yaml")),
        load("cw32-data/pinouts/cw32f020.yaml"),
        load("cw32-data/af/cw32f020.yaml"),
    )
}
fn valid(input: &Value, kind: &str) -> bool {
    let (_, pins, candidates) = inputs(kind);
    validate(input, kind, &pins, &candidates).is_ok()
}
fn core(part: &str) -> chip::Core {
    let mut value = load(&format!("cw32-data/data/chips/{part}.json"));
    let mut core: chip::Core = serde_json::from_value(value["cores"][0].take()).unwrap();
    for p in &mut core.peripherals {
        if p.name == "ADC" || p.name.starts_with("GTIM") {
            p.pins.clear();
        }
    }
    core
}
fn registers() -> BTreeMap<String, ir::IR> {
    ["gpio", "gpioc", "gpiof"]
        .into_iter()
        .map(|name| {
            (
                name.into(),
                serde_yaml::from_slice(
                    &fs::read(root().join(format!("cw32-data/registers/{name}_v1.yaml"))).unwrap(),
                )
                .unwrap(),
            )
        })
        .collect()
}

#[test]
fn qualified_sources_and_all_route_tuples_validate() {
    for kind in ["analog", "pwm"] {
        assert!(valid(&inputs(kind).0, kind));
    }
}

#[test]
fn exact_packages_and_alias_have_only_qualified_bonded_routes() {
    for (part, adc_count, pwm_count) in [
        ("CW32F020", 9, 17),
        ("CW32F020F6U7", 9, 17),
        ("CW32F020K6U7", 11, 32),
        ("CW32F020C6U7", 13, 46),
    ] {
        let mut core = core(part);
        assert_eq!(
            apply(
                &root(),
                "cw32-data/af/cw32f020-analog.yaml",
                "CW32F020",
                "analog",
                &mut core,
                None
            )
            .unwrap(),
            adc_count
        );
        assert_eq!(
            apply(
                &root(),
                "cw32-data/af/cw32f020-pwm.yaml",
                "CW32F020",
                "pwm",
                &mut core,
                Some(&registers())
            )
            .unwrap(),
            pwm_count
        );
        assert!(!core.peripherals.iter().any(|p| p.name == "ATIM"));
        for p in core
            .peripherals
            .iter()
            .filter(|p| p.name == "ADC" || p.name.starts_with("GTIM"))
        {
            for route in &p.pins {
                assert!(!["PA13", "PA14", "PF3", "NRST"].contains(&route.pin.as_str()));
                assert!(core.pins.iter().any(|pin| pin.name == route.pin));
                if p.name == "ADC" {
                    assert!(route.af.is_none());
                } else {
                    assert!(["CH1", "CH2", "CH3", "CH4"].contains(&route.signal.as_str()));
                }
            }
        }
        if adc_count == 9 {
            let adc = core.peripherals.iter().find(|p| p.name == "ADC").unwrap();
            assert!(
                !adc.pins
                    .iter()
                    .any(|p| ["PB0", "PB2", "PB10", "PB11"].contains(&p.pin.as_str()))
            );
        }
    }
}

#[test]
fn analog_source_identity_and_hardware_mux_cannot_be_swapped() {
    let base = inputs("analog").0;
    for (field, bad) in [
        ("pin", json!("PB2")),
        ("signal", json!("IN1")),
        ("source_signal", json!("ADC_IN1")),
        ("channel", json!(1)),
        ("mux", json!(1)),
        ("mux", json!(13)),
        ("af", json!(0)),
        ("peripheral", json!("ADC2")),
        ("source_macro", json!("ADC_ExInputCH1")),
        ("source_line", json!(207)),
        ("sdk_pin_comment_line", json!(190)),
        ("source_sha256", json!(GPIO_HEADER)),
    ] {
        let mut bad_input = base.clone();
        bad_input["routes"][0][field] = bad;
        assert!(
            !valid(&bad_input, "analog"),
            "accepted incorrect ADC {field}"
        );
    }
    for (field, bad) in [
        ("mux_bits", json!("0001")),
        ("source_signal", json!("AIN1")),
        ("pin", json!("PA1")),
        ("table", json!("22-5")),
        ("pdf_page", json!(377)),
        ("printed_page", json!(378)),
        ("sha256", json!(DATASHEET)),
    ] {
        let mut input = base.clone();
        input["routes"][0]["manual_cell"][field] = bad;
        assert!(
            !valid(&input, "analog"),
            "accepted incorrect manual {field}"
        );
    }
}

#[test]
fn pwm_wrong_af_channel_peripheral_and_non_pwm_signals_are_rejected() {
    let base = inputs("pwm").0;
    for (field, bad) in [
        ("af", json!(1)),
        ("signal", json!("CH2")),
        ("source_signal", json!("CH2")),
        ("peripheral", json!("GTIM1")),
        ("peripheral", json!("ATIM")),
        ("signal", json!("ETR")),
        ("signal", json!("TOGP")),
        ("function", json!("GTIM2CH2")),
        ("gpio_field", json!("AFR1")),
        ("gpio_register", json!("AFRH")),
        ("source_macro", json!("PA00_AFx_GTIM2CH2")),
        ("source_line", json!(1)),
    ] {
        let mut input = base.clone();
        input["routes"][0][field] = bad;
        assert!(!valid(&input, "pwm"), "accepted incorrect PWM {field}");
    }
    for (field, bad) in [
        ("pin", json!("PA1")),
        ("af", json!(1)),
        ("function", json!("GTIM2_CH2")),
        ("table", json!("5-4")),
        ("printed_page", json!(27)),
        ("pdf_page", json!(26)),
        ("sha256", json!(MANUAL)),
    ] {
        let mut input = base.clone();
        input["routes"][0]["datasheet_cell"][field] = bad;
        assert!(!valid(&input, "pwm"), "accepted incorrect AF cell {field}");
    }
}

#[test]
fn package_provenance_and_restricted_pin_mutations_fail_closed() {
    for kind in ["analog", "pwm"] {
        let base = inputs(kind).0;
        for bad_pin in ["PA13", "PA14", "PF3", "NRST", "PA16", "PD0"] {
            let mut input = base.clone();
            input["routes"][0]["pin"] = json!(bad_pin);
            assert!(!valid(&input, kind), "accepted restricted {kind} {bad_pin}");
        }
        for (field, bad) in [
            ("positions", json!({"QFN48":"11","QFN32":"6","QFN20":"3"})),
            ("table", json!("5-1")),
            ("pdf_page", json!(24)),
            ("source_name", json!("PA01")),
        ] {
            let mut input = base.clone();
            input["routes"][0]["pin_cell"][field] = bad;
            assert!(
                !valid(&input, kind),
                "accepted incorrect package source {field}"
            );
        }
        let mut input = base.clone();
        input["routes"][0]["package_pins"]["CW32F020F6U7"] = Value::Null;
        assert!(!valid(&input, kind));
        let mut input = base.clone();
        input["routes"][0]["package_pins"]["CW32F020F6U7"] = json!("4");
        assert!(!valid(&input, kind));
        let mut input = base.clone();
        input["routes"].as_array_mut().unwrap().pop();
        assert!(!valid(&input, kind));
        let mut input = base.clone();
        input["routes"][1] = base["routes"][0].clone();
        assert!(!valid(&input, kind));
        let mut input = base.clone();
        input["sources"]["datasheet"]["sha256"] =
            json!("9fe3f5cf054612faf3b94de3e0f4166886e7b7ab2a43ebced009a48270aaca91");
        assert!(!valid(&input, kind));
        let mut input = base.clone();
        input["alias_pin_policy"] = json!("largest-package");
        assert!(!valid(&input, kind));
        let mut input = base.clone();
        input["profile"] = json!("CW32F003");
        assert!(!valid(&input, kind));
    }
    let mut input = inputs("analog").0;
    input["routes"][8]["package_pins"]["CW32F020F6U7"] = json!("11"); // PB0 is unbonded.
    assert!(!valid(&input, "analog"));
}

#[test]
fn projection_rejects_unqualified_core_and_does_not_partially_modify_it() {
    let mut bad = core("CW32F020F6U7");
    bad.pins.push(chip::core::Pin { name: "PB0".into() });
    let before = serde_json::to_value(&bad).unwrap();
    assert!(
        apply(
            &root(),
            "cw32-data/af/cw32f020-analog.yaml",
            "CW32F020",
            "analog",
            &mut bad,
            None
        )
        .is_err()
    );
    assert_eq!(serde_json::to_value(&bad).unwrap(), before);
    let mut bad = core("CW32F020C6U7");
    bad.peripherals
        .iter_mut()
        .find(|p| p.name == "ADC")
        .unwrap()
        .registers
        .as_mut()
        .unwrap()
        .version = "v1".into();
    assert!(
        apply(
            &root(),
            "cw32-data/af/cw32f020-analog.yaml",
            "CW32F020",
            "analog",
            &mut bad,
            None
        )
        .is_err()
    );
    let mut bad = core("CW32F020C6U7");
    assert!(
        apply(
            &root(),
            "cw32-data/af/cw32f020-pwm.yaml",
            "CW32F030",
            "pwm",
            &mut bad,
            Some(&registers())
        )
        .is_err()
    );
}

#[test]
fn pinned_sidecars_reject_hash_or_path_downgrades() {
    let input = inputs("analog").0;
    assert!(
        pinned_yaml(
            &root(),
            &input["sources"]["pinouts"],
            "cw32-data/pinouts/cw32f020.yaml"
        )
        .is_ok()
    );
    let mut source = input["sources"]["pinouts"].clone();
    source["sha256"] = json!(DATASHEET);
    assert!(pinned_yaml(&root(), &source, "cw32-data/pinouts/cw32f020.yaml").is_err());
    let mut source = input["sources"]["pinouts"].clone();
    source["file"] = json!("cw32-data/pinouts/cw32f030.yaml");
    assert!(pinned_yaml(&root(), &source, "cw32-data/pinouts/cw32f020.yaml").is_err());
}
