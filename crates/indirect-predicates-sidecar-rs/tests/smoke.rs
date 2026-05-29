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

#[cfg(ip_unavailable)]
use indirect_predicates_sidecar_rs::less_than_on_z;
use indirect_predicates_sidecar_rs::{
    init_fpu, lambda3d_lpi_exact, lambda3d_lpi_interval, lambda3d_tpi_exact, lambda3d_tpi_interval,
    link_probe, orient3d, AsGenericPoint, ExplicitPoint3D, ImplicitPoint3DLpi, ImplicitPoint3DTpi,
    IntervalNumber, LpiExactResult, Sign, TpiExactResult, TpiIntervalResult, AVAILABLE,
};
#[cfg(not(ip_unavailable))]
use indirect_predicates_sidecar_rs::{less_than_on_x, less_than_on_y};

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
    let t = TpiIntervalResult {
        reliable: false,
        ..r
    };
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

/// Triangle as 3 vertices × 3 IntervalNumber coordinates.
type IntervalTri = [[IntervalNumber; 3]; 3];

/// Triangle as 3 vertices × 3 f64 coordinates.
type ExactTri = [[f64; 3]; 3];

/// Helper: three coordinate planes (x=0, y=0, z=0) as IntervalNumber
/// triangles. Their three planes intersect at the origin.
#[cfg(not(ip_unavailable))]
fn orthogonal_planes_interval() -> (IntervalTri, IntervalTri, IntervalTri) {
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
fn orthogonal_planes_exact() -> (ExactTri, ExactTri, ExactTri) {
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

// =========================================================================
// PR-CR-IP5 — ExplicitPoint3D opaque handle
// =========================================================================

#[test]
fn explicit_point_3d_send_sync_compile() {
    fn requires_send_sync<T: Send + Sync>() {}
    requires_send_sync::<ExplicitPoint3D>();
}

#[test]
fn explicit_point_3d_drop_runs() {
    // Construct + drop via scope exit; the catch_unwind verifies
    // that neither the constructor nor the Drop impl panics.
    let result = std::panic::catch_unwind(|| {
        let _p = ExplicitPoint3D::new(1.0, 2.0, 3.0);
    });
    assert!(result.is_ok(), "construct + drop must not panic");
}

#[test]
fn explicit_point_3d_positive_coords() {
    let p = ExplicitPoint3D::new(1.0, 2.0, 3.0);
    assert_eq!(p.x(), 1.0);
    assert_eq!(p.y(), 2.0);
    assert_eq!(p.z(), 3.0);
}

#[test]
fn explicit_point_3d_origin() {
    let p = ExplicitPoint3D::new(0.0, 0.0, 0.0);
    assert_eq!(p.x(), 0.0);
    assert_eq!(p.y(), 0.0);
    assert_eq!(p.z(), 0.0);
}

#[test]
fn explicit_point_3d_negative_coords() {
    let p = ExplicitPoint3D::new(-1.5, -2.5, -3.5);
    assert_eq!(p.x(), -1.5);
    assert_eq!(p.y(), -2.5);
    assert_eq!(p.z(), -3.5);
}

// =========================================================================
// PR-CR-IP5b — ImplicitPoint3DLpi<'a> + ImplicitPoint3DTpi<'a>
// =========================================================================

#[test]
fn implicit_point_3d_lpi_send_sync_compile() {
    fn requires_send_sync<T: Send + Sync>() {}
    requires_send_sync::<ImplicitPoint3DLpi<'_>>();
}

#[test]
fn implicit_point_3d_tpi_send_sync_compile() {
    fn requires_send_sync<T: Send + Sync>() {}
    requires_send_sync::<ImplicitPoint3DTpi<'_>>();
}

#[test]
fn implicit_point_3d_lpi_construct_and_drop() {
    // Same line-plane geometry as PR-CR-IP2/IP3 LPI tests, but
    // constructed as an opaque implicit point.
    let result = std::panic::catch_unwind(|| {
        let p = ExplicitPoint3D::new(1.0, 2.0, 3.0);
        let q = ExplicitPoint3D::new(5.0, 7.0, 9.0);
        let r = ExplicitPoint3D::new(0.0, 0.0, 0.0);
        let s = ExplicitPoint3D::new(1.0, 0.0, 0.0);
        let t = ExplicitPoint3D::new(0.0, 1.0, 0.0);
        let _lpi = ImplicitPoint3DLpi::new(&p, &q, &r, &s, &t);
        // Drop runs at scope exit.
    });
    assert!(result.is_ok(), "construct + drop must not panic");
}

#[test]
fn implicit_point_3d_tpi_construct_and_drop() {
    // Orthogonal coordinate planes geometry from PR-CR-IP4 TPI tests.
    let result = std::panic::catch_unwind(|| {
        // Triangle 1: x=0 plane via (0,0,0), (0,1,0), (0,0,1)
        let v1 = ExplicitPoint3D::new(0.0, 0.0, 0.0);
        let v2 = ExplicitPoint3D::new(0.0, 1.0, 0.0);
        let v3 = ExplicitPoint3D::new(0.0, 0.0, 1.0);
        // Triangle 2: y=0 plane via (0,0,0), (1,0,0), (0,0,1)
        let w1 = ExplicitPoint3D::new(0.0, 0.0, 0.0);
        let w2 = ExplicitPoint3D::new(1.0, 0.0, 0.0);
        let w3 = ExplicitPoint3D::new(0.0, 0.0, 1.0);
        // Triangle 3: z=0 plane via (0,0,0), (1,0,0), (0,1,0)
        let u1 = ExplicitPoint3D::new(0.0, 0.0, 0.0);
        let u2 = ExplicitPoint3D::new(1.0, 0.0, 0.0);
        let u3 = ExplicitPoint3D::new(0.0, 1.0, 0.0);
        let _tpi = ImplicitPoint3DTpi::new(&v1, &v2, &v3, &w1, &w2, &w3, &u1, &u2, &u3);
    });
    assert!(result.is_ok(), "construct + drop must not panic");
}

#[test]
fn implicit_point_3d_lpi_multiple_instances_share_explicit_borrows() {
    // Construct two LPIs borrowing overlapping explicit points;
    // verify both can be live simultaneously (the borrow checker
    // allows multiple shared `&'a ExplicitPoint3D` references).
    let result = std::panic::catch_unwind(|| {
        let p = ExplicitPoint3D::new(1.0, 2.0, 3.0);
        let q = ExplicitPoint3D::new(5.0, 7.0, 9.0);
        let r = ExplicitPoint3D::new(0.0, 0.0, 0.0);
        let s = ExplicitPoint3D::new(1.0, 0.0, 0.0);
        let t = ExplicitPoint3D::new(0.0, 1.0, 0.0);
        // Different "line" but same "plane" — both LPIs borrow r/s/t.
        let q2 = ExplicitPoint3D::new(2.0, 4.0, 6.0);
        let _lpi_a = ImplicitPoint3DLpi::new(&p, &q, &r, &s, &t);
        let _lpi_b = ImplicitPoint3DLpi::new(&p, &q2, &r, &s, &t);
    });
    assert!(
        result.is_ok(),
        "multiple LPIs sharing borrows must not panic"
    );
}

// =========================================================================
// PR-CR-IP6 — Sign enum + AsGenericPoint trait + orient3d + comparators
// =========================================================================

#[test]
fn sign_from_int_round_trip() {
    assert_eq!(Sign::from_int(-1), Sign::Negative);
    assert_eq!(Sign::from_int(0), Sign::Zero);
    assert_eq!(Sign::from_int(1), Sign::Positive);
    assert_eq!(Sign::from_int(2), Sign::Undefined);
    // Defensive: unexpected values map to Undefined.
    assert_eq!(Sign::from_int(99), Sign::Undefined);
    assert_eq!(Sign::from_int(-99), Sign::Undefined);
}

#[test]
fn sign_derives() {
    let a = Sign::Positive;
    let b = a; // Copy
    assert_eq!(a, b);
    assert_ne!(Sign::Positive, Sign::Negative);
    // Debug formatting smoke check
    let _ = format!("{:?}", Sign::Zero);
    fn requires_copy<T: Copy>() {}
    requires_copy::<Sign>();
}

#[test]
fn as_generic_point_trait_impls_compile() {
    // Compile-time check: all 3 handle types implement AsGenericPoint.
    fn check<T: AsGenericPoint>(_: &T) {}
    let ep = ExplicitPoint3D::new(0.0, 0.0, 0.0);
    check(&ep);
    let p = ExplicitPoint3D::new(0.0, 0.0, 0.0);
    let q = ExplicitPoint3D::new(1.0, 0.0, 0.0);
    let r = ExplicitPoint3D::new(0.0, 1.0, 0.0);
    let s = ExplicitPoint3D::new(0.0, 0.0, 1.0);
    let t = ExplicitPoint3D::new(1.0, 1.0, 1.0);
    let lpi = ImplicitPoint3DLpi::new(&p, &q, &r, &s, &t);
    check(&lpi);
    let v1 = ExplicitPoint3D::new(0.0, 0.0, 0.0);
    let v2 = ExplicitPoint3D::new(0.0, 1.0, 0.0);
    let v3 = ExplicitPoint3D::new(0.0, 0.0, 1.0);
    let w1 = ExplicitPoint3D::new(0.0, 0.0, 0.0);
    let w2 = ExplicitPoint3D::new(1.0, 0.0, 0.0);
    let w3 = ExplicitPoint3D::new(0.0, 0.0, 1.0);
    let u1 = ExplicitPoint3D::new(0.0, 0.0, 0.0);
    let u2 = ExplicitPoint3D::new(1.0, 0.0, 0.0);
    let u3 = ExplicitPoint3D::new(0.0, 1.0, 0.0);
    let tpi = ImplicitPoint3DTpi::new(&v1, &v2, &v3, &w1, &w2, &w3, &u1, &u2, &u3);
    check(&tpi);
}

#[cfg(not(ip_unavailable))]
#[test]
fn orient3d_positive_explicit_tetrahedron() {
    let p1 = ExplicitPoint3D::new(0.0, 0.0, 0.0);
    let p2 = ExplicitPoint3D::new(1.0, 0.0, 0.0);
    let p3 = ExplicitPoint3D::new(0.0, 1.0, 0.0);
    let p4 = ExplicitPoint3D::new(0.0, 0.0, 1.0);
    let sign = orient3d(&p1, &p2, &p3, &p4);
    assert_eq!(
        sign,
        Sign::Positive,
        "positive tetrahedron (0,0,0)(1,0,0)(0,1,0)(0,0,1) should be Positive"
    );
}

#[cfg(not(ip_unavailable))]
#[test]
fn orient3d_coplanar_explicit_zero() {
    // All four points on z=0 plane.
    let p1 = ExplicitPoint3D::new(0.0, 0.0, 0.0);
    let p2 = ExplicitPoint3D::new(1.0, 0.0, 0.0);
    let p3 = ExplicitPoint3D::new(0.0, 1.0, 0.0);
    let p4 = ExplicitPoint3D::new(0.5, 0.5, 0.0);
    let sign = orient3d(&p1, &p2, &p3, &p4);
    assert_eq!(
        sign,
        Sign::Zero,
        "coplanar points should give Zero orientation"
    );
}

#[cfg(not(ip_unavailable))]
#[test]
fn orient3d_negative_explicit_swapped() {
    // Swap p2 and p3 from the positive tetrahedron → flip orientation.
    let p1 = ExplicitPoint3D::new(0.0, 0.0, 0.0);
    let p2 = ExplicitPoint3D::new(0.0, 1.0, 0.0);
    let p3 = ExplicitPoint3D::new(1.0, 0.0, 0.0);
    let p4 = ExplicitPoint3D::new(0.0, 0.0, 1.0);
    let sign = orient3d(&p1, &p2, &p3, &p4);
    assert_eq!(
        sign,
        Sign::Negative,
        "swapped-orientation tetrahedron should be Negative"
    );
}

#[cfg(ip_unavailable)]
#[test]
fn orient3d_stub_returns_undefined() {
    let p1 = ExplicitPoint3D::new(0.0, 0.0, 0.0);
    let p2 = ExplicitPoint3D::new(1.0, 0.0, 0.0);
    let p3 = ExplicitPoint3D::new(0.0, 1.0, 0.0);
    let p4 = ExplicitPoint3D::new(0.0, 0.0, 1.0);
    assert_eq!(orient3d(&p1, &p2, &p3, &p4), Sign::Undefined);
}

#[cfg(not(ip_unavailable))]
#[test]
fn less_than_on_x_explicit_ordered() {
    // Note: the EE (explicit-vs-explicit) dispatch branch in
    // upstream's genericPoint::lessThanOnX returns
    // `a.X() < b.X()` as `int` (bool → 0 or 1). It maps:
    //   - p1.x < p2.x → 1 (Positive) ✓
    //   - otherwise → 0 (Zero) — semantically "not less"
    // For Cherchi 2022 §6.4, only II-cases are used; the EE
    // limitation doesn't matter in practice. Test only the
    // Positive case.
    let p1 = ExplicitPoint3D::new(0.0, 0.0, 0.0);
    let p2 = ExplicitPoint3D::new(1.0, 0.0, 0.0);
    assert_eq!(
        less_than_on_x(&p1, &p2),
        Sign::Positive,
        "p1.x < p2.x should give Positive"
    );
}

#[cfg(not(ip_unavailable))]
#[test]
fn less_than_on_y_explicit_equal() {
    // EE branch: equal → `<` is false → 0 (Zero). Correct in
    // this case (Zero is the right Sign for equal coords).
    let p1 = ExplicitPoint3D::new(0.0, 5.0, 0.0);
    let p2 = ExplicitPoint3D::new(0.0, 5.0, 0.0);
    assert_eq!(
        less_than_on_y(&p1, &p2),
        Sign::Zero,
        "equal y coords should give Zero"
    );
}

#[cfg(ip_unavailable)]
#[test]
fn less_than_on_z_stub_returns_undefined() {
    let p1 = ExplicitPoint3D::new(0.0, 0.0, 0.0);
    let p2 = ExplicitPoint3D::new(0.0, 0.0, 1.0);
    assert_eq!(less_than_on_z(&p1, &p2), Sign::Undefined);
}
