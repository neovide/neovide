use crate::settings::*;

#[derive(Clone, SettingGroup, PartialEq)]
pub struct WindowsSettings {
    pub title_background_color: String,
    pub title_text_color: String,
}

impl Default for WindowsSettings {
    fn default() -> Self {
        Self {
            title_background_color: "".to_string(),
            title_text_color: "".to_string(),
        }
    }
}
