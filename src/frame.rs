use core::fmt;

use clap::{builder::PossibleValue, ValueEnum};

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

impl Frame {
    fn to_static_str(&self) -> &'static str {
        match self {
            Frame::Full => "full",

            #[cfg(target_os = "macos")]
            Frame::Transparent => "transparent",
            #[cfg(target_os = "macos")]
            Frame::Buttonless => "buttonless",

            Frame::None => "none",
        }
    }
}

impl Default for Frame {
    fn default() -> Frame {
        Frame::Full
    }
}

impl ValueEnum for Frame {
    fn value_variants<'a>() -> &'a [Self] {
        #[cfg(target_os = "macos")]
        let variants = { &[Self::Full, Self::Transparent, Self::Buttonless, Self::None] };
        #[cfg(not(target_os = "macos"))]
        let variants = { &[Self::Full, Self::None] };

        variants
    }

    fn to_possible_value(&self) -> Option<PossibleValue> {
        let value = self.to_static_str();

        Some(PossibleValue::new(value))
    }
}

impl fmt::Display for Frame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_static_str())
    }
}
