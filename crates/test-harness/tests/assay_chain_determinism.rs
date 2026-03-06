//! Chain determinism tests (B20).
//!
//! Separated from assay_generative_chain.rs so that proptest regression seeds
//! from chain correctness tests don't get replayed for determinism checks.

use proptest::prelude::*;
use test_harness::assay::strategies_v2::{
    execute_chain, is_known_kernel_limitation, strats_v2, GenerativeChainScenario,
};
use test_harness::helpers::mesh_volume;

/// Execute a chain with panic catching.
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
        cases: 10,
        max_shrink_iters: 30,
        timeout: 0,
        fork: false,
        ..ProptestConfig::default()
    })]

    /// Chain determinism: run each scenario 3 times and compare topology
    /// counts (V, E, F) and volumes across runs.
    ///
    /// B20: Reset the global SequentialID counter between runs so each run
    /// starts from the same ID base. Without this, FxHashMap/FxHashSet with
    /// SequentialID keys produce different bucket assignments across runs,
    /// which can cause non-deterministic iteration order in any iterated
    /// hash-based collection within the truck pipeline.
    #[test]
    fn chain_deterministic(
        scenario in strats_v2::generative_chain_scenario()
    ) {
        let mut topologies: Vec<(usize, usize, usize)> = Vec::new();
        let mut volumes: Vec<f64> = Vec::new();

        for _ in 0..3 {
            // Reset ID counter so each run allocates identical SequentialIDs.
            // Safe because the previous run's entities are dropped before reset.
            truck_base::reset_id_sequence();

            match safe_execute_chain(&scenario) {
                Ok(mut result) => {
                    if let Ok(topo) = result.builder.topology_counts(&result.final_feature) {
                        topologies.push(topo);
                    }
                    if let Ok(mesh) = result.builder.tessellate(&result.final_feature) {
                        volumes.push(mesh_volume(&mesh));
                    }
                }
                Err(e) if is_known_kernel_limitation(&e) => return Ok(()),
                Err(e) => {
                    prop_assert!(false, "Unexpected chain error: {}", e);
                }
            }
        }

        // B20: Topology determinism is a hard assertion.
        for i in 1..topologies.len() {
            prop_assert_eq!(
                topologies[0], topologies[i],
                "Non-deterministic topology: run0={:?} vs run{}={:?}",
                topologies[0], i, topologies[i]
            );
        }

        // Volume determinism: should match within tessellation tolerance.
        for i in 1..volumes.len() {
            let diff = (volumes[0] - volumes[i]).abs();
            let tol = volumes[0].abs() * 0.001 + 0.1;
            prop_assert!(
                diff <= tol,
                "Non-deterministic volume: run0={:.4} vs run{}={:.4} (diff={:.4}, tol={:.4})",
                volumes[0], i, volumes[i], diff, tol
            );
        }
    }
}
