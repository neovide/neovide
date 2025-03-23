use tokio::process::Command as TokioCommand;

use crate::{cmd_line::CmdLineSettings, settings::*};

pub fn create_nvim_command(settings: &Settings) -> TokioCommand {
    let bin = settings
        .get::<CmdLineSettings>()
        .neovim_bin
        .unwrap_or("nvim".to_owned());
    let mut args = Vec::new();
    args.push("--embed".to_string());
    args.extend(settings.get::<CmdLineSettings>().neovim_args);
    create_platform_command(&bin, &args, settings)
}

// Creates a shell command if needed on this platform
#[cfg(target_os = "macos")]
fn create_platform_command(
    command: &str,
    args: &Vec<String>,
    _settings: &Settings,
) -> TokioCommand {
    use std::env;
    use uzers::os::unix::UserExt;
    if env::var_os("TERM").is_some() {
        // If $TERM is set, we assume user is running from a terminal, and we shouldn't
        // re-initialize the environment.
        // See https://github.com/neovide/neovide/issues/2584
        let mut result = TokioCommand::new(command);
        result.args(args);
        result
    } else {
        // Otherwise run inside a login shell
        let user = uzers::get_user_by_uid(uzers::get_current_uid()).unwrap();
        let shell = user.shell();
        // -f: Bypasses authentication for the already-logged-in user.
        // -p: Preserves the environment.
        // -q: Forces quiet logins, as if a .hushlogin is present.

        // Convert to a single string and add quotes
        let args =
            shlex::try_join(args.iter().map(|s| s.as_ref())).expect("Failed to join arguments");
        let mut result = TokioCommand::new("/usr/bin/login");
        result.args([
            "-fpq",
            user.name().to_str().unwrap(),
            shell.to_str().unwrap(),
            "-c",
        ]);
        result.arg(format!("{} {}", command, args));
        result
    }
}

// Creates a shell command if needed on this platform
#[cfg(target_os = "windows")]
fn create_platform_command(command: &str, args: &Vec<String>, settings: &Settings) -> TokioCommand {
    let mut result = if cfg!(target_os = "windows") && settings.get::<CmdLineSettings>().wsl {
        let mut result = TokioCommand::new("wsl");
        result.args(["$SHELL", "-l", "-c"]);
        // There's no need to use shlex on Windows, the WSL path conversion already takes care of
        // quoting
        result.arg(format!("{} {}", command, args.join(" ")));
        result
    } else {
        // There's no need to go through the shell on Windows when not using WSL
        let mut result = TokioCommand::new(command);
        result.args(args);
        result
    };

    result.creation_flags(windows::Win32::System::Threading::CREATE_NO_WINDOW.0);
    result
}

// Creates a shell command if needed on this platform
#[cfg(target_os = "linux")]
fn create_platform_command(
    command: &str,
    args: &Vec<String>,
    _settings: &Settings,
) -> TokioCommand {
    // On Linux we can just launch directly
    let mut result = TokioCommand::new(command);
    result.args(args);
    result
}
