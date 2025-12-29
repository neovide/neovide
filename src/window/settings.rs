use rmpv::Value;

use crate::error_msg;
use crate::settings::*;

#[cfg(target_os = "macos")]
pub use crate::platform::macos::settings::*;

#[derive(Clone, SettingGroup, PartialEq)]
pub struct WindowSettings {
    pub background_color: String,
    pub confirm_quit: bool,
    pub cursor_hack: bool,
    pub fullscreen: bool,
    pub has_mouse_grid_detection: bool,
    pub hide_mouse_when_typing: bool,
    pub input_ime: bool,
    pub iso_layout: bool,
    pub normal_opacity: f32,
    #[alias = "transparency"]
    pub opacity: f32,
    pub padding_bottom: u32,
    pub padding_left: u32,
    pub padding_right: u32,
    pub padding_top: u32,
    pub refresh_rate: u64,
    pub refresh_rate_idle: u64,
    pub remember_window_position: bool,
    pub remember_window_size: bool,
    pub scale_factor: f32,
    pub show_border: bool,
    pub theme: ThemeSettings,
    pub touch_deadzone: f32,
    pub touch_drag_timeout: f32,
    pub window_blurred: bool,

    #[cfg(target_os = "macos")]
    pub input_macos_alt_is_meta: bool,
    #[cfg(target_os = "macos")]
    pub input_macos_option_key_is_meta: OptionAsMeta,
    #[cfg(target_os = "macos")]
    pub macos_simple_fullscreen: bool,
    #[cfg(target_os = "windows")]
    pub title_background_color: String,
    #[cfg(target_os = "windows")]
    pub title_text_color: String,

    #[option = "mousemoveevent"]
    pub mouse_move_event: bool,
    #[option = "lines"]
    pub observed_lines: Option<u64>,
    #[option = "columns"]
    pub observed_columns: Option<u64>,
}

impl Default for WindowSettings {
    fn default() -> Self {
        Self {
            background_color: "".to_string(),
            confirm_quit: true,
            cursor_hack: true,
            fullscreen: false,
            has_mouse_grid_detection: false,
            hide_mouse_when_typing: false,
            input_ime: true,
            iso_layout: false,
            normal_opacity: 1.0,
            opacity: 1.0,
            padding_bottom: 0,
            padding_left: 0,
            padding_right: 0,
            padding_top: 0,
            refresh_rate: 60,
            refresh_rate_idle: 5,
            remember_window_position: true,
            remember_window_size: true,
            scale_factor: 1.0,
            show_border: true,
            theme: ThemeSettings::Auto,
            touch_deadzone: 6.0,
            touch_drag_timeout: 0.17,
            window_blurred: false,

            #[cfg(target_os = "macos")]
            input_macos_alt_is_meta: false,
            #[cfg(target_os = "macos")]
            input_macos_option_key_is_meta: OptionAsMeta::None,
            #[cfg(target_os = "macos")]
            macos_simple_fullscreen: false,
            #[cfg(target_os = "windows")]
            title_background_color: "".to_string(),
            #[cfg(target_os = "windows")]
            title_text_color: "".to_string(),

            // Neovim options
            mouse_move_event: false,
            observed_columns: None,
            observed_lines: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeSettings {
    Auto,
    Dark,
    Light,
    BgColor,
}

impl ParseFromValue for ThemeSettings {
    fn parse_from_value(&mut self, value: Value) {
        if value.is_str() {
            *self = match value.as_str().unwrap() {
                "auto" => ThemeSettings::Auto,
                "dark" => ThemeSettings::Dark,
                "light" => ThemeSettings::Light,
                "bg_color" => ThemeSettings::BgColor,
                value => {
                    error_msg!("Setting Theme expected one of `auto`, `dark`, `light`, `bg_color`, but received {value:?}");
                    return;
                }
            };
        } else {
            error_msg!("Setting Theme expected string, but received {value:?}");
        }
    }
}

impl From<ThemeSettings> for Value {
    fn from(value: ThemeSettings) -> Self {
        match value {
            ThemeSettings::Auto => Value::from("auto"),
            ThemeSettings::Dark => Value::from("dark"),
            ThemeSettings::Light => Value::from("light"),
            ThemeSettings::BgColor => Value::from("bg_color"),
        }
    }
}
