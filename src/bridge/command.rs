#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::process::Command as StdCommand;
use tokio::process::Command as TokioCommand;

use crate::{bridge::RestartDetails, cmd_line::CmdLineSettings, settings::*};

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

pub fn create_nvim_command(settings: &Settings) -> TokioCommand {
    let cmdline_settings = settings.get::<CmdLineSettings>();
    create_tokio_nvim_command(&cmdline_settings, true)
}

pub fn create_restart_nvim_command(details: &RestartDetails) -> TokioCommand {
    let mut cmd = TokioCommand::new(&details.progpath);
    cmd.arg("--embed");
    for arg in details.argv.iter().skip(1) {
        cmd.arg(arg);
    }

    #[cfg(target_os = "windows")]
    cmd.creation_flags(windows::Win32::System::Threading::CREATE_NO_WINDOW.0);

    cmd
}

pub fn create_blocking_nvim_command(cmdline_settings: &CmdLineSettings, embed: bool) -> StdCommand {
    let (bin, args) = build_nvim_command_parts(cmdline_settings, embed);
    let spec = create_command_spec(&bin, &args, cmdline_settings);
    let mut cmd = std_command_from_spec(spec);
    if let Some(dir) = &cmdline_settings.chdir {
        cmd.current_dir(dir);
    }
    cmd
}

fn create_tokio_nvim_command(cmdline_settings: &CmdLineSettings, embed: bool) -> TokioCommand {
    let (bin, args) = build_nvim_command_parts(cmdline_settings, embed);
    let spec = create_command_spec(&bin, &args, cmdline_settings);
    let mut cmd = tokio_command_from_spec(spec);
    if let Some(dir) = &cmdline_settings.chdir {
        cmd.current_dir(dir);
    }
    cmd
}

fn build_nvim_command_parts(
    cmdline_settings: &CmdLineSettings,
    embed: bool,
) -> (String, Vec<String>) {
    let bin = cmdline_settings
        .neovim_bin
        .clone()
        .unwrap_or_else(|| "nvim".to_owned());
    let mut args = Vec::new();
    if embed {
        args.push("--embed".to_string());
    }
    args.extend(cmdline_settings.neovim_args.clone());
    (bin, args)
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
        // -f: Bypasses authentication for the already-logged-in user.
        // -p: Preserves the environment.
        // -q: Forces quiet logins, as if a .hushlogin is present.

        // Convert to a single string and add quotes
        let args =
            shlex::try_join(args.iter().map(|s| s.as_ref())).expect("Failed to join arguments");
        CommandSpec::new(
            "/usr/bin/login",
            vec![
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
