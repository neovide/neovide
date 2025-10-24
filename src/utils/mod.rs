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

/// Convert a Vector of Windows path strings to a Vector of WSL paths if `wsl` is true.
///
/// If conversion of a path fails, the path is passed to neovim unchanged.
#[cfg(target_os = "windows")]
pub fn handle_wslpaths(paths: Vec<String>, wsl: bool) -> Vec<String> {
    if !wsl {
        return paths;
    }

    paths
        .into_iter()
        .map(|path| {
            let path = std::fs::canonicalize(&path).map_or(path, |p| p.to_string_lossy().into());
            windows_to_wsl(&path).unwrap_or(path)
        })
        .collect()
}
