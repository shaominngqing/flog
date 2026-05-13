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

/// Render `tree` into `out`, pushing one `Vec<JsonHotRegion>`
/// into `click_map` for each line added. Clickable rows (foldable containers)
/// get a single-element vec; leaves and close-brace lines get an empty vec.
///
/// `outer_prefix` is whitespace the caller wants prepended (e.g. `"   "` for
/// three-space section indent). `max_width` is the total budget INCLUDING
/// `outer_prefix` — callers typically pass `panel_width - outer_prefix.len()`.
/// Strings are truncated with `…` when the rendered line would exceed it.
pub fn append_render(
    out: &mut Vec<Line<'static>>,
    click_map: &mut Vec<Vec<super::action::JsonHotRegion>>,
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
        assert_eq!(cmap.len(), 1);
        assert!(cmap[0].is_empty());
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
        // Non-empty container: ToggleFold + CopyNode
        assert_eq!(cmap[0].len(), 2);
        assert!(matches!(
            cmap[0][0].action,
            crate::ui::json_viewer::JsonAction::ToggleFold(0)
        ));
        assert!(matches!(
            cmap[0][1].action,
            crate::ui::json_viewer::JsonAction::CopyNode(0)
        ));
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
        // Click map: opener has ToggleFold + CopyNode; children empty; closer empty.
        assert_eq!(cmap[0].len(), 2);
        assert!(matches!(
            cmap[0][0].action,
            crate::ui::json_viewer::JsonAction::ToggleFold(0)
        ));
        assert!(matches!(
            cmap[0][1].action,
            crate::ui::json_viewer::JsonAction::CopyNode(0)
        ));
        assert!(cmap[1].is_empty());
        assert!(cmap[2].is_empty());
        assert!(cmap[3].is_empty());
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
    fn collapsed_non_empty_container_has_copy_icon() {
        // Build a Tree with a non-empty object, force it collapsed, render,
        // verify ⧉ appears in the line text AND a CopyNode region exists.
        let t = tree::parse(r#"{"a": 1, "b": 2}"#).unwrap();
        let mut s = state::init_state(&t, 0);
        s.expanded[0] = false; // force collapsed
        let mut out = Vec::new();
        let mut cmap = Vec::new();
        append_render(&mut out, &mut cmap, &t, &s, "sec", "", 80);
        assert_eq!(out.len(), 1);
        let rendered: String = out[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(
            rendered.contains('⧉'),
            "expected ⧉ in collapsed non-empty container: {:?}",
            rendered
        );
        // Click map must have at least 2 regions: ToggleFold + CopyNode
        assert!(
            cmap[0].len() >= 2,
            "expected ToggleFold + CopyNode regions, got {:?}",
            cmap[0]
        );
        let has_copy = cmap[0]
            .iter()
            .any(|r| matches!(r.action, crate::ui::json_viewer::JsonAction::CopyNode(0)));
        assert!(has_copy, "expected CopyNode(0) region: {:?}", cmap[0]);
    }

    #[test]
    fn empty_container_has_no_copy_icon() {
        let t = tree::parse("{}").unwrap();
        let s = state::init_state(&t, 0);
        let mut out = Vec::new();
        let mut cmap = Vec::new();
        append_render(&mut out, &mut cmap, &t, &s, "sec", "", 80);
        assert_eq!(out.len(), 1);
        let rendered: String = out[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(
            !rendered.contains('⧉'),
            "empty container should have no ⧉: {:?}",
            rendered
        );
        assert!(
            cmap[0].is_empty(),
            "empty container should have no regions: {:?}",
            cmap[0]
        );
    }

    #[test]
    fn expanded_opener_has_copy_icon() {
        // Root expanded: opener line should have ⧉ and a CopyNode region.
        let t = tree::parse(r#"{"a": 1}"#).unwrap();
        let s = state::init_state(&t, 1); // depth 1 → root expanded
        let mut out = Vec::new();
        let mut cmap = Vec::new();
        append_render(&mut out, &mut cmap, &t, &s, "sec", "", 80);
        // opener line is out[0]
        let opener: String = out[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(
            opener.contains('⧉'),
            "expanded opener should have ⧉: {:?}",
            opener
        );
        let has_copy = cmap[0]
            .iter()
            .any(|r| matches!(r.action, crate::ui::json_viewer::JsonAction::CopyNode(0)));
        assert!(
            has_copy,
            "opener click map should have CopyNode(0): {:?}",
            cmap[0]
        );
        // ToggleFold should also be there
        let has_toggle = cmap[0]
            .iter()
            .any(|r| matches!(r.action, crate::ui::json_viewer::JsonAction::ToggleFold(0)));
        assert!(
            has_toggle,
            "opener click map should have ToggleFold(0): {:?}",
            cmap[0]
        );
    }

    // ── URL detection tests (Task 3) ──────────────────────────────────────

    /// Render a leaf string that contains a URL and return both lines and click_map.
    fn render_with_map(
        text: &str,
        width: usize,
    ) -> (Vec<String>, Vec<Vec<super::super::action::JsonHotRegion>>) {
        let t = tree::parse(text).unwrap();
        let s = state::init_state(&t, 0); // leave root at default (leaf = single node)
        let mut out = Vec::new();
        let mut cmap = Vec::new();
        append_render(&mut out, &mut cmap, &t, &s, "sec", "", width);
        assert_eq!(out.len(), cmap.len(), "out and click_map must stay in sync");
        let lines = out
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect();
        (lines, cmap)
    }

    #[test]
    fn url_in_string_is_underlined_and_clickable() {
        // String value containing https://example.com — wide enough to display without truncation.
        let (lines, cmap) = render_with_map(r#""https://example.com""#, 80);
        assert_eq!(lines.len(), 1);
        // 1. The rendered line contains the URL text.
        assert!(
            lines[0].contains("https://example.com"),
            "URL text should appear in line: {:?}",
            lines[0]
        );
        // 2. The click_map has a JsonHotRegion with OpenUrl action.
        assert!(
            !cmap[0].is_empty(),
            "click_map row 0 should be non-empty for URL string"
        );
        let has_open_url = cmap[0].iter().any(|r| {
            matches!(&r.action, super::super::action::JsonAction::OpenUrl(u) if u == "https://example.com")
        });
        assert!(
            has_open_url,
            "expected OpenUrl(\"https://example.com\") in cmap[0]: {:?}",
            cmap[0]
        );
        // 3. The URL span has UNDERLINED modifier — verify via Span styles in the Line.
        let t = tree::parse(r#""https://example.com""#).unwrap();
        let s = state::init_state(&t, 0);
        let mut out_lines = Vec::new();
        let mut out_cmap = Vec::new();
        append_render(&mut out_lines, &mut out_cmap, &t, &s, "sec", "", 80);
        let url_span = out_lines[0]
            .spans
            .iter()
            .find(|sp| sp.content.contains("https://example.com"));
        assert!(url_span.is_some(), "no span containing URL text");
        let style = url_span.unwrap().style;
        use ratatui::style::Modifier;
        assert!(
            style.add_modifier.contains(Modifier::UNDERLINED),
            "URL span should have UNDERLINED modifier, got: {:?}",
            style
        );
    }

    #[test]
    fn truncated_url_action_carries_full_url() {
        // Use a very long URL that will overflow a narrow width.
        let long_url = "https://example.com/very/long/path/that/definitely/exceeds/narrow/width/abcdefghijklmnopqrstuvwxyz";
        let json = format!("\"{}\"", long_url);
        let (_lines, cmap) = render_with_map(&json, 30); // narrow enough to truncate
        assert!(
            !cmap[0].is_empty(),
            "click_map row 0 should have regions for URL string"
        );
        let open_url_action = cmap[0].iter().find_map(|r| {
            if let super::super::action::JsonAction::OpenUrl(u) = &r.action {
                Some(u.clone())
            } else {
                None
            }
        });
        assert!(
            open_url_action.is_some(),
            "expected OpenUrl action in cmap[0]"
        );
        assert_eq!(
            open_url_action.unwrap(),
            long_url,
            "OpenUrl action must carry the FULL URL, not the truncated display version"
        );
    }

    #[test]
    fn non_url_string_no_url_region() {
        // Plain string without any URL.
        let (_lines, cmap) = render_with_map(r#""hello world""#, 80);
        assert_eq!(lines_count_check(&cmap), 1);
        let has_url = cmap[0]
            .iter()
            .any(|r| matches!(&r.action, super::super::action::JsonAction::OpenUrl(_)));
        assert!(
            !has_url,
            "plain string should not have OpenUrl region: {:?}",
            cmap[0]
        );
    }

    fn lines_count_check(cmap: &[Vec<super::super::action::JsonHotRegion>]) -> usize {
        cmap.len()
    }

    #[test]
    fn truncated_non_url_string_registers_expand() {
        // Long plain string that will be truncated — should register ExpandFullValue.
        let long_str = "abcdefghijklmnopqrstuvwxyz_ABCDEFGHIJKLMNOPQRSTUVWXYZ_1234567890";
        let json = format!("\"{}\"", long_str);
        let (_lines, cmap) = render_with_map(&json, 20); // narrow enough to truncate
        let has_expand = cmap[0].iter().any(|r| {
            matches!(
                &r.action,
                super::super::action::JsonAction::ExpandFullValue(_)
            )
        });
        assert!(
            has_expand,
            "truncated non-URL string should register ExpandFullValue: {:?}",
            cmap[0]
        );
    }

    /// Regression for C-1: when the `before` segment is truncated (the URL
    /// comes after a long prefix that overflows), `ExpandFullValue` must still
    /// be registered. The old buggy guard `url_display_truncated ||
    /// displayed_after.contains('…')` missed this case because `after` is
    /// empty when `before` itself exhausts the budget.
    #[test]
    fn url_with_long_before_prefix_registers_expand() {
        // "aaaa...aaaa https://x.com" — `before` fills the budget, so the
        // entire string is truncated even though `after` is empty.
        let before = "a".repeat(40);
        let json = format!("\"{}https://x.com\"", before);
        let (_lines, cmap) = render_with_map(&json, 20); // narrow: 2 marker + budget 18
        let has_expand = cmap[0].iter().any(|r| {
            matches!(
                &r.action,
                super::super::action::JsonAction::ExpandFullValue(_)
            )
        });
        assert!(
            has_expand,
            "string truncated via `before` segment should register ExpandFullValue: {:?}",
            cmap[0]
        );
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
