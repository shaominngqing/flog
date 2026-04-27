//! Mock rule editor state + entry/exit methods.
//!
//! Mock-edit state machine transitions (audit UI-028):
//! - `Normal` → `enter_mock_rules()` → toggles the rules-list side panel
//!   in `NetworkState`. Does **not** transition to `MockRuleEdit`.
//! - `Normal` → `enter_mock_edit(id)` → `MockRuleEdit` with a populated
//!   `MockEditState::from_rule(&rule)`.
//! - `MockRuleEdit` → `save_mock_edit()` → `Normal`, rule in `mock_rules`
//!   updated in place.
//! - `MockRuleEdit` → `cancel_mock_edit()` → `Normal`, `mock_rules`
//!   unchanged.

use super::{App, AppMode};

/// Mock rule editor state — bundles fields formerly scattered on `App`.
///
/// See audit UI-026 / UI-034. The editor has 5 logical fields indexed 0..5:
/// 0 = URL pattern, 1 = HTTP method, 2 = status code, 3 = delay ms,
/// 4 = response body (multi-line `TextEditor`).
///
/// `rule_id` == `None` means "new-rule draft"; `Some(id)` means "editing
/// the existing rule with that id".
pub struct MockEditState {
    /// `None` = new-rule draft; `Some(id)` = editing rule with that id.
    pub rule_id: Option<usize>,
    /// Currently-focused field index (0..5). 4 = body, 0..4 = top row.
    pub field: usize,
    /// String buffers for the 4 single-line fields: URL, method, status, delay.
    pub top_values: Vec<String>,
    /// Multi-line editor for the response body (field 5).
    pub body: crate::ui::text_editor::TextEditor,
}

impl MockEditState {
    /// Blank editor state — `rule_id: None` (new-rule draft), all fields empty.
    pub fn new_blank() -> Self {
        Self {
            rule_id: None,
            field: 0,
            top_values: Vec::new(),
            body: crate::ui::text_editor::TextEditor::new(""),
        }
    }

    /// Populate editor state from an existing `MockRule`. Flattens the
    /// previously-nested `enter_mock_rules` → overwrite dance (audit UI-034).
    ///
    /// JSON bodies are pretty-printed for readability; non-JSON bodies pass
    /// through unchanged.
    pub fn from_rule(rule: &crate::domain::mock::MockRule) -> Self {
        let pretty_body =
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&rule.response_body) {
                serde_json::to_string_pretty(&val).unwrap_or_else(|_| rule.response_body.clone())
            } else {
                rule.response_body.clone()
            };
        Self {
            rule_id: Some(rule.id),
            field: 0,
            top_values: vec![
                rule.url_pattern.clone(),
                rule.method.clone().unwrap_or_else(|| "*".to_string()),
                rule.status_code.to_string(),
                rule.delay_ms.to_string(),
            ],
            body: crate::ui::text_editor::TextEditor::new(&pretty_body),
        }
    }
}

impl Default for MockEditState {
    fn default() -> Self {
        Self::new_blank()
    }
}

impl App {
    // ── Mock rule state machine (audit UI-028) ──────────────────────────
    //
    //   Normal ──enter_mock_rules()─► Normal  (side-panel toggle only)
    //          │
    //          └─enter_mock_edit(id)─► MockRuleEdit
    //                                     │ save_mock_edit()    ─► Normal (rule updated)
    //                                     │ cancel_mock_edit()  ─► Normal (no change)
    //
    // `enter_mock_rules` is a NAME HOLDOVER (audit UI-022): it neither
    // enters MockRuleEdit mode nor opens an editor — it toggles the mock
    // rules list panel in the right sidebar. Renaming is deferred so this
    // step keeps to app-internal scope.
    //
    // `enter_mock_edit(id)` is the only path INTO MockRuleEdit. It loads
    // the existing rule via `MockEditState::from_rule(rule)` and sets
    // `mode = AppMode::MockRuleEdit`. If `id` doesn't exist it is a no-op.
    //
    // From MockRuleEdit:
    //   - `save_mock_edit()` writes back to `mock_rules`, clears rule_id,
    //     returns to Normal. The caller (event.rs Ctrl-S / Ctrl-Enter, and
    //     the mouse handler for the Save button) is responsible for then
    //     broadcasting via `ConnectorHandle::send_mock_sync`.
    //   - `cancel_mock_edit()` is the Esc / Cancel-button path — resets
    //     rule_id and drops back to Normal without touching `mock_rules`.

    /// Toggles the mock-rules list side panel on the Network tab.
    ///
    /// Despite the name (audit UI-022 ack — rename deferred), this does
    /// NOT enter `AppMode::MockRuleEdit`. No-op if no app is connected.
    pub fn enter_mock_rules(&mut self) {
        if !self.has_connected_client() {
            self.show_status("Mock unavailable — no client connected".to_string());
            return;
        }
        // Toggle mock rules panel in the right side (like detail panel)
        self.network.show_mock_rules_panel = !self.network.show_mock_rules_panel;
        if self.network.show_mock_rules_panel {
            self.network.show_detail = false; // hide detail when showing rules
        }
    }

    /// Enters `MockRuleEdit` with fields populated from the given rule.
    ///
    /// No-op if `rule_id` is not present in `mock_rules`. Flattened via
    /// [`MockEditState::from_rule`] per audit UI-034.
    pub fn enter_mock_edit(&mut self, rule_id: usize) {
        if let Some(rule) = self.mock_rules.rules().iter().find(|r| r.id == rule_id) {
            self.mock_edit = MockEditState::from_rule(rule);
            self.mode = AppMode::MockRuleEdit;
        }
    }

    /// Commits `mock_edit` back to `mock_rules` and returns to `Normal`.
    ///
    /// The caller is responsible for broadcasting the updated rules via
    /// `ConnectorHandle::send_mock_sync`.
    pub fn save_mock_edit(&mut self) {
        if let Some(id) = self.mock_edit.rule_id {
            if let Some(rule) = self.mock_rules.get_mut(id) {
                rule.url_pattern = self.mock_edit.top_values[0].clone();
                rule.method = if self.mock_edit.top_values[1] == "*" {
                    None
                } else {
                    Some(self.mock_edit.top_values[1].clone())
                };
                rule.status_code = self.mock_edit.top_values[2].parse().unwrap_or(200);
                rule.delay_ms = self.mock_edit.top_values[3].parse().unwrap_or(0);
                rule.response_body = self.mock_edit.body.content();
            }
        }
        self.mock_edit.rule_id = None;
        self.mode = AppMode::Normal;
    }

    /// Discards in-progress edits and returns to `Normal`. `mock_rules`
    /// stays unchanged.
    ///
    /// Save / cancel semantics (audit UI-027 ack): save writes changes and
    /// exits; cancel drops changes and exits. Both are unconditional —
    /// there is no "unsaved changes" prompt. The MockRuleEdit mode is the
    /// only place where rule edits live; once we leave Normal the edits
    /// are committed, and vice versa. Characterization tests
    /// (`cancel_mock_edit_discards_changes`,
    /// `save_mock_edit_updates_rule_and_exits_mode`) lock this flow.
    pub fn cancel_mock_edit(&mut self) {
        self.mock_edit.rule_id = None;
        self.mode = AppMode::Normal;
    }
}
