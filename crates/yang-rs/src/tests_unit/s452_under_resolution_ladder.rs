//! Yang §4.5.2 — the under-resolution certificate names the FIRST density
//! rung of the one refinement loop (spec `specs/yang_452_refinement_loop.md`
//! §6.1; `boolean::failures::density_for_demand`).
//!
//! History: these pins first guarded the whole-op ladder's rung list
//! (`refine_452_rounds_for`, d_ε/2 … d_ε/64). The ladder was replaced by
//! the per-face loop in the faithful cleanup's Phase 1c (2026-10-14); the
//! certificate's role is unchanged — it is SUFFICIENT ("this rung places
//! that corner"), never necessary — and now expresses itself as a density
//! factor `k = next power of two ≥ √f` (chord sagitta ∝ 1/N², so d_ε/f is
//! density √f). The measurements pinned here are the originals.
//!
//! R0085 op 2 (2026-09-18, release): a 1.05-radius torus grazes a gear
//! extrude's cap; the §4-I9 STOP names a traveller whose own crossed corner
//! reads demand 1.87, but the same chain's rider at the tooth corner nearest
//! the exact exit reads |d_far(q)| = 1.4410e-3 against d_ε = 2.6205e-2 —
//! demand 18.19 — and d_ε/32 converges (d_ε/16 still exceeds the
//! clearance). P0015 (2026-10-03): demand 159 and the op converges at
//! d_ε/32 — a demand past the old ceiling certified nothing, so the loop
//! starts at the largest rung the ceiling allows and keeps doubling.
//! P0039 (2026-10-12): demand 2.24 and the op converges only at d_ε/64 —
//! the first rung is the start of the sequence, never its end.

use crate::boolean::failures::density_for_demand;
use crate::stage1_tessellate::FACE_REFINE_MAX;

/// R0085 op 2: the torus's Stage-1 chord band and the tightest crossed
/// corner's clearance (`YANG_S4_CARRIER_DOMAIN=census`, `-RESOLUTION` v4214).
const R0085_D_EPS_FAR: f64 = 2.620494e-2;
const R0085_TIGHTEST_CLEARANCE: f64 = 1.441027e-3;
/// The STOP'd site's own clearance (v386 crossing v387) — RESOLVED alone.
const R0085_STOP_SITE_CLEARANCE: f64 = 1.402243e-2;

#[test]
fn no_certificate_or_a_satisfied_one_asks_for_one_doubling() {
    assert_eq!(density_for_demand(None), 2);
    assert_eq!(density_for_demand(Some(f64::NAN)), 2);
    assert_eq!(density_for_demand(Some(0.5)), 2);
    assert_eq!(density_for_demand(Some(1.0)), 2);
    let own = R0085_D_EPS_FAR / R0085_STOP_SITE_CLEARANCE;
    assert!((own - 1.8688).abs() < 1e-3, "own ratio {own}");
    assert_eq!(
        density_for_demand(Some(own)),
        2,
        "d_ε/2 … d_ε/4 is density 2"
    );
    assert_eq!(density_for_demand(Some(2.24)), 2, "P0039's natural demand");
    assert_eq!(density_for_demand(Some(3.9)), 2);
}

#[test]
fn r0085_s_rider_demand_names_density_eight() {
    // 18.19 lies between 4² = 16 and 8² = 64: d_ε/32 is the rung that
    // converged, which is density √32 ≈ 5.7 → the next power of two, 8.
    let demand = R0085_D_EPS_FAR / R0085_TIGHTEST_CLEARANCE;
    assert!((demand - 18.185).abs() < 1e-2, "demand {demand}");
    assert_eq!(density_for_demand(Some(demand)), 8);
    assert_eq!(density_for_demand(Some(16.0)), 4, "exactly 4² asks for 4");
    assert_eq!(density_for_demand(Some(16.5)), 8);
}

#[test]
fn p0015_s_demand_past_the_old_ceiling_starts_at_the_ceiling() {
    // √159 ≈ 12.6 → 16, which is the ceiling since 2026-10-25 (it was 8, the
    // old d_ε/64 as a density, until P0056 / F0067 measured convergence at
    // ×16). P0015 converged at d_ε/32 while its certificate read 159 — a
    // demand past the ceiling certifies nothing about the rungs inside it.
    assert_eq!(FACE_REFINE_MAX, 16, "four doublings from the natural mesh");
    assert_eq!(density_for_demand(Some(159.37)), 16);
    assert_eq!(density_for_demand(Some(4096.0)), FACE_REFINE_MAX);
    assert_eq!(density_for_demand(Some(1e12)), FACE_REFINE_MAX);
    assert_eq!(density_for_demand(Some(60.0)), 8, "√60 ≈ 7.7 → 8");
}
