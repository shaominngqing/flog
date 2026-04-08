//! Ring-buffer log storage. Parser-agnostic — receives pre-parsed `LogEntry` values.

use super::entry::LogEntry;

const MAX_ENTRIES: usize = 100_000;
const DRAIN_FRACTION: usize = 10; // drain 1/10 when full

/// In-memory log storage with capacity management and repeat folding.
pub struct LogStore {
    entries: Vec<LogEntry>,
}

impl LogStore {
    pub fn new() -> Self {
        Self {
            entries: Vec::with_capacity(1024),
        }
    }

    /// Add a pre-parsed log entry. Returns the number of drained (evicted) entries.
    pub fn add_entry(&mut self, entry: LogEntry) -> usize {
        // Smart folding: consecutive identical entries collapse into one.
        if let Some(last) = self.entries.last_mut() {
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
            let n = MAX_ENTRIES / DRAIN_FRACTION;
            self.entries.drain(..n);
            n
        } else {
            0
        };

        self.entries.push(entry);
        drained
    }

    /// Append a continuation line to the most recent entry.
    pub fn append_continuation(&mut self, content: String) {
        if let Some(last) = self.entries.last_mut() {
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
}
