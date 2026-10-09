//! Source-qualified electrical facts, projected without inferring IP compatibility.
use anyhow::{Context, Result, ensure};
use cw32_data_serde::chip::core::{
    Peripheral,
    peripheral::{AdcLimits, ClockLimits, FlashLimits, I2cLimits, IwdtClock},
};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::Path};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Catalog {
    schema_version: u32,
    source_authority: String,
    scope: String,
    policies: BTreeMap<String, String>,
    profiles: BTreeMap<String, Profile>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Profile {
    policy_family: String,
    factory_trim_source: TrimSource,
    clock_limits: ClockLimits,
    adc_limits: AdcLimits,
    iwdt_clock: IwdtClock,
    i2c_limits: I2cLimits,
    flash_limits: Option<FlashLimits>,
    #[serde(default)]
    hse_sources: Vec<HseSource>,
    #[serde(default)]
    pll_sources: Vec<HseSource>,
    #[serde(default)]
    hex_sources: Vec<HseSource>,
}
#[derive(Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct HseSource {
    source_ref: String,
    sha256: String,
    pdf_pages_1_based: Vec<u32>,
    printed_pages: Vec<u32>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TrimSource {
    #[serde(default, rename = "source_ref")]
    _source_ref: Option<String>,
    artifact: String,
    sha256: String,
    line_1_based: u32,
    value: u32,
    qualification: String,
}
fn policy(root: &Path, catalog: &Catalog, name: &str) -> Result<Value> {
    let path = format!("docs/{name}.json");
    let bytes = fs::read(root.join(&path))?;
    let expected = catalog
        .policies
        .get(&path)
        .context("missing electrical source policy lock")?;
    ensure!(
        format!("{:x}", Sha256::digest(&bytes)) == *expected,
        "electrical source policy changed: {path}"
    );
    Ok(serde_json::from_slice(&bytes)?)
}
fn family<'a>(policy: &'a Value, line: &str) -> Result<&'a Value> {
    policy["families"]
        .as_array()
        .and_then(|v| v.iter().find(|f| f["family"] == line))
        .or_else(|| policy["families"].get(line))
        .context("missing own-family electrical policy")
}
fn number(value: &Value) -> Result<u32> {
    Ok(value
        .as_u64()
        .context("missing electrical number")?
        .try_into()?)
}
fn equal(value: u32, source: &Value) -> Result<()> {
    ensure!(
        value == number(source)?,
        "electrical metadata differs from reviewed source policy"
    );
    Ok(())
}
pub fn apply(root: &Path, path: &str, line: &str, peripherals: &mut [Peripheral]) -> Result<()> {
    let catalog: Catalog = crate::read_yaml(root.join(path))?;
    ensure!(
        catalog.schema_version == 1 && !catalog.scope.is_empty(),
        "invalid electrical catalog"
    );
    ensure!(
        catalog.source_authority == "sources/evidence-sources.json",
        "unrecognized original-source authority"
    );
    let p = catalog
        .profiles
        .get(line)
        .context("missing electrical chip profile")?;
    ensure!(
        p.policy_family == line,
        "electrical facts must use their own family"
    );
    let adc_policy = policy(root, &catalog, "adc-clock-source-bounds")?;
    let envelope_policy = policy(root, &catalog, "rcc-operating-envelope")?;
    let watchdog_policy = policy(root, &catalog, "watchdog-evidence")?;
    let i2c_policy = policy(root, &catalog, "i2c-clock-bounds-sources")?;
    let flash_policy = policy(root, &catalog, "flash-next-batch-audit")?;
    let corrections = policy(root, &catalog, "flash-lock-access-corrections")?;
    let a = family(&adc_policy, line)?;
    let r = family(&envelope_policy, line)?;
    let w = family(&watchdog_policy, line)?;
    let i = family(&i2c_policy, line)?;
    let c = &p.clock_limits;
    let mut hex_routes = Vec::new();
    let lock: Value = serde_json::from_slice(&fs::read(root.join(&catalog.source_authority))?)?;
    let lse_path = "cw32-data/lse-ownership.yaml";
    let lse_bytes = fs::read(root.join(lse_path))?;
    ensure!(
        catalog.policies.get(lse_path) == Some(&format!("{:x}", Sha256::digest(&lse_bytes))),
        "LSE ownership source policy changed"
    );
    let lse_review: Value = crate::read_yaml(root.join(lse_path))?;
    let own_lse = family(&lse_review, line)?;
    ensure!(
        serde_json::to_value(&c.lse)? == own_lse["lse"],
        "LSE ownership facts differ from own-source review"
    );
    if let Some(lse) = &c.lse {
        ensure!(
            lse.pin_lock || !lse.pin_lock_requires_enable_lock,
            "absent LSE pad lock cannot require an enable lock"
        );
    }
    let evidence_path = lse_review["source_evidence"]
        .as_str()
        .context("missing LSE evidence")?;
    let evidence_bytes = fs::read(root.join(evidence_path))?;
    ensure!(
        lse_review["source_evidence_sha256"] == format!("{:x}", Sha256::digest(&evidence_bytes)),
        "LSE source evidence changed"
    );
    let lse_evidence: Value = serde_json::from_slice(&evidence_bytes)?;
    for kind in ["manual_source", "datasheet_source"] {
        let path = own_lse[kind].as_str().context("missing own LSE source")?;
        let original = lock["artifacts"]
            .as_array()
            .context("missing sources")?
            .iter()
            .find(|a| a["path"] == path)
            .context("LSE source is not canonical")?;
        ensure!(
            original["sha256"] == lse_evidence["sources"][path]["sha256"]
                && original["provenance"]["status"] == "selected"
                && original["provenance"]["chip_scope"]
                    .as_array()
                    .context("missing chip scope")?
                    .iter()
                    .any(|f| f == line),
            "LSE source is not selected own-family evidence"
        );
    }
    if let Some(pll) = &c.pll {
        // Qualification is explicit and own-family; shared register layout is insufficient.
        ensure!(
            line == "CW32L083" && pll.hsi_supported,
            "unqualified PLL family/source"
        );
        let review_path = "cw32-data/pll-qualified.yaml";
        let review_bytes = fs::read(root.join(review_path))?;
        ensure!(
            catalog.policies.get(review_path)
                == Some(&format!("{:x}", Sha256::digest(&review_bytes))),
            "PLL source policy changed"
        );
        let review: Value = crate::read_yaml(root.join(review_path))?;
        let own = family(&review, line)?;
        ensure!(
            serde_json::to_value(pll)? == own["pll"]
                && serde_json::to_value(&p.pll_sources)? == own["sources"],
            "PLL facts or source attribution differ from own-source review"
        );
        ensure!(
            p.pll_sources.len() == 2,
            "PLL requires own manual and datasheet"
        );
        for source in &p.pll_sources {
            let original = lock["artifacts"]
                .as_array()
                .context("missing sources")?
                .iter()
                .find(|a| a["id"] == source.source_ref)
                .context("PLL source_ref is not canonical")?;
            ensure!(
                original["sha256"] == source.sha256
                    && original["provenance"]["status"] == "selected"
                    && original["provenance"]["chip_scope"]
                        .as_array()
                        .context("missing chip scope")?
                        .iter()
                        .any(|f| f == line),
                "PLL source is not selected own-family evidence"
            );
            let page_count = original["provenance"]["pdf_page_count"]
                .as_u64()
                .context("missing PDF length")?;
            ensure!(
                !source.pdf_pages_1_based.is_empty()
                    && source.pdf_pages_1_based.len() == source.printed_pages.len()
                    && source
                        .pdf_pages_1_based
                        .iter()
                        .all(|p| *p > 0 && u64::from(*p) <= page_count),
                "invalid PLL evidence pages"
            );
        }
        ensure!(
            pll.input_range_hz.0 > 0
                && pll.input_range_hz.0 < pll.input_range_hz.1
                && pll.output_range_hz.0 > 0
                && pll.output_range_hz.0 < pll.output_range_hz.1,
            "invalid PLL electrical intervals"
        );
        ensure!(
            pll.input_ranges_hz
                .iter()
                .chain(pll.output_ranges_hz.iter())
                .all(|(lo, hi)| *lo > 0 && lo < hi),
            "invalid PLL analog bins"
        );
        ensure!(
            pll.supply_mv.0 <= c.hsi_supply_mv.0
                && pll.supply_mv.1 >= c.hsi_supply_mv.1
                && pll.temperature_c.0 <= c.hsi_temperature_c.0
                && pll.temperature_c.1 >= c.hsi_temperature_c.1,
            "PLL conditions do not cover HSI qualification"
        );
    } else {
        ensure!(
            p.pll_sources.is_empty(),
            "PLL sources require qualified PLL metadata"
        );
    }
    // Presence is part of the locked source policy, independent of whether
    // the authored HSE subtree is present. Missing family/field is Null, so
    // unqualified families cannot attach a range or an HSE configuration.
    let hse_review_path = "cw32-data/hse-qualified.yaml";
    let hse_review_bytes = fs::read(root.join(hse_review_path))?;
    ensure!(
        catalog.policies.get(hse_review_path)
            == Some(&format!("{:x}", Sha256::digest(&hse_review_bytes))),
        "HSE source policy changed"
    );
    let hse_review: Value = crate::read_yaml(root.join(hse_review_path))?;
    let own_clock_policy = &hse_review["families"][line];
    ensure!(
        serde_json::to_value(&c.hse)? == own_clock_policy["hse"],
        "HSE presence or facts differ from own-source review"
    );
    ensure!(
        serde_json::to_value(c.hsi_operating_range_hz)?
            == own_clock_policy["hsi_operating_range_hz"],
        "HSIOSC operating-range presence or facts differ from own-source review"
    );
    if let Some((minimum, maximum)) = c.hsi_operating_range_hz {
        ensure!(
            minimum > 0 && minimum <= maximum,
            "invalid HSIOSC operating range"
        );
    }
    if let Some(hse) = &c.hse {
        let own = family(&hse_review, line)?;
        if let Some(divisor) = hse.fixed_ccs_hsi_divisor {
            ensure!(
                divisor > 0 && c.hsi_frequency_hz % u32::from(divisor) == 0,
                "invalid fixed CCS HSIOSC divisor"
            );
        }
        ensure!(
            serde_json::to_value(&p.hse_sources)? == own["sources"],
            "HSE source attribution changed"
        );
        ensure!(
            p.hse_sources.len() == 2,
            "HSE needs own manual and datasheet"
        );
        for source in &p.hse_sources {
            let original = lock["artifacts"]
                .as_array()
                .context("missing sources")?
                .iter()
                .find(|a| a["id"] == source.source_ref)
                .context("HSE source_ref is not canonical")?;
            ensure!(
                original["sha256"] == source.sha256
                    && original["provenance"]["status"] == "selected"
                    && original["provenance"]["chip_scope"]
                        .as_array()
                        .context("missing chip scope")?
                        .iter()
                        .any(|f| f == line),
                "HSE source is not selected own-family evidence"
            );
            let page_count = original["provenance"]["pdf_page_count"]
                .as_u64()
                .context("missing PDF length")?;
            ensure!(
                !source.pdf_pages_1_based.is_empty()
                    && source.pdf_pages_1_based.len() == source.printed_pages.len()
                    && source
                        .pdf_pages_1_based
                        .iter()
                        .all(|p| *p > 0 && u64::from(*p) <= page_count),
                "invalid HSE evidence pages"
            );
        }
        ensure!(
            hse.ccs_requires_lsi
                && hse.ccs_numerator_hz
                    > u64::from(hse.ccs_cycle_count) * u64::from(hse.ccs_lsi_maximum_hz),
            "HSE detector has no margin at the legal unchanged LSI maximum"
        );
        for source in [&hse.crystal, &hse.bypass] {
            ensure!(
                source.minimum_hz > 0
                    && source.minimum_hz <= source.maximum_hz
                    && source.supply_mv.0 <= source.supply_mv.1
                    && source.temperature_c.0 <= source.temperature_c.1,
                "invalid HSE qualification envelope"
            );
            let maximum_count = hse.ccs_numerator_hz.div_ceil(u64::from(source.minimum_hz));
            ensure!(
                maximum_count > 0 && maximum_count <= u64::from(hse.ccs_maximum_count),
                "HSE detector count does not fit the qualified range"
            );
        }
    } else {
        ensure!(
            p.hse_sources.is_empty(),
            "HSE sources without qualified hardware facts"
        );
    }
    if let Some(hex) = &c.hex {
        ensure!(
            matches!(line, "CW32F002" | "CW32F003") && c.hse.is_none(),
            "direct HEX must have its own F002/F003 qualification"
        );
        let review_path = "cw32-data/hex-qualified.yaml";
        let review_bytes = fs::read(root.join(review_path))?;
        ensure!(
            catalog.policies.get(review_path)
                == Some(&format!("{:x}", Sha256::digest(&review_bytes))),
            "HEX source policy changed"
        );
        let review: Value = crate::read_yaml(root.join(review_path))?;
        let own = family(&review, line)?;
        ensure!(
            serde_json::to_value(hex)? == own["hex"],
            "HEX facts differ from own-source review"
        );
        ensure!(
            serde_json::to_value(&p.hex_sources)? == own["sources"],
            "HEX source attribution changed"
        );
        ensure!(
            p.hex_sources.len() == 2,
            "HEX needs own manual and datasheet"
        );
        for source in &p.hex_sources {
            let original = lock["artifacts"]
                .as_array()
                .context("missing sources")?
                .iter()
                .find(|a| a["id"] == source.source_ref)
                .context("HEX source_ref is not canonical")?;
            ensure!(
                original["sha256"] == source.sha256
                    && original["provenance"]["status"] == "selected"
                    && original["provenance"]["chip_scope"]
                        .as_array()
                        .context("missing chip scope")?
                        .iter()
                        .any(|f| f == line),
                "HEX source is not selected own-family evidence"
            );
            let pages = original["provenance"]["pdf_page_count"]
                .as_u64()
                .context("missing PDF length")?;
            ensure!(
                !source.pdf_pages_1_based.is_empty()
                    && source.pdf_pages_1_based.len() == source.printed_pages.len()
                    && source
                        .pdf_pages_1_based
                        .iter()
                        .all(|p| *p > 0 && u64::from(*p) <= pages),
                "invalid HEX evidence pages"
            );
        }
        hex_routes = serde_json::from_value::<Vec<cw32_data_serde::chip::core::peripheral::Pin>>(
            own["routes"].clone(),
        )?;
        ensure!(
            hex_routes.len() == 2
                && hex_routes.iter().enumerate().all(|(index, pin)| pin.pin
                    == format!("PB{index}")
                    && pin.signal == format!("HEX_PB0{index}")
                    && pin.af.is_none()
                    && pin.adc_mux.is_none()
                    && pin.comparator_mux.is_none()),
            "HEX routes disagree with own-manual PINMUX"
        );
        let input = &hex.input;
        ensure!(
            input.minimum_hz > 0
                && input.minimum_hz <= input.maximum_hz
                && input.supply_mv.0 <= input.supply_mv.1
                && input.temperature_c.0 <= input.temperature_c.1,
            "invalid HEX qualification envelope"
        );
        ensure!(
            hex.duty_percent.0 > 0
                && hex.duty_percent.0 <= hex.duty_percent.1
                && hex.duty_percent.1 < 100
                && hex.minimum_high_low_ns > 0
                && hex.maximum_rise_fall_ns > 0,
            "invalid HEX waveform limits"
        );
    } else {
        ensure!(
            p.hex_sources.is_empty(),
            "HEX sources without qualified hardware facts"
        );
    }
    let trim = &p.factory_trim_source;
    ensure!(
        trim.value == c.factory_hsi_trim_address
            && trim.line_1_based > 0
            && !trim.qualification.is_empty(),
        "unqualified factory trim address"
    );
    ensure!(
        lock["artifacts"]
            .as_array()
            .context("missing original sources")?
            .iter()
            .flat_map(|a| a["members"].as_array().into_iter().flatten())
            .any(|m| m["path"] == trim.artifact && m["sha256"] == trim.sha256),
        "factory trim source is outside original-source lock"
    );
    let supported = a["hsi"]["supported_divisors"]
        .as_array()
        .context("missing HSI divider qualification")?;
    ensure!(
        supported
            .iter()
            .any(|v| v.as_u64() == Some(u64::from(c.default_hsi_divisor))),
        "default divider outside qualified hardware choices"
    );

    equal(c.hsi_frequency_hz, &a["hsi"]["nominal_oscillator_hz"])?;
    equal(
        c.hsi_error_percent * 10_000,
        &a["hsi"]["factory_error_bound_ppm"],
    )?;
    ensure!(
        serde_json::to_value(c.hsi_supply_mv)? == r["factory_hsi"]["supply_mv"],
        "HSI supply qualification changed"
    );
    ensure!(
        serde_json::to_value(c.hsi_temperature_c)? == r["factory_hsi"]["temperature_ambient_c"],
        "HSI ambient qualification changed"
    );
    equal(c.high_voltage_bus_max_hz, &r["absolute_rated_bus_max_hz"])?;
    equal(c.initial_flash_wait, &r["flash"]["candidate_initial_wait"])?;
    ensure!(
        c.flash_wait_step_hz != 0
            && r["flash"]["wait_clock_ceilings_hz"]
                .as_array()
                .context("missing WAIT table")?
                .iter()
                .enumerate()
                .all(|(i, v)| v.as_u64() == Some(u64::from(c.flash_wait_step_hz) * (i as u64 + 1))),
        "FLASH WAIT quantum changed"
    );
    for band in r["voltage_bus_limits"]
        .as_array()
        .context("missing bus voltage limits")?
    {
        let below = number(&band["supply_min_mv"])? < u32::from(c.low_voltage_threshold_mv);
        equal(
            if below {
                c.low_voltage_bus_max_hz
            } else {
                c.high_voltage_bus_max_hz
            },
            &band["hclk_max_hz"],
        )?;
        if below {
            equal(
                u32::from(c.low_voltage_threshold_mv),
                &band["supply_max_exclusive_mv"],
            )?;
        }
    }
    let limits = &p.adc_limits;
    ensure!(
        limits.supply_mv == c.hsi_supply_mv,
        "ADC supply qualification changed"
    );
    ensure!(
        serde_json::to_value(&limits.sample_cycles)? == a["adc"]["sample_cycles"],
        "ADC sample cycles changed"
    );
    equal(
        u32::from(limits.comparison_cycles),
        &a["adc"]["comparison_cycles"],
    )?;
    let low = a["adc"]["bands"].is_array();
    let bands = a["adc"][if low {
        "bands"
    } else {
        "supply_reference_bands"
    }]
    .as_array()
    .context("missing ADC band policy")?;
    ensure!(
        bands.len() == limits.supply_bands.len(),
        "ADC voltage bands changed"
    );
    for (b, s) in limits.supply_bands.iter().zip(bands) {
        equal(
            u32::from(b.supply_min_mv),
            &s[if low {
                "supply_lower_mv"
            } else {
                "supply_min_mv"
            }],
        )?;
        equal(
            b.maximum_clock_hz,
            &s[if low {
                "max_clock_hz"
            } else {
                "actual_adcclk_max_hz"
            }],
        )?;
        equal(
            b.maximum_sample_rate_hz,
            &s[if low {
                "max_sample_rate_hz"
            } else {
                "conversion_rate_max_sps"
            }],
        )?;
        if low {
            equal(b.minimum_acquisition_ps, &s["min_acquisition_ps"])?;
        }
    }
    ensure!(
        limits.minimum_clock_hz == a["adc"]["min_clock_hz"].as_u64().unwrap_or(0) as u32,
        "ADC minimum clock changed"
    );
    if low {
        ensure!(
            limits.internal_acquisition_ps == 40_000_000,
            "low ADC internal acquisition qualification changed"
        );
        let audit = if line == "CW32L012" {
            "docs/adc-l012-dual-ownership.md"
        } else {
            "docs/adc-low-sequence-audit.md"
        };
        let bytes = fs::read(root.join(audit))?;
        ensure!(
            Some(&format!("{:x}", Sha256::digest(&bytes))) == catalog.policies.get(audit),
            "internal acquisition source audit changed"
        );
    }
    if !low {
        equal(
            limits.input_follower_maximum_rate_hz,
            &a["adc"]["input_follower_rate_max_sps"],
        )?;
        for (name, bands) in [
            ("Internal1V5", &limits.internal_1v5_bands),
            ("Internal2V5", &limits.internal_2v5_bands),
        ] {
            let source: Vec<_> = a["adc"]["internal_reference_limits"]
                .as_array()
                .context("missing internal reference policy")?
                .iter()
                .filter(|b| b["reference"] == name)
                .collect();
            ensure!(
                source.len() == bands.len(),
                "ADC internal reference bands changed"
            );
            for (b, s) in bands.iter().zip(source) {
                equal(u32::from(b.supply_min_mv), &s["supply_min_mv"])?;
                equal(b.maximum_clock_hz, &s["max_adc_hz"])?;
                equal(b.maximum_sample_rate_hz, &s["max_sps"])?;
            }
        }
    }
    equal(p.iwdt_clock.typical_hz, &w["clock_typical_hz"])?;
    equal(p.iwdt_clock.minimum_hz, &w["clock_min_hz"])?;
    equal(p.iwdt_clock.maximum_hz, &w["clock_max_hz"])?;
    ensure!(
        p.iwdt_clock.uses_lsi == (w["clock_source"] == "LSI"),
        "IWDT clock source changed"
    );
    equal(p.i2c_limits.maximum_frequency_hz, &i["maximum_scl_hz"])?;
    if let Some(waveform) = &p.i2c_limits.waveform {
        ensure!(line == "CW32L012", "waveform proof is L012-only");
        ensure!(
            waveform.maximum_frequency_hz == [100_000, 400_000, p.i2c_limits.maximum_frequency_hz],
            "I2C mode bounds changed"
        );
        ensure!(
            serde_json::to_value(waveform.low)? == i2c_policy["l012_limits"]["solver_ns"]["low"],
            "I2C low qualification changed"
        );
        ensure!(
            serde_json::to_value(waveform.high)? == i2c_policy["l012_limits"]["solver_ns"]["high"],
            "I2C high qualification changed"
        );
        ensure!(
            serde_json::to_value(waveform.hold)? == i2c_policy["l012_limits"]["solver_ns"]["hold"],
            "I2C hold qualification changed"
        );
        ensure!(
            serde_json::to_value(waveform.setup)?
                == i2c_policy["l012_limits"]["solver_ns"]["setup"],
            "I2C setup qualification changed"
        );
        ensure!(
            serde_json::to_value(waveform.data_setup)?
                == i2c_policy["l012_limits"]["solver_ns"]["data_setup"],
            "I2C data_setup qualification changed"
        );
        ensure!(
            serde_json::to_value(waveform.data_valid)?
                == i2c_policy["l012_limits"]["solver_ns"]["data_valid"],
            "I2C data_valid qualification changed"
        );
        ensure!(
            serde_json::to_value(waveform.rise)? == i2c_policy["l012_limits"]["solver_ns"]["rise"],
            "I2C rise qualification changed"
        );
    }
    if let Some(f) = &p.flash_limits {
        let s = family(&flash_policy, line)?;
        // L01x specifies FLASH under its own general working conditions, with
        // no separate Vprog row. This is not a borrowed legacy-family voltage.
        let voltages = s["electrical"]["program_voltage_range_V"]
            .as_array()
            .or_else(|| {
                matches!(line, "CW32L010" | "CW32L011" | "CW32L012")
                    .then(|| s["electrical"]["general_VDD_range_V"].as_array())
                    .flatten()
            })
            .context("missing own FLASH supply policy")?;
        // Retain stricter qualified API floors: W031's 2.0 V requirement
        // covers both RF DCDC and LDO modes; its Vprog row alone starts at 1.8 V.
        let source_min = (voltages[0].as_f64().unwrap() * 1000.0).round() as u16;
        let source_max = (voltages[1].as_f64().unwrap() * 1000.0).round() as u16;
        ensure!(
            f.supply_mv.0 >= source_min
                && f.supply_mv.1 <= source_max
                && f.supply_mv.0 <= f.supply_mv.1,
            "FLASH Vprog qualification broadened"
        );
        ensure!(
            line != "CW32W031" || f.supply_mv.0 == 2000,
            "W031 dual-RF-mode FLASH minimum must remain 2.0 V; see locked storage policy"
        );
        let storage_policy = fs::read(root.join("docs/flash-storage.md"))?;
        ensure!(
            Some(&format!("{:x}", Sha256::digest(&storage_policy)))
                == catalog.policies.get("docs/flash-storage.md"),
            "conservative FLASH qualification policy changed"
        );
        ensure!(
            f.low_voltage_threshold_mv == c.low_voltage_threshold_mv
                && f.low_voltage_maximum_hclk_hz == c.low_voltage_bus_max_hz
                && f.wait_step_hz == c.flash_wait_step_hz
                && f.maximum_wait_states == c.initial_flash_wait,
            "FLASH bus or WAIT qualification changed"
        );
        equal(f.lock_group_bytes, &s["protection"]["lock_group_bytes"])?;
        let corrected = corrections["corrections"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["family"] == line);
        let bits = corrected
            .map(|s| s["retained_lock_bits"].as_array().unwrap().len() as u32)
            .unwrap_or(number(&s["protection"]["lock_bits_documented"])?);
        ensure!((1..=64).contains(&bits), "FLASH lock count outside u64");
        ensure!(
            f.lock_mask == u64::MAX >> (64 - bits),
            "FLASH lock mask changed"
        );
        ensure!(
            f.has_cache_control == !s["registers"]["CR2_FETCH_bit"].is_null(),
            "FLASH cache control changed"
        );
        ensure!(
            u64::from(f.maximum_hclk_hz)
                == s["electrical"]["max_device_HCLK_MHz"].as_u64().unwrap() * 1_000_000,
            "FLASH HCLK limit changed"
        );
    }
    let mut counts = [0; 5];
    for peripheral in peripherals {
        match peripheral.registers.as_ref().map(|r| r.kind.as_str()) {
            Some("sysctrl") => {
                peripheral.clock_limits = Some(p.clock_limits.clone());
                peripheral.pins.extend(hex_routes.clone());
                counts[0] += 1;
            }
            Some("adc") => {
                peripheral.adc_limits = Some(p.adc_limits.clone());
                counts[1] += 1;
            }
            Some("iwdt") => {
                peripheral.iwdt_clock = Some(p.iwdt_clock.clone());
                counts[2] += 1;
            }
            Some("i2c") => {
                peripheral.i2c_limits = Some(p.i2c_limits.clone());
                counts[3] += 1;
            }
            Some("flash") => {
                peripheral.flash_limits = p.flash_limits.clone();
                counts[4] += 1;
            }
            _ => {}
        }
    }
    ensure!(
        counts[0] == 1 && counts[1] > 0 && counts[2] == 1 && counts[3] > 0 && counts[4] == 1,
        "electrical metadata missing hardware owner"
    );
    Ok(())
}
