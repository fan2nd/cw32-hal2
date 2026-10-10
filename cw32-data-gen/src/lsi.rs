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
    let exact_family = matches!(line, "CW32F002" | "CW32F003");
    let l031 = line == "CW32L031";
    let r031 = line == "CW32R031";
    let w031 = line == "CW32W031";
    let native_family = l031 || r031 || w031;
    let has_rtc = classic || native_family;
    let qualified = classic || exact_family || native_family;
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
            == [
                "CW32A030", "CW32F002", "CW32F003", "CW32F020", "CW32F030", "CW32L031", "CW32R031",
                "CW32W031"
            ],
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
    // explicit null requires independently proved own-family hardware absence.
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
    let exact_part = if exact_family {
        validate_exact_family(root, chip, line, own, &lsi)?
    } else if native_family {
        validate_native_family(root, chip, line, own, &lsi)?
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
    if r031 || w031 {
        validate_native_source_facts(line, own, &lock)?;
    }
    let sources = own["sources"]
        .as_array()
        .context("missing LSI own sources")?;
    let expected = match line {
        "CW32F002" => [
            "vendor:CW32F002_UserManual_CN_V1.4.pdf",
            "vendor:CW32F002_DataSheet_CN_V1.2.pdf",
        ],
        "CW32F003" => [
            "vendor:CW32F003_UserManual_CN_V2.3.pdf",
            "vendor:CW32F003_DataSheet_CN_V1.9.pdf",
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
        "CW32L031" => [
            "vendor:CW32L031_UserManual_CN_V1.6.pdf",
            "vendor:CW32L031_DataSheet_CN_V1.9.pdf",
        ],
        "CW32R031" => [
            "vendor:CW32R031_UserManual_CN_V1.3.pdf",
            "vendor:CW32R031_DataSheet_CN_V1.2.pdf",
        ],
        "CW32W031" => [
            "vendor:CW32W031_UserManual_CN_V1.4.pdf",
            "vendor:CW32W031_DataSheet_CN_V1.3.pdf",
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
        if w031 {
            ensure!(
                lock["artifacts"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|a| a["id"] == id)
                    .count()
                    == 1
                    && original["provenance"]["chip_scope"] == serde_json::json!(["CW32W031"]),
                "W031 requires unique selected own-family RM/DS evidence"
            );
        }
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
        if r031 || w031 {
            ensure!(
                block.items.iter().filter(|r| r.name == reg).count() == 1,
                "{line} register identity must be unique: {name}.{reg}"
            );
        }
        let ir::BlockItemInner::Register(register) = &item.inner else {
            anyhow::bail!("LSI object is not a register")
        };
        ensure!(
            register.bit_size == 32
                && matches!(register.access, ir::Access::Read | ir::Access::ReadWrite),
            "LSI register is not readable u32"
        );
        if native_family {
            ensure!(
                register.access
                    == if name == "SYSCTRL" && reg == "ISR" {
                        ir::Access::Read
                    } else {
                        ir::Access::ReadWrite
                    },
                "{line} own register access changed: {name}.{reg}"
            );
        }
        if r031 || w031 {
            ensure!(
                register.fieldset.as_deref() == Some(reg),
                "{line} register fieldset identity changed: {name}.{reg}"
            );
        }
        ir.fieldsets
            .get(register.fieldset.as_ref().context("missing LSI fields")?)
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
        if r031 || w031 {
            ensure!(
                fields.fields.iter().filter(|f| f.name == name).count() == 1,
                "{line} field identity must be unique: {name}"
            );
        }
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
        .chain(has_rtc.then_some("RTC"))
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
    let mut required_fields = if exact_family {
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
    } else if native_family {
        vec![
            ("SYSCTRL", "CR0", "SYSCLK"),
            ("SYSCTRL", "CR0", "PCLKPRS"),
            ("SYSCTRL", "CR0", "HCLKPRS"),
            ("SYSCTRL", "CR0", "KEY"),
            ("SYSCTRL", "CR1", "HSIEN"),
            ("SYSCTRL", "CR1", "HSEEN"),
            ("SYSCTRL", "CR1", "LSIEN"),
            ("SYSCTRL", "CR1", "LSEEN"),
            ("SYSCTRL", "CR1", "LSELOCK"),
            ("SYSCTRL", "CR1", "LSECCS"),
            ("SYSCTRL", "CR1", "HSECCS"),
            ("SYSCTRL", "CR1", "CLKCCS"),
            ("SYSCTRL", "CR1", "KEY"),
            ("SYSCTRL", "IER", "LSIRDY"),
            ("SYSCTRL", "IER", "KEY"),
            ("SYSCTRL", "ISR", "LSIRDY"),
            ("SYSCTRL", "ISR", "HSISTABLE"),
            ("SYSCTRL", "ISR", "HSESTABLE"),
            ("SYSCTRL", "ISR", "LSISTABLE"),
            ("SYSCTRL", "ISR", "LSESTABLE"),
            ("SYSCTRL", "ISR", "HSEFAIL"),
            ("SYSCTRL", "ISR", "HSEFAULT"),
            ("SYSCTRL", "ISR", "LSEFAIL"),
            ("SYSCTRL", "ISR", "LSEFAULT"),
            ("SYSCTRL", "HSI", "TRIM"),
            ("SYSCTRL", "HSI", "DIV"),
            ("SYSCTRL", "HSI", "STABLE"),
            ("SYSCTRL", "HSE", "DRIVER"),
            ("SYSCTRL", "HSE", "FREQRANGE"),
            ("SYSCTRL", "HSE", "WAITCYCLE"),
            ("SYSCTRL", "HSE", "MODE"),
            ("SYSCTRL", "HSE", "FLT"),
            ("SYSCTRL", "HSE", "DETCNT"),
            ("SYSCTRL", "HSE", "STABLE"),
            ("SYSCTRL", "LSI", "TRIM"),
            ("SYSCTRL", "LSI", "WAITCYCLE"),
            ("SYSCTRL", "LSI", "STABLE"),
            ("SYSCTRL", "LSE", "DRIVER"),
            ("SYSCTRL", "LSE", "AMP"),
            ("SYSCTRL", "LSE", "WAITCYCLE"),
            ("SYSCTRL", "LSE", "MODE"),
            ("SYSCTRL", "LSE", "STABLE"),
            ("SYSCTRL", "MCO", "SOURCE"),
            ("RTC", "CR1", "SOURCE"),
            ("AWT", "CR", "SRC"),
            ("GPIOB", "AFRH", "AFR11"),
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
        actual_fields == required_fields
            && actual_fields.len()
                == if exact_family {
                    22
                } else if native_family {
                    53
                } else {
                    32
                },
        "LSI field roster must be exact, complete and unique"
    );
    let mut required_gates = ["AWT"]
        .into_iter()
        .chain(has_rtc.then_some("RTC"))
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
        actual_gates == required_gates && actual_gates.len() == if exact_family { 6 } else { 9 },
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
            !en.fields.iter().any(|f| f.name == "KEY")
                && (!(r031 || w031) || !rst.fields.iter().any(|f| f.name == "KEY")),
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
    if native_family {
        let sysctrl = peripheral("SYSCTRL")?
            .registers
            .as_ref()
            .context("missing native LSI SYSCTRL IP")?;
        ensure!(
            sysctrl.version == "cw32l031_v1" && own["sysctrl_version"] == sysctrl.version,
            "{line} factory-LSI requires its own validated SYSCTRL IP"
        );
        let ir = registers
            .get(&sysctrl.kind)
            .context("missing native LSI SYSCTRL IR")?;
        let block = ir
            .blocks
            .get(&sysctrl.block)
            .context("missing native LSI SYSCTRL block")?;
        ensure!(
            block
                .items
                .iter()
                .all(|r| !matches!(r.name.as_str(), "PLL" | "HEX"))
                && register("SYSCTRL", "CR1", 4)?
                    .fields
                    .iter()
                    .all(|f| !matches!(f.name.as_str(), "PLLEN" | "HEXEN" | "LSILOCK"))
                && core.peripherals.iter().all(|p| !matches!(
                    p.name.as_str(),
                    "AUTOTRIM" | "LCD" | "LPTIM" | "LPTIM1" | "LPTIM2"
                )),
            "{line} source/root absence differs from selected hardware"
        );
        // chiptool IR keeps register access; the existing separate overlay is
        // the source of per-field read-only restrictions in the generated PAC.
        let access = parse_policy(&fs::read(root.join("cw32-data/field-access.yaml"))?)?;
        let access = access["registers"]["sysctrl_cw32l031_v1"]
            .as_array()
            .context("missing native LSI source status access")?;
        for (name, bit) in [("HSI", 15), ("HSE", 19), ("LSI", 15), ("LSE", 15)] {
            ensure!(
                access
                    .iter()
                    .filter(|f| f["block"] == "SYSCTRL"
                        && f["register"] == name
                        && f["fieldset"] == name
                        && f["field"] == "STABLE"
                        && f["bit_offset"] == bit
                        && f["bit_size"] == 1)
                    .count()
                    == 1,
                "{line} source STABLE must retain its reviewed read-only overlay"
            );
        }
        if r031 || w031 {
            ensure!(
                access.len() == 4,
                "{line} requires exactly four native STABLE overlays"
            );
        }
        if exact_part {
            let bonded = chip.name != "CW32L031F8U6";
            ensure!(
                core.pins.iter().any(|p| p.name == "PB11") == bonded
                    && chip.packages[0]
                        .pins
                        .iter()
                        .any(|p| p.signals.iter().any(|s| s == "PB11"))
                        == bonded,
                "{line} PB11 physical bonding differs from own package evidence"
            );
            if r031 || w031 {
                let pins = if w031 {
                    [
                        ("PB11", "16"),
                        ("PF0", "63"),
                        ("PF1", "64"),
                        ("PC14", "61"),
                        ("PC15", "62"),
                    ]
                } else {
                    [
                        ("PB11", "25"),
                        ("PF0", "4"),
                        ("PF1", "5"),
                        ("PC14", "2"),
                        ("PC15", "3"),
                    ]
                };
                for (pin, position) in pins {
                    if w031 {
                        ensure!(
                            chip.packages[0]
                                .pins
                                .iter()
                                .filter(|p| p.signals.iter().any(|s| s == pin))
                                .count()
                                == 1
                                && chip.packages[0]
                                    .pins
                                    .iter()
                                    .filter(|p| p.position == position)
                                    .count()
                                    == 1,
                            "W031 exact oscillator/output package pin must be unique: {pin}"
                        );
                    }
                    ensure!(
                        core.pins.iter().filter(|p| p.name == pin).count() == 1
                            && chip.packages[0]
                                .pins
                                .iter()
                                .filter(|p| p.position == position
                                    && p.signals.iter().any(|s| s == pin))
                                .count()
                                == 1,
                        "{line} exact oscillator/output bonding changed: {pin}"
                    );
                }
            }
        }
    }
    if exact_family {
        // The independent complete manual AF-table exclusion is cross-checked
        // against every entry in the frozen full SDK catalog, including entries
        // not merged as supported routes. None alone is never absence evidence.
        let af_path = own["no_lsi_output_evidence"]["sdk_catalog"]
            .as_str()
            .context("missing own-family AF catalog")?;
        let af_bytes = fs::read(root.join(af_path))?;
        ensure!(
            format!("{:x}", Sha256::digest(&af_bytes))
                == own["no_lsi_output_evidence"]["sdk_catalog_sha256"],
            "F002/F003 complete SDK AF catalog changed"
        );
        let af = parse_policy(&af_bytes)?;
        let (sdk_ref, sdk_sha256, af_counts) = match line {
            "CW32F002" => (
                "vendor:CW32F002_StandardPeripheralLib_V1.2.zip",
                "108b6e1483669933e789c6868cf0a595ebba6bc4b77f78f5a45bb8bc96cbeffc",
                [107, 21, 15],
            ),
            "CW32F003" => (
                "vendor:CW32F003_StandardPeripheralLib_V1.7.zip",
                "fc2d753bfeaa0300d73b67f4c2d4f912cd065e6cb6d465935a1ad161bdd57333",
                [133, 21, 11],
            ),
            _ => unreachable!("bounded exact-only family"),
        };
        let sdk = lock["artifacts"]
            .as_array()
            .context("missing source lock")?
            .iter()
            .find(|a| a["id"] == sdk_ref)
            .context("missing own-family SDK source")?;
        let header_path = own["no_lsi_output_evidence"]["sdk_header_source_ref"]
            .as_str()
            .and_then(|s| s.strip_prefix("member:"))
            .context("missing own-family AF member")?;
        let header = sdk["members"]
            .as_array()
            .context("missing SDK members")?
            .iter()
            .find(|m| m["path"] == header_path)
            .context("missing own-family AF header source")?;
        ensure!(
            af["profile"] == line
                && af["source"]["sdk_sha256"] == sdk["sha256"]
                && sdk["sha256"] == sdk_sha256
                && sdk["provenance"]["status"] == "selected"
                && sdk["provenance"]["chip_scope"]
                    .as_array()
                    .is_some_and(|scope| scope.iter().any(|family| family == line))
                && af["source"]["header_sha256"] == header["sha256"],
            "F002/F003 AF absence check must use its own locked header"
        );
        for (section, count) in ["routes", "gpio_selection", "unresolved"]
            .into_iter()
            .zip(af_counts)
        {
            let entries = af[section]
                .as_array()
                .context("missing complete own-family AF section")?;
            ensure!(
                entries.len() == count
                    && entries.iter().all(|r| r["function"]
                        .as_str()
                        .is_some_and(|f| !f.contains("LSI"))
                        && r["source_macro"]
                            .as_str()
                            .is_some_and(|f| !f.contains("LSI"))),
                "F002/F003 complete AF catalog must contain no independent LSI output"
            );
        }
        ensure!(
            core.peripherals
                .iter()
                .all(|p| p.pins.iter().all(|r| !r.signal.contains("LSI"))),
            "F002/F003 selected pin routes contain an unsupported LSI output"
        );
        let sysctrl = peripheral("SYSCTRL")?
            .registers
            .as_ref()
            .context("missing SYSCTRL IP")?;
        ensure!(
            own["sysctrl_version"] == sysctrl.version,
            "F002/F003 factory-LSI requires its validated SYSCTRL IP"
        );
        ensure!(
            core.peripherals.iter().all(|p| !matches!(
                p.name.as_str(),
                "RTC" | "AUTOTRIM" | "LCD" | "LPTIM" | "LPTIM1" | "LPTIM2"
            )),
            "F002/F003 absent direct roots differ from selected hardware"
        );
        let ir = registers
            .get(&sysctrl.kind)
            .context("missing own-family SYSCTRL IR")?;
        let block = ir
            .blocks
            .get(&sysctrl.block)
            .context("missing own-family SYSCTRL block")?;
        ensure!(
            block
                .items
                .iter()
                .all(|r| !matches!(r.name.as_str(), "PLL" | "LSE" | "HSE")),
            "F002/F003 system source absence differs from selected hardware"
        );
        let cr1 = register("SYSCTRL", "CR1", 4)?;
        ensure!(
            cr1.fields.iter().all(|f| !matches!(
                f.name.as_str(),
                "PLLEN" | "LSEEN" | "HSECCS" | "LSECCS" | "CLKCCS"
            )),
            "F002/F003 CCS/system-source absence differs from selected hardware"
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
    let observer_irq = if native_family { "SYSCTRL" } else { "RCC" };
    ensure!(
        core.interrupts
            .iter()
            .any(|i| i.name == observer_irq && u16::from(i.number) == lsi.rcc_irq),
        "RCC pending observer IRQ changed"
    );
    // All source/package/register/consumer checks finish before the only injection.
    if exact_part {
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

// Independently bound L031, R031 and W031 facts share their RTC/external-detector,
// PB11 and nine-gate structural validator. IP reuse never admits another family.
fn validate_native_family(
    root: &Path,
    chip: &crate::ChipInput,
    line: &str,
    own: &Value,
    lsi: &cw32_data_serde::chip::core::peripheral::LsiSysclk,
) -> Result<bool> {
    use serde_json::json;
    let binding = match line {
        "CW32L031" => json!({
            "parts": [
                {"name":"CW32L031C8T6", "package":"LQFP48"},
                {"name":"CW32L031C8U6", "package":"QFN48"},
                {"name":"CW32L031F8U6", "package":"QFN20"}
            ],
            "supply_mv": [1650, 5500],
            "rm_sha256": "4288cfd97b56385059c5a283f69972047af4773ef8bbc4d8b51d8155fb17a760",
            "ds_sha256": "90525f4085d00e9d586a991c24f2e4398d92a41e6cc1402c413e963423a35855",
            "rm_pages": [
                47, 54, 55, 56, 57, 58, 59, 60, 61, 64, 67, 68, 69, 70, 71, 72, 73, 74, 75, 76,
                77, 78, 79, 80, 81, 82, 89, 91, 92, 94, 95, 107, 117, 137, 141, 142, 150, 159,
                160, 161, 165, 168, 169, 170, 171, 172, 179, 180, 303, 304, 321, 322, 353, 475,
                488, 493
            ],
            "ds_pages": [9, 10, 23, 24, 25, 26, 27, 30, 32, 33, 38, 47, 77, 78],
            "pb11_bonded": {
                "CW32L031C8T6": true,
                "CW32L031C8U6": true,
                "CW32L031F8U6": false
            },
            "conservative_policy": {
                "pb11_unbonded_register_inspection": true,
                "gpio_filter_7": "reject_documented_awt_overflow",
                "mco_7": "reject_undocumented",
                "pb11_blank_af_2_4": "reject_undocumented",
                "cold_external_detectors": "reject_either_enabled",
                "ccs_and_lselock": "preserve_configurable",
                "rtc_lsi_bounds": "rate_only_all_sysclk"
            }
        }),
        "CW32R031" => json!({
            "parts": [
                {"name":"CW32R031C8U6", "package":"QFN48"}
            ],
            "supply_mv": [2200, 3600],
            "rm_sha256": "fbee9b6942be9fa09f00c946705644d5356c4249f3e2280e3dfe5f0cb342eddb",
            "ds_sha256": "88759314fa4cf8b6caf7098df4829489179e27aa3752a13de85ce955b29c5805",
            "rm_pages": [
                49, 56, 58, 61, 69, 70, 71, 72, 73, 74, 75, 76, 77, 78, 79, 80, 81, 82, 83, 84,
                91, 93, 94, 109, 139, 142, 143, 144, 152, 167, 170, 171, 172, 173, 174, 182,
                306, 307, 308, 309, 356, 477, 485, 490, 495, 497, 498, 499, 500, 511, 515, 516,
                518, 520, 533
            ],
            "ds_pages": [9, 10, 11, 13, 14, 15, 16, 18, 28, 29, 30, 31, 32, 33, 35, 40, 42, 54],
            "pb11_bonded": {
                "CW32R031C8U6": true
            },
            "conservative_policy": {
                "gpio_filter_7": "reject_documented_awt_overflow",
                "mco_7": "reject_undocumented",
                "pb11_blank_af_2_4": "reject_undocumented",
                "cold_external_detectors": "reject_either_enabled",
                "ccs_and_lselock": "preserve_configurable",
                "rtc_lsi_bounds": "rate_only_all_sysclk"
            }
        }),
        "CW32W031" => json!({
            "parts": [{"name": "CW32W031R8U6", "package": "QFN64"}],
            "supply_mv": [2000, 3600],
            "rm_sha256": "b6973677946a9332b0e5b3e954119768aa40469d44140a73e18648e9419bedc9",
            "ds_sha256": "45ec43e6370956d09f9b9aa4c0f6c83fb0661f576d2d2bf64e0a6d7c4e203d3c",
            "rm_pages": [
                48, 49, 55, 56, 57, 58, 59, 60, 61, 65, 67, 68, 69, 70, 71, 72, 73, 74, 75, 76,
                77, 78, 79, 80, 81, 82, 83, 90, 92, 93, 108, 109, 118, 142, 143, 151, 159, 160,
                166, 169, 170, 171, 172, 173, 174, 175, 176, 177, 178, 180, 181, 306, 307, 308,
                309, 322, 323, 356, 363, 366, 478, 486, 491, 496, 498, 499, 500, 501, 502, 503,
                504, 505, 506, 507, 508, 509, 510, 511, 512, 513, 514, 515, 516, 517, 518, 519,
                520, 521, 522, 523, 524, 525, 526, 527, 528, 529, 530, 531, 532, 533
            ],
            "ds_pages": [9, 14, 16, 26, 27, 28, 29, 30, 32, 33, 34, 41, 42, 52, 53, 71],
            "pb11_bonded": {"CW32W031R8U6": true},
            "conservative_policy": {
                "gpio_filter_7": "reject_documented_awt_overflow",
                "mco_7": "reject_undocumented",
                "pb11_blank_af_2_4": "reject_undocumented",
                "cold_external_detectors": "reject_either_enabled",
                "ccs_and_lselock": "preserve_configurable",
                "rtc_lsi_bounds": "rate_only_all_sysclk"
            }
        }),
        _ => anyhow::bail!("unreviewed native factory-LSI family"),
    };
    let parts = &binding["parts"];
    ensure!(
        own["exact_parts"] == *parts && own["sysctrl_version"] == "cw32l031_v1",
        "{line} exact factory-LSI qualification roster or IP changed"
    );
    let memory = json!([
        {"name":"FLASH", "kind":"flash", "address":0, "size":65_536},
        {"name":"RAM", "kind":"ram", "address":0x2000_0000, "size":8_192}
    ]);
    let catalog = parse_policy(&fs::read(root.join("cw32-data/parts.yaml"))?)?;
    let datasheet_id = format!("{line}_datasheet");
    let roster = catalog["parts"]
        .as_array()
        .context("missing native LSI parts catalog")?;
    for part in parts.as_array().unwrap() {
        let selected = roster
            .iter()
            .filter(|p| p["name"] == part["name"])
            .collect::<Vec<_>>();
        ensure!(
            selected.len() == 1
                && selected[0]["family"] == line
                && selected[0]["feature"] == part["name"].as_str().unwrap().to_ascii_lowercase()
                && selected[0]["package"] == part["package"]
                && selected[0]["memory"] == memory
                && selected[0]["datasheet_source_id"] == datasheet_id,
            "{line} exact factory-LSI package/memory differs from own catalog"
        );
    }
    ensure!(
        catalog["sources"][&datasheet_id]["source_ref"] == own["sources"][1]["source_ref"]
            && catalog["sources"][&datasheet_id]["sha256"] == binding["ds_sha256"],
        "{line} physical package evidence is not the selected own datasheet"
    );
    let selected = parts
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == chip.name);
    if let Some(part) = selected {
        ensure!(
            chip.packages.len() == 1
                && part["package"] == chip.packages[0].package
                && serde_json::to_value(&chip.memory)? == json!([memory]),
            "{line} exact factory-LSI requires the reviewed physical package and memory"
        );
    }
    ensure!(
        lsi.nominal_hz == 32_800
            && lsi.minimum_hz == 31_816
            && lsi.maximum_hz == 33_784
            && serde_json::to_value(lsi.supply_mv)? == binding["supply_mv"]
            && lsi.temperature_c == (-40, 85)
            && lsi.factory_trim_address == 0x0010_0a02
            && lsi.rcc_irq == 4
            && lsi.rtc_allowed_sources == [0, 4, 5, 6, 7]
            && lsi.awt_allowed_sources == [0, 2, 3, 4]
            && lsi.uart_allowed_sources == [0, 1, 2]
            && lsi.uarts == ["UART1", "UART2", "UART3"]
            && lsi.gpio_banks == ["GPIOA", "GPIOB", "GPIOC", "GPIOF"]
            && lsi.gpio_filter_allowed_sources == [0, 1, 2, 3, 4, 6]
            && lsi.mco_allowed_sources == [0, 1, 2, 3, 5, 6, 8, 9]
            && lsi.lsi_output_pin.as_deref() == Some("PB11")
            && lsi.lsi_output_allowed_af == [0, 3, 5, 6, 7],
        "{line} own factory-LSI envelope or complete consumer facts changed"
    );
    ensure!(
        own["sources"][0]["sha256"] == binding["rm_sha256"]
            && own["sources"][1]["sha256"] == binding["ds_sha256"]
            && own["sources"][0]["pdf_pages_1_based"] == binding["rm_pages"]
            && own["sources"][1]["pdf_pages_1_based"] == binding["ds_pages"],
        "{line} own source identity or evidence pages changed"
    );
    ensure!(
        own["ready_observers"] == json!(["IER.LSIRDY", "ISR.LSIRDY", "NVIC.SYSCTRL.pending"])
            && own["stable_observers"] == json!(["LSI.STABLE", "ISR.LSISTABLE"])
            && own["absent_direct_roots"]
                == json!(["system_PLL", "system_HEX", "AUTOTRIM", "LCD", "LPTIM"])
            && own["other_clock_sources"]
                == json!({
                    "IWDT":["RC10K"], "VC":["PCLK","RC150K"], "LVD":["HSIOSC","RC150K"]
                })
            && own["pb11_bonded"] == binding["pb11_bonded"]
            && own["conservative_policy"] == binding["conservative_policy"],
        "{line} complete root/observer/bonding and conservative policy changed"
    );
    Ok(selected.is_some())
}

// R031 and W031 independently bind the same 53 positions and nine gate pairs.
// W031's selected SVD is 7b3f7cd1e1e9b311f8bf6da711b9af0a144040883a231dfcc93b9cd2bafe84d0;
// the separate literal bindings below prevent shared IP from granting admission.
// Per-field WO/RO evidence is retained because chiptool IR loses that access.
fn validate_native_source_facts(line: &str, own: &Value, lock: &Value) -> Result<()> {
    use serde_json::json;
    let binding = match line {
        "CW32R031" => json!({
            "sdk_source": {
                "source_ref": "vendor:CW32R031_StandardPeripheralLib_V1.1.zip",
                "sha256": "cec9df232d64b53b638c8a372f50fde1fd677f72c04bb3ae467b8138d8433082"
            },
            "sdk_members": [
                {
                    "source_ref": "member:cw32r031/IdeSupport/MDK/WHXY.CW32R031_DFP.1.0.2/SVD/CW32R031.svd",
                    "sha256": "0d9273507521d7f63e7614e9ddd445a315ea3206689c86fad9f45b60a44e656a",
                    "member_chain": ["IdeSupport/MDK/WHXY.CW32R031_DFP.1.0.2.pack", "SVD/CW32R031.svd"]
                },
                {
                    "source_ref": "member:cw32r031/IdeSupport/MDK/WHXY.CW32R031_DFP.1.0.2/WHXY.CW32R031_DFP.pdsc",
                    "sha256": "efdd38b9ecf017f74d8ae86682588d772503fe0fe07b3af56964133327c35c9f",
                    "member_chain": ["IdeSupport/MDK/WHXY.CW32R031_DFP.1.0.2.pack", "WHXY.CW32R031_DFP.pdsc"]
                },
                {
                    "source_ref": "member:cw32r031/Libraries/inc/cw32r031.h",
                    "sha256": "b01fb95cf3ab03855f83df0f5e9683878b7387066bc122d2743451c02397d385",
                    "member_chain": ["Libraries/inc/cw32r031.h"]
                },
                {
                    "source_ref": "member:cw32r031/Libraries/inc/cw32r031_rcc.h",
                    "sha256": "117d7e1c37dd6670dff3ec04f3bb619e6e6e9d252491b942316a815f8bd94137",
                    "member_chain": ["Libraries/inc/cw32r031_rcc.h"]
                },
                {
                    "source_ref": "member:cw32r031/Libraries/src/cw32r031_rcc.c",
                    "sha256": "31e4d708aa220e2c247c50b55b2166dd03e33c6c91b06246b548c2d3e8c7a4f2",
                    "member_chain": ["Libraries/src/cw32r031_rcc.c"]
                }
            ],
            "package_pins": {"PB11": "25", "PF0": "4", "PF1": "5", "PC14": "2", "PC15": "3"},
            "supply_contract": {
                "vdda_equals_vdd": true,
                "vddrf_and_grounds": "own_board_requirements",
                "rf_temperature_row": "does_not_redefine_mcu_lsi",
                "firmware_rf_power_probe": false
            },
            "rf_boundary": {
                "rfclk_source": "dedicated_external_16mhz_crystal",
                "rf_pll": "separate_from_mcu_sysclk",
                "direct_lsi_rfclk_root": false,
                "indirect_host_path": ["SYSCLK", "PCLK", "SYSCTRL.AHBEN.GPIOA", "PA00..PA03", "RF_SPI"],
                "host_pins": {"PA00": "MISO", "PA01": "MOSI", "PA02": "SCK", "PA03": "CS"},
                "xtal_oclk_divisors": [1, 2, 4, 8],
                "xtal_oclk_hse_bypass": "board_connection_owner_preserves_availability",
                "gpio_gate_handover": "finish_quiet_host_transfers_and_allow_whole_bank_activity",
                "waveform_or_packet_continuity_qualified": false,
                "rf_runtime_admission": false
            },
            "generator_svd_path": "cw32r031/IdeSupport/MDK/WHXY.CW32R031_DFP.1.0.2/SVD/CW32R031.svd",
            "generator_header_path": "cw32r031/Libraries/inc/cw32r031.h"
        }),
        "CW32W031" => json!({
            "sdk_source": {
                "source_ref": "vendor:CW32W031_StandardPeripheralLib_V1.3.zip",
                "sha256": "1010176816766d025c84babf84ffdf2ee03c5687d2a29133267d0013a1970337"
            },
            "sdk_members": [
                {
                    "source_ref": "member:cw32w031/IdeSupport/MDK/WHXY.CW32W031_DFP.1.0.2/SVD/CW32W031.svd",
                    "sha256": "7b3f7cd1e1e9b311f8bf6da711b9af0a144040883a231dfcc93b9cd2bafe84d0",
                    "member_chain": ["IdeSupport/MDK/WHXY.CW32W031_DFP.1.0.2.pack", "SVD/CW32W031.svd"]
                },
                {
                    "source_ref": "member:cw32w031/IdeSupport/MDK/WHXY.CW32W031_DFP.1.0.2/WHXY.CW32W031_DFP.pdsc",
                    "sha256": "932cf4107a4a543cb92de4a80bf3e286f49dec9f5ea36659b682eff26f7d5b36",
                    "member_chain": ["IdeSupport/MDK/WHXY.CW32W031_DFP.1.0.2.pack", "WHXY.CW32W031_DFP.pdsc"]
                },
                {
                    "source_ref": "member:cw32w031/Libraries/inc/cw32w031.h",
                    "sha256": "164a3a23edd474b51d9c23de4bc7d4560d281effd44d15e86e30b3170c5bff69",
                    "member_chain": ["Libraries/inc/cw32w031.h"]
                },
                {
                    "source_ref": "member:cw32w031/Libraries/inc/cw32w031_rcc.h",
                    "sha256": "29bbaacf4cad7cd57fd1165773a36b6afc9d4a6ec36a06eb9615f790370dcdd6",
                    "member_chain": ["Libraries/inc/cw32w031_rcc.h"]
                },
                {
                    "source_ref": "member:cw32w031/Libraries/src/cw32w031_rcc.c",
                    "sha256": "75f0f05280ee97c685ae97e5a89bf2a8fa03dfcc263c300ca6bed6c0ef62605b",
                    "member_chain": ["Libraries/src/cw32w031_rcc.c"]
                }
            ],
            "package_pins": {"PB11": "16", "PF0": "63", "PF1": "64", "PC14": "61", "PC15": "62"},
            "supply_contract": {
                "ldo_supply_mv": [1800, 3600],
                "dcdc_supply_mv": [2000, 3600],
                "qualified_intersection_mv": [2000, 3600],
                "vdda_equals_vdd": true,
                "vddrf_same_supply_as_vdd_when_rf_used": true,
                "vddrf_and_grounds": "own_board_requirements",
                "rf_temperature_row": "does_not_redefine_mcu_lsi",
                "firmware_rf_power_probe": false
            },
            "rf_boundary": {
                "rfclk_source": "dedicated_external_32mhz_crystal",
                "rf_pll": "separate_from_mcu_sysclk",
                "direct_lsi_rfclk_root": false,
                "indirect_host_path": [
                    "SYSCLK", "HCLK", "PCLK", "SYSCTRL.APBEN2.SPI", "internal_SPI1",
                    "RF_register_FIFO_interface"
                ],
                "host_gate": {"register": "APBEN2", "byte_offset": 52, "field": "SPI", "bit_offset": 8},
                "internal_host_pins": {"PB05": "MOSI", "PB04": "MISO", "PB03": "CS", "PB13": "SCK", "PB06": "RF_IRQ"},
                "internal_host_pin_source": {
                    "source_ref": "vendor:CW32W031_DataSheet_CN_V1.3.pdf",
                    "pdf_page_1_based": 14,
                    "printed_page": 13,
                    "table": "4-2"
                },
                "internal_pins_are_not_extra_external_tokens": true,
                "host_bit_rate_exclusive_maximum": 10000000,
                "external_spi_unavailable_when_rf_used": true,
                "gpio_gate_handover": "finish_quiet_host_transfers_and_allow_whole_bank_activity",
                "gpiob_inspection_can_resume_host_irq_events": true,
                "gate_restoration_does_not_undo_work": true,
                "waveform_or_packet_continuity_qualified": false,
                "rf_runtime_admission": false
            },
            "interrupt_contract": {
                "ready_irq": "SYSCTRL",
                "ready_irq_number": 4,
                "ready_vector_number": 20,
                "external_running_fault_irq": "CLKFAULT",
                "external_running_fault_irq_number": 31,
                "external_running_fault_vector_number": 47,
                "ready_irq_in_svd": true,
                "fault_irq_in_svd": false,
                "fault_irq_in_header_and_startup": true,
                "fault_irq_pending_observer": false
            },
            "generator_svd_path": "cw32w031/IdeSupport/MDK/WHXY.CW32W031_DFP.1.0.2/SVD/CW32W031.svd",
            "generator_header_path": "cw32w031/Libraries/inc/cw32w031.h"
        }),
        _ => anyhow::bail!("unreviewed native own-source binding"),
    };
    ensure!(
        own["peripheral_addresses"]
            == json!({
                "SYSCTRL": 0x4001_0000, "RTC": 0x4000_2800, "AWT": 0x4001_4c00,
                "UART1": 0x4001_3800, "UART2": 0x4000_4400, "UART3": 0x4000_4800,
                "GPIOA": 0x4800_0000, "GPIOB": 0x4800_0400,
                "GPIOC": 0x4800_0800, "GPIOF": 0x4800_1400
            }),
        "{line} own peripheral base addresses changed"
    );
    let fields = own["register_fields"]
        .as_array()
        .context("missing native field evidence")?;
    let expected_fields = [
        ("SYSCTRL", "CR0", "SYSCLK", 0, 0, 3),
        ("SYSCTRL", "CR0", "PCLKPRS", 0, 3, 2),
        ("SYSCTRL", "CR0", "HCLKPRS", 0, 5, 3),
        ("SYSCTRL", "CR0", "KEY", 0, 16, 16),
        ("SYSCTRL", "CR1", "HSIEN", 4, 0, 1),
        ("SYSCTRL", "CR1", "HSEEN", 4, 1, 1),
        ("SYSCTRL", "CR1", "LSIEN", 4, 3, 1),
        ("SYSCTRL", "CR1", "LSEEN", 4, 4, 1),
        ("SYSCTRL", "CR1", "LSELOCK", 4, 5, 1),
        ("SYSCTRL", "CR1", "LSECCS", 4, 6, 1),
        ("SYSCTRL", "CR1", "HSECCS", 4, 7, 1),
        ("SYSCTRL", "CR1", "CLKCCS", 4, 8, 1),
        ("SYSCTRL", "CR1", "KEY", 4, 16, 16),
        ("SYSCTRL", "IER", "LSIRDY", 12, 3, 1),
        ("SYSCTRL", "IER", "KEY", 12, 16, 16),
        ("SYSCTRL", "ISR", "LSIRDY", 16, 3, 1),
        ("SYSCTRL", "ISR", "HSISTABLE", 16, 11, 1),
        ("SYSCTRL", "ISR", "HSESTABLE", 16, 12, 1),
        ("SYSCTRL", "ISR", "LSISTABLE", 16, 14, 1),
        ("SYSCTRL", "ISR", "LSESTABLE", 16, 15, 1),
        ("SYSCTRL", "ISR", "HSEFAIL", 16, 6, 1),
        ("SYSCTRL", "ISR", "HSEFAULT", 16, 8, 1),
        ("SYSCTRL", "ISR", "LSEFAIL", 16, 5, 1),
        ("SYSCTRL", "ISR", "LSEFAULT", 16, 7, 1),
        ("SYSCTRL", "HSI", "TRIM", 24, 0, 11),
        ("SYSCTRL", "HSI", "DIV", 24, 11, 4),
        ("SYSCTRL", "HSI", "STABLE", 24, 15, 1),
        ("SYSCTRL", "HSE", "DRIVER", 28, 0, 2),
        ("SYSCTRL", "HSE", "FREQRANGE", 28, 2, 2),
        ("SYSCTRL", "HSE", "WAITCYCLE", 28, 4, 2),
        ("SYSCTRL", "HSE", "MODE", 28, 6, 1),
        ("SYSCTRL", "HSE", "FLT", 28, 7, 1),
        ("SYSCTRL", "HSE", "DETCNT", 28, 8, 11),
        ("SYSCTRL", "HSE", "STABLE", 28, 19, 1),
        ("SYSCTRL", "LSI", "TRIM", 32, 0, 10),
        ("SYSCTRL", "LSI", "WAITCYCLE", 32, 10, 2),
        ("SYSCTRL", "LSI", "STABLE", 32, 15, 1),
        ("SYSCTRL", "LSE", "DRIVER", 36, 0, 2),
        ("SYSCTRL", "LSE", "AMP", 36, 2, 2),
        ("SYSCTRL", "LSE", "WAITCYCLE", 36, 4, 2),
        ("SYSCTRL", "LSE", "MODE", 36, 6, 1),
        ("SYSCTRL", "LSE", "STABLE", 36, 15, 1),
        ("SYSCTRL", "MCO", "SOURCE", 112, 0, 4),
        ("RTC", "CR1", "SOURCE", 8, 8, 3),
        ("AWT", "CR", "SRC", 0, 8, 3),
        ("UART1", "CR2", "SOURCE", 4, 8, 2),
        ("UART2", "CR2", "SOURCE", 4, 8, 2),
        ("UART3", "CR2", "SOURCE", 4, 8, 2),
        ("GPIOA", "FILTER", "FLTCLK", 64, 16, 3),
        ("GPIOB", "FILTER", "FLTCLK", 64, 16, 3),
        ("GPIOC", "FILTER", "FLTCLK", 64, 16, 3),
        ("GPIOF", "FILTER", "FLTCLK", 64, 16, 3),
        ("GPIOB", "AFRH", "AFR11", 20, 12, 4),
    ];
    ensure!(
        fields.len() == expected_fields.len(),
        "{line} own field count changed"
    );
    for (fact, (peripheral, register, field, offset, bit, width)) in
        fields.iter().zip(expected_fields)
    {
        let register_access = if register == "ISR" {
            "read-only"
        } else {
            "read-write"
        };
        let field_access = if field == "KEY" {
            "write-only"
        } else if register == "ISR" || field == "STABLE" {
            "read-only"
        } else {
            "read-write"
        };
        ensure!(
            fact["peripheral"] == peripheral
                && fact["register"] == register
                && fact["field"] == field
                && fact["byte_offset"] == offset
                && fact["bit_offset"] == bit
                && fact["bit_size"] == width
                && fact["register_width_bits"] == 32
                && fact["register_access"] == register_access
                && fact["field_access"] == field_access,
            "{line} own field/access evidence changed: {peripheral}.{register}.{field}"
        );
    }
    let gates = own["consumer_gates"]
        .as_array()
        .context("missing native gate evidence")?;
    let expected_gates = [
        ("RTC", "APBEN1", 56, "APBRST1", 72, 3),
        ("AWT", "APBEN2", 52, "APBRST2", 68, 13),
        ("UART1", "APBEN2", 52, "APBRST2", 68, 9),
        ("UART2", "APBEN1", 56, "APBRST1", 72, 7),
        ("UART3", "APBEN1", 56, "APBRST1", 72, 8),
        ("GPIOA", "AHBEN", 48, "AHBRST", 64, 4),
        ("GPIOB", "AHBEN", 48, "AHBRST", 64, 5),
        ("GPIOC", "AHBEN", 48, "AHBRST", 64, 6),
        ("GPIOF", "AHBEN", 48, "AHBRST", 64, 9),
    ];
    ensure!(
        gates.len() == expected_gates.len(),
        "{line} own gate count changed"
    );
    for (fact, (peripheral, register, offset, reset, reset_offset, bit)) in
        gates.iter().zip(expected_gates)
    {
        ensure!(
            fact["peripheral"] == peripheral
                && fact["register"] == register
                && fact["byte_offset"] == offset
                && fact["bit_offset"] == bit
                && fact["reset_register"] == reset
                && fact["reset_byte_offset"] == reset_offset
                && fact["register_access"] == "read-write"
                && fact["reset_register_access"] == "read-write"
                && fact["gate_controls_work"] == peripheral.starts_with("GPIO")
                && fact["enable_active_value"] == true
                && fact["reset_asserted_value"] == false
                && fact["keyed"] == false,
            "{line} own gate/reset evidence changed: {peripheral}"
        );
    }
    ensure!(
        own["sdk_source"] == binding["sdk_source"] && own["sdk_members"] == binding["sdk_members"],
        "{line} selected SDK/member identities changed"
    );
    let selected = lock["artifacts"]
        .as_array()
        .context("missing source authority")?
        .iter()
        .filter(|a| a["id"] == own["sdk_source"]["source_ref"])
        .collect::<Vec<_>>();
    ensure!(selected.len() == 1, "{line} SDK source must be unique");
    let sdk = selected[0];
    ensure!(
        sdk["sha256"] == own["sdk_source"]["sha256"]
            && sdk["provenance"]["status"] == "selected"
            && sdk["provenance"]["chip_scope"] == json!([line])
            && sdk["provenance"]["generator_svd_path"] == binding["generator_svd_path"]
            && sdk["provenance"]["generator_header_path"] == binding["generator_header_path"],
        "{line} SDK must remain selected own-family register/IRQ evidence"
    );
    let members = sdk["members"]
        .as_array()
        .context("missing native SDK members")?;
    for fact in own["sdk_members"].as_array().unwrap() {
        let path = fact["source_ref"]
            .as_str()
            .and_then(|s| s.strip_prefix("member:"))
            .context("invalid native member source")?;
        let selected = members
            .iter()
            .filter(|m| m["path"] == path)
            .collect::<Vec<_>>();
        ensure!(
            selected.len() == 1
                && selected[0]["sha256"] == fact["sha256"]
                && selected[0]["members"] == fact["member_chain"],
            "{line} selected SDK member/provenance changed: {path}"
        );
    }
    ensure!(
        own["pb11_output_af"] == 1
            && own["package_pins"] == binding["package_pins"]
            && own["supply_contract"] == binding["supply_contract"]
            && own["rf_boundary"] == binding["rf_boundary"],
        "{line} own supply, output or RF functional boundary changed"
    );
    if line == "CW32W031" {
        ensure!(
            own["interrupt_contract"] == binding["interrupt_contract"],
            "W031 SYSCTRL4 readiness and CLKFAULT31 running-fault identities differ"
        );
    }
    Ok(())
}

// Both exact-only families use this one structural admission process. The local
// match binds differences to separately reviewed own-family sources, never to
// the mere presence of a shared register backend or to an absent RTC/output.
fn validate_exact_family(
    root: &Path,
    chip: &crate::ChipInput,
    line: &str,
    own: &Value,
    lsi: &cw32_data_serde::chip::core::peripheral::LsiSysclk,
) -> Result<bool> {
    use serde_json::json;
    let binding = match line {
        "CW32F002" => json!({
            "parts": [
                {"name":"CW32F002F3P7", "package":"TSSOP20"},
                {"name":"CW32F002F3U7", "package":"QFN20"}
            ],
            "sysctrl_version": "cw32f002_v1",
            "minimum_hz": 31_160, "maximum_hz": 34_440,
            "flash_bytes": 16_384, "ram_bytes": 2_048,
            "rm_sha256": "e453b3f82d9d180a86cd5f7cb18d858d7f228afc8101d87c700ca9d5c02d5add",
            "ds_sha256": "6d0c5c37d069e5b4e33d394b0be6c53938868e9375e7b4bbbbdd40d62bb9d506",
            "rm_pages": [
                37, 39, 40, 41, 44, 45, 46, 47, 48, 49, 50, 51, 54, 55, 57, 58, 59, 60, 61, 62,
                63, 65, 70, 72, 73, 74, 75, 76, 101, 105, 106, 113, 123, 125, 129, 132, 136,
                137, 152, 168, 178, 185, 186, 188, 194, 195, 203, 227, 228, 236, 268, 303, 340,
                346, 351, 356
            ],
            "ds_pages": [8, 22, 23, 24, 31, 37],
            "other_clock_sources": {
                "IWDT":["RC10K"], "VC":["PCLK","RC150K"], "LVD":["HSIOSC","RC150K"]
            },
            "no_lsi_output_evidence": {
                "source_ref":"vendor:CW32F002_UserManual_CN_V1.4.pdf",
                "pdf_pages_1_based":[105], "printed_pages":[104], "complete_af_catalog":true,
                "sdk_catalog":"cw32-data/af/cw32f002.yaml",
                "sdk_catalog_sha256":"ad2faf10e3271b0f84ef045fe69cacc8dc170538240f9df1d1cc168270080e1e",
                "sdk_header_source_ref":"member:cw32f002/Libraries/inc/cw32f002_gpio.h"
            }
        }),
        "CW32F003" => json!({
            "parts": [
                {"name":"CW32F003F4P7", "package":"TSSOP20"},
                {"name":"CW32F003F4U7", "package":"QFN20"},
                {"name":"CW32F003E4P7", "package":"TSSOP24"}
            ],
            "sysctrl_version": "cw32f003_v1",
            "minimum_hz": 31_816, "maximum_hz": 33_784,
            "flash_bytes": 20_480, "ram_bytes": 3_072,
            "rm_sha256": "0fa58dac223add7f2ac1ee714a7df7db4e414e0f193601b80dda96a948bfc738",
            "ds_sha256": "5fe15321b3963472c2629030767cf61a0ce36063815b54add74025b5b13b95dd",
            "rm_pages": [
                42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 55, 56, 57, 58, 59, 60, 61, 62, 63,
                64, 65, 66, 67, 70, 71, 72, 74, 75, 90, 91, 92, 93, 94, 95, 96, 97, 98, 99,
                100, 101, 107, 108, 110, 115, 131, 188, 189, 221, 222, 245, 246, 248, 287,
                355, 356, 360, 402, 410, 415, 420
            ],
            "ds_pages": [5, 8, 23, 24, 25, 26, 27, 32, 37, 38, 40, 62],
            "other_clock_sources": {
                "IWDT":["RC10K"], "VC":["PCLK","RC150K"], "LVD":["HSIOSC","RC150K"],
                "ATIM":["PCLK","ETR"], "IR":["GTIM/BTIM/UART output or software data"]
            },
            "no_lsi_output_evidence": {
                "source_ref":"vendor:CW32F003_UserManual_CN_V2.3.pdf",
                "pdf_pages_1_based":[107], "printed_pages":[106], "complete_af_catalog":true,
                "sdk_catalog":"cw32-data/af/cw32f003.yaml",
                "sdk_catalog_sha256":"335e35021f2ee0dcd25e3b80ce232577e13ec2fe2ed69e13c108d3ae517a0cf9",
                "sdk_header_source_ref":"member:cw32f003/Libraries/inc/cw32f003_gpio.h"
            }
        }),
        _ => anyhow::bail!("unreviewed exact-only factory-LSI family"),
    };
    let parts = binding["parts"].as_array().unwrap();
    ensure!(
        own["exact_parts"] == binding["parts"]
            && own["sysctrl_version"] == binding["sysctrl_version"],
        "exact factory-LSI qualification roster or IP changed"
    );
    let catalog = parse_policy(&fs::read(root.join("cw32-data/parts.yaml"))?)?;
    let roster = catalog["parts"]
        .as_array()
        .context("missing exact parts catalog")?;
    let datasheet_id = format!("{line}_datasheet");
    let memory = json!([
        {"name":"FLASH", "kind":"flash", "address":0, "size":binding["flash_bytes"]},
        {"name":"RAM", "kind":"ram", "address":0x2000_0000, "size":binding["ram_bytes"]}
    ]);
    for part in parts {
        let selected = roster
            .iter()
            .filter(|p| p["name"] == part["name"])
            .collect::<Vec<_>>();
        ensure!(
            selected.len() == 1
                && selected[0]["family"] == line
                && selected[0]["feature"] == part["name"].as_str().unwrap().to_ascii_lowercase()
                && selected[0]["package"] == part["package"]
                && selected[0]["memory"] == memory
                && selected[0]["datasheet_source_id"] == datasheet_id
                && catalog["sources"][&datasheet_id]["source_ref"]
                    == own["sources"][1]["source_ref"]
                && catalog["sources"][&datasheet_id]["sha256"] == binding["ds_sha256"],
            "exact factory-LSI qualification differs from own datasheet/catalog"
        );
    }
    let selected = parts.iter().find(|p| p["name"] == chip.name);
    if let Some(part) = selected {
        ensure!(
            chip.packages.len() == 1
                && part["package"] == chip.packages[0].package
                && serde_json::to_value(&chip.memory)? == json!([memory]),
            "exact factory-LSI qualification requires the reviewed physical package and memory"
        );
    }
    ensure!(
        lsi.nominal_hz == 32_800
            && binding["minimum_hz"] == lsi.minimum_hz
            && binding["maximum_hz"] == lsi.maximum_hz
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
        "exact factory-LSI source envelope or complete selector facts changed"
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
            && own["other_clock_sources"] == binding["other_clock_sources"]
            && own["ready_observers"] == json!(["IER.LSIRDY", "ISR.LSIRDY", "NVIC.RCC.pending"])
            && own["stable_observers"] == json!(["LSI.STABLE", "ISR.LSISTABLE"])
            && own["no_lsi_output_evidence"] == binding["no_lsi_output_evidence"],
        "exact factory-LSI complete observer/negative/direct-output evidence changed"
    );
    ensure!(
        own["sources"][0]["sha256"] == binding["rm_sha256"]
            && own["sources"][1]["sha256"] == binding["ds_sha256"]
            && own["sources"][0]["pdf_pages_1_based"] == binding["rm_pages"]
            && own["sources"][1]["pdf_pages_1_based"] == binding["ds_pages"],
        "exact factory-LSI selected source identity or evidence pages changed"
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
