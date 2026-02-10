use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use feature_engine::types::{FeatureTree, Operation};
use kernel_fork::{EdgeRenderData, RenderMesh};
use waffle_types::{ClosedProfile, GeomRef, SketchConstraint, SketchEntity, SolvedSketch};

/// Serde helper for HashMap<u32, (f64, f64)> — JSON string keys ↔ u32.
mod u32_key_map {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::collections::HashMap;

    pub fn serialize<S>(map: &HashMap<u32, (f64, f64)>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let string_map: HashMap<String, (f64, f64)> =
            map.iter().map(|(k, v)| (k.to_string(), *v)).collect();
        string_map.serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<HashMap<u32, (f64, f64)>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let string_map: HashMap<String, (f64, f64)> = HashMap::deserialize(deserializer)?;
        string_map
            .into_iter()
            .map(|(k, v)| {
                k.parse::<u32>()
                    .map(|key| (key, v))
                    .map_err(serde::de::Error::custom)
            })
            .collect()
    }
}

/// Messages from the UI (JavaScript main thread) to the engine (WASM Worker).
/// Serialized as JSON for postMessage transfer.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum UiToEngine {
    // -- Sketch operations --
    /// Enter sketch mode on a face or datum plane.
    BeginSketch {
        plane: GeomRef,
    },
    /// Add a geometric entity to the active sketch.
    AddSketchEntity {
        entity: SketchEntity,
    },
    /// Add a constraint to the active sketch.
    AddConstraint {
        constraint: SketchConstraint,
    },
    /// Run the constraint solver on the active sketch.
    SolveSketch,
    /// Exit sketch mode and commit the sketch as a feature.
    FinishSketch {
        #[serde(default, with = "u32_key_map")]
        solved_positions: HashMap<u32, (f64, f64)>,
        #[serde(default)]
        solved_profiles: Vec<ClosedProfile>,
    },

    // -- Feature operations --
    /// Add a new feature to the feature tree.
    AddFeature {
        operation: Operation,
    },
    /// Edit an existing feature's parameters.
    EditFeature {
        feature_id: Uuid,
        operation: Operation,
    },
    /// Delete a feature from the tree.
    DeleteFeature {
        feature_id: Uuid,
    },
    /// Suppress/unsuppress a feature.
    SuppressFeature {
        feature_id: Uuid,
        suppressed: bool,
    },
    /// Reorder a feature to a new position.
    ReorderFeature {
        feature_id: Uuid,
        new_position: usize,
    },
    /// Rename a feature.
    RenameFeature {
        feature_id: Uuid,
        new_name: String,
    },
    /// Set the rollback index.
    SetRollbackIndex {
        index: Option<usize>,
    },

    // -- History --
    Undo,
    Redo,

    // -- Selection --
    /// User selected an entity in the viewport.
    SelectEntity {
        geom_ref: GeomRef,
    },
    /// User is hovering over an entity in the viewport.
    HoverEntity {
        geom_ref: Option<GeomRef>,
    },

    // -- File operations --
    SaveProject,
    LoadProject {
        data: String,
    },
    ExportStep,
}

/// Messages from the engine (WASM Worker) to the UI (JavaScript main thread).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum EngineToUi {
    /// The model has been rebuilt.
    ModelUpdated {
        feature_tree: FeatureTree,
        meshes: Vec<RenderMesh>,
        edges: Vec<EdgeRenderData>,
    },

    /// Sketch constraint solver completed.
    SketchSolved { solved: SolvedSketch },

    /// The hovered entity changed.
    HoverChanged { geom_ref: Option<GeomRef> },

    /// The selection changed.
    SelectionChanged { geom_refs: Vec<GeomRef> },

    /// An error occurred in the engine.
    Error {
        message: String,
        feature_id: Option<Uuid>,
    },

    /// Save project is ready.
    SaveReady { json_data: String },

    /// Project loaded successfully.
    ProjectLoaded { feature_tree: FeatureTree },

    /// STEP export is ready.
    ExportReady { step_data: String },
}
