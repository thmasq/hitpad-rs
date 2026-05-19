#![no_std]
#![no_main]
#![feature(impl_trait_in_assoc_type)]

mod config;
mod types;

use defmt_rtt as _;
use panic_probe as _;

use embassy_executor::Spawner;
use embassy_rp::bind_interrupts;
use embassy_rp::peripherals::USB;
use embassy_rp::usb::{Driver, InterruptHandler};
use embassy_time::{Duration, Timer};

bind_interrupts!(struct Irqs {
    USBCTRL_IRQ => InterruptHandler<USB>;
});

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_rp::init(Default::default());

    defmt::info!("Hitpad-RS Booting...");

    let _usb_driver = Driver::new(p.USB, Irqs);

    loop {
        // Read GPIOs here using our config.rs mappings

        // Execute SOCD cleaning here

        // Send USB report here

        // Sleep to maintain exactly 1000Hz (1ms) polling
        Timer::after(Duration::from_millis(1)).await;
    }
}
