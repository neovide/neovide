use crate::settings::*;

#[derive(Clone, Default, PartialEq, SettingGroup)]
#[setting_prefix = "font"]
pub struct FontSettings {
    pub use_italic_as_oblique: bool,
}
