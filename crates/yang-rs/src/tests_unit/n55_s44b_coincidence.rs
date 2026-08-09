//! N55 §4.4.1(b) numerical-duplicate merge criterion
//! (`is_relocation_coincidence`, deviation N55).
//!
//! The compliant replacement for the retired `subfeature` weld's absolute
//! `MIN_FEATURE_SIZE` floor. Yang Fig-11(b) "merge p with q if too close" is a
//! numerical-coincidence test: two relocated endpoints are the SAME
//! intersection point iff their gap is below the scale-relative working
//! tolerance `TAU_WORK·(1+scale)`. These oracles pin the load-bearing
//! discriminant: it merges machine-ε relocation twins (exact-dedup — the
//! recovered R0055/F0056/F0057/F0059) but REFUSES a genuine sub-feature edge
//! at micro-scale (R0072's ~1e-7 collapse — the R0091 silent-wrong hazard),
//! which must stay a loud STOP → curved re-CDT.

use crate::stage4_correct::is_relocation_coincidence;
use cad_primitives::{MIN_FEATURE_SIZE, TAU_WORK};

/// Machine-ε relocation twins merge at every model scale (the recovered cases:
/// R0055 at scale ~58, F0056/F0057/F0059 at scale ~0.25 — all gaps ~1e-16).
#[test]
fn machine_epsilon_twins_merge_at_any_scale() {
    // R0055-shape: gap ~5e-15 at scale ~58.
    assert!(is_relocation_coincidence(5e-15, 58.0));
    // F0056-shape: gap ~1e-16 at scale ~0.26.
    assert!(is_relocation_coincidence(1e-16, 0.26));
    // Exactly coincident.
    assert!(is_relocation_coincidence(0.0, 1000.0));
}

/// The R0072 discriminant: a ~1e-7 gap at micro-scale (~2e-4 span) is 0.4 % of
/// the model — a REAL sub-feature edge, NOT a numerical coincidence. The
/// compliant band must REFUSE it (the old absolute `MIN_FEATURE_SIZE` floor
/// wrongly accepted it → R0091 silent-wrong hazard). This is the whole reason
/// N55 exists; if this ever passes, the criterion has regressed to the weld.
#[test]
fn micro_scale_sub_feature_edge_is_refused() {
    // R0072's largest merge: len 9.1e-7 at scale 2.35e-4.
    assert!(!is_relocation_coincidence(9.1e-7, 2.35e-4));
    // …and its smallest non-ε merge: len 3.5e-8 at scale 4.6e-4 — still ≫ band.
    assert!(!is_relocation_coincidence(3.5e-8, 4.6e-4));
    // The old absolute floor would have merged all of these (< MIN_FEATURE_SIZE).
    const _: () = assert!(9.1e-7 < MIN_FEATURE_SIZE && 3.5e-8 < MIN_FEATURE_SIZE);
}

/// The band is exactly `TAU_WORK·(1+scale)` — strictly below merges, at/above
/// refuses. Pins the boundary so a future edit cannot silently loosen it.
#[test]
fn band_is_scale_relative_tau_work() {
    let scale = 10.0;
    let band = TAU_WORK * (1.0 + scale);
    assert!(is_relocation_coincidence(band * 0.5, scale));
    assert!(!is_relocation_coincidence(band, scale)); // strict `<`
    assert!(!is_relocation_coincidence(band * 2.0, scale));
    // Never as loose as the retired absolute floor, at any realistic scale.
    assert!(band < MIN_FEATURE_SIZE);
}
