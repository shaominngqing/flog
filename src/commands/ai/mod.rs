//! AI-oriented headless inspection commands.

pub mod args;
mod output;
mod redact;
mod session;
mod snapshot;

use std::io;

use crate::cli::AiCommand;

pub async fn run(command: AiCommand) -> io::Result<()> {
    match command {
        AiCommand::Snapshot(args) => {
            let result = session::collect_snapshot_session(
                args.select.port,
                args.select.wait,
                args.settle,
                args.select.app.as_deref(),
                args.select.device.as_deref(),
            )
            .await;
            match result {
                Ok(session) => {
                    let mut payload = snapshot::build_snapshot(
                        &session.app,
                        snapshot::SnapshotBuildOptions {
                            last: args.last,
                            include_headers: args.include_headers,
                            include_body: args.include_body,
                            redact: !args.no_redact,
                            ports_scanned: session.ports_scanned,
                            wait_ms: args.select.wait.as_millis() as u64,
                            settle_ms: args.settle.as_millis() as u64,
                            complete: session.complete,
                            warnings: session.warnings,
                        },
                    );
                    payload.app = Some(output::AiApp {
                        id: session.candidate.app_id,
                        name: session.candidate.app_name,
                        package: session.candidate.package_name,
                        version: session.candidate.app_version,
                        device: session.candidate.device_name,
                        device_id: session.candidate.device_id,
                        os: session.candidate.os,
                        build_mode: session.candidate.build_mode,
                        port: session.candidate.port,
                    });
                    print_json(&output::AiEnvelope::snapshot(payload))
                }
                Err(error) => print_json(&output::AiEnvelope::error("snapshot", error)),
            }
        }
        AiCommand::Watch(_) => print_json(&not_implemented("watch")),
        AiCommand::Get(_) => print_json(&not_implemented("get")),
        AiCommand::Doctor(_) => print_json(&not_implemented("doctor")),
        AiCommand::Screenshot(_) => print_json(&not_implemented("screenshot")),
    }
}

fn not_implemented(command: &str) -> output::AiEnvelope<output::ErrorPayload> {
    let _known_error_codes = output::AiErrorCode::ALL.len();
    output::AiEnvelope::error(
        command,
        output::AiError::new(
            output::AiErrorCode::InternalError,
            format!("flog ai {command} is not implemented yet."),
            vec!["Use `flog ai snapshot --format json` for the current implementation.".to_string()],
        ),
    )
}

fn print_json<T: serde::Serialize>(value: &T) -> io::Result<()> {
    let json = serde_json::to_string_pretty(value).map_err(io::Error::other)?;
    println!("{json}");
    Ok(())
}
