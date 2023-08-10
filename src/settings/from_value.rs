use std::collections::HashMap;

use super::Value;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ParseValueError {
    TypeMismatch {
        expect: &'static str,
        actual: &'static str,
    },
    EnumParseFailed {
        ty: &'static str,
        value: String,
    },
}

impl std::fmt::Display for ParseValueError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseValueError::TypeMismatch { expect, actual } => {
                write!(f, "Type mismatch: expect {}, actual {}", expect, actual)
            }
            ParseValueError::EnumParseFailed { ty, value } => {
                write!(f, "Expected enum {}, but got {}", ty, value)
            }
        }
    }
}

// Trait to allow for conversion from rmpv::Value to any other data type.
// Note: Feel free to implement this trait for custom types in each subsystem.
// The reverse conversion (MyType->Value) can be performed by implementing `From<MyType> for Value`
pub trait ParseFromValue: Sized {
    fn parse_from_value(value: &Value) -> Result<Self, ParseValueError>;
}

// FromValue implementations for most typical types
impl ParseFromValue for f32 {
    fn parse_from_value(value: &Value) -> Result<f32, ParseValueError> {
        value
            .as_f64()
            .map(|v| v as f32)
            .or_else(|| value.as_i64().map(|v| v as f32))
            .or_else(|| value.as_u64().map(|v| v as f32))
            .ok_or_else(|| ParseValueError::TypeMismatch {
                expect: "f32",
                actual: value_type_name(value),
            })
    }
}

impl ParseFromValue for u64 {
    fn parse_from_value(value: &Value) -> Result<u64, ParseValueError> {
        value.as_u64().ok_or_else(|| ParseValueError::TypeMismatch {
            expect: "u64",
            actual: value_type_name(value),
        })
    }
}

impl ParseFromValue for u32 {
    fn parse_from_value(value: &Value) -> Result<u32, ParseValueError> {
        value
            .as_u64()
            .ok_or_else(|| ParseValueError::TypeMismatch {
                expect: "u32",
                actual: value_type_name(value),
            })
            .map(|v| v as u32)
    }
}

impl ParseFromValue for i32 {
    fn parse_from_value(value: &Value) -> Result<i32, ParseValueError> {
        value
            .as_i64()
            .ok_or_else(|| ParseValueError::TypeMismatch {
                expect: "i32",
                actual: value_type_name(value),
            })
            .map(|v| v as i32)
    }
}

impl ParseFromValue for String {
    fn parse_from_value(value: &Value) -> Result<String, ParseValueError> {
        value
            .as_str()
            .map(String::from)
            .ok_or_else(|| ParseValueError::TypeMismatch {
                expect: "String",
                actual: value_type_name(value),
            })
    }
}

impl ParseFromValue for bool {
    fn parse_from_value(value: &Value) -> Result<bool, ParseValueError> {
        value
            .as_bool()
            .or_else(|| value.as_u64().map(|v| v != 0))
            .ok_or_else(|| ParseValueError::TypeMismatch {
                expect: "bool",
                actual: value_type_name(value),
            })
    }
}

impl<T, U> ParseFromValue for HashMap<T, U>
where
    T: ParseFromValue + Eq + std::hash::Hash,
    U: ParseFromValue,
{
    fn parse_from_value(value: &Value) -> Result<Self, ParseValueError> {
        value
            .as_map()
            .map(|kvs| {
                let mut new_map = HashMap::<T, U>::new();
                for (k, v) in kvs {
                    let key = T::parse_from_value(k)?;
                    let value = U::parse_from_value(v)?;
                    new_map.insert(key, value);
                }
                Ok(new_map)
            })
            .ok_or_else(|| ParseValueError::TypeMismatch {
                expect: "map",
                actual: value_type_name(value),
            })?
    }
}

impl<T> ParseFromValue for Vec<T>
where
    T: ParseFromValue,
{
    fn parse_from_value(value: &Value) -> Result<Self, ParseValueError> {
        value
            .as_array()
            .map(|arr| {
                let mut new_vec = Vec::<T>::new();
                for v in arr {
                    new_vec.push(T::parse_from_value(v)?);
                }
                Ok(new_vec)
            })
            .ok_or_else(|| ParseValueError::TypeMismatch {
                expect: "array",
                actual: value_type_name(value),
            })?
    }
}

pub fn value_type_name(value: &Value) -> &'static str {
    if value.is_nil() {
        "nil"
    } else if value.is_bool() {
        "bool"
    } else if value.is_i64() {
        "i64"
    } else if value.is_u64() {
        "u64"
    } else if value.is_f64() {
        "f64"
    } else if value.is_str() {
        "str"
    } else if value.is_array() {
        "array"
    } else if value.is_map() {
        "map"
    } else if value.is_bin() {
        "bin"
    } else if value.is_ext() {
        "ext"
    } else {
        "unknown"
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    macro_rules! assert_parse {
        ($tp:ty, $value:expr, $exp:expr) => {{
            let v0 = <$tp>::parse_from_value($value);
            let expect_value: Result<$tp, ParseValueError> = ($exp).clone();
            assert_eq!(
                v0, $exp,
                "v0 should equal {expect_value:?} but is actually {v0:?}"
            )
        }};
    }

    #[test]
    fn test_parse_from_value_f32() {
        let v1 = Value::from(1.0);
        let v2 = Value::from(-1);
        let v3 = Value::from(std::u64::MAX);
        let v1p = 1.0;
        let v2p = -1.0;
        let v3p = std::u64::MAX as f32;

        assert_parse!(f32, &v1, Ok(v1p));
        assert_parse!(f32, &v2, Ok(v2p));
        assert_parse!(f32, &v3, Ok(v3p));
        assert_parse!(
            f32,
            &Value::from("asd"),
            Err(ParseValueError::TypeMismatch {
                expect: "f32",
                actual: "str",
            })
        );
    }

    #[test]
    fn test_parse_from_value_u64() {
        let v1 = Value::from(std::u64::MAX);
        let v1p = std::u64::MAX;

        assert_parse!(u64, &v1, Ok(v1p));

        assert_parse!(
            u64,
            &Value::from(-1),
            Err(ParseValueError::TypeMismatch {
                expect: "u64",
                actual: "i64",
            })
        );
    }

    #[test]
    fn test_parse_from_value_u32() {
        let v1 = Value::from(std::u64::MAX);
        let v1p = std::u64::MAX as u32;

        assert_parse!(u32, &v1, Ok(v1p));
        assert_parse!(
            u32,
            &Value::from(-1),
            Err(ParseValueError::TypeMismatch {
                expect: "u32",
                actual: "i64",
            })
        );
    }

    #[test]
    fn test_parse_from_value_i32() {
        let v1 = Value::from(std::i64::MAX);
        let v1p = std::i64::MAX as i32;

        assert_parse!(i32, &v1, Ok(v1p));
        assert_parse!(i32, &Value::from(-1), Ok(-1));
    }

    #[test]
    fn test_parse_from_value_string() {
        let v1 = Value::from("bar");
        let v1p = "bar";

        assert_parse!(String, &v1, Ok(v1p.to_string()));
        assert_parse!(
            String,
            &Value::from(-1),
            Err(ParseValueError::TypeMismatch {
                expect: "str",
                actual: "i64",
            })
        );
    }

    #[test]
    fn test_parse_from_value_bool() {
        let v1 = Value::from(true);
        let v1p = true;
        let v2 = Value::from(0);
        let v2p = false;
        let v3 = Value::from(1);
        let v3p = true;

        assert_parse!(bool, &v1, Ok(v1p));
        assert_parse!(bool, &v2, Ok(v2p));
        assert_parse!(bool, &v3, Ok(v3p));
        assert_parse!(
            bool,
            &Value::from("asd"),
            Err(ParseValueError::TypeMismatch {
                expect: "bool",
                actual: "str",
            })
        );
    }

    #[test]
    fn test_parse_from_value_array() {
        let v1 = Value::from(vec![1, 2, 3]);
        let v1p = vec![1, 2, 3];

        assert_parse!(Vec<i32>, &v1, Ok(v1p.clone()));
        assert_parse!(
            Vec<i32>,
            &Value::from("str"),
            Err(ParseValueError::TypeMismatch {
                expect: "array",
                actual: "str",
            })
        );
    }

    #[test]
    fn test_parse_from_value_map() {
        let v1 = Value::from(vec![
            (Value::from("a"), Value::from(1)),
            (Value::from("b"), Value::from(2)),
        ]);
        let v1p: HashMap<String, i32> =
            HashMap::from_iter(vec![("a".to_string(), 1), ("b".to_string(), 2)]);

        assert_parse!(HashMap<String, i32>, &v1, Ok(v1p.clone()));
        assert_parse!(
            HashMap<String, i32>,
            &Value::from("str"),
            Err(ParseValueError::TypeMismatch {
                expect: "map",
                actual: "str",
            })
        );
    }
}
