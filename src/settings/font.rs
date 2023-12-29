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
pub struct FontSettings {
    /// Font family to use for the normal font.
    pub family: Vec<SimpleFontDescription>,
    pub bold: Option<Vec<SecondaryFontDescription>>,
    pub italic: Option<Vec<SecondaryFontDescription>>,
    pub bold_italic: Option<Vec<SecondaryFontDescription>>,
    pub size: f32,
    pub width: Option<f32>,
    pub features: Option<HashMap<String /* family */, Vec<String> /* features */>>,
    pub allow_float_size: Option<bool>,
    pub hinting: Option<String>,
    pub edging: Option<String>,
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

impl From<FontSettings> for FontOptions {
    fn from(value: FontSettings) -> Self {
        FontOptions {
            normal: value.family.into_iter().map(|x| x.into()).collect(),
            italic: value.italic,
            bold: value.bold,
            bold_italic: value.bold_italic,
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
