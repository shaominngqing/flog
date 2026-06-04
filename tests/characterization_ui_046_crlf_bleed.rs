//! Characterization tests for UI-046.
//!
//! Bug: WS message bodies commonly carry `\r\n` (Azure Speech framing,
//! embedded HTTP-style headers, etc.). When those strings reached the
//! crossterm backend via a ratatui `Span`, the literal CRLF was emitted
//! to the terminal, the physical cursor jumped to the next line's
//! column 0, and the detail pane's following cells were painted on top
//! of the left-hand table.
//!
//! Invariant we want to hold: every string that becomes the body of a
//! `Span` is free of C0 control characters (\x00..=\x1f except \t) and
//! of \x7f DEL. The two public producers of such strings in `ui/` are
//! `wrap_text` and `wrap_multiline`; ad-hoc Span bodies should go
//! through `sanitize_for_cell`. These tests pin the contract.

use flog::ui::{sanitize_for_cell, wrap_multiline, wrap_text};

fn assert_cell_safe(s: &str, context: &str) {
    for (i, ch) in s.char_indices() {
        let code = ch as u32;
        let is_c0 = code < 0x20 && ch != '\t';
        let is_del = ch == '\x7f';
        assert!(
            !is_c0 && !is_del,
            "{}: byte {} = {:?} is a control char in output {:?}",
            context,
            i,
            ch,
            s
        );
    }
}

#[test]
fn ui_046_sanitize_strips_crlf() {
    let out = sanitize_for_cell("Path: foo\r\nX-RequestId: bar\r\nHello");
    // Exact replacement policy: \r dropped, \n -> space. That yields
    // a readable single-line preview without changing display width
    // semantics that callers depend on.
    assert_eq!(out, "Path: foo X-RequestId: bar Hello");
}

#[test]
fn ui_046_sanitize_strips_lone_lf_and_cr() {
    assert_eq!(sanitize_for_cell("a\nb"), "a b");
    assert_eq!(sanitize_for_cell("a\rb"), "ab");
}

#[test]
fn ui_046_sanitize_preserves_tab() {
    // Tabs are the one C0 char we leave alone — callers that want a
    // specific column width handle them explicitly.
    assert_eq!(sanitize_for_cell("a\tb"), "a\tb");
}

#[test]
fn ui_046_sanitize_strips_other_c0_and_del() {
    // ANSI ESC, bell, backspace, DEL — all would corrupt a terminal.
    assert_eq!(sanitize_for_cell("a\x1b[31mred\x1b[0m"), "a[31mred[0m");
    assert_eq!(sanitize_for_cell("bell\x07x"), "bellx");
    assert_eq!(sanitize_for_cell("bs\x08x"), "bsx");
    assert_eq!(sanitize_for_cell("del\x7fx"), "delx");
}

#[test]
fn ui_046_sanitize_is_noop_on_clean_text() {
    let clean = "Hello, world! 中文 🚀";
    assert_eq!(sanitize_for_cell(clean), clean);
}

#[test]
fn ui_046_wrap_text_output_is_cell_safe() {
    let messy = "Path: synthesis.context\r\nX-RequestId: abc\r\nContent-Type: application/json\r\n\r\n{\"x\":1}";
    for (i, line) in wrap_text(messy, 40, 20).iter().enumerate() {
        assert_cell_safe(line, &format!("wrap_text line {}", i));
    }
}

#[test]
fn ui_046_wrap_multiline_output_is_cell_safe() {
    // wrap_multiline treats \n as a soft line break. After splitting,
    // the per-segment strings must still be cell-safe (no leftover \r
    // from \r\n sequences).
    let messy = "line one\r\nline two\r\nline three";
    for (i, line) in wrap_multiline(messy, 40, 20).iter().enumerate() {
        assert_cell_safe(line, &format!("wrap_multiline line {}", i));
    }
}
