//! Pure SSE merged-field navigation.
//!
//! `sse_navigate_fields` computes the next field index given the current
//! index, total candidate count, and a direction. The function saturates
//! at `0` / `count - 1` so j/k don't wrap (preserving the original
//! Phase 2.5A behavior, locked in by the UI-008 characterization
//! tests). Returns `0` when `count == 0`.
//!
//! The plan's initial sketch specified wrap semantics, but the
//! characterization fence (`ui_008_sse_merged_j_saturates_at_max`,
//! `ui_008_sse_merged_k_saturates_at_zero`) documents the shipped
//! saturating behavior — so we keep it. The function is still "pure"
//! (no I/O, no mutation) which is what matters for testability.
//!
//! Extracted in Phase 3 Step 3.6 Task 6 (audit UI-008).

use super::click_region::ScrollDir;

#[allow(dead_code)] // wired up via Task 6 call-site replacement
pub(crate) fn sse_navigate_fields(current: usize, count: usize, dir: ScrollDir) -> usize {
    if count == 0 {
        return 0;
    }
    match dir {
        ScrollDir::Down => (current + 1).min(count - 1),
        ScrollDir::Up => current.saturating_sub(1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn down_increments_saturates_at_count_minus_one() {
        assert_eq!(sse_navigate_fields(0, 3, ScrollDir::Down), 1);
        assert_eq!(sse_navigate_fields(1, 3, ScrollDir::Down), 2);
        assert_eq!(sse_navigate_fields(2, 3, ScrollDir::Down), 2);
    }

    #[test]
    fn up_decrements_saturates_at_zero() {
        assert_eq!(sse_navigate_fields(2, 3, ScrollDir::Up), 1);
        assert_eq!(sse_navigate_fields(1, 3, ScrollDir::Up), 0);
        assert_eq!(sse_navigate_fields(0, 3, ScrollDir::Up), 0);
    }

    #[test]
    fn zero_count_returns_zero() {
        assert_eq!(sse_navigate_fields(0, 0, ScrollDir::Up), 0);
        assert_eq!(sse_navigate_fields(5, 0, ScrollDir::Down), 0);
    }
}
