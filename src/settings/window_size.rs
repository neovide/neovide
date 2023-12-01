use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use winit::dpi::{PhysicalPosition, PhysicalSize};

use crate::{
    dimensions::Dimensions, settings::SETTINGS, window::WindowSettings, window::WinitWindowWrapper,
};

const SETTINGS_FILE: &str = "neovide-settings.json";

pub const DEFAULT_GRID_SIZE: Dimensions = Dimensions {
    width: 100,
    height: 50,
};

pub const MIN_GRID_SIZE: Dimensions = Dimensions {
    width: 20,
    height: 6,
};

pub const MAX_GRID_SIZE: Dimensions = Dimensions {
    width: 10000,
    height: 1000,
};

#[derive(Serialize, Deserialize, Debug)]
pub enum PersistentWindowSettings {
    Maximized,
    Windowed {
        #[serde(default)]
        position: PhysicalPosition<i32>,
        #[serde(default)]
        pixel_size: Option<PhysicalSize<u32>>,
        #[serde(default)]
        grid_size: Option<Dimensions>,
    },
}

#[derive(Serialize, Deserialize)]
struct PersistentSettings {
    window: PersistentWindowSettings,
}

fn neovide_std_datapath() -> PathBuf {
    dirs::data_local_dir().unwrap().join("neovide")
}

fn settings_path() -> PathBuf {
    let mut settings_path = neovide_std_datapath();
    settings_path.push(SETTINGS_FILE);
    settings_path
}

fn load_settings() -> Result<PersistentSettings, String> {
    let settings_path = settings_path();
    let json = std::fs::read_to_string(settings_path).map_err(|e| e.to_string())?;
    serde_json::from_str(&json).map_err(|e| e.to_string())
}

pub fn load_last_window_settings() -> Result<PersistentWindowSettings, String> {
    let settings = load_settings()?;
    let loaded_settings = settings.window;
    log::debug!("Loaded window settings: {:?}", loaded_settings);

    Ok(loaded_settings)
}

pub fn save_window_size(window_wrapper: &WinitWindowWrapper) {
    let window = window_wrapper.windowed_context.window();
    // Don't save the window size when the window is minimized, since the size can be 0
    // Note wayland can't determine this
    if window.is_minimized() == Some(true) {
        return;
    }
    let maximized = window.is_maximized();
    let pixel_size = window.inner_size();
    let grid_size = window_wrapper.get_grid_size();
    let position = window.outer_position().ok();
    let window_settings = SETTINGS.get::<WindowSettings>();

    let settings = PersistentSettings {
        window: if maximized && window_settings.remember_window_size {
            PersistentWindowSettings::Maximized
        } else {
            PersistentWindowSettings::Windowed {
                pixel_size: { window_settings.remember_window_size.then_some(pixel_size) },
                grid_size: { window_settings.remember_window_size.then_some(grid_size) },
                position: {
                    window_settings
                        .remember_window_position
                        .then_some(position)
                        .flatten()
                        .unwrap_or_default()
                },
            }
        },
    };

    let settings_path = settings_path();
    std::fs::create_dir_all(neovide_std_datapath()).unwrap();
    let json = serde_json::to_string(&settings).unwrap();
    log::debug!("Saved Window Settings: {}", json);
    std::fs::write(&settings_path, json)
        .unwrap_or_else(|_| panic!("Can't write to {settings_path:?}"));
}
