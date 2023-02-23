use super::Value;
use log::error;

// Trait to allow for conversion from rmpv::Value to any other data type.
// Note: Feel free to implement this trait for custom types in each subsystem.
// The reverse conversion (MyType->Value) can be performed by implementing `From<MyType> for Value`
pub trait ParseFromValue {
    fn parse_from_value(&mut self, value: Value);
}

// FromValue implementations for most typical types
impl ParseFromValue for f32 {
    fn parse_from_value(&mut self, value: Value) {
        if value.is_f64() {
            *self = value.as_f64().unwrap() as f32;
        } else if value.is_i64() {
            *self = value.as_i64().unwrap() as f32;
        } else if value.is_u64() {
            *self = value.as_u64().unwrap() as f32;
        } else {
            error!("Setting expected an f32, but received {:?}", value);
        }
    }
}

impl ParseFromValue for u64 {
    fn parse_from_value(&mut self, value: Value) {
        if value.is_u64() {
            *self = value.as_u64().unwrap();
        } else {
            error!("Setting expected a u64, but received {:?}", value);
        }
    }
}

impl ParseFromValue for u32 {
    fn parse_from_value(&mut self, value: Value) {
        if value.is_u64() {
            *self = value.as_u64().unwrap() as u32;
        } else {
            error!("Setting expected a u32, but received {:?}", value);
        }
    }
}

impl ParseFromValue for i32 {
    fn parse_from_value(&mut self, value: Value) {
        if value.is_i64() {
            *self = value.as_i64().unwrap() as i32;
        } else {
            error!("Setting expected an i32, but received {:?}", value);
        }
    }
}

impl ParseFromValue for String {
    fn parse_from_value(&mut self, value: Value) {
        if value.is_str() {
            *self = String::from(value.as_str().unwrap());
        } else {
            error!("Setting expected a string, but received {:?}", value);
        }
    }
}

impl ParseFromValue for bool {
    fn parse_from_value(&mut self, value: Value) {
        if value.is_bool() {
            *self = value.as_bool().unwrap();
        } else if value.is_u64() {
            *self = value.as_u64().unwrap() != 0;
        } else {
            error!("Setting expected a bool or 0/1, but received {:?}", value);
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_parse_from_value_f32() {
        let mut v0: f32 = 0.0;
        let v1 = Value::from(1.0);
        let v2 = Value::from(-1);
        let v3 = Value::from(std::u64::MAX);
        let v1p = 1.0;
        let v2p = -1.0;
        let v3p = std::u64::MAX as f32;

        v0.parse_from_value(v1);
        assert_eq!(v0, v1p, "v0 should equal {v1p} but is actually {v0}");
        v0.parse_from_value(v2);
        assert_eq!(v0, v2p, "v0 should equal {v2p} but is actually {v0}");
        v0.parse_from_value(v3);
        assert_eq!(v0, v3p, "v0 should equal {v3p} but is actually {v0}");

        // This is a noop and prints an error
        v0.parse_from_value(Value::from("asd"));
        assert_eq!(v0, v3p, "v0 should equal {v3p} but is actually {v0}");
    }

    #[test]
    fn test_parse_from_value_u64() {
        let mut v0: u64 = 0;
        let v1 = Value::from(std::u64::MAX);
        let v1p = std::u64::MAX;

        v0.parse_from_value(v1);
        assert_eq!(v0, v1p, "v0 should equal {v1p} but is actually {v0}");

        // This is a noop and prints an error
        v0.parse_from_value(Value::from(-1));
        assert_eq!(v0, v1p, "v0 should equal {v1p} but is actually {v0}");
    }

    #[test]
    fn test_parse_from_value_u32() {
        let mut v0: u32 = 0;
        let v1 = Value::from(std::u64::MAX);
        let v1p = std::u64::MAX as u32;

        v0.parse_from_value(v1);
        assert_eq!(v0, v1p, "v0 should equal {v1p} but is actually {v0}");

        // This is a noop and prints an error
        v0.parse_from_value(Value::from(-1));
        assert_eq!(v0, v1p, "v0 should equal {v1p} but is actually {v0}");
    }

    #[test]
    fn test_parse_from_value_i32() {
        let mut v0: i32 = 0;
        let v1 = Value::from(std::i64::MAX);
        let v1p = std::i64::MAX as i32;

        v0.parse_from_value(v1);
        assert_eq!(v0, v1p, "v0 should equal {v1p} but is actually {v0}");

        // This is a noop and prints an error
        v0.parse_from_value(Value::from(-1));
        assert_eq!(v0, v1p, "v0 should equal {v1p} but is actually {v0}");
    }

    #[test]
    fn test_parse_from_value_string() {
        let mut v0: String = "foo".to_string();
        let v1 = Value::from("bar");
        let v1p = "bar";

        v0.parse_from_value(v1);
        assert_eq!(v0, v1p, "v0 should equal {v1p} but is actually {v0}");

        // This is a noop and prints an error
        v0.parse_from_value(Value::from(-1));
        assert_eq!(v0, v1p, "v0 should equal {v1p} but is actually {v0}");
    }

    #[test]
    fn test_parse_from_value_bool() {
        let mut v0: bool = false;
        let v1 = Value::from(true);
        let v1p = true;
        let v2 = Value::from(0);
        let v2p = false;
        let v3 = Value::from(1);
        let v3p = true;

        v0.parse_from_value(v1);
        assert_eq!(v0, v1p, "v0 should equal {v1p} but is actually {v0}");
        v0.parse_from_value(v2);
        assert_eq!(v0, v2p, "v0 should equal {v2p} but is actually {v0}");
        v0.parse_from_value(v3);
        assert_eq!(v0, v3p, "v0 should equal {v3p} but is actually {v0}");

        // This is a noop and prints an error
        v0.parse_from_value(Value::from(-1));
        assert_eq!(v0, v3p, "v0 should equal {v3p} but is actually {v0}");
    }
}
