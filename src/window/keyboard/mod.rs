mod layout;
mod modifiers;
mod settings;
mod token;

use crate::settings::SETTINGS;

pub use self::{
    layout::KeyboardLayout,
    modifiers::Modifiers,
    settings::{initialize_settings, KeyboardSettings},
    token::Token,
};

type KeycodeToTokenFn<T> = fn(T, bool) -> Option<Token<'static>>;

pub fn neovim_keybinding_string<T, U>(
    keycode: Option<U>,
    keytext: Option<String>,
    modifiers: T,
    keycode_to_token: KeycodeToTokenFn<U>,
) -> Option<String>
where
    T: Into<Modifiers>,
{
    let modifiers: Modifiers = modifiers.into();
    if let Some(text) = keytext {
        Some(
            if text == "<" {
                Token::new("lt", true, true)
            } else {
                Token::new(&text, false, false)
            }
            .into_string(modifiers),
        )
    } else if let Some(keycode) = keycode {
        match SETTINGS.get::<KeyboardSettings>().layout {
            KeyboardLayout::Qwerty => keycode_to_token(keycode, modifiers.shift),
        }
        .map(|e| e.into_string(modifiers))
    } else {
        None
    }
}

pub fn unsupported_key<T, R>(keycode: T) -> Option<R>
where
    T: std::fmt::Debug,
{
    log::trace!("Unsupported key: {:?}", keycode);
    None
}
