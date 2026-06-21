use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use feature_engine::types::{FeatureTree, Operation};
use waffle_types::kernel::{EdgeRenderData, RenderMesh};
use waffle_types::{
    ClosedProfile, GearParams, GeomRef, PlanetaryParams, PlanetaryResult, Region, SketchConstraint,
    SketchEntity, SolvedSketch,
};

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

fn default_origin() -> [f64; 3] {
    [0.0, 0.0, 0.0]
}

fn default_normal() -> [f64; 3] {
    [0.0, 0.0, 1.0]
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
    /// Run the constraint solver on the active sketch. The UI may pass its LIVE
    /// state to replace the active sketch atomically before solving — the
    /// append-only `AddSketchEntity` / `AddConstraint` paths keep the ORIGINAL
    /// drawn positions and cannot express a removal or a REFERENCE (driven)
    /// dimension toggle. `entities` carries current point positions (so a drag
    /// persists); `constraints` is the DRIVING set (reference dims excluded).
    /// `None` (omitted) solves the existing engine state unchanged.
    SolveSketch {
        #[serde(default)]
        entities: Option<Vec<SketchEntity>>,
        #[serde(default)]
        constraints: Option<Vec<SketchConstraint>>,
    },
    /// Exit sketch mode and commit the sketch as a feature.
    FinishSketch {
        #[serde(default, with = "u32_key_map")]
        solved_positions: HashMap<u32, (f64, f64)>,
        #[serde(default)]
        solved_profiles: Vec<ClosedProfile>,
        #[serde(default = "default_origin")]
        plane_origin: [f64; 3],
        #[serde(default = "default_normal")]
        plane_normal: [f64; 3],
        /// Final entity state from the JS solver (includes solved radii).
        /// Overrides stale entities from AddSketchEntity calls.
        #[serde(default)]
        entities: Vec<SketchEntity>,
        #[serde(default)]
        constraints: Vec<SketchConstraint>,
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
    /// Rename a body (set its display-name override), independent of features.
    /// `body_id` is the persistent body identity (`FeatureTree::body_id`).
    /// An empty `new_name` clears the override (reverts to the derived name).
    RenameBody {
        body_id: String,
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
    ExportStl,
    /// Export a single body to STL. `body_id` is the persistent body identity
    /// (`FeatureTree::body_id` = `"{feature_id}/{output_key.tag()}"`).
    ExportBodyStl {
        body_id: String,
    },

    // -- Tab / document management --
    /// Switch to a different tab, saving current features and loading new ones.
    SwitchTab {
        /// Features of the tab being switched TO.
        features: FeatureTree,
    },
    /// Reset engine to a clean state (new document).
    NewDocument,

    // -- Settings --
    /// Set the document display unit (mm, cm, m, in, ft).
    SetDisplayUnit {
        unit: String,
    },

    // -- Gear generation (stateless) --
    /// Generate a gear preview polyline for live rendering.
    GenerateGearPreview {
        params: GearParams,
    },
    /// Generate a full gear profile with sketch entities.
    GenerateGearProfile {
        params: GearParams,
    },
    /// Generate a planetary gear stage: validate + compute the positioned
    /// sun/planet/ring `GearParams`. Stateless.
    GeneratePlanetary {
        params: PlanetaryParams,
    },
    /// Generate a lightweight planetary preview: one polyline per positioned
    /// gear (sun, N planets, ring). Stateless; mirrors `GenerateGearPreview`.
    GeneratePlanetaryPreview {
        params: PlanetaryParams,
    },

    // -- Region selection (stateless) --
    /// Compute every minimal closed face of a solved sketch, so the UI can
    /// select the smallest region under a click (including sub-regions of
    /// overlapping shapes). Stateless: derived purely from the inputs.
    ComputeRegions {
        entities: Vec<SketchEntity>,
        #[serde(default, with = "u32_key_map")]
        solved_positions: HashMap<u32, (f64, f64)>,
        /// Relative chord tolerance for tessellating curved boundaries.
        #[serde(default)]
        chord_tolerance: Option<f64>,
    },
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
        /// Errors from features that failed during rebuild (feature_id, message).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        errors: Vec<(Uuid, String)>,
        /// Non-fatal warnings from rebuild (e.g., auto-union fallback).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        warnings: Vec<String>,
        /// Decimated preview mesh for thumbnail rendering (optional).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        preview_mesh: Option<feature_engine::preview_mesh::PreviewMesh>,
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

    /// STL export is ready (base64-encoded binary STL).
    StlExportReady { stl_data: String },

    /// Gear preview polyline generated.
    GearPreviewGenerated { polyline: Vec<(f64, f64)> },

    /// Full gear profile generated with sketch entities.
    GearProfileGenerated {
        entities: Vec<SketchEntity>,
        #[serde(with = "u32_key_map")]
        positions: HashMap<u32, (f64, f64)>,
        profiles: Vec<ClosedProfile>,
        pitch_radius: f64,
    },

    /// Minimal closed faces of a sketch, in selection order.
    RegionsComputed { regions: Vec<Region> },

    /// Planetary stage generated: positioned gears + derived radii + hints.
    PlanetaryGenerated { result: PlanetaryResult },

    /// Planetary preview generated: one polyline per gear (sun, N planets,
    /// ring). Empty when the params are invalid.
    PlanetaryPreviewGenerated { polylines: Vec<Vec<(f64, f64)>> },
}
