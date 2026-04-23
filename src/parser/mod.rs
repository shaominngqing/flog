//! Multi-strategy log line parser.
//!
//! Uses the Strategy pattern: a chain of parsers is tried in priority order.
//! The first parser that recognizes the line wins.

pub mod generic;
pub mod keyword;
pub mod network;
pub mod structured;

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
        for strategy in &self.strategies {
            if let Some(entry) = strategy.try_parse(line) {
                return ParseResult::NewEntry(entry);
            }
        }

        ParseResult::Ignored
    }
}

#[cfg(test)]
mod tests {
    //! Characterization tests for the MultiStrategyParser chain.
    //!
    //! Covers DOM-013 (hard-wired chain order, strategy fall-through). Phase 3
    //! may add `with_strategies(...)`; until then the chain is fixed to
    //! `[structured, generic, keyword]` and these tests lock that order so a
    //! reshuffle can't silently change which strategy wins on a given input.
    use super::*;
    use crate::domain::LogLevel;
    use crate::parser::{
        generic::GenericParser, keyword::KeywordParser, structured::StructuredParser,
    };

    fn parse_line(line: &str) -> ParseResult {
        MultiStrategyParser::default_chain().parse(line)
    }

    fn unwrap_new(r: ParseResult) -> crate::domain::LogEntry {
        match r {
            ParseResult::NewEntry(e) => e,
            ParseResult::Ignored => panic!("expected NewEntry, got Ignored"),
        }
    }

    // ---- DOM-013: chain order + fall-through ----

    #[test]
    fn dom_013_default_chain_has_three_strategies() {
        // Lock current chain length so Phase 3 redesign can't silently drop one.
        let p = MultiStrategyParser::default_chain();
        assert_eq!(p.strategies.len(), 3);
        assert_eq!(p.strategies[0].name(), "Structured");
        assert_eq!(p.strategies[1].name(), "Generic");
        assert_eq!(p.strategies[2].name(), "Keyword");
    }

    #[test]
    fn dom_013_structured_wins_on_bracket_format() {
        // `[INFO][Network] msg` is handled by StructuredParser — never reaches
        // the generic or keyword strategies.
        let entry = unwrap_new(parse_line("[INFO][Network] hello"));
        assert_eq!(entry.level, LogLevel::Info);
        assert_eq!(entry.tag, "Network");
        assert_eq!(entry.message, "hello");
    }

    #[test]
    fn dom_013_generic_wins_when_structured_rejects() {
        // `I/flutter (123): plain text` — StructuredParser bails (no
        // `[LEVEL][Tag]` bracket), GenericParser accepts via FLUTTER_PLAIN_RE.
        let line = "I/flutter (123): plain output";
        // Structured alone returns None
        assert!(StructuredParser.try_parse(line).is_none());
        // Generic picks it up
        assert!(GenericParser.try_parse(line).is_some());
        let entry = unwrap_new(parse_line(line));
        assert_eq!(entry.tag, "flutter");
        assert_eq!(entry.level, LogLevel::System);
    }

    #[test]
    fn dom_013_keyword_wins_when_first_two_reject() {
        // A bare line with no Flutter prefix, no bracket format → only
        // KeywordParser (the fallback) accepts it.
        let line = "Something failed with an error";
        assert!(StructuredParser.try_parse(line).is_none());
        assert!(GenericParser.try_parse(line).is_none());
        assert!(KeywordParser.try_parse(line).is_some());
        let entry = unwrap_new(parse_line(line));
        assert_eq!(entry.level, LogLevel::Error);
        assert_eq!(entry.tag, "App");
    }

    #[test]
    fn dom_013_fully_empty_line_produces_ignored() {
        // Empty/whitespace-only input: structured/generic reject, keyword also
        // returns None for empty trimmed → overall Ignored (not SYSTEM entry —
        // DOM-013 plan's "unparseable → SYSTEM" claim is inaccurate; we lock
        // the actual current behavior per Rule 6).
        assert!(matches!(parse_line(""), ParseResult::Ignored));
        assert!(matches!(parse_line("   "), ParseResult::Ignored));
        assert!(matches!(parse_line("\t\t"), ParseResult::Ignored));
    }

    #[test]
    fn dom_013_fall_through_structured_then_generic_then_keyword() {
        // Order check: a line recognizable by multiple strategies should fall
        // to the FIRST that matches. `[INFO] [Network] msg` (with space) is
        // rejected by StructuredParser (its BRACKET_RE has no space) but
        // accepted by GenericParser BRACKET_LEVEL_RE — so generic wins.
        let line = "I/flutter (1234): [INFO] [Network] payload";
        assert!(StructuredParser.try_parse(line).is_none());
        let entry = unwrap_new(parse_line(line));
        assert_eq!(entry.level, LogLevel::Info);
        assert_eq!(entry.tag, "Network");
    }

    #[test]
    fn dom_013_unparseable_junk_routes_to_keyword() {
        // A blob that no structured/generic pattern matches: keyword takes it
        // and tags it `App` with inferred level. Documents that the fallback
        // is lossy but never drops nonempty lines.
        let line = "random text without any known markers";
        let entry = unwrap_new(parse_line(line));
        assert_eq!(entry.tag, "App");
        assert_eq!(entry.level, LogLevel::Info); // no error/warning/debug keyword
    }

    #[test]
    fn dom_013_structured_intercepts_before_generic_on_pipe_format() {
        // Pipe format `HH:MM:SS.mmm │ LEVEL │ Tag │ msg` is ONLY handled by
        // StructuredParser. Ensure generic doesn't accidentally accept it.
        let line = "18:05:26.675 │ INFO    │ Network        │ msg";
        let gen_result = GenericParser.try_parse(line);
        assert!(
            gen_result.is_none(),
            "pipe format should only be claimed by structured"
        );
        let entry = unwrap_new(parse_line(line));
        assert_eq!(entry.timestamp, "18:05:26.675");
    }

    #[test]
    fn dom_013_parse_result_ignored_is_observable() {
        // ParseResult::Ignored is the variant for "no strategy matched."
        // Pattern-match for type stability.
        match parse_line("") {
            ParseResult::Ignored => {}
            ParseResult::NewEntry(_) => panic!("empty line must not produce NewEntry"),
        }
    }

    #[test]
    fn dom_013_parse_returns_new_entry_for_known_format() {
        // Shape of the Ok path: NewEntry wraps a LogEntry.
        match parse_line("[DEBUG][Tag] msg") {
            ParseResult::NewEntry(e) => assert_eq!(e.level, LogLevel::Debug),
            ParseResult::Ignored => panic!("expected NewEntry"),
        }
    }

    #[test]
    fn dom_013_multibyte_utf8_message_passes_through() {
        // Boundary: multi-byte UTF-8 in tag and message must not panic and
        // must reach the structured parser intact.
        let line = "[INFO][网络] 你好，世界";
        let entry = unwrap_new(parse_line(line));
        assert_eq!(entry.tag, "网络");
        assert_eq!(entry.message, "你好，世界");
    }

    #[test]
    fn dom_013_very_long_line_does_not_panic() {
        // Boundary: 10KB line should parse (or be keyword-inferred) without
        // blow-up.
        let big_msg = "x".repeat(10_000);
        let line = format!("[INFO][Big] {big_msg}");
        let entry = unwrap_new(parse_line(&line));
        assert_eq!(entry.level, LogLevel::Info);
        assert_eq!(entry.message.len(), 10_000);
    }
}
