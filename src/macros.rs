#[macro_export]
macro_rules! pin_to_bit {
    (PA0) => {
        0
    };
    (PA1) => {
        1
    };
    (PA2) => {
        2
    };
    (PA3) => {
        3
    };
    (PA4) => {
        4
    };
    (PA5) => {
        5
    };
    (PA6) => {
        6
    };
    (PA7) => {
        7
    };
    (PA8) => {
        8
    };
    (PA9) => {
        9
    };
    (PA10) => {
        10
    };
    (PA11) => {
        11
    };
    (PA12) => {
        12
    };
    (PA13) => {
        13
    };
    (PA14) => {
        14
    };
    (PA15) => {
        15
    };
    (PB0) => {
        16
    };
    (PB1) => {
        17
    };
    (PB2) => {
        18
    };
    (PB3) => {
        19
    };
    (PB4) => {
        20
    };
    (PB5) => {
        21
    };
    (PB6) => {
        22
    };
    (PB7) => {
        23
    };
    (PB8) => {
        24
    };
    (PB9) => {
        25
    };
    (PB10) => {
        26
    };
    (PB11) => {
        27
    };
    (PB12) => {
        28
    };
    (PB13) => {
        29
    };
    (PB14) => {
        30
    };
    (PB15) => {
        31
    };
}

#[macro_export]
macro_rules! define_gamepad_config {
    (
        $( reboot_pin: $reboot:ident, )?
        profiles: [
            $(
                $name:literal => {
                    $( $pin:ident : $btn:ident ),* $(,)?
                }
            ),* $(,)?
        ]
    ) => {
        pub const REBOOT_PIN: Option<u8> = {
            let mut _r = None;
            $( _r = Some($crate::pin_to_bit!($reboot)); )?
            _r
        };

        pub const PROFILES: &[$crate::types::Profile] = &[
            $(
                $crate::types::Profile::new($name)
                $( .bind($crate::pin_to_bit!($pin), $crate::types::Button::$btn) )*
            ),*
        ];

        pub const PIN_MASK: u32 = 0
            $( $( | (1 << $crate::pin_to_bit!($pin)) )* )*
            $( | (1 << $crate::pin_to_bit!($reboot)) )?;

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
