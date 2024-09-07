use std::{collections::HashMap, sync::Arc};

use super::{style::Style, OpacitySetting};

/// The `StyleRegistry` struct is responsible for keeping styles updated with corresponding opacity settings.
/// Styles and opacities are associated with background and foreground packed colors.
#[derive(Default)]
pub struct StyleRegistry {
    /// Maps style IDs to their corresponding styles
    defined_styles: HashMap<u64, Arc<Style>>,

    /// Associates each color with opacity settings.
    /// This is used to update the opacity of all styles when the global opacity changes.
    defined_opacities: HashMap<u64, OpacitySetting>,

    /// Maps packed colors to their opacity settings
    background_color_style_map: HashMap<u64, Vec<u64>>,

    /// Maps packed foreground colors to their corresponding style IDs
    foreground_color_style_map: HashMap<u64, Vec<u64>>,
}

impl StyleRegistry {
    pub fn new() -> Self {
        Self {
            defined_opacities: HashMap::new(),
            defined_styles: HashMap::new(),
            background_color_style_map: HashMap::new(),
            foreground_color_style_map: HashMap::new(),
        }
    }

    pub fn default_style(&self) -> Option<Style> {
        self.defined_styles.get(&0).map(|style| (**style).clone())
    }

    pub fn defined_styles(&self) -> &HashMap<u64, Arc<Style>> {
        &self.defined_styles
    }

    pub fn set_style(&mut self, mut style: Style, id: u64) {
        self.update_style_opacities_from_existing_mapping(&mut style);
        self.update_color_to_style_mapping(&style, id);
        self.defined_styles.insert(id, Arc::new(style));
    }

    /// Set the foreground and background opacity of a color and update all styles that use this color
    pub fn set_opacity(&mut self, packed_color: u64, color_opacity: OpacitySetting) {
        // Update the opacity of all styles that use this color
        if let Some(styles_id_with_same_background) =
            self.background_color_style_map.get(&packed_color)
        {
            styles_id_with_same_background.iter().for_each(|id| {
                if let Some(arc) = self.defined_styles.get(id) {
                    let mut style = (**arc).to_owned();
                    style.set_background_opacity(color_opacity.clone());
                    self.defined_styles.insert(*id, Arc::new(style));
                }
            });
        }

        if let Some(styles_id_with_same_foreground) =
            self.foreground_color_style_map.get(&packed_color)
        {
            styles_id_with_same_foreground.iter().for_each(|id| {
                if let Some(arc) = self.defined_styles.get(id) {
                    let mut style = (**arc).to_owned();
                    style.set_foreground_opacity(color_opacity.clone());
                    self.defined_styles.insert(*id, Arc::new(style));
                }
            });
        }

        self.defined_opacities.insert(packed_color, color_opacity);
    }

    /// Updates the opacity of the background and foreground style based on an existing opacity mapping.
    /// This function should be called when a style is defined.
    fn update_style_opacities_from_existing_mapping(&mut self, style: &mut Style) {
        if let Some(o) = style.bg().and_then(|bg| self.defined_opacities.get(&bg)) {
            style.set_background_opacity(o.clone());
        }

        if let Some(o) = style.fg().and_then(|fg| self.defined_opacities.get(&fg)) {
            style.set_foreground_opacity(o.clone());
        }
    }

    /// Add style id in the background and foreground mapping with corresponding color.
    /// Should be called when a new style is defined
    fn update_color_to_style_mapping(&mut self, style: &Style, id: u64) {
        if let Some(packed_color) = style.bg() {
            self.background_color_style_map
                .entry(packed_color)
                .or_default()
                .push(id);
        }

        if let Some(packed_color) = style.fg() {
            self.foreground_color_style_map
                .entry(packed_color)
                .or_default()
                .push(id);
        }
    }
}
