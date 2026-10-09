//! Own-L012 Hall capture pins; never infer routes from a compatible register map.
use super::*;

const ROUTES: &[(&str, &str, u32)] = &[
    ("PA3", "CH1", 666),
    ("PA4", "CH2", 676),
    ("PA5", "CH3", 686),
    ("PB2", "CH1", 815),
    ("PB5", "CH1", 845),
    ("PB6", "CH2", 855),
    ("PB7", "CH3", 865),
    ("PB10", "CH2", 893),
    ("PB11", "CH3", 903),
    ("PB13", "CH1", 923),
    ("PB14", "CH2", 933),
    ("PB15", "CH3", 943),
];

fn qualify(source: &Profile, profile: &str) -> Result<()> {
    ensure!(
        profile == "CW32L012" && source.profile == profile,
        "unreviewed Hall timer family"
    );
    ensure!(
        source.datasheet["sha256"]
            == "08a605fe8843d1eefa2b1115474b5a74df2833068f0ad6f35225e4f39377da76",
        "Hall pin datasheet changed"
    );
    ensure!(
        source.routes.len() == ROUTES.len(),
        "Hall input route count changed"
    );
    for (route, &(pin, signal, line)) in source.routes.iter().zip(ROUTES) {
        let number: u32 = pin[2..].parse()?;
        let source_macro = format!("P{}{:02}_AFx_HALLTIM{signal}", &pin[1..2], number);
        let cell = route
            .datasheet_cell
            .as_ref()
            .context("Hall pin table cell missing")?;
        ensure!(
            route.pin == pin
                && route.signal == signal
                && route.peripheral == "HALLTIM"
                && route.af == 9
                && route.source_line == Some(line)
                && route.source_macro.as_deref() == Some(source_macro.as_str())
                && route.source_kind.as_deref() == Some("sdk-and-datasheet")
                && cell.function == format!("HALLTIM_{signal}")
                && cell.pdf_page == 39
                && cell.printed_page == 36
                && cell.table == if pin.starts_with("PA") { "5-3" } else { "5-4" },
            "unqualified Hall input route"
        );
    }
    Ok(())
}

/// Project twelve reviewed AF9 input candidates through the selected package.
pub fn apply(
    root: &Path,
    path: &str,
    profile: &str,
    core: &mut chip::Core,
    registers: &BTreeMap<String, ir::IR>,
) -> Result<usize> {
    ensure!(
        path == "cw32-data/af/cw32l012-halltim.yaml",
        "unreviewed Hall route sidecar"
    );
    let source: Profile = crate::read_yaml(root.join(path))?;
    qualify(&source, profile)?;
    let peripheral = core
        .peripherals
        .iter()
        .find(|p| p.name == "HALLTIM")
        .context("Hall instance missing")?;
    let r = peripheral
        .registers
        .as_ref()
        .context("Hall registers missing")?;
    ensure!(
        r.kind == "halltim"
            && r.version == "cw32l012_v1"
            && r.block == "HALLTIM"
            && peripheral.address == 0x40006400,
        "unreviewed Hall instance/register variant"
    );
    let clock = peripheral
        .rcc
        .as_ref()
        .context("Hall clock identity missing")?;
    let control = peripheral
        .rcc_control
        .as_ref()
        .context("Hall RCC control missing")?;
    ensure!(
        clock.kernel_clock == chip::core::peripheral::rcc::KernelClock::Clock("PCLK".into())
            && control.enable.register == "APBEN2"
            && control.enable.field == "HALLTIM"
            && control
                .reset
                .as_ref()
                .is_some_and(|f| f.register == "APBRST2" && f.field == "HALLTIM")
            && control.reset_asserted_value == Some(false)
            && control.shared_enable_group.is_none()
            && control.shared_reset_group.is_none(),
        "unqualified Hall clock/reset ownership"
    );
    let ir = registers.get("halltim").context("Hall IR missing")?;
    let cr = &ir.fieldsets["CR"];
    for (name, offset, bits, enumm) in [
        ("DIV", 2, 2, Some("Prescaler")),
        ("MMS", 4, 3, Some("MasterMode")),
        ("FLT2LEN", 8, 15, None),
    ] {
        let f = cr
            .fields
            .iter()
            .find(|f| f.name == name)
            .context("Hall control field missing")?;
        ensure!(
            f.bit_offset == ir::BitOffset::Regular(offset)
                && f.bit_size == bits
                && f.enumm.as_deref() == enumm,
            "unqualified Hall control semantics"
        );
    }
    super::project(&source, profile, core, registers)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn source() -> Profile {
        serde_yaml::from_str(include_str!("../../../cw32-data/af/cw32l012-halltim.yaml")).unwrap()
    }
    #[test]
    fn rejects_foreign_family_and_route_inference() {
        for family in ["CW32L010", "CW32L011", "CW32F030", "CW32L083"] {
            assert!(qualify(&source(), family).is_err());
        }
        let mut s = source();
        s.routes[0].af = 8;
        assert!(qualify(&s, "CW32L012").is_err());
        let mut s = source();
        s.routes[0].signal = "CH2".into();
        assert!(qualify(&s, "CW32L012").is_err());
        let mut s = source();
        s.routes[0].datasheet_cell.as_mut().unwrap().pdf_page = 40;
        assert!(qualify(&s, "CW32L012").is_err());
    }
    #[test]
    fn exact_routes_and_optional_input_contract() {
        qualify(&source(), "CW32L012").unwrap();
        let mut value: serde_json::Value =
            serde_yaml::from_str(include_str!("../../../cw32-data/inputs/cw32l012.yaml")).unwrap();
        let input: crate::Input = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(
            input.halltim_metadata.as_deref(),
            Some("cw32-data/af/cw32l012-halltim.yaml")
        );
        value.as_object_mut().unwrap().remove("halltim_metadata");
        let input: crate::Input = serde_json::from_value(value).unwrap();
        assert!(input.halltim_metadata.is_none());
    }
}
