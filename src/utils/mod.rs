mod ring_buffer;
#[cfg(test)]
mod test;

pub use ring_buffer::*;
#[cfg(test)]
pub use test::*;

#[cfg(not(target_os = "windows"))]
pub fn is_tty() -> bool {
    use std::io::IsTerminal;
    std::io::stdout().is_terminal()
}
