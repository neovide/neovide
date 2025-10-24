use winit::event::Modifiers;

pub fn is_ascii_alphabetic_char(text: &str) -> bool {
    text.len() == 1 && text.chars().next().unwrap().is_ascii_alphabetic()
}

pub fn format_modifier_string(
    modifiers: &Modifiers,
    meta_is_pressed: bool,
    text: &str,
    is_special: bool,
) -> String {
    let state = modifiers.state();
    let include_shift = is_special || (state.control_key() && is_ascii_alphabetic_char(text));
    let have_meta = meta_is_pressed || is_special && state.alt_key(); // e.g. non-meta 'option' with <F1> yeilds <M-F1>

    let mut ret = String::new();
    (state.shift_key() && include_shift).then(|| ret += "S-");
    state.control_key().then(|| ret += "C-");
    have_meta.then(|| ret += "M-");
    state.super_key().then(|| ret += "D-");
    ret
}
