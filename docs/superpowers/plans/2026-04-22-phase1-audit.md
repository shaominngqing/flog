# Phase 1 — Audit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Produce 4 structured Audit reports + 1 consolidated index covering the entire flog codebase, classifying every finding into A/B/C/D/E, so Phases 2–5 have an authoritative action list.

**Architecture:** Four read-only subagents (Transport / Domain / UI+event / flog_dart) run in parallel from isolated worktrees; each writes a markdown report to `docs/superpowers/audit/`. Main Claude then validates format, resolves C-class with the user, and merges a prioritized index. One docs-only commit closes Phase 1.

**Tech Stack:** Rust 1.x (flog binary), Dart 3.x (flog_dart), ratatui TUI, tokio async. This phase writes NO code — only markdown.

**Parent spec:** `docs/superpowers/specs/2026-04-22-project-cleanup-design.md` §3

---

## File Structure

Created this phase (all under `docs/superpowers/audit/`):

| File | Owner | Content |
|---|---|---|
| `01-transport.md` | Subagent 1 | Transport + Discovery findings |
| `02-domain.md` | Subagent 2 | Domain + Parser + session findings |
| `03-ui.md` | Subagent 3 | UI + app.rs + event.rs + cli.rs findings |
| `04-flog-dart.md` | Subagent 4 | flog_dart package findings |
| `00-index.md` | Main Claude | Prioritized B + C class consolidated index |

Also created: `docs/superpowers/journal/phase-1.md` (by main Claude at phase close).

Nothing in `src/` or `flog_dart/lib/` is modified in this phase.

---

## Pre-flight (Task 0)

### Task 0: Pre-flight checks

**Files:** (none modified — verification only)

- [ ] **Step 0.1: Confirm working tree is clean on master at the Phase 0 commit**

Run: `git log --oneline -1 && git status`
Expected:
```
f3b2a12 docs(superpowers): Phase 0 — project cleanup design + brainstorming journal
位于分支 master
...
未跟踪的文件:
  flog_062120.log
  flog_dart/test/
```
The two untracked items are expected — `flog_062120.log` is runtime output, `flog_dart/test/` is out of scope for Phase 1 reports (Subagent 4 may still read it but we do not add it to git here).

- [ ] **Step 0.2: Record baseline snapshot for Phase 6 retrospective**

Run:
```bash
mkdir -p docs/superpowers/audit
cat > docs/superpowers/audit/.baseline.md <<'EOF'
# Baseline snapshot — recorded at start of Phase 1

Date: 2026-04-22
Git HEAD: f3b2a12
cargo test: 217 unit + 1 integration, all green
cargo clippy -- -D warnings: FAILS (1 error PI approximation, 18 warnings)
Files > 800 lines (red): event.rs 1677, logs/mod.rs 1358, app.rs 1167, network/detail.rs 1109, source_select.rs 898
Files 500-800 lines (yellow): json_viewer/render.rs 745, network/mod.rs 700, structured_parser.rs 693, device_monitor.rs 654, main.rs 546, flog_dio.dart 504
Known dead code (clippy): MockRuleStore::enabled_count, LogStore::clear, adb::is_available, UsbDevice, list_devices
EOF
```

- [ ] **Step 0.3: Commit baseline snapshot**

```bash
git add docs/superpowers/audit/.baseline.md
git commit -m "$(cat <<'EOF'
chore(audit): record Phase 1 baseline snapshot

Frozen metrics (test count, file sizes, known dead code) so Phase 6
retrospective can compare before vs after.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 1: Dispatch 4 audit subagents in parallel

**Files:**
- Create: `docs/superpowers/audit/01-transport.md`
- Create: `docs/superpowers/audit/02-domain.md`
- Create: `docs/superpowers/audit/03-ui.md`
- Create: `docs/superpowers/audit/04-flog-dart.md`

All four agents run **read-only** — they must not edit code, must not run `cargo test`, must not modify any file outside `docs/superpowers/audit/`. Each writes exactly one report file.

- [ ] **Step 1.1: Dispatch all 4 subagents in one message with parallel Agent tool calls**

Four independent agents, no shared state, so dispatch in a single message with 4 `Agent` tool invocations. Use:
- Subagent 1: `subagent_type: "Explore"`, thoroughness: "very thorough"
- Subagent 2: `subagent_type: "Explore"`, thoroughness: "very thorough"
- Subagent 3: `subagent_type: "Explore"`, thoroughness: "very thorough"
- Subagent 4: `subagent_type: "general-purpose"` (needs to read Dart + may run `flutter analyze` / `dart analyze` read-only to cross-check)

Every subagent prompt MUST include (verbatim boilerplate):

```
You are conducting a read-only code audit of the flog project, Phase 1.
You MUST NOT edit any source code, run tests, or modify any file outside
docs/superpowers/audit/. You write exactly ONE markdown report.

Parent spec: docs/superpowers/specs/2026-04-22-project-cleanup-design.md
Focus: spec §3 (Phase 1 — Audit).

CLASSIFY EVERY FINDING into one of five labels:
  A. Correct-but-ugly  — behavior correct, code ugly. Goes to Phase 3 redesign.
  B. Confirmed bug     — behavior wrong (crash, wrong data, surprising UX). Goes to Phase 2.5 red test + Phase 3 fix.
  C. Ambiguous         — unclear if feature or bug. Main Claude will ask the user to adjudicate.
  D. Architecture smell — missing abstraction, responsibility misplaced, patch-style code, awkward state machine, magic values whose concept is not extracted. Goes to Phase 3 redesign.
  E. Mechanical        — truly zero-risk mechanical fix (clippy equivalent rewrite, dead code, typo). Goes to Phase 2.

REPORT FORMAT (YAML entries):

```yaml
id: <SCOPE>-<N>        # e.g. TRANS-007
label: A | B | C | D | E
location: <file>:<line-range>
title: <one-line summary>
evidence: |
  <3-10 lines of code quote + observed behavior>
proposed_action: |
  <A/D: new design sketch>
  <B: expected behavior>
  <C: question for user>
  <E: concrete fix>
risk: low | medium | high
```

FORBIDDEN WORDS in the report: "TODO", "待讨论", "maybe", "可能", "也许",
"probably". If something is unclear, classify it C (ambiguous) with a concrete
question — do NOT leave tentative language.

END OF REPORT: append a summary table counting A/B/C/D/E.

Also: every pub/public symbol you touch in evidence that you think should
NOT be public — flag it as a D finding. Every magic number/string whose
meaning is not obvious at the call site — flag it as D (with proposed name).

Read project memory at /Users/shaomingqing/.claude/projects/-Users-shaomingqing-FlutterProject-flog/memory/
for prior feedback and context before auditing.

DO NOT produce your report until you have:
1. Read every file in your scope
2. Read the parent spec §3 fully
3. Read project memory
```

**Subagent-specific prompts** (append to boilerplate above):

Subagent 1 (Transport & Discovery):
```
Your scope:
- src/transport/ (entire directory)
- src/input/connector.rs
- src/input/protocol.rs
- src/main.rs (only parts about connection lifecycle / server startup)
- src/replay.rs

Focus areas:
- Concurrency & lifecycle: ghost device detection, Hello timeout, ADB port
  cycling (recently patched — regression risk high)
- Cross-platform symmetry: Localhost / AdbForward / Usbmuxd — are the three
  transport paths responsibility-symmetric, or does one leak abstraction?
- Protocol coupling: ClientMessage / ServerMessage — hidden coupling between
  transport layer and protocol layer?
- ConnectorEvent error branches — any unhandled disconnection / malformed
  message path?
- Recent commits db32426, b767269 — are the patches surface fixes or is
  there an underlying architecture issue? Classify accordingly.

ID prefix: TRANS-
Output: docs/superpowers/audit/01-transport.md
```

Subagent 2 (Domain + Parser):
```
Your scope:
- src/domain/ (entire directory)
- src/parser/ (entire directory)
- src/session.rs

Focus areas:
- LogStore ring buffer edges: 100K cap, 10% drain, consecutive-duplicate folding
- FilterState: regex pre-compilation, pipe-separated OR degenerate behavior
- NetworkFilter: ProtocolFilter + MethodFilter + StatusFilter — can these
  three enum sets be unified, or do they justify separate types?
- network_store FlogNetMessage state transitions
- Mock / SSE merge / WS chat — are there repeated patterns begging for a
  shared abstraction?
- Parser chain (structured → generic → keyword → network): is the
  fall-through responsibility split clear, or are there overlaps/gaps?
- filter.rs (420 lines) and structured_parser.rs (693 lines) — split points?

ID prefix: DOM-
Output: docs/superpowers/audit/02-domain.md
```

Subagent 3 (UI + event + app + cli):
```
Your scope (the largest — expect the most findings):
- src/ui/ (entire directory)
- src/app.rs (1167 lines)
- src/event.rs (1677 lines)
- src/cli.rs

Focus areas:
- event.rs state machine + key routing: hidden dead paths? Keys that don't
  match the intended mode?
- app.rs: which fields are genuinely top-level vs UI-tab-local state that
  got misplaced at the top?
- AppMode::InputActive(InputField) — is this a patch-shaped state machine
  merger (commit 63f783a)? Does the merged form carry all responsibilities
  cleanly, or should each input field own its own mode?
- logs/mod.rs (1358) / network/detail.rs (1109) / source_select.rs (898):
  identify seams for splitting. source_select.rs in particular may not
  deserve to exist as a single unit.
- Scroll model: Logs vs Network — are move_up/down, select_up/down,
  auto_scroll truly unified across both tabs, or parallel implementations
  that drifted?
- json_viewer/ as a shared component: does anything leak beyond its scope?
- Magic values: colors, layout constants, timing constants. Flag as D.
- CRITICAL for Phase 2.5: is UI logic separable from rendering? For each
  large UI file, state which pure-function extractions are feasible and
  which areas need TestBackend snapshot fallback.

ID prefix: UI-
Output: docs/superpowers/audit/03-ui.md
```

Subagent 4 (flog_dart):
```
Your scope:
- flog_dart/lib/ (entire directory)
- flog_dart/test/ (read-only — it is currently untracked)

Focus areas:
- FlogDio auto-insertion of FlogMockInterceptor + FlogHttpInterceptor:
  is the ordering guarantee robust? (project memory records a past incident)
- FlogMockInterceptor matching logic: URL pattern edge cases, method filter
  interactions, how it behaves with rules that conflict
- FlogHttpInterceptor response-modification timing: does it correctly run
  before business interceptors that mutate responses?
- FlogSseParser / FlogWebSocket wrappers: behavior under binary payloads,
  error streams, mid-stream disconnects
- flogEnabled compile-time constant: is tree-shaking truly zero-overhead
  in release builds? Any code path where flogEnabled=false still leaves
  runtime cost?
- VM service extension ext.flog.syncMockRules: error paths, version skew
- Public API surface: anything unnecessarily pub? Anything missing docs?
- flog_dio.dart is 504 lines (yellow zone) — split or justify?

ID prefix: DART-
Output: docs/superpowers/audit/04-flog-dart.md
```

- [ ] **Step 1.2: Wait for all 4 subagents to complete**

Each subagent returns a single summary message. Check that each claims its
report file was written.

- [ ] **Step 1.3: Verify all 4 report files exist**

Run: `ls -la docs/superpowers/audit/`
Expected: `01-transport.md`, `02-domain.md`, `03-ui.md`, `04-flog-dart.md`, `.baseline.md` all present.

---

## Task 2: Validate report format compliance

**Files:** (read-only validation)

- [ ] **Step 2.1: Forbidden-words scan**

Run:
```bash
grep -nE "TODO|待讨论|maybe|也许|probably|可能是|暂定" docs/superpowers/audit/0[1-4]-*.md || echo "PASS: no forbidden words"
```
Expected: `PASS: no forbidden words`

If any match appears:
- If the matched word is inside a `proposed_action` of a C-class entry quoting the user's future question, that's allowed — whitelist by hand.
- If it's tentative language in A/B/D/E entries, the subagent failed. Dispatch a fresh subagent for that scope with `subagent_type: "general-purpose"` and prompt:
  ```
  Report docs/superpowers/audit/<file>.md contains tentative language:
  <paste grep matches>.
  Rewrite those entries to either (a) concrete classification in A/B/D/E
  with definite claims, or (b) move them to label C with a concrete
  user-facing question. Do not introduce new findings.
  ```

- [ ] **Step 2.2: Required-field scan**

Every YAML entry must contain: `id`, `label`, `location`, `title`, `evidence`, `proposed_action`, `risk`. Run:
```bash
for f in docs/superpowers/audit/0[1-4]-*.md; do
  echo "=== $f ==="
  grep -c "^id:" "$f"
  grep -c "^label:" "$f"
  grep -c "^location:" "$f"
  grep -c "^title:" "$f"
  grep -c "^risk:" "$f"
done
```
Expected: per file, all 5 counts are equal (one of each field per entry).

If mismatched, dispatch a fix subagent for that file only.

- [ ] **Step 2.3: Label-validity scan**

Run:
```bash
grep "^label:" docs/superpowers/audit/0[1-4]-*.md | grep -vE "label: [ABCDE]$" || echo "PASS: all labels valid"
```
Expected: `PASS: all labels valid`

- [ ] **Step 2.4: Summary-table presence**

Each report must end with a summary table counting A/B/C/D/E. Run:
```bash
for f in docs/superpowers/audit/0[1-4]-*.md; do
  echo "=== $f ==="
  tail -20 "$f" | grep -E "^\|.*[ABCDE].*\|" | head -10
done
```
Expected: each file shows a summary table with rows for each of A/B/C/D/E.

---

## Task 3: Resolve C-class findings with user

**Files:** (modifies `0[1-4]-*.md` in place to reclassify C entries)

- [ ] **Step 3.1: Extract all C-class findings**

Run:
```bash
python3 - <<'PY'
import re, os, glob
for f in sorted(glob.glob("docs/superpowers/audit/0[1-4]-*.md")):
    with open(f) as fh:
        content = fh.read()
    # crude YAML-block splitter on ```yaml fences
    blocks = re.findall(r"```yaml\s*\n(.*?)\n```", content, re.DOTALL)
    for b in blocks:
        if re.search(r"^label:\s*C\s*$", b, re.MULTILINE):
            idm = re.search(r"^id:\s*(\S+)", b, re.MULTILINE)
            tm  = re.search(r"^title:\s*(.+)$", b, re.MULTILINE)
            print(f"[{os.path.basename(f)}] {idm.group(1) if idm else '?'} — {tm.group(1) if tm else '?'}")
PY
```

Collect output into a list. If empty → skip to Task 4.

- [ ] **Step 3.2: Present C entries to user via AskUserQuestion**

For each C entry (or in batches of up to 4), present to user using `AskUserQuestion`. The question shape:

```
Audit condensed question for <id>:
  <title>
  Evidence: <brief>
  Claude's reading: <1-line interpretation>
  How should this be classified?

Options:
  A (correct but ugly)
  B (bug — describe expected behavior)
  D (architecture smell)
  E (zero-risk mechanical)
```

Record each user decision.

- [ ] **Step 3.3: Apply reclassifications in place**

For each C entry the user decided on, use `Edit` to change `label: C` → the chosen label in the exact report file. If the user chose B, also update `proposed_action` to record the expected behavior they described.

- [ ] **Step 3.4: Re-verify no C labels remain**

Run:
```bash
grep "^label: C$" docs/superpowers/audit/0[1-4]-*.md || echo "PASS: all C-class resolved"
```
Expected: `PASS: all C-class resolved`.

---

## Task 4: Build consolidated index

**Files:**
- Create: `docs/superpowers/audit/00-index.md`

- [ ] **Step 4.1: Extract B-class findings across all reports**

Run (output will be used to build the index):
```bash
python3 - <<'PY' > /tmp/audit-b-list.txt
import re, glob, os
out = []
for f in sorted(glob.glob("docs/superpowers/audit/0[1-4]-*.md")):
    with open(f) as fh: content = fh.read()
    for b in re.findall(r"```yaml\s*\n(.*?)\n```", content, re.DOTALL):
        if re.search(r"^label:\s*B\s*$", b, re.MULTILINE):
            idm = re.search(r"^id:\s*(\S+)", b, re.MULTILINE)
            tm  = re.search(r"^title:\s*(.+)$", b, re.MULTILINE)
            rm  = re.search(r"^risk:\s*(\S+)", b, re.MULTILINE)
            lm  = re.search(r"^location:\s*(.+)$", b, re.MULTILINE)
            out.append((rm.group(1) if rm else "?", idm.group(1) if idm else "?",
                        tm.group(1) if tm else "?", lm.group(1) if lm else "?",
                        os.path.basename(f)))
# sort: high -> medium -> low
order = {"high": 0, "medium": 1, "low": 2}
out.sort(key=lambda r: order.get(r[0], 9))
for r in out:
    print(f"- **[{r[0].upper()}]** `{r[1]}` — {r[2]}  \n    location: `{r[3]}`  (from {r[4]})")
PY
cat /tmp/audit-b-list.txt
```

- [ ] **Step 4.2: Write 00-index.md**

Create `docs/superpowers/audit/00-index.md` using the `Write` tool with this skeleton (fill the B-list section from step 4.1 output):

```markdown
# Audit consolidated index — 2026-04-22

Phase 1 of the flog cleanup. Summarizes the user-actionable findings from
the four audit reports so the user can quickly adjudicate priority and
gate entry to Phase 2.

## Summary counts

| Scope | A | B | C | D | E |
|---|---|---|---|---|---|
| 01-transport | <n> | <n> | 0 | <n> | <n> |
| 02-domain    | <n> | <n> | 0 | <n> | <n> |
| 03-ui        | <n> | <n> | 0 | <n> | <n> |
| 04-flog-dart | <n> | <n> | 0 | <n> | <n> |
| **Total**    | <n> | <n> | **0** | <n> | <n> |

(C count MUST be 0 at this point — Task 3 resolved all C entries.)

## B-class findings — prioritized (bugs to fix in Phase 3)

<paste step 4.1 output here, high → medium → low>

## Phase 3 redesign scope — D-class by module

Grouping D findings by target module so Phase 3 step planning can map
each step to its source entries.

### Parser layer
- <DOM-xxx entries that touch src/parser/>

### Domain layer
- <DOM-xxx entries that touch src/domain/>

### Transport layer
- <TRANS-xxx entries>

### flog_dart
- <DART-xxx entries>

### App state machine (app.rs + AppMode)
- <UI-xxx entries touching app.rs / AppMode>

### Event dispatch (event.rs)
- <UI-xxx entries touching event.rs>

### UI Logs view
- <UI-xxx entries in src/ui/logs/>

### UI Network view
- <UI-xxx entries in src/ui/network/>

### UI shared components
- <UI-xxx entries in json_viewer/input_field/text_editor/source_select>

## Notes for Phase 2 (Mechanical)

E-class total: <n>. Subagents dispatched in Phase 2 will filter by report
and work their scope in parallel worktrees.

## Notes for Phase 2.5 (Characterization)

Of the D entries above, the following flag "cannot be pure-function-tested
in current form" and REQUIRE redesign before testing:
- <any UI entries with that caveat>

## Gate check for Phase 2 entry

- [ ] Every C-class resolved into A/B/D/E (Task 3 done)
- [ ] B list reviewed by user
- [ ] User confirms Phase 2 may begin
```

- [ ] **Step 4.3: User reviews the index**

Send the user a short message:
```
Phase 1 audit complete. Index at docs/superpowers/audit/00-index.md.
Read that one file. Confirm or flag any B-class entry that should be
re-classified, or any D you think is actually an A (or vice versa).
When satisfied, say "Phase 2 approved".
```

- [ ] **Step 4.4: Apply any user corrections**

If the user asks to re-classify anything, use `Edit` to fix the entry in the underlying `0N-*.md` report AND update the index. Re-run the verification commands from Task 2 (format) and Task 3 (no C remaining).

---

## Task 5: Write Phase 1 journal + commit

**Files:**
- Create: `docs/superpowers/journal/phase-1.md`

- [ ] **Step 5.1: Write phase-1.md**

Use `Write` to create `docs/superpowers/journal/phase-1.md`:

```markdown
# Phase 1 Journal — Audit

## 入口
- 时间：<fill in when Task 0 ran>
- 状态：Phase 0 (spec + brainstorming journal) commit f3b2a12 on master
- 执行者：主 Claude + 4 audit subagents

## 时间线
- <HH:MM> Task 0: baseline snapshot recorded, committed
- <HH:MM> Task 1: 4 subagents dispatched in parallel
- <HH:MM> Task 1: subagent 1 (transport) returned, <n> findings
- <HH:MM> Task 1: subagent 2 (domain)    returned, <n> findings
- <HH:MM> Task 1: subagent 3 (ui)        returned, <n> findings
- <HH:MM> Task 1: subagent 4 (flog_dart) returned, <n> findings
- <HH:MM> Task 2: format validation — <pass count> failures, <n> fixes dispatched
- <HH:MM> Task 3: <n> C-class entries extracted, user adjudicated
- <HH:MM> Task 4: index assembled, user reviewed
- <HH:MM> Task 5: this journal + commit

## 意外发现
<list any finding that was genuinely surprising — not everything, just what
stood out. Used by Phase 6 methodology>

## 出口
- 时间：<fill in>
- 状态：Phase 1 验收门槛全部达成
  - [x] 4 份 audit 报告齐全，格式合规
  - [x] 报告里无禁止词
  - [x] 所有 C 类条目用户已裁决完
  - [x] 00-index.md 合并完成
- 移交 Phase 2 的事项：
  - E-class count per scope (dispatched to 4 subagents in Phase 2)
  - Any UI entry flagged as "cannot be pure-function-tested" — Phase 2.5 UI
    subagent must take a TestBackend-snapshot fallback for those locations
```

- [ ] **Step 5.2: Final pre-commit verification**

Run:
```bash
ls docs/superpowers/audit/
```
Expected: `00-index.md  01-transport.md  02-domain.md  03-ui.md  04-flog-dart.md  .baseline.md`

```bash
ls docs/superpowers/journal/
```
Expected: `phase-0-brainstorming.md  phase-1.md`

```bash
git status docs/superpowers/
```
Expected: new files under `audit/` (index + 4 reports) and `journal/phase-1.md` untracked.

```bash
grep "^label: C$" docs/superpowers/audit/0[1-4]-*.md || echo "PASS"
```
Expected: `PASS`.

- [ ] **Step 5.3: Commit Phase 1**

```bash
git add docs/superpowers/audit/ docs/superpowers/journal/phase-1.md
git commit -m "$(cat <<'EOF'
chore(audit): Phase 1 — audit reports (4 subagents, A/B/C/D/E classified)

Four read-only subagents audited transport / domain / UI / flog_dart
in parallel and produced structured findings. All C-class entries
resolved with user. 00-index.md consolidates B-class bugs and groups
D-class smells by target module for Phase 3 step planning.

Spec: docs/superpowers/specs/2026-04-22-project-cleanup-design.md §3

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
git log --oneline -3
```

Expected: new commit appears on top of `f3b2a12`.

---

## Task 6: Hand off to Phase 2

- [ ] **Step 6.1: Send user a phase-exit message**

```
Phase 1 complete. Ready for Phase 2 (Mechanical cleanup).

Next action: when you say "go Phase 2", I'll invoke writing-plans to
produce the Phase 2 implementation plan, based on the E-class findings
from the audit.

Reminder from spec §11 work cadence: each phase entry is a user
check-in — you can reorder, skip, or adjust phase goals at any
phase boundary.
```

- [ ] **Step 6.2: Stop**

Do NOT auto-dispatch Phase 2. The spec's work cadence requires a user
check-in at each phase boundary. Main Claude waits for explicit approval
to begin Phase 2.

---

## Phase 1 acceptance checklist (spec §3.7)

- [ ] 4 audit reports complete and format-compliant
- [ ] No forbidden words ("TODO"/"待讨论"/tentative language)
- [ ] All C-class resolved into A/B/D/E
- [ ] `00-index.md` merged
- [ ] 1 docs-only commit on master
- [ ] `docs/superpowers/journal/phase-1.md` written

---

## Risks & mitigations

| Risk | Mitigation |
|---|---|
| Subagent produces a short/shallow report | Task 2 format scan catches missing fields; fresh subagent re-dispatched for that scope |
| Subagent introduces tentative language | Task 2.1 forbidden-word scan; fresh subagent for targeted rewrite |
| User disagrees with many A/D classifications | Step 4.4 applies corrections in place; re-run Task 2 validators after edits |
| Subagent accidentally edits code | All subagents prompted read-only; verify with `git diff --stat` before commit — no `src/` or `flog_dart/lib/` changes permitted |
| Phase 1 takes longer than estimated | Parallel dispatch of 4 subagents is the speed lever; sequential fallback if one subagent blocks |

---

## Downstream dependencies

Phase 2, 2.5, 3 planning all read from `docs/superpowers/audit/00-index.md`.
Do NOT start Phase 2 planning until this plan's Task 5 commit is on master.
