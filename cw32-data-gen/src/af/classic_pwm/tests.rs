use super::*;
use std::path::PathBuf;

const FAMILIES: [&str; 7] = [
    "CW32F002", "CW32F003", "CW32L031", "CW32R031", "CW32W031", "CW32L052", "CW32L083",
];

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
fn inputs(profile: &str) -> (Value, Value, Value) {
    let prefix = profile.to_lowercase();
    (
        load(&format!("cw32-data/af/{prefix}-pwm.yaml")),
        load(&format!("cw32-data/pinouts/{prefix}.yaml")),
        load(&format!("cw32-data/af/{prefix}.yaml")),
    )
}
fn valid(profile: &str, input: &Value) -> bool {
    let (_, pins, candidates) = inputs(profile);
    validate(input, &profiles::spec(profile).unwrap(), &pins, &candidates).is_ok()
}
fn core(part: &str) -> chip::Core {
    let mut value = load(&format!("cw32-data/data/chips/{part}.json"));
    let mut core: chip::Core = serde_json::from_value(value["cores"][0].take()).unwrap();
    for peripheral in &mut core.peripherals {
        if peripheral.name.starts_with("GTIM") {
            peripheral.pins.clear();
        }
    }
    core
}
fn registers(core: &chip::Core) -> BTreeMap<String, ir::IR> {
    core.peripherals
        .iter()
        .filter(|p| p.name.starts_with("GPIO"))
        .map(|p| {
            let r = p.registers.as_ref().unwrap();
            (
                r.kind.clone(),
                serde_json::from_value(load(&format!(
                    "cw32-data/data/registers/{}_{}.json",
                    r.kind, r.version
                )))
                .unwrap(),
            )
        })
        .collect()
}
fn project(profile: &str, core: &mut chip::Core) -> Result<usize> {
    let registers = registers(core);
    apply(
        &root(),
        &format!("cw32-data/af/{}-pwm.yaml", profile.to_lowercase()),
        profile,
        core,
        &registers,
    )
}

#[test]
fn every_own_family_source_and_canonical_route_array_validates() {
    for profile in FAMILIES {
        assert!(valid(profile, &inputs(profile).0), "{profile}");
    }
    assert!(profiles::spec("CW32L010").is_none());
    assert!(profiles::spec("CW32F020").is_none());
}

#[test]
fn every_exact_package_and_alias_projects_only_bonded_qualified_channels() {
    let proof = load("docs/classic-pwm-route-evidence.json");
    for profile in FAMILIES {
        let (input, _, _) = inputs(profile);
        for (part, count) in proof["families"][profile]["package_counts"]
            .as_object()
            .unwrap()
        {
            let mut core = core(part);
            assert_eq!(
                project(profile, &mut core).unwrap() as u64,
                count.as_u64().unwrap(),
                "{part}"
            );
            let actual: BTreeSet<_> = core
                .peripherals
                .iter()
                .filter(|p| p.name.starts_with("GTIM"))
                .flat_map(|p| {
                    p.pins
                        .iter()
                        .map(|r| (p.name.clone(), r.pin.clone(), r.signal.clone(), r.af))
                })
                .collect();
            let expected: BTreeSet<_> = array(&input, "routes")
                .unwrap()
                .iter()
                .filter(|r| {
                    core.pins
                        .iter()
                        .any(|p| p.name == r["pin"].as_str().unwrap())
                })
                .map(|r| {
                    (
                        r["peripheral"].as_str().unwrap().to_owned(),
                        r["pin"].as_str().unwrap().to_owned(),
                        r["signal"].as_str().unwrap().to_owned(),
                        Some(r["af"].as_u64().unwrap() as u8),
                    )
                })
                .collect();
            assert_eq!(actual, expected, "{part}");
            assert!(
                actual
                    .iter()
                    .all(|r| !["PC14", "PC15", "PF0", "PF1"].contains(&r.1.as_str()))
            );
        }
    }
}

#[test]
fn wrong_channels_afs_source_cells_and_package_provenance_fail_closed() {
    for profile in FAMILIES {
        let base = inputs(profile).0;
        for (field, bad) in [
            ("pin", json!("PF3")),
            ("af", json!(0)),
            ("af", json!(8)),
            ("peripheral", json!("ATIM")),
            ("signal", json!("ETR")),
            ("signal", json!("TOGP")),
            ("source_signal", json!("CH9")),
            ("source_kind", json!("candidate")),
            ("source_line", json!(1)),
            ("channel", json!(0)),
            ("function", json!("GTIM4CH4")),
            ("gpio_register", json!("AFRX")),
            ("gpio_field", json!("AFR16")),
            ("oscillator_aliases", json!(["OSC_IN"])),
            ("package_pins", json!({})),
            ("pin_cell", json!({})),
            ("datasheet_cell", json!({})),
            ("source_sha256", json!("unqualified")),
        ] {
            let mut bad_input = base.clone();
            bad_input["routes"][0][field] = bad;
            assert!(!valid(profile, &bad_input), "{profile}: {field}");
        }
        for field in [
            "pin",
            "function",
            "af",
            "table",
            "pdf_page",
            "printed_page",
            "sha256",
        ] {
            let mut bad = base.clone();
            bad["routes"][0]["datasheet_cell"][field] = Value::Null;
            assert!(!valid(profile, &bad), "{profile}: datasheet {field}");
        }
        let mut missing = base.clone();
        missing["routes"].as_array_mut().unwrap().pop();
        assert!(!valid(profile, &missing));
        let mut duplicate = base.clone();
        duplicate["routes"][1] = base["routes"][0].clone();
        assert!(!valid(profile, &duplicate));
    }
}

#[test]
fn sdk_only_debug_oscillator_and_radio_routes_cannot_be_reintroduced() {
    for profile in FAMILIES {
        let base = inputs(profile).0;
        for group in ["excluded_sdk_routes", "excluded_safety_routes"] {
            for excluded in array(&base, group).unwrap() {
                let mut bad = base.clone();
                bad["routes"][0]["pin"] = excluded["pin"].clone();
                bad["routes"][0]["af"] = excluded["af"].clone();
                assert!(!valid(profile, &bad), "{profile}: {excluded}");
            }
        }
        for source in [
            "datasheet",
            "reference_manual",
            "sdk_archive",
            "gpio_header",
            "pinouts",
            "sdk_candidates",
        ] {
            let mut bad = base.clone();
            bad["sources"][source]["sha256"] = json!("0".repeat(64));
            assert!(!valid(profile, &bad), "{profile}: {source}");
        }
        for (field, value) in [
            ("profile", json!("CW32F020")),
            ("kind", json!("analog")),
            ("status", json!("candidate-sdk-and-register-verified")),
            ("register_version", json!("v1")),
            ("alias_pin_policy", json!("largest-package")),
        ] {
            let mut bad = base.clone();
            bad[field] = value;
            assert!(!valid(profile, &bad));
        }
    }
}

#[test]
fn projection_rejects_wrong_registers_pin_scope_and_duplicate_without_mutation() {
    for profile in FAMILIES {
        for corruption in 0..4 {
            let mut bad = core(profile);
            match corruption {
                0 => bad.pins.push(chip::core::Pin {
                    name: "PZ15".into(),
                }),
                1 => {
                    bad.peripherals
                        .iter_mut()
                        .find(|p| p.name.starts_with("GTIM"))
                        .unwrap()
                        .registers
                        .as_mut()
                        .unwrap()
                        .version = "v1".into()
                }
                2 => {
                    bad.peripherals.retain(|p| !p.name.starts_with("GTIM"));
                }
                3 => {
                    project(profile, &mut bad).unwrap();
                }
                _ => unreachable!(),
            }
            let before = serde_json::to_value(&bad).unwrap();
            assert!(
                project(profile, &mut bad).is_err(),
                "{profile}: {corruption}"
            );
            assert_eq!(serde_json::to_value(&bad).unwrap(), before);
        }
        let mut core = core(profile);
        let mut registers = registers(&core);
        registers.clear();
        let before = serde_json::to_value(&core).unwrap();
        assert!(
            apply(
                &root(),
                &format!("cw32-data/af/{}-pwm.yaml", profile.to_lowercase()),
                profile,
                &mut core,
                &registers
            )
            .is_err()
        );
        assert_eq!(serde_json::to_value(&core).unwrap(), before);
        assert!(
            apply(
                &root(),
                "cw32-data/af/cw32f020-pwm.yaml",
                profile,
                &mut core,
                &registers
            )
            .is_err()
        );
    }
}
