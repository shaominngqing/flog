use crate::app::App;
use crate::commands::ai::redact::{preview_text, redact_text_patterns};
use crate::domain::LogLevel;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogsListOptions {
    pub last: usize,
    pub level: Option<LogLevel>,
    pub tag: Option<String>,
    pub search: Option<String>,
}

pub fn build_logs_list(app: &App, options: LogsListOptions) -> Vec<serde_json::Value> {
    let search = options.search.as_ref().map(|s| s.to_ascii_lowercase());
    let tag = options.tag.as_ref().map(|s| s.to_ascii_lowercase());
    let mut matches = app
        .store
        .iter()
        .enumerate()
        .filter(|(_, log)| options.level.is_none_or(|level| log.level == level))
        .filter(|(_, log)| {
            tag.as_ref()
                .is_none_or(|tag| log.tag.to_ascii_lowercase().contains(tag))
        })
        .filter(|(_, log)| {
            search.as_ref().is_none_or(|search| {
                log.message.to_ascii_lowercase().contains(search)
                    || log.tag.to_ascii_lowercase().contains(search)
                    || log
                        .error
                        .as_deref()
                        .is_some_and(|error| error.to_ascii_lowercase().contains(search))
            })
        })
        .collect::<Vec<_>>();

    let start = matches.len().saturating_sub(options.last);
    matches
        .drain(start..)
        .map(|(index, log)| {
            let message = preview_text(&redact_text_patterns(&log.message), 500);
            serde_json::json!({
                "id": format!("log#{index}"),
                "timestamp": log.timestamp,
                "level": log.level.as_str(),
                "tag": log.tag,
                "message": message.preview,
                "message_truncated": message.truncated,
                "original_bytes": message.original_bytes,
                "repeat_count": log.repeat_count,
                "has_error": log.error.is_some(),
                "has_stacktrace": log.stacktrace.is_some(),
            })
        })
        .collect()
}

#[cfg(test)]
#[path = "logs_tests.rs"]
mod tests;
