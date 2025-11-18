mod ring_buffer;
#[cfg(test)]
mod test;

#[cfg(target_os = "windows")]
use wslpath_rs::windows_to_wsl;

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

/// Expands a leading tilde to the current user home directory.
pub fn expand_tilde(path: &str) -> String {
    let Some(remainder) = path.strip_prefix('~') else {
        return path.to_owned();
    };

    let Some(mut home) = dirs::home_dir() else {
        return path.to_owned();
    };

    if remainder.is_empty() {
        return home.to_string_lossy().into();
    }

    // only support the current user home. anything else should be left untouched.
    let trimmed = remainder.trim_start_matches(['/', '\\']);
    if trimmed.len() == remainder.len() {
        return path.to_owned();
    }

    if !trimmed.is_empty() {
        home.push(trimmed);
    }

    home.to_string_lossy().into()
}

#[cfg(test)]
mod tilde_tests {
    use super::expand_tilde;

    #[cfg(unix)]
    #[test]
    fn expands_unix_style_paths() {
        let expanded = expand_tilde("~/config");
        let expected = match dirs::home_dir() {
            Some(mut home) => {
                home.push("config");
                home.to_string_lossy().into_owned()
            }
            None => "~/config".into(),
        };
        assert_eq!(expanded, expected);
    }

    #[cfg(windows)]
    #[test]
    fn expands_windows_style_paths() {
        let expanded = expand_tilde("~\\AppData");
        let expected = match dirs::home_dir() {
            Some(mut home) => {
                home.push("AppData");
                home.to_string_lossy().into_owned()
            }
            None => "~\\AppData".into(),
        };
        assert_eq!(expanded, expected);
    }

    #[test]
    fn ignores_non_tilde_paths() {
        assert_eq!(expand_tilde("/tmp/icons"), "/tmp/icons");
    }

    #[test]
    fn ignores_tilde_user_paths() {
        assert_eq!(expand_tilde("~other"), "~other");
    }

    #[test]
    fn handles_repeated_separators() {
        let expanded = expand_tilde("~//icons");
        let expected = match dirs::home_dir() {
            Some(mut home) => {
                home.push("icons");
                home.to_string_lossy().into_owned()
            }
            None => "~//icons".into(),
        };
        assert_eq!(expanded, expected);
    }
}
