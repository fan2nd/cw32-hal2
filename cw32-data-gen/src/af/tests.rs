use super::*;
use serde_json::{Value, json};

fn evidence() -> Value {
    json!({"pin":"PA2", "af":5, "peripheral":"UART3", "signal":"RX",
        "source_macro":null, "source_line":null, "source_kind":"datasheet",
        "gpio_register":"AFRL", "gpio_field":"AFR2",
        "datasheet_cell":{"pin":"PA2", "af":5, "table":"5-3",
            "pdf_page":31, "printed_page":30, "function":"UART3_RXD"}})
}

fn valid(value: Value) -> bool {
    let route: Route = serde_json::from_value(value).unwrap();
    validate_route_evidence(&route, capabilities("CW32W031").unwrap()).is_ok()
}

#[test]
fn explicit_datasheet_route_needs_no_fabricated_sdk_macro() {
    assert!(valid(evidence()));
}

#[test]
fn datasheet_route_requires_exact_source_coordinate() {
    let mut value = evidence();
    value["datasheet_cell"] = Value::Null;
    assert!(!valid(value));
    for (field, bad) in [
        ("pin", json!("PA3")),
        ("af", json!(4)),
        ("table", json!("5-2")),
        ("pdf_page", json!(0)),
        ("printed_page", json!(31)),
        ("function", json!("")),
    ] {
        let mut value = evidence();
        value["datasheet_cell"][field] = bad;
        assert!(!valid(value), "accepted invalid {field}");
    }
}

#[test]
fn route_evidence_cannot_be_silently_downgraded() {
    let mut value = evidence();
    value["source_kind"] = json!("sdk-and-datasheet");
    assert!(!valid(value.clone()));
    value["source_macro"] = json!("PA02_AFx_UART3RXD");
    value["source_line"] = json!(916);
    assert!(valid(value.clone()));
    value["source_kind"] = json!("datasheet-corrected-sdk");
    assert!(valid(value.clone()));
    value["source_kind"] = Value::Null;
    assert!(valid(value.clone()));
    value["source_kind"] = json!("candidate");
    assert!(!valid(value));
    let mut fabricated = evidence();
    fabricated["source_macro"] = json!("invented");
    fabricated["source_line"] = json!(1);
    assert!(!valid(fabricated));
}

#[test]
fn gpio_and_undocumented_selectors_are_rejected() {
    for af in [0, 8, 15, 255] {
        let mut value = evidence();
        value["af"] = json!(af);
        value["datasheet_cell"]["af"] = json!(af);
        assert!(!valid(value));
    }
}

#[test]
fn l012_extended_selectors_are_explicit_and_do_not_leak_to_other_families() {
    for profile in [
        "CW32F002", "CW32F003", "CW32L010", "CW32L011", "CW32L031", "CW32R031", "CW32W031",
        "CW32L052", "CW32L083", "CW32L012",
    ] {
        let capability = capabilities(profile).unwrap();
        for af in [0, 1, 7, 8, 9, 10, 15, 255] {
            let mut value = evidence();
            value["af"] = json!(af);
            value["datasheet_cell"]["af"] = json!(af);
            value["datasheet_cell"]["pdf_page"] = json!(30 + capability.pdf_printed_page_offset);
            let route = serde_json::from_value(value).unwrap();
            assert_eq!(
                validate_route_evidence(&route, capability).is_ok(),
                (1..=if profile == "CW32L012" { 9 } else { 7 }).contains(&af),
                "{profile} AF{af}"
            );
        }
    }
    assert!(capabilities("CW32UNREVIEWED").is_err());
}

#[test]
fn low_power_datasheet_page_offsets_are_not_universal() {
    for profile in ["CW32L011", "CW32L012"] {
        let capability = capabilities(profile).unwrap();
        let mut value = evidence();
        let route = serde_json::from_value(value.clone()).unwrap();
        assert!(validate_route_evidence(&route, capability).is_err());
        value["datasheet_cell"]["pdf_page"] = json!(33);
        let route = serde_json::from_value(value).unwrap();
        assert!(validate_route_evidence(&route, capability).is_ok());
    }
}

#[test]
fn new_profile_capabilities_require_exact_source_declaration() {
    let mut value = json!({"schema_version": 1, "profile": "CW32L012",
        "status":"verified-sdk-and-datasheet", "datasheet":{}, "routes":[]});
    let valid = |value: Value| {
        validate_profile_capabilities(&serde_json::from_value(value).unwrap()).is_ok()
    };
    assert!(!valid(value.clone()));
    value["selector_capability"] = json!({"min":1,"max":9,"pdf_printed_page_offset":3,
        "evidence":"DS V1.0 Tables 5-3..6"});
    assert!(valid(value.clone()));
    for (field, bad) in [
        ("min", json!(0)),
        ("max", json!(15)),
        ("pdf_printed_page_offset", json!(1)),
        ("evidence", json!("")),
    ] {
        let mut broken = value.clone();
        broken["selector_capability"][field] = bad;
        assert!(!valid(broken), "accepted invalid capability {field}");
    }
}
