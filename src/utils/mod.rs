mod ring_buffer;

pub use ring_buffer::*;

pub fn is_tty() -> bool {
    use std::io::IsTerminal;
    std::io::stdout().is_terminal()
}
