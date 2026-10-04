//! What KIND of quantity a number is — the dimension algebra.
//!
//! [`Dimension`] is the named kind a field that consumes a number asks for (a
//! depth is a `Length`, a revolve angle an `Angle`, a density a `Density`);
//! [`Dim`] is the exponent vector that arithmetic composes, so `w * h` is a
//! length² and `area(f) / w` a length again.
//!
//! ## Why this lives here and not in the expression evaluator
//!
//! It arrived with P1 (`specs/agent_mechanical_design.md` §6) inside
//! `feature_engine::expr::dim`, which is where the *evaluator* lives — and
//! while the evaluator was the only consumer, that was the right place. M1
//! gives it a second one two crates down: a tolerance on a drawing dimension
//! (`crate::annotation::tolerance`) is a number whose kind must be stated, or
//! it is the bare float in unknown units that the whole of P1 exists to
//! refuse. `waffle-types` is below `feature-engine`, so the shared half moved
//! HERE and `feature_engine::expr::dim` re-exports it: one definition, one
//! serialized spelling, one set of rules.
//!
//! What did NOT move is everything about *evaluating* an expression —
//! `Quantity`, `Tag`, the unit-suffix table and the typed `as_*` boundaries
//! stay in `feature_engine::expr::dim`, because they are about a parse tree
//! and its working space, which this crate knows nothing about.
//!
//! ## The mass axis (M1)
//!
//! P1 shipped two exponents, length and angle, and recorded the consequence:
//! `mass(body)` parsed and refused by name because "a mass has no dimension
//! this evaluator can name". M1 gives it one. `Dim` is three exponents now,
//! which makes `Density` (mass · length⁻³) expressible as the composition it
//! actually is, so `density × volume` IS a mass by the arithmetic rather than
//! by a special case.
//!
//! Adding a variant to [`Dimension`] is a wire-breaking change: it is
//! serialized as `DesignParameter.unit`, and a reader that has never heard of
//! `"Mass"` fails the whole document with an unknown-variant error. That is
//! why format v14 moves the reader floor (`docs/FILE_FORMAT.md` §4).

use serde::{Deserialize, Serialize};

/// The kind of quantity a field that consumes a number asks for.
///
/// This is the tag an expression-driven field carries (`specs/
/// agent_mechanical_design.md` §6 P1): a depth or radius is a
/// `Length`, a revolve or pattern angle an `Angle`, an instance count a
/// `Count`, a scale factor a `Ratio`, a material density a `Density` (M1).
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
    /// A mass (M1). Working space and model unit both KILOGRAMS — unlike a
    /// length, there is no mm-scale convention to preserve here, and kg is
    /// the unit the material table's density is quoted against.
    #[serde(alias = "mass")]
    Mass,
    /// A mass per unit volume (M1), model unit kg/m³. What a material entry
    /// declares; `Density × Length³` is a `Mass` by the exponents.
    #[serde(alias = "density")]
    Density,
}

impl Dimension {
    /// The exponent vector a value must carry to be accepted here.
    pub fn dim(self) -> Dim {
        match self {
            Dimension::Length => Dim::LENGTH,
            Dimension::Angle => Dim::ANGLE,
            Dimension::Count | Dimension::Ratio => Dim::NONE,
            Dimension::Mass => Dim::MASS,
            Dimension::Density => Dim::DENSITY,
        }
    }

    /// Name for a diagnostic ("expected a length, got an angle").
    pub fn label(self) -> &'static str {
        match self {
            Dimension::Length => "a length",
            Dimension::Angle => "an angle",
            Dimension::Count => "a count (a plain number)",
            Dimension::Ratio => "a ratio (a plain number)",
            Dimension::Mass => "a mass",
            Dimension::Density => "a density",
        }
    }
}

/// A dimension as exponents: `length^length · angle^angle · mass^mass`.
///
/// Exponents, not a closed enum, because arithmetic composes them: `w * h`
/// is a length², `w * h / t` a length again. A field accepts exactly the
/// exponent vector it asked for, so a length² never lands in a depth.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct Dim {
    pub length: i8,
    pub angle: i8,
    /// The mass exponent (M1). `0` for every pre-M1 dimension, which is why
    /// `Default` still means dimensionless.
    pub mass: i8,
}

impl Dim {
    /// Dimensionless.
    pub const NONE: Dim = Dim {
        length: 0,
        angle: 0,
        mass: 0,
    };
    /// A distance.
    pub const LENGTH: Dim = Dim {
        length: 1,
        angle: 0,
        mass: 0,
    };
    /// A rotation.
    pub const ANGLE: Dim = Dim {
        length: 0,
        angle: 1,
        mass: 0,
    };
    /// An area — what `area(face)` measures (D2). No field accepts one, so
    /// it exists to be composed with (`area(f) / w` is a length) and to be
    /// refused by name where it does not belong.
    pub const AREA: Dim = Dim {
        length: 2,
        angle: 0,
        mass: 0,
    };
    /// A volume — what `volume(body)` measures (D2).
    pub const VOLUME: Dim = Dim {
        length: 3,
        angle: 0,
        mass: 0,
    };
    /// A mass — what `mass(body)` measures (M1).
    pub const MASS: Dim = Dim {
        length: 0,
        angle: 0,
        mass: 1,
    };
    /// A density: mass per volume. `DENSITY.compose(VOLUME, 1) == MASS`,
    /// which is the whole reason the mass axis is an exponent rather than a
    /// flag.
    pub const DENSITY: Dim = Dim {
        length: -3,
        angle: 0,
        mass: 1,
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
            mass: axis(self.mass, other.mass)?,
        })
    }

    /// `self^n`, or `None` on exponent overflow (see [`Dim::compose`]).
    pub fn scaled(self, n: i32) -> Option<Dim> {
        let scale = |e: i8| -> Option<i8> { i8::try_from((e as i32).checked_mul(n)?).ok() };
        Some(Dim {
            length: scale(self.length)?,
            angle: scale(self.angle)?,
            mass: scale(self.mass)?,
        })
    }

    /// Halve every exponent, or `None` when one of them is odd (`sqrt` of a
    /// length is not a dimension this system can name).
    pub fn halved(self) -> Option<Dim> {
        if self.length % 2 != 0 || self.angle % 2 != 0 || self.mass % 2 != 0 {
            return None;
        }
        Some(Dim {
            length: self.length / 2,
            angle: self.angle / 2,
            mass: self.mass / 2,
        })
    }

    /// Name for a diagnostic.
    pub fn label(self) -> String {
        match (self.length, self.angle, self.mass) {
            (0, 0, 0) => "a plain number".to_string(),
            (1, 0, 0) => "a length".to_string(),
            (0, 1, 0) => "an angle".to_string(),
            (0, 0, 1) => "a mass".to_string(),
            (-3, 0, 1) => "a density".to_string(),
            (l, a, m) => {
                let mut parts = Vec::new();
                if l != 0 {
                    parts.push(format!("length^{l}"));
                }
                if a != 0 {
                    parts.push(format!("angle^{a}"));
                }
                if m != 0 {
                    parts.push(format!("mass^{m}"));
                }
                parts.join("·")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_named_dimension_round_trips_through_serde_in_both_spellings() {
        for (d, pascal, lower) in [
            (Dimension::Length, "\"Length\"", "\"length\""),
            (Dimension::Angle, "\"Angle\"", "\"angle\""),
            (Dimension::Count, "\"Count\"", "\"count\""),
            (Dimension::Ratio, "\"Ratio\"", "\"ratio\""),
            (Dimension::Mass, "\"Mass\"", "\"mass\""),
            (Dimension::Density, "\"Density\"", "\"density\""),
        ] {
            assert_eq!(serde_json::to_string(&d).unwrap(), pascal);
            assert_eq!(
                serde_json::from_str::<Dimension>(pascal).unwrap(),
                d,
                "{pascal}"
            );
            assert_eq!(
                serde_json::from_str::<Dimension>(lower).unwrap(),
                d,
                "{lower}"
            );
        }
    }

    #[test]
    fn a_density_times_a_volume_is_a_mass_by_the_exponents() {
        // The whole point of a mass AXIS rather than a flag: no special
        // case computes this, the arithmetic does.
        assert_eq!(Dim::DENSITY.compose(Dim::VOLUME, 1), Some(Dim::MASS));
        // ...and a mass divided by a volume is a density again.
        assert_eq!(Dim::MASS.compose(Dim::VOLUME, -1), Some(Dim::DENSITY));
        // A mass is not a length, an angle or a plain number.
        assert_ne!(Dim::MASS, Dim::LENGTH);
        assert_ne!(Dim::MASS, Dim::NONE);
        assert_eq!(Dimension::Mass.dim(), Dim::MASS);
        assert_eq!(Dimension::Density.dim(), Dim::DENSITY);
    }

    #[test]
    fn the_mass_axis_overflows_loudly_like_the_others() {
        assert_eq!(Dim::MASS.scaled(128), None);
        assert_eq!(Dim::MASS.scaled(127).unwrap().mass, 127);
        let big = Dim::MASS.scaled(100).unwrap();
        assert_eq!(big.compose(big, 1), None);
        assert_eq!(big.compose(big, -1), Some(Dim::NONE));
    }

    #[test]
    fn an_odd_mass_exponent_has_no_square_root() {
        assert_eq!(Dim::MASS.halved(), None);
        assert_eq!(Dim::MASS.scaled(2).unwrap().halved(), Some(Dim::MASS));
        // A density's length exponent is odd, so it has none either.
        assert_eq!(Dim::DENSITY.halved(), None);
    }

    #[test]
    fn labels_name_the_composite_including_the_mass_axis() {
        assert_eq!(Dim::NONE.label(), "a plain number");
        assert_eq!(Dim::MASS.label(), "a mass");
        assert_eq!(Dim::DENSITY.label(), "a density");
        assert_eq!(Dim::AREA.label(), "length^2");
        assert_eq!(
            Dim::MASS.compose(Dim::LENGTH, 1).unwrap().label(),
            "length^1·mass^1"
        );
        assert_eq!(Dimension::Mass.label(), "a mass");
        assert_eq!(Dimension::Density.label(), "a density");
    }
}
