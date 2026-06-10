//! ADVERSARY stress tests for PR-CR-BL2 Cycle A (`compute_inside_out`).
//!
//! Independently authored — these deliberately attack cases the BL2 RED
//! oracle suite (`inside_out.rs::tests`) does not cover:
//!
//! - rays that thread the other solid's VERTICES exactly (octahedron
//!   fixtures, so the ray is NOT in any face plane — a +X ray through an
//!   axis-aligned cube corner always lies in two of its face planes);
//! - rays exactly IN the other solid's face planes (collinear cubes —
//!   the C++ `DISCARD` + grazing-corner path);
//! - three-deep nesting (C ⊂ B ⊂ A) and mixed containment + overlap with
//!   three labels;
//! - input-ordering invariance of the (own label, inner label) multiset;
//! - point-touching solids (ray origin exactly ON the other surface —
//!   the hit-at-parameter-zero corner of the sort's origin filter);
//! - the BL1 through-cut fixture, whose cap-disc patches may have no
//!   explicit non-border vertex (Cycle-B `FullyImplicitPatch` territory:
//!   a loud error is in-scope, a silently WRONG label is a bug);
//! - non-representable decimal offsets (0.3 / 0.7 / 0.9) so LPI sort keys
//!   exercise the exact comparators off the float grid.
//!
//! Truth sources here are independent of the implementation: input solids
//! are convex and axis-bounded, so strict AABB containment of patch
//! centroids (with a relative margin, resolved via float line-plane
//! intersection for LPI vertices) decides inside/outside on the box
//! fixtures, and hand-derived expectations cover the octahedron fixtures.
//! No tolerances are applied to the implementation's outputs.

use std::collections::BTreeSet;

use cad_primitives::Point3;

use crate::arrangements::fast_trimesh::VertexCoords;
use crate::arrangements::soup::{mesh_arrangement, ArrangementSoup, Label};
use crate::labeled_arrangement::InputId;
use crate::labeling::inside_out::{compute_inside_out, InsideOutError};
use crate::labeling::patches::{compute_all_patches, Patches};

const A: InputId = InputId(0);
const B: InputId = InputId(1);
const C: InputId = InputId(2);

// ════════════════════════════════════════════════════════════════════
// Builders (local copies — independent of the oracle suite's).
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

/// Axis-aligned octahedron: 6 verts at center ± r per axis, 8 slanted
/// faces, outward winding. No face is axis-perpendicular, so a +X ray
/// through one of its VERTICES is not contained in any face plane — the
/// clean "ray threads a vertex" stressor.
fn octahedron(cx: f64, cy: f64, cz: f64, r: f64, label: InputId) -> Solid {
    // vert ids: 0 = +x, 1 = -x, 2 = +y, 3 = -y, 4 = +z, 5 = -z
    let coords = vec![
        cx + r,
        cy,
        cz,
        cx - r,
        cy,
        cz,
        cx,
        cy + r,
        cz,
        cx,
        cy - r,
        cz,
        cx,
        cy,
        cz + r,
        cx,
        cy,
        cz - r,
    ];
    let xv = |s: i8| if s > 0 { 0u32 } else { 1u32 };
    let yv = |s: i8| if s > 0 { 2u32 } else { 3u32 };
    let zv = |s: i8| if s > 0 { 4u32 } else { 5u32 };
    let mut tris = Vec::with_capacity(8);
    for sx in [1i8, -1] {
        for sy in [1i8, -1] {
            for sz in [1i8, -1] {
                // (X, Y, Z) is outward-wound when sx*sy*sz > 0, else swap.
                if sx * sy * sz > 0 {
                    tris.push([xv(sx), yv(sy), zv(sz)]);
                } else {
                    tris.push([xv(sx), zv(sz), yv(sy)]);
                }
            }
        }
    }
    let labels = vec![vec![label]; tris.len()];
    (coords, tris, labels)
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
    crate::arrangements::require_ffi_shim();
    let (coords, tris, labels) = solid;
    mesh_arrangement(&coords, &tris, &labels).expect("arrangement")
}

fn canonical(l: &Label) -> Label {
    let mut l = l.clone();
    l.sort_unstable();
    l
}

// ════════════════════════════════════════════════════════════════════
// Independent truth (oracle side — float bounds on convex box inputs).
// ════════════════════════════════════════════════════════════════════

/// Float resolution of a soup vertex (truth side only; the implementation
/// is never compared against a tolerance — only against strict-containment
/// truth on convex axis-aligned boxes with a relative margin).
fn approx_coords(c: &VertexCoords) -> [f64; 3] {
    match c {
        VertexCoords::Explicit(p) => [p.x(), p.y(), p.z()],
        VertexCoords::Lpi { line, plane } => {
            let [p, q] = [line[0], line[1]];
            let sub = |u: Point3, v: Point3| [u.x() - v.x(), u.y() - v.y(), u.z() - v.z()];
            let cross = |u: [f64; 3], v: [f64; 3]| {
                [
                    u[1] * v[2] - u[2] * v[1],
                    u[2] * v[0] - u[0] * v[2],
                    u[0] * v[1] - u[1] * v[0],
                ]
            };
            let dot = |u: [f64; 3], v: [f64; 3]| u[0] * v[0] + u[1] * v[1] + u[2] * v[2];
            let n = cross(sub(plane[1], plane[0]), sub(plane[2], plane[0]));
            let d = dot(n, sub(q, p));
            assert!(d != 0.0, "truth: LPI line parallel to plane");
            let t = dot(n, sub(plane[0], p)) / d;
            [
                p.x() + t * (q.x() - p.x()),
                p.y() + t * (q.y() - p.y()),
                p.z() + t * (q.z() - p.z()),
            ]
        }
        VertexCoords::Tpi { .. } => panic!("truth: fixtures must not produce TPI vertices"),
    }
}

/// AABB of one input's prepped triangles. For axis-aligned box INPUTS the
/// AABB is the solid itself, so it is an exact truth source.
fn input_aabb(soup: &ArrangementSoup, label: InputId) -> ([f64; 3], [f64; 3]) {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    let mut found = false;
    for (t, tri) in soup.in_tris.iter().enumerate() {
        if !soup.in_labels[t].contains(&label) {
            continue;
        }
        found = true;
        for &v in tri {
            let p = approx_coords(&soup.verts[v as usize]);
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
    }
    assert!(found, "truth: no input tris for {label:?}");
    (lo, hi)
}

/// Truth: every triangle centroid of `patch` strictly inside `label`'s box
/// (relative margin keeps border-loop vertices from flipping the verdict).
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

/// All input labels present in the soup's prepped input triangles.
fn all_input_labels(soup: &ArrangementSoup) -> Vec<InputId> {
    let set: BTreeSet<InputId> = soup
        .in_labels
        .iter()
        .flat_map(|l| l.iter().copied())
        .collect();
    set.into_iter().collect()
}

/// Box-fixture truth for one patch: the set of OTHER inputs whose box
/// strictly contains every centroid of the patch. Exact for fixtures whose
/// inputs are all axis-aligned boxes.
fn expected_inner_boxes(soup: &ArrangementSoup, patch: &[u32]) -> Label {
    let own: BTreeSet<InputId> = soup.labels[patch[0] as usize].iter().copied().collect();
    all_input_labels(soup)
        .into_iter()
        .filter(|id| !own.contains(id) && patch_inside_box(soup, patch, *id))
        .collect()
}

/// Structural invariants every Ok result must satisfy, on any fixture:
/// one label per patch; inner ⊆ (all input labels \ own); sorted + deduped.
fn assert_structural(soup: &ArrangementSoup, patches: &Patches, inner: &[Label]) {
    assert_eq!(
        inner.len(),
        patches.patches.len(),
        "one inner label per patch"
    );
    let all: BTreeSet<InputId> = all_input_labels(soup).into_iter().collect();
    for (pi, patch) in patches.patches.iter().enumerate() {
        let own: BTreeSet<InputId> = soup.labels[patch[0] as usize].iter().copied().collect();
        let mut sorted = inner[pi].clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(inner[pi], sorted, "patch {pi}: inner label sorted+deduped");
        for id in &inner[pi] {
            assert!(
                !own.contains(id),
                "patch {pi}: inner label contains own surface label {id:?}"
            );
            assert!(
                all.contains(id),
                "patch {pi}: inner label {id:?} is not an input label"
            );
        }
    }
}

/// Run the full pipeline and return (soup, patches, inner labels).
fn run(solid: Solid) -> (ArrangementSoup, Patches, Vec<Label>) {
    let soup = arrange(solid);
    let patches = compute_all_patches(&soup).expect("patches");
    let inner = compute_inside_out(&soup, &patches).expect("inside_out");
    assert_structural(&soup, &patches, &inner);
    // Determinism: a second run is bit-identical.
    let again = compute_inside_out(&soup, &patches).expect("inside_out (2nd run)");
    assert_eq!(inner, again, "same input → identical inner labels");
    (soup, patches, inner)
}

/// Per-patch (own label, inner label) pairs, canonicalized + sorted — the
/// triangulation-independent fingerprint used by the invariance attacks.
fn label_fingerprint(
    soup: &ArrangementSoup,
    patches: &Patches,
    inner: &[Label],
) -> Vec<(Label, Label)> {
    let mut v: Vec<(Label, Label)> = patches
        .patches
        .iter()
        .zip(inner)
        .map(|(patch, il)| (canonical(&soup.labels[patch[0] as usize]), canonical(il)))
        .collect();
    v.sort();
    v
}

// ════════════════════════════════════════════════════════════════════
// Attack 1a — ray threads the other solid's VERTEX exactly, from OUTSIDE.
// Octahedron B sits east of cube A with its ±x apexes exactly on the
// +X line through A's y=z corners; every other A corner's ray either
// grazes B's +z apex tangentially or misses. No octahedron face plane
// contains a +X line, so this is a pure vertex-hit perturbation test
// (unlike cube corners, where the ray also lies in two face planes).
// Truth: A and B are disjoint → both inner labels empty.
// ════════════════════════════════════════════════════════════════════
#[test]
fn adversary_vertex_threading_from_outside_is_outside() {
    // A = [0,1]^3. B = octahedron center (4,0,0), r=1: apexes (3,0,0) and
    // (5,0,0) lie exactly on the y=0,z=0 line through A's corners; the
    // (y,z)=(0,1) corners' ray passes exactly through apex (4,0,1).
    let (soup, patches, inner) = run(concat(
        cube(0.0, 0.0, 0.0, 1.0, A),
        octahedron(4.0, 0.0, 0.0, 1.0, B),
    ));
    assert_eq!(patches.patches.len(), 2, "disjoint solids → 2 patches");
    for (pi, il) in inner.iter().enumerate() {
        assert!(
            il.is_empty(),
            "disjoint solids: patch {pi} must have empty inner label, got {il:?} \
             (soup labels {:?})",
            soup.labels[patches.patches[pi][0] as usize]
        );
    }
}

// ════════════════════════════════════════════════════════════════════
// Attack 1b — ray threads the containing solid's VERTEX exactly, from
// INSIDE. Cube A is strictly inside octahedron B and every +X ray from
// any A corner exits B exactly through B's +x apex (corners with
// y=z=center) or exactly through an edge of B (the z'=0 / y'=0 seams).
// Truth: A's patch inner = {B}; B's patch inner = {}.
//
// FAILING (BUG, confirmed by probe): the candidate scan also raises a
// vertex event for B's BACKWARD apex (-2,1,1) — its triangles' AABBs
// touch the ray-start plane, so the AABB pre-filter keeps them. The
// perturbed ray's hit on that one-ring is real but BEHIND the origin;
// `sort_hits_along_ray` discards it, `perturb_ray_and_find_inters_tri`
// (inside_out.rs:293-296) then returns None from inside the first
// non-empty-hits offset, and the caller (inside_out.rs:449-453) turns
// that into Err(PerturbationExhausted) — on valid input whose answer
// is {B}. The C++ skips a no-winner event (booleans.cpp:698-701
// `if(winner_tri != -1)`); with all-behind hits it would index an
// empty vector (booleans.cpp:1048 after the origin-discard loop) — UB
// in the reference, but "skip the event" is the correct semantics.
// ════════════════════════════════════════════════════════════════════
#[test]
fn adversary_vertex_threading_from_inside_is_inside() {
    // B = octahedron center (1,1,1), r=3 (|x'|+|y'|+|z'| <= 3).
    // A = cube [1, 1.5]^3 — strictly inside (corner sum 1.5 < 3).
    // Rays from A corners run at y,z in {1, 1.5}: (1,1) threads B's apex
    // (4,1,1) exactly; (1.5,1) and (1,1.5) hit B's edges exactly;
    // (1.5,1.5) hits a face interior. Every choice is degenerate-or-clean
    // and every one must classify A as inside B.
    let (soup, patches, inner) = run(concat(
        cube(1.0, 1.0, 1.0, 0.5, A),
        octahedron(1.0, 1.0, 1.0, 3.0, B),
    ));
    assert_eq!(patches.patches.len(), 2, "enclosed solids → 2 patches");
    for (pi, patch) in patches.patches.iter().enumerate() {
        let own = canonical(&soup.labels[patch[0] as usize]);
        let expect: Label = if own == vec![A] { vec![B] } else { vec![] };
        assert_eq!(
            canonical(&inner[pi]),
            expect,
            "patch {pi} (own {own:?}): cube-in-octahedron containment"
        );
    }
}

// ════════════════════════════════════════════════════════════════════
// Attack 2 — ray exactly IN the other solid's face planes: collinear
// cubes with identical y/z extents. The +X ray from any A corner lies
// in two of B's face planes (DISCARD path), threads two B corners
// (vertex events whose one-rings include ray-coplanar triangles), and
// must still come out empty/empty. This is the sharpest perturbation
// stressor for axis-aligned inputs: the perturbed ray's off-axis
// coordinates are equal-ulp, so it can run exactly along a face
// diagonal or inside a coplanar face triangle.
// Truth: disjoint → both inner labels empty.
//
// FAILING (BUG, confirmed by probe): the near-corner event of B finds a
// perturbation winner (B's facing tri, "entering" → outside, correct so
// far), but the far-corner event finds NO strict interior hit at any of
// the 8 offsets — the equal-ulp (+y,+z) perturbed ray runs EXACTLY
// along the far face's y=z diagonal, and the ray-coplanar one-ring tris
// have a collinear (tv_i, tv_j, ray.v0) edge that zeroes orient3d — so
// prune_intersections_and_sort_along_ray (inside_out.rs:449-453)
// returns Err(PerturbationExhausted) on valid disjoint input. The C++
// skips a -1 winner and proceeds to the correct empty/empty answer
// (booleans.cpp:698-701).
// ════════════════════════════════════════════════════════════════════
#[test]
fn adversary_ray_in_face_planes_collinear_cubes() {
    let (soup, patches, inner) = run(concat(
        cube(0.0, 0.0, 0.0, 1.0, A),
        cube(2.0, 0.0, 0.0, 1.0, B),
    ));
    assert_eq!(patches.patches.len(), 2);
    for (pi, il) in inner.iter().enumerate() {
        assert!(
            il.is_empty(),
            "collinear disjoint cubes: patch {pi} inner must be empty, got {il:?} \
             (own {:?})",
            soup.labels[patches.patches[pi][0] as usize]
        );
    }
}

// ════════════════════════════════════════════════════════════════════
// Attack 3 — nested three deep: C ⊂ B ⊂ A, no surface intersections.
// The ray from C crosses TWO shells, both via exact face-diagonal edge
// hits (centered cubes share the y=z diagonal plane), so the exact
// sorting of two perturbation winners along one ray is load-bearing.
// Truth: inner(C) = {A,B}, inner(B) = {A}, inner(A) = {}.
// ════════════════════════════════════════════════════════════════════
#[test]
fn adversary_nested_three_deep() {
    let (soup, patches, inner) = run(concat(
        concat(cube(0.0, 0.0, 0.0, 6.0, A), cube(1.0, 1.0, 1.0, 4.0, B)),
        cube(2.0, 2.0, 2.0, 2.0, C),
    ));
    assert_eq!(patches.patches.len(), 3, "three nested shells → 3 patches");
    for (pi, patch) in patches.patches.iter().enumerate() {
        let own = canonical(&soup.labels[patch[0] as usize]);
        let expect: Label = if own == vec![C] {
            vec![A, B]
        } else if own == vec![B] {
            vec![A]
        } else {
            vec![]
        };
        assert_eq!(
            canonical(&inner[pi]),
            expect,
            "patch {pi} (own {own:?}): three-deep nesting"
        );
    }
}

// ════════════════════════════════════════════════════════════════════
// Attack 4a — mixed containment + overlap in one soup: B fully inside A,
// C interpenetrating A at a CORNER (C ∩ B = ∅; corner overlap keeps an
// original — explicit, non-border — vertex on every patch). Per-patch
// truth from strict box containment of centroids (all inputs are boxes
// → exact). Expected: B's shell = {A}; A's corner cap = {C}; C's inside
// piece = {A}; all other patches = {}.
// ════════════════════════════════════════════════════════════════════
#[test]
fn adversary_mixed_containment_and_overlap() {
    let (soup, patches, inner) = run(concat(
        concat(cube(0.0, 0.0, 0.0, 4.0, A), cube(1.0, 1.0, 1.0, 1.0, B)),
        cube(3.5, 3.5, 3.5, 1.0, C),
    ));
    // A is cut by C's corner loop (>= 2 patches), B one shell, C cut by A.
    assert!(
        patches.patches.len() >= 5,
        "A cut + B shell + C cut → at least 5 patches, got {}",
        patches.patches.len()
    );
    let mut inside_seen = 0;
    for (pi, patch) in patches.patches.iter().enumerate() {
        let expect = expected_inner_boxes(&soup, patch);
        if !expect.is_empty() {
            inside_seen += 1;
        }
        assert_eq!(
            canonical(&inner[pi]),
            expect,
            "patch {pi} (own {:?}): inner label vs box-containment truth",
            soup.labels[patch[0] as usize]
        );
    }
    // Fixture sanity: B's shell ({A}), A's cap inside C ({C}) and C's
    // piece inside A ({A}) must all exist.
    assert!(
        inside_seen >= 3,
        "fixture sanity: at least 3 inside patches, got {inside_seen}"
    );
}

// ════════════════════════════════════════════════════════════════════
// Attack 4b — same mix but with C piercing A's FACE instead of a corner:
// A's disc patch under C is bounded entirely by intersection loops (no
// original A vertex on it), so Cycle A may loudly refuse with
// `FullyImplicitPatch` (the C++ generated-ray fallback is Cycle B) —
// acceptable; a silently wrong label is a bug.
// ════════════════════════════════════════════════════════════════════
#[test]
fn adversary_mixed_overlap_face_pierce_loud_or_correct() {
    let soup = arrange(concat(
        concat(cube(0.0, 0.0, 0.0, 4.0, A), cube(1.0, 1.0, 1.0, 1.0, B)),
        cube(3.5, 0.5, 0.5, 2.0, C),
    ));
    let patches = compute_all_patches(&soup).expect("patches");
    match compute_inside_out(&soup, &patches) {
        Ok(inner) => {
            assert_structural(&soup, &patches, &inner);
            for (pi, patch) in patches.patches.iter().enumerate() {
                assert_eq!(
                    canonical(&inner[pi]),
                    expected_inner_boxes(&soup, patch),
                    "patch {pi} (own {:?}): inner label vs box truth",
                    soup.labels[patch[0] as usize]
                );
            }
        }
        // Cycle-B scope limit, loud and typed — acceptable, documented.
        // NOTE the variant name is broader than its trigger: it also fires
        // for an all-EXPLICIT patch whose every vertex is on the border.
        Err(InsideOutError::FullyImplicitPatch { patch }) => {
            eprintln!(
                "face-pierce characterization: loud FullyImplicitPatch {{ patch: {patch} }} \
                 (Cycle-B scope)"
            );
        }
        Err(other) => {
            panic!("face-pierce mix: expected Ok(correct) or FullyImplicitPatch, got {other:?}")
        }
    }
}

// ════════════════════════════════════════════════════════════════════
// Attack 5 — input-ordering invariance: reversing the global triangle
// order and swapping the solid concat order may change patch/tri ids
// and even the triangulation, but the multiset of (own label, inner
// label) pairs is a topological invariant.
// ════════════════════════════════════════════════════════════════════
#[test]
fn adversary_permutation_and_concat_order_invariance() {
    let cut_boxes = || concat(cube(0.0, 0.0, 0.0, 2.0, A), cube(1.0, 1.0, 1.0, 2.0, B));

    let (s0, p0, i0) = run(cut_boxes());
    let baseline = label_fingerprint(&s0, &p0, &i0);

    // (a) reverse the global triangle order (labels permuted alongside).
    let (coords, tris, labels) = cut_boxes();
    let rev: Solid = (
        coords,
        tris.into_iter().rev().collect(),
        labels.into_iter().rev().collect(),
    );
    let (s1, p1, i1) = run(rev);
    assert_eq!(
        label_fingerprint(&s1, &p1, &i1),
        baseline,
        "reversing input triangle order changed the (own, inner) multiset"
    );

    // (b) swap the solid concat order.
    let (s2, p2, i2) = run(concat(
        cube(1.0, 1.0, 1.0, 2.0, B),
        cube(0.0, 0.0, 0.0, 2.0, A),
    ));
    assert_eq!(
        label_fingerprint(&s2, &p2, &i2),
        baseline,
        "swapping solid concat order changed the (own, inner) multiset"
    );
}

// ════════════════════════════════════════════════════════════════════
// Attack 6 — point-touching solids: B's apex lies exactly ON A's slant
// face (x+y+z=3), welded by the arrangement with no intersection
// segments. The ray origin can be the touch point itself — a point ON
// the other solid's surface — so the first hit is at ray parameter
// EXACTLY ZERO. A grazing touch is not containment: both inner labels
// must be empty. (The C++ keeps hits with lessThanOnX(hit, v0) == 0,
// so a hit at the origin is classified by its facet orientation — for
// a tangential touch that can flip the verdict to "inside".)
//
// FAILING (BUG, confirmed by probe): B's ray origin is the welded touch
// vertex (scaled (4,4,4)), exactly ON A's slant plane. The hit at ray
// parameter ZERO survives the origin filter (`before_origin` discards
// only strictly-Negative, inside_out.rs:358-365 — faithful to the C++
// `lessThanOnX(hit, v0) < 0`, booleans.cpp:1190), and the nearest-hit
// orientation (back-face) classifies B as inside A: silently WRONG
// inner = [A] for a solid that merely touches. Reference parity note:
// the C++ explicit-ray branch has the same defect on this measure-zero
// configuration; a hit at parameter zero needs to be resolved (or the
// origin choice must avoid vertices lying on another solid's surface).
// ════════════════════════════════════════════════════════════════════
#[test]
fn adversary_point_touching_solids_are_outside() {
    let (soup, patches, inner) = run(concat(
        tetra(0.0, 0.0, 0.0, 3.0, A),
        tetra(1.0, 1.0, 1.0, 3.0, B),
    ));
    assert_eq!(patches.patches.len(), 2, "point-touch must not cut");
    for (pi, il) in inner.iter().enumerate() {
        assert!(
            il.is_empty(),
            "point-touching solids: patch {pi} (own {:?}) must be OUTSIDE the \
             other (touch is not containment), got inner {il:?}",
            soup.labels[patches.patches[pi][0] as usize]
        );
    }
}

// ════════════════════════════════════════════════════════════════════
// Attack 7 — through-cut cap discs (the BL1 adversary fixture): box B
// pierces long box A completely. A's two cap-disc patches (on its y=0
// and y=1 faces) are bounded entirely by intersection loops, so they
// may have NO explicit non-border vertex → the loud Cycle-A
// `FullyImplicitPatch` error is acceptable (Cycle-B scope) — but a
// silently WRONG label is a bug. Characterize: either the loud error,
// or Ok with every patch matching box-containment truth.
// ════════════════════════════════════════════════════════════════════
#[test]
fn adversary_through_cut_caps_loud_or_correct() {
    let soup = arrange(concat(
        boxx(0.0, 0.0, 0.0, 6.0, 1.0, 1.0, A),
        boxx(2.5, -1.0, 0.25, 1.0, 3.0, 0.5, B),
    ));
    let patches = compute_all_patches(&soup).expect("patches");
    assert_eq!(
        patches.patches.len(),
        6,
        "through-cut: A exterior + 2 discs, B 2 caps + tunnel band"
    );
    match compute_inside_out(&soup, &patches) {
        Ok(inner) => {
            assert_structural(&soup, &patches, &inner);
            for (pi, patch) in patches.patches.iter().enumerate() {
                let expect = expected_inner_boxes(&soup, patch);
                assert_eq!(
                    canonical(&inner[pi]),
                    expect,
                    "patch {pi} (own {:?}): through-cut inner label vs box truth",
                    soup.labels[patch[0] as usize]
                );
            }
            // Fixture sanity: A's discs ({B}×2) and B's tunnel ({A}).
            let inside_count = inner.iter().filter(|l| !l.is_empty()).count();
            assert_eq!(
                inside_count, 3,
                "through-cut: exactly 3 inside patches (2 discs + tunnel), \
                 got {inner:?}"
            );
            eprintln!("through-cut characterization: Ok branch (all labels correct)");
        }
        // Cycle-B scope limit, loud and typed — acceptable, documented.
        Err(InsideOutError::FullyImplicitPatch { patch }) => {
            eprintln!(
                "through-cut characterization: loud FullyImplicitPatch {{ patch: {patch} }} \
                 (Cycle-B scope)"
            );
        }
        Err(other) => {
            panic!("through-cut: expected Ok(correct) or FullyImplicitPatch, got {other:?}")
        }
    }
}

// ════════════════════════════════════════════════════════════════════
// Attack 8 — non-representable decimal offsets: cubes at 0.3 with size
// 0.9 / 0.7, so every intersection vertex and LPI sort key lives off
// the float grid. Truth from strict box containment; plus determinism
// (asserted inside `run`).
// ════════════════════════════════════════════════════════════════════
#[test]
fn adversary_non_representable_offsets_match_box_truth() {
    let (soup, patches, inner) = run(concat(
        cube(0.0, 0.0, 0.0, 1.0, A),
        cube(0.3, 0.3, 0.3, 0.9, B),
    ));
    assert!(
        patches.patches.len() >= 4,
        "corner overlap must cut both shells, got {} patches",
        patches.patches.len()
    );
    let mut inside_seen = 0;
    for (pi, patch) in patches.patches.iter().enumerate() {
        let expect = expected_inner_boxes(&soup, patch);
        if !expect.is_empty() {
            inside_seen += 1;
        }
        assert_eq!(
            canonical(&inner[pi]),
            expect,
            "patch {pi} (own {:?}): inner label vs box truth at 0.3/0.9 offsets",
            soup.labels[patch[0] as usize]
        );
    }
    assert!(
        inside_seen >= 2,
        "fixture sanity: one inside patch per solid, got {inside_seen}"
    );
}

// ════════════════════════════════════════════════════════════════════
// Attack 9 — fully empty soup + empty patch list: must be Ok(empty),
// not MissingInputTris (the guard is `in_tris empty AND tris non-empty`).
// ════════════════════════════════════════════════════════════════════
#[test]
fn adversary_empty_soup_is_ok_empty() {
    let soup = ArrangementSoup {
        verts: Vec::new(),
        tris: Vec::new(),
        labels: Vec::new(),
        jolly_count: 0,
        in_tris: Vec::new(),
        in_labels: Vec::new(),
    };
    let patches = Patches {
        patches: Vec::new(),
        tri_to_patch: Vec::new(),
        border_verts: Vec::new(),
    };
    let inner = compute_inside_out(&soup, &patches).expect("empty soup must be Ok");
    assert!(inner.is_empty(), "no patches → no inner labels");
}
