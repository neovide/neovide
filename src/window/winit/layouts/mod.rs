mod qwerty;

use crate::window::keyboard::Modifiers;
use skulpin::winit::event::ModifiersState;

pub use qwerty::handle_qwerty_layout;

impl From<Option<ModifiersState>> for Modifiers {
    fn from(state: Option<ModifiersState>) -> Modifiers {
        if let Some(modifiers) = state {
            Modifiers {
                shift: modifiers.shift(),
                control: modifiers.ctrl(),
                meta: modifiers.alt(),
                logo: modifiers.logo(),
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
