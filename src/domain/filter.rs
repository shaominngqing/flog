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
        let parts = vec![
            "timeout".to_string(),
            "500".to_string(),
            "refused".to_string(),
        ];
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

    // ==================================================================
    // Phase 2.5B Task 2 — characterization tests locking A/D behavior
    // ==================================================================

    // ---- DOM-004: FilterState combines four orthogonal dimensions -----
    // Each dimension tested independently. Phase 3 may split but behavior
    // surface must not change.

    #[test]
    fn dom_004_level_dimension_blocks_below_min() {
        let f = FilterState {
            min_level: LogLevel::Warning,
            ..FilterState::default()
        };
        let mut e = entry("t", "hello");
        e.level = LogLevel::Info;
        assert!(!f.matches(&e));
        e.level = LogLevel::Warning;
        assert!(f.matches(&e));
        e.level = LogLevel::Error;
        assert!(f.matches(&e));
    }

    #[test]
    fn dom_004_tag_dimension_include_only() {
        let mut f = FilterState::default();
        f.parse_tag_filter("keep");
        assert!(f.matches(&entry("keep", "m")));
        assert!(!f.matches(&entry("drop", "m")));
    }

    #[test]
    fn dom_004_tag_dimension_exclude_only() {
        let mut f = FilterState::default();
        f.parse_tag_filter("-noise");
        assert!(f.matches(&entry("keep", "m")));
        assert!(!f.matches(&entry("noise", "m")));
    }

    #[test]
    fn dom_004_search_dimension_only() {
        let mut f = FilterState::default();
        f.set_search("alpha");
        assert!(f.matches(&entry("t", "alpha beta")));
        assert!(!f.matches(&entry("t", "beta gamma")));
    }

    #[test]
    fn dom_004_exclude_dimension_only() {
        let mut f = FilterState::default();
        f.set_exclude("drop");
        assert!(f.matches(&entry("t", "keep it")));
        assert!(!f.matches(&entry("t", "drop this")));
    }

    // ---- DOM-005: compiled regex+plain must stay in sync --------------

    #[test]
    fn dom_005_set_search_resyncs_compiled_state_a_plain() {
        let mut f = FilterState::default();
        f.set_search("/regex/");
        f.set_search("plain");
        // plain mode must not retain compiled_regex from previous call
        assert!(!f.search_regex);
        assert!(f.matches(&entry("t", "plain truth")));
    }

    #[test]
    fn dom_005_set_search_resyncs_compiled_state_b_regex() {
        let mut f = FilterState::default();
        f.set_search("plain");
        f.set_search("/^abc/");
        assert!(f.search_regex);
        assert!(f.matches(&entry("t", "abcdef")));
        assert!(!f.matches(&entry("t", "zabc")));
    }

    #[test]
    fn dom_005_set_search_resyncs_compiled_state_c_empty() {
        let mut f = FilterState::default();
        f.set_search("something");
        f.set_search("");
        // empty query disables the filter
        assert!(f.matches(&entry("t", "anything at all")));
    }

    #[test]
    fn dom_005_set_exclude_resyncs_compiled_state() {
        let mut f = FilterState::default();
        f.set_exclude("/bad/");
        f.set_exclude("nope");
        // plain mode active
        assert!(!f.exclude_regex);
        assert!(!f.matches(&entry("t", "nope this out")));
    }

    // ---- DOM-018 (B, already noted in plan): search_positions current
    // behavior. This locks what currently happens — Task 12 writes the
    // red test asserting the desired-fixed behavior. Here we just lock
    // the current plain-mode branches.

    // ---- DOM-018 helper tests -----------------------------------------

    #[test]
    fn merge_overlapping_ranges_empty_and_single() {
        let empty: Vec<Range<usize>> = Vec::new();
        assert!(merge_overlapping_ranges(empty).is_empty());
        // Single-element input returns unchanged.
        let one = std::iter::once(2..5usize).collect::<Vec<_>>();
        assert_eq!(merge_overlapping_ranges(one), vec![2..5]);
    }

    #[test]
    fn merge_overlapping_ranges_disjoint_sorted() {
        assert_eq!(
            merge_overlapping_ranges(vec![0..2, 5..7, 10..12]),
            vec![0..2, 5..7, 10..12]
        );
    }

    #[test]
    fn merge_overlapping_ranges_disjoint_unsorted_input_is_sorted() {
        assert_eq!(
            merge_overlapping_ranges(vec![10..12, 0..2, 5..7]),
            vec![0..2, 5..7, 10..12]
        );
    }

    #[test]
    fn merge_overlapping_ranges_touching_ranges_coalesce() {
        // 0..3 and 3..5 are adjacent/touching → merge into 0..5
        assert_eq!(merge_overlapping_ranges(vec![0..3, 3..5]), vec![0..5]);
    }

    #[test]
    fn merge_overlapping_ranges_strictly_overlapping_coalesce() {
        // The DOM-018 case: "the" → 0..3 and "e" → 2..3 inside "thee".
        assert_eq!(merge_overlapping_ranges(vec![0..3, 2..3]), vec![0..3]);
    }

    #[test]
    fn merge_overlapping_ranges_multiple_overlaps_collapse() {
        assert_eq!(
            merge_overlapping_ranges(vec![0..3, 1..4, 3..5, 10..12]),
            vec![0..5, 10..12]
        );
    }

    #[test]
    fn search_positions_empty_query_returns_empty() {
        let f = FilterState::default();
        assert!(f.search_positions("anything").is_empty());
    }

    #[test]
    fn search_positions_plain_single_term() {
        let mut f = FilterState::default();
        f.set_search("foo");
        let p = f.search_positions("say foo often foo");
        assert_eq!(p.len(), 2);
        assert_eq!(p[0], 4..7);
        assert_eq!(p[1], 14..17);
    }

    #[test]
    fn search_positions_plain_or_multi() {
        let mut f = FilterState::default();
        f.set_search("foo|bar");
        let p = f.search_positions("foo and bar");
        // sorted by start
        assert_eq!(p[0].start, 0);
        assert!(p.last().unwrap().start >= p[0].start);
    }

    #[test]
    fn search_positions_regex_mode() {
        let mut f = FilterState::default();
        f.set_search("/f.o/");
        let p = f.search_positions("fao fbo fco");
        assert_eq!(p.len(), 3);
    }

    #[test]
    fn search_positions_invalid_regex_returns_empty() {
        let mut f = FilterState::default();
        f.set_search("/[invalid/");
        // invalid regex fails to compile; search_query still set but
        // compiled_regex is None → returns empty
        assert!(f.search_positions("abc").is_empty());
    }

    #[test]
    fn search_positions_plain_empty_parts_skipped() {
        // Direct construct: plain_search with an empty part should skip it.
        let mut f = FilterState::default();
        f.set_search("foo||bar");
        let p = f.search_positions("bar");
        // only "bar" matches; "" parts skipped
        assert_eq!(p.len(), 1);
    }

    #[test]
    fn search_positions_case_insensitive() {
        let mut f = FilterState::default();
        f.set_search("FOO");
        let p = f.search_positions("foo FOO Foo");
        assert_eq!(p.len(), 3);
    }

    // ---- DOM-019: filter parallel implementations — characterize
    // every filter combination branch here too.

    #[test]
    fn dom_019_all_dimensions_combined() {
        let mut f = FilterState {
            min_level: LogLevel::Info,
            ..FilterState::default()
        };
        f.parse_tag_filter("keep,-noise");
        f.set_search("alpha");
        f.set_exclude("drop");

        // passes all
        let mut pass = entry("keep", "alpha value");
        pass.level = LogLevel::Info;
        assert!(f.matches(&pass));

        // fails level
        let mut below = entry("keep", "alpha value");
        below.level = LogLevel::Debug;
        assert!(!f.matches(&below));

        // fails tag include
        let mut wrong_tag = entry("other", "alpha value");
        wrong_tag.level = LogLevel::Info;
        assert!(!f.matches(&wrong_tag));

        // fails tag exclude
        let mut excluded_tag = entry("noise", "alpha value");
        excluded_tag.level = LogLevel::Info;
        assert!(!f.matches(&excluded_tag));

        // fails search
        let mut no_search = entry("keep", "beta");
        no_search.level = LogLevel::Info;
        assert!(!f.matches(&no_search));

        // fails exclude
        let mut dropped = entry("keep", "alpha drop");
        dropped.level = LogLevel::Info;
        assert!(!f.matches(&dropped));
    }

    // ---- Rule 10: core-module test density (filter.rs) ---------------

    #[test]
    fn matches_separator_bypasses_all_filters() {
        let mut f = FilterState {
            min_level: LogLevel::Error,
            ..FilterState::default()
        };
        f.set_search("nothing-matches");
        let sep = entry("────", "");
        assert!(f.matches(&sep));
    }

    #[test]
    fn matches_search_hits_tag_not_message() {
        let mut f = FilterState::default();
        f.set_search("network");
        // message has no "network" but tag does
        assert!(f.matches(&entry("network", "hello world")));
    }

    #[test]
    fn matches_exclude_hits_tag_not_message() {
        let mut f = FilterState::default();
        f.set_exclude("noise");
        assert!(!f.matches(&entry("noise_tag", "innocent message")));
    }

    #[test]
    fn matches_full_message_includes_extra_lines() {
        let mut f = FilterState::default();
        f.set_search("continuation");
        let mut e = entry("t", "first line");
        e.extra_lines.push("continuation line".to_string());
        assert!(f.matches(&e));
    }

    #[test]
    fn tag_regex_mode_include() {
        let mut f = FilterState::default();
        f.parse_tag_filter("net.*");
        assert!(f.tag_regex);
        assert!(f.matches(&entry("network", "m")));
        assert!(f.matches(&entry("net", "m")));
        assert!(!f.matches(&entry("other", "m")));
    }

    #[test]
    fn tag_regex_mode_exclude() {
        let mut f = FilterState::default();
        f.parse_tag_filter("-hb.*");
        assert!(f.tag_regex);
        assert!(!f.matches(&entry("hb_start", "m")));
        assert!(!f.matches(&entry("hb_end", "m")));
        assert!(f.matches(&entry("other", "m")));
    }

    #[test]
    fn tag_regex_mode_wildcard_glob() {
        let mut f = FilterState::default();
        // '*' triggers regex mode
        f.parse_tag_filter("foo*");
        assert!(f.tag_regex);
    }

    #[test]
    fn parse_tag_filter_strips_dash_and_empty_segments() {
        let mut f = FilterState::default();
        f.parse_tag_filter(",  ,-,-,--,+");
        // bare '-' with nothing after → empty → skipped
        // bare '+' with nothing after → empty → skipped
        // '--' → tag_exclude gets "-"? strip once then keep
        // lock current behavior
        // At least none of the include/exclude lists should contain "" entries
        assert!(!f.tag_include.iter().any(|s| s.is_empty()));
        assert!(!f.tag_exclude.iter().any(|s| s.is_empty()));
    }

    #[test]
    fn parse_tag_filter_clears_prior_state() {
        let mut f = FilterState::default();
        f.parse_tag_filter("alpha,-beta");
        f.parse_tag_filter("gamma");
        assert_eq!(f.tag_include, vec!["gamma".to_string()]);
        assert!(f.tag_exclude.is_empty());
    }

    #[test]
    fn set_search_regex_ci_suffix() {
        let mut f = FilterState::default();
        f.set_search("/AbC/i");
        assert!(f.search_regex);
        assert!(f.matches(&entry("t", "abc")));
        assert!(f.matches(&entry("t", "ABC")));
    }

    #[test]
    fn set_search_regex_unterminated_slash() {
        let mut f = FilterState::default();
        // "/pattern" without trailing "/" is still regex mode per impl
        f.set_search("/abc");
        assert!(f.search_regex);
    }

    #[test]
    fn set_search_invalid_regex_compiles_to_none() {
        let mut f = FilterState::default();
        f.set_search("/[unclosed/");
        assert!(f.search_regex);
        // no hits because compiled_regex is None; matches_multi returns false
        assert!(!f.matches(&entry("t", "anything [unclosed")));
    }

    #[test]
    fn set_exclude_invalid_regex_compiles_to_none() {
        let mut f = FilterState::default();
        f.set_exclude("/[unclosed/");
        // All entries pass: exclude with None regex and empty plain_parts is no-op
        assert!(f.matches(&entry("t", "anything")));
    }

    #[test]
    fn clear_resets_min_level_to_system() {
        let mut f = FilterState {
            min_level: LogLevel::Error,
            ..FilterState::default()
        };
        f.clear();
        assert_eq!(f.min_level, LogLevel::System);
    }

    #[test]
    fn clear_resets_tag_state() {
        let mut f = FilterState::default();
        f.parse_tag_filter("+foo|-bar");
        f.clear();
        assert!(f.tag_include.is_empty());
        assert!(f.tag_exclude.is_empty());
        assert!(!f.tag_regex);
    }

    #[test]
    fn matches_multi_unicode_plain() {
        // CJK substring match via lowercase (no-op for CJK, but ensures
        // no panic)
        let parts = vec!["世界".to_string()];
        assert!(matches_multi(None, &parts, "hello 世界"));
        assert!(!matches_multi(None, &parts, "hello world"));
    }

    #[test]
    fn default_state_matches_everything() {
        let f = FilterState::default();
        assert!(f.matches(&entry("any", "any message")));
    }

    #[test]
    fn min_level_system_includes_all_levels() {
        let f = FilterState::default();
        assert_eq!(f.min_level, LogLevel::System);
        for lvl in [
            LogLevel::System,
            LogLevel::Verbose,
            LogLevel::Debug,
            LogLevel::Info,
            LogLevel::Warning,
            LogLevel::Error,
        ] {
            let mut e = entry("t", "m");
            e.level = lvl;
            assert!(f.matches(&e));
        }
    }

    #[test]
    fn tag_filter_include_case_insensitive() {
        let mut f = FilterState::default();
        f.parse_tag_filter("Network");
        assert!(f.matches(&entry("network", "m")));
        assert!(f.matches(&entry("NETWORK", "m")));
    }

    #[test]
    fn tag_filter_exclude_case_insensitive() {
        let mut f = FilterState::default();
        f.parse_tag_filter("-Noise");
        assert!(!f.matches(&entry("noise", "m")));
        assert!(!f.matches(&entry("NOISE", "m")));
    }
}
