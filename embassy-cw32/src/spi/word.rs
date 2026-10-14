use super::{Word, sealed};

impl sealed::Word for u8 {
    const BITS: u8 = 8;
    #[cfg(spi_dma)]
    fn as_bytes(words: &[Self]) -> Option<&[u8]> {
        Some(words)
    }
    #[cfg(spi_dma)]
    fn as_bytes_mut(words: &mut [Self]) -> Option<&mut [u8]> {
        Some(words)
    }
    fn into_u16(self) -> u16 {
        u16::from(self)
    }
    fn from_u16(value: u16) -> Self {
        value as Self
    }
}
impl Word for u8 {}
impl sealed::Word for u16 {
    const BITS: u8 = 16;
    fn into_u16(self) -> u16 {
        self
    }
    fn from_u16(value: u16) -> Self {
        value
    }
}
impl Word for u16 {}

macro_rules! word {
    ($name:ident, $storage:ty, $bits:literal) => {
        #[doc = concat!(stringify!($bits), "-bit SPI frame, stored in the low bits.")]
        #[repr(transparent)]
        #[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
        #[cfg_attr(feature = "defmt", derive(defmt::Format))]
        pub struct $name(pub $storage);
        impl super::sealed::Word for $name {
            const BITS: u8 = $bits;
            fn into_u16(self) -> u16 {
                (self.0 as u16) & ((1 << $bits) - 1)
            }
            fn from_u16(value: u16) -> Self {
                Self((value & ((1 << $bits) - 1)) as $storage)
            }
        }
        impl super::Word for $name {}
    };
}
word!(U4, u8, 4);
word!(U5, u8, 5);
word!(U6, u8, 6);
word!(U7, u8, 7);
word!(U9, u16, 9);
word!(U10, u16, 10);
word!(U11, u16, 11);
word!(U12, u16, 12);
word!(U13, u16, 13);
word!(U14, u16, 14);
word!(U15, u16, 15);
