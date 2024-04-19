use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::System::LibraryLoader::GetModuleFileNameA;
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExA, RegDeleteTreeA, RegSetValueExA, HKEY, HKEY_CURRENT_USER,
    KEY_READ, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ,
};
use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};

use crate::error_msg;

const REGISTRY_PATH_DIRECTORY: PCSTR =
    s!("Software\\Classes\\Directory\\Background\\shell\\Neovide");
const REGISTRY_PATH_DIRECTORY_COMMAND: PCSTR =
    s!("Software\\Classes\\Directory\\Background\\shell\\Neovide\\command");
const REGISTRY_PATH_FOLDER: PCSTR = s!("Software\\Classes\\*\\shell\\Neovide");
const REGISTRY_PATH_FOLDER_COMMAND: PCSTR = s!("Software\\Classes\\*\\shell\\Neovide\\command");

fn get_neovide_path() -> String {
    let mut buffer = vec![0u8; MAX_PATH as usize];
    let len: u32;
    unsafe {
        len = GetModuleFileNameA(HMODULE::default(), &mut buffer);
    }
    buffer.truncate(len as usize);
    String::from_utf8(buffer).unwrap()
}

fn unregister_rightclick() -> bool {
    unsafe {
        RegDeleteTreeA(HKEY_CURRENT_USER, REGISTRY_PATH_DIRECTORY).0 == 0
            && RegDeleteTreeA(HKEY_CURRENT_USER, REGISTRY_PATH_FOLDER).0 == 0
    }
}

fn register_rightclick_directory() -> bool {
    let mut registry_key = HKEY::default();

    let neovide_path = get_neovide_path();
    let neovide_description = "Open with Neovide";
    let neovide_command = format!("{} \"%V\"", neovide_path);

    unsafe {
        if RegCreateKeyExA(
            HKEY_CURRENT_USER,
            REGISTRY_PATH_DIRECTORY,
            0,
            PCSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_READ | KEY_WRITE,
            None,
            &mut registry_key,
            None,
        )
        .0 != 0
        {
            let _ = RegCloseKey(registry_key);
            return false;
        }

        let _ = RegSetValueExA(
            registry_key,
            PCSTR::null(),
            0,
            REG_SZ,
            Some(neovide_description.as_bytes()),
        );
        let _ = RegSetValueExA(
            registry_key,
            s!("Icon"),
            0,
            REG_SZ,
            Some(neovide_path.as_bytes()),
        );
        let _ = RegCloseKey(registry_key);

        if RegCreateKeyExA(
            HKEY_CURRENT_USER,
            REGISTRY_PATH_DIRECTORY_COMMAND,
            0,
            PCSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_READ | KEY_WRITE,
            None,
            &mut registry_key,
            None,
        )
        .0 != 0
        {
            return false;
        }

        let _ = RegSetValueExA(
            registry_key,
            PCSTR::null(),
            0,
            REG_SZ,
            Some(neovide_command.as_bytes()),
        );
        let _ = RegCloseKey(registry_key);
    }

    true
}

fn register_rightclick_file() -> bool {
    let mut registry_key = HKEY::default();

    let neovide_path = get_neovide_path();
    let neovide_description = "Open with Neovide";
    let neovide_command = format!("{} \"%1\"", neovide_path);

    unsafe {
        if RegCreateKeyExA(
            HKEY_CURRENT_USER,
            REGISTRY_PATH_FOLDER,
            0,
            PCSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_READ | KEY_WRITE,
            None,
            &mut registry_key,
            None,
        )
        .0 != 0
        {
            let _ = RegCloseKey(registry_key);
            return false;
        }

        let _ = RegSetValueExA(
            registry_key,
            PCSTR::null(),
            0,
            REG_SZ,
            Some(neovide_description.as_bytes()),
        );
        let _ = RegSetValueExA(
            registry_key,
            s!("Icon"),
            0,
            REG_SZ,
            Some(get_neovide_path().as_bytes()),
        );
        let _ = RegCloseKey(registry_key);

        if RegCreateKeyExA(
            HKEY_CURRENT_USER,
            REGISTRY_PATH_FOLDER_COMMAND,
            0,
            PCSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_READ | KEY_WRITE,
            None,
            &mut registry_key,
            None,
        )
        .0 != 0
        {
            return false;
        }

        let _ = RegSetValueExA(
            registry_key,
            PCSTR::null(),
            0,
            REG_SZ,
            Some(neovide_command.as_bytes()),
        );
        let _ = RegCloseKey(registry_key);
    }

    true
}

pub fn register_right_click() {
    if unregister_rightclick() {
        error_msg!("Could not unregister previous menu item. Possibly already registered.");
    }
    if !register_rightclick_directory() {
        error_msg!("Could not register directory context menu item. Possibly already registered.");
    }
    if !register_rightclick_file() {
        error_msg!("Could not register file context menu item. Possibly already registered.");
    }
}

pub fn unregister_right_click() {
    if !unregister_rightclick() {
        error_msg!("Could not remove context menu items. Possibly already removed.");
    }
}

pub fn windows_fix_dpi() {
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2).unwrap();
    }
}
