//! ModelBuilder — fluent API for scripting CAD workflows in tests.
//!
//! Wraps `wasm_bridge::dispatch()` to test the real dispatch path, not a simulation.
//! All methods accept string names instead of UUIDs for readability.

use std::collections::HashMap;

use feature_engine::types::*;
use kernel_fork::types::{KernelSolidHandle, RenderMesh};
use kernel_fork::{MockKernel, TruckKernel};
use modeling_ops::types::OpResult;
use modeling_ops::KernelBundle;
use uuid::Uuid;
use waffle_types::Role;
use waffle_types::*;
use wasm_bridge::messages::*;
use wasm_bridge::EngineState;

use crate::helpers::*;
use crate::oracle;
use crate::stl;

/// A fluent builder for constructing and verifying CAD models in tests.
///
/// Wraps `EngineState` + `KernelBundle` and provides named-feature access,
/// sketch lifecycle management, and inline assertions.
pub struct ModelBuilder {
    pub state: EngineState,
    pub(crate) kernel: Box<dyn KernelBundle>,
    named_features: HashMap<String, Uuid>,
    history: Vec<(String, String)>,
    auto_check: bool,
}

impl ModelBuilder {
    /// Create a new ModelBuilder with MockKernel (deterministic, fast).
    pub fn mock() -> Self {
        Self {
            state: EngineState::new(),
            kernel: Box::new(MockKernel::new()),
            named_features: HashMap::new(),
            history: Vec::new(),
            auto_check: false,
        }
    }

    /// Create a new ModelBuilder with TruckKernel (real geometry).
    pub fn truck() -> Self {
        Self {
            state: EngineState::new(),
            kernel: Box::new(TruckKernel::new()),
            named_features: HashMap::new(),
            history: Vec::new(),
            auto_check: false,
        }
    }

    /// Enable auto-checking: after every operation, verify no engine errors.
    pub fn with_auto_check(mut self) -> Self {
        self.auto_check = true;
        self
    }

    // ── Sketch Shortcuts ────────────────────────────────────────────────

    /// Create a rectangular sketch in one call (BeginSketch → entities → FinishSketch).
    #[allow(clippy::too_many_arguments)]
    pub fn rect_sketch(
        &mut self,
        name: &str,
        origin: [f64; 3],
        normal: [f64; 3],
        x: f64,
        y: f64,
        w: f64,
        h: f64,
    ) -> Result<Uuid, HarnessError> {
        self.check_name_available(name)?;

        let plane = datum_plane_ref(Uuid::new_v4());
        wasm_bridge::dispatch(
            &mut self.state,
            UiToEngine::BeginSketch { plane },
            self.kernel.as_mut(),
        );

        let (entities, positions, profiles) = rect_profile(x, y, w, h);
        for entity in entities {
            wasm_bridge::dispatch(
                &mut self.state,
                UiToEngine::AddSketchEntity { entity },
                self.kernel.as_mut(),
            );
        }

        let response = wasm_bridge::dispatch(
            &mut self.state,
            UiToEngine::FinishSketch {
                solved_positions: positions,
                solved_profiles: profiles,
                plane_origin: origin,
                plane_normal: normal,
            },
            self.kernel.as_mut(),
        );

        self.extract_last_feature_id(name, "FinishSketch", response)
    }

    /// Create a circular sketch (polygon approximation) in one call.
    pub fn circle_sketch(
        &mut self,
        name: &str,
        origin: [f64; 3],
        normal: [f64; 3],
        cx: f64,
        cy: f64,
        r: f64,
    ) -> Result<Uuid, HarnessError> {
        self.check_name_available(name)?;

        let plane = datum_plane_ref(Uuid::new_v4());
        wasm_bridge::dispatch(
            &mut self.state,
            UiToEngine::BeginSketch { plane },
            self.kernel.as_mut(),
        );

        let (entities, positions, profiles) = circle_profile(cx, cy, r, 16);
        for entity in entities {
            wasm_bridge::dispatch(
                &mut self.state,
                UiToEngine::AddSketchEntity { entity },
                self.kernel.as_mut(),
            );
        }

        let response = wasm_bridge::dispatch(
            &mut self.state,
            UiToEngine::FinishSketch {
                solved_positions: positions,
                solved_profiles: profiles,
                plane_origin: origin,
                plane_normal: normal,
            },
            self.kernel.as_mut(),
        );

        self.extract_last_feature_id(name, "FinishSketch(circle)", response)
    }

    // ── Manual Sketch ───────────────────────────────────────────────────

    /// Begin a manual sketch on the given plane.
    pub fn begin_sketch(&mut self, origin: [f64; 3], normal: [f64; 3]) -> &mut Self {
        let plane = datum_plane_ref(Uuid::new_v4());
        wasm_bridge::dispatch(
            &mut self.state,
            UiToEngine::BeginSketch { plane },
            self.kernel.as_mut(),
        );
        // Store plane info for finish_sketch_manual
        self.history
            .push(("BeginSketch".into(), format!("{:?}/{:?}", origin, normal)));
        self
    }

    /// Add a point entity to the active sketch.
    pub fn add_point(&mut self, id: u32, x: f64, y: f64) -> &mut Self {
        wasm_bridge::dispatch(
            &mut self.state,
            UiToEngine::AddSketchEntity {
                entity: SketchEntity::Point {
                    id,
                    x,
                    y,
                    construction: false,
                },
            },
            self.kernel.as_mut(),
        );
        self
    }

    /// Add a line entity to the active sketch.
    pub fn add_line(&mut self, id: u32, start: u32, end: u32) -> &mut Self {
        wasm_bridge::dispatch(
            &mut self.state,
            UiToEngine::AddSketchEntity {
                entity: SketchEntity::Line {
                    id,
                    start_id: start,
                    end_id: end,
                    construction: false,
                },
            },
            self.kernel.as_mut(),
        );
        self
    }

    /// Add a circle entity to the active sketch.
    pub fn add_circle_entity(&mut self, id: u32, center: u32, radius: f64) -> &mut Self {
        wasm_bridge::dispatch(
            &mut self.state,
            UiToEngine::AddSketchEntity {
                entity: SketchEntity::Circle {
                    id,
                    center_id: center,
                    radius,
                    construction: false,
                },
            },
            self.kernel.as_mut(),
        );
        self
    }

    /// Add an arc entity to the active sketch.
    pub fn add_arc(&mut self, id: u32, center: u32, start: u32, end: u32) -> &mut Self {
        wasm_bridge::dispatch(
            &mut self.state,
            UiToEngine::AddSketchEntity {
                entity: SketchEntity::Arc {
                    id,
                    center_id: center,
                    start_id: start,
                    end_id: end,
                    construction: false,
                },
            },
            self.kernel.as_mut(),
        );
        self
    }

    /// Finish the manual sketch with explicit positions and profiles.
    pub fn finish_sketch_manual(
        &mut self,
        name: &str,
        positions: HashMap<u32, (f64, f64)>,
        profiles: Vec<ClosedProfile>,
        origin: [f64; 3],
        normal: [f64; 3],
    ) -> Result<Uuid, HarnessError> {
        self.check_name_available(name)?;

        let response = wasm_bridge::dispatch(
            &mut self.state,
            UiToEngine::FinishSketch {
                solved_positions: positions,
                solved_profiles: profiles,
                plane_origin: origin,
                plane_normal: normal,
            },
            self.kernel.as_mut(),
        );

        self.extract_last_feature_id(name, "FinishSketch(manual)", response)
    }

    // ── Feature Operations ──────────────────────────────────────────────

    /// Add an extrude feature.
    pub fn extrude(
        &mut self,
        name: &str,
        sketch_name: &str,
        depth: f64,
    ) -> Result<Uuid, HarnessError> {
        self.check_name_available(name)?;
        let sketch_id = self.feature_id(sketch_name)?;

        let response = wasm_bridge::dispatch(
            &mut self.state,
            UiToEngine::AddFeature {
                operation: Operation::Extrude {
                    params: ExtrudeParams {
                        sketch_id,
                        profile_index: 0,
                        depth,
                        direction: None,
                        symmetric: false,
                        cut: false,
                        target_body: None,
                    },
                },
            },
            self.kernel.as_mut(),
        );

        self.extract_last_feature_id(name, "AddFeature(Extrude)", response)
    }

    /// Add a cut extrude feature.
    pub fn extrude_cut(
        &mut self,
        name: &str,
        sketch_name: &str,
        depth: f64,
    ) -> Result<Uuid, HarnessError> {
        self.check_name_available(name)?;
        let sketch_id = self.feature_id(sketch_name)?;

        let response = wasm_bridge::dispatch(
            &mut self.state,
            UiToEngine::AddFeature {
                operation: Operation::Extrude {
                    params: ExtrudeParams {
                        sketch_id,
                        profile_index: 0,
                        depth,
                        direction: None,
                        symmetric: false,
                        cut: true,
                        target_body: None,
                    },
                },
            },
            self.kernel.as_mut(),
        );

        self.extract_last_feature_id(name, "AddFeature(ExtrudeCut)", response)
    }

    /// Add an extrude feature with explicit direction (for sketch-on-face).
    pub fn extrude_on_face(
        &mut self,
        name: &str,
        sketch_name: &str,
        depth: f64,
        direction: [f64; 3],
    ) -> Result<Uuid, HarnessError> {
        self.check_name_available(name)?;
        let sketch_id = self.feature_id(sketch_name)?;

        let response = wasm_bridge::dispatch(
            &mut self.state,
            UiToEngine::AddFeature {
                operation: Operation::Extrude {
                    params: ExtrudeParams {
                        sketch_id,
                        profile_index: 0,
                        depth,
                        direction: Some(direction),
                        symmetric: false,
                        cut: false,
                        target_body: None,
                    },
                },
            },
            self.kernel.as_mut(),
        );

        self.extract_last_feature_id(name, "AddFeature(ExtrudeOnFace)", response)
    }

    /// Add a revolve feature.
    pub fn revolve(
        &mut self,
        name: &str,
        sketch_name: &str,
        axis_origin: [f64; 3],
        axis_dir: [f64; 3],
        angle_deg: f64,
    ) -> Result<Uuid, HarnessError> {
        self.check_name_available(name)?;
        let sketch_id = self.feature_id(sketch_name)?;

        let response = wasm_bridge::dispatch(
            &mut self.state,
            UiToEngine::AddFeature {
                operation: Operation::Revolve {
                    params: RevolveParams {
                        sketch_id,
                        profile_index: 0,
                        axis_origin,
                        axis_direction: axis_dir,
                        angle: angle_deg,
                    },
                },
            },
            self.kernel.as_mut(),
        );

        self.extract_last_feature_id(name, "AddFeature(Revolve)", response)
    }

    /// Add a fillet feature targeting edges of another feature.
    pub fn fillet(&mut self, name: &str, target: &str, radius: f64) -> Result<Uuid, HarnessError> {
        self.check_name_available(name)?;
        let target_id = self.feature_id(target)?;

        let response = wasm_bridge::dispatch(
            &mut self.state,
            UiToEngine::AddFeature {
                operation: Operation::Fillet {
                    params: FilletParams {
                        edges: vec![edge_ref_best_effort(target_id)],
                        radius,
                    },
                },
            },
            self.kernel.as_mut(),
        );

        self.extract_last_feature_id(name, "AddFeature(Fillet)", response)
    }

    /// Add a chamfer feature targeting edges of another feature.
    pub fn chamfer(
        &mut self,
        name: &str,
        target: &str,
        distance: f64,
    ) -> Result<Uuid, HarnessError> {
        self.check_name_available(name)?;
        let target_id = self.feature_id(target)?;

        let response = wasm_bridge::dispatch(
            &mut self.state,
            UiToEngine::AddFeature {
                operation: Operation::Chamfer {
                    params: ChamferParams {
                        edges: vec![edge_ref_best_effort(target_id)],
                        distance,
                    },
                },
            },
            self.kernel.as_mut(),
        );

        self.extract_last_feature_id(name, "AddFeature(Chamfer)", response)
    }

    /// Add a shell feature removing faces of another feature.
    pub fn shell(
        &mut self,
        name: &str,
        target: &str,
        thickness: f64,
    ) -> Result<Uuid, HarnessError> {
        self.check_name_available(name)?;
        let target_id = self.feature_id(target)?;

        let response = wasm_bridge::dispatch(
            &mut self.state,
            UiToEngine::AddFeature {
                operation: Operation::Shell {
                    params: ShellParams {
                        faces_to_remove: vec![face_ref(target_id, Role::EndCapPositive, 0)],
                        thickness,
                    },
                },
            },
            self.kernel.as_mut(),
        );

        self.extract_last_feature_id(name, "AddFeature(Shell)", response)
    }

    /// Add a boolean union feature.
    pub fn boolean_union(&mut self, name: &str, a: &str, b: &str) -> Result<Uuid, HarnessError> {
        self.boolean_op(name, a, b, BooleanOp::Union)
    }

    /// Add a boolean subtract feature (a minus b).
    pub fn boolean_subtract(&mut self, name: &str, a: &str, b: &str) -> Result<Uuid, HarnessError> {
        self.boolean_op(name, a, b, BooleanOp::Subtract)
    }

    /// Add a boolean intersect feature.
    pub fn boolean_intersect(
        &mut self,
        name: &str,
        a: &str,
        b: &str,
    ) -> Result<Uuid, HarnessError> {
        self.boolean_op(name, a, b, BooleanOp::Intersect)
    }

    fn boolean_op(
        &mut self,
        name: &str,
        a: &str,
        b: &str,
        op: BooleanOp,
    ) -> Result<Uuid, HarnessError> {
        self.check_name_available(name)?;
        let a_id = self.feature_id(a)?;
        let b_id = self.feature_id(b)?;

        let response = wasm_bridge::dispatch(
            &mut self.state,
            UiToEngine::AddFeature {
                operation: Operation::BooleanCombine {
                    params: BooleanParams {
                        body_a: body_ref(a_id),
                        body_b: body_ref(b_id),
                        operation: op,
                    },
                },
            },
            self.kernel.as_mut(),
        );

        self.extract_last_feature_id(name, "AddFeature(Boolean)", response)
    }

    // ── History ─────────────────────────────────────────────────────────

    /// Undo the last operation.
    pub fn undo(&mut self) -> Result<&mut Self, HarnessError> {
        let response =
            wasm_bridge::dispatch(&mut self.state, UiToEngine::Undo, self.kernel.as_mut());
        match response {
            EngineToUi::ModelUpdated { .. } => {
                self.history.push(("Undo".into(), "ModelUpdated".into()));
                Ok(self)
            }
            EngineToUi::Error { message, .. } => Err(HarnessError::DispatchError { message }),
            _ => Err(HarnessError::DispatchError {
                message: "unexpected undo response".into(),
            }),
        }
    }

    /// Redo the last undone operation.
    pub fn redo(&mut self) -> Result<&mut Self, HarnessError> {
        let response =
            wasm_bridge::dispatch(&mut self.state, UiToEngine::Redo, self.kernel.as_mut());
        match response {
            EngineToUi::ModelUpdated { .. } => {
                self.history.push(("Redo".into(), "ModelUpdated".into()));
                Ok(self)
            }
            EngineToUi::Error { message, .. } => Err(HarnessError::DispatchError { message }),
            _ => Err(HarnessError::DispatchError {
                message: "unexpected redo response".into(),
            }),
        }
    }

    // ── Feature Management ──────────────────────────────────────────────

    /// Suppress a feature by name.
    pub fn suppress(&mut self, name: &str) -> Result<&mut Self, HarnessError> {
        let id = self.feature_id(name)?;
        let response = wasm_bridge::dispatch(
            &mut self.state,
            UiToEngine::SuppressFeature {
                feature_id: id,
                suppressed: true,
            },
            self.kernel.as_mut(),
        );
        match response {
            EngineToUi::ModelUpdated { .. } => Ok(self),
            EngineToUi::Error { message, .. } => Err(HarnessError::DispatchError { message }),
            _ => Err(HarnessError::DispatchError {
                message: "unexpected suppress response".into(),
            }),
        }
    }

    /// Unsuppress a feature by name.
    pub fn unsuppress(&mut self, name: &str) -> Result<&mut Self, HarnessError> {
        let id = self.feature_id(name)?;
        let response = wasm_bridge::dispatch(
            &mut self.state,
            UiToEngine::SuppressFeature {
                feature_id: id,
                suppressed: false,
            },
            self.kernel.as_mut(),
        );
        match response {
            EngineToUi::ModelUpdated { .. } => Ok(self),
            EngineToUi::Error { message, .. } => Err(HarnessError::DispatchError { message }),
            _ => Err(HarnessError::DispatchError {
                message: "unexpected unsuppress response".into(),
            }),
        }
    }

    /// Delete a feature by name.
    pub fn delete_feature(&mut self, name: &str) -> Result<&mut Self, HarnessError> {
        let id = self.feature_id(name)?;
        let response = wasm_bridge::dispatch(
            &mut self.state,
            UiToEngine::DeleteFeature { feature_id: id },
            self.kernel.as_mut(),
        );
        match response {
            EngineToUi::ModelUpdated { .. } => {
                self.named_features.remove(name);
                Ok(self)
            }
            EngineToUi::Error { message, .. } => Err(HarnessError::DispatchError { message }),
            _ => Err(HarnessError::DispatchError {
                message: "unexpected delete response".into(),
            }),
        }
    }

    /// Reorder a feature by name to a new position.
    pub fn reorder(&mut self, name: &str, position: usize) -> Result<&mut Self, HarnessError> {
        let id = self.feature_id(name)?;
        let response = wasm_bridge::dispatch(
            &mut self.state,
            UiToEngine::ReorderFeature {
                feature_id: id,
                new_position: position,
            },
            self.kernel.as_mut(),
        );
        match response {
            EngineToUi::ModelUpdated { .. } => Ok(self),
            EngineToUi::Error { message, .. } => Err(HarnessError::DispatchError { message }),
            _ => Err(HarnessError::DispatchError {
                message: "unexpected reorder response".into(),
            }),
        }
    }

    // ── Queries ─────────────────────────────────────────────────────────

    /// Get the UUID of a named feature.
    pub fn feature_id(&self, name: &str) -> Result<Uuid, HarnessError> {
        self.named_features
            .get(name)
            .copied()
            .ok_or_else(|| HarnessError::FeatureNotFound {
                name: name.to_string(),
            })
    }

    /// Get the current feature count.
    pub fn feature_count(&self) -> usize {
        self.state.engine.tree.features.len()
    }

    /// Get the solid handle for a named feature.
    pub fn solid_handle(&self, name: &str) -> Result<KernelSolidHandle, HarnessError> {
        let id = self.feature_id(name)?;
        let result = self
            .state
            .engine
            .get_result(id)
            .ok_or_else(|| HarnessError::NoSolid {
                name: name.to_string(),
            })?;
        if result.outputs.is_empty() {
            return Err(HarnessError::NoSolid {
                name: name.to_string(),
            });
        }
        Ok(result.outputs[0].1.handle.clone())
    }

    /// Tessellate a named feature's solid.
    pub fn tessellate(&mut self, name: &str) -> Result<RenderMesh, HarnessError> {
        let handle = self.solid_handle(name)?;
        self.kernel
            .tessellate(&handle, 0.1)
            .map_err(|e| HarnessError::Engine(e.to_string()))
    }

    /// Get topology counts (V, E, F) for a named feature's solid.
    pub fn topology_counts(&self, name: &str) -> Result<(usize, usize, usize), HarnessError> {
        let handle = self.solid_handle(name)?;
        let introspect = self.kernel.as_introspect();
        let v = introspect.list_vertices(&handle).len();
        let e = introspect.list_edges(&handle).len();
        let f = introspect.list_faces(&handle).len();
        Ok((v, e, f))
    }

    /// Get face signatures for a named feature's solid.
    pub fn face_signatures(
        &self,
        name: &str,
    ) -> Result<Vec<(kernel_fork::KernelId, TopoSignature)>, HarnessError> {
        let handle = self.solid_handle(name)?;
        let introspect = self.kernel.as_introspect();
        Ok(introspect.compute_all_signatures(&handle, TopoKind::Face))
    }

    /// Build a GeomRef to select a face by role on a named feature.
    pub fn select_face_by_role(
        &self,
        name: &str,
        role: Role,
        idx: usize,
    ) -> Result<GeomRef, HarnessError> {
        let id = self.feature_id(name)?;
        Ok(face_ref(id, role, idx))
    }

    /// Select a face by approximate normal direction.
    pub fn select_face_by_normal(
        &self,
        name: &str,
        normal: [f64; 3],
        tol: f64,
    ) -> Result<GeomRef, HarnessError> {
        let id = self.feature_id(name)?;
        Ok(GeomRef {
            kind: TopoKind::Face,
            anchor: Anchor::FeatureOutput {
                feature_id: id,
                output_key: OutputKey::Main,
            },
            selector: Selector::Query {
                query: TopoQuery {
                    filters: vec![Filter::NormalDirection {
                        direction: normal,
                        tolerance: tol,
                    }],
                    tie_break: Some(TieBreak::LargestArea),
                },
            },
            policy: ResolvePolicy::BestEffort,
        })
    }

    /// Get a reference to the kernel bundle (for direct oracle calls).
    pub fn kernel(&self) -> &dyn KernelBundle {
        self.kernel.as_ref()
    }

    /// Get a mutable reference to the kernel bundle.
    pub fn kernel_mut(&mut self) -> &mut dyn KernelBundle {
        self.kernel.as_mut()
    }

    /// Get engine errors.
    pub fn engine_errors(&self) -> &[(Uuid, String)] {
        &self.state.engine.errors
    }

    /// Get the OpResult for a named feature (if it has one).
    pub fn op_result(&self, name: &str) -> Result<&OpResult, HarnessError> {
        let id = self.feature_id(name)?;
        self.state
            .engine
            .get_result(id)
            .ok_or_else(|| HarnessError::NoSolid {
                name: name.to_string(),
            })
    }

    /// Get the dispatch history log.
    pub fn history(&self) -> &[(String, String)] {
        &self.history
    }

    // ── File I/O ────────────────────────────────────────────────────────

    /// Save the project and return the JSON string.
    pub fn save(&mut self) -> Result<String, HarnessError> {
        let response = wasm_bridge::dispatch(
            &mut self.state,
            UiToEngine::SaveProject,
            self.kernel.as_mut(),
        );
        match response {
            EngineToUi::SaveReady { json_data } => Ok(json_data),
            EngineToUi::Error { message, .. } => Err(HarnessError::DispatchError { message }),
            _ => Err(HarnessError::DispatchError {
                message: "unexpected save response".into(),
            }),
        }
    }

    /// Load a project from JSON, replacing the current state.
    pub fn load(&mut self, json: &str) -> Result<&mut Self, HarnessError> {
        let response = wasm_bridge::dispatch(
            &mut self.state,
            UiToEngine::LoadProject {
                data: json.to_string(),
            },
            self.kernel.as_mut(),
        );
        match response {
            EngineToUi::ModelUpdated { .. } => {
                // Re-map named features from the loaded tree
                self.named_features.clear();
                for feature in &self.state.engine.tree.features {
                    self.named_features.insert(feature.name.clone(), feature.id);
                }
                Ok(self)
            }
            EngineToUi::Error { message, .. } => Err(HarnessError::DispatchError { message }),
            _ => Err(HarnessError::DispatchError {
                message: "unexpected load response".into(),
            }),
        }
    }

    /// Export a named feature's solid as binary STL.
    pub fn export_stl(&mut self, name: &str) -> Result<Vec<u8>, HarnessError> {
        let mesh = self.tessellate(name)?;
        stl::export_binary_stl(&mesh, name)
    }

    // ── Inline Assertions ───────────────────────────────────────────────

    /// Assert the feature tree has exactly `expected` features.
    pub fn assert_feature_count(&self, expected: usize) -> Result<&Self, HarnessError> {
        let actual = self.feature_count();
        if actual == expected {
            Ok(self)
        } else {
            Err(HarnessError::AssertionFailed {
                detail: format!(
                    "expected {} features, got {}. Features: {:?}",
                    expected,
                    actual,
                    self.state
                        .engine
                        .tree
                        .features
                        .iter()
                        .map(|f| &f.name)
                        .collect::<Vec<_>>()
                ),
            })
        }
    }

    /// Assert that a named feature has a solid (non-empty OpResult outputs).
    pub fn assert_has_solid(&self, name: &str) -> Result<&Self, HarnessError> {
        let id = self.feature_id(name)?;
        let result = self.state.engine.get_result(id);
        match result {
            Some(r) if !r.outputs.is_empty() => Ok(self),
            _ => Err(HarnessError::NoSolid {
                name: name.to_string(),
            }),
        }
    }

    /// Assert no engine errors exist.
    pub fn assert_no_errors(&self) -> Result<&Self, HarnessError> {
        if self.state.engine.errors.is_empty() {
            Ok(self)
        } else {
            Err(HarnessError::AssertionFailed {
                detail: format!(
                    "expected no errors, got {}: {:?}",
                    self.state.engine.errors.len(),
                    self.state.engine.errors
                ),
            })
        }
    }

    /// Assert that engine errors exist (useful for negative testing).
    pub fn assert_has_errors(&self) -> Result<&Self, HarnessError> {
        if !self.state.engine.errors.is_empty() {
            Ok(self)
        } else {
            Err(HarnessError::AssertionFailed {
                detail: "expected errors, but none found".to_string(),
            })
        }
    }

    // ── Oracle Integration ──────────────────────────────────────────────

    /// Run all mesh oracles on a named feature's tessellation.
    pub fn check_mesh(&mut self, name: &str) -> Result<Vec<oracle::OracleVerdict>, HarnessError> {
        let mesh = self.tessellate(name)?;
        Ok(oracle::run_all_mesh_checks(&mesh))
    }

    /// Run topology oracles on a named feature's solid.
    pub fn check_topology(&self, name: &str) -> Result<Vec<oracle::OracleVerdict>, HarnessError> {
        let handle = self.solid_handle(name)?;
        let introspect = self.kernel.as_introspect();
        Ok(oracle::run_topology_checks(introspect, &handle))
    }

    // ── Internal Helpers ────────────────────────────────────────────────

    fn check_name_available(&self, name: &str) -> Result<(), HarnessError> {
        if self.named_features.contains_key(name) {
            Err(HarnessError::DuplicateName {
                name: name.to_string(),
            })
        } else {
            Ok(())
        }
    }

    fn extract_last_feature_id(
        &mut self,
        name: &str,
        msg_type: &str,
        response: EngineToUi,
    ) -> Result<Uuid, HarnessError> {
        match response {
            EngineToUi::ModelUpdated { feature_tree, .. } => {
                let id = feature_tree
                    .features
                    .last()
                    .ok_or_else(|| HarnessError::DispatchError {
                        message: format!("{}: ModelUpdated but no features", msg_type),
                    })?
                    .id;
                self.named_features.insert(name.to_string(), id);
                self.history
                    .push((msg_type.to_string(), "ModelUpdated".to_string()));
                if self.auto_check {
                    self.check_errors()?;
                }
                Ok(id)
            }
            EngineToUi::Error { message, .. } => Err(HarnessError::DispatchError { message }),
            _ => Err(HarnessError::DispatchError {
                message: format!("{}: unexpected response", msg_type),
            }),
        }
    }

    fn check_errors(&self) -> Result<(), HarnessError> {
        if !self.state.engine.errors.is_empty() {
            let last = &self.state.engine.errors[self.state.engine.errors.len() - 1];
            Err(HarnessError::Engine(format!(
                "auto_check: engine error on feature {}: {}",
                last.0, last.1
            )))
        } else {
            Ok(())
        }
    }
}
