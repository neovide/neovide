use wslpath_rs::windows_to_wsl;

/// Convert a Vector of Windows path strings to a Vector of WSL paths if `wsl` is true.
///
/// If conversion of a path fails, the path is passed to neovim unchanged.
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
