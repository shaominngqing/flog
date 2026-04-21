# UI Layout Polish v2 — Design

**Date:** 2026-04-21
**Scope:** Iteration 2 on the 2026-04-20 UI polish, based on live testing of the shipped branch.
**Predecessor:** `2026-04-20-ui-layout-polish-design.md`

---

## 1. Problems found during smoke test

1. **Redundant `LIVE` badge + app context** in both tab bar (top-right) and status bar (bottom-left).
2. **Jump-to-Bottom pill** sits at 70% height and occludes reading area.
3. **Active device card's SURFACE1 fill** clashes with SAPPHIRE border — the fill extends beyond what the border encloses, looking broken.
4. **Network tab's top chrome** was never updated in v1 — still uses the old 3-row toolbar layout and inconsistent style.
5. **Tab labels too small**: `▤ Logs  ⇄ Network` underline treatment feels weightless; users want a strong active indicator.
6. **Small annoyances**: `Ttag...` looks like a single word, no `│` separator between `search` and `T`, Warning-row DEBUG pill contrast weak, tag column wrap/alignment wobbly.

## 2. Design Principles (carry-over)

- Information goes to exactly one surface, or is split with clear division of labor.
- No color-fill + border combos where fill could leak beyond border glyph cells.
- Floating chrome (pills/overlays) hugs viewport edges, not middle.
- Logs and Network chrome structures must be identical — same 3-level (operation row × 2 + column header) pattern.

---

## 3. Top chrome structure (applies to both Logs and Network)

**Row budget**: 6 rows above content (was 3 in v1).

```
Row 0 (tab bar)            ▤ Logs    ⇄ Network                      [Android]  AuraLang
Row 1 (separator)          ──────────────────────────────────────────────────────────────
Row 2 (op row 1)            /search...                                                 184/184
Row 3 (op row 2)            T  tag...      │   S V D I W E
Row 4 (separator)          ──────────────────────────────────────────────────────────────
Row 5 (column header)       TIME          LEVEL     TAG             MESSAGE
Row 6+ (content)           ...
```

### 3.1 Tab bar — Row 0 (1 row tall, not 2)

- **Active tab**: ` {icon} {label} ` rendered as a solid BLUE pill (fg=MANTLE, bg=BLUE, BOLD), 1-space left/right padding inside the pill.
- **Inactive tab**: plain `{icon} {label}` with OVERLAY0 fg, no bg, no bold.
- **No underline row** — the pill itself is the active indicator; underline row removed. Net −1 row vs v1.
- **Right side** (no LIVE, no underline):
  - Platform pill: `[iOS]` MANTLE-fg on BLUE-bg, `[Android]` on GREEN-bg, `[Sim]` on MAUVE-bg.
  - App name: SUBTEXT0 fg, no version, no device, no bold.
- Click regions: the whole active/inactive tab pill (including padding) is clickable. Right-side context is display-only.

### 3.2 Separator rule — Row 1 and Row 4

- 1-row full-width `─` in SURFACE0 on MANTLE bg. Same rule used twice: below the tab bar and below operation rows.

### 3.3 Operation rows — Rows 2 and 3

**Logs tab**:

- Row 2: ` /search...` (20 chars wide) ...right-aligned ` 184/184` counter in SUBTEXT0.
- Row 3: ` T  tag...   │   S V D I W E` — the `T` GREEN-pill followed by 2 spaces then `tag...` placeholder; `│` SURFACE1 separator; level buttons.

**Network tab**:

- Row 2: ` /filter url...` right-aligned ` 35/35`.
- Row 3: ` All HTTP SSE WS  │  All GET POST PUT DEL PATCH  │  All OK Fail Active Pending` — three pill groups separated by `│` SURFACE1.

Row 3 is the only structural difference between the two tabs; both are 1 row.

### 3.4 Column header — Row 5

Logs: ` TIME          LEVEL     TAG             MESSAGE`
Network: ` PROTO  METHOD  URL                                            STATUS  TIME  SIZE`

- fg = OVERLAY0, no bg (MANTLE), uppercase.
- Column widths match the runtime row columns:
  - Logs: cursor(1) + bookmark(2) + TIME(12) + LEVEL(9) + TAG(14) + MESSAGE(rest)
  - Network: existing widths — don't change.

---

## 4. Jump-to-Bottom overlay — hug the bottom

- Vertical: pill top-edge = `area.y + area.height - 4` (3-row pill + 1 padding from bottom).
- Horizontal: center (unchanged).
- **Background**: BASE (log list's bg). Not SURFACE0. This makes the pill look "transparent" with only the SAPPHIRE border.
- Border: SAPPHIRE rounded `╭╮╰╯` (unchanged).
- Content: `  ↓ Jump to bottom  ` in TEXT; optional `  N new  ` suffix in YELLOW.
- Visibility rule unchanged (`!auto_scroll`).
- If list height < 5 rows, don't render.

---

## 5. Device picker — ACTIVE card without fill

### 5.1 Normal app card
- Border: single-line `┌─┐` SURFACE0
- Background: MANTLE (same as device container — "transparent")
- Dot: `○` OVERLAY0
- Label: SUBTEXT0 regular

### 5.2 ACTIVE app card
- Border: double-line `╔═╗` SAPPHIRE
- Background: **MANTLE** (change from SURFACE1) — same as surroundings, no fill
- Dot: `●` GREEN
- Label: TEXT **bold**
- `[ACTIVE]` pill: GREEN bg + MANTLE fg + BOLD, inline in top edge
- Port: SAPPHIRE fg, right-aligned in top edge (inside the `═`s)

### 5.3 Selection cursor (from v1 Task 9) — keep
- `▎` SAPPHIRE U+258E in the gutter between device container's `│` and the app card's left border
- Shown when `is_selected && !is_active`

---

## 6. Network tab — redesign to match Logs structure

Network tab currently has a very different toolbar (see `src/ui/network/mod.rs` / `src/ui/network/filter.rs`). Rewrite to match §3 structure exactly:

- Tab bar handled by shared `ui::tab_bar::draw_tab_bar` — already shows right-side `[Platform] AppName`.
- Separator rule — reuse helper from `ui/logs/mod.rs` (move to shared `ui/mod.rs` if needed).
- Op row 1: search `/filter url...` on left, `35/35` count on right.
- Op row 2: three protocol/method/status pill groups with `│` separators.
- Separator rule.
- Column header: ` PROTO  METHOD  URL...  STATUS  TIME  SIZE` in OVERLAY0.

Pill colors:
- `All` selected = GREEN bg + MANTLE fg BOLD (the "neutral" highlight)
- Protocol: `HTTP` BLUE, `SSE` GREEN, `WS` PEACH
- Method: `GET` GREEN, `POST` BLUE, `PUT` PEACH, `DEL` RED, `PATCH` MAUVE
- Status: `OK` GREEN, `Fail` RED, `Active` YELLOW, `Pending` OVERLAY0
- Non-selected pill: color fg, MANTLE bg (outline only)

Row 3 total width ≈ 90 characters — fine for wide terminals (>100 cols). Narrow terminals truncate gracefully.

---

## 7. Bottom status bar (unchanged conceptually, one addition)

```
 ● LIVE   184/184   ⇅ AuraLang v1.0.0 · Xiaomi 23127PN0CC · :9753    Clear  Export  Stats  Help  Quit
```

- Prepend `⇅` U+21C5 (SAPPHIRE) before the app context string to signal "click to switch".
- The `⇅ {AppName v · Device · :Port}` cluster is the clickable hit region (extend `source_info_x` to include the `⇅ ` prefix).
- Right-side buttons unchanged from v1.

**Remove** the right-side LIVE badge from tab bar (§3.1) — status bar is now the sole LIVE indicator.

---

## 8. Polish items

### 8.1 `Ttag...` → `T  tag...`
In toolbar placeholder path of `draw_toolbar`: after the GREEN `T` pill, emit 2 spaces then `"tag..."` instead of concatenating. When tags are active, pills render as-is (no change).

### 8.2 `│` separator between search and tag
After the `safe_pad(&st, sw)` search span, before the 3-space gap, push `  │  ` SURFACE1-fg MANTLE-bg. Adjust `x` accordingly. Affects toolbar-op-row-2 (`T tag... │ S V D I W E`): already has `│`; we're adding one at `(search | tag)` boundary.

### 8.3 Tag column alignment bug
Investigate `draw_log_list` `safe_pad(&entry.tag, TAG_WIDTH)`. Confirm padding is applied. If `entry.tag` contains wide characters (unlikely), `safe_pad` uses display width — already correct. The wobbly alignment in the screenshots may be due to `MAX_WRAP_LINES` kicking in and the continuation row's `empty_prefix` diverging. Verify the `empty_prefix` vector's column widths match the header's exactly. Fix any drift.

### 8.4 Warning row DEBUG pill contrast
`level_badge` returns `(SUBTEXT0, SURFACE0, false)` for Debug. On a Warning row (bg=WARNING_ROW_BG=rgb(50,45,30) — muted dark yellow), SURFACE0 (rgb(54,58,79)) has insufficient contrast. Fix: when rendering the level pill, if the row_bg is `WARNING_ROW_BG` or `ERROR_ROW_BG`, override the pill's bg to MANTLE (darker). Alternatively simpler: always use MANTLE for non-Info/Warning/Error pill bg regardless of row_bg. Pick the simpler.

### 8.5 Column header line
Add a new renderer `draw_column_header_logs` / `draw_column_header_network`. Outputs single-row `Paragraph`. Column starts must match the data row layout constants. Extend `LayoutCache` with `col_header_y: u16`.

---

## 9. Row accounting

**Chrome**: Tab(1) + Sep(1) + Op1(1) + Op2(1) + Sep(1) + ColHeader(1) = 6 rows.
**Footer**: Status bar 1 row.
**Content area**: terminal_h - 7.

Compare v1: Tab(2) + Sep(1) + Toolbar(1) + Status(1) = 5 rows chrome; content = h-5.
So v2 chrome takes 2 more rows than v1 (6 vs 4 above content area, plus 1 below). With timeline already removed in v1 (3 rows freed), net v2 vs original master is still +1 row of content.

---

## 10. LayoutCache changes

Add:
- `pub col_header_y: u16` — Y of the column header row (Logs tab) — used only for mouse-hit guards, likely overlaps with existing list_y check.
- `pub net_col_header_y: u16` — Network equivalent.
- `pub tab_app_context_x: (u16, u16)` — (start, end) of right-side `[Platform] AppName` string — unused for now (display only), but registered for future hover.

Remove:
- Nothing — keep all existing fields.

---

## 11. Out of scope

- Mouse-hover tooltips.
- Customizable column widths.
- Persistent per-tab chrome state (always 6 rows).
- Theme switching.
- Reviewing the Device picker's 6-row app card layout (kept as-is from v1 Task 8).

---

## 12. Validation

After implementation:
- `cargo build --release` passes.
- `cargo test` passes (jump::tests unaffected).
- Smoke test on Android real device + iOS sim:
  - Tab pill shows BLUE bg only on active tab; no underline row.
  - Right side shows `[Platform] AppName` only; no LIVE there.
  - Row 2 and Row 3 operation rows render both on Logs and Network tabs in the 6-row chrome pattern.
  - Jump-to-Bottom pill appears 4 rows from bottom of list area with transparent (BASE) background.
  - Device picker ACTIVE card has no SURFACE1 fill; SAPPHIRE double-line border on MANTLE bg.
  - Status bar has `⇅` prefix before app context; clicking anywhere on `⇅ Appname ... :Port` opens device picker.
  - Warning row DEBUG pill is readable.
  - `T  tag...` has 2-space separation.
