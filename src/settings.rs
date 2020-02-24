use std::collections::HashMap;
use std::convert::TryInto;
use std::any::{Any, TypeId};

pub use rmpv::Value;
use nvim_rs::Neovim;
use nvim_rs::compat::tokio::Compat;
use flexi_logger::{Logger, Criterion, Naming, Cleanup};
use tokio::process::ChildStdin;
use parking_lot::RwLock;
use log::warn;

use crate::error_handling::ResultPanicExplanation;

lazy_static! {
    pub static ref SETTINGS: Settings = Settings::new();
}

struct SettingsObject {
    object: Box<dyn Any + Send + Sync>,
}

pub struct Settings {
    pub neovim_arguments: Vec<String>,
    settings: RwLock<HashMap<TypeId, SettingsObject>>,
    listeners: RwLock<HashMap<String, fn (&str, Option<Value>)->Value>>,
}

impl Settings {

    fn new() -> Settings {

        let mut no_idle = false;
        let mut buffer_frames = 1;

        let neovim_arguments = std::env::args().filter(|arg| {
            if arg == "--log" {
                Logger::with_str("neovide")
                    .log_to_file()
                    .rotate(Criterion::Size(10_000_000), Naming::Timestamps, Cleanup::KeepLogFiles(1))
                    .start()
                    .expect("Could not start logger");
                false
            } else if arg == "--noIdle" {
                no_idle = true;
                false
            } else if arg == "--extraBufferFrames" {
                buffer_frames = 60;
                false
            } else {
                true
            }
        }).collect::<Vec<String>>();


        Settings{
            neovim_arguments,
            settings: RwLock::new(HashMap::new()),
            listeners: RwLock::new(HashMap::new()),
        }
    }

    pub fn add_listener(&self, property_name: &str, func: fn (&str, Option<Value>)-> Value) {
        self.listeners.write().insert(String::from(property_name), func);
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
                    self.listeners.read().get(&name).unwrap()(&name, Some(value));
                },
                Err(error) => {
                    warn!("Initial value load failed for {}: {}", name, error);
                    let setting = self.listeners.read().get(&name).unwrap()(&name, None);
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

        self.listeners.read().get(&name).unwrap()(&name, Some(value));
    }

    /*
    pub fn new() -> Settings {
        let mut no_idle = false;
        let mut buffer_frames = 1;

        let neovim_arguments = std::env::args().filter(|arg| {
            if arg == "--log" {
                Logger::with_str("neovide")
                    .log_to_file()
                    .rotate(Criterion::Size(10_000_000), Naming::Timestamps, Cleanup::KeepLogFiles(1))
                    .start()
                    .expect("Could not start logger");
                false
            } else if arg == "--noIdle" {
                no_idle = true;
                false
            } else if arg == "--extraBufferFrames" {
                buffer_frames = 60;
                false
            } else {
                true
            }
        }).collect::<Vec<String>>();

        let mut settings = HashMap::new();

        settings.insert("no_idle".to_string(),  Setting::new_bool(no_idle));
        settings.insert("extra_buffer_frames".to_string(), Setting::new_u16(buffer_frames));
        settings.insert("refresh_rate".to_string(), Setting::new_u16(60));

        Settings { neovim_arguments, settings: Mutex::new(settings) }
    }
    */
}
