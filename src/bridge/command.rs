use std::{
    path::Path,
    process::{Command as StdCommand, Stdio},
};

use log::{error, info, warn};
use tokio::process::Command as TokioCommand;

use crate::{cmd_line::CmdLineSettings, settings::*};

pub fn create_nvim_command() -> TokioCommand {
    let mut cmd = build_nvim_cmd();

    info!("Starting neovim with: {:?}", cmd);

    #[cfg(not(debug_assertions))]
    cmd.stderr(Stdio::piped());

    #[cfg(debug_assertions)]
    cmd.stderr(Stdio::inherit());

    #[cfg(windows)]
    set_windows_creation_flags(&mut cmd);

    cmd
}

#[cfg(windows)]
fn set_windows_creation_flags(cmd: &mut TokioCommand) {
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
}

fn build_nvim_cmd() -> TokioCommand {
    if let Some(path) = SETTINGS.get::<CmdLineSettings>().neovim_bin {
        if platform_exists(&path) {
            return build_nvim_cmd_with_args(&path);
        } else {
            warn!("NEOVIM_BIN is invalid falling back to first bin in PATH");
        }
    }
    if let Some(path) = platform_which("nvim") {
        build_nvim_cmd_with_args(&path)
    } else {
        error!("nvim not found!");
        std::process::exit(1);
    }
}

fn platform_exists(bin: &str) -> bool {
    #[cfg(windows)]
    if SETTINGS.get::<CmdLineSettings>().wsl {
        if let Ok(output) = StdCommand::new("wsl")
            .args(&["$SHELL", "-lic"])
            .arg(format!("exists -x {}", bin))
            .output()
        {
            return output.status.success();
        } else {
            error!("wsl exists failed");
            std::process::exit(1);
        }
    }

    #[cfg(target_os = "macos")]
    {
        let shell = env::var("SHELL").unwrap();
        if let Ok(output) = StdCommand::new(shell)
            .args(&["-lic"])
            .arg(format!("exists -x {}", bin))
            .output()
        {
            return output.status.success();
        } else {
            error!("macos exists failed");
            std::process::exit(1);
        }
    }

    Path::new(&bin).exists()
}

fn platform_which(bin: &str) -> Option<String> {
    #[cfg(windows)]
    if SETTINGS.get::<CmdLineSettings>().wsl {
        if let Ok(output) = StdCommand::new("wsl")
            .args(&["$SHELL", "-lic"])
            .arg(format!("which {}", bin))
            .output()
        {
            if output.status.success() {
                return Some(String::from_utf8(output.stdout).unwrap());
            } else {
                return None;
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        let shell = env::var("SHELL").unwrap();
        if let Ok(output) = StdCommand::new(shell)
            .args(&["-lic"])
            .arg(format!("which {}", bin))
            .output()
        {
            if output.status.success() {
                return Some(String::from_utf8(output.stdout).unwrap());
            } else {
                return None;
            }
        }
    }

    if let Ok(path) = which::which(bin) {
        path.into_os_string().into_string().ok()
    } else {
        None
    }
}

fn build_nvim_cmd_with_args(bin: &str) -> TokioCommand {
    let mut args = vec!["--embed".to_string()];
    args.extend(SETTINGS.get::<CmdLineSettings>().neovim_args);

    #[cfg(windows)]
    if SETTINGS.get::<CmdLineSettings>().wsl {
        let mut cmd = TokioCommand::new("wsl");
        cmd.args(&["$SHELL", "-lc", bin.trim()]);
        cmd.args(args);
        return cmd;
    }

    #[cfg(target_os = "macos")]
    {
        let shell = env::var("SHELL").unwrap();
        let mut cmd = TokioCommand::new(shell);
        cmd.args(&["-lc", bin.trim()]);
        cmd.args(args);
        return cmd;
    }

    let mut cmd = TokioCommand::new(bin);
    cmd.args(args);
    cmd
}
