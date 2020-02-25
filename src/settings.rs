use std::collections::HashMap;
use std::convert::TryInto;
use std::any::{Any, TypeId};

pub use rmpv::Value;
use nvim_rs::Neovim;
use nvim_rs::compat::tokio::Compat;
use flexi_logger::{Logger, Criterion, Naming, Cleanup};
use tokio::process::ChildStdin;
use parking_lot::RwLock;
use log::{error,warn};

use crate::error_handling::ResultPanicExplanation;

lazy_static! {
    pub static ref SETTINGS: Settings = Settings::new();
}

pub trait FromValue {
    fn from_value(&mut self, value: Value);
}

impl FromValue for f32 {
    fn from_value(&mut self, value: Value) {
        if value.is_f32() {
            *self = value.as_f64().unwrap() as f32; 
        }else{
            error!("Setting expected an f32, but received {:?}", value);
        }

    }
}

impl FromValue for u64 {
    fn from_value(&mut self, value: Value) {
        if value.is_u64() {
            *self = value.as_u64().unwrap(); 
        }else{
            error!("Setting expected a u64, but received {:?}", value);
        }

    }
}

impl FromValue for u32 {
    fn from_value(&mut self, value: Value) {
        if value.is_u64() {
            *self = value.as_u64().unwrap() as u32; 
        }else{
            error!("Setting expected a u32, but received {:?}", value);
        }
    }
}

impl FromValue for i32 {
    fn from_value(&mut self, value: Value) {
        if value.is_i64() {
            *self = value.as_i64().unwrap() as i32; 
        }else{
            error!("Setting expected an i32, but received {:?}", value);
        }
    }
}

impl FromValue for String {
    fn from_value(&mut self, value: Value) {
        if value.is_str() {
            *self = String::from(value.as_str().unwrap());
        }else{
            error!("Setting expected a string, but received {:?}", value);
        }

    }
}

impl FromValue for bool {
    fn from_value(&mut self, value: Value) {
        // TODO -- Warn when incorrect type
        if value.is_bool() {
            *self = value.as_bool().unwrap();
        }
    }
}

#[macro_export]
macro_rules! register_nvim_setting {
    ($vim_setting_name: expr, $type_name:ident :: $field_name: ident) => {{
        fn update_func(value: Value) {
            let mut s = SETTINGS.get::<$type_name>();
            s.$field_name.from_value(value);
            SETTINGS.set(&s);
        }

        fn reader_func() -> Value {
            let s = SETTINGS.get::<$type_name>();
            s.$field_name.into()
        }

        SETTINGS.set_setting_handlers($vim_setting_name, update_func, reader_func);
    }};
}

type UpdateHandlerFunc = fn(Value);
type ReaderFunc = fn()->Value;

struct SettingsObject {
    object: Box<dyn Any + Send + Sync>,
}

pub struct Settings {
    pub neovim_arguments: Vec<String>,
    settings: RwLock<HashMap<TypeId, SettingsObject>>,
    listeners: RwLock<HashMap<String, UpdateHandlerFunc>>,
    readers: RwLock<HashMap<String, ReaderFunc>>,
}

impl Settings {

    fn new() -> Settings {

        let neovim_arguments = std::env::args().filter(|arg| {
            if arg == "--log" {
                Logger::with_str("neovide")
                    .log_to_file()
                    .rotate(Criterion::Size(10_000_000), Naming::Timestamps, Cleanup::KeepLogFiles(1))
                    .start()
                    .expect("Could not start logger");
                false
            } else {
                true
            }
        }).collect::<Vec<String>>();

        Settings{
            neovim_arguments,
            settings: RwLock::new(HashMap::new()),
            listeners: RwLock::new(HashMap::new()),
            readers: RwLock::new(HashMap::new()),
        }
    }

    pub fn set_setting_handlers(&self, property_name: &str, update_func: UpdateHandlerFunc, reader_func: ReaderFunc) {
        self.listeners.write().insert(String::from(property_name), update_func);
        self.readers.write().insert(String::from(property_name), reader_func);
    }
    
    pub fn set<T: Clone + Send + Sync + 'static >(&self, t: &T) {
        let type_id : TypeId = TypeId::of::<T>();
        let t : T = (*t).clone();
        self.settings.write().insert(type_id, SettingsObject{ object: Box::new(t)});
    }

    pub fn get<'a, T: Clone + Send + Sync + 'static>(&'a self) -> T {
        let read_lock = self.settings.read();
        let boxed = &read_lock.get(&TypeId::of::<T>()).expect("Trying to retrieve a settings object that doesn't exist");
        let value: &T = boxed.object.downcast_ref::<T>().expect("Attempted to extract a settings object of the wrong type");
        (*value).clone()
    }

    pub async fn read_initial_values(&self, nvim: &Neovim<Compat<ChildStdin>>) {
        let keys : Vec<String> = self.listeners.read().keys().cloned().collect();
        for name in keys {
            let variable_name = format!("neovide_{}", name.to_string());
            match nvim.get_var(&variable_name).await {
                Ok(value) => {
                    self.listeners.read().get(&name).unwrap()(value);
                },
                Err(error) => {
                    warn!("Initial value load failed for {}: {}", name, error);
                    let setting = self.readers.read().get(&name).unwrap()();
                    nvim.set_var(&variable_name, setting).await.ok();
                }
            }
        }
    }

    pub async fn setup_changed_listeners(&self, nvim: &Neovim<Compat<ChildStdin>>) {
        let keys : Vec<String> = self.listeners.read().keys().cloned().collect();
        for name in keys {
            let vimscript = format!(
                concat!(
                "exe \"",
                "fun! NeovideNotify{0}Changed(d, k, z)\n",
                "call rpcnotify(1, 'setting_changed', '{0}', g:neovide_{0})\n",
                "endf\n",
                "call dictwatcheradd(g:, 'neovide_{0}', 'NeovideNotify{0}Changed')\"",
                )
            , name);
            nvim.command(&vimscript).await
                .unwrap_or_explained_panic(&format!("Could not setup setting notifier for {}", name));
        }
    }

    pub fn handle_changed_notification(&self, arguments: Vec<Value>) {
        let mut arguments = arguments.into_iter();
        let (name, value) = (arguments.next().unwrap(), arguments.next().unwrap());
           
        let name: Result<String, _>= name.try_into();
        let name = name.unwrap();

        self.listeners.read().get(&name).unwrap()(value);
    }
}
