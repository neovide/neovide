use skia_safe::font_style::Weight;

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub enum FontWeight {
    Invisible,
    Thin,
    ExtraLight,
    Light,
    Normal,
    Medium,
    SemiBold,
    Bold,
    ExtraBold,
    Black,
    ExtraBlack,
    Custom(u32),
}

impl From<u32> for FontWeight {
    fn from(weight: u32) -> Self {
        match weight {
            0 => FontWeight::Invisible,
            100 => FontWeight::Thin,
            200 => FontWeight::ExtraLight,
            300 => FontWeight::Light,
            400 => FontWeight::Normal,
            500 => FontWeight::Medium,
            600 => FontWeight::SemiBold,
            700 => FontWeight::Bold,
            800 => FontWeight::ExtraBold,
            900 => FontWeight::Black,
            1000 => FontWeight::ExtraBlack,
            other => FontWeight::Custom(other),
        }
    }
}

impl From<Weight> for FontWeight {
    fn from(weight: Weight) -> Self {
        (*weight as u32).into()
    }
}

impl From<FontWeight> for Weight {
    fn from(weight: FontWeight) -> Self {
        match weight {
            FontWeight::Invisible => Weight::INVISIBLE,
            FontWeight::Thin => Weight::THIN,
            FontWeight::ExtraLight => Weight::EXTRA_LIGHT,
            FontWeight::Light => Weight::LIGHT,
            FontWeight::Normal => Weight::NORMAL,
            FontWeight::Medium => Weight::MEDIUM,
            FontWeight::SemiBold => Weight::SEMI_BOLD,
            FontWeight::Bold => Weight::BOLD,
            FontWeight::ExtraBold => Weight::EXTRA_BOLD,
            FontWeight::Black => Weight::BLACK,
            FontWeight::ExtraBlack => Weight::EXTRA_BLACK,
            FontWeight::Custom(other) => (other as i32).into(),
        }
    }
}
