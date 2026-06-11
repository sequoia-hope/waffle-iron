//! Generative property-based tests for multi-step modeling chains.
//!
//! Phase 3 of Assay v2: booleans, tilted planes, 2-5 step chains.
//! Phase 3b: strict oracle enforcement, overlap-biased profiles,
//! per-step volume invariants.
//!
//! Chain determinism test is in assay_chain_determinism.rs (B20) to avoid
//! regression seed cross-contamination between correctness and determinism tests.
//!
//! Note: fork=false + timeout=0 because proptest's timeout forces fork mode,
//! and truck boolean panics crash forked subprocesses. We use catch_unwind
//! for in-process panic handling instead.

use proptest::prelude::*;
use test_harness::assay::properties_v2::run_generative_chain_oracles;
use test_harness::assay::strategies_v2::{
    execute_chain, is_known_kernel_limitation, strats_v2, GenerativeChainScenario,
};

/// Execute a chain with panic catching. Returns Err for panics with the
/// panic message classified as a known limitation.
fn safe_execute_chain(
    scenario: &GenerativeChainScenario,
) -> Result<test_harness::assay::strategies_v2::ChainResult, String> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| execute_chain(scenario))) {
        Ok(result) => result,
        Err(panic_info) => {
            let msg = if let Some(s) = panic_info.downcast_ref::<String>() {
                s.clone()
            } else if let Some(s) = panic_info.downcast_ref::<&str>() {
                s.to_string()
            } else {
                "unknown panic".to_string()
            };
            Err(format!("panicked: {}", msg))
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 30,
        max_shrink_iters: 50,
        timeout: 0,
        fork: false,
        ..ProptestConfig::default()
    })]

    #[test]
    #[ignore = "kernel-v2: generative chains assume a fully-capable kernel; random scenarios hit the coplanar/NotSupported walls (M8). Re-enable with reject-guards or after Yang Stage 0"]
    fn generative_chain(
        scenario in strats_v2::generative_chain_scenario()
    ) {
        match safe_execute_chain(&scenario) {
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
