//! Fixed test-network responder. Response timing and transceiver behavior require
//! board qualification; do not use this as a scheduled production LIN node.
#![no_std]
#![no_main]
use embassy_cw32::{
    self as hal,
    usart::{self, LinBreakThreshold, LinChecksum, LinIdentifier, LinResponse, LinRxEvent, LinUart},
};
hal::bind_interrupts!(struct Irqs {
    UART1 => usart::InterruptHandler<hal::peripherals::UART1>;
});
#[embassy_executor::main]
async fn main(_spawner: embassy_executor::Spawner) {
    let p = hal::init(Default::default());
    #[cfg(feature = "cw32l010f8p6")]
    let (tx_pin, rx_pin) = (p.PA1, p.PA0);
    #[cfg(any(
        feature = "cw32l011k8t6",
        feature = "cw32l012c8t6",
        feature = "cw32l012c8u6"
    ))]
    let (tx_pin, rx_pin) = (p.PA2, p.PA3);
    let mut config = usart::Config::default();
    config.baudrate = 19_200;
    let uart = LinUart::new(p.UART1, tx_pin, rx_pin, Irqs, config, LinBreakThreshold::Bits11)
        .unwrap();
    let (mut tx, mut rx) = uart.split();
    // The test network must assign this response exclusively to this node.
    let response = LinResponse::new(
        LinIdentifier::new(0x12).unwrap(), &[0x10, 0x20], LinChecksum::Enhanced,
    ).unwrap();
    // 0 waits for Break, 1 expects sync, 2 expects PID. Echoed response bytes
    // are ignored until another Break; no echo comparison/collision detection.
    let mut phase = 0;
    loop {
        match rx.read_event().await {
            Ok(LinRxEvent::Break) => phase = 1,
            Ok(LinRxEvent::Data(0x55)) if phase == 1 => phase = 2,
            Ok(LinRxEvent::Data(pid)) if phase == 2 => {
                phase = 0;
                if LinIdentifier::from_protected(pid) == Ok(response.identifier()) {
                    tx.write_response(&response).await.unwrap();
                    tx.flush().await.unwrap();
                }
            }
            _ => phase = 0,
        }
    }
}
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
