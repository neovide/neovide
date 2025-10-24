use tokio::process::Command as TokioCommand;

use crate::settings::*;

// Creates a shell command if needed on this platform
pub fn create_platform_command(
    command: &str,
    args: &Vec<String>,
    _settings: &Settings,
) -> TokioCommand {
    // On Linux we can just launch directly
    let mut result = TokioCommand::new(command);
    result.args(args);
    result
}
