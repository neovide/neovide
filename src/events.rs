use skulpin::skia_safe::Color4f;

pub struct GridLineCell {
    text: String,
    highlight_id: Option<usize>,
    repeat: Option<usize>
}

pub enum RedrawEvent {
    Resize { grid: usize, width: usize, height: usize },
    DefaultColorsSet { foreground: Color4f, background: Color4f, special: Color4f },
    HighlightAttributesDefine { id: usize, style: Style },
    GridLine { grid: usize, row: usize, column_start: usize, cells: Vec<GridLineCell> },
    Clear { grid: usize },
    CursorGoto { grid: usize, row: usize, column: usize },
    Scroll { grid: usize, top: usize, bottom: usize, left: usize, right: usize, rows: isize, cols: isize }
}

fn unpack_color(packed_color: u64) -> Color4f {
    let packed_color = packed_color as u32;
    let r = ((packed_color & 0xff0000) >> 16) as f32;
    let g = ((packed_color & 0xff00) >> 8) as f32;
    let b = (packed_color & 0xff) as f32;
    Color4f {
        r: r / 255.0,
        g: g / 255.0,
        b: b / 255.0,
        a: 1.0
    }
}

pub fn parse_neovim_event(event_name: 
