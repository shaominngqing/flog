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

    /// Append a continuation line to the most recent entry.
    #[allow(dead_code)]
    pub fn append_continuation(&mut self, content: String) {
        if let Some(last) = self.entries.back_mut() {
            last.extra_lines.push(content);
        }
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

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}
