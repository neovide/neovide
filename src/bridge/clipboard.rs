use std::error::Error;

use rmpv::Value;

use clipboard::ClipboardContext;
use clipboard::ClipboardProvider;

pub fn get_remote_clipboard(format: Option<&str>) -> Result<Value, Box<dyn Error>> {
    let mut ctx: ClipboardContext = ClipboardProvider::new()?;
    let clipboard_raw = ctx.get_contents()?.replace('\r', "");

    let lines = if let Some("dos") = format {
        // add \r to lines of current file format is dos
        clipboard_raw.replace('\n', "\r\n")
    } else {
        // else, \r is stripped, leaving only \n
        clipboard_raw
    }
    .split('\n')
    .map(Value::from)
    .collect::<Vec<Value>>();

    let lines = Value::from(lines);
    // v paste is normal paste (everything in lines is pasted)
    // V paste is paste with extra endline (line paste)
    // If you want V paste, copy text with extra endline
    let paste_mode = Value::from("v");

    // returns [content: [String], paste_mode: v or V]
    Ok(Value::from(vec![lines, paste_mode]))
}

pub fn set_remote_clipboard(arguments: Vec<Value>) -> Result<(), Box<dyn Error>> {
    if arguments.len() != 3 {
        return Err("expected exactly 3 arguments to set_remote_clipboard".into());
    }

    #[cfg(not(windows))]
    let endline = "\n";
    #[cfg(windows)]
    let endline = "\r\n";

    let lines = arguments[0]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str().map(String::from))
                .map(|s| s.replace('\r', "")) // strip \r
                .collect::<Vec<String>>()
                .join(endline)
        })
        .ok_or("can't build string from provided text")?;

    let mut ctx: ClipboardContext = ClipboardProvider::new()?;
    ctx.set_contents(lines)
}
