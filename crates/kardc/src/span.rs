//! Source spans — half-open byte ranges `[start, end)` into one source file.
//!
//! Every token and AST node carries a `Span` so diagnostics can point at the
//! exact source text that produced them.

/// A half-open byte range into the source file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    /// A placeholder span used for synthesized nodes that have no source text.
    pub const DUMMY: Span = Span { start: 0, end: 0 };

    pub fn new(start: usize, end: usize) -> Span {
        Span { start, end }
    }

    /// The smallest span covering both `self` and `other`.
    pub fn merge(self, other: Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}
