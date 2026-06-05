//! Command-line argument parsing.

use std::path::PathBuf;
use std::time::Duration;

use clap::{Args, Parser, Subcommand, ValueEnum};

/// flog — Flutter Log Viewer & Network Inspector.
///
/// Starts a WebSocket server and waits for flog_dart clients to connect.
#[derive(Subcommand, Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Update,
    Uninstall,
    Doctor,
    Devices,
    InstallSkill(InstallSkillArgs),
    #[command(subcommand)]
    Ai(AiCommand),
}

#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillAgent {
    All,
    Codex,
    Claude,
    Cursor,
}

#[derive(Args, Debug, Clone, PartialEq, Eq)]
pub struct InstallSkillArgs {
    #[arg(long, value_enum, default_value_t = SkillAgent::All)]
    pub agent: SkillAgent,
    #[arg(long, default_value = ".")]
    pub project: PathBuf,
}

#[derive(Subcommand, Debug, Clone, PartialEq, Eq)]
pub enum AiCommand {
    Snapshot(AiSnapshotArgs),
    Logs(AiLogsArgs),
    Net(AiNetArgs),
    Watch(AiWatchArgs),
    Get(AiGetArgs),
    Curl(AiCurlArgs),
    Doctor(AiDoctorArgs),
    Screenshot(AiScreenshotArgs),
}

#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiFormat {
    Json,
    Ndjson,
}

#[derive(Args, Debug, Clone, PartialEq, Eq)]
pub struct AiSelectArgs {
    #[arg(long)]
    pub device: Option<String>,
    #[arg(long)]
    pub app: Option<String>,
    #[arg(long, default_value = "9753")]
    pub port: u16,
    #[arg(long, value_parser = parse_duration, default_value = "5s")]
    pub wait: Duration,
}

#[derive(Args, Debug, Clone, PartialEq, Eq)]
pub struct AiSnapshotArgs {
    #[command(flatten)]
    pub select: AiSelectArgs,
    #[arg(long, default_value = "20")]
    pub last: usize,
    #[arg(long, default_value = "20")]
    pub net_last: usize,
    #[arg(long, value_enum, default_value_t = AiFormat::Json)]
    pub format: AiFormat,
    #[arg(long, value_parser = parse_duration, default_value = "750ms")]
    pub settle: Duration,
    #[arg(long)]
    pub errors: bool,
    #[arg(long)]
    pub network: bool,
    #[arg(long)]
    pub sse: bool,
    #[arg(long)]
    pub ws: bool,
    #[arg(long)]
    pub include_headers: bool,
    #[arg(long)]
    pub include_body: bool,
    #[arg(long)]
    pub no_redact: bool,
    #[arg(long)]
    pub screenshot: bool,
}

#[derive(Args, Debug, Clone, PartialEq, Eq)]
pub struct AiLogsArgs {
    #[command(flatten)]
    pub select: AiSelectArgs,
    #[arg(long, default_value = "50")]
    pub last: usize,
    #[arg(long, value_parser = parse_level)]
    pub level: Option<crate::domain::LogLevel>,
    #[arg(long)]
    pub tag: Option<String>,
    #[arg(long)]
    pub search: Option<String>,
}

#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiNetProtocol {
    Http,
    Sse,
    Ws,
}

#[derive(Args, Debug, Clone, PartialEq, Eq)]
pub struct AiNetArgs {
    #[command(flatten)]
    pub select: AiSelectArgs,
    #[arg(long, default_value = "50")]
    pub last: usize,
    #[arg(long)]
    pub failed: bool,
    #[arg(long)]
    pub status: Option<String>,
    #[arg(long)]
    pub method: Option<String>,
    #[arg(long)]
    pub url: Option<String>,
    #[arg(long, value_enum)]
    pub protocol: Option<AiNetProtocol>,
    #[arg(long, value_parser = parse_duration)]
    pub slow: Option<Duration>,
}

#[derive(Args, Debug, Clone, PartialEq, Eq)]
pub struct AiWatchArgs {
    #[command(flatten)]
    pub select: AiSelectArgs,
    #[arg(long, value_parser = parse_duration, default_value = "30s")]
    pub duration: Duration,
    #[arg(long, value_enum, default_value_t = AiFormat::Ndjson)]
    pub format: AiFormat,
    #[arg(long)]
    pub errors: bool,
    #[arg(long)]
    pub network: bool,
    #[arg(long)]
    pub since: Option<String>,
}

#[derive(Args, Debug, Clone, PartialEq, Eq)]
pub struct AiGetArgs {
    pub id: String,
    #[command(flatten)]
    pub select: AiSelectArgs,
    #[arg(long)]
    pub detail: bool,
    #[arg(long)]
    pub no_redact: bool,
}

#[derive(Args, Debug, Clone, PartialEq, Eq)]
pub struct AiCurlArgs {
    pub id: String,
    #[command(flatten)]
    pub select: AiSelectArgs,
    #[arg(long)]
    pub no_redact: bool,
}

#[derive(Args, Debug, Clone, PartialEq, Eq)]
pub struct AiDoctorArgs {
    #[arg(long, value_enum, default_value_t = AiFormat::Json)]
    pub format: AiFormat,
}

#[derive(Args, Debug, Clone, PartialEq, Eq)]
pub struct AiScreenshotArgs {
    #[command(flatten)]
    pub select: AiSelectArgs,
    #[arg(long, value_enum, default_value_t = AiFormat::Json)]
    pub format: AiFormat,
    #[arg(long)]
    pub out: Option<String>,
}

#[derive(Parser, Debug)]
#[command(name = "flog", version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

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

pub(crate) fn parse_duration(input: &str) -> Result<Duration, String> {
    let Some((number, unit)) = split_duration(input) else {
        return Err("duration must use ms, s, or m suffix".to_string());
    };
    let value = number
        .parse::<u64>()
        .map_err(|_| format!("invalid duration value '{number}'"))?;
    match unit {
        "ms" => Ok(Duration::from_millis(value)),
        "s" => Ok(Duration::from_secs(value)),
        "m" => Ok(Duration::from_secs(value * 60)),
        _ => Err(format!("invalid duration unit '{unit}', use ms/s/m")),
    }
}

fn split_duration(input: &str) -> Option<(&str, &str)> {
    if input.is_empty() {
        return None;
    }
    let unit_start = input
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(input.len());
    if unit_start == 0 || unit_start == input.len() {
        return None;
    }
    Some(input.split_at(unit_start))
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

    #[test]
    fn cli_update_subcommand() {
        let cli = Cli::parse_from(["flog", "update"]);
        assert_eq!(cli.command, Some(Command::Update));
    }

    #[test]
    fn cli_uninstall_subcommand() {
        let cli = Cli::parse_from(["flog", "uninstall"]);
        assert_eq!(cli.command, Some(Command::Uninstall));
    }

    #[test]
    fn cli_doctor_subcommand() {
        let cli = Cli::parse_from(["flog", "doctor"]);
        assert_eq!(cli.command, Some(Command::Doctor));
    }

    #[test]
    fn cli_devices_subcommand() {
        let cli = Cli::parse_from(["flog", "devices"]);
        assert_eq!(cli.command, Some(Command::Devices));
    }

    #[test]
    fn cli_install_skill_defaults_parse() {
        let cli = Cli::parse_from(["flog", "install-skill"]);
        let Some(Command::InstallSkill(args)) = cli.command else {
            panic!("expected install-skill command");
        };
        assert_eq!(args.agent, SkillAgent::All);
        assert_eq!(args.project, PathBuf::from("."));
    }

    #[test]
    fn cli_install_skill_agent_and_project_parse() {
        let cli = Cli::parse_from([
            "flog",
            "install-skill",
            "--agent",
            "cursor",
            "--project",
            "/tmp/app",
        ]);
        let Some(Command::InstallSkill(args)) = cli.command else {
            panic!("expected install-skill command");
        };
        assert_eq!(args.agent, SkillAgent::Cursor);
        assert_eq!(args.project, PathBuf::from("/tmp/app"));
    }

    #[test]
    fn cli_ai_snapshot_defaults_parse() {
        let cli = Cli::parse_from(["flog", "ai", "snapshot"]);
        let Some(Command::Ai(AiCommand::Snapshot(args))) = cli.command else {
            panic!("expected ai snapshot command");
        };
        assert_eq!(args.last, 20);
        assert_eq!(args.net_last, 20);
    }

    #[test]
    fn cli_ai_snapshot_screenshot_parse() {
        let cli = Cli::parse_from(["flog", "ai", "snapshot", "--screenshot"]);
        let Some(Command::Ai(AiCommand::Snapshot(args))) = cli.command else {
            panic!("expected ai snapshot command");
        };
        assert!(args.screenshot);
    }

    #[test]
    fn cli_ai_get_parse() {
        let cli = Cli::parse_from(["flog", "ai", "get", "net#42", "--detail"]);
        let Some(Command::Ai(AiCommand::Get(args))) = cli.command else {
            panic!("expected ai get command");
        };
        assert_eq!(args.id, "net#42");
        assert!(args.detail);
    }

    #[test]
    fn cli_ai_logs_filters_parse() {
        let cli = Cli::parse_from([
            "flog", "ai", "logs", "--level", "error", "--tag", "Network", "--search", "timeout",
            "--last", "25",
        ]);
        let Some(Command::Ai(AiCommand::Logs(args))) = cli.command else {
            panic!("expected ai logs command");
        };
        assert_eq!(args.level, Some(LogLevel::Error));
        assert_eq!(args.tag.as_deref(), Some("Network"));
        assert_eq!(args.search.as_deref(), Some("timeout"));
        assert_eq!(args.last, 25);
    }

    #[test]
    fn cli_ai_net_filters_parse() {
        let cli = Cli::parse_from([
            "flog",
            "ai",
            "net",
            "--failed",
            "--status",
            "5xx",
            "--method",
            "POST",
            "--url",
            "dictionary",
            "--protocol",
            "sse",
            "--slow",
            "1000ms",
            "--last",
            "15",
        ]);
        let Some(Command::Ai(AiCommand::Net(args))) = cli.command else {
            panic!("expected ai net command");
        };
        assert!(args.failed);
        assert_eq!(args.status.as_deref(), Some("5xx"));
        assert_eq!(args.method.as_deref(), Some("POST"));
        assert_eq!(args.url.as_deref(), Some("dictionary"));
        assert_eq!(args.protocol, Some(AiNetProtocol::Sse));
        assert_eq!(args.slow.unwrap().as_millis(), 1000);
        assert_eq!(args.last, 15);
    }

    #[test]
    fn cli_ai_curl_parse() {
        let cli = Cli::parse_from(["flog", "ai", "curl", "net#42"]);
        let Some(Command::Ai(AiCommand::Curl(args))) = cli.command else {
            panic!("expected ai curl command");
        };
        assert_eq!(args.id, "net#42");
    }

    #[test]
    fn cli_ai_watch_duration_parse() {
        let cli = Cli::parse_from(["flog", "ai", "watch", "--duration", "30s"]);
        let Some(Command::Ai(AiCommand::Watch(args))) = cli.command else {
            panic!("expected ai watch command");
        };
        assert_eq!(args.duration.as_millis(), 30_000);
    }
}
