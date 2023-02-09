use std::path::PathBuf;

use glutin::dpi::PhysicalPosition;
use serde::{Deserialize, Serialize};

use crate::{dimensions::Dimensions, settings::SETTINGS, window::WindowSettings};

const SETTINGS_FILE: &str = "neovide-settings.json";

pub const DEFAULT_WINDOW_GEOMETRY: Dimensions = Dimensions {
    width: 100,
    height: 50,
};

#[derive(Serialize, Deserialize, Debug)]
pub enum PersistentWindowSettings {
    Maximized,
    Windowed {
        #[serde(default)]
        position: PhysicalPosition<i32>,
        #[serde(default)]
        size: Dimensions,
    },
}

#[derive(Serialize, Deserialize)]
struct PersistentSettings {
    window: PersistentWindowSettings,
}

#[cfg(windows)]
fn neovim_std_datapath() -> PathBuf {
    let mut data_path = dirs::home_dir().unwrap();
    data_path.push("AppData/local/nvim-data");
    data_path
}

#[cfg(unix)]
fn neovim_std_datapath() -> PathBuf {
    let xdg_dirs = xdg::BaseDirectories::with_prefix("nvim").unwrap();
    xdg_dirs.get_data_home()
}

fn settings_path() -> PathBuf {
    let mut settings_path = neovim_std_datapath();
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
    let mut loaded_settings = settings.window;
    log::debug!("Loaded window settings: {:?}", loaded_settings);

    if let PersistentWindowSettings::Windowed { size, .. } = &mut loaded_settings {
        if size.width == 0 || size.height == 0 {
            *size = DEFAULT_WINDOW_GEOMETRY;
        }
    }

    Ok(loaded_settings)
}

pub fn last_window_geometry() -> Dimensions {
    load_last_window_settings()
        .and_then(|window_settings| {
            if let PersistentWindowSettings::Windowed { size, .. } = window_settings {
                Ok(size)
            } else {
                Err(String::from("Window was maximized"))
            }
        })
        .unwrap_or(DEFAULT_WINDOW_GEOMETRY)
}

pub fn save_window_geometry(
    maximized: bool,
    grid_size: Option<Dimensions>,
    position: Option<PhysicalPosition<i32>>,
) {
    let window_settings = SETTINGS.get::<WindowSettings>();

    let settings = PersistentSettings {
        window: if maximized && window_settings.remember_window_size {
            PersistentWindowSettings::Maximized
        } else {
            PersistentWindowSettings::Windowed {
                size: {
                    window_settings
                        .remember_window_size
                        .then_some(grid_size)
                        .flatten()
                        .unwrap_or(DEFAULT_WINDOW_GEOMETRY)
                },
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
    std::fs::create_dir_all(neovim_std_datapath()).unwrap();
    let json = serde_json::to_string(&settings).unwrap();
    log::debug!("Saved Window Settings: {}", json);
    std::fs::write(settings_path, json).unwrap();
}

pub fn parse_window_geometry(input: &str) -> Result<Dimensions, String> {
    let invalid_parse_err = format!("Invalid geometry: {input}\nValid format: <width>x<height>");

    input
        .split('x')
        .map(|dimension| {
            dimension
                .parse::<u64>()
                .map_err(|_| invalid_parse_err.as_str())
                .and_then(|dimension| {
                    if dimension > 0 {
                        Ok(dimension)
                    } else {
                        Err("Invalid geometry: Window dimensions should be greater than 0.")
                    }
                })
        })
        .collect::<Result<Vec<_>, &str>>()
        .and_then(|dimensions| {
            if let [width, height] = dimensions[..] {
                Ok(Dimensions { width, height })
            } else {
                Err(invalid_parse_err.as_str())
            }
        })
        .map_err(|msg| msg.to_owned())
}
