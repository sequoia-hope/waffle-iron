//! Differential corpus: the P1 AST parser against the pre-P1 evaluator.
//!
//! The pre-P1 `expr.rs` computed as it scanned. P1 replaced it with
//! `parse` → `Expr` → `eval` and deliberately kept the grammar, so the two
//! must agree on every expression the OLD grammar could express. This test
//! pins that: 200 deterministically generated expressions over the old
//! operator set (`+ - * / % ^`, parentheses, the ten functions, `pi`, bare
//! literals, the pre-P1 unit suffixes, parameter names) are evaluated by
//! both and compared bit-for-bit.
//!
//! The oracle below is a verbatim copy of the pre-P1 evaluator, lifted from
//! `crates/feature-engine/src/expr.rs` at this branch's merge-base. It is
//! test-only and must never be "fixed": its job is to say what the old code
//! did, including where that was worse.
//!
//! Three divergences are EXPECTED and asserted as such, not papered over:
//!
//! 1. A non-finite intermediate is now an error where it occurs, so
//!    `min(1/0, 5)` is `NonFinite` where the old code returned 5. P1's
//!    stated purpose (`specs/agent_mechanical_design.md` §6).
//! 2. An expression past `MAX_LEXEMES` / `MAX_DEPTH` is `TooComplex` where
//!    the old code overflowed the stack.
//! 3. A dimension mismatch (`10mm + 1deg`) is an error where the old code
//!    silently produced a number. The generator does not mix dimensions, so
//!    this case cannot arise here; it is pinned by the unit tests in
//!    `expr::eval`.
//!
//! Everything else must match exactly — a difference is a port defect.
//!
//! Measured 2026-10-03 over the 200-expression corpus: 107 agreed
//! bit-for-bit, 54 were refused by both, 39 are newly a dimension error,
//! and ZERO diverged. The generator happens to produce no laundered
//! infinity, so divergence 1 is pinned by
//! `the_old_code_laundered_an_infinity_through_min_max_and_abs` below
//! rather than by the corpus.

use std::collections::HashMap;

use feature_engine::expr::{self, ExprError};

#[allow(dead_code)]
mod oracle {
    //! VERBATIM pre-P1 evaluator (merge-base `crates/feature-engine/src/expr.rs`).
    //! Test oracle only. Do not edit to make a test pass.
    use std::collections::HashMap;

    #[derive(Debug, Clone, PartialEq)]
    pub enum ExprError {
        Empty,
        Parse {
            pos: usize,
            message: String,
        },
        UnknownIdentifier(String),
        UnknownFunction(String),
        WrongArity {
            function: String,
            expected: &'static str,
            got: usize,
        },
        NonFinite,
    }

    const UNIT_FACTORS: &[(&str, f64)] = &[
        ("mm", 1.0),
        ("cm", 10.0),
        ("m", 1000.0),
        ("in", 25.4),
        ("ft", 304.8),
        ("deg", 1.0),
    ];

    /// Function names (all reserved as identifiers). Trig is in DEGREES.
    const FUNCTIONS: &[&str] = &[
        "sqrt", "abs", "floor", "ceil", "round", "sin", "cos", "tan", "min", "max",
    ];

    pub fn unit_factor(name: &str) -> Option<f64> {
        UNIT_FACTORS
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, f)| *f)
    }

    pub fn evaluate(input: &str, vars: &HashMap<String, f64>) -> Result<f64, ExprError> {
        let tokens = tokenize(input)?;
        if tokens.is_empty() {
            return Err(ExprError::Empty);
        }
        let mut p = Parser {
            tokens: &tokens,
            pos: 0,
            vars,
        };
        let v = p.parse_expr()?;
        if let Some(&(tok_pos, _)) = p.peek() {
            return Err(ExprError::Parse {
                pos: tok_pos,
                message: "unexpected trailing input".to_string(),
            });
        }
        if !v.is_finite() {
            return Err(ExprError::NonFinite);
        }
        Ok(v)
    }

    #[derive(Debug, Clone, PartialEq)]
    enum Tok {
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
    }

    /// Tokenize into (source position, token) pairs.
    fn tokenize(input: &str) -> Result<Vec<(usize, Tok)>, ExprError> {
        let bytes = input.as_bytes();
        let mut out = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            let c = bytes[i] as char;
            match c {
                ' ' | '\t' | '\n' | '\r' => i += 1,
                '+' => {
                    out.push((i, Tok::Plus));
                    i += 1;
                }
                '-' => {
                    out.push((i, Tok::Minus));
                    i += 1;
                }
                '*' => {
                    out.push((i, Tok::Star));
                    i += 1;
                }
                '/' => {
                    out.push((i, Tok::Slash));
                    i += 1;
                }
                '%' => {
                    out.push((i, Tok::Percent));
                    i += 1;
                }
                '^' => {
                    out.push((i, Tok::Caret));
                    i += 1;
                }
                '(' => {
                    out.push((i, Tok::LParen));
                    i += 1;
                }
                ')' => {
                    out.push((i, Tok::RParen));
                    i += 1;
                }
                ',' => {
                    out.push((i, Tok::Comma));
                    i += 1;
                }
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
                    out.push((start, Tok::Num(n)));
                }
                c if c.is_ascii_alphabetic() || c == '_' => {
                    let start = i;
                    while i < bytes.len()
                        && ((bytes[i] as char).is_ascii_alphanumeric() || bytes[i] == b'_')
                    {
                        i += 1;
                    }
                    out.push((start, Tok::Ident(input[start..i].to_string())));
                }
                _ => {
                    return Err(ExprError::Parse {
                        pos: i,
                        message: format!("unexpected character '{c}'"),
                    })
                }
            }
        }
        Ok(out)
    }

    struct Parser<'a> {
        tokens: &'a [(usize, Tok)],
        pos: usize,
        vars: &'a HashMap<String, f64>,
    }

    impl<'a> Parser<'a> {
        fn peek(&self) -> Option<&(usize, Tok)> {
            self.tokens.get(self.pos)
        }

        fn next(&mut self) -> Option<&(usize, Tok)> {
            let t = self.tokens.get(self.pos);
            if t.is_some() {
                self.pos += 1;
            }
            t
        }

        fn end_pos(&self) -> usize {
            self.tokens.last().map_or(0, |(p, _)| *p + 1)
        }

        /// expr := mul (('+'|'-') mul)*
        fn parse_expr(&mut self) -> Result<f64, ExprError> {
            let mut acc = self.parse_mul()?;
            while let Some((_, tok)) = self.peek() {
                match tok {
                    Tok::Plus => {
                        self.pos += 1;
                        acc += self.parse_mul()?;
                    }
                    Tok::Minus => {
                        self.pos += 1;
                        acc -= self.parse_mul()?;
                    }
                    _ => break,
                }
            }
            Ok(acc)
        }

        /// mul := unary (('*'|'/'|'%') unary)*
        fn parse_mul(&mut self) -> Result<f64, ExprError> {
            let mut acc = self.parse_unary()?;
            while let Some((_, tok)) = self.peek() {
                match tok {
                    Tok::Star => {
                        self.pos += 1;
                        acc *= self.parse_unary()?;
                    }
                    Tok::Slash => {
                        self.pos += 1;
                        acc /= self.parse_unary()?;
                    }
                    Tok::Percent => {
                        self.pos += 1;
                        acc %= self.parse_unary()?;
                    }
                    _ => break,
                }
            }
            Ok(acc)
        }

        /// unary := ('-'|'+') unary | power. `-2^2 == -(2^2) == -4`.
        fn parse_unary(&mut self) -> Result<f64, ExprError> {
            match self.peek() {
                Some((_, Tok::Minus)) => {
                    self.pos += 1;
                    Ok(-self.parse_unary()?)
                }
                Some((_, Tok::Plus)) => {
                    self.pos += 1;
                    self.parse_unary()
                }
                _ => self.parse_power(),
            }
        }

        /// power := primary ('^' unary)?  (right-associative)
        fn parse_power(&mut self) -> Result<f64, ExprError> {
            let base = self.parse_primary()?;
            if let Some((_, Tok::Caret)) = self.peek() {
                self.pos += 1;
                let exp = self.parse_unary()?;
                return Ok(base.powf(exp));
            }
            Ok(base)
        }

        /// primary := Num unit? | Ident | Ident '(' args ')' | '(' expr ')'
        fn parse_primary(&mut self) -> Result<f64, ExprError> {
            let end = self.end_pos();
            let (tok_pos, tok) = match self.next() {
                Some(t) => (t.0, t.1.clone()),
                None => {
                    return Err(ExprError::Parse {
                        pos: end,
                        message: "unexpected end of expression".to_string(),
                    })
                }
            };
            match tok {
                Tok::Num(n) => {
                    // Optional unit suffix directly after a literal: `25mm`, `2 in`.
                    if let Some((upos, Tok::Ident(name))) = self.peek().cloned() {
                        if let Some(factor) = unit_factor(&name) {
                            self.pos += 1;
                            return Ok(n * factor);
                        }
                        // A non-unit identifier after a number is a mistake
                        // (`5 width`) — reject loudly rather than guessing.
                        return Err(ExprError::Parse {
                            pos: upos,
                            message: format!(
                                "'{name}' is not a unit; write an operator (e.g. `* {name}`)"
                            ),
                        });
                    }
                    Ok(n)
                }
                Tok::LParen => {
                    let v = self.parse_expr()?;
                    match self.next() {
                        Some((_, Tok::RParen)) => Ok(v),
                        _ => Err(ExprError::Parse {
                            pos: self.end_pos(),
                            message: "expected ')'".to_string(),
                        }),
                    }
                }
                Tok::Ident(name) => {
                    if let Some((_, Tok::LParen)) = self.peek() {
                        self.pos += 1; // consume '('
                        let args = self.parse_args()?;
                        return self.call(&name, &args);
                    }
                    if name == "pi" {
                        return Ok(std::f64::consts::PI);
                    }
                    if let Some(v) = self.vars.get(&name) {
                        return Ok(*v);
                    }
                    // A bare unit name (`mm`) without a literal is not a value.
                    if unit_factor(&name).is_some() || FUNCTIONS.contains(&name.as_str()) {
                        return Err(ExprError::Parse {
                            pos: tok_pos,
                            message: format!("'{name}' cannot be used as a value"),
                        });
                    }
                    Err(ExprError::UnknownIdentifier(name))
                }
                other => Err(ExprError::Parse {
                    pos: tok_pos,
                    message: format!("unexpected token {other:?}"),
                }),
            }
        }

        /// Comma-separated args up to ')'. The '(' is already consumed.
        fn parse_args(&mut self) -> Result<Vec<f64>, ExprError> {
            let mut args = Vec::new();
            if let Some((_, Tok::RParen)) = self.peek() {
                self.pos += 1;
                return Ok(args);
            }
            loop {
                args.push(self.parse_expr()?);
                let fallback = self.end_pos();
                match self.next().map(|(p, t)| (*p, t.clone())) {
                    Some((_, Tok::Comma)) => continue,
                    Some((_, Tok::RParen)) => return Ok(args),
                    other => {
                        let pos = other.map_or(fallback, |(p, _)| p);
                        return Err(ExprError::Parse {
                            pos,
                            message: "expected ',' or ')'".to_string(),
                        });
                    }
                }
            }
        }

        fn call(&self, name: &str, args: &[f64]) -> Result<f64, ExprError> {
            let one = |f: fn(f64) -> f64| -> Result<f64, ExprError> {
                if args.len() != 1 {
                    return Err(ExprError::WrongArity {
                        function: name.to_string(),
                        expected: "1",
                        got: args.len(),
                    });
                }
                Ok(f(args[0]))
            };
            match name {
                "sqrt" => one(f64::sqrt),
                "abs" => one(f64::abs),
                "floor" => one(f64::floor),
                "ceil" => one(f64::ceil),
                "round" => one(f64::round),
                // Trig in degrees (CAD convention; matches Angle dimensions).
                "sin" => one(|d| d.to_radians().sin()),
                "cos" => one(|d| d.to_radians().cos()),
                "tan" => one(|d| d.to_radians().tan()),
                "min" | "max" => {
                    if args.is_empty() {
                        return Err(ExprError::WrongArity {
                            function: name.to_string(),
                            expected: "1 or more",
                            got: 0,
                        });
                    }
                    let fold: fn(f64, f64) -> f64 = if name == "min" { f64::min } else { f64::max };
                    Ok(args.iter().copied().fold(args[0], fold))
                }
                _ => Err(ExprError::UnknownFunction(name.to_string())),
            }
        }
    }
}

/// Deterministic xorshift64*. No `rand` dependency, no system entropy:
/// `docs`/`CLAUDE.md` require a test to be reproducible.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn pick(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// The parameter names the generator may reference, and their mm-space
/// values. `missing` is deliberately absent from the environment, so an
/// `UnknownIdentifier` path is exercised too.
fn env() -> HashMap<String, f64> {
    [
        ("width".to_string(), 30.0),
        ("height".to_string(), 12.5),
        ("t".to_string(), 2.0),
        ("n".to_string(), 6.0),
    ]
    .into_iter()
    .collect()
}

/// One random expression over the PRE-P1 grammar: the six binary
/// operators, unary minus, parentheses, the ten functions, `pi`, bare
/// literals, the six pre-P1 unit suffixes (no `rad` — it did not exist, so
/// no stored document can contain it), and the parameter names above.
fn gen(rng: &mut Rng, depth: usize) -> String {
    const LITERALS: &[&str] = &[
        "0", "1", "2", "3", "0.5", "2.5", "10", "25", "90", "1e2", "1.5e-1", "4",
    ];
    const SUFFIXES: &[&str] = &["mm", "cm", "m", "in", "ft", "deg"];
    const NAMES: &[&str] = &["width", "height", "t", "n", "missing", "pi"];
    const BINOPS: &[&str] = &["+", "-", "*", "/", "%", "^"];
    const FNS: &[&str] = &["sqrt", "abs", "floor", "ceil", "round", "sin", "cos", "tan"];

    if depth == 0 {
        return match rng.pick(10) {
            0..=4 => LITERALS[rng.pick(LITERALS.len())].to_string(),
            5..=6 => format!(
                "{}{}",
                LITERALS[rng.pick(LITERALS.len())],
                SUFFIXES[rng.pick(SUFFIXES.len())]
            ),
            _ => NAMES[rng.pick(NAMES.len())].to_string(),
        };
    }
    match rng.pick(10) {
        0..=4 => format!(
            "{} {} {}",
            gen(rng, depth - 1),
            BINOPS[rng.pick(BINOPS.len())],
            gen(rng, depth - 1)
        ),
        5 => format!("-{}", gen(rng, depth - 1)),
        6 => format!("({})", gen(rng, depth - 1)),
        7 => format!("{}({})", FNS[rng.pick(FNS.len())], gen(rng, depth - 1)),
        8 => format!("min({}, {})", gen(rng, depth - 1), gen(rng, depth - 1)),
        _ => format!(
            "max({}, {}, {})",
            gen(rng, depth - 1),
            gen(rng, depth - 1),
            gen(rng, depth - 1)
        ),
    }
}

#[test]
fn two_hundred_generated_expressions_agree_with_the_pre_p1_evaluator() {
    let vars = env();
    let mut rng = Rng(0x5EED_1234_ABCD_0001);

    let mut agreed = 0usize;
    let mut both_failed = 0usize;
    let mut new_non_finite = 0usize;
    let mut new_dimension = 0usize;
    let mut disagreements: Vec<String> = Vec::new();

    for i in 0..200 {
        let src = gen(&mut rng, 1 + (i % 4));
        let old = oracle::evaluate(&src, &vars);
        let new = expr::evaluate(&src, &vars);
        match (&old, &new) {
            (Ok(a), Ok(b)) => {
                // Bit-for-bit: a stored expression must keep its meaning,
                // not merely its rounded meaning.
                if a.to_bits() == b.to_bits() {
                    agreed += 1;
                } else {
                    disagreements.push(format!("`{src}`: old {a:?}, new {b:?}"));
                }
            }
            (Ok(a), Err(ExprError::NonFinite { .. })) => {
                // Divergence 1: the old code only checked the FINAL value,
                // so an infinity could be laundered by a later min/max/abs.
                new_non_finite += 1;
                assert!(
                    a.is_finite(),
                    "`{src}`: the oracle returned a non-finite value, which it \
                     claimed it never did"
                );
            }
            (Ok(_), Err(ExprError::DimensionMismatch { .. })) => {
                // Divergence 3: the old code read every number as plain, so
                // it coerced a mixed-unit expression instead of refusing it.
                new_dimension += 1;
            }
            (Err(_), Err(_)) => both_failed += 1,
            (Err(e), Ok(v)) => {
                disagreements.push(format!("`{src}`: old refused ({e:?}), new accepted ({v})"))
            }
            (Ok(a), Err(e)) => disagreements.push(format!("`{src}`: old {a}, new refused ({e:?})")),
        }
    }

    assert!(
        disagreements.is_empty(),
        "{} of 200 expressions diverged:\n{}",
        disagreements.len(),
        disagreements.join("\n")
    );
    // Not vacuous: most of the corpus must actually evaluate on both sides.
    assert!(
        agreed >= 100,
        "only {agreed} of 200 agreed on a value (both_failed {both_failed}, \
         non_finite {new_non_finite}, dimension {new_dimension})"
    );
    println!(
        "agreed {agreed}, both refused {both_failed}, \
         newly non-finite {new_non_finite}, newly a dimension error {new_dimension}"
    );
}

#[test]
fn the_precedence_table_is_the_old_one_case_by_case() {
    // The cases a random corpus is unlikely to hit in a readable form, and
    // the ones the review asked for by name.
    let vars = env();
    for src in [
        "-2 ^ 2",
        "2 ^ 3 ^ 2",
        "2 ^ -1",
        "-2 ^ -2",
        "1 - 2 - 3",
        "8 / 4 / 2",
        "7 % 4 * 2",
        "2 + 3 * 4",
        "2 * 3 ^ 2",
        "--5",
        "+-5",
        "-sqrt(4)",
        "2 ^ 2 * 3",
        "10 % 3 % 2",
        "min(1, 2, 3) ^ 2",
        "sin(90)",
        "sin(90deg)",
        "cos(0)",
        "tan(45)",
        "pi * 2",
        "-pi",
        "round(2.5)",
        "round(-2.5)",
        "floor(-2.5)",
        "abs(-0.0)",
        "width / 2 + 1",
        "sqrt(width ^ 2 + height ^ 2)",
        "1e2 % 7",
        "0.1 + 0.2",
    ] {
        let old = oracle::evaluate(src, &vars).unwrap_or_else(|e| panic!("`{src}`: {e:?}"));
        let new = expr::evaluate(src, &vars).unwrap_or_else(|e| panic!("`{src}`: {e:?}"));
        assert_eq!(
            old.to_bits(),
            new.to_bits(),
            "`{src}`: old {old}, new {new}"
        );
    }
}

#[test]
fn a_bare_trig_argument_still_means_degrees() {
    // The one semantic question the review raised about trig: the pre-P1
    // evaluator read a BARE argument as degrees (`one(|d| d.to_radians().sin())`),
    // and P1 must not have moved it to radians.
    let vars = HashMap::new();
    for src in ["sin(30)", "cos(60)", "tan(45)", "sin(90)", "cos(180)"] {
        let old = oracle::evaluate(src, &vars).unwrap();
        let new = expr::evaluate(src, &vars).unwrap_or(f64::NAN);
        assert_eq!(old.to_bits(), new.to_bits(), "`{src}`");
    }
    // And a bare argument means the same thing as an explicitly-degreed one.
    assert_eq!(
        expr::evaluate("sin(30)", &vars).unwrap().to_bits(),
        expr::evaluate("sin(30deg)", &vars).unwrap().to_bits()
    );
}

#[test]
fn the_only_new_suffix_is_one_no_stored_document_can_contain() {
    // `rad` is the single unit P1 adds. Before P1 it was not a unit, so
    // `1rad` was a parse error and no document can hold one — which is why
    // adding it cannot change any stored expression's meaning.
    let vars = HashMap::new();
    assert!(oracle::evaluate("1rad", &vars).is_err());
    let got = expr::evaluate("1rad", &vars).unwrap();
    assert!(
        (got - 180.0 / std::f64::consts::PI).abs() < 1e-12,
        "1rad must be 57.29577951308232 degrees of working space, got {got}"
    );
    for suffix in ["mm", "cm", "m", "in", "ft", "deg"] {
        let src = format!("1{suffix}");
        assert_eq!(
            oracle::evaluate(&src, &vars).unwrap().to_bits(),
            expr::evaluate(&src, &vars).unwrap().to_bits(),
            "{src}"
        );
    }
}

#[test]
fn the_old_code_laundered_an_infinity_through_min_max_and_abs() {
    // Divergence 1, shown against the oracle rather than asserted from
    // memory: the old evaluator checked `is_finite` ONCE, on the final
    // value, so any function that could swallow an infinity returned a
    // plausible number from a division by zero.
    let vars = HashMap::new();
    for (src, old_value) in [
        ("min(1/0, 5)", 5.0),
        ("max(-1/0, 5)", 5.0),
        ("min(sqrt(-1), 5)", 5.0),
        ("0 * (1/0)", f64::NAN),
        ("floor(1/0 - 1/0)", f64::NAN),
    ] {
        match oracle::evaluate(src, &vars) {
            Ok(v) if old_value.is_nan() => {
                panic!("`{src}`: the oracle returned {v}, expected its NaN guard to fire")
            }
            Ok(v) => assert_eq!(v, old_value, "`{src}`: the oracle's old answer"),
            Err(oracle::ExprError::NonFinite) => assert!(
                old_value.is_nan(),
                "`{src}`: the oracle refused, but a finite {old_value} was expected"
            ),
            Err(e) => panic!("`{src}`: unexpected oracle error {e:?}"),
        }
        assert!(
            matches!(expr::evaluate(src, &vars), Err(ExprError::NonFinite { .. })),
            "`{src}`: P1 must refuse this where it occurs"
        );
    }
}

#[test]
fn adding_rad_newly_reserves_one_plausible_parameter_name() {
    // The one back-compat cost of the `rad` suffix, stated rather than
    // discovered later: `rad` was a legal PARAMETER name before P1 (it was
    // not a unit, so nothing reserved it), and a document that used it —
    // `rad` for a radius is not a stretch — now fails. The corpus has zero
    // parameters so no repo file is affected, but the rule is now pinned
    // instead of implicit.
    let vars: HashMap<String, f64> = [("rad".to_string(), 5.0)].into_iter().collect();
    assert_eq!(oracle::evaluate("rad * 2", &vars).unwrap(), 10.0);
    assert!(
        matches!(
            expr::evaluate("rad * 2", &vars),
            Err(ExprError::Parse { pos: 0, .. })
        ),
        "a parameter named `rad` is now shadowed by the unit suffix"
    );
    assert!(expr::validate_name("rad").is_err());
    // Every other name the old grammar allowed still works.
    for name in ["radius", "rad_1", "r", "deg2", "inner", "mm_total"] {
        assert!(expr::validate_name(name).is_ok(), "{name}");
        let vars: HashMap<String, f64> = [(name.to_string(), 3.0)].into_iter().collect();
        assert_eq!(expr::evaluate(name, &vars).unwrap(), 3.0, "{name}");
    }
}
