// SPDX-License-Identifier: MIT OR Apache-2.0
use chip::core::{Peripheral, peripheral};
use cw32_data_serde::{Chip, chip};
use peripheral::rcc::{Field, KernelClock, StopMode};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

fn read<T: DeserializeOwned>(value: Value) -> T {
    serde_json::from_value(value).unwrap()
}

fn encoded<T: Serialize>(value: T) -> Value {
    serde_json::to_value(value).unwrap()
}

#[test]
fn every_curated_chip_round_trips_without_changing_any_json_value() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../cw32-data/data/chips");
    let mut names = std::fs::read_dir(root)
        .unwrap()
        .map(|p| p.unwrap().path())
        .collect::<Vec<_>>();
    names.sort();
    let mut count = 0;
    for path in names
        .into_iter()
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
    {
        let input: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let model: Chip = serde_json::from_value(input.clone()).unwrap();
        assert_eq!(encoded(model), input, "{}", path.display());
        count += 1;
    }
    assert!(
        count >= 54,
        "expected the full curated chip catalog, got {count}"
    );
}

#[test]
fn missing_options_and_default_collections_have_exact_omission_rules() {
    let p: Peripheral = read(json!({"name":"X"}));
    assert_eq!(p.address, 0);
    assert_eq!(p.spi, None);
    assert_eq!(encoded(p), json!({"name":"X","address":0}));
    let d: peripheral::DmaChannel = read(json!({"signal":"TX", "dma":null, "request":null}));
    assert_eq!(encoded(d), json!({"signal":"TX"}));
    let d: chip::core::DmaChannels =
        read(json!({"name":"C", "dma":"D", "channel":0, "supports_2d":false}));
    assert_eq!(
        encoded(d),
        json!({"name":"C", "dma":"D", "channel":0, "supports_2d":false})
    );
}

#[test]
fn omitted_required_arrays_are_errors_even_when_empty_arrays_serialize_away() {
    let afio: peripheral::Afio = read(json!({"register":"R", "field":"F", "values":[]}));
    assert_eq!(encoded(afio), json!({"register":"R", "field":"F"}));
    assert!(
        serde_json::from_value::<peripheral::Afio>(json!({"register":"R", "field":"F"})).is_err()
    );
    assert!(serde_json::from_value::<peripheral::AfioValue>(json!({"value":0})).is_err());
    assert_eq!(
        encoded(read::<peripheral::AfioValue>(json!({"value":0,"pins":[]}))),
        json!({"value":0})
    );
    assert!(serde_json::from_value::<chip::Package>(json!({"name":"N", "package":"P"})).is_err());
    assert!(
        serde_json::from_value::<chip::Core>(
            json!({"name":"cm0p", "peripherals":[], "interrupts":[], "dma_channels":[]})
        )
        .is_err()
    );
}

#[test]
fn enum_wire_forms_defaults_and_invalid_values_are_preserved() {
    assert_eq!(encoded(chip::memory::Kind::Eeprom), json!("eeprom"));
    assert_eq!(encoded(KernelClock::Clock("PCLK".into())), json!("PCLK"));
    assert_eq!(
        encoded(KernelClock::Mux(Field {
            register: "MUX".into(),
            field: "SEL".into()
        })),
        json!({"register":"MUX", "field":"SEL"})
    );
    assert_eq!(encoded(StopMode::Standby), json!("Standby"));
    let base =
        json!({"bus_clock":"PCLK", "kernel_clock":"PCLK", "enable":{"register":"EN", "field":"X"}});
    let rcc: peripheral::Rcc = read(base.clone());
    assert_eq!(rcc.stop_mode, StopMode::Stop1);
    assert_eq!(encoded(rcc), base);
    for invalid in [json!("Flash"), json!("rom"), json!(0)] {
        assert!(serde_json::from_value::<chip::memory::Kind>(invalid).is_err());
    }
    for invalid in [json!("stop1"), json!(0), Value::Null] {
        assert!(serde_json::from_value::<StopMode>(invalid).is_err());
    }
    for invalid in [
        json!(1),
        json!({"register":"R"}),
        json!({"clock":"PCLK"}),
        Value::Null,
    ] {
        assert!(serde_json::from_value::<KernelClock>(invalid).is_err());
    }
}

#[test]
fn integer_widths_required_fields_and_unknown_field_behavior_are_preserved() {
    for invalid in [json!(-1), json!(256), json!(1.5), json!("1")] {
        assert!(
            serde_json::from_value::<chip::core::Interrupt>(
                json!({"name":"IRQ", "number":invalid})
            )
            .is_err()
        );
    }
    assert!(serde_json::from_value::<Peripheral>(json!({"address":0})).is_err());
    assert!(
        serde_json::from_value::<Peripheral>(json!({"name":"X", "address":4294967296_u64}))
            .is_err()
    );
    assert!(
        serde_json::from_value::<peripheral::DmaChannel>(json!({"signal":"TX", "request":256}))
            .is_err()
    );
    assert!(
        serde_json::from_value::<peripheral::Rcc>(
            json!({"bus_clock":"PCLK", "kernel_clock":"PCLK"})
        )
        .is_err()
    );
    let p: Peripheral = read(json!({"name":"X", "future_schema_field":17}));
    assert_eq!(encoded(p), json!({"name":"X","address":0}));
}

#[test]
fn analog_mux_is_independent_of_channel_label_and_digital_af() {
    let p: peripheral::Pin = read(json!({"pin":"PA0","signal":"IN0","adc_mux":4}));
    assert_eq!(p.af, None);
    assert_eq!(p.adc_mux, Some(4));
    assert_eq!(encoded(p), json!({"pin":"PA0","signal":"IN0","adc_mux":4}));
    assert!(
        serde_json::from_value::<peripheral::Pin>(
            json!({"pin":"PA0","signal":"IN0","adc_mux":256})
        )
        .is_err()
    );
}

#[test]
fn declaration_order_and_derived_order_remain_part_of_the_api() {
    let c: Chip = read(
        json!({"name":"N", "family":"F", "line":"L", "die":"D", "device_id":1,"packages":[],"memory":[],"docs":[],"cores":[]}),
    );
    assert_eq!(
        serde_json::to_string(&c).unwrap(),
        r#"{"name":"N","family":"F","line":"L","die":"D","device_id":1,"packages":[],"memory":[],"docs":[],"cores":[]}"#
    );
    let a = chip::memory::Access {
        read: false,
        write: true,
        execute: true,
    };
    let b = chip::memory::Access {
        read: true,
        write: false,
        execute: false,
    };
    let copied = a;
    assert_eq!(copied, a);
    assert!(a < b);
    assert!(chip::memory::Kind::Flash < chip::memory::Kind::Ram);
    assert!(StopMode::Stop1 < StopMode::Stop2 && StopMode::Stop2 < StopMode::Standby);
    assert!(
        KernelClock::Clock("Z".into())
            < KernelClock::Mux(Field {
                register: "A".into(),
                field: "A".into()
            })
    );
}

#[test]
fn regex_helper_is_hygienic_cached_and_rejects_invalid_patterns_on_use() {
    fn cached() -> &'static regex::Regex {
        cw32_data_serde::regex!(r"^P[A-Z][0-9]+$")
    }
    assert!(cached().is_match("PA10"));
    assert!(!cached().is_match("PA"));
    assert!(std::ptr::eq(cached(), cached()));
    assert!(std::panic::catch_unwind(|| cw32_data_serde::regex!("(")).is_err());
}

#[test]
fn spi_limits_are_optional_and_preserve_explicit_source_values() {
    let value = json!({"name":"SPI1", "address":0,
        "spi":{"maximum_frequency":12_000_000,"minimum_divisor":4}});
    let p: Peripheral = read(value.clone());
    assert_eq!(p.spi.as_ref().unwrap().maximum_frequency, 12_000_000);
    assert_eq!(p.spi.as_ref().unwrap().minimum_divisor, 4);
    assert_eq!(encoded(p), value);
    let null: Peripheral = read(json!({"name":"X", "spi":null}));
    assert_eq!(encoded(null), json!({"name":"X","address":0}));
    for invalid in [
        json!({"maximum_frequency":12_000_000}),
        json!({"minimum_divisor":4}),
        json!({"maximum_frequency":-1,"minimum_divisor":4}),
        json!({"maximum_frequency":12_000_000,"minimum_divisor":65_536}),
    ] {
        assert!(serde_json::from_value::<peripheral::Spi>(invalid).is_err());
    }
}

#[test]
fn clock_controls_are_optional_and_do_not_invent_a_kernel() {
    let plain: Peripheral = read(json!({"name":"UART1"}));
    assert!(plain.rcc_control.is_none());
    assert!(encoded(plain).get("rcc_control").is_none());
    let value = json!({"name":"UART1","address":0,"rcc_control":{
        "controller":"SYSCTRL","bus_clock":"PCLK",
        "enable":{"register":"APBEN2","field":"UART1"},"enable_active_value":true,
        "enable_write_key":{"field":{"register":"APBEN2","field":"KEY"},"value":23130},
        "reset":{"register":"APBRST2","field":"UART1"},"reset_asserted_value":false,
        "shared_enable_group":"fixture-shared-enable","shared_reset_group":"fixture-shared-reset"
    }});
    let p: Peripheral = read(value.clone());
    assert!(p.rcc.is_none());
    assert_eq!(
        p.rcc_control.as_ref().unwrap().reset_asserted_value,
        Some(false)
    );
    assert_eq!(encoded(p), value);
    let mut invalid = value;
    invalid["rcc_control"]["enable_active_value"] = json!(1);
    assert!(serde_json::from_value::<Peripheral>(invalid).is_err());
}

#[test]
fn mixed_register_read_only_field_schema_is_strict_and_round_trips() {
    use cw32_data_serde::register_write::FieldAccesses;
    let input: Value =
        serde_yaml::from_str(include_str!("../../cw32-data/field-access.yaml")).unwrap();
    let parsed: FieldAccesses = read(input.clone());
    assert_eq!(encoded(parsed), input);
    let mut wrong = input.clone();
    wrong["registers"]["ram_v1"][0]["access"] = json!("ReadWrite");
    assert!(serde_json::from_value::<FieldAccesses>(wrong).is_err());
    let mut wrong = input;
    wrong["registers"]["ram_v1"][0]["bit_offset"] = json!(-1);
    assert!(serde_json::from_value::<FieldAccesses>(wrong).is_err());
}

#[test]
fn lcd_qualified_ram_domains_round_trip_without_filling_holes() {
    let value = json!({"name":"LCD","lcd":{"segments":[0,35,52,55],"ram_registers":[0,8,13],"lsi_typical_hz":32800}});
    let p: Peripheral = read(value.clone());
    assert_eq!(p.lcd.as_ref().unwrap().ram_registers, vec![0, 8, 13]);
    let mut expected = value;
    expected["address"] = json!(0);
    assert_eq!(encoded(p), expected);
    assert!(
        serde_json::from_value::<peripheral::Lcd>(
            json!({"segments":[256],"ram_registers":[0],"lsi_typical_hz":32800})
        )
        .is_err()
    );
    assert!(
        serde_json::from_value::<peripheral::Lcd>(json!({"segments":[0],"lsi_typical_hz":32800}))
            .is_err()
    );
}

#[test]
fn flash_lock_mask_preserves_all_sixty_four_source_qualified_groups() {
    let limits = json!({
        "supply_mv":[1650,5500], "lock_group_bytes":4096,
        "lock_mask":u64::MAX, "has_cache_control":false,
        "maximum_hclk_hz":64_000_000, "low_voltage_threshold_mv":1800,
        "low_voltage_maximum_hclk_hz":24_000_000,
        "wait_step_hz":24_000_000, "maximum_wait_states":2
    });
    let model: peripheral::FlashLimits = read(limits.clone());
    assert_eq!(model.lock_mask, u64::MAX);
    assert_eq!(encoded(model), limits);
    let mut smaller = limits.clone();
    smaller["lock_mask"] = json!(65535);
    assert_eq!(
        encoded(read::<peripheral::FlashLimits>(smaller.clone())),
        smaller
    );
    let mut missing = limits;
    missing.as_object_mut().unwrap().remove("lock_mask");
    assert!(serde_json::from_value::<peripheral::FlashLimits>(missing).is_err());
    let p: Peripheral = read(json!({"name":"FLASH"}));
    assert!(p.flash_limits.is_none());
    assert_eq!(encoded(p), json!({"name":"FLASH","address":0}));
}

#[test]
fn rtc_source_metadata_is_optional_and_preserves_the_exact_rate() {
    let missing: Peripheral = read(json!({"name":"RTC"}));
    assert!(missing.rtc_calendar.is_none());
    let explicit_null: Peripheral = read(json!({"name":"RTC","rtc_calendar":null}));
    assert_eq!(encoded(explicit_null), json!({"name":"RTC","address":0}));
    let facts = json!({"source":"LSI","source_encoding":2,"nominal_hz":32800,
        "minimum_hz":31160,"maximum_hz":34440,"temperature_c":[-40,105],
        "supply_mv":[1650,5500],"factory_trim_address":75266,"calendar_divisor":32768,
        "prescaler_first":0,"prescaler_second":0});
    let p: Peripheral = read(json!({"name":"RTC","rtc_calendar":facts}));
    assert_eq!(encoded(p)["rtc_calendar"], facts);
    assert!(serde_json::from_value::<peripheral::RtcCalendar>(json!({"source":"LSI"})).is_err());
}

#[test]
fn trigger_register_extension_preserves_legacy_shape_and_field_widths() {
    let legacy = json!({"signal":"TRGI", "source":"BTIM1_TRGO"});
    let decoded: peripheral::Trigger = read(legacy.clone());
    assert!(decoded.registers.is_empty());
    assert_eq!(encoded(decoded), legacy);
    let route = json!({"signal":"START_CONVERSION", "source":"BTIM1_TRGO", "registers":[{
        "peripheral":"ADC", "register":"TRIGGER", "field":"BTIM1TRGO", "role":"independent_enable",
        "bit_offset":13, "bit_size":1, "value":1
    }]});
    let decoded: peripheral::Trigger = read(route.clone());
    assert_eq!(decoded.registers[0].value, 1);
    assert_eq!(encoded(decoded), route);
    for missing in [
        "peripheral",
        "register",
        "field",
        "role",
        "bit_offset",
        "bit_size",
        "value",
    ] {
        let mut invalid = route.clone();
        invalid["registers"][0]
            .as_object_mut()
            .unwrap()
            .remove(missing);
        assert!(
            serde_json::from_value::<peripheral::Trigger>(invalid).is_err(),
            "{missing}"
        );
    }
    for invalid_value in [json!(-1), json!(4294967296_u64), json!(1.25)] {
        let mut invalid = route.clone();
        invalid["registers"][0]["value"] = invalid_value;
        assert!(serde_json::from_value::<peripheral::Trigger>(invalid).is_err());
    }
}

#[test]
fn optional_hardware_facts_keep_independent_domains_and_signed_bounds() {
    let legacy: Peripheral = read(json!({"name": "GPIOA"}));
    assert!(
        legacy.gpio.is_none()
            && legacy.cordic.is_none()
            && legacy.aes.is_none()
            && legacy.trng.is_none()
            && legacy.ram_parity.is_none()
    );
    let facts = json!({"name": "X", "address": 0,
        "gpio": {"output_mask": 3, "pull_down_mask": 1},
        "gpio_interrupt": {"serviced_mask": 7, "clear_noop_mask": 65535, "level_trigger": false},
        "cordic": {"domains": [{"name": "SIGNED", "minimum": -2147483648_i64, "maximum": 2147483647_i64}]},
        "aes": {"block_words": 4, "key_words_128": 4, "key_words_192": 6, "key_words_256": 8},
        "trng": {"output_words": 2}, "ram_parity": {"enable_status": false}});
    assert_eq!(encoded(read::<Peripheral>(facts.clone())), facts);
    for invalid in [
        json!({"output_mask": 65536, "pull_down_mask": 0}),
        json!({"output_mask": 1}),
    ] {
        assert!(serde_json::from_value::<peripheral::Gpio>(invalid).is_err());
    }
    assert!(
        serde_json::from_value::<peripheral::CordicDomain>(
            json!({"name": "X", "minimum": -2147483649_i64, "maximum": 0})
        )
        .is_err()
    );
    assert!(
        serde_json::from_value::<peripheral::CordicDomain>(
            json!({"name": "X", "minimum": 0, "maximum": 2147483648_i64})
        )
        .is_err()
    );
    let explicit_nulls: Peripheral = read(
        json!({"name": "X", "gpio": null, "cordic": null, "aes": null, "trng": null, "ram_parity": null}),
    );
    assert_eq!(encoded(explicit_nulls), json!({"name": "X", "address": 0}));
}

#[test]
fn lse_hardware_policy_distinguishes_configurable_ccs_and_absent_speed() {
    let catalog: Value =
        serde_yaml::from_str(include_str!("../../cw32-data/lse-qualified.yaml")).unwrap();
    for part in ["CW32F020C6U7", "CW32L031F8U6"] {
        let value = catalog["parts"][part]["configuration"].clone();
        let parsed: peripheral::LseConfiguration = read(value.clone());
        assert_eq!(encoded(parsed), value);
        let mut missing = value.clone();
        missing.as_object_mut().unwrap().remove("configurable_ccs");
        assert!(serde_json::from_value::<peripheral::LseConfiguration>(missing).is_err());
        let mut invalid = value;
        invalid["configurable_ccs"] = json!(1);
        assert!(serde_json::from_value::<peripheral::LseConfiguration>(invalid).is_err());
    }
    let compact: peripheral::LseConfiguration =
        read(catalog["parts"]["CW32L031F8U6"]["configuration"].clone());
    assert!(
        compact.configurable_ccs
            && compact.gpio_speed_offset.is_none()
            && compact.output_routes.is_empty()
    );
    let classic: peripheral::LseConfiguration =
        read(catalog["parts"]["CW32F020C6U7"]["configuration"].clone());
    assert!(!classic.configurable_ccs && classic.gpio_speed_offset == Some(8));
}

#[test]
fn native_lse_schema_requires_explicit_monitor_facts_and_rejects_old_name() {
    let catalog: Value =
        serde_yaml::from_str(include_str!("../../cw32-data/lse-qualified.yaml")).unwrap();
    for part in ["CW32L010F8P6", "CW32L011K8T6", "CW32L012C8T6"] {
        let value = catalog["parts"][part]["configuration"].clone();
        let parsed: peripheral::LseConfiguration = read(value.clone());
        let n = parsed.native_low_power.as_ref().unwrap();
        assert_eq!(n.detector_margin_lse_edges, 1);
        let serialized = encoded(parsed);
        assert!(serialized["native_low_power"].is_object());
        assert!(serialized.get("native_l010").is_none());
        for required in ["monitor_reference", "detector_margin_lse_edges"] {
            let mut missing = value.clone();
            missing["native_low_power"]
                .as_object_mut()
                .unwrap()
                .remove(required);
            assert!(serde_json::from_value::<peripheral::LseConfiguration>(missing).is_err());
        }
        let mut old = value.clone();
        let profile = old
            .as_object_mut()
            .unwrap()
            .remove("native_low_power")
            .unwrap();
        old["native_l010"] = profile;
        assert!(serde_json::from_value::<peripheral::LseConfiguration>(old).is_err());
        if part == "CW32L010F8P6" {
            assert!(serialized["native_low_power"]["lsi_factory_trim_address"].is_null());
            let mut omitted = value;
            omitted["native_low_power"]
                .as_object_mut()
                .unwrap()
                .remove("lsi_factory_trim_address");
            let parsed: peripheral::LseConfiguration = read(omitted);
            assert!(
                parsed
                    .native_low_power
                    .unwrap()
                    .lsi_factory_trim_address
                    .is_none()
            );
        } else {
            assert_eq!(
                serialized["native_low_power"]["lsi_factory_trim_address"],
                json!(0x0010_07c2u32)
            );
        }
    }
    let old: peripheral::LseConfiguration =
        read(catalog["parts"]["CW32F020C6U7"]["configuration"].clone());
    assert!(old.native_low_power.is_none());
    assert!(encoded(old).get("native_low_power").is_none());
}
