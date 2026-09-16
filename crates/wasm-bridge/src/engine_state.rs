use std::collections::HashMap;

use feature_engine::Engine;
use waffle_types::{
    ClosedProfile, GeomRef, ProjectedEntity, Sketch, SketchConstraint, SketchEntity, SolveStatus,
};

use crate::session::DocumentSession;

/// The name a document carries until it is saved or loaded under another.
const UNTITLED: &str = "Untitled";

/// The display unit a document with no stated preference is shown in.
const DEFAULT_DISPLAY_UNIT: &str = "mm";

/// The engine state wrapper for the WASM bridge.
///
/// Holds the parametric modeling engine and manages the active sketch session.
pub struct EngineState {
    /// The parametric modeling engine.
    pub engine: Engine,
    /// The currently active sketch being edited, if any.
    pub active_sketch: Option<ActiveSketch>,
    /// Current selection state.
    pub selection: Vec<GeomRef>,
    /// Current hover state.
    pub hover: Option<GeomRef>,
    /// The open document (`specs/waffle_server_mode.md` §2.3 S2): metadata,
    /// the tab list, every inactive tab's tree, per-tab undo and the
    /// revision. The document's name and display unit live HERE, not in a
    /// second copy on this struct — read them through [`EngineState::project_name`]
    /// and [`EngineState::display_unit`].
    pub session: DocumentSession,
    /// The document's `sources` table (v4 §2.3) — metadata only; content
    /// lives in `engine.sources`. Document-scoped: survives tab switches,
    /// cleared by `NewDocument`, attached to every save.
    pub sources: Vec<file_format::SourceEntry>,
    /// Unknown `document.*` keys captured at load, re-emitted on save (§2.6).
    pub document_extra: serde_json::Map<String, serde_json::Value>,
    /// Unknown envelope keys captured at load, re-emitted on save (§2.6).
    pub envelope_extra: serde_json::Map<String, serde_json::Value>,
    /// The open `Assembly` tab, evaluated (Phase 3b). `None` while a Part tab
    /// is active; its instance bodies are what the renderer shows.
    pub assembly: Option<crate::assembly_view::AssemblyView>,
    /// The assembly context the live Part is open in (Phase 3d-4,
    /// `OpenPartInContext`): the evaluated assembly plus which leaf is being
    /// edited. Its OTHER leaves render as ghosts in the part's frame; the
    /// engine's `context` (the resolution snapshot) is derived from it.
    pub context_view: Option<crate::assembly_view::ContextView>,
}

/// An active sketch editing session.
pub struct ActiveSketch {
    /// The plane the sketch is on.
    pub plane: GeomRef,
    /// Sketch entities added so far.
    pub entities: Vec<SketchEntity>,
    /// Constraints added so far.
    pub constraints: Vec<SketchConstraint>,
    /// Last solve status.
    pub solve_status: SolveStatus,
}

impl EngineState {
    /// Create a new engine state.
    pub fn new() -> Self {
        Self {
            engine: Engine::new(),
            active_sketch: None,
            selection: Vec::new(),
            hover: None,
            session: DocumentSession::new(UNTITLED),
            sources: Vec::new(),
            document_extra: serde_json::Map::new(),
            envelope_extra: serde_json::Map::new(),
            assembly: None,
            context_view: None,
        }
    }

    /// The document's name, used for save and export file names.
    pub fn project_name(&self) -> &str {
        &self.session.document().name
    }

    /// The document's display unit. A document that states no preference is
    /// shown in millimetres, which is what the field this replaced defaulted to.
    pub fn display_unit(&self) -> &str {
        self.session
            .document()
            .display_unit
            .as_deref()
            .unwrap_or(DEFAULT_DISPLAY_UNIT)
    }

    /// Rename the document (its save and export file names follow).
    pub fn set_project_name(&mut self, name: impl Into<String>) {
        self.session.set_meta(Some(name.into()), None);
    }

    /// Set the document's display unit.
    pub fn set_display_unit(&mut self, unit: impl Into<String>) {
        self.session.set_meta(None, Some(unit.into()));
    }

    /// Leave any in-context editing session: the ghost view and the engine's
    /// resolution snapshot go together.
    pub fn clear_context(&mut self) {
        self.context_view = None;
        self.engine.context = None;
    }

    /// Begin a new sketch session on the given plane.
    pub fn begin_sketch(&mut self, plane: GeomRef) {
        self.active_sketch = Some(ActiveSketch {
            plane,
            entities: Vec::new(),
            constraints: Vec::new(),
            solve_status: SolveStatus::UnderConstrained { dof: 0 },
        });
    }

    /// Add an entity to the active sketch.
    pub fn add_sketch_entity(&mut self, entity: SketchEntity) -> Result<(), BridgeError> {
        let sketch = self
            .active_sketch
            .as_mut()
            .ok_or(BridgeError::NoActiveSketch)?;
        sketch.entities.push(entity);
        Ok(())
    }

    /// Add a constraint to the active sketch.
    pub fn add_sketch_constraint(
        &mut self,
        constraint: SketchConstraint,
    ) -> Result<(), BridgeError> {
        let sketch = self
            .active_sketch
            .as_mut()
            .ok_or(BridgeError::NoActiveSketch)?;
        sketch.constraints.push(constraint);
        Ok(())
    }

    /// REPLACE the active sketch's constraint list. The UI is the source of
    /// truth for which constraints are live: it removes constraints, and it
    /// excludes REFERENCE (driven) dimensions — which display a measured value
    /// but must NOT constrain. The incremental `add_sketch_constraint` path
    /// cannot express removal or reference toggling, so the UI re-syncs the full
    /// driving set here before each solve.
    pub fn set_sketch_constraints(
        &mut self,
        constraints: Vec<SketchConstraint>,
    ) -> Result<(), BridgeError> {
        let sketch = self
            .active_sketch
            .as_mut()
            .ok_or(BridgeError::NoActiveSketch)?;
        sketch.constraints = constraints;
        Ok(())
    }

    /// REPLACE the active sketch's entity list with the UI's live geometry, so
    /// the solver starts from the current point positions (a drag persists)
    /// rather than the append-only original drawn positions.
    pub fn set_sketch_entities(&mut self, entities: Vec<SketchEntity>) -> Result<(), BridgeError> {
        let sketch = self
            .active_sketch
            .as_mut()
            .ok_or(BridgeError::NoActiveSketch)?;
        sketch.entities = entities;
        Ok(())
    }

    /// Build a Sketch struct from the active sketch state.
    pub fn build_sketch(&self) -> Result<Sketch, BridgeError> {
        let active = self
            .active_sketch
            .as_ref()
            .ok_or(BridgeError::NoActiveSketch)?;

        Ok(Sketch {
            id: uuid::Uuid::new_v4(),
            plane: active.plane.clone(),
            plane_origin: [0.0, 0.0, 0.0],
            plane_normal: [0.0, 0.0, 1.0],
            entities: active.entities.clone(),
            constraints: active.constraints.clone(),
            solve_status: active.solve_status.clone(),
            solved_positions: HashMap::new(),
            solved_profiles: Vec::new(),
            projected: Vec::new(),
        })
    }

    /// Finish the active sketch and commit it as a feature.
    /// Accepts solved positions, profiles, and plane geometry from the JS-side solver.
    /// When `entities`/`constraints` are non-empty, they override the stale copies
    /// accumulated via AddSketchEntity/AddConstraint (the JS solver may have updated
    /// properties like circle radius that the Rust side never received).
    // Eight inputs, one past clippy's default. This mirrors the FinishSketch
    // message's payload one-for-one; grouping them into a struct would just
    // rename the message type, and the signature is the bridge contract.
    #[allow(clippy::too_many_arguments)]
    pub fn finish_sketch(
        &mut self,
        solved_positions: HashMap<u32, (f64, f64)>,
        solved_profiles: Vec<ClosedProfile>,
        plane_origin: [f64; 3],
        plane_normal: [f64; 3],
        entities: Vec<SketchEntity>,
        constraints: Vec<SketchConstraint>,
        projected: Vec<ProjectedEntity>,
    ) -> Result<Sketch, BridgeError> {
        let mut sketch = self.build_sketch()?;
        if !entities.is_empty() {
            sketch.entities = entities;
        }
        if !constraints.is_empty() {
            sketch.constraints = constraints;
        }
        sketch.solved_positions = solved_positions;
        sketch.solved_profiles = solved_profiles;
        sketch.plane_origin = plane_origin;
        sketch.plane_normal = plane_normal;
        sketch.projected = projected;
        self.active_sketch = None;
        Ok(sketch)
    }

    /// Reset to a clean state (new document).
    pub fn reset(&mut self) {
        self.assembly = None;
        self.context_view = None;
        self.engine = Engine::new();
        self.active_sketch = None;
        self.selection.clear();
        self.hover = None;
        // A new document is a new session: one empty Part tab, fresh
        // metadata, revision back to 0.
        self.session = DocumentSession::new(UNTITLED);
        self.sources.clear();
        self.document_extra.clear();
        self.envelope_extra.clear();
    }
}

impl Default for EngineState {
    fn default() -> Self {
        Self::new()
    }
}

/// Errors from the WASM bridge layer.
#[derive(Debug, Clone, thiserror::Error)]
pub enum BridgeError {
    #[error("no active sketch")]
    NoActiveSketch,

    #[error("engine error: {0}")]
    Engine(#[from] feature_engine::types::EngineError),

    #[error("serialization error: {reason}")]
    Serialization { reason: String },

    #[error("not implemented: {operation}")]
    NotImplemented { operation: String },

    #[error("no mesh data available for export")]
    NoMeshData,

    #[error("invalid request: {reason}")]
    InvalidRequest { reason: String },

    /// A tab operation the session refused (an unknown tab id, a kind this
    /// build cannot open, the last tab). Each maps to an agent error code of
    /// the closed set (`specs/waffle_mcp_server.md` §6.1).
    #[error("{0}")]
    Session(#[from] crate::session::SessionError),
}
