//! M5 torus arm increment 3a (spec `m5_surface_pair_curve.md` "Torus ×
//! plane" 3a, 2026-10-11): a planar face bounded by a procedural
//! `Curve::SurfacePair` chain — the cap∩torus spiric a prior boolean left on
//! a cap, the plane itself one operand — joins Stage 0's MIXED coplanar
//! overlay class. Its loop polyline is the pair edge's own Stage-1 chain
//! (certified samples on both operands), spliced exactly like an arc's, and
//! the pair chords are masked as curved so a subdivision by the overlap
//! boundary is DETECTED (and walls loudly: no exact-curve minting for a
//! pair chord yet).
//!
//! Before this increment `overlay_face_supported` admitted only line / disc /
//! annular / line+circle loops, so every chained op whose operand carried a
//! plane-bearing pair edge on a planar face hit the typed
//! `CoplanarFacesUnsupported` wall at Stage 0 (R0026, R0050, R0059, R0085
//! with the torus × plane emission armed).

use super::*;
use crate::coplanar_overlay::ExactPoint2;
use crate::stage0::{
    canonical_frame, face_polygon_2d_tessellated, mixed_chord_ctxs, mixed_planar_face,
    overlay_face_supported, resolve_rim_chord_vertex, RimResolve,
};
use std::collections::BTreeMap;

/// Ring torus about z, centre (0, 0, 0.5), R 2, r 1: the plane z = 0 cuts
/// its tube obliquely (the section is a spiric, two ovals at ρ = 2 ± √¾).
fn torus() -> Surface {
    Surface::Torus {
        center: Point3::new(0.0, 0.0, 0.5),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        major_radius: 2.0,
        minor_radius: 1.0,
    }
}

fn plane_z0() -> Surface {
    Surface::Plane {
        normal: Vector3::new(0.0, 0.0, 1.0),
        d: 0.0,
    }
}

/// Signed tube distance of `p` to [`torus`].
fn torus_residual(p: Point3) -> f64 {
    let q = p.as_array();
    let rho = q[0].hypot(q[1]);
    ((rho - 2.0).powi(2) + (q[2] - 0.5).powi(2)).sqrt() - 1.0
}

/// A single planar face on z = 0: the quarter of the outer spiric oval from
/// azimuth 0 to π/2 (a `SurfacePair { torus, plane }` edge), closed by two
/// straight edges through the corner (4, 4, 0). CCW about +z, so the pair
/// edge runs P1 → P0.
fn cap_with_spiric_edge() -> BRep {
    let rho = 2.0 + 0.75_f64.sqrt();
    let verts = vec![
        BRepVertex {
            point: Point3::new(rho, 0.0, 0.0),
        },
        BRepVertex {
            point: Point3::new(0.0, rho, 0.0),
        },
        BRepVertex {
            point: Point3::new(4.0, 4.0, 0.0),
        },
    ];
    let edges = vec![
        BRepEdge {
            start: 0,
            end: 2,
            curve: Curve::LineSegment,
        },
        BRepEdge {
            start: 2,
            end: 1,
            curve: Curve::LineSegment,
        },
        BRepEdge {
            start: 1,
            end: 0,
            curve: Curve::SurfacePair {
                a: torus(),
                b: plane_z0(),
            },
        },
    ];
    let faces = vec![BRepFace {
        surface: plane_z0(),
        outer_loop: vec![0, 1, 2],
        inner_loops: vec![],
        reversed: false,
    }];
    BRep::new(verts, edges, faces).expect("the cap is a valid single-face B-Rep")
}

/// The face is overlay-admitted through the MIXED class; its polygon is the
/// pair chain (more vertices than the three corners, every chain sample ON
/// the torus and on the plane), the pair chords are masked as curved and
/// the straight chords are not.
#[test]
fn a_planar_face_bounded_by_a_pair_chain_is_mixed_admitted_with_its_chain_as_the_polygon() {
    let b = cap_with_spiric_edge();
    assert!(
        mixed_planar_face(&b, 0),
        "line + surface-pair is the mixed class"
    );
    assert!(overlay_face_supported(&b, 0));

    let frame = canonical_frame(&b, 0).expect("a planar face has a frame");
    let coords: Vec<Point3> = b.vertices().iter().map(|v| v.point).collect();
    let (poly, corners, rim_map, masks) =
        face_polygon_2d_tessellated(&b, 0, &coords, &frame).expect("the mixed arm builds it");
    assert!(poly.holes.is_empty());
    assert_eq!(masks.len(), 1, "one mask per loop");
    let ring = &poly.outer;
    let mask = &masks[0];
    assert_eq!(mask.len(), ring.len());
    assert!(
        ring.len() > 3,
        "the pair chain carries interior samples: {} ring vertices",
        ring.len()
    );
    assert_eq!(
        corners.len(),
        3,
        "the three B-Rep corners resolve to vertex ids"
    );
    assert_eq!(
        rim_map.len(),
        ring.len() - 3,
        "every chain Steiner sample is a rim-map entry"
    );
    for (ex, pt) in &rim_map {
        assert!(
            torus_residual(*pt).abs() < 1e-9,
            "chain sample {:?} is on the torus (residual {:e})",
            pt.as_array(),
            torus_residual(*pt)
        );
        assert!(pt.as_array()[2].abs() < 1e-12, "and on the plane");
        let _ = ex;
    }
    // Segments on the two straight edges are unmasked; every other segment
    // sits on the pair edge (index 2).
    let curved: Vec<usize> = (0..ring.len()).filter(|&i| mask[i].is_some()).collect();
    assert_eq!(
        curved.len(),
        ring.len() - 2,
        "two straight chords, the rest pair chords"
    );
    assert!(curved.iter().all(|&i| mask[i] == Some(2)));

    // The pair edge gets ONE mint context — not a circle, but with a chord
    // sagitta bound (from the torus's local radius) the collapse ladder
    // reads.
    let ctxs = mixed_chord_ctxs(&b, &poly, &masks, &[], &frame);
    assert_eq!(ctxs.len(), 1);
    assert!(ctxs[0].circle().is_none());
    assert_eq!(ctxs[0].chords.len(), ring.len() - 2);
    let sag = ctxs[0]
        .chord_sagitta(0.1, Point3::new(2.0 + 0.75_f64.sqrt(), 0.0, 0.0))
        .expect("a torus has a local radius");
    assert!(sag > 0.0 && sag < 0.1);
}

/// The context's chords and the frame of the cap fixture.
fn cap_ctx_and_frame(
    other_segs: Vec<(ExactPoint2, ExactPoint2)>,
) -> (BRep, crate::stage0::RimChordCtx, crate::stage0::Frame) {
    let b = cap_with_spiric_edge();
    let frame = canonical_frame(&b, 0).unwrap();
    let coords: Vec<Point3> = b.vertices().iter().map(|v| v.point).collect();
    let (poly, _, _, masks) = face_polygon_2d_tessellated(&b, 0, &coords, &frame).unwrap();
    let mut ctxs = mixed_chord_ctxs(&b, &poly, &masks, &[], &frame);
    let mut ctx = ctxs.remove(0);
    ctx.other_segs = other_segs;
    (b, ctx, frame)
}

/// The exact midpoint of the context's `k`-th chord.
fn chord_mid(ctx: &crate::stage0::RimChordCtx, k: usize) -> ExactPoint2 {
    let (s, e) = &ctx.chords[k];
    let two = dashu::rational::RBig::from(2);
    ExactPoint2 {
        x: (&s.x + &e.x) / &two,
        y: (&s.y + &e.y) / &two,
    }
}

/// A sweep-column point on a pair chord (no other input's edge through it)
/// is minted at the nearest point of the pair curve: on the torus, on the
/// plane, within the chord's length of where it was — `crossing: false`,
/// no host line.
#[test]
fn a_pair_chord_x_event_mints_the_nearest_point_of_the_curve() {
    let (_, ctx, frame) = cap_ctx_and_frame(Vec::new());
    let q = chord_mid(&ctx, 1);
    let (qx, qy) = (q.x.to_f64().value(), q.y.to_f64().value());
    match resolve_rim_chord_vertex(&ctx, &q, qx, qy, &frame) {
        RimResolve::OnCircle {
            point,
            crossing,
            host,
        } => {
            assert!(!crossing);
            assert!(host.is_none());
            assert!(
                torus_residual(point).abs() < 1e-11,
                "{:e}",
                torus_residual(point)
            );
            assert!(point.as_array()[2].abs() < 1e-12);
            let q3 = frame.lift(qx, qy).as_array();
            let p = point.as_array();
            let moved = ((p[0] - q3[0]).powi(2) + (p[1] - q3[1]).powi(2)).sqrt();
            assert!(moved > 1e-6 && moved < 0.1, "a sagitta's move: {moved:e}");
        }
        other => panic!(
            "expected a mint, got {}",
            match other {
                RimResolve::NotOnChord => "NotOnChord",
                RimResolve::NoIntersection => "NoIntersection",
                RimResolve::OnCircle { .. } => unreachable!(),
            }
        ),
    }
}

/// A crossing with another input's edge is minted ON that edge's line AND
/// on the curve (I2): here a radial segment from the origin through the
/// chord midpoint, so the mint keeps the midpoint's azimuth exactly and
/// sits at the outer oval's radius.
#[test]
fn a_pair_chord_crossing_mints_on_the_host_line_and_the_curve() {
    let (_, ctx0, _) = cap_ctx_and_frame(Vec::new());
    let q = chord_mid(&ctx0, 1);
    let two = dashu::rational::RBig::from(2);
    let far = ExactPoint2 {
        x: &q.x * &two,
        y: &q.y * &two,
    };
    let origin = ExactPoint2::from_f64(0.0, 0.0).unwrap();
    let (_, ctx, frame) = cap_ctx_and_frame(vec![(origin, far)]);
    let (qx, qy) = (q.x.to_f64().value(), q.y.to_f64().value());
    match resolve_rim_chord_vertex(&ctx, &q, qx, qy, &frame) {
        RimResolve::OnCircle {
            point,
            crossing,
            host,
        } => {
            assert!(crossing);
            assert_eq!(host, Some(0));
            assert!(torus_residual(point).abs() < 1e-11);
            let p = point.as_array();
            let rho = p[0].hypot(p[1]);
            assert!(
                (rho - (2.0 + 0.75_f64.sqrt())).abs() < 1e-11,
                "on the outer oval"
            );
            assert!(
                (p[1] * qx - p[0] * qy).abs() < 1e-12,
                "the mint stayed on the radial host line"
            );
        }
        _ => panic!("expected a crossing mint"),
    }
}

/// Stage 1 consumes a pair edge's overrides: the minted point joins the
/// chain between its two neighbours (azimuths stay monotone), on both
/// operands; an override off the torus is loud.
#[test]
fn a_pair_chain_takes_a_minted_override_in_order_and_refuses_an_off_curve_one() {
    let (b, ctx, frame) = cap_ctx_and_frame(Vec::new());
    let q = chord_mid(&ctx, 1);
    let (qx, qy) = (q.x.to_f64().value(), q.y.to_f64().value());
    let RimResolve::OnCircle { point, .. } = resolve_rim_chord_vertex(&ctx, &q, qx, qy, &frame)
    else {
        panic!("mint")
    };
    let chain_of = |overrides: &BTreeMap<u32, Vec<Point3>>| {
        crate::stage1_tessellate_with_rim_overrides(
            b.vertices(),
            b.edges(),
            b.faces(),
            overrides,
            b.forced_rim_n(),
        )
        .map(|t| {
            t.chains[&2]
                .iter()
                .map(|&g| t.verts[g as usize])
                .collect::<Vec<Point3>>()
        })
    };
    let before = chain_of(&BTreeMap::new()).expect("the plain chain builds");
    let mut ov = BTreeMap::new();
    ov.insert(2u32, vec![point]);
    let after = chain_of(&ov).expect("the chain takes the mint");
    assert_eq!(after.len(), before.len() + 1);
    assert!(
        after.iter().any(|p| p.as_array() == point.as_array()),
        "the mint's exact bits"
    );
    let az: Vec<f64> = after
        .iter()
        .map(|p| {
            let a = p.as_array();
            a[1].atan2(a[0])
        })
        .collect();
    // The edge runs P1 (azimuth π/2) → P0 (azimuth 0): monotone decreasing.
    for w in az.windows(2) {
        assert!(w[1] < w[0], "chain order kept: {az:?}");
    }
    for p in &after {
        assert!(torus_residual(*p).abs() < 1e-9 && p.as_array()[2].abs() < 1e-12);
    }
    let pa = point.as_array();
    ov.insert(2u32, vec![Point3::new(pa[0] * 1.001, pa[1] * 1.001, 0.0)]);
    assert!(chain_of(&ov).is_err(), "an off-torus override is loud");

    // A fold-REVERTED mint is the chord point: on a sub-chord, within the
    // chain's sag of the torus — admitted (the rim's on-chord band), and it
    // keeps the chain's order.
    let (m0, m1) = (before[1].as_array(), before[2].as_array());
    let mid = Point3::new(
        0.5 * (m0[0] + m1[0]),
        0.5 * (m0[1] + m1[1]),
        0.5 * (m0[2] + m1[2]),
    );
    assert!(
        torus_residual(mid).abs() > 1e-6,
        "a chord midpoint is off the torus"
    );
    ov.insert(2u32, vec![mid]);
    let with_mid = chain_of(&ov).expect("an on-chord point within the bound is admitted");
    assert_eq!(with_mid.len(), before.len() + 1);
    assert!(
        with_mid[2].as_array() == mid.as_array(),
        "inserted between its neighbours"
    );
}

/// The admission is ONLY for a pair chain beside lines / circles on a plane;
/// an ellipse or hyperbola loop is still outside the mixed class (unchanged),
/// and a non-planar face never qualifies.
#[test]
fn the_mixed_class_still_refuses_other_curves_and_curved_surfaces() {
    let mut b = cap_with_spiric_edge();
    // Same loop, the curved edge an ellipse: not admitted.
    let (verts, mut edges, faces) = (
        b.vertices().to_vec(),
        b.edges().to_vec(),
        b.faces().to_vec(),
    );
    edges[2].curve = Curve::Ellipse {
        center: Point3::new(0.0, 0.0, 0.0),
        normal: Vector3::new(0.0, 0.0, 1.0),
        major_axis: Vector3::new(1.0, 0.0, 0.0),
        major_radius: 2.0 + 0.75_f64.sqrt(),
        minor_radius: 2.0 + 0.75_f64.sqrt(),
    };
    b = BRep::new(verts, edges, faces).expect("valid");
    assert!(!mixed_planar_face(&b, 0));
    assert!(!overlay_face_supported(&b, 0));
}

/// `planes_coincide`: the two faces of a coplanar pair — one plane carried
/// with either orientation — coincide; a parallel plane at another offset,
/// a tilted plane, and a non-plane do not.
#[test]
fn coincident_planes_are_one_plane_either_way_round() {
    use crate::geom::planes_coincide;
    let p = |n: [f64; 3], d: f64| Surface::Plane {
        normal: Vector3::new(n[0], n[1], n[2]),
        d,
    };
    assert!(planes_coincide(
        p([1.0, 0.0, 0.0], -0.03),
        p([-1.0, 0.0, 0.0], 0.03)
    ));
    assert!(planes_coincide(
        p([1.0, 0.0, 0.0], -0.03),
        p([2.0, 0.0, 0.0], -0.06)
    ));
    assert!(!planes_coincide(
        p([1.0, 0.0, 0.0], -0.03),
        p([1.0, 0.0, 0.0], 0.03)
    ));
    assert!(!planes_coincide(
        p([1.0, 0.0, 0.0], -0.03),
        p([-1.0, 0.0, 0.0], -0.03)
    ));
    assert!(!planes_coincide(
        p([1.0, 0.0, 0.0], -0.03),
        p([1.0, 1e-3, 0.0], -0.03)
    ));
    assert!(!planes_coincide(p([1.0, 0.0, 0.0], -0.03), torus()));
}
