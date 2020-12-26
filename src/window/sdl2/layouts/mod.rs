mod qwerty;

use crate::window::layouts_shared;
use layouts_shared::modifiers::Modifiers;
use skulpin::sdl2::keyboard::Mod;

pub use qwerty::handle_qwerty_layout;

impl Into<Modifiers> for Mod {
    fn into(self) -> Modifiers {
        Modifiers {
            shift: self.contains(Mod::LSHIFTMOD) || self.contains(Mod::RSHIFTMOD),
            control: self.contains(Mod::LCTRLMOD) || self.contains(Mod::RCTRLMOD),
            meta: self.contains(Mod::LALTMOD) || self.contains(Mod::RALTMOD),
            logo: self.contains(Mod::LGUIMOD) || self.contains(Mod::RGUIMOD),
        }
    }
}
