//! The `#include` directives of a C or C++ file (spec #806, "Finding includes"), found without running the
//! preprocessor: comments, string and character literals and C++ raw strings are skipped, a backslash at a line's end
//! joins it to the next, and no `#if` is evaluated.

/// One `#include "path"` or `#include <path>` directive.
#[derive(Debug, PartialEq)]
pub(super) struct Include {
    pub(super) path: String,
    /// Whether it is `"path"` and not `<path>`.
    pub(super) quoted: bool,
    /// The raw line of the `#`.
    pub(super) line: u32,
}

/// The directives of `text`, in order.
pub(super) fn includes(text: &str) -> Vec<Include> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let (spliced, splices) = splice(text.as_bytes());
    let mut found = Vec::new();
    // The line counter only moves forward: `line` is the raw line at offset `counted`.
    let mut line = 1u32;
    let mut counted = 0usize;
    let mut next_splice = 0usize;
    let mut at_line_start = true;
    let mut i = 0;
    while i < spliced.len() {
        match spliced[i] {
            b'\n' => {
                at_line_start = true;
                i += 1;
            }
            b' ' | b'\t' | b'\r' | 0x0b | 0x0c => i += 1,
            b'/' if spliced.get(i + 1) == Some(&b'/') => {
                i = spliced[i..]
                    .iter()
                    .position(|&b| b == b'\n')
                    .map_or(spliced.len(), |n| i + n);
            }
            b'/' if spliced.get(i + 1) == Some(&b'*') => {
                // A comment is one space, so it leaves the next code at a line's start only if it began at one.
                i = find(&spliced, i + 2, b"*/").map_or(spliced.len(), |n| n + 2);
            }
            b'#' if at_line_start => {
                if let Some((path, quoted, end)) = directive(&spliced, i + 1) {
                    for &b in &spliced[counted..i] {
                        line += u32::from(b == b'\n');
                    }
                    while next_splice < splices.len() && splices[next_splice] <= i {
                        line += 1;
                        next_splice += 1;
                    }
                    counted = i;
                    found.push(Include { path, quoted, line });
                    i = end;
                } else {
                    i += 1;
                }
                at_line_start = false;
            }
            b'"' => {
                at_line_start = false;
                i = match raw_string_open(&spliced, i) {
                    Some(delimiter) => skip_raw_string(&spliced, i, delimiter),
                    None => skip_quoted(&spliced, i, b'"'),
                };
            }
            b'0'..=b'9' if i == 0 || !is_identifier(spliced[i - 1]) => {
                at_line_start = false;
                i = skip_number(&spliced, i);
            }
            b'\'' => {
                at_line_start = false;
                i = skip_quoted(&spliced, i, b'\'');
            }
            _ => {
                at_line_start = false;
                i += 1;
            }
        }
    }
    found
}

/// `text` with every backslash-newline removed, and the offsets in the result where one was.
fn splice(text: &[u8]) -> (Vec<u8>, Vec<usize>) {
    let mut out = Vec::with_capacity(text.len());
    let mut splices = Vec::new();
    let mut i = 0;
    while i < text.len() {
        if text[i] == b'\\' {
            let after = match (text.get(i + 1), text.get(i + 2)) {
                (Some(b'\n'), _) => Some(i + 2),
                (Some(b'\r'), Some(b'\n')) => Some(i + 3),
                _ => None,
            };
            if let Some(after) = after {
                splices.push(out.len());
                i = after;
                continue;
            }
        }
        out.push(text[i]);
        i += 1;
    }
    (out, splices)
}

/// The offset of the first `needle` at or after `from`.
fn find(text: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    text.get(from..)?
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|n| from + n)
}

/// The directive after a `#` at `i - 1`: `include`, then `"path"` or `<path>` on the same line. Its path, whether
/// quoted, and the offset past it.
fn directive(text: &[u8], mut i: usize) -> Option<(String, bool, usize)> {
    let blanks = |mut at: usize| {
        while matches!(text.get(at), Some(b' ' | b'\t')) {
            at += 1;
        }
        at
    };
    i = blanks(i);
    text.get(i..)?.strip_prefix(b"include")?;
    i += "include".len();
    if text.get(i).is_some_and(|&b| is_identifier(b)) {
        return None;
    }
    i = blanks(i);
    let (close, quoted) = match text.get(i)? {
        b'"' => (b'"', true),
        b'<' => (b'>', false),
        _ => return None,
    };
    let start = i + 1;
    let len = text[start..]
        .iter()
        .position(|&b| b == close || b == b'\n')?;
    if text[start + len] != close {
        return None;
    }
    let path = String::from_utf8_lossy(&text[start..start + len]).into_owned();
    Some((path, quoted, start + len + 1))
}

/// For the `"` at `i`: the delimiter of the raw string it opens (`R"delimiter(`), when the prefix before it is one of
/// `R`, `LR`, `uR`, `UR` and `u8R`.
fn raw_string_open(text: &[u8], i: usize) -> Option<&[u8]> {
    let start = text[..i]
        .iter()
        .rposition(|&b| !is_identifier(b))
        .map_or(0, |n| n + 1);
    if !matches!(&text[start..i], b"R" | b"LR" | b"uR" | b"UR" | b"u8R") {
        return None;
    }
    let rest = &text[i + 1..];
    let open = rest.iter().position(|&b| b == b'(')?;
    let delimiter = &rest[..open];
    let valid = delimiter.len() <= 16
        && !delimiter
            .iter()
            .any(|b| matches!(b, b' ' | b'"' | b')' | b'\\' | b'\t' | b'\n' | b'\r'));
    valid.then_some(delimiter)
}

/// The offset past the raw string whose opening `"` is at `i`; the end of the text when it never closes.
fn skip_raw_string(text: &[u8], i: usize, delimiter: &[u8]) -> usize {
    let mut closing = vec![b')'];
    closing.extend_from_slice(delimiter);
    closing.push(b'"');
    find(text, i + 1 + delimiter.len(), &closing).map_or(text.len(), |n| n + closing.len())
}

fn is_identifier(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// The offset past the number that begins with the digit at `i`: a preprocessing number, whose `'` before a digit or
/// letter is a digit separator (`1'000`, `0xFF'FF`), not a character literal.
fn skip_number(text: &[u8], i: usize) -> usize {
    let mut at = i + 1;
    while let Some(&b) = text.get(at) {
        let next = text.get(at + 1).copied();
        at += match b {
            b'e' | b'E' | b'p' | b'P' if matches!(next, Some(b'+' | b'-')) => 2,
            b'\'' if next.is_some_and(is_identifier) => 2,
            _ if is_identifier(b) || b == b'.' => 1,
            _ => break,
        };
    }
    at
}

/// The offset past the string or character literal opened at `i` by `quote`: at its closing quote, or at the line's end
/// when it never closes there.
fn skip_quoted(text: &[u8], i: usize, quote: u8) -> usize {
    let mut at = i + 1;
    while let Some(&b) = text.get(at) {
        match b {
            b'\n' => return at,
            b'\\' => at += 2,
            _ if b == quote => return at + 1,
            _ => at += 1,
        }
    }
    text.len()
}
