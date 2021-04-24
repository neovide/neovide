mod qwerty;

use crate::{settings::SETTINGS, window::keyboard::Modifiers, WindowSettings};
use skulpin::sdl2::keyboard::Mod;

pub use qwerty::handle_qwerty_layout;

impl From<Mod> for Modifiers {
    fn from(mods: Mod) -> Modifiers {
        Modifiers {
            shift: mods.contains(Mod::LSHIFTMOD) || mods.contains(Mod::RSHIFTMOD),
            control: mods.contains(Mod::LCTRLMOD) || mods.contains(Mod::RCTRLMOD),
            meta: mods.contains(Mod::LALTMOD),
            logo: mods.contains(Mod::LGUIMOD) || mods.contains(Mod::RGUIMOD),
        }
    }
}
