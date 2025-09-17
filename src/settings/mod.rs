pub mod font;
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
use winit::event_loop::EventLoopProxy;

use crate::{bridge::NeovimWriter, window::EventPayload};
pub use from_value::ParseFromValue;
pub use window_size::{
    clamped_grid_size, load_last_window_settings, neovide_std_datapath, save_window_size,
    PersistentWindowSettings, DEFAULT_GRID_SIZE, MIN_GRID_SIZE,
};

pub mod config;
pub use config::{Config, HotReloadConfigs};

pub trait SettingGroup {
    type ChangedEvent: Debug + Clone + Send + Sync + Any;
    fn register(settings: &Settings);
}

// Function types to handle settings updates
type UpdateHandlerFunc = fn(&Settings, Value) -> SettingsChanged;
type ReaderHandlerFunc = fn(&Settings) -> Option<Value>;

// The Settings struct acts as a global container where each of Neovide's subsystems can store
// their own settings. It will also coordinate updates between Neovide and nvim to make sure the
// settings remain consistent on both sides.
// Note: As right now we're only sending new setting values to Neovide during the
// read_initial_values call, after that point we should not modify the contents of the Settings
// struct except when prompted by an update event from nvim. Otherwise, the settings in Neovide and
// nvim will get out of sync.
#[derive(Default, Debug)]
pub struct Settings {
    settings: RwLock<HashMap<TypeId, Box<dyn Any + Send + Sync>>>,
    updaters: RwLock<HashMap<SettingLocation, UpdateHandlerFunc>>,
    readers: RwLock<HashMap<SettingLocation, ReaderHandlerFunc>>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum SettingLocation {
    // Setting from global variable with neovide prefix
    NeovideGlobal(String),
    // Setting from global neovim option
    NeovimOption(String),
}

impl Settings {
    pub fn new() -> Self {
        Self::default()
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

    pub fn setting_locations(&self) -> Vec<SettingLocation> {
        self.updaters.read().keys().cloned().collect()
    }

    pub async fn read_initial_values(&self, nvim: &Neovim<NeovimWriter>) -> Result<()> {
        let deprecated_settings = ["transparency".to_owned()];
        let keys: Vec<SettingLocation> = self
            .updaters
            .read()
            .keys()
            .filter(|key| !matches!(key, SettingLocation::NeovideGlobal(name) if deprecated_settings.contains(name)))
            .cloned()
            .collect();

        for location in keys {
            match &location {
                SettingLocation::NeovideGlobal(name) => {
                    let variable_name = format!("neovide_{name}");
                    match nvim.get_var(&variable_name).await {
                        Ok(value) => {
                            self.updaters.read().get(&location).unwrap()(self, value);
                        }
                        Err(error) => {
                            trace!("Initial value load failed for {name}: {error}");
                            let value = self.readers.read().get(&location).unwrap()(self);
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
                        self.updaters.read().get(&location).unwrap()(self, value);
                    }
                    Err(error) => {
                        trace!("Initial value load failed for {name}: {error}");
                    }
                },
            }
        }
        Ok(())
    }

    pub fn handle_setting_changed_notification(
        &self,
        arguments: Vec<Value>,
        event_loop_proxy: &EventLoopProxy<EventPayload>,
    ) {
        let mut arguments = arguments.into_iter();
        let (name, value) = (arguments.next().unwrap(), arguments.next().unwrap());

        let name: Result<String, _> = name.try_into();
        let name = name.unwrap();

        let event = self
            .updaters
            .read()
            .get(&SettingLocation::NeovideGlobal(name))
            .unwrap()(self, value);
        let _ = event_loop_proxy.send_event(EventPayload::new(
            event.into(),
            winit::window::WindowId::from(0),
        ));
    }

    pub fn handle_option_changed_notification(
        &self,
        arguments: Vec<Value>,
        event_loop_proxy: &EventLoopProxy<EventPayload>,
    ) {
        let mut arguments = arguments.into_iter();
        let (name, value) = (arguments.next().unwrap(), arguments.next().unwrap());

        let name: Result<String, _> = name.try_into();
        let name = name.unwrap();

        let event = self
            .updaters
            .read()
            .get(&SettingLocation::NeovimOption(name))
            .unwrap()(self, value);

        let _ = event_loop_proxy.send_event(EventPayload::new(
            event.into(),
            winit::window::WindowId::from(0),
        ));
    }

    pub fn register<T: SettingGroup>(&self) {
        T::register(self);
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum SettingsChanged {
    Window(crate::window::WindowSettingsChanged),
    Cursor(crate::renderer::cursor_renderer::CursorSettingsChanged),
    Renderer(crate::renderer::RendererSettingsChanged),
    ProgressBar(crate::renderer::progress_bar::ProgressBarSettingsChanged),
    #[cfg(test)]
    Test(tests::TestSettingsChanged),
}

#[cfg(test)]
mod tests {
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

    use async_trait::async_trait;
    use nvim_rs::{Handler, Neovim};

    use super::*;
    use crate::{
        bridge::{
            create_nvim_command,
            session::{NeovimInstance, NeovimSession},
        },
        cmd_line::CmdLineSettings,
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

        fn noop_update(_settings: &Settings, _value: Value) -> SettingsChanged {
            SettingsChanged::Test(TestSettingsChanged::Foo("hello".to_string()))
        }
        fn noop_read(_settings: &Settings) -> Option<Value> {
            None
        }

        settings.set_setting_handlers(location.clone(), noop_update, noop_read);
        let listeners = settings.updaters.read();
        let listener = listeners.get(&location).unwrap();
        assert!(core::ptr::fn_addr_eq(
            noop_update as UpdateHandlerFunc,
            *listener
        ));
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
        settings.register::<TestSettings>();

        //create_nvim_command tries to read from CmdLineSettings.neovim_args
        settings.set::<CmdLineSettings>(&CmdLineSettings::default());

        let command = create_nvim_command(&settings);
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

        settings
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
