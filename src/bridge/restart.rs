use rmpv::Value;

#[derive(Clone, Debug)]
pub struct RestartDetails {
    pub progpath: String,
    pub argv: Vec<String>,
}

impl RestartDetails {
    pub fn from_values(arguments: &[Value]) -> Option<Self> {
        if arguments.len() < 2 {
            return None;
        }

        let progpath = arguments[0].as_str()?.to_string();
        let argv = arguments
            .get(1)?
            .as_array()?
            .iter()
            .filter_map(|value| value.as_str().map(|s| s.to_string()))
            .collect::<Vec<_>>();

        Some(Self { progpath, argv })
    }
}
