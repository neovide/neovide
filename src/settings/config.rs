// Config file handling

use serde::{Deserialize, Serialize};
use skia_safe::font::Edging;

use glutin::dpi::PhysicalPosition;

use crate::settings::SETTINGS;
use crate::utils::Dimensions;

use super::DEFAULT_WINDOW_GEOMETRY;

use std::path::PathBuf;

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct ConfigFile {
    pub font_antialias: FontAntialias,
    pub multigrid: bool,
    pub maximized: bool,
    //pub window: ConfigWindowSettings,
}

#[derive(Debug, Deserialize, Serialize, Default, Clone, Copy)]
pub struct ConfigSettings {
    pub font_antialias: FontAntialias,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy)]
pub struct ConfigWindowSettings {
    pub position: PhysicalPosition<i32>,
    pub geometry: Dimensions,
}

impl Default for ConfigWindowSettings {
    fn default() -> Self {
        Self {
            position: PhysicalPosition::default(),
            geometry: DEFAULT_WINDOW_GEOMETRY,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
pub enum FontAntialias {
    Off,
    On,
    Subpixel,
}

impl FontAntialias {
    pub fn is_subpixel(&self) -> bool {
        *self == FontAntialias::Subpixel
    }
}

impl Default for FontAntialias {
    fn default() -> Self {
        FontAntialias::On
    }
}

impl From<Edging> for FontAntialias {
    fn from(f: Edging) -> Self {
        match f {
            Edging::Alias => FontAntialias::Off,
            Edging::AntiAlias => FontAntialias::On,
            Edging::SubpixelAntiAlias => FontAntialias::Subpixel,
        }
    }
}

impl From<FontAntialias> for Edging {
    fn from(val: FontAntialias) -> Self {
        match val {
            FontAntialias::Off => Edging::Alias,
            FontAntialias::On => Edging::AntiAlias,
            FontAntialias::Subpixel => Edging::SubpixelAntiAlias,
        }
    }
}

const CONFIG_FILE: &str = "config.toml";

#[cfg(unix)]
fn neovide_config_path() -> PathBuf {
    let xdg_dirs = xdg::BaseDirectories::with_prefix("neovide").unwrap();
    xdg_dirs.get_config_home()
}

#[cfg(windows)]
fn neovide_config_path() -> PathBuf {
    let mut data_path = dirs::home_dir().unwrap();
    // I have no idea where this should be
    todo!("I have no idea where this should be");
    data_path.push("Documents/Neovide/");
    data_path
}

fn config_path() -> PathBuf {
    let mut config_path = neovide_config_path();
    config_path.push(CONFIG_FILE);
    config_path
}

pub fn save_default_config() {
    println!("saving config");
    let config_path = config_path();
    std::fs::create_dir_all(neovide_config_path()).unwrap();

    let toml = toml::to_string(&ConfigFile::default()).unwrap();
    std::fs::write(config_path, &toml).unwrap();
}

pub fn load_config() -> ConfigFile {
    println!("loading config");
    let config_path = config_path();
    let toml = std::fs::read_to_string(config_path);
    let config: ConfigFile = if let Ok(toml) = toml {
        toml::from_str(&toml).unwrap()
    } else {
        println!("Failed to load config, using default and saving default");
        save_default_config();
        ConfigFile::default()
    };

    // I wanted to set font_alias in RendererSettings
    // but it's not something that should be changeable from nvim,
    // so I had to make another setting struct
    let config_settings = ConfigSettings {
        font_antialias: config.font_antialias,
    };
    // and use SETTINGS for global access
    SETTINGS.set::<ConfigSettings>(&config_settings);
    config
}
