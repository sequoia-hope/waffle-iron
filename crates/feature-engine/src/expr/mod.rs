//! Arithmetic expression parser and evaluator for design parameters.
//!
//! Expressions drive measurements (sketch dimensions, extrude depth, revolve
//! angle, datum offsets) from named design variables on the feature tree
//! (`FeatureTree::parameters`). See `specs/parameterized_designs.md` and
//! `specs/agent_mechanical_design.md` §6 (increment P1).
//!
//! ## Two passes, not one
//!
//! [`parse`] produces an [`Expr`] AST; [`eval`] walks it against an
//! environment. Parsing is independent of the environment, so an
//! expression's *dependencies* can be read off the tree
//! ([`Expr::identifiers`]) without evaluating it — which is what the
//! parameter table's dependency lists need. The evaluator never re-lexes.
//!
//! ## Unit convention: mm-space working numbers
//!
//! The evaluator's magnitudes live in a fixed working space, unchanged from
//! the pre-P1 evaluator so that every stored expression keeps its meaning:
//!
//! - a **length** magnitude is MILLIMETRES, converted to model metres by
//!   [`Quantity::as_length_meters`] (the factor is [`MM_TO_METERS`]);
//! - an **angle** magnitude is DEGREES, which is also what every angle field
//!   on the feature tree stores ([`Quantity::as_angle_degrees`]);
//!   [`Quantity::as_angle_radians`] converts to the model's base angular
//!   unit for a consumer that wants it;
//! - a **count** or **ratio** magnitude is the plain number.
//!
//! Conversion out of the working space happens ONLY at a typed boundary —
//! the `as_*` methods above — never inside the arithmetic. This is
//! deliberately independent of the document's *display* unit: switching the
//! display from mm to inches must never rescale expression-driven geometry.
//!
//! ## Dimensions
//!
//! A numeric literal may carry a unit suffix (`25mm`, `1.5in`, `2 cm`,
//! `90deg`, `1rad`). A suffix does two things: it scales the literal into
//! the working space, and it **commits** the value's dimension
//! ([`Tag::Tagged`]). A bare literal stays [`Tag::Untagged`] — a plain
//! number that adopts whatever dimension its context asks for, which is
//! exactly the pre-P1 meaning of `25` (25 mm in a length field, 25° in an
//! angle field).
//!
//! Dimensions compose through the arithmetic as exponents ([`Dim`]), so
//! `w * h` is a length², `w * h / t` is a length again, and `10mm + 90deg`
//! is a typed [`ExprError::DimensionMismatch`] naming the byte offset of
//! the suffix that disagreed. A committed dimension that reaches the wrong
//! field is refused there, not coerced: `25deg` in an extrude depth is an
//! error, where before P1 it silently meant 25 mm.
//!
//! ## Grammar
//!
//! ```text
//! expr    := mul (('+' | '-') mul)*
//! mul     := unary (('*' | '/' | '%') unary)*
//! unary   := ('-' | '+') unary | power
//! power   := primary ('^' unary)?             // right-associative
//! primary := number unit? | ident | ident '(' args? ')' | '(' expr ')'
//! args    := expr (',' expr)*
//! ```
//!
//! `^` binds tighter than unary minus (`-2^2 == -4`). The constant `pi` is a
//! bare number. Trig functions take and return degrees. Every intermediate
//! result must be finite: a non-finite subexpression is an error where it
//! occurs, not a value that can be hidden by a later `min`.

mod dim;
mod eval;
mod lex;
mod parse;

use std::collections::HashMap;
use std::fmt;

pub use dim::{Dim, Dimension, Quantity, Tag, Unit, UNITS};
pub use eval::{eval, Env};
pub use parse::{BinOp, Expr, UnOp};

/// Scale factor from the evaluator's mm-space numbers to internal meters.
pub const MM_TO_METERS: f64 = 1e-3;

/// Most lexemes one expression may contain.
///
/// Both the parser and the evaluator walk the tree recursively, and a left-
/// associative chain (`1+1+1+…`) parses iteratively but *evaluates* down a
/// tree whose depth is half the token count. Measured on this box (8 MB
/// native stack, 2026-10-03): such a chain overflows the stack somewhere
/// between 3 000 and 4 000 terms, so the evaluator's own safe depth is
/// ~1 500 on the WASM worker's 4 MB stack. This bound keeps the deepest
/// reachable evaluation at ~512 — a 3× margin — and no real design
/// expression comes near 1 024 lexemes.
pub const MAX_LEXEMES: usize = 1024;

/// Deepest nesting (parentheses, call arguments, chained unary or `^`) the
/// parser will descend.
///
/// Separate from [`MAX_LEXEMES`] because nesting costs several stack frames
/// per level: a parenthesis nest overflows between 1 500 and 3 000 levels
/// natively, so ~750 on the WASM worker. 64 is far below that and far above
/// anything a person or an agent writes.
pub const MAX_DEPTH: usize = 64;

/// Function names (all reserved as identifiers). Trig is in DEGREES.
pub const FUNCTIONS: &[&str] = &[
    "sqrt", "abs", "floor", "ceil", "round", "sin", "cos", "tan", "min", "max",
];

/// A half-open byte range of the source expression. Every diagnostic that
/// can point at a place in the input carries one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// The smallest span covering both.
    pub fn join(self, other: Self) -> Self {
        Self {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}..{}", self.start, self.end)
    }
}

/// The multiplier for a unit-suffix name, if `name` is one. The factor is
/// into the working space (mm for a length, degrees for an angle).
pub fn unit_factor(name: &str) -> Option<f64> {
    dim::unit_by_name(name).map(|u| u.factor)
}

/// The unit with this suffix name, if any.
pub fn unit_by_name(name: &str) -> Option<&'static Unit> {
    dim::unit_by_name(name)
}

/// True if `name` may not be used as a parameter name (unit suffixes,
/// function names, constants).
pub fn is_reserved_word(name: &str) -> bool {
    name == "pi" || dim::unit_by_name(name).is_some() || FUNCTIONS.contains(&name)
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

/// Parse `input` into an AST without evaluating it.
pub fn parse(input: &str) -> Result<Expr, ExprError> {
    parse::parse(input)
}

/// Parse and evaluate `input` against `env`, keeping the dimension tag.
/// The caller accepts the result at a typed boundary
/// ([`Quantity::as_length_meters`] and friends).
pub fn evaluate_quantity(input: &str, env: &Env) -> Result<Quantity, ExprError> {
    eval(&parse(input)?, env)
}

/// Evaluate `input` against `vars` (parameter name → working-space value),
/// returning the working-space magnitude and DISCARDING the dimension.
///
/// This is the dimension-agnostic shim for callers that only want a number
/// (a preview, a script argument cache). A field that means something —
/// a depth, an angle, a count — must use [`evaluate_quantity`] and accept
/// the result for its own [`Dimension`], so a committed unit that does not
/// belong cannot be read as a plain number.
pub fn evaluate(input: &str, vars: &HashMap<String, f64>) -> Result<f64, ExprError> {
    Ok(evaluate_quantity(input, &untagged_env(vars))?.value)
}

/// An environment of plain numbers: every value [`Tag::Untagged`], so it
/// adopts whatever dimension its context asks for.
pub fn untagged_env(vars: &HashMap<String, f64>) -> Env {
    vars.iter()
        .map(|(k, v)| (k.clone(), Quantity::untagged(*v)))
        .collect()
}

/// Evaluation failure. `Display` gives a user-facing message.
#[derive(Debug, Clone, PartialEq)]
pub enum ExprError {
    /// The expression is empty or all whitespace.
    Empty,
    /// Tokenizer/parser failure at a byte offset.
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
    /// A result (or an intermediate) is NaN/infinite — division by zero,
    /// `sqrt` of a negative, an overflowing power.
    NonFinite { span: Span },
    /// A dimension that cannot be what the context needs: mixed units inside
    /// the expression, or a committed unit reaching the wrong field. `span`
    /// points at the suffix that committed the offending dimension.
    DimensionMismatch {
        expected: String,
        found: String,
        span: Span,
    },
    /// A function called outside its usable domain (`tan` at its pole).
    FunctionDomain {
        function: &'static str,
        message: String,
        span: Span,
    },
    /// A count field got a value that is not a whole non-negative number.
    NotACount { value: f64 },
    /// The expression is bigger or more deeply nested than the parser will
    /// walk. Both the parser and the evaluator are recursive, so without a
    /// bound a nested-parenthesis expression overflows the stack — in WASM
    /// a trap that kills the engine, not a catchable error. `what` names
    /// the bound that was hit.
    TooComplex { what: &'static str, limit: usize },
}

impl ExprError {
    /// The byte range this error points at, when it points at one.
    pub fn span(&self) -> Option<Span> {
        match self {
            ExprError::Parse { pos, .. } => Some(Span::new(*pos, *pos)),
            ExprError::NonFinite { span }
            | ExprError::DimensionMismatch { span, .. }
            | ExprError::FunctionDomain { span, .. } => Some(*span),
            ExprError::Empty
            | ExprError::UnknownIdentifier(_)
            | ExprError::UnknownFunction(_)
            | ExprError::WrongArity { .. }
            | ExprError::NotACount { .. }
            | ExprError::TooComplex { .. } => None,
        }
    }

    /// The byte offset this error points at, when it points at one.
    pub fn offset(&self) -> Option<usize> {
        self.span().map(|s| s.start)
    }
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
            ExprError::NonFinite { span } => {
                write!(f, "result is not a finite number (at bytes {span})")
            }
            ExprError::DimensionMismatch {
                expected,
                found,
                span,
            } => write!(f, "expected {expected}, got {found} (at bytes {span})"),
            ExprError::FunctionDomain {
                function,
                message,
                span,
            } => write!(f, "{function}(): {message} (at bytes {span})"),
            ExprError::NotACount { value } => {
                write!(f, "expected a whole non-negative count, got {value}")
            }
            ExprError::TooComplex { what, limit } => {
                write!(f, "expression is too complex ({what} exceeds {limit})")
            }
        }
    }
}

impl std::error::Error for ExprError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserved_words_cover_units_functions_and_pi() {
        for name in [
            "mm", "cm", "m", "in", "ft", "deg", "rad", "pi", "sqrt", "min",
        ] {
            assert!(is_reserved_word(name), "{name} must be reserved");
            assert!(validate_name(name).is_err(), "{name} must not be a name");
        }
        assert!(!is_reserved_word("width"));
        assert!(validate_name("width").is_ok());
        assert!(validate_name("_a1").is_ok());
        assert!(validate_name("1abc").is_err());
        assert!(validate_name("").is_err());
        assert!(validate_name("a-b").is_err());
    }

    #[test]
    fn unit_factors_are_working_space_multipliers() {
        assert_eq!(unit_factor("mm"), Some(1.0));
        assert_eq!(unit_factor("cm"), Some(10.0));
        assert_eq!(unit_factor("m"), Some(1000.0));
        assert_eq!(unit_factor("in"), Some(25.4));
        assert_eq!(unit_factor("ft"), Some(304.8));
        assert_eq!(unit_factor("deg"), Some(1.0));
        assert_eq!(unit_factor("rad"), Some(180.0 / std::f64::consts::PI));
        assert_eq!(unit_factor("furlong"), None);
    }

    #[test]
    fn spans_join_and_display() {
        let s = Span::new(2, 5).join(Span::new(7, 9));
        assert_eq!(s, Span::new(2, 9));
        assert_eq!(s.to_string(), "2..9");
    }

    #[test]
    fn errors_report_their_offset() {
        assert_eq!(
            ExprError::Parse {
                pos: 4,
                message: "x".into()
            }
            .offset(),
            Some(4)
        );
        assert_eq!(
            ExprError::NonFinite {
                span: Span::new(3, 8)
            }
            .offset(),
            Some(3)
        );
        assert_eq!(ExprError::Empty.offset(), None);
    }
}
