//! Mock rule types and matching for the interceptor-based mock system.
//!
//! DOM-007 acknowledged (Phase 3 Step 3.2): mock rules live here rather
//! than being merged with SSE/WS extensions because they are
//! HTTP-specific and syncable over the wire (`ClientMessage::MockSync`
//! over the direct WebSocket channel, not a VM Service extension).
//! Co-locating the three protocol extensions would couple orthogonal
//! concerns. Keep as-is.

use serde::Serialize;

/// A single mock rule that intercepts matching requests.
#[derive(Debug, Clone, Serialize)]
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
    #[serde(skip)]
    #[allow(dead_code)]
    pub hit_count: u32,
}

/// Manages a collection of mock rules.
pub struct MockRuleStore {
    rules: Vec<MockRule>,
    next_id: usize,
}

impl Default for MockRuleStore {
    fn default() -> Self {
        Self {
            rules: Vec::new(),
            next_id: 1,
        }
    }
}

impl MockRuleStore {
    pub fn new() -> Self {
        Self::default()
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
    #[allow(dead_code)]
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

    /// Whether the store has no rules.
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// Get a mutable reference to a rule by ID.
    pub fn get_mut(&mut self, id: usize) -> Option<&mut MockRule> {
        self.rules.iter_mut().find(|r| r.id == id)
    }

    /// Number of enabled rules.
    #[cfg(test)]
    pub fn enabled_count(&self) -> usize {
        self.rules.iter().filter(|r| r.enabled).count()
    }

    /// Serialize all rules to a JSON string for syncing to Dart.
    pub fn to_json_string(&self) -> String {
        serde_json::to_string(&self.rules).unwrap_or_else(|_| "[]".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_rule() {
        let mut store = MockRuleStore::new();
        let id = store.add("/api/users".into(), Some("GET".into()), 200, "[]".into(), 0);
        assert_eq!(store.len(), 1);
        assert_eq!(store.enabled_count(), 1);
        assert!(id > 0);
    }

    #[test]
    fn test_find_match_by_url() {
        let mut store = MockRuleStore::new();
        store.add("/api/users".into(), None, 200, "[]".into(), 0);

        let matched = store.find_match("https://example.com/api/users?page=1", "GET");
        assert!(matched.is_some());
        assert_eq!(matched.unwrap().status_code, 200);
    }

    #[test]
    fn test_find_match_respects_method() {
        let mut store = MockRuleStore::new();
        store.add(
            "/api/users".into(),
            Some("POST".into()),
            201,
            "{}".into(),
            0,
        );

        // GET should NOT match
        let matched = store.find_match("https://example.com/api/users", "GET");
        assert!(matched.is_none());

        // POST should match
        let matched = store.find_match("https://example.com/api/users", "POST");
        assert!(matched.is_some());
        assert_eq!(matched.unwrap().status_code, 201);
    }

    #[test]
    fn test_toggle_rule() {
        let mut store = MockRuleStore::new();
        let id = store.add("/api".into(), None, 200, "".into(), 0);
        assert_eq!(store.enabled_count(), 1);

        store.toggle(id);
        assert_eq!(store.enabled_count(), 0);

        // Disabled rule should not match
        let matched = store.find_match("/api/test", "GET");
        assert!(matched.is_none());

        store.toggle(id);
        assert_eq!(store.enabled_count(), 1);
    }

    #[test]
    fn test_remove_rule() {
        let mut store = MockRuleStore::new();
        let id = store.add("/api".into(), None, 200, "".into(), 0);
        assert_eq!(store.len(), 1);

        store.remove(id);
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn test_hit_count_increments() {
        let mut store = MockRuleStore::new();
        store.add("/api".into(), None, 200, "".into(), 0);

        store.find_match("/api/test", "GET");
        store.find_match("/api/other", "POST");

        assert_eq!(store.rules()[0].hit_count, 2);
    }

    #[test]
    fn test_to_json_string() {
        let mut store = MockRuleStore::new();
        store.add("/api/users".into(), Some("GET".into()), 200, "[]".into(), 0);
        let json = store.to_json_string();
        assert!(json.contains("url_pattern"));
        assert!(json.contains("/api/users"));
        // hit_count should be skipped
        assert!(!json.contains("hit_count"));
    }
}
