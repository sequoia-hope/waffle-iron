//! Arithmetic expression evaluator for design parameters.
//!
//! Expressions drive measurements (sketch dimensions, extrude depth, revolve
//! angle, datum offsets) from named design variables on the feature tree
//! (`FeatureTree::parameters`). See `specs/parameterized_designs.md`.
//!
//! ## Unit convention: mm-space
//!
//! Expressions evaluate to plain numbers with a fixed interpretation:
//!
//! - **Length contexts** read the result as MILLIMETERS (converted to meters
//!   via [`MM_TO_METERS`] at the assignment site).
//! - **Angle contexts** read the result as DEGREES, verbatim.
//!
//! Numeric literals may carry a unit suffix (`25mm`, `1.5in`, `2 cm`) which
//! scales them into mm-space; bare literals are already mm-space (so `25`
//! means 25 mm in a length, 25° in an angle). This is deliberately
//! independent of the document's *display* unit — switching the display from
//! mm to inches must never rescale expression-driven geometry.
//!
//! Grammar: `+ - * / % ^` with standard precedence (`^` right-associative,
//! binding tighter than unary minus), parentheses, function calls, the
//! constant `pi`, and identifiers resolved against the parameter environment.
//! Trig functions take and return degrees.

use std::collections::HashMap;
use std::fmt;

/// Scale factor from the evaluator's mm-space numbers to internal meters.
pub const MM_TO_METERS: f64 = 1e-3;

/// Unit suffixes accepted after numeric literals, as multipliers into
/// mm-space. `deg` is an identity marker so angle literals can be explicit.
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

/// The multiplier for a unit-suffix name, if `name` is one.
pub fn unit_factor(name: &str) -> Option<f64> {
    UNIT_FACTORS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, f)| *f)
}

/// True if `name` may not be used as a parameter name (unit suffixes,
/// function names, constants).
pub fn is_reserved_word(name: &str) -> bool {
    name == "pi" || unit_factor(name).is_some() || FUNCTIONS.contains(&name)
}

/// Validate a parameter name: identifier syntax, not reserved.
pub fn validate_name(name: &str) -> Result<(), String> {
    let mut chars = name.chars();
    match chars.next() {
        None => return Err("name is empty".to_string()),
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        Some(c) => return Err(format!("name must start with a letter or '_' (got '{c}')")),
    }
    if let Some(c) = chars.find(|c| !c.is_ascii_alphanumeric() && *c != '_') {
        return Err(format!("name contains invalid character '{c}'"));
    }
    if is_reserved_word(name) {
        return Err(format!("'{name}' is a reserved word"));
    }
    Ok(())
}

/// Evaluation failure. `Display` gives a user-facing message.
#[derive(Debug, Clone, PartialEq)]
pub enum ExprError {
    /// The expression is empty or all whitespace.
    Empty,
    /// Tokenizer/parser failure at a character offset.
    Parse { pos: usize, message: String },
    /// An identifier that is neither a parameter, unit, nor constant.
    UnknownIdentifier(String),
    /// A call to a name that is not a function.
    UnknownFunction(String),
    /// A function called with the wrong number of arguments.
    WrongArity {
        function: String,
        expected: &'static str,
        got: usize,
    },
    /// The result (or an intermediate) is NaN/infinite (e.g. division by
    /// zero, sqrt of a negative).
    NonFinite,
}

impl fmt::Display for ExprError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExprError::Empty => write!(f, "expression is empty"),
            ExprError::Parse { pos, message } => {
                write!(f, "parse error at position {pos}: {message}")
            }
            ExprError::UnknownIdentifier(name) => write!(f, "unknown variable '{name}'"),
            ExprError::UnknownFunction(name) => write!(f, "unknown function '{name}'"),
            ExprError::WrongArity {
                function,
                expected,
                got,
            } => write!(f, "{function}() takes {expected} argument(s), got {got}"),
            ExprError::NonFinite => write!(f, "result is not a finite number"),
        }
    }
}

impl std::error::Error for ExprError {}

/// Evaluate `input` against `vars` (parameter name → mm-space value).
/// Returns the mm-space result, guaranteed finite.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn eval(s: &str) -> Result<f64, ExprError> {
        evaluate(s, &HashMap::new())
    }

    fn eval_with(s: &str, vars: &[(&str, f64)]) -> Result<f64, ExprError> {
        let map: HashMap<String, f64> = vars.iter().map(|(k, v)| (k.to_string(), *v)).collect();
        evaluate(s, &map)
    }

    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1e-12 * expected.abs().max(1.0),
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn literals_and_arithmetic() {
        assert_close(eval("25").unwrap(), 25.0);
        assert_close(eval("2 + 3 * 4").unwrap(), 14.0);
        assert_close(eval("(2 + 3) * 4").unwrap(), 20.0);
        assert_close(eval("10 / 4").unwrap(), 2.5);
        assert_close(eval("7 % 4").unwrap(), 3.0);
        assert_close(eval("2 ^ 10").unwrap(), 1024.0);
        assert_close(eval("2 ^ 3 ^ 2").unwrap(), 512.0); // right-assoc
        assert_close(eval("-2 ^ 2").unwrap(), -4.0); // power binds tighter
        assert_close(eval("1.5e2").unwrap(), 150.0);
        assert_close(eval("1e-3").unwrap(), 0.001);
        assert_close(eval("--5").unwrap(), 5.0);
        assert_close(eval("+5").unwrap(), 5.0);
    }

    #[test]
    fn unit_suffixes_scale_into_mm_space() {
        assert_close(eval("25mm").unwrap(), 25.0);
        assert_close(eval("2 cm").unwrap(), 20.0);
        assert_close(eval("1m").unwrap(), 1000.0);
        assert_close(eval("1in").unwrap(), 25.4);
        assert_close(eval("1 ft").unwrap(), 304.8);
        assert_close(eval("90 deg").unwrap(), 90.0);
        assert_close(eval("1in + 2mm").unwrap(), 27.4);
    }

    #[test]
    fn variables_resolve() {
        assert_close(eval_with("width", &[("width", 30.0)]).unwrap(), 30.0);
        assert_close(
            eval_with("width / 2 + 1", &[("width", 30.0)]).unwrap(),
            16.0,
        );
        assert_eq!(
            eval("nope"),
            Err(ExprError::UnknownIdentifier("nope".to_string()))
        );
    }

    #[test]
    fn functions_work_and_trig_is_degrees() {
        assert_close(eval("sqrt(16)").unwrap(), 4.0);
        assert_close(eval("abs(-3)").unwrap(), 3.0);
        assert_close(eval("floor(2.7)").unwrap(), 2.0);
        assert_close(eval("ceil(2.2)").unwrap(), 3.0);
        assert_close(eval("round(2.5)").unwrap(), 3.0);
        assert_close(eval("sin(30)").unwrap(), 0.5);
        assert_close(eval("cos(60)").unwrap(), 0.5);
        assert_close(eval("tan(45)").unwrap(), 1.0);
        assert_close(eval("min(3, 1, 2)").unwrap(), 1.0);
        assert_close(eval("max(3, 1, 2)").unwrap(), 3.0);
        assert_close(eval("pi").unwrap(), std::f64::consts::PI);
        assert_close(eval("2 * pi * 5").unwrap(), 31.41592653589793);
    }

    #[test]
    fn errors_are_loud() {
        assert_eq!(eval(""), Err(ExprError::Empty));
        assert_eq!(eval("   "), Err(ExprError::Empty));
        assert!(matches!(eval("1 / 0"), Err(ExprError::NonFinite)));
        assert!(matches!(eval("sqrt(-1)"), Err(ExprError::NonFinite)));
        assert!(matches!(eval("2 +"), Err(ExprError::Parse { .. })));
        assert!(matches!(eval("(1 + 2"), Err(ExprError::Parse { .. })));
        assert!(matches!(eval("1 + $"), Err(ExprError::Parse { .. })));
        assert!(matches!(eval("5 width"), Err(ExprError::Parse { .. })));
        assert!(matches!(eval("mm"), Err(ExprError::Parse { .. })));
        assert!(matches!(
            eval("sqrt(1, 2)"),
            Err(ExprError::WrongArity { .. })
        ));
        assert!(matches!(eval("min()"), Err(ExprError::WrongArity { .. })));
        assert!(matches!(
            eval("bogus(1)"),
            Err(ExprError::UnknownFunction(_))
        ));
        assert!(matches!(eval("1 2"), Err(ExprError::Parse { .. })));
    }

    #[test]
    fn variable_shadowing_reserved_is_impossible() {
        // Even if a caller sneaks a reserved name into the env, literals with
        // unit suffixes keep their meaning and bare unit names stay rejected.
        let r = eval_with("25mm", &[("mm", 999.0)]);
        assert_close(r.unwrap(), 25.0);
        assert!(validate_name("mm").is_err());
        assert!(validate_name("pi").is_err());
        assert!(validate_name("sqrt").is_err());
        assert!(validate_name("width").is_ok());
        assert!(validate_name("_a1").is_ok());
        assert!(validate_name("1abc").is_err());
        assert!(validate_name("").is_err());
        assert!(validate_name("a-b").is_err());
    }
}
