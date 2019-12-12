use std::error;
use std::fmt;

use rmpv::Value;
use skulpin::skia_safe::Color4f;

use crate::editor::{Colors, Style};

#[derive(Debug, Clone)]
pub enum EventParseError {
    InvalidArray(Value),
    InvalidString(Value),
    InvalidU64(Value),
    InvalidI64(Value),
    InvalidEventFormat
}
type Result<T> = std::result::Result<T, EventParseError>;

impl fmt::Display for EventParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            EventParseError::InvalidArray(value) => write!(f, "invalid array format {}", value),
            EventParseError::InvalidString(value) => write!(f, "invalid string format {}", value),
            EventParseError::InvalidU64(value) => write!(f, "invalid u64 format {}", value),
            EventParseError::InvalidI64(value) => write!(f, "invalid i64 format {}", value),
            EventParseError::InvalidEventFormat => write!(f, "invalid event format")
        }
    }
}

impl error::Error for EventParseError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        None
    }
}

#[derive(Debug)]
pub struct GridLineCell {
    pub text: String,
    pub highlight_id: Option<u64>,
    pub repeat: Option<u64>
}

#[derive(Debug)]
pub enum RedrawEvent {
    Resize { grid: u64, width: u64, height: u64 },
    DefaultColorsSet { foreground: Color4f, background: Color4f, special: Color4f },
    HighlightAttributesDefine { id: u64, style: Style },
    GridLine { grid: u64, row: u64, column_start: u64, cells: Vec<GridLineCell> },
    Clear { grid: u64 },
    CursorGoto { grid: u64, row: u64, column: u64 },
    Scroll { grid: u64, top: u64, bottom: u64, left: u64, right: u64, rows: i64, columns: i64 }
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

fn parse_array(array_value: &Value) -> Result<Vec<Value>> {
    if let Value::Array(content) = array_value.clone() {
        Ok(content.to_vec())
    } else {
        Err(EventParseError::InvalidArray(array_value.clone()))
    }
}

fn parse_string(string_value: &Value) -> Result<String> {
    if let Value::String(content) = string_value.clone() {
        Ok(content.into_str().ok_or(EventParseError::InvalidString(string_value.clone()))?)
    } else {
        Err(EventParseError::InvalidString(string_value.clone()))
    }
}

fn parse_u64(u64_value: &Value) -> Result<u64> {
    if let Value::Integer(content) = u64_value.clone() {
        Ok(content.as_u64().ok_or(EventParseError::InvalidU64(u64_value.clone()))?)
    } else {
        Err(EventParseError::InvalidU64(u64_value.clone()))
    }
}

fn parse_i64(i64_value: &Value) -> Result<i64> {
    if let Value::Integer(content) = i64_value.clone() {
        Ok(content.as_i64().ok_or(EventParseError::InvalidI64(i64_value.clone()))?)
    } else {
        Err(EventParseError::InvalidI64(i64_value.clone()))
    }
}

fn parse_default_colors(default_colors_arguments: Vec<Value>) -> Result<RedrawEvent> {
    if let [
        foreground, background, special, _term_foreground, _term_background
    ] = default_colors_arguments.as_slice() {
        Ok(RedrawEvent::DefaultColorsSet {
            foreground: unpack_color(parse_u64(&foreground)?),
            background: unpack_color(parse_u64(&background)?),
            special: unpack_color(parse_u64(special)?),
        })
    } else {
        Err(EventParseError::InvalidEventFormat)
    }
}

fn parse_hl_attr_define(hl_attr_define_arguments: Vec<Value>) -> Result<RedrawEvent> {
    if let [
        id, Value::Map(attributes), _terminal_attributes, _info
    ] = hl_attr_define_arguments.as_slice() {
        let mut style = Style::new(Colors::new(None, None, None));
        for attribute in attributes {
            if let (Value::String(name), value) = attribute {
                match (name.as_str().unwrap(), value) {
                    ("foreground", Value::Integer(packed_color)) => style.colors.foreground = Some(unpack_color(packed_color.as_u64().unwrap())),
                    ("background", Value::Integer(packed_color)) => style.colors.background = Some(unpack_color(packed_color.as_u64().unwrap())),
                    ("special", Value::Integer(packed_color)) => style.colors.special = Some(unpack_color(packed_color.as_u64().unwrap())),
                    _ => println!("Ignored style attribute: {}", name)
                }
            } else {
                println!("Invalid attribute format");
            }
        }
        Ok(RedrawEvent::HighlightAttributesDefine { id: parse_u64(&id)?, style })
    } else {
        Err(EventParseError::InvalidEventFormat)
    }
}

fn parse_grid_line_cell(grid_line_cell: Value) -> Result<GridLineCell> {
    let cell_contents = parse_array(&grid_line_cell)?;
    let text_value = cell_contents.get(0).ok_or(EventParseError::InvalidEventFormat)?;
    Ok(GridLineCell {
        text: parse_string(&text_value)?,
        highlight_id: cell_contents.get(1).map(|highlight_id| parse_u64(highlight_id)).transpose()?,
        repeat: cell_contents.get(2).map(|repeat| parse_u64(repeat)).transpose()?
    })
}

fn parse_grid_line(grid_line_arguments: Vec<Value>) -> Result<RedrawEvent> {
    if let [grid_id, row, column_start, cells] = grid_line_arguments.as_slice() {
        Ok(RedrawEvent::GridLine {
            grid: parse_u64(&grid_id)?, 
            row: parse_u64(&row)?, column_start: parse_u64(&column_start)?,
            cells: parse_array(&cells)?
                .into_iter()
                .map(parse_grid_line_cell)
                .collect::<Result<Vec<GridLineCell>>>()?
        })
    } else {
        Err(EventParseError::InvalidEventFormat)
    }
}

fn parse_clear(clear_arguments: Vec<Value>) -> Result<RedrawEvent> {
    if let [grid_id] = clear_arguments.as_slice() {
        Ok(RedrawEvent::Clear { grid: parse_u64(&grid_id)? })
    } else {
        Err(EventParseError::InvalidEventFormat)
    }
}

fn parse_cursor_goto(cursor_goto_arguments: Vec<Value>) -> Result<RedrawEvent> {
    if let [grid_id, column, row] = cursor_goto_arguments.as_slice() {
        Ok(RedrawEvent::CursorGoto { 
            grid: parse_u64(&grid_id)?, row: parse_u64(&row)?, column: parse_u64(&column)?
        })
    } else {
        Err(EventParseError::InvalidEventFormat)
    }
}

fn parse_grid_scroll(grid_scroll_arguments: Vec<Value>) -> Result<RedrawEvent> {
    if let [grid_id, top, bottom, left, right, rows, columns] = grid_scroll_arguments.as_slice() {
        Ok(RedrawEvent::Scroll {
            grid: parse_u64(&grid_id)?, 
            top: parse_u64(&top)?, bottom: parse_u64(&bottom)?,
            left: parse_u64(&left)?, right: parse_u64(&right)?,
            rows: parse_i64(&rows)?, columns: parse_i64(&columns)?
        })
    } else {
        Err(EventParseError::InvalidEventFormat)
    }
}

pub fn parse_redraw_event(event_value: Value) -> Result<Vec<RedrawEvent>> {
    let mut event_contents = parse_array(&event_value)?.to_vec();
    let name_value = event_contents.get(0).ok_or(EventParseError::InvalidEventFormat)?;
    let event_name = parse_string(&name_value)?;
    let events = event_contents;
    let mut parsed_events = Vec::new();

    for event in &events[1..] {
        let event_parameters = parse_array(&event)?;
        let possible_parsed_event = match event_name.clone().as_ref() {
            "default_colors_set" => Some(parse_default_colors(event_parameters)?),
            "hl_attr_define" => Some(parse_hl_attr_define(event_parameters)?),
            "grid_line" => Some(parse_grid_line(event_parameters)?),
            "grid_clear" => Some(parse_clear(event_parameters)?),
            "grid_cursor_goto" => Some(parse_cursor_goto(event_parameters)?),
            "grid_scroll" => Some(parse_grid_scroll(event_parameters)?),
            _ => None
        };

        if let Some(parsed_event) = possible_parsed_event {
            parsed_events.push(parsed_event);
        } else {
            println!("Did not parse {}", event_name);
        }
    }

    Ok(parsed_events)
}

pub fn parse_neovim_event(event_name: String, events: Vec<Value>) -> Result<Vec<RedrawEvent>> {
    let mut resulting_events = Vec::new();
    if event_name == "redraw" {
        for event in events {
            resulting_events.append(&mut parse_redraw_event(event)?);
        }
    } else {
        println!("Unknown global event {}", event_name);
    }
    Ok(resulting_events)
}






