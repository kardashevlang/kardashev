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
    ArraySize, BinOp, Block, ConstDecl, EnumDecl, ErrorSetDecl, Expr, FieldDecl, FieldInit, Func,
    ImportDecl, Item, Module, Param, Stmt, StructDecl, SwitchArm, TestBlock, TypeExpr, UnOp,
    UnionDecl, UnionVariant,
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

    /// The kind two tokens ahead (clamped to the trailing `Eof`). Used by
    /// `parse_const` to distinguish `= error {` (a named error-set declaration,
    /// v0.139) from `= error .` (an error-literal value const).
    fn peek3_kind(&self) -> &TokenKind {
        let i = (self.pos + 2).min(self.tokens.len().saturating_sub(1));
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

    /// Consume an integer literal, returning its value and span. A negative or
    /// absurd literal is a sema concern (`E0224`), not the parser's.
    fn expect_int(&mut self) -> PResult<(i64, Span)> {
        let span = self.peek_span();
        let v = match self.peek_kind() {
            TokenKind::Int(v) => *v,
            _ => return Err(self.expected("integer literal")),
        };
        self.bump();
        Ok((v, span))
    }

    /// Parse the array size `N` inside a `[N]T` type, with the cursor on the
    /// token after the `[` (SPEC §14.1, §24.1). An integer literal `[3]T` is a
    /// literal size (`ArraySize::Lit`); an identifier `[n]T` is a comptime
    /// value-parameter name (`ArraySize::Param`, v0.128, bound per
    /// monomorphisation). Whether a literal is non-negative, and whether a named
    /// size resolves to a bound comptime value, are sema concerns
    /// (`E0224`/§24.2), not the parser's.
    fn parse_array_size(&mut self) -> PResult<ArraySize> {
        match self.peek_kind() {
            TokenKind::Int(_) => {
                let (v, _) = self.expect_int()?;
                Ok(ArraySize::Lit(v))
            }
            TokenKind::Ident(name) => {
                let name = name.clone();
                self.bump();
                Ok(ArraySize::Param(name))
            }
            _ => Err(self.expected("an array size (integer literal or parameter name)")),
        }
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

    /// Push a custom `E0200` diagnostic at `span` and return the unwind
    /// sentinel. Used where the failure is a *semantic* shape constraint on the
    /// surrounding form rather than an unexpected token — e.g. a `for` index
    /// range that does not start at `0`, or a capture-arity mismatch — so the
    /// message can describe the real problem instead of "expected X, found Y".
    fn error_at(&mut self, span: Span, msg: impl Into<String>) -> ParseError {
        self.diags
            .push(Diagnostic::error(span, "E0200", msg.into()));
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
            // An optional leading `comptime` keyword marks a compile-time
            // parameter (SPEC §17.1, §24.1). The rest of the parameter —
            // `IDENT : type` — is parsed identically regardless of what the
            // annotation is: a `comptime IDENT: type` is a compile-time *type*
            // parameter (v0.120), while a `comptime IDENT: usize` (or any int
            // type) is a compile-time *value* parameter (v0.128, array-size
            // generics). The annotation is always just an ordinary `TypeExpr`
            // here; sema distinguishes the two by whether it resolves to the
            // literal `type` or to an integer type. A plain parameter has
            // `is_comptime = false`.
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

    /// Parse a type *name* — the identifier at the core of a type reference.
    ///
    /// This is normally an `IDENT` (e.g. `i32`, `Point`, `Self`), but in type
    /// position the reflection builtin `@This()` (SPEC §32.2) denotes the
    /// **enclosing struct type** and **desugars to `Self`** (the v0.130
    /// self-type, resolved contextually in sema). So an `@This()` here yields
    /// the name `"Self"` with a span covering the whole `@This()` form, exactly
    /// as if `Self` had been written. The only `@`-builtin valid in type
    /// position is `@This()`: an `@` followed by anything other than `This`, or
    /// `This` not followed by `()`, is `E0200` (`@import` is a top-level item
    /// and the expression builtins `@sizeOf`/`@typeName` are values, not types).
    ///
    /// Used wherever a bare type name is expected — the `*T`, `[]T`, `[N]T`, and
    /// plain forms — so `@This()` and `*@This()` (a pointer receiver, SPEC §30)
    /// parse uniformly. For an ordinary identifier this is exactly
    /// [`Parser::expect_ident`], so existing type parsing is unchanged.
    fn parse_type_name(&mut self) -> PResult<(String, Span)> {
        if self.at_punct(&TokenKind::At) {
            let start = self.peek_span();
            self.bump(); // `@`
            let is_this = matches!(self.peek_kind(), TokenKind::Ident(s) if s == "This");
            if !is_this {
                return Err(self.expected("`This` (the only `@`-builtin valid in a type)"));
            }
            self.bump(); // `This`
            self.expect_punct(&TokenKind::LParen, "`(`")?;
            let rparen = self.expect_punct(&TokenKind::RParen, "`)`")?;
            // `@This()` === `Self` (SPEC §32.2 / §30): the enclosing struct type.
            return Ok(("Self".to_string(), start.merge(rparen)));
        }
        self.expect_ident()
    }

    /// Parse a type reference (SPEC §11.1, §12.1, §14.1, §15). A leading `?`
    /// marks an optional type `?T` (`optional = true`); a leading `!` marks an
    /// error union `!T` (`error_union = true`). The two prefixes are **mutually
    /// exclusive** in v0.115 and the parser consumes at most one: after a `?`
    /// or `!` it requires the inner type name, so `?!T` / `!?T` fail with an
    /// "expected identifier" diagnostic. A bare type has every flag `false`.
    ///
    /// A leading `[` introduces either a fixed-size array `[N]T` (`array_len =
    /// Some(..)`, `name = T`) when an array size follows the `[`, or a slice
    /// `[]T` (`slice = true`) when the brackets are empty (SPEC §14.1, §15.2,
    /// §24.1). The array size is either an integer literal `[3]T`
    /// (`ArraySize::Lit(3)`, v0.117) or an identifier `[n]T`
    /// (`ArraySize::Param("n")`, a comptime value-parameter name, v0.128). A
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
            let (name, name_span) = self.parse_type_name()?;
            return Ok(TypeExpr {
                name,
                optional: false,
                error_union: false,
                error_set: None,
                array_len: None,
                pointer: true,
                slice: false,
                span: start.merge(name_span),
            });
        }
        if self.at_punct(&TokenKind::LBracket) {
            let start = self.peek_span();
            self.bump(); // `[`
            // Empty brackets `[]T` are a slice (SPEC §15.2); `[N]T` (an array
            // size inside the brackets) is a fixed-size array (SPEC §14.1,
            // §24.1). Distinguish by whether a `]` immediately follows the `[`.
            if self.at_punct(&TokenKind::RBracket) {
                self.bump(); // `]`
                let (name, name_span) = self.parse_type_name()?;
                return Ok(TypeExpr {
                    name,
                    optional: false,
                    error_union: false,
                    error_set: None,
                    array_len: None,
                    pointer: false,
                    slice: true,
                    span: start.merge(name_span),
                });
            }
            let size = self.parse_array_size()?;
            self.expect_punct(&TokenKind::RBracket, "`]`")?;
            let (name, name_span) = self.parse_type_name()?;
            return Ok(TypeExpr {
                name,
                optional: false,
                error_union: false,
                error_set: None,
                array_len: Some(size),
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
        let (name, name_span) = self.parse_type_name()?;
        // `Set!T` — a *named* error union (SPEC §34.1, v0.139): a base type name
        // `Set` immediately followed by `!` in type position, where `Set` is the
        // error set and the type after the `!` is the payload. This is only the
        // named form when neither a `?` (optional) nor a prefix `!` (the global
        // error union `!T`) was already consumed, keeping those forms unchanged.
        // The base-name-then-`!` shape only arises in type position, so it never
        // disturbs expression parsing (where `!` is logical negation / `!=`).
        if opt_span.is_none() && err_span.is_none() && self.at_punct(&TokenKind::Bang) {
            self.bump(); // `!`
            let (payload, payload_span) = self.parse_type_name()?;
            return Ok(TypeExpr {
                name: payload,
                optional: false,
                error_union: true,
                error_set: Some(name),
                array_len: None,
                pointer: false,
                slice: false,
                span: name_span.merge(payload_span),
            });
        }
        let span = match opt_span.or(err_span) {
            Some(prefix) => prefix.merge(name_span),
            None => name_span,
        };
        Ok(TypeExpr {
            name,
            optional: opt_span.is_some(),
            error_union: err_span.is_some(),
            error_set: None,
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
                // `= error {` is a named error-set declaration (SPEC §34.1,
                // v0.139): `const Name = error{ A, B };`. The `error` keyword is
                // overloaded — `= error .Name` is instead an error-literal value
                // const, so the set form is selected only when `{` (not `.`)
                // follows `error`; the `error .` case falls through to the value
                // path below and parses as an `Expr::ErrorLit`.
                TokenKind::Keyword(Kw::Error)
                    if matches!(self.peek3_kind(), TokenKind::LBrace) =>
                {
                    return self.parse_error_set_decl(is_pub, name, start);
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
        let (fields, methods) = self.parse_struct_body()?;
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

    /// Parse the body of a `struct { … }` — the fields, then the methods /
    /// associated functions — with the opening `{` already consumed and the
    /// cursor positioned just after it. Stops at (without consuming) the
    /// closing `}`, returning `(fields, methods)`.
    ///
    /// This is shared by a **named** struct declaration `const Name = struct {
    /// … };` (SPEC §9.1, §10) and an **anonymous** `struct { … }` *type value*
    /// (SPEC §25.1, §26.1) — both use identical field+method syntax, so a
    /// type-constructor's struct becomes a real container (the foundation of
    /// `ArrayList(T)`) for free.
    ///
    /// Fields come first: `IDENT : type`, comma-separated with an optional
    /// trailing comma. A `pub`/`fn` keyword (the start of a method) or the
    /// closing `}` ends the field list — field names are identifiers, so they
    /// never collide with the `pub`/`fn` keywords that introduce methods. Then
    /// zero or more methods / associated functions, each a `pub? fn ...` parsed
    /// with the shared [`Parser::parse_func_decl`] logic (so both struct forms
    /// grow new function-syntax features for free), until the closing `}` (SPEC
    /// §10). A method's `self: Self` / `*Self` receiver — where `Self` denotes
    /// the enclosing (instantiated) struct, SPEC §26.1 — is just an ordinary
    /// parameter to the parser; resolving `Self` is a sema concern. Duplicate
    /// field names are likewise a sema concern (`E0162`), not the parser's.
    fn parse_struct_body(&mut self) -> PResult<(Vec<FieldDecl>, Vec<Func>)> {
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
        let mut methods = Vec::new();
        while !self.at_punct(&TokenKind::RBrace) {
            let m_start = self.peek_span();
            let m_pub = self.eat_kw(Kw::Pub);
            if !self.at_kw(Kw::Fn) {
                return Err(self.expected("`fn` or `}`"));
            }
            methods.push(self.parse_func_decl(m_pub, m_start)?);
        }
        Ok((fields, methods))
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

    /// Parse the tail of a named error-set declaration, with `const IDENT`
    /// already consumed and the cursor on the `=`:
    /// `= "error" "{" IDENT ("," IDENT)* ","? "}" ";"` (SPEC §34.1, v0.139). A
    /// comma-separated list of error-member names with an optional trailing
    /// comma — the same shape as a plain `enum` body. Member-duplication
    /// detection is a sema concern (`E0331`), not the parser's. This mirrors
    /// `= struct`/`= enum`/`= union(enum)` and is dispatched from `parse_const`
    /// only when `{` (not `.`) follows the `error` keyword, so the value form
    /// `const C = error.Name;` is left to the expression path.
    fn parse_error_set_decl(&mut self, is_pub: bool, name: String, start: Span) -> PResult<Item> {
        self.bump(); // `=`
        if !self.eat_kw(Kw::Error) {
            return Err(self.expected("`error`"));
        }
        self.expect_punct(&TokenKind::LBrace, "`{`")?;
        let mut members = Vec::new();
        while !self.at_punct(&TokenKind::RBrace) {
            let (mname, _) = self.expect_ident()?;
            members.push(mname);
            if !self.eat_punct(&TokenKind::Comma) {
                break; // no separator → the member list is done
            }
        }
        self.expect_punct(&TokenKind::RBrace, "`}`")?;
        let semi = self.expect_punct(&TokenKind::Semicolon, "`;`")?;
        Ok(Item::ErrorSet(ErrorSetDecl {
            is_pub,
            name,
            members,
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
            TokenKind::Keyword(Kw::For) => self.parse_for(),
            TokenKind::Keyword(Kw::Break) => self.parse_break(),
            TokenKind::Keyword(Kw::Continue) => self.parse_continue(),
            TokenKind::Keyword(Kw::Defer) => self.parse_defer(),
            TokenKind::Keyword(Kw::Errdefer) => self.parse_errdefer(),
            TokenKind::Keyword(Kw::Switch) => self.parse_switch(),
            TokenKind::LBrace => Ok(Stmt::Block(self.parse_block()?)),
            // A simple-name target followed by an assignment operator (`=` or a
            // compound `+= -= *= /= %=`, SPEC §27.1) is a `Stmt::Assign`; a
            // field/index place is handled by `parse_expr_stmt`.
            TokenKind::Ident(_) if assign_op_kind(self.peek2_kind()).is_some() => {
                self.parse_assign()
            }
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
        // Plain `=` → `op: None`; a compound `+= -= *= /= %=` → the matching
        // `BinOp` (SPEC §27.1). The `parse_stmt` dispatch only routes here on an
        // assignment operator, but fall back to a real diagnostic regardless.
        let op = match assign_op_kind(self.peek_kind()) {
            Some(op) => op,
            None => return Err(self.expected("`=`, `+=`, `-=`, `*=`, `/=`, or `%=`")),
        };
        self.bump(); // the assignment operator
        let value = self.parse_expr()?;
        let semi = self.expect_punct(&TokenKind::Semicolon, "`;`")?;
        Ok(Stmt::Assign {
            name,
            op,
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
            && assign_op_kind(self.peek2_kind()).is_some()
        {
            let (name, name_span) = self.expect_ident()?;
            // Plain `=` → `op: None`; a compound `+= -= *= /= %=` → its `BinOp`
            // (SPEC §27.1). The guard above ensures this is an assignment
            // operator, so `flatten` keeps the (inner) op and never drops one.
            let op = assign_op_kind(self.peek_kind()).flatten();
            self.bump(); // the assignment operator
            let value = self.parse_expr()?;
            let span = name_span.merge(value.span());
            Ok(Stmt::Assign {
                name,
                op,
                value,
                span,
            })
        } else {
            let e = self.parse_expr()?;
            Ok(Stmt::Expr(e))
        }
    }

    /// Parse a `for` loop (SPEC §29.1):
    /// `for "(" iter ("," 0 "..")? ")" "|" elem ("," index)? "|" block`.
    ///
    /// The iterable is a full expression. An optional `, 0 ..` after it (a
    /// `Comma`, an `Int` literal that must be `0`, then a `DotDot`) selects the
    /// **index form** `for (xs, 0..) |x, i| { … }`, which additionally binds a
    /// 0-based `usize` index. The literal is required to be exactly `0` (else
    /// `E0200` "for index range must start at 0"). After `)`, a `| elem |` (or
    /// `| elem, index |`) capture list names the loop bindings.
    ///
    /// The capture count must agree with the presence of the index range
    /// (`E0200` otherwise): the `, 0..` form requires **two** captures (the
    /// element and the index), and the plain form requires **exactly one**.
    /// `index` is `Some(name)` iff the index form was written. That the iterable
    /// is actually an array/slice is a sema concern (SPEC §29.1), not the
    /// parser's. The body is an ordinary braced block (a loop-body scope, so
    /// `break`/`continue`/`defer` behave; the lowering to an indexed `while` is
    /// an emit concern, SPEC §29.2).
    fn parse_for(&mut self) -> PResult<Stmt> {
        let start = self.peek_span();
        self.bump(); // `for`
        self.expect_punct(&TokenKind::LParen, "`(`")?;
        let iter = self.parse_expr()?;
        // An optional `, 0 ..` marks the index form. The integer between the
        // comma and the `..` must be exactly `0` — the index always counts from
        // zero (SPEC §29.1) — so a non-zero start is rejected here.
        let index_form = if self.eat_punct(&TokenKind::Comma) {
            let (lo, lo_span) = self.expect_int()?;
            if lo != 0 {
                return Err(self.error_at(lo_span, "for index range must start at 0"));
            }
            self.expect_punct(&TokenKind::DotDot, "`..`")?;
            true
        } else {
            false
        };
        self.expect_punct(&TokenKind::RParen, "`)`")?;
        // The capture list: `| elem |` or `| elem, index |`.
        let pipe_span = self.expect_punct(&TokenKind::Pipe, "`|`")?;
        let (elem, _) = self.expect_ident()?;
        let second = if self.eat_punct(&TokenKind::Comma) {
            let (name, _) = self.expect_ident()?;
            Some(name)
        } else {
            None
        };
        let close_pipe = self.expect_punct(&TokenKind::Pipe, "`|`")?;
        // The capture arity must match the index form (SPEC §29.1): the
        // `, 0..` form binds an element *and* an index (exactly two captures);
        // the plain form binds only the element (exactly one).
        let index = if index_form {
            match second {
                Some(name) => Some(name),
                None => {
                    return Err(self.error_at(
                        pipe_span.merge(close_pipe),
                        "a `for (.., 0..)` index loop requires two captures `|elem, index|`",
                    ));
                }
            }
        } else if second.is_some() {
            return Err(self.error_at(
                pipe_span.merge(close_pipe),
                "a `for` without `, 0..` takes exactly one capture `|elem|`",
            ));
        } else {
            None
        };
        let body = self.parse_block()?;
        let span = start.merge(body.span);
        Ok(Stmt::For {
            iter,
            elem,
            index,
            body,
            span,
        })
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
        // A compound assignment (`+= -= *= /= %=`, SPEC §27.1) is accepted on a
        // place just like a plain `=`; the matched op (`None`/`Some(binop)`) is
        // threaded into `Stmt::FieldAssign`.
        let place_op = if matches!(
            expr,
            Expr::Field { .. } | Expr::Index { .. } | Expr::Deref { .. }
        ) {
            assign_op_kind(self.peek_kind())
        } else {
            None
        };
        if let Some(op) = place_op {
            self.bump(); // the assignment operator
            let value = self.parse_expr()?;
            let semi = self.expect_punct(&TokenKind::Semicolon, "`;`")?;
            let span = expr.span().merge(semi);
            return Ok(Stmt::FieldAssign {
                place: expr,
                op,
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
        let mut lhs = self.parse_bitor()?;
        while self.at_kw(Kw::And) {
            self.bump();
            let rhs = self.parse_bitor()?;
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

    /// Bitwise-or level (SPEC §28.1): infix `|` (`BinOp::BitOr`), left-
    /// associative, binding looser than `^`/`&` but tighter than the `and`
    /// keyword. The `|` is **infix here only** — it is reached after a left
    /// operand has already been parsed, so the capture form `| IDENT |`
    /// (recognised in the `if`/`catch`/`switch` grammar positions, never as an
    /// expression operand) is unaffected by this level.
    fn parse_bitor(&mut self) -> PResult<Expr> {
        let mut lhs = self.parse_bitxor()?;
        while self.at_punct(&TokenKind::Pipe) {
            self.bump();
            let rhs = self.parse_bitxor()?;
            let span = lhs.span().merge(rhs.span());
            lhs = Expr::Binary {
                op: BinOp::BitOr,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                span,
            };
        }
        Ok(lhs)
    }

    /// Bitwise-xor level (SPEC §28.1): infix `^` (`BinOp::BitXor`), left-
    /// associative, sitting between `|` (looser) and `&` (tighter).
    fn parse_bitxor(&mut self) -> PResult<Expr> {
        let mut lhs = self.parse_bitand()?;
        while self.at_punct(&TokenKind::Caret) {
            self.bump();
            let rhs = self.parse_bitand()?;
            let span = lhs.span().merge(rhs.span());
            lhs = Expr::Binary {
                op: BinOp::BitXor,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                span,
            };
        }
        Ok(lhs)
    }

    /// Bitwise-and level (SPEC §28.1): infix `&` (`BinOp::BitAnd`), left-
    /// associative, binding tighter than `^`/`|` but looser than equality
    /// (`==`/`!=`), so `x & y == z` parses as `x & (y == z)`. The `&` is
    /// **infix here only** — it is reached after a left operand, so the prefix
    /// address-of `&place` (parsed at the unary level, SPEC §15.1) is
    /// unaffected.
    fn parse_bitand(&mut self) -> PResult<Expr> {
        let mut lhs = self.parse_eq()?;
        while self.at_punct(&TokenKind::Amp) {
            self.bump();
            let rhs = self.parse_eq()?;
            let span = lhs.span().merge(rhs.span());
            lhs = Expr::Binary {
                op: BinOp::BitAnd,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                span,
            };
        }
        Ok(lhs)
    }

    /// Equality level (SPEC §28.1): infix `==`/`!=` (`BinOp::Eq`/`Ne`), left-
    /// associative, binding looser than the relational operators.
    fn parse_eq(&mut self) -> PResult<Expr> {
        let mut lhs = self.parse_rel()?;
        loop {
            let op = match self.peek_kind() {
                TokenKind::EqEq => BinOp::Eq,
                TokenKind::BangEq => BinOp::Ne,
                _ => break,
            };
            self.bump();
            let rhs = self.parse_rel()?;
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

    /// Relational level (SPEC §28.1): infix `< <= > >=` (`BinOp::Lt`/`Le`/
    /// `Gt`/`Ge`), left-associative, binding tighter than equality but looser
    /// than the shift operators.
    fn parse_rel(&mut self) -> PResult<Expr> {
        let mut lhs = self.parse_shift()?;
        loop {
            let op = match self.peek_kind() {
                TokenKind::Lt => BinOp::Lt,
                TokenKind::Le => BinOp::Le,
                TokenKind::Gt => BinOp::Gt,
                TokenKind::Ge => BinOp::Ge,
                _ => break,
            };
            self.bump();
            let rhs = self.parse_shift()?;
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

    /// Shift level (SPEC §28.1): infix `<<`/`>>` (`BinOp::Shl`/`Shr`), left-
    /// associative, binding tighter than the relational operators but looser
    /// than additive (`+`/`-`), so `1 << 2 + 3` parses as `1 << (2 + 3)`.
    fn parse_shift(&mut self) -> PResult<Expr> {
        let mut lhs = self.parse_add()?;
        loop {
            let op = match self.peek_kind() {
                TokenKind::Shl => BinOp::Shl,
                TokenKind::Shr => BinOp::Shr,
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
        // `~expr` (SPEC §28.1) is the bitwise-complement prefix, sitting at the
        // unary level alongside `-`/`!`; it yields `Expr::Unary { UnOp::BitNot }`.
        let op = match self.peek_kind() {
            TokenKind::Minus => Some(UnOp::Neg),
            TokenKind::Bang => Some(UnOp::Not),
            TokenKind::Tilde => Some(UnOp::BitNot),
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
            TokenKind::Keyword(Kw::Struct) => {
                // An anonymous `struct { f: T, ... }` **type value** (SPEC
                // §25.1) — the body of a type-returning function
                // `fn F(comptime T: type) type { return struct {…}; }`. This
                // arm is reached only when `struct` appears in *expression*
                // position (after `return`/`=` inside a function body, etc.).
                //
                // A top-level *named* struct declaration `const Name = struct
                // {…};` (SPEC §9.1) is dispatched earlier, in `parse_const`'s
                // `= struct` branch (selected by `peek2_kind`), and never
                // reaches `parse_expr`/`parse_primary`, so the two `struct`
                // forms do not collide. The body — fields then methods — is
                // parsed by the shared [`Parser::parse_struct_body`], so an
                // anonymous struct-type value accepts the **same** field+method
                // syntax as a named struct declaration (SPEC §26.1): the
                // fields-only v0.129 case yields `methods: vec![]`, while a
                // method-carrying generic struct (whose methods may use `Self`
                // and the type parameter) parses its `pub? fn …` tail here too.
                self.bump(); // `struct`
                self.expect_punct(&TokenKind::LBrace, "`{`")?;
                let (fields, methods) = self.parse_struct_body()?;
                let rbrace = self.expect_punct(&TokenKind::RBrace, "`}`")?;
                Ok(Expr::StructType {
                    fields,
                    methods,
                    span: tok.span.merge(rbrace),
                })
            }
            TokenKind::Keyword(Kw::Unreachable) => {
                // The `unreachable` keyword (SPEC §35) — a diverging primary
                // that asserts a path is impossible. It is a single keyword
                // with no operands, so it is its own primary expression. As a
                // statement it reaches here via the expr-statement path
                // (`unreachable;`); in a value position (`var x = unreachable;`,
                // a `switch`/`if` arm tail) it adopts the expected type in sema
                // and diverges. `@panic("…")` needs no parser arm — it already
                // parses through the `@ IDENT ( args )` rule below as
                // `Expr::Builtin{ name: "panic", args: [StrLit] }`.
                self.bump(); // `unreachable`
                Ok(Expr::Unreachable { span: tok.span })
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
            TokenKind::At => {
                // A comptime reflection builtin `@name(args)` in expression
                // position (SPEC §32.1): `@sizeOf(T)` → `usize`, `@typeName(T)`
                // → `[]u8`. The builtin *name* is an identifier after the `@`;
                // the parenthesised arguments are ordinary expressions (the type
                // argument is just an `Ident` naming a type, which sema resolves
                // — substitution-aware, like `alloc`'s type argument, SPEC §16).
                // The parser does not special-case the name beyond requiring the
                // `@ IDENT ( … )` shape; an unknown builtin name is a sema
                // concern. (`@import` is a top-level item, dispatched in
                // `parse_item` before any expression is parsed, and `@This()` is
                // a *type*, parsed in `parse_type` — neither reaches here.)
                self.bump(); // `@`
                let (name, _) = self.expect_ident()?;
                self.expect_punct(&TokenKind::LParen, "`(`")?;
                let args = self.parse_args()?;
                let rparen = self.expect_punct(&TokenKind::RParen, "`)`")?;
                Ok(Expr::Builtin {
                    name,
                    args,
                    span: tok.span.merge(rparen),
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

/// Classify an assignment-operator token for the `op` field of a
/// `Stmt::Assign` / `Stmt::FieldAssign` (SPEC §27.1, v0.131).
///
/// Returns `None` if `kind` is not an assignment operator. Otherwise the outer
/// `Some` marks an assignment, and the inner `Option<BinOp>` is `None` for a
/// plain `=` or `Some(binop)` for a compound `+= -= *= /= %=` (mapping to
/// `Add`/`Sub`/`Mul`/`Div`/`Rem`), which means `place = place op rhs`.
fn assign_op_kind(kind: &TokenKind) -> Option<Option<BinOp>> {
    match kind {
        TokenKind::Eq => Some(None),
        TokenKind::PlusEq => Some(Some(BinOp::Add)),
        TokenKind::MinusEq => Some(Some(BinOp::Sub)),
        TokenKind::StarEq => Some(Some(BinOp::Mul)),
        TokenKind::SlashEq => Some(Some(BinOp::Div)),
        TokenKind::PercentEq => Some(Some(BinOp::Rem)),
        _ => None,
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
        TokenKind::PlusEq => "`+=`".to_string(),
        TokenKind::MinusEq => "`-=`".to_string(),
        TokenKind::StarEq => "`*=`".to_string(),
        TokenKind::SlashEq => "`/=`".to_string(),
        TokenKind::PercentEq => "`%=`".to_string(),
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
        TokenKind::Caret => "`^`".to_string(),
        TokenKind::Tilde => "`~`".to_string(),
        TokenKind::Shl => "`<<`".to_string(),
        TokenKind::Shr => "`>>`".to_string(),
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

    // ---- v0.132: bitwise & shift operators (SPEC §28) ---------------------

    /// Each infix bitwise/shift operator parses to its `BinOp`.
    #[test]
    fn bitwise_infix_ops_parse() {
        for (tok, want) in [
            (TokenKind::Amp, BinOp::BitAnd),
            (TokenKind::Pipe, BinOp::BitOr),
            (TokenKind::Caret, BinOp::BitXor),
            (TokenKind::Shl, BinOp::Shl),
            (TokenKind::Shr, BinOp::Shr),
        ] {
            let e = parse_assign_rhs(vec![id("a"), tok.clone(), id("b")]);
            match e {
                Expr::Binary { op, .. } => assert_eq!(op, want, "for token {:?}", tok),
                other => panic!("expected binary for {:?}, got {:?}", tok, other),
            }
        }
    }

    /// `a << 2` / `a >> 1` parse with an integer right operand.
    #[test]
    fn shift_with_int_operand() {
        let e = parse_assign_rhs(vec![id("a"), TokenKind::Shl, TokenKind::Int(2)]);
        assert!(
            matches!(e, Expr::Binary { op: BinOp::Shl, .. }),
            "expected `a << 2` to be Shl, got {:?}",
            e
        );
        let e = parse_assign_rhs(vec![id("a"), TokenKind::Shr, TokenKind::Int(1)]);
        assert!(
            matches!(e, Expr::Binary { op: BinOp::Shr, .. }),
            "expected `a >> 1` to be Shr, got {:?}",
            e
        );
    }

    /// `~a` is the bitwise-complement prefix at the unary level.
    #[test]
    fn bitnot_prefix_parses() {
        let e = parse_assign_rhs(vec![TokenKind::Tilde, id("a")]);
        match e {
            Expr::Unary {
                op: UnOp::BitNot,
                expr,
                ..
            } => assert!(matches!(*expr, Expr::Ident { .. })),
            other => panic!("expected `~a` to be UnOp::BitNot, got {:?}", other),
        }
    }

    /// `~a.b` binds the prefix `~` over the whole postfix chain: `~(a.b)`.
    #[test]
    fn bitnot_binds_below_postfix() {
        let e = parse_assign_rhs(vec![TokenKind::Tilde, id("a"), TokenKind::Dot, id("b")]);
        match e {
            Expr::Unary {
                op: UnOp::BitNot,
                expr,
                ..
            } => assert!(matches!(*expr, Expr::Field { .. }), "got {:?}", expr),
            other => panic!("expected `~(a.b)`, got {:?}", other),
        }
    }

    /// `&` binds tighter than `|`: `a | b & c` ==> `a | (b & c)`.
    #[test]
    fn precedence_bitor_bitand() {
        let e = parse_assign_rhs(vec![
            id("a"),
            TokenKind::Pipe,
            id("b"),
            TokenKind::Amp,
            id("c"),
        ]);
        match e {
            Expr::Binary {
                op: BinOp::BitOr,
                rhs,
                ..
            } => assert!(
                matches!(*rhs, Expr::Binary { op: BinOp::BitAnd, .. }),
                "expected `&` on the right of `|`, got {:?}",
                rhs
            ),
            other => panic!("expected `|` at the root, got {:?}", other),
        }
    }

    /// `^` sits between `|` and `&`: `a | b ^ c` ==> `a | (b ^ c)`.
    #[test]
    fn precedence_bitor_bitxor() {
        let e = parse_assign_rhs(vec![
            id("a"),
            TokenKind::Pipe,
            id("b"),
            TokenKind::Caret,
            id("c"),
        ]);
        match e {
            Expr::Binary {
                op: BinOp::BitOr,
                rhs,
                ..
            } => assert!(
                matches!(*rhs, Expr::Binary { op: BinOp::BitXor, .. }),
                "expected `^` on the right of `|`, got {:?}",
                rhs
            ),
            other => panic!("expected `|` at the root, got {:?}", other),
        }
    }

    /// `&` binds tighter than `^`: `a ^ b & c` ==> `a ^ (b & c)`.
    #[test]
    fn precedence_bitxor_bitand() {
        let e = parse_assign_rhs(vec![
            id("a"),
            TokenKind::Caret,
            id("b"),
            TokenKind::Amp,
            id("c"),
        ]);
        match e {
            Expr::Binary {
                op: BinOp::BitXor,
                rhs,
                ..
            } => assert!(
                matches!(*rhs, Expr::Binary { op: BinOp::BitAnd, .. }),
                "expected `&` on the right of `^`, got {:?}",
                rhs
            ),
            other => panic!("expected `^` at the root, got {:?}", other),
        }
    }

    /// Additive binds tighter than shift: `1 << 2 + 3` ==> `1 << (2 + 3)`.
    #[test]
    fn precedence_shift_below_additive() {
        let e = parse_assign_rhs(vec![
            TokenKind::Int(1),
            TokenKind::Shl,
            TokenKind::Int(2),
            TokenKind::Plus,
            TokenKind::Int(3),
        ]);
        match e {
            Expr::Binary {
                op: BinOp::Shl,
                rhs,
                ..
            } => assert!(
                matches!(*rhs, Expr::Binary { op: BinOp::Add, .. }),
                "expected `+` on the right of `<<`, got {:?}",
                rhs
            ),
            other => panic!("expected `<<` at the root, got {:?}", other),
        }
    }

    /// Equality binds tighter than `&` (which is below `==`): `x & y == z`
    /// ==> `x & (y == z)`.
    #[test]
    fn precedence_bitand_below_equality() {
        let e = parse_assign_rhs(vec![
            id("x"),
            TokenKind::Amp,
            id("y"),
            TokenKind::EqEq,
            id("z"),
        ]);
        match e {
            Expr::Binary {
                op: BinOp::BitAnd,
                rhs,
                ..
            } => assert!(
                matches!(*rhs, Expr::Binary { op: BinOp::Eq, .. }),
                "expected `==` on the right of `&`, got {:?}",
                rhs
            ),
            other => panic!("expected `&` at the root, got {:?}", other),
        }
    }

    /// Relational binds tighter than equality: `a == b < c` ==> `a == (b < c)`.
    #[test]
    fn precedence_equality_below_relational() {
        let e = parse_assign_rhs(vec![
            id("a"),
            TokenKind::EqEq,
            id("b"),
            TokenKind::Lt,
            id("c"),
        ]);
        match e {
            Expr::Binary {
                op: BinOp::Eq,
                rhs,
                ..
            } => assert!(
                matches!(*rhs, Expr::Binary { op: BinOp::Lt, .. }),
                "expected `<` on the right of `==`, got {:?}",
                rhs
            ),
            other => panic!("expected `==` at the root, got {:?}", other),
        }
    }

    /// Shift binds tighter than relational: `a < b << c` ==> `a < (b << c)`.
    #[test]
    fn precedence_relational_below_shift() {
        let e = parse_assign_rhs(vec![
            id("a"),
            TokenKind::Lt,
            id("b"),
            TokenKind::Shl,
            id("c"),
        ]);
        match e {
            Expr::Binary {
                op: BinOp::Lt,
                rhs,
                ..
            } => assert!(
                matches!(*rhs, Expr::Binary { op: BinOp::Shl, .. }),
                "expected `<<` on the right of `<`, got {:?}",
                rhs
            ),
            other => panic!("expected `<` at the root, got {:?}", other),
        }
    }

    /// `const MASK = (1 << 8) - 1;`-style nesting: parenthesised shift then
    /// additive. `(1 << 8) - 1` ==> `(1 << 8) - 1`.
    #[test]
    fn parenthesised_shift_then_sub() {
        let e = parse_assign_rhs(vec![
            TokenKind::LParen,
            TokenKind::Int(1),
            TokenKind::Shl,
            TokenKind::Int(8),
            TokenKind::RParen,
            TokenKind::Minus,
            TokenKind::Int(1),
        ]);
        match e {
            Expr::Binary {
                op: BinOp::Sub,
                lhs,
                ..
            } => assert!(
                matches!(*lhs, Expr::Binary { op: BinOp::Shl, .. }),
                "expected `<<` on the left of `-`, got {:?}",
                lhs
            ),
            other => panic!("expected `-` at the root, got {:?}", other),
        }
    }

    /// REGRESSION: prefix `&x` is still address-of, never an infix bitand —
    /// an infix `&` only applies *between* operands. `a & &b` ==>
    /// `BitAnd(a, AddrOf(b))`.
    #[test]
    fn infix_bitand_vs_prefix_addrof() {
        let e = parse_assign_rhs(vec![id("a"), TokenKind::Amp, TokenKind::Amp, id("b")]);
        match e {
            Expr::Binary {
                op: BinOp::BitAnd,
                lhs,
                rhs,
                ..
            } => {
                assert!(matches!(*lhs, Expr::Ident { .. }), "lhs is `a`");
                assert!(
                    matches!(*rhs, Expr::AddrOf { .. }),
                    "rhs is address-of `&b`, got {:?}",
                    rhs
                );
            }
            other => panic!("expected `a & (&b)`, got {:?}", other),
        }
    }

    /// REGRESSION: a leading prefix `&x` with no following infix `&` is still
    /// a bare address-of (unchanged from SPEC §15.1).
    #[test]
    fn prefix_addrof_still_works() {
        let e = parse_assign_rhs(vec![TokenKind::Amp, id("y")]);
        assert!(
            matches!(e, Expr::AddrOf { .. }),
            "expected `&y` address-of, got {:?}",
            e
        );
    }

    /// REGRESSION: a capture `if (cond) |v| {}` still binds `v`, and an infix
    /// `|` *inside* the parenthesised condition is a `BitOr` — the two `|`
    /// positions never collide. `if (a | b) |v| { }`.
    #[test]
    fn if_capture_coexists_with_infix_bitor() {
        let stmts = body_stmts(vec![
            TokenKind::Keyword(Kw::If),
            TokenKind::LParen,
            id("a"),
            TokenKind::Pipe,
            id("b"),
            TokenKind::RParen,
            TokenKind::Pipe,
            id("v"),
            TokenKind::Pipe,
            TokenKind::LBrace,
            TokenKind::RBrace,
        ]);
        match &stmts[0] {
            Stmt::If { cond, capture, .. } => {
                assert_eq!(capture.as_deref(), Some("v"), "capture binds `v`");
                assert!(
                    matches!(cond, Expr::Binary { op: BinOp::BitOr, .. }),
                    "condition `a | b` is BitOr, got {:?}",
                    cond
                );
            }
            other => panic!("expected if-capture, got {:?}", other),
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

    /// Parse `fn f() void { <body> }` and return a clone of its first statement.
    fn first_stmt(body: Vec<TokenKind>) -> Stmt {
        let mut kinds = vec![
            TokenKind::Keyword(Kw::Fn),
            id("f"),
            TokenKind::LParen,
            TokenKind::RParen,
            id("void"),
            TokenKind::LBrace,
        ];
        kinds.extend(body);
        kinds.push(TokenKind::RBrace);
        let m = parse(&toks(kinds)).expect("should parse");
        match &m.items[0] {
            Item::Func(f) => f.body.stmts[0].clone(),
            other => panic!("expected func, got {:?}", other),
        }
    }

    #[test]
    fn plain_assign_has_no_op() {
        // `x = 1;` → Stmt::Assign { name: x, op: None } (SPEC §27.1).
        match first_stmt(vec![
            id("x"),
            TokenKind::Eq,
            TokenKind::Int(1),
            TokenKind::Semicolon,
        ]) {
            Stmt::Assign { name, op, .. } => {
                assert_eq!(name, "x");
                assert_eq!(op, None);
            }
            other => panic!("expected assign, got {:?}", other),
        }
    }

    #[test]
    fn compound_assign_add_to_name() {
        // `x += 1;` → Stmt::Assign { name: x, op: Some(Add) } (SPEC §27.1).
        match first_stmt(vec![
            id("x"),
            TokenKind::PlusEq,
            TokenKind::Int(1),
            TokenKind::Semicolon,
        ]) {
            Stmt::Assign { name, op, .. } => {
                assert_eq!(name, "x");
                assert_eq!(op, Some(BinOp::Add));
            }
            other => panic!("expected compound assign, got {:?}", other),
        }
    }

    #[test]
    fn compound_assign_all_ops_to_name() {
        // Each compound token maps to the matching BinOp (SPEC §27).
        for (tok, want) in [
            (TokenKind::PlusEq, BinOp::Add),
            (TokenKind::MinusEq, BinOp::Sub),
            (TokenKind::StarEq, BinOp::Mul),
            (TokenKind::SlashEq, BinOp::Div),
            (TokenKind::PercentEq, BinOp::Rem),
        ] {
            match first_stmt(vec![
                id("x"),
                tok.clone(),
                TokenKind::Int(2),
                TokenKind::Semicolon,
            ]) {
                Stmt::Assign { op, .. } => assert_eq!(op, Some(want), "token {:?}", tok),
                other => panic!("expected assign for {:?}, got {:?}", tok, other),
            }
        }
    }

    #[test]
    fn compound_index_place_mul() {
        // `a[i] *= 2;` → Stmt::FieldAssign { place: Index, op: Some(Mul) }.
        match first_stmt(vec![
            id("a"),
            TokenKind::LBracket,
            id("i"),
            TokenKind::RBracket,
            TokenKind::StarEq,
            TokenKind::Int(2),
            TokenKind::Semicolon,
        ]) {
            Stmt::FieldAssign { place, op, .. } => {
                assert!(matches!(place, Expr::Index { .. }));
                assert_eq!(op, Some(BinOp::Mul));
            }
            other => panic!("expected field assign, got {:?}", other),
        }
    }

    #[test]
    fn compound_field_place_sub() {
        // `s.f -= 3;` → Stmt::FieldAssign { place: Field, op: Some(Sub) }.
        match first_stmt(vec![
            id("s"),
            TokenKind::Dot,
            id("f"),
            TokenKind::MinusEq,
            TokenKind::Int(3),
            TokenKind::Semicolon,
        ]) {
            Stmt::FieldAssign { place, op, .. } => {
                assert!(matches!(place, Expr::Field { .. }));
                assert_eq!(op, Some(BinOp::Sub));
            }
            other => panic!("expected field assign, got {:?}", other),
        }
    }

    #[test]
    fn plain_field_assign_has_no_op() {
        // `s.f = 3;` keeps op: None (the plain `=` path is unchanged).
        match first_stmt(vec![
            id("s"),
            TokenKind::Dot,
            id("f"),
            TokenKind::Eq,
            TokenKind::Int(3),
            TokenKind::Semicolon,
        ]) {
            Stmt::FieldAssign { op, .. } => assert_eq!(op, None),
            other => panic!("expected field assign, got {:?}", other),
        }
    }

    #[test]
    fn compound_deref_place_div_rem() {
        // `p.* /= 4;` and `p.* %= 4;` → FieldAssign over a Deref place.
        for (tok, want) in [(TokenKind::SlashEq, BinOp::Div), (TokenKind::PercentEq, BinOp::Rem)] {
            match first_stmt(vec![
                id("p"),
                TokenKind::Dot,
                TokenKind::Star,
                tok.clone(),
                TokenKind::Int(4),
                TokenKind::Semicolon,
            ]) {
                Stmt::FieldAssign { place, op, .. } => {
                    assert!(matches!(place, Expr::Deref { .. }));
                    assert_eq!(op, Some(want), "token {:?}", tok);
                }
                other => panic!("expected field assign for {:?}, got {:?}", tok, other),
            }
        }
    }

    #[test]
    fn while_continue_clause_compound() {
        // `while (c) : (i += 1) { }` — the continue-clause threads the op too.
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Fn),
            id("f"),
            TokenKind::LParen,
            TokenKind::RParen,
            id("void"),
            TokenKind::LBrace,
            TokenKind::Keyword(Kw::While),
            TokenKind::LParen,
            id("c"),
            TokenKind::RParen,
            TokenKind::Colon,
            TokenKind::LParen,
            id("i"),
            TokenKind::PlusEq,
            TokenKind::Int(1),
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
        match &body.stmts[0] {
            Stmt::While { cont: Some(cont), .. } => match cont.as_ref() {
                Stmt::Assign { name, op, .. } => {
                    assert_eq!(name, "i");
                    assert_eq!(*op, Some(BinOp::Add));
                }
                other => panic!("expected assign continue-clause, got {:?}", other),
            },
            other => panic!("expected while with continue clause, got {:?}", other),
        }
    }

    #[test]
    fn index_expr_statement_not_assign() {
        // `a[i];` is a plain expression statement, not a (compound) assignment.
        match first_stmt(vec![
            id("a"),
            TokenKind::LBracket,
            id("i"),
            TokenKind::RBracket,
            TokenKind::Semicolon,
        ]) {
            Stmt::Expr(Expr::Index { .. }) => {}
            other => panic!("expected index expr stmt, got {:?}", other),
        }
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

    // ---- v0.139 named error sets ----------------------------------------

    #[test]
    fn error_set_decl_basic() {
        // const E = error{ A, B };  ==>  Item::ErrorSet{ name: "E", members: [A, B] }
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Const),
            id("E"),
            TokenKind::Eq,
            TokenKind::Keyword(Kw::Error),
            TokenKind::LBrace,
            id("A"),
            TokenKind::Comma,
            id("B"),
            TokenKind::RBrace,
            TokenKind::Semicolon,
        ]))
        .expect("should parse");
        match &m.items[0] {
            Item::ErrorSet(es) => {
                assert!(!es.is_pub, "no `pub` keyword");
                assert_eq!(es.name, "E");
                assert_eq!(es.members, vec!["A".to_string(), "B".to_string()]);
                assert!(es.span.start < es.span.end);
            }
            other => panic!("expected error-set decl, got {:?}", other),
        }
    }

    #[test]
    fn error_set_decl_pub_single_trailing_comma() {
        // pub const F = error{ X, };  — `pub`, one member, trailing comma.
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Pub),
            TokenKind::Keyword(Kw::Const),
            id("F"),
            TokenKind::Eq,
            TokenKind::Keyword(Kw::Error),
            TokenKind::LBrace,
            id("X"),
            TokenKind::Comma,
            TokenKind::RBrace,
            TokenKind::Semicolon,
        ]))
        .expect("should parse");
        match &m.items[0] {
            Item::ErrorSet(es) => {
                assert!(es.is_pub, "`pub const F = error{{ X }};` is public");
                assert_eq!(es.name, "F");
                assert_eq!(es.members, vec!["X".to_string()]);
            }
            other => panic!("expected error-set decl, got {:?}", other),
        }
    }

    #[test]
    fn named_error_union_return_type() {
        // fn f() E!i32 { return 0; }  — `E!i32` is an error union over the
        // named set `E`, payload `i32`. `error_set` is Some("E"); the payload
        // name is "i32".
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Fn),
            id("f"),
            TokenKind::LParen,
            TokenKind::RParen,
            id("E"),
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
                assert!(f.ret.error_union, "`E!i32` is an error union");
                assert_eq!(
                    f.ret.error_set,
                    Some("E".to_string()),
                    "`E!i32` carries the named set `E`"
                );
                assert_eq!(f.ret.name, "i32", "payload type is `i32`");
                assert!(!f.ret.optional, "not optional");
                assert!(f.ret.span.start < f.ret.span.end);
            }
            other => panic!("expected func, got {:?}", other),
        }
    }

    #[test]
    fn global_error_union_has_no_named_set() {
        // fn f() !i32 { ... }  — the prefix `!` form keeps `error_set: None`
        // (the implicit global error set), unchanged by v0.139.
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
                assert!(f.ret.error_union, "`!i32` is an error union");
                assert_eq!(f.ret.error_set, None, "`!i32` has no named set");
                assert_eq!(f.ret.name, "i32");
            }
            other => panic!("expected func, got {:?}", other),
        }
    }

    #[test]
    fn named_error_union_local_var() {
        // var x: E!i32 = ...; inside a fn — the `Set!T` form is recognised in
        // any type position, not just returns.
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
            id("FileErr"),
            TokenKind::Bang,
            id("i32"),
            TokenKind::Eq,
            TokenKind::Int(0),
            TokenKind::Semicolon,
            TokenKind::RBrace,
        ]))
        .expect("should parse");
        match &m.items[0] {
            Item::Func(f) => match &f.body.stmts[0] {
                Stmt::Let { ty: Some(ty), .. } => {
                    assert!(ty.error_union, "`FileErr!i32` is an error union");
                    assert_eq!(ty.error_set, Some("FileErr".to_string()));
                    assert_eq!(ty.name, "i32");
                }
                other => panic!("expected `let`, got {:?}", other),
            },
            other => panic!("expected func, got {:?}", other),
        }
    }

    #[test]
    fn error_literal_const_is_not_a_set_decl() {
        // const C = error.A;  — `= error .` (not `error {`) stays an inferred
        // value const whose initializer is an `Expr::ErrorLit`, NOT a set decl.
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Const),
            id("C"),
            TokenKind::Eq,
            TokenKind::Keyword(Kw::Error),
            TokenKind::Dot,
            id("A"),
            TokenKind::Semicolon,
        ]))
        .expect("should parse");
        match &m.items[0] {
            Item::Const(c) => {
                assert_eq!(c.name, "C");
                assert!(c.ty.is_none(), "inferred const, no annotation");
                match &c.value {
                    Expr::ErrorLit { name, .. } => assert_eq!(name, "A"),
                    other => panic!("expected error literal, got {:?}", other),
                }
            }
            other => panic!("expected value const, got {:?}", other),
        }
    }

    #[test]
    fn error_dot_expression_still_parses_after_v0139() {
        // Regression: `error.A` in expression position is unchanged by v0.139.
        let e = parse_assign_rhs(vec![
            TokenKind::Keyword(Kw::Error),
            TokenKind::Dot,
            id("A"),
        ]);
        match e {
            Expr::ErrorLit { name, .. } => assert_eq!(name, "A"),
            other => panic!("expected error literal, got {:?}", other),
        }
    }

    #[test]
    fn empty_error_set_decl() {
        // const Empty = error{};  — a degenerate but well-formed empty set.
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Const),
            id("Empty"),
            TokenKind::Eq,
            TokenKind::Keyword(Kw::Error),
            TokenKind::LBrace,
            TokenKind::RBrace,
            TokenKind::Semicolon,
        ]))
        .expect("should parse");
        match &m.items[0] {
            Item::ErrorSet(es) => {
                assert_eq!(es.name, "Empty");
                assert!(es.members.is_empty());
            }
            other => panic!("expected error-set decl, got {:?}", other),
        }
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
                assert_eq!(
                    ty.array_len,
                    Some(ArraySize::Lit(3)),
                    "`[3]i32` must set array_len"
                );
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
                assert_eq!(ty.array_len, Some(ArraySize::Lit(2)));
                match value {
                    Expr::ArrayLit { elem, elems, .. } => {
                        assert_eq!(elem.name, "i32");
                        assert_eq!(elem.array_len, Some(ArraySize::Lit(2)));
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
                assert_eq!(elem.array_len, Some(ArraySize::Lit(3)));
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

    // ---- v0.128: comptime value parameters (`[n]T`, `comptime n: usize`) ---

    #[test]
    fn array_type_named_size_is_param() {
        // fn f(a: [n]i32) void { }  — `[n]i32` sets array_len=Some(Param("n")),
        // name=i32 (SPEC §24.1: the size is a comptime value-parameter name).
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Fn),
            id("f"),
            TokenKind::LParen,
            id("a"),
            TokenKind::Colon,
            TokenKind::LBracket,
            id("n"),
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
                assert_eq!(ty.name, "i32");
                assert_eq!(
                    ty.array_len,
                    Some(ArraySize::Param("n".to_string())),
                    "`[n]i32` must set array_len to a Param size"
                );
                assert!(!ty.slice, "`[n]i32` is an array, not a slice");
                assert!(!ty.pointer);
                assert!(!ty.optional);
                assert!(!ty.error_union);
            }
            other => panic!("expected func, got {:?}", other),
        }
    }

    #[test]
    fn comptime_value_param_and_named_return_array() {
        // fn zeros(comptime n: usize) [n]i32 { return [n]i32{}; }
        // The parameter is comptime (a value parameter, annotation `usize`), and
        // the return type is `[n]i32` with a Param array size (SPEC §24.1).
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Fn),
            id("zeros"),
            TokenKind::LParen,
            TokenKind::Keyword(Kw::Comptime),
            id("n"),
            TokenKind::Colon,
            id("usize"),
            TokenKind::RParen,
            // return type `[n]i32`
            TokenKind::LBracket,
            id("n"),
            TokenKind::RBracket,
            id("i32"),
            TokenKind::LBrace,
            TokenKind::Keyword(Kw::Return),
            // `[n]i32{}` — an array literal whose element type carries the Param
            // size; the count-vs-N check is a sema concern.
            TokenKind::LBracket,
            id("n"),
            TokenKind::RBracket,
            id("i32"),
            TokenKind::LBrace,
            TokenKind::RBrace,
            TokenKind::Semicolon,
            TokenKind::RBrace,
        ]))
        .expect("`fn zeros(comptime n: usize) [n]i32` should parse");
        match &m.items[0] {
            Item::Func(f) => {
                assert_eq!(f.name, "zeros");
                assert_eq!(f.params.len(), 1);
                let p = &f.params[0];
                assert_eq!(p.name, "n");
                assert!(p.is_comptime, "`comptime n: usize` is a comptime param");
                assert_eq!(p.ty.name, "usize");
                assert_eq!(p.ty.array_len, None, "the annotation `usize` is scalar");
                // Return type `[n]i32`.
                assert_eq!(f.ret.name, "i32");
                assert_eq!(
                    f.ret.array_len,
                    Some(ArraySize::Param("n".to_string())),
                    "return `[n]i32` must carry a Param size"
                );
                // The array-literal initializer's element type also parses `[n]`.
                match &f.body.stmts[0] {
                    Stmt::Return {
                        value: Some(Expr::ArrayLit { elem, .. }),
                        ..
                    } => {
                        assert_eq!(elem.name, "i32");
                        assert_eq!(elem.array_len, Some(ArraySize::Param("n".to_string())));
                    }
                    other => panic!("expected `return [n]i32{{}}`, got {:?}", other),
                }
            }
            other => panic!("expected func, got {:?}", other),
        }
    }

    #[test]
    fn array_size_must_be_int_or_ident() {
        // `[+]i32` (a non-int, non-ident array size) is rejected with E0200: the
        // parser requires either an integer literal or a parameter name there.
        let err = parse(&toks(vec![
            TokenKind::Keyword(Kw::Fn),
            id("f"),
            TokenKind::LParen,
            id("a"),
            TokenKind::Colon,
            TokenKind::LBracket,
            TokenKind::Plus,
            TokenKind::RBracket,
            id("i32"),
            TokenKind::RParen,
            id("void"),
            TokenKind::LBrace,
            TokenKind::RBrace,
        ]))
        .expect_err("`[+]i32` should fail");
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
                assert_eq!(ty.array_len, Some(ArraySize::Lit(3)));
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

    // ---- v0.129: generic structs / type-returning functions --------------

    /// A type-constructor `fn B(comptime T: type) type { return struct { v: T
    /// }; }` parses: the return type is the bare ident `type`, the comptime
    /// type parameter parses as before, and the `return`-expression body is an
    /// `Expr::StructType` with one field `v: T` (SPEC §25.1).
    #[test]
    fn type_constructor_returns_struct_type() {
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Fn),
            id("B"),
            TokenKind::LParen,
            TokenKind::Keyword(Kw::Comptime),
            id("T"),
            TokenKind::Colon,
            id("type"),
            TokenKind::RParen,
            id("type"), // bare `type` return type
            TokenKind::LBrace,
            TokenKind::Keyword(Kw::Return),
            TokenKind::Keyword(Kw::Struct),
            TokenKind::LBrace,
            id("v"),
            TokenKind::Colon,
            id("T"),
            TokenKind::RBrace,
            TokenKind::Semicolon,
            TokenKind::RBrace,
        ]))
        .expect("should parse");
        match &m.items[0] {
            Item::Func(f) => {
                assert_eq!(f.name, "B");
                assert_eq!(f.ret.name, "type");
                assert!(!f.ret.optional && !f.ret.error_union);
                assert_eq!(f.params.len(), 1);
                assert!(f.params[0].is_comptime);
                assert_eq!(f.params[0].ty.name, "type");
                assert_eq!(f.body.stmts.len(), 1);
                match &f.body.stmts[0] {
                    Stmt::Return {
                        value: Some(Expr::StructType { fields, .. }),
                        ..
                    } => {
                        assert_eq!(fields.len(), 1);
                        assert_eq!(fields[0].name, "v");
                        assert_eq!(fields[0].ty.name, "T");
                    }
                    other => panic!("expected `return struct {{ v: T }};`, got {:?}", other),
                }
            }
            other => panic!("expected func, got {:?}", other),
        }
    }

    /// A `struct { … }` type value with multiple fields (composite field types)
    /// and a trailing comma parses — field types reuse `parse_type`, so a slice
    /// field `[]T` works just like in a struct declaration (SPEC §25.1).
    #[test]
    fn struct_type_value_multi_field_composite() {
        // fn L(comptime T: type) type { return struct { items: []T, n: i32, }; }
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Fn),
            id("L"),
            TokenKind::LParen,
            TokenKind::Keyword(Kw::Comptime),
            id("T"),
            TokenKind::Colon,
            id("type"),
            TokenKind::RParen,
            id("type"),
            TokenKind::LBrace,
            TokenKind::Keyword(Kw::Return),
            TokenKind::Keyword(Kw::Struct),
            TokenKind::LBrace,
            id("items"),
            TokenKind::Colon,
            TokenKind::LBracket,
            TokenKind::RBracket,
            id("T"),
            TokenKind::Comma,
            id("n"),
            TokenKind::Colon,
            id("i32"),
            TokenKind::Comma, // trailing comma
            TokenKind::RBrace,
            TokenKind::Semicolon,
            TokenKind::RBrace,
        ]))
        .expect("should parse");
        match &m.items[0] {
            Item::Func(f) => match &f.body.stmts[0] {
                Stmt::Return {
                    value: Some(Expr::StructType { fields, .. }),
                    ..
                } => {
                    assert_eq!(fields.len(), 2);
                    assert_eq!(fields[0].name, "items");
                    assert_eq!(fields[0].ty.name, "T");
                    assert!(fields[0].ty.slice);
                    assert_eq!(fields[1].name, "n");
                    assert_eq!(fields[1].ty.name, "i32");
                }
                other => panic!("expected struct-type return, got {:?}", other),
            },
            other => panic!("expected func, got {:?}", other),
        }
    }

    /// An empty `struct {}` type value parses to an `Expr::StructType` with no
    /// fields (mirrors the empty-struct declaration form, SPEC §25.1).
    #[test]
    fn empty_struct_type_value() {
        // fn U(comptime T: type) type { return struct {}; }
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Fn),
            id("U"),
            TokenKind::LParen,
            TokenKind::Keyword(Kw::Comptime),
            id("T"),
            TokenKind::Colon,
            id("type"),
            TokenKind::RParen,
            id("type"),
            TokenKind::LBrace,
            TokenKind::Keyword(Kw::Return),
            TokenKind::Keyword(Kw::Struct),
            TokenKind::LBrace,
            TokenKind::RBrace,
            TokenKind::Semicolon,
            TokenKind::RBrace,
        ]))
        .expect("should parse");
        match &m.items[0] {
            Item::Func(f) => match &f.body.stmts[0] {
                Stmt::Return {
                    value: Some(Expr::StructType { fields, .. }),
                    ..
                } => assert!(fields.is_empty()),
                other => panic!("expected empty struct-type return, got {:?}", other),
            },
            other => panic!("expected func, got {:?}", other),
        }
    }

    /// A type alias `const IL = B(i32);` parses as an ordinary (inferred) value
    /// `const` whose initializer is an `Expr::Call` (no new syntax — SPEC
    /// §25.1). sema later recognises the callee as a type-constructor.
    #[test]
    fn type_alias_const_parses_as_call() {
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Const),
            id("IL"),
            TokenKind::Eq,
            id("B"),
            TokenKind::LParen,
            id("i32"),
            TokenKind::RParen,
            TokenKind::Semicolon,
        ]))
        .expect("should parse");
        match &m.items[0] {
            Item::Const(c) => {
                assert_eq!(c.name, "IL");
                assert!(c.ty.is_none(), "inferred const carries no annotation");
                match &c.value {
                    Expr::Call { callee, args, .. } => {
                        assert_eq!(callee, "B");
                        assert_eq!(args.len(), 1);
                        assert!(matches!(&args[0], Expr::Ident { name, .. } if name == "i32"));
                    }
                    other => panic!("expected call `B(i32)`, got {:?}", other),
                }
            }
            other => panic!("expected const, got {:?}", other),
        }
    }

    /// A top-level `const P = struct { x: i32 };` must remain a **named struct
    /// declaration** (`Item::Struct`), not an `Expr::StructType` — the `=
    /// struct` item-position dispatch in `parse_const` still wins over the new
    /// expression-position `struct` form (SPEC §25.1 caution; §9.1).
    #[test]
    fn top_level_struct_decl_not_misparsed_as_struct_type() {
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
                assert_eq!(s.fields[0].ty.name, "i32");
                assert!(s.methods.is_empty());
            }
            other => panic!("expected named struct decl, got {:?}", other),
        }
    }

    // ---- v0.130: generic-struct methods ----------------------------------

    /// A type-constructor whose `struct { … }` declares **methods** after its
    /// fields parses to an `Expr::StructType` carrying both (SPEC §26.1):
    /// `fn L(comptime T: type) type { return struct { items: []T, n: usize, fn
    /// len(self: Self) usize { return self.n; } }; }`. The struct-type value
    /// has 2 fields and 1 method `len` whose first parameter is `self: Self`
    /// (the contextual self-type name, resolved in sema). The method tail
    /// reuses the same parsing as a named struct declaration's methods.
    #[test]
    fn generic_struct_with_methods() {
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Fn),
            id("L"),
            TokenKind::LParen,
            TokenKind::Keyword(Kw::Comptime),
            id("T"),
            TokenKind::Colon,
            id("type"),
            TokenKind::RParen,
            id("type"),
            TokenKind::LBrace,
            TokenKind::Keyword(Kw::Return),
            TokenKind::Keyword(Kw::Struct),
            TokenKind::LBrace,
            // fields: items: []T, n: usize,
            id("items"),
            TokenKind::Colon,
            TokenKind::LBracket,
            TokenKind::RBracket,
            id("T"),
            TokenKind::Comma,
            id("n"),
            TokenKind::Colon,
            id("usize"),
            TokenKind::Comma,
            // method: fn len(self: Self) usize { return self.n; }
            TokenKind::Keyword(Kw::Fn),
            id("len"),
            TokenKind::LParen,
            id("self"),
            TokenKind::Colon,
            id("Self"),
            TokenKind::RParen,
            id("usize"),
            TokenKind::LBrace,
            TokenKind::Keyword(Kw::Return),
            id("self"),
            TokenKind::Dot,
            id("n"),
            TokenKind::Semicolon,
            TokenKind::RBrace, // method body
            TokenKind::RBrace, // struct body
            TokenKind::Semicolon,
            TokenKind::RBrace, // fn body
        ]))
        .expect("should parse");
        match &m.items[0] {
            Item::Func(f) => match &f.body.stmts[0] {
                Stmt::Return {
                    value: Some(Expr::StructType { fields, methods, .. }),
                    ..
                } => {
                    assert_eq!(fields.len(), 2);
                    assert_eq!(fields[0].name, "items");
                    assert!(fields[0].ty.slice);
                    assert_eq!(fields[0].ty.name, "T");
                    assert_eq!(fields[1].name, "n");
                    assert_eq!(fields[1].ty.name, "usize");
                    assert_eq!(methods.len(), 1);
                    let len = &methods[0];
                    assert_eq!(len.name, "len");
                    assert!(!len.is_pub);
                    assert_eq!(len.ret.name, "usize");
                    assert_eq!(len.params.len(), 1);
                    assert_eq!(len.params[0].name, "self");
                    assert_eq!(len.params[0].ty.name, "Self");
                    assert!(!len.params[0].ty.pointer);
                    assert_eq!(len.body.stmts.len(), 1);
                }
                other => panic!("expected struct-type return, got {:?}", other),
            },
            other => panic!("expected func, got {:?}", other),
        }
    }

    /// A v0.129 fields-only `struct { v: T }` type value still parses with an
    /// **empty** method list — adding the method tail must not change the
    /// fields-only behaviour (SPEC §26.1: a `StructType` with empty methods
    /// behaves exactly as v0.129).
    #[test]
    fn fields_only_struct_type_has_empty_methods() {
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Fn),
            id("B"),
            TokenKind::LParen,
            TokenKind::Keyword(Kw::Comptime),
            id("T"),
            TokenKind::Colon,
            id("type"),
            TokenKind::RParen,
            id("type"),
            TokenKind::LBrace,
            TokenKind::Keyword(Kw::Return),
            TokenKind::Keyword(Kw::Struct),
            TokenKind::LBrace,
            id("v"),
            TokenKind::Colon,
            id("T"),
            TokenKind::RBrace,
            TokenKind::Semicolon,
            TokenKind::RBrace,
        ]))
        .expect("should parse");
        match &m.items[0] {
            Item::Func(f) => match &f.body.stmts[0] {
                Stmt::Return {
                    value: Some(Expr::StructType { fields, methods, .. }),
                    ..
                } => {
                    assert_eq!(fields.len(), 1);
                    assert!(methods.is_empty());
                }
                other => panic!("expected struct-type return, got {:?}", other),
            },
            other => panic!("expected func, got {:?}", other),
        }
    }

    /// A generic struct may declare a `pub` method with a `*Self` pointer
    /// receiver and an associated function (no `self`), interleaved fields and
    /// methods just like a named struct declaration (SPEC §26.1). The parser
    /// treats `*Self` as an ordinary pointer type and a no-`self` `fn` as an
    /// ordinary function; method/associated-function distinction is sema's job.
    #[test]
    fn generic_struct_pub_and_pointer_self_and_assoc() {
        // return struct {
        //     n: usize,
        //     pub fn inc(self: *Self) void { return; }
        //     fn empty() usize { return 0; }
        // };
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Fn),
            id("L"),
            TokenKind::LParen,
            TokenKind::Keyword(Kw::Comptime),
            id("T"),
            TokenKind::Colon,
            id("type"),
            TokenKind::RParen,
            id("type"),
            TokenKind::LBrace,
            TokenKind::Keyword(Kw::Return),
            TokenKind::Keyword(Kw::Struct),
            TokenKind::LBrace,
            id("n"),
            TokenKind::Colon,
            id("usize"),
            TokenKind::Comma,
            // pub fn inc(self: *Self) void { return; }
            TokenKind::Keyword(Kw::Pub),
            TokenKind::Keyword(Kw::Fn),
            id("inc"),
            TokenKind::LParen,
            id("self"),
            TokenKind::Colon,
            TokenKind::Star,
            id("Self"),
            TokenKind::RParen,
            id("void"),
            TokenKind::LBrace,
            TokenKind::Keyword(Kw::Return),
            TokenKind::Semicolon,
            TokenKind::RBrace,
            // fn empty() usize { return 0; }
            TokenKind::Keyword(Kw::Fn),
            id("empty"),
            TokenKind::LParen,
            TokenKind::RParen,
            id("usize"),
            TokenKind::LBrace,
            TokenKind::Keyword(Kw::Return),
            TokenKind::Int(0),
            TokenKind::Semicolon,
            TokenKind::RBrace,
            TokenKind::RBrace, // struct body
            TokenKind::Semicolon,
            TokenKind::RBrace, // fn body
        ]))
        .expect("should parse");
        match &m.items[0] {
            Item::Func(f) => match &f.body.stmts[0] {
                Stmt::Return {
                    value: Some(Expr::StructType { fields, methods, .. }),
                    ..
                } => {
                    assert_eq!(fields.len(), 1);
                    assert_eq!(methods.len(), 2);
                    let inc = &methods[0];
                    assert_eq!(inc.name, "inc");
                    assert!(inc.is_pub);
                    assert_eq!(inc.params.len(), 1);
                    assert_eq!(inc.params[0].name, "self");
                    assert_eq!(inc.params[0].ty.name, "Self");
                    assert!(inc.params[0].ty.pointer, "`*Self` is a pointer receiver");
                    let empty = &methods[1];
                    assert_eq!(empty.name, "empty");
                    assert!(!empty.is_pub);
                    assert!(empty.params.is_empty(), "associated fn has no self");
                }
                other => panic!("expected struct-type return, got {:?}", other),
            },
            other => panic!("expected func, got {:?}", other),
        }
    }

    // ---- v0.133: for loops over arrays & slices ---------------------------

    #[test]
    fn for_simple_no_index() {
        // for (xs) |x| {}  →  Stmt::For { elem: x, index: None }.
        match first_stmt(vec![
            TokenKind::Keyword(Kw::For),
            TokenKind::LParen,
            id("xs"),
            TokenKind::RParen,
            TokenKind::Pipe,
            id("x"),
            TokenKind::Pipe,
            TokenKind::LBrace,
            TokenKind::RBrace,
        ]) {
            Stmt::For {
                iter,
                elem,
                index,
                body,
                ..
            } => {
                assert!(matches!(iter, Expr::Ident { ref name, .. } if name == "xs"));
                assert_eq!(elem, "x");
                assert_eq!(index, None, "no `, 0..` → no index binding");
                assert!(body.stmts.is_empty());
            }
            other => panic!("expected for loop, got {:?}", other),
        }
    }

    #[test]
    fn for_with_index_range() {
        // for (xs, 0..) |x, i| {}  →  Stmt::For { elem: x, index: Some(i) }.
        match first_stmt(vec![
            TokenKind::Keyword(Kw::For),
            TokenKind::LParen,
            id("xs"),
            TokenKind::Comma,
            TokenKind::Int(0),
            TokenKind::DotDot,
            TokenKind::RParen,
            TokenKind::Pipe,
            id("x"),
            TokenKind::Comma,
            id("i"),
            TokenKind::Pipe,
            TokenKind::LBrace,
            TokenKind::RBrace,
        ]) {
            Stmt::For {
                elem, index, ..
            } => {
                assert_eq!(elem, "x");
                assert_eq!(index.as_deref(), Some("i"), "`, 0..` binds the index");
            }
            other => panic!("expected for loop, got {:?}", other),
        }
    }

    #[test]
    fn for_two_captures_without_range_is_error() {
        // for (xs) |x, i| {}  — two captures but no `, 0..` → E0200.
        let err = parse(&toks(vec![
            TokenKind::Keyword(Kw::Fn),
            id("f"),
            TokenKind::LParen,
            TokenKind::RParen,
            id("void"),
            TokenKind::LBrace,
            TokenKind::Keyword(Kw::For),
            TokenKind::LParen,
            id("xs"),
            TokenKind::RParen,
            TokenKind::Pipe,
            id("x"),
            TokenKind::Comma,
            id("i"),
            TokenKind::Pipe,
            TokenKind::LBrace,
            TokenKind::RBrace,
            TokenKind::RBrace,
        ]))
        .expect_err("two captures without `, 0..` should fail");
        assert!(err.iter().any(|d| d.code == "E0200"));
    }

    #[test]
    fn for_index_range_with_one_capture_is_error() {
        // for (xs, 0..) |x| {}  — index range but only one capture → E0200.
        let err = parse(&toks(vec![
            TokenKind::Keyword(Kw::Fn),
            id("f"),
            TokenKind::LParen,
            TokenKind::RParen,
            id("void"),
            TokenKind::LBrace,
            TokenKind::Keyword(Kw::For),
            TokenKind::LParen,
            id("xs"),
            TokenKind::Comma,
            TokenKind::Int(0),
            TokenKind::DotDot,
            TokenKind::RParen,
            TokenKind::Pipe,
            id("x"),
            TokenKind::Pipe,
            TokenKind::LBrace,
            TokenKind::RBrace,
            TokenKind::RBrace,
        ]))
        .expect_err("index range with one capture should fail");
        assert!(err.iter().any(|d| d.code == "E0200"));
    }

    #[test]
    fn for_index_range_must_start_at_zero() {
        // for (xs, 1..) |x, i| {}  — range must start at 0 → E0200.
        let err = parse(&toks(vec![
            TokenKind::Keyword(Kw::Fn),
            id("f"),
            TokenKind::LParen,
            TokenKind::RParen,
            id("void"),
            TokenKind::LBrace,
            TokenKind::Keyword(Kw::For),
            TokenKind::LParen,
            id("xs"),
            TokenKind::Comma,
            TokenKind::Int(1),
            TokenKind::DotDot,
            TokenKind::RParen,
            TokenKind::Pipe,
            id("x"),
            TokenKind::Comma,
            id("i"),
            TokenKind::Pipe,
            TokenKind::LBrace,
            TokenKind::RBrace,
            TokenKind::RBrace,
        ]))
        .expect_err("a non-zero index range start should fail");
        assert!(
            err.iter()
                .any(|d| d.code == "E0200" && d.message.contains("must start at 0")),
            "expected the `must start at 0` diagnostic, got {:?}",
            err
        );
    }

    #[test]
    fn for_iterates_a_slice_expression() {
        // for (xs[0..n]) |x| {}  — the iterable is a full (slice) expression.
        match first_stmt(vec![
            TokenKind::Keyword(Kw::For),
            TokenKind::LParen,
            id("xs"),
            TokenKind::LBracket,
            TokenKind::Int(0),
            TokenKind::DotDot,
            id("n"),
            TokenKind::RBracket,
            TokenKind::RParen,
            TokenKind::Pipe,
            id("x"),
            TokenKind::Pipe,
            TokenKind::LBrace,
            TokenKind::RBrace,
        ]) {
            Stmt::For { iter, index, .. } => {
                assert!(
                    matches!(iter, Expr::SliceExpr { .. }),
                    "iterable parses as a full slice expression, got {:?}",
                    iter
                );
                assert_eq!(index, None);
            }
            other => panic!("expected for loop, got {:?}", other),
        }
    }

    #[test]
    fn for_body_holds_statements() {
        // for (xs, 0..) |x, i| { print(x); }  — body block carries its stmts.
        match first_stmt(vec![
            TokenKind::Keyword(Kw::For),
            TokenKind::LParen,
            id("xs"),
            TokenKind::Comma,
            TokenKind::Int(0),
            TokenKind::DotDot,
            TokenKind::RParen,
            TokenKind::Pipe,
            id("x"),
            TokenKind::Comma,
            id("i"),
            TokenKind::Pipe,
            TokenKind::LBrace,
            id("print"),
            TokenKind::LParen,
            id("x"),
            TokenKind::RParen,
            TokenKind::Semicolon,
            TokenKind::RBrace,
        ]) {
            Stmt::For { body, index, .. } => {
                assert_eq!(index.as_deref(), Some("i"));
                assert_eq!(body.stmts.len(), 1, "the loop body keeps its statement");
            }
            other => panic!("expected for loop, got {:?}", other),
        }
    }

    // ---- v0.136: comptime reflection builtins (SPEC §32) ------------------

    #[test]
    fn builtin_size_of_expr() {
        // fn f() void { var n = @sizeOf(i32); }
        // => Stmt::Let { value: Expr::Builtin { name: "sizeOf", args: [Ident i32] } }
        let stmt = parse_one_stmt(vec![
            TokenKind::Keyword(Kw::Var),
            id("n"),
            TokenKind::Eq,
            TokenKind::At,
            id("sizeOf"),
            TokenKind::LParen,
            id("i32"),
            TokenKind::RParen,
            TokenKind::Semicolon,
        ]);
        match stmt {
            Stmt::Let { name, ty, value, .. } => {
                assert_eq!(name, "n");
                assert!(ty.is_none(), "inferred binding has no annotation");
                match value {
                    Expr::Builtin { name, args, .. } => {
                        assert_eq!(name, "sizeOf");
                        assert_eq!(args.len(), 1);
                        match &args[0] {
                            Expr::Ident { name, .. } => assert_eq!(name, "i32"),
                            other => panic!("expected the type arg `i32` as an Ident, got {:?}", other),
                        }
                    }
                    other => panic!("expected @sizeOf builtin, got {:?}", other),
                }
            }
            other => panic!("expected a `var` binding, got {:?}", other),
        }
    }

    #[test]
    fn builtin_type_name_expr() {
        // fn f() void { var s = @typeName(Point); }
        // => Expr::Builtin { name: "typeName", args: [Ident Point] }
        let stmt = parse_one_stmt(vec![
            TokenKind::Keyword(Kw::Var),
            id("s"),
            TokenKind::Eq,
            TokenKind::At,
            id("typeName"),
            TokenKind::LParen,
            id("Point"),
            TokenKind::RParen,
            TokenKind::Semicolon,
        ]);
        match stmt {
            Stmt::Let { value: Expr::Builtin { name, args, .. }, .. } => {
                assert_eq!(name, "typeName");
                assert_eq!(args.len(), 1);
                assert!(matches!(&args[0], Expr::Ident { name, .. } if name == "Point"));
            }
            other => panic!("expected `var s = @typeName(Point);`, got {:?}", other),
        }
    }

    #[test]
    fn builtin_unknown_name_parses_as_builtin() {
        // The parser does not validate the builtin name (sema does): any
        // `@IDENT( … )` shape parses to `Expr::Builtin`. `@whatever(x)` parses.
        let stmt = parse_one_stmt(vec![
            TokenKind::Keyword(Kw::Var),
            id("z"),
            TokenKind::Eq,
            TokenKind::At,
            id("whatever"),
            TokenKind::LParen,
            id("x"),
            TokenKind::RParen,
            TokenKind::Semicolon,
        ]);
        assert!(matches!(
            stmt,
            Stmt::Let { value: Expr::Builtin { .. }, .. }
        ));
    }

    #[test]
    fn this_in_type_desugars_to_self() {
        // fn m(self: *@This()) void {}
        // The `self` parameter type is `*Self`: name == "Self", pointer == true.
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Fn),
            id("m"),
            TokenKind::LParen,
            id("self"),
            TokenKind::Colon,
            TokenKind::Star,
            TokenKind::At,
            id("This"),
            TokenKind::LParen,
            TokenKind::RParen,
            TokenKind::RParen,
            id("void"),
            TokenKind::LBrace,
            TokenKind::RBrace,
        ]))
        .expect("should parse");
        match &m.items[0] {
            Item::Func(f) => {
                assert_eq!(f.name, "m");
                assert_eq!(f.params.len(), 1);
                let p = &f.params[0];
                assert_eq!(p.name, "self");
                // `@This()` desugared to the self-type `Self`, behind a pointer.
                assert_eq!(p.ty.name, "Self");
                assert!(p.ty.pointer, "`*@This()` is a pointer type");
                assert!(!p.ty.optional && !p.ty.error_union && !p.ty.slice);
                assert!(p.ty.array_len.is_none());
            }
            other => panic!("expected func, got {:?}", other),
        }
    }

    #[test]
    fn this_bare_in_type_desugars_to_self() {
        // fn g(self: @This()) void {} — a bare (non-pointer) `@This()` type
        // also desugars to `Self`: name == "Self", pointer == false.
        let m = parse(&toks(vec![
            TokenKind::Keyword(Kw::Fn),
            id("g"),
            TokenKind::LParen,
            id("self"),
            TokenKind::Colon,
            TokenKind::At,
            id("This"),
            TokenKind::LParen,
            TokenKind::RParen,
            TokenKind::RParen,
            id("void"),
            TokenKind::LBrace,
            TokenKind::RBrace,
        ]))
        .expect("should parse");
        match &m.items[0] {
            Item::Func(f) => {
                let p = &f.params[0];
                assert_eq!(p.ty.name, "Self");
                assert!(!p.ty.pointer, "bare `@This()` is not a pointer");
            }
            other => panic!("expected func, got {:?}", other),
        }
    }

    #[test]
    fn non_this_at_in_type_is_error() {
        // fn h(self: @Nope()) void {} — `@` in type position must be `@This()`.
        let res = parse(&toks(vec![
            TokenKind::Keyword(Kw::Fn),
            id("h"),
            TokenKind::LParen,
            id("self"),
            TokenKind::Colon,
            TokenKind::At,
            id("Nope"),
            TokenKind::LParen,
            TokenKind::RParen,
            TokenKind::RParen,
            id("void"),
            TokenKind::LBrace,
            TokenKind::RBrace,
        ]));
        let diags = res.expect_err("a non-`This` `@` in type position is an error");
        assert!(diags.iter().any(|d| d.code == "E0200"));
    }

    #[test]
    fn at_import_is_still_an_item() {
        // @import("x.ks"); — a top-level import item (SPEC §22), unchanged by the
        // new expression/type `@` handling.
        let m = parse(&toks(vec![
            TokenKind::At,
            id("import"),
            TokenKind::LParen,
            TokenKind::Str("x.ks".to_string()),
            TokenKind::RParen,
            TokenKind::Semicolon,
        ]))
        .expect("should parse");
        assert_eq!(m.items.len(), 1);
        match &m.items[0] {
            Item::Import(imp) => assert_eq!(imp.path, "x.ks"),
            other => panic!("expected an @import item, got {:?}", other),
        }
    }

    // ---- v0.141: `unreachable` and `@panic` (SPEC §35) -------------------

    #[test]
    fn unreachable_as_statement() {
        // fn f() void { unreachable; }
        // The `unreachable` keyword parses as a primary `Expr::Unreachable`,
        // reaching here through the expr-statement path → `Stmt::Expr`.
        let stmt = parse_one_stmt(vec![
            TokenKind::Keyword(Kw::Unreachable),
            TokenKind::Semicolon,
        ]);
        match stmt {
            Stmt::Expr(Expr::Unreachable { span }) => {
                assert!(span.start < span.end, "carries its keyword span");
            }
            other => panic!("expected `unreachable;` as an expr statement, got {:?}", other),
        }
    }

    #[test]
    fn unreachable_in_value_position() {
        // fn f() void { var x = unreachable; }
        // In a value position `unreachable` is the binding's initialiser
        // expression (it adopts the expected type in sema and diverges).
        let stmt = parse_one_stmt(vec![
            TokenKind::Keyword(Kw::Var),
            id("x"),
            TokenKind::Eq,
            TokenKind::Keyword(Kw::Unreachable),
            TokenKind::Semicolon,
        ]);
        match stmt {
            Stmt::Let { name, ty, value, .. } => {
                assert_eq!(name, "x");
                assert!(ty.is_none(), "inferred binding has no annotation");
                assert!(
                    matches!(value, Expr::Unreachable { .. }),
                    "the initialiser is `Expr::Unreachable`, got {:?}",
                    value
                );
            }
            other => panic!("expected `var x = unreachable;`, got {:?}", other),
        }
    }

    #[test]
    fn panic_builtin_with_string_arg() {
        // fn f() void { @panic("boom"); }
        // `@panic("…")` needs no dedicated parser arm: it parses through the
        // existing `@ IDENT ( args )` rule as `Expr::Builtin{ name: "panic",
        // args: [StrLit] }`, reaching here as an expr statement.
        let stmt = parse_one_stmt(vec![
            TokenKind::At,
            id("panic"),
            TokenKind::LParen,
            TokenKind::Str("boom".to_string()),
            TokenKind::RParen,
            TokenKind::Semicolon,
        ]);
        match stmt {
            Stmt::Expr(Expr::Builtin { name, args, .. }) => {
                assert_eq!(name, "panic");
                assert_eq!(args.len(), 1, "`@panic` takes exactly one argument");
                match &args[0] {
                    Expr::StrLit { value, .. } => assert_eq!(value, "boom"),
                    other => panic!("expected a `[]u8` string-literal arg, got {:?}", other),
                }
            }
            other => panic!("expected `@panic(\"boom\");` as a builtin, got {:?}", other),
        }
    }

    #[test]
    fn unreachable_in_switch_else_arm() {
        // switch (c) { .Red => { return; } else => { unreachable; } }
        // A `switch` arm body is a block (SPEC §13/§20), so the diverging
        // `else => unreachable` arm is written `else => { unreachable; }`; the
        // `unreachable` parses as the block's single expr statement.
        let body = parse_fn_body(vec![
            TokenKind::Keyword(Kw::Switch),
            TokenKind::LParen,
            id("c"),
            TokenKind::RParen,
            TokenKind::LBrace,
            // .Red => { return; }
            TokenKind::Dot,
            id("Red"),
            TokenKind::FatArrow,
            TokenKind::LBrace,
            TokenKind::Keyword(Kw::Return),
            TokenKind::Semicolon,
            TokenKind::RBrace,
            TokenKind::Comma,
            // else => { unreachable; }
            TokenKind::Keyword(Kw::Else),
            TokenKind::FatArrow,
            TokenKind::LBrace,
            TokenKind::Keyword(Kw::Unreachable),
            TokenKind::Semicolon,
            TokenKind::RBrace,
            TokenKind::RBrace, // close switch
        ]);
        match &body.stmts[0] {
            Stmt::Switch { arms, default, .. } => {
                assert_eq!(arms.len(), 1);
                let def = default.as_ref().expect("the `else` arm sets `default`");
                assert_eq!(def.stmts.len(), 1, "the default arm body holds `unreachable;`");
                assert!(
                    matches!(def.stmts[0], Stmt::Expr(Expr::Unreachable { .. })),
                    "the `else` arm body is `unreachable;`, got {:?}",
                    def.stmts[0]
                );
            }
            other => panic!("expected switch, got {:?}", other),
        }
    }
}
