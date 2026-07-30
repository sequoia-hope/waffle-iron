//! ADVERSARY stress tests for PR-CR-BL1 (`compute_all_patches`).
//!
//! Independently authored — these deliberately attack cases the BL1 RED
//! oracle suite (`patches.rs::tests`) does not cover: hand-built degenerate
//! adjacency (odd non-manifold edges, label-mismatch error paths, empty
//! soup), three-solid arrangements, a straight through-cut (two disjoint
//! intersection loops on one solid), and input-ordering / winding
//! invariance of the patch partition.
//!
//! All oracles here are independent re-derivations (BTreeMap-based), NOT
//! calls into the implementation's adjacency. Patch flood is pure topology:
//! no tolerances anywhere.

use std::collections::BTreeMap;

use cad_primitives::Point3;

use crate::arrangements::fast_trimesh::VertexCoords;
use crate::arrangements::soup::{mesh_arrangement, ArrangementSoup, Label};
use crate::labeled_arrangement::InputId;
use crate::labeling::patches::{compute_all_patches, PatchError, Patches};

const A: InputId = InputId(0);
const B: InputId = InputId(1);
const C: InputId = InputId(2);

// ════════════════════════════════════════════════════════════════════
// Shared builders (local copies — independent of the oracle suite's).
// ════════════════════════════════════════════════════════════════════

type Solid = (Vec<f64>, Vec<[u32; 3]>, Vec<Label>);

fn tetra(ox: f64, oy: f64, oz: f64, s: f64, label: InputId) -> Solid {
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

/// Axis-aligned box [o, o+s] (per-axis extents), 12 tris, outward winding.
fn boxx(ox: f64, oy: f64, oz: f64, sx: f64, sy: f64, sz: f64, label: InputId) -> Solid {
    let p = |x: f64, y: f64, z: f64| (ox + x * sx, oy + y * sy, oz + z * sz);
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

fn cube(ox: f64, oy: f64, oz: f64, s: f64, label: InputId) -> Solid {
    boxx(ox, oy, oz, s, s, s, label)
}

fn concat(s0: Solid, s1: Solid) -> Solid {
    let (mut coords, mut tris, mut labels) = s0;
    let off = (coords.len() / 3) as u32;
    coords.extend_from_slice(&s1.0);
    for t in s1.1 {
        tris.push([t[0] + off, t[1] + off, t[2] + off]);
    }
    labels.extend(s1.2);
    (coords, tris, labels)
}

fn arrange(solid: Solid) -> ArrangementSoup {
    let (coords, tris, labels) = solid;
    mesh_arrangement(&coords, &tris, &labels).expect("arrangement")
}

/// Baseline two-cube corner-overlap fixture (matches the oracle suite's
/// `cut_boxes_soup`) — used as the reference for the invariance attacks.
fn cut_boxes() -> Solid {
    concat(cube(0.0, 0.0, 0.0, 2.0, A), cube(1.0, 1.0, 1.0, 2.0, B))
}

/// Hand-built soup: dummy explicit verts (patch flood is pure topology —
/// `compute_all_patches` must never read coordinates), caller's tris/labels.
fn hand_soup(n_verts: u32, tris: Vec<[u32; 3]>, labels: Vec<Label>) -> ArrangementSoup {
    let verts = (0..n_verts)
        .map(|i| VertexCoords::Explicit(Point3::new(f64::from(i), 0.0, 0.0)))
        .collect();
    ArrangementSoup {
        verts,
        tris,
        labels,
        source: Vec::new(), // BL test fixture; provenance not exercised here
        intersection_edges: Default::default(),
        jolly_count: 0,
        in_tris: Vec::new(),
        in_labels: Vec::new(),
        multiplier: 1.0,
    }
}

// ════════════════════════════════════════════════════════════════════
// Independent oracles.
// ════════════════════════════════════════════════════════════════════

fn edge_tri_counts(soup: &ArrangementSoup) -> BTreeMap<(u32, u32), Vec<u32>> {
    let mut m: BTreeMap<(u32, u32), Vec<u32>> = BTreeMap::new();
    for (t, tri) in soup.tris.iter().enumerate() {
        for k in 0..3 {
            let (u, v) = (tri[k], tri[(k + 1) % 3]);
            m.entry((u.min(v), u.max(v))).or_default().push(t as u32);
        }
    }
    m
}

fn canonical(label: &Label) -> Label {
    let mut l = label.clone();
    l.sort_unstable();
    l
}

/// Full structural re-check, applied to EVERY soup/patch pair in this
/// module: exact partition, tri_to_patch consistency, label-constancy,
/// manifold-edge maximality, non-manifold separation, border-vert exactness.
fn assert_structural_invariants(soup: &ArrangementSoup, p: &Patches) {
    // Partition + tri_to_patch consistency.
    assert_eq!(p.tri_to_patch.len(), soup.tris.len(), "tri_to_patch length");
    let mut seen = vec![0u32; soup.tris.len()];
    for (pi, patch) in p.patches.iter().enumerate() {
        assert!(!patch.is_empty(), "patch {pi} is empty");
        let ref_l = canonical(&soup.labels[patch[0] as usize]);
        for &t in patch {
            seen[t as usize] += 1;
            assert_eq!(
                p.tri_to_patch[t as usize], pi as u32,
                "tri_to_patch[{t}] disagrees with patch membership {pi}"
            );
            assert_eq!(
                canonical(&soup.labels[t as usize]),
                ref_l,
                "patch {pi}: tri {t} label differs from patch label"
            );
        }
    }
    assert!(
        seen.iter().all(|&c| c == 1),
        "every triangle in exactly one patch (counts: min {:?} max {:?})",
        seen.iter().min(),
        seen.iter().max()
    );

    // Edge semantics: manifold edges never separate; non-manifold edges
    // never connect (a tri pair sharing ONLY a non-manifold edge must be in
    // different patches IF the non-manifold edge is their only connection —
    // not directly checkable locally, so check the implication that holds
    // locally: manifold ⇒ same patch).
    let e2t = edge_tri_counts(soup);
    for (edge, tris) in &e2t {
        if tris.len() == 2 {
            assert_eq!(
                p.tri_to_patch[tris[0] as usize], p.tri_to_patch[tris[1] as usize],
                "manifold edge {edge:?} must not separate patches"
            );
        }
    }

    // Border verts are EXACTLY the endpoints of non-manifold edges.
    let mut expect: Vec<u32> = e2t
        .iter()
        .filter(|(_, tris)| tris.len() > 2)
        .flat_map(|((u, v), _)| [*u, *v])
        .collect();
    expect.sort_unstable();
    expect.dedup();
    assert_eq!(
        p.border_verts, expect,
        "border_verts must be exactly the non-manifold edge endpoints"
    );
}

/// Canonical label → number of patches carrying it.
fn per_label_patch_counts(soup: &ArrangementSoup, p: &Patches) -> BTreeMap<Label, usize> {
    let mut m: BTreeMap<Label, usize> = BTreeMap::new();
    for patch in &p.patches {
        *m.entry(canonical(&soup.labels[patch[0] as usize]))
            .or_default() += 1;
    }
    m
}

// ════════════════════════════════════════════════════════════════════
// Attack 1 — empty soup (0 tris): must be Ok and fully empty, not panic.
// ════════════════════════════════════════════════════════════════════
#[test]
fn adversary_empty_soup_is_ok_and_empty() {
    let soup = hand_soup(0, vec![], vec![]);
    let p = compute_all_patches(&soup).expect("empty soup must be Ok");
    assert!(p.patches.is_empty(), "no tris → no patches");
    assert!(p.tri_to_patch.is_empty(), "no tris → empty tri_to_patch");
    assert!(p.border_verts.is_empty(), "no edges → no border verts");
}

// ════════════════════════════════════════════════════════════════════
// Attack 2 — single-solid arrangement (no second input at all): one
// patch covering everything, no border verts.
// ════════════════════════════════════════════════════════════════════
#[test]
fn adversary_single_solid_is_one_patch() {
    let soup = arrange(tetra(0.0, 0.0, 0.0, 1.0, A));
    let p = compute_all_patches(&soup).expect("patches");
    assert_structural_invariants(&soup, &p);
    assert_eq!(p.patches.len(), 1, "single closed shell → exactly 1 patch");
    assert_eq!(
        p.patches[0].len(),
        soup.tris.len(),
        "the single patch must cover every triangle"
    );
    assert!(p.border_verts.is_empty(), "no intersections → no borders");
}

// ════════════════════════════════════════════════════════════════════
// Attack 3 — ODD non-manifold valence: an edge with exactly 3 incident
// triangles (a "book" of 3 pages). Cinolib manifold ::= ≤2, so the flood
// must stop at this edge: 3 single-triangle patches, border = {0, 1}.
// The arrangement always produces 4-incident intersection edges, so the
// oracle suite never exercises odd valence.
// ════════════════════════════════════════════════════════════════════
#[test]
fn adversary_three_incident_edge_stops_flood() {
    // Pages [0,1,2], [0,1,3], [0,1,4] share spine edge (0,1).
    let soup = hand_soup(
        5,
        vec![[0, 1, 2], [0, 1, 3], [0, 1, 4]],
        vec![vec![A], vec![A], vec![A]],
    );
    let p = compute_all_patches(&soup).expect("patches");
    assert_structural_invariants(&soup, &p);
    assert_eq!(
        p.patches.len(),
        3,
        "3-incident spine edge must stop the flood: 3 single-tri patches, got {:?}",
        p.patches
    );
    assert_eq!(
        p.border_verts,
        vec![0, 1],
        "border verts must be exactly the spine endpoints"
    );
    assert_eq!(
        p.tri_to_patch,
        vec![0, 1, 2],
        "ascending seed scan: tri i seeds patch i"
    );
}

// ════════════════════════════════════════════════════════════════════
// Attack 4 — 4-incident edge with MIXED labels (two solids meeting along
// an edge, A|A|B|B). The flood must stop at the non-manifold edge BEFORE
// the label check — mixed labels across a non-manifold edge are the
// normal arrangement situation and must NOT yield LabelMismatch.
// ════════════════════════════════════════════════════════════════════
#[test]
fn adversary_mixed_labels_across_nonmanifold_edge_is_not_a_mismatch() {
    let soup = hand_soup(
        6,
        vec![[0, 1, 2], [0, 1, 3], [0, 1, 4], [0, 1, 5]],
        vec![vec![A], vec![A], vec![B], vec![B]],
    );
    let p = compute_all_patches(&soup)
        .expect("mixed labels across a NON-manifold edge must not be LabelMismatch");
    assert_structural_invariants(&soup, &p);
    assert_eq!(
        p.patches.len(),
        4,
        "flood must not cross the 4-incident edge"
    );
    assert_eq!(p.border_verts, vec![0, 1]);
}

// ════════════════════════════════════════════════════════════════════
// Attack 5 — label mismatch across a MANIFOLD edge: loud LabelMismatch
// error (the C++ assert), never a panic, never a silently mixed patch.
// ════════════════════════════════════════════════════════════════════
#[test]
fn adversary_label_mismatch_across_manifold_edge_is_loud_error() {
    // Two tris sharing manifold edge (0,1), different labels.
    let soup = hand_soup(4, vec![[0, 1, 2], [1, 0, 3]], vec![vec![A], vec![B]]);
    match compute_all_patches(&soup) {
        Err(PatchError::LabelMismatch { seed: 0, tri: 1 }) => {}
        other => panic!("expected LabelMismatch {{ seed: 0, tri: 1 }}, got {other:?}"),
    }
}

// ════════════════════════════════════════════════════════════════════
// Attack 6 — label comparison must be SET equality (C++ bitset), not Vec
// order: [A,B] and [B,A] on manifold-adjacent tris are the SAME label.
// ════════════════════════════════════════════════════════════════════
#[test]
fn adversary_label_vec_order_is_not_a_mismatch() {
    let soup = hand_soup(4, vec![[0, 1, 2], [1, 0, 3]], vec![vec![A, B], vec![B, A]]);
    let p = compute_all_patches(&soup)
        .expect("[A,B] vs [B,A] is the same label set — must not be LabelMismatch");
    assert_structural_invariants(&soup, &p);
    assert_eq!(p.patches.len(), 1, "same label set → one patch");
}

// ════════════════════════════════════════════════════════════════════
// Attack 7 — THREE solids, two disjoint pairwise overlaps (A∩B and B∩C
// at opposite corners of B; A∩C empty). B's surface is cut by TWO
// separate intersection loops → exactly 3 patches; A and C by one loop
// each → exactly 2 patches each.
// ════════════════════════════════════════════════════════════════════
#[test]
fn adversary_three_solid_chain_floods_each_loop_independently() {
    let solid = concat(
        concat(cube(0.0, 0.0, 0.0, 2.0, A), cube(1.0, 1.0, 1.0, 2.0, B)),
        cube(2.5, 2.5, 2.5, 2.0, C),
    );
    let soup = arrange(solid);
    let p = compute_all_patches(&soup).expect("patches");
    assert_structural_invariants(&soup, &p);

    let counts = per_label_patch_counts(&soup, &p);
    assert_eq!(
        counts.get(&vec![A]),
        Some(&2),
        "A is cut by one corner loop → exactly 2 patches, got {counts:?}"
    );
    assert_eq!(
        counts.get(&vec![B]),
        Some(&3),
        "B is cut by TWO disjoint corner loops → main surface + 2 caps = 3 \
         patches, got {counts:?}"
    );
    assert_eq!(
        counts.get(&vec![C]),
        Some(&2),
        "C is cut by one corner loop → exactly 2 patches, got {counts:?}"
    );
    assert_eq!(p.patches.len(), 7, "2 + 3 + 2 patches total");
}

// ════════════════════════════════════════════════════════════════════
// Attack 8 — straight THROUGH-CUT: box B pierces long box A completely
// (entering A's y=0 face, exiting its y=1 face). Two disjoint loops:
//   A → exterior surface + a disc on the y=0 face + a disc on the y=1
//       face = exactly 3 patches;
//   B → entry-side cap + tunnel band inside A + exit-side cap = 3.
// ════════════════════════════════════════════════════════════════════
#[test]
fn adversary_through_cut_splits_pierced_solid_into_three_patches() {
    // A: [0,6]×[0,1]×[0,1].  B: [2.5,3.5]×[-1,2]×[0.25,0.75] — passes
    // clean through A along y; no coplanar face pairs anywhere.
    let solid = concat(
        boxx(0.0, 0.0, 0.0, 6.0, 1.0, 1.0, A),
        boxx(2.5, -1.0, 0.25, 1.0, 3.0, 0.5, B),
    );
    let soup = arrange(solid);
    let p = compute_all_patches(&soup).expect("patches");
    assert_structural_invariants(&soup, &p);

    let counts = per_label_patch_counts(&soup, &p);
    assert_eq!(
        counts.get(&vec![A]),
        Some(&3),
        "through-cut A: exterior + 2 face discs = 3 patches, got {counts:?}"
    );
    assert_eq!(
        counts.get(&vec![B]),
        Some(&3),
        "through-cut B: 2 outside caps + tunnel band = 3 patches, got {counts:?}"
    );
    assert!(
        !p.border_verts.is_empty(),
        "two intersection loops must produce border verts"
    );
}

// ════════════════════════════════════════════════════════════════════
// Attacks 9–11 — input-ordering / winding invariance. Patch ids, tri
// ids, and even the triangulation may legitimately change, but the
// TOPOLOGY may not: per-label patch counts and the border-vert COUNT
// (the set of geometric intersection-curve vertices) must be invariant.
// ════════════════════════════════════════════════════════════════════

fn topology_fingerprint(solid: Solid) -> (BTreeMap<Label, usize>, usize) {
    let soup = arrange(solid);
    let p = compute_all_patches(&soup).expect("patches");
    assert_structural_invariants(&soup, &p);
    (per_label_patch_counts(&soup, &p), p.border_verts.len())
}

#[test]
fn adversary_triangle_permutation_invariance() {
    let baseline = topology_fingerprint(cut_boxes());

    // Reverse the global triangle order (labels permuted alongside).
    let (coords, tris, labels) = cut_boxes();
    let rev_tris: Vec<[u32; 3]> = tris.into_iter().rev().collect();
    let rev_labels: Vec<Label> = labels.into_iter().rev().collect();
    let permuted = topology_fingerprint((coords, rev_tris, rev_labels));

    assert_eq!(
        baseline, permuted,
        "reversing input triangle order changed the patch topology \
         (per-label patch counts, border-vert count)"
    );
}

#[test]
fn adversary_solid_concat_order_invariance() {
    let ab = topology_fingerprint(cut_boxes());
    let ba = topology_fingerprint(concat(
        cube(1.0, 1.0, 1.0, 2.0, B),
        cube(0.0, 0.0, 0.0, 2.0, A),
    ));
    assert_eq!(
        ab, ba,
        "swapping the solid concat order changed the patch topology"
    );
}

#[test]
fn adversary_winding_reversal_invariance() {
    let baseline = topology_fingerprint(cut_boxes());

    // Reverse the winding of solid B's triangles only (B is inside-out;
    // patch flood is orientation-blind topology).
    let a = cube(0.0, 0.0, 0.0, 2.0, A);
    let (bc, bt, bl) = cube(1.0, 1.0, 1.0, 2.0, B);
    let bt_rev: Vec<[u32; 3]> = bt.into_iter().map(|[u, v, w]| [u, w, v]).collect();
    let flipped = topology_fingerprint(concat(a, (bc, bt_rev, bl)));

    assert_eq!(
        baseline, flipped,
        "reversing one solid's winding changed the patch topology"
    );
}

// ════════════════════════════════════════════════════════════════════
// Attack 12 — non-manifold edges that BOTH cut and share vertices: two
// 4-incident spines meeting at a shared vertex (bow-tie). Border verts
// must be the union (deduped), and the patches around each spine must
// stay separate.
// ════════════════════════════════════════════════════════════════════
#[test]
fn adversary_two_spines_sharing_a_vertex_dedup_border() {
    // Spine 1: edge (0,1) with tris [0,1,2],[0,1,3],[0,1,4],[0,1,5].
    // Spine 2: edge (1,6) with tris [1,6,7],[1,6,8],[1,6,9].
    // Vertex 1 is on both spines → must appear once in border_verts.
    let soup = hand_soup(
        10,
        vec![
            [0, 1, 2],
            [0, 1, 3],
            [0, 1, 4],
            [0, 1, 5],
            [1, 6, 7],
            [1, 6, 8],
            [1, 6, 9],
        ],
        vec![vec![A]; 7],
    );
    let p = compute_all_patches(&soup).expect("patches");
    assert_structural_invariants(&soup, &p);
    assert_eq!(p.patches.len(), 7, "every page is its own patch");
    assert_eq!(
        p.border_verts,
        vec![0, 1, 6],
        "border verts = union of spine endpoints, sorted + deduped"
    );
}
