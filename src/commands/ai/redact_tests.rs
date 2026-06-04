use super::*;

#[test]
fn redact_headers_hides_sensitive_keys_case_insensitively() {
    let value = serde_json::json!({
        "Authorization": "Bearer abc",
        "content-type": "application/json",
        "X-Api-Key": "secret"
    });

    let redacted = redact_json_value(&value);

    assert_eq!(redacted["Authorization"], "[redacted]");
    assert_eq!(redacted["content-type"], "application/json");
    assert_eq!(redacted["X-Api-Key"], "[redacted]");
}

#[test]
fn redact_body_hides_nested_secret_keys() {
    let value = serde_json::json!({
        "user": {"token": "abc"},
        "items": [{"password": "pw"}],
        "ok": true
    });

    let redacted = redact_json_value(&value);

    assert_eq!(redacted["user"]["token"], "[redacted]");
    assert_eq!(redacted["items"][0]["password"], "[redacted]");
    assert_eq!(redacted["ok"], true);
}

#[test]
fn preview_text_truncates_and_reports_original_bytes() {
    let preview = preview_text("abcdef", 4);
    assert_eq!(preview.preview, "abcd");
    assert!(preview.truncated);
    assert_eq!(preview.original_bytes, 6);
}
