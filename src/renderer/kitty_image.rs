use itertools::Itertools;
use serde::de::{Error, Unexpected};
use serde::{Deserialize, Deserializer};

use super::ImageFragment;

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

    // These are not directly documented for display, but actually needed
    #[serde(default, rename = "i")]
    pub id: u32,
    #[serde(default, rename = "p")]
    pub placement_id: u32,
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

pub fn parse_kitty_image_placeholder(
    text: &str,
    start_column: u32,
    color: u32,
    underline_color: u32,
    fragments: &mut Vec<ImageFragment>,
) -> bool {
    if !text.starts_with(IMAGE_PLACEHOLDER) {
        return false;
    }

    if text.len() % 3 != 0 {
        log::warn!("Invalid Kitty placeholder {text}");
    }
    let image_id = color.swap_bytes() >> 8;
    let placement_id = underline_color.swap_bytes() >> 8;

    fragments.extend(
        text.chars()
            .tuples()
            .enumerate()
            .flat_map(|(index, (placeholder, row, column))| {
                if placeholder != IMAGE_PLACEHOLDER {
                    log::warn!("Invalid Kitty placeholder {text}");
                    None
                } else {
                    let col = get_row_or_col(column);
                    let row = get_row_or_col(row);
                    Some((index, col, row))
                }
            })
            // Group consecutive columns together
            .chunk_by(|(index, col, row)| (*col as isize - *index as isize, *row))
            .into_iter()
            .map(|(_, chunk)| {
                let mut chunk_iter = chunk.into_iter();
                let (index, col, row) = chunk_iter.next().unwrap();
                let len = chunk_iter.count() + 1;
                ImageFragment {
                    dst_col: index as u32 + start_column,
                    src_row: row,
                    src_range: col..col + len as u32,
                    image_id,
                    placement_id,
                }
            }),
    );

    true
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
