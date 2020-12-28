use std::convert::TryInto;
use std::error;
use std::fmt;

use rmpv::Value;
use skulpin::skia_safe::Color4f;

use crate::editor::{Colors, CursorMode, CursorShape, Style};

#[derive(Debug, Clone)]
pub enum ParseError {
    InvalidArray(Value),
    InvalidMap(Value),
    InvalidString(Value),
    InvalidU64(Value),
    InvalidI64(Value),
    InvalidF64(Value),
    InvalidBool(Value),
    InvalidWindowAnchor(Value),
    InvalidFormat,
}
type Result<T> = std::result::Result<T, ParseError>;

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ParseError::InvalidArray(value) => write!(f, "invalid array format {}", value),
            ParseError::InvalidMap(value) => write!(f, "invalid map format {}", value),
            ParseError::InvalidString(value) => write!(f, "invalid string format {}", value),
            ParseError::InvalidU64(value) => write!(f, "invalid u64 format {}", value),
            ParseError::InvalidI64(value) => write!(f, "invalid i64 format {}", value),
            ParseError::InvalidF64(value) => write!(f, "invalid f64 format {}", value),
            ParseError::InvalidBool(value) => write!(f, "invalid bool format {}", value),
            ParseError::InvalidWindowAnchor(value) => {
                write!(f, "invalid window anchor format {}", value)
            }
            ParseError::InvalidFormat => write!(f, "invalid event format"),
        }
    }
}

impl error::Error for ParseError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        None
    }
}

#[derive(Debug)]
pub struct GridLineCell {
    pub text: String,
    pub highlight_id: Option<u64>,
    pub repeat: Option<u64>,
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

#[derive(Debug)]
pub enum GuiOption {
    ArabicShape(bool),
    AmbiWidth(String),
    Emoji(bool),
    GuiFont(String),
    GuiFontSet(String),
    GuiFontWide(String),
    LineSpace(u64),
    Pumblend(u64),
    ShowTabLine(u64),
    TermGuiColors(bool),
    Unknown(String, Value),
}

#[derive(Debug)]
pub enum WindowAnchor {
    NorthWest,
    NorthEast,
    SouthWest,
    SouthEast,
}

#[derive(Debug)]
pub enum EditorMode {
    // The set of modes reported will change in new versions of Nvim, for
    // instance more sub-modes and temporary states might be represented as
    // separate modes. (however we can safely do this as these are the main modes)
    // for instance if we are in Terminal mode and even though status-line shows Terminal,
    // we still get one of these as the _editor_ mode
    Normal,
    Insert,
    Visual,
    CmdLine,
    Unknown(String),
}

#[derive(Debug)]
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

#[derive(Debug)]
pub enum ChannelStreamType {
    STDIO,
    STDERR,
    Socket,
    Job,
}

impl Default for ChannelStreamType {
    fn default() -> Self {
        Self::STDIO
    }
}

#[derive(Debug)]
pub enum ChannelMode {
    Bytes,
    Terminal,
    RPC,
}

impl Default for ChannelMode {
    fn default() -> Self {
        Self::Bytes
    }
}

#[derive(Debug, Default)]
pub struct ClientVersion {
    pub major: u64,
    pub minor: Option<u64>,
    pub patch: Option<u64>,
    pub prerelease: Option<String>,
    pub commit: Option<String>,
}

#[derive(Debug)]
pub enum ClientType {
    Remote,
    UI,
    Embedder,
    Host,
    Plugin,
}

impl Default for ClientType {
    fn default() -> Self {
        Self::Remote
    }
}

#[derive(Debug, Default)]
pub struct ClientInfo {
    pub name: String,
    pub version: ClientVersion,
    pub client_type: ClientType,
    // methods
    // attributes
}

#[derive(Debug, Default)]
pub struct ChannelInfo {
    pub id: u64,
    pub stream: ChannelStreamType,
    pub mode: ChannelMode,
    pub pty: Option<String>,
    pub buffer: Option<String>,
    pub client: Option<ClientInfo>,
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

fn extract_values<Arr: AsMut<[Value]>>(values: Vec<Value>, mut arr: Arr) -> Result<Arr> {
    let arr_ref = arr.as_mut();

    if values.len() != arr_ref.len() {
        Err(ParseError::InvalidFormat)
    } else {
        for (i, val) in values.into_iter().enumerate() {
            arr_ref[i] = val;
        }

        Ok(arr)
    }
}

fn parse_array(array_value: Value) -> Result<Vec<Value>> {
    array_value.try_into().map_err(ParseError::InvalidArray)
}

fn parse_map(map_value: Value) -> Result<Vec<(Value, Value)>> {
    map_value.try_into().map_err(ParseError::InvalidMap)
}

fn parse_string(string_value: Value) -> Result<String> {
    string_value.try_into().map_err(ParseError::InvalidString)
}

fn parse_u64(u64_value: Value) -> Result<u64> {
    u64_value.try_into().map_err(ParseError::InvalidU64)
}

fn parse_i64(i64_value: Value) -> Result<i64> {
    i64_value.try_into().map_err(ParseError::InvalidI64)
}

fn parse_f64(f64_value: Value) -> Result<f64> {
    f64_value.try_into().map_err(ParseError::InvalidF64)
}

fn parse_bool(bool_value: Value) -> Result<bool> {
    bool_value.try_into().map_err(ParseError::InvalidBool)
}

fn parse_set_title(set_title_arguments: Vec<Value>) -> Result<RedrawEvent> {
    let [title] = extract_values(set_title_arguments, [Value::Nil])?;

    Ok(RedrawEvent::SetTitle {
        title: parse_string(title)?,
    })
}

fn parse_mode_info_set(mode_info_set_arguments: Vec<Value>) -> Result<RedrawEvent> {
    let [_cursor_style_enabled, mode_info] =
        extract_values(mode_info_set_arguments, [Value::Nil, Value::Nil])?;

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
    let [name, value] = extract_values(option_set_arguments, [Value::Nil, Value::Nil])?;

    let name = parse_string(name)?;

    Ok(RedrawEvent::OptionSet {
        gui_option: match name.as_str() {
            "arabicshape" => GuiOption::ArabicShape(parse_bool(value)?),
            "ambiwidth" => GuiOption::AmbiWidth(parse_string(value)?),
            "emoji" => GuiOption::Emoji(parse_bool(value)?),
            "guifont" => GuiOption::GuiFont(parse_string(value)?),
            "guifontset" => GuiOption::GuiFontSet(parse_string(value)?),
            "guifontwide" => GuiOption::GuiFontWide(parse_string(value)?),
            "linespace" => GuiOption::LineSpace(parse_u64(value)?),
            "pumblend" => GuiOption::Pumblend(parse_u64(value)?),
            "showtabline" => GuiOption::ShowTabLine(parse_u64(value)?),
            "termguicolors" => GuiOption::TermGuiColors(parse_bool(value)?),
            _ => GuiOption::Unknown(name, value),
        },
    })
}

fn parse_mode_change(mode_change_arguments: Vec<Value>) -> Result<RedrawEvent> {
    let [mode, mode_index] = extract_values(mode_change_arguments, [Value::Nil, Value::Nil])?;
    let mode_name = parse_string(mode)?;

    Ok(RedrawEvent::ModeChange {
        mode: match mode_name.as_str() {
            "normal" => EditorMode::Normal,
            "insert" => EditorMode::Insert,
            "visual" => EditorMode::Visual,
            "cmdline_normal" => EditorMode::CmdLine,
            _ => EditorMode::Unknown(mode_name),
        },
        mode_index: parse_u64(mode_index)?,
    })
}

fn parse_grid_resize(grid_resize_arguments: Vec<Value>) -> Result<RedrawEvent> {
    let [grid_id, width, height] =
        extract_values(grid_resize_arguments, [Value::Nil, Value::Nil, Value::Nil])?;

    Ok(RedrawEvent::Resize {
        grid: parse_u64(grid_id)?,
        width: parse_u64(width)?,
        height: parse_u64(height)?,
    })
}

fn parse_default_colors(default_colors_arguments: Vec<Value>) -> Result<RedrawEvent> {
    let values = [Value::Nil, Value::Nil, Value::Nil, Value::Nil, Value::Nil];
    let [foreground, background, special, _term_foreground, _term_background] =
        extract_values(default_colors_arguments, values)?;

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
                ("underline", Value::Boolean(underline)) => style.underline = underline,
                ("undercurl", Value::Boolean(undercurl)) => style.undercurl = undercurl,
                ("blend", Value::Integer(blend)) => style.blend = blend.as_u64().unwrap() as u8,
                _ => println!("Ignored style attribute: {}", name),
            }
        } else {
            println!("Invalid attribute format");
        }
    }

    Ok(style)
}

fn parse_hl_attr_define(hl_attr_define_arguments: Vec<Value>) -> Result<RedrawEvent> {
    let values = [Value::Nil, Value::Nil, Value::Nil, Value::Nil];
    let [id, attributes, _terminal_attributes, _info] =
        extract_values(hl_attr_define_arguments, values)?;

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
        .map(|v| take_value(v))
        .ok_or(ParseError::InvalidFormat)?;

    let highlight_id = cell_contents
        .get_mut(1)
        .map(|v| take_value(v))
        .map(parse_u64)
        .transpose()?;
    let repeat = cell_contents
        .get_mut(2)
        .map(|v| take_value(v))
        .map(parse_u64)
        .transpose()?;

    Ok(GridLineCell {
        text: parse_string(text_value)?,
        highlight_id,
        repeat,
    })
}

fn parse_grid_line(grid_line_arguments: Vec<Value>) -> Result<RedrawEvent> {
    let values = [Value::Nil, Value::Nil, Value::Nil, Value::Nil];
    let [grid_id, row, column_start, cells] = extract_values(grid_line_arguments, values)?;

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
    let [grid_id] = extract_values(grid_clear_arguments, [Value::Nil])?;

    Ok(RedrawEvent::Clear {
        grid: parse_u64(grid_id)?,
    })
}

fn parse_grid_destroy(grid_destroy_arguments: Vec<Value>) -> Result<RedrawEvent> {
    let [grid_id] = extract_values(grid_destroy_arguments, [Value::Nil])?;

    Ok(RedrawEvent::Destroy {
        grid: parse_u64(grid_id)?,
    })
}

fn parse_grid_cursor_goto(cursor_goto_arguments: Vec<Value>) -> Result<RedrawEvent> {
    let [grid_id, row, column] =
        extract_values(cursor_goto_arguments, [Value::Nil, Value::Nil, Value::Nil])?;

    Ok(RedrawEvent::CursorGoto {
        grid: parse_u64(grid_id)?,
        row: parse_u64(row)?,
        column: parse_u64(column)?,
    })
}

fn parse_grid_scroll(grid_scroll_arguments: Vec<Value>) -> Result<RedrawEvent> {
    let values = [
        Value::Nil,
        Value::Nil,
        Value::Nil,
        Value::Nil,
        Value::Nil,
        Value::Nil,
        Value::Nil,
    ];
    let [grid_id, top, bottom, left, right, rows, columns] =
        extract_values(grid_scroll_arguments, values)?;
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
    let values = [
        Value::Nil,
        Value::Nil,
        Value::Nil,
        Value::Nil,
        Value::Nil,
        Value::Nil,
    ];
    let [grid, _window, start_row, start_column, width, height] =
        extract_values(win_pos_arguments, values)?;

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
        _ => Err(ParseError::InvalidWindowAnchor(value_str.into())),
    }
}

fn parse_win_float_pos(win_float_pos_arguments: Vec<Value>) -> Result<RedrawEvent> {
    let values = [
        Value::Nil,
        Value::Nil,
        Value::Nil,
        Value::Nil,
        Value::Nil,
        Value::Nil,
        Value::Nil,
    ];
    let [grid, _window, anchor, anchor_grid, anchor_row, anchor_column, focusable] =
        extract_values(win_float_pos_arguments, values)?;

    Ok(RedrawEvent::WindowFloatPosition {
        grid: parse_u64(grid)?,
        anchor: parse_window_anchor(anchor)?,
        anchor_grid: parse_u64(anchor_grid)?,
        anchor_row: parse_f64(anchor_row)?,
        anchor_column: parse_f64(anchor_column)?,
        focusable: parse_bool(focusable)?,
    })
}

fn parse_win_external_pos(win_external_pos_arguments: Vec<Value>) -> Result<RedrawEvent> {
    let [grid, _window] = extract_values(win_external_pos_arguments, [Value::Nil, Value::Nil])?;

    Ok(RedrawEvent::WindowExternalPosition {
        grid: parse_u64(grid)?,
    })
}

fn parse_win_hide(win_hide_arguments: Vec<Value>) -> Result<RedrawEvent> {
    let [grid] = extract_values(win_hide_arguments, [Value::Nil])?;

    Ok(RedrawEvent::WindowHide {
        grid: parse_u64(grid)?,
    })
}

fn parse_win_close(win_close_arguments: Vec<Value>) -> Result<RedrawEvent> {
    let [grid] = extract_values(win_close_arguments, [Value::Nil])?;

    Ok(RedrawEvent::WindowClose {
        grid: parse_u64(grid)?,
    })
}

fn parse_msg_set_pos(msg_set_pos_arguments: Vec<Value>) -> Result<RedrawEvent> {
    let values = [Value::Nil, Value::Nil, Value::Nil, Value::Nil];
    let [grid, row, scrolled, separator_character] = extract_values(msg_set_pos_arguments, values)?;

    Ok(RedrawEvent::MessageSetPosition {
        grid: parse_u64(grid)?,
        row: parse_u64(row)?,
        scrolled: parse_bool(scrolled)?,
        separator_character: parse_string(separator_character)?,
    })
}

fn parse_win_viewport(win_viewport_arguments: Vec<Value>) -> Result<RedrawEvent> {
    let values = [
        Value::Nil,
        Value::Nil,
        Value::Nil,
        Value::Nil,
        Value::Nil,
        Value::Nil,
    ];
    let [grid, _window, top_line, bottom_line, current_line, current_column] =
        extract_values(win_viewport_arguments, values)?;

    Ok(RedrawEvent::WindowViewport {
        grid: parse_u64(grid)?,
        top_line: parse_f64(top_line)?,
        bottom_line: parse_f64(bottom_line)?,
        current_line: parse_f64(current_line)?,
        current_column: parse_f64(current_column)?,
    })
}

fn parse_styled_content(line: Value) -> Result<StyledContent> {
    parse_array(line)?
        .into_iter()
        .map(|tuple| {
            let [style_id, text] = extract_values(parse_array(tuple)?, [Value::Nil, Value::Nil])?;

            Ok((parse_u64(style_id)?, parse_string(text)?))
        })
        .collect()
}

fn parse_cmdline_show(cmdline_show_arguments: Vec<Value>) -> Result<RedrawEvent> {
    let values = [
        Value::Nil,
        Value::Nil,
        Value::Nil,
        Value::Nil,
        Value::Nil,
        Value::Nil,
    ];
    let [content, position, first_character, prompt, indent, level] =
        extract_values(cmdline_show_arguments, values)?;

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
    let [position, level] = extract_values(cmdline_pos_arguments, [Value::Nil, Value::Nil])?;

    Ok(RedrawEvent::CommandLinePosition {
        position: parse_u64(position)?,
        level: parse_u64(level)?,
    })
}

fn parse_cmdline_special_char(cmdline_special_char_arguments: Vec<Value>) -> Result<RedrawEvent> {
    let [character, shift, level] = extract_values(
        cmdline_special_char_arguments,
        [Value::Nil, Value::Nil, Value::Nil],
    )?;

    Ok(RedrawEvent::CommandLineSpecialCharacter {
        character: parse_string(character)?,
        shift: parse_bool(shift)?,
        level: parse_u64(level)?,
    })
}

fn parse_cmdline_block_show(cmdline_block_show_arguments: Vec<Value>) -> Result<RedrawEvent> {
    let [lines] = extract_values(cmdline_block_show_arguments, [Value::Nil])?;

    Ok(RedrawEvent::CommandLineBlockShow {
        lines: parse_array(lines)?
            .into_iter()
            .map(parse_styled_content)
            .collect::<Result<_>>()?,
    })
}

fn parse_cmdline_block_append(cmdline_block_append_arguments: Vec<Value>) -> Result<RedrawEvent> {
    let [line] = extract_values(cmdline_block_append_arguments, [Value::Nil])?;

    Ok(RedrawEvent::CommandLineBlockAppend {
        line: parse_styled_content(line)?,
    })
}

fn parse_msg_show(msg_show_arguments: Vec<Value>) -> Result<RedrawEvent> {
    let [kind, content, replace_last] =
        extract_values(msg_show_arguments, [Value::Nil, Value::Nil, Value::Nil])?;

    Ok(RedrawEvent::MessageShow {
        kind: MessageKind::parse(&parse_string(kind)?),
        content: parse_styled_content(content)?,
        replace_last: parse_bool(replace_last)?,
    })
}

fn parse_msg_showmode(msg_showmode_arguments: Vec<Value>) -> Result<RedrawEvent> {
    let [content] = extract_values(msg_showmode_arguments, [Value::Nil])?;

    Ok(RedrawEvent::MessageShowMode {
        content: parse_styled_content(content)?,
    })
}

fn parse_msg_showcmd(msg_showcmd_arguments: Vec<Value>) -> Result<RedrawEvent> {
    let [content] = extract_values(msg_showcmd_arguments, [Value::Nil])?;

    Ok(RedrawEvent::MessageShowCommand {
        content: parse_styled_content(content)?,
    })
}

fn parse_msg_ruler(msg_ruler_arguments: Vec<Value>) -> Result<RedrawEvent> {
    let [content] = extract_values(msg_ruler_arguments, [Value::Nil])?;

    Ok(RedrawEvent::MessageRuler {
        content: parse_styled_content(content)?,
    })
}

fn parse_msg_history_entry(entry: Value) -> Result<(MessageKind, StyledContent)> {
    let [kind, content] = extract_values(parse_array(entry)?, [Value::Nil, Value::Nil])?;

    Ok((
        MessageKind::parse(&parse_string(kind)?),
        parse_styled_content(content)?,
    ))
}

fn parse_msg_history_show(msg_history_show_arguments: Vec<Value>) -> Result<RedrawEvent> {
    let [entries] = extract_values(msg_history_show_arguments, [Value::Nil])?;

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
        .ok_or(ParseError::InvalidFormat)
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

pub fn parse_channel_stream_type(channel_stream_value: Value) -> Result<ChannelStreamType> {
    match parse_string(channel_stream_value)?.as_ref() {
        "stdio" => Ok(ChannelStreamType::STDIO),
        "stderr" => Ok(ChannelStreamType::STDERR),
        "socket" => Ok(ChannelStreamType::Socket),
        "job" => Ok(ChannelStreamType::Job),
        _ => Err(ParseError::InvalidFormat),
    }
}

pub fn parse_channel_mode(channel_mode_value: Value) -> Result<ChannelMode> {
    match parse_string(channel_mode_value)?.as_ref() {
        "bytes" => Ok(ChannelMode::Bytes),
        "terminal" => Ok(ChannelMode::Terminal),
        "rpc" => Ok(ChannelMode::RPC),
        _ => Err(ParseError::InvalidFormat),
    }
}

pub fn parse_client_version(version_value: Value) -> Result<ClientVersion> {
    let version_map = parse_map(version_value)?;

    let mut version = ClientVersion::default();

    for version_property in version_map {
        if let (Value::String(name), value) = version_property {
            match (name.as_str().unwrap(), value) {
                ("major", major) => version.major = parse_u64(major)?,
                ("minor", minor) => version.minor = Some(parse_u64(minor)?),
                ("patch", patch) => version.patch = Some(parse_u64(patch)?),
                ("prerelease", prerelease) => version.prerelease = Some(parse_string(prerelease)?),
                ("commit", commit) => version.commit = Some(parse_string(commit)?),
                _ => println!("Ignored client version property: {}", name),
            }
        } else {
            println!("Invalid client version format");
        }
    }

    Ok(version)
}

pub fn parse_client_type(client_type_value: Value) -> Result<ClientType> {
    match parse_string(client_type_value)?.as_ref() {
        "remote" => Ok(ClientType::Remote),
        "ui" => Ok(ClientType::UI),
        "embedder" => Ok(ClientType::Embedder),
        "host" => Ok(ClientType::Host),
        "plugin" => Ok(ClientType::Plugin),
        _ => Err(ParseError::InvalidFormat),
    }
}

pub fn parse_client_info(client_info_value: Value) -> Result<ClientInfo> {
    let client_info_map = parse_map(client_info_value)?;

    let mut client_info = ClientInfo::default();

    for info_property in client_info_map {
        if let (Value::String(name), value) = info_property {
            match (name.as_str().unwrap(), value) {
                ("name", name) => client_info.name = parse_string(name)?,
                ("version", version) => client_info.version = parse_client_version(version)?,
                ("type", client_type) => client_info.client_type = parse_client_type(client_type)?,
                _ => println!("Ignored client type property: {}", name),
            }
        } else {
            println!("Invalid client info format");
        }
    }

    Ok(client_info)
}

pub fn parse_channel_info(channel_value: Value) -> Result<ChannelInfo> {
    let channel_map = parse_map(channel_value)?;

    let mut channel_info = ChannelInfo::default();

    for channel_property in channel_map {
        if let (Value::String(name), value) = channel_property {
            match (name.as_str().unwrap(), value) {
                ("id", channel_id) => channel_info.id = parse_u64(channel_id)?,
                ("stream", stream) => channel_info.stream = parse_channel_stream_type(stream)?,
                ("mode", mode) => channel_info.mode = parse_channel_mode(mode)?,
                ("pty", pty) => channel_info.pty = Some(parse_string(pty)?),
                ("buffer", buffer) => channel_info.buffer = Some(parse_string(buffer)?),
                ("client", client_info) => {
                    channel_info.client = Some(parse_client_info(client_info)?)
                }
                _ => println!("Ignored channel info property: {}", name),
            }
        } else {
            println!("Invalid channel info format");
        }
    }

    Ok(channel_info)
}

pub fn parse_channel_list(channel_infos: Vec<Value>) -> Result<Vec<ChannelInfo>> {
    channel_infos
        .into_iter()
        .map(parse_channel_info)
        .collect::<Result<Vec<ChannelInfo>>>()
}
