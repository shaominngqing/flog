use super::*;
use crate::app::App;
use crate::commands::ai::get::RecordId;
use crate::domain::network::NetworkEntry;

#[test]
fn curl_for_http_request_includes_method_url_headers_and_body() {
    let mut app = App::new();
    let mut entry = NetworkEntry::new_http(
        42,
        "POST".to_string(),
        "https://api.example.com/users".to_string(),
        String::new(),
    );
    entry.request_headers =
        Some(r#"{"authorization":"Bearer secret","content-type":"application/json"}"#.to_string());
    entry.request_body = Some(r#"{"name":"Ada","token":"secret"}"#.to_string());
    app.network_store.push_entry(entry);

    let value = build_curl(&app, &RecordId::Net(42), true).unwrap();
    let curl = value["curl"].as_str().unwrap();

    assert!(curl.contains("curl -X POST 'https://api.example.com/users'"));
    assert!(curl.contains("-H 'authorization: [redacted]'"));
    assert!(curl.contains("-H 'content-type: application/json'"));
    assert!(curl.contains("--data-raw"));
    assert!(curl.contains(r#""token":"[redacted]""#));
}

#[test]
fn curl_rejects_non_network_id() {
    let app = App::new();

    let error = build_curl(&app, &RecordId::Log(0), true).unwrap_err();

    assert_eq!(
        error.message,
        "cURL export is only available for network request ids like net#42."
    );
}
