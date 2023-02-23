use skia_safe::Color4f;

#[derive(new, Debug, Clone, PartialEq)]
pub struct Colors {
    pub foreground: Option<Color4f>,
    pub background: Option<Color4f>,
    pub special: Option<Color4f>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum UnderlineStyle {
    Underline,
    UnderDouble,
    UnderDash,
    UnderDot,
    UnderCurl,
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
}

impl Style {
    pub fn foreground(&self, default_colors: &Colors) -> Color4f {
        if self.reverse {
            self.colors
                .background
                .unwrap_or_else(|| default_colors.background.unwrap())
        } else {
            self.colors
                .foreground
                .unwrap_or_else(|| default_colors.foreground.unwrap())
        }
    }

    pub fn background(&self, default_colors: &Colors) -> Color4f {
        if self.reverse {
            self.colors
                .foreground
                .unwrap_or_else(|| default_colors.foreground.unwrap())
        } else {
            self.colors
                .background
                .unwrap_or_else(|| default_colors.background.unwrap())
        }
    }

    pub fn special(&self, default_colors: &Colors) -> Color4f {
        self.colors
            .special
            .unwrap_or_else(|| self.foreground(default_colors))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const COLORS: Colors = Colors {
        foreground: Some(Color4f::new(0.1, 0.1, 0.1, 0.1)),
        background: Some(Color4f::new(0.2, 0.1, 0.1, 0.1)),
        special: Some(Color4f::new(0.3, 0.1, 0.1, 0.1)),
    };

    const DEFAULT_COLORS: Colors = Colors {
        foreground: Some(Color4f::new(0.1, 0.2, 0.1, 0.1)),
        background: Some(Color4f::new(0.2, 0.2, 0.1, 0.1)),
        special: Some(Color4f::new(0.3, 0.2, 0.1, 0.1)),
    };

    #[test]
    fn test_foreground() {
        let mut style = Style::new(COLORS);

        assert_eq!(
            style.foreground(&DEFAULT_COLORS),
            COLORS.foreground.unwrap()
        );
        style.colors.foreground = None;
        assert_eq!(
            style.foreground(&DEFAULT_COLORS),
            DEFAULT_COLORS.foreground.unwrap()
        );
    }

    #[test]
    fn test_foreground_reverse() {
        let mut style = Style::new(COLORS);
        style.reverse = true;

        assert_eq!(
            style.foreground(&DEFAULT_COLORS),
            COLORS.background.unwrap()
        );
        style.colors.background = None;
        assert_eq!(
            style.foreground(&DEFAULT_COLORS),
            DEFAULT_COLORS.background.unwrap()
        );
    }

    #[test]
    fn test_background() {
        let mut style = Style::new(COLORS);

        assert_eq!(
            style.background(&DEFAULT_COLORS),
            COLORS.background.unwrap()
        );
        style.colors.background = None;
        assert_eq!(
            style.background(&DEFAULT_COLORS),
            DEFAULT_COLORS.background.unwrap()
        );
    }

    #[test]
    fn test_background_reverse() {
        let mut style = Style::new(COLORS);
        style.reverse = true;

        assert_eq!(
            style.background(&DEFAULT_COLORS),
            COLORS.foreground.unwrap()
        );
        style.colors.foreground = None;
        assert_eq!(
            style.background(&DEFAULT_COLORS),
            DEFAULT_COLORS.foreground.unwrap()
        );
    }

    #[test]
    fn test_special() {
        let mut style = Style::new(COLORS);

        assert_eq!(style.special(&DEFAULT_COLORS), COLORS.special.unwrap());
        style.colors.special = None;
        assert_eq!(
            style.special(&DEFAULT_COLORS),
            style.foreground(&DEFAULT_COLORS),
        );
    }
}
