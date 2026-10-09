//! Source-qualified CW32L010/L011 BTIM1 UPDATE routes.
//!
//! Each destination has its own generated selector type. Static peripheral
//! ownership prevents safe reconfiguration while a route is live, including if
//! it is forgotten. Direct PAC access must not alter these resources or clocks.
//! No interrupt, DMA, fanout, lossless event log or cycle-exact timing is implied.
use super::low_level::{ConfigError, Prescaler, Timer, Timing};
use crate::{
    Peri, pac,
    peripherals::{BTIM1, BTIM2},
    rcc::ClockBounds,
    time::Hertz,
};

include!(concat!(env!("OUT_DIR"), "/timer_trigger_routes.rs"));

/// BTIM1 UPDATE routed through TRGO. Constructed stopped; timing is immutable.
/// Consume this source into [`Btim2Cascade`] or [`crate::adc::triggered::Btim1Adc`].
pub struct Btim1Update {
    timer: Timer<'static, BTIM1>,
}
impl Btim1Update {
    /// Configure 1..=65536 source ticks. Tokens are consumed even on error.
    pub fn try_new(
        peripheral: Peri<'static, BTIM1>,
        prescaler: Prescaler,
        ticks: u32,
    ) -> Result<Self, ConfigError> {
        let timing = Timing::new(prescaler, ticks)?;
        let mut timer = Timer::try_new(peripheral)?;
        timer.apply_timing(timing);
        Ok(Self { timer })
    }
    /// Nominal PCLK, without a timer multiplier.
    pub fn kernel_clock(&self) -> Hertz {
        self.timer.kernel_clock()
    }
    /// Qualified PCLK envelope under RCC's operating conditions.
    pub fn kernel_clock_bounds(&self) -> ClockBounds {
        self.timer.kernel_clock_bounds()
    }
    /// Nominal update interval in PCLK cycles; excludes startup/synchronization.
    pub fn clock_divisor(&self) -> u32 {
        self.timer.clock_divisor()
    }
    pub(crate) fn prepare(&mut self) {
        // The destination must already be detached. MMS=0 forwards UG itself;
        // it does not suppress every possible TRGO event during reconfiguration.
        self.stop();
        self.timer.apply_timing(self.timer.timing);
    }
    pub(crate) fn start(&mut self, one_shot: bool) {
        pac::BTIM1.cr2().write(|v| v.set_mms(BTIM1_UPDATE_MMS));
        pac::BTIM1.cr1().write(|v| {
            v.set_urs(true);
            v.set_oneshot(one_shot);
            v.set_en(true);
        });
    }
    pub(crate) fn stop(&mut self) {
        pac::BTIM1.cr1().write(|v| v.set_urs(true));
        pac::BTIM1.cr2().write(|v| v.set_mms(0));
    }
    pub(crate) fn running(&self) -> bool {
        pac::BTIM1.cr1().read().en()
    }
}
impl Drop for Btim1Update {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Count BTIM1 update edges in BTIM2, wrapping at the selected event count.
///
/// Owns both timers. The destination uses positive polarity, no filter/reset
/// input, and no prescaling. The shared BTIM gate/reset policy is retained.
/// No mutable source or peripheral token can be extracted from a live route.
pub struct Btim2Cascade {
    source: Btim1Update,
    counter: Timer<'static, BTIM2>,
}
impl Btim2Cascade {
    /// Construct stopped; `events` is 1..=65536. Tokens are consumed on error.
    pub fn try_new(
        source: Btim1Update,
        destination: Peri<'static, BTIM2>,
        events: u32,
    ) -> Result<Self, ConfigError> {
        let timing = Timing::new(Prescaler::Div1, events)?;
        let mut counter = Timer::try_new(destination)?;
        counter.apply_timing(timing);
        Ok(Self { source, counter })
    }
    /// This destination's qualified source selector.
    pub fn trigger_input(&self) -> Btim2TriggerInput {
        Btim2TriggerInput::Btim1Update
    }
    /// Restart from zero. Detach before any source change or software update.
    pub fn start(&mut self) {
        self.stop();
        self.source.prepare();
        self.counter.apply_timing(self.counter.timing);
        pac::BTIM2.smcr().write(|v| {
            v.set_trgisrc(self.trigger_input() as u8);
            v.set_sms(3); // Own manual §11.9.3: external rising-edge counting.
        });
        pac::BTIM2.cr1().write(|v| {
            v.set_urs(true);
            v.set_en(true);
        });
        self.source.start(false);
    }
    /// Disconnect destination before stopping the producer. Retain ownership.
    pub fn stop(&mut self) {
        // Stop counting before SMS=0 selects the internal PCLK source. Otherwise
        // the intervening bus cycles could advance the destination counter.
        pac::BTIM2.cr1().write(|v| v.set_urs(true));
        pac::BTIM2.smcr().write(|v| {
            v.set_sms(0);
            v.set_trgisrc(0);
        });
        self.source.stop();
    }
    /// Current count modulo the configured event period.
    pub fn get_counter(&self) -> u16 {
        self.counter.get_counter()
    }
    /// Coalescing overflow indication, not a lossless event count.
    pub fn is_overflow_pending(&self) -> bool {
        self.counter.is_overflow_pending()
    }
    /// Clear only overflow with a typed W0C command, preserving trigger status.
    pub fn clear_overflow(&mut self) {
        pac::BTIM2.icr().write(|v| {
            v.set_uif(false);
            v.set_tif(true);
        });
    }
    /// Configured PCLK divisor, assuming each emitted trigger edge is received.
    /// This does not establish a maximum lossless trigger rate.
    pub fn clock_divisor(&self) -> u64 {
        u64::from(self.source.clock_divisor()) * u64::from(self.counter.period_ticks())
    }
    /// Qualified source PCLK envelope under RCC's operating conditions.
    pub fn kernel_clock_bounds(&self) -> ClockBounds {
        self.source.kernel_clock_bounds()
    }
}
impl Drop for Btim2Cascade {
    fn drop(&mut self) {
        self.stop();
    }
}
