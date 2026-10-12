//! Own-manual-qualified classic ATIM register family. CH4 is internal only.
use super::{Channel, Prescaler, Registers, Timing, pac};
pub(super) const PRESCALERS: &[Prescaler] = &[
    Prescaler::Div1,
    Prescaler::Div2,
    Prescaler::Div4,
    Prescaler::Div8,
    Prescaler::Div16,
    Prescaler::Div32,
    Prescaler::Div64,
    Prescaler::Div256,
];
fn prescaler_bits(value: Prescaler) -> u32 {
    match value {
        Prescaler::Div256 => 7,
        _ => {
            assert!(value.bits() <= 6);
            u32::from(value.bits())
        }
    }
}
// Preserve only defined configuration fields, never a software command or a
// reserved bit read back from CR. Used for UIE and native input EN changes.
pub(super) fn command_safe_control(regs: pac::atim::Atim) -> pac::atim::regs::Cr {
    let old = regs.cr().read();
    let mut value = pac::atim::regs::Cr::default();
    value.set_en(old.en());
    value.set_comp(old.comp());
    value.set_ct(old.ct());
    value.set_pwm2s(old.pwm2s());
    value.set_prs(old.prs());
    value.set_bufpen(old.bufpen());
    value.set_uie(old.uie());
    value.set_mode(old.mode());
    value.set_oneshot(old.oneshot());
    value.set_occs(old.occs());
    value.set_urs(old.urs());
    value.set_tie(old.tie());
    value.set_bie(old.bie());
    value.set_cisa(old.cisa());
    value.set_occe(old.occe());
    value.set_dir(old.dir());
    value.set_ove(old.ove());
    value.set_unde(old.unde());
    value
}
impl Registers for pac::atim::Atim {
    const CHANNELS: usize = 3;
    // Bit1 is reserved reset-one and is preserved by every R1W0 clear.
    const FLAGS: u32 = 0x0007_ffff;
    fn control(self) -> u32 {
        command_safe_control(self).0
    }
    fn write_control(self, value: u32) {
        self.cr().write(|v| v.0 = value);
    }
    fn stopped_control(timing: Timing) -> u32 {
        (2 << 12) | (1 << 17) | (1 << 7) | (1 << 3) | (prescaler_bits(timing.prescaler) << 4)
    }
    fn initialize(self, timing: Timing) {
        let control = Self::stopped_control(timing);
        self.write_control(control);
        self.dtr().write(|v| v.0 = 0); // MOE, AOE, break, safety, VC and dead-time all disabled.
        self.fltr().write(|v| v.0 = 0); // All A/B outputs forced low, no inversion.
        self.mscr().write(|v| v.0 = 0); // PCLK, no slave/master/cascade; L052 reserved MSM stays zero.
        self.trig().write(|v| v.0 = 0); // No ADC trigger.
        self.rcr().write(|v| v.0 = 0); // Every overflow updates, no repetition masking.
        self.ch1cr().write(|v| v.0 = 1 << 6);
        self.ch2cr().write(|v| v.0 = 1 << 6);
        self.ch3cr().write(|v| v.0 = 1 << 6); // A preload; capture/IRQ/DMA/B preload all disabled.
        self.ch4cr().write(|v| v.0 = 0);
        self.ch1ccrb().write(|v| v.0 = 0);
        self.ch2ccrb().write(|v| v.0 = 0);
        self.ch3ccrb().write(|v| v.0 = 0);
        self.ch4ccr().write(|v| v.0 = 0);
        for channel in [Channel::Ch1, Channel::Ch2, Channel::Ch3] {
            self.write_compare(channel, 0);
        }
        self.write_counter(0);
        self.write_period(timing);
        self.update(control);
        self.clear_flags(2);
    }
    fn write_period(self, timing: Timing) {
        assert!(timing.period() >= 2);
        self.arr().write(|v| v.0 = u32::from(timing.reload));
    }
    fn update(self, control: u32) {
        self.write_control((control & !(1 << 17)) | (1 << 25));
        let _ = self.cr().read();
        self.write_control(control);
    }
    fn counter(self) -> u16 {
        self.cnt().read().cnt()
    }
    fn write_counter(self, value: u16) {
        self.cnt().write(|v| v.set_cnt(value));
    }
    fn clear_flags(self, value: u32) {
        self.icr().write(|v| v.0 = value);
    }
    fn disable_requests(self) {
        self.cr().write(|v| {
            v.0 = self.control()
                & !((1 << 10) | (1 << 19) | (1 << 20) | (1 << 28) | (1 << 29) | (7 << 24))
        });
        self.ch1cr().modify(|v| v.0 &= !0x0000_cf00);
        self.ch2cr().modify(|v| v.0 &= !0x0000_cf00);
        self.ch3cr().modify(|v| v.0 &= !0x0000_cf00);
        self.ch4cr().modify(|v| v.0 &= !6);
        self.trig().write(|v| v.0 = 0);
    }
    fn mode(self, channel: Channel) -> u8 {
        ((self.fltr().read().0 >> (channel.index() * 8)) & 7) as u8
    }
    fn write_mode(self, channel: Channel, mode: u8) {
        assert!(channel.index() < 3 && matches!(mode, 0 | 1 | 6 | 7));
        let shift = channel.index() * 8;
        self.fltr()
            .modify(|v| v.0 = (v.0 & !(15 << shift)) | (u32::from(mode) << shift));
    }
    fn write_compare(self, channel: Channel, value: u16) {
        match channel {
            Channel::Ch1 => self.ch1ccra().write(|v| v.0 = u32::from(value)),
            Channel::Ch2 => self.ch2ccra().write(|v| v.0 = u32::from(value)),
            Channel::Ch3 => self.ch3ccra().write(|v| v.0 = u32::from(value)),
            Channel::Ch4 => unreachable!(),
        }
    }
    fn enable_outputs(self) {
        self.dtr().write(|v| v.0 = 1 << 12);
    }
    fn disable_outputs(self) {
        self.dtr().write(|v| v.0 = 0);
    }
}

// These helpers operate on the selected classic ATIM PAC directly. They are
// deliberately separate from the four-channel GTIM/buffered input abstraction.
#[cfg(atim_classic_input)]
pub(crate) mod input {
    use super::{Timing, command_safe_control, pac, prescaler_bits};
    use crate::timer::{
        ClassicAtimChannel,
        input_capture::ClassicAtimChannelConfig,
        low_level::InputCaptureMode,
        qei::{ClassicAtimQeiConfig, ClassicAtimQeiMode, Direction},
    };

    fn input_control(timing: Timing) -> pac::atim::regs::Cr {
        let mut value = pac::atim::regs::Cr::default();
        value.set_prs(prescaler_bits(timing.prescaler) as u8);
        value.set_bufpen(true);
        value.set_mode(2);
        value.set_urs(true);
        // CT=0, continuous up-count, every request and software command off.
        value
    }

    pub(crate) fn classic_input_initialize(regs: pac::atim::Atim, timing: Timing) {
        // EN=0 alone cannot stop captures. Disable acceptance before input setup.
        classic_capture_disable_all(regs);
        regs.cr().write(|v| *v = input_control(timing));
        regs.mscr().write(|_| {}); // Direct IA1S/IB1S, SMS/TS=0; reserved MSM remains zero.
        regs.fltr().write(|_| {}); // Native filter zero, non-inverted inputs.
        regs.dtr().write(|_| {}); // No MOE/AOE, break, safety or dead time.
        regs.trig().write(|_| {}); // No ADC trigger.
        regs.rcr().write(|_| {}); // RCR and OV/UD masks zero.
        regs.ch4cr().write(|_| {});
        regs.ch4ccr().write(|v| v.set_ccr4(0));
        regs.cnt().write(|v| v.set_cnt(0));
        regs.arr().write(|v| v.set_arr(timing.reload));
        // A deliberate UG with URS cleared, while capture and all requests are off.
        regs.cr().write(|v| {
            *v = input_control(timing);
            v.set_urs(false);
            v.set_ug(true);
        });
        let _ = regs.cr().read();
        regs.cr().write(|v| *v = input_control(timing));
        regs.icr().write(|v| {
            *v = pac::atim::regs::Icr::write_noop();
            v.set_uif(false);
            v.set_c1af(false);
            v.set_c1bf(false);
            v.set_c2af(false);
            v.set_c2bf(false);
            v.set_c3af(false);
            v.set_c3bf(false);
            v.set_c1ae(false);
            v.set_c1be(false);
            v.set_c2ae(false);
            v.set_c2be(false);
            v.set_c3ae(false);
            v.set_c3be(false);
        });
    }

    pub(crate) fn classic_input_start(regs: pac::atim::Atim) {
        critical_section::with(|_| {
            let mut value = command_safe_control(regs);
            value.set_en(true);
            regs.cr().write_value(value);
        });
    }

    pub(crate) fn classic_input_stop(regs: pac::atim::Atim) {
        critical_section::with(|_| {
            let mut value = command_safe_control(regs);
            value.set_en(false);
            regs.cr().write_value(value);
        });
    }

    fn acceptance(config: ClassicAtimChannelConfig) -> u8 {
        if !config.enabled {
            return 0;
        }
        match config.mode {
            InputCaptureMode::Rising => 1,
            InputCaptureMode::Falling => 2,
            InputCaptureMode::BothEdges => 3,
        }
    }

    fn write_capture_pair(regs: pac::atim::Atim, pair: usize, a: u8, b: u8) {
        // Fresh typed writes retain the partner's intended BKS while keeping
        // IRQ/DMA, preload, compare selectors and CCGA/CCGB commands zero.
        match pair {
            0 => regs.ch1cr().write(|v| {
                v.set_csa(true);
                v.set_csb(true);
                v.set_bksa(a);
                v.set_bksb(b);
            }),
            1 => regs.ch2cr().write(|v| {
                v.set_csa(true);
                v.set_csb(true);
                v.set_bksa(a);
                v.set_bksb(b);
            }),
            2 => regs.ch3cr().write(|v| {
                v.set_csa(true);
                v.set_csb(true);
                v.set_bksa(a);
                v.set_bksb(b);
            }),
            _ => unreachable!(),
        }
    }

    pub(crate) fn classic_capture_set_channel(
        regs: pac::atim::Atim,
        channel: ClassicAtimChannel,
        a: ClassicAtimChannelConfig,
        b: ClassicAtimChannelConfig,
    ) {
        critical_section::with(|_| {
            let pair = channel.index() / 2;
            let selected_a = channel.index() % 2 == 0;
            let a_acceptance = acceptance(a);
            let b_acceptance = acceptance(b);
            write_capture_pair(
                regs,
                pair,
                if selected_a { 0 } else { a_acceptance },
                if selected_a { b_acceptance } else { 0 },
            );
            let filter = if selected_a { a.filter } else { b.filter } as u8;
            regs.fltr().modify(|v| match channel {
                ClassicAtimChannel::Ch1A => v.set_ocm1aflt1a(filter),
                ClassicAtimChannel::Ch1B => v.set_ocm1bflt1b(filter),
                ClassicAtimChannel::Ch2A => v.set_ocm2aflt2a(filter),
                ClassicAtimChannel::Ch2B => v.set_ocm2bflt2b(filter),
                ClassicAtimChannel::Ch3A => v.set_ocm3aflt3a(filter),
                ClassicAtimChannel::Ch3B => v.set_ocm3bflt3b(filter),
            });
            write_capture_pair(regs, pair, a_acceptance, b_acceptance);
        });
    }

    pub(crate) fn classic_capture_disable_all(regs: pac::atim::Atim) {
        for pair in 0..3 {
            write_capture_pair(regs, pair, 0, 0);
        }
    }

    pub(crate) fn classic_capture_value(regs: pac::atim::Atim, channel: ClassicAtimChannel) -> u16 {
        match channel {
            ClassicAtimChannel::Ch1A => regs.ch1ccra().read().ccr1a(),
            ClassicAtimChannel::Ch1B => regs.ch1ccrb().read().ccr1b(),
            ClassicAtimChannel::Ch2A => regs.ch2ccra().read().ccr2a(),
            ClassicAtimChannel::Ch2B => regs.ch2ccrb().read().ccr2b(),
            ClassicAtimChannel::Ch3A => regs.ch3ccra().read().ccr3a(),
            ClassicAtimChannel::Ch3B => regs.ch3ccrb().read().ccr3b(),
        }
    }

    pub(crate) fn classic_capture_pending(
        regs: pac::atim::Atim,
        channel: ClassicAtimChannel,
    ) -> bool {
        let flags = regs.isr().read();
        match channel {
            ClassicAtimChannel::Ch1A => flags.c1af(),
            ClassicAtimChannel::Ch1B => flags.c1bf(),
            ClassicAtimChannel::Ch2A => flags.c2af(),
            ClassicAtimChannel::Ch2B => flags.c2bf(),
            ClassicAtimChannel::Ch3A => flags.c3af(),
            ClassicAtimChannel::Ch3B => flags.c3bf(),
        }
    }

    pub(crate) fn classic_capture_clear_capture(
        regs: pac::atim::Atim,
        channel: ClassicAtimChannel,
    ) {
        regs.icr().write(|v| {
            *v = pac::atim::regs::Icr::write_noop();
            match channel {
                ClassicAtimChannel::Ch1A => v.set_c1af(false),
                ClassicAtimChannel::Ch1B => v.set_c1bf(false),
                ClassicAtimChannel::Ch2A => v.set_c2af(false),
                ClassicAtimChannel::Ch2B => v.set_c2bf(false),
                ClassicAtimChannel::Ch3A => v.set_c3af(false),
                ClassicAtimChannel::Ch3B => v.set_c3bf(false),
            }
        });
    }

    pub(crate) fn classic_capture_overcapture_pending(
        regs: pac::atim::Atim,
        channel: ClassicAtimChannel,
    ) -> bool {
        let flags = regs.isr().read();
        match channel {
            ClassicAtimChannel::Ch1A => flags.c1ae(),
            ClassicAtimChannel::Ch1B => flags.c1be(),
            ClassicAtimChannel::Ch2A => flags.c2ae(),
            ClassicAtimChannel::Ch2B => flags.c2be(),
            ClassicAtimChannel::Ch3A => flags.c3ae(),
            ClassicAtimChannel::Ch3B => flags.c3be(),
        }
    }

    pub(crate) fn classic_capture_clear_overcapture(
        regs: pac::atim::Atim,
        channel: ClassicAtimChannel,
    ) {
        regs.icr().write(|v| {
            *v = pac::atim::regs::Icr::write_noop();
            match channel {
                ClassicAtimChannel::Ch1A => v.set_c1ae(false),
                ClassicAtimChannel::Ch1B => v.set_c1be(false),
                ClassicAtimChannel::Ch2A => v.set_c2ae(false),
                ClassicAtimChannel::Ch2B => v.set_c2be(false),
                ClassicAtimChannel::Ch3A => v.set_c3ae(false),
                ClassicAtimChannel::Ch3B => v.set_c3be(false),
            }
        });
    }

    pub(crate) fn classic_qei_configure(regs: pac::atim::Atim, config: ClassicAtimQeiConfig) {
        // Encoder edges are selected by SMS. BKS=0 suppresses unused CCR capture;
        // CSA/CSB=1 preserves native filtered-input interpretation.
        classic_capture_disable_all(regs);
        regs.fltr().write(|v| {
            v.set_ocm1aflt1a(config.filter as u8);
            v.set_ocm1bflt1b(config.filter as u8);
            v.set_ccp1a(config.ch1a_inverted);
            v.set_ccp1b(config.ch1b_inverted);
        });
        regs.mscr().write(|v| {
            v.set_sms(match config.mode {
                ClassicAtimQeiMode::Ch1A => 4,
                ClassicAtimQeiMode::Ch1B => 5,
                ClassicAtimQeiMode::Both => 6,
            });
            // IA1S=IB1S=0 direct external phases. L052 reserved bit4 stays zero.
        });
    }

    pub(crate) fn classic_qei_start(regs: pac::atim::Atim) {
        classic_input_start(regs);
        // EN can reload ARR for down-counting. Final zero is deliberate; inputs
        // in this initialization observation gap are not retained history.
        regs.cnt().write(|v| v.set_cnt(0));
        regs.icr().write(|v| {
            *v = pac::atim::regs::Icr::write_noop();
            v.set_uif(false);
            v.set_ovf(false);
            v.set_undf(false);
        });
    }

    pub(crate) fn classic_qei_stop(regs: pac::atim::Atim) {
        classic_input_stop(regs);
        regs.mscr().write(|_| {});
        classic_capture_disable_all(regs);
    }

    pub(crate) fn classic_qei_direction(regs: pac::atim::Atim) -> Direction {
        if regs.cr().read().dir() {
            Direction::Downcounting
        } else {
            Direction::Upcounting
        }
    }

    pub(crate) fn classic_qei_overflow(regs: pac::atim::Atim) -> bool {
        regs.isr().read().ovf()
    }

    pub(crate) fn classic_qei_underflow(regs: pac::atim::Atim) -> bool {
        regs.isr().read().undf()
    }

    pub(crate) fn classic_qei_clear_overflow(regs: pac::atim::Atim) {
        regs.icr().write(|v| {
            *v = pac::atim::regs::Icr::write_noop();
            v.set_ovf(false);
        });
    }

    pub(crate) fn classic_qei_clear_underflow(regs: pac::atim::Atim) {
        regs.icr().write(|v| {
            *v = pac::atim::regs::Icr::write_noop();
            v.set_undf(false);
        });
    }
}
