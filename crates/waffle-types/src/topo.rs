use serde::{Deserialize, Serialize};

/// The kind of topological entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TopoKind {
    Vertex,
    Edge,
    Face,
    Shell,
    Solid,
}

/// Geometric signature of a topological entity.
/// Used for signature-based matching when role-based resolution fails.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopoSignature {
    /// Surface type (planar, cylindrical, conical, spherical, toroidal, nurbs).
    pub surface_type: Option<String>,
    /// Surface area (for faces).
    pub area: Option<f64>,
    /// Centroid position [x, y, z].
    pub centroid: Option<[f64; 3]>,
    /// Outward-pointing normal at centroid (for faces).
    pub normal: Option<[f64; 3]>,
    /// Axis-aligned bounding box [min_x, min_y, min_z, max_x, max_y, max_z].
    pub bbox: Option<[f64; 6]>,
    /// Hash of the adjacency structure.
    pub adjacency_hash: Option<u64>,
    /// Edge length (for edges).
    pub length: Option<f64>,
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
        }
    }
}

/// User-specified geometric query for selecting entities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopoQuery {
    /// Filters to narrow down candidate entities.
    pub filters: Vec<Filter>,
    /// How to break ties if multiple entities match.
    pub tie_break: Option<TieBreak>,
}

/// Filter predicate for TopoQuery.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
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
pub enum TieBreak {
    /// Pick the entity with the largest area.
    LargestArea,
    /// Pick the entity nearest to the given point.
    NearestTo { point: [f64; 3] },
    /// Pick the entity with the smallest index (arbitrary but deterministic).
    SmallestIndex,
}
