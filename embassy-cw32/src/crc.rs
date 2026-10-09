//! Hardware cyclic redundancy check engine with fixed algorithm presets.
//!
//! All supported families implement CRC16. F002/F003 implement only the four
//! polynomial-0x1021 presets (MODE 4–7). F030/A030 additionally implement CRC32.
//! The default is CRC32 on F030/A030 and CCITT on CRC16-only families.
//!
//! F020/F030/A030 accept native 8-, 16- and 32-bit input transactions, processing
//! the low byte first. The other families accept one byte per write, using the
//! SDK's documented word-sized store containing a zero-extended byte. Native
//! halfword/word methods are absent on those families. Reading a result does not
//! reset the calculation. No programmable polynomial, custom seed, DMA or IRQ
//! API is provided. In particular L052's undocumented INIT register is unused.
//!
//! Construction checks the peripheral reset and clock state, then writes CR to
//! initialize the selected preset. It never pulses SYSCTRL peripheral reset.
//! Drop disables the CRC clock; the exclusive peripheral borrow is retained for
//! the driver's lifetime. See `docs/crc-evidence.md` for the source boundaries.

use crate::rcc::{Readback, SealedRccPeripheral};
use crate::{Peri, pac, peripherals::CRC};

/// The fixed CRC algorithms implemented by the selected peripheral.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Mode {
    /// CRC-16/ARC (IBM): polynomial 0x8005, seed 0, reflected, XOR-out 0.
    #[cfg(crc_poly_8005)]
    Ibm = 0,
    /// CRC-16/MAXIM-DOW: polynomial 0x8005, seed 0, reflected, XOR-out 0xffff.
    #[cfg(crc_poly_8005)]
    Maxim = 1,
    /// CRC-16/USB: polynomial 0x8005, seed 0xffff, reflected, XOR-out 0xffff.
    #[cfg(crc_poly_8005)]
    Usb = 2,
    /// CRC-16/MODBUS: polynomial 0x8005, seed 0xffff, reflected, XOR-out 0.
    #[cfg(crc_poly_8005)]
    Modbus = 3,
    /// Vendor CRC16_CCITT: polynomial 0x1021, seed 0, reflected, XOR-out 0.
    Ccitt = 4,
    /// CRC-16/CCITT-FALSE: polynomial 0x1021, seed 0xffff, not reflected, XOR-out 0.
    CcittFalse = 5,
    /// CRC-16/X-25: polynomial 0x1021, seed 0xffff, reflected, XOR-out 0xffff.
    X25 = 6,
    /// CRC-16/XMODEM: polynomial 0x1021, seed 0, not reflected, XOR-out 0.
    Xmodem = 7,
    /// CRC-32: polynomial 0x04c11db7, seed 0xffffffff, reflected, XOR-out 0xffffffff.
    #[cfg(crc_32bit)]
    Crc32 = 8,
    /// CRC-32/MPEG-2: polynomial 0x04c11db7, seed 0xffffffff, not reflected, XOR-out 0.
    #[cfg(crc_32bit)]
    Mpeg2 = 9,
}

/// Configuration for the hardware CRC engine.
#[derive(Clone, Copy, Debug)]
pub struct Config {
    /// Algorithm preset, including fixed initial value and reflection settings.
    pub mode: Mode,
}
impl Default for Config {
    fn default() -> Self {
        #[cfg(crc_32bit)]
        let mode = Mode::Crc32;
        #[cfg(not(crc_32bit))]
        let mode = Mode::Ccitt;
        Self { mode }
    }
}

/// A CRC startup failure; no CRC data or control register has been written.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[non_exhaustive]
pub enum Error {
    /// SYSCTRL holds CRC in reset. Release it before constructing this driver.
    HeldInReset,
    /// The CRC clock-enable bit did not read back as enabled.
    ClockNotEnabled,
}

/// Exclusive hardware CRC driver, following Embassy ownership and feed/reset API.
pub struct Crc<'d> {
    _peripheral: Peri<'d, CRC>,
    config: Config,
}
impl<'d> Crc<'d> {
    /// Enable the peripheral clock and initialize the selected preset.
    ///
    /// Panics if the clock is inaccessible or CRC is held in reset. Use
    /// [`Self::try_new`] for checked startup.
    pub fn new(peripheral: Peri<'d, CRC>, config: Config) -> Self {
        Self::try_new(peripheral, config).expect("CRC startup failed")
    }

    /// Check reset/clock state, enable the clock and initialize the preset.
    ///
    /// The typed [`Mode`] exposes only algorithms documented for this chip, so
    /// there is no unchecked numeric algorithm to validate after MMIO starts.
    /// This does not release or pulse peripheral reset and does not use INIT.
    pub fn try_new(peripheral: Peri<'d, CRC>, config: Config) -> Result<Self, Error> {
        critical_section::with(|cs| {
            let rcc = CRC::RCC_INFO;
            if rcc.reset_asserted() {
                return Err(Error::HeldInReset);
            }
            rcc.enable_with_cs_readback(cs, Readback::None)
                .expect("unpolled CRC gate write cannot fail");
            // Synchronize and check the clock-enable write before CRC access.
            if !rcc.is_enabled() {
                return Err(Error::ClockNotEnabled);
            }
            Ok(())
        })?;
        pac::CRC.cr().write(|w| w.set_mode(config.mode as u8));
        Ok(Self {
            _peripheral: peripheral,
            config,
        })
    }

    /// Reset the accumulator to the selected algorithm's fixed initial state.
    ///
    /// Always writes CR, even when MODE already contains the selected value.
    pub fn reset(&mut self) {
        pac::CRC.cr().write(|w| w.set_mode(self.config.mode as u8));
    }

    /// Feed one byte without resetting the current calculation.
    ///
    /// Uses an 8-bit bus store on F020/F030/A030. Other families use a 32-bit
    /// bus store with only its low eight bits populated, consuming one byte.
    pub fn feed_byte(&mut self, byte: u8) {
        #[cfg(crc_input_16bit)]
        pac::CRC.dr8().write(|w| w.0 = byte);
        #[cfg(not(crc_input_16bit))]
        pac::CRC.dr().write(|w| w.set_dr(byte));
    }

    /// Feed a byte slice without resetting the current calculation.
    pub fn feed_bytes(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.feed_byte(byte);
        }
    }

    /// Feed a halfword using one native 16-bit transaction, low byte first.
    /// Available only on F020/F030/A030.
    #[cfg(crc_input_16bit)]
    pub fn feed_halfword(&mut self, halfword: u16) {
        pac::CRC.dr16().write(|w| w.0 = halfword);
    }

    /// Feed a word using one native 32-bit transaction, low byte first.
    /// Available only on F020/F030/A030.
    #[cfg(crc_input_32bit)]
    pub fn feed_word(&mut self, word: u32) {
        pac::CRC.dr32().write(|w| w.0 = word);
    }

    /// Read the current result without resetting it, zero-extending CRC16.
    pub fn read(&self) -> u32 {
        #[cfg(crc_32bit)]
        if matches!(self.config.mode, Mode::Crc32 | Mode::Mpeg2) {
            return pac::CRC.result32().read().0;
        }
        #[cfg(not(crc_32bit))]
        let _ = self.config.mode;
        #[cfg(crc_input_16bit)]
        let result = pac::CRC.result16().read().0;
        #[cfg(any(cw32f002, cw32f003))]
        let result = pac::CRC.result().read().result16();
        #[cfg(not(any(crc_input_16bit, cw32f002, cw32f003)))]
        let result = pac::CRC.result().read().result();
        u32::from(result)
    }
}
impl Drop for Crc<'_> {
    fn drop(&mut self) {
        critical_section::with(|cs| {
            CRC::RCC_INFO
                .disable_with_cs_readback(cs, Readback::None)
                .expect("unpolled CRC gate write cannot fail");
        });
    }
}
