//! Named string constants for clickable pills in the network detail panel.
//!
//! These labels are rendered by `src/ui/network/detail.rs` and consumed by
//! the click-detection code in `src/event/mod.rs` (via `.len()`). Keeping
//! them in one place prevents the magic `" Events ".len()` / `" Merged ".len()`
//! pattern (see audit UI-016) and makes any renderer rename an explicit
//! lockstep edit.
//!
//! Invariant: if the renderer changes a pill label, the constant below MUST
//! change too, otherwise click hit-boxes will drift.

/// SSE detail header pill: shows the raw per-chunk Events view.
pub(crate) const SSE_EVENTS_PILL: &str = " Events ";

/// SSE detail header pill: switches into the Merged-field view.
pub(crate) const SSE_MERGED_PILL: &str = " Merged ";

/// WS detail header pill: shows the chat-style grouped Chat view.
pub(crate) const WS_CHAT_PILL: &str = " Chat ";

/// WS detail header pill: shows the raw per-message Raw/List view.
pub(crate) const WS_LIST_PILL: &str = " Raw ";

/// One-character gap between consecutive pills (rendered as a literal space
/// in the label row — the click-detector adds this to compute the start of
/// the next pill).
pub(crate) const PILL_PADDING: usize = 1;

#[cfg(test)]
mod tests {
    use super::*;

    /// Guard against accidental whitespace edits to the SSE pill labels.
    /// If this test fails, either the renderer's label changed (update the
    /// constant AND the renderer in lockstep) or a stray edit slipped in.
    #[test]
    fn sse_pill_labels_unchanged() {
        assert_eq!(SSE_EVENTS_PILL, " Events ");
        assert_eq!(SSE_MERGED_PILL, " Merged ");
        assert_eq!(PILL_PADDING, 1);
    }

    /// Same guard for the WS chat pills. The raw view is labelled `" Raw "`
    /// in the renderer (see `ui/network/detail.rs`); keep the constant name
    /// `WS_LIST_PILL` but the value is " Raw ".
    #[test]
    fn ws_pill_labels_unchanged() {
        assert_eq!(WS_CHAT_PILL, " Chat ");
        assert_eq!(WS_LIST_PILL, " Raw ");
    }
}
