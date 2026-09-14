use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use feature_engine::types::{DesignParameter, FeatureTree, Operation};
use waffle_types::kernel::{EdgeRenderData, RenderMesh};
use waffle_types::{
    ClosedProfile, GearParams, GeomRef, PlanetaryParams, PlanetaryResult, ProjectedEntity, Region,
    SketchConstraint, SketchEntity, SolvedSketch,
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
// One message at a time; the fat variants (an assembly, a document) are
// the payload itself — boxing them would buy nothing (same call as
// `Operation` / `TabKind`).
#[allow(clippy::large_enum_variant)]
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
        /// Projected-geometry bindings (point id → external source). Empty for
        /// ordinary sketches. See specs/projected_sketch_geometry.md.
        #[serde(default)]
        projected: Vec<ProjectedEntity>,
        /// Provenance for the committed sketch, recorded in the same undo
        /// step (`specs/waffle_mcp_server.md` ICR-4). The app sends none.
        #[serde(default)]
        provenance: Option<feature_engine::types::Provenance>,
    },

    // -- Feature operations --
    /// Add a new feature to the feature tree.
    AddFeature {
        operation: Operation,
        /// Provenance recorded in the same undo step (ICR-4).
        #[serde(default)]
        provenance: Option<feature_engine::types::Provenance>,
    },
    /// Edit an existing feature's parameters.
    EditFeature {
        feature_id: Uuid,
        operation: Operation,
        /// Replaces the feature's provenance in the same undo step (ICR-4);
        /// absent leaves the record untouched.
        #[serde(default)]
        provenance: Option<feature_engine::types::Provenance>,
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
    /// Legacy single-tab save: the live tree as a one-tab v4 document (with
    /// the document's `sources` table attached). Tests and programmatic
    /// callers; the app saves through `SaveDocument`.
    SaveProject,
    /// Load a `.waffle` file (any version, migrated on the way in): the
    /// engine adopts the document's `sources` table, registers every usable
    /// embed into its source store, and rebuilds the active tab's tree.
    LoadProject {
        data: String,
    },
    /// v4 single writer (`specs/waffle_v4_document_model.md` §4 inv. 7): the
    /// UI hands over its document metadata and tab list — inactive tabs
    /// carry their trees, the active tab's tree is taken from the live
    /// engine — and the engine attaches its `sources` table (embeds from the
    /// source store per each entry's `pack`) and returns the verified file
    /// as `SaveReady`.
    SaveDocument {
        document: file_format::DocumentMetadata,
        tabs: Vec<file_format::Tab>,
        active_tab: String,
    },
    /// The host fetched a source's content through its locator (v4 §2.3):
    /// register it (hash recorded on the entry) and rebuild so dependent
    /// features recover from `SourceUnavailable`.
    ProvideSource {
        source_id: Uuid,
        data: String,
        /// The commit the host fetched `data` at (git locators): recorded
        /// as the entry's `resolved`, so `resolved.commit` and
        /// `content_hash` describe the same bytes (v4 §4 inv. 4).
        #[serde(default)]
        resolved_commit: Option<String>,
    },
    /// Edit a `sources` entry's policy or addressing (v4 §2.4 pin semantics,
    /// Phase 2 P2-4). `pack`: writer policy (refused `false` on an `Embedded`
    /// source — it has no origin to unpack to). `git_ref`: retarget a `Git`
    /// locator — pinning to the commit already resolved keeps the content;
    /// any other ref drops the content and `resolved` so the host re-resolves
    /// (never content from one commit labelled with another). "Update to tip"
    /// is not here: the host re-resolves the ref and answers `ProvideSource`
    /// with the new `resolved_commit`.
    UpdateSourceEntry {
        source_id: Uuid,
        #[serde(default)]
        pack: Option<bool>,
        #[serde(default)]
        git_ref: Option<file_format::GitRef>,
    },
    /// The document's `sources` table with per-entry availability (whether
    /// the engine's store holds the content). The host resolves the missing
    /// ones through their locators and answers with `ProvideSource`
    /// (v4 §2.3 content resolution order, Phase 2 P2-3).
    ListSources,
    /// Import a STEP file the host fetched through a locator (a git file
    /// URL, a share link): a LINKED `Step` source — not packed, with its
    /// content hash and resolved commit recorded — plus an ImportedBody
    /// feature naming it. The same file re-resolves from its origin on
    /// later opens.
    ImportStepFromLocator {
        file_name: String,
        locator: file_format::Locator,
        data: String,
        #[serde(default)]
        resolved_commit: Option<String>,
    },
    /// Open (or re-evaluate) an `Assembly` tab (Phase 3b): the UI hands over
    /// the tab's assembly and the feature trees of this document's Part tabs
    /// (the engine only ever holds one live tree); parts of linked `.waffle`
    /// sources come from the source store. Every distinct part is built once,
    /// connector frames are derived from the current geometry, placements
    /// are solved and returned as `ModelUpdated.assembly`; the instance
    /// bodies are then what the per-body accessors enumerate.
    OpenAssembly {
        assembly: feature_engine::assembly::AssemblyTree,
        #[serde(default)]
        part_trees: HashMap<String, FeatureTree>,
        /// This document's OTHER assembly tabs, so an instance may be of an
        /// assembly (a sub-assembly, 3d-2).
        #[serde(default)]
        assembly_trees: HashMap<String, feature_engine::assembly::AssemblyTree>,
    },
    /// The tabs of a linked `.waffle` source (for "add instance"): id, name
    /// and kind of each.
    ListSourceTabs {
        source_id: Uuid,
    },
    /// Can this pick carry a mate connector? Answered from the assembly
    /// ALREADY evaluated (no rebuild), so the app can refuse a pick at
    /// creation instead of minting a connector that silently resolves to a
    /// default frame — see `specs/assembly_connector_frame_resolver.md` §2.4.
    /// `instance_path` names the leaf whose part the reference belongs to.
    ProbeConnectorRef {
        instance_path: Vec<Uuid>,
        geom_ref: waffle_types::GeomRef,
    },
    /// Open a Part tab IN THE CONTEXT of an assembly (Phase 3d-4, in-context
    /// editing, v4 §2.8). `features` is the part's tree (it becomes the live
    /// tree); the assembly and this document's trees are evaluated exactly as
    /// for `OpenAssembly` — `part_trees` must therefore carry `features` under
    /// the edited part's tab id, so the assembly builds the part being edited
    /// from the same recipe. The instance at `instance_path` is the one being
    /// edited: the engine snapshots every OTHER leaf's geometry relative to
    /// its placement as the part's edit context (scoped `GeomRef`s resolve
    /// through it), renders those leaves as ghost bodies in the edited part's
    /// frame (their face and edge refs carry the scope), and reports
    /// `ModelUpdated.context`. Send it again to update the context; any
    /// `SwitchTab`/`OpenAssembly`/`LoadProject` drops it. The instance must be
    /// a same-document Part (a linked part is read-only).
    OpenPartInContext {
        features: FeatureTree,
        assembly_tab_id: String,
        instance_path: Vec<Uuid>,
        assembly: feature_engine::assembly::AssemblyTree,
        #[serde(default)]
        part_trees: HashMap<String, FeatureTree>,
        #[serde(default)]
        assembly_trees: HashMap<String, feature_engine::assembly::AssemblyTree>,
    },
    /// Fork of a linked document (v4 §7.1): rewrite every `Relative` source
    /// into an absolute `Git` locator in `base`'s repository, pinned at
    /// `commit` (the commit the link was opened at), so the copy's links keep
    /// resolving from the user's own storage. The UI mints the new
    /// `document.id` and saves through `SaveDocument` afterwards.
    RebaseSources {
        base: file_format::Locator,
        commit: String,
    },
    /// Import a STEP file as a new ImportedBody feature (task #138). `data`
    /// is the raw STEP text from the file picker; the engine compresses it
    /// into the feature's embedded payload. Placement starts at identity —
    /// edit the feature to position it.
    ImportStep {
        file_name: String,
        data: String,
    },
    /// A body's volume, surface area, bounding box and topology counts
    /// (`specs/waffle_mcp_server.md` ICR-1). Volume and area are exact from
    /// the B-Rep when the kernel can integrate the body, otherwise from the
    /// render mesh — the answer always says which. Query: no rebuild.
    MeasureBody {
        body_id: String,
    },
    /// Every face of a body as the `GeomRef` the viewport's face ranges carry,
    /// with its signature (`specs/waffle_mcp_server.md` ICR-3). `filter` uses
    /// the `TopoQuery` filter rules (`tie_break` is ignored: a listing returns
    /// every match). Query: no rebuild.
    ListFaces {
        body_id: String,
        #[serde(default)]
        filter: Option<waffle_types::TopoQuery>,
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

    // -- Design parameters (variables) --
    /// Replace the design-parameter table (the UI always sends the complete
    /// list) and rebuild. Undoable. Evaluated values/errors come back on the
    /// `ModelUpdated.feature_tree.parameters`.
    SetParameters {
        parameters: Vec<DesignParameter>,
    },
    /// Stateless: evaluate one expression against the current parameter
    /// table's cached values (mm-space result). Used by dialogs and the
    /// dimension input for live validation/preview.
    EvaluateExpression {
        expression: String,
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

/// One face of a `FacesListed` answer (ICR-3).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListedFace {
    pub geom_ref: waffle_types::GeomRef,
    pub signature: waffle_types::TopoSignature,
}

/// How a [`Measured`] quantity was obtained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MeasureMethod {
    /// Integrated exactly from the B-Rep by the kernel.
    Exact,
    /// Computed from the render mesh — chordal on curved faces, so curved
    /// volumes come out low. Never to be presented as exact.
    Mesh,
}

/// One measured quantity with its provenance (ICR-1, spec §2.6).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Measured {
    pub value: f64,
    pub method: MeasureMethod,
    /// Why the exact value was unavailable, verbatim from the kernel, when
    /// `method` is `Mesh`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exact_unavailable: Option<String>,
}

/// Messages from the engine (WASM Worker) to the UI (JavaScript main thread).
#[allow(clippy::large_enum_variant)] // see `UiToEngine`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum EngineToUi {
    /// The model has been rebuilt.
    ModelUpdated {
        /// The feature the command created or edited (`AddFeature`,
        /// `EditFeature`, `FinishSketch`, `ImportStep`; ICR-4), so a host
        /// learns the id without diffing trees. Absent for every other command.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        feature_id: Option<Uuid>,
        feature_tree: FeatureTree,
        meshes: Vec<RenderMesh>,
        edges: Vec<EdgeRenderData>,
        /// Errors from features that failed during rebuild (feature_id, message).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        errors: Vec<(Uuid, String)>,
        /// `errors`, typed (`specs/waffle_mcp_server.md` ICR-2): the same
        /// entries in the same order, each with a `kind` to branch on.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        feature_errors: Vec<feature_engine::types::FeatureError>,
        /// Non-fatal warnings from rebuild (e.g., auto-union fallback).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        warnings: Vec<String>,
        /// Decimated preview mesh for thumbnail rendering (optional).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        preview_mesh: Option<feature_engine::preview_mesh::PreviewMesh>,
        /// The document's `sources` table with availability (same rows as
        /// `SourcesListed`), so the UI's Sources panel is reactive.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        sources: Vec<SourceStatus>,
        /// Present while an `Assembly` tab is open: solved placements and
        /// the evaluation's problems (Phase 3b).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        assembly: Option<AssemblyStatus>,
        /// Present while a Part is open in the context of an assembly
        /// (`OpenPartInContext`, Phase 3d-4).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        context: Option<ContextStatus>,
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
        /// The failure's class when it is an engine error (ICR-2); absent for
        /// bridge-level failures such as a message sent in the wrong state.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        kind: Option<feature_engine::types::ErrorKind>,
    },

    /// Answer to `ListFaces` (ICR-3): ordered by canonical `GeomRef` JSON.
    FacesListed {
        body_id: String,
        faces: Vec<ListedFace>,
    },

    /// Answer to `MeasureBody` (ICR-1). Lengths in meters. The bounding box
    /// is taken from the render mesh (chord-inscribed on curved faces).
    BodyMeasured {
        body_id: String,
        volume_m3: Measured,
        surface_area_m2: Measured,
        bbox_min: [f64; 3],
        bbox_max: [f64; 3],
        face_count: usize,
        edge_count: usize,
        vertex_count: usize,
        /// Every edge bounds exactly two faces.
        closed: bool,
    },

    /// Save project is ready.
    SaveReady { json_data: String },

    /// Project loaded successfully.
    ProjectLoaded { feature_tree: FeatureTree },

    /// Answer to `ListSources`.
    SourcesListed { sources: Vec<SourceStatus> },

    /// Answer to `ListSourceTabs`.
    SourceTabsListed {
        source_id: Uuid,
        tabs: Vec<SourceTabInfo>,
    },

    /// Answer to `ProbeConnectorRef`: whether the pick derives a frame, what
    /// it was derived from (`"cylindrical face"`, `"circular edge"`, …), and
    /// the resolver's own reason when it does not.
    ConnectorRefProbed {
        ok: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        kind: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },

    /// STEP export is ready. `warnings` names anything the export left out
    /// (a mesh-backed imported body has no analytic geometry to write).
    ExportReady {
        step_data: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        warnings: Vec<String>,
    },

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

    /// Result of `EvaluateExpression`: exactly one of `value` (mm-space
    /// number) or `error` (user-facing message) is set.
    ExpressionEvaluated {
        value: Option<f64>,
        error: Option<String>,
    },

    /// Planetary stage generated: positioned gears + derived radii + hints.
    PlanetaryGenerated { result: PlanetaryResult },

    /// Planetary preview generated: one polyline per gear (sun, N planets,
    /// ring). Empty when the params are invalid.
    PlanetaryPreviewGenerated { polylines: Vec<Vec<(f64, f64)>> },
}

/// One `sources` entry as the host needs it to resolve content: the entry's
/// identity and addressing, and whether the engine already holds its bytes.
/// The embed blob is never sent (it is the content itself).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceStatus {
    pub id: Uuid,
    pub name: String,
    /// The source kind's `type` tag (`Waffle`, `Step`, … or an unknown one).
    pub kind: String,
    pub locator: file_format::Locator,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved: Option<file_format::Resolved>,
    /// Effective pack policy (§2.3).
    pub pack: bool,
    /// Whether the engine's source store holds the content.
    pub available: bool,
}

/// The evaluated assembly as the UI needs it (Phase 3b).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssemblyStatus {
    /// Solved placement per non-suppressed instance (derived hints; the UI
    /// writes them back into the tab for saving).
    pub placements: std::collections::BTreeMap<Uuid, feature_engine::assembly::Transform>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    /// Parts that were built (tab id, and source id for linked parts).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<feature_engine::assembly::PartRef>,
    /// Every connector's evaluated frame, in WORLD coordinates, so the
    /// viewport can draw it and the panel can label it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub connectors: Vec<ConnectorFrameInfo>,
}

/// One connector's evaluated frame for the UI: an orthonormal basis at a
/// point, in world coordinates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorFrameInfo {
    pub id: Uuid,
    /// What the frame was derived from (`"cylindrical face"`, …), absent for
    /// a connector carrying an explicit frame or one that failed to resolve
    /// (the failure is in `AssemblyStatus.errors`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    pub origin: [f64; 3],
    pub x_axis: [f64; 3],
    pub y_axis: [f64; 3],
    pub z_axis: [f64; 3],
}

/// The edit context a Part is open in (Phase 3d-4), as the UI needs it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextStatus {
    pub assembly_tab_id: String,
    /// The edited instance (its path in the assembly) and its display name.
    pub instance_path: Vec<Uuid>,
    pub instance_name: String,
    /// World placement of the edited instance at snapshot time.
    pub placement: feature_engine::assembly::Transform,
    /// The other instances rendered as ghosts.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub instances: Vec<ContextInstanceInfo>,
    /// The assembly evaluation's problems (same as `AssemblyStatus`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

/// One ghost instance of an edit context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextInstanceInfo {
    pub path: Vec<Uuid>,
    pub name: String,
    pub part_tab_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part_source_id: Option<Uuid>,
}

/// One tab of a linked document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceTabInfo {
    pub id: String,
    pub name: String,
    /// `Part`, `Assembly`, or an unknown kind's tag.
    pub kind: String,
}
