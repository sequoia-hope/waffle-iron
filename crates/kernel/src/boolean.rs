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

// ── SSI-based boolean operations (box-cylinder, cylinder-cylinder) ──────

use crate::geometry::curve::{Arc3D, Circle3D};
use crate::geometry::surface::Cylinder;
use crate::ssi::{self, Aabb};
use crate::waffle_kernel::CylinderParams;

/// Perform an SSI-based boolean operation on solids involving cylinders.
pub(crate) fn ssi_boolean_op(
    solid_a: &WaffleSolid,
    solid_b: &WaffleSolid,
    op: BoolOp,
    id_alloc: &mut dyn FnMut() -> u64,
) -> Result<BooleanResult, KernelError> {
    let a_is_cyl = solid_a.cylinder_params.is_some();
    let b_is_cyl = solid_b.cylinder_params.is_some();

    if a_is_cyl && b_is_cyl {
        // Cylinder-cylinder boolean
        let cyl_a = solid_a.cylinder_params.as_ref().unwrap();
        let cyl_b = solid_b.cylinder_params.as_ref().unwrap();
        cyl_cyl_boolean(cyl_a, cyl_b, op, id_alloc)
    } else if !a_is_cyl && b_is_cyl {
        // Box-cylinder boolean (A=box, B=cyl)
        let box_aabb = ssi::compute_box_aabb(solid_a);
        let cyl = solid_b.cylinder_params.as_ref().unwrap();
        box_cyl_boolean(&box_aabb, solid_a, cyl, op, id_alloc)
    } else if a_is_cyl && !b_is_cyl {
        // Cylinder-box boolean (A=cyl, B=box)
        let box_aabb = ssi::compute_box_aabb(solid_b);
        let cyl = solid_a.cylinder_params.as_ref().unwrap();
        match op {
            BoolOp::Union => box_cyl_boolean(&box_aabb, solid_b, cyl, BoolOp::Union, id_alloc),
            BoolOp::Intersect => {
                box_cyl_boolean(&box_aabb, solid_b, cyl, BoolOp::Intersect, id_alloc)
            }
            BoolOp::Subtract => Err(KernelError::NotSupported {
                operation: "cylinder minus box".to_string(),
            }),
        }
    } else {
        Err(KernelError::NotSupported {
            operation: "unsupported boolean operand combination".to_string(),
        })
    }
}

/// Box-cylinder boolean dispatch.
fn box_cyl_boolean(
    box_aabb: &Aabb,
    box_solid: &WaffleSolid,
    cyl: &CylinderParams,
    op: BoolOp,
    id_alloc: &mut dyn FnMut() -> u64,
) -> Result<BooleanResult, KernelError> {
    let enclosed = ssi::cyl_enclosed_in_box(cyl, box_aabb);
    let disjoint = ssi::box_cyl_disjoint(box_aabb, cyl);

    match op {
        BoolOp::Subtract => {
            if enclosed {
                build_box_minus_enclosed_cyl(box_aabb, cyl, id_alloc)
            } else if disjoint {
                clone_solid_as_result(box_solid, id_alloc)
            } else {
                Err(KernelError::NotSupported {
                    operation: "partial box-cylinder subtract".to_string(),
                })
            }
        }
        BoolOp::Union => {
            if enclosed {
                // Cylinder fully inside box → union = box
                clone_solid_as_result(box_solid, id_alloc)
            } else if disjoint {
                build_disjoint_box_cyl_union(box_aabb, cyl, id_alloc)
            } else {
                Err(KernelError::NotSupported {
                    operation: "partial box-cylinder union".to_string(),
                })
            }
        }
        BoolOp::Intersect => {
            if enclosed {
                // Cylinder fully inside box → intersect = cylinder
                build_cyl_result(cyl, id_alloc)
            } else if disjoint {
                Err(KernelError::BooleanFailed {
                    reason: "no intersection (disjoint)".to_string(),
                })
            } else {
                Err(KernelError::NotSupported {
                    operation: "partial box-cylinder intersect".to_string(),
                })
            }
        }
    }
}

/// Cylinder-cylinder boolean dispatch.
fn cyl_cyl_boolean(
    cyl_a: &CylinderParams,
    cyl_b: &CylinderParams,
    op: BoolOp,
    id_alloc: &mut dyn FnMut() -> u64,
) -> Result<BooleanResult, KernelError> {
    let disjoint = ssi::cyls_disjoint(cyl_a, cyl_b);

    if disjoint {
        match op {
            BoolOp::Union => build_disjoint_cyl_cyl_union(cyl_a, cyl_b, id_alloc),
            BoolOp::Subtract => build_cyl_result(cyl_a, id_alloc),
            BoolOp::Intersect => Err(KernelError::BooleanFailed {
                reason: "no intersection (disjoint cylinders)".to_string(),
            }),
        }
    } else {
        // Compute z range overlap
        let z_min = cyl_a.center_bottom[2].max(cyl_b.center_bottom[2]);
        let z_max =
            (cyl_a.center_bottom[2] + cyl_a.depth).min(cyl_b.center_bottom[2] + cyl_b.depth);
        if z_max <= z_min + 1e-9 {
            return Err(KernelError::BooleanFailed {
                reason: "no Z overlap".to_string(),
            });
        }

        // Compute 2D intersection points
        let c1 = [cyl_a.center_bottom[0], cyl_a.center_bottom[1]];
        let c2 = [cyl_b.center_bottom[0], cyl_b.center_bottom[1]];
        let r1 = cyl_a.radius;
        let r2 = cyl_b.radius;
        let dx = c2[0] - c1[0];
        let dy = c2[1] - c1[1];
        let d = (dx * dx + dy * dy).sqrt();
        let a = (r1 * r1 - r2 * r2 + d * d) / (2.0 * d);
        let h = (r1 * r1 - a * a).max(0.0).sqrt();
        let ux = dx / d;
        let uy = dy / d;
        let mid_x = c1[0] + a * ux;
        let mid_y = c1[1] + a * uy;
        let p1 = [mid_x - h * uy, mid_y + h * ux];
        let p2 = [mid_x + h * uy, mid_y - h * ux];

        build_partial_cyl_cyl(cyl_a, cyl_b, op, &p1, &p2, z_min, z_max, id_alloc)
    }
}

// ── Clone solid as BooleanResult ───────────────────────────────────────

/// Clone a WaffleSolid into a new BooleanResult with fresh IDs.
fn clone_solid_as_result(
    solid: &WaffleSolid,
    id_alloc: &mut dyn FnMut() -> u64,
) -> Result<BooleanResult, KernelError> {
    let mut face_map = HashMap::new();
    let mut edge_map = HashMap::new();
    let mut vertex_map = HashMap::new();

    for &idx in solid.face_map.values() {
        face_map.insert(id_alloc(), idx);
    }
    for &idx in solid.edge_map.values() {
        edge_map.insert(id_alloc(), idx);
    }
    for &idx in solid.vertex_map.values() {
        vertex_map.insert(id_alloc(), idx);
    }

    Ok(BooleanResult {
        arena: solid.arena.clone(),
        face_map,
        edge_map,
        vertex_map,
        face_geometry: solid.face_geometry.clone(),
        edge_geometry: solid.edge_geometry.clone(),
    })
}

// ── Build cylinder B-Rep from CylinderParams ───────────────────────────

/// Build a standalone cylinder B-Rep result (for intersect = cylinder case).
pub(crate) fn build_cyl_result(
    cyl: &CylinderParams,
    id_alloc: &mut dyn FnMut() -> u64,
) -> Result<BooleanResult, KernelError> {
    let center = cyl.center_bottom;
    let r = cyl.radius;
    let depth = cyl.depth;
    let dir = cyl.direction;
    let x_axis = cyl.x_axis;

    let bottom_seam = v3_add(center, v3_scale(x_axis, r));
    let top_seam = v3_add(bottom_seam, v3_scale(dir, depth));
    let top_center = v3_add(center, v3_scale(dir, depth));

    let mut arena = TopoArena::new();
    let solid_idx = arena.add_solid();
    let shell_idx = arena.add_shell(solid_idx);
    arena.solids[solid_idx.0].outer_shell = shell_idx;

    let bottom_face = arena.add_face(shell_idx);
    let top_face = arena.add_face(shell_idx);
    let side_face = arena.add_face(shell_idx);
    arena.shells[shell_idx.0].face = bottom_face;

    let bottom_loop = arena.add_loop(bottom_face);
    let top_loop = arena.add_loop(top_face);
    let side_loop = arena.add_loop(side_face);
    arena.faces[bottom_face.0].outer_loop = bottom_loop;
    arena.faces[top_face.0].outer_loop = top_loop;
    arena.faces[side_face.0].outer_loop = side_loop;

    let v_bottom = arena.add_vertex(bottom_seam);
    let v_top = arena.add_vertex(top_seam);

    let (e_bottom, he_bot_a, he_bot_b) = arena.add_edge();
    let (e_top, he_top_a, he_top_b) = arena.add_edge();
    let (e_seam, he_seam_a, he_seam_b) = arena.add_edge();

    // Bottom cap: self-loop
    arena.half_edges[he_bot_a.0].origin = v_bottom;
    arena.half_edges[he_bot_a.0].next = he_bot_a;
    arena.half_edges[he_bot_a.0].prev = he_bot_a;
    arena.half_edges[he_bot_a.0].loop_ = bottom_loop;
    arena.loops[bottom_loop.0].half_edge = he_bot_a;

    // Top cap: self-loop
    arena.half_edges[he_top_a.0].origin = v_top;
    arena.half_edges[he_top_a.0].next = he_top_a;
    arena.half_edges[he_top_a.0].prev = he_top_a;
    arena.half_edges[he_top_a.0].loop_ = top_loop;
    arena.loops[top_loop.0].half_edge = he_top_a;

    // Side: 4 half-edges: seam_a → top_b → seam_b → bot_b
    arena.half_edges[he_seam_a.0].origin = v_bottom;
    arena.half_edges[he_seam_a.0].next = he_top_b;
    arena.half_edges[he_seam_a.0].prev = he_bot_b;
    arena.half_edges[he_seam_a.0].loop_ = side_loop;

    arena.half_edges[he_top_b.0].origin = v_top;
    arena.half_edges[he_top_b.0].next = he_seam_b;
    arena.half_edges[he_top_b.0].prev = he_seam_a;
    arena.half_edges[he_top_b.0].loop_ = side_loop;

    arena.half_edges[he_seam_b.0].origin = v_top;
    arena.half_edges[he_seam_b.0].next = he_bot_b;
    arena.half_edges[he_seam_b.0].prev = he_top_b;
    arena.half_edges[he_seam_b.0].loop_ = side_loop;

    arena.half_edges[he_bot_b.0].origin = v_bottom;
    arena.half_edges[he_bot_b.0].next = he_seam_a;
    arena.half_edges[he_bot_b.0].prev = he_seam_b;
    arena.half_edges[he_bot_b.0].loop_ = side_loop;

    arena.loops[side_loop.0].half_edge = he_seam_a;

    arena.vertices[v_bottom.0].half_edge = Some(he_bot_a);
    arena.vertices[v_top.0].half_edge = Some(he_top_a);

    // Face geometry
    let mut face_geometry = HashMap::new();
    face_geometry.insert(
        bottom_face,
        SurfaceGeom::Planar(Plane {
            origin: Point3::from_array(center),
            normal: Vector3::from_array(v3_negate(dir)),
        }),
    );
    face_geometry.insert(
        top_face,
        SurfaceGeom::Planar(Plane {
            origin: Point3::from_array(top_center),
            normal: Vector3::from_array(dir),
        }),
    );
    face_geometry.insert(
        side_face,
        SurfaceGeom::Cylindrical(Cylinder {
            origin: Point3::from_array(center),
            axis: Vector3::from_array(dir),
            radius: r,
        }),
    );

    // Edge geometry
    let mut edge_geometry = HashMap::new();
    edge_geometry.insert(
        e_bottom,
        CurveGeom::Circular(Circle3D {
            center: Point3::from_array(center),
            normal: Vector3::from_array(v3_negate(dir)),
            radius: r,
        }),
    );
    edge_geometry.insert(
        e_top,
        CurveGeom::Circular(Circle3D {
            center: Point3::from_array(top_center),
            normal: Vector3::from_array(dir),
            radius: r,
        }),
    );
    edge_geometry.insert(
        e_seam,
        CurveGeom::Linear(Line3D {
            origin: Point3::from_array(bottom_seam),
            direction: Vector3::from_array(v3_scale(dir, depth)),
        }),
    );

    // Build maps
    let mut face_map = HashMap::new();
    let mut edge_map = HashMap::new();
    let mut vertex_map = HashMap::new();
    face_map.insert(id_alloc(), bottom_face);
    face_map.insert(id_alloc(), top_face);
    face_map.insert(id_alloc(), side_face);
    edge_map.insert(id_alloc(), e_bottom);
    edge_map.insert(id_alloc(), e_top);
    edge_map.insert(id_alloc(), e_seam);
    vertex_map.insert(id_alloc(), v_bottom);
    vertex_map.insert(id_alloc(), v_top);

    Ok(BooleanResult {
        arena,
        face_map,
        edge_map,
        vertex_map,
        face_geometry,
        edge_geometry,
    })
}

// ── Build box-minus-enclosed-cylinder ──────────────────────────────────

/// Build a box with a cylindrical through-hole (enclosed cylinder subtract).
///
/// Uses build_brep_from_polygons for the box (correct edge sharing),
/// then adds inner circle loops and the cylinder side face.
/// Result topology: 4 side faces + 2 holed caps + 1 cylinder inner face = 7 faces.
/// V=10, E=15, F=7 → V-E+F = 2.
fn build_box_minus_enclosed_cyl(
    aabb: &Aabb,
    cyl: &CylinderParams,
    id_alloc: &mut dyn FnMut() -> u64,
) -> Result<BooleanResult, KernelError> {
    let z_min = aabb.min[2];
    let z_max = aabb.max[2];
    let cx = cyl.center_bottom[0];
    let cy = cyl.center_bottom[1];
    let r = cyl.radius;
    let dir = cyl.direction;

    // Step 1: Build box using build_brep_from_polygons (correct shared edges)
    let box_faces = make_box_face_polys(aabb);
    let tau_weld = 1e-7;
    let mut result = build_brep_from_polygons(&box_faces, tau_weld, id_alloc)?;

    // Step 2: Find bottom and top face indices by normal direction
    let mut face_bot = None;
    let mut face_top = None;
    for (&fi, geom) in &result.face_geometry {
        if let SurfaceGeom::Planar(plane) = geom {
            if plane.normal.z < -0.5 {
                face_bot = Some(fi);
            } else if plane.normal.z > 0.5 {
                face_top = Some(fi);
            }
        }
    }
    let face_bot = face_bot.ok_or(KernelError::BooleanFailed {
        reason: "cannot find bottom face".to_string(),
    })?;
    let face_top = face_top.ok_or(KernelError::BooleanFailed {
        reason: "cannot find top face".to_string(),
    })?;

    // Step 3: Add cylinder seam vertices
    let bot_seam = [cx + r, cy, z_min];
    let top_seam = [cx + r, cy, z_max];
    let v_bot_seam = result.arena.add_vertex(bot_seam);
    let v_top_seam = result.arena.add_vertex(top_seam);

    // Step 4: Add inner circle loops for bottom and top caps
    let inner_loop_bot = result.arena.add_loop(face_bot);
    let inner_loop_top = result.arena.add_loop(face_top);
    result.arena.faces[face_bot.0]
        .inner_loops
        .push(inner_loop_bot);
    result.arena.faces[face_top.0]
        .inner_loops
        .push(inner_loop_top);

    // Inner circle self-loops
    let (e_bot_circle, he_ibot_a, he_ibot_b) = result.arena.add_edge();
    result.arena.half_edges[he_ibot_a.0].origin = v_bot_seam;
    result.arena.half_edges[he_ibot_a.0].next = he_ibot_a;
    result.arena.half_edges[he_ibot_a.0].prev = he_ibot_a;
    result.arena.half_edges[he_ibot_a.0].loop_ = inner_loop_bot;
    result.arena.loops[inner_loop_bot.0].half_edge = he_ibot_a;

    let (e_top_circle, he_itop_a, he_itop_b) = result.arena.add_edge();
    result.arena.half_edges[he_itop_a.0].origin = v_top_seam;
    result.arena.half_edges[he_itop_a.0].next = he_itop_a;
    result.arena.half_edges[he_itop_a.0].prev = he_itop_a;
    result.arena.half_edges[he_itop_a.0].loop_ = inner_loop_top;
    result.arena.loops[inner_loop_top.0].half_edge = he_itop_a;

    // Step 5: Add cylinder side face
    let shell_idx = ShellIdx(0);
    let face_cyl = result.arena.add_face(shell_idx);
    let loop_cyl = result.arena.add_loop(face_cyl);
    result.arena.faces[face_cyl.0].outer_loop = loop_cyl;

    // Seam edge (vertical)
    let (e_seam, he_seam_a, he_seam_b) = result.arena.add_edge();

    // Cylinder side loop: seam_a → itop_b → seam_b → ibot_b
    result.arena.half_edges[he_seam_a.0].origin = v_bot_seam;
    result.arena.half_edges[he_seam_a.0].next = he_itop_b;
    result.arena.half_edges[he_seam_a.0].prev = he_ibot_b;
    result.arena.half_edges[he_seam_a.0].loop_ = loop_cyl;

    result.arena.half_edges[he_itop_b.0].origin = v_top_seam;
    result.arena.half_edges[he_itop_b.0].next = he_seam_b;
    result.arena.half_edges[he_itop_b.0].prev = he_seam_a;
    result.arena.half_edges[he_itop_b.0].loop_ = loop_cyl;

    result.arena.half_edges[he_seam_b.0].origin = v_top_seam;
    result.arena.half_edges[he_seam_b.0].next = he_ibot_b;
    result.arena.half_edges[he_seam_b.0].prev = he_itop_b;
    result.arena.half_edges[he_seam_b.0].loop_ = loop_cyl;

    result.arena.half_edges[he_ibot_b.0].origin = v_bot_seam;
    result.arena.half_edges[he_ibot_b.0].next = he_seam_a;
    result.arena.half_edges[he_ibot_b.0].prev = he_seam_b;
    result.arena.half_edges[he_ibot_b.0].loop_ = loop_cyl;

    result.arena.loops[loop_cyl.0].half_edge = he_seam_a;

    // Set vertex half-edge refs
    result.arena.vertices[v_bot_seam.0].half_edge = Some(he_ibot_a);
    result.arena.vertices[v_top_seam.0].half_edge = Some(he_itop_a);

    // Step 6: Set face geometry for cylinder face
    // Use negative radius to signal inward-facing normals (hole surface)
    result.face_geometry.insert(
        face_cyl,
        SurfaceGeom::Cylindrical(Cylinder {
            origin: Point3::from_array(cyl.center_bottom),
            axis: Vector3::from_array(dir),
            radius: -r,
        }),
    );

    // Step 7: Set edge geometry for cylinder edges
    result.edge_geometry.insert(
        e_bot_circle,
        CurveGeom::Circular(Circle3D {
            center: Point3::from_array([cx, cy, z_min]),
            normal: Vector3::new(0.0, 0.0, 1.0),
            radius: r,
        }),
    );
    result.edge_geometry.insert(
        e_top_circle,
        CurveGeom::Circular(Circle3D {
            center: Point3::from_array([cx, cy, z_max]),
            normal: Vector3::new(0.0, 0.0, 1.0),
            radius: r,
        }),
    );
    result.edge_geometry.insert(
        e_seam,
        CurveGeom::Linear(Line3D {
            origin: Point3::from_array(bot_seam),
            direction: Vector3::from_array([0.0, 0.0, z_max - z_min]),
        }),
    );

    // Step 8: Add IDs for new entities
    result.face_map.insert(id_alloc(), face_cyl);
    result.edge_map.insert(id_alloc(), e_bot_circle);
    result.edge_map.insert(id_alloc(), e_top_circle);
    result.edge_map.insert(id_alloc(), e_seam);
    result.vertex_map.insert(id_alloc(), v_bot_seam);
    result.vertex_map.insert(id_alloc(), v_top_seam);

    Ok(result)
}

// ── Disjoint unions ────────────────────────────────────────────────────

/// Build a disjoint union of a box and a cylinder.
fn build_disjoint_box_cyl_union(
    aabb: &Aabb,
    cyl: &CylinderParams,
    id_alloc: &mut dyn FnMut() -> u64,
) -> Result<BooleanResult, KernelError> {
    // Build box as polygon faces
    let box_faces = make_box_face_polys(aabb);
    let tau_weld = 1e-7;
    let mut result = build_brep_from_polygons(&box_faces, tau_weld, id_alloc)?;

    // Build cylinder and merge into the same arena
    let cyl_result = build_cyl_result(cyl, id_alloc)?;

    // Merge the cylinder arena into the box result
    merge_brep_into(&mut result, &cyl_result, id_alloc);

    Ok(result)
}

/// Build a disjoint union of two cylinders.
fn build_disjoint_cyl_cyl_union(
    cyl_a: &CylinderParams,
    cyl_b: &CylinderParams,
    id_alloc: &mut dyn FnMut() -> u64,
) -> Result<BooleanResult, KernelError> {
    let mut result = build_cyl_result(cyl_a, id_alloc)?;
    let cyl_b_result = build_cyl_result(cyl_b, id_alloc)?;
    merge_brep_into(&mut result, &cyl_b_result, id_alloc);
    Ok(result)
}

/// Create FacePoly list for an axis-aligned box.
/// Vertex winding is CCW when viewed from the outward normal direction.
fn make_box_face_polys(aabb: &Aabb) -> Vec<FacePoly> {
    let mn = aabb.min;
    let mx = aabb.max;
    vec![
        // Bottom (z=min, normal -Z): CCW from -Z
        FacePoly {
            verts: vec![
                [mn[0], mx[1], mn[2]],
                [mx[0], mx[1], mn[2]],
                [mx[0], mn[1], mn[2]],
                [mn[0], mn[1], mn[2]],
            ],
            normal: [0.0, 0.0, -1.0],
            origin: mn,
        },
        // Top (z=max, normal +Z): CCW from +Z
        FacePoly {
            verts: vec![
                [mn[0], mn[1], mx[2]],
                [mx[0], mn[1], mx[2]],
                [mx[0], mx[1], mx[2]],
                [mn[0], mx[1], mx[2]],
            ],
            normal: [0.0, 0.0, 1.0],
            origin: [mn[0], mn[1], mx[2]],
        },
        // Front (y=min, normal -Y): CCW from -Y
        FacePoly {
            verts: vec![
                [mx[0], mn[1], mn[2]],
                [mx[0], mn[1], mx[2]],
                [mn[0], mn[1], mx[2]],
                [mn[0], mn[1], mn[2]],
            ],
            normal: [0.0, -1.0, 0.0],
            origin: [mn[0], mn[1], mn[2]],
        },
        // Back (y=max, normal +Y): CCW from +Y
        FacePoly {
            verts: vec![
                [mn[0], mx[1], mn[2]],
                [mn[0], mx[1], mx[2]],
                [mx[0], mx[1], mx[2]],
                [mx[0], mx[1], mn[2]],
            ],
            normal: [0.0, 1.0, 0.0],
            origin: [mn[0], mx[1], mn[2]],
        },
        // Right (x=max, normal +X): CCW from +X
        FacePoly {
            verts: vec![
                [mx[0], mx[1], mn[2]],
                [mx[0], mx[1], mx[2]],
                [mx[0], mn[1], mx[2]],
                [mx[0], mn[1], mn[2]],
            ],
            normal: [1.0, 0.0, 0.0],
            origin: [mx[0], mn[1], mn[2]],
        },
        // Left (x=min, normal -X): CCW from -X
        FacePoly {
            verts: vec![
                [mn[0], mn[1], mn[2]],
                [mn[0], mn[1], mx[2]],
                [mn[0], mx[1], mx[2]],
                [mn[0], mx[1], mn[2]],
            ],
            normal: [-1.0, 0.0, 0.0],
            origin: [mn[0], mn[1], mn[2]],
        },
    ]
}

/// Merge a second BooleanResult into the first (for disjoint unions).
fn merge_brep_into(
    target: &mut BooleanResult,
    source: &BooleanResult,
    id_alloc: &mut dyn FnMut() -> u64,
) {
    let v_offset = target.arena.vertices.len();
    let he_offset = target.arena.half_edges.len();
    let e_offset = target.arena.edges.len();
    let l_offset = target.arena.loops.len();
    let f_offset = target.arena.faces.len();
    let sh_offset = target.arena.shells.len();
    let so_offset = target.arena.solids.len();

    // Copy vertices with offset
    for v in &source.arena.vertices {
        let mut vc = v.clone();
        if let Some(ref mut he) = vc.half_edge {
            he.0 += he_offset;
        }
        target.arena.vertices.push(vc);
    }

    // Copy half-edges with offset
    for he in &source.arena.half_edges {
        let mut hec = he.clone();
        hec.origin.0 += v_offset;
        hec.edge.0 += e_offset;
        hec.twin.0 += he_offset;
        hec.next.0 += he_offset;
        hec.prev.0 += he_offset;
        hec.loop_.0 += l_offset;
        target.arena.half_edges.push(hec);
    }

    // Copy edges with offset
    for e in &source.arena.edges {
        let mut ec = e.clone();
        ec.half_edge.0 += he_offset;
        target.arena.edges.push(ec);
    }

    // Copy loops with offset
    for l in &source.arena.loops {
        let mut lc = l.clone();
        lc.half_edge.0 += he_offset;
        lc.face.0 += f_offset;
        target.arena.loops.push(lc);
    }

    // Copy faces with offset
    for f in &source.arena.faces {
        let mut fc = f.clone();
        fc.outer_loop.0 += l_offset;
        fc.inner_loops.iter_mut().for_each(|l| l.0 += l_offset);
        fc.shell.0 += sh_offset;
        target.arena.faces.push(fc);
    }

    // Copy shells with offset
    for s in &source.arena.shells {
        let mut sc = s.clone();
        sc.face.0 += f_offset;
        sc.solid.0 += so_offset;
        target.arena.shells.push(sc);
    }

    // Copy solids with offset
    for s in &source.arena.solids {
        let mut sc = s.clone();
        sc.outer_shell.0 += sh_offset;
        sc.inner_shells.iter_mut().for_each(|s| s.0 += sh_offset);
        target.arena.solids.push(sc);
    }

    // Copy face geometry with offset
    for (&fi, geom) in &source.face_geometry {
        target
            .face_geometry
            .insert(FaceIdx(fi.0 + f_offset), geom.clone());
    }

    // Copy edge geometry with offset
    for (&ei, geom) in &source.edge_geometry {
        target
            .edge_geometry
            .insert(EdgeIdx(ei.0 + e_offset), geom.clone());
    }

    // Add new face/edge/vertex maps with fresh IDs
    for &fi in source.face_map.values() {
        target.face_map.insert(id_alloc(), FaceIdx(fi.0 + f_offset));
    }
    for &ei in source.edge_map.values() {
        target.edge_map.insert(id_alloc(), EdgeIdx(ei.0 + e_offset));
    }
    for &vi in source.vertex_map.values() {
        target
            .vertex_map
            .insert(id_alloc(), VertexIdx(vi.0 + v_offset));
    }
}

// ── Partial cylinder-cylinder boolean ──────────────────────────────────

/// Build the result of a partial overlap cylinder-cylinder boolean.
///
/// The two cylinders share the same Z range and have 2 intersection points
/// in the XY plane. The result has 4 vertices (2 at z_min, 2 at z_max),
/// 6 edges (2 vertical lines + 4 arcs), and 4 faces (2 cylindrical + 2 planar caps).
#[allow(clippy::too_many_arguments)]
fn build_partial_cyl_cyl(
    cyl_a: &CylinderParams,
    cyl_b: &CylinderParams,
    op: BoolOp,
    p1: &[f64; 2],
    p2: &[f64; 2],
    z_min: f64,
    z_max: f64,
    id_alloc: &mut dyn FnMut() -> u64,
) -> Result<BooleanResult, KernelError> {
    // 4 vertices: 2 intersection points at z_min and z_max
    let v0_pos = [p1[0], p1[1], z_min]; // intersection point 1, bottom
    let v1_pos = [p2[0], p2[1], z_min]; // intersection point 2, bottom
    let v2_pos = [p1[0], p1[1], z_max]; // intersection point 1, top
    let v3_pos = [p2[0], p2[1], z_max]; // intersection point 2, top

    let ca = [cyl_a.center_bottom[0], cyl_a.center_bottom[1]];
    let cb = [cyl_b.center_bottom[0], cyl_b.center_bottom[1]];
    let ra = cyl_a.radius;
    let rb = cyl_b.radius;

    // Compute arc angles for each cylinder
    let angle_a1 = (p1[1] - ca[1]).atan2(p1[0] - ca[0]);
    let angle_a2 = (p2[1] - ca[1]).atan2(p2[0] - ca[0]);
    let angle_b1 = (p1[1] - cb[1]).atan2(p1[0] - cb[0]);
    let angle_b2 = (p2[1] - cb[1]).atan2(p2[0] - cb[0]);

    // "Outside" arcs: the part of each cylinder NOT inside the other
    // For cyl_a: the arc from p1 to p2 going away from cyl_b (the longer arc if partially overlapping)
    // For cyl_b: the arc from p2 to p1 going away from cyl_a

    // Determine which arc of cyl_a is "outside" cyl_b:
    // Sample the midpoint of each arc and check if it's outside cyl_b
    let sweep_a_short = normalize_angle(angle_a2 - angle_a1);
    let sweep_a_long = std::f64::consts::TAU - sweep_a_short;

    // Midpoint of short arc from p1 to p2 on cyl_a
    let mid_a_short_angle = angle_a1 + sweep_a_short / 2.0;
    let mid_a_short = [
        ca[0] + ra * mid_a_short_angle.cos(),
        ca[1] + ra * mid_a_short_angle.sin(),
    ];
    let mid_a_short_in_b =
        (mid_a_short[0] - cb[0]).powi(2) + (mid_a_short[1] - cb[1]).powi(2) < rb * rb;

    // The outside arc of A is the one NOT inside B
    let (a_outside_start, a_outside_sweep, a_inside_start, a_inside_sweep) = if mid_a_short_in_b {
        // Short arc is inside B → outside arc is the long arc (from p2 to p1, CCW)
        (angle_a2, sweep_a_long, angle_a1, sweep_a_short)
    } else {
        // Short arc is outside B → outside arc is the short arc
        (angle_a1, sweep_a_short, angle_a2, sweep_a_long)
    };

    // Same for cyl_b
    let sweep_b_short = normalize_angle(angle_b2 - angle_b1);
    let sweep_b_long = std::f64::consts::TAU - sweep_b_short;

    let mid_b_short_angle = angle_b1 + sweep_b_short / 2.0;
    let mid_b_short = [
        cb[0] + rb * mid_b_short_angle.cos(),
        cb[1] + rb * mid_b_short_angle.sin(),
    ];
    let mid_b_short_in_a =
        (mid_b_short[0] - ca[0]).powi(2) + (mid_b_short[1] - ca[1]).powi(2) < ra * ra;

    let (b_outside_start, b_outside_sweep, b_inside_start, b_inside_sweep) = if mid_b_short_in_a {
        (angle_b2, sweep_b_long, angle_b1, sweep_b_short)
    } else {
        (angle_b1, sweep_b_short, angle_b2, sweep_b_long)
    };

    // Select which arcs to use based on operation
    struct ArcSpec {
        start_angle: f64,
        sweep: f64,
    }

    let make_arc = |_c: [f64; 2], _r: f64, start: f64, sweep: f64, _origin: [f64; 3]| -> ArcSpec {
        ArcSpec {
            start_angle: start,
            sweep,
        }
    };

    // For union: A_outside + B_outside
    // For subtract: A_outside + B_inside (flipped)
    // For intersect: A_inside + B_inside
    let (arc1, arc2, flip_arc2) = match op {
        BoolOp::Union => (
            make_arc(
                ca,
                ra,
                a_outside_start,
                a_outside_sweep,
                cyl_a.center_bottom,
            ),
            make_arc(
                cb,
                rb,
                b_outside_start,
                b_outside_sweep,
                cyl_b.center_bottom,
            ),
            false,
        ),
        BoolOp::Subtract => (
            make_arc(
                ca,
                ra,
                a_outside_start,
                a_outside_sweep,
                cyl_a.center_bottom,
            ),
            make_arc(cb, rb, b_inside_start, b_inside_sweep, cyl_b.center_bottom),
            true,
        ),
        BoolOp::Intersect => (
            make_arc(ca, ra, a_inside_start, a_inside_sweep, cyl_a.center_bottom),
            make_arc(cb, rb, b_inside_start, b_inside_sweep, cyl_b.center_bottom),
            false,
        ),
    };

    // Build B-Rep: 4 vertices, 6 edges, 4 faces
    let mut arena = TopoArena::new();
    let solid_idx = arena.add_solid();
    let shell_idx = arena.add_shell(solid_idx);
    arena.solids[solid_idx.0].outer_shell = shell_idx;

    let v0 = arena.add_vertex(v0_pos);
    let v1 = arena.add_vertex(v1_pos);
    let v2 = arena.add_vertex(v2_pos);
    let v3 = arena.add_vertex(v3_pos);

    // 4 faces: cyl_a patch, cyl_b patch, top cap, bottom cap
    let face_cyl_a = arena.add_face(shell_idx);
    let face_cyl_b = arena.add_face(shell_idx);
    let face_top = arena.add_face(shell_idx);
    let face_bot = arena.add_face(shell_idx);
    arena.shells[shell_idx.0].face = face_cyl_a;

    let loop_cyl_a = arena.add_loop(face_cyl_a);
    let loop_cyl_b = arena.add_loop(face_cyl_b);
    let loop_top = arena.add_loop(face_top);
    let loop_bot = arena.add_loop(face_bot);
    arena.faces[face_cyl_a.0].outer_loop = loop_cyl_a;
    arena.faces[face_cyl_b.0].outer_loop = loop_cyl_b;
    arena.faces[face_top.0].outer_loop = loop_top;
    arena.faces[face_bot.0].outer_loop = loop_bot;

    // 6 edges: line_p1 (v0↔v2), line_p2 (v1↔v3),
    //          arc_a_bot (v0↔v1), arc_a_top (v2↔v3),
    //          arc_b_bot (v1↔v0), arc_b_top (v3↔v2)
    let (e_line_p1, he_lp1_a, he_lp1_b) = arena.add_edge(); // v0→v2 / v2→v0
    let (e_line_p2, he_lp2_a, he_lp2_b) = arena.add_edge(); // v1→v3 / v3→v1
    let (e_arc_a_bot, he_aab_a, he_aab_b) = arena.add_edge(); // arc_a at bottom: v0→v1 / v1→v0
    let (e_arc_a_top, he_aat_a, he_aat_b) = arena.add_edge(); // arc_a at top: v2→v3 / v3→v2
    let (e_arc_b_bot, he_abb_a, he_abb_b) = arena.add_edge(); // arc_b at bottom: v1→v0 / v0→v1
    let (e_arc_b_top, he_abt_a, he_abt_b) = arena.add_edge(); // arc_b at top: v3→v2 / v2→v3

    // Cyl_a patch loop: arc_a_bot(v0→v1) → line_p2(v1→v3) → arc_a_top_rev(v3→v2) → line_p1_rev(v2→v0)
    arena.half_edges[he_aab_a.0].origin = v0;
    arena.half_edges[he_aab_a.0].next = he_lp2_a;
    arena.half_edges[he_aab_a.0].prev = he_lp1_b;
    arena.half_edges[he_aab_a.0].loop_ = loop_cyl_a;

    arena.half_edges[he_lp2_a.0].origin = v1;
    arena.half_edges[he_lp2_a.0].next = he_aat_b;
    arena.half_edges[he_lp2_a.0].prev = he_aab_a;
    arena.half_edges[he_lp2_a.0].loop_ = loop_cyl_a;

    arena.half_edges[he_aat_b.0].origin = v3;
    arena.half_edges[he_aat_b.0].next = he_lp1_b;
    arena.half_edges[he_aat_b.0].prev = he_lp2_a;
    arena.half_edges[he_aat_b.0].loop_ = loop_cyl_a;

    arena.half_edges[he_lp1_b.0].origin = v2;
    arena.half_edges[he_lp1_b.0].next = he_aab_a;
    arena.half_edges[he_lp1_b.0].prev = he_aat_b;
    arena.half_edges[he_lp1_b.0].loop_ = loop_cyl_a;

    arena.loops[loop_cyl_a.0].half_edge = he_aab_a;

    // Cyl_b patch loop: arc_b_bot(v1→v0) → line_p1(v0→v2) → arc_b_top_rev(v2→v3) → line_p2_rev(v3→v1)
    // Wait, need to think about winding. For outward-facing normals:
    // If the arc_b is the "outside" arc, its normal should point outward.
    // The winding should be CCW when viewed from outside.
    // The cyl_b patch boundary goes: v1→v0 (arc_b bottom) → v0→v2 (line_p1) → v2→v3 (arc_b top) → v3→v1 (line_p2 rev)
    arena.half_edges[he_abb_a.0].origin = v1;
    arena.half_edges[he_abb_a.0].next = he_lp1_a;
    arena.half_edges[he_abb_a.0].prev = he_lp2_b;
    arena.half_edges[he_abb_a.0].loop_ = loop_cyl_b;

    arena.half_edges[he_lp1_a.0].origin = v0;
    arena.half_edges[he_lp1_a.0].next = he_abt_b;
    arena.half_edges[he_lp1_a.0].prev = he_abb_a;
    arena.half_edges[he_lp1_a.0].loop_ = loop_cyl_b;

    arena.half_edges[he_abt_b.0].origin = v2;
    arena.half_edges[he_abt_b.0].next = he_lp2_b;
    arena.half_edges[he_abt_b.0].prev = he_lp1_a;
    arena.half_edges[he_abt_b.0].loop_ = loop_cyl_b;

    arena.half_edges[he_lp2_b.0].origin = v3;
    arena.half_edges[he_lp2_b.0].next = he_abb_a;
    arena.half_edges[he_lp2_b.0].prev = he_abt_b;
    arena.half_edges[he_lp2_b.0].loop_ = loop_cyl_b;

    arena.loops[loop_cyl_b.0].half_edge = he_abb_a;

    // Bottom cap loop: arc_a_bot_rev(v1→v0) → arc_b_bot_rev(v0→v1)
    // Wait, the bottom cap is bounded by the bottom arcs from both cylinders.
    // The cap boundary goes around the 2D cross-section perimeter.
    arena.half_edges[he_aab_b.0].origin = v1;
    arena.half_edges[he_aab_b.0].next = he_abb_b;
    arena.half_edges[he_aab_b.0].prev = he_abb_b;
    arena.half_edges[he_aab_b.0].loop_ = loop_bot;

    arena.half_edges[he_abb_b.0].origin = v0;
    arena.half_edges[he_abb_b.0].next = he_aab_b;
    arena.half_edges[he_abb_b.0].prev = he_aab_b;
    arena.half_edges[he_abb_b.0].loop_ = loop_bot;

    arena.loops[loop_bot.0].half_edge = he_aab_b;

    // Top cap loop: arc_a_top(v2→v3) → arc_b_top(v3→v2)
    arena.half_edges[he_aat_a.0].origin = v2;
    arena.half_edges[he_aat_a.0].next = he_abt_a;
    arena.half_edges[he_aat_a.0].prev = he_abt_a;
    arena.half_edges[he_aat_a.0].loop_ = loop_top;

    arena.half_edges[he_abt_a.0].origin = v3;
    arena.half_edges[he_abt_a.0].next = he_aat_a;
    arena.half_edges[he_abt_a.0].prev = he_aat_a;
    arena.half_edges[he_abt_a.0].loop_ = loop_top;

    arena.loops[loop_top.0].half_edge = he_aat_a;

    // Vertex half-edge refs
    arena.vertices[v0.0].half_edge = Some(he_aab_a);
    arena.vertices[v1.0].half_edge = Some(he_abb_a);
    arena.vertices[v2.0].half_edge = Some(he_lp1_b);
    arena.vertices[v3.0].half_edge = Some(he_aat_b);

    // ── Face geometry ───────────────────────────────────────────────

    let mut face_geometry = HashMap::new();
    face_geometry.insert(
        face_cyl_a,
        SurfaceGeom::Cylindrical(Cylinder {
            origin: Point3::from_array(cyl_a.center_bottom),
            axis: Vector3::from_array(cyl_a.direction),
            radius: ra,
        }),
    );
    let cyl_b_geom = SurfaceGeom::Cylindrical(Cylinder {
        origin: Point3::from_array(cyl_b.center_bottom),
        axis: Vector3::from_array(cyl_b.direction),
        radius: if flip_arc2 { -rb } else { rb },
    });
    face_geometry.insert(face_cyl_b, cyl_b_geom);
    face_geometry.insert(
        face_bot,
        SurfaceGeom::Planar(Plane {
            origin: Point3::new(0.0, 0.0, z_min),
            normal: Vector3::new(0.0, 0.0, -1.0),
        }),
    );
    face_geometry.insert(
        face_top,
        SurfaceGeom::Planar(Plane {
            origin: Point3::new(0.0, 0.0, z_max),
            normal: Vector3::new(0.0, 0.0, 1.0),
        }),
    );

    // ── Edge geometry ───────────────────────────────────────────────

    let mut edge_geometry: HashMap<EdgeIdx, CurveGeom> = HashMap::new();

    // Vertical lines
    edge_geometry.insert(
        e_line_p1,
        CurveGeom::Linear(Line3D {
            origin: Point3::from_array(v0_pos),
            direction: Vector3::new(0.0, 0.0, z_max - z_min),
        }),
    );
    edge_geometry.insert(
        e_line_p2,
        CurveGeom::Linear(Line3D {
            origin: Point3::from_array(v1_pos),
            direction: Vector3::new(0.0, 0.0, z_max - z_min),
        }),
    );

    // Arc edges
    let make_arc_geom =
        |center_2d: [f64; 2], radius: f64, start_angle: f64, sweep: f64, z: f64| -> Arc3D {
            let sp = [
                center_2d[0] + radius * start_angle.cos(),
                center_2d[1] + radius * start_angle.sin(),
                z,
            ];
            Arc3D {
                center: Point3::new(center_2d[0], center_2d[1], z),
                normal: Vector3::new(0.0, 0.0, 1.0),
                radius,
                start_point: Point3::from_array(sp),
                sweep_angle: sweep,
            }
        };

    edge_geometry.insert(
        e_arc_a_bot,
        CurveGeom::Arc(make_arc_geom(ca, ra, arc1.start_angle, arc1.sweep, z_min)),
    );
    edge_geometry.insert(
        e_arc_a_top,
        CurveGeom::Arc(make_arc_geom(ca, ra, arc1.start_angle, arc1.sweep, z_max)),
    );
    edge_geometry.insert(
        e_arc_b_bot,
        CurveGeom::Arc(make_arc_geom(cb, rb, arc2.start_angle, arc2.sweep, z_min)),
    );
    edge_geometry.insert(
        e_arc_b_top,
        CurveGeom::Arc(make_arc_geom(cb, rb, arc2.start_angle, arc2.sweep, z_max)),
    );

    // ── Build maps ──────────────────────────────────────────────────

    let mut face_map = HashMap::new();
    let mut edge_map = HashMap::new();
    let mut vertex_map = HashMap::new();

    face_map.insert(id_alloc(), face_cyl_a);
    face_map.insert(id_alloc(), face_cyl_b);
    face_map.insert(id_alloc(), face_top);
    face_map.insert(id_alloc(), face_bot);

    edge_map.insert(id_alloc(), e_line_p1);
    edge_map.insert(id_alloc(), e_line_p2);
    edge_map.insert(id_alloc(), e_arc_a_bot);
    edge_map.insert(id_alloc(), e_arc_a_top);
    edge_map.insert(id_alloc(), e_arc_b_bot);
    edge_map.insert(id_alloc(), e_arc_b_top);

    vertex_map.insert(id_alloc(), v0);
    vertex_map.insert(id_alloc(), v1);
    vertex_map.insert(id_alloc(), v2);
    vertex_map.insert(id_alloc(), v3);

    Ok(BooleanResult {
        arena,
        face_map,
        edge_map,
        vertex_map,
        face_geometry,
        edge_geometry,
    })
}

/// Normalize an angle difference to [0, 2π).
fn normalize_angle(mut angle: f64) -> f64 {
    while angle < 0.0 {
        angle += std::f64::consts::TAU;
    }
    while angle >= std::f64::consts::TAU {
        angle -= std::f64::consts::TAU;
    }
    angle
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
