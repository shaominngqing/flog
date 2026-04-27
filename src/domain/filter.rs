//! Combined log filter — level + tag (include/exclude) + search +
//! exclude-search, with regex and plain-OR modes per query field.
//!
//! DOM-004 acknowledged (Phase 3 Step 3.2): the four dimensions are
//! applied as a single pipeline in [`FilterState::matches`]. Splitting
//! into four sub-structs would require 4x the plumbing with no gain in
//! call-site ergonomics; the characterization tests below freeze the
//! combined behaviour.

use super::entry::{LogEntry, LogLevel};
use super::filter_traits::MessageFilter;
use regex::Regex;
use std::ops::Range;

/// Combined filter state — level, tag, search, exclude.
///
/// Phase 3 DOM-005: the `*_regex` bool flags and all compiled regex /
/// plain-part vectors are `pub(crate)` — only `set_search` / `set_exclude`
/// / `parse_tag_filter` may mutate them so the query string and its
/// compiled representation stay in sync. External callers read the
/// query strings (`search_query`, `exclude_query`) and the min_level /
/// tag_include / tag_exclude shapes.
#[derive(Debug, Clone)]
pub struct FilterState {
    pub min_level: LogLevel,
    pub tag_include: Vec<String>,
    pub tag_exclude: Vec<String>,
    pub search_query: String,
    pub(crate) search_regex: bool,
    compiled_regex: Option<Regex>,
    /// Plain-mode parts split by '|'. Empty when search is empty or in regex mode.
    compiled_search_plain: Vec<String>,
    pub exclude_query: String,
    pub(crate) exclude_regex: bool,
    compiled_exclude: Option<Regex>,
    compiled_exclude_plain: Vec<String>,
    pub(crate) tag_regex: bool,
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

// Merge overlapping or touching ranges produced by OR-term search into a
// minimal non-overlapping cover. Used by `FilterState::search_positions` so
// the UI highlighter never double-renders the same character span.
// Phase 3 DOM-018.
pub(crate) fn merge_overlapping_ranges(mut ranges: Vec<Range<usize>>) -> Vec<Range<usize>> {
    if ranges.len() <= 1 {
        return ranges;
    }
    ranges.sort_by_key(|r| r.start);
    let mut merged: Vec<Range<usize>> = Vec::with_capacity(ranges.len());
    let mut cur = ranges[0].clone();
    for r in ranges.into_iter().skip(1) {
        if r.start <= cur.end {
            // Overlap or touch — extend current range.
            cur.end = cur.end.max(r.end);
        } else {
            merged.push(cur);
            cur = r;
        }
    }
    merged.push(cur);
    merged
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
                let positions: Vec<Range<usize>> =
                    re.find_iter(text).map(|m| m.start()..m.end()).collect();
                return merge_overlapping_ranges(positions);
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
        merge_overlapping_ranges(positions)
    }

    /// 解析 Tag 过滤输入字符串，预编译正则
    pub fn parse_tag_filter(&mut self, input: &str) {
        self.tag_include.clear();
        self.tag_exclude.clear();
        self.compiled_tag_include.clear();
        self.compiled_tag_exclude.clear();
        self.tag_regex = input.contains('*') || input.contains('.');

        for part in input.split([',', '|']) {
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

impl MessageFilter<LogEntry> for FilterState {
    fn matches(&self, item: &LogEntry) -> bool {
        // Delegate to the inherent method — kept to avoid breaking the
        // many existing call sites that use method syntax.
        FilterState::matches(self, item)
    }
}

#[cfg(test)]
#[path = "filter_tests.rs"]
mod tests;
