use crate::{ANALOG_BUTTON_STATE, DEBOUNCED_STATE, config, types};
use embassy_stm32::pac;
use embassy_time::{Duration, Ticker, Timer};
use portable_atomic::Ordering;

#[allow(clippy::inline_always, clippy::similar_names)]
#[inline(always)]
pub fn read_gpio_state(digital_mask: u64) -> u64 {
    let porta = pac::GPIOA.idr().read().0 as u64;
    let portb = pac::GPIOB.idr().read().0 as u64;
    let portc = pac::GPIOC.idr().read().0 as u64;

    let combined = (portc << 32) | (portb << 16) | porta;

    !combined & digital_mask
}

#[embassy_executor::task]
pub async fn digital_sampler_task(initial_state: u64, digital_mask: u64) {
    let mut history = [0u64; 16];
    history.fill(initial_state);
    let mut history_idx = 0;
    let mut current_debounced = initial_state;

    let mut ticker = Ticker::every(Duration::from_micros(50));

    loop {
        let raw_state = read_gpio_state(digital_mask);

        history[history_idx] = raw_state;
        history_idx = (history_idx + 1) % 16;

        let mut all_ones = 0xFFFF_FFFF_FFFF_FFFF;
        let mut all_zeros = 0xFFFF_FFFF_FFFF_FFFF;

        for state in &history {
            all_ones &= state;
            all_zeros &= !state;
        }

        current_debounced = (current_debounced | all_ones) & !all_zeros;
        DEBOUNCED_STATE.store(current_debounced, Ordering::Relaxed);

        ticker.next().await;
    }
}

#[embassy_executor::task]
pub async fn analog_sampler_task() {
    defmt::info!("Initializing Dual ADC...");

    pac::RCC.ahb1enr().modify(|w| w.set_adc12en(true));

    pac::ADC1.cr().modify(|w| w.set_advregen(true));
    pac::ADC2.cr().modify(|w| w.set_advregen(true));

    Timer::after_micros(20).await;

    pac::ADC1.cr().modify(|w| w.set_adcal(true));
    while pac::ADC1.cr().read().adcal() {}
    pac::ADC2.cr().modify(|w| w.set_adcal(true));
    while pac::ADC2.cr().read().adcal() {}

    // Common Settings
    // DUAL = 0b00110 (Regular simultaneous mode only)
    // MDMA = 0b10 (MDMA mode for 12 to 10-bit resolution)
    pac::ADC12_COMMON.ccr().modify(|w| {
        w.set_dual(pac::adccommon::vals::Dual::from_bits(0b00110));
        w.set_mdma(pac::adccommon::vals::Mdma::from_bits(0b10));
    });

    let config_adc = |adc: pac::adc::Adc, channels: &[u8]| {
        let ovsr_bits = match crate::ADC_OVERSAMPLING {
            2 => 0b0000,
            4 => 0b0001,
            8 => 0b0010,
            16 => 0b0011,
            32 => 0b0100,
            64 => 0b0101,
            128 => 0b0110,
            256 => 0b0111,
            _ => panic!("Unsupported ADC_OVERSAMPLING ratio!"),
        };

        adc.cfgr2().modify(|w| {
            w.set_rovse(true);
            w.set_ovsr(ovsr_bits.into());
            w.set_ovss(0);
        });

        adc.sqr1().modify(|w| {
            w.set_l((channels.len() - 1) as u8);
            if channels.len() > 0 {
                w.set_sq(0, channels[0]);
            }
            if channels.len() > 1 {
                w.set_sq(1, channels[1]);
            }
            if channels.len() > 2 {
                w.set_sq(2, channels[2]);
            }
            if channels.len() > 3 {
                w.set_sq(3, channels[3]);
            }
        });
        adc.sqr2().modify(|w| {
            if channels.len() > 4 {
                w.set_sq(4, channels[4]);
            }
            if channels.len() > 5 {
                w.set_sq(5, channels[5]);
            }
            if channels.len() > 6 {
                w.set_sq(6, channels[6]);
            }
            if channels.len() > 7 {
                w.set_sq(7, channels[7]);
            }
        });

        adc.smpr1().modify(|w| {
            for i in 0..10 {
                w.set_smp(i, pac::adc::vals::SampleTime::from_bits(0b110));
            }
        });
        adc.smpr2().modify(|w| {
            for i in 10..20 {
                w.set_smp(i, pac::adc::vals::SampleTime::from_bits(0b110));
            }
        });

        adc.isr().modify(|w| w.set_adrdy(true));
        adc.cr().modify(|w| w.set_aden(true));
        while !adc.isr().read().adrdy() {}
    };

    config_adc(pac::ADC1, config::ADC1_CHANNELS);
    config_adc(pac::ADC2, config::ADC2_CHANNELS);

    defmt::info!("Dual ADC Initialized! Starting 8kHz loop...");

    let mut ticker = Ticker::every(Duration::from_micros(125));

    let mut ema_positions = [0i32; 16];

    loop {
        pac::ADC1.cr().modify(|w| w.set_adstart(true));

        let mut analog_buttons = 0u32;

        for i in 0..8 {
            while !pac::ADC1.isr().read().eoc() {}

            let cdr = pac::ADC12_COMMON.cdr().read();
            let adc1_val = cdr.rdata_mst() as i32;
            let adc2_val = cdr.rdata_slv() as i32;

            ema_positions[i] = ema_positions[i] + ((adc1_val - ema_positions[i]) >> 2);
            ema_positions[i + 8] = ema_positions[i + 8] + ((adc2_val - ema_positions[i + 8]) >> 2);

            if ema_positions[i] > 30_000 {
                let pin = config::ADC1_SEQUENCE[i] as usize;
                if let Some(binding) = config::PROFILES[0].pin_map[pin] {
                    analog_buttons |= types::ButtonState::from(binding.button()).bits();
                }
            }
            if ema_positions[i + 8] > 30_000 {
                let pin = config::ADC2_SEQUENCE[i] as usize;
                if let Some(binding) = config::PROFILES[0].pin_map[pin] {
                    analog_buttons |= types::ButtonState::from(binding.button()).bits();
                }
            }
        }

        ANALOG_BUTTON_STATE.store(analog_buttons, Ordering::Relaxed);

        ticker.next().await;
    }
}
