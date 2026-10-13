use rmpv::Value;

use super::events::ParseError;

#[derive(Clone, Debug, PartialEq)]
pub struct RestartDetails {
    pub listen_addr: String,
    pub channel_config: Option<Vec<(Value, Value)>>,
}

impl RestartDetails {
    pub fn from_values(arguments: Vec<Value>) -> Result<Self, ParseError> {
        let mut arguments = arguments.into_iter();
        let listen_addr = arguments
            .next()
            .ok_or_else(|| ParseError::Format("restart event missing listen address".to_owned()))?
            .try_into()
            .map_err(ParseError::String)?;

        // ignore extra parameters that future ui events may add
        let channel_config = match arguments.next() {
            None => None,
            Some(Value::Map(channel_config)) => Some(channel_config),
            Some(invalid) => return Err(ParseError::Map(invalid)),
        };

        Ok(Self { listen_addr, channel_config })
    }
}

#[cfg(test)]
mod tests {
    use super::RestartDetails;
    use rmpv::Value;

    #[test]
    fn parses_listen_addr_restart_payload() {
        let details = RestartDetails::from_values(vec![Value::from("/tmp/nvim.sock")]).unwrap();

        assert_eq!(
            details,
            RestartDetails { listen_addr: "/tmp/nvim.sock".to_string(), channel_config: None }
        );
    }

    #[test]
    fn rejects_restart_payload_with_extra_arguments() {
        let details =
            RestartDetails::from_values(vec![Value::from("/tmp/nvim.sock"), Value::from("echo 1")]);

        assert!(details.is_err());
    }

    #[test]
    fn rejects_restart_payload_with_invalid_type() {
        let details =
            RestartDetails::from_values(vec![Value::from("/tmp/nvim.sock"), Value::from(1)]);

        assert!(details.is_err());
    }
}
