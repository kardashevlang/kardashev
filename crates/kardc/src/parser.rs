//! Parser: tokens → AST.
//!
//! A hand-written recursive-descent parser with precedence climbing for the
//! expression grammar, implementing SPEC §2 exactly. Every node is built with
//! a span that merges its children, so diagnostics from later stages point at
//! the right source text.
//!
//! Error handling collects multiple diagnostics: on a parse error a `E02xx`
//! diagnostic is pushed and the parser synchronizes — to the next `;`/`}` for a
//! statement, or to the next top-level item keyword for an item — so a single
//! `parse` call can report several independent syntax errors. `parse` returns
//! `Err` if any diagnostic was produced.

use crate::ast::{
    BinOp, Block, ConstDecl, EnumDecl, Expr, FieldDecl, FieldInit, Func, ImportDecl, Item, Module,
    Param, Stmt, StructDecl, SwitchArm, TestBlock, TypeExpr, UnOp, UnionDecl, UnionVariant,
};
use crate::diag::Diagnostic;
use crate::span::Span;
use crate::token::{Kw, Token, TokenKind};

/// Sentinel returned by a failed production. The diagnostic has already been
/// pushed into `Parser::diags`; the value just unwinds to the nearest recovery
/// point.
struct ParseError;

type PResult<T> = Result<T, ParseError>;

/// Parse a token stream (terminated by `Eof`) into a [`Module`].
///
/// On success returns the module; otherwise returns every diagnostic gathered
/// during parsing and recovery.
pub fn parse(tokens: &[Token]) -> Result<Module, Vec<Diagnostic>> {
    if tokens.is_empty() {
        return Ok(Module { items: Vec::new() });
    }
    let mut p = Parser {
        tokens,
        pos: 0,
        diags: Vec::new(),
    };
    let mut items = Vec::new();
    while !p.at_eof() {
        match p.parse_item() {
            Ok(item) => items.push(item),
            Err(_) => p.sync_item(),
        }
    }
    if p.diags.is_empty() {
        Ok(Module { items })
    } else {
        Err(p.diags)
    }
}

struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
    diags: Vec<Diagnostic>,
}

impl<'a> Parser<'a> {
    // ---- cursor helpers ---------------------------------------------------

    fn peek(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn peek_kind(&self) -> &TokenKind {
        &self.tokens[self.pos].kind
    }

    fn peek_span(&self) -> Span {
        self.tokens[self.pos].span
    }

    /// The kind one token ahead (clamped to the trailing `Eof`).
    fn peek2_kind(&self) -> &TokenKind {
        let i = (self.pos + 1).min(self.tokens.len().saturating_sub(1));
        &self.tokens[i].kind
    }

    /// Advance one token, never past the final `Eof`.
    fn bump(&mut self) {
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
    }

    fn at_eof(&self) -> bool {
        matches!(self.peek_kind(), TokenKind::Eof)
    }

    /// True if the current token equals a punctuation/operator kind. Only call
    /// with payload-free kinds (`LParen`, `Semicolon`, …), never `Ident`/`Int`/
    /// `Str`/`Keyword`, since equality there also compares the payload.
    fn at_punct(&self, kind: &TokenKind) -> bool {
        &self.tokens[self.pos].kind == kind
    }

    fn eat_punct(&mut self, kind: &TokenKind) -> bool {
        if self.at_punct(kind) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn expect_punct(&mut self, kind: &TokenKind, what: &str) -> PResult<Span> {
        if self.at_punct(kind) {
            let sp = self.peek_span();
            self.bump();
            Ok(sp)
        } else {
            Err(self.expected(what))
        }
    }

    fn at_kw(&self, kw: Kw) -> bool {
        matches!(self.peek_kind(), TokenKind::Keyword(k) if *k == kw)
    }

    fn eat_kw(&mut self, kw: Kw) -> bool {
        if self.at_kw(kw) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn expect_ident(&mut self) -> PResult<(String, Span)> {
        let span = self.peek_span();
        let name = match self.peek_kind() {
            TokenKind::Ident(s) => s.clone(),
            _ => return Err(self.expected("identifier")),
        };
        self.bump();
        Ok((name, span))
    }

    fn expect_str(&mut self) -> PResult<(String, Span)> {
        let span = self.peek_span();
        let s = match self.peek_kind() {
            TokenKind::Str(s) => s.clone(),
            _ => return Err(self.expected("string literal")),
        };
        self.bump();
        Ok((s, span))
    }

    /// Consume an integer literal, returning its value and span. Used for the
    /// array length `N` in a `[N]T` type (SPEC §14.1); a negative or absurd `N`
    /// is a sema concern (`E0224`), not the parser's.
    fn expect_int(&mut self) -> PResult<(i64, Span)> {
        let span = self.peek_span();
        let v = match self.peek_kind() {
            TokenKind::Int(v) => *v,
            _ => return Err(self.expected("integer literal")),
        };
        self.bump();
        Ok((v, span))
    }

    /// Push an `E0200` "expected X, found Y" diagnostic at the current token.
    fn expected(&mut self, what: &str) -> ParseError {
        let found = describe_kind(self.peek_kind());
        let span = self.peek_span();
        self.diags.push(Diagnostic::error(
            span,
            "E0200",
            format!("expected {}, found {}", what, found),
        ));
        ParseError
    }

    // ---- recovery ---------------------------------------------------------

    /// Skip tokens until the next top-level item keyword (or `Eof`).
    fn sync_item(&mut self) {
        while !self.at_eof() {
            match self.peek_kind() {
                TokenKind::Keyword(Kw::Pub)
                | TokenKind::Keyword(Kw::Fn)
                | TokenKind::Keyword(Kw::Const)
                | TokenKind::Keyword(Kw::Test)
                | TokenKind::At => return,
                _ => self.bump(),
            }
        }
    }

    /// Skip to the end of the current statement: consume through the next `;`,
    /// or stop (without consuming) at a closing `}` or `Eof`.
    fn sync_stmt(&mut self) {
        while !self.at_eof() {
            match self.peek_kind() {
                TokenKind::Semicolon => {
                    self.bump();
                    return;
                }
                TokenKind::RBrace => return,
                _ => self.bump(),
            }
        }
    }

    // ---- items ------------------------------------------------------------

    fn parse_item(&mut self) -> PResult<Item> {
        let start = self.peek_span();
        // `@import("path");` — a top-level builtin item (SPEC §22). The leading
        // `@` opens a builtin item form; it is never `pub`, so dispatch on it
        // before consuming the optional `pub` keyword.
        if self.at_punct(&TokenKind::At) {
            return self.parse_import(start);
        }
        let is_pub = self.eat_kw(Kw::Pub);
        let k = self.peek_kind().clone();
        match k {
            TokenKind::Keyword(Kw::Fn) => self.parse_func(is_pub, start),
            TokenKind::Keyword(Kw::Const) => self.parse_const(is_pub, start),
            TokenKind::Keyword(Kw::Test) => {
                if is_pub {
                    let sp = self.peek_span();
                    self.diags.push(Diagnostic::error(
                        sp,
                        "E0201",
                        "`test` blocks cannot be marked `pub`",
                    ));
                }
                self.parse_test(start)
            }
            _ => Err(self.expected("`fn`, `const`, or `test`")),
        }
    }

    /// Parse a top-level `@import("path");` declaration (SPEC §22.1). The cursor
    /// is on the `@` token. In v0.126 the only builtin item is `@import`, so the
    /// identifier after `@` must be exactly `import` (else `E0200`); the path is
    /// a string literal. The flattener (`modules::resolve`) resolves the path and
    /// erases this item before sema/emit. `@import` is a top-level item form, not
    /// an expression.
    fn parse_import(&mut self, start: Span) -> PResult<Item> {
        self.bump(); // `@`
        // The builtin name; only `import` exists in v0.126. Check the spelling
        // before consuming so a bad `@notimport(...)` reports `expected import`.
        let is_import = matches!(self.peek_kind(), TokenKind::Ident(s) if s == "import");
        if !is_import {
            return Err(self.expected("`import`"));
        }
        self.bump(); // `import`
        self.expect_punct(&TokenKind::LParen, "`(`")?;
        let (path, _) = self.expect_str()?;
        self.expect_punct(&TokenKind::RParen, "`)`")?;
        let semi = self.expect_punct(&TokenKind::Semicolon, "`;`")?;
        let span = start.merge(semi);
        Ok(Item::Import(ImportDecl { path, span }))
    }

    fn parse_func(&mut self, is_pub: bool, start: Span) -> PResult<Item> {
        Ok(Item::Func(self.parse_func_decl(is_pub, start)?))
    }

    /// Parse a function definition with the optional `pub` already consumed and
    /// the cursor positioned on the `fn` keyword. Shared by top-level functions
    /// and struct methods / associated functions (SPEC §10), so both grow new
    /// function-syntax features for free.
    fn parse_func_decl(&mut self, is_pub: bool, start: Span) -> PResult<Func> {
        self.bump(); // `fn`
        let (name, _) = self.expect_ident()?;
        self.expect_punct(&TokenKind::LParen, "`(`")?;
        let params = self.parse_params()?;
        self.expect_punct(&TokenKind::RParen, "`)`")?;
        let ret = self.parse_type()?;
        let body = self.parse_block()?;
        let span = start.merge(body.span);
        Ok(Func {
            is_pub,
            name,
            params,
            ret,
            body,
            span,
        })
    }

    fn parse_params(&mut self) -> PResult<Vec<Param>> {
        let mut params = Vec::new();
        if self.at_punct(&TokenKind::RParen) {
            return Ok(params);
        }
        loop {
            // An optional leading `comptime` keyword marks a compile-time type
            // parameter `comptime IDENT: type` (SPEC §17.1). The rest of the
            // parameter — `IDENT : type` — is parsed identically; for a comptime
            // type parameter the type name is the literal `type`, but it is just
            // an ordinary Ident-named `TypeExpr` here (sema gives `type`, and the
            // "comptime params precede runtime params" rule, their meaning). A
            // plain parameter has `is_comptime = false`.
            let comptime_span = if self.at_kw(Kw::Comptime) {
                let sp = self.peek_span();
                self.bump(); // `comptime`
                Some(sp)
            } else {
                None
            };
            let (name, name_span) = self.expect_ident()?;
            self.expect_punct(&TokenKind::Colon, "`:`")?;
            let ty = self.parse_type()?;
            // The span covers the `comptime` keyword when present, so diagnostics
            // about a comptime type parameter point at the whole declaration.
            let start = comptime_span.unwrap_or(name_span);
            let span = start.merge(ty.span);
            params.push(Param {
                name,
                ty,
                is_comptime: comptime_span.is_some(),
                span,
            });
            if self.eat_punct(&TokenKind::Comma) {
                if self.at_punct(&TokenKind::RParen) {
                    break; // trailing comma
                }
            } else {
                break;
            }
        }
        Ok(params)
    }

    /// Parse a type reference (SPEC §11.1, §12.1, §14.1, §15). A leading `?`
    /// marks an optional type `?T` (`optional = true`); a leading `!` marks an
    /// error union `!T` (`error_union = true`). The two prefixes are **mutually
    /// exclusive** in v0.115 and the parser consumes at most one: after a `?`
    /// or `!` it requires the inner type name, so `?!T` / `!?T` fail with an
    /// "expected identifier" diagnostic. A bare type has every flag `false`.
    ///
    /// A leading `[` introduces either a fixed-size array `[N]T` (`array_len =
    /// Some(N)`, `name = T`) when an integer length follows the `[`, or a slice
    /// `[]T` (`slice = true`) when the brackets are empty (SPEC §14.1, §15.2). A
    /// leading `*` introduces a single pointer `*T` (`pointer = true`, SPEC
    /// §15.1). In v0.118 the pointer / slice / array forms are **not** combined
    /// with `?`/`!` or with each other, so each is a distinct leading form that
    /// requires a plain element type name after its prefix (`[N]?T`, `*?T`,
    /// `[]?T` all fail with "expected identifier"). The node's span covers the
    /// prefix when present.
    fn parse_type(&mut self) -> PResult<TypeExpr> {
        // `*T` — a single pointer (SPEC §15.1). `parse_type` is only ever
        // called in type position, so a leading `*` here never collides with
        // the `*` multiplication operator (which lives in `parse_mul`).
        if self.at_punct(&TokenKind::Star) {
            let start = self.peek_span();
            self.bump(); // `*`
            let (name, name_span) = self.expect_ident()?;
            return Ok(TypeExpr {
                name,
                optional: false,
                error_union: false,
                array_len: None,
                pointer: true,
                slice: false,
                span: start.merge(name_span),
            });
        }
        if self.at_punct(&TokenKind::LBracket) {
            let start = self.peek_span();
            self.bump(); // `[`
            // Empty brackets `[]T` are a slice (SPEC §15.2); `[N]T` (an integer
            // length) is a fixed-size array (SPEC §14.1). Distinguish by
            // whether a `]` immediately follows the `[`.
            if self.at_punct(&TokenKind::RBracket) {
                self.bump(); // `]`
                let (name, name_span) = self.expect_ident()?;
                return Ok(TypeExpr {
                    name,
                    optional: false,
                    error_union: false,
                    array_len: None,
                    pointer: false,
                    slice: true,
                    span: start.merge(name_span),
                });
            }
            let (len, _) = self.expect_int()?;
            self.expect_punct(&TokenKind::RBracket, "`]`")?;
            let (name, name_span) = self.expect_ident()?;
            return Ok(TypeExpr {
                name,
                optional: false,
                error_union: false,
                array_len: Some(len),
                pointer: false,
                slice: false,
                span: start.merge(name_span),
            });
        }
        let opt_span = if self.at_punct(&TokenKind::Question) {
            let sp = self.peek_span();
            self.bump();
            Some(sp)
        } else {
            None
        };
        // Only consider a `!` error-union prefix when there was no `?`, so the
        // two prefixes can never both apply to one type.
        let err_span = if opt_span.is_none() && self.at_punct(&TokenKind::Bang) {
            let sp = self.peek_span();
            self.bump();
            Some(sp)
        } else {
            None
        };
        let (name, name_span) = self.expect_ident()?;
        let span = match opt_span.or(err_span) {
            Some(prefix) => prefix.merge(name_span),
            None => name_span,
        };
        Ok(TypeExpr {
            name,
            optional: opt_span.is_some(),
            error_union: err_span.is_some(),
            array_len: None,
            pointer: false,
            slice: false,
            span,
        })
    }

    fn parse_const(&mut self, is_pub: bool, start: Span) -> PResult<Item> {
        self.bump(); // `const`
        let (name, _) = self.expect_ident()?;
        // After `const IDENT`, the next token selects the item form:
        //   - `:` introduces a typed value binding `const IDENT : type = expr ;`
        //     (SPEC §2), with an explicit annotation (`ty = Some`).
        //   - `=` introduces either a *type declaration* — `= struct { … }`
        //     (SPEC §9.1) / `= enum { … }` (SPEC §13.1) / `= union(enum) { … }`
        //     (SPEC §20.1), selected by the keyword that follows the `=` — or,
        //     for any other following token, an *inferred* value binding
        //     `const IDENT = expr ;` (SPEC §18.1) whose type sema infers from
        //     the initializer (`ty = None`).
        if self.at_punct(&TokenKind::Eq) {
            match self.peek2_kind() {
                TokenKind::Keyword(Kw::Struct) => {
                    return self.parse_struct_decl(is_pub, name, start);
                }
                TokenKind::Keyword(Kw::Enum) => {
                    return self.parse_enum_decl(is_pub, name, start);
                }
                TokenKind::Keyword(Kw::Union) => {
                    return self.parse_union_decl(is_pub, name, start);
                }
                _ => {
                    // Inferred value const `const IDENT = expr ;` (no annotation).
                    self.bump(); // `=`
                    let value = self.parse_expr()?;
                    let semi = self.expect_punct(&TokenKind::Semicolon, "`;`")?;
                    let span = start.merge(semi);
                    return Ok(Item::Const(ConstDecl {
                        is_pub,
                        name,
                        ty: None,
                        value,
                        span,
                    }));
                }
            }
        }
        // Annotated value const `const IDENT : type = expr ;`.
        self.expect_punct(&TokenKind::Colon, "`:`")?;
        let ty = self.parse_type()?;
        self.expect_punct(&TokenKind::Eq, "`=`")?;
        let value = self.parse_expr()?;
        let semi = self.expect_punct(&TokenKind::Semicolon, "`;`")?;
        let span = start.merge(semi);
        Ok(Item::Const(ConstDecl {
            is_pub,
            name,
            ty: Some(ty),
            value,
            span,
        }))
    }

    /// Parse the tail of a struct declaration, with `const IDENT` already
    /// consumed and the cursor on the `=`:
    /// `= "struct" "{" (field ("," field)* ","?)? (func)* "}" ";"` where
    /// `field := IDENT ":" type` and `func := "pub"? "fn" ...` (SPEC §9.1,
    /// §10). The struct body is fields first, then zero or more methods /
    /// associated functions. Supports an empty `struct {}`.
    fn parse_struct_decl(&mut self, is_pub: bool, name: String, start: Span) -> PResult<Item> {
        self.bump(); // `=`
        if !self.eat_kw(Kw::Struct) {
            return Err(self.expected("`struct`"));
        }
        self.expect_punct(&TokenKind::LBrace, "`{`")?;
        // Fields come first: `IDENT : type`, comma-separated with an optional
        // trailing comma. A `pub`/`fn` keyword (the start of a method) or the
        // closing `}` ends the field list. Field names are identifiers, so they
        // never collide with the `pub`/`fn` keywords that introduce methods.
        let mut fields = Vec::new();
        while !self.at_punct(&TokenKind::RBrace)
            && !self.at_kw(Kw::Fn)
            && !self.at_kw(Kw::Pub)
        {
            let (fname, fname_span) = self.expect_ident()?;
            self.expect_punct(&TokenKind::Colon, "`:`")?;
            let ty = self.parse_type()?;
            let span = fname_span.merge(ty.span);
            fields.push(FieldDecl {
                name: fname,
                ty,
                span,
            });
            if !self.eat_punct(&TokenKind::Comma) {
                break; // no separator → the field list is done
            }
        }
        // Then methods / associated functions: each is `pub? fn ...`, parsed
        // with the shared function logic, until the closing `}` (SPEC §10).
        let mut methods = Vec::new();
        while !self.at_punct(&TokenKind::RBrace) {
            let m_start = self.peek_span();
            let m_pub = self.eat_kw(Kw::Pub);
            if !self.at_kw(Kw::Fn) {
                return Err(self.expected("`fn` or `}`"));
            }
            methods.push(self.parse_func_decl(m_pub, m_start)?);
        }
        self.expect_punct(&TokenKind::RBrace, "`}`")?;
        let semi = self.expect_punct(&TokenKind::Semicolon, "`;`")?;
        Ok(Item::Struct(StructDecl {
            is_pub,
            name,
            fields,
            methods,
            span: start.merge(semi),
        }))
    }

    /// Parse the tail of an enum declaration, with `const IDENT` already
    /// consumed and the cursor on the `=`:
    /// `= "enum" "{" IDENT ("," IDENT)* ","? "}" ";"` (SPEC §13.1). Plain
    /// (C-like) enums: a comma-separated list of variant names (no payloads),
    /// with an optional trailing comma. Variants are 0-based; duplicate-variant
    /// detection is a sema concern (`E0211`/`E0212`), not the parser's.
    fn parse_enum_decl(&mut self, is_pub: bool, name: String, start: Span) -> PResult<Item> {
        self.bump(); // `=`
        if !self.eat_kw(Kw::Enum) {
            return Err(self.expected("`enum`"));
        }
        self.expect_punct(&TokenKind::LBrace, "`{`")?;
        let mut variants = Vec::new();
        while !self.at_punct(&TokenKind::RBrace) {
            let (vname, _) = self.expect_ident()?;
            variants.push(vname);
            if !self.eat_punct(&TokenKind::Comma) {
                break; // no separator → the variant list is done
            }
        }
        self.expect_punct(&TokenKind::RBrace, "`}`")?;
        let semi = self.expect_punct(&TokenKind::Semicolon, "`;`")?;
        Ok(Item::Enum(EnumDecl {
            is_pub,
            name,
            variants,
            span: start.merge(semi),
        }))
    }

    /// Parse the tail of a tagged-union declaration, with `const IDENT` already
    /// consumed and the cursor on the `=`:
    /// `= "union" "(" "enum" ")" "{" variant ("," variant)* ","? "}" ";"` where
    /// `variant := IDENT ":" type` (SPEC §20.1). The `(enum)` after `union` is
    /// required syntax (kardashev's only union flavour is the tagged
    /// `union(enum)`). Each variant carries a payload type, parsed with the
    /// shared [`Parser::parse_type`] so payloads grow new type forms for free.
    /// A comma separates variants, with an optional trailing comma; duplicate
    /// variant names and payload-type resolution are sema concerns, not the
    /// parser's.
    fn parse_union_decl(&mut self, is_pub: bool, name: String, start: Span) -> PResult<Item> {
        self.bump(); // `=`
        if !self.eat_kw(Kw::Union) {
            return Err(self.expected("`union`"));
        }
        // The required `(enum)` tag after `union`.
        self.expect_punct(&TokenKind::LParen, "`(`")?;
        if !self.eat_kw(Kw::Enum) {
            return Err(self.expected("`enum`"));
        }
        self.expect_punct(&TokenKind::RParen, "`)`")?;
        self.expect_punct(&TokenKind::LBrace, "`{`")?;
        let mut variants = Vec::new();
        while !self.at_punct(&TokenKind::RBrace) {
            let (vname, vname_span) = self.expect_ident()?;
            self.expect_punct(&TokenKind::Colon, "`:`")?;
            let payload = self.parse_type()?;
            let span = vname_span.merge(payload.span);
            variants.push(UnionVariant {
                name: vname,
                payload,
                span,
            });
            if !self.eat_punct(&TokenKind::Comma) {
                break; // no separator → the variant list is done
            }
        }
        self.expect_punct(&TokenKind::RBrace, "`}`")?;
        let semi = self.expect_punct(&TokenKind::Semicolon, "`;`")?;
        Ok(Item::Union(UnionDecl {
            is_pub,
            name,
            variants,
            span: start.merge(semi),
        }))
    }

    fn parse_test(&mut self, start: Span) -> PResult<Item> {
        self.bump(); // `test`
        let (name, _) = self.expect_str()?;
        let body = self.parse_block()?;
        let span = start.merge(body.span);
        Ok(Item::Test(TestBlock { name, body, span }))
    }

    // ---- blocks & statements ---------------------------------------------

    fn parse_block(&mut self) -> PResult<Block> {
        let lbrace = self.expect_punct(&TokenKind::LBrace, "`{`")?;
        let mut stmts = Vec::new();
        while !self.at_eof() && !self.at_punct(&TokenKind::RBrace) {
            match self.parse_stmt() {
                Ok(s) => stmts.push(s),
                Err(_) => self.sync_stmt(),
            }
        }
        let rbrace = self.expect_punct(&TokenKind::RBrace, "`}`")?;
        Ok(Block {
            stmts,
            span: lbrace.merge(rbrace),
        })
    }

    fn parse_stmt(&mut self) -> PResult<Stmt> {
        let k = self.peek_kind().clone();
        match k {
            TokenKind::Keyword(Kw::Var) | TokenKind::Keyword(Kw::Const) => self.parse_let(),
            TokenKind::Keyword(Kw::Return) => self.parse_return(),
            TokenKind::Keyword(Kw::If) => self.parse_if(),
            TokenKind::Keyword(Kw::While) => self.parse_while(),
            TokenKind::Keyword(Kw::Break) => self.parse_break(),
            TokenKind::Keyword(Kw::Continue) => self.parse_continue(),
            TokenKind::Keyword(Kw::Defer) => self.parse_defer(),
            TokenKind::Keyword(Kw::Errdefer) => self.parse_errdefer(),
            TokenKind::Keyword(Kw::Switch) => self.parse_switch(),
            TokenKind::LBrace => Ok(Stmt::Block(self.parse_block()?)),
            TokenKind::Ident(_) if matches!(self.peek2_kind(), TokenKind::Eq) => self.parse_assign(),
            _ => self.parse_expr_stmt(),
        }
    }

    fn parse_let(&mut self) -> PResult<Stmt> {
        let start = self.peek_span();
        let is_const = matches!(self.peek_kind(), TokenKind::Keyword(Kw::Const));
        self.bump(); // `var` | `const`
        let (name, _) = self.expect_ident()?;
        // The type annotation is optional (SPEC §18.1): a `:` introduces an
        // explicit `: type` (`ty = Some`); otherwise the binding's type is
        // inferred from the initializer in sema (`ty = None`). Either way an
        // `= expr ;` initializer follows.
        let ty = if self.eat_punct(&TokenKind::Colon) {
            Some(self.parse_type()?)
        } else {
            None
        };
        self.expect_punct(&TokenKind::Eq, "`=`")?;
        let value = self.parse_expr()?;
        let semi = self.expect_punct(&TokenKind::Semicolon, "`;`")?;
        Ok(Stmt::Let {
            is_const,
            name,
            ty,
            value,
            span: start.merge(semi),
        })
    }

    fn parse_assign(&mut self) -> PResult<Stmt> {
        let (name, name_span) = self.expect_ident()?;
        self.expect_punct(&TokenKind::Eq, "`=`")?;
        let value = self.parse_expr()?;
        let semi = self.expect_punct(&TokenKind::Semicolon, "`;`")?;
        Ok(Stmt::Assign {
            name,
            value,
            span: name_span.merge(semi),
        })
    }

    fn parse_return(&mut self) -> PResult<Stmt> {
        let start = self.peek_span();
        self.bump(); // `return`
        if self.at_punct(&TokenKind::Semicolon) {
            let semi = self.peek_span();
            self.bump();
            return Ok(Stmt::Return {
                value: None,
                span: start.merge(semi),
            });
        }
        let value = self.parse_expr()?;
        let semi = self.expect_punct(&TokenKind::Semicolon, "`;`")?;
        Ok(Stmt::Return {
            value: Some(value),
            span: start.merge(semi),
        })
    }

    /// Parse an `if` statement (SPEC §2, §21.1):
    /// `if "(" cond ")" ("|" IDENT "|")? block ("else" (if_stmt | block))?`.
    /// After the parenthesised condition an optional `| IDENT |` payload capture
    /// (`TokenKind::Pipe IDENT Pipe`) selects the optional-`if` form
    /// `if (opt) |v| { … } else { … }` (SPEC §21.1): `cond` is an optional `?T`,
    /// and `v` (`capture = Some`) binds the unwrapped `T` inside the then-block.
    /// A plain `if (cond)` with no pipes leaves `capture = None` and `cond` is a
    /// `bool` (SPEC §2). That the captured condition is actually an optional, and
    /// the binding of `v`, are sema concerns (`E0280`), not the parser's. The
    /// optional `else` is parsed exactly as before — another `if` (an `else if`
    /// chain) or a block.
    fn parse_if(&mut self) -> PResult<Stmt> {
        let start = self.peek_span();
        self.bump(); // `if`
        self.expect_punct(&TokenKind::LParen, "`(`")?;
        let cond = self.parse_expr()?;
        self.expect_punct(&TokenKind::RParen, "`)`")?;
        // An optional `| IDENT |` capture (SPEC §21.1) binds the unwrapped
        // optional payload in the then-block. A bare `if (cond)` (no pipes)
        // leaves `capture = None` (a plain boolean `if`).
        let capture = if self.at_punct(&TokenKind::Pipe) {
            self.bump(); // `|`
            let (cap, _) = self.expect_ident()?;
            self.expect_punct(&TokenKind::Pipe, "`|`")?;
            Some(cap)
        } else {
            None
        };
        let then = self.parse_block()?;
        let mut end = then.span;
        let els = if self.at_kw(Kw::Else) {
            self.bump(); // `else`
            let s = if self.at_kw(Kw::If) {
                self.parse_if()?
            } else {
                Stmt::Block(self.parse_block()?)
            };
            end = s.span();
            Some(Box::new(s))
        } else {
            None
        };
        Ok(Stmt::If {
            cond,
            capture,
            then,
            els,
            span: start.merge(end),
        })
    }

    fn parse_while(&mut self) -> PResult<Stmt> {
        let start = self.peek_span();
        self.bump(); // `while`
        self.expect_punct(&TokenKind::LParen, "`(`")?;
        let cond = self.parse_expr()?;
        self.expect_punct(&TokenKind::RParen, "`)`")?;
        let cont = if self.at_punct(&TokenKind::Colon) {
            self.bump(); // `:`
            self.expect_punct(&TokenKind::LParen, "`(`")?;
            let c = self.parse_loop_cont()?;
            self.expect_punct(&TokenKind::RParen, "`)`")?;
            Some(Box::new(c))
        } else {
            None
        };
        let body = self.parse_block()?;
        let span = start.merge(body.span);
        Ok(Stmt::While {
            cond,
            cont,
            body,
            span,
        })
    }

    /// Parse a `while` continue-clause: an assignment `IDENT = expr` or a bare
    /// expression, with no trailing `;` (the closing `)` terminates it).
    fn parse_loop_cont(&mut self) -> PResult<Stmt> {
        if matches!(self.peek_kind(), TokenKind::Ident(_))
            && matches!(self.peek2_kind(), TokenKind::Eq)
        {
            let (name, name_span) = self.expect_ident()?;
            self.expect_punct(&TokenKind::Eq, "`=`")?;
            let value = self.parse_expr()?;
            let span = name_span.merge(value.span());
            Ok(Stmt::Assign { name, value, span })
        } else {
            let e = self.parse_expr()?;
            Ok(Stmt::Expr(e))
        }
    }

    fn parse_break(&mut self) -> PResult<Stmt> {
        let start = self.peek_span();
        self.bump(); // `break`
        let semi = self.expect_punct(&TokenKind::Semicolon, "`;`")?;
        Ok(Stmt::Break(start.merge(semi)))
    }

    fn parse_continue(&mut self) -> PResult<Stmt> {
        let start = self.peek_span();
        self.bump(); // `continue`
        let semi = self.expect_punct(&TokenKind::Semicolon, "`;`")?;
        Ok(Stmt::Continue(start.merge(semi)))
    }

    fn parse_defer(&mut self) -> PResult<Stmt> {
        let start = self.peek_span();
        self.bump(); // `defer`
        let inner = self.parse_stmt()?;
        let span = start.merge(inner.span());
        Ok(Stmt::Defer {
            stmt: Box::new(inner),
            span,
        })
    }

    /// Parse an `errdefer stmt;` statement (SPEC §21.2): like `defer`, it
    /// registers a single following statement to run on scope exit, but only on
    /// **error-return** paths (a `try` propagation or `return error.X`), not on
    /// normal exit. Parsing mirrors [`Parser::parse_defer`] exactly — the
    /// deferred body is one ordinary statement (a `print(x);` expression
    /// statement, a `{ … }` block, etc.) — and yields [`Stmt::ErrDefer`]. Which
    /// exit edges fire it is an emit concern (SPEC §4.4 / §21.2); sema checks the
    /// inner statement like a `defer`'s.
    fn parse_errdefer(&mut self) -> PResult<Stmt> {
        let start = self.peek_span();
        self.bump(); // `errdefer`
        let inner = self.parse_stmt()?;
        let span = start.merge(inner.span());
        Ok(Stmt::ErrDefer {
            stmt: Box::new(inner),
            span,
        })
    }

    /// Parse a `switch` statement (SPEC §13.1, §20.1):
    /// `switch "(" expr ")" "{" arm* else_arm? "}"`. Each arm is a
    /// comma-separated list of constant-pattern labels, then `=>`, then an
    /// optional `| IDENT |` payload capture, then a block; the optional
    /// `else => block` arm becomes `default`. Arms are separated by `,`, and a
    /// trailing `,` after a `}` block is optional, so the separator is consumed
    /// leniently after every arm. Labels are parsed as full expressions (they
    /// will be enum literals `.V` / `Enum.V` or integer literals); their
    /// validity against the scrutinee type and the exhaustiveness of the arms
    /// are sema concerns (`E0210`–`E0215`). A `| IDENT |` after `=>` binds the
    /// matched tagged-union variant's payload as a local in the arm body
    /// (`SwitchArm.capture`); a capture on a non-union switch (or a union arm
    /// missing one) is a sema concern (`E0272`), not the parser's.
    fn parse_switch(&mut self) -> PResult<Stmt> {
        let start = self.peek_span();
        self.bump(); // `switch`
        self.expect_punct(&TokenKind::LParen, "`(`")?;
        let scrutinee = self.parse_expr()?;
        self.expect_punct(&TokenKind::RParen, "`)`")?;
        self.expect_punct(&TokenKind::LBrace, "`{`")?;
        let mut arms = Vec::new();
        let mut default = None;
        while !self.at_eof() && !self.at_punct(&TokenKind::RBrace) {
            if self.at_kw(Kw::Else) {
                // The default arm: `else => block`. A later `else` simply
                // overwrites (a duplicate-default is a sema concern).
                self.bump(); // `else`
                self.expect_punct(&TokenKind::FatArrow, "`=>`")?;
                default = Some(self.parse_block()?);
            } else {
                let arm_start = self.peek_span();
                let mut labels = vec![self.parse_expr()?];
                while self.eat_punct(&TokenKind::Comma) {
                    // Tolerate a trailing `,` before the `=>` in a label list.
                    if self.at_punct(&TokenKind::FatArrow) {
                        break;
                    }
                    labels.push(self.parse_expr()?);
                }
                self.expect_punct(&TokenKind::FatArrow, "`=>`")?;
                // An optional `| IDENT |` payload capture (SPEC §20.1) binds the
                // matched variant's payload in the arm body. A bare `=>` (no
                // pipes) leaves `capture = None` (enum / integer switches).
                let capture = if self.at_punct(&TokenKind::Pipe) {
                    self.bump(); // `|`
                    let (cap, _) = self.expect_ident()?;
                    self.expect_punct(&TokenKind::Pipe, "`|`")?;
                    Some(cap)
                } else {
                    None
                };
                let body = self.parse_block()?;
                let span = arm_start.merge(body.span);
                arms.push(SwitchArm {
                    labels,
                    capture,
                    body,
                    span,
                });
            }
            // Arms are separated by `,`; a trailing comma after a block is
            // optional, so consume one if present and otherwise carry on.
            self.eat_punct(&TokenKind::Comma);
        }
        let rbrace = self.expect_punct(&TokenKind::RBrace, "`}`")?;
        Ok(Stmt::Switch {
            scrutinee,
            arms,
            default,
            span: start.merge(rbrace),
        })
    }

    fn parse_expr_stmt(&mut self) -> PResult<Stmt> {
        let expr = self.parse_expr()?;
        // A field-access place (`a.b.c = e;`), an index place (`a[i] = e;` /
        // `s[i] = e;`, SPEC §14.1/§15.2), or a deref place (`p.* = e;`, SPEC
        // §15.1) followed by `=` is a place assignment, reusing
        // `Stmt::FieldAssign`; a simple `name = e;` is handled by `parse_assign`
        // earlier in `parse_stmt`. Anything else is an expression statement.
        // Composites like `a[i].x = e;` / `m.data[i] = e;` parse here too, since
        // their place is a `Field`/`Index`/`Deref` chain.
        if matches!(
            expr,
            Expr::Field { .. } | Expr::Index { .. } | Expr::Deref { .. }
        ) && self.at_punct(&TokenKind::Eq)
        {
            self.bump(); // `=`
            let value = self.parse_expr()?;
            let semi = self.expect_punct(&TokenKind::Semicolon, "`;`")?;
            let span = expr.span().merge(semi);
            return Ok(Stmt::FieldAssign {
                place: expr,
                value,
                span,
            });
        }
        self.expect_punct(&TokenKind::Semicolon, "`;`")?;
        Ok(Stmt::Expr(expr))
    }

    // ---- expressions (precedence climbing) -------------------------------

    fn parse_expr(&mut self) -> PResult<Expr> {
        self.parse_orelse()
    }

    /// The lowest expression level (SPEC §11.1, §12.1): `orelse` and `catch`
    /// bind looser than every other operator, including `or`. They share this
    /// level and are left-associative, so `a orelse b catch c` nests as
    /// `((a orelse b) catch c)`. Each operand is a full `or`-expression.
    /// `a orelse b` unwraps an optional; `a catch b` unwraps an error union.
    fn parse_orelse(&mut self) -> PResult<Expr> {
        let mut lhs = self.parse_or()?;
        loop {
            if self.at_kw(Kw::Orelse) {
                self.bump();
                let rhs = self.parse_or()?;
                let span = lhs.span().merge(rhs.span());
                lhs = Expr::Orelse {
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                    span,
                };
            } else if self.at_kw(Kw::Catch) {
                self.bump();
                let default = self.parse_or()?;
                let span = lhs.span().merge(default.span());
                lhs = Expr::Catch {
                    expr: Box::new(lhs),
                    default: Box::new(default),
                    span,
                };
            } else {
                break;
            }
        }
        Ok(lhs)
    }

    fn parse_or(&mut self) -> PResult<Expr> {
        let mut lhs = self.parse_and()?;
        while self.at_kw(Kw::Or) {
            self.bump();
            let rhs = self.parse_and()?;
            let span = lhs.span().merge(rhs.span());
            lhs = Expr::Binary {
                op: BinOp::Or,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                span,
            };
        }
        Ok(lhs)
    }

    fn parse_and(&mut self) -> PResult<Expr> {
        let mut lhs = self.parse_cmp()?;
        while self.at_kw(Kw::And) {
            self.bump();
            let rhs = self.parse_cmp()?;
            let span = lhs.span().merge(rhs.span());
            lhs = Expr::Binary {
                op: BinOp::And,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                span,
            };
        }
        Ok(lhs)
    }

    fn parse_cmp(&mut self) -> PResult<Expr> {
        let mut lhs = self.parse_add()?;
        loop {
            let op = match self.peek_kind() {
                TokenKind::EqEq => BinOp::Eq,
                TokenKind::BangEq => BinOp::Ne,
                TokenKind::Lt => BinOp::Lt,
                TokenKind::Le => BinOp::Le,
                TokenKind::Gt => BinOp::Gt,
                TokenKind::Ge => BinOp::Ge,
                _ => break,
            };
            self.bump();
            let rhs = self.parse_add()?;
            let span = lhs.span().merge(rhs.span());
            lhs = Expr::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                span,
            };
        }
        Ok(lhs)
    }

    fn parse_add(&mut self) -> PResult<Expr> {
        let mut lhs = self.parse_mul()?;
        loop {
            let op = match self.peek_kind() {
                TokenKind::Plus => BinOp::Add,
                TokenKind::Minus => BinOp::Sub,
                _ => break,
            };
            self.bump();
            let rhs = self.parse_mul()?;
            let span = lhs.span().merge(rhs.span());
            lhs = Expr::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                span,
            };
        }
        Ok(lhs)
    }

    fn parse_mul(&mut self) -> PResult<Expr> {
        let mut lhs = self.parse_unary()?;
        loop {
            let op = match self.peek_kind() {
                TokenKind::Star => BinOp::Mul,
                TokenKind::Slash => BinOp::Div,
                TokenKind::Percent => BinOp::Rem,
                _ => break,
            };
            self.bump();
            let rhs = self.parse_unary()?;
            let span = lhs.span().merge(rhs.span());
            lhs = Expr::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                span,
            };
        }
        Ok(lhs)
    }

    fn parse_unary(&mut self) -> PResult<Expr> {
        // `try expr` (SPEC §12.1) is a prefix at the unary level: it parses a
        // unary operand after it (mirroring the `comptime` prefix) and yields
        // `Expr::Try`. The parser accepts `try` in any expression position; the
        // v0.115 statement-level-only restriction (E0190/E0191) is enforced in
        // sema, not here.
        if self.at_kw(Kw::Try) {
            let start = self.peek_span();
            self.bump(); // `try`
            let inner = self.parse_unary()?;
            let span = start.merge(inner.span());
            return Ok(Expr::Try {
                expr: Box::new(inner),
                span,
            });
        }
        // `&place` (SPEC §15.1) is an address-of prefix at the unary level: it
        // parses a unary operand (so `&x`, `&a.b`, `&a[i]`, `&p.*` all work)
        // and yields `Expr::AddrOf`. Whether the operand is a valid lvalue is a
        // sema concern (`E0231`), not the parser's.
        if self.at_punct(&TokenKind::Amp) {
            let start = self.peek_span();
            self.bump(); // `&`
            let inner = self.parse_unary()?;
            let span = start.merge(inner.span());
            return Ok(Expr::AddrOf {
                place: Box::new(inner),
                span,
            });
        }
        let op = match self.peek_kind() {
            TokenKind::Minus => Some(UnOp::Neg),
            TokenKind::Bang => Some(UnOp::Not),
            _ => None,
        };
        if let Some(op) = op {
            let start = self.peek_span();
            self.bump();
            let inner = self.parse_unary()?;
            let span = start.merge(inner.span());
            Ok(Expr::Unary {
                op,
                expr: Box::new(inner),
                span,
            })
        } else {
            self.parse_comptime()
        }
    }

    fn parse_comptime(&mut self) -> PResult<Expr> {
        if self.at_kw(Kw::Comptime) {
            let start = self.peek_span();
            self.bump();
            let inner = self.parse_postfix()?;
            let span = start.merge(inner.span());
            Ok(Expr::Comptime {
                expr: Box::new(inner),
                span,
            })
        } else {
            self.parse_postfix()
        }
    }

    /// Postfix level (SPEC §9.1, §10): a primary followed by zero or more `.name`
    /// accesses, left-associative so `a.b.c` nests as `(a.b).c`. When a `.name`
    /// is immediately followed by `(`, it is a method / associated-function call
    /// `Expr::MethodCall` instead of a plain field access; otherwise it stays
    /// `Expr::Field`. This composes left-to-right, so chains like `a.m().n` and
    /// `a.b.c(args)` parse naturally. Sits between `primary` and the
    /// `comptime`/`unary` levels.
    fn parse_postfix(&mut self) -> PResult<Expr> {
        let mut expr = self.parse_primary()?;
        loop {
            if self.at_punct(&TokenKind::Dot) {
                self.bump(); // `.`
                // `.*` dereferences a pointer (SPEC §15.1): a `*` immediately
                // after the `.` is `Expr::Deref` (the `.*` form lexes as `Dot`
                // then `Star`). This composes left-to-right with the other
                // postfix forms, so `p.*`, `a.b.*`, `f().*`, and `p.*.x` all
                // parse naturally.
                if self.at_punct(&TokenKind::Star) {
                    let star = self.peek_span();
                    self.bump(); // `*`
                    let span = expr.span().merge(star);
                    expr = Expr::Deref {
                        expr: Box::new(expr),
                        span,
                    };
                    continue;
                }
                // `.?` force-unwraps an optional (SPEC §11.1): a `?` immediately
                // after the `.` is `Expr::Unwrap`, panicking on null. Otherwise a
                // `.name` is a field access or — when followed by `(` — a method /
                // associated-function call. This composes left-to-right, so chains
                // like `a.b.?`, `f().?`, and `a.?.b` parse naturally.
                if self.at_punct(&TokenKind::Question) {
                    let q = self.peek_span();
                    self.bump(); // `?`
                    let span = expr.span().merge(q);
                    expr = Expr::Unwrap {
                        expr: Box::new(expr),
                        span,
                    };
                    continue;
                }
                let (name, name_span) = self.expect_ident()?;
                if self.at_punct(&TokenKind::LParen) {
                    self.bump(); // `(`
                    let args = self.parse_args()?;
                    let rparen = self.expect_punct(&TokenKind::RParen, "`)`")?;
                    let span = expr.span().merge(rparen);
                    expr = Expr::MethodCall {
                        receiver: Box::new(expr),
                        method: name,
                        args,
                        span,
                    };
                } else {
                    let span = expr.span().merge(name_span);
                    expr = Expr::Field {
                        base: Box::new(expr),
                        field: name,
                        span,
                    };
                }
            } else if self.at_punct(&TokenKind::LBracket) {
                // A postfix `[` after an operand is either an index `base[i]`
                // (SPEC §14.1) or a slice range `base[lo..hi]` (SPEC §15.2).
                // Parse the first expression, then a `..` (DotDot) selects the
                // slice form (`Expr::SliceExpr`); otherwise it is an index
                // (`Expr::Index`). Both compose left-to-right with `.field` /
                // calls / `.?` / `.*`, so `a[i]`, `a[i].x`, `m.data[i]`,
                // `a[i][j]`, and `a[lo..hi]` all parse naturally.
                self.bump(); // `[`
                let lo = self.parse_expr()?;
                if self.at_punct(&TokenKind::DotDot) {
                    self.bump(); // `..`
                    let hi = self.parse_expr()?;
                    let rbracket = self.expect_punct(&TokenKind::RBracket, "`]`")?;
                    let span = expr.span().merge(rbracket);
                    expr = Expr::SliceExpr {
                        base: Box::new(expr),
                        lo: Box::new(lo),
                        hi: Box::new(hi),
                        span,
                    };
                } else {
                    let rbracket = self.expect_punct(&TokenKind::RBracket, "`]`")?;
                    let span = expr.span().merge(rbracket);
                    expr = Expr::Index {
                        base: Box::new(expr),
                        index: Box::new(lo),
                        span,
                    };
                }
            } else {
                break;
            }
        }
        Ok(expr)
    }

    fn parse_primary(&mut self) -> PResult<Expr> {
        let tok = self.peek().clone();
        match tok.kind {
            TokenKind::Int(value) => {
                self.bump();
                Ok(Expr::Int {
                    value,
                    span: tok.span,
                })
            }
            TokenKind::Str(value) => {
                // A string literal `"…"` in expression position is an
                // `Expr::StrLit` of type `[]u8` (SPEC §23.1). The `Str` token
                // already carries the decoded (unescaped) contents, so we just
                // move them into the node. (The `test "name" { … }` block name
                // consumes a `Str` directly in `parse_test` and never reaches
                // here, so test names are not string-literal expressions.)
                self.bump();
                Ok(Expr::StrLit {
                    value,
                    span: tok.span,
                })
            }
            TokenKind::Keyword(Kw::True) => {
                self.bump();
                Ok(Expr::Bool {
                    value: true,
                    span: tok.span,
                })
            }
            TokenKind::Keyword(Kw::False) => {
                self.bump();
                Ok(Expr::Bool {
                    value: false,
                    span: tok.span,
                })
            }
            TokenKind::Keyword(Kw::Null) => {
                // The `null` literal (SPEC §11.1): the empty optional. Its `?T`
                // type is taken from the expected type at its position in sema.
                self.bump();
                Ok(Expr::Null { span: tok.span })
            }
            TokenKind::Keyword(Kw::Error) => {
                // `error.Name` (SPEC §12.1): an error value from the implicit
                // global error set. The `error` keyword is always followed by
                // `.` then the error name; the value coerces to any `!T`.
                self.bump(); // `error`
                self.expect_punct(&TokenKind::Dot, "`.`")?;
                let (name, name_span) = self.expect_ident()?;
                Ok(Expr::ErrorLit {
                    name,
                    span: tok.span.merge(name_span),
                })
            }
            TokenKind::Dot => {
                // A leading `.Variant` in expression-start position is an
                // unqualified enum literal (SPEC §13.1); its enum type comes
                // from context (the expected type or a `switch` scrutinee).
                // This is distinct from the *postfix* `.field` / `.method()` /
                // `.?` forms, which follow an operand and are handled in
                // `parse_postfix`; here there is no preceding operand.
                self.bump(); // `.`
                let (variant, variant_span) = self.expect_ident()?;
                Ok(Expr::EnumLit {
                    variant,
                    span: tok.span.merge(variant_span),
                })
            }
            TokenKind::Ident(name) => {
                self.bump();
                if self.at_punct(&TokenKind::LParen) {
                    self.bump(); // `(`
                    let args = self.parse_args()?;
                    let rparen = self.expect_punct(&TokenKind::RParen, "`)`")?;
                    Ok(Expr::Call {
                        callee: name,
                        args,
                        span: tok.span.merge(rparen),
                    })
                } else if self.at_punct(&TokenKind::LBrace) {
                    // Struct literal `Name{ .f = e, ... }` (SPEC §9.1). Reached
                    // only where an expression is expected, so it never collides
                    // with `if`/`while` blocks (whose conditions are parenthesised
                    // and whose `{` follows a `)`, not an identifier).
                    self.bump(); // `{`
                    let fields = self.parse_field_inits()?;
                    let rbrace = self.expect_punct(&TokenKind::RBrace, "`}`")?;
                    Ok(Expr::StructLit {
                        name,
                        fields,
                        span: tok.span.merge(rbrace),
                    })
                } else {
                    Ok(Expr::Ident {
                        name,
                        span: tok.span,
                    })
                }
            }
            TokenKind::LParen => {
                self.bump(); // `(`
                let inner = self.parse_expr()?;
                self.expect_punct(&TokenKind::RParen, "`)`")?;
                Ok(inner)
            }
            TokenKind::LBracket => {
                // An array literal `[N]T{ e0, e1, … }` (SPEC §14.1). A leading
                // `[` in expression-start position is always an array literal:
                // parse the array type `[N]T` (which consumes the `[`, length,
                // `]`, and element type name), then require `{` and a
                // comma-separated element list `}`. This is the only way `[`
                // starts a primary; a postfix `[` (indexing) is handled in
                // `parse_postfix` after an operand.
                let elem = self.parse_type()?;
                self.expect_punct(&TokenKind::LBrace, "`{`")?;
                let elems = self.parse_array_elems()?;
                let rbrace = self.expect_punct(&TokenKind::RBrace, "`}`")?;
                Ok(Expr::ArrayLit {
                    elem,
                    elems,
                    span: tok.span.merge(rbrace),
                })
            }
            _ => Err(self.expected("an expression")),
        }
    }

    /// Parse the comma-separated element expressions of an array literal, with
    /// the opening `{` already consumed and the cursor positioned just after
    /// it. Stops at (without consuming) the closing `}`. Supports an empty list
    /// and an optional trailing comma. Element count vs. the declared length
    /// `N` is a sema concern (`E0221`), not the parser's.
    fn parse_array_elems(&mut self) -> PResult<Vec<Expr>> {
        let mut elems = Vec::new();
        if self.at_punct(&TokenKind::RBrace) {
            return Ok(elems);
        }
        loop {
            let e = self.parse_expr()?;
            elems.push(e);
            if self.eat_punct(&TokenKind::Comma) {
                if self.at_punct(&TokenKind::RBrace) {
                    break; // trailing comma
                }
            } else {
                break;
            }
        }
        Ok(elems)
    }

    fn parse_args(&mut self) -> PResult<Vec<Expr>> {
        let mut args = Vec::new();
        if self.at_punct(&TokenKind::RParen) {
            return Ok(args);
        }
        loop {
            let e = self.parse_expr()?;
            args.push(e);
            if self.eat_punct(&TokenKind::Comma) {
                if self.at_punct(&TokenKind::RParen) {
                    break; // trailing comma
                }
            } else {
                break;
            }
        }
        Ok(args)
    }

    /// Parse the `.f = e` initializers of a struct literal, with the opening
    /// `{` already consumed and the cursor positioned just after it. Stops at
    /// (without consuming) the closing `}`. Supports an empty initializer list
    /// and an optional trailing comma (SPEC §9.1).
    fn parse_field_inits(&mut self) -> PResult<Vec<FieldInit>> {
        let mut fields = Vec::new();
        if self.at_punct(&TokenKind::RBrace) {
            return Ok(fields);
        }
        loop {
            let dot = self.expect_punct(&TokenKind::Dot, "`.`")?;
            let (name, _) = self.expect_ident()?;
            self.expect_punct(&TokenKind::Eq, "`=`")?;
            let value = self.parse_expr()?;
            let span = dot.merge(value.span());
            fields.push(FieldInit { name, value, span });
            if self.eat_punct(&TokenKind::Comma) {
                if self.at_punct(&TokenKind::RBrace) {
                    break; // trailing comma
                }
            } else {
                break;
            }
        }
        Ok(fields)
    }
}

/// Human-readable description of a token kind for diagnostic messages.
fn describe_kind(kind: &TokenKind) -> String {
    match kind {
        TokenKind::Ident(s) => format!("identifier `{}`", s),
        TokenKind::Int(v) => format!("integer `{}`", v),
        TokenKind::Str(s) => format!("string `\"{}\"`", s),
        TokenKind::Keyword(kw) => format!("keyword `{}`", kw.spelling()),
        TokenKind::LParen => "`(`".to_string(),
        TokenKind::RParen => "`)`".to_string(),
        TokenKind::LBrace => "`{`".to_string(),
        TokenKind::RBrace => "`}`".to_string(),
        TokenKind::LBracket => "`[`".to_string(),
        TokenKind::RBracket => "`]`".to_string(),
        TokenKind::Comma => "`,`".to_string(),
        TokenKind::Semicolon => "`;`".to_string(),
        TokenKind::Colon => "`:`".to_string(),
        TokenKind::Dot => "`.`".to_string(),
        TokenKind::Eq => "`=`".to_string(),
        TokenKind::EqEq => "`==`".to_string(),
        TokenKind::BangEq => "`!=`".to_string(),
        TokenKind::Lt => "`<`".to_string(),
        TokenKind::Le => "`<=`".to_string(),
        TokenKind::Gt => "`>`".to_string(),
        TokenKind::Ge => "`>=`".to_string(),
        TokenKind::Plus => "`+`".to_string(),
        TokenKind::Minus => "`-`".to_string(),
        TokenKind::Star => "`*`".to_string(),
        TokenKind::Slash => "`/`".to_string(),
        TokenKind::Percent => "`%`".to_string(),
        TokenKind::Bang => "`!`".to_string(),
        TokenKind::Question => "`?`".to_string(),
        TokenKind::FatArrow => "`=>`".to_string(),
        TokenKind::Amp => "`&`".to_string(),
        TokenKind::DotDot => "`..`".to_string(),
        TokenKind::Pipe => "`|`".to_string(),
        TokenKind::At => "`@`".to_string(),
        TokenKind::Eof => "end of input".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a token stream from kinds, assigning each a distinct 1-wide span
    /// and appending the trailing `Eof`. (The lexer is exercised separately;
    /// the parser is tested directly against token streams.)
    fn toks(kinds: Vec<TokenKind>) -> Vec<Token> {
        let mut v = Vec::new();
        let mut pos = 0usize;
        for k in kinds {
            v.push(Token::new(k, Span::new(pos, pos + 1)));
            pos += 1;
        }
        v.push(Token::new(TokenKind::Eof, Span::new(pos, pos + 1)));
        v
    }

    fn id(s: &str) -> TokenKind {
        TokenKind::Ident(s.to_string())
    }

    #[test]
    fn full_function() {
        // pub fn add(a: i32, b: i32) i32 { return a + b; }
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Pub),
            TokenKind::Keyword(Kw::Fn),
            id("add"),
            TokenKind::LParen,
            id("a"),
            TokenKind::Colon,
            id("i32"),
            TokenKind::Comma,
            id("b"),
            TokenKind::Colon,
            id("i32"),
            TokenKind::RParen,
            id("i32"),
            TokenKind::LBrace,
            TokenKind::Keyword(Kw::Return),
            id("a"),
            TokenKind::Plus,
            id("b"),
            TokenKind::Semicolon,
            TokenKind::RBrace,
        ]))
        .expect("should parse");
        assert_eq!(m.items.len(), 1);
        match &m.items[0] {
            Item::Func(f) => {
                assert!(f.is_pub);
                assert_eq!(f.name, "add");
                assert_eq!(f.params.len(), 2);
                assert_eq!(f.params[0].name, "a");
                assert_eq!(f.params[0].ty.name, "i32");
                assert!(!f.params[0].is_comptime);
                assert_eq!(f.params[1].name, "b");
                assert!(!f.params[1].is_comptime);
                assert_eq!(f.ret.name, "i32");
                assert!(f.span.start < f.span.end);
                assert_eq!(f.body.stmts.len(), 1);
                match &f.body.stmts[0] {
                    Stmt::Return {
                        value: Some(Expr::Binary { op: BinOp::Add, .. }),
                        ..
                    } => {}
                    other => panic!("expected `return a + b`, got {:?}", other),
                }
            }
            other => panic!("expected func, got {:?}", other),
        }
    }

    #[test]
    fn const_declaration() {
        // const MAX: i64 = 10;
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Const),
            id("MAX"),
            TokenKind::Colon,
            id("i64"),
            TokenKind::Eq,
            TokenKind::Int(10),
            TokenKind::Semicolon,
        ]))
        .expect("should parse");
        match &m.items[0] {
            Item::Const(c) => {
                assert!(!c.is_pub);
                assert_eq!(c.name, "MAX");
                let ty = c.ty.as_ref().expect("annotated const carries Some(ty)");
                assert_eq!(ty.name, "i64");
                match &c.value {
                    Expr::Int { value: 10, .. } => {}
                    other => panic!("expected int 10, got {:?}", other),
                }
            }
            other => panic!("expected const, got {:?}", other),
        }
    }

    #[test]
    fn test_block() {
        // test "adds" { expect(true); }
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Test),
            TokenKind::Str("adds".to_string()),
            TokenKind::LBrace,
            id("expect"),
            TokenKind::LParen,
            TokenKind::Keyword(Kw::True),
            TokenKind::RParen,
            TokenKind::Semicolon,
            TokenKind::RBrace,
        ]))
        .expect("should parse");
        match &m.items[0] {
            Item::Test(t) => {
                assert_eq!(t.name, "adds");
                assert_eq!(t.body.stmts.len(), 1);
                match &t.body.stmts[0] {
                    Stmt::Expr(Expr::Call { callee, args, .. }) => {
                        assert_eq!(callee, "expect");
                        assert_eq!(args.len(), 1);
                        assert!(matches!(args[0], Expr::Bool { value: true, .. }));
                    }
                    other => panic!("expected expect(true) call, got {:?}", other),
                }
            }
            other => panic!("expected test, got {:?}", other),
        }
    }

    /// `var s = "hello";` — the initializer parses as an `Expr::StrLit`
    /// (SPEC §23.1) carrying the decoded contents.
    #[test]
    fn str_lit_in_var_init() {
        // fn f() void { var s = "hello"; }
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Fn),
            id("f"),
            TokenKind::LParen,
            TokenKind::RParen,
            id("void"),
            TokenKind::LBrace,
            TokenKind::Keyword(Kw::Var),
            id("s"),
            TokenKind::Eq,
            TokenKind::Str("hello".to_string()),
            TokenKind::Semicolon,
            TokenKind::RBrace,
        ]))
        .expect("should parse");
        let body = match &m.items[0] {
            Item::Func(f) => &f.body,
            other => panic!("expected func, got {:?}", other),
        };
        match &body.stmts[0] {
            Stmt::Let {
                is_const: false,
                name,
                ty: None,
                value: Expr::StrLit { value, .. },
                ..
            } => {
                assert_eq!(name, "s");
                assert_eq!(value, "hello");
            }
            other => panic!("expected `var s = \"hello\";`, got {:?}", other),
        }
    }

    /// `print("hi")` — a string literal passed as a call argument is a `StrLit`.
    #[test]
    fn str_lit_call_arg() {
        // fn f() void { print("hi"); }
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Fn),
            id("f"),
            TokenKind::LParen,
            TokenKind::RParen,
            id("void"),
            TokenKind::LBrace,
            id("print"),
            TokenKind::LParen,
            TokenKind::Str("hi".to_string()),
            TokenKind::RParen,
            TokenKind::Semicolon,
            TokenKind::RBrace,
        ]))
        .expect("should parse");
        let body = match &m.items[0] {
            Item::Func(f) => &f.body,
            other => panic!("expected func, got {:?}", other),
        };
        match &body.stmts[0] {
            Stmt::Expr(Expr::Call { callee, args, .. }) => {
                assert_eq!(callee, "print");
                assert_eq!(args.len(), 1);
                match &args[0] {
                    Expr::StrLit { value, .. } => assert_eq!(value, "hi"),
                    other => panic!("expected StrLit arg, got {:?}", other),
                }
            }
            other => panic!("expected print(\"hi\") call, got {:?}", other),
        }
    }

    /// A `Str` inside a larger expression (here `s.len`) parses: the string
    /// literal is the base operand of a postfix `.len` field access.
    #[test]
    fn str_lit_in_larger_expr() {
        // fn f() void { var n = "abc".len; }
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Fn),
            id("f"),
            TokenKind::LParen,
            TokenKind::RParen,
            id("void"),
            TokenKind::LBrace,
            TokenKind::Keyword(Kw::Var),
            id("n"),
            TokenKind::Eq,
            TokenKind::Str("abc".to_string()),
            TokenKind::Dot,
            id("len"),
            TokenKind::Semicolon,
            TokenKind::RBrace,
        ]))
        .expect("should parse");
        let body = match &m.items[0] {
            Item::Func(f) => &f.body,
            other => panic!("expected func, got {:?}", other),
        };
        match &body.stmts[0] {
            Stmt::Let {
                value: Expr::Field { base, field, .. },
                ..
            } => {
                assert_eq!(field, "len");
                match base.as_ref() {
                    Expr::StrLit { value, .. } => assert_eq!(value, "abc"),
                    other => panic!("expected StrLit base, got {:?}", other),
                }
            }
            other => panic!("expected `\"abc\".len` field access, got {:?}", other),
        }
    }

    /// `test "name" { … }` still parses with the name as the test's `name`
    /// field, NOT as a `StrLit` expression: the `Str` is consumed by
    /// `parse_test`, not by `parse_primary`.
    #[test]
    fn test_name_is_not_str_lit_expr() {
        // test "adds" { }
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Test),
            TokenKind::Str("adds".to_string()),
            TokenKind::LBrace,
            TokenKind::RBrace,
        ]))
        .expect("should parse");
        match &m.items[0] {
            Item::Test(t) => {
                assert_eq!(t.name, "adds");
                assert!(t.body.stmts.is_empty());
            }
            other => panic!("expected test, got {:?}", other),
        }
    }

    #[test]
    fn if_else() {
        // fn f() void { if (x) { return; } else { return; } }
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Fn),
            id("f"),
            TokenKind::LParen,
            TokenKind::RParen,
            id("void"),
            TokenKind::LBrace,
            TokenKind::Keyword(Kw::If),
            TokenKind::LParen,
            id("x"),
            TokenKind::RParen,
            TokenKind::LBrace,
            TokenKind::Keyword(Kw::Return),
            TokenKind::Semicolon,
            TokenKind::RBrace,
            TokenKind::Keyword(Kw::Else),
            TokenKind::LBrace,
            TokenKind::Keyword(Kw::Return),
            TokenKind::Semicolon,
            TokenKind::RBrace,
            TokenKind::RBrace,
        ]))
        .expect("should parse");
        let body = match &m.items[0] {
            Item::Func(f) => &f.body,
            other => panic!("expected func, got {:?}", other),
        };
        match &body.stmts[0] {
            Stmt::If {
                els: Some(els),
                then,
                ..
            } => {
                assert_eq!(then.stmts.len(), 1);
                assert!(matches!(**els, Stmt::Block(_)));
            }
            other => panic!("expected if/else, got {:?}", other),
        }
    }

    #[test]
    fn while_with_continue_expr() {
        // fn f() void { while (x) : (y) { break; } }
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Fn),
            id("f"),
            TokenKind::LParen,
            TokenKind::RParen,
            id("void"),
            TokenKind::LBrace,
            TokenKind::Keyword(Kw::While),
            TokenKind::LParen,
            id("x"),
            TokenKind::RParen,
            TokenKind::Colon,
            TokenKind::LParen,
            id("y"),
            TokenKind::RParen,
            TokenKind::LBrace,
            TokenKind::Keyword(Kw::Break),
            TokenKind::Semicolon,
            TokenKind::RBrace,
            TokenKind::RBrace,
        ]))
        .expect("should parse");
        let body = match &m.items[0] {
            Item::Func(f) => &f.body,
            other => panic!("expected func, got {:?}", other),
        };
        match &body.stmts[0] {
            Stmt::While {
                cont: Some(cont),
                body,
                ..
            } => {
                assert!(matches!(cont.as_ref(), Stmt::Expr(Expr::Ident { .. })));
                assert!(matches!(body.stmts[0], Stmt::Break(_)));
            }
            other => panic!("expected while-with-continue, got {:?}", other),
        }
    }

    #[test]
    fn defer_statement() {
        // fn f() void { defer print(x); }
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Fn),
            id("f"),
            TokenKind::LParen,
            TokenKind::RParen,
            id("void"),
            TokenKind::LBrace,
            TokenKind::Keyword(Kw::Defer),
            id("print"),
            TokenKind::LParen,
            id("x"),
            TokenKind::RParen,
            TokenKind::Semicolon,
            TokenKind::RBrace,
        ]))
        .expect("should parse");
        let body = match &m.items[0] {
            Item::Func(f) => &f.body,
            other => panic!("expected func, got {:?}", other),
        };
        match &body.stmts[0] {
            Stmt::Defer { stmt, .. } => match &**stmt {
                Stmt::Expr(Expr::Call { callee, .. }) => assert_eq!(callee, "print"),
                other => panic!("expected deferred print() call, got {:?}", other),
            },
            other => panic!("expected defer, got {:?}", other),
        }
    }

    /// Parse the statements `stmt_kinds` by wrapping them in
    /// `fn f() void { <stmt_kinds> }` and returning the function body's
    /// statements. (v0.125 `if`-capture / `errdefer` statement tests.)
    fn body_stmts(stmt_kinds: Vec<TokenKind>) -> Vec<Stmt> {
        let mut kinds = vec![
            TokenKind::Keyword(Kw::Fn),
            id("f"),
            TokenKind::LParen,
            TokenKind::RParen,
            id("void"),
            TokenKind::LBrace,
        ];
        kinds.extend(stmt_kinds);
        kinds.push(TokenKind::RBrace);
        let m = parse(&toks(kinds)).expect("should parse");
        match m.items.into_iter().next() {
            Some(Item::Func(f)) => f.body.stmts,
            other => panic!("expected func, got {:?}", other),
        }
    }

    #[test]
    fn if_optional_capture() {
        // fn f() void { if (opt) |v| { } }
        let stmts = body_stmts(vec![
            TokenKind::Keyword(Kw::If),
            TokenKind::LParen,
            id("opt"),
            TokenKind::RParen,
            TokenKind::Pipe,
            id("v"),
            TokenKind::Pipe,
            TokenKind::LBrace,
            TokenKind::RBrace,
        ]);
        match &stmts[0] {
            Stmt::If {
                cond,
                capture,
                els,
                ..
            } => {
                assert_eq!(capture.as_deref(), Some("v"));
                assert!(matches!(cond, Expr::Ident { .. }));
                assert!(els.is_none(), "no else clause");
            }
            other => panic!("expected if with capture, got {:?}", other),
        }
    }

    #[test]
    fn if_optional_capture_with_else() {
        // fn f() void { if (opt) |v| { } else { } }
        let stmts = body_stmts(vec![
            TokenKind::Keyword(Kw::If),
            TokenKind::LParen,
            id("opt"),
            TokenKind::RParen,
            TokenKind::Pipe,
            id("v"),
            TokenKind::Pipe,
            TokenKind::LBrace,
            TokenKind::RBrace,
            TokenKind::Keyword(Kw::Else),
            TokenKind::LBrace,
            TokenKind::RBrace,
        ]);
        match &stmts[0] {
            Stmt::If {
                capture,
                els: Some(els),
                ..
            } => {
                assert_eq!(capture.as_deref(), Some("v"));
                assert!(matches!(**els, Stmt::Block(_)), "else is a block");
            }
            other => panic!("expected if-capture/else, got {:?}", other),
        }
    }

    #[test]
    fn plain_if_has_no_capture() {
        // fn f() void { if (c) { } }  — a bare boolean `if` sets `capture = None`.
        let stmts = body_stmts(vec![
            TokenKind::Keyword(Kw::If),
            TokenKind::LParen,
            id("c"),
            TokenKind::RParen,
            TokenKind::LBrace,
            TokenKind::RBrace,
        ]);
        match &stmts[0] {
            Stmt::If { capture, .. } => {
                assert!(capture.is_none(), "a plain `if (c)` has no capture");
            }
            other => panic!("expected plain if, got {:?}", other),
        }
    }

    #[test]
    fn errdefer_statement() {
        // fn f() void { errdefer print(1); }
        let stmts = body_stmts(vec![
            TokenKind::Keyword(Kw::Errdefer),
            id("print"),
            TokenKind::LParen,
            TokenKind::Int(1),
            TokenKind::RParen,
            TokenKind::Semicolon,
        ]);
        match &stmts[0] {
            Stmt::ErrDefer { stmt, .. } => match &**stmt {
                Stmt::Expr(Expr::Call { callee, .. }) => assert_eq!(callee, "print"),
                other => panic!("expected errdeferred print() call, got {:?}", other),
            },
            other => panic!("expected errdefer, got {:?}", other),
        }
    }

    #[test]
    fn errdefer_wrapping_a_block() {
        // fn f() void { errdefer { print(1); } }
        let stmts = body_stmts(vec![
            TokenKind::Keyword(Kw::Errdefer),
            TokenKind::LBrace,
            id("print"),
            TokenKind::LParen,
            TokenKind::Int(1),
            TokenKind::RParen,
            TokenKind::Semicolon,
            TokenKind::RBrace,
        ]);
        match &stmts[0] {
            Stmt::ErrDefer { stmt, .. } => match &**stmt {
                Stmt::Block(b) => {
                    assert_eq!(b.stmts.len(), 1);
                    assert!(matches!(b.stmts[0], Stmt::Expr(Expr::Call { .. })));
                }
                other => panic!("expected errdefer wrapping a block, got {:?}", other),
            },
            other => panic!("expected errdefer, got {:?}", other),
        }
    }

    /// Parse a single expression by wrapping it in `fn f() void { x = <e>; }`
    /// and returning the assignment's RHS.
    fn parse_assign_rhs(expr_kinds: Vec<TokenKind>) -> Expr {
        let mut kinds = vec![
            TokenKind::Keyword(Kw::Fn),
            id("f"),
            TokenKind::LParen,
            TokenKind::RParen,
            id("void"),
            TokenKind::LBrace,
            id("x"),
            TokenKind::Eq,
        ];
        kinds.extend(expr_kinds);
        kinds.push(TokenKind::Semicolon);
        kinds.push(TokenKind::RBrace);
        let m = parse(&toks(kinds)).expect("should parse");
        match &m.items[0] {
            Item::Func(f) => match &f.body.stmts[0] {
                Stmt::Assign { value, .. } => value.clone(),
                other => panic!("expected assign, got {:?}", other),
            },
            other => panic!("expected func, got {:?}", other),
        }
    }

    /// Like [`parse_assign_rhs`] but returns the raw parse result, so a test can
    /// assert that an ill-formed expression is *rejected*.
    fn parse_assign_rhs_result(expr_kinds: Vec<TokenKind>) -> Result<Module, Vec<Diagnostic>> {
        let mut kinds = vec![
            TokenKind::Keyword(Kw::Fn),
            id("f"),
            TokenKind::LParen,
            TokenKind::RParen,
            id("void"),
            TokenKind::LBrace,
            id("x"),
            TokenKind::Eq,
        ];
        kinds.extend(expr_kinds);
        kinds.push(TokenKind::Semicolon);
        kinds.push(TokenKind::RBrace);
        parse(&toks(kinds))
    }

    #[test]
    fn precedence_add_mul() {
        // a + b * c  ==>  (a + (b * c))
        let e = parse_assign_rhs(vec![
            id("a"),
            TokenKind::Plus,
            id("b"),
            TokenKind::Star,
            id("c"),
        ]);
        match e {
            Expr::Binary {
                op: BinOp::Add,
                rhs,
                ..
            } => match *rhs {
                Expr::Binary { op: BinOp::Mul, .. } => {}
                other => panic!("expected `*` on the right, got {:?}", other),
            },
            other => panic!("expected `+` at the root, got {:?}", other),
        }
    }

    #[test]
    fn precedence_or_and() {
        // a or b and c  ==>  (a or (b and c))
        let e = parse_assign_rhs(vec![
            id("a"),
            TokenKind::Keyword(Kw::Or),
            id("b"),
            TokenKind::Keyword(Kw::And),
            id("c"),
        ]);
        match e {
            Expr::Binary {
                op: BinOp::Or,
                rhs,
                ..
            } => match *rhs {
                Expr::Binary { op: BinOp::And, .. } => {}
                other => panic!("expected `and` on the right, got {:?}", other),
            },
            other => panic!("expected `or` at the root, got {:?}", other),
        }
    }

    #[test]
    fn syntax_error_is_reported() {
        // fn f() void { return 1 }   <-- missing `;` before `}`
        let err = parse(&toks(vec![
            TokenKind::Keyword(Kw::Fn),
            id("f"),
            TokenKind::LParen,
            TokenKind::RParen,
            id("void"),
            TokenKind::LBrace,
            TokenKind::Keyword(Kw::Return),
            TokenKind::Int(1),
            TokenKind::RBrace,
        ]))
        .expect_err("should fail");
        assert!(!err.is_empty());
        assert!(err.iter().any(|d| d.code == "E0200"));
    }

    // ---- v0.112: structs --------------------------------------------------

    #[test]
    fn struct_decl_two_fields() {
        // pub const Point = struct { x: i32, y: i32, };  (trailing comma)
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Pub),
            TokenKind::Keyword(Kw::Const),
            id("Point"),
            TokenKind::Eq,
            TokenKind::Keyword(Kw::Struct),
            TokenKind::LBrace,
            id("x"),
            TokenKind::Colon,
            id("i32"),
            TokenKind::Comma,
            id("y"),
            TokenKind::Colon,
            id("i32"),
            TokenKind::Comma,
            TokenKind::RBrace,
            TokenKind::Semicolon,
        ]))
        .expect("should parse");
        match &m.items[0] {
            Item::Struct(s) => {
                assert!(s.is_pub);
                assert_eq!(s.name, "Point");
                assert_eq!(s.fields.len(), 2);
                assert_eq!(s.fields[0].name, "x");
                assert_eq!(s.fields[0].ty.name, "i32");
                assert_eq!(s.fields[1].name, "y");
                assert_eq!(s.fields[1].ty.name, "i32");
                assert!(s.span.start < s.span.end);
            }
            other => panic!("expected struct, got {:?}", other),
        }
    }

    #[test]
    fn struct_decl_empty() {
        // const Unit = struct {};
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Const),
            id("Unit"),
            TokenKind::Eq,
            TokenKind::Keyword(Kw::Struct),
            TokenKind::LBrace,
            TokenKind::RBrace,
            TokenKind::Semicolon,
        ]))
        .expect("should parse");
        match &m.items[0] {
            Item::Struct(s) => {
                assert!(!s.is_pub);
                assert_eq!(s.name, "Unit");
                assert!(s.fields.is_empty());
            }
            other => panic!("expected empty struct, got {:?}", other),
        }
    }

    #[test]
    fn value_const_still_parses() {
        // const MAX: i64 = 10;  — the `: type` form must remain a value const.
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Const),
            id("MAX"),
            TokenKind::Colon,
            id("i64"),
            TokenKind::Eq,
            TokenKind::Int(10),
            TokenKind::Semicolon,
        ]))
        .expect("should parse");
        assert!(matches!(&m.items[0], Item::Const(c) if c.name == "MAX"));
    }

    #[test]
    fn struct_literal() {
        // x = Point{ .x = 1, .y = 2 };
        let e = parse_assign_rhs(vec![
            id("Point"),
            TokenKind::LBrace,
            TokenKind::Dot,
            id("x"),
            TokenKind::Eq,
            TokenKind::Int(1),
            TokenKind::Comma,
            TokenKind::Dot,
            id("y"),
            TokenKind::Eq,
            TokenKind::Int(2),
            TokenKind::RBrace,
        ]);
        match e {
            Expr::StructLit { name, fields, .. } => {
                assert_eq!(name, "Point");
                assert_eq!(fields.len(), 2);
                assert_eq!(fields[0].name, "x");
                assert!(matches!(fields[0].value, Expr::Int { value: 1, .. }));
                assert_eq!(fields[1].name, "y");
                assert!(matches!(fields[1].value, Expr::Int { value: 2, .. }));
            }
            other => panic!("expected struct literal, got {:?}", other),
        }
    }

    #[test]
    fn empty_struct_literal() {
        // x = Unit{};
        let e = parse_assign_rhs(vec![id("Unit"), TokenKind::LBrace, TokenKind::RBrace]);
        match e {
            Expr::StructLit { name, fields, .. } => {
                assert_eq!(name, "Unit");
                assert!(fields.is_empty());
            }
            other => panic!("expected empty struct literal, got {:?}", other),
        }
    }

    #[test]
    fn nested_field_access() {
        // x = a.b.c;  ==>  Field(Field(a, b), c)  (left-assoc)
        let e = parse_assign_rhs(vec![
            id("a"),
            TokenKind::Dot,
            id("b"),
            TokenKind::Dot,
            id("c"),
        ]);
        match e {
            Expr::Field { base, field, .. } => {
                assert_eq!(field, "c");
                match *base {
                    Expr::Field { base, field, .. } => {
                        assert_eq!(field, "b");
                        assert!(matches!(*base, Expr::Ident { ref name, .. } if name == "a"));
                    }
                    other => panic!("expected `a.b` on the left, got {:?}", other),
                }
            }
            other => panic!("expected field access at the root, got {:?}", other),
        }
    }

    #[test]
    fn field_assign_statement() {
        // fn f() void { a.b = 1; }
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Fn),
            id("f"),
            TokenKind::LParen,
            TokenKind::RParen,
            id("void"),
            TokenKind::LBrace,
            id("a"),
            TokenKind::Dot,
            id("b"),
            TokenKind::Eq,
            TokenKind::Int(1),
            TokenKind::Semicolon,
            TokenKind::RBrace,
        ]))
        .expect("should parse");
        let body = match &m.items[0] {
            Item::Func(f) => &f.body,
            other => panic!("expected func, got {:?}", other),
        };
        match &body.stmts[0] {
            Stmt::FieldAssign { place, value, .. } => {
                match place {
                    Expr::Field { base, field, .. } => {
                        assert_eq!(field, "b");
                        assert!(matches!(**base, Expr::Ident { ref name, .. } if name == "a"));
                    }
                    other => panic!("expected field place `a.b`, got {:?}", other),
                }
                assert!(matches!(value, Expr::Int { value: 1, .. }));
            }
            other => panic!("expected field assign, got {:?}", other),
        }
    }

    #[test]
    fn simple_assign_stays_assign() {
        // fn f() void { a = 1; }  — bare-name assignment is still `Stmt::Assign`.
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Fn),
            id("f"),
            TokenKind::LParen,
            TokenKind::RParen,
            id("void"),
            TokenKind::LBrace,
            id("a"),
            TokenKind::Eq,
            TokenKind::Int(1),
            TokenKind::Semicolon,
            TokenKind::RBrace,
        ]))
        .expect("should parse");
        let body = match &m.items[0] {
            Item::Func(f) => &f.body,
            other => panic!("expected func, got {:?}", other),
        };
        assert!(matches!(&body.stmts[0], Stmt::Assign { name, .. } if name == "a"));
    }

    #[test]
    fn if_while_blocks_not_struct_literals() {
        // fn f() void { if (x) { } while (y) { } }
        // The `{` after each `)` opens a block, NOT a struct literal, so neither
        // the `if` nor the `while` condition is misparsed as a struct literal.
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Fn),
            id("f"),
            TokenKind::LParen,
            TokenKind::RParen,
            id("void"),
            TokenKind::LBrace,
            TokenKind::Keyword(Kw::If),
            TokenKind::LParen,
            id("x"),
            TokenKind::RParen,
            TokenKind::LBrace,
            TokenKind::RBrace,
            TokenKind::Keyword(Kw::While),
            TokenKind::LParen,
            id("y"),
            TokenKind::RParen,
            TokenKind::LBrace,
            TokenKind::RBrace,
            TokenKind::RBrace,
        ]))
        .expect("should parse");
        let body = match &m.items[0] {
            Item::Func(f) => &f.body,
            other => panic!("expected func, got {:?}", other),
        };
        assert_eq!(body.stmts.len(), 2);
        match &body.stmts[0] {
            Stmt::If { cond, then, .. } => {
                assert!(matches!(cond, Expr::Ident { name, .. } if name == "x"));
                assert!(then.stmts.is_empty());
            }
            other => panic!("expected if, got {:?}", other),
        }
        match &body.stmts[1] {
            Stmt::While { cond, body, .. } => {
                assert!(matches!(cond, Expr::Ident { name, .. } if name == "y"));
                assert!(body.stmts.is_empty());
            }
            other => panic!("expected while, got {:?}", other),
        }
    }

    // ---- v0.113: struct methods & associated functions --------------------

    #[test]
    fn struct_with_method_and_assoc_fn() {
        // const Counter = struct {
        //     n: i32,
        //     pub fn get(self: Counter) i32 { return self.n; }
        //     pub fn zero() Counter { return Counter{ .n = 0 }; }
        // };
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Const),
            id("Counter"),
            TokenKind::Eq,
            TokenKind::Keyword(Kw::Struct),
            TokenKind::LBrace,
            // field: n: i32,
            id("n"),
            TokenKind::Colon,
            id("i32"),
            TokenKind::Comma,
            // method: pub fn get(self: Counter) i32 { return self.n; }
            TokenKind::Keyword(Kw::Pub),
            TokenKind::Keyword(Kw::Fn),
            id("get"),
            TokenKind::LParen,
            id("self"),
            TokenKind::Colon,
            id("Counter"),
            TokenKind::RParen,
            id("i32"),
            TokenKind::LBrace,
            TokenKind::Keyword(Kw::Return),
            id("self"),
            TokenKind::Dot,
            id("n"),
            TokenKind::Semicolon,
            TokenKind::RBrace,
            // assoc fn: pub fn zero() Counter { return Counter{ .n = 0 }; }
            TokenKind::Keyword(Kw::Pub),
            TokenKind::Keyword(Kw::Fn),
            id("zero"),
            TokenKind::LParen,
            TokenKind::RParen,
            id("Counter"),
            TokenKind::LBrace,
            TokenKind::Keyword(Kw::Return),
            id("Counter"),
            TokenKind::LBrace,
            TokenKind::Dot,
            id("n"),
            TokenKind::Eq,
            TokenKind::Int(0),
            TokenKind::RBrace,
            TokenKind::Semicolon,
            TokenKind::RBrace,
            // close struct
            TokenKind::RBrace,
            TokenKind::Semicolon,
        ]))
        .expect("should parse");
        match &m.items[0] {
            Item::Struct(s) => {
                assert_eq!(s.name, "Counter");
                assert_eq!(s.fields.len(), 1);
                assert_eq!(s.fields[0].name, "n");
                assert_eq!(s.methods.len(), 2);
                // method `get` — first param is `self`.
                let get = &s.methods[0];
                assert!(get.is_pub);
                assert_eq!(get.name, "get");
                assert_eq!(get.params.len(), 1);
                assert_eq!(get.params[0].name, "self");
                assert_eq!(get.params[0].ty.name, "Counter");
                assert_eq!(get.ret.name, "i32");
                assert_eq!(get.body.stmts.len(), 1);
                // associated fn `zero` — no params, no `self`.
                let zero = &s.methods[1];
                assert!(zero.is_pub);
                assert_eq!(zero.name, "zero");
                assert!(zero.params.is_empty());
                assert_eq!(zero.ret.name, "Counter");
            }
            other => panic!("expected struct with methods, got {:?}", other),
        }
    }

    #[test]
    fn struct_method_no_trailing_comma_then_method() {
        // A field with no trailing comma may still be followed by a method:
        // const Wrap = struct { v: i32 fn id(self: Wrap) i32 { return self.v; } };
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Const),
            id("Wrap"),
            TokenKind::Eq,
            TokenKind::Keyword(Kw::Struct),
            TokenKind::LBrace,
            id("v"),
            TokenKind::Colon,
            id("i32"),
            // no comma here, method follows directly
            TokenKind::Keyword(Kw::Fn),
            id("id"),
            TokenKind::LParen,
            id("self"),
            TokenKind::Colon,
            id("Wrap"),
            TokenKind::RParen,
            id("i32"),
            TokenKind::LBrace,
            TokenKind::Keyword(Kw::Return),
            id("self"),
            TokenKind::Dot,
            id("v"),
            TokenKind::Semicolon,
            TokenKind::RBrace,
            TokenKind::RBrace,
            TokenKind::Semicolon,
        ]))
        .expect("should parse");
        match &m.items[0] {
            Item::Struct(s) => {
                assert_eq!(s.fields.len(), 1);
                assert_eq!(s.methods.len(), 1);
                assert_eq!(s.methods[0].name, "id");
                assert!(!s.methods[0].is_pub);
            }
            other => panic!("expected struct, got {:?}", other),
        }
    }

    #[test]
    fn struct_decl_two_fields_still_sets_empty_methods() {
        // A v0.112-style struct with only fields must now carry an empty methods
        // list (regression guard for the new field).
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Const),
            id("Point"),
            TokenKind::Eq,
            TokenKind::Keyword(Kw::Struct),
            TokenKind::LBrace,
            id("x"),
            TokenKind::Colon,
            id("i32"),
            TokenKind::Comma,
            id("y"),
            TokenKind::Colon,
            id("i32"),
            TokenKind::RBrace,
            TokenKind::Semicolon,
        ]))
        .expect("should parse");
        match &m.items[0] {
            Item::Struct(s) => {
                assert_eq!(s.fields.len(), 2);
                assert!(s.methods.is_empty());
            }
            other => panic!("expected struct, got {:?}", other),
        }
    }

    #[test]
    fn method_call_with_arg() {
        // x = c.bumped(1);  ==>  MethodCall { receiver: c, method: bumped, args: [1] }
        let e = parse_assign_rhs(vec![
            id("c"),
            TokenKind::Dot,
            id("bumped"),
            TokenKind::LParen,
            TokenKind::Int(1),
            TokenKind::RParen,
        ]);
        match e {
            Expr::MethodCall {
                receiver,
                method,
                args,
                ..
            } => {
                assert_eq!(method, "bumped");
                assert!(matches!(*receiver, Expr::Ident { ref name, .. } if name == "c"));
                assert_eq!(args.len(), 1);
                assert!(matches!(args[0], Expr::Int { value: 1, .. }));
            }
            other => panic!("expected method call, got {:?}", other),
        }
    }

    #[test]
    fn associated_call_no_args() {
        // x = Counter.zero();  ==>  MethodCall { receiver: Counter, method: zero, args: [] }
        let e = parse_assign_rhs(vec![
            id("Counter"),
            TokenKind::Dot,
            id("zero"),
            TokenKind::LParen,
            TokenKind::RParen,
        ]);
        match e {
            Expr::MethodCall {
                receiver,
                method,
                args,
                ..
            } => {
                assert_eq!(method, "zero");
                assert!(matches!(*receiver, Expr::Ident { ref name, .. } if name == "Counter"));
                assert!(args.is_empty());
            }
            other => panic!("expected associated call, got {:?}", other),
        }
    }

    #[test]
    fn field_access_without_parens_stays_field() {
        // x = a.b;  ==>  Field, NOT MethodCall (no `(` after `.b`).
        let e = parse_assign_rhs(vec![id("a"), TokenKind::Dot, id("b")]);
        match e {
            Expr::Field { base, field, .. } => {
                assert_eq!(field, "b");
                assert!(matches!(*base, Expr::Ident { ref name, .. } if name == "a"));
            }
            other => panic!("expected field access, got {:?}", other),
        }
    }

    #[test]
    fn method_call_then_field_chain() {
        // x = a.m().n;  ==>  Field { base: MethodCall { a, m }, field: n }  (left-assoc)
        let e = parse_assign_rhs(vec![
            id("a"),
            TokenKind::Dot,
            id("m"),
            TokenKind::LParen,
            TokenKind::RParen,
            TokenKind::Dot,
            id("n"),
        ]);
        match e {
            Expr::Field { base, field, .. } => {
                assert_eq!(field, "n");
                match *base {
                    Expr::MethodCall {
                        receiver, method, ..
                    } => {
                        assert_eq!(method, "m");
                        assert!(matches!(*receiver, Expr::Ident { ref name, .. } if name == "a"));
                    }
                    other => panic!("expected `a.m()` on the left, got {:?}", other),
                }
            }
            other => panic!("expected field-of-method-call, got {:?}", other),
        }
    }

    #[test]
    fn method_call_statement() {
        // fn f() void { x.tick(); }  — a method call used as an expr statement.
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Fn),
            id("f"),
            TokenKind::LParen,
            TokenKind::RParen,
            id("void"),
            TokenKind::LBrace,
            id("x"),
            TokenKind::Dot,
            id("tick"),
            TokenKind::LParen,
            TokenKind::RParen,
            TokenKind::Semicolon,
            TokenKind::RBrace,
        ]))
        .expect("should parse");
        let body = match &m.items[0] {
            Item::Func(f) => &f.body,
            other => panic!("expected func, got {:?}", other),
        };
        match &body.stmts[0] {
            Stmt::Expr(Expr::MethodCall { method, .. }) => assert_eq!(method, "tick"),
            other => panic!("expected method-call statement, got {:?}", other),
        }
    }

    // ---- v0.114: optionals (`?T`, `null`, `orelse`, `.?`) -----------------

    #[test]
    fn optional_param_type() {
        // fn f(a: ?i32) void { }  — the `?` marks the param type optional.
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Fn),
            id("f"),
            TokenKind::LParen,
            id("a"),
            TokenKind::Colon,
            TokenKind::Question,
            id("i32"),
            TokenKind::RParen,
            id("void"),
            TokenKind::LBrace,
            TokenKind::RBrace,
        ]))
        .expect("should parse");
        match &m.items[0] {
            Item::Func(f) => {
                assert_eq!(f.params.len(), 1);
                assert_eq!(f.params[0].ty.name, "i32");
                assert!(f.params[0].ty.optional, "`?i32` param must be optional");
                // The non-optional return type stays optional = false.
                assert_eq!(f.ret.name, "void");
                assert!(!f.ret.optional);
            }
            other => panic!("expected func, got {:?}", other),
        }
    }

    #[test]
    fn optional_struct_local_type() {
        // fn f() void { var x: ?Point = null; }  — `?Point` local is optional,
        // and its initializer is the `null` literal.
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Fn),
            id("f"),
            TokenKind::LParen,
            TokenKind::RParen,
            id("void"),
            TokenKind::LBrace,
            TokenKind::Keyword(Kw::Var),
            id("x"),
            TokenKind::Colon,
            TokenKind::Question,
            id("Point"),
            TokenKind::Eq,
            TokenKind::Keyword(Kw::Null),
            TokenKind::Semicolon,
            TokenKind::RBrace,
        ]))
        .expect("should parse");
        let body = match &m.items[0] {
            Item::Func(f) => &f.body,
            other => panic!("expected func, got {:?}", other),
        };
        match &body.stmts[0] {
            Stmt::Let { name, ty, value, .. } => {
                assert_eq!(name, "x");
                let ty = ty.as_ref().expect("annotated local carries Some(ty)");
                assert_eq!(ty.name, "Point");
                assert!(ty.optional, "`?Point` local must be optional");
                assert!(matches!(value, Expr::Null { .. }));
            }
            other => panic!("expected let, got {:?}", other),
        }
    }

    #[test]
    fn non_optional_type_has_optional_false() {
        // fn f(a: i32) i32 { return a; }  — no `?`, so both types are non-optional.
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Fn),
            id("f"),
            TokenKind::LParen,
            id("a"),
            TokenKind::Colon,
            id("i32"),
            TokenKind::RParen,
            id("i32"),
            TokenKind::LBrace,
            TokenKind::Keyword(Kw::Return),
            id("a"),
            TokenKind::Semicolon,
            TokenKind::RBrace,
        ]))
        .expect("should parse");
        match &m.items[0] {
            Item::Func(f) => {
                assert!(!f.params[0].ty.optional);
                assert!(!f.ret.optional);
            }
            other => panic!("expected func, got {:?}", other),
        }
    }

    #[test]
    fn null_literal() {
        // x = null;
        let e = parse_assign_rhs(vec![TokenKind::Keyword(Kw::Null)]);
        assert!(matches!(e, Expr::Null { .. }), "expected null, got {:?}", e);
    }

    #[test]
    fn orelse_expression() {
        // x = y orelse 0;  ==>  Orelse { lhs: Ident y, rhs: Int 0 }
        let e = parse_assign_rhs(vec![
            id("y"),
            TokenKind::Keyword(Kw::Orelse),
            TokenKind::Int(0),
        ]);
        match e {
            Expr::Orelse { lhs, rhs, .. } => {
                assert!(matches!(*lhs, Expr::Ident { ref name, .. } if name == "y"));
                assert!(matches!(*rhs, Expr::Int { value: 0, .. }));
            }
            other => panic!("expected orelse, got {:?}", other),
        }
    }

    #[test]
    fn orelse_is_left_associative() {
        // a orelse b orelse c  ==>  ((a orelse b) orelse c)
        let e = parse_assign_rhs(vec![
            id("a"),
            TokenKind::Keyword(Kw::Orelse),
            id("b"),
            TokenKind::Keyword(Kw::Orelse),
            id("c"),
        ]);
        match e {
            Expr::Orelse { lhs, rhs, .. } => {
                assert!(matches!(*rhs, Expr::Ident { ref name, .. } if name == "c"));
                assert!(matches!(*lhs, Expr::Orelse { .. }), "left operand should nest");
            }
            other => panic!("expected orelse at the root, got {:?}", other),
        }
    }

    #[test]
    fn orelse_binds_looser_than_or() {
        // a or b orelse c  ==>  ((a or b) orelse c)  — `orelse` is the loosest.
        let e = parse_assign_rhs(vec![
            id("a"),
            TokenKind::Keyword(Kw::Or),
            id("b"),
            TokenKind::Keyword(Kw::Orelse),
            id("c"),
        ]);
        match e {
            Expr::Orelse { lhs, rhs, .. } => {
                assert!(matches!(*rhs, Expr::Ident { ref name, .. } if name == "c"));
                assert!(
                    matches!(*lhs, Expr::Binary { op: BinOp::Or, .. }),
                    "left operand should be the `or`, got {:?}",
                    lhs
                );
            }
            other => panic!("expected orelse at the root, got {:?}", other),
        }
    }

    #[test]
    fn unwrap_postfix() {
        // x = y.?;  ==>  Unwrap { expr: Ident y }
        let e = parse_assign_rhs(vec![id("y"), TokenKind::Dot, TokenKind::Question]);
        match e {
            Expr::Unwrap { expr, .. } => {
                assert!(matches!(*expr, Expr::Ident { ref name, .. } if name == "y"));
            }
            other => panic!("expected unwrap, got {:?}", other),
        }
    }

    #[test]
    fn unwrap_composes_with_field_access() {
        // x = a.b.?;  ==>  Unwrap { expr: Field { base: Ident a, field: b } }
        let e = parse_assign_rhs(vec![
            id("a"),
            TokenKind::Dot,
            id("b"),
            TokenKind::Dot,
            TokenKind::Question,
        ]);
        match e {
            Expr::Unwrap { expr, .. } => match *expr {
                Expr::Field { base, field, .. } => {
                    assert_eq!(field, "b");
                    assert!(matches!(*base, Expr::Ident { ref name, .. } if name == "a"));
                }
                other => panic!("expected `a.b` inside the unwrap, got {:?}", other),
            },
            other => panic!("expected unwrap at the root, got {:?}", other),
        }
    }

    #[test]
    fn unwrap_composes_with_call() {
        // x = f().?;  ==>  Unwrap { expr: Call f }
        let e = parse_assign_rhs(vec![
            id("f"),
            TokenKind::LParen,
            TokenKind::RParen,
            TokenKind::Dot,
            TokenKind::Question,
        ]);
        match e {
            Expr::Unwrap { expr, .. } => {
                assert!(matches!(*expr, Expr::Call { ref callee, .. } if callee == "f"));
            }
            other => panic!("expected unwrap of a call, got {:?}", other),
        }
    }

    #[test]
    fn unwrap_then_field_chain() {
        // x = a.?.b;  ==>  Field { base: Unwrap { expr: a }, field: b }
        let e = parse_assign_rhs(vec![
            id("a"),
            TokenKind::Dot,
            TokenKind::Question,
            TokenKind::Dot,
            id("b"),
        ]);
        match e {
            Expr::Field { base, field, .. } => {
                assert_eq!(field, "b");
                assert!(matches!(*base, Expr::Unwrap { .. }));
            }
            other => panic!("expected field-of-unwrap, got {:?}", other),
        }
    }

    #[test]
    fn unwrap_then_orelse() {
        // x = a.? orelse b;  ==>  Orelse { lhs: Unwrap { a }, rhs: Ident b }
        // `.?` is postfix (tightest), `orelse` the loosest, so the unwrap is the
        // left operand of the `orelse`.
        let e = parse_assign_rhs(vec![
            id("a"),
            TokenKind::Dot,
            TokenKind::Question,
            TokenKind::Keyword(Kw::Orelse),
            id("b"),
        ]);
        match e {
            Expr::Orelse { lhs, rhs, .. } => {
                assert!(matches!(*lhs, Expr::Unwrap { .. }));
                assert!(matches!(*rhs, Expr::Ident { ref name, .. } if name == "b"));
            }
            other => panic!("expected orelse at the root, got {:?}", other),
        }
    }

    // ---- v0.115: error unions (`!T`, `error.Name`, `try`, `catch`) --------

    #[test]
    fn error_union_return_type() {
        // fn f() !i32 { return 0; }  — the `!` marks the return type an error
        // union; `optional` stays false (the two prefixes are exclusive).
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Fn),
            id("f"),
            TokenKind::LParen,
            TokenKind::RParen,
            TokenKind::Bang,
            id("i32"),
            TokenKind::LBrace,
            TokenKind::Keyword(Kw::Return),
            TokenKind::Int(0),
            TokenKind::Semicolon,
            TokenKind::RBrace,
        ]))
        .expect("should parse");
        match &m.items[0] {
            Item::Func(f) => {
                assert_eq!(f.ret.name, "i32");
                assert!(f.ret.error_union, "`!i32` return must be an error union");
                assert!(!f.ret.optional, "`!i32` must not also be optional");
                assert!(f.ret.span.start < f.ret.span.end);
            }
            other => panic!("expected func, got {:?}", other),
        }
    }

    #[test]
    fn error_union_param_and_local_types() {
        // fn f(a: !i32) void { var x: !bool = true; }  — `!T` works in param and
        // local positions too, each through the shared `parse_type`.
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Fn),
            id("f"),
            TokenKind::LParen,
            id("a"),
            TokenKind::Colon,
            TokenKind::Bang,
            id("i32"),
            TokenKind::RParen,
            id("void"),
            TokenKind::LBrace,
            TokenKind::Keyword(Kw::Var),
            id("x"),
            TokenKind::Colon,
            TokenKind::Bang,
            id("bool"),
            TokenKind::Eq,
            TokenKind::Keyword(Kw::True),
            TokenKind::Semicolon,
            TokenKind::RBrace,
        ]))
        .expect("should parse");
        match &m.items[0] {
            Item::Func(f) => {
                assert!(f.params[0].ty.error_union, "`!i32` param must be an error union");
                assert!(!f.params[0].ty.optional);
                match &f.body.stmts[0] {
                    Stmt::Let { ty, .. } => {
                        let ty = ty.as_ref().expect("annotated local carries Some(ty)");
                        assert_eq!(ty.name, "bool");
                        assert!(ty.error_union, "`!bool` local must be an error union");
                        assert!(!ty.optional);
                    }
                    other => panic!("expected let, got {:?}", other),
                }
            }
            other => panic!("expected func, got {:?}", other),
        }
    }

    #[test]
    fn optional_and_error_union_are_mutually_exclusive() {
        // `?i32` is optional-not-error; `!i32` is error-not-optional. A type is
        // never flagged as both (SPEC §12.1).
        fn ret_ty(prefix: TokenKind) -> TypeExpr {
            let m = parse(&toks(vec![
                TokenKind::Keyword(Kw::Fn),
                id("f"),
                TokenKind::LParen,
                TokenKind::RParen,
                prefix,
                id("i32"),
                TokenKind::LBrace,
                TokenKind::Keyword(Kw::Return),
                TokenKind::Int(0),
                TokenKind::Semicolon,
                TokenKind::RBrace,
            ]))
            .expect("should parse");
            match &m.items[0] {
                Item::Func(f) => f.ret.clone(),
                other => panic!("expected func, got {:?}", other),
            }
        }
        let q = ret_ty(TokenKind::Question);
        assert!(q.optional && !q.error_union, "`?i32` is optional only");
        let bang = ret_ty(TokenKind::Bang);
        assert!(bang.error_union && !bang.optional, "`!i32` is error-union only");
    }

    #[test]
    fn bare_type_has_neither_flag() {
        // A plain `i32` is neither optional nor an error union (regression guard
        // for the new `error_union` field).
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Fn),
            id("f"),
            TokenKind::LParen,
            TokenKind::RParen,
            id("i32"),
            TokenKind::LBrace,
            TokenKind::Keyword(Kw::Return),
            TokenKind::Int(0),
            TokenKind::Semicolon,
            TokenKind::RBrace,
        ]))
        .expect("should parse");
        match &m.items[0] {
            Item::Func(f) => {
                assert!(!f.ret.optional);
                assert!(!f.ret.error_union);
            }
            other => panic!("expected func, got {:?}", other),
        }
    }

    #[test]
    fn double_type_prefix_is_rejected() {
        // `?!i32` is not a valid type: after `?` the parser requires an
        // identifier and finds `!`, so it reports E0200 (mutual exclusion).
        let err = parse(&toks(vec![
            TokenKind::Keyword(Kw::Fn),
            id("f"),
            TokenKind::LParen,
            TokenKind::RParen,
            TokenKind::Question,
            TokenKind::Bang,
            id("i32"),
            TokenKind::LBrace,
            TokenKind::RBrace,
        ]))
        .expect_err("`?!i32` should fail");
        assert!(err.iter().any(|d| d.code == "E0200"));
    }

    #[test]
    fn error_literal() {
        // x = error.Oops;  ==>  ErrorLit { name: "Oops" }
        let e = parse_assign_rhs(vec![
            TokenKind::Keyword(Kw::Error),
            TokenKind::Dot,
            id("Oops"),
        ]);
        match e {
            Expr::ErrorLit { name, span } => {
                assert_eq!(name, "Oops");
                assert!(span.start < span.end, "span should cover `error.Oops`");
            }
            other => panic!("expected error literal, got {:?}", other),
        }
    }

    #[test]
    fn error_without_dot_is_rejected() {
        // `error 0` (no `.`) is a syntax error: `error` must be followed by `.`.
        let e = parse_assign_rhs_result(vec![TokenKind::Keyword(Kw::Error), TokenKind::Int(0)]);
        let err = e.expect_err("`error 0` should fail");
        assert!(err.iter().any(|d| d.code == "E0200"));
    }

    #[test]
    fn try_prefix_on_call() {
        // x = try f();  ==>  Try { expr: Call f }
        let e = parse_assign_rhs(vec![
            TokenKind::Keyword(Kw::Try),
            id("f"),
            TokenKind::LParen,
            TokenKind::RParen,
        ]);
        match e {
            Expr::Try { expr, .. } => {
                assert!(matches!(*expr, Expr::Call { ref callee, .. } if callee == "f"));
            }
            other => panic!("expected try, got {:?}", other),
        }
    }

    #[test]
    fn try_as_return_value() {
        // fn f() void { return try g(); }  — `try` parses as the return value
        // (statement-position legality is checked later in sema).
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Fn),
            id("f"),
            TokenKind::LParen,
            TokenKind::RParen,
            id("void"),
            TokenKind::LBrace,
            TokenKind::Keyword(Kw::Return),
            TokenKind::Keyword(Kw::Try),
            id("g"),
            TokenKind::LParen,
            TokenKind::RParen,
            TokenKind::Semicolon,
            TokenKind::RBrace,
        ]))
        .expect("should parse");
        let body = match &m.items[0] {
            Item::Func(f) => &f.body,
            other => panic!("expected func, got {:?}", other),
        };
        match &body.stmts[0] {
            Stmt::Return {
                value: Some(Expr::Try { expr, .. }),
                ..
            } => {
                assert!(matches!(**expr, Expr::Call { ref callee, .. } if callee == "g"));
            }
            other => panic!("expected `return try g();`, got {:?}", other),
        }
    }

    #[test]
    fn catch_expression() {
        // x = g() catch 0;  ==>  Catch { expr: Call g, default: Int 0 }
        let e = parse_assign_rhs(vec![
            id("g"),
            TokenKind::LParen,
            TokenKind::RParen,
            TokenKind::Keyword(Kw::Catch),
            TokenKind::Int(0),
        ]);
        match e {
            Expr::Catch { expr, default, .. } => {
                assert!(matches!(*expr, Expr::Call { ref callee, .. } if callee == "g"));
                assert!(matches!(*default, Expr::Int { value: 0, .. }));
            }
            other => panic!("expected catch, got {:?}", other),
        }
    }

    #[test]
    fn catch_is_left_associative() {
        // a catch b catch c  ==>  ((a catch b) catch c)
        let e = parse_assign_rhs(vec![
            id("a"),
            TokenKind::Keyword(Kw::Catch),
            id("b"),
            TokenKind::Keyword(Kw::Catch),
            id("c"),
        ]);
        match e {
            Expr::Catch { expr, default, .. } => {
                assert!(matches!(*default, Expr::Ident { ref name, .. } if name == "c"));
                assert!(matches!(*expr, Expr::Catch { .. }), "left operand should nest");
            }
            other => panic!("expected catch at the root, got {:?}", other),
        }
    }

    #[test]
    fn catch_shares_level_with_orelse() {
        // a orelse b catch c  ==>  ((a orelse b) catch c)  — `catch` and `orelse`
        // share the lowest precedence level and are left-associative.
        let e = parse_assign_rhs(vec![
            id("a"),
            TokenKind::Keyword(Kw::Orelse),
            id("b"),
            TokenKind::Keyword(Kw::Catch),
            id("c"),
        ]);
        match e {
            Expr::Catch { expr, default, .. } => {
                assert!(matches!(*default, Expr::Ident { ref name, .. } if name == "c"));
                assert!(
                    matches!(*expr, Expr::Orelse { .. }),
                    "left operand should be the `orelse`, got {:?}",
                    expr
                );
            }
            other => panic!("expected catch at the root, got {:?}", other),
        }
    }

    #[test]
    fn catch_binds_looser_than_or() {
        // a or b catch c  ==>  ((a or b) catch c)  — `catch` is among the loosest.
        let e = parse_assign_rhs(vec![
            id("a"),
            TokenKind::Keyword(Kw::Or),
            id("b"),
            TokenKind::Keyword(Kw::Catch),
            id("c"),
        ]);
        match e {
            Expr::Catch { expr, .. } => {
                assert!(
                    matches!(*expr, Expr::Binary { op: BinOp::Or, .. }),
                    "left operand should be the `or`, got {:?}",
                    expr
                );
            }
            other => panic!("expected catch at the root, got {:?}", other),
        }
    }

    // ---- v0.116: enums & switch -------------------------------------------

    /// Wrap a sequence of statement tokens inside `fn f() void { ... }` and
    /// return the parsed function body (so `switch` statements can be exercised
    /// in their natural statement position).
    fn parse_fn_body(stmt_kinds: Vec<TokenKind>) -> Block {
        let mut kinds = vec![
            TokenKind::Keyword(Kw::Fn),
            id("f"),
            TokenKind::LParen,
            TokenKind::RParen,
            id("void"),
            TokenKind::LBrace,
        ];
        kinds.extend(stmt_kinds);
        kinds.push(TokenKind::RBrace);
        let m = parse(&toks(kinds)).expect("should parse");
        match &m.items[0] {
            Item::Func(f) => f.body.clone(),
            other => panic!("expected func, got {:?}", other),
        }
    }

    #[test]
    fn enum_decl_three_variants() {
        // pub const Color = enum { Red, Green, Blue };
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Pub),
            TokenKind::Keyword(Kw::Const),
            id("Color"),
            TokenKind::Eq,
            TokenKind::Keyword(Kw::Enum),
            TokenKind::LBrace,
            id("Red"),
            TokenKind::Comma,
            id("Green"),
            TokenKind::Comma,
            id("Blue"),
            TokenKind::RBrace,
            TokenKind::Semicolon,
        ]))
        .expect("should parse");
        match &m.items[0] {
            Item::Enum(e) => {
                assert!(e.is_pub);
                assert_eq!(e.name, "Color");
                assert_eq!(e.variants, vec!["Red", "Green", "Blue"]);
                assert!(e.span.start < e.span.end);
            }
            other => panic!("expected enum, got {:?}", other),
        }
    }

    #[test]
    fn enum_decl_trailing_comma() {
        // const E = enum { A, };  — a single variant with a trailing comma.
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Const),
            id("E"),
            TokenKind::Eq,
            TokenKind::Keyword(Kw::Enum),
            TokenKind::LBrace,
            id("A"),
            TokenKind::Comma,
            TokenKind::RBrace,
            TokenKind::Semicolon,
        ]))
        .expect("should parse");
        match &m.items[0] {
            Item::Enum(e) => {
                assert!(!e.is_pub);
                assert_eq!(e.name, "E");
                assert_eq!(e.variants, vec!["A"]);
            }
            other => panic!("expected enum, got {:?}", other),
        }
    }

    #[test]
    fn struct_decl_still_parses_alongside_enum_branch() {
        // The `= struct {...}` branch must still parse after the `= enum`
        // dispatch is added (regression guard for the const-item dispatch).
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Const),
            id("Point"),
            TokenKind::Eq,
            TokenKind::Keyword(Kw::Struct),
            TokenKind::LBrace,
            id("x"),
            TokenKind::Colon,
            id("i32"),
            TokenKind::RBrace,
            TokenKind::Semicolon,
        ]))
        .expect("should parse");
        assert!(matches!(&m.items[0], Item::Struct(s) if s.name == "Point"));
    }

    #[test]
    fn enum_lit_unqualified() {
        // x = .Red;  ==>  EnumLit { variant: "Red" }
        let e = parse_assign_rhs(vec![TokenKind::Dot, id("Red")]);
        match e {
            Expr::EnumLit { variant, span } => {
                assert_eq!(variant, "Red");
                assert!(span.start < span.end, "span should cover `.Red`");
            }
            other => panic!("expected enum literal, got {:?}", other),
        }
    }

    #[test]
    fn enum_lit_qualified_is_field() {
        // x = Color.Red;  ==>  Field { base: Ident Color, field: "Red" } — the
        // qualified form reuses `Expr::Field`, with no new parser code.
        let e = parse_assign_rhs(vec![id("Color"), TokenKind::Dot, id("Red")]);
        match e {
            Expr::Field { base, field, .. } => {
                assert_eq!(field, "Red");
                assert!(matches!(*base, Expr::Ident { ref name, .. } if name == "Color"));
            }
            other => panic!("expected field access, got {:?}", other),
        }
    }

    #[test]
    fn enum_lit_does_not_break_postfix_field() {
        // x = a.b;  must still be postfix `Field`, NOT a leading-dot EnumLit —
        // the leading-`.` rule only fires at the *start* of a primary.
        let e = parse_assign_rhs(vec![id("a"), TokenKind::Dot, id("b")]);
        assert!(
            matches!(e, Expr::Field { ref field, .. } if field == "b"),
            "expected postfix field, got {:?}",
            e
        );
    }

    #[test]
    fn switch_over_enum_with_else() {
        // switch (c) { .Red => { return; }, .Green => { return; }, else => { return; } }
        let body = parse_fn_body(vec![
            TokenKind::Keyword(Kw::Switch),
            TokenKind::LParen,
            id("c"),
            TokenKind::RParen,
            TokenKind::LBrace,
            // .Red => { return; },
            TokenKind::Dot,
            id("Red"),
            TokenKind::FatArrow,
            TokenKind::LBrace,
            TokenKind::Keyword(Kw::Return),
            TokenKind::Semicolon,
            TokenKind::RBrace,
            TokenKind::Comma,
            // .Green => { return; },
            TokenKind::Dot,
            id("Green"),
            TokenKind::FatArrow,
            TokenKind::LBrace,
            TokenKind::Keyword(Kw::Return),
            TokenKind::Semicolon,
            TokenKind::RBrace,
            TokenKind::Comma,
            // else => { return; }
            TokenKind::Keyword(Kw::Else),
            TokenKind::FatArrow,
            TokenKind::LBrace,
            TokenKind::Keyword(Kw::Return),
            TokenKind::Semicolon,
            TokenKind::RBrace,
            TokenKind::RBrace, // close switch
        ]);
        match &body.stmts[0] {
            Stmt::Switch {
                scrutinee,
                arms,
                default,
                span,
            } => {
                assert!(matches!(scrutinee, Expr::Ident { name, .. } if name == "c"));
                assert_eq!(arms.len(), 2);
                assert_eq!(arms[0].labels.len(), 1);
                assert!(matches!(&arms[0].labels[0], Expr::EnumLit { variant, .. } if variant == "Red"));
                assert!(matches!(&arms[1].labels[0], Expr::EnumLit { variant, .. } if variant == "Green"));
                assert!(default.is_some(), "the `else` arm should set `default`");
                assert!(span.start < span.end);
            }
            other => panic!("expected switch, got {:?}", other),
        }
    }

    #[test]
    fn switch_multi_label_arm() {
        // switch (c) { .A, .B => { } else => { } }  — one arm with two labels;
        // no comma between the block and `else` (the separator is optional).
        let body = parse_fn_body(vec![
            TokenKind::Keyword(Kw::Switch),
            TokenKind::LParen,
            id("c"),
            TokenKind::RParen,
            TokenKind::LBrace,
            // .A, .B => { }
            TokenKind::Dot,
            id("A"),
            TokenKind::Comma,
            TokenKind::Dot,
            id("B"),
            TokenKind::FatArrow,
            TokenKind::LBrace,
            TokenKind::RBrace,
            // else => { }
            TokenKind::Keyword(Kw::Else),
            TokenKind::FatArrow,
            TokenKind::LBrace,
            TokenKind::RBrace,
            TokenKind::RBrace, // close switch
        ]);
        match &body.stmts[0] {
            Stmt::Switch { arms, default, .. } => {
                assert_eq!(arms.len(), 1);
                assert_eq!(arms[0].labels.len(), 2);
                assert!(matches!(&arms[0].labels[0], Expr::EnumLit { variant, .. } if variant == "A"));
                assert!(matches!(&arms[0].labels[1], Expr::EnumLit { variant, .. } if variant == "B"));
                assert!(default.is_some());
            }
            other => panic!("expected switch, got {:?}", other),
        }
    }

    #[test]
    fn switch_on_int_with_else() {
        // switch (n) { 1 => { } 2 => { } else => { } }  — integer scrutinee,
        // integer-literal labels, no commas between arms (separator optional).
        let body = parse_fn_body(vec![
            TokenKind::Keyword(Kw::Switch),
            TokenKind::LParen,
            id("n"),
            TokenKind::RParen,
            TokenKind::LBrace,
            TokenKind::Int(1),
            TokenKind::FatArrow,
            TokenKind::LBrace,
            TokenKind::RBrace,
            TokenKind::Int(2),
            TokenKind::FatArrow,
            TokenKind::LBrace,
            TokenKind::RBrace,
            TokenKind::Keyword(Kw::Else),
            TokenKind::FatArrow,
            TokenKind::LBrace,
            TokenKind::RBrace,
            TokenKind::RBrace, // close switch
        ]);
        match &body.stmts[0] {
            Stmt::Switch {
                scrutinee,
                arms,
                default,
                ..
            } => {
                assert!(matches!(scrutinee, Expr::Ident { name, .. } if name == "n"));
                assert_eq!(arms.len(), 2);
                assert!(matches!(&arms[0].labels[0], Expr::Int { value: 1, .. }));
                assert!(matches!(&arms[1].labels[0], Expr::Int { value: 2, .. }));
                assert!(default.is_some(), "an int switch needs an `else`");
            }
            other => panic!("expected switch, got {:?}", other),
        }
    }

    #[test]
    fn switch_qualified_enum_labels() {
        // switch (c) { Color.Red => { } else => { } }  — a qualified `Enum.V`
        // label parses as `Expr::Field`, and the leading-dot rule does not
        // interfere with it.
        let body = parse_fn_body(vec![
            TokenKind::Keyword(Kw::Switch),
            TokenKind::LParen,
            id("c"),
            TokenKind::RParen,
            TokenKind::LBrace,
            id("Color"),
            TokenKind::Dot,
            id("Red"),
            TokenKind::FatArrow,
            TokenKind::LBrace,
            TokenKind::RBrace,
            TokenKind::Keyword(Kw::Else),
            TokenKind::FatArrow,
            TokenKind::LBrace,
            TokenKind::RBrace,
            TokenKind::RBrace, // close switch
        ]);
        match &body.stmts[0] {
            Stmt::Switch { arms, .. } => {
                assert_eq!(arms.len(), 1);
                match &arms[0].labels[0] {
                    Expr::Field { base, field, .. } => {
                        assert_eq!(field, "Red");
                        assert!(matches!(**base, Expr::Ident { ref name, .. } if name == "Color"));
                    }
                    other => panic!("expected `Color.Red` field label, got {:?}", other),
                }
            }
            other => panic!("expected switch, got {:?}", other),
        }
    }

    // ---- v0.117: fixed-size arrays (`[N]T`) -------------------------------

    #[test]
    fn array_type_in_param() {
        // fn f(a: [3]i32) void { }  — `[3]i32` sets array_len=Some(3), name=i32.
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Fn),
            id("f"),
            TokenKind::LParen,
            id("a"),
            TokenKind::Colon,
            TokenKind::LBracket,
            TokenKind::Int(3),
            TokenKind::RBracket,
            id("i32"),
            TokenKind::RParen,
            id("void"),
            TokenKind::LBrace,
            TokenKind::RBrace,
        ]))
        .expect("should parse");
        match &m.items[0] {
            Item::Func(f) => {
                assert_eq!(f.params.len(), 1);
                let ty = &f.params[0].ty;
                assert_eq!(ty.name, "i32");
                assert_eq!(ty.array_len, Some(3), "`[3]i32` must set array_len");
                assert!(!ty.optional, "an array type is not optional");
                assert!(!ty.error_union, "an array type is not an error union");
                assert!(ty.span.start < ty.span.end, "span should cover `[3]i32`");
                // The non-array return type carries `array_len = None`.
                assert_eq!(f.ret.name, "void");
                assert_eq!(f.ret.array_len, None);
            }
            other => panic!("expected func, got {:?}", other),
        }
    }

    #[test]
    fn non_array_type_has_array_len_none() {
        // A plain `i32` carries `array_len = None` (regression guard for the new
        // field — every TypeExpr construction must set it).
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Fn),
            id("f"),
            TokenKind::LParen,
            id("a"),
            TokenKind::Colon,
            id("i32"),
            TokenKind::RParen,
            id("i32"),
            TokenKind::LBrace,
            TokenKind::Keyword(Kw::Return),
            id("a"),
            TokenKind::Semicolon,
            TokenKind::RBrace,
        ]))
        .expect("should parse");
        match &m.items[0] {
            Item::Func(f) => {
                assert_eq!(f.params[0].ty.array_len, None);
                assert_eq!(f.ret.array_len, None);
            }
            other => panic!("expected func, got {:?}", other),
        }
    }

    #[test]
    fn array_local_type() {
        // fn f() void { var a: [2]i32 = [2]i32{ 1, 2 }; }  — `[2]i32` local type
        // plus an array-literal initializer.
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Fn),
            id("f"),
            TokenKind::LParen,
            TokenKind::RParen,
            id("void"),
            TokenKind::LBrace,
            TokenKind::Keyword(Kw::Var),
            id("a"),
            TokenKind::Colon,
            TokenKind::LBracket,
            TokenKind::Int(2),
            TokenKind::RBracket,
            id("i32"),
            TokenKind::Eq,
            TokenKind::LBracket,
            TokenKind::Int(2),
            TokenKind::RBracket,
            id("i32"),
            TokenKind::LBrace,
            TokenKind::Int(1),
            TokenKind::Comma,
            TokenKind::Int(2),
            TokenKind::RBrace,
            TokenKind::Semicolon,
            TokenKind::RBrace,
        ]))
        .expect("should parse");
        let body = match &m.items[0] {
            Item::Func(f) => &f.body,
            other => panic!("expected func, got {:?}", other),
        };
        match &body.stmts[0] {
            Stmt::Let { name, ty, value, .. } => {
                assert_eq!(name, "a");
                let ty = ty.as_ref().expect("annotated local carries Some(ty)");
                assert_eq!(ty.name, "i32");
                assert_eq!(ty.array_len, Some(2));
                match value {
                    Expr::ArrayLit { elem, elems, .. } => {
                        assert_eq!(elem.name, "i32");
                        assert_eq!(elem.array_len, Some(2));
                        assert_eq!(elems.len(), 2);
                        assert!(matches!(elems[0], Expr::Int { value: 1, .. }));
                        assert!(matches!(elems[1], Expr::Int { value: 2, .. }));
                    }
                    other => panic!("expected array literal, got {:?}", other),
                }
            }
            other => panic!("expected let, got {:?}", other),
        }
    }

    #[test]
    fn array_literal_three_elems() {
        // x = [3]i32{ 1, 2, 3 };  ==>  ArrayLit { elem: [3]i32, elems: [1,2,3] }
        let e = parse_assign_rhs(vec![
            TokenKind::LBracket,
            TokenKind::Int(3),
            TokenKind::RBracket,
            id("i32"),
            TokenKind::LBrace,
            TokenKind::Int(1),
            TokenKind::Comma,
            TokenKind::Int(2),
            TokenKind::Comma,
            TokenKind::Int(3),
            TokenKind::RBrace,
        ]);
        match e {
            Expr::ArrayLit { elem, elems, span } => {
                assert_eq!(elem.name, "i32");
                assert_eq!(elem.array_len, Some(3));
                assert_eq!(elems.len(), 3);
                assert!(matches!(elems[2], Expr::Int { value: 3, .. }));
                assert!(span.start < span.end, "span should cover the literal");
            }
            other => panic!("expected array literal, got {:?}", other),
        }
    }

    #[test]
    fn array_literal_empty_and_trailing_comma() {
        // x = [0]i32{};  — an empty element list is accepted by the parser
        // (the count-vs-N check is a sema concern).
        let e = parse_assign_rhs(vec![
            TokenKind::LBracket,
            TokenKind::Int(0),
            TokenKind::RBracket,
            id("i32"),
            TokenKind::LBrace,
            TokenKind::RBrace,
        ]);
        match e {
            Expr::ArrayLit { elems, .. } => assert!(elems.is_empty()),
            other => panic!("expected empty array literal, got {:?}", other),
        }
        // x = [2]i32{ 1, 2, };  — a trailing comma after the last element.
        let e = parse_assign_rhs(vec![
            TokenKind::LBracket,
            TokenKind::Int(2),
            TokenKind::RBracket,
            id("i32"),
            TokenKind::LBrace,
            TokenKind::Int(1),
            TokenKind::Comma,
            TokenKind::Int(2),
            TokenKind::Comma,
            TokenKind::RBrace,
        ]);
        match e {
            Expr::ArrayLit { elems, .. } => assert_eq!(elems.len(), 2),
            other => panic!("expected array literal, got {:?}", other),
        }
    }

    #[test]
    fn index_expression() {
        // x = a[0];  ==>  Index { base: Ident a, index: Int 0 }
        let e = parse_assign_rhs(vec![
            id("a"),
            TokenKind::LBracket,
            TokenKind::Int(0),
            TokenKind::RBracket,
        ]);
        match e {
            Expr::Index { base, index, .. } => {
                assert!(matches!(*base, Expr::Ident { ref name, .. } if name == "a"));
                assert!(matches!(*index, Expr::Int { value: 0, .. }));
            }
            other => panic!("expected index, got {:?}", other),
        }
    }

    #[test]
    fn index_then_field_composition() {
        // x = a[i].x;  ==>  Field { base: Index { a, i }, field: "x" }  (left-assoc)
        let e = parse_assign_rhs(vec![
            id("a"),
            TokenKind::LBracket,
            id("i"),
            TokenKind::RBracket,
            TokenKind::Dot,
            id("x"),
        ]);
        match e {
            Expr::Field { base, field, .. } => {
                assert_eq!(field, "x");
                match *base {
                    Expr::Index { base, index, .. } => {
                        assert!(matches!(*base, Expr::Ident { ref name, .. } if name == "a"));
                        assert!(matches!(*index, Expr::Ident { ref name, .. } if name == "i"));
                    }
                    other => panic!("expected `a[i]` on the left, got {:?}", other),
                }
            }
            other => panic!("expected field-of-index, got {:?}", other),
        }
    }

    #[test]
    fn field_then_index_composition() {
        // x = m.data[i];  ==>  Index { base: Field { m, data }, index: i }
        let e = parse_assign_rhs(vec![
            id("m"),
            TokenKind::Dot,
            id("data"),
            TokenKind::LBracket,
            id("i"),
            TokenKind::RBracket,
        ]);
        match e {
            Expr::Index { base, index, .. } => {
                assert!(matches!(*index, Expr::Ident { ref name, .. } if name == "i"));
                match *base {
                    Expr::Field { base, field, .. } => {
                        assert_eq!(field, "data");
                        assert!(matches!(*base, Expr::Ident { ref name, .. } if name == "m"));
                    }
                    other => panic!("expected `m.data` base, got {:?}", other),
                }
            }
            other => panic!("expected index-of-field, got {:?}", other),
        }
    }

    #[test]
    fn nested_index_composition() {
        // x = a[i][j];  ==>  Index { base: Index { a, i }, index: j }  (left-assoc)
        let e = parse_assign_rhs(vec![
            id("a"),
            TokenKind::LBracket,
            id("i"),
            TokenKind::RBracket,
            TokenKind::LBracket,
            id("j"),
            TokenKind::RBracket,
        ]);
        match e {
            Expr::Index { base, index, .. } => {
                assert!(matches!(*index, Expr::Ident { ref name, .. } if name == "j"));
                assert!(matches!(*base, Expr::Index { .. }), "left operand should nest");
            }
            other => panic!("expected nested index, got {:?}", other),
        }
    }

    #[test]
    fn index_assign_statement() {
        // fn f() void { a[1] = 5; }  — index-assign reuses Stmt::FieldAssign with
        // an `Index` place.
        let body = parse_fn_body(vec![
            id("a"),
            TokenKind::LBracket,
            TokenKind::Int(1),
            TokenKind::RBracket,
            TokenKind::Eq,
            TokenKind::Int(5),
            TokenKind::Semicolon,
        ]);
        match &body.stmts[0] {
            Stmt::FieldAssign { place, value, .. } => {
                match place {
                    Expr::Index { base, index, .. } => {
                        assert!(matches!(**base, Expr::Ident { ref name, .. } if name == "a"));
                        assert!(matches!(**index, Expr::Int { value: 1, .. }));
                    }
                    other => panic!("expected index place `a[1]`, got {:?}", other),
                }
                assert!(matches!(value, Expr::Int { value: 5, .. }));
            }
            other => panic!("expected index assign, got {:?}", other),
        }
    }

    #[test]
    fn index_field_assign_statement() {
        // fn f() void { a[i].x = 5; }  — a composite place (`Field` of `Index`)
        // routes to Stmt::FieldAssign too.
        let body = parse_fn_body(vec![
            id("a"),
            TokenKind::LBracket,
            id("i"),
            TokenKind::RBracket,
            TokenKind::Dot,
            id("x"),
            TokenKind::Eq,
            TokenKind::Int(5),
            TokenKind::Semicolon,
        ]);
        match &body.stmts[0] {
            Stmt::FieldAssign { place, .. } => {
                assert!(
                    matches!(place, Expr::Field { base, .. } if matches!(**base, Expr::Index { .. })),
                    "expected `Field` of `Index` place, got {:?}",
                    place
                );
            }
            other => panic!("expected field/index assign, got {:?}", other),
        }
    }

    #[test]
    fn array_len_is_field_access() {
        // x = a.len;  ==>  Field { base: Ident a, field: "len" } — `a.len` parses
        // as an ordinary field access (no new parser code; sema treats `len` on
        // an array specially).
        let e = parse_assign_rhs(vec![id("a"), TokenKind::Dot, id("len")]);
        match e {
            Expr::Field { base, field, .. } => {
                assert_eq!(field, "len");
                assert!(matches!(*base, Expr::Ident { ref name, .. } if name == "a"));
            }
            other => panic!("expected field access `a.len`, got {:?}", other),
        }
    }

    #[test]
    fn array_type_not_combined_with_optional() {
        // `[2]?i32` is rejected: after `]` the parser requires a plain element
        // type name and finds `?`, reporting E0200 (SPEC §14.1: arrays are not
        // combined with `?`/`!` in v0.117).
        let err = parse(&toks(vec![
            TokenKind::Keyword(Kw::Fn),
            id("f"),
            TokenKind::LParen,
            id("a"),
            TokenKind::Colon,
            TokenKind::LBracket,
            TokenKind::Int(2),
            TokenKind::RBracket,
            TokenKind::Question,
            id("i32"),
            TokenKind::RParen,
            id("void"),
            TokenKind::LBrace,
            TokenKind::RBrace,
        ]))
        .expect_err("`[2]?i32` should fail");
        assert!(err.iter().any(|d| d.code == "E0200"));
    }

    #[test]
    fn empty_brackets_now_parse_as_slice_type() {
        // In v0.117 `[]i32` (empty brackets) was a syntax error; in v0.118 the
        // empty-bracket form is a slice type `[]T` (SPEC §15.2). This replaces
        // the old `array_type_missing_length_is_rejected` guard, whose premise
        // the slice syntax intentionally reverses. `[N]i32` is still an array.
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Fn),
            id("f"),
            TokenKind::LParen,
            id("a"),
            TokenKind::Colon,
            TokenKind::LBracket,
            TokenKind::RBracket,
            id("i32"),
            TokenKind::RParen,
            id("void"),
            TokenKind::LBrace,
            TokenKind::RBrace,
        ]))
        .expect("`[]i32` should now parse as a slice type");
        match &m.items[0] {
            Item::Func(f) => {
                let ty = &f.params[0].ty;
                assert_eq!(ty.name, "i32");
                assert!(ty.slice, "`[]i32` must set slice");
                assert_eq!(ty.array_len, None, "a slice has no fixed length");
                assert!(!ty.pointer);
                assert!(!ty.optional);
                assert!(!ty.error_union);
            }
            other => panic!("expected func, got {:?}", other),
        }
    }

    // ---- v0.118: pointers (`*T`, `&`, `.*`) & slices (`[]T`, `a[lo..hi]`) --

    #[test]
    fn pointer_type_in_param() {
        // fn f(p: *i32) void { }  — `*i32` sets pointer=true, name=i32.
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Fn),
            id("f"),
            TokenKind::LParen,
            id("p"),
            TokenKind::Colon,
            TokenKind::Star,
            id("i32"),
            TokenKind::RParen,
            id("void"),
            TokenKind::LBrace,
            TokenKind::RBrace,
        ]))
        .expect("should parse");
        match &m.items[0] {
            Item::Func(f) => {
                let ty = &f.params[0].ty;
                assert_eq!(ty.name, "i32");
                assert!(ty.pointer, "`*i32` must set pointer");
                assert!(!ty.slice);
                assert_eq!(ty.array_len, None);
                assert!(!ty.optional);
                assert!(!ty.error_union);
                assert!(ty.span.start < ty.span.end, "span should cover `*i32`");
            }
            other => panic!("expected func, got {:?}", other),
        }
    }

    #[test]
    fn slice_type_local() {
        // fn f() void { var s: []i32 = a[0..2]; }  — `[]i32` local slice type
        // with a slice-expression initializer.
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Fn),
            id("f"),
            TokenKind::LParen,
            TokenKind::RParen,
            id("void"),
            TokenKind::LBrace,
            TokenKind::Keyword(Kw::Var),
            id("s"),
            TokenKind::Colon,
            TokenKind::LBracket,
            TokenKind::RBracket,
            id("i32"),
            TokenKind::Eq,
            id("a"),
            TokenKind::LBracket,
            TokenKind::Int(0),
            TokenKind::DotDot,
            TokenKind::Int(2),
            TokenKind::RBracket,
            TokenKind::Semicolon,
            TokenKind::RBrace,
        ]))
        .expect("should parse");
        let body = match &m.items[0] {
            Item::Func(f) => &f.body,
            other => panic!("expected func, got {:?}", other),
        };
        match &body.stmts[0] {
            Stmt::Let { name, ty, value, .. } => {
                assert_eq!(name, "s");
                let ty = ty.as_ref().expect("annotated local carries Some(ty)");
                assert_eq!(ty.name, "i32");
                assert!(ty.slice, "`[]i32` local must set slice");
                assert!(!ty.pointer);
                assert_eq!(ty.array_len, None);
                assert!(matches!(value, Expr::SliceExpr { .. }), "init is a slice expr");
            }
            other => panic!("expected let, got {:?}", other),
        }
    }

    #[test]
    fn non_pointer_slice_type_flags_false() {
        // A plain `i32` carries pointer=false and slice=false (regression guard
        // for the two new TypeExpr fields — every construction must set them).
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Fn),
            id("f"),
            TokenKind::LParen,
            id("a"),
            TokenKind::Colon,
            id("i32"),
            TokenKind::RParen,
            id("i32"),
            TokenKind::LBrace,
            TokenKind::Keyword(Kw::Return),
            id("a"),
            TokenKind::Semicolon,
            TokenKind::RBrace,
        ]))
        .expect("should parse");
        match &m.items[0] {
            Item::Func(f) => {
                assert!(!f.params[0].ty.pointer);
                assert!(!f.params[0].ty.slice);
                assert!(!f.ret.pointer);
                assert!(!f.ret.slice);
            }
            other => panic!("expected func, got {:?}", other),
        }
    }

    #[test]
    fn array_type_still_parses_with_new_flags() {
        // `[3]i32` must still be a fixed-size array (array_len=Some(3)) and now
        // also carry pointer=false, slice=false (regression guard that the new
        // slice branch did not steal the `[N]T` array form).
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Fn),
            id("f"),
            TokenKind::LParen,
            id("a"),
            TokenKind::Colon,
            TokenKind::LBracket,
            TokenKind::Int(3),
            TokenKind::RBracket,
            id("i32"),
            TokenKind::RParen,
            id("void"),
            TokenKind::LBrace,
            TokenKind::RBrace,
        ]))
        .expect("should parse");
        match &m.items[0] {
            Item::Func(f) => {
                let ty = &f.params[0].ty;
                assert_eq!(ty.array_len, Some(3));
                assert!(!ty.slice, "`[3]i32` is an array, not a slice");
                assert!(!ty.pointer);
            }
            other => panic!("expected func, got {:?}", other),
        }
    }

    #[test]
    fn addrof_expression() {
        // x = &y;  ==>  AddrOf { place: Ident y }
        let e = parse_assign_rhs(vec![TokenKind::Amp, id("y")]);
        match e {
            Expr::AddrOf { place, span } => {
                assert!(matches!(*place, Expr::Ident { ref name, .. } if name == "y"));
                assert!(span.start < span.end, "span should cover `&y`");
            }
            other => panic!("expected address-of, got {:?}", other),
        }
    }

    #[test]
    fn addrof_composes_with_place() {
        // x = &a.b;  ==>  AddrOf { place: Field { base: a, field: b } } — `&`
        // binds a unary operand, so it takes the whole postfix place.
        let e = parse_assign_rhs(vec![TokenKind::Amp, id("a"), TokenKind::Dot, id("b")]);
        match e {
            Expr::AddrOf { place, .. } => match *place {
                Expr::Field { base, field, .. } => {
                    assert_eq!(field, "b");
                    assert!(matches!(*base, Expr::Ident { ref name, .. } if name == "a"));
                }
                other => panic!("expected `a.b` place, got {:?}", other),
            },
            other => panic!("expected address-of, got {:?}", other),
        }
    }

    #[test]
    fn addrof_of_index() {
        // x = &a[i];  ==>  AddrOf { place: Index { base: a, index: i } }
        let e = parse_assign_rhs(vec![
            TokenKind::Amp,
            id("a"),
            TokenKind::LBracket,
            id("i"),
            TokenKind::RBracket,
        ]);
        match e {
            Expr::AddrOf { place, .. } => {
                assert!(matches!(*place, Expr::Index { .. }), "expected index place");
            }
            other => panic!("expected address-of, got {:?}", other),
        }
    }

    #[test]
    fn deref_postfix() {
        // x = p.*;  ==>  Deref { expr: Ident p }
        let e = parse_assign_rhs(vec![id("p"), TokenKind::Dot, TokenKind::Star]);
        match e {
            Expr::Deref { expr, span } => {
                assert!(matches!(*expr, Expr::Ident { ref name, .. } if name == "p"));
                assert!(span.start < span.end, "span should cover `p.*`");
            }
            other => panic!("expected deref, got {:?}", other),
        }
    }

    #[test]
    fn deref_composes_with_field() {
        // x = p.*.x;  ==>  Field { base: Deref { expr: p }, field: x } — `.*`
        // is postfix and composes left-to-right with a following `.field`.
        let e = parse_assign_rhs(vec![
            id("p"),
            TokenKind::Dot,
            TokenKind::Star,
            TokenKind::Dot,
            id("x"),
        ]);
        match e {
            Expr::Field { base, field, .. } => {
                assert_eq!(field, "x");
                assert!(matches!(*base, Expr::Deref { .. }));
            }
            other => panic!("expected field-of-deref, got {:?}", other),
        }
    }

    #[test]
    fn deref_of_field_left_assoc() {
        // x = a.b.*;  ==>  Deref { expr: Field { base: a, field: b } } — the
        // `.*` applies to the whole preceding `a.b` chain.
        let e = parse_assign_rhs(vec![
            id("a"),
            TokenKind::Dot,
            id("b"),
            TokenKind::Dot,
            TokenKind::Star,
        ]);
        match e {
            Expr::Deref { expr, .. } => match *expr {
                Expr::Field { base, field, .. } => {
                    assert_eq!(field, "b");
                    assert!(matches!(*base, Expr::Ident { ref name, .. } if name == "a"));
                }
                other => panic!("expected `a.b` inside the deref, got {:?}", other),
            },
            other => panic!("expected deref at the root, got {:?}", other),
        }
    }

    #[test]
    fn deref_assign_statement() {
        // fn f() void { p.* = 5; }  — deref-assign reuses Stmt::FieldAssign with
        // a `Deref` place.
        let body = parse_fn_body(vec![
            id("p"),
            TokenKind::Dot,
            TokenKind::Star,
            TokenKind::Eq,
            TokenKind::Int(5),
            TokenKind::Semicolon,
        ]);
        match &body.stmts[0] {
            Stmt::FieldAssign { place, value, .. } => {
                match place {
                    Expr::Deref { expr, .. } => {
                        assert!(matches!(**expr, Expr::Ident { ref name, .. } if name == "p"));
                    }
                    other => panic!("expected deref place `p.*`, got {:?}", other),
                }
                assert!(matches!(value, Expr::Int { value: 5, .. }));
            }
            other => panic!("expected deref assign, got {:?}", other),
        }
    }

    #[test]
    fn slice_expr_range() {
        // x = a[1..3];  ==>  SliceExpr { base: a, lo: 1, hi: 3 }
        let e = parse_assign_rhs(vec![
            id("a"),
            TokenKind::LBracket,
            TokenKind::Int(1),
            TokenKind::DotDot,
            TokenKind::Int(3),
            TokenKind::RBracket,
        ]);
        match e {
            Expr::SliceExpr { base, lo, hi, span } => {
                assert!(matches!(*base, Expr::Ident { ref name, .. } if name == "a"));
                assert!(matches!(*lo, Expr::Int { value: 1, .. }));
                assert!(matches!(*hi, Expr::Int { value: 3, .. }));
                assert!(span.start < span.end, "span should cover `a[1..3]`");
            }
            other => panic!("expected slice expr, got {:?}", other),
        }
    }

    #[test]
    fn index_without_range_stays_index() {
        // x = a[i];  must stay an `Expr::Index` (no `..`), so plain indexing is
        // unchanged by the new slice-range form.
        let e = parse_assign_rhs(vec![
            id("a"),
            TokenKind::LBracket,
            id("i"),
            TokenKind::RBracket,
        ]);
        match e {
            Expr::Index { base, index, .. } => {
                assert!(matches!(*base, Expr::Ident { ref name, .. } if name == "a"));
                assert!(matches!(*index, Expr::Ident { ref name, .. } if name == "i"));
            }
            other => panic!("expected index, got {:?}", other),
        }
    }

    #[test]
    fn slice_expr_with_expr_bounds() {
        // x = a[lo..hi];  — the bounds are full expressions, not just literals.
        let e = parse_assign_rhs(vec![
            id("a"),
            TokenKind::LBracket,
            id("lo"),
            TokenKind::DotDot,
            id("hi"),
            TokenKind::RBracket,
        ]);
        match e {
            Expr::SliceExpr { lo, hi, .. } => {
                assert!(matches!(*lo, Expr::Ident { ref name, .. } if name == "lo"));
                assert!(matches!(*hi, Expr::Ident { ref name, .. } if name == "hi"));
            }
            other => panic!("expected slice expr, got {:?}", other),
        }
    }

    #[test]
    fn slice_element_assign_statement() {
        // fn f() void { s[i] = 7; }  — element assignment on a slice reuses
        // Stmt::FieldAssign with an `Index` place (same shape as arrays).
        let body = parse_fn_body(vec![
            id("s"),
            TokenKind::LBracket,
            id("i"),
            TokenKind::RBracket,
            TokenKind::Eq,
            TokenKind::Int(7),
            TokenKind::Semicolon,
        ]);
        match &body.stmts[0] {
            Stmt::FieldAssign { place, value, .. } => {
                match place {
                    Expr::Index { base, index, .. } => {
                        assert!(matches!(**base, Expr::Ident { ref name, .. } if name == "s"));
                        assert!(matches!(**index, Expr::Ident { ref name, .. } if name == "i"));
                    }
                    other => panic!("expected index place `s[i]`, got {:?}", other),
                }
                assert!(matches!(value, Expr::Int { value: 7, .. }));
            }
            other => panic!("expected index assign, got {:?}", other),
        }
    }

    #[test]
    fn pointer_type_not_combined_with_optional() {
        // `*?i32` is rejected: after `*` the parser requires a plain element
        // type name and finds `?`, reporting E0200 (SPEC §15: not combined with
        // `?`/`!` in v0.118).
        let err = parse(&toks(vec![
            TokenKind::Keyword(Kw::Fn),
            id("f"),
            TokenKind::LParen,
            id("p"),
            TokenKind::Colon,
            TokenKind::Star,
            TokenKind::Question,
            id("i32"),
            TokenKind::RParen,
            id("void"),
            TokenKind::LBrace,
            TokenKind::RBrace,
        ]))
        .expect_err("`*?i32` should fail");
        assert!(err.iter().any(|d| d.code == "E0200"));
    }

    #[test]
    fn slice_type_not_combined_with_optional() {
        // `[]?i32` is rejected: after the empty `[]` the parser requires a plain
        // element type name and finds `?`, reporting E0200 (SPEC §15.2).
        let err = parse(&toks(vec![
            TokenKind::Keyword(Kw::Fn),
            id("f"),
            TokenKind::LParen,
            id("s"),
            TokenKind::Colon,
            TokenKind::LBracket,
            TokenKind::RBracket,
            TokenKind::Question,
            id("i32"),
            TokenKind::RParen,
            id("void"),
            TokenKind::LBrace,
            TokenKind::RBrace,
        ]))
        .expect_err("`[]?i32` should fail");
        assert!(err.iter().any(|d| d.code == "E0200"));
    }

    // ---- v0.120: comptime generics ---------------------------------------

    #[test]
    fn comptime_type_param() {
        // fn id(comptime T: type, x: T) T { return x; }
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Fn),
            id("id"),
            TokenKind::LParen,
            TokenKind::Keyword(Kw::Comptime),
            id("T"),
            TokenKind::Colon,
            id("type"),
            TokenKind::Comma,
            id("x"),
            TokenKind::Colon,
            id("T"),
            TokenKind::RParen,
            id("T"),
            TokenKind::LBrace,
            TokenKind::Keyword(Kw::Return),
            id("x"),
            TokenKind::Semicolon,
            TokenKind::RBrace,
        ]))
        .expect("should parse");
        match &m.items[0] {
            Item::Func(f) => {
                assert_eq!(f.name, "id");
                assert_eq!(f.params.len(), 2);
                // The leading `comptime` marks the first param as a compile-time
                // type parameter; its `ty.name` is the literal `type`.
                assert!(f.params[0].is_comptime);
                assert_eq!(f.params[0].name, "T");
                assert_eq!(f.params[0].ty.name, "type");
                // The span covers the `comptime` keyword too.
                assert!(f.params[0].span.start < f.params[0].span.end);
                // The runtime param is ordinary; its type names the type param.
                assert!(!f.params[1].is_comptime);
                assert_eq!(f.params[1].name, "x");
                assert_eq!(f.params[1].ty.name, "T");
                assert_eq!(f.ret.name, "T");
            }
            other => panic!("expected func, got {:?}", other),
        }
    }

    #[test]
    fn non_generic_fn_has_no_comptime_params() {
        // fn add(a: i32, b: i32) i32 { return a + b; } — every param runtime.
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Fn),
            id("add"),
            TokenKind::LParen,
            id("a"),
            TokenKind::Colon,
            id("i32"),
            TokenKind::Comma,
            id("b"),
            TokenKind::Colon,
            id("i32"),
            TokenKind::RParen,
            id("i32"),
            TokenKind::LBrace,
            TokenKind::Keyword(Kw::Return),
            id("a"),
            TokenKind::Plus,
            id("b"),
            TokenKind::Semicolon,
            TokenKind::RBrace,
        ]))
        .expect("should parse");
        match &m.items[0] {
            Item::Func(f) => {
                assert_eq!(f.params.len(), 2);
                assert!(f.params.iter().all(|p| !p.is_comptime));
            }
            other => panic!("expected func, got {:?}", other),
        }
    }

    #[test]
    fn generic_call_parses_as_plain_call() {
        // A generic call `id(i32, 5)` needs no new call syntax: the type
        // argument is an ordinary `Ident` arg, so it parses as `Expr::Call`
        // with two args (SPEC §17.1). Binding the first arg as a *type* argument
        // is a sema/backend concern, not the parser's.
        let e = parse_assign_rhs(vec![
            id("id"),
            TokenKind::LParen,
            id("i32"),
            TokenKind::Comma,
            TokenKind::Int(5),
            TokenKind::RParen,
        ]);
        match e {
            Expr::Call { callee, args, .. } => {
                assert_eq!(callee, "id");
                assert_eq!(args.len(), 2);
                assert!(matches!(&args[0], Expr::Ident { name, .. } if name == "i32"));
                assert!(matches!(args[1], Expr::Int { value: 5, .. }));
            }
            other => panic!("expected a plain call, got {:?}", other),
        }
    }

    // ---- v0.121: type inference for var/const -----------------------------

    /// Parse the kinds of a single statement by wrapping them in
    /// `fn f() void { <stmt kinds> }` and returning the lone body statement.
    fn parse_one_stmt(stmt_kinds: Vec<TokenKind>) -> Stmt {
        let mut kinds = vec![
            TokenKind::Keyword(Kw::Fn),
            id("f"),
            TokenKind::LParen,
            TokenKind::RParen,
            id("void"),
            TokenKind::LBrace,
        ];
        kinds.extend(stmt_kinds);
        kinds.push(TokenKind::RBrace);
        let m = parse(&toks(kinds)).expect("should parse");
        match &m.items[0] {
            Item::Func(f) => {
                assert_eq!(f.body.stmts.len(), 1, "expected exactly one statement");
                f.body.stmts[0].clone()
            }
            other => panic!("expected func, got {:?}", other),
        }
    }

    #[test]
    fn inferred_local_var_has_no_annotation() {
        // var x = 5;  — no `: type`, so `Stmt::Let.ty` is `None` (SPEC §18.1).
        let s = parse_one_stmt(vec![
            TokenKind::Keyword(Kw::Var),
            id("x"),
            TokenKind::Eq,
            TokenKind::Int(5),
            TokenKind::Semicolon,
        ]);
        match s {
            Stmt::Let {
                is_const,
                name,
                ty,
                value,
                ..
            } => {
                assert!(!is_const, "`var` is not const");
                assert_eq!(name, "x");
                assert!(ty.is_none(), "an un-annotated `var` infers its type (ty == None)");
                assert!(matches!(value, Expr::Int { value: 5, .. }));
            }
            other => panic!("expected let, got {:?}", other),
        }
    }

    #[test]
    fn annotated_local_var_keeps_some_ty() {
        // var y: i32 = 5;  — the `: type` form keeps `ty == Some` (SPEC §18.1).
        let s = parse_one_stmt(vec![
            TokenKind::Keyword(Kw::Var),
            id("y"),
            TokenKind::Colon,
            id("i32"),
            TokenKind::Eq,
            TokenKind::Int(5),
            TokenKind::Semicolon,
        ]);
        match s {
            Stmt::Let { name, ty, .. } => {
                assert_eq!(name, "y");
                let ty = ty.expect("annotated `var` carries Some(ty)");
                assert_eq!(ty.name, "i32");
                assert!(!ty.optional);
                assert!(!ty.error_union);
            }
            other => panic!("expected let, got {:?}", other),
        }
    }

    #[test]
    fn inferred_local_const_has_no_annotation() {
        // const k = 7;  — a local `const` infers too (ty == None).
        let s = parse_one_stmt(vec![
            TokenKind::Keyword(Kw::Const),
            id("k"),
            TokenKind::Eq,
            TokenKind::Int(7),
            TokenKind::Semicolon,
        ]);
        match s {
            Stmt::Let {
                is_const, name, ty, ..
            } => {
                assert!(is_const, "`const` local is const");
                assert_eq!(name, "k");
                assert!(ty.is_none(), "an un-annotated local `const` infers its type");
            }
            other => panic!("expected let, got {:?}", other),
        }
    }

    #[test]
    fn inferred_top_level_const_has_no_annotation() {
        // const Z = 7;  — a top-level inferred value const (ConstDecl.ty == None,
        // SPEC §18.1). Before v0.121 this required a `: type` annotation.
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Const),
            id("Z"),
            TokenKind::Eq,
            TokenKind::Int(7),
            TokenKind::Semicolon,
        ]))
        .expect("should parse");
        match &m.items[0] {
            Item::Const(c) => {
                assert!(!c.is_pub);
                assert_eq!(c.name, "Z");
                assert!(c.ty.is_none(), "inferred top-level const has ty == None");
                assert!(matches!(c.value, Expr::Int { value: 7, .. }));
            }
            other => panic!("expected const, got {:?}", other),
        }
    }

    #[test]
    fn annotated_top_level_const_keeps_some_ty() {
        // const W: i32 = 7;  — the annotated form keeps ConstDecl.ty == Some.
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Const),
            id("W"),
            TokenKind::Colon,
            id("i32"),
            TokenKind::Eq,
            TokenKind::Int(7),
            TokenKind::Semicolon,
        ]))
        .expect("should parse");
        match &m.items[0] {
            Item::Const(c) => {
                assert_eq!(c.name, "W");
                let ty = c.ty.as_ref().expect("annotated const carries Some(ty)");
                assert_eq!(ty.name, "i32");
            }
            other => panic!("expected const, got {:?}", other),
        }
    }

    #[test]
    fn inferred_const_from_expression() {
        // const P = 1 + 2 * 3;  — the inferred initializer is a full expression,
        // not just a literal, and still produces a value const (ty == None).
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Const),
            id("P"),
            TokenKind::Eq,
            TokenKind::Int(1),
            TokenKind::Plus,
            TokenKind::Int(2),
            TokenKind::Star,
            TokenKind::Int(3),
            TokenKind::Semicolon,
        ]))
        .expect("should parse");
        match &m.items[0] {
            Item::Const(c) => {
                assert_eq!(c.name, "P");
                assert!(c.ty.is_none());
                assert!(matches!(
                    c.value,
                    Expr::Binary { op: BinOp::Add, .. }
                ));
            }
            other => panic!("expected const, got {:?}", other),
        }
    }

    #[test]
    fn struct_const_still_parses_after_inference_dispatch() {
        // const P = struct { x: i32 };  — a `= struct` still becomes a struct
        // declaration, not an inferred value const (SPEC §9.1, §18.1).
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Const),
            id("P"),
            TokenKind::Eq,
            TokenKind::Keyword(Kw::Struct),
            TokenKind::LBrace,
            id("x"),
            TokenKind::Colon,
            id("i32"),
            TokenKind::RBrace,
            TokenKind::Semicolon,
        ]))
        .expect("should parse");
        match &m.items[0] {
            Item::Struct(s) => {
                assert_eq!(s.name, "P");
                assert_eq!(s.fields.len(), 1);
                assert_eq!(s.fields[0].name, "x");
            }
            other => panic!("expected struct, got {:?}", other),
        }
    }

    #[test]
    fn enum_const_still_parses_after_inference_dispatch() {
        // const E = enum { A, B };  — a `= enum` still becomes an enum
        // declaration, not an inferred value const (SPEC §13.1, §18.1).
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Const),
            id("E"),
            TokenKind::Eq,
            TokenKind::Keyword(Kw::Enum),
            TokenKind::LBrace,
            id("A"),
            TokenKind::Comma,
            id("B"),
            TokenKind::RBrace,
            TokenKind::Semicolon,
        ]))
        .expect("should parse");
        match &m.items[0] {
            Item::Enum(e) => {
                assert_eq!(e.name, "E");
                assert_eq!(e.variants, vec!["A", "B"]);
            }
            other => panic!("expected enum, got {:?}", other),
        }
    }

    // ---- v0.124: tagged unions (`union(enum)`) + switch capture -----------

    #[test]
    fn union_decl_two_variants() {
        // pub const Shape = union(enum) { circle: i32, rect: i64, };  (trailing
        // comma) — each variant carries a payload type.
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Pub),
            TokenKind::Keyword(Kw::Const),
            id("Shape"),
            TokenKind::Eq,
            TokenKind::Keyword(Kw::Union),
            TokenKind::LParen,
            TokenKind::Keyword(Kw::Enum),
            TokenKind::RParen,
            TokenKind::LBrace,
            id("circle"),
            TokenKind::Colon,
            id("i32"),
            TokenKind::Comma,
            id("rect"),
            TokenKind::Colon,
            id("i64"),
            TokenKind::Comma,
            TokenKind::RBrace,
            TokenKind::Semicolon,
        ]))
        .expect("should parse");
        match &m.items[0] {
            Item::Union(u) => {
                assert!(u.is_pub);
                assert_eq!(u.name, "Shape");
                assert_eq!(u.variants.len(), 2);
                assert_eq!(u.variants[0].name, "circle");
                assert_eq!(u.variants[0].payload.name, "i32");
                assert_eq!(u.variants[1].name, "rect");
                assert_eq!(u.variants[1].payload.name, "i64");
                assert!(u.span.start < u.span.end);
                assert!(u.variants[0].span.start < u.variants[0].span.end);
            }
            other => panic!("expected union, got {:?}", other),
        }
    }

    #[test]
    fn union_decl_composite_payload_types() {
        // const Val = union(enum) { i: i32, p: *i32, s: []u8 };  — payloads reuse
        // `parse_type`, so pointer / slice forms work as variant payloads.
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Const),
            id("Val"),
            TokenKind::Eq,
            TokenKind::Keyword(Kw::Union),
            TokenKind::LParen,
            TokenKind::Keyword(Kw::Enum),
            TokenKind::RParen,
            TokenKind::LBrace,
            id("i"),
            TokenKind::Colon,
            id("i32"),
            TokenKind::Comma,
            id("p"),
            TokenKind::Colon,
            TokenKind::Star,
            id("i32"),
            TokenKind::Comma,
            id("s"),
            TokenKind::Colon,
            TokenKind::LBracket,
            TokenKind::RBracket,
            id("u8"),
            TokenKind::RBrace,
            TokenKind::Semicolon,
        ]))
        .expect("should parse");
        match &m.items[0] {
            Item::Union(u) => {
                assert!(!u.is_pub);
                assert_eq!(u.name, "Val");
                assert_eq!(u.variants.len(), 3);
                assert_eq!(u.variants[0].payload.name, "i32");
                assert!(!u.variants[0].payload.pointer);
                assert_eq!(u.variants[1].name, "p");
                assert!(u.variants[1].payload.pointer, "`*i32` payload is a pointer");
                assert_eq!(u.variants[2].name, "s");
                assert!(u.variants[2].payload.slice, "`[]u8` payload is a slice");
            }
            other => panic!("expected union, got {:?}", other),
        }
    }

    #[test]
    fn struct_and_enum_still_parse_after_union_branch() {
        // The `= struct` / `= enum` dispatch must keep working after the new
        // `= union(enum)` branch joins the const-item dispatch (regression).
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Const),
            id("Point"),
            TokenKind::Eq,
            TokenKind::Keyword(Kw::Struct),
            TokenKind::LBrace,
            id("x"),
            TokenKind::Colon,
            id("i32"),
            TokenKind::RBrace,
            TokenKind::Semicolon,
            TokenKind::Keyword(Kw::Const),
            id("E"),
            TokenKind::Eq,
            TokenKind::Keyword(Kw::Enum),
            TokenKind::LBrace,
            id("A"),
            TokenKind::RBrace,
            TokenKind::Semicolon,
        ]))
        .expect("should parse");
        assert!(matches!(&m.items[0], Item::Struct(s) if s.name == "Point"));
        assert!(matches!(&m.items[1], Item::Enum(e) if e.name == "E"));
    }

    #[test]
    fn union_construction_parses_as_struct_lit() {
        // x = Shape{ .circle = 5 };  — union construction reuses `Expr::StructLit`
        // with no new parser code; sema distinguishes union vs struct by name.
        let e = parse_assign_rhs(vec![
            id("Shape"),
            TokenKind::LBrace,
            TokenKind::Dot,
            id("circle"),
            TokenKind::Eq,
            TokenKind::Int(5),
            TokenKind::RBrace,
        ]);
        match e {
            Expr::StructLit { name, fields, .. } => {
                assert_eq!(name, "Shape");
                assert_eq!(fields.len(), 1);
                assert_eq!(fields[0].name, "circle");
                assert!(matches!(fields[0].value, Expr::Int { value: 5, .. }));
            }
            other => panic!("expected struct literal, got {:?}", other),
        }
    }

    #[test]
    fn switch_with_capture_arms() {
        // switch (v) { .circle => |r| { return; } .rect => |w| { return; }
        //              else => { return; } }
        // The `| IDENT |` after `=>` sets `SwitchArm.capture`.
        let body = parse_fn_body(vec![
            TokenKind::Keyword(Kw::Switch),
            TokenKind::LParen,
            id("v"),
            TokenKind::RParen,
            TokenKind::LBrace,
            // .circle => |r| { return; }
            TokenKind::Dot,
            id("circle"),
            TokenKind::FatArrow,
            TokenKind::Pipe,
            id("r"),
            TokenKind::Pipe,
            TokenKind::LBrace,
            TokenKind::Keyword(Kw::Return),
            TokenKind::Semicolon,
            TokenKind::RBrace,
            // .rect => |w| { return; }
            TokenKind::Dot,
            id("rect"),
            TokenKind::FatArrow,
            TokenKind::Pipe,
            id("w"),
            TokenKind::Pipe,
            TokenKind::LBrace,
            TokenKind::Keyword(Kw::Return),
            TokenKind::Semicolon,
            TokenKind::RBrace,
            // else => { return; }
            TokenKind::Keyword(Kw::Else),
            TokenKind::FatArrow,
            TokenKind::LBrace,
            TokenKind::Keyword(Kw::Return),
            TokenKind::Semicolon,
            TokenKind::RBrace,
            TokenKind::RBrace, // close switch
        ]);
        match &body.stmts[0] {
            Stmt::Switch { arms, default, .. } => {
                assert_eq!(arms.len(), 2);
                assert!(matches!(&arms[0].labels[0], Expr::EnumLit { variant, .. } if variant == "circle"));
                assert_eq!(arms[0].capture.as_deref(), Some("r"));
                assert!(matches!(&arms[1].labels[0], Expr::EnumLit { variant, .. } if variant == "rect"));
                assert_eq!(arms[1].capture.as_deref(), Some("w"));
                assert!(default.is_some(), "the `else` arm should set `default`");
            }
            other => panic!("expected switch, got {:?}", other),
        }
    }

    #[test]
    fn switch_without_capture_sets_none() {
        // switch (c) { .Red => { } else => { } }  — a bare `=>` (no `| |`) leaves
        // `SwitchArm.capture == None` (regression guard for the new field).
        let body = parse_fn_body(vec![
            TokenKind::Keyword(Kw::Switch),
            TokenKind::LParen,
            id("c"),
            TokenKind::RParen,
            TokenKind::LBrace,
            TokenKind::Dot,
            id("Red"),
            TokenKind::FatArrow,
            TokenKind::LBrace,
            TokenKind::RBrace,
            TokenKind::Keyword(Kw::Else),
            TokenKind::FatArrow,
            TokenKind::LBrace,
            TokenKind::RBrace,
            TokenKind::RBrace, // close switch
        ]);
        match &body.stmts[0] {
            Stmt::Switch { arms, .. } => {
                assert_eq!(arms.len(), 1);
                assert!(arms[0].capture.is_none(), "a bare `=>` arm has no capture");
            }
            other => panic!("expected switch, got {:?}", other),
        }
    }

    #[test]
    fn switch_capture_on_multi_label_arm() {
        // switch (v) { .a, .b => |x| { } else => { } }  — a capture follows the
        // whole label list, after the `=>`.
        let body = parse_fn_body(vec![
            TokenKind::Keyword(Kw::Switch),
            TokenKind::LParen,
            id("v"),
            TokenKind::RParen,
            TokenKind::LBrace,
            TokenKind::Dot,
            id("a"),
            TokenKind::Comma,
            TokenKind::Dot,
            id("b"),
            TokenKind::FatArrow,
            TokenKind::Pipe,
            id("x"),
            TokenKind::Pipe,
            TokenKind::LBrace,
            TokenKind::RBrace,
            TokenKind::Keyword(Kw::Else),
            TokenKind::FatArrow,
            TokenKind::LBrace,
            TokenKind::RBrace,
            TokenKind::RBrace, // close switch
        ]);
        match &body.stmts[0] {
            Stmt::Switch { arms, .. } => {
                assert_eq!(arms.len(), 1);
                assert_eq!(arms[0].labels.len(), 2);
                assert_eq!(arms[0].capture.as_deref(), Some("x"));
            }
            other => panic!("expected switch, got {:?}", other),
        }
    }

    fn str(s: &str) -> TokenKind {
        TokenKind::Str(s.to_string())
    }

    #[test]
    fn import_item_parses() {
        // @import("util.ks");
        let m = parse(&toks(vec![
            TokenKind::At,
            id("import"),
            TokenKind::LParen,
            str("util.ks"),
            TokenKind::RParen,
            TokenKind::Semicolon,
        ]))
        .expect("should parse");
        assert_eq!(m.items.len(), 1);
        match &m.items[0] {
            Item::Import(imp) => assert_eq!(imp.path, "util.ks"),
            other => panic!("expected import, got {:?}", other),
        }
    }

    #[test]
    fn import_and_fn_program() {
        // @import("util.ks"); fn main() void { }
        let m = parse(&toks(vec![
            TokenKind::At,
            id("import"),
            TokenKind::LParen,
            str("util.ks"),
            TokenKind::RParen,
            TokenKind::Semicolon,
            TokenKind::Keyword(Kw::Fn),
            id("main"),
            TokenKind::LParen,
            TokenKind::RParen,
            id("void"),
            TokenKind::LBrace,
            TokenKind::RBrace,
        ]))
        .expect("should parse");
        assert_eq!(m.items.len(), 2);
        match &m.items[0] {
            Item::Import(imp) => assert_eq!(imp.path, "util.ks"),
            other => panic!("expected import, got {:?}", other),
        }
        match &m.items[1] {
            Item::Func(f) => assert_eq!(f.name, "main"),
            other => panic!("expected fn, got {:?}", other),
        }
    }

    #[test]
    fn bad_builtin_item_reports_e0200() {
        // @notimport("x");  — only `@import` is a builtin item in v0.126.
        let err = parse(&toks(vec![
            TokenKind::At,
            id("notimport"),
            TokenKind::LParen,
            str("x"),
            TokenKind::RParen,
            TokenKind::Semicolon,
        ]))
        .expect_err("should reject unknown builtin item");
        assert!(err.iter().any(|d| d.code == "E0200"));
    }
}
