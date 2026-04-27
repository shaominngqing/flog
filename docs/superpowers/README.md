# Superpowers — campaign artifacts index

This directory is the paper trail for the 2026-04-22 → 2026-04-24 flog
cleanup campaign. If you want to understand *why* the codebase looks
the way it does — why a particular abstraction was extracted, why
`event/` is a directory and not a single file, why a test is marked
`#[ignore = "bug: DART-001, fix in Phase 3"]` — this is the index.

## Directory layout

```
docs/superpowers/
├── README.md              # this file
├── specs/                 # design documents (Phase 0 output)
├── plans/                 # phase + step plans (pre-execution)
├── audit/                 # raw audit findings (Phase 1 output)
└── journal/               # per-phase exit notes
```

## The six-phase model

The campaign ran six phases; each phase produced exactly one commit
on `master` (the "phase commit") + however many intermediate commits
its subagents produced.

| Phase | Purpose                                          | Output                                                                   |
|-------|--------------------------------------------------|--------------------------------------------------------------------------|
| 0     | Brainstorm the campaign, pin scope               | `specs/2026-04-22-project-cleanup-design.md`                             |
| 1     | Audit (4 read-only subagents in parallel)        | `audit/01-*.md` through `audit/04-*.md` + `audit/00-index.md`            |
| 2     | Mechanical cleanup (clippy / dead code / fmt)    | Sibling-file pattern, clippy 0 warnings, Phase 2 journal                 |
| 2.5A  | Logic / render separation for testability        | `journal/phase-2.5a.md`                                                  |
| 2.5B  | Characterization test harness                    | `journal/phase-2.5b.md` — the regression fence                           |
| 3     | Redesign (per step, serial, test-guarded)        | `plans/2026-04-23-phase3-step*.md`, `journal/phase3-step*.md`            |
| 4     | Why-comments + residual splits                   | `plans/2026-04-24-phase4-comments.md`, `journal/phase4.md`               |
| 5     | Docs                                             | `plans/2026-04-24-phase5-docs.md`, `journal/phase5.md`                   |
| 6     | Retrospective + methodology case study (pending) | `retrospectives/` + `methodology/` (not yet populated)                   |

## The audit taxonomy

Every finding in `audit/*.md` carries one of five labels:

| Label | Meaning                                     | Handled by |
|-------|---------------------------------------------|------------|
| **A** | Correct-but-ugly behaviour                  | Phase 3 redesign, A-class test freezes behaviour. |
| **B** | Confirmed bug                               | Phase 2.5 red/ignored test; Phase 3 makes it green. |
| **C** | Ambiguous — feature or bug?                 | Resolved with user before Phase 2 entry (all C = 0). |
| **D** | Architecture smell                          | Phase 3 redesign with D-class characterization guard. |
| **E** | Mechanical 0-risk tidy-up                   | Phase 2 only. |

Every finding has a stable id (e.g. `TRANS-009`, `DOM-003`, `UI-041`,
`DART-023`) that source comments, CLAUDE.md, and the engineering docs
under `docs/` cite freely.

## Reading order for a new contributor

If you want to understand the codebase's current shape end-to-end:

1. `docs/ARCHITECTURE.md` — four-layer model + data flow.
2. `docs/MODULES.md` — per-module index. Use this as a "which file
   owns X?" lookup.
3. `docs/PROTOCOL.md` — wire format between flog and flog_dart.
4. `docs/CONTRIBUTING.md` — process rules (audit taxonomy, testing,
   commit format, file budget, flog_dart release flow).
5. `docs/superpowers/audit/00-index.md` — the 115-entry finding list
   with severity summary. Skim the B-class items to get a feel for
   the bugs Phase 3 fixed.
6. `docs/superpowers/journal/phase4.md` → `journal/phase3-step10.md` →
   `journal/phase3-step1.md` — reverse-chronological exit notes if
   you want the campaign story.

If you're only trying to make one change: the doc under `docs/` is
usually sufficient. Dive into `superpowers/` only when you want to
understand "why is the code shaped this way".

## How artifacts interconnect

```
spec (Phase 0)
  ↓
  defines 6-phase roadmap + audit taxonomy + file budget
  ↓
audit (Phase 1)  ←──── user approves C-class resolutions
  ↓
  produces stable finding ids (TRANS-*, DOM-*, UI-*, DART-*)
  ↓
plans (per phase / step)
  ↓
  each plan references its owning audit ids
  ↓
code commits
  ↓
  commits reference audit ids in their message bodies
  ↓
journal (per phase / step)
  ↓
  exit notes acknowledge which audit ids moved: ignored→green, etc.
  ↓
docs/ (Phase 5)
  ↓
  surface the final shape; cross-link to audit for "why"
```

This chain is why every non-obvious design choice in the current code
can be traced back to a specific audit entry in 2-3 hops.

## Conventions

- **Plans** are named `YYYY-MM-DD-<slug>.md`. They are appended-to,
  not edited, during execution — divergences go into the journal.
- **Specs** follow `YYYY-MM-DD-<slug>-design.md`.
- **Audits** live one per scope (`01-transport.md`, `02-domain.md`,
  `03-ui.md`, `04-flog-dart.md`) + a consolidated `00-index.md`.
  New findings discovered mid-campaign go into an "Addenda" section
  of the appropriate scope file (see the addenda for DOM-025,
  UI-041, UI-042, DART-033).
- **Journals** are named `phase<N>` or `phase<N>-step<M>`. They record
  the exit state: HEAD at start, HEAD at exit, test count delta,
  line-count delta, audit-id movement.

## Where to find Phase 6 (retrospective + methodology)

Not yet produced. When it lands it will add:

- `docs/superpowers/retrospectives/2026-04-22-flog-cleanup-retrospective.md`
  — flog-specific retrospective.
- `docs/superpowers/methodology/ai-long-workflow-case-study.md` —
  generalised case study of running a multi-phase refactoring campaign
  with AI subagents.

See `specs/2026-04-22-project-cleanup-design.md §7` for the intended
scope.
