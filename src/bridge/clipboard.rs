use rmpv::Value;

use crate::clipboard::Clipboard;

pub fn get_clipboard_contents(
    clipboard: &mut Clipboard,
    register: &Value,
) -> Result<Value, String> {
    let register = register.as_str().unwrap_or("+");
    let clipboard_raw =
        clipboard.get_contents(register).map_err(|error| error.to_string())?.replace('\r', "");
    let is_line_paste = clipboard_raw.ends_with('\n');

    let lines = clipboard_raw.split('\n').map(Value::from).collect::<Vec<Value>>();

    let lines = Value::from(lines);
    // v paste is normal paste (everything in lines is pasted)
    // V paste is paste with extra endline (line paste)
    // If you want V paste, copy text with extra endline.
    let paste_mode = Value::from(if is_line_paste { "V" } else { "v" });

    // Return [content: [String], paste_mode: v or V]
    Ok(Value::from(vec![lines, paste_mode]))
}

pub fn set_clipboard_contents(
    clipboard: &mut Clipboard,
    value: &Value,
    register: &Value,
) -> Result<Value, String> {
    #[cfg(not(windows))]
    let endline = "\n";
    #[cfg(windows)]
    let endline = "\r\n";
    let register = register.as_str().unwrap_or("+");

    let lines = value
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str().map(String::from))
                .map(|s| s.replace('\r', "")) // strip \r
                .collect::<Vec<String>>()
                .join(endline)
        })
        .ok_or_else(|| "can't build string from provided text".to_string())?;

    clipboard.set_contents(lines, register).map_err(|error| error.to_string())?;

    Ok(Value::Nil)
}
