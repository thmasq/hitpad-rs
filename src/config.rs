//! Main user configuration file. Edit this, run `cargo build --release`, and flash!

use crate::types::{
    BootOverride,
    Button::{self, Action1, Action2, Action3, Left, Right, Select, Start},
    InputMode::{self, Keyboard, PS5, XInput},
};

// ==========================================
// SYSTEM & BOOT SETTINGS
// ==========================================

/// The default mode the controller uses when plugged in normally.
pub const DEFAULT_MODE: InputMode = Keyboard;

/// Hold these buttons while plugging in the USB to override the default mode.
pub const BOOT_OVERRIDES: &[BootOverride] = &[
    BootOverride {
        button: Action1,
        mode: XInput,
    }, // PC (Cross/A)
    BootOverride {
        button: Action2,
        mode: Keyboard,
    }, // Keyboard (Circle/B)
    BootOverride {
        button: Action3,
        mode: PS5,
    }, // PS5 (Square/X)
];

// ==========================================
// PROFILE MANAGEMENT
// ==========================================

/// Buttons you must hold down to trigger a profile switch.
pub const PROFILE_MODIFIER: &[Button] = &[Start, Select];

/// While holding the modifier buttons, press these to switch profiles.
#[allow(dead_code)]
pub const PROFILE_NEXT: Button = Right;
#[allow(dead_code)]
pub const PROFILE_PREV: Button = Left;

// ==========================================
// HARDWARE PIN MAPPINGS
// ==========================================

define_gamepad_config! {
    reboot_pin: PB10,
    profiles: [
        "Standard FightStick" => {
            PC1: Analog(Left),
            PC2: Analog(Right),
            PA0: Analog(Action1),
            PA1: Analog(Action2),
            PA6: Analog(Action3),
            PA2: Analog(Action4),
            PA3: Analog(Action5),
            PA7: Analog(Action6),
            PB0: Analog(Action7),
            PC4: Analog(Action8),
            PB1: Analog(Up),
            PC0: Analog(Down),

            PA4: AnalogSingle(Start),
            PA5: AnalogSingle(Select),
            PC3: Home,
            PC5: Touchpad,
        },
        "Platformer" => {
            PC1: Analog(Left),
            PC2: Analog(Right),
            PC0: Analog(Down),
            PA0: Analog(Action1),
            PC1: Start,
            PC2: Select
        }
    ],
    // The sequence for DMA conversion
    adc1_sequence: [PA4, PA0, PA1, PA6, PA2, PA3, PA7, PB0],
    adc2_sequence: [PA5, PC4, PB1, PC0, PC1, PC2, PC3, PC5],
}

// ==========================================
// COMPILE-TIME VALIDATION (Do not touch)
// ==========================================
const _: () = crate::types::validate_config(
    PROFILES,
    REBOOT_PIN,
    PROFILE_MODIFIER,
    BOOT_OVERRIDES,
    ADC1_SEQUENCE,
    ADC2_SEQUENCE,
);
