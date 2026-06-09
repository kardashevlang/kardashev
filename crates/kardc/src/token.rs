//! Tokens produced by the lexer.

use crate::span::Span;

/// Reserved keywords of the Zig-philosophy kardashev language.
///
/// Type names (`i32`, `bool`, ...) are *not* keywords — they are ordinary
/// identifiers resolved to builtin types during semantic analysis, which keeps
/// the lexer tiny and lets the type namespace grow without new tokens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kw {
    Pub,
    Fn,
    Const,
    Var,
    Return,
    If,
    Else,
    While,
    Break,
    Continue,
    Defer,
    Comptime,
    Test,
    True,
    False,
    And,
    Or,
    Struct,
    Orelse,
    Null,
    Try,
    Catch,
    Error,
    Enum,
    Switch,
    Union,
    Errdefer,
    For,
    Unreachable,
}

impl Kw {
    /// Map an identifier spelling to a keyword, if it is one.
    pub fn from_str(s: &str) -> Option<Kw> {
        Some(match s {
            "pub" => Kw::Pub,
            "fn" => Kw::Fn,
            "const" => Kw::Const,
            "var" => Kw::Var,
            "return" => Kw::Return,
            "if" => Kw::If,
            "else" => Kw::Else,
            "while" => Kw::While,
            "break" => Kw::Break,
            "continue" => Kw::Continue,
            "defer" => Kw::Defer,
            "comptime" => Kw::Comptime,
            "test" => Kw::Test,
            "true" => Kw::True,
            "false" => Kw::False,
            "and" => Kw::And,
            "or" => Kw::Or,
            "struct" => Kw::Struct,
            "orelse" => Kw::Orelse,
            "null" => Kw::Null,
            "try" => Kw::Try,
            "catch" => Kw::Catch,
            "error" => Kw::Error,
            "enum" => Kw::Enum,
            "switch" => Kw::Switch,
            "union" => Kw::Union,
            "errdefer" => Kw::Errdefer,
            "for" => Kw::For,
            "unreachable" => Kw::Unreachable,
            _ => return None,
        })
    }

    pub fn spelling(self) -> &'static str {
        match self {
            Kw::Pub => "pub",
            Kw::Fn => "fn",
            Kw::Const => "const",
            Kw::Var => "var",
            Kw::Return => "return",
            Kw::If => "if",
            Kw::Else => "else",
            Kw::While => "while",
            Kw::Break => "break",
            Kw::Continue => "continue",
            Kw::Defer => "defer",
            Kw::Comptime => "comptime",
            Kw::Test => "test",
            Kw::True => "true",
            Kw::False => "false",
            Kw::And => "and",
            Kw::Or => "or",
            Kw::Struct => "struct",
            Kw::Orelse => "orelse",
            Kw::Null => "null",
            Kw::Try => "try",
            Kw::Catch => "catch",
            Kw::Error => "error",
            Kw::Enum => "enum",
            Kw::Switch => "switch",
            Kw::Union => "union",
            Kw::Errdefer => "errdefer",
            Kw::For => "for",
            Kw::Unreachable => "unreachable",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum TokenKind {
    // Literals / names.
    Ident(String),
    Int(i64),
    Str(String),
    Keyword(Kw),

    // Punctuation & operators.
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Semicolon,
    Colon,
    Dot,
    Eq,      // =
    PlusEq,    // +=
    MinusEq,   // -=
    StarEq,    // *=
    SlashEq,   // /=
    PercentEq, // %=
    EqEq,    // ==
    BangEq,  // !=
    Lt,      // <
    Le,      // <=
    Gt,      // >
    Ge,      // >=
    Plus,    // +
    Minus,   // -
    Star,    // *
    Slash,   // /
    Percent,  // %
    Bang,     // !
    Question, // ?
    FatArrow, // =>  (switch arms)
    Amp,      // &   (address-of / bitwise and)
    DotDot,   // ..  (slice ranges)
    Pipe,     // |   (switch / capture payload binding / bitwise or)
    At,       // @   (builtins: @import)
    Caret,    // ^   (bitwise xor)
    Tilde,    // ~   (bitwise not)
    Shl,      // <<  (left shift)
    Shr,      // >>  (right shift)

    /// End of input. Always the final token.
    Eof,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    pub fn new(kind: TokenKind, span: Span) -> Token {
        Token { kind, span }
    }
}
