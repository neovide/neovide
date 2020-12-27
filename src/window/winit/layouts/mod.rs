mod qwerty;

use crate::window::keyboard::Modifiers;
use skulpin::winit::event::ModifiersState;

pub use qwerty::handle_qwerty_layout;

impl From<Option<ModifiersState>> for Modifiers {
    fn from(state: Option<ModifiersState>) -> Modifiers {
        if let Some(modifiers) = state {
            Modifiers {
                shift: state.shift(),
                control: state.ctrl(),
                meta: state.alt(),
                logo: state.logo(),
            }
        } else {
            Modifiers::new(false, false, false, false)
        }
    }
}
