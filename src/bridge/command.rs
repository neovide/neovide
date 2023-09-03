use std::{
    env, eprintln,
    process::{Command as StdCommand, Stdio},
};

use log::debug;
use tokio::process::Command as TokioCommand;

use crate::{cmd_line::CmdLineSettings, settings::*};

pub fn create_nvim_command() -> TokioCommand {
    let mut cmd = build_nvim_cmd();

    debug!("Starting neovim with: {:?}", cmd);

    #[cfg(not(debug_assertions))]
    cmd.stderr(Stdio::piped());

    #[cfg(debug_assertions)]
    cmd.stderr(Stdio::inherit());

    #[cfg(windows)]
    set_windows_creation_flags(&mut cmd);

    cmd
}

#[cfg(target_os = "windows")]
fn set_windows_creation_flags(cmd: &mut TokioCommand) {
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
}

fn build_nvim_cmd() -> TokioCommand {
    if let Some(path) = SETTINGS.get::<CmdLineSettings>().neovim_bin {
        // if neovim_bin contains a path separator, then try to launch it directly
        // otherwise use which to find the fully path
        if path.contains('/') || path.contains('\\') {
            if neovim_ok(&path) {
                return build_nvim_cmd_with_args(&path);
            }
        } else if let Some(path) = platform_which(&path) {
            if neovim_ok(&path) {
                return build_nvim_cmd_with_args(&path);
            }
        }

        eprintln!("ERROR: NEOVIM_BIN='{}' was not found.", path);
        std::process::exit(1);
    } else if let Some(path) = platform_which("nvim") {
        if neovim_ok(&path) {
            return build_nvim_cmd_with_args(&path);
        }
    }
    eprintln!("ERROR: nvim not found!");
    std::process::exit(1);
}

// Creates a shell command if needed on this platform (wsl or macos)
fn create_platform_shell_command(command: &str, args: &[&str]) -> StdCommand {
    if cfg!(target_os = "windows") && SETTINGS.get::<CmdLineSettings>().wsl {
        let mut result = StdCommand::new("wsl");
        result.args(["$SHELL", "-lc"]);
        result.arg(format!("{} {}", command, args.join(" ")));
        #[cfg(windows)]
        std::os::windows::process::CommandExt::creation_flags(
            &mut result,
            winapi::um::winbase::CREATE_NO_WINDOW,
        );

        result
    } else if cfg!(target_os = "macos") {
        let shell = env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
        let mut result = StdCommand::new(shell);

        if env::var_os("TERM").is_none() {
            result.arg("-l");
        }
        result.arg("-c");
        result.arg(format!("{} {}", command, args.join(" ")));

        result
    } else {
        // On Linux, just run the command directly
        let mut result = StdCommand::new(command);
        result.args(args);
        result
    }
}

fn neovim_ok(bin: &str) -> bool {
    let is_wsl = SETTINGS.get::<CmdLineSettings>().wsl;

    let mut cmd = create_platform_shell_command(bin, &["-v"]);
    if let Ok(output) = cmd.output() {
        if output.status.success() {
            // The output is not utf8 on Windows and can contain special characters.
            // But a lossy conversion is OK for our purposes
            let stdout = String::from_utf8_lossy(&output.stdout);
            if !(stdout.starts_with("NVIM v") && output.stderr.is_empty()) {
                let error_message_prefix = format!(
                    concat!(
                        "ERROR: Unexpected output from neovim binary:\n",
                        "\t{bin} -v\n",
                        "Check that your shell doesn't output anything extra when running:",
                        "\n\t"
                    ),
                    bin = bin
                );

                if is_wsl {
                    eprintln!("{error_message_prefix}wsl '$SHELL' -lc '{bin} -v'");
                } else {
                    eprintln!("{error_message_prefix}$SHELL -lc '{bin} -v'");
                }
                std::process::exit(1);
            }
            return true;
        }
    }
    false
}

fn platform_which(bin: &str) -> Option<String> {
    let is_wsl = SETTINGS.get::<CmdLineSettings>().wsl;

    // The which crate won't work in WSL, a shell always needs to be started
    // In all other cases always try which::which first to avoid shell specific problems
    if !is_wsl {
        if let Ok(path) = which::which(bin) {
            return path.into_os_string().into_string().ok();
        }
    }

    // But if that does not work, try the shell anyway
    let mut which_command = create_platform_shell_command("which", &[bin]);
    debug!("Running which command: {:?}", which_command);
    if let Ok(output) = which_command.output() {
        if output.status.success() {
            // The output is not utf8 on Windows and can contain special characters.
            // This might fail with special characters in the path, but that probably does
            // not matter, since which::which should handle almost all cases.
            let nvim_path = String::from_utf8_lossy(&output.stdout);
            return Some(nvim_path.trim().to_owned());
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn nvim_cmd_impl(bin: &str, args: &[String]) -> TokioCommand {
    let shell = env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
    let mut cmd = TokioCommand::new(shell);
    let args_str = args
        .iter()
        .map(|arg| shlex::quote(arg))
        .collect::<Vec<_>>()
        .join(" ");
    if env::var_os("TERM").is_none() {
        cmd.arg("-l");
    }
    cmd.arg("-c");
    cmd.arg(&format!("{} {}", bin, args_str));
    cmd
}

#[cfg(not(target_os = "macos"))]
fn nvim_cmd_impl(bin: &str, args: &[String]) -> TokioCommand {
    if cfg!(target_os = "windows") && SETTINGS.get::<CmdLineSettings>().wsl {
        let mut cmd = TokioCommand::new("wsl");
        cmd.args(["$SHELL", "-lc", &format!("{} {}", bin, args.join(" "))]);
        cmd
    } else {
        let mut cmd = TokioCommand::new(bin);
        cmd.args(args);
        cmd
    }
}

fn build_nvim_cmd_with_args(bin: &str) -> TokioCommand {
    let mut args = vec!["--embed".to_string()];
    args.extend(SETTINGS.get::<CmdLineSettings>().neovim_args);
    nvim_cmd_impl(bin, &args)
}
