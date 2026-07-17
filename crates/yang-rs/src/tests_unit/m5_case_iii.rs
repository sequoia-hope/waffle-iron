#[allow(unused_imports)]
use super::*;

// ── Case-III graze guard (spec `yang_172_case_iii_graze_guard`) ──────

fn ctup(ap: [f64; 3], ad: [f64; 3], r: f64) -> (Point3, Vector3, f64) {
    (
        Point3::new(ap[0], ap[1], ap[2]),
        Vector3::new(ad[0], ad[1], ad[2]),
        r,
    )
}

const Z: [f64; 3] = [0.0, 0.0, 1.0];
const X: [f64; 3] = [1.0, 0.0, 0.0];

fn sag(r: f64, n: usize) -> f64 {
    r * (1.0 - (std::f64::consts::PI / n as f64).cos())
}

/// The C0116 pair (perpendicular axes, r 0.5/0.3, axis distance 0.79 ⇒
/// depth 0.01): the guard demands the MINIMAL N clearing the factor-2
/// sagitta margin — above the measured Phase-0 green floor of 16.
#[test]
pub(crate) fn graze_demand_c0116_pair_derives_minimal_n() {
    let demand = cyl_pair_graze_demand(
        ctup([0.0, 0.0, 0.0], Z, 0.5),
        ctup([-1.5, 0.79, 1.0], X, 0.3),
    );
    let GrazeDemand::Boost(n) = demand else {
        panic!("expected Boost, got {demand:?}");
    };
    let depth = 0.8 - 0.79;
    assert!(
        sag(0.5, n) + sag(0.3, n) <= depth / 2.0,
        "derived N={n} must clear the depth with the factor-2 margin"
    );
    assert!(
        sag(0.5, n - 1) + sag(0.3, n - 1) > depth / 2.0,
        "derived N={n} must be MINIMAL (no over-refinement)"
    );
}

/// A deep crossing derives a tiny N (absorbed later by the natural-N
/// gate); disjoint and exactly-tangent pairs are silent — `depth ≤ 0`
/// belongs to the Case-IV side / measure-zero contact.
#[test]
pub(crate) fn graze_demand_deep_disjoint_tangent() {
    // Deep: axis distance 0.4 ⇒ depth 0.4 ⇒ a tiny N (5 at these radii,
    // the minimal N with sag(0.5,N)+sag(0.3,N) ≤ 0.2) that any natural
    // Stage-1 N (12 here) absorbs.
    assert_eq!(
        cyl_pair_graze_demand(
            ctup([0.0, 0.0, 0.0], Z, 0.5),
            ctup([-1.5, 0.4, 1.0], X, 0.3)
        ),
        GrazeDemand::Boost(5)
    );
    // Disjoint (gap 0.01) and exact tangency (depth == 0.0).
    assert_eq!(
        cyl_pair_graze_demand(
            ctup([0.0, 0.0, 0.0], Z, 0.5),
            ctup([-1.5, 0.81, 1.0], X, 0.3)
        ),
        GrazeDemand::None
    );
    assert_eq!(
        cyl_pair_graze_demand(
            ctup([0.0, 0.0, 0.0], Z, 0.5),
            ctup([-1.5, 0.8, 1.0], X, 0.3)
        ),
        GrazeDemand::None
    );
}

/// A depth inside the #178-calibrated coincidence-authoring noise class
/// (≤ max(TAU_MODEL, scale·TAU_WORK)/100 = 1e-9 at this scale) is
/// authored tangency residue — silent, today's behavior preserved.
#[test]
pub(crate) fn graze_demand_subnoise_depth_is_silent() {
    assert_eq!(
        cyl_pair_graze_demand(
            ctup([0.0, 0.0, 0.0], Z, 0.5),
            ctup([-1.5, 0.8 - 1e-12, 1.0], X, 0.3)
        ),
        GrazeDemand::None
    );
}

/// A genuine micro-graze (depth 1e-8: above the 1e-9 noise line, below
/// the rim-N-cap observability floor ≈ 4.7e-7) is the typed-STOP arm.
#[test]
pub(crate) fn graze_demand_micro_graze_is_sub_sagitta() {
    let demand = cyl_pair_graze_demand(
        ctup([0.0, 0.0, 0.0], Z, 0.5),
        ctup([-1.5, 0.8 - 1e-8, 1.0], X, 0.3),
    );
    let GrazeDemand::SubSagitta { depth, floor } = demand else {
        panic!("expected SubSagitta, got {demand:?}");
    };
    assert!((depth - 1e-8).abs() < 1e-12, "depth={depth:e}");
    assert!(
        depth < floor && floor < 1e-5,
        "floor={floor:e} must sit between the depth and render scale"
    );
}

/// Parallel axes crossing near INTERNAL tangency (d − |r_a−r_b| small):
/// the crescent-side graze also demands refinement.
#[test]
pub(crate) fn graze_demand_parallel_internal_graze() {
    let d = 0.5 + 0.01; // |r_a − r_b| = 0.5, depth = 0.01 (render-observable)
    let demand = cyl_pair_graze_demand(ctup([0.0, 0.0, 0.0], Z, 1.0), ctup([d, 0.0, 0.0], Z, 0.5));
    let GrazeDemand::Boost(n) = demand else {
        panic!("expected Boost, got {demand:?}");
    };
    assert!(
        sag(1.0, n) + sag(0.5, n) <= 0.01 / 2.0,
        "derived N={n} must clear the internal-graze depth"
    );
}

/// The render-observability scope line: a lens shallower than the render
/// mesh's combined sagitta (2·1e-3·(r_a+r_b)) cannot be represented at
/// any output resolution — the C0057 class (parallel, depth 1e-6, would
/// demand N=3142) keeps the measured corpus-green status quo; §4.5.2
/// local refinement is its roadmap home. Above the 4096-cap floor, so no
/// STOP either.
#[test]
pub(crate) fn graze_demand_sub_render_depth_is_silent() {
    assert_eq!(
        cyl_pair_graze_demand(
            ctup([0.0, 0.0, 0.0], Z, 0.5),
            ctup([0.999999, 0.0, 0.3], Z, 0.5)
        ),
        GrazeDemand::None
    );
}

/// Axis-generic cylinder B-Rep (the `guard_cyl` shape with a free axis):
/// base rim at `base`, top rim at `base + h·axis`.
pub(crate) fn graze_cyl(base: [f64; 3], axis: [f64; 3], r: f64, h: f64) -> BRep {
    let ax = Vector3::new(axis[0], axis[1], axis[2]);
    let (e1, _) = ortho_basis(ax);
    let b = Point3::new(base[0], base[1], base[2]);
    let top = Point3::new(
        base[0] + h * axis[0],
        base[1] + h * axis[1],
        base[2] + h * axis[2],
    );
    let seam = |c: Point3| Point3::new(c.x() + r * e1.x(), c.y() + r * e1.y(), c.z() + r * e1.z());
    let verts = vec![
        BRepVertex { point: seam(b) },
        BRepVertex { point: seam(top) },
    ];
    let edges = vec![
        BRepEdge {
            start: 0,
            end: 0,
            curve: Curve::Circle {
                center: b,
                normal: Vector3::new(-axis[0], -axis[1], -axis[2]),
                radius: r,
            },
        },
        BRepEdge {
            start: 1,
            end: 1,
            curve: Curve::Circle {
                center: top,
                normal: ax,
                radius: r,
            },
        },
        BRepEdge {
            start: 0,
            end: 1,
            curve: Curve::LineSegment,
        },
    ];
    let faces = vec![
        BRepFace {
            surface: Surface::Cylinder {
                axis_point: b,
                axis_dir: ax,
                radius: r,
            },
            outer_loop: vec![0, 2, 1, 2],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(-axis[0], -axis[1], -axis[2]),
                d: -(b.x() * axis[0] + b.y() * axis[1] + b.z() * axis[2]),
            },
            outer_loop: vec![0],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: Surface::Plane {
                normal: ax,
                d: top.x() * axis[0] + top.y() * axis[1] + top.z() * axis[2],
            },
            outer_loop: vec![1],
            inner_loops: Vec::new(),
            reversed: false,
        },
    ];
    BRep::new(verts, edges, faces).expect("graze guard cylinder")
}

/// Scan level, C0116 geometry: the cross pair demands a boost above both
/// solids' natural N.
#[test]
pub(crate) fn graze_guard_scan_boosts_c0116_shape() {
    let boss = graze_cyl([0.0, 0.0, 0.0], Z, 0.5, 2.0);
    let tool = graze_cyl([-1.5, 0.79, 1.0], X, 0.3, 3.5);
    let n = graze_min_rim_segments(&boss, &tool)
        .expect("no STOP")
        .expect("guard must fire");
    assert!(
        sag(0.5, n) + sag(0.3, n) <= 0.01 / 2.0,
        "boosted N={n} must make the meshes sample the wedge"
    );
}

/// Scan level: a deep crossing derives N=3, absorbed by the natural-N
/// self-limiting gate — byte-identical path.
#[test]
pub(crate) fn graze_guard_scan_deep_crossing_absorbed() {
    let boss = graze_cyl([0.0, 0.0, 0.0], Z, 0.5, 2.0);
    let tool = graze_cyl([-1.5, 0.4, 1.0], X, 0.3, 3.5);
    assert!(matches!(graze_min_rim_segments(&boss, &tool), Ok(None)));
}

/// Scan level: an in-extent micro-graze STOPs with the typed error.
#[test]
pub(crate) fn graze_guard_micro_graze_stops_loudly() {
    let boss = graze_cyl([0.0, 0.0, 0.0], Z, 0.5, 2.0);
    let tool = graze_cyl([-1.5, 0.8 - 1e-8, 1.0], X, 0.3, 3.5);
    match graze_min_rim_segments(&boss, &tool) {
        Err(YangError::SubSagittaGrazeIntersection {
            face_a: 0,
            face_b: 0,
            depth,
            ..
        }) => assert!((depth - 1e-8).abs() < 1e-12),
        other => panic!("expected SubSagittaGrazeIntersection, got {other:?}"),
    }
}

/// Scan level, phase-aware Case-III filter: a render-observable parallel
/// graze (depth 5e-3) whose seam-anchored NATURAL meshes already catch
/// the lens — both rings anchor at +x, so A's +x vertex-generator
/// penetrates B's polygon and crosses B's bottom-cap disc — is NOT a
/// Case-III miss ("the meshes miss intersections"): the guard must stay
/// silent and keep the byte-identical path, even though the derived N
/// (45) exceeds both naturals (10/12).
#[test]
pub(crate) fn graze_guard_phase_hit_pair_is_silent() {
    let a = graze_cyl([0.0, 0.0, 0.0], Z, 0.5, 2.0);
    let b = graze_cyl([0.995, 0.0, 0.3], Z, 0.5, 1.4);
    assert!(matches!(graze_min_rim_segments(&a, &b), Ok(None)));
}

/// Scan level, witness check: the same micro-graze pair with the tool
/// displaced along its own axis so the graze region (the common
/// perpendicular at x=0) lies outside the tool's axial span — the
/// infinite surfaces graze off-face; no STOP (the adjacent-boss class).
#[test]
pub(crate) fn graze_guard_micro_graze_off_extent_is_silent() {
    let boss = graze_cyl([0.0, 0.0, 0.0], Z, 0.5, 2.0);
    let tool = graze_cyl([5.0, 0.8 - 1e-8, 1.0], X, 0.3, 3.5);
    assert!(matches!(graze_min_rim_segments(&boss, &tool), Ok(None)));
}
