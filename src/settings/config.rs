// Config file handling

use std::env;

use serde::Deserialize;

use crate::{dimensions::Dimensions, frame::Frame};

use std::path::{Path, PathBuf};

const CONFIG_FILE: &str = "config.toml";

#[cfg(unix)]
fn neovide_config_dir() -> PathBuf {
    let xdg_dirs = xdg::BaseDirectories::with_prefix("neovide").unwrap();
    xdg_dirs.get_config_home()
}

#[cfg(windows)]
fn neovide_config_dir() -> PathBuf {
    let mut path = dirs::config_dir().unwrap();
    path.push("neovide");
    path
}

pub fn config_path() -> PathBuf {
    let mut config_path = neovide_config_dir();
    config_path.push(CONFIG_FILE);
    config_path
}

#[derive(Debug, Deserialize, Default)]
pub struct Config {
    pub multigrid: Option<bool>,
    pub maximized: Option<bool>,
    pub vsync: Option<bool>,
    pub srgb: Option<bool>,
    pub no_idle: Option<bool>,
    pub neovim_bin: Option<PathBuf>,
    pub frame: Option<Frame>,
}

impl Config {
    pub fn write_to_env(&self) {
        if let Some(multigrid) = self.multigrid {
            env::set_var("NEOVIDE_MULTIGRID", multigrid.to_string());
        }
        if let Some(maximized) = self.maximized {
            env::set_var("NEOVIDE_MAXIMIZED", maximized.to_string());
        }
        if let Some(vsync) = self.vsync {
            env::set_var("NEOVIDE_VSYNC", vsync.to_string());
        }
        if let Some(srgb) = self.srgb {
            env::set_var("NEOVIDE_SRGB", srgb.to_string());
        }
        if let Some(no_idle) = self.no_idle {
            env::set_var("NEOVIDE_NO_IDLE", no_idle.to_string());
        }
        if let Some(frame) = self.frame {
            env::set_var("NEOVIDE_FRAME", frame.to_string());
        }
        if let Some(neovim_bin) = &self.neovim_bin {
            env::set_var("NEOVIM_BIN", neovim_bin.to_string_lossy().to_string());
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
