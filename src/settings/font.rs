use std::collections::HashMap;

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, Hash, PartialEq, Eq)]
pub struct FamilyAndStyle {
    pub family: String,
    pub style: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct FontSettings {
    /// Font family to use for the normal font.
    pub family: FamilyAndStyle,
    pub bold: Option<FamilyAndStyle>,
    pub italic: Option<FamilyAndStyle>,
    pub bold_italic: Option<FamilyAndStyle>,
    pub size: f32,
    pub width: String,
    pub features: Option<HashMap<String /* family */, String /* features */>>,
}
