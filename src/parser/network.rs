//! Parser for flog_net protocol messages.

use crate::domain::network::FlogNetMessage;

const FLOG_NET_TAG: &str = "flog_net";

pub fn try_parse_network(tag: &str, message: &str) -> Option<FlogNetMessage> {
    if tag != FLOG_NET_TAG {
        return None;
    }
    serde_json::from_str(message).ok()
}
