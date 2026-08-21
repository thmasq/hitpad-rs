use crate::types::{ButtonState, GamepadState, SocdMode};
use embassy_usb::Builder;
use embassy_usb::driver::{Driver as UsbDriver, Endpoint, EndpointIn};
use portable_atomic::Ordering;

pub fn usb_config<'a>() -> embassy_usb::Config<'a> {
    let mut config = embassy_usb::Config::new(0x045E, 0x028E);
    config.manufacturer = Some("©Microsoft Corporation");
    config.product = Some("Controller");
    config.serial_number = Some("08FEC93");
    config.max_power = 500;
    config.max_packet_size_0 = 64;
    config.device_class = 0xFF;
    config.device_sub_class = 0xFF;
    config.device_protocol = 0xFF;
    config.composite_with_iads = false;
    config.device_release = 0x0114;
    config.supports_remote_wakeup = true;
    config
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct XInputReport {
    pub report_id: u8,
    pub report_size: u8,
    pub buttons1: u8,
    pub buttons2: u8,
    pub lt: u8,
    pub rt: u8,
    pub lx: i16,
    pub ly: i16,
    pub rx: i16,
    pub ry: i16,
    pub reserved: [u8; 6],
}

pub struct Driver<'d, D: UsbDriver<'d>> {
    pub ep_in: D::EndpointIn,
    #[allow(dead_code)]
    pub ep_out: D::EndpointOut,
}

impl<'d, D: UsbDriver<'d>> Driver<'d, D> {
    pub fn new(builder: &mut Builder<'d, D>) -> Self {
        // INTERFACE 0: Control
        let mut function = builder.function(0xFF, 0x5D, 0x01);
        let mut interface = function.interface();
        let mut alt = interface.alt_setting(0xFF, 0x5D, 0x01, None);

        let ep_in = alt.endpoint_interrupt_in(None, 32, 1);
        let ep_out = alt.endpoint_interrupt_out(None, 32, 8);

        alt.descriptor(
            0x21,
            &[
                0x00,
                0x01,
                0x01,
                0x25,
                ep_in.info().addr.into(),
                0x14,
                0x00,
                0x00,
                0x00,
                0x00,
                0x13,
                ep_out.info().addr.into(),
                0x08,
                0x00,
                0x00,
            ],
        );
        drop(alt);
        drop(interface);
        drop(function);

        // INTERFACE 1: Audio
        let mut function = builder.function(0xFF, 0x5D, 0x03);
        let mut interface = function.interface();
        let mut alt = interface.alt_setting(0xFF, 0x5D, 0x03, None);

        let ep_a_in1 = alt.endpoint_interrupt_in(None, 32, 2);
        let ep_a_out1 = alt.endpoint_interrupt_out(None, 32, 4);
        let ep_a_in2 = alt.endpoint_interrupt_in(None, 32, 64);
        let ep_a_out2 = alt.endpoint_interrupt_out(None, 32, 16);

        alt.descriptor(
            0x21,
            &[
                0x00,
                0x01,
                0x01,
                0x01,
                ep_a_in1.info().addr.into(),
                0x40,
                0x01,
                ep_a_out1.info().addr.into(),
                0x20,
                0x16,
                ep_a_in2.info().addr.into(),
                0x00,
                0x00,
                0x00,
                0x00,
                0x00,
                0x00,
                0x16,
                ep_a_out2.info().addr.into(),
                0x00,
                0x00,
                0x00,
                0x00,
                0x00,
                0x00,
            ],
        );
        drop(alt);
        drop(interface);
        drop(function);

        // INTERFACE 2: Plugin
        let mut function = builder.function(0xFF, 0x5D, 0x02);
        let mut interface = function.interface();
        let mut alt = interface.alt_setting(0xFF, 0x5D, 0x02, None);

        let ep_p_in = alt.endpoint_interrupt_in(None, 32, 8);

        alt.descriptor(
            0x21,
            &[
                0x00,
                0x01,
                0x01,
                0x22,
                ep_p_in.info().addr.into(),
                0x03,
                0x00,
            ],
        );
        drop(alt);
        drop(interface);
        drop(function);

        // INTERFACE 3: Security
        let mut function = builder.function(0xFF, 0xFD, 0x13);
        let mut interface = function.interface();
        let mut alt = interface.alt_setting(0xFF, 0xFD, 0x13, None);

        alt.descriptor(0x41, &[0x00, 0x01, 0x01, 0x03]);
        drop(alt);
        drop(interface);
        drop(function);

        Self { ep_in, ep_out }
    }

    pub fn translate_state(state: GamepadState) -> XInputReport {
        let mut report = XInputReport {
            report_id: 0,
            report_size: 20,
            buttons1: 0,
            buttons2: 0,
            lt: 0,
            rt: 0,
            lx: 0,
            ly: 0,
            rx: 0,
            ry: 0,
            reserved: [0; 6],
        };

        if state.buttons.contains(ButtonState::UP) {
            report.buttons1 |= 1 << 0;
        }
        if state.buttons.contains(ButtonState::DOWN) {
            report.buttons1 |= 1 << 1;
        }
        if state.buttons.contains(ButtonState::LEFT) {
            report.buttons1 |= 1 << 2;
        }
        if state.buttons.contains(ButtonState::RIGHT) {
            report.buttons1 |= 1 << 3;
        }
        if state.buttons.contains(ButtonState::START) {
            report.buttons1 |= 1 << 4;
        }
        if state.buttons.contains(ButtonState::SELECT) {
            report.buttons1 |= 1 << 5;
        } // Back
        if state.buttons.contains(ButtonState::L3) {
            report.buttons1 |= 1 << 6;
        }
        if state.buttons.contains(ButtonState::R3) {
            report.buttons1 |= 1 << 7;
        }

        if state.buttons.contains(ButtonState::ACTION5) {
            report.buttons2 |= 1 << 0;
        } // LB
        if state.buttons.contains(ButtonState::ACTION6) {
            report.buttons2 |= 1 << 1;
        } // RB
        if state.buttons.contains(ButtonState::HOME) {
            report.buttons2 |= 1 << 2;
        } // Guide

        if state.buttons.contains(ButtonState::ACTION1) {
            report.buttons2 |= 1 << 4;
        } // A
        if state.buttons.contains(ButtonState::ACTION2) {
            report.buttons2 |= 1 << 5;
        } // B
        if state.buttons.contains(ButtonState::ACTION3) {
            report.buttons2 |= 1 << 6;
        } // X
        if state.buttons.contains(ButtonState::ACTION4) {
            report.buttons2 |= 1 << 7;
        } // Y

        if state.buttons.contains(ButtonState::ACTION7) {
            report.lt = 255;
        } // LT
        if state.buttons.contains(ButtonState::ACTION8) {
            report.rt = 255;
        } // RT

        report
    }

    pub async fn write_report(
        &mut self,
        report: XInputReport,
    ) -> Result<(), embassy_usb::driver::EndpointError> {
        let buf: &[u8] = unsafe {
            core::slice::from_raw_parts(
                &report as *const _ as *const u8,
                core::mem::size_of::<XInputReport>(),
            )
        };
        self.ep_in.write(buf).await
    }
}

#[embassy_executor::task]
pub async fn main_loop_task(
    mut driver: Driver<
        'static,
        embassy_stm32::usb::Driver<'static, embassy_stm32::peripherals::USB_OTG_HS>,
    >,
) {
    defmt::info!("XInput Mode active.");

    // WAIT for the host to actually enable the endpoint before we start writing
    driver.ep_in.wait_enabled().await;

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

        let report = Driver::<
            'static,
            embassy_stm32::usb::Driver<'static, embassy_stm32::peripherals::USB_OTG_HS>,
        >::translate_state(state);

        if driver.write_report(report).await.is_err() {
            embassy_time::Timer::after(embassy_time::Duration::from_millis(10)).await;
        }
    }
}
