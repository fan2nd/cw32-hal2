#![no_std]
#![no_main]

use cw32_uart_buffered_example as examples;
use embassy_cw32 as hal;

#[embassy_executor::main]
async fn main(_spawner: embassy_executor::Spawner) {
    let p = hal::init(Default::default());
    let ring =
        cortex_m::singleton!(: [u8; examples::RING_SIZE] = [0; examples::RING_SIZE]).unwrap();
    let mut rx = examples::receiver(p, ring);
    let mut chunk = [0u8; examples::CHUNK_SIZE];
    loop {
        // Arm before sending a short burst from the peer. A silent line need
        // not notify, and later traffic may overflow the ring during this wait.
        rx.wait_for_idle().await;
        // Inspect a bounded batch, including any retained error after data.
        // Bytes may span multiple bursts: idle supplies no buffer boundary.
        for _ in 0..=examples::RING_SIZE / examples::CHUNK_SIZE {
            match rx.blocking_read(&mut chunk) {
                Ok(0) => break,
                Ok(count) => {
                    core::hint::black_box(&chunk[..count]);
                }
                Err(error) => {
                    core::hint::black_box(error);
                }
            }
        }
    }
}
