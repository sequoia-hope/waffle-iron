//! Recipe executor — translates declarative AssayRecipe into ModelBuilder operations.
//!
//! Uses the existing ModelBuilder infrastructure (rect_sketch, circle_sketch,
//! extrude, boolean_union, etc.) and existing oracles (mesh_volume, watertight).

use crate::assay::catalog::{AssayRecipe, BoolOp, Profile};
use crate::assay::scoring::ExecutionResult;

use crate::workflow::ModelBuilder;
use kernel::types::RenderMesh;

/// Execute a recipe against a ModelBuilder and return measured results.
pub fn execute_recipe(
    builder: &mut ModelBuilder,
    recipe: &AssayRecipe,
) -> Result<ExecutionResult, String> {
    let mut counter = Counter::new();

    let final_name = build_solid(builder, &mut counter, recipe, false)?;

    // Tessellate
    let mesh = builder
        .tessellate(&final_name)
        .map_err(|e| format!("Tessellation failed: {}", e))?;

    // Volume from mesh (signed volume via divergence theorem)
    let volume = mesh_volume(&mesh);

    // Topology from introspection
    let (v, e, f) = builder
        .topology_counts(&final_name)
        .map_err(|e| format!("Topology query failed: {}", e))?;

    let euler = if v > 0 || e > 0 || f > 0 {
        Some(v as i64 - e as i64 + f as i64)
    } else {
        None
    };

    let face_count = if f > 0 { Some(f) } else { None };

    // Watertight check
    let watertight = check_watertight(&mesh);

    // Bounding box
    let bbox = mesh_bbox(&mesh);

    Ok(ExecutionResult {
        volume: Some(volume),
        euler,
        face_count,
        watertight,
        bbox,
    })
}

/// Counter for generating unique names within a recipe execution.
struct Counter(usize);

impl Counter {
    fn new() -> Self {
        Counter(0)
    }
    fn next(&mut self, prefix: &str) -> String {
        self.0 += 1;
        format!("{}_{}", prefix, self.0)
    }
}

/// Recursively build a solid from a recipe, returning the feature name.
///
/// `for_boolean`: when true, uses `extrude_no_merge` so bodies remain separate
/// for explicit boolean operations (prevents auto-union during extrude).
fn build_solid(
    builder: &mut ModelBuilder,
    counter: &mut Counter,
    recipe: &AssayRecipe,
    for_boolean: bool,
) -> Result<String, String> {
    match recipe {
        AssayRecipe::Extrude {
            profile,
            origin,
            normal,
            depth,
        } => {
            let sketch_name = counter.next("sketch");
            let extrude_name = counter.next("extrude");

            match profile {
                Profile::Rect { cx, cy, w, h } => {
                    builder
                        .rect_sketch(&sketch_name, *origin, *normal, *cx, *cy, *w, *h)
                        .map_err(|e| format!("rect_sketch failed: {}", e))?;
                }
                Profile::Circle { cx, cy, r } => {
                    builder
                        .circle_sketch(&sketch_name, *origin, *normal, *cx, *cy, *r)
                        .map_err(|e| format!("circle_sketch failed: {}", e))?;
                }
            }

            if for_boolean {
                builder
                    .extrude_no_merge(&extrude_name, &sketch_name, *depth)
                    .map_err(|e| format!("extrude failed: {}", e))?;
            } else {
                builder
                    .extrude(&extrude_name, &sketch_name, *depth)
                    .map_err(|e| format!("extrude failed: {}", e))?;
            }

            Ok(extrude_name)
        }

        AssayRecipe::Boolean { a, b, op } => {
            let name_a = build_solid(builder, counter, a, true)?;
            let name_b = build_solid(builder, counter, b, true)?;
            let bool_name = counter.next("boolean");

            match op {
                BoolOp::Union => builder
                    .boolean_union(&bool_name, &name_a, &name_b)
                    .map_err(|e| format!("boolean_union failed: {}", e))?,
                BoolOp::Subtract => builder
                    .boolean_subtract(&bool_name, &name_a, &name_b)
                    .map_err(|e| format!("boolean_subtract failed: {}", e))?,
                BoolOp::Intersect => builder
                    .boolean_intersect(&bool_name, &name_a, &name_b)
                    .map_err(|e| format!("boolean_intersect failed: {}", e))?,
            };

            Ok(bool_name)
        }

        AssayRecipe::Revolve {
            profile,
            origin,
            normal,
            axis_origin,
            axis_dir,
            angle_rad,
        } => {
            let sketch_name = counter.next("sketch");
            let revolve_name = counter.next("revolve");

            match profile {
                Profile::Rect { cx, cy, w, h } => {
                    builder
                        .rect_sketch(&sketch_name, *origin, *normal, *cx, *cy, *w, *h)
                        .map_err(|e| format!("rect_sketch failed: {}", e))?;
                }
                Profile::Circle { cx, cy, r } => {
                    builder
                        .circle_sketch(&sketch_name, *origin, *normal, *cx, *cy, *r)
                        .map_err(|e| format!("circle_sketch failed: {}", e))?;
                }
            }

            // ModelBuilder::revolve takes degrees
            let angle_deg = angle_rad.to_degrees();
            builder
                .revolve(
                    &revolve_name,
                    &sketch_name,
                    *axis_origin,
                    *axis_dir,
                    angle_deg,
                )
                .map_err(|e| format!("revolve failed: {}", e))?;

            Ok(revolve_name)
        }

        AssayRecipe::Chain { initial, steps } => {
            let mut current_name = build_solid(builder, counter, initial, true)?;

            for step in steps {
                let operand_name = build_solid(builder, counter, &step.operand, true)?;
                let bool_name = counter.next("chain_step");

                match step.op {
                    BoolOp::Union => builder
                        .boolean_union(&bool_name, &current_name, &operand_name)
                        .map_err(|e| format!("chain union failed: {}", e))?,
                    BoolOp::Subtract => builder
                        .boolean_subtract(&bool_name, &current_name, &operand_name)
                        .map_err(|e| format!("chain subtract failed: {}", e))?,
                    BoolOp::Intersect => builder
                        .boolean_intersect(&bool_name, &current_name, &operand_name)
                        .map_err(|e| format!("chain intersect failed: {}", e))?,
                };

                current_name = bool_name;
            }

            Ok(current_name)
        }
    }
}

/// Compute signed volume of a triangle mesh using the divergence theorem.
fn mesh_volume(mesh: &RenderMesh) -> f64 {
    let mut volume = 0.0;

    let verts = &mesh.vertices;
    let indices = &mesh.indices;

    let num_tris = indices.len() / 3;
    for i in 0..num_tris {
        let i0 = indices[i * 3] as usize;
        let i1 = indices[i * 3 + 1] as usize;
        let i2 = indices[i * 3 + 2] as usize;

        let v0 = [
            verts[i0 * 3] as f64,
            verts[i0 * 3 + 1] as f64,
            verts[i0 * 3 + 2] as f64,
        ];
        let v1 = [
            verts[i1 * 3] as f64,
            verts[i1 * 3 + 1] as f64,
            verts[i1 * 3 + 2] as f64,
        ];
        let v2 = [
            verts[i2 * 3] as f64,
            verts[i2 * 3 + 1] as f64,
            verts[i2 * 3 + 2] as f64,
        ];

        let cross = [
            v1[1] * v2[2] - v1[2] * v2[1],
            v1[2] * v2[0] - v1[0] * v2[2],
            v1[0] * v2[1] - v1[1] * v2[0],
        ];
        volume += v0[0] * cross[0] + v0[1] * cross[1] + v0[2] * cross[2];
    }

    (volume / 6.0).abs()
}

/// Check if a triangle mesh is watertight (every edge shared by exactly 2 triangles).
///
/// Uses position-based edge matching (quantized to 1e-6 grid) instead of index-based,
/// because the kernel produces per-face tessellation with non-shared vertices.
fn check_watertight(mesh: &RenderMesh) -> bool {
    use std::collections::HashMap;

    if mesh.indices.is_empty() {
        return true;
    }

    // Quantize vertex positions to avoid floating-point mismatches
    let quantize = |idx: u32| -> (i64, i64, i64) {
        let base = idx as usize * 3;
        (
            (mesh.vertices[base] as f64 * 1e6).round() as i64,
            (mesh.vertices[base + 1] as f64 * 1e6).round() as i64,
            (mesh.vertices[base + 2] as f64 * 1e6).round() as i64,
        )
    };

    type GridEdge = ((i64, i64, i64), (i64, i64, i64));
    let mut edge_count: HashMap<GridEdge, u32> = HashMap::new();
    let num_tris = mesh.indices.len() / 3;

    for i in 0..num_tris {
        let tri = [
            mesh.indices[i * 3],
            mesh.indices[i * 3 + 1],
            mesh.indices[i * 3 + 2],
        ];

        for j in 0..3 {
            let a = quantize(tri[j]);
            let b = quantize(tri[(j + 1) % 3]);
            let edge = if a < b { (a, b) } else { (b, a) };
            *edge_count.entry(edge).or_insert(0) += 1;
        }
    }

    edge_count.values().all(|&count| count == 2)
}

/// Compute axis-aligned bounding box from mesh vertices.
fn mesh_bbox(mesh: &RenderMesh) -> Option<([f64; 3], [f64; 3])> {
    if mesh.vertices.is_empty() {
        return None;
    }

    let mut min = [f64::MAX; 3];
    let mut max = [f64::MIN; 3];

    let num_verts = mesh.vertices.len() / 3;
    for i in 0..num_verts {
        for j in 0..3 {
            let v = mesh.vertices[i * 3 + j] as f64;
            if v < min[j] {
                min[j] = v;
            }
            if v > max[j] {
                max[j] = v;
            }
        }
    }

    Some((min, max))
}
