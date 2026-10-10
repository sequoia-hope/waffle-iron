//! Yang §4.5.2 on the certificate's own sites — the LOCAL form of the
//! boundary-point domain ladder (spec
//! `specs/yang_45_boundary_point_domain_certificate.md` §8, P0031,
//! 2026-10-10).
//!
//! A §4.5 domain fire names an erroneous region: the vertex that converged
//! outside its face's domain, its exact post position, and the faces its
//! triangles carry. The paper raises the resolution of "the parametric
//! surfaces associated with the erroneous regions" and a ring of their
//! neighbours — not the whole body. `domain_fire_local_rim_overrides` spends
//! that demand as an apex-centred lens of extra rim samples on every
//! cylinder / cone face a fired vertex sits on, over the face's coaxial rim
//! closure, full rims and ARCS alike, so a chained operand's arc-bounded
//! strip (P0031's A) is served and its two arc chains stay paired
//! index-for-index.

use super::*;
use crate::stage4_correct::DomainFire;

fn fire_at(post: [f64; 3], incident: Vec<(InputId, u32)>) -> DomainFire {
    DomainFire {
        v: 0,
        input: incident[0].0,
        face: incident[0].1,
        edge: 0,
        f_pre: -1.0e-2,
        f_post: 1.0e-2,
        pre: post,
        post,
        incident,
    }
}

/// Azimuths about +z of points on a z-axis cylinder, sorted.
fn azimuths(pts: &[Point3]) -> Vec<f64> {
    let mut a: Vec<f64> = pts
        .iter()
        .map(|p| {
            let q = p.as_array();
            q[1].atan2(q[0])
        })
        .collect();
    a.sort_by(f64::total_cmp);
    a
}

/// A partial cylinder strip `[Arc, Line, Arc, Line]` about +z of radius
/// `r`, sweeping CCW from `theta_a` to `theta_b`, between `z0` and `z1` —
/// the shape a previous boolean leaves of a cylinder lateral (P0031's A
/// face 0). Bottom arc CCW about +z, top arc traversed backwards (stored
/// normal −z), the kv14 convention the strip arm pairs index-for-index.
fn strip(r: f64, z0: f64, z1: f64, theta_a: f64, theta_b: f64) -> BRep {
    let on = |t: f64, z: f64| Point3::new(r * t.cos(), r * t.sin(), z);
    let verts = vec![
        BRepVertex {
            point: on(theta_a, z0),
        },
        BRepVertex {
            point: on(theta_b, z0),
        },
        BRepVertex {
            point: on(theta_b, z1),
        },
        BRepVertex {
            point: on(theta_a, z1),
        },
    ];
    let edges = vec![
        BRepEdge {
            start: 0,
            end: 1,
            curve: Curve::Circle {
                center: Point3::new(0.0, 0.0, z0),
                normal: Vector3::new(0.0, 0.0, 1.0),
                radius: r,
            },
        },
        BRepEdge {
            start: 1,
            end: 2,
            curve: Curve::LineSegment,
        },
        BRepEdge {
            start: 2,
            end: 3,
            curve: Curve::Circle {
                center: Point3::new(0.0, 0.0, z1),
                normal: Vector3::new(0.0, 0.0, -1.0),
                radius: r,
            },
        },
        BRepEdge {
            start: 3,
            end: 0,
            curve: Curve::LineSegment,
        },
    ];
    let faces = vec![BRepFace {
        surface: Surface::Cylinder {
            axis_point: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            radius: r,
        },
        outer_loop: vec![0, 1, 2, 3],
        inner_loops: vec![],
        reversed: false,
    }];
    BRep::new(verts, edges, faces).expect("the strip is a valid single-face B-Rep")
}

fn full_cylinder() -> BRep {
    let (v, e, f) = boolean_functional::rt_cylinder(0.0, 4.0, 10.0);
    BRep::new(v, e, f).expect("the cylinder is a valid B-Rep")
}

/// A fire on a full-rim cylinder lenses BOTH rims of its closure with the
/// identical azimuth set — the apex (the fire's own azimuth) and `d − 1`
/// uniformly spaced samples each side at the natural step over `d` — and
/// nothing on the other operand. The next rung halves the spacing.
#[test]
fn a_full_rim_fire_lenses_both_rims_identically_and_the_rungs_halve_the_step() {
    let a = full_cylinder();
    let b = full_cylinder();
    let apex = 1.0_f64;
    let fire = fire_at(
        [10.0 * apex.cos(), 10.0 * apex.sin(), 0.0],
        vec![(InputId::A, 0), (InputId::A, 1)],
    );
    let mut spacing_at: Vec<f64> = Vec::new();
    for d in [2usize, 4] {
        let (ma, mb) = domain_fire_local_rim_overrides(&a, &b, std::slice::from_ref(&fire), d);
        assert!(mb.is_empty(), "the fire names no face of B");
        assert_eq!(
            ma.keys().copied().collect::<Vec<u32>>(),
            vec![0, 1],
            "both rims of the tube closure take the lens (divisor {d})"
        );
        let bottom = azimuths(&ma[&0]);
        let top = azimuths(&ma[&1]);
        assert_eq!(bottom.len(), 2 * d - 1, "2d − 1 samples per rim (d = {d})");
        for (x, y) in bottom.iter().zip(&top) {
            assert!((x - y).abs() < 1e-12, "the two rims share one azimuth set");
        }
        assert!(
            bottom.iter().any(|t| (t - apex).abs() < 1e-12),
            "the apex is the fire's own azimuth"
        );
        let steps: Vec<f64> = bottom.windows(2).map(|w| w[1] - w[0]).collect();
        let s0 = steps[0];
        for s in &steps {
            assert!((s - s0).abs() < 1e-12, "uniform spacing inside the lens");
        }
        spacing_at.push(s0);
        for e in [0u32, 1] {
            for p in &ma[&e] {
                let q = p.as_array();
                let r = q[0].hypot(q[1]);
                assert!(
                    (r - 10.0).abs() < 1e-12,
                    "every sample is ON the rim circle"
                );
                let z = if e == 0 { 0.0 } else { 4.0 };
                assert!((q[2] - z).abs() < 1e-12, "and in its rim's plane");
            }
        }
    }
    assert!(
        (spacing_at[0] - 2.0 * spacing_at[1]).abs() < 1e-12,
        "divisor 4 samples at half the spacing of divisor 2: {spacing_at:?}"
    );
}

/// A fire whose faces are all planar names no surface to refine: the local
/// form derives nothing and leaves the op to the body-wide ladder.
#[test]
fn a_fire_on_planar_faces_only_derives_no_lens() {
    let a = full_cylinder();
    let b = full_cylinder();
    let fire = fire_at([3.0, 4.0, 0.0], vec![(InputId::A, 1), (InputId::A, 2)]);
    let (ma, mb) = domain_fire_local_rim_overrides(&a, &b, &[fire], 2);
    assert!(ma.is_empty() && mb.is_empty());
}

/// The arc-admitting closure names a strip's two arcs; the full-rim closure
/// (the #195 arms' vocabulary) sees nothing on the same face.
#[test]
fn the_closure_with_arcs_names_the_strips_two_chains() {
    let s = strip(10.0, 0.0, 4.0, -2.0, -1.0);
    assert_eq!(
        coaxial_rim_closure(&s, 0, [0.0; 3], [0.0, 0.0, 1.0]),
        Some(vec![]),
        "no full rim on an arc-bounded strip"
    );
    assert_eq!(
        coaxial_circle_closure_with_arcs(&s, 0, [0.0; 3], [0.0, 0.0, 1.0]),
        Some(vec![0, 2]),
        "both arcs of the strip are the band that moves together"
    );
}

/// On an arc-bounded strip the lens lands on BOTH arcs with one azimuth
/// set, every sample strictly inside the sweep; a lens that would reach past
/// an arc's end is clipped (the strip arm refuses an override at or beyond
/// an endpoint), and the strip rebuilds with the lens in place.
#[test]
fn an_arc_strip_takes_the_lens_on_both_chains_clipped_to_its_sweep() {
    let (ta, tb) = (-2.0_f64, -1.0_f64);
    let s = strip(10.0, 0.0, 4.0, ta, tb);
    let b = full_cylinder();
    // Mid-arc: the whole lens fits.
    let mid = -1.5_f64;
    let fire = fire_at(
        [10.0 * mid.cos(), 10.0 * mid.sin(), 0.0],
        vec![(InputId::A, 0)],
    );
    let (ma, mb) = domain_fire_local_rim_overrides(&s, &b, &[fire], 2);
    assert!(mb.is_empty());
    assert_eq!(ma.keys().copied().collect::<Vec<u32>>(), vec![0, 2]);
    let bottom = azimuths(&ma[&0]);
    let top = azimuths(&ma[&2]);
    assert_eq!(bottom, top, "identical azimuth sets on the two chains");
    assert_eq!(bottom.len(), 3, "the full lens fits mid-arc");
    for t in &bottom {
        assert!(
            *t > ta && *t < tb,
            "sample {t} inside the sweep ({ta}, {tb})"
        );
    }
    let before = s.mesh.verts.len();
    let refined = s
        .rebuilt_with_rim_overrides(&ma)
        .expect("Stage 1 takes the lens on both arcs of the strip");
    assert!(
        refined.mesh.verts.len() > before,
        "the lens adds samples: {before} -> {}",
        refined.mesh.verts.len()
    );

    // Near the start end: the outward sample past `ta` is clipped on BOTH
    // chains, so they still pair.
    let near = ta + 0.05;
    let fire = fire_at(
        [10.0 * near.cos(), 10.0 * near.sin(), 0.0],
        vec![(InputId::A, 0)],
    );
    let (ma, _) = domain_fire_local_rim_overrides(&s, &b, &[fire], 2);
    let bottom = azimuths(&ma[&0]);
    let top = azimuths(&ma[&2]);
    assert_eq!(bottom, top);
    assert_eq!(
        bottom.len(),
        2,
        "one outward sample fell past the arc start"
    );
    for t in &bottom {
        assert!(
            *t > ta && *t < tb,
            "sample {t} inside the sweep ({ta}, {tb})"
        );
    }
    s.rebuilt_with_rim_overrides(&ma)
        .expect("a clipped lens rebuilds too");
}

/// The budget: many fires on one closure (R0070's shape — 90 fires on a
/// revolved gear) would propagate their lenses along the whole coaxial
/// closure; over `LOCAL_452_MAX_SAMPLES_PER_N · N` the lens is dropped and
/// the op keeps the body-wide ladder.
#[test]
fn a_closure_over_the_budget_derives_nothing() {
    let a = full_cylinder();
    let b = full_cylinder();
    let fires: Vec<DomainFire> = (0..60)
        .map(|k| {
            let t = k as f64 * 0.1;
            fire_at(
                [10.0 * t.cos(), 10.0 * t.sin(), 0.0],
                vec![(InputId::A, 0), (InputId::A, 1)],
            )
        })
        .collect();
    let (ma, mb) = domain_fire_local_rim_overrides(&a, &b, &fires, 2);
    assert!(
        ma.is_empty() && mb.is_empty(),
        "180 samples on an N ≈ 13 tube is over 4·N"
    );
    // Two fires stay inside it (P0031's spend: 6 of 56).
    let (ma, _) = domain_fire_local_rim_overrides(&a, &b, &fires[..2], 2);
    assert_eq!(ma.len(), 2);
}
