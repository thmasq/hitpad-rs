use bitflags::bitflags;
use core::marker::ConstParamTy;

const MAX_PINS: usize = 30;

bitflags! {
    /// A bitmask representing the physical buttons currently held down.
    #[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
    pub struct ButtonState: u32 {
        const UP       = 1 << 0;
        const DOWN     = 1 << 1;
        const LEFT     = 1 << 2;
        const RIGHT    = 1 << 3;
        const ACTION1  = 1 << 4;
        const ACTION2  = 1 << 5;
        const ACTION3  = 1 << 6;
        const ACTION4  = 1 << 7;
        const ACTION5  = 1 << 8;
        const ACTION6  = 1 << 9;
        const ACTION7  = 1 << 10;
        const ACTION8  = 1 << 11;
        const L3       = 1 << 12;
        const R3       = 1 << 13;
        const START    = 1 << 14;
        const SELECT   = 1 << 15;
        const HOME     = 1 << 16;
        const TOUCHPAD = 1 << 17;
    }
}

impl From<Button> for ButtonState {
    #[inline(always)]
    fn from(btn: Button) -> Self {
        match btn {
            Button::Up => ButtonState::UP,
            Button::Down => ButtonState::DOWN,
            Button::Left => ButtonState::LEFT,
            Button::Right => ButtonState::RIGHT,
            Button::Action1 => ButtonState::ACTION1,
            Button::Action2 => ButtonState::ACTION2,
            Button::Action3 => ButtonState::ACTION3,
            Button::Action4 => ButtonState::ACTION4,
            Button::Action5 => ButtonState::ACTION5,
            Button::Action6 => ButtonState::ACTION6,
            Button::Action7 => ButtonState::ACTION7,
            Button::Action8 => ButtonState::ACTION8,
            Button::L3 => ButtonState::L3,
            Button::R3 => ButtonState::R3,
            Button::Start => ButtonState::START,
            Button::Select => ButtonState::SELECT,
            Button::Home => ButtonState::HOME,
            Button::Touchpad => ButtonState::TOUCHPAD,
        }
    }
}

/// The global representation of the controller's state during a single tick.
#[derive(Default, Clone, Copy, Debug)]
pub struct GamepadState {
    pub buttons: ButtonState,
    // Note: if I ever decide to add analog trigger/stick support, it would go here.
    // pub left_trigger: u8,
    // pub right_trigger: u8,
}

#[derive(Default, Clone, Copy, Debug, PartialEq, Eq, ConstParamTy)]
pub enum SocdMode {
    /// Up + Down = Neutral, Left + Right = Neutral (CPT Standard)
    #[default]
    Neutral,
    /// Up + Down = Up, Left + Right = Neutral
    UpPriority,
}

impl GamepadState {
    /// Cleans the current directional inputs based on the selected SOCD resolution mode.
    #[inline]
    pub fn apply_socd<const MODE: SocdMode>(&mut self) {
        let left_right = ButtonState::LEFT | ButtonState::RIGHT;
        let up_down = ButtonState::UP | ButtonState::DOWN;

        if self.buttons.contains(left_right) {
            self.buttons.remove(left_right);
        }

        if self.buttons.contains(up_down) {
            match MODE {
                SocdMode::Neutral => {
                    self.buttons.remove(up_down);
                }
                SocdMode::UpPriority => {
                    self.buttons.remove(ButtonState::DOWN);
                }
            }
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum InputMode {
    XInput,
    Keyboard,
    PS5,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Button {
    Up,
    Down,
    Left,
    Right,
    Action1,
    Action2,
    Action3,
    Action4,
    Action5,
    Action6,
    Action7,
    Action8,
    L3,
    R3,
    Start,
    Select,
    Home,
    Touchpad,
}

pub struct BootOverride {
    pub button: Button,
    pub mode: InputMode,
}

#[derive(Copy, Clone)]
pub struct Profile {
    pub name: &'static str,
    // We map GPIO pin index -> Option<Button>
    // e.g., pin_map[2] = Some(Button::Up)
    pub pin_map: [Option<Button>; MAX_PINS],
}

impl Profile {
    /// Creates a new, empty profile entirely at compile-time
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            pin_map: [None; MAX_PINS],
        }
    }

    /// Const builder method.
    pub const fn bind(mut self, pin: u8, button: Button) -> Self {
        if pin as usize >= MAX_PINS {
            panic!("Invalid GPIO pin! RP2040 only supports pins 0-29.");
        }

        self.pin_map[pin as usize] = Some(button);
        self
    }
}

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
