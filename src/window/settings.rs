use crate::{cmd_line::CmdLineSettings, settings::*};

#[derive(Clone, SettingGroup, PartialEq)]
pub struct WindowSettings {
    pub refresh_rate: u64,
    pub refresh_rate_idle: u64,
    pub idle: bool,
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
    pub input_macos_alt_is_meta: bool,
    pub input_ime: bool,
    pub unlink_border_highlights: bool,
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
            input_macos_alt_is_meta: false,
            input_ime: true,
            mouse_move_event: false,
            observed_lines: None,
            observed_columns: None,
            unlink_border_highlights: true,
            show_border: false,
        }
    }
}
