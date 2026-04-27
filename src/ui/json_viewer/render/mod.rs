//! JSON tree rendering.
//!
//! Split across submodules (Phase 3 UI-030 mirror):
//! * [`lines`]     — render loop + per-line emission
//!   (leaf / opener / closer / collapsed container)
//! * [`summaries`] — DevTools-style `{k: v, …}` / `[v, …] (N)` previews
//!   for collapsed containers
//! * `mod.rs`      — public entry [`append_render`] + tests

mod lines;
mod summaries;

use ratatui::text::Line;

use super::state::JsonViewerState;
use super::tree::Tree;

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
    lines::render_node(
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
