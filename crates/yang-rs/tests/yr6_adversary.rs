//! PR-YR6 ADVERSARY — independent verification of curved Surface/Curve
//! variants + loud rejection.
//!
//! This file is written by a fresh verifier who did NOT author the production
//! code or the GREEN/RED tests. Fixtures are built independently (NOT copied
//! from the in-lib `single_triangle_topology` helper) so we are not trusting
//! the author's fixture. It must NOT modify production code.
//!
//! Claims verified here:
//! (a) Loud rejection is reachable and returns the EXACT
//!     `Err(YangError::CurvedSurfaceNotYetSupported { face })` variant for ALL
//!     THREE curved surfaces (Sphere, Cylinder, Cone), with the CORRECT face
//!     index — including when the curved face is NOT face 0.
//! (f) The curve variants (`Circle`/`Ellipse`) construct and round-trip; they
//!     are types-only (no production handling) — exercised by construction.

use cad_primitives::{Point3, Vector3};
use yang_rs::{BRep, BRepEdge, BRepFace, BRepVertex, Curve, Surface, YangError};

fn pt(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// Build a single planar triangle's (verts, edges, faces) with a caller-chosen
/// surface. Independent reconstruction (not the in-lib helper). Triangle is the
/// CCW unit right triangle in the z=0 plane; a planar fixture passes the
/// degeneracy + winding checks before the surface match is hit.
fn one_triangle(surface: Surface) -> (Vec<BRepVertex>, Vec<BRepEdge>, Vec<BRepFace>) {
    let verts = vec![
        BRepVertex {
            point: pt(0.0, 0.0, 0.0),
        },
        BRepVertex {
            point: pt(2.0, 0.0, 0.0),
        },
        BRepVertex {
            point: pt(0.0, 2.0, 0.0),
        },
    ];
    let edges = vec![
        BRepEdge {
            start: 0,
            end: 1,
            curve: Curve::LineSegment,
        },
        BRepEdge {
            start: 1,
            end: 2,
            curve: Curve::LineSegment,
        },
        BRepEdge {
            start: 2,
            end: 0,
            curve: Curve::LineSegment,
        },
    ];
    let faces = vec![BRepFace {
        surface,
        outer_loop: vec![0, 1, 2],
        inner_loops: Vec::new(),
    }];
    (verts, edges, faces)
}

fn sphere() -> Surface {
    Surface::Sphere {
        center: pt(1.0, 1.0, 1.0),
        radius: 3.0,
    }
}
fn cylinder() -> Surface {
    Surface::Cylinder {
        axis_point: pt(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 2.0,
    }
}
fn cone() -> Surface {
    Surface::Cone {
        apex: pt(0.0, 0.0, 5.0),
        axis_dir: Vector3::new(0.0, 0.0, -1.0),
        half_angle: 0.4,
    }
}

// ---- (a) reachable + exact variant + correct index (face 0) ----

#[test]
fn adversary_sphere_face0_rejected_exact() {
    let (v, e, f) = one_triangle(sphere());
    let r = BRep::new(v, e, f);
    assert!(
        matches!(r, Err(YangError::CurvedSurfaceNotYetSupported { face: 0 })),
        "sphere face 0: expected CurvedSurfaceNotYetSupported {{ face: 0 }}, got {r:?}"
    );
}

#[test]
fn adversary_cylinder_face0_rejected_exact() {
    // PR-YR7 migration: a cylinder face on a *triangle* (no Circle rims) is no
    // longer CurvedSurfaceNotYetSupported — the cylinder lateral path is now
    // implemented, but this fixture lacks the lateral's 2 required Circle rim
    // edges, so it is rejected as MalformedTopology. It must STILL error loudly
    // (never silently succeed); only the error *kind* changed.
    let (v, e, f) = one_triangle(cylinder());
    let r = BRep::new(v, e, f);
    assert!(
        matches!(r, Err(YangError::MalformedTopology(_))),
        "cylinder face 0 on a triangle: expected MalformedTopology (lateral lacks \
         its 2 Circle rim edges), got {r:?}"
    );
}

#[test]
fn adversary_cone_face0_rejected_exact() {
    let (v, e, f) = one_triangle(cone());
    let r = BRep::new(v, e, f);
    assert!(
        matches!(r, Err(YangError::CurvedSurfaceNotYetSupported { face: 0 })),
        "cone face 0: expected CurvedSurfaceNotYetSupported {{ face: 0 }}, got {r:?}"
    );
}

// ---- (a) never Ok, never panic ----

#[test]
fn adversary_curved_never_ok() {
    for s in [sphere(), cylinder(), cone()] {
        let (v, e, f) = one_triangle(s);
        let r = BRep::new(v, e, f);
        assert!(r.is_err(), "curved face must never be Ok, got {r:?}");
    }
}

// ---- (a) correct index when curved face is NOT face 0 ----

/// A 3-face B-Rep: faces 0 and 1 are valid planar triangles, face 2 carries a
/// curved surface. The error must report `face: 2` (the actual offending face),
/// not a hardcoded 0. Each face has its own 3 verts + 3 edges so the loops are
/// well-formed; planar faces must pass degeneracy/winding before the curved
/// face is reached.
fn three_faces_curved_at_2(curved: Surface) -> (Vec<BRepVertex>, Vec<BRepEdge>, Vec<BRepFace>) {
    let mut verts = Vec::new();
    let mut edges = Vec::new();
    let mut faces = Vec::new();
    // Three planar triangles stacked at distinct z, each a separate vertex set.
    let surfaces = [
        Surface::Plane {
            normal: Vector3::new(0.0, 0.0, 1.0),
            d: 0.0,
        },
        Surface::Plane {
            normal: Vector3::new(0.0, 0.0, 1.0),
            d: -1.0,
        },
        curved,
    ];
    for (i, surf) in surfaces.into_iter().enumerate() {
        let base = (i as u32) * 3;
        let z = i as f64;
        verts.push(BRepVertex {
            point: pt(0.0, 0.0, z),
        });
        verts.push(BRepVertex {
            point: pt(2.0, 0.0, z),
        });
        verts.push(BRepVertex {
            point: pt(0.0, 2.0, z),
        });
        let e0 = edges.len() as u32;
        edges.push(BRepEdge {
            start: base,
            end: base + 1,
            curve: Curve::LineSegment,
        });
        edges.push(BRepEdge {
            start: base + 1,
            end: base + 2,
            curve: Curve::LineSegment,
        });
        edges.push(BRepEdge {
            start: base + 2,
            end: base,
            curve: Curve::LineSegment,
        });
        faces.push(BRepFace {
            surface: surf,
            outer_loop: vec![e0, e0 + 1, e0 + 2],
            inner_loops: Vec::new(),
        });
    }
    (verts, edges, faces)
}

#[test]
fn adversary_curved_face2_reports_index_2() {
    // PR-YR7 migration: sphere/cone still report CurvedSurfaceNotYetSupported
    // at the offending face index 2. The cylinder is now implemented for the
    // proper seam-edge encoding, but THIS fixture's face 2 is a cylinder on a
    // *triangle* (no Circle rims), so it is rejected as MalformedTopology. The
    // intent — error loudly, not silently succeed — is preserved; only the
    // cylinder's error kind changed.
    for curved in [sphere(), cone()] {
        let (v, e, f) = three_faces_curved_at_2(curved);
        let r = BRep::new(v, e, f);
        assert!(
            matches!(r, Err(YangError::CurvedSurfaceNotYetSupported { face: 2 })),
            "curved face at index 2 must report face: 2 (not hardcoded 0), got {r:?}"
        );
    }

    // Cylinder arm: now MalformedTopology (lateral on a triangle lacks its 2
    // Circle rim edges). Still a loud error, never Ok.
    let (v, e, f) = three_faces_curved_at_2(cylinder());
    let r = BRep::new(v, e, f);
    assert!(
        matches!(r, Err(YangError::MalformedTopology(_))),
        "cylinder face at index 2 on a triangle: expected MalformedTopology, got {r:?}"
    );
}

/// Control: the SAME 3-face fixture with ALL planar faces must succeed —
/// proves faces 0/1 are well-formed and the rejection above is genuinely the
/// curved face at index 2 doing the work, not malformed topology.
#[test]
fn adversary_three_planar_faces_ok() {
    let (v, e, f) = three_faces_curved_at_2(Surface::Plane {
        normal: Vector3::new(0.0, 0.0, 1.0),
        d: -2.0,
    });
    let r = BRep::new(v, e, f);
    assert!(
        r.is_ok(),
        "all-planar 3-face B-Rep must construct, got {r:?}"
    );
}

// ---- (f) Curve variants construct + round-trip (types-only) ----

#[test]
fn adversary_curve_circle_roundtrip() {
    let c = Curve::Circle {
        center: pt(1.0, 2.0, 3.0),
        normal: Vector3::new(0.0, 0.0, 1.0),
        radius: 4.0,
    };
    match c {
        Curve::Circle {
            center,
            normal,
            radius,
        } => {
            assert_eq!(center, pt(1.0, 2.0, 3.0));
            assert_eq!(normal, Vector3::new(0.0, 0.0, 1.0));
            assert_eq!(radius, 4.0);
        }
        other => panic!("expected Circle, got {other:?}"),
    }
}

#[test]
fn adversary_curve_ellipse_roundtrip() {
    let c = Curve::Ellipse {
        center: pt(0.0, 0.0, 0.0),
        normal: Vector3::new(0.0, 0.0, 1.0),
        major_axis: Vector3::new(1.0, 0.0, 0.0),
        major_radius: 5.0,
        minor_radius: 2.0,
    };
    match c {
        Curve::Ellipse {
            center,
            normal,
            major_axis,
            major_radius,
            minor_radius,
        } => {
            assert_eq!(center, pt(0.0, 0.0, 0.0));
            assert_eq!(normal, Vector3::new(0.0, 0.0, 1.0));
            assert_eq!(major_axis, Vector3::new(1.0, 0.0, 0.0));
            assert_eq!(major_radius, 5.0);
            assert_eq!(minor_radius, 2.0);
        }
        other => panic!("expected Ellipse, got {other:?}"),
    }
}
