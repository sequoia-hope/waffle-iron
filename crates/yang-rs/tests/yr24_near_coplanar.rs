//! PR-YR24 — Stage-1 NEAR-coplanar input gate (KV4-F1).
//!
//! The KV4-F1 corpus family (R0029, F0016/18/19/21/25) unions two boxes
//! extruded from the SAME oblique sketch plane. f64 vertex construction
//! leaves femto-scale residuals (the recovered R0029 plane offsets differ by
//! ~1.4e-13 on a |coord| ~ 6e2 model), so the faces are NEAR-coplanar, not
//! bit-exactly coplanar: the cherchi-rs EXACT coplanar deferral (deviation
//! N17) does not fire, the exact arrangement faithfully builds sub-f64-ulp
//! sliver patches, and in/out classification fails with
//! `NoExplicitRayOrigin` (the C++ reference `booleans.cpp:504-575` would
//! exit there too).
//!
//! Per Yang 2025 §4.5.5 coplanar handling is Stage-0 PRE-discretization
//! B-Rep work ("check coplanar planes and perform 2D Boolean operations
//! before mesh discretizations", `refs/text/yang2025_hybrid_boolean.txt:717-731`)
//! — roadmap M8, not yet implemented. Until M8, near-coplanar input must hit
//! the SAME loud typed wall as exact-coplanar input:
//! `YangError::CoplanarFacesUnsupported`.
//!
//! RED geometry provenance: the two B-Reps below are the VERBATIM inputs of
//! corpus case R0029 at the `yang_rs::boolean()` boundary, recovered by
//! replaying R0029 through the test-harness KV2 adapter with a temporary
//! env-gated dump probe at the `boolean()` entry (the probe was removed; the
//! coordinates are committed here verbatim). The near-coplanar pair is
//! A face 1 ↔ B face 1: identical unit normal direction, d = 23.84180252162639
//! vs 23.84180252162625.

use cad_primitives::{BoolOp, Point3, Vector3};
use yang_rs::{boolean, BRep, BRepEdge, BRepFace, BRepVertex, Curve, Surface, YangError};

/// Build a hexahedral (8-vertex, 6-quad-face) B-Rep from verbatim dumped
/// data: vertex coordinates, per-face plane (normal, d with n·x + d = 0),
/// and the shared 24-edge / 6-loop topology the corpus boxes use.
fn hex_brep(verts: [[f64; 3]; 8], planes: [([f64; 3], f64); 6]) -> BRep {
    let vertices: Vec<BRepVertex> = verts
        .iter()
        .map(|&[x, y, z]| BRepVertex {
            point: Point3::new(x, y, z),
        })
        .collect();
    // Edge (start, end) pairs exactly as dumped from the R0029 replay.
    const EDGE_PAIRS: [(u32, u32); 24] = [
        (0, 1),
        (1, 2),
        (2, 3),
        (3, 0),
        (4, 5),
        (5, 6),
        (6, 7),
        (7, 4),
        (2, 1),
        (1, 5),
        (5, 4),
        (4, 2),
        (3, 2),
        (2, 4),
        (4, 7),
        (7, 3),
        (0, 3),
        (3, 7),
        (7, 6),
        (6, 0),
        (1, 0),
        (0, 6),
        (6, 5),
        (5, 1),
    ];
    let edges: Vec<BRepEdge> = EDGE_PAIRS
        .iter()
        .map(|&(start, end)| BRepEdge {
            start,
            end,
            curve: Curve::LineSegment,
        })
        .collect();
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
    BRep::new(vertices, edges, faces).expect("hex BRep::new failed")
}

/// Corpus case R0029, operand A — verbatim from the replay dump.
fn r0029_a() -> BRep {
    hex_brep(
        [
            [-45.99420295584295, 289.97915911310525, 653.826743209303],
            [-191.92189641481409, 17.604191465421977, 653.826743209303],
            [-109.7064915407901, -26.443558236674463, 566.4538007378272],
            [36.22120191818104, 245.93140941100881, 566.4538007378272],
            [-309.7667759530226, 80.74079908204098, 324.1674998932614],
            [-391.9821808270466, 124.78854878413742, 411.54044236473715],
            [-246.05448736807546, 397.1635164318207, 411.54044236473715],
            [-163.83908249405147, 353.11576672972427, 324.1674998932614],
        ],
        [
            (
                [0.6026151226794615, -0.3228572568748562, 0.7298069646154802],
                -355.82863272709255,
            ),
            (
                [-0.6026151226794616, 0.3228572568748561, -0.7298069646154801],
                23.84180252162639,
            ),
            (
                [
                    -0.4722529252184476,
                    -0.881463087498631,
                    -1.0717864349013949e-16,
                ],
                -75.11823203333986,
            ),
            (
                [
                    0.6432979003079651,
                    -0.34465347388445655,
                    -0.6836532705975593,
                ],
                448.71808496704176,
            ),
            (
                [0.4722529252184477, 0.881463087498631, 8.574291479211158e-17],
                -233.8850280131068,
            ),
            (
                [-0.643297900307965, 0.3446534738844566, 0.6836532705975592],
                -576.5210901294479,
            ),
        ],
    )
}

/// Corpus case R0029, operand B — verbatim from the replay dump.
fn r0029_b() -> BRep {
    hex_brep(
        [
            [-117.96495179021835, 297.66236390886377, 559.9320351124034],
            [-238.1870351841908, 73.26708088345916, 559.9320351124034],
            [-175.58546093408052, 39.727642894298526, 493.40333828290136],
            [-55.36337754010813, 264.1229259197031, 493.40333828290136],
            [-306.7208862324801, 109.98479724964821, 334.58962271424826],
            [-369.3224604825904, 143.52423523880884, 401.11831954375026],
            [-249.10037708861793, 367.9195182642135, 401.11831954375026],
            [-186.49880283850771, 334.3800802750528, 334.58962271424826],
        ],
        [
            (
                [0.6026151226794615, -0.32285725687485667, 0.7298069646154799],
                -241.45238075491108,
            ),
            (
                [
                    -0.6026151226794617,
                    0.32285725687485634,
                    -0.7298069646154799,
                ],
                23.84180252162625,
            ),
            (
                [
                    -0.4722529252184474,
                    -0.8814630874986312,
                    -1.717936239323118e-16,
                ],
                -47.90229678729742,
            ),
            (
                [0.6432979003079651, -0.3446534738844565, -0.6836532705975594],
                463.96283441712086,
            ),
            (
                [
                    0.4722529252184481,
                    0.8814630874986309,
                    -8.58968119661559e-17,
                ],
                -206.66909276706423,
            ),
            (
                [-0.643297900307965, 0.34465347388445666, 0.6836532705975593],
                -561.276340679369,
            ),
        ],
    )
}

/// PR-YR26 (M8 slice b): the verbatim R0029 union now SUCCEEDS through the
/// §4.5.5 Stage-0 coplanar overlay. The PR-YR24 gate used to convert this
/// case into the loud `CoplanarFacesUnsupported` M8 wall; M8 slice b is that
/// wall coming down for planar A×B pairs: the near-coplanar pair A#1/B#1
/// (d residual ~1.4e-13) is snapped onto ONE canonical shared plane (the
/// §4.5.5 "trimmed common planar surface" — THE place the femto residual is
/// reconciled), the exact 2D overlay segments it into A-only / B-only /
/// Overlap regions, and the Overlap is tessellated IDENTICALLY for both
/// solids, so the femto sliver patches that used to defeat in/out
/// classification (`NoExplicitRayOrigin`) never exist.
///
/// Union invariants asserted (the exact volume is not hand-derivable for
/// this oblique corpus geometry, so the oracle pins the solid-semantics
/// bounds + mesh validity instead): watertight, outward-oriented, χ = 2,
/// and max(vol A, vol B) ≤ vol(A∪B) ≤ vol A + vol B.
#[test]
fn r0029_near_coplanar_union_succeeds_via_stage0_overlay() {
    let Some(sb) = yang_rs::native_backend() else {
        eprintln!("[yr24] SKIP: native FFI shim not linked (stub build)");
        return;
    };
    let a = r0029_a();
    let b = r0029_b();
    let out = match boolean(&a, &b, BoolOp::Union, &sb) {
        Ok(out) => out,
        Err(e) => panic!(
            "R0029 union: M8 slice b must HANDLE the planar near-coplanar \
             pair via the §4.5.5 overlay, got Err({e})"
        ),
    };

    let signed_volume = |mesh: &yang_rs::Mesh| -> f64 {
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
    };

    // Watertight 2-manifold + χ = 2 on position-welded elements.
    use std::collections::{BTreeMap, BTreeSet};
    let mesh = out.as_mesh();
    let key = |v: u32| {
        let p = mesh.verts[v as usize];
        [p.x().to_bits(), p.y().to_bits(), p.z().to_bits()]
    };
    let mut edges: BTreeMap<([u64; 3], [u64; 3]), (usize, i64)> = BTreeMap::new();
    let mut verts: BTreeSet<[u64; 3]> = BTreeSet::new();
    for t in &mesh.tris {
        for k in 0..3 {
            verts.insert(key(t[k]));
            let (p, q) = (key(t[k]), key(t[(k + 1) % 3]));
            let (lo, hi, dir) = if p <= q { (p, q, 1) } else { (q, p, -1) };
            let e = edges.entry((lo, hi)).or_insert((0, 0));
            e.0 += 1;
            e.1 += dir;
        }
    }
    for (e, (count, balance)) in &edges {
        assert_eq!(*count, 2, "R0029 union: edge {e:?} must have 2 tris");
        assert_eq!(*balance, 0, "R0029 union: edge {e:?} direction-balanced");
    }
    let euler = verts.len() as i64 - edges.len() as i64 + mesh.tris.len() as i64;
    assert_eq!(euler, 2, "R0029 union result is sphere-like (χ = 2)");

    let (va, vb) = (signed_volume(a.as_mesh()), signed_volume(b.as_mesh()));
    let vu = signed_volume(mesh);
    assert!(vu > 0.0, "R0029 union: outward orientation, got {vu}");
    let tol = (va + vb) * 1e-9;
    assert!(
        vu >= va.max(vb) - tol && vu <= va + vb + tol,
        "R0029 union volume {vu} outside solid bounds [{}, {}]",
        va.max(vb),
        va + vb
    );
}

/// Negative control: the SAME oblique geometry with B translated 5.0 along
/// the shared plane normal is genuinely non-coplanar (offset gap ≫ band) and
/// must NOT trip the gate.
#[test]
fn translated_oblique_pair_does_not_trip_gate() {
    let Some(sb) = yang_rs::native_backend() else {
        eprintln!("[yr24] SKIP: native FFI shim not linked (stub build)");
        return;
    };
    let a = r0029_a();
    let b0 = r0029_b();
    // Translate B by 5.0 along face 1's (unit) normal. Plane offsets shift by
    // n·t: for a plane n·x + d = 0 translated by t, d' = d − n·t.
    let n = [
        -0.6026151226794617,
        0.32285725687485634,
        -0.7298069646154799,
    ];
    let t = [5.0 * n[0], 5.0 * n[1], 5.0 * n[2]];
    let vertices: Vec<BRepVertex> = b0
        .vertices()
        .iter()
        .map(|v| BRepVertex {
            point: Point3::new(v.point.x() + t[0], v.point.y() + t[1], v.point.z() + t[2]),
        })
        .collect();
    let faces: Vec<BRepFace> = b0
        .faces()
        .iter()
        .map(|f| {
            let Surface::Plane { normal, d } = f.surface else {
                panic!("non-plane face in box");
            };
            let na = normal.as_array();
            BRepFace {
                surface: Surface::Plane {
                    normal,
                    d: d - (na[0] * t[0] + na[1] * t[1] + na[2] * t[2]),
                },
                outer_loop: f.outer_loop.clone(),
                inner_loops: f.inner_loops.clone(),
                reversed: f.reversed,
            }
        })
        .collect();
    let b = BRep::new(vertices, b0.edges().to_vec(), faces).expect("translated BRep::new failed");

    let r = boolean(&a, &b, BoolOp::Union, &sb);
    assert!(
        !matches!(r, Err(YangError::CoplanarFacesUnsupported { .. })),
        "genuinely non-coplanar oblique pair must NOT trip the near-coplanar \
         gate, got {r:?}"
    );
}
