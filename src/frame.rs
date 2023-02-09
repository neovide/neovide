use core::fmt;

use clap::{builder::PossibleValue, ValueEnum};

// Options for the frame decorations
#[derive(Debug, Clone, Copy, Eq, PartialEq, Default)]
pub enum Frame {
    #[default]
    Full,
    #[cfg(target_os = "macos")]
    Transparent,
    #[cfg(target_os = "macos")]
    Buttonless,
    None,
}

impl From<&'_ Frame> for &'static str {
    fn from(frame: &'_ Frame) -> Self {
        match frame {
            Frame::Full => "full",

            #[cfg(target_os = "macos")]
            Frame::Transparent => "transparent",
            #[cfg(target_os = "macos")]
            Frame::Buttonless => "buttonless",

            Frame::None => "none",
        }
    }
}

impl ValueEnum for Frame {
    fn value_variants<'a>() -> &'a [Self] {
        #[cfg(target_os = "macos")]
        return &[Self::Full, Self::Transparent, Self::Buttonless, Self::None];
        #[cfg(not(target_os = "macos"))]
        return &[Self::Full, Self::None];
    }

    fn to_possible_value(&self) -> Option<PossibleValue> {
        Some(PossibleValue::new(<&str>::from(self)))
    }
}

impl fmt::Display for Frame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", <&str>::from(self))
    }
}
