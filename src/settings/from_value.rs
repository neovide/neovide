use std::collections::HashMap;

use super::Value;
use log::error;

// Trait to allow for conversion from rmpv::Value to any other data type.
// Note: Feel free to implement this trait for custom types in each subsystem.
// The reverse conversion (MyType->Value) can be performed by implementing `From<MyType> for Value`
pub trait ParseFromValue {
    fn parse_from_value(&mut self, value: &Value) -> bool;
}

// FromValue implementations for most typical types
impl ParseFromValue for f32 {
    fn parse_from_value(&mut self, value: &Value) -> bool {
        if value.is_f64() {
            *self = value.as_f64().unwrap() as f32;
            true
        } else if value.is_i64() {
            *self = value.as_i64().unwrap() as f32;
            true
        } else if value.is_u64() {
            *self = value.as_u64().unwrap() as f32;
            true
        } else {
            error!("Setting expected an f32, but received {:?}", value);
            false
        }
    }
}

impl ParseFromValue for u64 {
    fn parse_from_value(&mut self, value: &Value) -> bool {
        if value.is_u64() {
            *self = value.as_u64().unwrap();
            true
        } else {
            error!("Setting expected a u64, but received {:?}", value);
            false
        }
    }
}

impl ParseFromValue for u32 {
    fn parse_from_value(&mut self, value: &Value) -> bool {
        if value.is_u64() {
            *self = value.as_u64().unwrap() as u32;
            true
        } else {
            error!("Setting expected a u32, but received {:?}", value);
            false
        }
    }
}

impl ParseFromValue for i32 {
    fn parse_from_value(&mut self, value: &Value) -> bool {
        if value.is_i64() {
            *self = value.as_i64().unwrap() as i32;
            true
        } else {
            error!("Setting expected an i32, but received {:?}", value);
            false
        }
    }
}

impl ParseFromValue for String {
    fn parse_from_value(&mut self, value: &Value) -> bool {
        if value.is_str() {
            *self = String::from(value.as_str().unwrap());
            true
        } else {
            error!("Setting expected a string, but received {:?}", value);
            false
        }
    }
}

impl ParseFromValue for bool {
    fn parse_from_value(&mut self, value: &Value) -> bool {
        if value.is_bool() {
            *self = value.as_bool().unwrap();
            true
        } else if value.is_u64() {
            *self = value.as_u64().unwrap() != 0;
            true
        } else {
            error!("Setting expected a bool or 0/1, but received {:?}", value);
            false
        }
    }
}

impl<T, U> ParseFromValue for HashMap<T, U>
where
    T: Default + ParseFromValue + Eq + std::hash::Hash,
    U: Default + ParseFromValue,
{
    fn parse_from_value(&mut self, value: &Value) -> bool {
        if value.is_map() {
            let mut new_map = HashMap::<T, U>::new();
            let kvs = value.as_map().unwrap();
            for (k, v) in kvs {
                let mut key = T::default();
                if !key.parse_from_value(k) {
                    return false;
                }
                let mut value = U::default();
                if !value.parse_from_value(v) {
                    return false;
                }
                new_map.insert(key, value);
            }
            *self = new_map;
            true
        } else {
            error!("Setting expected a map, but received {:?}", value);
            false
        }
    }
}

impl<T> ParseFromValue for Vec<T>
where
    T: Default + ParseFromValue,
{
    fn parse_from_value(&mut self, value: &Value) -> bool {
        if value.is_array() {
            let mut new_vec = Vec::<T>::new();
            let arr = value.as_array().unwrap();
            for v in arr {
                let mut value = T::default();
                if !value.parse_from_value(v) {
                    return false;
                }
                new_vec.push(value);
            }
            *self = new_vec;
            true
        } else {
            error!("Setting expected an array, but received {:?}", value);
            false
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

        v0.parse_from_value(&v1);
        assert_eq!(v0, v1p, "v0 should equal {v1p} but is actually {v0}");
        v0.parse_from_value(&v2);
        assert_eq!(v0, v2p, "v0 should equal {v2p} but is actually {v0}");
        v0.parse_from_value(&v3);
        assert_eq!(v0, v3p, "v0 should equal {v3p} but is actually {v0}");

        // This is a noop and prints an error
        v0.parse_from_value(&Value::from("asd"));
        assert_eq!(v0, v3p, "v0 should equal {v3p} but is actually {v0}");
    }

    #[test]
    fn test_parse_from_value_u64() {
        let mut v0: u64 = 0;
        let v1 = Value::from(std::u64::MAX);
        let v1p = std::u64::MAX;

        v0.parse_from_value(&v1);
        assert_eq!(v0, v1p, "v0 should equal {v1p} but is actually {v0}");

        // This is a noop and prints an error
        v0.parse_from_value(&Value::from(-1));
        assert_eq!(v0, v1p, "v0 should equal {v1p} but is actually {v0}");
    }

    #[test]
    fn test_parse_from_value_u32() {
        let mut v0: u32 = 0;
        let v1 = Value::from(std::u64::MAX);
        let v1p = std::u64::MAX as u32;

        v0.parse_from_value(&v1);
        assert_eq!(v0, v1p, "v0 should equal {v1p} but is actually {v0}");

        // This is a noop and prints an error
        v0.parse_from_value(&Value::from(-1));
        assert_eq!(v0, v1p, "v0 should equal {v1p} but is actually {v0}");
    }

    #[test]
    fn test_parse_from_value_i32() {
        let mut v0: i32 = 0;
        let v1 = Value::from(std::i64::MAX);
        let v1p = std::i64::MAX as i32;

        v0.parse_from_value(&v1);
        assert_eq!(v0, v1p, "v0 should equal {v1p} but is actually {v0}");

        // This is a noop and prints an error
        v0.parse_from_value(&Value::from(-1));
        assert_eq!(v0, v1p, "v0 should equal {v1p} but is actually {v0}");
    }

    #[test]
    fn test_parse_from_value_string() {
        let mut v0: String = "foo".to_string();
        let v1 = Value::from("bar");
        let v1p = "bar";

        v0.parse_from_value(&v1);
        assert_eq!(v0, v1p, "v0 should equal {v1p} but is actually {v0}");

        // This is a noop and prints an error
        v0.parse_from_value(&Value::from(-1));
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

        v0.parse_from_value(&v1);
        assert_eq!(v0, v1p, "v0 should equal {v1p} but is actually {v0}");
        v0.parse_from_value(&v2);
        assert_eq!(v0, v2p, "v0 should equal {v2p} but is actually {v0}");
        v0.parse_from_value(&v3);
        assert_eq!(v0, v3p, "v0 should equal {v3p} but is actually {v0}");

        // This is a noop and prints an error
        v0.parse_from_value(&Value::from(-1));
        assert_eq!(v0, v3p, "v0 should equal {v3p} but is actually {v0}");
    }
}
