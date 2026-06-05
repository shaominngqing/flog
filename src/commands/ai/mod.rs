//! AI-oriented headless inspection commands.

pub mod args;
mod curl;
mod doctor;
mod get;
mod logs;
mod net;
mod output;
mod redact;
mod screenshot;
mod session;
mod snapshot;
mod watch;

use std::io;

use crate::cli::{AiCommand, AiNetProtocol};
use crate::domain::network::Protocol;

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
                            net_last: args.net_last,
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
                            screenshot::capture_with_flutter(&device_id_for_screenshot, None).await;
                        payload.screenshot =
                            Some(serde_json::to_value(result).unwrap_or(serde_json::Value::Null));
                    }
                    print_json(&output::AiEnvelope::snapshot(payload))
                }
                Err(error) => print_json(&output::AiEnvelope::error("snapshot", error)),
            }
        }
        AiCommand::Logs(args) => {
            let result = session::collect_snapshot_session(
                args.select.port,
                args.select.wait,
                std::time::Duration::from_millis(750),
                args.select.app.as_deref(),
                args.select.device.as_deref(),
            )
            .await;
            match result {
                Ok(session) => {
                    let logs = logs::build_logs_list(
                        &session.app,
                        logs::LogsListOptions {
                            last: args.last,
                            level: args.level,
                            tag: args.tag,
                            search: args.search,
                        },
                    );
                    print_json(&output::AiEnvelope::new("logs", true, LogsPayload { logs }))
                }
                Err(error) => print_json(&output::AiEnvelope::error("logs", error)),
            }
        }
        AiCommand::Net(args) => {
            let result = session::collect_snapshot_session(
                args.select.port,
                args.select.wait,
                std::time::Duration::from_millis(750),
                args.select.app.as_deref(),
                args.select.device.as_deref(),
            )
            .await;
            match result {
                Ok(session) => {
                    let network = net::build_net_list(
                        &session.app,
                        net::NetListOptions {
                            last: args.last,
                            failed: args.failed,
                            status: args.status,
                            method: args.method,
                            url: args.url,
                            protocol: args.protocol.map(ai_protocol),
                            slow: args.slow,
                        },
                    );
                    print_json(&output::AiEnvelope::new(
                        "net",
                        true,
                        NetPayload { network },
                    ))
                }
                Err(error) => print_json(&output::AiEnvelope::error("net", error)),
            }
        }
        AiCommand::Watch(args) => watch::run_watch(args).await,
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
                Ok(session) => match get::parse_record_id(&args.id).and_then(|record_id| {
                    let mode = if args.detail {
                        get::RecordDetailMode::Detail
                    } else {
                        get::RecordDetailMode::Summary
                    };
                    get::lookup_record(&session.app, &record_id, mode, !args.no_redact)
                }) {
                    Ok(record) => {
                        print_json(&output::AiEnvelope::new("get", true, GetPayload { record }))
                    }
                    Err(error) => print_json(&output::AiEnvelope::error("get", error)),
                },
                Err(error) => print_json(&output::AiEnvelope::error("get", error)),
            }
        }
        AiCommand::Curl(args) => {
            let result = session::collect_snapshot_session(
                args.select.port,
                args.select.wait,
                std::time::Duration::from_millis(750),
                args.select.app.as_deref(),
                args.select.device.as_deref(),
            )
            .await;
            match result {
                Ok(session) => match get::parse_record_id(&args.id).and_then(|record_id| {
                    curl::build_curl(&session.app, &record_id, !args.no_redact)
                }) {
                    Ok(request) => print_json(&output::AiEnvelope::new(
                        "curl",
                        true,
                        CurlPayload { request },
                    )),
                    Err(error) => print_json(&output::AiEnvelope::error("curl", error)),
                },
                Err(error) => print_json(&output::AiEnvelope::error("curl", error)),
            }
        }
        AiCommand::Doctor(_) => {
            let payload = doctor::run_doctor(9753).await;
            print_json(&output::AiEnvelope::new("doctor", true, payload))
        }
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
            let result =
                screenshot::capture_with_flutter(device_id, args.out.map(Into::into)).await;
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

#[derive(serde::Serialize)]
struct LogsPayload {
    logs: Vec<serde_json::Value>,
}

#[derive(serde::Serialize)]
struct NetPayload {
    network: Vec<serde_json::Value>,
}

#[derive(serde::Serialize)]
struct CurlPayload {
    request: serde_json::Value,
}

fn ai_protocol(protocol: AiNetProtocol) -> Protocol {
    match protocol {
        AiNetProtocol::Http => Protocol::Http,
        AiNetProtocol::Sse => Protocol::Sse,
        AiNetProtocol::Ws => Protocol::Ws,
    }
}

fn print_json<T: serde::Serialize>(value: &T) -> io::Result<()> {
    let json = serde_json::to_string_pretty(value).map_err(io::Error::other)?;
    println!("{json}");
    Ok(())
}
