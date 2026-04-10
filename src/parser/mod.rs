//! Multi-strategy log line parser.
//!
//! Uses the Strategy pattern: a chain of parsers is tried in priority order.
//! The first parser that recognizes the line wins.

pub mod structured;
pub mod generic;
pub mod keyword;
pub mod network;

use crate::domain::{LogEntry, ParseResult};

/// A parser that can recognize a specific log format.
///
/// Implementations should return `Some(LogEntry)` if the line matches their format,
/// or `None` to pass to the next parser in the chain.
pub trait LogLineParser: Send + Sync {
    /// Human-readable name (e.g., "Structured", "Generic").
    fn name(&self) -> &'static str;

    /// Try to parse a raw line as a new log entry.
    fn try_parse(&self, line: &str) -> Option<LogEntry>;

    /// Try to parse a raw line as a continuation of the previous entry.
    fn try_continuation(&self, line: &str) -> Option<String>;
}

/// Orchestrates multiple parsers, trying each in priority order.
pub struct MultiStrategyParser {
    strategies: Vec<Box<dyn LogLineParser>>,
}

impl MultiStrategyParser {
    /// Create a parser chain with the default set of strategies.
    pub fn default_chain() -> Self {
        Self {
            strategies: vec![
                Box::new(structured::StructuredParser),
                Box::new(generic::GenericParser),
                Box::new(keyword::KeywordParser),
            ],
        }
    }

    /// Parse a raw line, trying each strategy in order.
    pub fn parse(&self, line: &str) -> ParseResult {
        // Try continuation first (belongs to previous entry).
        for strategy in &self.strategies {
            if let Some(content) = strategy.try_continuation(line) {
                return ParseResult::Continuation(content);
            }
        }

        // Try new entry.
        for strategy in &self.strategies {
            if let Some(entry) = strategy.try_parse(line) {
                return ParseResult::NewEntry(entry);
            }
        }

        ParseResult::Ignored
    }
}
