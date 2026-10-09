//! L083 polling AES block engine with explicit hardware-word ordering.
//!
//! Word 0 maps to DATA0/KEY0, the documented least-significant 32 bits.
//! This API deliberately makes no standard byte-array interoperability claim:
//! the vendor example proves only a round-trip, not a known-answer byte mapping.
//! Only independent ECB blocks are supported. There is no chaining, padding,
//! authentication, DMA, IRQ or async interface. ECB is not a message protocol.
//!
//! Poll budgets count CR reads, not elapsed time. Timeout never aborts, resets,
//! gates off, or reports output. Busy engines cannot be overwritten. Drop clears
//! all writable key/data registers only if idle, then disables the dedicated
//! gate. If still busy, Drop retains the gate and hardware key/data; call
//! `wait_idle` successfully before dropping to obtain register clearing.
//! Caller-owned key memory is borrowed for this driver's lifetime and is never
//! erased by the driver. No side-channel resistance or certified erasure claim
//! is made. See `docs/l083-aes-trng-evidence.md`.

use crate::{Peri, PeripheralType, crypto_geometry::*, pac, rcc::RccPeripheral};
use pac::aes::vals::{KeySize, Mode, Operation};

/// Four hardware words, DATA0 first (bits31:0), then DATA1..3.
pub type Block = [u32; AES_BLOCK_WORDS];

/// Borrowed hardware-order key. KEY0 contains the least-significant 32 bits.
/// No hidden host-side copy, byte-order conversion, or host zeroization occurs.
pub enum Key<'k> {
    Bits128(&'k [u32; AES_KEY_WORDS_128]),
    Bits192(&'k [u32; AES_KEY_WORDS_192]),
    Bits256(&'k [u32; AES_KEY_WORDS_256]),
}
impl Key<'_> {
    fn size(&self) -> KeySize {
        match self {
            Self::Bits128(_) => KeySize::Bits128,
            Self::Bits192(_) => KeySize::Bits192,
            Self::Bits256(_) => KeySize::Bits256,
        }
    }
    fn words(&self) -> &[u32] {
        match self {
            Self::Bits128(w) => *w,
            Self::Bits192(w) => *w,
            Self::Bits256(w) => *w,
        }
    }
}

/// Startup, configuration or bounded-completion failure.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Error {
    HeldInReset,
    ClockNotEnabled,
    /// Prior hardware operation is active. No mode/key/data was overwritten.
    Busy,
    /// Zero is rejected before submitting an operation.
    InvalidPollBudget,
    /// Configuration did not read back. No operation was submitted.
    WriteFailure,
    /// Operation may still be active. No output was read or returned.
    Timeout,
}

trait SealedInstance {
    fn regs() -> pac::aes::Aes;
}
/// A source-qualified AES register block and its actual RCC owner.
#[allow(private_bounds)]
pub trait Instance: SealedInstance + PeripheralType + RccPeripheral {}
impl SealedInstance for crate::peripherals::AES {
    fn regs() -> pac::aes::Aes {
        pac::AES
    }
}
impl Instance for crate::peripherals::AES {}

/// Exclusive blocking hardware AES owner with a borrowed key.
pub struct Aes<'d, 'k, T: Instance> {
    _peripheral: Peri<'d, T>,
    key: Key<'k>,
}
impl<'d, 'k, T: Instance> Aes<'d, 'k, T> {
    /// Enable the dedicated gate without resetting existing hardware state.
    /// Refuses an asserted reset or active operation. A busy startup leaves the
    /// gate enabled. No key registers are touched until the first block call.
    pub fn new(peripheral: Peri<'d, T>, key: Key<'k>) -> Result<Self, Error> {
        if T::RCC_INFO.reset_asserted() {
            return Err(Error::HeldInReset);
        }
        critical_section::with(|cs| T::RCC_INFO.enable_with_cs(cs))
            .map_err(|_| Error::ClockNotEnabled)?;
        if !T::RCC_INFO.is_enabled() {
            return Err(Error::ClockNotEnabled);
        }
        if T::regs().cr().read().start() == Operation::Active {
            return Err(Error::Busy);
        }
        Ok(Self {
            _peripheral: peripheral,
            key,
        })
    }
    /// Read only the operation status, without acknowledging or reading output.
    pub fn is_busy(&self) -> bool {
        T::regs().cr().read().start() == Operation::Active
    }

    /// Wait at most `polls` status reads. Does not collect a timed-out result or
    /// erase the key. After success, Drop can clear registers and gate off.
    pub fn wait_idle(&mut self, polls: u32) -> Result<(), Error> {
        if polls == 0 {
            return Err(Error::InvalidPollBudget);
        }
        for _ in 0..polls {
            if !self.is_busy() {
                return Ok(());
            }
            core::hint::spin_loop();
        }
        Err(Error::Timeout)
    }
    /// Encrypt one independent block. An earlier timed-out result may be
    /// discarded only after hardware is idle. Input remains unchanged on error.
    pub fn encrypt_block(&mut self, input: &Block, polls: u32) -> Result<Block, Error> {
        self.process(input, Mode::Encrypt, polls)
    }
    /// Decrypt one independent block using the same original key; the manual
    /// does not prescribe STM32-style inverse-key preprocessing.
    pub fn decrypt_block(&mut self, input: &Block, polls: u32) -> Result<Block, Error> {
        self.process(input, Mode::Decrypt, polls)
    }
    fn process(&mut self, input: &Block, mode: Mode, polls: u32) -> Result<Block, Error> {
        if polls == 0 {
            return Err(Error::InvalidPollBudget);
        }
        if self.is_busy() {
            return Err(Error::Busy);
        }
        let r = T::regs();
        let size = self.key.size();
        // START is zero only while already idle; it is never used as an abort.
        r.cr().write(|w| {
            w.set_mode(mode);
            w.set_keysize(size);
        });
        let c = r.cr().read();
        if c.mode() != mode || c.keysize() != size {
            return Err(Error::WriteFailure);
        }
        let key = self.key.words();
        r.key0().write(|w| w.set_key0(key[0]));
        r.key1().write(|w| w.set_key1(key[1]));
        r.key2().write(|w| w.set_key2(key[2]));
        r.key3().write(|w| w.set_key3(key[3]));
        r.key4()
            .write(|w| w.set_key4(key.get(4).copied().unwrap_or(0)));
        r.key5()
            .write(|w| w.set_key5(key.get(5).copied().unwrap_or(0)));
        r.key6()
            .write(|w| w.set_key6(key.get(6).copied().unwrap_or(0)));
        r.key7()
            .write(|w| w.set_key7(key.get(7).copied().unwrap_or(0)));
        r.data0().write(|w| w.set_data0(input[0]));
        r.data1().write(|w| w.set_data1(input[1]));
        r.data2().write(|w| w.set_data2(input[2]));
        r.data3().write(|w| w.set_data3(input[3]));
        r.cr().write(|w| {
            w.set_mode(mode);
            w.set_keysize(size);
            w.set_start(Operation::Active);
        });
        self.wait_idle(polls)?;
        Ok([
            r.data0().read().data0(),
            r.data1().read().data1(),
            r.data2().read().data2(),
            r.data3().read().data3(),
        ])
    }
}
impl<T: Instance> Drop for Aes<'_, '_, T> {
    fn drop(&mut self) {
        if self.is_busy() {
            return;
        }
        let r = T::regs();
        // Volatile MMIO writes cannot be optimized out. Internal secret copies,
        // physical remanence and caller RAM are beyond this erasure statement.
        r.key0().write(|w| w.set_key0(0));
        r.key1().write(|w| w.set_key1(0));
        r.key2().write(|w| w.set_key2(0));
        r.key3().write(|w| w.set_key3(0));
        r.key4().write(|w| w.set_key4(0));
        r.key5().write(|w| w.set_key5(0));
        r.key6().write(|w| w.set_key6(0));
        r.key7().write(|w| w.set_key7(0));
        r.data0().write(|w| w.set_data0(0));
        r.data1().write(|w| w.set_data1(0));
        r.data2().write(|w| w.set_data2(0));
        r.data3().write(|w| w.set_data3(0));
        critical_section::with(|cs| {
            let _ = T::RCC_INFO.disable_with_cs(cs);
        });
    }
}
