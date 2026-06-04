use std::io;
use std::time::{Duration, Instant};

use chrono::Utc;
use serde::Serialize;
use serde_json::json;

use crate::cli::AiWatchArgs;
use crate::domain::network::FlogNetKind;
use crate::input::{ClientMessage, ConnectorEvent, DisconnectReason};

use super::output::{AiEnvelope, AiError};
use super::redact::{preview_text, redact_text_patterns};
use super::session::{self, AiAppCandidate};

pub async fn run_watch(args: AiWatchArgs) -> io::Result<()> {
    let candidate = match session::discover_selected_candidate(
        args.select.port,
        args.select.wait,
        args.select.app.as_deref(),
        args.select.device.as_deref(),
    )
    .await
    {
        Ok(candidate) => candidate,
        Err(error) => return emit_error(error),
    };
    let mut connection = match session::connect_candidate(&candidate).await {
        Ok(connection) => connection,
        Err(error) => return emit_error(error),
    };

    connection.send_subscribe();
    emit_line(&watch_event(
        "connected",
        &candidate,
        serde_json::Value::Null,
    ))?;

    let deadline = Instant::now() + args.duration;
    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match tokio::time::timeout(remaining.min(Duration::from_millis(500)), connection.recv())
            .await
        {
            Ok(Some(ConnectorEvent::Connected(_))) => {}
            Ok(Some(ConnectorEvent::Message(message))) => {
                if should_emit(&args, &message) {
                    emit_line(&watch_event(
                        "message",
                        &candidate,
                        client_message_value(&message),
                    ))?;
                }
            }
            Ok(Some(ConnectorEvent::Disconnected { reason })) => {
                emit_line(&watch_event(
                    "disconnected",
                    &candidate,
                    json!({ "reason": disconnect_reason_value(&reason) }),
                ))?;
                break;
            }
            Ok(None) => break,
            Err(_) => {}
        }
    }

    connection.cleanup().await;
    Ok(())
}

fn emit_error(error: AiError) -> io::Result<()> {
    emit_line(&AiEnvelope::error("watch", error))
}

fn emit_line<T: Serialize>(value: &T) -> io::Result<()> {
    let json = serde_json::to_string(value).map_err(io::Error::other)?;
    println!("{json}");
    Ok(())
}

fn watch_event(
    event_type: &str,
    candidate: &AiAppCandidate,
    payload: serde_json::Value,
) -> serde_json::Value {
    json!({
        "ok": true,
        "meta": {
            "flog_version": env!("CARGO_PKG_VERSION"),
            "schema_version": super::output::AI_SCHEMA_VERSION,
            "command": "watch",
            "generated_at": Utc::now().to_rfc3339(),
        },
        "event": event_type,
        "app": {
            "id": candidate.app_id,
            "name": candidate.app_name,
            "package": candidate.package_name,
            "version": candidate.app_version,
            "device": candidate.device_name,
            "device_id": candidate.device_id,
            "os": candidate.os,
            "build_mode": candidate.build_mode,
            "port": candidate.port,
        },
        "payload": payload,
    })
}

fn should_emit(args: &AiWatchArgs, message: &ClientMessage) -> bool {
    if args.network && !matches!(message, ClientMessage::Net { .. }) {
        return false;
    }
    if args.errors && !is_error_message(message) {
        return false;
    }
    true
}

fn disconnect_reason_value(reason: &DisconnectReason) -> serde_json::Value {
    match reason {
        DisconnectReason::PeerClosed => json!({ "kind": "peer_closed" }),
        DisconnectReason::ReadError(error) => {
            json!({ "kind": "read_error", "message": redact_text_patterns(error) })
        }
        DisconnectReason::WriteError(error) => {
            json!({ "kind": "write_error", "message": redact_text_patterns(error) })
        }
        DisconnectReason::WriterChannelClosed => json!({ "kind": "writer_channel_closed" }),
    }
}

fn is_error_message(message: &ClientMessage) -> bool {
    match message {
        ClientMessage::Log { level, .. } => level
            .as_deref()
            .map(|level| {
                let level = level.to_ascii_lowercase();
                level == "e" || level == "error"
            })
            .unwrap_or(false),
        ClientMessage::Net { msg } => match msg {
            FlogNetKind::Err { .. } => true,
            FlogNetKind::Res { status, .. } => status.is_some_and(|status| status >= 400),
            _ => false,
        },
        ClientMessage::Hello { .. } => false,
    }
}

pub(crate) fn client_message_value(message: &ClientMessage) -> serde_json::Value {
    match message {
        ClientMessage::Hello {
            device,
            app,
            app_version,
            os,
            package_name,
            port,
            build_mode,
            session_id,
        } => json!({
            "type": "hello",
            "device": device,
            "app": app,
            "app_version": app_version,
            "os": os,
            "package_name": package_name,
            "port": port,
            "build_mode": build_mode,
            "session_id": session_id,
        }),
        ClientMessage::Log {
            level,
            tag,
            message,
            error,
            stack_trace,
            timestamp,
        } => json!({
            "type": "log",
            "level": level,
            "tag": tag,
            "message": redact_text_patterns(message),
            "error": error.as_deref().map(redact_text_patterns),
            "stacktrace": stack_trace.as_deref().map(|stack| preview_text(&redact_text_patterns(stack), 1200)),
            "timestamp": timestamp,
        }),
        ClientMessage::Net { msg } => net_message_value(msg),
    }
}

fn net_message_value(message: &FlogNetKind) -> serde_json::Value {
    match message {
        FlogNetKind::Req {
            id,
            p,
            method,
            url,
            size,
            ts,
            ..
        } => json!({
            "type": "net",
            "id": format!("net#{id}"),
            "net_id": id,
            "kind": "req",
            "protocol": p,
            "method": method,
            "url": url,
            "size": size,
            "timestamp": ts,
        }),
        FlogNetKind::Res {
            id,
            status,
            duration,
            size,
            error,
            mocked,
            ts,
            ..
        } => json!({
            "type": "net",
            "id": format!("net#{id}"),
            "net_id": id,
            "kind": "res",
            "status": status,
            "duration_ms": duration,
            "size": size,
            "error": error.as_deref().map(redact_text_patterns),
            "mocked": mocked,
            "timestamp": ts,
        }),
        FlogNetKind::Err {
            id,
            error,
            duration,
            ts,
        } => json!({
            "type": "net",
            "id": format!("net#{id}"),
            "net_id": id,
            "kind": "err",
            "error": error.as_deref().map(redact_text_patterns),
            "duration_ms": duration,
            "timestamp": ts,
        }),
        FlogNetKind::Chunk {
            id,
            data,
            size,
            seq,
            ts,
        } => json!({
            "type": "net",
            "id": format!("net#{id}"),
            "net_id": id,
            "kind": "chunk",
            "data": data.as_deref().map(|data| preview_text(&redact_text_patterns(data), 500)),
            "size": size,
            "seq": seq,
            "timestamp": ts,
        }),
        FlogNetKind::Done { id, duration, ts } => json!({
            "type": "net",
            "id": format!("net#{id}"),
            "net_id": id,
            "kind": "done",
            "duration_ms": duration,
            "timestamp": ts,
        }),
        FlogNetKind::Open { id, url, ts } => json!({
            "type": "net",
            "id": format!("net#{id}"),
            "net_id": id,
            "kind": "open",
            "url": url,
            "timestamp": ts,
        }),
        FlogNetKind::Connecting { id, url, ts } => json!({
            "type": "net",
            "id": format!("net#{id}"),
            "net_id": id,
            "kind": "connecting",
            "url": url,
            "timestamp": ts,
        }),
        FlogNetKind::Send { id, data, size, ts } => json!({
            "type": "net",
            "id": format!("net#{id}"),
            "net_id": id,
            "kind": "send",
            "data": data.as_deref().map(|data| preview_text(&redact_text_patterns(data), 500)),
            "size": size,
            "timestamp": ts,
        }),
        FlogNetKind::Recv { id, data, size, ts } => json!({
            "type": "net",
            "id": format!("net#{id}"),
            "net_id": id,
            "kind": "recv",
            "data": data.as_deref().map(|data| preview_text(&redact_text_patterns(data), 500)),
            "size": size,
            "timestamp": ts,
        }),
        FlogNetKind::Close {
            id,
            code,
            reason,
            duration,
            ts,
        } => json!({
            "type": "net",
            "id": format!("net#{id}"),
            "net_id": id,
            "kind": "close",
            "code": code,
            "reason": reason.as_deref().map(redact_text_patterns),
            "duration_ms": duration,
            "timestamp": ts,
        }),
    }
}

#[cfg(test)]
#[path = "watch_tests.rs"]
mod tests;
