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
            tag_regex: false,
            compiled_tag_include: Vec::new(),
            compiled_tag_exclude: Vec::new(),
        }
    }
}

impl FilterState {
    /// 设置搜索查询，自动检测正则模式
    pub fn set_search(&mut self, query: &str) {
        if let Some(regex_body) = query.strip_prefix('/') {
            let (pattern, case_insensitive) = if let Some(p) = regex_body.strip_suffix("/i") {
                (p, true)
            } else if let Some(p) = regex_body.strip_suffix('/') {
                (p, false)
            } else {
                (regex_body, false)
            };
            let full_pattern = if case_insensitive {
                format!("(?i){}", pattern)
            } else {
                pattern.to_string()
            };
            self.search_regex = true;
            self.compiled_regex = Regex::new(&full_pattern).ok();
            self.search_query = query.to_string();
        } else {
            self.search_regex = false;
            self.compiled_regex = None;
            self.search_query = query.to_string();
        }
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
                self.tag_include.iter().any(|inc| tag_lower == inc.to_lowercase())
            };
            if !matched {
                return false;
            }
        }

        // 搜索过滤
        if !self.search_query.is_empty() {
            let full = entry.full_message();
            if self.search_regex {
                if let Some(ref re) = self.compiled_regex {
                    if !re.is_match(&full) && !re.is_match(tag) {
                        return false;
                    }
                }
            } else {
                let query_lower = self.search_query.to_lowercase();
                let msg_lower = full.to_lowercase();
                let tag_lower = tag.to_lowercase();
                if !msg_lower.contains(&query_lower) && !tag_lower.contains(&query_lower) {
                    return false;
                }
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

        let query_lower = self.search_query.to_lowercase();
        let text_lower = text.to_lowercase();
        let mut positions = Vec::new();
        let mut start = 0;

        while let Some(pos) = text_lower[start..].find(&query_lower) {
            let abs_start = start + pos;
            let abs_end = abs_start + query_lower.len();
            positions.push(abs_start..abs_end);
            start = abs_end;
        }

        positions
    }

    /// 解析 Tag 过滤输入字符串，预编译正则
    pub fn parse_tag_filter(&mut self, input: &str) {
        self.tag_include.clear();
        self.tag_exclude.clear();
        self.compiled_tag_include.clear();
        self.compiled_tag_exclude.clear();
        self.tag_regex = input.contains('*') || input.contains('.');

        for part in input.split(',') {
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
                self.tag_include.push(trimmed.to_string());
                if self.tag_regex {
                    if let Ok(re) = Regex::new(&format!("(?i){}", trimmed)) {
                        self.compiled_tag_include.push(re);
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
        self.tag_regex = false;
        self.min_level = LogLevel::System;
    }
}
