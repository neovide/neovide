use std::{collections::HashMap, sync::Arc};

use skia_safe::Color4f;

use crate::editor::style::{Colors, Style};

use super::grid::GridCell;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum CursorShape {
    Block,
    Horizontal,
    Vertical,
}

impl CursorShape {
    pub fn from_type_name(name: &str) -> Option<CursorShape> {
        match name {
            "block" => Some(CursorShape::Block),
            "horizontal" => Some(CursorShape::Horizontal),
            "vertical" => Some(CursorShape::Vertical),
            _ => None,
        }
    }
}

#[derive(Default, Debug, Clone, PartialEq)]
pub struct CursorMode {
    pub shape: Option<CursorShape>,
    pub style_id: Option<u64>,
    pub cell_percentage: Option<f32>,
    pub blinkwait: Option<u64>,
    pub blinkon: Option<u64>,
    pub blinkoff: Option<u64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Cursor {
    pub grid_position: (u64, u64),
    pub parent_window_id: u64,
    pub shape: CursorShape,
    pub cell_percentage: Option<f32>,
    pub blinkwait: Option<u64>,
    pub blinkon: Option<u64>,
    pub blinkoff: Option<u64>,
    pub style: Option<Arc<Style>>,
    pub enabled: bool,
    pub double_width: bool,
    pub grid_cell: GridCell,
}

impl Cursor {
    pub fn new() -> Cursor {
        Cursor {
            grid_position: (0, 0),
            parent_window_id: 0,
            shape: CursorShape::Block,
            style: None,
            cell_percentage: None,
            blinkwait: None,
            blinkon: None,
            blinkoff: None,
            enabled: true,
            double_width: false,
            grid_cell: (" ".to_string(), None),
        }
    }

    pub fn foreground(&self, default_colors: &Colors) -> Color4f {
        self.style
            .as_ref()
            .and_then(|s| s.colors.foreground)
            .unwrap_or_else(|| default_colors.background.unwrap())
    }

    pub fn background(&self, default_colors: &Colors) -> Color4f {
        self.style
            .as_ref()
            .and_then(|s| s.colors.background)
            .unwrap_or_else(|| default_colors.foreground.unwrap())
    }

    pub fn alpha(&self) -> u8 {
        return self
            .style
            .as_ref()
            .map(|s| (255_f32 * ((100 - s.blend) as f32 / 100.0_f32)) as u8)
            .unwrap_or(255);
    }

    pub fn change_mode(&mut self, cursor_mode: &CursorMode, styles: &HashMap<u64, Arc<Style>>) {
        let CursorMode {
            shape,
            style_id,
            cell_percentage,
            blinkwait,
            blinkon,
            blinkoff,
        } = cursor_mode;

        if let Some(shape) = shape {
            self.shape = shape.clone();
        }

        if let Some(style_id) = style_id {
            self.style = styles.get(style_id).cloned();
        }

        self.cell_percentage = *cell_percentage;
        self.blinkwait = *blinkwait;
        self.blinkon = *blinkon;
        self.blinkoff = *blinkoff;
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

    const NONE_COLORS: Colors = Colors {
        foreground: None,
        background: None,
        special: None,
    };

    #[test]
    fn test_from_type_name() {
        assert_eq!(
            CursorShape::from_type_name("block"),
            Some(CursorShape::Block)
        );
        assert_eq!(
            CursorShape::from_type_name("horizontal"),
            Some(CursorShape::Horizontal)
        );
        assert_eq!(
            CursorShape::from_type_name("vertical"),
            Some(CursorShape::Vertical)
        );
    }

    #[test]
    fn test_foreground() {
        let mut cursor = Cursor::new();
        let style = Some(Arc::new(Style::new(COLORS)));

        assert_eq!(
            cursor.foreground(&DEFAULT_COLORS),
            DEFAULT_COLORS.background.unwrap()
        );
        cursor.style = style;
        assert_eq!(
            cursor.foreground(&DEFAULT_COLORS),
            COLORS.foreground.unwrap()
        );

        cursor.style = Some(Arc::new(Style::new(NONE_COLORS)));
        assert_eq!(
            cursor.foreground(&DEFAULT_COLORS),
            DEFAULT_COLORS.background.unwrap()
        );
    }

    #[test]
    fn test_background() {
        let mut cursor = Cursor::new();
        let style = Some(Arc::new(Style::new(COLORS)));

        assert_eq!(
            cursor.background(&DEFAULT_COLORS),
            DEFAULT_COLORS.foreground.unwrap()
        );
        cursor.style = style;
        assert_eq!(
            cursor.background(&DEFAULT_COLORS),
            COLORS.background.unwrap()
        );

        cursor.style = Some(Arc::new(Style::new(NONE_COLORS)));
        assert_eq!(
            cursor.background(&DEFAULT_COLORS),
            DEFAULT_COLORS.foreground.unwrap()
        );
    }

    #[test]
    fn test_change_mode() {
        let cursor_mode = CursorMode {
            shape: Some(CursorShape::Horizontal),
            style_id: Some(1),
            cell_percentage: Some(100.0),
            blinkwait: Some(1),
            blinkon: Some(1),
            blinkoff: Some(1),
        };
        let mut styles = HashMap::new();
        styles.insert(1, Arc::new(Style::new(COLORS)));

        let mut cursor = Cursor::new();

        cursor.change_mode(&cursor_mode, &styles);
        assert_eq!(cursor.shape, CursorShape::Horizontal);
        assert_eq!(cursor.style, styles.get(&1).cloned());
        assert_eq!(cursor.cell_percentage, Some(100.0));
        assert_eq!(cursor.blinkwait, Some(1));
        assert_eq!(cursor.blinkon, Some(1));
        assert_eq!(cursor.blinkoff, Some(1));

        let cursor_mode_with_none = CursorMode {
            shape: None,
            style_id: None,
            cell_percentage: None,
            blinkwait: None,
            blinkon: None,
            blinkoff: None,
        };
        cursor.change_mode(&cursor_mode_with_none, &styles);
        assert_eq!(cursor.shape, CursorShape::Horizontal);
        assert_eq!(cursor.style, styles.get(&1).cloned());
        assert_eq!(cursor.cell_percentage, None);
        assert_eq!(cursor.blinkwait, None);
        assert_eq!(cursor.blinkon, None);
        assert_eq!(cursor.blinkoff, None);
    }
}
