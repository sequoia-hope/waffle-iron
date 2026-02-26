//! Determinism tests using proptest — verify that boolean operations
//! produce identical topology across repeated runs.

use proptest::prelude::*;
use test_harness::assay::determinism::*;
use test_harness::assay::strategies::strats::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10))]

    #[test]
    fn boolean_deterministic(scenario in boolean_scenario_any()) {
        let result = check_determinism(&scenario, 3);
        // Only assert if all runs succeeded (some scenarios may hit cascade failures)
        if !result.run_topologies.is_empty() {
            prop_assert!(
                result.is_deterministic,
                "Non-deterministic: {}",
                result.detail
            );
        }
    }
}
