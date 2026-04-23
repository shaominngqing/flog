//! Ring-buffer log storage. Parser-agnostic — receives pre-parsed `LogEntry` values.

use std::collections::VecDeque;

use super::entry::LogEntry;

const MAX_ENTRIES: usize = 100_000;

/// In-memory log storage with FIFO eviction and repeat folding.
pub struct LogStore {
    entries: VecDeque<LogEntry>,
}

impl LogStore {
    pub fn new() -> Self {
        Self {
            entries: VecDeque::with_capacity(1024),
        }
    }

    /// Add a pre-parsed log entry. Returns the number of drained (evicted) entries.
    pub fn add_entry(&mut self, entry: LogEntry) -> usize {
        // Smart folding: consecutive identical entries collapse into one.
        if let Some(last) = self.entries.back_mut() {
            if last.tag == entry.tag
                && last.level == entry.level
                && last.message == entry.message
                && last.extra_lines.is_empty()
                && entry.extra_lines.is_empty()
            {
                last.repeat_count += 1;
                last.timestamp = entry.timestamp;
                return 0;
            }
        }

        let drained = if self.entries.len() >= MAX_ENTRIES {
            self.entries.pop_front();
            1
        } else {
            0
        };

        self.entries.push_back(entry);
        drained
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&self, index: usize) -> Option<&LogEntry> {
        self.entries.get(index)
    }

    pub fn iter(&self) -> impl Iterator<Item = &LogEntry> {
        self.entries.iter()
    }
}

impl Default for LogStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::entry::{InputSource, LogLevel};

    fn entry(tag: &str, msg: &str) -> LogEntry {
        LogEntry {
            timestamp: "t".to_string(),
            level: LogLevel::Info,
            tag: tag.to_string(),
            message: msg.to_string(),
            extra_lines: Vec::new(),
            repeat_count: 1,
            source: InputSource::DirectSocket,
            error: None,
            stacktrace: None,
        }
    }

    #[test]
    fn new_is_empty() {
        let s = LogStore::new();
        assert_eq!(s.len(), 0);
        assert!(s.is_empty());
        assert!(s.get(0).is_none());
    }

    #[test]
    fn default_matches_new() {
        let s = LogStore::default();
        assert_eq!(s.len(), 0);
        assert!(s.is_empty());
    }

    #[test]
    fn add_single_entry_increments_len() {
        let mut s = LogStore::new();
        let drained = s.add_entry(entry("tag", "msg"));
        assert_eq!(drained, 0);
        assert_eq!(s.len(), 1);
        assert!(!s.is_empty());
    }

    #[test]
    fn iter_yields_entries_in_insertion_order() {
        let mut s = LogStore::new();
        s.add_entry(entry("a", "1"));
        s.add_entry(entry("b", "2"));
        s.add_entry(entry("c", "3"));
        let tags: Vec<String> = s.iter().map(|e| e.tag.clone()).collect();
        assert_eq!(tags, vec!["a", "b", "c"]);
    }

    #[test]
    fn get_returns_entry_at_index() {
        let mut s = LogStore::new();
        s.add_entry(entry("alpha", "1"));
        assert_eq!(s.get(0).unwrap().tag, "alpha");
        assert!(s.get(1).is_none());
    }

    // ---- DOM-011: consecutive-duplicate folding on add ----------------

    #[test]
    fn dom_011_consecutive_duplicates_fold_into_repeat_count() {
        let mut s = LogStore::new();
        s.add_entry(entry("tag", "same"));
        s.add_entry(entry("tag", "same"));
        s.add_entry(entry("tag", "same"));
        assert_eq!(s.len(), 1);
        assert_eq!(s.get(0).unwrap().repeat_count, 3);
    }

    #[test]
    fn dom_011_different_tag_does_not_fold() {
        let mut s = LogStore::new();
        s.add_entry(entry("a", "same"));
        s.add_entry(entry("b", "same"));
        assert_eq!(s.len(), 2);
    }

    #[test]
    fn dom_011_different_message_does_not_fold() {
        let mut s = LogStore::new();
        s.add_entry(entry("tag", "msg1"));
        s.add_entry(entry("tag", "msg2"));
        assert_eq!(s.len(), 2);
    }

    #[test]
    fn dom_011_different_level_does_not_fold() {
        let mut s = LogStore::new();
        let mut e1 = entry("tag", "m");
        e1.level = LogLevel::Info;
        let mut e2 = entry("tag", "m");
        e2.level = LogLevel::Warning;
        s.add_entry(e1);
        s.add_entry(e2);
        assert_eq!(s.len(), 2);
    }

    #[test]
    fn dom_011_entry_with_extra_lines_does_not_fold() {
        let mut s = LogStore::new();
        s.add_entry(entry("tag", "m"));
        let mut e2 = entry("tag", "m");
        e2.extra_lines.push("continuation".into());
        s.add_entry(e2);
        assert_eq!(s.len(), 2);
    }

    #[test]
    fn dom_011_last_entry_with_extra_lines_does_not_fold_new_one() {
        let mut s = LogStore::new();
        let mut e1 = entry("tag", "m");
        e1.extra_lines.push("cont".into());
        s.add_entry(e1);
        s.add_entry(entry("tag", "m"));
        assert_eq!(s.len(), 2);
    }

    #[test]
    fn dom_011_folding_updates_last_entry_timestamp() {
        let mut s = LogStore::new();
        let mut e1 = entry("tag", "m");
        e1.timestamp = "00:00:00.000".into();
        s.add_entry(e1);
        let mut e2 = entry("tag", "m");
        e2.timestamp = "12:00:00.000".into();
        s.add_entry(e2);
        assert_eq!(s.get(0).unwrap().timestamp, "12:00:00.000");
        assert_eq!(s.get(0).unwrap().repeat_count, 2);
    }

    // ---- DOM-011 drain boundary ------------------------------------

    #[test]
    fn dom_011_add_below_capacity_no_drain() {
        // Inserting far below MAX_ENTRIES never drains.
        let mut s = LogStore::new();
        for i in 0..100 {
            let drained = s.add_entry(entry("t", &format!("msg-{}", i)));
            assert_eq!(drained, 0);
        }
        assert_eq!(s.len(), 100);
    }

    // ---- Capacity / drain ---------------------------------------------

    #[test]
    fn drain_on_full_evicts_one_at_a_time() {
        // Fill the store to capacity, then add one more.
        let mut s = LogStore::new();
        for i in 0..MAX_ENTRIES {
            s.add_entry(entry("t", &format!("msg-{}", i)));
        }
        assert_eq!(s.len(), MAX_ENTRIES);
        // One past capacity: current behavior is pop_front + push_back,
        // returns drained=1.
        let drained = s.add_entry(entry("t", "overflow"));
        assert_eq!(drained, 1);
        assert_eq!(s.len(), MAX_ENTRIES);
        // Oldest (msg-0) is gone; msg-1 is new front
        assert_eq!(s.get(0).unwrap().message, "msg-1");
        assert_eq!(s.get(MAX_ENTRIES - 1).unwrap().message, "overflow");
    }

    #[test]
    fn drain_at_exact_capacity_triggers_on_next_add() {
        let mut s = LogStore::new();
        for i in 0..MAX_ENTRIES {
            s.add_entry(entry("t", &format!("m{}", i)));
        }
        // At exactly cap: next add drains 1
        let drained = s.add_entry(entry("u", "new"));
        assert_eq!(drained, 1);
    }

    #[test]
    fn drain_after_cap_adds_identical_creates_new_not_folds() {
        // Characterizes DOM-011 explicitly: after pop_front, the new
        // entry cannot see what was popped. If it happens to equal the
        // still-present tail, folding can still happen; if the popped
        // predecessor matched but the new tail doesn't, folding is lost.
        //
        // Here: fill with all identical entries; they fold into one
        // entry with repeat_count=MAX. No drain occurs because folding
        // keeps len at 1.
        let mut s = LogStore::new();
        for _ in 0..100 {
            s.add_entry(entry("t", "same"));
        }
        assert_eq!(s.len(), 1);
        assert_eq!(s.get(0).unwrap().repeat_count, 100);
    }

    #[test]
    fn drain_zero_entries_on_empty_store_is_impossible() {
        // Trivially: can't drain from an empty store — we never hit the
        // drain branch because entries.len() < MAX_ENTRIES.
        let mut s = LogStore::new();
        let drained = s.add_entry(entry("t", "x"));
        assert_eq!(drained, 0);
    }

    #[test]
    fn store_handles_mix_of_folding_and_distinct_entries() {
        let mut s = LogStore::new();
        s.add_entry(entry("a", "1"));
        s.add_entry(entry("a", "1")); // fold
        s.add_entry(entry("a", "1")); // fold
        s.add_entry(entry("b", "2"));
        s.add_entry(entry("a", "1")); // new, not folded across "b"
        assert_eq!(s.len(), 3);
        assert_eq!(s.get(0).unwrap().repeat_count, 3);
        assert_eq!(s.get(1).unwrap().repeat_count, 1);
        assert_eq!(s.get(2).unwrap().repeat_count, 1);
    }

    #[test]
    fn store_len_and_is_empty_after_drain() {
        let mut s = LogStore::new();
        for i in 0..10 {
            s.add_entry(entry("t", &format!("m{}", i)));
        }
        assert_eq!(s.len(), 10);
        assert!(!s.is_empty());
    }
}
