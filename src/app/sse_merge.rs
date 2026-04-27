//! SSE Merged View rule types + the count helper exposed to the event
//! layer for j/k navigation (audit UI-008).

use super::App;

/// A segment in a JSON field path.
#[derive(Clone, Debug, PartialEq)]
pub enum SsePathSegment {
    Key(String),
    Index(usize),
}

/// A saved SSE merge rule: which JSON field path to concatenate across chunks.
#[derive(Clone)]
pub struct SseMergeRule {
    /// JSON field path like `["choices", 0, "delta", "content"]`
    pub field_path: Vec<SsePathSegment>,
    /// Human-readable path string like `choices[0].delta.content`
    pub field_display: String,
}

impl App {
    /// Count of candidate fields available in SSE merged mode for the
    /// currently selected entry, or 0 when no entry is selected or it
    /// is not SSE. Used by the j/k navigation call-site (UI-008).
    pub fn sse_merged_field_count(&mut self) -> usize {
        let sel = self.network.selected;
        let indices = self.network.filtered_indices(&self.network_store).to_vec();
        let Some(&idx) = indices.get(sel) else {
            return 0;
        };
        let Some(entry) = self.network_store.get(idx) else {
            return 0;
        };
        if entry.protocol != crate::domain::network::Protocol::Sse {
            return 0;
        }
        let chunks_data: Vec<&str> = entry.sse_chunks.iter().map(|c| c.data.as_str()).collect();
        crate::domain::sse_merge::extract_field_paths(&chunks_data).len()
    }
}
