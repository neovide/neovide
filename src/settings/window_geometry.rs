use crate::renderer::Renderer;
use crate::settings::SETTINGS;
use crate::window::WindowSettings;
use glutin::dpi::PhysicalSize;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy)]
pub struct WindowGeometry {
    pub width: u64,
    pub height: u64,
}

pub fn try_to_load_last_window_size() -> Result<WindowGeometry, String> {
    let mut settings_path = dirs::home_dir().unwrap();
    settings_path.push(".local/share/nvim/neovide-settings.txt");
    let serialized_size = std::fs::read_to_string(&settings_path).map_err(|e| e.to_string())?;

    let de: PhysicalSize<u32> =
        serde_json::from_str(&serialized_size).map_err(|e| e.to_string())?;
    log::debug!("Loaded Window Size: {:?}", de);
    Ok(WindowGeometry {
        width: de.width as u64,
        height: de.height as u64,
    })
}
#[cfg(unix)]
fn neovim_std_datapath() -> PathBuf {
    let mut settings_path = dirs::home_dir().unwrap();
    settings_path.push(".local/share/nvim/neovide-settings.txt");
    settings_path
}
#[cfg(windows)]
fn neovim_std_datapath() -> PathBuf {
    let mut settings_path = dirs::home_dir().unwrap();
    settings_path.push("AppData/Local/nvim-data/neovide-settings.txt");
    settings_path
}

const PRIVATE_WINDOW_GEOMETRY: WindowGeometry = WindowGeometry {
    width: 100,
    height: 50,
};

lazy_static! {
    pub static ref DEFAULT_WINDOW_GEOMETRY: WindowGeometry = try_to_load_last_window_size()
        .or::<String>(Ok(PRIVATE_WINDOW_GEOMETRY))
        .unwrap();
}

pub fn maybe_save_window_size(window_size: PhysicalSize<u32>, renderer: &Renderer) {
    let saved_window_size = if SETTINGS.get::<WindowSettings>().remember_dimension {
        PhysicalSize::<u32> {
            width: (window_size.width as f32 / renderer.font_width as f32) as u32,
            height: (window_size.height as f32 / renderer.font_height as f32) as u32,
        }
    } else {
        PhysicalSize::<u32> {
            width: PRIVATE_WINDOW_GEOMETRY.width as u32,
            height: PRIVATE_WINDOW_GEOMETRY.height as u32,
        }
    };

    let settings_path = neovim_std_datapath();
    let se = serde_json::to_string(&saved_window_size).unwrap();
    //log::debug!("Saved Window Size: {}", se);
    std::fs::write(settings_path, se).unwrap();
}

pub fn parse_window_geometry(geometry: Option<String>) -> Result<WindowGeometry, String> {
    geometry.map_or(Ok(*DEFAULT_WINDOW_GEOMETRY), |input| {
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
