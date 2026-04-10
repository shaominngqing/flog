//! Filtering for network entries by status, method, protocol, and search text.

use crate::domain::network::{NetworkEntry, NetworkStatus, Protocol};

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

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Pending => "Pending",
            Self::Active => "Active",
            Self::Completed => "Completed",
            Self::Failed => "Failed",
        }
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

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Delete => "DELETE",
            Self::Patch => "PATCH",
        }
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

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Http => "HTTP",
            Self::Sse => "SSE",
            Self::Ws => "WS",
        }
    }
}

pub struct NetworkFilter {
    pub status: StatusFilter,
    pub method: MethodFilter,
    pub protocol: ProtocolFilter,
    pub search: String,
}

impl NetworkFilter {
    pub fn new() -> Self {
        Self {
            status: StatusFilter::All,
            method: MethodFilter::All,
            protocol: ProtocolFilter::All,
            search: String::new(),
        }
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
            let search_lower = self.search.to_lowercase();
            let url_match = entry.url.to_lowercase().contains(&search_lower);
            let path_match = entry.path.to_lowercase().contains(&search_lower);
            if !url_match && !path_match {
                return false;
            }
        }
        true
    }

    pub fn is_active(&self) -> bool {
        self.status != StatusFilter::All
            || self.method != MethodFilter::All
            || self.protocol != ProtocolFilter::All
            || !self.search.is_empty()
    }

    pub fn reset(&mut self) {
        self.status = StatusFilter::All;
        self.method = MethodFilter::All;
        self.protocol = ProtocolFilter::All;
        self.search.clear();
    }
}
