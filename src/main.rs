#![no_std]
#![no_main]
#![feature(impl_trait_in_assoc_type, adt_const_params)]
#![allow(clippy::future_not_send)]

#[macro_use]
mod macros;

mod config;
mod host;
mod keyboard;
mod ps5;
mod sampling;
mod types;
mod xinput;

use crate::pac::interrupt;
use crate::sampling::digital_sampler_task;
use crate::sampling::read_gpio_state;
use defmt_rtt as _;
use embassy_stm32::time::Hertz;
use embassy_usb::class::hid::HidBootProtocol;
use embassy_usb::class::hid::HidSubclass;
use panic_probe as _;

use crate::types::InputMode;
use cortex_m_rt::entry;
use embassy_executor::Executor;
use embassy_stm32::Config;
use embassy_stm32::bind_interrupts;
use embassy_stm32::pac;
use embassy_stm32::peripherals::{USB_OTG_FS, USB_OTG_HS};
use embassy_stm32::usb::host::{HostDriver, HostState};
use embassy_stm32::usb::{Driver, InterruptHandler};
use embassy_usb::Builder;
use embassy_usb::Handler;
use embassy_usb::class::hid::State as HidState;
use embassy_usb::class::hid::{ReportId, RequestHandler};
use embassy_usb::control::OutResponse;
use portable_atomic::{AtomicU32, AtomicU64, Ordering};
use static_cell::StaticCell;

bind_interrupts!(struct Irqs {
    OTG_HS => InterruptHandler<USB_OTG_HS>;
});

#[unsafe(link_section = ".sram3")]
static EP_OUT_BUFFER_HS: StaticCell<[u8; 1024]> = StaticCell::new();

#[unsafe(link_section = ".sram3")]
static CONTROL_BUF_HS: StaticCell<[u8; 64]> = StaticCell::new();

#[unsafe(link_section = ".sram3")]
static EXECUTOR0: StaticCell<Executor> = StaticCell::new();

static HID_STATE_HS: StaticCell<HidState> = StaticCell::new();
static CONFIG_DESC_HS: StaticCell<[u8; 256]> = StaticCell::new();
static BOS_DESC_HS: StaticCell<[u8; 256]> = StaticCell::new();
static MSOS_DESC_HS: StaticCell<[u8; 256]> = StaticCell::new();
static DEVICE_HANDLER_HS: StaticCell<MyDeviceHandler> = StaticCell::new();
static REQUEST_HANDLER_HS: StaticCell<MyRequestHandler> = StaticCell::new();
static HOST_STATE: HostState<12> = HostState::new();

static mut PS5_HANDLER: crate::ps5::Ps5RequestHandler = crate::ps5::Ps5RequestHandler;
static mut HID_STATE: embassy_usb::class::hid::State = embassy_usb::class::hid::State::new();

pub static DEBOUNCED_STATE: AtomicU64 = AtomicU64::new(0);
pub static ANALOG_BUTTON_STATE: AtomicU32 = AtomicU32::new(0);

const ADC_OVERSAMPLING: u16 = 16;

struct MyDeviceHandler {
    configured: bool,
}

impl MyDeviceHandler {
    const fn new() -> Self {
        Self { configured: false }
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

#[embassy_stm32::interrupt]
unsafe fn OTG_FS() {
    HostDriver::<'static, USB_OTG_FS, 12>::on_interrupt(&HOST_STATE);
}

#[inline(never)]
fn enable_caches(cp: &mut cortex_m::peripheral::Peripherals) {
    cp.SCB.enable_icache();
    cp.SCB.enable_dcache(&mut cp.CPUID);
}

#[entry]
#[allow(clippy::too_many_lines)]
fn main() -> ! {
    let mut cp = cortex_m::Peripherals::take().unwrap();

    // Configure the MPU for the 32KB DMA_RAM region at 0x24000000
    unsafe {
        cp.MPU.ctrl.modify(|r| r & !1);
        cp.MPU.rnr.write(0);
        cp.MPU.rbar.write(0x2400_0000 | (1 << 4));

        // Size: 32KB (0x0E), Enable=1
        // TEX=1, C=0, B=0, S=1 -> Normal memory, Non-cacheable, Shareable
        // AP=0b011 -> Full Access
        // XN=1 -> Execute Never (prevent instruction fetches from data buffers)
        cp.MPU.rasr.write(
            ((1 << 28)       // XN
                | (0b011 << 24) // AP (Full Access)
                | (1 << 19))     // B=0 (Non-bufferable)
                | (0x0E << 1)   // SIZE = 32KB
                | 1, // ENABLE
        );

        // Re-enable MPU with default memory map for background regions (PRIVDEFENA)
        cp.MPU.ctrl.modify(|r| r | 1 | (1 << 2));
    }

    enable_caches(&mut cp);

    // Zero-initialize the custom DMA_RAM region.
    // The startup code ignores NOLOAD sections, so it contains random garbage.
    // StaticCell requires its memory to be 0 at startup!
    unsafe {
        core::ptr::write_bytes(0x2400_0000 as *mut u8, 0, 32768);
    }

    let mut config = Config::default();
    {
        use embassy_stm32::rcc::{
            AHBPrescaler, APBPrescaler, Hse, HseMode, Hsi48Config, Pll, PllDiv, PllMul, PllPreDiv,
            PllSource, Sysclk, Usbphycsel, VoltageScale,
        };

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
        config.rcc.mux.usbphycsel = Usbphycsel::HSE;

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
        w.set_gpiocen(true);
        w.set_gpiomen(true);
    });

    let mut digital_mask: u64 = 0;
    let mut analog_mask: u64 = 0;

    if let Some(reboot_idx) = config::REBOOT_PIN {
        digital_mask |= 1 << reboot_idx;
    }

    for (pin_idx, mapped_btn) in config::PROFILES[0].pin_map.iter().enumerate() {
        if let Some(binding) = mapped_btn {
            match binding {
                crate::types::ButtonBinding::Digital(_) => digital_mask |= 1 << pin_idx,
                crate::types::ButtonBinding::Analog(_)
                | crate::types::ButtonBinding::AnalogSingle(_) => {
                    analog_mask |= 1 << pin_idx;
                }
            }
        }
    }

    let configure_pin = |pin: usize, is_digital: bool| {
        let (port, pin_num) = if pin < 16 {
            (&pac::GPIOA, pin)
        } else if pin < 32 {
            (&pac::GPIOB, pin - 16)
        } else {
            (&pac::GPIOC, pin - 32)
        };

        if is_digital {
            port.moder()
                .modify(|w| w.set_moder(pin_num, pac::gpio::vals::Moder::INPUT));
            port.pupdr()
                .modify(|w| w.set_pupdr(pin_num, pac::gpio::vals::Pupdr::PULL_UP));
        } else {
            port.moder()
                .modify(|w| w.set_moder(pin_num, pac::gpio::vals::Moder::ANALOG));
            port.pupdr()
                .modify(|w| w.set_pupdr(pin_num, pac::gpio::vals::Pupdr::FLOATING));
        }
    };

    for pin in 0..48 {
        if (digital_mask & (1 << pin)) != 0 {
            configure_pin(pin, true);
        } else if (analog_mask & (1 << pin)) != 0 {
            configure_pin(pin, false);
        }
    }

    embassy_time::block_for(embassy_time::Duration::from_millis(10));

    let initial_state = read_gpio_state(digital_mask);

    DEBOUNCED_STATE.store(initial_state, Ordering::Relaxed);

    let active_mode = detect_boot_mode(initial_state);
    defmt::info!("Active input mode: {}", mode_str(active_mode));

    let mut driver_config_hs = embassy_stm32::usb::Config::default();
    driver_config_hs.vbus_detection = false;

    let driver_hs = Driver::new_hs(
        p.USB_OTG_HS,
        Irqs,
        p.PM6, // DP
        p.PM5, // DM
        EP_OUT_BUFFER_HS.init([0; 1024]),
        driver_config_hs,
    );

    let device_handler_hs = DEVICE_HANDLER_HS.init(MyDeviceHandler::new());
    let request_handler_hs = REQUEST_HANDLER_HS.init(MyRequestHandler {});

    // Fetch the specific USB descriptor configuration for our selected module
    let usb_config_hs = match active_mode {
        InputMode::Keyboard => crate::keyboard::usb_config(),
        InputMode::XInput => crate::xinput::usb_config(),
        InputMode::PS5 => crate::ps5::usb_config(),
    };

    let mut builder_hs = Builder::new(
        driver_hs,
        usb_config_hs,
        CONFIG_DESC_HS.init([0; 256]),
        BOS_DESC_HS.init([0; 256]),
        MSOS_DESC_HS.init([0; 256]),
        CONTROL_BUF_HS.init([0; 64]),
    );

    builder_hs.handler(device_handler_hs);

    unsafe {
        cortex_m::peripheral::NVIC::unmask(embassy_stm32::pac::Interrupt::OTG_FS);
    }

    let driver_host = HostDriver::new_fs(
        p.USB_OTG_FS,
        p.PM11, // DP
        p.PM12, // DM
        &HOST_STATE,
    );

    let executor = EXECUTOR0.init(Executor::new());

    match active_mode {
        InputMode::Keyboard => {
            let driver = crate::keyboard::Driver::new(
                &mut builder_hs,
                HID_STATE_HS.init(HidState::new()),
                request_handler_hs,
            );
            let usb_hs = builder_hs.build();

            executor.run(move |spawner| {
                spawner.spawn(usb_task_hs(usb_hs).unwrap());
                spawner.spawn(digital_sampler_task(initial_state, digital_mask).unwrap());
                spawner.spawn(crate::sampling::analog_sampler_task().unwrap());
                spawner.spawn(crate::keyboard::main_loop_task(driver).unwrap());
            })
        }
        InputMode::XInput => {
            let driver = crate::xinput::Driver::new(&mut builder_hs);
            let usb_hs = builder_hs.build();

            executor.run(move |spawner| {
                spawner.spawn(usb_task_hs(usb_hs).unwrap());
                spawner.spawn(digital_sampler_task(initial_state, digital_mask).unwrap());
                spawner.spawn(crate::sampling::analog_sampler_task().unwrap());
                spawner.spawn(crate::xinput::main_loop_task(driver).unwrap());
            })
        }
        InputMode::PS5 => {
            let config = embassy_usb::class::hid::Config {
                report_descriptor: crate::ps5::PS5_REPORT_DESC,
                request_handler: Some(unsafe { &mut *core::ptr::addr_of_mut!(crate::PS5_HANDLER) }),
                poll_ms: 1,
                max_packet_size: 64,
                hid_subclass: HidSubclass::No,
                hid_boot_protocol: HidBootProtocol::None,
            };

            let hid = embassy_usb::class::hid::HidReaderWriter::<'static, _, 64, 64>::new(
                &mut builder_hs,
                unsafe { &mut *core::ptr::addr_of_mut!(HID_STATE) },
                config,
            );

            let usb_hs = builder_hs.build();

            executor.run(move |spawner| {
                spawner.spawn(usb_task_hs(usb_hs).unwrap());
                spawner.spawn(crate::host::host_task(driver_host).unwrap());
                spawner.spawn(digital_sampler_task(initial_state, digital_mask).unwrap());
                spawner.spawn(crate::sampling::analog_sampler_task().unwrap());
                spawner.spawn(crate::ps5::main_loop_task(hid).unwrap());
            })
        }
    }
}

fn detect_boot_mode(raw_state: u64) -> InputMode {
    for boot_override in config::BOOT_OVERRIDES {
        for (pin_idx, mapped_btn) in config::PROFILES[0].pin_map.iter().enumerate() {
            if let Some(btn) = mapped_btn
                && btn.button() as u8 == boot_override.button as u8
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
async fn usb_task_hs(mut usb: embassy_usb::UsbDevice<'static, Driver<'static, USB_OTG_HS>>) {
    usb.run().await;
}
