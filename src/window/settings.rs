#[cfg(target_os = "macos")]
use {
    log::error,
    rmpv::{Utf8String, Value},
    serde::{
        de::{value, IntoDeserializer},
        Deserialize,
    },
    winit::platform::macos::OptionAsAlt,
};

use crate::{cmd_line::CmdLineSettings, settings::*};

#[derive(Clone, SettingGroup)]
pub struct WindowSettings {
    pub refresh_rate: u64,
    pub refresh_rate_idle: u64,
    pub idle: bool,
    pub transparency: f32,
    pub scale_factor: f32,
    pub fullscreen: bool,
    pub iso_layout: bool,
    pub remember_window_size: bool,
    pub remember_window_position: bool,
    pub hide_mouse_when_typing: bool,
    pub touch_deadzone: f32,
    pub touch_drag_timeout: f32,
    pub background_color: String,
    pub confirm_quit: bool,
    pub padding_top: u32,
    pub padding_left: u32,
    pub padding_right: u32,
    pub padding_bottom: u32,
    pub theme: String,
}

impl Default for WindowSettings {
    fn default() -> Self {
        Self {
            transparency: 1.0,
            scale_factor: 1.0,
            fullscreen: false,
            iso_layout: false,
            refresh_rate: 60,
            refresh_rate_idle: 5,
            idle: SETTINGS.get::<CmdLineSettings>().idle,
            remember_window_size: true,
            remember_window_position: true,
            hide_mouse_when_typing: false,
            touch_deadzone: 6.0,
            touch_drag_timeout: 0.17,
            background_color: "".to_string(),
            confirm_quit: true,
            padding_top: 0,
            padding_left: 0,
            padding_right: 0,
            padding_bottom: 0,
            theme: "".to_string(),
        }
    }
}

#[cfg(target_os = "macos")]
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct OptionAsMeta(pub OptionAsAlt);

#[cfg(target_os = "macos")]
impl ParseFromValue for OptionAsMeta {
    fn parse_from_value(&mut self, value: Value) {
        let s = value.as_str().unwrap();
        match OptionAsAlt::deserialize(s.into_deserializer()) as Result<OptionAsAlt, value::Error> {
            Ok(oa) => *self = OptionAsMeta(oa),
            Err(e) => error!("Setting neovide_input_macos_option_key_is_meta expected one of OnlyLeft, OnlyRight, Both, or None, but received {:?}: {:?}", e, value),
        };
    }
}

#[cfg(target_os = "macos")]
impl From<OptionAsMeta> for Value {
    fn from(oam: OptionAsMeta) -> Self {
        let s = serde_json::to_string(&oam.0).unwrap();
        Value::String(Utf8String::from(s))
    }
}

#[derive(Clone, SettingGroup)]
#[setting_prefix = "input"]
pub struct KeyboardSettings {
    pub macos_alt_is_meta: bool,
    #[cfg(target_os = "macos")]
    pub macos_option_key_is_meta: OptionAsMeta,
    pub ime: bool,
}

#[allow(clippy::derivable_impls)]
impl Default for KeyboardSettings {
    fn default() -> Self {
        Self {
            macos_alt_is_meta: false,
            #[cfg(target_os = "macos")]
            macos_option_key_is_meta: OptionAsMeta(OptionAsAlt::None),
            ime: true,
        }
    }
}
