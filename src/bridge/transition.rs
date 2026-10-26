//! A requested replacement of the neovim connection behind one route. The
//! `NeovimHandler` creates these requests from redraw events. The window
//! wrapper keeps at most one request per route. On `NeovimExited`, it consumes
//! any remaining request and asks `NeovimRuntime` to attach to the destination.
//! A restart can carry channel properties to restore. A connect event supplies
//! only an address.

use rmpv::Value;

use super::events::ParseError;

/// The new server address and optional channel properties from a `restart` UI
/// event. Neovide applies the properties with `nvim_chan_set` before attaching
/// the UI.
#[derive(Clone, Debug, PartialEq)]
pub struct RestartDetails {
    pub listen_addr: String,
    pub channel_config: Option<Vec<(Value, Value)>>,
}

/// Which connection neovim asked this route to use after its old session exits.
/// A restart is provisional when announced. Neovim can still fail to quit the
/// old server. If a probe observes that failure, the wrapper cancels the
/// pending request. An unanswered probe cannot confirm an abort. For `Connect`,
/// `previous_address` records the old server for a retry if attachment fails.
/// Neovim does not test the destination before detaching. Neither address
/// establishes that its server is still running or reachable.
#[derive(Clone, Debug, PartialEq)]
pub enum SessionTransition {
    Restart(RestartDetails),
    Connect { address: String, previous_address: Option<String> },
}

/// An attachment failure retained with its transition for the application to
/// present. The old session has reported exit, and route cleanup precedes the
/// error window.
#[derive(Debug)]
pub struct TransitionFailure {
    pub transition: SessionTransition,
    pub cause: anyhow::Error,
}

impl SessionTransition {
    pub fn address(&self) -> &str {
        match self {
            Self::Restart(details) => &details.listen_addr,
            Self::Connect { address, .. } => address,
        }
    }

    /// Return both addresses when a failed connect can try the old server.
    /// Retrying the same address would repeat the failed connection attempt.
    pub fn connect_retry_addresses(&self) -> Option<(&str, &str)> {
        match self {
            Self::Connect { address, previous_address: Some(previous_address) }
                if address != previous_address =>
            {
                Some((address, previous_address))
            }
            _ => None,
        }
    }

    /// A connect back to the previous address may need the old screen.
    /// Neovim can send only changed cells when the UI reattaches there.
    pub fn reuses_previous_address(&self) -> bool {
        match self {
            Self::Connect { address, previous_address } => {
                previous_address.as_ref() == Some(address)
            }
            Self::Restart(_) => false,
        }
    }

    pub fn channel_config(&self) -> Option<&[(Value, Value)]> {
        match self {
            Self::Restart(details) => details.channel_config.as_deref(),
            Self::Connect { .. } => None,
        }
    }
}

impl RestartDetails {
    pub fn from_values(arguments: Vec<Value>) -> Result<Self, ParseError> {
        let mut arguments = arguments.into_iter();
        let listen_addr = arguments
            .next()
            .ok_or_else(|| ParseError::Format("restart event missing listen address".to_owned()))?
            .try_into()
            .map_err(ParseError::String)?;

        // Later UI protocol versions may append fields after the channel
        // properties.
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
    use rmpv::Value;

    use super::RestartDetails;
    use super::SessionTransition;

    #[test]
    fn connect_retry_requires_a_distinct_previous_address() {
        let connect = |address: &str, previous_address: Option<&str>| SessionTransition::Connect {
            address: address.to_owned(),
            previous_address: previous_address.map(str::to_owned),
        };

        assert_eq!(connect("next", Some("old")).connect_retry_addresses(), Some(("next", "old")));
        assert_eq!(connect("next", None).connect_retry_addresses(), None);
        assert_eq!(connect("same", Some("same")).connect_retry_addresses(), None);
        assert_eq!(
            SessionTransition::Restart(RestartDetails {
                listen_addr: "next".to_owned(),
                channel_config: None,
            })
            .connect_retry_addresses(),
            None
        );
    }

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
