mod ring_buffer;
#[cfg(test)]
mod test;

#[cfg(target_os = "macos")]
use std::path::{Path, PathBuf};

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

#[cfg(target_os = "macos")]
pub fn resolved_cwd(chdir: Option<&str>) -> Option<String> {
    let current_dir = std::env::current_dir().ok();

    let cwd = match chdir {
        Some(dir) if dir.starts_with('~') => PathBuf::from(expand_tilde(dir)),
        Some(dir) if Path::new(dir).is_absolute() => PathBuf::from(dir),
        Some(dir) => current_dir?.join(dir),
        None => current_dir?,
    };

    Some(cwd.to_string_lossy().into_owned())
}

#[cfg(target_os = "macos")]
pub fn resolve_relative_path(path: &str, cwd: Option<&Path>) -> String {
    if path.starts_with('~') {
        return expand_tilde(path);
    }

    if Path::new(path).is_absolute() {
        return path.to_owned();
    }

    let Some(cwd) = cwd else {
        return path.to_owned();
    };

    cwd.join(path).to_string_lossy().into_owned()
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

#[cfg(all(test, target_os = "macos"))]
mod resolves_path_tests {
    use std::path::Path;

    use super::{resolve_relative_path, resolved_cwd};

    #[test]
    fn expands_tilde_chdir_to_home_directory() {
        let resolved = resolved_cwd(Some("~/project")).unwrap();
        let expected = match dirs::home_dir() {
            Some(mut home) => {
                home.push("project");
                home.to_string_lossy().into_owned()
            }
            None => "~/project".into(),
        };

        assert_eq!(resolved, expected);
    }

    #[test]
    fn resolves_relative_chdir_against_current_directory() {
        let resolved = resolved_cwd(Some("project")).unwrap();
        let expected = std::env::current_dir().unwrap().join("project");

        assert_eq!(resolved, expected.to_string_lossy());
    }

    #[test]
    fn expands_tilde_path_to_home_directory() {
        let resolved = resolve_relative_path("~/project", Some(Path::new("/path/to/user")));
        let expected = match dirs::home_dir() {
            Some(mut home) => {
                home.push("project");
                home.to_string_lossy().into_owned()
            }
            None => "~/project".into(),
        };

        assert_eq!(resolved, expected);
    }

    #[test]
    fn preserves_named_user_home_paths() {
        assert_eq!(
            resolve_relative_path("~user/project", Some(Path::new("/path/to/user"))),
            "~user/project"
        );
    }

    #[test]
    fn resolves_plain_relative_path_against_cwd() {
        assert_eq!(
            resolve_relative_path("project", Some(Path::new("/path/to/user"))),
            "/path/to/user/project"
        );
    }
}
