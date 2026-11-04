//! `Operation::BoardShape` — PCB mechanical definition authored on a planar
//! sketch (`specs/board_shape_feature.md`).
//!
//! Validates entity role assignments (`outline`, `cutout`, `hole`, `keepout`)
//! analytically over exact `{Line, Arc, Circle}` geometry, builds the 3D board
//! substrate solid (`outline` extruded by `thickness`, minus interior cutouts
//! and drilled holes), and emits deterministic top-face mate connectors keyed
//! by stable element ID (`waffle:<sketch_id>/<entity_id>`).

use std::collections::{BTreeMap, HashMap};
use std::f64::consts::{PI, TAU};

use modeling_ops::{
    diff, execute_boolean, execute_extrude, BodyOutput, BooleanKind, Diagnostics, KernelBundle,
    OpResult, Provenance,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use waffle_types::kernel::units::{
    COS_FACE_NORMAL_ALIGNMENT, MIN_FEATURE_SIZE, TAU_COINCIDENT, TAU_MODEL, TAU_NORMALIZE, TAU_WORK,
};
use waffle_types::kernel::{
    AxisKind, ConstructSeed, KernelId, KernelIntrospect, KernelSolidHandle,
};
use waffle_types::{
    CircleProfile, ClosedProfile, OutputKey, Region, RegionEdge, Role, Sketch, SketchEntity,
    SketchPlaneBasis, SolveStatus, TopoKind, ARC_SAMPLES,
};

use crate::assembly::Frame;
use crate::connector::{ConnectorGeometry, PartConnector};
use crate::context::EditContext;
use crate::rebuild::{active_sketch, sketch_x_axis, unit_normal};
use crate::types::{EngineError, Feature, FeatureTree};

/// Which side(s) of the board a component height keepout applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum BoardSide {
    Front,
    Back,
    Both,
}

/// Role assigned to a sketch entity in [`BoardShapeParams::roles`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum BoardEntityRole {
    /// Segment of the single closed outer board perimeter (`Line`, `Arc`, or lone `Circle`).
    Outline,
    /// Segment of an interior closed board cutout (`Line`, `Arc`, or `Circle`).
    Cutout,
    /// Circular drilled hole (`Circle` entity only).
    Hole {
        #[serde(default)]
        plated: bool,
        /// Copper-to-hole-edge clearance in metres (finite, `>= 0.0`).
        #[serde(default)]
        clearance: f64,
    },
    /// Segment of a closed component placement keepout (`Line`, `Arc`, or `Circle`).
    Keepout {
        side: BoardSide,
        /// Maximum component height allowed inside the keepout in metres
        /// (finite, `>= 0.0`; `0.0` forbids components of any height).
        max_height: f64,
    },
}

impl BoardEntityRole {
    /// Canonical lowercase role family name for diagnostics and error payloads.
    pub fn label(&self) -> &'static str {
        match self {
            BoardEntityRole::Outline => "outline",
            BoardEntityRole::Cutout => "cutout",
            BoardEntityRole::Hole { .. } => "hole",
            BoardEntityRole::Keepout { .. } => "keepout",
        }
    }
}

/// Parameters for [`crate::types::Operation::BoardShape`].
///
/// All length fields are stored in **metres**; `thickness_expr` evaluates in
/// working space (**mm**) via `Dimension::Length` and writes metres into
/// `thickness`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct BoardShapeParams {
    /// `Feature.id` of the upstream planar `Operation::Sketch` in the tree.
    pub sketch_id: Uuid,
    /// Board identifier (trimmed text must be non-empty).
    pub board: String,
    /// Substrate thickness in metres (finite, `>= MIN_FEATURE_SIZE`).
    pub thickness: f64,
    /// Optional expression driving `thickness` (`Dimension::Length`, mm → m).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thickness_expr: Option<String>,
    /// Per-entity board role assignments keyed by `SketchEntity::id()`.
    #[cfg_attr(
        feature = "json-schema",
        schemars(with = "std::collections::BTreeMap<String, BoardEntityRole>")
    )]
    #[serde(with = "u32_role_map")]
    pub roles: BTreeMap<u32, BoardEntityRole>,
}

/// Serde adapter for `BTreeMap<u32, BoardEntityRole>`: serializes keys as
/// decimal strings and refuses non-`u32` or duplicate keys loudly.
mod u32_role_map {
    use std::collections::BTreeMap;
    use std::fmt;

    use serde::de::{self, MapAccess, Visitor};
    use serde::ser::SerializeMap;
    use serde::{Deserializer, Serializer};

    use super::BoardEntityRole;

    pub fn serialize<S>(
        map: &BTreeMap<u32, BoardEntityRole>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut out = serializer.serialize_map(Some(map.len()))?;
        for (k, v) in map {
            out.serialize_entry(&k.to_string(), v)?;
        }
        out.end()
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<BTreeMap<u32, BoardEntityRole>, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct RoleMapVisitor;

        impl<'de> Visitor<'de> for RoleMapVisitor {
            type Value = BTreeMap<u32, BoardEntityRole>;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a map of decimal u32 sketch entity IDs to board entity roles")
            }

            fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut map = BTreeMap::new();
                while let Some((raw_key, role)) = access.next_entry::<String, BoardEntityRole>()? {
                    let id = raw_key.parse::<u32>().map_err(|_| {
                        de::Error::custom(format!(
                            "board role key `{raw_key}` is not a valid u32 sketch entity ID"
                        ))
                    })?;
                    if map.insert(id, role).is_some() {
                        return Err(de::Error::custom(format!(
                            "duplicate board role assignment for sketch entity {id}"
                        )));
                    }
                }
                Ok(map)
            }
        }

        deserializer.deserialize_map(RoleMapVisitor)
    }
}

/// Typed, machine-readable validation errors for [`BoardShapeParams`]
/// (`specs/board_shape_feature.md` §4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, thiserror::Error)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BoardShapeError {
    #[error("referenced sketch {sketch_id} did not solve cleanly: {reason}")]
    UnsolvedSketch { sketch_id: Uuid, reason: String },

    #[error("sketch plane frame is invalid: {reason}")]
    InvalidSketchPlane { reason: String },

    #[error("board name must not be empty")]
    EmptyBoardName,

    #[error("board thickness {thickness} m must be finite and at least {MIN_FEATURE_SIZE} m")]
    InvalidThickness { thickness: f64 },

    #[error("sketch has no entity with id {entity_id}")]
    UnknownEntity { entity_id: u32 },

    #[error(
        "sketch entity {entity_id} is construction geometry and cannot carry board role `{role}`"
    )]
    ConstructionEntity { entity_id: u32, role: String },

    #[error("sketch entity {entity_id} of kind `{kind}` cannot carry board role `{role}` (only Line, Arc, and Circle are supported)")]
    UnsupportedEntityKind {
        entity_id: u32,
        kind: String,
        role: String,
    },

    #[error("board hole role on entity {entity_id} requires a Circle, found `{kind}`")]
    HoleNotCircle { entity_id: u32, kind: String },

    #[error(
        "hole on entity {entity_id} has invalid clearance {clearance} m (must be finite and >= 0)"
    )]
    InvalidHoleClearance { entity_id: u32, clearance: f64 },

    #[error("keepout on entity {entity_id} has invalid max_height {max_height} m (must be finite and >= 0)")]
    InvalidKeepoutHeight { entity_id: u32, max_height: f64 },

    #[error("sketch entity {entity_id} is geometrically degenerate: {reason}")]
    DegenerateEntity { entity_id: u32, reason: String },

    #[error("board shape requires at least one `outline` entity forming a closed outer perimeter")]
    MissingOutline,

    #[error("board outline is not closed: open endpoint at [{:.6}, {:.6}] m (nearest open endpoint {:.6e} m away)", at[0], at[1], gap_m)]
    OutlineNotClosed { gap_m: f64, at: [f64; 2] },

    #[error("board outline branches at [{:.6}, {:.6}] m (degree {degree}; expected 2)", at[0], at[1])]
    OutlineBranching { at: [f64; 2], degree: usize },

    #[error("board outline forms {count} separate closed loops; exactly 1 outer outline loop is allowed")]
    MultipleOutlines { count: usize },

    #[error("board `{role}` loop {entity_ids:?} self-intersects or encloses zero area")]
    SelfIntersectingLoop { role: String, entity_ids: Vec<u32> },

    #[error("board cutout is not closed: open endpoint at [{:.6}, {:.6}] m (nearest open endpoint {:.6e} m away)", at[0], at[1], gap_m)]
    CutoutNotClosed { gap_m: f64, at: [f64; 2] },

    #[error("board cutout branches at [{:.6}, {:.6}] m (degree {degree}; expected 2)", at[0], at[1])]
    CutoutBranching { at: [f64; 2], degree: usize },

    #[error("board cutout {entity_ids:?} is not strictly inside the board outline")]
    CutoutOutsideOutline { entity_ids: Vec<u32> },

    #[error(
        "board cutouts {first_entity_ids:?} and {second_entity_ids:?} overlap, touch, or nest"
    )]
    CutoutsOverlapOrNested {
        first_entity_ids: Vec<u32>,
        second_entity_ids: Vec<u32>,
    },

    #[error("board hole on entity {entity_id} is not strictly inside the board outline")]
    HoleOutsideOutline { entity_id: u32 },

    #[error(
        "board hole on entity {hole_entity_id} overlaps or touches cutout {cutout_entity_ids:?}"
    )]
    HoleOverlapsCutout {
        hole_entity_id: u32,
        cutout_entity_ids: Vec<u32>,
    },

    #[error("board holes on entities {first_entity_id} and {second_entity_id} overlap or touch")]
    HolesOverlap {
        first_entity_id: u32,
        second_entity_id: u32,
    },

    #[error("board keepout is not closed: open endpoint at [{:.6}, {:.6}] m (nearest open endpoint {:.6e} m away)", at[0], at[1], gap_m)]
    KeepoutNotClosed { gap_m: f64, at: [f64; 2] },

    #[error("board keepout branches at [{:.6}, {:.6}] m (degree {degree}; expected 2)", at[0], at[1])]
    KeepoutBranching { at: [f64; 2], degree: usize },
}

/// One validated, CCW-oriented 2D curve segment of a board loop in sketch `(u, v)` metres.
#[derive(Debug, Clone, PartialEq)]
pub enum BoardSegment2D {
    Line {
        entity_id: u32,
        stable_id: String,
        start: [f64; 2],
        end: [f64; 2],
    },
    Arc {
        entity_id: u32,
        stable_id: String,
        start: [f64; 2],
        end: [f64; 2],
        center: [f64; 2],
        radius: f64,
        ccw: bool,
    },
    Circle {
        entity_id: u32,
        stable_id: String,
        center: [f64; 2],
        /// Circle radius in metres (`diameter_m = 2.0 * radius`).
        radius: f64,
        /// Exact circle diameter in metres (`2.0 * radius`, `SUITE-SPEC-AGREED.md`).
        diameter_m: f64,
    },
}

/// Validated, CCW-oriented closed 2D board contour (`Outline`, `Cutout`, or `Keepout`).
#[derive(Debug, Clone, PartialEq)]
pub struct EvaluatedBoardLoop {
    /// Ordered sketch entity IDs around the loop.
    pub entity_ids: Vec<u32>,
    /// Parallel stable element IDs (`waffle:<sketch_id>/<entity_id>`).
    pub stable_ids: Vec<String>,
    /// Ordered 2D segments in sketch `(u, v)` metres, oriented counter-clockwise.
    pub segments: Vec<BoardSegment2D>,
    /// Exact analytical enclosed area in square metres (`> 0`).
    pub area_m2: f64,
}

/// Validated circular drilled board hole (`SUITE-SPEC-AGREED.md` diameter-aware).
#[derive(Debug, Clone, PartialEq)]
pub struct EvaluatedBoardHole {
    pub entity_id: u32,
    /// Stable element ID (`waffle:<sketch_id>/<entity_id>`).
    pub stable_id: String,
    /// Deterministic `PartConnector::feature_id` UUID for assembly addressing.
    pub connector_id: Uuid,
    pub center_uv: [f64; 2],
    pub radius_m: f64,
    /// Exact hole diameter in metres (`2.0 * radius_m`).
    pub diameter_m: f64,
    pub plated: bool,
    pub clearance_m: f64,
    /// Top-face (`+normal`) mate connector frame in Part coordinates.
    pub frame: Frame,
}

/// Validated closed 2D component height keepout zone.
#[derive(Debug, Clone, PartialEq)]
pub struct EvaluatedBoardKeepout {
    pub entity_ids: Vec<u32>,
    pub stable_ids: Vec<String>,
    pub loop_2d: EvaluatedBoardLoop,
    pub side: BoardSide,
    pub max_height_m: f64,
}

/// Full analytical evaluation of an [`crate::types::Operation::BoardShape`] feature.
#[derive(Debug, Clone, PartialEq)]
pub struct EvaluatedBoardShape {
    pub board: String,
    pub sketch_id: Uuid,
    pub thickness_m: f64,
    pub outline: EvaluatedBoardLoop,
    pub cutouts: Vec<EvaluatedBoardLoop>,
    pub holes: Vec<EvaluatedBoardHole>,
    pub keepouts: Vec<EvaluatedBoardKeepout>,
    /// Top-face hole mate connectors keyed by stable element ID (`waffle:<sketch_id>/<entity_id>`).
    pub connectors: BTreeMap<String, PartConnector>,
}

/// Canonical stable element ID for a Waffle-authored board sketch entity:
/// `waffle:<sketch_feature_uuid>/<entity_id>`.
pub fn element_stable_id(sketch_id: Uuid, entity_id: u32) -> String {
    format!("waffle:{sketch_id}/{entity_id}")
}

/// Deterministic 128-bit FNV-1a UUID for a board hole's [`PartConnector::feature_id`],
/// derived from `(board_shape_feature_id, stable_id)` so multiple holes on one
/// `BoardShape` feature have distinct, rebuild-stable assembly connector IDs.
pub fn hole_connector_uuid(board_feature_id: Uuid, stable_id: &str) -> Uuid {
    Uuid::from_u128(crate::fnv1a_128(
        board_feature_id,
        ["hole-connector", stable_id],
    ))
}

// ── Validated raw primitives before chaining ─────────────────────────────────

#[derive(Debug, Clone)]
enum RawEntity {
    Line {
        entity_id: u32,
        start_id: u32,
        end_id: u32,
        start: [f64; 2],
        end: [f64; 2],
    },
    Arc {
        entity_id: u32,
        start_id: u32,
        end_id: u32,
        start: [f64; 2],
        end: [f64; 2],
        center: [f64; 2],
    },
    Circle {
        entity_id: u32,
        center: [f64; 2],
        radius: f64,
    },
}

#[derive(Debug, Clone, Copy)]
enum LoopKind {
    Outline,
    Cutout,
    Keepout,
}

impl LoopKind {
    fn label(self) -> &'static str {
        match self {
            LoopKind::Outline => "outline",
            LoopKind::Cutout => "cutout",
            LoopKind::Keepout => "keepout",
        }
    }

    fn not_closed(self, gap_m: f64, at: [f64; 2]) -> BoardShapeError {
        match self {
            LoopKind::Outline => BoardShapeError::OutlineNotClosed { gap_m, at },
            LoopKind::Cutout => BoardShapeError::CutoutNotClosed { gap_m, at },
            LoopKind::Keepout => BoardShapeError::KeepoutNotClosed { gap_m, at },
        }
    }

    fn branching(self, at: [f64; 2], degree: usize) -> BoardShapeError {
        match self {
            LoopKind::Outline => BoardShapeError::OutlineBranching { at, degree },
            LoopKind::Cutout => BoardShapeError::CutoutBranching { at, degree },
            LoopKind::Keepout => BoardShapeError::KeepoutBranching { at, degree },
        }
    }
}

/// Analytically validate [`BoardShapeParams`] against `sketch` and compute the
/// board loops, holes, keepouts, exact areas, and top-face hole mate connectors.
///
/// Runs before any kernel call so invalid role assignments fail loudly with
/// their dedicated [`BoardShapeError`] variant and never leave partial solids.
pub fn evaluate_board_shape(
    feature_id: Uuid,
    params: &BoardShapeParams,
    sketch: &Sketch,
) -> Result<EvaluatedBoardShape, BoardShapeError> {
    // B2: upstream sketch solve status.
    match &sketch.solve_status {
        SolveStatus::OverConstrained { conflicts } => {
            return Err(BoardShapeError::UnsolvedSketch {
                sketch_id: params.sketch_id,
                reason: format!("over-constrained (conflicts: {conflicts:?})"),
            });
        }
        SolveStatus::SolveFailed { reason } => {
            return Err(BoardShapeError::UnsolvedSketch {
                sketch_id: params.sketch_id,
                reason: reason.clone(),
            });
        }
        SolveStatus::Unsolved
        | SolveStatus::FullyConstrained
        | SolveStatus::UnderConstrained { .. } => {}
    }

    // B3: sketch plane frame validity.
    if !sketch.plane_origin.iter().all(|c| c.is_finite()) {
        return Err(BoardShapeError::InvalidSketchPlane {
            reason: "sketch plane origin contains non-finite coordinates".to_string(),
        });
    }
    if !sketch.plane_normal.iter().all(|c| c.is_finite()) {
        return Err(BoardShapeError::InvalidSketchPlane {
            reason: "sketch plane normal contains non-finite coordinates".to_string(),
        });
    }
    let normal_len = norm3(sketch.plane_normal);
    if normal_len < TAU_WORK {
        return Err(BoardShapeError::InvalidSketchPlane {
            reason: "sketch plane normal is zero-length".to_string(),
        });
    }
    let normal = unit_normal(sketch.plane_normal);
    if let Some(x) = sketch.plane_x_axis {
        if !SketchPlaneBasis::x_axis_is_usable(normal, x) {
            return Err(BoardShapeError::InvalidSketchPlane {
                reason: format!(
                    "sketch plane x_axis {x:?} is non-finite, zero-length, or parallel to normal {normal:?}"
                ),
            });
        }
    }
    let x_axis = sketch_x_axis(sketch);
    let y_axis = cross3(normal, x_axis);

    // B4 & B5: board name and substrate thickness.
    let board_name = params.board.trim();
    if board_name.is_empty() {
        return Err(BoardShapeError::EmptyBoardName);
    }
    if !params.thickness.is_finite() || params.thickness < MIN_FEATURE_SIZE {
        return Err(BoardShapeError::InvalidThickness {
            thickness: params.thickness,
        });
    }

    // Point lookup: Point entities overridden by solved_positions when present.
    let mut point_coords: HashMap<u32, [f64; 2]> = HashMap::new();
    let mut entity_by_id: HashMap<u32, &SketchEntity> = HashMap::new();
    for entity in &sketch.entities {
        entity_by_id.insert(entity.id(), entity);
        if let SketchEntity::Point { id, x, y, .. } = entity {
            point_coords.insert(*id, [*x, *y]);
        }
    }
    for (&id, &(u, v)) in &sketch.solved_positions {
        point_coords.insert(id, [u, v]);
    }

    // B6–B12: per-entity role and primitive validation.
    let mut outline_raw: Vec<RawEntity> = Vec::new();
    let mut cutout_raw: Vec<RawEntity> = Vec::new();
    let mut hole_specs: Vec<(u32, [f64; 2], f64, bool, f64)> = Vec::new();
    let mut keepout_groups: BTreeMap<(BoardSide, u64), (f64, Vec<RawEntity>)> = BTreeMap::new();

    for (&entity_id, role) in &params.roles {
        let Some(&entity) = entity_by_id.get(&entity_id) else {
            return Err(BoardShapeError::UnknownEntity { entity_id });
        };
        if entity.is_construction() {
            return Err(BoardShapeError::ConstructionEntity {
                entity_id,
                role: role.label().to_string(),
            });
        }
        match entity {
            SketchEntity::Point { .. } => {
                return Err(BoardShapeError::UnsupportedEntityKind {
                    entity_id,
                    kind: "Point".to_string(),
                    role: role.label().to_string(),
                });
            }
            SketchEntity::Spline { .. } => {
                return Err(BoardShapeError::UnsupportedEntityKind {
                    entity_id,
                    kind: "Spline".to_string(),
                    role: role.label().to_string(),
                });
            }
            SketchEntity::Gear { .. } => {
                return Err(BoardShapeError::UnsupportedEntityKind {
                    entity_id,
                    kind: "Gear".to_string(),
                    role: role.label().to_string(),
                });
            }
            SketchEntity::Sprocket { .. } => {
                return Err(BoardShapeError::UnsupportedEntityKind {
                    entity_id,
                    kind: "Sprocket".to_string(),
                    role: role.label().to_string(),
                });
            }
            SketchEntity::Line { .. } | SketchEntity::Arc { .. } | SketchEntity::Circle { .. } => {}
        }

        match role {
            BoardEntityRole::Hole { plated, clearance } => {
                match entity {
                    SketchEntity::Line { .. } => {
                        return Err(BoardShapeError::HoleNotCircle {
                            entity_id,
                            kind: "Line".to_string(),
                        });
                    }
                    SketchEntity::Arc { .. } => {
                        return Err(BoardShapeError::HoleNotCircle {
                            entity_id,
                            kind: "Arc".to_string(),
                        });
                    }
                    SketchEntity::Circle { .. } => {}
                    _ => unreachable!("filtered above"),
                }
                if !clearance.is_finite() || *clearance < 0.0 {
                    return Err(BoardShapeError::InvalidHoleClearance {
                        entity_id,
                        clearance: *clearance,
                    });
                }
                let raw = validate_raw_entity(entity, &point_coords)?;
                if let RawEntity::Circle { center, radius, .. } = raw {
                    hole_specs.push((entity_id, center, radius, *plated, *clearance));
                }
            }
            BoardEntityRole::Outline => {
                outline_raw.push(validate_raw_entity(entity, &point_coords)?);
            }
            BoardEntityRole::Cutout => {
                cutout_raw.push(validate_raw_entity(entity, &point_coords)?);
            }
            BoardEntityRole::Keepout { side, max_height } => {
                if !max_height.is_finite() || *max_height < 0.0 {
                    return Err(BoardShapeError::InvalidKeepoutHeight {
                        entity_id,
                        max_height: *max_height,
                    });
                }
                let raw = validate_raw_entity(entity, &point_coords)?;
                let canon_bits = (*max_height + 0.0).to_bits();
                keepout_groups
                    .entry((*side, canon_bits))
                    .or_insert_with(|| (*max_height + 0.0, Vec::new()))
                    .1
                    .push(raw);
            }
        }
    }

    // B13–B17: build and validate the single closed outer outline loop.
    if outline_raw.is_empty() {
        return Err(BoardShapeError::MissingOutline);
    }
    let mut outline_loops =
        chain_and_validate_loops(params.sketch_id, &outline_raw, LoopKind::Outline)?;
    if outline_loops.len() != 1 {
        return Err(BoardShapeError::MultipleOutlines {
            count: outline_loops.len(),
        });
    }
    let outline = outline_loops.remove(0);

    // B17–B19: build and validate cutout loops.
    let cutouts = if cutout_raw.is_empty() {
        Vec::new()
    } else {
        chain_and_validate_loops(params.sketch_id, &cutout_raw, LoopKind::Cutout)?
    };

    // B20: every cutout must lie strictly inside the outline.
    for cutout in &cutouts {
        if loop_loop_distance(cutout, &outline) <= TAU_MODEL {
            return Err(BoardShapeError::CutoutOutsideOutline {
                entity_ids: cutout.entity_ids.clone(),
            });
        }
        let witness_cutout = sample_boundary_point(cutout);
        let witness_outline = sample_boundary_point(&outline);
        if !point_in_loop(witness_cutout, &outline) || point_in_loop(witness_outline, cutout) {
            return Err(BoardShapeError::CutoutOutsideOutline {
                entity_ids: cutout.entity_ids.clone(),
            });
        }
    }

    // B21: cutouts must be pairwise disjoint and un-nested.
    for i in 0..cutouts.len() {
        for j in (i + 1)..cutouts.len() {
            let ca = &cutouts[i];
            let cb = &cutouts[j];
            if loop_loop_distance(ca, cb) <= TAU_MODEL {
                return Err(BoardShapeError::CutoutsOverlapOrNested {
                    first_entity_ids: ca.entity_ids.clone(),
                    second_entity_ids: cb.entity_ids.clone(),
                });
            }
            if point_in_loop(sample_boundary_point(ca), cb)
                || point_in_loop(sample_boundary_point(cb), ca)
            {
                return Err(BoardShapeError::CutoutsOverlapOrNested {
                    first_entity_ids: ca.entity_ids.clone(),
                    second_entity_ids: cb.entity_ids.clone(),
                });
            }
        }
    }

    // B22–B24: holes strictly inside outline, disjoint from cutouts, pairwise disjoint.
    for &(entity_id, center, radius, _, _) in &hole_specs {
        if point_to_loop_distance(center, &outline) <= radius + TAU_MODEL
            || !point_in_loop(center, &outline)
        {
            return Err(BoardShapeError::HoleOutsideOutline { entity_id });
        }
        for cutout in &cutouts {
            if point_to_loop_distance(center, cutout) <= radius + TAU_MODEL
                || point_in_loop(center, cutout)
            {
                return Err(BoardShapeError::HoleOverlapsCutout {
                    hole_entity_id: entity_id,
                    cutout_entity_ids: cutout.entity_ids.clone(),
                });
            }
        }
    }
    for i in 0..hole_specs.len() {
        for j in (i + 1)..hole_specs.len() {
            let (id_a, ca, ra, _, _) = hole_specs[i];
            let (id_b, cb, rb, _, _) = hole_specs[j];
            if dist2(ca, cb) <= ra + rb + TAU_MODEL {
                return Err(BoardShapeError::HolesOverlap {
                    first_entity_id: id_a,
                    second_entity_id: id_b,
                });
            }
        }
    }

    // B17, B25–B26: keepout loops (grouped by `(side, max_height)`).
    let mut keepouts = Vec::new();
    for ((side, _), (max_height_m, group_raw)) in keepout_groups {
        let loops = chain_and_validate_loops(params.sketch_id, &group_raw, LoopKind::Keepout)?;
        for loop_2d in loops {
            keepouts.push(EvaluatedBoardKeepout {
                entity_ids: loop_2d.entity_ids.clone(),
                stable_ids: loop_2d.stable_ids.clone(),
                loop_2d,
                side,
                max_height_m,
            });
        }
    }
    keepouts.sort_by_key(|k| (k.entity_ids[0], k.side));

    // Construct top-face hole records and PartConnectors in deterministic entity_id order.
    let mut holes = Vec::with_capacity(hole_specs.len());
    let mut connectors = BTreeMap::new();
    for (entity_id, center_uv, radius_m, plated, clearance_m) in hole_specs {
        let stable_id = element_stable_id(params.sketch_id, entity_id);
        let connector_id = hole_connector_uuid(feature_id, &stable_id);
        let origin = [
            sketch.plane_origin[0]
                + center_uv[0] * x_axis[0]
                + center_uv[1] * y_axis[0]
                + params.thickness * normal[0],
            sketch.plane_origin[1]
                + center_uv[0] * x_axis[1]
                + center_uv[1] * y_axis[1]
                + params.thickness * normal[1],
            sketch.plane_origin[2]
                + center_uv[0] * x_axis[2]
                + center_uv[1] * y_axis[2]
                + params.thickness * normal[2],
        ];
        let frame = Frame {
            origin,
            z_axis: normal,
            x_axis,
        };
        let connector = PartConnector {
            feature_id: connector_id,
            name: format!("{board_name} hole {entity_id}"),
            key: Some(stable_id.clone()),
            frame,
            geometry: Some(ConnectorGeometry::CircularEdge(AxisKind::Cylindrical)),
        };
        connectors.insert(stable_id.clone(), connector);
        holes.push(EvaluatedBoardHole {
            entity_id,
            stable_id,
            connector_id,
            center_uv,
            radius_m,
            diameter_m: 2.0 * radius_m,
            plated,
            clearance_m,
            frame,
        });
    }

    Ok(EvaluatedBoardShape {
        board: board_name.to_string(),
        sketch_id: params.sketch_id,
        thickness_m: params.thickness,
        outline,
        cutouts,
        holes,
        keepouts,
        connectors,
    })
}

/// Execute an [`crate::types::Operation::BoardShape`] feature on the kernel bundle.
pub fn execute(
    feature: &Feature,
    params: &BoardShapeParams,
    kb: &mut dyn KernelBundle,
    feature_results: &HashMap<Uuid, OpResult>,
    tree: &FeatureTree,
    context: Option<&EditContext>,
) -> Result<(OpResult, EvaluatedBoardShape), EngineError> {
    let sketch = active_sketch(params.sketch_id, feature_results, tree, kb, context)?;
    let evaluated = evaluate_board_shape(feature.id, params, &sketch)?;

    let origin = sketch.plane_origin;
    let normal = unit_normal(sketch.plane_normal);
    let x_axis = sketch_x_axis(&sketch);

    let outer_seed = ConstructSeed::from_u128(crate::fnv1a_128(
        feature.id,
        std::iter::once("outline").chain(evaluated.outline.stable_ids.iter().map(String::as_str)),
    ));
    let saved_seed = kb.set_construct_seed(Some(outer_seed));
    let build_result = build_board_solid(feature.id, &evaluated, kb, origin, normal, x_axis);
    kb.set_construct_seed(saved_seed);

    let (board_handle, warnings) = build_result?;

    let after = diff::snapshot(kb.as_introspect(), &board_handle);
    let empty_before = diff::TopoSnapshot {
        faces: Vec::new(),
        edges: Vec::new(),
        vertices: Vec::new(),
    };
    let diff_result = diff::diff(&empty_before, &after);
    let role_assignments = assign_board_roles(kb.as_introspect(), &board_handle, normal);

    let op_result = OpResult {
        outputs: vec![(
            OutputKey::Main,
            BodyOutput {
                handle: board_handle,
                mesh: None,
                edges: None,
            },
        )],
        provenance: Provenance {
            created: diff_result.created,
            deleted: Vec::new(),
            modified: Vec::new(),
            role_assignments,
        },
        diagnostics: Diagnostics {
            warnings,
            ..Diagnostics::default()
        },
    };

    Ok((op_result, evaluated))
}

#[derive(Debug, Clone, Copy)]
struct PlaneExtrude {
    origin: [f64; 3],
    normal: [f64; 3],
    x_axis: [f64; 3],
    depth: f64,
}

fn build_board_solid(
    feature_id: Uuid,
    evaluated: &EvaluatedBoardShape,
    kb: &mut dyn KernelBundle,
    origin: [f64; 3],
    normal: [f64; 3],
    x_axis: [f64; 3],
) -> Result<(KernelSolidHandle, Vec<String>), EngineError> {
    let t = evaluated.thickness_m;
    let base_frame = PlaneExtrude {
        origin,
        normal,
        x_axis,
        depth: t,
    };
    let base_res = extrude_board_loop(kb, &evaluated.outline, base_frame)?;
    let mut warnings = base_res.diagnostics.warnings;
    let mut board_handle = base_res
        .outputs
        .into_iter()
        .next()
        .ok_or_else(|| EngineError::RebuildFailed {
            feature_name: evaluated.board.clone(),
            reason: "board outline extrude produced no solid".to_string(),
        })?
        .1
        .handle;

    // Symmetric through-cutter frame: [-t, +2t] avoids coplanar caps at 0 and +t
    // (Yang Stage-0 coplanar cap refusal, `kicad.rs:451-457`).
    let cut_frame = PlaneExtrude {
        origin: [
            origin[0] - t * normal[0],
            origin[1] - t * normal[1],
            origin[2] - t * normal[2],
        ],
        normal,
        x_axis,
        depth: 3.0 * t,
    };

    for cutout in &evaluated.cutouts {
        let seed = ConstructSeed::from_u128(crate::fnv1a_128(
            feature_id,
            std::iter::once("cutout").chain(cutout.stable_ids.iter().map(String::as_str)),
        ));
        kb.set_construct_seed(Some(seed));
        let tool_res = extrude_board_loop(kb, cutout, cut_frame)?;
        let tool_handle = tool_res
            .outputs
            .into_iter()
            .next()
            .ok_or_else(|| EngineError::RebuildFailed {
                feature_name: evaluated.board.clone(),
                reason: "board cutout extrude produced no tool solid".to_string(),
            })?
            .1
            .handle;
        let sub_res = execute_boolean(kb, &board_handle, &tool_handle, BooleanKind::Subtract)?;
        warnings.extend(sub_res.diagnostics.warnings);
        board_handle = sub_res
            .outputs
            .into_iter()
            .next()
            .ok_or_else(|| EngineError::RebuildFailed {
                feature_name: evaluated.board.clone(),
                reason: format!(
                    "board cutout {:?} consumed the entire board",
                    cutout.entity_ids
                ),
            })?
            .1
            .handle;
    }

    for hole in &evaluated.holes {
        let seed = ConstructSeed::from_u128(crate::fnv1a_128(
            feature_id,
            ["hole", hole.stable_id.as_str()],
        ));
        kb.set_construct_seed(Some(seed));
        let tool_res =
            extrude_circle(kb, hole.entity_id, hole.center_uv, hole.radius_m, cut_frame)?;
        let tool_handle = tool_res
            .outputs
            .into_iter()
            .next()
            .ok_or_else(|| EngineError::RebuildFailed {
                feature_name: evaluated.board.clone(),
                reason: format!(
                    "board hole {} extrude produced no tool solid",
                    hole.entity_id
                ),
            })?
            .1
            .handle;
        let sub_res = execute_boolean(kb, &board_handle, &tool_handle, BooleanKind::Subtract)?;
        warnings.extend(sub_res.diagnostics.warnings);
        board_handle = sub_res
            .outputs
            .into_iter()
            .next()
            .ok_or_else(|| EngineError::RebuildFailed {
                feature_name: evaluated.board.clone(),
                reason: format!("board hole {} consumed the entire board", hole.entity_id),
            })?
            .1
            .handle;
    }

    Ok((board_handle, warnings))
}

fn extrude_circle(
    kb: &mut dyn KernelBundle,
    entity_id: u32,
    center_uv: [f64; 2],
    radius: f64,
    frame: PlaneExtrude,
) -> Result<OpResult, EngineError> {
    let profile = ClosedProfile {
        entity_ids: vec![entity_id],
        is_outer: true,
        vertex_ids: Vec::new(),
        circle: Some(CircleProfile {
            center_u: center_uv[0],
            center_v: center_uv[1],
            radius,
        }),
        spline_segments: Vec::new(),
        arc_segments: Vec::new(),
    };
    let face_ids = kb.make_faces_from_profiles(
        &[profile],
        frame.origin,
        frame.normal,
        frame.x_axis,
        &HashMap::new(),
    )?;
    let face_id = face_ids
        .into_iter()
        .next()
        .ok_or(EngineError::ProfileOutOfRange { index: 0, count: 0 })?;
    Ok(execute_extrude(
        kb,
        face_id,
        frame.normal,
        frame.depth,
        None,
    )?)
}

fn extrude_board_loop(
    kb: &mut dyn KernelBundle,
    loop_2d: &EvaluatedBoardLoop,
    frame: PlaneExtrude,
) -> Result<OpResult, EngineError> {
    if let [BoardSegment2D::Circle {
        entity_id,
        center,
        radius,
        ..
    }] = loop_2d.segments.as_slice()
    {
        return extrude_circle(kb, *entity_id, *center, *radius, frame);
    }

    let region = loop_to_region(loop_2d);
    let face_id = kb.make_face_from_region(&region, frame.origin, frame.normal, frame.x_axis)?;
    Ok(execute_extrude(
        kb,
        face_id,
        frame.normal,
        frame.depth,
        None,
    )?)
}

/// Convert a validated CCW line/arc loop into a [`Region`] with exact minor
/// [`RegionEdge::Arc`] segments (each `sweep <= PI / 2`) for `KernelV2Adapter`
/// plus a dense `outer` polygon for `MockKernel`.
fn loop_to_region(loop_2d: &EvaluatedBoardLoop) -> Region {
    let mut outer_edges = Vec::new();
    let mut outer = Vec::new();
    for seg in &loop_2d.segments {
        match *seg {
            BoardSegment2D::Line { start, end, .. } => {
                outer.push((start[0], start[1]));
                outer_edges.push(RegionEdge::Line {
                    a: (start[0], start[1]),
                    b: (end[0], end[1]),
                });
            }
            BoardSegment2D::Arc {
                start,
                end,
                center,
                radius,
                ccw,
                ..
            } => {
                let sweep = directed_arc_sweep(center, start, end, ccw);
                // Split into sub-arcs of sweep <= PI / 2 so every `RegionEdge::Arc`
                // strictly satisfies `kernel-v2`'s minor-arc (`sweep < PI`) gate.
                let pieces = ((sweep / (0.5 * PI)).ceil() as usize).max(1);
                let start_angle = (start[1] - center[1]).atan2(start[0] - center[0]);
                let signed_total = if ccw { sweep } else { -sweep };
                let mut prev = start;
                for k in 1..=pieces {
                    let next = if k == pieces {
                        end
                    } else {
                        let a = start_angle + signed_total * (k as f64 / pieces as f64);
                        [center[0] + radius * a.cos(), center[1] + radius * a.sin()]
                    };
                    let a0 = (prev[1] - center[1]).atan2(prev[0] - center[0]);
                    let sub_sweep = directed_arc_sweep(center, prev, next, ccw);
                    let signed_sub = if ccw { sub_sweep } else { -sub_sweep };
                    outer.push((prev[0], prev[1]));
                    for s in 1..ARC_SAMPLES {
                        let ang = a0 + signed_sub * (s as f64 / ARC_SAMPLES as f64);
                        outer.push((
                            center[0] + radius * ang.cos(),
                            center[1] + radius * ang.sin(),
                        ));
                    }
                    outer_edges.push(RegionEdge::Arc {
                        a: (prev[0], prev[1]),
                        b: (next[0], next[1]),
                        center: (center[0], center[1]),
                        radius,
                        ccw,
                    });
                    prev = next;
                }
            }
            BoardSegment2D::Circle { .. } => {
                unreachable!("standalone Circle handled via CircleProfile before loop_to_region")
            }
        }
    }

    Region {
        outer,
        holes: Vec::new(),
        area: loop_2d.area_m2,
        profile_entity_ids: Some(loop_2d.entity_ids.clone()),
        outer_edges,
        hole_edges: Vec::new(),
        boundary_entity_ids: Some(loop_2d.entity_ids.clone()),
    }
}

fn assign_board_roles(
    introspect: &dyn KernelIntrospect,
    solid: &KernelSolidHandle,
    normal: [f64; 3],
) -> Vec<(KernelId, Role)> {
    let faces = introspect.list_faces(solid);
    let mut assignments = Vec::with_capacity(faces.len());
    let mut side_index = 0usize;
    for face_id in faces {
        let sig = introspect.compute_signature(face_id, TopoKind::Face);
        let dot = sig
            .normal
            .filter(|_| sig.surface_type.as_deref() == Some("planar"))
            .map(|n| n[0] * normal[0] + n[1] * normal[1] + n[2] * normal[2])
            .unwrap_or(0.0);
        if dot > COS_FACE_NORMAL_ALIGNMENT {
            assignments.push((face_id, Role::EndCapPositive));
        } else if dot < -COS_FACE_NORMAL_ALIGNMENT {
            assignments.push((face_id, Role::EndCapNegative));
        } else {
            assignments.push((face_id, Role::SideFace { index: side_index }));
            side_index += 1;
        }
    }
    assignments
}

// ── Primitive validation & loop chaining ─────────────────────────────────────

fn lookup_finite_point(
    entity_id: u32,
    point_id: u32,
    label: &str,
    point_coords: &HashMap<u32, [f64; 2]>,
) -> Result<[f64; 2], BoardShapeError> {
    let Some(&p) = point_coords.get(&point_id) else {
        return Err(BoardShapeError::DegenerateEntity {
            entity_id,
            reason: format!("{label} point {point_id} does not exist in sketch"),
        });
    };
    if !p[0].is_finite() || !p[1].is_finite() {
        return Err(BoardShapeError::DegenerateEntity {
            entity_id,
            reason: format!("{label} point {point_id} has non-finite coordinates"),
        });
    }
    Ok(p)
}

fn validate_raw_entity(
    entity: &SketchEntity,
    point_coords: &HashMap<u32, [f64; 2]>,
) -> Result<RawEntity, BoardShapeError> {
    match entity {
        SketchEntity::Line {
            id,
            start_id,
            end_id,
            ..
        } => {
            let start = lookup_finite_point(*id, *start_id, "start", point_coords)?;
            let end = lookup_finite_point(*id, *end_id, "end", point_coords)?;
            let len = dist2(start, end);
            if !len.is_finite() || len <= TAU_MODEL {
                return Err(BoardShapeError::DegenerateEntity {
                    entity_id: *id,
                    reason: format!("line length {len:.3e} m is at or below {TAU_MODEL:.1e} m"),
                });
            }
            Ok(RawEntity::Line {
                entity_id: *id,
                start_id: *start_id,
                end_id: *end_id,
                start,
                end,
            })
        }
        SketchEntity::Arc {
            id,
            center_id,
            start_id,
            end_id,
            ..
        } => {
            let center = lookup_finite_point(*id, *center_id, "center", point_coords)?;
            let start = lookup_finite_point(*id, *start_id, "start", point_coords)?;
            let end = lookup_finite_point(*id, *end_id, "end", point_coords)?;
            let r_start = dist2(start, center);
            let r_end = dist2(end, center);
            let min_radius = 0.5 * MIN_FEATURE_SIZE;
            if !r_start.is_finite()
                || !r_end.is_finite()
                || r_start < min_radius
                || r_end < min_radius
            {
                return Err(BoardShapeError::DegenerateEntity {
                    entity_id: *id,
                    reason: format!(
                        "arc radius ({r_start:.3e} m, {r_end:.3e} m) is below {min_radius:.1e} m"
                    ),
                });
            }
            if (r_start - r_end).abs() > MIN_FEATURE_SIZE {
                return Err(BoardShapeError::DegenerateEntity {
                    entity_id: *id,
                    reason: format!(
                        "arc start radius {r_start:.6e} m and end radius {r_end:.6e} m differ by {:.3e} m",
                        (r_start - r_end).abs()
                    ),
                });
            }
            if start_id == end_id || dist2(start, end) <= TAU_MODEL {
                return Err(BoardShapeError::DegenerateEntity {
                    entity_id: *id,
                    reason: "arc start and end points coincide (use Circle for full circles)"
                        .to_string(),
                });
            }
            Ok(RawEntity::Arc {
                entity_id: *id,
                start_id: *start_id,
                end_id: *end_id,
                start,
                end,
                center,
            })
        }
        SketchEntity::Circle {
            id,
            center_id,
            radius,
            ..
        } => {
            let center = lookup_finite_point(*id, *center_id, "center", point_coords)?;
            let min_radius = 0.5 * MIN_FEATURE_SIZE;
            if !radius.is_finite() || *radius < min_radius {
                return Err(BoardShapeError::DegenerateEntity {
                    entity_id: *id,
                    reason: format!("circle radius {radius:.3e} m is below {min_radius:.1e} m"),
                });
            }
            Ok(RawEntity::Circle {
                entity_id: *id,
                center,
                radius: *radius,
            })
        }
        _ => unreachable!("non-curve entities rejected before validate_raw_entity"),
    }
}

#[derive(Debug, Clone)]
struct OpenCurve {
    entity_id: u32,
    start_cluster: usize,
    end_cluster: usize,
    arc_center: Option<[f64; 2]>,
}

fn chain_and_validate_loops(
    sketch_id: Uuid,
    raw_entities: &[RawEntity],
    kind: LoopKind,
) -> Result<Vec<EvaluatedBoardLoop>, BoardShapeError> {
    let mut loops = Vec::new();
    let mut open_raw = Vec::new();

    for raw in raw_entities {
        match *raw {
            RawEntity::Circle {
                entity_id,
                center,
                radius,
            } => {
                let stable_id = element_stable_id(sketch_id, entity_id);
                let area_m2 = PI * radius * radius;
                loops.push(EvaluatedBoardLoop {
                    entity_ids: vec![entity_id],
                    stable_ids: vec![stable_id.clone()],
                    segments: vec![BoardSegment2D::Circle {
                        entity_id,
                        stable_id,
                        center,
                        radius,
                        diameter_m: 2.0 * radius,
                    }],
                    area_m2,
                });
            }
            RawEntity::Line { .. } | RawEntity::Arc { .. } => open_raw.push(raw.clone()),
        }
    }

    if !open_raw.is_empty() {
        let chained = chain_open_curves(sketch_id, &open_raw, kind)?;
        loops.extend(chained);
    }

    loops.sort_by_key(|l| l.entity_ids[0]);
    Ok(loops)
}

fn chain_open_curves(
    sketch_id: Uuid,
    open_raw: &[RawEntity],
    kind: LoopKind,
) -> Result<Vec<EvaluatedBoardLoop>, BoardShapeError> {
    // Collect all endpoint occurrences: (entity_idx, is_end, point_id, uv).
    let mut endpoints: Vec<(u32, [f64; 2])> = Vec::with_capacity(open_raw.len() * 2);
    for raw in open_raw {
        match *raw {
            RawEntity::Line {
                start_id,
                end_id,
                start,
                end,
                ..
            }
            | RawEntity::Arc {
                start_id,
                end_id,
                start,
                end,
                ..
            } => {
                endpoints.push((start_id, start));
                endpoints.push((end_id, end));
            }
            RawEntity::Circle { .. } => unreachable!(),
        }
    }

    let m = endpoints.len();
    let mut parent: Vec<usize> = (0..m).collect();
    fn find(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }
    let union = |parent: &mut [usize], a: usize, b: usize| {
        let ra = find(parent, a);
        let rb = find(parent, b);
        if ra != rb {
            if ra < rb {
                parent[rb] = ra;
            } else {
                parent[ra] = rb;
            }
        }
    };

    for i in 0..m {
        for j in (i + 1)..m {
            // Never weld the two ends of the same entity.
            if i / 2 == j / 2 {
                continue;
            }
            let (pid_i, uv_i) = endpoints[i];
            let (pid_j, uv_j) = endpoints[j];
            let same_pid = pid_i == pid_j;
            let coord_dist = dist2(uv_i, uv_j);
            if same_pid || coord_dist <= TAU_MODEL {
                union(&mut parent, i, j);
            }
        }
    }

    // Compact union-find roots into dense cluster IDs with canonical coordinates
    // chosen from the lowest point_id in each cluster for bit-determinism.
    let mut root_to_cluster: HashMap<usize, usize> = HashMap::new();
    let mut cluster_best_pid: Vec<u32> = Vec::new();
    let mut cluster_uv: Vec<[f64; 2]> = Vec::new();
    let mut endpoint_cluster = vec![0usize; m];

    for i in 0..m {
        let r = find(&mut parent, i);
        let (pid, uv) = endpoints[i];
        if let Some(&cid) = root_to_cluster.get(&r) {
            if pid < cluster_best_pid[cid] {
                cluster_best_pid[cid] = pid;
                cluster_uv[cid] = uv;
            }
            endpoint_cluster[i] = cid;
        } else {
            let cid = cluster_uv.len();
            root_to_cluster.insert(r, cid);
            cluster_best_pid.push(pid);
            cluster_uv.push(uv);
            endpoint_cluster[i] = cid;
        }
    }

    let num_clusters = cluster_uv.len();
    let mut degree = vec![0usize; num_clusters];
    let mut incident: Vec<Vec<usize>> = vec![Vec::new(); num_clusters];
    let mut curves: Vec<OpenCurve> = Vec::with_capacity(open_raw.len());

    for (idx, raw) in open_raw.iter().enumerate() {
        let sc = endpoint_cluster[2 * idx];
        let ec = endpoint_cluster[2 * idx + 1];
        let (entity_id, arc_center) = match *raw {
            RawEntity::Line { entity_id, .. } => (entity_id, None),
            RawEntity::Arc {
                entity_id, center, ..
            } => (entity_id, Some(center)),
            RawEntity::Circle { .. } => unreachable!(),
        };
        if sc == ec || dist2(cluster_uv[sc], cluster_uv[ec]) <= TAU_MODEL {
            return Err(BoardShapeError::DegenerateEntity {
                entity_id,
                reason: "endpoints collapsed onto the same welded vertex".to_string(),
            });
        }
        degree[sc] += 1;
        degree[ec] += 1;
        incident[sc].push(idx);
        incident[ec].push(idx);
        curves.push(OpenCurve {
            entity_id,
            start_cluster: sc,
            end_cluster: ec,
            arc_center,
        });
    }

    // Check open endpoints (degree == 1) before branching (degree > 2).
    let open_clusters: Vec<usize> = (0..num_clusters).filter(|&c| degree[c] == 1).collect();
    if let Some(&first_open) = open_clusters.first() {
        let at = cluster_uv[first_open];
        let gap_m = open_clusters
            .iter()
            .skip(1)
            .map(|&other| dist2(at, cluster_uv[other]))
            .fold(f64::INFINITY, f64::min);
        return Err(kind.not_closed(if gap_m.is_finite() { gap_m } else { 0.0 }, at));
    }

    for c in 0..num_clusters {
        if degree[c] > 2 {
            return Err(kind.branching(cluster_uv[c], degree[c]));
        }
    }

    // Every cluster has degree 2: decompose into simple vertex-disjoint cycles.
    let mut visited = vec![false; curves.len()];
    let mut out_loops = Vec::new();

    for start_edge in 0..curves.len() {
        if visited[start_edge] {
            continue;
        }
        visited[start_edge] = true;
        let first = &curves[start_edge];
        let start_vertex = first.start_cluster;
        let mut curr_vertex = first.end_cluster;
        let mut chain: Vec<(usize, bool)> = vec![(start_edge, true)];

        while curr_vertex != start_vertex {
            let next_edge = incident[curr_vertex]
                .iter()
                .copied()
                .find(|&ei| !visited[ei])
                .expect("2-regular graph cycle must close on start_vertex");
            visited[next_edge] = true;
            let forward = curves[next_edge].start_cluster == curr_vertex;
            curr_vertex = if forward {
                curves[next_edge].end_cluster
            } else {
                curves[next_edge].start_cluster
            };
            chain.push((next_edge, forward));
        }

        let mut segments = Vec::with_capacity(chain.len());
        let mut entity_ids = Vec::with_capacity(chain.len());
        for (edge_idx, forward) in chain {
            let curve = &curves[edge_idx];
            let (ca, cb) = if forward {
                (curve.start_cluster, curve.end_cluster)
            } else {
                (curve.end_cluster, curve.start_cluster)
            };
            let a = cluster_uv[ca];
            let b = cluster_uv[cb];
            let stable_id = element_stable_id(sketch_id, curve.entity_id);
            entity_ids.push(curve.entity_id);
            match curve.arc_center {
                None => segments.push(BoardSegment2D::Line {
                    entity_id: curve.entity_id,
                    stable_id,
                    start: a,
                    end: b,
                }),
                Some(raw_center) => {
                    let (center, radius) = refine_welded_arc(a, b, raw_center);
                    segments.push(BoardSegment2D::Arc {
                        entity_id: curve.entity_id,
                        stable_id,
                        start: a,
                        end: b,
                        center,
                        radius,
                        ccw: forward,
                    });
                }
            }
        }

        let signed_area = compute_loop_signed_area(&segments);
        if !signed_area.is_finite()
            || signed_area.abs() <= TAU_MODEL * TAU_MODEL
            || loop_self_intersects(&segments)
        {
            return Err(BoardShapeError::SelfIntersectingLoop {
                role: kind.label().to_string(),
                entity_ids,
            });
        }

        if signed_area < 0.0 {
            segments = reverse_segments(&segments);
        }
        let ordered_entity_ids: Vec<u32> = segments
            .iter()
            .map(|s| match s {
                BoardSegment2D::Line { entity_id, .. }
                | BoardSegment2D::Arc { entity_id, .. }
                | BoardSegment2D::Circle { entity_id, .. } => *entity_id,
            })
            .collect();
        let stable_ids: Vec<String> = ordered_entity_ids
            .iter()
            .map(|&id| element_stable_id(sketch_id, id))
            .collect();

        out_loops.push(EvaluatedBoardLoop {
            entity_ids: ordered_entity_ids,
            stable_ids,
            segments,
            area_m2: signed_area.abs(),
        });
    }

    Ok(out_loops)
}

/// Ensure an arc's center is equidistant from its welded endpoints `a` and `b`
/// to machine precision while keeping bit-identical authored coordinates when
/// already concentric within `TAU_COINCIDENT`.
fn refine_welded_arc(a: [f64; 2], b: [f64; 2], raw_center: [f64; 2]) -> ([f64; 2], f64) {
    let ra = dist2(a, raw_center);
    let rb = dist2(b, raw_center);
    let scale = ra.max(rb).max(1.0);
    if (ra - rb).abs() <= TAU_COINCIDENT * scale {
        return (raw_center, 0.5 * (ra + rb));
    }
    let mid = [0.5 * (a[0] + b[0]), 0.5 * (a[1] + b[1])];
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let chord = dx.hypot(dy);
    let perp = [-dy / chord, dx / chord];
    let proj = (raw_center[0] - mid[0]) * perp[0] + (raw_center[1] - mid[1]) * perp[1];
    let center = [mid[0] + proj * perp[0], mid[1] + proj * perp[1]];
    let radius = 0.5 * (dist2(a, center) + dist2(b, center));
    (center, radius)
}

fn reverse_segments(segments: &[BoardSegment2D]) -> Vec<BoardSegment2D> {
    segments
        .iter()
        .rev()
        .map(|s| match s {
            BoardSegment2D::Line {
                entity_id,
                stable_id,
                start,
                end,
            } => BoardSegment2D::Line {
                entity_id: *entity_id,
                stable_id: stable_id.clone(),
                start: *end,
                end: *start,
            },
            BoardSegment2D::Arc {
                entity_id,
                stable_id,
                start,
                end,
                center,
                radius,
                ccw,
            } => BoardSegment2D::Arc {
                entity_id: *entity_id,
                stable_id: stable_id.clone(),
                start: *end,
                end: *start,
                center: *center,
                radius: *radius,
                ccw: !*ccw,
            },
            BoardSegment2D::Circle { .. } => s.clone(),
        })
        .collect()
}

// ── Analytical 2D geometry (areas, intersections, distances, winding) ───────

#[derive(Debug, Clone, Copy)]
enum Prim2D {
    Line {
        a: [f64; 2],
        b: [f64; 2],
    },
    Arc {
        a: [f64; 2],
        b: [f64; 2],
        center: [f64; 2],
        radius: f64,
        start_angle: f64,
        /// Signed angular sweep in `(-TAU, TAU) \ {0}`.
        signed_sweep: f64,
    },
    Circle {
        center: [f64; 2],
        radius: f64,
    },
}

fn loop_primitives(loop_2d: &EvaluatedBoardLoop) -> Vec<Prim2D> {
    segments_to_primitives(&loop_2d.segments)
}

fn segments_to_primitives(segments: &[BoardSegment2D]) -> Vec<Prim2D> {
    let mut out = Vec::with_capacity(segments.len());
    for seg in segments {
        match *seg {
            BoardSegment2D::Line { start, end, .. } => {
                out.push(Prim2D::Line { a: start, b: end });
            }
            BoardSegment2D::Arc {
                start,
                end,
                center,
                radius,
                ccw,
                ..
            } => {
                let sweep = directed_arc_sweep(center, start, end, ccw);
                let pieces = ((sweep / (0.5 * PI)).ceil() as usize).max(1);
                let a0 = (start[1] - center[1]).atan2(start[0] - center[0]);
                let signed_total = if ccw { sweep } else { -sweep };
                let mut prev = start;
                let mut prev_angle = a0;
                for k in 1..=pieces {
                    let (next, next_angle) = if k == pieces {
                        (end, a0 + signed_total)
                    } else {
                        let ang = a0 + signed_total * (k as f64 / pieces as f64);
                        (
                            [
                                center[0] + radius * ang.cos(),
                                center[1] + radius * ang.sin(),
                            ],
                            ang,
                        )
                    };
                    out.push(Prim2D::Arc {
                        a: prev,
                        b: next,
                        center,
                        radius,
                        start_angle: wrap_angle(prev_angle),
                        signed_sweep: next_angle - prev_angle,
                    });
                    prev = next;
                    prev_angle = next_angle;
                }
            }
            BoardSegment2D::Circle { center, radius, .. } => {
                out.push(Prim2D::Circle { center, radius });
            }
        }
    }
    out
}

fn compute_loop_signed_area(segments: &[BoardSegment2D]) -> f64 {
    let mut poly_twice = 0.0;
    let mut lune_sum = 0.0;
    for seg in segments {
        match *seg {
            BoardSegment2D::Line { start, end, .. } => {
                poly_twice += start[0] * end[1] - end[0] * start[1];
            }
            BoardSegment2D::Arc {
                start,
                end,
                center,
                radius,
                ccw,
                ..
            } => {
                poly_twice += start[0] * end[1] - end[0] * start[1];
                let sweep = directed_arc_sweep(center, start, end, ccw);
                let lune = 0.5 * radius * radius * (sweep - sweep.sin());
                if ccw {
                    lune_sum += lune;
                } else {
                    lune_sum -= lune;
                }
            }
            BoardSegment2D::Circle { radius, .. } => {
                return PI * radius * radius;
            }
        }
    }
    0.5 * poly_twice + lune_sum
}

fn loop_self_intersects(segments: &[BoardSegment2D]) -> bool {
    if segments.len() == 1 {
        return false;
    }
    let prims = segments_to_primitives(segments);
    let n = prims.len();
    if n < 2 {
        return true;
    }
    if n == 2 && prims.iter().all(|p| matches!(p, Prim2D::Line { .. })) {
        return true;
    }

    for i in 0..n {
        for j in (i + 1)..n {
            let adjacent = j == i + 1 || (i == 0 && j == n - 1);
            if adjacent {
                let mut shared = Vec::with_capacity(2);
                if i + 1 == j {
                    if let Some(p) = prim_end(&prims[i]) {
                        shared.push(p);
                    }
                }
                if i == 0 && j == n - 1 {
                    if let Some(p) = prim_end(&prims[j]) {
                        shared.push(p);
                    }
                }
                if adjacent_prims_fold_back(&prims[i], &prims[j], i == 0 && j == n - 1) {
                    return true;
                }
                for pt in prim_intersections(&prims[i], &prims[j]) {
                    if shared.iter().all(|&s| dist2(pt, s) > TAU_MODEL) {
                        return true;
                    }
                }
            } else if prim_prim_distance(&prims[i], &prims[j]) <= TAU_MODEL {
                return true;
            }
        }
    }
    false
}

/// Detect immediate overlap at a shared junction (`e_prev -> e_next`) when
/// two collinear lines or two co-circular arcs reverse onto each other.
fn adjacent_prims_fold_back(p_a: &Prim2D, p_b: &Prim2D, swapped_order: bool) -> bool {
    let (prev, next) = if swapped_order {
        (p_b, p_a)
    } else {
        (p_a, p_b)
    };
    match (*prev, *next) {
        (Prim2D::Line { a: a1, b: b1 }, Prim2D::Line { a: a2, b: b2 }) => {
            let u_back = unit2([a1[0] - b1[0], a1[1] - b1[1]]);
            let u_fwd = unit2([b2[0] - a2[0], b2[1] - a2[1]]);
            u_back[0] * u_fwd[0] + u_back[1] * u_fwd[1] > 1.0 - TAU_WORK
        }
        (
            Prim2D::Arc {
                b: b1,
                center: c1,
                radius: r1,
                signed_sweep: sw1,
                ..
            },
            Prim2D::Arc {
                a: a2,
                center: c2,
                radius: r2,
                signed_sweep: sw2,
                ..
            },
        ) if dist2(c1, c2) <= TAU_MODEL
            && (r1 - r2).abs() <= TAU_MODEL
            && dist2(b1, a2) <= TAU_MODEL =>
        {
            // Opposite sweep directions on the same circle at the shared endpoint overlap!
            (sw1 > 0.0) != (sw2 > 0.0)
        }
        _ => false,
    }
}

fn prim_start(p: &Prim2D) -> Option<[f64; 2]> {
    match *p {
        Prim2D::Line { a, .. } | Prim2D::Arc { a, .. } => Some(a),
        Prim2D::Circle { .. } => None,
    }
}

fn prim_end(p: &Prim2D) -> Option<[f64; 2]> {
    match *p {
        Prim2D::Line { b, .. } | Prim2D::Arc { b, .. } => Some(b),
        Prim2D::Circle { .. } => None,
    }
}

fn sample_boundary_point(loop_2d: &EvaluatedBoardLoop) -> [f64; 2] {
    match loop_2d.segments[0] {
        BoardSegment2D::Line { start, .. } | BoardSegment2D::Arc { start, .. } => start,
        BoardSegment2D::Circle { center, radius, .. } => [center[0] + radius, center[1]],
    }
}

fn point_to_loop_distance(p: [f64; 2], loop_2d: &EvaluatedBoardLoop) -> f64 {
    loop_primitives(loop_2d)
        .iter()
        .map(|prim| point_to_prim_distance(p, prim))
        .fold(f64::INFINITY, f64::min)
}

fn loop_loop_distance(a: &EvaluatedBoardLoop, b: &EvaluatedBoardLoop) -> f64 {
    let pa = loop_primitives(a);
    let pb = loop_primitives(b);
    let mut best = f64::INFINITY;
    for x in &pa {
        for y in &pb {
            let d = prim_prim_distance(x, y);
            if d < best {
                best = d;
                if best == 0.0 {
                    return 0.0;
                }
            }
        }
    }
    best
}

fn point_to_prim_distance(p: [f64; 2], prim: &Prim2D) -> f64 {
    match *prim {
        Prim2D::Line { a, b } => dist2(p, project_point_to_segment(p, a, b)),
        Prim2D::Circle { center, radius } => (dist2(p, center) - radius).abs(),
        Prim2D::Arc {
            a,
            b,
            center,
            radius,
            start_angle,
            signed_sweep,
        } => {
            let d = dist2(p, center);
            if d <= TAU_NORMALIZE {
                return radius;
            }
            let phi = (p[1] - center[1]).atan2(p[0] - center[0]);
            if angle_in_sweep(phi, start_angle, signed_sweep) {
                (d - radius).abs()
            } else {
                dist2(p, a).min(dist2(p, b))
            }
        }
    }
}

fn prim_prim_distance(p1: &Prim2D, p2: &Prim2D) -> f64 {
    if !prim_intersections(p1, p2).is_empty() {
        return 0.0;
    }

    let mut best = f64::INFINITY;
    for pt in [prim_start(p1), prim_end(p1)].into_iter().flatten() {
        best = best.min(point_to_prim_distance(pt, p2));
    }
    for pt in [prim_start(p2), prim_end(p2)].into_iter().flatten() {
        best = best.min(point_to_prim_distance(pt, p1));
    }

    match (*p1, *p2) {
        (Prim2D::Line { .. }, Prim2D::Line { .. }) => {}
        (Prim2D::Line { a, b }, Prim2D::Circle { center, .. })
        | (Prim2D::Line { a, b }, Prim2D::Arc { center, .. }) => {
            let q = project_point_to_segment(center, a, b);
            best = best.min(point_to_prim_distance(q, p2));
        }
        (Prim2D::Circle { center, .. }, Prim2D::Line { a, b })
        | (Prim2D::Arc { center, .. }, Prim2D::Line { a, b }) => {
            let q = project_point_to_segment(center, a, b);
            best = best.min(point_to_prim_distance(q, p1));
        }
        (c1, c2) => {
            let (center1, r1) = circle_support(&c1);
            let (center2, r2) = circle_support(&c2);
            let d = dist2(center1, center2);
            if d > TAU_NORMALIZE {
                let ux = (center2[0] - center1[0]) / d;
                let uy = (center2[1] - center1[1]) / d;
                for sign in [-1.0, 1.0] {
                    let cand1 = [center1[0] + sign * r1 * ux, center1[1] + sign * r1 * uy];
                    if point_on_circle_prim(cand1, &c1) {
                        best = best.min(point_to_prim_distance(cand1, &c2));
                    }
                    let cand2 = [center2[0] + sign * r2 * ux, center2[1] + sign * r2 * uy];
                    if point_on_circle_prim(cand2, &c2) {
                        best = best.min(point_to_prim_distance(cand2, &c1));
                    }
                }
            } else {
                // Concentric arcs/circles: test cardinal rays plus any overlap ray.
                for ang in [0.0, 0.5 * PI, PI, -0.5 * PI] {
                    let cand1 = [center1[0] + r1 * ang.cos(), center1[1] + r1 * ang.sin()];
                    if point_on_circle_prim(cand1, &c1) {
                        best = best.min(point_to_prim_distance(cand1, &c2));
                    }
                }
            }
        }
    }

    best
}

fn circle_support(p: &Prim2D) -> ([f64; 2], f64) {
    match *p {
        Prim2D::Circle { center, radius } | Prim2D::Arc { center, radius, .. } => (center, radius),
        Prim2D::Line { .. } => unreachable!(),
    }
}

fn point_on_circle_prim(pt: [f64; 2], prim: &Prim2D) -> bool {
    match *prim {
        Prim2D::Circle { .. } => true,
        Prim2D::Arc {
            center,
            start_angle,
            signed_sweep,
            ..
        } => {
            let phi = (pt[1] - center[1]).atan2(pt[0] - center[0]);
            angle_in_sweep(phi, start_angle, signed_sweep)
        }
        Prim2D::Line { .. } => false,
    }
}

/// Closed-form curve–curve intersection over `{Line, Arc, Circle}`.
fn prim_intersections(p1: &Prim2D, p2: &Prim2D) -> Vec<[f64; 2]> {
    match (*p1, *p2) {
        (Prim2D::Line { a: a1, b: b1 }, Prim2D::Line { a: a2, b: b2 }) => {
            line_line_intersections(a1, b1, a2, b2)
        }
        (Prim2D::Line { a, b }, c @ (Prim2D::Arc { .. } | Prim2D::Circle { .. })) => {
            line_circle_prim_intersections(a, b, &c)
        }
        (c @ (Prim2D::Arc { .. } | Prim2D::Circle { .. }), Prim2D::Line { a, b }) => {
            line_circle_prim_intersections(a, b, &c)
        }
        (c1, c2) => circle_circle_prim_intersections(&c1, &c2),
    }
}

fn line_line_intersections(
    a1: [f64; 2],
    b1: [f64; 2],
    a2: [f64; 2],
    b2: [f64; 2],
) -> Vec<[f64; 2]> {
    let u = [b1[0] - a1[0], b1[1] - a1[1]];
    let v = [b2[0] - a2[0], b2[1] - a2[1]];
    let w = [a2[0] - a1[0], a2[1] - a1[1]];
    let len_u = u[0].hypot(u[1]);
    let len_v = v[0].hypot(v[1]);
    if len_u <= TAU_NORMALIZE || len_v <= TAU_NORMALIZE {
        return Vec::new();
    }
    let denom = u[0] * v[1] - u[1] * v[0];
    if denom.abs() > TAU_NORMALIZE * len_u * len_v {
        let t = (w[0] * v[1] - w[1] * v[0]) / denom;
        let s = (w[0] * u[1] - w[1] * u[0]) / denom;
        let eps_t = TAU_COINCIDENT / len_u;
        let eps_s = TAU_COINCIDENT / len_v;
        if (-eps_t..=1.0 + eps_t).contains(&t) && (-eps_s..=1.0 + eps_s).contains(&s) {
            let tc = t.clamp(0.0, 1.0);
            return vec![[a1[0] + tc * u[0], a1[1] + tc * u[1]]];
        }
        return Vec::new();
    }

    // Parallel / collinear: check perpendicular offset and 1D interval overlap.
    let perp = (w[0] * u[1] - w[1] * u[0]).abs() / len_u;
    if perp > TAU_COINCIDENT {
        return Vec::new();
    }
    let ux = u[0] / len_u;
    let uy = u[1] / len_u;
    let s0 = w[0] * ux + w[1] * uy;
    let s1 = (b2[0] - a1[0]) * ux + (b2[1] - a1[1]) * uy;
    let lo = s0.min(s1).max(0.0);
    let hi = s0.max(s1).min(len_u);
    if hi + TAU_COINCIDENT < lo {
        return Vec::new();
    }
    let p_lo = [
        a1[0] + lo.clamp(0.0, len_u) * ux,
        a1[1] + lo.clamp(0.0, len_u) * uy,
    ];
    let p_hi = [
        a1[0] + hi.clamp(0.0, len_u) * ux,
        a1[1] + hi.clamp(0.0, len_u) * uy,
    ];
    if dist2(p_lo, p_hi) > TAU_MODEL {
        vec![p_lo, p_hi]
    } else {
        vec![p_lo]
    }
}

fn line_circle_prim_intersections(a: [f64; 2], b: [f64; 2], cprim: &Prim2D) -> Vec<[f64; 2]> {
    let (center, radius) = circle_support(cprim);
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let len = dx.hypot(dy);
    if len <= TAU_NORMALIZE {
        return Vec::new();
    }
    let ux = dx / len;
    let uy = dy / len;
    let fx = a[0] - center[0];
    let fy = a[1] - center[1];
    let t_mid = -(fx * ux + fy * uy);
    let mx = fx + t_mid * ux;
    let my = fy + t_mid * uy;
    let h_sq = mx * mx + my * my;
    let disc = radius * radius - h_sq;
    if disc < -2.0 * radius * TAU_COINCIDENT {
        return Vec::new();
    }
    let offset = disc.max(0.0).sqrt();
    let mut out: Vec<[f64; 2]> = Vec::with_capacity(2);
    for t in [t_mid - offset, t_mid + offset] {
        if (-TAU_COINCIDENT..=len + TAU_COINCIDENT).contains(&t) {
            let tc = t.clamp(0.0, len);
            let pt = [a[0] + tc * ux, a[1] + tc * uy];
            if point_on_circle_prim(pt, cprim)
                && out.iter().all(|&prev| dist2(prev, pt) > TAU_COINCIDENT)
            {
                out.push(pt);
            }
        }
    }
    out
}

fn circle_circle_prim_intersections(c1: &Prim2D, c2: &Prim2D) -> Vec<[f64; 2]> {
    let (center1, r1) = circle_support(c1);
    let (center2, r2) = circle_support(c2);
    let dx = center2[0] - center1[0];
    let dy = center2[1] - center1[1];
    let d = dx.hypot(dy);

    if d <= TAU_MODEL && (r1 - r2).abs() <= TAU_MODEL {
        // Co-circular: sample endpoints + midpoints of each arc that lie on the other.
        let mut out: Vec<[f64; 2]> = Vec::new();
        for (src, other) in [(c1, c2), (c2, c1)] {
            for pt in cocircular_test_points(src) {
                if point_on_circle_prim(pt, other)
                    && out.iter().all(|&prev| dist2(prev, pt) > TAU_COINCIDENT)
                {
                    out.push(pt);
                }
            }
        }
        return out;
    }

    if d <= TAU_NORMALIZE
        || d > r1 + r2 + TAU_COINCIDENT
        || d + r1.min(r2) + TAU_COINCIDENT < r1.max(r2)
    {
        return Vec::new();
    }

    let x = (d * d - r2 * r2 + r1 * r1) / (2.0 * d);
    let h_sq = r1 * r1 - x * x;
    if h_sq < -2.0 * r1 * TAU_COINCIDENT {
        return Vec::new();
    }
    let h = h_sq.max(0.0).sqrt();
    let ux = dx / d;
    let uy = dy / d;
    let nx = -uy;
    let ny = ux;

    let mut out: Vec<[f64; 2]> = Vec::with_capacity(2);
    for sign in [-1.0, 1.0] {
        let pt = [
            center1[0] + x * ux + sign * h * nx,
            center1[1] + x * uy + sign * h * ny,
        ];
        if point_on_circle_prim(pt, c1)
            && point_on_circle_prim(pt, c2)
            && out.iter().all(|&prev| dist2(prev, pt) > TAU_COINCIDENT)
        {
            out.push(pt);
        }
    }
    out
}

fn cocircular_test_points(prim: &Prim2D) -> Vec<[f64; 2]> {
    match *prim {
        Prim2D::Circle { center, radius } => vec![
            [center[0] + radius, center[1]],
            [center[0], center[1] + radius],
            [center[0] - radius, center[1]],
            [center[0], center[1] - radius],
        ],
        Prim2D::Arc {
            a,
            b,
            center,
            radius,
            start_angle,
            signed_sweep,
        } => {
            let mid_ang = start_angle + 0.5 * signed_sweep;
            let q1_ang = start_angle + 0.25 * signed_sweep;
            let q3_ang = start_angle + 0.75 * signed_sweep;
            vec![
                a,
                [
                    center[0] + radius * q1_ang.cos(),
                    center[1] + radius * q1_ang.sin(),
                ],
                [
                    center[0] + radius * mid_ang.cos(),
                    center[1] + radius * mid_ang.sin(),
                ],
                [
                    center[0] + radius * q3_ang.cos(),
                    center[1] + radius * q3_ang.sin(),
                ],
                b,
            ]
        }
        Prim2D::Line { .. } => Vec::new(),
    }
}

/// Exact analytical point-in-loop containment via Cauchy winding number.
/// Callers establish `point_to_loop_distance(p, loop_2d) > TAU_MODEL` first.
fn point_in_loop(p: [f64; 2], loop_2d: &EvaluatedBoardLoop) -> bool {
    if let [BoardSegment2D::Circle { center, radius, .. }] = loop_2d.segments.as_slice() {
        return dist2(p, *center) < *radius;
    }

    let mut total_angle = 0.0;
    for prim in loop_primitives(loop_2d) {
        match prim {
            Prim2D::Line { a, b } => {
                total_angle += chord_angle(p, a, b);
            }
            Prim2D::Arc {
                a,
                b,
                center,
                radius,
                start_angle,
                signed_sweep,
            } => {
                total_angle +=
                    arc_winding_angle(p, (a, b), (center, radius), (start_angle, signed_sweep), 0);
            }
            Prim2D::Circle { center, radius } => {
                if dist2(p, center) < radius {
                    total_angle += TAU;
                }
            }
        }
    }
    (total_angle / TAU).round() as i32 != 0
}

fn arc_winding_angle(
    p: [f64; 2],
    endpoints: ([f64; 2], [f64; 2]),
    circle: ([f64; 2], f64),
    angles: (f64, f64),
    depth: u32,
) -> f64 {
    let (a, b) = endpoints;
    let (center, radius) = circle;
    let (start_angle, signed_sweep) = angles;
    let u = [a[0] - p[0], a[1] - p[1]];
    let v = [b[0] - p[0], b[1] - p[1]];
    let du = u[0].hypot(u[1]);
    let dv = v[0].hypot(v[1]);
    let dot = u[0] * v[0] + u[1] * v[1];
    let arc_len = radius * signed_sweep.abs();

    if depth >= 32 || (signed_sweep.abs() <= 0.5 * PI && dot > 0.0 && arc_len < du + dv) {
        return chord_angle(p, a, b);
    }

    let half = 0.5 * signed_sweep;
    let mid_ang = start_angle + half;
    let mid = [
        center[0] + radius * mid_ang.cos(),
        center[1] + radius * mid_ang.sin(),
    ];
    arc_winding_angle(p, (a, mid), circle, (start_angle, half), depth + 1)
        + arc_winding_angle(p, (mid, b), circle, (mid_ang, half), depth + 1)
}

fn chord_angle(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let ux = a[0] - p[0];
    let uy = a[1] - p[1];
    let vx = b[0] - p[0];
    let vy = b[1] - p[1];
    (ux * vy - uy * vx).atan2(ux * vx + uy * vy)
}

fn directed_arc_sweep(center: [f64; 2], start: [f64; 2], end: [f64; 2], ccw: bool) -> f64 {
    let a0 = (start[1] - center[1]).atan2(start[0] - center[0]);
    let a1 = (end[1] - center[1]).atan2(end[0] - center[0]);
    let mut delta = if ccw { a1 - a0 } else { a0 - a1 };
    while delta <= 0.0 {
        delta += TAU;
    }
    while delta > TAU {
        delta -= TAU;
    }
    delta
}

fn wrap_angle(mut a: f64) -> f64 {
    while a <= -PI {
        a += TAU;
    }
    while a > PI {
        a -= TAU;
    }
    a
}

fn angle_in_sweep(phi: f64, start_angle: f64, signed_sweep: f64) -> bool {
    let ang_tol = TAU_WORK;
    if signed_sweep >= 0.0 {
        let mut delta = phi - start_angle;
        while delta < -ang_tol {
            delta += TAU;
        }
        while delta >= TAU - ang_tol {
            delta -= TAU;
        }
        delta <= signed_sweep + ang_tol
    } else {
        let mut delta = start_angle - phi;
        while delta < -ang_tol {
            delta += TAU;
        }
        while delta >= TAU - ang_tol {
            delta -= TAU;
        }
        delta <= -signed_sweep + ang_tol
    }
}

fn project_point_to_segment(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let len_sq = dx * dx + dy * dy;
    if len_sq <= TAU_NORMALIZE * TAU_NORMALIZE {
        return a;
    }
    let t = (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len_sq).clamp(0.0, 1.0);
    [a[0] + t * dx, a[1] + t * dy]
}

fn dist2(a: [f64; 2], b: [f64; 2]) -> f64 {
    (b[0] - a[0]).hypot(b[1] - a[1])
}

fn unit2(v: [f64; 2]) -> [f64; 2] {
    let n = v[0].hypot(v[1]);
    if n <= TAU_NORMALIZE {
        [0.0, 0.0]
    } else {
        [v[0] / n, v[1] / n]
    }
}

fn norm3(v: [f64; 3]) -> f64 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

fn cross3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
