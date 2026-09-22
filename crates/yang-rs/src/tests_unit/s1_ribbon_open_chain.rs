//! KV14 Slice B ribbon — opening an encircling loop into its u-ascending
//! chain is a function of the loop's WINDING, not of the anchor vertex's
//! neighbours (R0063, 2026-09-22, `docs/yang_tail_triage.md`).
//!
//! R0063's op-2 output is a cylinder whose lateral carries a rectangular
//! notch. The notch's bottom corner was the min-u anchor of the lower
//! encircling loop, and the notch WALL — a generator line — rises from it at
//! the same azimuth: the anchor's two neighbours tied in u to within one ulp,
//! and the old rule ("walk toward whichever neighbour continues upward")
//! decided the tie by rounding. Descending order lays the ribbon the long way
//! round at two heights, the chart polygon crosses itself, and Stage 1 STOPs
//! loud (`Stage1ChartCrossing`, demand `None`). The fixture below reproduces
//! the shape with an EXACT tie (identical azimuth bits on both notch corners)
//! and both loop senses; the tessellation must be orientation-agnostic.

use super::*;

/// The lower rim's vertex azimuths (degrees). Rims resample at N = 12 with a
/// floor of TWO pieces per arc (`max(2, ceil(sweep · 12 / 2π))`), so every
/// arc here splits in half: the 54° arc 155° → 209° into two 27° chords
/// (its midpoint sample at 182° reads as −178° from `atan2`, the smallest
/// boundary angle), every other arc into chords ≤ 25°.
const BOTTOM_DEG: [f64; 13] = [
    0.0, 25.0, 50.0, 75.0, 100.0, 125.0, 155.0, 209.0, 234.0, 259.0, 284.0, 309.0, 334.0,
];
/// Index into [`BOTTOM_DEG`] of the notch's first corner (209°); the notch
/// spans to the next azimuth (234°).
const NOTCH: usize = 7;
/// The upper rim's vertex azimuths. The seam is the midpoint of the widest
/// gap in the UNION of both rims' boundary angles (first such gap in sorted
/// order). The 170° vertex splits the lower arc's first half (155° → 182°)
/// so it cannot tie, and the 54° arc 182° → 236° puts its midpoint sample
/// exactly on the notch corner's azimuth (209°), leaving 182° → 209° the
/// unique widest gap (27°): the seam lands at 195.5° and the notch's 209°
/// corners are the min-u anchor.
const TOP_DEG: [f64; 14] = [
    0.0, 25.0, 50.0, 75.0, 100.0, 125.0, 155.0, 170.0, 182.0, 236.0, 259.0, 284.0, 309.0, 334.0,
];

/// A unit cylinder strip of height 2 with a 25°-wide, 1-high notch in its
/// lower boundary at azimuth 209°…234°. The notch WALL at 209° is a
/// generator line, so the anchor's two loop neighbours tie in u EXACTLY
/// (identical azimuth bits on both corners). `descending` lists the lower
/// loop's edges in the θ-decreasing order the R0063 B-Rep carries: the old
/// neighbour-comparison rule then walked the loop descending (the anchor's
/// successor was its twin), and the ribbon crossed itself.
fn notched_strip(descending: bool) -> (Vec<BRepVertex>, Vec<BRepEdge>, Vec<BRepFace>) {
    let on = |deg: f64, z: f64| Point3::new(deg.to_radians().cos(), deg.to_radians().sin(), z);
    let n = BOTTOM_DEG.len();
    let n_top = TOP_DEG.len();
    // 0..n bottom rim at z = 0; n, n+1 = the notch's raised corners at
    // (209°, 1) and (234°, 1); n+2.. top rim at z = 2.
    let mut pts: Vec<Point3> = BOTTOM_DEG.iter().map(|&d| on(d, 0.0)).collect();
    pts.push(on(BOTTOM_DEG[NOTCH], 1.0));
    pts.push(on(BOTTOM_DEG[NOTCH + 1], 1.0));
    let top0 = pts.len() as u32;
    pts.extend(TOP_DEG.iter().map(|&d| on(d, 2.0)));
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
        if k == NOTCH {
            edges.push(line(a, top0 - 2));
            edges.push(arc(top0 - 2, top0 - 1, 1.0));
            edges.push(line(top0 - 1, b));
        } else {
            edges.push(arc(a, b, 0.0));
        }
    }
    let mut outer_loop: Vec<u32> = (0..edges.len() as u32).collect();
    if descending {
        outer_loop.reverse();
    }
    let top_first = edges.len() as u32;
    for k in 0..n_top {
        edges.push(arc(top0 + k as u32, top0 + ((k + 1) % n_top) as u32, 2.0));
    }
    let faces = vec![BRepFace {
        surface: Surface::Cylinder {
            axis_point: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            radius: 1.0,
        },
        outer_loop,
        inner_loops: vec![(top_first..top_first + n_top as u32).collect()],
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

/// Boundary-edge census: every mesh edge is covered once (boundary) or twice
/// (interior); returns the boundary edge count.
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
fn notched_strip_opens_the_same_ribbon_in_both_loop_senses() {
    let (va, ea, fa) = notched_strip(false);
    let (vd, ed, fd) = notched_strip(true);
    let asc = stage1_tessellate(&va, &ea, &fa).expect("ascending lower loop tessellates");
    // RED before the fix: `Stage1ChartCrossing { face: 0, crossings: 2,
    // demand_n: None }` — the descending loop was opened descending.
    let desc = stage1_tessellate(&vd, &ed, &fd).expect("descending lower loop tessellates");

    // The lateral area of the notched strip: 2π·2 minus the 25°×1 notch, to
    // the N = 12 chord error (~1.1%).
    let exact = 2.0 * std::f64::consts::PI * 2.0 - (25f64.to_radians()) * 1.0;
    for (tag, t) in [("asc", &asc), ("desc", &desc)] {
        let area = mesh_area(t);
        assert!(
            (area - exact).abs() / exact < 0.02,
            "{tag}: area {area} vs exact {exact}"
        );
        assert!(boundary_edges(t) > 0, "{tag}: an open strip has a boundary");
    }
    // Orientation-agnostic: the same boundary and the same coverage.
    assert_eq!(asc.tris.len(), desc.tris.len(), "same triangle count");
    assert_eq!(boundary_edges(&asc), boundary_edges(&desc));
    assert!(
        (mesh_area(&asc) - mesh_area(&desc)).abs() < 1e-12,
        "same area to rounding"
    );
}
