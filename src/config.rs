//! Main user configuration file. Edit this, run `cargo build --release`, and flash!

use crate::hardware::types::{BootOverride, Button::*, InputMode, InputMode::*, Profile};

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

/// A dedicated hardware pin that, when grounded, reboots the microcontroller with a software reset.
/// Set to `None` if you don't have a dedicated reboot button.
pub const REBOOT_PIN: Option<u8> = Some(26);

// ==========================================
// 2. PROFILE MANAGEMENT
// ==========================================

/// Buttons you must hold down to trigger a profile switch.
pub const PROFILE_MODIFIER: &[crate::hardware::types::Button] = &[Start, Select];

/// While holding the modifier buttons, press these to switch profiles.
pub const PROFILE_NEXT: crate::hardware::types::Button = Right;
pub const PROFILE_PREV: crate::hardware::types::Button = Left;

// ==========================================
// 3. HARDWARE PIN MAPPINGS
// ==========================================

/// Define your layouts here. The first profile is the default.
pub const PROFILES: &[Profile] = &[
    // --- PROFILE 0: Standard Fighting Game Layout ---
    Profile::new("Standard FightStick")
        .bind(2, Up)
        .bind(3, Down)
        .bind(4, Left)
        .bind(5, Right)
        .bind(6, Action1) // Light Punch (Square / X)
        .bind(7, Action2) // Medium Punch (Triangle / Y)
        .bind(8, Action3) // Heavy Punch (R1 / RB)
        .bind(9, Action4) // All Punch (L1 / LB)
        .bind(10, Action5) // Light Kick (Cross / A)
        .bind(11, Action6) // Medium Kick (Circle / B)
        .bind(12, Action7) // Heavy Kick (R2 / RT)
        .bind(13, Action8) // All Kick (L2 / LT)
        .bind(14, Start)
        .bind(15, Select)
        .bind(16, Home)
        .bind(17, Touchpad),
    // --- PROFILE 1: Platformer / Smash Alternative ---
    Profile::new("Platformer")
        .bind(2, Action1) // Put "Jump" on the Up button
        .bind(3, Down)
        .bind(4, Left)
        .bind(5, Right)
        // ... rest of the binds
        .bind(14, Start)
        .bind(15, Select),
];

// ==========================================
// COMPILE-TIME VALIDATION (Do not touch)
// ==========================================
const _: () = crate::hardware::validation::validate_config(
    PROFILES,
    REBOOT_PIN,
    PROFILE_MODIFIER,
    BOOT_OVERRIDES,
);
