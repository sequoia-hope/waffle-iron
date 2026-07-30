//! M3 adversarial audit — Adversary role (role-separated TDD).
//!
//! Goal: try to BREAK the M3 functional boolean (`yang_rs::boolean()` producing
//! a watertight 2-manifold B-Rep from the labeled arrangement) and audit it
//! against the spec (`specs/yang_m3_functional_boolean.md`) and DoD §1.
//!
//! These tests are INDEPENDENT of the existing `m3_*` oracles: they use
//! different geometry (so a coincidental pass on the canonical cubes can't hide
//! a generalization bug), independently-computed analytic volumes, and exercise
//! branches the existing suite leaves uncovered (XOR end-to-end, coplanar →
//! error, near-coincident weld, mislabeled tri).
//!
//! Sidecar-backed tests self-skip when the C++ binary is absent
//! (`yang_rs::native_backend()` → `None`, FFI stub build).

use cad_primitives::{BoolOp, Point3, Vector3};
use cherchi_rs::labeled_arrangement::{InputId as LaInputId, LabeledArrangement};
use cherchi_rs::{Mesh, MeshBoolean};
use std::error::Error;
use yang_rs::{boolean, BRep, BRepEdge, BRepFace, BRepVertex, Curve, Surface, YangError};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

// =========================================================================
// Shared geometry helpers (independent of the production / existing-test ones)
// =========================================================================

/// Axis-aligned unit cube BRep at `origin` with correct OUTWARD normals and
/// correct plane offsets (`n·x + d = 0`). Identical math to the existing
/// `unit_cube_brep_offset_at`, re-derived here so this file is self-contained.
fn cube(origin: [f64; 3]) -> BRep {
    let [x, y, z] = origin;
    let verts = vec![
        BRepVertex { point: p(x, y, z) },
        BRepVertex {
            point: p(x + 1.0, y, z),
        },
        BRepVertex {
            point: p(x + 1.0, y + 1.0, z),
        },
        BRepVertex {
            point: p(x, y + 1.0, z),
        },
        BRepVertex {
            point: p(x, y, z + 1.0),
        },
        BRepVertex {
            point: p(x + 1.0, y, z + 1.0),
        },
        BRepVertex {
            point: p(x + 1.0, y + 1.0, z + 1.0),
        },
        BRepVertex {
            point: p(x, y + 1.0, z + 1.0),
        },
    ];
    let face_verts: [[u32; 4]; 6] = [
        [0, 1, 2, 3], // bottom (z)
        [4, 7, 6, 5], // top (z+1)
        [0, 4, 5, 1], // front (y)
        [1, 5, 6, 2], // right (x+1)
        [2, 6, 7, 3], // back (y+1)
        [3, 7, 4, 0], // left (x)
    ];
    let mut edges = Vec::with_capacity(24);
    let mut loops = Vec::with_capacity(6);
    for vs in &face_verts {
        let base = edges.len() as u32;
        for i in 0..4 {
            edges.push(BRepEdge {
                start: vs[i],
                end: vs[(i + 1) % 4],
                curve: Curve::LineSegment,
            });
        }
        loops.push(vec![base, base + 1, base + 2, base + 3]);
    }
    let normals: [Vector3; 6] = [
        Vector3::new(0.0, 0.0, -1.0),
        Vector3::new(0.0, 0.0, 1.0),
        Vector3::new(0.0, -1.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Vector3::new(-1.0, 0.0, 0.0),
    ];
    let offs = [z, -(z + 1.0), y, -(x + 1.0), -(y + 1.0), x];
    let faces: Vec<BRepFace> = (0..6)
        .map(|i| BRepFace {
            surface: Surface::Plane {
                normal: normals[i],
                d: offs[i],
            },
            outer_loop: loops[i].clone(),
            inner_loops: Vec::new(),
            reversed: false,
        })
        .collect();
    BRep::new(verts, edges, faces).expect("cube BRep::new failed")
}

fn signed_volume(mesh: &Mesh) -> f64 {
    let mut acc = 0.0;
    for tri in &mesh.tris {
        let a = mesh.verts[tri[0] as usize].as_array();
        let b = mesh.verts[tri[1] as usize].as_array();
        let c = mesh.verts[tri[2] as usize].as_array();
        let cx = b[1] * c[2] - b[2] * c[1];
        let cy = b[2] * c[0] - b[0] * c[2];
        let cz = b[0] * c[1] - b[1] * c[0];
        acc += a[0] * cx + a[1] * cy + a[2] * cz;
    }
    acc / 6.0
}

fn unpaired_half_edges(mesh: &Mesh) -> usize {
    use std::collections::HashMap;
    let mut counts: HashMap<(u32, u32), i32> = HashMap::new();
    for tri in &mesh.tris {
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            *counts.entry((tri[i], tri[j])).or_insert(0) += 1;
        }
    }
    let mut unpaired = 0;
    for (&(s, e), &fwd) in &counts {
        let rev = counts.get(&(e, s)).copied().unwrap_or(0);
        if fwd != rev {
            unpaired += (fwd - rev).unsigned_abs() as usize;
        }
    }
    unpaired
}

fn euler_characteristic(mesh: &Mesh) -> i64 {
    use std::collections::HashSet;
    let v = mesh.num_verts() as i64;
    let f = mesh.num_tris() as i64;
    let mut edges: HashSet<(u32, u32)> = HashSet::new();
    for tri in &mesh.tris {
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            let (a, b) = (tri[i], tri[j]);
            edges.insert(if a < b { (a, b) } else { (b, a) });
        }
    }
    v - edges.len() as i64 + f
}

/// Full structural + numeric audit on a sidecar result.
fn audit(op: BoolOp, a: &BRep, b: &BRep, expected_volume: f64) {
    let Some(sb) = yang_rs::native_backend() else {
        eprintln!("[m3_adversary] SKIP: native FFI shim not linked (stub build)");
        return;
    };
    let r = boolean(a, b, op, &sb).expect("yang-rs boolean failed");

    assert!(!r.faces().is_empty(), "{op:?}: output BRep has no faces");

    // Full attribution (no None / skeleton).
    let attr = r.triangle_attribution();
    assert_eq!(
        attr.len(),
        r.num_tris(),
        "{op:?}: attribution len != output tri count"
    );
    assert!(r.num_tris() > 0, "{op:?}: empty output mesh");
    for t in 0..attr.len() as u32 {
        assert!(
            attr.lookup(t).is_some(),
            "{op:?}: tri {t} unattributed (skeleton, not closed)"
        );
    }

    // I9 signed volume WITH sign.
    let vol = signed_volume(r.as_mesh());
    assert!(
        (vol - expected_volume).abs() < 1e-6,
        "{op:?}: signed volume {vol} != expected {expected_volume} (Δ={})",
        (vol - expected_volume).abs()
    );

    // I8 watertight.
    assert_eq!(
        unpaired_half_edges(r.as_mesh()),
        0,
        "{op:?}: unpaired half-edges (not watertight)"
    );

    // I10 Euler = 2 (genus 0).
    assert_eq!(
        euler_characteristic(r.as_mesh()),
        2,
        "{op:?}: Euler V-E+F != 2"
    );
}

// =========================================================================
// ATTACK 1 — Generalization: independent interpenetration geometry.
//
// A@[0,0,0] (unit), B@[0.7,0.3,0.4] (unit).
// Overlap box = [0.7,1] × [0.3,1] × [0.4,1] = 0.3 · 0.7 · 0.6 = 0.126.
//   union     = 1 + 1 − 0.126 = 1.874
//   intersect = 0.126
//   subtract  = 1 − 0.126 = 0.874
// No coincident planes: A faces {0,1}×3 ; B faces {0.7,1.7},{0.3,1.3},{0.4,1.4}.
//
// *** CONFIRMED BUG (BUG-1, generalization) ***
// These three tests FAIL. Root cause (proven by direct arrangement probe):
// for this in-scope corner-clipping interpenetration the C++ sidecar's
// arrangement mesh contains a BIT-EXACT duplicate vertex — indices 17 and 20
// are both `[0.7000000000000001, 0.3, 1.0]` — used by SHARED triangles
// (23, 24, 60, 66). yang-rs's I6 weld guard correctly detects this and
// returns `NonManifoldInput`, so `boolean()` aborts.
//
// Conclusion: invariant I6 ("welded mesh, no two distinct indices coincident")
// holds ONLY for the canonical diagonal cubes (the spec's empirical basis:
// "22 unique verts"). It does NOT generalize to arbitrary in-scope
// interpenetrations. M3 therefore fails on geometry squarely inside its
// stated scope. The fix belongs in the Implementer's domain: either weld the
// arrangement mesh before adjacency (index merge of bit-coincident verts) or
// have the sidecar parser dedup. Owner: Implementer (yang-rs boolean / weld).
// =========================================================================

const OFF_B: [f64; 3] = [0.7, 0.3, 0.4];
const OVERLAP: f64 = 0.3 * 0.7 * 0.6; // 0.126

#[test]
fn a1_union_independent_geometry() {
    // FAILS — BUG-1: arrangement has bit-exact duplicate verts → weld guard
    // → NonManifoldInput. See ATTACK 1 header.
    audit(
        BoolOp::Union,
        &cube([0.0, 0.0, 0.0]),
        &cube(OFF_B),
        1.0 + 1.0 - OVERLAP,
    );
}

#[test]
fn a1_intersect_independent_geometry() {
    // FAILS — BUG-1 (same root cause).
    audit(
        BoolOp::Intersect,
        &cube([0.0, 0.0, 0.0]),
        &cube(OFF_B),
        OVERLAP,
    );
}

// =========================================================================
// ATTACK 2 — flip_for_op correctness for SUBTRACT and XOR.
//
// a2_subtract_independent_geometry FAILS for the SAME reason as ATTACK 1
// (BUG-1: duplicate arrangement verts → weld guard), NOT a flip/sign bug —
// the call never reaches volume evaluation. The subtract SIGN itself is
// validated on the diagonal cubes by the existing `m3_subtract_*` oracle.
//
// *** CONFIRMED BUG (BUG-2, XOR unreconstructable) ***
// a2_xor_diagonal_cubes FAILS with `NonManifoldOutput`. XOR keeps ALL 48
// arrangement tris (verified) and the kept sub-mesh IS watertight (0 unpaired
// half-edges with the XOR flip applied), so the keep/flip path is correct.
// The failure is in `reconstruct_topology`: XOR yields TWO disjoint closed
// shells, and per-attribution flood-fill patches produce boundary cycles that
// `patch_boundary_cycle` rejects (multi-cycle / dead-end) → NonManifoldOutput.
// XOR is wired through `keep_set` + `flip_for_op` but has NO M3 spec oracle
// and is exercised by no existing test — a DoD §1.2 untested-branch gap.
// Owner: Implementer (reconstruct_topology multi-shell) or Manager (declare
// XOR out of M3 scope and gate it).
// =========================================================================

#[test]
fn a2_subtract_independent_geometry_positive_sign() {
    // A − B on the independent geometry: +0.874 (positive, correct sign).
    // FAILS — BUG-1 (weld guard), not a sign bug. See ATTACK 2 header.
    audit(
        BoolOp::Subtract,
        &cube([0.0, 0.0, 0.0]),
        &cube(OFF_B),
        1.0 - OVERLAP,
    );
}

#[test]
fn a2_xor_diagonal_cubes() {
    // XOR's symmetric-difference result is multi-shell / has a void
    // (A∖B and B∖A) that reconstruct_topology cannot reassemble yet, so XOR is
    // DEFERRED from M3 (spec §Scope) — boolean() must error LOUDLY with
    // YangError::UnsupportedOp, not a generic NonManifoldOutput or a silently
    // wrong result.
    let Some(sb) = yang_rs::native_backend() else {
        eprintln!("[m3_adversary] SKIP: native FFI shim not linked (stub build)");
        return;
    };
    let a = cube([0.0, 0.0, 0.0]);
    let b = cube([0.5, 0.5, 0.5]);
    match boolean(&a, &b, BoolOp::Xor, &sb) {
        Err(YangError::UnsupportedOp(BoolOp::Xor)) => {}
        other => panic!("xor must be deferred (UnsupportedOp(Xor)), got {other:?}"),
    }
}

#[test]
fn a2_xor_independent_geometry() {
    // XOR is deferred (spec §Scope) — must error loudly regardless of geometry.
    let Some(sb) = yang_rs::native_backend() else {
        eprintln!("[m3_adversary] SKIP: native FFI shim not linked (stub build)");
        return;
    };
    let a = cube([0.0, 0.0, 0.0]);
    let b = cube(OFF_B);
    match boolean(&a, &b, BoolOp::Xor, &sb) {
        Err(YangError::UnsupportedOp(BoolOp::Xor)) => {}
        other => panic!("xor(indep) must be deferred (UnsupportedOp(Xor)), got {other:?}"),
    }
}

// =========================================================================
// ATTACK 3 — Out-of-scope coplanar overlap must FAIL LOUDLY, not silently wrong.
//
// A@[0,0,0], B@[1,0,0] abut sharing the x=1 plane (A's right face == B's left
// face).
//
// HISTORY: the pre-M8 contract was "error loudly" (Stage-6 F2
// FaceResolutionFailed, later MeshBooleanFailed(CoplanarPairDeferred) under
// the native backend). PR-YR26 (M8 slice b) HANDLES planar A×B coplanar
// pairs via the §4.5.5 Stage-0 overlay, so the contract flips: these cases
// must now produce the CORRECT solid, asserted against the analytic volume
// + watertightness — a far stronger oracle than any-Err.
// =========================================================================

#[test]
fn a3_coplanar_shared_face_union_is_merged_box() {
    let Some(sb) = yang_rs::native_backend() else {
        eprintln!("[m3_adversary] SKIP: native FFI shim not linked (stub build)");
        return;
    };
    let a = cube([0.0, 0.0, 0.0]);
    let b = cube([1.0, 0.0, 0.0]); // shares plane x=1 (opposite normals)
    let r = boolean(&a, &b, BoolOp::Union, &sb)
        .expect("PR-YR26: coplanar abut is handled by the Stage-0 overlay");
    let mesh = r.as_mesh();
    assert_eq!(unpaired_half_edges(mesh), 0, "a3: union watertight");
    let vol = signed_volume(mesh);
    assert!(
        (vol - 2.0).abs() < 1e-9,
        "a3: side-by-side unit cubes union to the 2×1×1 box, got vol {vol}"
    );
    assert_eq!(euler_characteristic(mesh), 2, "a3: χ = 2");
}

#[test]
fn a3_coplanar_face_overlap_x_offset_union_is_exact() {
    // B offset only in x by 0.5: the solids overlap in [0.5,1]×[0,1]² AND
    // share the y∈{0,1} / z∈{0,1} face planes (4 simultaneous equal-normal
    // coplanar pairs). PR-YR26: all four pairs are overlaid; the overlap
    // sheets on each shared plane are part of the union boundary (equal
    // normals → keep). Union volume = 1 + 1 − 0.5 = 1.5.
    let Some(sb) = yang_rs::native_backend() else {
        eprintln!("[m3_adversary] SKIP: native FFI shim not linked (stub build)");
        return;
    };
    let a = cube([0.0, 0.0, 0.0]);
    let b = cube([0.5, 0.0, 0.0]);
    let r = boolean(&a, &b, BoolOp::Union, &sb)
        .expect("PR-YR26: 4-pair coplanar overlap is handled by the Stage-0 overlay");
    let mesh = r.as_mesh();
    assert_eq!(unpaired_half_edges(mesh), 0, "a3-xoff: union watertight");
    let vol = signed_volume(mesh);
    assert!(
        (vol - 1.5).abs() < 1e-9,
        "a3-xoff: union volume must be 1.5, got {vol}"
    );
    assert_eq!(euler_characteristic(mesh), 2, "a3-xoff: χ = 2");
}

// =========================================================================
// ATTACK 4 — I6 weld guard.
//
// Feed boolean() a backend whose labeled_arrangement returns a mesh with two
// distinct vertex indices at identical coords. The bit-exact guard must return
// NonManifoldInput.
// =========================================================================

struct WeldMockBackend {
    arrangement: LabeledArrangement,
}
impl MeshBoolean for WeldMockBackend {
    fn boolean(
        &self,
        _a: &Mesh,
        _b: &Mesh,
        _op: BoolOp,
    ) -> Result<Mesh, Box<dyn Error + Send + Sync>> {
        Ok(self.arrangement.mesh.clone())
    }
    fn labeled_arrangement(
        &self,
        _a: &Mesh,
        _b: &Mesh,
    ) -> Result<LabeledArrangement, Box<dyn Error + Send + Sync>> {
        Ok(self.arrangement.clone())
    }
}

#[test]
fn a4_bit_exact_coincident_verts_trip_weld_guard() {
    let a = cube([0.0, 0.0, 0.0]);
    let b = cube([0.5, 0.5, 0.5]);
    // Verts 0 and 3 are bit-identical (0,0,0) at distinct indices.
    let verts = vec![
        p(0.0, 0.0, 0.0), // 0
        p(1.0, 0.0, 0.0), // 1
        p(0.0, 1.0, 0.0), // 2
        p(0.0, 0.0, 0.0), // 3 — DUPLICATE of vertex 0 (distinct index)
    ];
    let mesh = Mesh::new(verts, vec![[0u32, 1, 2], [3, 1, 2]]);
    let la = LabeledArrangement {
        mesh,
        surface: vec![vec![LaInputId(0)]; 2],
        inside: vec![vec![false, false]; 2],
        patch: vec![0u32, 0],
        source: Vec::new(),
        intersection_edges: Default::default(),
        num_inputs: 2,
    };
    let backend = WeldMockBackend { arrangement: la };
    match boolean(&a, &b, BoolOp::Union, &backend) {
        Err(YangError::NonManifoldInput) => {}
        other => panic!("expected NonManifoldInput from weld guard, got {other:?}"),
    }
}

#[test]
fn a4_near_coincident_within_tau_work_trips_guard_planar() {
    // I6 STRICTNESS ASSESSMENT (asserting the CURRENT behavior).
    //
    // PR-KV10 closed the gap this test used to document: for ALL-PLANAR
    // input pairs (these cube fixtures) the weld is NEAR-aware within the
    // scale-relative `TAU_WORK·(1+|coord|)` band, because the exact
    // arrangement of chained oblique planar inputs legitimately mints
    // femto-distinct copies of one junction point (the F0016-class corpus
    // residue). A duplicate perturbed by 1e-13 therefore WELDS like the
    // bit-exact one, and the two coincident triangles trip the same
    // NonManifoldInput guard as `a4_bit_exact_coincident_verts_trip_weld_guard`.
    // (Curved pipelines keep the bit-exact weld — Stage-4 owns junction
    // duplicate collapse there.)
    let a = cube([0.0, 0.0, 0.0]);
    let b = cube([0.5, 0.5, 0.5]);
    let verts = vec![
        p(0.0, 0.0, 0.0),
        p(1.0, 0.0, 0.0),
        p(0.0, 1.0, 0.0),
        p(1e-13, 0.0, 0.0), // within TAU_WORK of vert 0 but NOT bit-identical
    ];
    let mesh = Mesh::new(verts, vec![[0u32, 1, 2], [3, 1, 2]]);
    let la = LabeledArrangement {
        mesh,
        surface: vec![vec![LaInputId(0)]; 2],
        inside: vec![vec![false, false]; 2],
        patch: vec![0u32, 0],
        source: Vec::new(),
        intersection_edges: Default::default(),
        num_inputs: 2,
    };
    let backend = WeldMockBackend { arrangement: la };
    match boolean(&a, &b, BoolOp::Union, &backend) {
        Err(YangError::NonManifoldInput) => {}
        other => panic!("expected NonManifoldInput from the near-aware weld guard, got {other:?}"),
    }
}

// =========================================================================
// ATTACK 5 — Determinism: identical output across two runs.
// =========================================================================

#[test]
fn a5_determinism_union_diagonal_cubes() {
    let Some(sb) = yang_rs::native_backend() else {
        eprintln!("[m3_adversary] SKIP: native FFI shim not linked (stub build)");
        return;
    };
    let a = cube([0.0, 0.0, 0.0]);
    let b = cube([0.5, 0.5, 0.5]);
    let r1 = boolean(&a, &b, BoolOp::Union, &sb).expect("run 1");
    let r2 = boolean(&a, &b, BoolOp::Union, &sb).expect("run 2");

    assert_eq!(
        r1.as_mesh().verts,
        r2.as_mesh().verts,
        "determinism: mesh.verts differ across runs"
    );
    assert_eq!(
        r1.as_mesh().tris,
        r2.as_mesh().tris,
        "determinism: mesh.tris differ across runs"
    );
    assert_eq!(
        r1.faces().len(),
        r2.faces().len(),
        "determinism: face count differs across runs"
    );
    assert_eq!(
        r1.triangle_attribution().len(),
        r2.triangle_attribution().len(),
        "determinism: attribution len differs"
    );
    for t in 0..r1.triangle_attribution().len() as u32 {
        assert_eq!(
            r1.triangle_attribution().lookup(t),
            r2.triangle_attribution().lookup(t),
            "determinism: attribution[{t}] differs across runs"
        );
    }
}

// =========================================================================
// ATTACK 6 — Face-resolution robustness (MockBackend, hand-built arrangement).
// =========================================================================

struct LabelMock {
    arrangement: LabeledArrangement,
}
impl MeshBoolean for LabelMock {
    fn boolean(
        &self,
        _a: &Mesh,
        _b: &Mesh,
        _op: BoolOp,
    ) -> Result<Mesh, Box<dyn Error + Send + Sync>> {
        Ok(self.arrangement.mesh.clone())
    }
    fn labeled_arrangement(
        &self,
        _a: &Mesh,
        _b: &Mesh,
    ) -> Result<LabeledArrangement, Box<dyn Error + Send + Sync>> {
        Ok(self.arrangement.clone())
    }
}

#[test]
fn a6_equidistant_two_planes_tie_fails_resolution() {
    // A genuine attribution tie on a NON-DEGENERATE (positive-area) triangle
    // must FaceResolutionFailed (F3 tie), not silently pick one.
    //
    // Post-fix scoping of the F3 tie-guard: the M3 production rule treats a
    // DEGENERATE (zero-area) triangle as a harmless edge-sliver — it sits on a
    // solid edge where two adjacent planes always tie — and attributes it
    // deterministically to the lowest tied face (slivers are load-bearing for
    // edge-pairing / watertightness, so dropping or erroring on them is wrong).
    // Therefore the F3 tie-guard is scoped to REAL (positive-area) triangles
    // only. This test exercises that scoped case: a triangle with substantial
    // area whose CENTROID is genuinely ambiguous between two of solid A's face
    // planes.
    //
    // Fixture: cube A @ origin (planes x=0, x=1, y=0, y=1, z=0, z=1). A real
    // 2D triangle in the plane z=0.5 with vertices whose x- and y-coordinates
    // each sum to zero ⇒ centroid = (0, 0, 0.5):
    //   - ‖cross(e1, e2)‖ = 1.5  ≫ MIN_FEATURE_SIZE² (1e-12) ⇒ NON-degenerate.
    //   - centroid distance to plane x=0 is 0 and to plane y=0 is 0 (both well
    //     within TAU_WORK=1e-12) ⇒ a genuine 2-plane tie.
    //   - every OTHER A plane is ≥ 0.5 away (x=1, y=1 at 1.0; z=0, z=1 at 0.5),
    //     so exactly two planes tie at distance 0 — unambiguously F3.
    let a = cube([0.0, 0.0, 0.0]);
    let b = cube([0.5, 0.5, 0.5]);
    let verts = vec![p(-0.5, -0.5, 0.5), p(0.5, -0.5, 0.5), p(0.0, 1.0, 0.5)];
    let mesh = Mesh::new(verts, vec![[0u32, 1, 2]]);
    let la = LabeledArrangement {
        mesh,
        surface: vec![vec![LaInputId(0)]],
        inside: vec![vec![false, false]],
        patch: vec![0],
        source: Vec::new(),
        intersection_edges: Default::default(),
        num_inputs: 2,
    };
    let backend = LabelMock { arrangement: la };
    match boolean(&a, &b, BoolOp::Union, &backend) {
        Err(YangError::FaceResolutionFailed { tri }) => assert_eq!(tri, 0),
        other => panic!("expected FaceResolutionFailed (tie), got {other:?}"),
    }
}

#[test]
fn a6_mislabeled_tri_on_a_plane_but_labeled_b_fails() {
    // surface label names solid B, but the tri geometry lies on an A plane
    // (z=0) and on NO B plane (B is at [0.5,0.5,0.5], planes z∈{0.5,1.5}).
    // Centroid off all B planes → F3 FaceResolutionFailed.
    let a = cube([0.0, 0.0, 0.0]);
    let b = cube([0.5, 0.5, 0.5]);
    // Centroid lies in z=0 plane (an A plane), but label claims B.
    let verts = vec![p(0.6, 0.6, 0.0), p(0.8, 0.6, 0.0), p(0.7, 0.8, 0.0)];
    let mesh = Mesh::new(verts, vec![[0u32, 1, 2]]);
    let la = LabeledArrangement {
        mesh,
        surface: vec![vec![LaInputId(1)]], // claims B
        inside: vec![vec![false, false]],
        patch: vec![0],
        source: Vec::new(),
        intersection_edges: Default::default(),
        num_inputs: 2,
    };
    let backend = LabelMock { arrangement: la };
    match boolean(&a, &b, BoolOp::Union, &backend) {
        Err(YangError::FaceResolutionFailed { tri }) => assert_eq!(tri, 0),
        other => panic!("expected FaceResolutionFailed (mislabeled), got {other:?}"),
    }
}
