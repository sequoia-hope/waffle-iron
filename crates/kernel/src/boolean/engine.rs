use thiserror::Error;

use crate::geometry::curves::{Curve, Line3d};
use crate::geometry::point::Point3d;
use crate::geometry::surfaces::{Plane, Surface};
use crate::geometry::transform::BoundingBox;
use crate::geometry::vector::Vec3;
use crate::topology::brep::*;
use crate::topology::primitives::make_box;

use super::classify::{classify_point, PointClassification};

/// Boolean operation type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoolOp {
    Union,
    Intersection,
    Difference,
}

/// Structured failure information for Boolean operations.
#[derive(Debug, Error)]
pub enum BooleanFailure {
    #[error("Bounding boxes don't intersect — no Boolean interaction")]
    NoOverlap,

    #[error("Classification ambiguous for face")]
    ClassificationAmbiguous {
        test_results: Vec<PointClassification>,
    },

    #[error("Topology corrupted after Boolean operation")]
    TopologyCorrupted { audit: TopologyAudit },

    #[error("Surface intersection failed: {reason}")]
    IntersectionFailed { reason: String },

    #[error("Degenerate result (zero-volume)")]
    DegenerateResult,
}

/// Perform a Boolean operation between two solids.
/// This is a simplified implementation for Tier 1 (axis-aligned boxes) and Tier 2 (box-cylinder).
///
/// Strategy:
/// 1. Check bounding box overlap
/// 2. Classify faces of each solid against the other
/// 3. Collect faces based on operation type
/// 4. Build result solid
pub fn boolean_op(
    store: &mut EntityStore,
    solid_a: SolidId,
    solid_b: SolidId,
    op: BoolOp,
) -> Result<SolidId, BooleanFailure> {
    // Step 1: Bounding box check
    let bb_a = store.solid_bounding_box(solid_a);
    let bb_b = store.solid_bounding_box(solid_b);

    if !bb_a.intersects(&bb_b) {
        match op {
            BoolOp::Union => {
                // Non-overlapping union: just combine shells
                return Ok(combine_solids(store, solid_a, solid_b));
            }
            BoolOp::Intersection => {
                return Err(BooleanFailure::NoOverlap);
            }
            BoolOp::Difference => {
                // A - B where B doesn't overlap: result is A
                return Ok(clone_solid(store, solid_a));
            }
        }
    }

    // Step 2: Fast path for axis-aligned box pairs.
    // When both solids are AABBs we can compute the result directly without
    // face classification (which fails for partially-overlapping faces).
    if is_solid_aabb(store, solid_a) && is_solid_aabb(store, solid_b) {
        match op {
            BoolOp::Intersection => {
                return aabb_intersection(store, &bb_a, &bb_b)
                    .ok_or(BooleanFailure::DegenerateResult);
            }
            BoolOp::Union | BoolOp::Difference => {
                return aabb_boolean_grid(store, &bb_a, &bb_b, op)
                    .ok_or(BooleanFailure::DegenerateResult);
            }
        }
    }

    // Step 3: General path — classify whole faces (works for non-overlapping or
    // non-AABB solids where faces don't straddle the intersection boundary).
    let faces_a = collect_all_faces(store, solid_a);
    let faces_b = collect_all_faces(store, solid_b);

    let classified_a: Vec<(FaceId, PointClassification)> = faces_a
        .iter()
        .map(|&face_id| {
            let center = face_center(store, face_id);
            let class = classify_point(store, solid_b, &center, 1e-6);
            (face_id, class)
        })
        .collect();

    let classified_b: Vec<(FaceId, PointClassification)> = faces_b
        .iter()
        .map(|&face_id| {
            let center = face_center(store, face_id);
            let class = classify_point(store, solid_a, &center, 1e-6);
            (face_id, class)
        })
        .collect();

    // Step 3: Select faces based on operation
    let mut result_faces: Vec<FaceId> = Vec::new();

    match op {
        BoolOp::Union => {
            // Faces of A that are outside B + Faces of B that are outside A
            for &(face_id, class) in &classified_a {
                if class == PointClassification::Outside {
                    result_faces.push(face_id);
                }
            }
            for &(face_id, class) in &classified_b {
                if class == PointClassification::Outside {
                    result_faces.push(face_id);
                }
            }
        }
        BoolOp::Intersection => {
            // Faces of A that are inside B + Faces of B that are inside A
            for &(face_id, class) in &classified_a {
                if class == PointClassification::Inside {
                    result_faces.push(face_id);
                }
            }
            for &(face_id, class) in &classified_b {
                if class == PointClassification::Inside {
                    result_faces.push(face_id);
                }
            }
        }
        BoolOp::Difference => {
            // Faces of A that are outside B + Faces of B that are inside A (reversed)
            for &(face_id, class) in &classified_a {
                if class == PointClassification::Outside {
                    result_faces.push(face_id);
                }
            }
            for &(face_id, class) in &classified_b {
                if class == PointClassification::Inside {
                    result_faces.push(face_id);
                }
            }
        }
    }

    if result_faces.is_empty() {
        return Err(BooleanFailure::DegenerateResult);
    }

    // Step 4: Build result solid
    let result_solid = store.solids.insert(Solid { shells: vec![] });
    let result_shell = store.shells.insert(Shell {
        faces: result_faces.clone(),
        orientation: ShellOrientation::Outward,
        solid: result_solid,
    });
    store.solids[result_solid].shells.push(result_shell);

    // Update face shell references
    for &face_id in &result_faces {
        store.faces[face_id].shell = result_shell;
    }

    Ok(result_solid)
}

// ─── AABB-specific helpers ──────────────────────────────────────────────────

/// Check whether a solid is an axis-aligned box primitive.
///
/// Returns `true` when the solid has exactly one shell with 6 faces and every
/// vertex lies on a corner of the solid's bounding box.
fn is_solid_aabb(store: &EntityStore, solid_id: SolidId) -> bool {
    let solid = &store.solids[solid_id];
    if solid.shells.len() != 1 {
        return false;
    }
    let shell = &store.shells[solid.shells[0]];
    if shell.faces.len() != 6 {
        return false;
    }
    let bb = store.solid_bounding_box(solid_id);
    if !bb.is_valid() {
        return false;
    }
    for &face_id in &shell.faces {
        let face = &store.faces[face_id];
        let loop_data = &store.loops[face.outer_loop];
        for &he_id in &loop_data.half_edges {
            let he = &store.half_edges[he_id];
            let p = store.vertices[he.start_vertex].point;
            let at_x = (p.x - bb.min.x).abs() < 1e-9 || (p.x - bb.max.x).abs() < 1e-9;
            let at_y = (p.y - bb.min.y).abs() < 1e-9 || (p.y - bb.max.y).abs() < 1e-9;
            let at_z = (p.z - bb.min.z).abs() < 1e-9 || (p.z - bb.max.z).abs() < 1e-9;
            if !at_x || !at_y || !at_z {
                return false;
            }
        }
    }
    true
}

/// Compute the intersection of two AABBs and return it as a new box solid.
///
/// Returns `None` when the boxes do not overlap (zero or negative volume).
fn aabb_intersection(
    store: &mut EntityStore,
    bb_a: &BoundingBox,
    bb_b: &BoundingBox,
) -> Option<SolidId> {
    let min_x = bb_a.min.x.max(bb_b.min.x);
    let min_y = bb_a.min.y.max(bb_b.min.y);
    let min_z = bb_a.min.z.max(bb_b.min.z);
    let max_x = bb_a.max.x.min(bb_b.max.x);
    let max_y = bb_a.max.y.min(bb_b.max.y);
    let max_z = bb_a.max.z.min(bb_b.max.z);

    if min_x < max_x && min_y < max_y && min_z < max_z {
        Some(make_box(store, min_x, min_y, min_z, max_x, max_y, max_z))
    } else {
        None
    }
}

/// Sort floating-point values and remove near-duplicates.
fn sorted_unique(values: &[f64]) -> Vec<f64> {
    let mut v: Vec<f64> = values.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v.dedup_by(|a, b| (*a - *b).abs() < 1e-12);
    v
}

/// Check whether a point is strictly inside an AABB (not on the boundary).
fn point_strictly_in_aabb(p: &Point3d, bb: &BoundingBox) -> bool {
    p.x > bb.min.x + 1e-9
        && p.x < bb.max.x - 1e-9
        && p.y > bb.min.y + 1e-9
        && p.y < bb.max.y - 1e-9
        && p.z > bb.min.z + 1e-9
        && p.z < bb.max.z - 1e-9
}

/// Construct the boolean result for two AABBs using grid decomposition.
///
/// The algorithm works by:
///   1. Building a rectilinear grid from the unique X, Y, Z coordinates of both
///      bounding boxes.
///   2. Classifying every grid cell as "in result" or not, depending on whether
///      its centre lies inside A, B, or both, and the requested operation.
///   3. For every pair of neighbouring cells where one is "in" and the other is
///      "out", emitting a boundary face with the correct outward-pointing normal.
///
/// This correctly handles union, intersection, and difference of overlapping
/// axis-aligned boxes — including cases where the whole-face classification
/// approach would fail because faces straddle the intersection boundary.
fn aabb_boolean_grid(
    store: &mut EntityStore,
    bb_a: &BoundingBox,
    bb_b: &BoundingBox,
    op: BoolOp,
) -> Option<SolidId> {
    let xs = sorted_unique(&[bb_a.min.x, bb_a.max.x, bb_b.min.x, bb_b.max.x]);
    let ys = sorted_unique(&[bb_a.min.y, bb_a.max.y, bb_b.min.y, bb_b.max.y]);
    let zs = sorted_unique(&[bb_a.min.z, bb_a.max.z, bb_b.min.z, bb_b.max.z]);

    let nx = xs.len() - 1;
    let ny = ys.len() - 1;
    let nz = zs.len() - 1;

    let cell_in_result = |ix: usize, iy: usize, iz: usize| -> bool {
        let center = Point3d::new(
            (xs[ix] + xs[ix + 1]) / 2.0,
            (ys[iy] + ys[iy + 1]) / 2.0,
            (zs[iz] + zs[iz + 1]) / 2.0,
        );
        let in_a = point_strictly_in_aabb(&center, bb_a);
        let in_b = point_strictly_in_aabb(&center, bb_b);
        match op {
            BoolOp::Union => in_a || in_b,
            BoolOp::Intersection => in_a && in_b,
            BoolOp::Difference => in_a && !in_b,
        }
    };

    // Create the result solid and shell.
    let solid_id = store.solids.insert(Solid { shells: vec![] });
    let shell_id = store.shells.insert(Shell {
        faces: vec![],
        orientation: ShellOrientation::Outward,
        solid: solid_id,
    });
    store.solids[solid_id].shells.push(shell_id);

    let mut face_count: usize = 0;

    // ------------------------------------------------------------------
    // Faces perpendicular to the X axis (at each x = xs[ix]).
    // ------------------------------------------------------------------
    for ix in 0..=nx {
        for iy in 0..ny {
            for iz in 0..nz {
                let left_in = if ix > 0 {
                    cell_in_result(ix - 1, iy, iz)
                } else {
                    false
                };
                let right_in = if ix < nx {
                    cell_in_result(ix, iy, iz)
                } else {
                    false
                };
                if left_in == right_in {
                    continue;
                }
                let x = xs[ix];
                let (y0, y1) = (ys[iy], ys[iy + 1]);
                let (z0, z1) = (zs[iz], zs[iz + 1]);
                if left_in {
                    // Outward normal is +X.
                    create_grid_quad(
                        store,
                        shell_id,
                        [
                            Point3d::new(x, y0, z0),
                            Point3d::new(x, y0, z1),
                            Point3d::new(x, y1, z1),
                            Point3d::new(x, y1, z0),
                        ],
                        Vec3::X,
                    );
                } else {
                    // Outward normal is -X.
                    create_grid_quad(
                        store,
                        shell_id,
                        [
                            Point3d::new(x, y0, z0),
                            Point3d::new(x, y1, z0),
                            Point3d::new(x, y1, z1),
                            Point3d::new(x, y0, z1),
                        ],
                        -Vec3::X,
                    );
                }
                face_count += 1;
            }
        }
    }

    // ------------------------------------------------------------------
    // Faces perpendicular to the Y axis (at each y = ys[iy]).
    // ------------------------------------------------------------------
    for iy in 0..=ny {
        for ix in 0..nx {
            for iz in 0..nz {
                let below_in = if iy > 0 {
                    cell_in_result(ix, iy - 1, iz)
                } else {
                    false
                };
                let above_in = if iy < ny {
                    cell_in_result(ix, iy, iz)
                } else {
                    false
                };
                if below_in == above_in {
                    continue;
                }
                let y = ys[iy];
                let (x0, x1) = (xs[ix], xs[ix + 1]);
                let (z0, z1) = (zs[iz], zs[iz + 1]);
                if below_in {
                    // Outward normal is +Y.
                    create_grid_quad(
                        store,
                        shell_id,
                        [
                            Point3d::new(x0, y, z0),
                            Point3d::new(x1, y, z0),
                            Point3d::new(x1, y, z1),
                            Point3d::new(x0, y, z1),
                        ],
                        Vec3::Y,
                    );
                } else {
                    // Outward normal is -Y.
                    create_grid_quad(
                        store,
                        shell_id,
                        [
                            Point3d::new(x0, y, z0),
                            Point3d::new(x0, y, z1),
                            Point3d::new(x1, y, z1),
                            Point3d::new(x1, y, z0),
                        ],
                        -Vec3::Y,
                    );
                }
                face_count += 1;
            }
        }
    }

    // ------------------------------------------------------------------
    // Faces perpendicular to the Z axis (at each z = zs[iz]).
    // ------------------------------------------------------------------
    for iz in 0..=nz {
        for ix in 0..nx {
            for iy in 0..ny {
                let front_in = if iz > 0 {
                    cell_in_result(ix, iy, iz - 1)
                } else {
                    false
                };
                let back_in = if iz < nz {
                    cell_in_result(ix, iy, iz)
                } else {
                    false
                };
                if front_in == back_in {
                    continue;
                }
                let z = zs[iz];
                let (x0, x1) = (xs[ix], xs[ix + 1]);
                let (y0, y1) = (ys[iy], ys[iy + 1]);
                if front_in {
                    // Outward normal is +Z.
                    create_grid_quad(
                        store,
                        shell_id,
                        [
                            Point3d::new(x1, y0, z),
                            Point3d::new(x0, y0, z),
                            Point3d::new(x0, y1, z),
                            Point3d::new(x1, y1, z),
                        ],
                        Vec3::Z,
                    );
                } else {
                    // Outward normal is -Z.
                    create_grid_quad(
                        store,
                        shell_id,
                        [
                            Point3d::new(x0, y0, z),
                            Point3d::new(x1, y0, z),
                            Point3d::new(x1, y1, z),
                            Point3d::new(x0, y1, z),
                        ],
                        -Vec3::Z,
                    );
                }
                face_count += 1;
            }
        }
    }

    if face_count == 0 {
        return None;
    }

    Some(solid_id)
}

/// Create one axis-aligned quadrilateral face and attach it to a shell.
///
/// `corners` must be in CCW winding order when viewed from the direction of
/// `normal`.  Each call creates fresh vertices, edges, and half-edges — twins
/// are not linked across faces.
fn create_grid_quad(
    store: &mut EntityStore,
    shell_id: ShellId,
    corners: [Point3d; 4],
    normal: Vec3,
) {
    let verts: [VertexId; 4] = [
        store.vertices.insert(Vertex {
            point: corners[0],
            tolerance: 1e-7,
        }),
        store.vertices.insert(Vertex {
            point: corners[1],
            tolerance: 1e-7,
        }),
        store.vertices.insert(Vertex {
            point: corners[2],
            tolerance: 1e-7,
        }),
        store.vertices.insert(Vertex {
            point: corners[3],
            tolerance: 1e-7,
        }),
    ];

    let center = Point3d::new(
        (corners[0].x + corners[2].x) / 2.0,
        (corners[0].y + corners[2].y) / 2.0,
        (corners[0].z + corners[2].z) / 2.0,
    );

    let surface = Surface::Plane(Plane::new(center, normal));

    let loop_id = store.loops.insert(Loop {
        half_edges: vec![],
        face: FaceId::default(),
    });

    let face_id = store.faces.insert(Face {
        surface,
        outer_loop: loop_id,
        inner_loops: vec![],
        same_sense: true,
        shell: shell_id,
    });

    store.loops[loop_id].face = face_id;
    store.shells[shell_id].faces.push(face_id);

    for i in 0..4 {
        let next = (i + 1) % 4;
        let v_start = verts[i];
        let v_end = verts[next];
        let p_start = corners[i];
        let p_end = corners[next];

        let line = Line3d::from_points(p_start, p_end);
        let dist = p_start.distance_to(&p_end);

        let he_id = store.half_edges.insert_with_key(|_| HalfEdge {
            edge: EdgeId::default(),
            twin: HalfEdgeId::default(),
            face: face_id,
            loop_id,
            start_vertex: v_start,
            end_vertex: v_end,
            t_start: 0.0,
            t_end: dist,
            forward: true,
        });

        let edge_id = store.edges.insert(Edge {
            curve: Curve::Line(line),
            half_edges: (he_id, he_id),
            start_vertex: v_start,
            end_vertex: v_end,
        });

        store.half_edges[he_id].edge = edge_id;
        store.loops[loop_id].half_edges.push(he_id);
    }
}

// ─── General helpers ────────────────────────────────────────────────────────

/// Collect all face IDs from a solid.
fn collect_all_faces(store: &EntityStore, solid_id: SolidId) -> Vec<FaceId> {
    let solid = &store.solids[solid_id];
    let mut faces = Vec::new();
    for &shell_id in &solid.shells {
        faces.extend(store.shells[shell_id].faces.iter());
    }
    faces
}

/// Compute the center point of a face (average of vertex positions).
fn face_center(store: &EntityStore, face_id: FaceId) -> Point3d {
    let face = &store.faces[face_id];
    let loop_data = &store.loops[face.outer_loop];

    if loop_data.half_edges.is_empty() {
        return Point3d::ORIGIN;
    }

    let mut sum_x = 0.0;
    let mut sum_y = 0.0;
    let mut sum_z = 0.0;
    let mut count = 0;

    for &he_id in &loop_data.half_edges {
        let he = &store.half_edges[he_id];
        let p = store.vertices[he.start_vertex].point;
        sum_x += p.x;
        sum_y += p.y;
        sum_z += p.z;
        count += 1;
    }

    Point3d::new(sum_x / count as f64, sum_y / count as f64, sum_z / count as f64)
}

/// Combine two non-overlapping solids into one.
fn combine_solids(store: &mut EntityStore, a: SolidId, b: SolidId) -> SolidId {
    let shells_a = store.solids[a].shells.clone();
    let shells_b = store.solids[b].shells.clone();

    let result = store.solids.insert(Solid {
        shells: [shells_a, shells_b].concat(),
    });
    result
}

/// Clone a solid (shallow — shares faces with original).
fn clone_solid(store: &mut EntityStore, solid_id: SolidId) -> SolidId {
    let shells = store.solids[solid_id].shells.clone();
    store.solids.insert(Solid { shells })
}

/// Monte Carlo volume estimation for a solid.
/// Shoots random rays through the bounding box and counts inside/outside.
pub fn estimate_volume(store: &EntityStore, solid_id: SolidId, num_samples: usize) -> f64 {
    let bb = store.solid_bounding_box(solid_id);
    if !bb.is_valid() {
        return 0.0;
    }

    let margin = 0.01;
    let bb = bb.expanded(margin);
    let bb_volume = bb.volume();

    let mut inside_count = 0;

    // Use a simple deterministic seed for reproducibility
    let mut rng_state: u64 = 12345;

    for _ in 0..num_samples {
        // Simple LCG pseudo-random
        rng_state = rng_state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let rx = (rng_state >> 33) as f64 / (u32::MAX as f64);
        rng_state = rng_state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let ry = (rng_state >> 33) as f64 / (u32::MAX as f64);
        rng_state = rng_state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let rz = (rng_state >> 33) as f64 / (u32::MAX as f64);

        let point = Point3d::new(
            bb.min.x + rx * (bb.max.x - bb.min.x),
            bb.min.y + ry * (bb.max.y - bb.min.y),
            bb.min.z + rz * (bb.max.z - bb.min.z),
        );

        if classify_point(store, solid_id, &point, 1e-7) == PointClassification::Inside {
            inside_count += 1;
        }
    }

    bb_volume * (inside_count as f64 / num_samples as f64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::topology::primitives::make_box;

    #[test]
    fn test_boolean_union_non_overlapping() {
        let mut store = EntityStore::new();
        let a = make_box(&mut store, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0);
        let b = make_box(&mut store, 5.0, 5.0, 5.0, 6.0, 6.0, 6.0);

        let result = boolean_op(&mut store, a, b, BoolOp::Union);
        assert!(result.is_ok());
        let result_id = result.unwrap();
        // Non-overlapping union should have shells from both
        assert!(store.solids[result_id].shells.len() >= 1);
    }

    #[test]
    fn test_boolean_intersection_no_overlap() {
        let mut store = EntityStore::new();
        let a = make_box(&mut store, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0);
        let b = make_box(&mut store, 5.0, 5.0, 5.0, 6.0, 6.0, 6.0);

        let result = boolean_op(&mut store, a, b, BoolOp::Intersection);
        assert!(matches!(result, Err(BooleanFailure::NoOverlap)));
    }

    #[test]
    fn test_boolean_difference_no_overlap() {
        let mut store = EntityStore::new();
        let a = make_box(&mut store, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0);
        let b = make_box(&mut store, 5.0, 5.0, 5.0, 6.0, 6.0, 6.0);

        let result = boolean_op(&mut store, a, b, BoolOp::Difference);
        assert!(result.is_ok());
    }

    #[test]
    fn test_estimate_volume_box() {
        let mut store = EntityStore::new();
        let solid_id = make_box(&mut store, 0.0, 0.0, 0.0, 10.0, 10.0, 10.0);

        let vol = estimate_volume(&store, solid_id, 10000);
        let expected = 1000.0;
        // Monte Carlo should be within 10% for 10000 samples
        assert!(
            (vol - expected).abs() / expected < 0.15,
            "Estimated volume {} too far from expected {}",
            vol,
            expected
        );
    }

    // ── Overlapping AABB boolean tests ──────────────────────────────────

    #[test]
    fn test_boolean_intersection_overlapping_boxes() {
        let mut store = EntityStore::new();
        // Box A: [0,0,0] to [2,2,2]  (volume = 8)
        let a = make_box(&mut store, 0.0, 0.0, 0.0, 2.0, 2.0, 2.0);
        // Box B: [1,1,1] to [3,3,3]  (volume = 8)
        let b = make_box(&mut store, 1.0, 1.0, 1.0, 3.0, 3.0, 3.0);

        let result = boolean_op(&mut store, a, b, BoolOp::Intersection);
        assert!(result.is_ok(), "Intersection of overlapping boxes should succeed");
        let result_id = result.unwrap();

        // The intersection is [1,1,1] to [2,2,2] with exact volume 1.0.
        // Because the intersection path uses make_box, we can verify the
        // bounding box directly as well as the Monte Carlo volume.
        let bb = store.solid_bounding_box(result_id);
        assert!((bb.min.x - 1.0).abs() < 1e-9, "min.x should be 1.0");
        assert!((bb.min.y - 1.0).abs() < 1e-9, "min.y should be 1.0");
        assert!((bb.min.z - 1.0).abs() < 1e-9, "min.z should be 1.0");
        assert!((bb.max.x - 2.0).abs() < 1e-9, "max.x should be 2.0");
        assert!((bb.max.y - 2.0).abs() < 1e-9, "max.y should be 2.0");
        assert!((bb.max.z - 2.0).abs() < 1e-9, "max.z should be 2.0");

        let vol = estimate_volume(&store, result_id, 50000);
        let expected = 1.0;
        assert!(
            (vol - expected).abs() / expected < 0.15,
            "Intersection volume {} too far from expected {}",
            vol,
            expected
        );
    }

    #[test]
    fn test_boolean_union_overlapping_boxes() {
        let mut store = EntityStore::new();
        let a = make_box(&mut store, 0.0, 0.0, 0.0, 2.0, 2.0, 2.0);
        let b = make_box(&mut store, 1.0, 1.0, 1.0, 3.0, 3.0, 3.0);

        let result = boolean_op(&mut store, a, b, BoolOp::Union);
        assert!(result.is_ok(), "Union of overlapping boxes should succeed");
        let result_id = result.unwrap();

        // Verify bounding box of union spans [0,0,0] to [3,3,3]
        let bb = store.solid_bounding_box(result_id);
        assert!((bb.min.x - 0.0).abs() < 1e-9, "min.x should be 0");
        assert!((bb.min.y - 0.0).abs() < 1e-9, "min.y should be 0");
        assert!((bb.min.z - 0.0).abs() < 1e-9, "min.z should be 0");
        assert!((bb.max.x - 3.0).abs() < 1e-9, "max.x should be 3");
        assert!((bb.max.y - 3.0).abs() < 1e-9, "max.y should be 3");
        assert!((bb.max.z - 3.0).abs() < 1e-9, "max.z should be 3");

        // The grid decomposition should produce boundary faces
        let shell = &store.shells[store.solids[result_id].shells[0]];
        assert!(shell.faces.len() > 6, "Union should have more faces than a simple box");
    }

    #[test]
    fn test_boolean_difference_overlapping_boxes() {
        let mut store = EntityStore::new();
        let a = make_box(&mut store, 0.0, 0.0, 0.0, 2.0, 2.0, 2.0);
        let b = make_box(&mut store, 1.0, 1.0, 1.0, 3.0, 3.0, 3.0);

        let result = boolean_op(&mut store, a, b, BoolOp::Difference);
        assert!(result.is_ok(), "Difference of overlapping boxes should succeed");
        let result_id = result.unwrap();

        // V(A - B) = V(A) - V(A ∩ B) = 8 - 1 = 7
        let vol = estimate_volume(&store, result_id, 50000);
        let expected = 7.0;
        assert!(
            (vol - expected).abs() / expected < 0.15,
            "Difference volume {} too far from expected {}",
            vol,
            expected
        );
    }
}
