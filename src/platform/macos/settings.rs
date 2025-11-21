use rmpv::Value;

use crate::{error_msg, settings::*};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionAsMeta {
    OnlyLeft,
    OnlyRight,
    Both,
    None,
}

impl ParseFromValue for OptionAsMeta {
    fn parse_from_value(&mut self, value: Value) {
        if value.is_str() {
            *self = match value.as_str().unwrap() {
                "only_left" => OptionAsMeta::OnlyLeft,
                "only_right" => OptionAsMeta::OnlyRight,
                "both" => OptionAsMeta::Both,
                "none" => OptionAsMeta::None,
                value => {
                    error_msg!("Setting OptionAsMeta expected one of `only_left`, `only_right`, `both`, or `none`, but received {value:?}");
                    return;
                }
            };
        } else {
            error_msg!("Setting OptionAsMeta expected string, but received {value:?}");
        }
    }
}

impl From<OptionAsMeta> for Value {
    fn from(meta: OptionAsMeta) -> Self {
        match meta {
            OptionAsMeta::OnlyLeft => Value::from("only_left"),
            OptionAsMeta::OnlyRight => Value::from("only_right"),
            OptionAsMeta::Both => Value::from("both"),
            OptionAsMeta::None => Value::from("none"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeSettings {
    Auto,
    Dark,
    Light,
    BgColor,
}

impl ParseFromValue for ThemeSettings {
    fn parse_from_value(&mut self, value: Value) {
        if value.is_str() {
            *self = match value.as_str().unwrap() {
                "auto" => ThemeSettings::Auto,
                "dark" => ThemeSettings::Dark,
                "light" => ThemeSettings::Light,
                "bg_color" => ThemeSettings::BgColor,
                value => {
                    error_msg!("Setting Theme expected one of `auto`, `dark`, `light`, `bg_color`, but received {value:?}");
                    return;
                }
            };
        } else {
            error_msg!("Setting Theme expected string, but received {value:?}");
        }
    }
}

impl From<ThemeSettings> for Value {
    fn from(value: ThemeSettings) -> Self {
        match value {
            ThemeSettings::Auto => Value::from("auto"),
            ThemeSettings::Dark => Value::from("dark"),
            ThemeSettings::Light => Value::from("light"),
            ThemeSettings::BgColor => Value::from("bg_color"),
        }
    }
}
