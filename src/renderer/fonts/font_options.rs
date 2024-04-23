use std::{collections::HashMap, fmt, iter, num::ParseFloatError, sync::Arc};

use itertools::Itertools;
use log::warn;
use serde::Deserialize;
use skia_safe::{
    font_style::{Slant, Weight, Width},
    FontStyle,
};

use crate::editor;

const DEFAULT_FONT_SIZE: f32 = 14.0;
const FONT_OPTS_SEPARATOR: char = ':';
const FONT_LIST_SEPARATOR: char = ',';
const FONT_HINTING_PREFIX: &str = "#h-";
const FONT_EDGING_PREFIX: &str = "#e-";
const FONT_HEIGHT_PREFIX: char = 'h';
const FONT_WIDTH_PREFIX: char = 'w';
const FONT_BOLD_OPT: &str = "b";
const FONT_ITALIC_OPT: &str = "i";

const INVALID_SIZE_ERR: &str = "Invalid size";
const INVALID_WIDTH_ERR: &str = "Invalid width";

/// Description of the normal font.
#[derive(Clone, Debug, Deserialize, PartialEq, Hash, Eq, Default)]
pub struct FontDescription {
    pub family: String,
    pub style: Option<String>,
}

/// Description of the italic and bold fonts.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct SecondaryFontDescription {
    pub family: Option<String>,
    pub style: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct FontFeature(pub String, pub u16);

/// What a specific font is about.
// TODO: could be made a bitfield sometime?
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct CoarseStyle {
    bold: bool,
    italic: bool,
}

impl CoarseStyle {
    /// Returns the textual name of this style.
    pub fn name(&self) -> Option<&'static str> {
        let name = match (self.bold, self.italic) {
            (true, true) => "Bold Italic",
            (true, false) => "Bold",
            (false, true) => "Italic",
            (false, false) => return None,
        };
        Some(name)
    }

    /// Iterates through all possible style permutations.
    pub fn permutations() -> impl Iterator<Item = CoarseStyle> {
        iter::repeat([true, false])
            .take(2)
            .multi_cartesian_product()
            .map(|values| CoarseStyle {
                bold: values[0],
                italic: values[1],
            })
    }
}

impl From<CoarseStyle> for FontStyle {
    fn from(CoarseStyle { bold, italic }: CoarseStyle) -> Self {
        match (bold, italic) {
            (true, true) => FontStyle::bold_italic(),
            (true, false) => FontStyle::bold(),
            (false, true) => FontStyle::italic(),
            (false, false) => FontStyle::normal(),
        }
    }
}

impl From<&editor::Style> for CoarseStyle {
    fn from(fine_style: &editor::Style) -> Self {
        Self {
            bold: fine_style.bold,
            italic: fine_style.italic,
        }
    }
}

// essentially just a convenience impl
impl From<&Arc<editor::Style>> for CoarseStyle {
    fn from(fine_style: &Arc<editor::Style>) -> Self {
        Self::from(&**fine_style)
    }
}

#[derive(Clone, Debug)]
pub struct FontOptions {
    pub normal: Vec<FontDescription>,
    pub italic: Option<Vec<SecondaryFontDescription>>,
    pub bold: Option<Vec<SecondaryFontDescription>>,
    pub bold_italic: Option<Vec<SecondaryFontDescription>>,
    pub features: HashMap<String /* family */, Vec<FontFeature> /* features */>,
    pub size: f32,
    pub width: f32,
    pub hinting: FontHinting,
    pub edging: FontEdging,
}

impl FontFeature {
    pub fn parse(feature: &str) -> Result<Self, &str> {
        if let Some(name) = feature.strip_prefix('+') {
            Ok(FontFeature(name.trim().to_string(), 1u16))
        } else if let Some(name) = feature.strip_prefix('-') {
            Ok(FontFeature(name.trim().to_string(), 0u16))
        } else if let Some((name, value)) = feature.split_once('=') {
            let value = value.parse();
            if let Ok(value) = value {
                Ok(FontFeature(name.to_string(), value))
            } else {
                warn!("Wrong feature format: {}", feature);
                Err(feature)
            }
        } else {
            warn!("Wrong feature format: {}", feature);
            Err(feature)
        }
    }
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
                font_options.normal = parsed_font_list
                    .into_iter()
                    .map(|family| FontDescription {
                        family,
                        style: None,
                    })
                    .collect();
            }
        }

        let mut style: Vec<String> = vec![];
        for part in parts {
            if let Some(hinting_string) = part.strip_prefix(FONT_HINTING_PREFIX) {
                font_options.hinting = FontHinting::parse(hinting_string)?;
            } else if let Some(edging_string) = part.strip_prefix(FONT_EDGING_PREFIX) {
                font_options.edging = FontEdging::parse(edging_string)?;
            } else if part.starts_with(FONT_HEIGHT_PREFIX) && part.len() > 1 {
                font_options.size = parse_pixels(part).map_err(|_| INVALID_SIZE_ERR)?;
            } else if part.starts_with(FONT_WIDTH_PREFIX) && part.len() > 1 {
                font_options.width = parse_pixels(part).map_err(|_| INVALID_WIDTH_ERR)?;
            } else if part == FONT_BOLD_OPT {
                style.push("Bold".to_string());
            } else if part == FONT_ITALIC_OPT {
                style.push("Italic".to_string());
            }
        }
        let style = if style.is_empty() {
            None
        } else {
            Some(style.into_iter().unique().sorted().join(" "))
        };
        for font in font_options.normal.iter_mut() {
            font.style.clone_from(&style);
        }

        Ok(font_options)
    }

    pub fn primary_font(&self) -> Option<FontDescription> {
        self.normal.first().cloned()
    }

    pub fn font_list(&self, style: CoarseStyle) -> Vec<FontDescription> {
        let fonts = match (style.bold, style.italic) {
            (true, true) => &self.bold_italic,
            (true, false) => &self.bold,
            (false, true) => &self.italic,
            (false, false) => &None,
        };

        let fonts = fonts
            .as_ref()
            .map(|fonts| {
                fonts
                    .iter()
                    .flat_map(|font| font.fallback(&self.normal))
                    .collect()
            })
            .unwrap_or_else(|| self.normal.clone());

        fonts
            .into_iter()
            .map(|font| FontDescription {
                style: font.style.or_else(|| style.name().map(str::to_string)),
                ..font
            })
            .collect()
    }

    pub fn possible_fonts(&self) -> Vec<FontDescription> {
        CoarseStyle::permutations()
            // partial functions when /s
            .flat_map(|style| self.font_list(style))
            .collect()
    }
}

impl Default for FontOptions {
    fn default() -> Self {
        FontOptions {
            normal: Vec::new(),
            italic: None,
            bold: None,
            bold_italic: None,
            features: HashMap::new(),
            size: points_to_pixels(DEFAULT_FONT_SIZE),
            width: 0.0,
            hinting: FontHinting::default(),
            edging: FontEdging::default(),
        }
    }
}

impl PartialEq for FontOptions {
    fn eq(&self, other: &Self) -> bool {
        self.normal == other.normal
            && self.bold == other.bold
            && self.italic == other.italic
            && self.bold_italic == other.bold_italic
            && self.features == other.features
            && self.edging == other.edging
            && (self.size - other.size).abs() < f32::EPSILON
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

impl FontDescription {
    pub fn as_family_and_font_style(&self) -> (&str, FontStyle) {
        // support font weights:
        // Thin, ExtraLight, Light, Normal, Medium, SemiBold, Bold, ExtraBold, Black, ExtraBlack
        // W{weight}
        // support font slants:
        // Upright, Italic, Oblique

        let style = if let Some(style) = &self.style {
            let mut weight = Weight::NORMAL;
            let mut slant = Slant::Upright;

            for part in style.split_whitespace() {
                match part {
                    "Thin" => weight = Weight::THIN,
                    "ExtraLight" => weight = Weight::EXTRA_LIGHT,
                    "Light" => weight = Weight::LIGHT,
                    "Normal" => weight = Weight::NORMAL,
                    "Medium" => weight = Weight::MEDIUM,
                    "SemiBold" => weight = Weight::SEMI_BOLD,
                    "Bold" => weight = Weight::BOLD,
                    "ExtraBold" => weight = Weight::EXTRA_BOLD,
                    "Black" => weight = Weight::BLACK,
                    "ExtraBlack" => weight = Weight::EXTRA_BLACK,
                    "Italic" => slant = Slant::Italic,
                    "Oblique" => slant = Slant::Oblique,
                    _ => {
                        if let Some(rest) = part.strip_prefix('W') {
                            if let Ok(weight_value) = rest.parse::<i32>() {
                                weight = Weight::from(weight_value);
                            }
                        }
                    }
                }
            }
            FontStyle::new(weight, Width::NORMAL, slant)
        } else {
            FontStyle::default()
        };
        (self.family.as_str(), style)
    }
}

impl SecondaryFontDescription {
    pub fn fallback(&self, primary: &[FontDescription]) -> Vec<FontDescription> {
        if let Some(family) = &self.family {
            vec![FontDescription {
                family: family.clone(),
                style: self.style.clone(),
            }]
        } else {
            primary
                .iter()
                .map(|font| FontDescription {
                    family: font.family.clone(),
                    style: self.style.clone(),
                })
                .collect()
        }
    }
}

impl fmt::Display for FontDescription {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.family)?;
        if let Some(style) = &self.style {
            write!(f, " {}", style)?;
        }
        Ok(())
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
            font_options.normal.len(),
            1,
            "font list length should equal {}, but {}",
            1,
            font_options.normal.len(),
        );
    }

    #[test]
    fn test_parse_many_fonts_from_guifont_setting() {
        let guifont_setting = "Fira Code Mono,Console";
        let font_options = FontOptions::parse(guifont_setting).unwrap();

        assert_eq!(
            font_options.normal.len(),
            2,
            "font list length should equal {}, but {}",
            2,
            font_options.normal.len(),
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

        for font in font_options.normal.iter() {
            assert_eq!(
                font.family, "Fira Code Mono",
                "font family should equal {}, but {}",
                "Fira Code Mono", font.family,
            );

            assert_eq!(
                font.style,
                Some("Bold Italic".to_string()),
                "font style should equal {:?}, but {:?}",
                Some("Bold Italic".to_string()),
                font.style,
            );
        }

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

    #[test]
    fn test_parse_font_style() {
        let font_style = FontDescription {
            family: "Fira Code Mono".to_string(),
            style: Some("Bold Italic".to_string()),
        };

        let (family, style) = font_style.as_family_and_font_style();

        assert_eq!(
            family, "Fira Code Mono",
            "font family should equal {}, but {}",
            family, "Fira Code Mono"
        );

        assert_eq!(style.weight(), Weight::BOLD);
        assert_eq!(style.slant(), Slant::Italic);
    }

    #[test]
    fn test_parse_font_style_semibold() {
        let font_style = FontDescription {
            family: "Fira Code Mono".to_string(),
            style: Some("SemiBold".to_string()),
        };

        let (family, style) = font_style.as_family_and_font_style();

        assert_eq!(
            family, "Fira Code Mono",
            "font family should equal {}, but {}",
            family, "Fira Code Mono"
        );

        assert_eq!(style.weight(), Weight::SEMI_BOLD);
        assert_eq!(style.slant(), Slant::Upright);
    }

    #[test]
    fn test_parse_font_style_variable_weight() {
        let font_style = FontDescription {
            family: "Fira Code Mono".to_string(),
            style: Some("W100".to_string()),
        };

        let (family, style) = font_style.as_family_and_font_style();

        assert_eq!(
            family, "Fira Code Mono",
            "font family should equal {}, but {}",
            family, "Fira Code Mono"
        );

        assert_eq!(style.weight(), Weight::from(100));
        assert_eq!(style.slant(), Slant::Upright);
    }
}
