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

use indirect_predicates_sidecar_rs::{
    init_fpu, lambda3d_lpi_interval, link_probe, IntervalNumber, AVAILABLE,
};

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

// =========================================================================
// PR-CR-IP2 — IntervalNumber + lambda3d_LPI_interval + FPU init
// =========================================================================

#[test]
fn interval_number_point_constructor() {
    let p = IntervalNumber::point(3.0);
    assert_eq!(p.inf, 3.0);
    assert_eq!(p.sup, 3.0);
    assert_eq!(p, IntervalNumber::new(3.0, 3.0));
}

#[test]
fn interval_number_new_constructor() {
    let i = IntervalNumber::new(1.0, 2.0);
    assert_eq!(i.inf, 1.0);
    assert_eq!(i.sup, 2.0);
    // No validation: inf > sup is allowed (upstream's concern).
    let inverted = IntervalNumber::new(5.0, 1.0);
    assert_eq!(inverted.inf, 5.0);
    assert_eq!(inverted.sup, 1.0);
}

#[test]
fn interval_number_copy_and_eq() {
    let a = IntervalNumber::new(1.0, 2.0);
    let b = a;
    assert_eq!(a, b);
    let c = IntervalNumber::new(1.0, 2.5);
    assert_ne!(a, c);
    // Copy semantics
    fn requires_copy<T: Copy>() {}
    requires_copy::<IntervalNumber>();
}

#[test]
fn init_fpu_is_callable_and_idempotent() {
    init_fpu();
    init_fpu();
    init_fpu();
    init_fpu();
    init_fpu();
    // No panic, no return value to check.
}

#[cfg(not(ip_unavailable))]
#[test]
fn lambda3d_lpi_non_degenerate_is_reliable() {
    init_fpu();
    // Line P=(1,2,3) → Q=(5,7,9). Plane z=0 through R/S/T.
    // The line crosses z=0 at parameter t = -0.5, so the
    // intersection is non-degenerate.
    let pt = |x: f64, y: f64, z: f64| {
        [
            IntervalNumber::point(x),
            IntervalNumber::point(y),
            IntervalNumber::point(z),
        ]
    };
    let result = lambda3d_lpi_interval(
        pt(1.0, 2.0, 3.0), // P
        pt(5.0, 7.0, 9.0), // Q
        pt(0.0, 0.0, 0.0), // R
        pt(1.0, 0.0, 0.0), // S
        pt(0.0, 1.0, 0.0), // T (plane z=0)
    );
    assert!(
        result.reliable,
        "non-degenerate line/plane should be reliable; got: {result:?}"
    );
    for lambda in [
        result.lambda_x,
        result.lambda_y,
        result.lambda_z,
        result.lambda_d,
    ] {
        assert!(
            !lambda.inf.is_nan() && !lambda.sup.is_nan(),
            "lambda components must be non-NaN; got {lambda:?}"
        );
    }
}

#[cfg(not(ip_unavailable))]
#[test]
fn lambda3d_lpi_coplanar_is_unreliable() {
    init_fpu();
    // Line P=(0,0,0) → Q=(1,1,0) lies in the plane z=0. The
    // denominator should straddle (or equal) zero — reliable: false.
    let pt = |x: f64, y: f64, z: f64| {
        [
            IntervalNumber::point(x),
            IntervalNumber::point(y),
            IntervalNumber::point(z),
        ]
    };
    let result = lambda3d_lpi_interval(
        pt(0.0, 0.0, 0.0), // P (in plane)
        pt(1.0, 1.0, 0.0), // Q (in plane)
        pt(0.0, 0.0, 0.0), // R
        pt(1.0, 0.0, 0.0), // S
        pt(0.0, 1.0, 0.0), // T (plane z=0)
    );
    assert!(
        !result.reliable,
        "line-in-plane should be unreliable; got: {result:?}"
    );
}

#[cfg(ip_unavailable)]
#[test]
fn lambda3d_lpi_stub_returns_zeros() {
    let pt = |x: f64, y: f64, z: f64| {
        [
            IntervalNumber::point(x),
            IntervalNumber::point(y),
            IntervalNumber::point(z),
        ]
    };
    let result = lambda3d_lpi_interval(
        pt(1.0, 2.0, 3.0),
        pt(5.0, 7.0, 9.0),
        pt(0.0, 0.0, 0.0),
        pt(1.0, 0.0, 0.0),
        pt(0.0, 1.0, 0.0),
    );
    assert!(!result.reliable, "stub must mark result unreliable");
    assert_eq!(result.lambda_x, IntervalNumber::new(0.0, 0.0));
    assert_eq!(result.lambda_y, IntervalNumber::new(0.0, 0.0));
    assert_eq!(result.lambda_z, IntervalNumber::new(0.0, 0.0));
    assert_eq!(result.lambda_d, IntervalNumber::new(0.0, 0.0));
}
