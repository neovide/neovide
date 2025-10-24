use tokio::process::Command as TokioCommand;

use crate::{cmd_line::CmdLineSettings, settings::*};

// Creates a shell command if needed on this platform
pub fn create_platform_command(
    command: &str,
    args: &Vec<String>,
    settings: &Settings,
) -> TokioCommand {
    let mut result = if settings.get::<CmdLineSettings>().wsl {
        let mut result = TokioCommand::new("wsl");
        result.args(["$SHELL", "-l", "-c"]);
        let args =
            shlex::try_join(args.iter().map(|s| s.as_ref())).expect("Failed to join arguments");
        result.arg(format!("{command} {args}"));
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
