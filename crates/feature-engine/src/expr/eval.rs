//! AST evaluation with dimension tracking.
//!
//! Each node produces a [`Quantity`]: a working-space magnitude and a
//! dimension tag. Two invariants hold at every node:
//!
//! 1. **Finite.** A non-finite intermediate is an error *where it occurs*,
//!    not a value a later operation can hide (`min(1/0, 5)` is an error,
//!    not `5`).
//! 2. **Dimensionally consistent.** `+`, `-`, `%`, `min` and `max` unify
//!    their operands' dimensions; `*` and `/` compose them; `^` requires a
//!    plain exponent. An uncommitted number unifies with anything, which is
//!    what keeps `width + 1` meaning "one more millimetre".

use std::collections::HashMap;

use super::dim::{Dim, Dimension, Quantity, Tag};
use super::parse::{BinOp, Expr, UnOp};
use super::{ExprError, Span};

/// Parameter name → its value and dimension.
pub type Env = HashMap<String, Quantity>;

/// `tan` is refused when `|cos θ|` is below this: the result would exceed
/// 1e9 working-space units (a kilometre of millimetres), which is numerical
/// noise rather than a dimension anyone drew.
const TAN_POLE_COS: f64 = 1e-9;

/// Evaluate `ast` against `env`.
pub fn eval(ast: &Expr, env: &Env) -> Result<Quantity, ExprError> {
    match ast {
        Expr::Number { value, unit, span } => Ok(match unit {
            Some(u) => Quantity::tagged(*value, u.dim, *span),
            None => Quantity::untagged(*value),
        }),
        Expr::Ident { name, .. } => env
            .get(name)
            .copied()
            .ok_or_else(|| ExprError::UnknownIdentifier(name.clone())),
        Expr::Unary { op, operand, span } => {
            let q = eval(operand, env)?;
            let value = match op {
                UnOp::Neg => -q.value,
                UnOp::Pos => q.value,
            };
            finite(value, q.tag, *span)
        }
        Expr::Binary { op, lhs, rhs, span } => {
            let l = eval(lhs, env)?;
            let r = eval(rhs, env)?;
            let tag = match op {
                BinOp::Add | BinOp::Sub | BinOp::Rem => unify(l.tag, r.tag)?,
                BinOp::Mul => compose(l.tag, r.tag, 1),
                BinOp::Div => compose(l.tag, r.tag, -1),
                BinOp::Pow => pow_tag(l.tag, r, *span)?,
            };
            let value = match op {
                BinOp::Add => l.value + r.value,
                BinOp::Sub => l.value - r.value,
                BinOp::Mul => l.value * r.value,
                BinOp::Div => l.value / r.value,
                BinOp::Rem => l.value % r.value,
                BinOp::Pow => l.value.powf(r.value),
            };
            finite(value, tag, *span)
        }
        Expr::Call { name, args, span } => call(name, args, env, *span),
    }
}

/// Wrap a computed magnitude, refusing a non-finite one at its own span.
fn finite(value: f64, tag: Tag, span: Span) -> Result<Quantity, ExprError> {
    if !value.is_finite() {
        return Err(ExprError::NonFinite { span });
    }
    Ok(Quantity { value, tag })
}

/// Dimensions that must agree (`+`, `-`, `%`, `min`, `max`). An uncommitted
/// operand adopts the other's dimension.
fn unify(a: Tag, b: Tag) -> Result<Tag, ExprError> {
    match (a, b) {
        (Tag::Untagged, Tag::Untagged) => Ok(Tag::Untagged),
        (Tag::Untagged, t) | (t, Tag::Untagged) => Ok(t),
        (Tag::Tagged { dim: d1, at: a1 }, Tag::Tagged { dim: d2, at: a2 }) => {
            if d1 == d2 {
                Ok(Tag::Tagged { dim: d1, at: a1 })
            } else {
                Err(ExprError::DimensionMismatch {
                    expected: d1.label(),
                    found: d2.label(),
                    span: a2,
                })
            }
        }
    }
}

/// Dimensions that compose (`*` with `sign = 1`, `/` with `sign = -1`).
/// Two plain numbers stay plain; otherwise the exponents add.
fn compose(a: Tag, b: Tag, sign: i8) -> Tag {
    match (a, b) {
        (Tag::Untagged, Tag::Untagged) => Tag::Untagged,
        (Tag::Tagged { dim, at }, Tag::Untagged) => Tag::Tagged { dim, at },
        (Tag::Untagged, Tag::Tagged { dim, at }) => Tag::Tagged {
            dim: Dim::NONE.compose(dim, sign),
            at,
        },
        (Tag::Tagged { dim: d1, at }, Tag::Tagged { dim: d2, .. }) => Tag::Tagged {
            dim: d1.compose(d2, sign),
            at,
        },
    }
}

/// The dimension of `base ^ exponent`.
///
/// An exponent carries no unit (`2 ^ 10mm` is meaningless), and a
/// dimensioned base needs a whole exponent — a half power of a length is
/// not a dimension this system can name.
fn pow_tag(base: Tag, exponent: Quantity, span: Span) -> Result<Tag, ExprError> {
    if let Tag::Tagged { dim, at } = exponent.tag {
        if dim != Dim::NONE {
            return Err(ExprError::DimensionMismatch {
                expected: "a plain number (an exponent carries no unit)".to_string(),
                found: dim.label(),
                span: at,
            });
        }
    }
    match base {
        Tag::Untagged => Ok(Tag::Untagged),
        Tag::Tagged { dim, at } if dim == Dim::NONE => Ok(Tag::Tagged { dim, at }),
        Tag::Tagged { dim, at } => {
            if exponent.value.fract() != 0.0 || exponent.value.abs() > i32::MAX as f64 {
                return Err(ExprError::DimensionMismatch {
                    expected: format!("a whole exponent (raising {} to a fraction)", dim.label()),
                    found: format!("{}", exponent.value),
                    span,
                });
            }
            Ok(Tag::Tagged {
                dim: dim.scaled(exponent.value as i32),
                at,
            })
        }
    }
}

fn call(name: &str, args: &[Expr], env: &Env, span: Span) -> Result<Quantity, ExprError> {
    // Arity was validated at parse time; evaluate what is there.
    let vals: Vec<Quantity> = args
        .iter()
        .map(|a| eval(a, env))
        .collect::<Result<_, _>>()?;
    match name {
        // Shape-preserving: the dimension rides through unchanged.
        "abs" => finite(vals[0].value.abs(), vals[0].tag, span),
        "floor" => finite(vals[0].value.floor(), vals[0].tag, span),
        "ceil" => finite(vals[0].value.ceil(), vals[0].tag, span),
        "round" => finite(vals[0].value.round(), vals[0].tag, span),
        "sqrt" => {
            let tag = match vals[0].tag {
                Tag::Untagged => Tag::Untagged,
                Tag::Tagged { dim, at } => {
                    let Some(half) = dim.halved() else {
                        return Err(ExprError::DimensionMismatch {
                            expected: "a dimension with even exponents (sqrt halves them)"
                                .to_string(),
                            found: dim.label(),
                            span: at,
                        });
                    };
                    Tag::Tagged { dim: half, at }
                }
            };
            finite(vals[0].value.sqrt(), tag, span)
        }
        // Trig takes DEGREES and returns a plain number. An uncommitted
        // argument is read as degrees, which is the pre-P1 meaning of
        // `sin(30)`; a length argument is refused.
        "sin" | "cos" | "tan" => {
            vals[0].check(Dimension::Angle)?;
            let radians = vals[0].value.to_radians();
            let value = match name {
                "sin" => radians.sin(),
                "cos" => radians.cos(),
                _ => {
                    if radians.cos().abs() < TAN_POLE_COS {
                        return Err(ExprError::FunctionDomain {
                            function: "tan",
                            message: format!(
                                "{} deg is at a pole (|cos| < {TAN_POLE_COS:e})",
                                vals[0].value
                            ),
                            span,
                        });
                    }
                    radians.tan()
                }
            };
            finite(value, Tag::Untagged, span)
        }
        "min" | "max" => {
            let mut tag = vals[0].tag;
            for v in &vals[1..] {
                tag = unify(tag, v.tag)?;
            }
            let fold: fn(f64, f64) -> f64 = if name == "min" { f64::min } else { f64::max };
            let value = vals[1..]
                .iter()
                .fold(vals[0].value, |a, v| fold(a, v.value));
            finite(value, tag, span)
        }
        // Parse-time validation makes this unreachable, but an unreachable!
        // here would be a panic in a library: report it instead.
        _ => Err(ExprError::UnknownFunction(name.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::super::{evaluate_quantity, untagged_env, Dim, Dimension};
    use super::*;

    fn q(s: &str) -> Result<Quantity, ExprError> {
        evaluate_quantity(s, &Env::new())
    }

    fn v(s: &str) -> f64 {
        q(s).unwrap().value
    }

    fn with(s: &str, vars: &[(&str, f64)]) -> Result<Quantity, ExprError> {
        let map: HashMap<String, f64> = vars.iter().map(|(k, v)| (k.to_string(), *v)).collect();
        evaluate_quantity(s, &untagged_env(&map))
    }

    fn close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1e-12 * expected.abs().max(1.0),
            "expected {expected}, got {actual}"
        );
    }

    // -- arithmetic: the pre-P1 numbers, unchanged --

    #[test]
    fn literals_and_arithmetic() {
        close(v("25"), 25.0);
        close(v("2 + 3 * 4"), 14.0);
        close(v("(2 + 3) * 4"), 20.0);
        close(v("10 / 4"), 2.5);
        close(v("7 % 4"), 3.0);
        close(v("2 ^ 10"), 1024.0);
        close(v("2 ^ 3 ^ 2"), 512.0); // right-assoc
        close(v("-2 ^ 2"), -4.0); // power binds tighter
        close(v("1.5e2"), 150.0);
        close(v("1e-3"), 0.001);
        close(v("--5"), 5.0);
        close(v("+5"), 5.0);
    }

    #[test]
    fn unit_suffixes_scale_into_the_working_space() {
        close(v("25mm"), 25.0);
        close(v("2 cm"), 20.0);
        close(v("1m"), 1000.0);
        close(v("1in"), 25.4);
        close(v("1 ft"), 304.8);
        close(v("90 deg"), 90.0);
        close(v("1in + 2mm"), 27.4);
        close(v("1 rad"), 180.0 / std::f64::consts::PI);
    }

    #[test]
    fn variables_resolve() {
        close(with("width", &[("width", 30.0)]).unwrap().value, 30.0);
        close(
            with("width / 2 + 1", &[("width", 30.0)]).unwrap().value,
            16.0,
        );
        assert_eq!(
            q("nope"),
            Err(ExprError::UnknownIdentifier("nope".to_string()))
        );
    }

    #[test]
    fn functions_work_and_trig_is_degrees() {
        close(v("sqrt(16)"), 4.0);
        close(v("abs(-3)"), 3.0);
        close(v("floor(2.7)"), 2.0);
        close(v("ceil(2.2)"), 3.0);
        close(v("round(2.5)"), 3.0);
        close(v("sin(30)"), 0.5);
        close(v("cos(60)"), 0.5);
        close(v("tan(45)"), 1.0);
        close(v("min(3, 1, 2)"), 1.0);
        close(v("max(3, 1, 2)"), 3.0);
        close(v("pi"), std::f64::consts::PI);
        close(v("2 * pi * 5"), 31.41592653589793);
    }

    // -- loud failures --

    #[test]
    fn syntax_and_name_errors_are_loud() {
        assert_eq!(q(""), Err(ExprError::Empty));
        assert!(matches!(q("2 +"), Err(ExprError::Parse { .. })));
        assert!(matches!(q("(1 + 2"), Err(ExprError::Parse { .. })));
        assert!(matches!(q("1 + $"), Err(ExprError::Parse { .. })));
        assert!(matches!(q("5 width"), Err(ExprError::Parse { .. })));
        assert!(matches!(q("mm"), Err(ExprError::Parse { .. })));
        assert!(matches!(q("sqrt(1, 2)"), Err(ExprError::WrongArity { .. })));
        assert!(matches!(q("min()"), Err(ExprError::WrongArity { .. })));
        assert!(matches!(q("bogus(1)"), Err(ExprError::UnknownFunction(_))));
        assert!(matches!(q("1 2"), Err(ExprError::Parse { .. })));
    }

    #[test]
    fn a_non_finite_intermediate_fails_where_it_occurs() {
        assert_eq!(
            q("1 / 0"),
            Err(ExprError::NonFinite {
                span: Span::new(0, 5)
            })
        );
        assert!(matches!(q("sqrt(-1)"), Err(ExprError::NonFinite { .. })));
        // The soft spot P1 closes: a later `min` used to hide the infinity.
        assert!(matches!(q("min(1/0, 5)"), Err(ExprError::NonFinite { .. })));
        assert!(matches!(q("abs(1/0)"), Err(ExprError::NonFinite { .. })));
        assert!(matches!(
            q("1e300 * 1e300"),
            Err(ExprError::NonFinite { .. })
        ));
    }

    #[test]
    fn tan_at_its_pole_is_an_error_not_a_huge_number() {
        let err = q("tan(90)").unwrap_err();
        assert!(
            matches!(
                &err,
                ExprError::FunctionDomain {
                    function: "tan",
                    ..
                }
            ),
            "{err:?}"
        );
        assert_eq!(err.offset(), Some(0));
        assert!(matches!(
            q("tan(270)"),
            Err(ExprError::FunctionDomain { .. })
        ));
        // Near but not at the pole still evaluates.
        assert!(q("tan(89.99999)").is_ok());
    }

    // -- dimensions --

    #[test]
    fn a_bare_number_stays_uncommitted_and_adopts_its_context() {
        let n = q("25").unwrap();
        assert_eq!(n.tag, Tag::Untagged);
        assert_eq!(n.as_length_meters().unwrap(), 0.025);
        assert_eq!(n.as_angle_degrees().unwrap(), 25.0);
        // Arithmetic among plain numbers stays plain.
        assert_eq!(q("2 + 3 * 4").unwrap().tag, Tag::Untagged);
        assert_eq!(q("sin(30)").unwrap().tag, Tag::Untagged);
    }

    #[test]
    fn a_unit_suffix_commits_the_dimension_at_its_own_offset() {
        let l = q("25mm").unwrap();
        assert_eq!(l.dimension(), Some(Dimension::Length));
        assert_eq!(l.tag.at(), Some(Span::new(0, 4)));
        let a = q("90deg").unwrap();
        assert_eq!(a.dimension(), Some(Dimension::Angle));
        assert_eq!(a.tag.at(), Some(Span::new(0, 5)));
    }

    #[test]
    fn a_committed_unit_taints_the_whole_expression() {
        for s in ["25mm + 1", "1 + 25mm", "25mm * 2", "2 * 25mm", "-25mm"] {
            assert_eq!(
                q(s).unwrap().dimension(),
                Some(Dimension::Length),
                "{s} must be a length"
            );
        }
        assert_eq!(q("90deg / 2").unwrap().dimension(), Some(Dimension::Angle));
    }

    #[test]
    fn mixing_a_length_and_an_angle_is_a_typed_error_naming_the_suffix() {
        let err = q("10mm + 1deg").unwrap_err();
        assert_eq!(
            err,
            ExprError::DimensionMismatch {
                expected: "a length".into(),
                found: "an angle".into(),
                span: Span::new(7, 11),
            },
            "the error points at `1deg`"
        );
        assert!(matches!(
            q("90deg - 1in"),
            Err(ExprError::DimensionMismatch { .. })
        ));
        assert!(matches!(
            q("min(1mm, 1deg)"),
            Err(ExprError::DimensionMismatch { .. })
        ));
    }

    #[test]
    fn multiplication_and_division_compose_exponents() {
        assert_eq!(q("10mm * 10mm").unwrap().dim(), Dim::LENGTH.scaled(2));
        assert_eq!(q("10mm * 10mm / 2mm").unwrap().dim(), Dim::LENGTH);
        assert_eq!(q("10mm / 2mm").unwrap().dim(), Dim::NONE);
        assert_eq!(
            q("1 / 2mm").unwrap().dim(),
            Dim {
                length: -1,
                angle: 0
            }
        );
        // An area cannot be a depth.
        assert!(matches!(
            q("10mm * 10mm").unwrap().as_length_meters(),
            Err(ExprError::DimensionMismatch { .. })
        ));
        // ...but its square root can.
        assert_eq!(
            q("sqrt(10mm * 10mm)").unwrap().as_length_meters().unwrap(),
            0.010
        );
    }

    #[test]
    fn powers_scale_exponents_and_need_a_plain_exponent() {
        assert_eq!(q("2mm ^ 3").unwrap().dim(), Dim::LENGTH.scaled(3));
        assert_eq!(q("2mm ^ 0").unwrap().dim(), Dim::NONE);
        assert!(matches!(
            q("2 ^ 3mm"),
            Err(ExprError::DimensionMismatch { .. })
        ));
        assert!(
            matches!(q("4mm ^ 0.5"), Err(ExprError::DimensionMismatch { .. })),
            "a fractional power of a length has no nameable dimension"
        );
        // A fractional power of a plain number is fine.
        close(v("4 ^ 0.5"), 2.0);
    }

    #[test]
    fn sqrt_of_an_odd_exponent_is_refused() {
        assert!(matches!(
            q("sqrt(4mm)"),
            Err(ExprError::DimensionMismatch { .. })
        ));
        assert_eq!(q("sqrt(4)").unwrap().tag, Tag::Untagged);
    }

    #[test]
    fn trig_takes_an_angle_or_a_plain_number_and_returns_a_plain_number() {
        close(q("sin(30deg)").unwrap().value, 0.5);
        assert_eq!(q("sin(30deg)").unwrap().tag, Tag::Untagged);
        assert!(matches!(
            q("sin(30mm)"),
            Err(ExprError::DimensionMismatch { .. })
        ));
        // The compatibility case: a plain number times a trig result is
        // still acceptable as a length.
        close(
            q("10 * sin(30)").unwrap().as_length_meters().unwrap(),
            0.005,
        );
    }

    #[test]
    fn a_tagged_variable_propagates_its_dimension() {
        let mut env = Env::new();
        env.insert(
            "turn".to_string(),
            Quantity::tagged(90.0, Dim::ANGLE, Span::new(0, 0)),
        );
        env.insert(
            "wall".to_string(),
            Quantity::tagged(3.0, Dim::LENGTH, Span::new(0, 0)),
        );
        let got = evaluate_quantity("turn / 2", &env).unwrap();
        assert_eq!(got.value, 45.0);
        assert_eq!(got.dimension(), Some(Dimension::Angle));
        assert!(matches!(
            evaluate_quantity("wall + turn", &env),
            Err(ExprError::DimensionMismatch { .. })
        ));
        assert!(matches!(
            evaluate_quantity("wall", &env).unwrap().as_angle_degrees(),
            Err(ExprError::DimensionMismatch { .. })
        ));
    }

    #[test]
    fn mixed_length_units_add_in_the_working_space() {
        // 10 mm + 1 in = 35.4 mm = 0.0354 m.
        let got = q("10mm + 1in").unwrap();
        close(got.value, 35.4);
        close(got.as_length_meters().unwrap(), 0.0354);
    }
}
