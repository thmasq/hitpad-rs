//! Main user configuration file. Edit this, run `cargo build --release`, and flash!

use crate::{
    define_gamepad_config,
    types::{
        BootOverride,
        Button::{self, Action1, Action2, Action3, Left, Right, Select, Start},
        InputMode::{self, Keyboard, PS5, XInput},
    },
};

// ==========================================
// 1. SYSTEM & BOOT SETTINGS
// ==========================================

/// The default mode the controller uses when plugged in normally.
pub const DEFAULT_MODE: InputMode = PS5;

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
// 2. PROFILE MANAGEMENT
// ==========================================

/// Buttons you must hold down to trigger a profile switch.
pub const PROFILE_MODIFIER: &[Button] = &[Start, Select];

/// While holding the modifier buttons, press these to switch profiles.
#[allow(dead_code)]
pub const PROFILE_NEXT: Button = Right;
#[allow(dead_code)]
pub const PROFILE_PREV: Button = Left;

// ==========================================
// 3. HARDWARE PIN MAPPINGS
// ==========================================

define_gamepad_config! {
    reboot_pin: PB10,
    profiles: [
        "Standard FightStick" => {
            PA2: Up,
            PA3: Down,
            PA4: Left,
            PA5: Right,
            PA6: Action1,
            PA7: Action2,
            PA8: Action3,
            PA9: Action4,
            PA10: Action5,
            PA11: Action6,
            PA12: Action7,
            PA13: Action8,
            PA14: Start,
            PA15: Select,
            PB0: Home,
            PB1: Touchpad
        },
        "Platformer" => {
            PA2: Action1,
            PA3: Down,
            PA4: Left,
            PA5: Right,
            PA14: Start,
            PA15: Select
        }
    ]
}

// ==========================================
// COMPILE-TIME VALIDATION (Do not touch)
// ==========================================
const _: () = crate::types::validate_config(PROFILES, REBOOT_PIN, PROFILE_MODIFIER, BOOT_OVERRIDES);
