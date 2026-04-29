// MIT License
//
// Copyright (c) 2022 G. Cherchi, M. Livesu, R. Scateni, M. Attene and F. Pellacini
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

//! Preprocessing utilities for Cherchi mesh arrangement.
//!
//! Spatial dedup of vertices, degenerate triangle removal, coordinate scaling.
//!
//! Ported from Cherchi processing.h + processing.cpp
//! MIT License (c) 2022 Cherchi, Livesu, Scateni, Attene, Pellacini

use std::collections::{BTreeMap, HashMap};

use super::super::indirect_predicates::ImplicitPoint;

/// Cosurface orientation classification per Cherchi 2020 §5.4 / Hoffmann
/// 1989 §5.3.
///
/// Two coplanar triangles that share a sorted vertex key are either
/// rotations of each other (`Parallel` — outward normals align, the
/// cosurface IS A∪B's boundary) or rotations of each other's reverse
/// (`AntiParallel` — outward normals oppose, both surfaces are interior to
/// A∪B and must annihilate).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Orientation {
    Parallel,
    AntiParallel,
}

impl Orientation {
    /// Detect orientation between two triangles known to share a sorted
    /// vertex key (i.e. the same 3 distinct vertex IDs as a multiset).
    /// Triangles are passed as un-sorted windings.
    ///
    /// Algorithm: find the offset `i` where `t1[i] == t2[0]`. If
    /// `t1[(i+1) % 3] == t2[1]`, `t2` is a cyclic rotation of `t1` (even
    /// permutation) → `Parallel`. Otherwise `t2` is a cyclic rotation of
    /// `t1`'s reverse (odd permutation) → `AntiParallel`.
    pub(crate) fn detect(t1: [usize; 3], t2: [usize; 3]) -> Self {
        let i = (0..3)
            .find(|&i| t1[i] == t2[0])
            .expect("Orientation::detect: t1 and t2 must share vertex set");
        if t1[(i + 1) % 3] == t2[1] {
            Orientation::Parallel
        } else {
            Orientation::AntiParallel
        }
    }
}

/// Compute the multiplier (power-of-2 scaling factor) for predicate stability.
///
/// Scales coordinates so the max absolute coordinate is near
/// `R = 11259470696.0` (avg_max_coord * old_multiplier), then rounds to the
/// nearest power of 2.
///
/// Ported from processing.cpp:47-64
#[allow(dead_code)]
pub(crate) fn compute_multiplier(coords: &[[f64; 3]]) -> f64 {
    const R: f64 = 11_259_470_696.0; // avg_max_coord (167.78) * old_multiplier (67108864.0)

    let mut abs_max: f64 = 0.0;
    for c in coords {
        for &v in c {
            let a = v.abs();
            if a > abs_max {
                abs_max = a;
            }
        }
    }

    if abs_max == 0.0 {
        return 1.0;
    }

    let div = R / abs_max;

    // Closest power of 2
    let e = div.log2().round() as i32;
    let multiplier = if e >= 0 {
        (1u64 << e.min(62)) as f64
    } else {
        1.0 / ((1u64 << (-e).min(62)) as f64)
    };

    if multiplier < 0.0 {
        1.0 // temporary fix, matching C++
    } else {
        multiplier
    }
}

/// Compute the multiplier from flat coordinate slice `[x0, y0, z0, x1, y1, z1, ...]`.
///
/// Convenience wrapper matching the C++ signature that takes `vector<double>`.
///
/// Ported from processing.cpp:47-64
#[allow(dead_code)]
pub(crate) fn compute_multiplier_flat(coords: &[f64]) -> f64 {
    const R: f64 = 11_259_470_696.0;

    if coords.is_empty() {
        return 1.0;
    }

    let max_coord = coords.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let min_coord = coords.iter().copied().fold(f64::INFINITY, f64::min);

    let abs_max = max_coord.abs().max(min_coord.abs());

    if abs_max == 0.0 {
        return 1.0;
    }

    let div = R / abs_max;

    let e = div.log2().round() as i32;
    let multiplier = if e >= 0 {
        (1u64 << e.min(62)) as f64
    } else {
        1.0 / ((1u64 << (-e).min(62)) as f64)
    };

    if multiplier < 0.0 {
        1.0
    } else {
        multiplier
    }
}

/// Merge duplicated vertices in the input mesh.
///
/// Takes flat coordinates and triangle indices, produces deduplicated vertex
/// list and remapped triangle indices.
///
/// Ported from processing.cpp:68-120 (sequential path)
#[allow(dead_code)]
pub(crate) fn merge_duplicated_vertices(
    in_coords: &[[f64; 3]],
    in_tris: &[usize],
) -> (Vec<[f64; 3]>, Vec<usize>) {
    let mut verts: Vec<[f64; 3]> = Vec::with_capacity(in_coords.len());
    let mut tris: Vec<usize> = Vec::with_capacity(in_tris.len());

    // Use a BTreeMap keyed on ImplicitPoint::Explicit for exact predicate-based dedup.
    // Matches C++ btree_map<genericPoint*, uint> with exact geometric ordering.
    let mut v_map: BTreeMap<ImplicitPoint, usize> = BTreeMap::new();

    for &v_id in in_tris {
        let v = in_coords[v_id];
        let point = ImplicitPoint::Explicit(v);

        let next_id = verts.len();
        let entry = v_map.entry(point).or_insert_with(|| {
            verts.push(v);
            next_id
        });
        tris.push(*entry);
    }

    (verts, tris)
}

/// Merge duplicated vertices from flat coordinate data.
///
/// Takes `[x0, y0, z0, x1, ...]` and triangle indices, returns deduplicated
/// `Vec<[f64; 3]>` and remapped triangle indices.
///
/// Ported from processing.cpp:68-120 (sequential path, flat coords variant)
#[allow(dead_code)]
pub(crate) fn merge_duplicated_vertices_flat(
    in_coords: &[f64],
    in_tris: &[usize],
) -> (Vec<[f64; 3]>, Vec<usize>) {
    let mut verts: Vec<[f64; 3]> = Vec::with_capacity(in_coords.len() / 3);
    let mut tris: Vec<usize> = Vec::with_capacity(in_tris.len());

    let mut v_map: BTreeMap<ImplicitPoint, usize> = BTreeMap::new();

    for &v_id in in_tris {
        let v = [
            in_coords[3 * v_id],
            in_coords[3 * v_id + 1],
            in_coords[3 * v_id + 2],
        ];
        let point = ImplicitPoint::Explicit(v);

        let next_id = verts.len();
        let entry = v_map.entry(point).or_insert_with(|| {
            verts.push(v);
            next_id
        });
        tris.push(*entry);
    }

    (verts, tris)
}

/// Remove degenerate (collinear) and duplicated triangles from the mesh.
///
/// Degenerate triangles have collinear vertices. Duplicated triangles (same sorted
/// vertex triple) get their labels merged via bitwise OR.
///
/// Returns `(tris, labels, clean_to_orig, orientations)` where
/// `clean_to_orig[i]` is the original triangle index that surviving triangle
/// `i` came from, and `orientations[i]` records the cosurface orientation
/// (Cherchi 2020 §5.4 / Hoffmann 1989 §5.3) when triangle `i` was merged with at
/// least one duplicate (`None` otherwise). PR10 Path A-refined.
///
/// Ported from processing.cpp:125-173
#[allow(dead_code)]
pub(crate) fn remove_degenerate_and_duplicated_triangles(
    verts: &[[f64; 3]],
    in_tris: &[usize],
    in_labels: &[u32],
) -> (Vec<usize>, Vec<u32>, Vec<usize>, Vec<Option<Orientation>>) {
    let num_orig_tris = in_tris.len() / 3;

    // PR10 invariant: when both A (label bit 0) and B (label bit 1) are
    // present in the input, A must come first. The STAGE2-survivor
    // convention used downstream by the cosurface short-circuit is "A is
    // the survivor"; this assert pins the ordering for the boolean path.
    // Legacy single-mesh / non-boolean tests use label==0 throughout and
    // are exempt (no A/B distinction).
    let any_a = in_labels.iter().any(|l| l & 1 != 0);
    let any_b = in_labels.iter().any(|l| l & 2 != 0);
    debug_assert!(
        !(any_a && any_b) || in_labels.first().is_none_or(|l| l & 1 != 0),
        "PR10: when both A and B labels are present, A-mesh tris must come first in in_labels"
    );

    let mut tris = Vec::with_capacity(in_tris.len());
    let mut labels = Vec::with_capacity(num_orig_tris);
    let mut clean_to_orig: Vec<usize> = Vec::with_capacity(num_orig_tris);
    let mut orientations: Vec<Option<Orientation>> = Vec::with_capacity(num_orig_tris);

    // Map from sorted vertex triple → index in output labels
    let mut tris_map: HashMap<[usize; 3], usize> = HashMap::with_capacity(num_orig_tris);

    for t_id in 0..num_orig_tris {
        let v0_id = in_tris[3 * t_id];
        let v1_id = in_tris[3 * t_id + 1];
        let v2_id = in_tris[3 * t_id + 2];
        let l = in_labels[t_id];

        // Check for degenerate (collinear) triangle
        if points_are_collinear_3d(&verts[v0_id], &verts[v1_id], &verts[v2_id]) {
            continue;
        }

        // Sorted triple for dedup
        let mut tri_key = [v0_id, v1_id, v2_id];
        tri_key.sort();

        match tris_map.entry(tri_key) {
            std::collections::hash_map::Entry::Vacant(e) => {
                let label_idx = labels.len();
                e.insert(label_idx);
                labels.push(l);
                clean_to_orig.push(t_id);
                orientations.push(None);
                tris.push(v0_id);
                tris.push(v1_id);
                tris.push(v2_id);
            }
            std::collections::hash_map::Entry::Occupied(e) => {
                // Merge labels for duplicate triangle
                let pos = *e.get();
                let prev_label = labels[pos];
                labels[pos] |= l;

                // PR10: detect cosurface orientation between the survivor
                // (already in `tris` at offset 3*pos) and this dropped
                // triangle. Both reference the same sorted vertex triple by
                // construction, so `Orientation::detect` is well-defined.
                // Refs: Cherchi 2020 §5.4 / Hoffmann 1989 §5.3.
                let surv_tri = [tris[3 * pos], tris[3 * pos + 1], tris[3 * pos + 2]];
                let dropped_tri = [v0_id, v1_id, v2_id];
                let detected = Orientation::detect(surv_tri, dropped_tri);
                orientations[pos] = match orientations[pos] {
                    None => Some(detected),
                    Some(prev) => {
                        debug_assert_eq!(
                            prev, detected,
                            "PR10: STAGE2 orientation conflict for sorted_key={:?}",
                            tri_key
                        );
                        Some(prev)
                    }
                };

                if std::env::var("CHERCHI_DEBUG").as_deref() == Ok("1") {
                    eprintln!(
                        "[stage2-merge] sorted_key=[{},{},{}] dropped=tri{} dropped_label={:#06b} survivor=tri{} prev_label={:#06b} merged_label={:#06b} orient={:?}",
                        tri_key[0],
                        tri_key[1],
                        tri_key[2],
                        t_id,
                        l,
                        clean_to_orig[pos],
                        prev_label,
                        labels[pos],
                        orientations[pos]
                    );
                }
            }
        }
    }

    (tris, labels, clean_to_orig, orientations)
}

/// Compute approximate coordinates from the vertex list, dividing by the multiplier.
///
/// Materializes each ImplicitPoint and divides by the multiplier.
/// The last 5 vertices are jolly points and are excluded from output.
///
/// Ported from processing.cpp:186-210
#[allow(dead_code)]
pub(crate) fn compute_approximate_coordinates(
    vertices: &[crate::boolean::indirect_predicates::ImplicitPoint],
    multiplier: f64,
) -> Vec<[f64; 3]> {
    if multiplier == 0.0 {
        let mut out = Vec::with_capacity(vertices.len());
        for v in vertices {
            out.push(v.materialize().unwrap_or([0.0, 0.0, 0.0]));
        }
        return out;
    }

    // Exclude last 5 jolly points
    let n = if vertices.len() >= 5 {
        vertices.len() - 5
    } else {
        vertices.len()
    };

    let mut out = Vec::with_capacity(n);
    for v in &vertices[..n] {
        let coords = v.materialize().unwrap_or([0.0, 0.0, 0.0]);
        out.push([
            coords[0] / multiplier,
            coords[1] / multiplier,
            coords[2] / multiplier,
        ]);
    }
    out
}

/// Check if three 3D points are collinear (degenerate triangle test).
///
/// Uses cross-product magnitude. Replaces `cinolib::points_are_colinear_3d`.
///
/// Ported from processing.cpp:144-146 (cinolib dependency replaced)
fn points_are_collinear_3d(a: &[f64; 3], b: &[f64; 3], c: &[f64; 3]) -> bool {
    // Cross product of (b - a) × (c - a)
    let ux = b[0] - a[0];
    let uy = b[1] - a[1];
    let uz = b[2] - a[2];
    let vx = c[0] - a[0];
    let vy = c[1] - a[1];
    let vz = c[2] - a[2];

    let cx = uy * vz - uz * vy;
    let cy = uz * vx - ux * vz;
    let cz = ux * vy - uy * vx;

    // If cross product is zero, points are collinear
    cx == 0.0 && cy == 0.0 && cz == 0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_multiplier() {
        // Coordinates around magnitude 1.0
        let coords = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let m = compute_multiplier(&coords);
        // Should be a power of 2 near R/1.0 = 11259470696
        assert!(m > 0.0);
        // Check it's a power of 2
        assert!((m.log2() - m.log2().round()).abs() < 1e-10);
    }

    #[test]
    fn test_compute_multiplier_flat() {
        let coords = vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
        let m = compute_multiplier_flat(&coords);
        assert!(m > 0.0);
        assert!((m.log2() - m.log2().round()).abs() < 1e-10);
    }

    #[test]
    fn test_compute_multiplier_zero() {
        let coords: Vec<[f64; 3]> = vec![[0.0, 0.0, 0.0]];
        assert_eq!(compute_multiplier(&coords), 1.0);
    }

    #[test]
    fn test_processing_dedup() {
        // 4 coords, but v0 and v3 are at the same position
        let coords = vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0], // duplicate of v0
        ];
        // tri0: v0,v1,v2   tri1: v3,v1,v2 (v3 == v0 in position)
        let tris = vec![0, 1, 2, 3, 1, 2];
        let (verts, new_tris) = merge_duplicated_vertices(&coords, &tris);

        // Should have 3 unique vertices
        assert_eq!(verts.len(), 3);
        // tri0 and tri1 should map v0 and v3 to the same ID
        assert_eq!(new_tris[0], new_tris[3]); // v0 and v3 merged
    }

    #[test]
    fn test_processing_dedup_flat() {
        let coords = vec![
            0.0, 0.0, 0.0, // v0
            1.0, 0.0, 0.0, // v1
            0.0, 1.0, 0.0, // v2
            0.0, 0.0, 0.0, // v3 = v0
        ];
        let tris = vec![0, 1, 2, 3, 1, 2];
        let (verts, new_tris) = merge_duplicated_vertices_flat(&coords, &tris);
        assert_eq!(verts.len(), 3);
        assert_eq!(new_tris[0], new_tris[3]);
    }

    #[test]
    fn test_remove_degenerate() {
        let verts = vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [2.0, 0.0, 0.0], // collinear with v0,v1
        ];
        // tri0: good triangle  tri1: degenerate (v0,v1,v3 are collinear)
        let tris = vec![0, 1, 2, 0, 1, 3];
        let labels = vec![1, 2];
        let (new_tris, new_labels, clean_to_orig, orientations) =
            remove_degenerate_and_duplicated_triangles(&verts, &tris, &labels);

        // Only tri0 survives
        assert_eq!(new_tris.len(), 3);
        assert_eq!(new_labels.len(), 1);
        assert_eq!(new_labels[0], 1);
        assert_eq!(clean_to_orig, vec![0]); // tri0 (original index 0) survived
        assert_eq!(orientations, vec![None]); // no merge → orientation unset
    }

    #[test]
    fn test_remove_duplicate_triangles() {
        let verts = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        // Same triangle twice with different labels
        let tris = vec![0, 1, 2, 0, 2, 1]; // reversed winding but same sorted triple
        let labels = vec![1, 2];
        let (new_tris, new_labels, clean_to_orig, orientations) =
            remove_degenerate_and_duplicated_triangles(&verts, &tris, &labels);

        // One triangle with merged label (1 | 2 = 3)
        assert_eq!(new_tris.len(), 3);
        assert_eq!(new_labels.len(), 1);
        assert_eq!(new_labels[0], 3);
        assert_eq!(clean_to_orig, vec![0]); // first occurrence kept
                                            // Survivor [0,1,2] vs dropped [0,2,1]: dropped is reverse of survivor.
                                            // Cherchi 2020 §5.4 / Hoffmann 1989 §5.3 → AntiParallel.
        assert_eq!(orientations, vec![Some(Orientation::AntiParallel)]);
    }

    #[test]
    fn test_clean_to_orig_with_mixed_removals() {
        let verts = vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [2.0, 0.0, 0.0], // collinear with v0,v1
            [0.0, 0.0, 1.0],
        ];
        // tri0: good, tri1: degenerate (collinear), tri2: good
        let tris = vec![0, 1, 2, 0, 1, 3, 0, 2, 4];
        let labels = vec![1, 1, 2];
        let (new_tris, new_labels, clean_to_orig, orientations) =
            remove_degenerate_and_duplicated_triangles(&verts, &tris, &labels);

        // tri0 and tri2 survive, tri1 removed
        assert_eq!(new_tris.len(), 6); // 2 triangles × 3 verts
        assert_eq!(new_labels.len(), 2);
        assert_eq!(clean_to_orig, vec![0, 2]); // original indices 0 and 2
        assert_eq!(orientations, vec![None, None]); // no merges
    }

    #[test]
    fn test_compute_approximate_coordinates() {
        use crate::boolean::indirect_predicates::ImplicitPoint;
        let verts = vec![
            ImplicitPoint::Explicit([2.0, 4.0, 6.0]),
            ImplicitPoint::Explicit([8.0, 10.0, 12.0]),
            // 5 jolly points
            ImplicitPoint::Explicit([0.0; 3]),
            ImplicitPoint::Explicit([0.0; 3]),
            ImplicitPoint::Explicit([0.0; 3]),
            ImplicitPoint::Explicit([0.0; 3]),
            ImplicitPoint::Explicit([0.0; 3]),
        ];
        let result = compute_approximate_coordinates(&verts, 2.0);
        assert_eq!(result.len(), 2);
        assert!((result[0][0] - 1.0).abs() < 1e-10);
        assert!((result[0][1] - 2.0).abs() < 1e-10);
        assert!((result[0][2] - 3.0).abs() < 1e-10);
        assert!((result[1][0] - 4.0).abs() < 1e-10);
    }

    #[test]
    fn test_points_are_collinear() {
        // Collinear points
        assert!(points_are_collinear_3d(
            &[0.0, 0.0, 0.0],
            &[1.0, 0.0, 0.0],
            &[2.0, 0.0, 0.0],
        ));
        // Non-collinear points
        assert!(!points_are_collinear_3d(
            &[0.0, 0.0, 0.0],
            &[1.0, 0.0, 0.0],
            &[0.0, 1.0, 0.0],
        ));
    }

    /// A-01 RED test (audit `docs/audits/cherchi_port_audit.md` Cluster I).
    ///
    /// Three points where the inexact f64 cross-product rounds to exactly
    /// (0, 0, 0) — so the current implementation reports them as collinear —
    /// but they are NOT collinear in exact arithmetic on the f64 inputs.
    /// The Shewchuk-exact `orient2d` on the XY projection returns `-6.0`,
    /// not `0.0`, so a Cherchi 2020 §3 / `cinolib::points_are_colinear_3d`
    /// faithful port must return `false`.
    ///
    /// Fixture construction (verified):
    ///   `a = (0, 0, 0)`, `b = (3e8, 7, 0)`, `c = (9e16/7, 3e8, 0)`.
    ///   The inexact f64 cross has z-component
    ///   `b[0]*c[1] - b[1]*c[0] = 3e8 * 3e8 - 7 * round_to_f64(9e16/7)`.
    ///   `3e8 * 3e8` rounds to exactly `9e16` and `7 * round_to_f64(9e16/7)`
    ///   also rounds to exactly `9e16`, so the f64 subtraction is `0.0`.
    ///   The other two cross components vanish trivially because
    ///   `a.z = b.z = c.z = 0`. Pre-fix returns `true`.
    ///
    ///   The exact 2D `orient2d` on the XY projection computes the
    ///   determinant in adaptive expansion arithmetic and returns `-6.0`,
    ///   exposing that `c` is not exactly on the line through `a` and `b`
    ///   (since `9e16/7` cannot be exactly represented in f64). Post-fix
    ///   returns `false`.
    ///
    /// Note on direction: the audit text described the opposite divergence
    /// (exact says collinear, f64 misses it). Per IEEE-754 round-to-nearest-
    /// even, that direction is unreachable: if `ux*vy = uy*vx` as exact reals
    /// on the f64 inputs, then `round(ux*vy) = round(uy*vx)` (rounding is a
    /// deterministic function of the exact value), so `cross = 0` in f64 too.
    /// The reachable direction — and the one this fixture exercises — is the
    /// reverse: f64 falsely says collinear because two products coincidentally
    /// round to equal values while the exact reals differ.
    ///
    /// Both directions are predicate-kernel bugs of the same class (Cluster I):
    /// the inexact f64 path disagrees with Shewchuk-exact `orient2d`, and
    /// downstream code relying on this predicate cannot trust its answers.
    /// The audit's recommended fix (three calls to `geometry_predicates::
    /// orient2d` on orthogonal projections, AND-ing `== 0.0`) addresses both
    /// directions.
    ///
    /// Ref: `cinolib::points_are_colinear_3d` (predicates.cpp:244-266) calls
    /// Shewchuk's exact `orient2d` on three orthogonal projections.
    #[test]
    fn test_points_are_collinear_3d_handles_f64_rounding() {
        let a = [0.0_f64, 0.0, 0.0];
        let b = [3.0e8_f64, 7.0, 0.0];
        let c = [9.0e16_f64 / 7.0, 3.0e8, 0.0];

        // Sanity: confirm the fixture is on a plane (z=0) and the inexact f64
        // cross product really rounds to zero.
        let ux = b[0] - a[0];
        let uy = b[1] - a[1];
        let vx = c[0] - a[0];
        let vy = c[1] - a[1];
        let cz_f64 = ux * vy - uy * vx;
        assert_eq!(
            cz_f64, 0.0,
            "fixture sanity: f64 cross product z-component must round to 0.0 \
             (this is what causes the pre-fix function to falsely return true)"
        );

        // The actual red-vs-green assertion. Pre-fix: f64 cross == (0,0,0)
        // ⇒ returns true (incorrect). Post-fix: orient2d on XY projection
        // returns -6.0, not 0.0 ⇒ returns false (correct).
        assert!(
            !points_are_collinear_3d(&a, &b, &c),
            "post-fix Shewchuk-exact orient2d on XY projection returns -6.0 \
             (not zero), so points are NOT collinear in exact arithmetic on \
             these f64 inputs. The f64 cross product happens to round to 0 \
             because 3e8*3e8 and 7*round(9e16/7) both round to 9e16. The \
             current inexact implementation reports collinear (true), which \
             is a Cluster I predicate-kernel bug per audit A-01."
        );
    }

    /// PR10 Phase B red test — Orientation::detect for all 6 permutations.
    ///
    /// **RED phase (PR10 Phase B): does not compile until Phase C defines
    /// the `Orientation` enum and its `detect` helper.** Compile failure IS
    /// the red signal per FIP §8 bug-fix variant.
    ///
    /// Two triangles share a sorted vertex key (STAGE2 dedup match). The
    /// un-sorted windings carry the orientation signal:
    ///
    ///   - Cyclic rotation of `t1` ⇒ same outward normal ⇒ Parallel.
    ///   - Cyclic rotation of `t1`'s reverse ⇒ opposite normal ⇒ AntiParallel.
    ///
    /// Refs: Cherchi 2020 §5.4 (coplanar pocket map); Hoffmann 1989 §5.3
    /// (cosurface sub-modes).
    #[test]
    fn test_orientation_detect_parallel_and_antiparallel() {
        let t1 = [7, 4, 6];

        // 3 cyclic rotations of t1 → Parallel (same outward normal).
        assert_eq!(
            Orientation::detect(t1, [7, 4, 6]),
            Orientation::Parallel,
            "identity rotation of t1 must be Parallel"
        );
        assert_eq!(
            Orientation::detect(t1, [4, 6, 7]),
            Orientation::Parallel,
            "cyclic rotation of t1 must be Parallel"
        );
        assert_eq!(
            Orientation::detect(t1, [6, 7, 4]),
            Orientation::Parallel,
            "cyclic rotation of t1 must be Parallel"
        );

        // 3 cyclic rotations of reverse(t1)=[6,4,7] → AntiParallel.
        assert_eq!(
            Orientation::detect(t1, [6, 4, 7]),
            Orientation::AntiParallel,
            "reverse of t1 must be AntiParallel"
        );
        assert_eq!(
            Orientation::detect(t1, [4, 7, 6]),
            Orientation::AntiParallel,
            "cyclic rotation of reverse(t1) must be AntiParallel"
        );
        assert_eq!(
            Orientation::detect(t1, [7, 6, 4]),
            Orientation::AntiParallel,
            "cyclic rotation of reverse(t1) must be AntiParallel"
        );
    }
}
