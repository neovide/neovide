use std::fmt;

use skulpin::winit::event::{KeyboardInput, ElementState, ModifiersState, VirtualKeyCode};

fn parse_keycode(keycode: VirtualKeyCode) -> Option<(String, bool)> {
    match keycode {
        VirtualKeyCode::Key1 => Some(("1".to_string(), false)),
        VirtualKeyCode::Key2 => Some(("2".to_string(), false)),
        VirtualKeyCode::Key3 => Some(("3".to_string(), false)),
        VirtualKeyCode::Key4 => Some(("4".to_string(), false)),
        VirtualKeyCode::Key5 => Some(("5".to_string(), false)),
        VirtualKeyCode::Key6 => Some(("6".to_string(), false)),
        VirtualKeyCode::Key7 => Some(("7".to_string(), false)),
        VirtualKeyCode::Key8 => Some(("8".to_string(), false)),
        VirtualKeyCode::Key9 => Some(("9".to_string(), false)),
        VirtualKeyCode::Key0 => Some(("0".to_string(), false)),
        VirtualKeyCode::A => Some(("a".to_string(), false)),
        VirtualKeyCode::B => Some(("b".to_string(), false)),
        VirtualKeyCode::C => Some(("c".to_string(), false)),
        VirtualKeyCode::D => Some(("d".to_string(), false)),
        VirtualKeyCode::E => Some(("e".to_string(), false)),
        VirtualKeyCode::F => Some(("f".to_string(), false)),
        VirtualKeyCode::G => Some(("g".to_string(), false)),
        VirtualKeyCode::H => Some(("h".to_string(), false)),
        VirtualKeyCode::I => Some(("i".to_string(), false)),
        VirtualKeyCode::J => Some(("j".to_string(), false)),
        VirtualKeyCode::K => Some(("k".to_string(), false)),
        VirtualKeyCode::L => Some(("l".to_string(), false)),
        VirtualKeyCode::M => Some(("m".to_string(), false)),
        VirtualKeyCode::N => Some(("n".to_string(), false)),
        VirtualKeyCode::O => Some(("o".to_string(), false)),
        VirtualKeyCode::P => Some(("p".to_string(), false)),
        VirtualKeyCode::Q => Some(("q".to_string(), false)),
        VirtualKeyCode::R => Some(("r".to_string(), false)),
        VirtualKeyCode::S => Some(("s".to_string(), false)),
        VirtualKeyCode::T => Some(("t".to_string(), false)),
        VirtualKeyCode::U => Some(("u".to_string(), false)),
        VirtualKeyCode::V => Some(("v".to_string(), false)),
        VirtualKeyCode::W => Some(("w".to_string(), false)),
        VirtualKeyCode::X => Some(("x".to_string(), false)),
        VirtualKeyCode::Y => Some(("y".to_string(), false)),
        VirtualKeyCode::Z => Some(("z".to_string(), false)),
        VirtualKeyCode::Escape => Some(("ESC".to_string(), true)),
        VirtualKeyCode::F1 => Some(("F1".to_string(), true)),
        VirtualKeyCode::F2 => Some(("F2".to_string(), true)),
        VirtualKeyCode::F3 => Some(("F3".to_string(), true)),
        VirtualKeyCode::F4 => Some(("F4".to_string(), true)),
        VirtualKeyCode::F5 => Some(("F5".to_string(), true)),
        VirtualKeyCode::F6 => Some(("F6".to_string(), true)),
        VirtualKeyCode::F7 => Some(("F7".to_string(), true)),
        VirtualKeyCode::F8 => Some(("F8".to_string(), true)),
        VirtualKeyCode::F9 => Some(("F9".to_string(), true)),
        VirtualKeyCode::F10 => Some(("F10".to_string(), true)),
        VirtualKeyCode::F11 => Some(("F11".to_string(), true)),
        VirtualKeyCode::F12 => Some(("F12".to_string(), true)),
        VirtualKeyCode::F13 => Some(("F13".to_string(), true)),
        VirtualKeyCode::F14 => Some(("F14".to_string(), true)),
        VirtualKeyCode::F15 => Some(("F15".to_string(), true)),
        VirtualKeyCode::F16 => Some(("F16".to_string(), true)),
        VirtualKeyCode::F17 => Some(("F17".to_string(), true)),
        VirtualKeyCode::F18 => Some(("F18".to_string(), true)),
        VirtualKeyCode::F19 => Some(("F19".to_string(), true)),
        VirtualKeyCode::F20 => Some(("F20".to_string(), true)),
        VirtualKeyCode::F21 => Some(("F21".to_string(), true)),
        VirtualKeyCode::F22 => Some(("F22".to_string(), true)),
        VirtualKeyCode::F23 => Some(("F23".to_string(), true)),
        VirtualKeyCode::F24 => Some(("F24".to_string(), true)),
        VirtualKeyCode::Insert => Some(("Insert".to_string(), true)),
        VirtualKeyCode::Home => Some(("Home".to_string(), true)),
        VirtualKeyCode::Delete => Some(("Delete".to_string(), true)),
        VirtualKeyCode::End => Some(("End".to_string(), true)),
        VirtualKeyCode::PageDown => Some(("PageDown".to_string(), true)),
        VirtualKeyCode::PageUp => Some(("PageUp".to_string(), true)),
        VirtualKeyCode::Left => Some(("Left".to_string(), true)),
        VirtualKeyCode::Up => Some(("Up".to_string(), true)),
        VirtualKeyCode::Right => Some(("Right".to_string(), true)),
        VirtualKeyCode::Down => Some(("Down".to_string(), true)),
        VirtualKeyCode::Back => Some(("BS".to_string(), true)),
        VirtualKeyCode::Return => Some(("Enter".to_string(), true)),
        VirtualKeyCode::Space => Some(("Space".to_string(), true)),
        VirtualKeyCode::Apostrophe => Some(("'".to_string(), false)),
        VirtualKeyCode::Backslash => Some(("Bslash".to_string(), true)),
        VirtualKeyCode::Colon => Some((":".to_string(), false)),
        VirtualKeyCode::Comma => Some((",".to_string(), false)),
        VirtualKeyCode::Decimal => Some((".".to_string(), false)),
        VirtualKeyCode::Divide => Some(("/".to_string(), false)),
        VirtualKeyCode::Equals => Some(("=".to_string(), false)),
        VirtualKeyCode::Minus => Some(("-".to_string(), false)),
        VirtualKeyCode::Semicolon => Some((";".to_string(), false)),
        _ => None
    }
}

fn append_modifiers(modifiers: ModifiersState, keycode_text: String, special: bool) -> String {
    let mut result = keycode_text;
    let mut special = special;

    if modifiers.shift {
        result = match result.as_ref() {
            "," => "<".to_string(),
            "." => ">".to_string(),
            ";" => ":".to_string(),
            "=" => "+".to_string(),
            "-" => "_".to_string(),
            "1" => "!".to_string(),
            "2" => "@".to_string(),
            "3" => "#".to_string(),
            "4" => "$".to_string(),
            "5" => "%".to_string(),
            "6" => "^".to_string(),
            "7" => "&".to_string(),
            "8" => "*".to_string(),
            "9" => "(".to_string(),
            "0" => ")".to_string(),
            other => {
                special = true;
                format!("S-{}", result)
            }
        };
    }
    if modifiers.ctrl {
        result = format!("C-{}", result);
    }
    if modifiers.alt {
        result = format!("M-{}", result);
    }
    if modifiers.logo {
        result = format!("D-{}", result);
    }
    if special {
        result = format!("<{}>", result);
    }

    result
}

pub fn construct_keybinding_string(input: KeyboardInput) -> Option<String> {
    match input {
        KeyboardInput {
            state: ElementState::Pressed,
            virtual_keycode: Some(keycode),
            modifiers,
            ..
        } => {
            if let Some((keycode_text, special)) = parse_keycode(keycode) {
                Some(append_modifiers(modifiers, keycode_text, special))
            } else {
                None
            }
        },
        _ => None
    }
}
