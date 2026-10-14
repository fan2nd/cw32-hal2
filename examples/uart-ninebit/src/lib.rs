#![no_std]
use embassy_cw32::{self as hal, usart};

hal::bind_interrupts!(pub struct Irqs {
    UART1 => usart::InterruptHandler<hal::peripherals::UART1>;
});

pub fn config() -> usart::Config {
    let mut config = usart::Config::default();
    config.data_bits = usart::DataBits::DataBits9;
    config
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    cortex_m::interrupt::disable();
    loop {
        cortex_m::asm::wfi();
    }
}
