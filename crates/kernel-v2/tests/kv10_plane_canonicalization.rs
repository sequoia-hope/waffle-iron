//! PR-KV10 — same-plane sibling plane-bit canonicalization (M8 slice d).
//!
//! A boolean output legitimately carries several faces that are disjoint
//! fragments of ONE plane (e.g. a box side plane split in two by a crossing
//! union). On oblique geometry those fragments' yang-facing `(normal, d)`
//! bits historically differed at the ~1e-16 level — each face's normal was
//! a per-fragment Newell recomputation and `to_yang_brep` re-derived `d`
//! from each face's own first loop vertex. `scan_near_coplanar`'s benign
//! intra-solid exclusion is BIT-identity, so the femto-distinct siblings
//! walled the NEXT boolean as `UnsupportedCoplanar` even when the incoming
//! solid shares no plane at all (the F0016-class corpus residue — the
//! dominant intra-solid sub-class of the M8 coplanar wall, 20/54 cases).
//!
//! `to_yang_brep` now canonicalizes: planar faces whose unit normals agree
//! component-wise within `TAU_WORK` and whose offsets agree within a
//! scale-relative `TAU_WORK·(1+|d|)` band emit ONE representative's exact
//! bits. Genuinely distinct parallel planes are ≥ MIN_FEATURE_SIZE apart —
//! six orders of magnitude beyond the band — so only rounding-noise
//! siblings collapse.

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::{
    boolean_op, extrude, tessellate, to_yang_brep, validate_solid, BrepArena, Profile,
};

/// An oblique orthonormal frame (irrational direction cosines — the noise
/// trigger; axis-aligned geometry cancels exactly in the Newell sums and
/// never reproduced the bug).
fn oblique_frame() -> ([f64; 3], [f64; 3], [f64; 3]) {
    fn norm(a: [f64; 3]) -> [f64; 3] {
        let l = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
        [a[0] / l, a[1] / l, a[2] / l]
    }
    let u = norm([1.0, 2.0, 3.0]);
    let w = [0.3, -0.4, 0.5];
    // v = normalize(w − (w·u)u), n = u × v.
    let d = w[0] * u[0] + w[1] * u[1] + w[2] * u[2];
    let v = norm([w[0] - d * u[0], w[1] - d * u[1], w[2] - d * u[2]]);
    let n = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    (u, v, n)
}

/// Extrude the rectangle `[x0,x1]×[y0,y1]` (frame in-plane coords) from
/// frame height `z0` to `z1` along the oblique frame normal.
fn oblique_box(
    a: &mut BrepArena,
    x: (f64, f64),
    y: (f64, f64),
    z: (f64, f64),
) -> kernel_v2::SolidId {
    let (u, v, n) = oblique_frame();
    let origin = Point3::new(0.1 + z.0 * n[0], 0.2 + z.0 * n[1], 0.3 + z.0 * n[2]);
    let p = Profile::new(
        origin,
        Vector3::new(u[0], u[1], u[2]),
        Vector3::new(v[0], v[1], v[2]),
        vec![
            Point2::new(x.0, y.0),
            Point2::new(x.1, y.0),
            Point2::new(x.1, y.1),
            Point2::new(x.0, y.1),
        ],
        vec![],
    )
    .unwrap();
    extrude(a, &p, Vector3::new(n[0], n[1], n[2]), z.1 - z.0)
        .unwrap()
        .solid
}

fn mesh_signed_volume(mesh: &kernel_v2::RenderMesh) -> f64 {
    let mut six_v = 0.0f64;
    for t in mesh.indices.chunks_exact(3) {
        let p = |i: u32| {
            let k = (i as usize) * 3;
            [
                mesh.positions[k],
                mesh.positions[k + 1],
                mesh.positions[k + 2],
            ]
        };
        let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
        six_v += a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
            + a[2] * (b[0] * c[1] - b[1] * c[0]);
    }
    six_v / 6.0
}

/// Build the F0016-class chain: union splits box1's top plane into two
/// disjoint fragments; the third solid shares NO plane with the union but
/// overlaps a fragment's region.
fn split_fragment_union(a: &mut BrepArena) -> kernel_v2::SolidId {
    let b1 = oblique_box(a, (0.0, 4.0), (0.0, 4.0), (0.0, 1.0));
    let b2 = oblique_box(a, (1.0, 3.0), (-1.0, 5.0), (-1.0, 2.0));
    boolean_op(a, b1, b2, BoolOp::Union).expect("first union (no shared planes)")
}

/// The union output's yang-facing B-Rep must emit BIT-identical planes for
/// same-plane sibling fragments (the canonicalization invariant itself).
#[test]
fn sibling_fragments_emit_bit_identical_planes() {
    let mut a = BrepArena::new();
    let u1 = split_fragment_union(&mut a);
    let y = to_yang_brep(&a, u1).expect("yang conversion");

    // Group planar faces by quantized (n, d) at 1e-9 granularity; every
    // group must collapse to exactly ONE exact-bits plane.
    let mut groups: std::collections::BTreeMap<[i64; 4], Vec<[u64; 4]>> =
        std::collections::BTreeMap::new();
    for f in y.faces() {
        if let yang_rs::Surface::Plane { normal, d } = f.surface {
            let n = normal.as_array();
            let q = |x: f64| (x * 1e9).round() as i64;
            groups
                .entry([q(n[0]), q(n[1]), q(n[2]), q(d)])
                .or_default()
                .push([n[0].to_bits(), n[1].to_bits(), n[2].to_bits(), d.to_bits()]);
        }
    }
    let mut sibling_groups = 0usize;
    for (key, bits) in &groups {
        if bits.len() > 1 {
            sibling_groups += 1;
            for b in &bits[1..] {
                assert_eq!(
                    b, &bits[0],
                    "same-plane sibling faces emit distinct plane bits (group {key:?})"
                );
            }
        }
    }
    // The construction MUST have produced sibling fragments (top z=1 plane
    // split by the crossing box at minimum) — otherwise this test is vacuous.
    assert!(
        sibling_groups >= 1,
        "fixture defect: union produced no same-plane sibling faces"
    );
}

/// The chained boolean over the fragment-carrying union must run (this was
/// the loud `UnsupportedCoplanar` intra-solid wall) and produce the exact
/// expected volume.
#[test]
fn chained_boolean_over_split_fragments_succeeds() {
    let mut a = BrepArena::new();
    let u1 = split_fragment_union(&mut a);
    // No plane of b3 coincides with any plane of u1; b3 overlaps the
    // x∈[0,1] top-plane fragment's region.
    let b3 = oblique_box(&mut a, (0.2, 0.8), (1.1, 2.3), (0.3, 2.7));
    let out = boolean_op(&mut a, u1, b3, BoolOp::Union)
        .expect("chained union over split same-plane fragments");
    validate_solid(&a, out).expect("valid output");

    // vol = 16 + 36 − 8 (u1 = 44) + 0.6·1.2·2.4 − 0.6·1.2·0.7 (b3 minus
    // its in-u1 part) = 45.224. Rotation preserves volume.
    let mesh = tessellate(&a, out).expect("tessellation");
    let vol = mesh_signed_volume(&mesh);
    assert!((vol - 45.224).abs() < 1e-6, "union volume {vol} != 45.224");
}
