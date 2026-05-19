#![no_std]
#![no_main]
#![feature(impl_trait_in_assoc_type, adt_const_params)]

mod config;
mod keyboard;
mod types;

use defmt_rtt as _;
use panic_probe as _;

use embassy_executor::Spawner;
use embassy_rp::bind_interrupts;
use embassy_rp::gpio::{AnyPin, Input, Pull};
use embassy_rp::peripherals::USB;
use embassy_rp::usb::{Driver, InterruptHandler};
use embassy_time::{Duration, Timer};
use embassy_usb::Builder;
use embassy_usb::class::hid::State as HidState;
use static_cell::StaticCell;

use crate::keyboard::KeyboardDriver;
use crate::types::{ButtonState, GamepadState, InputMode, SocdMode};

bind_interrupts!(struct Irqs {
    USBCTRL_IRQ => InterruptHandler<USB>;
});

static HID_STATE: StaticCell<HidState> = StaticCell::new();
static CONFIG_DESC: StaticCell<[u8; 256]> = StaticCell::new();
static BOS_DESC: StaticCell<[u8; 256]> = StaticCell::new();
static MSOS_DESC: StaticCell<[u8; 256]> = StaticCell::new();
static CONTROL_BUF: StaticCell<[u8; 64]> = StaticCell::new();

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());

    defmt::info!("Hitpad-RS Booting...");

    // All 30 pins are initialized as pull-up inputs. The reboot pin (if any) is read
    // alongside button pins. It simply isn't mapped to any ButtonState.
    let pins: [Input<'static>; 30] = [
        Input::new(p.PIN_0.into::<AnyPin>(), Pull::Up),
        Input::new(p.PIN_1.into::<AnyPin>(), Pull::Up),
        Input::new(p.PIN_2.into::<AnyPin>(), Pull::Up),
        Input::new(p.PIN_3.into::<AnyPin>(), Pull::Up),
        Input::new(p.PIN_4.into::<AnyPin>(), Pull::Up),
        Input::new(p.PIN_5.into::<AnyPin>(), Pull::Up),
        Input::new(p.PIN_6.into::<AnyPin>(), Pull::Up),
        Input::new(p.PIN_7.into::<AnyPin>(), Pull::Up),
        Input::new(p.PIN_8.into::<AnyPin>(), Pull::Up),
        Input::new(p.PIN_9.into::<AnyPin>(), Pull::Up),
        Input::new(p.PIN_10.into::<AnyPin>(), Pull::Up),
        Input::new(p.PIN_11.into::<AnyPin>(), Pull::Up),
        Input::new(p.PIN_12.into::<AnyPin>(), Pull::Up),
        Input::new(p.PIN_13.into::<AnyPin>(), Pull::Up),
        Input::new(p.PIN_14.into::<AnyPin>(), Pull::Up),
        Input::new(p.PIN_15.into::<AnyPin>(), Pull::Up),
        Input::new(p.PIN_16.into::<AnyPin>(), Pull::Up),
        Input::new(p.PIN_17.into::<AnyPin>(), Pull::Up),
        Input::new(p.PIN_18.into::<AnyPin>(), Pull::Up),
        Input::new(p.PIN_19.into::<AnyPin>(), Pull::Up),
        Input::new(p.PIN_20.into::<AnyPin>(), Pull::Up),
        Input::new(p.PIN_21.into::<AnyPin>(), Pull::Up),
        Input::new(p.PIN_22.into::<AnyPin>(), Pull::Up),
        Input::new(p.PIN_23.into::<AnyPin>(), Pull::Up),
        Input::new(p.PIN_24.into::<AnyPin>(), Pull::Up),
        Input::new(p.PIN_25.into::<AnyPin>(), Pull::Up),
        Input::new(p.PIN_26.into::<AnyPin>(), Pull::Up),
        Input::new(p.PIN_27.into::<AnyPin>(), Pull::Up),
        Input::new(p.PIN_28.into::<AnyPin>(), Pull::Up),
        Input::new(p.PIN_29.into::<AnyPin>(), Pull::Up),
    ];

    // Read pins once before USB is set up. If a boot override button is held, it wins over DEFAULT_MODE. First match wins.
    let active_mode = detect_boot_mode(&pins);
    defmt::info!("Active input mode: {}", mode_str(active_mode));

    let driver = Driver::new(p.USB, Irqs);
    let mut usb_config = embassy_usb::Config::new(0x1209, 0x0001);
    usb_config.manufacturer = Some("Hitpad-RS");
    usb_config.product = Some("Keyboard Mode");

    let mut builder = Builder::new(
        driver,
        usb_config,
        CONFIG_DESC.init([0; 256]),
        BOS_DESC.init([0; 256]),
        MSOS_DESC.init([0; 256]),
        CONTROL_BUF.init([0; 64]),
    );

    let mut keyboard = KeyboardDriver::new(&mut builder, HID_STATE.init(HidState::new()));
    let usb = builder.build();
    spawner.spawn(usb_task(usb).expect("Failed to spawn USB task"));

    loop {
        if let Some(reboot_idx) = config::REBOOT_PIN {
            if pins[reboot_idx as usize].is_low() {
                defmt::info!("Reboot pin triggered, resetting...");
                cortex_m::peripheral::SCB::sys_reset();
            }
        }

        let mut state = GamepadState::default();
        for (pin_idx, mapped_btn) in config::PROFILES[0].pin_map.iter().enumerate() {
            if let Some(btn) = mapped_btn {
                if pins[pin_idx].is_low() {
                    state.buttons |= ButtonState::from(*btn);
                }
            }
        }

        state.apply_socd::<{ SocdMode::Neutral }>();

        let report = keyboard.translate_state(&state);
        keyboard.write_report(report).await;

        Timer::after(Duration::from_millis(1)).await;
    }
}

/// Checks whether any boot override button is held at startup.
/// Iterates BOOT_OVERRIDES in order; first pressed button wins.
/// Falls back to DEFAULT_MODE if nothing is held.
fn detect_boot_mode(pins: &[Input<'static>; 30]) -> InputMode {
    for boot_override in config::BOOT_OVERRIDES {
        for (pin_idx, mapped_btn) in config::PROFILES[0].pin_map.iter().enumerate() {
            if let Some(btn) = mapped_btn {
                if *btn as u8 == boot_override.button as u8 && pins[pin_idx].is_low() {
                    return boot_override.mode;
                }
            }
        }
    }
    config::DEFAULT_MODE
}

fn mode_str(mode: InputMode) -> &'static str {
    match mode {
        InputMode::Keyboard => "Keyboard",
        InputMode::XInput => "XInput",
        InputMode::PS5 => "PS5",
    }
}

#[embassy_executor::task]
async fn usb_task(mut usb: embassy_usb::UsbDevice<'static, Driver<'static, USB>>) {
    usb.run().await;
}
