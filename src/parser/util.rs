//! Shared regex helpers for all parser strategies.
//!
//! Phase 3 Step 3.1 extraction — see Audit DOM-015. Previously the
//! ANSI-strip regex was duplicated in `generic.rs` and `structured.rs`.
//! Centralising here means a single source of truth; updates (e.g.,
//! supporting OSC-8 hyperlinks) happen in one place.

use regex::Regex;
use std::sync::LazyLock;

/// ANSI escape sequence matcher (CSI-style, e.g. `\x1b[31m`).
///
/// LazyLock regex compilation is deliberate — compiles on first use,
/// O(1) thereafter. Audit DOM-014 reviewed and approved.
pub static ANSI_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\x1b\[[0-9;]*m").unwrap());

/// Strip ANSI escape sequences from a string. Returns `Cow` so callers
/// don't allocate when no escapes are present.
pub fn strip_ansi(s: &str) -> std::borrow::Cow<'_, str> {
    ANSI_RE.replace_all(s, "")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_ansi_removes_csi_color() {
        assert_eq!(strip_ansi("\x1b[31mred\x1b[0m"), "red");
    }

    #[test]
    fn strip_ansi_handles_multiple_codes_in_one_line() {
        assert_eq!(
            strip_ansi("\x1b[1;31mbold red\x1b[0m plain \x1b[32mgreen"),
            "bold red plain green"
        );
    }

    #[test]
    fn strip_ansi_passes_through_when_no_codes() {
        assert_eq!(strip_ansi("plain text"), "plain text");
    }

    #[test]
    fn strip_ansi_handles_empty() {
        assert_eq!(strip_ansi(""), "");
    }

    #[test]
    fn strip_ansi_only_strips_csi_not_other_escapes() {
        // The regex is ESC[...m; OSC hyperlinks (ESC ] ...) are not
        // matched. Lock this behaviour — Phase 3+ extensions may add
        // OSC support.
        let osc = "\x1b]8;;https://example.com\x07link\x1b]8;;\x07";
        assert_eq!(strip_ansi(osc), osc);
    }

    #[test]
    fn ansi_re_is_shared_instance() {
        // Two is_match calls use the same compiled regex. This is
        // more documentation than test — it proves ANSI_RE is
        // reachable as a pub static.
        assert!(ANSI_RE.is_match("\x1b[0m"));
        assert!(!ANSI_RE.is_match("plain"));
    }
}
