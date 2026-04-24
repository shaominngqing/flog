//! Command-line argument parsing.

use clap::Parser;

/// flog — Flutter Log Viewer & Network Inspector.
///
/// Starts a WebSocket server and waits for flog_dart clients to connect.
#[derive(Parser, Debug)]
#[command(name = "flog", version, about)]
pub struct Cli {
    /// Server port for flog_dart connections
    #[arg(long, default_value = "9753")]
    pub port: u16,

    /// Initial minimum log level (v/d/i/w/e)
    #[arg(long, value_parser = parse_level)]
    pub level: Option<crate::domain::LogLevel>,

    /// Initial tag filter
    #[arg(long)]
    pub tag: Option<String>,
}

fn parse_level(s: &str) -> Result<crate::domain::LogLevel, String> {
    match s.to_lowercase().as_str() {
        "v" | "verbose" => Ok(crate::domain::LogLevel::Verbose),
        "d" | "debug" => Ok(crate::domain::LogLevel::Debug),
        "i" | "info" => Ok(crate::domain::LogLevel::Info),
        "w" | "warn" | "warning" => Ok(crate::domain::LogLevel::Warning),
        "e" | "error" => Ok(crate::domain::LogLevel::Error),
        _ => Err(format!("unknown level '{}', use v/d/i/w/e", s)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::LogLevel;
    use clap::Parser;

    // ── parse_level: every accepted alias ─────────────────────────────

    #[test]
    fn parse_level_verbose_aliases() {
        assert_eq!(parse_level("v").unwrap(), LogLevel::Verbose);
        assert_eq!(parse_level("verbose").unwrap(), LogLevel::Verbose);
        assert_eq!(parse_level("VERBOSE").unwrap(), LogLevel::Verbose);
    }

    #[test]
    fn parse_level_debug_aliases() {
        assert_eq!(parse_level("d").unwrap(), LogLevel::Debug);
        assert_eq!(parse_level("debug").unwrap(), LogLevel::Debug);
        assert_eq!(parse_level("DEBUG").unwrap(), LogLevel::Debug);
    }

    #[test]
    fn parse_level_info_aliases() {
        assert_eq!(parse_level("i").unwrap(), LogLevel::Info);
        assert_eq!(parse_level("info").unwrap(), LogLevel::Info);
    }

    #[test]
    fn parse_level_warning_aliases() {
        assert_eq!(parse_level("w").unwrap(), LogLevel::Warning);
        assert_eq!(parse_level("warn").unwrap(), LogLevel::Warning);
        assert_eq!(parse_level("warning").unwrap(), LogLevel::Warning);
    }

    #[test]
    fn parse_level_error_aliases() {
        assert_eq!(parse_level("e").unwrap(), LogLevel::Error);
        assert_eq!(parse_level("error").unwrap(), LogLevel::Error);
    }

    #[test]
    fn parse_level_rejects_unknown() {
        let err = parse_level("zz").unwrap_err();
        assert!(err.contains("unknown level 'zz'"));
    }

    // ── Cli::parse_from: fixture CLI args ─────────────────────────────

    #[test]
    fn cli_defaults() {
        let cli = Cli::parse_from(["flog"]);
        assert_eq!(cli.port, 9753);
        assert!(cli.level.is_none());
        assert!(cli.tag.is_none());
    }

    #[test]
    fn cli_custom_port() {
        let cli = Cli::parse_from(["flog", "--port", "12345"]);
        assert_eq!(cli.port, 12345);
    }

    #[test]
    fn cli_level_warning() {
        let cli = Cli::parse_from(["flog", "--level", "w"]);
        assert_eq!(cli.level, Some(LogLevel::Warning));
    }

    #[test]
    fn cli_level_long_form() {
        let cli = Cli::parse_from(["flog", "--level", "error"]);
        assert_eq!(cli.level, Some(LogLevel::Error));
    }

    #[test]
    fn cli_tag_passed_through() {
        let cli = Cli::parse_from(["flog", "--tag", "network,-noise"]);
        assert_eq!(cli.tag.as_deref(), Some("network,-noise"));
    }

    #[test]
    fn cli_all_flags_combined() {
        let cli = Cli::parse_from(["flog", "--port", "9000", "--level", "info", "--tag", "net"]);
        assert_eq!(cli.port, 9000);
        assert_eq!(cli.level, Some(LogLevel::Info));
        assert_eq!(cli.tag.as_deref(), Some("net"));
    }

    #[test]
    fn cli_invalid_level_fails() {
        let err = Cli::try_parse_from(["flog", "--level", "nope"]).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("unknown level") || msg.contains("'nope'"));
    }
}
