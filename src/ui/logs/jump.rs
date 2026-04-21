//! Jump-to-bottom overlay — visibility decision + label helpers.

/// Returns true when the overlay should be shown.
/// Rule: shown whenever the list is not at its tail (auto_scroll == false).
pub fn should_show(auto_scroll: bool) -> bool {
    !auto_scroll
}

/// Returns the label text for the pill.
/// - no new logs: "  ↓ Jump to bottom  "
/// - N new since pause: "  ↓ Jump to bottom  N new  "
pub fn label(new_since_pause: usize) -> String {
    if new_since_pause == 0 {
        "  ↓ Jump to bottom  ".to_string()
    } else {
        format!("  ↓ Jump to bottom  {} new  ", new_since_pause)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_when_auto_scroll() {
        assert!(!should_show(true));
    }

    #[test]
    fn shown_when_paused() {
        assert!(should_show(false));
    }

    #[test]
    fn label_no_new() {
        assert_eq!(label(0), "  ↓ Jump to bottom  ");
    }

    #[test]
    fn label_with_new() {
        assert_eq!(label(42), "  ↓ Jump to bottom  42 new  ");
    }
}
