use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::roles::Role;
use crate::topo::{TopoKind, TopoQuery, TopoSignature};

/// Persistent geometry reference. The core of the persistent naming system.
/// A GeomRef identifies a specific topological entity across parametric rebuilds.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct GeomRef {
    /// What kind of topological entity this references.
    pub kind: TopoKind,
    /// Which feature's output contains this entity.
    pub anchor: Anchor,
    /// How to find the specific entity within the anchor's output.
    pub selector: Selector,
    /// What to do when resolution is ambiguous or fails.
    #[serde(default)]
    pub policy: ResolvePolicy,
    /// Where the anchor lives when it is NOT in the current tab (v4 §2.8,
    /// in-context editing): the assembly whose instance owns the geometry
    /// and the chain of instance ids down to it. Absent ⇒ local (the current
    /// tab). A reader that does not know this field would silently resolve
    /// the anchor locally, so its arrival bumped the reader floor (format v5).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<RefScope>,
}

/// Scope of a geometry reference into another tab's instance (v4 §2.8).
///
/// `source_id` absent ⇒ this document; `tab_id` absent ⇒ the current tab;
/// `instance_path` = the chain of assembly-instance ids from the referencing
/// assembly (`tab_id`) down to the part instance whose feature output the
/// anchor names. A Part feature edited in the context of an assembly
/// references the OTHER instances this way; the engine resolves such a
/// reference through its edit context (the `feature_engine::context`
/// module) and reports it loudly when the context is not open.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct RefScope {
    /// The document the assembly lives in; absent ⇒ this document.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<Uuid>,
    /// The assembly tab the instance path starts from; absent ⇒ the current tab.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_id: Option<String>,
    /// Chain of assembly-instance ids down to the owning part instance.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub instance_path: Vec<Uuid>,
}

impl RefScope {
    /// A scope into `instance_path` of the assembly tab `tab_id` of this document.
    pub fn in_assembly(tab_id: impl Into<String>, instance_path: Vec<Uuid>) -> Self {
        Self {
            source_id: None,
            tab_id: Some(tab_id.into()),
            instance_path,
        }
    }
}

/// Identifies which feature output contains the target entity.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
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
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum OutputKey {
    /// The primary solid body output.
    Main,
    /// A secondary body (e.g., from boolean split).
    Body { index: usize },
    /// A sketch profile (closed loop suitable for extrusion).
    Profile { index: usize },
    /// A datum plane/axis/point output.
    Datum { name: String },
    /// A body a custom feature script named in its return value
    /// (`specs/custom_features_and_modeling_roadmap.md` §A6: `#{ hub: … }`
    /// ⇒ `Named { name: "hub" }`). The script's `main` body stays `Main`.
    Named { name: String },
}

impl OutputKey {
    /// Stable string tag for this output key, used to build a persistent body
    /// identity (`"{feature_id}/{tag}"`). Must round-trip stably across
    /// rebuilds for body names to stick to the right output.
    pub fn tag(&self) -> String {
        match self {
            OutputKey::Main => "Main".to_string(),
            OutputKey::Body { index } => format!("Body:{index}"),
            OutputKey::Profile { index } => format!("Profile:{index}"),
            OutputKey::Datum { name } => format!("Datum:{name}"),
            OutputKey::Named { name } => format!("Named:{name}"),
        }
    }
}

/// How to find a specific entity within a feature's output.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum Selector {
    /// Select by semantic role assigned during the operation.
    Role { role: Role, index: usize },
    /// Select by geometric signature matching.
    Signature { signature: TopoSignature },
    /// Select by user-specified geometric query.
    Query { query: TopoQuery },
    /// Select by 3D position (nearest entity within tolerance).
    Position { x: f64, y: f64, z: f64 },
}

/// What to do when GeomRef resolution is ambiguous or fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum ResolvePolicy {
    /// Fail the rebuild if the reference cannot be uniquely resolved.
    Strict,
    /// Use the closest match and emit a warning.
    #[default]
    BestEffort,
}
