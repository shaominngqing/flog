# AI CLI and Skill Design

## Goal

Add an AI-first read-only diagnostic surface for flog so an agent can inspect a
running Flutter app without asking the user to copy text from the TUI.

The first version should provide:

```bash
flog ai snapshot
flog ai watch
flog ai get <id>
flog ai doctor
flog ai screenshot
```

and a Codex skill named `flog-inspect` that calls these commands and turns
their structured output into an analysis.

This is not a replacement for the TUI. The TUI remains the human interface.
The AI CLI is a stable, compact, machine-readable diagnostic interface.

## User Experience

A typical user prompt should be enough:

```text
Use flog to figure out why the page is stuck loading.
```

The skill should run a bounded snapshot, optionally include a screenshot when
the prompt is visual, and then answer with evidence:

```text
The request did not fail at the transport layer. `net#42` completed with 200
and 14 SSE chunks, but the merged content field is empty. `log#188` shows the
parser expected `choices.0.delta.content`. This points to a response-shape
mismatch rather than a network outage.
```

The user should not need to know which device is connected, which port
`flog_dart` selected, or which fields are worth copying.

## Scope

### In Scope

- Headless CLI commands under `flog ai`.
- JSON output with a stable schema version.
- Bounded collection windows, settle waits, and explicit timeouts.
- Stable record ids such as `log#184`, `net#42`, and `chunk#42.13`.
- Summary and notable diagnostics designed for AI context windows.
- Redaction and truncation by default.
- Optional screenshots through Flutter's screenshot command first, with
  platform fallbacks where practical.
- A Codex skill that invokes the CLI instead of reimplementing the protocol.
- Focused Rust tests for parsing, output schema, redaction, truncation,
  diagnostics, and fake-server collection.

### Out of Scope for v1

- MCP server.
- TUI-hosted local HTTP API.
- Write actions such as mock-rule sync or replay from the AI CLI.
- Long-term persistent storage.
- App-internal screenshot protocol in `flog_dart`.
- Precise macOS desktop window capture.
- Browser UI or GUI.

These are intentionally deferred so v1 solves the main pain: reliable AI
access to recent app state.

## Product Principles

1. Failures must be actionable. If no app is found, the CLI returns structured
   JSON explaining what was scanned and what to try next.
2. Defaults protect the user's context window. The default snapshot is a
   diagnosis-oriented summary, not a dump of every byte.
3. Defaults protect privacy. Headers and bodies are summarized and redacted
   unless explicitly requested.
4. Details are pull-based. The first snapshot gives ids; `flog ai get <id>`
   retrieves one detailed object when needed.
5. Screenshot capture is opt-in. Visual state is useful, but it can include
   sensitive information outside the app.
6. The skill is thin. Rust owns discovery, transport, protocol parsing, and
   snapshot generation.

## Command Design

### `flog ai snapshot`

Collects a short headless session and prints JSON.

Default behavior:

- Scan devices using the existing transport discovery path.
- Scan ports `base..base+9`, defaulting to `9753..9762`.
- Collect responding `flog_dart` apps. Select the app only when exactly one
  app responds or `--app` / `--device` narrows selection to one app.
- Wait for Hello, send Subscribe to request replay, collect replayed frames,
  and wait for a short settle window before producing output.
- Include summary, notable diagnostics, recent logs, notable logs, and notable
  network entries.
- Exclude full request and response bodies by default.

Important flags:

```bash
flog ai snapshot --format json
flog ai snapshot --last 300
flog ai snapshot --wait 10s --settle 750ms
flog ai snapshot --errors --network --sse --ws
flog ai snapshot --include-headers --include-body
flog ai snapshot --no-redact
flog ai snapshot --screenshot
flog ai snapshot --device <device-id> --app <app-id-or-name>
flog ai snapshot --port 9753
```

`--format json` is the only required stable format in v1. Human-readable text
can exist later, but the skill should use JSON only.

### `flog ai watch`

Streams newline-delimited JSON events for a bounded period.

Examples:

```bash
flog ai watch --duration 30s --errors --network
flog ai watch --since now --format ndjson
```

This is for tasks where the AI says "click the button again and I will watch".
The command must have a default maximum duration so it does not run forever in
agent contexts.

### `flog ai get <id>`

Retrieves a single detailed object from a fresh headless collection window.

Examples:

```bash
flog ai get net#42 --chunks --body
flog ai get log#188 --stacktrace
flog ai get chunk#42.13 --body
```

Because v1 does not maintain persistent storage, `get` depends on the Dart-side
`FlogStore` replay buffer still containing the object. If the object cannot be
found, the command returns `record_not_found` with a suggestion to rerun
`snapshot` or use `watch`.

### `flog ai doctor`

Produces machine-readable diagnostics for the AI skill.

Checks:

- `flog` version and executable path.
- Flutter SDK availability.
- `flutter devices` availability.
- adb availability and authorization hints.
- macOS usbmuxd socket availability.
- port status for `9753..9762`.
- whether any `flog_dart` app responds.
- screenshot capability for visible devices when cheap to determine.

### `flog ai screenshot`

Captures the current visible device screen and returns JSON pointing to a local
PNG path.

Examples:

```bash
flog ai screenshot --format json
flog ai screenshot --device <device-id> --out /tmp/flog-ai/screen.png
```

`snapshot --screenshot` internally uses the same capability.

## Screenshot Strategy

The first version should prefer Flutter's own command:

```bash
flutter screenshot -d <device-id> -o <path>
```

This keeps the user model simple and uses tooling Flutter developers already
have. Flutter's default screenshot mode delegates to the target device's native
screenshot capability, so it captures the currently displayed screen, including
content outside Flutter such as status bars or system permission sheets.

Fallbacks:

- Android: `adb -s <serial> exec-out screencap -p`.
- iOS simulator: `xcrun simctl io <udid> screenshot <path>`.
- iOS real device: report `screenshot_unsupported` unless a reliable optional
  dependency is explicitly configured.
- macOS desktop Flutter app: report `screenshot_unsupported` in v1 unless a
  precise app-window capture method is added later.

Screenshot failure must not fail the whole snapshot. The top-level result can
be `ok: true` while `screenshot.ok` is false.

Example:

```json
{
  "screenshot": {
    "ok": false,
    "error": {
      "code": "flutter_screenshot_failed",
      "message": "Flutter screenshot failed for device emulator-5554.",
      "next_actions": [
        "Run `flutter devices`",
        "Unlock the device screen",
        "Try `flog ai doctor --format json`"
      ]
    }
  }
}
```

Screenshots should be written to a temp directory such as
`$TMPDIR/flog-ai/screenshots/` unless `--out` is provided. The JSON should
include the path, source, capture time, and a warning that the image may include
sensitive visible content.

## JSON Output Model

Every command should use the same envelope.

```json
{
  "ok": true,
  "meta": {
    "flog_version": "0.x",
    "schema_version": 1,
    "command": "snapshot",
    "generated_at": "2026-06-04T10:22:33Z"
  },
  "app": {
    "id": "local:9753",
    "name": "MyFlutterApp",
    "package": "com.example.app",
    "version": "1.2.3",
    "device": "iPhone 15 Simulator",
    "device_id": "ios-sim:...",
    "os": "ios",
    "build_mode": "debug",
    "port": 9753
  },
  "collection": {
    "ports_scanned": [9753, 9754, 9755, 9756, 9757, 9758, 9759, 9760, 9761, 9762],
    "wait_ms": 5000,
    "settle_ms": 750,
    "complete": true,
    "warnings": []
  },
  "summary": {
    "logs": 300,
    "errors": 2,
    "warnings": 4,
    "network": 18,
    "failed_requests": 1,
    "active_sse": 1,
    "websockets": 2
  },
  "notable": [],
  "logs": [],
  "network": [],
  "screenshot": null,
  "diagnostics": []
}
```

Failure uses the same envelope shape:

```json
{
  "ok": false,
  "meta": {
    "flog_version": "0.x",
    "schema_version": 1,
    "command": "snapshot",
    "generated_at": "2026-06-04T10:22:33Z"
  },
  "error": {
    "code": "no_flog_app_found",
    "message": "No flog_dart app responded on ports 9753-9762 within 5s.",
    "next_actions": [
      "Run `flog ai doctor --format json`",
      "Check that Flog.init() is called before runApp()",
      "Confirm this is a debug build or FLOG_ENABLED=true"
    ]
  },
  "diagnostics": []
}
```

## Record Shapes

### Logs

```json
{
  "id": "log#188",
  "timestamp": "10:22:31.044",
  "level": "error",
  "tag": "ChatRepository",
  "message": "response parser expected content but got null",
  "stacktrace": {
    "present": true,
    "preview": "#0 ChatRepository.parse...",
    "truncated": true,
    "original_bytes": 12394
  },
  "repeat_count": 1
}
```

### Network

```json
{
  "id": "net#42",
  "protocol": "sse",
  "method": "POST",
  "url": "https://api.example.com/v1/chat/completions",
  "status": 200,
  "network_status": "completed",
  "duration_ms": 1812,
  "source": "app",
  "request": {
    "headers": "redacted",
    "body": {
      "present": true,
      "preview": "{\"model\":\"gpt-...\"}",
      "truncated": true,
      "original_bytes": 4096
    }
  },
  "response": {
    "headers": "redacted",
    "body": {
      "present": false
    }
  },
  "sse": {
    "chunks": 14,
    "merged_fields": {
      "choices.0.delta.content": ""
    },
    "warnings": ["completed_empty_merge"]
  }
}
```

## Notable Diagnostics

`notable` is the AI-oriented index. It should be generated by pure functions
from stores and domain helpers, not by reading TUI state.

Initial notable rules:

- Error logs and high-signal warnings.
- HTTP 4xx and 5xx responses.
- HTTP transport errors and timeouts.
- Orphan responses.
- SSE streams that complete with empty auto-detected merged content.
- SSE streams with chunks but no done event during the collection window.
- WebSocket abnormal close codes or failed opens.
- Large truncated bodies or stack traces.
- Redaction applied to fields the AI may ask to inspect.
- Incomplete replay or collection timeout.

Each notable item should include:

- `id`: stable record id or diagnostic id.
- `severity`: `info`, `warning`, or `error`.
- `kind`: short machine-readable kind.
- `message`: concise human-readable summary.
- `evidence`: list of related ids.
- `next_actions`: optional action hints.

## Redaction and Truncation

Redaction applies by default to:

- `authorization`
- `cookie`
- `set-cookie`
- `x-api-key`
- fields containing `token`, `secret`, `password`, `api_key`, or `apikey`

Body redaction should cover JSON object keys recursively when the body parses
as JSON. For non-JSON text, use conservative pattern redaction for obvious
bearer tokens and key-value secrets.

Truncation defaults:

- Log message preview: bounded by character count and byte count.
- Stack trace preview: first useful frames plus `truncated`.
- Headers: included only when requested, still redacted by default.
- Bodies: omitted by default; previews only when requested or needed for a
  notable diagnostic.
- Screenshot: image file path only, never base64 in JSON by default.

The output must include enough metadata for the AI to know data is incomplete:
`truncated`, `original_bytes`, and `redacted`.

## Architecture

New modules:

```text
src/commands/ai/
  mod.rs
  args.rs
  session.rs
  snapshot.rs
  screenshot.rs
  output.rs
  redact.rs
```

Optional pure domain module:

```text
src/domain/diagnostics.rs
```

Responsibilities:

- `cli.rs`: clap surface for `Command::Ai(AiCommand)`.
- `commands/ai/args.rs`: AI subcommand args and parse helpers.
- `commands/ai/session.rs`: headless discovery, connection, Subscribe, replay
  settle, and bounded watch collection.
- `commands/ai/snapshot.rs`: convert collected `App` state or stores into the
  JSON output model.
- `commands/ai/screenshot.rs`: Flutter screenshot first, platform fallbacks,
  and screenshot JSON.
- `commands/ai/output.rs`: stable serializable envelope structs and error
  helpers.
- `commands/ai/redact.rs`: redaction and truncation helpers.
- `domain/diagnostics.rs`: pure notable detection over `LogEntry`,
  `NetworkEntry`, and domain helpers such as SSE merge and WS chat utilities.

The AI command layer may depend on `transport`, `input`, `run::dispatch`, `app`,
and `domain`. It must not depend on `ui` or ratatui.

No module under `domain/`, `parser/`, `input/`, `transport/`, or `app/` may
depend on the AI command layer.

## Headless Collection Flow

```text
parse args
  -> start_discovery(base_port)
  -> collect DeviceEvent values until timeout or target found
  -> for each selected device and port, resolve TransportAddr
  -> connect or connect_stream
  -> read Hello
  -> create headless App
  -> add ConnectedApp metadata
  -> send Subscribe when supported
  -> dispatch ClientMessage values into App through run::dispatch
  -> wait for settle window
  -> build snapshot output
  -> optionally capture screenshot
  -> print JSON
```

The settle window is important because Dart replays its `FlogStore` buffer
after subscription. A snapshot should wait until no new frame has arrived for
the settle duration or the global wait deadline expires.

## Multi-App Behavior

v1 should keep selection simple:

- If exactly one app responds, select it.
- If multiple apps respond and no selector was provided, return
  `multiple_apps_found` with a compact list and ask the skill to rerun with
  `--app` or `--device`.
- `--app` may match app id, app name, or package name.
- `--device` matches the transport device id.

This avoids surprising the user by analyzing the wrong running app.

## Error Codes

Initial error codes:

- `no_device_found`
- `no_flog_app_found`
- `multiple_apps_found`
- `handshake_timeout`
- `app_busy`
- `replay_incomplete`
- `record_not_found`
- `adb_forward_failed`
- `usbmuxd_connect_failed`
- `flutter_not_found`
- `flutter_devices_failed`
- `flutter_screenshot_failed`
- `screenshot_unsupported`
- `protocol_mismatch`
- `permission_or_authorization_required`
- `internal_error`

Errors should include `next_actions` whenever the user can do something useful.

## Skill Design

Skill name: `flog-inspect`.

Trigger examples:

- "Use flog to inspect the app."
- "Look at my Flutter logs."
- "Why did this request fail?"
- "Check the current page with flog."
- "The page is stuck loading."
- "Analyze the latest SSE or WebSocket traffic."

Core workflow:

1. Choose whether the prompt is visual. If it mentions page, screen, UI,
   loading spinner, button, layout, or "current page", include
   `--screenshot`.
2. Run `flog ai snapshot --format json --last 300` plus selected flags.
3. If `ok=false`, explain the failure using `error.code` and `next_actions`.
4. If `ok=true`, inspect `summary`, `notable`, `logs`, `network`, and
   screenshot path if present.
5. Run `flog ai get <id>` only for the smallest additional detail needed.
6. Ask before using `--no-redact`. Prefer `--include-body` with redaction.
7. In the answer, cite stable ids such as `net#42` and `log#188`.
8. Separate visual observations from data-derived conclusions.

The skill should be concise. It should not include a copy of the wire protocol.
It should treat the CLI JSON schema as the contract.

## Testing

### Rust Unit Tests

- Clap parses `flog ai` subcommands and key flags.
- Duration parsing accepts useful forms and rejects ambiguous input.
- JSON envelope serialization is stable.
- Error envelope contains code, message, and next actions.
- Redaction covers sensitive headers and nested JSON body keys.
- Truncation records `truncated` and `original_bytes`.
- Notable generation covers:
  - error log
  - warning log
  - HTTP 4xx and 5xx
  - HTTP error
  - orphan response
  - completed empty SSE merge
  - active SSE with chunks but no done
  - abnormal WS close

### Integration Tests

- Fake flog server sends Hello, logs, HTTP, SSE, and WS frames; snapshot returns
  expected ids and summaries.
- Fake server replay plus settle avoids missing the final replayed frame.
- No app found returns `no_flog_app_found`.
- Multiple responding apps returns `multiple_apps_found`.
- `get net#...` finds a record from replay and returns detailed fields.

### Screenshot Tests

Most screenshot behavior is platform-dependent. Unit-test command selection and
error mapping through pure helpers. Manual verification should cover at least:

- Android emulator with Flutter screenshot.
- iOS simulator with Flutter screenshot.
- Flutter missing from PATH.
- Locked or unavailable device returning a structured screenshot error.

## Rollout Plan

1. Land the CLI JSON schema and pure helpers behind tests.
2. Implement headless snapshot collection against fake and real local servers.
3. Add redaction, truncation, and notable diagnostics.
4. Add `get` and bounded `watch`.
5. Add screenshot support.
6. Add `flog-inspect` skill.
7. Document user-facing examples in README after the behavior is stable.

## Open Design Decisions Resolved for v1

- Use headless CLI first, not MCP or a TUI-hosted API.
- Use Flutter screenshot first, not direct platform tools first.
- Do not make screenshots default.
- Do not add app-internal screenshot protocol in v1.
- Do not allow AI write/control actions in v1.
- Return JSON for both success and failure.
- Prefer multiple-app failure over guessing.
