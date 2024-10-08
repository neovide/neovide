#[cfg(target_os = "macos")]
use {log::error, rmpv::Value};

use crate::settings::*;

#[derive(Clone, SettingGroup, PartialEq)]
pub struct WindowSettings {
    pub refresh_rate: u64,
    pub refresh_rate_idle: u64,
    pub transparency: f32,
    pub window_blurred: bool,
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
    #[cfg(target_os = "windows")]
    pub corner_preference: CornerPreference,
    #[cfg(target_os = "macos")]
    pub input_macos_alt_is_meta: bool,
    #[cfg(target_os = "macos")]
    pub input_macos_option_key_is_meta: OptionAsMeta,
    pub input_ime: bool,
    pub show_border: bool,

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
            transparency: 1.0,
            window_blurred: false,
            scale_factor: 1.0,
            fullscreen: false,
            iso_layout: false,
            refresh_rate: 60,
            refresh_rate_idle: 5,
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
            #[cfg(target_os = "windows")]
            corner_preference: CornerPreference::Default,
            #[cfg(target_os = "macos")]
            input_macos_alt_is_meta: false,
            #[cfg(target_os = "macos")]
            input_macos_option_key_is_meta: OptionAsMeta::None,
            input_ime: true,
            mouse_move_event: false,
            observed_lines: None,
            observed_columns: None,
            show_border: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg(target_os = "windows")]
pub enum CornerPreference {
    Default,
    DoNotRound,
    Round,
    RoundSmall,
}

#[cfg(target_os = "windows")]
impl ParseFromValue for CornerPreference {
    fn parse_from_value(&mut self, value: rmpv::Value) {
        *self = match value.as_str() {
            Some(value) => match value {
                "default" => CornerPreferece::Default,
                "do_not_round" => CornerPreference::DoNotRound,
                "round" => CornerPreference::Round,
                "round_small" => CornerPreference::RoundSmall,
                value => {
                    error!("Bad CornerPreference");
                    return;
                }
            },
            None => {
                error!(
                    "Setting CornerPreference expected string, but received {:?}",
                    value
                );
                return;
            }
        }
    }
}

#[cfg(target_os = "windows")]
impl Into<winit::platform::windows::CornerPreference> for CornerPreference {
    fn into(self) -> winit::platform::windows::CornerPreference {
        use winit::platform::windows::CornerPreference as WinitCornerPreference;
        match self {
            CornerPreference::Default => WinitCornerPreference::Default,
            CornerPreference::DoNotRound => WinitCornerPreference::DoNotRound,
            CornerPreference::Round => WinitCornerPreference::Round,
            CornerPreference::RoundSmall => WinitCornerPreference::RoundSmall,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg(target_os = "macos")]
pub enum OptionAsMeta {
    OnlyLeft,
    OnlyRight,
    Both,
    None,
}

#[cfg(target_os = "macos")]
impl ParseFromValue for OptionAsMeta {
    fn parse_from_value(&mut self, value: Value) {
        if value.is_str() {
            *self = match value.as_str().unwrap() {
                "only_left" => OptionAsMeta::OnlyLeft,
                "only_right" => OptionAsMeta::OnlyRight,
                "both" => OptionAsMeta::Both,
                "none" => OptionAsMeta::None,
                value => {
                    error!("Setting OptionAsMeta expected one of `only_left`, `only_right`, `both`, or `none`, but received {:?}", value);
                    return;
                }
            };
        } else {
            error!(
                "Setting OptionAsMeta expected string, but received {:?}",
                value
            );
        }
    }
}

#[cfg(target_os = "macos")]
impl From<OptionAsMeta> for Value {
    fn from(meta: OptionAsMeta) -> Self {
        match meta {
            OptionAsMeta::OnlyLeft => Value::from("only_left"),
            OptionAsMeta::OnlyRight => Value::from("only_right"),
            OptionAsMeta::Both => Value::from("both"),
            OptionAsMeta::None => Value::from("none"),
        }
    }
}
