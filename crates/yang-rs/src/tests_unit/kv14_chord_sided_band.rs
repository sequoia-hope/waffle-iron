//! KV14 (spec `yang_stage1_curved_holed_patch` "The strip arm's dispatch",
//! 2026-09-07 — R0032 face 3): a partial cylinder / cone band whose SIDES
//! are chord runs left by a prior boolean (a torus∩cone polyline crossing
//! the band obliquely) is NOT the structured 2-arc + 2-ruling wall. Its two
//! arcs sweep different azimuth ranges, so their shared chains carry
//! different sample counts at any real rim density, and the chord runs'
//! interior vertices are boundary vertices the strip never referenced. The
//! structured strip arm is the 4-edge wall whose arc chains pair
//! index-for-index; everything else is the Slice D/E chart CDT.

use super::*;
use std::f64::consts::PI;

/// The two surfaces the band lives on, with their isometric developments.
#[derive(Clone, Copy)]
enum Band {
    /// Cone: apex at the origin, axis +z, `tan α`.
    Cone { tan_a: f64 },
    /// Cylinder: axis z through the origin, `radius`.
    Cylinder { radius: f64 },
}

impl Band {
    fn on(self, theta: f64, z: f64) -> Point3 {
        let r = match self {
            Band::Cone { tan_a } => z * tan_a,
            Band::Cylinder { radius } => radius,
        };
        Point3::new(r * theta.cos(), r * theta.sin(), z)
    }
    fn surface(self) -> Surface {
        match self {
            Band::Cone { tan_a } => Surface::Cone {
                apex: Point3::new(0.0, 0.0, 0.0),
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                half_angle: tan_a.atan(),
            },
            Band::Cylinder { radius } => Surface::Cylinder {
                axis_point: Point3::new(0.0, 0.0, 0.0),
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                radius,
            },
        }
    }
    fn arc(self, start: u32, end: u32, z: f64, up: bool) -> BRepEdge {
        let radius = match self {
            Band::Cone { tan_a } => z * tan_a,
            Band::Cylinder { radius } => radius,
        };
        BRepEdge {
            start,
            end,
            curve: Curve::Circle {
                center: Point3::new(0.0, 0.0, z),
                normal: Vector3::new(0.0, 0.0, if up { 1.0 } else { -1.0 }),
                radius,
            },
        }
    }
    /// Isometric development of an on-surface point (θ, z): the cone's
    /// annular sector (`ℓ = z / cos α`, `ψ = θ · sin α`), the cylinder's
    /// rectangle (`u = r·θ`, `v = z`).
    fn develop(self, theta: f64, z: f64) -> [f64; 2] {
        match self {
            Band::Cone { tan_a } => {
                let a = tan_a.atan();
                let l = z / a.cos();
                let psi = theta * a.sin();
                [l * psi.cos(), l * psi.sin()]
            }
            Band::Cylinder { radius } => [radius * theta, z],
        }
    }
}

/// A band between rims at `z0` (bottom, CCW about +z over `[b_lo, b_hi]`)
/// and `z1` (top, traversed backwards over `[t_lo, t_hi]`), whose two sides
/// are `side_chords`-segment polylines of on-surface points interpolated
/// linearly in (θ, z) — the chord runs a prior boolean's intersection
/// polyline leaves across a band. Returns the B-Rep plus the polygon in the
/// development (for the exact area) and the arc edge indices.
struct Fixture {
    band: Band,
    verts: Vec<BRepVertex>,
    edges: Vec<BRepEdge>,
    faces: Vec<BRepFace>,
    dev_polygon: Vec<[f64; 2]>,
    bottom_arc: u32,
    top_arc: u32,
}

#[allow(clippy::too_many_arguments)]
fn chord_sided_band(
    band: Band,
    z0: f64,
    z1: f64,
    (b_lo, b_hi): (f64, f64),
    (t_lo, t_hi): (f64, f64),
    side_chords: usize,
) -> Fixture {
    let mut verts: Vec<BRepVertex> = Vec::new();
    let mut push = |theta: f64, z: f64| -> u32 {
        verts.push(BRepVertex {
            point: band.on(theta, z),
        });
        (verts.len() - 1) as u32
    };
    let b0 = push(b_lo, z0);
    let b1 = push(b_hi, z0);
    // Right side: (b_hi, z0) → (t_hi, z1) through `side_chords − 1` interior
    // on-surface points.
    let mut right: Vec<u32> = vec![b1];
    for k in 1..side_chords {
        let s = k as f64 / side_chords as f64;
        right.push(push(b_hi + s * (t_hi - b_hi), z0 + s * (z1 - z0)));
    }
    let t1 = push(t_hi, z1);
    right.push(t1);
    let t0 = push(t_lo, z1);
    // Left side: (t_lo, z1) → (b_lo, z0).
    let mut left: Vec<u32> = vec![t0];
    for k in 1..side_chords {
        let s = k as f64 / side_chords as f64;
        left.push(push(t_lo + s * (b_lo - t_lo), z1 + s * (z0 - z1)));
    }
    left.push(b0);

    let line = |start: u32, end: u32| BRepEdge {
        start,
        end,
        curve: Curve::LineSegment,
    };
    let mut edges: Vec<BRepEdge> = Vec::new();
    edges.push(band.arc(b0, b1, z0, true));
    let bottom_arc = 0u32;
    for w in right.windows(2) {
        edges.push(line(w[0], w[1]));
    }
    let top_arc = edges.len() as u32;
    edges.push(band.arc(t1, t0, z1, false));
    for w in left.windows(2) {
        edges.push(line(w[0], w[1]));
    }
    let outer: Vec<u32> = (0..edges.len() as u32).collect();
    let faces = vec![BRepFace {
        surface: band.surface(),
        outer_loop: outer,
        inner_loops: vec![],
        reversed: false,
    }];

    // The exact region in the development: arcs densely sampled, sides as
    // straight (θ, z)-interpolated runs (a chord's chart image differs from
    // the straight segment only at second order in its angular span).
    let mut dev_polygon: Vec<[f64; 2]> = Vec::new();
    let dense = 4000;
    for k in 0..=dense {
        let s = k as f64 / dense as f64;
        dev_polygon.push(band.develop(b_lo + s * (b_hi - b_lo), z0));
    }
    for k in 1..side_chords {
        let s = k as f64 / side_chords as f64;
        dev_polygon.push(band.develop(b_hi + s * (t_hi - b_hi), z0 + s * (z1 - z0)));
    }
    for k in 0..=dense {
        let s = k as f64 / dense as f64;
        dev_polygon.push(band.develop(t_hi + s * (t_lo - t_hi), z1));
    }
    for k in 1..side_chords {
        let s = k as f64 / side_chords as f64;
        dev_polygon.push(band.develop(t_lo + s * (b_lo - t_lo), z1 + s * (z0 - z1)));
    }
    Fixture {
        band,
        verts,
        edges,
        faces,
        dev_polygon,
        bottom_arc,
        top_arc,
    }
}

fn shoelace(poly: &[[f64; 2]]) -> f64 {
    let n = poly.len();
    let mut a = 0.0;
    for i in 0..n {
        let p = poly[i];
        let q = poly[(i + 1) % n];
        a += p[0] * q[1] - q[0] * p[1];
    }
    0.5 * a.abs()
}

fn tri_area(t: &Stage1Tess, tri: &[u32; 3]) -> f64 {
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
}

/// The patch is a bounded, watertight, conformal, outward-facing
/// tessellation of the face: its count-1 (boundary) mesh edges are EXACTLY
/// the loop's sampled segments — every arc-chain and chord-run vertex is on
/// the boundary and nothing else is — no edge is covered more than twice,
/// and the flat-triangle area fills the developed region from below.
fn assert_chord_sided_band(fx: &Fixture, t: &Stage1Tess) {
    assert!(!t.tris.is_empty(), "must produce triangles");
    // Expected boundary segment set from the shared chains + line edges.
    let f = &fx.faces[0];
    let mut expected: std::collections::BTreeSet<(u32, u32)> = Default::default();
    let key = |x: u32, y: u32| (x.min(y), x.max(y));
    for &e in &f.outer_loop {
        let ed = &fx.edges[e as usize];
        match ed.curve {
            Curve::Circle { .. } => {
                let chain = t.chains.get(&e).expect("arc chain built");
                assert_eq!(chain[0], ed.start);
                assert_eq!(*chain.last().unwrap(), ed.end);
                for w in chain.windows(2) {
                    expected.insert(key(w[0], w[1]));
                }
            }
            Curve::LineSegment => {
                expected.insert(key(ed.start, ed.end));
            }
            _ => unreachable!(),
        }
    }
    let mut count: std::collections::BTreeMap<(u32, u32), u32> = Default::default();
    for tri in &t.tris {
        for k in 0..3 {
            *count.entry(key(tri[k], tri[(k + 1) % 3])).or_insert(0) += 1;
        }
    }
    let boundary: std::collections::BTreeSet<(u32, u32)> = count
        .iter()
        .filter(|(_, &c)| c == 1)
        .map(|(&k, _)| k)
        .collect();
    assert!(
        count.values().all(|&c| c <= 2),
        "an edge is covered more than twice (fold / double cover)"
    );
    assert_eq!(
        boundary, expected,
        "the mesh boundary must be exactly the loop's sampled segments \
         (a chord-run vertex the strip skipped shows up here as a missing segment)"
    );
    // Every mesh vertex — boundary sample or lifted interior Steiner point —
    // lies ON the surface (the chart CDT lifts its Steiner points exactly).
    for (i, v) in t.verts.iter().enumerate() {
        let p = v.as_array();
        let rho = (p[0] * p[0] + p[1] * p[1]).sqrt();
        let r_surface = match fx.band {
            Band::Cone { tan_a } => p[2] * tan_a,
            Band::Cylinder { radius } => radius,
        };
        assert!(
            (rho - r_surface).abs() <= 1e-9,
            "vertex {i} {p:?} is off the surface by {}",
            rho - r_surface
        );
    }
    // No fold: every triangle has the same orientation in the isometric
    // development (a folded / overlapping facet would be reversed there and
    // is invisible to the edge-count oracle above).
    let dev = |v: u32| -> [f64; 2] {
        let p = t.verts[v as usize].as_array();
        fx.band.develop(p[1].atan2(p[0]), p[2])
    };
    let signed = |tri: &[u32; 3]| -> f64 {
        let a = dev(tri[0]);
        let b = dev(tri[1]);
        let c = dev(tri[2]);
        0.5 * ((b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1]))
    };
    let signs: Vec<f64> = t.tris.iter().map(signed).collect();
    assert!(
        signs.iter().all(|&s| s > 0.0) || signs.iter().all(|&s| s < 0.0),
        "a triangle is folded in the development: signed areas {signs:?}"
    );
    // Area sanity: the flat facets fill the developed region (the reference
    // draws the sides straight in the chart, a second-order approximation of
    // a 3D chord's image, so this is a 1% band, not an inscribed bound; a
    // double cover or a missing strip would be a whole facet away).
    let reference = shoelace(&fx.dev_polygon);
    let area: f64 = t.tris.iter().map(|tri| tri_area(t, tri)).sum();
    assert!(
        (area - reference).abs() <= 0.01 * reference,
        "patch area {area} vs developed region {reference}"
    );
    // Outward: every triangle's normal has a positive radial component.
    for tri in &t.tris {
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
        let cen = [(a[0] + b[0] + c[0]) / 3.0, (a[1] + b[1] + c[1]) / 3.0];
        assert!(
            n[0] * cen[0] + n[1] * cen[1] > 0.0,
            "triangle {tri:?} must face radially outward"
        );
    }
}

fn arc_samples(t: &Stage1Tess, e: u32) -> usize {
    t.chains.get(&e).map(|c| c.len()).unwrap_or(0)
}

const DEG: f64 = PI / 180.0;

/// R0032 face 3's shape on a cone: [A, L, L, L, A, L, L, L] — the bottom arc
/// sweeps 60°, the top 30°, the sides are 3-chord runs. At N = 36 the chains
/// carry 7 and 4 samples: no index pairing exists, and the strip's old
/// `mismatched sample counts` STOP was this loop's only exit.
#[test]
fn cone_chord_sided_band_unequal_sweeps_routes_to_chart_cdt() {
    let band = Band::Cone { tan_a: 0.5 };
    let fx = chord_sided_band(
        band,
        2.0,
        2.2,
        (0.0, 60.0 * DEG),
        (10.0 * DEG, 40.0 * DEG),
        3,
    );
    assert_eq!(fx.faces[0].outer_loop.len(), 8);
    let t = stage1_tessellate_min_segments(&fx.verts, &fx.edges, &fx.faces, Some(36))
        .expect("a chord-sided cone band tessellates through the chart CDT");
    assert_ne!(
        arc_samples(&t, fx.bottom_arc),
        arc_samples(&t, fx.top_arc),
        "fixture must exercise UNEQUAL arc chain counts"
    );
    assert_chord_sided_band(&fx, &t);
}

/// The silent half: equal sweeps (both 60°, the top shifted by 10°) give
/// equal chain counts, so the old strip arm paired the arcs index-for-index
/// and IGNORED the six chord-run vertices — a non-conformal mesh with cracks
/// along both sides. The boundary-segment oracle is the RED here.
#[test]
fn cone_chord_sided_band_equal_sweeps_keeps_every_chord_run_vertex() {
    let band = Band::Cone { tan_a: 0.5 };
    let fx = chord_sided_band(
        band,
        2.0,
        2.2,
        (0.0, 60.0 * DEG),
        (10.0 * DEG, 70.0 * DEG),
        3,
    );
    let t = stage1_tessellate_min_segments(&fx.verts, &fx.edges, &fx.faces, Some(36))
        .expect("a chord-sided cone band tessellates through the chart CDT");
    assert_eq!(
        arc_samples(&t, fx.bottom_arc),
        arc_samples(&t, fx.top_arc),
        "fixture must exercise EQUAL arc chain counts"
    );
    assert_chord_sided_band(&fx, &t);
}

/// A 4-edge [A, L, A, L] loop whose sides are single oblique chords (not
/// rulings): the arcs sweep 60° and 30°, so the chains cannot pair. The
/// strip's precondition is the index pairing, not the edge count alone.
#[test]
fn cone_four_edge_chord_sided_wall_with_unpairable_chains_routes_to_chart_cdt() {
    let band = Band::Cone { tan_a: 0.5 };
    let fx = chord_sided_band(
        band,
        2.0,
        2.2,
        (0.0, 60.0 * DEG),
        (10.0 * DEG, 40.0 * DEG),
        1,
    );
    assert_eq!(fx.faces[0].outer_loop.len(), 4);
    let t = stage1_tessellate_min_segments(&fx.verts, &fx.edges, &fx.faces, Some(36))
        .expect("a 4-edge chord-sided cone wall tessellates through the chart CDT");
    assert_ne!(arc_samples(&t, fx.bottom_arc), arc_samples(&t, fx.top_arc));
    assert_chord_sided_band(&fx, &t);
}

/// The cylinder twin of the unequal-sweep band: the partial-cylinder strip
/// arm shares the dispatch and the metric.
#[test]
fn cylinder_chord_sided_band_unequal_sweeps_routes_to_chart_cdt() {
    let band = Band::Cylinder { radius: 1.0 };
    let fx = chord_sided_band(
        band,
        0.0,
        0.25,
        (0.0, 60.0 * DEG),
        (10.0 * DEG, 40.0 * DEG),
        3,
    );
    assert_eq!(fx.faces[0].outer_loop.len(), 8);
    let t = stage1_tessellate_min_segments(&fx.verts, &fx.edges, &fx.faces, Some(36))
        .expect("a chord-sided cylinder band tessellates through the chart CDT");
    assert_ne!(arc_samples(&t, fx.bottom_arc), arc_samples(&t, fx.top_arc));
    assert_chord_sided_band(&fx, &t);
}

/// Cylinder, equal sweeps: the silent-crack half on the cylinder arm.
#[test]
fn cylinder_chord_sided_band_equal_sweeps_keeps_every_chord_run_vertex() {
    let band = Band::Cylinder { radius: 1.0 };
    let fx = chord_sided_band(
        band,
        0.0,
        0.25,
        (0.0, 60.0 * DEG),
        (10.0 * DEG, 70.0 * DEG),
        3,
    );
    let t = stage1_tessellate_min_segments(&fx.verts, &fx.edges, &fx.faces, Some(36))
        .expect("a chord-sided cylinder band tessellates through the chart CDT");
    assert_eq!(arc_samples(&t, fx.bottom_arc), arc_samples(&t, fx.top_arc));
    assert_chord_sided_band(&fx, &t);
}

/// The structured wall is untouched: a 4-edge [A, L, A, L] cone wall whose
/// sides ARE rulings (equal sweeps, equal chains) still takes the strip —
/// the same triangle set as before this change (2 triangles per chain step,
/// no interior Steiner vertices).
#[test]
fn cone_ruling_sided_wall_still_takes_the_strip() {
    let band = Band::Cone { tan_a: 0.5 };
    let fx = chord_sided_band(band, 2.0, 2.2, (0.0, 60.0 * DEG), (0.0, 60.0 * DEG), 1);
    assert_eq!(fx.faces[0].outer_loop.len(), 4);
    let t = stage1_tessellate_min_segments(&fx.verts, &fx.edges, &fx.faces, Some(36))
        .expect("a ruling-sided cone wall tessellates through the strip");
    let m = arc_samples(&t, fx.bottom_arc) - 1;
    assert_eq!(arc_samples(&t, fx.top_arc) - 1, m);
    assert_eq!(
        t.tris.len(),
        2 * m,
        "the strip emits two triangles per chain step"
    );
    assert!(
        t.sources
            .iter()
            .all(|s| !matches!(s, TessellationSource::BRepFace { .. })),
        "the strip mints no interior vertices"
    );
    assert_chord_sided_band(&fx, &t);
}
