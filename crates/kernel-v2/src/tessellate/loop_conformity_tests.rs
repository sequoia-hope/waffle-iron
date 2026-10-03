//! P0013 — the render chord density a face's own loop clearance demands
//! (spec `specs/yang_p0013_tip_land_under_the_chord.md` §4 P4).
//!
//! A planar cap bounded by a full circle is rendered as the INSCRIBED
//! `N`-gon, which recedes from the true circle by the sagitta
//! `s(N) = r·(1 − cos(π/N))`. An inner loop the exact face clears by LESS
//! than `s(N)` is therefore CUT by the chord polygon: the two constraint
//! rings cross and the CDT rightly refuses the ring. These pin the
//! derivation that sizes `N` from the clearance instead.
//!
//! Run: `cargo test -p kernel-v2 --lib loop_conformity`

use super::{
    circle_segment_count, loop_conformity_n_for, LOOP_CONFORMITY_MAX_SEGMENTS,
    RENDER_CHORD_TOLERANCE_REL,
};

/// P0013's measured numbers, verbatim: the boss cap's circle and the star
/// hole's 7th tip, which the exact geometry clears by 9.2807e-6
/// (`KV2_LOOP_CONFORMITY_PROBE` prints exactly this pair).
const P0013_R: f64 = 2.1627665069443046e-2;
const P0013_CLEARANCE: f64 = 9.280774694829519e-6;

#[test]
fn loop_conformity_derives_p0013s_measured_demand() {
    assert_eq!(loop_conformity_n_for(P0013_R, P0013_CLEARANCE), Some(108));
}

/// The derivation's own contract: the derived `N`'s sagitta is strictly
/// below the clearance, and `N − 1`'s is not (it is the SMALLEST such `N`).
#[test]
fn derived_n_is_the_smallest_whose_sagitta_clears() {
    let sag = |n: u32| P0013_R * (1.0 - (std::f64::consts::PI / f64::from(n)).cos());
    let n = loop_conformity_n_for(P0013_R, P0013_CLEARANCE).unwrap();
    assert!(
        sag(n) < P0013_CLEARANCE,
        "sag({n}) = {:e} must clear {P0013_CLEARANCE:e}",
        sag(n)
    );
    assert!(
        sag(n - 1) >= P0013_CLEARANCE,
        "sag({}) = {:e} must NOT clear {P0013_CLEARANCE:e}",
        n - 1,
        sag(n - 1)
    );
}

/// The canonical render density (N = 71 at rel = 1e-3) does NOT clear
/// P0013's land — the premise of the whole increment.
#[test]
fn canonical_density_does_not_clear_p0013s_land() {
    let n = circle_segment_count(RENDER_CHORD_TOLERANCE_REL);
    assert_eq!(n, 71);
    let sag = P0013_R * (1.0 - (std::f64::consts::PI / f64::from(n)).cos());
    assert!(
        sag > P0013_CLEARANCE,
        "sag(71) = {sag:e} must exceed the 9.2808e-6 land"
    );
}

#[test]
fn non_positive_clearance_derives_nothing() {
    // A loop that reaches the circle, or crosses it: an invalid face, not an
    // under-sampled one — the loud CDT reject stays the tripwire.
    assert_eq!(loop_conformity_n_for(P0013_R, 0.0), None);
    assert_eq!(loop_conformity_n_for(P0013_R, -1e-9), None);
    assert_eq!(loop_conformity_n_for(0.0, 1e-6), None);
}

#[test]
fn a_roomy_clearance_derives_a_small_or_no_demand() {
    // Half the radius of room: the demand is tiny (and well under the
    // canonical 71, so `tessellate` keeps the canonical density).
    let n = loop_conformity_n_for(P0013_R, P0013_R * 0.5).unwrap();
    assert!(
        n < 71,
        "a roomy clearance must not raise the density (N={n})"
    );
}

/// Near-tangency fails CLOSED: no practical density separates the loops, so
/// the derivation declines rather than emitting a ruinous `N`.
#[test]
fn near_tangency_declines_above_the_cap() {
    let huge = loop_conformity_n_for(P0013_R, 1e-18);
    assert_eq!(huge, None);
    // Just inside the cap still derives.
    let sag_at_cap =
        P0013_R * (1.0 - (std::f64::consts::PI / f64::from(LOOP_CONFORMITY_MAX_SEGMENTS)).cos());
    assert!(loop_conformity_n_for(P0013_R, sag_at_cap * 1.5).is_some());
}
