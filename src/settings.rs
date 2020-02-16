use std::collections::HashMap;
use std::convert::TryInto;

use rmpv::Value;
use nvim_rs::Neovim;
use nvim_rs::compat::tokio::Compat;
use flexi_logger::{Logger, Criterion, Naming, Cleanup};
use tokio::process::ChildStdin;
use parking_lot::Mutex;

use crate::error_handling::ResultPanicExplanation;

lazy_static! {
    pub static ref SETTINGS: Settings = Settings::new();
}

pub enum Setting {
    Bool(bool),
    U16(u16),
    String(String)
}

impl Setting {
    fn new_bool(value: bool) -> Setting {
        Setting::Bool(value)
    }

    pub fn read_bool(&self) -> bool {
        if let Setting::Bool(value) = self {
            *value
        } else {
            panic!("Could not read setting as bool");
        }
    }

    fn new_u16(value: u16) -> Setting {
        Setting::U16(value)
    }

    pub fn read_u16(&self) -> u16 {
        if let Setting::U16(value) = self {
            *value
        } else {
            panic!("Could not read setting as u16");
        }
    }
    
    fn new_string(value: String) -> Setting {
        Setting::String(value)
    }

    pub fn read_string(&self) -> String {
        if let Setting::String(value) = self {
            value.clone()
        } else {
            panic!("Could not read setting as string");
        }
    }

    fn parse(&mut self, value: Value) {
        match self {
            Setting::Bool(internal_bool) => {
                if let Ok(value) = value.try_into() {
                    let intermediate: u64 = value;
                    *internal_bool = intermediate != 0;
                }
            },
            Setting::U16(internal_u16) => {
                if let Ok(value) = value.try_into() {
                    let intermediate: u64 = value;
                    *internal_u16 = intermediate as u16;
                }
            },
            Setting::String(internal_string) => {
                if let Ok(value) = value.try_into() {
                    let intermediate: String = value;
                    *internal_string = intermediate;
                }
            }
        }
    }

    fn unparse(&self) -> Value {
        match self {
            Setting::Bool(internal_bool) => {
                let value = if *internal_bool {
                    1
                } else {
                    0
                };
                Value::from(value)
            },
            Setting::U16(internal_u16) => Value::from(*internal_u16),
            Setting::String(internal_string) => Value::from(internal_string.as_str()),
        }
    }

    fn clone(&self) -> Setting {
        match self {
            Setting::Bool(_) => Setting::new_bool(self.read_bool()),
            Setting::U16(_) => Setting::new_u16(self.read_u16()),
            Setting::String(_) => Setting::new_string(self.read_string()),
        }
    }
}

pub struct Settings {
    pub neovim_arguments: Vec<String>,
    pub settings: Mutex<HashMap<String, Setting>>
}

impl Settings {
    pub async fn read_initial_values(&self, nvim: &Neovim<Compat<ChildStdin>>) {
        let keys : Vec<String>= self.settings.lock().keys().cloned().collect();
        for name in keys {
            let variable_name = format!("g:neovide_{}", name.to_string());
            if let Ok(value) = nvim.get_var(&variable_name).await {
                self.settings.lock().get_mut(&name).unwrap().parse(value);
            } else {
                let setting = self.get(&name);
                nvim.set_var(&variable_name, setting.unparse()).await.ok();
            }
        }
    }

    pub async fn setup_changed_listeners(&self, nvim: &Neovim<Compat<ChildStdin>>) {
        let keys : Vec<String>= self.settings.lock().keys().cloned().collect();
        for name in keys {
            let vimscript = 
                format!("function NeovideNotify{}Changed(d, k, z)\n", name) +
               &format!("  call rpcnotify(1, \"setting_changed\", \"{}\", g:neovide_{})\n", name, name) +
                        "endfunction\n" +
               &format!("call dictwatcheradd(g:, \"neovide_{}\", \"NeovideNotify{}Changed\")", name, name);
            nvim.exec(&vimscript, false).await
                .unwrap_or_explained_panic(&format!("Could not setup setting notifier for {}", name));
        }
    }

    pub fn handle_changed_notification(&self, arguments: Vec<Value>) {
        let mut arguments = arguments.into_iter();
        let (name, value) = (arguments.next().unwrap(), arguments.next().unwrap());
        dbg!(&name, &value);
           
        let name: Result<String, _>= name.try_into();
        let name = name.unwrap();

        self.settings.lock().get_mut(&name).unwrap().parse(value);
    }

    pub fn get(&self, name: &str) -> Setting {
        let settings = self.settings.lock();
        let setting = settings.get(name).expect(&format!("Could not find option {}", name));
        setting.clone()
    }

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

        Settings { neovim_arguments, settings: Mutex::new(settings) }
    }
}
