---
name: flog-inspect
description: Use when a user asks an agent to inspect Flutter app logs, network traffic, SSE/WebSocket streams, current page state, screenshots, or debugging context through flog instead of copying from the TUI.
---

# flog Inspect

Use the `flog ai` CLI. Do not reimplement the flog wire protocol.

## Workflow

1. Decide if the request is visual. If the user mentions page, screen, UI, loading, button, layout, or current state, include `--screenshot`.
2. Run `flog ai snapshot --format json --last 300`. Add `--screenshot` for visual requests.
3. If `ok=false`, explain `error.code`, `message`, and `next_actions`.
4. If `ok=true`, inspect `summary`, `notable`, `logs`, `network`, and `screenshot`.
5. Use `flog ai get <id>` only for the smallest extra detail needed.
6. Do not use `--no-redact` unless the user explicitly approves exposing secrets.
7. Cite stable ids such as `log#188`, `net#42`, and `chunk#42.13`.
8. Separate visual observations from log/network conclusions.

## Commands

```bash
flog ai snapshot --format json --last 300
flog ai snapshot --format json --last 300 --screenshot
flog ai get net#42 --body
flog ai watch --duration 30s --format ndjson
flog ai doctor --format json
```
