//! Smoke tests for the FFI build chain.
//!
//! Tests run in two states depending on whether
//! `Indirect_Predicates` was found at build time:
//! - **Available** (`cfg!(not(ip_unavailable))`): real shim linked,
//!   `link_probe()` returns +1.
//! - **Unavailable** (`cfg!(ip_unavailable)`): stub linked,
//!   `link_probe()` returns -2.
//!
//! All tests pass in either state.

use indirect_predicates_sidecar_rs::{link_probe, AVAILABLE};

#[cfg(not(ip_unavailable))]
#[test]
fn link_probe_returns_one_when_available() {
    assert_eq!(
        link_probe(),
        1,
        "expected +1 (sign of dot((1,0),(0,1)) at (0,0))"
    );
}

#[cfg(ip_unavailable)]
#[test]
fn link_probe_returns_sentinel_when_unavailable() {
    assert_eq!(link_probe(), -2, "expected -2 sentinel from stub");
}

#[test]
fn available_flag_matches_cfg() {
    let expected_when_available = cfg!(not(ip_unavailable));
    assert_eq!(
        AVAILABLE, expected_when_available,
        "AVAILABLE must agree with cfg(ip_unavailable)"
    );
}

#[test]
fn link_probe_is_deterministic() {
    let first = link_probe();
    for _ in 0..1000 {
        assert_eq!(
            link_probe(),
            first,
            "link_probe must be deterministic across repeated calls"
        );
    }
}

#[test]
fn link_probe_does_not_panic() {
    let result = std::panic::catch_unwind(link_probe);
    assert!(result.is_ok(), "link_probe must never panic");
}

#[test]
fn description_documents_wasm_incompatibility() {
    // Guards the Cargo.toml description from accidentally dropping
    // the WASM-incompat marker (a load-bearing piece of
    // documentation for downstream consumers).
    let description = env!("CARGO_PKG_DESCRIPTION");
    assert!(
        description.contains("NOT WASM-compatible"),
        "crate description must document WASM incompatibility; got: {description:?}"
    );
}
