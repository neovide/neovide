use std::collections::HashMap;
use std::sync::Arc;

use skulpin::skia_safe::Color4f;

use super::style::{Style, Colors};

#[derive(Debug, Clone, PartialEq)]
pub enum CursorShape {
    Block,
    Horizontal,
    Vertical
}

impl CursorShape {
    pub fn from_type_name(name: &str) -> Option<CursorShape> {
        match name {
            "block" => Some(CursorShape::Block),
            "horizontal" => Some(CursorShape::Horizontal),
            "vertical" => Some(CursorShape::Vertical),
            _ => None
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

#[derive(Clone, PartialEq)]
pub struct Cursor {
    pub position: (u64, u64),
    pub shape: CursorShape,
    pub cell_percentage: Option<f32>,
    pub blinkwait: Option<u64>,
    pub blinkon: Option<u64>,
    pub blinkoff: Option<u64>,
    pub style: Option<Arc<Style>>,
    pub enabled: bool,
    pub mode_list: Vec<CursorMode>
}

impl Cursor {
    pub fn new() -> Cursor {
        Cursor {
            position: (0, 0),
            shape: CursorShape::Block,
            style: None,
            cell_percentage: None,
            blinkwait: None,
            blinkon: None,
            blinkoff: None,
            enabled: true,
            mode_list: Vec::new()
        }
    }

    pub fn foreground(&self, default_colors: &Colors) -> Color4f {
        if let Some(style) = &self.style {
            style.colors.foreground.clone().unwrap_or_else(||default_colors.background.clone().unwrap())
        } else {
            default_colors.background.clone().unwrap()
        }
    }

    pub fn background(&self, default_colors: &Colors) -> Color4f {
        if let Some(style) = &self.style {
            style.colors.background.clone().unwrap_or_else(||default_colors.foreground.clone().unwrap())
        } else {
            default_colors.foreground.clone().unwrap()
        }
    }

    pub fn change_mode(&mut self, mode_index: u64, styles: &HashMap<u64, Arc<Style>>) {
        if let Some(CursorMode { shape, style_id, cell_percentage, blinkwait, blinkon, blinkoff }) = self.mode_list.get(mode_index as usize) {
            if let Some(shape) = shape {
                self.shape = shape.clone();
            }

            if let Some(style_id) = style_id {
                self.style = styles
                    .get(style_id)
                    .cloned();
            }

            self.cell_percentage = *cell_percentage;
            self.blinkwait = *blinkwait;
            self.blinkon = *blinkon;
            self.blinkoff = *blinkoff;
        }
    }
}
