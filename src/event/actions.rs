//! Event-layer action helpers (clipboard, replay, mock, copy logs).
//!
//! Extracted from `event/mod.rs` in Phase 3 Step 3.6 Task 5 to keep the
//! dispatcher small. These are invoked by `apply::apply_click_region`
//! and from keyboard handlers.

use crate::app::App;

/// Copy text to system clipboard (pbcopy on macOS, xclip on Linux).
pub(super) fn copy_to_clipboard(text: &str) -> String {
    let result = std::process::Command::new("pbcopy")
        .stdin(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            if let Some(ref mut stdin) = child.stdin {
                stdin.write_all(text.as_bytes())?;
            }
            child.wait()
        });

    match result {
        Ok(_) => "Copied to clipboard".to_string(),
        Err(_) => {
            let r2 = std::process::Command::new("xclip")
                .args(["-selection", "clipboard"])
                .stdin(std::process::Stdio::piped())
                .spawn()
                .and_then(|mut child| {
                    use std::io::Write;
                    if let Some(ref mut stdin) = child.stdin {
                        stdin.write_all(text.as_bytes())?;
                    }
                    child.wait()
                });
            match r2 {
                Ok(_) => "Copied to clipboard".to_string(),
                Err(_) => "Copy failed (no pbcopy/xclip)".to_string(),
            }
        }
    }
}

/// Replay the currently selected HTTP request.
pub(super) fn replay_selected(app: &mut App) {
    if !app.has_connected_client() {
        app.show_status("Replay unavailable — no client connected".to_string());
        return;
    }

    let indices = app.network.filtered_indices(&app.network_store).to_vec();
    if let Some(&idx) = indices.get(app.network.selected) {
        if let Some(entry) = app.network_store.get(idx).cloned() {
            if entry.protocol == crate::domain::network::Protocol::Http {
                if let Some(handle) = app.get_active_handle() {
                    handle.send_replay(
                        entry.method.clone(),
                        entry.url.clone(),
                        entry.request_headers.clone(),
                        entry.request_body.clone(),
                    );
                    app.show_status("Replaying request...".to_string());
                }
            } else {
                app.show_status("Replay is only available for HTTP requests".to_string());
            }
        }
    }
}

/// Copy selected network request as cURL command.
pub(super) fn copy_as_curl(app: &mut App) {
    let indices = app.network.filtered_indices(&app.network_store).to_vec();
    let entry = if let Some(&idx) = indices.get(app.network.selected) {
        app.network_store.get(idx).cloned()
    } else {
        None
    };

    let entry = match entry {
        Some(e) => e,
        None => {
            app.show_status("No request selected".to_string());
            return;
        }
    };

    if entry.protocol != crate::domain::network::Protocol::Http {
        app.show_status("Copy as cURL is only available for HTTP requests".to_string());
        return;
    }

    let mut cmd = format!("curl -X {} '{}'", entry.method, entry.url);

    // Add headers
    if let Some(ref headers_json) = entry.request_headers {
        if let Ok(serde_json::Value::Object(map)) = serde_json::from_str(headers_json) {
            for (key, val) in &map {
                let val_str = match val {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Array(arr) => {
                        // Dio stores headers as arrays
                        arr.iter()
                            .filter_map(|v| v.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    }
                    other => other.to_string(),
                };
                cmd.push_str(&format!(" \\\n  -H '{}: {}'", key, val_str));
            }
        }
    }

    // Add body
    if let Some(ref body) = entry.request_body {
        if !body.is_empty() {
            let escaped = body.replace('\'', "'\\''");
            cmd.push_str(&format!(" \\\n  -d '{}'", escaped));
        }
    }

    let msg = copy_to_clipboard(&cmd);
    app.show_status(format!("cURL {}", msg));
}

/// Copy selected network request's response body to clipboard.
pub(super) fn copy_response(app: &mut App) {
    let indices = app.network.filtered_indices(&app.network_store).to_vec();
    let entry = if let Some(&idx) = indices.get(app.network.selected) {
        app.network_store.get(idx).cloned()
    } else {
        None
    };

    let entry = match entry {
        Some(e) => e,
        None => {
            app.show_status("No request selected".to_string());
            return;
        }
    };

    // SSE: copy merged text (if in merged mode) or all chunk data
    if entry.protocol == crate::domain::network::Protocol::Sse && !entry.sse_chunks.is_empty() {
        let chunks_data: Vec<&str> = entry.sse_chunks.iter().map(|c| c.data.as_str()).collect();
        let text = if app.network.sse_merged_mode {
            let rule_key = entry
                .path
                .split('?')
                .next()
                .unwrap_or(&entry.path)
                .to_string();
            if let Some(rule) = app.network.sse_merge_rules.get(&rule_key) {
                crate::domain::sse_merge::merge_field(&chunks_data, &rule.field_path)
            } else {
                chunks_data.join("\n")
            }
        } else {
            chunks_data.join("\n")
        };
        if text.is_empty() {
            app.show_status("No SSE data".to_string());
            return;
        }
        let msg = copy_to_clipboard(&text);
        app.show_status(format!("Response {}", msg));
        return;
    }

    // WS: copy chat summary (if in chat mode) or all message data
    if entry.protocol == crate::domain::network::Protocol::Ws && !entry.ws_messages.is_empty() {
        let text = if app.network.ws_chat_mode {
            let msgs: Vec<(crate::domain::network::WsDirection, &str, u64)> = entry
                .ws_messages
                .iter()
                .map(|m| (m.direction, m.data.as_str(), m.size))
                .collect();
            let groups = crate::domain::ws_chat::group_messages(&msgs);
            let mut lines = Vec::new();
            for group in &groups {
                let arrow = match group.direction {
                    crate::domain::network::WsDirection::Send => "\u{2192}",
                    crate::domain::network::WsDirection::Recv => "\u{2190}",
                };
                if group.is_binary {
                    let total_kb = group.total_size as f64 / 1024.0;
                    lines.push(format!(
                        "{} {} [binary {:.1}KB]",
                        arrow, group.type_label, total_kb
                    ));
                } else if let Some(ref merged) = group.merged_delta {
                    lines.push(format!(
                        "{} {} (\u{00d7}{})",
                        arrow,
                        group.type_label,
                        group.msg_indices.len()
                    ));
                    if !merged.is_empty() {
                        lines.push(merged.clone());
                    }
                } else {
                    for &mi in &group.msg_indices {
                        if let Some(msg) = entry.ws_messages.get(mi) {
                            let preview = crate::domain::ws_chat::preview_message(&msg.data, 200);
                            lines.push(format!("{} {}", arrow, preview));
                        }
                    }
                }
            }
            lines.join("\n")
        } else {
            entry
                .ws_messages
                .iter()
                .map(|m| m.data.as_str())
                .collect::<Vec<_>>()
                .join("\n")
        };
        if text.is_empty() {
            app.show_status("No WS data".to_string());
            return;
        }
        let msg = copy_to_clipboard(&text);
        app.show_status(format!("Response {}", msg));
        return;
    }

    let body = entry.response_body.as_deref().unwrap_or("");
    if body.is_empty() {
        app.show_status("No response body".to_string());
        return;
    }

    // Try pretty-print JSON
    let text = if let Ok(value) = serde_json::from_str::<serde_json::Value>(body) {
        serde_json::to_string_pretty(&value).unwrap_or_else(|_| body.to_string())
    } else {
        body.to_string()
    };

    let msg = copy_to_clipboard(&text);
    app.show_status(format!("Response {}", msg));
}

/// Trigger mock rule sync to connected clients.
pub(super) fn trigger_mock_sync(app: &App) {
    if let Some(handle) = app.get_active_handle() {
        let json = app.mock_rules.to_json_string();
        handle.send_mock_sync(json);
    }
}

/// Create a mock rule from the currently selected network request and open editor.
pub(super) fn mock_from_selected(app: &mut App) {
    if !app.has_connected_client() {
        app.show_status("Mock unavailable — no client connected".to_string());
        return;
    }

    let indices = app.network.filtered_indices(&app.network_store).to_vec();
    let entry = if let Some(&idx) = indices.get(app.network.selected) {
        app.network_store.get(idx).cloned()
    } else {
        None
    };

    let entry = match entry {
        Some(e) => e,
        None => {
            app.show_status("No request selected".to_string());
            return;
        }
    };

    if entry.protocol != crate::domain::network::Protocol::Http {
        app.show_status("Mock is only available for HTTP requests".to_string());
        return;
    }

    let url_pattern = entry
        .path
        .split('?')
        .next()
        .unwrap_or(&entry.path)
        .to_string();
    let method = if entry.method.is_empty() {
        None
    } else {
        Some(entry.method.clone())
    };

    // Dedup: check if a rule with same URL pattern + method already exists
    let already_exists = app
        .mock_rules
        .rules()
        .iter()
        .any(|r| r.url_pattern == url_pattern && r.method == method);
    if already_exists {
        app.show_status(format!("Rule already exists: {}", url_pattern));
        app.network.show_mock_rules_panel = true;
        app.network.show_detail = false;
        return;
    }

    let status_code = entry.http_status.unwrap_or(200);
    let response_body = entry
        .response_body
        .clone()
        .unwrap_or_else(|| "{}".to_string());

    app.mock_rules
        .add(url_pattern.clone(), method, status_code, response_body, 0);
    trigger_mock_sync(app);

    // Show rules panel in right side and give feedback
    app.network.show_mock_rules_panel = true;
    app.network.show_detail = false;
    app.mock_rule_selected = app.mock_rules.len().saturating_sub(1);
    app.show_status(format!("Mock rule added: {}", url_pattern));
}

pub(super) fn copy_current_log(app: &mut App) {
    if let Some(idx) = app.selected_store_index() {
        if let Some(entry) = app.store.get(idx) {
            let text = entry.full_message();
            let msg = copy_to_clipboard(&text);
            app.show_status(msg);
        }
    }
}
