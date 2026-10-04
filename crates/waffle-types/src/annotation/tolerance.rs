//! Tolerances — size tolerances on a dimension, and geometric tolerances in
//! a feature control frame (`specs/drawings_and_mbd.md` §9, increment M1).
//!
//! ## A tolerance value is not a float
//!
//! §9 sketches `Symmetric { plus_minus: f64 }` and friends. A bare `f64` here
//! is the defect P1 exists to refuse: nothing in the type says whether 0.1 is
//! a tenth of a millimetre, a tenth of a metre or a tenth of a degree, and a
//! tolerance read in the wrong unit is a scrapped part. So every magnitude is
//! a [`ToleranceValue`] — the number plus the [`Dimension`] it commits to —
//! and it is read through a NAMED boundary ([`ToleranceValue::as_length_meters`],
//! [`ToleranceValue::as_angle_radians`]), the same discipline
//! `feature_engine::expr::Quantity` applies to an evaluated expression, and for
//! the same reason: an angle field must not be able to accidentally take
//! radians for degrees.
//!
//! Model units, matching [`super::measure`]: **metres** for a length,
//! **radians** for an angle. That is deliberately not degrees even though
//! every angle on the *feature tree* is stored in degrees — a tolerance lives
//! next to its dimension's measured value and is formatted by the same code,
//! so it uses that neighbour's unit. `ToleranceValue::angle_degrees` is the
//! constructor a degree-thinking caller wants.
//!
//! ## `limits_of` is the one place a tolerance becomes two numbers
//!
//! Every arm — symmetric, bilateral, limits, an ISO 286 fit — describes the
//! same thing in the end: the largest and smallest the feature may be. That
//! conversion lives in [`Tolerance::limits_of`] and nowhere else, so the SVG
//! renderer, a PMI frame and (eventually) M3's AP242 writer cannot each have
//! their own idea of what `H7` means. A renderer is handed the resolved pair
//! in the layout record; it never computes one.
//!
//! ## Deviations from the §9 sketch, and why
//!
//! - **`Fit { hole, shaft }` are both OPTIONAL.** §9 has both mandatory,
//!   which only describes a mating-pair callout (`⌀20 H7/g6`). The far more
//!   common drawing is a single feature carrying its own class — a hole
//!   `⌀20 H7`, a shaft `⌀20 g6` — and with both mandatory there is no way to
//!   write one. Both absent is refused by name
//!   ([`ToleranceError::FitWithNoClass`]), so the pair cannot degenerate into
//!   a tolerance that tolerances nothing.
//! - **`Limits` are absolute SIZES; `Bilateral` are SIGNED DEVIATIONS.** §9
//!   gives both arms the same `upper`/`lower` shape without saying which. The
//!   distinction is ISO 129-1's own (a limit dimension prints `25.021 /
//!   25.000`; a deviation dimension prints `25 +0.021 / 0`), and leaving it
//!   implicit is how a `minus: 0.05` meaning "0.05 smaller" gets read as
//!   "0.05 larger". So `Bilateral.minus` is normally NEGATIVE, both arms are
//!   validated for `upper >= lower`, and the doc comments say so on the
//!   fields.
//! - **A geometric tolerance's datum rule is enforced.** ISO 1101 makes a
//!   datum reference meaningless on a form characteristic (flatness is
//!   flatness, with respect to nothing) and mandatory on an orientation or
//!   location one. [`GeometricTolerance::validate`] refuses both mistakes —
//!   a frame that is malformed by the standard would otherwise be exported to
//!   AP242 verbatim in M3 and rejected by a CMM, far from where it was typed.

use serde::{Deserialize, Serialize};

use super::iso286;
use super::DimensionKind;
use crate::dimension::Dimension;

/// What went wrong with a tolerance, by name.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ToleranceError {
    #[error("a tolerance magnitude must be a length or an angle, not {found}")]
    NotAToleranceDimension { found: &'static str },
    #[error("a tolerance magnitude must be finite, got {value}")]
    NotFinite { value: f64 },
    #[error("a tolerance magnitude is {expected}, but this one is {found}")]
    DimensionMismatch {
        expected: &'static str,
        found: &'static str,
    },
    #[error("a tolerance's two magnitudes must share a dimension; got {first} and {second}")]
    MixedDimensions {
        first: &'static str,
        second: &'static str,
    },
    #[error("a tolerance's upper limit {upper} is below its lower limit {lower}")]
    Inverted { upper: f64, lower: f64 },
    #[error("a symmetric tolerance's ± magnitude must not be negative, got {value}")]
    NegativeSymmetric { value: f64 },
    #[error("a fit tolerance names neither a hole class nor a shaft class")]
    FitWithNoClass,
    #[error(
        "a fit class is a letter and an IT grade, like `H7` or `g6`; `{text}` is not one: {why}"
    )]
    NotAFitClass { text: String, why: &'static str },
    #[error("an ISO 286 fit needs a nominal SIZE, which a {kind} dimension is not")]
    FitOnANonSize { kind: &'static str },
    #[error("ISO 286: {0}")]
    Iso286(#[from] iso286::Iso286Error),
    #[error(
        "a {characteristic} tolerance is a form tolerance and takes no datum reference, \
         but {count} were given"
    )]
    FormTakesNoDatum {
        characteristic: &'static str,
        count: usize,
    },
    #[error("a {characteristic} tolerance needs at least one datum reference")]
    DatumRequired { characteristic: &'static str },
    #[error("a geometric tolerance's zone width is a length, not {found}")]
    ZoneIsNotALength { found: &'static str },
    #[error("a geometric tolerance's zone width must be positive, got {value}")]
    ZoneNotPositive { value: f64 },
}

/// A tolerance magnitude: the number AND the dimension it commits to.
///
/// Metres for a [`Dimension::Length`], RADIANS for a [`Dimension::Angle`] —
/// see the module docs. No other dimension is a tolerance, and
/// [`ToleranceValue::validate`] refuses one.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct ToleranceValue {
    /// Metres for a length, radians for an angle.
    pub magnitude: f64,
    /// Which of those two this is. Not optional and not inferred: a
    /// tolerance whose dimension has to be guessed from its neighbour is the
    /// bare float this type replaces.
    pub dimension: Dimension,
}

impl ToleranceValue {
    /// A length tolerance, in METRES.
    pub fn length_meters(magnitude: f64) -> Self {
        Self {
            magnitude,
            dimension: Dimension::Length,
        }
    }

    /// An angular tolerance, in RADIANS.
    pub fn angle_radians(magnitude: f64) -> Self {
        Self {
            magnitude,
            dimension: Dimension::Angle,
        }
    }

    /// An angular tolerance written in DEGREES, converted on the way in.
    pub fn angle_degrees(degrees: f64) -> Self {
        Self::angle_radians(degrees.to_radians())
    }

    /// Accept as a LENGTH, in metres.
    pub fn as_length_meters(self) -> Result<f64, ToleranceError> {
        self.check(Dimension::Length)?;
        Ok(self.magnitude)
    }

    /// Accept as an ANGLE, in radians.
    pub fn as_angle_radians(self) -> Result<f64, ToleranceError> {
        self.check(Dimension::Angle)?;
        Ok(self.magnitude)
    }

    /// Accept as an ANGLE, in degrees.
    pub fn as_angle_degrees(self) -> Result<f64, ToleranceError> {
        Ok(self.as_angle_radians()?.to_degrees())
    }

    /// The magnitude in its own model unit, whichever that is. For a caller
    /// that genuinely does not care which — a sign test, a formatter that
    /// has already branched on the dimension — and nothing else.
    pub fn magnitude(self) -> f64 {
        self.magnitude
    }

    /// Finite, and a dimension a tolerance can have.
    pub fn validate(self) -> Result<(), ToleranceError> {
        if !self.magnitude.is_finite() {
            return Err(ToleranceError::NotFinite {
                value: self.magnitude,
            });
        }
        match self.dimension {
            Dimension::Length | Dimension::Angle => Ok(()),
            other => Err(ToleranceError::NotAToleranceDimension {
                found: other.label(),
            }),
        }
    }

    fn check(self, want: Dimension) -> Result<(), ToleranceError> {
        if self.dimension == want {
            Ok(())
        } else {
            Err(ToleranceError::DimensionMismatch {
                expected: want.label(),
                found: self.dimension.label(),
            })
        }
    }
}

/// Which side of a fit a class describes. The role, not the spelling, is what
/// decides the ISO 286 lookup and the canonical display case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FitRole {
    /// An internal feature (a hole). Classes are upper case: `H7`.
    Hole,
    /// An external feature (a shaft). Classes are lower case: `g6`.
    Shaft,
}

/// An ISO 286 tolerance class: a deviation letter and an IT grade.
///
/// Persisted and displayed as the string a drafter writes — `"H7"`, `"g6"` —
/// because that is the only spelling anyone reading the file or the drawing
/// recognises. A malformed one fails deserialization loudly rather than
/// becoming a fit nobody can resolve.
///
/// The CASE in `letter` is whatever was authored; it carries no meaning,
/// because the role comes from which field of [`Tolerance::Fit`] the class
/// sits in. [`FitClass::display_for`] prints it in the canonical case for its
/// role, so a shaft class authored as `G6` still prints `g6`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "json-schema", schemars(with = "String"))]
pub struct FitClass {
    /// The deviation letter, as authored (`"H"`, `"g"`, `"js"`).
    pub letter: String,
    /// The IT grade, 1..=18.
    pub grade: u8,
}

impl FitClass {
    /// Parse `"H7"`, `"g6"`, `"JS9"`. The letter part may be one or two
    /// alphabetic characters; the rest must be the grade.
    pub fn parse(text: &str) -> Result<Self, ToleranceError> {
        let trimmed = text.trim();
        let split = trimmed.find(|c: char| c.is_ascii_digit()).ok_or_else(|| {
            ToleranceError::NotAFitClass {
                text: trimmed.to_string(),
                why: "it has no IT grade",
            }
        })?;
        let (letter, grade) = trimmed.split_at(split);
        if letter.is_empty() || !letter.chars().all(|c| c.is_ascii_alphabetic()) {
            return Err(ToleranceError::NotAFitClass {
                text: trimmed.to_string(),
                why: "the deviation letter must be one or two ASCII letters",
            });
        }
        let grade: u8 = grade.parse().map_err(|_| ToleranceError::NotAFitClass {
            text: trimmed.to_string(),
            why: "the IT grade must be a whole number",
        })?;
        if !(1..=18).contains(&grade) {
            return Err(ToleranceError::NotAFitClass {
                text: trimmed.to_string(),
                why: "the IT grade must be between 1 and 18",
            });
        }
        Ok(Self {
            letter: letter.to_string(),
            grade,
        })
    }

    /// How this class PRINTS for `role`: upper case for a hole, lower for a
    /// shaft, whatever the authored case was.
    pub fn display_for(&self, role: FitRole) -> String {
        let letter = match role {
            FitRole::Hole => self.letter.to_ascii_uppercase(),
            FitRole::Shaft => self.letter.to_ascii_lowercase(),
        };
        format!("{letter}{}", self.grade)
    }

    /// The limit deviations this class gives at `nominal_mm`, in
    /// MICROMETRES, from the ISO 286 tables.
    pub fn deviations_um(
        &self,
        nominal_mm: f64,
        role: FitRole,
    ) -> Result<iso286::Deviations, ToleranceError> {
        let d = match role {
            FitRole::Hole => iso286::hole_deviations(nominal_mm, &self.letter, self.grade)?,
            FitRole::Shaft => iso286::shaft_deviations(nominal_mm, &self.letter, self.grade)?,
        };
        Ok(d)
    }
}

impl std::fmt::Display for FitClass {
    /// As authored — `letter` then `grade`. Use [`FitClass::display_for`] for
    /// the canonical case of a known role.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.letter, self.grade)
    }
}

impl TryFrom<String> for FitClass {
    type Error = ToleranceError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        FitClass::parse(&value)
    }
}

impl From<FitClass> for String {
    fn from(value: FitClass) -> String {
        value.to_string()
    }
}

/// A size tolerance on a dimension (§9).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum Tolerance {
    /// `25 ±0.1`. `plus_minus` must not be negative.
    Symmetric { plus_minus: ToleranceValue },
    /// `25 +0.021 / −0.005`. Both are SIGNED deviations from the nominal, so
    /// `minus` is normally NEGATIVE and a unilateral tolerance is this arm
    /// with one of them zero. `plus >= minus` is required.
    Bilateral {
        /// The upper deviation, normally ≥ 0.
        plus: ToleranceValue,
        /// The lower deviation, normally ≤ 0.
        minus: ToleranceValue,
    },
    /// `25.021 / 25.000` — the two absolute SIZES, not deviations. The
    /// nominal plays no part in resolving this arm. `upper >= lower`.
    Limits {
        upper: ToleranceValue,
        lower: ToleranceValue,
    },
    /// An ISO 286 class on one or both sides of a fit: `⌀20 H7`,
    /// `⌀20 H7/g6`. At least one must be present.
    Fit {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        hole: Option<FitClass>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        shaft: Option<FitClass>,
    },
    /// A boxed basic dimension: the theoretically exact size, with the
    /// variation controlled by a geometric tolerance instead. It has no
    /// limits of its own, which is why [`Tolerance::limits_of`] returns
    /// `None` for it rather than a zero-width band.
    Basic,
}

/// The two limits of size a tolerance resolves to, in the dimension's own
/// model unit (metres, or radians for an angular dimension).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct Limits {
    pub upper: f64,
    pub lower: f64,
}

impl Limits {
    /// `upper − lower`: the width of the tolerance band.
    pub fn width(self) -> f64 {
        self.upper - self.lower
    }
}

impl Tolerance {
    /// The dimension every magnitude in this tolerance commits to, or `None`
    /// for an arm that carries no magnitude ([`Tolerance::Fit`], which is
    /// always a length by ISO 286, and [`Tolerance::Basic`], which carries
    /// nothing).
    pub fn dimension(&self) -> Option<Dimension> {
        match self {
            Tolerance::Symmetric { plus_minus } => Some(plus_minus.dimension),
            Tolerance::Bilateral { plus, .. } => Some(plus.dimension),
            Tolerance::Limits { upper, .. } => Some(upper.dimension),
            Tolerance::Fit { .. } => Some(Dimension::Length),
            Tolerance::Basic => None,
        }
    }

    /// Structurally sound: finite magnitudes of one tolerable dimension, a
    /// band that is not inverted, a fit that names a class.
    ///
    /// This does NOT consult the ISO 286 tables — a fit's letter and grade
    /// are checked when it is resolved at a nominal, because whether `H7` is
    /// tabulated depends on the size.
    pub fn validate(&self) -> Result<(), ToleranceError> {
        let pair = |a: ToleranceValue, b: ToleranceValue| -> Result<(), ToleranceError> {
            a.validate()?;
            b.validate()?;
            if a.dimension != b.dimension {
                return Err(ToleranceError::MixedDimensions {
                    first: a.dimension.label(),
                    second: b.dimension.label(),
                });
            }
            if a.magnitude < b.magnitude {
                return Err(ToleranceError::Inverted {
                    upper: a.magnitude,
                    lower: b.magnitude,
                });
            }
            Ok(())
        };
        match self {
            Tolerance::Symmetric { plus_minus } => {
                plus_minus.validate()?;
                if plus_minus.magnitude < 0.0 {
                    return Err(ToleranceError::NegativeSymmetric {
                        value: plus_minus.magnitude,
                    });
                }
                Ok(())
            }
            Tolerance::Bilateral { plus, minus } => pair(*plus, *minus),
            Tolerance::Limits { upper, lower } => pair(*upper, *lower),
            Tolerance::Fit { hole, shaft } => {
                if hole.is_none() && shaft.is_none() {
                    return Err(ToleranceError::FitWithNoClass);
                }
                Ok(())
            }
            Tolerance::Basic => Ok(()),
        }
    }

    /// Whether this tolerance may sit on a `kind` dimension.
    ///
    /// Two rules. An ANGULAR tolerance belongs only on an angular dimension
    /// and a length tolerance only on a linear one — the dimension's own
    /// measured value is in the other unit, so a mismatch would print a
    /// radian band under a millimetre nominal. And an ISO 286 fit needs a
    /// nominal SIZE: the tables are indexed by diameter, so a fit on a
    /// radius (half a size, which nobody writes) or on an angle or an
    /// ordinate is refused by name rather than resolved against a number
    /// that means something else.
    pub fn check_for(&self, kind: DimensionKind) -> Result<(), ToleranceError> {
        self.validate()?;
        if let Some(dim) = self.dimension() {
            let want = if kind.is_angular() {
                Dimension::Angle
            } else {
                Dimension::Length
            };
            if dim != want {
                return Err(ToleranceError::DimensionMismatch {
                    expected: want.label(),
                    found: dim.label(),
                });
            }
        }
        if matches!(self, Tolerance::Fit { .. }) && !is_iso286_size(kind) {
            return Err(ToleranceError::FitOnANonSize { kind: kind.name() });
        }
        Ok(())
    }

    /// The two limits of size, given the dimension's measured `nominal` in
    /// its own model unit (metres, or radians for an angular dimension).
    ///
    /// `None` for [`Tolerance::Basic`], which has no limits by definition.
    /// This is the ONLY place a tolerance becomes a pair of numbers; a
    /// renderer reads the result out of the layout record and never
    /// recomputes it.
    pub fn limits_of(&self, nominal: f64) -> Result<Option<Limits>, ToleranceError> {
        self.validate()?;
        if !nominal.is_finite() {
            return Err(ToleranceError::NotFinite { value: nominal });
        }
        Ok(match self {
            Tolerance::Symmetric { plus_minus } => Some(Limits {
                upper: nominal + plus_minus.magnitude,
                lower: nominal - plus_minus.magnitude,
            }),
            Tolerance::Bilateral { plus, minus } => Some(Limits {
                upper: nominal + plus.magnitude,
                lower: nominal + minus.magnitude,
            }),
            Tolerance::Limits { upper, lower } => Some(Limits {
                upper: upper.magnitude,
                lower: lower.magnitude,
            }),
            Tolerance::Fit { hole, shaft } => {
                // ISO 286 is tabulated in millimetres against the nominal
                // size; the model is in metres.
                let nominal_mm = nominal * 1e3;
                let mut upper = f64::NEG_INFINITY;
                let mut lower = f64::INFINITY;
                for (class, role) in [
                    (hole.as_ref(), FitRole::Hole),
                    (shaft.as_ref(), FitRole::Shaft),
                ] {
                    let Some(class) = class else { continue };
                    let d = class.deviations_um(nominal_mm, role)?;
                    // µm → metres, then back onto the nominal.
                    upper = upper.max(nominal + d.upper_um * 1e-6);
                    lower = lower.min(nominal + d.lower_um * 1e-6);
                }
                // Both-absent is already refused by `validate`.
                Some(Limits { upper, lower })
            }
            Tolerance::Basic => None,
        })
    }
}

/// Whether `kind` measures something ISO 286 calls a size — the question a
/// fit tolerance has to answer before it can look anything up.
fn is_iso286_size(kind: DimensionKind) -> bool {
    matches!(
        kind,
        DimensionKind::Distance
            | DimensionKind::PointLineDistance
            | DimensionKind::HDistance
            | DimensionKind::VDistance
            | DimensionKind::Diameter
    )
}

/// The geometric characteristic a feature control frame controls (ISO 1101).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum Characteristic {
    Flatness,
    Straightness,
    Circularity,
    Cylindricity,
    Perpendicularity,
    Parallelism,
    Angularity,
    Position,
    Concentricity,
    Symmetry,
    Profile,
    Runout,
}

/// What a characteristic requires of its datum list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatumRule {
    /// A form characteristic: no datum is meaningful.
    None,
    /// An orientation or location characteristic: at least one is required.
    AtLeastOne,
    /// Either is legal (`Profile`, which controls a form when unreferenced
    /// and a location when referenced).
    Optional,
}

impl Characteristic {
    /// The ISO 1101 symbol, for a renderer that has the GD&T glyphs.
    pub fn symbol(self) -> char {
        match self {
            Characteristic::Flatness => '\u{23E5}',         // ⏥
            Characteristic::Straightness => '\u{23E4}',     // ⏤
            Characteristic::Circularity => '\u{25CB}',      // ○
            Characteristic::Cylindricity => '\u{232D}',     // ⌭
            Characteristic::Perpendicularity => '\u{27C2}', // ⟂
            Characteristic::Parallelism => '\u{2225}',      // ∥
            Characteristic::Angularity => '\u{2220}',       // ∠
            Characteristic::Position => '\u{2316}',         // ⌖
            Characteristic::Concentricity => '\u{25CE}',    // ◎
            Characteristic::Symmetry => '\u{232F}',         // ⌯
            Characteristic::Profile => '\u{2313}',          // ⌓
            Characteristic::Runout => '\u{2197}',           // ↗
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Characteristic::Flatness => "flatness",
            Characteristic::Straightness => "straightness",
            Characteristic::Circularity => "circularity",
            Characteristic::Cylindricity => "cylindricity",
            Characteristic::Perpendicularity => "perpendicularity",
            Characteristic::Parallelism => "parallelism",
            Characteristic::Angularity => "angularity",
            Characteristic::Position => "position",
            Characteristic::Concentricity => "concentricity",
            Characteristic::Symmetry => "symmetry",
            Characteristic::Profile => "profile",
            Characteristic::Runout => "runout",
        }
    }

    /// Whether this characteristic takes datum references, per ISO 1101.
    ///
    /// The four FORM characteristics are self-referential — a surface is
    /// flat or it is not, with respect to nothing — so a datum on one is a
    /// frame no inspector can act on. The orientation and location ones are
    /// meaningless WITHOUT a datum, because they control a relationship.
    /// `Profile` is the one that is honestly both.
    pub fn datum_rule(self) -> DatumRule {
        match self {
            Characteristic::Flatness
            | Characteristic::Straightness
            | Characteristic::Circularity
            | Characteristic::Cylindricity => DatumRule::None,
            Characteristic::Perpendicularity
            | Characteristic::Parallelism
            | Characteristic::Angularity
            | Characteristic::Position
            | Characteristic::Concentricity
            | Characteristic::Symmetry
            | Characteristic::Runout => DatumRule::AtLeastOne,
            Characteristic::Profile => DatumRule::Optional,
        }
    }
}

/// A material condition modifier (ASME Y14.5 / ISO 2692).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum MaterialCondition {
    /// Maximum material condition, Ⓜ.
    Mmc,
    /// Least material condition, Ⓛ.
    Lmc,
    /// Regardless of feature size — the default, written explicitly.
    Rfs,
}

impl MaterialCondition {
    pub fn symbol(self) -> Option<char> {
        match self {
            MaterialCondition::Mmc => Some('\u{24C2}'), // Ⓜ
            MaterialCondition::Lmc => Some('\u{24C1}'), // Ⓛ
            // RFS is the default and has no symbol; a frame that prints one
            // would be printing a glyph no standard defines.
            MaterialCondition::Rfs => None,
        }
    }
}

/// A reference to a datum, by the label a `Datum` annotation (§7) carries.
///
/// A LABEL and not a [`crate::geom_ref::GeomRef`]: a datum feature control
/// frame references datum `A`, and which face `A` is, is the `Datum`
/// annotation's business. That also keeps a `GeometricTolerance` free of
/// references, which is what lets the layout record carry one verbatim —
/// `layout`'s invariant is that it holds no path back to the model.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct DatumRef {
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modifier: Option<MaterialCondition>,
}

impl DatumRef {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            modifier: None,
        }
    }
}

/// The shape of a tolerance zone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum ZoneShape {
    /// A cylinder of the stated diameter, printed `⌀`.
    Diametral,
    /// Two parallel planes the stated distance apart — the default.
    Width,
    /// A sphere of the stated diameter, printed `S⌀`.
    Spherical,
}

impl ZoneShape {
    /// The prefix this zone prints before the value, if any.
    pub fn prefix(self) -> &'static str {
        match self {
            ZoneShape::Diametral => "\u{2300}",
            ZoneShape::Width => "",
            ZoneShape::Spherical => "S\u{2300}",
        }
    }
}

/// A geometric tolerance — the content of a feature control frame (§9).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct GeometricTolerance {
    pub characteristic: Characteristic,
    /// The zone's width (or diameter, per `zone`). Always a LENGTH — every
    /// ISO 1101 zone is a distance, angularity included, which controls a
    /// surface's position within two planes rather than an angle directly.
    pub value: ToleranceValue,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modifier: Option<MaterialCondition>,
    /// Ordered — primary, secondary, tertiary.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub datums: Vec<DatumRef>,
    #[serde(default = "width_zone")]
    pub zone: ZoneShape,
}

fn width_zone() -> ZoneShape {
    ZoneShape::Width
}

impl GeometricTolerance {
    /// A frame with no datums and a width zone.
    pub fn new(characteristic: Characteristic, value: ToleranceValue) -> Self {
        Self {
            characteristic,
            value,
            modifier: None,
            datums: Vec::new(),
            zone: ZoneShape::Width,
        }
    }

    /// The same frame, referencing `datums` in order.
    pub fn with_datums(mut self, datums: Vec<DatumRef>) -> Self {
        self.datums = datums;
        self
    }

    /// The same frame with a diametral zone.
    pub fn diametral(mut self) -> Self {
        self.zone = ZoneShape::Diametral;
        self
    }

    /// Well formed by ISO 1101: a positive length zone, and a datum list the
    /// characteristic actually admits.
    pub fn validate(&self) -> Result<(), ToleranceError> {
        self.value.validate()?;
        if self.value.dimension != Dimension::Length {
            return Err(ToleranceError::ZoneIsNotALength {
                found: self.value.dimension.label(),
            });
        }
        if self.value.magnitude <= 0.0 {
            return Err(ToleranceError::ZoneNotPositive {
                value: self.value.magnitude,
            });
        }
        match self.characteristic.datum_rule() {
            DatumRule::None if !self.datums.is_empty() => Err(ToleranceError::FormTakesNoDatum {
                characteristic: self.characteristic.name(),
                count: self.datums.len(),
            }),
            DatumRule::AtLeastOne if self.datums.is_empty() => Err(ToleranceError::DatumRequired {
                characteristic: self.characteristic.name(),
            }),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests;
