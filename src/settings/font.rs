use std::collections::HashMap;

use serde::Deserialize;

use crate::renderer::fonts::font_options::{
    points_to_pixels, FontDescription, FontEdging, FontFeature, FontHinting, FontOptions,
    SecondaryFontDescription,
};

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum SimpleFontDescription {
    String(String),
    Details(FontDescription),
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum FontDescriptionSettings {
    Vec(Vec<SimpleFontDescription>),
    Single(SimpleFontDescription),
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum SimpleSecondaryFontDescription {
    String(String),
    Details(SecondaryFontDescription),
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum SecondaryFontDescriptionSettings {
    Vec(Vec<SimpleSecondaryFontDescription>),
    Single(SimpleSecondaryFontDescription),
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct FontSettings {
    /// Font family to use for the normal font.
    pub normal: FontDescriptionSettings,
    pub bold: Option<SecondaryFontDescriptionSettings>,
    pub italic: Option<SecondaryFontDescriptionSettings>,
    pub bold_italic: Option<SecondaryFontDescriptionSettings>,
    pub size: f32,
    pub width: Option<f32>,
    pub features: Option<HashMap<String /* family */, Vec<String> /* features */>>,
    pub allow_float_size: Option<bool>,
    pub hinting: Option<String>,
    pub edging: Option<String>,
}

impl From<FontDescriptionSettings> for Vec<FontDescription> {
    fn from(value: FontDescriptionSettings) -> Self {
        match value {
            FontDescriptionSettings::Single(value) => vec![value.into()],
            FontDescriptionSettings::Vec(value) => value.into_iter().map(|x| x.into()).collect(),
        }
    }
}

impl From<SecondaryFontDescriptionSettings> for Vec<SecondaryFontDescription> {
    fn from(value: SecondaryFontDescriptionSettings) -> Self {
        match value {
            SecondaryFontDescriptionSettings::Single(value) => vec![value.into()],
            SecondaryFontDescriptionSettings::Vec(value) => {
                value.into_iter().map(|x| x.into()).collect()
            }
        }
    }
}

impl From<SimpleFontDescription> for FontDescription {
    fn from(value: SimpleFontDescription) -> Self {
        match value {
            SimpleFontDescription::String(value) => FontDescription {
                family: value,
                style: None,
            },
            SimpleFontDescription::Details(value) => value,
        }
    }
}

impl From<SimpleSecondaryFontDescription> for SecondaryFontDescription {
    fn from(value: SimpleSecondaryFontDescription) -> Self {
        match value {
            SimpleSecondaryFontDescription::String(value) => SecondaryFontDescription {
                family: Some(value),
                style: None,
            },
            SimpleSecondaryFontDescription::Details(value) => value,
        }
    }
}

impl From<FontSettings> for FontOptions {
    fn from(value: FontSettings) -> Self {
        FontOptions {
            normal: value.normal.into(),
            italic: value.italic.map(|value| value.into()),
            bold: value.bold.map(|value| value.into()),
            bold_italic: value.bold_italic.map(|value| value.into()),
            features: value
                .features
                .map(|features| {
                    features
                        .into_iter()
                        .map(|(family, features)| {
                            (
                                family,
                                features
                                    .iter()
                                    .map(|feature| FontFeature::parse(feature))
                                    .filter_map(|x| x.ok())
                                    .collect::<Vec<_>>(),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default(),
            size: points_to_pixels(value.size),
            width: points_to_pixels(value.width.unwrap_or_default()),
            hinting: value
                .hinting
                .map(|hinting| FontHinting::parse(&hinting).unwrap_or_default())
                .unwrap_or_default(),
            edging: value
                .edging
                .map(|edging| FontEdging::parse(&edging).unwrap_or_default())
                .unwrap_or_default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::renderer::fonts::font_options::CoarseStyle;

    use super::*;

    #[test]
    fn test_normal_font_single() {
        let settings = r#"
        {
            "normal": "Consolas",
            "size": 20
        }
        "#;

        let settings: FontSettings = serde_json::from_str(settings).unwrap();
        match settings.normal {
            FontDescriptionSettings::Single(font) => {
                let font: FontDescription = font.into();
                assert_eq!(font.family, "Consolas");
            }
            _ => panic!("Unexpected value"),
        }
    }

    #[test]
    fn test_normal_font_vec() {
        let settings = r#"
        {
            "normal": ["Consolas", "Noto Emoji"],
            "size": 20
        }
        "#;

        let settings: FontSettings = serde_json::from_str(settings).unwrap();
        match settings.normal {
            FontDescriptionSettings::Vec(fonts) => {
                assert_eq!(fonts.len(), 2);
                let font0: FontDescription = fonts[0].clone().into();
                assert_eq!(font0.family, "Consolas");
                let font1: FontDescription = fonts[1].clone().into();
                assert_eq!(font1.family, "Noto Emoji");
            }
            _ => panic!("Unexpected value"),
        }
    }

    #[test]
    fn test_secondary_font_single() {
        let settings = r#"
        {
            "normal": "Consolas",
            "bold": "Consolas",
            "size": 20
        }
        "#;

        let settings: FontSettings = serde_json::from_str(settings).unwrap();
        match settings.bold {
            Some(SecondaryFontDescriptionSettings::Single(font)) => {
                let font: SecondaryFontDescription = font.into();
                assert_eq!(font.family.unwrap(), "Consolas");
            }
            _ => panic!("Unexpected value"),
        }
    }

    #[test]
    fn test_secondary_font_vec() {
        let settings = r#"
        {
            "normal": "Consolas",
            "bold": ["Consolas", "Noto Emoji"],
            "size": 20
        }
        "#;

        let settings: FontSettings = serde_json::from_str(settings).unwrap();
        match settings.bold {
            Some(SecondaryFontDescriptionSettings::Vec(fonts)) => {
                assert_eq!(fonts.len(), 2);
                let font0: SecondaryFontDescription = fonts[0].clone().into();
                assert_eq!(font0.family.unwrap(), "Consolas");
                let font1: SecondaryFontDescription = fonts[1].clone().into();
                assert_eq!(font1.family.unwrap(), "Noto Emoji");
            }
            _ => panic!("Unexpected value"),
        }
    }

    #[test]
    fn test_secondary_font_not_found_fallback() {
        let settings = r#"
        {
            "normal": ["Consolas", "Noto Emoji"],
            "bold": "NotFound",
            "size": 19
        }
        "#;

        let settings: FontSettings = serde_json::from_str(settings).unwrap();
        let options = FontOptions::from(settings);
        let style: CoarseStyle = CoarseStyle::permutations()
            .filter(|style| style.name() == Some("Bold"))
            .collect::<Vec<CoarseStyle>>()[0];
        let fonts = options.font_list(style);

        assert_eq!(fonts.len(), 3);
        assert_eq!(
            fonts,
            vec![
                FontDescription {
                    family: "NotFound".into(),
                    style: Some("Bold".into())
                },
                FontDescription {
                    family: "Consolas".into(),
                    style: Some("Bold".into())
                },
                FontDescription {
                    family: "Noto Emoji".into(),
                    style: Some("Bold".into())
                }
            ]
        );
    }

    #[test]
    fn test_oneof_secondary_font_not_found_fallback() {
        let settings = r#"
        {
            "normal": ["Consolas", "Noto Emoji"],
            "bold": ["NotFound", "Menlo"],
            "size": 19
        }
        "#;

        let settings: FontSettings = serde_json::from_str(settings).unwrap();
        let options = FontOptions::from(settings);
        let style: CoarseStyle = CoarseStyle::permutations()
            .filter(|style| style.name() == Some("Bold"))
            .collect::<Vec<CoarseStyle>>()[0];
        let fonts = options.font_list(style);

        assert_eq!(fonts.len(), 4);
        assert_eq!(
            fonts,
            vec![
                FontDescription {
                    family: "NotFound".into(),
                    style: Some("Bold".into())
                },
                FontDescription {
                    family: "Menlo".into(),
                    style: Some("Bold".into())
                },
                FontDescription {
                    family: "Consolas".into(),
                    style: Some("Bold".into())
                },
                FontDescription {
                    family: "Noto Emoji".into(),
                    style: Some("Bold".into())
                }
            ]
        );
    }
}
