use crate::settings::SETTINGS;
use crate::window::WindowSettings;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[cfg(unix)]
const SETTINGS_PATH: &str = ".local/share/nvim/neovide-settings.json";
#[cfg(windows)]
const SETTINGS_PATH: &str = "AppData/Local/nvim-data/neovide-settings.json";

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub struct WindowGeometry {
    pub width: u64,
    pub height: u64,
}

impl From<(u64, u64)> for WindowGeometry {
    fn from((width, height): (u64, u64)) -> Self {
        WindowGeometry { width, height }
    }
}

fn neovim_std_datapath() -> PathBuf {
    let mut settings_path = dirs::home_dir().unwrap();
    settings_path.push(SETTINGS_PATH);
    settings_path
}

pub fn try_to_load_last_window_size() -> Result<WindowGeometry, String> {
    let settings_path = neovim_std_datapath();
    let serialized_size = std::fs::read_to_string(&settings_path).map_err(|e| e.to_string())?;

    let deserialize_size: WindowGeometry =
        serde_json::from_str(&serialized_size).map_err(|e| e.to_string())?;
    log::debug!("Loaded Window Size: {:?}", deserialize_size);
    Ok(deserialize_size)
}
pub const DEFAULT_WINDOW_GEOMETRY: WindowGeometry = WindowGeometry {
    width: 100,
    height: 50,
};

pub fn maybe_save_window_size(grid_size: Option<WindowGeometry>) {
    let settings = SETTINGS.get::<WindowSettings>();
    let saved_window_size = if settings.remember_window_size && grid_size.is_some() {
        grid_size.unwrap()
    } else {
        WindowGeometry {
            width: DEFAULT_WINDOW_GEOMETRY.width as u64,
            height: DEFAULT_WINDOW_GEOMETRY.height as u64,
        }
    };

    let settings_path = neovim_std_datapath();
    let serialized_size = serde_json::to_string(&saved_window_size).unwrap();
    log::debug!("Saved Window Size: {}", serialized_size);
    std::fs::write(settings_path, serialized_size).unwrap();
}

pub fn parse_window_geometry(geometry: Option<String>) -> Result<WindowGeometry, String> {
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
                    Ok(WindowGeometry { width, height })
                } else {
                    Err(invalid_parse_err.as_str())
                }
            })
            .map_err(|msg| msg.to_owned())
    })
}
