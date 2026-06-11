# flog

```
███████╗██╗      ██████╗  ██████╗
██╔════╝██║     ██╔═══██╗██╔════╝
█████╗  ██║     ██║   ██║██║  ███╗
██╔══╝  ██║     ██║   ██║██║   ██║
██║     ███████╗╚██████╔╝╚██████╔╝
╚═╝     ╚══════╝ ╚═════╝  ╚═════╝
```

**Terminal log viewer + network inspector for Flutter developers.**

![Logs main view](docs/screenshots/logs-main.png)

```bash
curl -fsSL https://raw.githubusercontent.com/shaominngqing/flog/master/install.sh | bash
```

## Screenshots

### ▤ Logs

![Logs detail](docs/screenshots/logs-detail.png)

### ⇄ Network

![Network list](docs/screenshots/network-main.png)

Mock a response from the TUI — the editor, plus a `[Mock]` badge on the request list:

![Mock editor](docs/screenshots/mock-editor.png)

![Network with mock badge](docs/screenshots/network-detail-mock.png)

### SSE streaming

Raw `Events` view:

![SSE events](docs/screenshots/sse-events.png)

`Merged` view — joins multiple chunks into one stream per JSON field (auto-detects OpenAI / Claude format):

![SSE merged](docs/screenshots/sse-merged.png)

### WebSocket

`Chat` view — send on the left (green →), recv on the right (blue ←); `*.delta` messages are coalesced; base64 audio collapses to `[binary N KB]`:

![WS chat](docs/screenshots/ws-chat.png)

`Raw` view — full JSON tree:

![WS raw](docs/screenshots/ws-raw.png)

### Device picker

![Device picker](docs/screenshots/device-picker.png)

## What problem it solves

Flutter log debugging has two pain points:

**Terminal logs are unreadable** — `flutter run` output mixes business logs with system noise, no level coloring, no filtering, JSON collapsed on a single line. Finding what you care about inside a sea of `I/flutter`, `W/1.raster`, `D/TrafficStats` lines is an eyes-first problem.

**Network requests are hard to debug** — inspecting requests means adding `print` statements or firing up DevTools, and every restart means reconnecting. There's no lightweight way to watch HTTP / SSE / WebSocket calls straight from a terminal.

## What flog does

flog is a standalone terminal log viewer + network inspector. Keep it running in one terminal window; your Flutter app connects automatically via `flog_dart` and you get a live view of structured logs and network traffic.

**Two tabs:**

- **▤ Logs** — live log stream with level coloring, aligned tag pills, system-noise filtering, collapsible JSON.
- **⇄ Network** — Flipper-style inspector with HTTP / SSE / WebSocket support; request details, headers, and bodies all viewable.

**No reattachment needed** — flog stays running across `flutter run` restarts; `flog_dart` reconnects automatically. Start flog first or the app first, either works.

## Architecture

flog uses a **Direct Socket + Data Source** architecture:

- **Dart side = data source.** `FlogStore` is a 50 000-message FIFO ring buffer. The app starts recording at launch, independent of whether flog is attached.
- **flog TUI = pure renderer.** On connection, Dart replays the buffer; flog then receives live messages with no gap. Disconnects don't lose data; switching app sessions rebuilds instantly.
- **System logs captured automatically.** `Flog.init()` registers hooks for `debugPrint` / `FlutterError.onError` / `PlatformDispatcher.onError`, so framework exceptions and layout overflows flow into `FlogStore` with no `flutter logs` needed.

No VM Service dependency — logs don't travel via `print` / `developer.log`. No terminal noise — `flog_net` frames don't appear in the Flutter console. Automatic device discovery via `flutter devices`. All platforms covered:

- **macOS / iOS simulator** — direct to `localhost`.
- **Android** — `adb forward` port forwarding (automatic).
- **iOS real device** — usbmuxd USB port forwarding.

## Logs features

- Level filter (Verbose / Debug / Info / Warning / Error).
- Tag filter with include + exclude, regex supported.
- Full-text search (regex with `/pattern/i`, match highlighting, `n` / `N` jump).
- Exclude search (drop any row matching the pattern).
- Detail panel (collapsible JSON tree, syntax highlight, depth-aware coloring).
- Bookmarks (right-click to toggle; survive session restart).
- Log export (dumps the filtered view to a file).
- Stats (level distribution, tag ranking).
- Consecutive-duplicate folding.
- Jump-to-Bottom pill (appears when you scroll off the tail; shows buffered count).
- 100 000-entry ring buffer.

## Network features

- HTTP / SSE / WebSocket support.
- Request list (Protocol, Method, URL, Status, Duration, Size).
- Detail panel (collapsible JSON tree):
  - General (URL / Method / Status / Duration / Size).
  - Query Parameters (auto-parsed from URL).
  - Request + Response Headers.
  - Request + Response Body (JSON pretty-print with syntax colors).
  - SSE Events (per-chunk JSON with **Merged View** — automatic field concatenation for OpenAI / Claude streaming).
  - WebSocket Messages (**Chat View**: direction-aware columns, type labels, delta concatenation, binary blob folding — with Raw fallback).
- Inline filter pills (Protocol / Method / Status).
- URL search + exclude.
- Copy as cURL (HTTP only).
- Copy Response (or the Merged / Chat text in the streaming modes).
- **Replay** — resend a captured request from the detail panel.
- **Performance stats** — latency percentiles, top-5 slowest, status distribution, per-domain breakdown.
- **Mock** — author rules in the TUI (URL pattern / method / status / body / delay); synced to the running Dart app via the WebSocket control channel. Intercepted requests resolve locally and still appear in the inspector tagged `Mocked` (HTTP only).
- **SSE Merged View** — concatenate a chosen JSON field across all SSE chunks; automatic LLM streaming detection. Per-URL rules persist across calls.
- Auto-scroll + LIVE indicator.
- 10 000-entry ring buffer.

## Usage

```bash
# Start flog (default port 9753)
flog

# Custom port
flog --port 9754

# Initial filters
flog --level w
flog --tag network,-flog_net
```

### AI inspection

`flog ai` provides a headless JSON interface for AI agents:

```bash
flog ai snapshot --format json --last 20 --net-last 20
flog ai logs --level error --last 20
flog ai net --failed --last 20
flog ai get net#42 --detail
flog ai curl net#42
flog ai snapshot --format json --screenshot
flog ai doctor --format json
```

The output is read-only, redacted by default, and uses stable ids such as
`log#12` and `net#42` so an agent can cite evidence without copying from the
TUI. Snapshots stay compact by default; use `get --detail` only for the exact
record that needs large fields.

Install the AI guidance into the current project:

```bash
cd path/to/your/flutter-app
flog install-skill
```

This writes project-level guidance with `AGENTS.md` as the main entry,
`CLAUDE.md` importing it via `@AGENTS.md`, and
`.cursor/rules/flog-inspect.mdc` as a lightweight Cursor adapter. The full
workflow lives at `.flog/skills/flog-inspect/SKILL.md`. There is no shared
cross-agent skill directory today, so flog installs these adapters by default;
use `--agent codex|claude|cursor` to target one tool.

## With flog_dart

flog recognises any Flutter log output; pair it with [flog_dart](https://pub.dev/packages/flog_dart) for precise level / tag parsing and the Network Inspector:

```yaml
# pubspec.yaml
dependencies:
  flog_dart: ^0.10.0
```

### Bootstrap

Call `Flog.init()` as early as possible in `main()` — synchronous, non-blocking:

```dart
import 'package:flog_dart/flog_dart.dart';

void main() {
  WidgetsFlutterBinding.ensureInitialized();
  Flog.init();
  runApp(MyApp());
}
```

### Network Inspector (recommended)

Swap `Dio()` for `FlogDio()` — zero config, auto-injected HTTP logging + mock:

```dart
import 'package:flog_dart/flog_dart.dart';

final dio = FlogDio(baseUrl: 'https://api.example.com');

// Normal Dio API; every request appears in the flog Network panel
final response = await dio.get('/users');

// Built-in SSE support
final sse = await dio.sse('/chat/completions',
  method: 'POST',
  data: {'prompt': 'hello'},
);
await for (final event in sse.stream) {
  print(event);
}
```

> Release builds tree-shake automatically: `flogEnabled` is `false` in release and all flog code is removed by AOT.

### Logging

```dart
import 'package:flog_dart/flog_dart.dart';

final log = FlogLogger('Network');
log.i('-> GET /api/users');
log.e('Connection failed', error: e, stackTrace: st);
```

### Manual interceptors

```dart
final dio = Dio();
dio.interceptors.addAll([
  FlogHttpInterceptor(),        // ← must be FIRST
  ApiResponseInterceptor(),
  LoggingInterceptor(),
]);
```

> **Ordering matters.** `FlogHttpInterceptor` must sit before any interceptor that calls `handler.reject()`. Otherwise flog never sees the response and the request stays Pending forever.

### SSE

```dart
await for (final data in FlogSseParser.wrap(
  response.data!.stream,
  url: '/api/chat/completions',
  method: 'POST',
)) {
  final json = jsonDecode(data);
  // ...
}
```

### WebSocket

```dart
final ws = await FlogWebSocket.connect('wss://example.com/ws');
ws.send(jsonEncode({'type': 'hello'}));
ws.stream.listen((data) => print(data));
await ws.close();
```

## Keyboard shortcuts

Press `?` inside flog for the full interactive help.

### Logs

| Key | Action |
|-----|--------|
| `1` / `2` | Switch Logs / Network tab |
| `/` | Focus Search (supports `a|b`, `/regex/`, `/regex/i`) |
| `\` | Focus Exclude |
| `t` | Focus Tag filter (e.g. `+network|-flog_net`) |
| `n` / `N` | Next / previous match |
| `j/k` or arrows | Move selection |
| `PgUp` / `PgDn` | Page scroll |
| `Home` / `End` | Top / bottom |
| `G` | Jump to bottom (resume LIVE) |
| `Enter` | Toggle detail panel |
| Right-click | Toggle bookmark |
| `c` | Copy selected log |
| `e` | Export filtered logs to file |
| `S` | Statistics view |
| `s` | Text-selection mode |
| Click `⇅ AppName …` | Open device picker (switch app) |
| `?` | Help |
| `Esc` | Clear filters / close overlay |
| `q` | Quit |

### Network

| Key | Action |
|-----|--------|
| `/` | URL search |
| `\` | Exclude search |
| `c` | Copy as cURL (HTTP only) |
| `y` | Copy response (Merged / Chat in streaming modes) |
| `r` | Replay request (HTTP only) |
| `M` | Create mock rule from selected (HTTP only) |
| `Ctrl+M` | Open mock rules panel |
| `S` | Stats overlay |
| `E` / `C` | Expand all / collapse all JSON sections |
| `Enter` | Toggle detail panel |
| `j/k` | Move selection (or switch field in SSE Merged mode) |
| `G` / `End` | Jump to bottom |
| `Esc` | Exit merged mode / clear filters |
| `s` | Text-selection mode |

## Installation

```bash
# One-liner
curl -fsSL https://raw.githubusercontent.com/shaominngqing/flog/master/install.sh | bash

# Or via Cargo
cargo install --path .
```

Supported: macOS (Intel / Apple Silicon), Linux (x86_64 / aarch64), Windows.

### Maintenance Commands

```bash
flog update      # update from the latest GitHub Release, with confirmation
flog uninstall   # remove flog and local config; exported flog_*.log files stay
flog doctor      # check update network, adb, usbmuxd, and ports 9753..9762
flog devices     # list discovered devices and flog_dart apps
```

These commands do not enter the TUI, so they are useful for maintenance and
diagnostics. `doctor` distinguishes `free`, `flog_dart <app>`, and
`open, not flog_dart` ports; `devices` performs a short scan and returns early
once it finds a connectable `flog_dart` app.

## Contributor docs

- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) — high-level architecture.
- [`docs/MODULES.md`](docs/MODULES.md) — per-module index.
- [`docs/PROTOCOL.md`](docs/PROTOCOL.md) — wire protocol spec.
- [`docs/CONTRIBUTING.md`](docs/CONTRIBUTING.md) — audit taxonomy, testing rules, commit format.

Current version (**0.8.0**) — adds Network Timing collection and terminal timelines, plus progressive `flog ai` inspection commands
and `flog install-skill` project guidance for Codex, Claude Code, and Cursor,
while keeping AI output compact and redacted by default. See
`docs/superpowers/` for the campaign audit trail.

## License

MIT

---

[中文](README.md)
