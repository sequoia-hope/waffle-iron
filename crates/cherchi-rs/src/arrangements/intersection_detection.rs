//! Mesh arrangement Stage 1 — triangle-pair intersection detection.
//!
//! Ported from Cherchi 2020 `arrangements/code/intersection_classification.cpp:47-94`
//! (MIT). © 2020 G. Cherchi, M. Livesu, R. Scateni, M. Attene.
//! See ../../LICENSE-THIRD-PARTY.md for full attribution.
//!
//! Cherchi 2020 §5 (mesh arrangement). This is the first stage:
//! given a triangle soup, find all pairs whose pairwise intersection
//! is non-empty. Subsequent stages (classification, re-triangulation,
//! assembly) consume the pair list to produce the arrangement.
//!
//! ## Deliberate deviations from upstream
//!
//! 1. **Spatial index RESTORED 2026-10-17** (was deviation #20, "no
//!    spatial index"). Upstream uses `cinolib::Octree` for O(n log n)
//!    average pair pruning (cpp:47-94); cherchi-rs started with O(n²) +
//!    AABB pre-pruning, justified as "not on critical path for the meshes
//!    Yang-rs will produce". Measured false on assay R0044 (yang matrix
//!    M-1): a 68-triangle cutter against a 100k-triangle revolved gear
//!    spent 29.8 s of Stage 2's 39.2 s in this loop, and the §4.5.2
//!    refined rung (398k triangles) 16× that. The broad phase is now the
//!    crate's deterministic AABB octree ([`crate::aabb_tree::AabbTree`],
//!    the cinolib-octree equivalent already serving BL2 and the
//!    improper-contact census), whose `query` returns exactly the
//!    inclusive-overlap set — so the pair list is BYTE-IDENTICAL to the
//!    brute loop (same pairs, same ascending order), pinned by the
//!    `tree_broad_phase_matches_brute` test below.
//!
//! 2. **Coplanar pairs included alongside Intersects** (deviation
//!    #21). Upstream's `classifyIntersections` (the next stage)
//!    consumes both uniformly. Filtering at detection would force
//!    downstream re-detection.
//!
//! ## Discovery during implementation
//!
//! CR9's `Coplanar` return covers BOTH "full coplanar" AND "edge of
//! one triangle lies in the other's plane" (per the CR9 docstring at
//! `predicates/triangle_intersect.rs:30-35`). The latter triggers
//! for many spatially-distant pairs (e.g. cube faces in perpendicular
//! planes where an edge of one happens to lie in the other's plane).
//!
//! AABB pre-pruning correctly filters these spatially-distant
//! "Coplanar-via-edge-in-plane" pairs. Upstream Cherchi does the
//! same via Octree pruning. The pruned algorithm matches upstream
//! behavior — it's a STRICT improvement over brute-force iteration,
//! not just a perf optimization.
//!
//! This is captured by the Group 7 property test's
//! `assert_pruned_subset_of_brute` (not `==` against brute-force).

use cad_primitives::Point3;
#[cfg(feature = "parallel")]
use rayon::prelude::*;

use crate::aabb_tree::{Aabb, AabbTree};
use crate::arrangements::FastTrimesh;
use crate::predicates::{triangle_intersects_triangle_3d, TriangleIntersection};

/// Detect all pairs of intersecting triangles in a single mesh.
///
/// Returns a list of `(t_a, t_b)` pairs with `t_a < t_b` where
/// [`triangle_intersects_triangle_3d`] reports either
/// `Intersects` or `Coplanar` (filters out `Disjoint`).
/// Coplanar pairs are included because downstream classification
/// (CR14+) consumes them uniformly with intersecting pairs.
///
/// **Algorithm**: per-triangle AABBs into the crate's deterministic
/// octree ([`AabbTree`], the cinolib-octree equivalent); each triangle's
/// box query (inclusive overlap, sorted) feeds the exact
/// triangle-triangle predicate. Byte-identical to the former O(n²) loop.
///
/// **Parallelism** (`parallel` feature, task #198): each ROW `i` (its
/// surviving `(i, j)` pairs with `j > i`) is a pure function of the immutable
/// soup + AABBs, so the rows are computed over a `rayon` pool. The rows are
/// collected in ASCENDING `i` order (an indexed parallel iterator collected
/// into a `Vec` preserves order) and flattened serially, so the result is
/// BYTE-IDENTICAL to the serial nested loop — same pairs, same order. Off by
/// default (Hard Rule #5); WASM never enables it.
///
/// **Output invariants**:
/// - Every pair satisfies `pair.0 < pair.1`.
/// - Each unordered pair appears at most once.
/// - The list contains exactly the non-`Disjoint` pairs.
/// - Pairs are in ascending lexicographic `(t_a, t_b)` order.
pub fn detect_intersecting_pairs(soup: &FastTrimesh) -> Vec<(u32, u32)> {
    let n = soup.num_tris();
    if n < 2 {
        return Vec::new();
    }
    // Per-triangle AABBs once, into the octree broad phase.
    let boxes: Vec<Aabb> = (0..n)
        .map(|t| {
            let (lo, hi) = tri_aabb(soup, t);
            Aabb {
                lo: [lo.x(), lo.y(), lo.z()],
                hi: [hi.x(), hi.y(), hi.z()],
            }
        })
        .collect();
    let tree = AabbTree::build(boxes);

    // Row `i`: the surviving `(i, j)` pairs (j > i), in ascending `j` (the
    // octree query is sorted). Pure in `(soup, tree)`, so distinct `i` rows
    // are independent.
    let row = |i: u32| -> Vec<(u32, u32)> {
        tree.query(&tree.items()[i as usize])
            .into_iter()
            .filter(|&j| j > i && pair_reported(soup, i, j))
            .map(|j| (i, j))
            .collect()
    };

    // Collect rows in index (`i`) order — identical whether the map ran serially
    // or over the rayon pool — then flatten in order.
    #[cfg(feature = "parallel")]
    let rows: Vec<Vec<(u32, u32)>> = (0..n).into_par_iter().map(row).collect();
    #[cfg(not(feature = "parallel"))]
    let rows: Vec<Vec<(u32, u32)>> = (0..n).map(row).collect();

    let mut out = Vec::with_capacity(rows.iter().map(Vec::len).sum());
    for r in rows {
        out.extend(r);
    }
    out
}

#[cfg(test)]
thread_local! {
    /// Test-only: disable the shared-sub-simplex skip, to pin that it
    /// leaves every arrangement byte-identical.
    static NO_VALID_COMPLEX_SKIP: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Test-only: run `f` with the shared-sub-simplex skip disabled (the
/// pre-2026-10-17 pair list).
#[cfg(test)]
pub(crate) fn without_valid_complex_skip<R>(f: impl FnOnce() -> R) -> R {
    NO_VALID_COMPLEX_SKIP.with(|c| c.set(true));
    let r = f();
    NO_VALID_COMPLEX_SKIP.with(|c| c.set(false));
    r
}

/// The detection predicate for one AABB-overlapping pair: the reference's
/// shared-sub-simplex verdict when it decides, else the full exact
/// triangle-triangle test (non-`Disjoint` is reported).
fn pair_reported(soup: &FastTrimesh, i: u32, j: u32) -> bool {
    #[cfg(test)]
    let skip_on = !NO_VALID_COMPLEX_SKIP.with(|c| c.get());
    #[cfg(not(test))]
    let skip_on = true;
    if skip_on {
        match shared_simplex_verdict(soup, i, j) {
            Some(SharedSimplex::ValidComplex) => return false,
            Some(SharedSimplex::Intersect) => return true,
            None => {}
        }
    }
    !matches!(
        triangle_intersects_triangle_3d(
            soup.tri_vert(i, 0),
            soup.tri_vert(i, 1),
            soup.tri_vert(i, 2),
            soup.tri_vert(j, 0),
            soup.tri_vert(j, 1),
            soup.tri_vert(j, 2)
        ),
        TriangleIntersection::Disjoint
    )
}

/// Verdict of [`shared_simplex_verdict`] for a pair that shares a vertex.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SharedSimplex {
    /// The two triangles meet ONLY at their shared vertex / edge (or are
    /// coincident): cinolib's `SIMPLICIAL_COMPLEX`. Not reported.
    ValidComplex,
    /// They share a sub-simplex AND overlap elsewhere: `INTERSECT`.
    Intersect,
}

/// The shared-sub-simplex branch of the reference's detection predicate:
/// `Triangle::intersects_triangle(t, ignore_if_valid_complex = true)`
/// (`find_intersections`, cpp:47-94) reports a pair only when
/// `triangle_triangle_intersect_3d` returns `INTERSECT`, never
/// `SIMPLICIAL_COMPLEX` — two triangles meeting exactly at a shared vertex
/// or edge are a valid complex and carry no arrangement geometry (the
/// classifier's own cpp:460 shared-edge early return). Vertex identity is
/// the soup's vertex INDEX: `merge_duplicated_vertices` has already fused
/// every bit-identical coordinate, so index equality is the reference's
/// `vec_equals_3d`. Exact (adaptive predicates with N24's zero tier).
///
/// - 3 shared vertices → `ValidComplex` (coincident; duplicates were
///   removed upstream anyway).
/// - 2 shared (an edge e, opposite vertices o0 / o1): non-coplanar
///   (`orient3d(t0, o1) ≠ 0`) → `ValidComplex`; coplanar with o0 and o1
///   STRICTLY on opposite sides of e in any axis-dropped projection →
///   `ValidComplex`; else `Intersect`.
/// - 1 shared vertex v: the edge of either triangle opposite v touching the
///   other triangle → `Intersect`, else `ValidComplex`. A coplanar edge
///   (`segment_intersects_triangle_3d` → `Coplanar`, which needs the 2-D
///   refinement) returns `None`: the caller runs the full test, exactly as
///   before this branch existed.
/// - 0 shared → `None` (the full test).
///
/// Why it matters (yang matrix M-1, measured 2026-10-17): on assay R0044's
/// 100k-triangle revolved gear, 603,453 of 604,277 candidate pairs were
/// SAME-mesh neighbours; the full test asks them `orient3d` of a shared
/// vertex against the other's plane — exactly 0.0 every time — and N24
/// re-certifies each zero in rationals, ~44 µs a pair, 26.7 s of a 39 s
/// Stage 2. This branch asks only the opposite vertices' signs.
fn shared_simplex_verdict(soup: &FastTrimesh, i: u32, j: u32) -> Option<SharedSimplex> {
    use crate::predicates::SegmentTriangleIntersection as St;
    use crate::predicates::{orient2d, orient3d, segment_intersects_triangle_3d, Sign};
    use cad_primitives::Point2;
    let ti = soup.tri(i);
    let tj = soup.tri(j);
    let mut i_shared = [false; 3];
    let mut j_shared = [false; 3];
    for (a, &va) in ti.iter().enumerate() {
        for (b, &vb) in tj.iter().enumerate() {
            if va == vb {
                i_shared[a] = true;
                j_shared[b] = true;
            }
        }
    }
    let count = i_shared.iter().filter(|&&x| x).count();
    let pi = |k: usize| soup.tri_vert(i, k as u32);
    let pj = |k: usize| soup.tri_vert(j, k as u32);
    match count {
        0 => None,
        3 => Some(SharedSimplex::ValidComplex),
        2 => {
            let opp0 = (0..3).find(|&k| !i_shared[k])?;
            let opp1 = (0..3).find(|&k| !j_shared[k])?;
            let e: Vec<usize> = (0..3).filter(|&k| i_shared[k]).collect();
            if orient3d(pi(0), pi(1), pi(2), pj(opp1)) != Sign::Zero {
                return Some(SharedSimplex::ValidComplex);
            }
            // Coplanar: the reference's three axis-dropped projections.
            for drop in 0..3 {
                let pr = |p: cad_primitives::Point3| -> Point2 {
                    let c = p.as_array();
                    match drop {
                        0 => Point2::new(c[1], c[2]),
                        1 => Point2::new(c[0], c[2]),
                        _ => Point2::new(c[0], c[1]),
                    }
                };
                let (e0, e1) = (pr(pi(e[0])), pr(pi(e[1])));
                let s0 = orient2d(e0, e1, pr(pi(opp0)));
                let s1 = orient2d(e0, e1, pr(pj(opp1)));
                if matches!(
                    (s0, s1),
                    (Sign::Positive, Sign::Negative) | (Sign::Negative, Sign::Positive)
                ) {
                    return Some(SharedSimplex::ValidComplex);
                }
            }
            Some(SharedSimplex::Intersect)
        }
        _ => {
            let v0 = (0..3).find(|&k| i_shared[k])?;
            let v1 = (0..3).find(|&k| j_shared[k])?;
            let (o0a, o0b) = (pi((v0 + 1) % 3), pi((v0 + 2) % 3));
            let (o1a, o1b) = (pj((v1 + 1) % 3), pj((v1 + 2) % 3));
            let r0 = segment_intersects_triangle_3d(o0a, o0b, pj(0), pj(1), pj(2));
            let r1 = segment_intersects_triangle_3d(o1a, o1b, pi(0), pi(1), pi(2));
            match (r0, r1) {
                (St::Intersects, _) | (_, St::Intersects) => Some(SharedSimplex::Intersect),
                (St::Disjoint, St::Disjoint) => Some(SharedSimplex::ValidComplex),
                _ => None,
            }
        }
    }
}

fn tri_aabb(soup: &FastTrimesh, t: u32) -> (Point3, Point3) {
    let v0 = soup.tri_vert(t, 0);
    let v1 = soup.tri_vert(t, 1);
    let v2 = soup.tri_vert(t, 2);
    let min = Point3::new(
        v0.x().min(v1.x()).min(v2.x()),
        v0.y().min(v1.y()).min(v2.y()),
        v0.z().min(v1.z()).min(v2.z()),
    );
    let max = Point3::new(
        v0.x().max(v1.x()).max(v2.x()),
        v0.y().max(v1.y()).max(v2.y()),
        v0.z().max(v1.z()).max(v2.z()),
    );
    (min, max)
}

#[cfg(test)]
fn aabbs_overlap(a_min: Point3, a_max: Point3, b_min: Point3, b_max: Point3) -> bool {
    !(a_max.x() < b_min.x()
        || a_min.x() > b_max.x()
        || a_max.y() < b_min.y()
        || a_min.y() > b_max.y()
        || a_max.z() < b_min.z()
        || a_min.z() > b_max.z())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arrangements::Plane;

    fn p(x: f64, y: f64, z: f64) -> Point3 {
        Point3::new(x, y, z)
    }

    /// Hand-coded brute force without AABB pre-pruning. Used by the
    /// property test (Group 7) to verify AABB doesn't false-negative.
    fn brute_force_pairs(soup: &FastTrimesh) -> Vec<(u32, u32)> {
        let mut out = Vec::new();
        let n = soup.num_tris();
        for i in 0..n {
            for j in (i + 1)..n {
                let a0 = soup.tri_vert(i, 0);
                let a1 = soup.tri_vert(i, 1);
                let a2 = soup.tri_vert(i, 2);
                let b0 = soup.tri_vert(j, 0);
                let b1 = soup.tri_vert(j, 1);
                let b2 = soup.tri_vert(j, 2);
                match triangle_intersects_triangle_3d(a0, a1, a2, b0, b1, b2) {
                    TriangleIntersection::Disjoint => continue,
                    _ => out.push((i, j)),
                }
            }
        }
        out
    }

    /// The pre-2026-10-17 brute broad phase: every `(i, j)` whose inclusive
    /// AABBs overlap, then the exact test. Kept as the octree's oracle.
    fn detect_intersecting_pairs_brute(soup: &FastTrimesh) -> Vec<(u32, u32)> {
        let n = soup.num_tris();
        let aabbs: Vec<(Point3, Point3)> = (0..n).map(|t| tri_aabb(soup, t)).collect();
        let mut out = Vec::new();
        for i in 0..n {
            let (a_min, a_max) = aabbs[i as usize];
            for j in (i + 1)..n {
                let (b_min, b_max) = aabbs[j as usize];
                if aabbs_overlap(a_min, a_max, b_min, b_max) && pair_reported(soup, i, j) {
                    out.push((i, j));
                }
            }
        }
        out
    }

    /// The octree broad phase returns EXACTLY the brute loop's list — same
    /// pairs, same ascending order — on a soup with shared edges, near
    /// misses, coincident boxes and far-apart triangles (the byte-identity
    /// the 2026-10-17 swap rests on).
    #[test]
    fn tree_broad_phase_matches_brute() {
        let mut s: u64 = 0x9e37_79b9_7f4a_7c15;
        let mut next = || {
            s = s
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((s >> 11) as f64) / ((1u64 << 53) as f64)
        };
        let mut verts: Vec<Point3> = Vec::new();
        let mut tris: Vec<[u32; 3]> = Vec::new();
        // A fan strip (every neighbour shares an edge) …
        for k in 0..40u32 {
            let x = k as f64 * 0.1;
            verts.push(p(x, 0.0, 0.0));
            verts.push(p(x, 1.0, 0.05 * (k % 3) as f64));
        }
        for k in 0..39u32 {
            tris.push([2 * k, 2 * k + 2, 2 * k + 1]);
            tris.push([2 * k + 1, 2 * k + 2, 2 * k + 3]);
        }
        // … crossed by random slivers and small triangles, some far away.
        for k in 0..300u32 {
            let far = if k % 7 == 0 { 50.0 } else { 0.0 };
            let c = [next() * 4.0 + far, next() * 1.2 - 0.1, next() * 0.4 - 0.2];
            let e = 0.05 + next() * (if k % 5 == 0 { 2.0 } else { 0.3 });
            let b = verts.len() as u32;
            verts.push(p(c[0], c[1], c[2]));
            verts.push(p(c[0] + e, c[1] + next() * e, c[2] + next() * e));
            verts.push(p(c[0] + next() * e, c[1] + e, c[2] - next() * e));
            tris.push([b, b + 1, b + 2]);
        }
        let soup = FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap();
        let tree = detect_intersecting_pairs(&soup);
        let brute = detect_intersecting_pairs_brute(&soup);
        assert!(
            brute.len() > 200,
            "the soup must exercise the test: {}",
            brute.len()
        );
        assert_eq!(tree, brute);
    }

    /// A closed mesh alone is a valid simplicial complex: every pair of its
    /// triangles meets at a shared vertex / edge or not at all, so the
    /// reference reports NO pair (cinolib `ignore_if_valid_complex`); the
    /// full test alone reports every neighbour.
    #[test]
    fn closed_mesh_alone_reports_no_pairs() {
        let c = [
            p(0.0, 0.0, 0.0),
            p(1.0, 0.0, 0.0),
            p(1.0, 1.0, 0.0),
            p(0.0, 1.0, 0.0),
            p(0.0, 0.0, 1.0),
            p(1.0, 0.0, 1.0),
            p(1.0, 1.0, 1.0),
            p(0.0, 1.0, 1.0),
        ];
        let tris = vec![
            [0u32, 2, 1],
            [0, 3, 2],
            [4, 5, 6],
            [4, 6, 7],
            [0, 1, 5],
            [0, 5, 4],
            [3, 7, 6],
            [3, 6, 2],
            [0, 4, 7],
            [0, 7, 3],
            [1, 2, 6],
            [1, 6, 5],
        ];
        let soup = FastTrimesh::from_soup(&c, &tris, Plane::XY).unwrap();
        assert_eq!(detect_intersecting_pairs(&soup), vec![]);
        let full = without_valid_complex_skip(|| detect_intersecting_pairs(&soup));
        assert!(full.len() > 30, "every neighbour pair: {}", full.len());
    }

    /// A fold — two triangles sharing an edge, coplanar, on the SAME side
    /// of it — and a shared-vertex pair whose opposite edge pierces the
    /// other triangle are both still reported (cinolib `INTERSECT`).
    #[test]
    fn shared_simplex_overlaps_are_still_reported() {
        let verts = vec![
            p(0.0, 0.0, 0.0),
            p(2.0, 0.0, 0.0),
            p(1.0, 1.0, 0.0),
            p(1.0, 0.5, 0.0),
            // shared-vertex piercer: vertex 0 shared, opposite edge crosses tri 0
            p(1.0, 0.3, -1.0),
            p(1.0, 0.3, 1.0),
        ];
        let tris = vec![[0u32, 1, 2], [0, 1, 3], [0, 4, 5]];
        let soup = FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap();
        let pairs = detect_intersecting_pairs(&soup);
        assert!(pairs.contains(&(0, 1)), "coplanar fold: {pairs:?}");
        assert!(pairs.contains(&(0, 2)), "shared-vertex piercer: {pairs:?}");
    }

    // -----------------------------------------------------------------
    // Group 1 — Boundary conditions
    // -----------------------------------------------------------------

    #[test]
    fn empty_mesh_has_no_pairs() {
        let soup = FastTrimesh::from_soup(&[], &[], Plane::XY).unwrap();
        assert_eq!(detect_intersecting_pairs(&soup), vec![]);
    }

    #[test]
    fn single_tri_has_no_pairs() {
        let verts = vec![p(0.0, 0.0, 0.0), p(1.0, 0.0, 0.0), p(0.0, 1.0, 0.0)];
        let tris = vec![[0u32, 1, 2]];
        let soup = FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap();
        assert_eq!(detect_intersecting_pairs(&soup), vec![]);
    }

    #[test]
    fn far_apart_tris_yield_no_pairs() {
        let verts = vec![
            // Tri 0 at origin
            p(0.0, 0.0, 0.0),
            p(1.0, 0.0, 0.0),
            p(0.0, 1.0, 0.0),
            // Tri 1 at (10, 10, 10) — AABBs don't overlap
            p(10.0, 10.0, 10.0),
            p(11.0, 10.0, 10.0),
            p(10.0, 11.0, 10.0),
        ];
        let tris = vec![[0u32, 1, 2], [3, 4, 5]];
        let soup = FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap();
        assert_eq!(detect_intersecting_pairs(&soup), vec![]);
    }

    // -----------------------------------------------------------------
    // Group 2 — Disjoint triangles
    // -----------------------------------------------------------------

    #[test]
    fn parallel_planes_no_pairs() {
        // Two triangles in z=0 and z=1; AABBs overlap only if their
        // x,y projections overlap. Place them with overlapping x,y so
        // AABBs DO overlap but geometry is disjoint.
        let verts = vec![
            p(0.0, 0.0, 0.0),
            p(2.0, 0.0, 0.0),
            p(0.0, 2.0, 0.0),
            p(0.0, 0.0, 1.0),
            p(2.0, 0.0, 1.0),
            p(0.0, 2.0, 1.0),
        ];
        let tris = vec![[0u32, 1, 2], [3, 4, 5]];
        let soup = FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap();
        assert_eq!(detect_intersecting_pairs(&soup), vec![]);
    }

    #[test]
    fn perpendicular_separated_no_pairs() {
        // Tri 0 in XY plane at origin; Tri 1 in XZ plane far away.
        let verts = vec![
            p(0.0, 0.0, 0.0),
            p(1.0, 0.0, 0.0),
            p(0.0, 1.0, 0.0),
            p(5.0, 0.0, 0.0),
            p(6.0, 0.0, 0.0),
            p(5.0, 0.0, 1.0),
        ];
        let tris = vec![[0u32, 1, 2], [3, 4, 5]];
        let soup = FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap();
        assert_eq!(detect_intersecting_pairs(&soup), vec![]);
    }

    // -----------------------------------------------------------------
    // Group 3 — Intersecting triangles
    // -----------------------------------------------------------------

    #[test]
    fn crossing_in_3d_one_pair() {
        // Two triangles crossing through each other: one in XY plane
        // through origin, one in YZ plane intersecting it along Y axis.
        let verts = vec![
            // Tri 0 in XY (z=0)
            p(-1.0, -1.0, 0.0),
            p(1.0, -1.0, 0.0),
            p(0.0, 1.0, 0.0),
            // Tri 1 in YZ-ish plane crossing through tri 0
            p(0.0, -1.0, -1.0),
            p(0.0, -1.0, 1.0),
            p(0.0, 1.0, 0.0),
        ];
        let tris = vec![[0u32, 1, 2], [3, 4, 5]];
        let soup = FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap();
        assert_eq!(detect_intersecting_pairs(&soup), vec![(0, 1)]);
    }

    #[test]
    fn edge_touching_one_pair() {
        // Two coplanar triangles sharing edge (0, 1) on opposite sides of
        // it. CR9 returns Intersects; the reference's detection predicate
        // calls the pair a valid complex and does not report it.
        let verts = vec![
            p(0.0, 0.0, 0.0),
            p(1.0, 0.0, 0.0),
            p(0.0, 1.0, 0.0),
            p(0.0, -1.0, 0.0),
        ];
        let tris = vec![[0u32, 1, 2], [0, 1, 3]];
        let soup = FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap();
        assert_eq!(detect_intersecting_pairs(&soup), vec![]);
        assert_eq!(
            without_valid_complex_skip(|| detect_intersecting_pairs(&soup)),
            vec![(0, 1)]
        );
    }

    #[test]
    fn vertex_touching_one_pair() {
        // Two triangles sharing only vertex 0.
        let verts = vec![
            p(0.0, 0.0, 0.0),
            p(1.0, 0.0, 0.0),
            p(0.0, 1.0, 0.0),
            p(-1.0, 0.0, 0.0),
            p(0.0, -1.0, 0.0),
        ];
        let tris = vec![[0u32, 1, 2], [0, 3, 4]];
        let soup = FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap();
        assert_eq!(detect_intersecting_pairs(&soup), vec![(0, 1)]);
    }

    #[test]
    fn t_junction_one_pair() {
        // Tri 1's vertex 3 lies on tri 0's edge (0, 1).
        let verts = vec![
            p(0.0, 0.0, 0.0),
            p(2.0, 0.0, 0.0),
            p(1.0, 2.0, 0.0),
            p(1.0, 0.0, 0.0), // on edge (0, 1)
            p(1.0, -1.0, 1.0),
            p(2.0, -1.0, 1.0),
        ];
        let tris = vec![[0u32, 1, 2], [3, 4, 5]];
        let soup = FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap();
        assert_eq!(detect_intersecting_pairs(&soup), vec![(0, 1)]);
    }

    // -----------------------------------------------------------------
    // Group 4 — Coplanar pairs
    // -----------------------------------------------------------------

    #[test]
    fn coplanar_overlapping_one_pair() {
        // Two overlapping triangles in z=0 plane.
        let verts = vec![
            p(0.0, 0.0, 0.0),
            p(2.0, 0.0, 0.0),
            p(0.0, 2.0, 0.0),
            p(1.0, 1.0, 0.0),
            p(3.0, 1.0, 0.0),
            p(1.0, 3.0, 0.0),
        ];
        let tris = vec![[0u32, 1, 2], [3, 4, 5]];
        let soup = FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap();
        assert_eq!(detect_intersecting_pairs(&soup), vec![(0, 1)]);
    }

    #[test]
    fn coplanar_shared_edge_one_pair() {
        let verts = vec![
            p(0.0, 0.0, 0.0),
            p(1.0, 0.0, 0.0),
            p(0.0, 1.0, 0.0),
            p(1.0, 1.0, 0.0),
        ];
        let tris = vec![[0u32, 1, 2], [1, 3, 2]];
        let soup = FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap();
        // Shared edge (1, 2), opposite sides: a valid complex (reference).
        assert_eq!(detect_intersecting_pairs(&soup), vec![]);
        assert_eq!(
            without_valid_complex_skip(|| detect_intersecting_pairs(&soup)),
            vec![(0, 1)]
        );
    }

    #[test]
    fn coplanar_shared_vertex_one_pair() {
        // Two coplanar triangles touching only at vertex 0.
        let verts = vec![
            p(0.0, 0.0, 0.0),
            p(1.0, 0.0, 0.0),
            p(0.0, 1.0, 0.0),
            p(-1.0, 0.0, 0.0),
            p(0.0, -1.0, 0.0),
        ];
        let tris = vec![[0u32, 1, 2], [0, 3, 4]];
        let soup = FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap();
        assert_eq!(detect_intersecting_pairs(&soup), vec![(0, 1)]);
    }

    // -----------------------------------------------------------------
    // Group 5 — Multi-triangle meshes
    // -----------------------------------------------------------------

    #[test]
    fn tetrahedron_all_face_pairs_share_edges() {
        // Tetrahedron has 4 faces; every pair of faces shares an edge.
        // CR9 treats shared-edge pairs as Intersects (all 6 under the full
        // test); the reference's detection predicate reports none — a
        // closed tetrahedron is a valid simplicial complex.
        let verts = vec![
            p(0.0, 0.0, 0.0),
            p(1.0, 0.0, 0.0),
            p(0.0, 1.0, 0.0),
            p(0.0, 0.0, 1.0),
        ];
        let tris = vec![[0u32, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]];
        let soup = FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap();
        assert!(detect_intersecting_pairs(&soup).is_empty());
        let pairs = without_valid_complex_skip(|| detect_intersecting_pairs(&soup));
        // 4 choose 2 = 6 pairs; all share an edge.
        assert_eq!(pairs.len(), 6);
    }

    #[test]
    fn two_cubes_offset_have_pairs() {
        // Two unit cubes, second offset 0.5 along X — guaranteed
        // non-trivial intersection. Just confirm the pair count is > 0.
        let mut verts = Vec::new();
        let mut tris: Vec<[u32; 3]> = Vec::new();
        for (i, origin) in [[0.0, 0.0, 0.0], [0.5, 0.0, 0.0]].iter().enumerate() {
            let (cv, ct) = unit_cube_at(*origin);
            let base = (i * 8) as u32;
            verts.extend(cv);
            for t in &ct {
                tris.push([t[0] + base, t[1] + base, t[2] + base]);
            }
        }
        let soup = FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap();
        let pairs = detect_intersecting_pairs(&soup);
        assert!(
            !pairs.is_empty(),
            "two overlapping cubes should produce pairs"
        );
    }

    fn unit_cube_at(origin: [f64; 3]) -> (Vec<Point3>, Vec<[u32; 3]>) {
        let [x, y, z] = origin;
        let v = vec![
            p(x, y, z),
            p(x + 1.0, y, z),
            p(x + 1.0, y + 1.0, z),
            p(x, y + 1.0, z),
            p(x, y, z + 1.0),
            p(x + 1.0, y, z + 1.0),
            p(x + 1.0, y + 1.0, z + 1.0),
            p(x, y + 1.0, z + 1.0),
        ];
        let t = vec![
            [0, 3, 2],
            [0, 2, 1],
            [4, 5, 6],
            [4, 6, 7],
            [0, 1, 5],
            [0, 5, 4],
            [2, 3, 7],
            [2, 7, 6],
            [1, 2, 6],
            [1, 6, 5],
            [0, 4, 7],
            [0, 7, 3],
        ];
        (v, t)
    }

    #[test]
    fn star_of_david_multiple_pairs() {
        // Two coplanar overlapping triangles (star pattern).
        let verts = vec![
            p(0.0, 0.0, 0.0),
            p(3.0, 0.0, 0.0),
            p(1.5, 3.0, 0.0),
            p(0.0, 2.0, 0.0),
            p(3.0, 2.0, 0.0),
            p(1.5, -1.0, 0.0),
        ];
        let tris = vec![[0u32, 1, 2], [3, 4, 5]];
        let soup = FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap();
        let pairs = detect_intersecting_pairs(&soup);
        // The two triangles are coplanar and overlap → 1 pair.
        assert_eq!(pairs, vec![(0, 1)]);
    }

    #[test]
    fn five_disjoint_random_tris_no_pairs() {
        // 5 triangles widely separated in space.
        let verts = vec![
            p(0.0, 0.0, 0.0),
            p(1.0, 0.0, 0.0),
            p(0.0, 1.0, 0.0),
            p(10.0, 0.0, 0.0),
            p(11.0, 0.0, 0.0),
            p(10.0, 1.0, 0.0),
            p(0.0, 10.0, 0.0),
            p(1.0, 10.0, 0.0),
            p(0.0, 11.0, 0.0),
            p(0.0, 0.0, 10.0),
            p(1.0, 0.0, 10.0),
            p(0.0, 1.0, 10.0),
            p(10.0, 10.0, 10.0),
            p(11.0, 10.0, 10.0),
            p(10.0, 11.0, 10.0),
        ];
        let tris = vec![
            [0u32, 1, 2],
            [3, 4, 5],
            [6, 7, 8],
            [9, 10, 11],
            [12, 13, 14],
        ];
        let soup = FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap();
        assert_eq!(detect_intersecting_pairs(&soup), vec![]);
    }

    #[test]
    fn non_manifold_three_tris_all_paired() {
        // Three triangles sharing edge (0, 1). CR9 reports Intersects
        // for every pair (shared edge); the reference reports none (two
        // are coplanar on opposite sides of the edge, the third is
        // transversal to both — each pair a valid complex).
        let verts = vec![
            p(0.0, 0.0, 0.0),
            p(1.0, 0.0, 0.0),
            p(0.0, 1.0, 0.0),
            p(0.0, -1.0, 0.0),
            p(0.0, 0.0, 1.0),
        ];
        let tris = vec![[0u32, 1, 2], [0, 1, 3], [0, 1, 4]];
        let soup = FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap();
        assert!(detect_intersecting_pairs(&soup).is_empty());
        let pairs = without_valid_complex_skip(|| detect_intersecting_pairs(&soup));
        // 3 choose 2 = 3 pairs, all sharing edge (0, 1).
        assert_eq!(pairs.len(), 3);
    }

    // -----------------------------------------------------------------
    // Group 6 — Pair invariants
    // -----------------------------------------------------------------

    #[test]
    fn pairs_are_sorted_ascending() {
        let verts = vec![
            p(0.0, 0.0, 0.0),
            p(1.0, 0.0, 0.0),
            p(0.0, 1.0, 0.0),
            p(0.0, 0.0, 1.0),
        ];
        let tris = vec![[0u32, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]];
        let soup = FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap();
        let pairs = detect_intersecting_pairs(&soup);
        for &(a, b) in &pairs {
            assert!(a < b, "pair ({a}, {b}) violates a < b");
        }
    }

    #[test]
    fn no_duplicate_pairs() {
        let verts = vec![
            p(0.0, 0.0, 0.0),
            p(1.0, 0.0, 0.0),
            p(0.0, 1.0, 0.0),
            p(0.0, 0.0, 1.0),
        ];
        let tris = vec![[0u32, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]];
        let soup = FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap();
        let mut pairs = detect_intersecting_pairs(&soup);
        let original_len = pairs.len();
        pairs.sort();
        pairs.dedup();
        assert_eq!(pairs.len(), original_len);
    }

    #[test]
    fn empty_iff_no_intersections() {
        // A single triangle has no pairs (no intersections possible).
        let verts = vec![p(0.0, 0.0, 0.0), p(1.0, 0.0, 0.0), p(0.0, 1.0, 0.0)];
        let tris = vec![[0u32, 1, 2]];
        let soup = FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap();
        assert!(detect_intersecting_pairs(&soup).is_empty());
    }

    // -----------------------------------------------------------------
    // Group 7 — Property test against brute-force
    // -----------------------------------------------------------------

    /// Property: AABB pre-pruning is conservative. Every pair the
    /// pruned algorithm returns must also be in the brute-force
    /// result. The reverse direction (`brute ⊆ pruned`) does NOT
    /// hold: CR9's `Coplanar` return is triggered whenever one
    /// triangle has an edge in the other's plane, even when the
    /// triangles are far apart spatially (their AABBs don't overlap).
    /// AABB pre-pruning correctly filters these spurious pairs out —
    /// matching upstream's spatial-pruned detection. So our pruned
    /// version is a STRICT improvement over brute-force, not just
    /// faster.
    fn assert_pruned_subset_of_brute(soup: &FastTrimesh) {
        use std::collections::HashSet;
        let pruned = detect_intersecting_pairs(soup);
        let brute: HashSet<(u32, u32)> = brute_force_pairs(soup).into_iter().collect();
        for &pair in &pruned {
            assert!(
                brute.contains(&pair),
                "pruned has pair {:?} not in brute-force result",
                pair
            );
        }
    }

    #[test]
    fn property_pruned_subset_of_brute_force() {
        // Tetrahedron — every face pair intersects.
        let verts = vec![
            p(0.0, 0.0, 0.0),
            p(1.0, 0.0, 0.0),
            p(0.0, 1.0, 0.0),
            p(0.0, 0.0, 1.0),
        ];
        let tris = vec![[0u32, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]];
        let tetra = FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap();
        assert_pruned_subset_of_brute(&tetra);

        // Two cubes offset — has many CR9 Coplanar-via-edge-in-plane
        // false positives that AABB correctly filters out. The
        // subset invariant still holds.
        let mut verts = Vec::new();
        let mut tris: Vec<[u32; 3]> = Vec::new();
        for (i, origin) in [[0.0, 0.0, 0.0], [0.5, 0.0, 0.0]].iter().enumerate() {
            let (cv, ct) = unit_cube_at(*origin);
            let base = (i * 8) as u32;
            verts.extend(cv);
            for t in &ct {
                tris.push([t[0] + base, t[1] + base, t[2] + base]);
            }
        }
        let two_cubes = FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap();
        assert_pruned_subset_of_brute(&two_cubes);

        // Empty mesh
        let empty = FastTrimesh::from_soup(&[], &[], Plane::XY).unwrap();
        assert_pruned_subset_of_brute(&empty);

        // 5 disjoint tris
        let verts = vec![
            p(0.0, 0.0, 0.0),
            p(1.0, 0.0, 0.0),
            p(0.0, 1.0, 0.0),
            p(10.0, 0.0, 0.0),
            p(11.0, 0.0, 0.0),
            p(10.0, 1.0, 0.0),
            p(0.0, 10.0, 0.0),
            p(1.0, 10.0, 0.0),
            p(0.0, 11.0, 0.0),
            p(0.0, 0.0, 10.0),
            p(1.0, 0.0, 10.0),
            p(0.0, 1.0, 10.0),
            p(5.0, 5.0, 5.0),
            p(6.0, 5.0, 5.0),
            p(5.0, 6.0, 5.0),
        ];
        let tris = vec![
            [0u32, 1, 2],
            [3, 4, 5],
            [6, 7, 8],
            [9, 10, 11],
            [12, 13, 14],
        ];
        let disjoint = FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap();
        assert_pruned_subset_of_brute(&disjoint);

        // Coplanar overlapping + disjoint mix
        let verts = vec![
            p(0.0, 0.0, 0.0),
            p(2.0, 0.0, 0.0),
            p(0.0, 2.0, 0.0),
            p(1.0, 1.0, 0.0),
            p(3.0, 1.0, 0.0),
            p(1.0, 3.0, 0.0),
            p(10.0, 0.0, 0.0),
            p(11.0, 0.0, 0.0),
            p(10.0, 1.0, 0.0),
        ];
        let tris = vec![[0u32, 1, 2], [3, 4, 5], [6, 7, 8]];
        let mixed = FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap();
        assert_pruned_subset_of_brute(&mixed);
    }
}
