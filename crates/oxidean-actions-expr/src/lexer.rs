//! Tokenizer for the `${{ }}` expression grammar.

use crate::ExprError;

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    LParen,
    RParen,
    LBracket,
    RBracket,
    Dot,
    Comma,
    Bang,  // !
    Ne,    // !=
    Eq,    // ==
    And,   // &&
    Or,    // ||
    Lt,    // <
    Le,    // <=
    Gt,    // >
    Ge,    // >=
    Star,  // * (dereference, only valid after Dot)
    Minus, // - (unary negate, numeric)
    Null,
    True,
    False,
    Num(f64),
    Str(String),
    Ident(String),
}

pub fn lex(src: &str) -> Result<Vec<Tok>, ExprError> {
    let b = src.as_bytes();
    let mut toks = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i] as char;
        match c {
            ' ' | '\t' | '\n' | '\r' => i += 1,
            '(' => {
                toks.push(Tok::LParen);
                i += 1;
            }
            ')' => {
                toks.push(Tok::RParen);
                i += 1;
            }
            '[' => {
                toks.push(Tok::LBracket);
                i += 1;
            }
            ']' => {
                toks.push(Tok::RBracket);
                i += 1;
            }
            '.' => {
                toks.push(Tok::Dot);
                i += 1;
            }
            ',' => {
                toks.push(Tok::Comma);
                i += 1;
            }
            '*' => {
                toks.push(Tok::Star);
                i += 1;
            }
            '-' => {
                toks.push(Tok::Minus);
                i += 1;
            }
            '!' => {
                if b.get(i + 1) == Some(&b'=') {
                    toks.push(Tok::Ne);
                    i += 2;
                } else {
                    toks.push(Tok::Bang);
                    i += 1;
                }
            }
            '=' if b.get(i + 1) == Some(&b'=') => {
                toks.push(Tok::Eq);
                i += 2;
            }
            '&' if b.get(i + 1) == Some(&b'&') => {
                toks.push(Tok::And);
                i += 2;
            }
            '|' if b.get(i + 1) == Some(&b'|') => {
                toks.push(Tok::Or);
                i += 2;
            }
            '<' => {
                if b.get(i + 1) == Some(&b'=') {
                    toks.push(Tok::Le);
                    i += 2;
                } else {
                    toks.push(Tok::Lt);
                    i += 1;
                }
            }
            '>' => {
                if b.get(i + 1) == Some(&b'=') {
                    toks.push(Tok::Ge);
                    i += 2;
                } else {
                    toks.push(Tok::Gt);
                    i += 1;
                }
            }
            '\'' => {
                // GitHub strings are single-quoted; '' escapes a quote.
                i += 1;
                let mut start = i;
                let mut out = String::new();
                let mut closed = false;
                while i < b.len() {
                    if b[i] == b'\'' {
                        if b.get(i + 1) == Some(&b'\'') {
                            out.push_str(&src[start..i]);
                            out.push('\'');
                            i += 2;
                            start = i;
                            continue;
                        }
                        out.push_str(&src[start..i]);
                        i += 1;
                        closed = true;
                        break;
                    }
                    i += 1;
                }
                if !closed {
                    return Err(ExprError::new("unterminated string literal"));
                }
                toks.push(Tok::Str(out));
            }
            c if c.is_ascii_digit()
                || (c == '.' && b.get(i + 1).is_some_and(|n| n.is_ascii_digit())) =>
            {
                let start = i;
                while i < b.len()
                    && (b[i].is_ascii_digit()
                        || b[i] == b'.'
                        || b[i] == b'e'
                        || b[i] == b'E'
                        || ((b[i] == b'+' || b[i] == b'-')
                            && matches!(b.get(i.wrapping_sub(1)), Some(b'e' | b'E'))))
                {
                    i += 1;
                }
                let n: f64 = src[start..i]
                    .parse()
                    .map_err(|_| ExprError::new(format!("bad number '{}'", &src[start..i])))?;
                toks.push(Tok::Num(n));
            }
            c if c.is_ascii_alphabetic() || c == '_' => {
                let start = i;
                while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_' || b[i] == b'-')
                {
                    i += 1;
                }
                let word = &src[start..i];
                toks.push(match word {
                    "null" => Tok::Null,
                    "true" => Tok::True,
                    "false" => Tok::False,
                    _ => Tok::Ident(word.to_string()),
                });
            }
            other => {
                return Err(ExprError::new(format!("unexpected character '{other}'")));
            }
        }
    }
    Ok(toks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexes_operators() {
        let t = lex("a.b == 'x' && !c || d[0] >= 1.5").unwrap();
        assert_eq!(
            t,
            vec![
                Tok::Ident("a".into()),
                Tok::Dot,
                Tok::Ident("b".into()),
                Tok::Eq,
                Tok::Str("x".into()),
                Tok::And,
                Tok::Bang,
                Tok::Ident("c".into()),
                Tok::Or,
                Tok::Ident("d".into()),
                Tok::LBracket,
                Tok::Num(0.0),
                Tok::RBracket,
                Tok::Ge,
                Tok::Num(1.5),
            ]
        );
    }

    #[test]
    fn lexes_escaped_string() {
        let t = lex("'it''s'").unwrap();
        assert_eq!(t, vec![Tok::Str("it's".into())]);
    }

    #[test]
    fn rejects_unterminated_string() {
        assert!(lex("'abc").is_err());
    }
}
