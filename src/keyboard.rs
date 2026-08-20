use core::sync::atomic::Ordering;

use crate::types::{ButtonState, GamepadState, SocdMode};
use embassy_stm32::usb::Instance;
use embassy_time::{Duration, Timer};
use embassy_usb::Builder;
use embassy_usb::class::hid::HidBootProtocol::Keyboard;
use embassy_usb::class::hid::HidSubclass::Boot;
use embassy_usb::class::hid::{Config as HidConfig, HidWriter, RequestHandler};
use usbd_hid::descriptor::AsInputReport;
use usbd_hid::descriptor::SerializedDescriptor;
use usbd_hid::descriptor::generator_prelude::gen_hid_descriptor;

#[gen_hid_descriptor(
    (collection = APPLICATION, usage_page = GENERIC_DESKTOP, usage = KEYBOARD) = {
        (usage_page = KEYBOARD, usage_min = 0xE0, usage_max = 0xE7) = {
            #[packed_bits = 8] #[item_settings(data,variable,absolute)] modifiers=input;
        };
        (usage_page = KEYBOARD, usage_min = 0x00, usage_max = 0x77) = {
            #[packed_bits = 32] #[item_settings(data,variable,absolute)] keybits0=input;
            #[packed_bits = 32] #[item_settings(data,variable,absolute)] keybits1=input;
            #[packed_bits = 32] #[item_settings(data,variable,absolute)] keybits2=input;
            #[packed_bits = 16] #[item_settings(data,variable,absolute)] keybits3=input;
            #[packed_bits = 8]  #[item_settings(data,variable,absolute)] keybits4=input;
        };
    }
)]
#[derive(Default)]
pub struct NkroReport {
    pub modifiers: u8,
    pub keybits0: u32,
    pub keybits1: u32,
    pub keybits2: u32,
    pub keybits3: u16,
    pub keybits4: u8,
}

impl NkroReport {
    pub const fn set_key(&mut self, keycode: u8) {
        let bit = keycode as usize;
        match bit {
            0..=31 => self.keybits0 |= 1 << bit,
            32..=63 => self.keybits1 |= 1 << (bit - 32),
            64..=95 => self.keybits2 |= 1 << (bit - 64),
            96..=111 => self.keybits3 |= 1 << (bit - 96),
            112..=119 => self.keybits4 |= 1 << (bit - 112),
            _ => {}
        }
    }
}

pub fn usb_config<'a>() -> embassy_usb::Config<'a> {
    let mut config = embassy_usb::Config::new(0x1209, 0x0001);
    config.manufacturer = Some("Hitpad-RS");
    config.product = Some("Keyboard Mode");
    config.max_power = 100;
    config.max_packet_size_0 = 64;
    config.device_class = 0x00;
    config.device_sub_class = 0x00;
    config.device_protocol = 0x00;
    config.composite_with_iads = false;
    config.device_release = 0x0103;
    config.supports_remote_wakeup = true;
    config
}

pub struct Driver<'d, T: Instance> {
    writer: HidWriter<'d, embassy_stm32::usb::Driver<'d, T>, 16>,
}

impl<'d, T: Instance> Driver<'d, T> {
    pub fn new(
        builder: &mut Builder<'d, embassy_stm32::usb::Driver<'d, T>>,
        state: &'d mut embassy_usb::class::hid::State<'d>,
        request_handler: &'d mut dyn RequestHandler,
    ) -> Self {
        let hid_config = HidConfig {
            report_descriptor: NkroReport::desc(),
            request_handler: Some(request_handler),
            poll_ms: 1,
            max_packet_size: 16,
            hid_subclass: Boot,
            hid_boot_protocol: Keyboard,
        };
        Self {
            writer: HidWriter::new(builder, state, hid_config),
        }
    }

    /// Translates state into standard USB Keycodes.
    pub fn translate_state(state: GamepadState) -> NkroReport {
        let mut report = NkroReport::default();

        // --- DIRECTIONAL INPUTS (WASD) ---
        if state.buttons.contains(ButtonState::UP) {
            report.set_key(0x1A);
        } // W
        if state.buttons.contains(ButtonState::DOWN) {
            report.set_key(0x16);
        } // S
        if state.buttons.contains(ButtonState::LEFT) {
            report.set_key(0x04);
        } // A
        if state.buttons.contains(ButtonState::RIGHT) {
            report.set_key(0x07);
        } // D

        // --- ACTION BUTTONS (Top Row: Y U I O, Bottom Row: H J K L) ---
        if state.buttons.contains(ButtonState::ACTION1) {
            report.set_key(0x1C);
        } // Y
        if state.buttons.contains(ButtonState::ACTION2) {
            report.set_key(0x18);
        } // U
        if state.buttons.contains(ButtonState::ACTION3) {
            report.set_key(0x0C);
        } // I
        if state.buttons.contains(ButtonState::ACTION4) {
            report.set_key(0x12);
        } // O
        if state.buttons.contains(ButtonState::ACTION5) {
            report.set_key(0x0B);
        } // H
        if state.buttons.contains(ButtonState::ACTION6) {
            report.set_key(0x0D);
        } // J
        if state.buttons.contains(ButtonState::ACTION7) {
            report.set_key(0x0E);
        } // K
        if state.buttons.contains(ButtonState::ACTION8) {
            report.set_key(0x0F);
        } // L

        // --- SYSTEM BUTTONS ---
        if state.buttons.contains(ButtonState::START) {
            report.set_key(0x28);
        } // Enter
        if state.buttons.contains(ButtonState::SELECT) {
            report.set_key(0x29);
        } // Esc
        if state.buttons.contains(ButtonState::HOME) {
            report.set_key(0x2A);
        } // Backspace
        if state.buttons.contains(ButtonState::TOUCHPAD) {
            report.set_key(0x2B);
        } // Tab
        if state.buttons.contains(ButtonState::L3) {
            report.set_key(0x14);
        } // Q
        if state.buttons.contains(ButtonState::R3) {
            report.set_key(0x08);
        } // E

        report
    }

    /// Sends the report over the endpoint.
    pub async fn write_report(&mut self, report: NkroReport) {
        let _ = self.writer.write_serialize(&report).await;
    }
}

#[embassy_executor::task]
pub async fn main_loop_task(mut driver: Driver<'static, embassy_stm32::peripherals::USB_OTG_HS>) {
    loop {
        let debounced_state = crate::DEBOUNCED_STATE.load(Ordering::Relaxed);

        if let Some(reboot_idx) = crate::config::REBOOT_PIN
            && (debounced_state & (1 << reboot_idx)) != 0
        {
            defmt::info!("Reboot pin triggered, resetting...");
            cortex_m::peripheral::SCB::sys_reset();
        }

        let mut state = GamepadState::default();
        for (pin_idx, mapped_btn) in crate::config::PROFILES[0].pin_map.iter().enumerate() {
            if let Some(btn) = mapped_btn
                && (debounced_state & (1 << pin_idx)) != 0
            {
                state.buttons |= ButtonState::from(*btn);
            }
        }

        state.apply_socd::<{ SocdMode::Neutral }>();

        let report =
            Driver::<'static, embassy_stm32::peripherals::USB_OTG_HS>::translate_state(state);
        driver.write_report(report).await;

        Timer::after(Duration::from_millis(1)).await;
    }
}
