use crate::settings::SETTINGS;
use crate::utils::Dimensions;
use crate::window::WindowSettings;
use std::path::PathBuf;

#[cfg(unix)]
const SETTINGS_PATH: &str = ".local/share/nvim/neovide-settings.json";
#[cfg(windows)]
const SETTINGS_PATH: &str = "AppData/Local/nvim-data/neovide-settings.json";

pub const DEFAULT_WINDOW_GEOMETRY: Dimensions = Dimensions {
    width: 100,
    height: 50,
};

fn neovim_std_datapath() -> PathBuf {
    let mut settings_path = dirs::home_dir().unwrap();
    settings_path.push(SETTINGS_PATH);
    settings_path
}

pub fn try_to_load_last_window_size() -> Result<Dimensions, String> {
    let settings_path = neovim_std_datapath();
    let json = std::fs::read_to_string(&settings_path).map_err(|e| e.to_string())?;

    let loaded_geometry: Dimensions = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    log::debug!("Loaded Window Size: {:?}", loaded_geometry);

    if loaded_geometry.width == 0 || loaded_geometry.height == 0 {
        log::warn!("Invalid Saved Window Size. Reverting to default");
        Ok(DEFAULT_WINDOW_GEOMETRY)
    } else {
        Ok(loaded_geometry)
    }
}

pub fn maybe_save_window_size(grid_size: Option<Dimensions>) {
    let settings = SETTINGS.get::<WindowSettings>();
    let saved_window_size = if settings.remember_window_size {
        grid_size.unwrap_or(DEFAULT_WINDOW_GEOMETRY)
    } else {
        DEFAULT_WINDOW_GEOMETRY
    };

    let settings_path = neovim_std_datapath();
    let json = serde_json::to_string(&saved_window_size).unwrap();
    log::debug!("Saved Window Size: {}", json);
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
