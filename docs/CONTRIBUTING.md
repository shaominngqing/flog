# Contributing to flog

Read [ARCHITECTURE.md](ARCHITECTURE.md) and [MODULES.md](MODULES.md) first
if you are new — this document covers process (setup, tests, file
budget, audit discipline, commit style) rather than what the code does.

## 1. Setup

### Rust toolchain

```bash
rustup toolchain install stable
rustup component add clippy rustfmt
```

Minimum version: whatever `Cargo.toml`'s resolver settles on. There is
no explicit MSRV pin; the CI matrix is the contract.

### Flutter toolchain (only if you'll run against a real app)

```bash
# Install the Flutter SDK separately — https://docs.flutter.dev/get-started/install
flutter doctor -v
```

Used for device discovery (`flutter devices --machine`), the
`flog_dart` package, and any end-to-end testing against a real app.

### Coverage tool (optional)

```bash
cargo install cargo-llvm-cov
```

Phase 2.5B set coverage floors (see §4 below).

## 2. Build and test commands

```bash
cargo build                           # debug
cargo build --release                 # release
cargo test --all                      # Rust test suite
cargo test <name> -- --nocapture      # single test with stdout
cargo clippy --all-targets -- -D warnings
cargo fmt -- --check
cargo install --path .                # installs to ~/.cargo/bin/flog

# flog_dart (from flog_dart/)
dart test
dart analyze
```

CI runs `cargo fmt --check`, `cargo clippy -- -D warnings`, and
`cargo test --all` on every commit. A PR that breaks any of those is
blocked.

## 3. Running locally

### Against a real Flutter app

```bash
# Terminal A: start flog (waits for apps to connect)
flog

# Terminal B: add flog_dart to your Flutter app's pubspec.yaml,
# call Flog.init() early in main(), run the app:
flutter run
```

flog discovers the device via `flutter devices --machine`, scans
ports `9753..9762`, completes the Hello handshake, and starts
receiving `Log` + `Net` frames.

Flags:

```bash
flog --port 9753            # override base port
flog --level w              # start with min level = Warning
flog --tag network,-flog_net   # include network, exclude flog_net
```

### Offline / headless

There is no stdin-pipe mode today — logging must go through the
WebSocket protocol. The in-repo integration test
`tests/ws_server_test_direct.rs` is the closest thing to an offline
smoke test; it spins up `support/fake_flog_server.rs` and exercises
the Connector end-to-end.

## 4. Testing strategy

### 4.1 The 5-class audit taxonomy

Phase 1 of the cleanup campaign classified every finding against one
of five labels (see `docs/superpowers/specs/2026-04-22-project-cleanup-design.md`):

| Label | Meaning                                                        | Treatment |
|-------|----------------------------------------------------------------|-----------|
| **A** | Correct-but-ugly — behaviour right, code inelegant             | Phase 3 redesign, A-class test freezes behaviour. |
| **B** | Confirmed bug — behaviour wrong                                | Phase 2.5 writes a red / ignored test, Phase 3 makes it green. |
| **C** | Ambiguous — unclear if feature or bug                          | Resolved with the user (all C-class reached 0 by Phase 1 exit). |
| **D** | Architecture smell — abstraction missing, responsibility wrong | Phase 3 redesign, D-class characterization test freezes behaviour. |
| **E** | Mechanical — 0-risk tidy-up (clippy etc.)                      | Phase 2 only. |

New findings uncovered mid-flight get written up as audit addenda in
`docs/superpowers/audit/*.md` with the same schema (id / label /
location / evidence / proposed_action / risk).

Raw audit findings: [docs/superpowers/audit/](superpowers/audit/).
Campaign journals: [docs/superpowers/journal/](superpowers/journal/).

### 4.2 Characterization over TDD (for existing behaviour)

Phase 2.5B established the regression fence. The rule is:

> Before changing existing behaviour, lock the current behaviour with
> a characterization test. A ← freezes "correct" behaviour before the
> redesign that makes it pretty; D ← freezes "current" behaviour
> before the redesign that makes it correctly-shaped; B ← writes the
> *expected* behaviour as a red test, marked `#[ignore = "bug: <id>, fix in Phase 3"]`,
> which turns green in Phase 3 along with the fix.

This is why `tests/characterization_*.rs` exists — the green tests
are the safety net Phase 3 leaned on to move fast without regressing
the 2 000+ existing behaviours.

For **new** behaviour, use TDD normally: write the failing test, make
it pass. Do not mix characterization + TDD for the same change.

### 4.3 Rules 2 / 3 / 9 / 10 / 11

From the cleanup campaign spec (`§5.1`):

1. **Rule 2 — A-class tests stay green.** If a refactor turns an
   A-class test red, the refactor broke correct behaviour; revert or
   fix before proceeding.
2. **Rule 3 — B-class tests go from ignored → green by Phase 3 exit.**
   The `"bug: <id>, fix in Phase 3"` `#[ignore]` label is the bug-fix
   todo list.
3. **Rule 9 — every redesign sub-step ships new tests** that pin the
   *new structure's* contract (not behaviour — behaviour is already
   pinned by A/D characterization tests).
4. **Rule 10 — one module at a time.** Cross-module couplings become
   their own sub-step.
5. **Rule 11 — diff review after every sub-step.** Remove dead code,
   stale comments, half-landed abstractions. This is how the campaign
   closed with zero `#[allow(dead_code)]` and 2 166 tests.

### 4.4 Coverage floors

Phase 2.5B enforced with `cargo-llvm-cov`:

- `src/event/**`, `src/app/**` — **≥ 70 %** branch coverage.
- `src/domain/filter.rs`, `src/domain/network_filter.rs` — **≥ 85 %** branch.
- `src/ui/logs/**`, `src/ui/network/**` (extracted logic fns only) —
  **≥ 70 %**.
- Everything else — **≥ 60 %** overall.

Verify locally:

```bash
cargo llvm-cov --html --ignore-filename-regex 'tests/|_tests.rs$'
open target/llvm-cov/html/index.html
```

## 5. File-size convention (§5.5 of the cleanup spec)

A signal, not a judgement. The Audit design call overrides any
numerical threshold.

| Range      | Policy                                                                         |
|------------|--------------------------------------------------------------------------------|
| < 300 LOC  | Comfortable.                                                                   |
| 300–500    | Green — no concerns.                                                           |
| 500–800    | Yellow — the Step design document must include one sentence on why it can stay. |
| > 800      | Red — default **must split**. Whitelist exceptions (large `match`, protocol type, pure constant table) require user approval. |

### Test-file sibling pattern

To keep production source files small **and** keep tests co-located
with the code they exercise, use the `#[cfg(test)] #[path = "…"] mod tests;`
pattern:

```rust
// src/domain/filter.rs (production)

// … module code …

#[cfg(test)]
#[path = "filter_tests.rs"]
mod tests;
```

```rust
// src/domain/filter_tests.rs (tests)
use super::*;
// … tests …
```

Every test file in `src/**/*_tests.rs` follows this pattern. When you
split a production module, split its tests too and update the `#[path]`
attribute.

## 6. Audit-driven refactoring

### Writing an audit entry

When you notice a design issue you don't plan to fix in the current
change, add an entry to the appropriate audit file:

- `docs/superpowers/audit/01-transport.md`
- `docs/superpowers/audit/02-domain.md`
- `docs/superpowers/audit/03-ui.md`
- `docs/superpowers/audit/04-flog-dart.md`

Use the established schema:

```yaml
id: TRANS-nnn         # prefix matches the scope file
label: D              # one of A / B / D / E (C resolved before Phase 3)
location: src/transport/...rs:NN-MM
title: <1-line summary>
evidence: |
  <3–10 line code reference + observed behaviour>
proposed_action: |
  A/D — redesign direction
  B — expected behaviour
  E — specific mechanical fix
risk: low | medium | high
```

Keep the entry factual; no "TODO" / "might" / "probably" language —
either it's a finding (with a label) or it's not.

### Writing a phase / step plan

Plans live in `docs/superpowers/plans/YYYY-MM-DD-<slug>.md`. Use
a past plan as a template. Each plan lists:

- Context / HEAD at start.
- Red lines (what MUST NOT be touched).
- A numbered task list; one commit per task.
- Exit gates.

Plans are executed with the `superpowers:subagent-driven-development`
or `superpowers:executing-plans` skills; the subagent-spawning caller
keeps the plan as the single source of truth so retries and
mid-session hand-offs remain consistent.

## 7. Commit conventions

### Message format

```
<type>(<scope>): <summary> (Phase N <audit-id>)

<body explaining *why*; wrap at ~72 chars>
```

Types follow Conventional Commits: `feat`, `fix`, `refactor`, `docs`,
`test`, `chore`. Scope is the module or subsystem (e.g. `transport`,
`domain`, `ui/logs`, `flog_dart`). The `Phase N` + audit id tail is
optional but preferred for cleanup-campaign commits.

Examples from the current `git log`:

```
refactor(main): extract server + render_loop (Phase 4)
fix(discovery): verify flog_dart identity and clean up ghost devices
docs(arch): add ARCHITECTURE.md (Phase 5)
test(flog_dart/sse): DART-001 repro guards — W3C multi-line data + multi-event-per-chunk
```

### One commit per plan task

When executing a written plan, one task = one commit. Don't squash
unrelated work into the same commit; don't split one task across
multiple commits. This keeps `git log` readable and `git bisect`
useful.

### No auto-skip of hooks

Don't pass `--no-verify`; fix the underlying issue. The pre-commit
hooks (clippy, fmt, test) are the last line of defence.

## 8. Working with flog_dart

`flog_dart/` is published to [pub.dev](https://pub.dev/packages/flog_dart)
and is **public API surface** for every Flutter developer using flog.
Changes here have a different bar than internal Rust refactors.

### Red lines

- **Do not change public API signatures in a patch release.** Adding
  optional positional / named params is fine; removing or renaming
  public classes, methods, or constants is not.
- **Do not regress `flogEnabled` tree-shaking.** Every new feature
  must be wrapped in `if (!flogEnabled) return;` at the entry points
  so release-mode apps pay zero cost.
- **Do not log from mock / replay paths without guarding on
  `flogEnabled`.** DART-004 was this exact bug.

### Release flow

1. Make the change under `flog_dart/`.
2. Bump `flog_dart/pubspec.yaml` `version:` following semver.
3. Add a `## <version>` entry at the top of `flog_dart/CHANGELOG.md`.
4. `cd flog_dart && dart test && dart analyze`.
5. `dart pub publish --dry-run` from inside `flog_dart/`.
6. Commit + tag + push, then `dart pub publish`.

### v0.8 migration (forward reference)

The next breaking release of `flog_dart` will reshape the SSE
subsystem (see [PROTOCOL.md §9.1](PROTOCOL.md) and audit DART-033):

- `FlogSseParser` becomes a `StreamTransformer<List<int>, SseEvent>`.
- `FlogDio.sse` will expose the raw byte stream alongside typed events.
- Byte buffer gains a hard limit.

The **wire protocol stays unchanged**. flog TUI 0.4.x will keep
working against both v0.7.x and v0.8.x; migration touches only
Dart-side call sites.

## 9. AI reading order

If you are an AI assistant picking this codebase up cold, read in
this order:

1. [CLAUDE.md](../CLAUDE.md) — project-specific agent instructions.
2. [ARCHITECTURE.md](ARCHITECTURE.md) — the four-layer model, data flows.
3. [PROTOCOL.md](PROTOCOL.md) — wire format so you don't invent fields.
4. [MODULES.md](MODULES.md) — the index from "which file has the X?" to a path.
5. The relevant source files themselves.

Do not skip to step 5 on a non-trivial change. The audit trail
(`docs/superpowers/audit/`) and the phase journals
(`docs/superpowers/journal/`) record the reasoning behind existing
shapes that may look arbitrary out of context.
