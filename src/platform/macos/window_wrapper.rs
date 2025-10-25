use crate::window::{
    settings::OptionAsMeta, WindowSettings, WindowSettingsChanged, WinitWindowWrapper,
};
use winit::{
    platform::macos::{self, WindowExtMacOS},
    window::Window,
};

use super::MacosWindowFeature;

pub trait WinitWindowWrapperExt {
    fn init_macos(&mut self, window: &Window);
    fn set_macos_option_as_meta(&mut self, option: OptionAsMeta);
    fn set_simple_fullscreen(&mut self, fullscreen: bool);
    fn handle_scale_factor_update_macos(&mut self, scale_factor: f64);
    fn handle_size_changed_macos(&mut self);
    fn extra_titlebar_height_in_pixels_macos(&self) -> u32;
    fn handle_settings_changed_macos(&self, changed_setting: WindowSettingsChanged);
}

impl WinitWindowWrapperExt for WinitWindowWrapper {
    fn init_macos(&mut self, window: &Window) {
        self.macos_feature = Some(MacosWindowFeature::from_winit_window(
            window,
            self.settings.clone(),
        ));
        let WindowSettings {
            input_macos_option_key_is_meta,
            macos_simple_fullscreen,
            ..
        } = self.settings.get::<WindowSettings>();
        self.set_macos_option_as_meta(input_macos_option_key_is_meta);
        self.set_simple_fullscreen(macos_simple_fullscreen);
    }

    fn set_macos_option_as_meta(&mut self, option: OptionAsMeta) {
        let winit_option = match option {
            OptionAsMeta::OnlyLeft => macos::OptionAsAlt::OnlyLeft,
            OptionAsMeta::OnlyRight => macos::OptionAsAlt::OnlyRight,
            OptionAsMeta::Both => macos::OptionAsAlt::Both,
            OptionAsMeta::None => macos::OptionAsAlt::None,
        };

        if let Some(skia_renderer) = &self.skia_renderer {
            let window = skia_renderer.window();
            if winit_option != window.option_as_alt() {
                window.set_option_as_alt(winit_option);
            }
        }
    }

    fn set_simple_fullscreen(&mut self, fullscreen: bool) {
        if let Some(skia_renderer) = &self.skia_renderer {
            let window = skia_renderer.window();
            window.set_simple_fullscreen(fullscreen);
        }
    }

    fn handle_scale_factor_update_macos(&mut self, scale_factor: f64) {
        self.macos_feature
            .as_mut()
            .unwrap()
            .handle_scale_factor_update(scale_factor);
    }

    fn handle_size_changed_macos(&mut self) {
        self.macos_feature.as_mut().unwrap().handle_size_changed();
    }

    fn extra_titlebar_height_in_pixels_macos(&self) -> u32 {
        if let Some(macos_feature) = &self.macos_feature {
            macos_feature.extra_titlebar_height_in_pixels()
        } else {
            0
        }
    }

    fn handle_settings_changed_macos(&self, changed_setting: WindowSettingsChanged) {
        if let Some(macos_feature) = &self.macos_feature {
            macos_feature.handle_settings_changed(changed_setting);
        }
    }
}
