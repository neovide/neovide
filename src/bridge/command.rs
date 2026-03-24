#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command as StdCommand;
use tokio::process::Command as TokioCommand;

use crate::{
    bridge::RestartDetails, cmd_line::CmdLineSettings, settings::*, utils::handle_wslpaths,
};

#[cfg(target_os = "macos")]
const FORKED_FROM_TTY_ENV_VAR: &str = "NEOVIDE_FORKED_FROM_TTY";

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

pub fn create_restart_nvim_command(
    settings: &Settings,
    details: &RestartDetails,
    cwd: Option<&Path>,
) -> TokioCommand {
    let cmdline_settings = settings.get::<CmdLineSettings>();
    let spec = create_restart_command_spec(details, &cmdline_settings);

    #[allow(unused_mut)]
    let mut cmd = tokio_command_from_spec(spec);
    if let Some(dir) = command_cwd(&cmdline_settings, cwd) {
        cmd.current_dir(dir);
    }

    #[cfg(target_os = "windows")]
    cmd.creation_flags(windows::Win32::System::Threading::CREATE_NO_WINDOW.0);

    cmd
}

pub fn create_blocking_nvim_command(cmdline_settings: &CmdLineSettings, embed: bool) -> StdCommand {
    let (bin, args) = build_nvim_command_parts(cmdline_settings, embed);
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
) -> TokioCommand {
    let (bin, args) = build_nvim_command_parts(cmdline_settings, embed);
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
) -> (String, Vec<String>) {
    let bin = cmdline_settings.neovim_bin.clone().unwrap_or_else(|| "nvim".to_owned());
    let mut args = cmdline_settings.neovim_args.clone();
    if embed {
        append_embed_arg(&mut args);
    }

    args.extend(build_auto_open_args(cmdline_settings));

    (bin, args)
}

fn create_restart_command_spec(
    details: &RestartDetails,
    cmdline_settings: &CmdLineSettings,
) -> CommandSpec {
    let (program, args) = build_restart_command_parts(details, cmdline_settings);
    create_command_spec(&program, &args, cmdline_settings)
}

fn build_restart_command_parts(
    details: &RestartDetails,
    cmdline_settings: &CmdLineSettings,
) -> (String, Vec<String>) {
    if should_replay_startup_command(cmdline_settings) {
        return build_nvim_command_parts(cmdline_settings, true);
    }

    build_inner_restart_command_parts(details)
}

fn should_replay_startup_command(cmdline_settings: &CmdLineSettings) -> bool {
    cmdline_settings.server.is_none()
}

fn build_inner_restart_command_parts(details: &RestartDetails) -> (String, Vec<String>) {
    let mut args = details.argv.iter().skip(1).cloned().collect::<Vec<_>>();
    prepend_embed_arg(&mut args);
    (details.progpath.clone(), args)
}

fn build_auto_open_args(cmdline_settings: &CmdLineSettings) -> Vec<String> {
    cmdline_settings
        .tabs
        .then(|| "-p".to_string())
        .into_iter()
        .chain(handle_wslpaths(cmdline_settings.files_to_open.clone(), cmdline_settings.wsl))
        .collect()
}

fn append_embed_arg(args: &mut Vec<String>) {
    if !args.iter().any(|arg| arg == "--embed") {
        args.push("--embed".to_string());
    }
}

fn prepend_embed_arg(args: &mut Vec<String>) {
    if !args.iter().any(|arg| arg == "--embed") {
        args.insert(0, "--embed".to_string());
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

    use crate::cmd_line::handle_command_line_arguments;

    use super::*;

    fn parse_cmdline_settings(args: &[&str]) -> CmdLineSettings {
        let settings = Settings::new();
        let args = args.iter().map(|arg| arg.to_string()).collect();
        handle_command_line_arguments(args, &settings).expect("Could not parse arguments");
        settings.get::<CmdLineSettings>()
    }

    fn parse_settings(args: &[&str]) -> Settings {
        let settings = Settings::new();
        let args = args.iter().map(|arg| arg.to_string()).collect();
        handle_command_line_arguments(args, &settings).expect("Could not parse arguments");
        settings
    }

    #[test]
    fn build_nvim_command_parts_places_embed_before_auto_open_args() {
        let cmdline_settings =
            parse_cmdline_settings(&["neovide", "./foo.txt", "./bar.md", "--grid=420x240"]);

        let (_, args) = build_nvim_command_parts(&cmdline_settings, true);

        assert_eq!(args, vec!["--embed", "-p", "./foo.txt", "./bar.md"]);
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

        let (bin, args) = build_nvim_command_parts(&cmdline_settings, true);

        assert_eq!(bin, "ssh");
        assert_eq!(args, vec!["my-server", "nvim", "--embed"]);
    }

    #[test]
    fn build_restart_command_parts_replays_original_launcher_command() {
        let cmdline_settings = parse_cmdline_settings(&[
            "neovide",
            "--no-tabs",
            "--neovim-bin",
            "ssh",
            "--",
            "my-server",
            "nvim",
        ]);
        let restart_details = RestartDetails {
            progpath: "/usr/bin/nvim".to_string(),
            argv: vec!["nvim".to_string(), "--clean".to_string()],
        };

        let (program, args) = build_restart_command_parts(&restart_details, &cmdline_settings);

        assert_eq!(program, "ssh");
        assert_eq!(args, vec!["my-server", "nvim", "--embed"]);
    }

    #[test]
    fn build_restart_command_parts_replays_original_auto_open_args() {
        let cmdline_settings =
            parse_cmdline_settings(&["neovide", "./foo.txt", "./bar.md", "--grid=420x240"]);
        let restart_details = RestartDetails {
            progpath: "nvim".to_string(),
            argv: vec!["nvim".to_string(), "--clean".to_string()],
        };

        let (_, args) = build_restart_command_parts(&restart_details, &cmdline_settings);

        assert_eq!(args, vec!["--embed", "-p", "./foo.txt", "./bar.md"]);
    }

    #[test]
    fn build_restart_command_parts_keeps_embed_before_restart_args_for_server_mode() {
        let cmdline_settings = parse_cmdline_settings(&["neovide", "--server", "127.0.0.1:7777"]);
        let restart_details = RestartDetails {
            progpath: "nvim".to_string(),
            argv: vec!["nvim".to_string(), "-p".to_string(), "foo.txt".to_string()],
        };

        let (program, args) = build_restart_command_parts(&restart_details, &cmdline_settings);

        assert_eq!(program, "nvim");
        assert_eq!(args, vec!["--embed", "-p", "foo.txt"]);
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

    #[test]
    fn create_restart_nvim_command_prefers_route_cwd() {
        let settings = parse_settings(&["neovide", "--chdir", "/cmdline/cwd"]);
        let restart_details = RestartDetails {
            progpath: "nvim".to_string(),
            argv: vec!["nvim".to_string(), "--clean".to_string()],
        };

        let command =
            create_restart_nvim_command(&settings, &restart_details, Some(Path::new("/route/cwd")));

        assert_eq!(command.as_std().get_current_dir(), Some(Path::new("/route/cwd")));
    }

    #[test]
    fn create_restart_nvim_command_falls_back_to_cmdline_cwd() {
        let settings = parse_settings(&["neovide", "--chdir", "/cmdline/cwd"]);
        let restart_details = RestartDetails {
            progpath: "nvim".to_string(),
            argv: vec!["nvim".to_string(), "--clean".to_string()],
        };

        let command = create_restart_nvim_command(&settings, &restart_details, None);

        assert_eq!(command.as_std().get_current_dir(), Some(Path::new("/cmdline/cwd")));
    }
}
