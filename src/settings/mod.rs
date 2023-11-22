//! Neovide's settings are stored in a central lazy_static variable called SETTINGS. It contains
//! a hashmap of sub-settings structs that are indexed by their type.
//!
//! The value of each setting are sourced from 4 different places and overwrite each other in this
//! order:
//! 1. Config File
//! 2. Environment Variables
//! 3. Command Line Arguments
//! 4. Neovim Variables and Options
//!
//! This order was selected such that settings methods that change more infrequently are overwritten by
//! settings that change more frequently. A command line argument is more effermable than an environment
//! variable. Similarly a user's config may define commands which increment or decrement a global
//! neovide variable which they wouldn't want to be overwritten by the value in the file.
//!
//! Lastly, some settings are not changeable after startup. These settings are required to be set
//! either before window creation or before the neovim process is started and so cannot be sourced
//! easily from neovim global state.

mod cmd_line;
mod config_file;
mod from_value;
mod nvim_state;
mod window_size;

use parking_lot::RwLock;
use rmpv::Value;
use std::{
    any::{Any, TypeId},
    collections::HashMap,
    fmt::Debug,
    sync::Arc,
};

pub use cmd_line::*;
pub use config_file::Config;
pub use from_value::ParseFromValue;
pub use nvim_state::{NvimStateManager, SettingGroup, SettingLocation};
pub use window_size::{
    load_last_window_settings, save_window_size, PersistentWindowSettings, DEFAULT_GRID_SIZE,
    MAX_GRID_SIZE, MIN_GRID_SIZE,
};

lazy_static! {
    pub static ref SETTINGS: Arc<SettingsManager> = Arc::new(SettingsManager::new());
    pub static ref NVIM_STATE: NvimStateManager = NvimStateManager::new(&SETTINGS);
}

// The Settings struct acts as a global container where each of Neovide's subsystems can store
// their own settings. It will also coordinate updates between Neovide and nvim to make sure the
// settings remain consistent on both sides.
// Note: As right now we're only sending new setting values to Neovide during the
// read_initial_values call, after that point we should not modify the contents of the Settings
// struct except when prompted by an update event from nvim. Otherwise, the settings in Neovide and
// nvim will get out of sync.
pub struct SettingsManager {
    settings: RwLock<HashMap<TypeId, Box<dyn Any + Send + Sync>>>,
}

impl SettingsManager {
    fn new() -> Self {
        Self {
            settings: RwLock::new(HashMap::new()),
        }
    }

    pub fn set<T: Clone + Send + Sync + 'static>(&self, t: &T) {
        let type_id: TypeId = TypeId::of::<T>();
        let t: T = (*t).clone();
        let mut write_lock = self.settings.write();
        write_lock.insert(type_id, Box::new(t));
    }

    pub fn get<T: Clone + Send + Sync + 'static>(&'_ self) -> T {
        let read_lock = self.settings.read();
        let boxed = &read_lock
            .get(&TypeId::of::<T>())
            .expect("Trying to retrieve a settings object that doesn't exist: {:?}");
        let value: &T = boxed
            .downcast_ref::<T>()
            .expect("Attempted to extract a settings object of the wrong type");
        (*value).clone()
    }
}
