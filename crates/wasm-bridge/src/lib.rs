use serde::{Deserialize, Serialize};

use cad_kernel::geometry::point::Point3d;
use cad_kernel::geometry::vector::Vec3;
use cad_kernel::operations::chamfer::chamfer_edge;
use cad_kernel::operations::extrude::{extrude_profile, Profile};
use cad_kernel::operations::feature::{Feature, FeatureTree, Parameter, SketchProfile};
use cad_kernel::operations::fillet::fillet_edge;
use cad_kernel::operations::revolve::revolve_profile;
use cad_kernel::topology::brep::EntityStore;
use cad_kernel::topology::primitives;
use cad_kernel::boolean::engine::{boolean_op, BoolOp};
use cad_kernel::validation::audit::full_verify;
use cad_solver::{Constraint, Sketch, SolverConfig, solve_sketch};
use cad_tessellation::{tessellate_solid, mesh_to_obj, mesh_to_stl, TriangleMesh};

/// The main CAD engine state, designed to be used from WASM.
#[derive(Default)]
pub struct CadEngine {
    store: EntityStore,
    solids: Vec<cad_kernel::topology::brep::SolidId>,
    feature_tree: FeatureTree,
}

/// Serializable mesh data for sending to JavaScript.
#[derive(Debug, Serialize, Deserialize)]
pub struct MeshData {
    pub positions: Vec<f32>,
    pub normals: Vec<f32>,
    pub indices: Vec<u32>,
}

impl From<TriangleMesh> for MeshData {
    fn from(mesh: TriangleMesh) -> Self {
        Self {
            positions: mesh.positions,
            normals: mesh.normals,
            indices: mesh.indices,
        }
    }
}

/// Serializable model info.
#[derive(Debug, Serialize, Deserialize)]
pub struct ModelInfo {
    pub solid_count: usize,
    pub face_count: usize,
    pub edge_count: usize,
    pub vertex_count: usize,
}

/// Serializable verification report.
#[derive(Debug, Serialize, Deserialize)]
pub struct VerifyReport {
    pub topology_valid: bool,
    pub geometry_errors: usize,
}

/// Sketch solver result data.
#[derive(Debug, Serialize, Deserialize)]
pub struct SolveResult {
    pub converged: bool,
    pub iterations: usize,
    pub residual: f64,
    pub points: Vec<(f64, f64)>,
}

impl CadEngine {
    pub fn new() -> Self {
        Self::default()
    }

    // ── Primitive creation ──────────────────────────────────────────

    /// Create a box primitive and return its index.
    pub fn create_box(&mut self, x0: f64, y0: f64, z0: f64, x1: f64, y1: f64, z1: f64) -> usize {
        let solid_id = primitives::make_box(&mut self.store, x0, y0, z0, x1, y1, z1);
        self.solids.push(solid_id);
        self.solids.len() - 1
    }

    /// Create a cylinder primitive and return its index.
    pub fn create_cylinder(&mut self, cx: f64, cy: f64, cz: f64, radius: f64, height: f64) -> usize {
        let solid_id = primitives::make_cylinder(
            &mut self.store,
            Point3d::new(cx, cy, cz),
            radius,
            height,
            24,
        );
        self.solids.push(solid_id);
        self.solids.len() - 1
    }

    /// Create a sphere primitive and return its index.
    pub fn create_sphere(&mut self, cx: f64, cy: f64, cz: f64, radius: f64) -> usize {
        let solid_id = primitives::make_sphere(
            &mut self.store,
            Point3d::new(cx, cy, cz),
            radius,
            16,
            12,
        );
        self.solids.push(solid_id);
        self.solids.len() - 1
    }

    // ── Profile operations ──────────────────────────────────────────

    /// Extrude a rectangular profile and return the solid index.
    pub fn extrude_rectangle(&mut self, width: f64, height: f64, depth: f64) -> usize {
        let profile = Profile::rectangle(width, height);
        let solid_id = extrude_profile(&mut self.store, &profile, Vec3::Z, depth);
        self.solids.push(solid_id);
        self.solids.len() - 1
    }

    /// Extrude an arbitrary polygon profile along Z.
    pub fn extrude_polygon(&mut self, points: &[(f64, f64)], depth: f64) -> usize {
        let pts: Vec<Point3d> = points.iter().map(|&(x, y)| Point3d::new(x, y, 0.0)).collect();
        let profile = Profile::from_points(pts);
        let solid_id = extrude_profile(&mut self.store, &profile, Vec3::Z, depth);
        self.solids.push(solid_id);
        self.solids.len() - 1
    }

    /// Revolve a profile around the Z axis.
    pub fn revolve_polygon(
        &mut self,
        points: &[(f64, f64, f64)],
        angle: f64,
        segments: usize,
    ) -> usize {
        let pts: Vec<Point3d> = points.iter().map(|&(x, y, z)| Point3d::new(x, y, z)).collect();
        let solid_id = revolve_profile(
            &mut self.store,
            &pts,
            Point3d::ORIGIN,
            Vec3::Z,
            angle,
            segments,
        );
        self.solids.push(solid_id);
        self.solids.len() - 1
    }

    // ── Edge operations ─────────────────────────────────────────────

    /// Chamfer an edge of a solid. Returns new solid index.
    pub fn chamfer(
        &mut self,
        solid_idx: usize,
        v0: (f64, f64, f64),
        v1: (f64, f64, f64),
        distance: f64,
    ) -> usize {
        let solid_id = self.solids[solid_idx];
        let new_id = chamfer_edge(
            &mut self.store,
            solid_id,
            Point3d::new(v0.0, v0.1, v0.2),
            Point3d::new(v1.0, v1.1, v1.2),
            distance,
        );
        self.solids.push(new_id);
        self.solids.len() - 1
    }

    /// Fillet an edge of a solid. Returns new solid index.
    pub fn fillet(
        &mut self,
        solid_idx: usize,
        v0: (f64, f64, f64),
        v1: (f64, f64, f64),
        radius: f64,
        segments: usize,
    ) -> usize {
        let solid_id = self.solids[solid_idx];
        let new_id = fillet_edge(
            &mut self.store,
            solid_id,
            Point3d::new(v0.0, v0.1, v0.2),
            Point3d::new(v1.0, v1.1, v1.2),
            radius,
            segments,
        );
        self.solids.push(new_id);
        self.solids.len() - 1
    }

    // ── Boolean operations ──────────────────────────────────────────

    /// Perform a Boolean operation between two solids.
    pub fn boolean(&mut self, a_idx: usize, b_idx: usize, op: &str) -> Result<usize, String> {
        let solid_a = self.solids[a_idx];
        let solid_b = self.solids[b_idx];

        let op_type = match op {
            "union" => BoolOp::Union,
            "intersection" => BoolOp::Intersection,
            "difference" => BoolOp::Difference,
            _ => return Err(format!("Unknown operation: {}", op)),
        };

        match boolean_op(&mut self.store, solid_a, solid_b, op_type) {
            Ok(result_id) => {
                self.solids.push(result_id);
                Ok(self.solids.len() - 1)
            }
            Err(e) => Err(format!("Boolean operation failed: {}", e)),
        }
    }

    // ── Tessellation & export ───────────────────────────────────────

    /// Tessellate a solid and return mesh data.
    pub fn tessellate(&self, solid_idx: usize) -> MeshData {
        let solid_id = self.solids[solid_idx];
        let mesh = tessellate_solid(&self.store, solid_id);
        MeshData::from(mesh)
    }

    /// Export a solid to OBJ format.
    pub fn export_obj(&self, solid_idx: usize) -> String {
        let solid_id = self.solids[solid_idx];
        let mesh = tessellate_solid(&self.store, solid_id);
        mesh_to_obj(&mesh)
    }

    /// Export a solid to binary STL format.
    pub fn export_stl(&self, solid_idx: usize) -> Vec<u8> {
        let solid_id = self.solids[solid_idx];
        let mesh = tessellate_solid(&self.store, solid_id);
        mesh_to_stl(&mesh)
    }

    // ── Model queries ───────────────────────────────────────────────

    /// Get model info for a solid.
    pub fn model_info(&self, solid_idx: usize) -> ModelInfo {
        let solid_id = self.solids[solid_idx];
        let solid = &self.store.solids[solid_id];

        let mut face_count = 0;
        let mut edge_count = 0;
        let mut vertex_count = 0;

        for &shell_id in &solid.shells {
            let (v, e, f) = self.store.count_topology(shell_id);
            vertex_count += v;
            edge_count += e;
            face_count += f;
        }

        ModelInfo {
            solid_count: 1,
            face_count,
            edge_count,
            vertex_count,
        }
    }

    /// Verify solid topology and geometry.
    pub fn verify(&self, solid_idx: usize) -> VerifyReport {
        let solid_id = self.solids[solid_idx];
        let report = full_verify(&self.store, solid_id);
        VerifyReport {
            topology_valid: report.topology_valid,
            geometry_errors: report.geometry_errors.len(),
        }
    }

    /// Get bounding box of a solid: ((min_x, min_y, min_z), (max_x, max_y, max_z)).
    pub fn bounding_box(&self, solid_idx: usize) -> ((f64, f64, f64), (f64, f64, f64)) {
        let solid_id = self.solids[solid_idx];
        let bb = self.store.solid_bounding_box(solid_id);
        ((bb.min.x, bb.min.y, bb.min.z), (bb.max.x, bb.max.y, bb.max.z))
    }

    pub fn solid_count(&self) -> usize {
        self.solids.len()
    }

    // ── Feature tree ────────────────────────────────────────────────

    /// Add a parameter to the feature tree.
    pub fn add_parameter(&mut self, name: &str, value: f64) -> usize {
        self.feature_tree.add_parameter(Parameter::new(name, value))
    }

    /// Set a parameter value by name.
    pub fn set_parameter(&mut self, name: &str, value: f64) -> bool {
        self.feature_tree.set_parameter_value(name, value)
    }

    /// Add a sketch feature with a rectangle profile.
    pub fn add_sketch_rect(&mut self, w: f64, h: f64) -> usize {
        let hw = w / 2.0;
        let hh = h / 2.0;
        self.feature_tree.add_feature(Feature::Sketch {
            plane_origin: [0.0, 0.0, 0.0],
            plane_normal: [0.0, 0.0, 1.0],
            profiles: vec![SketchProfile {
                points: vec![[-hw, -hh], [hw, -hh], [hw, hh], [-hw, hh]],
                closed: true,
            }],
        })
    }

    /// Add an extrude feature referencing a sketch.
    pub fn add_extrude(&mut self, sketch_idx: usize, height: f64, symmetric: bool) -> usize {
        self.feature_tree.add_feature(Feature::Extrude {
            sketch_index: sketch_idx,
            distance: Parameter::new("extrude_height", height),
            direction: [0.0, 0.0, 1.0],
            symmetric,
        })
    }

    /// Evaluate the feature tree, replacing all solids.
    pub fn evaluate_features(&mut self) -> usize {
        let mut new_store = EntityStore::new();
        let new_solids = self.feature_tree.evaluate(&mut new_store);
        let count = new_solids.len();
        self.store = new_store;
        self.solids = new_solids;
        count
    }

    // ── Sketch solver ───────────────────────────────────────────────

    /// Solve a constrained sketch and return results.
    pub fn solve_sketch_ops(&mut self, ops: &[(&str, f64, f64, f64, f64)]) -> Result<SolveResult, String> {
        let mut sketch = Sketch::new();

        for &(op, a, b, c, _d) in ops {
            match op {
                "point" => { sketch.add_point(a, b); }
                "line" => { sketch.add_line(a as usize, b as usize); }
                "circle" => { sketch.add_circle(a, b, c); }
                "fix" => {
                    sketch.add_constraint(Constraint::Fixed {
                        point: a as usize, x: b, y: c,
                    });
                }
                "horizontal" => {
                    sketch.add_constraint(Constraint::Horizontal { line: a as usize });
                }
                "vertical" => {
                    sketch.add_constraint(Constraint::Vertical { line: a as usize });
                }
                "distance" => {
                    sketch.add_constraint(Constraint::Distance {
                        point_a: a as usize, point_b: b as usize, value: c,
                    });
                }
                "coincident" => {
                    sketch.add_constraint(Constraint::Coincident {
                        point_a: a as usize, point_b: b as usize,
                    });
                }
                "parallel" => {
                    sketch.add_constraint(Constraint::Parallel {
                        line_a: a as usize, line_b: b as usize,
                    });
                }
                "perpendicular" => {
                    sketch.add_constraint(Constraint::Perpendicular {
                        line_a: a as usize, line_b: b as usize,
                    });
                }
                "radius" => {
                    sketch.add_constraint(Constraint::Radius {
                        entity: a as usize, value: b,
                    });
                }
                "tangent" => {
                    sketch.add_constraint(Constraint::Tangent {
                        entity_a: a as usize, entity_b: b as usize,
                    });
                }
                _ => return Err(format!("Unknown sketch operation: {op}")),
            }
        }

        let config = SolverConfig::default();
        match solve_sketch(&mut sketch, &config) {
            Ok(result) => {
                let points: Vec<(f64, f64)> = sketch
                    .point_entities()
                    .iter()
                    .map(|&idx| sketch.point_position(idx))
                    .collect();
                Ok(SolveResult {
                    converged: result.converged,
                    iterations: result.iterations,
                    residual: result.final_residual,
                    points,
                })
            }
            Err(e) => Err(format!("Solver error: {}", e)),
        }
    }

    /// Solve a sketch and extrude the resulting profile.
    pub fn sketch_and_extrude(
        &mut self,
        ops: &[(&str, f64, f64, f64, f64)],
        depth: f64,
    ) -> Result<usize, String> {
        let mut sketch = Sketch::new();

        for &(op, a, b, c, _d) in ops {
            match op {
                "point" => { sketch.add_point(a, b); }
                "line" => { sketch.add_line(a as usize, b as usize); }
                "fix" => {
                    sketch.add_constraint(Constraint::Fixed {
                        point: a as usize, x: b, y: c,
                    });
                }
                "horizontal" => {
                    sketch.add_constraint(Constraint::Horizontal { line: a as usize });
                }
                "vertical" => {
                    sketch.add_constraint(Constraint::Vertical { line: a as usize });
                }
                "distance" => {
                    sketch.add_constraint(Constraint::Distance {
                        point_a: a as usize, point_b: b as usize, value: c,
                    });
                }
                _ => {}
            }
        }

        let config = SolverConfig { max_iterations: 200, ..SolverConfig::default() };
        solve_sketch(&mut sketch, &config).map_err(|e| format!("Solver: {e}"))?;

        match sketch.extract_profile() {
            Some(pts) => {
                let profile_pts: Vec<Point3d> =
                    pts.iter().map(|&(x, y)| Point3d::new(x, y, 0.0)).collect();
                let profile = Profile::from_points(profile_pts);
                let solid_id = extrude_profile(&mut self.store, &profile, Vec3::Z, depth);
                self.solids.push(solid_id);
                Ok(self.solids.len() - 1)
            }
            None => Err("Could not extract closed profile from sketch".to_string()),
        }
    }
}

/// Serialize mesh data to JSON for WASM bridge.
pub fn mesh_to_json(mesh: &MeshData) -> String {
    serde_json::to_string(mesh).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_engine_create_box() {
        let mut engine = CadEngine::new();
        let idx = engine.create_box(0.0, 0.0, 0.0, 1.0, 1.0, 1.0);

        let info = engine.model_info(idx);
        assert_eq!(info.face_count, 6);
        assert_eq!(info.vertex_count, 8);
        assert_eq!(info.edge_count, 12);
    }

    #[test]
    fn test_engine_tessellate() {
        let mut engine = CadEngine::new();
        let idx = engine.create_box(0.0, 0.0, 0.0, 1.0, 1.0, 1.0);

        let mesh = engine.tessellate(idx);
        assert!(!mesh.positions.is_empty());
        assert!(!mesh.indices.is_empty());
    }

    #[test]
    fn test_engine_boolean_non_overlapping() {
        let mut engine = CadEngine::new();
        let a = engine.create_box(0.0, 0.0, 0.0, 1.0, 1.0, 1.0);
        let b = engine.create_box(5.0, 5.0, 5.0, 6.0, 6.0, 6.0);

        let result = engine.boolean(a, b, "union");
        assert!(result.is_ok());
    }

    #[test]
    fn test_engine_extrude_rectangle() {
        let mut engine = CadEngine::new();
        let idx = engine.extrude_rectangle(10.0, 5.0, 20.0);

        let info = engine.model_info(idx);
        assert_eq!(info.face_count, 6);
    }

    #[test]
    fn test_engine_extrude_polygon() {
        let mut engine = CadEngine::new();
        let points = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 5.0), (0.0, 5.0)];
        let idx = engine.extrude_polygon(&points, 8.0);

        let info = engine.model_info(idx);
        assert_eq!(info.face_count, 6);
    }

    #[test]
    fn test_engine_chamfer() {
        let mut engine = CadEngine::new();
        let box_idx = engine.create_box(0.0, 0.0, 0.0, 10.0, 10.0, 10.0);
        let chamf_idx = engine.chamfer(box_idx, (0.0, 0.0, 0.0), (10.0, 0.0, 0.0), 2.0);

        let info = engine.model_info(chamf_idx);
        assert_eq!(info.face_count, 7, "Chamfered box should have 7 faces");
    }

    #[test]
    fn test_engine_fillet() {
        let mut engine = CadEngine::new();
        let box_idx = engine.create_box(0.0, 0.0, 0.0, 10.0, 10.0, 10.0);
        let fillet_idx = engine.fillet(box_idx, (0.0, 0.0, 0.0), (10.0, 0.0, 0.0), 2.0, 4);

        let info = engine.model_info(fillet_idx);
        assert!(info.face_count >= 9, "Filleted box should have >= 9 faces, got {}", info.face_count);
    }

    #[test]
    fn test_engine_verify_box() {
        let mut engine = CadEngine::new();
        let idx = engine.create_box(0.0, 0.0, 0.0, 1.0, 1.0, 1.0);
        let report = engine.verify(idx);
        assert!(report.topology_valid);
    }

    #[test]
    fn test_engine_bounding_box() {
        let mut engine = CadEngine::new();
        let idx = engine.create_box(1.0, 2.0, 3.0, 4.0, 5.0, 6.0);
        let ((min_x, min_y, min_z), (max_x, max_y, max_z)) = engine.bounding_box(idx);
        assert!((min_x - 1.0).abs() < 1e-6);
        assert!((min_y - 2.0).abs() < 1e-6);
        assert!((min_z - 3.0).abs() < 1e-6);
        assert!((max_x - 4.0).abs() < 1e-6);
        assert!((max_y - 5.0).abs() < 1e-6);
        assert!((max_z - 6.0).abs() < 1e-6);
    }

    #[test]
    fn test_engine_export_obj() {
        let mut engine = CadEngine::new();
        let idx = engine.create_box(0.0, 0.0, 0.0, 1.0, 1.0, 1.0);
        let obj = engine.export_obj(idx);
        assert!(obj.contains("v "));
        assert!(obj.contains("f "));
    }

    #[test]
    fn test_engine_export_stl() {
        let mut engine = CadEngine::new();
        let idx = engine.create_box(0.0, 0.0, 0.0, 1.0, 1.0, 1.0);
        let stl = engine.export_stl(idx);
        assert!(stl.len() > 84);
    }

    #[test]
    fn test_engine_solve_sketch() {
        let mut engine = CadEngine::new();
        let ops = vec![
            ("point", 0.0, 0.0, 0.0, 0.0),
            ("point", 9.0, 0.5, 0.0, 0.0),
            ("point", 9.5, 4.5, 0.0, 0.0),
            ("point", 0.5, 5.5, 0.0, 0.0),
            ("line", 0.0, 1.0, 0.0, 0.0),
            ("line", 1.0, 2.0, 0.0, 0.0),
            ("line", 2.0, 3.0, 0.0, 0.0),
            ("line", 3.0, 0.0, 0.0, 0.0),
            ("fix", 0.0, 0.0, 0.0, 0.0),
            ("horizontal", 4.0, 0.0, 0.0, 0.0),
            ("horizontal", 6.0, 0.0, 0.0, 0.0),
            ("vertical", 5.0, 0.0, 0.0, 0.0),
            ("vertical", 7.0, 0.0, 0.0, 0.0),
            ("distance", 0.0, 1.0, 10.0, 0.0),
            ("distance", 1.0, 2.0, 5.0, 0.0),
        ];

        let result = engine.solve_sketch_ops(&ops);
        assert!(result.is_ok(), "Solver failed: {:?}", result.err());
        let sr = result.unwrap();
        assert!(sr.converged);
        assert_eq!(sr.points.len(), 4);
    }

    #[test]
    fn test_engine_sketch_and_extrude() {
        let mut engine = CadEngine::new();
        let ops = vec![
            ("point", 0.0, 0.0, 0.0, 0.0),
            ("point", 10.0, 0.0, 0.0, 0.0),
            ("point", 10.0, 5.0, 0.0, 0.0),
            ("point", 0.0, 5.0, 0.0, 0.0),
            ("line", 0.0, 1.0, 0.0, 0.0),
            ("line", 1.0, 2.0, 0.0, 0.0),
            ("line", 2.0, 3.0, 0.0, 0.0),
            ("line", 3.0, 0.0, 0.0, 0.0),
            ("fix", 0.0, 0.0, 0.0, 0.0),
            ("horizontal", 4.0, 0.0, 0.0, 0.0),
            ("vertical", 5.0, 0.0, 0.0, 0.0),
        ];

        let result = engine.sketch_and_extrude(&ops, 15.0);
        assert!(result.is_ok(), "Sketch+extrude failed: {:?}", result.err());
        let idx = result.unwrap();

        let info = engine.model_info(idx);
        assert_eq!(info.face_count, 6);

        let (_, (_, _, max_z)) = engine.bounding_box(idx);
        assert!((max_z - 15.0).abs() < 0.5, "Height should be ~15, got {max_z}");
    }

    #[test]
    fn test_engine_feature_tree() {
        let mut engine = CadEngine::new();
        engine.add_sketch_rect(10.0, 5.0);
        engine.add_extrude(0, 20.0, false);
        let count = engine.evaluate_features();
        assert_eq!(count, 1);

        let info = engine.model_info(0);
        assert_eq!(info.face_count, 6);
    }

    #[test]
    fn test_engine_mesh_to_json() {
        let mut engine = CadEngine::new();
        let idx = engine.create_box(0.0, 0.0, 0.0, 1.0, 1.0, 1.0);
        let mesh = engine.tessellate(idx);
        let json = mesh_to_json(&mesh);
        assert!(json.contains("positions"));
        assert!(json.contains("indices"));
    }

    #[test]
    fn test_engine_revolve() {
        let mut engine = CadEngine::new();
        let points = vec![
            (3.0, 0.0, 0.0),
            (5.0, 0.0, 4.0),
            (3.5, 0.0, 8.0),
        ];
        let idx = engine.revolve_polygon(&points, std::f64::consts::TAU, 12);
        let info = engine.model_info(idx);
        assert!(info.face_count > 0, "Revolved solid should have faces");
    }
}
