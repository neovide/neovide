use serde::de::{Error, Unexpected};
use serde::{Deserialize, Deserializer};

pub const IMAGE_PLACEHOLDER: char = '\u{10EEEE}';
include!(concat!(env!("OUT_DIR"), "/kitty_rowcolumn_diacritics.rs"));

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(tag = "a")]
pub enum KittyImage {
    #[serde(rename = "t")]
    Transmit(Transmit),
    #[serde(rename = "p")]
    Display(Display),
    #[serde(rename = "d")]
    Delete,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Transmit {
    #[serde(default, rename = "f", deserialize_with = "image_format_from_int")]
    pub format: ImageFormat,
    #[serde(default, rename = "t")]
    pub transmission_medium: TransmissionMedium,
    #[serde(default, rename = "s")]
    pub width: u32,
    #[serde(default, rename = "v")]
    pub height: u32,
    #[serde(default, rename = "S")]
    pub file_data_size: u32,
    #[serde(default, rename = "O")]
    pub file_data_offset: u32,
    #[serde(default, rename = "i")]
    pub id: u32,
    #[serde(default, rename = "I")]
    pub image_number: u32,
    #[serde(default, rename = "p")]
    pub placement_id: u32,
    #[serde(default, rename = "o")]
    pub compression: Compression,
    #[serde(default, rename = "m", deserialize_with = "bool_from_int")]
    pub more_chunks: bool,
    #[serde(default)]
    pub data: String,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Display {
    #[serde(default, rename = "x")]
    pub left: u32,
    #[serde(default, rename = "y")]
    pub top: u32,
    #[serde(default, rename = "w")]
    pub width: u32,
    #[serde(default, rename = "h")]
    pub height: u32,
    #[serde(default, rename = "X")]
    pub x_offset: u32,
    #[serde(default, rename = "Y")]
    pub y_offset: u32,
    #[serde(default, rename = "c")]
    pub columns: u32,
    #[serde(default, rename = "r")]
    pub rows: u32,
    #[serde(default, rename = "C", deserialize_with = "bool_from_int")]
    pub dont_move_cursor: bool,
    #[serde(default, rename = "U", deserialize_with = "bool_from_int")]
    pub virtual_placement: bool,
    #[serde(default, rename = "z")]
    pub zindex: i32,
    #[serde(default, rename = "P")]
    pub parent: u32,
    #[serde(default, rename = "Q")]
    pub parent_placement: u32,
    #[serde(default, rename = "H")]
    pub x_placement_offset: i32,
    #[serde(default, rename = "V")]
    pub y_placement_offset: i32,
}

#[derive(Clone, PartialEq, Debug, Default)]
#[repr(u8)]
pub enum ImageFormat {
    Rgb = 24,
    #[default]
    Rgba = 32,
    Png = 100,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Default)]
pub enum Compression {
    #[default]
    None,
    #[serde(rename = "z")]
    ZLib,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Default)]
pub enum TransmissionMedium {
    #[default]
    #[serde(rename = "d")]
    Direct,
    #[serde(rename = "f")]
    File,
    #[serde(rename = "t")]
    TemporaryFile,
    #[serde(rename = "s")]
    SharedMemory,
}

fn bool_from_int<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    match u8::deserialize(deserializer)? {
        0 => Ok(false),
        1 => Ok(true),
        other => Err(Error::invalid_value(
            Unexpected::Unsigned(other as u64),
            &"zero or one",
        )),
    }
}

fn image_format_from_int<'de, D>(deserializer: D) -> Result<ImageFormat, D::Error>
where
    D: Deserializer<'de>,
{
    match u8::deserialize(deserializer)? {
        24 => Ok(ImageFormat::Rgb),
        32 => Ok(ImageFormat::Rgba),
        100 => Ok(ImageFormat::Png),
        other => Err(Error::invalid_value(
            Unexpected::Unsigned(other as u64),
            &"Unknown image format",
        )),
    }
}
