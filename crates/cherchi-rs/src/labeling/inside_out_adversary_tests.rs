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
//!   explicit non-border vertex (Cycle-B `NoExplicitRayOrigin` territory:
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
// `NoExplicitRayOrigin` (the C++ generated-ray fallback is Cycle B) —
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
        Err(InsideOutError::NoExplicitRayOrigin { patch }) => {
            eprintln!(
                "face-pierce characterization: loud NoExplicitRayOrigin {{ patch: {patch} }} \
                 (Cycle-B scope)"
            );
        }
        Err(other) => {
            panic!("face-pierce mix: expected Ok(correct) or NoExplicitRayOrigin, got {other:?}")
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
// `NoExplicitRayOrigin` error is acceptable (Cycle-B scope) — but a
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
        Err(InsideOutError::NoExplicitRayOrigin { patch }) => {
            eprintln!(
                "through-cut characterization: loud NoExplicitRayOrigin {{ patch: {patch} }} \
                 (Cycle-B scope)"
            );
        }
        Err(other) => {
            panic!("through-cut: expected Ok(correct) or NoExplicitRayOrigin, got {other:?}")
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

// ═════════════════════════════════════════════════════════════════════
// ═════════════════════════════════════════════════════════════════════
//
//   PR-CR-BL2 Cycle B ADVERSARY — the GENERATED-RAY branch
//   (`find_ray_endpoints`, inside_out.rs generated-ray section, commit
//   83d9f165; C++ booleans.cpp:525-575 + the `ray.tv` discard branches
//   of sortIntersectedTrisAlong*).
//
//   Every fixture below contains at least one ORIGINLESS patch — a
//   through-cut band or a hole disc bounded entirely by intersection
//   loops, whose every vertex is implicit (LPI) or on the patch border
//   — so `compute_inside_out` must take the synthetic-origin path:
//   approximate centroid, −0.1 offset along the dominant-normal axis,
//   exact straddle + strict-interior validation, `seed_tri` recorded,
//   and the sort discarding hits on the opposite side of the seed plane
//   from v1.
//
//   Truth stays implementation-independent: strict AABB containment of
//   patch-triangle centroids for all-box fixtures (exact — boxes are
//   their own AABBs), with LPI vertices resolved by the adversary's own
//   f64 line-plane math (`approx_coords` above), plus a hand-rolled
//   diamond-prism membership test for the rotated-peg fixture.
//
// ═════════════════════════════════════════════════════════════════════
// ═════════════════════════════════════════════════════════════════════

/// Shared per-patch box-truth assertion: every patch's inner label equals
/// the strict-containment truth; returns the number of inside patches.
fn assert_all_match_box_truth(soup: &ArrangementSoup, patches: &Patches, inner: &[Label]) -> usize {
    let mut inside_seen = 0;
    for (pi, patch) in patches.patches.iter().enumerate() {
        let expect = expected_inner_boxes(soup, patch);
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
    inside_seen
}

/// Count patches whose surface label is exactly `{l}`.
fn count_label(soup: &ArrangementSoup, patches: &Patches, l: InputId) -> usize {
    patches
        .patches
        .iter()
        .filter(|p| canonical(&soup.labels[p[0] as usize]) == vec![l])
        .count()
}

// ════════════════════════════════════════════════════════════════════
// Attack B1 — dominant-axis coverage, X: the peg pierces cube A along
// the X axis, so A's two hole discs sit on its x=0 / x=2 faces (seed
// plane normal X, origin offset along −x, +X validation ray) and the
// band's side walls have Y- and Z-dominant normals. The BL2 oracle
// only ever pierces along Z; a projection / perturbation-axis mix-up
// in the Axis::X arm would be invisible to it.
// Truth: band inside A; A's 2 discs inside B; everything else outside.
// ════════════════════════════════════════════════════════════════════
#[test]
fn adversary_b_generated_ray_through_cut_along_x() {
    let (soup, patches, inner) = run(concat(
        cube(0.0, 0.0, 0.0, 2.0, A),
        boxx(-1.0, 0.5, 0.5, 4.0, 1.0, 1.0, B),
    ));
    let inside = assert_all_match_box_truth(&soup, &patches, &inner);
    assert_eq!(inside, 3, "band + two discs inside");
    assert_eq!(count_label(&soup, &patches, A), 3, "A: shell + 2 discs");
    assert_eq!(
        count_label(&soup, &patches, B),
        3,
        "B: below / band / above"
    );
}

// ════════════════════════════════════════════════════════════════════
// Attack B2 — dominant-axis coverage, Y: same through-cut rotated to
// pierce along Y (discs on y=0 / y=2, Axis::Y seed planes).
// ════════════════════════════════════════════════════════════════════
#[test]
fn adversary_b_generated_ray_through_cut_along_y() {
    let (soup, patches, inner) = run(concat(
        cube(0.0, 0.0, 0.0, 2.0, A),
        boxx(0.5, -1.0, 0.5, 1.0, 4.0, 1.0, B),
    ));
    let inside = assert_all_match_box_truth(&soup, &patches, &inner);
    assert_eq!(inside, 3, "band + two discs inside");
    assert_eq!(count_label(&soup, &patches, A), 3, "A: shell + 2 discs");
    assert_eq!(
        count_label(&soup, &patches, B),
        3,
        "B: below / band / above"
    );
}

// ════════════════════════════════════════════════════════════════════
// Attack B3 — TWO separate pegs through one cube: two independent
// originless bands and four originless discs in the same soup, three
// input labels. Each band's generated ray crosses only ITS hole's
// geometry; each disc must come out inside exactly its own peg. A
// patch-indexed mix-up between generated rays (e.g. a cached seed_tri
// leaking across patches) would cross-contaminate the labels.
// Truth: B band {A}, C band {A}, 2 discs {B}, 2 discs {C}, rest {}.
// ════════════════════════════════════════════════════════════════════
#[test]
fn adversary_b_two_pegs_one_cube() {
    let (soup, patches, inner) = run(concat(
        concat(
            cube(0.0, 0.0, 0.0, 2.0, A),
            boxx(0.2, 0.2, -1.0, 0.6, 0.6, 4.0, B),
        ),
        boxx(1.2, 1.2, -1.0, 0.6, 0.6, 4.0, C),
    ));
    let inside = assert_all_match_box_truth(&soup, &patches, &inner);
    assert_eq!(inside, 6, "2 bands + 4 discs inside");
    assert_eq!(count_label(&soup, &patches, A), 5, "A: shell + 4 discs");
    assert_eq!(count_label(&soup, &patches, B), 3);
    assert_eq!(count_label(&soup, &patches, C), 3);
}

// ════════════════════════════════════════════════════════════════════
// Attack B4 — one peg through TWO stacked (separated) cubes: the peg
// splits into five segments (below A / band-in-A / between / band-in-C
// / above), two of which are originless bands inside DIFFERENT
// containers, and each cube grows two originless discs. The generated
// rays of A's discs travel up through the gap and through cube C —
// forward hits on a third solid must be kept and classified by
// orientation (front-face first → outside C).
// Truth: band-in-A {A}, band-in-C {C}, A discs {B}×2, C discs {B}×2.
// ════════════════════════════════════════════════════════════════════
#[test]
fn adversary_b_peg_through_two_stacked_cubes() {
    let (soup, patches, inner) = run(concat(
        concat(cube(0.0, 0.0, 0.0, 2.0, A), cube(0.0, 0.0, 3.0, 2.0, C)),
        boxx(0.5, 0.5, -1.0, 1.0, 1.0, 7.0, B),
    ));
    let inside = assert_all_match_box_truth(&soup, &patches, &inner);
    assert_eq!(inside, 6, "2 bands + 4 discs inside");
    assert_eq!(count_label(&soup, &patches, A), 3, "A: shell + 2 discs");
    assert_eq!(count_label(&soup, &patches, C), 3, "C: shell + 2 discs");
    assert_eq!(
        count_label(&soup, &patches, B),
        5,
        "B: below / band-in-A / between / band-in-C / above"
    );
}

// ════════════════════════════════════════════════════════════════════
// Attack B5 — the seed-plane discard, armed: slab C spans the FULL xy
// footprint under cube A at z ∈ [-2, -0.05], so the generated origin
// of A's bottom disc (seed plane z=0, origin at centroid z −0.1) lands
// strictly INSIDE C, and its +Z ray crosses C's top face at z=-0.05 —
// a BACK-face hit that, if kept, would silently label the disc inside
// C. Only the seed-plane discard (hits on the opposite side of z=0
// from v1) rejects it; the explicit-ray origin filter (lessThan vs v0)
// would KEEP it, since z=-0.05 is past v0's z=-0.1. The peg also
// pierces C's top face, adding a gap band between C and A that is
// inside NEITHER solid.
// Truth: A discs {B} (never C!), B below-segment {C}, gap band {},
// band-in-A {A}, C's disc {B}, shells {}.
// ════════════════════════════════════════════════════════════════════
#[test]
fn adversary_b_behind_seed_plane_solid_must_be_discarded() {
    let (soup, patches, inner) = run(concat(
        concat(
            cube(0.0, 0.0, 0.0, 2.0, A),
            boxx(-1.0, -1.0, -2.0, 4.0, 4.0, 1.95, C),
        ),
        boxx(0.5, 0.5, -1.0, 1.0, 1.0, 4.0, B),
    ));
    let inside = assert_all_match_box_truth(&soup, &patches, &inner);
    // A's discs ({B}×2) + band-in-A ({A}) + B's below-segment ({C}) +
    // C's hole disc ({B}) = 5 inside patches.
    assert_eq!(inside, 5, "discs + band + below-segment + C disc");
    // The sharpest assertion: no A patch may be labeled inside C — the
    // only C crossing an A-disc ray ever sees is behind its seed plane.
    for (pi, patch) in patches.patches.iter().enumerate() {
        if canonical(&soup.labels[patch[0] as usize]) == vec![A] {
            assert!(
                !inner[pi].contains(&C),
                "patch {pi} (own A): behind-seed-plane hit on C leaked into \
                 the inner label, got {:?}",
                inner[pi]
            );
        }
    }
    assert_eq!(count_label(&soup, &patches, A), 3, "A: shell + 2 discs");
    assert_eq!(
        count_label(&soup, &patches, B),
        4,
        "B: in-C (incl bottom cap) / gap band / band-in-A / above (incl top cap)"
    );
    assert_eq!(count_label(&soup, &patches, C), 2, "C: shell + 1 disc");
}

// ════════════════════════════════════════════════════════════════════
// Attack B6 — forward third-solid hits must be KEPT: slab D spans the
// full xy footprint ABOVE cube A at z ∈ [2.5, 3.5], so the generated
// +Z rays of BOTH A discs cross D's bottom face in FRONT of their seed
// planes — a front-face hit that must be kept and classified outside
// (an over-eager discard, or a wrongly-oriented seed test, would also
// be caught by the peg's in-D segment expecting {C}... naming: D is
// labeled C here). The peg's top cap (z=3) sits INSIDE the slab.
// Truth: A discs {B}, band-in-A {A}, gap band {}, B top segment {C},
// C's hole disc {B}, shells {}.
// ════════════════════════════════════════════════════════════════════
#[test]
fn adversary_b_forward_solid_hits_are_kept_and_outside() {
    let (soup, patches, inner) = run(concat(
        concat(
            cube(0.0, 0.0, 0.0, 2.0, A),
            boxx(-1.0, -1.0, 2.5, 4.0, 4.0, 1.0, C),
        ),
        boxx(0.5, 0.5, -1.0, 1.0, 1.0, 4.0, B),
    ));
    let inside = assert_all_match_box_truth(&soup, &patches, &inner);
    // A discs ({B}×2) + band-in-A ({A}) + B's in-slab segment ({C}) +
    // C's hole disc ({B}) = 5.
    assert_eq!(inside, 5);
    // No A patch is inside the slab (front-face first ⇒ outside).
    for (pi, patch) in patches.patches.iter().enumerate() {
        if canonical(&soup.labels[patch[0] as usize]) == vec![A] {
            assert!(
                !inner[pi].contains(&C),
                "patch {pi} (own A): forward front-face hit on the slab must \
                 classify OUTSIDE, got {:?}",
                inner[pi]
            );
        }
    }
    assert_eq!(count_label(&soup, &patches, A), 3);
    assert_eq!(count_label(&soup, &patches, C), 2, "slab: shell + 1 disc");
}

// ════════════════════════════════════════════════════════════════════
// Attack B7 — oblique peg: the square peg is rotated 45° about its Z
// axis (a diamond prism), so the intersection loops on A's faces run
// diagonally, every loop vertex is an LPI point with non-trivial
// coordinates (1 ± 0.7 off the binary grid), the band's side-wall
// normals are (±1,±1,0)/√2 (dominant-axis TIE between X and Y), and
// the disc seed triangles are diagonal slivers. Truth for "inside the
// peg" is a hand-rolled strict diamond-prism membership test —
// expected_inner_boxes would be UNSOUND here (the peg's AABB strictly
// contains the diamond, so shell centroids near the hole could sit in
// the AABB while outside the solid).
// ════════════════════════════════════════════════════════════════════
fn diamond_prism(cx: f64, cy: f64, d: f64, zlo: f64, zhi: f64, label: InputId) -> Solid {
    // verts 0..3 at zlo (CCW from +z), 4..7 at zhi.
    let ring = [(cx + d, cy), (cx, cy + d), (cx - d, cy), (cx, cy - d)];
    let mut coords = Vec::with_capacity(24);
    for &z in &[zlo, zhi] {
        for &(x, y) in &ring {
            coords.push(x);
            coords.push(y);
            coords.push(z);
        }
    }
    let mut tris = vec![
        [0, 2, 1],
        [0, 3, 2], // bottom cap (normal -z)
        [4, 5, 6],
        [4, 6, 7], // top cap (normal +z)
    ];
    for i in 0..4u32 {
        let j = (i + 1) % 4;
        tris.push([i, j, 4 + j]);
        tris.push([i, 4 + j, 4 + i]);
    }
    let labels = vec![vec![label]; tris.len()];
    (coords, tris, labels)
}

/// Strict membership of every patch-triangle centroid in the OPEN diamond
/// prism |x-cx|+|y-cy| < d, zlo < z < zhi (relative margin as elsewhere).
fn patch_inside_diamond(
    soup: &ArrangementSoup,
    patch: &[u32],
    cx: f64,
    cy: f64,
    d: f64,
    zlo: f64,
    zhi: f64,
) -> bool {
    let eps = d * 1e-9;
    patch.iter().all(|&t| {
        let tri = soup.tris[t as usize];
        let mut c = [0.0; 3];
        for &v in &tri {
            let p = approx_coords(&soup.verts[v as usize]);
            for k in 0..3 {
                c[k] += p[k] / 3.0;
            }
        }
        (c[0] - cx).abs() + (c[1] - cy).abs() < d - eps && c[2] > zlo + eps && c[2] < zhi - eps
    })
}

#[test]
fn adversary_b_rotated_diamond_peg_through_cube() {
    let (soup, patches, inner) = run(concat(
        cube(0.0, 0.0, 0.0, 2.0, A),
        diamond_prism(1.0, 1.0, 0.7, -1.0, 3.0, B),
    ));
    // The soup lives in the arrangement's SCALED coordinate space (the
    // Cherchi compute_multiplier scale-up), so the diamond parameters are
    // re-derived from B's prepped input AABB: the diamond's AABB is
    // [cx−d, cx+d] × [cy−d, cy+d] × [zlo, zhi] exactly.
    let (lo, hi) = input_aabb(&soup, B);
    let (cx, cy) = ((lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0);
    let d = (hi[0] - lo[0]) / 2.0;
    let (zlo, zhi) = (lo[2], hi[2]);
    let mut a_inside = 0;
    let mut b_inside = 0;
    for (pi, patch) in patches.patches.iter().enumerate() {
        let own = canonical(&soup.labels[patch[0] as usize]);
        let expect: Label = if own == vec![A] {
            if patch_inside_diamond(&soup, patch, cx, cy, d, zlo, zhi) {
                a_inside += 1;
                vec![B]
            } else {
                vec![]
            }
        } else if patch_inside_box(&soup, patch, A) {
            b_inside += 1;
            vec![A]
        } else {
            vec![]
        };
        assert_eq!(
            canonical(&inner[pi]),
            expect,
            "patch {pi} (own {own:?}): rotated-peg inner label vs diamond/box truth"
        );
    }
    assert_eq!(
        a_inside, 2,
        "A's two diagonal hole discs are inside the peg"
    );
    assert_eq!(b_inside, 1, "exactly the peg's band is inside A");
    assert_eq!(count_label(&soup, &patches, A), 3, "A: shell + 2 discs");
    assert_eq!(
        count_label(&soup, &patches, B),
        3,
        "B: below / band / above"
    );
}

// ════════════════════════════════════════════════════════════════════
// Attack B8 — sliver peg: a 0.01 × 0.01 cross-section peg (at the
// non-representable offset 0.495) through the unit cube. The disc
// patches' triangles are tiny and thin, so the f64 approximate
// centroid is maximally stressed relative to the triangle size — the
// EXACT straddle + strict-interior validation must still accept (or
// reject and move to the next triangle), never mislabel. The band's
// −0.1 origin offset is 10× the peg width, placing the side-wall rays'
// origins well OUTSIDE the peg (but inside A).
// ════════════════════════════════════════════════════════════════════
#[test]
fn adversary_b_sliver_peg_through_unit_cube() {
    let (soup, patches, inner) = run(concat(
        cube(0.0, 0.0, 0.0, 1.0, A),
        boxx(0.495, 0.495, -1.0, 0.01, 0.01, 3.0, B),
    ));
    let inside = assert_all_match_box_truth(&soup, &patches, &inner);
    assert_eq!(inside, 3, "band + two tiny discs inside");
    assert_eq!(count_label(&soup, &patches, A), 3, "A: shell + 2 discs");
    assert_eq!(
        count_label(&soup, &patches, B),
        3,
        "B: below / band / above"
    );
}

// ════════════════════════════════════════════════════════════════════
// Attack B9 — determinism + triangle-permutation invariance for the
// generated-ray fixture: reversing the global triangle order (and the
// solid concat order) re-shuffles which patch triangle seeds the
// synthetic ray and re-orders the candidate scan, but the multiset of
// (own label, inner label) pairs is a topological invariant. (The
// bit-identical two-run determinism check runs inside `run` for every
// fixture above; this attack covers the *input-presentation* axis.)
//
// FAILING — BOTH sub-attacks (UPSTREAM BUG, confirmed by probe — NOT
// the generated-ray branch): under the REVERSED triangle order (and
// equally under the swapped concat order, which fails with the same
// 2-patch merged signature), `mesh_arrangement` (arrangements/soup.rs)
// emits a soup with the same vert/tri counts
// (37/88) but a DIFFERENT edge structure: 120 edges with only 12
// non-manifold, vs the forward order's 116 edges with 16 non-manifold.
// The two intersection loops need 16 shared (multiplicity-4) edges to
// fence the patches; the reversed soup is missing 4 loop segments
// (scaled coords: z=0 loop (2,2,0)-(2,5,0) and (6,3,0)-(6,6,0); z=8
// loop (2,2,8)-(5,2,8) and (3,6,8)-(6,6,8)). Probe evidence: the two
// z=8 segments exist only as multiplicity-2 edges incident to PEG
// (label B) wall triangles — cube A's top-face re-triangulation does
// not contain its constraint segment as an edge — and the two z=0
// segments exist as an edge on NEITHER side. With those fence gaps
// manifold (≤2 incident tris), the BL1 flood-fill (patches.rs:124-136,
// faithful: it stops only at >2-incident edges) leaks through, and
// shell + discs merge into ONE patch per solid (2 patches instead of
// 6). `compute_inside_out` then labels the merged patches correctly
// *for its input* (a merged patch has an explicit non-border origin
// and is mostly outside) — the wrong final labels are inherited, not
// produced, by BL2. The conforming-soup invariant the AR3b suite
// checks (no interior AREA overlaps, soup.rs:1196) is blind to this:
// a constraint segment crossing a perpendicular face's triangulation
// overlaps no triangle interior. Fix belongs in the AR3b global-soup
// orchestration: every intersection-curve segment must end up a
// shared (multiplicity-4) edge of both incident surfaces, independent
// of input triangle order.
// ════════════════════════════════════════════════════════════════════
#[test]
#[ignore = "RED witness for PR-CR-AR3c: AR3b constraint realization is input-order-DEPENDENT \
            on closed intersection loops (4 fence segments unrealized under reversed tri order / \
            swapped concat order -> BL1 flood leaks). Upstream arrangement bug, not BL2. \
            Un-ignore when AR3c lands. Run: cargo test -p cherchi-rs --features indirect-predicates \
            adversary_b_generated_ray_permutation -- --ignored"]
fn adversary_b_generated_ray_permutation_invariance() {
    let through_cut = || {
        concat(
            cube(0.0, 0.0, 0.0, 2.0, A),
            boxx(0.5, 0.5, -1.0, 1.0, 1.0, 4.0, B),
        )
    };

    let (s0, p0, i0) = run(through_cut());
    let baseline = label_fingerprint(&s0, &p0, &i0);
    assert_eq!(
        baseline.iter().filter(|(_, il)| !il.is_empty()).count(),
        3,
        "fixture sanity: band + two discs inside"
    );

    // (a) swap the solid concat order (the peg becomes input 0).
    // FAILING — same upstream fence-gap merge (2 patches, all outside).
    let (s2, p2, i2) = run(concat(
        boxx(0.5, 0.5, -1.0, 1.0, 1.0, 4.0, B),
        cube(0.0, 0.0, 0.0, 2.0, A),
    ));
    assert_eq!(
        label_fingerprint(&s2, &p2, &i2),
        baseline,
        "swapping solid concat order changed the (own, inner) multiset"
    );

    // (b) reverse the global triangle order (labels permuted alongside).
    // FAILING — see the header comment (upstream fence gaps).
    let (coords, tris, labels) = through_cut();
    let rev: Solid = (
        coords,
        tris.into_iter().rev().collect(),
        labels.into_iter().rev().collect(),
    );
    let (s1, p1, i1) = run(rev);
    assert_eq!(
        label_fingerprint(&s1, &p1, &i1),
        baseline,
        "reversing input triangle order changed the (own, inner) multiset \
         (upstream: the reversed soup is missing 4 of 16 intersection-loop \
         fence edges, so BL1 patches leak and merge)"
    );
}
