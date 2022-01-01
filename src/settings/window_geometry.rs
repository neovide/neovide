use crate::settings::SETTINGS;
use crate::utils::Dimensions;
use crate::window::WindowSettings;
use glutin::dpi::PhysicalPosition;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
#[cfg(unix)]
use xdg;

const SETTINGS_FILE: &str = "neovide-settings.json";

pub const DEFAULT_WINDOW_GEOMETRY: Dimensions = Dimensions {
    width: 100,
    height: 50,
};

#[derive(Serialize, Deserialize)]
struct PersistentWindowSettings {
    #[serde(default)]
    position: PhysicalPosition<i32>,
    #[serde(default = "default_size")]
    size: Dimensions,
}

fn default_size() -> Dimensions {
    DEFAULT_WINDOW_GEOMETRY
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
    let json = std::fs::read_to_string(&settings_path).map_err(|e| e.to_string())?;
    serde_json::from_str(&json).map_err(|e| e.to_string())
}

pub fn try_to_load_last_window_size() -> Result<Dimensions, String> {
    let settings = load_settings()?;
    let loaded_geometry = settings.window.size;
    log::debug!("Loaded Window Size: {:?}", loaded_geometry);

    if loaded_geometry.width == 0 || loaded_geometry.height == 0 {
        log::warn!("Invalid Saved Window Size. Reverting to default");
        Ok(DEFAULT_WINDOW_GEOMETRY)
    } else {
        Ok(loaded_geometry)
    }
}

pub fn load_last_window_position() -> PhysicalPosition<i32> {
    let settings = load_settings();
    if let Ok(settings) = settings {
        let loaded_position = settings.window.position;
        log::debug!("Loaded Window Position: {:?}", loaded_position);
        loaded_position
    } else {
        PhysicalPosition::default()
    }
}

pub fn save_window_geometry(
    grid_size: Option<Dimensions>,
    position: Option<PhysicalPosition<i32>>,
) {
    let window_settings = SETTINGS.get::<WindowSettings>();
    let settings = PersistentSettings {
        window: PersistentWindowSettings {
            size: {
                window_settings
                    .remember_window_size
                    .then(|| grid_size)
                    .flatten()
                    .unwrap_or(DEFAULT_WINDOW_GEOMETRY)
            },
            position: {
                window_settings
                    .remember_window_position
                    .then(|| position)
                    .flatten()
                    .unwrap_or_default()
            },
        },
    };

    let settings_path = settings_path();
    std::fs::create_dir_all(neovim_std_datapath()).unwrap();
    let json = serde_json::to_string(&settings).unwrap();
    log::debug!("Saved Window Settings: {}", json);
    std::fs::write(settings_path, json).unwrap();
}

pub fn parse_window_geometry(geometry: Option<String>) -> Result<Dimensions, String> {
    let saved_window_size =
        try_to_load_last_window_size().or::<String>(Ok(DEFAULT_WINDOW_GEOMETRY));
    geometry.map_or(saved_window_size, |input| {
        let invalid_parse_err = format!(
            "Invalid geometry: {}\nValid format: <width>x<height>",
            input
        );

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
    })
}
