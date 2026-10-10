//! Own-source factory-LSI SYSCLK qualification and complete direct-consumer facts.
use anyhow::{Context, Result, ensure};
use chiptool::ir;
use cw32_data_serde::chip::Core;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::Path};

fn parse_policy(bytes: &[u8]) -> Result<Value> {
    // serde_yaml::Value rejects duplicate mapping keys recursively before
    // conversion to JSON Value, including arbitrary family/field mappings.
    let value: serde_yaml::Value = serde_yaml::from_slice(bytes)?;
    Ok(serde_json::to_value(value)?)
}

pub fn apply(
    root: &Path,
    chip: &crate::ChipInput,
    line: &str,
    core: &mut Core,
    registers: &BTreeMap<String, ir::IR>,
) -> Result<()> {
    let classic = matches!(line, "CW32F020" | "CW32F030" | "CW32A030");
    let f002 = line == "CW32F002";
    let qualified = classic || f002;
    let Some(clock) = core
        .peripherals
        .iter()
        .find_map(|p| p.clock_limits.as_ref())
    else {
        // A source-import fixture without any clock metadata has no source
        // capability. The qualified real families must never take this exit.
        ensure!(
            !qualified,
            "qualified factory-LSI family lacks clock metadata"
        );
        return Ok(());
    };
    let path = "cw32-data/lsi-sysclk-qualified.yaml";
    let bytes = fs::read(root.join(path))?;
    let electrical = parse_policy(&fs::read(root.join("cw32-data/electrical.yaml"))?)?;
    ensure!(
        electrical["policies"][path] == format!("{:x}", Sha256::digest(&bytes)),
        "factory-LSI source policy changed"
    );
    let policy = parse_policy(&bytes)?;
    ensure!(
        policy["schema_version"] == 1
            && policy["source_authority"] == "sources/evidence-sources.json",
        "unknown factory-LSI authority"
    );
    let families = policy["families"]
        .as_object()
        .context("missing LSI family roster")?;
    ensure!(
        families.keys().map(String::as_str).collect::<Vec<_>>()
            == ["CW32A030", "CW32F002", "CW32F020", "CW32F030"],
        "factory-LSI family scope changed"
    );
    let own = &policy["families"][line];
    // Family electrical profiles must never preseed exact-only capabilities.
    ensure!(
        core.peripherals
            .iter()
            .filter_map(|p| p.clock_limits.as_ref())
            .all(|c| c.lsi_sysclk.is_some() == classic),
        "factory-LSI facts must be classic family facts or exact-only post-projection facts"
    );
    ensure!(
        own["lsi_sysclk"].is_object() == qualified,
        "qualified family policy requires nonnull factory-LSI facts"
    );
    if !qualified {
        return Ok(());
    }
    // Option deserialization accepts a missing field. Qualification does not:
    // explicit null is permitted solely for the independently proved F002 absence.
    ensure!(
        own["lsi_sysclk"]
            .as_object()
            .is_some_and(|v| v.contains_key("lsi_output_pin")),
        "factory-LSI direct-output fact must be explicit"
    );
    let lsi: cw32_data_serde::chip::core::peripheral::LsiSysclk =
        serde_json::from_value(own["lsi_sysclk"].clone())?;
    if classic {
        ensure!(
            serde_json::to_value(&clock.lsi_sysclk)? == own["lsi_sysclk"],
            "factory-LSI facts differ from own source"
        );
        ensure!(
            lsi.lsi_output_pin.as_deref() == Some("PB11")
                && lsi.lsi_output_allowed_af == [0, 2, 3, 4, 5, 6, 7]
                && lsi.rtc_allowed_sources == [0, 4, 5, 6, 7]
                && lsi.uarts == ["UART1", "UART2", "UART3"]
                && lsi.gpio_banks == ["GPIOA", "GPIOB", "GPIOC", "GPIOF"],
            "classic RTC/direct-output/consumer facts must remain complete"
        );
    }
    let exact_f002 = if f002 {
        validate_f002(root, chip, own, &lsi)?
    } else {
        false
    };
    ensure!(
        own["watchdog_source"] == "independent_rc10k"
            && own["gpio_inspection"] == "whole_bank_functional_handover"
            && own["factory_read_width_bits"] == 16
            && own["trim_write"] == "stopped_field_only"
            && own["waitcycle_preserved"] == true
            && own["cycle_timing_qualified"] == false,
        "factory-LSI operating contract changed"
    );
    let lock: Value =
        serde_json::from_slice(&fs::read(root.join("sources/evidence-sources.json"))?)?;
    let sources = own["sources"]
        .as_array()
        .context("missing LSI own sources")?;
    let expected = match line {
        "CW32F002" => [
            "vendor:CW32F002_UserManual_CN_V1.4.pdf",
            "vendor:CW32F002_DataSheet_CN_V1.2.pdf",
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
        _ => anyhow::bail!("unqualified factory-LSI family"),
    };
    ensure!(
        sources.len() == expected.len(),
        "incomplete LSI source roster"
    );
    for (source, id) in sources.iter().zip(expected) {
        ensure!(source["source_ref"] == id, "wrong own LSI source selection");
        let original = lock["artifacts"]
            .as_array()
            .context("missing source lock")?
            .iter()
            .find(|a| a["id"] == id)
            .context("LSI source is not canonical")?;
        ensure!(
            source["sha256"] == original["sha256"]
                && original["provenance"]["status"] == "selected"
                && original["provenance"]["chip_scope"]
                    .as_array()
                    .context("missing scope")?
                    .iter()
                    .any(|s| s == line),
            "LSI source is not selected own-family evidence"
        );
        let pages = source["pdf_pages_1_based"]
            .as_array()
            .context("missing pages")?;
        let printed = source["printed_pages"]
            .as_array()
            .context("missing printed pages")?;
        let count = original["provenance"]["pdf_page_count"]
            .as_u64()
            .context("missing page count")?;
        ensure!(
            !pages.is_empty()
                && pages.len() == printed.len()
                && pages.iter().zip(printed).all(|(p, q)| p
                    .as_u64()
                    .is_some_and(|p| p > 0 && p <= count && q.as_u64() == Some(p - 1))),
            "invalid LSI evidence pages"
        );
    }
    let peripheral = |name: &str| {
        core.peripherals
            .iter()
            .find(|p| p.name == name)
            .with_context(|| format!("missing LSI consumer {name}"))
    };
    let register = |name: &str, reg: &str, offset: u32| -> Result<&ir::FieldSet> {
        let p = peripheral(name)?
            .registers
            .as_ref()
            .context("missing LSI registers")?;
        let ir = registers.get(&p.kind).context("missing LSI register IR")?;
        let block = ir.blocks.get(&p.block).context("missing LSI block")?;
        let item = block
            .items
            .iter()
            .find(|r| r.name == reg && r.byte_offset == offset)
            .with_context(|| format!("wrong LSI register {name}.{reg}"))?;
        ensure!(item.array.is_none(), "unexpected LSI register array");
        let ir::BlockItemInner::Register(reg) = &item.inner else {
            anyhow::bail!("LSI object is not a register")
        };
        ensure!(
            reg.bit_size == 32 && matches!(reg.access, ir::Access::Read | ir::Access::ReadWrite),
            "LSI register is not readable u32"
        );
        ir.fieldsets
            .get(reg.fieldset.as_ref().context("missing LSI fields")?)
            .context("missing LSI fieldset")
    };
    let field = |fields: &ir::FieldSet, name: &str, bit: u32, size: u32| -> Result<()> {
        ensure!(
            fields.fields.iter().any(|f| f.name == name
                && f.bit_offset == ir::BitOffset::Regular(bit)
                && f.bit_size == size
                && f.array.is_none()),
            "LSI field mismatch: {name}"
        );
        Ok(())
    };
    let text = |v: &Value| v.as_str().map(str::to_owned).context("missing LSI text");
    let number = |v: &Value| {
        v.as_u64()
            .and_then(|x| u32::try_from(x).ok())
            .context("missing LSI number")
    };
    let addresses = own["peripheral_addresses"]
        .as_object()
        .context("missing LSI peripheral addresses")?;
    let expected_names: Vec<_> = ["SYSCTRL", "AWT"]
        .into_iter()
        .chain(classic.then_some("RTC"))
        .chain(lsi.uarts.iter().map(String::as_str))
        .chain(lsi.gpio_banks.iter().map(String::as_str))
        .collect();
    ensure!(
        addresses.len() == expected_names.len(),
        "incomplete LSI address roster"
    );
    for name in expected_names {
        ensure!(
            addresses[name].as_u64() == Some(u64::from(peripheral(name)?.address)),
            "LSI peripheral address mismatch: {name}"
        );
    }
    // Require complete unique identities before checking their source-bound IR
    // positions. Merely iterating supplied entries would accept omissions.
    let mut required_fields = if f002 {
        vec![
            ("SYSCTRL", "CR0", "SYSCLK"),
            ("SYSCTRL", "CR0", "PCLKPRS"),
            ("SYSCTRL", "CR0", "HCLKPRS"),
            ("SYSCTRL", "CR0", "KEY"),
            ("SYSCTRL", "CR1", "HSIEN"),
            ("SYSCTRL", "CR1", "HEXEN"),
            ("SYSCTRL", "CR1", "LSIEN"),
            ("SYSCTRL", "CR1", "KEY"),
            ("SYSCTRL", "IER", "LSIRDY"),
            ("SYSCTRL", "IER", "KEY"),
            ("SYSCTRL", "ISR", "LSIRDY"),
            ("SYSCTRL", "ISR", "LSISTABLE"),
            ("SYSCTRL", "LSI", "TRIM"),
            ("SYSCTRL", "LSI", "WAITCYCLE"),
            ("SYSCTRL", "LSI", "STABLE"),
            ("SYSCTRL", "MCO", "SOURCE"),
            ("AWT", "CR", "SRC"),
        ]
    } else {
        vec![
            ("SYSCTRL", "CR0", "SYSCLK"),
            ("SYSCTRL", "CR0", "KEY"),
            ("SYSCTRL", "CR1", "HSIEN"),
            ("SYSCTRL", "CR1", "HSEEN"),
            ("SYSCTRL", "CR1", "PLLEN"),
            ("SYSCTRL", "CR1", "LSIEN"),
            ("SYSCTRL", "CR1", "LSEEN"),
            ("SYSCTRL", "CR1", "LSECCS"),
            ("SYSCTRL", "CR1", "HSECCS"),
            ("SYSCTRL", "CR1", "CLKCCS"),
            ("SYSCTRL", "CR1", "KEY"),
            ("SYSCTRL", "IER", "LSIRDY"),
            ("SYSCTRL", "ISR", "LSIRDY"),
            ("SYSCTRL", "ISR", "HSESTABLE"),
            ("SYSCTRL", "ISR", "LSISTABLE"),
            ("SYSCTRL", "ISR", "LSESTABLE"),
            ("SYSCTRL", "LSI", "TRIM"),
            ("SYSCTRL", "LSI", "WAITCYCLE"),
            ("SYSCTRL", "LSI", "STABLE"),
            ("SYSCTRL", "HSE", "STABLE"),
            ("SYSCTRL", "LSE", "STABLE"),
            ("SYSCTRL", "MCO", "SOURCE"),
            ("RTC", "CR1", "SOURCE"),
            ("AWT", "CR", "SRC"),
            ("GPIOB", "AFRH", "AFR11"),
        ]
    };
    required_fields.extend(
        lsi.uarts
            .iter()
            .map(|name| (name.as_str(), "CR2", "SOURCE")),
    );
    required_fields.extend(
        lsi.gpio_banks
            .iter()
            .map(|name| (name.as_str(), "FILTER", "FLTCLK")),
    );
    let fields = own["register_fields"]
        .as_array()
        .context("missing LSI field roster")?;
    let mut actual_fields = fields
        .iter()
        .map(|f| {
            (
                f["peripheral"].as_str().unwrap_or(""),
                f["register"].as_str().unwrap_or(""),
                f["field"].as_str().unwrap_or(""),
            )
        })
        .collect::<Vec<_>>();
    required_fields.sort();
    actual_fields.sort();
    ensure!(
        actual_fields == required_fields && actual_fields.len() == if f002 { 22 } else { 32 },
        "LSI field roster must be exact, complete and unique"
    );
    let mut required_gates = ["AWT"]
        .into_iter()
        .chain(classic.then_some("RTC"))
        .chain(lsi.uarts.iter().map(String::as_str))
        .chain(lsi.gpio_banks.iter().map(String::as_str))
        .collect::<Vec<_>>();
    let gates = own["consumer_gates"]
        .as_array()
        .context("missing LSI gate roster")?;
    let mut actual_gates = gates
        .iter()
        .map(|g| g["peripheral"].as_str().unwrap_or(""))
        .collect::<Vec<_>>();
    required_gates.sort();
    actual_gates.sort();
    ensure!(
        actual_gates == required_gates && actual_gates.len() == if f002 { 6 } else { 9 },
        "LSI gate roster must be exact, complete and unique"
    );
    for fact in own["register_fields"]
        .as_array()
        .context("missing LSI fields")?
    {
        let fields = register(
            &text(&fact["peripheral"])?,
            &text(&fact["register"])?,
            number(&fact["byte_offset"])?,
        )?;
        field(
            fields,
            &text(&fact["field"])?,
            number(&fact["bit_offset"])?,
            number(&fact["bit_size"])?,
        )?;
    }
    for fact in own["consumer_gates"]
        .as_array()
        .context("missing LSI gates")?
    {
        let name = text(&fact["peripheral"])?;
        let gate = peripheral(&name)?
            .rcc_control
            .as_ref()
            .context("missing qualified LSI consumer gate")?;
        let reset = gate.reset.as_ref().context("missing LSI consumer reset")?;
        ensure!(
            gate.controller == "SYSCTRL"
                && gate.enable.register == text(&fact["register"])?
                && gate.enable.field == name
                && reset.register == text(&fact["reset_register"])?
                && reset.field == name
                && gate.enable_active_value
                && gate.reset_asserted_value == Some(false)
                && gate.enable_write_key.is_none()
                && fact["keyed"] == false
                && fact["enable_active_value"] == true
                && fact["reset_asserted_value"] == false
                && fact["gate_controls_work"] == name.starts_with("GPIO"),
            "LSI gate protocol differs from own source"
        );
        let en = register(
            "SYSCTRL",
            &gate.enable.register,
            number(&fact["byte_offset"])?,
        )?;
        let rst = register(
            "SYSCTRL",
            &reset.register,
            number(&fact["reset_byte_offset"])?,
        )?;
        field(en, &name, number(&fact["bit_offset"])?, 1)?;
        field(rst, &name, number(&fact["bit_offset"])?, 1)?;
        ensure!(
            !en.fields.iter().any(|f| f.name == "KEY"),
            "qualified LSI gate must be unkeyed"
        );
    }
    for (kind, expected) in [("uart", &lsi.uarts), ("gpio", &lsi.gpio_banks)] {
        let actual: Vec<_> = core
            .peripherals
            .iter()
            .filter(|p| {
                p.registers.as_ref().is_some_and(|r| {
                    if kind == "gpio" {
                        matches!(r.kind.as_ref(), "gpio" | "gpioc" | "gpiof")
                    } else {
                        r.kind == kind
                    }
                })
            })
            .map(|p| p.name.as_str())
            .collect();
        ensure!(
            actual == expected.iter().map(String::as_str).collect::<Vec<_>>(),
            "incomplete LSI {kind} roster"
        );
    }
    if f002 {
        // The independent complete manual AF-table exclusion is cross-checked
        // against every entry in the frozen full SDK catalog, including entries
        // not merged as supported routes. None alone is never absence evidence.
        let af_bytes = fs::read(root.join("cw32-data/af/cw32f002.yaml"))?;
        ensure!(
            format!("{:x}", Sha256::digest(&af_bytes))
                == own["no_lsi_output_evidence"]["sdk_catalog_sha256"],
            "F002 complete SDK AF catalog changed"
        );
        let af = parse_policy(&af_bytes)?;
        let sdk = lock["artifacts"]
            .as_array()
            .context("missing source lock")?
            .iter()
            .find(|a| a["id"] == "vendor:CW32F002_StandardPeripheralLib_V1.2.zip")
            .context("missing F002 SDK source")?;
        let header_path = own["no_lsi_output_evidence"]["sdk_header_source_ref"]
            .as_str()
            .and_then(|s| s.strip_prefix("member:"))
            .context("missing F002 AF member")?;
        let header = sdk["members"]
            .as_array()
            .context("missing SDK members")?
            .iter()
            .find(|m| m["path"] == header_path)
            .context("missing F002 AF header source")?;
        ensure!(
            af["profile"] == "CW32F002"
                && af["source"]["sdk_sha256"] == sdk["sha256"]
                && sdk["sha256"]
                    == "108b6e1483669933e789c6868cf0a595ebba6bc4b77f78f5a45bb8bc96cbeffc"
                && af["source"]["header_sha256"] == header["sha256"],
            "F002 AF absence check must use its own locked header"
        );
        for (section, count) in [("routes", 107), ("gpio_selection", 21), ("unresolved", 15)] {
            let entries = af[section]
                .as_array()
                .context("missing complete F002 AF section")?;
            ensure!(
                entries.len() == count
                    && entries.iter().all(|r| r["function"]
                        .as_str()
                        .is_some_and(|f| !f.contains("LSI"))
                        && r["source_macro"]
                            .as_str()
                            .is_some_and(|f| !f.contains("LSI"))),
                "F002 complete AF catalog must contain no independent LSI output"
            );
        }
        ensure!(
            core.peripherals
                .iter()
                .all(|p| p.pins.iter().all(|r| !r.signal.contains("LSI"))),
            "F002 selected pin routes contain an unsupported LSI output"
        );
        let sysctrl = peripheral("SYSCTRL")?
            .registers
            .as_ref()
            .context("missing SYSCTRL IP")?;
        ensure!(
            sysctrl.version == "cw32f002_v1" && own["sysctrl_version"] == sysctrl.version,
            "F002 factory-LSI requires its validated SYSCTRL IP"
        );
        ensure!(
            core.peripherals.iter().all(|p| !matches!(
                p.name.as_str(),
                "RTC" | "AUTOTRIM" | "LCD" | "LPTIM" | "LPTIM1" | "LPTIM2"
            )),
            "F002 absent direct roots differ from selected hardware"
        );
        let ir = registers
            .get(&sysctrl.kind)
            .context("missing F002 SYSCTRL IR")?;
        let block = ir
            .blocks
            .get(&sysctrl.block)
            .context("missing F002 SYSCTRL block")?;
        ensure!(
            block
                .items
                .iter()
                .all(|r| !matches!(r.name.as_str(), "PLL" | "LSE" | "HSE")),
            "F002 system source absence differs from selected hardware"
        );
        let cr1 = register("SYSCTRL", "CR1", 4)?;
        ensure!(
            cr1.fields.iter().all(|f| !matches!(
                f.name.as_str(),
                "PLLEN" | "LSEEN" | "HSECCS" | "LSECCS" | "CLKCCS"
            )),
            "F002 CCS/system-source absence differs from selected hardware"
        );
    } else {
        let rtc = peripheral("RTC")?
            .rtc_calendar
            .as_ref()
            .context("missing factory-LSI RTC facts")?;
        ensure!(
            lsi.nominal_hz == rtc.nominal_hz
                && lsi.minimum_hz == rtc.minimum_hz
                && lsi.maximum_hz == rtc.maximum_hz
                && lsi.supply_mv == rtc.supply_mv
                && lsi.temperature_c == rtc.temperature_c
                && lsi.factory_trim_address == rtc.factory_trim_address,
            "same-source LSI/RTC qualification diverges"
        );
    }
    ensure!(
        core.interrupts
            .iter()
            .any(|i| i.name == "RCC" && u16::from(i.number) == lsi.rcc_irq),
        "RCC pending observer IRQ changed"
    );
    // All source/package/register/consumer checks finish before the only injection.
    if exact_f002 {
        let clock = core
            .peripherals
            .iter_mut()
            .find(|p| p.name == "SYSCTRL")
            .and_then(|p| p.clock_limits.as_mut())
            .context("missing SYSCTRL clocks")?;
        clock.lsi_sysclk = Some(lsi);
    }
    Ok(())
}

fn validate_f002(
    root: &Path,
    chip: &crate::ChipInput,
    own: &Value,
    lsi: &cw32_data_serde::chip::core::peripheral::LsiSysclk,
) -> Result<bool> {
    use serde_json::json;
    let parts = json!([
        {"name":"CW32F002F3P7", "package":"TSSOP20"},
        {"name":"CW32F002F3U7", "package":"QFN20"}
    ]);
    ensure!(
        own["exact_parts"] == parts,
        "F002 exact qualification roster changed"
    );
    let catalog = parse_policy(&fs::read(root.join("cw32-data/parts.yaml"))?)?;
    let roster = catalog["parts"]
        .as_array()
        .context("missing exact parts catalog")?;
    for part in parts.as_array().unwrap() {
        let selected = roster
            .iter()
            .filter(|p| p["name"] == part["name"])
            .collect::<Vec<_>>();
        ensure!(
            selected.len() == 1
                && selected[0]["family"] == "CW32F002"
                && selected[0]["package"] == part["package"]
                && selected[0]["datasheet_source_id"] == "CW32F002_datasheet"
                && catalog["sources"]["CW32F002_datasheet"]["source_ref"]
                    == "vendor:CW32F002_DataSheet_CN_V1.2.pdf"
                && catalog["sources"]["CW32F002_datasheet"]["sha256"]
                    == "6d0c5c37d069e5b4e33d394b0be6c53938868e9375e7b4bbbbdd40d62bb9d506",
            "F002 exact qualification differs from own datasheet/catalog"
        );
    }
    let selected = parts
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == chip.name);
    if let Some(part) = selected {
        ensure!(
            chip.packages.len() == 1 && part["package"] == chip.packages[0].package,
            "F002 exact qualification requires the reviewed physical package"
        );
    }
    ensure!(
        lsi.nominal_hz == 32_800
            && lsi.minimum_hz == 31_160
            && lsi.maximum_hz == 34_440
            && lsi.supply_mv == (1650, 5500)
            && lsi.temperature_c == (-40, 105)
            && lsi.factory_trim_address == 0x0010_07ba
            && lsi.rcc_irq == 4
            && lsi.rtc_allowed_sources.is_empty()
            && lsi.lsi_output_pin.is_none()
            && lsi.lsi_output_allowed_af.is_empty()
            && lsi.awt_allowed_sources == [0, 2, 3, 4]
            && lsi.uart_allowed_sources == [0, 1]
            && lsi.uarts == ["UART1", "UART2"]
            && lsi.gpio_banks == ["GPIOA", "GPIOB", "GPIOC"]
            && lsi.gpio_filter_allowed_sources == [0, 1, 2, 3, 4, 6]
            && lsi.mco_allowed_sources == [0, 1, 2, 3, 5, 8, 9],
        "F002 source envelope or complete selector facts changed"
    );
    ensure!(
        own["absent_direct_roots"]
            == json!([
                "RTC",
                "system_PLL",
                "system_LSE",
                "HSE_CCS",
                "LSE_CCS",
                "AUTOTRIM",
                "LCD",
                "LPTIM",
                "LSI_OUT"
            ])
            && own["other_clock_sources"]
                == json!({"IWDT":["RC10K"], "VC":["PCLK","RC150K"], "LVD":["HSIOSC","RC150K"]})
            && own["ready_observers"] == json!(["IER.LSIRDY", "ISR.LSIRDY", "NVIC.RCC.pending"])
            && own["stable_observers"] == json!(["LSI.STABLE", "ISR.LSISTABLE"])
            && own["no_lsi_output_evidence"]
                == json!({
            "source_ref":"vendor:CW32F002_UserManual_CN_V1.4.pdf",
            "pdf_pages_1_based":[105], "printed_pages":[104], "complete_af_catalog":true,
            "sdk_catalog":"cw32-data/af/cw32f002.yaml",
            "sdk_catalog_sha256":"ad2faf10e3271b0f84ef045fe69cacc8dc170538240f9df1d1cc168270080e1e",
            "sdk_header_source_ref":"member:cw32f002/Libraries/inc/cw32f002_gpio.h"}),
        "F002 complete observer/negative/direct-output evidence changed"
    );
    ensure!(
        own["sources"][0]["sha256"]
            == "e453b3f82d9d180a86cd5f7cb18d858d7f228afc8101d87c700ca9d5c02d5add"
            && own["sources"][1]["sha256"]
                == "6d0c5c37d069e5b4e33d394b0be6c53938868e9375e7b4bbbbdd40d62bb9d506"
            && own["sources"][0]["pdf_pages_1_based"]
                == json!([
                    37, 39, 40, 41, 44, 45, 46, 47, 48, 49, 50, 51, 54, 55, 57, 58, 59, 60, 61, 62,
                    63, 65, 70, 72, 73, 74, 75, 76, 101, 105, 106, 113, 123, 125, 129, 132, 136,
                    137, 152, 168, 178, 185, 186, 188, 194, 195, 203, 227, 228, 236, 268, 303, 340,
                    346, 351, 356
                ])
            && own["sources"][1]["pdf_pages_1_based"] == json!([8, 22, 23, 24, 31, 37]),
        "F002 selected source identity or evidence pages changed"
    );
    Ok(selected.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_lsi_policy_keys_are_rejected_at_every_depth() {
        for input in [
            "families: {}\nfamilies: {}\n",
            "families:\n  CW32F020: {}\n  CW32F020: {}\n",
            "families:\n  CW32F020:\n    lsi_sysclk: {nominal_hz: 32800, nominal_hz: 1}\n",
        ] {
            assert!(
                parse_policy(input.as_bytes())
                    .unwrap_err()
                    .to_string()
                    .contains("duplicate")
            );
        }
    }
}
