# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Is

flog is a terminal-native log viewer for Flutter developers, written in Rust. It connects to Flutter apps via VM Service WebSocket, ADB logcat, or stdin pipe and displays structured, filterable logs in an interactive TUI.

## Build & Test Commands

```bash
cargo build                    # Debug build
cargo build --release          # Release build
cargo test                     # Run all tests
cargo test --test ws_connect_test -- --nocapture  # Single test with output
cargo clippy                   # Lint
cargo fmt                      # Format
```

## Architecture

Four-layer architecture with strict dependency direction: `ui → app → domain ← parser/input`.

### Layers (all under `src/`)

- **`domain/`** — Pure data types with zero UI dependencies
  - `entry.rs` — `LogEntry`, `LogLevel`, `InputSource` types
  - `filter.rs` — `FilterState` with level/tag/search filtering, pre-compiled regex
  - `store.rs` — Ring-buffer log storage (100K cap, drains oldest 10% when full, folds consecutive duplicates)

- **`parser/`** — Strategy-pattern log format parser chain, tried in order:
  1. `structured.rs` — Structured `[LEVEL][Tag] message` format
  2. `generic.rs` — Flutter standard patterns (`I/flutter`, VM Service timestamps, exception blocks)
  3. `keyword.rs` — Fallback heuristic scanning for level keywords
  - Unrecognized lines get SYSTEM level (never dropped)

- **`input/`** — Async input source abstraction
  - `discover.rs` — Auto-discovers Flutter VM Service via `ps aux` scanning
  - `vm_service.rs` — WebSocket connection to Dart VM Service
  - `adb.rs` — ADB logcat integration
  - `stdin_source.rs` — Pipe mode (`flutter run | flog --stdin`)
  - All sources emit `SourceEvent` (RawLine, RawLineWithTimestamp, ParsedEntry)

- **`ui/`** — ratatui-based TUI with Catppuccin Macchiato theme
  - `mod.rs` — Main rendering (log list, toolbar, status bar)
  - `detail.rs` — JSON detail panel with syntax highlighting
  - `source_select.rs` — Source picker UI
  - `stats.rs` — Statistics view (level distribution, tag ranking)
  - `timeline.rs` — Timeline heatmap
  - `help.rs` — Keyboard shortcut overlay
  - `highlight.rs` — JSON syntax highlighting

### Key Top-Level Modules

- `app.rs` — Central state machine (`AppMode`: Normal, Search, TagFilter, Help, Stats, SourceSelect)
- `event.rs` — Keyboard/mouse event dispatch
- `cli.rs` — CLI argument parsing (clap)
- `session.rs` — Session persistence to `~/.config/flog/session.toml`
- `main.rs` — Tokio async entry point, terminal setup, event loop

### Concurrency Model

Tokio multi-threaded runtime. Source tasks run in background, sending `SourceEvent`s through channels. Main thread polls terminal events and source events in a unified loop. App state is behind `Arc<Mutex<App>>`.

## flog_logger Dart Package

`flog_logger/` contains a lightweight Dart package published on pub.dev. It provides `FlogLogger` class that prints `[LEVEL][Tag] message` format — the structured format that the Rust parser recognizes natively. Pure Dart, no Flutter SDK dependency.

## CI/CD

GitHub Actions (`release.yml`) builds on tag push (`v*`) for 5 targets: macOS x86_64/aarch64, Linux x86_64/aarch64, Windows x86_64. Artifacts are packaged and uploaded to GitHub Releases.
