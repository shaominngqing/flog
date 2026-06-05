---
name: flog-inspect
description: Use when a user asks an agent to inspect Flutter app logs, network traffic, SSE/WebSocket streams, current page state, screenshots, or debugging context through flog instead of copying from the TUI.
---

# flog Inspect

Use the `flog ai` CLI. Do not reimplement the flog wire protocol.

## When Triggered

Users may not say "use flog" or name this skill. If they ask you to inspect a Flutter app's current state, logs, HTTP requests, SSE/WebSocket traffic, loading issue, button/page behavior, or screenshot-worthy UI problem, use this workflow proactively.

If device/app selection is ambiguous, first run the smallest snapshot without guessing. If it reports multiple apps, rerun with `--device` and/or `--app` from the error's next actions.

## Workflow

1. Decide if the request is visual. If the user mentions page, screen, UI, loading, button, layout, or current state, include `--screenshot`.
2. Start small: run `flog ai snapshot --format json --last 20 --net-last 20`. Add `--screenshot` for visual requests.
3. If `ok=false`, explain `error.code`, `message`, and `next_actions`.
4. If `ok=true`, inspect `summary`, `notable`, `logs`, `network`, and `screenshot`.
5. If the snapshot is not enough, narrow first:
   - Logs: `flog ai logs --level error --last 20`, `flog ai logs --tag Auth --last 20`, or `flog ai logs --search timeout --last 20`.
   - Network: `flog ai net --failed --last 20`, `flog ai net --status 5xx --last 20`, `flog ai net --method POST --url login --last 20`, or `flog ai net --slow 1000ms --last 20`.
6. Use `flog ai get <id> --detail` only after choosing a stable id from snapshot/list output.
7. Use `flog ai curl net#42` when the user asks how to reproduce a request.
8. Do not use `--no-redact` unless the user explicitly approves exposing secrets.
9. Cite stable ids such as `log#188`, `net#42`, and `chunk#42.13`.
10. Separate visual observations from log/network conclusions.

## Response Pattern

- Lead with what the app is doing now, not with the command output.
- Cite the evidence ids you used.
- Say when data is incomplete, for example no connected app, no failed requests, replay buffer missing an id, or screenshot capture failed.
- Ask the user to reproduce the action only when the current snapshot/list/detail does not contain enough evidence.

## Commands

```bash
flog ai snapshot --format json --last 20 --net-last 20
flog ai snapshot --format json --last 20 --net-last 20 --screenshot
flog ai logs --level error --last 20
flog ai net --failed --last 20
flog ai get net#42 --detail
flog ai curl net#42
flog ai watch --duration 30s --format ndjson
flog ai doctor --format json
```
