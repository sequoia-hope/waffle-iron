//! Tokenizer. Every lexeme carries the byte range it came from, so a
//! diagnostic downstream can point into the source.

use super::{ExprError, Span, MAX_LEXEMES};

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Num(f64),
    Ident(String),
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Caret,
    LParen,
    RParen,
    Comma,
    /// The separator of a dotted entity name (`plate.top_face`), D2. Only
    /// legal inside a measurement function's argument list; anywhere else
    /// the parser refuses it. A `.` that starts a number (`.5`) is lexed as
    /// part of the number, not as this.
    Dot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Lexeme {
    pub tok: Tok,
    pub span: Span,
}

/// Tokenize `input`. Whitespace separates; anything else is an error at its
/// own byte offset.
pub fn tokenize(input: &str) -> Result<Vec<Lexeme>, ExprError> {
    let bytes = input.as_bytes();
    let mut out: Vec<Lexeme> = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        // Bounded before anything is pushed: the tree the parser builds from
        // these lexemes is walked recursively, so an unbounded token stream
        // is an unbounded stack (see `MAX_LEXEMES`).
        if out.len() >= MAX_LEXEMES {
            return Err(ExprError::TooComplex {
                what: "token count",
                limit: MAX_LEXEMES,
            });
        }
        let c = bytes[i] as char;
        let single = match c {
            '+' => Some(Tok::Plus),
            '-' => Some(Tok::Minus),
            '*' => Some(Tok::Star),
            '/' => Some(Tok::Slash),
            '%' => Some(Tok::Percent),
            '^' => Some(Tok::Caret),
            '(' => Some(Tok::LParen),
            ')' => Some(Tok::RParen),
            ',' => Some(Tok::Comma),
            _ => None,
        };
        if let Some(tok) = single {
            out.push(Lexeme {
                tok,
                span: Span::new(i, i + 1),
            });
            i += 1;
            continue;
        }
        // A `.` is the start of a number only when a digit follows it
        // (`.5`); otherwise it is the separator of a dotted entity name
        // (D2). Decided here, by one character of lookahead, because the
        // two readings are disjoint and the number arm below would
        // otherwise consume the dot and fail on "invalid number '.'".
        if c == '.' && !bytes.get(i + 1).is_some_and(u8::is_ascii_digit) {
            out.push(Lexeme {
                tok: Tok::Dot,
                span: Span::new(i, i + 1),
            });
            i += 1;
            continue;
        }
        match c {
            ' ' | '\t' | '\n' | '\r' => i += 1,
            '0'..='9' | '.' => {
                let start = i;
                while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                    i += 1;
                }
                // Exponent part: 1e-3 / 2.5E+6. Only when digits follow.
                if i < bytes.len()
                    && (bytes[i] == b'e' || bytes[i] == b'E')
                    && i + 1 < bytes.len()
                    && (bytes[i + 1].is_ascii_digit()
                        || ((bytes[i + 1] == b'+' || bytes[i + 1] == b'-')
                            && i + 2 < bytes.len()
                            && bytes[i + 2].is_ascii_digit()))
                {
                    i += 2; // consume 'e' and sign-or-digit
                    while i < bytes.len() && bytes[i].is_ascii_digit() {
                        i += 1;
                    }
                }
                let text = &input[start..i];
                let n: f64 = text.parse().map_err(|_| ExprError::Parse {
                    pos: start,
                    message: format!("invalid number '{text}'"),
                })?;
                out.push(Lexeme {
                    tok: Tok::Num(n),
                    span: Span::new(start, i),
                });
            }
            c if c.is_ascii_alphabetic() || c == '_' => {
                let start = i;
                while i < bytes.len()
                    && ((bytes[i] as char).is_ascii_alphanumeric() || bytes[i] == b'_')
                {
                    i += 1;
                }
                out.push(Lexeme {
                    tok: Tok::Ident(input[start..i].to_string()),
                    span: Span::new(start, i),
                });
            }
            _ => {
                // `c` is one BYTE read as a char, which is wrong for a
                // multibyte character (`π` would print as 'Ï'). Decode the
                // real character for the message; `pos` stays a byte offset,
                // which is what every other span in this module is.
                let shown = input[i..].chars().next().unwrap_or(c);
                return Err(ExprError::Parse {
                    pos: i,
                    message: format!("unexpected character '{shown}'"),
                });
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(s: &str) -> Vec<Tok> {
        tokenize(s).unwrap().into_iter().map(|l| l.tok).collect()
    }

    #[test]
    fn spans_are_byte_ranges_of_the_source() {
        let lx = tokenize("25mm + width").unwrap();
        assert_eq!(lx[0].tok, Tok::Num(25.0));
        assert_eq!(lx[0].span, Span::new(0, 2));
        assert_eq!(lx[1].tok, Tok::Ident("mm".into()));
        assert_eq!(lx[1].span, Span::new(2, 4));
        assert_eq!(lx[2].span, Span::new(5, 6)); // '+'
        assert_eq!(lx[3].tok, Tok::Ident("width".into()));
        assert_eq!(lx[3].span, Span::new(7, 12));
    }

    #[test]
    fn numbers_take_exponents_only_when_digits_follow() {
        assert_eq!(toks("1.5e2"), vec![Tok::Num(150.0)]);
        assert_eq!(toks("1e-3"), vec![Tok::Num(0.001)]);
        // `2e` is a number then an identifier, not a broken exponent.
        assert_eq!(toks("2e"), vec![Tok::Num(2.0), Tok::Ident("e".into())]);
    }

    #[test]
    fn whitespace_is_skipped_and_operators_are_single_bytes() {
        assert_eq!(
            toks(" 1\t+\n2 "),
            vec![Tok::Num(1.0), Tok::Plus, Tok::Num(2.0)]
        );
        assert_eq!(
            toks("-*/%^(),"),
            vec![
                Tok::Minus,
                Tok::Star,
                Tok::Slash,
                Tok::Percent,
                Tok::Caret,
                Tok::LParen,
                Tok::RParen,
                Tok::Comma
            ]
        );
    }

    #[test]
    fn an_unexpected_character_names_its_offset() {
        assert_eq!(
            tokenize("1 + $"),
            Err(ExprError::Parse {
                pos: 4,
                message: "unexpected character '$'".into()
            })
        );
        assert_eq!(
            tokenize("1.2.3"),
            Err(ExprError::Parse {
                pos: 0,
                message: "invalid number '1.2.3'".into()
            })
        );
    }

    #[test]
    fn a_dot_is_a_number_only_when_a_digit_follows_it() {
        // D2: `plate.top_face` must lex, so a bare `.` is its own token.
        assert_eq!(
            toks("plate.top_face"),
            vec![
                Tok::Ident("plate".into()),
                Tok::Dot,
                Tok::Ident("top_face".into())
            ]
        );
        // A leading-dot literal is unchanged: `.5` is still the number.
        assert_eq!(toks(".5"), vec![Tok::Num(0.5)]);
        assert_eq!(
            toks("1 + .25"),
            vec![Tok::Num(1.0), Tok::Plus, Tok::Num(0.25)]
        );
        // `1.2.3` is still ONE number text, so it is still the same refusal:
        // the number arm's own loop consumes interior dots.
        assert!(tokenize("1.2.3").is_err());
        // A lone dot used to be `invalid number '.'` from the lexer; it is
        // now a token the PARSER refuses, which is still an error.
        assert_eq!(toks("."), vec![Tok::Dot]);
    }

    #[test]
    fn an_empty_input_yields_no_lexemes() {
        assert!(tokenize("").unwrap().is_empty());
        assert!(tokenize("   ").unwrap().is_empty());
    }
}
