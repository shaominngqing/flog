# AI Low-Token Inspection Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `flog ai` support low-token, progressive inspection with lightweight snapshots, filtered log/network lists, single-record detail, and cURL extraction.

**Architecture:** Keep all AI commands under `src/commands/ai/`, reusing the existing headless session collector. Add pure list/detail/curl builders so behavior is testable without devices, then wire those builders through `cli.rs` and `commands/ai/mod.rs`.

**Tech Stack:** Rust, clap, serde_json, existing flog domain stores and connector/session helpers.

---

### Task 1: CLI Surface

**Files:**
- Modify: `src/cli.rs`

- [x] Add `flog ai logs`, `flog ai net`, and `flog ai curl`.
- [x] Change `snapshot --last` default to `20` and add `--net-last` default `20`.
- [x] Add `get --detail`; default `get` stays compact.
- [x] Add CLI parser tests for the new commands and flags.

### Task 2: Pure AI List Builders

**Files:**
- Create: `src/commands/ai/logs.rs`
- Create: `src/commands/ai/net.rs`
- Modify: `src/commands/ai/snapshot.rs`

- [x] Add failing tests for log list filtering by level/tag/search/last.
- [x] Add failing tests for network list filtering by failed/status/method/url/protocol/slow/last.
- [x] Add `net_last` support in `SnapshotBuildOptions`.
- [x] Keep list records compact: ids, timestamps/status, preview fields, counts, and no body/header blobs.

### Task 3: Record Detail And cURL

**Files:**
- Modify: `src/commands/ai/get.rs`
- Create: `src/commands/ai/curl.rs`

- [x] Add failing tests showing `get net#id` is compact by default and `get net#id --detail` includes detail.
- [x] Add failing tests for `flog ai curl net#id` producing a usable redacted cURL command.
- [x] Reuse existing record id parsing; reject non-network ids for cURL with a structured error.

### Task 4: Command Wiring And Skill Guidance

**Files:**
- Modify: `src/commands/ai/mod.rs`
- Modify: `skills/flog-inspect/SKILL.md`
- Modify: `README.md`
- Modify: `README_EN.md`

- [x] Route the new commands through the existing session collector.
- [x] Update skill guidance to use the progressive flow: snapshot -> logs/net list -> get/curl -> screenshot.
- [ ] Run `cargo fmt -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --all`, and smoke-test the new commands against the connected Android app when available.

### Task 5: Project Skill Installer

**Files:**
- Modify: `src/cli.rs`
- Create: `src/commands/install_skill.rs`
- Modify: `src/commands/mod.rs`
- Modify: `skills/flog-inspect/SKILL.md`
- Modify: `README.md`
- Modify: `README_EN.md`

- [x] Add `flog install-skill`.
- [x] Embed the skill content in the binary so curl/bash-installed users do not need a source checkout or GitHub download.
- [x] Install project-local adapters with `AGENTS.md` as the main entry, `CLAUDE.md` as an `@AGENTS.md` import, and Cursor as a lightweight `.cursor/rules/flog-inspect.mdc` rule.
- [x] Keep repeated installs idempotent with a managed block.
