//! JSON tree rendering.
//!
//! Every rendered row has the layout
//!   <outer_prefix><indent><marker><content>
//! where `indent` derives from node depth (not from text width!) and
//! `marker` is a fixed-width 2-char cell (▼/▶/spaces) so content columns
//! always align.

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};
use unicode_width::UnicodeWidthStr;

use super::super::{BLUE, OVERLAY0};
use super::palette::{
    brace_color, key_color, BOOL_COLOR, COMMA_COLOR, FOLD_COLOR, NULL_COLOR, NUM_COLOR, STR_COLOR,
};
use super::state::JsonViewerState;
use super::tree::{NodeKind, Tree};

/// Render `tree` into `out`, pushing one `Option<(section_key, node_id)>`
/// into `click_map` for each line added. Clickable rows (foldable containers)
/// get `Some(...)`; leaves and close-brace lines get `None`.
///
/// `outer_prefix` is whitespace the caller wants prepended (e.g. `"   "` for
/// three-space section indent). `max_width` is the total budget INCLUDING
/// `outer_prefix` — callers typically pass `panel_width - outer_prefix.len()`.
/// Strings are truncated with `…` when the rendered line would exceed it.
pub fn append_render(
    out: &mut Vec<Line<'static>>,
    click_map: &mut Vec<Option<(String, u32)>>,
    tree: &Tree,
    state: &JsonViewerState,
    section_key: &str,
    outer_prefix: &str,
    max_width: usize,
) {
    render_node(
        out,
        click_map,
        tree,
        state,
        section_key,
        outer_prefix,
        max_width,
        0,
        false, // root never has a trailing comma
    );
}

/// Render node `id` and its subtree. `trailing_comma` says whether to append
/// a comma after this node's value (used for non-last siblings).
#[allow(clippy::too_many_arguments)]
fn render_node(
    out: &mut Vec<Line<'static>>,
    click_map: &mut Vec<Option<(String, u32)>>,
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
    click_map: &mut Vec<Option<(String, u32)>>,
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
        spans.push(Span::styled(
            format!("\"{}\"", key),
            Style::default().fg(key_color(depth as usize)),
        ));
        spans.push(Span::styled(": ", Style::default().fg(COMMA_COLOR)));
    }

    // Remaining width for the value
    let used: usize = spans.iter().map(|s| s.content.as_ref().width()).sum();
    let remaining = max_width
        .saturating_sub(used)
        .saturating_sub(if trailing_comma { 1 } else { 0 });

    spans.extend(render_leaf_value(&node.kind, remaining));

    if trailing_comma {
        spans.push(Span::styled(",", Style::default().fg(COMMA_COLOR)));
    }

    out.push(Line::from(spans));
    click_map.push(None);
}

fn render_leaf_value(kind: &NodeKind, max_width: usize) -> Vec<Span<'static>> {
    match kind {
        NodeKind::Null => vec![Span::styled(
            "null",
            Style::default()
                .fg(NULL_COLOR)
                .add_modifier(Modifier::ITALIC),
        )],
        NodeKind::Bool(b) => vec![Span::styled(
            if *b { "true" } else { "false" },
            Style::default().fg(BOOL_COLOR),
        )],
        NodeKind::Number(s) => vec![Span::styled(s.clone(), Style::default().fg(NUM_COLOR))],
        NodeKind::String(s) => {
            let quoted = format!("\"{}\"", s);
            let text = if quoted.width() > max_width && max_width >= 3 {
                // Output format is `"<content>…"` — three fixed cells
                // (open quote, ellipsis, close quote).
                let budget = max_width.saturating_sub(3);
                let mut w = 0usize;
                let mut cut = 0usize;
                for (i, ch) in s.char_indices() {
                    let cw = ch.to_string().as_str().width();
                    if w + cw > budget {
                        break;
                    }
                    w += cw;
                    cut = i + ch.len_utf8();
                }
                format!("\"{}…\"", &s[..cut])
            } else {
                quoted
            };
            vec![Span::styled(text, Style::default().fg(STR_COLOR))]
        }
        // Containers reach this only during the stubbed path in render_node;
        // once Tasks 5/6 land, containers never hit here.
        NodeKind::Object => vec![Span::styled("{…}", Style::default().fg(FOLD_COLOR))],
        NodeKind::Array => vec![Span::styled("[…]", Style::default().fg(FOLD_COLOR))],
    }
}

#[allow(clippy::too_many_arguments)]
fn push_container_collapsed(
    out: &mut Vec<Line<'static>>,
    click_map: &mut Vec<Option<(String, u32)>>,
    tree: &Tree,
    section_key: &str,
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
        spans.push(Span::styled(
            format!("\"{}\"", key),
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

    out.push(Line::from(spans));
    click_map.push(if empty {
        None
    } else {
        Some((section_key.to_string(), id))
    });
}

/// Render a collapsed container's one-line summary.
/// Objects: `{k: v, k2: v2, …}`. Arrays: `[v, v2, …] (<len>)`.
/// Width-aware: stops emitting entries when the budget is exhausted and
/// appends `…` + closer.
fn summarize_container(tree: &Tree, id: u32, max_width: usize) -> Vec<Span<'static>> {
    let node = tree.node(id);
    let depth = node.depth;
    let bc = Style::default().fg(brace_color(depth as usize));
    let (open, close) = match node.kind {
        NodeKind::Object => ("{", "}"),
        NodeKind::Array => ("[", "]"),
        _ => unreachable!(),
    };

    let mut spans: Vec<Span<'static>> = Vec::new();
    spans.push(Span::styled(open.to_string(), bc));

    // Reserve room for the closer + (optionally) `…` + (for arrays) ` (<len>)`
    let count = node.children.len();
    let count_suffix = if matches!(node.kind, NodeKind::Array) {
        format!(" ({})", count)
    } else {
        String::new()
    };
    // Reserve for: close + count_suffix + possible ", …" tail (3 cols).
    // Over-reserves by 2 when nothing truncates, which is acceptable — the
    // summary just gets slightly narrower, and the total line always fits.
    let reserved = close.width() + count_suffix.width() + 3;
    let budget = max_width.saturating_sub(1 + reserved); // -1 for open already pushed

    let mut used = 0usize;
    let mut emitted = 0usize;

    for (i, &cid) in node.children.iter().enumerate() {
        let preview = preview_child(tree, cid);
        let chunk_w: usize = preview.iter().map(|s| s.content.as_ref().width()).sum();
        let sep_w = if i == 0 { 0 } else { 2 }; // ", "
        if used + sep_w + chunk_w > budget {
            break;
        }
        if i > 0 {
            spans.push(Span::styled(", ", Style::default().fg(COMMA_COLOR)));
            used += 2;
        }
        spans.extend(preview);
        used += chunk_w;
        emitted += 1;
    }

    if emitted < count {
        if emitted > 0 {
            spans.push(Span::styled(", ", Style::default().fg(COMMA_COLOR)));
        }
        spans.push(Span::styled("…".to_string(), Style::default().fg(OVERLAY0)));
    }
    spans.push(Span::styled(close.to_string(), bc));
    if !count_suffix.is_empty() {
        spans.push(Span::styled(count_suffix, Style::default().fg(OVERLAY0)));
    }
    spans
}

/// One-entry preview inside a summary. Objects show `key: value`;
/// arrays show just `value`. Nested containers render as `{…}` / `[…]`.
fn preview_child(tree: &Tree, cid: u32) -> Vec<Span<'static>> {
    let child = tree.node(cid);
    let mut spans: Vec<Span<'static>> = Vec::new();
    if let Some(ref k) = child.key {
        spans.push(Span::styled(
            format!("\"{}\"", k),
            Style::default().fg(key_color(child.depth as usize)),
        ));
        spans.push(Span::styled(": ", Style::default().fg(COMMA_COLOR)));
    }
    match &child.kind {
        NodeKind::Null => spans.push(Span::styled(
            "null",
            Style::default()
                .fg(NULL_COLOR)
                .add_modifier(Modifier::ITALIC),
        )),
        NodeKind::Bool(b) => spans.push(Span::styled(
            if *b { "true" } else { "false" },
            Style::default().fg(BOOL_COLOR),
        )),
        NodeKind::Number(s) => spans.push(Span::styled(s.clone(), Style::default().fg(NUM_COLOR))),
        NodeKind::String(s) => {
            // In previews, clip long strings by column width (CJK-aware).
            // Budget = 20 display columns, matching "short summary" intent
            // without letting wide-character strings hog the line.
            const PREVIEW_BUDGET: usize = 20;
            let full_w = s.as_str().width();
            let text = if full_w <= PREVIEW_BUDGET {
                format!("\"{}\"", s)
            } else {
                let mut w = 0usize;
                let mut cut = 0usize;
                for (i, ch) in s.char_indices() {
                    let cw = ch.to_string().as_str().width();
                    if w + cw > PREVIEW_BUDGET {
                        break;
                    }
                    w += cw;
                    cut = i + ch.len_utf8();
                }
                format!("\"{}…\"", &s[..cut])
            };
            spans.push(Span::styled(text, Style::default().fg(STR_COLOR)));
        }
        NodeKind::Object => spans.push(Span::styled("{…}", Style::default().fg(FOLD_COLOR))),
        NodeKind::Array => spans.push(Span::styled("[…]", Style::default().fg(FOLD_COLOR))),
    }
    spans
}

fn push_container_opener(
    out: &mut Vec<Line<'static>>,
    click_map: &mut Vec<Option<(String, u32)>>,
    tree: &Tree,
    section_key: &str,
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
        spans.push(Span::styled(
            format!("\"{}\"", key),
            Style::default().fg(key_color(depth as usize)),
        ));
        spans.push(Span::styled(": ", Style::default().fg(COMMA_COLOR)));
    }
    spans.push(Span::styled(
        open.to_string(),
        Style::default().fg(brace_color(depth as usize)),
    ));

    out.push(Line::from(spans));
    click_map.push(Some((section_key.to_string(), id)));
}

fn push_container_closer(
    out: &mut Vec<Line<'static>>,
    click_map: &mut Vec<Option<(String, u32)>>,
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
    click_map.push(None);
}

#[cfg(test)]
mod tests {
    use super::super::{state, tree};
    use super::*;

    fn render(text: &str, width: usize) -> Vec<String> {
        let t = tree::parse(text).unwrap();
        let mut s = state::init_state(&t, 0);
        // Force the root collapsed so these tests exercise the collapsed path.
        if !s.expanded.is_empty() {
            s.expanded[0] = false;
        }
        let mut out = Vec::new();
        let mut cmap = Vec::new();
        append_render(&mut out, &mut cmap, &t, &s, "sec", "", width);
        assert_eq!(out.len(), cmap.len(), "out and click_map must stay in sync");
        out.iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect()
    }

    #[test]
    fn leaf_null() {
        let lines = render("null", 80);
        assert_eq!(lines, vec!["  null"]);
    }

    #[test]
    fn leaf_bool() {
        assert_eq!(render("true", 80), vec!["  true"]);
        assert_eq!(render("false", 80), vec!["  false"]);
    }

    #[test]
    fn leaf_number() {
        assert_eq!(render("1776684313608", 80), vec!["  1776684313608"]);
    }

    #[test]
    fn leaf_string_short() {
        assert_eq!(render(r#""hello""#, 80), vec![r#"  "hello""#]);
    }

    #[test]
    fn leaf_string_truncated() {
        // Marker "  " (width 2) + content budget 8 → string span must be ≤ 8
        // The string span is `"<prefix>…"` so content width must be ≤ 5.
        let lines = render(r#""abcdefghij""#, 10);
        let line = &lines[0];
        let line_w = unicode_width::UnicodeWidthStr::width(line.as_str());
        assert!(
            line_w <= 10,
            "line width {} exceeds max_width 10: {:?}",
            line_w,
            line
        );
        // Must keep the leading marker and the bookend quotes.
        assert!(
            line.starts_with("  \""),
            "missing marker + open quote: {:?}",
            line
        );
        assert!(
            line.ends_with("…\""),
            "missing ellipsis + close quote: {:?}",
            line
        );
        // Content between the quotes must be a non-empty prefix of "abcdefghij".
        let content = &line["  \"".len()..line.len() - "…\"".len()];
        assert!(!content.is_empty());
        assert!(
            "abcdefghij".starts_with(content),
            "not a prefix: {:?}",
            content
        );
    }

    #[test]
    fn collapsed_object_shows_summary() {
        let lines = render(r#"{"code": 0, "message": "ok"}"#, 80);
        assert_eq!(lines.len(), 1);
        assert!(
            lines[0].contains("▶"),
            "should have fold marker: {:?}",
            lines
        );
        assert!(lines[0].contains("code"));
        assert!(lines[0].contains('0'));
        assert!(lines[0].contains("message"));
        assert!(lines[0].contains("ok"));
    }

    #[test]
    fn collapsed_array_shows_length_suffix() {
        let lines = render("[1,2,3,4,5,6,7,8,9,10,11,12]", 40);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("(12)"), "should show count: {:?}", lines);
    }

    #[test]
    fn empty_object_not_foldable() {
        let t = tree::parse("{}").unwrap();
        let s = state::init_state(&t, 0);
        let mut out = Vec::new();
        let mut cmap = Vec::new();
        append_render(&mut out, &mut cmap, &t, &s, "sec", "", 80);
        // Empty object: one line, no click target
        assert_eq!(cmap, vec![None]);
        let rendered: String = out[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(rendered.contains("{}"));
    }

    #[test]
    fn collapsed_object_is_clickable() {
        let t = tree::parse(r#"{"a": 1}"#).unwrap();
        let mut s = state::init_state(&t, 0);
        // Force the root collapsed to test the click map.
        s.expanded[0] = false;
        let mut out = Vec::new();
        let mut cmap = Vec::new();
        append_render(&mut out, &mut cmap, &t, &s, "sec", "", 80);
        assert_eq!(cmap.len(), 1);
        assert_eq!(cmap[0], Some(("sec".into(), 0)));
    }

    #[test]
    fn nested_container_in_summary_shows_placeholder() {
        // Root object has one child `a` which is a nested object. When the
        // root is collapsed, `preview_child` should render `a`'s nested
        // object as `{…}`, not as its contents.
        let t = tree::parse(r#"{"a": {"b": 1}}"#).unwrap();
        let mut s = state::init_state(&t, 0);
        s.expanded[0] = false; // force root collapsed
        let mut out = Vec::new();
        let mut cmap = Vec::new();
        append_render(&mut out, &mut cmap, &t, &s, "sec", "", 80);
        assert_eq!(out.len(), 1);
        let rendered: String = out[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(
            rendered.contains("{…}"),
            "nested object should preview as placeholder: {:?}",
            rendered
        );
    }

    #[test]
    fn collapsed_array_fits_within_max_width() {
        // Sweep widths from 20 to 80, rendering a long array. Total line
        // width must never exceed max_width, including the `…] (N)` tail.
        let t = tree::parse("[111,222,333,444,555,666,777,888,999,1000,1001,1002]").unwrap();
        let mut s = state::init_state(&t, 0);
        s.expanded[0] = false; // force root collapsed
        for max_w in 20..=80 {
            let mut out = Vec::new();
            let mut cmap = Vec::new();
            append_render(&mut out, &mut cmap, &t, &s, "sec", "", max_w);
            assert_eq!(out.len(), 1);
            let rendered: String = out[0].spans.iter().map(|s| s.content.as_ref()).collect();
            let w = unicode_width::UnicodeWidthStr::width(rendered.as_str());
            assert!(
                w <= max_w,
                "width {} exceeds max_width {}: {:?}",
                w,
                max_w,
                rendered
            );
        }
    }

    #[test]
    fn collapsed_object_fits_within_max_width() {
        // Symmetric sweep for objects (no count suffix, but still should fit).
        let t =
            tree::parse(r#"{"code":200,"message":"ok","trace_id":"abc-def-1234","user":"alice"}"#)
                .unwrap();
        let mut s = state::init_state(&t, 0);
        s.expanded[0] = false;
        for max_w in 10..=80 {
            let mut out = Vec::new();
            let mut cmap = Vec::new();
            append_render(&mut out, &mut cmap, &t, &s, "sec", "", max_w);
            assert_eq!(out.len(), 1);
            let rendered: String = out[0].spans.iter().map(|s| s.content.as_ref()).collect();
            let w = unicode_width::UnicodeWidthStr::width(rendered.as_str());
            assert!(
                w <= max_w,
                "width {} exceeds max_width {}: {:?}",
                w,
                max_w,
                rendered
            );
        }
    }

    #[test]
    fn expanded_root_object_renders_children() {
        // With default_expand_depth=1, root is expanded and children are collapsed.
        let t = tree::parse(r#"{"a": 1, "b": "hi"}"#).unwrap();
        let s = state::init_state(&t, 1);
        let mut out = Vec::new();
        let mut cmap = Vec::new();
        append_render(&mut out, &mut cmap, &t, &s, "sec", "", 80);
        // Expected lines: ▼ {, "a": 1,, "b": "hi", }
        assert_eq!(
            out.len(),
            4,
            "lines={:?}",
            out.iter()
                .map(|l| l
                    .spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>())
                .collect::<Vec<_>>()
        );
        let texts: Vec<String> = out
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        assert!(texts[0].contains('{'));
        assert!(texts[0].contains('▼'));
        assert!(texts[1].contains("\"a\""));
        assert!(texts[1].contains('1'));
        assert!(texts[1].ends_with(','));
        assert!(texts[2].contains("\"b\""));
        assert!(texts[2].contains("hi"));
        assert!(texts[3].trim_end() == "  }");
        // Click map: opener clickable, children None, closer None
        assert_eq!(cmap[0], Some(("sec".into(), 0)));
        assert_eq!(cmap[1], None);
        assert_eq!(cmap[2], None);
        assert_eq!(cmap[3], None);
    }

    #[test]
    fn nested_expansion_with_collapsed_grandchild() {
        let t = tree::parse(r#"{"a": {"b": {"c": 1}}}"#).unwrap();
        // Expand depth 1: root and "a" expand; "b" stays collapsed.
        let s = state::init_state(&t, 1);
        let mut out = Vec::new();
        let mut cmap = Vec::new();
        append_render(&mut out, &mut cmap, &t, &s, "sec", "", 80);
        let texts: Vec<String> = out
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        // Expect: ▼ { / ▼ "a": { / ▶ "b": {c: 1} / } / }
        assert_eq!(out.len(), 5, "lines={:?}", texts);
        assert!(texts[0].contains('▼'));
        assert!(texts[1].contains('▼'));
        assert!(texts[1].contains("\"a\""));
        assert!(texts[2].contains('▶'));
        assert!(texts[2].contains("\"b\""));
        assert!(texts[2].contains("\"c\""));
        // Indentation alignment: children at depth d have exactly 2d spaces before marker
        let leading_spaces = |s: &str| s.chars().take_while(|c| *c == ' ').count();
        // texts[0] = "▼ {"     -> 0 leading spaces
        // texts[1] = "  ▼ \"a\"…" -> 2 leading spaces
        // texts[2] = "    ▶ \"b\"…" -> 4 leading spaces
        assert_eq!(leading_spaces(&texts[0]), 0);
        assert_eq!(leading_spaces(&texts[1]), 2);
        assert_eq!(leading_spaces(&texts[2]), 4);
    }

    #[test]
    fn array_of_objects_alignment() {
        // Screenshot scenario: array with each element an object.
        let t = tree::parse(r#"{"items": [{"id":1},{"id":2}]}"#).unwrap();
        // Expand root + items so we see both array entries as collapsed children.
        let s = state::init_state(&t, 1);
        // items is at depth 1, so already expanded. Array entries (depth 2) stay collapsed.
        let mut out = Vec::new();
        let mut cmap = Vec::new();
        append_render(&mut out, &mut cmap, &t, &s, "sec", "", 80);
        let texts: Vec<String> = out
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        // Expected: ▼ { / ▼ "items": [ / ▶ {id: 1}, / ▶ {id: 2} / ] / }
        assert_eq!(out.len(), 6, "lines={:?}", texts);
        let leading_spaces = |s: &str| s.chars().take_while(|c| *c == ' ').count();
        assert_eq!(leading_spaces(&texts[2]), 4); // first array element
        assert_eq!(leading_spaces(&texts[3]), 4); // second array element
                                                  // Closer at items' depth (1): 2 indent cols + 2 blank marker cols = 4.
        assert_eq!(leading_spaces(&texts[4]), 4);
    }
}
