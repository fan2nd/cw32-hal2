use std::{env, fs, path::PathBuf};

fn main() {
    let metadata = &cw32_metapac::metadata::METADATA;
    assert_eq!(metadata.memory.len(), 1, "select one exact package");
    assert!(metadata.peripherals.iter().any(|p| {
        p.clock_limits
            .as_ref()
            .is_some_and(|c| c.lsi_sysclk.is_some())
    }));
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
