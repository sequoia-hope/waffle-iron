//! P3b increment-1 unit fixtures: `line_edge_cylinder_face_pierce` contract
//! (spec `specs/yang_169_p3b_curved_partner_pierce.md` §3.1–3.2, §4).
//!
//! Fixtures use the canonical-tube builder [`rj_cylinder`] (2 full rims +
//! seam — the same vocabulary F0082's pierced face 2 presents) and drive the
//! primitive directly: it is UNWIRED this increment (wiring into
//! `junction_pierce_points` is increment 3, behind `YANG_P3B_PIERCE_ENABLE`).

use super::n2_junction::rj_cylinder;
use crate::boolean::line_edge_cylinder_face_pierce;
use crate::*;

/// The z-axis tube r=0.25, v∈[0,1] and its lateral-face index.
fn tube() -> (BRep, u32) {
    let b = rj_cylinder([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 0.25, 1.0);
    let f = b
        .faces()
        .iter()
        .position(|f| matches!(f.surface, Surface::Cylinder { .. }))
        .expect("fixture has a lateral face") as u32;
    (b, f)
}

/// Two planes whose intersection line carries the segment y=0.1, z=0.5 —
/// the owner edge's incident surfaces (n·x + d = 0 convention).
fn owner_planes() -> (Surface, Surface) {
    (
        Surface::Plane {
            normal: Vector3::new(0.0, 0.0, 1.0),
            d: -0.5,
        },
        Surface::Plane {
            normal: Vector3::new(0.0, 1.0, 0.0),
            d: -0.1,
        },
    )
}

/// A mid-height chord crossing the tube mints BOTH quadratic roots with the
/// analytic values: x = ∓√(r²−y²), transversality |n̂_x| = √(r²−y²)/r.
#[test]
fn transversal_chord_mints_both_roots_exactly() {
    let (y, f_idx) = tube();
    let (s1, s2) = owner_planes();
    let p0 = Point3::new(-1.0, 0.1, 0.5);
    let p1 = Point3::new(1.0, 0.1, 0.5);
    let out = line_edge_cylinder_face_pierce(p0, p1, s1, s2, f_idx, &y.faces()[f_idx as usize], &y);
    assert_eq!(out.len(), 2, "both crossings are genuine mints: {out:?}");
    let x_hit = (0.25f64 * 0.25 - 0.1 * 0.1).sqrt(); // 0.2291287847…
    let transv = x_hit / 0.25; // 0.9165151389…
    for (pp, expect_x) in out.iter().zip([-x_hit, x_hit]) {
        let p = pp.point.as_array();
        assert!(
            (p[0] - expect_x).abs() < 1e-15 && (p[1] - 0.1).abs() < 1e-15,
            "analytic pierce point, got {p:?} want x={expect_x}"
        );
        assert!((p[2] - 0.5).abs() < 1e-15);
        assert!(pp.t > 0.0 && pp.t < 1.0);
        assert!(
            (pp.transversality - transv).abs() < 1e-12,
            "radial transversality, got {}",
            pp.transversality
        );
        // On the cylinder exactly (the mint IS the junction).
        assert!(((p[0] * p[0] + p[1] * p[1]).sqrt() - 0.25).abs() < 1e-15);
    }
    assert!(out[0].t < out[1].t, "sorted by chord parameter");
    assert_eq!(out[0].partner_face, f_idx);
}

/// Tangential graze (chord at y = r): the radial normal at the touch point
/// is ⊥ the segment — the transversality floor rejects; nothing mints
/// (the #137 route, spec §3.1).
#[test]
fn tangential_graze_must_not_mint() {
    let (y, f_idx) = tube();
    let s1 = Surface::Plane {
        normal: Vector3::new(0.0, 0.0, 1.0),
        d: -0.5,
    };
    let s2 = Surface::Plane {
        normal: Vector3::new(0.0, 1.0, 0.0),
        d: -0.25,
    };
    let p0 = Point3::new(-1.0, 0.25, 0.5);
    let p1 = Point3::new(1.0, 0.25, 0.5);
    let out = line_edge_cylinder_face_pierce(p0, p1, s1, s2, f_idx, &y.faces()[f_idx as usize], &y);
    assert!(out.is_empty(), "graze must not mint: {out:?}");
}

/// Axial containment (spec §3.2): the same chord OUTSIDE the tube span and
/// ON a rim plane both fail closed — a rim-margin pierce is a rim corner.
#[test]
fn outside_span_and_rim_plane_must_not_mint() {
    let (y, f_idx) = tube();
    let s2 = Surface::Plane {
        normal: Vector3::new(0.0, 1.0, 0.0),
        d: -0.1,
    };
    for z in [1.5, -0.5, 1.0, 0.0] {
        let s1z = Surface::Plane {
            normal: Vector3::new(0.0, 0.0, 1.0),
            d: -z,
        };
        let p0 = Point3::new(-1.0, 0.1, z);
        let p1 = Point3::new(1.0, 0.1, z);
        let out =
            line_edge_cylinder_face_pierce(p0, p1, s1z, s2, f_idx, &y.faces()[f_idx as usize], &y);
        assert!(out.is_empty(), "z={z} must fail closed: {out:?}");
    }
}

/// Endpoint margin: an owner edge STARTING on the tube surface has its
/// near root at t=0 — a vertex-on-surface corner, not a mid-edge pierce;
/// the far root (t past the segment for this fixture) never mints either.
#[test]
fn endpoint_on_surface_must_not_mint_that_root() {
    let (y, f_idx) = tube();
    let s1 = Surface::Plane {
        normal: Vector3::new(0.0, 0.0, 1.0),
        d: -0.5,
    };
    let s2 = Surface::Plane {
        normal: Vector3::new(0.0, 1.0, 0.0),
        d: 0.0,
    };
    let p0 = Point3::new(0.25, 0.0, 0.5); // exactly on the tube
    let p1 = Point3::new(1.0, 0.0, 0.5);
    let out = line_edge_cylinder_face_pierce(p0, p1, s1, s2, f_idx, &y.faces()[f_idx as usize], &y);
    assert!(out.is_empty(), "endpoint contact must not mint: {out:?}");
}

/// Producer-fault guard: an owner surface that does NOT contain the pierce
/// point (the segment is not on it) rejects the mint loudly-by-absence.
#[test]
fn off_owner_surface_postcondition_must_not_mint() {
    let (y, f_idx) = tube();
    let s1 = Surface::Plane {
        normal: Vector3::new(0.0, 0.0, 1.0),
        d: -0.9, // z=0.9 plane — the z=0.5 segment is NOT on it
    };
    let s2 = Surface::Plane {
        normal: Vector3::new(0.0, 1.0, 0.0),
        d: -0.1,
    };
    let p0 = Point3::new(-1.0, 0.1, 0.5);
    let p1 = Point3::new(1.0, 0.1, 0.5);
    let out = line_edge_cylinder_face_pierce(p0, p1, s1, s2, f_idx, &y.faces()[f_idx as usize], &y);
    assert!(out.is_empty(), "off-surface owner must not mint: {out:?}");
}

/// Scope gates (spec §0): planar faces and non-canonical loop vocabularies
/// return empty — fail closed, never a guess.
#[test]
fn non_tube_faces_must_not_mint() {
    let (y, f_idx) = tube();
    let (s1, s2) = owner_planes();
    let p0 = Point3::new(-1.0, 0.1, 0.5);
    let p1 = Point3::new(1.0, 0.1, 0.5);
    // A cap (plane) face: wrong surface.
    let cap = y
        .faces()
        .iter()
        .position(|f| matches!(f.surface, Surface::Plane { .. }))
        .unwrap();
    assert!(
        line_edge_cylinder_face_pierce(p0, p1, s1, s2, cap as u32, &y.faces()[cap], &y).is_empty()
    );
    // A synthetic cylinder face with only ONE full rim in its outer loop:
    // outside the canonical-tube vocabulary (strip/holed = later widening).
    let lateral = &y.faces()[f_idx as usize];
    let one_rim = BRepFace {
        surface: lateral.surface,
        outer_loop: lateral
            .outer_loop
            .iter()
            .copied()
            .filter(|&ei| {
                let e = &y.edges()[ei as usize];
                !(matches!(e.curve, Curve::Circle { .. }) && e.start == e.end && e.start == 1)
            })
            .collect(),
        inner_loops: Vec::new(),
        reversed: lateral.reversed,
    };
    assert!(
        line_edge_cylinder_face_pierce(p0, p1, s1, s2, f_idx, &one_rim, &y).is_empty(),
        "one-rim vocabulary must fail closed"
    );
}
