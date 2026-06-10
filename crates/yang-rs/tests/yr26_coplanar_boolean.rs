//! PR-YR26 RED (M8 slice b) — §4.5.5 coplanar-overlap booleans through the
//! native backend, MESH-level oracles.
//!
//! PR-YR25 shipped the EXACT 2D overlay engine (`yang_rs::coplanar_overlay`);
//! this slice WIRES it into the pipeline at Stage 0/1: where the PR-YR24 gate
//! used to return `CoplanarFacesUnsupported` for a near-coplanar planar A×B
//! face pair, the pipeline now
//!
//! 1. snaps both faces onto ONE canonical shared plane (face A's plane — the
//!    §4.5.5 "trimmed common planar surface", THE place femto residuals are
//!    reconciled symbolically),
//! 2. runs the exact 2D overlay (Yang 2025 §4.5.5, Fig. 16,
//!    `refs/text/yang2025_hybrid_boolean.txt:717-731` / `752-760`), and
//! 3. tessellates the pair via the overlay so the Overlap region gets
//!    IDENTICAL meshes on both solids ("identical meshes are generated for
//!    both models in this part"), with boundary sampling shared with the
//!    adjacent faces ("The common part and the other two parts share
//!    identical sampling points on their boundaries" — Fig. 16 caption).
//!
//! Downstream the exact duplicates weld into multi-label `{A,B}` arrangement
//! triangles. The keep-rules alone cannot resolve those (Cherchi's C++ keeps
//! the overlap sheet for EVERY op — verified against the sidecar below); the
//! result-boundary rule decides instead: a coplanar-overlap triangle is on
//! the result boundary iff exactly ONE side of its plane is inside the
//! result, which reduces to the (op, normal-agreement) table:
//!
//! | config              | Union | Intersect | Subtract A−B |
//! |---------------------|-------|-----------|--------------|
//! | opposite normals    | drop  | drop      | KEEP (A's)   |
//! | equal normals       | KEEP  | KEEP      | drop         |
//!
//! (Opposite normals = solids on opposite sides, stacked boxes; equal
//! normals = both interiors on the same side, flush/pocket faces.)
//!
//! ## Sidecar cross-check
//!
//! Each fixture is also run through the C++ `mesh_booleans` binary
//! (`SidecarBoolean`, dev-dep). Where the C++ output is a closed manifold
//! (no kept membrane) our volumes must MATCH it exactly; where the C++
//! keeps the zero-volume overlap sheet (its documented coplanar behavior),
//! the C++ signed volume differs from the solid-semantics volume by EXACTLY
//! the sheet's divergence-theorem contribution — asserted analytically, so
//! the deviation is pinned, not hand-waved.

use cad_primitives::{BoolOp, Point3, Vector3};
use cherchi_sidecar_rs::SidecarBoolean;
use std::collections::BTreeMap;
use yang_rs::{boolean, BRep, BRepEdge, BRepFace, BRepVertex, Curve, Mesh, MeshBoolean, Surface};

// ════════════════════════════════════════════════════════════════════
// fixtures
// ════════════════════════════════════════════════════════════════════

/// Axis-aligned box B-Rep [lo, hi] (8 verts / 24 edges / 6 quad faces,
/// outward plane normals; the yr24 hexahedron topology).
fn box_brep(lo: [f64; 3], hi: [f64; 3]) -> BRep {
    let v = |x: f64, y: f64, z: f64| BRepVertex {
        point: Point3::new(x, y, z),
    };
    // 0..3 bottom ring, 4..7 top ring (4 above 2, 5 above 1, 6 above 0,
    // 7 above 3 — the yr24 EDGE_PAIRS hexahedron convention).
    let vertices = vec![
        v(lo[0], lo[1], lo[2]),
        v(hi[0], lo[1], lo[2]),
        v(hi[0], hi[1], lo[2]),
        v(lo[0], hi[1], lo[2]),
        v(hi[0], hi[1], hi[2]),
        v(hi[0], lo[1], hi[2]),
        v(lo[0], lo[1], hi[2]),
        v(lo[0], hi[1], hi[2]),
    ];
    const EDGE_PAIRS: [(u32, u32); 24] = [
        (0, 1),
        (1, 2),
        (2, 3),
        (3, 0), // f0 bottom
        (4, 5),
        (5, 6),
        (6, 7),
        (7, 4), // f1 top
        (2, 1),
        (1, 5),
        (5, 4),
        (4, 2), // f2 x = hi
        (3, 2),
        (2, 4),
        (4, 7),
        (7, 3), // f3 y = hi
        (0, 3),
        (3, 7),
        (7, 6),
        (6, 0), // f4 x = lo
        (1, 0),
        (0, 6),
        (6, 5),
        (5, 1), // f5 y = lo
    ];
    let edges: Vec<BRepEdge> = EDGE_PAIRS
        .iter()
        .map(|&(start, end)| BRepEdge {
            start,
            end,
            curve: Curve::LineSegment,
        })
        .collect();
    let planes: [([f64; 3], f64); 6] = [
        ([0.0, 0.0, -1.0], lo[2]),
        ([0.0, 0.0, 1.0], -hi[2]),
        ([1.0, 0.0, 0.0], -hi[0]),
        ([0.0, 1.0, 0.0], -hi[1]),
        ([-1.0, 0.0, 0.0], lo[0]),
        ([0.0, -1.0, 0.0], lo[1]),
    ];
    let faces: Vec<BRepFace> = planes
        .iter()
        .enumerate()
        .map(|(i, &(n, d))| BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(n[0], n[1], n[2]),
                d,
            },
            outer_loop: (4 * i as u32..4 * i as u32 + 4).collect(),
            inner_loops: Vec::new(),
            reversed: false,
        })
        .collect();
    BRep::new(vertices, edges, faces).expect("box BRep::new")
}

/// A = [0,2]³ (every fixture shares this base solid).
fn solid_a() -> BRep {
    box_brep([0.0, 0.0, 0.0], [2.0, 2.0, 2.0])
}

/// F0002 pattern: B stacked on A, sharing the z = 2 plane EXACTLY and
/// FULLY (A's top face ≡ B's bottom face). Opposite-normal config.
fn b_stacked() -> BRep {
    box_brep([0.0, 0.0, 2.0], [2.0, 2.0, 4.0])
}

/// The YR24 R0029 class, reduced: the same stacked fixture with a
/// 1e-13-scale plane residual injected on B's bottom (z = 2 + 1e-13) —
/// NEAR-coplanar, not bit-exact. The Stage-0 snap must reconcile it to the
/// exact fixture's results.
fn b_stacked_near() -> BRep {
    box_brep([0.0, 0.0, 2.0 + 1e-13], [2.0, 2.0, 4.0])
}

/// Flush partial overlap: B = [1,3]×[0,2]×[2,4] on top of A — the shared
/// z = 2 plane carries an A-only region ([0,1]), an Overlap region ([1,2]),
/// and a B-only region ([2,3]). Opposite-normal config; exercises the
/// shared-boundary-sampling propagation into the adjacent side faces.
fn b_partial() -> BRep {
    box_brep([1.0, 0.0, 2.0], [3.0, 2.0, 4.0])
}

/// Blind-pocket pattern: B = [0.5,1.5]²×[1,2] sits inside A with its top
/// face FLUSH with A's top face (z = 2). EQUAL-normal config (both top
/// faces point +z): subtract must OPEN the pocket (drop the overlap sheet);
/// union must keep A's full top face; intersect = B.
fn b_pocket() -> BRep {
    box_brep([0.5, 0.5, 1.0], [1.5, 1.5, 2.0])
}

// ════════════════════════════════════════════════════════════════════
// mesh metrics (independent oracle helpers)
// ════════════════════════════════════════════════════════════════════

fn signed_volume(mesh: &Mesh) -> f64 {
    mesh.tris
        .iter()
        .map(|t| {
            let a = mesh.verts[t[0] as usize];
            let b = mesh.verts[t[1] as usize];
            let c = mesh.verts[t[2] as usize];
            (a.x() * (b.y() * c.z() - b.z() * c.y()) - a.y() * (b.x() * c.z() - b.z() * c.x())
                + a.z() * (b.x() * c.y() - b.y() * c.x()))
                / 6.0
        })
        .sum()
}

/// Per undirected POSITION-keyed edge: (count, direction balance). Keyed on
/// coordinate bits so two backends' index choices can't affect the verdict.
fn edge_stats(mesh: &Mesh) -> BTreeMap<([u64; 3], [u64; 3]), (usize, i64)> {
    let key = |v: u32| {
        let p = mesh.verts[v as usize];
        [p.x().to_bits(), p.y().to_bits(), p.z().to_bits()]
    };
    let mut m: BTreeMap<([u64; 3], [u64; 3]), (usize, i64)> = BTreeMap::new();
    for t in &mesh.tris {
        for k in 0..3 {
            let (a, b) = (key(t[k]), key(t[(k + 1) % 3]));
            let (lo, hi, dir) = if a <= b { (a, b, 1) } else { (b, a, -1) };
            let e = m.entry((lo, hi)).or_insert((0, 0));
            e.0 += 1;
            e.1 += dir;
        }
    }
    m
}

fn assert_watertight(mesh: &Mesh, what: &str) {
    assert!(!mesh.tris.is_empty(), "{what}: output must be non-empty");
    for (edge, (count, balance)) in edge_stats(mesh) {
        assert_eq!(
            count, 2,
            "{what}: edge {edge:?} must have exactly 2 incident tris"
        );
        assert_eq!(balance, 0, "{what}: edge {edge:?} once per direction");
    }
}

/// Euler characteristic V − E + F over position-welded mesh elements.
fn euler_characteristic(mesh: &Mesh) -> i64 {
    use std::collections::BTreeSet;
    let mut vs: BTreeSet<[u64; 3]> = BTreeSet::new();
    for t in &mesh.tris {
        for &v in t {
            let p = mesh.verts[v as usize];
            vs.insert([p.x().to_bits(), p.y().to_bits(), p.z().to_bits()]);
        }
    }
    vs.len() as i64 - edge_stats(mesh).len() as i64 + mesh.tris.len() as i64
}

fn assert_rel_eq(got: f64, expect: f64, what: &str) {
    let tol = expect.abs().max(1.0) * 1e-9;
    assert!(
        (got - expect).abs() <= tol,
        "{what}: {got} != expected {expect} (tol {tol})"
    );
}

/// Run `boolean()` through the production native backend; panic on error.
fn run(a: &BRep, b: &BRep, op: BoolOp, what: &str) -> BRep {
    let nb = yang_rs::native_backend().expect("native backend always available");
    match boolean(a, b, op, &nb) {
        Ok(out) => out,
        Err(e) => panic!("{what}: boolean() failed: {e}"),
    }
}

/// Closed-solid result oracle: watertight, χ = 2 (sphere-like), exact volume.
fn assert_solid(out: &BRep, vol: f64, what: &str) {
    let mesh = out.as_mesh();
    assert_watertight(mesh, what);
    assert_eq!(euler_characteristic(mesh), 2, "{what}: χ must be 2");
    let v = signed_volume(mesh);
    assert!(v > 0.0, "{what}: outward orientation (positive volume)");
    assert_rel_eq(v, vol, what);
}

/// Sidecar reference run on the SAME Stage-1 input meshes (the C++ binary;
/// self-skips with a loud eprintln when unavailable). Returns the signed
/// volume of the C++ output.
fn sidecar_volume(a: &BRep, b: &BRep, op: BoolOp, what: &str) -> Option<f64> {
    let Ok(sb) = SidecarBoolean::from_env() else {
        eprintln!("[yr26] SKIP sidecar cross-check {what}: binary not found (CHERCHI2022_BIN)");
        return None;
    };
    let out = sb
        .boolean(a.as_mesh(), b.as_mesh(), op)
        .unwrap_or_else(|e| panic!("{what}: sidecar reference failed: {e}"));
    Some(signed_volume(&out))
}

// ════════════════════════════════════════════════════════════════════
// Oracle #1 — exact stacked boxes (F0002 pattern, opposite normals).
// Union = merged solid (16, watertight, NO interior membrane);
// Subtract A−B = A exactly (8); Intersect = empty (the zero-volume
// shared face is NOT a solid).
// ════════════════════════════════════════════════════════════════════
#[test]
fn stacked_union_is_merged_solid() {
    let out = run(&solid_a(), &b_stacked(), BoolOp::Union, "stacked union");
    assert_solid(&out, 16.0, "stacked union");
}

#[test]
fn stacked_subtract_is_a_exactly() {
    let out = run(
        &solid_a(),
        &b_stacked(),
        BoolOp::Subtract,
        "stacked subtract",
    );
    assert_solid(&out, 8.0, "stacked subtract");
}

#[test]
fn stacked_intersect_is_empty() {
    let out = run(
        &solid_a(),
        &b_stacked(),
        BoolOp::Intersect,
        "stacked intersect",
    );
    assert_eq!(
        out.num_tris(),
        0,
        "stacked intersect: A∩B has zero volume — the result is EMPTY \
         (mesh-level solid semantics; the C++ keeps the degenerate sheet, \
          see the sidecar deviation oracle below)"
    );
}

// ════════════════════════════════════════════════════════════════════
// Oracle #2 — NEAR-coplanar stacked boxes (the YR24 R0029 residual
// class): identical results to the exact fixture — the Stage-0 snap
// onto the canonical shared plane reconciles the 1e-13 residual.
// ════════════════════════════════════════════════════════════════════
#[test]
fn near_coplanar_stacked_matches_exact_results() {
    let a = solid_a();
    let b = b_stacked_near();
    let out = run(&a, &b, BoolOp::Union, "near stacked union");
    assert_solid(&out, 16.0, "near stacked union");

    let out = run(&a, &b, BoolOp::Subtract, "near stacked subtract");
    assert_solid(&out, 8.0, "near stacked subtract");

    let out = run(&a, &b, BoolOp::Intersect, "near stacked intersect");
    assert_eq!(out.num_tris(), 0, "near stacked intersect: empty");
}

// ════════════════════════════════════════════════════════════════════
// Oracle #3 — flush partial overlap (A-only + Overlap + B-only regions
// on the z=2 plane; opposite normals). Exercises §4.5.5 shared boundary
// sampling: the overlay's region boundaries subdivide the adjacent side
// faces, so the output must still be watertight.
// ════════════════════════════════════════════════════════════════════
#[test]
fn partial_overlap_union_is_watertight_16() {
    let out = run(&solid_a(), &b_partial(), BoolOp::Union, "partial union");
    assert_solid(&out, 16.0, "partial union");
}

#[test]
fn partial_overlap_subtract_is_a_exactly() {
    let out = run(
        &solid_a(),
        &b_partial(),
        BoolOp::Subtract,
        "partial subtract",
    );
    assert_solid(&out, 8.0, "partial subtract");
}

#[test]
fn partial_overlap_intersect_is_empty() {
    let out = run(
        &solid_a(),
        &b_partial(),
        BoolOp::Intersect,
        "partial intersect",
    );
    assert_eq!(out.num_tris(), 0, "partial intersect: empty");
}

// ════════════════════════════════════════════════════════════════════
// Oracle #4 — blind pocket (B inside A, top faces flush; EQUAL normals).
// Subtract opens the pocket (7, watertight — the overlap sheet is the
// pocket OPENING and must be dropped); Union = A (8, the overlap is
// part of the union's boundary and must be KEPT); Intersect = B (1).
// ════════════════════════════════════════════════════════════════════
#[test]
fn pocket_subtract_opens_the_pocket() {
    let out = run(&solid_a(), &b_pocket(), BoolOp::Subtract, "pocket subtract");
    assert_solid(&out, 7.0, "pocket subtract");
}

#[test]
fn pocket_union_is_a() {
    let out = run(&solid_a(), &b_pocket(), BoolOp::Union, "pocket union");
    assert_solid(&out, 8.0, "pocket union");
}

#[test]
fn pocket_intersect_is_b() {
    let out = run(&solid_a(), &b_pocket(), BoolOp::Intersect, "pocket intersect");
    assert_solid(&out, 1.0, "pocket intersect");
}

// ════════════════════════════════════════════════════════════════════
// Oracle #5 — sidecar cross-check. The C++ reference keeps the
// zero-volume coplanar-overlap sheet for EVERY op (its keep rules see
// surface = {A,B}, inside = ∅: booleans.cpp:1422 union / 1397
// intersection / 1467 subtraction branch 1). So:
//   (a) where solid semantics agree with the C++ (no sheet in the
//       C++ result OR the sheet IS the correct boundary), our volume
//       must MATCH the sidecar's;
//   (b) where the C++ keeps a spurious sheet, the sidecar volume must
//       equal ours PLUS the sheet's exact divergence-theorem
//       contribution (area × plane height / 3 for z-planes) — pinning
//       the deviation analytically rather than hand-waving it.
// ════════════════════════════════════════════════════════════════════
#[test]
fn sidecar_cross_check_volumes() {
    let a = solid_a();

    // Sheet contribution of a region of area S on the plane z = h, normal
    // +z, to the divergence-theorem signed volume: S·h/3.
    let sheet = |area: f64, h: f64| area * h / 3.0;

    // (fixture, op, our exact volume, sidecar expected volume, label)
    let stacked = b_stacked();
    let partial = b_partial();
    let pocket = b_pocket();
    let cases: Vec<(&BRep, BoolOp, f64, f64, &str)> = vec![
        // stacked: overlap sheet = [0,2]² at z=2 (area 4, kept with +z).
        (
            &stacked,
            BoolOp::Union,
            16.0,
            16.0 + sheet(4.0, 2.0),
            "stacked union",
        ),
        (&stacked, BoolOp::Subtract, 8.0, 8.0, "stacked subtract"),
        (
            &stacked,
            BoolOp::Intersect,
            0.0,
            sheet(4.0, 2.0),
            "stacked intersect",
        ),
        // partial: overlap sheet = [1,2]×[0,2] at z=2 (area 2).
        (
            &partial,
            BoolOp::Union,
            16.0,
            16.0 + sheet(2.0, 2.0),
            "partial union",
        ),
        (&partial, BoolOp::Subtract, 8.0, 8.0, "partial subtract"),
        (
            &partial,
            BoolOp::Intersect,
            0.0,
            sheet(2.0, 2.0),
            "partial intersect",
        ),
        // pocket: overlap sheet = [0.5,1.5]² at z=2 (area 1). Union and
        // intersect KEEP it legitimately (it IS the result boundary there),
        // so the sidecar agrees with solid semantics; subtract is the
        // C++-keeps-the-opening deviation case.
        (&pocket, BoolOp::Union, 8.0, 8.0, "pocket union"),
        (
            &pocket,
            BoolOp::Subtract,
            7.0,
            7.0 + sheet(1.0, 2.0),
            "pocket subtract",
        ),
        (&pocket, BoolOp::Intersect, 1.0, 1.0, "pocket intersect"),
    ];

    for (b, op, ours_expect, sidecar_expect, what) in cases {
        let Some(side_vol) = sidecar_volume(&a, b, op, what) else {
            return; // binary unavailable — already logged
        };
        assert_rel_eq(
            side_vol,
            sidecar_expect,
            &format!("{what}: sidecar reference volume"),
        );
        let nb = yang_rs::native_backend().expect("native backend");
        let ours = match boolean(&a, b, op, &nb) {
            Ok(out) => signed_volume(out.as_mesh()),
            Err(e) => panic!("{what}: boolean() failed: {e}"),
        };
        assert_rel_eq(ours, ours_expect, &format!("{what}: yang volume"));
    }
}

// ════════════════════════════════════════════════════════════════════
// Oracle #6 — multi-label arrangement evidence (mesh level): feeding
// the §4.5.5 "identical meshes" configuration to the native backend
// produces `surface.len() == 2` triangles exactly on the overlap, and
// their patch is bounded by the overlay interface (patch borders exist).
// This pins the LabeledArrangement-level contract this slice relies on.
// ════════════════════════════════════════════════════════════════════
#[test]
fn arrangement_carries_multi_label_overlap_tris() {
    // Hand-build the Stage-0 output for the stacked fixture: two box
    // meshes with IDENTICAL shared-face triangulation (what the overlay
    // emits for full overlap).
    fn box_mesh(lo: [f64; 3], hi: [f64; 3]) -> Mesh {
        let p = |x: f64, y: f64, z: f64| Point3::new(x, y, z);
        let verts = vec![
            p(lo[0], lo[1], lo[2]),
            p(hi[0], lo[1], lo[2]),
            p(hi[0], hi[1], lo[2]),
            p(lo[0], hi[1], lo[2]),
            p(lo[0], lo[1], hi[2]),
            p(hi[0], lo[1], hi[2]),
            p(hi[0], hi[1], hi[2]),
            p(lo[0], hi[1], hi[2]),
        ];
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
        Mesh::new(verts, tris)
    }
    let ma = box_mesh([0.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
    let mb = box_mesh([0.0, 0.0, 2.0], [2.0, 2.0, 4.0]);

    let nb = yang_rs::native_backend().expect("native backend");
    let la = nb
        .labeled_arrangement(&ma, &mb)
        .expect("identical-overlap stacked arrangement must succeed");

    let multi: Vec<usize> = (0..la.mesh.tris.len())
        .filter(|&t| la.surface[t].len() > 1)
        .collect();
    assert_eq!(
        multi.len(),
        2,
        "exactly the 2 shared-face triangles carry surface = {{A,B}}"
    );
    for &t in &multi {
        // The overlap is the interface between the solids: on both
        // surfaces, inside neither.
        assert!(
            la.inside[t].iter().all(|&b| !b),
            "overlap tri {t} is inside neither solid"
        );
        // Geometric sanity: all three verts on z = 2.
        for &v in &la.mesh.tris[t] {
            assert_eq!(la.mesh.verts[v as usize].z(), 2.0, "overlap tri on z=2");
        }
    }
    // The overlap patch is its own patch (bounded by the §4.5.5
    // intersection curves at its rim).
    let p0 = la.patch[multi[0]];
    assert_eq!(la.patch[multi[1]], p0, "overlap tris share one patch");
    for t in 0..la.mesh.tris.len() {
        if !multi.contains(&t) {
            assert_ne!(
                la.patch[t], p0,
                "non-overlap tri {t} must not join the overlap patch"
            );
        }
    }
}
