use rmpv::Value;

use crate::error_msg;
use crate::settings::ParseFromValue;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CornerPreference {
    Default,
    Round,
    RoundSmall,
    DoNotRound,
}

impl ParseFromValue for CornerPreference {
    fn parse_from_value(&mut self, value: Value) {
        if value.is_str() {
            *self = match value.as_str().unwrap() {
                "default" => CornerPreference::Default,
                "round" => CornerPreference::Round,
                "round_small" => CornerPreference::RoundSmall,
                "do_not_round" => CornerPreference::DoNotRound,
                value => {
                    error_msg!(
                        "Setting CornerPreference expected one of `default`, `do_not_round`, `round`, `round_small`, but received {value:?}"
                    );
                    return;
                }
            };
        } else {
            error_msg!("Setting CornerPreference expected string, but received {value:?}");
        }
    }
}

impl From<CornerPreference> for Value {
    fn from(value: CornerPreference) -> Self {
        match value {
            CornerPreference::Default => Value::from("default"),
            CornerPreference::Round => Value::from("round"),
            CornerPreference::RoundSmall => Value::from("round_small"),
            CornerPreference::DoNotRound => Value::from("do_not_round"),
        }
    }
}

impl From<CornerPreference> for winit::platform::windows::CornerPreference {
    fn from(value: CornerPreference) -> Self {
        use winit::platform::windows::CornerPreference as WinitCornerPreference;

        match value {
            CornerPreference::Default => WinitCornerPreference::Default,
            CornerPreference::Round => WinitCornerPreference::Round,
            CornerPreference::RoundSmall => WinitCornerPreference::RoundSmall,
            CornerPreference::DoNotRound => WinitCornerPreference::DoNotRound,
        }
    }
}
