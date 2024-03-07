#[cfg(windows)]
use std::os::windows::process::CommandExt;

use std::{
    env,
    process::{Command as StdCommand, Stdio},
};

use anyhow::{bail, Result};
use log::debug;
use regex::Regex;
use tokio::process::Command as TokioCommand;

use crate::{cmd_line::CmdLineSettings, error_handling::ResultPanicExplanation, settings::*};

pub fn create_nvim_command() -> Result<TokioCommand> {
    let mut cmd = build_nvim_cmd()?;

    debug!("Starting neovim with: {:?}", cmd);

    #[cfg(not(debug_assertions))]
    cmd.stderr(Stdio::piped());

    #[cfg(debug_assertions)]
    cmd.stderr(Stdio::inherit());

    #[cfg(windows)]
    cmd.creation_flags(winapi::um::winbase::CREATE_NO_WINDOW);

    Ok(cmd)
}

fn build_nvim_cmd() -> Result<TokioCommand> {
    if let Some(cmdline) = SETTINGS.get::<CmdLineSettings>().neovim_bin {
        if let Some((bin, args)) = lex_nvim_cmdline(&cmdline)? {
            return Ok(build_nvim_cmd_with_args(bin, args));
        }

        bail!("ERROR: NEOVIM_BIN='{}' was not found.", cmdline);
    }

    if let Some(path) = platform_which("nvim") {
        if neovim_ok(&path, &[])? {
            return Ok(build_nvim_cmd_with_args(path, vec![]));
        }
    }

    bail!("ERROR: nvim not found!")
}

// Creates a shell command if needed on this platform (wsl or macOS)
fn create_platform_shell_command(command: &str, args: &[&str]) -> StdCommand {
    if cfg!(target_os = "windows") && SETTINGS.get::<CmdLineSettings>().wsl {
        let mut result = StdCommand::new("wsl");
        result.args(["$SHELL", "-lc"]);
        result.arg(format!("{} {}", command, args.join(" ")));

        #[cfg(windows)]
        result.creation_flags(winapi::um::winbase::CREATE_NO_WINDOW);

        result
    } else if cfg!(target_os = "macos") {
        let user =
            env::var("USER").unwrap_or_explained_panic("USER environment variable not found");
        let shell = env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
        let shell_name = shell.rsplit('/').next().unwrap();

        // Exec the shell with argv[0] prepended by '-' so it becomes a login shell.
        // `login` normally does this itself, but `-l` disables this.
        let exec = format!("exec -a -{} {} {}", shell_name, command, args.join(" "));

        // See "man login". It sets up some important env vars like $PATH and $HOME.
        // On macOS, use the `login` command so the it will appear as a tty session.
        let mut result = StdCommand::new("/usr/bin/login");

        // We use a special flag to tell login not to prompt us for a password, because we're
        // going to spawn it as the current user anyway. The addition of "p",
        // preserves the environment.
        // -f: Bypasses authentication for the already-logged-in user.
        // -l: Skips changing directory to $HOME and prepending '-' to argv[0].
        // -p: Preserves the environment.
        // -q: Forces quiet logins, as if a .hushlogin is present.
        result.args(["-flpq", &user, &shell, "-lc", &exec]);

        result
    } else {
        // On Linux and non-WSL Windows, just run the command directly
        let mut result = StdCommand::new(command);
        result.args(args);

        #[cfg(windows)]
        result.creation_flags(winapi::um::winbase::CREATE_NO_WINDOW);

        result
    }
}

fn create_error_message(bin: &str, stdout: &str, stderr: Vec<&str>, is_wsl: bool) -> String {
    let mut error_message = format!(
        concat!(
            "ERROR: Unexpected output from neovim binary:\n",
            "\t{bin} -v\n",
            "stdout: {stdout}\n",
            "stderr: {stderr}\n",
            "\t"
        ),
        bin = bin,
        stdout = stdout,
        stderr = stderr.join("\n")
    );

    if is_wsl {
        error_message.push_str("\n\nPlease check your WSL configuration.\n");
    } else {
        error_message.push_str("\n\nPlease check your shell configuration.\n");
    }

    error_message
}

fn neovim_ok(bin: &str, args: &[String]) -> Result<bool> {
    let is_wsl = SETTINGS.get::<CmdLineSettings>().wsl;
    let mut args = args.iter().map(String::as_str).collect::<Vec<_>>();
    args.push("-v");

    let mut cmd = create_platform_shell_command(bin, &args);
    let output = cmd.output();

    if output.is_err() {
        return Ok(false);
    }

    // The output is not utf8 on Windows and can contain special characters.
    // But a lossy conversion is OK for our purposes
    let output = output?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    let error_regex = Regex::new(r"your \d+x\d+ screen size is bogus. expect trouble").unwrap();
    let (_, non_matching_stderr_lines): (Vec<_>, Vec<_>) =
        stderr.lines().partition(|line| error_regex.is_match(line));

    let unexpeted_output = !output.status.success()
        || stdout.is_empty()
        || !(stdout.starts_with("NVIM v") && stderr.is_empty())
        || non_matching_stderr_lines.len() != stderr.lines().count();

    if unexpeted_output {
        let error_message = create_error_message(bin, &stdout, non_matching_stderr_lines, is_wsl);
        let command = if is_wsl {
            "wsl '$SHELL' -lc '{bin} -v'"
        } else {
            "$SHELL -lc '{bin} -v'"
        };

        bail!("{error_message}{command}")
    }

    Ok(true)
}

fn lex_nvim_cmdline(cmdline: &str) -> Result<Option<(String, Vec<String>)>> {
    let is_windows = cfg!(target_os = "windows") && !SETTINGS.get::<CmdLineSettings>().wsl;
    // shlex::split does not work with windows path separators, so pass the cmdline as it is
    // Note that on WSL we can still try to split it to support specifying neovim-bin as
    // /usr/bin/env nvim for example
    if is_windows {
        Some((cmdline.to_owned(), Vec::new()))
    } else {
        shlex::split(cmdline)
            .filter(|t| !t.is_empty())
            .map(|mut tokens| (tokens.remove(0), tokens))
    }
    .and_then(|(bin, args)| {
        // if neovim_bin contains a path separator, then try to launch it directly
        // otherwise use which to find the full path
        if !bin.contains('/') && !bin.contains('\\') {
            platform_which(&bin).map(|bin| (bin, args))
        } else {
            Some((bin, args))
        }
    })
    .map_or(Ok(None), |(bin, args)| {
        neovim_ok(&bin, &args).map(|res| res.then_some((bin, args)))
    })
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
fn nvim_cmd_impl(bin: String, mut args: Vec<String>) -> TokioCommand {
    let user = env::var("USER").unwrap_or_explained_panic("USER environment variable not found");
    let shell = env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    let shell_name = shell.rsplit('/').next().unwrap();

    args.insert(0, bin);
    let args = match shlex::try_join(args.iter().map(String::as_str)) {
        Ok(args) => args,
        Err(_) => panic!("Failed to join arguments"),
    };

    // See "man login". It sets up some important env vars like $PATH and $HOME.
    // On macOS, use the `login` command so the it will appear as a tty session.
    let mut cmd = TokioCommand::new("/usr/bin/login");

    // Exec the shell with argv[0] prepended by '-' so it becomes a login shell.
    // `login` normally does this itself, but `-l` disables this.
    let exec = format!("exec -a -{} {}", shell_name, args);

    // We use a special flag to tell login not to prompt us for a password, because we're
    // going to spawn it as the current user anyway. The addition of "p",
    // preserves the environment.
    // -f: Bypasses authentication for the already-logged-in user.
    // -l: Skips changing directory to $HOME and prepending '-' to argv[0].
    // -p: Preserves the environment.
    // -q: Forces quiet logins, as if a .hushlogin is present.
    cmd.args(["-flpq", &user, &shell, "-lc", &exec]);

    cmd
}

#[cfg(not(target_os = "macos"))]
fn nvim_cmd_impl(bin: String, mut args: Vec<String>) -> TokioCommand {
    if cfg!(target_os = "windows") && SETTINGS.get::<CmdLineSettings>().wsl {
        args.insert(0, bin);
        let mut cmd = TokioCommand::new("wsl");
        cmd.args(["$SHELL", "-lc", &args.join(" ")]);
        cmd
    } else {
        let mut cmd = TokioCommand::new(bin);
        cmd.args(args);
        cmd
    }
}

fn build_nvim_cmd_with_args(bin: String, mut args: Vec<String>) -> TokioCommand {
    args.push("--embed".to_string());
    args.extend(SETTINGS.get::<CmdLineSettings>().neovim_args);
    nvim_cmd_impl(bin, args)
}
