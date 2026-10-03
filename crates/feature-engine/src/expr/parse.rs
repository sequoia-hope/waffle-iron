//! Recursive-descent parser producing an [`Expr`] AST.
//!
//! The parser is independent of the environment: it validates syntax,
//! function names and arities, folds a unit suffix into its literal, and
//! records every byte range. What it cannot know — whether an identifier
//! names a parameter — stays for the evaluator, so the parameter table's
//! fixpoint still sees [`ExprError::UnknownIdentifier`] from evaluation.

use std::collections::BTreeSet;

use super::dim::{unit_by_name, Unit};
use super::lex::{tokenize, Lexeme, Tok};
use super::{ExprError, Span, FUNCTIONS, MAX_DEPTH};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Pow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Pos,
}

/// One node of a parsed expression.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// A literal, already scaled into the working space by its unit suffix
    /// (so `1in` is `value: 25.4, unit: in`). `pi` is one of these too, with
    /// no unit. `unit` is what commits the dimension.
    Number {
        value: f64,
        unit: Option<&'static Unit>,
        span: Span,
    },
    /// A parameter reference, resolved at evaluation.
    Ident { name: String, span: Span },
    Unary {
        op: UnOp,
        operand: Box<Expr>,
        span: Span,
    },
    Binary {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
        span: Span,
    },
    /// A call to a known function with a validated arity.
    Call {
        name: String,
        args: Vec<Expr>,
        span: Span,
    },
}

impl Expr {
    pub fn span(&self) -> Span {
        match self {
            Expr::Number { span, .. }
            | Expr::Ident { span, .. }
            | Expr::Unary { span, .. }
            | Expr::Binary { span, .. }
            | Expr::Call { span, .. } => *span,
        }
    }

    /// Every identifier this expression reads, sorted and deduplicated.
    ///
    /// This is the dependency list of an expression — available without an
    /// environment and without evaluating, which is why P1 parses to a tree
    /// instead of evaluating as it scans.
    pub fn identifiers(&self) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        self.collect_identifiers(&mut out);
        out
    }

    /// The byte spans of every reference to `name`, in source order.
    ///
    /// This is what makes a parameter rename exact (P5): the AST says which
    /// byte ranges of the source ARE references to `name`, so a rename
    /// splices precisely those. A textual search cannot do it — `w2` contains
    /// `w`, `"w"` inside a longer identifier is not a reference, and a unit
    /// suffix (`in`, `mm`) lexes as part of its literal and is not an
    /// identifier node at all.
    pub fn reference_spans(&self, name: &str) -> Vec<Span> {
        let mut out = Vec::new();
        self.collect_reference_spans(name, &mut out);
        out.sort_by_key(|s| s.start);
        out
    }

    fn collect_reference_spans(&self, name: &str, out: &mut Vec<Span>) {
        match self {
            Expr::Number { .. } => {}
            Expr::Ident {
                name: ident, span, ..
            } => {
                if ident == name {
                    out.push(*span);
                }
            }
            Expr::Unary { operand, .. } => operand.collect_reference_spans(name, out),
            Expr::Binary { lhs, rhs, .. } => {
                lhs.collect_reference_spans(name, out);
                rhs.collect_reference_spans(name, out);
            }
            // A call's CALLEE is not an identifier node: every function name
            // is a reserved word (`expr::is_reserved_word`), so no parameter
            // can be named one and `sqrt` is never a reference to rename.
            Expr::Call { args, .. } => {
                for a in args {
                    a.collect_reference_spans(name, out);
                }
            }
        }
    }

    fn collect_identifiers(&self, out: &mut BTreeSet<String>) {
        match self {
            Expr::Number { .. } => {}
            Expr::Ident { name, .. } => {
                out.insert(name.clone());
            }
            Expr::Unary { operand, .. } => operand.collect_identifiers(out),
            Expr::Binary { lhs, rhs, .. } => {
                lhs.collect_identifiers(out);
                rhs.collect_identifiers(out);
            }
            Expr::Call { args, .. } => {
                for a in args {
                    a.collect_identifiers(out);
                }
            }
        }
    }
}

/// How many arguments a function takes.
#[derive(Debug, Clone, Copy)]
enum Arity {
    Exactly1,
    AtLeast1,
}

fn arity_of(name: &str) -> Option<Arity> {
    match name {
        "sqrt" | "abs" | "floor" | "ceil" | "round" | "sin" | "cos" | "tan" => {
            Some(Arity::Exactly1)
        }
        "min" | "max" => Some(Arity::AtLeast1),
        _ => None,
    }
}

/// Parse `input` into an AST.
pub fn parse(input: &str) -> Result<Expr, ExprError> {
    let lexemes = tokenize(input)?;
    if lexemes.is_empty() {
        return Err(ExprError::Empty);
    }
    let mut p = Parser {
        lexemes: &lexemes,
        pos: 0,
        depth: 0,
    };
    let ast = p.parse_expr()?;
    if let Some(lx) = p.peek() {
        return Err(ExprError::Parse {
            pos: lx.span.start,
            message: "unexpected trailing input".to_string(),
        });
    }
    Ok(ast)
}

struct Parser<'a> {
    lexemes: &'a [Lexeme],
    pos: usize,
    /// How many nesting levels deep the descent currently is. Bounded by
    /// [`MAX_DEPTH`]: this parser is recursive, so an unbounded nest is a
    /// stack overflow — an abort natively and a trap in WASM, neither of
    /// which a caller can report.
    depth: usize,
}

impl<'a> Parser<'a> {
    /// Run `f` one nesting level deeper, or refuse at the bound.
    fn nested<T>(
        &mut self,
        f: impl FnOnce(&mut Self) -> Result<T, ExprError>,
    ) -> Result<T, ExprError> {
        if self.depth >= MAX_DEPTH {
            return Err(ExprError::TooComplex {
                what: "nesting depth",
                limit: MAX_DEPTH,
            });
        }
        self.depth += 1;
        let out = f(self);
        self.depth -= 1;
        out
    }

    fn peek(&self) -> Option<&'a Lexeme> {
        self.lexemes.get(self.pos)
    }

    fn next(&mut self) -> Option<&'a Lexeme> {
        let lx = self.lexemes.get(self.pos);
        if lx.is_some() {
            self.pos += 1;
        }
        lx
    }

    /// One past the last byte consumed, for an "unexpected end" diagnostic.
    fn end_pos(&self) -> usize {
        self.lexemes.last().map_or(0, |lx| lx.span.end)
    }

    /// expr := mul (('+'|'-') mul)*
    fn parse_expr(&mut self) -> Result<Expr, ExprError> {
        let mut lhs = self.parse_mul()?;
        while let Some(lx) = self.peek() {
            let op = match lx.tok {
                Tok::Plus => BinOp::Add,
                Tok::Minus => BinOp::Sub,
                _ => break,
            };
            self.pos += 1;
            let rhs = self.parse_mul()?;
            lhs = binary(op, lhs, rhs);
        }
        Ok(lhs)
    }

    /// mul := unary (('*'|'/'|'%') unary)*
    fn parse_mul(&mut self) -> Result<Expr, ExprError> {
        let mut lhs = self.parse_unary()?;
        while let Some(lx) = self.peek() {
            let op = match lx.tok {
                Tok::Star => BinOp::Mul,
                Tok::Slash => BinOp::Div,
                Tok::Percent => BinOp::Rem,
                _ => break,
            };
            self.pos += 1;
            let rhs = self.parse_unary()?;
            lhs = binary(op, lhs, rhs);
        }
        Ok(lhs)
    }

    /// unary := ('-'|'+') unary | power. `-2^2 == -(2^2) == -4`.
    fn parse_unary(&mut self) -> Result<Expr, ExprError> {
        let Some(lx) = self.peek() else {
            return self.parse_power();
        };
        let op = match lx.tok {
            Tok::Minus => UnOp::Neg,
            Tok::Plus => UnOp::Pos,
            _ => return self.parse_power(),
        };
        let start = lx.span;
        self.pos += 1;
        let operand = self.nested(Self::parse_unary)?;
        let span = start.join(operand.span());
        Ok(Expr::Unary {
            op,
            operand: Box::new(operand),
            span,
        })
    }

    /// power := primary ('^' unary)?  (right-associative)
    fn parse_power(&mut self) -> Result<Expr, ExprError> {
        let base = self.parse_primary()?;
        if let Some(lx) = self.peek() {
            if lx.tok == Tok::Caret {
                self.pos += 1;
                let exp = self.nested(Self::parse_unary)?;
                return Ok(binary(BinOp::Pow, base, exp));
            }
        }
        Ok(base)
    }

    /// primary := Num unit? | Ident | Ident '(' args ')' | '(' expr ')'
    fn parse_primary(&mut self) -> Result<Expr, ExprError> {
        let end = self.end_pos();
        let Some(lx) = self.next() else {
            return Err(ExprError::Parse {
                pos: end,
                message: "unexpected end of expression".to_string(),
            });
        };
        match &lx.tok {
            Tok::Num(n) => self.finish_number(*n, lx.span),
            Tok::LParen => {
                let inner = self.nested(Self::parse_expr)?;
                match self.next() {
                    Some(close) if close.tok == Tok::RParen => Ok(inner),
                    _ => Err(ExprError::Parse {
                        pos: self.end_pos(),
                        message: "expected ')'".to_string(),
                    }),
                }
            }
            Tok::Ident(name) => self.finish_ident(name.clone(), lx.span),
            other => Err(ExprError::Parse {
                pos: lx.span.start,
                message: format!("unexpected token {other:?}"),
            }),
        }
    }

    /// A literal and its optional unit suffix. The suffix is folded into the
    /// literal's value (`25mm` → 25.0 mm-space) and recorded, because it is
    /// what commits the dimension.
    fn finish_number(&mut self, n: f64, span: Span) -> Result<Expr, ExprError> {
        let Some(next) = self.peek() else {
            return Ok(Expr::Number {
                value: n,
                unit: None,
                span,
            });
        };
        let Tok::Ident(name) = &next.tok else {
            return Ok(Expr::Number {
                value: n,
                unit: None,
                span,
            });
        };
        if let Some(unit) = unit_by_name(name) {
            let span = span.join(next.span);
            self.pos += 1;
            return Ok(Expr::Number {
                value: n * unit.factor,
                unit: Some(unit),
                span,
            });
        }
        // A non-unit identifier after a number is a mistake (`5 width`) —
        // reject loudly rather than guessing an operator.
        Err(ExprError::Parse {
            pos: next.span.start,
            message: format!("'{name}' is not a unit; write an operator (e.g. `* {name}`)"),
        })
    }

    /// An identifier: a call, the constant `pi`, or a parameter reference.
    fn finish_ident(&mut self, name: String, span: Span) -> Result<Expr, ExprError> {
        let is_call = self.peek().is_some_and(|lx| lx.tok == Tok::LParen);
        if is_call {
            self.pos += 1; // consume '('
            let (args, close) = self.parse_args()?;
            let Some(arity) = arity_of(&name) else {
                return Err(ExprError::UnknownFunction(name));
            };
            let ok = match arity {
                Arity::Exactly1 => args.len() == 1,
                Arity::AtLeast1 => !args.is_empty(),
            };
            if !ok {
                return Err(ExprError::WrongArity {
                    function: name,
                    expected: match arity {
                        Arity::Exactly1 => "1",
                        Arity::AtLeast1 => "1 or more",
                    },
                    got: args.len(),
                });
            }
            return Ok(Expr::Call {
                name,
                args,
                span: span.join(close),
            });
        }
        if name == "pi" {
            return Ok(Expr::Number {
                value: std::f64::consts::PI,
                unit: None,
                span,
            });
        }
        // A bare unit or function name is not a value. Decided here, at
        // parse time, so no environment can turn `mm` into a variable.
        if unit_by_name(&name).is_some() || FUNCTIONS.contains(&name.as_str()) {
            return Err(ExprError::Parse {
                pos: span.start,
                message: format!("'{name}' cannot be used as a value"),
            });
        }
        Ok(Expr::Ident { name, span })
    }

    /// Comma-separated args up to ')'. The '(' is already consumed. Returns
    /// the args and the closing paren's span.
    fn parse_args(&mut self) -> Result<(Vec<Expr>, Span), ExprError> {
        let mut args = Vec::new();
        if let Some(lx) = self.peek() {
            if lx.tok == Tok::RParen {
                let span = lx.span;
                self.pos += 1;
                return Ok((args, span));
            }
        }
        loop {
            args.push(self.nested(Self::parse_expr)?);
            let fallback = self.end_pos();
            let Some(lx) = self.next() else {
                return Err(ExprError::Parse {
                    pos: fallback,
                    message: "expected ',' or ')'".to_string(),
                });
            };
            match lx.tok {
                Tok::Comma => continue,
                Tok::RParen => return Ok((args, lx.span)),
                _ => {
                    return Err(ExprError::Parse {
                        pos: lx.span.start,
                        message: "expected ',' or ')'".to_string(),
                    })
                }
            }
        }
    }
}

fn binary(op: BinOp, lhs: Expr, rhs: Expr) -> Expr {
    let span = lhs.span().join(rhs.span());
    Expr::Binary {
        op,
        lhs: Box::new(lhs),
        rhs: Box::new(rhs),
        span,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A compact rendering of the tree, so precedence is testable without
    /// writing nested constructors.
    fn sexpr(e: &Expr) -> String {
        match e {
            Expr::Number { value, unit, .. } => match unit {
                Some(u) => format!("{value}[{}]", u.name),
                None => format!("{value}"),
            },
            Expr::Ident { name, .. } => name.clone(),
            Expr::Unary { op, operand, .. } => {
                let o = match op {
                    UnOp::Neg => "neg",
                    UnOp::Pos => "pos",
                };
                format!("({o} {})", sexpr(operand))
            }
            Expr::Binary { op, lhs, rhs, .. } => {
                let o = match op {
                    BinOp::Add => "+",
                    BinOp::Sub => "-",
                    BinOp::Mul => "*",
                    BinOp::Div => "/",
                    BinOp::Rem => "%",
                    BinOp::Pow => "^",
                };
                format!("({o} {} {})", sexpr(lhs), sexpr(rhs))
            }
            Expr::Call { name, args, .. } => {
                let a: Vec<String> = args.iter().map(sexpr).collect();
                format!("({name} {})", a.join(" "))
            }
        }
    }

    fn tree(s: &str) -> String {
        sexpr(&parse(s).unwrap())
    }

    #[test]
    fn precedence_is_mul_over_add() {
        assert_eq!(tree("2 + 3 * 4"), "(+ 2 (* 3 4))");
        assert_eq!(tree("(2 + 3) * 4"), "(* (+ 2 3) 4)");
        assert_eq!(tree("1 - 2 - 3"), "(- (- 1 2) 3)", "left-associative");
        assert_eq!(tree("8 / 4 / 2"), "(/ (/ 8 4) 2)");
        assert_eq!(tree("7 % 4 * 2"), "(* (% 7 4) 2)");
    }

    #[test]
    fn power_is_right_associative_and_binds_tighter_than_unary() {
        assert_eq!(tree("2 ^ 3 ^ 2"), "(^ 2 (^ 3 2))");
        assert_eq!(tree("-2 ^ 2"), "(neg (^ 2 2))");
        assert_eq!(tree("2 ^ -1"), "(^ 2 (neg 1))");
        assert_eq!(tree("2 * 3 ^ 2"), "(* 2 (^ 3 2))");
    }

    #[test]
    fn unary_chains_and_pi_is_a_number() {
        assert_eq!(tree("--5"), "(neg (neg 5))");
        assert_eq!(tree("+5"), "(pos 5)");
        assert_eq!(tree("pi"), format!("{}", std::f64::consts::PI));
    }

    #[test]
    fn a_unit_suffix_is_folded_into_the_literal_and_recorded() {
        assert_eq!(tree("25mm"), "25[mm]");
        assert_eq!(tree("2 cm"), "20[cm]");
        assert_eq!(tree("1in"), "25.4[in]");
        assert_eq!(tree("90 deg"), "90[deg]");
        assert_eq!(tree("1in + 2mm"), "(+ 25.4[in] 2[mm])");
    }

    #[test]
    fn calls_carry_their_arguments() {
        assert_eq!(tree("sqrt(16)"), "(sqrt 16)");
        assert_eq!(tree("min(3, 1, 2)"), "(min 3 1 2)");
        assert_eq!(tree("max(a, b * 2)"), "(max a (* b 2))");
    }

    #[test]
    fn identifiers_are_the_dependency_list() {
        let ast = parse("sqrt(width ^ 2 + height ^ 2) / 2 + width").unwrap();
        let ids: Vec<String> = ast.identifiers().into_iter().collect();
        assert_eq!(ids, vec!["height".to_string(), "width".to_string()]);
        // `pi`, units and function names are not dependencies.
        let ast = parse("2 * pi * r * sin(30 deg)").unwrap();
        let ids: Vec<String> = ast.identifiers().into_iter().collect();
        assert_eq!(ids, vec!["r".to_string()]);
        assert!(parse("25mm").unwrap().identifiers().is_empty());
    }

    #[test]
    fn spans_cover_the_whole_subexpression() {
        let ast = parse("1 + 2 * 3").unwrap();
        assert_eq!(ast.span(), Span::new(0, 9));
        let Expr::Binary { rhs, .. } = &ast else {
            panic!("expected a binary root");
        };
        assert_eq!(rhs.span(), Span::new(4, 9));
    }

    #[test]
    fn arity_and_unknown_functions_are_refused_at_parse_time() {
        assert_eq!(
            parse("sqrt(1, 2)"),
            Err(ExprError::WrongArity {
                function: "sqrt".into(),
                expected: "1",
                got: 2
            })
        );
        assert_eq!(
            parse("min()"),
            Err(ExprError::WrongArity {
                function: "min".into(),
                expected: "1 or more",
                got: 0
            })
        );
        assert_eq!(
            parse("bogus(1)"),
            Err(ExprError::UnknownFunction("bogus".into()))
        );
    }

    #[test]
    fn syntax_errors_name_their_byte_offset() {
        assert_eq!(parse(""), Err(ExprError::Empty));
        assert_eq!(parse("   "), Err(ExprError::Empty));
        assert_eq!(parse("2 +").unwrap_err().offset(), Some(3));
        assert_eq!(parse("(1 + 2").unwrap_err().offset(), Some(6));
        assert_eq!(parse("1 2").unwrap_err().offset(), Some(2));
        assert_eq!(
            parse("5 width"),
            Err(ExprError::Parse {
                pos: 2,
                message: "'width' is not a unit; write an operator (e.g. `* width`)".into()
            })
        );
        assert_eq!(
            parse("mm"),
            Err(ExprError::Parse {
                pos: 0,
                message: "'mm' cannot be used as a value".into()
            })
        );
        assert_eq!(
            parse("1 + sqrt"),
            Err(ExprError::Parse {
                pos: 4,
                message: "'sqrt' cannot be used as a value".into()
            })
        );
        assert_eq!(
            parse("f(1"),
            Err(ExprError::Parse {
                pos: 3,
                message: "expected ',' or ')'".into()
            })
        );
        assert_eq!(parse("*2").unwrap_err().offset(), Some(0));
    }

    #[test]
    fn a_multibyte_character_is_named_correctly_at_its_byte_offset() {
        // The offset is a BYTE offset (every span in this module is), and
        // the character in the message is the real one, not one byte of it.
        assert_eq!(
            parse("1 + π"),
            Err(ExprError::Parse {
                pos: 4,
                message: "unexpected character 'π'".into()
            })
        );
        assert_eq!(
            parse("2mm × 3"),
            Err(ExprError::Parse {
                pos: 4,
                message: "unexpected character '×'".into()
            })
        );
    }

    #[test]
    fn identifiers_take_digits_and_underscores_but_never_start_with_a_digit() {
        assert_eq!(tree("bore_d2 + _x1"), "(+ bore_d2 _x1)");
        assert_eq!(parse("2mm_x").unwrap_err().offset(), Some(1));
        // `2mm 3` is implicit multiplication nobody wrote: a parse error,
        // never 6.
        assert_eq!(
            parse("2mm 3"),
            Err(ExprError::Parse {
                pos: 4,
                message: "unexpected trailing input".into()
            })
        );
        assert!(parse("2 3").is_err());
        assert!(parse("width height").is_err());
    }

    #[test]
    fn a_deep_nest_is_a_typed_error_not_a_stack_overflow() {
        // Both the parser and the evaluator are recursive: without this
        // bound a nest like this aborts the process (and traps the WASM
        // engine), which no caller can report. Measured 2026-10-03: 3 000
        // levels overflowed an 8 MB stack.
        // Just past the depth bound, the depth bound is what names it.
        let nest = format!(
            "{}1{}",
            "(".repeat(MAX_DEPTH + 1),
            ")".repeat(MAX_DEPTH + 1)
        );
        assert_eq!(
            parse(&nest),
            Err(ExprError::TooComplex {
                what: "nesting depth",
                limit: MAX_DEPTH
            })
        );
        // Far past it, whichever bound trips first does — both are typed.
        for n in [MAX_DEPTH + 1, 1_000, 10_000] {
            let nest = format!("{}1{}", "(".repeat(n), ")".repeat(n));
            assert!(
                matches!(parse(&nest), Err(ExprError::TooComplex { .. })),
                "{n} parentheses"
            );
            let unary = format!("{}1", "-".repeat(n));
            assert!(
                matches!(parse(&unary), Err(ExprError::TooComplex { .. })),
                "{n} unary minuses"
            );
            // `n` carets, so `n` levels of right-associative descent.
            let power = vec!["2"; n + 2].join("^");
            assert!(
                matches!(parse(&power), Err(ExprError::TooComplex { .. })),
                "{n} chained powers"
            );
            let args = format!("min({}1{})", "max(".repeat(n), ")".repeat(n));
            assert!(
                matches!(parse(&args), Err(ExprError::TooComplex { .. })),
                "{n} nested calls"
            );
        }
        // Just inside the bound still parses.
        let ok = format!("{}1{}", "(".repeat(MAX_DEPTH), ")".repeat(MAX_DEPTH));
        assert!(parse(&ok).is_ok());
    }

    #[test]
    fn a_long_flat_chain_is_a_typed_error_too() {
        // A left-associative chain parses ITERATIVELY, so the depth bound
        // does not see it — but it evaluates down a tree half as deep as
        // the token count, which is the other way to overflow. The lexeme
        // bound is what stops it.
        let chain = vec!["1"; 10_000].join("+");
        assert_eq!(
            parse(&chain),
            Err(ExprError::TooComplex {
                what: "token count",
                limit: super::super::MAX_LEXEMES
            })
        );
        let ok = vec!["1"; 400].join("+");
        assert!(parse(&ok).is_ok());
    }
}
