mod qwerty;

use crate::window::keyboard::Modifiers;
use skulpin::winit::event::ModifiersState;

pub use qwerty::handle_qwerty_layout;

impl Into<Modifiers> for Option<ModifiersState> {
    fn into(self) -> Modifiers {
        if let Some(modifiers) = self {
            Modifiers {
                shift: modifiers.shift(),
                control: modifiers.ctrl(),
                meta: modifiers.alt(),
                logo: modifiers.logo(),
            }
        } else {
            Modifiers::new(false, false, false, false)
        }
    }
}
