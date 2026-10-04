use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use feature_engine::types::{DesignParameter, FeatureTree, Operation};
use waffle_types::kernel::{EdgeRenderData, RenderMesh};
use waffle_types::{
    ClosedProfile, GearParams, GeomRef, PlanetaryParams, PlanetaryResult, ProjectedEntity, Region,
    SketchConstraint, SketchEntity, SolvedSketch, SprocketDimensions, SprocketParams,
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

/// The sketch state the UI is holding, as it travels with a sketch-operation
/// request (S1).
///
/// It is a `Sketch` minus the identity and the plane reference, which an
/// in-progress sketch does not have yet and no operation reads. Positions
/// come along because the operations run on the geometry the user is LOOKING
/// at — the solved positions — and not on the entities' declared
/// coordinates.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LiveSketch {
    pub entities: Vec<SketchEntity>,
    #[serde(default)]
    pub constraints: Vec<SketchConstraint>,
    #[serde(default, with = "u32_key_map")]
    pub solved_positions: HashMap<u32, (f64, f64)>,
    #[serde(default)]
    pub projected: Vec<ProjectedEntity>,
    #[serde(default = "default_origin")]
    pub plane_origin: [f64; 3],
    #[serde(default = "default_normal")]
    pub plane_normal: [f64; 3],
    #[serde(default)]
    pub plane_x_axis: Option<[f64; 3]>,
}

impl LiveSketch {
    /// This state as a `Sketch` the solver and the operations accept. The id
    /// and plane reference are placeholders: nothing in `sketch_solver::ops`
    /// reads either, and an in-progress sketch has no committed identity.
    pub fn to_sketch(&self) -> waffle_types::Sketch {
        waffle_types::Sketch {
            id: Uuid::nil(),
            plane: GeomRef {
                kind: waffle_types::TopoKind::Face,
                anchor: waffle_types::Anchor::Datum {
                    datum_id: Uuid::nil(),
                },
                selector: waffle_types::Selector::Role {
                    role: waffle_types::Role::ProfileFace,
                    index: 0,
                },
                policy: waffle_types::ResolvePolicy::BestEffort,
                scope: None,
            },
            plane_face: None,
            plane_origin: self.plane_origin,
            plane_normal: self.plane_normal,
            plane_x_axis: self.plane_x_axis,
            entities: self.entities.clone(),
            constraints: self.constraints.clone(),
            solve_status: waffle_types::SolveStatus::Unsolved,
            solved_positions: self.solved_positions.clone(),
            solved_profiles: Vec::new(),
            projected: self.projected.clone(),
        }
    }
}

/// A read-only question about sketch geometry (S1 previews).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SketchQuery {
    /// The connected run of curves through `seed`. `only_seed` is the
    /// Alt-click case: this entity alone.
    Chain {
        seed: u32,
        #[serde(default)]
        only_seed: bool,
    },
    /// The piece of a line a trim would take, and whether it has any cut at
    /// all (no intersection ⇒ the whole entity goes).
    TrimPreview { entity: u32, at: [f64; 2] },
    /// Where a fillet of `radius` would land; `radius` absent ⇒ the tool's
    /// default for that corner.
    FilletPreview {
        corner: u32,
        #[serde(default)]
        radius: Option<f64>,
    },
    /// The offset of a chain, as a polyline to draw. `cursor` derives the
    /// signed distance from a pointer position (the hover case); `distance`
    /// and `side` give it exactly (the typed-value case). Neither ⇒ the
    /// chain's own polyline, which is the unarmed hover ghost.
    OffsetPreview {
        chain: Vec<u32>,
        #[serde(default)]
        cursor: Option<[f64; 2]>,
        #[serde(default)]
        distance: Option<f64>,
        #[serde(default)]
        side: Option<waffle_types::Side>,
    },
}

/// The answer to a [`SketchQuery`]. `Refused` carries the typed reason's tag
/// (`"branching"`, `"radius-collapse"`, …) — the same vocabulary the
/// operations refuse with, so a caller never has to map two sets of strings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SketchQueryResult {
    Chain {
        ids: Vec<u32>,
        /// Whether the run closes on itself; `None` when it does not order
        /// at all (a branch), which is still a valid selection.
        closed: Option<bool>,
    },
    TrimPreview {
        /// Endpoints of the piece under the cursor.
        start: [f64; 2],
        end: [f64; 2],
        /// Cuts found on the entity. Zero ⇒ a trim removes it whole.
        cuts: u32,
    },
    FilletPreview {
        center: [f64; 2],
        radius: f64,
        tangent_a: [f64; 2],
        tangent_b: [f64; 2],
        /// The default radius for this corner, so the tool's popup can
        /// pre-fill it without a second round trip.
        default_radius: f64,
    },
    OffsetPreview {
        polyline: Vec<[f64; 2]>,
        closed: bool,
        /// Signed distance from `cursor` to the chain, when one was given:
        /// magnitude is the distance, sign is the side.
        #[serde(default)]
        signed_distance: Option<f64>,
        /// Entities in the chain — the hover hint's count.
        size: u32,
    },
    Refused {
        reason: String,
    },
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
/// A board STEP that accompanies a `.kicad_pcb` (`specs/kicad_board_link.md`
/// §2.1 `board_step`): with a locator it becomes a LINKED `Step` source,
/// without one an embedded copy. Its first-level products are the
/// component models (C3).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoardStepData {
    pub file_name: String,
    pub data: String,
    #[serde(default)]
    pub locator: Option<file_format::Locator>,
    #[serde(default)]
    pub resolved_commit: Option<String>,
}

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
        /// The sketch's own +u direction (`Sketch.plane_x_axis`). Absent ⇒
        /// the engine derives one from the normal, which is every sketch the
        /// UI authors.
        #[serde(default)]
        plane_x_axis: Option<[f64; 3]>,
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
    /// Give one entity a name (N1, `specs/agent_mechanical_design.md` §5.2).
    /// One undo step, no rebuild. The engine re-validates the name and the
    /// dotted body segment, so a host cannot store an unchecked one.
    SetEntityName {
        name: String,
        named: Box<feature_engine::names::NamedRef>,
    },
    /// Remove one entity name (N1).
    ClearEntityName {
        name: String,
    },
    /// The document's entity names, each with whether it still resolves (N1).
    /// `body_id` limits the answer to the names of one body.
    QueryEntityNames {
        #[serde(default)]
        body_id: Option<String>,
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
    SaveDocument,
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
    /// The content of one source the engine's store holds, as text — what
    /// the script editor opens (A-M4). Refused for a source whose content
    /// is not in the store (resolve it first).
    ReadSource {
        source_id: Uuid,
    },
    /// Add an EMBEDDED `Script` source to the document
    /// (`specs/custom_features_and_modeling_roadmap.md` §A7/§A8): the text
    /// as given, or one of the engine's built-in library scripts
    /// (`library`: `gear` | `sprocket`). Any text is accepted — a script
    /// that does not parse is an inert asset until a node names it, and the
    /// editor saves work in progress — the answer reports the header check
    /// so a host can show it. Not an undo step: sources are assets, not
    /// edits (v4 §2.3). `name` defaults to the header's `@feature name`,
    /// else `script.rhai`.
    AddScriptSource {
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        text: Option<String>,
        #[serde(default)]
        library: Option<String>,
    },
    /// Replace a `Script` source's text and rebuild, so every node naming
    /// it regenerates (the editor's Save, A-M4). Refused for a non-script
    /// source. Not an undo step (the editor keeps its own history); a node
    /// the new text breaks shows its typed error, never stale geometry
    /// (P10).
    SetScriptSource {
        source_id: Uuid,
        text: String,
    },
    /// Check a script without a node (A-M4 `script_run_check`): parse the
    /// header, compile, confirm the entry function. `text` checks unsaved
    /// text; `source_id` checks a stored script source. With `args` the
    /// script is also DRY-RUN — evaluated against a recorder with no
    /// kernel, so runtime errors, `ctx.fail`, limits and the output
    /// contract are exercised — and the recorded children are counted.
    CheckScript {
        #[serde(default)]
        source_id: Option<Uuid>,
        #[serde(default)]
        text: Option<String>,
        #[serde(default)]
        entry: Option<String>,
        #[serde(default)]
        args: Option<std::collections::BTreeMap<String, serde_json::Value>>,
    },
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
    /// Import a `.kicad_pcb` from a file picker / paste
    /// (`specs/kicad_board_link.md` §2.4): a packed `Embedded` `KicadPcb`
    /// source, a Board Part tab (Derived outline sketch + extrude + cutouts),
    /// one placeholder Part per footprint shape, and a Board assembly tab
    /// (one instance per footprint keyed by its uuid, a connector per
    /// mounting hole). The Board tab becomes active. A file the reader
    /// refuses lands nothing — not even the source.
    ImportKicad {
        file_name: String,
        data: String,
        /// The board's STEP export beside it (C3): its products become the
        /// component models, matched per footprint by reference designator.
        #[serde(default)]
        board_step: Option<BoardStepData>,
    },
    /// The same for a board the host fetched through a locator: a LINKED
    /// `KicadPcb` source (hashed, resolved commit recorded, not packed).
    LinkKicadFromLocator {
        file_name: String,
        locator: file_format::Locator,
        data: String,
        #[serde(default)]
        resolved_commit: Option<String>,
        #[serde(default)]
        board_step: Option<BoardStepData>,
    },
    /// What a linked KiCad board knows about a body or an assembly instance
    /// (`specs/kicad_board_link.md` §2.4, C4) — the hover card's question.
    /// `body_id` is a render body id (`"{instance…}/{feature}/{key}"` in an
    /// assembly, `"{feature}/{key}"` in a part); `instance_path` names an
    /// instance directly. An id that derives from no KiCad source answers
    /// with every field `None` — not an error, the card simply does not
    /// show.
    QueryEntityMeta {
        #[serde(default)]
        body_id: Option<String>,
        #[serde(default)]
        instance_path: Option<Vec<Uuid>>,
    },
    /// Open (or re-evaluate) an `Assembly` tab (Phase 3b): the UI hands over
    /// the tab's assembly and the feature trees of this document's Part tabs
    /// (the engine only ever holds one live tree); parts of linked `.waffle`
    /// sources come from the source store. Every distinct part is built once,
    /// connector frames are derived from the current geometry, placements
    /// are solved and returned as `ModelUpdated.assembly`; the instance
    /// bodies are then what the per-body accessors enumerate.
    OpenAssembly {
        /// The `Assembly` tab being opened. The session makes it active — a
        /// switch the tree-carrying `SwitchTab` cannot express, and without it
        /// the session's `active_tab` would go stale and the NEXT switch would
        /// stash the live tree onto the wrong tab (S2 C3a).
        ///
        /// The tab's assembly AND every part / sub-assembly tree it references
        /// come from the session (S2 C3b) — the UI used to re-send every Part
        /// tree on every evaluation, which was the largest payload on this
        /// wire. Parts of linked `.waffle` sources still resolve through the
        /// engine's source store.
        tab_id: String,
    },
    /// Open (or re-evaluate) a `Drawing` tab (D4a,
    /// `specs/drawings_and_mbd.md` §8): every view's source tab is built,
    /// projected and annotated, and the resulting layouts are written back
    /// onto the views as their caches and returned as
    /// `ModelUpdated.drawing`.
    ///
    /// The drawing sibling of [`UiToEngine::OpenAssembly`], on the same
    /// terms: the tab's content and every tree it references come from the
    /// session, so the message carries only the tab id, and the session is
    /// made to agree about which tab is active (without that the next switch
    /// would stash the live tree onto the wrong tab).
    OpenDrawing {
        tab_id: String,
    },
    /// Replace a `Drawing` tab's content — what every drawing TOOL's edit
    /// becomes. Re-evaluates the tab when it is the open one; an edit to a
    /// background drawing tab is recorded and shown when that tab opens.
    ///
    /// **Not for the page.** A drawing carries annotations, an annotation's
    /// anchors carry `u64` persistent ids, and a JSON number in JavaScript is
    /// an `f64` — so a whole drawing that went out to the page and came back
    /// would have every pid above `2^53` silently rounded, and every
    /// dimension anchored on one would refuse as "resolves to no geometry"
    /// (measured; see `waffle_types::pid_str`). The tools
    /// construct the `Drawing` in Rust, where `u64` is exact. The page uses
    /// [`UiToEngine::DrawingEdit`], which never carries one.
    EditDrawing {
        tab_id: String,
        drawing: feature_engine::drawing::Drawing,
    },
    /// One TARGETED edit to a `Drawing` tab (D4a) — what the page sends.
    ///
    /// Targeted rather than whole-document for the reason above: nothing in
    /// this message is a persisted document type, so nothing in it can be
    /// corrupted by a round trip through JavaScript. An annotation's anchors
    /// arrive as decimal strings.
    DrawingEdit {
        tab_id: String,
        edit: DrawingEdit,
    },
    /// Replace an `Assembly` tab's tree — the assembly panel's edits
    /// (instances, connectors, mates), which used to live only in the JS tab
    /// copy (S2 C3b). Re-evaluates when it is the tab on screen; an edit to a
    /// background tab is recorded and shown when that tab opens.
    EditAssembly {
        tab_id: String,
        assembly: feature_engine::assembly::AssemblyTree,
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
        /// The Part tab being opened in context: the session makes it active,
        /// for the same reason `OpenAssembly` names one (S2 C3a). Its tree is
        /// the session's copy of that tab — which IS the live tree once the
        /// switch has happened, so `features` is no longer on the wire, and
        /// neither are the assembly and the trees it references (S2 C3b).
        tab_id: String,
        assembly_tab_id: String,
        instance_path: Vec<Uuid>,
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
    /// The minimum distance between two operands, and the closest point on
    /// each (Q1 of `specs/agent_mechanical_design.md` §4.2). With `along`,
    /// the gap along that direction instead — negative when the operands
    /// overlap along it. Query: no rebuild.
    MeasureDistance {
        a: MeasureOperand,
        b: MeasureOperand,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        along: Option<[f64; 3]>,
    },
    /// Whether two bodies share interior volume, touch, or are apart (Q2 of
    /// `specs/agent_mechanical_design.md` §4.2). Answered by the kernel's own
    /// Intersect boolean, run on copies in a scratch arena. Query: no rebuild,
    /// and no change to the live kernel.
    MeasureInterference {
        /// Persistent body ids, as `model_summary` lists them.
        a: String,
        b: String,
    },
    /// Volume, surface area, centroid and the inertia tensor about the
    /// centroid of one body (Q3 of `specs/agent_mechanical_design.md` §4.2).
    ///
    /// `density_kg_m3` defaults to 1 — the document model carries no material
    /// table, so the answer reports which density it used rather than
    /// inventing a material. Query: no rebuild.
    MeasureMass {
        body_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        density_kg_m3: Option<f64>,
    },
    /// The cap loops of a planar section through each named body (Q4 of
    /// `specs/agent_mechanical_design.md` §4.2), as 2D curves in the cut
    /// plane's own frame.
    ///
    /// The plane arrives already resolved to an origin and a unit normal: the
    /// tool layer owns the shapes a plane can be NAMED by (a face reference, a
    /// datum, an N1 name), because those are the page's vocabularies. The kept
    /// half-space is the one the normal points away from, exactly as
    /// `KernelProjection::section_with_plane` defines it. Query: no rebuild.
    MeasureSection {
        /// Already expanded to concrete body ids by the tool — never "all",
        /// so the engine measures exactly what the answer names.
        body_ids: Vec<String>,
        plane_origin: [f64; 3],
        plane_normal: [f64; 3],
    },
    /// Sampled wall thickness of one body (Q5 of
    /// `specs/agent_mechanical_design.md` §4.2): rays cast inward from points
    /// on every face to the first face opposite.
    ///
    /// `spacing_m` asks for a denser sample than the default; the answer
    /// always reports the spacing it used. Query: no rebuild.
    MeasureThickness {
        body_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        spacing_m: Option<f64>,
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
    /// Every face, edge or vertex of one body with its full geometric content
    /// (Q6 of `specs/agent_mechanical_design.md` §4.2): the persistent id, the
    /// reference that names it, its N1 name, its signature, its analytic axis
    /// where it has one, an edge's exact arc length and a vertex's position.
    /// Query: no rebuild.
    ListEntities {
        body_id: String,
        kind: EntityListKind,
        #[serde(default)]
        filter: Option<EntityListFilter>,
    },
    ExportStep,
    /// An R12 DXF drawing. Query: no rebuild.
    ///
    /// Two shapes, by what is open (`specs/drawings_and_mbd.md` §8 / §12):
    ///
    /// - **A Part or Assembly tab**: one orthographic view of every live
    ///   body — §12's flat pattern, increment D1a. `view_dir` is the
    ///   direction of SIGHT, away from the viewer; absent means the top view
    ///   (`[0, 0, -1]`). `up` is which world direction points up on the
    ///   paper; absent lets the kernel pick one that is not parallel to
    ///   `view_dir`.
    /// - **A Drawing tab** (D4a): the SHEET — every view of it, each placed
    ///   at its own scale and position, in one file in sheet millimetres. A
    ///   `view_id` narrows it to one view, still at the view's own scale but
    ///   alone and at the paper origin, which is what a cutting table wants
    ///   from a sheet it should not read the rest of.
    ///
    /// `view_dir` / `up` on a Drawing tab, or `sheet_id` / `view_id` on a
    /// Part tab, are refused rather than ignored: each pair names a
    /// projection the other shape does not have, so honouring one and
    /// dropping the other would silently export a different drawing than the
    /// caller asked for.
    ExportDxf {
        #[serde(default)]
        view_dir: Option<[f64; 3]>,
        #[serde(default)]
        up: Option<[f64; 3]>,
        /// Which sheet of the open Drawing tab; absent means its first.
        #[serde(default)]
        sheet_id: Option<Uuid>,
        /// One view of that sheet, alone.
        #[serde(default)]
        view_id: Option<Uuid>,
    },
    ExportStl,
    /// Export a single body to STL. `body_id` is the persistent body identity
    /// (`FeatureTree::body_id` = `"{feature_id}/{output_key.tag()}"`).
    ExportBodyStl {
        body_id: String,
    },

    // -- Tab / document management --
    /// Make `tab_id` the active tab (S2 C3). The session stashes the live tree
    /// and the undo history into the outgoing tab and loads the incoming one's
    /// — the tree is NOT on the wire, because the session already holds every
    /// tab's. Switching to an `Assembly` tab leaves the live tree empty; the
    /// assembly itself is evaluated by `OpenAssembly`.
    SwitchTab {
        tab_id: String,
    },
    /// Append a tab of `kind` (`Part` or `Assembly`) and mint its id. `name`
    /// defaults to `"Part N"` / `"Assembly N"`, counting existing tabs of that
    /// kind exactly as the tab bar's + buttons do. The new tab is the LAST one
    /// of the `ModelUpdated.document.tabs` that answers — it does not become
    /// active; send `SwitchTab` for that.
    AddTab {
        /// `Part` or `Assembly`; any other kind is refused.
        kind: String,
        #[serde(default)]
        name: Option<String>,
    },
    /// Remove a tab and its parked undo history. Closing the ACTIVE tab makes
    /// its successor active (the next tab, or the last if it was last) and
    /// rebuilds. The document's last tab cannot be closed.
    CloseTab {
        tab_id: String,
    },
    RenameTab {
        tab_id: String,
        name: String,
    },
    /// Move a tab to `index` in the bar (0 = first); an index past the end
    /// moves it last, as `tab_move` documents.
    MoveTab {
        tab_id: String,
        index: usize,
    },
    /// Reset engine to a clean state (new document).
    NewDocument,

    // -- Settings --
    /// Set the document's name, its display unit (mm, cm, m, in, ft), or both
    /// (S2 C3). `modified` is stamped at save time, never here. This replaced
    /// `SetDisplayUnit`: the document's metadata has one home (the session's
    /// `DocumentMetadata`) and one message that writes it.
    SetDocumentMeta {
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        display_unit: Option<String>,
        /// The document's identity, and the creation time preserved from the
        /// file it was opened from (S2 C3c). Both are the HOST's to mint and
        /// latch — the storage record is keyed by the identity (v4 P2-5) — and
        /// the session needs them because it composes the saved file now.
        #[serde(default)]
        id: Option<Uuid>,
        #[serde(default)]
        created: Option<chrono::DateTime<chrono::Utc>>,
    },

    // -- Design parameters (variables) --
    /// Replace the design-parameter table (the UI always sends the complete
    /// list) and rebuild. Undoable. Evaluated values/errors come back on the
    /// `ModelUpdated.feature_tree.parameters`.
    SetParameters {
        parameters: Vec<DesignParameter>,
        /// `(old name, new name)` pairs whose dependents must follow
        /// (`specs/agent_mechanical_design.md` §6 P5). The table below
        /// already carries the new name; this is what tells the engine to
        /// rewrite every OTHER expression that reads the old one — other
        /// parameters and every `*_expr` field on the tree — through the
        /// AST. Absent (the pre-P5 shape, and every send that renames
        /// nothing) means no rewrite.
        #[serde(default)]
        renames: Vec<(String, String)>,
    },
    /// Stateless: evaluate one expression against the current parameter
    /// table's cached values (mm-space result). Used by dialogs and the
    /// dimension input for live validation/preview.
    ///
    /// `dimension` is the kind of field the caller means it for (P1). When
    /// set, an expression whose committed dimension does not fit — `25deg`
    /// for a `Length` — comes back as an error instead of a number, which
    /// is the same refusal the rebuild would make at that field.
    EvaluateExpression {
        expression: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        dimension: Option<feature_engine::expr::Dimension>,
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
    /// Generate a sprocket preview polyline for live rendering. Stateless;
    /// mirrors `GenerateGearPreview`. Invalid parameters are an
    /// `InvalidRequest` naming the offending value.
    GenerateSprocketPreview {
        params: SprocketParams,
    },
    /// Generate a full ISO 606 sprocket profile with sketch entities (points
    /// and arcs) and its kernel-ready profile. Stateless; mirrors
    /// `GenerateGearProfile`.
    GenerateSprocketProfile {
        params: SprocketParams,
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

    /// Apply sketch operations (S1, `specs/agent_mechanical_design.md` §10.1)
    /// to the sketch state the UI is holding, and answer with the result.
    ///
    /// The live state travels WITH the request, exactly as `SolveSketch`'s
    /// does, because the UI is the owner of the in-progress sketch and the
    /// engine's `active_sketch` lags it by a round trip. The engine is the
    /// owner of the GEOMETRY: what the operations decide, not where the
    /// pointer was.
    ApplySketchOps {
        live: LiveSketch,
        ops: Vec<waffle_types::SketchOp>,
        /// The UI's own entity-id counter, so minted ids cannot collide with
        /// one it has already handed out. `0` ⇒ derive the floor from the
        /// sketch.
        #[serde(default)]
        next_id: u32,
    },

    /// Ask about sketch geometry without changing anything: the connected
    /// chain through an entity, and the trim / fillet / offset previews.
    ///
    /// A preview computed by different code from the commit is a preview that
    /// can lie, so the hover feedback and the operation answer come from one
    /// implementation (§10.1).
    QuerySketch {
        live: LiveSketch,
        query: SketchQuery,
    },

    // -- Agent tools --
    /// Run one agent tool (`specs/waffle_server_mode.md` §2.3 S3). The
    /// semantics live in [`crate::tools`], so the page and a native host
    /// answer identically; the gates that are page state (the engine lock,
    /// `UserBusy`, `AgentPaused`) stay with the host (§3.3).
    Tool {
        name: String,
        #[serde(default)]
        arguments: serde_json::Value,
        /// Per-call host state (the agent's name, for provenance). Read from
        /// the authoring tools onward; the read-only tools ignore it.
        #[serde(default)]
        context: Option<serde_json::Value>,
    },
}

/// The open document as the Rust session knows it
/// (`specs/waffle_server_mode.md` §2.3 S2, checkpoint C2): what a host needs
/// to show a tab bar and name a document state, and what C4 turns the JS
/// store's `$state` into a mirror of. Never a tab's tree — the active tab's
/// is `ModelUpdated.feature_tree`, and an inactive tab's is not display data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentInfo {
    pub id: Uuid,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_unit: Option<String>,
    /// Preserved from the file the document was opened from, never re-stamped
    /// (S2 C4): the store used to parse it out of the `.waffle` itself, and
    /// that second parser is what C4 deletes.
    pub created: chrono::DateTime<chrono::Utc>,
    pub tabs: Vec<crate::session::TabInfo>,
    pub active_tab: String,
    /// The OPEN tab's assembly, when it is an `Assembly` tab (S2 C4b).
    ///
    /// The tab list carries no content, by design — but the assembly panel
    /// edits this tree live (instances, connectors, mates), and once the store
    /// stops keeping its own tab copies it has no other source. Only the open
    /// tab's: an inactive tab's assembly is not display data, and the session
    /// supplies it to every message that needs it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assembly_tree: Option<feature_engine::assembly::AssemblyTree>,
    /// Increments on every committed mutation, so a host can name the state a
    /// viewer holds (spec §4.1).
    pub revision: u64,
}

/// One face of a `FacesListed` answer (ICR-3).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListedFace {
    pub geom_ref: waffle_types::GeomRef,
    pub signature: waffle_types::TopoSignature,
    /// The N1 entity name pointing at this face, when one does
    /// (`specs/agent_mechanical_design.md` §5.2: every result carrying a
    /// `GeomRef` also carries `name`). Absent when the face is unnamed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Which topology [`UiToEngine::ListEntities`] lists (Q6).
///
/// Its own enum rather than `waffle_types::TopoKind`: that type is serde-
/// tagged (`{"type":"Face"}`) and carries `Shell`/`Solid`, neither of which is
/// a listable entity. One lowercase token is what a tool argument wants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityListKind {
    Face,
    Edge,
    Vertex,
}

impl EntityListKind {
    pub fn topo(self) -> waffle_types::TopoKind {
        match self {
            EntityListKind::Face => waffle_types::TopoKind::Face,
            EntityListKind::Edge => waffle_types::TopoKind::Edge,
            EntityListKind::Vertex => waffle_types::TopoKind::Vertex,
        }
    }
}

/// What narrows an [`UiToEngine::ListEntities`] answer (Q6 §4.3).
///
/// Every field is independent and they COMPOSE: an entity is listed only if
/// it passes all of the ones that are present. `query` is the same
/// `TopoQuery` vocabulary `ListFaces` already takes, so one filter language
/// serves both listings and `Selector::Query`; the other two ask about
/// things a `TopoSignature` cannot express.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EntityListFilter {
    /// The `TopoQuery` filter rules (`SurfaceType`, `NormalDirection`,
    /// `NearPoint`, `AreaRange`). `tie_break` is ignored: a listing returns
    /// every match.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<waffle_types::TopoQuery>,
    /// A glob over the entity's N1 name: `*` matches any run of characters
    /// and `?` any single one. An entity with no name never matches a glob.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// `[min, max]` in meters. Keeps only entities whose own bounding box
    /// lies INSIDE this box, boundary included — "what is in this region",
    /// not "what reaches into it". An entity whose signature carries no bbox
    /// is excluded rather than assumed to fit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bbox: Option<[[f64; 3]; 2]>,
}

/// One entity of an `EntitiesListed` answer (Q6).
///
/// `signature`, `axis`, `length` and `position` are all present or absent by
/// KIND rather than by luck: a face has no `length`, a vertex no `axis`. A
/// field that is absent for a reason the caller should know about says so
/// (`length_unavailable`), because a missing number and an impossible one are
/// different facts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListedEntity {
    /// The entity's persistent id, and the id its geometry was introduced
    /// under (`waffle_types::kernel::EntityPid`). `null` for a kernel with no
    /// persistent identity for this body — a mesh-backed import — never a
    /// fabricated number.
    ///
    /// Decimal STRINGS, like every pid that crosses this boundary
    /// (`waffle_types::pid_str`): these ids are content-seeded `u64`s, a
    /// gear body hands out a thousand of them, and a JSON number in
    /// JavaScript rounds the ones above `2^53` onto a different entity.
    /// Q6 shipped them as numbers; the `is_u64()` assertion in
    /// `tests/tool_entity_list.rs` is now `is_string()`.
    #[serde(
        default,
        with = "waffle_types::pid_str::option",
        skip_serializing_if = "Option::is_none"
    )]
    pub pid: Option<u64>,
    #[serde(
        default,
        with = "waffle_types::pid_str::option",
        skip_serializing_if = "Option::is_none"
    )]
    pub root_pid: Option<u64>,
    /// The reference that names this entity. For a FACE this is exactly the
    /// ref `face_list` and the viewport hand out, so the two tools cannot
    /// drift; for an edge or a vertex it is a `Selector::Pid` ref, which is
    /// the durable identity D0 gave them. `null` when neither exists.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geom_ref: Option<waffle_types::GeomRef>,
    /// The N1 name pointing at this entity, when one does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// What the name's resolution had to say, verbatim — empty when the name
    /// reached this entity by its persistent id, which is the normal case.
    ///
    /// Non-empty means N1's LOUD FALLBACK fired: the pid the name was stored
    /// over is gone and the name was rebound through the reference it was
    /// authored with, which matches by geometry and may well be naming a
    /// different entity than the user meant. N1 reports that on
    /// `names_list`; a listing that showed the bare `name` beside it would be
    /// the one place the warning disappeared, so it travels here too.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub name_warnings: Vec<String>,
    /// The N0 signature: surface type, area, centroid, normal, bbox, and the
    /// rotation-invariant `AxisDescriptor` of a surface of revolution.
    pub signature: waffle_types::TopoSignature,
    /// The analytic axis LINE of a rotational face or a circular/elliptical
    /// edge — a point on it plus a direction, which the signature's
    /// `AxisDescriptor` deliberately does not carry (it is rotation- and
    /// position-invariant by design). `null` for a plane, whose orientation
    /// is its `signature.normal`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub axis: Option<ListedAxis>,
    /// An edge's ARC length (Q6), never its chord. Faces and vertices have
    /// none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub length: Option<ListedLength>,
    /// Why this edge has no `length` — the kernel's own refusal, verbatim.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub length_unavailable: Option<String>,
    /// A vertex's position in meters. Faces and edges have none (their
    /// `signature.centroid` is the comparable field).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<[f64; 3]>,
}

/// The analytic axis of one entity (`KernelIntrospect::entity_axis`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListedAxis {
    /// `cylindrical`, `conical`, `spherical`, `toroidal`, `circular` or
    /// `elliptical`.
    pub kind: String,
    /// A point ON the axis — the cylinder's axis point, the cone's apex, the
    /// sphere's/torus's centre, the circle's centre.
    pub origin: [f64; 3],
    /// The unit axis direction, and `null` for `spherical`: a sphere has no
    /// intrinsic axis, so it carries a CENTRE and nothing else. (The kernel's
    /// own `EntityAxis` fills a canonical pole there to keep its field
    /// infallible; publishing that as the sphere's direction would make an
    /// agent believe a frame the geometry does not have, and would contradict
    /// the `null` that `signature.axis.direction` reports for the same face.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direction: Option<[f64; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub radius: Option<f64>,
}

/// One edge's arc length and the tier it is (Q6).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListedLength {
    /// Meters, along the curve.
    pub arc_length_m: f64,
    /// `line`, `circle`, `arc`, `ellipse_arc`, `hyperbola_arc`,
    /// `surface_pair` or `polyline`.
    pub curve_type: String,
    /// Whether the edge closes on itself (a full circle or ellipse).
    pub closed: bool,
    pub method: LengthTierWire,
    /// `quadrature` only: the measured difference against the same
    /// quadrature at twice the step count.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub residual_m: Option<f64>,
    /// `chords` only, and only when the sampler was ours: the band on each
    /// sample point. Absent for an imported body's polyline, whose source
    /// tolerance we do not know.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chord_bound_m: Option<f64>,
}

/// How an arc length was obtained (Q6) — the wire form of
/// `waffle_types::kernel::LengthMethod`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LengthTierWire {
    /// A closed form: a chord, `2πr`, `rΔθ`.
    Exact,
    /// A converged quadrature of a closed-form speed whose integral is
    /// elliptic (an ellipse or hyperbola arc). `residual_m` is the witness.
    Quadrature,
    /// The sum of a sampled polyline's chords — a LOWER bound on the true
    /// length.
    Chords,
}

/// The body-level frame an `EntitiesListed` answer carries (Q6): the mass
/// properties' principal axes, reused from Q3 rather than recomputed.
///
/// Every field is `null` together when the kernel refuses the integration (a
/// mesh-backed import), and `unavailable` then says why — a listing must not
/// fail because a body has no closed-form moments.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListedBodyFrame {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub centroid: Option<[f64; 3]>,
    /// The inertia tensor's eigenvalues, ascending, at unit density.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub principal_moments: Option<[f64; 3]>,
    /// The unit eigenvector of each, as rows, right-handed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub principal_axes: Option<[[f64; 3]; 3]>,
    /// Which tier those numbers are — Q3's own, carried, not re-derived.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<MeasureMethod>,
    /// The kernel's refusal, verbatim, when there are no axes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unavailable: Option<String>,
}

/// One operand of [`UiToEngine::MeasureDistance`] (Q1 §4.3): a whole body, a
/// face / edge / vertex named by the `GeomRef` `face_list` hands out, an N1
/// entity name, or a free point in space (meters).
///
/// An axis operand is not in Q1 — the kernel refuses it, typed, rather than
/// approximating it as a long segment.
// A `GeomRef` operand dwarfs a point one, as it does in every message that
// carries a reference (see `UiToEngine`): two of these exist per call, and
// boxing one arm would buy nothing but a serde indirection.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MeasureOperand {
    Body {
        body_id: String,
    },
    Entity {
        geom_ref: waffle_types::GeomRef,
    },
    Point {
        point: [f64; 3],
    },
    /// An entity or body by its N1 name (`specs/agent_mechanical_design.md`
    /// §5.2): the name is resolved to the reference it labels, so a measure
    /// reads whatever the name points at today.
    Name {
        name: String,
    },
}

/// What an N1 name can be assigned to, or looked up through — the `EntityRef`
/// of `specs/agent_mechanical_design.md` §5.2.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
pub enum EntityTarget {
    /// A face, edge or vertex, by the `GeomRef` `face_list` hands out.
    Entity { geom_ref: waffle_types::GeomRef },
    /// A whole body, by the id `model_summary` reports. Naming one sets its
    /// DISPLAY name (the mechanism `body_rename` already owns), because that
    /// name is what a dotted entity name's first segment has to match.
    Body { body_id: String },
    /// An entity that already has a name — re-labelling, in one step.
    Name { name: String },
}

/// One entry of an `EntityNamesListed` answer (N1).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListedName {
    pub name: String,
    pub kind: waffle_types::TopoKind,
    /// The reference the name points at. `null` for a BODY name, which is a
    /// display name rather than an entry in the name table (`kind` is
    /// `Solid` and `body_id` names it).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geom_ref: Option<waffle_types::GeomRef>,
    /// The body the name lives in, by persistent body id, when it is known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_id: Option<String>,
    /// That body's CURRENT display name. A dotted name whose first segment no
    /// longer equals this has drifted (the body was renamed after the name was
    /// assigned); the name still resolves, because its identity was never in
    /// the label.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    /// Whether the reference resolves against the geometry as it stands.
    pub resolves: bool,
    /// Which reference answered: `pid`, `selector`, or `query` (the authored
    /// fallback, meaning the persistent id is gone). `null` when the name does
    /// not resolve, and for a body name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_by: Option<feature_engine::names::ResolvedBy>,
    /// Which RUNG of the ladder answered (N2 §5.3): `pid`, `pid_root`, `role`,
    /// `signature`, `query`, `position`, or one of the `BestEffort` rebinds
    /// (`role_clamped`, `signature_low_confidence`, `query_first_of_kind`,
    /// `kind_fallback`, `position_nearest`). `resolved_by` says which stored
    /// reference answered; this says how. Together they close N1's open item:
    /// `pid` no longer hides whether the id answered directly or through its
    /// lineage root. `null` when the name does not resolve.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_via: Option<feature_engine::resolve::ResolvedVia>,
    /// True when the name points at something that is NOT the identity it
    /// recorded: the stored persistent id was gone and the authored fallback
    /// answered, or the rung that answered was a `BestEffort` rebind. The one
    /// flag worth branching on — `resolves: true` alone does not mean the name
    /// still means what it did.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub rebound: bool,
    /// Why the recorded identity stopped answering, when something else did
    /// (N2 §5.3). `PidGone` here with `resolves: true` says: the face this
    /// name was given to is gone, and the entity reported is whatever the
    /// authored selector found instead.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lost_identity: Option<feature_engine::types::ResolutionReason>,
    /// The typed classification of a refusal (N2 §5.3 item 2), so an agent
    /// branches on `NoMatch` / `Ambiguous` / `PidGone` / `ScopeMissing` instead
    /// of reading `warnings`. `null` when the name resolves, and for a refusal
    /// the ladder did not classify.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refusal: Option<feature_engine::types::ResolutionReason>,
    /// Why it does not resolve, or what the resolver warned about — verbatim.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created: Option<feature_engine::types::Provenance>,
}

/// What a measured closest point lies on (Q1).
///
/// `kernel_id` is the kernel's TRANSIENT entity id: stable within this kernel
/// session only, never to be persisted — a durable reference is a `GeomRef`.
/// It is here so a caller can tell two answers apart and match a point to an
/// entity it already listed in the same session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeasuredOn {
    pub kind: waffle_types::TopoKind,
    pub kernel_id: u64,
}

/// A closest-point witness nested inside a Q2 answer — the same content
/// [`EngineToUi::DistanceMeasured`] carries, as a struct because here it is a
/// field rather than a whole message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeasuredGap {
    pub value_m: f64,
    pub method: MeasureMethod,
    /// The tessellation band in meters; the bound on `value_m` when `method`
    /// is `mesh`.
    pub chord_bound_m: f64,
    pub points: [[f64; 3]; 2],
    pub on: [Option<MeasuredOn>; 2],
}

/// One lump of an intersection region (Q2).
///
/// The region solid itself is not returned: the Intersect runs in a scratch
/// arena that is dropped with the answer, so it has no id in the live kernel
/// (the spec's `keep_region` waits for a kernel that can adopt a solid across
/// arenas). These numbers are what say WHERE the collision is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InterferenceRegion {
    pub volume_m3: f64,
    pub centroid: [f64; 3],
    pub aabb_min: [f64; 3],
    pub aabb_max: [f64; 3],
}

/// Why a Q2 answer is `contact`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContactEvidenceWire {
    /// The regularized Intersect was empty and the measured gap is zero: the
    /// bodies meet on a shared face, edge or vertex.
    EmptyIntersectionAtZeroDistance,
    /// The Intersect produced a body at or under the minimum-feature volume
    /// floor — a sliver, not shared interior.
    SliverIntersection,
}

/// Answer to [`UiToEngine::MeasureInterference`] (Q2). Three outcomes and no
/// fourth: a boolean the kernel could not run is an ERROR, never `disjoint`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MeasuredInterference {
    /// The bodies share interior volume.
    Interferes {
        /// The total, m³ — the sum over `regions`.
        volume_m3: f64,
        method: MeasureMethod,
        chord_bound_m: f64,
        regions: Vec<InterferenceRegion>,
    },
    /// They touch but share no interior.
    Contact {
        evidence: ContactEvidenceWire,
        /// Present when the evidence is a sliver: the sliver's volume.
        #[serde(skip_serializing_if = "Option::is_none")]
        sliver_volume_m3: Option<f64>,
        closest: MeasuredGap,
    },
    /// They do not touch. `distance` is Q1's answer, always filled in.
    Disjoint { distance: MeasuredGap },
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

// ---------------------------------------------------------------------------
// Q4 — section as data
// ---------------------------------------------------------------------------

/// One curve of a section cap loop (Q4): the serde-able form of
/// `waffle_types::kernel::projection::Curve2`.
///
/// `Curve2` is built on `cad_primitives::Point2`, which has no serde
/// implementation, so the kernel's analytic arms cannot cross this boundary as
/// themselves. This enum is a one-to-one mirror of them — no arm is collapsed
/// and nothing is flattened to a polyline on the way out, because a cap
/// bounded by a circle is a different drawing (and a different area) from a
/// cap bounded by 64 chords, and an agent that re-derives geometry from the
/// answer must get the circle.
///
/// Every coordinate is in the cut plane's `(u, v)` frame
/// ([`SectionBasis`]) and in meters. Angles and parameters are
/// counter-clockwise in that frame, and `start < end` always — a cap loop is a
/// point set to hatch, so the walk direction lives in `curves`' ORDER rather
/// than in each curve.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SectionCurve {
    /// A curve the section degenerated to a single point.
    Point {
        at: [f64; 2],
    },
    Line {
        start: [f64; 2],
        end: [f64; 2],
    },
    /// A circular arc, or a full circle when `end_angle_rad − start_angle_rad`
    /// is 2π. Angles from `+u`.
    Circle {
        center: [f64; 2],
        radius: f64,
        start_angle_rad: f64,
        end_angle_rad: f64,
    },
    /// An elliptical arc, or a full ellipse when `end_param − start_param` is
    /// 2π. The point set is
    /// `center + major_radius·cos t·major_axis + minor_radius·sin t·perp(major_axis)`,
    /// `perp((x, y)) = (−y, x)`.
    Ellipse {
        center: [f64; 2],
        major_axis: [f64; 2],
        major_radius: f64,
        minor_radius: f64,
        start_param: f64,
        end_param: f64,
    },
    /// What the kernel could not keep analytic, sampled at the render chord
    /// density. `closed` means the last point joins the first (the list does
    /// not repeat it).
    Polyline {
        points: Vec<[f64; 2]>,
        closed: bool,
    },
}

/// Whether a cap loop bounds material or a hole in it (Q4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SectionLoopKind {
    /// Counter-clockwise in the cap frame: positive signed area.
    Outer,
    /// Clockwise: negative signed area, a hole in the cap.
    Hole,
}

/// One cap boundary loop of a section (Q4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SectionCapLoop {
    /// The loop's curves in B-Rep walk order: consecutive curves share an
    /// endpoint and the last shares one with the first.
    pub curves: Vec<SectionCurve>,
    /// Green's-theorem area in the cap frame — positive for an outer loop,
    /// negative for a hole.
    pub signed_area_m2: f64,
    /// Whether every curve of this loop stayed analytic, so `signed_area_m2`
    /// is exact. False means at least one curve is a sampled `polyline`, whose
    /// chord polygon under-counts the area it bounds by its own sagitta
    /// deficit.
    pub exact: bool,
    pub kind: SectionLoopKind,
}

/// The frame a section's loops are expressed in (Q4) — the kernel's own, never
/// re-derived.
///
/// `origin` is the plane origin the caller passed and the line of sight `w` is
/// the NEGATED plane normal: the viewer stands on the discarded side and looks
/// at the cap, which is the drafting convention and the frame that makes an
/// outer loop's area positive. A world point is `origin + u·u_axis +
/// v·v_axis`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SectionBasis {
    pub origin: [f64; 3],
    pub u_axis: [f64; 3],
    pub v_axis: [f64; 3],
    /// The line of sight, away from the viewer — the negated plane normal.
    pub w_axis: [f64; 3],
}

/// One body's section (Q4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SectionedBody {
    pub body_id: String,
    /// The cap's boundary loops, outer and holes. EMPTY when the plane misses
    /// this body — which is a typed answer, not a failure: `kept_material`
    /// says which side it missed on.
    pub loops: Vec<SectionCapLoop>,
    /// Net cap area in m²: outer loops minus holes, the sum of
    /// `signed_area_m2`.
    pub area_m2: f64,
    /// The cap's area centroid in the cap frame `(u, v)`, meters. `null` when
    /// the cap has no area.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub centroid_uv: Option<[f64; 2]>,
    /// The same point in world coordinates, meters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub centroid: Option<[f64; 3]>,
    /// Whether the centroid is exact. It is computed by Green's theorem over
    /// each loop FLATTENED at the render chord density — the areas the kernel
    /// reports are closed-form, but a first moment over a circular arc is not
    /// a quantity `SectionLoop` carries — so it is exact only when every curve
    /// of every loop is a `line`, where the flattened polygon IS the cap.
    pub centroid_exact: bool,
    /// `exact` when every loop is, else `mesh`: the tier of `area_m2`.
    pub method: MeasureMethod,
    /// Whether at least one cap face was identified by its PLANE rather than
    /// by its descent from the cutting half-space — the §4.5.5 Stage-0
    /// signature of a cut plane COPLANAR with a face of this body. A coplanar
    /// cut is a legitimate section, and this is the one configuration where
    /// the kernel's own lineage cannot name the cap, so it is reported rather
    /// than left to look like a section that quietly found nothing.
    pub cap_shared_with_model: bool,
    /// Whether the cut kept any material at all. `false` with no loops means
    /// the plane missed the body on the DISCARDED side (the whole body is
    /// gone); `true` with no loops means it missed on the KEPT side.
    pub kept_material: bool,
}

/// Why one body produced no section (Q4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SectionDeclineKind {
    /// A kernel capability wall — a Stage-0 coplanar refusal the overlay
    /// could not resolve, a curved partial-patch operand, a mesh-backed
    /// imported body. A caller must not retry it with different numbers.
    NotSupported,
    /// The boolean ran and STOPped, or the plane was refused. Loud, and never
    /// folded into an empty section.
    Failed,
}

/// One body the section DECLINED to answer for (Q4).
///
/// A decline is never an empty `loops`: "the plane misses this body" and "the
/// kernel could not cut this body" are different answers, and a wall-thickness
/// or clearance decision made on the second one read as the first would be
/// made on no evidence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SectionDecline {
    pub body_id: String,
    pub kind: SectionDeclineKind,
    /// The kernel's own refusal, verbatim.
    pub reason: String,
}

// ---------------------------------------------------------------------------
// Q5 — sampled wall thickness
// ---------------------------------------------------------------------------

/// One end of a measured thickness (Q5): the face it sits on, and how that
/// face can be named.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThicknessFace {
    /// The kernel's TRANSIENT face id — stable within this kernel session
    /// only, never to be persisted.
    pub kernel_id: u64,
    /// The face's persistent id, as a DECIMAL STRING (`waffle_types::pid_str`):
    /// content-seeded `u64`s above `2^53` do not survive a JSON number.
    #[serde(
        default,
        with = "waffle_types::pid_str::option",
        skip_serializing_if = "Option::is_none"
    )]
    pub pid: Option<u64>,
    #[serde(
        default,
        with = "waffle_types::pid_str::option",
        skip_serializing_if = "Option::is_none"
    )]
    pub root_pid: Option<u64>,
    /// The N1 name pointing at this face, when one does.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// The thinnest site a thickness sample found (Q5).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThinnestSite {
    pub thickness_m: f64,
    /// Where the ray started, on `from` — in meters, world frame.
    pub point: [f64; 3],
    /// Where it landed, on `to`.
    pub opposite: [f64; 3],
    pub from: ThicknessFace,
    pub to: ThicknessFace,
    /// Whether `from` and `to` are two DISTINCT faces that share an edge — so
    /// this reading crossed a CORNER rather than a wall.
    ///
    /// Two faces meeting at an edge enclose a wedge that goes to zero at the
    /// edge, so the number measures how close the site got to it. `false` for
    /// a site that hit its own face: a solid cylinder's diameter, measured
    /// across its own lateral face, is a wall.
    pub faces_share_an_edge: bool,
}

/// One bar of the thickness histogram (Q5). Bins are equal-width over
/// `[min, max]`; `count` is sites, not area.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ThicknessBin {
    pub lo_m: f64,
    pub hi_m: f64,
    pub count: usize,
}

/// Sites that produced no thickness, by reason (Q5).
///
/// Counted rather than dropped: a body whose casts mostly fail has a thickness
/// answer covering less of it than the sample count suggests, and nothing else
/// in the answer would say so.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ThicknessDeclines {
    /// The inward ray left the body without hitting a face — a non-closed
    /// shell, or a cast that grazed out along a tangency.
    pub no_hit: usize,
    /// The only hit was on the site's OWN face within the local sagitta of
    /// the facet the site came from, so it could not be told from the ray's
    /// own start. A thickness that small is below what a render-density cast
    /// can resolve.
    pub below_self_band: usize,
    /// The face carries no analytic surface to take an inward normal from.
    pub no_surface: usize,
}

/// Answer to [`UiToEngine::MeasureThickness`] (Q5).
///
/// **Every number here is SAMPLED.** `method` is `sampled` and nothing else:
/// a wall thinner than `spacing_m` between two sample sites can be missed
/// entirely, so `min_m` is an upper bound on the body's true minimum wall and
/// is never presented as the medial-axis answer.
///
/// Two minima cross the wire. `min_m` is the shortest cast anywhere, which any
/// acute edge drives towards zero; `min_wall_m` is the shortest cast between
/// two faces that do NOT meet at an edge, which is the wall. A rule reads the
/// second.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeasuredThickness {
    pub body_id: String,
    /// The shortest cast anywhere on the body — §4.2's question as posed.
    /// Any ACUTE edge drives it towards zero (a 4 mm slot through a 10/7 mm
    /// tube reports 0.043 mm here), so a WALL decision reads `min_wall_m`.
    pub min_m: f64,
    /// The shortest cast that did NOT cross a corner — the thinnest wall.
    /// Absent when every site crossed one, which is a body the sample found
    /// no wall in rather than a body with a zero wall.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_wall_m: Option<f64>,
    /// The unweighted mean over sites. The sites are approximately
    /// area-uniform (every facet is subdivided to `spacing_m`), so this
    /// approximates the area-weighted mean wall.
    pub mean_m: f64,
    pub max_m: f64,
    pub thinnest: ThinnestSite,
    /// Where `min_wall_m` is. Absent on the same bodies that number is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinnest_wall: Option<ThinnestSite>,
    pub histogram: Vec<ThicknessBin>,
    /// Sites that produced a thickness.
    pub samples: usize,
    /// The sample spacing used, meters — the largest gap between neighbouring
    /// sites on one face.
    pub spacing_m: f64,
    /// The tessellation band the sites were derived at, meters.
    pub chord_bound_m: f64,
    /// Sites whose refined hit was certified on the analytic surface pair.
    /// The rest kept their facet hit, which is inside the true surface by at
    /// most `chord_bound_m`.
    pub refined: usize,
    pub declines: ThicknessDeclines,
    /// `sampled`, always. The field exists so the tier is read rather than
    /// assumed, and so a later exact method has somewhere to say it is one.
    pub method: ThicknessMethod,
}

/// How a thickness was obtained (Q5). One arm today, deliberately: a sampled
/// answer must never be able to serialize as an exact one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThicknessMethod {
    Sampled,
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
        /// The subset of `warnings` a FEATURE raised, with its id (N2 §5.3
        /// item 4). The feature tree puts a warning glyph on that row, and a
        /// reference that rebound or a sketch whose face moved is exactly the
        /// state a user needs to see on the feature rather than in a toast
        /// that scrolls away. The message is the warning WITHOUT the feature's
        /// name prefix, which the row already shows.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        feature_warnings: Vec<(Uuid, String)>,
        /// Features whose bodies a later feature consumed (a merge, cut or
        /// union took custody): not live, not rendered, not a valid boolean
        /// operand (`specs/b4_balanced_union.md` §2.4). Sorted for a stable
        /// wire form.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        consumed_features: Vec<Uuid>,
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
        /// Present while a `Drawing` tab is open: its evaluated sheets
        /// (D4a).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        drawing: Option<DrawingStatus>,
        /// Present while a Part is open in the context of an assembly
        /// (`OpenPartInContext`, Phase 3d-4).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        context: Option<ContextStatus>,
        /// The open part's named mate connectors (its `MateConnector`
        /// features), in the part's coordinates, for the viewport to draw.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        connectors: Vec<PartConnectorInfo>,
        /// The document session (S2 C2): metadata, the tab list, the active
        /// tab and the revision.
        ///
        /// **Nothing reads this yet** — C4 makes the JS store's tab,
        /// assembly and metadata `$state` a mirror of it. Until C3 gives
        /// `SwitchTab` a tab id, the store can switch tabs without telling
        /// the session, so `active_tab` names the last tab a load or a save
        /// reported, not necessarily the one on screen.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        document: Option<DocumentInfo>,
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

    /// Answer to `QueryEntityMeta` (`specs/kicad_board_link.md` C4). Every
    /// field `None` when the body or instance derives from no KiCad source.
    EntityMeta {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        board: Option<feature_engine::kicad::BoardMeta>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        component: Option<feature_engine::kicad::ComponentMeta>,
        /// The source the board came from — for an "open at this commit"
        /// link.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source: Option<SourceStatus>,
    },

    /// Answer to `ListFaces` (ICR-3): ordered by canonical `GeomRef` JSON.
    FacesListed {
        body_id: String,
        faces: Vec<ListedFace>,
    },

    /// Answer to `ListEntities` (Q6): ordered by persistent id, which is
    /// content-seeded and therefore the same order after a rebuild.
    EntitiesListed {
        body_id: String,
        kind: EntityListKind,
        entities: Vec<ListedEntity>,
        /// The body's own frame (Q3's principal axes), or why it has none.
        body: ListedBodyFrame,
        /// How many entities a filter arm EXCLUDED because their own data
        /// could not answer it, rather than because they failed it.
        ///
        /// Without this an agent cannot tell "no entity is in that box" from
        /// "no entity could be asked", and those call for opposite next
        /// moves. Today only the `bbox` arm can contribute: an entity whose
        /// signature carries no bounding box is not assumed to fit, so it
        /// drops out of a `bbox`-filtered listing — and that is the fact this
        /// counts. (An unnamed entity failing a `name` glob is NOT counted:
        /// "this entity's name does not match" is a real answer when there is
        /// no name. Zero whenever no filter is given.)
        excluded_unevaluable: usize,
        /// This body's N1 names that resolve to nothing, so no entity in the
        /// listing carries them — in name order. A name the user set and then
        /// invalidated is a fact about the listing they asked for, and an
        /// empty `name` on every entity is not a way to learn it.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        unresolved_names: Vec<String>,
    },

    /// Answer to `QueryEntityNames` (N1), in name order.
    EntityNamesListed { names: Vec<ListedName> },

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

    /// Answer to `MeasureDistance` (Q1). Lengths in meters.
    ///
    /// `method` is `exact` only when the kernel could certify the number
    /// analytically; otherwise it is `mesh` and `chord_bound_m` is the band
    /// the true value lies within. A mesh number is never presented as exact.
    DistanceMeasured {
        /// The distance, or the gap along the requested direction (negative
        /// when the operands overlap along it).
        value_m: f64,
        method: MeasureMethod,
        /// The tessellation band, in meters — the bound on `value_m` when
        /// `method` is `mesh`. Reported either way.
        chord_bound_m: f64,
        /// The closest point on the first operand, then on the second.
        points: [[f64; 3]; 2],
        /// What each point lies on; `null` for a free-point operand.
        on: [Option<MeasuredOn>; 2],
    },

    /// Answer to `MeasureInterference` (Q2).
    InterferenceMeasured {
        /// The two body ids, echoed in the order they were asked about — the
        /// witness points in `result` follow that order.
        a: String,
        b: String,
        result: MeasuredInterference,
    },

    /// Answer to `MeasureMass` (Q3). SI throughout: m³, m², meters, kg/m³,
    /// kg, kg·m².
    ///
    /// `method` covers every number at once — they come out of one
    /// integration over the same faces, so they cannot be at different tiers.
    MassMeasured {
        body_id: String,
        volume_m3: f64,
        surface_area_m2: f64,
        centroid: [f64; 3],
        /// About the centroid, in the world axes, scaled by `density_kg_m3`.
        inertia_at_centroid: [[f64; 3]; 3],
        /// The tensor's eigenvalues, ascending.
        principal_moments: [f64; 3],
        /// The unit eigenvector of each, as rows, right-handed.
        principal_axes: [[f64; 3]; 3],
        /// The density used — 1 unless the caller passed one, because the
        /// document model has no material table.
        density_kg_m3: f64,
        mass_kg: f64,
        method: MeasureMethod,
        /// The tessellation band in meters when `method` is `mesh`; 0 when
        /// the answer is exact, which carries no band.
        chord_bound_m: f64,
    },

    /// Answer to `MeasureSection` (Q4). Areas in m², coordinates in meters.
    SectionMeasured {
        /// The plane as it was cut: the origin passed through, and the
        /// NORMALIZED normal (a caller's non-unit normal is normalized, and
        /// the answer says what was used).
        plane_origin: [f64; 3],
        plane_normal: [f64; 3],
        /// The frame `bodies[].loops` are expressed in — the kernel's own.
        /// `null` only when every body declined, so there is no frame the
        /// kernel chose to report.
        #[serde(skip_serializing_if = "Option::is_none")]
        basis: Option<SectionBasis>,
        /// One entry per body the kernel sectioned, in the order asked.
        bodies: Vec<SectionedBody>,
        /// The bodies it refused, named and typed.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        declines: Vec<SectionDecline>,
    },

    /// Answer to `MeasureThickness` (Q5). Lengths in meters.
    ///
    /// Nested in a `result` field, as `InterferenceMeasured` nests its own:
    /// the enum is internally tagged, so a newtype variant would have to
    /// flatten its struct into the message and a field named `type` anywhere
    /// inside it would collide with the tag.
    ThicknessMeasured { result: MeasuredThickness },

    /// Save project is ready.
    SaveReady { json_data: String },

    /// Project loaded successfully.
    ProjectLoaded { feature_tree: FeatureTree },

    /// Answer to `ListSources`.
    SourcesListed { sources: Vec<SourceStatus> },

    /// Answer to `ReadSource`: the source's text.
    SourceContent {
        source_id: Uuid,
        name: String,
        /// The source kind's `type` tag (`Script`, `Step`, …).
        kind: String,
        text: String,
    },

    /// Answer to `AddScriptSource`: the new source's id and the `sources`
    /// table as it now stands (so a host's Sources panel stays in step
    /// without a model update), plus the header check of the text.
    ScriptSourceAdded {
        source_id: Uuid,
        name: String,
        sources: Vec<SourceStatus>,
        check: ScriptCheck,
    },

    /// Answer to `CheckScript`.
    ScriptChecked {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source_id: Option<Uuid>,
        check: ScriptCheck,
    },

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

    /// DXF export is ready. `warnings` names anything the export left out,
    /// on the same terms as [`EngineToUi::ExportReady`].
    DxfExportReady {
        dxf_data: String,
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

    /// Sprocket preview polyline generated.
    SprocketPreviewGenerated { polyline: Vec<(f64, f64)> },

    /// Full sprocket profile generated: the same shape as
    /// `GearProfileGenerated` (so hosts display both the same way) plus the
    /// resolved ISO 606 dimensions.
    SprocketProfileGenerated {
        entities: Vec<SketchEntity>,
        #[serde(with = "u32_key_map")]
        positions: HashMap<u32, (f64, f64)>,
        profiles: Vec<ClosedProfile>,
        pitch_radius: f64,
        dimensions: SprocketDimensions,
    },

    /// Minimal closed faces of a sketch, in selection order.
    RegionsComputed { regions: Vec<Region> },

    /// Result of `ApplySketchOps`: the sketch AFTER the batch, plus what
    /// changed and the pins the following solve should see.
    ///
    /// The full entity and constraint lists come back rather than only the
    /// edit, because adopting a whole state is one assignment in the UI while
    /// replaying an edit is three loops that can disagree with the engine's
    /// idea of the result. The `edit` is still here: it is what an undo entry
    /// and a tool's answer are made of.
    SketchOpsApplied {
        entities: Vec<SketchEntity>,
        constraints: Vec<SketchConstraint>,
        #[serde(default)]
        projected: Vec<ProjectedEntity>,
        edit: waffle_types::SketchEdit,
        /// Constraints for the NEXT solve only — a `MovePoint`'s pin. Never
        /// persisted (`waffle_types::SketchOp::MovePoint`).
        #[serde(default)]
        transient_constraints: Vec<SketchConstraint>,
        /// The next free entity id: the UI advances its counter to this.
        next_id: u32,
    },

    /// Answer to `QuerySketch`.
    SketchQueried { result: SketchQueryResult },

    /// Result of `EvaluateExpression`: exactly one of `value` (mm-space
    /// number) or `error` (user-facing message) is set. `dimension` names
    /// the kind of quantity the expression produced — `"length"`,
    /// `"angle"`, `"ratio"`, a composite like `"length^2"`, or
    /// `"unitless"` when no unit suffix committed one (a plain number,
    /// which any field accepts).
    ExpressionEvaluated {
        value: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        dimension: Option<String>,
        error: Option<String>,
    },

    /// Planetary stage generated: positioned gears + derived radii + hints.
    PlanetaryGenerated { result: PlanetaryResult },

    /// Planetary preview generated: one polyline per gear (sun, N planets,
    /// ring). Empty when the params are invalid.
    PlanetaryPreviewGenerated { polylines: Vec<Vec<(f64, f64)>> },

    /// The answer to [`UiToEngine::Tool`]: an MCP tool result, in the wire
    /// shape the relay forwards unchanged (S3).
    ToolResult {
        #[serde(flatten)]
        result: crate::tools::ToolResult,
        /// The model update a tool that CHANGED the document carries with its
        /// answer (S3 C4).
        ///
        /// A `ToolResult` is not a `ModelUpdated`, so without this nothing
        /// refreshes the host's tree, meshes and errors after an authoring
        /// call — the page's worker collects meshes only for a `ModelUpdated`,
        /// and its store assigns the tree only from one. Omitted entirely for
        /// the read-only tools, whose answers stay byte-identical on the wire.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model: Option<Box<EngineToUi>>,
    },
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
    /// The named mate connectors of every rendered part instance (each
    /// part's `MateConnector` features), in WORLD coordinates — what an
    /// assembly connector can be made from (`part_connector`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub part_connectors: Vec<PartConnectorInfo>,
}

/// One edit to a drawing, in primitives (D4a).
///
/// The page's edit vocabulary. Every field is a string, a number or a small
/// enum — deliberately no `Drawing`, no `Annotation` and no `GeomRef` — so
/// that nothing a JavaScript `JSON.parse` would damage crosses the wire. The
/// engine builds the document types on this side.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum DrawingEdit {
    AddView {
        #[serde(default)]
        sheet_id: Option<Uuid>,
        /// The Part or Assembly tab the view draws.
        source_tab: String,
        #[serde(default)]
        bodies: Vec<String>,
        projection: feature_engine::drawing::Projection,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        scale: Option<f64>,
        #[serde(default)]
        placement_mm: Option<[f64; 2]>,
    },
    EditView {
        view_id: Uuid,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        scale: Option<f64>,
        #[serde(default)]
        placement_mm: Option<[f64; 2]>,
        #[serde(default)]
        bodies: Option<Vec<String>>,
        #[serde(default)]
        hidden_lines: Option<bool>,
        #[serde(default)]
        silhouettes: Option<bool>,
    },
    /// Delete a view, and with it every view projected FROM it — which is
    /// what deleting a parent view means; a child left behind would name a
    /// parent that is not on the sheet.
    DeleteView {
        view_id: Uuid,
    },
    AddAnnotation {
        view_id: Uuid,
        annotation: DrawingAnnotationSpec,
    },
    DeleteAnnotation {
        view_id: Uuid,
        index: usize,
    },
    /// Change one sheet — its name, paper, title block — and the DRAWING's
    /// projection standard (D4b).
    ///
    /// The standard rides on the sheet door because that is where it is
    /// authored and read (a title block prints it), but it is the drawing's
    /// own setting per §8: sheets that disagreed about which side a projected
    /// view shows would be two standards in one document.
    EditSheet {
        #[serde(default)]
        sheet_id: Option<Uuid>,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        size: Option<feature_engine::drawing::SheetSize>,
        #[serde(default)]
        orientation: Option<feature_engine::drawing::Orientation>,
        #[serde(default)]
        projection_angle: Option<feature_engine::drawing::ProjectionAngle>,
        #[serde(default)]
        title_block_show: Option<bool>,
        /// The whole row list, replaced. Not a per-row edit: the rows are an
        /// ORDER as well as a set (a title block is read top to bottom), and
        /// an index-addressed edit of a list the caller did not just read is
        /// how the wrong row gets changed.
        #[serde(default)]
        title_block_fields: Option<Vec<feature_engine::drawing::TitleBlockField>>,
    },
    AddSheet {
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        size: Option<feature_engine::drawing::SheetSize>,
        #[serde(default)]
        orientation: Option<feature_engine::drawing::Orientation>,
    },
    /// Delete a sheet and the views on it. Refused for the LAST sheet: a
    /// drawing with no sheet shows nothing and refuses every export by name,
    /// which reads as a broken tab rather than an empty one.
    DeleteSheet {
        sheet_id: Uuid,
    },
}

/// An annotation to author, in primitives (D4a).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrawingAnnotationSpec {
    /// `Dimension` | `Note` | `CentreMark` | `CentreLine` | `Datum`.
    pub annotation: String,
    /// The dimension kind, for `Dimension`.
    #[serde(default)]
    pub kind: Option<String>,
    /// The entities measured, as persistent ids.
    #[serde(default)]
    pub anchors: Vec<DrawingAnchorSpec>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    /// A dimension's value as an EXPRESSION (D4c): `Measured::Expr`, which
    /// D2 made evaluable and this is the authoring half of. Still no
    /// `value`: a literal number is not expressible at this boundary at all.
    #[serde(default)]
    pub expr: Option<String>,
    #[serde(default)]
    pub precision: Option<u8>,
    #[serde(default)]
    pub dual_unit: Option<String>,
    #[serde(default)]
    pub placement: Option<[f64; 2]>,
}

/// One anchor of an authored annotation: a persistent id and what kind of
/// entity it names.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrawingAnchorSpec {
    /// A decimal STRING, because a `u64` is not exact as a JSON number in
    /// JavaScript (`waffle_types::pid_str`).
    #[serde(with = "waffle_types::pid_str")]
    pub pid: u64,
    #[serde(default = "edge_kind")]
    pub kind: waffle_types::TopoKind,
}

fn edge_kind() -> waffle_types::TopoKind {
    waffle_types::TopoKind::Edge
}

/// The evaluated drawing as the UI needs it (D4a,
/// `specs/drawings_and_mbd.md` §8).
///
/// Carries the whole drawing, caches included, because that is what the sheet
/// draws. It is the `ViewLayout` per view — curves and already-measured
/// annotations — that holds no `GeomRef` and no kernel handle, which is the
/// D3 argument this is the producer for: the renderer has no path back to the
/// model and so no way to draw a value other than the measured one.
///
/// The AUTHORED annotations beside those layouts do carry their anchors, and
/// a `Selector::Pid` in one serializes as a JSON **number** — a `u64` a
/// JavaScript `JSON.parse` rounds above `2^53` (see
/// `waffle_types::pid_str` for the measurement). That is
/// inert, not safe by construction: the page reads the authored annotations
/// only as a count, draws from the layouts, and writes back exclusively
/// through [`DrawingEdit`], whose every field is a primitive and whose
/// annotation deletes address by INDEX. Nothing may start echoing a pid read
/// from here; a page that needs one reads `anchors` below, which crosses as
/// a string.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrawingStatus {
    pub tab_id: String,
    /// The tab's drawing, with every view that rebuilt carrying its layout.
    pub drawing: feature_engine::drawing::Drawing,
    /// Per view id, the entities it drew with the persistent ids an
    /// annotation anchors on.
    ///
    /// Beside the drawing rather than inside it, and for the reason the
    /// LAYOUT carries no reference at all (D3): the document stores the
    /// annotations, not the ids available to make one from, and the renderer
    /// reads only the layout. This is the authoring path — picking an edge on
    /// the sheet to dimension it needs the edge's id.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub anchors: std::collections::BTreeMap<Uuid, Vec<feature_engine::drawing::ViewAnchor>>,
    /// What the projections declined to decide, by counter name, non-zero
    /// ones only (D1c `ProjectionDeclines`). Present for the same reason the
    /// DXF export carries them: they are what tells a decided drawing from a
    /// quiet one, and a sheet that silently dropped a hundred hidden arcs
    /// looks finished.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub declines: std::collections::BTreeMap<String, u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

/// One named mate connector of a part (`specs/part_mate_connectors.md`), as
/// evaluated: an orthonormal basis at a point, in world coordinates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartConnectorInfo {
    /// The `MateConnector` feature's id.
    pub feature_id: Uuid,
    /// The feature's name.
    pub name: String,
    /// The part instance it is on, in an assembly; empty for the open part.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub instance_path: Vec<Uuid>,
    /// What the frame was derived from (`"planar face"`, …); absent for an
    /// explicit frame.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    pub origin: [f64; 3],
    pub x_axis: [f64; 3],
    pub y_axis: [f64; 3],
    pub z_axis: [f64; 3],
}

impl PartConnectorInfo {
    /// A part connector placed by `placement` (identity for the open part).
    pub fn new(
        connector: &feature_engine::connector::PartConnector,
        instance_path: Vec<Uuid>,
        placement: &feature_engine::assembly::Transform,
    ) -> Option<Self> {
        let world = connector.frame.transformed(placement);
        let (x_axis, y_axis, z_axis) = world.basis().ok()?;
        Some(PartConnectorInfo {
            feature_id: connector.feature_id,
            name: connector.name.clone(),
            instance_path,
            kind: connector.geometry.map(|k| k.label().to_string()),
            origin: world.origin,
            x_axis,
            y_axis,
            z_axis,
        })
    }
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

/// The result of checking a script (`CheckScript`, `AddScriptSource`):
/// the parsed interface when the header parses and the script compiles,
/// else the typed failure (`stage`: `header` | `parse`); with arguments,
/// the dry run's outcome too (A-M4).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptCheck {
    /// Header parsed, script compiled, entry function present.
    pub ok: bool,
    /// The declared interface (`name`, `version`, `params`, `outputs`),
    /// present when `ok`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interface: Option<serde_json::Value>,
    /// The failure when not `ok`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ScriptCheckError>,
    /// The dry run, when arguments were supplied and the check passed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dry_run: Option<ScriptDryRun>,
}

/// A typed script failure: the stage (`header`, `parse`, `args`, `runtime`,
/// `limit`, `fail`) and the interpreter's own message (a header failure
/// starts with `line N:`; a parse failure ends with `(line N, position M)`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptCheckError {
    pub stage: String,
    pub reason: String,
}

/// What a dry run of a script recorded, without a kernel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptDryRun {
    /// The arguments resolved, the script ran to completion, and its return
    /// value satisfied the header's `@output` contract.
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ScriptCheckError>,
    /// The recorded child operations, in order, by label (`sketch`,
    /// `extrude`, …).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<String>,
    /// `ctx.log` / `print` lines.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub logs: Vec<String>,
    /// The public output names the return value provides, in node order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<String>,
}
