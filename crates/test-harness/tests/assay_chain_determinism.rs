//! Chain determinism tests (B20).
//!
//! Separated from assay_generative_chain.rs so that proptest regression seeds
//! from chain correctness tests don't get replayed for determinism checks.
//!
//! The property is OUTCOME equality, not "a correct result twice": a chain
//! that ends in a loud kernel error must end in the SAME loud error every
//! run, and a chain that completes must complete with the same step count,
//! the same topology counts and the same volumes. Until 2026-09-27 an error
//! outcome was discarded as a "known kernel limitation" (with panics and
//! non-manifold results in the same bucket), so the test could neither see a
//! run-to-run flip between error and success nor a panic.

use proptest::prelude::*;
use test_harness::assay::strategies_v2::{execute_chain, strats_v2, GenerativeChainScenario};
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

/// Everything one run of a chain produces that a second run must reproduce.
#[derive(Debug, Clone, PartialEq)]
enum Outcome {
    Completed {
        completed_steps: usize,
        final_feature: String,
        topology: Option<(usize, usize, usize)>,
        /// Volumes are compared with a tolerance in `same_outcome`, not here.
        volumes: Vec<f64>,
    },
    Failed(String),
}

fn run_once(scenario: &GenerativeChainScenario) -> Outcome {
    match safe_execute_chain(scenario) {
        Ok(mut result) => {
            let topology = result.builder.topology_counts(&result.final_feature).ok();
            let mut volumes = result.step_volumes.clone();
            if let Ok(mesh) = result.builder.tessellate(&result.final_feature) {
                volumes.push(mesh_volume(&mesh));
            }
            Outcome::Completed {
                completed_steps: result.completed_steps,
                final_feature: result.final_feature,
                topology,
                volumes,
            }
        }
        Err(e) => Outcome::Failed(e),
    }
}

/// Replace every UUID (8-4-4-4-12 hex) in `text` with `<uuid>`.
///
/// Feature ids are minted fresh by every `ModelBuilder` run, and the engine's
/// error texts embed them (`rebuild.rs`: "Output key Main not found in
/// feature <uuid>"). Two runs that fail for the SAME reason therefore differ
/// in that identifier alone, which is not non-determinism of the chain —
/// measured 2026-10-05 on a persisted proptest seed: fresh UUIDs on each
/// replay, same missing output key both times. Everything else in the text
/// (the reason, the output key, face ids) still has to match.
fn normalize_ids(text: &str) -> String {
    let bytes = text.as_bytes();
    let is_hex = |b: u8| b.is_ascii_hexdigit();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < bytes.len() {
        // Groups of 8-4-4-4-12 hex digits separated by '-'.
        const GROUPS: [usize; 5] = [8, 4, 4, 4, 12];
        let mut j = i;
        let mut ok = true;
        for (g, &len) in GROUPS.iter().enumerate() {
            if j + len > bytes.len() || !bytes[j..j + len].iter().all(|&b| is_hex(b)) {
                ok = false;
                break;
            }
            j += len;
            if g < GROUPS.len() - 1 {
                if j >= bytes.len() || bytes[j] != b'-' {
                    ok = false;
                    break;
                }
                j += 1;
            }
        }
        // A UUID is bounded by non-hex characters (or the text's ends), so a
        // longer hex run is never half-matched.
        let bounded =
            ok && (i == 0 || !is_hex(bytes[i - 1])) && (j == bytes.len() || !is_hex(bytes[j]));
        if bounded {
            out.push_str("<uuid>");
            i = j;
        } else {
            // Advance by one char (UTF-8 aware).
            let ch = text[i..].chars().next().expect("in-bounds char");
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

#[test]
fn normalize_ids_replaces_only_whole_uuids() {
    let text = "Output key Main not found in feature c90cefcc-d6e5-4733-b923-977e2c799f3f; \
                FaceId(19) deadbeef stays";
    assert_eq!(
        normalize_ids(text),
        "Output key Main not found in feature <uuid>; FaceId(19) deadbeef stays"
    );
    // Two different ids normalize to the same text; a different reason does not.
    let a = normalize_ids("x 00000000-0000-0000-0000-000000000000 y");
    let b = normalize_ids("x ffffffff-ffff-ffff-ffff-ffffffffffff y");
    assert_eq!(a, b);
    assert_ne!(normalize_ids("reason A"), normalize_ids("reason B"));
}

/// Outcome equality with a tessellation-tolerance on volumes.
fn same_outcome(a: &Outcome, b: &Outcome) -> Result<(), String> {
    match (a, b) {
        (
            Outcome::Completed {
                completed_steps: sa,
                final_feature: fa,
                topology: ta,
                volumes: va,
            },
            Outcome::Completed {
                completed_steps: sb,
                final_feature: fb,
                topology: tb,
                volumes: vb,
            },
        ) => {
            if sa != sb || fa != fb {
                return Err(format!(
                    "chain length differs: {sa} steps ending at {fa} vs {sb} steps ending at {fb}"
                ));
            }
            if ta != tb {
                return Err(format!("topology counts differ: {ta:?} vs {tb:?}"));
            }
            if va.len() != vb.len() {
                return Err(format!(
                    "volume count differs: {} vs {}",
                    va.len(),
                    vb.len()
                ));
            }
            for (i, (x, y)) in va.iter().zip(vb).enumerate() {
                let tol = x.abs() * 1e-9 + 1e-12;
                if (x - y).abs() > tol {
                    return Err(format!("volume {i} differs: {x:.12} vs {y:.12}"));
                }
            }
            Ok(())
        }
        (Outcome::Failed(x), Outcome::Failed(y)) if normalize_ids(x) == normalize_ids(y) => Ok(()),
        (x, y) => Err(format!("outcome differs:\n  run0 = {x:?}\n  runN = {y:?}")),
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

    /// Chain determinism: run each scenario 3 times and require the SAME
    /// outcome — completed steps, topology counts, volumes, or the same
    /// error text — every time.
    #[test]
    fn chain_deterministic(
        scenario in strats_v2::generative_chain_scenario()
    ) {
        let first = run_once(&scenario);
        // A panic is a P9 violation regardless of determinism.
        if let Outcome::Failed(e) = &first {
            prop_assert!(!e.starts_with("panicked"), "chain panicked: {e}");
        }
        for run in 1..3 {
            let again = run_once(&scenario);
            if let Err(why) = same_outcome(&first, &again) {
                prop_assert!(false, "non-deterministic chain (run {run}): {why}");
            }
        }
    }
}
