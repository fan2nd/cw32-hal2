#![no_std]
#![no_main]

use cw32_uart_half_duplex_example as examples;
use embassy_cw32::{self as hal, usart};

#[cortex_m_rt::entry]
fn main() -> ! {
    let p = hal::init(Default::default());
    let mut uart = usart::HalfDuplexUart::new_blocking(p.UART1, p.PA2, examples::config()).unwrap();
    // PA2 is the ONE bidirectional wire. Add an external pull-up and common
    // ground. The peer must wait for each request's final stop bit, send one
    // response byte, then release the line. Do not use TX/RX loopback wiring.
    // This example does not supply a peer or validate its turnaround timing.
    loop {
        uart.blocking_write(&[0x55]).unwrap();
        let mut response = [0];
        uart.blocking_read(&mut response).unwrap();
        core::hint::black_box(response);
    }
}
