use std::error::Error;

use rmpv::Value;

use crate::clipboard;

pub fn get_clipboard_contents(format: Option<&str>) -> Result<Value, Box<dyn Error + Send + Sync>> {
    let clipboard_raw = clipboard::get_contents()?.replace('\r', "");
    let is_line_paste = clipboard_raw.ends_with('\n');

    let lines = if let Some("dos") = format {
        // Add \r to lines if current file format is dos.
        clipboard_raw.replace('\n', "\r\n")
    } else {
        // Else, \r is stripped, leaving only \n.
        clipboard_raw
    }
    .split('\n')
    .map(Value::from)
    .collect::<Vec<Value>>();

    let lines = Value::from(lines);
    // v paste is normal paste (everything in lines is pasted)
    // V paste is paste with extra endline (line paste)
    // If you want V paste, copy text with extra endline.
    let paste_mode = Value::from(if is_line_paste { "V" } else { "v" });

    // Return [content: [String], paste_mode: v or V]
    Ok(Value::from(vec![lines, paste_mode]))
}

pub fn set_clipboard_contents(value: &Value) -> Result<Value, Box<dyn Error + Send + Sync>> {
    #[cfg(not(windows))]
    let endline = "\n";
    #[cfg(windows)]
    let endline = "\r\n";

    let lines = value
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str().map(String::from))
                .map(|s| s.replace('\r', "")) // strip \r
                .collect::<Vec<String>>()
                .join(endline)
        })
        .ok_or("can't build string from provided text")?;

    clipboard::set_contents(lines)?;

    Ok(Value::Nil)
}
