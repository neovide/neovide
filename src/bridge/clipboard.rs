use std::error::Error;

use rmpv::Value;

use clipboard::ClipboardContext;
use clipboard::ClipboardProvider;

pub fn get_remote_clipboard() -> Result<Value, Box<dyn Error>> {
    let mut ctx: ClipboardContext = ClipboardProvider::new()?;
    let lines = ctx
        .get_contents()?
        .replace("\r", "")
        .split("\n")
        .map(|line| Value::from(line))
        .collect::<Vec<Value>>();

    // returns a [[String], RegType]
    Ok(Value::from(vec![
        Value::from(lines),
        Value::from("v"), // default to normal paste
    ]))
}

pub fn set_remote_clipboard(arguments: Vec<Value>) -> Result<(), Box<dyn Error>> {
    if arguments.len() != 3 {
        return Err("expected exactly 3 arguments to set_remote_clipboard".into());
    }
    let lines = arguments[0]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect::<Vec<String>>()
                .join("\n")
        })
        .ok_or("can't build string from clipboard")?;

    let register = arguments[2].as_str();
    if register != Some("+") {
        return Err("incompatible register to set remote clipboard".into());
    }
    let mut ctx: ClipboardContext = ClipboardProvider::new()?;
    ctx.set_contents(lines)
}
