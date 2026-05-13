//! Mesh repair and cleanup functions for tessellation output.
//!
//! These functions fix winding consistency, remove degenerate/duplicate triangles,
//! resolve non-manifold topology, fill boundary holes, and perform T-junction
//! resolution. They are applied as a post-processing pipeline in `tessellate_solid_ext`.
//!
//! ## Deprecation notice (A15.6)
//!
//! Several functions in this module (`fill_boundary_holes`, `close_near_boundary_chains`)
//! are part of the deprecated S-H clipping repair pipeline. They mask classification
//! errors rather than solving them and will be removed when the Yang hybrid pipeline
//! is operational. Do NOT invest in improving these paths.

use crate::geometry::surface::SurfaceGeom;
use crate::topology::arena::TopoArena;
use crate::topology::half_edge::{EdgeIdx, FaceIdx};
use crate::types::{FaceRange, KernelId};
use crate::units::{
    COS_HOLE_COHERENCE, HOLE_CIRCULARITY_CV, HOLE_FILL_COHERENCE_MIN_EDGES, HOLE_PLANARITY_RATIO,
    TAU_TESS_GRID_FACTOR, TAU_TESS_GRID_MIN, TAU_WORK,
};
use crate::vecmath::{compute_plane_basis, v3_cross, v3_dot, v3_length, v3_sub};
use std::collections::{BTreeMap, HashSet};

use super::{collect_loop_boundary, EdgeDiscretization};

/// Fix winding consistency: for each triangle, compute the geometric normal
/// from the cross product of its edges and compare against the average of
/// its stored vertex normals. If they disagree (dot < 0), swap two indices
/// to flip the winding order.
pub(super) fn fix_winding_consistency(vertices: &[f32], normals: &[f32], indices: &mut [u32]) {
    let num_tris = indices.len() / 3;
    for t in 0..num_tris {
        let i0 = indices[t * 3] as usize;
        let i1 = indices[t * 3 + 1] as usize;
        let i2 = indices[t * 3 + 2] as usize;

        // Get vertex positions
        let v0 = [
            vertices[i0 * 3] as f64,
            vertices[i0 * 3 + 1] as f64,
            vertices[i0 * 3 + 2] as f64,
        ];
        let v1 = [
            vertices[i1 * 3] as f64,
            vertices[i1 * 3 + 1] as f64,
            vertices[i1 * 3 + 2] as f64,
        ];
        let v2 = [
            vertices[i2 * 3] as f64,
            vertices[i2 * 3 + 1] as f64,
            vertices[i2 * 3 + 2] as f64,
        ];

        // Geometric normal from cross product of edges
        let e1 = v3_sub(v1, v0);
        let e2 = v3_sub(v2, v0);
        let geo_normal = v3_cross(e1, e2);

        // Skip degenerate triangles
        let geo_len = v3_length(geo_normal);
        if geo_len < TAU_WORK {
            continue;
        }

        // Average stored vertex normal
        let avg_n = [
            (normals[i0 * 3] + normals[i1 * 3] + normals[i2 * 3]) as f64 / 3.0,
            (normals[i0 * 3 + 1] + normals[i1 * 3 + 1] + normals[i2 * 3 + 1]) as f64 / 3.0,
            (normals[i0 * 3 + 2] + normals[i1 * 3 + 2] + normals[i2 * 3 + 2]) as f64 / 3.0,
        ];

        // If geometric normal disagrees with stored normal, flip winding
        if v3_dot(geo_normal, avg_n) < 0.0 {
            indices.swap(t * 3 + 1, t * 3 + 2);
        }
    }
}

/// Count unpaired edges using oracle-compatible quantization grid.
pub(super) fn count_unpaired_in_mesh(vertices: &[f32], indices: &[u32]) -> usize {
    if vertices.is_empty() || indices.is_empty() {
        return 0;
    }
    let n_verts = vertices.len() / 3;
    let n_tris = indices.len() / 3;
    let max_abs = vertices.iter().map(|v| v.abs()).fold(0.0_f32, f32::max);
    let grid = (max_abs as f64 * TAU_TESS_GRID_FACTOR).max(TAU_TESS_GRID_MIN);
    let inv_grid = 1.0 / grid;
    type QPos = (i64, i64, i64);
    let quantize = |idx: u32| -> QPos {
        let i = idx as usize;
        if i >= n_verts {
            return (0, 0, 0);
        }
        (
            (vertices[i * 3] as f64 * inv_grid).round() as i64,
            (vertices[i * 3 + 1] as f64 * inv_grid).round() as i64,
            (vertices[i * 3 + 2] as f64 * inv_grid).round() as i64,
        )
    };
    let make_edge = |a: QPos, b: QPos| -> (QPos, QPos) {
        if a <= b {
            (a, b)
        } else {
            (b, a)
        }
    };
    let mut edge_counts: BTreeMap<(QPos, QPos), usize> = BTreeMap::new();
    for t in 0..n_tris {
        let base = t * 3;
        let tri = [indices[base], indices[base + 1], indices[base + 2]];
        let qt = [quantize(tri[0]), quantize(tri[1]), quantize(tri[2])];
        if qt[0] == qt[1] || qt[1] == qt[2] || qt[0] == qt[2] {
            continue;
        }
        for e in 0..3 {
            *edge_counts
                .entry(make_edge(qt[e], qt[(e + 1) % 3]))
                .or_insert(0) += 1;
        }
    }
    edge_counts.values().filter(|&&c| c != 2).count()
}

/// Weld boundary vertices that are close enough to match in the oracle grid.
///
/// The boolean pipeline can produce seam vertices that are very close but
/// not exactly coincident, causing oracle edge matching to report "unpaired"
/// edges. This function identifies boundary (unpaired-edge) vertices, then
/// uses union-find to cluster those within distance `grid * 1.5` of each
/// other. Each cluster is replaced by its centroid, ensuring all seam
/// vertices match in the oracle quantization.
pub(super) fn weld_boundary_vertices(vertices: &mut [f32], indices: &[u32]) {
    weld_boundary_vertices_with_scale(vertices, indices, 5.0);
}

/// Progressive boundary vertex welding with configurable scale factor.
///
/// Clusters boundary vertices (endpoints of unpaired edges) within
/// `scale_factor × grid` distance and snaps each cluster to its centroid.
/// Higher scale factors capture larger S-H clipping divergences but risk
/// merging genuinely distinct vertices. Used in the convergence loop at
/// progressively increasing scales (5, 10, 20, 40).
pub(super) fn weld_boundary_vertices_with_scale(
    vertices: &mut [f32],
    indices: &[u32],
    scale_factor: f64,
) {
    if vertices.is_empty() || indices.is_empty() {
        return;
    }
    let n_verts = vertices.len() / 3;
    let n_tris = indices.len() / 3;

    let max_abs = vertices.iter().map(|v| v.abs()).fold(0.0_f32, f32::max);
    let grid = (max_abs as f64 * TAU_TESS_GRID_FACTOR).max(TAU_TESS_GRID_MIN);
    let inv_grid = 1.0 / grid;

    // Quantize helper
    let quantize = |idx: u32| -> (i64, i64, i64) {
        let i = idx as usize;
        if i >= n_verts {
            return (0, 0, 0);
        }
        let x = (vertices[i * 3] as f64 * inv_grid).round() as i64;
        let y = (vertices[i * 3 + 1] as f64 * inv_grid).round() as i64;
        let z = (vertices[i * 3 + 2] as f64 * inv_grid).round() as i64;
        (x, y, z)
    };

    // Build undirected edge counts
    type QPos = (i64, i64, i64);
    let mut edge_counts: BTreeMap<(QPos, QPos), usize> = BTreeMap::new();
    let make_edge = |a: QPos, b: QPos| -> (QPos, QPos) {
        if a <= b {
            (a, b)
        } else {
            (b, a)
        }
    };

    for t in 0..n_tris {
        let base = t * 3;
        let tri = [indices[base], indices[base + 1], indices[base + 2]];
        let qt = [quantize(tri[0]), quantize(tri[1]), quantize(tri[2])];
        for e in 0..3 {
            let edge = make_edge(qt[e], qt[(e + 1) % 3]);
            if edge.0 != edge.1 {
                *edge_counts.entry(edge).or_insert(0) += 1;
            }
        }
    }

    // Collect boundary vertex indices (endpoints of unpaired edges)
    let mut boundary_verts: Vec<u32> = Vec::new();
    let mut is_boundary: HashSet<u32> = HashSet::new();
    for t in 0..n_tris {
        let base = t * 3;
        let tri = [indices[base], indices[base + 1], indices[base + 2]];
        let qt = [quantize(tri[0]), quantize(tri[1]), quantize(tri[2])];
        for e in 0..3 {
            let edge = make_edge(qt[e], qt[(e + 1) % 3]);
            if edge.0 != edge.1 {
                if let Some(&count) = edge_counts.get(&edge) {
                    if count != 2 {
                        for &vi in &[tri[e], tri[(e + 1) % 3]] {
                            if is_boundary.insert(vi) {
                                boundary_verts.push(vi);
                            }
                        }
                    }
                }
            }
        }
    }

    if boundary_verts.is_empty() {
        return;
    }

    // Union-find for clustering close boundary vertices
    let n = boundary_verts.len();
    let mut parent: Vec<usize> = (0..n).collect();

    fn find(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }

    fn union(parent: &mut [usize], a: usize, b: usize) {
        let ra = find(parent, a);
        let rb = find(parent, b);
        if ra != rb {
            parent[rb] = ra;
        }
    }

    // Weld threshold uses configurable scale factor to capture near-miss seam
    // vertices that diverge due to Sutherland-Hodgman clipping at intersection
    // boundaries. Progressive scales (5→10→20→40) catch divergences at
    // different magnitudes without over-welding in a single pass.
    let weld_dist_sq = (grid * scale_factor) * (grid * scale_factor);

    // O(N²) pairwise check — N is small (boundary vertices only)
    for (i, &bv_i) in boundary_verts.iter().enumerate() {
        let ai = bv_i as usize;
        let ax = vertices[ai * 3] as f64;
        let ay = vertices[ai * 3 + 1] as f64;
        let az = vertices[ai * 3 + 2] as f64;
        for (j, &bv_j) in boundary_verts.iter().enumerate().skip(i + 1) {
            let bj = bv_j as usize;
            let bx = vertices[bj * 3] as f64;
            let by = vertices[bj * 3 + 1] as f64;
            let bz = vertices[bj * 3 + 2] as f64;
            let dx = ax - bx;
            let dy = ay - by;
            let dz = az - bz;
            if dx * dx + dy * dy + dz * dz < weld_dist_sq {
                union(&mut parent, i, j);
            }
        }
    }

    // Compute centroid for each cluster and assign
    let mut cluster_sum: BTreeMap<usize, [f64; 3]> = BTreeMap::new();
    let mut cluster_count: BTreeMap<usize, usize> = BTreeMap::new();

    for (i, &bv_i) in boundary_verts.iter().enumerate() {
        let root = find(&mut parent, i);
        let vi = bv_i as usize;
        let entry = cluster_sum.entry(root).or_insert([0.0; 3]);
        entry[0] += vertices[vi * 3] as f64;
        entry[1] += vertices[vi * 3 + 1] as f64;
        entry[2] += vertices[vi * 3 + 2] as f64;
        *cluster_count.entry(root).or_insert(0) += 1;
    }

    // Only weld clusters with >1 vertex (actual merges)
    for (i, &bv_i) in boundary_verts.iter().enumerate() {
        let root = find(&mut parent, i);
        let count = cluster_count[&root];
        if count <= 1 {
            continue;
        }
        let sum = cluster_sum[&root];
        let vi = bv_i as usize;
        vertices[vi * 3] = (sum[0] / count as f64) as f32;
        vertices[vi * 3 + 1] = (sum[1] / count as f64) as f32;
        vertices[vi * 3 + 2] = (sum[2] / count as f64) as f32;
    }
}

/// If the mesh signed volume is negative, the entire solid is inside-out.
/// Flip all triangle windings and negate all normals to fix orientation.
pub(super) fn fix_global_orientation(
    vertices: &mut [f32],
    normals: &mut [f32],
    indices: &mut [u32],
) {
    let num_tris = indices.len() / 3;
    if num_tris == 0 {
        return;
    }

    // Compute signed volume using divergence theorem
    let mut vol = 0.0f64;
    for t in 0..num_tris {
        let i0 = indices[t * 3] as usize;
        let i1 = indices[t * 3 + 1] as usize;
        let i2 = indices[t * 3 + 2] as usize;
        let v0 = [
            vertices[i0 * 3] as f64,
            vertices[i0 * 3 + 1] as f64,
            vertices[i0 * 3 + 2] as f64,
        ];
        let v1 = [
            vertices[i1 * 3] as f64,
            vertices[i1 * 3 + 1] as f64,
            vertices[i1 * 3 + 2] as f64,
        ];
        let v2 = [
            vertices[i2 * 3] as f64,
            vertices[i2 * 3 + 1] as f64,
            vertices[i2 * 3 + 2] as f64,
        ];
        vol += v0[0] * (v1[1] * v2[2] - v1[2] * v2[1])
            + v1[0] * (v2[1] * v0[2] - v2[2] * v0[1])
            + v2[0] * (v0[1] * v1[2] - v0[2] * v1[1]);
    }
    vol /= 6.0;

    if vol < 0.0 {
        // Flip all triangle windings
        for t in 0..num_tris {
            indices.swap(t * 3 + 1, t * 3 + 2);
        }
        // Negate all normals
        for n in normals.iter_mut() {
            *n = -*n;
        }
        // Also flip the unused vertices' normals? No — only normals array matters
    }
}

pub(super) fn remove_degenerate_triangles(
    vertices: &[f32],
    indices: &mut Vec<u32>,
    face_ranges: &mut Vec<FaceRange>,
) {
    let mut new_indices = Vec::with_capacity(indices.len());
    let mut new_ranges = Vec::new();

    for range in face_ranges.iter() {
        let range_start = new_indices.len() as u32;
        let tri_start = range.start_index as usize / 3;
        let tri_end = range.end_index as usize / 3;

        for t in tri_start..tri_end {
            let base = t * 3;
            if base + 2 >= indices.len() {
                break;
            }
            let i0 = indices[base] as usize;
            let i1 = indices[base + 1] as usize;
            let i2 = indices[base + 2] as usize;

            if i0 * 3 + 2 >= vertices.len()
                || i1 * 3 + 2 >= vertices.len()
                || i2 * 3 + 2 >= vertices.len()
            {
                continue;
            }

            // Match oracle computation exactly: f32 arithmetic, area = |cross|/2
            let ax = vertices[i1 * 3] - vertices[i0 * 3];
            let ay = vertices[i1 * 3 + 1] - vertices[i0 * 3 + 1];
            let az = vertices[i1 * 3 + 2] - vertices[i0 * 3 + 2];
            let bx = vertices[i2 * 3] - vertices[i0 * 3];
            let by = vertices[i2 * 3 + 1] - vertices[i0 * 3 + 1];
            let bz = vertices[i2 * 3 + 2] - vertices[i0 * 3 + 2];
            let cx = ay * bz - az * by;
            let cy = az * bx - ax * bz;
            let cz = ax * by - ay * bx;
            let area = (cx * cx + cy * cy + cz * cz).sqrt() / 2.0;

            // Keep non-degenerate triangles (matches oracle threshold)
            if area >= TAU_WORK as f32 {
                new_indices.push(indices[base]);
                new_indices.push(indices[base + 1]);
                new_indices.push(indices[base + 2]);
            }
        }

        let range_end = new_indices.len() as u32;
        if range_end > range_start {
            new_ranges.push(FaceRange {
                face_id: range.face_id,
                start_index: range_start,
                end_index: range_end,
            });
        }
    }

    *indices = new_indices;
    *face_ranges = new_ranges;
}

/// Remove exact duplicate triangles (same winding, same quantized positions).
///
/// When the boolean produces duplicate face fragments (same 3 vertices in
/// same cyclic order), keep only one copy. This is conservative — it only
/// removes triangles that are exact duplicates, not triangles that merely
/// share edges.
pub(super) fn remove_duplicate_triangles(
    vertices: &[f32],
    indices: &mut Vec<u32>,
    face_ranges: &mut Vec<FaceRange>,
) {
    let n_tris = indices.len() / 3;
    if n_tris < 2 {
        return;
    }

    let max_abs = vertices.iter().map(|v| v.abs()).fold(0.0_f32, f32::max);
    let grid = (max_abs as f64 * TAU_TESS_GRID_FACTOR).max(TAU_TESS_GRID_MIN);
    let inv_grid = 1.0 / grid;

    type QPos = (i64, i64, i64);
    let quantize = |idx: u32| -> QPos {
        let i = idx as usize * 3;
        if i + 2 >= vertices.len() {
            return (0, 0, 0);
        }
        (
            (vertices[i] as f64 * inv_grid).round() as i64,
            (vertices[i + 1] as f64 * inv_grid).round() as i64,
            (vertices[i + 2] as f64 * inv_grid).round() as i64,
        )
    };

    // Canonical form: rotate to minimum vertex, preserving winding direction.
    let tri_key = |a: QPos, b: QPos, c: QPos| -> [QPos; 3] {
        if a <= b && a <= c {
            [a, b, c]
        } else if b <= a && b <= c {
            [b, c, a]
        } else {
            [c, a, b]
        }
    };

    let mut seen: HashSet<[QPos; 3]> = HashSet::new();
    let mut new_indices = Vec::with_capacity(indices.len());
    let mut new_ranges = Vec::new();

    for range in face_ranges.iter() {
        let range_start = new_indices.len() as u32;
        let tri_start = range.start_index as usize / 3;
        let tri_end = range.end_index as usize / 3;

        for t in tri_start..tri_end {
            let base = t * 3;
            if base + 2 >= indices.len() {
                break;
            }
            let qa = quantize(indices[base]);
            let qb = quantize(indices[base + 1]);
            let qc = quantize(indices[base + 2]);
            let key = tri_key(qa, qb, qc);

            if seen.insert(key) {
                new_indices.push(indices[base]);
                new_indices.push(indices[base + 1]);
                new_indices.push(indices[base + 2]);
            }
        }

        let range_end = new_indices.len() as u32;
        if range_end > range_start {
            new_ranges.push(FaceRange {
                face_id: range.face_id,
                start_index: range_start,
                end_index: range_end,
            });
        }
    }

    *indices = new_indices;
    *face_ranges = new_ranges;
}

/// Remove winding-insensitive duplicate triangles.
///
/// Two triangles with the same 3 quantized vertex positions (regardless of
/// winding order) are duplicates. The first occurrence is kept; subsequent
/// occurrences are removed. This catches opposite-winding duplicates that
/// `remove_duplicate_triangles` (winding-sensitive) misses.
pub(super) fn remove_winding_insensitive_duplicates(
    vertices: &[f32],
    indices: &mut Vec<u32>,
    face_ranges: &mut Vec<FaceRange>,
) {
    let n_tris = indices.len() / 3;
    if n_tris < 2 {
        return;
    }

    let max_abs = vertices.iter().map(|v| v.abs()).fold(0.0_f32, f32::max);
    let grid = (max_abs as f64 * TAU_TESS_GRID_FACTOR).max(TAU_TESS_GRID_MIN);
    let inv_grid = 1.0 / grid;

    type QPos = (i64, i64, i64);
    let quantize = |idx: u32| -> QPos {
        let i = idx as usize * 3;
        if i + 2 >= vertices.len() {
            return (0, 0, 0);
        }
        (
            (vertices[i] as f64 * inv_grid).round() as i64,
            (vertices[i + 1] as f64 * inv_grid).round() as i64,
            (vertices[i + 2] as f64 * inv_grid).round() as i64,
        )
    };

    // Canonical form: sort the 3 vertices (winding-insensitive).
    let tri_key = |a: QPos, b: QPos, c: QPos| -> [QPos; 3] {
        let mut arr = [a, b, c];
        arr.sort();
        arr
    };

    let mut seen: HashSet<[QPos; 3]> = HashSet::new();
    let mut new_indices = Vec::with_capacity(indices.len());
    let mut new_ranges = Vec::new();

    // ── PR-Y40 INFRA: canonical-key collision probe (env-gated, default-off) ──
    // Tracks each canonical-key insertion: when a collision occurs, records
    // (winner_face_id, winner_tri_offset, loser_face_id, loser_tri_offset, key).
    // Default-off path is byte-identical: `seen` HashSet drives behavior; the
    // probe maintains a PARALLEL `first_seen` HashMap only when enabled.
    let y40_enabled = y40_collision_probe_enabled();
    let mut y40_first_seen: std::collections::HashMap<[QPos; 3], Y40FirstSeen> =
        std::collections::HashMap::new();
    let mut y40_collisions: Vec<Y40Collision> = Vec::new();

    for (range_idx, range) in face_ranges.iter().enumerate() {
        let range_start = new_indices.len() as u32;
        let tri_start = range.start_index as usize / 3;
        let tri_end = range.end_index as usize / 3;

        for t in tri_start..tri_end {
            let base = t * 3;
            if base + 2 >= indices.len() {
                break;
            }
            let qa = quantize(indices[base]);
            let qb = quantize(indices[base + 1]);
            let qc = quantize(indices[base + 2]);
            let key = tri_key(qa, qb, qc);

            if seen.insert(key) {
                new_indices.push(indices[base]);
                new_indices.push(indices[base + 1]);
                new_indices.push(indices[base + 2]);
                if y40_enabled {
                    y40_first_seen.insert(
                        key,
                        Y40FirstSeen {
                            face_id: range.face_id.0,
                            range_idx,
                            tri_offset: t - tri_start,
                        },
                    );
                }
            } else if y40_enabled {
                let winner = y40_first_seen
                    .get(&key)
                    .copied()
                    .unwrap_or(Y40FirstSeen {
                        face_id: u64::MAX,
                        range_idx: usize::MAX,
                        tri_offset: usize::MAX,
                    });
                y40_collisions.push(Y40Collision {
                    key,
                    winner,
                    loser: Y40FirstSeen {
                        face_id: range.face_id.0,
                        range_idx,
                        tri_offset: t - tri_start,
                    },
                });
            }
        }

        let range_end = new_indices.len() as u32;
        if range_end > range_start {
            new_ranges.push(FaceRange {
                face_id: range.face_id,
                start_index: range_start,
                end_index: range_end,
            });
        }
    }

    if y40_enabled {
        y40_write_collisions(&y40_collisions, n_tris);
    }

    *indices = new_indices;
    *face_ranges = new_ranges;
}

// ── PR-Y40 INFRA probe types + helpers (env-gated, default-off) ──

#[derive(Debug, Clone, Copy)]
struct Y40FirstSeen {
    face_id: u64,
    range_idx: usize,
    tri_offset: usize,
}

#[derive(Debug, Clone, Copy)]
struct Y40Collision {
    key: [(i64, i64, i64); 3],
    winner: Y40FirstSeen,
    loser: Y40FirstSeen,
}

fn y40_collision_probe_enabled() -> bool {
    std::env::var("Y40_COLLISION_PROBE").as_deref() == Ok("1")
}

std::thread_local! {
    static Y40_INVOCATION_COUNTER: std::cell::RefCell<u64> =
        const { std::cell::RefCell::new(0) };
}

fn y40_next_invocation() -> u64 {
    Y40_INVOCATION_COUNTER.with(|c| {
        let mut n = c.borrow_mut();
        *n += 1;
        *n
    })
}

fn y40_write_collisions(collisions: &[Y40Collision], n_tris_input: usize) {
    let invocation = y40_next_invocation();
    let dump_dir = match std::env::var("Y40_COLLISION_PROBE_DIR") {
        Ok(d) => d,
        Err(_) => return,
    };
    let _ = std::fs::create_dir_all(&dump_dir);

    let case = crate::boolean::yang_integration::current_case_id()
        .unwrap_or_else(|| format!("seq_{}", std::process::id()));

    // Per-invocation collisions TSV
    let coll_path = std::path::PathBuf::from(&dump_dir)
        .join(format!("{}_inv{:03}_collisions.tsv", case, invocation));
    let mut out = String::new();
    out.push_str("collision_idx\tkey_xa\tkey_ya\tkey_za\tkey_xb\tkey_yb\tkey_zb\tkey_xc\tkey_yc\tkey_zc\twinner_face_id\twinner_range_idx\twinner_tri_off\tloser_face_id\tloser_range_idx\tloser_tri_off\n");
    for (i, c) in collisions.iter().enumerate() {
        let [k0, k1, k2] = c.key;
        out.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            i,
            k0.0, k0.1, k0.2,
            k1.0, k1.1, k1.2,
            k2.0, k2.1, k2.2,
            c.winner.face_id, c.winner.range_idx, c.winner.tri_offset,
            c.loser.face_id, c.loser.range_idx, c.loser.tri_offset,
        ));
    }
    let _ = std::fs::write(&coll_path, out);

    // Per-invocation summary
    use std::collections::BTreeMap;
    let mut distinct_winners: std::collections::HashSet<u64> = std::collections::HashSet::new();
    let mut distinct_losers: std::collections::HashSet<u64> = std::collections::HashSet::new();
    let mut pair_counts: BTreeMap<(u64, u64), usize> = BTreeMap::new();
    let mut loser_counts: BTreeMap<u64, usize> = BTreeMap::new();
    let mut winner_counts: BTreeMap<u64, usize> = BTreeMap::new();
    for c in collisions {
        distinct_winners.insert(c.winner.face_id);
        distinct_losers.insert(c.loser.face_id);
        *pair_counts.entry((c.winner.face_id, c.loser.face_id)).or_insert(0) += 1;
        *loser_counts.entry(c.loser.face_id).or_insert(0) += 1;
        *winner_counts.entry(c.winner.face_id).or_insert(0) += 1;
    }

    let hist_path = std::path::PathBuf::from(&dump_dir)
        .join(format!("{}_inv{:03}_histogram.tsv", case, invocation));
    let mut hist = String::new();
    hist.push_str("winner_face_id\tloser_face_id\tcount\n");
    for ((w, l), n) in &pair_counts {
        hist.push_str(&format!("{}\t{}\t{}\n", w, l, n));
    }
    let _ = std::fs::write(&hist_path, hist);

    let summary_path = std::path::PathBuf::from(&dump_dir)
        .join(format!("{}_inv{:03}_summary.tsv", case, invocation));
    let mut summary = String::new();
    summary.push_str("metric\tvalue\n");
    summary.push_str(&format!("invocation\t{}\n", invocation));
    summary.push_str(&format!("n_tris_input\t{}\n", n_tris_input));
    summary.push_str(&format!("total_collisions\t{}\n", collisions.len()));
    summary.push_str(&format!("distinct_winner_face_ids\t{}\n", distinct_winners.len()));
    summary.push_str(&format!("distinct_loser_face_ids\t{}\n", distinct_losers.len()));
    summary.push_str("\nloser_face_id\tcount\n");
    for (l, n) in &loser_counts {
        summary.push_str(&format!("{}\t{}\n", l, n));
    }
    summary.push_str("\nwinner_face_id\tcount\n");
    for (w, n) in &winner_counts {
        summary.push_str(&format!("{}\t{}\n", w, n));
    }
    let _ = std::fs::write(&summary_path, summary);
}

/// Core non-manifold removal logic shared by both aggressive and conservative modes.
///
/// Topology-aware non-manifold edge repair for the bounded tessellation path.
///
/// Uses B-Rep topology (half-edge twin relationships) to determine which two
/// faces should share each boundary edge. For non-manifold mesh edges (3+
/// triangles sharing), triangles whose face_id is NOT one of the two expected
/// faces are removed first. Falls through to the aggressive heuristic for
/// interior edges not in the edge discretization.
pub(super) fn remove_nonmanifold_topology_aware(
    arena: &TopoArena,
    face_map: &BTreeMap<u64, FaceIdx>,
    disc: &EdgeDiscretization,
    vertices: &[f32],
    indices: &mut Vec<u32>,
    face_ranges: &mut Vec<FaceRange>,
) {
    let n_tris = indices.len() / 3;
    if n_tris < 2 {
        return;
    }

    // Build quantization grid matching the test oracle exactly.
    let max_abs = vertices.iter().map(|v| v.abs()).fold(0.0_f32, f32::max);
    let grid = (max_abs as f64 * TAU_TESS_GRID_FACTOR).max(TAU_TESS_GRID_MIN);
    let inv_grid = 1.0 / grid;

    type QPos = (i64, i64, i64);
    let quantize_f32 = |idx: u32| -> QPos {
        let i = idx as usize * 3;
        if i + 2 >= vertices.len() {
            return (0, 0, 0);
        }
        (
            (vertices[i] as f64 * inv_grid).round() as i64,
            (vertices[i + 1] as f64 * inv_grid).round() as i64,
            (vertices[i + 2] as f64 * inv_grid).round() as i64,
        )
    };
    let quantize_f64 = |pos: &[f64; 3]| -> QPos {
        (
            (pos[0] * inv_grid).round() as i64,
            (pos[1] * inv_grid).round() as i64,
            (pos[2] * inv_grid).round() as i64,
        )
    };

    // Step 1: Build reverse map from FaceIdx → KernelId (u64).
    let mut face_idx_to_kid: BTreeMap<FaceIdx, u64> = BTreeMap::new();
    for (&kid, &fidx) in face_map {
        face_idx_to_kid.insert(fidx, kid);
    }

    // Step 2: Build edge→(KernelId, KernelId) map from B-Rep topology.
    // For each edge, find its two adjacent faces via half-edge twins.
    // Then map the edge's discretized vertex positions to quantized mesh edges.
    type UEdge = (QPos, QPos);
    let mut topo_edge_faces: BTreeMap<UEdge, HashSet<u64>> = BTreeMap::new();

    for (i, edge) in arena.edges.iter().enumerate() {
        let edge_idx = EdgeIdx(i);
        let he_a = edge.half_edge;
        // PR-Y20-MODE-A: NMM (twin=None) — skip edges without paired twin
        // (cannot record both adjacent faces for the unpaired direction).
        let he_b = match arena.half_edges[he_a.0].twin {
            Some(t) => t,
            None => continue,
        };
        let loop_a = arena.half_edges[he_a.0].loop_;
        let loop_b = arena.half_edges[he_b.0].loop_;
        let face_a = arena.loops[loop_a.0].face;
        let face_b = arena.loops[loop_b.0].face;

        let kid_a = face_idx_to_kid.get(&face_a).copied();
        let kid_b = face_idx_to_kid.get(&face_b).copied();

        // Get discretized vertices for this edge
        if let Some(verts) = disc.edge_verts.get(&edge_idx) {
            // Create quantized mesh edge keys for each consecutive pair of
            // discretized vertices along this edge.
            for pair in verts.windows(2) {
                let qa = quantize_f64(&disc.positions[pair[0]]);
                let qb = quantize_f64(&disc.positions[pair[1]]);
                let key: UEdge = if qa <= qb { (qa, qb) } else { (qb, qa) };
                let entry = topo_edge_faces.entry(key).or_default();
                if let Some(ka) = kid_a {
                    entry.insert(ka);
                }
                if let Some(kb) = kid_b {
                    entry.insert(kb);
                }
            }
            // For full-circle edges (closed loops), also connect last→first
            if verts.len() >= 3 {
                let qa = quantize_f64(&disc.positions[*verts.last().unwrap()]);
                let qb = quantize_f64(&disc.positions[verts[0]]);
                if qa != qb {
                    let key: UEdge = if qa <= qb { (qa, qb) } else { (qb, qa) };
                    let entry = topo_edge_faces.entry(key).or_default();
                    if let Some(ka) = kid_a {
                        entry.insert(ka);
                    }
                    if let Some(kb) = kid_b {
                        entry.insert(kb);
                    }
                }
            }
        }
    }

    // Step 3: Build tri→face_id mapping from face_ranges.
    let mut tri_face_id: Vec<u64> = vec![0; n_tris];
    for range in face_ranges.iter() {
        let tri_start = range.start_index as usize / 3;
        let tri_end = range.end_index as usize / 3;
        for item in tri_face_id
            .iter_mut()
            .take(tri_end.min(n_tris))
            .skip(tri_start)
        {
            *item = range.face_id.0;
        }
    }

    // Step 4: Build edge → triangle list for mesh edges.
    let mut edge_tris: BTreeMap<UEdge, Vec<usize>> = BTreeMap::new();
    for t in 0..n_tris {
        let base = t * 3;
        let tri = [indices[base], indices[base + 1], indices[base + 2]];
        for j in 0..3 {
            let pa = quantize_f32(tri[j]);
            let pb = quantize_f32(tri[(j + 1) % 3]);
            let key: UEdge = if pa <= pb { (pa, pb) } else { (pb, pa) };
            edge_tris.entry(key).or_default().push(t);
        }
    }

    // Step 5: For non-manifold edges, use topology info to remove wrong-face triangles.
    let mut remove_set: HashSet<usize> = HashSet::new();

    // Collect and sort non-manifold edges for determinism.
    let mut nm_edges: Vec<(UEdge, Vec<usize>)> = edge_tris
        .iter()
        .filter(|(_, tris)| tris.len() >= 3)
        .map(|(e, t)| (*e, t.clone()))
        .collect();
    nm_edges.sort_by_key(|(edge, _)| *edge);

    for (edge_key, tris) in &nm_edges {
        let live: Vec<usize> = tris
            .iter()
            .copied()
            .filter(|t| !remove_set.contains(t))
            .collect();
        if live.len() <= 2 {
            continue;
        }

        // Look up expected faces from B-Rep topology
        if let Some(expected_faces) = topo_edge_faces.get(edge_key) {
            if expected_faces.is_empty() {
                continue; // No topology info, fall through to aggressive
            }

            // Partition triangles into "expected" (face_id in expected set) and "extra"
            let mut expected: Vec<usize> = Vec::new();
            let mut extra: Vec<usize> = Vec::new();
            for &t in &live {
                if expected_faces.contains(&tri_face_id[t]) {
                    expected.push(t);
                } else {
                    extra.push(t);
                }
            }

            // If removing all extras would leave >=2 triangles, do it
            if expected.len() >= 2 {
                for &t in &extra {
                    remove_set.insert(t);
                }
                // If still more than 2 expected, remove smallest-area extras
                // among expected (same face appearing multiple times)
                if expected.len() > 2 {
                    // Sort by area ascending, keep the 2 largest
                    expected.sort_by(|&a, &b| {
                        let area_a = tri_area_f32(vertices, indices, a);
                        let area_b = tri_area_f32(vertices, indices, b);
                        area_a
                            .partial_cmp(&area_b)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                    for &t in &expected[..(expected.len() - 2)] {
                        remove_set.insert(t);
                    }
                }
            } else if expected.len() == 1 && !extra.is_empty() {
                // Keep the 1 expected + the largest extra
                extra.sort_by(|&a, &b| {
                    let area_a = tri_area_f32(vertices, indices, a);
                    let area_b = tri_area_f32(vertices, indices, b);
                    area_b
                        .partial_cmp(&area_a)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                // Remove all but the first (largest) extra
                for &t in &extra[1..] {
                    remove_set.insert(t);
                }
            }
            // If expected.len() == 0, all triangles are "extra" — don't remove
            // blindly, fall through to aggressive.
        }
    }

    if !remove_set.is_empty() {
        // Rebuild indices and face_ranges, skipping removed triangles.
        let mut new_indices = Vec::with_capacity(indices.len());
        let mut new_ranges = Vec::new();

        for range in face_ranges.iter() {
            let range_start = new_indices.len() as u32;
            let tri_start = range.start_index as usize / 3;
            let tri_end = range.end_index as usize / 3;

            for t in tri_start..tri_end.min(n_tris) {
                if remove_set.contains(&t) {
                    continue;
                }
                let base = t * 3;
                new_indices.push(indices[base]);
                new_indices.push(indices[base + 1]);
                new_indices.push(indices[base + 2]);
            }

            let range_end = new_indices.len() as u32;
            if range_end > range_start {
                new_ranges.push(FaceRange {
                    face_id: range.face_id,
                    start_index: range_start,
                    end_index: range_end,
                });
            }
        }

        *indices = new_indices;
        *face_ranges = new_ranges;
    }
}

/// Flip non-manifold interior diagonals to resolve earcut conflicts.
///
/// When two faces share corner vertex positions without a B-Rep boundary edge,
/// earcut may create the same interior diagonal in both faces, producing 3+
/// triangles per edge. This function identifies such diagonals and flips the
/// diagonal in one face (replacing 2 triangles with 2 using the alternative
/// diagonal) to eliminate the non-manifold condition without removing triangles.
///
/// Research basis: Edge flipping is a fundamental Delaunay refinement operation
/// [Shewchuk 1997]. Applied selectively to interior diagonals only.
pub(super) fn flip_nonmanifold_interior_diagonals(
    _arena: &TopoArena,
    _face_map: &BTreeMap<u64, FaceIdx>,
    disc: &EdgeDiscretization,
    vertices: &[f32],
    indices: &mut [u32],
    face_ranges: &mut [FaceRange],
) {
    let max_iterations = 10;

    for _iteration in 0..max_iterations {
        let n_tris = indices.len() / 3;
        if n_tris < 3 {
            return;
        }

        // Build quantization grid matching the existing pipeline.
        let max_abs = vertices.iter().map(|v| v.abs()).fold(0.0_f32, f32::max);
        let grid = (max_abs as f64 * TAU_TESS_GRID_FACTOR).max(TAU_TESS_GRID_MIN);
        let inv_grid = 1.0 / grid;

        type QPos = (i64, i64, i64);
        let quantize = |idx: u32| -> QPos {
            let i = idx as usize * 3;
            if i + 2 >= vertices.len() {
                return (0, 0, 0);
            }
            (
                (vertices[i] as f64 * inv_grid).round() as i64,
                (vertices[i + 1] as f64 * inv_grid).round() as i64,
                (vertices[i + 2] as f64 * inv_grid).round() as i64,
            )
        };
        let quantize_f64 = |pos: &[f64; 3]| -> QPos {
            (
                (pos[0] * inv_grid).round() as i64,
                (pos[1] * inv_grid).round() as i64,
                (pos[2] * inv_grid).round() as i64,
            )
        };

        // Build B-Rep boundary edge set from discretization.
        type UEdge = (QPos, QPos);
        let mut brep_edges: HashSet<UEdge> = HashSet::new();
        for verts in disc.edge_verts.values() {
            for pair in verts.windows(2) {
                let qa = quantize_f64(&disc.positions[pair[0]]);
                let qb = quantize_f64(&disc.positions[pair[1]]);
                let key: UEdge = if qa <= qb { (qa, qb) } else { (qb, qa) };
                brep_edges.insert(key);
            }
            // Handle closed-loop edges (last→first).
            if verts.len() >= 3 {
                let qa = quantize_f64(&disc.positions[*verts.last().unwrap()]);
                let qb = quantize_f64(&disc.positions[verts[0]]);
                if qa != qb {
                    let key: UEdge = if qa <= qb { (qa, qb) } else { (qb, qa) };
                    brep_edges.insert(key);
                }
            }
        }

        // Build tri→face_id mapping from face_ranges.
        let mut tri_face_id: Vec<u64> = vec![0; n_tris];
        for range in face_ranges.iter() {
            let tri_start = range.start_index as usize / 3;
            let tri_end = range.end_index as usize / 3;
            for item in tri_face_id
                .iter_mut()
                .take(tri_end.min(n_tris))
                .skip(tri_start)
            {
                *item = range.face_id.0;
            }
        }

        // Build edge→triangle list for mesh edges.
        let mut edge_tris: BTreeMap<UEdge, Vec<usize>> = BTreeMap::new();
        for t in 0..n_tris {
            let base = t * 3;
            let tri = [indices[base], indices[base + 1], indices[base + 2]];
            for j in 0..3 {
                let pa = quantize(tri[j]);
                let pb = quantize(tri[(j + 1) % 3]);
                let key: UEdge = if pa <= pb { (pa, pb) } else { (pb, pa) };
                edge_tris.entry(key).or_default().push(t);
            }
        }

        // Find non-manifold interior edges (not B-Rep boundaries).
        let mut nm_edges: Vec<(UEdge, Vec<usize>)> = edge_tris
            .iter()
            .filter(|(edge, tris)| tris.len() >= 3 && !brep_edges.contains(edge))
            .map(|(e, t)| (*e, t.clone()))
            .collect();
        nm_edges.sort_by_key(|(edge, _)| *edge);

        if nm_edges.is_empty() {
            return; // No more non-manifold interior edges — done.
        }

        let mut flipped_any = false;

        for (nm_edge, tris) in &nm_edges {
            // Group triangles by face_id.
            let mut face_groups: BTreeMap<u64, Vec<usize>> = BTreeMap::new();
            for &t in tris {
                face_groups.entry(tri_face_id[t]).or_default().push(t);
            }

            // Look for a face with exactly 2 triangles sharing this edge — a flippable quad.
            for face_tris in face_groups.values() {
                if face_tris.len() != 2 {
                    continue;
                }

                let t_a = face_tris[0];
                let t_b = face_tris[1];

                // Extract vertex indices for both triangles.
                let tri_a = [indices[t_a * 3], indices[t_a * 3 + 1], indices[t_a * 3 + 2]];
                let tri_b = [indices[t_b * 3], indices[t_b * 3 + 1], indices[t_b * 3 + 2]];

                // Find the two shared vertices (on the non-manifold edge) and the
                // two non-shared vertices (the quad's opposite corners).
                let qa0 = nm_edge.0;
                let qa1 = nm_edge.1;

                // Find which vertex indices in tri_a correspond to the nm edge endpoints.
                let mut shared_a = [u32::MAX; 2]; // indices from tri_a on the nm edge
                let mut opp_a = u32::MAX; // opposite vertex in tri_a
                for &vi in &tri_a {
                    let qv = quantize(vi);
                    if qv == qa0 && shared_a[0] == u32::MAX {
                        shared_a[0] = vi;
                    } else if qv == qa1 && shared_a[1] == u32::MAX {
                        shared_a[1] = vi;
                    } else {
                        opp_a = vi;
                    }
                }

                let mut shared_b = [u32::MAX; 2];
                let mut opp_b = u32::MAX;
                for &vi in &tri_b {
                    let qv = quantize(vi);
                    if qv == qa0 && shared_b[0] == u32::MAX {
                        shared_b[0] = vi;
                    } else if qv == qa1 && shared_b[1] == u32::MAX {
                        shared_b[1] = vi;
                    } else {
                        opp_b = vi;
                    }
                }

                if opp_a == u32::MAX || opp_b == u32::MAX {
                    continue; // Couldn't identify quad vertices.
                }
                if shared_a[0] == u32::MAX || shared_a[1] == u32::MAX {
                    continue;
                }

                // Check that the new diagonal doesn't create a new non-manifold edge.
                let new_diag_qa = quantize(opp_a);
                let new_diag_qb = quantize(opp_b);
                let new_diag_key: UEdge = if new_diag_qa <= new_diag_qb {
                    (new_diag_qa, new_diag_qb)
                } else {
                    (new_diag_qb, new_diag_qa)
                };
                let existing_count = edge_tris.get(&new_diag_key).map_or(0, |t| t.len());
                if existing_count >= 2 {
                    continue; // Flipping would create another non-manifold edge.
                }

                // Compute the face normal from the ACTUAL vertex order of tri_a.
                let pos = |vi: u32| -> [f64; 3] {
                    let i = vi as usize * 3;
                    [
                        vertices[i] as f64,
                        vertices[i + 1] as f64,
                        vertices[i + 2] as f64,
                    ]
                };

                let p_a0 = pos(tri_a[0]);
                let p_a1 = pos(tri_a[1]);
                let p_a2 = pos(tri_a[2]);
                let face_normal = v3_cross(v3_sub(p_a1, p_a0), v3_sub(p_a2, p_a0));

                let p_oa = pos(opp_a);
                let p_ob = pos(opp_b);
                let p_s0 = pos(shared_a[0]);
                let p_s1 = pos(shared_a[1]);

                // New triangle 1: (shared_a[0], opp_a, opp_b)
                let new1_normal = v3_cross(v3_sub(p_oa, p_s0), v3_sub(p_ob, p_s0));
                let new1_area = v3_length(new1_normal);

                // New triangle 2: (shared_a[1], opp_b, opp_a)
                let new2_normal = v3_cross(v3_sub(p_ob, p_s1), v3_sub(p_oa, p_s1));
                let new2_area = v3_length(new2_normal);

                // Reject if either new triangle is degenerate.
                if new1_area < TAU_WORK || new2_area < TAU_WORK {
                    continue;
                }

                // Check winding: both new triangles must have normals
                // in the same direction as the original face normal.
                let dot1 = v3_dot(new1_normal, face_normal);
                let dot2 = v3_dot(new2_normal, face_normal);

                if dot1 > 0.0 && dot2 > 0.0 {
                    // Winding is correct.
                    indices[t_a * 3] = shared_a[0];
                    indices[t_a * 3 + 1] = opp_a;
                    indices[t_a * 3 + 2] = opp_b;

                    indices[t_b * 3] = shared_a[1];
                    indices[t_b * 3 + 1] = opp_b;
                    indices[t_b * 3 + 2] = opp_a;
                } else if dot1 < 0.0 && dot2 < 0.0 {
                    // Reverse winding for both.
                    indices[t_a * 3] = shared_a[0];
                    indices[t_a * 3 + 1] = opp_b;
                    indices[t_a * 3 + 2] = opp_a;

                    indices[t_b * 3] = shared_a[1];
                    indices[t_b * 3 + 1] = opp_a;
                    indices[t_b * 3 + 2] = opp_b;
                } else {
                    continue; // Non-convex quad — flip would invert a triangle.
                }

                flipped_any = true;
                break; // Restart edge scanning after a flip.
            }

            if flipped_any {
                break; // Rebuild edge maps and retry.
            }
        }

        if !flipped_any {
            return; // No flips possible — done.
        }
    }
}

/// Steiner-fan re-tessellation for faces with non-manifold interior diagonals.
///
/// After edge-flip repair, some faces may still contribute to non-manifold
/// interior edges (e.g., when 3+ faces share the same diagonal, or when the
/// quad is non-convex and can't be flipped).  For each such face, replace its
/// earcut triangulation with a centroid-fan: add the face polygon's centroid
/// as a new Steiner vertex and create N triangles (centroid→V_i→V_{i+1}) for
/// an N-vertex boundary.
///
/// Since each face's centroid is unique, no two faces can share interior
/// edges — only boundary edges are shared, which are B-Rep edges with exactly
/// 2 adjacent faces.
/// Position-based edge-flip for non-manifold edges in the fan-path mesh.
///
/// Like `flip_nonmanifold_interior_diagonals` but works without B-Rep
/// topology.  Groups triangles by face_range face_id, finds pairs of
/// triangles within the same face sharing a non-manifold edge, and flips
/// the diagonal if the resulting quad is convex and the new diagonal isn't
/// already non-manifold.
pub(super) fn flip_nonmanifold_edges_position_based(
    vertices: &[f32],
    indices: &mut [u32],
    face_ranges: &[FaceRange],
) {
    let max_iterations = 10;

    for _iteration in 0..max_iterations {
        let n_tris = indices.len() / 3;
        if n_tris < 3 {
            return;
        }

        let max_abs = vertices.iter().map(|v| v.abs()).fold(0.0_f32, f32::max);
        let grid = (max_abs as f64 * TAU_TESS_GRID_FACTOR).max(TAU_TESS_GRID_MIN);
        let inv_grid = 1.0 / grid;

        type QPos = (i64, i64, i64);
        let quantize = |idx: u32| -> QPos {
            let i = idx as usize * 3;
            if i + 2 >= vertices.len() {
                return (0, 0, 0);
            }
            (
                (vertices[i] as f64 * inv_grid).round() as i64,
                (vertices[i + 1] as f64 * inv_grid).round() as i64,
                (vertices[i + 2] as f64 * inv_grid).round() as i64,
            )
        };

        // Build tri→face_id mapping.
        let mut tri_face_id: Vec<u64> = vec![0; n_tris];
        for range in face_ranges.iter() {
            let tri_start = range.start_index as usize / 3;
            let tri_end = range.end_index as usize / 3;
            for item in tri_face_id
                .iter_mut()
                .take(tri_end.min(n_tris))
                .skip(tri_start)
            {
                *item = range.face_id.0;
            }
        }

        // Build edge→triangle list.
        type UEdge = (QPos, QPos);
        let mut edge_tris: BTreeMap<UEdge, Vec<usize>> = BTreeMap::new();
        for t in 0..n_tris {
            let base = t * 3;
            let tri = [indices[base], indices[base + 1], indices[base + 2]];
            for j in 0..3 {
                let pa = quantize(tri[j]);
                let pb = quantize(tri[(j + 1) % 3]);
                let key: UEdge = if pa <= pb { (pa, pb) } else { (pb, pa) };
                edge_tris.entry(key).or_default().push(t);
            }
        }

        // Find non-manifold edges (3+ triangles).
        let mut nm_edges: Vec<(UEdge, Vec<usize>)> = edge_tris
            .iter()
            .filter(|(_, tris)| tris.len() >= 3)
            .map(|(e, t)| (*e, t.clone()))
            .collect();
        nm_edges.sort_by_key(|(edge, _)| *edge);

        if nm_edges.is_empty() {
            return;
        }

        let mut flipped_any = false;

        for (nm_edge, tris) in &nm_edges {
            // Group by face_id.
            let mut face_groups: BTreeMap<u64, Vec<usize>> = BTreeMap::new();
            for &t in tris {
                face_groups.entry(tri_face_id[t]).or_default().push(t);
            }

            // Look for a face with exactly 2 triangles sharing this edge.
            let mut flipped_any_this_edge = false;
            for face_tris in face_groups.values() {
                if face_tris.len() != 2 {
                    continue;
                }

                let t_a = face_tris[0];
                let t_b = face_tris[1];

                let tri_a = [indices[t_a * 3], indices[t_a * 3 + 1], indices[t_a * 3 + 2]];
                let tri_b = [indices[t_b * 3], indices[t_b * 3 + 1], indices[t_b * 3 + 2]];

                let qa0 = nm_edge.0;
                let qa1 = nm_edge.1;

                // Find shared and opposite vertices.
                let mut shared_a = [u32::MAX; 2];
                let mut opp_a = u32::MAX;
                for &vi in &tri_a {
                    let qv = quantize(vi);
                    if qv == qa0 && shared_a[0] == u32::MAX {
                        shared_a[0] = vi;
                    } else if qv == qa1 && shared_a[1] == u32::MAX {
                        shared_a[1] = vi;
                    } else {
                        opp_a = vi;
                    }
                }

                let mut opp_b = u32::MAX;
                for &vi in &tri_b {
                    let qv = quantize(vi);
                    if qv != qa0 && qv != qa1 {
                        opp_b = vi;
                    }
                }

                if opp_a == u32::MAX
                    || opp_b == u32::MAX
                    || shared_a[0] == u32::MAX
                    || shared_a[1] == u32::MAX
                {
                    continue;
                }

                // Check new diagonal doesn't create another nm edge.
                let new_diag_qa = quantize(opp_a);
                let new_diag_qb = quantize(opp_b);
                let new_diag_key: UEdge = if new_diag_qa <= new_diag_qb {
                    (new_diag_qa, new_diag_qb)
                } else {
                    (new_diag_qb, new_diag_qa)
                };
                let existing_count = edge_tris.get(&new_diag_key).map_or(0, |t| t.len());
                if existing_count >= 2 {
                    continue;
                }

                // Compute face normal for winding check.
                let pos = |vi: u32| -> [f64; 3] {
                    let i = vi as usize * 3;
                    [
                        vertices[i] as f64,
                        vertices[i + 1] as f64,
                        vertices[i + 2] as f64,
                    ]
                };

                let p_a0 = pos(tri_a[0]);
                let p_a1 = pos(tri_a[1]);
                let p_a2 = pos(tri_a[2]);
                let face_normal = v3_cross(v3_sub(p_a1, p_a0), v3_sub(p_a2, p_a0));

                let p_oa = pos(opp_a);
                let p_ob = pos(opp_b);
                let p_s0 = pos(shared_a[0]);
                let p_s1 = pos(shared_a[1]);

                let new1_normal = v3_cross(v3_sub(p_oa, p_s0), v3_sub(p_ob, p_s0));
                let new2_normal = v3_cross(v3_sub(p_ob, p_s1), v3_sub(p_oa, p_s1));

                if v3_length(new1_normal) < TAU_WORK || v3_length(new2_normal) < TAU_WORK {
                    continue;
                }

                let dot1 = v3_dot(new1_normal, face_normal);
                let dot2 = v3_dot(new2_normal, face_normal);

                if dot1 > 0.0 && dot2 > 0.0 {
                    indices[t_a * 3] = shared_a[0];
                    indices[t_a * 3 + 1] = opp_a;
                    indices[t_a * 3 + 2] = opp_b;
                    indices[t_b * 3] = shared_a[1];
                    indices[t_b * 3 + 1] = opp_b;
                    indices[t_b * 3 + 2] = opp_a;
                } else if dot1 < 0.0 && dot2 < 0.0 {
                    indices[t_a * 3] = shared_a[0];
                    indices[t_a * 3 + 1] = opp_b;
                    indices[t_a * 3 + 2] = opp_a;
                    indices[t_b * 3] = shared_a[1];
                    indices[t_b * 3 + 1] = opp_a;
                    indices[t_b * 3 + 2] = opp_b;
                } else {
                    continue;
                }

                flipped_any_this_edge = true;
                flipped_any = true;
                break;
            }

            // Cross-face fallback: when no single face has 2 triangles sharing
            // this edge, try pairs across different face ranges.
            if !flipped_any_this_edge {
                let qa0 = nm_edge.0;
                let qa1 = nm_edge.1;

                'outer: for i in 0..tris.len() {
                    for j in (i + 1)..tris.len() {
                        let t_a = tris[i];
                        let t_b = tris[j];

                        let tri_a = [indices[t_a * 3], indices[t_a * 3 + 1], indices[t_a * 3 + 2]];
                        let tri_b = [indices[t_b * 3], indices[t_b * 3 + 1], indices[t_b * 3 + 2]];

                        // Find shared and opposite vertices.
                        let mut shared = [u32::MAX; 2];
                        let mut opp_a = u32::MAX;
                        for &vi in &tri_a {
                            let qv = quantize(vi);
                            if qv == qa0 && shared[0] == u32::MAX {
                                shared[0] = vi;
                            } else if qv == qa1 && shared[1] == u32::MAX {
                                shared[1] = vi;
                            } else {
                                opp_a = vi;
                            }
                        }

                        let mut opp_b = u32::MAX;
                        for &vi in &tri_b {
                            let qv = quantize(vi);
                            if qv != qa0 && qv != qa1 {
                                opp_b = vi;
                            }
                        }

                        if opp_a == u32::MAX
                            || opp_b == u32::MAX
                            || shared[0] == u32::MAX
                            || shared[1] == u32::MAX
                        {
                            continue;
                        }

                        // Check new diagonal doesn't create another nm edge.
                        let new_diag_qa = quantize(opp_a);
                        let new_diag_qb = quantize(opp_b);
                        let new_diag_key: UEdge = if new_diag_qa <= new_diag_qb {
                            (new_diag_qa, new_diag_qb)
                        } else {
                            (new_diag_qb, new_diag_qa)
                        };
                        let existing_count = edge_tris.get(&new_diag_key).map_or(0, |t| t.len());
                        if existing_count >= 2 {
                            continue;
                        }

                        // Compute face normal for winding check.
                        let pos = |vi: u32| -> [f64; 3] {
                            let i = vi as usize * 3;
                            [
                                vertices[i] as f64,
                                vertices[i + 1] as f64,
                                vertices[i + 2] as f64,
                            ]
                        };

                        let p_a0 = pos(tri_a[0]);
                        let p_a1 = pos(tri_a[1]);
                        let p_a2 = pos(tri_a[2]);
                        let face_normal = v3_cross(v3_sub(p_a1, p_a0), v3_sub(p_a2, p_a0));

                        let p_oa = pos(opp_a);
                        let p_ob = pos(opp_b);
                        let p_s0 = pos(shared[0]);
                        let p_s1 = pos(shared[1]);

                        let new1_normal = v3_cross(v3_sub(p_oa, p_s0), v3_sub(p_ob, p_s0));
                        let new2_normal = v3_cross(v3_sub(p_ob, p_s1), v3_sub(p_oa, p_s1));

                        if v3_length(new1_normal) < TAU_WORK || v3_length(new2_normal) < TAU_WORK {
                            continue;
                        }

                        let dot1 = v3_dot(new1_normal, face_normal);
                        let dot2 = v3_dot(new2_normal, face_normal);

                        if dot1 > 0.0 && dot2 > 0.0 {
                            indices[t_a * 3] = shared[0];
                            indices[t_a * 3 + 1] = opp_a;
                            indices[t_a * 3 + 2] = opp_b;
                            indices[t_b * 3] = shared[1];
                            indices[t_b * 3 + 1] = opp_b;
                            indices[t_b * 3 + 2] = opp_a;
                        } else if dot1 < 0.0 && dot2 < 0.0 {
                            indices[t_a * 3] = shared[0];
                            indices[t_a * 3 + 1] = opp_b;
                            indices[t_a * 3 + 2] = opp_a;
                            indices[t_b * 3] = shared[1];
                            indices[t_b * 3 + 1] = opp_a;
                            indices[t_b * 3 + 2] = opp_b;
                        } else {
                            continue;
                        }

                        flipped_any = true;
                        break 'outer;
                    }
                }
            }

            if flipped_any {
                break;
            }
        }

        if !flipped_any {
            return;
        }
    }
}

#[allow(clippy::too_many_arguments, clippy::ptr_arg)]
pub(super) fn retessellate_nonmanifold_faces_with_steiner_fan(
    arena: &TopoArena,
    face_map: &BTreeMap<u64, FaceIdx>,
    face_geometry: &BTreeMap<FaceIdx, SurfaceGeom>,
    disc: &EdgeDiscretization,
    vertices: &mut Vec<f32>,
    normals: &mut Vec<f32>,
    indices: &mut Vec<u32>,
    face_ranges: &mut Vec<FaceRange>,
) {
    let n_tris = indices.len() / 3;
    if n_tris < 2 {
        return;
    }

    // Build quantization grid.
    let max_abs = vertices.iter().map(|v| v.abs()).fold(0.0_f32, f32::max);
    let grid = (max_abs as f64 * TAU_TESS_GRID_FACTOR).max(TAU_TESS_GRID_MIN);
    let inv_grid = 1.0 / grid;

    type QPos = (i64, i64, i64);
    let quantize = |idx: u32| -> QPos {
        let i = idx as usize * 3;
        if i + 2 >= vertices.len() {
            return (0, 0, 0);
        }
        (
            (vertices[i] as f64 * inv_grid).round() as i64,
            (vertices[i + 1] as f64 * inv_grid).round() as i64,
            (vertices[i + 2] as f64 * inv_grid).round() as i64,
        )
    };

    // Build B-Rep boundary edge set.
    type UEdge = (QPos, QPos);
    let mut brep_edges: HashSet<UEdge> = HashSet::new();
    let quantize_f64 = |pos: &[f64; 3]| -> QPos {
        (
            (pos[0] * inv_grid).round() as i64,
            (pos[1] * inv_grid).round() as i64,
            (pos[2] * inv_grid).round() as i64,
        )
    };
    for verts in disc.edge_verts.values() {
        for pair in verts.windows(2) {
            let qa = quantize_f64(&disc.positions[pair[0]]);
            let qb = quantize_f64(&disc.positions[pair[1]]);
            let key: UEdge = if qa <= qb { (qa, qb) } else { (qb, qa) };
            brep_edges.insert(key);
        }
        if verts.len() >= 3 {
            let qa = quantize_f64(&disc.positions[*verts.last().unwrap()]);
            let qb = quantize_f64(&disc.positions[verts[0]]);
            if qa != qb {
                let key: UEdge = if qa <= qb { (qa, qb) } else { (qb, qa) };
                brep_edges.insert(key);
            }
        }
    }

    // Build tri→face_id mapping.
    let mut tri_face_id: Vec<u64> = vec![0; n_tris];
    for range in face_ranges.iter() {
        let tri_start = range.start_index as usize / 3;
        let tri_end = range.end_index as usize / 3;
        for item in tri_face_id
            .iter_mut()
            .take(tri_end.min(n_tris))
            .skip(tri_start)
        {
            *item = range.face_id.0;
        }
    }

    // Build edge→triangle count for detection.
    let mut edge_counts: BTreeMap<UEdge, usize> = BTreeMap::new();
    for t in 0..n_tris {
        let base = t * 3;
        let tri = [indices[base], indices[base + 1], indices[base + 2]];
        for j in 0..3 {
            let pa = quantize(tri[j]);
            let pb = quantize(tri[(j + 1) % 3]);
            let key: UEdge = if pa <= pb { (pa, pb) } else { (pb, pa) };
            *edge_counts.entry(key).or_insert(0) += 1;
        }
    }

    // Find non-manifold interior edges (count >= 3, not B-Rep boundary).
    let nm_edges: Vec<UEdge> = edge_counts
        .iter()
        .filter(|(edge, &count)| count >= 3 && !brep_edges.contains(edge))
        .map(|(e, _)| *e)
        .collect();

    if nm_edges.is_empty() {
        return;
    }

    // Identify which face_ids have triangles on non-manifold interior edges.
    let mut affected_face_ids: HashSet<u64> = HashSet::new();
    for (t, &fid) in tri_face_id.iter().enumerate().take(n_tris) {
        let base = t * 3;
        let tri = [indices[base], indices[base + 1], indices[base + 2]];
        for j in 0..3 {
            let pa = quantize(tri[j]);
            let pb = quantize(tri[(j + 1) % 3]);
            let key: UEdge = if pa <= pb { (pa, pb) } else { (pb, pa) };
            if nm_edges.contains(&key) {
                affected_face_ids.insert(fid);
            }
        }
    }

    if affected_face_ids.is_empty() {
        return;
    }

    // Reverse map: face_id → (FaceIdx, kernel_id)
    let mut id_to_face: BTreeMap<u64, FaceIdx> = BTreeMap::new();
    for (&kid, &face_idx) in face_map {
        id_to_face.insert(kid, face_idx);
    }

    // For each affected face, re-tessellate with centroid-fan.
    for &fid in &affected_face_ids {
        let face_idx = match id_to_face.get(&fid) {
            Some(&fi) => fi,
            None => continue,
        };

        // Skip faces with inner loops (holes) — centroid-fan doesn't handle them.
        if !arena.faces[face_idx.0].inner_loops.is_empty() {
            continue;
        }

        // Get boundary for this face.
        let boundary = collect_loop_boundary(arena, arena.faces[face_idx.0].outer_loop, disc);
        if boundary.len() < 3 {
            continue;
        }

        // Compute face normal from geometry.
        let normal_f32 = match face_geometry.get(&face_idx) {
            Some(SurfaceGeom::Planar(plane)) => [
                plane.normal.x as f32,
                plane.normal.y as f32,
                plane.normal.z as f32,
            ],
            _ => {
                // Compute Newell normal from boundary.
                let loop_verts: Vec<[f64; 3]> =
                    boundary.iter().map(|&i| disc.positions[i]).collect();
                let bn = loop_verts.len();
                let mut newell = [0.0f64; 3];
                for i in 0..bn {
                    let curr = loop_verts[i];
                    let next = loop_verts[(i + 1) % bn];
                    newell[0] += (curr[1] - next[1]) * (curr[2] + next[2]);
                    newell[1] += (curr[2] - next[2]) * (curr[0] + next[0]);
                    newell[2] += (curr[0] - next[0]) * (curr[1] + next[1]);
                }
                let nlen = v3_length(newell);
                if nlen > TAU_WORK {
                    [
                        (newell[0] / nlen) as f32,
                        (newell[1] / nlen) as f32,
                        (newell[2] / nlen) as f32,
                    ]
                } else {
                    continue; // Degenerate face — skip.
                }
            }
        };

        // Compute centroid of boundary polygon.
        let n = boundary.len();
        let mut centroid = [0.0f64; 3];
        for &vi in &boundary {
            centroid[0] += disc.positions[vi][0];
            centroid[1] += disc.positions[vi][1];
            centroid[2] += disc.positions[vi][2];
        }
        centroid[0] /= n as f64;
        centroid[1] /= n as f64;
        centroid[2] /= n as f64;

        // Point-in-polygon test using winding number (2D projection).
        let stored_normal = [
            normal_f32[0] as f64,
            normal_f32[1] as f64,
            normal_f32[2] as f64,
        ];
        let (u_axis, v_axis) = compute_plane_basis(stored_normal);
        let loop_verts_2d: Vec<[f64; 2]> = boundary
            .iter()
            .map(|&i| {
                let d = v3_sub(disc.positions[i], disc.positions[boundary[0]]);
                [v3_dot(d, u_axis), v3_dot(d, v_axis)]
            })
            .collect();
        let centroid_2d = {
            let d = v3_sub(centroid, disc.positions[boundary[0]]);
            [v3_dot(d, u_axis), v3_dot(d, v_axis)]
        };

        if !point_in_polygon_winding(&centroid_2d, &loop_verts_2d) {
            continue; // Centroid outside polygon — skip.
        }

        // Remove old triangles for this face.
        // Find the face_range for this face.
        let range_idx = face_ranges.iter().position(|r| r.face_id.0 == fid);
        let range = match range_idx {
            Some(ri) => &face_ranges[ri],
            None => continue,
        };
        let old_start = range.start_index as usize;
        let old_end = range.end_index as usize;

        // Blank out old indices (set to u32::MAX to mark for removal).
        for idx in indices[old_start..old_end].iter_mut() {
            *idx = u32::MAX;
        }

        // Add centroid vertex.
        let centroid_vi = vertices.len() as u32 / 3;
        vertices.push(centroid[0] as f32);
        vertices.push(centroid[1] as f32);
        vertices.push(centroid[2] as f32);
        normals.push(normal_f32[0]);
        normals.push(normal_f32[1]);
        normals.push(normal_f32[2]);

        // Collect boundary vertex indices in the output vertex buffer.
        // We need to find which output vertex indices correspond to each boundary
        // discretization index. The bounded tessellation emits vertices in
        // boundary order, starting from the face_range's first vertex.
        // Since the old vertices are still in the buffer, we can map boundary
        // positions to existing output vertex indices via quantization.
        let mut boundary_out_indices: Vec<u32> = Vec::with_capacity(n);
        // Build a position→output-vertex-index map from the existing mesh.
        let n_verts = vertices.len() / 3;
        let mut pos_to_vi: BTreeMap<QPos, u32> = BTreeMap::new();
        for vi in 0..n_verts {
            let qp = (
                (vertices[vi * 3] as f64 * inv_grid).round() as i64,
                (vertices[vi * 3 + 1] as f64 * inv_grid).round() as i64,
                (vertices[vi * 3 + 2] as f64 * inv_grid).round() as i64,
            );
            pos_to_vi.entry(qp).or_insert(vi as u32);
        }

        for &bi in &boundary {
            let qp = quantize_f64(&disc.positions[bi]);
            if let Some(&vi) = pos_to_vi.get(&qp) {
                boundary_out_indices.push(vi);
            } else {
                // Boundary vertex not found — add it.
                let new_vi = vertices.len() as u32 / 3;
                vertices.push(disc.positions[bi][0] as f32);
                vertices.push(disc.positions[bi][1] as f32);
                vertices.push(disc.positions[bi][2] as f32);
                normals.push(normal_f32[0]);
                normals.push(normal_f32[1]);
                normals.push(normal_f32[2]);
                boundary_out_indices.push(new_vi);
            }
        }

        if boundary_out_indices.len() < 3 {
            continue;
        }

        // Check winding: boundary should match stored normal.
        let bverts: Vec<[f64; 3]> = boundary.iter().map(|&i| disc.positions[i]).collect();
        let mut newell = [0.0f64; 3];
        for i in 0..n {
            let curr = bverts[i];
            let next = bverts[(i + 1) % n];
            newell[0] += (curr[1] - next[1]) * (curr[2] + next[2]);
            newell[1] += (curr[2] - next[2]) * (curr[0] + next[0]);
            newell[2] += (curr[0] - next[0]) * (curr[1] + next[1]);
        }
        let reverse = v3_dot(newell, stored_normal) < 0.0;

        // Create fan triangles: centroid → V_i → V_{i+1}.
        let new_start = indices.len() as u32;
        for i in 0..n {
            let next_i = (i + 1) % n;
            if reverse {
                indices.push(centroid_vi);
                indices.push(boundary_out_indices[next_i]);
                indices.push(boundary_out_indices[i]);
            } else {
                indices.push(centroid_vi);
                indices.push(boundary_out_indices[i]);
                indices.push(boundary_out_indices[next_i]);
            }
        }
        let new_end = indices.len() as u32;

        // Update face_range to point to new triangles.
        if let Some(ri) = range_idx {
            face_ranges[ri].start_index = new_start;
            face_ranges[ri].end_index = new_end;
        }
    }

    // Compact: remove blanked-out indices (u32::MAX).
    compact_blanked_indices(indices, face_ranges);

    // Sort face_ranges by start_index to restore contiguity after compaction.
    // Retessellated faces have their fan triangles appended to the buffer end;
    // after compaction these ranges end up at the tail, breaking array ordering.
    face_ranges.sort_by_key(|r| r.start_index);

    // Remove empty ranges (faces entirely blanked with no replacement).
    face_ranges.retain(|r| r.start_index != r.end_index);
}

/// Point-in-polygon test using winding number algorithm.
/// Returns true if the point is strictly inside the polygon.
pub(super) fn point_in_polygon_winding(point: &[f64; 2], polygon: &[[f64; 2]]) -> bool {
    let n = polygon.len();
    if n < 3 {
        return false;
    }
    let mut winding: i32 = 0;
    for i in 0..n {
        let j = (i + 1) % n;
        let yi = polygon[i][1];
        let yj = polygon[j][1];
        if yi <= point[1] {
            if yj > point[1] {
                // Upward crossing
                let cross = (polygon[j][0] - polygon[i][0]) * (point[1] - polygon[i][1])
                    - (point[0] - polygon[i][0]) * (polygon[j][1] - polygon[i][1]);
                if cross > 0.0 {
                    winding += 1;
                }
            }
        } else if yj <= point[1] {
            // Downward crossing
            let cross = (polygon[j][0] - polygon[i][0]) * (point[1] - polygon[i][1])
                - (point[0] - polygon[i][0]) * (polygon[j][1] - polygon[i][1]);
            if cross < 0.0 {
                winding -= 1;
            }
        }
    }
    winding != 0
}

/// Remove blanked-out indices (u32::MAX markers) and update face_ranges.
pub(super) fn compact_blanked_indices(indices: &mut Vec<u32>, face_ranges: &mut [FaceRange]) {
    // Build a mapping from old index positions to new positions.
    let mut new_indices: Vec<u32> = Vec::with_capacity(indices.len());
    let mut old_to_new: Vec<usize> = Vec::with_capacity(indices.len());

    let mut tri_idx = 0;
    while tri_idx + 2 < indices.len() {
        if indices[tri_idx] == u32::MAX
            || indices[tri_idx + 1] == u32::MAX
            || indices[tri_idx + 2] == u32::MAX
        {
            // Skip this blanked triangle.
            old_to_new.push(usize::MAX);
            old_to_new.push(usize::MAX);
            old_to_new.push(usize::MAX);
        } else {
            let new_pos = new_indices.len();
            old_to_new.push(new_pos);
            old_to_new.push(new_pos + 1);
            old_to_new.push(new_pos + 2);
            new_indices.push(indices[tri_idx]);
            new_indices.push(indices[tri_idx + 1]);
            new_indices.push(indices[tri_idx + 2]);
        }
        tri_idx += 3;
    }

    // Update face_ranges.
    for range in face_ranges.iter_mut() {
        let old_start = range.start_index as usize;
        let old_end = range.end_index as usize;

        // Find first non-blanked index in [old_start, old_end).
        let mut new_start = usize::MAX;
        let mut new_end = 0usize;
        let mut i = old_start;
        while i < old_end && i < old_to_new.len() {
            if old_to_new[i] != usize::MAX {
                if new_start == usize::MAX {
                    new_start = old_to_new[i];
                }
                // The last valid index + 1 in the new buffer (end of last valid triangle).
                new_end = old_to_new[i] + 3;
                i += 3; // Skip to next triangle.
            } else {
                i += 3;
            }
        }

        if new_start == usize::MAX {
            // Face was entirely blanked — range becomes empty.
            range.start_index = 0;
            range.end_index = 0;
        } else {
            range.start_index = new_start as u32;
            range.end_index = new_end as u32;
        }
    }

    *indices = new_indices;
}

/// Compute triangle area from f32 vertices for sorting during removal.
pub(super) fn tri_area_f32(vertices: &[f32], indices: &[u32], tri_idx: usize) -> f64 {
    let base = tri_idx * 3;
    if base + 2 >= indices.len() {
        return 0.0;
    }
    let i0 = indices[base] as usize * 3;
    let i1 = indices[base + 1] as usize * 3;
    let i2 = indices[base + 2] as usize * 3;
    if i0 + 2 >= vertices.len() || i1 + 2 >= vertices.len() || i2 + 2 >= vertices.len() {
        return 0.0;
    }
    let v0 = [
        vertices[i0] as f64,
        vertices[i0 + 1] as f64,
        vertices[i0 + 2] as f64,
    ];
    let v1 = [
        vertices[i1] as f64,
        vertices[i1 + 1] as f64,
        vertices[i1 + 2] as f64,
    ];
    let v2 = [
        vertices[i2] as f64,
        vertices[i2 + 1] as f64,
        vertices[i2 + 2] as f64,
    ];
    let e1 = v3_sub(v1, v0);
    let e2 = v3_sub(v2, v0);
    v3_length(v3_cross(e1, e2))
}

/// For each non-manifold edge (shared by 3+ triangles), removes excess triangles
/// to bring the count down to 2. Processes edges in sorted order for determinism.
///
/// In `conservative` mode, fill triangles are only removed if at least 2 of their
/// 3 edges have count >= 3, and real triangles are only removed if all 3 edges
/// have count >= 3. This prevents creating new unpaired (boundary) edges.
///
/// In aggressive mode (`conservative = false`), all excess triangles are removed
/// with no safety check.
pub(super) fn remove_nonmanifold_duplicates_inner(
    vertices: &[f32],
    indices: &mut Vec<u32>,
    face_ranges: &mut Vec<FaceRange>,
    conservative: bool,
) {
    let n_tris = indices.len() / 3;
    if n_tris < 2 {
        return;
    }

    // Build quantization grid matching the test oracle exactly.
    let max_abs = vertices.iter().map(|v| v.abs()).fold(0.0_f32, f32::max);
    let grid = (max_abs as f64 * TAU_TESS_GRID_FACTOR).max(TAU_TESS_GRID_MIN);
    let inv_grid = 1.0 / grid;

    type QPos = (i64, i64, i64);
    let quantize = |idx: u32| -> QPos {
        let i = idx as usize * 3;
        if i + 2 >= vertices.len() {
            return (0, 0, 0);
        }
        (
            (vertices[i] as f64 * inv_grid).round() as i64,
            (vertices[i + 1] as f64 * inv_grid).round() as i64,
            (vertices[i + 2] as f64 * inv_grid).round() as i64,
        )
    };

    // Build a map from triangle index to its face_id.
    let mut tri_face_id: Vec<KernelId> = vec![KernelId(0); n_tris];
    for range in face_ranges.iter() {
        let tri_start = range.start_index as usize / 3;
        let tri_end = range.end_index as usize / 3;
        for item in tri_face_id
            .iter_mut()
            .take(tri_end.min(n_tris))
            .skip(tri_start)
        {
            *item = range.face_id;
        }
    }

    // Compute triangle area for each triangle (for removal priority).
    let tri_area: Vec<f64> = (0..n_tris)
        .map(|t| {
            let base = t * 3;
            let i0 = indices[base] as usize * 3;
            let i1 = indices[base + 1] as usize * 3;
            let i2 = indices[base + 2] as usize * 3;
            if i0 + 2 >= vertices.len() || i1 + 2 >= vertices.len() || i2 + 2 >= vertices.len() {
                return 0.0;
            }
            let v0 = [
                vertices[i0] as f64,
                vertices[i0 + 1] as f64,
                vertices[i0 + 2] as f64,
            ];
            let v1 = [
                vertices[i1] as f64,
                vertices[i1 + 1] as f64,
                vertices[i1 + 2] as f64,
            ];
            let v2 = [
                vertices[i2] as f64,
                vertices[i2 + 1] as f64,
                vertices[i2 + 2] as f64,
            ];
            let e1 = v3_sub(v1, v0);
            let e2 = v3_sub(v2, v0);
            v3_length(v3_cross(e1, e2))
        })
        .collect();

    // Iterate: batch-remove, then re-check. Converges because each iteration
    // removes at least one triangle.
    let mut remove_set: HashSet<usize> = HashSet::new();

    for _pass in 0..10 {
        // Build edge -> list of triangle indices (excluding already removed).
        type UEdge = (QPos, QPos);
        let mut edge_tris: BTreeMap<UEdge, Vec<usize>> = BTreeMap::new();
        for t in 0..n_tris {
            if remove_set.contains(&t) {
                continue;
            }
            let base = t * 3;
            let tri = [indices[base], indices[base + 1], indices[base + 2]];
            for j in 0..3 {
                let pa = quantize(tri[j]);
                let pb = quantize(tri[(j + 1) % 3]);
                let key: UEdge = if pa <= pb { (pa, pb) } else { (pb, pa) };
                edge_tris.entry(key).or_default().push(t);
            }
        }

        // Collect non-manifold edges and sort for deterministic processing.
        let mut nm_edges: Vec<(UEdge, Vec<usize>)> = edge_tris
            .iter()
            .filter(|(_, tris)| tris.len() >= 3)
            .map(|(e, t)| (*e, t.clone()))
            .collect();

        if nm_edges.is_empty() {
            break;
        }

        nm_edges.sort_by_key(|(edge, _)| *edge);

        let prev_remove_count = remove_set.len();

        // Build per-triangle edge lists and effective counts for safety checks.
        let tri_edge_keys: Vec<[UEdge; 3]> = (0..n_tris)
            .map(|t| {
                if remove_set.contains(&t) {
                    return [((0, 0, 0), (0, 0, 0)); 3];
                }
                let base = t * 3;
                let tri = [indices[base], indices[base + 1], indices[base + 2]];
                let mut edges = [((0, 0, 0), (0, 0, 0)); 3];
                for j in 0..3 {
                    let pa = quantize(tri[j]);
                    let pb = quantize(tri[(j + 1) % 3]);
                    edges[j] = if pa <= pb { (pa, pb) } else { (pb, pa) };
                }
                edges
            })
            .collect();

        let mut eff_edge_count: BTreeMap<UEdge, usize> = BTreeMap::new();
        for (e, tris) in edge_tris.iter() {
            eff_edge_count.insert(*e, tris.len());
        }

        for (_nm_edge, tris) in &nm_edges {
            let mut live: Vec<usize> = tris
                .iter()
                .copied()
                .filter(|t| !remove_set.contains(t))
                .collect();
            live.sort_unstable();
            live.dedup();

            if live.len() <= 2 {
                continue;
            }

            // Sort by removal priority: fill first, then smaller area, then higher index.
            live.sort_by(|&a, &b| {
                let a_fill = tri_face_id[a].0 >= u64::MAX - 1;
                let b_fill = tri_face_id[b].0 >= u64::MAX - 1;
                b_fill
                    .cmp(&a_fill)
                    .then_with(|| {
                        tri_area[a]
                            .partial_cmp(&tri_area[b])
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .then_with(|| b.cmp(&a))
            });

            let target_removals = live.len() - 2;
            let mut removed_count = 0;
            for &t in &live {
                if removed_count >= target_removals {
                    break;
                }

                // Safety check: removing this triangle must not drop any of its
                // edges below count 2 (which would create new boundary edges).
                let safe = if conservative {
                    tri_edge_keys[t]
                        .iter()
                        .all(|e| eff_edge_count.get(e).copied().unwrap_or(0) >= 3)
                } else {
                    true
                };

                if safe {
                    remove_set.insert(t);
                    removed_count += 1;
                    for e in &tri_edge_keys[t] {
                        if let Some(c) = eff_edge_count.get_mut(e) {
                            *c = c.saturating_sub(1);
                        }
                    }
                }
            }

            // If conservative mode couldn't remove enough (other edges have count=2),
            // try paired removal: find two triangles that share TWO edges (the NM edge
            // + one other). Removing both simultaneously drops the NM edge by 2 and
            // the shared edge by 2 (from 2→0), so we must also check that the partner
            // edge itself has count >= 4 or that the pair forms opposite-winding
            // duplicates (canceling faces). Instead, find triangles that share the NM
            // edge AND have a mutual second edge — removing both preserves the mutual
            // edge at count 0 but the third edges each drop by 1.
            //
            // Safer approach: for count=4 edges, check if two triangles share the
            // SAME 3 quantized vertices (winding-insensitive duplicates). If so,
            // they are coplanar duplicates and one can be safely removed.
            if conservative && removed_count < target_removals && live.len() == 4 {
                let remaining: Vec<usize> = live
                    .iter()
                    .copied()
                    .filter(|t| !remove_set.contains(t))
                    .collect();
                // Check for winding-insensitive duplicate pairs
                for i in 0..remaining.len() {
                    if removed_count >= target_removals {
                        break;
                    }
                    let ti = remaining[i];
                    if remove_set.contains(&ti) {
                        continue;
                    }
                    let mut ki = tri_edge_keys[ti];
                    ki.sort();
                    for &tj in &remaining[(i + 1)..] {
                        if removed_count >= target_removals {
                            break;
                        }
                        if remove_set.contains(&tj) {
                            continue;
                        }
                        let mut kj = tri_edge_keys[tj];
                        kj.sort();
                        // Same 3 edges = same triangle (possibly different winding)
                        if ki == kj {
                            // Remove the one with smaller area (likely the degenerate one)
                            let victim = if tri_area[ti] <= tri_area[tj] { ti } else { tj };
                            remove_set.insert(victim);
                            removed_count += 1;
                            for e in &tri_edge_keys[victim] {
                                if let Some(c) = eff_edge_count.get_mut(e) {
                                    *c = c.saturating_sub(1);
                                }
                            }
                        }
                    }
                }
            }
        }

        if remove_set.len() == prev_remove_count {
            break;
        }
    }

    if remove_set.is_empty() {
        return;
    }

    // Rebuild indices and face_ranges, skipping removed triangles.
    let mut new_indices = Vec::with_capacity(indices.len());
    let mut new_ranges = Vec::new();

    for range in face_ranges.iter() {
        let range_start = new_indices.len() as u32;
        let tri_start = range.start_index as usize / 3;
        let tri_end = range.end_index as usize / 3;

        for t in tri_start..tri_end.min(n_tris) {
            if remove_set.contains(&t) {
                continue;
            }
            let base = t * 3;
            new_indices.push(indices[base]);
            new_indices.push(indices[base + 1]);
            new_indices.push(indices[base + 2]);
        }

        let range_end = new_indices.len() as u32;
        if range_end > range_start {
            new_ranges.push(FaceRange {
                face_id: range.face_id,
                start_index: range_start,
                end_index: range_end,
            });
        }
    }

    *indices = new_indices;
    *face_ranges = new_ranges;
}

/// Conservative non-manifold removal: only removes fill triangles (with safety)
/// and fully-redundant real triangles. Used in the fan-path pipeline where
/// fill triangles may be needed for watertightness.
pub(super) fn remove_nonmanifold_duplicates(
    vertices: &[f32],
    indices: &mut Vec<u32>,
    face_ranges: &mut Vec<FaceRange>,
) {
    remove_nonmanifold_duplicates_inner(vertices, indices, face_ranges, true);
}

/// Aggressive non-manifold removal: removes all excess triangles without safety
/// checks. Used in the bounded-path pipeline where all triangles are real face
/// tessellations and non-manifold edges come from overlapping adjacent faces.
pub(super) fn remove_nonmanifold_duplicates_aggressive(
    vertices: &[f32],
    indices: &mut Vec<u32>,
    face_ranges: &mut Vec<FaceRange>,
) {
    remove_nonmanifold_duplicates_inner(vertices, indices, face_ranges, false);
}

/// Targeted non-manifold edge repair: for each non-manifold edge (count=3),
/// try removing each candidate triangle and check if the overall unpaired
/// count improves. Keep the best removal. This handles cases where
/// conservative removal is blocked (other edges have count=2) but removing
/// a specific triangle results in fillable boundary holes.
///
/// Only processes edges with count exactly 3 (the most common case after
/// all other repair). Higher counts are left to aggressive removal.
pub(super) fn repair_targeted_nonmanifold(
    vertices: &mut [f32],
    normals: &[f32],
    indices: &mut Vec<u32>,
    face_ranges: &mut Vec<FaceRange>,
) {
    let n_tris = indices.len() / 3;
    if n_tris < 3 {
        return;
    }

    let max_abs = vertices.iter().map(|v| v.abs()).fold(0.0_f32, f32::max);
    let grid = (max_abs as f64 * TAU_TESS_GRID_FACTOR).max(TAU_TESS_GRID_MIN);
    let inv_grid = 1.0 / grid;

    type QPos = (i64, i64, i64);
    let quantize = |idx: u32, verts: &[f32]| -> QPos {
        let i = idx as usize * 3;
        if i + 2 >= verts.len() {
            return (0, 0, 0);
        }
        (
            (verts[i] as f64 * inv_grid).round() as i64,
            (verts[i + 1] as f64 * inv_grid).round() as i64,
            (verts[i + 2] as f64 * inv_grid).round() as i64,
        )
    };

    // Build edge → triangle list
    type UEdge = (QPos, QPos);
    let mut edge_tris: BTreeMap<UEdge, Vec<usize>> = BTreeMap::new();
    for t in 0..n_tris {
        let base = t * 3;
        let tri = [indices[base], indices[base + 1], indices[base + 2]];
        for j in 0..3 {
            let pa = quantize(tri[j], vertices);
            let pb = quantize(tri[(j + 1) % 3], vertices);
            let key: UEdge = if pa <= pb { (pa, pb) } else { (pb, pa) };
            edge_tris.entry(key).or_default().push(t);
        }
    }

    // Find edges with exactly 3 sharing triangles
    let nm3_edges: Vec<(UEdge, Vec<usize>)> = edge_tris
        .iter()
        .filter(|(_, tris)| tris.len() == 3)
        .map(|(e, t)| (*e, t.clone()))
        .collect();

    if nm3_edges.is_empty() {
        return;
    }

    let baseline = count_unpaired_in_mesh(vertices, indices);

    // For each nm3 edge, try removing each of the 3 triangles.
    // After removal, simulate fill_boundary_holes to see if the resulting
    // boundary holes are fillable. Pick the removal that yields the best
    // post-fill unpaired count.
    let mut best_removal: Option<usize> = None;
    let mut best_score = baseline;

    // Build temporary face ranges for trial runs
    let trial_face_range = |trial_idx: &[u32]| -> Vec<FaceRange> {
        vec![FaceRange {
            face_id: KernelId(u64::MAX),
            start_index: 0,
            end_index: trial_idx.len() as u32,
        }]
    };

    for (_, tris) in &nm3_edges {
        for &t in tris {
            // Build trial index buffer without triangle t
            let mut trial_indices: Vec<u32> = (0..n_tris)
                .filter(|&i| i != t)
                .flat_map(|i| {
                    let b = i * 3;
                    [indices[b], indices[b + 1], indices[b + 2]]
                })
                .collect();

            // Simulate fill on the trial buffer
            let mut trial_ranges = trial_face_range(&trial_indices);
            fill_boundary_holes(vertices, normals, &mut trial_indices, &mut trial_ranges);

            let score = count_unpaired_in_mesh(vertices, &trial_indices);
            if score < best_score {
                best_score = score;
                best_removal = Some(t);
            }
        }
    }

    if let Some(remove_tri) = best_removal {
        // Apply the best removal
        let mut new_indices = Vec::with_capacity(indices.len() - 3);
        let mut new_ranges = Vec::new();

        for range in face_ranges.iter() {
            let range_start = new_indices.len() as u32;
            let tri_start = range.start_index as usize / 3;
            let tri_end = range.end_index as usize / 3;

            for t in tri_start..tri_end {
                if t == remove_tri {
                    continue;
                }
                let base = t * 3;
                if base + 2 >= indices.len() {
                    break;
                }
                new_indices.push(indices[base]);
                new_indices.push(indices[base + 1]);
                new_indices.push(indices[base + 2]);
            }

            let range_end = new_indices.len() as u32;
            if range_end > range_start {
                new_ranges.push(FaceRange {
                    face_id: range.face_id,
                    start_index: range_start,
                    end_index: range_end,
                });
            }
        }

        *indices = new_indices;
        *face_ranges = new_ranges;

        // Fill any boundary holes created by the removal
        fill_boundary_holes(vertices, normals, indices, face_ranges);
        remove_degenerate_triangles(vertices, indices, face_ranges);

        // Recurse to handle remaining nm3 edges (up to 10 depth)
        if count_nonmanifold_edges(vertices, indices) > 0
            && count_unpaired_in_mesh(vertices, indices) < baseline
        {
            repair_targeted_nonmanifold(vertices, normals, indices, face_ranges);
        }
    }
}

/// Count boundary edges (edges shared by exactly 1 triangle) in the mesh.
/// Uses the same quantization grid as the watertightness oracle.
pub(super) fn count_boundary_edges(vertices: &[f32], indices: &[u32]) -> usize {
    let n_tris = indices.len() / 3;
    if n_tris == 0 {
        return 0;
    }

    let max_abs = vertices.iter().map(|v| v.abs()).fold(0.0_f32, f32::max);
    let grid = (max_abs as f64 * TAU_TESS_GRID_FACTOR).max(TAU_TESS_GRID_MIN);
    let inv_grid = 1.0 / grid;

    type QPos = (i64, i64, i64);
    let quantize = |idx: u32| -> QPos {
        let i = idx as usize * 3;
        if i + 2 >= vertices.len() {
            return (0, 0, 0);
        }
        (
            (vertices[i] as f64 * inv_grid).round() as i64,
            (vertices[i + 1] as f64 * inv_grid).round() as i64,
            (vertices[i + 2] as f64 * inv_grid).round() as i64,
        )
    };

    let mut edge_counts: BTreeMap<(QPos, QPos), u32> = BTreeMap::new();
    for t in 0..n_tris {
        let base = t * 3;
        let tri = [indices[base], indices[base + 1], indices[base + 2]];
        for j in 0..3 {
            let pa = quantize(tri[j]);
            let pb = quantize(tri[(j + 1) % 3]);
            let key = if pa <= pb { (pa, pb) } else { (pb, pa) };
            *edge_counts.entry(key).or_insert(0) += 1;
        }
    }

    edge_counts.values().filter(|&&c| c == 1).count()
}

/// Count non-manifold edges (edges shared by 3+ triangles).
pub(super) fn count_nonmanifold_edges(vertices: &[f32], indices: &[u32]) -> usize {
    let n_tris = indices.len() / 3;
    if n_tris == 0 {
        return 0;
    }

    let max_abs = vertices.iter().map(|v| v.abs()).fold(0.0_f32, f32::max);
    let grid = (max_abs as f64 * TAU_TESS_GRID_FACTOR).max(TAU_TESS_GRID_MIN);
    let inv_grid = 1.0 / grid;

    type QPos = (i64, i64, i64);
    let quantize = |idx: u32| -> QPos {
        let i = idx as usize * 3;
        if i + 2 >= vertices.len() {
            return (0, 0, 0);
        }
        (
            (vertices[i] as f64 * inv_grid).round() as i64,
            (vertices[i + 1] as f64 * inv_grid).round() as i64,
            (vertices[i + 2] as f64 * inv_grid).round() as i64,
        )
    };

    let mut edge_counts: BTreeMap<(QPos, QPos), u32> = BTreeMap::new();
    for t in 0..n_tris {
        let base = t * 3;
        let tri = [indices[base], indices[base + 1], indices[base + 2]];
        for j in 0..3 {
            let pa = quantize(tri[j]);
            let pb = quantize(tri[(j + 1) % 3]);
            let key = if pa <= pb { (pa, pb) } else { (pb, pa) };
            *edge_counts.entry(key).or_insert(0) += 1;
        }
    }

    edge_counts.values().filter(|&&c| c >= 3).count()
}

/// Resolve mesh-level T-junctions.
///
/// A T-junction occurs when triangle T1 has an edge A→B, while adjacent
/// triangles T2, T3 have edges A→C and C→B (vertex C lies on the interior
/// of edge AB). This makes edges {A,B}, {A,C}, and {C,B} all appear with
/// count 1 (unpaired) in the oracle.
///
/// Fix: find boundary edges where a boundary vertex lies on the edge interior,
/// and split the triangle into two triangles at that vertex.
pub(super) fn resolve_mesh_t_junctions(
    vertices: &[f32],
    _normals: &[f32],
    indices: &mut Vec<u32>,
    face_ranges: &mut Vec<FaceRange>,
) {
    let n_tris = indices.len() / 3;
    if n_tris < 2 {
        return;
    }

    // Compute oracle-compatible quantization grid
    let max_abs = vertices.iter().map(|v| v.abs()).fold(0.0_f32, f32::max);
    let grid = (max_abs as f64 * TAU_TESS_GRID_FACTOR).max(TAU_TESS_GRID_MIN);
    let inv_grid = 1.0 / grid;

    type QPos = (i64, i64, i64);
    let quantize_pos = |idx: u32| -> QPos {
        let i = idx as usize * 3;
        if i + 2 >= vertices.len() {
            return (0, 0, 0);
        }
        (
            (vertices[i] as f64 * inv_grid).round() as i64,
            (vertices[i + 1] as f64 * inv_grid).round() as i64,
            (vertices[i + 2] as f64 * inv_grid).round() as i64,
        )
    };

    // Build undirected edge counts (oracle-style)
    let mut edge_counts: BTreeMap<(QPos, QPos), usize> = BTreeMap::new();
    let make_edge = |a: QPos, b: QPos| -> (QPos, QPos) {
        if a <= b {
            (a, b)
        } else {
            (b, a)
        }
    };

    for t in 0..n_tris {
        let base = t * 3;
        let qa = quantize_pos(indices[base]);
        let qb = quantize_pos(indices[base + 1]);
        let qc = quantize_pos(indices[base + 2]);
        *edge_counts.entry(make_edge(qa, qb)).or_insert(0) += 1;
        *edge_counts.entry(make_edge(qb, qc)).or_insert(0) += 1;
        *edge_counts.entry(make_edge(qc, qa)).or_insert(0) += 1;
    }

    // Collect boundary edges (undirected count != 2), sorted for determinism.
    let mut boundary_edges_vec: Vec<(QPos, QPos)> = edge_counts
        .iter()
        .filter(|(_, &c)| c != 2)
        .map(|(&e, _)| e)
        .collect();
    boundary_edges_vec.sort();
    let boundary_edges: std::collections::HashSet<(QPos, QPos)> =
        boundary_edges_vec.iter().copied().collect();

    if boundary_edges.is_empty() {
        return;
    }

    // Collect ONLY vertices that are endpoints of boundary edges (T-junction
    // candidates must themselves be on the boundary manifold).
    let mut boundary_verts: BTreeMap<QPos, u32> = BTreeMap::new();
    for &(qa, qb) in &boundary_edges_vec {
        // Find a vertex index for each quantized position
        for t in 0..n_tris {
            let base = t * 3;
            for k in 0..3 {
                let idx = indices[base + k];
                let qp = quantize_pos(idx);
                if qp == qa {
                    boundary_verts.entry(qa).or_insert(idx);
                }
                if qp == qb {
                    boundary_verts.entry(qb).or_insert(idx);
                }
            }
        }
    }

    // For each boundary edge, check if a BOUNDARY vertex lies on its interior.
    // Build map: triangle_index → list of (edge_local_idx, split_vertex_idx)
    let mut splits: BTreeMap<usize, Vec<(usize, u32)>> = BTreeMap::new();

    for t in 0..n_tris {
        let base = t * 3;
        let tri = [indices[base], indices[base + 1], indices[base + 2]];
        let qtri = [
            quantize_pos(tri[0]),
            quantize_pos(tri[1]),
            quantize_pos(tri[2]),
        ];

        for local_e in 0..3 {
            let qa = qtri[local_e];
            let qb = qtri[(local_e + 1) % 3];
            let edge_key = make_edge(qa, qb);

            if !boundary_edges.contains(&edge_key) {
                continue;
            }

            // Get f64 positions for the edge endpoints
            let ai = tri[local_e] as usize * 3;
            let bi = tri[(local_e + 1) % 3] as usize * 3;
            let ax = vertices[ai] as f64;
            let ay = vertices[ai + 1] as f64;
            let az = vertices[ai + 2] as f64;
            let bx = vertices[bi] as f64;
            let by = vertices[bi + 1] as f64;
            let bz = vertices[bi + 2] as f64;
            let dx = bx - ax;
            let dy = by - ay;
            let dz = bz - az;
            let edge_len_sq = dx * dx + dy * dy + dz * dz;
            if edge_len_sq < TAU_TESS_GRID_MIN * TAU_TESS_GRID_MIN {
                continue;
            }

            // Only check boundary vertices (not all mesh vertices)
            let mut best: Option<(f64, u32)> = None;
            // Sort boundary vertex candidates for deterministic tiebreaking.
            let mut bv_sorted: Vec<(QPos, u32)> =
                boundary_verts.iter().map(|(&k, &v)| (k, v)).collect();
            bv_sorted.sort();
            for &(qp, vidx) in &bv_sorted {
                if qp == qa || qp == qb {
                    continue;
                }

                let vi = vidx as usize * 3;
                let vx = vertices[vi] as f64;
                let vy = vertices[vi + 1] as f64;
                let vz = vertices[vi + 2] as f64;

                // Parametric position along edge
                let avx = vx - ax;
                let avy = vy - ay;
                let avz = vz - az;
                let t_param = (avx * dx + avy * dy + avz * dz) / edge_len_sq;
                if t_param <= crate::units::TJUNCTION_ENDPOINT_MARGIN
                    || t_param >= 1.0 - crate::units::TJUNCTION_ENDPOINT_MARGIN
                {
                    continue; // not clearly in interior
                }

                // Distance from line
                let px = ax + dx * t_param;
                let py = ay + dy * t_param;
                let pz = az + dz * t_param;
                let dist_sq = (vx - px) * (vx - px) + (vy - py) * (vy - py) + (vz - pz) * (vz - pz);
                // Tight tolerance: slightly more than half oracle grid cell
                let tol = grid * crate::units::TJUNCTION_GRID_FRACTION;
                if dist_sq < tol * tol {
                    // Pick the closest candidate (lowest dist_sq, tiebreak by QPos order)
                    if best.is_none() || dist_sq < best.unwrap().0 {
                        best = Some((dist_sq, vidx));
                    }
                }
            }

            if let Some((_, split_v)) = best {
                // Verify split produces non-degenerate triangles.
                // The third vertex (opposite the split edge) must not be collinear.
                let opp_idx = tri[(local_e + 2) % 3];
                let oi = opp_idx as usize * 3;
                let sv = split_v as usize * 3;
                if oi + 2 < vertices.len() && sv + 2 < vertices.len() {
                    let ox = vertices[oi] as f64;
                    let oy = vertices[oi + 1] as f64;
                    let oz = vertices[oi + 2] as f64;
                    let svx = vertices[sv] as f64;
                    let svy = vertices[sv + 1] as f64;
                    let svz = vertices[sv + 2] as f64;
                    // Check triangle (A, V, Opp): area = |cross(AV, AOpp)| / 2
                    let av = [svx - ax, svy - ay, svz - az];
                    let ao = [ox - ax, oy - ay, oz - az];
                    let c1x = av[1] * ao[2] - av[2] * ao[1];
                    let c1y = av[2] * ao[0] - av[0] * ao[2];
                    let c1z = av[0] * ao[1] - av[1] * ao[0];
                    let area1 = (c1x * c1x + c1y * c1y + c1z * c1z).sqrt() / 2.0;

                    // Check triangle (V, B, Opp): area = |cross(VB, VOpp)| / 2
                    let vb = [bx - svx, by - svy, bz - svz];
                    let vo = [ox - svx, oy - svy, oz - svz];
                    let c2x = vb[1] * vo[2] - vb[2] * vo[1];
                    let c2y = vb[2] * vo[0] - vb[0] * vo[2];
                    let c2z = vb[0] * vo[1] - vb[1] * vo[0];
                    let area2 = (c2x * c2x + c2y * c2y + c2z * c2z).sqrt() / 2.0;

                    // Only split if both triangles are non-degenerate
                    if area1 > TAU_TESS_GRID_MIN * crate::units::TJUNCTION_AREA_FRACTION
                        && area2 > TAU_TESS_GRID_MIN * crate::units::TJUNCTION_AREA_FRACTION
                    {
                        splits.entry(t).or_default().push((local_e, split_v));
                    }
                }
            }
        }
    }

    if splits.is_empty() {
        return;
    }

    // Rebuild index buffer, splitting triangles with T-junctions.
    let mut new_indices: Vec<u32> = Vec::with_capacity(indices.len() + splits.len() * 3);
    let mut new_ranges: Vec<FaceRange> = Vec::new();

    for range in face_ranges.iter() {
        let range_start = new_indices.len() as u32;
        let tri_start = range.start_index as usize / 3;
        let tri_end = range.end_index as usize / 3;

        for t in tri_start..tri_end {
            let base = t * 3;
            if base + 2 >= indices.len() {
                break;
            }

            if let Some(tri_splits) = splits.get(&t) {
                let i0 = indices[base];
                let i1 = indices[base + 1];
                let i2 = indices[base + 2];

                // Apply first split only
                let (local_e, split_v) = tri_splits[0];
                match local_e {
                    0 => {
                        // Split edge 0→1: [0,V,2] + [V,1,2]
                        new_indices.extend_from_slice(&[i0, split_v, i2]);
                        new_indices.extend_from_slice(&[split_v, i1, i2]);
                    }
                    1 => {
                        // Split edge 1→2: [0,1,V] + [0,V,2]
                        new_indices.extend_from_slice(&[i0, i1, split_v]);
                        new_indices.extend_from_slice(&[i0, split_v, i2]);
                    }
                    2 => {
                        // Split edge 2→0: [V,1,2] + [0,1,V]
                        new_indices.extend_from_slice(&[split_v, i1, i2]);
                        new_indices.extend_from_slice(&[i0, i1, split_v]);
                    }
                    _ => {
                        new_indices.extend_from_slice(&[i0, i1, i2]);
                    }
                }
            } else {
                new_indices.extend_from_slice(&[
                    indices[base],
                    indices[base + 1],
                    indices[base + 2],
                ]);
            }
        }

        let range_end = new_indices.len() as u32;
        if range_end > range_start {
            new_ranges.push(FaceRange {
                face_id: range.face_id,
                start_index: range_start,
                end_index: range_end,
            });
        }
    }

    *indices = new_indices;
    *face_ranges = new_ranges;
}

/// Check whether a boundary cycle is an intentional face opening (through-hole).
///
/// Returns `true` if BOTH conditions are met:
/// 1. ALL triangles adjacent to the cycle edges have geometric normals that agree
///    within `COS_HOLE_COHERENCE` — indicating the cycle is on a single face.
/// 2. The cycle winds CLOCKWISE relative to the face normal (inner loop), meaning
///    it's a hole boundary, not an outer face boundary.
///
/// Through-hole inner boundaries wind CW relative to the face normal (right-hand rule
/// excludes the hole interior from the face). Outer face boundaries and S-H artifact
/// gaps wind CCW. This distinguishes through-holes from legitimate fill targets.
type QuantizedPt = (i64, i64, i64);
type DirectedEdgeMap = BTreeMap<(QuantizedPt, QuantizedPt), usize>;

pub(super) fn boundary_cycle_is_coherent(
    cycle: &[QuantizedPt],
    vertices: &[f32],
    indices: &[u32],
    directed_edge_to_tri: &DirectedEdgeMap,
) -> bool {
    // Collect geometric normals of triangles adjacent to cycle edges,
    // and resolve cycle vertex positions.
    let mut adj_normals: Vec<[f64; 3]> = Vec::new();
    let mut cycle_positions: Vec<[f64; 3]> = Vec::with_capacity(cycle.len());

    let max_abs = vertices.iter().map(|v| v.abs()).fold(0.0_f32, f32::max);
    let grid = (max_abs as f64 * TAU_TESS_GRID_FACTOR).max(TAU_TESS_GRID_MIN);
    let inv_grid = 1.0 / grid;

    for i in 0..cycle.len() {
        let a = cycle[i];
        let b = cycle[(i + 1) % cycle.len()];
        // Try both directions: the cycle ordering may not match the triangle edge direction
        let tri_idx = directed_edge_to_tri
            .get(&(a, b))
            .or_else(|| directed_edge_to_tri.get(&(b, a)))
            .copied();
        if let Some(t) = tri_idx {
            let base = t * 3;
            if base + 2 >= indices.len() {
                continue;
            }
            let ia = indices[base] as usize * 3;
            let ib = indices[base + 1] as usize * 3;
            let ic = indices[base + 2] as usize * 3;
            if ia + 2 >= vertices.len() || ib + 2 >= vertices.len() || ic + 2 >= vertices.len() {
                continue;
            }
            // Geometric normal of adjacent triangle
            let ax = (vertices[ib] - vertices[ia]) as f64;
            let ay = (vertices[ib + 1] - vertices[ia + 1]) as f64;
            let az = (vertices[ib + 2] - vertices[ia + 2]) as f64;
            let bx = (vertices[ic] - vertices[ia]) as f64;
            let by = (vertices[ic + 1] - vertices[ia + 1]) as f64;
            let bz = (vertices[ic + 2] - vertices[ia + 2]) as f64;
            let nx = ay * bz - az * by;
            let ny = az * bx - ax * bz;
            let nz = ax * by - ay * bx;
            let len = (nx * nx + ny * ny + nz * nz).sqrt();
            if len > TAU_WORK {
                adj_normals.push([nx / len, ny / len, nz / len]);
            }

            // Resolve position of vertex 'a' from this triangle
            let mut found = false;
            for &vi in &[indices[base], indices[base + 1], indices[base + 2]] {
                let vi3 = vi as usize * 3;
                if vi3 + 2 >= vertices.len() {
                    continue;
                }
                let qx = (vertices[vi3] as f64 * inv_grid).round() as i64;
                let qy = (vertices[vi3 + 1] as f64 * inv_grid).round() as i64;
                let qz = (vertices[vi3 + 2] as f64 * inv_grid).round() as i64;
                if (qx, qy, qz) == a {
                    cycle_positions.push([
                        vertices[vi3] as f64,
                        vertices[vi3 + 1] as f64,
                        vertices[vi3 + 2] as f64,
                    ]);
                    found = true;
                    break;
                }
            }
            if !found {
                // Fallback: reconstruct from quantized position
                cycle_positions.push([a.0 as f64 * grid, a.1 as f64 * grid, a.2 as f64 * grid]);
            }
        }
    }

    // Need at least 3 adjacent normals for meaningful coherence check
    if adj_normals.len() < 3 || cycle_positions.len() < 3 {
        return false;
    }

    // Compute average face normal
    let mut avg = [0.0_f64; 3];
    for n in &adj_normals {
        avg[0] += n[0];
        avg[1] += n[1];
        avg[2] += n[2];
    }
    let avg_len = (avg[0] * avg[0] + avg[1] * avg[1] + avg[2] * avg[2]).sqrt();
    if avg_len < TAU_WORK {
        return false;
    }
    avg[0] /= avg_len;
    avg[1] /= avg_len;
    avg[2] /= avg_len;

    // Check if ALL normals agree with the average (coherence check)
    let min_dot = adj_normals
        .iter()
        .map(|n| n[0] * avg[0] + n[1] * avg[1] + n[2] * avg[2])
        .fold(f64::INFINITY, f64::min);

    let normals_coherent = min_dot > COS_HOLE_COHERENCE;

    if !normals_coherent {
        return false;
    }

    // Winding direction check: compute the signed area of the cycle projected
    // onto the face normal using the Newell method.
    //
    // For CCW triangles (standard outward-facing), boundary edges trace:
    //   - Outer boundary: CCW relative to face normal → positive signed area
    //   - Inner boundary (hole): CW relative to face normal → negative signed area
    //
    // Through-holes are inner boundaries → negative signed area → skip.
    let n = cycle_positions.len();
    let mut cross_sum = [0.0_f64; 3];
    for i in 0..n {
        let j = (i + 1) % n;
        let vi = &cycle_positions[i];
        let vj = &cycle_positions[j];
        cross_sum[0] += vi[1] * vj[2] - vi[2] * vj[1];
        cross_sum[1] += vi[2] * vj[0] - vi[0] * vj[2];
        cross_sum[2] += vi[0] * vj[1] - vi[1] * vj[0];
    }

    let signed_area = (cross_sum[0] * avg[0] + cross_sum[1] * avg[1] + cross_sum[2] * avg[2]) / 2.0;

    if signed_area < 0.0 {
        // CW winding = inner loop on a coherent-normal face = through-hole
        return true;
    }

    // For CCW cycles with coherent normals: the cycle might be the matching
    // boundary on the cylinder wall side of a through-hole. These are nearly
    // perfectly circular and perfectly planar. Only detect these with strict
    // thresholds to avoid false positives on revolve caps and S-H gaps.
    let n_pos = cycle_positions.len() as f64;
    let cx = cycle_positions.iter().map(|p| p[0]).sum::<f64>() / n_pos;
    let cy = cycle_positions.iter().map(|p| p[1]).sum::<f64>() / n_pos;
    let cz = cycle_positions.iter().map(|p| p[2]).sum::<f64>() / n_pos;

    let distances: Vec<f64> = cycle_positions
        .iter()
        .map(|p| {
            let dx = p[0] - cx;
            let dy = p[1] - cy;
            let dz = p[2] - cz;
            (dx * dx + dy * dy + dz * dz).sqrt()
        })
        .collect();

    let mean_dist = distances.iter().sum::<f64>() / n_pos;
    if mean_dist < TAU_WORK {
        return false;
    }

    // Check planarity: out-of-plane distance < 0.1% of mean radius
    let max_out_of_plane = cycle_positions
        .iter()
        .map(|p| ((p[0] - cx) * avg[0] + (p[1] - cy) * avg[1] + (p[2] - cz) * avg[2]).abs())
        .fold(0.0_f64, f64::max);
    if max_out_of_plane > mean_dist * HOLE_PLANARITY_RATIO {
        return false;
    }

    // Check strict circularity: coefficient of variation < 5%
    let variance = distances
        .iter()
        .map(|d| (d - mean_dist).powi(2))
        .sum::<f64>()
        / n_pos;
    let cv = variance.sqrt() / mean_dist;
    cv < HOLE_CIRCULARITY_CV
}

/// Fill small boundary holes in the mesh.
///
/// After boolean operations, S-H clipping can leave small holes where face
/// boundaries don't perfectly align. This function detects cycles of boundary
/// edges (edges that appear exactly once) and fills them with triangles.
///
/// Only fills holes with ≤ 128 edges (small to medium polygonal holes).
/// Larger holes indicate structural issues that shouldn't be auto-filled.
///
/// **DEPRECATED (A15.6):** Synthetic fill triangles mask S-H classification
/// errors. Will be removed when Yang hybrid pipeline is operational.
pub(super) fn fill_boundary_holes(
    vertices: &[f32],
    _normals: &[f32],
    indices: &mut Vec<u32>,
    face_ranges: &mut Vec<FaceRange>,
) {
    let n_tris = indices.len() / 3;
    if n_tris < 2 {
        return;
    }

    // Compute oracle-compatible quantization grid
    let max_abs = vertices.iter().map(|v| v.abs()).fold(0.0_f32, f32::max);
    let grid = (max_abs as f64 * TAU_TESS_GRID_FACTOR).max(TAU_TESS_GRID_MIN);
    let inv_grid = 1.0 / grid;

    type QPos = (i64, i64, i64);
    let quantize_pos = |idx: u32| -> QPos {
        let i = idx as usize * 3;
        if i + 2 >= vertices.len() {
            return (0, 0, 0);
        }
        (
            (vertices[i] as f64 * inv_grid).round() as i64,
            (vertices[i + 1] as f64 * inv_grid).round() as i64,
            (vertices[i + 2] as f64 * inv_grid).round() as i64,
        )
    };

    // Build directed edge → count and vertex index mapping
    let mut directed_counts: BTreeMap<(QPos, QPos), usize> = BTreeMap::new();
    // Map quantized position → vertex index (first seen)
    let mut pos_to_idx: BTreeMap<QPos, u32> = BTreeMap::new();

    for t in 0..n_tris {
        let base = t * 3;
        let tri_indices = [indices[base], indices[base + 1], indices[base + 2]];
        let tri_pos: Vec<QPos> = tri_indices.iter().map(|&i| quantize_pos(i)).collect();

        for j in 0..3 {
            let a = tri_pos[j];
            let b = tri_pos[(j + 1) % 3];
            *directed_counts.entry((a, b)).or_insert(0) += 1;
            pos_to_idx.entry(a).or_insert(tri_indices[j]);
        }
    }

    // Find boundary edges: directed edges that appear once and have no reverse
    let mut boundary_edges: Vec<(QPos, QPos)> = Vec::new();
    for (&(a, b), &count) in &directed_counts {
        if count == 1 {
            let rev_count = directed_counts.get(&(b, a)).copied().unwrap_or(0);
            if rev_count == 0 {
                boundary_edges.push((a, b));
            }
        }
    }

    if boundary_edges.is_empty() {
        return;
    }

    // Sort for deterministic cycle detection (eliminates BTreeMap ordering nondeterminism)
    boundary_edges.sort();

    // Build adjacency: for each boundary vertex, what are the next vertices?
    // Use Vec to handle branching (vertex with multiple outgoing boundary edges).
    let mut next_vertices: BTreeMap<QPos, Vec<QPos>> = BTreeMap::new();
    for &(a, b) in &boundary_edges {
        next_vertices.entry(a).or_default().push(b);
    }

    // Find cycles of boundary edges (max length 20 to cover medium holes)
    let mut used_edges = std::collections::HashSet::new();
    let mut fill_triangles: Vec<[u32; 3]> = Vec::new();

    for &(start, start_next) in &boundary_edges {
        if used_edges.contains(&(start, start_next)) {
            continue;
        }

        // Trace the cycle starting with this specific edge
        let mut cycle: Vec<QPos> = vec![start];
        let mut current = start_next;
        let mut found_cycle = false;

        for _ in 0..128 {
            if current == start && cycle.len() >= 3 {
                found_cycle = true;
                break;
            }
            cycle.push(current);
            // Pick the next vertex that isn't already in the cycle (avoid infinite loops)
            let next = next_vertices
                .get(&current)
                .and_then(|nexts| {
                    nexts.iter().find(|&&n| {
                        !used_edges.contains(&(current, n)) && (n == start || !cycle.contains(&n))
                    })
                })
                .copied();
            if let Some(n) = next {
                current = n;
            } else {
                break;
            }
        }

        if !found_cycle || cycle.len() > 128 {
            continue;
        }

        // Mark edges as used
        for i in 0..cycle.len() {
            let a = cycle[i];
            let b = cycle[(i + 1) % cycle.len()];
            used_edges.insert((a, b));
        }

        // Fan-triangulate the cycle to fill the hole
        let cycle_indices: Vec<u32> = cycle
            .iter()
            .filter_map(|q| pos_to_idx.get(q).copied())
            .collect();

        if cycle_indices.len() != cycle.len() {
            continue;
        }

        // Fan-triangulate, skipping degenerate triangles (collinear vertices)
        for i in 1..cycle_indices.len() - 1 {
            let ia = cycle_indices[0] as usize * 3;
            let ib = cycle_indices[i] as usize * 3;
            let ic = cycle_indices[i + 1] as usize * 3;
            if ia + 2 >= vertices.len() || ib + 2 >= vertices.len() || ic + 2 >= vertices.len() {
                continue;
            }
            let ax = vertices[ib] - vertices[ia];
            let ay = vertices[ib + 1] - vertices[ia + 1];
            let az = vertices[ib + 2] - vertices[ia + 2];
            let bx = vertices[ic] - vertices[ia];
            let by = vertices[ic + 1] - vertices[ia + 1];
            let bz = vertices[ic + 2] - vertices[ia + 2];
            let cx = ay * bz - az * by;
            let cy = az * bx - ax * bz;
            let cz = ax * by - ay * bx;
            let area = (cx * cx + cy * cy + cz * cz).sqrt() / 2.0;
            if area >= TAU_WORK as f32 {
                fill_triangles.push([cycle_indices[0], cycle_indices[i], cycle_indices[i + 1]]);
            }
        }
    }

    if fill_triangles.is_empty() {
        return;
    }

    // Add fill triangles as a new face range (or append to the last face range)
    let fill_start = indices.len() as u32;
    for tri in &fill_triangles {
        indices.push(tri[0]);
        indices.push(tri[1]);
        indices.push(tri[2]);
    }
    let fill_end = indices.len() as u32;

    // Add as a new face range with a synthetic face ID
    if fill_end > fill_start {
        face_ranges.push(FaceRange {
            face_id: KernelId(u64::MAX), // synthetic fill face
            start_index: fill_start,
            end_index: fill_end,
        });
    }
}

/// Close near-miss boundary chains by snapping close chain endpoints together.
///
/// After all other post-processing, some boundary edges form short open chains
/// where the start and end vertices are very close (within a few oracle grid
/// cells) but not identical. This happens when S-H clipping produces slightly
/// different intersection coordinates on adjacent faces.
///
/// This function finds such chains, snaps the endpoint vertex positions to
/// match the start vertex, and fills the resulting closed cycle with triangles.
///
/// **DEPRECATED (A15.6):** Vertex-snapping repair masks S-H classification
/// errors. Will be removed when Yang hybrid pipeline is operational.
pub(super) fn close_near_boundary_chains(
    vertices: &mut [f32],
    normals: &[f32],
    indices: &mut Vec<u32>,
    face_ranges: &mut Vec<FaceRange>,
) {
    // Strategy: find small groups of unpaired boundary edges that share vertices
    // and can be "healed" by adding fill triangles with the correct winding.
    //
    // For a manifold mesh, every directed half-edge A→B must have a matching B→A.
    // Unpaired edges (A→B exists but B→A doesn't) indicate missing faces.
    // When N unpaired edges share exactly N vertices (forming a polygon hole),
    // we can fill it with a fan of triangles.

    let n_tris = indices.len() / 3;
    if n_tris < 4 {
        return;
    }

    let max_abs = vertices.iter().map(|v| v.abs()).fold(0.0_f32, f32::max);
    let grid = (max_abs as f64 * TAU_TESS_GRID_FACTOR).max(TAU_TESS_GRID_MIN);
    let inv_grid = 1.0 / grid;

    type QPos = (i64, i64, i64);
    let quantize_pos = |idx: u32| -> QPos {
        let i = idx as usize * 3;
        if i + 2 >= vertices.len() {
            return (0, 0, 0);
        }
        (
            (vertices[i] as f64 * inv_grid).round() as i64,
            (vertices[i + 1] as f64 * inv_grid).round() as i64,
            (vertices[i + 2] as f64 * inv_grid).round() as i64,
        )
    };

    // Build directed edge counts and edge-to-triangle map
    let mut directed_counts: BTreeMap<(QPos, QPos), usize> = BTreeMap::new();
    let mut pos_to_idx: BTreeMap<QPos, u32> = BTreeMap::new();
    let mut directed_edge_to_tri: BTreeMap<(QPos, QPos), usize> = BTreeMap::new();

    for t in 0..n_tris {
        let base = t * 3;
        let tri_indices = [indices[base], indices[base + 1], indices[base + 2]];
        let tri_pos: Vec<QPos> = tri_indices.iter().map(|&i| quantize_pos(i)).collect();

        for j in 0..3 {
            let a = tri_pos[j];
            let b = tri_pos[(j + 1) % 3];
            *directed_counts.entry((a, b)).or_insert(0) += 1;
            pos_to_idx.entry(a).or_insert(tri_indices[j]);
            directed_edge_to_tri.entry((a, b)).or_insert(t);
        }
    }

    // Find boundary edges: directed half-edges with no matching reverse
    let mut boundary_edges: Vec<(QPos, QPos)> = Vec::new();
    for (&(a, b), &count) in &directed_counts {
        if count == 1 && directed_counts.get(&(b, a)).copied().unwrap_or(0) == 0 {
            boundary_edges.push((a, b));
        }
    }

    if boundary_edges.is_empty() {
        return;
    }

    // Sort for deterministic processing.
    boundary_edges.sort();

    // Collect boundary vertex adjacency (undirected) using union-find-like component detection
    let mut boundary_verts: HashSet<QPos> = HashSet::new();
    let mut vert_adj: BTreeMap<QPos, HashSet<QPos>> = BTreeMap::new();
    for &(a, b) in &boundary_edges {
        boundary_verts.insert(a);
        boundary_verts.insert(b);
        vert_adj.entry(a).or_default().insert(b);
        vert_adj.entry(b).or_default().insert(a);
    }

    // Find connected components of boundary vertices
    let mut visited: HashSet<QPos> = HashSet::new();
    let mut fill_triangles: Vec<[u32; 3]> = Vec::new();
    let boundary_edge_set: HashSet<(QPos, QPos)> = boundary_edges.iter().copied().collect();
    // Sort boundary vertices for deterministic component discovery.
    let mut sorted_boundary_verts: Vec<QPos> = boundary_verts.iter().copied().collect();
    sorted_boundary_verts.sort();

    for &start in &sorted_boundary_verts {
        if visited.contains(&start) {
            continue;
        }

        // BFS to find connected component (sort neighbors for determinism)
        let mut component: Vec<QPos> = Vec::new();
        let mut queue = vec![start];
        while let Some(v) = queue.pop() {
            if visited.contains(&v) {
                continue;
            }
            visited.insert(v);
            component.push(v);
            if let Some(neighbors) = vert_adj.get(&v) {
                let mut sorted_neighbors: Vec<QPos> = neighbors.iter().copied().collect();
                sorted_neighbors.sort();
                for &n in &sorted_neighbors {
                    if !visited.contains(&n) {
                        queue.push(n);
                    }
                }
            }
        }

        // Handle boundary components up to 64 vertices (raised from 32 to fill
        // larger boundary holes at complex boolean intersections, e.g.
        // cylinder-cylinder saddle curves with high tessellation resolution).
        if component.len() < 3 || component.len() > 64 {
            continue;
        }

        // Count boundary edges in this component
        let comp_set: HashSet<QPos> = component.iter().copied().collect();
        let comp_edges: Vec<(QPos, QPos)> = boundary_edges
            .iter()
            .filter(|(a, b)| comp_set.contains(a) && comp_set.contains(b))
            .copied()
            .collect();

        // For a triangle hole (3 edges, 3 vertices): add ONE triangle
        // that produces the 3 REVERSE edges needed to pair the boundary
        if component.len() == 3 && comp_edges.len() == 3 {
            let a = component[0];
            let b = component[1];
            let c = component[2];

            // We need to find a winding (a,b,c) such that the 3 half-edges
            // a→b, b→c, c→a are exactly the reverses of the 3 boundary edges.
            // Try both windings and pick the one that produces more reverse matches.
            let winding_abc = [
                boundary_edge_set.contains(&(b, a)),
                boundary_edge_set.contains(&(c, b)),
                boundary_edge_set.contains(&(a, c)),
            ];
            let winding_acb = [
                boundary_edge_set.contains(&(c, a)),
                boundary_edge_set.contains(&(b, c)),
                boundary_edge_set.contains(&(a, b)),
            ];

            let abc_matches: usize = winding_abc.iter().filter(|&&x| x).count();
            let acb_matches: usize = winding_acb.iter().filter(|&&x| x).count();

            if let (Some(&ia), Some(&ib), Some(&ic)) =
                (pos_to_idx.get(&a), pos_to_idx.get(&b), pos_to_idx.get(&c))
            {
                // Check area is non-degenerate
                let ai = ia as usize * 3;
                let bi = ib as usize * 3;
                let ci = ic as usize * 3;
                if ai + 2 < vertices.len() && bi + 2 < vertices.len() && ci + 2 < vertices.len() {
                    let ax = vertices[bi] - vertices[ai];
                    let ay = vertices[bi + 1] - vertices[ai + 1];
                    let az = vertices[bi + 2] - vertices[ai + 2];
                    let bx = vertices[ci] - vertices[ai];
                    let by = vertices[ci + 1] - vertices[ai + 1];
                    let bz = vertices[ci + 2] - vertices[ai + 2];
                    let cx_n = ay * bz - az * by;
                    let cy_n = az * bx - ax * bz;
                    let cz_n = ax * by - ay * bx;
                    let area = (cx_n * cx_n + cy_n * cy_n + cz_n * cz_n).sqrt() / 2.0;
                    if area >= TAU_WORK as f32 {
                        // Also consider stored vertex normals: the geometric
                        // normal of the fill triangle should agree with the
                        // average stored normal at its vertices.
                        let mut use_abc = abc_matches >= acb_matches;

                        // If edge matching is tied, use normals as tiebreaker.
                        // Also verify the edge-based choice against normals.
                        if ai + 2 < normals.len()
                            && bi + 2 < normals.len()
                            && ci + 2 < normals.len()
                        {
                            let snx = (normals[ai] + normals[bi] + normals[ci]) as f64 / 3.0;
                            let sny =
                                (normals[ai + 1] + normals[bi + 1] + normals[ci + 1]) as f64 / 3.0;
                            let snz =
                                (normals[ai + 2] + normals[bi + 2] + normals[ci + 2]) as f64 / 3.0;
                            // cx_n, cy_n, cz_n is the geometric normal for ABC winding
                            let dot = cx_n as f64 * snx + cy_n as f64 * sny + cz_n as f64 * snz;
                            // If normals disagree with edge-based choice, flip
                            if abc_matches == acb_matches {
                                use_abc = dot >= 0.0;
                            } else if (use_abc && dot < 0.0) || (!use_abc && dot > 0.0) {
                                // Edge matching and normals disagree — trust normals
                                use_abc = dot >= 0.0;
                            }
                        }

                        if use_abc {
                            fill_triangles.push([ia, ib, ic]);
                        } else {
                            fill_triangles.push([ia, ic, ib]);
                        }
                    }
                }
            }
        }

        // For a polygon hole (4+ edges, same number of vertices): trace the
        // boundary loop, determine winding, and fan-triangulate.
        // Upper bound raised to 32 to match the component limit above.
        // Skip coherent-normal cycles (through-hole openings).
        if component.len() >= 4 && component.len() <= 64 && comp_edges.len() == component.len() {
            // Order vertices by tracing through the boundary edges (undirected)
            let target_len = component.len();
            let mut ordered: Vec<QPos> = vec![component[0]];
            let mut remaining: Vec<QPos> = component[1..].to_vec();
            remaining.sort();
            while !remaining.is_empty() && ordered.len() < target_len {
                let last = *ordered.last().unwrap();
                if let Some(pos) = remaining.iter().position(|&v| {
                    boundary_edge_set.contains(&(last, v)) || boundary_edge_set.contains(&(v, last))
                }) {
                    ordered.push(remaining.remove(pos));
                } else {
                    break;
                }
            }

            // Check coherence AFTER ordering so the winding check is correct.
            if ordered.len() == target_len
                && ordered.len() >= HOLE_FILL_COHERENCE_MIN_EDGES
                && boundary_cycle_is_coherent(&ordered, vertices, indices, &directed_edge_to_tri)
            {
                #[cfg(debug_assertions)]
                eprintln!(
                    "close_near_boundary_chains: skipping coherent {}-vertex polygon hole (through-hole opening)",
                    component.len()
                );
                continue;
            }

            if ordered.len() == target_len {
                // Determine winding: count how many consecutive pairs (ordered[i], ordered[i+1])
                // match the REVERSE of a boundary edge (meaning our fill polygon's edge would
                // pair the boundary edge).
                let fwd_matches: usize = (0..target_len)
                    .filter(|&i| {
                        let a = ordered[i];
                        let b = ordered[(i + 1) % target_len];
                        boundary_edge_set.contains(&(b, a))
                    })
                    .count();
                let rev_matches: usize = (0..target_len)
                    .filter(|&i| {
                        let a = ordered[i];
                        let b = ordered[(i + 1) % target_len];
                        boundary_edge_set.contains(&(a, b))
                    })
                    .count();

                let reverse_winding = rev_matches > fwd_matches;

                // Resolve vertex indices
                let vert_indices: Vec<Option<u32>> =
                    ordered.iter().map(|q| pos_to_idx.get(q).copied()).collect();
                if vert_indices.iter().all(|v| v.is_some()) {
                    let vidx: Vec<u32> = vert_indices.into_iter().map(|v| v.unwrap()).collect();

                    // Fan triangulation from vertex 0
                    for j in 1..(target_len - 1) {
                        if reverse_winding {
                            fill_triangles.push([vidx[0], vidx[j + 1], vidx[j]]);
                        } else {
                            fill_triangles.push([vidx[0], vidx[j], vidx[j + 1]]);
                        }
                    }
                }
            }
        }

        // Open chain closure: when boundary edges form an open chain (not a
        // complete cycle), check if the chain endpoints are within 10× grid.
        // If so, snap them together and fill with fan triangles.
        // This handles S-H clipping divergence at cylinder-box intersection
        // boundaries where the tessellation produces almost-closed chains.
        // Only for chains up to 32 vertices to avoid filling large boundaries.
        if component.len() >= 3
            && component.len() <= 64
            && !comp_edges.is_empty()
            && comp_edges.len() < component.len()
        {
            // Build directed adjacency from boundary edges within this component
            let mut fwd: BTreeMap<QPos, QPos> = BTreeMap::new();
            let mut rev_map: BTreeMap<QPos, QPos> = BTreeMap::new();
            for &(a, b) in &comp_edges {
                fwd.insert(a, b);
                rev_map.insert(b, a);
            }

            // Find chain start: a vertex that has an outgoing boundary edge
            // but no incoming boundary edge within this component
            let chain_starts: Vec<QPos> = comp_edges
                .iter()
                .map(|&(a, _)| a)
                .filter(|a| !rev_map.contains_key(a))
                .collect();

            // We need exactly one chain start for a single open chain
            if chain_starts.len() == 1 {
                let chain_start = chain_starts[0];
                let mut chain: Vec<QPos> = vec![chain_start];
                let mut cur = chain_start;
                while let Some(&next) = fwd.get(&cur) {
                    chain.push(next);
                    cur = next;
                    if chain.len() > component.len() + 1 {
                        break; // safety valve
                    }
                }

                // Check if chain endpoints are within 10× grid distance
                let chain_end = *chain.last().unwrap();
                if chain.len() >= 3 && chain_start != chain_end {
                    if let (Some(&start_idx), Some(&end_idx)) =
                        (pos_to_idx.get(&chain_start), pos_to_idx.get(&chain_end))
                    {
                        let si = start_idx as usize * 3;
                        let ei = end_idx as usize * 3;
                        if si + 2 < vertices.len() && ei + 2 < vertices.len() {
                            let dx = (vertices[si] - vertices[ei]) as f64;
                            let dy = (vertices[si + 1] - vertices[ei + 1]) as f64;
                            let dz = (vertices[si + 2] - vertices[ei + 2]) as f64;
                            let dist_sq = dx * dx + dy * dy + dz * dz;
                            let snap_threshold = grid * 10.0;
                            let snap_threshold_sq = snap_threshold * snap_threshold;

                            if dist_sq <= snap_threshold_sq {
                                // Normal-coherence check for open chains ≥ 8 edges
                                if chain.len() >= HOLE_FILL_COHERENCE_MIN_EDGES
                                    && boundary_cycle_is_coherent(
                                        &chain[..chain.len() - 1],
                                        vertices,
                                        indices,
                                        &directed_edge_to_tri,
                                    )
                                {
                                    #[cfg(debug_assertions)]
                                    eprintln!(
                                        "close_near_boundary_chains: skipping coherent {}-vertex open chain (through-hole opening)",
                                        chain.len()
                                    );
                                    continue;
                                }

                                // Snap chain end to chain start position
                                vertices[ei] = vertices[si];
                                vertices[ei + 1] = vertices[si + 1];
                                vertices[ei + 2] = vertices[si + 2];

                                // Now the chain forms a closed loop — fill with fan triangles.
                                // Determine winding from boundary edges.
                                let fwd_matches: usize = (0..chain.len() - 1)
                                    .filter(|&i| {
                                        let a = chain[i];
                                        let b = chain[i + 1];
                                        boundary_edge_set.contains(&(b, a))
                                    })
                                    .count();
                                let rev_matches: usize = (0..chain.len() - 1)
                                    .filter(|&i| {
                                        let a = chain[i];
                                        let b = chain[i + 1];
                                        boundary_edge_set.contains(&(a, b))
                                    })
                                    .count();

                                let reverse_winding = rev_matches > fwd_matches;

                                // Use chain without duplicate end (it's snapped to start)
                                let loop_verts = &chain[..chain.len() - 1];
                                let vert_indices: Vec<Option<u32>> = loop_verts
                                    .iter()
                                    .map(|q| pos_to_idx.get(q).copied())
                                    .collect();
                                if vert_indices.iter().all(|v| v.is_some()) {
                                    let vidx: Vec<u32> =
                                        vert_indices.into_iter().map(|v| v.unwrap()).collect();
                                    let n = vidx.len();
                                    for j in 1..(n - 1) {
                                        if reverse_winding {
                                            fill_triangles.push([vidx[0], vidx[j + 1], vidx[j]]);
                                        } else {
                                            fill_triangles.push([vidx[0], vidx[j], vidx[j + 1]]);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if fill_triangles.is_empty() {
        return;
    }

    let fill_start = indices.len() as u32;
    for tri in &fill_triangles {
        indices.push(tri[0]);
        indices.push(tri[1]);
        indices.push(tri[2]);
    }
    let fill_end = indices.len() as u32;

    if fill_end > fill_start {
        face_ranges.push(FaceRange {
            face_id: KernelId(u64::MAX - 1), // synthetic boundary fill
            start_index: fill_start,
            end_index: fill_end,
        });
    }
}

/// Remove isolated triangles from the mesh.
///
/// An isolated triangle has ALL 3 edges appearing exactly once (no other
/// triangle shares any of its edges). These arise from stray face fragments
/// produced by Sutherland-Hodgman clipping at corner intersections — thin
/// slivers that the B-Rep stitching can't pair because no adjacent face
/// has matching edges.
///
/// Removal is safe because isolated triangles don't share edges with any
/// other triangle, so removing them doesn't break any existing edge pairings.
pub(super) fn remove_isolated_triangles(
    vertices: &[f32],
    indices: &mut Vec<u32>,
    face_ranges: &mut Vec<FaceRange>,
) {
    let n_tris = indices.len() / 3;
    if n_tris < 2 {
        return;
    }

    // Compute oracle-compatible quantization grid for edge matching
    let max_abs = vertices.iter().map(|v| v.abs()).fold(0.0_f32, f32::max);
    let grid = (max_abs as f64 * TAU_TESS_GRID_FACTOR).max(TAU_TESS_GRID_MIN);
    let inv_grid = 1.0 / grid;
    let quantize = |idx: u32| -> (i64, i64, i64) {
        let i = idx as usize * 3;
        if i + 2 >= vertices.len() {
            return (0, 0, 0);
        }
        (
            (vertices[i] as f64 * inv_grid).round() as i64,
            (vertices[i + 1] as f64 * inv_grid).round() as i64,
            (vertices[i + 2] as f64 * inv_grid).round() as i64,
        )
    };

    type PosEdge = ((i64, i64, i64), (i64, i64, i64));
    fn make_edge(a: (i64, i64, i64), b: (i64, i64, i64)) -> PosEdge {
        if a <= b {
            (a, b)
        } else {
            (b, a)
        }
    }

    // Build edge count map
    let mut edge_counts: BTreeMap<PosEdge, usize> = BTreeMap::new();
    for t in 0..n_tris {
        let base = t * 3;
        let va = quantize(indices[base]);
        let vb = quantize(indices[base + 1]);
        let vc = quantize(indices[base + 2]);
        *edge_counts.entry(make_edge(va, vb)).or_insert(0) += 1;
        *edge_counts.entry(make_edge(vb, vc)).or_insert(0) += 1;
        *edge_counts.entry(make_edge(vc, va)).or_insert(0) += 1;
    }

    // Mark triangles where ALL 3 edges are unpaired (count != 2)
    let mut keep = vec![true; n_tris];
    for (t, should_keep) in keep.iter_mut().enumerate().take(n_tris) {
        let base = t * 3;
        let va = quantize(indices[base]);
        let vb = quantize(indices[base + 1]);
        let vc = quantize(indices[base + 2]);
        let e1 = edge_counts.get(&make_edge(va, vb)).copied().unwrap_or(0);
        let e2 = edge_counts.get(&make_edge(vb, vc)).copied().unwrap_or(0);
        let e3 = edge_counts.get(&make_edge(vc, va)).copied().unwrap_or(0);
        if e1 != 2 && e2 != 2 && e3 != 2 {
            *should_keep = false;
        }
    }

    let removed = keep.iter().filter(|&&k| !k).count();
    #[cfg(test)]
    {
        let unpaired = edge_counts.values().filter(|&&c| c != 2).count();
        if unpaired > 0 || removed > 0 {
            eprintln!(
                "remove_isolated_triangles: n_tris={}, unpaired_edges={}, isolated_removed={}",
                n_tris, unpaired, removed
            );
            // Show triangles with any unpaired edges
            for t in 0..n_tris {
                let base = t * 3;
                let va = quantize(indices[base]);
                let vb = quantize(indices[base + 1]);
                let vc = quantize(indices[base + 2]);
                let e1 = edge_counts.get(&make_edge(va, vb)).copied().unwrap_or(0);
                let e2 = edge_counts.get(&make_edge(vb, vc)).copied().unwrap_or(0);
                let e3 = edge_counts.get(&make_edge(vc, va)).copied().unwrap_or(0);
                if e1 != 2 || e2 != 2 || e3 != 2 {
                    let i0 = indices[base] as usize;
                    let i1 = indices[base + 1] as usize;
                    let i2 = indices[base + 2] as usize;
                    eprintln!(
                        "  tri[{}]: edge_counts=({},{},{}) v0=({:.4},{:.4},{:.4}) v1=({:.4},{:.4},{:.4}) v2=({:.4},{:.4},{:.4})",
                        t, e1, e2, e3,
                        vertices[i0*3], vertices[i0*3+1], vertices[i0*3+2],
                        vertices[i1*3], vertices[i1*3+1], vertices[i1*3+2],
                        vertices[i2*3], vertices[i2*3+1], vertices[i2*3+2],
                    );
                }
            }
        }
    }
    if removed == 0 {
        return;
    }

    // Rebuild indices and face ranges without isolated triangles
    let mut new_indices = Vec::with_capacity(indices.len());
    let mut new_ranges = Vec::new();

    for range in face_ranges.iter() {
        let range_start = new_indices.len() as u32;
        let tri_start = range.start_index as usize / 3;
        let tri_end = range.end_index as usize / 3;

        for t in tri_start..tri_end {
            if t < n_tris && keep[t] {
                new_indices.push(indices[t * 3]);
                new_indices.push(indices[t * 3 + 1]);
                new_indices.push(indices[t * 3 + 2]);
            }
        }

        let range_end = new_indices.len() as u32;
        if range_end > range_start {
            new_ranges.push(FaceRange {
                face_id: range.face_id,
                start_index: range_start,
                end_index: range_end,
            });
        }
    }

    *indices = new_indices;
    *face_ranges = new_ranges;
}

/// Snap all vertex positions to the oracle's quantization grid.
///
/// The oracle uses grid = max(TAU_ORACLE_MIN, max_abs * TAU_ORACLE_FACTOR) to quantize vertex positions
/// for edge matching. Two vertices at positions P1 and P2 with |P1-P2| < grid/2
/// can still fall in adjacent grid cells, causing the oracle to see them as
/// different positions. By snapping all vertices to grid centers, we guarantee
/// that vertices within grid/2 of each other become exactly the same position.
///
/// Max position change: grid/2 ≈ 5e-5 at unit scale (0.05mm), well within
/// manufacturing tolerance and f32 visual precision.
/// Snap boundary vertex positions to the oracle's quantization grid.
/// Only vertices on unpaired edges are snapped, preserving interior mesh quality.
pub(super) fn snap_boundary_to_oracle_grid(vertices: &mut [f32], indices: &[u32]) {
    if vertices.is_empty() || indices.is_empty() {
        return;
    }
    let n_verts = vertices.len() / 3;
    let n_tris = indices.len() / 3;

    let max_abs = vertices.iter().map(|v| v.abs()).fold(0.0_f32, f32::max);
    let grid = (max_abs as f64 * TAU_TESS_GRID_FACTOR).max(TAU_TESS_GRID_MIN);
    let inv_grid = 1.0 / grid;

    let quantize = |idx: u32| -> (i64, i64, i64) {
        let i = idx as usize;
        if i >= n_verts {
            return (0, 0, 0);
        }
        (
            (vertices[i * 3] as f64 * inv_grid).round() as i64,
            (vertices[i * 3 + 1] as f64 * inv_grid).round() as i64,
            (vertices[i * 3 + 2] as f64 * inv_grid).round() as i64,
        )
    };

    type QPos = (i64, i64, i64);
    let make_edge = |a: QPos, b: QPos| -> (QPos, QPos) {
        if a <= b {
            (a, b)
        } else {
            (b, a)
        }
    };

    let mut edge_counts: BTreeMap<(QPos, QPos), usize> = BTreeMap::new();
    for t in 0..n_tris {
        let base = t * 3;
        let qt = [
            quantize(indices[base]),
            quantize(indices[base + 1]),
            quantize(indices[base + 2]),
        ];
        for e in 0..3 {
            let edge = make_edge(qt[e], qt[(e + 1) % 3]);
            if edge.0 != edge.1 {
                *edge_counts.entry(edge).or_insert(0) += 1;
            }
        }
    }

    // Collect boundary vertex indices
    let mut is_boundary = HashSet::new();
    for t in 0..n_tris {
        let base = t * 3;
        let tri = [indices[base], indices[base + 1], indices[base + 2]];
        let qt = [quantize(tri[0]), quantize(tri[1]), quantize(tri[2])];
        for e in 0..3 {
            let edge = make_edge(qt[e], qt[(e + 1) % 3]);
            if edge.0 != edge.1 && edge_counts.get(&edge).copied().unwrap_or(0) != 2 {
                is_boundary.insert(tri[e] as usize);
                is_boundary.insert(tri[(e + 1) % 3] as usize);
            }
        }
    }

    // Snap only boundary vertices to the grid
    for &vi in &is_boundary {
        if vi < n_verts {
            for j in 0..3 {
                let idx = vi * 3 + j;
                vertices[idx] = ((vertices[idx] as f64 * inv_grid).round() * grid) as f32;
            }
        }
    }
}
