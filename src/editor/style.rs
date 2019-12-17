use skulpin::skia_safe::Color4f;

#[derive(new, PartialEq, Debug, Clone)]
pub struct Colors {
    pub foreground: Option<Color4f>,
    pub background: Option<Color4f>,
    pub special: Option<Color4f>
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
    pub underline: bool,
    #[new(default)]
    pub undercurl: bool,
    #[new(default)]
    pub blend: u8
}

impl Style {
    pub fn foreground(&self, default_colors: &Colors) -> Color4f {
        if self.reverse {
            self.colors.background.clone().unwrap_or(default_colors.background.clone().unwrap())
        } else {
            self.colors.foreground.clone().unwrap_or(default_colors.foreground.clone().unwrap())
        }
    }

    pub fn background(&self, default_colors: &Colors) -> Color4f {
        if self.reverse {
            self.colors.foreground.clone().unwrap_or(default_colors.foreground.clone().unwrap())
        } else {
            self.colors.background.clone().unwrap_or(default_colors.background.clone().unwrap())
        }
    }

    pub fn special(&self, default_colors: &Colors) -> Color4f {
        self.colors.special.clone().unwrap_or(default_colors.special.clone().unwrap())
    }
}
