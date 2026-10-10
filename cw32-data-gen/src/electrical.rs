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
fn validate_pll_source_selection(line: &str, sources: &[HseSource]) -> Result<()> {
    // Exact selected identities matter: the historical F020 file has the same filename
    // but a different printed revision. The common x030 manual explicitly covers A030.
    let expected = match line {
        "CW32L083" => [
            "vendor:CW32L083_UserManual_CN_V2.0.pdf",
            "vendor:CW32L083_DataSheet_CN_V1.9.pdf",
        ],
        "CW32F020" => [
            "vendor:CW32F020_UserManual_CN_V1.4.pdf",
            "vendor:current-datasheets/CW32F020_DataSheet_CN_V1.3.pdf",
        ],
        "CW32F030" => [
            "vendor:CW32x030_UserManual_CN_V2.5.pdf",
            "vendor:CW32F030_DataSheet_CN_V1.9.pdf",
        ],
        "CW32A030" => [
            "vendor:CW32x030_UserManual_CN_V2.5.pdf",
            "vendor:CW32A030_DataSheet_CN_V1.1.pdf",
        ],
        _ => anyhow::bail!("unqualified PLL family/source"),
    };
    ensure!(
        sources.len() == expected.len()
            && sources
                .iter()
                .zip(expected)
                .all(|(source, expected)| source.source_ref == expected),
        "PLL requires the exact selected own manual and datasheet"
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
    ensure!(
        c.lse_configuration.is_none(),
        "family electrical profiles cannot preseed active LSE qualification"
    );
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
        validate_pll_source_selection(line, &p.pll_sources)?;
        ensure!(pll.hsi_supported, "unqualified PLL source");
        let review_path = "cw32-data/pll-qualified.yaml";
        let review_bytes = fs::read(root.join(review_path))?;
        ensure!(
            catalog.policies.get(review_path)
                == Some(&format!("{:x}", Sha256::digest(&review_bytes))),
            "PLL source policy changed"
        );
        let review: Value = crate::read_yaml(root.join(review_path))?;
        let own = family(&review, line)?;
        if line != "CW32L083" {
            let receipt_path = "sources/x030-f020-hsi-pll-source-receipt.json";
            let receipt_bytes = fs::read(root.join(receipt_path))?;
            ensure!(
                review["source_evidence"] == receipt_path
                    && review["source_evidence_sha256"]
                        == format!("{:x}", Sha256::digest(&receipt_bytes)),
                "PLL own-source receipt changed"
            );
            let receipt: Value = serde_json::from_slice(&receipt_bytes)?;
            ensure!(
                receipt["profiles"][line]["manual_source"] == p.pll_sources[0].source_ref
                    && receipt["profiles"][line]["datasheet_source"] == p.pll_sources[1].source_ref,
                "PLL receipt does not identify the selected own sources"
            );
        }
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

// Active LSE qualification is deliberately applied after exact-package expansion.
// Family clock profiles can carry pad ownership, never oscillator qualification.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LseCatalog {
    schema_version: u32,
    source_authority: String,
    policies: BTreeMap<String, String>,
    parts: BTreeMap<String, LseProfile>,
}
#[derive(Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct LseProfile {
    family: String,
    package: String,
    input_pin: String,
    output_pin: String,
    sources: Vec<LseSource>,
    configuration: cw32_data_serde::chip::core::peripheral::LseConfiguration,
}
#[derive(Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct LseSource {
    citation_ref: String,
    source_ref: String,
    sha256: String,
    pdf_pages_1_based: Vec<u32>,
    printed_pages: Vec<u32>,
}
/// Decode through Value first: its mapping visitor rejects duplicate keys at
/// every depth before typed BTreeMap fields could overwrite a prior entry.
fn parse_lse_catalog(bytes: &[u8]) -> Result<LseCatalog> {
    let unique: serde_yaml::Value = serde_yaml::from_slice(bytes)?;
    Ok(serde_yaml::from_value(unique)?)
}
const LSE_PARTS: [(&str, &str, &str); 19] = [
    ("CW32A030C8T7", "CW32A030", "LQFP48"),
    ("CW32F030C8T7", "CW32F030", "LQFP48"),
    ("CW32F020C6U7", "CW32F020", "QFN48"),
    ("CW32L031C8T6", "CW32L031", "LQFP48"),
    ("CW32L031C8U6", "CW32L031", "QFN48"),
    ("CW32L031F8U6", "CW32L031", "QFN20"),
    ("CW32R031C8U6", "CW32R031", "QFN48"),
    ("CW32W031R8U6", "CW32W031", "QFN64"),
    ("CW32L052C8T6", "CW32L052", "LQFP48"),
    ("CW32L052R8S6", "CW32L052", "LQFP64（7×7mm）"),
    ("CW32L052R8T6", "CW32L052", "LQFP64（10×10mm）"),
    ("CW32L083RBT6", "CW32L083", "LQFP64（10×10mm）"),
    ("CW32L083RCT6", "CW32L083", "LQFP64（10×10mm）"),
    ("CW32L083RCS6", "CW32L083", "LQFP64（7×7mm）"),
    ("CW32L083MCT6", "CW32L083", "LQFP80"),
    ("CW32L083VCT6", "CW32L083", "LQFP100"),
    ("CW32L010F8P6", "CW32L010", "TSSOP20"),
    ("CW32L010F8U6", "CW32L010", "QFN20"),
    ("CW32L010Y8M6", "CW32L010", "SOP16"),
];
// Re-read from x030 RM Rev2.5 PDF187–195 and own F020 RM Rev1.4 PDF184–192. Values
// and masks are observations, not writes. KEY and ICR are never included.
const LSE_RTC_RESET: [(&str, u32, u32, u32); 13] = [
    ("CR0", 4, 0, 0xef),
    ("CR1", 8, 0, 0x703),
    ("CR2", 12, 0, 0x6ff),
    ("COMPEN", 16, 0, 0xffff),
    ("DATE", 20, 0, 0x07ff_ffff),
    ("TIME", 24, 0x0012_0000, 0x003f_7f7f),
    ("ALARMA", 28, 0x0012_0000, 0x7fbf_ffff),
    ("ALARMB", 32, 0x0012_0000, 0x7fbf_ffff),
    ("TAMPDATE", 36, 0, 0xff3f),
    ("TAMPTIME", 40, 0, 0x003f_7f7f),
    ("AWTARR", 44, 0xffff, 0xffff),
    ("IER", 48, 0, 0x5f),
    ("ISR", 52, 0, 0x5f),
];
fn validate_lse_parts(catalog: &LseCatalog) -> Result<()> {
    ensure!(
        catalog.parts.len() == LSE_PARTS.len()
            && LSE_PARTS.iter().all(|(part, family, package)| catalog
                .parts
                .get(*part)
                .is_some_and(|p| p.family == *family
                    && p.package == *package
                    && p.input_pin == if *family == "CW32L010" { "PB1" } else { "PC14" }
                    && p.output_pin == if *family == "CW32L010" { "PB0" } else { "PC15" })),
        "active LSE qualification must be present for exactly the reviewed exact parts"
    );
    Ok(())
}
// Native L010 source-zero admission masks. These are observations, not proof of
// power-on reset; H24, WINDOW and stored calendar/prescaler values are excluded.
const LSE_L010_RTC_ADMISSION: [(&str, u32, u32, u32); 6] = [
    ("CR0", 4, 0, 0xe7),
    ("CR1", 8, 0, 0x705),
    ("CR2", 12, 0, 0x6ff),
    ("COMPCFR1", 16, 0, 0xffff),
    ("IER", 48, 0, 0x5f),
    ("ISR", 52, 0, 0x5f),
];
fn validate_l010_lse_configuration(
    part: &str,
    c: &cw32_data_serde::chip::core::peripheral::LseConfiguration,
) -> Result<()> {
    let n = c
        .native_l010
        .as_ref()
        .context("L010 requires own native LSE facts")?;
    ensure!(
        matches!(part, "CW32L010F8P6" | "CW32L010F8U6" | "CW32L010Y8M6")
            && c.nominal_hz == 32_768
            && c.maximum_hz == 100_000
            && c.configurable_ccs
            && c.supply_mv == (1620, 5500)
            && c.temperature_c == (-40, 85)
            && c.startup_cycles == [256, 1024, 4096, 16384]
            && (c.rtc_source, c.uart_source, c.mco_source) == (0, 2, 6)
            && c.awt_source.is_none()
            && c.startup_consumers.is_none()
            && c.gpio_dir_offset == 0
            && c.gpio_speed_offset.is_none()
            && c.output_routes.is_empty()
            && n.drive_bits == 4
            && n.startup_drive_bits == 4
            && n.monitored_lsi_maximum_hz == 36_080
            && n.detector_lse_edges == 128
            && n.detector_lsi_cycles == 256
            && (
                n.rtc_first_divisor,
                n.rtc_second_divisor,
                n.rtc_calendar_divisor
            ) == (1, 16384, 32768)
            && n.rtc_output_routes
                .iter()
                .map(|r| (r.pin.as_str(), r.af))
                .collect::<Vec<_>>()
                == [("PB4", 2), ("PB6", 2)],
        "L010 native electrical, monitor, drive, selector or calendar facts differ from own source"
    );
    ensure!(
        c.rtc_reset.len() == LSE_L010_RTC_ADMISSION.len()
            && LSE_L010_RTC_ADMISSION.iter().zip(&c.rtc_reset).all(
                |((name, offset, value, mask), r)| r.register == *name
                    && r.byte_offset == *offset
                    && r.value == *value
                    && r.mask == *mask
            ),
        "L010 requires the exact six-row source-zero admission image including ISR"
    );
    Ok(())
}
fn validate_lse_configuration(
    part: &str,
    c: &cw32_data_serde::chip::core::peripheral::LseConfiguration,
) -> Result<()> {
    if part.starts_with("CW32L010") {
        return validate_l010_lse_configuration(part, c);
    }
    ensure!(
        c.native_l010.is_none(),
        "native L010 facts cannot qualify another family"
    );
    ensure!(
        c.nominal_hz == 32_768
            && c.maximum_hz == 1_000_000
            && c.supply_mv.0 > 0
            && c.supply_mv.0 <= c.supply_mv.1
            && c.temperature_c.0 <= c.temperature_c.1
            && c.startup_cycles == [256, 1024, 4096, 16384]
            && (c.rtc_source, c.uart_source, c.mco_source) == (0, 2, 6)
            && c.awt_source
                == if part.starts_with("CW32L052") || part.starts_with("CW32L083") {
                    None
                } else {
                    Some(3)
                }
            && c.gpio_dir_offset == 0
            && c.gpio_speed_offset == if c.configurable_ccs { None } else { Some(8) },
        "LSE electrical/source/GPIO facts exceed the bounded own-source cohort"
    );
    if part.starts_with("CW32L052") {
        let native = c
            .startup_consumers
            .as_ref()
            .context("L052 requires own native startup/consumer facts")?;
        let expected_routes: Vec<_> = if part == "CW32L052C8T6" {
            vec![]
        } else {
            vec![("PC4", 6)]
        };
        ensure!(
            native.startup_analog
                && native.autotrim_source == 3
                && native.lptim.source == 2
                && native.lptim.gate_controls_work
                && native.lcd.source == 1
                && native.lcd.gate_controls_work
                && native.uarts == ["UART1", "UART2", "UART3"]
                && native
                    .lsi_output_routes
                    .iter()
                    .map(|r| (r.pin.as_str(), r.af))
                    .collect::<Vec<_>>()
                    == expected_routes,
            "L052 native startup/consumer facts differ from the own-source exact-package roster"
        );
    } else if part.starts_with("CW32L083") {
        let native = c
            .startup_consumers
            .as_ref()
            .context("L083 requires own native consumer facts")?;
        let expected_routes = match part {
            "CW32L083RBT6" | "CW32L083RCT6" | "CW32L083RCS6" => vec![("PC4", 6)],
            "CW32L083MCT6" => vec![("PC4", 6), ("PF2", 4)],
            "CW32L083VCT6" => vec![("PC4", 6), ("PD5", 6), ("PF2", 4)],
            _ => anyhow::bail!("unqualified L083 part"),
        };
        ensure!(
            !native.startup_analog
                && native.autotrim_source == 3
                && native.lptim.source == 2
                && native.lptim.gate_controls_work
                && native.lcd.source == 1
                && native.lcd.gate_controls_work
                && native.uarts == ["UART1", "UART2", "UART3", "UART4", "UART5", "UART6"]
                && native
                    .lsi_output_routes
                    .iter()
                    .map(|r| (r.pin.as_str(), r.af))
                    .collect::<Vec<_>>()
                    == expected_routes,
            "L083 native consumer facts differ from the own-source exact-package roster"
        );
    } else {
        ensure!(
            c.startup_consumers.is_none(),
            "unqualified native LSE startup/consumer facts"
        );
    }
    ensure!(
        c.rtc_reset.len() == 13,
        "LSE requires the full RTC reset roster"
    );
    for ((name, offset, value, mask), actual) in LSE_RTC_RESET.iter().zip(&c.rtc_reset) {
        ensure!(
            actual.register == *name
                && actual.byte_offset == *offset
                && actual.value
                    == if part.starts_with("CW32L052") && *name == "ALARMA" {
                        0x0412_0000
                    } else {
                        *value
                    }
                && actual.mask == *mask,
            "LSE RTC reset roster must be exact, complete, unique and ordered"
        );
    }
    ensure!(
        if part == "CW32L031F8U6" {
            c.output_routes.is_empty()
        } else {
            c.output_routes.len() == 2
                && c.output_routes[0].pin == "PB12"
                && c.output_routes[0].af == 3
                && c.output_routes[1].pin == "PF1"
                && c.output_routes[1].af == 1
        },
        "LSE direct-output routes must be the exact unique own-package pair"
    );
    Ok(())
}
fn lse_register<'a>(
    registers: &'a BTreeMap<String, chiptool::ir::IR>,
    core: &cw32_data_serde::chip::Core,
    peripheral: &str,
    register: &str,
    offset: u32,
) -> Result<(&'a chiptool::ir::IR, &'a chiptool::ir::FieldSet)> {
    use chiptool::ir::BlockItemInner;
    let owner = core
        .peripherals
        .iter()
        .find(|p| p.name == peripheral)
        .with_context(|| format!("missing LSE consumer {peripheral}"))?;
    let selected = owner
        .registers
        .as_ref()
        .context("LSE consumer has no selected registers")?;
    let ir = registers
        .get(&selected.kind)
        .context("missing LSE selected register IR")?;
    let block = ir
        .blocks
        .get(&selected.block)
        .context("missing LSE selected block")?;
    let item = block
        .items
        .iter()
        .find(|r| r.name == register && r.byte_offset == offset)
        .with_context(|| format!("LSE register layout mismatch: {peripheral}.{register}"))?;
    let BlockItemInner::Register(reg) = &item.inner else {
        anyhow::bail!("LSE register is not a register")
    };
    let fieldset = ir
        .fieldsets
        .get(reg.fieldset.as_ref().context("LSE register lacks fields")?)
        .context("missing LSE fieldset")?;
    Ok((ir, fieldset))
}
fn lse_field(fields: &chiptool::ir::FieldSet, name: &str, offset: u32, size: u32) -> Result<()> {
    ensure!(
        fields.fields.iter().any(|f| f.name == name
            && f.bit_offset == chiptool::ir::BitOffset::Regular(offset)
            && f.bit_size == size
            && f.array.is_none()),
        "LSE source field mismatch: {name}"
    );
    Ok(())
}
fn lse_source_enum(
    ir: &chiptool::ir::IR,
    fields: &chiptool::ir::FieldSet,
    field: &str,
    value: u8,
) -> Result<()> {
    let field = fields
        .fields
        .iter()
        .find(|f| f.name == field)
        .context("missing LSE source field")?;
    let enumeration = ir
        .enums
        .get(
            field
                .enumm
                .as_ref()
                .context("LSE source must be native enum")?,
        )
        .context("missing LSE source enum")?;
    ensure!(
        enumeration
            .variants
            .iter()
            .filter(|v| v.name == "LSE" && v.value == u64::from(value))
            .count()
            == 1,
        "LSE source enum encoding differs from own manual"
    );
    Ok(())
}
// The source AF catalog includes unmerged special functions. Bound every
// numeric value before narrowing; malformed AF262 must never become AF6.
fn lse_lsi_af_routes(
    af: &serde_yaml::Value,
    bonded: &std::collections::BTreeSet<&str>,
) -> Result<Vec<(String, u8)>> {
    let mut actual = Vec::new();
    for (_, value) in af.as_mapping().context("missing AF catalog")? {
        if let Some(entries) = value.as_sequence() {
            for entry in entries {
                if entry["function"].as_str() == Some("LSIOUT") {
                    let pin = entry["pin"].as_str().context("missing LSI AF pin")?;
                    let af = u8::try_from(entry["af"].as_u64().context("missing LSI AF value")?)
                        .context("LSI AF value exceeds u8")?;
                    ensure!(af < 16, "LSI AF value exceeds native four-bit selector");
                    if bonded.contains(pin) {
                        actual.push((pin.to_string(), af));
                    }
                }
            }
        }
    }
    Ok(actual)
}

/// Check the own L010 layout without importing another family's admission or
/// factory-LSI policy. The complete source/package checks run before this branch.
fn validate_l010_lse_registers(
    root: &Path,
    chip: &crate::ChipInput,
    core: &cw32_data_serde::chip::Core,
    registers: &BTreeMap<String, chiptool::ir::IR>,
    c: &cw32_data_serde::chip::core::peripheral::LseConfiguration,
    proof: &Value,
) -> Result<()> {
    let n = c
        .native_l010
        .as_ref()
        .context("missing L010 native facts")?;
    let (sysctrl, lse) = lse_register(registers, core, "SYSCTRL", "LSE", 36)?;
    let expected_fields = [
        ("DRIVER", 0, 4),
        ("WAITCYCLE", 4, 2),
        ("MODE", 6, 1),
        ("PDRIVER", 8, 4),
        ("PINLOCK", 17, 1),
        ("STABLE", 18, 1),
    ];
    ensure!(
        lse.fields.len() == expected_fields.len(),
        "L010 has no amplitude or undocumented analog fields"
    );
    for (name, bit, width) in expected_fields {
        lse_field(lse, name, bit, width)?;
    }
    for (field, enumeration, bits, count) in [
        ("DRIVER", "LseDrive", 4, 16),
        ("PDRIVER", "LseDrive", 4, 16),
        ("WAITCYCLE", "LseWait", 2, 4),
    ] {
        ensure!(
            lse.fields
                .iter()
                .find(|f| f.name == field)
                .and_then(|f| f.enumm.as_deref())
                == Some(enumeration),
            "L010 LSE field requires own native enum"
        );
        let e = sysctrl
            .enums
            .get(enumeration)
            .context("missing L010 LSE enum")?;
        ensure!(
            e.bit_size == bits && e.variants.len() == count,
            "L010 native enum width or count differs from manual"
        );
        for (i, v) in e.variants.iter().enumerate() {
            let name = if enumeration == "LseDrive" {
                format!("LEVEL{i}")
            } else {
                format!("CYCLES{}", c.startup_cycles[i])
            };
            ensure!(
                v.value == i as u64 && v.name == name,
                "L010 native enum must exactly preserve source encodings"
            );
        }
    }
    for (register, offset, field, bit, width) in [
        ("CR0", 0, "SYSCLK", 0, 3),
        ("CR1", 4, "LSEEN", 4, 1),
        ("CR1", 4, "LSELOCK", 5, 1),
        ("CR1", 4, "LSECCS", 6, 1),
        ("CR1", 4, "KEY", 16, 16),
        ("CR2", 8, "LSEBRKEN", 11, 1),
        ("LSI", 32, "TRIM", 0, 10),
        ("LSI", 32, "WAITCYCLE", 10, 2),
        ("LSI", 32, "STABLE", 15, 1),
        ("MCO", 112, "SOURCE", 0, 4),
        ("ISR", 16, "LSERDY", 4, 1),
        ("ISR", 16, "LSEFAIL", 5, 1),
        ("ISR", 16, "LSEFAULT", 7, 1),
        ("ISR", 16, "LSESTABLE", 15, 1),
        ("IER", 12, "LSERDY", 4, 1),
        ("IER", 12, "LSEFAIL", 5, 1),
        ("IER", 12, "LSEFAULT", 7, 1),
    ] {
        let (_, fields) = lse_register(registers, core, "SYSCTRL", register, offset)?;
        lse_field(fields, field, bit, width)?;
    }
    for (name, gate, gate_offset, bit, reset, reset_offset) in [
        ("RTC", "APBEN2", 52, 1, "APBRST2", 68),
        ("UART1", "APBEN1", 56, 3, "APBRST1", 72),
        ("UART2", "APBEN1", 56, 4, "APBRST1", 72),
        ("LPTIM", "APBEN2", 52, 7, "APBRST2", 68),
        ("GPIOB", "AHBEN", 48, 5, "AHBRST", 64),
    ] {
        for (register, offset) in [(gate, gate_offset), (reset, reset_offset)] {
            let (_, fields) = lse_register(registers, core, "SYSCTRL", register, offset)?;
            lse_field(fields, name, bit, 1)?;
        }
        ensure!(
            proof["consumer_gates"][name]
                == serde_json::json!([gate, gate_offset, bit, reset, reset_offset])
                && proof["consumer_gate_policy"][name]
                    == if name == "GPIOB" {
                        "configuration_and_work"
                    } else {
                        "configuration_only"
                    },
            "native L010 gate facts cannot inherit work-gate assumptions"
        );
    }
    let actual_uarts: Vec<_> = core
        .peripherals
        .iter()
        .filter(|p| p.registers.as_ref().is_some_and(|r| r.kind == "uart"))
        .map(|p| p.name.as_str())
        .collect();
    ensure!(
        actual_uarts == ["UART1", "UART2"],
        "native L010 UART roster changed"
    );
    for name in actual_uarts {
        let (ir, fields) = lse_register(registers, core, name, "CR1", 0)?;
        lse_field(fields, "SOURCE", 12, 2)?;
        lse_source_enum(ir, fields, "SOURCE", c.uart_source)?;
    }
    for (register, offset, field, bit, width) in [
        ("CR", 16, "EN", 0, 1),
        ("CFGR", 12, "ICLKSRC", 25, 2),
        ("CFGR", 12, "TRIGSEL", 13, 3),
        ("CFGR", 12, "TRIGEN", 17, 2),
    ] {
        let (_, fields) = lse_register(registers, core, "LPTIM", register, offset)?;
        lse_field(fields, field, bit, width)?;
    }
    let (lptim_ir, cfgr) = lse_register(registers, core, "LPTIM", "CFGR", 12)?;
    lse_source_enum(lptim_ir, cfgr, "ICLKSRC", 2)?;
    for (register, offset, field, bit, width) in [
        ("CR0", 4, "H24", 3, 1),
        ("CR1", 8, "SOURCE", 8, 3),
        ("CR1", 8, "ACCESS", 0, 1),
        ("CR1", 8, "WINDOW", 1, 1),
        ("CR1", 8, "WAIT", 2, 1),
        ("PSC", 64, "PSC1", 20, 8),
        ("PSC", 64, "PSC2", 0, 20),
    ] {
        let (_, fields) = lse_register(registers, core, "RTC", register, offset)?;
        lse_field(fields, field, bit, width)?;
    }
    for entry in &c.rtc_reset {
        let (_, fields) = lse_register(registers, core, "RTC", &entry.register, entry.byte_offset)?;
        let mut mask = 0u32;
        for field in &fields.fields {
            let chiptool::ir::BitOffset::Regular(bit) = field.bit_offset else {
                anyhow::bail!("nonregular native RTC admission field")
            };
            ensure!(
                field.array.is_none()
                    && field.bit_size > 0
                    && field.bit_size < 32
                    && bit + field.bit_size <= 32,
                "unsupported native RTC admission field"
            );
            if (entry.register == "CR0" && field.name == "H24")
                || (entry.register == "CR1" && field.name == "WINDOW")
            {
                continue;
            }
            mask |= ((1u32 << field.bit_size) - 1) << bit;
        }
        ensure!(mask == entry.mask, "L010 native RTC admission mask changed");
    }
    for (register, offset) in [
        ("DIR", 0),
        ("OPENDRAIN", 4),
        ("PUR", 16),
        ("AFRL", 24),
        ("ANALOG", 28),
        ("RISEIE", 36),
        ("FALLIE", 40),
        ("ISR", 52),
        ("FILTER", 64),
    ] {
        lse_register(registers, core, "GPIOB", register, offset)?;
    }
    let (_, afrl) = lse_register(registers, core, "GPIOB", "AFRL", 24)?;
    for (field, bit) in [("AFR0", 0), ("AFR1", 4), ("AFR4", 16), ("AFR6", 24)] {
        lse_field(afrl, field, bit, 3)?;
    }
    // Keep hardware RTC routes distinct from direct LSE routes and bonding.
    let af: serde_yaml::Value =
        serde_yaml::from_slice(&fs::read(root.join("cw32-data/af/cw32l010.yaml"))?)?;
    let mut rtc_routes = Vec::new();
    for (_, value) in af.as_mapping().context("missing L010 AF catalog")? {
        if let Some(entries) = value.as_sequence() {
            for entry in entries {
                if entry["function"].as_str() == Some("RTCOUT") {
                    let pin = entry["pin"].as_str().context("missing RTC output pin")?;
                    let value = entry["af"].as_u64().context("missing RTC output AF")?;
                    rtc_routes.push((pin, value));
                }
            }
        }
    }
    ensure!(
        rtc_routes == [("PB4", 2), ("PB6", 2)],
        "L010 own RTC output AF roster changed"
    );
    let bonded: Vec<_> = n
        .rtc_output_routes
        .iter()
        .filter(|r| core.pins.iter().any(|p| p.name == r.pin))
        .collect();
    let evidence = proof["package_pins"][&chip.name]["rtc_output_routes"]
        .as_array()
        .context("missing L010 package RTC routes")?;
    ensure!(
        evidence.len() == bonded.len(),
        "L010 package RTC route omission"
    );
    for (r, source) in bonded.iter().zip(evidence) {
        ensure!(
            source["pin"] == r.pin
                && source["af"] == r.af
                && chip.packages[0]
                    .pins
                    .iter()
                    .any(|p| source["position"] == p.position
                        && p.signals.iter().any(|s| s == &r.pin)),
            "L010 RTC output bonding changed"
        );
    }
    let access: serde_yaml::Value =
        serde_yaml::from_slice(&fs::read(root.join("cw32-data/field-access.yaml"))?)?;
    ensure!(
        access["registers"]["sysctrl_cw32l010_v1"]
            .as_sequence()
            .context("missing native L010 access overlay")?
            .iter()
            .any(|f| f["register"].as_str() == Some("LSE")
                && f["field"].as_str() == Some("STABLE")
                && f["bit_offset"].as_u64() == Some(18)
                && f["bit_size"].as_u64() == Some(1)),
        "L010 LSE.STABLE must retain the own-source RO overlay"
    );
    ensure!(
        proof["lsi_monitor_prerequisite"]["maximum_hz"] == n.monitored_lsi_maximum_hz
            && proof["lsi_monitor_prerequisite"]["cold_lsi_startup_supported"] == false,
        "native L010 monitor requires unchanged legal LSI; no imported calibration policy"
    );
    Ok(())
}

/// Project exact-part LSE capabilities only after verifying package, own sources,
/// selected register fields and the complete conservative consumer snapshot.
pub fn apply_lse(
    root: &Path,
    chip: &crate::ChipInput,
    line: &str,
    core: &mut cw32_data_serde::chip::Core,
    registers: &BTreeMap<String, chiptool::ir::IR>,
) -> Result<()> {
    ensure!(
        core.peripherals.iter().all(|p| p
            .clock_limits
            .as_ref()
            .is_none_or(|c| c.lse_configuration.is_none())),
        "LSE configuration cannot be preseeded or inherited from a family profile"
    );
    let catalog = parse_lse_catalog(&fs::read(root.join("cw32-data/lse-qualified.yaml"))?)?;
    ensure!(
        catalog.schema_version == 1 && catalog.source_authority == "sources/evidence-sources.json",
        "unknown active LSE source catalog"
    );
    validate_lse_parts(&catalog)?;
    let read_policy = |path: &str| -> Result<Value> {
        let bytes = fs::read(root.join(path))?;
        ensure!(
            catalog.policies.get(path) == Some(&format!("{:x}", Sha256::digest(&bytes))),
            "active LSE source policy changed: {path}"
        );
        Ok(serde_json::from_slice(&bytes)?)
    };
    let x030_proof = read_policy("docs/lse-active-first-cohort.json")?;
    let x030_rtc_proof = read_policy("docs/lse-active-rtc-admission.json")?;
    let f020_proof = read_policy("docs/lse-active-f020.json")?;
    let f020_rtc_proof = read_policy("docs/lse-active-f020-rtc-admission.json")?;
    let l031_proof = read_policy("docs/lse-active-l031.json")?;
    let l031_rtc_proof = read_policy("docs/lse-active-l031-rtc-admission.json")?;
    let r031_proof = read_policy("docs/lse-active-r031.json")?;
    let r031_rtc_proof = read_policy("docs/lse-active-r031-rtc-admission.json")?;
    let w031_proof = read_policy("docs/lse-active-w031.json")?;
    let w031_rtc_proof = read_policy("docs/lse-active-w031-rtc-admission.json")?;
    let l052_proof = read_policy("docs/lse-active-l052.json")?;
    let l052_rtc_proof = read_policy("docs/lse-active-l052-rtc-admission.json")?;
    let l083_proof = read_policy("docs/lse-active-l083.json")?;
    let l083_rtc_proof = read_policy("docs/lse-active-l083-rtc-admission.json")?;
    let l010_proof = read_policy("docs/lse-l010-qualification.json")?;
    let l010_rtc_proof = read_policy("docs/lse-l010-rtc-admission.json")?;
    let own_proof = |family: &str| -> Result<(&Value, &Value)> {
        Ok(match family {
            "CW32A030" | "CW32F030" => (&x030_proof, &x030_rtc_proof),
            "CW32F020" => (&f020_proof, &f020_rtc_proof),
            "CW32L031" => (&l031_proof, &l031_rtc_proof),
            "CW32R031" => (&r031_proof, &r031_rtc_proof),
            "CW32W031" => (&w031_proof, &w031_rtc_proof),
            "CW32L052" => (&l052_proof, &l052_rtc_proof),
            "CW32L083" => (&l083_proof, &l083_rtc_proof),
            "CW32L010" => (&l010_proof, &l010_rtc_proof),
            _ => anyhow::bail!("unreviewed active LSE family"),
        })
    };
    for (part, p) in &catalog.parts {
        // Equal layouts do not extend another family's source authority.
        let (proof, rtc_proof) = own_proof(&p.family)?;
        ensure!(
            proof["schema_version"] == 1 && rtc_proof["schema_version"] == 1,
            "unknown LSE review schema"
        );
        validate_lse_configuration(part, &p.configuration)?;
        ensure!(
            p.configuration.configurable_ccs
                == matches!(
                    p.family.as_str(),
                    "CW32L031" | "CW32R031" | "CW32W031" | "CW32L052" | "CW32L083" | "CW32L010"
                ),
            "LSE CCS hardware facts differ from bounded own-family cohort"
        );
        let expected_configuration = proof["configurations"]
            .get(part)
            .unwrap_or(&proof["configuration"]);
        let mut profile = serde_json::to_value(p)?;
        profile.as_object_mut().unwrap().remove("configuration");
        ensure!(
            profile == proof["parts"][part]
                && serde_json::to_value(&p.configuration)? == *expected_configuration
                && serde_json::to_value(&p.configuration.rtc_reset)? == rtc_proof["rtc_reset"],
            "active LSE facts differ from new own-source review"
        );
    }
    let Some(p) = catalog.parts.get(&chip.name) else {
        return Ok(());
    };
    ensure!(
        p.family == line
            && LSE_PARTS
                .iter()
                .any(|(part, family, package)| chip.name == *part
                    && line == *family
                    && p.package == *package)
            && p.input_pin == if line == "CW32L010" { "PB1" } else { "PC14" }
            && p.output_pin == if line == "CW32L010" { "PB0" } else { "PC15" },
        "LSE cannot derive exact-part qualification from a family alias"
    );
    ensure!(
        chip.packages.len() == 1 && chip.packages[0].package == p.package,
        "LSE qualification requires its reviewed exact package"
    );
    let (proof, _) = own_proof(line)?;
    let package_pins = &proof["package_pins"][&chip.name];
    let input_position = package_pins["input_position"].as_str().unwrap_or("3");
    let output_position = package_pins["output_position"].as_str().unwrap_or("4");
    if p.configuration.configurable_ccs {
        ensure!(
            package_pins["input_position"].is_string()
                && package_pins["output_position"].is_string(),
            "missing own exact-package oscillator positions"
        );
    }
    for (pin, position, alias) in [
        (p.input_pin.as_str(), input_position, "OSC32_IN"),
        (p.output_pin.as_str(), output_position, "OSC32_OUT"),
    ] {
        ensure!(
            core.pins.iter().any(|p| p.name == pin)
                && chip.packages[0].pins.iter().any(|p| p.position == position
                    && p.signals.iter().any(|s| s == pin)
                    && p.signals.iter().any(|s| s == alias)),
            "LSE requires the reviewed bonded oscillator pair"
        );
    }
    let authority: Value =
        serde_json::from_slice(&fs::read(root.join(&catalog.source_authority))?)?;
    ensure!(
        p.sources.len() == 2,
        "LSE requires own manual and own datasheet"
    );
    let (expected_manual, expected_datasheet) = match line {
        "CW32A030" => (
            "vendor:CW32x030_UserManual_CN_V2.5.pdf",
            "vendor:CW32A030_DataSheet_CN_V1.1.pdf",
        ),
        "CW32F030" => (
            "vendor:CW32x030_UserManual_CN_V2.5.pdf",
            "vendor:CW32F030_DataSheet_CN_V1.9.pdf",
        ),
        "CW32F020" => (
            "vendor:CW32F020_UserManual_CN_V1.4.pdf",
            "vendor:current-datasheets/CW32F020_DataSheet_CN_V1.3.pdf",
        ),
        "CW32L031" => (
            "vendor:CW32L031_UserManual_CN_V1.6.pdf",
            "vendor:CW32L031_DataSheet_CN_V1.9.pdf",
        ),
        "CW32R031" => (
            "vendor:CW32R031_UserManual_CN_V1.3.pdf",
            "vendor:CW32R031_DataSheet_CN_V1.2.pdf",
        ),
        "CW32W031" => (
            "vendor:CW32W031_UserManual_CN_V1.4.pdf",
            "vendor:CW32W031_DataSheet_CN_V1.3.pdf",
        ),
        "CW32L052" => (
            "vendor:CW32L052_UserManual_CN_V1.5.pdf",
            "vendor:CW32L052_DataSheet_CN_V1.3.pdf",
        ),
        "CW32L083" => (
            "vendor:CW32L083_UserManual_CN_V2.0.pdf",
            "vendor:CW32L083_DataSheet_CN_V1.9.pdf",
        ),
        "CW32L010" => (
            "vendor:CW32L010_UserManual_CN_V1.2.pdf",
            "vendor:CW32L010_DataSheet_CN_V1.3.pdf",
        ),
        _ => anyhow::bail!("unreviewed active LSE family"),
    };
    for (source, expected) in p.sources.iter().zip([expected_manual, expected_datasheet]) {
        ensure!(
            source.source_ref == expected,
            "LSE requires canonical own-source IDs"
        );
        let original = authority["artifacts"]
            .as_array()
            .context("missing source authority")?
            .iter()
            .find(|a| a["id"] == source.source_ref)
            .context("LSE source is not canonical")?;
        let page_count = original["provenance"]["pdf_page_count"]
            .as_u64()
            .context("missing source page count")?;
        ensure!(
            original["sha256"] == source.sha256
                && original["provenance"]["status"] == "selected"
                && original["provenance"]["chip_scope"]
                    .as_array()
                    .context("missing source scope")?
                    .iter()
                    .any(|f| f == line)
                && !source.pdf_pages_1_based.is_empty()
                && source.pdf_pages_1_based.len() == source.printed_pages.len()
                && source
                    .pdf_pages_1_based
                    .iter()
                    .zip(&source.printed_pages)
                    .all(|(pdf, printed)| *pdf > 0
                        && u64::from(*pdf) <= page_count
                        && *pdf == *printed + 1),
            "LSE source identity, own-family scope or evidence pages changed"
        );
    }
    let c = &p.configuration;
    if line == "CW32L010" {
        validate_l010_lse_registers(root, chip, core, registers, c, proof)?;
        core.peripherals
            .iter_mut()
            .find(|p| p.name == "SYSCTRL")
            .context("native LSE has no SYSCTRL owner")?
            .clock_limits
            .as_mut()
            .context("native LSE has no clock limits")?
            .lse_configuration = Some(c.clone());
        return Ok(());
    }
    if c.configurable_ccs {
        let monitor = &proof["lsi_monitor_prerequisite"];
        let selectors = &monitor["selector_allowlists_before_disabled_LSI_trim_change"];
        ensure!(
            selectors["SYSCTRL.CR0.SYSCLK"] == serde_json::json!([0, 1, 4])
                && selectors["SYSCTRL.MCO.SOURCE"] == serde_json::json!([0, 1, 2, 3, 5, 6, 8, 9])
                && selectors[if c.startup_consumers.is_some() {
                    "AUTOTRIM.CR.SRC"
                } else {
                    "AWT.CR.SRC"
                }] == serde_json::json!([0, 2, 3, 4])
                && selectors["UARTx.CR2.SOURCE"] == serde_json::json!([0, 1, 2])
                && selectors["GPIOx.FILTER.FLTCLK"] == serde_json::json!([0, 1, 2, 3, 4, 6, 7]),
            "cold LSI admission selectors differ from own-source record"
        );
        let rtc = core
            .peripherals
            .iter()
            .find(|p| p.name == "RTC")
            .unwrap()
            .rtc_calendar
            .as_ref()
            .context("missing own RTC calibration metadata")?;
        ensure!(
            monitor["factory_trim_halfword_address"] == rtc.factory_trim_address
                && monitor["trim_bit_offset"] == 0
                && monitor["trim_bit_size"] == 10
                && monitor["wait_bit_offset"] == 10
                && monitor["wait_bit_size"] == 2
                && monitor["stable_bit"] == 15,
            "LSI calibration source facts differ from the selected native clock"
        );
        let (_, lsi) = lse_register(registers, core, "SYSCTRL", "LSI", 32)?;
        lse_field(lsi, "TRIM", 0, 10)?;
        lse_field(lsi, "WAITCYCLE", 10, 2)?;
        lse_field(lsi, "STABLE", 15, 1)?;
        let routes = proof["lsi_output_routes_by_part"][&chip.name]
            .as_array()
            .context("missing exact-package LSI output roster")?;
        let expected_routes: Vec<(&str, u8)> = if let Some(native) = &c.startup_consumers {
            native
                .lsi_output_routes
                .iter()
                .map(|r| (r.pin.as_str(), r.af))
                .collect()
        } else if core.pins.iter().any(|p| p.name == "PB11") {
            vec![("PB11", 1)]
        } else {
            vec![]
        };
        ensure!(
            routes.len() == expected_routes.len(),
            "missing or extraneous bonded LSI route"
        );
        for (route, (pin, af)) in routes.iter().zip(expected_routes) {
            let position = route["position"]
                .as_str()
                .context("missing direct LSI position")?;
            ensure!(
                route["pin"] == pin
                    && route["af"] == af
                    && core.pins.iter().any(|p| p.name == pin)
                    && chip.packages[0]
                        .pins
                        .iter()
                        .any(|p| p.position == position && p.signals.iter().any(|s| s == pin)),
                "changed exact-package LSI route"
            );
        }
        if let Some(native) = &c.startup_consumers {
            // LSIOUT is explicitly left unmerged in the existing AF catalog.
            // Compare the own qualified allowlist against its complete raw AF
            // roster filtered by the already verified exact package bonding.
            let af: serde_yaml::Value = serde_yaml::from_slice(&fs::read(
                root.join(format!("cw32-data/af/{}.yaml", line.to_ascii_lowercase())),
            )?)?;
            let bonded = core.pins.iter().map(|p| p.name.as_str()).collect();
            let actual = lse_lsi_af_routes(&af, &bonded)?;
            let expected: Vec<_> = native
                .lsi_output_routes
                .iter()
                .map(|r| (r.pin.clone(), r.af))
                .collect();
            ensure!(
                actual == expected,
                "native LSI qualification differs from complete bonded own AF roster"
            );
        }
    }
    if line == "CW32L083" {
        for (name, register, offset, bit) in [
            ("RTC", "APBEN1", 56, 3),
            ("UART1", "APBEN2", 52, 9),
            ("UART2", "APBEN1", 56, 7),
            ("UART3", "APBEN1", 56, 8),
            ("UART4", "APBEN1", 56, 9),
            ("UART5", "APBEN1", 56, 10),
            ("UART6", "APBEN2", 52, 1),
            ("AUTOTRIM", "APBEN2", 52, 13),
        ] {
            let (_, gate) = lse_register(registers, core, "SYSCTRL", register, offset)?;
            lse_field(gate, name, bit, 1)?;
            ensure!(
                proof["consumer_gate_policy"][name] == "configuration_only"
                    && proof["consumer_gate_sources"]["registers"][name]
                        == serde_json::json!([register, offset, bit]),
                "L083 configuration gate differs from own source"
            );
        }
        let rtc = core
            .peripherals
            .iter()
            .find(|p| p.name == "RTC")
            .unwrap()
            .rtc_calendar
            .as_ref()
            .unwrap();
        let monitor = &proof["lsi_monitor_prerequisite"];
        ensure!(
            monitor["factory_frequency_hz"] == serde_json::json!([rtc.minimum_hz, rtc.maximum_hz])
                && monitor["factory_temperature_c"] == serde_json::to_value(rtc.temperature_c)?
                && monitor["factory_supply_mv"] == serde_json::to_value(rtc.supply_mv)?,
            "L083 detector envelope differs from own factory LSI qualification"
        );
    }
    let (_, cr1) = lse_register(registers, core, "SYSCTRL", "CR1", 4)?;
    for (name, offset, size) in [
        ("LSEEN", 4, 1),
        ("LSELOCK", 5, 1),
        ("LSECCS", 6, 1),
        ("HSECCS", 7, 1),
        ("CLKCCS", 8, 1),
        ("KEY", 16, 16),
    ] {
        lse_field(cr1, name, offset, size)?;
    }
    let (sysctrl, lse) = lse_register(registers, core, "SYSCTRL", "LSE", 36)?;
    for (name, offset, size) in [
        ("DRIVER", 0, 2),
        ("AMP", 2, 2),
        ("WAITCYCLE", 4, 2),
        ("MODE", 6, 1),
        ("STABLE", 15, 1),
    ] {
        lse_field(lse, name, offset, size)?;
    }
    let mut analog_fields = vec![
        ("DRIVER", "LseDrive"),
        ("AMP", "LseAmplitude"),
        ("WAITCYCLE", "LseWait"),
    ];
    if c.startup_consumers
        .as_ref()
        .is_some_and(|n| n.startup_analog)
    {
        lse_field(lse, "PDRIVER", 8, 2)?;
        lse_field(lse, "PAMP", 10, 2)?;
        analog_fields.extend([("PDRIVER", "LseDrive"), ("PAMP", "LseAmplitude")]);
    }
    if line == "CW32L083" {
        ensure!(
            !lse.fields
                .iter()
                .any(|f| matches!(f.name.as_str(), "PDRIVER" | "PAMP")),
            "L083 has no startup analog bank"
        );
    }
    for (field, name) in analog_fields {
        ensure!(
            lse.fields
                .iter()
                .find(|f| f.name == field)
                .and_then(|f| f.enumm.as_deref())
                == Some(name),
            "LSE parameter requires its own native enum"
        );
        let enumeration = sysctrl
            .enums
            .get(name)
            .context("missing native LSE parameter enum")?;
        let values: std::collections::BTreeSet<_> =
            enumeration.variants.iter().map(|v| v.value).collect();
        ensure!(
            enumeration.bit_size == 2
                && enumeration.variants.len() == 4
                && values == [0, 1, 2, 3].into_iter().collect(),
            "LSE parameter enum must cover the four source-defined values exactly"
        );
    }
    let (_, status) = lse_register(registers, core, "SYSCTRL", "ISR", 16)?;
    lse_field(status, "LSEFAIL", 5, 1)?;
    lse_field(status, "LSEFAULT", 7, 1)?;
    let (_, mco) = lse_register(registers, core, "SYSCTRL", "MCO", 112)?;
    lse_field(mco, "SOURCE", 0, 4)?;
    if let Some(source) = c.awt_source {
        ensure!(
            c.startup_consumers.is_none(),
            "AWT and AUTOTRIM cannot overlap"
        );
        let (awt_ir, awt) = lse_register(registers, core, "AWT", "CR", 0)?;
        lse_field(awt, "SRC", 8, 3)?;
        lse_field(awt, "EN", 0, 1)?;
        lse_source_enum(awt_ir, awt, "SRC", source)?;
    }
    let actual_uarts: Vec<_> = core
        .peripherals
        .iter()
        .filter(|p| p.registers.as_ref().is_some_and(|r| r.kind == "uart"))
        .map(|p| p.name.as_str())
        .collect();
    let expected_uarts: Vec<_> = if let Some(native) = &c.startup_consumers {
        let (ir, autotrim) = lse_register(registers, core, "AUTOTRIM", "CR", 0)?;
        lse_field(autotrim, "SRC", 8, 3)?;
        lse_field(autotrim, "EN", 0, 1)?;
        lse_field(autotrim, "MD", 1, 2)?;
        lse_field(autotrim, "AUTO", 3, 1)?;
        lse_source_enum(ir, autotrim, "SRC", native.autotrim_source)?;
        let (_, cr) = lse_register(registers, core, "LPTIM", "CR", 16)?;
        lse_field(cr, "EN", 0, 1)?;
        let (_, cfgr) = lse_register(registers, core, "LPTIM", "CFGR", 12)?;
        lse_field(cfgr, "ICLKSRC", 25, 2)?;
        let (_, cr0) = lse_register(registers, core, "LCD", "CR0", 0)?;
        lse_field(cr0, "EN", 0, 1)?;
        let (_, cr1) = lse_register(registers, core, "LCD", "CR1", 4)?;
        lse_field(cr1, "CLKCS", 7, 1)?;
        for (name, bit) in [("LPTIM", 15), ("LCD", 13)] {
            let (_, gate) = lse_register(registers, core, "SYSCTRL", "APBEN1", 56)?;
            lse_field(gate, name, bit, 1)?;
            ensure!(
                proof["consumer_gate_policy"][name] == "configuration_and_work",
                "unqualified native work gate"
            );
        }
        native.uarts.iter().map(String::as_str).collect()
    } else {
        vec!["UART1", "UART2", "UART3"]
    };
    ensure!(
        actual_uarts == expected_uarts,
        "qualified UART roster must equal all selected native UART instances"
    );
    // Own L052 PAC retains the vendor SORCE spelling; no register rename.
    let uart_source_field = if line == "CW32L052" {
        "SORCE"
    } else {
        "SOURCE"
    };
    for name in expected_uarts {
        let (uart_ir, uart) = lse_register(registers, core, name, "CR2", 4)?;
        lse_field(uart, uart_source_field, 8, 2)?;
        lse_source_enum(uart_ir, uart, uart_source_field, c.uart_source)?;
    }
    let (rtc_ir, rtc) = lse_register(registers, core, "RTC", "CR1", 8)?;
    lse_field(rtc, "SOURCE", 8, 3)?;
    lse_source_enum(rtc_ir, rtc, "SOURCE", c.rtc_source)?;
    for entry in &c.rtc_reset {
        let (_, fields) = lse_register(registers, core, "RTC", &entry.register, entry.byte_offset)?;
        let mut mask = 0u32;
        for field in &fields.fields {
            let chiptool::ir::BitOffset::Regular(offset) = field.bit_offset else {
                anyhow::bail!("nonregular RTC reset mask")
            };
            ensure!(
                field.array.is_none()
                    && field.bit_size > 0
                    && field.bit_size < 32
                    && offset + field.bit_size <= 32,
                "unsupported RTC reset field"
            );
            mask |= ((1u32 << field.bit_size) - 1) << offset;
        }
        ensure!(
            mask == entry.mask,
            "RTC reset defined-bit mask differs from source roster: {}",
            entry.register
        );
    }
    for gpio in ["GPIOB", "GPIOC", "GPIOF"] {
        lse_register(registers, core, gpio, "DIR", c.gpio_dir_offset)?;
        if let Some(offset) = c.gpio_speed_offset {
            lse_register(registers, core, gpio, "SPEED", offset)?;
        }
        if c.configurable_ccs {
            let peripheral = core.peripherals.iter().find(|p| p.name == gpio).unwrap();
            let selected = peripheral.registers.as_ref().unwrap();
            let block = &registers[&selected.kind].blocks[&selected.block];
            let actual: Vec<_> = block
                .items
                .iter()
                .map(|r| serde_json::json!({"register": r.name, "byte_offset": r.byte_offset}))
                .collect();
            ensure!(
                serde_json::to_value(actual)? == proof["gpio_registers"],
                "GPIO native register roster differs from own-source evidence"
            );
        }
        lse_register(registers, core, gpio, "ANALOG", 28)?;
    }
    for (gpio, register, offset, field, bit) in [
        ("GPIOB", "AFRH", 20, "AFR12", 16),
        ("GPIOF", "AFRL", 24, "AFR1", 4),
    ] {
        let (_, fields) = lse_register(registers, core, gpio, register, offset)?;
        lse_field(fields, field, bit, 4)?;
    }
    if c.configurable_ccs {
        let bonded_routes = package_pins["output_routes"]
            .as_array()
            .context("missing own-package direct routes")?;
        ensure!(
            bonded_routes.len() == c.output_routes.len(),
            "incomplete package LSE output roster"
        );
        for route in bonded_routes {
            let pin = route["pin"].as_str().context("missing route pin")?;
            let position = route["position"]
                .as_str()
                .context("missing route position")?;
            ensure!(
                c.output_routes
                    .iter()
                    .any(|r| r.pin == pin && Some(u64::from(r.af)) == route["af"].as_u64())
                    && chip.packages[0]
                        .pins
                        .iter()
                        .any(|p| p.position == position && p.signals.iter().any(|s| s == pin)),
                "unbonded or changed own-package direct route"
            );
        }
        for pin in ["PB12", "PF1"] {
            ensure!(
                core.pins.iter().any(|p| p.name == pin)
                    == c.output_routes.iter().any(|r| r.pin == pin),
                "exact package output route omission"
            );
        }
    }
    for route in &c.output_routes {
        ensure!(
            core.pins.iter().any(|p| p.name == route.pin),
            "unbonded LSE direct output"
        );
    }
    let owner = core
        .peripherals
        .iter_mut()
        .find(|p| p.name == "SYSCTRL")
        .context("active LSE has no SYSCTRL owner")?;
    owner
        .clock_limits
        .as_mut()
        .context("LSE requires source-qualified clock limits")?
        .lse_configuration = Some(c.clone());
    Ok(())
}

#[cfg(test)]
mod pll_source_tests {
    use super::*;

    #[test]
    fn pll_rejects_historical_foreign_missing_and_unqualified_sources() {
        let mut catalog: Catalog =
            serde_yaml::from_str(include_str!("../../cw32-data/electrical.yaml")).unwrap();
        for line in ["CW32L083", "CW32F020", "CW32F030", "CW32A030"] {
            validate_pll_source_selection(line, &catalog.profiles[line].pll_sources).unwrap();
        }
        let f020 = &mut catalog.profiles.get_mut("CW32F020").unwrap().pll_sources;
        f020[1].source_ref = "vendor:CW32F020_DataSheet_CN_V1.3.pdf".into();
        assert!(validate_pll_source_selection("CW32F020", f020).is_err());
        let a030 = &mut catalog.profiles.get_mut("CW32A030").unwrap().pll_sources;
        a030[1].source_ref = "vendor:CW32F030_DataSheet_CN_V1.9.pdf".into();
        assert!(validate_pll_source_selection("CW32A030", a030).is_err());
        a030.pop();
        assert!(validate_pll_source_selection("CW32A030", a030).is_err());
        assert!(
            validate_pll_source_selection("CW32L052", &catalog.profiles["CW32L083"].pll_sources)
                .is_err()
        );
    }
}

#[cfg(test)]
mod lse_tests {
    use super::*;
    fn catalog() -> LseCatalog {
        parse_lse_catalog(include_bytes!("../../cw32-data/lse-qualified.yaml")).unwrap()
    }
    #[test]
    fn l010_lse_rejects_cross_family_and_native_fact_drift() {
        let catalog = catalog();
        for part in ["CW32L010F8P6", "CW32L010F8U6", "CW32L010Y8M6"] {
            let original = catalog.parts[part].configuration.clone();
            validate_lse_configuration(part, &original).unwrap();
            for mutation in 0..15 {
                let mut c = original.clone();
                match mutation {
                    0 => c.native_l010 = None,
                    1 => c.maximum_hz = 1_000_000,
                    2 => c.native_l010.as_mut().unwrap().drive_bits = 2,
                    3 => c.native_l010.as_mut().unwrap().startup_drive_bits = 2,
                    4 => c.native_l010.as_mut().unwrap().monitored_lsi_maximum_hz = 33_784,
                    5 => c.native_l010.as_mut().unwrap().detector_lse_edges = 129,
                    6 => c.native_l010.as_mut().unwrap().detector_lsi_cycles = 128,
                    7 => c.native_l010.as_mut().unwrap().rtc_first_divisor = 0,
                    8 => c.native_l010.as_mut().unwrap().rtc_second_divisor = 32768,
                    9 => c.native_l010.as_mut().unwrap().rtc_calendar_divisor = 16384,
                    10 => c.native_l010.as_mut().unwrap().rtc_output_routes[0].af = 3,
                    11 => c.rtc_reset.pop().map(|_| ()).unwrap(),
                    12 => c.rtc_reset[1].mask = 0x703,
                    13 => c.awt_source = Some(3),
                    _ => c.configurable_ccs = false,
                }
                assert!(
                    validate_lse_configuration(part, &c).is_err(),
                    "{part} mutation{mutation}"
                );
            }
            for unqualified in ["CW32L010", "CW32L011F8P6", "CW32L012F8P6"] {
                assert!(validate_lse_configuration(unqualified, &original).is_err());
            }
        }
        for (part, profile) in &catalog.parts {
            if profile.family != "CW32L010" {
                assert!(profile.configuration.native_l010.is_none());
                validate_lse_configuration(part, &profile.configuration).unwrap();
                let mut c = profile.configuration.clone();
                c.native_l010 = catalog.parts["CW32L010F8P6"]
                    .configuration
                    .native_l010
                    .clone();
                assert!(validate_lse_configuration(part, &c).is_err());
            }
        }
    }
    #[test]
    fn lse_rejects_duplicate_catalog_keys() {
        let original = include_str!("../../cw32-data/lse-qualified.yaml");
        for (needle, replacement) in [
            ("parts:\n", "parts:\n  CW32A030C8T7: {}\n"),
            (
                "policies:\n",
                "policies:\n  docs/lse-active-first-cohort.json: duplicate\n",
            ),
            (
                "    family: CW32A030\n",
                "    family: CW32A030\n    family: CW32A030\n",
            ),
            (
                "      nominal_hz: 32768\n",
                "      nominal_hz: 32768\n      nominal_hz: 32768\n",
            ),
        ] {
            assert!(original.contains(needle));
            let duplicate = original.replacen(needle, replacement, 1);
            let error = parse_lse_catalog(duplicate.as_bytes())
                .err()
                .expect("duplicate key must fail");
            assert!(error.to_string().contains("duplicate"), "{error}");
        }
    }
    #[test]
    fn lse_rejects_broadened_or_incomplete_qualification() {
        let mut c = catalog();
        validate_lse_parts(&c).unwrap();
        let p = c.parts.remove("CW32A030C8T7").unwrap();
        assert!(validate_lse_parts(&c).is_err());
        c.parts.insert("CW32A030".into(), p);
        assert!(validate_lse_parts(&c).is_err());
        let mut c = catalog();
        c.parts.get_mut("CW32F030C8T7").unwrap().package = "QFN32".into();
        assert!(validate_lse_parts(&c).is_err());
        for unqualified in ["CW32F020", "CW32F020F6U7", "CW32F020K6U7"] {
            let mut c = catalog();
            let p = c.parts.remove("CW32F020C6U7").unwrap();
            c.parts.insert(unqualified.into(), p);
            assert!(validate_lse_parts(&c).is_err());
        }
        let mut c = catalog();
        c.parts.get_mut("CW32F020C6U7").unwrap().input_pin = "PC13".into();
        assert!(validate_lse_parts(&c).is_err());
    }
    #[test]
    fn lse_rejects_malformed_reset_routes_and_sources() {
        let original = catalog()
            .parts
            .remove("CW32F030C8T7")
            .unwrap()
            .configuration;
        validate_lse_configuration("CW32F030C8T7", &original).unwrap();
        let mut c = original.clone();
        c.rtc_reset.pop();
        assert!(validate_lse_configuration("CW32F030C8T7", &c).is_err());
        let mut c = original.clone();
        c.rtc_reset[12] = c.rtc_reset[11].clone();
        assert!(validate_lse_configuration("CW32F030C8T7", &c).is_err());
        let mut c = original.clone();
        c.rtc_reset[1].mask &= !1;
        assert!(validate_lse_configuration("CW32F030C8T7", &c).is_err());
        let mut c = original.clone();
        c.output_routes[1] = c.output_routes[0].clone();
        assert!(validate_lse_configuration("CW32F030C8T7", &c).is_err());
        let mut c = original.clone();
        c.awt_source = Some(2);
        assert!(validate_lse_configuration("CW32F030C8T7", &c).is_err());
        let mut c = original.clone();
        c.mco_source = 5;
        assert!(validate_lse_configuration("CW32F030C8T7", &c).is_err());
        let mut c = original.clone();
        c.gpio_dir_offset = 8;
        assert!(validate_lse_configuration("CW32F030C8T7", &c).is_err());
    }
    #[test]
    fn lse_compact_package_requires_its_complete_empty_output_roster() {
        let mut catalog = catalog();
        let compact = &catalog.parts["CW32L031F8U6"].configuration;
        validate_lse_configuration("CW32L031F8U6", compact).unwrap();
        let mut wrong = compact.clone();
        wrong.output_routes = catalog.parts["CW32L031C8U6"]
            .configuration
            .output_routes
            .clone();
        assert!(validate_lse_configuration("CW32L031F8U6", &wrong).is_err());
        assert!(validate_lse_configuration("CW32L031C8U6", compact).is_err());
        for unqualified in [
            "CW32L031",
            "CW32L031F8P6",
            "CW32L031K8V6",
            "CW32L031K8U6",
            "CW32R031",
            "CW32W031",
        ] {
            let p = catalog.parts.remove("CW32L031F8U6").unwrap();
            catalog.parts.insert(unqualified.into(), p);
            assert!(validate_lse_parts(&catalog).is_err());
            let p = catalog.parts.remove(unqualified).unwrap();
            catalog.parts.insert("CW32L031F8U6".into(), p);
        }
    }
    #[test]
    fn l052_lsi_af_rejects_overflow_and_out_of_native_width() {
        let bonded = ["PC4"].into_iter().collect();
        for (value, expected_error) in [(262, "exceeds u8"), (16, "four-bit selector")] {
            let raw =
                format!("special_functions:\n- pin: PC4\n  function: LSIOUT\n  af: {value}\n");
            let af = serde_yaml::from_str(&raw).unwrap();
            let error = lse_lsi_af_routes(&af, &bonded).unwrap_err();
            assert!(error.to_string().contains(expected_error));
        }
        let af =
            serde_yaml::from_str("special_functions:\n- pin: PC4\n  function: LSIOUT\n  af: 6\n")
                .unwrap();
        assert_eq!(
            lse_lsi_af_routes(&af, &bonded).unwrap(),
            [("PC4".to_string(), 6)]
        );
        assert!(
            lse_lsi_af_routes(&af, &std::collections::BTreeSet::new())
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn l052_requires_complete_native_facts_and_own_reset_values() {
        let c = catalog();
        let original = &c.parts["CW32L052R8T6"].configuration;
        validate_lse_configuration("CW32L052R8T6", original).unwrap();
        let mut invalid = original.clone();
        invalid.startup_consumers = None;
        assert!(validate_lse_configuration("CW32L052R8T6", &invalid).is_err());
        let mut invalid = original.clone();
        invalid.awt_source = Some(3);
        assert!(validate_lse_configuration("CW32L052R8T6", &invalid).is_err());
        let mut invalid = original.clone();
        invalid
            .rtc_reset
            .iter_mut()
            .find(|r| r.register == "ALARMA")
            .unwrap()
            .value = 0x0012_0000;
        assert!(validate_lse_configuration("CW32L052R8T6", &invalid).is_err());
        for which in 0..7 {
            let mut invalid = original.clone();
            let n = invalid.startup_consumers.as_mut().unwrap();
            match which {
                0 => n.startup_analog = false,
                1 => n.lptim.gate_controls_work = false,
                2 => n.lcd.gate_controls_work = false,
                3 => n.uarts.pop().map(|_| ()).unwrap(),
                4 => n.lsi_output_routes[0].af = 1,
                5 => n.autotrim_source = 7,
                _ => n.lcd.source = 0,
            }
            assert!(validate_lse_configuration("CW32L052R8T6", &invalid).is_err());
        }
        assert!(validate_lse_configuration("CW32L052C8T6", original).is_err());
        let compact = &c.parts["CW32L052C8T6"].configuration;
        validate_lse_configuration("CW32L052C8T6", compact).unwrap();
        assert!(validate_lse_configuration("CW32L052R8T6", compact).is_err());
    }

    #[test]
    fn l083_requires_own_bank_uart_routes_and_reset() {
        let catalog = catalog();
        for part in [
            "CW32L083RBT6",
            "CW32L083RCT6",
            "CW32L083RCS6",
            "CW32L083MCT6",
            "CW32L083VCT6",
        ] {
            let original = &catalog.parts[part].configuration;
            validate_lse_configuration(part, original).unwrap();
            for which in 0..8 {
                let mut invalid = original.clone();
                match which {
                    0 => invalid.startup_consumers = None,
                    1 => invalid.startup_consumers.as_mut().unwrap().startup_analog = true,
                    2 => {
                        invalid.startup_consumers.as_mut().unwrap().uarts.pop();
                    }
                    3 => {
                        invalid
                            .startup_consumers
                            .as_mut()
                            .unwrap()
                            .lptim
                            .gate_controls_work = false
                    }
                    4 => {
                        invalid
                            .startup_consumers
                            .as_mut()
                            .unwrap()
                            .lcd
                            .gate_controls_work = false
                    }
                    5 => {
                        invalid
                            .startup_consumers
                            .as_mut()
                            .unwrap()
                            .lsi_output_routes
                            .pop();
                    }
                    6 => {
                        invalid
                            .rtc_reset
                            .iter_mut()
                            .find(|r| r.register == "ALARMA")
                            .unwrap()
                            .value = 0x0412_0000
                    }
                    _ => invalid.awt_source = Some(3),
                }
                assert!(validate_lse_configuration(part, &invalid).is_err());
            }
        }
        // Larger packages are not a source for another package's output roster.
        let large = &catalog.parts["CW32L083VCT6"].configuration;
        assert!(validate_lse_configuration("CW32L083MCT6", large).is_err());
        assert!(validate_lse_configuration("CW32L083RBT6", large).is_err());
        let mut invalid = catalog;
        let profile = invalid.parts.remove("CW32L083RBT6").unwrap();
        invalid.parts.insert("CW32L083".into(), profile);
        assert!(validate_lse_parts(&invalid).is_err());
    }

    #[test]
    fn lse_rejects_preseed_before_package_admission() {
        let c = catalog()
            .parts
            .remove("CW32F030C8T7")
            .unwrap()
            .configuration;
        let electrical: Catalog =
            serde_yaml::from_str(include_str!("../../cw32-data/electrical.yaml")).unwrap();
        let mut limits = electrical.profiles["CW32F030"].clock_limits.clone();
        limits.lse_configuration = Some(c);
        let owner: Peripheral =
            serde_json::from_value(serde_json::json!({"name":"SYSCTRL", "clock_limits":limits}))
                .unwrap();
        let mut core = cw32_data_serde::chip::Core {
            name: "cm0p".into(),
            nvic_priority_bits: None,
            peripherals: vec![owner],
            interrupts: vec![],
            dma_channels: vec![],
            pins: vec![],
        };
        let chip = crate::ChipInput {
            name: "CW32F030".into(),
            memory: vec![],
            packages: vec![],
            docs: vec![],
        };
        let error = apply_lse(
            Path::new("unused"),
            &chip,
            "CW32F030",
            &mut core,
            &BTreeMap::new(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("preseeded"));
    }
}
