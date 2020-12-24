/// Information about how to translate keyboard into Vim input
pub struct Keypress<'a> {
    /// The Vim input token corresponding to the keypress
    text: &'a str,
    /// Whether the token should be enclosed in brackets, such as <Esc> or <BS>
    special: bool,
    /// Whether the shift key was pressed
    shift: bool,
    // Whether the control key was pressed
    ctrl: bool,
    /// Whether the alt key was pressed
    alt: bool,
}

impl<'a> Keypress<'a> {
    pub const fn new(text: &'a str, special: bool, shift: bool, ctrl: bool, alt: bool) -> Self {
        Self {
            text,
            special,
            shift,
            ctrl,
            alt,
        }
    }

    pub fn as_token(&self, gui: bool) -> String {
        let mut result = self.text.to_string();
        let mut special = if result == "<" {
            result = "lt".to_string();
            true
        } else {
            self.special
        };

        if self.shift {
            special = true;
            result = format!("S-{}", result);
        }
        if self.ctrl {
            special = true;
            result = format!("C-{}", result);
        }
        if self.alt {
            special = true;
            result = format!("M-{}", result);
        }
        if cfg!(not(target_os = "windows")) && gui {
            special = true;
            result = format!("D-{}", result);
        }

        if special {
            result = format!("<{}>", result);
        }

        result
    }
}