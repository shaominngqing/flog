use super::*;
use std::time::Duration;

#[test]
fn parse_duration_accepts_ms_seconds_and_minutes() {
    assert_eq!(parse_duration("750ms").unwrap(), Duration::from_millis(750));
    assert_eq!(parse_duration("5s").unwrap(), Duration::from_secs(5));
    assert_eq!(parse_duration("2m").unwrap(), Duration::from_secs(120));
}

#[test]
fn parse_duration_rejects_empty_unitless_and_unknown_units() {
    assert!(parse_duration("").is_err());
    assert!(parse_duration("500").is_err());
    assert!(parse_duration("1h").is_err());
}
