#![no_std]
#![no_main]
#![feature(impl_trait_in_assoc_type, adt_const_params)]
#![allow(clippy::future_not_send)]

mod config;
mod keyboard;
mod types;

use defmt_rtt as _;
use embassy_rp::config::Config;
use embassy_rp::multicore::Stack;
use panic_probe as _;

use cortex_m_rt::entry;
use embassy_executor::Executor;
use embassy_rp::bind_interrupts;
use embassy_rp::gpio::{AnyPin, Input, Pull};
use embassy_rp::pac;
use embassy_rp::peripherals::USB;
use embassy_rp::usb::{Driver, InterruptHandler};
use embassy_time::{Duration, Timer};
use embassy_usb::Builder;
use embassy_usb::class::hid::State as HidState;
use portable_atomic::{AtomicU32, Ordering};
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

static DEBOUNCED_STATE: AtomicU32 = AtomicU32::new(0);

static mut CORE1_STACK: Stack<4096> = Stack::new();
static EXECUTOR0: StaticCell<Executor> = StaticCell::new();
static EXECUTOR1: StaticCell<Executor> = StaticCell::new();

#[entry]
fn main() -> ! {
    let p = embassy_rp::init(Config::default());

    defmt::info!("Hitpad-RS Booting in Dual-Core Mode...");

    let _pins: [Input<'static>; 30] = [
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

    let initial_state = !pac::SIO.gpio_in(0).read() & 0x3FFF_FFFF;
    DEBOUNCED_STATE.store(initial_state, Ordering::Relaxed);

    embassy_rp::multicore::spawn_core1(
        p.CORE1,
        unsafe { &mut *core::ptr::addr_of_mut!(CORE1_STACK) },
        move || {
            let executor1 = EXECUTOR1.init(Executor::new());
            executor1.run(|spawner| {
                spawner.spawn(sampler_task(initial_state).unwrap());
            });
        },
    );

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

    let keyboard = KeyboardDriver::new(&mut builder, HID_STATE.init(HidState::new()));
    let usb = builder.build();

    let executor0 = EXECUTOR0.init(Executor::new());
    executor0.run(move |spawner| {
        spawner.spawn(usb_task(usb).unwrap());
        spawner.spawn(main_loop_task(keyboard, initial_state).unwrap());
    })
}

#[embassy_executor::task]
async fn main_loop_task(mut keyboard: KeyboardDriver<'static>, initial_state: u32) {
    let active_mode = detect_boot_mode(initial_state);
    defmt::info!("Active input mode: {}", mode_str(active_mode));

    loop {
        let debounced_state = DEBOUNCED_STATE.load(Ordering::Relaxed);

        if let Some(reboot_idx) = config::REBOOT_PIN
            && (debounced_state & (1 << reboot_idx)) != 0
        {
            defmt::info!("Reboot pin triggered, resetting...");
            cortex_m::peripheral::SCB::sys_reset();
        }

        let mut state = GamepadState::default();
        for (pin_idx, mapped_btn) in config::PROFILES[0].pin_map.iter().enumerate() {
            if let Some(btn) = mapped_btn
                && (debounced_state & (1 << pin_idx)) != 0
            {
                state.buttons |= ButtonState::from(*btn);
            }
        }

        state.apply_socd::<{ SocdMode::Neutral }>();

        let report = KeyboardDriver::<'static>::translate_state(state);
        keyboard.write_report(report).await;

        Timer::after(Duration::from_millis(1)).await;
    }
}

/// High-speed independent sampling task running on Core 1.
/// Runs every 50 microseconds to guarantee a 16-sample debounce resolves in 0.8ms.
#[embassy_executor::task]
async fn sampler_task(initial_state: u32) {
    let mut history = [0u32; 16];
    history.fill(initial_state);
    let mut history_idx = 0;
    let mut current_debounced = initial_state;

    loop {
        let raw_state = !pac::SIO.gpio_in(0).read() & 0x3FFF_FFFF;

        history[history_idx] = raw_state;
        history_idx = (history_idx + 1) % 16;

        let mut all_ones = 0xFFFF_FFFF;
        let mut all_zeros = 0xFFFF_FFFF;

        for state in &history {
            all_ones &= state;
            all_zeros &= !state;
        }

        current_debounced = (current_debounced | all_ones) & !all_zeros;

        DEBOUNCED_STATE.store(current_debounced, Ordering::Relaxed);

        Timer::after(Duration::from_micros(50)).await;
    }
}

/// Checks whether any boot override button is held at startup.
fn detect_boot_mode(raw_state: u32) -> InputMode {
    for boot_override in config::BOOT_OVERRIDES {
        for (pin_idx, mapped_btn) in config::PROFILES[0].pin_map.iter().enumerate() {
            if let Some(btn) = mapped_btn
                && *btn as u8 == boot_override.button as u8
                && (raw_state & (1 << pin_idx)) != 0
            {
                return boot_override.mode;
            }
        }
    }
    config::DEFAULT_MODE
}

const fn mode_str(mode: InputMode) -> &'static str {
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
