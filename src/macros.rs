#[macro_export]
macro_rules! define_gamepad_config {
    (
        $( reboot_pin: $reboot:literal, )?
        profiles: [
            $(
                $name:literal => {
                    $( $pin:literal : $btn:ident ),* $(,)?
                }
            ),* $(,)?
        ]
    ) => {
        pub const REBOOT_PIN: Option<u8> = {
            let mut _r = None;
            $( _r = Some($reboot); )?
            _r
        };

        pub const PROFILES: &[$crate::types::Profile] = &[
            $(
                $crate::types::Profile::new($name)
                $( .bind($pin, $crate::types::Button::$btn) )*
            ),*
        ];

        pub const PIN_MASK: u32 = 0
            $( $( | (1 << $pin) )* )*
    		$( | (1 << $reboot) )?;

        #[macro_export]
        macro_rules! claim_gamepad_pins {
            ($p:expr) => {
                ::paste::paste! {
                    #[allow(non_snake_case)]
                    #[inline(always)]
                    fn ERROR_PIN_ALREADY_CLAIMED_BY_GAMEPAD_CONFIG<T>(_: T) {}

        			match () {
                        $(
            				$(
                                _ if false => { ERROR_PIN_ALREADY_CLAIMED_BY_GAMEPAD_CONFIG($p.[<PIN_ $pin>]); }
            				)*
                        )*
                        $(
            				_ if false => { ERROR_PIN_ALREADY_CLAIMED_BY_GAMEPAD_CONFIG($p.[<PIN_ $reboot>]); }
                        )?
                        _ => {}
        			}
                }
            }
        }
    };
}
