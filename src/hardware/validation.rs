use super::types::{BootOverride, Button, Profile};

/// Evaluated entirely during `cargo build`
pub const fn validate_config(
    profiles: &[Profile],
    reboot_pin: Option<u8>,
    modifiers: &[Button],
    overrides: &[BootOverride],
) {
    if profiles.is_empty() {
        panic!("Configuration Error: You must define at least one Profile!");
    }

    let mut profile_idx = 0;
    while profile_idx < profiles.len() {
        let profile = &profiles[profile_idx];

        // --- REBOOT PIN CHECK ---
        if let Some(r_pin) = reboot_pin {
            if profile.pin_map[r_pin as usize].is_some() {
                panic!("Hardware Conflict: A button is mapped to the REBOOT_PIN!");
            }
        }

        let mut pin_idx = 0;
        while pin_idx < profile.pin_map.len() {
            if let Some(btn_a) = profile.pin_map[pin_idx] {
                // --- BUTTON DEDUPLICATION ---
                // Check all subsequent pins to ensure this button isn't mapped twice
                let mut check_idx = pin_idx + 1;
                while check_idx < profile.pin_map.len() {
                    if let Some(btn_b) = profile.pin_map[check_idx] {
                        if btn_a as u8 == btn_b as u8 {
                            panic!(
                                "Configuration Error: A button is bound to multiple pins in the same profile!"
                            );
                        }
                    }
                    check_idx += 1;
                }
            }
            pin_idx += 1;
        }

        // --- ANTI-TRAP CHECK ---
        // Ensure this profile contains all the buttons needed to switch profiles
        let mut mod_idx = 0;
        while mod_idx < modifiers.len() {
            let req_btn = modifiers[mod_idx];
            let mut found = false;
            let mut p_idx = 0;

            while p_idx < profile.pin_map.len() {
                if let Some(mapped_btn) = profile.pin_map[p_idx] {
                    if mapped_btn as u8 == req_btn as u8 {
                        found = true;
                        break;
                    }
                }
                p_idx += 1;
            }

            if !found {
                panic!(
                    "Profile Trap Error: A profile is missing a required PROFILE_MODIFIER button!"
                );
            }
            mod_idx += 1;
        }

        profile_idx += 1;
    }

    // --- 4. BOOT OVERRIDE CHECK (Only applies to Default Profile 0) ---
    let default_profile = &profiles[0];
    let mut ovr_idx = 0;
    while ovr_idx < overrides.len() {
        let req_btn = overrides[ovr_idx].button;
        let mut found = false;
        let mut p_idx = 0;

        while p_idx < default_profile.pin_map.len() {
            if let Some(mapped_btn) = default_profile.pin_map[p_idx] {
                if mapped_btn as u8 == req_btn as u8 {
                    found = true;
                    break;
                }
            }
            p_idx += 1;
        }

        if !found {
            panic!(
                "Override Error: A BootOverride button is not mapped on the default Profile (Profile 0)!"
            );
        }
        ovr_idx += 1;
    }
}
