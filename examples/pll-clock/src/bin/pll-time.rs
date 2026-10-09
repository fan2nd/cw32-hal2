#![no_std]
#![no_main]
use embassy_cw32::{
    self as hal,
    gpio::{Level, Output, Speed},
    usart,
};
use embassy_executor::Spawner;
use embassy_time::{Instant, Timer};

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    // Default /6 x7 = 56 MHz is exactly divisible to the 1 MHz timer kernel.
    // Fractional-MHz PLL choices are not necessarily admitted by this driver.
    let p = hal::init(cw32_pll_clock_examples::config());
    let mut led = Output::new(p.PB0, Level::Low, Speed::Default);
    let mut uart = usart::UartTx::new_blocking(p.UART1, p.PA8, usart::Config::default()).unwrap();
    core::hint::black_box(hal::time_driver::tick_bounds());
    loop {
        Timer::after_millis(250).await;
        core::hint::black_box(Instant::now());
        led.toggle();
        uart.blocking_write(b"PLL clock: Embassy timer tick\r\n")
            .unwrap();
        uart.blocking_flush().unwrap();
    }
}
