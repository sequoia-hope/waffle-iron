//! PR-YR27 RED (M8 slice c) — Stage-6 face resolution: finite-extent
//! membership + same-plane output-face merging + Stage-0 pair-band keyed
//! membership.
//!
//! Three probe-verified findings from the independent YR26 review:
//!
//! **Finding 1 — same-plane output faces break chained booleans (assay
//! F0066).** A coplanar union emits the two same-plane fragments (e.g. A's
//! and B's x=0 side faces of exactly stacked boxes) as SEPARATE output
//! faces on bit-identical planes. The NEXT boolean in a chain then hits the
//! Stage-6 exact tie: a kept triangle's centroid lies within `TAU_WORK` of
//! BOTH faces' (identical) planes → `n_exact == 2` →
//! `FaceResolutionFailed` with the misleading "coplanar multi-solid label"
//! text. Fix: (a) at Stage-6 emission, merge edge-adjacent same-plane
//! same-orientation output faces into ONE face (non-adjacent same-plane
//! faces stay separate); (b) the residual tie class is broken by
//! finite-extent containment (Finding 3).
//!
//! RED error text (verbatim, asserted by reproduction):
//!   "yang-rs: geometric face resolution failed for kept triangle 10
//!    (coplanar multi-solid label, or centroid off all face planes / tie)"
//!
//! **Finding 2 — Stage-0 band vs Stage-6 membership mismatch.** Stage 0
//! accepts a near-coplanar pair with plane residual up to
//! `max(TAU_MODEL, scale·TAU_WORK)` and SNAPS the pair faces onto the
//! canonical (face A) plane — but Stage-6 membership still measures the
//! snapped triangles against B's STORED face plane with `TAU_WORK`, so a
//! B-only triangle with residual in (1e-12, 1e-7] fails membership →
//! `FaceResolutionFailed`. Fix: for exactly the faces that went through a
//! Stage-0 pair, membership is measured against the CANONICAL pair plane
//! (keyed to the pair — NEVER a global tolerance change).
//!
//! **Finding 3 — exact tie on INFINITE planes.** Membership resolves a
//! kept triangle by centroid distance to each face's INFINITE plane; an
//! L-profile cap triangle's centroid can lie EXACTLY on a side face's
//! plane (e.g. cap triangle (0,0),(2,0),(1,1) → centroid x = 1, the x=1
//! side plane) with no coplanarity anywhere → `n_exact == 2` tie → loud
//! error. Fix: break the tie by FINITE-EXTENT point-in-face containment
//! (exact 2D point-in-polygon in the face's plane frame, strict — the true
//! owning face strictly contains the centroid; the infinite-plane false
//! positive at best touches its boundary). Genuinely unresolvable
//! triangles keep the loud error.
//!
//! Fixed coordinates only: no rand, no time, no filesystem.

use cad_primitives::{BoolOp, Point3, Vector3};
use std::collections::BTreeMap;
use yang_rs::{boolean, BRep, BRepEdge, BRepFace, BRepVertex, Curve, Mesh, Surface, YangError};

// ════════════════════════════════════════════════════════════════════
// fixtures
// ════════════════════════════════════════════════════════════════════

/// Axis-aligned box B-Rep [lo, hi] (the yr24/yr26 hexahedron topology).
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

/// Prism B-Rep over a CCW (from +z) simple XY profile, extruded z0 → z1.
/// Same construction pattern as the nc1 `u_prism` fixture: bottom cap
/// (−z, profile walked in reverse), top cap (+z), one quad wall per
/// profile edge with the outward 2D edge normal.
fn prism_brep(profile: &[[f64; 2]], z0: f64, z1: f64) -> BRep {
    let n = profile.len() as u32;
    let mut verts: Vec<BRepVertex> = Vec::with_capacity(2 * n as usize);
    for &[x, y] in profile {
        verts.push(BRepVertex {
            point: Point3::new(x, y, z0),
        });
    }
    for &[x, y] in profile {
        verts.push(BRepVertex {
            point: Point3::new(x, y, z1),
        });
    }
    let line = |s: u32, e: u32| BRepEdge {
        start: s,
        end: e,
        curve: Curve::LineSegment,
    };
    let mut edges: Vec<BRepEdge> = Vec::new();
    let mut faces: Vec<BRepFace> = Vec::new();

    // Bottom cap (z=z0), outward −z: profile in reverse order.
    let base = edges.len() as u32;
    for i in 0..n {
        let s = (n - i) % n;
        let e = (n - i - 1) % n;
        edges.push(line(s, e));
    }
    faces.push(BRepFace {
        surface: Surface::Plane {
            normal: Vector3::new(0.0, 0.0, -1.0),
            d: z0,
        },
        outer_loop: (base..base + n).collect(),
        inner_loops: Vec::new(),
        reversed: false,
    });

    // Top cap (z=z1), outward +z: profile order.
    let base = edges.len() as u32;
    for i in 0..n {
        edges.push(line(n + i, n + (i + 1) % n));
    }
    faces.push(BRepFace {
        surface: Surface::Plane {
            normal: Vector3::new(0.0, 0.0, 1.0),
            d: -z1,
        },
        outer_loop: (base..base + n).collect(),
        inner_loops: Vec::new(),
        reversed: false,
    });

    // Walls.
    for i in 0..n {
        let bi = i;
        let bj = (i + 1) % n;
        let ti = n + i;
        let tj = n + (i + 1) % n;
        let base = edges.len() as u32;
        edges.push(line(bi, bj));
        edges.push(line(bj, tj));
        edges.push(line(tj, ti));
        edges.push(line(ti, bi));
        let a = profile[i as usize];
        let b = profile[((i + 1) % n) as usize];
        let dx = b[0] - a[0];
        let dy = b[1] - a[1];
        let len = (dx * dx + dy * dy).sqrt();
        let nx = dy / len;
        let ny = -dx / len;
        let d = -(nx * a[0] + ny * a[1]);
        faces.push(BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(nx, ny, 0.0),
                d,
            },
            outer_loop: vec![base, base + 1, base + 2, base + 3],
            inner_loops: Vec::new(),
            reversed: false,
        });
    }

    BRep::new(verts, edges, faces).expect("prism BRep::new")
}

/// The reviewer's Finding-3 geometry: L profile
/// (0,0),(2,0),(2,1),(1,1),(1,2),(0,2) (CCW, area 3), extruded z ∈ [0,1].
/// Its cap CDT emits a triangle whose centroid lies EXACTLY on the x = 1
/// side-face plane (e.g. (0,0),(2,0),(1,1) → centroid x = (0+2+1)/3 = 1).
fn l_prism() -> BRep {
    prism_brep(
        &[
            [0.0, 0.0],
            [2.0, 0.0],
            [2.0, 1.0],
            [1.0, 1.0],
            [1.0, 2.0],
            [0.0, 2.0],
        ],
        0.0,
        1.0,
    )
}

// ════════════════════════════════════════════════════════════════════
// mesh metrics (yr26 conventions — independent oracle helpers)
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

// ════════════════════════════════════════════════════════════════════
// Finding 1 — chained coplanar unions (the assay F0066 pattern).
//
// RED: u1 (exactly stacked union) succeeds but emits each shared side
// plane as TWO bit-identical-plane faces (10 faces instead of 6), and the
// NEXT union over u1 fails loudly with
//   YangError::FaceResolutionFailed (Display: "... (coplanar multi-solid
//   label, or centroid off all face planes / tie)")
// because a kept side triangle's centroid exact-ties BOTH fragments.
// ════════════════════════════════════════════════════════════════════

#[test]
fn stacked_union_merges_same_plane_adjacent_output_faces() {
    // A = [0,2]²×[0,2] stacked under B = [0,2]²×[2,4]: union is the
    // [0,2]²×[0,4] box. Each of the 4 side planes carries an A-fragment
    // and a B-fragment on the BIT-IDENTICAL plane, edge-adjacent along the
    // z = 2 seam — Stage 6 must emit them as ONE face each (Finding 1a).
    // Non-adjacent same-plane faces (none here) stay separate.
    let a = box_brep([0.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
    let b = box_brep([0.0, 0.0, 2.0], [2.0, 2.0, 4.0]);
    let u1 = run(&a, &b, BoolOp::Union, "stacked union");
    assert_solid(&u1, 16.0, "stacked union");
    assert_eq!(
        u1.faces().len(),
        6,
        "stacked union must merge the per-input same-plane adjacent side \
         fragments into one face per plane (got faces: {:?})",
        u1.faces().iter().map(|f| f.surface).collect::<Vec<_>>()
    );
}

#[test]
fn chained_union_over_coplanar_union_output_succeeds() {
    // F0066 pattern, reduced: u1 = exactly stacked union; u2 = u1 ∪ C with
    // C = [0.5,1.5]²×[3.5,4.5] protruding through u1's top. RED: Stage 6
    // exact-ties a kept u1 side triangle between the two bit-identical
    // side-plane fragment faces → FaceResolutionFailed.
    let a = box_brep([0.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
    let b = box_brep([0.0, 0.0, 2.0], [2.0, 2.0, 4.0]);
    let u1 = run(&a, &b, BoolOp::Union, "stacked union (chain step 1)");
    let c = box_brep([0.5, 0.5, 3.5], [1.5, 1.5, 4.5]);
    let u2 = run(&u1, &c, BoolOp::Union, "chained union (chain step 2)");
    // 16 + the 1×1×0.5 protrusion above z = 4.
    assert_solid(&u2, 16.5, "chained union");
}

// ════════════════════════════════════════════════════════════════════
// Finding 2 — Stage-0 pair band vs Stage-6 membership mismatch.
//
// RED: B's bottom face is near-coplanar with A's top face (residual r in
// (TAU_WORK, TAU_MODEL]); Stage 0 ACCEPTS the pair and snaps B's bottom
// onto A's canonical z = 1 plane, but Stage 6 still measures the snapped
// B-only triangles against B's STORED plane (z = 1 + r) with TAU_WORK →
//   YangError::FaceResolutionFailed (Display: "... (coplanar multi-solid
//   label, or centroid off all face planes / tie)")
// Fix: membership for exactly the Stage-0 pair faces is measured against
// the CANONICAL pair plane (keyed — no global tolerance change).
// ════════════════════════════════════════════════════════════════════

/// One near-partial-overlap stacked fixture: A = [0,2]²×[0,1],
/// B = [1,3]×[0,2]×[1+r, 2+r]. The shared plane carries an A-only region
/// ([0,1]×[0,2]), an Overlap region ([1,2]×[0,2]) and a B-only region
/// ([2,3]×[0,2] — the triangles that currently fail membership).
/// After the Stage-0 snap B's bottom sits exactly at z = 1, so
/// union volume = 4 + 4·(1+r) and subtract A−B = A = 4 exactly.
fn near_partial_case(r: f64, what: &str) {
    let a = box_brep([0.0, 0.0, 0.0], [2.0, 2.0, 1.0]);
    let b = box_brep([1.0, 0.0, 1.0 + r], [3.0, 2.0, 2.0 + r]);

    let out = run(&a, &b, BoolOp::Union, what);
    assert_solid(&out, 4.0 + 4.0 * (1.0 + r), &format!("{what}: union"));

    let out = run(&a, &b, BoolOp::Subtract, what);
    assert_solid(&out, 4.0, &format!("{what}: subtract"));
}

#[test]
fn near_partial_overlap_residual_1e10_resolves_membership() {
    near_partial_case(1e-10, "near-partial r=1e-10");
}

#[test]
fn near_partial_overlap_residual_1e8_resolves_membership() {
    near_partial_case(1e-8, "near-partial r=1e-8");
}

// ════════════════════════════════════════════════════════════════════
// Finding 3 — exact tie on INFINITE planes (no coplanarity anywhere).
//
// RED: the L-prism cap CDT emits a triangle whose centroid lies EXACTLY
// on the x = 1 side-face plane (e.g. cap triangle (0,0),(2,0),(1,1) →
// centroid (1, 1/3)) → n_exact == 2 tie between the cap plane and the
// x = 1 side plane →
//   YangError::FaceResolutionFailed (Display: "... (coplanar multi-solid
//   label, or centroid off all face planes / tie)")
// Fix: finite-extent strict containment — (1, 1/3) is strictly inside the
// L cap but OUTSIDE the x = 1 side face's region (y ∈ [1,2]).
// ════════════════════════════════════════════════════════════════════

#[test]
fn l_profile_union_resolves_infinite_plane_exact_tie() {
    let l = l_prism();
    // C protrudes through the L's bottom; no plane of C coincides with any
    // plane of L (no Stage-0 pair anywhere — the tie is pre-existing and
    // independent of coplanarity).
    let c = box_brep([0.25, 0.25, -0.5], [0.75, 0.75, 0.5]);
    let out = run(&l, &c, BoolOp::Union, "L ∪ box");
    // L volume 3·1 plus the 0.5×0.5×0.5 protrusion below z = 0.
    assert_solid(&out, 3.0 + 0.125, "L ∪ box");
}

/// Confirms the Finding-3 trigger really is the documented exact tie (not
/// some other failure): the L-prism's own cap triangulation must contain a
/// cap triangle whose centroid lies bit-exactly on a side-face plane.
#[test]
fn l_profile_cap_cdt_emits_on_side_plane_centroid() {
    let l = l_prism();
    let mesh = l.as_mesh();
    let mut found = false;
    for t in &mesh.tris {
        let p = [
            mesh.verts[t[0] as usize],
            mesh.verts[t[1] as usize],
            mesh.verts[t[2] as usize],
        ];
        // Cap triangle (all three verts on z = 0 or all on z = 1)…
        let cap = p.iter().all(|q| q.z() == 0.0) || p.iter().all(|q| q.z() == 1.0);
        if !cap {
            continue;
        }
        // …whose centroid lies exactly on the x = 1 or y = 1 side plane.
        let cx = (p[0].x() + p[1].x() + p[2].x()) / 3.0;
        let cy = (p[0].y() + p[1].y() + p[2].y()) / 3.0;
        if cx == 1.0 || cy == 1.0 {
            found = true;
            break;
        }
    }
    assert!(
        found,
        "fixture invariant: the L cap CDT must emit a triangle whose \
         centroid lies exactly on a side plane (else this fixture no longer \
         reproduces Finding 3 — adjust the profile)"
    );
}

// ════════════════════════════════════════════════════════════════════
// Loud-error contract: a genuinely unresolvable triangle (no input face
// within tolerance at all) must STILL fail loudly after the fixes — the
// finite-extent containment breaks ties, it never widens membership.
// (Pinned via the public error type: FaceResolutionFailed survives.)
// ════════════════════════════════════════════════════════════════════

#[test]
fn face_resolution_error_type_still_exists_and_is_loud() {
    // Type-level pin (compile-time): the loud error variant survives the
    // fix — P9 contract that unresolvable membership is never silent.
    let e = YangError::FaceResolutionFailed { tri: 0 };
    let text = format!("{e}");
    assert!(
        text.contains("face resolution failed"),
        "loud error text: {text}"
    );
}
