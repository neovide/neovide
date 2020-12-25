use super::modifiers::Modifiers;

/// Information about how to translate keyboard into Vim input
#[derive(Debug, Clone)]
pub struct Keypress<'a> {
    /// The name of the key in Vimscript.
    /// See `:help key-notation` for more details.
    key_name: &'a str,

    /// Whether the token should be enclosed in brackets, such as <Esc> or <BS>
    special: bool,

    /// Whether the shift key should be considered for inclusion in the token.
    use_shift: bool,
}

impl<'a> Keypress<'a> {
    pub const fn new(key_name: &'a str, special: bool, use_shift: bool) -> Self {
        Self {
            key_name,
            special,
            use_shift,
        }
    }

    /// Converts the keypress to a Neovim input
    pub fn as_token(&self, mods: Modifiers) -> String {
        let shift = self.use_shift && mods.shift;
        let special = self.special || shift || mods.control || mods.meta || use_command(mods.logo);
        let open = if special { "<" } else { "" };
        let command = if use_command(mods.logo) { "D-" } else { "" };
        let shift = if shift { "S-" } else { "" };
        let control = if mods.control { "C-" } else { "" };
        let meta = if mods.meta { "M-" } else { "" };
        let close = if special { ">" } else { "" };
        let result = format!(
            "{}{}{}{}{}{}{}",
            open, command, shift, control, meta, self.key_name, close
        );
        result
    }
}

#[cfg(not(target_os = "windows"))]
fn use_command(gui: bool) -> bool {
    return gui;
}

#[cfg(target_os = "windows")]
fn use_command(_: bool) -> bool {
    false
}
