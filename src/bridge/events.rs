use std::{
    convert::TryInto,
    error,
    fmt::{self, Debug},
};

use log::debug;
use rmpv::Value;
use skia_safe::Color4f;

use crate::editor::{Colors, CursorMode, CursorShape, Style, UnderlineStyle};

#[derive(Clone, Debug)]
pub enum ParseError {
    Array(Value),
    Map(Value),
    String(Value),
    U64(Value),
    I64(Value),
    F64(Value),
    Bool(Value),
    WindowAnchor(Value),
    Format(String),
}
type Result<T> = std::result::Result<T, ParseError>;

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ParseError::Array(value) => write!(f, "invalid array format {value}"),
            ParseError::Map(value) => write!(f, "invalid map format {value}"),
            ParseError::String(value) => write!(f, "invalid string format {value}"),
            ParseError::U64(value) => write!(f, "invalid u64 format {value}"),
            ParseError::I64(value) => write!(f, "invalid i64 format {value}"),
            ParseError::F64(value) => write!(f, "invalid f64 format {value}"),
            ParseError::Bool(value) => write!(f, "invalid bool format {value}"),
            ParseError::WindowAnchor(value) => {
                write!(f, "invalid window anchor format {value}")
            }
            ParseError::Format(debug_text) => {
                write!(f, "invalid event format {debug_text}")
            }
        }
    }
}

impl error::Error for ParseError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        None
    }
}

#[derive(Clone, Debug)]
pub struct GridLineCell {
    pub text: String,
    pub highlight_id: Option<u64>,
    pub repeat: Option<u64>,
}

pub type StyledContent = Vec<(u64, String)>;

#[derive(Clone, Debug)]
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
    Warning,
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
            _ => MessageKind::Unknown,
        }
    }
}

#[derive(Clone, Debug)]
pub enum GuiOption {
    ArabicShape(bool),
    AmbiWidth(String),
    Emoji(bool),
    GuiFont(String),
    GuiFontSet(String),
    GuiFontWide(String),
    LineSpace(i64),
    Pumblend(u64),
    ShowTabLine(u64),
    TermGuiColors(bool),
    Unknown(String, Value),
}

#[derive(Clone, Debug)]
pub enum WindowAnchor {
    NorthWest,
    NorthEast,
    SouthWest,
    SouthEast,
}

#[derive(Clone, Debug)]
pub enum EditorMode {
    // The set of modes reported will change in new versions of Nvim, for
    // instance more sub-modes and temporary states might be represented as
    // separate modes. (however we can safely do this as these are the main modes)
    // for instance if we are in Terminal mode and even though status-line shows Terminal,
    // we still get one of these as the _editor_ mode.
    Normal,
    Insert,
    Visual,
    Replace,
    CmdLine,
    Unknown(String),
}

#[derive(Clone, Debug)]
pub enum RedrawEvent {
    SetTitle {
        title: String,
    },
    ModeInfoSet {
        cursor_modes: Vec<CursorMode>,
    },
    OptionSet {
        gui_option: GuiOption,
    },
    ModeChange {
        mode: EditorMode,
        mode_index: u64,
    },
    MouseOn,
    MouseOff,
    BusyStart,
    BusyStop,
    Flush,
    Resize {
        grid: u64,
        width: u64,
        height: u64,
    },
    DefaultColorsSet {
        colors: Colors,
    },
    HighlightAttributesDefine {
        id: u64,
        style: Style,
    },
    GridLine {
        grid: u64,
        row: u64,
        column_start: u64,
        cells: Vec<GridLineCell>,
    },
    Clear {
        grid: u64,
    },
    Destroy {
        grid: u64,
    },
    CursorGoto {
        grid: u64,
        row: u64,
        column: u64,
    },
    Scroll {
        grid: u64,
        top: u64,
        bottom: u64,
        left: u64,
        right: u64,
        rows: i64,
        columns: i64,
    },
    WindowPosition {
        grid: u64,
        start_row: u64,
        start_column: u64,
        width: u64,
        height: u64,
    },
    WindowFloatPosition {
        grid: u64,
        anchor: WindowAnchor,
        anchor_grid: u64,
        anchor_row: f64,
        anchor_column: f64,
        focusable: bool,
        sort_order: Option<u64>,
    },
    WindowExternalPosition {
        grid: u64,
    },
    WindowHide {
        grid: u64,
    },
    WindowClose {
        grid: u64,
    },
    MessageSetPosition {
        grid: u64,
        row: u64,
        scrolled: bool,
        separator_character: String,
    },
    WindowViewport {
        grid: u64,
        top_line: f64,
        bottom_line: f64,
        current_line: f64,
        current_column: f64,
        line_count: Option<f64>,
        scroll_delta: Option<f64>,
    },
    CommandLineShow {
        content: StyledContent,
        position: u64,
        first_character: String,
        prompt: String,
        indent: u64,
        level: u64,
    },
    CommandLinePosition {
        position: u64,
        level: u64,
    },
    CommandLineSpecialCharacter {
        character: String,
        shift: bool,
        level: u64,
    },
    CommandLineHide,
    CommandLineBlockShow {
        lines: Vec<StyledContent>,
    },
    CommandLineBlockAppend {
        line: StyledContent,
    },
    CommandLineBlockHide,
    MessageShow {
        kind: MessageKind,
        content: StyledContent,
        replace_last: bool,
    },
    MessageClear,
    MessageShowMode {
        content: StyledContent,
    },
    MessageShowCommand {
        content: StyledContent,
    },
    MessageRuler {
        content: StyledContent,
    },
    MessageHistoryShow {
        entries: Vec<(MessageKind, StyledContent)>,
    },
}

fn unpack_color(packed_color: u64) -> Color4f {
    let packed_color = packed_color as u32;
    let r = ((packed_color & 0x00ff_0000) >> 16) as f32;
    let g = ((packed_color & 0xff00) >> 8) as f32;
    let b = (packed_color & 0xff) as f32;
    Color4f {
        r: r / 255.0,
        g: g / 255.0,
        b: b / 255.0,
        a: 1.0,
    }
}

fn extract_values<const REQ: usize>(values: Vec<Value>) -> Result<[Value; REQ]> {
    if REQ > values.len() {
        Err(ParseError::Format(format!("{values:?}")))
    } else {
        let mut required_values = vec![Value::Nil; REQ];

        for (index, value) in values.into_iter().enumerate() {
            if index < REQ {
                required_values[index] = value;
            }
        }

        Ok(required_values.try_into().unwrap())
    }
}

fn extract_values_with_optional<const REQ: usize, const OPT: usize>(
    values: Vec<Value>,
) -> Result<([Value; REQ], [Option<Value>; OPT])> {
    if REQ > values.len() {
        Err(ParseError::Format(format!("{values:?}")))
    } else {
        let mut required_values = vec![Value::Nil; REQ];
        let mut optional_values = vec![None; OPT];

        for (index, value) in values.into_iter().enumerate() {
            if index < REQ {
                required_values[index] = value;
            } else {
                optional_values[index - REQ] = Some(value);
            }
        }

        Ok((
            required_values.try_into().unwrap(),
            optional_values.try_into().unwrap(),
        ))
    }
}

fn parse_array(array_value: Value) -> Result<Vec<Value>> {
    array_value.try_into().map_err(ParseError::Array)
}

fn parse_map(map_value: Value) -> Result<Vec<(Value, Value)>> {
    map_value.try_into().map_err(ParseError::Map)
}

fn parse_string(string_value: Value) -> Result<String> {
    match string_value {
        Value::String(s) => Ok(s.into_str().unwrap_or_else(|| String::from("\u{FFFD}"))),
        _ => Err(ParseError::String(string_value)),
    }
}

fn parse_u64(u64_value: Value) -> Result<u64> {
    u64_value.try_into().map_err(ParseError::U64)
}

fn parse_i64(i64_value: Value) -> Result<i64> {
    i64_value.try_into().map_err(ParseError::I64)
}

fn parse_f64(f64_value: Value) -> Result<f64> {
    f64_value.try_into().map_err(ParseError::F64)
}

fn parse_bool(bool_value: Value) -> Result<bool> {
    bool_value.try_into().map_err(ParseError::Bool)
}

fn parse_set_title(set_title_arguments: Vec<Value>) -> Result<RedrawEvent> {
    let [title] = extract_values(set_title_arguments)?;

    Ok(RedrawEvent::SetTitle {
        title: parse_string(title)?,
    })
}

fn parse_mode_info_set(mode_info_set_arguments: Vec<Value>) -> Result<RedrawEvent> {
    let [_cursor_style_enabled, mode_info] = extract_values(mode_info_set_arguments)?;

    let mode_info_values = parse_array(mode_info)?;
    let mut cursor_modes = Vec::with_capacity(mode_info_values.len());

    for mode_info_value in mode_info_values {
        let info_map = parse_map(mode_info_value)?;
        let mut mode_info = CursorMode::default();

        for (name, value) in info_map {
            match parse_string(name)?.as_str() {
                "cursor_shape" => {
                    mode_info.shape = CursorShape::from_type_name(&parse_string(value)?);
                }
                "cell_percentage" => {
                    mode_info.cell_percentage = Some(parse_u64(value)? as f32 / 100.0);
                }
                "blinkwait" => {
                    mode_info.blinkwait = Some(parse_u64(value)?);
                }
                "blinkon" => {
                    mode_info.blinkon = Some(parse_u64(value)?);
                }
                "blinkoff" => {
                    mode_info.blinkoff = Some(parse_u64(value)?);
                }
                "attr_id" => {
                    mode_info.style_id = Some(parse_u64(value)?);
                }
                _ => {}
            }
        }

        cursor_modes.push(mode_info);
    }

    Ok(RedrawEvent::ModeInfoSet { cursor_modes })
}

fn parse_option_set(option_set_arguments: Vec<Value>) -> Result<RedrawEvent> {
    let [name, value] = extract_values(option_set_arguments)?;

    let name = parse_string(name)?;

    Ok(RedrawEvent::OptionSet {
        gui_option: match name.as_str() {
            "arabicshape" => GuiOption::ArabicShape(parse_bool(value)?),
            "ambiwidth" => GuiOption::AmbiWidth(parse_string(value)?),
            "emoji" => GuiOption::Emoji(parse_bool(value)?),
            "guifont" => GuiOption::GuiFont(parse_string(value)?),
            "guifontset" => GuiOption::GuiFontSet(parse_string(value)?),
            "guifontwide" => GuiOption::GuiFontWide(parse_string(value)?),
            "linespace" => GuiOption::LineSpace(parse_i64(value)?),
            "pumblend" => GuiOption::Pumblend(parse_u64(value)?),
            "showtabline" => GuiOption::ShowTabLine(parse_u64(value)?),
            "termguicolors" => GuiOption::TermGuiColors(parse_bool(value)?),
            _ => GuiOption::Unknown(name, value),
        },
    })
}

fn parse_mode_change(mode_change_arguments: Vec<Value>) -> Result<RedrawEvent> {
    let [mode, mode_index] = extract_values(mode_change_arguments)?;
    let mode_name = parse_string(mode)?;

    Ok(RedrawEvent::ModeChange {
        mode: match mode_name.as_str() {
            "normal" => EditorMode::Normal,
            "insert" => EditorMode::Insert,
            "visual" => EditorMode::Visual,
            "replace" => EditorMode::Replace,
            "cmdline_normal" => EditorMode::CmdLine,
            _ => EditorMode::Unknown(mode_name),
        },
        mode_index: parse_u64(mode_index)?,
    })
}

fn parse_grid_resize(grid_resize_arguments: Vec<Value>) -> Result<RedrawEvent> {
    let [grid_id, width, height] = extract_values(grid_resize_arguments)?;

    Ok(RedrawEvent::Resize {
        grid: parse_u64(grid_id)?,
        width: parse_u64(width)?,
        height: parse_u64(height)?,
    })
}

fn parse_default_colors(default_colors_arguments: Vec<Value>) -> Result<RedrawEvent> {
    let [foreground, background, special, _term_foreground, _term_background] =
        extract_values(default_colors_arguments)?;

    Ok(RedrawEvent::DefaultColorsSet {
        colors: Colors {
            foreground: Some(unpack_color(parse_u64(foreground)?)),
            background: Some(unpack_color(parse_u64(background)?)),
            special: Some(unpack_color(parse_u64(special)?)),
        },
    })
}

fn parse_style(style_map: Value) -> Result<Style> {
    let attributes = parse_map(style_map)?;

    let mut style = Style::new(Colors::new(None, None, None));

    for attribute in attributes {
        if let (Value::String(name), value) = attribute {
            match (name.as_str().unwrap(), value) {
                ("foreground", Value::Integer(packed_color)) => {
                    style.colors.foreground = Some(unpack_color(packed_color.as_u64().unwrap()))
                }
                ("background", Value::Integer(packed_color)) => {
                    style.colors.background = Some(unpack_color(packed_color.as_u64().unwrap()))
                }
                ("special", Value::Integer(packed_color)) => {
                    style.colors.special = Some(unpack_color(packed_color.as_u64().unwrap()))
                }
                ("reverse", Value::Boolean(reverse)) => style.reverse = reverse,
                ("italic", Value::Boolean(italic)) => style.italic = italic,
                ("bold", Value::Boolean(bold)) => style.bold = bold,
                ("strikethrough", Value::Boolean(strikethrough)) => {
                    style.strikethrough = strikethrough
                }
                ("blend", Value::Integer(blend)) => style.blend = blend.as_u64().unwrap() as u8,

                ("underline", Value::Boolean(true)) => {
                    style.underline = Some(UnderlineStyle::Underline)
                }
                ("undercurl", Value::Boolean(true)) => {
                    style.underline = Some(UnderlineStyle::UnderCurl)
                }
                ("underdotted" | "underdot", Value::Boolean(true)) => {
                    style.underline = Some(UnderlineStyle::UnderDot)
                }
                ("underdashed" | "underdash", Value::Boolean(true)) => {
                    style.underline = Some(UnderlineStyle::UnderDash)
                }
                ("underdouble" | "underlineline", Value::Boolean(true)) => {
                    style.underline = Some(UnderlineStyle::UnderDouble)
                }

                _ => debug!("Ignored style attribute: {}", name),
            }
        } else {
            debug!("Invalid attribute format");
        }
    }

    Ok(style)
}

fn parse_hl_attr_define(hl_attr_define_arguments: Vec<Value>) -> Result<RedrawEvent> {
    let [id, attributes, _terminal_attributes, _info] = extract_values(hl_attr_define_arguments)?;

    let style = parse_style(attributes)?;
    Ok(RedrawEvent::HighlightAttributesDefine {
        id: parse_u64(id)?,
        style,
    })
}

fn parse_grid_line_cell(grid_line_cell: Value) -> Result<GridLineCell> {
    fn take_value(val: &mut Value) -> Value {
        std::mem::replace(val, Value::Nil)
    }

    let mut cell_contents = parse_array(grid_line_cell)?;

    let text_value = cell_contents
        .first_mut()
        .map(take_value)
        .ok_or_else(|| ParseError::Format(format!("{cell_contents:?}")))?;

    let highlight_id = cell_contents
        .get_mut(1)
        .map(take_value)
        .map(parse_u64)
        .transpose()?;
    let repeat = cell_contents
        .get_mut(2)
        .map(take_value)
        .map(parse_u64)
        .transpose()?;

    Ok(GridLineCell {
        text: parse_string(text_value)?,
        highlight_id,
        repeat,
    })
}

fn parse_grid_line(grid_line_arguments: Vec<Value>) -> Result<RedrawEvent> {
    let [grid_id, row, column_start, cells] = extract_values(grid_line_arguments)?;

    Ok(RedrawEvent::GridLine {
        grid: parse_u64(grid_id)?,
        row: parse_u64(row)?,
        column_start: parse_u64(column_start)?,
        cells: parse_array(cells)?
            .into_iter()
            .map(parse_grid_line_cell)
            .collect::<Result<Vec<GridLineCell>>>()?,
    })
}

fn parse_grid_clear(grid_clear_arguments: Vec<Value>) -> Result<RedrawEvent> {
    let [grid_id] = extract_values(grid_clear_arguments)?;

    Ok(RedrawEvent::Clear {
        grid: parse_u64(grid_id)?,
    })
}

fn parse_grid_destroy(grid_destroy_arguments: Vec<Value>) -> Result<RedrawEvent> {
    let [grid_id] = extract_values(grid_destroy_arguments)?;

    Ok(RedrawEvent::Destroy {
        grid: parse_u64(grid_id)?,
    })
}

fn parse_grid_cursor_goto(cursor_goto_arguments: Vec<Value>) -> Result<RedrawEvent> {
    let [grid_id, row, column] = extract_values(cursor_goto_arguments)?;

    Ok(RedrawEvent::CursorGoto {
        grid: parse_u64(grid_id)?,
        row: parse_u64(row)?,
        column: parse_u64(column)?,
    })
}

fn parse_grid_scroll(grid_scroll_arguments: Vec<Value>) -> Result<RedrawEvent> {
    let [grid_id, top, bottom, left, right, rows, columns] = extract_values(grid_scroll_arguments)?;
    Ok(RedrawEvent::Scroll {
        grid: parse_u64(grid_id)?,
        top: parse_u64(top)?,
        bottom: parse_u64(bottom)?,
        left: parse_u64(left)?,
        right: parse_u64(right)?,
        rows: parse_i64(rows)?,
        columns: parse_i64(columns)?,
    })
}

fn parse_win_pos(win_pos_arguments: Vec<Value>) -> Result<RedrawEvent> {
    let [grid, _window, start_row, start_column, width, height] =
        extract_values(win_pos_arguments)?;

    Ok(RedrawEvent::WindowPosition {
        grid: parse_u64(grid)?,
        start_row: parse_u64(start_row)?,
        start_column: parse_u64(start_column)?,
        width: parse_u64(width)?,
        height: parse_u64(height)?,
    })
}

fn parse_window_anchor(value: Value) -> Result<WindowAnchor> {
    let value_str = parse_string(value)?;
    match value_str.as_str() {
        "NW" => Ok(WindowAnchor::NorthWest),
        "NE" => Ok(WindowAnchor::NorthEast),
        "SW" => Ok(WindowAnchor::SouthWest),
        "SE" => Ok(WindowAnchor::SouthEast),
        _ => Err(ParseError::WindowAnchor(value_str.into())),
    }
}

fn parse_win_float_pos(win_float_pos_arguments: Vec<Value>) -> Result<RedrawEvent> {
    let ([grid, _window, anchor, anchor_grid, anchor_row, anchor_column, focusable], [sort_order]) =
        extract_values_with_optional(win_float_pos_arguments)?;

    Ok(RedrawEvent::WindowFloatPosition {
        grid: parse_u64(grid)?,
        anchor: parse_window_anchor(anchor)?,
        anchor_grid: parse_u64(anchor_grid)?,
        anchor_row: parse_f64(anchor_row)?,
        anchor_column: parse_f64(anchor_column)?,
        focusable: parse_bool(focusable)?,
        sort_order: sort_order.map(parse_u64).transpose()?,
    })
}

fn parse_win_external_pos(win_external_pos_arguments: Vec<Value>) -> Result<RedrawEvent> {
    let [grid, _window] = extract_values(win_external_pos_arguments)?;

    Ok(RedrawEvent::WindowExternalPosition {
        grid: parse_u64(grid)?,
    })
}

fn parse_win_hide(win_hide_arguments: Vec<Value>) -> Result<RedrawEvent> {
    let [grid] = extract_values(win_hide_arguments)?;

    Ok(RedrawEvent::WindowHide {
        grid: parse_u64(grid)?,
    })
}

fn parse_win_close(win_close_arguments: Vec<Value>) -> Result<RedrawEvent> {
    let [grid] = extract_values(win_close_arguments)?;

    Ok(RedrawEvent::WindowClose {
        grid: parse_u64(grid)?,
    })
}

fn parse_msg_set_pos(msg_set_pos_arguments: Vec<Value>) -> Result<RedrawEvent> {
    let [grid, row, scrolled, separator_character] = extract_values(msg_set_pos_arguments)?;

    Ok(RedrawEvent::MessageSetPosition {
        grid: parse_u64(grid)?,
        row: parse_u64(row)?,
        scrolled: parse_bool(scrolled)?,
        separator_character: parse_string(separator_character)?,
    })
}

fn parse_win_viewport(win_viewport_arguments: Vec<Value>) -> Result<RedrawEvent> {
    let (
        [grid, _window, top_line, bottom_line, current_line, current_column],
        [line_count, scroll_delta],
    ) = extract_values_with_optional(win_viewport_arguments)?;

    Ok(RedrawEvent::WindowViewport {
        grid: parse_u64(grid)?,
        top_line: parse_f64(top_line)?,
        bottom_line: parse_f64(bottom_line)?,
        current_line: parse_f64(current_line)?,
        current_column: parse_f64(current_column)?,
        line_count: line_count.map(parse_f64).transpose()?,
        scroll_delta: scroll_delta.map(parse_f64).transpose()?,
    })
}

fn parse_styled_content(line: Value) -> Result<StyledContent> {
    parse_array(line)?
        .into_iter()
        .map(|tuple| {
            let [style_id, text] = extract_values(parse_array(tuple)?)?;

            Ok((parse_u64(style_id)?, parse_string(text)?))
        })
        .collect()
}

fn parse_cmdline_show(cmdline_show_arguments: Vec<Value>) -> Result<RedrawEvent> {
    let [content, position, first_character, prompt, indent, level] =
        extract_values(cmdline_show_arguments)?;

    Ok(RedrawEvent::CommandLineShow {
        content: parse_styled_content(content)?,
        position: parse_u64(position)?,
        first_character: parse_string(first_character)?,
        prompt: parse_string(prompt)?,
        indent: parse_u64(indent)?,
        level: parse_u64(level)?,
    })
}

fn parse_cmdline_pos(cmdline_pos_arguments: Vec<Value>) -> Result<RedrawEvent> {
    let [position, level] = extract_values(cmdline_pos_arguments)?;

    Ok(RedrawEvent::CommandLinePosition {
        position: parse_u64(position)?,
        level: parse_u64(level)?,
    })
}

fn parse_cmdline_special_char(cmdline_special_char_arguments: Vec<Value>) -> Result<RedrawEvent> {
    let [character, shift, level] = extract_values(cmdline_special_char_arguments)?;

    Ok(RedrawEvent::CommandLineSpecialCharacter {
        character: parse_string(character)?,
        shift: parse_bool(shift)?,
        level: parse_u64(level)?,
    })
}

fn parse_cmdline_block_show(cmdline_block_show_arguments: Vec<Value>) -> Result<RedrawEvent> {
    let [lines] = extract_values(cmdline_block_show_arguments)?;

    Ok(RedrawEvent::CommandLineBlockShow {
        lines: parse_array(lines)?
            .into_iter()
            .map(parse_styled_content)
            .collect::<Result<_>>()?,
    })
}

fn parse_cmdline_block_append(cmdline_block_append_arguments: Vec<Value>) -> Result<RedrawEvent> {
    let [line] = extract_values(cmdline_block_append_arguments)?;

    Ok(RedrawEvent::CommandLineBlockAppend {
        line: parse_styled_content(line)?,
    })
}

fn parse_msg_show(msg_show_arguments: Vec<Value>) -> Result<RedrawEvent> {
    let [kind, content, replace_last] = extract_values(msg_show_arguments)?;

    Ok(RedrawEvent::MessageShow {
        kind: MessageKind::parse(&parse_string(kind)?),
        content: parse_styled_content(content)?,
        replace_last: parse_bool(replace_last)?,
    })
}

fn parse_msg_showmode(msg_showmode_arguments: Vec<Value>) -> Result<RedrawEvent> {
    let [content] = extract_values(msg_showmode_arguments)?;

    Ok(RedrawEvent::MessageShowMode {
        content: parse_styled_content(content)?,
    })
}

fn parse_msg_showcmd(msg_showcmd_arguments: Vec<Value>) -> Result<RedrawEvent> {
    let [content] = extract_values(msg_showcmd_arguments)?;

    Ok(RedrawEvent::MessageShowCommand {
        content: parse_styled_content(content)?,
    })
}

fn parse_msg_ruler(msg_ruler_arguments: Vec<Value>) -> Result<RedrawEvent> {
    let [content] = extract_values(msg_ruler_arguments)?;

    Ok(RedrawEvent::MessageRuler {
        content: parse_styled_content(content)?,
    })
}

fn parse_msg_history_entry(entry: Value) -> Result<(MessageKind, StyledContent)> {
    let [kind, content] = extract_values(parse_array(entry)?)?;

    Ok((
        MessageKind::parse(&parse_string(kind)?),
        parse_styled_content(content)?,
    ))
}

fn parse_msg_history_show(msg_history_show_arguments: Vec<Value>) -> Result<RedrawEvent> {
    let [entries] = extract_values(msg_history_show_arguments)?;

    Ok(RedrawEvent::MessageHistoryShow {
        entries: parse_array(entries)?
            .into_iter()
            .map(parse_msg_history_entry)
            .collect::<Result<_>>()?,
    })
}

pub fn parse_redraw_event(event_value: Value) -> Result<Vec<RedrawEvent>> {
    let mut event_contents = parse_array(event_value)?.into_iter();
    let event_name = event_contents
        .next()
        .ok_or_else(|| ParseError::Format(format!("{event_contents:?}")))
        .and_then(parse_string)?;

    let events = event_contents;
    let mut parsed_events = Vec::with_capacity(events.len());

    for event in events {
        let event_parameters = parse_array(event)?;
        let possible_parsed_event = match event_name.as_str() {
            "set_title" => Some(parse_set_title(event_parameters)?),
            "set_icon" => None, // Ignore set icon for now
            "mode_info_set" => Some(parse_mode_info_set(event_parameters)?),
            "option_set" => Some(parse_option_set(event_parameters)?),
            "mode_change" => Some(parse_mode_change(event_parameters)?),
            "mouse_on" => Some(RedrawEvent::MouseOn),
            "mouse_off" => Some(RedrawEvent::MouseOff),
            "busy_start" => Some(RedrawEvent::BusyStart),
            "busy_stop" => Some(RedrawEvent::BusyStop),
            "flush" => Some(RedrawEvent::Flush),
            "grid_resize" => Some(parse_grid_resize(event_parameters)?),
            "default_colors_set" => Some(parse_default_colors(event_parameters)?),
            "hl_attr_define" => Some(parse_hl_attr_define(event_parameters)?),
            "grid_line" => Some(parse_grid_line(event_parameters)?),
            "grid_clear" => Some(parse_grid_clear(event_parameters)?),
            "grid_destroy" => Some(parse_grid_destroy(event_parameters)?),
            "grid_cursor_goto" => Some(parse_grid_cursor_goto(event_parameters)?),
            "grid_scroll" => Some(parse_grid_scroll(event_parameters)?),
            "win_pos" => Some(parse_win_pos(event_parameters)?),
            "win_float_pos" => Some(parse_win_float_pos(event_parameters)?),
            "win_external_pos" => Some(parse_win_external_pos(event_parameters)?),
            "win_hide" => Some(parse_win_hide(event_parameters)?),
            "win_close" => Some(parse_win_close(event_parameters)?),
            "msg_set_pos" => Some(parse_msg_set_pos(event_parameters)?),
            "win_viewport" => Some(parse_win_viewport(event_parameters)?),
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
            _ => None,
        };

        if let Some(parsed_event) = possible_parsed_event {
            parsed_events.push(parsed_event);
        }
    }

    Ok(parsed_events)
}
