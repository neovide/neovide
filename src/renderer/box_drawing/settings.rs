use serde::Deserialize;

#[derive(Debug, Deserialize, Default, Clone, PartialEq, Eq)]
#[serde(tag = "type", content = "chars")]
#[serde(rename_all = "snake_case")]
pub enum BoxDrawingMode {
    /// render box chars using glyphs in current font
    #[default]
    FontGlyph,
    /// render box chars natively, ignoring glyph data in the current font. If native rendering
    /// is not supported for a particular unicode char, fall back to glyphs in font.
    Native,
    /// render only a specific subset of box drawing unicode char using native drawing. Each
    /// unicode char in the string is considered to be a sigle box char.
    SelectedNative(String),
}

#[derive(Debug, Deserialize, Default, Clone, PartialEq)]
pub struct BoxDrawingSettings {
    pub mode: Option<BoxDrawingMode>,
    pub thickness_multipliers: Option<ThicknessMultipliers>,
    pub stroke_width_ratio: Option<f32>,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq)]
pub struct ThicknessMultipliers(pub [f32; 3]);

impl Default for ThicknessMultipliers {
    fn default() -> Self {
        Self([1.0, 1.5, 2.0])
    }
}
