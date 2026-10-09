use std::{env, fs, path::PathBuf};

fn main() {
    let metadata = &cw32_metapac::metadata::METADATA;
    configure_ram();
    let time_driver = select_time_driver();
    let x030 = matches!(metadata.line, "CW32F030" | "CW32A030");
    // Own-family-qualified DMA controller; peripheral requests are bounded below.
    let dma = x030 || metadata.line == "CW32L083";
    let f020 = metadata.line == "CW32F020";
    let classic_adc = matches!(
        metadata.line,
        "CW32F030"
            | "CW32A030"
            | "CW32F020"
            | "CW32F002"
            | "CW32F003"
            | "CW32L031"
            | "CW32R031"
            | "CW32W031"
            | "CW32L052"
            | "CW32L083"
    );
    let sequence_adc = matches!(metadata.line, "CW32L010" | "CW32L011");
    let adc = classic_adc || sequence_adc;
    let dual_adc = metadata.line == "CW32L012";
    let uart = true;
    let spi = true;
    let i2c = true;
    let serial = uart || spi || i2c;
    let cfgs = [
        "rcc_hse",
        "rcc_pll",
        "rcc_external_clock",
        "aes_cw32l083_v1",
        "trng_cw32l083_v1",
        "cordic_cw32l012_v1",
        "eau_cw32l012_v1",
        "adc",
        "vc",
        "dac_cw32l012_v1",
        "opa_cw32l012_v1",
        "vc_two_speed",
        "vc_ready",
        "vc_cr2",
        "vref",
        "vref_vcdiv",
        "vref_vc2ref",
        "lvd_low",
        "ir_sysctrl",
        "ir_modes",
        "ir_software",
        "ir_invert",
        "rtc",
        "rtc_alarm_direct_access",
        "awt",
        "autotrim",
        "lptim",
        "lcd",
        "lptim_cr0",
        "halltim_cw32l012_v1",
        "flash",
        "flash_v1",
        "flash_cw32f002_v1",
        "flash_cw32f003_v1",
        "flash_cw32f020_v1",
        "flash_cw32l031_v1",
        "flash_cw32l010_v1",
        "flash_cw32l011_v1",
        "flash_cw32l012_v1",
        "flash_cw32l083_v1",
        "gpio_v1",
        "gpio_cw32f002_v1",
        "gpio_cw32l031_v1",
        "gpio_cw32l010_v1",
        "gpio_cw32l011_v1",
        "gpio_cw32l012_v1",
        "rcc_v1",
        "rcc_cw32f020_v1",
        "rcc_cw32f002_v1",
        "rcc_cw32f003_v1",
        "rcc_cw32l031_v1",
        "rcc_cw32l010_v1",
        "rcc_cw32l011_v1",
        "rcc_cw32l012_v1",
        "gpio_cw32l052_v1",
        "gpio_cw32l083_v1",
        "rcc_cw32l052_v1",
        "rcc_cw32l083_v1",
        "cw32l052",
        "cw32l083",
        "cw32x030",
        "gpio_af",
        "gpio_exti",
        "gpio_irq_level",
        "crc_poly_8005",
        "crc_input_16bit",
        "crc_input_32bit",
        "wwdt",
        "crc",
        "crc_32bit",
        "uart",
        "uart_v1",
        "uart_cw32f002_v1",
        "uart_cw32l010_v1",
        "uart_cw32l012_v1",
        "uart_cw32l052_v1",
        "uart_cw32l083_v1",
        "uart_cw32l031_v1",
        "spi",
        "spi_v1",
        "spi_cw32f002_v1",
        "spi_cw32l010_v1",
        "spi_cw32l012_v1",
        "spi_cw32l031_v1",
        "i2c_cw32l031_v1",
        "i2c_cw32l012_v1",
        "i2c",
        "i2c_v1",
        "i2c_cw32f002_v1",
        "i2c_cw32l010_v1",
        "i2c_cw32l011_v1",
        "dma_v1",
        "uart_dma",
        "spi_dma",
        "adc_cw32l010_v1",
        "adc_cw32l011_v1",
        "adc_cw32l012_v1",
        "adc_v1",
        "adc_cw32f020_v1",
        "adc_cw32f002_v1",
        "adc_cw32f003_v1",
        "adc_cw32l031_v1",
        "adc_cw32l052_v1",
        "adc_cw32l083_v1",
        "atim",
        "atim_classic",
        "atim_buffered",
        "atim_classic_complementary",
        "atim_v1",
        "atim_cw32f003_v1",
        "atim_cw32l052_v1",
        "atim_cw32l010_v1",
        "atim_cw32l012_v1",
        "gtim_classic",
        "gtim_buffered",
        "gtim_cw32l010_v1",
        "gtim_cw32l012_v1",
        "gtim_v1",
        "gtim_cw32f002_v1",
        "gtim_cw32l031_v1",
        "gtim_cw32l052_v1",
        "rtc_v1",
        "rtc_cw32f020_v1",
        "rtc_cw32l010_v1",
        "rtc_cw32l031_v1",
        "rtc_cw32l052_v1",
        "rtc_cw32l011_v1",
        "rtc_cw32l012_v1",
        "trigger_btim1_update",
        "btim",
        "btim_v1",
        "btim_cw32f002_v1",
        "btim_cw32l031_v1",
        "btim_cw32l052_v1",
        "btim_cw32l010_v1",
        "btim_cw32l012_v1",
        "iwdt",
        "cw32f002",
        "cw32f003",
        "cw32f020",
        "cw32l010",
        "cw32l011",
        "cw32l012",
        "cw32l031",
        "cw32r031",
        "cw32w031",
    ];
    for cfg in cfgs {
        println!("cargo:rustc-check-cfg=cfg({cfg})");
    }
    generate_trigger_routes();
    // Own-manual-qualified timer IP; version selection is checked before use.
    for (name, versions) in [
        ("AWT", &["v1", "cw32f030_v1", "cw32l031_v1"][..]),
        ("AUTOTRIM", &["cw32l052_v1", "cw32l083_v1"][..]),
        ("LPTIM", &["cw32l010_v1", "cw32l012_v1", "cw32l083_v1"][..]),
    ] {
        if let Some(p) = metadata.peripherals.iter().find(|p| p.name == name) {
            let registers = p.registers.as_ref().unwrap();
            assert!(
                versions.contains(&registers.version),
                "unreviewed low-power timer IP"
            );
            println!("cargo:rustc-cfg={}", name.to_ascii_lowercase());
            if registers.version == "cw32l012_v1" {
                println!("cargo:rustc-cfg=lptim_cr0");
            }
        }
    }
    if let Some(p) = metadata.peripherals.iter().find(|p| p.name == "HALLTIM") {
        let r = p.registers.as_ref().unwrap();
        assert_eq!(metadata.line, "CW32L012");
        assert_eq!((r.kind, r.version), ("halltim", "cw32l012_v1"));
        assert!(p.rcc.is_some() && p.rcc_control.is_some() && !p.pins.is_empty());
        println!("cargo:rustc-cfg=halltim_cw32l012_v1");
    }
    if let Some(p) = metadata.peripherals.iter().find(|p| p.name == "LCD") {
        assert!(p.lcd.is_some(), "LCD needs source-qualified chip facts");
        println!("cargo:rustc-cfg=lcd");
    }
    // Capabilities come from the selected, reviewed register IR, not family aliases.
    let comparators: Vec<_> = metadata
        .peripherals
        .iter()
        .filter(|p| p.registers.as_ref().is_some_and(|r| r.kind == "vc"))
        .collect();
    if let Some(first) = comparators.first() {
        println!("cargo:rustc-cfg=vc");
        let regs = first.registers.as_ref().unwrap();
        let control = regs
            .ir
            .fieldsets
            .iter()
            .find(|f| f.name.eq_ignore_ascii_case("CR0"))
            .unwrap();
        let response = control
            .fields
            .iter()
            .find(|f| f.name.eq_ignore_ascii_case("RESP"))
            .unwrap();
        assert_eq!(response.enumm, Some("ResponseSpeed"));
        assert!([1, 2].contains(&response.bit_size));
        if response.bit_size == 1 {
            println!("cargo:rustc-cfg=vc_two_speed");
        }
        let status = regs
            .ir
            .fieldsets
            .iter()
            .find(|f| f.name.eq_ignore_ascii_case("SR"))
            .unwrap();
        if status
            .fields
            .iter()
            .any(|f| f.name.eq_ignore_ascii_case("READY"))
        {
            println!("cargo:rustc-cfg=vc_ready");
        }
        if regs
            .ir
            .blocks
            .iter()
            .find(|b| b.name == regs.block)
            .unwrap()
            .items
            .iter()
            .any(|r| r.name.eq_ignore_ascii_case("CR2"))
        {
            println!("cargo:rustc-cfg=vc_cr2");
        }
        for p in &comparators {
            assert_eq!(p.registers.as_ref().unwrap().version, regs.version);
            assert!(
                p.rcc_control.is_some(),
                "comparator clock control is unqualified"
            );
            assert_eq!(p.comparator_limits, first.comparator_limits);
        }
    }
    let reference_dividers: Vec<_> = metadata
        .peripherals
        .iter()
        .filter(|p| p.reference_divider.is_some())
        .collect();
    for peripheral in &reference_dividers {
        let facts = peripheral.reference_divider.as_ref().unwrap();
        let registers = peripheral.registers.as_ref().unwrap();
        assert!(matches!(registers.kind, "vcdiv" | "vc2ref"));
        assert_eq!(facts.consumers.len(), 2);
        assert!(facts.consumers.contains(&facts.clock_owner));
        let fieldset = if registers.kind == "vcdiv" {
            "DIV"
        } else {
            "REF"
        };
        let control = registers
            .ir
            .fieldsets
            .iter()
            .find(|f| f.name.eq_ignore_ascii_case(fieldset))
            .unwrap();
        let divider = control
            .fields
            .iter()
            .find(|f| f.name.eq_ignore_ascii_case("DIV"))
            .unwrap();
        let input = control
            .fields
            .iter()
            .find(|f| f.name.eq_ignore_ascii_case("VIN"))
            .unwrap();
        assert_eq!(divider.enumm, Some("DividerRatio"));
        assert_eq!(
            divider.bit_size,
            if registers.kind == "vcdiv" { 3 } else { 4 }
        );
        assert_eq!(input.enumm, Some("InputSource"));
        let clock = metadata
            .peripherals
            .iter()
            .find(|p| p.name == facts.clock_owner)
            .unwrap();
        assert!(clock.rcc_control.is_some());
        for name in facts.consumers {
            let consumer = comparators.iter().find(|p| p.name == *name).unwrap();
            assert_eq!(consumer.rcc_control, clock.rcc_control);
        }
        println!("cargo:rustc-cfg=vref");
        println!("cargo:rustc-cfg=vref_{}", registers.kind);
    }
    for kind in ["dac", "opa"] {
        for p in metadata
            .peripherals
            .iter()
            .filter(|p| p.registers.as_ref().is_some_and(|r| r.kind == kind))
        {
            assert_eq!(p.registers.as_ref().unwrap().version, "cw32l012_v1");
            assert!(p.rcc_control.is_some());
            println!("cargo:rustc-cfg={kind}_cw32l012_v1");
        }
    }
    let assert_version = |name: &str, expected: &str| {
        let registers = metadata
            .peripherals
            .iter()
            .find(|p| p.name == name)
            .expect("verified backend peripheral is missing")
            .registers
            .as_ref()
            .expect("verified backend registers are missing");
        assert_eq!(
            registers.version, expected,
            "{name}: unreviewed register version for {}",
            metadata.line
        );
    };
    if let Some(peripheral) = metadata.peripherals.iter().find(|p| p.name == "RTC") {
        let regs = peripheral.registers.as_ref().unwrap();
        assert!(matches!(
            regs.version,
            "v1" | "cw32f020_v1"
                | "cw32l010_v1"
                | "cw32l011_v1"
                | "cw32l012_v1"
                | "cw32l031_v1"
                | "cw32l052_v1"
        ));
        assert!(peripheral.rtc_calendar.is_some());
        println!("cargo:rustc-cfg=rtc");
        println!("cargo:rustc-cfg=rtc_{}", regs.version);
        let alarms = peripheral
            .rtc_alarms
            .as_ref()
            .expect("RTC alarm source facts");
        assert_eq!(alarms.alarm_a_ignore_bit, 1);
        assert!(!alarms.alarm_b_configuration_supported);
        if alarms.async_wait {
            assert!(alarms.direct_register_access);
            assert_eq!(peripheral.interrupts.len(), 1);
            assert_eq!(peripheral.interrupts[0].interrupt, "RTC");
            assert_eq!(peripheral.interrupts[0].signal, "GLOBAL");
            assert!(
                !metadata
                    .peripherals
                    .iter()
                    .filter(|p| p.name != "RTC")
                    .any(|p| p.interrupts.iter().any(|i| i.interrupt == "RTC")),
                "RTC vector must be dedicated"
            );
            println!("cargo:rustc-cfg=rtc_alarm_direct_access");
        } else {
            assert!(!alarms.direct_register_access);
        }
    }
    if metadata.line == "CW32L012" {
        assert_version("CORDIC", "cw32l012_v1");
        assert_version("EAU", "cw32l012_v1");
        println!("cargo:rustc-cfg=cordic_cw32l012_v1");
        println!("cargo:rustc-cfg=eau_cw32l012_v1");
        generate_accelerator_domains();
    }
    if metadata.line == "CW32L083" {
        assert_version("AES", "cw32l083_v1");
        assert_version("TRNG", "cw32l083_v1");
        generate_crypto_facts();
        println!("cargo:rustc-cfg=aes_cw32l083_v1");
        println!("cargo:rustc-cfg=trng_cw32l083_v1");
    }
    // Each mapping is independently qualified against that family's BTIM chapter.
    let btim_version = match metadata.line {
        "CW32F030" | "CW32A030" | "CW32F020" => "v1",
        "CW32F002" | "CW32F003" => "cw32f002_v1",
        "CW32L031" | "CW32R031" | "CW32W031" | "CW32L083" => "cw32l031_v1",
        "CW32L052" => "cw32l052_v1",
        "CW32L010" | "CW32L011" => "cw32l010_v1",
        "CW32L012" => "cw32l012_v1",
        _ => panic!("unverified basic-timer family"),
    };
    for name in ["BTIM1", "BTIM2", "BTIM3"] {
        assert_version(name, btim_version);
    }
    println!("cargo:rustc-cfg=btim");
    println!("cargo:rustc-cfg=btim_{btim_version}");
    let atim_version = match metadata.line {
        "CW32F030" | "CW32A030" | "CW32L031" | "CW32R031" | "CW32W031" | "CW32L083" => Some("v1"),
        "CW32F003" => Some("cw32f003_v1"),
        "CW32L052" => Some("cw32l052_v1"),
        "CW32L010" | "CW32L011" => Some("cw32l010_v1"),
        "CW32L012" => Some("cw32l012_v1"),
        _ => None,
    };
    if let Some(version) = atim_version {
        assert_version("ATIM", version);
        println!("cargo:rustc-cfg=atim");
        println!("cargo:rustc-cfg=atim_{version}");
        println!(
            "cargo:rustc-cfg={}",
            if matches!(version, "cw32l010_v1" | "cw32l012_v1") {
                "atim_buffered"
            } else {
                "atim_classic"
            }
        );
    }
    // GTIM versions are qualified independently of mere peripheral presence.
    let gtim_version = match metadata.line {
        "CW32F030" | "CW32A030" | "CW32F020" => Some("v1"),
        "CW32F002" | "CW32F003" => Some("cw32f002_v1"),
        "CW32L031" | "CW32R031" | "CW32W031" | "CW32L083" => Some("cw32l031_v1"),
        "CW32L052" => Some("cw32l052_v1"),
        "CW32L010" | "CW32L011" => Some("cw32l010_v1"),
        "CW32L012" => Some("cw32l012_v1"),
        _ => None,
    };
    if let Some(version) = gtim_version {
        for peripheral in metadata
            .peripherals
            .iter()
            .filter(|p| p.name.starts_with("GTIM"))
        {
            assert_version(peripheral.name, version);
        }
        if matches!(version, "cw32l010_v1" | "cw32l012_v1") {
            println!("cargo:rustc-cfg=gtim_buffered");
        } else {
            println!("cargo:rustc-cfg=gtim_classic");
        }
        println!("cargo:rustc-cfg=gtim_{version}");
    }
    let flash_version = match metadata.line {
        "CW32F030" | "CW32A030" => Some("v1"),
        "CW32F002" => Some("cw32f002_v1"),
        "CW32F003" => Some("cw32f003_v1"),
        "CW32F020" => Some("cw32f020_v1"),
        "CW32L031" | "CW32R031" | "CW32W031" | "CW32L052" => Some("cw32l031_v1"),
        "CW32L010" => Some("cw32l010_v1"),
        "CW32L011" => Some("cw32l011_v1"),
        "CW32L012" => Some("cw32l012_v1"),
        "CW32L083" => Some("cw32l083_v1"),
        _ => None,
    };
    let flash_storage = flash_version.is_some();
    let mut flash_storage_size = None;
    if let Some(version) = flash_version {
        assert_version("FLASH", version);
        println!("cargo:rustc-cfg=flash");
        println!("cargo:rustc-cfg=flash_{version}");
        // Only reviewed exact ordering codes qualify for this storage API.
        // Generic profiles lack memory metadata. Package-neutral aliases keep
        // their verified memory metadata but are not qualified for this API.
        let expected_size = match metadata.name {
            "CW32F002F3P7" | "CW32F002F3U7" => Some(16384),
            "CW32F003E4P7" | "CW32F003F4P7" | "CW32F003F4U7" => Some(20480),
            "CW32F020C6U7" | "CW32F020F6U7" | "CW32F020K6U7" | "CW32F030F6P7" => Some(32768),
            "CW32A030C8T7" | "CW32F030C8T7" | "CW32F030F8V7" | "CW32F030K8T7" | "CW32F030K8U7"
            | "CW32L031C8T6" | "CW32L031C8U6" | "CW32L031F8P6" | "CW32L031F8U6"
            | "CW32L031K8U6" | "CW32L031K8V6" | "CW32L052C8T6" | "CW32L052R8S6"
            | "CW32L052R8T6" | "CW32R031C8U6" | "CW32W031R8U6" | "CW32L010F8P6"
            | "CW32L010F8U6" | "CW32L010Y8M6" | "CW32L011K8T6" | "CW32L011K8U6"
            | "CW32L012C8T6" | "CW32L012C8U6" => Some(65536),
            "CW32L083RBT6" => Some(131072),
            "CW32L083MCT6" | "CW32L083RCS6" | "CW32L083RCT6" | "CW32L083VCT6" => Some(262144),
            _ => None,
        };
        if let Some(expected_size) = expected_size {
            assert_eq!(metadata.memory.len(), 1);
            let flash = metadata.memory[0]
                .iter()
                .find(|m| m.name == "FLASH")
                .unwrap();
            assert_eq!(flash.kind, cw32_metapac::metadata::MemoryRegionKind::Flash);
            assert_eq!((flash.address, flash.size), (0, expected_size));
            flash_storage_size = Some(flash.size);
        }
    }
    if dma {
        assert_version("DMA", "v1");
        assert_eq!(metadata.dma_channels.len(), 5);
        for (index, channel) in metadata.dma_channels.iter().enumerate() {
            assert_eq!(channel.dma, "DMA");
            assert_eq!(channel.channel as usize, index);
            assert_version(&format!("DMACHANNEL{}", index + 1), "v1");
        }
        println!("cargo:rustc-cfg=dma_v1");
    }
    // Qualification is distinct from a shared register layout. Only these
    // reviewed profiles may expose staged UART/SPI DMA; other DMA-v1 users
    // cannot acquire the capability from their IP version alone. See
    // docs/l083-peripheral-dma.md for the own-family source contract.
    let staged_profile = match metadata.line {
        "CW32F030" | "CW32A030" => Some((3, "v1", "v1")),
        "CW32L083" => Some((6, "cw32l083_v1", "cw32l031_v1")),
        _ => None,
    };
    let qualify_requests = |prefix: &str, count: u8, version: &str| {
        assert!(dma, "staged requests require a qualified DMA controller");
        let mut names = Vec::new();
        for index in 1..=count {
            let name = format!("{prefix}{index}");
            assert_version(&name, version);
            let peripheral = metadata
                .peripherals
                .iter()
                .find(|p| p.name == name)
                .unwrap();
            assert_eq!(peripheral.dma_channels.len(), 2);
            for signal in ["RX", "TX"] {
                let routes: Vec<_> = peripheral
                    .dma_channels
                    .iter()
                    .filter(|r| r.signal == signal)
                    .collect();
                assert_eq!(
                    routes.len(),
                    1,
                    "staged direction requires one qualified route"
                );
                let route = routes[0];
                // The reviewed request table applies to every physical channel.
                // No channel-limited/muxed/remapped record may become a blanket impl.
                assert_eq!(route.dma, Some("DMA"));
                assert!(
                    route.channel.is_none() && route.dmamux.is_none() && route.remap.is_empty()
                );
                assert!(route.request.is_some_and(|request| request < 64));
            }
            assert_ne!(
                peripheral.dma_channels[0].request,
                peripheral.dma_channels[1].request
            );
            names.push(name);
        }
        names
    };
    let (uart_dma_peripherals, spi_dma_peripherals) =
        if let Some((uarts, uart_ip, spi_ip)) = staged_profile {
            (
                qualify_requests("UART", uarts, uart_ip),
                qualify_requests("SPI", 2, spi_ip),
            )
        } else {
            (Vec::new(), Vec::new())
        };
    if !uart_dma_peripherals.is_empty() {
        println!("cargo:rustc-cfg=uart_dma");
    }
    if !spi_dma_peripherals.is_empty() {
        println!("cargo:rustc-cfg=spi_dma");
    }
    let selected: &[&str] = match metadata.line {
        "CW32F030" | "CW32A030" => {
            assert_version("GPIOA", "v1");
            assert_version("SYSCTRL", "v1");
            assert_version("ADC", "v1");
            for name in ["GTIM1", "GTIM2", "GTIM3", "GTIM4"] {
                assert_version(name, "v1");
            }
            &[
                "gpio_v1",
                "rcc_v1",
                "cw32x030",
                "gpio_exti",
                "crc_32bit",
                "adc_v1",
                "gtim_v1",
            ]
        }
        "CW32F020" => {
            assert_version("FLASH", "cw32f020_v1");
            assert_version("GPIOA", "v1");
            assert_version("SYSCTRL", "cw32f020_v1");
            assert_version("CRC", "cw32f020_v1");
            assert_version("ADC", "cw32f020_v1");
            for name in ["GTIM1", "GTIM2", "GTIM3", "GTIM4"] {
                assert_version(name, "v1");
            }
            &[
                "gpio_v1",
                "rcc_cw32f020_v1",
                "cw32f020",
                "adc_cw32f020_v1",
                "gtim_v1",
            ]
        }
        "CW32F002" => {
            assert_version("SYSCTRL", "cw32f002_v1");
            assert_version("FLASH", "cw32f002_v1");
            assert_version("GPIOA", "cw32f002_v1");
            assert_version("ADC", "cw32f002_v1");
            &[
                "gpio_cw32f002_v1",
                "rcc_cw32f002_v1",
                "cw32f002",
                "adc_cw32f002_v1",
            ]
        }
        "CW32F003" => {
            assert_version("SYSCTRL", "cw32f003_v1");
            assert_version("FLASH", "cw32f003_v1");
            assert_version("GPIOA", "cw32f002_v1");
            assert_version("ADC", "cw32f003_v1");
            &[
                "gpio_cw32f002_v1",
                "rcc_cw32f003_v1",
                "cw32f003",
                "adc_cw32f003_v1",
            ]
        }
        "CW32L010" => {
            assert_version("SYSCTRL", "cw32l010_v1");
            assert_version("FLASH", "cw32l010_v1");
            assert_version("GPIOA", "cw32l010_v1");
            {
                assert_version("ADC", "cw32l010_v1");
                &[
                    "gpio_cw32l010_v1",
                    "rcc_cw32l010_v1",
                    "cw32l010",
                    "adc_cw32l010_v1",
                ]
            }
        }
        "CW32L011" => {
            assert_version("SYSCTRL", "cw32l011_v1");
            assert_version("FLASH", "cw32l011_v1");
            assert_version("GPIOA", "cw32l011_v1");
            {
                assert_version("ADC", "cw32l011_v1");
                &[
                    "gpio_cw32l011_v1",
                    "rcc_cw32l011_v1",
                    "cw32l011",
                    "adc_cw32l011_v1",
                ]
            }
        }
        "CW32L012" => {
            for name in ["GPIOA", "SYSCTRL", "FLASH"] {
                assert_version(name, "cw32l012_v1");
            }
            for name in ["ADC1", "ADC2"] {
                assert_version(name, "cw32l012_v1");
            }
            assert_version("BGR", "cw32l012_v1");
            &[
                "gpio_cw32l012_v1",
                "rcc_cw32l012_v1",
                "cw32l012",
                "adc_cw32l012_v1",
            ]
        }
        "CW32L031" | "CW32R031" | "CW32W031" => {
            for name in ["GPIOA", "SYSCTRL", "FLASH"] {
                assert_version(name, "cw32l031_v1");
            }
            assert_version("ADC", "cw32l031_v1");
            match metadata.line {
                "CW32L031" => &[
                    "gpio_cw32l031_v1",
                    "rcc_cw32l031_v1",
                    "cw32l031",
                    "adc_cw32l031_v1",
                ],
                "CW32R031" => &[
                    "gpio_cw32l031_v1",
                    "rcc_cw32l031_v1",
                    "cw32r031",
                    "adc_cw32l031_v1",
                ],
                _ => &[
                    "gpio_cw32l031_v1",
                    "rcc_cw32l031_v1",
                    "cw32w031",
                    "adc_cw32l031_v1",
                ],
            }
        }
        "CW32L052" => {
            assert_version("GPIOA", "cw32l052_v1");
            assert_version("SYSCTRL", "cw32l052_v1");
            assert_version("FLASH", "cw32l031_v1");
            assert_version("ADC", "cw32l052_v1");
            &[
                "gpio_cw32l052_v1",
                "rcc_cw32l052_v1",
                "cw32l052",
                "adc_cw32l052_v1",
            ]
        }
        "CW32L083" => {
            for name in ["GPIOA", "SYSCTRL", "FLASH"] {
                assert_version(name, "cw32l083_v1");
            }
            assert_version("ADC", "cw32l083_v1");
            &[
                "gpio_cw32l083_v1",
                "rcc_cw32l083_v1",
                "cw32l083",
                "adc_cw32l083_v1",
            ]
        }
        _ => panic!("HAL backend is not implemented for this family"),
    };
    for cfg in selected {
        println!("cargo:rustc-cfg={cfg}");
    }
    // Presence is independent of register version. Backend assertions above
    // qualify the selected device before exposing its peripheral module.
    if metadata
        .peripherals
        .iter()
        .any(|p| p.registers.as_ref().is_some_and(|r| r.kind == "adc"))
    {
        println!("cargo:rustc-cfg=adc");
    }
    if x030 {
        assert_version("FLASH", "v1");
    }
    // All GPIO backends now have reviewed IRQ field/command masks.
    let exti = true;
    if exti {
        println!("cargo:rustc-cfg=gpio_exti");
    }
    assert_version(
        "CRC",
        match metadata.line {
            "CW32F002" | "CW32F003" => "cw32f002_v1",
            "CW32F020" => "cw32f020_v1",
            "CW32L010" | "CW32L011" | "CW32L012" => "cw32l010_v1",
            "CW32L031" | "CW32R031" | "CW32W031" | "CW32L083" => "cw32l031_v1",
            "CW32L052" => "cw32l052_v1",
            _ => "v1",
        },
    );
    println!("cargo:rustc-cfg=crc");
    if !matches!(metadata.line, "CW32F002" | "CW32F003") {
        println!("cargo:rustc-cfg=crc_poly_8005");
    }
    if x030 || f020 {
        println!("cargo:rustc-cfg=crc_input_16bit");
        println!("cargo:rustc-cfg=crc_input_32bit");
    }
    if !matches!(metadata.line, "CW32L010" | "CW32L011") {
        assert_version(
            "WWDT",
            match metadata.line {
                "CW32L012" => "cw32l012_v1",
                "CW32L031" | "CW32L052" | "CW32L083" | "CW32R031" | "CW32W031" => "cw32l031_v1",
                _ => "v1",
            },
        );
        println!("cargo:rustc-cfg=wwdt");
    }
    for (enabled, kind, prefix, version) in [
        (
            uart,
            "uart",
            "UART",
            match metadata.line {
                "CW32F002" | "CW32F003" => "cw32f002_v1",
                "CW32L010" | "CW32L011" => "cw32l010_v1",
                "CW32L012" => "cw32l012_v1",
                "CW32L052" => "cw32l052_v1",
                "CW32L083" => "cw32l083_v1",
                "CW32L031" | "CW32R031" | "CW32W031" => "cw32l031_v1",
                _ => "v1",
            },
        ),
        (
            spi,
            "spi",
            "SPI",
            match metadata.line {
                "CW32F002" | "CW32F003" => "cw32f002_v1",
                "CW32L010" | "CW32L011" => "cw32l010_v1",
                "CW32L012" => "cw32l012_v1",
                "CW32L031" | "CW32R031" | "CW32W031" | "CW32L052" | "CW32L083" => "cw32l031_v1",
                _ => "v1",
            },
        ),
        (
            i2c,
            "i2c",
            "I2C",
            match metadata.line {
                "CW32F002" | "CW32F003" => "cw32f002_v1",
                "CW32L010" => "cw32l010_v1",
                "CW32L011" => "cw32l011_v1",
                "CW32L012" => "cw32l012_v1",
                "CW32L031" | "CW32R031" | "CW32W031" | "CW32L052" | "CW32L083" => "cw32l031_v1",
                _ => "v1",
            },
        ),
    ] {
        if enabled {
            for p in metadata
                .peripherals
                .iter()
                .filter(|p| p.name.starts_with(prefix))
            {
                assert_version(p.name, version);
            }
            println!("cargo:rustc-cfg={kind}");
            println!("cargo:rustc-cfg={kind}_{version}");
        }
    }
    if serial {
        println!("cargo:rustc-cfg=gpio_af");
    }
    assert_version(
        "IWDT",
        if matches!(
            metadata.line,
            "CW32L031" | "CW32R031" | "CW32W031" | "CW32L052" | "CW32L083"
        ) {
            "cw32l031_v1"
        } else {
            "v1"
        },
    );
    println!("cargo:rustc-cfg=iwdt");
    assert_eq!(metadata.nvic_priority_bits, Some(2));
    let mut out =
        String::from("// Generated from selected-chip PAC metadata and reviewed pinout data.\n");
    if let Some(facts) = metadata
        .peripherals
        .iter()
        .find(|p| p.name == "ATIM")
        .and_then(|p| p.atim_complementary.as_ref())
    {
        let expected = if matches!(atim_version, Some("cw32l010_v1" | "cw32l012_v1")) {
            (4, 1008, 1)
        } else {
            assert_eq!(atim_version, Some("v1"));
            assert!(matches!(metadata.line, "CW32F030" | "CW32A030"));
            println!("cargo:rustc-cfg=atim_classic_complementary");
            (3, 1010, 0)
        };
        assert_eq!(
            (
                facts.channels,
                facts.dead_time_max_ticks,
                facts.break_inputs
            ),
            expected
        );
        out.push_str(&format!("impl crate::timer::sealed::ComplementaryInstance for crate::peripherals::ATIM {{ const DEAD_TIME_MAX_TICKS: u16 = {}; }}\nimpl crate::timer::ComplementaryInstance for crate::peripherals::ATIM {{}}\n", facts.dead_time_max_ticks));
    }

    if dma {
        assert!(metadata.memory.len() <= 1);
        let sram = metadata
            .memory
            .first()
            .and_then(|regions| {
                regions
                    .iter()
                    .find(|region| region.kind == cw32_metapac::metadata::MemoryRegionKind::Ram)
            })
            .map(|region| {
                assert_eq!(region.address, 0x2000_0000);
                // Capacity is the selected part fact from parts.yaml, not a
                // common-family guess. Generic profiles have no memory map.
                assert!(region.size > 0);
                assert!(region.address.checked_add(region.size).is_some());
                (region.address, region.size)
            });
        out.push_str(&format!(
            "pub(crate) const DMA_CHANNEL_COUNT: usize = {};\npub(crate) const DMA_COPY_SRAM: Option<(u32, u32)> = {sram:?};\n",
            metadata.dma_channels.len()
        ));
    }
    generate_gpio(&mut out);
    generate_rcc(&mut out, time_driver);
    generate_time_driver(&mut out, time_driver);
    generate_gpio_interrupts(&mut out);
    generate_electrical(&mut out);
    generate_lvd_ir(&mut out);
    if flash_storage {
        out.push_str(&format!(
            "pub(crate) const FLASH_STORAGE_SIZE: Option<u32> = {flash_storage_size:?};\n"
        ));
    }
    out.push_str("embassy_hal_internal::peripherals! {\n");
    for peripheral in metadata.peripherals {
        if Some(peripheral.name) != time_driver {
            out.push_str(&format!("    {},\n", peripheral.name));
        }
    }
    for p in metadata.peripherals {
        if let Some(ir) = &p.ir {
            if !metadata.peripherals.iter().any(|p| p.name == ir.owner) {
                out.push_str(&format!("    {},\n", ir.owner));
            }
        }
    }
    for channel in metadata.dma_channels {
        out.push_str(&format!("    {},\n", channel.name));
    }
    for pin in metadata.pins {
        if safe_pin(pin.name) {
            out.push_str(&format!("    {},\n", pin.name));
        }
    }
    out.push_str("}\nembassy_hal_internal::interrupt_mod! {\n");
    for interrupt in metadata.interrupts {
        out.push_str(&format!("    {},\n", interrupt.name));
    }
    out.push_str("}\n");
    for peripheral in metadata
        .peripherals
        .iter()
        .filter(|p| matches!(p.name, "AWT" | "LPTIM" | "AUTOTRIM"))
    {
        let irq = peripheral
            .interrupts
            .iter()
            .find(|i| i.signal == "GLOBAL")
            .expect("reviewed timer GLOBAL interrupt is missing");
        let kind = peripheral.name.to_ascii_lowercase();
        out.push_str(&format!(
            "crate::{kind}::impl_instance!({}, {});\n",
            peripheral.name, irq.interrupt
        ));
    }
    for p in metadata.peripherals.iter().filter(|p| p.name == "HALLTIM") {
        out.push_str(&format!("crate::halltim::impl_instance!({});\n", p.name));
    }
    generate_lcd(&mut out);
    generate_rtc(&mut out);
    for pin in metadata.pins {
        if safe_pin(pin.name) {
            let port = pin.name.as_bytes()[1] - b'A';
            let number: u8 = pin.name[2..].parse().expect("invalid pin metadata");
            out.push_str(&format!(
                "crate::gpio::impl_pin!({}, {}, {});\n",
                pin.name, port, number
            ));
            if exti {
                let bank = format!("GPIO{}", char::from(b'A' + port));
                let irq = metadata
                    .peripherals
                    .iter()
                    .find(|p| p.name == bank)
                    .and_then(|p| p.interrupts.iter().find(|i| i.signal == "GLOBAL"))
                    .expect("reviewed GPIO GLOBAL interrupt is missing");
                out.push_str(&format!(
                    "impl crate::exti::ExtiPin for crate::peripherals::{} {{ type Interrupt = crate::interrupt::typelevel::{}; }}\n",
                    pin.name, irq.interrupt
                ));
            }
        }
    }
    if serial {
        if uart {
            for peripheral in metadata
                .peripherals
                .iter()
                .filter(|p| p.name.starts_with("UART"))
            {
                let number: u8 = peripheral.name[4..]
                    .parse()
                    .expect("invalid UART instance number");
                let irq = peripheral
                    .interrupts
                    .iter()
                    .find(|i| i.signal == "GLOBAL")
                    .expect("verified UART global interrupt is missing");
                out.push_str(&format!(
                    "crate::usart::impl_instance!({}, {}, {});\n",
                    peripheral.name,
                    irq.interrupt,
                    number - 1
                ));
            }
        }
        if spi {
            for peripheral in metadata
                .peripherals
                .iter()
                .filter(|p| p.name.starts_with("SPI"))
            {
                let limits = peripheral
                    .spi
                    .as_ref()
                    .expect("verified SPI clock limits are missing");
                let irq = peripheral
                    .interrupts
                    .iter()
                    .find(|i| i.signal == "GLOBAL")
                    .expect("verified SPI global interrupt is missing");
                out.push_str(&format!(
                    "crate::spi::impl_instance!({}, {}, {}, {});\n",
                    peripheral.name,
                    limits.maximum_frequency,
                    limits.minimum_divisor,
                    irq.interrupt,
                ));
            }
        }
        for p in metadata.peripherals {
            if let Some(limits) = &p.dac_limits {
                out.push_str(&format!("crate::dac::impl_instance!({});\n", p.name));
                out.push_str(&format!("pub(crate) const DAC_SUPPLY_RANGE_MV: (u16,u16) = {:?};\npub(crate) const DAC_MAX_CODE: u16 = {};\n", limits.supply_mv, (1u16 << limits.resolution_bits) - 1));
                for pin in p.pins {
                    assert!(pin.af.is_none());
                    out.push_str(&format!(
                        "crate::dac::impl_pin!({}, {}, {});\n",
                        p.name,
                        pin.pin,
                        pin.signal.strip_prefix("OUT").unwrap()
                    ));
                }
            }
            if let Some(limits) = &p.opa_limits {
                out.push_str(&format!(
                    "crate::opamp::impl_instance!({}, {}, {:?}, {});\n",
                    p.name,
                    p.name.strip_prefix("OPA").unwrap(),
                    limits.supply_mv,
                    limits.output_headroom_mv
                ));
                for pin in p.pins {
                    assert!(pin.af.is_none());
                    out.push_str(&format!(
                        "crate::opamp::impl_pin!({}, {}, {});\n",
                        p.name, pin.pin, pin.signal
                    ));
                }
            }
        }
        for peripheral in &reference_dividers {
            let facts = peripheral.reference_divider.as_ref().unwrap();
            out.push_str(&format!(
                "crate::vref::impl_instance!({}, {}, {}, {}, {}, {:?}, {});\n",
                peripheral.name,
                facts.clock_owner,
                facts.consumers[0],
                facts.consumers[1],
                facts.negative_mux,
                facts.supply_mv,
                facts.input_uses_vdda
            ));
        }
        for peripheral in &comparators {
            out.push_str(&format!(
                "crate::comparator::impl_instance!({});\n",
                peripheral.name
            ));
        }
        if let Some(first) = comparators.first() {
            let limits = first
                .comparator_limits
                .as_ref()
                .expect("comparator electrical limits missing");
            out.push_str(&format!(
                "pub(crate) const COMPARATOR_SUPPLY_RANGE_MV: (u16,u16) = {:?};\n",
                limits.supply_mv
            ));
            out.push_str(&format!(
                "pub(crate) const COMPARATOR_INPUT_USES_VDDA: bool = {};\n",
                limits.input_uses_vdda
            ));
        }
        if adc {
            let peripheral = metadata
                .peripherals
                .iter()
                .find(|p| p.registers.as_ref().is_some_and(|r| r.kind == "adc"))
                .expect("ADC instance missing");
            let links: Vec<_> = peripheral
                .interrupts
                .iter()
                .filter(|i| i.signal == "GLOBAL")
                .collect();
            assert_eq!(links.len(), 1, "software ADC requires one GLOBAL IRQ");
            let irq = links[0].interrupt;
            assert_eq!(
                metadata.interrupts.iter().filter(|i| i.name == irq).count(),
                1
            );
            assert!(
                !metadata
                    .peripherals
                    .iter()
                    .any(|other| other.name != peripheral.name
                        && other.interrupts.iter().any(|i| i.interrupt == irq)),
                "software ADC IRQ must be dedicated"
            );
            out.push_str(&format!(
                "crate::adc::impl_instance!({}, {});\n",
                peripheral.name, irq
            ));
        }
        if dual_adc {
            let peripheral = metadata
                .peripherals
                .iter()
                .find(|p| p.name == "ADC1")
                .expect("ADC1 missing");
            let links: Vec<_> = peripheral
                .interrupts
                .iter()
                .filter(|i| i.signal == "GLOBAL")
                .collect();
            assert_eq!(links.len(), 1, "ADC1 requires one GLOBAL IRQ");
            let irq = links[0].interrupt;
            assert_eq!(
                metadata.interrupts.iter().filter(|i| i.name == irq).count(),
                1
            );
            assert!(
                !metadata
                    .peripherals
                    .iter()
                    .any(|other| other.name != peripheral.name
                        && other.interrupts.iter().any(|i| i.interrupt == irq)),
                "ADC1 software IRQ must be dedicated"
            );
            out.push_str(&format!(
                "crate::adc::impl_instance!(ADC1, ADC2, {});\n",
                irq
            ));
            out.push_str("crate::adc::impl_instance!(ADC2, ADC1);\n");
        }
        if i2c {
            for peripheral in metadata
                .peripherals
                .iter()
                .filter(|p| p.name.starts_with("I2C"))
            {
                let number: u8 = if peripheral.name == "I2C" {
                    1
                } else {
                    peripheral.name[3..]
                        .parse()
                        .expect("invalid I2C instance number")
                };
                out.push_str(&format!(
                    "crate::i2c::impl_instance!({}, {});\n",
                    peripheral.name, number
                ));
            }
        }
        if gtim_version.is_some() {
            for peripheral in metadata
                .peripherals
                .iter()
                .filter(|p| p.name.starts_with("GTIM") && Some(p.name) != time_driver)
            {
                let name = peripheral.name;
                let number: u8 = if name == "GTIM" {
                    1
                } else {
                    name[4..].parse().expect("invalid GTIM number")
                };
                out.push_str(&format!(
                    "crate::timer::impl_gtim!({}, {});\n",
                    name, number
                ));
                if let Some(input) = &peripheral.classic_timer_input {
                    out.push_str(&format!(
                        "crate::timer::impl_classic_input!({}, {}, {:?});\n",
                        name,
                        input.capture_mux.to_ascii_lowercase(),
                        input.encoder_fixed_reload
                    ));
                }
            }
        }
        for name in ["BTIM1", "BTIM2", "BTIM3"] {
            out.push_str(&format!("crate::timer::impl_btim!({name});\n"));
        }
        if dma {
            for channel in metadata.dma_channels {
                let physical = format!("DMACHANNEL{}", channel.channel + 1);
                let peripheral = metadata
                    .peripherals
                    .iter()
                    .find(|p| p.name == physical)
                    .expect("DMA register instance missing");
                let irq = peripheral
                    .interrupts
                    .iter()
                    .find(|i| i.signal == "GLOBAL")
                    .expect("DMA shared interrupt mapping missing");
                out.push_str(&format!(
                    "crate::dma::impl_channel!({}, {}, {});\n",
                    channel.name, channel.channel, irq.interrupt
                ));
            }
        }
        for peripheral in metadata.peripherals {
            if Some(peripheral.name) == time_driver {
                continue;
            }
            let staged_peripheral = uart_dma_peripherals
                .iter()
                .chain(&spi_dma_peripherals)
                .any(|name| name == peripheral.name);
            // Preserve the existing x030 raw inventory; L083 adds only the
            // qualified UART/SPI RX/TX subset, never ADC/LCD or timer requests.
            if x030 || staged_peripheral {
                for route in peripheral.dma_channels {
                    if route.dma == Some("DMA") {
                        let request = route.request.expect("DMA request missing");
                        out.push_str(&format!(
                            "crate::dma::impl_request!({}_{}, {}, {}, {});\n",
                            peripheral.name, route.signal, peripheral.name, route.signal, request
                        ));
                    }
                }
            }
            for pin in peripheral.pins {
                if !safe_pin(pin.pin) {
                    continue;
                }
                assert!(
                    metadata.pins.iter().any(|p| p.name == pin.pin),
                    "AF pin not bonded on selected chip"
                );
                if (adc && peripheral.name == "ADC")
                    || (dual_adc && matches!(peripheral.name, "ADC1" | "ADC2"))
                {
                    if pin.signal.starts_with("IN") {
                        // Source labels are not always hardware mux values: R031
                        // ADC_IN0..8 select mux4..12. Use explicit reviewed metadata.
                        let channel = pin
                            .adc_mux
                            .expect("ADC pin route requires an explicit hardware mux");
                        assert!(
                            channel
                                <= if dual_adc {
                                    11
                                } else if sequence_adc {
                                    13
                                } else {
                                    12
                                },
                            "external ADC mux is not a verified pin route"
                        );
                        assert!(pin.af.is_none(), "analog route cannot use a digital AF");
                        out.push_str(&format!(
                            "crate::adc::impl_pin!({}, {}, {});\n",
                            peripheral.name, pin.pin, channel
                        ));
                    }
                    continue;
                }
                if peripheral
                    .registers
                    .as_ref()
                    .is_some_and(|r| r.kind == "vc")
                {
                    // Digital output AF metadata remains PAC-only in this polling subset.
                    let Some(mux) = pin.comparator_mux else {
                        continue;
                    };
                    let direction = if pin.signal.starts_with("INP") {
                        "INP"
                    } else if pin.signal.starts_with("INN") {
                        "INN"
                    } else {
                        panic!("unsupported comparator route")
                    };
                    assert!(pin.af.is_none() && pin.adc_mux.is_none());
                    out.push_str(&format!(
                        "crate::comparator::impl_pin!({}, {}, {}, {});\n",
                        peripheral.name, pin.pin, direction, mux
                    ));
                    continue;
                }
                let Some(af) = pin.af else {
                    continue;
                };
                if peripheral.name == "HALLTIM" {
                    let role = match pin.signal {
                        "CH1" => "Ch1",
                        "CH2" => "Ch2",
                        "CH3" => "Ch3",
                        _ => panic!("unreviewed Hall input signal"),
                    };
                    assert_eq!(af, 9);
                    out.push_str(&format!(
                        "crate::halltim::impl_pin!({}, {}, {}, {});\n",
                        peripheral.name, role, pin.pin, af
                    ));
                }
                if uart && peripheral.name.starts_with("UART") {
                    let role = match pin.signal {
                        "TX" => Some("TxPin"),
                        "RX" => Some("RxPin"),
                        "RTS" => Some("RtsPin"),
                        "CTS" => Some("CtsPin"),
                        _ => None,
                    };
                    if let Some(role) = role {
                        out.push_str(&format!(
                            "crate::usart::impl_pin!({}, {}, {}, {});\n",
                            pin.pin, peripheral.name, role, af
                        ));
                    }
                }
                if i2c && peripheral.name.starts_with("I2C") {
                    let role = match pin.signal {
                        "SCL" => Some("SclPin"),
                        "SDA" => Some("SdaPin"),
                        _ => None,
                    };
                    if let Some(role) = role {
                        out.push_str(&format!(
                            "crate::i2c::impl_pin!({}, {}, {}, {});\n",
                            role, peripheral.name, pin.pin, af
                        ));
                    }
                }
                let buffered_input = matches!(gtim_version, Some("cw32l010_v1" | "cw32l012_v1"))
                    && (peripheral.name.starts_with("GTIM") || peripheral.name == "ATIM");
                let capture_role = match pin.signal {
                    "CAP1" if peripheral.classic_timer_input.is_some() => Some("Ch1"),
                    "CAP2" if peripheral.classic_timer_input.is_some() => Some("Ch2"),
                    "CAP3" if peripheral.classic_timer_input.is_some() => Some("Ch3"),
                    "CAP4" if peripheral.classic_timer_input.is_some() => Some("Ch4"),
                    "CH1" if buffered_input => Some("Ch1"),
                    "CH2" if buffered_input => Some("Ch2"),
                    "CH3" if buffered_input => Some("Ch3"),
                    "CH4" if buffered_input => Some("Ch4"),
                    _ => None,
                };
                if let Some(role) = capture_role {
                    out.push_str(&format!(
                        "crate::timer::impl_capture_pin!({}, {}, {}, {});\n",
                        peripheral.name, role, pin.pin, af
                    ));
                }
                if gtim_version.is_some()
                    && peripheral.name.starts_with("GTIM")
                    && Some(peripheral.name) != time_driver
                {
                    let role = match pin.signal {
                        "CH1" => Some("Ch1"),
                        "CH2" => Some("Ch2"),
                        "CH3" => Some("Ch3"),
                        "CH4" => Some("Ch4"),
                        _ => None,
                    };
                    if let Some(role) = role {
                        out.push_str(&format!(
                            "crate::timer::impl_timer_pin!({}, {}, {}, {});\n",
                            peripheral.name, role, pin.pin, af
                        ));
                    }
                }
                if atim_version.is_some() && peripheral.name == "ATIM" {
                    let role = match pin.signal {
                        "CH1A" | "CH1" => Some("Ch1"),
                        "CH2A" | "CH2" => Some("Ch2"),
                        "CH3A" | "CH3" => Some("Ch3"),
                        "CH4" if matches!(atim_version, Some("cw32l010_v1" | "cw32l012_v1")) => {
                            Some("Ch4")
                        }
                        _ => None,
                    };
                    if let Some(role) = role {
                        out.push_str(&format!(
                            "crate::timer::impl_timer_pin!(ATIM, {}, {}, {});\n",
                            role, pin.pin, af
                        ));
                    }
                }
                if peripheral.name == "ATIM" && peripheral.atim_complementary.is_some() {
                    match pin.signal {
                        "CH1N" | "CH2N" | "CH3N" | "CH4N" | "CH1B" | "CH2B" | "CH3B" => out
                            .push_str(&format!(
                                "crate::timer::impl_complementary_pin!(Ch{}, {}, {});\n",
                                &pin.signal[2..3],
                                pin.pin,
                                af
                            )),
                        "BK" if matches!(atim_version, Some("cw32l010_v1" | "cw32l012_v1")) => out
                            .push_str(&format!(
                                "crate::timer::impl_timer_break_pin!({}, {});\n",
                                pin.pin, af
                            )),
                        _ => {}
                    }
                }
                if spi && peripheral.name.starts_with("SPI") {
                    let role = match pin.signal {
                        "SCK" => Some("SckPin"),
                        "MOSI" => Some("MosiPin"),
                        "MISO" => Some("MisoPin"),
                        _ => None,
                    };
                    if let Some(role) = role {
                        out.push_str(&format!(
                            "crate::spi::impl_pin!({}, {}, {}, {});\n",
                            role, peripheral.name, pin.pin, af
                        ));
                    }
                }
            }
        }
    }
    fs::write(
        PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("_generated.rs"),
        out,
    )
    .unwrap();
    println!("cargo:rerun-if-changed=build.rs");
}

// Restrict debug/reset/input-only pads according to each reviewed family.
// L012 PF3/BOOT is bidirectional and must not inherit the x030 exclusion.
fn safe_pin(name: &str) -> bool {
    match cw32_metapac::metadata::METADATA.line {
        "CW32F002" | "CW32F003" => !matches!(name, "PA2" | "PA5" | "PC5"),
        "CW32L010" => !matches!(name, "PA7" | "PA8" | "PB7"),
        "CW32L011" | "CW32L012" => !matches!(name, "PA13" | "PA14"),
        _ => !matches!(name, "PA13" | "PA14" | "PF3"),
    }
}

// Direct selected-family PAC views, following embassy-stm32's gpio_block().
// Unlike its uniform stride, CW32L012 compresses the F-bank address. Use each
// metadata address, and prove the accessed subset against the actual bank IR.
fn generate_gpio(out: &mut String) {
    use cw32_metapac::metadata::{PeripheralRegisters, ir};
    let metadata = &cw32_metapac::metadata::METADATA;
    fn register(
        regs: &PeripheralRegisters,
        name: &str,
    ) -> (&'static ir::BlockItem, &'static ir::Register) {
        let block = regs
            .ir
            .blocks
            .iter()
            .find(|b| b.name == regs.block)
            .unwrap();
        let item = block
            .items
            .iter()
            .find(|r| r.name.eq_ignore_ascii_case(name))
            .unwrap_or_else(|| panic!("missing GPIO register {}.{name}", regs.block));
        let ir::BlockItemInner::Register(reg) = &item.inner else {
            panic!("expected register")
        };
        (item, reg)
    }
    fn fields(regs: &PeripheralRegisters, name: &str) -> &'static [ir::Field] {
        let (_, reg) = register(regs, name);
        regs.ir
            .fieldsets
            .iter()
            .find(|f| Some(f.name) == reg.fieldset)
            .unwrap()
            .fields
    }
    fn offset(field: &ir::Field) -> u32 {
        let ir::BitOffset::Regular(ref position) = field.bit_offset else {
            panic!("nonlinear GPIO field")
        };
        assert!(field.array.is_none());
        position.offset
    }
    let gpios: Vec<_> = metadata
        .peripherals
        .iter()
        .filter(|p| p.name.starts_with("GPIO"))
        .collect();
    let common = gpios
        .iter()
        .find(|p| p.name == "GPIOA")
        .unwrap()
        .registers
        .as_ref()
        .unwrap();
    let sysctrl = metadata
        .peripherals
        .iter()
        .find(|p| p.name == "SYSCTRL")
        .unwrap()
        .registers
        .as_ref()
        .unwrap();
    let af_width = fields(common, "AFRL")[0].bit_size;
    assert!(matches!(af_width, 3 | 4));
    let af_mask = (1u32 << af_width) - 1;
    // Field width and documented selector domain are distinct on F020/L03x/L05x/L08x.
    let af_max = if matches!(metadata.line, "CW32F030" | "CW32A030" | "CW32L012") {
        15
    } else {
        7
    };
    assert!(af_max <= af_mask);

    // Unsafe AnyPin::steal validates family output-capable pads, not bonding on
    // the selected package. Safe singleton projection below remains unchanged.
    let mut pin_masks = [0u16; 6];
    let mut pull_down_masks = [0u16; 6];
    for p in &gpios {
        let port = usize::from(p.name.as_bytes()[4] - b'A');
        let facts = p.gpio.as_ref().expect("missing GPIO pad capabilities");
        assert!(port < 6);
        pin_masks[port] = facts.output_mask;
        pull_down_masks[port] = facts.pull_down_mask;
    }
    let mut clock_masks = [0u32; 6];
    out.push_str("pub(crate) fn gpio_block(port: u8) -> crate::pac::gpio::Gpio {\nlet address = match port {\n");
    for p in gpios {
        let port = usize::from(p.name.as_bytes()[4] - b'A');
        let actual = p.registers.as_ref().unwrap();
        assert_eq!(actual.version, common.version);
        let gate = p.rcc.as_ref().unwrap().enable.as_ref().unwrap();
        assert!(gate.register.eq_ignore_ascii_case("AHBEN"));
        let field = fields(sysctrl, gate.register)
            .iter()
            .find(|f| f.name.eq_ignore_ascii_case(gate.field))
            .unwrap();
        assert_eq!(field.bit_size, 1);
        clock_masks[port] = 1 << offset(field);
        // RMW/read/command widths and permissions must match the canonical
        // family view. Fields not used by HAL may legitimately differ by bank.
        let mut names = vec![
            "DIR",
            "OPENDRAIN",
            "PUR",
            "ANALOG",
            "RISEIE",
            "FALLIE",
            "IDR",
            "ODR",
            "BRR",
            "BSRR",
            "TOG",
        ];
        if pull_down_masks[port] != 0 {
            names.push("PDR");
        }
        if p.gpio_interrupt.as_ref().unwrap().level_trigger {
            names.extend(["HIGHIE", "LOWIE"]);
        }
        if common.version == "v1" {
            names.extend(["SPEED", "LOCK"]);
        }
        if common.version == "cw32l052_v1" {
            names.push("LOCK");
        }
        if common.version == "cw32l083_v1" {
            names.push("LCKR");
        }
        if pin_masks[port] & 0xff != 0 {
            names.push("AFRL");
        }
        if pin_masks[port] & 0xff00 != 0 {
            names.push("AFRH");
        }
        for name in names {
            let (a, ar) = register(actual, name);
            let (c, cr) = register(common, name);
            assert_eq!(
                (a.byte_offset, &a.array, ar.bit_size, &ar.access),
                (c.byte_offset, &c.array, cr.bit_size, &cr.access),
                "{} {name} differs from family GPIO view",
                p.name
            );
            let pins = if name == "PDR" {
                pull_down_masks[port]
            } else {
                pin_masks[port]
            };
            for pin in 0..16 {
                if pins & (1 << pin) == 0
                    || (name == "AFRL" && pin >= 8)
                    || (name == "AFRH" && pin < 8)
                {
                    continue;
                }
                let (position, width) = if name.starts_with("AFR") {
                    ((pin % 8) * 4, af_width)
                } else {
                    (pin, 1)
                };
                for regs in [actual, common] {
                    assert!(
                        fields(regs, name)
                            .iter()
                            .any(|f| offset(f) == position && f.bit_size == width),
                        "{} {name}: absent or different field for pin {pin}",
                        p.name
                    );
                }
            }
            if matches!(name, "LOCK" | "LCKR") {
                for regs in [actual, common] {
                    assert!(
                        fields(regs, name)
                            .iter()
                            .any(|f| f.name.eq_ignore_ascii_case("KEY")
                                && offset(f) == 16
                                && f.bit_size == 16)
                    );
                }
            }
        }
        // EXTI extends the common-view proof beyond synchronous GPIO. All raw
        // accesses require exact widths, offsets and permissions, even where
        // vendor fieldsets are sparse. ICR commands use own-manual metadata:
        // sparse PAC fields cannot narrow documented write-one no-op bits.
        let irq = p
            .gpio_interrupt
            .as_ref()
            .expect("missing GPIO interrupt facts");
        let idr_access = register(common, "IDR").1.access.clone();
        assert!(matches!(
            idr_access,
            ir::Access::Read | ir::Access::ReadWrite
        ));
        let mut irq_registers = vec![
            ("RISEIE", 0x24, ir::Access::ReadWrite),
            ("FALLIE", 0x28, ir::Access::ReadWrite),
            ("ISR", 0x34, ir::Access::Read),
            ("ICR", 0x38, ir::Access::ReadWrite),
            ("IDR", 0x50, idr_access),
        ];
        if irq.level_trigger {
            irq_registers.extend([
                ("HIGHIE", 0x2c, ir::Access::ReadWrite),
                ("LOWIE", 0x30, ir::Access::ReadWrite),
            ]);
        } else {
            for regs in [actual, common] {
                let block = regs
                    .ir
                    .blocks
                    .iter()
                    .find(|b| b.name == regs.block)
                    .unwrap();
                assert!(
                    !block
                        .items
                        .iter()
                        .any(|item| [0x2c, 0x30].contains(&item.byte_offset)),
                    "edge-only GPIO has unexpected native-level registers"
                );
            }
        }
        for (name, byte_offset, access) in irq_registers {
            for regs in [actual, common] {
                let (item, reg) = register(regs, name);
                assert_eq!(
                    (
                        item.byte_offset,
                        item.array.as_ref(),
                        reg.bit_size,
                        &reg.access
                    ),
                    (byte_offset, None, 32, &access),
                    "{} {name}: unproved EXTI register layout",
                    p.name
                );
                for pin in 0..16 {
                    if irq.serviced_mask & (1 << pin) != 0 {
                        assert!(
                            fields(regs, name)
                                .iter()
                                .any(|f| offset(f) == pin && f.bit_size == 1),
                            "{} {name}: unproved EXTI serviced bit {pin}",
                            p.name
                        );
                    }
                }
                if name == "ICR" {
                    for field in fields(regs, name) {
                        assert_eq!(field.bit_size, 1);
                        assert!(
                            offset(field) < 16 && irq.clear_noop_mask & (1 << offset(field)) != 0,
                            "GPIO ICR field is outside its documented command domain"
                        );
                    }
                }
            }
        }
        out.push_str(&format!("{port} => {:#x}usize,\n", p.address));
    }
    let keyed = matches!(metadata.line, "CW32L010" | "CW32L011" | "CW32L012");
    assert_eq!(
        fields(sysctrl, "AHBEN")
            .iter()
            .any(|f| f.name.eq_ignore_ascii_case("KEY") && offset(f) == 16 && f.bit_size == 16),
        keyed
    );
    for port in 0..6 {
        assert!(pin_masks[port] == 0 || clock_masks[port] != 0);
    }
    out.push_str("_ => panic!(\"invalid GPIO port\"),\n};\nunsafe { crate::pac::gpio::Gpio::from_ptr(address as *mut ()) }\n}\n");
    out.push_str(&format!(
        "pub(crate) const GPIO_PIN_MASKS: [u16; 6] = {pin_masks:?};\n"
    ));
    out.push_str("pub(crate) fn gpio_rcc(port: u8) -> crate::rcc::RccInfo { match port {\n");
    for peripheral in metadata
        .peripherals
        .iter()
        .filter(|p| p.name.starts_with("GPIO"))
    {
        assert!(
            peripheral.rcc_control.is_some(),
            "missing GPIO clock control"
        );
        let port = peripheral.name.as_bytes()[4] - b'A';
        out.push_str(&format!(
            "{port} => <peripherals::{} as crate::rcc::SealedRccPeripheral>::RCC_INFO,\n",
            peripheral.name
        ));
    }
    out.push_str("_ => panic!(\"invalid GPIO port\"), } }\n");
    if !matches!(metadata.line, "CW32L010" | "CW32L011") {
        out.push_str(&format!(
            "pub(crate) const GPIO_PULL_DOWN_MASKS: [u16; 6] = {pull_down_masks:?};\n"
        ));
    }
    out.push_str(&format!("pub(crate) const GPIO_AF_MASK: u32 = {af_mask};\npub(crate) const GPIO_AF_MAX: u32 = {af_max};\n"));
}

// Bank order, compact state slots and complete vector memberships all come
// from actual GPIO peripherals and their GLOBAL links, not family-name tables.
fn generate_gpio_interrupts(out: &mut String) {
    let metadata = &cw32_metapac::metadata::METADATA;
    let mut banks: Vec<_> = metadata
        .peripherals
        .iter()
        .filter(|p| p.name.starts_with("GPIO"))
        .collect();
    banks.sort_by_key(|p| p.name);
    let mut serviced = [0u16; 6];
    let mut commands = [0u16; 6];
    let mut state_indices = [usize::MAX; 6];
    let mut groups = std::collections::BTreeMap::<&str, Vec<u8>>::new();
    let mut level_trigger = None;
    for (index, bank) in banks.iter().enumerate() {
        assert!(bank.name.len() == 5);
        let port = bank.name.as_bytes()[4] - b'A';
        assert!(port < 6);
        let facts = bank
            .gpio_interrupt
            .as_ref()
            .expect("missing GPIO interrupt facts");
        assert_ne!(facts.serviced_mask, 0);
        assert_eq!(facts.serviced_mask & !facts.clear_noop_mask, 0);
        assert_eq!(
            *level_trigger.get_or_insert(facts.level_trigger),
            facts.level_trigger,
            "selected GPIO family has differing trigger policies"
        );
        serviced[usize::from(port)] = facts.serviced_mask;
        commands[usize::from(port)] = facts.clear_noop_mask;
        state_indices[usize::from(port)] = index;
        let links: Vec<_> = bank
            .interrupts
            .iter()
            .filter(|i| i.signal == "GLOBAL")
            .collect();
        assert_eq!(
            links.len(),
            1,
            "GPIO bank must have exactly one GLOBAL interrupt"
        );
        let irq = links[0].interrupt;
        assert_eq!(
            metadata.interrupts.iter().filter(|i| i.name == irq).count(),
            1,
            "GPIO GLOBAL link must name one real interrupt"
        );
        groups.entry(irq).or_default().push(port);
    }
    if level_trigger == Some(true) {
        println!("cargo:rustc-cfg=gpio_irq_level");
    }
    out.push_str(&format!(
        "pub(crate) const GPIO_SERVICED_MASKS: [u16; 6] = {serviced:?};\n"
    ));
    out.push_str(&format!(
        "pub(crate) const GPIO_CLEAR_NOOP_MASKS: [u16; 6] = {commands:?};\n"
    ));
    out.push_str(&format!(
        "pub(crate) const GPIO_BANK_COUNT: usize = {};\n",
        banks.len()
    ));
    // Use explicit absent slots instead of host-width usize::MAX literals.
    let state_indices: Vec<_> = state_indices
        .iter()
        .map(|index| {
            if *index == usize::MAX {
                "usize::MAX".into()
            } else {
                index.to_string()
            }
        })
        .collect();
    out.push_str(&format!(
        "pub(crate) const GPIO_STATE_INDEX: [usize; 6] = [{}];\n",
        state_indices.join(", ")
    ));
    let mut impls = String::from("// Generated from selected-chip GPIO GLOBAL interrupt links.\n");
    for (irq, ports) in groups {
        impls.push_str(&format!("impl sealed::GpioInterrupt for typelevel::{irq} {{}}\nimpl GpioInterrupt for typelevel::{irq} {{ const BANKS: &'static [u8] = &{ports:?}; }}\n"));
        if ports.len() == 1 {
            impls.push_str(&format!(
                "impl PortInterrupt for typelevel::{irq} {{ const PORT: u8 = {}; }}\n",
                ports[0]
            ));
        }
    }
    fs::write(
        PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("_generated_exti.rs"),
        impls,
    )
    .unwrap();
}

// Selected chip facts are data, not register-version selection logic.
fn generate_electrical(out: &mut String) {
    use std::fmt::Write;
    let peripherals = cw32_metapac::metadata::METADATA.peripherals;
    let c = peripherals
        .iter()
        .find_map(|p| p.clock_limits.as_ref())
        .expect("missing SYSCTRL electrical facts");
    writeln!(
        out,
        "pub(crate) const RCC_HSI_FREQUENCY_HZ: u32 = {};",
        c.hsi_frequency_hz
    )
    .unwrap();
    writeln!(
        out,
        "pub(crate) const RCC_HSI_ERROR_PERCENT: u32 = {};",
        c.hsi_error_percent
    )
    .unwrap();
    writeln!(
        out,
        "pub(crate) const RCC_HSI_SUPPLY_RANGE_MV: (u16,u16) = {:?};",
        c.hsi_supply_mv
    )
    .unwrap();
    writeln!(
        out,
        "pub(crate) const RCC_HSI_TEMPERATURE_RANGE_C: (i16,i16) = {:?};",
        c.hsi_temperature_c
    )
    .unwrap();
    writeln!(
        out,
        "pub(crate) const RCC_LOW_VOLTAGE_THRESHOLD_MV: u16 = {};",
        c.low_voltage_threshold_mv
    )
    .unwrap();
    writeln!(
        out,
        "pub(crate) const RCC_LOW_VOLTAGE_BUS_MAX_HZ: u32 = {};",
        c.low_voltage_bus_max_hz
    )
    .unwrap();
    writeln!(
        out,
        "pub(crate) const RCC_HIGH_VOLTAGE_BUS_MAX_HZ: u32 = {};",
        c.high_voltage_bus_max_hz
    )
    .unwrap();
    writeln!(
        out,
        "pub(crate) const RCC_FACTORY_HSI_TRIM_ADDRESS: usize = {};",
        c.factory_hsi_trim_address
    )
    .unwrap();
    writeln!(
        out,
        "pub(crate) const RCC_DEFAULT_HSI_DIV: crate::rcc::HsiDiv = crate::rcc::HsiDiv::Div{};",
        c.default_hsi_divisor
    )
    .unwrap();
    writeln!(
        out,
        "pub(crate) const RCC_INITIAL_FLASH_WAIT: u32 = {};",
        c.initial_flash_wait
    )
    .unwrap();
    writeln!(
        out,
        "pub(crate) const RCC_FLASH_WAIT_STEP_HZ: u32 = {};",
        c.flash_wait_step_hz
    )
    .unwrap();
    if c.hse.is_some() {
        println!("cargo:rustc-cfg=rcc_hse");
    }
    if c.hse.is_some() || c.hex.is_some() {
        println!("cargo:rustc-cfg=rcc_external_clock");
    }
    generate_lse(out, c);
    generate_pll(out, c);
    generate_hse(out, c);
    generate_hex(out, c);
    let a = peripherals
        .iter()
        .find_map(|p| p.adc_limits.as_ref())
        .expect("missing ADC electrical facts");
    for p in peripherals.iter().filter_map(|p| p.adc_limits.as_ref()) {
        assert_eq!(
            p, a,
            "ADC limits differ; use per-instance constants before adding this chip"
        );
    }
    if let Some(sequence) = &a.sequence {
        assert!(sequence.programmable_order && sequence.per_slot_result);
        if a.classic_scan.is_none() {
            assert!(sequence.per_slot_sample_time);
            assert_eq!(sequence.maximum_length, 8);
        }
        writeln!(
            out,
            "pub(crate) const ADC_SEQUENCE_MAX_LEN: usize = {};",
            sequence.maximum_length
        )
        .unwrap();
    }
    if let Some(scan) = &a.classic_scan {
        writeln!(
            out,
            "pub(crate) const ADC_SEQUENCE_SLOTS_PER_REGISTER: usize = {};",
            scan.slots_per_sequence_register
        )
        .unwrap();
        let sequence = a
            .sequence
            .as_ref()
            .expect("missing classic ADC sequence capabilities");
        assert!(matches!(sequence.maximum_length, 4 | 8));
        assert!(
            sequence.programmable_order
                && !sequence.per_slot_sample_time
                && sequence.per_slot_result
        );
        writeln!(
            out,
            "pub(crate) const ADC_BUFFERED_REQUIRES_SINGLE_CHANNEL: bool = {};",
            scan.buffered_requires_single_channel
        )
        .unwrap();
        writeln!(
            out,
            "pub(crate) const ADC_INTERNAL_REQUIRES_SINGLE_CHANNEL: bool = {};",
            scan.internal_requires_single_channel
        )
        .unwrap();
        writeln!(
            out,
            "pub(crate) const ADC_FIRST_INTERNAL_CHANNEL: u8 = {};",
            scan.first_internal_channel
        )
        .unwrap();
        writeln!(
            out,
            "pub(crate) const ADC_SUPPLY_CHANNEL: u8 = {};",
            scan.supply_channel
        )
        .unwrap();
        writeln!(
            out,
            "pub(crate) const ADC_TEMPERATURE_CHANNEL: Option<u8> = {:?};",
            scan.temperature_channel
        )
        .unwrap();
        writeln!(
            out,
            "pub(crate) const ADC_BANDGAP_CHANNEL: Option<u8> = {:?};",
            scan.bandgap_channel
        )
        .unwrap();
        writeln!(
            out,
            "pub(crate) const ADC_TEMPERATURE_STARTUP_US: u32 = {};",
            scan.temperature_startup_us
        )
        .unwrap();
        writeln!(
            out,
            "pub(crate) const ADC_BANDGAP_STARTUP_US: u32 = {};",
            scan.bandgap_startup_us
        )
        .unwrap();
    }
    writeln!(
        out,
        "pub(crate) const ADC_SUPPLY_RANGE_MV: (u16,u16) = {:?};",
        a.supply_mv
    )
    .unwrap();
    writeln!(
        out,
        "pub(crate) const ADC_SAMPLE_CYCLES: &[u16] = &{:?};",
        a.sample_cycles
    )
    .unwrap();
    writeln!(
        out,
        "pub(crate) const ADC_COMPARISON_CYCLES: u16 = {};",
        a.comparison_cycles
    )
    .unwrap();
    let bands = |bs: &[cw32_metapac::metadata::PeripheralAdcBand]| {
        bs.iter()
            .map(|b| {
                (
                    b.supply_min_mv,
                    b.maximum_clock_hz,
                    b.maximum_sample_rate_hz,
                    b.minimum_acquisition_ps,
                )
            })
            .collect::<Vec<_>>()
    };
    writeln!(
        out,
        "pub(crate) const ADC_SUPPLY_BANDS: &[(u16,u32,u32,u32)] = &{:?};",
        bands(a.supply_bands)
    )
    .unwrap();
    if !a.internal_1v5_bands.is_empty() {
        writeln!(
            out,
            "pub(crate) const ADC_INTERNAL_1V5_BANDS: &[(u16,u32,u32,u32)] = &{:?};",
            bands(a.internal_1v5_bands)
        )
        .unwrap();
        writeln!(
            out,
            "pub(crate) const ADC_INTERNAL_2V5_BANDS: &[(u16,u32,u32,u32)] = &{:?};",
            bands(a.internal_2v5_bands)
        )
        .unwrap();
    }
    if a.sample_cycles.len() > 4 {
        writeln!(
            out,
            "pub(crate) const ADC_MINIMUM_CLOCK_HZ: u32 = {};",
            a.minimum_clock_hz
        )
        .unwrap();
        writeln!(
            out,
            "pub(crate) const ADC_INTERNAL_ACQUISITION_PS: u64 = {};",
            a.internal_acquisition_ps
        )
        .unwrap();
    } else {
        writeln!(
            out,
            "pub(crate) const ADC_INPUT_FOLLOWER_MAXIMUM_RATE_HZ: u32 = {};",
            a.input_follower_maximum_rate_hz
        )
        .unwrap();
        writeln!(
            out,
            "pub(crate) const ADC_TEMPERATURE_ACQUISITION_MAXIMUM_RATE_HZ: u32 = {};",
            a.temperature_acquisition_maximum_rate_hz
        )
        .unwrap();
    }
    let w = peripherals
        .iter()
        .find_map(|p| p.iwdt_clock.as_ref())
        .expect("missing IWDT clock qualification");
    writeln!(out, "pub(crate) const IWDT_CLOCK: crate::wdg::ClockRange = crate::wdg::ClockRange {{ typical_hz: {}, min_hz: {}, max_hz: {} }};", w.typical_hz, w.minimum_hz, w.maximum_hz).unwrap();
    writeln!(
        out,
        "pub(crate) const IWDT_USES_LSI: bool = {};",
        w.uses_lsi
    )
    .unwrap();
    let i = peripherals
        .iter()
        .find_map(|p| p.i2c_limits.as_ref())
        .expect("missing I2C frequency qualification");
    for p in peripherals.iter().filter_map(|p| p.i2c_limits.as_ref()) {
        assert_eq!(
            p, i,
            "I2C limits differ; use per-instance constants before adding this chip"
        );
    }
    writeln!(
        out,
        "pub(crate) const I2C_MAXIMUM_FREQUENCY_HZ: u32 = {};",
        i.maximum_frequency_hz
    )
    .unwrap();
    if let Some(w) = &i.waveform {
        writeln!(
            out,
            "pub(crate) const I2C_TIMING_MAXIMUM_FREQUENCY_HZ: [u32;3] = {:?};",
            w.maximum_frequency_hz
        )
        .unwrap();
        writeln!(
            out,
            "pub(crate) const I2C_TIMING_LOW: [u32;3] = {:?};",
            w.low
        )
        .unwrap();
        writeln!(
            out,
            "pub(crate) const I2C_TIMING_HIGH: [u32;3] = {:?};",
            w.high
        )
        .unwrap();
        writeln!(
            out,
            "pub(crate) const I2C_TIMING_HOLD: [u32;3] = {:?};",
            w.hold
        )
        .unwrap();
        writeln!(
            out,
            "pub(crate) const I2C_TIMING_SETUP: [u32;3] = {:?};",
            w.setup
        )
        .unwrap();
        writeln!(
            out,
            "pub(crate) const I2C_TIMING_DATA_SETUP: [u32;3] = {:?};",
            w.data_setup
        )
        .unwrap();
        writeln!(
            out,
            "pub(crate) const I2C_TIMING_DATA_VALID: [u32;3] = {:?};",
            w.data_valid
        )
        .unwrap();
        writeln!(
            out,
            "pub(crate) const I2C_TIMING_RISE: [u32;3] = {:?};",
            w.rise
        )
        .unwrap();
    }
    if let Some(f) = peripherals.iter().find_map(|p| p.flash_limits.as_ref()) {
        writeln!(
            out,
            "pub(crate) const FLASH_SUPPLY_RANGE_MV: (u16,u16) = {:?};",
            f.supply_mv
        )
        .unwrap();
        writeln!(
            out,
            "pub(crate) const FLASH_LOCK_GROUP_BYTES: u32 = {};",
            f.lock_group_bytes
        )
        .unwrap();
        writeln!(
            out,
            "pub(crate) const FLASH_LOCK_MASK: u64 = {};",
            f.lock_mask
        )
        .unwrap();
        writeln!(
            out,
            "pub(crate) const FLASH_MAX_HCLK_HZ: u32 = {};",
            f.maximum_hclk_hz
        )
        .unwrap();
        writeln!(
            out,
            "pub(crate) const FLASH_LOW_VOLTAGE_THRESHOLD_MV: u16 = {};",
            f.low_voltage_threshold_mv
        )
        .unwrap();
        writeln!(
            out,
            "pub(crate) const FLASH_LOW_VOLTAGE_MAX_HCLK_HZ: u32 = {};",
            f.low_voltage_maximum_hclk_hz
        )
        .unwrap();
        writeln!(
            out,
            "pub(crate) const FLASH_WAIT_STEP_HZ: u32 = {};",
            f.wait_step_hz
        )
        .unwrap();
        writeln!(
            out,
            "pub(crate) const FLASH_MAX_WAIT_STATES: u32 = {};",
            f.maximum_wait_states
        )
        .unwrap();
    }
}

// Generate the same trait + RCC_INFO boundary used by Embassy STM32. CW32's
// independently verified controller facts remain separate from kernel clocks.
// Project package-qualified pads and use only reviewed native PAC getters.
fn generate_lse(out: &mut String, c: &cw32_metapac::metadata::PeripheralClockLimits) {
    use cw32_metapac::metadata::ir;
    use std::fmt::Write;
    let sysctrl = cw32_metapac::metadata::METADATA
        .peripherals
        .iter()
        .find(|p| p.name == "SYSCTRL")
        .expect("missing SYSCTRL");
    let regs = sysctrl.registers.as_ref().unwrap();
    let block = regs
        .ir
        .blocks
        .iter()
        .find(|b| b.name == regs.block)
        .unwrap();
    let fieldset = |name: &str, offset: u32| {
        let item = block
            .items
            .iter()
            .find(|r| r.name.eq_ignore_ascii_case(name))
            .expect("missing LSE ownership register");
        assert!(item.array.is_none());
        assert_eq!(item.byte_offset, offset);
        let ir::BlockItemInner::Register(register) = &item.inner else {
            panic!("LSE ownership control is not a register")
        };
        assert_eq!(register.bit_size, 32);
        assert!(matches!(
            register.access,
            ir::Access::ReadWrite | ir::Access::Read
        ));
        regs.ir
            .fieldsets
            .iter()
            .find(|f| Some(f.name) == register.fieldset)
            .unwrap()
    };
    let cr1 = fieldset("CR1", 4);
    let lse_register = block
        .items
        .iter()
        .any(|r| r.name.eq_ignore_ascii_case("LSE"));
    assert_eq!(
        c.lse.is_some(),
        lse_register,
        "LSE presence lacks qualified facts"
    );
    let Some(lse) = &c.lse else {
        assert!(
            !cr1.fields
                .iter()
                .any(|f| matches!(f.name, "LSEEN" | "LSELOCK"))
        );
        assert!(
            !sysctrl
                .pins
                .iter()
                .any(|p| matches!(p.signal, "LSE_IN" | "LSE_OUT"))
        );
        out.push_str(
            "pub(crate) const RCC_LSE_PINS: (Option<u8>, Option<u8>) = (None, None);\n\
            pub(crate) fn rcc_lse_owned_pads() -> (bool, bool) { (false, false) }\n",
        );
        return;
    };
    let control = fieldset("LSE", 0x24);
    let field = |fields: &ir::FieldSet, name: &str, offset: u32| {
        let field = fields
            .fields
            .iter()
            .find(|f| f.name.eq_ignore_ascii_case(name))
            .expect("missing native LSE ownership getter");
        assert!(field.array.is_none() && field.enumm.is_none());
        assert_eq!(field.bit_size, 1);
        assert!(matches!(&field.bit_offset, ir::BitOffset::Regular(p) if p.offset == offset));
    };
    field(cr1, "LSEEN", 4);
    field(cr1, "LSELOCK", 5);
    field(control, "MODE", 6);
    assert!(lse.pin_lock || !lse.pin_lock_requires_enable_lock);
    assert_eq!(
        lse.pin_lock,
        control
            .fields
            .iter()
            .any(|f| f.name.eq_ignore_ascii_case("PINLOCK"))
    );
    if lse.pin_lock {
        field(control, "PINLOCK", 17);
    }
    let mut pins = [None; 2];
    for (index, signal) in ["LSE_IN", "LSE_OUT"].iter().enumerate() {
        let found: Vec<_> = sysctrl
            .pins
            .iter()
            .filter(|p| p.signal == *signal)
            .collect();
        assert!(found.len() <= 1, "ambiguous LSE pad");
        if let Some(pin) = found.first() {
            assert!(pin.af.is_none(), "oscillator pad is not a digital AF");
            let port = pin.pin.as_bytes()[1] - b'A';
            let bit: u8 = pin.pin[2..].parse().unwrap();
            assert!(port < 6 && bit < 16);
            pins[index] = Some(port * 16 + bit);
        }
    }
    writeln!(
        out,
        "pub(crate) const RCC_LSE_PINS: (Option<u8>, Option<u8>) = ({:?}, {:?});",
        pins[0], pins[1]
    )
    .unwrap();
    out.push_str(
        "pub(crate) fn rcc_lse_owned_pads() -> (bool, bool) {\n\
        let cr1 = crate::pac::SYSCTRL.cr1().read();\n\
        let lse = crate::pac::SYSCTRL.lse().read();\n",
    );
    let locked = match (lse.pin_lock, lse.pin_lock_requires_enable_lock) {
        (true, true) => "lse.pinlock() && cr1.lselock()",
        (true, false) => "lse.pinlock()",
        (false, false) => "false",
        (false, true) => unreachable!(),
    };
    writeln!(out, "let pad_lock = {locked};").unwrap();
    out.push_str("(cr1.lseen() || pad_lock, (cr1.lseen() && !lse.mode()) || pad_lock)\n}\n");
}

fn generate_pll(out: &mut String, c: &cw32_metapac::metadata::PeripheralClockLimits) {
    use std::fmt::Write;
    let Some(pll) = &c.pll else { return };
    assert!(pll.hsi_supported && c.hse.is_some());
    println!("cargo:rustc-cfg=rcc_pll");
    writeln!(
        out,
        "pub(crate) const RCC_PLL_INPUT_RANGE_HZ: (u32, u32) = {:?};",
        pll.input_range_hz
    )
    .unwrap();
    writeln!(
        out,
        "pub(crate) const RCC_PLL_OUTPUT_RANGE_HZ: (u32, u32) = {:?};",
        pll.output_range_hz
    )
    .unwrap();
    writeln!(
        out,
        "pub(crate) const RCC_PLL_INPUT_BINS_HZ: [(u32, u32); 4] = {:?};",
        pll.input_ranges_hz
    )
    .unwrap();
    writeln!(
        out,
        "pub(crate) const RCC_PLL_OUTPUT_BINS_HZ: [(u32, u32); 5] = {:?};",
        pll.output_ranges_hz
    )
    .unwrap();
    writeln!(
        out,
        "pub(crate) const RCC_PLL_MULTIPLIER_RANGE: (u8, u8) = {:?};",
        pll.multiplier_range
    )
    .unwrap();
    writeln!(
        out,
        "pub(crate) const RCC_PLL_SUPPLY_MV: (u16, u16) = {:?};",
        pll.supply_mv
    )
    .unwrap();
    writeln!(
        out,
        "pub(crate) const RCC_PLL_TEMPERATURE_C: (i16, i16) = {:?};",
        pll.temperature_c
    )
    .unwrap();
    writeln!(
        out,
        "pub(crate) const RCC_PLL_STARTUP_ENCODING: u8 = {:?};",
        pll.startup_encoding
    )
    .unwrap();
    writeln!(
        out,
        "pub(crate) const RCC_PLL_RESERVED_DEBUG_DEFAULT: u8 = {:?};",
        pll.reserved_debug_default
    )
    .unwrap();
}

fn generate_hse(out: &mut String, c: &cw32_metapac::metadata::PeripheralClockLimits) {
    use cw32_metapac::metadata::ir;
    use std::fmt::Write;
    let Some(hse) = &c.hse else { return };
    assert_eq!(hse.crystal.supply_mv, hse.bypass.supply_mv);
    assert_eq!(hse.crystal.temperature_c, hse.bypass.temperature_c);
    assert!(hse.ccs_requires_lsi);
    // HSE initialization also retains qualified HSI. CCS only needs LSI
    // coverage over the board conditions admitted by both source envelopes.
    let supply_mv = (
        hse.crystal.supply_mv.0.max(c.hsi_supply_mv.0),
        hse.crystal.supply_mv.1.min(c.hsi_supply_mv.1),
    );
    let temperature_c = (
        hse.crystal.temperature_c.0.max(c.hsi_temperature_c.0),
        hse.crystal.temperature_c.1.min(c.hsi_temperature_c.1),
    );
    assert!(supply_mv.0 <= supply_mv.1);
    assert!(temperature_c.0 <= temperature_c.1);
    assert!(hse.ccs_lsi_supply_mv.0 <= supply_mv.0 && hse.ccs_lsi_supply_mv.1 >= supply_mv.1);
    assert!(
        hse.ccs_lsi_temperature_c.0 <= temperature_c.0
            && hse.ccs_lsi_temperature_c.1 >= temperature_c.1
    );
    writeln!(
        out,
        "pub(crate) const RCC_HSE_CRYSTAL_RANGE_HZ: (u32,u32) = {:?};",
        (hse.crystal.minimum_hz, hse.crystal.maximum_hz)
    )
    .unwrap();
    writeln!(
        out,
        "pub(crate) const RCC_HSE_BYPASS_RANGE_HZ: (u32,u32) = {:?};",
        (hse.bypass.minimum_hz, hse.bypass.maximum_hz)
    )
    .unwrap();
    writeln!(
        out,
        "pub(crate) const RCC_HSE_SUPPLY_MV: (u16,u16) = {:?};",
        hse.crystal.supply_mv
    )
    .unwrap();
    writeln!(
        out,
        "pub(crate) const RCC_HSE_TEMPERATURE_C: (i16,i16) = {:?};",
        hse.crystal.temperature_c
    )
    .unwrap();
    if let Some(ranges) = hse.frequency_ranges_hz {
        writeln!(
            out,
            "pub(crate) const RCC_HSE_FREQUENCY_RANGES_HZ: [(u32,u32);4] = {:?};",
            ranges
        )
        .unwrap();
    }
    writeln!(
        out,
        "pub(crate) const RCC_HSE_CCS_CYCLE_COUNT: u32 = {};",
        hse.ccs_cycle_count
    )
    .unwrap();
    writeln!(
        out,
        "pub(crate) const RCC_HSE_CCS_NUMERATOR_HZ: u64 = {};",
        hse.ccs_numerator_hz
    )
    .unwrap();
    writeln!(
        out,
        "pub(crate) const RCC_HSE_CCS_MAXIMUM_COUNT: u16 = {};",
        hse.ccs_maximum_count
    )
    .unwrap();
    writeln!(
        out,
        "pub(crate) const RCC_HSE_CCS_LSI_MAXIMUM_HZ: u32 = {};",
        hse.ccs_lsi_maximum_hz
    )
    .unwrap();
    let sysctrl = cw32_metapac::metadata::METADATA
        .peripherals
        .iter()
        .find(|p| p.name == "SYSCTRL")
        .unwrap();
    let regs = sysctrl.registers.as_ref().unwrap();
    if matches!(
        regs.version,
        "cw32l010_v1" | "cw32l011_v1" | "cw32l012_v1" | "cw32l052_v1"
    ) {
        let divisor = hse
            .fixed_ccs_hsi_divisor
            .expect("missing source-qualified fixed CCS divisor");
        assert!(divisor != 0);
        writeln!(
            out,
            "pub(crate) const RCC_FIXED_CCS_HSI_DIVISOR: u32 = {divisor};"
        )
        .unwrap();
    }
    if regs.version == "cw32l012_v1" {
        let range = c
            .hsi_operating_range_hz
            .expect("missing source-qualified incoming HSIOSC range");
        assert!(range.0 > 0 && range.0 <= range.1);
        writeln!(
            out,
            "pub(crate) const RCC_HSI_OPERATING_RANGE_HZ: (u32,u32) = {range:?};"
        )
        .unwrap();
    }
    let hse_register = &regs
        .ir
        .blocks
        .iter()
        .find(|b| b.name == regs.block)
        .unwrap()
        .items
        .iter()
        .find(|r| r.name.eq_ignore_ascii_case("HSE"))
        .expect("missing HSE register")
        .inner;
    let ir::BlockItemInner::Register(hse_register) = hse_register else {
        panic!("HSE control is not a register")
    };
    // The reviewed F020 IP calls the same two-bit selector FREQ. Select the
    // exact own-IP field name, then verify its enum, width and location.
    let range_name = match regs.version {
        "cw32f020_v1" => Some("FREQ"),
        "v1" | "cw32l031_v1" | "cw32l052_v1" | "cw32l083_v1" => Some("FREQRANGE"),
        "cw32l010_v1" | "cw32l011_v1" | "cw32l012_v1" => None,
        _ => panic!("unqualified HSE selector IP"),
    };
    let fields = regs
        .ir
        .fieldsets
        .iter()
        .find(|f| Some(f.name) == hse_register.fieldset)
        .unwrap()
        .fields;
    assert_eq!(
        hse.frequency_ranges_hz.is_some(),
        range_name.is_some(),
        "HSE range data must match the qualified selector IP"
    );
    if let Some(name) = range_name {
        let field = fields
            .iter()
            .find(|f| f.name.eq_ignore_ascii_case(name))
            .expect("missing qualified HSE frequency selector");
        assert!(field.array.is_none());
        assert_eq!(field.bit_size, 2);
        assert!(matches!(&field.bit_offset, ir::BitOffset::Regular(p) if p.offset == 2));
        assert_eq!(field.enumm, Some("HseRange"));
    } else {
        assert!(
            !fields.iter().any(|f| f.name.eq_ignore_ascii_case("FREQ")
                || f.name.eq_ignore_ascii_case("FREQRANGE")
                || f.enumm == Some("HseRange")),
            "unexpected HSE selector on selector-free IP"
        );
    }
    let mut pins = [None; 2];
    for (index, signal) in ["HSE_IN", "HSE_OUT"].iter().enumerate() {
        let found: Vec<_> = sysctrl
            .pins
            .iter()
            .filter(|p| p.signal == *signal)
            .collect();
        assert!(found.len() <= 1, "ambiguous oscillator pad");
        if let Some(pin) = found.first() {
            assert!(
                pin.af.is_none(),
                "oscillator pad must not be represented as a digital AF"
            );
            let port = pin.pin.as_bytes()[1] - b'A';
            let bit: u8 = pin.pin[2..].parse().unwrap();
            pins[index] = Some((port, bit));
        }
    }
    // Only these existing backends inspect retained VC pad ownership during RCC.
    if matches!(regs.version, "cw32l010_v1" | "cw32l011_v1") {
        // Project existing own-source comparator routes onto the exact oscillator
        // pads. Masks are ordered OSC_IN, OSC_OUT; no family/pin literals in RCC.
        for owner in ["VC1", "VC2"] {
            for input in ["INP", "INN"] {
                let mut masks = [0u32; 2];
                if let Some(vc) = cw32_metapac::metadata::METADATA
                    .peripherals
                    .iter()
                    .find(|p| p.name == owner)
                {
                    for route in vc.pins.iter().filter(|p| p.signal.starts_with(input)) {
                        let mux = route.comparator_mux.expect("missing HSE/VC analog mux");
                        let port = route.pin.as_bytes()[1] - b'A';
                        let bit: u8 = route.pin[2..].parse().unwrap();
                        for (index, pin) in pins.iter().enumerate() {
                            if *pin == Some((port, bit)) {
                                masks[index] |= 1u32
                                    .checked_shl(u32::from(mux))
                                    .expect("HSE/VC mux exceeds input mask");
                            }
                        }
                    }
                }
                writeln!(
                    out,
                    "pub(crate) const RCC_HSE_{owner}_{input}_MASKS: [u32;2] = {:?};",
                    masks
                )
                .unwrap();
            }
        }
    }
    // Use the actual pad bank's PAC registers. Some HSE-capable GPIO blocks
    // have neither a lock register nor native high/low-level interrupts.
    let gpio_fields = |port: u8, name: &str| -> Option<&'static [ir::Field]> {
        let gpio = format!("GPIO{}", char::from(b'A' + port));
        let regs = cw32_metapac::metadata::METADATA
            .peripherals
            .iter()
            .find(|p| p.name == gpio)
            .expect("missing HSE GPIO bank")
            .registers
            .as_ref()
            .unwrap();
        let block = regs
            .ir
            .blocks
            .iter()
            .find(|b| b.name == regs.block)
            .unwrap();
        let item = block
            .items
            .iter()
            .find(|r| r.name.eq_ignore_ascii_case(name))?;
        let ir::BlockItemInner::Register(reg) = &item.inner else {
            panic!("HSE pad control is not a register")
        };
        assert!(item.array.is_none());
        assert_eq!(reg.bit_size, 32);
        assert_eq!(reg.access, ir::Access::ReadWrite);
        Some(
            regs.ir
                .fieldsets
                .iter()
                .find(|f| Some(f.name) == reg.fieldset)
                .unwrap()
                .fields,
        )
    };
    let pad_registers = pins.map(|pin| {
        pin.map(|(port, bit)| {
            let gpio = format!("GPIO{}", char::from(b'A' + port));
            let pads = cw32_metapac::metadata::METADATA
                .peripherals
                .iter()
                .find(|p| p.name == gpio)
                .unwrap()
                .gpio
                .as_ref()
                .expect("missing HSE GPIO pad capabilities");
            let mut registers = vec!["dir", "pur"];
            // A generic PDR field does not establish pull-down support on this pad.
            if pads.pull_down_mask & (1 << bit) != 0 {
                assert!(gpio_fields(port, "pdr").is_some(), "missing HSE pad PDR");
                registers.push("pdr");
            }
            registers.extend(["riseie", "fallie"]);
            registers.extend(
                ["highie", "lowie"]
                    .into_iter()
                    .filter(|name| gpio_fields(port, name).is_some()),
            );
            registers.extend(["opendrain", "filter", "analog"]);
            let lock = ["lock", "lckr"]
                .into_iter()
                .find(|name| gpio_fields(port, name).is_some());
            if let Some(lock) = lock {
                registers.push(lock);
                assert!(gpio_fields(port, lock).unwrap().iter().any(|f| {
                    f.name.eq_ignore_ascii_case("key")
                        && f.array.is_none()
                        && f.bit_size == 16
                        && matches!(&f.bit_offset, ir::BitOffset::Regular(p) if p.offset == 16)
                }));
            }
            for name in &registers {
                let fields = gpio_fields(port, name).expect("missing HSE pad control register");
                assert!(
                    fields.iter().any(|f| {
                        f.name.eq_ignore_ascii_case(&format!("pin{bit}"))
                            && f.array.is_none()
                            && f.bit_size == 1
                            && matches!(&f.bit_offset, ir::BitOffset::Regular(p) if p.offset == u32::from(bit))
                    }),
                    "missing HSE pad control field {name}.pin{bit}"
                );
            }
            let afreg = if bit < 8 { "afrl" } else { "afrh" };
            // The locked IR uses AFRn or PINn for the same pad-local mux.
            // Validate its physical field, then emit the actual typed PAC name.
            let af_fields: Vec<_> = gpio_fields(port, afreg)
                .expect("missing HSE pad AF register")
                .iter()
                .filter(|f| {
                    (f.name.eq_ignore_ascii_case(&format!("afr{bit}"))
                        || f.name.eq_ignore_ascii_case(&format!("pin{bit}")))
                        && f.array.is_none()
                        && matches!(f.bit_size, 3 | 4)
                        && matches!(&f.bit_offset, ir::BitOffset::Regular(p) if p.offset == u32::from(bit % 8) * 4)
                })
                .collect();
            assert_eq!(af_fields.len(), 1, "missing or ambiguous HSE pad AF field");
            let af_field = af_fields[0].name.to_ascii_lowercase();
            registers.retain(|name| !matches!(*name, "dir" | "analog" | "lock" | "lckr"));
            (lock, registers, af_field)
        })
    });
    writeln!(
        out,
        "pub(crate) const RCC_HSE_PINS: (Option<u8>,Option<u8>) = {:?};",
        (
            pins[0].map(|(p, b)| p * 16 + b),
            pins[1].map(|(p, b)| p * 16 + b)
        )
    )
    .unwrap();
    // Without a bonded input, neither crystal nor bypass is available.
    if pins[0].is_none() {
        out.push_str("pub(crate) fn rcc_configure_hse_pins(_bypass: bool, _timeout: u32, _cs: critical_section::CriticalSection<'_>) -> Result<(), crate::rcc::Error> {\nErr(crate::rcc::Error::HsePinsUnavailable)\n}\n");
        out.push_str("pub(crate) fn rcc_hse_pins_match(_bypass: bool, _timeout: u32, _cs: critical_section::CriticalSection<'_>) -> Result<bool, crate::rcc::Error> {\nOk(false)\n}\n");
        return;
    }
    out.push_str("pub(crate) fn rcc_configure_hse_pins(bypass: bool, timeout: u32, cs: critical_section::CriticalSection<'_>) -> Result<(), crate::rcc::Error> {\n");
    for (index, pin) in pins.iter().enumerate() {
        if index == 1 {
            out.push_str("if !bypass {\n");
        }
        if let Some((port, bit)) = pin {
            let p = char::from(b'A' + port);
            let (lock, registers, af_field) = pad_registers[index].as_ref().unwrap();
            writeln!(out, "gpio_rcc({port}).enable_with_cs_readback(cs, crate::rcc::Readback::Poll {{ attempts: timeout, spin: true }}).map_err(|_| crate::rcc::Error::HsePinConfigurationTimeout)?;\nlet r = crate::pac::GPIO{p};").unwrap();
            if let Some(lock) = lock {
                writeln!(
                    out,
                    "r.{lock}().modify(|w| {{ w.set_key(0x5a5a); w.set_pin{bit}(false); }});"
                )
                .unwrap();
            }
            // Disable the driver first; then pulls, edge/level IRQs, open drain
            // and filter, and finally the analog/digital input selector. Each
            // RMW preserves unrelated pads and the shared filter-clock field.
            writeln!(out, "r.dir().modify(|w| w.set_pin{bit}(true));").unwrap();
            for reg in registers {
                writeln!(out, "r.{reg}().modify(|w| w.set_pin{bit}(false));").unwrap();
            }
            let afreg = if *bit < 8 { "afrl" } else { "afrh" };
            writeln!(out, "r.{afreg}().modify(|w| w.set_{af_field}(0));").unwrap();
            writeln!(
                out,
                "r.analog().modify(|w| w.set_pin{bit}({}));",
                if index == 0 { "!bypass" } else { "true" }
            )
            .unwrap();
        } else {
            out.push_str("return Err(crate::rcc::Error::HsePinsUnavailable);\n");
        }
        if index == 1 {
            out.push_str("}\n");
        }
    }
    out.push_str("if !rcc_hse_pins_match(bypass, timeout, cs)? { return Err(crate::rcc::Error::HsePinConfigurationTimeout); }\nOk(())\n}\n");
    out.push_str("pub(crate) fn rcc_hse_pins_match(bypass: bool, timeout: u32, cs: critical_section::CriticalSection<'_>) -> Result<bool, crate::rcc::Error> {\nlet mut matches = true;\n");
    for (index, pin) in pins.iter().enumerate() {
        if index == 1 {
            out.push_str("if !bypass {\n");
        }
        if let Some((port, bit)) = pin {
            let p = char::from(b'A' + port);
            let (_, registers, af_field) = pad_registers[index].as_ref().unwrap();
            writeln!(out, "matches &= gpio_rcc({port}).inspect_for_init(cs, timeout, || {{\nlet r = crate::pac::GPIO{p};").unwrap();
            let afreg = if *bit < 8 { "afrl" } else { "afrh" };
            writeln!(out, "r.dir().read().pin{bit}() && r.analog().read().pin{bit}() == {} && r.{afreg}().read().{af_field}() == 0", if index == 0 { "!bypass" } else { "true" }).unwrap();
            for reg in registers {
                writeln!(out, "&& !r.{reg}().read().pin{bit}()").unwrap();
            }
            out.push_str("}).map_err(|_| crate::rcc::Error::HsePinConfigurationTimeout)?;\n");
        } else {
            out.push_str("return Ok(false);\n");
        }
        if index == 1 {
            out.push_str("}\n");
        }
    }
    out.push_str("Ok(matches)\n}\n");
}

// Direct digital HEX pads have their own F002/F003 projection. Their GPIO
// block has no LOCK register; do not reuse the crystal/HSE pad programming.
fn generate_hex(out: &mut String, c: &cw32_metapac::metadata::PeripheralClockLimits) {
    use std::fmt::Write;
    let Some(hex) = &c.hex else { return };
    writeln!(
        out,
        "pub(crate) const RCC_HEX_RANGE_HZ: (u32, u32) = {:?};",
        (hex.input.minimum_hz, hex.input.maximum_hz)
    )
    .unwrap();
    writeln!(
        out,
        "pub(crate) const RCC_HEX_SUPPLY_MV: (u16, u16) = {:?};",
        hex.input.supply_mv
    )
    .unwrap();
    writeln!(
        out,
        "pub(crate) const RCC_HEX_TEMPERATURE_C: (i16, i16) = {:?};",
        hex.input.temperature_c
    )
    .unwrap();
    let sysctrl = cw32_metapac::metadata::METADATA
        .peripherals
        .iter()
        .find(|p| p.name == "SYSCTRL")
        .unwrap();
    let mut pins = [None; 2];
    for (index, signal) in ["HEX_PB00", "HEX_PB01"].iter().enumerate() {
        let found: Vec<_> = sysctrl
            .pins
            .iter()
            .filter(|p| p.signal == *signal)
            .collect();
        assert!(found.len() <= 1, "ambiguous HEX pad");
        if let Some(pin) = found.first() {
            assert!(
                pin.af.is_none(),
                "HEX input must not be a numbered digital AF"
            );
            assert_eq!(pin.pin, format!("PB{index}"));
            pins[index] = Some(16 + index as u8);
        }
    }
    writeln!(
        out,
        "pub(crate) const RCC_HEX_PINS: [Option<u8>; 2] = {:?};",
        pins
    )
    .unwrap();
    out.push_str("pub(crate) fn rcc_configure_hex_pin(input: crate::rcc::HexInput, timeout: u32, cs: critical_section::CriticalSection<'_>) -> Result<(), crate::rcc::Error> {\nmatch input {\n");
    for (index, pin) in pins.iter().enumerate() {
        writeln!(out, "crate::rcc::HexInput::Pb{index} => {{").unwrap();
        if pin.is_some() {
            out.push_str("gpio_rcc(1).enable_with_cs_readback(cs, crate::rcc::Readback::Poll { attempts: timeout, spin: true }).map_err(|_| crate::rcc::Error::HexPinConfigurationTimeout)?;\nlet r = crate::pac::GPIOB;\n");
            // Disable output first, then pad-local features, then digital input.
            // RMW preserves every unrelated pad and the shared filter clock.
            for (reg, value) in [
                ("dir", true),
                ("pur", false),
                ("pdr", false),
                ("riseie", false),
                ("fallie", false),
                ("highie", false),
                ("lowie", false),
                ("opendrain", false),
                ("filter", false),
            ] {
                writeln!(out, "r.{reg}().modify(|w| w.set_pin{index}({value}));").unwrap();
            }
            writeln!(out, "r.afrl().modify(|w| w.set_afr{index}(0));\nr.analog().modify(|w| w.set_pin{index}(false));").unwrap();
        } else {
            out.push_str("return Err(crate::rcc::Error::HexPinUnavailable);\n");
        }
        out.push_str("}\n");
    }
    out.push_str("}\nfor _ in 0..timeout { if rcc_hex_pin_matches(input, timeout, cs)? { return Ok(()); } core::hint::spin_loop(); }\nErr(crate::rcc::Error::HexPinConfigurationTimeout)\n}\n");
    out.push_str("pub(crate) fn rcc_hex_pin_matches(input: crate::rcc::HexInput, timeout: u32, cs: critical_section::CriticalSection<'_>) -> Result<bool, crate::rcc::Error> {\nmatch input {\n");
    for (index, pin) in pins.iter().enumerate() {
        writeln!(out, "crate::rcc::HexInput::Pb{index} => {{").unwrap();
        if pin.is_some() {
            out.push_str(
                "gpio_rcc(1).inspect_for_init(cs, timeout, || { let r = crate::pac::GPIOB;\n",
            );
            writeln!(out, "r.dir().read().pin{index}() && !r.analog().read().pin{index}() && r.afrl().read().afr{index}() == 0").unwrap();
            for reg in [
                "pur",
                "pdr",
                "riseie",
                "fallie",
                "highie",
                "lowie",
                "opendrain",
                "filter",
            ] {
                writeln!(out, "&& !r.{reg}().read().pin{index}()").unwrap();
            }
            out.push_str("}).map_err(|_| crate::rcc::Error::HexPinConfigurationTimeout)\n");
        } else {
            out.push_str("Ok(false)\n");
        }
        out.push_str("}\n");
    }
    out.push_str("}\n}\n");
}

fn generate_rcc(out: &mut String, time_driver: Option<&str>) {
    use cw32_metapac::metadata::{METADATA, PeripheralRccKernelClock};
    use std::collections::BTreeMap;
    let sysctrl = METADATA
        .peripherals
        .iter()
        .find(|p| p.name == "SYSCTRL")
        .unwrap()
        .registers
        .as_ref()
        .unwrap();
    let mut enable_users = BTreeMap::new();
    let mut reset_users = BTreeMap::new();
    for peripheral in METADATA.peripherals {
        if let Some(control) = &peripheral.rcc_control {
            *enable_users
                .entry((control.enable.register, control.enable.field))
                .or_insert(0usize) += 1;
            if let Some(reset) = &control.reset {
                *reset_users
                    .entry((reset.register, reset.field))
                    .or_insert(0usize) += 1;
            }
        }
    }
    for peripheral in METADATA.peripherals {
        let Some(control) = &peripheral.rcc_control else {
            continue;
        };
        assert_eq!(control.controller, "SYSCTRL");
        let (enable_offset, enable_bit, width) = rcc_field(sysctrl, &control.enable);
        assert_eq!(width, 1);
        let reset = control.reset.as_ref().map(|reset| {
            let (offset, bit, width) = rcc_field(sysctrl, reset);
            assert_eq!(width, 1);
            (offset, bit)
        });
        assert_eq!(reset.is_some(), control.reset_asserted_value.is_some());
        let key = control.enable_write_key.as_ref().map(|key| {
            assert!(
                key.field
                    .register
                    .eq_ignore_ascii_case(control.enable.register)
            );
            let (offset, bit, width) = rcc_field(sysctrl, &key.field);
            assert_eq!(offset, enable_offset);
            assert!(u32::from(bit) + width <= 32);
            assert!(width == 32 || key.value < (1u32 << width));
            let mask = (u32::MAX >> (32 - width)) << bit;
            assert_eq!(mask & (1 << enable_bit), 0);
            (mask, key.value << bit)
        });
        if let Some(rcc) = &peripheral.rcc {
            let enable = rcc.enable.as_ref().unwrap();
            assert!(
                enable
                    .register
                    .eq_ignore_ascii_case(control.enable.register)
            );
            assert!(enable.field.eq_ignore_ascii_case(control.enable.field));
            assert_eq!(
                rcc.reset.as_ref().map(|r| rcc_field(sysctrl, r)),
                control.reset.as_ref().map(|r| rcc_field(sysctrl, r))
            );
        }
        let registers = peripheral.registers.as_ref().unwrap();
        let kind = registers.kind;
        let version = registers.version;
        // Readback and lifetime behavior are HAL policy, not invented hardware
        // facts. Retain original per-driver limits and error behavior.
        let policy_kind = if peripheral.name.starts_with("GPIO") {
            "gpio"
        } else {
            kind
        };
        let (enable_readback, disable_readback, reset_readback) = match policy_kind {
            "uart" => (
                "Poll { attempts: 100_000, spin: false }",
                "Poll { attempts: 100_000, spin: false }",
                false,
            ),
            "spi" => ("Poll { attempts: 100_000, spin: true }", "None", true),
            "i2c" if version == "cw32l012_v1" => {
                ("Poll { attempts: 32, spin: true }", "Once", true)
            }
            "i2c" => ("Poll { attempts: 100_000, spin: true }", "None", true),
            "gtim" | "atim" => ("Once", "Once", true),
            "gpio" if matches!(version, "cw32l010_v1" | "cw32l011_v1") => {
                ("Poll { attempts: 100_000, spin: true }", "None", false)
            }
            "adc" => ("Once", "Once", true),
            _ => ("Once", "None", false),
        };
        let independently_owned = matches!(
            kind,
            "uart" | "spi" | "i2c" | "gtim" | "atim" | "adc" | "lptim" | "halltim" | "autotrim"
        );
        let low_adc =
            kind == "adc" && matches!(version, "cw32l010_v1" | "cw32l011_v1" | "cw32l012_v1");
        let shared_enable = control.shared_enable_group.is_some()
            || enable_users[&(control.enable.register, control.enable.field)] > 1;
        let shared_reset = control.shared_reset_group.is_some()
            || control
                .reset
                .as_ref()
                .is_some_and(|r| reset_users[&(r.register, r.field)] > 1);
        // Shared reset/clock groups may contain bootloader or analog owners
        // outside HAL accounting. A first/last-user refcount is insufficient.
        let allow_reset = independently_owned && !low_adc && !shared_reset;
        let allow_disable = (independently_owned
            || matches!(kind, "crc" | "cordic" | "eau" | "aes" | "trng"))
            && !shared_enable;
        let bus = match control.bus_clock {
            "PCLK" => "pclk",
            "HCLK" => "hclk",
            other => panic!("unqualified RCC bus {other}"),
        };
        let kernel = match peripheral.rcc.as_ref().map(|rcc| &rcc.kernel_clock) {
            Some(PeripheralRccKernelClock::Clock("PCLK")) => Some("pclk"),
            Some(PeripheralRccKernelClock::Clock("HCLK")) => Some("hclk"),
            _ => None,
        };
        let frequency = kernel
            .map(|clock| format!("crate::rcc::try_clocks().map(|c| c.{clock})"))
            .unwrap_or_else(|| "None".into());
        let kernel_bounds = kernel
            .map(|clock| format!("crate::rcc::try_clocks().map(|c| c.{clock}_bounds())"))
            .unwrap_or_else(|| "None".into());
        let name = if Some(peripheral.name) == time_driver {
            "TimeDriverPeripheral".to_string()
        } else {
            format!("peripherals::{}", peripheral.name)
        };
        out.push_str(&format!(
            "impl crate::rcc::SealedRccPeripheral for {name} {{\n\
             const RCC_INFO: crate::rcc::RccInfo = unsafe {{ crate::rcc::RccInfo::new(\n\
             ({enable_offset}, {enable_bit}), {reset:?}, {key:?}, {}, {},\n\
             crate::rcc::RccPolicy {{ enable_readback: crate::rcc::Readback::{enable_readback},\n\
             disable_readback: crate::rcc::Readback::{disable_readback}, reset_readback: {reset_readback},\n\
             allow_reset: {allow_reset}, allow_disable: {allow_disable} }}) }};\n\
             fn bus_frequency() -> Option<crate::time::Hertz> {{ crate::rcc::try_clocks().map(|c| c.{bus}) }}\n\
             fn bus_clock_bounds() -> Option<crate::rcc::ClockBounds> {{ crate::rcc::try_clocks().map(|c| c.{bus}_bounds()) }}\n\
             fn frequency() -> Option<crate::time::Hertz> {{ {frequency} }}\n\
             fn kernel_clock_bounds() -> Option<crate::rcc::ClockBounds> {{ {kernel_bounds} }}\n\
             }}\nimpl crate::rcc::RccPeripheral for {name} {{}}\n",
            control.enable_active_value, control.reset_asserted_value.unwrap_or(false),
        ));
    }
}

fn rcc_field(
    sysctrl: &cw32_metapac::metadata::PeripheralRegisters,
    reference: &cw32_metapac::metadata::PeripheralRccRegister,
) -> (u8, u8, u32) {
    use cw32_metapac::metadata::ir;
    let block = sysctrl
        .ir
        .blocks
        .iter()
        .find(|b| b.name == sysctrl.block)
        .unwrap();
    let item = block
        .items
        .iter()
        .find(|r| r.name.eq_ignore_ascii_case(reference.register))
        .unwrap();
    assert!(item.array.is_none());
    assert_eq!(item.byte_offset % 4, 0);
    let ir::BlockItemInner::Register(register) = &item.inner else {
        panic!("RCC control is not a register")
    };
    assert_eq!(register.access, ir::Access::ReadWrite);
    assert_eq!(register.bit_size, 32);
    let fields = sysctrl
        .ir
        .fieldsets
        .iter()
        .find(|f| Some(f.name) == register.fieldset)
        .unwrap();
    let field = fields
        .fields
        .iter()
        .find(|f| f.name.eq_ignore_ascii_case(reference.field))
        .unwrap();
    assert!(field.array.is_none());
    let ir::BitOffset::Regular(position) = &field.bit_offset else {
        panic!("nonlinear RCC field")
    };
    assert!(field.bit_size > 0 && position.offset + field.bit_size <= 32);
    (
        (item.byte_offset / 4).try_into().unwrap(),
        position.offset.try_into().unwrap(),
        field.bit_size,
    )
}

// Inclusive Q1.31 bounds are rounded inward by data-gen from own-manual rationals.
fn generate_accelerator_domains() {
    let facts = cw32_metapac::metadata::METADATA
        .peripherals
        .iter()
        .find(|p| p.name == "CORDIC")
        .unwrap()
        .cordic
        .as_ref()
        .expect("missing CORDIC numeric domains");
    let mut out = String::new();
    for domain in facts.domains {
        let (name, minimum, maximum) = (domain.name, domain.minimum, domain.maximum);
        out.push_str(&format!(
            "pub(super) const {name}: (i32, i32) = ({minimum}, {maximum});\n"
        ));
    }
    fs::write(
        PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("_cordic_domains.rs"),
        out,
    )
    .unwrap();
}

// The extra IR token, where needed, represents SYSCTRL.IRMOD itself. It is
// generated from reviewed real-register metadata, not a fabricated peripheral.
fn generate_lvd_ir(out: &mut String) {
    use cw32_metapac::metadata::METADATA;
    let lvd = METADATA
        .peripherals
        .iter()
        .find(|p| p.name == "LVD")
        .unwrap();
    let facts = lvd
        .lvd
        .as_ref()
        .expect("missing LVD own-family qualification");
    let version = lvd.registers.as_ref().unwrap().version;
    assert!(matches!(version, "v1" | "cw32l031_v1" | "cw32l010_v1"));
    if version == "cw32l010_v1" {
        println!("cargo:rustc-cfg=lvd_low");
    }
    out.push_str(&format!(
        "pub(crate) const LVD_THRESHOLDS_MV: &[u16] = &{:?};\n",
        facts.thresholds_mv
    ));
    out.push_str(&format!(
        "pub(crate) const LVD_SUPPLY_NAME: &str = {:?};\n",
        facts.supply_name
    ));
    for input in facts.inputs {
        if safe_pin(input.pin) {
            out.push_str(&format!(
                "crate::lvd::impl_input_pin!({}, {});\n",
                input.pin, input.selector
            ));
        }
    }
    let p = METADATA
        .peripherals
        .iter()
        .find(|p| p.ir.is_some())
        .expect("missing IR subfunction");
    let ir = p.ir.as_ref().unwrap();
    if p.name == "SYSCTRL" {
        println!("cargo:rustc-cfg=ir_sysctrl");
    }
    if ir.mode_configurable {
        println!("cargo:rustc-cfg=ir_modes");
    }
    if ir.software_control {
        println!("cargo:rustc-cfg=ir_software");
    }
    if ir.invert {
        println!("cargo:rustc-cfg=ir_invert");
    }
    for pin in p
        .pins
        .iter()
        .filter(|p| p.signal == "IR_OUT" && safe_pin(p.pin))
    {
        out.push_str(&format!(
            "crate::ir::impl_output_pin!({}, {});\n",
            pin.pin,
            pin.af.unwrap()
        ));
    }
}

// Own-L083 source-backed geometry; typed commands remain in the authored PAC IR.
fn generate_crypto_facts() {
    let mut out = String::new();
    for name in ["AES", "TRNG"] {
        let peripheral = cw32_metapac::metadata::METADATA
            .peripherals
            .iter()
            .find(|p| p.name == name)
            .unwrap();
        let fields: Vec<(&str, u8)> = if name == "AES" {
            let facts = peripheral.aes.as_ref().expect("missing AES geometry");
            vec![
                ("block_words", facts.block_words),
                ("key_words_128", facts.key_words_128),
                ("key_words_192", facts.key_words_192),
                ("key_words_256", facts.key_words_256),
            ]
        } else {
            let facts = peripheral.trng.as_ref().expect("missing TRNG geometry");
            vec![("output_words", facts.output_words)]
        };
        for (field, value) in fields {
            out.push_str(&format!(
                "pub const {}_{}: usize = {value};\n",
                name,
                field.to_ascii_uppercase()
            ));
        }
    }
    fs::write(
        PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("_crypto.rs"),
        out,
    )
    .unwrap();
}

// Emit only the meaningful register-layout capability; no synthetic RAM gate.
fn configure_ram() {
    use cw32_metapac::metadata::METADATA;
    println!("cargo:rustc-check-cfg=cfg(ram_enable_status)");
    let ram = METADATA
        .peripherals
        .iter()
        .find(|p| p.name == "RAM")
        .unwrap();
    let regs = ram.registers.as_ref().unwrap();
    assert!(ram.rcc.is_none() && ram.rcc_control.is_none());
    let fields = regs
        .ir
        .fieldsets
        .iter()
        .find(|f| f.name == "Ier")
        .unwrap()
        .fields;
    let has_en = fields.iter().any(|f| f.name == "en");
    assert_eq!(
        has_en,
        ram.ram_parity
            .as_ref()
            .expect("missing RAM parity facts")
            .enable_status
    );
    if has_en {
        println!("cargo:rustc-cfg=ram_enable_status");
    }
}

// Derive LCD register access and pin routes from reviewed chip facts and typed IR.
fn generate_lcd(out: &mut String) {
    use cw32_metapac::metadata::{METADATA, ir};
    use std::fmt::Write;
    let Some(p) = METADATA.peripherals.iter().find(|p| p.name == "LCD") else {
        return;
    };
    let facts = p.lcd.as_ref().unwrap();
    let regs = p.registers.as_ref().unwrap();
    writeln!(out, "impl crate::lcd::sealed::Instance for crate::peripherals::LCD {{ const RAM_REGISTERS: &'static [u8] = &{:?}; const LSI_TYPICAL_HZ: u32 = {}; }} impl crate::lcd::Instance for crate::peripherals::LCD {{}}", facts.ram_registers, facts.lsi_typical_hz).unwrap();
    for pin in p.pins.iter().filter(|pin| safe_pin(pin.pin)) {
        if let Some(n) = pin.signal.strip_prefix("COM") {
            writeln!(out,"impl crate::lcd::sealed::ComPin<crate::peripherals::LCD, {n}> for crate::peripherals::{} {{}} impl crate::lcd::ComPin<crate::peripherals::LCD, {n}> for crate::peripherals::{} {{}}",pin.pin,pin.pin).unwrap();
        } else if let Some(n) = pin.signal.strip_prefix("SEG") {
            writeln!(out,"impl crate::lcd::sealed::SegPin<crate::peripherals::LCD, {n}> for crate::peripherals::{} {{}} impl crate::lcd::SegPin<crate::peripherals::LCD, {n}> for crate::peripherals::{} {{}}",pin.pin,pin.pin).unwrap();
        }
    }
    let block = regs
        .ir
        .blocks
        .iter()
        .find(|b| b.name == regs.block)
        .unwrap();
    let clear = block.items.iter().find(|r| r.byte_offset == 8).unwrap();
    let ir::BlockItemInner::Register(clear_reg) = &clear.inner else {
        panic!("LCD event clear must be register")
    };
    let clear_type = clear_reg.fieldset.unwrap();
    assert!(matches!(clear_type, "Icr" | "Intclr"));
    writeln!(out, "pub(crate) fn lcd_clear_frame() {{ let mut command=crate::pac::lcd::regs::{clear_type}::write_noop();command.set_intf(false);crate::pac::LCD.{}().write_value(command); }}",clear.name.to_ascii_lowercase()).unwrap();
    out.push_str("pub(crate) fn lcd_write_word(index: u8, pixels: u32) { match index {\n");
    for n in facts.ram_registers {
        let name = format!("RAM{n}");
        let fieldset = regs
            .ir
            .fieldsets
            .iter()
            .find(|f| f.name.eq_ignore_ascii_case(&name))
            .unwrap();
        assert_eq!(fieldset.fields.len(), 32);
        writeln!(out, "{n} => crate::pac::LCD.ram{n}().write(|w| {{").unwrap();
        for f in fieldset.fields {
            assert_eq!(f.bit_size, 1);
            let ir::BitOffset::Regular(ref offset) = f.bit_offset else {
                panic!("LCD pixel must have regular offset")
            };
            writeln!(
                out,
                "w.set_{}(pixels & (1u32 << {}) != 0);",
                f.name.to_ascii_lowercase(),
                offset.offset
            )
            .unwrap();
        }
        out.push_str("}),\n");
    }
    out.push_str("_ => unreachable!(\"unsupported LCD RAM register\"), } }\n");
    out.push_str(
        "pub(crate) fn lcd_enable_pin(is_com: bool, number: u8) { match (is_com, number) {\n",
    );
    for (is_com, number, register, bit) in facts
        .segments
        .iter()
        .map(|n| {
            (
                false,
                *n,
                if *n < 32 { "PINEN1" } else { "PINEN2" },
                u32::from(*n % 32),
            )
        })
        .chain((0..8u8).map(|n| {
            (
                true,
                n,
                if n < 4 { "PINEN2" } else { "PINEN1" },
                u32::from(if n < 4 { 24 + n } else { 35 - n }),
            )
        }))
    {
        let fieldset = regs
            .ir
            .fieldsets
            .iter()
            .find(|f| f.name.eq_ignore_ascii_case(register))
            .unwrap();
        let f=fieldset.fields.iter().find(|f|matches!(f.bit_offset,ir::BitOffset::Regular(ref o) if o.offset<=bit && o.offset+f.bit_size>bit)).expect("LCD pin bit missing");
        let ir::BitOffset::Regular(ref offset) = f.bit_offset else {
            unreachable!()
        };
        let field = f.name.to_ascii_lowercase();
        let value = if f.bit_size == 1 {
            "true".to_owned()
        } else {
            format!("w.{field}() | (1 << {})", bit - offset.offset)
        };
        writeln!(
            out,
            "({is_com}, {number}) => crate::pac::LCD.{}().modify(|w| w.set_{field}({value})),",
            register.to_ascii_lowercase()
        )
        .unwrap();
    }
    out.push_str("_ => unreachable!(\"unsupported LCD pin\"), } }\n");
}

fn generate_rtc(out: &mut String) {
    use cw32_metapac::metadata::METADATA;
    use std::fmt::Write;
    let Some(peripheral) = METADATA.peripherals.iter().find(|p| p.name == "RTC") else {
        return;
    };
    let f = peripheral.rtc_calendar.as_ref().expect("RTC source facts");
    writeln!(out, "impl crate::rtc::sealed::Instance for crate::peripherals::RTC {{ const SOURCE: u8 = {}; const SOURCE_NOMINAL_HZ: u32 = {}; const SOURCE_MINIMUM_HZ: u32 = {}; const SOURCE_MAXIMUM_HZ: u32 = {}; const TEMPERATURE_C: (i16,i16) = {:?}; const SUPPLY_MV: (u16,u16) = {:?}; #[cfg(not(any(rtc_cw32l010_v1,rtc_cw32l011_v1,rtc_cw32l012_v1)))] const FACTORY_TRIM_ADDRESS: usize = {}; const CALENDAR_DIVISOR: u32 = {}; #[cfg(any(rtc_cw32l010_v1,rtc_cw32l011_v1,rtc_cw32l012_v1))] const PRESCALER_FIRST: u16 = {}; #[cfg(any(rtc_cw32l010_v1,rtc_cw32l011_v1,rtc_cw32l012_v1))] const PRESCALER_SECOND: u32 = {}; }}", f.source_encoding,f.nominal_hz,f.minimum_hz,f.maximum_hz,f.temperature_c,f.supply_mv,f.factory_trim_address,f.calendar_divisor,f.prescaler_first,f.prescaler_second).unwrap();
}

// Project source-event mappings into destination-local types, never a global ITR enum.
fn generate_trigger_routes() {
    use cw32_metapac::metadata::{METADATA, TriggerRegister};
    let routes: Vec<_> = METADATA
        .peripherals
        .iter()
        .flat_map(|p| p.triggers.iter().map(move |route| (p, route)))
        .collect();
    if routes.is_empty() {
        return;
    }
    let adc_version = match METADATA.line {
        "CW32L010" => "cw32l010_v1",
        "CW32L011" => "cw32l011_v1",
        _ => panic!("unqualified trigger family"),
    };
    assert_eq!(routes.len(), 2, "unqualified trigger set");
    let field = |actual: &TriggerRegister, owner, reg, name, role, offset, size, value| {
        assert_eq!(
            (
                actual.peripheral,
                actual.register,
                actual.field,
                actual.role,
                actual.bit_offset,
                actual.bit_size,
                actual.value
            ),
            (owner, reg, name, role, offset, size, value)
        );
    };
    let mut timer = None;
    let mut adc = None;
    for (destination, route) in routes {
        assert_eq!(
            destination.registers.as_ref().unwrap().version,
            if destination.name == "ADC" {
                adc_version
            } else {
                "cw32l010_v1"
            }
        );
        assert_eq!(route.source, "BTIM1_TRGO");
        assert_eq!(route.registers.len(), 2);
        field(
            &route.registers[0],
            "BTIM1",
            "CR2",
            "MMS",
            "source_event",
            4,
            3,
            2,
        );
        let source = METADATA
            .peripherals
            .iter()
            .find(|p| p.name == "BTIM1")
            .unwrap();
        assert_eq!(source.registers.as_ref().unwrap().version, "cw32l010_v1");
        match destination.name {
            "BTIM2" => {
                assert!(timer.is_none(), "duplicate BTIM2 route");
                assert_eq!(route.signal, "TRGI");
                field(
                    &route.registers[1],
                    "BTIM2",
                    "SMCR",
                    "TRGISRC",
                    "selector",
                    7,
                    4,
                    8,
                );
                timer = Some(format!(
                    "pub(super) const BTIM1_UPDATE_MMS: u8 = {};\n\
                     /// Qualified BTIM2 TRGI source; values belong only to BTIM2.\n\
                     #[derive(Clone, Copy, Debug, Eq, PartialEq)]\n\
                     #[repr(u8)]\n\
                     pub enum Btim2TriggerInput {{\n\
                     /// BTIM1 UPDATE emitted through TRGO.\n\
                     Btim1Update = {},\n}}\n",
                    route.registers[0].value, route.registers[1].value
                ));
            }
            "ADC" => {
                assert!(adc.is_none(), "duplicate ADC route");
                assert_eq!(route.signal, "START_CONVERSION");
                field(
                    &route.registers[1],
                    "ADC",
                    "TRIGGER",
                    "BTIM1TRGO",
                    "independent_enable",
                    13,
                    1,
                    1,
                );
                adc = Some(
                    "/// Qualified ADC independent trigger enable.\n\
                    #[derive(Clone, Copy, Debug, Eq, PartialEq)]\n\
                    pub enum TriggerInput {\n\
                    /// BTIM1 UPDATE emitted through TRGO.\n\
                    Btim1Update,\n}\n",
                );
            }
            _ => panic!("unqualified trigger destination"),
        }
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    fs::write(
        out.join("timer_trigger_routes.rs"),
        timer.expect("missing BTIM2 mapping"),
    )
    .unwrap();
    fs::write(
        out.join("adc_trigger_routes.rs"),
        adc.expect("missing ADC mapping"),
    )
    .unwrap();
    println!("cargo:rustc-cfg=trigger_btim1_update");
}

// Select an actual independently owned timer and vector from generated metadata.
fn select_time_driver() -> Option<&'static str> {
    let gtim = env::var_os("CARGO_FEATURE_TIME_DRIVER_GTIM").is_some();
    let gtim1 = env::var_os("CARGO_FEATURE_TIME_DRIVER_GTIM1").is_some();
    let enabled = env::var_os("CARGO_FEATURE__TIME_DRIVER").is_some();
    assert!(
        !(gtim && gtim1),
        "select exactly one time-driver-gtim or time-driver-gtim1 feature"
    );
    assert!(
        !enabled || gtim || gtim1,
        "_time-driver requires an explicit timer selector"
    );
    let name = if gtim {
        "GTIM"
    } else if gtim1 {
        "GTIM1"
    } else {
        return None;
    };
    let metadata = &cw32_metapac::metadata::METADATA;
    let p = metadata
        .peripherals
        .iter()
        .find(|p| p.name == name)
        .unwrap_or_else(|| panic!("selected time driver {name} is absent on {}", metadata.line));
    let regs = p.registers.as_ref().unwrap();
    assert_eq!(regs.kind, "gtim");
    assert!(matches!(
        regs.version,
        "v1" | "cw32f002_v1" | "cw32l010_v1" | "cw32l012_v1" | "cw32l031_v1" | "cw32l052_v1"
    ));
    let irq = p
        .interrupts
        .iter()
        .find(|i| i.signal == "GLOBAL")
        .expect("time driver has no GLOBAL IRQ");
    assert!(
        !metadata.peripherals.iter().any(|other| other.name != name
            && other
                .interrupts
                .iter()
                .any(|i| i.interrupt == irq.interrupt)),
        "time-driver IRQ must not be shared"
    );
    let control = p
        .rcc_control
        .as_ref()
        .expect("time driver has no qualified RCC control");
    assert_eq!(control.bus_clock, "PCLK");
    assert!(control.shared_enable_group.is_none() && control.shared_reset_group.is_none());
    Some(name)
}

fn generate_time_driver(out: &mut String, selected: Option<&str>) {
    let Some(name) = selected else { return };
    let metadata = &cw32_metapac::metadata::METADATA;
    let p = metadata
        .peripherals
        .iter()
        .find(|p| p.name == name)
        .unwrap();
    let irq = p
        .interrupts
        .iter()
        .find(|i| i.signal == "GLOBAL")
        .unwrap()
        .interrupt;
    let number = metadata
        .interrupts
        .iter()
        .find(|i| i.name == irq)
        .unwrap()
        .number;
    out.push_str(&format!(
        "mod time_driver_resource {{ pub(crate) enum {name} {{}} }}\n\
         pub(crate) type TimeDriverPeripheral = time_driver_resource::{name};\n\
         pub(crate) use crate::pac::{name} as TIME_DRIVER_REGS;\n\
         pub(crate) type TimeDriverInterrupt = crate::interrupt::typelevel::{irq};\n\
         const _: () = assert!(crate::pac::Interrupt::{irq} as u16 == {number});\n\
         mod time_driver_vector {{\n\
         #[unsafe(no_mangle)] unsafe extern \"C\" fn {irq}() {{ crate::time_driver::on_interrupt(); }}\n\
         }}\n"
    ));
}
