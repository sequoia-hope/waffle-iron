//! Dimensions, unit literals, and the dimensioned value the evaluator
//! produces.
//!
//! A [`Quantity`] is a working-space magnitude (mm for a length, degrees for
//! an angle — see the module docs of [`super`]) plus a [`Tag`] saying
//! whether its dimension has been *committed* by a unit suffix. Nothing
//! leaves the working space until a typed boundary (`as_length_meters` and
//! friends) accepts it for a specific [`Dimension`]; that acceptance is
//! where a wrong unit is refused instead of coerced.
//!
//! ## The rules, in full
//!
//! [`Tag::Untagged`] unifying with anything is the backward-compatibility
//! hinge, so what it does and does NOT do is stated here rather than left
//! to be inferred. Every rule below is pinned by a test in
//! `super::eval::tests`.
//!
//! | Expression | Dimension | A `Length` field |
//! |---|---|---|
//! | `25` | uncommitted | accepts, 25 mm — the pre-P1 meaning |
//! | `25mm`, `1in`, `10mm + 1in` | length | accepts |
//! | `25deg`, `1rad` | angle | refuses |
//! | `25 * 1deg` | angle — one committed operand taints the product | refuses |
//! | `10mm * 10mm` | length² | refuses, "got length^2" |
//! | `sqrt(10mm * 10mm)` | length | accepts |
//! | `sqrt(4mm)` | — | refused IN the expression: an odd exponent halves to no nameable dimension |
//! | `25deg / 1deg` | ratio — **committed** dimensionless | **refuses** |
//! | `10mm % 3mm` | length — `%` unifies like `+` | accepts |
//! | `10 % 3mm` | length — the uncommitted operand adopts | accepts |
//! | `10mm % 1deg` | — | refused in the expression |
//! | `2mm ^ 3` | length³ | refuses |
//! | `2mm ^ 0` | ratio | refuses (see `25deg / 1deg`) |
//! | `4mm ^ 0.5` | — | refused: a fractional power of a length names no dimension |
//! | `2 ^ 3mm` | — | refused: an exponent carries no unit |
//! | `min(1mm, 1deg)` | — | refused: `min`/`max` unify like `+` |
//! | `sin(30)`, `sin(30deg)` | uncommitted | accepts (trig returns a plain number) |
//! | `sin(30mm)` | — | refused: trig takes an angle |
//!
//! The one that is a genuine choice is `25deg / 1deg`, and the choice is to
//! REFUSE it as a length. A value that never committed to a dimension
//! (`25`) adopts the field's; a value that committed to being
//! *dimensionless* has said what it is, and a bare number of millimetres is
//! not what it said. Writing `25deg / 1deg` where a depth is wanted is
//! almost always a units slip; `25` or `25mm` says the intended thing.
//! `Count` and `Ratio` are the same exponent vector and differ only in what
//! the boundary demands of the magnitude, so this rule covers both.
//!
//! There are no comparison or boolean operators in the grammar, so there is
//! no comparison rule to state.

use serde::{Deserialize, Serialize};

use super::{ExprError, Span, MM_TO_METERS};

/// Degrees per radian — the `rad` suffix's factor into degree working space.
const DEG_PER_RAD: f64 = 180.0 / std::f64::consts::PI;

/// The kind of quantity a field that consumes an expression asks for.
///
/// This is the tag an expression-driven field carries (`specs/
/// agent_mechanical_design.md` §6 P1): a depth or radius is a
/// `Length`, a revolve or pattern angle an `Angle`, an instance count a
/// `Count`, a scale factor a `Ratio`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum Dimension {
    /// A distance. Working space mm; model unit metres.
    #[serde(alias = "length")]
    Length,
    /// A rotation. Working space degrees; also the unit every angle field
    /// on the feature tree stores.
    #[serde(alias = "angle")]
    Angle,
    /// A whole non-negative number of things (instances, teeth).
    #[serde(alias = "count")]
    Count,
    /// A dimensionless number (a factor, a fraction).
    #[serde(alias = "ratio")]
    Ratio,
}

impl Dimension {
    /// The exponent vector a value must carry to be accepted here.
    pub fn dim(self) -> Dim {
        match self {
            Dimension::Length => Dim::LENGTH,
            Dimension::Angle => Dim::ANGLE,
            Dimension::Count | Dimension::Ratio => Dim::NONE,
        }
    }

    /// Name for a diagnostic ("expected a length, got an angle").
    pub fn label(self) -> &'static str {
        match self {
            Dimension::Length => "a length",
            Dimension::Angle => "an angle",
            Dimension::Count => "a count (a plain number)",
            Dimension::Ratio => "a ratio (a plain number)",
        }
    }
}

/// A dimension as exponents: `length^length · angle^angle`.
///
/// Exponents, not a closed enum, because arithmetic composes them: `w * h`
/// is a length², `w * h / t` a length again. A field accepts exactly the
/// exponent vector it asked for, so a length² never lands in a depth.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct Dim {
    pub length: i8,
    pub angle: i8,
}

impl Dim {
    /// Dimensionless.
    pub const NONE: Dim = Dim {
        length: 0,
        angle: 0,
    };
    /// A distance.
    pub const LENGTH: Dim = Dim {
        length: 1,
        angle: 0,
    };
    /// A rotation.
    pub const ANGLE: Dim = Dim {
        length: 0,
        angle: 1,
    };

    /// Compose: `self · other^sign`, or `None` when an exponent leaves the
    /// representable range.
    ///
    /// `None` rather than a saturating clamp, because a clamped exponent is
    /// a *wrong* dimension that can still be accepted: clamp `length^128`
    /// and `length^127` and their quotient reports `ratio` where the truth
    /// is `length`. Overflow is loud (P10) instead.
    pub fn compose(self, other: Dim, sign: i8) -> Option<Dim> {
        let axis = |a: i8, b: i8| -> Option<i8> { a.checked_add(b.checked_mul(sign)?) };
        Some(Dim {
            length: axis(self.length, other.length)?,
            angle: axis(self.angle, other.angle)?,
        })
    }

    /// `self^n`, or `None` on exponent overflow (see [`Dim::compose`]).
    pub fn scaled(self, n: i32) -> Option<Dim> {
        let scale = |e: i8| -> Option<i8> { i8::try_from((e as i32).checked_mul(n)?).ok() };
        Some(Dim {
            length: scale(self.length)?,
            angle: scale(self.angle)?,
        })
    }

    /// Halve every exponent, or `None` when one of them is odd (`sqrt` of a
    /// length is not a dimension this system can name).
    pub fn halved(self) -> Option<Dim> {
        if self.length % 2 != 0 || self.angle % 2 != 0 {
            return None;
        }
        Some(Dim {
            length: self.length / 2,
            angle: self.angle / 2,
        })
    }

    /// Name for a diagnostic.
    pub fn label(self) -> String {
        match (self.length, self.angle) {
            (0, 0) => "a plain number".to_string(),
            (1, 0) => "a length".to_string(),
            (0, 1) => "an angle".to_string(),
            (l, a) => {
                let mut parts = Vec::new();
                if l != 0 {
                    parts.push(format!("length^{l}"));
                }
                if a != 0 {
                    parts.push(format!("angle^{a}"));
                }
                parts.join("·")
            }
        }
    }
}

/// Whether a value's dimension has been committed.
#[derive(Debug, Clone, Copy)]
pub enum Tag {
    /// A plain number. Adopts whatever dimension its context asks for —
    /// this is what keeps a bare `25` meaning 25 mm in a length field and
    /// 25° in an angle field, exactly as it did before P1.
    Untagged,
    /// Committed by a unit suffix (or a parameter's declared unit). `at` is
    /// the byte range of the thing that committed it, so a mismatch can
    /// point at the `deg` that does not belong.
    Tagged { dim: Dim, at: Span },
}

// Equality is about the DIMENSION, never about where it was committed.
impl PartialEq for Tag {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Tag::Untagged, Tag::Untagged) => true,
            (Tag::Tagged { dim: a, .. }, Tag::Tagged { dim: b, .. }) => a == b,
            _ => false,
        }
    }
}

impl Eq for Tag {}

impl Tag {
    /// The exponent vector, treating an uncommitted value as dimensionless.
    pub fn dim(self) -> Dim {
        match self {
            Tag::Untagged => Dim::NONE,
            Tag::Tagged { dim, .. } => dim,
        }
    }

    /// Where the dimension was committed, if it was.
    pub fn at(self) -> Option<Span> {
        match self {
            Tag::Untagged => None,
            Tag::Tagged { at, .. } => Some(at),
        }
    }

    pub fn label(self) -> String {
        match self {
            Tag::Untagged => "a plain number".to_string(),
            Tag::Tagged { dim, .. } => dim.label(),
        }
    }

    /// Commit `dim` at `at` — unless already committed, in which case the
    /// first commitment site is kept (it is the one a diagnostic should
    /// name).
    pub fn committed(dim: Dim, at: Span) -> Tag {
        Tag::Tagged { dim, at }
    }
}

/// An evaluated expression: a working-space magnitude plus its dimension.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quantity {
    /// Millimetres for a length, degrees for an angle, the plain number
    /// otherwise. Always finite — the evaluator refuses a non-finite
    /// intermediate where it occurs.
    pub value: f64,
    pub tag: Tag,
}

impl Quantity {
    /// A plain number that adopts its context's dimension.
    pub fn untagged(value: f64) -> Self {
        Self {
            value,
            tag: Tag::Untagged,
        }
    }

    /// A value whose dimension is committed.
    pub fn tagged(value: f64, dim: Dim, at: Span) -> Self {
        Self {
            value,
            tag: Tag::committed(dim, at),
        }
    }

    pub fn dim(&self) -> Dim {
        self.tag.dim()
    }

    /// The named [`Dimension`] this quantity reports, when it has one.
    /// `None` for an uncommitted number (it has no dimension of its own)
    /// and for a composite like a length² that no field asks for.
    pub fn dimension(&self) -> Option<Dimension> {
        match self.tag {
            Tag::Untagged => None,
            Tag::Tagged { dim, .. } if dim == Dim::LENGTH => Some(Dimension::Length),
            Tag::Tagged { dim, .. } if dim == Dim::ANGLE => Some(Dimension::Angle),
            Tag::Tagged { dim, .. } if dim == Dim::NONE => Some(Dimension::Ratio),
            Tag::Tagged { .. } => None,
        }
    }

    /// A short name for the dimension, for a report an agent reads:
    /// `"length"`, `"angle"`, `"unitless"` (uncommitted), `"ratio"`
    /// (committed dimensionless), or the composite's exponents.
    pub fn dimension_label(&self) -> String {
        let Tag::Tagged { dim, .. } = self.tag else {
            return "unitless".to_string();
        };
        if dim == Dim::LENGTH {
            "length".to_string()
        } else if dim == Dim::ANGLE {
            "angle".to_string()
        } else if dim == Dim::NONE {
            "ratio".to_string()
        } else {
            dim.label()
        }
    }

    /// Accept as a LENGTH, returning the value in model METRES.
    pub fn as_length_meters(self) -> Result<f64, ExprError> {
        self.check(Dimension::Length)?;
        Ok(self.value * MM_TO_METERS)
    }

    /// Accept as an ANGLE, returning DEGREES — the unit every angle field on
    /// the feature tree stores.
    pub fn as_angle_degrees(self) -> Result<f64, ExprError> {
        self.check(Dimension::Angle)?;
        Ok(self.value)
    }

    /// Accept as an ANGLE, returning RADIANS (the model's base angular
    /// unit), for a consumer that works in radians.
    pub fn as_angle_radians(self) -> Result<f64, ExprError> {
        self.check(Dimension::Angle)?;
        Ok(self.value.to_radians())
    }

    /// Accept as a COUNT: dimensionless, whole and non-negative.
    pub fn as_count(self) -> Result<f64, ExprError> {
        self.check(Dimension::Count)?;
        if self.value < 0.0 || self.value.fract() != 0.0 {
            return Err(ExprError::NotACount { value: self.value });
        }
        Ok(self.value)
    }

    /// Accept as a RATIO: dimensionless.
    pub fn as_ratio(self) -> Result<f64, ExprError> {
        self.check(Dimension::Ratio)?;
        Ok(self.value)
    }

    /// Accept for `want`, returning the value in that field's own unit:
    /// metres for a length, DEGREES for an angle (the stored convention),
    /// the plain number for a count or ratio.
    pub fn accept(self, want: Dimension) -> Result<f64, ExprError> {
        match want {
            Dimension::Length => self.as_length_meters(),
            Dimension::Angle => self.as_angle_degrees(),
            Dimension::Count => self.as_count(),
            Dimension::Ratio => self.as_ratio(),
        }
    }

    /// Accept for `want` and COMMIT to it, keeping the working-space
    /// magnitude. This is what a declared unit does to a parameter: the
    /// value does not move, but from here on it carries the dimension, so a
    /// field of another kind that reads it is refused. `at` is the
    /// commitment site to blame when the value had none of its own.
    pub fn retag(self, want: Dimension, at: Span) -> Result<Quantity, ExprError> {
        self.check(want)?;
        if want == Dimension::Count && (self.value < 0.0 || self.value.fract() != 0.0) {
            return Err(ExprError::NotACount { value: self.value });
        }
        Ok(Quantity {
            value: self.value,
            tag: Tag::committed(want.dim(), self.tag.at().unwrap_or(at)),
        })
    }

    /// Does this quantity's dimension fit `want`? An uncommitted number
    /// fits anything; a committed one must match exactly.
    pub fn check(self, want: Dimension) -> Result<(), ExprError> {
        match self.tag {
            Tag::Untagged => Ok(()),
            Tag::Tagged { dim, .. } if dim == want.dim() => Ok(()),
            Tag::Tagged { dim, at } => Err(ExprError::DimensionMismatch {
                expected: want.label().to_string(),
                found: dim.label(),
                span: at,
            }),
        }
    }
}

/// A unit suffix: its name, its multiplier into the working space, and the
/// dimension it commits.
#[derive(Debug, Clone, PartialEq)]
pub struct Unit {
    pub name: &'static str,
    /// Multiplier into the working space (mm for a length, degrees for an
    /// angle).
    pub factor: f64,
    pub dim: Dim,
}

/// Every accepted unit suffix. `deg` is an identity factor so an angle
/// literal can be explicit; `rad` converts into degree working space.
pub const UNITS: &[Unit] = &[
    Unit {
        name: "mm",
        factor: 1.0,
        dim: Dim::LENGTH,
    },
    Unit {
        name: "cm",
        factor: 10.0,
        dim: Dim::LENGTH,
    },
    Unit {
        name: "m",
        factor: 1000.0,
        dim: Dim::LENGTH,
    },
    Unit {
        name: "in",
        factor: 25.4,
        dim: Dim::LENGTH,
    },
    Unit {
        name: "ft",
        factor: 304.8,
        dim: Dim::LENGTH,
    },
    Unit {
        name: "deg",
        factor: 1.0,
        dim: Dim::ANGLE,
    },
    Unit {
        name: "rad",
        factor: DEG_PER_RAD,
        dim: Dim::ANGLE,
    },
];

pub fn unit_by_name(name: &str) -> Option<&'static Unit> {
    UNITS.iter().find(|u| u.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at() -> Span {
        Span::new(0, 2)
    }

    #[test]
    fn exponents_compose_multiply_and_divide() {
        let l = Dim::LENGTH;
        assert_eq!(
            l.compose(l, 1),
            Some(Dim {
                length: 2,
                angle: 0
            })
        );
        assert_eq!(l.compose(l, -1), Some(Dim::NONE));
        assert_eq!(
            Dim::NONE.compose(l, -1),
            Some(Dim {
                length: -1,
                angle: 0
            })
        );
        assert_eq!(
            l.scaled(3),
            Some(Dim {
                length: 3,
                angle: 0
            })
        );
        assert_eq!(l.scaled(0), Some(Dim::NONE));
    }

    #[test]
    fn exponent_overflow_is_none_not_a_clamp() {
        // A clamp would be a WRONG dimension a field could still accept:
        // clamp length^128 and length^127 and their quotient reports
        // `ratio` where the truth is `length`.
        assert_eq!(Dim::LENGTH.scaled(10_000), None);
        assert_eq!(Dim::LENGTH.scaled(128), None);
        assert_eq!(Dim::LENGTH.scaled(127).unwrap().length, 127);
        let big = Dim::LENGTH.scaled(100).unwrap();
        assert_eq!(big.compose(big, 1), None);
        assert_eq!(big.compose(big, -1), Some(Dim::NONE));
    }

    #[test]
    fn sqrt_halves_only_even_exponents() {
        assert_eq!(Dim::LENGTH.scaled(2).unwrap().halved(), Some(Dim::LENGTH));
        assert_eq!(Dim::LENGTH.halved(), None);
        assert_eq!(Dim::NONE.halved(), Some(Dim::NONE));
    }

    #[test]
    fn labels_name_the_dimension() {
        assert_eq!(Dim::NONE.label(), "a plain number");
        assert_eq!(Dim::LENGTH.label(), "a length");
        assert_eq!(Dim::ANGLE.label(), "an angle");
        assert_eq!(Dim::LENGTH.scaled(2).unwrap().label(), "length^2");
        assert_eq!(
            Dim::LENGTH.compose(Dim::ANGLE, -1).unwrap().label(),
            "length^1·angle^-1"
        );
    }

    #[test]
    fn tag_equality_ignores_the_commitment_site() {
        let a = Tag::committed(Dim::LENGTH, Span::new(0, 2));
        let b = Tag::committed(Dim::LENGTH, Span::new(9, 11));
        assert_eq!(a, b);
        assert_ne!(a, Tag::committed(Dim::ANGLE, Span::new(0, 2)));
        assert_ne!(a, Tag::Untagged);
    }

    #[test]
    fn untagged_adopts_every_dimension() {
        let q = Quantity::untagged(25.0);
        assert_eq!(q.as_length_meters().unwrap(), 0.025);
        assert_eq!(q.as_angle_degrees().unwrap(), 25.0);
        assert_eq!(q.as_count().unwrap(), 25.0);
        assert_eq!(q.as_ratio().unwrap(), 25.0);
        assert_eq!(q.dimension(), None);
        assert_eq!(q.dimension_label(), "unitless");
    }

    #[test]
    fn a_committed_length_is_refused_as_an_angle_and_vice_versa() {
        let len = Quantity::tagged(25.0, Dim::LENGTH, at());
        assert_eq!(len.as_length_meters().unwrap(), 0.025);
        let err = len.as_angle_degrees().unwrap_err();
        assert_eq!(
            err,
            ExprError::DimensionMismatch {
                expected: "an angle".into(),
                found: "a length".into(),
                span: at(),
            }
        );

        let ang = Quantity::tagged(90.0, Dim::ANGLE, at());
        assert_eq!(ang.as_angle_degrees().unwrap(), 90.0);
        assert!((ang.as_angle_radians().unwrap() - std::f64::consts::FRAC_PI_2).abs() < 1e-15);
        assert!(matches!(
            ang.as_length_meters(),
            Err(ExprError::DimensionMismatch { .. })
        ));
    }

    #[test]
    fn a_count_must_be_whole_and_non_negative() {
        let at = at();
        assert_eq!(Quantity::untagged(6.0).as_count().unwrap(), 6.0);
        assert_eq!(Quantity::untagged(0.0).as_count().unwrap(), 0.0);
        assert_eq!(
            Quantity::untagged(2.5).as_count(),
            Err(ExprError::NotACount { value: 2.5 })
        );
        assert_eq!(
            Quantity::untagged(-1.0).as_count(),
            Err(ExprError::NotACount { value: -1.0 })
        );
        // A length is refused before the whole-number test runs.
        assert!(matches!(
            Quantity::tagged(6.0, Dim::LENGTH, at).as_count(),
            Err(ExprError::DimensionMismatch { .. })
        ));
    }

    #[test]
    fn accept_routes_to_the_fields_own_unit() {
        let at = at();
        assert_eq!(
            Quantity::tagged(25.0, Dim::LENGTH, at)
                .accept(Dimension::Length)
                .unwrap(),
            0.025
        );
        assert_eq!(
            Quantity::tagged(90.0, Dim::ANGLE, at)
                .accept(Dimension::Angle)
                .unwrap(),
            90.0,
            "angle fields store degrees"
        );
    }

    #[test]
    fn composite_dimensions_have_no_named_dimension() {
        let area = Quantity::tagged(100.0, Dim::LENGTH.scaled(2).unwrap(), at());
        assert_eq!(area.dimension(), None);
        assert_eq!(area.dimension_label(), "length^2");
        assert!(matches!(
            area.as_length_meters(),
            Err(ExprError::DimensionMismatch { .. })
        ));
    }

    #[test]
    fn a_committed_dimensionless_value_reports_ratio() {
        let r = Quantity::tagged(2.0, Dim::NONE, at());
        assert_eq!(r.dimension(), Some(Dimension::Ratio));
        assert_eq!(r.dimension_label(), "ratio");
        assert_eq!(r.as_ratio().unwrap(), 2.0);
        assert!(matches!(
            r.as_length_meters(),
            Err(ExprError::DimensionMismatch { .. })
        ));
    }

    #[test]
    fn retag_commits_without_moving_the_magnitude() {
        let whole = Span::new(0, 4);
        let q = Quantity::untagged(25.0)
            .retag(Dimension::Length, whole)
            .unwrap();
        assert_eq!(q.value, 25.0, "the working-space magnitude does not move");
        assert_eq!(q.dimension(), Some(Dimension::Length));
        assert_eq!(q.tag.at(), Some(whole));
        assert_eq!(q.as_length_meters().unwrap(), 0.025);
        assert!(matches!(
            q.as_angle_degrees(),
            Err(ExprError::DimensionMismatch { .. })
        ));

        // An already-committed value keeps its own blame site.
        let own = Span::new(7, 9);
        let q = Quantity::tagged(90.0, Dim::ANGLE, own)
            .retag(Dimension::Angle, whole)
            .unwrap();
        assert_eq!(q.tag.at(), Some(own));

        // A declared unit the expression contradicts is refused.
        assert!(matches!(
            Quantity::tagged(90.0, Dim::ANGLE, own).retag(Dimension::Length, whole),
            Err(ExprError::DimensionMismatch { .. })
        ));
        assert_eq!(
            Quantity::untagged(2.5).retag(Dimension::Count, whole),
            Err(ExprError::NotACount { value: 2.5 })
        );
    }

    #[test]
    fn units_scale_into_the_working_space() {
        assert_eq!(unit_by_name("mm").unwrap().factor, 1.0);
        assert_eq!(unit_by_name("m").unwrap().dim, Dim::LENGTH);
        assert_eq!(unit_by_name("deg").unwrap().dim, Dim::ANGLE);
        assert_eq!(unit_by_name("rad").unwrap().dim, Dim::ANGLE);
        assert!(unit_by_name("fathom").is_none());
    }

    #[test]
    fn dimension_serde_accepts_both_spellings() {
        assert_eq!(
            serde_json::to_string(&Dimension::Length).unwrap(),
            "\"Length\""
        );
        assert_eq!(
            serde_json::from_str::<Dimension>("\"length\"").unwrap(),
            Dimension::Length
        );
        assert_eq!(
            serde_json::from_str::<Dimension>("\"Count\"").unwrap(),
            Dimension::Count
        );
    }
}
