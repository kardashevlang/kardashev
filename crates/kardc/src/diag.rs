//! Diagnostics.
//!
//! Zig-philosophy: failures are values, never hidden. A compilation either
//! succeeds or returns a `Vec<Diagnostic>` that is rendered against the source
//! with a filename, line/column and a caret under the offending span.

use crate::span::Span;

/// A single compiler error, anchored to a source span.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub span: Span,
    /// A stable error code, e.g. `"E0001"`, for documentation and tests.
    pub code: &'static str,
    pub message: String,
}

impl Diagnostic {
    pub fn error(span: Span, code: &'static str, message: impl Into<String>) -> Diagnostic {
        Diagnostic {
            span,
            code,
            message: message.into(),
        }
    }
}

/// 1-based line and column for a byte offset into `src`.
fn line_col(offset: usize, src: &str) -> (usize, usize) {
    let offset = offset.min(src.len());
    let mut line = 1usize;
    let mut col = 1usize;
    for (i, ch) in src.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

/// Return the full text of the 1-based `line`.
fn line_text(line: usize, src: &str) -> &str {
    src.lines().nth(line.saturating_sub(1)).unwrap_or("")
}

/// Render a diagnostic as a multi-line human-readable string:
///
/// ```text
/// error[E0001]: unexpected token
///  --> main.ks:3:5
///   |
/// 3 |     retunr 0;
///   |     ^^^^^^
/// ```
pub fn render(d: &Diagnostic, filename: &str, src: &str) -> String {
    let (line, col) = line_col(d.span.start, src);
    let text = line_text(line, src);
    let caret_len = d.span.end.saturating_sub(d.span.start).max(1);
    // Clamp the caret to the remaining columns on the line so it never runs away.
    let caret_room = text.len().saturating_sub(col.saturating_sub(1)).max(1);
    let caret_len = caret_len.min(caret_room);
    let gutter = format!("{}", line);
    let pad = " ".repeat(gutter.len());
    let mut out = String::new();
    out.push_str(&format!("error[{}]: {}\n", d.code, d.message));
    out.push_str(&format!("{} --> {}:{}:{}\n", pad, filename, line, col));
    out.push_str(&format!("{} |\n", pad));
    out.push_str(&format!("{} | {}\n", gutter, text));
    out.push_str(&format!(
        "{} | {}{}\n",
        pad,
        " ".repeat(col.saturating_sub(1)),
        "^".repeat(caret_len)
    ));
    out
}

/// Render every diagnostic in `diags`, separated by blank lines.
pub fn render_all(diags: &[Diagnostic], filename: &str, src: &str) -> String {
    diags
        .iter()
        .map(|d| render(d, filename, src))
        .collect::<Vec<_>>()
        .join("\n")
}
