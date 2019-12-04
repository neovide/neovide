mod editor;
mod window;

#[macro_use]
extern crate derive_new;
#[macro_use]
extern crate derivative;

use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::thread;

use env_logger::Env as LoggerEnv;
use neovim_lib::{Neovim, UiAttachOptions, Session};
use rmpv::Value;

use window::ui_loop;
use editor::{Editor, DrawCommand};

fn handle_grid_line(grid_line_arguments: &Vec<Value>, editor: &Arc<Mutex<Editor>>) {
    match grid_line_arguments.as_slice() {
        [Value::Integer(grid_id), Value::Integer(row), Value::Integer(col_start), Value::Array(cells)] => {
            let mut col_pos = col_start.as_u64().unwrap();
            for cell in cells.into_iter() {
                match cell {
                    Value::Array(cell_data) => {
                        let mut text = match cell_data.get(0).expect("Cell must have non zero size") {
                            Value::String(cell_text) => cell_text.as_str().expect("Could not process string").to_string(),
                            _ => panic!("Cell text was not a string")
                        };
                        match cell_data.get(2) {
                            Some(Value::Integer(repeat_count)) => {
                                text = text.repeat(repeat_count.as_u64().unwrap_or(1) as usize);
                            }
                            _ => {}
                        };

                        {
                            let mut editor = editor.lock().unwrap();
                            editor.draw(DrawCommand::new(text.to_string(), row.as_u64().unwrap(), col_pos));
                        }
                        col_pos = col_pos + text.chars().count() as u64;
                    },
                    _ => println!("Invalid cell shape")
                }
            }
        }
        _ => println!("Invalid grid_line format")
    };
}

fn handle_redraw_event(event_value: Value, editor: &Arc<Mutex<Editor>>) {
    match event_value {
        Value::Array(event_contents) => {
            let name_value = &event_contents[0];
            let arguments_value = &event_contents[1];
            match (name_value, arguments_value) {
                (Value::String(event_name), Value::Array(arguments)) => {
                    match event_name.as_str().expect("Invalid redraw command name format.") {
                        "grid_line" => handle_grid_line(arguments, &editor),
                        other => println!("Unhandled redraw command {}", other)
                    }
                },
                _ => {
                    println!("Unrecognized redraw event structure.");
                }
            }
        },
        _ => println!("Event is not an array...")
    }
}

fn nvim_event_loop(receiver: Receiver<(String, Vec<Value>)>, editor: &Arc<Mutex<Editor>>) {
    println!("UI thread spawned");
    loop {
        let (event_name, event_args) = receiver.recv().expect("Could not receive event.");
        match event_name.as_ref() {
            "redraw" => {
                for event in event_args {
                    handle_redraw_event(event, &editor);
                }
            },
            _ => println!("Unrecognized Event: {}", event_name)
        };
    }
}

fn main() {
    env_logger::from_env(LoggerEnv::default().default_filter_or("warn")).init();

    let mut session = Session::new_child().unwrap();
    let receiver = session.start_event_loop_channel();
    let mut nvim = Neovim::new(session);
    let mut options = UiAttachOptions::new();
    options.set_cmdline_external(false);
    options.set_messages_external(false);
    options.set_linegrid_external(true);
    options.set_rgb(true);
    nvim.ui_attach(100, 50, &options).unwrap();

    let editor = Arc::new(Mutex::new(Editor::new(nvim, 100, 50)));

    let nvim_editor = editor.clone();
    thread::spawn(move || {
        nvim_event_loop(receiver, &nvim_editor);
    });

    ui_loop(editor);
}
