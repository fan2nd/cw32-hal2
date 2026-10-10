use std::{env, fs, path::PathBuf};

fn main() {
    let metadata = &cw32_metapac::metadata::METADATA;
    assert_eq!(metadata.memory.len(), 1, "select one exact package");
    assert!(matches!(
        metadata.name,
        "CW32F002F3P7" | "CW32F002F3U7" | "CW32F020C6U7" | "CW32F030C8T7" | "CW32A030C8T7"
    ));
    assert!(metadata.peripherals.iter().any(|p| {
        p.clock_limits
            .as_ref()
            .is_some_and(|c| c.lsi_sysclk.is_some())
    }));
    if matches!(metadata.name, "CW32F002F3P7" | "CW32F002F3U7") {
        assert!(!metadata.peripherals.iter().any(|p| p.name == "RTC"));
        // Own F002 DS Rev1.2 tables 3-1 and 6-1: retain real linker limits.
        let banks = metadata.memory[0];
        assert_eq!(banks.len(), 2);
        let flash = banks.iter().find(|bank| bank.name == "FLASH").unwrap();
        let ram = banks.iter().find(|bank| bank.name == "RAM").unwrap();
        assert_eq!((flash.address, flash.size), (0x0000_0000, 16 * 1024));
        assert_eq!((ram.address, ram.size), (0x2000_0000, 2 * 1024));
    }
    let mut memory = String::from("MEMORY {\n");
    for bank in metadata.memory[0] {
        assert!(matches!(bank.name, "FLASH" | "RAM"));
        memory.push_str(&format!(
            "  {} : ORIGIN = {:#x}, LENGTH = {}\n",
            bank.name, bank.address, bank.size
        ));
    }
    memory.push_str("}\n");
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    fs::write(out.join("memory.x"), memory).unwrap();
    println!("cargo:rustc-link-search={}", out.display());
    println!("cargo:rustc-link-arg=-Tlink.x");
    println!("cargo:rerun-if-changed=build.rs");
}
