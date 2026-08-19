#![no_std]
#![no_main]
#![feature(impl_trait_in_assoc_type, adt_const_params)]
#![allow(clippy::future_not_send)]

mod config;
mod keyboard;
mod macros;
mod types;

use defmt_rtt as _;
use embassy_stm32::time::Hertz;
use panic_probe as _;

use cortex_m_rt::entry;
use embassy_executor::Executor;
use embassy_stm32::Config;
use embassy_stm32::bind_interrupts;
use embassy_stm32::pac;
use embassy_stm32::peripherals::{USB_OTG_FS, USB_OTG_HS};
use embassy_stm32::usb::{Driver, InterruptHandler};
use embassy_time::{Duration, Timer};
use embassy_usb::Builder;
use embassy_usb::Handler;
use embassy_usb::class::hid::State as HidState;
use embassy_usb::class::hid::{ReportId, RequestHandler};
use embassy_usb::control::OutResponse;
use portable_atomic::{AtomicU32, Ordering};
use static_cell::StaticCell;

use crate::keyboard::KeyboardDriver;
use crate::types::{ButtonState, GamepadState, InputMode, SocdMode};

bind_interrupts!(struct Irqs {
    OTG_FS => InterruptHandler<USB_OTG_FS>;
    OTG_HS => InterruptHandler<USB_OTG_HS>;
});

static EP_OUT_BUFFER_HS: StaticCell<[u8; 256]> = StaticCell::new();
static HID_STATE_HS: StaticCell<HidState> = StaticCell::new();
static CONFIG_DESC_HS: StaticCell<[u8; 256]> = StaticCell::new();
static BOS_DESC_HS: StaticCell<[u8; 256]> = StaticCell::new();
static MSOS_DESC_HS: StaticCell<[u8; 256]> = StaticCell::new();
static CONTROL_BUF_HS: StaticCell<[u8; 64]> = StaticCell::new();
static DEVICE_HANDLER_HS: StaticCell<MyDeviceHandler> = StaticCell::new();
static REQUEST_HANDLER_HS: StaticCell<MyRequestHandler> = StaticCell::new();

static EP_OUT_BUFFER_FS: StaticCell<[u8; 256]> = StaticCell::new();
static CONFIG_DESC_FS: StaticCell<[u8; 256]> = StaticCell::new();
static BOS_DESC_FS: StaticCell<[u8; 256]> = StaticCell::new();
static MSOS_DESC_FS: StaticCell<[u8; 256]> = StaticCell::new();
static CONTROL_BUF_FS: StaticCell<[u8; 64]> = StaticCell::new();
static DEVICE_HANDLER_FS: StaticCell<MyDeviceHandler> = StaticCell::new();

static DEBOUNCED_STATE: AtomicU32 = AtomicU32::new(0);
static EXECUTOR0: StaticCell<Executor> = StaticCell::new();

struct MyDeviceHandler {
    configured: bool,
}

impl MyDeviceHandler {
    fn new() -> Self {
        MyDeviceHandler { configured: false }
    }
}

impl Handler for MyDeviceHandler {
    fn enabled(&mut self, _enabled: bool) {
        self.configured = false;
        defmt::info!("Device enabled");
    }
    fn reset(&mut self) {
        self.configured = false;
        defmt::info!("Bus reset, the Vbus current limit is 100mA");
    }
    fn addressed(&mut self, addr: u8) {
        self.configured = false;
        defmt::info!("USB address set to: {}", addr);
    }
    fn configured(&mut self, configured: bool) {
        self.configured = configured;
        if configured {
            defmt::info!("Device configured");
        } else {
            defmt::info!("Device is no longer configured");
        }
    }
}

struct MyRequestHandler {}

impl RequestHandler for MyRequestHandler {
    fn get_report(&mut self, id: ReportId, _report_type: &mut [u8]) -> Option<usize> {
        defmt::info!("Get report for {:?}", id);
        None
    }
    fn set_report(&mut self, id: ReportId, data: &[u8]) -> OutResponse {
        defmt::info!("Set report for {:?}: {=[u8]:x}", id, data);
        OutResponse::Accepted
    }
    fn set_idle_ms(&mut self, id: Option<ReportId>, dur: u32) {
        defmt::info!("Set idle rate for {:?} to {}ms", id, dur);
    }
    fn get_idle_ms(&mut self, id: Option<ReportId>) -> Option<u32> {
        defmt::info!("Get idle rate for {:?}", id);
        None
    }
}

#[entry]
fn main() -> ! {
    let mut config = Config::default();

    {
        use embassy_stm32::rcc::*;

        config.rcc.hse = Some(Hse {
            freq: Hertz(24_000_000),
            mode: HseMode::Oscillator,
        });
        config.rcc.pll1 = Some(Pll {
            source: PllSource::HSE,
            prediv: PllPreDiv::DIV3,
            mul: PllMul::MUL150,
            divp: Some(PllDiv::DIV2),
            divq: None,
            divr: None,
            divs: None,
            divt: None,
        });
        config.rcc.sys = Sysclk::PLL1_P; // 600 Mhz
        config.rcc.ahb_pre = AHBPrescaler::DIV2; // 300 Mhz
        config.rcc.apb1_pre = APBPrescaler::DIV2; // 150 Mhz
        config.rcc.apb2_pre = APBPrescaler::DIV2; // 150 Mhz
        config.rcc.apb4_pre = APBPrescaler::DIV2; // 150 Mhz
        config.rcc.apb5_pre = APBPrescaler::DIV2; // 150 Mhz
        config.rcc.voltage_scale = VoltageScale::HIGH;

        config.rcc.hsi48 = Some(Hsi48Config {
            sync_from_usb: true,
        });
    }

    let p = embassy_stm32::init(config);

    defmt::info!("Hitpad-RS Booting on STM32H7R3...");

    pac::PWR.csr2().modify(|w| w.set_usb33den(true));

    claim_gamepad_pins!(p);

    pac::RCC.ahb4enr().modify(|w| {
        w.set_gpioaen(true);
        w.set_gpioben(true);
        w.set_gpiomen(true);
    });

    for pin in 0..16 {
        if (config::PIN_MASK & (1 << pin)) != 0 {
            pac::GPIOA
                .moder()
                .modify(|w| w.set_moder(pin, pac::gpio::vals::Moder::INPUT));
            pac::GPIOA
                .pupdr()
                .modify(|w| w.set_pupdr(pin, pac::gpio::vals::Pupdr::PULL_UP));
        }
    }
    for pin in 16..32 {
        if (config::PIN_MASK & (1 << pin)) != 0 {
            let b_pin = pin - 16;
            pac::GPIOB
                .moder()
                .modify(|w| w.set_moder(b_pin, pac::gpio::vals::Moder::INPUT));
            pac::GPIOB
                .pupdr()
                .modify(|w| w.set_pupdr(b_pin, pac::gpio::vals::Pupdr::PULL_UP));
        }
    }

    let initial_state = read_gpio_state();
    DEBOUNCED_STATE.store(initial_state, Ordering::Relaxed);

    let mut driver_config_hs = embassy_stm32::usb::Config::default();
    driver_config_hs.vbus_detection = false;

    let driver_hs = Driver::new_fs(
        p.USB_OTG_HS,
        Irqs,
        p.PM6, // DP
        p.PM5, // DM
        EP_OUT_BUFFER_HS.init([0; 256]),
        driver_config_hs,
    );

    let device_handler_hs = DEVICE_HANDLER_HS.init(MyDeviceHandler::new());
    let request_handler_hs = REQUEST_HANDLER_HS.init(MyRequestHandler {});

    let mut usb_config_hs: embassy_usb::Config<'_> = embassy_usb::Config::new(0x1209, 0x0001);
    usb_config_hs.manufacturer = Some("Hitpad-RS");
    usb_config_hs.product = Some("Keyboard Mode");
    usb_config_hs.serial_number = Some("32968645");
    usb_config_hs.max_power = 100;
    usb_config_hs.max_packet_size_0 = 64;
    usb_config_hs.device_class = 0x00;
    usb_config_hs.device_sub_class = 0x00;
    usb_config_hs.device_protocol = 0x00;
    usb_config_hs.composite_with_iads = false;
    usb_config_hs.device_release = 0x0103;
    usb_config_hs.supports_remote_wakeup = true;

    let mut builder_hs = Builder::new(
        driver_hs,
        usb_config_hs,
        CONFIG_DESC_HS.init([0; 256]),
        BOS_DESC_HS.init([0; 256]),
        MSOS_DESC_HS.init([0; 256]),
        CONTROL_BUF_HS.init([0; 64]),
    );

    builder_hs.handler(device_handler_hs);

    let keyboard = KeyboardDriver::new(
        &mut builder_hs,
        HID_STATE_HS.init(HidState::new()),
        request_handler_hs,
    );

    let usb_hs = builder_hs.build();

    let mut driver_config_fs = embassy_stm32::usb::Config::default();
    driver_config_fs.vbus_detection = false;

    let driver_fs = Driver::new_fs(
        p.USB_OTG_FS,
        Irqs,
        p.PM11, // DP
        p.PM12, // DM
        EP_OUT_BUFFER_FS.init([0; 256]),
        driver_config_fs,
    );

    let device_handler_fs = DEVICE_HANDLER_FS.init(MyDeviceHandler::new());

    let mut usb_config_fs = embassy_usb::Config::new(0x1209, 0x0002);
    usb_config_fs.manufacturer = Some("Hitpad-RS");
    usb_config_fs.product = Some("Auth Port (placeholder)");
    usb_config_fs.serial_number = Some("32968646");
    usb_config_fs.max_power = 100;
    usb_config_fs.max_packet_size_0 = 64;
    usb_config_fs.device_class = 0x00;
    usb_config_fs.device_sub_class = 0x00;
    usb_config_fs.device_protocol = 0x00;
    usb_config_fs.composite_with_iads = false;
    usb_config_fs.device_release = 0x0103;
    usb_config_fs.supports_remote_wakeup = true;

    let mut builder_fs = Builder::new(
        driver_fs,
        usb_config_fs,
        CONFIG_DESC_FS.init([0; 256]),
        BOS_DESC_FS.init([0; 256]),
        MSOS_DESC_FS.init([0; 256]),
        CONTROL_BUF_FS.init([0; 64]),
    );

    builder_fs.handler(device_handler_fs);

    let usb_fs = builder_fs.build();

    let executor = EXECUTOR0.init(Executor::new());
    executor.run(move |spawner| {
        spawner.spawn(usb_task_hs(usb_hs).unwrap());
        spawner.spawn(usb_task_fs(usb_fs).unwrap());
        spawner.spawn(sampler_task(initial_state).unwrap());
        spawner.spawn(main_loop_task(keyboard, initial_state).unwrap());
    })
}

/// Reads GPIOA (Pins 0-15) and GPIOB (Pins 16-31) and combines them into a 32-bit integer.
#[inline(always)]
fn read_gpio_state() -> u32 {
    let porta = pac::GPIOA.idr().read().0 as u32;
    let portb = pac::GPIOB.idr().read().0 as u32;

    let combined = (portb << 16) | porta;

    !combined & config::PIN_MASK
}

#[embassy_executor::task]
async fn main_loop_task(mut keyboard: KeyboardDriver<'static, USB_OTG_HS>, initial_state: u32) {
    let active_mode = detect_boot_mode(initial_state);
    defmt::info!("Active input mode: {}", mode_str(active_mode));

    loop {
        let debounced_state = DEBOUNCED_STATE.load(Ordering::Relaxed);

        if let Some(reboot_idx) = config::REBOOT_PIN {
            if (debounced_state & (1 << reboot_idx)) != 0 {
                defmt::info!("Reboot pin triggered, resetting...");
                cortex_m::peripheral::SCB::sys_reset();
            }
        }

        let mut state = GamepadState::default();
        for (pin_idx, mapped_btn) in config::PROFILES[0].pin_map.iter().enumerate() {
            if let Some(btn) = mapped_btn {
                if (debounced_state & (1 << pin_idx)) != 0 {
                    state.buttons |= ButtonState::from(*btn);
                }
            }
        }

        state.apply_socd::<{ SocdMode::Neutral }>();

        let report = KeyboardDriver::<'static, USB_OTG_HS>::translate_state(state);
        keyboard.write_report(report).await;

        Timer::after(Duration::from_millis(1)).await;
    }
}

#[embassy_executor::task]
async fn sampler_task(initial_state: u32) {
    let mut history = [0u32; 16];
    history.fill(initial_state);
    let mut history_idx = 0;
    let mut current_debounced = initial_state;

    loop {
        let raw_state = read_gpio_state();

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

fn detect_boot_mode(raw_state: u32) -> InputMode {
    for boot_override in config::BOOT_OVERRIDES {
        for (pin_idx, mapped_btn) in config::PROFILES[0].pin_map.iter().enumerate() {
            if let Some(btn) = mapped_btn {
                if *btn as u8 == boot_override.button as u8 && (raw_state & (1 << pin_idx)) != 0 {
                    return boot_override.mode;
                }
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
async fn usb_task_hs(mut usb: embassy_usb::UsbDevice<'static, Driver<'static, USB_OTG_HS>>) {
    usb.run().await;
}

#[embassy_executor::task]
async fn usb_task_fs(mut usb: embassy_usb::UsbDevice<'static, Driver<'static, USB_OTG_FS>>) {
    usb.run().await;
}
