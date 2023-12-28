// Config file handling

use std::env;

use notify::Watcher;
use serde::Deserialize;
use winit::event_loop::EventLoopProxy;

use crate::{error_msg, frame::Frame, window::UserEvent};

use std::path::{Path, PathBuf};

use super::font::FontSettings;

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
#[serde(rename_all = "kebab-case")]
pub struct Config {
    pub wsl: Option<bool>,
    pub no_multigrid: Option<bool>,
    pub maximized: Option<bool>,
    pub vsync: Option<bool>,
    pub srgb: Option<bool>,
    pub fork: Option<bool>,
    pub idle: Option<bool>,
    pub neovim_bin: Option<PathBuf>,
    pub frame: Option<Frame>,
    pub theme: Option<String>,
    pub font: Option<FontSettings>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum HotReloadConfigs {
    Font(Option<FontSettings>),
}

impl Config {
    /// Loads config from `config_path()` and writes it to env variables.
    pub fn init(event_loop_proxy: EventLoopProxy<UserEvent>) {
        let config = Config::load_from_path(&config_path());
        match &config {
            Ok(config) => config.write_to_env(),
            Err(Some(err)) => eprintln!("{err}"),
            Err(None) => {}
        };
        Self::watch_config_file(config.unwrap_or_default(), event_loop_proxy);
    }

    pub fn watch_config_file(init_config: Config, event_loop_proxy: EventLoopProxy<UserEvent>) {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut watcher = notify::RecommendedWatcher::new(
            tx,
            notify::Config::default().with_compare_contents(true),
        )
        .unwrap();
        watcher
            .watch(&config_path(), notify::RecursiveMode::NonRecursive)
            .unwrap();
        std::thread::spawn(move || {
            let mut previous_config = init_config;
            loop {
                match rx.recv() {
                    Ok(_) => {
                        match Config::load_from_path(&config_path()) {
                            Ok(config) => {
                                // compare config and previous, notify if changed
                                if config.font != previous_config.font {
                                    event_loop_proxy
                                        .send_event(UserEvent::ConfigsChanged(Box::new(
                                            HotReloadConfigs::Font(config.font.clone()),
                                        )))
                                        .unwrap();
                                }
                                previous_config = config;
                            }
                            Err(Some(err)) => {
                                error_msg!("Reload config file error: {err}");
                            }
                            _ => {}
                        }
                    }
                    Err(e) => {
                        eprintln!("Error while watching config file: {}", e);
                    }
                }
            }
        });
    }

    fn write_to_env(&self) {
        if let Some(wsl) = self.wsl {
            env::set_var("NEOVIDE_WSL", wsl.to_string());
        }
        if let Some(no_multigrid) = self.no_multigrid {
            env::set_var("NEOVIDE_NO_MULTIGRID", no_multigrid.to_string());
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
        if let Some(fork) = self.fork {
            env::set_var("NEOVIDE_FORK", fork.to_string());
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
