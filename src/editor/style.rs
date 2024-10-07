use skia_safe::Color4f;

#[derive(new, Debug, Clone, PartialEq)]
pub struct Colors {
    pub foreground: Option<Color4f>,
    pub background: Option<Color4f>,
    pub special: Option<Color4f>,
    pub fg: Option<u64>,
    pub bg: Option<u64>,
    pub sp: Option<u64>,
}

#[derive(Default, Debug, Clone, PartialEq)]
pub struct OpacitySettings {
    pub foreground: Option<OpacitySetting>,
    pub background: Option<OpacitySetting>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum UnderlineStyle {
    Underline,
    UnderDouble,
    UnderDash,
    UnderDot,
    UnderCurl,
}

#[derive(Default, Debug, Clone, PartialEq)]
pub struct OpacitySetting {
    /// Color is always fully opaque when true (except for floating window with blend)
    pub disable: bool,
    /// Base opacity of the color
    pub base_opacity: f32,
    /// Opacity multiplier applied to opacity setting
    pub multiplier: f32,
    /// If true, opacity also applies to foreground color
    pub applies_to_foreground: bool,
}

impl OpacitySetting {
    pub fn compute_background_opacity(&self, opacity: f32) -> f32 {
        match self.disable {
            true => 1.0,
            false => (self.base_opacity + self.multiplier * opacity).clamp(0.0, 1.0),
        }
    }

    pub fn compute_foreground_opacity(&self, opacity: f32) -> f32 {
        if !self.disable && self.applies_to_foreground {
            (self.base_opacity + self.multiplier * opacity).clamp(0.0, 1.0)
        } else {
            1.0
        }
    }
}

#[derive(new, Debug, Clone, PartialEq)]
pub struct Style {
    pub colors: Colors,
    #[new(default)]
    pub reverse: bool,
    #[new(default)]
    pub italic: bool,
    #[new(default)]
    pub bold: bool,
    #[new(default)]
    pub strikethrough: bool,
    #[new(default)]
    pub blend: u8,
    #[new(default)]
    pub underline: Option<UnderlineStyle>,
    #[new(default)]
    pub opacity_settings: OpacitySettings,
}

impl Style {
    pub fn foreground(&self, default_colors: &Colors, opacity: f32) -> Color4f {
        if self.reverse {
            self.background_with_opacity(default_colors, opacity)
        } else {
            self.foreground_with_opacity(default_colors, opacity)
        }
    }

    pub fn background(&self, default_colors: &Colors, opacity: f32) -> Color4f {
        if self.reverse {
            self.foreground_with_opacity(default_colors, opacity)
        } else {
            self.background_with_opacity(default_colors, opacity)
        }
    }

    fn background_with_opacity(&self, default_colors: &Colors, opacity: f32) -> Color4f {
        let default_opacity = default_colors.background.map_or(1.0, |color| color.a);
        self.colors
            .background
            .map_or(default_colors.background.unwrap(), |mut color| {
                color.a = self
                    .opacity_settings
                    .background
                    .as_ref()
                    .map_or(default_opacity, |setting| {
                        setting.compute_background_opacity(opacity)
                    });
                color
            })
    }

    fn foreground_with_opacity(&self, default_colors: &Colors, opacity: f32) -> Color4f {
        let default_opacity = default_colors.foreground.map_or(1.0, |color| color.a);
        self.colors
            .foreground
            .map_or(default_colors.foreground.unwrap(), |mut color| {
                color.a = self
                    .opacity_settings
                    .foreground
                    .as_ref()
                    .map_or(default_opacity, |setting| {
                        setting.compute_foreground_opacity(opacity)
                    });
                color
            })
    }

    pub fn special(&self, default_colors: &Colors, opacity: f32) -> Color4f {
        self.colors
            .special
            .unwrap_or_else(|| self.foreground(default_colors, opacity))
    }

    pub fn fg(&self) -> Option<u64> {
        if self.reverse {
            self.colors.bg
        } else {
            self.colors.fg
        }
    }

    pub fn bg(&self) -> Option<u64> {
        if self.reverse {
            self.colors.fg
        } else {
            self.colors.bg
        }
    }

    pub fn sp(&self) -> Option<u64> {
        self.colors.sp
    }

    pub fn set_background_opacity(&mut self, color_opacity: OpacitySetting) {
        self.opacity_settings.background = Some(color_opacity);
    }

    pub fn set_foreground_opacity(&mut self, color_opacity: OpacitySetting) {
        self.opacity_settings.foreground = Some(color_opacity);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const COLORS: Colors = Colors {
        foreground: Some(Color4f::new(0.1, 0.1, 0.1, 0.1)),
        background: Some(Color4f::new(0.2, 0.1, 0.1, 0.1)),
        special: Some(Color4f::new(0.3, 0.1, 0.1, 0.1)),
        fg: None,
        bg: None,
        sp: None,
    };

    const DEFAULT_COLORS: Colors = Colors {
        foreground: Some(Color4f::new(0.1, 0.2, 0.1, 0.1)),
        background: Some(Color4f::new(0.2, 0.2, 0.1, 0.1)),
        special: Some(Color4f::new(0.3, 0.2, 0.1, 0.1)),
        fg: None,
        bg: None,
        sp: None,
    };

    #[test]
    fn test_foreground() {
        let mut style = Style::new(COLORS);

        assert_eq!(
            style.foreground(&DEFAULT_COLORS, 1.0),
            COLORS.foreground.unwrap()
        );
        style.colors.foreground = None;
        assert_eq!(
            style.foreground(&DEFAULT_COLORS, 1.0),
            DEFAULT_COLORS.foreground.unwrap()
        );
    }

    #[test]
    fn test_foreground_reverse() {
        let mut style = Style::new(COLORS);
        style.reverse = true;

        assert_eq!(
            style.foreground(&DEFAULT_COLORS, 1.0),
            COLORS.background.unwrap()
        );
        style.colors.background = None;
        assert_eq!(
            style.foreground(&DEFAULT_COLORS, 1.0),
            DEFAULT_COLORS.background.unwrap()
        );
    }

    #[test]
    fn test_background() {
        let mut style = Style::new(COLORS);

        assert_eq!(
            style.background(&DEFAULT_COLORS, 1.0),
            COLORS.background.unwrap()
        );
        style.colors.background = None;
        assert_eq!(
            style.background(&DEFAULT_COLORS, 1.0),
            DEFAULT_COLORS.background.unwrap()
        );
    }

    #[test]
    fn test_background_reverse() {
        let mut style = Style::new(COLORS);
        style.reverse = true;

        assert_eq!(
            style.background(&DEFAULT_COLORS, 1.0),
            COLORS.foreground.unwrap()
        );
        style.colors.foreground = None;
        assert_eq!(
            style.background(&DEFAULT_COLORS, 1.0),
            DEFAULT_COLORS.foreground.unwrap()
        );
    }

    #[test]
    fn test_special() {
        let mut style = Style::new(COLORS);

        assert_eq!(style.special(&DEFAULT_COLORS, 1.0), COLORS.special.unwrap());
        style.colors.special = None;
        assert_eq!(
            style.special(&DEFAULT_COLORS, 1.0),
            style.foreground(&DEFAULT_COLORS, 1.0),
        );
    }
}
