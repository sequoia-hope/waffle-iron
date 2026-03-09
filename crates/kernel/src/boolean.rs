//! Box-box boolean operations using convex face-polygon clipping.
//!
//! Supports Union, Subtract, and Intersect on axis-aligned box solids
//! produced by the WaffleKernel extrude pipeline. Uses Sutherland-Hodgman
//! polygon clipping against convex half-spaces to classify face fragments
//! as inside, outside, or partial with respect to the opposing solid.

use crate::geometry::curve::{CurveGeom, Line3D};
use crate::geometry::point::{Point3, Vector3};
use crate::geometry::surface::{Plane, SurfaceGeom};
use crate::topology::arena::TopoArena;
use crate::topology::half_edge::*;
use crate::types::*;
use crate::waffle_kernel::WaffleSolid;
use std::collections::HashMap;

// ── Public types ────────────────────────────────────────────────────────

/// The boolean operation to perform.
#[derive(Debug, Clone, Copy)]
pub(crate) enum BoolOp {
    Union,
    Subtract,
    Intersect,
}

/// Result of a boolean operation: a new B-Rep solid with topology and geometry.
pub(crate) struct BooleanResult {
    pub arena: TopoArena,
    pub face_map: HashMap<u64, FaceIdx>,
    pub edge_map: HashMap<u64, EdgeIdx>,
    pub vertex_map: HashMap<u64, VertexIdx>,
    pub face_geometry: HashMap<FaceIdx, SurfaceGeom>,
    pub edge_geometry: HashMap<EdgeIdx, CurveGeom>,
}

// ── Internal types ──────────────────────────────────────────────────────

/// A planar polygon with its face normal and a representative origin point.
#[derive(Debug, Clone)]
struct FacePoly {
    verts: Vec<[f64; 3]>,
    normal: [f64; 3],
    origin: [f64; 3],
}

// ── Vector math helpers ─────────────────────────────────────────────────

fn v3_sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn v3_add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn v3_scale(v: [f64; 3], s: f64) -> [f64; 3] {
    [v[0] * s, v[1] * s, v[2] * s]
}

fn v3_dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn v3_cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn v3_length(v: [f64; 3]) -> f64 {
    v3_dot(v, v).sqrt()
}

fn v3_negate(v: [f64; 3]) -> [f64; 3] {
    [-v[0], -v[1], -v[2]]
}

/// Compute polygon area using cross-product accumulation (works in 3D).
fn polygon_area_3d(verts: &[[f64; 3]]) -> f64 {
    if verts.len() < 3 {
        return 0.0;
    }
    let mut sum = [0.0, 0.0, 0.0];
    for i in 1..verts.len() - 1 {
        let ab = v3_sub(verts[i], verts[0]);
        let ac = v3_sub(verts[i + 1], verts[0]);
        let c = v3_cross(ab, ac);
        sum = v3_add(sum, c);
    }
    v3_length(sum) * 0.5
}

// ── Face polygon extraction ─────────────────────────────────────────────

/// Walk the outer loop of a face, collecting vertex positions.
fn collect_face_vertices(arena: &TopoArena, face_idx: FaceIdx) -> Vec<[f64; 3]> {
    let loop_idx = arena.faces[face_idx.0].outer_loop;
    let start_he = arena.loops[loop_idx.0].half_edge;
    let mut verts = Vec::new();
    let mut he = start_he;
    loop {
        let v = arena.half_edges[he.0].origin;
        verts.push(arena.vertices[v.0].position);
        he = arena.half_edges[he.0].next;
        if he == start_he {
            break;
        }
    }
    verts
}

/// Extract all face polygons from a WaffleSolid.
fn extract_face_polys(solid: &WaffleSolid) -> Vec<FacePoly> {
    let mut polys = Vec::new();
    for (&_kid, &face_idx) in &solid.face_map {
        let verts = collect_face_vertices(&solid.arena, face_idx);
        if verts.is_empty() {
            continue;
        }
        let (normal, origin) = match solid.face_geometry.get(&face_idx) {
            Some(SurfaceGeom::Planar(p)) => (
                [p.normal.x, p.normal.y, p.normal.z],
                [p.origin.x, p.origin.y, p.origin.z],
            ),
            _ => continue, // Skip non-planar faces for box-box booleans
        };
        polys.push(FacePoly {
            verts,
            normal,
            origin,
        });
    }
    polys
}

// ── Sutherland-Hodgman polygon clipping ─────────────────────────────────

/// Clip a polygon to keep only the portion on the INWARD side of a plane.
/// Points where `dot(p - plane_point, inward_normal) >= -tau` are kept.
fn clip_polygon_by_plane(
    verts: &[[f64; 3]],
    plane_point: [f64; 3],
    inward_normal: [f64; 3],
    tau: f64,
) -> Vec<[f64; 3]> {
    if verts.is_empty() {
        return vec![];
    }

    let mut output = Vec::with_capacity(verts.len() + 1);

    let dist = |p: [f64; 3]| -> f64 { v3_dot(v3_sub(p, plane_point), inward_normal) };

    let n = verts.len();
    for i in 0..n {
        let current = verts[i];
        let next = verts[(i + 1) % n];
        let d_current = dist(current);
        let d_next = dist(next);

        let current_inside = d_current >= -tau;
        let next_inside = d_next >= -tau;

        if current_inside {
            output.push(current);
            if !next_inside {
                // Crossing from inside to outside: emit intersection
                let t = d_current / (d_current - d_next);
                let intersection = v3_add(current, v3_scale(v3_sub(next, current), t));
                output.push(intersection);
            }
        } else if next_inside {
            // Crossing from outside to inside: emit intersection then next
            let t = d_current / (d_current - d_next);
            let intersection = v3_add(current, v3_scale(v3_sub(next, current), t));
            output.push(intersection);
        }
    }

    output
}

/// Check if a face polygon is coplanar with an opposing face.
fn is_coplanar(face_normal: [f64; 3], face_point: [f64; 3], opp: &FacePoly, tau: f64) -> bool {
    let dot_n = v3_dot(face_normal, opp.normal);
    if dot_n.abs() > 1.0 - 1e-6 {
        let dist = v3_dot(v3_sub(face_point, opp.origin), opp.normal).abs();
        dist < tau * 100.0
    } else {
        false
    }
}

/// Clip a polygon by a convex solid's interior (intersection of inward half-spaces).
/// For a convex solid, each face's inward normal is the NEGATION of its outward normal.
///
/// If `face_normal` is provided, skip opposing faces that are coplanar with the
/// polygon being clipped. Two faces are coplanar when their normals are parallel
/// (or anti-parallel) and a vertex of the polygon lies on the opposing face's plane.
fn clip_polygon_by_solid(
    verts: &[[f64; 3]],
    opposing_faces: &[FacePoly],
    tau: f64,
    face_normal: Option<[f64; 3]>,
) -> Vec<[f64; 3]> {
    let mut current = verts.to_vec();
    for face in opposing_faces {
        if current.is_empty() {
            break;
        }

        // Skip coplanar opposing faces
        if let Some(fn_normal) = face_normal {
            if is_coplanar(fn_normal, current[0], face, tau) {
                continue;
            }
        }

        // Inward normal = negation of the face's outward normal
        let inward = v3_negate(face.normal);
        current = clip_polygon_by_plane(&current, face.origin, inward, tau);
    }
    current
}

// ── Face classification ─────────────────────────────────────────────────

/// Classification of a face fragment with respect to the opposing solid.
#[derive(Debug)]
enum FaceClass {
    /// Entirely outside the opposing solid.
    Outside,
    /// Entirely inside the opposing solid.
    Inside,
    /// Partially inside: split into inside and outside fragments.
    Partial {
        inside: Vec<[f64; 3]>,
        outside: Vec<[f64; 3]>,
    },
}

/// Classify a face polygon against the opposing solid's faces.
fn classify_face(face: &FacePoly, opposing: &[FacePoly], tau: f64) -> FaceClass {
    let original_area = polygon_area_3d(&face.verts);
    if original_area < 1e-15 {
        return FaceClass::Outside;
    }

    // Check if this face has any coplanar partner on the opposing solid
    let has_coplanar = opposing
        .iter()
        .any(|opp| is_coplanar(face.normal, face.verts[0], opp, tau));

    let inside = clip_polygon_by_solid(&face.verts, opposing, tau, Some(face.normal));
    let inside_area = polygon_area_3d(&inside);

    // Face is fully outside if inside clip has negligible area
    if inside_area < 1e-15 {
        return FaceClass::Outside;
    }

    // Face is fully inside if inside clip matches original area
    let rel_diff = (inside_area - original_area).abs() / original_area;
    if rel_diff < 1e-6 {
        if has_coplanar {
            // The face lies on the surface of the opposing solid (coplanar match).
            // Treat as Partial with inside = original, outside = empty.
            return FaceClass::Partial {
                inside: face.verts.clone(),
                outside: vec![],
            };
        }
        // No coplanar partner → truly inside the opposing solid's volume.
        return FaceClass::Inside;
    }

    // Partial: find the critical splitting plane from the opposing solid
    let outside = find_outside_fragment(&face.verts, opposing, tau);
    FaceClass::Partial { inside, outside }
}

/// For a partially-clipped face, find the outside fragment by identifying
/// the critical B-face plane that cuts through the face and clipping
/// against its outward side.
fn find_outside_fragment(
    face_verts: &[[f64; 3]],
    opposing: &[FacePoly],
    tau: f64,
) -> Vec<[f64; 3]> {
    let original_area = polygon_area_3d(face_verts);

    for opp_face in opposing {
        // Clip against the OUTWARD side of this opposing face's plane.
        // The outward side keeps points where dot(p - origin, outward_normal) >= -tau.
        let clipped = clip_polygon_by_plane(face_verts, opp_face.origin, opp_face.normal, tau);
        let clipped_area = polygon_area_3d(&clipped);

        // If clipping reduced the polygon (but didn't eliminate it), this is the
        // critical cutting plane and the clipped result is the outside fragment.
        if clipped_area > 1e-15 {
            let rel_diff = (clipped_area - original_area).abs() / original_area;
            if rel_diff > 1e-6 {
                return clipped;
            }
        }
    }

    // Fallback: no single plane cut — return empty (treat as fully inside)
    vec![]
}

// ── Boolean operation dispatch ──────────────────────────────────────────

/// Perform a boolean operation on two box solids.
pub(crate) fn boolean_op(
    solid_a: &WaffleSolid,
    solid_b: &WaffleSolid,
    op: BoolOp,
    _opts: &BooleanOptions,
    id_alloc: &mut dyn FnMut() -> u64,
) -> Result<BooleanResult, KernelError> {
    let tau = 1e-9;
    let tau_weld = 1e-7;

    let a_faces = extract_face_polys(solid_a);
    let b_faces = extract_face_polys(solid_b);

    if a_faces.is_empty() || b_faces.is_empty() {
        return Err(KernelError::BooleanFailed {
            reason: "one or both solids have no planar faces".to_string(),
        });
    }

    // Classify each face of A against B's volume, and vice versa
    let a_classified: Vec<(FacePoly, FaceClass)> = a_faces
        .iter()
        .map(|f| (f.clone(), classify_face(f, &b_faces, tau)))
        .collect();

    let b_classified: Vec<(FacePoly, FaceClass)> = b_faces
        .iter()
        .map(|f| (f.clone(), classify_face(f, &a_faces, tau)))
        .collect();

    // Select face fragments based on the operation.
    //
    // With coplanar-face skipping in clip_polygon_by_solid:
    // - "Inside" for a non-coplanar face means truly inside the opposing volume
    // - "Partial" means the face has a coplanar partner on the opposing solid;
    //   the "inside" portion is the coplanar overlap region (on the surface, not
    //   inside the volume), and the "outside" portion is beyond the opposing solid.
    //
    // For Union:
    //   A_outside + A_coplanar_overlap + B_outside
    //   (the overlap is taken from A only to avoid duplication)
    //
    // For Subtract (A-B):
    //   A_outside + B_truly_inside_A (flipped normals)
    //   (coplanar overlap is removed from A; B's truly-inside face at the cut
    //    boundary replaces A's removed face)
    //
    // For Intersect:
    //   A_truly_inside + A_coplanar_overlap
    let mut result_polys = Vec::new();

    match op {
        BoolOp::Union => {
            // For union, partial faces (coplanar overlap) contribute:
            // - The ORIGINAL face from A (outside + inside = full face)
            // - Only the outside from B (non-overlapping portion of B)
            // Fully-inside faces are hidden inside the other solid: discard.
            // Fully-outside faces: keep.
            collect_union_fragments(&a_classified, &mut result_polys, true);
            collect_union_fragments(&b_classified, &mut result_polys, false);
        }
        BoolOp::Subtract => {
            // A - B: keep A's outside fragments only (coplanar overlap is cut away)
            // plus B's fully-inside-A faces with flipped normals (the new cut wall)
            collect_fragments(&a_classified, &mut result_polys, false, true, false, false);
            collect_fragments(&b_classified, &mut result_polys, true, false, true, false);
        }
        BoolOp::Intersect => {
            // A intersect B: keep A's fully-inside-B faces + coplanar overlap from A
            // plus B's fully-inside-A faces (the opposing wall)
            collect_fragments(&a_classified, &mut result_polys, false, false, true, true);
            collect_fragments(&b_classified, &mut result_polys, false, false, true, false);
        }
    }

    if result_polys.is_empty() {
        return Err(KernelError::BooleanFailed {
            reason: "boolean result has no faces".to_string(),
        });
    }

    build_brep_from_polygons(&result_polys, tau_weld, id_alloc)
}

/// Collect face fragments from classified faces.
///
/// - `flip_normals`: reverse normal and winding of collected faces
/// - `include_outside`: collect Outside faces and Partial outside fragments
/// - `include_fully_inside`: collect fully-Inside faces (truly enclosed by opposing solid)
/// - `include_partial_inside`: collect Partial inside fragments (coplanar overlap regions)
fn collect_fragments(
    classified: &[(FacePoly, FaceClass)],
    output: &mut Vec<FacePoly>,
    flip_normals: bool,
    include_outside: bool,
    include_fully_inside: bool,
    include_partial_inside: bool,
) {
    let emit =
        |output: &mut Vec<FacePoly>, verts: Vec<[f64; 3]>, normal: [f64; 3], origin: [f64; 3]| {
            if verts.len() < 3 {
                return;
            }
            let mut f = FacePoly {
                verts,
                normal,
                origin,
            };
            if flip_normals {
                f.normal = v3_negate(f.normal);
                f.verts.reverse();
            }
            output.push(f);
        };

    for (face, class) in classified {
        match class {
            FaceClass::Outside => {
                if include_outside {
                    emit(output, face.verts.clone(), face.normal, face.origin);
                }
            }
            FaceClass::Inside => {
                if include_fully_inside {
                    emit(output, face.verts.clone(), face.normal, face.origin);
                }
            }
            FaceClass::Partial { inside, outside } => {
                if include_outside {
                    emit(output, outside.clone(), face.normal, face.origin);
                }
                if include_partial_inside {
                    emit(output, inside.clone(), face.normal, face.origin);
                }
            }
        }
    }
}

/// Collect face fragments for a union operation.
///
/// - `is_primary`: if true, partial faces contribute the ORIGINAL face (outside + inside
///   merged back into the full face). If false, only the outside fragment is emitted.
///   This avoids duplicating the coplanar overlap region.
fn collect_union_fragments(
    classified: &[(FacePoly, FaceClass)],
    output: &mut Vec<FacePoly>,
    is_primary: bool,
) {
    for (face, class) in classified {
        match class {
            FaceClass::Outside => {
                output.push(face.clone());
            }
            FaceClass::Inside => {
                // Fully-inside faces are hidden — discard for union
            }
            FaceClass::Partial { outside, .. } => {
                if is_primary {
                    // Primary solid: emit the ORIGINAL face (= outside + inside combined)
                    output.push(face.clone());
                } else if outside.len() >= 3 {
                    // Secondary solid: emit only the outside fragment
                    output.push(FacePoly {
                        verts: outside.clone(),
                        normal: face.normal,
                        origin: face.origin,
                    });
                }
            }
        }
    }
}

// ── B-Rep construction from polygon soup ────────────────────────────────

/// Build a complete B-Rep (arena + maps + geometry) from a list of face polygons.
///
/// Steps:
/// 1. Weld vertices by quantizing to `tau_weld` grid
/// 2. Create faces and loops with half-edges
/// 3. Pair twin half-edges to form edges
/// 4. Assign planar geometry to faces and linear geometry to edges
/// 5. Build KernelId maps for all entities
fn build_brep_from_polygons(
    faces: &[FacePoly],
    tau_weld: f64,
    id_alloc: &mut dyn FnMut() -> u64,
) -> Result<BooleanResult, KernelError> {
    let mut arena = TopoArena::new();
    let mut face_map = HashMap::new();
    let mut edge_map = HashMap::new();
    let mut vertex_map = HashMap::new();
    let mut face_geometry: HashMap<FaceIdx, SurfaceGeom> = HashMap::new();
    let mut edge_geometry: HashMap<EdgeIdx, CurveGeom> = HashMap::new();

    // Step 1: Weld vertices — quantize positions to tau_weld grid
    let inv_tau = 1.0 / tau_weld;
    let mut pos_to_vertex: HashMap<(i64, i64, i64), VertexIdx> = HashMap::new();

    let quantize = |p: [f64; 3]| -> (i64, i64, i64) {
        (
            (p[0] * inv_tau).round() as i64,
            (p[1] * inv_tau).round() as i64,
            (p[2] * inv_tau).round() as i64,
        )
    };

    // Pre-scan all face vertices to build the welded vertex set
    let mut face_vert_indices: Vec<Vec<VertexIdx>> = Vec::with_capacity(faces.len());
    for face_poly in faces {
        let mut indices = Vec::with_capacity(face_poly.verts.len());
        for &pos in &face_poly.verts {
            let key = quantize(pos);
            let vidx = *pos_to_vertex
                .entry(key)
                .or_insert_with(|| arena.add_vertex(pos));
            indices.push(vidx);
        }
        face_vert_indices.push(indices);
    }

    // Step 2: Create solid, shell, faces, loops, and half-edges
    let solid_idx = arena.add_solid();
    let shell_idx = arena.add_shell(solid_idx);
    arena.solids[solid_idx.0].outer_shell = shell_idx;

    // Map directed edges (from, to) → HalfEdgeIdx for twin pairing
    let mut directed_he: HashMap<(VertexIdx, VertexIdx), HalfEdgeIdx> = HashMap::new();
    // Track all half-edges that need twin pairing
    let mut unpaired_hes: Vec<HalfEdgeIdx> = Vec::new();

    let mut first_face_idx = None;

    for (fi, face_poly) in faces.iter().enumerate() {
        let vert_indices = &face_vert_indices[fi];
        let n = vert_indices.len();
        if n < 3 {
            continue;
        }

        // Deduplicate consecutive vertices (from welding)
        let mut deduped_verts: Vec<VertexIdx> = Vec::with_capacity(n);
        for i in 0..n {
            let v = vert_indices[i];
            let prev = vert_indices[(i + n - 1) % n];
            if v != prev {
                deduped_verts.push(v);
            }
        }
        if deduped_verts.len() < 3 {
            continue;
        }

        let face_idx = arena.add_face(shell_idx);
        if first_face_idx.is_none() {
            first_face_idx = Some(face_idx);
        }
        let loop_idx = arena.add_loop(face_idx);
        arena.faces[face_idx.0].outer_loop = loop_idx;

        // Assign face geometry
        face_geometry.insert(
            face_idx,
            SurfaceGeom::Planar(Plane {
                origin: Point3::from_array(face_poly.origin),
                normal: Vector3::from_array(face_poly.normal),
            }),
        );

        // Allocate KernelId for this face
        let fid = id_alloc();
        face_map.insert(fid, face_idx);

        // Create half-edges for this face loop
        let m = deduped_verts.len();
        let first_he_idx = HalfEdgeIdx(arena.half_edges.len());

        for i in 0..m {
            let origin = deduped_verts[i];
            let he_idx = HalfEdgeIdx(arena.half_edges.len());
            let next_he = HalfEdgeIdx(first_he_idx.0 + ((i + 1) % m));
            let prev_he = HalfEdgeIdx(first_he_idx.0 + ((i + m - 1) % m));

            arena.half_edges.push(HalfEdge {
                origin,
                edge: EdgeIdx(0), // placeholder, set during twin pairing
                twin: he_idx,     // placeholder, set during twin pairing
                next: next_he,
                prev: prev_he,
                loop_: loop_idx,
            });

            // Set vertex half-edge reference
            arena.vertices[origin.0].half_edge = Some(he_idx);

            // Register directed edge for twin pairing
            let dest = deduped_verts[(i + 1) % m];
            directed_he.insert((origin, dest), he_idx);
            unpaired_hes.push(he_idx);
        }

        // Set loop's half-edge
        arena.loops[loop_idx.0].half_edge = first_he_idx;
    }

    // Set shell's face reference
    if let Some(ff) = first_face_idx {
        arena.shells[shell_idx.0].face = ff;
    }

    // Step 3: Twin pairing — match directed edges (A→B) with (B→A)
    let mut paired: std::collections::HashSet<HalfEdgeIdx> = std::collections::HashSet::new();

    for &he_idx in &unpaired_hes {
        if paired.contains(&he_idx) {
            continue;
        }
        let origin = arena.half_edges[he_idx.0].origin;
        let next_he = arena.half_edges[he_idx.0].next;
        let dest = arena.half_edges[next_he.0].origin;

        // Look for twin: the half-edge going from dest to origin
        if let Some(&twin_idx) = directed_he.get(&(dest, origin)) {
            if twin_idx != he_idx && !paired.contains(&twin_idx) {
                // Create an edge for this pair
                let edge_idx = EdgeIdx(arena.edges.len());
                arena.edges.push(Edge { half_edge: he_idx });

                arena.half_edges[he_idx.0].twin = twin_idx;
                arena.half_edges[he_idx.0].edge = edge_idx;
                arena.half_edges[twin_idx.0].twin = he_idx;
                arena.half_edges[twin_idx.0].edge = edge_idx;

                paired.insert(he_idx);
                paired.insert(twin_idx);

                // Assign edge geometry
                let p0 = arena.vertices[origin.0].position;
                let p1 = arena.vertices[dest.0].position;
                let dir = v3_sub(p1, p0);
                edge_geometry.insert(
                    edge_idx,
                    CurveGeom::Linear(Line3D {
                        origin: Point3::from_array(p0),
                        direction: Vector3::from_array(dir),
                    }),
                );

                // Allocate KernelId for this edge
                let eid = id_alloc();
                edge_map.insert(eid, edge_idx);
            }
        }
    }

    // Step 4: Verify all half-edges are paired (manifold check)
    let unpaired_count = unpaired_hes.len() - paired.len();
    if unpaired_count > 0 {
        return Err(KernelError::BooleanFailed {
            reason: format!(
                "non-manifold result: {} half-edges unpaired out of {}",
                unpaired_count,
                unpaired_hes.len()
            ),
        });
    }

    // Step 5: Build vertex map
    for (idx, _) in arena.vertices.iter().enumerate() {
        let vid = id_alloc();
        vertex_map.insert(vid, VertexIdx(idx));
    }

    Ok(BooleanResult {
        arena,
        face_map,
        edge_map,
        vertex_map,
        face_geometry,
        edge_geometry,
    })
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::Kernel;
    use crate::waffle_kernel::WaffleKernel;

    // ── Test helpers ────────────────────────────────────────────────

    /// Create a rectangular profile centered at (cx, cy) with width w and height h.
    fn make_rect_profile(
        cx: f64,
        cy: f64,
        w: f64,
        h: f64,
    ) -> (Vec<ClosedProfile>, HashMap<u32, (f64, f64)>) {
        let mut positions = HashMap::new();
        positions.insert(1, (cx - w / 2.0, cy - h / 2.0));
        positions.insert(2, (cx + w / 2.0, cy - h / 2.0));
        positions.insert(3, (cx + w / 2.0, cy + h / 2.0));
        positions.insert(4, (cx - w / 2.0, cy + h / 2.0));

        let profile = ClosedProfile {
            entity_ids: vec![10, 11, 12, 13],
            is_outer: true,
            circle: None,
            spline_segments: vec![],
        };

        (vec![profile], positions)
    }

    const XY_ORIGIN: [f64; 3] = [0.0, 0.0, 0.0];
    const XY_NORMAL: [f64; 3] = [0.0, 0.0, 1.0];
    const XY_X_AXIS: [f64; 3] = [1.0, 0.0, 0.0];
    const Z_DIR: [f64; 3] = [0.0, 0.0, 1.0];

    /// Create a box solid and return the WaffleSolid reference inside the kernel.
    fn make_box_solid(
        kernel: &mut WaffleKernel,
        cx: f64,
        cy: f64,
        w: f64,
        h: f64,
        depth: f64,
    ) -> KernelSolidHandle {
        let (profiles, positions) = make_rect_profile(cx, cy, w, h);
        let face_ids = kernel
            .make_faces_from_profiles(&profiles, XY_ORIGIN, XY_NORMAL, XY_X_AXIS, &positions)
            .expect("make_faces_from_profiles should succeed");
        kernel
            .extrude_face(face_ids[0], Z_DIR, depth)
            .expect("extrude_face should succeed")
    }

    /// Perform a boolean op on two boxes via the Kernel trait and return the handle.
    fn do_boolean_via_kernel(
        cx_a: f64,
        cy_a: f64,
        w_a: f64,
        h_a: f64,
        d_a: f64,
        cx_b: f64,
        cy_b: f64,
        w_b: f64,
        h_b: f64,
        d_b: f64,
        op: BoolOp,
    ) -> Result<(WaffleKernel, KernelSolidHandle), KernelError> {
        let mut kernel = WaffleKernel::new();
        let handle_a = make_box_solid(&mut kernel, cx_a, cy_a, w_a, h_a, d_a);
        let handle_b = make_box_solid(&mut kernel, cx_b, cy_b, w_b, h_b, d_b);

        let result = match op {
            BoolOp::Union => kernel.boolean_union(&handle_a, &handle_b)?,
            BoolOp::Subtract => kernel.boolean_subtract(&handle_a, &handle_b)?,
            BoolOp::Intersect => kernel.boolean_intersect(&handle_a, &handle_b)?,
        };
        Ok((kernel, result))
    }

    // Standard test case: A at x=[0,10], y=[0,10], z=[0,10]
    //                      B at x=[5,15], y=[0,10], z=[0,10]

    // ── Vector math unit tests ──────────────────────────────────────

    #[test]
    fn vec_sub() {
        let r = v3_sub([3.0, 2.0, 1.0], [1.0, 1.0, 1.0]);
        assert!((r[0] - 2.0).abs() < 1e-15);
        assert!((r[1] - 1.0).abs() < 1e-15);
        assert!((r[2] - 0.0).abs() < 1e-15);
    }

    #[test]
    fn vec_dot() {
        let d = v3_dot([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
        assert!(d.abs() < 1e-15);
    }

    #[test]
    fn vec_cross() {
        let c = v3_cross([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
        assert!((c[2] - 1.0).abs() < 1e-15);
    }

    // ── Clipping unit tests ─────────────────────────────────────────

    #[test]
    fn clip_square_by_half_plane() {
        // Unit square in XY plane, clip by x >= 0.5
        let square = vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
        ];
        let clipped = clip_polygon_by_plane(
            &square,
            [0.5, 0.0, 0.0], // plane point
            [1.0, 0.0, 0.0], // inward normal (keep x >= 0.5)
            1e-9,
        );
        let area = polygon_area_3d(&clipped);
        assert!(
            (area - 0.5).abs() < 0.01,
            "Clipped area should be ~0.5, got {}",
            area
        );
    }

    #[test]
    fn clip_fully_inside() {
        let square = vec![
            [0.2, 0.2, 0.0],
            [0.8, 0.2, 0.0],
            [0.8, 0.8, 0.0],
            [0.2, 0.8, 0.0],
        ];
        let clipped = clip_polygon_by_plane(
            &square,
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0], // keep x >= 0
            1e-9,
        );
        let orig_area = polygon_area_3d(&square);
        let clip_area = polygon_area_3d(&clipped);
        assert!(
            (clip_area - orig_area).abs() < 1e-10,
            "Fully-inside clip should preserve area"
        );
    }

    #[test]
    fn clip_fully_outside() {
        let square = vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
        ];
        let clipped = clip_polygon_by_plane(
            &square,
            [2.0, 0.0, 0.0],
            [1.0, 0.0, 0.0], // keep x >= 2
            1e-9,
        );
        assert!(
            clipped.is_empty() || polygon_area_3d(&clipped) < 1e-15,
            "Fully-outside clip should produce empty polygon"
        );
    }

    #[test]
    fn polygon_area_triangle() {
        let tri = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let area = polygon_area_3d(&tri);
        assert!(
            (area - 0.5).abs() < 1e-10,
            "Right triangle area should be 0.5, got {}",
            area
        );
    }

    #[test]
    fn polygon_area_unit_square() {
        let sq = vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
        ];
        let area = polygon_area_3d(&sq);
        assert!(
            (area - 1.0).abs() < 1e-10,
            "Unit square area should be 1.0, got {}",
            area
        );
    }

    // ── Boolean operation integration tests ─────────────────────────

    use crate::traits::KernelIntrospect;

    #[test]
    fn union_face_count() {
        let (k, result) = do_boolean_via_kernel(
            5.0,
            5.0,
            10.0,
            10.0,
            10.0,
            10.0,
            5.0,
            10.0,
            10.0,
            10.0,
            BoolOp::Union,
        )
        .expect("union should succeed");
        let faces = k.list_faces(&result);
        assert_eq!(
            faces.len(),
            10,
            "Union should have 10 faces, got {}",
            faces.len()
        );
    }

    #[test]
    fn subtract_face_count() {
        let (k, result) = do_boolean_via_kernel(
            5.0,
            5.0,
            10.0,
            10.0,
            10.0,
            10.0,
            5.0,
            10.0,
            10.0,
            10.0,
            BoolOp::Subtract,
        )
        .expect("subtract should succeed");
        let faces = k.list_faces(&result);
        assert_eq!(
            faces.len(),
            6,
            "Subtract should have 6 faces, got {}",
            faces.len()
        );
    }

    #[test]
    fn intersect_face_count() {
        let (k, result) = do_boolean_via_kernel(
            5.0,
            5.0,
            10.0,
            10.0,
            10.0,
            10.0,
            5.0,
            10.0,
            10.0,
            10.0,
            BoolOp::Intersect,
        )
        .expect("intersect should succeed");
        let faces = k.list_faces(&result);
        assert_eq!(
            faces.len(),
            6,
            "Intersect should have 6 faces, got {}",
            faces.len()
        );
    }

    #[test]
    fn union_euler_formula() {
        let (k, result) = do_boolean_via_kernel(
            5.0,
            5.0,
            10.0,
            10.0,
            10.0,
            10.0,
            5.0,
            10.0,
            10.0,
            10.0,
            BoolOp::Union,
        )
        .expect("union should succeed");
        let v = k.list_vertices(&result).len() as i64;
        let e = k.list_edges(&result).len() as i64;
        let f = k.list_faces(&result).len() as i64;
        assert_eq!(v - e + f, 2, "V-E+F must be 2 (V={}, E={}, F={})", v, e, f);
    }

    #[test]
    fn subtract_euler_formula() {
        let (k, result) = do_boolean_via_kernel(
            5.0,
            5.0,
            10.0,
            10.0,
            10.0,
            10.0,
            5.0,
            10.0,
            10.0,
            10.0,
            BoolOp::Subtract,
        )
        .expect("subtract should succeed");
        let v = k.list_vertices(&result).len() as i64;
        let e = k.list_edges(&result).len() as i64;
        let f = k.list_faces(&result).len() as i64;
        assert_eq!(v - e + f, 2, "V-E+F must be 2 (V={}, E={}, F={})", v, e, f);
    }

    #[test]
    fn intersect_euler_formula() {
        let (k, result) = do_boolean_via_kernel(
            5.0,
            5.0,
            10.0,
            10.0,
            10.0,
            10.0,
            5.0,
            10.0,
            10.0,
            10.0,
            BoolOp::Intersect,
        )
        .expect("intersect should succeed");
        let v = k.list_vertices(&result).len() as i64;
        let e = k.list_edges(&result).len() as i64;
        let f = k.list_faces(&result).len() as i64;
        assert_eq!(v - e + f, 2, "V-E+F must be 2 (V={}, E={}, F={})", v, e, f);
    }

    #[test]
    fn disjoint_boxes_union() {
        let (k, result) = do_boolean_via_kernel(
            5.0,
            5.0,
            10.0,
            10.0,
            10.0,
            100.0,
            5.0,
            10.0,
            10.0,
            10.0,
            BoolOp::Union,
        )
        .expect("disjoint union should succeed");
        let faces = k.list_faces(&result);
        assert_eq!(faces.len(), 12, "Disjoint union should have 12 faces");
    }

    #[test]
    fn disjoint_boxes_intersect_error() {
        let result = do_boolean_via_kernel(
            5.0,
            5.0,
            10.0,
            10.0,
            10.0,
            100.0,
            5.0,
            10.0,
            10.0,
            10.0,
            BoolOp::Intersect,
        );
        assert!(
            result.is_err(),
            "Intersect of disjoint boxes should produce an error"
        );
    }
}
