use std::{collections::HashSet, fmt, hash::Hash};

use itertools::Itertools;
use rmpv::{Utf8StringRef, Value, ValueRef};

#[derive(Debug, Clone)]
pub struct ApiInfoParseError(String);

impl std::error::Error for ApiInfoParseError {}

impl fmt::Display for ApiInfoParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl<'a> From<ValueRef<'a>> for ApiInfoParseError {
    fn from(value: ValueRef) -> Self {
        Self(format!("{}", value))
    }
}

impl From<&str> for ApiInfoParseError {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

#[allow(unused)]
#[derive(Debug)]
pub struct ApiVersion {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pub prerelease: bool,
    pub api_level: u64,
    pub api_compatible: u64,
    pub api_prerelease: bool,
}

impl ApiVersion {
    #[allow(dead_code)]
    pub fn has_version(&self, major: u64, minor: u64, patch: u64) -> bool {
        let actual_major = self.major;
        let actual_minor = self.minor;
        let actual_patch = self.patch;
        log::trace!("actual nvim version: {actual_major}.{actual_minor}.{actual_patch}");
        log::trace!("expect nvim version: {major}.{minor}.{patch}");
        let ret = actual_major > major
            || (actual_major == major && actual_minor > minor)
            || (actual_major == major && actual_minor == minor && actual_patch >= patch);
        log::trace!("has desired nvim version: {ret}");
        ret
    }
}

#[allow(unused)]
#[derive(Debug)]
pub struct ApiFunction {
    pub name: String,
    pub parameters: Vec<ApiParameter>,
    pub return_type: Option<ApiParameterType>,
    pub method: Option<bool>,
    pub since: u64,
    pub deprecated_since: Option<u64>,
}

impl Hash for ApiFunction {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.name.hash(state)
    }
}

impl PartialEq for ApiFunction {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Eq for ApiFunction {}

#[allow(unused)]
#[derive(Debug)]
pub enum ApiParameterType {
    Nil,
    Boolean,
    Integer,
    Float,
    String,
    Array,
    Dictionary,
    Object,
    Buffer,
    Window,
    Tabpage,
    ArrayOf(Box<ApiParameterType>),
    SizedArrayOf(Box<ApiParameterType>, usize),
    LuaRef,
    Void,
    Unknown(String),
}

impl ApiParameterType {
    fn new(value: Option<&str>) -> Self {
        match value {
            Some("Nil") => ApiParameterType::Nil,
            Some("Boolean") => ApiParameterType::Boolean,
            Some("Integer") => ApiParameterType::Integer,
            Some("Float") => ApiParameterType::Float,
            Some("String") => ApiParameterType::String,
            Some("Array") => ApiParameterType::Array,
            Some("Dictionary") => ApiParameterType::Dictionary,
            Some("Object") => ApiParameterType::Object,
            Some("Buffer") => ApiParameterType::Buffer,
            Some("Window") => ApiParameterType::Window,
            Some("Tabpage") => ApiParameterType::Tabpage,
            Some("LuaRef") => ApiParameterType::LuaRef,
            Some("void") => ApiParameterType::Void,
            Some(unknown) => {
                if let Some(array_of) = unknown.strip_prefix("ArrayOf(") {
                    let array_of = array_of.strip_suffix(')').unwrap();
                    let mut parts = array_of.split(',');
                    if let Some(name) = parts.next() {
                        let name = Box::new(Self::new(Some(name.trim())));
                        if let Some(s) = parts.next() {
                            let size = s.trim().parse::<usize>().unwrap_or(0);
                            return ApiParameterType::SizedArrayOf(name, size);
                        } else {
                            return ApiParameterType::ArrayOf(name);
                        }
                    }
                }
                ApiParameterType::Unknown(unknown.to_owned())
            }
            None => ApiParameterType::Unknown("".to_owned()),
        }
    }
}

#[allow(unused)]
#[derive(Debug)]
pub struct ApiParameter {
    pub name: String,
    pub parameter_type: ApiParameterType,
}

#[allow(unused)]
#[derive(Debug)]
pub struct ApiEvent {
    pub name: String,
    pub parameters: Vec<ApiParameter>,
    pub since: u64,
}

impl Hash for ApiEvent {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.name.hash(state)
    }
}

impl PartialEq for ApiEvent {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Eq for ApiEvent {}

#[allow(unused)]
#[derive(Debug)]
pub struct ApiInformation {
    pub channel: u64,
    pub version: ApiVersion,
    pub functions: HashSet<ApiFunction>,
    pub ui_options: Vec<String>,
    pub ui_events: HashSet<ApiEvent>,
    // types and error_error types are not implemented
}

impl ApiInformation {
    #[allow(dead_code)]
    pub fn has_event(&self, event_name: &str) -> bool {
        self.ui_events.iter().any(|event| event.name == event_name)
    }
}

fn parse_version(value: ValueRef) -> std::result::Result<ApiVersion, ApiInfoParseError> {
    let mut major = None;
    let mut minor = None;
    let mut patch = None;
    let mut prerelease = None;
    let mut api_level = None;
    let mut api_compatible = None;
    let mut api_prerelase = None;

    let version: Vec<(ValueRef, ValueRef)> = value.try_into()?;
    for (k, v) in version {
        let k: Utf8StringRef = k.try_into()?;
        match k.as_str() {
            Some("major") => major = Some(v.try_into()?),
            Some("minor") => minor = Some(v.try_into()?),
            Some("patch") => patch = Some(v.try_into()?),
            Some("prerelease") => prerelease = Some(v.try_into()?),
            Some("api_level") => api_level = Some(v.try_into()?),
            Some("api_compatible") => api_compatible = Some(v.try_into()?),
            // api_prerelease should be a boolean value, but Neovim 0.10.0 sets it to nil for some reason, so assume nil means release
            Some("api_prerelease") => api_prerelase = Some(!v.to_owned().is_nil() && v.try_into()?),
            _ => {}
        }
    }

    Ok(ApiVersion {
        major: major.ok_or("major field is missing")?,
        minor: minor.ok_or("minor field is missing")?,
        patch: patch.ok_or("patch field is missing")?,
        prerelease: prerelease.ok_or("prerelease field is isssing")?,
        api_level: api_level.ok_or("api_level field is missing")?,
        api_compatible: api_compatible.ok_or("api_compatible field is missing")?,
        api_prerelease: api_prerelase.ok_or("api_prerelease field is missing")?,
    })
}

fn parse_function(value: ValueRef) -> std::result::Result<ApiFunction, ApiInfoParseError> {
    let mut name = None;
    let mut parameters = None;
    let mut return_type = None;
    let mut method = None;
    let mut since = None;
    let mut deprecated_since = None;
    let fields: Vec<(ValueRef, ValueRef)> = value.try_into()?;
    for (key, v) in fields {
        let k: Utf8StringRef = key.try_into()?;
        match k.as_str() {
            Some("name") => {
                let n: Utf8StringRef = v.try_into()?;
                name = n.as_str().map(|n| n.to_owned())
            }
            Some("parameters") => parameters = Some(parse_parameters(v)?),
            Some("return_type") => return_type = Some(parse_parameter_type(v)?),
            Some("method") => method = Some(v.try_into()?),
            Some("since") => since = Some(v.try_into()?),
            Some("deprecated_since") => deprecated_since = Some(v.try_into()?),
            Some(key) => return Err(key.into()),
            _ => {}
        }
    }
    Ok(ApiFunction {
        name: name.ok_or("name field is missing")?,
        parameters: parameters.ok_or("parameters field is missing")?,
        return_type,
        method,
        since: since.ok_or("since field is missing")?,
        deprecated_since,
    })
}

fn parse_functions(
    value: ValueRef,
) -> std::result::Result<HashSet<ApiFunction>, ApiInfoParseError> {
    let functions: Vec<ValueRef> = value.try_into()?;
    functions
        .into_iter()
        .map(parse_function)
        .collect::<std::result::Result<HashSet<_>, _>>()
}

fn parse_parameter_type(
    value: ValueRef,
) -> std::result::Result<ApiParameterType, ApiInfoParseError> {
    let parameter_type: Utf8StringRef = value.try_into()?;
    Ok(ApiParameterType::new(parameter_type.as_str()))
}

fn parse_parameter(value: ValueRef) -> std::result::Result<ApiParameter, ApiInfoParseError> {
    let info: Vec<ValueRef> = value.try_into()?;
    if let Some((t, n)) = info.into_iter().collect_tuple() {
        let name: Utf8StringRef = n.try_into()?;
        let name = name.as_str();
        let parameter_type = parse_parameter_type(t)?;
        Ok(ApiParameter {
            name: name.map_or(Err("name field is missing"), |v| Ok(v.to_owned()))?,
            parameter_type,
        })
    } else {
        Err("Invalid parameter".into())
    }
}

fn parse_parameters(value: ValueRef) -> std::result::Result<Vec<ApiParameter>, ApiInfoParseError> {
    let parameters: Vec<ValueRef> = value.try_into()?;
    parameters
        .into_iter()
        .map(parse_parameter)
        .collect::<std::result::Result<Vec<_>, _>>()
}

fn parse_string(value: ValueRef) -> std::result::Result<String, ApiInfoParseError> {
    let value: Utf8StringRef = value.try_into()?;
    value
        .as_str()
        .map(|value| value.to_owned())
        .ok_or_else(|| "Failed to parse string value".into())
}

fn parse_string_vec(value: ValueRef) -> std::result::Result<Vec<String>, ApiInfoParseError> {
    let options: Vec<ValueRef> = value.try_into()?;
    options
        .into_iter()
        .map(parse_string)
        .collect::<std::result::Result<Vec<_>, _>>()
}

fn parse_ui_event(value: ValueRef) -> std::result::Result<ApiEvent, ApiInfoParseError> {
    let mut name = None;
    let mut parameters = None;
    let mut since = None;
    let fields: Vec<(ValueRef, ValueRef)> = value.try_into()?;
    for (key, v) in fields {
        let k: Utf8StringRef = key.try_into()?;
        match k.as_str() {
            Some("name") => {
                let n: Utf8StringRef = v.try_into()?;
                name = n.as_str().map(|n| n.to_owned())
            }
            Some("parameters") => parameters = Some(parse_parameters(v)?),
            Some("since") => since = Some(v.try_into()?),
            Some(key) => return Err(key.into()),
            _ => {}
        }
    }
    Ok(ApiEvent {
        name: name.ok_or("name field is missing")?,
        parameters: parameters.ok_or("parameters field is missing")?,
        since: since.ok_or("since field is missing")?,
    })
}

fn parse_ui_events(value: ValueRef) -> std::result::Result<HashSet<ApiEvent>, ApiInfoParseError> {
    let functions: Vec<ValueRef> = value.try_into()?;
    functions
        .into_iter()
        .map(parse_ui_event)
        .collect::<std::result::Result<HashSet<_>, _>>()
}

pub fn parse_api_info(value: &[Value]) -> std::result::Result<ApiInformation, ApiInfoParseError> {
    let channel = value[0].as_ref().try_into()?;

    let metadata: Vec<(ValueRef, ValueRef)> = value[1].as_ref().try_into()?;

    let mut version = None;
    let mut functions = None;
    let mut ui_options = None;
    let mut ui_events = None;

    for (k, v) in metadata {
        let k: Utf8StringRef = k.try_into()?;
        match k.as_str() {
            Some("version") => version = Some(parse_version(v)?),
            Some("functions") => functions = Some(parse_functions(v)?),
            Some("ui_options") => ui_options = Some(parse_string_vec(v)?),
            Some("ui_events") => ui_events = Some(parse_ui_events(v)?),
            _ => {}
        }
    }

    Ok(ApiInformation {
        channel,
        version: version.ok_or("version field is missing")?,
        functions: functions.ok_or("functions field is missing")?,
        ui_options: ui_options.ok_or("ui_options field is missing")?,
        ui_events: ui_events.ok_or("ui_events field is missing")?,
    })
}
