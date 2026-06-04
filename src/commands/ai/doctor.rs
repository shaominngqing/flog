use serde::Serialize;
use tokio::process::Command;

use super::output::AiErrorCode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Ok,
    Missing,
    Failed,
}

#[derive(Debug, Serialize)]
pub struct DoctorPayload {
    pub checks: Vec<DoctorCheck>,
    pub ports: Vec<u16>,
    pub known_error_codes: Vec<AiErrorCode>,
}

#[derive(Debug, Serialize)]
pub struct DoctorCheck {
    pub name: String,
    pub status: CheckStatus,
    pub message: String,
}

pub fn port_range(base: u16) -> Vec<u16> {
    (base..base + 10).collect()
}

pub fn command_status_from_error(kind: std::io::ErrorKind) -> CheckStatus {
    if kind == std::io::ErrorKind::NotFound {
        CheckStatus::Missing
    } else {
        CheckStatus::Failed
    }
}

pub async fn run_doctor(base_port: u16) -> DoctorPayload {
    let checks = vec![
        command_check("flutter", &["--version"]).await,
        command_check("adb", &["version"]).await,
    ];
    DoctorPayload {
        checks,
        ports: port_range(base_port),
        known_error_codes: AiErrorCode::ALL.to_vec(),
    }
}

async fn command_check(program: &str, args: &[&str]) -> DoctorCheck {
    match Command::new(program)
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
    {
        Ok(status) if status.success() => DoctorCheck {
            name: program.to_string(),
            status: CheckStatus::Ok,
            message: format!("{program} is available"),
        },
        Ok(status) => DoctorCheck {
            name: program.to_string(),
            status: CheckStatus::Failed,
            message: format!("{program} exited with status {status}"),
        },
        Err(e) => DoctorCheck {
            name: program.to_string(),
            status: command_status_from_error(e.kind()),
            message: e.to_string(),
        },
    }
}

#[cfg(test)]
#[path = "doctor_tests.rs"]
mod tests;
