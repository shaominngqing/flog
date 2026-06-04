//! AI-oriented headless inspection commands.

pub mod args;
mod get;
mod output;
mod redact;
mod session;
mod screenshot;
mod snapshot;
mod watch;

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
                    let device_id_for_screenshot = session.candidate.device_id.clone();
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
                    if args.screenshot {
                        let result =
                            screenshot::capture_with_flutter(&device_id_for_screenshot, None)
                                .await;
                        payload.screenshot =
                            Some(serde_json::to_value(result).unwrap_or(serde_json::Value::Null));
                    }
                    print_json(&output::AiEnvelope::snapshot(payload))
                }
                Err(error) => print_json(&output::AiEnvelope::error("snapshot", error)),
            }
        }
        AiCommand::Watch(args) => watch::run_watch(args.duration).await,
        AiCommand::Get(args) => {
            let result = session::collect_snapshot_session(
                args.select.port,
                args.select.wait,
                std::time::Duration::from_millis(750),
                args.select.app.as_deref(),
                args.select.device.as_deref(),
            )
            .await;
            match result {
                Ok(session) => match get::parse_record_id(&args.id)
                    .and_then(|record_id| get::lookup_record(&session.app, &record_id))
                {
                    Ok(record) => print_json(&output::AiEnvelope::new(
                        "get",
                        true,
                        GetPayload { record },
                    )),
                    Err(error) => print_json(&output::AiEnvelope::error("get", error)),
                },
                Err(error) => print_json(&output::AiEnvelope::error("get", error)),
            }
        }
        AiCommand::Doctor(_) => print_json(&not_implemented("doctor")),
        AiCommand::Screenshot(args) => {
            let Some(device_id) = args.select.device.as_deref() else {
                return print_json(&output::AiEnvelope::error(
                    "screenshot",
                    output::AiError::new(
                        output::AiErrorCode::NoDeviceFound,
                        "No device was selected for screenshot capture.",
                        vec![
                            "Run `flutter devices` to find a device id.".to_string(),
                            "Run `flog ai screenshot --device <device-id>`.".to_string(),
                        ],
                    ),
                ));
            };
            let result = screenshot::capture_with_flutter(device_id, args.out.map(Into::into)).await;
            print_json(&output::AiEnvelope::new(
                "screenshot",
                result.ok,
                screenshot::ScreenshotPayload { screenshot: result },
            ))
        }
    }
}

#[derive(serde::Serialize)]
struct GetPayload {
    record: serde_json::Value,
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
