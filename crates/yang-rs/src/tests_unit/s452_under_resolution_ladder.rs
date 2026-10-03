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
    // so a demand of exactly 4 must start at 8.
    assert_eq!(
        refine_452_rounds_for(Some(4.0)),
        vec![8.0, 16.0, 32.0, 64.0]
    );
    // Exactly at the ceiling the certificate can no longer name a rung the
    // budget reaches — see `demand_beyond_the_ceiling_still_runs_the_budget`.
    assert_eq!(
        refine_452_rounds_for(Some(REFINE_452_MAX_FACTOR)),
        P0015_FULL_LADDER.to_vec()
    );
}

/// P0015 (prospector seed 2 index 54, minimized 3 ops: a square boss, a
/// 27-tooth gear boss, a pentagon revolve-cut), measured 2026-10-03 in
/// release with `YANG_452_ROUNDS=2,4,8,16,32,64 YANG_452_PROBE=1`.
///
/// `YANG_S4_CARRIER_DOMAIN=census` reads SIX §4-I9 fires on one gear-facet
/// fan. The traveller's far surface is the revolve-cut's single lateral face
/// `B:2`, whose Stage-1 chord band is `d_ε = 9.118876e-2`; the TIGHTEST
/// crossed corner (q = v154 / v156) clears it by `5.721696e-4`, so
/// `under_resolution_ratio` — a max over every fire — reports **159.3737**.
/// That is past [`REFINE_452_MAX_FACTOR`], and the pre-2026-10-03 rule read
/// a demand past the ceiling as a PROOF of futility and ran ZERO rungs.
///
/// The measured ladder refutes the proof. `d_far(q)` is a property of the
/// corner, invariant under refinement, so the re-measured demand is exactly
/// `demand / f` at every rung — and the op CONVERGED at `d_ε/32` while the
/// certificate still read 9.96, i.e. eight rungs below what it demanded:
///
/// | rung | B tris | re-measured demand |
/// |---|---|---|
/// | natural | 126 | 159.3736669947812 |
/// | d_ε/2 | 166 | 79.6868334973906 |
/// | d_ε/4 | 236 | 39.8434167486953 |
/// | d_ε/8 | 316 | 19.92170837434765 |
/// | d_ε/16 | 446 | 9.960854187173824 |
/// | d_ε/32 | 626 | Ok, 5310 tris, unpaired 0, improper 0 |
///
/// So the certificate is SUFFICIENT ("this rung resolves that corner"), never
/// NECESSARY, and a demand it cannot place inside the budget says nothing
/// about the rungs inside the budget: the ladder must run the budget it has.
const P0015_D_EPS_FAR: f64 = 9.118876e-2;
const P0015_TIGHTEST_CLEARANCE: f64 = 5.721696e-4;
/// The rung at which P0015 emitted a watertight 2-manifold body.
const P0015_CONVERGING_RUNG: f64 = 32.0;
/// The whole doubling budget: [`REFINE_452_ROUNDS`]'s first rung to the
/// ceiling.
const P0015_FULL_LADDER: &[f64] = &[2.0, 4.0, 8.0, 16.0, 32.0, 64.0];

#[test]
fn demand_beyond_the_ceiling_still_runs_the_budget() {
    let demand = P0015_D_EPS_FAR / P0015_TIGHTEST_CLEARANCE;
    assert!((demand - 159.3737).abs() < 1e-3, "demand {demand}");
    // The certificate's own first-rung rule names 256 — past the ceiling, so
    // it cannot narrow a ladder that stops at 64.
    assert!(demand >= REFINE_452_MAX_FACTOR);
    let rungs = refine_452_rounds_for(Some(demand));
    assert_eq!(rungs, P0015_FULL_LADDER.to_vec());
    // The rung that actually converged is IN the ladder, and the certificate
    // still called it under-resolved by 4.98× when it did.
    assert!(rungs.contains(&P0015_CONVERGING_RUNG));
    let residual = demand / P0015_CONVERGING_RUNG;
    assert!(
        residual > 1.0,
        "the converging rung was still 'under-resolved' at {residual}"
    );
    // A corner ON the far surface (clearance 0 ⇒ infinite demand) is the same
    // uninformative certificate, not a licence to skip the budget.
    assert_eq!(
        refine_452_rounds_for(Some(f64::INFINITY)),
        P0015_FULL_LADDER.to_vec()
    );
    assert_eq!(refine_452_rounds_for(Some(1e3)), P0015_FULL_LADDER.to_vec());
}

#[test]
fn the_re_measured_demand_is_the_natural_demand_over_the_rung() {
    // The measured column above, to the digits the probe printed: the
    // certificate carries no new information per rung, so a re-measured
    // demand can never justify skipping a rung either.
    let natural = 159.3736669947812_f64;
    for (f, want) in [
        (2.0, 79.6868334973906_f64),
        (4.0, 39.8434167486953),
        (8.0, 19.92170837434765),
        (16.0, 9.960854187173824),
    ] {
        assert_eq!(natural / f, want, "rung {f}");
    }
}
