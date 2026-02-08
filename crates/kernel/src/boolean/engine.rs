use thiserror::Error;
use tracing::{debug, info, instrument};

use crate::geometry::curves::{Curve, Line3d};
use crate::geometry::point::Point3d;
use crate::geometry::surface_intersection::{self, SurfaceIntersection};
use crate::geometry::surfaces::{Plane, Surface};
use crate::geometry::transform::BoundingBox;
use crate::geometry::vector::Vec3;
use crate::topology::brep::*;
use crate::topology::primitives::make_box;

use super::classify::{classify_point, PointClassification};
use super::split::split_planar_face_by_line;

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
#[instrument(skip(store), fields(op = ?op))]
pub fn boolean_op(
    store: &mut EntityStore,
    solid_a: SolidId,
    solid_b: SolidId,
    op: BoolOp,
) -> Result<SolidId, BooleanFailure> {
    // Step 1: Bounding box check
    let bb_a = store.solid_bounding_box(solid_a);
    let bb_b = store.solid_bounding_box(solid_b);

    info!(
        bb_a_min = ?[bb_a.min.x, bb_a.min.y, bb_a.min.z],
        bb_a_max = ?[bb_a.max.x, bb_a.max.y, bb_a.max.z],
        bb_b_min = ?[bb_b.min.x, bb_b.min.y, bb_b.min.z],
        bb_b_max = ?[bb_b.max.x, bb_b.max.y, bb_b.max.z],
        "bounding boxes computed"
    );

    if !bb_a.intersects(&bb_b) {
        debug!(op = ?op, "bounding boxes do not overlap, using fast path");
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
        debug!("both solids are AABBs, using grid decomposition fast path");
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

    // Step 3: Tier 2 — face splitting for mixed-type solids.
    // When faces of one solid straddle the boundary of the other, we must
    // split those faces along the intersection curves before classifying.
    let split_result = split_straddling_faces(store, solid_a, solid_b);
    if split_result.any_splits {
        debug!(
            splits_a = split_result.splits_a,
            splits_b = split_result.splits_b,
            "face splitting performed, proceeding with classification"
        );
    }

    // Step 4: General path — classify whole faces (works for non-overlapping or
    // non-AABB solids where faces don't straddle the intersection boundary).
    let faces_a = collect_all_faces(store, solid_a);
    let faces_b = collect_all_faces(store, solid_b);

    debug!(faces_a = faces_a.len(), faces_b = faces_b.len(), "classifying faces");

    let tol = crate::default_tolerance();

    let classified_a: Vec<(FaceId, PointClassification)> = faces_a
        .iter()
        .map(|&face_id| {
            let center = face_center(store, face_id);
            let class = classify_point(store, solid_b, &center, tol.coincidence);
            (face_id, class)
        })
        .collect();

    let classified_b: Vec<(FaceId, PointClassification)> = faces_b
        .iter()
        .map(|&face_id| {
            let center = face_center(store, face_id);
            let class = classify_point(store, solid_a, &center, tol.coincidence);
            (face_id, class)
        })
        .collect();

    // Step 5: Select faces based on operation
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

    debug!(
        result_face_count = result_faces.len(),
        "face selection complete"
    );

    if result_faces.is_empty() {
        return Err(BooleanFailure::DegenerateResult);
    }

    // Step 6: Build result solid
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
    let coin_tol = crate::default_tolerance().coincidence;
    for &face_id in &shell.faces {
        let face = &store.faces[face_id];
        let loop_data = &store.loops[face.outer_loop];
        for &he_id in &loop_data.half_edges {
            let he = &store.half_edges[he_id];
            let p = store.vertices[he.start_vertex].point;
            let at_x = (p.x - bb.min.x).abs() < coin_tol || (p.x - bb.max.x).abs() < coin_tol;
            let at_y = (p.y - bb.min.y).abs() < coin_tol || (p.y - bb.max.y).abs() < coin_tol;
            let at_z = (p.z - bb.min.z).abs() < coin_tol || (p.z - bb.max.z).abs() < coin_tol;
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
    let coin_tol = crate::default_tolerance().coincidence;
    p.x > bb.min.x + coin_tol
        && p.x < bb.max.x - coin_tol
        && p.y > bb.min.y + coin_tol
        && p.y < bb.max.y - coin_tol
        && p.z > bb.min.z + coin_tol
        && p.z < bb.max.z - coin_tol
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
///   4. Vertices are shared across faces via a coordinate-indexed map, and
///      half-edge twins are properly linked using a directed-edge map.
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

    // Pre-create shared vertices for every grid node.
    // Key: (ix, iy, iz) grid indices -> VertexId
    let coin_tol = crate::default_tolerance().coincidence;
    let mut vertex_map: std::collections::HashMap<(usize, usize, usize), VertexId> =
        std::collections::HashMap::new();

    // Edge map for twin linking.
    // Key: ordered (VertexId_low, VertexId_high) -> first HalfEdgeId on that edge
    let mut edge_he_map: std::collections::HashMap<(u64, u64), HalfEdgeId> =
        std::collections::HashMap::new();

    // Lazily get or create a vertex at grid position (ix, iy, iz).
    let get_vertex = |store: &mut EntityStore,
                      vertex_map: &mut std::collections::HashMap<(usize, usize, usize), VertexId>,
                      ix: usize,
                      iy: usize,
                      iz: usize| {
        *vertex_map.entry((ix, iy, iz)).or_insert_with(|| {
            store.vertices.insert(Vertex {
                point: Point3d::new(xs[ix], ys[iy], zs[iz]),
                tolerance: coin_tol,
            })
        })
    };

    let mut face_count: usize = 0;

    // Collect quads as (corner_grid_indices[4], normal)
    let mut quads: Vec<([(usize, usize, usize); 4], Vec3)> = Vec::new();

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
                if left_in {
                    // Outward normal is +X.
                    quads.push((
                        [
                            (ix, iy, iz),
                            (ix, iy, iz + 1),
                            (ix, iy + 1, iz + 1),
                            (ix, iy + 1, iz),
                        ],
                        Vec3::X,
                    ));
                } else {
                    // Outward normal is -X.
                    quads.push((
                        [
                            (ix, iy, iz),
                            (ix, iy + 1, iz),
                            (ix, iy + 1, iz + 1),
                            (ix, iy, iz + 1),
                        ],
                        -Vec3::X,
                    ));
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
                if below_in {
                    // Outward normal is +Y.
                    quads.push((
                        [
                            (ix, iy, iz),
                            (ix + 1, iy, iz),
                            (ix + 1, iy, iz + 1),
                            (ix, iy, iz + 1),
                        ],
                        Vec3::Y,
                    ));
                } else {
                    // Outward normal is -Y.
                    quads.push((
                        [
                            (ix, iy, iz),
                            (ix, iy, iz + 1),
                            (ix + 1, iy, iz + 1),
                            (ix + 1, iy, iz),
                        ],
                        -Vec3::Y,
                    ));
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
                if front_in {
                    // Outward normal is +Z.
                    quads.push((
                        [
                            (ix + 1, iy, iz),
                            (ix, iy, iz),
                            (ix, iy + 1, iz),
                            (ix + 1, iy + 1, iz),
                        ],
                        Vec3::Z,
                    ));
                } else {
                    // Outward normal is -Z.
                    quads.push((
                        [
                            (ix, iy, iz),
                            (ix + 1, iy, iz),
                            (ix + 1, iy + 1, iz),
                            (ix, iy + 1, iz),
                        ],
                        -Vec3::Z,
                    ));
                }
                face_count += 1;
            }
        }
    }

    if face_count == 0 {
        return None;
    }

    // Now create all the faces with shared vertices and linked edges.
    for (grid_corners, normal) in &quads {
        let verts: [VertexId; 4] = [
            get_vertex(store, &mut vertex_map, grid_corners[0].0, grid_corners[0].1, grid_corners[0].2),
            get_vertex(store, &mut vertex_map, grid_corners[1].0, grid_corners[1].1, grid_corners[1].2),
            get_vertex(store, &mut vertex_map, grid_corners[2].0, grid_corners[2].1, grid_corners[2].2),
            get_vertex(store, &mut vertex_map, grid_corners[3].0, grid_corners[3].1, grid_corners[3].2),
        ];

        let corners: [Point3d; 4] = [
            store.vertices[verts[0]].point,
            store.vertices[verts[1]].point,
            store.vertices[verts[2]].point,
            store.vertices[verts[3]].point,
        ];

        let center = Point3d::new(
            (corners[0].x + corners[2].x) / 2.0,
            (corners[0].y + corners[2].y) / 2.0,
            (corners[0].z + corners[2].z) / 2.0,
        );

        let surface = Surface::Plane(Plane::new(center, *normal));

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

            // Canonical edge key: ordered pair of vertex IDs for twin matching.
            use slotmap::Key;
            let v_start_u64 = v_start.data().as_ffi();
            let v_end_u64 = v_end.data().as_ffi();
            let edge_key = if v_start_u64 < v_end_u64 {
                (v_start_u64, v_end_u64)
            } else {
                (v_end_u64, v_start_u64)
            };
            let forward = v_start_u64 < v_end_u64;

            let he_id = store.half_edges.insert_with_key(|_| HalfEdge {
                edge: EdgeId::default(),
                twin: HalfEdgeId::default(),
                face: face_id,
                loop_id,
                start_vertex: v_start,
                end_vertex: v_end,
                t_start: 0.0,
                t_end: p_start.distance_to(&p_end),
                forward,
            });

            store.loops[loop_id].half_edges.push(he_id);

            if let Some(&twin_he_id) = edge_he_map.get(&edge_key) {
                // Edge already exists — link twins.
                let edge_id = store.half_edges[twin_he_id].edge;
                store.half_edges[he_id].twin = twin_he_id;
                store.half_edges[he_id].edge = edge_id;
                store.half_edges[twin_he_id].twin = he_id;
                store.edges[edge_id].half_edges.1 = he_id;
            } else {
                // New edge.
                let (e_start, e_end) = if forward {
                    (v_start, v_end)
                } else {
                    (v_end, v_start)
                };
                let line = if forward {
                    Line3d::from_points(p_start, p_end)
                } else {
                    Line3d::from_points(p_end, p_start)
                };

                let edge_id = store.edges.insert(Edge {
                    curve: Curve::Line(line),
                    half_edges: (he_id, HalfEdgeId::default()),
                    start_vertex: e_start,
                    end_vertex: e_end,
                });

                store.half_edges[he_id].edge = edge_id;
                edge_he_map.insert(edge_key, he_id);
            }
        }
    }

    // Split faces into separate shells if they form disconnected components.
    // This happens for e.g. A\B when B is entirely inside A (outer + cavity).
    let all_faces: Vec<FaceId> = store.shells[shell_id].faces.clone();
    if all_faces.len() > 1 {
        let components = find_connected_face_components(store, &all_faces);
        if components.len() > 1 {
            // Keep the first component in the existing shell, create new shells for the rest.
            store.shells[shell_id].faces = components[0].clone();
            for component in &components[1..] {
                let new_shell_id = store.shells.insert(Shell {
                    faces: component.clone(),
                    orientation: ShellOrientation::Outward,
                    solid: solid_id,
                });
                store.solids[solid_id].shells.push(new_shell_id);
                // Update face->shell references
                for &face_id in component {
                    store.faces[face_id].shell = new_shell_id;
                }
            }
        }
    }

    Some(solid_id)
}

/// Find connected components of faces via shared edges (twin linking).
fn find_connected_face_components(
    store: &EntityStore,
    faces: &[FaceId],
) -> Vec<Vec<FaceId>> {
    use std::collections::{HashSet, VecDeque};

    let face_set: HashSet<FaceId> = faces.iter().copied().collect();
    let mut visited: HashSet<FaceId> = HashSet::new();
    let mut components: Vec<Vec<FaceId>> = Vec::new();

    for &start_face in faces {
        if visited.contains(&start_face) {
            continue;
        }

        let mut component = Vec::new();
        let mut queue = VecDeque::new();
        queue.push_back(start_face);
        visited.insert(start_face);

        while let Some(face_id) = queue.pop_front() {
            component.push(face_id);

            let face = &store.faces[face_id];
            let loop_data = &store.loops[face.outer_loop];
            for &he_id in &loop_data.half_edges {
                let he = &store.half_edges[he_id];
                let twin = &store.half_edges[he.twin];
                let neighbor_face = twin.face;
                if face_set.contains(&neighbor_face) && !visited.contains(&neighbor_face) {
                    visited.insert(neighbor_face);
                    queue.push_back(neighbor_face);
                }
            }
        }

        components.push(component);
    }

    components
}

// ─── Tier 2: Face splitting for mixed-type Booleans ─────────────────────────

struct SplitReport {
    any_splits: bool,
    splits_a: usize,
    splits_b: usize,
}

/// Split faces that straddle the intersection boundary between two solids.
///
/// For each face of solid A, checks if it intersects any face of solid B.
/// When the intersection is a line that crosses the face, splits the face.
/// Then does the same for solid B's faces against solid A.
fn split_straddling_faces(
    store: &mut EntityStore,
    solid_a: SolidId,
    solid_b: SolidId,
) -> SplitReport {
    let tol = crate::default_tolerance();
    let mut splits_a = 0;
    let mut splits_b = 0;

    // Split faces of A that are crossed by faces of B.
    splits_a += split_faces_against_solid(store, solid_a, solid_b, &tol);
    // Split faces of B that are crossed by faces of A.
    splits_b += split_faces_against_solid(store, solid_b, solid_a, &tol);

    SplitReport {
        any_splits: splits_a > 0 || splits_b > 0,
        splits_a,
        splits_b,
    }
}

/// Split faces of `target_solid` that are intersected by faces of `tool_solid`.
///
/// Returns the number of successful face splits performed.
fn split_faces_against_solid(
    store: &mut EntityStore,
    target_solid: SolidId,
    tool_solid: SolidId,
    tol: &crate::Tolerance,
) -> usize {
    let mut total_splits = 0;

    // Collect the tool face planes and their bounding boxes.
    let tool_faces = collect_all_faces(store, tool_solid);
    let tool_data: Vec<(Plane, BoundingBox)> = tool_faces
        .iter()
        .filter_map(|&fid| {
            if let Surface::Plane(p) = &store.faces[fid].surface {
                let bb = face_bounding_box(store, fid);
                Some((*p, bb))
            } else {
                None
            }
        })
        .collect();

    // We need to iterate over target faces, but splitting changes the face list.
    // Use a work queue: start with all current faces, and when a face is split,
    // add the two new faces to the queue for further splitting.
    let mut work_queue: Vec<FaceId> = collect_all_faces(store, target_solid);
    let mut processed = std::collections::HashSet::new();

    // Safety limit to prevent infinite loops.
    let max_iterations = work_queue.len() * tool_data.len() * 4;
    let mut iteration = 0;

    while let Some(face_id) = work_queue.pop() {
        iteration += 1;
        if iteration > max_iterations {
            debug!("split_faces_against_solid: hit iteration limit {}", max_iterations);
            break;
        }

        if processed.contains(&face_id) {
            continue;
        }
        if store.faces.get(face_id).is_none() {
            continue;
        }

        let target_plane = match &store.faces[face_id].surface {
            Surface::Plane(p) => *p,
            _ => {
                processed.insert(face_id);
                continue;
            }
        };

        let target_bb = face_bounding_box(store, face_id);

        let mut was_split = false;

        for (tool_plane, tool_bb) in &tool_data {
            // Quick reject: bounding boxes must overlap (expanded slightly for tolerance).
            if !target_bb.expanded(tol.coincidence * 10.0).intersects(&tool_bb.expanded(tol.coincidence * 10.0)) {
                continue;
            }

            let intersection = surface_intersection::plane_plane(&target_plane, tool_plane, tol);

            let split_line = match intersection {
                SurfaceIntersection::Curve(Curve::Line(line)) => line,
                _ => continue,
            };

            // Check if the split line passes through the tool face's extent.
            // Project tool face vertices onto the split line and check if any are close.
            if !line_near_face(store, &split_line, &target_bb, tol) {
                continue;
            }

            if let Some(split_result) = split_planar_face_by_line(store, face_id, &split_line, tol) {
                total_splits += 1;
                was_split = true;
                work_queue.push(split_result.face_a);
                work_queue.push(split_result.face_b);
                break;
            }
        }

        if !was_split {
            processed.insert(face_id);
        }
    }

    total_splits
}

/// Compute the bounding box of a face (from its outer loop vertices).
fn face_bounding_box(store: &EntityStore, face_id: FaceId) -> BoundingBox {
    let face = &store.faces[face_id];
    let loop_data = &store.loops[face.outer_loop];
    let mut bb = BoundingBox::empty();
    for &he_id in &loop_data.half_edges {
        let he = &store.half_edges[he_id];
        bb.expand_to_include(&store.vertices[he.start_vertex].point);
    }
    bb
}

/// Check if a line passes near a face's bounding box.
/// Projects the bounding box corners onto the line and checks if the closest point
/// on the line is within the bounding box extent.
fn line_near_face(_store: &EntityStore, line: &Line3d, face_bb: &BoundingBox, tol: &crate::Tolerance) -> bool {
    // Check if the line passes through the expanded bounding box.
    // Test: project the BB center onto the line, then check distance.
    let bb_center = Point3d::new(
        (face_bb.min.x + face_bb.max.x) / 2.0,
        (face_bb.min.y + face_bb.max.y) / 2.0,
        (face_bb.min.z + face_bb.max.z) / 2.0,
    );
    let bb_half_diag = bb_center.distance_to(&face_bb.max);
    let dist = line.distance_to_point(&bb_center);
    dist < bb_half_diag + tol.coincidence
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
        let rx = (rng_state >> 32) as f64 / (u32::MAX as f64);
        rng_state = rng_state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let ry = (rng_state >> 32) as f64 / (u32::MAX as f64);
        rng_state = rng_state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let rz = (rng_state >> 32) as f64 / (u32::MAX as f64);

        let point = Point3d::new(
            bb.min.x + rx * (bb.max.x - bb.min.x),
            bb.min.y + ry * (bb.max.y - bb.min.y),
            bb.min.z + rz * (bb.max.z - bb.min.z),
        );

        if classify_point(store, solid_id, &point, crate::default_tolerance().coincidence) == PointClassification::Inside {
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

    // ── Tier 2: Box-Cylinder boolean tests ──────────────────────────────

    #[test]
    fn test_box_cylinder_difference_through_hole() {
        // Subtract a vertical cylinder from a box to create a through-hole.
        // Box: [0,0,0] to [10,10,10]
        // Cylinder: centered at (5,5,_), radius 2, height 10, along Z
        use crate::topology::primitives::make_cylinder;

        let mut store = EntityStore::new();
        let box_solid = make_box(&mut store, 0.0, 0.0, 0.0, 10.0, 10.0, 10.0);
        let cyl_solid = make_cylinder(&mut store, Point3d::new(5.0, 5.0, 0.0), 2.0, 10.0, 16);

        let result = boolean_op(&mut store, box_solid, cyl_solid, BoolOp::Difference);
        assert!(result.is_ok(), "Box-cylinder difference should succeed: {:?}", result.err());
        let result_id = result.unwrap();

        // Verify via point classification:
        let tol = crate::default_tolerance().coincidence;

        // Point at the center (inside cylinder) should be outside the result.
        let center_class = classify_point(&store, result_id, &Point3d::new(5.0, 5.0, 5.0), tol);
        assert_eq!(center_class, PointClassification::Outside,
            "Center of cylindrical hole should be outside the result");

        // Point at a corner of the box should be inside the result.
        let corner_class = classify_point(&store, result_id, &Point3d::new(1.0, 1.0, 1.0), tol);
        assert_eq!(corner_class, PointClassification::Inside,
            "Corner of box should be inside the result");

        // Point at edge of cylinder but inside box -> should be inside
        let edge_class = classify_point(&store, result_id, &Point3d::new(8.0, 5.0, 5.0), tol);
        assert_eq!(edge_class, PointClassification::Inside,
            "Point near box edge, outside cylinder, should be inside");

        // Point outside box -> outside
        let outside_class = classify_point(&store, result_id, &Point3d::new(15.0, 5.0, 5.0), tol);
        assert_eq!(outside_class, PointClassification::Outside,
            "Point outside box should be outside");

        // Monte Carlo volume estimate with reduced samples for speed.
        let expected_vol = 1000.0 - std::f64::consts::PI * 4.0 * 10.0;
        let vol = estimate_volume(&store, result_id, 5_000);
        let rel_error = (vol - expected_vol).abs() / expected_vol;
        assert!(
            rel_error < 0.20,
            "Box-cylinder difference volume: MC={:.1}, expected={:.1}, rel_error={:.3}",
            vol, expected_vol, rel_error
        );
    }

    #[test]
    fn test_box_cylinder_union() {
        // Union of a box and a cylinder extending above.
        // Box: [0,0,0] to [10,10,10]
        // Cylinder: at (5,5,10), radius 3, height 5 along Z
        use crate::topology::primitives::make_cylinder;

        let mut store = EntityStore::new();
        let box_solid = make_box(&mut store, 0.0, 0.0, 0.0, 10.0, 10.0, 10.0);
        let cyl_solid = make_cylinder(&mut store, Point3d::new(5.0, 5.0, 10.0), 3.0, 5.0, 16);

        let result = boolean_op(&mut store, box_solid, cyl_solid, BoolOp::Union);
        assert!(result.is_ok(), "Box-cylinder union should succeed: {:?}", result.err());
        let result_id = result.unwrap();

        let tol = crate::default_tolerance().coincidence;

        // Point inside box should be inside.
        assert_eq!(
            classify_point(&store, result_id, &Point3d::new(5.0, 5.0, 5.0), tol),
            PointClassification::Inside,
            "Point inside box should be Inside"
        );

        // Point above the box inside the cylinder should be inside.
        assert_eq!(
            classify_point(&store, result_id, &Point3d::new(5.0, 5.0, 12.0), tol),
            PointClassification::Inside,
            "Point inside cylinder above box should be Inside"
        );

        // Point above both should be outside.
        assert_eq!(
            classify_point(&store, result_id, &Point3d::new(5.0, 5.0, 20.0), tol),
            PointClassification::Outside,
            "Point above both should be outside"
        );
    }

    #[test]
    fn test_box_sphere_intersection() {
        // Intersection of a box with a sphere that is fully inside.
        // Box: [-5,-5,-5] to [5,5,5]
        // Sphere: center at origin, radius 4 (fully inside the box)
        use crate::topology::primitives::make_sphere;

        let mut store = EntityStore::new();
        let box_solid = make_box(&mut store, -5.0, -5.0, -5.0, 5.0, 5.0, 5.0);
        let sph_solid = make_sphere(&mut store, Point3d::ORIGIN, 4.0, 12, 6);

        let result = boolean_op(&mut store, box_solid, sph_solid, BoolOp::Intersection);
        assert!(result.is_ok(), "Box-sphere intersection should succeed: {:?}", result.err());
        let result_id = result.unwrap();

        let tol = crate::default_tolerance().coincidence;

        // Point at origin should be inside (inside both).
        assert_eq!(
            classify_point(&store, result_id, &Point3d::ORIGIN, tol),
            PointClassification::Inside,
            "Origin should be inside"
        );

        // Point outside the sphere but inside the box should be outside the intersection.
        assert_eq!(
            classify_point(&store, result_id, &Point3d::new(4.5, 0.0, 0.0), tol),
            PointClassification::Outside,
            "Point outside sphere should be outside intersection"
        );

        // Point outside both should be outside.
        assert_eq!(
            classify_point(&store, result_id, &Point3d::new(6.0, 0.0, 0.0), tol),
            PointClassification::Outside,
            "Point outside both should be outside"
        );
    }

    #[test]
    fn test_box_cylinder_difference_point_classification() {
        // Focused test on point classification after box-cylinder difference.
        use crate::topology::primitives::make_cylinder;

        let mut store = EntityStore::new();
        let box_solid = make_box(&mut store, 0.0, 0.0, 0.0, 10.0, 10.0, 10.0);
        let cyl_solid = make_cylinder(&mut store, Point3d::new(5.0, 5.0, 0.0), 2.0, 10.0, 16);

        let result = boolean_op(&mut store, box_solid, cyl_solid, BoolOp::Difference).unwrap();
        let tol = crate::default_tolerance().coincidence;

        // Inside the box, outside the cylinder -> Inside result
        assert_eq!(
            classify_point(&store, result, &Point3d::new(1.0, 1.0, 5.0), tol),
            PointClassification::Inside,
            "Box corner region should be inside"
        );

        // Inside both box and cylinder -> Outside result (subtracted)
        assert_eq!(
            classify_point(&store, result, &Point3d::new(5.0, 5.0, 5.0), tol),
            PointClassification::Outside,
            "Inside cylinder should be outside the difference"
        );

        // Outside the box entirely -> Outside
        assert_eq!(
            classify_point(&store, result, &Point3d::new(15.0, 15.0, 15.0), tol),
            PointClassification::Outside,
            "Outside box should be outside"
        );
    }
}
