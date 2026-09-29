//! KV14 Slice B ribbon — the periodic strip's seam meridian must be one every
//! encircling loop crosses EXACTLY ONCE (P0006, 2026-09-29,
//! `docs/yang_tail_triage.md`).
//!
//! P0006's op-5 auto-union rejects its accumulated body at Stage 1: the
//! circle boss's cylinder lateral is a periodic strip whose LOWER encircling
//! loop is the bottom rim with gear-tooth excursions — intersection curves
//! that double back in θ — and whose one window (a square boss) sits on the
//! far side. The seam is chosen from the WINDOW vertices' widest gap, which
//! knows nothing about where the encircling loops' EDGES run: it landed
//! inside an excursion the lower loop crosses three times, the `rem_euclid`
//! unroll wrapped the excursion's far vertex to the other end of the strip,
//! and the chart polygon crossed itself eight times (`Stage1ChartCrossing`,
//! demand `None` — a cylinder's rim chords are straight in its strip).
//!
//! The fixture below is a unit strip whose lower loop carries a TONGUE — up
//! a generator at 200°, BACK along an arc to 180° at z = 1, up again, then
//! FORWARD to 220° at z = 1.5 and down — so meridians in 180°…200° are
//! crossed three times; a small window near 10° makes the window-free wedge
//! wrap the whole strip and puts the gap-scan seam at 190°, inside the
//! tongue. RED before the fix (the loud STOP); the seam validator rescues a
//! meridian the loop crosses once.

use super::*;

/// Lower rim vertex azimuths (degrees), ascending; the tongue rises at
/// `TONGUE` (200°) and lands at the next azimuth (220°).
const BOTTOM_DEG: [f64; 12] = [
    0.0, 30.0, 60.0, 90.0, 120.0, 150.0, 200.0, 220.0, 250.0, 280.0, 310.0, 340.0,
];
const TONGUE: usize = 6;
/// Upper rim vertex azimuths.
const TOP_DEG: [f64; 12] = [
    0.0, 30.0, 60.0, 90.0, 120.0, 150.0, 180.0, 210.0, 240.0, 270.0, 300.0, 330.0,
];
/// The window's corner azimuths (degrees) at heights z; a diamond of four
/// LINE chords. Its widest window-free wedge is the wrap 15° → 365°, whose
/// midpoint 190° is inside the tongue's 180°…200° doubled-back span.
const WINDOW: [(f64, f64); 4] = [(5.0, 1.7), (10.0, 1.55), (15.0, 1.7), (10.0, 1.85)];

/// A unit cylinder strip of height 2 whose lower boundary carries a tongue:
/// generator up at 200° to z = 1, arc BACK to 180° at z = 1 (normal −z, so
/// the CCW-about-normal sweep is the 20° short way), generator up to z = 1.5,
/// arc FORWARD 180° → 220° at z = 1.5, generator down to the rim at 220°.
fn tongued_strip() -> (Vec<BRepVertex>, Vec<BRepEdge>, Vec<BRepFace>) {
    let on = |deg: f64, z: f64| Point3::new(deg.to_radians().cos(), deg.to_radians().sin(), z);
    let n = BOTTOM_DEG.len();
    let mut pts: Vec<Point3> = BOTTOM_DEG.iter().map(|&d| on(d, 0.0)).collect();
    // Tongue corners, in loop order after the 200° rim vertex.
    let t_a = pts.len() as u32; // (200°, 1.0)
    pts.push(on(200.0, 1.0));
    let t_b = pts.len() as u32; // (180°, 1.0)
    pts.push(on(180.0, 1.0));
    let t_c = pts.len() as u32; // (180°, 1.5)
    pts.push(on(180.0, 1.5));
    let t_d = pts.len() as u32; // (220°, 1.5)
    pts.push(on(220.0, 1.5));
    let top0 = pts.len() as u32;
    pts.extend(TOP_DEG.iter().map(|&d| on(d, 2.0)));
    let win0 = pts.len() as u32;
    pts.extend(WINDOW.iter().map(|&(d, z)| on(d, z)));
    let verts: Vec<BRepVertex> = pts.iter().map(|&point| BRepVertex { point }).collect();
    let arc = |start: u32, end: u32, z: f64, up: bool| BRepEdge {
        start,
        end,
        curve: Curve::Circle {
            center: Point3::new(0.0, 0.0, z),
            normal: Vector3::new(0.0, 0.0, if up { 1.0 } else { -1.0 }),
            radius: 1.0,
        },
    };
    let line = |start: u32, end: u32| BRepEdge {
        start,
        end,
        curve: Curve::LineSegment,
    };
    let mut edges: Vec<BRepEdge> = Vec::new();
    for k in 0..n {
        let (a, b) = (k as u32, ((k + 1) % n) as u32);
        if k == TONGUE {
            edges.push(line(a, t_a));
            edges.push(arc(t_a, t_b, 1.0, false));
            edges.push(line(t_b, t_c));
            edges.push(arc(t_c, t_d, 1.5, true));
            edges.push(line(t_d, b));
        } else {
            edges.push(arc(a, b, 0.0, true));
        }
    }
    let outer_loop: Vec<u32> = (0..edges.len() as u32).collect();
    let top_first = edges.len() as u32;
    let n_top = TOP_DEG.len();
    for k in 0..n_top {
        edges.push(arc(
            top0 + k as u32,
            top0 + ((k + 1) % n_top) as u32,
            2.0,
            true,
        ));
    }
    let win_first = edges.len() as u32;
    for k in 0..4u32 {
        edges.push(line(win0 + k, win0 + (k + 1) % 4));
    }
    let faces = vec![BRepFace {
        surface: Surface::Cylinder {
            axis_point: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            radius: 1.0,
        },
        outer_loop,
        inner_loops: vec![
            (top_first..top_first + n_top as u32).collect(),
            (win_first..win_first + 4).collect(),
        ],
        reversed: false,
    }];
    (verts, edges, faces)
}

fn mesh_area(t: &Stage1Tess) -> f64 {
    t.tris
        .iter()
        .map(|tri| {
            let a = t.verts[tri[0] as usize].as_array();
            let b = t.verts[tri[1] as usize].as_array();
            let c = t.verts[tri[2] as usize].as_array();
            let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let n = [
                e1[1] * e2[2] - e1[2] * e2[1],
                e1[2] * e2[0] - e1[0] * e2[2],
                e1[0] * e2[1] - e1[1] * e2[0],
            ];
            0.5 * (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt()
        })
        .sum()
}

/// Every mesh edge is covered once (boundary) or twice (interior).
fn boundary_edges(t: &Stage1Tess) -> usize {
    let mut count: std::collections::BTreeMap<(u32, u32), u32> = Default::default();
    for tri in &t.tris {
        for k in 0..3 {
            let (a, b) = (tri[k], tri[(k + 1) % 3]);
            *count.entry((a.min(b), a.max(b))).or_insert(0) += 1;
        }
    }
    assert!(
        count.values().all(|&c| c <= 2),
        "no edge is covered more than twice"
    );
    count.values().filter(|&&c| c == 1).count()
}

#[test]
fn tongued_strip_seam_avoids_the_doubled_back_excursion() {
    let (v, e, f) = tongued_strip();
    // RED before the fix (stash-certified): `Stage1ChartCrossing { face: 0,
    // crossings: 1, demand_n: None }` — the seam at 190° sat inside the
    // tongue.
    let t = stage1_tessellate(&v, &e, &f).expect("tongued strip tessellates");

    // Lateral area: the full 2π × 2 strip minus what the tongue removes —
    // between 180° and 200° the loop passes at z = 0 (rim), z = 1 (back arc)
    // and z = 1.5 (forward arc), so the slot [1, 1.5] is outside the face;
    // between 200° and 220° only the forward arc bounds it, so [0, 1.5] is
    // outside — minus the window (a diamond of chords: half the product of
    // its chart diagonals, 10°·r by 0.3).
    let deg = |d: f64| d.to_radians();
    let removed = deg(20.0) * 0.5 + deg(20.0) * 1.5;
    let window = 0.5 * deg(10.0) * 0.3;
    let exact = 2.0 * std::f64::consts::PI * 2.0 - removed - window;
    let area = mesh_area(&t);
    assert!(
        (area - exact).abs() / exact < 0.03,
        "area {area} vs exact {exact}"
    );
    assert!(boundary_edges(&t) > 0, "an open strip has a boundary");
}

/// Upper rim vertex azimuths for the stepped strip: the 42° arc 168° → 210°
/// splits in half at 189°, so the first upper vertex after the 190° seam is
/// 210° — the seam closure chord runs from (210°, 2) down to the step's
/// foot (195°, 0), through the step's top arc.
const TOP_DEG_STEP: [f64; 12] = [
    0.0, 30.0, 60.0, 90.0, 120.0, 150.0, 168.0, 210.0, 240.0, 270.0, 300.0, 330.0,
];
/// Lower rim vertex azimuths for the stepped strip: the step rises at 195°
/// and lands at 225°.
const BOTTOM_DEG_STEP: [f64; 12] = [
    0.0, 30.0, 60.0, 90.0, 120.0, 165.0, 195.0, 225.0, 255.0, 285.0, 315.0, 345.0,
];
const STEP: usize = 6;

/// A unit cylinder strip of height 2 whose lower boundary carries a raised
/// STEP — up a generator at 195° to z = 1, forward along an arc to 225°,
/// down to the rim — with the same window as `tongued_strip` so the gap
/// scan's seam is 190°. Every loop crosses that meridian exactly once, yet
/// the ribbon's seam closure (the chord from the upper chain's first
/// vertex, (210°, 2), to the lower chain's, (195°, 0)) passes through the
/// step's top arc: the un-minimized P0006 lineage's face 13 (a serpentine
/// wall beside the seam, two crossings).
fn stepped_strip() -> (Vec<BRepVertex>, Vec<BRepEdge>, Vec<BRepFace>) {
    let on = |deg: f64, z: f64| Point3::new(deg.to_radians().cos(), deg.to_radians().sin(), z);
    let n = BOTTOM_DEG_STEP.len();
    let mut pts: Vec<Point3> = BOTTOM_DEG_STEP.iter().map(|&d| on(d, 0.0)).collect();
    let s_a = pts.len() as u32;
    pts.push(on(BOTTOM_DEG_STEP[STEP], 1.0));
    let s_b = pts.len() as u32;
    pts.push(on(BOTTOM_DEG_STEP[STEP + 1], 1.0));
    let top0 = pts.len() as u32;
    pts.extend(TOP_DEG_STEP.iter().map(|&d| on(d, 2.0)));
    let win0 = pts.len() as u32;
    pts.extend(WINDOW.iter().map(|&(d, z)| on(d, z)));
    let verts: Vec<BRepVertex> = pts.iter().map(|&point| BRepVertex { point }).collect();
    let arc = |start: u32, end: u32, z: f64| BRepEdge {
        start,
        end,
        curve: Curve::Circle {
            center: Point3::new(0.0, 0.0, z),
            normal: Vector3::new(0.0, 0.0, 1.0),
            radius: 1.0,
        },
    };
    let line = |start: u32, end: u32| BRepEdge {
        start,
        end,
        curve: Curve::LineSegment,
    };
    let mut edges: Vec<BRepEdge> = Vec::new();
    for k in 0..n {
        let (a, b) = (k as u32, ((k + 1) % n) as u32);
        if k == STEP {
            edges.push(line(a, s_a));
            edges.push(arc(s_a, s_b, 1.0));
            edges.push(line(s_b, b));
        } else {
            edges.push(arc(a, b, 0.0));
        }
    }
    let outer_loop: Vec<u32> = (0..edges.len() as u32).collect();
    let top_first = edges.len() as u32;
    let n_top = TOP_DEG_STEP.len();
    for k in 0..n_top {
        edges.push(arc(top0 + k as u32, top0 + ((k + 1) % n_top) as u32, 2.0));
    }
    let win_first = edges.len() as u32;
    for k in 0..4u32 {
        edges.push(line(win0 + k, win0 + (k + 1) % 4));
    }
    let faces = vec![BRepFace {
        surface: Surface::Cylinder {
            axis_point: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            radius: 1.0,
        },
        outer_loop,
        inner_loops: vec![
            (top_first..top_first + n_top as u32).collect(),
            (win_first..win_first + 4).collect(),
        ],
        reversed: false,
    }];
    (verts, edges, faces)
}

#[test]
fn stepped_strip_seam_closure_clears_the_step() {
    let (v, e, f) = stepped_strip();
    // RED before the ribbon-simplicity rule: `Stage1ChartCrossing { face: 0,
    // crossings: 1, demand_n: None }` — the seam closure chord crossed the
    // step's top arc although every loop crossed the 190° meridian once.
    let t = stage1_tessellate(&v, &e, &f).expect("stepped strip tessellates");
    let deg = |d: f64| d.to_radians();
    let exact = 2.0 * std::f64::consts::PI * 2.0 - deg(30.0) * 1.0 - 0.5 * deg(10.0) * 0.3;
    let area = mesh_area(&t);
    assert!(
        (area - exact).abs() / exact < 0.03,
        "area {area} vs exact {exact}"
    );
    assert!(boundary_edges(&t) > 0, "an open strip has a boundary");
}
