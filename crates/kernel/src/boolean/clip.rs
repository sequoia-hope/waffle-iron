//! Sutherland-Hodgman polygon clipping, intersection caching, coplanarity
//! classification, and pre-processing (dedup, merge, resolve T-junctions).

use crate::units::{
    TAU_CACHE_STEP_FACTOR, TAU_NORMALIZE, TAU_PARALLEL, TAU_SNAP_FACTOR, TAU_TESS_GRID_FACTOR,
    TAU_TESS_GRID_MIN,
};
use crate::vecmath::*;
use std::collections::HashMap;

use super::{polygon_area_3d, FacePoly};

// ── Intersection cache for cross-face deduplication ──────────────────────

/// Cache for intersection points computed during Sutherland-Hodgman clipping.
///
/// When two adjacent faces share a geometric edge (A,B), canonical ordering
/// (Step 1) ensures identical operand order. But the faces may store slightly
/// different copies of A and B (from prior operations). The cache ensures the
/// first computation wins and all subsequent lookups return the exact same
/// `[f64; 3]`, eliminating unpaired edges in tessellation.
///
/// Key: (sorted quantized edge endpoints, quantized plane normal+offset).
/// Quantization at tau * 1e-3 — coarse enough to match "same" geometric edges,
/// fine enough to distinguish genuinely different edges.
///
/// Ref [#9] Cherchi 2020: indirect predicates — same principle of avoiding
/// recomputation. Ref [#10] Levy 2025: exact constructions cached per-edge.
pub(super) struct IntersectionCache {
    cache: HashMap<([i64; 6], [i64; 4]), [f64; 3]>,
    inv_quant: f64,
}

impl IntersectionCache {
    pub(super) fn new(tau: f64) -> Self {
        let step = (tau * TAU_CACHE_STEP_FACTOR).max(TAU_NORMALIZE);
        Self {
            cache: HashMap::new(),
            inv_quant: 1.0 / step,
        }
    }

    /// Look up or insert an intersection point.
    ///
    /// Edge endpoints `a` and `b` are sorted lexicographically, then quantized
    /// to form a cache key together with the plane definition. Multi-probe
    /// lookup (floor/ceil in each quantized dimension) catches near-boundary
    /// cases where the same geometric edge quantizes to adjacent cells.
    pub(super) fn get_or_insert(
        &mut self,
        a: [f64; 3],
        b: [f64; 3],
        plane_pt: [f64; 3],
        plane_n: [f64; 3],
        computed: [f64; 3],
    ) -> [f64; 3] {
        let iq = self.inv_quant;

        // Sort endpoints lexicographically for canonical key
        let (lo, hi) = if (a[0], a[1], a[2]) < (b[0], b[1], b[2]) {
            (a, b)
        } else {
            (b, a)
        };

        let q = |v: f64| -> i64 { (v * iq).round() as i64 };

        let edge_key = [q(lo[0]), q(lo[1]), q(lo[2]), q(hi[0]), q(hi[1]), q(hi[2])];

        // Quantize plane by normal direction + signed distance from origin
        let plane_d =
            plane_n[0] * plane_pt[0] + plane_n[1] * plane_pt[1] + plane_n[2] * plane_pt[2];
        let plane_key = [
            q(plane_n[0] * 1e3),
            q(plane_n[1] * 1e3),
            q(plane_n[2] * 1e3),
            q(plane_d),
        ];

        let primary = (edge_key, plane_key);

        // Multi-probe: check floor/ceil variations of the edge key
        // to handle near-boundary quantization
        let floor_ceil = |v: f64| -> [i64; 2] {
            let s = v * iq;
            [s.floor() as i64, s.ceil() as i64]
        };

        let lx = floor_ceil(lo[0]);
        let ly = floor_ceil(lo[1]);
        let lz = floor_ceil(lo[2]);
        let hx = floor_ceil(hi[0]);
        let hy = floor_ceil(hi[1]);
        let hz = floor_ceil(hi[2]);

        // Gap B: also probe floor/ceil of plane_d
        let pd = floor_ceil(plane_d);

        // Gap A fix: probe combined (lo_variation, hi_variation) pairs
        // with plane_d multi-probing (Gap B)
        for &lxi in &lx {
            for &lyi in &ly {
                for &lzi in &lz {
                    for &hxi in &hx {
                        for &hyi in &hy {
                            for &hzi in &hz {
                                for &pdi in &pd {
                                    let probe_edge = [lxi, lyi, lzi, hxi, hyi, hzi];
                                    let probe_plane =
                                        [plane_key[0], plane_key[1], plane_key[2], pdi];
                                    if let Some(&cached) =
                                        self.cache.get(&(probe_edge, probe_plane))
                                    {
                                        return cached;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // No match — insert computed value
        self.cache.insert(primary, computed);
        computed
    }
}

// ── Sutherland-Hodgman polygon clipping ─────────────────────────────────

/// Snap a coordinate to a grid to eliminate floating-point divergence.
///
/// When adjacent faces share a geometric edge and are clipped by the same
/// plane, the intersection coordinates differ by ~1e-15 due to floating-point
/// non-associativity. Snapping to a fine grid collapses this divergence so
/// that both faces get identical intersection coordinates.
#[inline]
fn snap_to_grid(val: f64, inv_grid: f64) -> f64 {
    (val * inv_grid).round() / inv_grid
}

/// Clip a polygon to keep only the portion on the INWARD side of a plane.
/// Points where `dot(p - plane_point, inward_normal) >= -tau` are kept.
///
/// Intersection points are snapped to a fine grid (tau_weld-scale) to ensure
/// that adjacent faces sharing a geometric edge get identical intersection
/// coordinates when clipped by the same plane.
#[cfg(test)]
pub(super) fn clip_polygon_by_plane(
    verts: &[[f64; 3]],
    plane_point: [f64; 3],
    inward_normal: [f64; 3],
    tau: f64,
) -> Vec<[f64; 3]> {
    clip_polygon_by_plane_cached(verts, plane_point, inward_normal, tau, None)
}

/// Clip a polygon by a plane with optional intersection cache.
///
/// When `cache` is Some, intersection points are deduplicated across faces:
/// if the same geometric edge (identified by quantized endpoints) was already
/// clipped by the same plane, the cached result is returned instead of
/// recomputing, eliminating floating-point divergence between adjacent faces.
pub(super) fn clip_polygon_by_plane_cached(
    verts: &[[f64; 3]],
    plane_point: [f64; 3],
    inward_normal: [f64; 3],
    tau: f64,
    cache: Option<&mut IntersectionCache>,
) -> Vec<[f64; 3]> {
    if verts.is_empty() {
        return vec![];
    }

    let mut output = Vec::with_capacity(verts.len() + 1);

    let dist = |p: [f64; 3]| -> f64 { v3_dot(v3_sub(p, plane_point), inward_normal) };

    // Snap grid for intersection points: fine enough to be geometrically
    // insignificant, coarse enough to collapse ~1e-15 floating-point divergence.
    // Use tau * 1e-4 — for unit-scale models this is ~1e-11, well below any
    // geometric significance but well above machine epsilon (~2.2e-16).
    let snap_grid = tau * TAU_SNAP_FACTOR;
    let inv_grid = if snap_grid > 0.0 {
        1.0 / snap_grid
    } else {
        0.0
    };

    // Canonical intersection computation: sort edge endpoints lexicographically
    // so that adjacent faces sharing a geometric edge (A,B) vs (B,A) compute
    // I = Lo + t*(Hi-Lo) with identical operand order, producing bitwise-identical
    // results. Ref [#4] Shewchuk: deterministic evaluation order.
    let canonical_intersection = |a: [f64; 3], b: [f64; 3], d_a: f64, d_b: f64| -> [f64; 3] {
        // Lexicographic comparison: x first, then y, then z
        let a_is_lo = (a[0], a[1], a[2]) < (b[0], b[1], b[2]);
        let (lo, hi, d_lo, d_hi) = if a_is_lo {
            (a, b, d_a, d_b)
        } else {
            (b, a, d_b, d_a)
        };
        let t = d_lo / (d_lo - d_hi);
        let mut intersection = v3_add(lo, v3_scale(v3_sub(hi, lo), t));
        if inv_grid > 0.0 {
            intersection[0] = snap_to_grid(intersection[0], inv_grid);
            intersection[1] = snap_to_grid(intersection[1], inv_grid);
            intersection[2] = snap_to_grid(intersection[2], inv_grid);
        }
        intersection
    };

    // We need to use the cache in both closure captures, so collect
    // all intersections to cache-process after the loop.
    // Instead, we avoid the borrow issue by collecting into a separate vec
    // and caching at the end. Actually, let's just process inline.
    let n = verts.len();

    // Collect raw intersections first, then cache them
    struct PendingIntersection {
        insert_pos: usize, // position in output where this goes
        a: [f64; 3],
        b: [f64; 3],
        computed: [f64; 3],
    }
    let mut pending: Vec<PendingIntersection> = Vec::new();

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
                let computed = canonical_intersection(current, next, d_current, d_next);
                let pos = output.len();
                output.push(computed);
                pending.push(PendingIntersection {
                    insert_pos: pos,
                    a: current,
                    b: next,
                    computed,
                });
            }
        } else if next_inside {
            let computed = canonical_intersection(current, next, d_current, d_next);
            let pos = output.len();
            output.push(computed);
            pending.push(PendingIntersection {
                insert_pos: pos,
                a: current,
                b: next,
                computed,
            });
        }
    }

    // Apply cache deduplication: replace computed values with cached ones
    if let Some(cache) = cache {
        for p in &pending {
            let cached = cache.get_or_insert(p.a, p.b, plane_point, inward_normal, p.computed);
            output[p.insert_pos] = cached;
        }
    }

    output
}

/// Coplanarity classification between two face planes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum CoplanarClass {
    NotCoplanar,
    SameDirection,
    AntiParallel,
}

/// Classify whether a face is coplanar with an opposing face, and if so,
/// whether their normals are parallel or anti-parallel.
pub(super) fn classify_coplanarity(
    face_normal: [f64; 3],
    face_point: [f64; 3],
    opp: &FacePoly,
    tau: f64,
) -> CoplanarClass {
    let dot_n = v3_dot(face_normal, opp.normal);
    if dot_n.abs() > 1.0 - TAU_PARALLEL {
        let dist = v3_dot(v3_sub(face_point, opp.origin), opp.normal).abs();
        if dist < tau * 100.0 {
            if dot_n > 0.0 {
                CoplanarClass::SameDirection
            } else {
                CoplanarClass::AntiParallel
            }
        } else {
            CoplanarClass::NotCoplanar
        }
    } else {
        CoplanarClass::NotCoplanar
    }
}

/// Check if a face polygon is coplanar with an opposing face.
pub(super) fn is_coplanar(
    face_normal: [f64; 3],
    face_point: [f64; 3],
    opp: &FacePoly,
    tau: f64,
) -> bool {
    classify_coplanarity(face_normal, face_point, opp, tau) != CoplanarClass::NotCoplanar
}

/// Clip a polygon by a convex solid's interior (intersection of inward half-spaces).
/// For a convex solid, each face's inward normal is the NEGATION of its outward normal.
///
/// If `face_normal` is provided, skip opposing faces that are coplanar with the
/// polygon being clipped. Two faces are coplanar when their normals are parallel
/// (or anti-parallel) and a vertex of the polygon lies on the opposing face's plane.
pub(super) fn clip_polygon_by_solid(
    verts: &[[f64; 3]],
    opposing_faces: &[FacePoly],
    tau: f64,
    face_normal: Option<[f64; 3]>,
    cache: &mut Option<IntersectionCache>,
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
        current = clip_polygon_by_plane_cached(&current, face.origin, inward, tau, cache.as_mut());
    }
    current
}

/// Test if a point is inside a closed polyhedral solid.
/// Casts a ray in +Z direction, counts face crossings. Odd = inside.
///
/// Check if a set of face polygons forms a convex solid.
///
/// A solid is convex if every vertex of every face lies on or behind (inside)
/// every face plane. Uses signed distance: positive = outside, negative = inside.
/// Capped at 200 faces to avoid O(V*F) blowup for large face sets.
#[allow(dead_code)]
pub(super) fn is_face_set_convex(faces: &[FacePoly], tau: f64) -> bool {
    // Quick heuristic: very small or very large face sets
    if faces.len() <= 6 {
        return true; // Box or simpler — always convex
    }
    if faces.len() > 200 {
        return false; // Too many faces to check efficiently
    }

    // Collect all unique vertices
    let mut all_verts: Vec<[f64; 3]> = Vec::new();
    for f in faces {
        for &v in &f.verts {
            // Simple dedup: skip if already present (exact match)
            let exists = all_verts.iter().any(|&ev| {
                let d = v3_sub(v, ev);
                v3_dot(d, d) < tau * tau
            });
            if !exists {
                all_verts.push(v);
            }
        }
    }

    // Check: every vertex is on or behind every face plane
    for face in faces {
        let n = face.normal;
        let o = face.origin;
        for &v in &all_verts {
            let d = v3_dot(v3_sub(v, o), n);
            if d > tau * 100.0 {
                return false; // Vertex is significantly outside this face plane
            }
        }
    }

    true
}

// ── Pre-processing: dedup, merge, resolve T-junctions ───────────────────

/// Merge nearby vertices across all polygons using multi-probe spatial hashing.
///
/// When independent Sutherland-Hodgman clips produce slightly different coordinates
/// for the same geometric point (e.g., two adjacent faces clipped by the same plane),
/// this function maps them to the same canonical representative vertex.
///
/// Only vertices within `merge_tol` of an existing canonical vertex are merged.
/// Remove near-duplicate face polygons from the boolean result.
///
/// When face classification produces overlapping fragments (e.g., a face
/// partially inside the opposing solid is emitted twice from different
/// classification paths), the resulting mesh has non-manifold edges.
/// This function identifies fragments with nearly identical centroids,
/// normals, and vertex counts, and keeps only one copy.
pub(super) fn dedup_face_polys(polys: &[FacePoly], tau_weld: f64) -> Vec<FacePoly> {
    if polys.len() < 2 {
        return polys.to_vec();
    }

    let tol_sq = (tau_weld * 10.0) * (tau_weld * 10.0);

    let centroid = |p: &FacePoly| -> [f64; 3] {
        let n = p.verts.len() as f64;
        if n == 0.0 {
            return [0.0; 3];
        }
        let mut c = [0.0; 3];
        for v in &p.verts {
            c[0] += v[0];
            c[1] += v[1];
            c[2] += v[2];
        }
        c[0] /= n;
        c[1] /= n;
        c[2] /= n;
        c
    };

    let mut keep = vec![true; polys.len()];

    for i in 0..polys.len() {
        if !keep[i] {
            continue;
        }
        let ci = centroid(&polys[i]);
        let ni = polys[i].normal;
        let ai = polygon_area_3d(&polys[i].verts);

        for j in (i + 1)..polys.len() {
            if !keep[j] {
                continue;
            }
            // Same vertex count?
            if polys[i].verts.len() != polys[j].verts.len() {
                continue;
            }
            // Nearly parallel normals?
            let dot = v3_dot(ni, polys[j].normal);
            if dot.abs() < 0.99 {
                continue;
            }
            // Nearly same area?
            let aj = polygon_area_3d(&polys[j].verts);
            if ai > 0.0 && (aj - ai).abs() / ai > 0.01 {
                continue;
            }
            // Nearly same centroid?
            let cj = centroid(&polys[j]);
            let dx = ci[0] - cj[0];
            let dy = ci[1] - cj[1];
            let dz = ci[2] - cj[2];
            if dx * dx + dy * dy + dz * dz < tol_sq {
                keep[j] = false;
            }
        }
    }

    polys
        .iter()
        .enumerate()
        .filter(|(i, _)| keep[*i])
        .map(|(_, p)| p.clone())
        .collect()
}

/// This avoids moving isolated vertices and preserves original face geometry.
#[allow(dead_code)]
pub(super) fn merge_nearby_vertices(polys: &[FacePoly], tau_weld: f64) -> Vec<FacePoly> {
    // Compute merge tolerance directly from vertex coordinates to align with
    // the oracle's f32 quantization grid (max_abs * 1e-5). Use 2x grid to
    // ensure vertices within one oracle grid cell always merge.
    let max_coord = polys
        .iter()
        .flat_map(|f| f.verts.iter())
        .flat_map(|v| v.iter())
        .map(|c| c.abs())
        .fold(0.0_f64, f64::max);
    let oracle_grid = (max_coord * TAU_TESS_GRID_FACTOR).max(TAU_TESS_GRID_MIN);
    let merge_tol = (oracle_grid * 2.0).max(tau_weld * 10.0);
    let inv_tau = 1.0 / merge_tol;
    let weld_dist_sq = merge_tol * merge_tol;

    // Build canonical vertex map: each vertex gets mapped to its representative
    let mut canonical: HashMap<(i64, i64, i64), [f64; 3]> = HashMap::new();

    let find_or_insert =
        |pos: [f64; 3], canonical: &mut HashMap<(i64, i64, i64), [f64; 3]>| -> [f64; 3] {
            let sx = pos[0] * inv_tau;
            let sy = pos[1] * inv_tau;
            let sz = pos[2] * inv_tau;
            let primary = (sx.round() as i64, sy.round() as i64, sz.round() as i64);

            // Multi-probe: check floor/ceil in each axis
            let kx = [sx.floor() as i64, sx.ceil() as i64];
            let ky = [sy.floor() as i64, sy.ceil() as i64];
            let kz = [sz.floor() as i64, sz.ceil() as i64];

            for &cx in &kx {
                for &cy in &ky {
                    for &cz in &kz {
                        if let Some(&rep) = canonical.get(&(cx, cy, cz)) {
                            let dx = pos[0] - rep[0];
                            let dy = pos[1] - rep[1];
                            let dz = pos[2] - rep[2];
                            if dx * dx + dy * dy + dz * dz < weld_dist_sq {
                                return rep; // Use existing canonical vertex
                            }
                        }
                    }
                }
            }

            // No match found — this vertex becomes its own canonical representative
            canonical.insert(primary, pos);
            pos
        };

    let mut result = Vec::with_capacity(polys.len());
    for poly in polys {
        let merged: Vec<[f64; 3]> = poly
            .verts
            .iter()
            .map(|&v| find_or_insert(v, &mut canonical))
            .collect();

        // Deduplicate consecutive vertices
        let mut deduped: Vec<[f64; 3]> = Vec::with_capacity(merged.len());
        for i in 0..merged.len() {
            let prev = if i == 0 { merged.len() - 1 } else { i - 1 };
            if merged[i] != merged[prev] {
                deduped.push(merged[i]);
            }
        }

        if deduped.len() >= 3 {
            result.push(FacePoly {
                verts: deduped,
                normal: poly.normal,
                origin: poly.origin,
                surface_geom: poly.surface_geom.clone(),
            });
        }
    }
    result
}

/// Resolve T-junctions in a polygon soup.
///
/// When boolean classification splits some faces but not others, adjacent
/// faces can have mismatched edges: one face has a long edge from A→C, while
/// an adjacent face introduces a vertex B between A and C. This creates a
/// T-junction that makes edge pairing impossible.
///
/// This function detects and resolves T-junctions by:
/// 1. Collecting all vertices from all polygons
/// 2. For each face edge, checking if any vertex from other faces lies on the
///    edge interior (within tolerance)
/// 3. Inserting those vertices into the edge, splitting it
pub(super) fn resolve_t_junctions(polys: &[FacePoly], tau: f64) -> Vec<FacePoly> {
    // Collect all unique vertices (quantized for lookup)
    let inv_tau = 1.0 / tau;
    let quantize = |p: [f64; 3]| -> (i64, i64, i64) {
        (
            (p[0] * inv_tau).round() as i64,
            (p[1] * inv_tau).round() as i64,
            (p[2] * inv_tau).round() as i64,
        )
    };

    // Build set of all vertices across all polygons
    let mut all_verts: Vec<[f64; 3]> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for poly in polys {
        for &v in &poly.verts {
            let key = quantize(v);
            if seen.insert(key) {
                all_verts.push(v);
            }
        }
    }

    // For each polygon, check each edge for T-junction vertices
    let mut result = Vec::with_capacity(polys.len());
    for poly in polys {
        let n = poly.verts.len();
        if n < 3 {
            result.push(poly.clone());
            continue;
        }

        let mut new_verts: Vec<[f64; 3]> = Vec::new();
        for i in 0..n {
            let a = poly.verts[i];
            let b = poly.verts[(i + 1) % n];
            new_verts.push(a);

            // Find vertices that lie strictly on the interior of edge A→B
            let edge_vec = v3_sub(b, a);
            let edge_len_sq = v3_dot(edge_vec, edge_vec);
            if edge_len_sq < tau * tau {
                continue; // degenerate edge
            }

            // Collect candidate split points with their parametric position
            let mut splits: Vec<(f64, [f64; 3])> = Vec::new();
            let a_key = quantize(a);
            let b_key = quantize(b);

            for &v in &all_verts {
                let v_key = quantize(v);
                // Skip edge endpoints
                if v_key == a_key || v_key == b_key {
                    continue;
                }

                // Check if v lies on the line segment A→B
                let av = v3_sub(v, a);
                let t = v3_dot(av, edge_vec) / edge_len_sq;
                if t <= tau || t >= 1.0 - tau {
                    continue; // not in interior
                }

                // Check distance from the line (relative to edge length).
                // S-H clipping divergence between adjacent faces can be much
                // larger than tau, so use generous tolerances.
                let proj = v3_add(a, v3_scale(edge_vec, t));
                let diff = v3_sub(v, proj);
                let dist_sq = v3_dot(diff, diff);
                let abs_tol = tau * 1000.0; // ~tau_weld for S-H divergence
                let rel_tol_sq = edge_len_sq * TAU_SNAP_FACTOR * TAU_SNAP_FACTOR;
                if dist_sq < abs_tol * abs_tol || dist_sq < rel_tol_sq {
                    // Use the original vertex position (not projection) to
                    // maintain consistency with the face that owns this vertex.
                    splits.push((t, v));
                }
            }

            // Sort splits by parametric position and insert
            splits.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
            for (_, split_pt) in splits {
                new_verts.push(split_pt);
            }
        }

        result.push(FacePoly {
            verts: new_verts,
            normal: poly.normal,
            origin: poly.origin,
            surface_geom: poly.surface_geom.clone(),
        });
    }

    result
}
