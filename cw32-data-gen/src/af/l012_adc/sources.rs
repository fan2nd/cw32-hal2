//! Source identities are independent of the editable analog sidecar.
pub(super) const SOURCES: &[(&str, &str, &str)] = &[
    (
        "datasheet",
        "CW32L012_DataSheet_CN_V1.0.pdf",
        "08a605fe8843d1eefa2b1115474b5a74df2833068f0ad6f35225e4f39377da76",
    ),
    (
        "reference_manual",
        "CW32L012_UserManual_CN_V1.4.pdf",
        "a9e54694a26f03c1f3e3041f40844900168ca2b8e6142f3328671b92e6b7a340",
    ),
    (
        "sdk_archive",
        "CW32L012_StandardPeripheralLib_V1.0.5.zip",
        "8b0a4c0ee865642d08f353aec98906c900a8b10f672120e649e6b1bf5e29c18f",
    ),
    (
        "adc_header",
        "cw32l012/Libraries/inc/cw32l012_adc.h",
        "13a189fef19ca32b64c6fa289c2e144069e8961ed4cf72bdbe62cd190266b045",
    ),
    (
        "pinouts",
        "cw32-data/pinouts/cw32l012.yaml",
        "70ad225317acd9b1bd51a19fff15f6834f3efb4f58563b33344ab5d69eab07fb",
    ),
];

// Instance, mux0..11 pins, first SDK pin-comment line. DAC mux12/13 are internal.
pub(super) const ROUTES: [(&str, [&str; 12], usize); 2] = [
    (
        "ADC1",
        [
            "PA0", "PA1", "PA2", "PA3", "PA4", "PA5", "PA6", "PA7", "PB0", "PB1", "PB10", "PB2",
        ],
        187,
    ),
    (
        "ADC2",
        [
            "PA5", "PA6", "PA7", "PB0", "PB1", "PA8", "PA9", "PA10", "PA11", "PA12", "PB10", "PB2",
        ],
        205,
    ),
];

pub(super) fn source(key: &str) -> (&'static str, &'static str) {
    let &(_, file, hash) = SOURCES.iter().find(|s| s.0 == key).unwrap();
    (file, hash)
}
