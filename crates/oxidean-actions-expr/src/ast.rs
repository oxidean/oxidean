//! Recursive-descent parser → AST for the expression grammar.
//!
//! Precedence (low→high): `||` → `&&` → `==`/`!=` → `<` `<=` `>` `>=` →
//! unary `!`/`-` → postfix `.name`/`[i]`/`.*` → calls/literals/parens.

use crate::lexer::{lex, Tok};
use crate::ExprError;

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    /// Bare identifier — resolved as a context name or object member.
    Ident(String),
    /// `expr . name` — case-insensitive member lookup.
    Member(Box<Expr>, String),
    /// `expr [ e ]` — array index or object member.
    Index(Box<Expr>, Box<Expr>),
    /// `expr .*` — collect the object's values (or array's elements).
    Deref(Box<Expr>),
    Call(String, Vec<Expr>),
    Not(Box<Expr>),
    Neg(Box<Expr>),
    Bin(BinOp, Box<Expr>, Box<Expr>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Or,
    And,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

pub fn parse(src: &str) -> Result<Expr, ExprError> {
    let toks = lex(src)?;
    if toks.is_empty() {
        return Err(ExprError::new("empty expression"));
    }
    let mut p = Parser { toks, pos: 0 };
    let e = p.or()?;
    if p.pos != p.toks.len() {
        return Err(ExprError::new(format!(
            "unexpected token {:?}",
            p.toks[p.pos]
        )));
    }
    Ok(e)
}

struct Parser {
    toks: Vec<Tok>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }
    fn next(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.pos).cloned();
        if t.is_some() {
            self.pos += 1;
        }
        t
    }
    fn expect(&mut self, t: Tok) -> Result<(), ExprError> {
        match self.next() {
            Some(ref got) if *got == t => Ok(()),
            other => Err(ExprError::new(format!("expected {t:?}, got {other:?}"))),
        }
    }

    fn or(&mut self) -> Result<Expr, ExprError> {
        let mut lhs = self.and()?;
        while self.peek() == Some(&Tok::Or) {
            self.next();
            let rhs = self.and()?;
            lhs = Expr::Bin(BinOp::Or, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }
    fn and(&mut self) -> Result<Expr, ExprError> {
        let mut lhs = self.eq()?;
        while self.peek() == Some(&Tok::And) {
            self.next();
            let rhs = self.eq()?;
            lhs = Expr::Bin(BinOp::And, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }
    fn eq(&mut self) -> Result<Expr, ExprError> {
        let mut lhs = self.cmp()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Eq) => BinOp::Eq,
                Some(Tok::Ne) => BinOp::Ne,
                _ => break,
            };
            self.next();
            let rhs = self.cmp()?;
            lhs = Expr::Bin(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }
    fn cmp(&mut self) -> Result<Expr, ExprError> {
        let mut lhs = self.unary()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Lt) => BinOp::Lt,
                Some(Tok::Le) => BinOp::Le,
                Some(Tok::Gt) => BinOp::Gt,
                Some(Tok::Ge) => BinOp::Ge,
                _ => break,
            };
            self.next();
            let rhs = self.unary()?;
            lhs = Expr::Bin(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }
    fn unary(&mut self) -> Result<Expr, ExprError> {
        match self.peek() {
            Some(Tok::Bang) => {
                self.next();
                Ok(Expr::Not(Box::new(self.unary()?)))
            }
            Some(Tok::Minus) => {
                self.next();
                Ok(Expr::Neg(Box::new(self.unary()?)))
            }
            _ => self.postfix(),
        }
    }
    /// Postfix chain: `.name`, `[expr]`, `.*`.
    fn postfix(&mut self) -> Result<Expr, ExprError> {
        let mut e = self.primary()?;
        loop {
            match self.peek() {
                Some(Tok::Dot) => {
                    self.next();
                    match self.next() {
                        Some(Tok::Ident(name)) => {
                            e = Expr::Member(Box::new(e), name);
                        }
                        Some(Tok::Star) => {
                            e = Expr::Deref(Box::new(e));
                        }
                        Some(Tok::Str(name)) => {
                            // `a.'key'` is legal in GitHub expressions.
                            e = Expr::Member(Box::new(e), name);
                        }
                        other => {
                            return Err(ExprError::new(format!(
                                "expected member name after '.', got {other:?}"
                            )));
                        }
                    }
                }
                Some(Tok::LBracket) => {
                    self.next();
                    let idx = self.or()?;
                    self.expect(Tok::RBracket)?;
                    e = Expr::Index(Box::new(e), Box::new(idx));
                }
                _ => break,
            }
        }
        Ok(e)
    }
    fn primary(&mut self) -> Result<Expr, ExprError> {
        match self.next() {
            Some(Tok::Null) => Ok(Expr::Null),
            Some(Tok::True) => Ok(Expr::Bool(true)),
            Some(Tok::False) => Ok(Expr::Bool(false)),
            Some(Tok::Num(n)) => Ok(Expr::Num(n)),
            Some(Tok::Str(s)) => Ok(Expr::Str(s)),
            Some(Tok::LParen) => {
                let e = self.or()?;
                self.expect(Tok::RParen)?;
                Ok(e)
            }
            Some(Tok::Ident(name)) => {
                if self.peek() == Some(&Tok::LParen) {
                    self.next();
                    let mut args = Vec::new();
                    if self.peek() != Some(&Tok::RParen) {
                        loop {
                            args.push(self.or()?);
                            match self.peek() {
                                Some(Tok::Comma) => {
                                    self.next();
                                }
                                _ => break,
                            }
                        }
                    }
                    self.expect(Tok::RParen)?;
                    Ok(Expr::Call(name, args))
                } else {
                    Ok(Expr::Ident(name))
                }
            }
            other => Err(ExprError::new(format!("unexpected token {other:?}"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_precedence() {
        // a || b && !c  →  a || (b && (!c))
        let e = parse("a || b && !c").unwrap();
        assert!(matches!(e, Expr::Bin(BinOp::Or, _, _)));
        if let Expr::Bin(_, _, rhs) = &e {
            assert!(matches!(**rhs, Expr::Bin(BinOp::And, _, _)));
        }
    }

    #[test]
    fn parses_member_index_deref_call() {
        parse("github.event.commits[0].id").unwrap();
        parse("steps.*.outputs.foo").unwrap();
        parse("needs['build-a'].result").unwrap();
        parse("format('{0}-{1}', github.sha, matrix.os)").unwrap();
    }

    #[test]
    fn rejects_garbage() {
        for bad in ["", "a.", "(", "a == ", "f(", "[0", "a b", "'x", "&& a"] {
            assert!(parse(bad).is_err(), "expected parse error: {bad:?}");
        }
    }
}
