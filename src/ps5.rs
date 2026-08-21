use core::cell::{Cell, RefCell};

use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_usb::class::hid::{ReportId, RequestHandler};
use embassy_usb::control::OutResponse;
use portable_atomic::Ordering;

use crate::types::{ButtonState, GamepadState, SocdMode};

pub struct Ps5AuthData {
    pub nonce: [u8; 63],
    pub state: [u8; 15],
}

pub static PS5_AUTH_DATA: Mutex<CriticalSectionRawMutex, RefCell<Ps5AuthData>> =
    Mutex::new(RefCell::new(Ps5AuthData {
        nonce: [0; 63],
        state: [0; 15],
    }));

pub static AUTH_PAYLOAD_TO_DONGLE: Channel<CriticalSectionRawMutex, [u8; 63], 2> = Channel::new();

pub static HASH_REQ_CHANNEL: Channel<CriticalSectionRawMutex, Ps5Report, 2> = Channel::new();
pub static HASH_RES_CHANNEL: Channel<CriticalSectionRawMutex, [u8; 8], 2> = Channel::new();

static LATEST_HASH: Mutex<CriticalSectionRawMutex, Cell<[u8; 8]>> = Mutex::new(Cell::new([0; 8]));

pub fn usb_config<'a>() -> embassy_usb::Config<'a> {
    let mut config = embassy_usb::Config::new(0x2B81, 0x0101);
    config.manufacturer = Some("Activtor");
    config.product = Some("P5General");
    config.serial_number = None;
    config.max_power = 500;
    config.max_packet_size_0 = 64;
    config.device_class = 0x00;
    config.device_sub_class = 0x00;
    config.device_protocol = 0x00;
    config.composite_with_iads = false;
    config.device_release = 0x0001;
    config.supports_remote_wakeup = false;
    config
}

pub static PS5_REPORT_DESC: &[u8] = &[
    0x05, 0x01, 0x09, 0x05, 0xA1, 0x01, 0x85, 0x01, 0x09, 0x30, 0x09, 0x31, 0x09, 0x32, 0x09, 0x35,
    0x09, 0x33, 0x09, 0x34, 0x15, 0x00, 0x26, 0xFF, 0x00, 0x75, 0x08, 0x95, 0x06, 0x81, 0x02, 0x06,
    0x00, 0xFF, 0x09, 0x20, 0x95, 0x01, 0x81, 0x02, 0x05, 0x01, 0x09, 0x39, 0x15, 0x00, 0x25, 0x07,
    0x35, 0x00, 0x46, 0x3B, 0x01, 0x65, 0x14, 0x75, 0x04, 0x95, 0x01, 0x81, 0x42, 0x65, 0x00, 0x05,
    0x09, 0x19, 0x01, 0x29, 0x0E, 0x15, 0x00, 0x25, 0x01, 0x75, 0x01, 0x95, 0x0E, 0x81, 0x02, 0x06,
    0x00, 0xFF, 0x09, 0x21, 0x95, 0x0E, 0x81, 0x02, 0x06, 0x00, 0xFF, 0x09, 0x22, 0x15, 0x00, 0x26,
    0xFF, 0x00, 0x75, 0x08, 0x95, 0x34, 0x81, 0x02, 0x85, 0x02, 0x09, 0x23, 0x95, 0x2F, 0x91, 0x02,
    0x85, 0x03, 0x0A, 0x21, 0x28, 0x95, 0x2F, 0xB1, 0x02, 0x06, 0x80, 0xFF, 0x85, 0xE0, 0x09, 0x57,
    0x95, 0x02, 0xB1, 0x02, 0xC0, 0x06, 0xF0, 0xFF, 0x09, 0x40, 0xA1, 0x01, 0x85, 0xF0, 0x09, 0x47,
    0x95, 0x3F, 0xB1, 0x02, 0x85, 0xF1, 0x09, 0x48, 0x95, 0x3F, 0xB1, 0x02, 0x85, 0xF2, 0x09, 0x49,
    0x95, 0x0F, 0xB1, 0x02, 0xC0,
];

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct Ps5Report {
    pub report_id: u8,
    pub lx: u8,
    pub ly: u8,
    pub rx: u8,
    pub ry: u8,
    pub lt: u8,
    pub rt: u8,
    pub report_counter: u8,
    pub buttons1: u8,
    pub buttons2: u8,
    pub buttons3: u8,
    pub data_11: u8,
    pub auth_seq_number: u32,
    pub gyro: [u8; 6],
    pub accel: [u8; 6],
    pub data_28_29: u16,
    pub data_30_31: u16,
    pub tp_data: [u8; 9],
    pub data_40_55: [u8; 16],
    pub hash: [u8; 8],
}

pub struct Ps5RequestHandler;

impl RequestHandler for Ps5RequestHandler {
    fn get_report(&mut self, id: ReportId, buf: &mut [u8]) -> Option<usize> {
        let report_id = match id {
            ReportId::Feature(n) => n,
            _ => return None,
        };

        match report_id {
            0x03 => {
                let output: [u8; 48] = [
                    0x21, 0x28, 0x03, 0xC3, 0x00, 0x2C, 0x56, 0x01, 0x00, 0xD0, 0x07, 0x00, 0x80,
                    0x04, 0x00, 0x00, 0x80, 0x0D, 0x0D, 0x84, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                ];
                let len = core::cmp::min(buf.len(), output.len());
                buf[..len].copy_from_slice(&output[..len]);
                Some(len)
            }

            0xF1 => PS5_AUTH_DATA.lock(|auth| {
                let auth = auth.borrow();
                let len = core::cmp::min(buf.len(), auth.nonce.len());
                buf[..len].copy_from_slice(&auth.nonce[..len]);
                Some(len)
            }),

            0xF2 => PS5_AUTH_DATA.lock(|auth| {
                let auth = auth.borrow();
                let len = core::cmp::min(buf.len(), auth.state.len());
                buf[..len].copy_from_slice(&auth.state[..len]);
                Some(len)
            }),

            _ => None,
        }
    }

    fn set_report(&mut self, id: ReportId, data: &[u8]) -> OutResponse {
        let report_id = match id {
            ReportId::Feature(n) => n,
            _ => return OutResponse::Rejected,
        };

        if report_id == 0xF0 {
            let mut payload = [0u8; 63];
            let len = core::cmp::min(data.len(), payload.len());
            payload[..len].copy_from_slice(&data[..len]);
            let _ = AUTH_PAYLOAD_TO_DONGLE.try_send(payload);
            OutResponse::Accepted
        } else {
            OutResponse::Rejected
        }
    }
}

pub fn translate_state(state: GamepadState) -> Ps5Report {
    let mut report = Ps5Report {
        report_id: 1,
        lx: 128,
        ly: 128,
        rx: 128,
        ry: 128,
        lt: 0,
        rt: 0,
        report_counter: 0,
        buttons1: 0x08, // Neutral D-Pad
        buttons2: 0,
        buttons3: 0,
        data_11: 0,
        auth_seq_number: 0,
        gyro: [0; 6],
        accel: [0; 6],
        data_28_29: 0,
        data_30_31: 0x001A,
        tp_data: [0x80, 0, 0, 0, 0x80, 0, 0, 0, 0], // Empty touchpad state
        data_40_55: [0; 16],
        hash: [0; 8],
    };

    let up = state.buttons.contains(ButtonState::UP);
    let down = state.buttons.contains(ButtonState::DOWN);
    let left = state.buttons.contains(ButtonState::LEFT);
    let right = state.buttons.contains(ButtonState::RIGHT);

    if up && right {
        report.buttons1 = 1;
    } else if up && left {
        report.buttons1 = 7;
    } else if down && right {
        report.buttons1 = 3;
    } else if down && left {
        report.buttons1 = 5;
    } else if up {
        report.buttons1 = 0;
    } else if right {
        report.buttons1 = 2;
    } else if down {
        report.buttons1 = 4;
    } else if left {
        report.buttons1 = 6;
    }

    if state.buttons.contains(ButtonState::ACTION3) {
        report.buttons1 |= 1 << 4;
    } // Square
    if state.buttons.contains(ButtonState::ACTION1) {
        report.buttons1 |= 1 << 5;
    } // Cross
    if state.buttons.contains(ButtonState::ACTION2) {
        report.buttons1 |= 1 << 6;
    } // Circle
    if state.buttons.contains(ButtonState::ACTION4) {
        report.buttons1 |= 1 << 7;
    } // Triangle

    if state.buttons.contains(ButtonState::ACTION5) {
        report.buttons2 |= 1 << 0;
    } // L1
    if state.buttons.contains(ButtonState::ACTION6) {
        report.buttons2 |= 1 << 1;
    } // R1
    if state.buttons.contains(ButtonState::ACTION7) {
        report.buttons2 |= 1 << 2;
        report.lt = 255;
    } // L2
    if state.buttons.contains(ButtonState::ACTION8) {
        report.buttons2 |= 1 << 3;
        report.rt = 255;
    } // R2

    if state.buttons.contains(ButtonState::SELECT) {
        report.buttons2 |= 1 << 4;
    } // Share
    if state.buttons.contains(ButtonState::START) {
        report.buttons2 |= 1 << 5;
    } // Options
    if state.buttons.contains(ButtonState::L3) {
        report.buttons2 |= 1 << 6;
    }
    if state.buttons.contains(ButtonState::R3) {
        report.buttons2 |= 1 << 7;
    }

    if state.buttons.contains(ButtonState::HOME) {
        report.buttons3 |= 1 << 0;
    } // PS Button

    report
}

#[embassy_executor::task]
pub async fn main_loop_task(
    mut hid: embassy_usb::class::hid::HidReaderWriter<
        'static,
        embassy_stm32::usb::Driver<'static, embassy_stm32::peripherals::USB_OTG_HS>,
        64,
        64,
    >,
) {
    defmt::info!("PS5 Mode active. Auth decoupled.");

    // Background Dongle Polling
    // Runs as fast as the Full Speed USB bus allows (~2ms round trip).
    // Constantly asks the MagicBoots dongle for fresh hashes.
    let dongle_fut = async {
        loop {
            let debounced_state = crate::DEBOUNCED_STATE.load(Ordering::Relaxed);
            let mut state = GamepadState::default();
            for (pin_idx, mapped_btn) in crate::config::PROFILES[0].pin_map.iter().enumerate() {
                if let Some(btn) = mapped_btn
                    && (debounced_state & (1 << pin_idx)) != 0
                {
                    state.buttons |= ButtonState::from(btn.button());
                }
            }
            state.apply_socd::<{ SocdMode::Neutral }>();

            let report = translate_state(state);
            let _ = HASH_REQ_CHANNEL.send(report).await;

            // This blocks for ~2ms because of the FS USB frame rate
            let hash = HASH_RES_CHANNEL.receive().await;

            LATEST_HASH.lock(|c| c.set(hash));
        }
    };

    // High-Speed USB Reporting
    // Runs completely unblocked, paced strictly by the PS5 console's polling rate.
    let usb_fut = async {
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
                if let Some(binding) = mapped_btn {
                    if let crate::types::ButtonBinding::Digital(btn) = binding {
                        if (debounced_state & (1 << pin_idx)) != 0 {
                            state.buttons |= ButtonState::from(*btn);
                        }
                    }
                }
            }

            state.buttons |=
                ButtonState::from_bits_truncate(crate::ANALOG_BUTTON_STATE.load(Ordering::Relaxed));

            state.apply_socd::<{ SocdMode::Neutral }>();

            let mut report = translate_state(state);

            report.hash = LATEST_HASH.lock(|c| c.get());

            let buf: &[u8] =
                unsafe { core::slice::from_raw_parts(&report as *const _ as *const u8, 64) };

            if hid.write(buf).await.is_err() {
                embassy_time::Timer::after(embassy_time::Duration::from_millis(10)).await;
            }
        }
    };

    // Run both loops concurrently
    embassy_futures::join::join(dongle_fut, usb_fut).await;
}
