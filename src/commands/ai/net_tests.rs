use std::time::Duration;

use crate::app::App;
use crate::domain::network::{NetworkEntry, NetworkStatus, Protocol};

use super::{build_net_list, NetListOptions};

#[test]
fn net_list_filters_by_failed_method_url_status_protocol_slow_and_last() {
    let mut app = App::new();
    push_http(
        &mut app,
        1,
        "GET",
        "/ok",
        Some(200),
        NetworkStatus::Completed,
        50,
    );
    push_http(
        &mut app,
        2,
        "POST",
        "/dictionary/error",
        Some(503),
        NetworkStatus::Completed,
        1500,
    );
    push_http(
        &mut app,
        3,
        "POST",
        "/dictionary/slow",
        Some(502),
        NetworkStatus::Completed,
        2200,
    );

    let items = build_net_list(
        &app,
        NetListOptions {
            last: 1,
            failed: true,
            status: Some("5xx".to_string()),
            method: Some("POST".to_string()),
            url: Some("dictionary".to_string()),
            protocol: Some(Protocol::Http),
            slow: Some(Duration::from_millis(1000)),
        },
    );

    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["id"], "net#3");
    assert_eq!(items[0]["method"], "POST");
    assert_eq!(items[0]["status"], 502);
    assert_eq!(items[0]["duration_ms"], 2200);
}

#[test]
fn net_list_includes_protocol_specific_counts_without_bodies() {
    let mut app = App::new();
    let mut ws = NetworkEntry::new_ws(9, "wss://example.test/ws".to_string(), String::new());
    ws.ws_messages.push(crate::domain::network::WsMessage {
        direction: crate::domain::network::WsDirection::Send,
        data: "hello".to_string(),
        size: 5,
        event_timing: None,
    });
    app.network_store.push_entry(ws);

    let items = build_net_list(
        &app,
        NetListOptions {
            last: 10,
            failed: false,
            status: None,
            method: None,
            url: None,
            protocol: Some(Protocol::Ws),
            slow: None,
        },
    );

    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["id"], "net#9");
    assert_eq!(items[0]["protocol"], "ws");
    assert_eq!(items[0]["ws_messages"], 1);
    assert!(items[0].get("response_body").is_none());
    assert!(items[0].get("request_headers").is_none());
}

fn push_http(
    app: &mut App,
    id: u64,
    method: &str,
    url: &str,
    status: Option<u16>,
    network_status: NetworkStatus,
    duration: u64,
) {
    let mut entry = NetworkEntry::new_http(id, method.to_string(), url.to_string(), String::new());
    entry.http_status = status;
    entry.status = network_status;
    entry.duration = Some(duration);
    app.network_store.push_entry(entry);
}
