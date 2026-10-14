mod sealed {
    pub trait Sealed {}
}
/// Sealed SPI communication mode. Only master operation is implemented.
pub trait CommunicationMode: sealed::Sealed {}
/// SPI master.
pub struct Master;
impl sealed::Sealed for Master {}
impl CommunicationMode for Master {}
