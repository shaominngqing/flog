//! Filtering for network entries by status, method, protocol, and search text.

use crate::domain::filter::matches_multi;
use crate::domain::filter_traits::{FilterVariant, MessageFilter};
use crate::domain::network::{NetworkEntry, NetworkStatus, Protocol};
use regex::Regex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusFilter {
    All,
    Pending,
    Active,
    Completed,
    Failed,
}

impl StatusFilter {
    pub fn matches(&self, status: NetworkStatus) -> bool {
        match self {
            Self::All => true,
            Self::Pending => status == NetworkStatus::Pending,
            Self::Active => status == NetworkStatus::Active,
            Self::Completed => status == NetworkStatus::Completed,
            Self::Failed => status == NetworkStatus::Failed,
        }
    }
}

impl FilterVariant for StatusFilter {
    fn all() -> Self {
        Self::All
    }
    fn label(&self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Pending => "Pending",
            Self::Active => "Active",
            Self::Completed => "Completed",
            Self::Failed => "Failed",
        }
    }
    fn variants() -> &'static [Self] {
        &[
            Self::All,
            Self::Pending,
            Self::Active,
            Self::Completed,
            Self::Failed,
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MethodFilter {
    All,
    Get,
    Post,
    Put,
    Delete,
    Patch,
}

impl MethodFilter {
    pub fn matches(&self, method: &str) -> bool {
        match self {
            Self::All => true,
            Self::Get => method.eq_ignore_ascii_case("GET"),
            Self::Post => method.eq_ignore_ascii_case("POST"),
            Self::Put => method.eq_ignore_ascii_case("PUT"),
            Self::Delete => method.eq_ignore_ascii_case("DELETE"),
            Self::Patch => method.eq_ignore_ascii_case("PATCH"),
        }
    }
}

impl FilterVariant for MethodFilter {
    fn all() -> Self {
        Self::All
    }
    fn label(&self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Delete => "DEL",
            Self::Patch => "PATCH",
        }
    }
    fn variants() -> &'static [Self] {
        &[
            Self::All,
            Self::Get,
            Self::Post,
            Self::Put,
            Self::Delete,
            Self::Patch,
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolFilter {
    All,
    Http,
    Sse,
    Ws,
}

impl ProtocolFilter {
    pub fn matches(&self, protocol: Protocol) -> bool {
        match self {
            Self::All => true,
            Self::Http => protocol == Protocol::Http,
            Self::Sse => protocol == Protocol::Sse,
            Self::Ws => protocol == Protocol::Ws,
        }
    }
}

impl FilterVariant for ProtocolFilter {
    fn all() -> Self {
        Self::All
    }
    fn label(&self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Http => "HTTP",
            Self::Sse => "SSE",
            Self::Ws => "WS",
        }
    }
    fn variants() -> &'static [Self] {
        &[Self::All, Self::Http, Self::Sse, Self::Ws]
    }
}

pub struct NetworkFilter {
    pub status: StatusFilter,
    pub method: MethodFilter,
    pub protocol: ProtocolFilter,
    pub search: String,
    pub exclude: String,
    search_regex: Option<Regex>,
    search_plain: Vec<String>,
    exclude_regex: Option<Regex>,
    exclude_plain: Vec<String>,
}

impl NetworkFilter {
    pub fn new() -> Self {
        Self {
            status: StatusFilter::All,
            method: MethodFilter::All,
            protocol: ProtocolFilter::All,
            search: String::new(),
            exclude: String::new(),
            search_regex: None,
            search_plain: Vec::new(),
            exclude_regex: None,
            exclude_plain: Vec::new(),
        }
    }

    fn compile_query(query: &str) -> (Option<Regex>, Vec<String>) {
        if let Some(body) = query.strip_prefix('/') {
            let (pattern, ci) = if let Some(p) = body.strip_suffix("/i") {
                (p, true)
            } else if let Some(p) = body.strip_suffix('/') {
                (p, false)
            } else {
                (body, false)
            };
            let full = if ci {
                format!("(?i){}", pattern)
            } else {
                pattern.to_string()
            };
            return (Regex::new(&full).ok(), Vec::new());
        }
        let parts: Vec<String> = query
            .split('|')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        (None, parts)
    }

    pub fn set_search(&mut self, query: &str) {
        self.search = query.to_string();
        let (re, parts) = Self::compile_query(query);
        self.search_regex = re;
        self.search_plain = parts;
    }

    pub fn set_exclude(&mut self, query: &str) {
        self.exclude = query.to_string();
        let (re, parts) = Self::compile_query(query);
        self.exclude_regex = re;
        self.exclude_plain = parts;
    }

    pub fn matches(&self, entry: &NetworkEntry) -> bool {
        if !self.status.matches(entry.status) {
            return false;
        }
        if !self.method.matches(&entry.method) {
            return false;
        }
        if !self.protocol.matches(entry.protocol) {
            return false;
        }
        if !self.search.is_empty() {
            let url_hit = matches_multi(self.search_regex.as_ref(), &self.search_plain, &entry.url);
            let path_hit =
                matches_multi(self.search_regex.as_ref(), &self.search_plain, &entry.path);
            if !url_hit && !path_hit {
                return false;
            }
        }
        if !self.exclude.is_empty() {
            let url_hit =
                matches_multi(self.exclude_regex.as_ref(), &self.exclude_plain, &entry.url);
            let path_hit = matches_multi(
                self.exclude_regex.as_ref(),
                &self.exclude_plain,
                &entry.path,
            );
            if url_hit || path_hit {
                return false;
            }
        }
        true
    }

    pub fn reset(&mut self) {
        self.status = StatusFilter::All;
        self.method = MethodFilter::All;
        self.protocol = ProtocolFilter::All;
        self.search.clear();
        self.exclude.clear();
        self.search_regex = None;
        self.search_plain.clear();
        self.exclude_regex = None;
        self.exclude_plain.clear();
    }
}

impl Default for NetworkFilter {
    fn default() -> Self {
        Self::new()
    }
}

impl MessageFilter<NetworkEntry> for NetworkFilter {
    fn matches(&self, item: &NetworkEntry) -> bool {
        // Delegate to the inherent method to keep method-syntax call
        // sites working.
        NetworkFilter::matches(self, item)
    }
}

#[cfg(test)]
#[path = "network_filter_tests.rs"]
mod tests;
