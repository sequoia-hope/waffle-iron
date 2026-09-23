use serde::{Deserialize, Serialize};

/// Semantic role assigned to topological entities by modeling operations.
/// Roles provide stable, meaningful names for geometry that survive topology changes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum Role {
    /// The face on the positive extrusion direction end.
    EndCapPositive,
    /// The face on the negative extrusion direction end (original sketch plane face).
    EndCapNegative,
    /// A lateral face created by sweeping a profile edge.
    SideFace { index: usize },
    /// The face at the start of a revolution.
    RevStartFace,
    /// The face at the end of a revolution (if not full 360).
    RevEndFace,
    /// A face created by a fillet operation.
    FilletFace { index: usize },
    /// A face created by a chamfer operation.
    ChamferFace { index: usize },
    /// An inner face created by a shell operation.
    ShellInnerFace { index: usize },
    /// The original profile face (sketch plane) of an extrude/revolve.
    ProfileFace,
    /// An instance in a pattern operation.
    PatternInstance { index: usize },
    /// A face from the first body in a boolean operation.
    BooleanBodyAFace { index: usize },
    /// A face from the second body in a boolean operation.
    BooleanBodyBFace { index: usize },
    /// A face or edge a custom feature script named in its return value
    /// (`#{ hub_face: query }` ⇒ `Named { name: "hub_face" }` on the entity
    /// the query resolved to). Later features select it by name without
    /// knowing the script's private sub-tree.
    Named { name: String },
}
