//! Stage-1 chart simplicity (Yang §4.5.4; spec `yang_stage1_curved_holed_patch`
//! "The thin-band chart guard" + the detect-then-refine that backs it):
//! the crossing scan, the rim demand it derives, and the R0044-shaped spike
//! band that no rim-pair rule can make simple.

use super::*;

fn p2(x: f64, y: f64) -> cad_primitives::Point2 {
    cad_primitives::Point2::new(x, y)
}

#[test]
fn a_simple_polygon_and_touching_loops_have_no_crossings() {
    let square = vec![p2(0.0, 0.0), p2(1.0, 0.0), p2(1.0, 1.0), p2(0.0, 1.0)];
    assert!(chart_polygon_crossings(std::slice::from_ref(&square)).is_empty());
    // A hole touching the outer boundary at a vertex is not a crossing.
    let hole = vec![p2(0.0, 0.0), p2(0.3, 0.1), p2(0.1, 0.3)];
    assert!(chart_polygon_crossings(&[square, hole]).is_empty());
}

#[test]
fn a_bowtie_crosses_once_and_a_rim_chord_over_a_notch_crosses_twice() {
    let bowtie = vec![p2(0.0, 0.0), p2(1.0, 1.0), p2(1.0, 0.0), p2(0.0, 1.0)];
    assert_eq!(chart_polygon_crossings(&[bowtie]), vec![((0, 0), (0, 2))]);
    // A long chord (0,0)→(4,0) of one loop over a two-chord notch of the
    // same loop that dips below it: both notch chords cross it.
    let notched = vec![
        p2(0.0, 0.0),
        p2(4.0, 0.0),
        p2(4.0, 1.0),
        p2(2.5, 1.0),
        p2(2.0, -0.5), // the notch vertex, below the chord
        p2(1.5, 1.0),
        p2(0.0, 1.0),
    ];
    let x = chart_polygon_crossings(&[notched]);
    assert_eq!(x, vec![((0, 0), (0, 3)), ((0, 0), (0, 4))], "{x:?}");
}

/// The demand: a rim chord (3-D radius 10) at chart radius 14.142 crossed by
/// a notch chord whose nearest endpoint sits 0.02 inside the rim — the rim
/// must keep `sag ≤ 0.01`, i.e. N ≥ 71. (A chord that SHARES a vertex with
/// the rim sits at distance 0 and derives nothing; a crossing with no rim
/// chord derives nothing.)
#[test]
fn rim_demand_halves_the_crossed_vertex_distance() {
    let ell = 14.142_f64;
    // Chord 0→1 is the rim chord; chord 2→3 runs from a vertex 0.02 inside
    // the rim to one 0.4 inside.
    let polys = vec![vec![
        p2(ell, 0.0),
        p2(ell * 0.1_f64.cos(), ell * 0.1_f64.sin()),
        p2((ell - 0.02) * 0.05_f64.cos(), (ell - 0.02) * 0.05_f64.sin()),
        p2((ell - 0.4) * 0.05_f64.cos(), (ell - 0.4) * 0.05_f64.sin()),
    ]];
    let crossings = vec![((0usize, 0usize), (0usize, 2usize))];
    let n = cone_chart_rim_demand(&polys, &crossings, |(_, k)| (k == 0).then_some(10.0))
        .expect("a rim chord is involved");
    let sag = |n: usize| 10.0 * (1.0 - (std::f64::consts::PI / n as f64).cos());
    assert!(sag(n) <= 0.01 && sag(n - 1) > 0.01, "N={n}");
    assert_eq!(
        cone_chart_rim_demand(&polys, &crossings, |_| None),
        None,
        "no rim chord, nothing to refine"
    );
    // Chord 1→2 shares vertex 1 with the rim: distance 0, nothing derived.
    let touching = vec![((0usize, 0usize), (0usize, 1usize))];
    assert_eq!(
        cone_chart_rim_demand(&polys, &touching, |(_, k)| (k == 0).then_some(10.0)),
        None
    );
}

/// The R0044 shape at unit scale: the thin 45° cone band (rims 0.0283 apart
/// in slant) with a SPIKE bitten out of it from rim A up to 0.2 of the gap
/// below rim B, placed under a rim-B chord. The spike's apex is a plain
/// vertex joining two straight edges — NOT a circle, so the thin-band pair
/// guard cannot see it (R0044's notch vertex is a hyperbola × surface-pair
/// junction). The guard alone (N = 60 from the rim pair, outer dev sag
/// 0.0097) lets the rim-B chord pass over the apex (0.0057 below rim B):
/// `stage1_tessellate_once` reports the crossing with a demand (sag ≤
/// 0.0028 ⇒ N ≥ 133) and the driver's retry tessellates the face at that
/// density — area = band minus spike within 5 % (the corrugation
/// allowance), no fold, rim B at ≥ 133 segments per turn.
fn spike_band() -> (Vec<BRepVertex>, Vec<BRepEdge>, Vec<BRepFace>) {
    use std::f64::consts::PI;
    let alpha = PI / 4.0;
    let (h1, h2) = (10.0_f64, 10.02_f64);
    let h6 = h2 - 0.2 * (h2 - h1);
    let (r1, r2, r6) = (h1 * alpha.tan(), h2 * alpha.tan(), h6 * alpha.tan());
    let (t1, t2) = (1.50_f64, 1.54_f64); // under rim B's chord [1.466, 1.571] at N = 60
    let on = |r: f64, theta: f64, z: f64| Point3::new(r * theta.cos(), r * theta.sin(), z);
    let verts = [
        on(r1, 0.0, h1),             // V0
        on(r1, t1, h1),              // V1
        on(r6, 0.5 * (t1 + t2), h6), // V2 the spike apex
        on(r1, t2, h1),              // V3
        on(r1, PI, h1),              // V4
        on(r2, PI, h2),              // V5
        on(r2, PI / 3.0, h2),        // V6
        on(r2, 0.0, h2),             // V7
    ]
    .into_iter()
    .map(|point| BRepVertex { point })
    .collect::<Vec<_>>();
    let circ = |z: f64, r: f64, sign: f64| Curve::Circle {
        center: Point3::new(0.0, 0.0, z),
        normal: Vector3::new(0.0, 0.0, sign),
        radius: r,
    };
    let line = Curve::LineSegment;
    let e = |start: u32, end: u32, curve: Curve| BRepEdge { start, end, curve };
    let edges = vec![
        e(0, 1, circ(h1, r1, 1.0)),
        e(1, 2, line),
        e(2, 3, line),
        e(3, 4, circ(h1, r1, 1.0)),
        e(4, 5, line),
        e(5, 6, circ(h2, r2, -1.0)),
        e(6, 7, circ(h2, r2, -1.0)),
        e(7, 0, line),
    ];
    let faces = vec![BRepFace {
        surface: Surface::Cone {
            apex: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            half_angle: alpha,
        },
        outer_loop: (0..8).collect(),
        inner_loops: vec![],
        reversed: false,
    }];
    (verts, edges, faces)
}

#[test]
fn a_spike_under_a_rim_chord_is_reported_with_a_demand_by_one_pass() {
    let (verts, edges, faces) = spike_band();
    let mut n_used = None;
    let empty = std::collections::BTreeMap::new();
    let got = stage1_tessellate_once(
        &verts,
        &edges,
        &faces,
        &empty,
        &empty,
        &empty,
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
            assert!(crossings >= 1, "crossings {crossings}");
            assert!(n > cur, "demand {n} must exceed the pass's N {cur}");
            // sag(10.02, N) ≤ 0.0057/2 ⇒ N ≥ 133.
            let sag = |n: usize| 10.02 * (1.0 - (std::f64::consts::PI / n as f64).cos());
            assert!(
                sag(n) <= 0.00283 && (125..=145).contains(&n),
                "demand N={n}"
            );
        }
        Err(e) => panic!("expected a chart crossing with a demand, got {e:?}"),
        Ok(_) => panic!("expected a chart crossing with a demand, got a tessellation"),
    }
}

#[test]
fn the_driver_refines_the_spike_band_to_the_demand_and_tessellates() {
    use std::f64::consts::PI;
    let (verts, edges, faces) = spike_band();
    let t =
        stage1_tessellate(&verts, &edges, &faces).expect("the §4.5.4 retry lands the spike band");
    let alpha = PI / 4.0;
    let (h1, h2) = (10.0_f64, 10.02_f64);
    let h6 = h2 - 0.2 * (h2 - h1);
    let n_upper = t
        .verts
        .iter()
        .filter(|p| (p.as_array()[2] - h2).abs() < 1e-9)
        .count();
    assert!(
        n_upper >= 60,
        "rim B vertices {n_upper} (≥ 133 segments/turn ⇒ ≥ 67 on the half turn)"
    );
    // Band sector minus the spike triangle (base = rim A's 0.04 rad of arc
    // in the development, height = the spike's slant reach).
    let (l1, l2, l6) = (h1 / alpha.cos(), h2 / alpha.cos(), h6 / alpha.cos());
    let expect =
        0.5 * alpha.sin() * PI * (l2 * l2 - l1 * l1) - 0.5 * (l1 * 0.04 * alpha.sin()) * (l6 - l1);
    let mut area = 0.0;
    let mut cover: std::collections::BTreeMap<(u32, u32), u32> = Default::default();
    for tri in &t.tris {
        let p: Vec<[f64; 3]> = tri
            .iter()
            .map(|&i| t.verts[i as usize].as_array())
            .collect();
        let e1 = [p[1][0] - p[0][0], p[1][1] - p[0][1], p[1][2] - p[0][2]];
        let e2 = [p[2][0] - p[0][0], p[2][1] - p[0][1], p[2][2] - p[0][2]];
        let c = [
            e1[1] * e2[2] - e1[2] * e2[1],
            e1[2] * e2[0] - e1[0] * e2[2],
            e1[0] * e2[1] - e1[1] * e2[0],
        ];
        area += 0.5 * (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt();
        for k in 0..3 {
            let (x, y) = (tri[k], tri[(k + 1) % 3]);
            *cover.entry((x.min(y), x.max(y))).or_insert(0) += 1;
        }
    }
    assert!(
        (area - expect).abs() < 0.05 * expect,
        "spike band area {area} vs {expect}"
    );
    assert!(cover.values().all(|&c| c <= 2), "fold");
}
