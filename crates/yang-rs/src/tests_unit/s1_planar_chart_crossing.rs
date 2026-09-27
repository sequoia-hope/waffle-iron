//! Stage-1 chart simplicity on the PLANAR CDT path (Yang §4.5.4; spec
//! `yang_stage1_curved_holed_patch` "The planar path's scan", 2026-09-12):
//! the F0082 face-372 shape — a cylinder's base cap that re-enters the next
//! union carrying the two corners where the near-coplanar stack's
//! plane∩plane line meets the rectangle walls, each 1.457e-3 INSIDE the rim
//! circle (r 0.2123). At the natural rim density a rim chord passes over both
//! corners, the loop's corner chords cross it, and the flood-fill CDT
//! refused the ring loud (`face 372: CDT triangulation failed`). The scan
//! reports the crossing with the rim demand that clears the corners and the
//! driver's retry tessellates the cap at that density.

use super::*;

fn p2(x: f64, y: f64) -> cad_primitives::Point2 {
    cad_primitives::Point2::new(x, y)
}

/// The measured F0082 numbers (`YANG_CDT_PROBE=372`, fitted rim circle):
/// r = 0.212325, the two plane∩plane∩wall corners at radial 0.210868
/// (gap 1.457e-3) at azimuths 27.6° and 207.6°; the −x wall meets the rim at
/// 210°, the +x wall at −28°; the rim arc runs from 210° clockwise (seen
/// from +z) round through 0° to −28° (238°); the cap faces −z.
const R: f64 = 0.212325;
const GAP: f64 = 1.457e-3;

fn cap_segment() -> (Vec<BRepVertex>, Vec<BRepEdge>, Vec<BRepFace>) {
    let deg = std::f64::consts::PI / 180.0;
    let at = |radius: f64, azimuth_deg: f64| {
        Point3::new(
            radius * (azimuth_deg * deg).cos(),
            radius * (azimuth_deg * deg).sin(),
            0.0,
        )
    };
    let verts = [
        at(R - GAP, 27.6),  // V0: the +x-wall ∩ plane∩plane-line corner
        at(R - GAP, 207.6), // V1: the −x-wall corner
        at(R, 210.0),       // V2: the −x wall meets the rim
        at(R, -28.0),       // V3: the +x wall meets the rim
    ]
    .into_iter()
    .map(|point| BRepVertex { point })
    .collect::<Vec<_>>();
    let line = Curve::LineSegment;
    let e = |start: u32, end: u32, curve: Curve| BRepEdge { start, end, curve };
    let edges = vec![
        e(0, 1, line), // the plane∩plane line, a diameter
        e(1, 2, line), // the −x wall, 3e-3 long
        e(
            2,
            3,
            Curve::Circle {
                center: Point3::new(0.0, 0.0, 0.0),
                normal: Vector3::new(0.0, 0.0, -1.0), // CCW about −z = clockwise from +z
                radius: R,
            },
        ),
        e(3, 0, line), // the +x wall
    ];
    let faces = vec![BRepFace {
        surface: Surface::Plane {
            normal: Vector3::new(0.0, 0.0, -1.0),
            d: 0.0,
        },
        outer_loop: vec![0, 1, 2, 3],
        inner_loops: vec![],
        reversed: false,
    }];
    (verts, edges, faces)
}

/// The exact face area: the 238° circular segment (the disc on the arc's
/// side of chord V2V3) MINUS the quadrilateral V0 V1 V2 V3 — the part of
/// that segment below the plane∩plane diameter V0V1, which the union
/// removed (the cap is inside the stack there).
fn exact_area() -> f64 {
    let deg = std::f64::consts::PI / 180.0;
    let sweep = 238.0 * deg;
    let segment = 0.5 * R * R * (sweep - sweep.sin());
    let at = |radius: f64, azimuth_deg: f64| {
        (
            radius * (azimuth_deg * deg).cos(),
            radius * (azimuth_deg * deg).sin(),
        )
    };
    let poly = [
        at(R - GAP, 27.6),
        at(R - GAP, 207.6),
        at(R, 210.0),
        at(R, -28.0),
    ];
    let mut twice = 0.0;
    for k in 0..4 {
        let (a, b) = (poly[k], poly[(k + 1) % 4]);
        twice += a.0 * b.1 - a.1 * b.0;
    }
    segment - 0.5 * twice.abs()
}

/// One pass at the natural density reports the crossing with the demand that
/// halves the corners' 1.457e-3 gap: `sag(R, N) ≤ 7.3e-4` ⇒ N = 38.
#[test]
fn a_corner_inside_the_rim_band_is_reported_with_a_demand_by_one_pass() {
    let (verts, edges, faces) = cap_segment();
    let mut n_used = None;
    let empty = std::collections::BTreeMap::new();
    let no_demands = std::collections::BTreeMap::new();
    let got = stage1_tessellate_once(
        &verts,
        &edges,
        &faces,
        &empty,
        &empty,
        &empty,
        &crate::stage1_tessellate::FaceConstraints::new(),
        &no_demands,
        None,
        &mut n_used,
    );
    match got {
        Err(YangError::Stage1ChartCrossing {
            face: 0,
            crossings,
            demand_n: Some(n),
        }) => {
            let cur = n_used.expect("the pass chose an N");
            assert!(
                crossings >= 2,
                "both corner chords cross the rim chord: {crossings}"
            );
            assert!(n > cur, "demand {n} must exceed the pass's N {cur}");
            let sag = |n: usize| R * (1.0 - (std::f64::consts::PI / n as f64).cos());
            assert!(
                sag(n) <= GAP / 2.0 && sag(n - 1) > GAP / 2.0,
                "demand N={n}: the minimal N whose sagitta clears half the gap"
            );
            assert_eq!(n, 38, "the measured F0082 demand");
        }
        Err(e) => panic!("expected a chart crossing with a demand, got {e:?}"),
        Ok(_) => panic!("expected a chart crossing with a demand, got a tessellation"),
    }
}

/// The driver's §4.5.4 retry lands the cap at the demanded density: a
/// fold-free triangulation whose area is the exact segment-plus-polygon
/// within the chord deficit, every triangle facing −z, both corners kept as
/// output vertices, and the rim sampled at ≥ 38 segments per turn.
#[test]
fn the_driver_refines_the_cap_segment_and_tessellates() {
    let (verts, edges, faces) = cap_segment();
    let t = stage1_tessellate(&verts, &edges, &faces)
        .expect("the §4.5.4 retry lands the re-entering cap");
    let expect = exact_area();
    let mut area = 0.0;
    let mut cover: std::collections::BTreeMap<(u32, u32), u32> = Default::default();
    for tri in &t.tris {
        let p: Vec<[f64; 3]> = tri
            .iter()
            .map(|&i| t.verts[i as usize].as_array())
            .collect();
        let e1 = [p[1][0] - p[0][0], p[1][1] - p[0][1], p[1][2] - p[0][2]];
        let e2 = [p[2][0] - p[0][0], p[2][1] - p[0][1], p[2][2] - p[0][2]];
        let nz = e1[0] * e2[1] - e1[1] * e2[0];
        assert!(
            nz < 0.0,
            "every triangle faces −z (the cap's sense): {tri:?} nz={nz}"
        );
        area += 0.5 * nz.abs();
        for k in 0..3 {
            let (x, y) = (tri[k], tri[(k + 1) % 3]);
            *cover.entry((x.min(y), x.max(y))).or_insert(0) += 1;
        }
    }
    // The chord deficit of a 238° arc at N = 38: ≈ ⅔·sag·arc ≈ 4.3e-4 on
    // 0.0742 (measured 3.9e-4).
    assert!(
        (area - expect).abs() < 0.01 * expect,
        "cap area {area} vs exact {expect}"
    );
    assert!(
        cover.values().all(|&c| c <= 2),
        "an edge covered more than twice: fold"
    );
    let on_rim = t
        .verts
        .iter()
        .filter(|p| {
            let a = p.as_array();
            ((a[0] * a[0] + a[1] * a[1]).sqrt() - R).abs() < 1e-9
        })
        .count();
    // 238° of a ≥ 38-segment turn: ≥ 26 rim vertices (the two wall
    // endpoints included).
    assert!(
        on_rim >= 26,
        "rim vertices {on_rim} (238° at ≥ 38 segments/turn)"
    );
    for corner in [&verts[0].point, &verts[1].point] {
        assert!(
            t.verts.iter().any(|p| p.as_array() == corner.as_array()),
            "the corner {corner:?} is an output vertex"
        );
    }
}

/// The planar rim demand reads the circle itself: a rim chord of a
/// r = 0.5 circle centred off the chart origin, crossed by a chord whose
/// endpoints sit 0.004 and 0.05 inside the circle, demands the N with
/// `sag(0.5, N) ≤ 0.002`; the centre offset does not leak into the distance.
#[test]
fn planar_rim_demand_measures_from_the_circle_centre() {
    let (cx, cy, r) = (3.0_f64, -2.0_f64, 0.5_f64);
    let on = |radius: f64, theta: f64| p2(cx + radius * theta.cos(), cy + radius * theta.sin());
    let polys = vec![vec![
        on(r, 0.0),
        on(r, 0.3),
        on(r - 0.004, 0.15),
        on(r - 0.05, 0.15),
    ]];
    let crossings = vec![((0usize, 0usize), (0usize, 2usize))];
    let rim = |(_, k): ChartSeg| {
        (k == 0).then_some(RimChart {
            center: (cx, cy),
            ell: r,
            radius: r,
        })
    };
    let n = chart_rim_demand(&polys, &crossings, rim).expect("a rim chord is involved");
    let sag = |n: usize| r * (1.0 - (std::f64::consts::PI / n as f64).cos());
    assert!(sag(n) <= 0.002 && sag(n - 1) > 0.002, "N={n}");
    // Measured from the chart origin instead, the same vertices would sit
    // ~3.6 away from a "rim" of chart radius 0.5 — a different, wrong N.
    let wrong = chart_rim_demand(&polys, &crossings, |(_, k)| {
        (k == 0).then_some(RimChart {
            center: (0.0, 0.0),
            ell: r,
            radius: r,
        })
    });
    assert_ne!(wrong, Some(n));
}
