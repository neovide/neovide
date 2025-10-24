pub mod clipboard;
pub mod bridge;
use std::env;

pub fn main() {
    // This variable is set by the AppImage runtime and causes problems for child processes
    env::remove_var("ARGV0");
}