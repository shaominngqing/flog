# Changelog

All notable changes to flog. This project follows
[Semantic Versioning](https://semver.org/).

## 0.6.0 — 2026-06-04

### What's new

- Add basic maintenance commands: `flog update`, `flog uninstall`,
  `flog doctor`, and `flog devices`.
- Add polished CLI output for maintenance commands with status markers,
  loading spinners, and a release-download progress bar.
- `flog doctor` checks GitHub release reachability, local tool availability,
  usbmuxd on macOS, and port status for `9753..9762`.
- `flog devices` lists discovered devices and connectable `flog_dart` apps
  without entering the TUI.

---

## 0.5.2 — 2026-06-03

### Fixed

- Recover iOS USB reconnects when usbmuxd re-attaches the same device serial
  with a new transient DeviceID. Existing retry tasks now read the latest
  discovered device record before opening the next tunnel.
- Keep the release build clean on the current Rust toolchain (`cargo fmt` and
  `cargo clippy --all-targets -- -D warnings`).

---

## 0.5.1 — 2026-05-15

### What's new

- **WebSocket "Connecting" state.** When a flog_dart 0.9.0 client begins a
  WebSocket handshake, the network panel immediately shows a Pending (`...`) entry
  for that connection. The entry upgrades to Active on success or Failed on error.
  Previously the entry only appeared after the handshake completed.

### Internal

- `FlogNetKind` gains a `Connecting` variant (`t: "connecting"`) — wire-format
  addition, fully backward-compatible with older flog_dart clients that don't emit
  this frame.
- `handle_open` updated to upsert: upgrades an existing Pending entry to Active
  rather than always inserting a new entry.

---

## 0.5.0 — 2026-04-27

JSON viewer interactive features: collapsible nodes, copy button with ✓ feedback,
full-value overlay for truncated strings, network tree caching.

---

## 0.4.0 — 2026-04-22

Four-layer architecture cleanup campaign (phases 1–5): `app/` explosion,
`event/` split, `run/` extraction, `ui/` submodule reorganisation.

---

## 0.2.0 — 2026-03-01

SSE Merged View, WS Chat View, device picker overlay, mock rules panel.

---

## 0.1.0 — 2026-02-01

Initial release: logs tab, network inspector (HTTP/SSE/WS), flog_dart
companion package, ADB/usbmuxd device discovery.
