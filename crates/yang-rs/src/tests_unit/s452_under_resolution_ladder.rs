//! Yang §4.5.2 op-level refinement — the UNDER-RESOLUTION certificate picks
//! the ladder (spec `specs/yang_452_local_refinement.md` §8;
//! `boolean::refine_452_rounds_for`, `stage4_correct::under_resolution_ratio`).
//!
//! Pinned on the R0085 op-2 measurement (2026-09-18, release,
//! `YANG_452_REFINE=census YANG_452_ROUNDS=2,4,8,16,32,64`): a 1.05-radius
//! torus grazes a gear extrude's cap at a tooth whose root polyline has rim
//! edges of 1.7e-3 … 1.9e-2 while the torus's Stage-1 chord band is
//! d_ε = 2.6205e-2. The §4-I9 STOP names traveller v386 (its own crossed
//! corner clears the torus by 1.4022e-2 — demand 1.87, within the fixed ladder), but the
//! same chain's rider at the tooth corner nearest the exact exit reads
//! |d_far(q)| = 1.4410e-3 — demand 18.19 — and every rung
//! d_ε/2 … d_ε/16 reproduces a Stage-4 STOP on that chain (d_ε/16 =
//! 1.64e-3 still exceeds the 1.44e-3 clearance); d_ε/32 = 8.19e-4 converges
//! (0 unpaired, 0 improper) and so does d_ε/64. The fixed `[2, 4]` budget
//! could never reach it; the certificate's own inequality names the rung.

use crate::boolean::{refine_452_rounds_for, REFINE_452_MAX_FACTOR, REFINE_452_ROUNDS};

/// R0085 op 2: the torus's Stage-1 chord band and the tightest crossed
/// corner's clearance (`YANG_S4_CARRIER_DOMAIN=census`, `-RESOLUTION` v4214).
const R0085_D_EPS_FAR: f64 = 2.620494e-2;
const R0085_TIGHTEST_CLEARANCE: f64 = 1.441027e-3;
/// The STOP'd site's own clearance (v386 crossing v387) — RESOLVED alone.
const R0085_STOP_SITE_CLEARANCE: f64 = 1.402243e-2;

#[test]
fn no_certificate_keeps_the_fixed_budget() {
    assert_eq!(refine_452_rounds_for(None), REFINE_452_ROUNDS.to_vec());
}

#[test]
fn resolved_sites_keep_the_fixed_budget() {
    // The STOP'd site alone reads 1.87 (the census prints its reciprocal,
    // 0.535) — the fixed ladder's d_ε/2 already places its corner; the
    // fixed ladder is the (unchanged) answer. Likewise any demand the fixed
    // ladder's last rung already exceeds.
    let own = R0085_D_EPS_FAR / R0085_STOP_SITE_CLEARANCE;
    assert!((own - 1.8688).abs() < 1e-3, "own ratio {own}");
    assert_eq!(refine_452_rounds_for(Some(own)), REFINE_452_ROUNDS.to_vec());
    assert_eq!(refine_452_rounds_for(Some(3.9)), REFINE_452_ROUNDS.to_vec());
    assert_eq!(
        refine_452_rounds_for(Some(f64::NAN)),
        REFINE_452_ROUNDS.to_vec()
    );
}

#[test]
fn r0085_demand_starts_the_ladder_at_the_converging_rung() {
    let demand = R0085_D_EPS_FAR / R0085_TIGHTEST_CLEARANCE;
    assert!((demand - 18.185).abs() < 1e-2, "demand {demand}");
    let rungs = refine_452_rounds_for(Some(demand));
    // First rung: the smallest power of two STRICTLY above the demand, so
    // d_ε/f < |d_far(q)| at the tightest corner (the certificate's own
    // inequality); then doubling to the ceiling.
    assert_eq!(rungs, vec![32.0, 64.0]);
    let (first, below) = (rungs[0], rungs[0] / 2.0);
    assert!(R0085_D_EPS_FAR / first < R0085_TIGHTEST_CLEARANCE);
    assert!(
        R0085_D_EPS_FAR / below > R0085_TIGHTEST_CLEARANCE,
        "the rung below ({below}) is still under-resolved"
    );
}

#[test]
fn demand_at_a_power_of_two_is_strict() {
    // d_ε/4 == |d_far(q)| is UNDER-RESOLVED (the census reads `dq <= de`),
    // so a demand of exactly 4 must start at 8, and exactly 64 exhausts the
    // ceiling.
    assert_eq!(
        refine_452_rounds_for(Some(4.0)),
        vec![8.0, 16.0, 32.0, 64.0]
    );
    assert_eq!(
        refine_452_rounds_for(Some(REFINE_452_MAX_FACTOR)),
        Vec::<f64>::new()
    );
}

#[test]
fn demand_beyond_the_ceiling_pays_no_futile_rung() {
    // A corner 1e-9 from the far surface would need d_ε/2.6e7: no rung is
    // certified, the ladder is empty and the standing STOP stands (the
    // guard shell's budget clause, honest and immediate).
    assert!(refine_452_rounds_for(Some(1e3)).is_empty());
    assert!(refine_452_rounds_for(Some(f64::INFINITY)).is_empty());
}
