use super::*;
fn fixture() -> (Authoring, Core, BTreeMap<String, ir::IR>) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let authoring = crate::read_yaml(root.join(PATH)).unwrap();
    let chip: cw32_data_serde::Chip =
        serde_json::from_slice(&fs::read(root.join("cw32-data/data/chips/CW32L010.json")).unwrap())
            .unwrap();
    let mut core = chip.cores[0].clone();
    for p in &mut core.peripherals {
        p.triggers.clear();
    }
    let mut irs = BTreeMap::new();
    for kind in ["btim", "adc"] {
        irs.insert(
            kind.into(),
            serde_yaml::from_slice(
                &fs::read(root.join(format!("cw32-data/registers/{kind}_cw32l010_v1.yaml")))
                    .unwrap(),
            )
            .unwrap(),
        );
    }
    (authoring, core, irs)
}
#[test]
fn destination_specific_projection_preserves_independent_enables() {
    let (a, mut c, r) = fixture();
    assert_eq!(project(&a, "CW32L010", &mut c, &r).unwrap(), 2);
    let adc = &c
        .peripherals
        .iter()
        .find(|p| p.name == "ADC")
        .unwrap()
        .triggers[0];
    assert_eq!(adc.source, "BTIM1_TRGO");
    assert_eq!(adc.registers[1].role, "independent_enable");
    assert_eq!(adc.registers[1].value, 1);
    assert_eq!(adc.registers[1].bit_offset, 13);
}
#[test]
fn reserved_conflicted_wrong_destination_and_encoding_are_rejected_transactionally() {
    for mutation in 0..14 {
        let (mut a, mut c, mut r) = fixture();
        let original = c.clone();
        match mutation {
            0 => a.routes[0].status = "reserved".into(),
            1 => a.routes[0]
                .conflict_ids
                .push("unresolved-source-conflict".into()),
            2 => a.routes[1].destination = "BTIM1".into(),
            3 => a.routes[1].registers[1].value = 12,
            4 => a.routes[0].registers[1].value = 1 << 13,
            5 => a.routes[0].registers[1].role = "selector".into(),
            6 => a.routes[0].source = "ITR8".into(),
            7 => a.routes[0].source_event = "IRQ".into(),
            8 => a.routes.push(a.routes[0].clone()),
            9 => c.peripherals.retain(|p| p.name != "BTIM1"),
            10 => {
                r.get_mut("adc")
                    .unwrap()
                    .fieldsets
                    .get_mut("TRIGGER")
                    .unwrap()
                    .fields
                    .iter_mut()
                    .find(|f| f.name == "BTIM1TRGO")
                    .unwrap()
                    .bit_offset = ir::BitOffset::Regular(14)
            }
            11 => {
                a.register_versions.remove("ADC");
            }
            12 => {
                a.register_versions
                    .insert("BTIM3".into(), "cw32l010_v1".into());
            }
            13 => {
                c.peripherals
                    .iter_mut()
                    .find(|p| p.name == "ADC")
                    .unwrap()
                    .registers
                    .as_mut()
                    .unwrap()
                    .version = "cw32l011_v1".into();
            }
            _ => unreachable!(),
        }
        let before = c.clone();
        assert!(
            project(&a, "CW32L010", &mut c, &r).is_err(),
            "mutation {mutation}"
        );
        assert_eq!(before, c);
        if mutation != 9 && mutation != 13 {
            assert_eq!(original, c);
        }
    }
}
#[test]
fn sibling_family_and_unqualified_source_edits_cannot_bind() {
    let (a, mut c, r) = fixture();
    assert!(project(&a, "CW32L011", &mut c, &r).is_err());
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    assert!(
        apply(
            root,
            "cw32-data/triggers/cw32l011.yaml",
            "CW32L011",
            &mut c,
            &r
        )
        .is_err()
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(fs::read(root.join(PATH)).unwrap())),
        QUALIFIED_SHA256
    );
}
