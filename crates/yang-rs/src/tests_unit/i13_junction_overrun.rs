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
    let (h, ..) = sample_hyperbola();
    let t = conic_param(&h, hyperbola_point(0.3)).expect("open-conic param is on");
    assert!((t - 0.3).abs() < 1e-12);
}

#[test]
fn conic_param_and_reversal_cover_parabola_and_hyperbola() {
    use crate::stage4_correct::{
        conic_param_deltas, conic_param_reversed, mixed_cycle_shared_conic,
    };
    use std::collections::BTreeMap;

    let (h, ..) = sample_hyperbola();
    assert!(!conic_param_reversed(
        &h,
        hyperbola_point(-0.5),
        hyperbola_point(0.2),
        hyperbola_point(0.9),
    ));
    assert!(conic_param_reversed(
        &h,
        hyperbola_point(-0.5),
        hyperbola_point(0.9),
        hyperbola_point(0.2),
    ));
    let (d1, d2) = conic_param_deltas(
        &h,
        hyperbola_point(-0.5),
        hyperbola_point(0.9),
        hyperbola_point(0.2),
    )
    .expect("hyperbola param deltas");
    assert!(d1 > 0.0 && d2 < 0.0);

    let vertex = Point3::new(0.5, -1.0, 2.0);
    let normal = Vector3::new(0.0, 0.0, 1.0);
    let axis_dir = Vector3::new(1.0, 0.0, 0.0);
    let focal = 0.8;
    let para = Curve::Parabola {
        vertex,
        normal,
        axis_dir,
        focal_length: focal,
    };
    let para_flip = Curve::Parabola {
        vertex,
        normal: Vector3::new(0.0, 0.0, -1.0),
        axis_dir,
        focal_length: focal,
    };
    let p_at = |t: f64| crate::geom::parabola_point(vertex, normal, axis_dir, focal, t);
    for &t in &[-4.5, -0.3, 0.0, 1.2, 6.0] {
        let got = conic_param(&para, p_at(t)).expect("parabola param");
        assert!((got - t).abs() < 1e-12, "t={t}: got {got}");
    }
    assert!(!conic_param_periodic(&para));
    assert!(conics_equal_up_to_normal_sign(&para, &para_flip));
    assert!(!conic_param_reversed(
        &para,
        p_at(-1.0),
        p_at(0.5),
        p_at(2.0)
    ));
    assert!(conic_param_reversed(
        &para,
        p_at(-1.0),
        p_at(2.0),
        p_at(0.5)
    ));

    let mut curves: BTreeMap<(u32, u32), Curve> = BTreeMap::new();
    curves.insert((1, 2), para);
    curves.insert((2, 3), para_flip);
    assert_eq!(
        mixed_cycle_shared_conic(&curves, (1, 2), (2, 3)),
        Some(para)
    );
    curves.insert((1, 2), h);
    curves.insert((2, 3), h);
    assert_eq!(mixed_cycle_shared_conic(&curves, (1, 2), (2, 3)), Some(h));
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
    // FLIPPED 2026-08-25: cone chartability is always-on (the off-knob was
    // deleted 2026-10-16).
    let cone = Surface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: 0.3,
    };
    assert!(SurfaceChart::supports(&cone));
    assert!(SurfaceChart::new(cone).is_some());
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

/// Build the unit-slope vertical cone `z = sqrt(x² + y²)` (`apex = (0, 0, 0)`,
/// `axis_dir = (0, 0, 1)`, `half_angle = π/4`) capped by a circle disc at `z = 6`.
fn vertical_unit_slope_cone_brep() -> crate::BRep {
    use crate::{BRep, BRepEdge, BRepFace, BRepVertex};
    BRep::new(
        vec![
            BRepVertex {
                point: Point3::new(6.0, 0.0, 6.0),
            },
            BRepVertex {
                point: Point3::new(0.0, 0.0, 0.0),
            },
        ],
        vec![BRepEdge {
            start: 0,
            end: 0,
            curve: Curve::Circle {
                center: Point3::new(0.0, 0.0, 6.0),
                normal: Vector3::new(0.0, 0.0, 1.0),
                radius: 6.0,
            },
        }],
        vec![
            BRepFace {
                surface: Surface::Cone {
                    apex: Point3::new(0.0, 0.0, 0.0),
                    axis_dir: Vector3::new(0.0, 0.0, 1.0),
                    half_angle: std::f64::consts::FRAC_PI_4,
                },
                outer_loop: vec![0],
                inner_loops: vec![],
                reversed: false,
            },
            BRepFace {
                surface: Surface::Plane {
                    normal: Vector3::new(0.0, 0.0, 1.0),
                    d: -6.0,
                },
                outer_loop: vec![0],
                inner_loops: vec![],
                reversed: false,
            },
        ],
    )
    .expect("vertical cone BRep")
}

/// Build a planar-face `BRep` carrying the given outward `Surface::Plane` list
/// (each materialized as an axis-aligned dummy quad so `BRep::new` validates
/// while `compute_phase_a` inherits `b.faces()[idx].surface`).
fn planes_carrier_brep(planes: &[Surface]) -> crate::BRep {
    use crate::{BRep, BRepEdge, BRepFace, BRepVertex};
    let mut verts = Vec::new();
    let mut edges = Vec::new();
    let mut faces = Vec::new();
    for (i, &surface) in planes.iter().enumerate() {
        let z = i as f64;
        let v0 = verts.len() as u32;
        verts.extend([
            BRepVertex {
                point: Point3::new(0.0, 0.0, z),
            },
            BRepVertex {
                point: Point3::new(1.0, 0.0, z),
            },
            BRepVertex {
                point: Point3::new(1.0, 1.0, z),
            },
            BRepVertex {
                point: Point3::new(0.0, 1.0, z),
            },
        ]);
        let e0 = edges.len() as u32;
        edges.extend([
            BRepEdge {
                start: v0,
                end: v0 + 1,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: v0 + 1,
                end: v0 + 2,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: v0 + 2,
                end: v0 + 3,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: v0 + 3,
                end: v0,
                curve: Curve::LineSegment,
            },
        ]);
        faces.push(BRepFace {
            surface,
            outer_loop: vec![e0, e0 + 1, e0 + 2, e0 + 3],
            inner_loops: vec![],
            reversed: false,
        });
    }
    BRep::new(verts, edges, faces).expect("planes carrier BRep")
}

/// Ledger N89 item 3 (REOPENED 2026-10-27; Yang et al. 2025 §4.5.3
/// `refs/text/yang2025_hybrid_boolean.txt:679-689, 718-751`, Fig. 15, matrix rows
/// `4.5.3-08` and `4.5.3-04`): a vertical square prism `[1.0, 2.0] × [-0.5, 0.5]`
/// with all 4 side planes parallel to the cone's Z-axis pierces the lateral face
/// of cone `z = sqrt(x² + y²)` in a closed boundary loop of FOUR `Curve::Hyperbola`
/// arcs (zero `Circle`/`Ellipse`/`LineSegment`/`SurfacePair` edges).
///
/// Along the `x = 1.0` hyperbola `z = sqrt(1 + y²)`, vertex 3 (`y = +0.05`) sits
/// backward behind vertex 2 (`y = +0.10`), producing a non-collinear discrete
/// tangent `|t̃| ≈ 0.0746 >> TAU_WORK` at ~86.5° ∈ (45°, 135°) to the exact
/// hyperbola tangent (so `angle_band_reversed` returns `false` unless both cycle
/// admission AND the live `Curve::Hyperbola` tangent arm in `angle_site_tangent`
/// run). Pin `sweep_reversed_intersections` directly.
#[test]
fn n89_item3_sweep_visits_and_corrects_all_hyperbola_prism_cone_loop() {
    use crate::brep::TriangleSideLabel;
    use crate::stage4_correct::{compute_phase_a, sweep_reversed_intersections};
    use crate::{InputId, Mesh, TriangleAttribution, TriangleAttributionMap};
    use std::collections::{BTreeMap, HashSet};

    let a = vertical_unit_slope_cone_brep();
    let b = planes_carrier_brep(&[
        // x = 1.0
        Surface::Plane {
            normal: Vector3::new(-1.0, 0.0, 0.0),
            d: 1.0,
        },
        // y = 0.5
        Surface::Plane {
            normal: Vector3::new(0.0, 1.0, 0.0),
            d: -0.5,
        },
        // x = 2.0
        Surface::Plane {
            normal: Vector3::new(1.0, 0.0, 0.0),
            d: -2.0,
        },
        // y = -0.5
        Surface::Plane {
            normal: Vector3::new(0.0, -1.0, 0.0),
            d: -0.5,
        },
    ]);

    let cone_pt = |x: f64, y: f64| Point3::new(x, y, (x * x + y * y).sqrt());
    let verts = vec![
        // 0..=5: x = 1.0 hyperbola arc (y from -0.5 to +0.5, with v3 reversed at y = +0.05)
        cone_pt(1.0, -0.5),  // 0: 3-surface corner (Cone ∩ x=1 ∩ y=-0.5)
        cone_pt(1.0, -0.10), // 1: monotone interior
        cone_pt(1.0, 0.10),  // 2: p_r (y = +0.10)
        cone_pt(1.0, 0.05),  // 3: reversed p_n (y = +0.05 < +0.10, |t̃| ≈ 0.0746 >> TAU_WORK)
        cone_pt(1.0, 0.30),  // 4: monotone continuation (y = +0.30 > +0.10)
        cone_pt(1.0, 0.5),   // 5: 3-surface corner (Cone ∩ x=1 ∩ y=+0.5)
        // 5..=7: y = +0.5 hyperbola arc
        cone_pt(1.5, 0.5), // 6: interior
        cone_pt(2.0, 0.5), // 7: 3-surface corner (Cone ∩ x=2 ∩ y=+0.5)
        // 7..=9: x = 2.0 hyperbola arc
        cone_pt(2.0, 0.0),  // 8: interior
        cone_pt(2.0, -0.5), // 9: 3-surface corner (Cone ∩ x=2 ∩ y=-0.5)
        // 9..=10->0: y = -0.5 hyperbola arc
        cone_pt(1.5, -0.5), // 10: interior
        // 11: interior hub of cone patch A(0)
        Point3::new(1.5, 0.0, 1.5),
        // 12..=15: top corners of the 4 vertical prism side strips at z = 5.0
        Point3::new(1.0, -0.5, 5.0), // 12
        Point3::new(1.0, 0.5, 5.0),  // 13
        Point3::new(2.0, 0.5, 5.0),  // 14
        Point3::new(2.0, -0.5, 5.0), // 15
    ];

    let mut tris: Vec<[u32; 3]> = Vec::new();
    let mut attr: Vec<Option<TriangleAttribution>> = Vec::new();
    let mut push = |tri: [u32; 3], input: InputId, face: u32| {
        tris.push(tri);
        attr.push(Some(TriangleAttribution { input, face }));
    };

    // Patch A(0): 11-triangle cone fan whose sole boundary cycle is the closed
    // 11-edge 4-Hyperbola intersection loop 0->1->...->10->0.
    for k in 0..11u32 {
        push([11, k, (k + 1) % 11], InputId::A, 0);
    }
    // Patch B(0) (x = 1.0): shares hyperbola edges 0..5 in opposite winding.
    for &(u, v) in &[(0u32, 1u32), (1, 2), (2, 3), (3, 4), (4, 5)] {
        push([12, v, u], InputId::B, 0);
    }
    push([12, 13, 5], InputId::B, 0);
    // Patch B(1) (y = +0.5): shares hyperbola edges 5->6->7.
    push([13, 6, 5], InputId::B, 1);
    push([13, 7, 6], InputId::B, 1);
    push([13, 14, 7], InputId::B, 1);
    // Patch B(2) (x = 2.0): shares hyperbola edges 7->8->9.
    push([14, 8, 7], InputId::B, 2);
    push([14, 9, 8], InputId::B, 2);
    push([14, 15, 9], InputId::B, 2);
    // Patch B(3) (y = -0.5): shares hyperbola edges 9->10->0.
    push([15, 10, 9], InputId::B, 3);
    push([15, 0, 10], InputId::B, 3);
    push([15, 12, 0], InputId::B, 3);

    let entry_pos: Vec<[f64; 3]> = verts.iter().map(|p| p.as_array()).collect();
    let mut mesh = Mesh::new(verts, tris);
    let mut side_labels = vec![TriangleSideLabel::default(); mesh.tris.len()];

    // Precondition: Phase A classifies EVERY cross-operand edge as `Curve::Hyperbola`
    // (11 edges on 4 hyperbolas, zero other curve variants).
    let (infos_pre, _inc_pre, curves_pre) = compute_phase_a(
        &mesh,
        &TriangleAttributionMap {
            attributions: attr.clone(),
        },
        &a,
        &b,
        &crate::stage3_ssi::NO_EDGE_PROVENANCE,
    )
    .expect("pre-sweep compute_phase_a");
    assert_eq!(
        curves_pre.len(),
        11,
        "all 11 A×B edges must carry SSI curves"
    );
    assert!(
        curves_pre
            .values()
            .all(|c| matches!(c, Curve::Hyperbola { .. })),
        "every intersection curve in the prism×cone loop must be Curve::Hyperbola, got {curves_pre:?}"
    );
    assert_eq!(infos_pre[0].cycles.len(), 1);
    assert_eq!(infos_pre[0].cycles[0].len(), 11);

    let swept = sweep_reversed_intersections(
        &mut mesh,
        &mut attr,
        &mut side_labels,
        &a,
        &b,
        0.1,
        &HashSet::new(),
        &entry_pos,
        &BTreeMap::new(),
    )
    .expect("sweep_reversed_intersections on 4-Hyperbola prism×cone loop");

    assert!(
        swept,
        "§4.5.3 sweep must visit the all-Hyperbola loop and remove reversed vertex 3"
    );
    assert!(
        mesh.tris.iter().all(|t| !t.contains(&3)),
        "reversed interior hyperbola vertex 3 must be collapsed out of mesh.tris"
    );
    for corner in [0u32, 5, 7, 9] {
        assert!(
            mesh.tris.iter().any(|t| t.contains(&corner)),
            "3-surface hyperbola×hyperbola corner {corner} must remain protected"
        );
    }

    // Postcondition: Cone patch A(0)'s cycle is now 10 edges with strictly
    // ascending y along the x = 1.0 hyperbola.
    let (infos_post, _, _) = compute_phase_a(
        &mesh,
        &TriangleAttributionMap { attributions: attr },
        &a,
        &b,
        &crate::stage3_ssi::NO_EDGE_PROVENANCE,
    )
    .expect("post-sweep compute_phase_a");
    assert_eq!(infos_post[0].cycles[0].len(), 10);
}

/// Companion sweep-level pin for `Curve::Parabola` (ledger N89 item 3; matrix
/// rows `4.5.3-08` and `4.5.3-04`): two planes parallel to opposite generators
/// of cone `z = sqrt(x² + y²)` (`z − x = 1` and `z + x = 4`) cut the cone lateral
/// in a closed 2-`Parabola` lens loop meeting at `(1.5, ±2.0, 2.5)` (all dyadic
/// `f64` coordinates). A non-collinear reversed interior vertex on the first
/// parabola is visited and collapsed by `sweep_reversed_intersections` via the
/// live `Curve::Parabola` tangent arm in `angle_site_tangent`.
#[test]
fn n89_item3_sweep_visits_and_corrects_all_parabola_lens_loop() {
    use crate::brep::TriangleSideLabel;
    use crate::stage4_correct::{compute_phase_a, sweep_reversed_intersections};
    use crate::{InputId, Mesh, TriangleAttribution, TriangleAttributionMap};
    use std::collections::{BTreeMap, HashSet};

    let a = vertical_unit_slope_cone_brep();
    let s2_inv = std::f64::consts::FRAC_1_SQRT_2;
    let b = planes_carrier_brep(&[
        // (-x + z)/√2 = 1/√2  <=>  z - x = 1  =>  x = (y² - 1)/2, z = (y² + 1)/2
        Surface::Plane {
            normal: Vector3::new(-s2_inv, 0.0, s2_inv),
            d: -s2_inv,
        },
        // (x + z)/√2 = 4/√2  <=>  z + x = 4  =>  x = 2 - y²/8, z = 2 + y²/8
        Surface::Plane {
            normal: Vector3::new(s2_inv, 0.0, s2_inv),
            d: -4.0 * s2_inv,
        },
    ]);

    let p0 = |y: f64| Point3::new(0.5 * (y * y - 1.0), y, 0.5 * (y * y + 1.0));
    let p1 = |y: f64| Point3::new(2.0 - 0.125 * y * y, y, 2.0 + 0.125 * y * y);

    let verts = vec![
        p0(-2.0),  // 0: 3-surface lens corner (1.5, -2.0, 2.5)
        p0(-0.5),  // 1: monotone interior
        p0(0.25),  // 2: p_r (y = 0.25)
        p0(0.125), // 3: reversed p_n (y = 0.125 < 0.25, non-collinear on parabola)
        p0(1.0),   // 4: monotone continuation (y = 1.0)
        p0(2.0),   // 5: 3-surface lens corner (1.5, +2.0, 2.5)
        p1(0.0),   // 6: return arc midpoint (2.0, 0.0, 2.0)
        // 7: interior hub of cone patch A(0)
        Point3::new(1.0, 0.0, 1.5),
        // 8, 9: hubs of the two oblique plane strips B(0), B(1)
        Point3::new(-2.0, 0.0, -1.0),
        Point3::new(3.5, 0.0, 0.5),
    ];

    let mut tris: Vec<[u32; 3]> = Vec::new();
    let mut attr: Vec<Option<TriangleAttribution>> = Vec::new();
    let mut push = |tri: [u32; 3], input: InputId, face: u32| {
        tris.push(tri);
        attr.push(Some(TriangleAttribution { input, face }));
    };

    for k in 0..7u32 {
        push([7, k, (k + 1) % 7], InputId::A, 0);
    }
    for &(u, v) in &[(0u32, 1u32), (1, 2), (2, 3), (3, 4), (4, 5)] {
        push([8, v, u], InputId::B, 0);
    }
    for &(u, v) in &[(5u32, 6u32), (6, 0)] {
        push([9, v, u], InputId::B, 1);
    }

    let entry_pos: Vec<[f64; 3]> = verts.iter().map(|p| p.as_array()).collect();
    let mut mesh = Mesh::new(verts, tris);
    let mut side_labels = vec![TriangleSideLabel::default(); mesh.tris.len()];

    let (_infos_pre, _inc_pre, curves_pre) = compute_phase_a(
        &mesh,
        &TriangleAttributionMap {
            attributions: attr.clone(),
        },
        &a,
        &b,
        &crate::stage3_ssi::NO_EDGE_PROVENANCE,
    )
    .expect("pre-sweep compute_phase_a on parabola lens");
    assert_eq!(curves_pre.len(), 7);
    assert!(
        curves_pre
            .values()
            .all(|c| matches!(c, Curve::Parabola { .. })),
        "every intersection curve in the lens loop must be Curve::Parabola, got {curves_pre:?}"
    );

    let swept = sweep_reversed_intersections(
        &mut mesh,
        &mut attr,
        &mut side_labels,
        &a,
        &b,
        0.2,
        &HashSet::new(),
        &entry_pos,
        &BTreeMap::new(),
    )
    .expect("sweep_reversed_intersections on 2-Parabola lens loop");

    assert!(
        swept,
        "§4.5.3 sweep must visit the all-Parabola loop and remove reversed vertex 3"
    );
    assert!(
        mesh.tris.iter().all(|t| !t.contains(&3)),
        "reversed interior parabola vertex 3 must be collapsed out of mesh.tris"
    );
    for corner in [0u32, 5] {
        assert!(
            mesh.tris.iter().any(|t| t.contains(&corner)),
            "3-surface parabola×parabola corner {corner} must remain protected"
        );
    }
}
