// SPDX-License-Identifier: MIT OR Apache-2.0
//! Project-authored data model for the upstream-compatible chip JSON interface.
//!
//! Field widths, declaration order and Serde defaults are compatibility contracts.
//! This crate describes data; source-backed hardware validation belongs to the
//! generator. Historical source ancestry is retained in the provenance record.

/// Cached regular expression at one call site, compiled on its first evaluation.
#[macro_export]
macro_rules! regex {
    ($pattern:literal) => {{
        static COMPILED: ::std::sync::OnceLock<$crate::__Regex> = ::std::sync::OnceLock::new();
        COMPILED.get_or_init(|| $crate::__Regex::new($pattern).expect("invalid regex! pattern"))
    }};
}

#[doc(hidden)]
pub use regex::Regex as __Regex;

// A record has the same serialization traits regardless of its namespace.
// Ordering is requested explicitly only for types that expose that API.
macro_rules! record {
    ($(#[$attribute:meta])* $name:ident { $($fields:tt)* }) => {
        #[derive(Clone, Debug, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize)]
        $(#[$attribute])*
        pub struct $name { $($fields)* }
    };
}

pub mod chip {
    pub mod memory {
        #[derive(
            Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, serde::Serialize, serde::Deserialize,
        )]
        #[serde(rename_all = "lowercase")]
        pub enum Kind {
            Flash,
            Ram,
            Eeprom,
        }

        record! {
            #[derive(Ord, PartialOrd)]
            Settings {
                pub erase_size: u32,
                pub write_size: u32,
                pub erase_value: u8,
            }
        }
        record! {
            #[derive(Copy, Ord, PartialOrd)]
            Access {
                pub read: bool,
                pub write: bool,
                pub execute: bool,
            }
        }
    }

    pub mod core {
        pub mod peripheral {
            record! {
                /// Own-source qualified ATIM subset, independent of register layout.
                AtimComplementary {
                    pub channels: u8,
                    pub dead_time_max_ticks: u16,
                    pub break_inputs: u8,
                }
            }

            pub mod rcc {
                record! {
                    #[derive(Ord, PartialOrd)]
                    Field {
                        pub register: String,
                        pub field: String,
                    }
                }

                #[derive(
                    Clone,
                    Debug,
                    Eq,
                    PartialEq,
                    Ord,
                    PartialOrd,
                    Hash,
                    serde::Serialize,
                    serde::Deserialize,
                )]
                #[serde(untagged)]
                pub enum KernelClock {
                    Clock(String),
                    Mux(Field),
                }

                #[derive(
                    Clone,
                    Debug,
                    Default,
                    Eq,
                    PartialEq,
                    Ord,
                    PartialOrd,
                    Hash,
                    serde::Serialize,
                    serde::Deserialize,
                )]
                pub enum StopMode {
                    #[default]
                    Stop1,
                    Stop2,
                    Standby,
                }

                pub(super) fn is_stop1(mode: &StopMode) -> bool {
                    matches!(mode, StopMode::Stop1)
                }
            }

            record! {
                #[derive(Ord, PartialOrd)]
                Registers {
                    pub kind: String,
                    pub version: String,
                    pub block: String,
                }
            }
            record! {
                #[derive(Ord, PartialOrd)]
                Rcc {
                    pub bus_clock: String,
                    pub kernel_clock: rcc::KernelClock,
                    pub enable: rcc::Field,
                    #[serde(skip_serializing_if = "Option::is_none")]
                    pub reset: Option<rcc::Field>,
                    #[serde(default, skip_serializing_if = "rcc::is_stop1")]
                    pub stop_mode: rcc::StopMode,
                }
            }
            record! {
                /// Family output-capable pads and supported pull-downs, independent of package bonding and interrupt/command masks.
                Gpio {
                    pub output_mask: u16,
                    pub pull_down_mask: u16,
                }
            }
            record! {
                /// Inclusive signed Q1.31 bounds rounded inward from the own-manual rational domain.
                CordicDomain {
                    pub name: String,
                    pub minimum: i32,
                    pub maximum: i32,
                }
            }
            record! {
                /// Own-source qualified numeric domains; operation and format selectors remain register IR.
                Cordic {
                    pub domains: Vec<CordicDomain>,
                }
            }
            record! {
                /// Own-source block and supported key geometry, in 32-bit register words.
                Aes {
                    pub block_words: u8,
                    pub key_words_128: u8,
                    pub key_words_192: u8,
                    pub key_words_256: u8,
                }
            }
            record! {
                /// Own-source output geometry, in 32-bit register words; no entropy guarantee.
                Trng {
                    pub output_words: u8,
                }
            }
            record! {
                /// Availability of the read-only checking-enabled status; no software parity control.
                RamParity {
                    pub enable_status: bool,
                }
            }
            record! {
                /// Independent GPIO serviced-source and documented W0C command domains.
                GpioInterrupt {
                    pub serviced_mask: u16,
                    pub clear_noop_mask: u16,
                    pub level_trigger: bool,
                }
            }
            record! {
                /// Verified clock controls, independent of kernel-clock completeness.
                RccControl {
                    pub controller: String,
                    pub bus_clock: String,
                    pub enable: rcc::Field,
                    pub enable_active_value: bool,
                    #[serde(default, skip_serializing_if = "Option::is_none")]
                    pub enable_write_key: Option<RccWriteKey>,
                    #[serde(default, skip_serializing_if = "Option::is_none")]
                    pub reset: Option<rcc::Field>,
                    #[serde(default, skip_serializing_if = "Option::is_none")]
                    pub reset_asserted_value: Option<bool>,
                    #[serde(default, skip_serializing_if = "Option::is_none")]
                    pub shared_enable_group: Option<String>,
                    #[serde(default, skip_serializing_if = "Option::is_none")]
                    pub shared_reset_group: Option<String>,
                }
            }
            record! {
                /// An unshifted write-key value accompanying a gate write.
                RccWriteKey {
                    pub field: rcc::Field,
                    pub value: u32,
                }
            }
            record! {
                /// Source-qualified master clock limits, independent of register layout.
                Spi {
                    pub maximum_frequency: u32,
                    pub minimum_divisor: u16,
                }
            }
            record! {
                LvdInput { pub pin: String, pub selector: u8, }
            }
            record! {
                Lvd { pub thresholds_mv: Vec<u16>, pub supply_name: String, pub inputs: Vec<LvdInput>, }
            }
            record! {
                /// Reviewed calendar source envelope and nominal hardware division.
                RtcCalendar { pub source: String,
                    pub source_encoding: u8,
                    pub nominal_hz: u32,
                    pub minimum_hz: u32,
                    pub maximum_hz: u32,
                    pub temperature_c: (i16, i16),
                    pub supply_mv: (u16, u16),
                    pub factory_trim_address: u32,
                    pub calendar_divisor: u32,
                    pub prescaler_first: u16,
                    pub prescaler_second: u32, }
            }
            record! {
                /// Own-source alarm capabilities, separate from calendar clock facts.
                RtcAlarms {
                    pub direct_register_access: bool,
                    pub async_wait: bool,
                    pub alarm_a_ignore_bit: u8,
                    pub alarm_b_configuration_supported: bool,
                }
            }
            record! {
                Lcd { pub segments: Vec<u8>, pub ram_registers: Vec<u8>, pub lsi_typical_hz: u32, }
            }
            record! {
                Ir { pub owner: String, pub register: String, pub mode_configurable: bool, pub software_control: bool, pub invert: bool, }
            }
            record! {
                        /// Qualified actual external-source range and own-device conditions.
                        ExternalClockLimits {
            pub minimum_hz: u32,
            pub maximum_hz: u32,
            pub supply_mv: (u16, u16),
            pub temperature_c: (i16, i16),
                        }
                    }
            record! {
                        /// Direct HSE electrical limits and own-manual detector policy.
                        HseLimits {
            pub crystal: ExternalClockLimits,
            pub bypass: ExternalClockLimits,
            pub filter_maximum_hz: u32,
            pub startup_cycles: [u32; 4],
            /// Hardware frequency-range selector bins; absent when the selector does not exist.
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub frequency_ranges_hz: Option<[(u32, u32); 4]>,
            /// Fixed HSIOSC divisor imposed by CCS, only when explicitly source-qualified.
            #[serde(default, skip_serializing_if = "Option::is_none")]
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
                    }
            record! {
                /// Direct HEX input limits; no crystal, detector or automatic fallback is implied.
                HexLimits {
                    pub input: ExternalClockLimits,
                    pub duty_percent: (u8, u8),
                    pub minimum_high_low_ns: u16,
                    pub maximum_rise_fall_ns: u16,
                    /// Percent of VDDIOx relative to VSS.
                    pub high_level_vddio_percent: (u8, u8),
                    pub low_level_vddio_percent: (u8, u8),
                }
            }
            record! {
                /// Inherited LSE pad ownership; does not qualify LSE configuration.
                Lse {
                    /// Whether the oscillator provides a separate pad lock.
                    pub pin_lock: bool,
                    /// Whether the pad lock requires the oscillator enable lock.
                    pub pin_lock_requires_enable_lock: bool,
                }
            }
            record! {
                /// Source-defined reset observation; excludes write-only command registers.
                #[serde(deny_unknown_fields)]
                LseRtcReset {
                    pub register: String,
                    pub byte_offset: u32,
                    pub value: u32,
                    pub mask: u32,
                }
            }
            record! {
                /// Direct LSE output alternate-function route.
                #[serde(deny_unknown_fields)]
                LseOutputRoute {
                    pub pin: String,
                    pub af: u8,
                }
            }
            record! {
                /// LPTIM/LCD source and the documented effect of their RCC gate.
                #[serde(deny_unknown_fields)]
                LseWorkGatedConsumer {
                    pub source: u8,
                    pub gate_controls_work: bool,
                }
            }
            record! {
                /// Own native facts for AUTOTRIM-equipped LSE hardware.
                #[serde(deny_unknown_fields)]
                LseStartupConsumers {
                    pub startup_analog: bool,
                    pub autotrim_source: u8,
                    pub lptim: LseWorkGatedConsumer,
                    pub lcd: LseWorkGatedConsumer,
                    pub uarts: Vec<String>,
                    pub lsi_output_routes: Vec<LseOutputRoute>,
                }
            }
            record! {
                /// Exact-family native low-power drive, detector and RTC facts. No amplitude field exists.
                #[serde(deny_unknown_fields)]
                LseNativeLowPower {
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
                    pub rtc_output_routes: Vec<LseOutputRoute>,
                }
            }
            record! {
                /// Optional init-only SYSCLK detector qualification. Not a second oscillator configuration.
                #[serde(deny_unknown_fields)]
                LseSysclkDetector {
                    /// Own-manual hardware edge count in one detector window.
                    pub lse_edges: u16,
                    /// Own-manual LSI cycles in one detector window.
                    pub lsi_cycles: u16,
                    /// Software phase margin, additional to the hardware edge count.
                    pub margin_lse_edges: u16,
                }
            }
            record! {
                        /// Exact-part active LSE qualification. Board frequency bounds remain mandatory.
                        #[serde(deny_unknown_fields)]
                        LseConfiguration {
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
                            #[serde(default, skip_serializing_if = "Option::is_none")]
                            pub startup_consumers: Option<LseStartupConsumers>,
                            #[serde(default, skip_serializing_if = "Option::is_none")]
                            pub native_low_power: Option<LseNativeLowPower>,
                            #[serde(default, skip_serializing_if = "Option::is_none")]
                            pub sysclk_detector: Option<LseSysclkDetector>,
                            pub mco_source: u8,
                            pub gpio_dir_offset: u32,
                            pub gpio_speed_offset: Option<u32>,
                            pub rtc_reset: Vec<LseRtcReset>,
                            pub output_routes: Vec<LseOutputRoute>,
                        }
                    }
            record! {
                /// Own-family PLL qualification; analog bins are distinct from electrical limits.
                PllLimits {
    pub input_range_hz: (u32, u32),
    pub output_range_hz: (u32, u32),
    pub input_ranges_hz: [(u32, u32); 4],
    pub output_ranges_hz: [(u32, u32); 5],
    pub multiplier_range: (u8, u8),
    pub supply_mv: (u16, u16),
    pub temperature_c: (i16, i16),
    pub hsi_supported: bool,
    /// Vendor-documented functional HSE oscillator and bypass references; rate bounds only.
    pub hse_supported: bool,
    pub startup_cycles: u32,
    pub startup_encoding: u8,
    pub reserved_debug_default: u8,
    pub cycle_to_cycle_jitter_ps: u32,
                }
            }
            record! {
                /// Own-source init-only factory LSI; rate bounds only, with the complete direct-consumer roster.
                #[serde(deny_unknown_fields)]
                LsiSysclk {
                    pub nominal_hz: u32,
                    pub minimum_hz: u32,
                    pub maximum_hz: u32,
                    pub supply_mv: (u16, u16),
                    pub temperature_c: (i16, i16),
                    pub factory_trim_address: u32,
                    pub rtc_allowed_sources: Vec<u8>,
                    pub awt_allowed_sources: Vec<u8>,
                    pub uart_allowed_sources: Vec<u8>,
                    pub uarts: Vec<String>,
                    pub gpio_banks: Vec<String>,
                    pub gpio_filter_allowed_sources: Vec<u8>,
                    pub mco_allowed_sources: Vec<u8>,
                    pub lsi_output_pin: String,
                    pub lsi_output_allowed_af: Vec<u8>,
                    pub rcc_irq: u16,
                }
            }
            record! {
                ClockLimits {
                    pub hsi_frequency_hz: u32,
                    pub hsi_error_percent: u32,
                    pub hsi_supply_mv: (u16, u16),
                    pub hsi_temperature_c: (i16, i16),
                    pub low_voltage_threshold_mv: u16,
                    pub low_voltage_bus_max_hz: u32,
                    pub high_voltage_bus_max_hz: u32,
                    pub factory_hsi_trim_address: u32,
                    /// Required legal incoming HSIOSC range; not an arbitrary-TRIM frequency guarantee.
                    #[serde(default, skip_serializing_if = "Option::is_none")]
                    pub hsi_operating_range_hz: Option<(u32, u32)>,
                    pub default_hsi_divisor: u8,
                    pub initial_flash_wait: u32,
                    pub flash_wait_step_hz: u32,
                    #[serde(default, skip_serializing_if = "Option::is_none")]
                    pub hse: Option<HseLimits>,
                    #[serde(default, skip_serializing_if = "Option::is_none")]
                    pub hex: Option<HexLimits>,
                    #[serde(default, skip_serializing_if = "Option::is_none")]
                    pub lse: Option<Lse>,
                    #[serde(default, skip_serializing_if = "Option::is_none")]
                    pub lse_configuration: Option<LseConfiguration>,
                    #[serde(default, skip_serializing_if = "Option::is_none")]
                    pub pll: Option<PllLimits>,
                    #[serde(default, skip_serializing_if = "Option::is_none")]
                    pub lsi_sysclk: Option<LsiSysclk>,
                }
            }
            record! {
                DacLimits { pub supply_mv: (u16, u16), pub resolution_bits: u8, }
            }
            record! {
                OpaLimits { pub supply_mv: (u16, u16), pub output_headroom_mv: u16, }
            }
            record! {
                ReferenceDivider {
                    pub clock_owner: String,
                    pub consumers: Vec<String>,
                    pub negative_mux: u8,
                    pub supply_mv: (u16, u16),
                    pub input_uses_vdda: bool,
                }
            }
            record! {
                ComparatorLimits {
                    pub supply_mv: (u16, u16),
                    pub input_uses_vdda: bool,
                }
            }
            record! {
                AdcBand {
                    pub supply_min_mv: u16,
                    pub maximum_clock_hz: u32,
                    pub maximum_sample_rate_hz: u32,
                    pub minimum_acquisition_ps: u32,
                }
            }
            record! {
                AdcSequence {
                    pub maximum_length: u8,
                    pub programmable_order: bool,
                    pub per_slot_sample_time: bool,
                    pub per_slot_result: bool,
                }
            }
            record! {
                /// Own-source classic scan and internal-source operating constraints.
                ClassicAdcScan {
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
            }
            record! {
                AdcLimits {
                    pub supply_mv: (u16, u16),
                    pub minimum_clock_hz: u32,
                    pub sample_cycles: Vec<u16>,
                    pub comparison_cycles: u16,
                    pub supply_bands: Vec<AdcBand>,
                    pub internal_1v5_bands: Vec<AdcBand>,
                    pub internal_2v5_bands: Vec<AdcBand>,
                    pub input_follower_maximum_rate_hz: u32,
                    pub temperature_acquisition_maximum_rate_hz: u32,
                    pub internal_acquisition_ps: u64,
                    #[serde(default, skip_serializing_if = "Option::is_none")]
                    pub sequence: Option<AdcSequence>,
                    #[serde(default, skip_serializing_if = "Option::is_none")]
                    pub classic_scan: Option<ClassicAdcScan>,
                }
            }
            record! {
                IwdtClock {
                    pub typical_hz: u32,
                    pub minimum_hz: u32,
                    pub maximum_hz: u32,
                    pub uses_lsi: bool,
                }
            }
            record! {
                I2cWaveform {
                    pub maximum_frequency_hz: [u32; 3],
                    pub low: [u32; 3],
                    pub high: [u32; 3],
                    pub hold: [u32; 3],
                    pub setup: [u32; 3],
                    pub data_setup: [u32; 3],
                    pub data_valid: [u32; 3],
                    pub rise: [u32; 3],
                }
            }
            record! {
                I2cLimits {
                    pub maximum_frequency_hz: u32,
                    pub waveform: Option<I2cWaveform>,
                }
            }
            record! {
                FlashLimits {
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
            }
            record! {
                ClassicTimerInput {
                    pub capture_mux: String,
                    pub encoder_fixed_reload: Option<u16>,
                }
            }
            record! {
                Pin {
                    pub pin: String,
                    pub signal: String,
                    #[serde(skip_serializing_if = "Option::is_none")]
                    pub af: Option<u8>,
                    #[serde(skip_serializing_if = "Option::is_none")]
                    pub adc_mux: Option<u8>,
                    #[serde(skip_serializing_if = "Option::is_none")]
                    /// Source-qualified hardware input mux, independent of signal numbering.
                    pub comparator_mux: Option<u8>,
                }
            }
            record! {
                #[derive(Ord, PartialOrd)]
                Interrupt {
                    pub signal: String,
                    pub interrupt: String,
                }
            }
            record! {
                #[derive(Ord, PartialOrd)]
                Trigger {
                    pub signal: String,
                    pub source: String,
                    /// Source event and destination-local register encoding; absent in legacy data.
                    #[serde(default, skip_serializing_if = "Vec::is_empty")]
                    pub registers: Vec<TriggerRegister>,
                }
            }
            record! {
                #[derive(Ord, PartialOrd)]
                TriggerRegister {
                    pub peripheral: String,
                    pub register: String,
                    pub field: String,
                    pub role: String,
                    pub bit_offset: u32,
                    pub bit_size: u32,
                    /// Unshifted field value, never an aggregate mask.
                    pub value: u32,
                }
            }
            record! {
                #[derive(Ord, PartialOrd)]
                RemapInfo {
                    pub register: String,
                    pub field: String,
                    pub value: u8,
                }
            }
            record! {
                #[derive(Ord, PartialOrd)]
                DmaChannel {
                    pub signal: String,
                    #[serde(skip_serializing_if = "Option::is_none")]
                    pub dma: Option<String>,
                    #[serde(skip_serializing_if = "Option::is_none")]
                    pub channel: Option<String>,
                    #[serde(skip_serializing_if = "Option::is_none")]
                    pub dmamux: Option<String>,
                    #[serde(default, skip_serializing_if = "Vec::is_empty")]
                    pub remap: Vec<RemapInfo>,
                    #[serde(skip_serializing_if = "Option::is_none")]
                    pub request: Option<u8>,
                }
            }
            record! {
                #[derive(Ord, PartialOrd)]
                AfioValue {
                    pub value: u8,
                    #[serde(skip_serializing_if = "Vec::is_empty")]
                    pub pins: Vec<String>,
                }
            }
            record! {
                #[derive(Ord, PartialOrd)]
                Afio {
                    pub register: String,
                    pub field: String,
                    #[serde(skip_serializing_if = "Vec::is_empty")]
                    pub values: Vec<AfioValue>,
                }
            }
        }

        record! {
            #[derive(Ord, PartialOrd)]
            Interrupt {
                pub name: String,
                pub number: u8,
            }
        }
        record! {
            #[derive(Ord, PartialOrd)]
            Pin {
                pub name: String,
            }
        }
        record! {
            #[derive(Ord, PartialOrd)]
            DmaChannels {
                pub name: String,
                pub dma: String,
                pub channel: u8,
                #[serde(skip_serializing_if = "Option::is_none")]
                pub dmamux: Option<String>,
                #[serde(skip_serializing_if = "Option::is_none")]
                pub dmamux_channel: Option<u8>,
                #[serde(skip_serializing_if = "Option::is_none")]
                pub supports_2d: Option<bool>,
            }
        }
        record! {
            Peripheral {
                pub name: String,
                #[serde(default)]
                pub address: u32,
                #[serde(skip_serializing_if = "Option::is_none")]
                pub registers: Option<peripheral::Registers>,
                #[serde(skip_serializing_if = "Option::is_none")]
                pub rcc: Option<peripheral::Rcc>,
                #[serde(default, skip_serializing_if = "Option::is_none")]
                pub rcc_control: Option<peripheral::RccControl>,
                #[serde(default, skip_serializing_if = "Option::is_none")]
                pub spi: Option<peripheral::Spi>,
                #[serde(default, skip_serializing_if = "Option::is_none")]
                pub classic_timer_input: Option<peripheral::ClassicTimerInput>,
                #[serde(default, skip_serializing_if = "Option::is_none")]
                pub atim_complementary: Option<peripheral::AtimComplementary>,
                #[serde(default, skip_serializing_if = "Option::is_none")]
                pub lvd: Option<peripheral::Lvd>,
                #[serde(default, skip_serializing_if = "Option::is_none")]
                pub lcd: Option<peripheral::Lcd>,
                #[serde(default, skip_serializing_if = "Option::is_none")]
                pub rtc_calendar: Option<peripheral::RtcCalendar>,
                #[serde(default, skip_serializing_if = "Option::is_none")]
                pub rtc_alarms: Option<peripheral::RtcAlarms>,
                #[serde(default, skip_serializing_if = "Option::is_none")]
                pub ir: Option<peripheral::Ir>,
                #[serde(default, skip_serializing_if = "Option::is_none")]
                pub gpio_interrupt: Option<peripheral::GpioInterrupt>,
                #[serde(default, skip_serializing_if = "Option::is_none")]
                pub gpio: Option<peripheral::Gpio>,
                #[serde(default, skip_serializing_if = "Option::is_none")]
                pub cordic: Option<peripheral::Cordic>,
                #[serde(default, skip_serializing_if = "Option::is_none")]
                pub aes: Option<peripheral::Aes>,
                #[serde(default, skip_serializing_if = "Option::is_none")]
                pub trng: Option<peripheral::Trng>,
                #[serde(default, skip_serializing_if = "Option::is_none")]
                pub ram_parity: Option<peripheral::RamParity>,
                #[serde(default, skip_serializing_if = "Option::is_none")]
                pub clock_limits: Option<peripheral::ClockLimits>,
                #[serde(default, skip_serializing_if = "Option::is_none")]
                pub adc_limits: Option<peripheral::AdcLimits>,
                #[serde(default, skip_serializing_if = "Option::is_none")]
                pub comparator_limits: Option<peripheral::ComparatorLimits>,
                #[serde(default, skip_serializing_if = "Option::is_none")]
                pub reference_divider: Option<peripheral::ReferenceDivider>,
                #[serde(default, skip_serializing_if = "Option::is_none")]
                pub dac_limits: Option<peripheral::DacLimits>,
                #[serde(default, skip_serializing_if = "Option::is_none")]
                pub opa_limits: Option<peripheral::OpaLimits>,
                #[serde(default, skip_serializing_if = "Option::is_none")]
                pub iwdt_clock: Option<peripheral::IwdtClock>,
                #[serde(default, skip_serializing_if = "Option::is_none")]
                pub i2c_limits: Option<peripheral::I2cLimits>,
                #[serde(default, skip_serializing_if = "Option::is_none")]
                pub flash_limits: Option<peripheral::FlashLimits>,
                #[serde(default, skip_serializing_if = "Vec::is_empty")]
                pub pins: Vec<peripheral::Pin>,
                #[serde(default, skip_serializing_if = "Vec::is_empty")]
                pub interrupts: Vec<peripheral::Interrupt>,
                #[serde(default, skip_serializing_if = "Vec::is_empty")]
                pub dma_channels: Vec<peripheral::DmaChannel>,
                #[serde(default, skip_serializing_if = "Vec::is_empty")]
                pub triggers: Vec<peripheral::Trigger>,
                #[serde(skip_serializing_if = "Option::is_none")]
                pub afio: Option<peripheral::Afio>,
            }
        }
    }

    record! {
        #[derive(Ord, PartialOrd)]
        PackagePin {
            pub position: String,
            pub signals: Vec<String>,
        }
    }
    record! {
        #[derive(Ord, PartialOrd)]
        Package {
            pub name: String,
            pub package: String,
            pub pins: Vec<PackagePin>,
        }
    }
    record! {
        #[derive(Ord, PartialOrd)]
        Memory {
            pub name: String,
            pub kind: memory::Kind,
            pub address: u32,
            pub size: u32,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub settings: Option<memory::Settings>,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub access: Option<memory::Access>,
        }
    }
    record! {
        #[derive(Ord, PartialOrd)]
        Doc {
            pub r#type: String,
            pub title: String,
            pub name: String,
            pub url: String,
        }
    }
    record! {
        Core {
            pub name: String,
            pub peripherals: Vec<core::Peripheral>,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub nvic_priority_bits: Option<u8>,
            pub interrupts: Vec<core::Interrupt>,
            pub dma_channels: Vec<core::DmaChannels>,
            pub pins: Vec<core::Pin>,
        }
    }
}

record! {
    Chip {
        pub name: String,
        pub family: String,
        pub line: String,
        pub die: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub device_id: Option<u16>,
        pub packages: Vec<chip::Package>,
        pub memory: Vec<Vec<chip::Memory>>,
        pub docs: Vec<chip::Doc>,
        pub cores: Vec<chip::Core>,
    }
}

pub mod register_write;
