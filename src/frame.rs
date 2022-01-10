// Options for the frame decorations
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Frame {
    Full,
    #[cfg(target_os = "macos")]
    Transparent,
    #[cfg(target_os = "macos")]
    Buttonless,
    None,
}

impl Default for Frame {
    fn default() -> Frame {
        Frame::Full
    }
}

impl Frame {
    pub fn from_string(decoration: String) -> Frame {
        match decoration.to_lowercase().as_str() {
            "full" => Frame::Full,
            #[cfg(target_os = "macos")]
            "transparent" => Frame::Transparent,
            #[cfg(target_os = "macos")]
            "buttonless" => Frame::Buttonless,
            "none" => Frame::None,
            _ => Frame::Full,
        }
    }
}
