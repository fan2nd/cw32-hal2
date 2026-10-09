#![no_std]
#![no_main]

use embassy_cw32::{
    self as hal, dma, peripherals,
    usart::{self, UartRx},
};
use embassy_executor::Spawner;

#[cfg(not(example_l083))]
hal::bind_interrupts!(struct Irqs {
    UART2 => usart::InterruptHandler<peripherals::UART2>;
    DMACH1 => dma::InterruptHandler<peripherals::DMA_CH1>;
});

#[cfg(example_l083)]
hal::bind_interrupts!(struct Irqs {
    UART2_UART5 => usart::SharedInterruptHandler<hal::interrupt::typelevel::UART2_UART5>;
    DMACH1 => dma::InterruptHandler<peripherals::DMA_CH1>;
});

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = hal::init(Default::default());
    let staging = cortex_m::singleton!(: [u8; 16] = [0; 16]).unwrap();
    #[cfg(not(example_l083))]
    let rx_pin = p.PA3;
    #[cfg(example_l083)]
    let rx_pin = p.PA7;
    let mut rx = UartRx::new_with_dma(
        p.UART2,
        rx_pin,
        p.DMA_CH1,
        Irqs,
        staging,
        usart::Config::default(),
    )
    .unwrap();
    loop {
        let mut packet = [0u8; 16];
        rx.read(&mut packet).await.unwrap();
        core::hint::black_box(packet);
    }
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    cortex_m::interrupt::disable();
    loop {
        cortex_m::asm::wfi();
    }
}
