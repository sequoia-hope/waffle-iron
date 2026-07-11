//! M8 plane-grouped n-ary coplanar overlay — RED oracles (task #129, spec
//! `specs/m8_plane_group_nary_overlay.md`).
//!
//! Driver: user case `error_coplanar.waffle` (2026-07-11) — a bridge slab
//! whose BOTTOM face is flush with BOTH tower tops of a U-shaped solid. The
//! bridge bottom lands in TWO Stage-0 coplanar cross pairs, which the
//! pre-slice `stage0_preprocess` walls (`multi-pair` residue). The slice
//! groups coplanar pairs into plane groups (connected components over shared
//! faces) and runs ONE n-ary exact overlay per group, so a face may be
//! segmented against the union of several disjoint partner faces.
//!
//! Corpus twin: assay case C0101 (user-exact geometry), pinned
//! `SupportedCorrect` in `assay_kv2::smoke_corpus_boundary_categories`.

use cad_primitives::{BoolOp, Point3, Vector3};
use std::collections::BTreeMap;
use yang_rs::{boolean, BRep, BRepEdge, BRepFace, BRepVertex, Curve, Mesh, Surface};

// ════════════════════════════════════════════════════════════════════
// fixtures (the yr24/yr26 hexahedron convention)
// ════════════════════════════════════════════════════════════════════

/// Axis-aligned box B-Rep [lo, hi] (8 verts / 24 edges / 6 quad faces,
/// outward plane normals).
fn box_brep(lo: [f64; 3], hi: [f64; 3]) -> BRep {
    let v = |x: f64, y: f64, z: f64| BRepVertex {
        point: Point3::new(x, y, z),
    };
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
        (3, 0), // f0 bottom (−z)
        (4, 5),
        (5, 6),
        (6, 7),
        (7, 4), // f1 top (+z)
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
        .map(|&(s, e)| BRepEdge {
            start: s,
            end: e,
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
    BRep::new(vertices, edges, faces).expect("valid box B-Rep")
}

// ════════════════════════════════════════════════════════════════════
// mesh metrics (independent oracle helpers, yr26 pattern)
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
    let tol = expect.abs().max(1e-12) * 1e-9;
    assert!(
        (got - expect).abs() <= tol,
        "{what}: {got} != expected {expect} (tol {tol})"
    );
}

fn run(a: &BRep, b: &BRep, op: BoolOp, what: &str) -> BRep {
    let nb = yang_rs::native_backend().expect("native backend always available");
    match boolean(a, b, op, &nb) {
        Ok(out) => out,
        Err(e) => panic!("{what}: boolean() failed: {e}"),
    }
}

/// Closed-solid oracle with a caller-chosen Euler characteristic (2 for a
/// ball, 0 for the genus-1 bridge frame).
fn assert_closed(out: &BRep, chi: i64, vol: f64, what: &str) {
    let mesh = out.as_mesh();
    assert_watertight(mesh, what);
    assert_eq!(euler_characteristic(mesh), chi, "{what}: χ must be {chi}");
    let v = signed_volume(mesh);
    assert!(v > 0.0, "{what}: outward orientation (positive volume)");
    assert_rel_eq(v, vol, what);
}

// ════════════════════════════════════════════════════════════════════
// fixtures: bridge configurations
// ════════════════════════════════════════════════════════════════════

/// Round-number U-solid with the towers strictly INSET from every base side
/// plane (no incidental corner-flush side pairs — the tower unions are the
/// already-supported 1×1 interior-overlap class):
/// base [−1.5,1.5]×[−0.5,0.5]×[0,0.2]; towers 0.8×0.8×1.0 on the base top.
fn u_solid_inset() -> BRep {
    let base = box_brep([-1.5, -0.5, 0.0], [1.5, 0.5, 0.2]);
    let tower_a = box_brep([-1.2, -0.4, 0.2], [-0.4, 0.4, 1.2]);
    let tower_b = box_brep([0.4, -0.4, 0.2], [1.2, 0.4, 1.2]);
    let u1 = run(&base, &tower_a, BoolOp::Union, "U: base ∪ tower A");
    run(&u1, &tower_b, BoolOp::Union, "U: ∪ tower B")
}

/// Narrow bridge: bottom face strictly INSIDE both tower tops (the minimal
/// pure two-pair plane group; the overlap boundaries are interior on both
/// faces). Spans the gap → the union is a genus-1 frame.
fn bridge_narrow() -> BRep {
    box_brep([-1.0, -0.3, 1.2], [1.0, 0.3, 1.4])
}

const VOL_U_INSET: f64 = 0.6 + 2.0 * (0.8 * 0.8 * 1.0);
const VOL_BRIDGE_NARROW: f64 = 2.0 * 0.6 * 0.2;

/// The user's EXACT world geometry (mm scale; `error_coplanar.waffle`, task
/// #129): 24.2×11.2 base slab below z=0, two unequal full-width towers up
/// from base-top regions at the slab's ends, 1 mm bridge spanning the full
/// footprint flush on both tower tops (all side planes corner-flush).
const X_LO: f64 = -0.012077922088792548;
const X_HI: f64 = 0.012077922088792548;
const Y_HALF: f64 = 0.005603895318927243;
const TA_LO: f64 = 0.003762989799724892; // tower A (+x end) inner wall
const TB_HI: f64 = -0.005730517499614507; // tower B (−x end) inner wall

fn u_solid_user() -> BRep {
    let base = box_brep([X_LO, -Y_HALF, -0.002], [X_HI, Y_HALF, 0.0]);
    let tower_a = box_brep([TA_LO, -Y_HALF, 0.0], [X_HI, Y_HALF, 0.01]);
    let tower_b = box_brep([X_LO, -Y_HALF, 0.0], [TB_HI, Y_HALF, 0.01]);
    let u1 = run(&base, &tower_a, BoolOp::Union, "user U: base ∪ tower A");
    run(&u1, &tower_b, BoolOp::Union, "user U: ∪ tower B")
}

fn bridge_user() -> BRep {
    box_brep([X_LO, -Y_HALF, 0.01], [X_HI, Y_HALF, 0.011])
}

fn vol_user_frame() -> f64 {
    let w = X_HI - X_LO;
    let h = 2.0 * Y_HALF;
    let base = w * h * 0.002;
    let tower_a = (X_HI - TA_LO) * h * 0.01;
    let tower_b = (TB_HI - X_LO) * h * 0.01;
    let bridge = w * h * 0.001;
    base + tower_a + tower_b + bridge
}

// ════════════════════════════════════════════════════════════════════
// Oracle #1 — canonical: narrow bridge union (pure two-pair group).
// ════════════════════════════════════════════════════════════════════
#[test]
fn narrow_bridge_union_is_genus1_frame() {
    let u = u_solid_inset();
    let out = run(&u, &bridge_narrow(), BoolOp::Union, "narrow bridge union");
    assert_closed(
        &out,
        0,
        VOL_U_INSET + VOL_BRIDGE_NARROW,
        "narrow bridge union",
    );
}

// ════════════════════════════════════════════════════════════════════
// Oracle #2 — branch Subtract: the flush tool removes nothing; the
// membranes are kept (opposite normals) and the result is U exactly.
// ════════════════════════════════════════════════════════════════════
#[test]
fn narrow_bridge_subtract_leaves_u_exactly() {
    let u = u_solid_inset();
    let out = run(
        &u,
        &bridge_narrow(),
        BoolOp::Subtract,
        "narrow bridge subtract",
    );
    assert_closed(&out, 2, VOL_U_INSET, "narrow bridge subtract");
}

// ════════════════════════════════════════════════════════════════════
// Oracle #3 — branch Intersect: zero-volume contact → EMPTY result
// (solid semantics drop the shared membranes).
// ════════════════════════════════════════════════════════════════════
#[test]
fn narrow_bridge_intersect_is_empty() {
    let u = u_solid_inset();
    let out = run(
        &u,
        &bridge_narrow(),
        BoolOp::Intersect,
        "narrow bridge intersect",
    );
    assert_eq!(
        out.num_tris(),
        0,
        "narrow bridge intersect: U∩bridge has zero volume — EMPTY"
    );
}

// ════════════════════════════════════════════════════════════════════
// Oracle #4 — the user's exact geometry end-to-end (6 cross pairs: the
// two-pair bottom group + four zero-overlap corner-flush side pairs).
// ════════════════════════════════════════════════════════════════════
#[test]
fn user_bridge_union_is_genus1_frame() {
    let u = u_solid_user();
    let out = run(&u, &bridge_user(), BoolOp::Union, "user bridge union");
    assert_closed(&out, 0, vol_user_frame(), "user bridge union");
}
