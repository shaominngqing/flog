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
