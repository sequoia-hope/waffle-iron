//! B-Rep construction from polygon soup ("stitching").
//!
//! Takes classified face polygons and builds a half-edge B-Rep topology:
//! vertex welding, face/loop/half-edge creation, twin pairing with
//! tolerance escalation, and KernelId allocation.
//!
//! **DEPRECATED (A15.6):** Progressive tolerance escalation (up to 5000×
//! tau_weld) masks upstream classification errors in the S-H clipping
//! pipeline. Will be replaced by Yang hybrid pipeline topology extraction.
//! Do NOT improve — see `specs/yang_hybrid_migration.md`.

use crate::geometry::curve::{Circle3D, CurveGeom, Line3D};
use crate::geometry::point::{Point3, Vector3};
use crate::geometry::surface::{Plane, SurfaceGeom};
use crate::topology::arena::TopoArena;
use crate::topology::half_edge::*;
use crate::types::*;
use crate::units::{TAU_COINCIDENT, TAU_NORMALIZE, TAU_PARALLEL, TAU_WORK};
use crate::vecmath::*;
use std::collections::BTreeMap;

use super::{polygon_area_3d, BooleanResult, FacePoly};

// ── B-Rep construction from polygon soup ────────────────────────────────

/// Build a complete B-Rep (arena + maps + geometry) from a list of face polygons.
///
/// Steps:
/// 1. Weld vertices by quantizing to `tau_weld` grid
/// 2. Create faces and loops with half-edges
/// 3. Pair twin half-edges to form edges
/// 4. Assign planar geometry to faces and linear geometry to edges
/// 5. Build KernelId maps for all entities
pub(super) fn build_brep_from_polygons(
    faces: &[FacePoly],
    tau_weld: f64,
    id_alloc: &mut dyn FnMut() -> u64,
) -> Result<BooleanResult, KernelError> {
    build_brep_from_polygons_inner(faces, tau_weld, false, id_alloc)
}

/// Build B-Rep from polygon soup with optional near-manifold tolerance.
///
/// When `allow_boundary` is true, allows up to 5% unpaired half-edges
/// (creates self-twin boundary edges). When false, any unpaired edges
/// produce an error.
pub(crate) fn build_brep_from_polygons_inner(
    faces: &[FacePoly],
    tau_weld: f64,
    allow_boundary: bool,
    id_alloc: &mut dyn FnMut() -> u64,
) -> Result<BooleanResult, KernelError> {
    let mut arena = TopoArena::new();
    let mut face_map = BTreeMap::new();
    let mut edge_map = BTreeMap::new();
    let mut vertex_map = BTreeMap::new();
    let mut face_geometry: BTreeMap<FaceIdx, SurfaceGeom> = BTreeMap::new();
    let mut edge_geometry: BTreeMap<EdgeIdx, CurveGeom> = BTreeMap::new();

    // Step 1: Weld vertices — quantize positions to tau_weld grid
    let inv_tau = 1.0 / tau_weld;
    let mut pos_to_vertex: BTreeMap<(i64, i64, i64), VertexIdx> = BTreeMap::new();

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
    let mut directed_he: BTreeMap<(VertexIdx, VertexIdx), HalfEdgeIdx> = BTreeMap::new();
    // Track all half-edges that need twin pairing
    let mut unpaired_hes: Vec<HalfEdgeIdx> = Vec::new();

    let mut first_face_idx = None;
    let mut face_idx_map: BTreeMap<usize, FaceIdx> = BTreeMap::new();

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
        face_idx_map.insert(fi, face_idx);
        if first_face_idx.is_none() {
            first_face_idx = Some(face_idx);
        }
        let loop_idx = arena.add_loop(face_idx);
        arena.faces[face_idx.0].outer_loop = loop_idx;

        // Assign face geometry: use tagged analytical surface when available,
        // otherwise default to planar (Ref #24 Barton: bijective re-mapping).
        let geom = face_poly
            .surface_geom
            .clone()
            .unwrap_or(SurfaceGeom::Planar(Plane {
                origin: Point3::from_array(face_poly.origin),
                normal: Vector3::from_array(face_poly.normal),
            }));
        face_geometry.insert(face_idx, geom);

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

    // Step 3b: Remove degenerate faces (area < tau_weld^2) and retry twin pairing.
    // Degenerate faces arise from near-coplanar clipping and produce unpaired half-edges.
    let tau_sq = tau_weld * tau_weld;
    let mut degenerate_faces: Vec<FaceIdx> = Vec::new();
    for (fi, face_poly) in faces.iter().enumerate() {
        if face_poly.verts.len() < 3 {
            continue;
        }
        let area = polygon_area_3d(&face_poly.verts);
        if area < tau_sq {
            // Find the FaceIdx for this face (fi-th face that was added)
            // We track face indices during creation
            if let Some(&fidx) = face_idx_map.get(&fi) {
                degenerate_faces.push(fidx);
            }
        }
    }

    if !degenerate_faces.is_empty() {
        // Collect half-edges belonging to degenerate faces
        let mut degen_hes: std::collections::HashSet<HalfEdgeIdx> =
            std::collections::HashSet::new();
        for &face_idx in &degenerate_faces {
            let loop_idx = arena.faces[face_idx.0].outer_loop;
            let start_he = arena.loops[loop_idx.0].half_edge;
            let mut he = start_he;
            loop {
                degen_hes.insert(he);
                he = arena.half_edges[he.0].next;
                if he == start_he {
                    break;
                }
            }
        }

        // Unpair any half-edges paired with degenerate face half-edges
        for &he in &degen_hes {
            if paired.contains(&he) {
                let twin = arena.half_edges[he.0].twin;
                if twin != he {
                    paired.remove(&he);
                    paired.remove(&twin);
                }
            }
        }

        // Remove degenerate half-edges from unpaired tracking
        unpaired_hes.retain(|he| !degen_hes.contains(he));

        // Retry twin pairing for newly unpaired half-edges
        for &he_idx in &unpaired_hes {
            if paired.contains(&he_idx) {
                continue;
            }
            let origin = arena.half_edges[he_idx.0].origin;
            let next_he = arena.half_edges[he_idx.0].next;
            let dest = arena.half_edges[next_he.0].origin;

            if let Some(&twin_idx) = directed_he.get(&(dest, origin)) {
                if twin_idx != he_idx
                    && !paired.contains(&twin_idx)
                    && !degen_hes.contains(&twin_idx)
                {
                    let edge_idx = EdgeIdx(arena.edges.len());
                    arena.edges.push(Edge { half_edge: he_idx });

                    arena.half_edges[he_idx.0].twin = twin_idx;
                    arena.half_edges[he_idx.0].edge = edge_idx;
                    arena.half_edges[twin_idx.0].twin = he_idx;
                    arena.half_edges[twin_idx.0].edge = edge_idx;

                    paired.insert(he_idx);
                    paired.insert(twin_idx);

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

                    let eid = id_alloc();
                    edge_map.insert(eid, edge_idx);
                }
            }
        }
    }

    // Step 3c: Position-based fallback twin pairing
    // Some edges fail to pair when same-position vertices got different indices
    // due to quantization grid boundary effects.
    {
        type QEdge = ((i64, i64, i64), (i64, i64, i64));
        let mut pos_directed_he: BTreeMap<QEdge, Vec<HalfEdgeIdx>> = BTreeMap::new();

        // Build position-based map for unpaired half-edges only
        for &he_idx in &unpaired_hes {
            if paired.contains(&he_idx) {
                continue;
            }
            let origin = arena.half_edges[he_idx.0].origin;
            let next_he = arena.half_edges[he_idx.0].next;
            let dest = arena.half_edges[next_he.0].origin;
            let origin_pos = quantize(arena.vertices[origin.0].position);
            let dest_pos = quantize(arena.vertices[dest.0].position);
            pos_directed_he
                .entry((origin_pos, dest_pos))
                .or_default()
                .push(he_idx);
        }

        // Try to pair using position keys (reverse direction lookup)
        for he_idx in unpaired_hes.clone() {
            if paired.contains(&he_idx) {
                continue;
            }
            let origin = arena.half_edges[he_idx.0].origin;
            let next_he = arena.half_edges[he_idx.0].next;
            let dest = arena.half_edges[next_he.0].origin;
            let origin_pos = quantize(arena.vertices[origin.0].position);
            let dest_pos = quantize(arena.vertices[dest.0].position);

            // Look for reverse-direction edge by position
            if let Some(candidates) = pos_directed_he.get(&(dest_pos, origin_pos)) {
                for &twin_idx in candidates {
                    if twin_idx != he_idx && !paired.contains(&twin_idx) {
                        // Pair them
                        let edge_idx = EdgeIdx(arena.edges.len());
                        arena.edges.push(Edge { half_edge: he_idx });
                        arena.half_edges[he_idx.0].twin = twin_idx;
                        arena.half_edges[he_idx.0].edge = edge_idx;
                        arena.half_edges[twin_idx.0].twin = he_idx;
                        arena.half_edges[twin_idx.0].edge = edge_idx;
                        paired.insert(he_idx);
                        paired.insert(twin_idx);

                        // Snap twin vertex positions to match so tessellation
                        // produces bit-identical f32 for shared edges.
                        let twin_origin = arena.half_edges[twin_idx.0].origin;
                        let twin_next = arena.half_edges[twin_idx.0].next;
                        let twin_dest = arena.half_edges[twin_next.0].origin;
                        if twin_origin != dest {
                            arena.vertices[twin_origin.0].position =
                                arena.vertices[dest.0].position;
                        }
                        if twin_dest != origin {
                            arena.vertices[twin_dest.0].position =
                                arena.vertices[origin.0].position;
                        }

                        // Add edge geometry
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
                        let eid = id_alloc();
                        edge_map.insert(eid, edge_idx);
                        break;
                    }
                }
            }
        }
    }

    // Step 3d: Iterative proximity-based twin pairing for remaining unpaired half-edges.
    // Independent S-H clipping produces slightly different intersection points for the
    // same geometric edge. Use progressively relaxing tolerance to pair these edges:
    // - Round 1: tight tolerance catches near-exact matches
    // - Round 2-3: looser tolerances catch larger floating-point deviations
    for &tol_mult in crate::units::STITCH_ESCALATION_FACTORS {
        let remaining_unpaired: Vec<HalfEdgeIdx> = unpaired_hes
            .iter()
            .filter(|he| !paired.contains(he))
            .copied()
            .collect();

        if remaining_unpaired.len() < 2 {
            break;
        }

        // Compute midpoints for all unpaired half-edges
        let midpoints: Vec<([f64; 3], [f64; 3], [f64; 3])> = remaining_unpaired
            .iter()
            .map(|&he| {
                let origin = arena.half_edges[he.0].origin;
                let next_he = arena.half_edges[he.0].next;
                let dest = arena.half_edges[next_he.0].origin;
                let p0 = arena.vertices[origin.0].position;
                let p1 = arena.vertices[dest.0].position;
                let mid = [
                    (p0[0] + p1[0]) * 0.5,
                    (p0[1] + p1[1]) * 0.5,
                    (p0[2] + p1[2]) * 0.5,
                ];
                (p0, p1, mid)
            })
            .collect();

        let tol = tau_weld * tol_mult;
        let tol_sq = tol * tol;

        for i in 0..remaining_unpaired.len() {
            let he_a = remaining_unpaired[i];
            if paired.contains(&he_a) {
                continue;
            }
            let (a_p0, a_p1, a_mid) = midpoints[i];

            let mut best_j = None;
            let mut best_dist = f64::INFINITY;

            for j in (i + 1)..remaining_unpaired.len() {
                let he_b = remaining_unpaired[j];
                if paired.contains(&he_b) {
                    continue;
                }
                let (b_p0, b_p1, b_mid) = midpoints[j];

                // Anti-parallel direction check: only pair edges whose
                // directions are anti-parallel (twins run opposite).
                let a_dir = v3_sub(a_p1, a_p0);
                let b_dir = v3_sub(b_p1, b_p0);
                let a_len_sq = v3_dot(a_dir, a_dir);
                let b_len_sq = v3_dot(b_dir, b_dir);
                if a_len_sq > TAU_NORMALIZE * TAU_NORMALIZE
                    && b_len_sq > TAU_NORMALIZE * TAU_NORMALIZE
                {
                    let cos_angle = v3_dot(a_dir, b_dir) / (a_len_sq.sqrt() * b_len_sq.sqrt());
                    if cos_angle > -0.5 {
                        continue; // not anti-parallel — skip
                    }
                }

                // Check reverse direction: A goes p0→p1, B should go ~p1→p0
                let fwd_dist = v3_dot(v3_sub(a_p0, b_p1), v3_sub(a_p0, b_p1))
                    + v3_dot(v3_sub(a_p1, b_p0), v3_sub(a_p1, b_p0));
                let mid_dist = v3_dot(v3_sub(a_mid, b_mid), v3_sub(a_mid, b_mid));

                if fwd_dist < tol_sq && mid_dist < tol_sq && fwd_dist < best_dist {
                    best_dist = fwd_dist;
                    best_j = Some(j);
                }
            }

            if let Some(j) = best_j {
                let he_b = remaining_unpaired[j];
                let edge_idx = EdgeIdx(arena.edges.len());
                arena.edges.push(Edge { half_edge: he_a });
                arena.half_edges[he_a.0].twin = he_b;
                arena.half_edges[he_a.0].edge = edge_idx;
                arena.half_edges[he_b.0].twin = he_a;
                arena.half_edges[he_b.0].edge = edge_idx;
                paired.insert(he_a);
                paired.insert(he_b);

                // Merge vertex positions: A goes p0→p1, B goes ~p1→~p0.
                // Snap B's vertices to A's positions so the tessellation
                // produces bit-identical f32 positions for shared edges.
                let a_origin = arena.half_edges[he_a.0].origin;
                let a_next_he = arena.half_edges[he_a.0].next;
                let a_dest = arena.half_edges[a_next_he.0].origin;
                let b_origin = arena.half_edges[he_b.0].origin;
                let b_next_he = arena.half_edges[he_b.0].next;
                let b_dest = arena.half_edges[b_next_he.0].origin;
                // B.origin ≈ A.dest (reverse direction), B.dest ≈ A.origin
                arena.vertices[b_origin.0].position = arena.vertices[a_dest.0].position;
                arena.vertices[b_dest.0].position = arena.vertices[a_origin.0].position;

                let p0 = arena.vertices[a_origin.0].position;
                let p1 = arena.vertices[a_dest.0].position;
                let dir = v3_sub(p1, p0);
                edge_geometry.insert(
                    edge_idx,
                    CurveGeom::Linear(Line3D {
                        origin: Point3::from_array(p0),
                        direction: Vector3::from_array(dir),
                    }),
                );
                let eid = id_alloc();
                edge_map.insert(eid, edge_idx);
            }
        }
    }

    // Step 4: Handle remaining unpaired half-edges.
    let unpaired_count = unpaired_hes
        .iter()
        .filter(|he| !paired.contains(he))
        .count();
    let total_count = unpaired_hes.len();

    if unpaired_count > 0 {
        let unpaired_ratio = unpaired_count as f64 / total_count.max(1) as f64;
        // Allow up to 5% unpaired in strict mode (S-H clipping creates small
        // T-junction gaps from independent floating-point intersection computation).
        // Allow up to 60% in tolerant mode (polygon approximation and fallback
        // from strict mode — tessellation hole-filling can repair small boundary gaps).
        let threshold = if allow_boundary {
            crate::units::STITCH_UNPAIRED_TOLERANT
        } else {
            crate::units::STITCH_UNPAIRED_STRICT
        };
        if unpaired_ratio > threshold {
            return Err(KernelError::BooleanFailed {
                reason: format!(
                    "non-manifold result: {} half-edges unpaired out of {} ({:.1}%)",
                    unpaired_count,
                    total_count,
                    unpaired_ratio * 100.0
                ),
            });
        }
    }

    // Create self-twin boundary edges for any remaining unpaired half-edges.
    for &he_idx in &unpaired_hes {
        if paired.contains(&he_idx) {
            continue;
        }
        let edge_idx = EdgeIdx(arena.edges.len());
        arena.edges.push(Edge { half_edge: he_idx });
        arena.half_edges[he_idx.0].edge = edge_idx;

        let origin = arena.half_edges[he_idx.0].origin;
        let next_he = arena.half_edges[he_idx.0].next;
        let dest = arena.half_edges[next_he.0].origin;
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
        let eid = id_alloc();
        edge_map.insert(eid, edge_idx);
    }

    // Step 4b: Reconstruct edge geometry from adjacent face surfaces.
    // Cylinder×Plane perpendicular → Circular edges (Patrikalakis Ch.5).
    reconstruct_edge_geometry(&arena, &face_geometry, &mut edge_geometry);

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
        cached_face_polys: None,
        cached_render_mesh: None,
    })
}

/// Post-stitch pass: reconstruct edge geometry from adjacent face surfaces.
///
/// For each edge, examines surface geometry of two adjacent faces.
/// Cylinder×Plane (perpendicular) → CurveGeom::Circular(Circle3D).
/// All other pairs: left as Linear.
///
/// Ref: Patrikalakis Ch.5 — plane-cylinder perpendicular SSI is a circle.
pub(crate) fn reconstruct_edge_geometry(
    arena: &TopoArena,
    face_geometry: &BTreeMap<FaceIdx, SurfaceGeom>,
    edge_geometry: &mut BTreeMap<EdgeIdx, CurveGeom>,
) {
    let edge_indices: Vec<EdgeIdx> = edge_geometry.keys().copied().collect();

    for edge_idx in edge_indices {
        // Edge → HalfEdge
        if edge_idx.0 >= arena.edges.len() {
            continue;
        }
        let he_a = arena.edges[edge_idx.0].half_edge;
        if he_a.0 >= arena.half_edges.len() {
            continue;
        }
        let he_b = arena.half_edges[he_a.0].twin;

        // Skip self-twin boundary edges (only one adjacent face)
        if he_a == he_b {
            continue;
        }
        if he_b.0 >= arena.half_edges.len() {
            continue;
        }

        // HalfEdge → Loop → Face
        let loop_a = arena.half_edges[he_a.0].loop_;
        let loop_b = arena.half_edges[he_b.0].loop_;
        if loop_a.0 >= arena.loops.len() || loop_b.0 >= arena.loops.len() {
            continue;
        }
        let face_a = arena.loops[loop_a.0].face;
        let face_b = arena.loops[loop_b.0].face;

        let (geom_a, geom_b) = match (face_geometry.get(&face_a), face_geometry.get(&face_b)) {
            (Some(a), Some(b)) => (a, b),
            _ => continue,
        };

        // Try to reconstruct a circular or elliptical edge from Cylinder×Plane pair
        if let Some(circle) = try_cyl_plane_circle(geom_a, geom_b) {
            // Validate: check edge endpoints lie on the circle
            let origin_v = arena.half_edges[he_a.0].origin;
            let next_he = arena.half_edges[he_a.0].next;
            let dest_v = if next_he.0 < arena.half_edges.len() {
                arena.half_edges[next_he.0].origin
            } else {
                continue;
            };

            let p0 = arena.vertices[origin_v.0].position;
            let p1 = arena.vertices[dest_v.0].position;
            let center = circle.center.to_array();

            let dist0 = ((p0[0] - center[0]).powi(2)
                + (p0[1] - center[1]).powi(2)
                + (p0[2] - center[2]).powi(2))
            .sqrt();
            let dist1 = ((p1[0] - center[0]).powi(2)
                + (p1[1] - center[1]).powi(2)
                + (p1[2] - center[2]).powi(2))
            .sqrt();

            // Both endpoints must be within tolerance of the circle radius
            if (dist0 - circle.radius).abs() < TAU_COINCIDENT
                && (dist1 - circle.radius).abs() < TAU_COINCIDENT
            {
                edge_geometry.insert(edge_idx, CurveGeom::Circular(circle));
            }
        } else if let Some(ellipse) = try_cyl_plane_ellipse(geom_a, geom_b) {
            // Oblique plane-cylinder: validate endpoints lie on the ellipse
            let origin_v = arena.half_edges[he_a.0].origin;
            let next_he = arena.half_edges[he_a.0].next;
            let dest_v = if next_he.0 < arena.half_edges.len() {
                arena.half_edges[next_he.0].origin
            } else {
                continue;
            };

            let p0 = arena.vertices[origin_v.0].position;
            let p1 = arena.vertices[dest_v.0].position;
            let center = [ellipse.center.x, ellipse.center.y, ellipse.center.z];
            let major = [
                ellipse.major_axis.x,
                ellipse.major_axis.y,
                ellipse.major_axis.z,
            ];
            let normal = [ellipse.normal.x, ellipse.normal.y, ellipse.normal.z];
            let minor = v3_cross(
                [normal[0], normal[1], normal[2]],
                [major[0], major[1], major[2]],
            );

            // Check that both endpoints are approximately on the ellipse
            let on_ellipse = |pt: [f64; 3]| -> bool {
                let d = v3_sub(pt, center);
                let u = v3_dot(d, major) / ellipse.semi_major;
                let v = v3_dot(d, minor) / ellipse.semi_minor;
                let r2 = u * u + v * v;
                (r2 - 1.0).abs() < crate::units::ELLIPSE_ON_CURVE_TOL
            };

            if on_ellipse(p0) && on_ellipse(p1) {
                edge_geometry.insert(
                    edge_idx,
                    CurveGeom::Elliptical(crate::geometry::curve::Ellipse3D {
                        center: ellipse.center,
                        normal: ellipse.normal,
                        major_axis: ellipse.major_axis,
                        semi_major: ellipse.semi_major,
                        semi_minor: ellipse.semi_minor,
                    }),
                );
            }
        }
    }
}

/// Intermediate type for oblique plane-cylinder ellipse data.
struct EllipseData {
    center: Point3,
    normal: Vector3,
    major_axis: Vector3,
    semi_major: f64,
    semi_minor: f64,
}

/// Try to construct an ellipse from a Cylinder×Plane oblique intersection.
///
/// Returns `Some(EllipseData)` if one surface is cylindrical and the other planar,
/// and the plane is oblique (neither perpendicular nor parallel) to the cylinder axis.
fn try_cyl_plane_ellipse(a: &SurfaceGeom, b: &SurfaceGeom) -> Option<EllipseData> {
    let (plane, cyl) = match (a, b) {
        (SurfaceGeom::Cylindrical(c), SurfaceGeom::Planar(p)) => (p, c),
        (SurfaceGeom::Planar(p), SurfaceGeom::Cylindrical(c)) => (p, c),
        _ => return None,
    };

    let n = plane.normal.to_array();
    let ax = cyl.axis.to_array();
    let dot_wn = n[0] * ax[0] + n[1] * ax[1] + n[2] * ax[2];

    // Must be oblique: not perpendicular (dot ≈ ±1) and not parallel (dot ≈ 0)
    let cos_angle = dot_wn.abs();
    if !(TAU_PARALLEL..=1.0 - TAU_PARALLEL).contains(&cos_angle) {
        return None;
    }

    let sin_gamma = (1.0 - cos_angle * cos_angle).max(0.0).sqrt();
    let r = cyl.radius.abs();
    let semi_minor = r;
    let semi_major = r / sin_gamma;

    // Major axis: projection of cylinder axis onto cutting plane
    let proj = v3_sub(ax, v3_scale(n, dot_wn));
    let proj_len = v3_length(proj);
    if proj_len < TAU_WORK {
        return None;
    }
    let major_axis = v3_scale(proj, 1.0 / proj_len);

    // Center: where cylinder axis pierces the plane
    let co = cyl.origin.to_array();
    let po = plane.origin.to_array();
    if dot_wn.abs() < TAU_WORK {
        return None;
    }
    let t = v3_dot(v3_sub(po, co), n) / dot_wn;
    let center = v3_add(co, v3_scale(ax, t));

    Some(EllipseData {
        center: Point3::from_array(center),
        normal: plane.normal,
        major_axis: Vector3::from_array(major_axis),
        semi_major,
        semi_minor,
    })
}

/// Try to construct a Circle3D from a Cylinder×Plane perpendicular intersection.
///
/// Returns `Some(Circle3D)` if one surface is cylindrical and the other planar,
/// and the plane normal is parallel to the cylinder axis (perpendicular cut).
fn try_cyl_plane_circle(a: &SurfaceGeom, b: &SurfaceGeom) -> Option<Circle3D> {
    let (plane, cyl) = match (a, b) {
        (SurfaceGeom::Cylindrical(c), SurfaceGeom::Planar(p)) => (p, c),
        (SurfaceGeom::Planar(p), SurfaceGeom::Cylindrical(c)) => (p, c),
        _ => return None,
    };

    // Check perpendicularity: plane normal parallel to cylinder axis
    let n = plane.normal.to_array();
    let ax = cyl.axis.to_array();
    let dot = n[0] * ax[0] + n[1] * ax[1] + n[2] * ax[2];
    if dot.abs() < 1.0 - TAU_PARALLEL {
        return None; // Oblique — intersection is an ellipse
    }

    // Project cylinder origin onto the plane to get circle center
    let co = cyl.origin.to_array();
    let po = plane.origin.to_array();
    let d = (co[0] - po[0]) * n[0] + (co[1] - po[1]) * n[1] + (co[2] - po[2]) * n[2];
    let center = Point3::new(co[0] - d * n[0], co[1] - d * n[1], co[2] - d * n[2]);

    Some(Circle3D {
        center,
        normal: plane.normal,
        radius: cyl.radius,
    })
}
