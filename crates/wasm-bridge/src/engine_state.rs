use std::collections::HashMap;

use feature_engine::Engine;
use waffle_types::{ClosedProfile, GeomRef, Sketch, SketchConstraint, SketchEntity, SolveStatus};

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
    /// Project name for save operations.
    pub project_name: String,
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
            project_name: "Untitled".to_string(),
        }
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
        })
    }

    /// Finish the active sketch and commit it as a feature.
    /// Accepts solved positions, profiles, and plane geometry from the JS-side solver.
    pub fn finish_sketch(
        &mut self,
        solved_positions: HashMap<u32, (f64, f64)>,
        solved_profiles: Vec<ClosedProfile>,
        plane_origin: [f64; 3],
        plane_normal: [f64; 3],
    ) -> Result<Sketch, BridgeError> {
        let mut sketch = self.build_sketch()?;
        sketch.solved_positions = solved_positions;
        sketch.solved_profiles = solved_profiles;
        sketch.plane_origin = plane_origin;
        sketch.plane_normal = plane_normal;
        self.active_sketch = None;
        Ok(sketch)
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
}
