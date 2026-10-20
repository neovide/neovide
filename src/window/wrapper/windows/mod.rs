mod context_menu;

use winit::event::WindowEvent;
use winit::platform::windows::BackdropType;
use winit::platform::windows::Color;
use winit::platform::windows::WindowExtWindows;
use winit::window::Window;
use winit::window::WindowId;

pub(super) use self::context_menu::register_right_click;
pub(super) use self::context_menu::unregister_right_click;
use super::ShortcutHandling;
use super::WindowSettingsChanged;
use super::WinitWindowWrapper;
use crate::window::settings::platform::CornerPreference;

pub(super) fn set_initial_window_appearance(
    window: &Window,
    corner_preference: CornerPreference,
    title_background_color: &str,
    title_text_color: &str,
) {
    window.set_corner_preference(corner_preference.into());
    set_title_colors(window, title_background_color, title_text_color);
}

pub(super) fn set_window_backdrop(window: &Window, blurred: bool) {
    if blurred {
        window.set_system_backdrop(BackdropType::TransientWindow); // Acrylic blur
    }
}

pub(super) fn set_title_colors(window: &Window, background: &str, text: &str) {
    if let Some(color) = WinitWindowWrapper::parse_winit_color(background) {
        window.set_title_background_color(Some(color));
    }
    if let Some(color) = WinitWindowWrapper::parse_winit_color(text) {
        window.set_title_text_color(color);
    }
}

impl WinitWindowWrapper {
    pub(super) fn handle_platform_window_settings_changed(
        &mut self,
        window_ids: &[WindowId],
        changed_setting: WindowSettingsChanged,
    ) {
        match changed_setting {
            WindowSettingsChanged::TitleBackgroundColor(color) => {
                for window_id in window_ids {
                    self.handle_title_background_color(*window_id, &color);
                }
            }
            WindowSettingsChanged::TitleTextColor(color) => {
                for window_id in window_ids {
                    self.handle_title_text_color(*window_id, &color);
                }
            }
            WindowSettingsChanged::CornerPreference(option) => {
                for window_id in window_ids {
                    self.set_corner_preference(*window_id, option);
                }
            }
            _ => {}
        }
    }

    pub(super) fn handle_platform_shortcuts(
        &mut self,
        _window_id: WindowId,
        _event: &WindowEvent,
    ) -> ShortcutHandling {
        ShortcutHandling::default()
    }

    pub(super) fn set_corner_preference(&self, window_id: WindowId, option: CornerPreference) {
        let Some(route) = self.routes.get(&window_id) else {
            return;
        };

        let skia_renderer = route.window.skia_renderer.borrow();
        skia_renderer.window().set_corner_preference(option.into());
    }

    pub(super) fn parse_winit_color(color: &str) -> Option<Color> {
        match csscolorparser::parse(color) {
            Ok(color) => {
                let color = color.to_rgba8();
                Some(Color::from_rgb(color[0], color[1], color[2]))
            }
            _ => None,
        }
    }

    pub(super) fn handle_title_background_color(&self, window_id: WindowId, color: &str) {
        let Some(route) = self.routes.get(&window_id) else {
            return;
        };

        let skia_renderer = route.window.skia_renderer.borrow();
        let winit_color = Self::parse_winit_color(color);
        skia_renderer.window().set_title_background_color(winit_color);
    }

    pub(super) fn handle_title_text_color(&self, window_id: WindowId, color: &str) {
        let Some(route) = self.routes.get(&window_id) else {
            return;
        };

        let skia_renderer = route.window.skia_renderer.borrow();
        if let Some(winit_color) = Self::parse_winit_color(color) {
            skia_renderer.window().set_title_text_color(winit_color);
        }
    }
}
