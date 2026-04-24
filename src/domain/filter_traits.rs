//! Shared traits for the three network filter enums.
//!
//! Phase 3 DOM-001: `StatusFilter`, `MethodFilter`, `ProtocolFilter` share
//! identical shape (an "all" sentinel variant plus specific variants that
//! the user cycles through by clicking a pill). This module formalises the
//! shape so UI code can cycle any of them via a single generic helper.

// Phase 3 DOM-001 introduces this trait alongside impls on the three network
// filter enums. Current consumers (event.rs) still dispatch via pill-id
// strings; future cleanup will route clicks through `FilterVariant::next`.
// Until then, tests exercise the trait but the bin build does not — hence
// the allow on the trait itself (impls are reachable via the library API).
#[allow(dead_code)]
/// Cycle-and-label shape shared by every network filter enum.
///
/// Each implementor must provide:
/// - [`FilterVariant::all`]: the "match everything" sentinel variant.
/// - [`FilterVariant::label`]: the short UI label shown inside the pill.
/// - [`FilterVariant::variants`]: the ordered list of variants; the first
///   entry is expected to be `all()` so cycling wraps to "match everything"
///   after the last specific variant.
/// - [`FilterVariant::next`]: cycle to the next variant in the `variants`
///   order, wrapping from the last back to the first.
pub trait FilterVariant: Sized + Copy + PartialEq + 'static {
    /// The "all-match" sentinel variant (must be `variants()[0]`).
    fn all() -> Self;

    /// Short UI label for the variant (e.g. "All", "HTTP", "GET", "200").
    fn label(&self) -> &'static str;

    /// Ordered list of variants. The first entry must be `all()`; subsequent
    /// entries define the click-cycle order.
    fn variants() -> &'static [Self];

    /// Cycle to the next variant in [`FilterVariant::variants`] order,
    /// wrapping from the last variant back to `all()`.
    fn next(&self) -> Self {
        let vs = Self::variants();
        let idx = vs.iter().position(|v| v == self).unwrap_or(0);
        vs[(idx + 1) % vs.len()]
    }
}
