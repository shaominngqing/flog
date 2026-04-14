//! WS Chat View utilities: type extraction, binary detection, and message grouping.

use crate::domain::network::WsDirection;

/// A group of consecutive WS messages with the same type and direction.
#[derive(Debug, Clone)]
pub struct ChatGroup {
    pub direction: WsDirection,
    pub type_label: String,
    pub msg_indices: Vec<usize>,
    pub merged_delta: Option<String>,
    pub is_binary: bool,
    pub total_size: u64,
}

/// Extract the "type" label from a WS message's JSON data.
/// Scans common keys: type, event, action, op, cmd, method.
/// Returns "[message]" for JSON without a type key, "[text]" for non-JSON.
pub fn extract_type(data: &str) -> String {
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(data) {
        for key in &["type", "event", "action", "op", "cmd", "method"] {
            if let Some(serde_json::Value::String(t)) = val.get(key) {
                return t.clone();
            }
        }
        "[message]".to_string()
    } else {
        "[text]".to_string()
    }
}

/// Check if a string looks like base64-encoded binary data.
/// Returns true if length > 1024 and content matches base64 charset.
pub fn is_base64_binary(s: &str) -> bool {
    s.len() > 1024
        && s.bytes().all(|b| {
            b.is_ascii_alphanumeric()
                || b == b'+'
                || b == b'/'
                || b == b'='
                || b == b'\n'
                || b == b'\r'
                || b == b' '
        })
}

/// Check if a JSON message contains any binary (large base64) field values.
pub fn has_binary_content(data: &str) -> bool {
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(data) {
        check_binary_recursive(&val)
    } else {
        is_base64_binary(data)
    }
}

fn check_binary_recursive(val: &serde_json::Value) -> bool {
    match val {
        serde_json::Value::String(s) => is_base64_binary(s),
        serde_json::Value::Object(map) => map.values().any(check_binary_recursive),
        serde_json::Value::Array(arr) => arr.iter().any(check_binary_recursive),
        _ => false,
    }
}

/// Extract the "delta" string field from a JSON message, if present.
pub fn extract_delta(data: &str) -> Option<String> {
    let val: serde_json::Value = serde_json::from_str(data).ok()?;
    val.get("delta")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Build a preview string for a message, replacing binary values with [binary N].
pub fn preview_message(data: &str, max_len: usize) -> String {
    if has_binary_content(data) {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(data) {
            let replaced = replace_binary_in_json(&val);
            let s = replaced.to_string();
            if max_len > 0 && s.len() > max_len {
                format!("{}...", &s[..max_len.saturating_sub(3)])
            } else {
                s
            }
        } else {
            let decoded_size = data.len() * 3 / 4;
            format!("[binary {}]", format_binary_size(decoded_size))
        }
    } else if max_len > 0 && data.len() > max_len {
        format!("{}...", &data[..max_len.saturating_sub(3)])
    } else {
        data.to_string()
    }
}

fn replace_binary_in_json(val: &serde_json::Value) -> serde_json::Value {
    match val {
        serde_json::Value::String(s) if is_base64_binary(s) => {
            let decoded_size = s.len() * 3 / 4;
            serde_json::Value::String(format!("[binary {}]", format_binary_size(decoded_size)))
        }
        serde_json::Value::Object(map) => {
            let new_map: serde_json::Map<String, serde_json::Value> = map
                .iter()
                .map(|(k, v)| (k.clone(), replace_binary_in_json(v)))
                .collect();
            serde_json::Value::Object(new_map)
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(replace_binary_in_json).collect())
        }
        other => other.clone(),
    }
}

pub fn format_binary_size(bytes: usize) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1}MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1}KB", bytes as f64 / 1024.0)
    } else {
        format!("{}B", bytes)
    }
}

/// Group consecutive WS messages by (direction, type).
/// Delta messages with string delta fields are merged.
/// Binary-only groups are flagged.
pub fn group_messages(messages: &[(WsDirection, &str, u64)]) -> Vec<ChatGroup> {
    let mut groups: Vec<ChatGroup> = Vec::new();

    for (idx, (direction, data, size)) in messages.iter().enumerate() {
        let type_label = extract_type(data);
        let is_binary = has_binary_content(data);
        let delta = extract_delta(data);

        let can_extend = if let Some(last) = groups.last() {
            last.direction == *direction && last.type_label == type_label
        } else {
            false
        };

        if can_extend {
            let last = groups.last_mut().unwrap();
            last.msg_indices.push(idx);
            last.total_size += size;
            if !is_binary {
                last.is_binary = false;
            }
            if let Some(d) = delta {
                if let Some(ref mut merged) = last.merged_delta {
                    merged.push_str(&d);
                }
            }
        } else {
            let is_delta_type = type_label.to_lowercase().contains("delta");
            groups.push(ChatGroup {
                direction: *direction,
                type_label,
                msg_indices: vec![idx],
                merged_delta: if is_delta_type {
                    delta.or_else(|| Some(String::new()))
                } else {
                    None
                },
                is_binary,
                total_size: *size,
            });
        }
    }

    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_type_with_type_field() {
        let data = r#"{"type":"session.update","session":{}}"#;
        assert_eq!(extract_type(data), "session.update");
    }

    #[test]
    fn test_extract_type_with_event_field() {
        let data = r#"{"event":"message","data":"hello"}"#;
        assert_eq!(extract_type(data), "message");
    }

    #[test]
    fn test_extract_type_no_known_key() {
        let data = r#"{"foo":"bar"}"#;
        assert_eq!(extract_type(data), "[message]");
    }

    #[test]
    fn test_extract_type_non_json() {
        assert_eq!(extract_type("hello world"), "[text]");
    }

    #[test]
    fn test_is_base64_binary_short() {
        assert!(!is_base64_binary("SGVsbG8="));
    }

    #[test]
    fn test_is_base64_binary_long() {
        let long_b64 = "A".repeat(2048);
        assert!(is_base64_binary(&long_b64));
    }

    #[test]
    fn test_is_base64_binary_not_b64() {
        let long_text = "Hello, world! This is not base64. ".repeat(100);
        assert!(!is_base64_binary(&long_text));
    }

    #[test]
    fn test_has_binary_content_in_json() {
        let b64 = "A".repeat(2048);
        let data = format!(r#"{{"type":"audio","data":"{}"}}"#, b64);
        assert!(has_binary_content(&data));
    }

    #[test]
    fn test_has_binary_content_no_binary() {
        let data = r#"{"type":"session.update","session":{"model":"gpt-4"}}"#;
        assert!(!has_binary_content(data));
    }

    #[test]
    fn test_extract_delta() {
        let data = r#"{"type":"response.audio_transcript.delta","delta":"Hello"}"#;
        assert_eq!(extract_delta(data), Some("Hello".to_string()));
    }

    #[test]
    fn test_extract_delta_no_field() {
        let data = r#"{"type":"session.update"}"#;
        assert_eq!(extract_delta(data), None);
    }

    #[test]
    fn test_preview_message_binary() {
        let b64 = "A".repeat(2048);
        let data = format!(r#"{{"type":"audio","data":"{}"}}"#, b64);
        let preview = preview_message(&data, 80);
        assert!(preview.contains("[binary"));
        assert!(!preview.contains(&b64));
    }

    #[test]
    fn test_preview_message_normal() {
        let data = r#"{"type":"session.update","model":"gpt-4"}"#;
        assert_eq!(preview_message(data, 80), data);
    }

    #[test]
    fn test_group_messages_basic() {
        let msgs: Vec<(WsDirection, &str, u64)> = vec![
            (WsDirection::Send, r#"{"type":"session.update"}"#, 25),
            (WsDirection::Recv, r#"{"type":"session.created"}"#, 100),
            (WsDirection::Recv, r#"{"type":"session.created"}"#, 100),
        ];
        let groups = group_messages(&msgs);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].msg_indices.len(), 1);
        assert_eq!(groups[1].msg_indices.len(), 2);
    }

    #[test]
    fn test_group_messages_delta_merge() {
        let msgs: Vec<(WsDirection, &str, u64)> = vec![
            (WsDirection::Recv, r#"{"type":"response.audio_transcript.delta","delta":"Hi"}"#, 50),
            (WsDirection::Recv, r#"{"type":"response.audio_transcript.delta","delta":" there"}"#, 50),
            (WsDirection::Recv, r#"{"type":"response.audio_transcript.delta","delta":"!"}"#, 50),
        ];
        let groups = group_messages(&msgs);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].merged_delta, Some("Hi there!".to_string()));
        assert_eq!(groups[0].msg_indices.len(), 3);
    }

    #[test]
    fn test_group_messages_direction_break() {
        let msgs: Vec<(WsDirection, &str, u64)> = vec![
            (WsDirection::Send, r#"{"type":"request"}"#, 10),
            (WsDirection::Recv, r#"{"type":"request"}"#, 10),
        ];
        let groups = group_messages(&msgs);
        assert_eq!(groups.len(), 2);
    }

    #[test]
    fn test_group_messages_interleaved_types() {
        let msgs: Vec<(WsDirection, &str, u64)> = vec![
            (WsDirection::Recv, r#"{"type":"a.delta","delta":"x"}"#, 10),
            (WsDirection::Recv, r#"{"type":"a.delta","delta":"y"}"#, 10),
            (WsDirection::Recv, r#"{"type":"done"}"#, 10),
            (WsDirection::Recv, r#"{"type":"a.delta","delta":"z"}"#, 10),
        ];
        let groups = group_messages(&msgs);
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0].merged_delta, Some("xy".to_string()));
        assert_eq!(groups[2].merged_delta, Some("z".to_string()));
    }
}
