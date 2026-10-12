#![no_std]
#![no_main]

use cw32_uart_buffered_example as examples;
use embassy_cw32 as hal;

#[embassy_executor::main]
async fn main(_spawner: embassy_executor::Spawner) {
    let p = hal::init(Default::default());
    let ring =
        cortex_m::singleton!(: [u8; examples::RING_SIZE] = [0; examples::RING_SIZE]).unwrap();
    let mut uart = examples::uart(p, ring);
    loop {
        let mut chunk = [0u8; examples::CHUNK_SIZE];
        // IRQ collection continues while this exact read or the echo is pending.
        match uart.read(&mut chunk).await {
            Ok(()) => uart.write(&chunk).await.unwrap(),
            Err(error) => {
                // Exact-read errors carry no count; discard the partial chunk.
                core::hint::black_box(error);
                uart.write(b"receive error\r\n").await.unwrap();
            }
        }
        uart.flush().await.unwrap();
    }
}
