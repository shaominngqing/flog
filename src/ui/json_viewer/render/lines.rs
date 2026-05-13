//! Per-line emitters for the JSON tree render loop.
//!
//! Every rendered row has the layout
//!   `<outer_prefix><indent><marker><content>`
//! where `indent` derives from node depth (not from text width!) and
//! `marker` is a fixed-width 2-char cell (▼/▶/spaces) so content columns
//! always align.
//!
//! **File-size note (yellow, ~600 lines):** The URL-detection string
//! renderer (`render_string_with_url`) requires separate truncation logic
//! for three segments (before/url/after) with different priority ordering,
//! making it inherently verbose. Splitting it further would obscure the
//! single conceptual concern (render one string leaf with optional URL
//! highlighting). The budget warning is acknowledged here per §5.5.

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::ui::{sanitize_for_cell, BLUE, LAVENDER, OVERLAY0};

use super::super::action::{JsonAction, JsonHotRegion};
use super::super::palette::{
    brace_color, key_color, BOOL_COLOR, COMMA_COLOR, FOLD_COLOR, NULL_COLOR, NUM_COLOR, STR_COLOR,
};
use super::super::state::JsonViewerState;
use super::super::tree::{NodeKind, Tree};
use super::summaries::summarize_container;

// ── URL detection ─────────────────────────────────────────────────────────────

static URL_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();

fn url_regex() -> &'static regex::Regex {
    URL_RE.get_or_init(|| regex::Regex::new(r#"https?://[^\s"'<>]+"#).unwrap())
}

/// Render node `id` and its subtree. `trailing_comma` says whether to append
/// a comma after this node's value (used for non-last siblings).
#[allow(clippy::too_many_arguments)]
pub(super) fn render_node(
    out: &mut Vec<Line<'static>>,
    click_map: &mut Vec<Vec<JsonHotRegion>>,
    tree: &Tree,
    state: &JsonViewerState,
    section_key: &str,
    outer_prefix: &str,
    max_width: usize,
    id: u32,
    trailing_comma: bool,
) {
    let node = tree.node(id);
    let is_container = matches!(node.kind, NodeKind::Object | NodeKind::Array);

    if !is_container {
        push_leaf_line(
            out,
            click_map,
            tree,
            section_key,
            outer_prefix,
            max_width,
            id,
            trailing_comma,
        );
        return;
    }

    let expanded = state.is_expanded(id);
    if !expanded || node.children.is_empty() {
        push_container_collapsed(
            out,
            click_map,
            tree,
            section_key,
            outer_prefix,
            max_width,
            id,
            trailing_comma,
        );
        return;
    }

    // Recursion safety: `tree::parse` uses `serde_json::from_str` which has
    // a default nesting cap of 128. Any JSON that parses is safe to recurse on.
    push_container_opener(out, click_map, tree, section_key, outer_prefix, id);
    let child_count = node.children.len();
    for (i, &cid) in node.children.iter().enumerate() {
        let child_trailing = i + 1 < child_count;
        render_node(
            out,
            click_map,
            tree,
            state,
            section_key,
            outer_prefix,
            max_width,
            cid,
            child_trailing,
        );
    }
    push_container_closer(out, click_map, tree, outer_prefix, id, trailing_comma);
}

#[allow(clippy::too_many_arguments)]
fn push_leaf_line(
    out: &mut Vec<Line<'static>>,
    click_map: &mut Vec<Vec<JsonHotRegion>>,
    tree: &Tree,
    _section_key: &str,
    outer_prefix: &str,
    max_width: usize,
    id: u32,
    trailing_comma: bool,
) {
    let node = tree.node(id);
    let depth = node.depth;
    let indent = "  ".repeat(depth as usize);
    let marker = "  "; // leaves are not foldable

    let mut spans: Vec<Span<'static>> = Vec::new();
    spans.push(Span::raw(format!("{}{}{}", outer_prefix, indent, marker)));

    // Key part (for object entries)
    if let Some(ref key) = node.key {
        // UI-046: JSON object keys come from user data too.
        spans.push(Span::styled(
            format!("\"{}\"", sanitize_for_cell(key)),
            Style::default().fg(key_color(depth as usize)),
        ));
        spans.push(Span::styled(": ", Style::default().fg(COMMA_COLOR)));
    }

    // Remaining width for the value
    let used: usize = spans.iter().map(|s| s.content.as_ref().width()).sum();
    let remaining = max_width
        .saturating_sub(used)
        .saturating_sub(if trailing_comma { 1 } else { 0 });

    let (value_spans, hot_regions) = render_leaf_value(&node.kind, remaining, used, id);
    spans.extend(value_spans);

    if trailing_comma {
        spans.push(Span::styled(",", Style::default().fg(COMMA_COLOR)));
    }

    out.push(Line::from(spans));
    click_map.push(hot_regions);
}

/// Render a leaf node's value into spans plus any hot regions.
///
/// `x_offset` is the number of display columns already used on the line
/// (prefix + indent + marker + key) so that URL/expand region ranges use
/// the same coordinate system as the rest of the click map.
///
/// Returns `(spans, hot_regions)`.
fn render_leaf_value(
    kind: &NodeKind,
    max_width: usize,
    x_offset: usize,
    node_id: u32,
) -> (Vec<Span<'static>>, Vec<JsonHotRegion>) {
    match kind {
        NodeKind::Null => (
            vec![Span::styled(
                "null",
                Style::default()
                    .fg(NULL_COLOR)
                    .add_modifier(Modifier::ITALIC),
            )],
            Vec::new(),
        ),
        NodeKind::Bool(b) => (
            vec![Span::styled(
                if *b { "true" } else { "false" },
                Style::default().fg(BOOL_COLOR),
            )],
            Vec::new(),
        ),
        NodeKind::Number(s) => (
            vec![Span::styled(s.clone(), Style::default().fg(NUM_COLOR))],
            Vec::new(),
        ),
        NodeKind::String(s) => render_string_value(s, max_width, x_offset, node_id),
        // Containers reach this only during the stubbed path in render_node;
        // once Tasks 5/6 land, containers never hit here.
        NodeKind::Object => (
            vec![Span::styled("{…}", Style::default().fg(FOLD_COLOR))],
            Vec::new(),
        ),
        NodeKind::Array => (
            vec![Span::styled("[…]", Style::default().fg(FOLD_COLOR))],
            Vec::new(),
        ),
    }
}

/// Render a JSON string value with URL detection (Task 3).
///
/// Layout: `"<before_url><url><after_url>"` — the first URL found is
/// underlined + LAVENDER; the rest is STR_COLOR. If no URL is found and the
/// string is truncated, registers an `ExpandFullValue` region.
///
/// Column ranges in `JsonHotRegion` are relative to line start (x=0),
/// consistent with ToggleFold / CopyNode ranges.
fn render_string_value(
    s: &str,
    max_width: usize,
    x_offset: usize,
    node_id: u32,
) -> (Vec<Span<'static>>, Vec<JsonHotRegion>) {
    // UI-046: sanitize before measuring/truncating.
    let s_safe = sanitize_for_cell(s);

    // Detect the first URL in the raw (untruncated) string — we always carry
    // the full URL in the action even when display is truncated.
    let url_match = url_regex().find(&s_safe);

    if let Some(m) = url_match {
        render_string_with_url(
            &s_safe,
            m.start(),
            m.end(),
            m.as_str(),
            max_width,
            x_offset,
            node_id,
        )
    } else {
        render_plain_string(&s_safe, max_width, x_offset, node_id)
    }
}

/// Render a string that contains a URL. Only the first URL is highlighted.
///
/// The rendered form is: `"<before>` (STR_COLOR) + `<url>` (LAVENDER +
/// UNDERLINED) + `<after>"` (STR_COLOR). Truncation is applied to keep the
/// total display width within `max_width`.
fn render_string_with_url(
    s_safe: &str,
    url_start: usize, // byte offset into s_safe
    url_end: usize,   // byte offset into s_safe
    full_url: &str,
    max_width: usize,
    x_offset: usize,
    node_id: u32,
) -> (Vec<Span<'static>>, Vec<JsonHotRegion>) {
    // Three logical segments: before, url, after.
    let before = &s_safe[..url_start];
    let url_part = &s_safe[url_start..url_end];
    let after = &s_safe[url_end..];

    // Budget: max_width minus 2 for the surrounding quotes.
    // If max_width < 3 we don't have room for even an empty quoted string.
    let content_budget = if max_width >= 2 {
        max_width.saturating_sub(2)
    } else {
        0
    };

    // Truncation helper: consume chars from `src` until `budget` display cols
    // are exhausted. Returns (displayed_text, did_truncate).
    let truncate_to = |src: &str, budget: usize| -> (String, bool) {
        let w = src.width();
        if w <= budget {
            return (src.to_string(), false);
        }
        // Need to truncate. Reserve 1 col for the `…` replacement.
        let avail = budget.saturating_sub(1);
        let mut used = 0usize;
        let mut cut = 0usize;
        for (i, ch) in src.char_indices() {
            let cw = UnicodeWidthChar::width(ch).unwrap_or(1);
            if used + cw > avail {
                break;
            }
            used += cw;
            cut = i + ch.len_utf8();
        }
        (format!("{}…", &src[..cut]), true)
    };

    let before_w = before.width();
    let url_w = url_part.width();
    let after_w = after.width();
    let total_content_w = before_w + url_w + after_w;

    let (displayed_before, displayed_url, displayed_after, url_display_truncated) =
        if total_content_w <= content_budget {
            // Everything fits.
            (
                before.to_string(),
                url_part.to_string(),
                after.to_string(),
                false,
            )
        } else {
            // Need to truncate. Strategy: preserve `before` and `url` as
            // much as possible; trim `after` first, then `url`, then `before`.
            let after_budget = content_budget.saturating_sub(before_w + url_w);
            if url_w <= content_budget.saturating_sub(before_w) {
                // before + url fit; truncate after.
                let (da, _) = truncate_to(after, after_budget);
                (before.to_string(), url_part.to_string(), da, false)
            } else {
                // url itself needs truncation (or before does too).
                let before_budget = before_w.min(content_budget.saturating_sub(3)); // leave room for url…
                let (db, _) = truncate_to(before, before_budget);
                let used_before = db.width();
                let url_budget = content_budget.saturating_sub(used_before);
                let (du, url_trunc) = truncate_to(url_part, url_budget);
                (db, du, String::new(), url_trunc)
            }
        };

    // Build spans. Track x position relative to line start.
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut hot_regions: Vec<JsonHotRegion> = Vec::new();

    // Open quote + before segment
    let open_and_before = format!("\"{}", displayed_before);
    let open_before_w = open_and_before.width();
    spans.push(Span::styled(
        open_and_before,
        Style::default().fg(STR_COLOR),
    ));

    // URL segment — LAVENDER + UNDERLINED
    let url_x_start = (x_offset + open_before_w) as u16;
    let url_display_w = displayed_url.width();
    let url_x_end = url_x_start + url_display_w as u16;
    spans.push(Span::styled(
        displayed_url,
        Style::default()
            .fg(LAVENDER)
            .add_modifier(Modifier::UNDERLINED),
    ));

    // After segment + close quote
    let after_and_close = format!("{}\"", displayed_after);
    spans.push(Span::styled(
        after_and_close,
        Style::default().fg(STR_COLOR),
    ));

    // Register OpenUrl hot region (full URL, not truncated display).
    // Only register if the URL span has non-zero width.
    if url_x_end > url_x_start {
        hot_regions.push(JsonHotRegion {
            range: url_x_start..url_x_end,
            action: JsonAction::OpenUrl(full_url.to_string()),
        });
    }

    // If the value was truncated and there are non-URL columns in the string
    // span, also register ExpandFullValue for those columns.
    // "Truncated" = total displayed content < total original content, covering
    // all three cases: before truncated, url truncated, or after truncated.
    let total_original_w = before_w + url_w + after_w;
    // +2 for the surrounding quotes
    let is_truncated =
        url_display_truncated || (total_original_w + 2 > max_width && max_width >= 3);

    if is_truncated {
        // URL or after segment was truncated but there's remaining content.
        // ExpandFullValue region = the non-URL columns of the string span.
        let str_start = x_offset as u16;
        let full_rendered_w = (x_offset
            + open_before_w
            + url_display_w
            + displayed_after.width()
            + 1/* close quote */) as u16;
        // Columns before the URL
        if url_x_start > str_start {
            hot_regions.push(JsonHotRegion {
                range: str_start..url_x_start,
                action: JsonAction::ExpandFullValue(node_id),
            });
        }
        // Columns after the URL (if any)
        if url_x_end < full_rendered_w {
            hot_regions.push(JsonHotRegion {
                range: url_x_end..full_rendered_w,
                action: JsonAction::ExpandFullValue(node_id),
            });
        }
    }

    (spans, hot_regions)
}

/// Render a plain string (no URL). Applies CJK-aware truncation and
/// registers `ExpandFullValue` when the string is truncated.
fn render_plain_string(
    s_safe: &str,
    max_width: usize,
    x_offset: usize,
    node_id: u32,
) -> (Vec<Span<'static>>, Vec<JsonHotRegion>) {
    let quoted = format!("\"{}\"", s_safe);
    let is_truncated = quoted.width() > max_width && max_width >= 3;
    let text = if is_truncated {
        // Output format is `"<content>…"` — three fixed cells
        // (open quote, ellipsis, close quote).
        let budget = max_width.saturating_sub(3);
        let mut w = 0usize;
        let mut cut = 0usize;
        for (i, ch) in s_safe.char_indices() {
            let cw = UnicodeWidthChar::width(ch).unwrap_or(1);
            if w + cw > budget {
                break;
            }
            w += cw;
            cut = i + ch.len_utf8();
        }
        format!("\"{}…\"", &s_safe[..cut])
    } else {
        quoted
    };

    let span = Span::styled(text.clone(), Style::default().fg(STR_COLOR));
    let hot_regions = if is_truncated {
        let x_start = x_offset as u16;
        let x_end = x_start + text.width() as u16;
        vec![JsonHotRegion {
            range: x_start..x_end,
            action: JsonAction::ExpandFullValue(node_id),
        }]
    } else {
        Vec::new()
    };

    (vec![span], hot_regions)
}

#[allow(clippy::too_many_arguments)]
fn push_container_collapsed(
    out: &mut Vec<Line<'static>>,
    click_map: &mut Vec<Vec<JsonHotRegion>>,
    tree: &Tree,
    _section_key: &str,
    outer_prefix: &str,
    max_width: usize,
    id: u32,
    trailing_comma: bool,
) {
    let node = tree.node(id);
    let depth = node.depth;
    let indent = "  ".repeat(depth as usize);
    let empty = node.children.is_empty();

    let mut spans: Vec<Span<'static>> = Vec::new();
    spans.push(Span::raw(format!("{}{}", outer_prefix, indent)));

    // Marker. Empty containers are not foldable — show blank marker.
    if empty {
        spans.push(Span::raw("  "));
    } else {
        spans.push(Span::styled("▶ ", Style::default().fg(BLUE)));
    }

    // Key
    if let Some(ref key) = node.key {
        // UI-046: JSON object keys come from user data too.
        spans.push(Span::styled(
            format!("\"{}\"", sanitize_for_cell(key)),
            Style::default().fg(key_color(depth as usize)),
        ));
        spans.push(Span::styled(": ", Style::default().fg(COMMA_COLOR)));
    }

    let used: usize = spans.iter().map(|s| s.content.as_ref().width()).sum();
    let remaining = max_width
        .saturating_sub(used)
        .saturating_sub(if trailing_comma { 1 } else { 0 });

    if empty {
        // Empty: just `{}` or `[]`
        let (open, close) = match node.kind {
            NodeKind::Object => ("{", "}"),
            NodeKind::Array => ("[", "]"),
            _ => unreachable!(),
        };
        spans.push(Span::styled(
            format!("{}{}", open, close),
            Style::default().fg(brace_color(depth as usize)),
        ));
    } else {
        spans.extend(summarize_container(tree, id, remaining));
    }

    if trailing_comma {
        spans.push(Span::styled(",", Style::default().fg(COMMA_COLOR)));
    }

    // Append ⧉ copy icon for non-empty containers.
    let mut hot_regions = if empty {
        Vec::new()
    } else {
        vec![JsonHotRegion {
            range: 0..u16::MAX,
            action: JsonAction::ToggleFold(id),
        }]
    };

    if !empty {
        let line_text: String = spans.iter().map(|s| s.content.as_ref()).collect();
        let line_w = line_text.width() as u16;
        spans.push(Span::styled(" ⧉", Style::default().fg(OVERLAY0)));
        hot_regions.push(JsonHotRegion {
            range: line_w..line_w + 2,
            action: JsonAction::CopyNode(id),
        });
    }

    out.push(Line::from(spans));
    click_map.push(hot_regions);
}

fn push_container_opener(
    out: &mut Vec<Line<'static>>,
    click_map: &mut Vec<Vec<JsonHotRegion>>,
    tree: &Tree,
    _section_key: &str,
    outer_prefix: &str,
    id: u32,
) {
    let node = tree.node(id);
    let depth = node.depth;
    let indent = "  ".repeat(depth as usize);
    let open = match node.kind {
        NodeKind::Object => "{",
        NodeKind::Array => "[",
        _ => unreachable!(),
    };

    let mut spans: Vec<Span<'static>> = Vec::new();
    spans.push(Span::raw(format!("{}{}", outer_prefix, indent)));
    spans.push(Span::styled("▼ ", Style::default().fg(BLUE)));
    if let Some(ref key) = node.key {
        // UI-046: JSON object keys come from user data too.
        spans.push(Span::styled(
            format!("\"{}\"", sanitize_for_cell(key)),
            Style::default().fg(key_color(depth as usize)),
        ));
        spans.push(Span::styled(": ", Style::default().fg(COMMA_COLOR)));
    }
    spans.push(Span::styled(
        open.to_string(),
        Style::default().fg(brace_color(depth as usize)),
    ));

    let child_count = node.children.len();
    let mut hot_regions = vec![JsonHotRegion {
        range: 0..u16::MAX,
        action: JsonAction::ToggleFold(id),
    }];

    if child_count > 0 {
        let line_text: String = spans.iter().map(|s| s.content.as_ref()).collect();
        let line_w = line_text.width() as u16;
        spans.push(Span::styled(" ⧉", Style::default().fg(OVERLAY0)));
        hot_regions.push(JsonHotRegion {
            range: line_w..line_w + 2,
            action: JsonAction::CopyNode(id),
        });
    }

    out.push(Line::from(spans));
    click_map.push(hot_regions);
}

fn push_container_closer(
    out: &mut Vec<Line<'static>>,
    click_map: &mut Vec<Vec<JsonHotRegion>>,
    tree: &Tree,
    outer_prefix: &str,
    id: u32,
    trailing_comma: bool,
) {
    let node = tree.node(id);
    let depth = node.depth;
    let indent = "  ".repeat(depth as usize);
    let close = match node.kind {
        NodeKind::Object => "}",
        NodeKind::Array => "]",
        _ => unreachable!(),
    };

    let mut spans: Vec<Span<'static>> = Vec::new();
    spans.push(Span::raw(format!("{}{}", outer_prefix, indent)));
    spans.push(Span::raw("  ")); // marker column — closer is not foldable
    spans.push(Span::styled(
        close.to_string(),
        Style::default().fg(brace_color(depth as usize)),
    ));
    if trailing_comma {
        spans.push(Span::styled(",", Style::default().fg(COMMA_COLOR)));
    }
    out.push(Line::from(spans));
    click_map.push(Vec::new());
}
