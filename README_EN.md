# flog

**Flutter Log Viewer — see your logs, finally.**

A terminal-native, cross-platform, intelligent log viewer for Flutter developers. One command. Zero config.

```bash
curl -fsSL https://raw.githubusercontent.com/shaominngqing/flog/master/install.sh | bash
```

Or via Cargo:

```bash
cargo install flog
```

## Features

- **Cross-platform** — Android (ADB), VM Service WebSocket (all platforms), stdin pipe
- **Intelligent** — Multi-strategy parser chain auto-detects structured `[LEVEL][Tag]` format, generic Flutter patterns, and keyword-based levels
- **Interactive** — Search (text or `/regex/i`), filter by level/tag, bookmarks, statistics, timeline heatmap
- **Mouse-friendly** — Click to select, double-click for detail, right-click to bookmark
- **Persistent** — Saves session (filters, bookmarks, search) across runs
- **Non-intrusive** — Connects via DDS proxy, never blocks `flutter run`

## flog_logger Integration

flog natively parses the [flog_logger](https://pub.dev/packages/flog_logger) structured format:

```dart
final log = FlogLogger('Network');
log.i('-> GET /api/scene-types');
log.d('  query: {_productId: 66000001}');
```

Output in your terminal:

```
[INFO][Network] -> GET /api/scene-types
[DEBUG][Network]   query: {_productId: 66000001}
```

flog displays these logs with proper level coloring, tag filtering, and full-width messages — no truncation, no noise.

## Usage

```bash
# Auto-detect — scans for running Flutter VM, connects via DDS proxy
flog

# Connect to a specific VM Service WebSocket
flog --uri ws://127.0.0.1:8181/TOKEN=/ws

# Android via ADB logcat
flog --adb
flog --adb -s emulator-5554

# Pipe from any command
flutter run 2>&1 | flog --stdin

# With initial filters
flog --level w --tag Network
flog --level d
```

### Recommended workflow

1. Start `flog` in one terminal (it waits for a Flutter app)
2. Run `flutter run` in another terminal
3. flog auto-discovers and connects within 1-2 seconds
4. Stop/restart `flutter run` freely — flog reconnects automatically

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `/` | Search (text or `/regex/i`) |
| `n` / `N` | Next / previous match |
| `j/k` or `Up/Down` | Scroll |
| `PgUp/PgDn` | Page scroll |
| `Home/End` | Jump to top/bottom |
| `Enter` | Toggle detail panel (JSON pretty-print) |
| `c` | Copy selected entry to clipboard |
| `e` | Export filtered logs to file |
| `?` | Help |
| `S` | Statistics view |
| `Esc` | Close panel / clear all filters |
| `q` / `Ctrl+C` | Quit |

## Mouse

| Action | Effect |
|--------|--------|
| Click row | Select |
| Double-click | Detail view |
| Right-click | Toggle bookmark |
| Scroll wheel | Scroll |
| Click toolbar | Search, filter, change level |
| Click source name | Switch source |

## Architecture

```
src/
├── domain/     — Core types (LogEntry, LogLevel, LogStore, FilterState)
├── input/      — Source abstraction (ADB, VM Service, stdin, auto-discovery)
├── parser/     — Multi-strategy format detection (Structured, Generic, Keyword)
└── ui/         — Terminal UI (ratatui + crossterm)
```

## Log Level Guidelines

| Level | Use for | Examples |
|-------|---------|---------|
| **INFO** | Business milestones | Connection ready, practice started, score result |
| **DEBUG** | Internal state | WS protocol, audio state, token cache, transcripts |
| **WARNING** | Recoverable issues | Session expiring (GoAway), token refresh failed |
| **ERROR** | Failures | Connection failed, parse error, reconnect failed |

## License

MIT

---

[中文](README.md)
