use std::collections::HashMap;

use super::Value;

pub trait ConvertIntoValue {
    fn convert_into_value(self) -> Value;
}

impl ConvertIntoValue for f32 {
    fn convert_into_value(self) -> Value {
        self.into()
    }
}

impl ConvertIntoValue for u64 {
    fn convert_into_value(self) -> Value {
        self.into()
    }
}

impl ConvertIntoValue for u32 {
    fn convert_into_value(self) -> Value {
        self.into()
    }
}

impl ConvertIntoValue for i32 {
    fn convert_into_value(self) -> Value {
        self.into()
    }
}

impl ConvertIntoValue for String {
    fn convert_into_value(self) -> Value {
        self.into()
    }
}

impl ConvertIntoValue for bool {
    fn convert_into_value(self) -> Value {
        self.into()
    }
}

impl<T: ConvertIntoValue> ConvertIntoValue for Vec<T> {
    fn convert_into_value(self) -> Value {
        let mut vec = Vec::new();
        for item in self {
            vec.push(item.convert_into_value());
        }
        Value::Array(vec)
    }
}

impl<T: ConvertIntoValue, U: ConvertIntoValue> ConvertIntoValue for HashMap<T, U> {
    fn convert_into_value(self) -> Value {
        let mut vec = Vec::new();
        for (k, v) in self {
            vec.push((k.convert_into_value(), v.convert_into_value()));
        }
        Value::Map(vec)
    }
}
