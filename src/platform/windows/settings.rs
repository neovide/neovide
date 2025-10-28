use crate::settings::*;
use neovide_derive::SettingGroup;
use std::path::PathBuf;

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

pub fn neovide_config_dir() -> PathBuf {
    let mut path = dirs::config_dir().unwrap();
    path.push("neovide");
    path
}
