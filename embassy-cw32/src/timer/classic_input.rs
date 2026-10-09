//! Source-qualified classic GTIM input operations, using the selected typed PAC.
//! CCR reads retain CCy; R1W0 commands acknowledge flags. No overcapture flag is documented.
use super::{
    Channel,
    input::InputRegisters,
    input_capture::ChannelConfig,
    low_level::InputCaptureMode,
    qei::{Direction, QeiMode},
};
use crate::pac::{self, gtim::vals::CcMode};

impl InputRegisters for pac::gtim::Gtim {
    fn input_initialize(self) {
        self.cmmr().write(|_| {});
        self.cr1().write(|_| {});
    }
    fn input_configure(self, channel: Channel, config: ChannelConfig) {
        let enabled = self.input_enabled(channel);
        self.input_enable(channel, false, config.mode);
        self.cr1().modify(|v| match channel {
            Channel::Ch1 => {
                v.set_ch1flt(config.filter as u8);
                v.set_ch1pol(false);
            }
            Channel::Ch2 => {
                v.set_ch2flt(config.filter as u8);
                v.set_ch2pol(false);
            }
            Channel::Ch3 => {
                v.set_ch3flt(config.filter as u8);
                v.set_ch3pol(false);
            }
            Channel::Ch4 => {
                v.set_ch4flt(config.filter as u8);
                v.set_ch4pol(false);
            }
        });
        self.input_enable(channel, enabled, config.mode);
    }
    fn input_enable(self, channel: Channel, enabled: bool, mode: InputCaptureMode) {
        let mode = if !enabled {
            CcMode::Disabled
        } else {
            match mode {
                InputCaptureMode::Rising => CcMode::CaptureRising,
                InputCaptureMode::Falling => CcMode::CaptureFalling,
                InputCaptureMode::BothEdges => CcMode::CaptureBoth,
            }
        };
        self.cmmr().modify(|v| match channel {
            Channel::Ch1 => v.set_cc1m(mode),
            Channel::Ch2 => v.set_cc2m(mode),
            Channel::Ch3 => v.set_cc3m(mode),
            Channel::Ch4 => v.set_cc4m(mode),
        });
    }
    fn input_start(self) {
        // Timer::start reconstructs timer-mode CR0. Preserve ENCMODE here.
        self.cr0().modify(|v| v.set_en(true));
    }
    fn input_enabled(self, channel: Channel) -> bool {
        let v = self.cmmr().read();
        let mode = match channel {
            Channel::Ch1 => v.cc1m(),
            Channel::Ch2 => v.cc2m(),
            Channel::Ch3 => v.cc3m(),
            Channel::Ch4 => v.cc4m(),
        };
        matches!(
            mode,
            CcMode::CaptureRising | CcMode::CaptureFalling | CcMode::CaptureBoth
        )
    }
    fn capture_pending(self, channel: Channel) -> bool {
        let v = self.isr().read();
        match channel {
            Channel::Ch1 => v.cc1(),
            Channel::Ch2 => v.cc2(),
            Channel::Ch3 => v.cc3(),
            Channel::Ch4 => v.cc4(),
        }
    }
    fn capture_value(self, channel: Channel) -> u16 {
        match channel {
            Channel::Ch1 => self.ccr1().read().ccr(),
            Channel::Ch2 => self.ccr2().read().ccr(),
            Channel::Ch3 => self.ccr3().read().ccr(),
            Channel::Ch4 => self.ccr4().read().ccr(),
        }
    }
    fn clear_capture(self, channel: Channel) {
        // The generated source-qualified no-op retains reserved reset bits7/8.
        // Never RMW asynchronous status or use the PAC's zero-default write.
        let mut command = pac::gtim::regs::Icr::write_noop();
        match channel {
            Channel::Ch1 => command.set_cc1(false),
            Channel::Ch2 => command.set_cc2(false),
            Channel::Ch3 => command.set_cc3(false),
            Channel::Ch4 => command.set_cc4(false),
        }
        self.icr().write_value(command);
    }
    fn encoder_mode(self, mode: QeiMode) {
        self.cr0().modify(|v| {
            v.set_encmode(match mode {
                QeiMode::Mode1 => 1,
                QeiMode::Mode2 => 2,
                QeiMode::Mode3 => 3,
            });
            v.set_encreset(0);
            v.set_encreload(0);
        });
    }
    fn encoder_direction(self) -> Direction {
        if self.isr().read().dir() {
            Direction::Downcounting
        } else {
            Direction::Upcounting
        }
    }
    fn encoder_underflow(self) -> bool {
        self.isr().read().ud()
    }
    fn encoder_clear_underflow(self) {
        let mut command = pac::gtim::regs::Icr::write_noop();
        command.set_ud(false);
        self.icr().write_value(command);
    }
}
