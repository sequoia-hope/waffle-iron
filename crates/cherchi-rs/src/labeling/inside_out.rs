//! Ray-cast in/out classification (PR-CR-BL2) — Cherchi 2022 §5, step 2.
//!
//! Ported from Cherchi et al. 2020 / 2022 (MIT).
//! © Gianmarco Cherchi et al.
//! https://github.com/gcherchi/FastAndRobustMeshArrangements
//! https://github.com/gcherchi/InteractiveAndRobustMeshBooleans
//! Source: `code/booleans.cpp::computeInsideOut` / `findRayEndpoints` /
//! `pruneIntersectionsAndSortAlongRay` / `analyzeSortedIntersections` /
//! `perturbRayAndFindIntersTri` + helpers.
//!
//! For every BL1 patch, shoot an axis-aligned ray from a vertex of the
//! patch and count which OTHER input solids contain it: the ray is tested
//! against the prepped ORIGINAL input triangles (`soup.in_tris`, the C++
//! `arr_in_tris` — closed shells), the hits are sorted exactly along the
//! ray (each hit is an LPI implicit point compared with `lessThanOnX/Y/Z`),
//! and the NEAREST hit per input label decides in/out by triangle
//! orientation (back-face first → the patch is inside that input).
//!
//! ## Cycle A scope (this slice)
//!
//! - Ray origins from EXPLICIT non-border patch vertices only. A fully
//!   implicit patch (every non-border vertex is LPI/TPI) returns the loud
//!   [`InsideOutError::FullyImplicitPatch`] — the C++ "generated ray"
//!   branch is Cycle B.
//! - Candidate triangles are brute-force: ALL `in_tris` are offered to the
//!   exact prune (the C++ octree is a pure acceleration structure feeding
//!   a superset; Cycle C adds it with a pruned ⊆ brute oracle).
//! - Vertex/edge ray-hits are resolved by `nextafter` ray perturbation
//!   over the hit element's incident input triangles, as in the C++.
//!
//! Port deviations (documented in `docs/yang_deviations.md`):
//! - Serial per-patch loop (crate rule #5; C++ is TBB-parallel).
//! - The C++ `btree_set` sort drops hits whose LPI points compare EQUAL
//!   on the ray axis; the port keeps the same set semantics explicitly.
//! - The C++ `std::exit` on a fully implicit patch is a typed error.
//! - Labels are `Vec<InputId>` sets, not `bitset<NBIT>`.

use cad_primitives::Point3;

use crate::arrangements::soup::{ArrangementSoup, Label};
use crate::labeling::patches::Patches;

/// Axis an in/out ray travels along (toward +axis).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
    Z,
}

/// An axis-aligned in/out ray: `v0` = origin (a patch vertex), `v1` = the
/// far endpoint past the global bbox (`max_coords + 0.5` in the C++).
#[derive(Debug, Clone)]
pub struct Ray {
    pub v0: Point3,
    pub v1: Point3,
    pub dir: Axis,
}

/// Loud failure surface — never silent (P9/P10).
#[derive(Debug, PartialEq, Eq)]
pub enum InsideOutError {
    /// The soup carries no prepped input triangles (`in_tris` empty) while
    /// patches exist — the arrangement predates the BL2 soup extension.
    MissingInputTris,
    /// A patch has no triangles (upstream BL1 invariant violation).
    EmptyPatch { patch: u32 },
    /// Every usable vertex of the patch is implicit (LPI/TPI) — the C++
    /// "generated ray" branch, deferred to Cycle B.
    FullyImplicitPatch { patch: u32 },
    /// All `nextafter` ray perturbations failed to produce a single clean
    /// interior hit on a vertex/edge-hit's incident triangles.
    PerturbationExhausted { patch: u32 },
    /// `orient3d(tri, ray.v1)` was Zero when classifying the nearest hit —
    /// the C++ asserts non-zero here.
    DegenerateOrientation { patch: u32, tri: u32 },
}

/// Port of `computeInsideOut` (booleans.cpp:621), serial: for each patch,
/// the sorted-ray-hit walk produces the patch's *inner label* — the set of
/// OTHER inputs that strictly contain it. Returns one `Label` per patch
/// (sorted, deduped; never contains the patch's own surface label).
pub fn compute_inside_out(
    soup: &ArrangementSoup,
    patches: &Patches,
) -> Result<Vec<Label>, InsideOutError> {
    // GREEN lands in the next commit; RED stub fails every oracle loudly.
    let _ = (soup, patches);
    Ok(Vec::new())
}

// =========================================================================
// RED oracle tests (PR-CR-BL2 Cycle A)
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arrangements::fast_trimesh::VertexCoords;
    use crate::arrangements::soup::mesh_arrangement;
    use crate::labeled_arrangement::InputId;
    use crate::labeling::patches::compute_all_patches;
    use dashu::rational::RBig;
    use std::collections::BTreeSet;

    const A: InputId = InputId(0);
    const B: InputId = InputId(1);

    // ----- fixtures (the BL1 suite's geometry) ----------------------------

    fn cube(
        ox: f64,
        oy: f64,
        oz: f64,
        s: f64,
        label: InputId,
    ) -> (Vec<f64>, Vec<[u32; 3]>, Vec<Label>) {
        let p = |x: f64, y: f64, z: f64| (ox + x * s, oy + y * s, oz + z * s);
        let corners = [
            p(0.0, 0.0, 0.0),
            p(1.0, 0.0, 0.0),
            p(1.0, 1.0, 0.0),
            p(0.0, 1.0, 0.0),
            p(0.0, 0.0, 1.0),
            p(1.0, 0.0, 1.0),
            p(1.0, 1.0, 1.0),
            p(0.0, 1.0, 1.0),
        ];
        let mut coords = Vec::with_capacity(24);
        for (x, y, z) in corners {
            coords.push(x);
            coords.push(y);
            coords.push(z);
        }
        let tris = vec![
            [0, 2, 1],
            [0, 3, 2],
            [4, 5, 6],
            [4, 6, 7],
            [0, 1, 5],
            [0, 5, 4],
            [2, 3, 7],
            [2, 7, 6],
            [1, 2, 6],
            [1, 6, 5],
            [3, 0, 4],
            [3, 4, 7],
        ];
        let labels = vec![vec![label]; tris.len()];
        (coords, tris, labels)
    }

    fn concat(
        s0: (Vec<f64>, Vec<[u32; 3]>, Vec<Label>),
        s1: (Vec<f64>, Vec<[u32; 3]>, Vec<Label>),
    ) -> (Vec<f64>, Vec<[u32; 3]>, Vec<Label>) {
        let (mut coords, mut tris, mut labels) = s0;
        let off = (coords.len() / 3) as u32;
        coords.extend_from_slice(&s1.0);
        for t in s1.1 {
            tris.push([t[0] + off, t[1] + off, t[2] + off]);
        }
        labels.extend(s1.2);
        (coords, tris, labels)
    }

    fn arrange(
        s0: (Vec<f64>, Vec<[u32; 3]>, Vec<Label>),
        s1: (Vec<f64>, Vec<[u32; 3]>, Vec<Label>),
    ) -> ArrangementSoup {
        crate::arrangements::require_ffi_shim();
        let (coords, tris, labels) = concat(s0, s1);
        mesh_arrangement(&coords, &tris, &labels).expect("arrangement")
    }

    // ----- independent coordinate resolution (oracle-side) ----------------
    // Pure-dashu line-plane intersection for Lpi; trilinear plane-plane-plane
    // is not needed by these fixtures (no TPI on axis-perpendicular cuts).

    fn to_r(x: f64) -> RBig {
        RBig::simplest_from_f64(x).expect("finite")
    }

    fn r3(p: Point3) -> [RBig; 3] {
        [to_r(p.x()), to_r(p.y()), to_r(p.z())]
    }

    fn approx_coords(c: &VertexCoords) -> [f64; 3] {
        match c {
            VertexCoords::Explicit(p) => [p.x(), p.y(), p.z()],
            VertexCoords::Lpi { line, plane } => {
                let [p, q] = [r3(line[0]), r3(line[1])];
                let [a, b, c3] = [r3(plane[0]), r3(plane[1]), r3(plane[2])];
                let sub =
                    |u: &[RBig; 3], v: &[RBig; 3]| [&u[0] - &v[0], &u[1] - &v[1], &u[2] - &v[2]];
                let cross = |u: &[RBig; 3], v: &[RBig; 3]| {
                    [
                        &u[1] * &v[2] - &u[2] * &v[1],
                        &u[2] * &v[0] - &u[0] * &v[2],
                        &u[0] * &v[1] - &u[1] * &v[0],
                    ]
                };
                let dot =
                    |u: &[RBig; 3], v: &[RBig; 3]| &u[0] * &v[0] + &u[1] * &v[1] + &u[2] * &v[2];
                let n = cross(&sub(&b, &a), &sub(&c3, &a));
                let d = dot(&n, &sub(&q, &p));
                assert!(d != RBig::ZERO, "oracle: line parallel to plane");
                let t = dot(&n, &sub(&a, &p)) / d;
                let lerp = |i: usize| &p[i] + &t * (&q[i] - &p[i]);
                [
                    lerp(0).to_f64().value(),
                    lerp(1).to_f64().value(),
                    lerp(2).to_f64().value(),
                ]
            }
            VertexCoords::Tpi { .. } => {
                panic!("oracle: fixtures must not produce TPI vertices")
            }
        }
    }

    /// Scaled open-AABB of one input's prepped triangles (truth source for
    /// "inside" on box fixtures — convex, axis-aligned).
    fn input_aabb(soup: &ArrangementSoup, label: InputId) -> ([f64; 3], [f64; 3]) {
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for (t, tri) in soup.in_tris.iter().enumerate() {
            if !soup.in_labels[t].contains(&label) {
                continue;
            }
            for &v in tri {
                let p = approx_coords(&soup.verts[v as usize]);
                for k in 0..3 {
                    lo[k] = lo[k].min(p[k]);
                    hi[k] = hi[k].max(p[k]);
                }
            }
        }
        (lo, hi)
    }

    /// Truth: does this patch sit strictly inside `label`'s box? Decided by
    /// the patch's triangle centroids vs the input's scaled AABB (strict,
    /// with a relative margin so border-loop vertices never flip it).
    fn patch_inside_box(soup: &ArrangementSoup, patch: &[u32], label: InputId) -> bool {
        let (lo, hi) = input_aabb(soup, label);
        let eps: f64 = (0..3).map(|k| hi[k] - lo[k]).fold(0.0, f64::max) * 1e-9;
        patch.iter().all(|&t| {
            let tri = soup.tris[t as usize];
            let mut c = [0.0; 3];
            for &v in &tri {
                let p = approx_coords(&soup.verts[v as usize]);
                for k in 0..3 {
                    c[k] += p[k] / 3.0;
                }
            }
            (0..3).all(|k| c[k] > lo[k] + eps && c[k] < hi[k] - eps)
        })
    }

    fn canonical(l: &Label) -> Label {
        let mut l = l.clone();
        l.sort_unstable();
        l
    }

    // ════════════════════════════════════════════════════════════════
    // Oracle #1 — corner-overlapping cubes: each patch's inner label
    // matches the geometric truth (centroids strictly inside the other
    // box ⇔ inner = {other}; else inner = ∅). Exercises ray casting,
    // exact sorting, and (axis-aligned fixture) perturbation paths.
    // ════════════════════════════════════════════════════════════════
    #[test]
    fn cut_boxes_inner_labels_match_geometry() {
        let soup = arrange(cube(0.0, 0.0, 0.0, 2.0, A), cube(1.0, 1.0, 1.0, 2.0, B));
        let patches = compute_all_patches(&soup).expect("patches");
        let inner = compute_inside_out(&soup, &patches).expect("inside_out");

        assert_eq!(
            inner.len(),
            patches.patches.len(),
            "one inner label per patch"
        );
        let mut inside_seen = 0;
        for (pi, patch) in patches.patches.iter().enumerate() {
            let own = canonical(&soup.labels[patch[0] as usize]);
            let other = if own == vec![A] { B } else { A };
            let expect: Label = if patch_inside_box(&soup, patch, other) {
                inside_seen += 1;
                vec![other]
            } else {
                vec![]
            };
            assert_eq!(
                canonical(&inner[pi]),
                expect,
                "patch {pi} (own label {own:?}): inner label vs geometric truth"
            );
        }
        assert!(
            inside_seen >= 2,
            "fixture sanity: at least one inside patch per solid (got {inside_seen})"
        );
    }

    // ════════════════════════════════════════════════════════════════
    // Oracle #2 — enclosed cube: B's whole shell is inside A; A's shell
    // is outside B.
    // ════════════════════════════════════════════════════════════════
    #[test]
    fn enclosed_cube_is_inside_outer() {
        let soup = arrange(cube(0.0, 0.0, 0.0, 2.0, A), cube(0.5, 0.5, 0.5, 1.0, B));
        let patches = compute_all_patches(&soup).expect("patches");
        let inner = compute_inside_out(&soup, &patches).expect("inside_out");

        assert_eq!(patches.patches.len(), 2);
        for (pi, patch) in patches.patches.iter().enumerate() {
            let own = canonical(&soup.labels[patch[0] as usize]);
            let expect: Label = if own == vec![B] { vec![A] } else { vec![] };
            assert_eq!(canonical(&inner[pi]), expect, "patch {pi} of {own:?}");
        }
    }

    // ════════════════════════════════════════════════════════════════
    // Oracle #3 — disjoint cubes: nothing is inside anything.
    // ════════════════════════════════════════════════════════════════
    #[test]
    fn disjoint_cubes_are_all_outside() {
        let soup = arrange(cube(0.0, 0.0, 0.0, 1.0, A), cube(5.0, 5.0, 5.0, 1.0, B));
        let patches = compute_all_patches(&soup).expect("patches");
        let inner = compute_inside_out(&soup, &patches).expect("inside_out");

        assert_eq!(inner.len(), 2);
        assert!(
            inner.iter().all(|l| l.is_empty()),
            "disjoint solids: every inner label empty, got {inner:?}"
        );
    }

    // ════════════════════════════════════════════════════════════════
    // Oracle #4 — structural invariants on every fixture: inner labels
    // never contain the patch's own surface label; inner labels are
    // sorted + deduped; deterministic across runs.
    // ════════════════════════════════════════════════════════════════
    #[test]
    fn structural_invariants_and_determinism() {
        let soup = arrange(cube(0.0, 0.0, 0.0, 2.0, A), cube(1.0, 1.0, 1.0, 2.0, B));
        let patches = compute_all_patches(&soup).expect("patches");
        let inner1 = compute_inside_out(&soup, &patches).expect("inside_out");
        let inner2 = compute_inside_out(&soup, &patches).expect("inside_out");
        assert_eq!(inner1, inner2, "same input → identical inner labels");
        assert_eq!(inner1.len(), patches.patches.len());

        for (pi, patch) in patches.patches.iter().enumerate() {
            let own: BTreeSet<InputId> = soup.labels[patch[0] as usize].iter().copied().collect();
            for id in &inner1[pi] {
                assert!(
                    !own.contains(id),
                    "patch {pi}: inner label contains own surface label {id:?}"
                );
            }
            let mut sorted = inner1[pi].clone();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(
                inner1[pi], sorted,
                "patch {pi}: inner label sorted + deduped"
            );
        }
    }

    // ════════════════════════════════════════════════════════════════
    // Oracle #5 — loud error paths: a soup without prepped input tris
    // must not silently classify everything as outside.
    // ════════════════════════════════════════════════════════════════
    #[test]
    fn missing_input_tris_is_loud() {
        let soup = arrange(cube(0.0, 0.0, 0.0, 2.0, A), cube(1.0, 1.0, 1.0, 2.0, B));
        let patches = compute_all_patches(&soup).expect("patches");
        let broken = ArrangementSoup {
            in_tris: Vec::new(),
            in_labels: Vec::new(),
            ..soup
        };
        match compute_inside_out(&broken, &patches) {
            Err(InsideOutError::MissingInputTris) => {}
            other => panic!("expected MissingInputTris, got {other:?}"),
        }
    }
}
