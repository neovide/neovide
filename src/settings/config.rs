// Config file handling

use std::env;

use serde::{Deserialize, Serialize};

use crate::dimensions::Dimensions;

use std::path::{Path, PathBuf};

const CONFIG_FILE: &str = "config.toml";

fn neovide_config_path() -> PathBuf {
    let mut path = dirs::config_dir().unwrap();
    path.push("neovide");
    path
}

pub fn config_path() -> PathBuf {
    let mut config_path = neovide_config_path();
    config_path.push(CONFIG_FILE);
    config_path
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct Config {
    pub multi_grid: Option<bool>,
    pub maximized: Option<bool>,
    pub vsync: Option<bool>,
    pub geometry: Option<Dimensions>,
}

impl Config {
    pub fn write_to_env(&self) {
        if let Some(multi_grid) = self.multi_grid {
            env::set_var("NEOVIDE_MULTIGRID", multi_grid.to_string());
        }
        if let Some(maximized) = self.maximized {
            env::set_var("NEOVIDE_MAXIMIZED", maximized.to_string());
        }
        if let Some(vsync) = self.vsync {
            env::set_var("NEOVIDE_VSYNC", (vsync).to_string());
        }
        if let Some(geometry) = self.geometry {
            env::set_var("NEOVIDE_GEOMETRY", (geometry).to_string());
        }
    }

    pub fn load_from_path(path: &Path) -> Result<Self, String> {
        let toml = std::fs::read_to_string(path).map_err(|e| {
            format!(
                "Error while trying to open config file {}:\n{}\nContinuing with default config.",
                path.to_string_lossy(),
                e
            )
        })?;
        let config = toml::from_str(&toml).map_err(|e| {
            format!(
                "Error while parsing config file {}:\n{}\nContinuing with default config.",
                path.to_string_lossy(),
                e
            )
        })?;
        Ok(config)
    }
}
