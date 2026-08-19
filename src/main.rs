#![no_std]
#![no_main]
#![feature(impl_trait_in_assoc_type, adt_const_params)]
#![allow(clippy::future_not_send)]

mod config;
mod keyboard;
mod macros;
mod types;

use crate::pac::interrupt;
use defmt_rtt as _;
use embassy_stm32::time::Hertz;
use panic_probe as _;

use cortex_m_rt::entry;
use embassy_executor::Executor;
use embassy_stm32::Config;
use embassy_stm32::bind_interrupts;
use embassy_stm32::pac;
use embassy_stm32::peripherals::{USB_OTG_FS, USB_OTG_HS};
use embassy_stm32::usb::host::{HostDriver, HostState};
use embassy_stm32::usb::{Driver, InterruptHandler};
use embassy_time::{Duration, Timer};
use embassy_usb::Builder;
use embassy_usb::Handler;
use embassy_usb::class::hid::State as HidState;
use embassy_usb::class::hid::{ReportId, RequestHandler};
use embassy_usb::control::OutResponse;
use embassy_usb_driver::host::UsbHostAllocator;
use embassy_usb_driver::host::UsbPipe;
use embassy_usb_driver::host::{DeviceEvent, UsbHostController};
use portable_atomic::{AtomicU32, Ordering};
use static_cell::StaticCell;

use crate::keyboard::KeyboardDriver;
use crate::types::{ButtonState, GamepadState, InputMode, SocdMode};

bind_interrupts!(struct Irqs {
    OTG_HS => InterruptHandler<USB_OTG_HS>;
});

#[unsafe(link_section = ".sram3")]
static EP_OUT_BUFFER_HS: StaticCell<[u8; 256]> = StaticCell::new();

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
static DEBOUNCED_STATE: AtomicU32 = AtomicU32::new(0);

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

    let driver_hs = Driver::new_hs(
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
    executor.run(move |spawner| {
        spawner.spawn(usb_task_hs(usb_hs).unwrap());
        spawner.spawn(host_task(driver_host).unwrap());
        spawner.spawn(sampler_task(initial_state).unwrap());
        spawner.spawn(main_loop_task(keyboard, initial_state).unwrap());
    })
}

/// Reads GPIOA (Pins 0-15) and GPIOB (Pins 16-31) and combines them into a 32-bit integer.
#[allow(clippy::inline_always, clippy::similar_names)]
#[inline(always)]
fn read_gpio_state() -> u32 {
    let porta = pac::GPIOA.idr().read().0;
    let portb = pac::GPIOB.idr().read().0;

    let combined = (portb << 16) | porta;

    !combined & config::PIN_MASK
}

#[embassy_executor::task]
async fn main_loop_task(mut keyboard: KeyboardDriver<'static, USB_OTG_HS>, initial_state: u32) {
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
async fn usb_task_hs(mut usb: embassy_usb::UsbDevice<'static, Driver<'static, USB_OTG_HS>>) {
    usb.run().await;
}

#[embassy_executor::task]
async fn host_task(mut host: HostDriver<'static, USB_OTG_FS, 12>) {
    defmt::info!("Starting USB Host on FS Port for Auth Dongle...");

    loop {
        let event = host.inner.wait_for_device_event().await;

        match event {
            DeviceEvent::Connected(_speed) => {
                defmt::info!("MagicBoots Dongle Plugged In!");

                let mut control_pipe = host.inner.allocator().alloc_pipe::<
                        embassy_usb_driver::host::pipe::Control,
                        embassy_usb_driver::host::pipe::InOut
                    >(
                        0,
                        &embassy_usb_driver::EndpointInfo {
                            addr: embassy_usb_driver::EndpointAddress::from_parts(0, embassy_usb_driver::Direction::Out),
                            ep_type: embassy_usb_driver::EndpointType::Control,
                            max_packet_size: 64,
                            interval_ms: 0,
                        },
                        None,
                    ).unwrap();

                // Build the standard GET_DESCRIPTOR request
                // bmRequestType: 0x80 (Dir: In, Type: Standard, Recipient: Device)
                // bRequest: 0x06 (GET_DESCRIPTOR)
                // wValue: 0x0100 (Type: Device Descriptor (1), Index: 0)
                // wIndex: 0x0000
                // wLength: 18 (Standard length of a Device Descriptor)
                let setup_packet = [0x80, 0x06, 0x00, 0x01, 0x00, 0x00, 18, 0x00];
                let mut descriptor_buf = [0u8; 18];

                match control_pipe
                    .control_in(&setup_packet, &mut descriptor_buf)
                    .await
                {
                    Ok(len) => {
                        defmt::info!(
                            "Got Device Descriptor ({} bytes): {:02x}",
                            len,
                            descriptor_buf
                        );
                    }
                    Err(e) => {
                        defmt::error!("Failed to get descriptor: {:?}", e);
                        continue;
                    }
                }

                // Assign an address to the device (Address 1)
                // bmRequestType: 0x00 (Dir: Out, Type: Standard, Recipient: Device)
                // bRequest: 0x05 (SET_ADDRESS)
                // wValue: 0x0001 (Address 1)
                // wLength: 0
                let set_addr_setup = [0x00, 0x05, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00];
                match control_pipe.control_out(&set_addr_setup, &[]).await {
                    Ok(()) => defmt::info!("Assigned Address 1 to Dongle"),
                    Err(e) => {
                        defmt::error!("Failed to set address: {:?}", e);
                        continue;
                    }
                }

                // USB Spec requires a short delay after SET_ADDRESS before the device responds to the new address
                embassy_time::Timer::after_millis(10).await;

                // Recreate the control pipe on the NEW address (1)
                // Drop the old pipe to free up the host hardware channel
                drop(control_pipe);

                let mut control_pipe = host.inner.allocator().alloc_pipe::<
    			embassy_usb_driver::host::pipe::Control,
    			embassy_usb_driver::host::pipe::InOut
                    >(
    			1,
    			&embassy_usb_driver::EndpointInfo {
                            addr: embassy_usb_driver::EndpointAddress::from_parts(0, embassy_usb_driver::Direction::Out),
                            ep_type: embassy_usb_driver::EndpointType::Control,
                            max_packet_size: 64,
                            interval_ms: 0,
    			},
    			None,
                    ).unwrap();

                // Set Configuration 1
                // bmRequestType: 0x00
                // bRequest: 0x09 (SET_CONFIGURATION)
                // wValue: 0x0001 (Config 1)
                let set_config_setup = [0x00, 0x09, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00];
                match control_pipe.control_out(&set_config_setup, &[]).await {
                    Ok(()) => defmt::info!("Dongle configured and active!"),
                    Err(e) => {
                        defmt::error!("Failed to set configuration: {:?}", e);
                    }
                }
            }
            DeviceEvent::Disconnected => {
                defmt::info!("MagicBoots Dongle Removed!");
            }
            DeviceEvent::Overcurrent => {
                defmt::info!("USB Overcurrent detected!");
            }
            _ => {}
        }
    }
}
