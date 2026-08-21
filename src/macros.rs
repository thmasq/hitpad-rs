#[rustfmt::skip]
macro_rules! pin_to_bit {
    (PA0) => { 0 };   (PA1) => { 1 };   (PA2) => { 2 };   (PA3) => { 3 };
    (PA4) => { 4 };   (PA5) => { 5 };   (PA6) => { 6 };   (PA7) => { 7 };
    (PA8) => { 8 };   (PA9) => { 9 };   (PA10) => { 10 }; (PA11) => { 11 };
    (PA12) => { 12 }; (PA13) => { 13 }; (PA14) => { 14 }; (PA15) => { 15 };
    (PB0) => { 16 };  (PB1) => { 17 };  (PB2) => { 18 };  (PB3) => { 19 };
    (PB4) => { 20 };  (PB5) => { 21 };  (PB6) => { 22 };  (PB7) => { 23 };
    (PB8) => { 24 };  (PB9) => { 25 };  (PB10) => { 26 }; (PB11) => { 27 };
    (PB12) => { 28 }; (PB13) => { 29 }; (PB14) => { 30 }; (PB15) => { 31 };
    (PC0) => { 32 };  (PC1) => { 33 };  (PC2) => { 34 };  (PC3) => { 35 };
    (PC4) => { 36 };  (PC5) => { 37 };  (PC6) => { 38 };  (PC7) => { 39 };
    (PC8) => { 40 };  (PC9) => { 41 };  (PC10) => { 42 }; (PC11) => { 43 };
    (PC12) => { 44 }; (PC13) => { 45 }; (PC14) => { 46 }; (PC15) => { 47 };
}

#[rustfmt::skip]
macro_rules! pin_to_adc {
    (PA0) => { $crate::types::AdcPin { adc1_ch: Some(0),  adc2_ch: Some(0),  fast: true  } };
    (PA1) => { $crate::types::AdcPin { adc1_ch: Some(1),  adc2_ch: Some(1),  fast: true  } };
    (PA2) => { $crate::types::AdcPin { adc1_ch: Some(14), adc2_ch: Some(14), fast: false } };
    (PA3) => { $crate::types::AdcPin { adc1_ch: Some(15), adc2_ch: Some(15), fast: false } };
    (PA4) => { $crate::types::AdcPin { adc1_ch: Some(18), adc2_ch: None,     fast: false } };
    (PA5) => { $crate::types::AdcPin { adc1_ch: None,     adc2_ch: Some(18), fast: false } };
    (PA6) => { $crate::types::AdcPin { adc1_ch: Some(3),  adc2_ch: Some(3),  fast: true  } };
    (PA7) => { $crate::types::AdcPin { adc1_ch: Some(7),  adc2_ch: Some(7),  fast: false } };
    (PB0) => { $crate::types::AdcPin { adc1_ch: Some(9),  adc2_ch: Some(9),  fast: false } };
    (PB1) => { $crate::types::AdcPin { adc1_ch: Some(5),  adc2_ch: Some(5),  fast: true  } };
    (PC0) => { $crate::types::AdcPin { adc1_ch: Some(10), adc2_ch: Some(10), fast: false } };
    (PC1) => { $crate::types::AdcPin { adc1_ch: Some(11), adc2_ch: Some(11), fast: false } };
    (PC2) => { $crate::types::AdcPin { adc1_ch: Some(12), adc2_ch: Some(12), fast: false } };
    (PC3) => { $crate::types::AdcPin { adc1_ch: Some(13), adc2_ch: Some(13), fast: false } };
    (PC4) => { $crate::types::AdcPin { adc1_ch: Some(4),  adc2_ch: Some(4),  fast: true  } };
    (PC5) => { $crate::types::AdcPin { adc1_ch: Some(8),  adc2_ch: Some(8),  fast: false } };
    ($other:ident) => { $crate::types::AdcPin { adc1_ch: None, adc2_ch: None, fast: false } };
}

macro_rules! parse_binding {
    (Analog, $inner:ident) => {
        $crate::types::ButtonBinding::Analog($crate::types::Button::$inner)
    };
    (AnalogSingle, $inner:ident) => {
        $crate::types::ButtonBinding::AnalogSingle($crate::types::Button::$inner)
    };
    (Digital, $inner:ident) => {
        $crate::types::ButtonBinding::Digital($crate::types::Button::$inner)
    };
    ($btn:ident,) => {
        $crate::types::ButtonBinding::Digital($crate::types::Button::$btn)
    };
}

macro_rules! check_binding {
    ($pin:ident, Analog, $inner:ident) => {
        assert!(
            pin_to_adc!($pin).adc1_ch.is_some() && pin_to_adc!($pin).adc2_ch.is_some(),
            concat!("Pin ", stringify!($pin), " is mapped to Analog, but it does not support dual ADCs. Use AnalogSingle or Digital.")
        );
    };
    ($pin:ident, AnalogSingle, $inner:ident) => {
        assert!(
            pin_to_adc!($pin).adc1_ch.is_none() || pin_to_adc!($pin).adc2_ch.is_none(),
            concat!("Pin ", stringify!($pin), " is mapped to AnalogSingle, but it supports dual ADCs. Use Analog instead.")
        );
        assert!(
            pin_to_adc!($pin).adc1_ch.is_some() || pin_to_adc!($pin).adc2_ch.is_some(),
            concat!("Pin ", stringify!($pin), " is mapped to AnalogSingle, but it has no ADC channels.")
        );
    };
    ($pin:ident, Digital, $inner:ident) => {};
    ($pin:ident, $btn:ident) => {};
}

macro_rules! define_gamepad_config {
    (
        $( reboot_pin: $reboot:ident, )?
        profiles: [
            $(
                $name:literal => {
                    $( $pin:ident : $btn:ident $(($inner:ident))? ),* $(,)?
                }
            ),* $(,)?
        ],
        adc1_sequence: [ $($adc1_pin:ident),* $(,)? ],
        adc2_sequence: [ $($adc2_pin:ident),* $(,)? ]
        $(,)?
    ) => {
        pub const REBOOT_PIN: Option<u8> = {
            let mut _r = None;
            $( _r = Some(pin_to_bit!($reboot)); )?
            _r
        };

        pub const PROFILES: &[$crate::types::Profile] = &[
            $(
                $crate::types::Profile::new($name)
                $( .bind(pin_to_bit!($pin), parse_binding!($btn, $($inner)?)) )*
            ),*
        ];

        pub const ADC1_SEQUENCE: &[u8] = &[ $( pin_to_bit!($adc1_pin) ),* ];
        pub const ADC2_SEQUENCE: &[u8] = &[ $( pin_to_bit!($adc2_pin) ),* ];

        pub const ADC1_CHANNELS: &[u8] = &[ $( pin_to_adc!($adc1_pin).adc1_ch.unwrap() ),* ];
        pub const ADC2_CHANNELS: &[u8] = &[ $( pin_to_adc!($adc2_pin).adc2_ch.unwrap() ),* ];

        const _: () = {
            // Check that every pin mapped to Analog/AnalogSingle matches the hardware capabilities
            $(
                $(
                    check_binding!($pin, $btn $(, $inner)?);
                )*
            )*

            // Check ADC1 sequence pins exist on ADC1
            $(
                assert!(
                    pin_to_adc!($adc1_pin).adc1_ch.is_some(),
                    concat!("Pin ", stringify!($adc1_pin), " in adc1_sequence does not have an ADC1 channel.")
                );
            )*

            // Check ADC2 sequence pins exist on ADC2
            $(
                assert!(
                    pin_to_adc!($adc2_pin).adc2_ch.is_some(),
                    concat!("Pin ", stringify!($adc2_pin), " in adc2_sequence does not have an ADC2 channel.")
                );
            )*

            // Check sequence lengths match
            assert!(
                [$(pin_to_bit!($adc1_pin)),*].len() == [$(pin_to_bit!($adc2_pin)),*].len(),
                "adc1_sequence and adc2_sequence must have the same length for Dual ADC mode!"
            );
        };


        #[macro_export]
        macro_rules! claim_gamepad_pins {
            ($p:expr) => {
                #[allow(non_snake_case)]
                #[inline(always)]
                fn ERROR_PIN_ALREADY_CLAIMED_BY_GAMEPAD_CONFIG<T>(_: T) {}

                match () {
                    $(
                        $(
                            _ if false => { ERROR_PIN_ALREADY_CLAIMED_BY_GAMEPAD_CONFIG(&$p.$pin); }
                        )*
        			)*
        			$(
                        _ if false => { ERROR_PIN_ALREADY_CLAIMED_BY_GAMEPAD_CONFIG(&$p.$reboot); }
        			)?
        			_ => {}
                }
            }
        }
    };
}
