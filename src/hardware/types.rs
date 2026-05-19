const MAX_PINS: usize = 30;

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
