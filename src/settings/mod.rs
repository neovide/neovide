mod from_value;
mod window_size;

use anyhow::{Context, Result};
use log::trace;
use nvim_rs::Neovim;
use parking_lot::RwLock;
use rmpv::Value;
use std::{
    any::{Any, TypeId},
    collections::HashMap,
    convert::TryInto,
    fmt::Debug,
};

use crate::bridge::NeovimWriter;
pub use from_value::ParseFromValue;
pub use window_size::{
    load_last_window_settings, save_window_size, PersistentWindowSettings, DEFAULT_GRID_SIZE,
    MAX_GRID_SIZE, MIN_GRID_SIZE,
};

mod config;
pub use config::{config_path, Config};

lazy_static! {
    pub static ref SETTINGS: Settings = Settings::new();
}

pub trait SettingGroup {
    fn register(&self);
}

// Function types to handle settings updates
type UpdateHandlerFunc = fn(Value);

// The Settings struct acts as a global container where each of Neovide's subsystems can store
// their own settings. It will also coordinate updates between Neovide and nvim to make sure the
// settings remain consistent on both sides.
// Note: As right now we're only sending new setting values to Neovide during the
// read_initial_values call, after that point we should not modify the contents of the Settings
// struct except when prompted by an update event from nvim. Otherwise, the settings in Neovide and
// nvim will get out of sync.
pub struct Settings {
    settings: RwLock<HashMap<TypeId, Box<dyn Any + Send + Sync>>>,
    listeners: RwLock<HashMap<SettingLocation, UpdateHandlerFunc>>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum SettingLocation {
    // Setting from global variable with neovide prefix
    NeovideGlobal(String),
    // Setting from global neovim option
    NeovimOption(String),
}

impl SettingLocation {
    #[cfg(test)]
    fn name(&self) -> &str {
        match self {
            SettingLocation::NeovideGlobal(name) => name,
            SettingLocation::NeovimOption(name) => name,
        }
    }
}

// Event published when a setting is updated.
#[derive(Clone)]
pub struct SettingChanged<T: Any + Send + Sync> {
    pub field: String,
    _type: std::marker::PhantomData<T>,
}

impl<T: Any + Send + Sync> Debug for SettingChanged<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SettingChanged")
            .field("field", &self.field)
            .field("_type", &self._type)
            .finish()
    }
}

impl<T: Any + Send + Sync> SettingChanged<T> {
    pub fn new(field: &str) -> Self {
        Self {
            field: String::from(field),
            _type: std::marker::PhantomData,
        }
    }
}

impl Settings {
    fn new() -> Self {
        Self {
            settings: RwLock::new(HashMap::new()),
            listeners: RwLock::new(HashMap::new()),
        }
    }

    pub fn set_setting_handlers(
        &self,
        setting_location: SettingLocation,
        update_func: UpdateHandlerFunc,
    ) {
        self.listeners
            .write()
            .insert(setting_location.clone(), update_func);
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

    pub async fn read_initial_values(&self, nvim: &Neovim<NeovimWriter>) -> Result<()> {
        let keys: Vec<SettingLocation> = self.listeners.read().keys().cloned().collect();

        for location in keys {
            match &location {
                SettingLocation::NeovideGlobal(name) => {
                    let variable_name = format!("neovide_{name}");
                    match nvim.get_var(&variable_name).await {
                        Ok(value) => {
                            self.listeners.read().get(&location).unwrap()(value);
                        }
                        Err(error) => {
                            trace!("Initial value load failed for {}: {}", name, error);
                        }
                    }
                }
                SettingLocation::NeovimOption(name) => match nvim.get_option(name).await {
                    Ok(value) => {
                        self.listeners.read().get(&location).unwrap()(value);
                    }
                    Err(error) => {
                        trace!("Initial value load failed for {}: {}", name, error);
                    }
                },
            }
        }
        Ok(())
    }

    pub async fn setup_changed_listeners(&self, nvim: &Neovim<NeovimWriter>) -> Result<()> {
        let keys: Vec<SettingLocation> = self.listeners.read().keys().cloned().collect();

        for location in keys {
            match &location {
                SettingLocation::NeovideGlobal(name) => {
                    let vimscript = format!(
                        concat!(
                            "exe \"",
                            "fun! NeovideNotify{0}Changed(d, k, z)\n",
                            "call rpcnotify(g:neovide_channel_id, 'setting_changed', '{0}', g:neovide_{0})\n",
                            "endf\n",
                            "call dictwatcheradd(g:, 'neovide_{0}', 'NeovideNotify{0}Changed')\"",
                        ),
                        name
                    );
                    nvim.command(&vimscript)
                        .await
                        .with_context(|| format!("Could not setup setting notifier for {name}"))?;
                }
                SettingLocation::NeovimOption(name) => {
                    let vimscript = format!(
                        concat!(
                            "exe \"",
                            "autocmd OptionSet {0} call rpcnotify(g:neovide_channel_id, 'option_changed', '{0}', &{0})\n",
                            "\"",
                        ),
                        name
                    );
                    nvim.command(&vimscript)
                        .await
                        .with_context(|| format!("Could not setup setting notifier for {name}"))?;
                }
            }
        }
        Ok(())
    }

    pub fn handle_setting_changed_notification(&self, arguments: Vec<Value>) {
        let mut arguments = arguments.into_iter();
        let (name, value) = (arguments.next().unwrap(), arguments.next().unwrap());

        let name: Result<String, _> = name.try_into();
        let name = name.unwrap();

        self.listeners
            .read()
            .get(&SettingLocation::NeovideGlobal(name))
            .unwrap()(value);
    }

    pub fn handle_option_changed_notification(&self, arguments: Vec<Value>) {
        let mut arguments = arguments.into_iter();
        let (name, value) = (arguments.next().unwrap(), arguments.next().unwrap());

        let name: Result<String, _> = name.try_into();
        let name = name.unwrap();

        self.listeners
            .read()
            .get(&SettingLocation::NeovimOption(name))
            .unwrap()(value);
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use nvim_rs::{Handler, Neovim};

    use super::*;
    use crate::{
        bridge::{
            create_nvim_command,
            session::{NeovimInstance, NeovimSession},
        },
        error_handling::ResultPanicExplanation,
    };

    #[derive(Clone)]
    pub struct NeovimHandler();

    #[async_trait]
    impl Handler for NeovimHandler {
        type Writer = NeovimWriter;

        async fn handle_notify(
            &self,
            _event_name: String,
            _arguments: Vec<Value>,
            _neovim: Neovim<NeovimWriter>,
        ) {
        }
    }

    #[test]
    fn test_set_setting_handlers() {
        let settings = Settings::new();

        let location = SettingLocation::NeovideGlobal("foo".to_owned());

        fn noop_update(_v: Value) {}

        settings.set_setting_handlers(location.clone(), noop_update);
        let listeners = settings.listeners.read();
        let listener = listeners.get(&location).unwrap();
        assert_eq!(&(noop_update as UpdateHandlerFunc), listener);
    }

    #[test]
    fn test_set() {
        let settings = Settings::new();

        let v1: u32 = 1;
        let v2: f32 = 1.0;
        let vt1 = TypeId::of::<u32>();
        let vt2 = TypeId::of::<f32>();
        let v3: u32 = 2;

        {
            settings.set(&v1);

            let values = settings.settings.read();
            let r1 = values.get(&vt1).unwrap().downcast_ref::<u32>().unwrap();
            assert_eq!(v1, *r1);
        }

        {
            settings.set(&v2);
            settings.set(&v3);

            let values = settings.settings.read();
            let r2 = values.get(&vt1).unwrap().downcast_ref::<u32>().unwrap();
            let r3 = values.get(&vt2).unwrap().downcast_ref::<f32>().unwrap();

            assert_eq!(v3, *r2);
            assert_eq!(v2, *r3);
        }
    }

    #[test]
    fn test_get() {
        let settings = Settings::new();

        let v1: u32 = 1;
        let v2: f32 = 1.0;
        let vt1 = TypeId::of::<u32>();
        let vt2 = TypeId::of::<f32>();

        let mut values = settings.settings.write();
        values.insert(vt1, Box::new(v1));
        values.insert(vt2, Box::new(v2));

        unsafe {
            settings.settings.force_unlock_write();
        }

        let r1 = settings.get::<u32>();
        let r2 = settings.get::<f32>();

        assert_eq!(v1, r1);
        assert_eq!(v2, r2);
    }

    #[tokio::test]
    async fn test_read_initial_values() {
        let settings = Settings::new();

        let v1: SettingLocation = SettingLocation::NeovideGlobal("foo".to_string());
        let v2: SettingLocation = SettingLocation::NeovideGlobal("bar".to_string());
        let v3: SettingLocation = SettingLocation::NeovideGlobal("baz".to_string());
        let v4: SettingLocation = SettingLocation::NeovideGlobal(format!("neovide_{}", v1.name()));
        let v5: SettingLocation = SettingLocation::NeovideGlobal(format!("neovide_{}", v2.name()));

        let command =
            create_nvim_command().unwrap_or_explained_panic("Could not create nvim command");
        let instance = NeovimInstance::Embedded(command);
        let NeovimSession { neovim: nvim, .. } = NeovimSession::new(instance, NeovimHandler())
            .await
            .unwrap_or_explained_panic("Could not locate or start the neovim process");
        nvim.set_var(v4.name(), Value::from(v2.name().to_owned()))
            .await
            .ok();

        fn noop_update(_v: Value) {}

        let mut listeners = settings.listeners.write();
        listeners.insert(v1.clone(), noop_update);
        listeners.insert(v2.clone(), noop_update);

        unsafe {
            settings.listeners.force_unlock_write();
        }

        settings
            .read_initial_values(&nvim)
            .await
            .unwrap_or_explained_panic("Could not read initial values");

        let rt1 = nvim.get_var(v4.name()).await.unwrap();
        let rt2 = nvim.get_var(v5.name()).await.unwrap();

        let r1 = rt1.as_str().unwrap();
        let r2 = rt2.as_str().unwrap();

        assert_eq!(r1, v2.name());
        assert_eq!(r2, v3.name());
    }
}
