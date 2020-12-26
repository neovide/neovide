mod qwerty;

use skulpin::winit::event::ModifiersState;

use crate::window::layouts_shared::modifiers::Modifiers;

pub use qwerty::*;

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
