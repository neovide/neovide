use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use winit::dpi::{PhysicalPosition, PhysicalSize};

use crate::{
    settings::Settings, units::GridSize, window::WindowSettings, window::WinitWindowWrapper,
};

const SETTINGS_FILE: &str = "neovide-settings.json";

pub const DEFAULT_GRID_SIZE: GridSize<u32> = GridSize {
    width: 100,
    height: 50,
};
pub const MIN_GRID_SIZE: GridSize<u32> = GridSize {
    width: 20,
    height: 6,
};
pub const MAX_GRID_SIZE: GridSize<u32> = GridSize {
    width: 10000,
    height: 1000,
};

#[derive(Serialize, Deserialize, Debug)]
pub enum PersistentWindowSettings {
    Maximized {
        #[serde(default)]
        grid_size: Option<GridSize<u32>>,
    },
    Windowed {
        #[serde(default)]
        position: PhysicalPosition<i32>,
        #[serde(default)]
        pixel_size: Option<PhysicalSize<u32>>,
        #[serde(default)]
        grid_size: Option<GridSize<u32>>,
    },
}

#[derive(Serialize, Deserialize)]
struct PersistentSettings {
    window: PersistentWindowSettings,
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

pub fn neovide_std_datapath() -> PathBuf {
    dirs::data_local_dir().unwrap().join("neovide")
}

pub fn load_last_window_settings() -> Result<PersistentWindowSettings, String> {
    let settings = load_settings()?;
    let loaded_settings = settings.window;
    log::debug!("Loaded window settings: {loaded_settings:?}");

    Ok(loaded_settings)
}

pub fn save_window_size(window_wrapper: &WinitWindowWrapper, settings: &Settings) {
    if window_wrapper.routes.is_empty() {
        return;
    }
    let window_id = window_wrapper.get_focused_route().unwrap();
    let route = window_wrapper.routes.get(&window_id).unwrap();
    let window = route.window.winit_window.clone();

    // Don't save the window size when the window is minimized, since the size can be 0
    // Note wayland can't determine this
    if window.is_minimized() == Some(true) {
        return;
    }
    let maximized = window.is_maximized();
    let pixel_size = window.inner_size();
    let grid_size = window_wrapper.get_grid_size();
    let position = window.outer_position().ok();
    let window_settings = settings.get::<WindowSettings>();

    let settings = PersistentSettings {
        window: if maximized && window_settings.remember_window_size {
            PersistentWindowSettings::Maximized {
                grid_size: { window_settings.remember_window_size.then_some(grid_size) },
            }
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
    log::debug!("Saved Window Settings: {json}");
    std::fs::write(&settings_path, json)
        .unwrap_or_else(|_| panic!("Can't write to {settings_path:?}"));
}

pub fn clamped_grid_size(grid_size: &GridSize<u32>) -> GridSize<u32> {
    grid_size.clamp(MIN_GRID_SIZE, MAX_GRID_SIZE)
}
