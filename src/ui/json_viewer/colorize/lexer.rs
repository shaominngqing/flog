//! Low-level token scanners for the raw-text JSON colorizer.
//!
//! Each function takes the full line (as a `&[char]` view) and an index,
//! advances past one token, and returns the slice it consumed along with
//! the new index. The scanners are tolerant: unterminated strings and
//! mid-edit partial numbers are returned as-is rather than signalling an
//! error.

/// Result of [`scan_string`]: the literal slice (including the opening
/// quote and, when present, the closing quote) and whether the string
/// was terminated on this line.
pub(super) struct ScannedString {
    pub literal: String,
    pub terminated: bool,
    pub end: usize,
}

/// Scan a JSON string starting at `chars[start]`, which MUST be `"`.
/// Handles backslash escapes (including `\"`). When the closing quote is
/// missing (unterminated string, e.g. mid-edit), returns the prefix
/// collected so far with `terminated = false` and `end = len`.
pub(super) fn scan_string(chars: &[char], start: usize) -> ScannedString {
    debug_assert!(chars.get(start).copied() == Some('"'));
    let len = chars.len();
    let mut s = String::new();
    s.push('"');
    let mut j = start + 1;
    let mut terminated = false;
    while j < len {
        let c = chars[j];
        if c == '\\' && j + 1 < len {
            s.push(c);
            s.push(chars[j + 1]);
            j += 2;
            continue;
        }
        if c == '"' {
            s.push('"');
            j += 1;
            terminated = true;
            break;
        }
        s.push(c);
        j += 1;
    }
    ScannedString {
        literal: s,
        terminated,
        end: if terminated { j } else { len },
    }
}

/// After a terminated string, peek ahead (skipping whitespace) and
/// return `true` iff the next char is `:` — i.e. this string is a key.
pub(super) fn is_key_after(chars: &[char], after: usize) -> bool {
    let mut k = after;
    while k < chars.len() && chars[k].is_ascii_whitespace() {
        k += 1;
    }
    k < chars.len() && chars[k] == ':'
}

/// Scan a JSON number starting at `chars[start]`, which MUST be a digit
/// or `-`. Consumes digits, `.`, `e`/`E`, and `+`/`-`. Returns the
/// collected text and the new index.
pub(super) fn scan_number(chars: &[char], start: usize) -> (String, usize) {
    let len = chars.len();
    let mut num = String::new();
    num.push(chars[start]);
    let mut j = start + 1;
    while j < len
        && (chars[j].is_ascii_digit()
            || chars[j] == '.'
            || chars[j] == 'e'
            || chars[j] == 'E'
            || chars[j] == '+'
            || chars[j] == '-')
    {
        num.push(chars[j]);
        j += 1;
    }
    (num, j)
}

/// Match a JSON keyword (`true`, `false`, `null`) at `chars[start]`.
/// The keyword is only recognised when the character after it is NOT
/// alphanumeric and NOT `_` — so `truely`, `falsey`, `nullable` stay as
/// plain text.
pub(super) fn match_keyword(chars: &[char], start: usize, keyword: &str) -> bool {
    let kw_chars: Vec<char> = keyword.chars().collect();
    let end = start + kw_chars.len();
    if chars.get(start..end).map(|s| s.to_vec()) != Some(kw_chars) {
        return false;
    }
    let after = chars.get(end).copied().unwrap_or(' ');
    !after.is_alphanumeric() && after != '_'
}

/// Continue scanning an already-opened multi-line string from the start
/// of the current line. Returns the consumed text + index + whether the
/// string closed on this line.
pub(super) fn scan_string_continuation(chars: &[char]) -> (String, usize, bool) {
    let len = chars.len();
    let mut buf = String::new();
    let mut i = 0;
    while i < len {
        let c = chars[i];
        if c == '\\' && i + 1 < len {
            buf.push(c);
            buf.push(chars[i + 1]);
            i += 2;
            continue;
        }
        if c == '"' {
            buf.push(c);
            i += 1;
            return (buf, i, true);
        }
        buf.push(c);
        i += 1;
    }
    (buf, i, false)
}
