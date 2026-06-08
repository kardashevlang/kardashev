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
    BinOp, Block, ConstDecl, Expr, FieldDecl, FieldInit, Func, Item, Module, Param, Stmt,
    StructDecl, TestBlock, TypeExpr, UnOp,
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
                | TokenKind::Keyword(Kw::Test) => return,
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
            let (name, name_span) = self.expect_ident()?;
            self.expect_punct(&TokenKind::Colon, "`:`")?;
            let ty = self.parse_type()?;
            let span = name_span.merge(ty.span);
            params.push(Param { name, ty, span });
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

    /// Parse a type reference (SPEC §11.1). A leading `?` marks an optional
    /// type `?T`: the parser records `optional = true` and parses the inner
    /// type name. v0.114 forbids nesting (`??T`), which sema rejects; the
    /// parser only ever consumes a single leading `?`. A type with no `?` has
    /// `optional = false`. The node's span covers the `?` when present.
    fn parse_type(&mut self) -> PResult<TypeExpr> {
        let opt_span = if self.at_punct(&TokenKind::Question) {
            let sp = self.peek_span();
            self.bump();
            Some(sp)
        } else {
            None
        };
        let (name, name_span) = self.expect_ident()?;
        let span = match opt_span {
            Some(q) => q.merge(name_span),
            None => name_span,
        };
        Ok(TypeExpr {
            name,
            optional: opt_span.is_some(),
            span,
        })
    }

    fn parse_const(&mut self, is_pub: bool, start: Span) -> PResult<Item> {
        self.bump(); // `const`
        let (name, _) = self.expect_ident()?;
        // A `const IDENT =` (rather than `const IDENT :`) introduces a struct
        // declaration (SPEC §9.1); a `const IDENT : type = expr;` is the
        // ordinary value binding of §2.
        if self.at_punct(&TokenKind::Eq) {
            return self.parse_struct_decl(is_pub, name, start);
        }
        self.expect_punct(&TokenKind::Colon, "`:`")?;
        let ty = self.parse_type()?;
        self.expect_punct(&TokenKind::Eq, "`=`")?;
        let value = self.parse_expr()?;
        let semi = self.expect_punct(&TokenKind::Semicolon, "`;`")?;
        let span = start.merge(semi);
        Ok(Item::Const(ConstDecl {
            is_pub,
            name,
            ty,
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
        self.expect_punct(&TokenKind::Colon, "`:`")?;
        let ty = self.parse_type()?;
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

    fn parse_if(&mut self) -> PResult<Stmt> {
        let start = self.peek_span();
        self.bump(); // `if`
        self.expect_punct(&TokenKind::LParen, "`(`")?;
        let cond = self.parse_expr()?;
        self.expect_punct(&TokenKind::RParen, "`)`")?;
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

    fn parse_expr_stmt(&mut self) -> PResult<Stmt> {
        let expr = self.parse_expr()?;
        // A field-access place followed by `=` is a field assignment
        // (`a.b.c = e;`); a simple `name = e;` is handled by `parse_assign`
        // earlier in `parse_stmt`. Anything else is an expression statement.
        if matches!(expr, Expr::Field { .. }) && self.at_punct(&TokenKind::Eq) {
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

    /// The lowest expression level (SPEC §11.1): `orelse` binds looser than
    /// every other operator, including `or`. Left-associative, so
    /// `a orelse b orelse c` nests as `(a orelse b) orelse c`. Each operand is
    /// a full `or`-expression.
    fn parse_orelse(&mut self) -> PResult<Expr> {
        let mut lhs = self.parse_or()?;
        while self.at_kw(Kw::Orelse) {
            self.bump();
            let rhs = self.parse_or()?;
            let span = lhs.span().merge(rhs.span());
            lhs = Expr::Orelse {
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                span,
            };
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
        while self.at_punct(&TokenKind::Dot) {
            self.bump(); // `.`
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
            _ => Err(self.expected("an expression")),
        }
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
                assert_eq!(f.params[1].name, "b");
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
                assert_eq!(c.ty.name, "i64");
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
}
