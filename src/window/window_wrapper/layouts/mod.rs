mod qwerty;

use crate::window::keyboard::Modifiers;
use glutin::keyboard::ModifiersState;

pub use qwerty::handle_qwerty_layout;

impl From<Option<ModifiersState>> for Modifiers {
    fn from(state: Option<ModifiersState>) -> Modifiers {
        if let Some(modifiers) = state {
            Modifiers {
                shift: modifiers.shift_key(),
                control: modifiers.control_key(),
                meta: modifiers.alt_key(),
                logo: modifiers.super_key(),
            }
        } else {
            Modifiers {
                shift: false,
                control: false,
                meta: false,
                logo: false,
            }
        }
    }
}
