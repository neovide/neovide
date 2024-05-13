//! Config file handling

use std::{env, fs, sync::mpsc, time::Duration};

use notify_debouncer_full::{
    new_debouncer,
    notify::{RecursiveMode, Watcher},
};
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

#[derive(Debug, Deserialize, Default, Clone)]
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
    pub title_hidden: Option<bool>,
    pub tabs: Option<bool>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum HotReloadConfigs {
    Font(Option<FontSettings>),
}

impl Config {
    /// Loads config from `config_path()` and writes it to env variables.
    pub fn init() -> Config {
        let config = Config::load_from_path(&config_path());
        match &config {
            Ok(config) => config.write_to_env(),
            Err(Some(err)) => eprintln!("{err}"),
            Err(None) => {}
        };
        config.unwrap_or_default()
    }

    pub fn watch_config_file(init_config: Config, event_loop_proxy: EventLoopProxy<UserEvent>) {
        std::thread::spawn(move || watcher_thread(init_config, event_loop_proxy));
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
        if let Some(title_hidden) = &self.title_hidden {
            env::set_var("NEOVIDE_TITLE_HIDDEN", title_hidden.to_string());
        }
        if let Some(tabs) = &self.tabs {
            env::set_var("NEOVIDE_TABS", tabs.to_string());
        }
    }

    // TODO: should maybe return well-typed error?
    fn load_from_path(path: &Path) -> Result<Self, Option<String>> {
        if !path.exists() {
            return Err(None);
        }
        let toml = fs::read_to_string(path).map_err(|e| {
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

fn watcher_thread(init_config: Config, event_loop_proxy: EventLoopProxy<UserEvent>) {
    let (tx, rx) = mpsc::channel();
    let mut debouncer = new_debouncer(Duration::from_millis(500), None, tx).unwrap();

    if let Err(e) = debouncer.watcher().watch(
        // watching the directory rather than the config file itself to also allow it to be deleted/created later on
        config_path()
            .parent()
            .expect("config path to point to a file which must be in some directory"),
        RecursiveMode::NonRecursive,
    ) {
        log::error!("Could not watch config file, chances are it just doesn't exist: {e}");
        return;
    }

    let mut previous_config = init_config;
    // XXX: compiler can't really know that the config_path() function result basically cannot change
    // if that turns out to be a problem for someone, please open an issue and describe why you're modifying
    // the env variables of processes on the fly
    let config_path = config_path();

    loop {
        if let Err(e) = rx.recv() {
            eprintln!("Error while watching config file: {}", e);
            continue;
        }

        let config = match Config::load_from_path(&config_path) {
            Ok(config) => config,
            Err(maybe_err) => {
                if let Some(err) = maybe_err {
                    error_msg!("While reloading config file: {err}");
                }
                continue;
            }
        };

        // notify if font changed
        if config.font != previous_config.font {
            event_loop_proxy
                .send_event(UserEvent::ConfigsChanged(Box::new(HotReloadConfigs::Font(
                    config.font.clone(),
                ))))
                .unwrap();
        }
        previous_config = config;
    }
}
