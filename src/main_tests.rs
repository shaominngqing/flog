use super::*;
use crate::app::App;
use crate::domain;
use crate::input::ClientMessage;

#[test]
fn split_stacktrace_finds_first_frame() {
    let input = "Error: FileSystemException: lock failed\n#0      _checkForErrorResponse (dart:io/common.dart:58:9)\n#1      _RandomAccessFile.lock (dart:io/file_impl.dart:1116:7)\n<asynchronous suspension>";
    let (body, stack) = split_stacktrace(input);
    assert_eq!(body, "Error: FileSystemException: lock failed");
    let stack = stack.expect("stacktrace present");
    assert!(stack.starts_with("#0      _checkForErrorResponse"));
    assert!(stack.contains("<asynchronous suspension>"));
}

#[test]
fn split_stacktrace_no_frames() {
    let input = "plain message with no frames";
    let (body, stack) = split_stacktrace(input);
    assert_eq!(body, "plain message with no frames");
    assert!(stack.is_none());
}

#[test]
fn split_stacktrace_empty_body() {
    let input =
        "#0      foo.bar (package:app/foo.dart:1:1)\n#1      baz.qux (package:app/baz.dart:2:2)";
    let (body, stack) = split_stacktrace(input);
    assert_eq!(body, "");
    assert!(stack.is_some());
}

#[test]
fn raw_log_re_matches_first_line_of_multiline() {
    let first = "[ERROR][Bootstrap] [ZoneError] FileSystemException: lock failed";
    let caps = RAW_LOG_RE.captures(first).expect("first line should match");
    assert_eq!(&caps[1], "ERROR");
    assert_eq!(&caps[2], "Bootstrap");
    assert!(caps[3].contains("ZoneError"));
}

#[test]
fn raw_log_re_rejects_full_multiline_input() {
    // Documents why we split on the first newline before matching:
    // the single-line regex cannot match a body that spans multiple lines.
    let full = "[ERROR][Bootstrap] msg\n#0      foo (a.dart:1:1)";
    assert!(RAW_LOG_RE.captures(full).is_none());
}

// ── format_ts (pure) ────────────────────────────────────────────
//
// format_ts 现在返回本地时区 HH:MM:SS.mmm（chrono::Local），具体时分秒
// 依赖 host 时区，所以这些测试锁定 "格式正确 + 和 chrono Local 同步"。

#[test]
fn format_ts_zero_matches_chrono_local() {
    use chrono::{Local, TimeZone};
    let expected = Local
        .timestamp_millis_opt(0)
        .single()
        .unwrap()
        .format("%H:%M:%S%.3f")
        .to_string();
    assert_eq!(format_ts(0), expected);
}

#[test]
fn format_ts_format_shape() {
    // 无论时区如何，格式是 HH:MM:SS.mmm（12 字符），毫秒保持不变。
    let ms = 25u64 * 3600 * 1000 + 123;
    let t = format_ts(ms);
    assert_eq!(t.len(), "00:00:00.000".len());
    assert!(t.contains(':'));
    assert!(t.ends_with(".123"));
}

#[test]
fn format_ts_ms_preserved_matches_chrono_local() {
    use chrono::{Local, TimeZone};
    // 1h 2m 3s + 456 ms
    let ms = (3600u64 + 2 * 60 + 3) * 1000 + 456;
    let expected = Local
        .timestamp_millis_opt(ms as i64)
        .single()
        .unwrap()
        .format("%H:%M:%S%.3f")
        .to_string();
    assert_eq!(format_ts(ms), expected);
}

// ── dispatch_client_message (pure state mutation) ──────────────

#[test]
fn dispatch_hello_is_noop_on_store() {
    let mut app = App::new();
    let before = app.store.len();
    dispatch_client_message(
        &mut app,
        ClientMessage::Hello {
            device: None,
            app: "test".into(),
            app_version: None,
            os: "macos".into(),
            package_name: None,
            port: None,
            build_mode: None,
            session_id: None,
        },
    );
    assert_eq!(app.store.len(), before);
}

#[test]
fn dispatch_log_structured_preserves_level_and_tag() {
    use chrono::{Local, TimeZone};
    let mut app = App::new();
    let ts_ms: u64 = 3_723_456; // 1h02m03.456s past epoch (UTC)
    dispatch_client_message(
        &mut app,
        ClientMessage::Log {
            level: Some("WARNING".into()),
            tag: Some("net".into()),
            message: "slow request".into(),
            error: None,
            stack_trace: None,
            timestamp: Some(ts_ms),
        },
    );
    let e = app.store.iter().last().expect("entry pushed");
    assert_eq!(e.level, domain::LogLevel::Warning);
    assert_eq!(e.tag, "net");
    // timestamp 是本地时区字符串，和 chrono Local 自比
    let expected = Local
        .timestamp_millis_opt(ts_ms as i64)
        .single()
        .unwrap()
        .format("%H:%M:%S%.3f")
        .to_string();
    assert_eq!(e.timestamp, expected);
}

#[test]
fn dispatch_log_raw_pattern_matches_structured_tag() {
    let mut app = App::new();
    dispatch_client_message(
        &mut app,
        ClientMessage::Log {
            level: None,
            tag: None,
            message: "[ERROR][Bootstrap] crashed hard".into(),
            error: None,
            stack_trace: None,
            timestamp: None,
        },
    );
    let e = app.store.iter().last().expect("entry pushed");
    assert_eq!(e.level, domain::LogLevel::Error);
    assert_eq!(e.tag, "Bootstrap");
    assert!(e.message.contains("crashed hard"));
}

#[test]
fn dispatch_log_unstructured_uses_debugprint_tag() {
    let mut app = App::new();
    dispatch_client_message(
        &mut app,
        ClientMessage::Log {
            level: None,
            tag: None,
            message: "plain flutter output\n#0      foo (a.dart:1:1)".into(),
            error: None,
            stack_trace: None,
            timestamp: None,
        },
    );
    let e = app.store.iter().last().expect("entry pushed");
    assert_eq!(e.tag, "debugPrint");
    assert_eq!(e.level, domain::LogLevel::Debug);
    assert!(e.stacktrace.is_some());
}

#[test]
fn dispatch_log_unknown_level_falls_back_to_info() {
    let mut app = App::new();
    dispatch_client_message(
        &mut app,
        ClientMessage::Log {
            level: Some("GALAXY".into()),
            tag: Some("t".into()),
            message: "m".into(),
            error: None,
            stack_trace: None,
            timestamp: None,
        },
    );
    let e = app.store.iter().last().unwrap();
    assert_eq!(e.level, domain::LogLevel::Info);
}

#[test]
fn reconnect_backoff_constants_match_documented_values() {
    // TRANS-008: lock the exponential backoff schedule so a casual
    // edit to the retry cadence is caught by the test suite.
    assert_eq!(RECONNECT_INITIAL_DELAY_SECS, 2);
    assert_eq!(RECONNECT_MAX_DELAY_SECS, 30);
    assert_eq!(RECONNECT_BACKOFF_FACTOR, 2);

    // Simulate the 2 → 4 → 8 → 16 → 30 (cap) sequence.
    let mut d = RECONNECT_INITIAL_DELAY_SECS;
    let mut seq = vec![d];
    for _ in 0..10 {
        d = (d * RECONNECT_BACKOFF_FACTOR).min(RECONNECT_MAX_DELAY_SECS);
        seq.push(d);
    }
    assert_eq!(&seq[..5], &[2, 4, 8, 16, 30]);
    // Once capped, it stays capped.
    assert!(seq.iter().all(|x| *x <= RECONNECT_MAX_DELAY_SECS));
}

#[test]
fn dispatch_net_routes_to_network_store() {
    let mut app = App::new();
    assert!(app.network_store.is_empty());
    let msg = domain::network::FlogNetKind::Req {
        id: 1,
        p: None,
        method: Some("GET".into()),
        url: Some("https://x.com".into()),
        headers: None,
        body: None,
        size: None,
        ts: None,
    };
    dispatch_client_message(&mut app, ClientMessage::Net { msg });
    assert_eq!(app.network_store.len(), 1);
}
