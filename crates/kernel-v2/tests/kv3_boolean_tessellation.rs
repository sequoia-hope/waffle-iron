//! PR-KV3 RED oracles: boolean delegation to yang-rs, render tessellation,
//! edge extraction, and introspection basics.
//!
//! - Round-trip: box → `to_yang_brep` → `from_yang_brep` (no boolean) →
//!   `validate_solid` green, identical (V,E,F,R,S,G), exact volume;
//!   `to_yang_brep` structure (8 verts / 24 per-face directed edges /
//!   6 planar faces, directed-continuous loops, outward planes).
//! - Boolean truth (cherchi parity-corpus geometry — 2³ boxes overlapping
//!   in a unit corner cube): Union/Intersect/Subtract volumes EXACTLY
//!   15 / 1 / 7, `validate_solid` green (watertight 2-manifold by
//!   definition of the suite), positive signed volume, Euler bookkeeping.
//! - Through-cut Subtract: 4³ box minus a 1×1 rod through ⇒ genus 1,
//!   two rings, χ lhs = rhs = 0, volume exactly 60.
//! - Tessellation: box → 12 tris / 6 ranges, per-face area exact;
//!   L-prism (concave) and holed extrude → per-face tessellated area ==
//!   B-Rep face area (outer − holes), triangle normals == face Newell
//!   normal, no triangle centroid inside a hole, mesh signed volume ==
//!   solid signed volume (winding consistency); boolean output faces
//!   (rings + collinear chain vertices) tessellate too.
//! - `extract_edges`: box → 12 unique segments, each shared by exactly
//!   two face loops.
//! - Error paths: coplanar-touching boxes → `UnsupportedCoplanar` (typed
//!   M8 boundary) for ALL ops; disjoint Intersect → `EmptyBooleanResult`;
//!   XOR → `BooleanFailed` (yang defers XOR, loud); curved yang output
//!   face → `UnsupportedBooleanOutputSurface` (mapper unit test on a
//!   hand-built cylinder `yang_rs::BRep`).
//! - Determinism: identical inputs ⇒ identical arenas and RenderMeshes.

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::geom::{newell, signed_volume};
use kernel_v2::*;

// =========================================================================
// Construction helpers (KV2 extrudes)
// =========================================================================

fn p2(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn p3(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// Axis-aligned box `lo..hi` via square profile + extrude (+z).
fn make_box(arena: &mut BrepArena, lo: [f64; 3], hi: [f64; 3]) -> SolidId {
    let profile = Profile::new(
        p3(0.0, 0.0, lo[2]),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            p2(lo[0], lo[1]),
            p2(hi[0], lo[1]),
            p2(hi[0], hi[1]),
            p2(lo[0], hi[1]),
        ],
        vec![],
    )
    .expect("box profile valid");
    extrude(arena, &profile, Vector3::new(0.0, 0.0, 1.0), hi[2] - lo[2])
        .expect("box extrude")
        .solid
}

/// Concave L prism (KV2 geometry): area 3, height 0.5, volume 1.5.
fn make_l_prism(arena: &mut BrepArena) -> SolidId {
    let profile = Profile::new(
        p3(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            p2(0.0, 0.0),
            p2(2.0, 0.0),
            p2(2.0, 1.0),
            p2(1.0, 1.0),
            p2(1.0, 2.0),
            p2(0.0, 2.0),
        ],
        vec![],
    )
    .expect("L profile valid");
    extrude(arena, &profile, Vector3::new(0.0, 0.0, 1.0), 0.5)
        .expect("L prism")
        .solid
}

/// 4×4 outer, 1×1 hole at (1,1)..(2,2), height 2 (KV2 geometry): vol 30.
fn make_holed_prism(arena: &mut BrepArena) -> SolidId {
    let profile = Profile::new(
        p3(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![p2(0.0, 0.0), p2(4.0, 0.0), p2(4.0, 4.0), p2(0.0, 4.0)],
        vec![vec![p2(1.0, 1.0), p2(2.0, 1.0), p2(2.0, 2.0), p2(1.0, 2.0)]],
    )
    .expect("holed profile valid");
    extrude(arena, &profile, Vector3::new(0.0, 0.0, 1.0), 2.0)
        .expect("holed prism")
        .solid
}

#[track_caller]
fn assert_counts(arena: &BrepArena, solid: SolidId, expect: (i64, i64, i64, i64, i64, i64)) {
    let c = arena.euler_counts(solid).expect("euler_counts");
    assert_eq!(
        (c.v, c.e, c.f, c.r, c.s, c.g),
        expect,
        "element counts (V,E,F,R,S,G)"
    );
    assert!(
        c.holds(),
        "Euler–Poincaré violated: {} != {}",
        c.lhs(),
        c.rhs()
    );
}

/// All face ids of a solid, in shell walk order.
fn solid_faces(arena: &BrepArena, solid: SolidId) -> Vec<FaceId> {
    let mut out = Vec::new();
    for &sh in &arena.solid(solid).expect("solid").shells {
        out.extend(arena.shell(sh).expect("shell").faces.iter().copied());
    }
    out
}

/// B-Rep face area: `(Newell(outer) + Σ Newell(ring)) · n̂ / 2` — rings wind
/// opposite, so holes subtract.
fn arena_face_area(arena: &BrepArena, face: FaceId) -> f64 {
    let f = arena.face(face).expect("face");
    let n = face_plane(arena, face).expect("plane").normal;
    let mut twice = {
        let pts = arena.loop_points(f.outer_loop).expect("outer pts");
        let nw = newell(&pts);
        nw[0] * n.x + nw[1] * n.y + nw[2] * n.z
    };
    for &rid in &f.inner_loops {
        let pts = arena.loop_points(rid).expect("ring pts");
        let nw = newell(&pts);
        twice += nw[0] * n.x + nw[1] * n.y + nw[2] * n.z;
    }
    twice / 2.0
}

// =========================================================================
// RenderMesh helpers
// =========================================================================

fn mesh_tri(mesh: &RenderMesh, t: usize) -> [[f64; 3]; 3] {
    let mut out = [[0.0; 3]; 3];
    for (k, slot) in out.iter_mut().enumerate() {
        let vi = mesh.indices[3 * t + k] as usize;
        *slot = [
            mesh.positions[3 * vi],
            mesh.positions[3 * vi + 1],
            mesh.positions[3 * vi + 2],
        ];
    }
    out
}

fn tri_cross(tri: &[[f64; 3]; 3]) -> [f64; 3] {
    let e1 = [
        tri[1][0] - tri[0][0],
        tri[1][1] - tri[0][1],
        tri[1][2] - tri[0][2],
    ];
    let e2 = [
        tri[2][0] - tri[0][0],
        tri[2][1] - tri[0][1],
        tri[2][2] - tri[0][2],
    ];
    [
        e1[1] * e2[2] - e1[2] * e2[1],
        e1[2] * e2[0] - e1[0] * e2[2],
        e1[0] * e2[1] - e1[1] * e2[0],
    ]
}

/// Signed area sum of one face's range, measured along the face normal.
fn range_area(mesh: &RenderMesh, range: &FaceRange, n: [f64; 3]) -> f64 {
    let mut twice = 0.0;
    for t in (range.start as usize / 3)..((range.start + range.count) as usize / 3) {
        let c = tri_cross(&mesh_tri(mesh, t));
        twice += c[0] * n[0] + c[1] * n[1] + c[2] * n[2];
    }
    twice / 2.0
}

fn mesh_signed_volume(mesh: &RenderMesh) -> f64 {
    let mut six = 0.0;
    for t in 0..mesh.num_triangles() {
        let tri = mesh_tri(mesh, t);
        let (a, b, c) = (tri[0], tri[1], tri[2]);
        six += a[0] * (b[1] * c[2] - b[2] * c[1])
            + a[1] * (b[2] * c[0] - b[0] * c[2])
            + a[2] * (b[0] * c[1] - b[1] * c[0]);
    }
    six / 6.0
}

/// Full-mesh oracle bundle shared by the tessellation tests:
/// - ranges are contiguous, multiples of 3, cover `indices` exactly, and
///   list the solid's faces in walk order;
/// - every triangle's normal direction equals its face's plane normal
///   (within 1e-12 after normalization — winding follows the walk);
/// - every face's tessellated (signed) area equals its B-Rep area
///   (outer − holes) within 1e-12;
/// - every vertex's stored normal is its face's plane normal;
/// - mesh signed volume equals the solid's B-Rep signed volume (1e-12).
#[track_caller]
fn assert_mesh_matches_solid(arena: &BrepArena, solid: SolidId, mesh: &RenderMesh) {
    let faces = solid_faces(arena, solid);
    assert_eq!(
        mesh.face_ranges.len(),
        faces.len(),
        "one range per face, in walk order"
    );
    let mut cursor = 0u32;
    for (range, &fid) in mesh.face_ranges.iter().zip(&faces) {
        assert_eq!(range.face, fid, "range face order");
        assert_eq!(range.start, cursor, "ranges contiguous");
        assert_eq!(range.count % 3, 0, "whole triangles");
        cursor += range.count;

        let n = face_plane(arena, fid).expect("plane").normal;
        let n = [n.x, n.y, n.z];
        // Triangle winding + per-vertex normals.
        for t in (range.start as usize / 3)..((range.start + range.count) as usize / 3) {
            let tri = mesh_tri(mesh, t);
            let c = tri_cross(&tri);
            let len = (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt();
            assert!(len > 0.0, "degenerate emitted triangle {t}");
            for k in 0..3 {
                assert!(
                    (c[k] / len - n[k]).abs() <= 1e-12,
                    "tri {t} normal {:?} != face normal {:?}",
                    [c[0] / len, c[1] / len, c[2] / len],
                    n
                );
            }
            for k in 0..3 {
                let vi = mesh.indices[3 * t + k] as usize;
                let vn = [
                    mesh.normals[3 * vi],
                    mesh.normals[3 * vi + 1],
                    mesh.normals[3 * vi + 2],
                ];
                assert!(
                    (vn[0] - n[0]).abs() <= 1e-12
                        && (vn[1] - n[1]).abs() <= 1e-12
                        && (vn[2] - n[2]).abs() <= 1e-12,
                    "vertex normal != face normal"
                );
            }
        }
        // Exact area cover.
        let got = range_area(mesh, range, n);
        let want = arena_face_area(arena, fid);
        assert!(
            (got - want).abs() <= 1e-12,
            "face {fid:?} tessellated area {got} != B-Rep area {want}"
        );
    }
    assert_eq!(cursor as usize, mesh.indices.len(), "ranges cover indices");
    assert_eq!(mesh.positions.len(), mesh.normals.len());
    assert_eq!(mesh.positions.len() % 3, 0);
    for &i in &mesh.indices {
        assert!((i as usize) < mesh.num_vertices(), "index in range");
    }

    let vol = signed_volume(arena, solid).expect("solid volume");
    let mvol = mesh_signed_volume(mesh);
    assert!(
        (mvol - vol).abs() <= 1e-12,
        "mesh signed volume {mvol} != solid volume {vol} (winding consistency)"
    );
}

// =========================================================================
// Round-trip conversion
// =========================================================================

#[test]
fn to_yang_box_structure() {
    let mut arena = BrepArena::new();
    let solid = make_box(&mut arena, [0.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
    let yb = to_yang_brep(&arena, solid).expect("to_yang_brep");

    assert_eq!(yb.vertices().len(), 8, "8 corner vertices");
    assert_eq!(yb.edges().len(), 24, "per-face directed edges: 6 × 4");
    assert_eq!(yb.faces().len(), 6);

    // Vertex set is exactly the corner set.
    let mut got: Vec<[i64; 3]> = yb
        .vertices()
        .iter()
        .map(|v| {
            let a = v.point.as_array();
            [a[0] as i64, a[1] as i64, a[2] as i64]
        })
        .collect();
    got.sort_unstable();
    let mut want = Vec::new();
    for z in [0i64, 2] {
        for y in [0i64, 2] {
            for x in [0i64, 2] {
                want.push([x, y, z]);
            }
        }
    }
    want.sort_unstable();
    assert_eq!(got, want);

    for (i, f) in yb.faces().iter().enumerate() {
        let yang_rs::Surface::Plane { normal, d } = f.surface else {
            panic!("face {i}: non-planar surface from planar solid");
        };
        assert!(f.inner_loops.is_empty(), "box face has no rings");
        assert!(!f.reversed, "planar faces encode sense in the normal");
        assert_eq!(f.outer_loop.len(), 4, "quad loop");
        // Directed continuity + closure.
        for k in 0..f.outer_loop.len() {
            let e0 = &yb.edges()[f.outer_loop[k] as usize];
            let e1 = &yb.edges()[f.outer_loop[(k + 1) % f.outer_loop.len()] as usize];
            assert_eq!(e0.end, e1.start, "face {i}: loop discontinuity");
            assert_eq!(e0.curve, yang_rs::Curve::LineSegment);
        }
        // Plane passes exactly through every loop vertex, outward of the
        // solid centroid (1,1,1). All-dyadic input ⇒ exact equality.
        let n = normal.as_array();
        for &ei in &f.outer_loop {
            let v = yb.vertices()[yb.edges()[ei as usize].start as usize]
                .point
                .as_array();
            assert_eq!(n[0] * v[0] + n[1] * v[1] + n[2] * v[2] + d, 0.0);
        }
        assert!(n[0] + n[1] + n[2] + d < 0.0, "face {i}: normal not outward");
    }
}

#[test]
fn round_trip_box_no_boolean() {
    let mut arena = BrepArena::new();
    let solid = make_box(&mut arena, [0.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
    let before = arena.euler_counts(solid).expect("counts");
    let vol_before = signed_volume(&arena, solid).expect("vol");
    assert_eq!(vol_before, 8.0);

    let yb = to_yang_brep(&arena, solid).expect("to_yang_brep");
    let rebuilt = from_yang_brep(&mut arena, &yb).expect("from_yang_brep");
    assert_ne!(rebuilt, solid, "rebuilt is a NEW solid");

    let report = validate_solid(&arena, rebuilt).expect("round-trip validates");
    let after = arena.euler_counts(rebuilt).expect("counts");
    assert_eq!(
        (after.v, after.e, after.f, after.r, after.s, after.g),
        (before.v, before.e, before.f, before.r, before.s, before.g),
        "identical element counts through the round trip"
    );
    assert_eq!((report.vertices, report.edges, report.faces), (8, 12, 6));
    assert_eq!(
        signed_volume(&arena, rebuilt).expect("vol"),
        8.0,
        "exact volume through the round trip"
    );
    // The original solid is untouched.
    assert_eq!(signed_volume(&arena, solid).expect("vol"), 8.0);
    validate_solid(&arena, solid).expect("original still validates");
}

// =========================================================================
// Boolean truth (corner-overlap 2³ boxes: |A| = |B| = 8, |A∩B| = 1)
// =========================================================================

fn corner_boxes(arena: &mut BrepArena) -> (SolidId, SolidId) {
    let a = make_box(arena, [0.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
    let b = make_box(arena, [1.0, 1.0, 1.0], [3.0, 3.0, 3.0]);
    (a, b)
}

#[test]
fn boolean_union_volume_15() {
    let mut arena = BrepArena::new();
    let (a, b) = corner_boxes(&mut arena);
    let out = boolean_op(&mut arena, a, b, BoolOp::Union).expect("union");
    validate_solid(&arena, out).expect("union validates");
    assert_counts(&arena, out, (20, 30, 12, 0, 1, 0));
    let vol = signed_volume(&arena, out).expect("vol");
    assert!(vol > 0.0, "outward orientation");
    assert_eq!(vol, 15.0, "8 + 8 − 1, exactly");
}

#[test]
fn boolean_intersect_volume_1() {
    let mut arena = BrepArena::new();
    let (a, b) = corner_boxes(&mut arena);
    let out = boolean_op(&mut arena, a, b, BoolOp::Intersect).expect("intersect");
    validate_solid(&arena, out).expect("intersect validates");
    assert_counts(&arena, out, (8, 12, 6, 0, 1, 0));
    assert_eq!(signed_volume(&arena, out).expect("vol"), 1.0);
}

#[test]
fn boolean_subtract_volume_7() {
    let mut arena = BrepArena::new();
    let (a, b) = corner_boxes(&mut arena);
    let out = boolean_op(&mut arena, a, b, BoolOp::Subtract).expect("subtract");
    validate_solid(&arena, out).expect("subtract validates");
    assert_counts(&arena, out, (14, 21, 9, 0, 1, 0));
    let vol = signed_volume(&arena, out).expect("vol");
    assert!(vol > 0.0, "outward orientation");
    assert_eq!(vol, 7.0, "8 − 1, exactly");
}

/// Inputs stay live and unmodified next to the result.
#[test]
fn boolean_inputs_survive() {
    let mut arena = BrepArena::new();
    let (a, b) = corner_boxes(&mut arena);
    let _ = boolean_op(&mut arena, a, b, BoolOp::Union).expect("union");
    validate_solid(&arena, a).expect("input A still validates");
    validate_solid(&arena, b).expect("input B still validates");
    assert_eq!(signed_volume(&arena, a).expect("vol"), 8.0);
    assert_eq!(signed_volume(&arena, b).expect("vol"), 8.0);
}

// =========================================================================
// Through-cut Subtract: genus bookkeeping
// =========================================================================

#[test]
fn through_cut_subtract_genus_1() {
    let mut arena = BrepArena::new();
    let a = make_box(&mut arena, [0.0, 0.0, 0.0], [4.0, 4.0, 4.0]);
    let rod = make_box(&mut arena, [1.5, 1.5, -1.0], [2.5, 2.5, 5.0]);
    let out = boolean_op(&mut arena, a, rod, BoolOp::Subtract).expect("through-cut");

    let report = validate_solid(&arena, out).expect("through-cut validates");
    assert_eq!((report.rings, report.genus), (2, 1), "annulus top+bottom");
    assert_eq!((report.euler_lhs, report.euler_rhs), (0, 0), "χ = 0 torus");
    // PR-KV7: output curve recovery fuses the collinear T-vertex splits the
    // arrangement leaves on the box edges (8 verts + 8 edges fewer than the
    // raw mesh-granular output; faces/rings/genus/volume identical).
    assert_counts(&arena, out, (16, 24, 10, 2, 1, 1));
    assert_eq!(
        signed_volume(&arena, out).expect("vol"),
        60.0,
        "64 − 4, exactly"
    );
}

// =========================================================================
// Multi-shell output (disjoint union) + empty result
// =========================================================================

#[test]
fn boolean_disjoint_union_two_shells() {
    let mut arena = BrepArena::new();
    let a = make_box(&mut arena, [0.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
    let b = make_box(&mut arena, [5.0, 5.0, 5.0], [7.0, 7.0, 7.0]);
    let out = boolean_op(&mut arena, a, b, BoolOp::Union).expect("disjoint union");
    let report = validate_solid(&arena, out).expect("disjoint union validates");
    assert_eq!(report.shells, 2, "two disjoint closed shells");
    assert_counts(&arena, out, (16, 24, 12, 0, 2, 0));
    assert_eq!(signed_volume(&arena, out).expect("vol"), 16.0);
}

#[test]
fn boolean_disjoint_intersect_is_empty() {
    let mut arena = BrepArena::new();
    let a = make_box(&mut arena, [0.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
    let b = make_box(&mut arena, [5.0, 5.0, 5.0], [7.0, 7.0, 7.0]);
    let err = boolean_op(&mut arena, a, b, BoolOp::Intersect).unwrap_err();
    assert_eq!(err, KernelV2Error::EmptyBooleanResult);
}

// =========================================================================
// Error paths
// =========================================================================

/// Coplanar-touching boxes (shared x=2 face): the cherchi arrangement
/// defers coplanar pairs — Yang Stage 0 / roadmap M8 boundary. Must be
/// the TYPED UnsupportedCoplanar for every op, not a generic failure.
#[test]
fn boolean_coplanar_touching_succeeds_via_stage0_overlay() {
    // PR-YR26 (M8 slice b) CONTRACT CHANGE: planar A×B coplanar face pairs
    // are now HANDLED by yang-rs's §4.5.5 Stage-0 overlay (canonical-plane
    // snap + exact 2D overlay + identical overlap meshes), so these cases
    // produce the CORRECT solid instead of the typed `UnsupportedCoplanar`
    // M8 wall — a strictly stronger oracle. The wall remains only for the
    // unsupported residue (intra-solid near pairs, curved/multi-pair faces).
    let mut arena = BrepArena::new();
    let a = make_box(&mut arena, [0.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
    let b = make_box(&mut arena, [2.0, 0.0, 0.0], [4.0, 2.0, 2.0]);
    // Side-by-side boxes sharing the full x = 2 face (opposite normals):
    // union = the merged 4×2×2 box (16); subtract A−B = A (8).
    let out = boolean_op(&mut arena, a, b, BoolOp::Union).expect("coplanar-touching union");
    validate_solid(&arena, out).expect("union validates");
    assert_eq!(
        signed_volume(&arena, out).expect("vol"),
        16.0,
        "8 + 8, exactly"
    );
    let out = boolean_op(&mut arena, a, b, BoolOp::Subtract).expect("coplanar-touching subtract");
    validate_solid(&arena, out).expect("subtract validates");
    assert_eq!(signed_volume(&arena, out).expect("vol"), 8.0, "A unchanged");
    // PR-YR27 hygiene (Finding 8): the Intersect cell was silently dropped
    // in the PR-YR26 rewrite of this test ("for ALL ops" — header line 22).
    // Touching boxes intersect in the zero-volume shared sheet only, which
    // is NOT a solid: kernel-v2's documented empty-result contract applies.
    let err = boolean_op(&mut arena, a, b, BoolOp::Intersect).unwrap_err();
    assert_eq!(
        err,
        KernelV2Error::EmptyBooleanResult,
        "coplanar-touching intersect: zero-volume sheet is empty, typed"
    );
    // Coplanar OVERLAP (shared bottom plane, EQUAL normals): the overlap
    // sheet is part of the union's bottom face. 8 + 8 − 1·1·2 = 14.
    let c = make_box(&mut arena, [1.0, 1.0, 0.0], [3.0, 3.0, 2.0]);
    let out = boolean_op(&mut arena, a, c, BoolOp::Union).expect("shared-bottom union");
    validate_solid(&arena, out).expect("shared-bottom union validates");
    assert_eq!(
        signed_volume(&arena, out).expect("vol"),
        14.0,
        "8 + 8 − 2, exactly"
    );
}

/// XOR is deferred by yang-rs (UnsupportedOp) — surfaces loudly as
/// BooleanFailed carrying the yang Display text.
#[test]
fn boolean_xor_fails_loud() {
    let mut arena = BrepArena::new();
    let (a, b) = corner_boxes(&mut arena);
    let err = boolean_op(&mut arena, a, b, BoolOp::Xor).unwrap_err();
    let KernelV2Error::BooleanFailed(msg) = err else {
        panic!("expected BooleanFailed, got {err:?}");
    };
    assert!(msg.contains("Xor"), "carries the yang error text: {msg}");
}

/// PR-KV5b FLIPPED the Phase-4a wall this test used to pin: a canonical
/// cylinder `yang_rs::BRep` (the M5 fixture shape) now reassembles into a
/// validated kernel-v2 cylinder solid instead of being rejected as an
/// unsupported output surface. (The remaining surface walls — Sphere/Cone
/// — keep `UnsupportedBooleanOutputSurface`; the named CURVE walls are
/// pinned end-to-end in tests/kv5b_curved_boolean.rs.)
#[test]
fn from_yang_accepts_canonical_cylinder_output_since_kv5b() {
    let cyl = yang_cylinder_brep([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0, 2.0);
    let mut arena = BrepArena::new();
    let solid = from_yang_brep(&mut arena, &cyl).expect("canonical cylinder reassembles (KV5b)");
    let report = validate_solid(&arena, solid).expect("validates");
    assert_eq!(
        (report.vertices, report.edges, report.faces, report.genus),
        (2, 3, 3, 0),
        "canonical cylinder topology V2/E3/F3/G0"
    );
}

/// An empty yang BRep (no faces) is the typed empty-result error.
#[test]
fn from_yang_empty_brep_is_empty_result() {
    let empty = yang_rs::BRep::new(vec![], vec![], vec![]).expect("empty BRep");
    let mut arena = BrepArena::new();
    let err = from_yang_brep(&mut arena, &empty).unwrap_err();
    assert_eq!(err, KernelV2Error::EmptyBooleanResult);
    assert_eq!(arena, BrepArena::new(), "arena untouched");
}

// =========================================================================
// Tessellation
// =========================================================================

#[test]
fn tessellate_box_12_tris_exact_area() {
    let mut arena = BrepArena::new();
    let solid = make_box(&mut arena, [0.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
    let mesh = tessellate(&arena, solid).expect("tessellate box");

    assert_eq!(mesh.num_triangles(), 12, "2 per quad face");
    assert_eq!(mesh.face_ranges.len(), 6);
    assert_eq!(
        mesh.num_vertices(),
        24,
        "per-face vertices: 4 per face, not shared across faces"
    );
    for range in &mesh.face_ranges {
        assert_eq!(range.count, 6, "2 triangles per box face");
        let n = face_plane(&arena, range.face).expect("plane").normal;
        let area = range_area(&mesh, range, [n.x, n.y, n.z]);
        assert_eq!(area, 4.0, "exact face area (dyadic coordinates)");
    }
    assert_mesh_matches_solid(&arena, solid, &mesh);
    assert_eq!(mesh_signed_volume(&mesh), 8.0, "exact mesh volume");
}

#[test]
fn tessellate_concave_l_prism() {
    let mut arena = BrepArena::new();
    let solid = make_l_prism(&mut arena);
    let mesh = tessellate(&arena, solid).expect("tessellate L prism");
    assert_mesh_matches_solid(&arena, solid, &mesh);
    assert!((mesh_signed_volume(&mesh) - 1.5).abs() <= 1e-12);
}

#[test]
fn tessellate_holed_prism_respects_hole() {
    let mut arena = BrepArena::new();
    let solid = make_holed_prism(&mut arena);
    let mesh = tessellate(&arena, solid).expect("tessellate holed prism");
    assert_mesh_matches_solid(&arena, solid, &mesh);
    assert!((mesh_signed_volume(&mesh) - 30.0).abs() <= 1e-12);

    // The two holed faces (base z=0, top z=2): area = 16 − 1 = 15, and no
    // triangle centroid falls strictly inside the hole square (1,1)..(2,2).
    let mut holed_faces = 0;
    for range in &mesh.face_ranges {
        let f = arena.face(range.face).expect("face");
        if f.inner_loops.is_empty() {
            continue;
        }
        holed_faces += 1;
        let n = face_plane(&arena, range.face).expect("plane").normal;
        let area = range_area(&mesh, range, [n.x, n.y, n.z]);
        assert!((area - 15.0).abs() <= 1e-12, "outer − hole, exactly");
        for t in (range.start as usize / 3)..((range.start + range.count) as usize / 3) {
            let tri = mesh_tri(&mesh, t);
            let cx = (tri[0][0] + tri[1][0] + tri[2][0]) / 3.0;
            let cy = (tri[0][1] + tri[1][1] + tri[2][1]) / 3.0;
            assert!(
                !(cx > 1.0 && cx < 2.0 && cy > 1.0 && cy < 2.0),
                "triangle {t} centroid ({cx},{cy}) inside the hole"
            );
        }
    }
    assert_eq!(holed_faces, 2, "base and top both carry the ring");
}

/// Boolean OUTPUT faces tessellate too: the through-cut result has holed
/// faces AND collinear chain vertices (split box edges) — the normal case
/// for downstream rendering of boolean results.
#[test]
fn tessellate_boolean_output() {
    let mut arena = BrepArena::new();
    let a = make_box(&mut arena, [0.0, 0.0, 0.0], [4.0, 4.0, 4.0]);
    let rod = make_box(&mut arena, [1.5, 1.5, -1.0], [2.5, 2.5, 5.0]);
    let out = boolean_op(&mut arena, a, rod, BoolOp::Subtract).expect("through-cut");
    let mesh = tessellate(&arena, out).expect("tessellate boolean output");
    assert_mesh_matches_solid(&arena, out, &mesh);
    assert!((mesh_signed_volume(&mesh) - 60.0).abs() <= 1e-12);

    // Also the corner-overlap union (collinear chains, no holes).
    let mut arena2 = BrepArena::new();
    let (a2, b2) = corner_boxes(&mut arena2);
    let union = boolean_op(&mut arena2, a2, b2, BoolOp::Union).expect("union");
    let mesh2 = tessellate(&arena2, union).expect("tessellate union");
    assert_mesh_matches_solid(&arena2, union, &mesh2);
    assert!((mesh_signed_volume(&mesh2) - 15.0).abs() <= 1e-12);
}

// =========================================================================
// Edge extraction + introspection
// =========================================================================

#[test]
fn extract_edges_box() {
    let mut arena = BrepArena::new();
    let solid = make_box(&mut arena, [0.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
    let segs = extract_edges(&arena, solid).expect("extract_edges");
    assert_eq!(segs.len(), 12, "box has 12 edges");

    // As unordered coordinate pairs: all distinct, and exactly the box
    // edge set (each axis-aligned, length 2).
    let key = |p: Point3| {
        let a = p.as_array();
        [a[0] as i64, a[1] as i64, a[2] as i64]
    };
    let mut pairs: Vec<[[i64; 3]; 2]> = segs
        .iter()
        .map(|s| {
            let (a, b) = (key(s[0]), key(s[1]));
            if a <= b {
                [a, b]
            } else {
                [b, a]
            }
        })
        .collect();
    pairs.sort_unstable();
    let n = pairs.len();
    pairs.dedup();
    assert_eq!(pairs.len(), n, "each edge reported once");
    for pr in &pairs {
        let d: i64 = (0..3).map(|k| (pr[1][k] - pr[0][k]).abs()).sum();
        assert_eq!(d, 2, "axis-aligned box edge of length 2: {pr:?}");
    }

    // Each segment is shared by exactly two face loops.
    for pr in &pairs {
        let mut uses = 0;
        for f in solid_faces(&arena, solid) {
            let face = arena.face(f).expect("face");
            let pts = arena.loop_points(face.outer_loop).expect("pts");
            let m = pts.len();
            for i in 0..m {
                let (a, b) = (key(pts[i]), key(pts[(i + 1) % m]));
                let s = if a <= b { [a, b] } else { [b, a] };
                if s == *pr {
                    uses += 1;
                }
            }
        }
        assert_eq!(uses, 2, "edge {pr:?} shared by exactly 2 faces");
    }
}

#[test]
fn introspection_basics() {
    let mut arena = BrepArena::new();
    let solid = make_box(&mut arena, [0.0, 0.0, 0.0], [2.0, 2.0, 2.0]);

    // Counts (KV1 surface, re-asserted here as the introspection contract).
    assert_counts(&arena, solid, (8, 12, 6, 0, 1, 0));
    // Signed volume (KV2 surface).
    assert_eq!(signed_volume(&arena, solid).expect("vol"), 8.0);
    // Surface area: 6 × 4 = 24, exactly.
    assert_eq!(surface_area(&arena, solid).expect("area"), 24.0);

    // face_plane: the 6 outward unit normals are exactly the axis set.
    let mut normals: Vec<[i64; 3]> = solid_faces(&arena, solid)
        .iter()
        .map(|&f| {
            let n = face_plane(&arena, f).expect("plane").normal;
            [n.x as i64, n.y as i64, n.z as i64]
        })
        .collect();
    normals.sort_unstable();
    let mut want = vec![
        [-1, 0, 0],
        [1, 0, 0],
        [0, -1, 0],
        [0, 1, 0],
        [0, 0, -1],
        [0, 0, 1],
    ];
    want.sort_unstable();
    assert_eq!(normals, want);

    // Surface area subtracts holes: holed prism = 2·(16−1) + 4·(4·2)
    // outer walls + 4·(1·2) hole walls = 30 + 32 + 8 = 70.
    let mut arena2 = BrepArena::new();
    let holed = make_holed_prism(&mut arena2);
    assert_eq!(surface_area(&arena2, holed).expect("area"), 70.0);
}

// =========================================================================
// Determinism
// =========================================================================

#[test]
fn boolean_and_tessellation_deterministic() {
    let run = || {
        let mut arena = BrepArena::new();
        let (a, b) = corner_boxes(&mut arena);
        let out = boolean_op(&mut arena, a, b, BoolOp::Subtract).expect("subtract");
        let mesh = tessellate(&arena, out).expect("tessellate");
        let edges = extract_edges(&arena, out).expect("edges");
        (arena, mesh, edges)
    };
    let (arena1, mesh1, edges1) = run();
    let (arena2, mesh2, edges2) = run();
    assert_eq!(arena1, arena2, "bit-identical arenas");
    assert_eq!(mesh1, mesh2, "bit-identical render meshes");
    assert_eq!(edges1.len(), edges2.len());
    for (e1, e2) in edges1.iter().zip(&edges2) {
        assert_eq!(e1[0].as_array(), e2[0].as_array());
        assert_eq!(e1[1].as_array(), e2[1].as_array());
    }
}

// =========================================================================
// Hand-built minimal cylinder yang BRep (yr7/yr13 seam-edge encoding) —
// fixture for the curved-output rejection test only.
// =========================================================================

fn vsub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn vadd(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn vscale(a: [f64; 3], s: f64) -> [f64; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn vcross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn vdot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn vunit(a: [f64; 3]) -> [f64; 3] {
    let l = vdot(a, a).sqrt();
    vscale(a, 1.0 / l)
}

fn yang_cylinder_brep(
    axis_point: [f64; 3],
    axis_dir: [f64; 3],
    radius: f64,
    height: f64,
) -> yang_rs::BRep {
    use yang_rs::{BRepEdge, BRepFace, BRepVertex, Curve, Surface};
    let axis_unit = vunit(axis_dir);
    let bottom_center = axis_point;
    let top_center = vadd(axis_point, vscale(axis_unit, height));
    let abs = [axis_unit[0].abs(), axis_unit[1].abs(), axis_unit[2].abs()];
    let world = if abs[0] <= abs[1] && abs[0] <= abs[2] {
        [1.0, 0.0, 0.0]
    } else if abs[1] <= abs[2] {
        [0.0, 1.0, 0.0]
    } else {
        [0.0, 0.0, 1.0]
    };
    let e1 = vunit(vcross(axis_unit, world));
    let v0 = vadd(bottom_center, vscale(e1, radius));
    let v1 = vadd(top_center, vscale(e1, radius));
    let _ = vsub(v1, v0);
    let verts = vec![
        BRepVertex {
            point: p3(v0[0], v0[1], v0[2]),
        },
        BRepVertex {
            point: p3(v1[0], v1[1], v1[2]),
        },
    ];
    let neg_axis = vscale(axis_unit, -1.0);
    let edges = vec![
        BRepEdge {
            start: 0,
            end: 0,
            curve: Curve::Circle {
                center: p3(bottom_center[0], bottom_center[1], bottom_center[2]),
                normal: Vector3::new(neg_axis[0], neg_axis[1], neg_axis[2]),
                radius,
            },
        },
        BRepEdge {
            start: 1,
            end: 1,
            curve: Curve::Circle {
                center: p3(top_center[0], top_center[1], top_center[2]),
                normal: Vector3::new(axis_unit[0], axis_unit[1], axis_unit[2]),
                radius,
            },
        },
        BRepEdge {
            start: 0,
            end: 1,
            curve: Curve::LineSegment,
        },
    ];
    let bottom_d = -vdot(neg_axis, bottom_center);
    let top_d = -vdot(axis_unit, top_center);
    let faces = vec![
        BRepFace {
            surface: Surface::Cylinder {
                axis_point: p3(axis_point[0], axis_point[1], axis_point[2]),
                axis_dir: Vector3::new(axis_dir[0], axis_dir[1], axis_dir[2]),
                radius,
            },
            outer_loop: vec![0, 2, 1, 2],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(neg_axis[0], neg_axis[1], neg_axis[2]),
                d: bottom_d,
            },
            outer_loop: vec![0],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(axis_unit[0], axis_unit[1], axis_unit[2]),
                d: top_d,
            },
            outer_loop: vec![1],
            inner_loops: Vec::new(),
            reversed: false,
        },
    ];
    yang_rs::BRep::new(verts, edges, faces).expect("cylinder yang BRep")
}
