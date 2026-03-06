//! Generative property-based tests for single extrude operations.
//!
//! Phase 1: Convex polygon profiles on axis-aligned planes.
//! Phase 2: Non-convex, star, arc-polygon profiles with region decomposition.

use proptest::prelude::*;
use test_harness::assay::properties_v2::run_generative_extrude_oracles;
use test_harness::assay::strategies_v2::{
    execute_generative_extrude, execute_generative_profile, is_known_kernel_limitation, strats_v2,
};

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 50,
        max_shrink_iters: 100,
        timeout: 30000,
        ..ProptestConfig::default()
    })]

    /// Phase 1: Convex polygon profiles — basic extrude validation.
    #[test]
    fn generative_single_extrude(
        scenario in strats_v2::generative_extrude_scenario()
    ) {
        match execute_generative_extrude(&scenario) {
            Ok(mut builder) => {
                let verdicts = run_generative_extrude_oracles(
                    &mut builder, "body", 100.0
                );
                for v in &verdicts {
                    prop_assert!(
                        v.passed,
                        "scenario={} | oracle {} FAILED: {}",
                        scenario, v.name, v.detail
                    );
                }
            }
            Err(e) if is_known_kernel_limitation(&e) => {
                // Known kernel limitation — discard this case
            }
            Err(e) => {
                prop_assert!(false, "scenario={} | Unexpected error: {}", scenario, e);
            }
        }
    }

    /// Phase 2: Complex profiles (non-convex, star, arc-polygon) — extrude validation.
    #[test]
    fn generative_complex_profile_extrude(
        scenario in strats_v2::generative_profile_scenario()
    ) {
        match execute_generative_profile(&scenario) {
            Ok(mut builder) => {
                let verdicts = run_generative_extrude_oracles(
                    &mut builder, "body", 100.0
                );
                for v in &verdicts {
                    prop_assert!(
                        v.passed,
                        "scenario={} | oracle {} FAILED: {}",
                        scenario, v.name, v.detail
                    );
                }
            }
            Err(e) if is_known_kernel_limitation(&e) => {
                // Known kernel limitation — discard this case
            }
            Err(e) => {
                prop_assert!(false, "scenario={} | Unexpected error: {}", scenario, e);
            }
        }
    }
}
