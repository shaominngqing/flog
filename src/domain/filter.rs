use super::entry::{LogEntry, LogLevel};
use regex::Regex;
use std::ops::Range;

/// 过滤状态
#[derive(Debug, Clone)]
pub struct FilterState {
    pub min_level: LogLevel,
    pub tag_include: Vec<String>,
    pub tag_exclude: Vec<String>,
    pub search_query: String,
    pub search_regex: bool,
    compiled_regex: Option<Regex>,
    /// Plain-mode parts split by '|'. Empty when search is empty or in regex mode.
    compiled_search_plain: Vec<String>,
    pub exclude_query: String,
    pub exclude_regex: bool,
    compiled_exclude: Option<Regex>,
    compiled_exclude_plain: Vec<String>,
    pub tag_regex: bool,
    /// 预编译的 tag include 正则
    compiled_tag_include: Vec<Regex>,
    /// 预编译的 tag exclude 正则
    compiled_tag_exclude: Vec<Regex>,
}

impl Default for FilterState {
    fn default() -> Self {
        Self {
            min_level: LogLevel::System,
            tag_include: Vec::new(),
            tag_exclude: Vec::new(),
            search_query: String::new(),
            search_regex: false,
            compiled_regex: None,
            compiled_search_plain: Vec::new(),
            exclude_query: String::new(),
            exclude_regex: false,
            compiled_exclude: None,
            compiled_exclude_plain: Vec::new(),
            tag_regex: false,
            compiled_tag_include: Vec::new(),
            compiled_tag_exclude: Vec::new(),
        }
    }
}

/// OR-match helper used by both Search and Exclude.
///
/// - If `regex` is `Some`, the regex owns the whole query (including `|`); `plain_parts` is ignored.
/// - Otherwise, return true if any non-empty entry in `plain_parts` is a case-insensitive
///   substring of `text`.
pub(crate) fn matches_multi(regex: Option<&Regex>, plain_parts: &[String], text: &str) -> bool {
    if let Some(re) = regex {
        return re.is_match(text);
    }
    if plain_parts.is_empty() {
        return false;
    }
    let text_lower = text.to_lowercase();
    for part in plain_parts {
        if part.is_empty() {
            continue;
        }
        if text_lower.contains(&part.to_lowercase()) {
            return true;
        }
    }
    false
}

impl FilterState {
    fn compile_query(query: &str) -> (bool, Option<Regex>, Vec<String>) {
        // Regex mode: /pattern/ or /pattern/i
        if let Some(regex_body) = query.strip_prefix('/') {
            let (pattern, case_insensitive) = if let Some(p) = regex_body.strip_suffix("/i") {
                (p, true)
            } else if let Some(p) = regex_body.strip_suffix('/') {
                (p, false)
            } else {
                (regex_body, false)
            };
            let full = if case_insensitive {
                format!("(?i){}", pattern)
            } else {
                pattern.to_string()
            };
            let compiled = Regex::new(&full).ok();
            return (true, compiled, Vec::new());
        }
        // Plain multi-term mode: split by '|', trim, drop empties
        let parts: Vec<String> = query
            .split('|')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        (false, None, parts)
    }

    /// Set the Search query. Supports `/regex/` (optionally `/regex/i`) or `a|b|c` OR syntax.
    pub fn set_search(&mut self, query: &str) {
        let (is_regex, compiled, parts) = Self::compile_query(query);
        self.search_query = query.to_string();
        self.search_regex = is_regex;
        self.compiled_regex = compiled;
        self.compiled_search_plain = parts;
    }

    /// Set the Exclude query. Same syntax as set_search.
    pub fn set_exclude(&mut self, query: &str) {
        let (is_regex, compiled, parts) = Self::compile_query(query);
        self.exclude_query = query.to_string();
        self.exclude_regex = is_regex;
        self.compiled_exclude = compiled;
        self.compiled_exclude_plain = parts;
    }

    /// 判断一条日志是否通过过滤
    pub fn matches(&self, entry: &LogEntry) -> bool {
        // Separators always pass through filters
        if entry.tag == "────" {
            return true;
        }

        if entry.level < self.min_level {
            return false;
        }

        let tag = &entry.tag;

        // Tag 排除（使用预编译正则）
        if self.tag_regex {
            for re in &self.compiled_tag_exclude {
                if re.is_match(tag) {
                    return false;
                }
            }
        } else {
            let tag_lower = tag.to_lowercase();
            for exclude in &self.tag_exclude {
                if tag_lower == exclude.to_lowercase() {
                    return false;
                }
            }
        }

        // Tag 包含
        if !self.tag_include.is_empty() {
            let matched = if self.tag_regex {
                self.compiled_tag_include.iter().any(|re| re.is_match(tag))
            } else {
                let tag_lower = tag.to_lowercase();
                self.tag_include
                    .iter()
                    .any(|inc| tag_lower == inc.to_lowercase())
            };
            if !matched {
                return false;
            }
        }

        // Search (OR across message and tag)
        if !self.search_query.is_empty() {
            let full = entry.full_message();
            let hit = matches_multi(
                self.compiled_regex.as_ref(),
                &self.compiled_search_plain,
                &full,
            ) || matches_multi(
                self.compiled_regex.as_ref(),
                &self.compiled_search_plain,
                tag,
            );
            if !hit {
                return false;
            }
        }

        // Exclude (any hit on message or tag → drop)
        if !self.exclude_query.is_empty() {
            let full = entry.full_message();
            let kill = matches_multi(
                self.compiled_exclude.as_ref(),
                &self.compiled_exclude_plain,
                &full,
            ) || matches_multi(
                self.compiled_exclude.as_ref(),
                &self.compiled_exclude_plain,
                tag,
            );
            if kill {
                return false;
            }
        }

        true
    }

    /// 在消息中查找搜索关键词的匹配位置（用于高亮）
    pub fn search_positions(&self, text: &str) -> Vec<Range<usize>> {
        if self.search_query.is_empty() {
            return Vec::new();
        }

        if self.search_regex {
            if let Some(ref re) = self.compiled_regex {
                return re.find_iter(text).map(|m| m.start()..m.end()).collect();
            }
            return Vec::new();
        }

        let text_lower = text.to_lowercase();
        let mut positions = Vec::new();
        for part in &self.compiled_search_plain {
            if part.is_empty() {
                continue;
            }
            let needle = part.to_lowercase();
            let mut start = 0;
            while let Some(pos) = text_lower[start..].find(&needle) {
                let abs_start = start + pos;
                let abs_end = abs_start + needle.len();
                positions.push(abs_start..abs_end);
                start = abs_end;
            }
        }
        positions.sort_by_key(|r| r.start);
        positions
    }

    /// 解析 Tag 过滤输入字符串，预编译正则
    pub fn parse_tag_filter(&mut self, input: &str) {
        self.tag_include.clear();
        self.tag_exclude.clear();
        self.compiled_tag_include.clear();
        self.compiled_tag_exclude.clear();
        self.tag_regex = input.contains('*') || input.contains('.');

        for part in input.split(|c: char| c == ',' || c == '|') {
            let trimmed = part.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Some(tag) = trimmed.strip_prefix('-') {
                if !tag.is_empty() {
                    self.tag_exclude.push(tag.to_string());
                    if self.tag_regex {
                        if let Ok(re) = Regex::new(&format!("(?i){}", tag)) {
                            self.compiled_tag_exclude.push(re);
                        }
                    }
                }
            } else {
                let tag = trimmed.strip_prefix('+').unwrap_or(trimmed);
                if !tag.is_empty() {
                    self.tag_include.push(tag.to_string());
                    if self.tag_regex {
                        if let Ok(re) = Regex::new(&format!("(?i){}", tag)) {
                            self.compiled_tag_include.push(re);
                        }
                    }
                }
            }
        }
    }

    /// 清除所有过滤
    pub fn clear(&mut self) {
        self.tag_include.clear();
        self.tag_exclude.clear();
        self.compiled_tag_include.clear();
        self.compiled_tag_exclude.clear();
        self.search_query.clear();
        self.search_regex = false;
        self.compiled_regex = None;
        self.compiled_search_plain.clear();
        self.exclude_query.clear();
        self.exclude_regex = false;
        self.compiled_exclude = None;
        self.compiled_exclude_plain.clear();
        self.tag_regex = false;
        self.min_level = LogLevel::System;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_multi_plain_single() {
        let parts = vec!["timeout".to_string()];
        assert!(matches_multi(None, &parts, "connection timeout error"));
        assert!(!matches_multi(None, &parts, "connection ok"));
    }

    #[test]
    fn matches_multi_plain_or() {
        let parts = vec!["timeout".to_string(), "500".to_string(), "refused".to_string()];
        assert!(matches_multi(None, &parts, "got 500 from server"));
        assert!(matches_multi(None, &parts, "connection refused"));
        assert!(!matches_multi(None, &parts, "ok 200"));
    }

    #[test]
    fn matches_multi_case_insensitive_plain() {
        let parts = vec!["TiMeOuT".to_string()];
        assert!(matches_multi(None, &parts, "hit a Timeout"));
    }

    #[test]
    fn matches_multi_regex_owns_pipe() {
        let re = Regex::new("foo|bar").unwrap();
        assert!(matches_multi(Some(&re), &[], "hello foo"));
        assert!(matches_multi(Some(&re), &[], "bar world"));
        assert!(!matches_multi(Some(&re), &[], "baz"));
    }

    #[test]
    fn matches_multi_empty_parts_no_regex_is_false() {
        assert!(!matches_multi(None, &[], "anything"));
    }

    #[test]
    fn matches_multi_skips_empty_parts() {
        let parts = vec!["".to_string(), "hit".to_string(), "".to_string()];
        assert!(matches_multi(None, &parts, "go hit target"));
        assert!(!matches_multi(None, &parts, "miss"));
    }

    fn entry(tag: &str, msg: &str) -> LogEntry {
        LogEntry {
            timestamp: String::new(),
            level: LogLevel::Info,
            tag: tag.to_string(),
            message: msg.to_string(),
            extra_lines: Vec::new(),
            error: None,
            stacktrace: None,
            repeat_count: 1,
            source: super::super::entry::InputSource::DirectSocket,
        }
    }

    #[test]
    fn search_plain_multi_or() {
        let mut f = FilterState::default();
        f.set_search("timeout|500");
        assert!(f.matches(&entry("net", "connection timeout")));
        assert!(f.matches(&entry("net", "got 500 back")));
        assert!(!f.matches(&entry("net", "all good")));
    }

    #[test]
    fn search_regex_passes_pipe_through() {
        let mut f = FilterState::default();
        f.set_search("/foo|bar/");
        assert!(f.matches(&entry("t", "foo world")));
        assert!(f.matches(&entry("t", "bar world")));
        assert!(!f.matches(&entry("t", "baz")));
    }

    #[test]
    fn exclude_plain_removes_matches() {
        let mut f = FilterState::default();
        f.set_exclude("heartbeat|ping");
        assert!(f.matches(&entry("t", "real work")));
        assert!(!f.matches(&entry("t", "heartbeat tick")));
        assert!(!f.matches(&entry("t", "ping 30ms")));
    }

    #[test]
    fn exclude_regex_supported() {
        let mut f = FilterState::default();
        f.set_exclude("/^hb_/");
        assert!(!f.matches(&entry("t", "hb_start")));
        assert!(f.matches(&entry("t", "other_start")));
    }

    #[test]
    fn search_and_exclude_intersect() {
        let mut f = FilterState::default();
        f.set_search("error");
        f.set_exclude("heartbeat");
        assert!(f.matches(&entry("t", "got error 500")));
        assert!(!f.matches(&entry("t", "heartbeat error")));
        assert!(!f.matches(&entry("t", "all good")));
    }

    #[test]
    fn exclude_empty_does_nothing() {
        let mut f = FilterState::default();
        f.set_exclude("");
        assert!(f.matches(&entry("t", "anything")));
    }

    #[test]
    fn clear_resets_exclude() {
        let mut f = FilterState::default();
        f.set_exclude("noise");
        f.clear();
        assert!(f.matches(&entry("t", "noise was here")));
    }

    #[test]
    fn parse_tag_filter_accepts_pipe_and_plus_prefix() {
        let mut f = FilterState::default();
        f.parse_tag_filter("+network|-flog_net");
        assert_eq!(f.tag_include, vec!["network".to_string()]);
        assert_eq!(f.tag_exclude, vec!["flog_net".to_string()]);
    }

    #[test]
    fn parse_tag_filter_comma_still_works() {
        let mut f = FilterState::default();
        f.parse_tag_filter("foo,-bar");
        assert_eq!(f.tag_include, vec!["foo".to_string()]);
        assert_eq!(f.tag_exclude, vec!["bar".to_string()]);
    }
}
