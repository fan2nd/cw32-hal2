//! Sealed operations for the source-qualified GTIM and buffered ATIM input paths.
//! Each macro expansion uses its own typed PAC. No register layout casts.
use super::{
    Channel, CounterRegisters,
    input_capture::ChannelConfig,
    qei::{Direction, QeiMode},
};
#[cfg(gtim_buffered)]
use crate::pac;

pub(crate) trait InputRegisters: CounterRegisters {
    fn input_initialize(self);
    fn input_configure(self, channel: Channel, config: ChannelConfig);
    fn input_enable(
        self,
        channel: Channel,
        enabled: bool,
        mode: super::low_level::InputCaptureMode,
    );
    fn input_start(self);
    fn input_enabled(self, channel: Channel) -> bool;
    fn capture_pending(self, channel: Channel) -> bool;
    fn capture_value(self, channel: Channel) -> u16;
    #[cfg(gtim_buffered)]
    fn overcapture_pending(self, channel: Channel) -> bool;
    #[cfg(gtim_buffered)]
    fn clear_overcapture(self, channel: Channel);
    #[cfg(gtim_classic)]
    fn clear_capture(self, channel: Channel);
    #[cfg(gtim_classic)]
    fn encoder_underflow(self) -> bool;
    #[cfg(gtim_classic)]
    fn encoder_clear_underflow(self);
    fn encoder_mode(self, mode: QeiMode);
    fn encoder_direction(self) -> Direction;
}

// The shared IP fields have been checked against each family's own manual/SDK.
// ATIM's additional ICR flags must also be preserved by an R1W0 acknowledgement.
#[cfg(gtim_buffered)]
macro_rules! impl_input_registers {
    ($ty:ty, $($extra_flag:ident),*) => {
        impl InputRegisters for $ty {
            fn input_initialize(self) {
                self.ccer().write(|_| {});
                self.ccmr1cap().write(|v| {
                    v.set_cc1s(1);
                    v.set_cc2s(1);
                });
                self.ccmr2cap().write(|v| {
                    v.set_cc3s(1);
                    v.set_cc4s(1);
                });
                // Timer initialization already selects each external CHy input,
                // CKD=0 (fDTS=PCLK), no XOR, no slave mode, no requests.
            }
            fn input_configure(self, channel: Channel, config: ChannelConfig) {
                let enabled = self.input_enabled(channel);
                self.input_enable(channel, false, config.mode);
                // CCyS can only be changed while CCyE=0. Disabling also resets
                // the capture prescaler; it leaves CNT and pending flags intact.
                match channel {
                    Channel::Ch1 => self.ccmr1cap().modify(|v| {
                        v.set_cc1s(1); v.set_ic1f(config.filter as u8); v.set_ic1psc(config.prescaler);
                    }),
                    Channel::Ch2 => self.ccmr1cap().modify(|v| {
                        v.set_cc2s(1); v.set_ic2f(config.filter as u8); v.set_ic2psc(config.prescaler);
                    }),
                    Channel::Ch3 => self.ccmr2cap().modify(|v| {
                        v.set_cc3s(1); v.set_ic3f(config.filter as u8); v.set_ic3psc(config.prescaler);
                    }),
                    Channel::Ch4 => self.ccmr2cap().modify(|v| {
                        v.set_cc4s(1); v.set_ic4f(config.filter as u8); v.set_ic4psc(config.prescaler);
                    }),
                }
                let p = !matches!(config.mode, super::low_level::InputCaptureMode::Rising);
                let np = matches!(config.mode, super::low_level::InputCaptureMode::BothEdges);
                self.ccer().modify(|v| match channel {
                    Channel::Ch1 => { v.set_cc1p(p); v.set_cc1np(np); },
                    Channel::Ch2 => { v.set_cc2p(p); v.set_cc2np(np); },
                    Channel::Ch3 => { v.set_cc3p(p); v.set_cc3np(np); },
                    Channel::Ch4 => { v.set_cc4p(p); v.set_cc4np(np); },
                });
                self.input_enable(channel, enabled, config.mode);
            }
            fn input_start(self) { self.cr1().modify(|v| v.set_cen(true)); }
            fn input_enable(self, channel: Channel, enabled: bool, _mode: super::low_level::InputCaptureMode) {
                self.ccer().modify(|v| match channel {
                    Channel::Ch1 => v.set_cc1e(enabled),
                    Channel::Ch2 => v.set_cc2e(enabled),
                    Channel::Ch3 => v.set_cc3e(enabled),
                    Channel::Ch4 => v.set_cc4e(enabled),
                });
            }
            fn input_enabled(self, channel: Channel) -> bool {
                let v = self.ccer().read();
                match channel {
                    Channel::Ch1 => v.cc1e(), Channel::Ch2 => v.cc2e(),
                    Channel::Ch3 => v.cc3e(), Channel::Ch4 => v.cc4e(),
                }
            }
            fn capture_pending(self, channel: Channel) -> bool {
                let v = self.isr().read();
                match channel {
                    Channel::Ch1 => v.cc1if(), Channel::Ch2 => v.cc2if(),
                    Channel::Ch3 => v.cc3if(), Channel::Ch4 => v.cc4if(),
                }
            }
            fn capture_value(self, channel: Channel) -> u16 {
                // Reading CCR acknowledges CCyIF on this IP. There must be no
                // subsequent software CCyIF clear, which could lose a new edge.
                match channel {
                    Channel::Ch1 => self.ccr1().read().ccr1(),
                    Channel::Ch2 => self.ccr2().read().ccr2(),
                    Channel::Ch3 => self.ccr3().read().ccr3(),
                    Channel::Ch4 => self.ccr4().read().ccr4(),
                }
            }
            fn overcapture_pending(self, channel: Channel) -> bool {
                let v = self.isr().read();
                match channel {
                    Channel::Ch1 => v.cc1of(), Channel::Ch2 => v.cc2of(),
                    Channel::Ch3 => v.cc3of(), Channel::Ch4 => v.cc4of(),
                }
            }
            fn clear_overcapture(self, channel: Channel) {
                // R1W0: write ones to every other defined flag, with reserved
                // fields zero. Never read/modify/write asynchronous status.
                self.icr().write(|v| {
                    v.set_uif(true); v.set_cc1if(true); v.set_cc2if(true);
                    v.set_cc3if(true); v.set_cc4if(true); v.set_tif(true);
                    v.set_cc1of(channel != Channel::Ch1);
                    v.set_cc2of(channel != Channel::Ch2);
                    v.set_cc3of(channel != Channel::Ch3);
                    v.set_cc4of(channel != Channel::Ch4);
                    v.set_idxf(true); v.set_dirf(true); v.set_ierrf(true); v.set_terrf(true);
                    $(v.$extra_flag(true);)*
                });
            }
            fn encoder_mode(self, mode: QeiMode) {
                self.smcr().modify(|v| {
                    v.set_smsh(false);
                    v.set_sms(match mode { QeiMode::Mode1 => 1, QeiMode::Mode2 => 2, QeiMode::Mode3 => 3 });
                });
            }
            fn encoder_direction(self) -> Direction {
                if self.cr1().read().dir() { Direction::Downcounting } else { Direction::Upcounting }
            }
        }
    };
}
#[cfg(gtim_buffered)]
impl_input_registers!(pac::gtim::Gtim,);
#[cfg(gtim_buffered)]
impl_input_registers!(
    pac::atim::Atim,
    set_comif,
    set_bif,
    set_b2if,
    set_sbif,
    set_cc5if,
    set_cc6if,
    set_cc5of,
    set_cc6of
);
