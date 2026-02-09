use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::roles::Role;
use crate::topo::{TopoKind, TopoQuery, TopoSignature};

/// Persistent geometry reference. The core of the persistent naming system.
/// A GeomRef identifies a specific topological entity across parametric rebuilds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeomRef {
    /// What kind of topological entity this references.
    pub kind: TopoKind,
    /// Which feature's output contains this entity.
    pub anchor: Anchor,
    /// How to find the specific entity within the anchor's output.
    pub selector: Selector,
    /// What to do when resolution is ambiguous or fails.
    pub policy: ResolvePolicy,
}

/// Identifies which feature output contains the target entity.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Anchor {
    /// References an output of a specific feature in the tree.
    FeatureOutput {
        feature_id: Uuid,
        output_key: OutputKey,
    },
    /// References a datum (construction plane, axis, or point).
    Datum { datum_id: Uuid },
}

/// Identifies which output of a feature to look in.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum OutputKey {
    /// The primary solid body output.
    Main,
    /// A secondary body (e.g., from boolean split).
    Body { index: usize },
    /// A sketch profile (closed loop suitable for extrusion).
    Profile { index: usize },
    /// A datum plane/axis/point output.
    Datum { name: String },
}

/// How to find a specific entity within a feature's output.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Selector {
    /// Select by semantic role assigned during the operation.
    Role { role: Role, index: usize },
    /// Select by geometric signature matching.
    Signature { signature: TopoSignature },
    /// Select by user-specified geometric query.
    Query { query: TopoQuery },
}

/// What to do when GeomRef resolution is ambiguous or fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ResolvePolicy {
    /// Fail the rebuild if the reference cannot be uniquely resolved.
    Strict,
    /// Use the closest match and emit a warning.
    BestEffort,
}
