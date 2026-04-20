# UI Layout Polish — Design

**Date:** 2026-04-20
**Scope:** flog TUI — top chrome, device picker, bottom status bar, empty states
**Goal:** Tighten visual hierarchy, remove dead real estate (timeline), add Jump-to-Bottom floating pill, and fix the device picker's broken containment.

---

## 1. Background

Current pain points (from screenshots):

1. **Device picker modal**: app cards visually "escape" the device header (child wider than parent); 3-field rows (Package/Platform/Mode) not column-aligned; ACTIVE tag collides with `Port: 9753`; spacing between devices feels arbitrary.
2. **Top chrome**: tab bar + toolbar + list column header are split across 4 rows with no clear hierarchy; `flog` logo pill takes toolbar space without adding info.
3. **Timeline**: the 3-row heatmap under the log list shows color variation but not height variation — users don't use it to locate events. Dead real estate.
4. **Bottom status bar**: `LIVE` pill is isolated on the left; right-side buttons use four different background colors (orange/blue/red/gray) with no tonal coherence; source info buried between them.
5. **Empty states**: "Waiting for connection" / "Waiting for logs" show the logo but leave users guessing about what to do next or which app is connected.

## 2. Design Principles

- **Hierarchy by row, not by boxes**: dedicate Row 0 to navigation (Tab + context), Row 1 to separator, Row 2 to operations (search/filter/level). Avoid nesting borders where a horizontal line does the job.
- **Reuse existing palette consistently**: no new colors. Keep Catppuccin Macchiato (BLUE / SAPPHIRE / GREEN / SURFACE0/1 / OVERLAY0 / MANTLE).
- **Fewer ornaments, more alignment**: column-align labels, drop the multi-color button bar in favor of uniform MANTLE-bg pills.
- **Overlay over layout shift**: Jump-to-Bottom is a floating overlay, not a reserved row. Never move log content to make room for chrome.

---

## 3. Top Chrome — 2 rows + 1 separator

### 3.1 Layout

```
Row 0 (tab bar + context)   ▤ Logs  ⇄ Network                AuraLang v1.0.0 · iPhone 17  ● LIVE
Row 1 (separator)           ──────────────────────────────────────────────────────────────────
Row 2 (toolbar)              /search...    T tags       │  S V D I W E  │         15/15
Row 3..n (log list)         ...
Row n (status bar)          ● LIVE   15/15   AuraLang v1.0.0 · iPhone 17 · :9753   Clear  Export  Stats  Help  Quit
```

### 3.2 Changes vs current

- **Row 0**: right-side appends `AppName vX.Y · DeviceName  ● LIVE`. Current bottom status bar is the only place to see which app is active; surface it at the top where context matters most.
- **Row 1**: one-line SURFACE0 rule. Replaces the current tab underline-indicator row; underline is promoted to the Row 0 tab label itself.
- **Row 2 toolbar**:
  - Remove the ` flog ` BLUE logo pill (tab bar already establishes identity).
  - Insert two SURFACE1 `│` separators to group search | levels | counts.
  - `15/15` filtered/total count moves into the toolbar (was in the bottom status bar).
- **Tab bar**: keep the two-line tab-label + underline pattern; underline color becomes BLUE on active, nothing on inactive. Hover/click regions unchanged.

### 3.3 Row-count accounting

Current top chrome: 2 (tab) + 1 (toolbar) + list column header embedded in toolbar = 3 rows.
New top chrome: 1 (tab+context) + 1 (separator) + 1 (toolbar) = 3 rows. **Neutral.**

---

## 4. Remove Timeline — +3 rows for the log list

Delete the `timeline::draw_timeline` block entirely. Update `draw_logs` vertical layout:

```rust
.constraints([
    Constraint::Length(1), // tab bar (was toolbar)
    Constraint::Length(1), // separator rule
    Constraint::Length(1), // toolbar
    Constraint::Min(3),    // main area (list + optional detail)
    Constraint::Length(1), // status bar
])
```

Remove `timeline_y` from `LayoutCache` and drop `ui/logs/timeline.rs` (or keep the file but stop calling it, TBD during implementation).

**Rationale**: user has never used the heatmap to locate events; 3 additional rows of log content is more valuable than any alternative feature.

---

## 5. Jump-to-Bottom Floating Pill

### 5.1 Behavior

- **Visible when**: `app.auto_scroll == false` (user has scrolled away from the tail).
- **Hidden when**: `auto_scroll == true` (already at bottom).
- **Content**:
  - No new logs: `↓ Jump to bottom`
  - New logs since pause (`app.new_logs_since_pause > 0`): `↓ Jump to bottom  N new`
- **Interaction**: click / `G` / `End` / `Ctrl+End` → `app.go_bottom()` + re-enable `auto_scroll` + clear `new_logs_since_pause`.

### 5.2 Rendering

Floating overlay (NOT a reserved row). Rendered after the log list, at horizontal center of the log viewport, vertical position ~70% of viewport height. 3 rows tall (top border / content / bottom border), pill width = content width + 4 padding.

```
              ╭───────────────────────────╮
              │  ↓ Jump to bottom  42 new │
              ╰───────────────────────────╯
```

- Border: SAPPHIRE rounded
- Bg: SURFACE0
- Fg: TEXT for label, YELLOW for `N new` count
- Occludes underlying log content for its 3 rows — acceptable since the user isn't reading the tail while paused.

### 5.3 Click region

Register in `LayoutCache` as `jump_to_bottom_rect: Option<(x, y, w, h)>`. Event handler maps clicks inside the rect to `app.go_bottom()`.

---

## 6. Device Picker — Containment Fix + Selection Affordance

### 6.1 Layout

```
╭─ Devices (2) ──────────────────── ↑↓ navigate  ⏎ connect  esc cancel ─╮
│                                                                       │
│  ┌─ [iOS] iPhone 17 (iPhone) ─────────── USB · 00008150...401C ──┐   │
│  │                                                                │   │
│  │  ╔═ ● AuraLang v1.0.0  [ACTIVE] ═══════════════ Port: 9753 ══╗ │   │
│  │  ║    Package   com.yuanfudao.aura                           ║ │   │
│  │  ║    Platform  ios                                          ║ │   │
│  │  ║    Mode      debug                                        ║ │   │
│  │  ╚═══════════════════════════════════════════════════════════╝ │   │
│  │                                                                │   │
│  │  ┌─ ○ Shopping v2.0.0 ──────────────────────── Port: 9754 ──┐ │   │
│  │  │    Package   com.shop.app                                │ │   │
│  │  │    Platform  ios                                         │ │   │
│  │  │    Mode      debug                                       │ │   │
│  │  └──────────────────────────────────────────────────────────┘ │   │
│  │                                                                │   │
│  └────────────────────────────────────────────────────────────────┘   │
│                                                                       │
│  ┌─ [Sim] macOS ─────────────────────────────────── localhost ───┐   │
│  │                                                                │   │
│  │    ○ Waiting for app...                                        │   │
│  │                                                                │   │
│  └────────────────────────────────────────────────────────────────┘   │
│                                                                       │
╰───────────────────────────────────────────────────────────────────────╯
```

### 6.2 Structural changes

- **Device becomes a container**: single-line-rounded border (`┌─...─┐`) with title embedded in the top edge: `┌─ [Pill] DeviceName ─── Conn · ID ──┐`. App cards are now nested inside with 2-space indent — they can never visually escape the device.
- **Title bar of modal**: left `Devices (N)`, right inline hotkey hints (` ↑↓ navigate  ⏎ connect  esc cancel `). Replaces the separate hint row previously rendered below.
- **Separator between devices**: one blank line (MANTLE). No more `─` rule; device borders provide the grouping.
- **Detail rows column-aligned**: `Package ` `Platform` `Mode    ` all padded to a common label width (10 chars). Drop the colon.
- **ACTIVE tag**: becomes `[ACTIVE]` bracketed pill in the active app card's top-edge title (inside the double-line border). Port stays right-aligned in the same line.
- **Status dot alignment**: `●` GREEN for ACTIVE, `○` OVERLAY0 for others (app cards or "Waiting for app…").

### 6.3 Two-state selection

| State | Border | Border color | Dot | Tag | Bg | Fg |
|---|---|---|---|---|---|---|
| **ACTIVE** (currently viewing) | Double-line `╔═╗` | SAPPHIRE | `●` GREEN | `[ACTIVE]` GREEN | SURFACE1 | TEXT bold |
| **Normal** (connected but not viewed) | Single-line `┌─┐` | SURFACE0 | `○` OVERLAY0 | none | MANTLE | SUBTEXT0 |

**Keyboard navigation**: `↑↓` moves a transient highlight (left-edge `▎` SAPPHIRE cursor bar on the hovered card — no border change during preview). `⏎` commits the selection, the newly-selected card's border flips to the double-line ACTIVE treatment, and the previously ACTIVE card reverts to Normal.

Single-app case: no navigation preview — only ACTIVE vs Normal.

### 6.4 Empty state

```
╭─ Devices ─────────────────────────────────────── esc cancel ─╮
│                                                              │
│                        No devices found                      │
│                                                              │
│            Run your Flutter app with flog_dart               │
│                                                              │
╰──────────────────────────────────────────────────────────────╯
```

### 6.5 Height & scroll

- Device container height = 2 (borders) + 1 (top padding) + sum(app card heights + 1 spacing each) + 1 (bottom padding).
- App card height = 2 (borders) + 4 (name row + 3 detail rows) = 6; with 1 trailing spacer between cards.
- Waiting device height = 2 (borders) + 3 (padding + "Waiting…" + padding) = 5.
- Existing scroll logic in `draw_device_picker` continues to work with recomputed heights.

### 6.6 Click regions

Click regions still registered in `LayoutCache.device_picker_items` as `(row, x_start, x_end, sel_idx)`. Now span the full card-row rect (including the double/single border) so the whole card is clickable.

---

## 7. Bottom Status Bar

### 7.1 Layout

```
● LIVE   15/15   AuraLang v1.0.0 · iPhone 17 · :9753        Clear  Export  Stats  Help  Quit
```

### 7.2 Changes

- **Left side** gains: `AppName vX.Y · DeviceName · :Port` (currently only shows source name). Context is redundant with top chrome but keeps users oriented when focused on the list.
- **Buttons** unified: single SURFACE0 bg + SUBTEXT0 fg; no more per-button BG (PEACH/SAPPHIRE/RED/etc). Hover/active flash uses SURFACE1 or reverse-video. Each button = `  Label  ` (2-space padding).
- **Drop the `──` yellow separator**: replaced by 2 spaces of MANTLE between left and right groups.
- **Toast** overlay remains unchanged (green OK pill left-aligned, covering the info strip for its duration).

### 7.3 Button set

Labels only (no single-char icons on left): `Clear` `Export` `Stats` `Help` `Quit`. Click regions registered as `bottom_buttons` — unchanged names, style-only change.

---

## 8. Empty States

### 8.1 Waiting for connection (no devices)

```
                            ███████╗██╗      ██████╗  ██████╗
                            ██╔════╝██║     ██╔═══██╗██╔════╝
                            █████╗  ██║     ██║   ██║██║  ███╗
                            ██╔══╝  ██║     ██║   ██║██║   ██║
                            ██║     ███████╗╚██████╔╝╚██████╔╝
                            ╚═╝     ╚══════╝ ╚═════╝  ╚═════╝

                          Flutter Log Viewer · Network Inspector

                          ⣾  Waiting for connection on port 9753...

                      ┌─ Quick Start ──────────────────────────────┐
                      │                                            │
                      │   1. Add flog_dart to your Flutter app     │
                      │   2. Run your app in debug mode            │
                      │   3. flog will auto-connect                │
                      │                                            │
                      └────────────────────────────────────────────┘
```

**Changes**:
- Subtitle expanded: `Flutter Log Viewer · Network Inspector` (current subtitle omits Network capability).
- New `Quick Start` card for first-time users.
- When `discovered_devices` is non-empty, replace Quick Start with the existing `Discovered devices:` list (preserve current behavior).

### 8.2 Waiting for logs (connected but silent)

```
                            ███████╗██╗      ██████╗  ██████╗
                            ██╔════╝██║     ██╔═══██╗██╔════╝
                            █████╗  ██║     ██║   ██║██║  ███╗
                            ██╔══╝  ██║     ██║   ██║██║   ██║
                            ██║     ███████╗╚██████╔╝╚██████╔╝
                            ╚═╝     ╚══════╝ ╚═════╝  ╚═════╝

                            Connected · AuraLang v1.0.0 (iOS)

                          ⣾  Waiting for logs...
```

**Change**: replace static subtitle with the connected app name + platform.

### 8.3 No matching logs

```
                              ∅

                        No matching logs
                   Try adjusting filters or level

                   ┌────────────────────────────┐
                   │  Active filters:           │
                   │    search: "timeout"       │
                   │    level:  WARNING+        │
                   │    tags:   +Network -Auth  │
                   └────────────────────────────┘

                      press esc to clear all
```

**Changes**:
- New "Active filters" card enumerates every non-default filter so the user immediately sees *why* nothing matches.
- Add `esc` hint to clear all filters.
- Only render filter rows that are actually active (e.g., omit `tags:` if both include/exclude empty).

---

## 9. Color & Spacing Summary

| Element | Bg | Fg | Accent |
|---|---|---|---|
| Tab bar row | MANTLE | BLUE (active) / OVERLAY0 (inactive) | BLUE underline on active |
| Separator rule | MANTLE | SURFACE0 | — |
| Toolbar | MANTLE | TEXT / OVERLAY0 placeholder | BLUE / YELLOW / GREEN per level pill |
| Toolbar separator `│` | MANTLE | SURFACE1 | — |
| Log list row (normal) | BASE | TEXT / SUBTEXT0 | per-level fg |
| Log list row (selected) | SURFACE1 | TEXT | BLUE cursor bar |
| Jump-to-bottom pill | SURFACE0 | TEXT (label) / YELLOW (count) | SAPPHIRE rounded border |
| Device picker modal bg | MANTLE | — | SURFACE1 border |
| Device container | MANTLE | TEXT | SURFACE0 single-line border |
| App card (normal) | MANTLE | SUBTEXT0 | SURFACE0 single-line border |
| App card (ACTIVE) | SURFACE1 | TEXT bold | SAPPHIRE double-line `╔═╗` + GREEN `[ACTIVE]` |
| Bottom status bar | MANTLE | SUBTEXT0 | GREEN LIVE pill |
| Bottom buttons | SURFACE0 | SUBTEXT0 | hover → SURFACE1 |

---

## 10. Out of Scope

- Mouse drag-resize of detail panel (unchanged).
- Network tab toolbar (separate design; Network tab uses a similar toolbar pattern but its redesign is not in this pass).
- Log entry rendering itself (time/level/tag columns, wrap behavior, stack preview — unchanged).
- Keybindings and help overlay content (unchanged).
- Timeline code removal vs soft-hide: defer the "delete file vs keep dormant" question to implementation.

---

## 11. Validation

After implementation:
- Build `cargo build` passes.
- `cargo clippy` shows no new warnings in touched modules.
- Manual smoke test on real device (iPhone 17 + iOS sim concurrently):
  - Device picker shows both devices with correct ACTIVE/Normal states.
  - Tab bar right-side shows current app + LIVE.
  - Removing timeline freed exactly 3 rows into the log list viewport.
  - Jump-to-bottom pill appears on scroll-up, hides on return to tail, counts new logs correctly.
  - Empty states (disconnect, reconnect, add filter that matches nothing) all render with new copy.
