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
    reboot_pin: 26,
    profiles: [
        "Standard FightStick" => {
            2: Up,
            3: Down,
            4: Left,
            5: Right,
            6: Action1,
            7: Action2,
            8: Action3,
            9: Action4,
            10: Action5,
            11: Action6,
            12: Action7,
            13: Action8,
            14: Start,
            15: Select,
            16: Home,
            17: Touchpad
        },
        "Platformer" => {
            2: Action1,
            3: Down,
            4: Left,
            5: Right,
            14: Start,
            15: Select
        }
    ]
}

// ==========================================
// COMPILE-TIME VALIDATION (Do not touch)
// ==========================================
const _: () = crate::types::validate_config(PROFILES, REBOOT_PIN, PROFILE_MODIFIER, BOOT_OVERRIDES);
