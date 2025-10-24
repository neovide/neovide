use winit::platform::windows::{Color, WindowExtWindows};

use crate::{
    settings::SettingsChanged,
    window::{WindowSettings, WinitWindowWrapper},
};

impl WinitWindowWrapper {
    pub fn handle_window_settings_changed(&mut self, changed_setting: SettingsChanged) {
        match changed_setting {
            SettingsChanged::Window(WindowSettingsChanged::TitleBackgroundColor(color)) => {
                self.handle_title_background_color(&color);
            }
            SettingsChanged::Window(WindowSettingsChanged::TitleTextColor(color)) => {
                self.handle_title_text_color(&color);
            }
            _ => {}
        }
    }

    fn parse_winit_color(color: &str) -> Option<Color> {
        match csscolorparser::parse(color) {
            Ok(color) => {
                let color = color.to_rgba8();
                Some(Color::from_rgb(color[0], color[1], color[2]))
            }
            _ => None,
        }
    }

    fn handle_title_background_color(&self, color: &str) {
        if let Some(skia_renderer) = &self.skia_renderer {
            let winit_color = Self::parse_winit_color(color);
            skia_renderer
                .window()
                .set_title_background_color(winit_color);
        }
    }

    fn handle_title_text_color(&self, color: &str) {
        if let Some(skia_renderer) = &self.skia_renderer {
            if let Some(winit_color) = Self::parse_winit_color(color) {
                skia_renderer.window().set_title_text_color(winit_color);
            }
        }
    }
}
