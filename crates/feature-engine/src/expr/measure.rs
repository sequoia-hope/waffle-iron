//! Measurement functions — D2 of `specs/drawings_and_mbd.md` §6 (and P4 of
//! `specs/agent_mechanical_design.md` §6, which is the same increment).
//!
//! An expression can read the MODEL: `depth = distance(wall_a, wall_b) / 2`.
//! This module holds the three pieces that are independent of how the
//! measurement is actually taken:
//!
//! 1. [`MEASUREMENTS`] — the function table. Name, arity and the
//!    **dimension of the answer**, all three validated or attached by the
//!    evaluator rather than by whoever answers the call. One table, so a
//!    measurer cannot report an area as a length.
//! 2. [`Measurer`] — the contract the answer comes through. The evaluator
//!    knows nothing about the kernel; `crate::measure::TreeMeasurer` is the
//!    implementation that does.
//! 3. [`MeasureRefusal`] — why a measurement could not be taken, in the two
//!    shapes a caller must respond to differently: a named entity that did
//!    not resolve, and a context that cannot answer at all.
//!
//! ## Arguments are entity NAMES, not expressions
//!
//! `distance(wall_a, wall_b)` takes two N1 entity names
//! (`specs/agent_mechanical_design.md` §5.2), optionally body-qualified
//! (`plate.top_face`). They are a **separate namespace** from design
//! parameters and are parsed as a separate AST node
//! ([`super::Expr::Measure`]), which is what keeps
//! [`super::Expr::identifiers`] — the parameter dependency list — free of
//! them. A parameter named `wall_a` and an entity named `wall_a` are
//! different things and neither shadows the other.
//!
//! §6 of the drawings spec says "arguments are `GeomRef`s written in the
//! existing selector syntax". There IS no textual selector syntax in the
//! tree — `Selector` is a serde-tagged enum authored as JSON — so what
//! landed is the N1 name, which §6 P4 of the MCP spec asks for in the same
//! words ("arguments accept N1 names") and which is the only spelling a
//! person or an agent can type into a parameter field.
//!
//! ## Working space
//!
//! A measurer answers in the evaluator's own working space (see
//! [`super`]): millimetres for a length, mm² for an area, mm³ for a volume,
//! degrees for an angle. The kernel works in metres, so the conversion
//! happens at the measurer, once, next to the number it scales — not here,
//! and not at the field boundary, which already has `as_length_meters` for
//! the other direction.

use super::dim::Dim;
use super::Span;

/// One measurement function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MeasureFn {
    /// How it is written in an expression.
    pub name: &'static str,
    /// How many entity names it takes. Every measurement has a fixed arity
    /// — there is no `min`-style variadic among them.
    pub arity: usize,
    /// The dimension of the answer, attached by the evaluator so that every
    /// caller of this function agrees about it.
    ///
    /// `None` for a function whose answer this evaluator cannot dimension.
    /// Such a function parses (its name and arity are checked, so the
    /// spelling cannot drift) and refuses at evaluation, naming why.
    ///
    /// **Every entry has one since M1.** `mass` was the one that did not:
    /// `Dim` carried length and angle exponents only, so a mass had no axis
    /// to live on, and the document model had no density to read. M1 added
    /// the mass axis (`waffle_types::dimension`) and the material table
    /// (`crate::types::Material`). The `None` arm stays because the
    /// evaluator must handle it — this is a library and the table is public
    /// — and because the next function whose answer has no nameable
    /// dimension should refuse the same way rather than inventing one.
    pub dim: Option<Dim>,
    /// What it measures, for a diagnostic and for the schema.
    pub what: &'static str,
}

/// What a [`MeasureFn`] with no `dim` refuses with, said once.
///
/// No entry in [`MEASUREMENTS`] is in that state since M1 — `mass` was, and
/// now carries [`Dim::MASS`] — so this is the message for a future function
/// that measures something this dimension system cannot name. It is still
/// reachable (the table is public and a caller can build a `MeasureFn`), and
/// the evaluator must not panic on it.
pub const NO_NAMEABLE_DIMENSION: &str = "this measurement has no dimension the \
    evaluator can name, so there is no number it could return that the rest of \
    the expression could carry honestly";

/// The measurement functions, in the order §6 of the drawings spec lists
/// them.
///
/// This table is the single source of truth for all three of: which names
/// are callable, how many entities each takes, and what dimension the
/// answer carries. The parser reads it for the first two and the evaluator
/// for the third.
pub const MEASUREMENTS: &[MeasureFn] = &[
    MeasureFn {
        name: "volume",
        arity: 1,
        dim: Some(Dim::VOLUME),
        what: "the volume of a solid",
    },
    MeasureFn {
        name: "area",
        arity: 1,
        dim: Some(Dim::AREA),
        what: "the area of a face",
    },
    MeasureFn {
        name: "length",
        arity: 1,
        dim: Some(Dim::LENGTH),
        what: "the arc length of an edge",
    },
    MeasureFn {
        name: "distance",
        arity: 2,
        dim: Some(Dim::LENGTH),
        what: "the minimum distance between two entities",
    },
    MeasureFn {
        name: "angle",
        arity: 2,
        dim: Some(Dim::ANGLE),
        what: "the angle between two planar faces, axes or straight edges",
    },
    MeasureFn {
        name: "radius",
        arity: 1,
        dim: Some(Dim::LENGTH),
        what: "the radius of a cylinder, cone, sphere or circular edge",
    },
    MeasureFn {
        name: "mass",
        arity: 1,
        dim: Some(Dim::MASS),
        what: "the mass of a solid, from its material's density (M1)",
    },
];

/// The measurement function called `name`, if there is one.
pub fn measure_fn(name: &str) -> Option<&'static MeasureFn> {
    MEASUREMENTS.iter().find(|m| m.name == name)
}

/// True if `name` is a measurement function.
///
/// Deliberately NOT part of [`super::is_reserved_word`]: a measurement name
/// is callable-only. See the note in [`super`].
pub fn is_measurement(name: &str) -> bool {
    measure_fn(name).is_some()
}

/// One entity-name argument, as written, with the byte range it occupies.
///
/// The span is what makes an entity RENAME exact, the same way
/// [`super::Expr::reference_spans`] makes a parameter rename exact: the AST
/// says which byte ranges of the source are entity references, and the
/// rename splices precisely those.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityArg {
    /// The name as written, dots included (`plate.top_face`). This is the
    /// key of `FeatureTree::names`.
    pub name: String,
    /// The byte range of the whole dotted path.
    pub span: Span,
}

/// One measurement, as the evaluator hands it to a [`Measurer`].
#[derive(Debug, Clone, Copy)]
pub struct MeasureCall<'a> {
    /// The function name, borrowed from [`MEASUREMENTS`] — so a measurer can
    /// match on it against the table's own `&'static str`s.
    pub function: &'static str,
    /// Its entity-name arguments, already checked to be `arity` of them.
    pub args: &'a [EntityArg],
    /// The byte range of the whole call, for a diagnostic.
    pub span: Span,
}

impl MeasureCall<'_> {
    /// The argument names, in order.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.args.iter().map(|a| a.name.as_str())
    }

    /// The `i`th argument's name. Panics only on a measurer that ignores
    /// the arity the parser enforced.
    pub fn name(&self, i: usize) -> &str {
        &self.args[i].name
    }
}

/// Why a measurement could not be taken.
///
/// Two shapes, because a caller responds to them differently: an entity
/// refusal names a thing the author can fix (a name that is gone, a name
/// pointing at the wrong kind of entity), while an unavailable context is
/// about where the expression is being evaluated and is not the author's
/// mistake.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MeasureRefusal {
    /// A named entity did not resolve — under `Strict`, always, so a near
    /// miss is a refusal and never a different entity (§5.3 N2) — or the
    /// kernel refused to measure it.
    Entity { name: String, reason: String },
    /// This context cannot answer the function at all: no model geometry
    /// here, or no density to compute a mass from.
    Unavailable { reason: String },
}

/// What answers a measurement.
///
/// The evaluator holds one of these as `Option<&dyn Measurer>`: `None` is
/// an environment with no model behind it (a stateless preview before any
/// rebuild), where every measurement is a typed refusal rather than a
/// guessed number.
pub trait Measurer {
    /// Measure `call`, answering in the evaluator's WORKING SPACE —
    /// millimetres, mm², mm³, degrees. The dimension is attached by the
    /// evaluator from [`MEASUREMENTS`]; an implementation that returns a
    /// number in the wrong unit is a bug in the implementation, which is
    /// why the scaling lives next to the kernel call.
    fn measure(&self, call: &MeasureCall<'_>) -> Result<f64, MeasureRefusal>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_covers_every_function_section_6_lists() {
        // §6 of `specs/drawings_and_mbd.md`, verbatim.
        let names: Vec<&str> = MEASUREMENTS.iter().map(|m| m.name).collect();
        assert_eq!(
            names,
            vec!["volume", "area", "length", "distance", "angle", "radius", "mass"]
        );
    }

    #[test]
    fn arities_and_dimensions_are_the_tables_business() {
        assert_eq!(measure_fn("distance").map(|m| m.arity), Some(2));
        assert_eq!(measure_fn("angle").map(|m| m.arity), Some(2));
        for one in ["volume", "area", "length", "radius", "mass"] {
            assert_eq!(measure_fn(one).map(|m| m.arity), Some(1), "{one}");
        }
        assert_eq!(
            measure_fn("distance").and_then(|m| m.dim),
            Some(Dim::LENGTH)
        );
        assert_eq!(measure_fn("length").and_then(|m| m.dim), Some(Dim::LENGTH));
        assert_eq!(measure_fn("radius").and_then(|m| m.dim), Some(Dim::LENGTH));
        assert_eq!(measure_fn("angle").and_then(|m| m.dim), Some(Dim::ANGLE));
        assert_eq!(measure_fn("area").and_then(|m| m.dim), Some(Dim::AREA));
        assert_eq!(measure_fn("volume").and_then(|m| m.dim), Some(Dim::VOLUME));
        // `mass` has one since M1 widened `Dim` with the mass axis.
        assert_eq!(measure_fn("mass").and_then(|m| m.dim), Some(Dim::MASS));
        // EVERY entry carries a dimension now, which is the property worth
        // pinning: a measurement whose answer the evaluator cannot
        // dimension can only refuse, and none of these do.
        for m in MEASUREMENTS {
            assert!(m.dim.is_some(), "{} has no dimension", m.name);
        }
        assert_eq!(measure_fn("nope"), None);
    }

    #[test]
    fn an_area_is_a_length_squared_and_a_volume_a_length_cubed() {
        // P1's exponents, which is what makes `distance(a,b)/2` a length and
        // `area(f)` refused by a depth.
        assert_eq!(Dim::AREA, Dim::LENGTH.compose(Dim::LENGTH, 1).unwrap());
        assert_eq!(Dim::VOLUME, Dim::AREA.compose(Dim::LENGTH, 1).unwrap());
        assert_eq!(Dim::AREA.label(), "length^2");
        assert_eq!(Dim::VOLUME.label(), "length^3");
    }
}
