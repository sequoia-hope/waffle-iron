//! M8-INTRA opposite-normal canonicalization — ADVERSARY (FIP Phase 4).
//!
//! Spec: `specs/m8_intra_opposite_plane_canonicalization.md`. The sign-aware
//! `canonicalize_sibling_planes` now matches sibling planes under EITHER sign.
//! The adversary concern this file guards is the flip side of the feature:
//! the new negated-arm matching must NOT wrongly collapse two GENUINELY
//! distinct opposite-normal faces (a solid's top and bottom, or any mirrored
//! parallel pair separated by a real feature-sized gap) onto one plane.
//!
//! These attacks run through the PUBLIC `to_yang_brep` seam (which calls
//! `canonicalize_sibling_planes` internally) on real extruded solids — the
//! producer path. The ULP-exact band/greedy attacks that need hand-crafted
//! plane bits live beside the RED unit tests in `src/boolean.rs` (that private
//! function is unreachable from an integration crate, and the spec's RED note
//! records that "a real solid cannot be coaxed into producing" those bits).

use cad_primitives::{Point2, Point3, Vector3};
use kernel_v2::{extrude, to_yang_brep, BrepArena, Profile};

/// An oblique orthonormal frame (irrational direction cosines) — axis-aligned
/// geometry cancels exactly in the Newell sums and never stresses the
/// rounding-noise band. Copied from `kv10_plane_canonicalization.rs`.
fn oblique_frame() -> ([f64; 3], [f64; 3], [f64; 3]) {
    fn norm(a: [f64; 3]) -> [f64; 3] {
        let l = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
        [a[0] / l, a[1] / l, a[2] / l]
    }
    let u = norm([1.0, 2.0, 3.0]);
    let w = [0.3, -0.4, 0.5];
    let d = w[0] * u[0] + w[1] * u[1] + w[2] * u[2];
    let v = norm([w[0] - d * u[0], w[1] - d * u[1], w[2] - d * u[2]]);
    let n = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    (u, v, n)
}

/// Extrude an oblique box; `base` shifts the in-plane origin so the whole solid
/// can be placed far from the world origin (to probe the scale-relative band).
fn oblique_box(
    a: &mut BrepArena,
    base: [f64; 3],
    x: (f64, f64),
    y: (f64, f64),
    z: (f64, f64),
) -> kernel_v2::SolidId {
    let (u, v, n) = oblique_frame();
    let origin = Point3::new(
        base[0] + z.0 * n[0],
        base[1] + z.0 * n[1],
        base[2] + z.0 * n[2],
    );
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

/// Count DISTINCT planar-face planes (exact bits) emitted for `solid`, and the
/// number of opposite-normal parallel pairs among them (dot ≈ −1).
fn plane_stats(a: &BrepArena, solid: kernel_v2::SolidId) -> (usize, usize) {
    let y = to_yang_brep(a, solid).expect("yang conversion");
    let mut planes: Vec<[f64; 3]> = Vec::new();
    let mut bits: std::collections::BTreeSet<[u64; 4]> = std::collections::BTreeSet::new();
    for f in y.faces() {
        if let yang_rs::Surface::Plane { normal, d } = f.surface {
            let n = normal.as_array();
            bits.insert([n[0].to_bits(), n[1].to_bits(), n[2].to_bits(), d.to_bits()]);
            planes.push(n);
        }
    }
    let mut opposite = 0usize;
    for i in 0..planes.len() {
        for j in (i + 1)..planes.len() {
            let dot: f64 = (0..3).map(|k| planes[i][k] * planes[j][k]).sum();
            if dot < -0.999 {
                opposite += 1;
            }
        }
    }
    (bits.len(), opposite)
}

/// A unit-ish oblique box must emit SIX distinct planes with THREE
/// opposite-normal pairs — the negated-arm matching must not fuse any
/// opposite face onto its partner.
#[test]
fn box_keeps_six_distinct_planes_with_opposite_pairs() {
    let mut a = BrepArena::new();
    let s = oblique_box(&mut a, [0.1, 0.2, 0.3], (0.0, 2.0), (0.0, 3.0), (0.0, 1.5));
    let (distinct, opposite) = plane_stats(&a, s);
    assert_eq!(
        distinct, 6,
        "box emitted {distinct} distinct planes, expected 6"
    );
    assert_eq!(
        opposite, 3,
        "expected 3 opposite-normal pairs, got {opposite}"
    );
}

/// Scale probe: a valid oblique box translated ~1000 units from the origin,
/// where the `TAU_WORK·(1+|d|)` offset band inflates to ~1e-9. Its three
/// opposite-normal pairs (gap ≥ 1.5) must still emit distinct planes — the
/// negated-arm matching stays sense-preserving at realistic CAD coordinates.
///
/// NOTE (adversary finding): the over-merge hazard is NOT reachable through the
/// real producer path. To make an opposite-normal pair's offsets approach the
/// merge band a box would need sub-`TAU_WORK`-thin caps, but such a solid is
/// rejected far upstream by `to_yang_brep`'s planarity gate
/// (`NonPlanarFace`) — a thickness of even 1e-5 with an oblique frame already
/// trips it. The near-band over-merge behavior is therefore pinned at the unit
/// level (`adversary_negated_offset_band_just_below_and_just_above` in
/// `src/boolean.rs`), not E2E.
#[test]
fn far_from_origin_box_not_over_merged() {
    let mut a = BrepArena::new();
    let s = oblique_box(
        &mut a,
        [1000.0, -800.0, 500.0],
        (0.0, 2.0),
        (0.0, 3.0),
        (0.0, 1.5),
    );
    let (distinct, opposite) = plane_stats(&a, s);
    assert_eq!(
        distinct, 6,
        "far box collapsed opposite caps: {distinct} planes"
    );
    assert_eq!(opposite, 3);
}
