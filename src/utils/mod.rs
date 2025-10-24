#[cfg(target_os = "windows")]
use crate::platform::windows;
mod ring_buffer;
#[cfg(test)]
mod test;

#[cfg(target_os = "windows")]
pub fn handle_wslpaths(paths: Vec<String>, wsl: bool) -> Vec<String> {
    windows::utils::handle_wslpaths(paths, wsl)
}

pub use ring_buffer::*;

#[cfg(not(target_os = "windows"))]
pub fn is_tty() -> bool {
    use std::io::IsTerminal;
    std::io::stdout().is_terminal()
}

#[cfg(not(target_os = "windows"))]
pub fn handle_wslpaths(paths: Vec<String>, _wsl: bool) -> Vec<String> {
    paths
}
