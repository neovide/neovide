use std::{any::Any, collections::HashMap, convert::TryInto, fmt::Debug, sync::Arc};

use anyhow::{Context, Result};
use log::trace;
use nvim_rs::Neovim;
use parking_lot::RwLock;
use rmpv::Value;

use crate::bridge::NeovimWriter;

use super::SettingsManager;

// Function types to handle settings updates
type UpdateHandlerFunc = fn(&SettingsManager, Value, bool);
type ReaderHandlerFunc = fn(&SettingsManager) -> Option<Value>;

pub trait SettingGroup: Send + Sync + Clone + 'static {
    type ChangedEvent: Debug + Clone + Send + Sync + Any;
    fn register(nvim_state: &NvimStateManager) -> Self;
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum SettingLocation {
    // Setting from global variable with neovide prefix
    NeovideGlobal(String),
    // Setting from global neovim option
    NeovimOption(String),
}

pub struct NvimStateManager {
    settings: Arc<SettingsManager>,
    updaters: RwLock<HashMap<SettingLocation, UpdateHandlerFunc>>,
    readers: RwLock<HashMap<SettingLocation, ReaderHandlerFunc>>,
}

impl NvimStateManager {
    pub fn new(settings: &Arc<SettingsManager>) -> Self {
        Self {
            settings: settings.clone(),
            updaters: RwLock::new(HashMap::new()),
            readers: RwLock::new(HashMap::new()),
        }
    }

    pub fn set_setting_handlers(
        &self,
        setting_location: SettingLocation,
        update_func: UpdateHandlerFunc,
        reader_func: ReaderHandlerFunc,
    ) {
        self.updaters
            .write()
            .insert(setting_location.clone(), update_func);

        self.readers
            .write()
            .insert(setting_location.clone(), reader_func);
    }

    pub fn setting_locations(&self) -> Vec<SettingLocation> {
        self.updaters.read().keys().cloned().collect()
    }

    pub async fn read_initial_values(&self, nvim: &Neovim<NeovimWriter>) -> Result<()> {
        let keys: Vec<SettingLocation> = self.updaters.read().keys().cloned().collect();

        for location in keys {
            match &location {
                SettingLocation::NeovideGlobal(name) => {
                    let variable_name = format!("neovide_{name}");
                    match nvim.get_var(&variable_name).await {
                        Ok(value) => {
                            self.updaters.read().get(&location).unwrap()(
                                self.settings.as_ref(),
                                value,
                                false,
                            );
                        }
                        Err(error) => {
                            trace!("Initial value load failed for {}: {}", name, error);
                            let value =
                                self.readers.read().get(&location).unwrap()(self.settings.as_ref());
                            if let Some(value) = value {
                                nvim.set_var(&variable_name, value).await.with_context(|| {
                                    format!("Could not set initial value for {name}")
                                })?;
                            }
                        }
                    }
                }
                SettingLocation::NeovimOption(name) => match nvim.get_option(name).await {
                    Ok(value) => {
                        self.updaters.read().get(&location).unwrap()(
                            self.settings.as_ref(),
                            value,
                            false,
                        );
                    }
                    Err(error) => {
                        trace!("Initial value load failed for {}: {}", name, error);
                    }
                },
            }
        }
        Ok(())
    }

    pub fn handle_setting_changed_notification(&self, arguments: Vec<Value>) {
        let mut arguments = arguments.into_iter();
        let (name, value) = (arguments.next().unwrap(), arguments.next().unwrap());

        let name: Result<String, _> = name.try_into();
        let name = name.unwrap();

        self.updaters
            .read()
            .get(&SettingLocation::NeovideGlobal(name))
            .unwrap()(self.settings.as_ref(), value, true);
    }

    pub fn handle_option_changed_notification(&self, arguments: Vec<Value>) {
        let mut arguments = arguments.into_iter();
        let (name, value) = (arguments.next().unwrap(), arguments.next().unwrap());

        let name: Result<String, _> = name.try_into();
        let name = name.unwrap();

        self.updaters
            .read()
            .get(&SettingLocation::NeovimOption(name))
            .unwrap()(self.settings.as_ref(), value, true);
    }

    pub fn register<T: SettingGroup>(&self) {
        let settings = T::register(self);
        self.settings.set(&settings);
    }
}

#[cfg(test)]
mod tests {
    use std::any::TypeId;

    use async_trait::async_trait;
    use nvim_rs::{Handler, Neovim};

    use super::*;
    use crate::{
        bridge::{
            create_nvim_command,
            session::{NeovimInstance, NeovimSession},
        },
        error_handling::ResultPanicExplanation,
        settings::{CmdLineSettings, SETTINGS},
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
        let settings = Arc::new(SettingsManager::new());
        let nvim_state = NvimStateManager::new(&settings);

        let location = SettingLocation::NeovideGlobal("foo".to_owned());

        fn noop_update(_settings: &SettingsManager, _value: Value, _send_changed_event: bool) {}
        fn noop_read(_settings: &SettingsManager) -> Option<Value> {
            None
        }

        nvim_state.set_setting_handlers(location.clone(), noop_update, noop_read);
        let listeners = nvim_state.updaters.read();
        let listener = listeners.get(&location).unwrap();
        assert_eq!(&(noop_update as UpdateHandlerFunc), listener);
    }

    #[test]
    fn test_set() {
        let settings = SettingsManager::new();

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
        let settings = SettingsManager::new();

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
        #[derive(Clone, SettingGroup)]
        struct TestSettings {
            foo: String,
            bar: String,
            baz: String,
            #[option = "mousemoveevent"]
            mousemoveevent_option: Option<bool>,
        }

        impl Default for TestSettings {
            fn default() -> Self {
                Self {
                    foo: "foo".to_string(),
                    bar: "bar".to_string(),
                    baz: "baz".to_string(),
                    mousemoveevent_option: None,
                }
            }
        }

        let settings = Arc::new(SettingsManager::new());
        let nvim_state = NvimStateManager::new(&settings);
        nvim_state.register::<TestSettings>();

        //create_nvim_command tries to read from CmdLineSettings.neovim_args
        //TODO: this sets a static variable. Can this have side effects on other tests?
        SETTINGS.set::<CmdLineSettings>(&CmdLineSettings::default());

        let command =
            create_nvim_command().unwrap_or_explained_panic("Could not create nvim command");
        let instance = NeovimInstance::Embedded(command);
        let NeovimSession { neovim: nvim, .. } = NeovimSession::new(instance, NeovimHandler())
            .await
            .unwrap_or_explained_panic("Could not locate or start the neovim process");
        nvim.set_var("neovide_bar", Value::from("bar_set".to_owned()))
            .await
            .expect("Could not set neovide_bar variable");
        nvim.set_option("mousemoveevent", Value::from(true))
            .await
            .expect("Could not set mousemoveevent option");

        nvim_state
            .read_initial_values(&nvim)
            .await
            .expect("Read initial values failed");

        let test_settings = settings.get::<TestSettings>();
        assert_eq!(test_settings.foo, "foo");
        assert_eq!(test_settings.bar, "bar_set");
        assert_eq!(test_settings.baz, "baz");
        assert_eq!(test_settings.mousemoveevent_option, Some(true));
    }
}
