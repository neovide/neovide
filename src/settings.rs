use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::convert::TryInto;

use rmpv::Value;
use nvim_rs::Neovim;
use nvim_rs::compat::tokio::Compat;
use flexi_logger::{Logger, Criterion, Naming, Cleanup};
use tokio::process::ChildStdin;

use crate::error_handling::ResultPanicExplanation;

lazy_static! {
    pub static ref SETTINGS: Settings = Settings::new();
}

pub enum Setting {
    Bool(AtomicBool),
    U16(AtomicU16)
}

impl Setting {
    fn new_bool(value: bool) -> Setting {
        Setting::Bool(AtomicBool::new(value))
    }

    pub fn read_bool(&self) -> bool {
        if let Setting::Bool(atomic_bool) = self {
            atomic_bool.load(Ordering::Relaxed)
        } else {
            panic!("Could not read setting as bool");
        }
    }

    fn new_u16(value: u16) -> Setting {
        Setting::U16(AtomicU16::new(value))
    }

    pub fn read_u16(&self) -> u16 {
        if let Setting::U16(atomic_u16) = self {
            atomic_u16.load(Ordering::Relaxed)
        } else {
            panic!("Could not read setting as u16");
        }
    }

    fn parse(&self, value: Value) {
        match self {
            Setting::Bool(atomic_bool) => {
                if let Ok(value) = value.try_into() {
                    let intermediate: u64 = value;
                    atomic_bool.store(intermediate != 0, Ordering::Relaxed);
                }
            },
            Setting::U16(atomic_u16) => {
                if let Ok(value) = value.try_into() {
                    let intermediate: u64 = value;
                    atomic_u16.store(intermediate as u16, Ordering::Relaxed);
                }
            }
        }
    }

    fn unparse(&self) -> Value {
        match self {
            Setting::Bool(atomic_bool) => {
                let value = if atomic_bool.load(Ordering::Relaxed) {
                    1
                } else {
                    0
                };
                Value::from(value)
            },
            Setting::U16(atomic_u16) => Value::from(atomic_u16.load(Ordering::Relaxed))
        }
    }
}

pub struct Settings {
    pub neovim_arguments: Vec<String>,
    pub settings: HashMap<String, Setting>
}

impl Settings {
    pub async fn read_initial_values(&self, nvim: &Neovim<Compat<ChildStdin>>) {
        for (name, setting) in self.settings.iter() {
            let variable_name = format!("g:neovide_{}", name.to_string());
            if let Ok(value) = nvim.get_var(&variable_name).await {
                setting.parse(value);
            } else {
                nvim.set_var(&variable_name, setting.unparse()).await.ok();
            }
        }
    }

    pub async fn setup_changed_listeners(&self, nvim: &Neovim<Compat<ChildStdin>>) {
        for name in self.settings.keys() {
            let name = name.to_string();
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

        if let Some(setting) = name
                .try_into()
                .ok()
                .as_ref()
                .and_then(|name: &String| self.settings.get(name)) {
            setting.parse(value);
        }
    }

    pub fn get(&self, name: &str) -> &Setting {
        self.settings.get(name).expect(&format!("Could not find option {}", name))
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

        Settings { neovim_arguments, settings }
    }
}
