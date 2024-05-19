mod ring_buffer;

pub use ring_buffer::*;

#[cfg(not(target_os = "windows"))]
pub fn is_tty() -> bool {
    use std::io::IsTerminal;
    std::io::stdout().is_terminal()
}
