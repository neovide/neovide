/// The keyboard modifiers associated with a keystroke
#[derive(Debug, Copy, Clone)]
pub struct Modifiers {
    /// Shift key
    pub shift: bool,

    /// Control key
    pub control: bool,

    /// Alt on Windows, option on Mac
    pub meta: bool,

    /// Windows key on PC, command key on Mac
    pub logo: bool,
}

impl Modifiers {
    pub fn new(shift: bool, control: bool, meta: bool, logo: bool) -> Self {
        Self {
            shift,
            control,
            meta,
            logo,
        }
    }
}
