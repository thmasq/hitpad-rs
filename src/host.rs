use embassy_stm32::peripherals::USB_OTG_FS;
use embassy_stm32::usb::host::HostDriver;
use embassy_time::{Duration, Timer};
use embassy_usb_driver::host::UsbHostController;
use embassy_usb_driver::host::{DeviceEvent, UsbHostAllocator, UsbPipe};

use crate::ps5::{AUTH_PAYLOAD_TO_DONGLE, HASH_REQ_CHANNEL, HASH_RES_CHANNEL, PS5_AUTH_DATA};

#[embassy_executor::task]
pub async fn host_task(mut host: HostDriver<'static, USB_OTG_FS, 12>) {
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

                let setup_packet = [0x80, 0x06, 0x00, 0x01, 0x00, 0x00, 18, 0x00];
                let mut descriptor_buf = [0u8; 18];
                if let Err(e) = control_pipe
                    .control_in(&setup_packet, &mut descriptor_buf)
                    .await
                {
                    defmt::error!("Failed to get descriptor: {:?}", e);
                    continue;
                }

                let set_addr_setup = [0x00, 0x05, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00];
                if let Err(e) = control_pipe.control_out(&set_addr_setup, &[]).await {
                    defmt::error!("Failed to set address: {:?}", e);
                    continue;
                }
                Timer::after_millis(10).await;

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

                let set_config_setup = [0x00, 0x09, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00];
                if let Err(e) = control_pipe.control_out(&set_config_setup, &[]).await {
                    defmt::error!("Failed to set configuration: {:?}", e);
                    continue;
                }

                defmt::info!("Dongle configured! Entering passthrough loop...");

                let mut int_out_pipe = host.inner.allocator().alloc_pipe::<
                    embassy_usb_driver::host::pipe::Interrupt,
                    embassy_usb_driver::host::pipe::Out
                >(
                    1,
                    &embassy_usb_driver::EndpointInfo {
                        addr: embassy_usb_driver::EndpointAddress::from_parts(3, embassy_usb_driver::Direction::Out),
                        ep_type: embassy_usb_driver::EndpointType::Interrupt,
                        max_packet_size: 64,
                        interval_ms: 1,
                    },
                    None,
                ).unwrap();

                let mut int_in_pipe = host.inner.allocator().alloc_pipe::<
                    embassy_usb_driver::host::pipe::Interrupt,
                    embassy_usb_driver::host::pipe::In
                >(
                    1,
                    &embassy_usb_driver::EndpointInfo {
                        addr: embassy_usb_driver::EndpointAddress::from_parts(4, embassy_usb_driver::Direction::In),
                        ep_type: embassy_usb_driver::EndpointType::Interrupt,
                        max_packet_size: 64,
                        interval_ms: 1,
                    },
                    None,
                ).unwrap();

                let mut nonce_buf = [0u8; 63];
                let mut state_buf = [0u8; 15];

                loop {
                    // Send Payload to Dongle (if we received one)
                    if let Ok(payload) = AUTH_PAYLOAD_TO_DONGLE.try_receive() {
                        let setup = [0x21, 0x09, 0xF0, 0x03, 0x00, 0x00, 63, 0x00];
                        if control_pipe.control_out(&setup, &payload).await.is_err() {
                            break;
                        }
                        defmt::info!("Sent 0xF0 Auth Payload to dongle.");
                    }

                    // Poll Nonce from Dongle
                    let setup_f1 = [0xA1, 0x01, 0xF1, 0x03, 0x00, 0x00, 63, 0x00];
                    match control_pipe.control_in(&setup_f1, &mut nonce_buf).await {
                        Ok(63) => PS5_AUTH_DATA
                            .lock(|auth| auth.borrow_mut().nonce.copy_from_slice(&nonce_buf)),
                        Err(_) => break,
                        _ => {}
                    }

                    // Poll State from Dongle
                    let setup_f2 = [0xA1, 0x01, 0xF2, 0x03, 0x00, 0x00, 15, 0x00];
                    match control_pipe.control_in(&setup_f2, &mut state_buf).await {
                        Ok(15) => PS5_AUTH_DATA
                            .lock(|auth| auth.borrow_mut().state.copy_from_slice(&state_buf)),
                        Err(_) => break,
                        _ => {}
                    }

                    // Handle Gamepad Report Hashing using the new Interrupt Pipes
                    if let Ok(report) = HASH_REQ_CHANNEL.try_receive() {
                        let mut buf = [0u8; 64];

                        // Serialize the report struct into bytes
                        unsafe {
                            core::ptr::copy_nonoverlapping(
                                &report as *const _ as *const u8,
                                buf.as_mut_ptr(),
                                64,
                            );
                        }

                        // Write to Dongle (the second parameter is `ensure_transaction_end`, we can set it to false here)
                        if let Err(e) = int_out_pipe.request_out(&buf, false).await {
                            defmt::error!("Interrupt OUT Error: {:?}", e);
                            break; // Pipe broken, exit loop
                        }

                        // Immediately read the hashed report back
                        let mut in_buf = [0u8; 64];
                        if let Err(e) = int_in_pipe.request_in(&mut in_buf).await {
                            defmt::error!("Interrupt IN Error: {:?}", e);
                            break;
                        }

                        // The last 8 bytes are the crypto hash signature
                        let mut real_hash = [0u8; 8];
                        real_hash.copy_from_slice(&in_buf[56..64]);

                        // Feed it back to the PS5 task
                        let _ = HASH_RES_CHANNEL.try_send(real_hash);
                    }

                    Timer::after(Duration::from_millis(5)).await;
                }

                // If we broke out of the inner loop, it means the pipe returned an error (disconnected).
                defmt::info!("Passthrough loop exited (Dongle Unplugged).");
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
