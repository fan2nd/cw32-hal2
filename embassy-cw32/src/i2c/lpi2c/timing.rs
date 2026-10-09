//! Checked waveform solver, L012 UM V1.4 Tables 23-1..3 and DS V1.0 Table 7-38.
//! Additional rise/data-valid limits: NXP UM10204 Rev 7 Table 11.
//! Minimum timings are conservatively no smaller than that datasheet's values.
//! Rise times are bounds, not exact values: use zero-rise latency for frequency
//! ceilings/minimum durations and worst-case latency for internal safety limits.
use super::{ClockBounds, Config, ConfigError, Hertz};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Timing {
    pub(super) prescale: u8,
    pub(super) clklo: u8,
    pub(super) clkhi: u8,
    pub(super) sethold: u8,
    pub(super) datavd: u8,
    pub(super) scl_filter: u8,
    pub(super) sda_filter: u8,
    pub(super) busidle: u16,
    pub(super) actual_frequency: Hertz,
    pub(super) frequency_bounds: (Hertz, Hertz),
}
#[derive(Clone, Copy)]
struct Limits {
    low: u64,
    high: u64,
    hold: u64,
    setup: u64,
    data_setup: u64,
    data_valid: u64,
    rise: u32,
}
fn limits(frequency: u32) -> Limits {
    let index = if frequency <= crate::I2C_TIMING_MAXIMUM_FREQUENCY_HZ[0] {
        0
    } else if frequency <= crate::I2C_TIMING_MAXIMUM_FREQUENCY_HZ[1] {
        1
    } else {
        2
    };
    Limits {
        low: u64::from(crate::I2C_TIMING_LOW[index]),
        high: u64::from(crate::I2C_TIMING_HIGH[index]),
        hold: u64::from(crate::I2C_TIMING_HOLD[index]),
        setup: u64::from(crate::I2C_TIMING_SETUP[index]),
        data_setup: u64::from(crate::I2C_TIMING_DATA_SETUP[index]),
        data_valid: u64::from(crate::I2C_TIMING_DATA_VALID[index]),
        rise: crate::I2C_TIMING_RISE[index],
    }
}
fn cycles(clock: u64, ns: u64) -> u64 {
    (clock * ns).div_ceil(1_000_000_000)
}

pub(super) fn select_timing(bounds: ClockBounds, config: &Config) -> Result<Timing, ConfigError> {
    if bounds.minimum().0 == 0 {
        return Err(ConfigError::ClockNotInitialized);
    }
    if config.frequency.0 == 0 {
        return Err(ConfigError::FrequencyZero);
    }
    if config.frequency.0 > crate::I2C_MAXIMUM_FREQUENCY_HZ {
        return Err(ConfigError::FrequencyTooHigh);
    }
    if config.poll_limit == 0 {
        return Err(ConfigError::ZeroPollLimit);
    }
    if config.scl_filter_cycles > 15
        || config.sda_filter_cycles > 15
        || config.sda_filter_cycles < config.scl_filter_cycles
    {
        return Err(ConfigError::InvalidFilter);
    }
    let spec = limits(config.frequency.0);
    if config.scl_rise_time_ns > spec.rise || config.sda_rise_time_ns > spec.rise {
        return Err(ConfigError::InvalidRiseTime);
    }
    // Whole-Hz endpoints round outwards, so all interval comparisons are
    // conservative even when HSI /7, /14 or a bus divider is fractional.
    let clock = u64::from(bounds.maximum().0);
    let slow_clock = u64::from(bounds.minimum().0);
    let filter_scl = u64::from(config.scl_filter_cycles);
    let filter_sda = u64::from(config.sda_filter_cycles);
    let rise_scl = cycles(clock, u64::from(config.scl_rise_time_ns));
    let rise_sda = cycles(clock, u64::from(config.sda_rise_time_ns));
    let wanted_period = clock.div_ceil(u64::from(config.frequency.0));
    let mut best: Option<(u64, Timing)> = None;
    for prescale in 0..=7 {
        let div = 1u64 << prescale;
        let scl_min = (2 + filter_scl) / div;
        let sda_min = (2 + filter_sda) / div;
        let scl_max = (2 + filter_scl + rise_scl) / div;
        let sda_max = (2 + filter_sda + rise_sda) / div;
        // Table 23-1: minimum data setup at the fast endpoint; maximum
        // data-valid time at the slow endpoint, including worst-case SDA rise.
        if (sda_min + 1) * div < cycles(clock, spec.data_setup)
            || 2 * div * 1_000_000_000 + u64::from(config.sda_rise_time_ns) * slow_clock
                > spec.data_valid * slow_clock
        {
            continue;
        }
        let sethold = 2u64
            .max(cycles(clock, spec.hold).div_ceil(div).saturating_sub(1))
            .max(
                cycles(clock, spec.setup)
                    .div_ceil(div)
                    .saturating_sub(1 + scl_min),
            )
            .max(sda_max / div + 1); // SETHOLD*2^PRESCALE > SDA_LATENCY.
        if sethold > 63 {
            continue;
        }
        for clklo in 3..=63u64 {
            // DATAVD=1 is the shortest legal hold. Worst-case SDA latency must
            // leave room for it. Digital filters are measured before prescaling.
            if (clklo + 1) * div < cycles(clock, spec.low)
                || clklo * div <= scl_max
                || clklo < sda_max + 2
                || filter_sda > clklo * div - 3
            {
                continue;
            }
            let clkhi = 1u64
                .max(
                    cycles(clock, spec.high)
                        .div_ceil(div)
                        .saturating_sub(1 + scl_min),
                )
                .max(
                    wanted_period
                        .div_ceil(div)
                        .saturating_sub(clklo + 2 + scl_min),
                )
                .max(8u64.saturating_sub(clklo + 2 + scl_min));
            if clkhi > 63 {
                continue;
            }
            let period = (clklo + clkhi + 2 + scl_min) * div;
            if period < wanted_period || period < 8 * div {
                continue;
            }
            let busidle = (clklo + sethold + 2) * 2;
            if busidle <= clkhi + 1 || busidle > 4095 {
                continue;
            }
            let timing = Timing {
                prescale,
                clklo: clklo as u8,
                clkhi: clkhi as u8,
                sethold: sethold as u8,
                datavd: 1,
                scl_filter: config.scl_filter_cycles,
                sda_filter: config.sda_filter_cycles,
                busidle: busidle as u16,
                actual_frequency: bounds.divided_by(period as u32).nominal(),
                frequency_bounds: (
                    bounds
                        .divided_by(((clklo + clkhi + 2 + scl_max) * div) as u32)
                        .minimum(),
                    bounds.divided_by(period as u32).maximum(),
                ),
            };
            if best.is_none_or(|(previous, _)| period < previous) {
                best = Some((period, timing));
            }
        }
    }
    best.map(|(_, timing)| timing)
        .ok_or(ConfigError::TimingNotRepresentable)
}
