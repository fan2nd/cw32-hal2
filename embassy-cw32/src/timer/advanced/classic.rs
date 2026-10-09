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
impl Registers for pac::atim::Atim {
    const CHANNELS: usize = 3;
    // Bit1 is reserved reset-one and is preserved by every R1W0 clear.
    const FLAGS: u32 = 0x0007_ffff;
    fn control(self) -> u32 {
        self.cr().read().0
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
    fn status(self) -> u32 {
        self.isr().read().0
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
