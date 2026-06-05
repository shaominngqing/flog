use super::*;
use crate::app::App;
use crate::domain::network::NetworkEntry;

#[test]
fn parse_record_id_accepts_log_net_and_chunk() {
    assert_eq!(parse_record_id("log#12").unwrap(), RecordId::Log(12));
    assert_eq!(parse_record_id("net#42").unwrap(), RecordId::Net(42));
    assert_eq!(
        parse_record_id("chunk#42.13").unwrap(),
        RecordId::Chunk {
            net_id: 42,
            chunk: 13
        }
    );
}

#[test]
fn parse_record_id_rejects_unknown_shape() {
    assert!(parse_record_id("request#1").is_err());
    assert!(parse_record_id("chunk#x.y").is_err());
}

#[test]
fn lookup_net_summary_omits_bodies_and_headers() {
    let mut app = App::new();
    let mut entry = NetworkEntry::new_http(
        7,
        "POST".to_string(),
        "https://api.example.com/users".to_string(),
        String::new(),
    );
    entry.request_headers = Some(r#"{"authorization":"Bearer secret"}"#.to_string());
    entry.request_body = Some(r#"{"name":"Ada"}"#.to_string());
    entry.response_body = Some(r#"{"ok":true}"#.to_string());
    app.network_store.push_entry(entry);

    let record = lookup_record(&app, &RecordId::Net(7), RecordDetailMode::Summary, true).unwrap();

    assert_eq!(record["id"], "net#7");
    assert_eq!(record["method"], "POST");
    assert!(record.get("request_headers").is_none());
    assert!(record.get("request_body").is_none());
    assert!(record.get("response_body").is_none());
}

#[test]
fn lookup_net_detail_includes_redacted_body_preview() {
    let mut app = App::new();
    let mut entry = NetworkEntry::new_http(
        8,
        "POST".to_string(),
        "https://api.example.com/login".to_string(),
        String::new(),
    );
    entry.request_headers = Some(r#"{"authorization":"Bearer secret"}"#.to_string());
    entry.request_body = Some(r#"{"token":"secret","email":"a@example.com"}"#.to_string());
    app.network_store.push_entry(entry);

    let record = lookup_record(&app, &RecordId::Net(8), RecordDetailMode::Detail, true).unwrap();

    assert_eq!(record["request"]["headers"]["authorization"], "[redacted]");
    let preview = record["request"]["body"]["preview"].as_str().unwrap();
    assert!(preview.contains(r#""email":"a@example.com""#));
    assert!(preview.contains(r#""token":"[redacted]""#));
}
