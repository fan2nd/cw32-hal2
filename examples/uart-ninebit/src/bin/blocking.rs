#![no_std]
#![no_main]

use cw32_uart_ninebit_example as examples;
use embassy_cw32::{self as hal, usart};

#[cortex_m_rt::entry]
fn main() -> ! {
    let p = hal::init(Default::default());
    let mut uart = usart::Uart::new_blocking(p.UART1, p.PA2, p.PA3, examples::config()).unwrap();
    // One frame at a time avoids deliberately queuing a burst before reading.
    loop {
        for word in [0x000u16, 0x100, 0x1ff, 0x055] {
            uart.blocking_write_u16(&[word]).unwrap();
            uart.blocking_flush().unwrap();
            let mut received = [0u16; 1];
            uart.blocking_read_u16(&mut received).unwrap();
            core::hint::black_box(received);
        }
    }
}
