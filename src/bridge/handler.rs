use async_trait::async_trait;
use log::trace;
use nvim_rs::{compat::tokio::Compat, Handler, Neovim};
use rmpv::Value;
use tokio::process::ChildStdin;
use tokio::task;

use std::ffi::{CString};
use std::ptr::{null, null_mut};
use winapi::shared::minwindef::{HKEY, MAX_PATH, DWORD};
use winapi::um::{
    winnt::{
        REG_SZ, REG_OPTION_NON_VOLATILE, KEY_WRITE,
    },
    winreg::{
        RegCreateKeyExA, RegSetValueExA, RegCloseKey, RegDeleteTreeA,
        HKEY_CLASSES_ROOT
    },
    libloaderapi::{
        GetModuleFileNameA
    }
};

use super::events::handle_redraw_event_group;
use crate::settings::SETTINGS;

// TODO(nganhkhoa): Move to another module
#[cfg(windows)]
unsafe fn unregister_rightclick() {
    let str_registry_path_1 = CString::new("Directory\\Background\\shell\\Neovide").unwrap();
    let str_registry_path_2 = CString::new("*\\shell\\Neovide").unwrap();
    RegDeleteTreeA(
        HKEY_CLASSES_ROOT, str_registry_path_1.as_ptr()
    );
    RegDeleteTreeA(
        HKEY_CLASSES_ROOT, str_registry_path_2.as_ptr()
    );
}

#[cfg(windows)]
unsafe fn register_rightclick_directory(neovide_path: &str) {
    let mut registry_key: HKEY = null_mut();
    let str_registry_path = CString::new("Directory\\Background\\shell\\Neovide").unwrap();
    let str_icon = CString::new("Icon").unwrap();
    let str_description= CString::new("Open with Neovide").unwrap();
    let str_neovide_path = CString::new(neovide_path.as_bytes()).unwrap();
    RegCreateKeyExA(
        HKEY_CLASSES_ROOT, str_registry_path.as_ptr(),
        0, null_mut(),
        REG_OPTION_NON_VOLATILE, KEY_WRITE,
        null_mut(), &mut registry_key, null_mut()
    );
    let registry_values = [
        (null(), REG_SZ,
            str_description.as_ptr() as *const u8, str_description.to_bytes().len() + 1),
        (str_icon.as_ptr(), REG_SZ,
            str_neovide_path.as_ptr() as *const u8, str_neovide_path.to_bytes().len() + 1),
    ];
    for &(key, keytype, value_ptr, size_in_bytes) in &registry_values {
        RegSetValueExA(
            registry_key, key, 0,
            keytype, value_ptr, size_in_bytes as u32
        );
    }
    RegCloseKey(registry_key);

    let str_registry_command_path = CString::new("Directory\\Background\\shell\\Neovide\\command").unwrap();
    let str_command = CString::new(format!("{} \"%V\"", neovide_path).as_bytes()).unwrap();
    RegCreateKeyExA(
        HKEY_CLASSES_ROOT, str_registry_command_path.as_ptr(),
        0, null_mut(),
        REG_OPTION_NON_VOLATILE, KEY_WRITE,
        null_mut(), &mut registry_key, null_mut()
    );
    let registry_values = [
        (null(), REG_SZ,
            str_command.as_ptr() as *const u8, str_command.to_bytes().len() + 1)
    ];
    for &(key, keytype, value_ptr, size_in_bytes) in &registry_values {
        RegSetValueExA(
            registry_key, key, 0,
            keytype, value_ptr, size_in_bytes as u32
        );
    }
    RegCloseKey(registry_key);
}

#[cfg(windows)]
unsafe fn register_rightclick_file(neovide_path: &str) {
    let mut registry_key: HKEY = null_mut();
    let str_registry_path = CString::new("*\\shell\\Neovide").unwrap();
    let str_icon = CString::new("Icon").unwrap();
    let str_description= CString::new("Open with Neovide").unwrap();
    let str_neovide_path = CString::new(neovide_path.as_bytes()).unwrap();
    RegCreateKeyExA(
        HKEY_CLASSES_ROOT, str_registry_path.as_ptr(),
        0, null_mut(),
        REG_OPTION_NON_VOLATILE, KEY_WRITE,
        null_mut(), &mut registry_key, null_mut()
    );
    let registry_values = [
        (null(), REG_SZ,
            str_description.as_ptr() as *const u8, str_description.to_bytes().len() + 1),
        (str_icon.as_ptr(), REG_SZ,
            str_neovide_path.as_ptr() as *const u8, str_neovide_path.to_bytes().len() + 1),
    ];
    for &(key, keytype, value_ptr, size_in_bytes) in &registry_values {
        RegSetValueExA(
            registry_key, key, 0,
            keytype, value_ptr, size_in_bytes as u32
        );
    }
    RegCloseKey(registry_key);

    let str_registry_command_path = CString::new("*\\shell\\Neovide\\command").unwrap();
    let str_command = CString::new(format!("{} \"%1\"", neovide_path).as_bytes()).unwrap();
    RegCreateKeyExA(
        HKEY_CLASSES_ROOT, str_registry_command_path.as_ptr(),
        0, null_mut(),
        REG_OPTION_NON_VOLATILE, KEY_WRITE,
        null_mut(), &mut registry_key, null_mut()
    );
    let registry_values = [
        (null(), REG_SZ,
            str_command.as_ptr() as *const u8, str_command.to_bytes().len() + 1)
    ];
    for &(key, keytype, value_ptr, size_in_bytes) in &registry_values {
        RegSetValueExA(
            registry_key, key, 0,
            keytype, value_ptr, size_in_bytes as u32
        );
    }
    RegCloseKey(registry_key);
}

#[derive(Clone)]
pub struct NeovimHandler();

#[async_trait]
impl Handler for NeovimHandler {
    type Writer = Compat<ChildStdin>;

    async fn handle_notify(
        &self,
        event_name: String,
        arguments: Vec<Value>,
        _neovim: Neovim<Compat<ChildStdin>>,
    ) {
        trace!("Neovim notification: {:?}", &event_name);
        task::spawn_blocking(move || match event_name.as_ref() {
            "redraw" => {
                handle_redraw_event_group(arguments);
            }
            "setting_changed" => {
                SETTINGS.handle_changed_notification(arguments);
            }
            "neovide.reg_right_click" => {
                if cfg!(windows) {
                    let neovide_path = unsafe {
                        let mut buffer = vec![0u8; MAX_PATH];
                        GetModuleFileNameA(null_mut(), buffer.as_mut_ptr() as *mut i8, MAX_PATH as DWORD);
                        CString::from_vec_unchecked(buffer)
                        .into_string().unwrap_or("".to_string())
                        .trim_end_matches(char::from(0)).to_string()
                    };
                    unsafe {
                        unregister_rightclick();
                        register_rightclick_directory(&neovide_path);
                        register_rightclick_file(&neovide_path);
                    }
                }
            }
            "neovide.unreg_right_click" => {
                if cfg!(windows) {
                    unsafe {
                        unregister_rightclick();
                    }
                }
            }
            _ => {}
        })
        .await
        .ok();
    }
}
