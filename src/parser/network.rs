//! Parser for flog_net protocol messages.

use crate::domain::network::FlogNetMessage;

const FLOG_NET_TAG: &str = "flog_net";

pub fn try_parse_network(tag: &str, message: &str) -> Option<FlogNetMessage> {
    if tag != FLOG_NET_TAG {
        return None;
    }
    serde_json::from_str(message).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_flog_net_message() {
        let json = r#"{"id":42,"t":"req","method":"GET","url":"https://example.com/api"}"#;
        let msg = try_parse_network("flog_net", json).expect("should parse");
        assert_eq!(msg.id, 42);
        assert_eq!(msg.t, "req");
        assert_eq!(msg.method.as_deref(), Some("GET"));
        assert_eq!(msg.url.as_deref(), Some("https://example.com/api"));
    }

    #[test]
    fn non_flog_net_tag_is_ignored() {
        let json = r#"{"id":1,"t":"req"}"#;
        assert!(try_parse_network("App", json).is_none());
        assert!(try_parse_network("Network", json).is_none());
        assert!(try_parse_network("", json).is_none());
    }

    #[test]
    fn malformed_json_returns_none() {
        assert!(try_parse_network("flog_net", "{not valid json").is_none());
        assert!(try_parse_network("flog_net", "{\"id\":\"not a number\",\"t\":\"req\"}").is_none());
    }

    #[test]
    fn empty_input_returns_none() {
        assert!(try_parse_network("flog_net", "").is_none());
        assert!(try_parse_network("", "").is_none());
    }

    #[test]
    fn extreme_length_input_handled() {
        // Very long URL string inside otherwise-valid JSON — must still parse
        let long_url = "a".repeat(100_000);
        let json = format!(r#"{{"id":1,"t":"req","url":"{long_url}"}}"#);
        let msg = try_parse_network("flog_net", &json).expect("should parse");
        assert_eq!(msg.id, 1);
        assert_eq!(msg.url.as_deref().map(|s| s.len()), Some(100_000));

        // Huge garbage string on wrong tag still short-circuits to None cheaply
        let garbage = "x".repeat(10_000);
        assert!(try_parse_network("other", &garbage).is_none());
    }
}
