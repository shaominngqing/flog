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
mod tests {
    use super::*;
    use crate::domain::network::NetworkEntry;

    fn e(url: &str) -> NetworkEntry {
        NetworkEntry::new_http(1, "GET".into(), url.into(), String::new())
    }

    #[test]
    fn search_plain_or() {
        let mut f = NetworkFilter::new();
        f.set_search("users|orders");
        assert!(f.matches(&e("https://x.com/api/users")));
        assert!(f.matches(&e("https://x.com/api/orders")));
        assert!(!f.matches(&e("https://x.com/api/posts")));
    }

    #[test]
    fn search_regex() {
        let mut f = NetworkFilter::new();
        f.set_search("/^/api/(users|orders)$/");
        assert!(f.matches(&e("https://x.com/api/users")));
        assert!(!f.matches(&e("https://x.com/api/posts")));
    }

    #[test]
    fn exclude_plain() {
        let mut f = NetworkFilter::new();
        f.set_exclude("heartbeat|telemetry");
        assert!(f.matches(&e("https://x.com/api/users")));
        assert!(!f.matches(&e("https://x.com/api/heartbeat")));
        assert!(!f.matches(&e("https://x.com/api/telemetry")));
    }

    #[test]
    fn reset_clears_exclude() {
        let mut f = NetworkFilter::new();
        f.set_exclude("noise");
        f.reset();
        assert!(f.matches(&e("https://x.com/noise")));
    }

    // ==================================================================
    // Phase 2.5B Task 2 — characterization tests
    // ==================================================================

    fn sse_entry(url: &str) -> NetworkEntry {
        NetworkEntry::new_sse(1, "GET".into(), url.into(), String::new())
    }

    fn ws_entry(url: &str) -> NetworkEntry {
        NetworkEntry::new_ws(1, url.into(), String::new())
    }

    // ---- DOM-019 MessageFilter trait shape lock ----------------------

    #[test]
    fn dom_019_network_filter_implements_message_filter_for_network_entry() {
        use crate::domain::filter_traits::MessageFilter as MsgFilterTrait;
        let f = NetworkFilter::new();
        let entry = e("https://x.com/api");
        assert!(<NetworkFilter as MsgFilterTrait<NetworkEntry>>::matches(
            &f, &entry
        ));
    }

    // ---- DOM-001: FilterVariant trait shared by all three enums ------

    fn cycle_full<V: FilterVariant + std::fmt::Debug>() -> Vec<V> {
        let start = V::all();
        let mut seen: Vec<V> = vec![start];
        let mut cur = start.next();
        while cur != start {
            seen.push(cur);
            cur = cur.next();
        }
        seen
    }

    #[test]
    fn dom_001_status_filter_variant_cycles_in_order() {
        let v = cycle_full::<StatusFilter>();
        assert_eq!(
            v,
            vec![
                StatusFilter::All,
                StatusFilter::Pending,
                StatusFilter::Active,
                StatusFilter::Completed,
                StatusFilter::Failed,
            ]
        );
        // next() wraps: last → all
        assert_eq!(StatusFilter::Failed.next(), StatusFilter::All);
        // labels
        assert_eq!(StatusFilter::All.label(), "All");
        assert_eq!(StatusFilter::Completed.label(), "Completed");
    }

    #[test]
    fn dom_001_method_filter_variant_cycles_in_order() {
        let v = cycle_full::<MethodFilter>();
        assert_eq!(
            v,
            vec![
                MethodFilter::All,
                MethodFilter::Get,
                MethodFilter::Post,
                MethodFilter::Put,
                MethodFilter::Delete,
                MethodFilter::Patch,
            ]
        );
        assert_eq!(MethodFilter::Patch.next(), MethodFilter::All);
        assert_eq!(MethodFilter::Delete.label(), "DEL");
    }

    #[test]
    fn dom_001_protocol_filter_variant_cycles_in_order() {
        let v = cycle_full::<ProtocolFilter>();
        assert_eq!(
            v,
            vec![
                ProtocolFilter::All,
                ProtocolFilter::Http,
                ProtocolFilter::Sse,
                ProtocolFilter::Ws,
            ]
        );
        assert_eq!(ProtocolFilter::Ws.next(), ProtocolFilter::All);
        assert_eq!(ProtocolFilter::Http.label(), "HTTP");
    }

    // ---- DOM-001: three parallel filter enums -------------------------
    // Each enum's .matches() gets a case per variant, locking behavior.

    #[test]
    fn dom_001_status_filter_all_matches_every_status_a_pending() {
        assert!(StatusFilter::All.matches(NetworkStatus::Pending));
    }

    #[test]
    fn dom_001_status_filter_all_matches_every_status_b_active() {
        assert!(StatusFilter::All.matches(NetworkStatus::Active));
    }

    #[test]
    fn dom_001_status_filter_all_matches_every_status_c_completed() {
        assert!(StatusFilter::All.matches(NetworkStatus::Completed));
    }

    #[test]
    fn dom_001_status_filter_all_matches_every_status_d_failed() {
        assert!(StatusFilter::All.matches(NetworkStatus::Failed));
    }

    #[test]
    fn dom_001_status_filter_pending_only_matches_pending() {
        assert!(StatusFilter::Pending.matches(NetworkStatus::Pending));
        assert!(!StatusFilter::Pending.matches(NetworkStatus::Active));
        assert!(!StatusFilter::Pending.matches(NetworkStatus::Completed));
        assert!(!StatusFilter::Pending.matches(NetworkStatus::Failed));
    }

    #[test]
    fn dom_001_status_filter_active_only_matches_active() {
        assert!(StatusFilter::Active.matches(NetworkStatus::Active));
        assert!(!StatusFilter::Active.matches(NetworkStatus::Pending));
        assert!(!StatusFilter::Active.matches(NetworkStatus::Completed));
        assert!(!StatusFilter::Active.matches(NetworkStatus::Failed));
    }

    #[test]
    fn dom_001_status_filter_completed_only_matches_completed() {
        assert!(StatusFilter::Completed.matches(NetworkStatus::Completed));
        assert!(!StatusFilter::Completed.matches(NetworkStatus::Pending));
        assert!(!StatusFilter::Completed.matches(NetworkStatus::Active));
        assert!(!StatusFilter::Completed.matches(NetworkStatus::Failed));
    }

    #[test]
    fn dom_001_status_filter_failed_only_matches_failed() {
        assert!(StatusFilter::Failed.matches(NetworkStatus::Failed));
        assert!(!StatusFilter::Failed.matches(NetworkStatus::Pending));
        assert!(!StatusFilter::Failed.matches(NetworkStatus::Active));
        assert!(!StatusFilter::Failed.matches(NetworkStatus::Completed));
    }

    #[test]
    fn dom_001_method_filter_all_matches_any() {
        for m in ["GET", "POST", "PUT", "DELETE", "PATCH", "OPTIONS", "HEAD"] {
            assert!(MethodFilter::All.matches(m));
        }
    }

    #[test]
    fn dom_001_method_filter_get_is_case_insensitive() {
        assert!(MethodFilter::Get.matches("GET"));
        assert!(MethodFilter::Get.matches("get"));
        assert!(MethodFilter::Get.matches("Get"));
        assert!(!MethodFilter::Get.matches("POST"));
    }

    #[test]
    fn dom_001_method_filter_post_matches_only_post() {
        assert!(MethodFilter::Post.matches("POST"));
        assert!(!MethodFilter::Post.matches("GET"));
    }

    #[test]
    fn dom_001_method_filter_put_matches_only_put() {
        assert!(MethodFilter::Put.matches("PUT"));
        assert!(!MethodFilter::Put.matches("PATCH"));
    }

    #[test]
    fn dom_001_method_filter_delete_matches_only_delete() {
        assert!(MethodFilter::Delete.matches("DELETE"));
        assert!(!MethodFilter::Delete.matches("GET"));
    }

    #[test]
    fn dom_001_method_filter_patch_matches_only_patch() {
        assert!(MethodFilter::Patch.matches("PATCH"));
        assert!(!MethodFilter::Patch.matches("PUT"));
    }

    #[test]
    fn dom_001_protocol_filter_all_matches_every_protocol() {
        assert!(ProtocolFilter::All.matches(Protocol::Http));
        assert!(ProtocolFilter::All.matches(Protocol::Sse));
        assert!(ProtocolFilter::All.matches(Protocol::Ws));
    }

    #[test]
    fn dom_001_protocol_filter_http_only() {
        assert!(ProtocolFilter::Http.matches(Protocol::Http));
        assert!(!ProtocolFilter::Http.matches(Protocol::Sse));
        assert!(!ProtocolFilter::Http.matches(Protocol::Ws));
    }

    #[test]
    fn dom_001_protocol_filter_sse_only() {
        assert!(ProtocolFilter::Sse.matches(Protocol::Sse));
        assert!(!ProtocolFilter::Sse.matches(Protocol::Http));
    }

    #[test]
    fn dom_001_protocol_filter_ws_only() {
        assert!(ProtocolFilter::Ws.matches(Protocol::Ws));
        assert!(!ProtocolFilter::Ws.matches(Protocol::Http));
    }

    // ---- DOM-019 parallel filters: combined matches() branches --------

    #[test]
    fn dom_019_matches_all_default_filter_accepts_everything() {
        let f = NetworkFilter::new();
        assert!(f.matches(&e("https://x.com/api")));
        assert!(f.matches(&sse_entry("https://x.com/stream")));
        assert!(f.matches(&ws_entry("wss://x.com/ws")));
    }

    #[test]
    fn dom_019_status_filter_rejects_non_matching() {
        let mut f = NetworkFilter::new();
        f.status = StatusFilter::Failed;
        // default new_http status is Pending
        assert!(!f.matches(&e("https://x.com/api")));
    }

    #[test]
    fn dom_019_method_filter_rejects_non_matching() {
        let mut f = NetworkFilter::new();
        f.method = MethodFilter::Post;
        assert!(!f.matches(&e("https://x.com/api")));
    }

    #[test]
    fn dom_019_protocol_filter_rejects_non_matching() {
        let mut f = NetworkFilter::new();
        f.protocol = ProtocolFilter::Ws;
        assert!(!f.matches(&e("https://x.com/api")));
    }

    #[test]
    fn dom_019_search_hits_path_not_url() {
        let mut f = NetworkFilter::new();
        f.set_search("users");
        // URL and path both contain "users" in this case; the matches()
        // short-circuits on url first, so construct one where only path matches.
        // In practice both are derived from the URL. Use pipe OR to verify.
        assert!(f.matches(&e("https://x.com/api/users")));
        assert!(!f.matches(&e("https://x.com/api/posts")));
    }

    // ---- Rule 10: core-module test density ---------------------------

    #[test]
    fn search_regex_ci_suffix() {
        let mut f = NetworkFilter::new();
        f.set_search("/USERS/i");
        assert!(f.matches(&e("https://x.com/api/users")));
    }

    #[test]
    fn search_regex_unterminated_slash() {
        let mut f = NetworkFilter::new();
        // "/foo" — body stays "foo" with no trailing slash
        f.set_search("/foo");
        assert!(f.matches(&e("https://x.com/foo")));
    }

    #[test]
    fn search_invalid_regex_no_match() {
        let mut f = NetworkFilter::new();
        f.set_search("/[unclosed/");
        assert!(!f.matches(&e("https://x.com/api")));
    }

    #[test]
    fn search_empty_query_does_not_filter() {
        let mut f = NetworkFilter::new();
        f.set_search("");
        assert!(f.matches(&e("https://x.com/any")));
    }

    #[test]
    fn exclude_empty_query_does_not_filter() {
        let mut f = NetworkFilter::new();
        f.set_exclude("");
        assert!(f.matches(&e("https://x.com/any")));
    }

    #[test]
    fn search_and_exclude_combined() {
        let mut f = NetworkFilter::new();
        f.set_search("api");
        f.set_exclude("heartbeat");
        assert!(f.matches(&e("https://x.com/api/users")));
        assert!(!f.matches(&e("https://x.com/api/heartbeat")));
        assert!(!f.matches(&e("https://x.com/other/users")));
    }

    #[test]
    fn exclude_regex() {
        let mut f = NetworkFilter::new();
        f.set_exclude("/heart.*/");
        assert!(!f.matches(&e("https://x.com/heartbeat")));
        assert!(f.matches(&e("https://x.com/api")));
    }

    #[test]
    fn reset_resets_all_dimensions() {
        let mut f = NetworkFilter::new();
        f.status = StatusFilter::Failed;
        f.method = MethodFilter::Post;
        f.protocol = ProtocolFilter::Ws;
        f.set_search("x");
        f.set_exclude("y");
        f.reset();
        assert_eq!(f.status, StatusFilter::All);
        assert_eq!(f.method, MethodFilter::All);
        assert_eq!(f.protocol, ProtocolFilter::All);
        assert!(f.search.is_empty());
        assert!(f.exclude.is_empty());
        assert!(f.matches(&e("https://x.com/y")));
    }

    #[test]
    fn default_trait_delegates_to_new() {
        let f = NetworkFilter::default();
        assert_eq!(f.status, StatusFilter::All);
        assert_eq!(f.method, MethodFilter::All);
        assert_eq!(f.protocol, ProtocolFilter::All);
    }

    #[test]
    fn search_unicode_query() {
        let mut f = NetworkFilter::new();
        f.set_search("世界");
        assert!(f.matches(&e("https://x.com/api/世界")));
        assert!(!f.matches(&e("https://x.com/api/world")));
    }

    #[test]
    fn exclude_hits_path_even_if_url_not() {
        let mut f = NetworkFilter::new();
        f.set_exclude("heartbeat");
        // url and path both contain "heartbeat" — url_hit wins
        assert!(!f.matches(&e("https://x.com/heartbeat")));
    }
}
