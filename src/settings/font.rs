use std::collections::HashMap;

use serde::Deserialize;

use crate::renderer::fonts::font_options::{
    FontDescription, FontEdging, FontFeature, FontHinting, FontOptions, SecondaryFontDescription,
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
    Single(SimpleSecondaryFontDescription),
    Vec(Vec<SimpleSecondaryFontDescription>),
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
            size: value.size,
            width: 0.0,
            allow_float_size: value.allow_float_size.unwrap_or_default(),
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
    use super::*;

    #[test]
    fn test_font_settings() {
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
                // assert_eq!(fonts[0].family, "Consolas");
                // assert_eq!(fonts[1].family, "Noto Emoji");
            }
            _ => panic!("Unexpected value"),
        }
    }
}
