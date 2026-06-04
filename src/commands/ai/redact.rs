use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TextPreview {
    pub present: bool,
    pub preview: String,
    pub truncated: bool,
    pub original_bytes: usize,
    pub redacted: bool,
}

pub fn redact_json_value(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (key, value) in map {
                if is_sensitive_key(key) {
                    out.insert(
                        key.clone(),
                        serde_json::Value::String("[redacted]".to_string()),
                    );
                } else {
                    out.insert(key.clone(), redact_json_value(value));
                }
            }
            serde_json::Value::Object(out)
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(redact_json_value).collect())
        }
        _ => value.clone(),
    }
}

pub fn preview_text(input: &str, max_chars: usize) -> TextPreview {
    let original_bytes = input.len();
    let preview: String = input.chars().take(max_chars).collect();
    let truncated = preview.len() < input.len();
    TextPreview {
        present: true,
        preview,
        truncated,
        original_bytes,
        redacted: false,
    }
}

pub fn redact_text_patterns(input: &str) -> String {
    static BEARER_RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"(?i)bearer\s+[A-Za-z0-9._~+/=-]+").unwrap()
    });
    BEARER_RE.replace_all(input, "Bearer [redacted]").to_string()
}

fn is_sensitive_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    matches!(
        key.as_str(),
        "authorization" | "cookie" | "set-cookie" | "x-api-key"
    ) || key.contains("token")
        || key.contains("secret")
        || key.contains("password")
        || key.contains("api_key")
        || key.contains("apikey")
}

#[cfg(test)]
#[path = "redact_tests.rs"]
mod tests;
