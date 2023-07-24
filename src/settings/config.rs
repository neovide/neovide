// Config file handling

use std::env;

use serde::Deserialize;

use crate::frame::Frame;

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
    pub wsl: Option<bool>,
    pub multigrid: Option<bool>,
    pub maximized: Option<bool>,
    pub vsync: Option<bool>,
    pub srgb: Option<bool>,
    pub idle: Option<bool>,
    pub neovim_bin: Option<PathBuf>,
    pub frame: Option<Frame>,
    pub theme: Option<String>,
}

impl Config {
    /// Loads config from `config_path()` and writes it to env variables.
    pub fn init() {
        match Config::load_from_path(&config_path()) {
            Ok(config) => config.write_to_env(),
            Err(Some(err)) => eprintln!("{err}"),
            Err(None) => {}
        };
    }

    fn write_to_env(&self) {
        if let Some(wsl) = self.wsl {
            env::set_var("NEOVIDE_WSL", wsl.to_string());
        }
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
        if let Some(idle) = self.idle {
            env::set_var("NEOVIDE_IDLE", idle.to_string());
        }
        if let Some(frame) = self.frame {
            env::set_var("NEOVIDE_FRAME", frame.to_string());
        }
        if let Some(neovim_bin) = &self.neovim_bin {
            env::set_var("NEOVIM_BIN", neovim_bin.to_string_lossy().to_string());
        }
        if let Some(theme) = &self.theme {
            env::set_var("NEOVIDE_THEME", theme);
        }
    }

    fn load_from_path(path: &Path) -> Result<Self, Option<String>> {
        if !path.exists() {
            return Err(None);
        }
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
