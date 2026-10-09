//! Segment LCD on CW32L052/L083, with internal resistor bias and LSI clock.
//!
//! Every wired COM/SEG pad must be passed to the constructor. The declared
//! supply and panel limits are board assumptions, not measured voltages.
//! External bias networks, charge pumps, LSE setup, blink and DMA are not
//! supported. No ADC/BGR/reference settings are changed.
//!
//! Display RAM is live: `present` waits for a frame event then writes the
//! owned pixels. CW32 has no atomic update/latch handshake; a multiword update
//! can cross a scan boundary. A successful wait proves a frame event, not
//! analogue settling or that every new pixel has already been displayed.
use crate::{
    Peri, PeripheralType,
    gpio::{AnyPin, Flex, Pin, SealedPin},
    interrupt::typelevel::{Binding, Handler, Interrupt},
    pac,
    rcc::RccPeripheral,
};
use core::{cell::Cell, marker::PhantomData, num::NonZeroU32};
use critical_section::Mutex;

pub use pac::lcd::vals::{Bias, BiasSource, Duty, ScanFrequency};

/// Explicit board and glass assumptions. Include supply tolerance and transients.
#[derive(Clone, Copy, Debug)]
pub struct ElectricalConfig {
    /// Highest VDD the board will apply during display operation, in mV.
    pub maximum_supply_mv: u16,
    /// Panel manufacturer's allowed peak drive between a COM and SEG, in mV.
    /// Internal contrast has no source-qualified voltage transfer function;
    /// this must tolerate the entire declared VDD, even at weakest contrast.
    pub panel_maximum_drive_mv: u16,
}
/// Configuration has no default: duty, bias and voltage depend on the glass.
#[derive(Clone, Copy, Debug)]
pub struct Config {
    pub duty: Duty,
    /// Static duty requires Third (BIAS=0), as required by the manual.
    pub bias: Bias,
    /// Only InternalLowest/Low/Medium/Highest are supported.
    pub drive: BiasSource,
    /// Source-described 128/256/512/64 Hz scan choices at nominal LSI.
    /// LSI tolerance is not bounded by this API.
    pub scan: ScanFrequency,
    /// 0 is maximum contrast; 15 is minimum. This is not a voltage value.
    pub contrast: u8,
    pub electrical: ElectricalConfig,
    /// Bound for oscillator and frame polling. Counts register polls, not time.
    pub poll_budget: NonZeroU32,
}
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    ClockNotInitialized,
    ClockGate,
    HeldInReset,
    ClockTimeout,
    FrameTimeout,
    InvalidDuty,
    InvalidBias,
    UnsupportedBiasSource,
    InvalidContrast,
    InvalidSupply,
    PanelVoltageExceeded,
    MissingCommon,
    DuplicatePin,
    SegmentUsedAsCommon,
    NoSegments,
    UnownedPixel,
}
pub(crate) struct State {
    waiting: Mutex<Cell<bool>>,
    frame: Mutex<Cell<bool>>,
}
impl State {
    const fn new() -> Self {
        Self {
            waiting: Mutex::new(Cell::new(false)),
            frame: Mutex::new(Cell::new(false)),
        }
    }
}
static STATE: State = State::new();
pub(crate) mod sealed {
    use super::*;
    pub trait Instance: RccPeripheral {
        const RAM_REGISTERS: &'static [u8];
        const LSI_TYPICAL_HZ: u32;
    }
    pub trait ComPin<T: super::Instance, const N: u8>: Pin {}
    pub trait SegPin<T: super::Instance, const N: u8>: Pin {}
}
#[allow(private_bounds)]
pub trait Instance: PeripheralType + sealed::Instance + 'static {}
#[allow(private_bounds)]
pub trait ComPin<T: Instance, const N: u8>: Pin + sealed::ComPin<T, N> {}
#[allow(private_bounds)]
pub trait SegPin<T: Instance, const N: u8>: Pin + sealed::SegPin<T, N> {}

/// Type-erased, owned display pad with a generated COM or SEG route.
pub struct LcdPin<'d, T: Instance> {
    pin: Peri<'d, AnyPin>,
    is_com: bool,
    number: u8,
    _instance: PhantomData<T>,
}
impl<'d, T: Instance> LcdPin<'d, T> {
    pub fn com<const N: u8>(pin: Peri<'d, impl ComPin<T, N>>) -> Self {
        Self {
            pin: pin.into(),
            is_com: true,
            number: N,
            _instance: PhantomData,
        }
    }
    pub fn segment<const N: u8>(pin: Peri<'d, impl SegPin<T, N>>) -> Self {
        Self {
            pin: pin.into(),
            is_com: false,
            number: N,
            _instance: PhantomData,
        }
    }
}
/// Handles only the LCD source of AUTOTRIM_LCD. Bind any other enabled source
/// on the shared vector to its own handler too. The driver never disables NVIC.
pub struct InterruptHandler<T: Instance>(PhantomData<T>);
impl<T: Instance> Handler<crate::interrupt::typelevel::AUTOTRIM_LCD> for InterruptHandler<T> {
    unsafe fn on_interrupt() {
        critical_section::with(|cs| {
            if STATE.waiting.borrow(cs).get()
                && pac::LCD.cr1().read().ie()
                && pac::LCD.cr1().read().intf()
            {
                clear_frame();
                pac::LCD.cr1().modify(|w| w.set_ie(false));
                STATE.frame.borrow(cs).set(true);
            }
        });
    }
}
/// Blocking, buffered display. The hardware RAM itself has no shadow latch.
pub struct Lcd<'d, T: Instance, const N: usize> {
    _peri: Peri<'d, T>,
    _pins: [Flex<'d>; N],
    pixels: [u32; 14],
    masks: [u32; 14],
    polls: NonZeroU32,
}
impl<'d, T: Instance, const N: usize> Lcd<'d, T, N> {
    /// Configure and enable a blank display. Does not promise analogue settling.
    /// All pins connected to the panel must be supplied, including unused pixels.
    /// A binding is required for safe frame waits on the shared LCD vector.
    pub fn new(
        peri: Peri<'d, T>,
        pins: [LcdPin<'d, T>; N],
        _irq: impl Binding<crate::interrupt::typelevel::AUTOTRIM_LCD, InterruptHandler<T>>,
        config: Config,
    ) -> Result<Self, Error> {
        if crate::rcc::try_clocks().is_none() {
            return Err(Error::ClockNotInitialized);
        }
        let commons = match config.duty {
            Duty::Static => 1,
            Duty::Half => 2,
            Duty::Third => 3,
            Duty::Quarter => 4,
            Duty::Sixth => 6,
            Duty::Eighth => 8,
            _ => return Err(Error::InvalidDuty),
        };
        if config.duty == Duty::Static && config.bias != Bias::Third {
            return Err(Error::InvalidBias);
        }
        if !matches!(
            config.drive,
            BiasSource::InternalLowest
                | BiasSource::InternalLow
                | BiasSource::InternalMedium
                | BiasSource::InternalHighest
        ) {
            return Err(Error::UnsupportedBiasSource);
        }
        if config.contrast > 15 {
            return Err(Error::InvalidContrast);
        }
        let voltage = config.electrical;
        if !(crate::rcc::HSI_BOUND_SUPPLY_MV.0..=crate::rcc::HSI_BOUND_SUPPLY_MV.1)
            .contains(&voltage.maximum_supply_mv)
        {
            return Err(Error::InvalidSupply);
        }
        if voltage.panel_maximum_drive_mv < voltage.maximum_supply_mv {
            return Err(Error::PanelVoltageExceeded);
        }
        let mut common_mask = 0u8;
        let mut masks = [0u32; 14];
        for (i, pin) in pins.iter().enumerate() {
            if pins[..i].iter().any(|p| {
                p.pin.pin_port() == pin.pin.pin_port()
                    || (p.is_com == pin.is_com && p.number == pin.number)
            }) {
                return Err(Error::DuplicatePin);
            }
            if pin.is_com {
                if pin.number >= commons {
                    return Err(Error::InvalidDuty);
                }
                common_mask |= 1 << pin.number;
            } else {
                if (commons == 6 && (30..=31).contains(&pin.number))
                    || (commons == 8 && (28..=31).contains(&pin.number))
                {
                    return Err(Error::SegmentUsedAsCommon);
                }
                masks[usize::from(pin.number / 4)] |=
                    ((1u32 << commons) - 1) << ((pin.number % 4) * 8);
            }
        }
        if u16::from(common_mask) != (1u16 << commons) - 1 {
            return Err(Error::MissingCommon);
        }
        if masks.iter().all(|m| *m == 0) {
            return Err(Error::NoSegments);
        }
        if !crate::rcc::enable_lsi(config.poll_budget) {
            return Err(Error::ClockTimeout);
        }
        critical_section::with(|cs| {
            T::RCC_INFO
                .enable_with_cs(cs)
                .map_err(|_| Error::ClockGate)?;
            if T::RCC_INFO.reset_asserted() {
                return Err(Error::HeldInReset);
            }
            pac::LCD.cr0().modify(|w| w.set_en(false));
            pac::LCD.cr1().write(|w| {
                w.set_clkcs(pac::lcd::vals::ClockSource::Lsi);
                w.set_lcdfs(config.scan);
            });
            pac::LCD.pinen1().write(|_| {});
            pac::LCD.pinen2().write(|_| {});
            // Establish pump-off internal bias while every LCD pad is disabled,
            // before MUX or an owned pad can expose a previous configuration.
            pac::LCD.cr0().write(|w| {
                w.set_duty(config.duty);
                w.set_bias(config.bias);
                w.set_inrs(config.drive);
                w.set_contrast(config.contrast);
                w.set_bump(false);
                w.set_en(false);
            });
            STATE.waiting.borrow(cs).set(false);
            STATE.frame.borrow(cs).set(false);
            clear_frame();
            Ok(())
        })?;
        for n in T::RAM_REGISTERS {
            crate::lcd_write_word(*n, 0);
        }
        let mut mux = false;
        let owned = pins.map(|pin| {
            mux |= !pin.is_com && (24..=27).contains(&pin.number);
            let flex = Flex::new(pin.pin);
            crate::lcd_enable_pin(pin.is_com, pin.number);
            flex
        });
        pac::LCD.pinen2().modify(|w| w.set_mux(mux));
        pac::LCD.cr0().modify(|w| w.set_en(true));
        unsafe {
            crate::interrupt::typelevel::AUTOTRIM_LCD::enable();
        }
        Ok(Self {
            _peri: peri,
            _pins: owned,
            pixels: [0; 14],
            masks,
            polls: config.poll_budget,
        })
    }
    /// Change one owned COM/SEG crosspoint in the software buffer.
    pub fn set_pixel(&mut self, common: u8, segment: u8, on: bool) -> Result<(), Error> {
        if common >= 8 || segment >= 56 {
            return Err(Error::UnownedPixel);
        }
        let index = usize::from(segment / 4);
        let bit = 1u32 << ((segment % 4) * 8 + common);
        if self.masks[index] & bit == 0 {
            return Err(Error::UnownedPixel);
        }
        if on {
            self.pixels[index] |= bit;
        } else {
            self.pixels[index] &= !bit;
        }
        Ok(())
    }
    /// Read the software buffer, rather than an instantaneous LCD waveform.
    pub fn pixel(&self, common: u8, segment: u8) -> Result<bool, Error> {
        if common >= 8 || segment >= 56 {
            return Err(Error::UnownedPixel);
        }
        let index = usize::from(segment / 4);
        let bit = 1u32 << ((segment % 4) * 8 + common);
        if self.masks[index] & bit == 0 {
            return Err(Error::UnownedPixel);
        }
        Ok(self.pixels[index] & bit != 0)
    }
    /// Fill or blank every owned pixel in the software buffer.
    pub fn fill(&mut self, on: bool) {
        self.pixels = if on { self.masks } else { [0; 14] };
    }
    /// Immediately copy the software buffer to live display RAM, without a frame wait.
    /// Multiword writes are not atomic and may briefly display a mixed frame.
    pub fn write_display(&mut self) {
        for n in T::RAM_REGISTERS {
            crate::lcd_write_word(*n, self.pixels[usize::from(*n)]);
        }
    }
    /// Wait for a new frame and then copy the buffer. No atomic latch is available.
    pub fn present(&mut self) -> Result<(), Error> {
        self.wait_next_frame()?;
        self.write_display();
        Ok(())
    }
    /// Wait for a fresh frame event, with BLINKCNT=0 and a bounded poll count.
    /// LCD IE is enabled only during this call and serviced through the binding.
    pub fn wait_next_frame(&mut self) -> Result<(), Error> {
        critical_section::with(|cs| {
            clear_frame();
            STATE.frame.borrow(cs).set(false);
            STATE.waiting.borrow(cs).set(true);
            pac::LCD.cr1().modify(|w| w.set_ie(true));
        });
        let mut complete = false;
        for _ in 0..self.polls.get() {
            if critical_section::with(|cs| {
                STATE.frame.borrow(cs).get() || pac::LCD.cr1().read().intf()
            }) {
                complete = true;
                break;
            }
            core::hint::spin_loop();
        }
        critical_section::with(|cs| {
            pac::LCD.cr1().modify(|w| w.set_ie(false));
            STATE.waiting.borrow(cs).set(false);
            clear_frame();
        });
        if complete {
            Ok(())
        } else {
            Err(Error::FrameTimeout)
        }
    }
    /// Adjust internal resistor contrast. 0 is strongest, 15 weakest.
    /// No voltage value or analogue settling time is implied.
    pub fn set_contrast(&mut self, contrast: u8) -> Result<(), Error> {
        if contrast > 15 {
            return Err(Error::InvalidContrast);
        }
        pac::LCD.cr0().modify(|w| w.set_contrast(contrast));
        Ok(())
    }
    pub fn contrast(&self) -> u8 {
        pac::LCD.cr0().read().contrast()
    }
    /// Datasheet typical LSI frequency only; trim/tolerance may change actual rate.
    pub fn typical_source_frequency(&self) -> crate::time::Hertz {
        crate::time::Hertz(T::LSI_TYPICAL_HZ)
    }
}
impl<T: Instance, const N: usize> Drop for Lcd<'_, T, N> {
    fn drop(&mut self) {
        critical_section::with(|cs| {
            pac::LCD.cr1().modify(|w| w.set_ie(false));
            STATE.waiting.borrow(cs).set(false);
            pac::LCD.cr0().modify(|w| w.set_en(false));
            pac::LCD.pinen1().write(|_| {});
            pac::LCD.pinen2().write(|_| {});
            clear_frame();
        });
    }
}
fn clear_frame() {
    crate::lcd_clear_frame();
}
