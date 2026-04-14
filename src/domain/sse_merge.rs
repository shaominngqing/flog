//! SSE merge utilities: extract JSON field paths and concatenate values across chunks.

use crate::app::SsePathSegment;

/// Extract all unique leaf-string field paths from a JSON value.
/// Returns vec of (path_segments, display_string) pairs.
pub fn extract_field_paths(json: &serde_json::Value) -> Vec<(Vec<SsePathSegment>, String)> {
    let mut paths = Vec::new();
    collect_paths(json, &mut Vec::new(), &mut String::new(), &mut paths);
    paths
}

fn collect_paths(
    val: &serde_json::Value,
    segments: &mut Vec<SsePathSegment>,
    display: &mut String,
    out: &mut Vec<(Vec<SsePathSegment>, String)>,
) {
    match val {
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                let prev_len = display.len();
                if !display.is_empty() {
                    display.push('.');
                }
                display.push_str(key);
                segments.push(SsePathSegment::Key(key.clone()));
                collect_paths(child, segments, display, out);
                segments.pop();
                display.truncate(prev_len);
            }
        }
        serde_json::Value::Array(arr) => {
            for (i, child) in arr.iter().enumerate() {
                let prev_len = display.len();
                display.push_str(&format!("[{}]", i));
                segments.push(SsePathSegment::Index(i));
                collect_paths(child, segments, display, out);
                segments.pop();
                display.truncate(prev_len);
            }
        }
        serde_json::Value::String(_) => {
            out.push((segments.clone(), display.clone()));
        }
        // Skip numbers, bools, nulls — only string leaves are useful for concatenation
        _ => {}
    }
}

/// Resolve a field path against a JSON value, returning the string value if found.
pub fn resolve_path(json: &serde_json::Value, path: &[SsePathSegment]) -> Option<String> {
    let mut current = json;
    for seg in path {
        match seg {
            SsePathSegment::Key(k) => {
                current = current.get(k)?;
            }
            SsePathSegment::Index(i) => {
                current = current.get(*i)?;
            }
        }
    }
    current.as_str().map(|s| s.to_string())
}

/// Auto-detect the best field path for SSE merge.
/// Priority: known LLM streaming patterns first, then first string field that appears in multiple chunks.
pub fn auto_detect_field(chunks_data: &[&str]) -> Option<(Vec<SsePathSegment>, String)> {
    if chunks_data.is_empty() {
        return None;
    }

    // Parse first chunk to get candidate paths
    let first: serde_json::Value = serde_json::from_str(chunks_data[0]).ok()?;
    let candidates = extract_field_paths(&first);

    if candidates.is_empty() {
        return None;
    }

    // Known LLM streaming patterns (check in order)
    let known_patterns = [
        "choices[0].delta.content",      // OpenAI / compatible
        "delta.text",                     // Claude API
        "output[0].delta.content",        // Some OpenAI variants
        "data",                           // Generic SSE
    ];

    for pattern in &known_patterns {
        if let Some(candidate) = candidates.iter().find(|(_, d)| d == pattern) {
            // Verify it resolves in at least the first chunk
            if resolve_path(&first, &candidate.0).is_some() {
                return Some(candidate.clone());
            }
        }
    }

    // Fallback: first string field that exists in at least 2 chunks (or 1 if only 1 chunk)
    let min_count = if chunks_data.len() > 1 { 2 } else { 1 };
    for (path, display) in &candidates {
        let mut count = 0;
        for cd in chunks_data.iter().take(5) {
            // Sample first 5 chunks
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(cd) {
                if resolve_path(&parsed, path).is_some() {
                    count += 1;
                }
            }
        }
        if count >= min_count {
            return Some((path.clone(), display.clone()));
        }
    }

    // Last resort: first candidate
    Some(candidates.into_iter().next().unwrap())
}

/// Concatenate a field across all chunks.
pub fn merge_field(chunks_data: &[&str], path: &[SsePathSegment]) -> String {
    let mut result = String::new();
    for cd in chunks_data {
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(cd) {
            if let Some(val) = resolve_path(&parsed, path) {
                result.push_str(&val);
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn openai_chunk(content: &str) -> String {
        serde_json::json!({
            "id": "chatcmpl-123",
            "object": "response.chunk",
            "choices": [{"delta": {"content": content}}],
            "model": "claude-sonnet-4.6"
        })
        .to_string()
    }

    #[test]
    fn test_extract_field_paths() {
        let json: serde_json::Value = serde_json::from_str(&openai_chunk("hello")).unwrap();
        let paths = extract_field_paths(&json);
        let displays: Vec<&str> = paths.iter().map(|(_, d)| d.as_str()).collect();
        assert!(displays.contains(&"id"));
        assert!(displays.contains(&"object"));
        assert!(displays.contains(&"choices[0].delta.content"));
        assert!(displays.contains(&"model"));
    }

    #[test]
    fn test_resolve_path() {
        let json: serde_json::Value = serde_json::from_str(&openai_chunk("hello")).unwrap();
        let path = vec![
            SsePathSegment::Key("choices".into()),
            SsePathSegment::Index(0),
            SsePathSegment::Key("delta".into()),
            SsePathSegment::Key("content".into()),
        ];
        assert_eq!(resolve_path(&json, &path), Some("hello".to_string()));
    }

    #[test]
    fn test_resolve_path_missing() {
        let json: serde_json::Value = serde_json::from_str(&openai_chunk("hello")).unwrap();
        let path = vec![SsePathSegment::Key("nonexistent".into())];
        assert_eq!(resolve_path(&json, &path), None);
    }

    #[test]
    fn test_auto_detect_openai() {
        let c1 = openai_chunk("Hello");
        let c2 = openai_chunk(" world");
        let chunks: Vec<&str> = vec![&c1, &c2];
        let result = auto_detect_field(&chunks);
        assert!(result.is_some());
        let (_, display) = result.unwrap();
        assert_eq!(display, "choices[0].delta.content");
    }

    #[test]
    fn test_merge_field() {
        let c1 = openai_chunk("Hello");
        let c2 = openai_chunk(" world");
        let c3 = openai_chunk("!");
        let chunks: Vec<&str> = vec![&c1, &c2, &c3];
        let path = vec![
            SsePathSegment::Key("choices".into()),
            SsePathSegment::Index(0),
            SsePathSegment::Key("delta".into()),
            SsePathSegment::Key("content".into()),
        ];
        assert_eq!(merge_field(&chunks, &path), "Hello world!");
    }

    #[test]
    fn test_merge_field_skips_missing() {
        let c1 = openai_chunk("Hello");
        // Chunk without content field
        let c2 = serde_json::json!({"id": "123", "object": "response.chunk"}).to_string();
        let c3 = openai_chunk(" world");
        let chunks: Vec<&str> = vec![&c1, &c2, &c3];
        let path = vec![
            SsePathSegment::Key("choices".into()),
            SsePathSegment::Index(0),
            SsePathSegment::Key("delta".into()),
            SsePathSegment::Key("content".into()),
        ];
        assert_eq!(merge_field(&chunks, &path), "Hello world");
    }

    #[test]
    fn test_auto_detect_empty_chunks() {
        let chunks: Vec<&str> = vec![];
        assert!(auto_detect_field(&chunks).is_none());
    }

    #[test]
    fn test_auto_detect_non_json() {
        let chunks: Vec<&str> = vec!["not json"];
        assert!(auto_detect_field(&chunks).is_none());
    }
}
