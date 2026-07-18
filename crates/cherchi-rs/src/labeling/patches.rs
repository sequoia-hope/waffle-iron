//! Patch flood-fill (PR-CR-BL1) — Cherchi 2022 §5, step 1.
//!
//! Ported from Cherchi et al. 2020 / 2022 (MIT).
//! © Gianmarco Cherchi et al.
//! https://github.com/gcherchi/FastAndRobustMeshArrangements
//! https://github.com/gcherchi/InteractiveAndRobustMeshBooleans
//! Source: `code/booleans.cpp::computeAllPatches` / `computeSinglePatch`.
//!
//! A *patch* is a maximal edge-connected set of arrangement triangles that
//! does not cross a non-manifold edge. After the AR3b arrangement, every
//! intersection segment between the two inputs is realized as mesh edges
//! shared by ≥ 3 triangles (typically 4: two from each input), so patches
//! are exactly the surface pieces the intersection curves cut the inputs
//! into. All triangles of a patch carry the same surface label (asserted
//! in the C++; a loud [`PatchError::LabelMismatch`] here).
//!
//! Port deviations (documented in `docs/yang_deviations.md`):
//! - The C++ floods over a global `FastTrimesh` rebuilt from the
//!   arrangement output. The Rust `FastTrimesh` port is per-base-triangle
//!   (its `from_soup` takes a projection `Plane`), so this module builds
//!   the same edge→incident-triangles adjacency directly from the soup's
//!   triangle list. Flood semantics are identical: an edge is manifold
//!   iff it has ≤ 2 incident triangles (cinolib `edgeIsManifold`).
//! - The C++ parallel variant (`adjT2E` precompute + TBB) is NOT ported —
//!   crate hard-rule #5 (single-threaded; determinism over speed). Only
//!   the serial `computeSinglePatch` overload is ported.
//! - `phmap::flat_hash_set<uint>` patches become sorted `Vec<u32>`
//!   (deterministic iteration; the C++ relies on hash-set order nowhere).
//! - Border-vertex marking (`tm.setVertInfo(v, 1)`) becomes the returned
//!   sorted `border_verts` vec — the BL2 `findRayEndpoints` consumer reads
//!   it instead of mutating shared mesh state.

use std::collections::{BTreeMap, BTreeSet};

use crate::arrangements::soup::{ArrangementSoup, Label};

/// Result of the patch flood-fill over an arrangement soup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Patches {
    /// Each patch as a sorted list of triangle ids. Patch order is the
    /// C++ seed order: ascending scan over triangle ids, a new patch per
    /// not-yet-visited seed.
    pub patches: Vec<Vec<u32>>,
    /// Triangle id → index into `patches` (every triangle is in exactly
    /// one patch).
    pub tri_to_patch: Vec<u32>,
    /// Sorted, deduplicated vertex ids incident to a non-manifold
    /// (patch-border) edge. The C++ marks these `vertInfo = 1` for
    /// `findRayEndpoints` (BL2): ray origins must avoid border vertices.
    pub border_verts: Vec<u32>,
}

/// Loud failure surface — never silent (P9/P10).
#[derive(Debug, PartialEq, Eq)]
pub enum PatchError {
    /// `labels.len()` does not match the triangle count.
    InputMismatch { tris: usize, labels: usize },
    /// A flood reached a triangle whose surface label differs from the
    /// seed's. The C++ asserts this invariant (`labels.surface[t] ==
    /// ref_l`); a violation means the upstream arrangement is wrong.
    LabelMismatch { seed: u32, tri: u32 },
}

/// Port of `computeAllPatches` (booleans.cpp:396, serial variant):
/// partition the soup's triangles into patches by flood-fill across
/// manifold edges, stopping at non-manifold (intersection) edges.
///
/// The C++ seeds patches by an ascending scan over triangle ids
/// (`for t_id ... if triInfo(t_id) != 1`) and floods each with the
/// stack-based `computeSinglePatch`; this port keeps both, so patch
/// order and membership are deterministic.
pub fn compute_all_patches(soup: &ArrangementSoup) -> Result<Patches, PatchError> {
    if soup.labels.len() != soup.tris.len() {
        return Err(PatchError::InputMismatch {
            tris: soup.tris.len(),
            labels: soup.labels.len(),
        });
    }

    // Edge → incident triangles, key = sorted vertex pair. This is the
    // FastTrimesh `adjE2T` equivalent (module-doc deviation: built directly
    // from the soup; manifold ::= ≤ 2 incident tris, cinolib semantics).
    let mut e2t: BTreeMap<(u32, u32), Vec<u32>> = BTreeMap::new();
    for (t, tri) in soup.tris.iter().enumerate() {
        for k in 0..3 {
            let (u, v) = (tri[k], tri[(k + 1) % 3]);
            e2t.entry((u.min(v), u.max(v))).or_default().push(t as u32);
        }
    }

    let n = soup.tris.len();
    let mut visited = vec![false; n]; // C++ triInfo == 1
    let mut tri_to_patch = vec![0u32; n];
    let mut patches: Vec<Vec<u32>> = Vec::new();
    let mut border: BTreeSet<u32> = BTreeSet::new(); // C++ vertInfo == 1

    for seed in 0..n as u32 {
        if visited[seed as usize] {
            continue;
        }
        // ----- computeSinglePatch (booleans.cpp:426, serial) -----
        let pid = patches.len() as u32;
        let ref_l = canonical(&soup.labels[seed as usize]);
        let mut patch: Vec<u32> = Vec::new();
        let mut stack = vec![seed];
        while let Some(curr) = stack.pop() {
            // The C++ stack can hold duplicate pushes (visited is set at
            // pop; its hash-set `patch.insert` dedups). The Vec port skips
            // already-visited pops instead.
            if visited[curr as usize] {
                continue;
            }
            visited[curr as usize] = true;
            patch.push(curr);
            tri_to_patch[curr as usize] = pid;
            // Patch-label check (spec `cherchi_patch_label_tolerance` §3).
            // The C++ `assert(labels.surface[t_id] == ref_l)` (booleans.cpp:449)
            // is a NDEBUG no-op in the release reference: it floods across the
            // manifold border between a merged `[A,B]` coplanar-overlap sheet
            // and the single-input `[A]` region it extends, and the patch takes
            // the seed's label (booleans.cpp:629). Match that where the labels
            // are COMPATIBLE — one canonical set ⊆ the other (L2a) — and stay
            // LOUD for genuinely DISJOINT labels (L2b: a real arrangement
            // corruption, deliberately stricter than the release reference; see
            // the deviation note in docs/yang_deviations.md).
            let cl = canonical(&soup.labels[curr as usize]);
            if cl != ref_l {
                let compatible =
                    cl.iter().all(|x| ref_l.contains(x)) || ref_l.iter().all(|x| cl.contains(x));
                if !compatible {
                    if std::env::var_os("CHERCHI_PATCH_PROBE").is_some() {
                        eprintln!(
                            "CHERCHI_PATCH_PROBE label-mismatch seed={seed} tri={curr} \
                             ref_l={ref_l:?} cl={cl:?} patch_len={}",
                            patch.len()
                        );
                    }
                    return Err(PatchError::LabelMismatch { seed, tri: curr });
                }
                // L2a: compatible — continue flooding; the patch keeps ref_l
                // (the seed's label), matching booleans.cpp:629.
            }
            let tri = soup.tris[curr as usize];
            for k in 0..3 {
                let (u, v) = (tri[k], tri[(k + 1) % 3]);
                let inc = &e2t[&(u.min(v), u.max(v))];
                if inc.len() <= 2 {
                    // Manifold edge → keep flooding.
                    for &t2 in inc {
                        if t2 != curr && !visited[t2 as usize] {
                            stack.push(t2);
                        }
                    }
                } else {
                    // Non-manifold (intersection) edge → stop flooding;
                    // mark patch-border vertices for BL2 ray endpoints.
                    border.insert(u);
                    border.insert(v);
                }
            }
        }
        patch.sort_unstable();
        patches.push(patch);
    }

    Ok(Patches {
        patches,
        tri_to_patch,
        border_verts: border.into_iter().collect(),
    })
}

/// Canonical (sorted) copy of a label for set-equality comparison.
/// The C++ compares `std::bitset` surface labels (order-free); the Rust
/// `Label` is a `Vec<InputId>`, so comparison canonicalizes first.
fn canonical(label: &Label) -> Label {
    let mut l = label.clone();
    l.sort_unstable();
    l
}

// =========================================================================
// RED oracle tests (PR-CR-BL1)
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arrangements::fast_trimesh::VertexCoords;
    use crate::arrangements::soup::mesh_arrangement;
    use crate::labeled_arrangement::InputId;
    use cad_primitives::Point3;
    use std::collections::BTreeMap;

    const A: InputId = InputId(0);
    const B: InputId = InputId(1);

    // ----- fixtures (mirror arrangements::soup tests) ---------------------

    fn tetra(
        ox: f64,
        oy: f64,
        oz: f64,
        s: f64,
        label: InputId,
    ) -> (Vec<f64>, Vec<[u32; 3]>, Vec<Label>) {
        let corners = [
            (ox, oy, oz),
            (ox + s, oy, oz),
            (ox, oy + s, oz),
            (ox, oy, oz + s),
        ];
        let mut coords = Vec::with_capacity(12);
        for (x, y, z) in corners {
            coords.push(x);
            coords.push(y);
            coords.push(z);
        }
        let tris = vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]];
        let labels = vec![vec![label]; tris.len()];
        (coords, tris, labels)
    }

    /// Axis-aligned box [o, o+s]^3 as 12 triangles, outward winding.
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
            [0, 3, 2], // bottom (z=0)
            [4, 5, 6],
            [4, 6, 7], // top (z=1)
            [0, 1, 5],
            [0, 5, 4], // front (y=0)
            [2, 3, 7],
            [2, 7, 6], // back (y=1)
            [1, 2, 6],
            [1, 6, 5], // right (x=1)
            [3, 0, 4],
            [3, 4, 7], // left (x=0)
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

    /// Two axis-aligned cubes overlapping at a corner — genuinely
    /// interpenetrating: the arrangement realizes the intersection loop as
    /// edges with 4 incident triangles (2 per input), verified by probe
    /// (edge-incidence histogram {2: 60, 4: 6} on this fixture).
    fn cut_boxes_soup() -> ArrangementSoup {
        let (coords, tris, labels) =
            concat(cube(0.0, 0.0, 0.0, 2.0, A), cube(1.0, 1.0, 1.0, 2.0, B));
        mesh_arrangement(&coords, &tris, &labels).expect("box overlap arrangement")
    }

    /// Two tetrahedra touching at exactly ONE POINT: B's apex (1,1,1) lies
    /// on A's slant plane x+y+z=3. The arrangement welds the touch point
    /// (B's apex splits A's slant face, 8 real verts / 10 tris) but creates
    /// NO intersection segments — every edge stays manifold.
    fn touching_tetra_soup() -> ArrangementSoup {
        let (coords, tris, labels) =
            concat(tetra(0.0, 0.0, 0.0, 3.0, A), tetra(1.0, 1.0, 1.0, 3.0, B));
        mesh_arrangement(&coords, &tris, &labels).expect("tetra touch arrangement")
    }

    fn disjoint_tetra_soup() -> ArrangementSoup {
        let (coords, tris, labels) = concat(
            tetra(0.0, 0.0, 0.0, 1.0, A),
            tetra(10.0, 10.0, 10.0, 1.0, B),
        );
        mesh_arrangement(&coords, &tris, &labels).expect("disjoint arrangement")
    }

    /// Cube B strictly inside cube A — fully enclosed: no surface
    /// intersection, two separate shells.
    fn enclosed_cube_soup() -> ArrangementSoup {
        let (coords, tris, labels) =
            concat(cube(0.0, 0.0, 0.0, 2.0, A), cube(0.5, 0.5, 0.5, 1.0, B));
        mesh_arrangement(&coords, &tris, &labels).expect("enclosed arrangement")
    }

    /// Edge → incident-triangle count over a soup (independent oracle copy:
    /// intentionally NOT the implementation's adjacency).
    fn edge_tri_counts(soup: &ArrangementSoup) -> BTreeMap<(u32, u32), Vec<u32>> {
        let mut m: BTreeMap<(u32, u32), Vec<u32>> = BTreeMap::new();
        for (t, tri) in soup.tris.iter().enumerate() {
            for k in 0..3 {
                let (u, v) = (tri[k], tri[(k + 1) % 3]);
                let key = (u.min(v), u.max(v));
                m.entry(key).or_default().push(t as u32);
            }
        }
        m
    }

    // ════════════════════════════════════════════════════════════════
    // Oracle #1 — partition: every triangle is in exactly one patch.
    // ════════════════════════════════════════════════════════════════
    #[test]
    fn partition_every_tri_in_exactly_one_patch() {
        let soup = cut_boxes_soup();
        let p = compute_all_patches(&soup).expect("patches");

        assert_eq!(
            p.tri_to_patch.len(),
            soup.tris.len(),
            "tri_to_patch must cover every triangle"
        );
        let mut seen = vec![0u32; soup.tris.len()];
        for (pi, patch) in p.patches.iter().enumerate() {
            assert!(!patch.is_empty(), "patch {pi} is empty");
            for &t in patch {
                seen[t as usize] += 1;
                assert_eq!(p.tri_to_patch[t as usize], pi as u32, "tri_to_patch[{t}]");
            }
        }
        assert!(
            seen.iter().all(|&c| c == 1),
            "every triangle in exactly one patch (counts: min {:?} max {:?})",
            seen.iter().min(),
            seen.iter().max()
        );
    }

    // ════════════════════════════════════════════════════════════════
    // Oracle #2 — label-COMPATIBLE (spec `cherchi_patch_label_tolerance` §3):
    // a patch's canonical labels form a COMPATIBILITY CHAIN (pairwise
    // subset-comparable), NOT strict equality. Relaxed from the pre-L2a
    // set-equality form: L2a admits a merged `[A,B]` coplanar-overlap sheet
    // into the single-input `[A]` region's patch, so `[A] ⊆ [A,B]` is a valid
    // in-patch pair. Disjoint labels never share a patch (L2b keeps them a loud
    // `LabelMismatch`), so the chain invariant holds. (`cut_boxes_soup` is
    // non-coplanar / label-homogeneous → the chain is trivially all-equal here;
    // the relaxed assertion encodes the CONTRACT the coplanar path now relies
    // on, per constitution "tests are permanent — fix it if it's wrong".)
    // ════════════════════════════════════════════════════════════════
    #[test]
    fn patches_are_label_compatible() {
        let soup = cut_boxes_soup();
        let p = compute_all_patches(&soup).expect("patches");
        assert!(!p.patches.is_empty(), "stub returned no patches");

        // `a ⊆ b` on canonical (sorted, deduped) label sets.
        let subset = |a: &Label, b: &Label| a.iter().all(|x| b.contains(x));

        for (pi, patch) in p.patches.iter().enumerate() {
            let labels: Vec<Label> = patch
                .iter()
                .map(|&t| canonical(&soup.labels[t as usize]))
                .collect();
            for (i, li) in labels.iter().enumerate() {
                for lj in &labels[i + 1..] {
                    assert!(
                        subset(li, lj) || subset(lj, li),
                        "patch {pi}: labels {li:?} and {lj:?} are not subset-comparable \
                         (compatibility chain violated)"
                    );
                }
            }
        }
    }

    // ════════════════════════════════════════════════════════════════
    // Oracle #3 — flood closure: two triangles sharing a MANIFOLD edge
    // (≤ 2 incident tris) are in the same patch (maximality), and a
    // non-manifold edge always separates patch interiors from flooding
    // through it (the flood must never cross it).
    // ════════════════════════════════════════════════════════════════
    #[test]
    fn manifold_adjacency_stays_within_patch() {
        let soup = cut_boxes_soup();
        let p = compute_all_patches(&soup).expect("patches");
        assert_eq!(p.tri_to_patch.len(), soup.tris.len());

        for (edge, tris) in edge_tri_counts(&soup) {
            if tris.len() == 2 {
                assert_eq!(
                    p.tri_to_patch[tris[0] as usize], p.tri_to_patch[tris[1] as usize],
                    "manifold edge {edge:?} must not separate patches (maximality)"
                );
            }
        }
    }

    // ════════════════════════════════════════════════════════════════
    // Oracle #4 — interpenetrating solids are CUT: the intersection
    // loop splits each solid's surface into ≥ 2 patches; with two
    // labels that is ≥ 4 patches total, ≥ 2 per input label.
    // ════════════════════════════════════════════════════════════════
    #[test]
    fn intersection_cuts_each_solid_into_multiple_patches() {
        let soup = cut_boxes_soup();
        let p = compute_all_patches(&soup).expect("patches");

        let mut per_label: BTreeMap<Label, usize> = BTreeMap::new();
        for patch in &p.patches {
            let l = canonical(&soup.labels[patch[0] as usize]);
            *per_label.entry(l).or_default() += 1;
        }
        assert!(
            p.patches.len() >= 4,
            "two corner-overlapping cubes must yield >= 4 patches, got {}",
            p.patches.len()
        );
        for (l, n) in &per_label {
            assert!(
                *n >= 2,
                "label {l:?} surface must be cut into >= 2 patches, got {n}"
            );
        }
    }

    // ════════════════════════════════════════════════════════════════
    // Oracle #5 — border verts are EXACTLY the endpoints of non-manifold
    // edges (sorted, deduped). C++ sets vertInfo=1 on those endpoints.
    // ════════════════════════════════════════════════════════════════
    #[test]
    fn border_verts_are_nonmanifold_edge_endpoints() {
        let soup = cut_boxes_soup();
        let p = compute_all_patches(&soup).expect("patches");

        let mut expect: Vec<u32> = edge_tri_counts(&soup)
            .iter()
            .filter(|(_, tris)| tris.len() > 2)
            .flat_map(|((u, v), _)| [*u, *v])
            .collect();
        expect.sort_unstable();
        expect.dedup();
        assert!(
            !expect.is_empty(),
            "fixture sanity: interpenetration must create non-manifold edges"
        );
        assert_eq!(
            p.border_verts, expect,
            "border_verts must be exactly the non-manifold edge endpoints"
        );
    }

    // ════════════════════════════════════════════════════════════════
    // Oracle #6 — disjoint solids: one patch per closed shell, no
    // border verts.
    // ════════════════════════════════════════════════════════════════
    #[test]
    fn disjoint_solids_are_one_patch_each() {
        let soup = disjoint_tetra_soup();
        let p = compute_all_patches(&soup).expect("patches");

        assert_eq!(p.patches.len(), 2, "two disjoint shells → two patches");
        assert!(
            p.border_verts.is_empty(),
            "no intersections → no border verts"
        );
        let l0 = canonical(&soup.labels[p.patches[0][0] as usize]);
        let l1 = canonical(&soup.labels[p.patches[1][0] as usize]);
        assert_ne!(l0, l1, "the two patches are the two distinct inputs");
    }

    // ════════════════════════════════════════════════════════════════
    // Oracle #7 — fully ENCLOSED solid (no surface intersection): the
    // shells never touch, so each is one patch even though B is inside A.
    // ════════════════════════════════════════════════════════════════
    #[test]
    fn enclosed_solid_keeps_separate_shell_patches() {
        let soup = enclosed_cube_soup();
        let p = compute_all_patches(&soup).expect("patches");

        assert_eq!(p.patches.len(), 2, "enclosed shell stays its own patch");
        assert!(p.border_verts.is_empty());
    }

    // ════════════════════════════════════════════════════════════════
    // Oracle #8 — determinism: two runs produce identical output, and
    // patch seed order matches ascending triangle-id discovery (the
    // first patch contains triangle 0; each patch's seed — its minimum
    // tri id — is increasing across patches).
    // ════════════════════════════════════════════════════════════════
    #[test]
    fn deterministic_and_seed_ordered() {
        let soup = cut_boxes_soup();
        let p1 = compute_all_patches(&soup).expect("patches");
        let p2 = compute_all_patches(&soup).expect("patches");
        assert_eq!(p1, p2, "same input → identical patches");

        let seeds: Vec<u32> = p1
            .patches
            .iter()
            .map(|patch| *patch.iter().min().expect("non-empty"))
            .collect();
        assert_eq!(seeds.first(), Some(&0), "first patch is seeded at tri 0");
        assert!(
            seeds.windows(2).all(|w| w[0] < w[1]),
            "patch order must follow ascending seed scan, got {seeds:?}"
        );
    }

    // ════════════════════════════════════════════════════════════════
    // Oracle #9 — loud input-mismatch error (P9/P10).
    // ════════════════════════════════════════════════════════════════
    #[test]
    fn label_count_mismatch_is_loud() {
        let soup = cut_boxes_soup();
        let broken = ArrangementSoup {
            labels: soup.labels[..soup.labels.len() - 1].to_vec(),
            ..soup
        };
        match compute_all_patches(&broken) {
            Err(PatchError::InputMismatch { .. }) => {}
            other => panic!("expected InputMismatch, got {other:?}"),
        }
    }

    // ════════════════════════════════════════════════════════════════
    // Oracle #10 — point-touch degeneracy: solids sharing exactly one
    // WELDED vertex (no intersection segments) are NOT cut: one patch
    // per solid, no border verts (all edges stay manifold).
    // ════════════════════════════════════════════════════════════════
    #[test]
    fn point_touching_solids_stay_one_patch_each() {
        let soup = touching_tetra_soup();
        // Fixture sanity: the touch point welds (8 real verts, 10 tris,
        // no non-manifold edges).
        assert!(
            edge_tri_counts(&soup).values().all(|t| t.len() <= 2),
            "fixture sanity: point-touch must not create non-manifold edges"
        );
        let p = compute_all_patches(&soup).expect("patches");
        assert_eq!(p.patches.len(), 2, "point-touch must not cut either solid");
        assert!(p.border_verts.is_empty());
    }

    // ════════════════════════════════════════════════════════════════
    // cherchi_patch_label_tolerance — mixed-label patch tolerance at
    // coplanar-sheet borders (spec `specs/cherchi_patch_label_tolerance.md`,
    // FIP Phase 2, RED).
    //
    // Post-canon wall of R0046/R0088/F0063 (task #14). The E2E RED for this
    // cycle is ALREADY committed: the test-harness trackers
    // `m8_rim_clustering_campaign::{red_r0046,red_r0088,red_f0063}` — do NOT
    // duplicate them here.
    //
    // Reference parity (spec §2) showed the C++ RELEASE `mesh_booleans`
    // SUCCEEDS on R0046's exact post-Stage-0 meshes: its patch flood crosses
    // the manifold border between a merged `[A,B]` coplanar overlap sheet and
    // the single-label `[A]` region (the B-side copies were dedup'd into the
    // sheet, so that border is a 2-incident MANIFOLD edge). The C++ debug
    // `assert(surface[t] == ref_l)` is a NO-OP under NDEBUG; the patch simply
    // takes the SEED's label (booleans.cpp:629). Our port hardened that assert
    // into `PatchError::LabelMismatch` (patches.rs:118) — the wall to retire.
    //
    // §3 L2 target: flooding CONTINUES across the label boundary; the mixed
    // patch carries the seed triangle's label; the production path never
    // returns `LabelMismatch`. `Patches` exposes no patch-label field, so L2 is
    // asserted through what IS observable — a full partition and the sheet
    // triangle's membership in the SEED's patch (whose minimum triangle id, the
    // seed, carries the single `[A]` label).
    //
    // GREEN-PHASE reconciliation (L2a/L2b split, spec amendment 2026-07-03;
    // Manager rulings): the pre-amendment note predicted blind L2 tolerance
    // would force updating `adversary_label_mismatch_across_manifold_edge_is_loud_error`
    // (adversary_tests.rs:315) and Oracle #2. Under the narrower split:
    // - the adversary fixture is DISJOINT `[A]`/`[B]` (L2b — stays a loud
    //   `LabelMismatch`), so it is left UNCHANGED (ruling 1); `guard_l2b_*`
    //   below adds multi-label disjoint coverage.
    // - Oracle #2 is RELAXED from set-equality to the L2a compatibility-CHAIN
    //   invariant and renamed `patches_are_label_compatible` (ruling 2) so it
    //   encodes the contract the coplanar path now relies on, even though its
    //   non-coplanar `cut_boxes_soup` fixture (L1) makes the chain all-equal.
    // Only COMPATIBLE (subset/superset) mixing at a coplanar-sheet border is
    // tolerated (L2a); disjoint labels stay loud (L2b).
    // ════════════════════════════════════════════════════════════════

    /// Hand-built soup (pure topology — patch flood never reads coordinates):
    /// dummy explicit verts, caller's tris/labels. Mirrors the
    /// `adversary_tests` helper of the same name.
    fn hand_soup(n_verts: u32, tris: Vec<[u32; 3]>, labels: Vec<Label>) -> ArrangementSoup {
        let verts = (0..n_verts)
            .map(|i| VertexCoords::Explicit(Point3::new(f64::from(i), 0.0, 0.0)))
            .collect();
        ArrangementSoup {
            verts,
            tris,
            labels,
            source: Vec::new(),
            jolly_count: 0,
            in_tris: Vec::new(),
            in_labels: Vec::new(),
            multiplier: 1.0,
        }
    }

    /// Partition check WITHOUT label-constancy (L2 permits a mixed-label patch —
    /// relaxing Oracle #2 at coplanar borders is exactly this cycle's change).
    /// Every triangle in exactly one patch; `tri_to_patch` agrees with membership.
    fn assert_partition(soup: &ArrangementSoup, p: &Patches) {
        assert_eq!(p.tri_to_patch.len(), soup.tris.len(), "tri_to_patch length");
        let mut seen = vec![0u32; soup.tris.len()];
        for (pi, patch) in p.patches.iter().enumerate() {
            assert!(!patch.is_empty(), "patch {pi} is empty");
            for &t in patch {
                seen[t as usize] += 1;
                assert_eq!(p.tri_to_patch[t as usize], pi as u32, "tri_to_patch[{t}]");
            }
        }
        assert!(
            seen.iter().all(|&c| c == 1),
            "every triangle in exactly one patch (counts: min {:?} max {:?})",
            seen.iter().min(),
            seen.iter().max()
        );
    }

    /// L2 (RED): a merged `[A,B]` coplanar-sheet triangle (T2) is manifold-
    /// adjacent (2-incident edge (2,3)) to the single-label `[A]` region
    /// (T0,T1); the sheet's opposite edge (3,4) is a genuine non-manifold
    /// intersection edge (3-incident: T2,T3,T4). TODAY `compute_all_patches`
    /// floods T0→T1→T2 across manifold edges and returns
    /// `Err(LabelMismatch { seed: 0, tri: 2 })`. TARGET (§3 L2): `Ok` — the
    /// flood crosses into T2 (the seed `[A]`'s patch), partitions all 5 tris,
    /// and still collects the (3,4) non-manifold border.
    ///
    /// GREEN (spec §3 L2a): un-ignored by the implementer once
    /// `compute_all_patches` adopts the C++ release semantics.
    #[test]
    fn red_merged_sheet_manifold_adjacent_to_single_label() {
        let soup = hand_soup(
            7,
            vec![
                [0, 1, 2], // T0  [A]    single-label region
                [0, 2, 3], // T1  [A]    manifold edge (0,2) with T0
                [2, 3, 4], // T2  [A,B]  merged sheet; manifold edge (2,3) with T1
                [3, 4, 5], // T3  [B]    across non-manifold intersection edge (3,4)
                [3, 4, 6], // T4  [B]
            ],
            vec![vec![A], vec![A], vec![A, B], vec![B], vec![B]],
        );

        // Fixture sanity (independent oracle): (2,3) is the manifold sheet
        // border (2-incident); (3,4) is the only non-manifold edge (3-incident).
        let e2t = edge_tri_counts(&soup);
        assert_eq!(e2t[&(2, 3)].len(), 2, "sheet border (2,3) must be manifold");
        assert_eq!(
            e2t[&(3, 4)].len(),
            3,
            "intersection edge (3,4) non-manifold"
        );

        // §3 L2: the production path must NOT wall on LabelMismatch.
        let p = compute_all_patches(&soup)
            .expect("L2: flood must cross the coplanar-sheet manifold border, not LabelMismatch");

        assert_partition(&soup, &p);

        // Membership: the merged sheet triangle T2 lands in the SEED's patch
        // with the single-label region T0,T1 (connected only by manifold edges).
        assert_eq!(p.tri_to_patch[0], p.tri_to_patch[1], "T0,T1 in one patch");
        assert_eq!(
            p.tri_to_patch[1], p.tri_to_patch[2],
            "merged sheet T2 joins the single-label region's patch (L2 flood)"
        );

        // "Patch label = seed's": Patches has no label field, so the only
        // observable is that the mixed patch's seed (its minimum tri id) is T0,
        // whose label is the single [A].
        let sheet_patch = &p.patches[p.tri_to_patch[2] as usize];
        let seed_tri = *sheet_patch.iter().min().expect("non-empty patch");
        assert_eq!(seed_tri, 0, "seed of the mixed patch is T0");
        assert_eq!(
            canonical(&soup.labels[seed_tri as usize]),
            vec![A],
            "mixed patch carries the seed's [A] label"
        );

        // border_verts still collects the genuine non-manifold edge (3,4).
        assert_eq!(
            p.border_verts,
            vec![3, 4],
            "the intersection edge's vertices must still be marked as border"
        );

        // Full membership pin (this fixture's expectation, parity-style): the
        // merged main patch {0,1,2} plus the two [B] tris cut off by (3,4).
        assert_eq!(p.tri_to_patch, vec![0, 0, 0, 1, 2], "patch membership");
        assert_eq!(p.patches.len(), 3, "one merged patch + two cut [B] patches");
    }

    /// L1 guard (PASSES today AND after): a label-homogeneous soup's patches are
    /// byte-identical — the fix must not perturb the non-coplanar path. Pins the
    /// full output on a small `[A]`-only fixture exercising both a manifold merge
    /// (T0,T1) and a non-manifold spine ((3,4), 3-incident: T2,T3,T4).
    #[test]
    fn guard_l1_label_homogeneous_patches_pinned() {
        let soup = hand_soup(
            8,
            vec![
                [0, 1, 2], // T0
                [0, 2, 3], // T1  manifold edge (0,2) with T0
                [3, 4, 5], // T2  ┐
                [3, 4, 6], // T3  ├ spine edge (3,4), 3-incident (non-manifold)
                [3, 4, 7], // T4  ┘
            ],
            vec![vec![A]; 5],
        );
        let p = compute_all_patches(&soup).expect("homogeneous soup patches");
        assert_eq!(
            p.patches,
            vec![vec![0, 1], vec![2], vec![3], vec![4]],
            "L1: merged pair + three spine tris, unchanged"
        );
        assert_eq!(
            p.tri_to_patch,
            vec![0, 0, 1, 2, 3],
            "L1: tri_to_patch pinned"
        );
        assert_eq!(p.border_verts, vec![3, 4], "L1: spine endpoints pinned");
    }

    /// L2b guard (spec §3 L2b): a DISJOINT label reached across a manifold edge
    /// stays a loud `LabelMismatch` — deliberately stricter than the release
    /// reference (which would silently mix). Distinct from
    /// `adversary_label_mismatch_across_manifold_edge_is_loud_error` (single vs
    /// single): here the seed is the MULTI-label `[A,B]` and the neighbor is the
    /// disjoint `[C]` (neither set ⊆ the other), pinning the subset test's
    /// negative branch. Added by the GREEN implementer as new-branch coverage
    /// (P4) — a guard for behavior L2b KEEPS, so not a RED-owned oracle.
    #[test]
    fn guard_l2b_disjoint_label_across_manifold_edge_stays_loud() {
        const C: InputId = InputId(2);
        // Two tris sharing manifold edge (0,1): T0 [A,B] (seed), T1 [C].
        let soup = hand_soup(4, vec![[0, 1, 2], [1, 0, 3]], vec![vec![A, B], vec![C]]);
        match compute_all_patches(&soup) {
            Err(PatchError::LabelMismatch { seed: 0, tri: 1 }) => {}
            other => panic!("L2b: expected LabelMismatch {{ seed: 0, tri: 1 }}, got {other:?}"),
        }
    }
}
