//! Mock rule types and matching for the proxy server.

/// A single mock rule that intercepts matching requests.
#[derive(Debug, Clone)]
pub struct MockRule {
    pub id: usize,
    /// Substring pattern matched against the request URL.
    pub url_pattern: String,
    /// Optional HTTP method filter (e.g. "GET", "POST").
    pub method: Option<String>,
    /// HTTP status code to return.
    pub status_code: u16,
    /// Response body to return.
    pub response_body: String,
    /// Optional delay in milliseconds before returning the response.
    pub delay_ms: u64,
    /// Whether this rule is active.
    pub enabled: bool,
    /// Number of times this rule has been matched.
    pub hit_count: u32,
}

/// Manages a collection of mock rules.
pub struct MockRuleStore {
    rules: Vec<MockRule>,
    next_id: usize,
}

impl MockRuleStore {
    pub fn new() -> Self {
        Self {
            rules: Vec::new(),
            next_id: 1,
        }
    }

    /// Add a new mock rule and return its ID.
    pub fn add(
        &mut self,
        url_pattern: String,
        method: Option<String>,
        status_code: u16,
        response_body: String,
        delay_ms: u64,
    ) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        self.rules.push(MockRule {
            id,
            url_pattern,
            method,
            status_code,
            response_body,
            delay_ms,
            enabled: true,
            hit_count: 0,
        });
        id
    }

    /// Find a matching rule for the given URL and method.
    /// Returns a clone of the matched rule (with hit_count incremented).
    pub fn find_match(&mut self, url: &str, method: &str) -> Option<MockRule> {
        for rule in self.rules.iter_mut() {
            if !rule.enabled {
                continue;
            }
            if !url.contains(&rule.url_pattern) {
                continue;
            }
            if let Some(ref m) = rule.method {
                if !m.eq_ignore_ascii_case(method) {
                    continue;
                }
            }
            rule.hit_count += 1;
            return Some(rule.clone());
        }
        None
    }

    /// Get all rules.
    pub fn rules(&self) -> &[MockRule] {
        &self.rules
    }

    /// Toggle a rule's enabled state by ID.
    pub fn toggle(&mut self, id: usize) {
        if let Some(rule) = self.rules.iter_mut().find(|r| r.id == id) {
            rule.enabled = !rule.enabled;
        }
    }

    /// Remove a rule by ID.
    pub fn remove(&mut self, id: usize) {
        self.rules.retain(|r| r.id != id);
    }

    /// Total number of rules.
    pub fn len(&self) -> usize {
        self.rules.len()
    }

    /// Number of enabled rules.
    pub fn enabled_count(&self) -> usize {
        self.rules.iter().filter(|r| r.enabled).count()
    }
}
