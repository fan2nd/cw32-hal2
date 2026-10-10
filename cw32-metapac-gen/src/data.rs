// Adapted from embassy-rs/stm32-data at 37a22f31552ba1fd29b3ef192c4578b84abee6e1.
// SPDX-License-Identifier: MIT OR Apache-2.0
use cw32_data_macros::EnumDebug;
use serde::Deserialize;

pub mod ir {
    use super::*;

    #[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
    pub struct IR {
        pub blocks: Vec<Block>,
        pub fieldsets: Vec<FieldSet>,
        pub enums: Vec<Enum>,
    }

    impl IR {
        pub fn from_chiptool(ir: chiptool::ir::IR) -> Self {
            let mut blocks: Vec<Block> = ir
                .blocks
                .iter()
                .map(|(name, block)| {
                    let items = block
                        .items
                        .iter()
                        .map(|item| BlockItem {
                            name: item.name.clone(),
                            description: item.description.clone(),
                            array: item.array.as_ref().map(|array| match &array {
                                chiptool::ir::Array::Regular(regular_array) => {
                                    Array::Regular(RegularArray {
                                        len: regular_array.len,
                                        stride: regular_array.stride,
                                    })
                                }
                                chiptool::ir::Array::Cursed(cursed_array) => {
                                    Array::Cursed(CursedArray {
                                        offsets: cursed_array.offsets.clone(),
                                    })
                                }
                            }),
                            byte_offset: item.byte_offset,
                            inner: match &item.inner {
                                chiptool::ir::BlockItemInner::Block(block) => {
                                    BlockItemInner::Block(BlockItemBlock {
                                        block: block.block.clone(),
                                    })
                                }
                                chiptool::ir::BlockItemInner::Register(register) => {
                                    BlockItemInner::Register(Register {
                                        access: match register.access {
                                            chiptool::ir::Access::Read => Access::Read,
                                            chiptool::ir::Access::ReadWrite => Access::ReadWrite,
                                            chiptool::ir::Access::Write => Access::Write,
                                        },
                                        bit_size: register.bit_size,
                                        fieldset: register.fieldset.as_ref().map(|fieldset| {
                                            fieldset.strip_prefix("regs::").unwrap().to_string()
                                        }),
                                    })
                                }
                            },
                        })
                        .collect();

                    #[allow(clippy::redundant_field_names)]
                    Block {
                        name: name.to_string(),
                        items: items,
                        extends: block.extends.clone(),
                        description: block.description.clone(),
                    }
                })
                .collect();

            blocks.sort_by_key(|b| b.name.clone());

            let mut fieldsets: Vec<FieldSet> = ir
                .fieldsets
                .iter()
                .map(|(name, fieldset)| {
                    let fields = fieldset
                        .fields
                        .iter()
                        .map(|field| Field {
                            name: field.name.clone(),
                            description: field.description.clone(),
                            bit_offset: match &field.bit_offset {
                                chiptool::ir::BitOffset::Regular(offset) => {
                                    BitOffset::Regular(RegularBitOffset { offset: *offset })
                                }
                                chiptool::ir::BitOffset::Cursed(ranges) => {
                                    BitOffset::Cursed(CursedBitOffset {
                                        ranges: ranges.clone(),
                                    })
                                }
                            },
                            bit_size: field.bit_size,
                            array: field.array.as_ref().map(|array| match &array {
                                chiptool::ir::Array::Regular(regular_array) => {
                                    Array::Regular(RegularArray {
                                        len: regular_array.len,
                                        stride: regular_array.stride,
                                    })
                                }
                                chiptool::ir::Array::Cursed(cursed_array) => {
                                    Array::Cursed(CursedArray {
                                        offsets: cursed_array.offsets.clone(),
                                    })
                                }
                            }),
                            enumm: field.enumm.as_ref().map(|fieldset| {
                                fieldset.strip_prefix("vals::").unwrap().to_string()
                            }),
                        })
                        .collect();

                    #[allow(clippy::redundant_field_names)]
                    FieldSet {
                        name: name.strip_prefix("regs::").unwrap().to_owned(),
                        fields: fields,
                        extends: fieldset.extends.clone(),
                        description: fieldset.description.clone(),
                        bit_size: fieldset.bit_size,
                    }
                })
                .collect();

            fieldsets.sort_by_key(|f| f.name.clone());

            let mut enums: Vec<Enum> = ir
                .enums
                .iter()
                .map(|(name, enumm)| {
                    let variants = enumm
                        .variants
                        .iter()
                        .map(|variant| EnumVariant {
                            name: variant.name.clone(),
                            description: variant.description.clone(),
                            value: variant.value,
                        })
                        .collect();

                    #[allow(clippy::redundant_field_names)]
                    Enum {
                        name: name.strip_prefix("vals::").unwrap().to_owned(),
                        description: enumm.description.clone(),
                        bit_size: enumm.bit_size,
                        variants: variants,
                    }
                })
                .collect();

            enums.sort_by_key(|e| e.name.clone());

            Self {
                blocks,
                fieldsets,
                enums,
            }
        }
    }

    #[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
    pub struct Block {
        pub name: String,
        pub extends: Option<String>,

        pub description: Option<String>,
        pub items: Vec<BlockItem>,
    }

    // Notice:
    // BlockItem has custom Debug implement,
    // when modify the struct, make sure Debug impl reflect the change.
    #[derive(Eq, PartialEq, Clone, Deserialize)]
    pub struct BlockItem {
        pub name: String,
        pub description: Option<String>,

        pub array: Option<Array>,
        pub byte_offset: u32,

        pub inner: BlockItemInner,
    }

    // Notice:
    // Debug implement AFFECT OUTPUT METAPAC, modify with caution
    impl std::fmt::Debug for BlockItem {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("BlockItem")
                .field("name", &self.name)
                .field("description", &self.description)
                .field("array", &self.array)
                .field("byte_offset", &format_args!("{:#x}", self.byte_offset))
                .field("inner", &self.inner)
                .finish()
        }
    }

    #[derive(EnumDebug, Eq, PartialEq, Clone, Deserialize)]
    pub enum BlockItemInner {
        Block(BlockItemBlock),
        Register(Register),
    }

    #[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
    pub struct Register {
        pub access: Access,
        pub bit_size: u32,
        pub fieldset: Option<String>,
    }

    #[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
    pub struct BlockItemBlock {
        pub block: String,
    }

    #[derive(EnumDebug, Eq, PartialEq, Clone, Deserialize)]
    pub enum Access {
        ReadWrite,
        Read,
        Write,
    }

    #[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
    pub struct FieldSet {
        pub name: String,
        pub extends: Option<String>,

        pub description: Option<String>,
        pub bit_size: u32,
        pub fields: Vec<Field>,
    }

    #[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
    pub struct Field {
        pub name: String,
        pub description: Option<String>,

        pub bit_offset: BitOffset,
        pub bit_size: u32,
        pub array: Option<Array>,
        pub enumm: Option<String>,
    }

    #[derive(EnumDebug, Eq, PartialEq, Clone, Deserialize)]
    pub enum Array {
        Regular(RegularArray),
        Cursed(CursedArray),
    }

    #[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
    pub struct RegularArray {
        pub len: u32,
        pub stride: u32,
    }

    #[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
    pub struct CursedArray {
        pub offsets: Vec<u32>,
    }

    #[derive(EnumDebug, Eq, PartialEq, Clone, Deserialize)]
    pub enum BitOffset {
        Regular(RegularBitOffset),
        Cursed(CursedBitOffset),
    }

    #[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
    pub struct RegularBitOffset {
        pub offset: u32,
    }

    #[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
    pub struct CursedBitOffset {
        pub ranges: Vec<core::ops::RangeInclusive<u32>>,
    }

    #[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
    pub struct Enum {
        pub name: String,
        pub description: Option<String>,
        pub bit_size: u32,
        pub variants: Vec<EnumVariant>,
    }

    #[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
    pub struct EnumVariant {
        pub name: String,
        pub description: Option<String>,
        pub value: u64,
    }
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
pub struct Chip {
    pub name: String,
    pub family: String,
    pub line: String,
    pub cores: Vec<Core>,
    pub memory: Vec<Vec<MemoryRegion>>,
    pub packages: Vec<Package>,
}

// Notice:
// MemoryRegion has custom Debug implement,
// when modify the struct, make sure Debug impl reflect the change.
#[derive(Eq, PartialEq, Clone, Deserialize)]
pub struct MemoryRegion {
    pub name: String,
    pub kind: MemoryRegionKind,
    pub address: u32,
    pub size: u32,
    pub settings: Option<FlashSettings>,
}

// Notice:
// Debug implement AFFECT OUTPUT METAPAC, modify with caution
impl std::fmt::Debug for MemoryRegion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MemoryRegion")
            .field("name", &self.name)
            .field("kind", &self.kind)
            .field("address", &format_args!("{:#x}", self.address))
            .field("size", &self.size)
            .field("settings", &self.settings)
            .finish()
    }
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
pub struct FlashSettings {
    pub erase_size: u32,
    pub write_size: u32,
    pub erase_value: u8,
}

#[derive(EnumDebug, Eq, PartialEq, Clone, Deserialize)]
pub enum MemoryRegionKind {
    #[serde(rename = "flash")]
    Flash,
    #[serde(rename = "ram")]
    Ram,
    #[serde(rename = "eeprom")]
    Eeprom,
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
pub struct Core {
    pub name: String,
    pub peripherals: Vec<Peripheral>,
    #[serde(default)]
    pub nvic_priority_bits: Option<u8>,
    pub interrupts: Vec<Interrupt>,
    pub dma_channels: Vec<DmaChannel>,
    pub pins: Vec<Pin>,
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
pub struct Interrupt {
    pub name: String,
    pub number: u32,
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
pub struct Package {
    pub name: String,
    pub package: String,
}

// Notice:
// Peripheral has custom Debug implement,
// when modify struct, make sure Debug impl reflect the change.
#[derive(Eq, PartialEq, Clone, Deserialize)]
pub struct Peripheral {
    pub name: String,
    pub address: u64,
    #[serde(default)]
    pub registers: Option<PeripheralRegisters>,
    #[serde(default)]
    pub rcc: Option<PeripheralRcc>,
    #[serde(default)]
    pub rcc_control: Option<PeripheralRccControl>,
    #[serde(default)]
    pub spi: Option<PeripheralSpi>,
    pub classic_timer_input: Option<PeripheralClassicTimerInput>,
    #[serde(default)]
    pub atim_complementary: Option<PeripheralAtimComplementary>,
    #[serde(default)]
    pub lvd: Option<PeripheralLvd>,
    pub lcd: Option<PeripheralLcd>,
    pub rtc_calendar: Option<PeripheralRtcCalendar>,
    #[serde(default)]
    pub rtc_alarms: Option<PeripheralRtcAlarms>,
    #[serde(default)]
    pub ir: Option<PeripheralIr>,
    #[serde(default)]
    pub gpio_interrupt: Option<PeripheralGpioInterrupt>,
    #[serde(default)]
    pub gpio: Option<PeripheralGpio>,
    #[serde(default)]
    pub cordic: Option<PeripheralCordic>,
    #[serde(default)]
    pub aes: Option<PeripheralAes>,
    #[serde(default)]
    pub trng: Option<PeripheralTrng>,
    #[serde(default)]
    pub ram_parity: Option<PeripheralRamParity>,
    #[serde(default)]
    pub clock_limits: Option<PeripheralClockLimits>,
    #[serde(default)]
    pub adc_limits: Option<PeripheralAdcLimits>,
    #[serde(default)]
    pub comparator_limits: Option<PeripheralComparatorLimits>,
    #[serde(default)]
    pub reference_divider: Option<PeripheralReferenceDivider>,
    #[serde(default)]
    pub dac_limits: Option<PeripheralDacLimits>,
    #[serde(default)]
    pub opa_limits: Option<PeripheralOpaLimits>,
    #[serde(default)]
    pub iwdt_clock: Option<PeripheralIwdtClock>,
    #[serde(default)]
    pub i2c_limits: Option<PeripheralI2cLimits>,
    #[serde(default)]
    pub flash_limits: Option<PeripheralFlashLimits>,
    #[serde(default)]
    pub pins: Vec<PeripheralPin>,
    #[serde(default)]
    pub dma_channels: Vec<PeripheralDmaChannel>,
    #[serde(default)]
    pub triggers: Vec<PeripheralTrigger>,
    #[serde(default)]
    pub interrupts: Vec<PeripheralInterrupt>,
    #[serde(default)]
    pub afio: Option<PeripheralAfio>,
}

// Notice:
// Debug implement AFFECT OUTPUT METAPAC, modify with caution
impl std::fmt::Debug for Peripheral {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Peripheral")
            .field("name", &self.name)
            .field("address", &format_args!("{:#x}", self.address))
            .field("registers", &self.registers)
            .field("rcc", &self.rcc)
            .field("rcc_control", &self.rcc_control)
            .field("spi", &self.spi)
            .field("classic_timer_input", &self.classic_timer_input)
            .field("atim_complementary", &self.atim_complementary)
            .field("lvd", &self.lvd)
            .field("lcd", &self.lcd)
            .field("rtc_calendar", &self.rtc_calendar)
            .field("rtc_alarms", &self.rtc_alarms)
            .field("ir", &self.ir)
            .field("gpio_interrupt", &self.gpio_interrupt)
            .field("gpio", &self.gpio)
            .field("cordic", &self.cordic)
            .field("aes", &self.aes)
            .field("trng", &self.trng)
            .field("ram_parity", &self.ram_parity)
            .field("clock_limits", &self.clock_limits)
            .field("adc_limits", &self.adc_limits)
            .field("comparator_limits", &self.comparator_limits)
            .field("reference_divider", &self.reference_divider)
            .field("dac_limits", &self.dac_limits)
            .field("opa_limits", &self.opa_limits)
            .field("iwdt_clock", &self.iwdt_clock)
            .field("i2c_limits", &self.i2c_limits)
            .field("flash_limits", &self.flash_limits)
            .field("pins", &self.pins)
            .field("dma_channels", &self.dma_channels)
            .field("triggers", &self.triggers)
            .field("interrupts", &self.interrupts)
            .field("afio", &self.afio)
            .finish()
    }
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
pub struct PeripheralInterrupt {
    pub signal: String,
    pub interrupt: String,
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
pub struct PeripheralAfio {
    pub register: String,
    pub field: String,
    #[serde(default)]
    pub values: Vec<PeripheralAfioValue>,
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
pub struct PeripheralAfioValue {
    pub value: u8,
    #[serde(default)]
    pub pins: Vec<String>,
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
pub struct PeripheralGpioInterrupt {
    pub serviced_mask: u16,
    pub clear_noop_mask: u16,
    pub level_trigger: bool,
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
pub struct PeripheralRccControl {
    pub controller: String,
    pub bus_clock: String,
    pub enable: PeripheralRccRegister,
    pub enable_active_value: bool,
    #[serde(default)]
    pub enable_write_key: Option<PeripheralRccWriteKey>,
    #[serde(default)]
    pub reset: Option<PeripheralRccRegister>,
    #[serde(default)]
    pub reset_asserted_value: Option<bool>,
    #[serde(default)]
    pub shared_enable_group: Option<String>,
    #[serde(default)]
    pub shared_reset_group: Option<String>,
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
pub struct PeripheralRccWriteKey {
    pub field: PeripheralRccRegister,
    pub value: u32,
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
pub struct PeripheralClassicTimerInput {
    pub capture_mux: String,
    pub encoder_fixed_reload: Option<u16>,
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
pub struct PeripheralSpi {
    pub maximum_frequency: u32,
    pub minimum_divisor: u16,
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
pub struct PeripheralRcc {
    pub bus_clock: String,
    pub kernel_clock: PeripheralRccKernelClock,
    #[serde(default)]
    pub enable: Option<PeripheralRccRegister>,
    #[serde(default)]
    pub reset: Option<PeripheralRccRegister>,
    #[serde(default)]
    pub stop_mode: StopMode,
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
#[serde(untagged)]
pub enum PeripheralRccKernelClock {
    Clock(String),
    Mux(PeripheralRccRegister),
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
pub struct PeripheralRccRegister {
    pub register: String,
    pub field: String,
}

#[derive(EnumDebug, Eq, PartialEq, Clone, Deserialize, Default)]
pub enum StopMode {
    #[default]
    Stop1, // Peripheral prevents chip from entering Stop1
    Stop2,   // Peripheral prevents chip from entering Stop2
    Standby, // Peripheral does not prevent chip from entering Stop
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
pub struct PeripheralPin {
    pub pin: String,
    pub signal: String,
    pub af: Option<u8>,
    /// Hardware ADC mux encoding; independent of the source signal label.
    pub adc_mux: Option<u8>,
    /// Source-qualified hardware input mux, independent of signal numbering.
    pub comparator_mux: Option<u8>,
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
pub struct Pin {
    pub name: String,
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
pub struct DmaChannel {
    pub name: String,
    pub dma: String,
    pub channel: u32,
    pub dmamux: Option<String>,
    pub dmamux_channel: Option<u32>,
    #[serde(default)]
    pub supports_2d: Option<bool>,
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize, Hash)]
pub struct PeripheralDmaChannel {
    pub signal: String,
    pub channel: Option<String>,
    pub dmamux: Option<String>,
    #[serde(default)]
    pub remap: Vec<RemapInfo>,
    pub dma: Option<String>,
    pub request: Option<u32>,
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize, Hash)]
pub struct PeripheralTrigger {
    pub signal: String,
    pub source: String,
    #[serde(default)]
    pub registers: Vec<TriggerRegister>,
}
// Reuse the project-authored schema; do not introduce a second owned model.
pub use cw32_data_serde::chip::core::peripheral::TriggerRegister;

#[derive(Debug, Eq, PartialEq, Clone, Deserialize, Hash)]
pub struct RemapInfo {
    pub register: String,
    pub field: String,
    pub value: u8,
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize, Hash)]
pub struct PeripheralRegisters {
    pub kind: String,
    pub version: String,
    pub block: String,
    #[serde(default)]
    pub ir: String,
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
pub struct PeripheralExternalClockLimits {
    pub minimum_hz: u32,
    pub maximum_hz: u32,
    pub supply_mv: (u16, u16),
    pub temperature_c: (i16, i16),
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
pub struct PeripheralHseLimits {
    pub crystal: PeripheralExternalClockLimits,
    pub bypass: PeripheralExternalClockLimits,
    pub filter_maximum_hz: u32,
    pub startup_cycles: [u32; 4],
    /// Hardware frequency-range selector bins; absent when the selector does not exist.
    #[serde(default)]
    pub frequency_ranges_hz: Option<[(u32, u32); 4]>,
    /// Fixed HSIOSC divisor imposed by CCS, only when explicitly source-qualified.
    #[serde(default)]
    pub fixed_ccs_hsi_divisor: Option<u8>,
    pub ccs_cycle_count: u32,
    pub ccs_numerator_hz: u64,
    pub ccs_maximum_count: u16,
    pub ccs_lsi_maximum_hz: u32,
    pub ccs_lsi_supply_mv: (u16, u16),
    pub ccs_lsi_temperature_c: (i16, i16),
    pub ccs_requires_lsi: bool,
    pub bypass_duty_percent: (u8, u8),
    pub bypass_minimum_high_low_ns: u16,
    pub bypass_maximum_rise_fall_ns: u16,
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
/// Direct HEX input limits; no crystal, detector or automatic fallback is implied.
pub struct PeripheralHexLimits {
    pub input: PeripheralExternalClockLimits,
    pub duty_percent: (u8, u8),
    pub minimum_high_low_ns: u16,
    pub maximum_rise_fall_ns: u16,
    /// Percent of VDDIOx relative to VSS.
    pub high_level_vddio_percent: (u8, u8),
    pub low_level_vddio_percent: (u8, u8),
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
/// Inherited LSE pad ownership; does not qualify LSE configuration.
pub struct PeripheralLse {
    /// Whether the oscillator provides a separate pad lock.
    pub pin_lock: bool,
    /// Whether the pad lock requires the oscillator enable lock.
    pub pin_lock_requires_enable_lock: bool,
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PeripheralLseRtcReset {
    pub register: String,
    pub byte_offset: u32,
    pub value: u32,
    pub mask: u32,
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PeripheralLseOutputRoute {
    pub pin: String,
    pub af: u8,
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PeripheralLseWorkGatedConsumer {
    pub source: u8,
    pub gate_controls_work: bool,
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PeripheralLseStartupConsumers {
    pub startup_analog: bool,
    pub autotrim_source: u8,
    pub lptim: PeripheralLseWorkGatedConsumer,
    pub lcd: PeripheralLseWorkGatedConsumer,
    pub uarts: Vec<String>,
    pub lsi_output_routes: Vec<PeripheralLseOutputRoute>,
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
/// Exact-family native low-power drive, detector and RTC facts. No amplitude field exists.
pub struct PeripheralLseNativeLowPower {
    pub drive_bits: u8,
    pub startup_drive_bits: u8,
    /// Qualified unchanged LSI reference upper bound; STABLE alone does not prove it.
    pub monitored_lsi_maximum_hz: u32,
    pub detector_lse_edges: u16,
    pub detector_lsi_cycles: u16,
    /// Engineering phase margin, separate from the hardware edge threshold.
    pub detector_margin_lse_edges: u16,
    /// Exact qualification: inherited_legal or factory_trim, checked per family.
    pub monitor_reference: String,
    /// Own-source LSI halfword address; required for factory_trim, absent otherwise.
    pub lsi_factory_trim_address: Option<u32>,
    /// Actual divisors; PSC stores each divisor minus one.
    pub rtc_first_divisor: u16,
    pub rtc_second_divisor: u16,
    pub rtc_calendar_divisor: u32,
    pub rtc_output_routes: Vec<PeripheralLseOutputRoute>,
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PeripheralLseConfiguration {
    pub nominal_hz: u32,
    pub maximum_hz: u32,
    /// True when CLKCCS/HSECCS/LSECCS are configurable hardware controls.
    pub configurable_ccs: bool,
    pub supply_mv: (u16, u16),
    pub temperature_c: (i16, i16),
    pub startup_cycles: [u32; 4],
    pub rtc_source: u8,
    pub uart_source: u8,
    pub awt_source: Option<u8>,
    pub startup_consumers: Option<PeripheralLseStartupConsumers>,
    pub native_low_power: Option<PeripheralLseNativeLowPower>,
    pub mco_source: u8,
    pub gpio_dir_offset: u32,
    pub gpio_speed_offset: Option<u32>,
    pub rtc_reset: Vec<PeripheralLseRtcReset>,
    pub output_routes: Vec<PeripheralLseOutputRoute>,
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
pub struct PeripheralPllLimits {
    pub input_range_hz: (u32, u32),
    pub output_range_hz: (u32, u32),
    pub input_ranges_hz: [(u32, u32); 4],
    pub output_ranges_hz: [(u32, u32); 5],
    pub multiplier_range: (u8, u8),
    pub supply_mv: (u16, u16),
    pub temperature_c: (i16, i16),
    pub hsi_supported: bool,
    pub startup_cycles: u32,
    pub startup_encoding: u8,
    pub reserved_debug_default: u8,
    pub cycle_to_cycle_jitter_ps: u32,
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
pub struct PeripheralClockLimits {
    pub hsi_frequency_hz: u32,
    pub hsi_error_percent: u32,
    pub hsi_supply_mv: (u16, u16),
    pub hsi_temperature_c: (i16, i16),
    pub low_voltage_threshold_mv: u16,
    pub low_voltage_bus_max_hz: u32,
    pub high_voltage_bus_max_hz: u32,
    pub factory_hsi_trim_address: u32,
    /// Required legal incoming HSIOSC range; not an arbitrary-TRIM frequency guarantee.
    #[serde(default)]
    pub hsi_operating_range_hz: Option<(u32, u32)>,
    pub default_hsi_divisor: u8,
    pub initial_flash_wait: u32,
    pub flash_wait_step_hz: u32,
    #[serde(default)]
    pub hse: Option<PeripheralHseLimits>,
    #[serde(default)]
    pub hex: Option<PeripheralHexLimits>,
    #[serde(default)]
    pub lse: Option<PeripheralLse>,
    #[serde(default)]
    pub lse_configuration: Option<PeripheralLseConfiguration>,
    #[serde(default)]
    pub pll: Option<PeripheralPllLimits>,
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
pub struct PeripheralAdcBand {
    pub supply_min_mv: u16,
    pub maximum_clock_hz: u32,
    pub maximum_sample_rate_hz: u32,
    pub minimum_acquisition_ps: u32,
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
pub struct PeripheralAdcSequence {
    pub maximum_length: u8,
    pub programmable_order: bool,
    pub per_slot_sample_time: bool,
    pub per_slot_result: bool,
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
pub struct PeripheralClassicAdcScan {
    pub slots_per_sequence_register: u8,
    pub buffered_requires_single_channel: bool,
    pub internal_requires_single_channel: bool,
    pub first_internal_channel: u8,
    pub supply_channel: u8,
    pub temperature_channel: Option<u8>,
    pub bandgap_channel: Option<u8>,
    /// Conservative software guard, not a characterized maximum.
    pub temperature_startup_us: u32,
    /// Conservative software guard for the approximate BGR startup.
    pub bandgap_startup_us: u32,
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
pub struct PeripheralAdcLimits {
    pub supply_mv: (u16, u16),
    pub minimum_clock_hz: u32,
    pub sample_cycles: Vec<u16>,
    pub comparison_cycles: u16,
    pub supply_bands: Vec<PeripheralAdcBand>,
    pub internal_1v5_bands: Vec<PeripheralAdcBand>,
    pub internal_2v5_bands: Vec<PeripheralAdcBand>,
    pub input_follower_maximum_rate_hz: u32,
    pub temperature_acquisition_maximum_rate_hz: u32,
    pub internal_acquisition_ps: u64,
    #[serde(default)]
    pub sequence: Option<PeripheralAdcSequence>,
    #[serde(default)]
    pub classic_scan: Option<PeripheralClassicAdcScan>,
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
pub struct PeripheralIwdtClock {
    pub typical_hz: u32,
    pub minimum_hz: u32,
    pub maximum_hz: u32,
    pub uses_lsi: bool,
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
pub struct PeripheralI2cLimits {
    pub maximum_frequency_hz: u32,
    pub waveform: Option<PeripheralI2cWaveform>,
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
pub struct PeripheralFlashLimits {
    pub supply_mv: (u16, u16),
    pub lock_group_bytes: u32,
    pub lock_mask: u64,
    pub has_cache_control: bool,
    pub maximum_hclk_hz: u32,
    pub low_voltage_threshold_mv: u16,
    pub low_voltage_maximum_hclk_hz: u32,
    pub wait_step_hz: u32,
    pub maximum_wait_states: u32,
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
pub struct PeripheralI2cWaveform {
    pub maximum_frequency_hz: [u32; 3],
    pub low: [u32; 3],
    pub high: [u32; 3],
    pub hold: [u32; 3],
    pub setup: [u32; 3],
    pub data_setup: [u32; 3],
    pub data_valid: [u32; 3],
    pub rise: [u32; 3],
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
pub struct PeripheralComparatorLimits {
    pub supply_mv: (u16, u16),
    pub input_uses_vdda: bool,
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
pub struct PeripheralLvdInput {
    pub pin: String,
    pub selector: u8,
}
#[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
pub struct PeripheralLvd {
    pub thresholds_mv: Vec<u16>,
    pub supply_name: String,
    pub inputs: Vec<PeripheralLvdInput>,
}
#[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
pub struct PeripheralIr {
    pub owner: String,
    pub register: String,
    pub mode_configurable: bool,
    pub software_control: bool,
    pub invert: bool,
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
pub struct PeripheralReferenceDivider {
    pub clock_owner: String,
    pub consumers: Vec<String>,
    pub negative_mux: u8,
    pub supply_mv: (u16, u16),
    pub input_uses_vdda: bool,
}
#[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
pub struct PeripheralDacLimits {
    pub supply_mv: (u16, u16),
    pub resolution_bits: u8,
}
#[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
pub struct PeripheralOpaLimits {
    pub supply_mv: (u16, u16),
    pub output_headroom_mv: u16,
}
#[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
pub struct PeripheralLcd {
    pub segments: Vec<u8>,
    pub ram_registers: Vec<u8>,
    pub lsi_typical_hz: u32,
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
pub struct PeripheralRtcCalendar {
    pub source: String,
    pub source_encoding: u8,
    pub nominal_hz: u32,
    pub minimum_hz: u32,
    pub maximum_hz: u32,
    pub temperature_c: (i16, i16),
    pub supply_mv: (u16, u16),
    pub factory_trim_address: u32,
    pub calendar_divisor: u32,
    pub prescaler_first: u16,
    pub prescaler_second: u32,
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
pub struct PeripheralRtcAlarms {
    pub direct_register_access: bool,
    pub async_wait: bool,
    pub alarm_a_ignore_bit: u8,
    pub alarm_b_configuration_supported: bool,
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
pub struct PeripheralAtimComplementary {
    pub channels: u8,
    pub dead_time_max_ticks: u16,
    pub break_inputs: u8,
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
/// Family output-capable pads and supported pull-downs, independent of package bonding and interrupt/command masks.
pub struct PeripheralGpio {
    pub output_mask: u16,
    pub pull_down_mask: u16,
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
/// Inclusive signed Q1.31 bounds rounded inward from the own-manual rational domain.
pub struct PeripheralCordicDomain {
    pub name: String,
    pub minimum: i32,
    pub maximum: i32,
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
/// Own-source qualified numeric domains; operation and format selectors remain register IR.
pub struct PeripheralCordic {
    pub domains: Vec<PeripheralCordicDomain>,
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
/// Own-source block and supported key geometry, in 32-bit register words.
pub struct PeripheralAes {
    pub block_words: u8,
    pub key_words_128: u8,
    pub key_words_192: u8,
    pub key_words_256: u8,
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
/// Own-source output geometry, in 32-bit register words; no entropy guarantee.
pub struct PeripheralTrng {
    pub output_words: u8,
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize)]
/// Availability of the read-only checking-enabled status; no software parity control.
pub struct PeripheralRamParity {
    pub enable_status: bool,
}
