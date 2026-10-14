use super::*;

/// Dedicated SPI interrupt handler for the constructor-selected async backend.
/// DMA owners also require both DMA channel handlers. Each backend checks its
/// own active state before MMIO; an inactive backend never touches the bank.
/// L012 SPI2/SPI3 constructors require the complete `SharedInterruptHandler`
/// instead. The ISR never reads DR or stores a caller-buffer/future pointer.
#[cfg(any(spi_async_pio, spi_dma))]
pub struct InterruptHandler<T: Instance>(PhantomData<T>);

#[cfg(any(spi_async_pio, spi_dma))]
impl<T: Instance> Handler<T::Interrupt> for InterruptHandler<T> {
    unsafe fn on_interrupt() {
        #[cfg(spi_async_pio)]
        dispatch_pio_interrupt(T::regs(), T::pio_state());
        #[cfg(spi_dma)]
        dispatch_dma_interrupt(T::regs(), T::state());
    }
}

/// Complete L012 SPI2/SPI3 handler. Bind it once to the actual `SPI23` vector;
/// either instance's PIO or DMA constructor accepts that binding. Each peer's
/// PIO and DMA state is visited; inactive states are skipped before MMIO, and
/// one owner's teardown never disables the peer's IRQ. DMA owners additionally
/// require both of their DMA channel handlers.
/// An externally owned inactive peer must have its interrupt source quiescent.
#[cfg(all(spi_cw32l012_v1, any(spi_async_pio, spi_dma)))]
pub struct SharedInterruptHandler<I: Interrupt>(PhantomData<I>);

#[cfg(all(spi_cw32l012_v1, any(spi_async_pio, spi_dma)))]
impl Handler<crate::interrupt::typelevel::SPI23>
    for SharedInterruptHandler<crate::interrupt::typelevel::SPI23>
{
    unsafe fn on_interrupt() {
        #[cfg(spi_async_pio)]
        dispatch_pio_interrupt(
            pac::SPI2,
            <crate::peripherals::SPI2 as sealed::Instance>::pio_state(),
        );
        #[cfg(spi_dma)]
        dispatch_dma_interrupt(
            pac::SPI2,
            <crate::peripherals::SPI2 as sealed::Instance>::state(),
        );
        #[cfg(spi_async_pio)]
        dispatch_pio_interrupt(
            pac::SPI3,
            <crate::peripherals::SPI3 as sealed::Instance>::pio_state(),
        );
        #[cfg(spi_dma)]
        dispatch_dma_interrupt(
            pac::SPI3,
            <crate::peripherals::SPI3 as sealed::Instance>::state(),
        );
    }
}
