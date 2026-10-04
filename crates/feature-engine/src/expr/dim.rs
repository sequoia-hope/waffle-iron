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
//! | `1kg`, `250g`, `1lb` | mass (M1) | refuses |
//! | `2.7g / 1cm^3` | density — mass · length⁻³, composed, not a suffix | refuses |
//! | `2700 * 1kg / 1m^3 * (10mm*10mm*10mm)` | mass | refuses |
//! | `2700` in a `Density` field | — | **refused**: a density has no bare spelling ([`density_needs_units`]) |
//!
//! That last row is the ONE exception to "an uncommitted number adopts its
//! field's dimension", and it is there because a density is the one dimension
//! whose working space (kg/mm³) is not a unit anyone authors in. See
//! [`density_needs_units`] for the measurement that motivated it.
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

use super::{ExprError, Span, MM_TO_METERS};

/// Degrees per radian — the `rad` suffix's factor into degree working space.
const DEG_PER_RAD: f64 = 180.0 / std::f64::consts::PI;

// `Dimension` and `Dim` live in `waffle_types::dimension` since M1 — the
// tolerance types in `waffle_types::annotation::tolerance` need the same
// vocabulary, and that crate is below this one. Re-exported here so every
// pre-M1 path (`feature_engine::expr::Dimension`, `expr::dim::Dim`) still
// names the one definition. See that module's docs for why the split falls
// where it does, and for the mass axis M1 added.
pub use waffle_types::dimension::{Dim, Dimension};

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
            Tag::Tagged { dim, .. } if dim == Dim::MASS => Some(Dimension::Mass),
            Tag::Tagged { dim, .. } if dim == Dim::DENSITY => Some(Dimension::Density),
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
        } else if dim == Dim::MASS {
            "mass".to_string()
        } else if dim == Dim::DENSITY {
            "density".to_string()
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

    /// Accept as a MASS, returning KILOGRAMS (M1).
    ///
    /// Working space and model unit coincide for a mass — there is no
    /// mm-scale convention to preserve, and kg is what a density is quoted
    /// against — so this is the identity rather than a conversion.
    pub fn as_mass_kilograms(self) -> Result<f64, ExprError> {
        self.check(Dimension::Mass)?;
        Ok(self.value)
    }

    /// Accept as a DENSITY, returning kg/m³ (M1).
    ///
    /// The working space is kg per CUBIC MILLIMETRE, because the mass axis
    /// is in kg and the length axis in mm, so the conversion is `1e9` —
    /// (1000 mm/m)³. That is why `2.7g / 1cm^3` comes back as 2700 and not
    /// as 2.7e-6.
    pub fn as_density_kg_m3(self) -> Result<f64, ExprError> {
        self.check(Dimension::Density)?;
        Ok(self.value / (MM_TO_METERS * MM_TO_METERS * MM_TO_METERS))
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
            Dimension::Mass => self.as_mass_kilograms(),
            Dimension::Density => self.as_density_kg_m3(),
        }
    }

    /// Accept for `want` and COMMIT to it, keeping the working-space
    /// magnitude. This is what a declared unit does to a parameter: the
    /// value does not move, but from here on it carries the dimension, so a
    /// field of another kind that reads it is refused. `at` is the
    /// commitment site to blame when the value had none of its own.
    pub fn retag(self, want: Dimension, at: Span) -> Result<Quantity, ExprError> {
        // Same refusal as `check`, raised here first only so the blame span
        // is the declaration's rather than empty.
        if want == Dimension::Density && matches!(self.tag, Tag::Untagged) {
            return Err(density_needs_units(at));
        }
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
    /// fits anything — except a [`Dimension::Density`], which has no bare
    /// spelling (see [`density_needs_units`]) — and a committed one must
    /// match exactly.
    pub fn check(self, want: Dimension) -> Result<(), ExprError> {
        match self.tag {
            // M1 review: refused BEFORE the general untagged-adopts rule,
            // because a bare number declared a density is the one case where
            // adopting produces a silently wrong magnitude rather than the
            // pre-P1 meaning.
            Tag::Untagged if want == Dimension::Density => {
                Err(density_needs_units(Span::default()))
            }
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

/// Why an UNCOMMITTED number cannot be a density (M1 review).
///
/// Every other working space in this evaluator is a unit a person authors in,
/// so a bare literal adopting the field's dimension means what they wrote:
/// `25` in a length field is 25 **mm**, in an angle field 25 **degrees**, in a
/// mass field 2.5 **kg**, in a count or ratio field the plain number. A
/// DENSITY's working space is **kg/mm³** — the mass axis is kg and the length
/// axis mm, which is what makes `density × volume` a mass by the arithmetic —
/// and nobody writes a density in kg/mm³. The material table quotes
/// `density_kg_m3`, the MCP `material_set` tool takes `density_kg_m3`, and the
/// `Dimension::Density` schema itself says "model unit kg/m³".
///
/// So a bare `2700` declared a density would commit to 2700 kg/mm³, which
/// [`Quantity::as_density_kg_m3`] reports as **2.7e12 kg/m³** — a silent
/// factor of 10⁹, with the right variable name and the right printed unit
/// beside it and nothing to say it is wrong. Measured before this refusal
/// landed: a `DesignParameter` with `unit: Density` and expression `"2700"`
/// evaluated with NO error and read back as `2.7e12`.
///
/// There is nothing to lose by refusing it, because there is no density
/// SUFFIX to write instead — the grammar composes one, and `2700kg / 1m^3`
/// and `2.7g / 1cm^3` both already come back as 2700. A `Mass` is NOT carved
/// out for the same reason it does not need to be: kg is its working space and
/// its model unit at once, so a bare `2.5` there means the 2.5 kg it looks
/// like.
fn density_needs_units(at: Span) -> ExprError {
    ExprError::DimensionMismatch {
        expected: "a density composed from its units (`2700kg / 1m^3`, `2.7g / 1cm^3`)".to_string(),
        found: "a plain number, which has no density spelling — the working space is kg/mm³, so \
                a bare 2700 would be 2.7e12 kg/m³"
            .to_string(),
        span: at,
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
///
/// The mass suffixes arrived with M1, and `kg` is the identity for the same
/// reason `mm` is for a length: the working space is the unit the quantity is
/// quoted in everywhere else (a material's density is kg/m³, so its mass
/// numerator is kg). There is no density SUFFIX, and none is needed — the
/// grammar composes one out of the two axes, so `2.7g / 1cm^3` IS a density
/// by [`Dim::DENSITY`] and `as_density_kg_m3` converts it. A single
/// `kg/m^3` token would have to be lexed as one identifier containing a
/// slash and a caret, which the lexer cannot do and should not learn to.
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
    Unit {
        name: "kg",
        factor: 1.0,
        dim: Dim::MASS,
    },
    Unit {
        name: "g",
        factor: 1e-3,
        dim: Dim::MASS,
    },
    Unit {
        name: "lb",
        factor: 0.453_592_37,
        dim: Dim::MASS,
    },
    Unit {
        name: "oz",
        factor: 0.028_349_523_125,
        dim: Dim::MASS,
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
                angle: 0,
                mass: 0
            })
        );
        assert_eq!(l.compose(l, -1), Some(Dim::NONE));
        assert_eq!(
            Dim::NONE.compose(l, -1),
            Some(Dim {
                length: -1,
                angle: 0,
                mass: 0
            })
        );
        assert_eq!(
            l.scaled(3),
            Some(Dim {
                length: 3,
                angle: 0,
                mass: 0
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

    #[test]
    fn every_mass_suffix_converts_into_kilogram_working_space() {
        for (name, kg) in [
            ("kg", 1.0),
            ("g", 1e-3),
            ("lb", 0.453_592_37),
            ("oz", 0.028_349_523_125),
        ] {
            let u = unit_by_name(name).unwrap_or_else(|| panic!("no unit {name}"));
            assert_eq!(u.dim, Dim::MASS, "{name}");
            assert_eq!(u.factor, kg, "{name}");
        }
        // 16 oz is a pound, to the bit — both factors are exact decimals of
        // the international avoirdupois definition, not rounded ones.
        let oz = unit_by_name("oz").unwrap().factor;
        let lb = unit_by_name("lb").unwrap().factor;
        assert!((16.0 * oz - lb).abs() < 1e-18, "{} vs {lb}", 16.0 * oz);
    }

    #[test]
    fn a_mass_boundary_is_kilograms_and_refuses_a_length() {
        let kg = Quantity::tagged(2.5, Dim::MASS, at());
        assert_eq!(kg.as_mass_kilograms().unwrap(), 2.5);
        assert_eq!(kg.accept(Dimension::Mass).unwrap(), 2.5);
        assert!(matches!(
            kg.as_length_meters(),
            Err(ExprError::DimensionMismatch { .. })
        ));
        // ...and a length is refused as a mass, by name.
        let mm = Quantity::tagged(2.5, Dim::LENGTH, at());
        let Err(ExprError::DimensionMismatch {
            expected, found, ..
        }) = mm.as_mass_kilograms()
        else {
            panic!("a length was accepted as a mass");
        };
        assert_eq!(expected, "a mass");
        assert_eq!(found, "a length");
    }

    /// The one carve-out in "an uncommitted number adopts anything", pinned
    /// next to the rule it excepts. See [`density_needs_units`] for why.
    #[test]
    fn an_uncommitted_number_adopts_every_dimension_except_a_density() {
        let n = Quantity::untagged(2700.0);
        for want in [
            Dimension::Length,
            Dimension::Angle,
            Dimension::Ratio,
            Dimension::Mass,
        ] {
            assert!(n.check(want).is_ok(), "{want:?}");
            assert!(n.retag(want, at()).is_ok(), "{want:?}");
        }
        // A density is refused by name, naming the spelling that works —
        // not accepted as 2700 kg/mm³ (2.7e12 kg/m³).
        let Err(ExprError::DimensionMismatch {
            expected,
            found,
            span,
        }) = n.retag(Dimension::Density, at())
        else {
            panic!("a bare 2700 was accepted as a density");
        };
        assert!(expected.contains("2700kg / 1m^3"), "{expected}");
        assert!(found.contains("kg/mm"), "{found}");
        assert_eq!(span, at(), "the declaration is the blame site");
        assert!(n.check(Dimension::Density).is_err());
        assert!(n.as_density_kg_m3().is_err());
        assert!(n.accept(Dimension::Density).is_err());
        // A COMMITTED density is unaffected: the refusal is about the
        // missing units, not about the dimension.
        let rho = Quantity::tagged(2.7e-6, Dim::DENSITY, at());
        assert!(rho.check(Dimension::Density).is_ok());
        assert!(rho.retag(Dimension::Density, at()).is_ok());
    }

    #[test]
    fn a_density_boundary_converts_out_of_kg_per_cubic_millimetre() {
        // The working space is kg/mm³, so aluminium's 2700 kg/m³ is
        // 2.7e-6 there — the 1e9 is what makes the number legible again.
        let d = Quantity::tagged(2.7e-6, Dim::DENSITY, at());
        let kg_m3 = d.as_density_kg_m3().unwrap();
        assert!((kg_m3 - 2700.0).abs() < 1e-9, "{kg_m3}");
        assert_eq!(d.dimension(), Some(Dimension::Density));
        assert_eq!(d.dimension_label(), "density");
        assert!(matches!(
            d.as_mass_kilograms(),
            Err(ExprError::DimensionMismatch { .. })
        ));
    }
}
