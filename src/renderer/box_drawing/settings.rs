use serde::Deserialize;

#[derive(Debug, Deserialize, Default, Clone, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum BoxDrawingMode {
    /// render box chars using glyphs in current font
    FontGlyph,
    /// render box chars natively, ignoring glyph data in the current font. If native rendering
    /// is not supported for a particular unicode char, fall back to glyphs in font.
    #[default]
    Native,
    /// render only a specific subset of box drawing unicode char using native drawing. Each
    /// unicode char in the string is considered to be a sigle box char.
    SelectedNative,
}

#[derive(Debug, Deserialize, Default, Clone, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct BoxDrawingSettings {
    pub mode: Option<BoxDrawingMode>,
    pub selected: Option<String>,
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
