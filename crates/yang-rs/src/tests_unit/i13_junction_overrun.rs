//! I13 (spec `yang_441_trim_cdt_construction.md` §I13) — the rim×cut junction
//! terminal-overrun increment's pure mechanisms:
//!
//! * I13a — the `SurfaceChart::Cone` projection/lift pair (the gated
//!   constructor is pinned off-by-default in `stage4_project`'s own tests);
//! * I13b — the `conic_param` open-conic arm (delegating to KV16's
//!   `geom::hyperbola_param`) and the `conic_param_periodic` split that keeps
//!   angle-domain consumers away from unbounded parameters;
//! * the hyperbola arm of `conics_equal_up_to_normal_sign`.
//!
//! The I13c selector-arm certificate tests live next to the selector in
//! `stage4_fold_risk` (circle-parameterized — `conic_param` on circles is
//! ungated). R0003 is the increment's measured pin case.

use crate::stage4_correct::{conic_param, conic_param_periodic, conics_equal_up_to_normal_sign};
use crate::stage4_project::SurfaceChart;
use crate::{Curve, Surface, Vector3};
use cad_primitives::{Point2, Point3};

fn sample_hyperbola() -> (Curve, Point3, Vector3, Vector3, f64, f64) {
    let center = Point3::new(1.0, -2.0, 0.5);
    let normal = Vector3::new(0.0, 0.0, 1.0);
    let major = Vector3::new(1.0, 0.0, 0.0);
    let (a, b) = (2.0, 0.75);
    (
        Curve::Hyperbola {
            center,
            normal,
            major_axis: major,
            semi_transverse: a,
            semi_conjugate: b,
        },
        center,
        normal,
        major,
        a,
        b,
    )
}

/// A point of the enum's stated parameterization
/// `center + a·cosh t·major + b·sinh t·(normal × major)`.
fn hyperbola_point(t: f64) -> Point3 {
    let (_, c, _, _, a, b) = sample_hyperbola();
    // normal × major = +y for the sample frame.
    Point3::new(c.x() + a * t.cosh(), c.y() + b * t.sinh(), c.z())
}

#[test]
fn hyperbola_param_round_trips_the_enum_parameterization() {
    let (_, c, n, m, _, b) = sample_hyperbola();
    for &t in &[-3.0, -0.7, 0.0, 0.4, 2.5] {
        let got = crate::geom::hyperbola_param(hyperbola_point(t), c, n, m, b);
        assert!(
            (got - t).abs() < 1e-12,
            "t={t}: recovered {got} (delta {:.3e})",
            (got - t).abs()
        );
    }
}

#[test]
fn hyperbola_param_is_strictly_monotone_along_the_branch() {
    let (_, c, n, m, _, b) = sample_hyperbola();
    let ts: Vec<f64> = (-10..=10)
        .map(|k| crate::geom::hyperbola_param(hyperbola_point(f64::from(k) * 0.37), c, n, m, b))
        .collect();
    assert!(
        ts.windows(2).all(|w| w[1] > w[0]),
        "parameters must ascend with the branch: {ts:?}"
    );
}

#[test]
fn conic_param_covers_the_hyperbola_by_default() {
    // FLIPPED 2026-08-25: the open-conic arm is always-on;
    // `YANG_441_OPEN_CONIC_PARAM=0|off` restores the pre-I13b `None`.
    let (h, ..) = sample_hyperbola();
    if crate::stage4_correct::open_conic_param_enabled() {
        let t = conic_param(&h, hyperbola_point(0.3)).expect("open-conic param is on");
        assert!((t - 0.3).abs() < 1e-12);
    } else {
        assert_eq!(conic_param(&h, hyperbola_point(0.3)), None);
    }
}

#[test]
fn conic_param_periodic_splits_the_angle_domain_from_the_open_one() {
    let (h, ..) = sample_hyperbola();
    assert!(!conic_param_periodic(&h));
    assert!(conic_param_periodic(&Curve::Circle {
        center: Point3::new(0.0, 0.0, 0.0),
        normal: Vector3::new(0.0, 0.0, 1.0),
        radius: 1.0,
    }));
    assert!(conic_param_periodic(&Curve::Ellipse {
        center: Point3::new(0.0, 0.0, 0.0),
        normal: Vector3::new(0.0, 0.0, 1.0),
        major_axis: Vector3::new(1.0, 0.0, 0.0),
        major_radius: 2.0,
        minor_radius: 1.0,
    }));
    assert!(!conic_param_periodic(&Curve::LineSegment));
}

#[test]
fn conics_equal_up_to_normal_sign_covers_the_hyperbola() {
    let (h, c, n, m, a, b) = sample_hyperbola();
    let flipped = Curve::Hyperbola {
        center: c,
        normal: Vector3::new(-n.as_array()[0], -n.as_array()[1], -n.as_array()[2]),
        major_axis: m,
        semi_transverse: a,
        semi_conjugate: b,
    };
    let other = Curve::Hyperbola {
        center: c,
        normal: n,
        major_axis: m,
        semi_transverse: a,
        semi_conjugate: b * 2.0,
    };
    assert!(conics_equal_up_to_normal_sign(&h, &h));
    assert!(conics_equal_up_to_normal_sign(&h, &flipped));
    assert!(!conics_equal_up_to_normal_sign(&h, &other));
}

// ---- I13a: the cone chart's projection/lift pair -------------------------

fn sample_cone_chart() -> SurfaceChart {
    // Deliberately skewed axis so nothing is axis-aligned by accident.
    let axis = crate::normalize3([0.2, -0.3, 0.93]);
    let e1 = crate::normalize3([axis[1], -axis[0], 0.0]);
    let e2 = [
        axis[1] * e1[2] - axis[2] * e1[1],
        axis[2] * e1[0] - axis[0] * e1[2],
        axis[0] * e1[1] - axis[1] * e1[0],
    ];
    SurfaceChart::Cone {
        apex: [3.0, 1.0, -2.0],
        axis,
        e1,
        e2,
        tan_half: 0.35,
    }
}

#[test]
fn cone_chart_lift_then_project_round_trips() {
    let chart = sample_cone_chart();
    for &(theta, z) in &[(0.0, 1.0), (1.2, 0.4), (-2.9, 7.5), (3.1, 0.001)] {
        let p = chart.lift(Point2::new(theta, z));
        let uv = chart.project(p);
        let dt = (uv.x() - theta).abs();
        let dt = dt.min((dt - std::f64::consts::TAU).abs()); // branch-safe
        assert!(dt < 1e-12, "theta {theta}: got {}", uv.x());
        assert!(
            (uv.y() - z).abs() < 1e-12 * (1.0 + z.abs()),
            "z {z}: got {}",
            uv.y()
        );
    }
}

#[test]
fn cone_chart_lift_lands_on_the_cone() {
    let chart = sample_cone_chart();
    let SurfaceChart::Cone {
        apex,
        axis,
        tan_half,
        ..
    } = chart
    else {
        unreachable!()
    };
    for &(theta, z) in &[(0.7, 2.0), (-1.3, 0.25)] {
        let p = chart.lift(Point2::new(theta, z)).as_array();
        let w = [p[0] - apex[0], p[1] - apex[1], p[2] - apex[2]];
        let h = w[0] * axis[0] + w[1] * axis[1] + w[2] * axis[2];
        let rad = [w[0] - h * axis[0], w[1] - h * axis[1], w[2] - h * axis[2]];
        let r = (rad[0] * rad[0] + rad[1] * rad[1] + rad[2] * rad[2]).sqrt();
        assert!(
            (h - z).abs() < 1e-12 * (1.0 + z.abs()),
            "station {z}: got {h}"
        );
        assert!(
            (r - z * tan_half).abs() < 1e-12 * (1.0 + r),
            "radius at {z}: got {r}, want {}",
            z * tan_half
        );
    }
}

#[test]
fn cone_chartability_follows_the_flipped_gate() {
    // FLIPPED 2026-08-25: cone chartability is always-on;
    // `YANG_441_CONE_CHART=0|off` restores the I2a Plane|Cylinder scope.
    let cone = Surface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: 0.3,
    };
    let on = crate::stage4_project::cone_chart_enabled();
    assert_eq!(SurfaceChart::supports(&cone), on);
    assert_eq!(SurfaceChart::new(cone).is_some(), on);
    // The consolidated pre-filter keeps the I2a scope for the others.
    assert!(SurfaceChart::supports(&Surface::Plane {
        normal: Vector3::new(0.0, 0.0, 1.0),
        d: 0.0,
    }));
    assert!(SurfaceChart::supports(&Surface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 1.0,
    }));
    assert!(!SurfaceChart::supports(&Surface::Sphere {
        center: Point3::new(0.0, 0.0, 0.0),
        radius: 1.0,
    }));
}
