use std::error;
use std::fmt;

use rmpv::Value;
use skulpin::skia_safe::Color4f;

use crate::editor::{Colors, Style, CursorMode, CursorShape};

#[derive(Debug, Clone)]
pub enum EventParseError {
    InvalidArray(Value),
    InvalidMap(Value),
    InvalidString(Value),
    InvalidU64(Value),
    InvalidI64(Value),
    InvalidBool(Value),
    InvalidEventFormat
}
type Result<T> = std::result::Result<T, EventParseError>;

impl fmt::Display for EventParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            EventParseError::InvalidArray(value) => write!(f, "invalid array format {}", value),
            EventParseError::InvalidMap(value) => write!(f, "invalid map format {}", value),
            EventParseError::InvalidString(value) => write!(f, "invalid string format {}", value),
            EventParseError::InvalidU64(value) => write!(f, "invalid u64 format {}", value),
            EventParseError::InvalidI64(value) => write!(f, "invalid i64 format {}", value),
            EventParseError::InvalidBool(value) => write!(f, "invalid bool format {}", value),
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

pub type StyledContent = Vec<(u64, String)>;

#[derive(Debug)]
pub enum MessageKind {
    Unknown,
    Confirm,
    ConfirmSubstitute,
    Error,
    Echo,
    EchoMessage,
    EchoError,
    LuaError,
    RpcError,
    ReturnPrompt,
    QuickFix,
    SearchCount,
    Warning
}

impl MessageKind {
    pub fn parse(kind: &str) -> MessageKind {
        match kind {
            "confirm" => MessageKind::Confirm,
            "confirm_sub" => MessageKind::ConfirmSubstitute,
            "emsg" => MessageKind::Error,
            "echo" => MessageKind::Echo,
            "echomsg" => MessageKind::EchoMessage,
            "echoerr" => MessageKind::EchoError,
            "lua_error" => MessageKind::LuaError,
            "rpc_error" => MessageKind::RpcError,
            "return_prompt" => MessageKind::ReturnPrompt,
            "quickfix" => MessageKind::QuickFix,
            "search_count" => MessageKind::SearchCount,
            "wmsg" => MessageKind::Warning,
            _ => MessageKind::Unknown
        }
    }
}

#[derive(Debug)]
pub enum GuiOption {
    AribicShape(bool),
    AmbiWidth(String),
    Emoji(bool),
    GuiFont(String),
    GuiFontSet(String),
    GuiFontWide(String),
    LineSpace(u64),
    Pumblend(u64),
    ShowTabLine(u64),
    TermGuiColors(bool),
    Unknown(String, Value)
}

#[derive(Debug)]
pub enum RedrawEvent {
    SetTitle { title: String },
    ModeInfoSet { cursor_modes: Vec<CursorMode> },
    OptionSet { gui_option: GuiOption },
    ModeChange { mode_index: u64 },
    BusyStart,
    BusyStop,
    Flush,
    Resize { grid: u64, width: u64, height: u64 },
    DefaultColorsSet { colors: Colors },
    HighlightAttributesDefine { id: u64, style: Style },
    GridLine { grid: u64, row: u64, column_start: u64, cells: Vec<GridLineCell> },
    Clear { grid: u64 },
    CursorGoto { grid: u64, row: u64, column: u64 },
    Scroll { grid: u64, top: u64, bottom: u64, left: u64, right: u64, rows: i64, columns: i64 },
    CommandLineShow { content: StyledContent, position: u64, first_character: String, prompt: String, indent: u64, level: u64 },
    CommandLinePosition { position: u64, level: u64 },
    CommandLineSpecialCharacter { character: String, shift: bool, level: u64 },
    CommandLineHide,
    CommandLineBlockShow { lines: Vec<StyledContent> },
    CommandLineBlockAppend { line: StyledContent },
    CommandLineBlockHide,
    MessageShow { kind: MessageKind, content: StyledContent, replace_last: bool },
    MessageClear,
    MessageShowMode { content: StyledContent },
    MessageShowCommand { content: StyledContent },
    MessageRuler { content: StyledContent },
    MessageHistoryShow { entries: Vec<(MessageKind, StyledContent)>}
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

fn parse_map(map_value: &Value) -> Result<Vec<(Value, Value)>> {
    if let Value::Map(content) = map_value.clone() {
        Ok(content)
    } else {
        Err(EventParseError::InvalidMap(map_value.clone()))
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

fn parse_bool(bool_value: &Value) -> Result<bool> {
    if let Value::Boolean(content) = bool_value.clone() {
        Ok(content)
    } else {
        Err(EventParseError::InvalidBool(bool_value.clone()))
    }
}

fn parse_set_title(set_title_arguments: Vec<Value>) -> Result<RedrawEvent> {
    if let [title] = set_title_arguments.as_slice() {
        Ok(RedrawEvent::SetTitle {
            title: parse_string(title)?
        })
    } else {
        Err(EventParseError::InvalidEventFormat)
    }
}

fn parse_mode_info_set(mode_info_set_arguments: Vec<Value>) -> Result<RedrawEvent> {
    if let [_cursor_style_enabled, mode_info] = mode_info_set_arguments.as_slice() {
        let mode_info_values = parse_array(mode_info)?;
        let mut cursor_modes = Vec::new();
        for mode_info_value in mode_info_values {
            let info_map = parse_map(&mode_info_value)?;
            let mut mode_info = CursorMode::new();
            for (name, value) in info_map {
                let name = parse_string(&name)?;
                match name.as_ref() {
                    "cursor_shape" => {
                        mode_info.shape = CursorShape::from_type_name(&parse_string(&value)?);
                    },
                    "cell_percentage" => {
                        mode_info.cell_percentage = Some(parse_u64(&value)? as f32 / 100.0);
                    },
                    "blinkwait" => {
                        mode_info.blinkwait = Some(parse_u64(&value)?);
                    },
                    "blinkon" => {
                        mode_info.blinkon = Some(parse_u64(&value)?);
                    },
                    "blinkoff" => {
                        mode_info.blinkoff = Some(parse_u64(&value)?);
                    }
                    "attr_id" => {
                        mode_info.style_id = Some(parse_u64(&value)?);
                    },
                    _ => {}
                }
            }
            cursor_modes.push(mode_info);
        }
        Ok(RedrawEvent::ModeInfoSet {
            cursor_modes
        })
    } else {
        Err(EventParseError::InvalidEventFormat)
    }
}

fn parse_option_set(option_set_arguments: Vec<Value>) -> Result<RedrawEvent> {
    if let [name, value] = option_set_arguments.as_slice() {
        Ok(RedrawEvent::OptionSet {
            gui_option: match parse_string(&name)?.as_ref() {
                "arabicshape" => GuiOption::AribicShape(parse_bool(&value)?),
                "ambiwidth" => GuiOption::AmbiWidth(parse_string(&value)?),
                "emoji" => GuiOption::Emoji(parse_bool(&value)?),
                "guifont" => GuiOption::GuiFont(parse_string(&value)?),
                "guifontset" => GuiOption::GuiFontSet(parse_string(&value)?),
                "guifontwide" => GuiOption::GuiFontWide(parse_string(&value)?),
                "linespace" => GuiOption::LineSpace(parse_u64(&value)?),
                "pumblend" => GuiOption::Pumblend(parse_u64(&value)?),
                "showtabline" => GuiOption::ShowTabLine(parse_u64(&value)?),
                "termguicolors" => GuiOption::TermGuiColors(parse_bool(&value)?),
                unknown_option => GuiOption::Unknown(unknown_option.to_string(), value.clone())
            }
        })
    } else {
        Err(EventParseError::InvalidEventFormat)
    }
}

fn parse_mode_change(mode_change_arguments: Vec<Value>) -> Result<RedrawEvent> {
    if let [_mode, mode_index] = mode_change_arguments.as_slice() {
        Ok(RedrawEvent::ModeChange {
            mode_index: parse_u64(&mode_index)?
        })
    } else {
        Err(EventParseError::InvalidEventFormat)
    }
}

fn parse_grid_resize(grid_resize_arguments: Vec<Value>) -> Result<RedrawEvent> {
    if let [grid_id, width, height] = grid_resize_arguments.as_slice() {
        Ok(RedrawEvent::Resize { 
            grid: parse_u64(&grid_id)?, width: parse_u64(&width)?, height: parse_u64(&height)?
        })
    } else {
        Err(EventParseError::InvalidEventFormat)
    }
}

fn parse_default_colors(default_colors_arguments: Vec<Value>) -> Result<RedrawEvent> {
    if let [
        foreground, background, special, _term_foreground, _term_background
    ] = default_colors_arguments.as_slice() {
        Ok(RedrawEvent::DefaultColorsSet {
            colors: Colors {
                foreground: Some(unpack_color(parse_u64(&foreground)?)),
                background: Some(unpack_color(parse_u64(&background)?)),
                special: Some(unpack_color(parse_u64(special)?)),
            }
        })
    } else {
        Err(EventParseError::InvalidEventFormat)
    }
}

fn parse_style(style_map: &Value) -> Result<Style> {
    if let Value::Map(attributes) = style_map {
        let mut style = Style::new(Colors::new(None, None, None));
        for attribute in attributes {
            if let (Value::String(name), value) = attribute {
                match (name.as_str().unwrap(), value) {
                    ("foreground", Value::Integer(packed_color)) => style.colors.foreground = Some(unpack_color(packed_color.as_u64().unwrap())),
                    ("background", Value::Integer(packed_color)) => style.colors.background = Some(unpack_color(packed_color.as_u64().unwrap())),
                    ("special", Value::Integer(packed_color)) => style.colors.special = Some(unpack_color(packed_color.as_u64().unwrap())),
                    ("reverse", Value::Boolean(reverse)) => style.reverse = *reverse,
                    ("italic", Value::Boolean(italic)) => style.italic = *italic,
                    ("bold", Value::Boolean(bold)) => style.bold = *bold,
                    ("strikethrough", Value::Boolean(strikethrough)) => style.strikethrough = *strikethrough,
                    ("underline", Value::Boolean(underline)) => style.underline = *underline,
                    ("undercurl", Value::Boolean(undercurl)) => style.undercurl = *undercurl,
                    ("blend", Value::Integer(blend)) => style.blend = blend.as_u64().unwrap() as u8,
                    _ => println!("Ignored style attribute: {}", name)
                }
            } else {
                println!("Invalid attribute format");
            }
        }
        Ok(style)
    } else {
        Err(EventParseError::InvalidMap(style_map.clone()))
    }
}

fn parse_hl_attr_define(hl_attr_define_arguments: Vec<Value>) -> Result<RedrawEvent> {
    if let [
        id, attributes, _terminal_attributes, _info
    ] = hl_attr_define_arguments.as_slice() {
        let style = parse_style(attributes)?;
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

fn parse_styled_content(line: &Value) -> Result<StyledContent> {
    parse_array(line)?.iter().map(|tuple| {
        if let [style_id, text] = parse_array(tuple)?.as_slice() {
            Ok((parse_u64(style_id)?, parse_string(text)?))
        } else {
            Err(EventParseError::InvalidEventFormat)
        }
    }).collect::<Result<StyledContent>>()
}

fn parse_cmdline_show(cmdline_show_arguments: Vec<Value>) -> Result<RedrawEvent> {
    if let [content, position, first_character, prompt, indent, level] = cmdline_show_arguments.as_slice() {
        Ok(RedrawEvent::CommandLineShow {
            content: parse_styled_content(&content)?,
            position: parse_u64(&position)?,
            first_character: parse_string(&first_character)?,
            prompt: parse_string(&prompt)?,
            indent: parse_u64(&indent)?,
            level: parse_u64(&level)?
        })
    } else {
        Err(EventParseError::InvalidEventFormat)
    }
}

fn parse_cmdline_pos(cmdline_pos_arguments: Vec<Value>) -> Result<RedrawEvent> {
    if let [position, level] = cmdline_pos_arguments.as_slice() {
        Ok(RedrawEvent::CommandLinePosition {
            position: parse_u64(&position)?,
            level: parse_u64(&level)?
        })
    } else {
        Err(EventParseError::InvalidEventFormat)
    }
}

fn parse_cmdline_special_char(cmdline_special_char_arguments: Vec<Value>) -> Result<RedrawEvent> {
    if let [character, shift, level] = cmdline_special_char_arguments.as_slice() {
        Ok(RedrawEvent::CommandLineSpecialCharacter {
            character: parse_string(&character)?,
            shift: parse_bool(&shift)?,
            level: parse_u64(&level)?
        })
    } else {
        Err(EventParseError::InvalidEventFormat)
    }
}

fn parse_cmdline_block_show(cmdline_block_show_arguments: Vec<Value>) -> Result<RedrawEvent> {
    if let [lines] = cmdline_block_show_arguments.as_slice() {
        Ok(RedrawEvent::CommandLineBlockShow {
            lines: parse_array(lines)?
                .iter()
                .map(parse_styled_content)
                .collect::<Result<Vec<StyledContent>>>()?
        })
    } else {
        Err(EventParseError::InvalidEventFormat)
    }
}

fn parse_cmdline_block_append(cmdline_block_append_arguments: Vec<Value>) -> Result<RedrawEvent> {
    if let [line] = cmdline_block_append_arguments.as_slice() {
        Ok(RedrawEvent::CommandLineBlockAppend {
            line: parse_styled_content(line)?
        })
    } else {
        Err(EventParseError::InvalidEventFormat)
    }
}

fn parse_msg_show(msg_show_arguments: Vec<Value>) -> Result<RedrawEvent> {
    if let [kind, content, replace_last] = msg_show_arguments.as_slice() {
        Ok(RedrawEvent::MessageShow {
            kind: MessageKind::parse(&parse_string(&kind)?),
            content: parse_styled_content(&content)?,
            replace_last: parse_bool(&replace_last)?
        })
    } else {
        Err(EventParseError::InvalidEventFormat)
    }
}

fn parse_msg_showmode(msg_showmode_arguments: Vec<Value>) -> Result<RedrawEvent> {
    if let [content] = msg_showmode_arguments.as_slice() {
        Ok(RedrawEvent::MessageShowMode {
            content: parse_styled_content(&content)?,
        })
    } else {
        Err(EventParseError::InvalidEventFormat)
    }
}

fn parse_msg_showcmd(msg_showcmd_arguments: Vec<Value>) -> Result<RedrawEvent> {
    if let [content] = msg_showcmd_arguments.as_slice() {
        Ok(RedrawEvent::MessageShowCommand {
            content: parse_styled_content(&content)?,
        })
    } else {
        Err(EventParseError::InvalidEventFormat)
    }
}

fn parse_msg_ruler(msg_ruler_arguments: Vec<Value>) -> Result<RedrawEvent> {
    if let [content] = msg_ruler_arguments.as_slice() {
        Ok(RedrawEvent::MessageRuler {
            content: parse_styled_content(&content)?,
        })
    } else {
        Err(EventParseError::InvalidEventFormat)
    }
}

fn parse_msg_history_entry(entry: &Value) -> Result<(MessageKind, StyledContent)> {
    if let [kind, content] = parse_array(entry)?.as_slice() {
        Ok((
            MessageKind::parse(&parse_string(kind)?),
            parse_styled_content(content)?
        ))
    } else {
        Err(EventParseError::InvalidEventFormat)
    }
}

fn parse_msg_history_show(msg_history_show_arguments: Vec<Value>) -> Result<RedrawEvent> {
    if let [entries] = msg_history_show_arguments.as_slice() {
        Ok(RedrawEvent::MessageHistoryShow {
            entries: parse_array(entries)?
                .iter()
                .map(parse_msg_history_entry)
                .collect::<Result<Vec<(MessageKind, StyledContent)>>>()?
        })
    } else {
        Err(EventParseError::InvalidEventFormat)
    }
}

pub fn parse_redraw_event(event_value: Value) -> Result<Vec<RedrawEvent>> {
    let event_contents = parse_array(&event_value)?.to_vec();
    let name_value = event_contents.get(0).ok_or(EventParseError::InvalidEventFormat)?;
    let event_name = parse_string(&name_value)?;
    let events = event_contents;
    let mut parsed_events = Vec::new();

    for event in &events[1..] {
        let event_parameters = parse_array(&event)?;
        let possible_parsed_event = match event_name.clone().as_ref() {
            "set_title" => Some(parse_set_title(event_parameters)?),
            "set_icon" => None, // Ignore set icon for now
            "mode_info_set" => Some(parse_mode_info_set(event_parameters)?),
            "option_set" => Some(parse_option_set(event_parameters)?),
            "mode_change" => Some(parse_mode_change(event_parameters)?),
            "busy_start" => Some(RedrawEvent::BusyStart),
            "busy_stop" => Some(RedrawEvent::BusyStop),
            "flush" => Some(RedrawEvent::Flush),
            "grid_resize" => Some(parse_grid_resize(event_parameters)?),
            "default_colors_set" => Some(parse_default_colors(event_parameters)?),
            "hl_attr_define" => Some(parse_hl_attr_define(event_parameters)?),
            "grid_line" => Some(parse_grid_line(event_parameters)?),
            "grid_clear" => Some(parse_clear(event_parameters)?),
            "grid_cursor_goto" => Some(parse_cursor_goto(event_parameters)?),
            "grid_scroll" => Some(parse_grid_scroll(event_parameters)?),
            "cmdline_show" => Some(parse_cmdline_show(event_parameters)?),
            "cmdline_pos" => Some(parse_cmdline_pos(event_parameters)?),
            "cmdline_special_char" => Some(parse_cmdline_special_char(event_parameters)?),
            "cmdline_hide" => Some(RedrawEvent::CommandLineHide),
            "cmdline_block_show" => Some(parse_cmdline_block_show(event_parameters)?),
            "cmdline_block_append" => Some(parse_cmdline_block_append(event_parameters)?),
            "cmdline_block_hide" => Some(RedrawEvent::CommandLineBlockHide),
            "msg_show" => Some(parse_msg_show(event_parameters)?),
            "msg_clear" => Some(RedrawEvent::MessageClear),
            "msg_showmode" => Some(parse_msg_showmode(event_parameters)?),
            "msg_showcmd" => Some(parse_msg_showcmd(event_parameters)?),
            "msg_ruler" => Some(parse_msg_ruler(event_parameters)?),
            "msg_history_show" => Some(parse_msg_history_show(event_parameters)?),
            _ => None
        };

        if let Some(parsed_event) = possible_parsed_event {
            parsed_events.push(parsed_event);
        }
    }

    Ok(parsed_events)
}

pub fn parse_neovim_event(event_name: String, arguments: Vec<Value>) -> Result<Vec<RedrawEvent>> {
    let mut resulting_events = Vec::new();
    if event_name == "redraw" {
        for event in arguments {
            resulting_events.append(&mut parse_redraw_event(event)?);
        }
    } else {
        println!("Unknown global event {}", event_name);
    }
    Ok(resulting_events)
}






