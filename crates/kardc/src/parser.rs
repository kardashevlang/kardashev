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
    BinOp, Block, ConstDecl, Expr, Func, Item, Module, Param, Stmt, TestBlock, TypeExpr, UnOp,
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
        self.bump(); // `fn`
        let (name, _) = self.expect_ident()?;
        self.expect_punct(&TokenKind::LParen, "`(`")?;
        let params = self.parse_params()?;
        self.expect_punct(&TokenKind::RParen, "`)`")?;
        let ret = self.parse_type()?;
        let body = self.parse_block()?;
        let span = start.merge(body.span);
        Ok(Item::Func(Func {
            is_pub,
            name,
            params,
            ret,
            body,
            span,
        }))
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

    fn parse_type(&mut self) -> PResult<TypeExpr> {
        let (name, span) = self.expect_ident()?;
        Ok(TypeExpr { name, span })
    }

    fn parse_const(&mut self, is_pub: bool, start: Span) -> PResult<Item> {
        self.bump(); // `const`
        let (name, _) = self.expect_ident()?;
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
        self.expect_punct(&TokenKind::Semicolon, "`;`")?;
        Ok(Stmt::Expr(expr))
    }

    // ---- expressions (precedence climbing) -------------------------------

    fn parse_expr(&mut self) -> PResult<Expr> {
        self.parse_or()
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
            let inner = self.parse_primary()?;
            let span = start.merge(inner.span());
            Ok(Expr::Comptime {
                expr: Box::new(inner),
                span,
            })
        } else {
            self.parse_primary()
        }
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
}
