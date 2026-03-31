use rmpv::Value;

#[derive(Clone, Debug, PartialEq)]
pub struct RestartDetails {
    pub listen_addr: String,
}

impl RestartDetails {
    pub fn from_values(arguments: &[Value]) -> Option<Self> {
        match arguments {
            [listen_addr] => Some(Self { listen_addr: listen_addr.as_str()?.to_string() }),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::RestartDetails;
    use rmpv::Value;

    #[test]
    fn parses_listen_addr_restart_payload() {
        let details = RestartDetails::from_values(&[Value::from("/tmp/nvim.sock")]);

        assert_eq!(details, Some(RestartDetails { listen_addr: "/tmp/nvim.sock".to_string() }));
    }

    #[test]
    fn rejects_restart_payload_with_extra_arguments() {
        let details =
            RestartDetails::from_values(&[Value::from("/tmp/nvim.sock"), Value::from("echo 1")]);

        assert_eq!(details, None);
    }

    #[test]
    fn rejects_restart_payload_with_invalid_type() {
        let details = RestartDetails::from_values(&[Value::from("/tmp/nvim.sock"), Value::from(1)]);

        assert_eq!(details, None);
    }
}
