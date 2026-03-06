//! Generative property-based tests for multi-step modeling chains.
//!
//! Phase 3 of Assay v2: booleans, tilted planes, 2-5 step chains.

use proptest::prelude::*;
use test_harness::assay::properties_v2::run_generative_chain_oracles;
use test_harness::assay::strategies_v2::{execute_chain, is_known_kernel_limitation, strats_v2};

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 50,
        max_shrink_iters: 100,
        timeout: 60000,  // 60s — chains take longer
        ..ProptestConfig::default()
    })]

    #[test]
    fn generative_chain(
        scenario in strats_v2::generative_chain_scenario()
    ) {
        match execute_chain(&scenario) {
            Ok(mut result) => {
                let verdicts = run_generative_chain_oracles(&mut result, 100.0);
                for v in &verdicts {
                    prop_assert!(
                        v.passed,
                        "chain({}steps) | oracle {} FAILED: {}",
                        result.completed_steps, v.name, v.detail
                    );
                }
            }
            Err(e) if is_known_kernel_limitation(&e) => {
                // Known kernel limitation — discard this case
            }
            Err(e) => {
                prop_assert!(false, "Unexpected chain error: {}", e);
            }
        }
    }
}
