use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::convert::TryInto;

use flexi_logger::{Cleanup, Criterion, Duplicate, Logger, Naming};
use log::{error, warn};
use nvim_rs::compat::tokio::Compat;
use nvim_rs::Neovim;
use parking_lot::RwLock;
pub use rmpv::Value;
use tokio::process::ChildStdin;

use crate::error_handling::ResultPanicExplanation;

lazy_static! {
    pub static ref SETTINGS: Settings = Settings::new();
}

// Trait to allow for conversion from rmpv::Value to any other data type.
// Note: Feel free to implement this trait for custom types in each subsystem.
// The reverse conversion (MyType->Value) can be performed by implementing `From<MyType> for Value`
pub trait FromValue {
    fn from_value(&mut self, value: Value);
}

// FromValue implementations for most typical types
impl FromValue for f32 {
    fn from_value(&mut self, value: Value) {
        if value.is_f64() {
            *self = value.as_f64().unwrap() as f32;
        } else if value.is_i64() {
            *self = value.as_i64().unwrap() as f32;
        } else if value.is_u64() {
            *self = value.as_u64().unwrap() as f32;
        } else {
            error!("Setting expected an f32, but received {:?}", value);
        }
    }
}

impl FromValue for u64 {
    fn from_value(&mut self, value: Value) {
        if value.is_u64() {
            *self = value.as_u64().unwrap();
        } else {
            error!("Setting expected a u64, but received {:?}", value);
        }
    }
}

impl FromValue for u32 {
    fn from_value(&mut self, value: Value) {
        if value.is_u64() {
            *self = value.as_u64().unwrap() as u32;
        } else {
            error!("Setting expected a u32, but received {:?}", value);
        }
    }
}

impl FromValue for i32 {
    fn from_value(&mut self, value: Value) {
        if value.is_i64() {
            *self = value.as_i64().unwrap() as i32;
        } else {
            error!("Setting expected an i32, but received {:?}", value);
        }
    }
}

impl FromValue for String {
    fn from_value(&mut self, value: Value) {
        if value.is_str() {
            *self = String::from(value.as_str().unwrap());
        } else {
            error!("Setting expected a string, but received {:?}", value);
        }
    }
}

impl FromValue for bool {
    fn from_value(&mut self, value: Value) {
        if value.is_bool() {
            *self = value.as_bool().unwrap();
        } else if value.is_u64() {
            *self = value.as_u64().unwrap() != 0;
        } else {
            error!("Setting expected a string, but received {:?}", value);
        }
    }
}

// Macro to register settings changed handlers.
// Note: Invocations to this macro must happen before the call to Settings::read_initial_values.
#[macro_export]
macro_rules! register_nvim_setting {
    ($vim_setting_name: expr, $type_name:ident :: $field_name: ident) => {{
        // The update func sets a new value for a setting
        fn update_func(value: Value) {
            let mut s = SETTINGS.get::<$type_name>();
            s.$field_name.from_value(value);
            SETTINGS.set(&s);
        }

        // The reader func retrieves the current value for a setting
        fn reader_func() -> Value {
            let s = SETTINGS.get::<$type_name>();
            s.$field_name.into()
        }

        SETTINGS.set_setting_handlers($vim_setting_name, update_func, reader_func);
    }};
}

// Function types to handle settings updates
type UpdateHandlerFunc = fn(Value);
type ReaderFunc = fn() -> Value;

// The Settings struct acts as a global container where each of Neovide's subsystems can store
// their own settings. It will also coordinate updates between Neovide and nvim to make sure the
// settings remain consistent on both sides.
// Note: As right now we're only sending new setting values to Neovide during the
// read_initial_values call, after that point we should not modify the contents of the Settings
// struct except when prompted by an update event from nvim. Otherwise, the settings in Neovide and
// nvim will get out of sync.
pub struct Settings {
    pub neovim_arguments: Vec<String>,
    settings: RwLock<HashMap<TypeId, Box<dyn Any + Send + Sync>>>,
    listeners: RwLock<HashMap<String, UpdateHandlerFunc>>,
    readers: RwLock<HashMap<String, ReaderFunc>>,
}

impl Settings {
    fn new() -> Settings {
        let mut log_to_file = false;
        let neovim_arguments = std::env::args()
            .filter(|arg| {
                if arg == "--log" {
                    log_to_file = true;
                    false
                } else if arg.starts_with("--geometry=") {
                    false
                } else if arg == "--wsl" {
                    false
                } else {
                    true
                }
            })
            .collect::<Vec<String>>();

        if log_to_file {
            Logger::with_env_or_str("neovide")
                .duplicate_to_stderr(Duplicate::Error)
                .log_to_file()
                .rotate(
                    Criterion::Size(10_000_000),
                    Naming::Timestamps,
                    Cleanup::KeepLogFiles(1),
                )
                .start()
                .expect("Could not start logger");
        } else {
            Logger::with_env_or_str("neovide = error")
                .start()
                .expect("Could not start logger");
        }

        Settings {
            neovim_arguments,
            settings: RwLock::new(HashMap::new()),
            listeners: RwLock::new(HashMap::new()),
            readers: RwLock::new(HashMap::new()),
        }
    }

    pub fn set_setting_handlers(
        &self,
        property_name: &str,
        update_func: UpdateHandlerFunc,
        reader_func: ReaderFunc,
    ) {
        self.listeners
            .write()
            .insert(String::from(property_name), update_func);
        self.readers
            .write()
            .insert(String::from(property_name), reader_func);
    }

    pub fn set<T: Clone + Send + Sync + 'static>(&self, t: &T) {
        let type_id: TypeId = TypeId::of::<T>();
        let t: T = (*t).clone();
        self.settings.write().insert(type_id, Box::new(t));
    }

    pub fn get<'a, T: Clone + Send + Sync + 'static>(&'a self) -> T {
        let read_lock = self.settings.read();
        let boxed = &read_lock
            .get(&TypeId::of::<T>())
            .expect("Trying to retrieve a settings object that doesn't exist");
        let value: &T = boxed
            .downcast_ref::<T>()
            .expect("Attempted to extract a settings object of the wrong type");
        (*value).clone()
    }

    pub async fn read_initial_values(&self, nvim: &Neovim<Compat<ChildStdin>>) {
        let keys: Vec<String> = self.listeners.read().keys().cloned().collect();
        for name in keys {
            let variable_name = format!("neovide_{}", name.to_string());
            match nvim.get_var(&variable_name).await {
                Ok(value) => {
                    self.listeners.read().get(&name).unwrap()(value);
                }
                Err(error) => {
                    warn!("Initial value load failed for {}: {}", name, error);
                    let setting = self.readers.read().get(&name).unwrap()();
                    nvim.set_var(&variable_name, setting).await.ok();
                }
            }
        }
    }

    pub async fn setup_changed_listeners(&self, nvim: &Neovim<Compat<ChildStdin>>) {
        let keys: Vec<String> = self.listeners.read().keys().cloned().collect();
        for name in keys {
            let vimscript = format!(
                concat!(
                    "exe \"",
                    "fun! NeovideNotify{0}Changed(d, k, z)\n",
                    "call rpcnotify(1, 'setting_changed', '{0}', g:neovide_{0})\n",
                    "endf\n",
                    "call dictwatcheradd(g:, 'neovide_{0}', 'NeovideNotify{0}Changed')\"",
                ),
                name
            );
            nvim.command(&vimscript)
                .await
                .unwrap_or_explained_panic(&format!(
                    "Could not setup setting notifier for {}",
                    name
                ));
        }
    }

    pub fn handle_changed_notification(&self, arguments: Vec<Value>) {
        let mut arguments = arguments.into_iter();
        let (name, value) = (arguments.next().unwrap(), arguments.next().unwrap());

        let name: Result<String, _> = name.try_into();
        let name = name.unwrap();

        self.listeners.read().get(&name).unwrap()(value);
    }
}
