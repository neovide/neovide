pub struct Panel;
impl Panel {
    pub const WIDTH: f64 = 600.0;
    pub const HEIGHT: f64 = 620.0;
    pub const TITLE_ORIGINAL_HEIGHT: f64 = 28.0;
    pub const HORIZONTAL_PADDING: f64 = 20.0;
    pub const TITLE_ADJUSTMENT: f64 = Self::TITLE_ORIGINAL_HEIGHT - Search::VERTICAL_PADDING;
    pub const CONTENT_HEIGHT: f64 = Self::HEIGHT - Self::TITLE_ADJUSTMENT;
}

pub struct Search;
impl Search {
    pub const FONT_SIZE: f64 = 20.0;
    pub const VERTICAL_PADDING: f64 = 20.0;
    pub const FIELD_TOP_OFFSET: f64 = -2.0;
    pub const FIELD_HEIGHT_EXTRA: f64 = 8.0;
    pub const SECTION_HEIGHT: f64 = Self::VERTICAL_PADDING * 2.0 + Self::FONT_SIZE;
}

pub struct Row;
impl Row {
    pub const HEIGHT: f64 = 48.0;
    pub const VERTICAL_GAP: f64 = 2.0;
    pub const BUTTON_HEIGHT: f64 = Self::HEIGHT - Self::VERTICAL_GAP;
    pub const CORNER_RADIUS: f64 = 6.0;
    pub const ICON_SIZE: f64 = 16.0;
    pub const ICON_TITLE_SPACING: f64 = 16.0;
    pub const TITLE_FONT_SIZE: f64 = 16.0;
    pub const LABEL_HEIGHT: f64 = 20.0;
}

pub struct Results;
impl Results {
    pub const OUTER_PADDING: f64 = 6.0;
    pub const INNER_PADDING: f64 = Panel::HORIZONTAL_PADDING - Self::OUTER_PADDING;
    pub const BOTTOM_PADDING: f64 = 20.0;
    pub const HEIGHT_ADJUSTMENT: f64 = 16.0;
    pub const CONTAINER_HEIGHT: f64 =
        Panel::HEIGHT - Search::SECTION_HEIGHT - BottomBar::HEIGHT - Self::HEIGHT_ADJUSTMENT;
    pub const EMPTY_LABEL_Y: f64 = 22.0;
    pub const EMPTY_LABEL_HEIGHT: f64 = 24.0;
    pub const EMPTY_LABEL_FONT_SIZE: f64 = 16.0;
}

pub struct BottomBar;
impl BottomBar {
    pub const VERTICAL_PADDING: f64 = 6.0;
    pub const FONT_SIZE: f64 = 12.0;
    pub const SHORTCUT_FONT_SIZE: f64 = Self::FONT_SIZE + 3.0;
    pub const BUTTON_VERTICAL_PADDING: f64 = 2.0;
    pub const BUTTON_LEADING_PADDING: f64 = 8.0;
    pub const BUTTON_TRAILING_PADDING: f64 = 2.0;
    pub const BUTTON_SPACING: f64 = 5.0;
    pub const LABEL_SHORTCUT_SPACING: f64 = 8.0;
    pub const SHORTCUT_SPACING: f64 = 2.0;
    pub const SHORTCUT_HORIZONTAL_PADDING: f64 = 6.0;
    pub const SHORTCUT_VERTICAL_PADDING: f64 = 1.0;
    pub const BUTTON_HEIGHT: f64 = 20.0;
    pub const BUTTON_CORNER_RADIUS: f64 = 3.0;
    pub const SHORTCUT_CHIP_CORNER_RADIUS: f64 = 2.0;
    pub const HEIGHT: f64 =
        Self::VERTICAL_PADDING * 2.0 + Self::FONT_SIZE + Self::BUTTON_VERTICAL_PADDING * 2.0;
    pub const HORIZONTAL_PADDING: f64 = Panel::HORIZONTAL_PADDING - Self::BUTTON_TRAILING_PADDING;
}
