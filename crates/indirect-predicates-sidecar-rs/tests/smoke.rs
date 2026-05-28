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
    init_fpu, lambda3d_lpi_exact, lambda3d_lpi_interval, lambda3d_tpi_exact,
    lambda3d_tpi_interval, link_probe, IntervalNumber, LpiExactResult, TpiExactResult,
    TpiIntervalResult, AVAILABLE,
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

// =========================================================================
// PR-CR-IP3 — lambda3d_lpi_exact (Shewchuk expansion arithmetic)
// =========================================================================

#[test]
fn lpi_exact_result_default_empty() {
    let r = LpiExactResult::default();
    assert!(r.lambda_x.is_empty());
    assert!(r.lambda_y.is_empty());
    assert!(r.lambda_z.is_empty());
    assert!(r.lambda_d.is_empty());
}

#[test]
fn lpi_exact_result_clone_and_eq() {
    let a = LpiExactResult {
        lambda_x: vec![1.0, 2.0],
        lambda_y: vec![3.0],
        lambda_z: vec![],
        lambda_d: vec![4.0],
    };
    let b = a.clone();
    assert_eq!(a, b);
    let c = LpiExactResult::default();
    assert_ne!(a, c);
    // Debug formatting smoke check
    let _ = format!("{a:?}");
}

#[cfg(not(ip_unavailable))]
#[test]
fn lambda3d_lpi_exact_non_degenerate_non_empty() {
    let result = lambda3d_lpi_exact(
        [1.0, 2.0, 3.0], // P
        [5.0, 7.0, 9.0], // Q
        [0.0, 0.0, 0.0], // R
        [1.0, 0.0, 0.0], // S
        [0.0, 1.0, 0.0], // T (plane z=0)
    );
    for (name, lambda) in [
        ("lambda_x", &result.lambda_x),
        ("lambda_y", &result.lambda_y),
        ("lambda_z", &result.lambda_z),
        ("lambda_d", &result.lambda_d),
    ] {
        assert!(
            !lambda.is_empty(),
            "{name} should be non-empty for non-degenerate input; got {lambda:?}"
        );
        for &entry in lambda {
            assert!(
                entry.is_finite(),
                "{name} contains non-finite entry: {entry}"
            );
        }
    }
}

#[cfg(not(ip_unavailable))]
#[test]
fn lambda3d_lpi_exact_coplanar_d_approximately_zero() {
    // Line P=(0,0,0) → Q=(1,1,0) lies in plane z=0 (R/S/T).
    let result = lambda3d_lpi_exact(
        [0.0, 0.0, 0.0],
        [1.0, 1.0, 0.0],
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
    );
    let d_sum: f64 = result.lambda_d.iter().sum();
    assert!(
        d_sum.abs() < 1e-12,
        "coplanar line should produce lambda_d summing to ≈ 0; got {d_sum} (expansion: {:?})",
        result.lambda_d
    );
}

#[cfg(not(ip_unavailable))]
#[test]
fn lambda3d_lpi_exact_agrees_with_interval() {
    let p = [1.0, 2.0, 3.0];
    let q = [5.0, 7.0, 9.0];
    let r = [0.0, 0.0, 0.0];
    let s = [1.0, 0.0, 0.0];
    let t = [0.0, 1.0, 0.0];
    let exact = lambda3d_lpi_exact(p, q, r, s, t);
    let pt = |x: f64, y: f64, z: f64| {
        [
            IntervalNumber::point(x),
            IntervalNumber::point(y),
            IntervalNumber::point(z),
        ]
    };
    let interval = lambda3d_lpi_interval(
        pt(p[0], p[1], p[2]),
        pt(q[0], q[1], q[2]),
        pt(r[0], r[1], r[2]),
        pt(s[0], s[1], s[2]),
        pt(t[0], t[1], t[2]),
    );
    assert!(interval.reliable, "interval should be reliable here");
    let d_sum: f64 = exact.lambda_d.iter().sum();
    assert!(
        d_sum >= interval.lambda_d.inf && d_sum <= interval.lambda_d.sup,
        "exact lambda_d sum {d_sum} should lie within interval [{}, {}]",
        interval.lambda_d.inf,
        interval.lambda_d.sup
    );
}

#[cfg(ip_unavailable)]
#[test]
fn lambda3d_lpi_exact_stub_returns_empty_vecs() {
    let result = lambda3d_lpi_exact(
        [1.0, 2.0, 3.0],
        [5.0, 7.0, 9.0],
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
    );
    assert!(result.lambda_x.is_empty());
    assert!(result.lambda_y.is_empty());
    assert!(result.lambda_z.is_empty());
    assert!(result.lambda_d.is_empty());
}

// =========================================================================
// PR-CR-IP4 — lambda3d_tpi_interval + lambda3d_tpi_exact
// =========================================================================

#[test]
fn tpi_interval_result_construct_and_eq() {
    let r = TpiIntervalResult {
        lambda_x: IntervalNumber::new(1.0, 2.0),
        lambda_y: IntervalNumber::new(3.0, 4.0),
        lambda_z: IntervalNumber::new(5.0, 6.0),
        lambda_d: IntervalNumber::new(7.0, 8.0),
        reliable: true,
    };
    let s = r;
    assert_eq!(r, s);
    let t = TpiIntervalResult { reliable: false, ..r };
    assert_ne!(r, t);
    fn requires_copy<T: Copy>() {}
    requires_copy::<TpiIntervalResult>();
}

#[test]
fn tpi_exact_result_default_empty() {
    let r = TpiExactResult::default();
    assert!(r.lambda_x.is_empty());
    assert!(r.lambda_y.is_empty());
    assert!(r.lambda_z.is_empty());
    assert!(r.lambda_d.is_empty());
}

#[test]
fn tpi_exact_result_clone_and_eq() {
    let a = TpiExactResult {
        lambda_x: vec![1.0, 2.0],
        lambda_y: vec![3.0],
        lambda_z: vec![],
        lambda_d: vec![4.0],
    };
    let b = a.clone();
    assert_eq!(a, b);
    let c = TpiExactResult::default();
    assert_ne!(a, c);
    let _ = format!("{a:?}");
}

/// Helper: three coordinate planes (x=0, y=0, z=0) as IntervalNumber
/// triangles. Their three planes intersect at the origin.
#[cfg(not(ip_unavailable))]
fn orthogonal_planes_interval() -> (
    [[IntervalNumber; 3]; 3],
    [[IntervalNumber; 3]; 3],
    [[IntervalNumber; 3]; 3],
) {
    let pt = |x: f64, y: f64, z: f64| {
        [
            IntervalNumber::point(x),
            IntervalNumber::point(y),
            IntervalNumber::point(z),
        ]
    };
    // x=0 plane via three vertices on it
    let v = [pt(0.0, 0.0, 0.0), pt(0.0, 1.0, 0.0), pt(0.0, 0.0, 1.0)];
    // y=0 plane
    let w = [pt(0.0, 0.0, 0.0), pt(1.0, 0.0, 0.0), pt(0.0, 0.0, 1.0)];
    // z=0 plane
    let u = [pt(0.0, 0.0, 0.0), pt(1.0, 0.0, 0.0), pt(0.0, 1.0, 0.0)];
    (v, w, u)
}

/// Helper: same orthogonal-planes geometry as exact doubles.
#[cfg(not(ip_unavailable))]
fn orthogonal_planes_exact() -> ([[f64; 3]; 3], [[f64; 3]; 3], [[f64; 3]; 3]) {
    let v = [[0.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    let w = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]];
    let u = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
    (v, w, u)
}

#[cfg(not(ip_unavailable))]
#[test]
fn lambda3d_tpi_interval_orthogonal_planes_reliable() {
    init_fpu();
    let (v, w, u) = orthogonal_planes_interval();
    let result = lambda3d_tpi_interval(v, w, u);
    assert!(
        result.reliable,
        "three orthogonal coordinate planes should be reliable; got {result:?}"
    );
    for lambda in [result.lambda_x, result.lambda_y, result.lambda_z, result.lambda_d] {
        assert!(
            !lambda.inf.is_nan() && !lambda.sup.is_nan(),
            "lambda components must be non-NaN; got {lambda:?}"
        );
    }
}

#[cfg(not(ip_unavailable))]
#[test]
fn lambda3d_tpi_interval_parallel_planes_unreliable() {
    init_fpu();
    let pt = |x: f64, y: f64, z: f64| {
        [
            IntervalNumber::point(x),
            IntervalNumber::point(y),
            IntervalNumber::point(z),
        ]
    };
    // Two parallel z=0 planes (offset in xy to avoid coincidence in
    // the trivial-degenerate sense) + one parallel z=1 plane.
    // No unique intersection point exists.
    let v = [pt(0.0, 0.0, 0.0), pt(1.0, 0.0, 0.0), pt(0.0, 1.0, 0.0)]; // z=0
    let w = [pt(0.0, 0.0, 1.0), pt(1.0, 0.0, 1.0), pt(0.0, 1.0, 1.0)]; // z=1
    let u = [pt(2.0, 2.0, 0.0), pt(3.0, 2.0, 0.0), pt(2.0, 3.0, 0.0)]; // z=0 (different triangle, same plane)
    let result = lambda3d_tpi_interval(v, w, u);
    assert!(
        !result.reliable,
        "parallel planes should not be reliable; got {result:?}"
    );
}

#[cfg(not(ip_unavailable))]
#[test]
fn lambda3d_tpi_exact_orthogonal_non_empty() {
    let (v, w, u) = orthogonal_planes_exact();
    let result = lambda3d_tpi_exact(v, w, u);
    for (name, lambda) in [
        ("lambda_x", &result.lambda_x),
        ("lambda_y", &result.lambda_y),
        ("lambda_z", &result.lambda_z),
        ("lambda_d", &result.lambda_d),
    ] {
        assert!(
            !lambda.is_empty(),
            "{name} should be non-empty for orthogonal planes; got {lambda:?}"
        );
        for &entry in lambda {
            assert!(
                entry.is_finite(),
                "{name} contains non-finite entry: {entry}"
            );
        }
    }
}

#[cfg(not(ip_unavailable))]
#[test]
fn lambda3d_tpi_exact_parallel_d_approximately_zero() {
    // Same parallel-plane geometry as the interval test.
    let v = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
    let w = [[0.0, 0.0, 1.0], [1.0, 0.0, 1.0], [0.0, 1.0, 1.0]];
    let u = [[2.0, 2.0, 0.0], [3.0, 2.0, 0.0], [2.0, 3.0, 0.0]];
    let result = lambda3d_tpi_exact(v, w, u);
    let d_sum: f64 = result.lambda_d.iter().sum();
    assert!(
        d_sum.abs() < 1e-12,
        "parallel planes should produce sum(lambda_d) ≈ 0; got {d_sum} (expansion: {:?})",
        result.lambda_d
    );
}

#[cfg(not(ip_unavailable))]
#[test]
fn lambda3d_tpi_exact_agrees_with_interval() {
    init_fpu();
    let (vi, wi, ui) = orthogonal_planes_interval();
    let (ve, we, ue) = orthogonal_planes_exact();
    let exact = lambda3d_tpi_exact(ve, we, ue);
    let interval = lambda3d_tpi_interval(vi, wi, ui);
    assert!(
        interval.reliable,
        "interval should be reliable here; got {interval:?}"
    );
    let d_sum: f64 = exact.lambda_d.iter().sum();
    assert!(
        d_sum >= interval.lambda_d.inf && d_sum <= interval.lambda_d.sup,
        "exact lambda_d sum {d_sum} should lie within interval [{}, {}]; expansion: {:?}",
        interval.lambda_d.inf,
        interval.lambda_d.sup,
        exact.lambda_d
    );
}
