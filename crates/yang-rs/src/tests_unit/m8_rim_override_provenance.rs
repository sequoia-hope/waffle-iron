#[allow(unused_imports)]
use super::*;

// ====================================================================
// M8 — rim-override PROVENANCE (spec `specs/m8_rim_override_provenance.md`)
//
// Two coplanar pairs on the two caps of one lateral each emit the same
// geometric split point in their OWN overlay frame, and each mirrors it
// onto the other cap's rim by an f64 projection — three ULP-twin
// spellings of one point. `RimSplitMap` keeps the cap's own bits as the
// rim's sample: an OWN push replaces a near-twin MIRROR in place, a
// MIRROR push is absorbed by a near OWN sample, and everything else
// stays bit-exact (own vs own: the R0088/R0070 band-close twin
// population; mirror vs mirror: the task-#144 same-ray twin record).
// ====================================================================

use crate::stage0::{RimPush, RimSampleKind, RimSplitMap};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// A one-ULP twin of `v` (the spelling the opposite cap's frame yields).
fn ulp_twin(v: f64) -> f64 {
    f64::from_bits(v.to_bits() + 1)
}

#[test]
fn own_then_mirror_twin_is_absorbed_by_the_own_sample() {
    let mut ov = RimSplitMap::new();
    let own = p(0.0284682039287273, -0.03011978015106054, 0.0);
    let mirror = p(ulp_twin(0.0284682039287273), -0.03011978015106054, 0.0);
    assert_eq!(ov.push_own(0, own), RimPush::Inserted);
    assert_eq!(ov.push_mirror(0, mirror), RimPush::AbsorbedByNear(0));
    assert_eq!(ov.count(0), 1);
    assert_eq!(ov.samples(0)[0].p, own, "the cap's own bits survive");
    assert_eq!(ov.samples(0)[0].kind, RimSampleKind::Own);
}

#[test]
fn mirror_then_own_twin_replaces_the_mirror_in_place() {
    let mut ov = RimSplitMap::new();
    let first = p(1.0, 0.0, 0.0);
    let mirror = p(0.5, 0.5, 0.0);
    let own = p(ulp_twin(0.5), 0.5, 0.0);
    ov.push_mirror(0, first);
    assert_eq!(ov.push_mirror(0, mirror), RimPush::Inserted);
    assert_eq!(ov.push_own(0, own), RimPush::ReplacedMirror(1));
    assert_eq!(ov.count(0), 2, "in place: the count is unchanged");
    assert_eq!(ov.samples(0)[1].p, own);
    assert_eq!(ov.samples(0)[1].kind, RimSampleKind::Own);
    assert_eq!(ov.samples(0)[0].p, first, "unrelated samples untouched");
}

#[test]
fn the_sprocket_cap_pair_sequence_leaves_one_own_sample_per_rim() {
    // Pair (bottom caps): rim 0 emits P, rim 1 receives mirror(P).
    // Pair (top caps): rim 1 emits Q (a ULP twin of P shifted in z), rim 0
    // receives mirror(Q). Each rim must end with exactly ONE sample — its
    // own — so the strip's two chains pair index-for-index.
    let mut ov = RimSplitMap::new();
    let pp = p(0.02, -0.03, 0.0);
    let m_p = p(ulp_twin(0.02), -0.03, 0.005);
    let q = p(0.02, ulp_twin(-0.03), 0.005);
    let m_q = p(0.02, -0.03, 0.0);
    ov.push_own(0, pp);
    ov.push_mirror(1, m_p);
    assert_eq!(ov.push_own(1, q), RimPush::ReplacedMirror(0));
    assert_eq!(ov.push_mirror(0, m_q), RimPush::DuplicateBits(0));
    assert_eq!((ov.count(0), ov.count(1)), (1, 1));
    assert_eq!(ov.samples(0)[0].p, pp);
    assert_eq!(ov.samples(1)[0].p, q);
    let pts = ov.to_points();
    assert_eq!(pts[&0], vec![pp]);
    assert_eq!(pts[&1], vec![q]);
}

#[test]
fn own_vs_own_stays_bit_exact_band_close_twins_both_enter() {
    // Two genuinely distinct crossings of ONE overlay, 1e-10 apart (below
    // TAU_MODEL, far above TAU_WORK): both must enter the ring.
    let mut ov = RimSplitMap::new();
    let a = p(0.5, 0.25, 0.0);
    let b = p(0.5 + 1.0e-10, 0.25, 0.0);
    assert_eq!(ov.push_own(0, a), RimPush::Inserted);
    assert_eq!(ov.push_own(0, b), RimPush::Inserted);
    assert_eq!(ov.count(0), 2);
    // And two own samples within TAU_WORK never merge either — only a
    // MIRROR yields to an own push.
    let c = p(ulp_twin(0.5), 0.25, 0.0);
    assert_eq!(ov.push_own(0, c), RimPush::Inserted);
    assert_eq!(ov.count(0), 3);
}

#[test]
fn mirror_vs_mirror_stays_bit_exact() {
    // Same-ray twin images (task #144 record): two bit-distinct mirrors
    // within TAU_WORK both stay — the downstream azimuth-merge count wall
    // keeps that class loud, exactly as before provenance.
    let mut ov = RimSplitMap::new();
    let a = p(0.5, 0.25, 1.0);
    let b = p(ulp_twin(0.5), 0.25, 1.0);
    assert_eq!(ov.push_mirror(0, a), RimPush::Inserted);
    assert_eq!(ov.push_mirror(0, b), RimPush::Inserted);
    assert_eq!(ov.push_mirror(0, b), RimPush::DuplicateBits(1));
    assert_eq!(ov.count(0), 2);
}

#[test]
fn a_mirror_beyond_tau_work_is_a_distinct_sample() {
    let mut ov = RimSplitMap::new();
    let own = p(0.5, 0.25, 0.0);
    let far = p(0.5 + 2.0e-12, 0.25, 0.0);
    ov.push_own(0, own);
    assert_eq!(ov.push_mirror(0, far), RimPush::Inserted);
    assert_eq!(ov.count(0), 2);
}

#[test]
fn to_points_preserves_insertion_order_and_skips_empty_edges() {
    let mut ov = RimSplitMap::new();
    assert!(ov.is_empty());
    let a = p(1.0, 0.0, 0.0);
    let b = p(0.0, 1.0, 0.0);
    let c = p(0.0, 0.0, 1.0);
    ov.push_own(7, a);
    ov.push_mirror(7, b);
    ov.push_own(3, c);
    assert!(!ov.is_empty());
    let pts = ov.to_points();
    assert_eq!(pts.keys().copied().collect::<Vec<_>>(), vec![3, 7]);
    assert_eq!(pts[&7], vec![a, b]);
    assert_eq!(pts[&3], vec![c]);
    assert_eq!(ov.points(9), None);
    assert_eq!(ov.iter().count(), 2);
}
