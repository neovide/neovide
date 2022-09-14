use std::error::Error;

use rmpv::Value;

use crate::{
    clipboard::{Clipboard, ClipboardCommand},
    event_aggregator::EVENT_AGGREGATOR,
};

pub fn get_clipboard_contents(format: Option<&str>) -> Result<Value, Box<dyn Error + Send + Sync>> {
    let mut clipboard: Clipboard = Clipboard::new()?;
    let clipboard_raw = clipboard.get_contents()?.replace('\r', "");

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

    // The X11 clipboard doesn't work correctly if we set the clipboard directly here, presumably
    // because the thread handling this request is rather short-lived. Instead, we delegate this
    // task to the clipboard command handler, which is run by a long-running thread.
    EVENT_AGGREGATOR.send(ClipboardCommand::SetContents(lines));

    Ok(Value::Nil)
}
