#[allow(unused_imports)]
use super::*;

// ====================================================================
// M8 — Stage-0 rim membership refinement
// (spec `specs/m8_stage0_rim_membership_refine.md`, gated at the call
// site by `YANG_STAGE0_RIM_REFINE`; the primitive itself is pure).
//
// The §4.5.5 2D Boolean classifies membership against the disc's CHORD
// polygon; a partner chain vertex strictly inside the exact rim circle
// but inside a sag crescent (outside the chord polygon) is misclassified
// `AOnly` (F0067: 126 gear root corners at dr −3.1e-4..−1.34e-3 →
// missing flank junctions → the A-top rim-weave → Stage-6
// non-2-manifold). `refine_rim_membership` subdivides rim spans with
// exact on-circle samples until the polygonal membership agrees with
// the exact Boolean for every partner feature, propagating each new
// sample bit-shared into the poly ring, the rim resolution map, and the
// cap + opposite rim overrides (matched counts).
// ====================================================================

use super::m5_case_iii::graze_cyl;
use crate::coplanar_overlay::{ExactPoint2, PolygonWithHoles};
use crate::stage0::{canonical_frame, refine_rim_membership, Frame, RimSplitMap};
use cad_primitives::Point2;
use std::collections::BTreeMap;

/// Unit cylinder fixture (r=1, axis +z, base at origin) + the frame of
/// its BOTTOM cap (face 1, rim edge 0, opposite rim edge 1) + an 8-gon
/// rim ring in that frame with its rim resolution map. 8-gon sagitta:
/// 1 − cos(π/8) ≈ 7.61e-2.
fn disc_fixture() -> (BRep, Frame, PolygonWithHoles, BTreeMap<ExactPoint2, Point3>) {
    let cyl = graze_cyl([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0, 1.0);
    let frame = canonical_frame(&cyl, 1).expect("bottom cap frame");
    let mut ring2 = Vec::new();
    let mut rim = BTreeMap::new();
    for k in 0..8u32 {
        let th = f64::from(k) * std::f64::consts::TAU / 8.0;
        let p3 = Point3::new(th.cos(), th.sin(), 0.0);
        let (u, v) = frame.project(p3);
        ring2.push((u, v, p3));
    }
    // Ring order = boundary order; the refinement handles either
    // orientation (shoelace), so world-θ order is fine as-is.
    let outer: Vec<Point2> = ring2.iter().map(|&(u, v, _)| Point2::new(u, v)).collect();
    for &(u, v, p3) in &ring2 {
        rim.insert(ExactPoint2::from_f64(u, v).expect("rim key"), p3);
    }
    (
        cyl,
        frame,
        PolygonWithHoles {
            outer,
            holes: Vec::new(),
        },
        rim,
    )
}

/// In-frame 2D coordinates of a world point at (radius, world azimuth θ)
/// on the cap plane.
fn feat2(frame: &Frame, r: f64, th: f64) -> Point2 {
    let (u, v) = frame.project(Point3::new(r * th.cos(), r * th.sin(), 0.0));
    Point2::new(u, v)
}

/// A partner vertex in a sag crescent refines the ring until the vertex
/// is strictly inside the chord polygon; every inserted sample lies on
/// the exact circle and lands in BOTH rims' overrides with matched
/// counts; a second call is a fixpoint no-op.
#[test]
fn crescent_feature_refines_to_containment() {
    let (cyl, frame, mut poly, mut rim) = disc_fixture();
    // Mid-span azimuth π/8, radius 0.95: chord min radius cos(π/8) ≈
    // 0.9239 < 0.95 < 1 ⇒ strictly inside the circle, strictly outside
    // the chord — the misclassification geometry.
    let partner = vec![feat2(&frame, 0.95, std::f64::consts::PI / 8.0)];
    let mut overrides: RimSplitMap = BTreeMap::new();
    let n = refine_rim_membership(
        &cyl,
        1,
        &mut poly,
        &mut rim,
        &partner,
        &frame,
        &mut overrides,
    )
    .expect("refine");
    assert!(n >= 1, "crescent feature must force ≥1 inserted sample");
    assert_eq!(poly.outer.len(), 8 + n, "ring grew by the inserted count");
    assert_eq!(rim.len(), 8 + n, "rim map grew in lockstep");
    // Every ring sample on the exact circle (stage-1 rim band).
    let (cu, cv) = frame.project(Point3::new(0.0, 0.0, 0.0));
    for q in &poly.outer {
        let r = ((q.x() - cu).powi(2) + (q.y() - cv).powi(2)).sqrt();
        assert!(
            (r - 1.0).abs() <= 1e-9 * 2.0,
            "ring sample off-circle: r={r}"
        );
    }
    // Overrides: cap rim edge 0 and opposite rim edge 1, matched counts
    // (the shared lateral's azimuth-merge conformality requirement).
    assert_eq!(overrides.get(&0).map(Vec::len), Some(n), "cap overrides");
    assert_eq!(
        overrides.get(&1).map(Vec::len),
        Some(n),
        "opposite overrides"
    );
    // Fixpoint: the violation is cleared.
    let n2 = refine_rim_membership(
        &cyl,
        1,
        &mut poly,
        &mut rim,
        &partner,
        &frame,
        &mut overrides,
    )
    .expect("refine fixpoint");
    assert_eq!(n2, 0, "second call must be a no-op");
}

/// A partner vertex within the stage-1 rim band of the circle is
/// on-circle content (junction/tangency machinery owns it) — the band
/// floor excludes it and nothing is inserted.
#[test]
fn band_floor_feature_is_untouched() {
    let (cyl, frame, mut poly, mut rim) = disc_fixture();
    let partner = vec![feat2(&frame, 1.0 - 1e-10, std::f64::consts::PI / 8.0)];
    let mut overrides: RimSplitMap = BTreeMap::new();
    let n = refine_rim_membership(
        &cyl,
        1,
        &mut poly,
        &mut rim,
        &partner,
        &frame,
        &mut overrides,
    )
    .expect("refine");
    assert_eq!(n, 0, "band-floor feature must not refine");
    assert!(overrides.is_empty());
}

/// A partner vertex outside the circle never violates membership (the
/// chord polygon is inscribed — it can only under-cover the disc).
#[test]
fn outside_feature_is_untouched() {
    let (cyl, frame, mut poly, mut rim) = disc_fixture();
    let partner = vec![feat2(&frame, 1.05, std::f64::consts::PI / 8.0)];
    let mut overrides: RimSplitMap = BTreeMap::new();
    let n = refine_rim_membership(
        &cyl,
        1,
        &mut poly,
        &mut rim,
        &partner,
        &frame,
        &mut overrides,
    )
    .expect("refine");
    assert_eq!(n, 0, "outside feature must not refine");
}

/// Deep-crescent feature (root-corner class, δ≈7.6e-2·…): multiple
/// rounds converge, and every violation among SEVERAL features on
/// distinct spans is cleared in one call.
#[test]
fn multiple_features_multiple_spans_converge() {
    let (cyl, frame, mut poly, mut rim) = disc_fixture();
    let partner = vec![
        feat2(&frame, 0.93, std::f64::consts::PI / 8.0),
        feat2(&frame, 0.97, 3.0 * std::f64::consts::PI / 8.0),
        feat2(&frame, 0.999, 5.0 * std::f64::consts::PI / 8.0),
    ];
    let mut overrides: RimSplitMap = BTreeMap::new();
    let n = refine_rim_membership(
        &cyl,
        1,
        &mut poly,
        &mut rim,
        &partner,
        &frame,
        &mut overrides,
    )
    .expect("refine");
    assert!(n >= 3, "three crescent features force ≥3 samples, got {n}");
    let n2 = refine_rim_membership(
        &cyl,
        1,
        &mut poly,
        &mut rim,
        &partner,
        &frame,
        &mut overrides,
    )
    .expect("refine fixpoint");
    assert_eq!(n2, 0);
}
