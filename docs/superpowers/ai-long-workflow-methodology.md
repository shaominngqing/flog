# AI long-workflow methodology — a cleanup-campaign case study

**Status:** standalone case study. The concrete project —
[`flog`](https://github.com/shaomingqing/flog), a Rust TUI + Dart
companion — is incidental. The technique generalises to any
existing, mostly-working codebase (Go, React, Python, ...) whose
owner can articulate quality debt and is willing to spend three
days running an AI-assisted cleanup.

**Reading time:** 30 minutes.
**Audience:** engineers considering using LLM agents for multi-day
refactoring or cleanup campaigns. Not an introduction to LLMs or to
software quality — assumes the reader already knows what a
characterization test is, what clippy / ruff / eslint is, and what
"drive subagents from a plan file" means.

## Why this document exists

In April 2026 one engineer ran a six-phase, three-day, ~162-commit
cleanup of a ~20 kLOC Rust/Dart project using AI subagents. It
worked. The user tested every phase boundary locally and signed off
each plan before execution. Coverage went from ~31% to ~90%, 10
oversized files dropped to 0, and 13 tracked bugs closed.

The user's prompt, at session start, said: "也是一个长工作流的AI实
践，所以需要记录一下" — *"this is also a long-workflow AI practice,
so it should be documented."*

This file documents the practice so another engineer can read it on
Monday and start a cleanup on Tuesday.

## 1. The premise — why long workflows fail for LLMs

LLM agents degrade along three axes as a task extends over hours and
days:

1. **Context loss.** The conversation grows past the cache window;
   earlier decisions get summarised into a blurb that omits the load-
   bearing detail. A decision made on hour 2 drifts by hour 18.
2. **Drift.** Without a committed artifact at every milestone, the
   agent's next turn re-derives intent from a compressed chat log.
   Re-derivation is not identity: the plan at hour 20 may be a
   subtly different plan than the one the user approved at hour 0.
3. **Over-abstraction.** Left to its own judgment, an LLM will
   create an abstraction whenever it sees a hand-written pattern
   twice. A codebase refactored for an hour by an unsupervised LLM
   often has one extra trait for every three it needed.

The workflow below is a set of artifacts, gates, and subagent-
dispatch disciplines designed to counter all three failure modes at
once. It is **not** prompt engineering — it is *process
engineering*. The prompts given to subagents are ordinary; what
makes the campaign reliable is what's written down before each
prompt runs.

## 2. The six-phase model

A linear sequence, one phase per commit-cluster, tests green at
every boundary, no code change in the final phase:

| Phase | Purpose                                        | Deliverable                          | Can this phase fail silently? |
|-------|------------------------------------------------|--------------------------------------|-------------------------------|
| 0     | Brainstorm scope + write the phase contract    | design spec + decision journal       | yes if skipped — entire campaign sets up wrong |
| 1     | Audit (read-only, classify findings)           | per-scope audit files + index        | no — read-only, verifiable by reading the code |
| 2     | Mechanical cleanup (lint, dead code, fmt)      | clippy/lint 0 warnings               | no — compiler verifies |
| 2.5   | Testability + characterization                 | regression fence (>90% coverage target) | yes if Rule 2/9/10 gates skipped |
| 3     | Redesign (step-by-step, one cluster per step)  | code reshape, B-class fixes land     | yes — this is the risk-heavy phase |
| 4     | Residual cleanup + why-comments                | file-budget compliance + targeted comments | no — small delta, guarded by tests |
| 5     | Documentation                                  | engineering + user docs              | no — docs-only |
| 6     | Retrospective + methodology                    | this file's cousin for the project   | no — docs-only |

The model is not original — it's a fusion of:
- **read before write** (Phase 1 before Phase 2),
- **test before refactor** (Phase 2.5 before Phase 3),
- **compile-green-at-every-commit** (standard),
- **documented exit before close** (Phases 5 + 6).

What's original is the *allocation of work between user and AI* at
each phase. See §6.

## 3. The audit taxonomy — A/B/C/D/E in five labels

Before any code change, every finding gets exactly one of five
labels. The distribution from the flog campaign:

| Label | Meaning                                     | Count (flog) | Handled by |
|-------|---------------------------------------------|--------------|------------|
| **A** | Correct-but-ugly behaviour (keep, just clean) | 27         | Phase 3 — A-class test freezes behaviour before touch |
| **B** | Confirmed bug                               | 13 → 14      | Phase 2.5 red/ignored test → Phase 3 un-ignore on fix |
| **C** | Ambiguous — is it a feature or a bug?        | 0 (forced)  | Resolved with user before Phase 2 — taxonomy gate |
| **D** | Architecture smell                          | 66 + addenda | Phase 3 redesign with A-class guard |
| **E** | Mechanical 0-risk tidy-up                   | 9           | Phase 2 only — no tests needed |

Every finding has a stable id (`TRANS-009`, `DOM-003`, `UI-041`,
`DART-023`). Ids appear in: commit messages, test names, source
comments, the plan for the owning phase, and the journal that
records the phase's exit. Given any id you can reconstruct every
step it moved through in 2–3 hops of `git log --grep` or file
search.

### Why five labels and not three (or ten)?

- **Two labels** (bug / not bug) has no room for "correct but the
  code is unreadable" (A) — which is the single most common finding
  in a mature codebase. Without A, those findings either get
  ignored (rot) or mis-labelled as bugs (subagent wastes time
  "fixing" what is already correct).
- **Ten labels** (severity × confidence × urgency + ...) is
  overhead without payoff. The subagent driving Phase 2 doesn't
  need to reason about urgency. The user does, once, when choosing
  which phase to start.
- **Five is the empirical sweet spot.** A: "lock it before moving."
  B: "this is wrong, prove it with a failing test first."
  C: "I can't tell without asking the user." D: "this is a design
  problem, not a bug — schedule a redesign with tests in place."
  E: "just do it, it's free." Every finding fits exactly one.

### The `C = 0` discipline

At the end of Phase 1 Task 3 (taxonomy audit), every C-class finding
is adjudicated with the user and re-labelled A / B / D / E. Phase 2
does **not** begin with C-labels outstanding. This gate is the
cleanest "user must be in the loop" moment in the entire campaign
— it forces the user to read the ambiguous findings and commit to
an interpretation. Every subsequent subagent call can then act
without backtracking.

## 4. Key techniques, with concrete instances

### 4.1 Characterization tests as a regression fence

**Principle.** Before you can safely reshape a codebase, you must
pin its current observable behaviour in tests. These tests are not
specifications — they simply declare "whatever the code does today,
that's what it does tomorrow." The flog campaign wrote 1 525 such
tests in Phase 2.5B, including 541 integration tests across 8
crates and 436 lib-tests.

The fence pays off during Phase 3. Any subagent-driven refactor
that breaks an A-class characterization test has two possible
resolutions: either the refactor is wrong, or the test was wrong.
The user then decides, explicitly. There is no silent behaviour
change because there is no silent regression.

**Gates that kept the fence honest:**

- **Rule 2 — per-module coverage gates.** Every core module has a
  hard coverage floor (≥ 90% for domain/, ≥ 85% for ui/, ≥ 80% for
  session/main/cli). A subagent cannot declare done until
  `cargo llvm-cov` reports the module at or above the floor. The
  same idea works for pytest `--cov` or Jest `--coverage`.
- **Rule 9 — multi-scenario.** If an audit entry lists N distinct
  scenarios, the test for it must have N distinct cases. "Filter
  state with include + exclude + regex" is three cases, not one.
- **Rule 10 — per-public-function density.** Core modules require
  ≥ 5 test cases per public function. A trivial getter can be
  exempted; anything that takes decisions cannot.

These numeric gates prevent the single most common subagent failure
mode: delivering a shallow test that covers one happy path and no
edge cases, then claiming the work is complete.

### 4.2 The red-lock pattern for B-class bugs

**Shape:** write an `#[ignore = "bug: <id>"]` test that asserts the
correct behaviour. The test fails (so it's ignored so CI stays
green). In the same commit that fixes the bug, un-ignore the test.
Now CI would fail if the bug returned.

**Three concrete instances from the flog campaign:**

- **DART-001** — the SSE parser dropped all events after the first
  `data:` line in a chunk. The red lock was the pre-existing
  `flog_dart/test/flog_sse_parser_test.dart` suite, committed as-is
  in Phase 1. It compile-failed (W3C tests referenced APIs that
  didn't exist). Phase 3 Step 3.4's fix commit `6179631` both
  implemented `FlogSseParser.wrapTyped` + `SseEvent` and made
  `flutter test` go from 84 pass / 1 fail → 131 pass / 0 fail.
  The test file's header comment was updated from "locks the bug"
  to "locks the fix."
- **UI-042** — WS chat ↔ raw toggle leaked collapse-key state,
  corrupting the render of the adjacent pane. User-reported
  mid-Phase-3. Step 3.6 wrote the red lock (commit `95f97d7`,
  `test(characterization): UI-042 red lock — WS chat↔raw toggle
  state leak`). Step 3.8 shipped the fix (`133b631`) and un-ignored
  the test in the same commit.
- **DOM-003** — HTTP response without prior request was silently
  dropped. Red lock planted in Phase 2.5B Task 12; fix in Phase 3
  Step 3.2 (`7e333a1`).

**Why "red lock" instead of just writing the fix?**

The red-ignored test is a commit-level artifact that encodes "we
know about this bug, we know the correct behaviour, the fix is
scheduled." If a subagent in an intervening phase somehow "fixes"
the bug by mistake — while working on something unrelated — the
test goes from ignored-failing to ignored-passing, and the next
developer sees it and un-ignores it. If a regression rolls back
the fix, the test fails and CI blocks. The pattern is not test-
driven-development (the fix commit isn't driven by the test), it's
**bug-surface-pinning**: the bug has a name, a file, a line, and a
test, before any fix is attempted.

### 4.3 Step = spec + plan + subagent dispatch

Phase 3 of the flog campaign decomposed into 10 steps. Each step
was one atomic unit:

1. A **plan file** under `docs/superpowers/plans/` that names the
   audit ids in scope, the target files, the exit gates.
2. A **spec reference** — the plan cites the Phase 0 design spec
   section that governs this scope (e.g. "§5.3 — transport layer").
3. A **subagent dispatch** or inline execution. For flog, most
   steps were single-round subagents (1 prompt → 1 response → 5–15
   commits). Some were multi-round when a first pass came back
   shallow.
4. A **journal file** under `docs/superpowers/journal/` recording
   the exit state: audit ids locked, files changed, test delta,
   coverage delta, deviations from the plan.

The pattern forces every step to be a self-contained unit. A new
contributor reading the step's plan, journal, and commits can
reconstruct exactly what happened without reading the conversation
that drove it. This artifact-centric design is what lets the
campaign survive LLM context loss.

### 4.4 File-size as signal, not judgment

flog used a **500-line budget** for production files (documented in
`docs/CONTRIBUTING.md §5.5`). The budget is not a law — it's a
signal. Three stances are legal:

- **Under 500**: no action required.
- **500–800 (yellow)**: the author must write a one-sentence
  justification in the file's `//!` module comment ("kept together
  because X is inseparable from Y").
- **Over 800 (red)**: must be split, or the justification must be
  on record in a plan + journal.

Test files (`*_tests.rs` in the sibling pattern) are exempt — they
grow linearly with observable scenarios, and a 700-line test file
is usually 50 small cases, not a readability problem.

**What the budget prevents, concretely.** An LLM will cheerfully
add 40 lines to the end of a 900-line file. A 900-line file has
nowhere the LLM can *see* to put the new code, so it accretes. A
human engineer learns to shudder at 900 lines and splits. An LLM
needs a numeric cue to do the same. The budget is that cue.

**What the budget doesn't prevent.** Over-splitting. See §5 on the
flog Phase 4 Task 2 drift.

### 4.5 Subagent dispatch with characterization fences

A subagent round, in this workflow, looks like:

```
[user-curated plan file]  ──▶  [subagent reads plan]
                               ├─▶ reads audit ids in scope
                               ├─▶ reads current file sizes + coverage
                               ├─▶ confirms gates (tests green, fence intact)
                               ├─▶ executes the plan's tasks
                               │      (1 commit per task, bite-sized)
                               ├─▶ runs `cargo test --all` + clippy + fmt
                               └─▶ writes the journal entry
```

The subagent is never asked "do the right thing" — it is asked
"execute this plan, which has exit gates." If the gates fail, the
subagent either backtracks or surfaces a blocker. The user can
inspect the journal, the commits, and the test output before
accepting the phase boundary.

**Concrete example — Phase 3 Step 3.6 (event dispatch redesign):**
- Plan (`plans/2026-04-24-phase3-step6-event.md`, ~200 lines) lists
  UI-001, UI-007, UI-008, UI-009, UI-016, UI-041 as in-scope.
- Plan requires `ClickRegion` enum in `event/click_region.rs`,
  `detect_click_region` in `event/detect.rs`, `apply_click_region`
  in `event/apply.rs`, zero reduction in `characterization_event_mouse`
  tests.
- Subagent executed 8 commits (ClickRegion scaffold → pure detect
  → classify_click → apply_click_region → pill constants → j/k
  SSE field navigation → mod.rs //! routing invariants → journal).
- Every commit compiled, tested green, clippy clean. The final
  event/mod.rs is 463 lines; the file that was 1677 lines pre-
  campaign is now a 10-file directory with a 35-line
  dispatcher shell.

### 4.6 Two-phase dispatch — "design for testability" as a refactor goal

The flog mouse dispatcher pre-campaign was a single 700-line
function interleaving (a) detecting which UI region the user
clicked, (b) resolving double-click semantics, (c) mutating state.
Coverage plateaued at 61% in Phase 2.5B because the "detection"
logic was unreachable without running the whole dispatcher under
a TestBackend.

Phase 3 Step 3.6 redesigned this as two phases:

```
detect_click_region(app, x, y) -> Option<ClickRegion>   [pure; ~300 lines]
apply_click_region(app, region, click_class, x, y)      [mutating; ~500 lines]
```

with the dispatcher reduced to:

```rust
fn handle_normal_mouse(app, event) {
    let region = detect::detect_click_region(app, x, y);
    let class = detect::classify_click(now, x, y, prev);
    if let Some(r) = region { apply::apply_click_region(app, r, class, x, y); }
}
```

**What this bought:**
- The pure `detect_click_region` can be tested by constructing an
  App, setting layout rects, and asserting click (x, y) → region.
  No TestBackend needed.
- `classify_click` is a pure function of time + coords + previous.
  Trivially unit-testable.
- `apply_click_region` has one match arm per region. Adding a new
  region adds a variant + a match arm, not editing a 700-line
  spaghetti.

**This is not refactoring for its own sake.** It is the audit
finding UI-041 concretised: "click-region detection cannot be
pure-function-tested in current form." The refactor makes the
finding go away.

The pattern — design-for-testability as an explicit refactor
target — generalises. Anywhere a function is "hard to test because
it does detection + mutation," splitting into detect/apply is a
candidate.

### 4.7 Subagent watchdog — event-size truncations and how to resume

The concrete runtime running the flog subagents had an event-size
watchdog: if a subagent emitted no visible output for ~15–20 min,
the turn was killed with "Truncated event message received." This
killed 3 task-attempts in Phase 2.5B before the user adapted.

**Symptoms:**
- Subagent goes silent, no tool calls for 10+ minutes.
- Eventually: runtime error, conversation ends, commits may be
  partially staged.

**Resolution:**
1. Run `git log --oneline` + `git status` to see what landed.
2. Often the subagent committed N–1 of N tasks and died on the
   last one. The plan can be resumed from that commit.
3. The repair commit is usually small — finish the last task
   manually or dispatch a narrower follow-up prompt.

**Prevention:**
- Cap subagent scope at ~6–10 commits per dispatch. flog learned
  this the hard way (Phase 2.5B Task 5 was split into 5a + 5b).
- Favour sequential over parallel when the scope is ambiguous.
  Parallel subagents collide on file edits; the "recovery" is
  often rewriting both branches anyway.
- Include explicit "commit after each task" in the plan. A
  subagent that commits at task boundaries has a recoverable
  failure surface; one that plans to "commit at the end" has a
  catastrophic failure surface.

## 5. What didn't work (must include)

An honest case study names its failures.

### 5.1 Worktree isolation was cosmetic

The initial design in Phase 0 (and Phase 2.5B) called for
subagents to run in isolated git worktrees
(`.claude/worktrees/<name>`), so parallel subagents wouldn't stomp
on each other's file edits. In practice, the runtime's `Agent(..., isolation: "worktree")`
flag created the worktrees but the subagents continued writing to
the main workspace. When the worktrees were inspected afterward,
they were empty; when the main workspace was inspected, all
subagent work was there.

**Lesson.** Worktree-based isolation requires a runtime that
*forces* the subagent to `cd` into the worktree and bounds its
filesystem writes. An annotation on the dispatch call isn't enough.
For flog, we fell back to sequential single-shot subagents, which
turned out to be nearly as fast (network + review was the real
bottleneck, not compile time).

### 5.2 Parallel subagents were conflict-prone

We attempted 4-way parallel subagents for Phase 1 audits (one per
scope: transport / domain / ui / flog_dart). That worked — audits
are read-only, no edits, no conflicts. We attempted 4-way parallel
for Phase 2 mechanical cleanup. It didn't work cleanly — two
subagents touched `src/main.rs` at the same time, clippy merges
were inconsistent, and the user ended up rebasing by hand. Time-
to-green for the parallel attempt was longer than time-to-green
for the sequential retry.

**Lesson.** Parallelise read-only work. Serialise write work,
except when each subagent has a strictly non-overlapping file set
*including build artifacts*.

### 5.3 Over-specification was worse than under-specification

Early Phase 3 step plans tried to prescribe per-line diffs ("move
function X to file Y, rename symbol Z, change line 44 from A to
B"). The subagent would dutifully execute and miss the shape of
the refactor. The plan author (the user + Claude driving the plan
session) didn't know the codebase at a per-line level; the
subagent did, but couldn't deviate from the plan. Net: a worse
result than either would have produced alone.

**Revised approach** (from Phase 3 Step 3.2 onward): plans name
audit ids in scope, name the target files and the exit gates, and
leave "how" to the subagent. The subagent can make local judgment
calls on extraction points while still being accountable to the
named ids and gates.

### 5.4 The 500-line limit produced over-splitting

Phase 4 Task 2 planned to split `src/app.rs` (1506 lines) into 5
files:

```
(plan excerpt)
- src/app/multi_app.rs
- src/app/mock_edit.rs
- src/app/sse_merge.rs
- src/app/layout_cache.rs
- src/app/mod.rs
```

The subagent shipped **11 files:**

```
(phase4 journal)
src/app/{mod, state_structs, network_state, scroll, input_fields,
         detail, mode, mock_edit, multi_app, sse_merge,
         layout_cache}.rs
```

The additional 6 were created because several intermediate files
overshot 500 lines themselves, and the subagent (correctly, by its
reading of the budget rule) split them further. The result is not
wrong — every file has a coherent theme, nothing is below 30 lines,
and the build is clean — but the cognitive shape of "App" is now
spread across more files than a reader needs.

**Lesson.** File-size budget is a signal, not a law. The plan
should name a target file *count*, not just a size bound. A
future iteration of this methodology would cap at "prefer ≤ 500,
accept up to 650 with a justification comment, split further only
if the subagent can name a distinct theme for each new file."

### 5.5 Subagents wrote tests based on imagined UI

Phase 2.5B Task 7 shipped 87 tests for the Logs view. 3 of them
asserted UI behaviour that didn't exist in the code — a "percent
readout" in the status bar and a "jump-to-bottom pill" visible at
narrow widths. The code had neither. The subagent had invented
them, plausibly, from context.

The resolution was not to "fix" the tests (there was no bug) but
to delete them. This is the risk of Rule 6 ("test observable
behaviour") applied by a subagent that also renders the imaginary
UI in its head: it will test what it imagines rather than what
exists. The mitigation is a read-the-code-first pass before test-
writing, but that pass is easier to specify than to enforce.

## 6. Cost accounting

A generalised estimate, calibrated against the flog campaign.

| Resource                 | flog actual | What you should plan for |
|--------------------------|-------------|--------------------------|
| Calendar time            | 3 days      | 3–7 days for 20–30 kLOC; scale linearly |
| Commits                  | 162         | expect 1.5–2× the raw-code-change count |
| Subagent rounds          | ~15–20      | expect 2–4 per phase step |
| User attention hours     | ~20–30      | ~10–15% of the elapsed wall-clock |
| Test-count change        | +1 948      | 5–10× the starting count if coverage is < 50% at entry |
| Doc pages produced       | ~15         | 5 engineering docs + ~10 journals |

**User intervention points** (where the user cannot be replaced):

1. **Phase 0 — scope decision.** Only the user knows what quality
   debt hurts them. An AI cannot generate a meaningful scope from
   a git log alone.
2. **Phase 1 — C-class adjudication.** Ambiguous findings require
   user interpretation. This is the one unavoidable "user reads
   the code" moment.
3. **Phase 3 mid-campaign — bug escalation.** UI-042 was caught by
   the user exercising the TUI, not by the test suite. Multi-day
   campaigns with a TUI or a UI in scope need user-run manual
   testing at least once per phase.
4. **Phase 3 — pace calls.** When a step is debating between two
   correct options ("should this constant be named
   `defaultCapacity` or `FLOG_STORE_CAPACITY`?"), the user picks
   and the campaign continues. Without a user in the loop these
   micro-decisions stall.
5. **Phase 6 — close.** Only the user decides when the campaign
   is done.

## 7. When this pattern applies

- **Existing codebase, mostly-working feature set.** The pattern
  assumes the code currently does something the user cares about.
  A campaign on greenfield code is just "writing the code." Use
  TDD directly.
- **Known quality debt.** The user can articulate at least 3–5
  concrete concerns ("file X is too long," "feature Y has no
  tests," "this module's API is confusing"). Without that, Phase 0
  has no anchor.
- **Owner has vocabulary to describe quality concerns.** "The
  domain layer should not depend on the UI layer" is a usable
  concern. "The code feels icky" is not.
- **Three-to-seven calendar days of owner attention available.**
  Shorter campaigns don't get through Phase 3 cleanly.
- **Tests can be automated.** `cargo test` / `pytest` / `go test` /
  `vitest run` — exit-code-driven, no TUI required. The entire
  regression fence depends on this.

## 8. When this pattern does not apply

- **Greenfield / no code yet.** Nothing to audit. Use spec →
  design → TDD instead.
- **Rapidly evolving spec.** If features are being added or
  removed week-over-week, the audit becomes stale before Phase 3
  executes. Freeze the spec first.
- **Absent user.** Subagents cannot resolve C-class findings on
  their own; they will invent an interpretation, which may be
  wrong. A campaign with a user who is "occasionally available" is
  slower, not faster — the wait for user input becomes the
  bottleneck.
- **Non-deterministic build.** If `cargo test` is flaky, the
  regression fence has false positives and the user loses trust in
  it. Fix the flake first, then run the campaign.
- **No CI, no clippy, no linter.** Phase 2 becomes "bootstrap
  linting," which is a separate campaign. Don't combine.

## 9. Prerequisites

Minimum skills the driving engineer needs:

- **Writing-plans skill** (see Superpowers' `writing-plans`
  meta-skill). The plan is the load-bearing artifact of every
  phase. A bad plan cannot be salvaged by a good subagent.
- **Subagent-driven-development literacy.** Knowing when to
  parallelise, when to serialise, how to fence with plan gates,
  how to recover from a truncation.
- **Characterization-test literacy.** Knowing the difference
  between a specification test ("X should do Y") and a
  characterization test ("X currently does Y, keep it that way").
- **Version-control hygiene.** Atomic commits, one-task-one-commit,
  conventional-commit-style messages, no amends of shared history.
- **Coverage tooling.** For Rust: `cargo llvm-cov`. For Python:
  `coverage.py`. For JS: `c8` / `nyc`. The per-module gate is
  enforceable only if you have the numbers.

## 10. Red flags during execution

Side-bar — anti-patterns to watch for while the campaign runs.

- **Subagent silent for > 5 minutes.** Either it's thinking
  hard, or the watchdog is about to fire. Don't wait longer than
  ~15 min; abort and resume from the last commit. The flog Phase 4
  Task 1 UI-003 migration first hit this at commit 30-of-239 sites
  (dispatch was subsequently narrowed to mechanical sed + batched
  commits).
- **`git status` shows uncommitted work at turn end.** The
  subagent intended to commit but didn't finish. Inspect the diff
  before accepting. If it's incomplete, back it out and re-dispatch
  with a narrower scope.
- **Clippy / lint errors introduced mid-refactor.** The subagent
  is not running `cargo clippy --all-targets -D warnings` before
  each commit. Fix the plan to require this gate and restart the
  step.
- **Test count drops.** The subagent deleted tests. Sometimes
  correct (imagined-UI tests from §5.5), sometimes not. Always
  inspect the diff: the ratio "tests removed / tests added"
  should be 0 in every phase except explicitly test-refactor
  phases.
- **Coverage drops.** The regression fence is leaking. Do not
  advance to the next phase until it recovers. This is the single
  most important numeric gate of the entire workflow.
- **Plan-file edits mid-execution.** The plan is the contract. If
  the subagent proposes editing the plan mid-run, stop and have
  the user review. Plan edits during execution should produce a
  new commit (`docs(plans): revise step X plan based on Y finding`),
  not a silent rewrite.
- **A B-class test flips green with no corresponding fix commit.**
  Someone fixed the bug accidentally. Good news, but the campaign
  lost traceability. File a correction commit that un-ignores the
  test and names the actual fix commit in its message.

## 11. Worked example — the flog-specific story

One full cycle, showing every artifact type:

```
spec
  specs/2026-04-22-project-cleanup-design.md
    (667 lines, §1–§7 — 6-phase roadmap, taxonomy, gates)

  ↓ drives

plan (per phase)
  plans/2026-04-22-phase1-audit.md
    (names 4 scope files, A/B/C/D/E taxonomy, C=0 gate)

  ↓ executed by

4 parallel read-only subagents
  each produces:
    audit/01-transport.md (15 findings)
    audit/02-domain.md   (25 findings)
    audit/03-ui.md       (42 findings)
    audit/04-flog-dart.md (33 findings)

  ↓ consolidated into

index
  audit/00-index.md — 115 findings, 27 A / 13 B / 0 C / 66 D / 9 E

  ↓ gate: user reviews B class, adjudicates C class

Phase 2 begins
  plans/2026-04-22-phase2-mechanical-cleanup.md
    (names all 9 E-class ids, clippy -D warnings as gate)
  subagent executes → 1 commit (rebased from 4 parallel attempts
  that collided on main.rs)

  ↓ gate: cargo test unchanged, clippy 0 warnings

Phase 2.5A, 2.5B, Phase 3 (×10 steps), Phase 4, Phase 5

  ↓ each phase produces

journal
  journal/phase-<N>.md — entry/exit HEAD, commits, audit ids closed

Phase 6 writes this methodology file + the flog retrospective +
a campaign-close journal.

  ↓

Campaign is closed — any further work is a new spec.
```

Given the paper trail, a reader opening `UI-042` in the audit
index can trace:
1. Audit addendum (2026-04-24).
2. Red lock commit (`95f97d7`, Phase 3 Step 3.6).
3. Fix commit (`133b631`, Phase 3 Step 3.8).
4. Journal entry (`journal/phase3-step8.md`, §"UI-042 resolution").
5. Retrospective mention (`retrospective-flog.md` §4 + §9).

Five hops, entirely mechanical, no conversational context needed.

## 12. How to start a campaign on Monday

If you want to run this pattern on a different codebase next week:

1. **Pick a scope bound.** 10–40 kLOC is the sweet spot. Smaller
   doesn't need the ceremony; larger needs more days than you
   probably have.

2. **Write the Phase 0 design** (1–2 hours).
   - 6-phase roadmap (copy from §2 above).
   - Audit taxonomy (copy from §3).
   - File budget (copy from §4.4; adjust for language).
   - Rules 1–11 (test density, observability, coverage gates).
   - Commit once as `docs: campaign plan + Phase 0 design`.

3. **Run Phase 1 with 3–4 read-only subagents** (0.5–1 day).
   Dispatch one per scope. Require the A/B/C/D/E label on every
   finding. Consolidate into an index. Review B class with
   yourself.

4. **Resolve all C-class findings** (0.5–1 day). This is the one
   unavoidable personal-attention step.

5. **Phase 2 mechanical cleanup** (1–2 hours). One subagent, clippy
   / lint / fmt as the gate.

6. **Phase 2.5 characterization** (1 day). This is the heaviest
   subagent phase. Dispatch per-module. Enforce Rule 2 (per-module
   coverage), Rule 9 (multi-scenario), Rule 10 (per-pub-fn
   density) as numeric gates.

7. **Phase 3 redesign** (1–3 days). Per step: 1 plan, 1 subagent
   dispatch, 1 journal. 5–15 steps depending on the codebase.

8. **Phase 4 residual** (0.5 day).

9. **Phase 5 docs** (1 day).

10. **Phase 6 retrospective** (0.5 day — mostly this template +
    numbers from your journals).

Expect slippage. Expect at least one truncation. Expect the user
to catch at least one bug the test suite missed. These are normal
— design the campaign so they are recoverable, not catastrophic.

## 13. Closing note

The single most important insight from the flog campaign is not
the six-phase model, not the A/B/C/D/E taxonomy, not the
red-lock pattern. It is this:

> **Write the plan. Run the subagent. Read the journal. Accept the
> commit. Then decide the next step.**

Every artifact in the workflow exists so the user-and-LLM loop can
be interrupted at any point and resumed from the last committed
artifact. The LLM's context loss is a liability; git + markdown
files are the remedy. Trust the paper trail, not the chat log.

A workflow that survives an LLM restart is a workflow that can run
for days.

---

*Companion: `retrospective-flog.md` in the same directory — the
flog-project-specific numbers, bugs, and commits.*
