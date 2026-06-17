use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use waffle_types::{GeomRef, OutputKey, Sketch};

/// User-assigned body display names, keyed by a body's persistent identity
/// (`"{feature_id}/{output_key.tag()}"`). Absent ⇒ the body uses a derived
/// name (its producing feature's name). Stored on the tree so it persists with
/// the document; `#[serde(default)]` keeps older files (no field) loading.
pub type BodyNames = HashMap<String, String>;

/// The ordered list of modeling features.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureTree {
    /// Ordered list of features. Index 0 is the first feature.
    pub features: Vec<Feature>,
    /// Features after this index are suppressed during rebuild.
    /// None means all features are active.
    pub active_index: Option<usize>,
    /// User-assigned body names, independent of feature names.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub body_names: BodyNames,
}

impl FeatureTree {
    pub fn new() -> Self {
        Self {
            features: Vec::new(),
            active_index: None,
            body_names: HashMap::new(),
        }
    }

    /// Persistent identity string for a body: its producing feature plus which
    /// output of that feature it is. This is the key into `body_names`.
    pub fn body_id(feature_id: Uuid, output_key: &OutputKey) -> String {
        format!("{}/{}", feature_id, output_key.tag())
    }

    /// Set (or clear, with `None`) a body's display-name override. Returns the
    /// previous override, if any. No rebuild needed — names don't affect geometry.
    pub fn set_body_name(&mut self, body_id: &str, name: Option<String>) -> Option<String> {
        match name {
            Some(n) => self.body_names.insert(body_id.to_string(), n),
            None => self.body_names.remove(body_id),
        }
    }

    /// Look up a body's display-name override, if the user has set one.
    pub fn body_name_override(&self, body_id: &str) -> Option<&str> {
        self.body_names.get(body_id).map(String::as_str)
    }

    /// Remove and return all body-name overrides owned by `feature_id`. Called
    /// on feature delete so the names are GC'd from the live tree but captured
    /// for undo (NOT triggered by a transient empty rebuild — a feature that
    /// errors then recovers keeps its body names).
    pub fn take_body_names(&mut self, feature_id: Uuid) -> BodyNames {
        // Keys are "{feature_id}/{tag}"; UUIDs contain no '/'.
        let prefix = format!("{feature_id}/");
        let keys: Vec<String> = self
            .body_names
            .keys()
            .filter(|k| k.starts_with(&prefix))
            .cloned()
            .collect();
        let mut taken = BodyNames::new();
        for k in keys {
            if let Some(v) = self.body_names.remove(&k) {
                taken.insert(k, v);
            }
        }
        taken
    }

    /// Re-merge body-name overrides (used to undo a feature delete).
    pub fn restore_body_names(&mut self, names: BodyNames) {
        self.body_names.extend(names);
    }

    /// Return active features (up to active_index).
    pub fn active_features(&self) -> &[Feature] {
        match self.active_index {
            Some(_) if self.features.is_empty() => &[],
            Some(idx) => &self.features[..=idx.min(self.features.len() - 1)],
            None => &self.features,
        }
    }
}

impl Default for FeatureTree {
    fn default() -> Self {
        Self::new()
    }
}

/// A single feature in the parametric feature tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Feature {
    /// Unique identifier.
    pub id: Uuid,
    /// User-visible name.
    pub name: String,
    /// The modeling operation this feature performs.
    pub operation: Operation,
    /// Whether this feature is suppressed.
    pub suppressed: bool,
    /// GeomRefs to geometry that this feature depends on.
    pub references: Vec<GeomRef>,
}

/// A parametric modeling operation with its parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Operation {
    Sketch { sketch: Sketch },
    Extrude { params: ExtrudeParams },
    Revolve { params: RevolveParams },
    Fillet { params: FilletParams },
    Chamfer { params: ChamferParams },
    Shell { params: ShellParams },
    BooleanCombine { params: BooleanParams },
    DatumPlane { params: DatumPlaneParams },
}

/// Depth mode for extrude operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum DepthMode {
    /// Use the `depth` field directly.
    Blind,
    /// Project target body vertices onto extrude direction, use max extent + margin.
    ThroughAll,
    /// Extrude up to a reference (face centroid, vertex, or datum plane).
    UpTo { reference: GeomRef },
}

/// Second direction for bidirectional extrude.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SecondDirection {
    /// Same depth as primary direction.
    Symmetric,
    /// Independent blind depth in second direction.
    Blind { depth: f64 },
    /// Through all in second direction.
    ThroughAll,
    /// Up to a reference in second direction.
    UpTo { reference: GeomRef },
}

fn default_true() -> bool {
    true
}

fn default_depth_mode() -> DepthMode {
    DepthMode::Blind
}

/// Parameters for an extrude operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtrudeParams {
    pub sketch_id: Uuid,
    pub profile_index: usize,
    pub depth: f64,
    pub direction: Option<[f64; 3]>,
    pub symmetric: bool,
    pub cut: bool,
    /// Auto-union with existing body. Defaults to true for boss extrudes.
    #[serde(default = "default_true")]
    pub merge: bool,
    pub target_body: Option<GeomRef>,
    #[serde(default = "default_depth_mode")]
    pub depth_mode: DepthMode,
    #[serde(default)]
    pub second_direction: Option<SecondDirection>,
    /// Explicit region boundary for a sketch sub-region (annulus, lens, …) that
    /// no whole-loop `profile_index` denotes. When `Some`, the face is built
    /// directly from this boundary and `profile_index` is ignored. Whole-loop
    /// selections leave this `None` and use `profile_index` (analytical path).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<waffle_types::Region>,
}

/// Parameters for a revolve operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevolveParams {
    pub sketch_id: Uuid,
    pub profile_index: usize,
    pub axis_origin: [f64; 3],
    pub axis_direction: [f64; 3],
    pub angle: f64,
    /// If true, subtract this revolve from the target body.
    #[serde(default)]
    pub cut: bool,
    /// If true (and cut=false), auto-union with the most recent body.
    #[serde(default = "default_merge_true")]
    pub merge: bool,
}

fn default_merge_true() -> bool {
    true
}

/// Parameters for a fillet operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilletParams {
    pub edges: Vec<GeomRef>,
    pub radius: f64,
}

/// Parameters for a chamfer operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChamferParams {
    pub edges: Vec<GeomRef>,
    pub distance: f64,
}

/// Parameters for a shell operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellParams {
    pub faces_to_remove: Vec<GeomRef>,
    pub thickness: f64,
}

/// Parameters for a boolean combine operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BooleanParams {
    pub body_a: GeomRef,
    pub body_b: GeomRef,
    pub operation: BooleanOp,
}

/// Boolean operation type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum BooleanOp {
    Union,
    Subtract,
    Intersect,
}

/// How a construction plane is defined.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "method")]
pub enum PlaneDefinition {
    /// Explicit origin + normal.
    #[serde(rename = "point-normal")]
    PointNormal { origin: [f64; 3], normal: [f64; 3] },
    /// Parallel offset from another plane.
    #[serde(rename = "offset")]
    Offset {
        #[serde(rename = "basePlaneId")]
        base_plane_id: Uuid,
        distance: f64,
    },
}

/// Parameters for a datum (construction) plane.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatumPlaneParams {
    pub name: String,
    pub definition: PlaneDefinition,
}

/// Errors from the feature engine.
#[derive(Debug, Clone, thiserror::Error)]
pub enum EngineError {
    #[error("feature not found: {id}")]
    FeatureNotFound { id: Uuid },

    #[error("sketch not found: {id}")]
    SketchNotFound { id: Uuid },

    #[error("profile index {index} out of range (sketch has {count} profiles)")]
    ProfileOutOfRange { index: usize, count: usize },

    #[error("GeomRef resolution failed: {reason}")]
    ResolutionFailed { reason: String },

    #[error("kernel error: {0}")]
    KernelError(#[from] waffle_types::kernel::KernelError),

    #[error("operation error: {0}")]
    OpError(#[from] modeling_ops::OpError),

    #[error("rebuild failed at feature {feature_name}: {reason}")]
    RebuildFailed {
        feature_name: String,
        reason: String,
    },

    #[error("nothing to undo")]
    NothingToUndo,

    #[error("nothing to redo")]
    NothingToRedo,
}
