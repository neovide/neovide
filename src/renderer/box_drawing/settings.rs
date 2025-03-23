use std::collections::HashMap;

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
    pub sizes: Option<LineSizes>,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct LineSizes(pub HashMap<String, (u16, u16)>);

impl Default for LineSizes {
    fn default() -> Self {
        Self(
            [
                ("default", (1_u16, 3_u16)), // Thin and thick values respectively, below size 12
                ("12", (1, 2)),              // Size 12 to 13.9999
                ("14", (2, 4)),
                ("18", (3, 6)),
            ]
            .into_iter()
            .map(|(k, s)| (k.to_string(), s))
            .collect(),
        )
    }
}
