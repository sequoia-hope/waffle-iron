use serde::{Deserialize, Serialize};

/// The kind of topological entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum TopoKind {
    Vertex,
    Edge,
    Face,
    Shell,
    Solid,
}

/// The rotation-invariant content of a face on a surface of revolution: what
/// identifies that face no matter where around the axis you look at it.
///
/// **Why a fingerprint needs this** (N0 of `specs/agent_mechanical_design.md`
/// §5.1). A face that goes all the way round its axis has no distinguished
/// point: its area-weighted centroid lies ON the axis, where the surface
/// normal is not defined and the direction from the axis to "the centroid" is
/// whatever f64 summation rounding left behind — measured at 3.1e-17 m off a
/// cylinder's axis, so changing that cylinder's height from 2 to 2.0001 swung
/// the reported centroid 58° round the axis and flipped the reported normal.
/// A fingerprint exists to be STABLE, so such a face reports no point normal
/// at all and carries this instead: the axis, the on-axis centroid (in
/// [`TopoSignature::centroid`], which is a feature — it pins the axial
/// position), and the surface's own radii.
///
/// Every cylinder, cone, sphere and torus face carries one, full turn or not;
/// it is read straight off the analytic surface, so it is exact either way.
/// Whether the face ALSO has a point normal is what says where its centroid
/// is: a face with a normal has its centroid projected onto the surface, a
/// face without has it on the axis.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct AxisDescriptor {
    /// Unit axis direction, canonicalised into one half-space so the value
    /// does not depend on which way round the constructor ran the axis.
    /// `None` for a sphere, which has no distinguished axis.
    pub direction: Option<[f64; 3]>,
    /// Distance from the axis: a cylinder's radius, a torus's MAJOR radius, a
    /// sphere's radius. `None` for a cone, whose radius varies along the axis.
    pub radius: Option<f64>,
    /// A cone's half-angle, in radians.
    pub half_angle: Option<f64>,
    /// A torus's tube radius.
    pub minor_radius: Option<f64>,
    /// How far this FACE reaches along the axis, in meters — the one field
    /// here that is about the trim rather than the surface, so a short band
    /// and a long one on the same cylinder are told apart. `None` for a
    /// sphere (no axis to measure along).
    pub extent: Option<f64>,
}

/// Geometric signature of a topological entity.
/// Used for signature-based matching when role-based resolution fails.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct TopoSignature {
    /// Surface type (planar, cylindrical, conical, spherical, toroidal, nurbs).
    pub surface_type: Option<String>,
    /// Surface area (for faces).
    pub area: Option<f64>,
    /// Centroid position [x, y, z]. For a face on a surface of revolution
    /// that goes all the way round, this is the area centroid ON THE AXIS and
    /// `normal` is `None`; a consumer that needs a point on the face itself
    /// must require `normal` alongside it (see [`AxisDescriptor`]).
    pub centroid: Option<[f64; 3]>,
    /// Outward-pointing normal at centroid (for faces). `None` where there is
    /// no single one: a full-turn surface of revolution, whose centroid lies
    /// on its axis (see [`AxisDescriptor`]).
    pub normal: Option<[f64; 3]>,
    /// Axis-aligned bounding box [min_x, min_y, min_z, max_x, max_y, max_z].
    pub bbox: Option<[f64; 6]>,
    /// Hash of the adjacency structure.
    pub adjacency_hash: Option<u64>,
    /// Edge length (for edges).
    pub length: Option<f64>,
    /// The rotation-invariant content of a surface of revolution. Serde-
    /// defaulted: a document written before N0 simply has no key here.
    #[serde(default)]
    pub axis: Option<AxisDescriptor>,
}

impl TopoSignature {
    pub fn empty() -> Self {
        Self {
            surface_type: None,
            area: None,
            centroid: None,
            normal: None,
            bbox: None,
            adjacency_hash: None,
            length: None,
            axis: None,
        }
    }
}

/// User-specified geometric query for selecting entities.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct TopoQuery {
    /// Filters to narrow down candidate entities.
    pub filters: Vec<Filter>,
    /// How to break ties if multiple entities match.
    pub tie_break: Option<TieBreak>,
}

/// Filter predicate for TopoQuery.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum Filter {
    /// Entity's surface/curve type must match.
    SurfaceType { surface_type: String },
    /// Entity's normal must be within `tolerance` radians of `direction`.
    NormalDirection { direction: [f64; 3], tolerance: f64 },
    /// Entity must be within `distance` of `point`.
    NearPoint { point: [f64; 3], distance: f64 },
    /// Entity's area must be in range [min, max].
    AreaRange { min: f64, max: f64 },
}

/// Tie-breaking strategy when multiple entities match a query.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum TieBreak {
    /// Pick the entity with the largest area.
    LargestArea,
    /// Pick the entity nearest to the given point.
    NearestTo { point: [f64; 3] },
    /// Pick the entity whose centroid lies farthest along `direction`
    /// (the top face of a boss: `[0, 0, 1]`). Ties keep the first.
    FarthestAlong { direction: [f64; 3] },
    /// Pick the entity with the smallest index (arbitrary but deterministic).
    SmallestIndex,
}
