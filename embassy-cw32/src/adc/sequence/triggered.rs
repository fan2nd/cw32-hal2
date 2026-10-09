//! CW32L010/L011 external-channel conversion started by one BTIM1 UPDATE.
//!
//! This distinct owner exposes no blocking-read or mutable configuration API.
//! It owns static ADC, timer and package-qualified pin tokens; forgetting it
//! cannot make those resources available for safe reconfiguration. No DMA or
//! periodic/lossless acquisition is provided. Public PAC access is outside
//! that ownership guarantee and must not concurrently modify these resources.
use super::*;
use crate::{gpio::Pin, peripherals::ADC, timer::trigger::Btim1Update};

include!(concat!(env!("OUT_DIR"), "/adc_trigger_routes.rs"));

/// Failure of the bounded one-shot acquisition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum TriggerError {
    /// The current acquisition or unread result is preserved.
    Busy,
    /// No acquisition is armed.
    NotArmed,
    /// Poll budget expired; the route is now permanently disarmed.
    Timeout,
    /// A disarmed/timed-out route cannot be armed again.
    Disarmed,
}
impl core::fmt::Display for TriggerError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::Busy => "ADC hardware trigger already armed",
            Self::NotArmed => "ADC hardware trigger is not armed",
            Self::Timeout => "ADC hardware trigger timed out",
            Self::Disarmed => "ADC hardware trigger was permanently disarmed",
        })
    }
}
impl core::error::Error for TriggerError {}

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Idle,
    Armed,
    Disarmed,
}

/// One external ADC sample per arm, emitted by BTIM1's one-shot update.
///
/// A successful sample may be followed by a new arm. Cancellation and timeout
/// are terminal for this owner: no immediate abort or trigger-pipeline drain
/// guarantee is assumed. All resources remain owned until drop. Conversion
/// timing uses the existing ADC supply/acquisition checks and ClockBounds.
pub struct Btim1Adc<P: Pin>
where
    Peri<'static, P>: AdcChannel<'static, ADC>,
{
    adc: Adc<'static, ADC>,
    source: Btim1Update,
    _pin: Peri<'static, P>,
    state: State,
}
impl<P: Pin> Btim1Adc<P>
where
    Peri<'static, P>: AdcChannel<'static, ADC>,
{
    /// Configure stopped, retaining existing shared BGR/gate policy. The board
    /// must satisfy the ADC supply, pin and source-impedance limits. Tokens are
    /// consumed on error; timing validation precedes any pin/ADC changes.
    pub fn try_new(
        peripheral: Peri<'static, ADC>,
        source: Btim1Update,
        mut pin: Peri<'static, P>,
        config: Config,
        sample_time: SampleTime,
    ) -> Result<Self, Error> {
        let clocks = crate::rcc::try_clocks().ok_or(Error::ClockNotInitialized)?;
        let channel = SealedAdcChannel::<ADC>::channel(&pin);
        let timing = config.timing(clocks.pclk_bounds(), channel, sample_time)?;
        // Reuse the established shared analog lifecycle privately; callers have
        // no access to this blocking owner or its reconfiguration methods.
        let mut adc = Adc::try_new(peripheral, config)?;
        SealedAdcChannel::<ADC>::setup(&mut pin);
        adc.shutdown();
        adc.timing = timing;
        adc.initialize();
        pac::ADC.sqrcfr().write(|v| v.set_sqrch(0, channel));
        Ok(Self {
            adc,
            source,
            _pin: pin,
            state: State::Idle,
        })
    }
    /// The ADC's qualified independent enable, not a timer selector number.
    pub fn trigger_input(&self) -> TriggerInput {
        TriggerInput::Btim1Update
    }
    /// Schedule one conversion without writing ADC_START=1. A busy arm preserves
    /// the pending/unread sample. Source delay excludes trigger synchronization.
    pub fn arm(&mut self) -> Result<(), TriggerError> {
        match self.state {
            State::Armed => return Err(TriggerError::Busy),
            State::Disarmed => return Err(TriggerError::Disarmed),
            State::Idle => {}
        }
        self.detach();
        self.source.prepare();
        pac::ADC.start().write(|v| v.set_start(false));
        let _ = pac::ADC.result(0).read().result();
        self.clear_flags();
        match self.trigger_input() {
            TriggerInput::Btim1Update => pac::ADC.trigger().write(|v| v.set_btim1trgo(true)),
        }
        self.state = State::Armed;
        self.source.start(true);
        Ok(())
    }
    /// Poll once. Completion requires the one-shot source stopped, both EOC/EOS,
    /// and ADC_START clear. A returned result is consumed exactly once.
    pub fn poll(&mut self) -> Result<Option<u16>, TriggerError> {
        match self.state {
            State::Idle => return Err(TriggerError::NotArmed),
            State::Disarmed => return Err(TriggerError::Disarmed),
            State::Armed => {}
        }
        if self.source.running() {
            return Ok(None);
        }
        let status = pac::ADC.isr().read();
        if !status.eoc() || !status.eos() || pac::ADC.start().read().start() {
            return Ok(None);
        }
        self.detach();
        self.source.stop();
        let result = pac::ADC.result(0).read().result() & Resolution::Bits12.max_count() as u16;
        self.clear_flags();
        self.state = State::Idle;
        Ok(Some(result))
    }
    /// Poll at most `budget` iterations, not a wall-clock duration. Timeout
    /// disconnects the route and prevents rearming; zero expires immediately.
    pub fn wait(&mut self, budget: u32) -> Result<u16, TriggerError> {
        match self.state {
            State::Idle => return Err(TriggerError::NotArmed),
            State::Disarmed => return Err(TriggerError::Disarmed),
            State::Armed => {}
        }
        for _ in 0..budget {
            if let Some(value) = self.poll()? {
                return Ok(value);
            }
        }
        self.disarm();
        Err(TriggerError::Timeout)
    }
    /// Permanently disconnect this route, then stop the timer and request ADC
    /// stop/sequence reset. Does not promise an atomic cutoff or hardware drain.
    pub fn disarm(&mut self) {
        self.detach();
        self.source.stop();
        pac::ADC.start().write(|v| v.set_start(false));
        let _ = pac::ADC.result(0).read().result();
        self.clear_flags();
        self.state = State::Disarmed;
    }
    /// Qualified ADC conversion timing, excluding trigger/startup latency.
    pub fn timing(&self) -> Timing {
        self.adc.timing
    }
    fn detach(&self) {
        pac::ADC.trigger().write(|v| v.set_btim1trgo(false));
    }
    fn clear_flags(&self) {
        // All four defined flags are W0C; use a write, never an ISR RMW.
        pac::ADC.icr().write(|v| {
            v.set_eoc(false);
            v.set_eos(false);
            v.set_awdl(false);
            v.set_awdh(false);
        });
    }
}
impl<P: Pin> Drop for Btim1Adc<P>
where
    Peri<'static, P>: AdcChannel<'static, ADC>,
{
    fn drop(&mut self) {
        self.disarm();
    }
}
