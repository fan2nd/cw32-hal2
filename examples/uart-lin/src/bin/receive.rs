//! Fixed-baud receive-only observations through an external LIN transceiver.
#![no_std]
#![no_main]
use embassy_cw32::{
    self as hal,
    usart::{self, LinBreakThreshold, LinReceiver},
};

hal::bind_interrupts!(struct Irqs {
    UART1 => usart::InterruptHandler<hal::peripherals::UART1>;
});
#[embassy_executor::main]
async fn main(_spawner: embassy_executor::Spawner) {
    let p = hal::init(Default::default());
    #[cfg(feature = "cw32l010f8p6")]
    let pin = p.PA0;
    #[cfg(any(
        feature = "cw32l011k8t6",
        feature = "cw32l012c8t6",
        feature = "cw32l012c8u6"
    ))]
    let pin = p.PA3;
    let mut config = usart::Config::default();
    config.baudrate = 19_200;
    let mut rx = LinReceiver::new(p.UART1, pin, Irqs, config, LinBreakThreshold::Bits11)
        .unwrap();
    loop {
        // Observe with a debugger. This example neither assembles frames nor
        // responds; an application must discard partial frames on any error.
        let _ = core::hint::black_box(rx.read_event().await);
    }
}
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
