use serde::{Deserialize, Serialize};

use cad_kernel::geometry::point::Point3d;
use cad_kernel::geometry::vector::Vec3;
use cad_kernel::topology::brep::EntityStore;
use cad_kernel::topology::primitives;
use cad_kernel::operations::extrude::{Profile, extrude_profile};
use cad_kernel::boolean::engine::{boolean_op, BoolOp};
use cad_tessellation::{tessellate_solid, TriangleMesh};

/// The main CAD engine state, designed to be used from WASM.
#[derive(Default)]
pub struct CadEngine {
    store: EntityStore,
    solids: Vec<cad_kernel::topology::brep::SolidId>,
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

impl CadEngine {
    pub fn new() -> Self {
        Self::default()
    }

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

    /// Extrude a rectangular profile and return the solid index.
    pub fn extrude_rectangle(&mut self, width: f64, height: f64, depth: f64) -> usize {
        let profile = Profile::rectangle(width, height);
        let solid_id = extrude_profile(&mut self.store, &profile, Vec3::Z, depth);
        self.solids.push(solid_id);
        self.solids.len() - 1
    }

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

    /// Tessellate a solid and return mesh data.
    pub fn tessellate(&self, solid_idx: usize) -> MeshData {
        let solid_id = self.solids[solid_idx];
        let mesh = tessellate_solid(&self.store, solid_id);
        MeshData::from(mesh)
    }

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

    pub fn solid_count(&self) -> usize {
        self.solids.len()
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
}
