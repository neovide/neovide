#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command as StdCommand;
use tokio::process::Command as TokioCommand;

use crate::{cmd_line::CmdLineSettings, utils::handle_wslpaths};

#[cfg(target_os = "macos")]
const FORKED_FROM_TTY_ENV_VAR: &str = "NEOVIDE_FORKED_FROM_TTY";

/// For route-local startup targets for an embedded nvim instance.
///
/// we keep these separate from process-global cmd line settings because a reused
/// neovide process can open additional windows with their own launch context.
/// that context needs to be part of the actual nvim argv so e.g :restart replays
/// the same buffers/tabs, instead of only preserving the initial empty startup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenArgs {
    pub files_to_open: Vec<String>,
    pub tabs: bool,
}

/// Mode is how neovide should populate argv when launching an embedded nvim instance.
///
/// Startup uses the original process command line targets,
/// Args uses route-local targets such as macOS handoff new windows,
/// None launches a blank embedded instance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpenMode {
    None,
    Startup,
    Args(OpenArgs),
}

#[derive(Clone)]
struct CommandSpec {
    program: String,
    args: Vec<String>,
    #[cfg(target_os = "windows")]
    creation_flags: Option<u32>,
}

impl CommandSpec {
    fn new(program: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            program: program.into(),
            args,
            #[cfg(target_os = "windows")]
            creation_flags: None,
        }
    }

    #[cfg(target_os = "windows")]
    fn with_creation_flags(mut self, flags: u32) -> Self {
        self.creation_flags = Some(flags);
        self
    }
}

pub fn create_blocking_nvim_command(cmdline_settings: &CmdLineSettings, embed: bool) -> StdCommand {
    let (bin, args) = build_nvim_command_parts(cmdline_settings, embed, OpenMode::Startup);
    let spec = create_command_spec(&bin, &args, cmdline_settings);
    let mut cmd = std_command_from_spec(spec);
    if let Some(dir) = command_cwd(cmdline_settings, None) {
        cmd.current_dir(dir);
    }
    cmd
}

pub fn create_tokio_nvim_command(
    cmdline_settings: &CmdLineSettings,
    embed: bool,
    cwd: Option<&Path>,
    mode: OpenMode,
) -> TokioCommand {
    let (bin, args) = build_nvim_command_parts(cmdline_settings, embed, mode);
    let spec = create_command_spec(&bin, &args, cmdline_settings);
    let mut cmd = tokio_command_from_spec(spec);
    if let Some(dir) = command_cwd(cmdline_settings, cwd) {
        cmd.current_dir(dir);
    }
    cmd
}

fn command_cwd(settings: &CmdLineSettings, cwd: Option<&Path>) -> Option<PathBuf> {
    cwd.map(Path::to_path_buf).or_else(|| settings.chdir.as_deref().map(PathBuf::from))
}

fn build_nvim_command_parts(
    cmdline_settings: &CmdLineSettings,
    embed: bool,
    mode: OpenMode,
) -> (String, Vec<String>) {
    let bin = cmdline_settings.neovim_bin.clone().unwrap_or_else(|| "nvim".to_owned());
    let mut args = cmdline_settings.neovim_args.clone();
    if embed {
        append_embed_arg(&mut args);
    }

    args.extend(build_open_args(cmdline_settings, mode));

    (bin, args)
}

fn build_open_args(cmdline_settings: &CmdLineSettings, open_mode: OpenMode) -> Vec<String> {
    let (files_to_open, tabs) = match open_mode {
        OpenMode::None => return Vec::new(),
        OpenMode::Startup => (cmdline_settings.files_to_open.clone(), cmdline_settings.tabs),
        OpenMode::Args(args) => (args.files_to_open, args.tabs),
    };

    tabs.then(|| "-p".to_string())
        .into_iter()
        .chain(handle_wslpaths(files_to_open, cmdline_settings.wsl))
        .collect()
}

fn append_embed_arg(args: &mut Vec<String>) {
    if !args.iter().any(|arg| arg == "--embed") {
        args.push("--embed".to_string());
    }
}

fn tokio_command_from_spec(spec: CommandSpec) -> TokioCommand {
    let CommandSpec {
        program,
        args,
        #[cfg(target_os = "windows")]
        creation_flags,
    } = spec;
    let mut result = TokioCommand::new(program);
    result.args(&args);
    #[cfg(target_os = "windows")]
    if let Some(flags) = creation_flags {
        result.creation_flags(flags);
    }
    result
}

fn std_command_from_spec(spec: CommandSpec) -> StdCommand {
    let CommandSpec {
        program,
        args,
        #[cfg(target_os = "windows")]
        creation_flags,
    } = spec;
    let mut result = StdCommand::new(program);
    result.args(&args);
    #[cfg(target_os = "windows")]
    if let Some(flags) = creation_flags {
        result.creation_flags(flags);
    }
    result
}

#[cfg(target_os = "macos")]
fn launched_from_desktop() -> bool {
    if std::env::var_os(FORKED_FROM_TTY_ENV_VAR).is_some() {
        return false;
    }

    // On macOS, apps launched from Finder or `open` are spawned by launchd = PPID 1.
    // This is more reliable than $TERM for detecting GUI vs terminal launches,
    // so we use this as a heuristic instead of relying on $TERM.
    // https://en.wikipedia.org/wiki/Launchd#Components
    use rustix::process;
    matches!(process::getppid(), Some(ppid) if ppid.is_init())
}

// Creates a shell command if needed on this platform.
#[cfg(target_os = "macos")]
fn create_command_spec(
    command: &str,
    args: &[String],
    _cmdline_settings: &CmdLineSettings,
) -> CommandSpec {
    use uzers::os::unix::UserExt;
    if !launched_from_desktop() {
        // If we're not launched from the desktop, assume a terminal launch and avoid
        // re-initializing the environment. See https://github.com/neovide/neovide/issues/2584
        CommandSpec::new(command, args.to_vec())
    } else {
        // Otherwise run inside a login shell to ensure the environment matches a
        // normal GUI launch. See https://github.com/neovide/neovide/issues/2584
        let user = uzers::get_user_by_uid(uzers::get_current_uid()).unwrap();
        let shell = user.shell();

        // Convert to a single string and add quotes
        let args =
            shlex::try_join(args.iter().map(|s| s.as_ref())).expect("Failed to join arguments");
        CommandSpec::new(
            "/usr/bin/login",
            vec![
                // -f: Bypasses authentication for the already-logged-in user.
                // -p: Preserves the environment.
                // -q: Forces quiet logins, as if a .hushlogin is present.
                "-fpq".to_string(),
                user.name().to_str().unwrap().to_string(),
                shell.to_str().unwrap().to_string(),
                "-c".to_string(),
                format!("{command} {args}"),
            ],
        )
    }
}

// Creates a shell command if needed on this platform.
#[cfg(target_os = "windows")]
fn create_command_spec(
    command: &str,
    args: &[String],
    cmdline_settings: &CmdLineSettings,
) -> CommandSpec {
    let spec = if cfg!(target_os = "windows") && cmdline_settings.wsl {
        let args =
            shlex::try_join(args.iter().map(|s| s.as_ref())).expect("Failed to join arguments");
        CommandSpec::new(
            "wsl",
            vec![
                "$SHELL".to_string(),
                "-l".to_string(),
                "-c".to_string(),
                format!("{command} {args}"),
            ],
        )
    } else {
        // There's no need to go through the shell on Windows when not using WSL
        CommandSpec::new(command, args.to_vec())
    };
    spec.with_creation_flags(windows::Win32::System::Threading::CREATE_NO_WINDOW.0)
}

// Creates a shell command if needed on this platform.
// On Linux we can just launch directly
#[cfg(target_os = "linux")]
fn create_command_spec(
    command: &str,
    args: &[String],
    _cmdline_settings: &CmdLineSettings,
) -> CommandSpec {
    CommandSpec::new(command, args.to_vec())
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use crate::{cmd_line::handle_command_line_arguments, settings::Settings};

    use super::*;

    fn parse_cmdline_settings(args: &[&str]) -> CmdLineSettings {
        let settings = Settings::new();
        let args = args.iter().map(|arg| arg.to_string()).collect();
        handle_command_line_arguments(args, &settings).expect("Could not parse arguments");
        settings.get::<CmdLineSettings>()
    }

    #[test]
    fn build_nvim_command_parts_places_embed_before_auto_open_args() {
        let cmdline_settings =
            parse_cmdline_settings(&["neovide", "./foo.txt", "./bar.md", "--grid=420x240"]);

        let (_, args) = build_nvim_command_parts(&cmdline_settings, true, OpenMode::Startup);

        assert_eq!(args, vec!["--embed", "-p", "./foo.txt", "./bar.md"]);
    }

    #[test]
    fn build_nvim_command_parts_skips_auto_open_args_when_requested() {
        let cmdline_settings =
            parse_cmdline_settings(&["neovide", "./foo.txt", "./bar.md", "--grid=420x240"]);

        let (_, args) = build_nvim_command_parts(&cmdline_settings, true, OpenMode::None);

        assert_eq!(args, vec!["--embed"]);
    }

    #[test]
    fn build_nvim_command_parts_uses_route_auto_open_args() {
        let cmdline_settings =
            parse_cmdline_settings(&["neovide", "./foo.txt", "./bar.md", "--grid=420x240"]);
        let open_args = OpenArgs { files_to_open: vec!["/tmp/project".to_string()], tabs: false };

        let (_, args) =
            build_nvim_command_parts(&cmdline_settings, true, OpenMode::Args(open_args));

        assert_eq!(args, vec!["--embed", "/tmp/project"]);
    }

    #[test]
    fn build_nvim_command_parts_preserves_launcher_args_before_embed() {
        let cmdline_settings = parse_cmdline_settings(&[
            "neovide",
            "--no-tabs",
            "--neovim-bin",
            "ssh",
            "--",
            "my-server",
            "nvim",
        ]);

        let (bin, args) = build_nvim_command_parts(&cmdline_settings, true, OpenMode::Startup);

        assert_eq!(bin, "ssh");
        assert_eq!(args, vec!["my-server", "nvim", "--embed"]);
    }

    #[test]
    fn command_cwd_prefers_override() {
        let cmdline_settings = parse_cmdline_settings(&["neovide", "--chdir", "/random/path"]);

        assert_eq!(
            command_cwd(&cmdline_settings, Some(Path::new("/route/cwd"))),
            Some(PathBuf::from("/route/cwd"))
        );
    }

    #[test]
    fn command_cwd_falls_back_to_cmdline_setting() {
        let cmdline_settings = parse_cmdline_settings(&["neovide", "--chdir", "/random/path"]);

        assert_eq!(command_cwd(&cmdline_settings, None), Some(PathBuf::from("/random/path")));
    }
}
