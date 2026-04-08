//! Command-line argument parsing.

use clap::Parser;

/// flog — Flutter Log Viewer. See your logs, finally.
///
/// Default: auto-scan for a running Flutter VM Service.
#[derive(Parser, Debug)]
#[command(name = "flog", version, about)]
pub struct Cli {
    /// Connect to VM Service WebSocket URL directly
    #[arg(long)]
    pub uri: Option<String>,

    /// Use ADB logcat (Android)
    #[arg(long)]
    pub adb: bool,

    /// Specify Android device serial (implies --adb)
    #[arg(short = 's', long = "device")]
    pub device: Option<String>,

    /// Read from stdin pipe
    #[arg(long)]
    pub stdin: bool,

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

impl Cli {
    pub fn input_mode(&self) -> InputMode {
        if self.stdin {
            InputMode::Stdin
        } else if let Some(ref uri) = self.uri {
            InputMode::VmService(uri.clone())
        } else if self.adb || self.device.is_some() {
            InputMode::Adb(self.device.clone())
        } else {
            InputMode::Auto
        }
    }
}

#[derive(Debug)]
pub enum InputMode {
    Auto,
    Adb(Option<String>),
    VmService(String),
    Stdin,
}
