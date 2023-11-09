use std::num::ParseFloatError;

use itertools::Itertools;

const DEFAULT_FONT_SIZE: f32 = 14.0;
const FONT_OPTS_SEPARATOR: char = ':';
const FONT_LIST_SEPARATOR: char = ',';
const FONT_HINTING_PREFIX: &str = "#h-";
const FONT_EDGING_PREFIX: &str = "#e-";
const FONT_HEIGHT_PREFIX: char = 'h';
const ALLOW_FLOAT_SIZE_OPT: char = '.';
const FONT_WIDTH_PREFIX: char = 'w';
const FONT_BOLD_OPT: &str = "b";
const FONT_ITALIC_OPT: &str = "i";

const INVALID_SIZE_ERR: &str = "Invalid size";
const INVALID_WIDTH_ERR: &str = "Invalid width";

#[derive(Clone, Debug)]
pub struct FontOptions {
    pub font_list: Vec<String>,
    pub size: f32,
    pub width: f32,
    pub bold: bool,
    pub italic: bool,
    pub allow_float_size: bool,
    pub hinting: FontHinting,
    pub edging: FontEdging,
}

impl FontOptions {
    pub fn parse(guifont_setting: &str) -> Result<FontOptions, &str> {
        let mut font_options = FontOptions::default();

        let mut parts = guifont_setting
            .split(FONT_OPTS_SEPARATOR)
            .filter(|part| !part.is_empty());

        if let Some(parts) = parts.next() {
            let parsed_font_list = parts
                .split(FONT_LIST_SEPARATOR)
                .filter(|fallback| !fallback.is_empty())
                .map(parse_font_name)
                .collect_vec();

            if !parsed_font_list.is_empty() {
                font_options.font_list = parsed_font_list;
            }
        }

        for part in parts {
            if let Some(hinting_string) = part.strip_prefix(FONT_HINTING_PREFIX) {
                font_options.hinting = FontHinting::parse(hinting_string)?;
            } else if let Some(edging_string) = part.strip_prefix(FONT_EDGING_PREFIX) {
                font_options.edging = FontEdging::parse(edging_string)?;
            } else if part.starts_with(FONT_HEIGHT_PREFIX) && part.len() > 1 {
                font_options.allow_float_size |= part[1..].contains(ALLOW_FLOAT_SIZE_OPT);
                font_options.size = parse_pixels(part).map_err(|_| INVALID_SIZE_ERR)?;
            } else if part.starts_with(FONT_WIDTH_PREFIX) && part.len() > 1 {
                font_options.allow_float_size |= part[1..].contains(ALLOW_FLOAT_SIZE_OPT);
                font_options.width = parse_pixels(part).map_err(|_| INVALID_WIDTH_ERR)?;
            } else if part == FONT_BOLD_OPT {
                font_options.bold = true;
            } else if part == FONT_ITALIC_OPT {
                font_options.italic = true;
            }
        }

        Ok(font_options)
    }

    pub fn primary_font(&self) -> Option<String> {
        self.font_list.first().cloned()
    }
}

impl Default for FontOptions {
    fn default() -> Self {
        FontOptions {
            font_list: Vec::new(),
            bold: false,
            italic: false,
            allow_float_size: false,
            size: points_to_pixels(DEFAULT_FONT_SIZE),
            width: 0.0,
            hinting: FontHinting::default(),
            edging: FontEdging::default(),
        }
    }
}

impl PartialEq for FontOptions {
    fn eq(&self, other: &Self) -> bool {
        self.font_list == other.font_list
            && (self.size - other.size).abs() < std::f32::EPSILON
            && self.bold == other.bold
            && self.italic == other.italic
            && self.edging == other.edging
            && self.hinting == other.hinting
    }
}

fn parse_pixels(part: &str) -> Result<f32, ParseFloatError> {
    Ok(points_to_pixels(part[1..].parse::<f32>()?))
}

fn parse_font_name(font_name: impl AsRef<str>) -> String {
    let parsed_font_name = font_name
        .as_ref()
        .chars()
        .batching(|iter| {
            let ch = iter.next();
            match ch? {
                '\\' => iter.next(),
                '_' => Some(' '),
                _ => ch,
            }
        })
        .collect();

    parsed_font_name
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, Default)]
pub enum FontEdging {
    #[default]
    AntiAlias,
    SubpixelAntiAlias,
    Alias,
}

impl FontEdging {
    const INVALID_ERR: &'static str = "Invalid edging";
    pub fn parse(value: &str) -> Result<Self, &str> {
        match value {
            "antialias" => Ok(Self::AntiAlias),
            "subpixelantialias" => Ok(Self::SubpixelAntiAlias),
            "alias" => Ok(Self::Alias),
            _ => Err(Self::INVALID_ERR),
        }
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, Default)]
pub enum FontHinting {
    #[default]
    Full,
    Normal,
    Slight,
    None,
}

impl FontHinting {
    const INVALID_ERR: &'static str = "Invalid hinting";
    pub fn parse(value: &str) -> Result<Self, &str> {
        match value {
            "full" => Ok(Self::Full),
            "normal" => Ok(Self::Normal),
            "slight" => Ok(Self::Slight),
            "none" => Ok(Self::None),
            _ => Err(Self::INVALID_ERR),
        }
    }
}

fn points_to_pixels(value: f32) -> f32 {
    // Fonts in neovim are using points, not pixels.
    //
    // Skia docs is incorrectly stating it uses points, but uses pixels:
    // https://api.skia.org/classSkFont.html#a7e28a156a517d01bc608c14c761346bf
    // https://github.com/mono/SkiaSharp/issues/1147#issuecomment-587421201
    //
    // So, we need to convert points to pixels.
    //
    // In reality, this depends on DPI/PPI of monitor, but here we only care about converting
    // from points to pixels, so this is standard constant values.
    if cfg!(target_os = "macos") {
        // On macos points == pixels
        value
    } else {
        let pixels_per_inch = 96.0;
        let points_per_inch = 72.0;
        value * (pixels_per_inch / points_per_inch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_one_font_from_guifont_setting() {
        let guifont_setting = "Fira Code Mono";
        let font_options = FontOptions::parse(guifont_setting).unwrap();

        assert_eq!(
            font_options.font_list.len(),
            1,
            "font list length should equal {}, but {}",
            1,
            font_options.font_list.len(),
        );
    }

    #[test]
    fn test_parse_many_fonts_from_guifont_setting() {
        let guifont_setting = "Fira Code Mono,Console";
        let font_options = FontOptions::parse(guifont_setting).unwrap();

        assert_eq!(
            font_options.font_list.len(),
            2,
            "font list length should equal {}, but {}",
            2,
            font_options.font_list.len(),
        );
    }

    #[test]
    fn test_parse_edging_from_guifont_setting() {
        let guifont_setting = "Fira Code Mono:#e-subpixelantialias";
        let font_options = FontOptions::parse(guifont_setting).unwrap();

        assert_eq!(
            font_options.edging,
            FontEdging::SubpixelAntiAlias,
            "font edging should equal {:?}, but {:?}",
            FontEdging::SubpixelAntiAlias,
            font_options.edging,
        );
    }

    #[test]
    fn test_parse_invalid_edging_from_guifont_setting() {
        let guifont_setting = "Fira Code Mono:#e-aliens";
        let err = FontOptions::parse(guifont_setting).unwrap_err();

        assert_eq!(
            err,
            FontEdging::INVALID_ERR,
            "parse error should equal {:?}, but {:?}",
            FontEdging::INVALID_ERR,
            err,
        );
    }

    #[test]
    fn test_parse_hinting_from_guifont_setting() {
        let guifont_setting = "Fira Code Mono:#h-slight";
        let font_options = FontOptions::parse(guifont_setting).unwrap();

        assert_eq!(
            font_options.hinting,
            FontHinting::Slight,
            "font hinting should equal {:?}, but {:?}",
            FontHinting::Slight,
            font_options.hinting,
        );
    }

    #[test]
    fn test_parse_invalid_hinting_from_guifont_setting() {
        let guifont_setting = "Fira Code Mono:#h-fool";
        let err = FontOptions::parse(guifont_setting).unwrap_err();

        assert_eq!(
            err,
            FontHinting::INVALID_ERR,
            "parse error should equal {:?}, but {:?}",
            FontHinting::INVALID_ERR,
            err,
        );
    }

    #[test]
    fn test_parse_font_size_float_from_guifont_setting() {
        let guifont_setting = "Fira Code Mono:h15.5";
        let font_options = FontOptions::parse(guifont_setting).unwrap();

        let font_size_pixels = points_to_pixels(15.5);
        assert_eq!(
            font_options.size, font_size_pixels,
            "font size should equal {}, but {}",
            font_size_pixels, font_options.size,
        );
    }

    #[test]
    fn test_parse_invalid_font_size_float_from_guifont_setting() {
        let guifont_setting = "Fira Code Mono:h15.a";
        let err = FontOptions::parse(guifont_setting).unwrap_err();

        assert_eq!(
            err, INVALID_SIZE_ERR,
            "parse err should equal {}, but {}",
            INVALID_SIZE_ERR, err,
        );
    }

    #[test]
    fn test_parse_invalid_font_width_float_from_guifont_setting() {
        let guifont_setting = "Fira Code Mono:w1.b";
        let err = FontOptions::parse(guifont_setting).unwrap_err();

        assert_eq!(
            err, INVALID_WIDTH_ERR,
            "parse err should equal {}, but {}",
            INVALID_WIDTH_ERR, err,
        );
    }

    #[test]
    #[allow(clippy::bool_assert_comparison)]
    fn test_parse_all_params_together_from_guifont_setting() {
        let guifont_setting = "Fira Code Mono:h15.5:b:i:#h-slight:#e-alias";
        let font_options = FontOptions::parse(guifont_setting).unwrap();

        let font_size_pixels = points_to_pixels(15.5);
        assert_eq!(
            font_options.size, font_size_pixels,
            "font size should equal {}, but {}",
            font_size_pixels, font_options.size,
        );

        assert_eq!(
            font_options.allow_float_size, true,
            "allow float size should equal {}, but {}",
            true, font_options.allow_float_size,
        );

        assert_eq!(
            font_options.bold, true,
            "bold should equal {}, but {}",
            true, font_options.bold,
        );

        assert_eq!(
            font_options.italic, true,
            "italic should equal {}, but {}",
            true, font_options.italic,
        );

        assert_eq!(
            font_options.edging,
            FontEdging::Alias,
            "font hinting should equal {:?}, but {:?}",
            FontEdging::Alias,
            font_options.edging,
        );

        assert_eq!(
            font_options.hinting,
            FontHinting::Slight,
            "font hinting should equal {:?}, but {:?}",
            FontHinting::Slight,
            font_options.hinting,
        );
    }

    #[test]
    fn test_parse_font_name_with_escapes() {
        let without_escapes_or_specials_chars = parse_font_name("Fira Code Mono");
        let without_escapes = parse_font_name("Fira_Code_Mono");
        let with_escapes = parse_font_name(r"Fira\_Code\_Mono");
        let with_too_many_escapes = parse_font_name(r"Fira\\_Code\\_Mono");
        let ignored_escape_at_the_end = parse_font_name(r"Fira_Code_Mono\");

        assert_eq!(
            without_escapes_or_specials_chars, "Fira Code Mono",
            "font name should equal {}, but {}",
            without_escapes_or_specials_chars, "Fira Code Mono"
        );

        assert_eq!(
            without_escapes, "Fira Code Mono",
            "font name should equal {}, but {}",
            without_escapes, "Fira Code Mono"
        );

        assert_eq!(
            with_escapes, "Fira_Code_Mono",
            "font name should equal {}, but {}",
            with_escapes, "Fira_Code_Mono"
        );

        assert_eq!(
            with_too_many_escapes, "Fira\\ Code\\ Mono",
            "font name should equal {}, but {}",
            with_too_many_escapes, "Fira\\ Code\\ Mono"
        );

        assert_eq!(
            ignored_escape_at_the_end, "Fira Code Mono",
            "font name should equal {}, but {}",
            ignored_escape_at_the_end, "Fira Code Mono"
        )
    }
}
