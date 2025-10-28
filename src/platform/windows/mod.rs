pub mod bridge;
pub mod profiling;
pub mod renderer;
pub mod settings;
pub mod utils;
pub mod vsync;
pub mod window;

use windows::Win32::{
    System::Console::{AttachConsole, ATTACH_PARENT_PROCESS},
    UI::HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2},
};
use windows_registry::{Result, CURRENT_USER};

use crate::error_msg;

const REGISTRY_PATH_DIRECTORY: &str = "Software\\Classes\\Directory\\Background\\shell\\Neovide";
const REGISTRY_PATH_DIRECTORY_COMMAND: &str =
    "Software\\Classes\\Directory\\Background\\shell\\Neovide\\command";
const REGISTRY_PATH_FOLDER: &str = "Software\\Classes\\*\\shell\\Neovide";
const REGISTRY_PATH_FOLDER_COMMAND: &str = "Software\\Classes\\*\\shell\\Neovide\\command";

fn get_neovide_path() -> String {
    std::env::current_exe()
        .unwrap_or_default()
        .to_str()
        .unwrap_or_default()
        .to_string()
}

fn unregister_rightclick() -> Result<()> {
    let key = CURRENT_USER;
    key.remove_tree(REGISTRY_PATH_DIRECTORY)?;
    key.remove_tree(REGISTRY_PATH_FOLDER)?;

    Ok(())
}

fn register_rightclick_directory() -> Result<()> {
    let neovide_path = get_neovide_path();
    let neovide_description = "Open with Neovide";
    let neovide_command = format!("{neovide_path} \"%V\"");

    let key = CURRENT_USER.create(REGISTRY_PATH_DIRECTORY)?;
    key.set_string("", neovide_description)?;
    key.set_string("Icon", &neovide_path)?;

    let key = CURRENT_USER.create(REGISTRY_PATH_DIRECTORY_COMMAND)?;
    key.set_string("", &neovide_command)?;

    Ok(())
}

fn register_rightclick_file() -> Result<()> {
    let neovide_path = get_neovide_path();
    let neovide_description = "Open with Neovide";
    let neovide_command = format!("{neovide_path} \"%1\"");

    let key = CURRENT_USER.create(REGISTRY_PATH_FOLDER)?;
    key.set_string("", neovide_description)?;
    key.set_string("Icon", &neovide_path)?;

    let key = CURRENT_USER.create(REGISTRY_PATH_FOLDER_COMMAND)?;
    key.set_string("", &neovide_command)?;

    Ok(())
}

pub fn register_right_click() {
    if register_rightclick_directory().is_err() {
        error_msg!("Could not register directory context menu item. Possibly already registered.");
    }
    if register_rightclick_file().is_err() {
        error_msg!("Could not register file context menu item. Possibly already registered.");
    }
}

pub fn unregister_right_click() {
    if unregister_rightclick().is_err() {
        error_msg!("Could not remove context menu items. Possibly already removed.");
    }
}

pub fn windows_fix_dpi() {
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)
            .expect("Failed to set DPI awareness!");
    }
}

pub fn windows_attach_to_console() {
    // Attach to parent console tip found here: https://github.com/rust-lang/rust/issues/67159#issuecomment-987882771
    unsafe {
        AttachConsole(ATTACH_PARENT_PROCESS).ok();
    }
}
