//! Yang §4.5 boundary-point DOMAIN certificate (spec
//! `specs/yang_45_boundary_point_domain_certificate.md`): the creases that
//! bound a face, read from the operand's B-Rep edges, and the predicate that
//! says a relocated model-edge crossing left its face across one of them.
//!
//! Measured pins: P0003 (2026-09-28), both fires. The cutter's end-cap rim
//! (a `Circle` edge of B) crossing the boss's lateral face relocated onto the
//! exact circle × lateral-plane root 9.9e-4 above the boss's top cap, past
//! the lateral face's top edge; the boss's top edge crossing B's torus facet
//! relocated onto the exact line × torus root inside the revolve's open
//! wedge, past the torus face's rim circle. The numbers below are the mesh
//! dump's own (`YANG_MESH_DUMP=1`, stages `s4-entry` / `after-reloc`).

use crate::stage4_boundary_curve::{
    boundary_crease_crossed, crease_divider, CreaseExtent, CreaseIndex, FaceCrease,
};
use crate::{BRep, BRepEdge, BRepFace, BRepVertex, Curve, Surface, Vector3};
use cad_primitives::Point3;

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// The boss's lateral face for sketch edge 4→5 (P0003, sketch `x = −u`,
/// `z = v`): a vertical plane through (0.016756, ·, −0.015697) and
/// (0.005357, ·, −0.044479).
fn boss_lateral_45() -> Surface {
    let (x0, z0) = (0.016755650999796708_f64, -0.015697444364386564_f64);
    let (x1, z1) = (0.005356976776709738_f64, -0.04447856562226117_f64);
    // In-plane direction (dx, 0, dz); normal ⊥ it in the xz plane.
    let (dx, dz) = (x1 - x0, z1 - z0);
    let l = (dx * dx + dz * dz).sqrt();
    let n = [dz / l, 0.0, -dx / l];
    Surface::Plane {
        normal: Vector3::new(n[0], n[1], n[2]),
        d: -(n[0] * x0 + n[2] * z0),
    }
}

fn boss_top() -> Surface {
    Surface::Plane {
        normal: Vector3::new(0.0, 1.0, 0.0),
        d: -0.05,
    }
}

/// The top edge of that lateral face: the crease it must not be left across.
fn lateral_top_edge_crease() -> FaceCrease {
    let verts = vec![
        BRepVertex {
            point: p(0.016755650999796708, 0.05, -0.015697444364386564),
        },
        BRepVertex {
            point: p(0.005356976776709738, 0.05, -0.04447856562226117),
        },
    ];
    let e = BRepEdge {
        start: 0,
        end: 1,
        curve: Curve::LineSegment,
    };
    let (divider, extent) = crease_divider(&e, &verts, boss_lateral_45()).expect("line crease");
    FaceCrease {
        divider,
        s_own: boss_lateral_45(),
        s_other: Some(boss_top()),
        extent,
        edge: 0,
    }
}

/// P0003's cutter: the 340° torus (axis x through (0.042, 0.12, −0.058),
/// R 0.1105, r 0.049) and its end-cap rim circle at 20° from −y toward +z.
fn cutter_torus() -> Surface {
    Surface::Torus {
        center: p(0.042, 0.12, -0.058),
        axis_dir: Vector3::new(1.0, 0.0, 0.0),
        major_radius: 0.1105,
        minor_radius: 0.049,
    }
}

fn cutter_rim_crease() -> FaceCrease {
    let a = 20.0_f64.to_radians();
    // The tube centre at angle 20°: c + R·(0, −cos a, sin a).
    let center = p(0.042, 0.12 - 0.1105 * a.cos(), -0.058 + 0.1105 * a.sin());
    // The end-cap plane contains the axis (x) and that radial direction.
    let normal = Vector3::new(0.0, -a.sin(), -a.cos());
    let e = BRepEdge {
        start: 0,
        end: 0,
        curve: Curve::Circle {
            center,
            normal,
            radius: 0.049,
        },
    };
    let (divider, extent) = crease_divider(&e, &[], cutter_torus()).expect("circle crease");
    let cap = Surface::Plane {
        normal,
        d: -(normal.as_array()[1] * center.as_array()[1]
            + normal.as_array()[2] * center.as_array()[2]),
    };
    FaceCrease {
        divider,
        s_own: cutter_torus(),
        s_other: Some(cap),
        extent,
        edge: 1,
    }
}

#[test]
fn line_crease_divider_contains_the_edge_and_is_transverse_to_the_face() {
    let c = lateral_top_edge_crease();
    let Surface::Plane { normal, d } = c.divider else {
        panic!("divider is a plane");
    };
    let n = normal.as_array();
    let CreaseExtent::Segment { p0, p1 } = c.extent else {
        panic!("segment extent");
    };
    for q in [p0, p1] {
        let qa = q.as_array();
        assert!((n[0] * qa[0] + n[1] * qa[1] + n[2] * qa[2] + d).abs() < 1e-15);
    }
    // Transverse: the divider's normal is orthogonal to the face normal.
    let Surface::Plane { normal: fnrm, .. } = boss_lateral_45() else {
        unreachable!()
    };
    let f = fnrm.as_array();
    assert!((n[0] * f[0] + n[1] * f[1] + n[2] * f[2]).abs() < 1e-12);
    // And for this vertical face it is the top cap's own plane (±y).
    assert!(n[0].abs() < 1e-12 && n[2].abs() < 1e-12 && (n[1].abs() - 1.0).abs() < 1e-12);
}

#[test]
fn p0003_rim_circle_crossing_left_the_lateral_face_past_its_top_edge() {
    // v44: the B rim chord × A lateral crossing, relocated onto the exact
    // circle × lateral-plane root 9.9e-4 above the top cap.
    let pre = p(
        0.010135225815110085,
        0.04970224448590975,
        -0.032413709457148374,
    );
    let post = p(0.0099491925424912, 0.0509928042635006, -0.03288343480173264);
    let fire = boundary_crease_crossed(pre, post, &[lateral_top_edge_crease()])
        .expect("the step crossed the top edge");
    assert_eq!(fire.crease, 0);
    // Below the cap before, above it after — the paper's "position p1
    // outside the surface S2 where the point is initially located".
    assert!(fire.f_pre < 0.0 && fire.f_post > 0.0, "{fire:?}");
    assert!((fire.f_pre - (0.04970224448590975 - 0.05)).abs() < 1e-12);
    assert!((fire.f_post - (0.0509928042635006 - 0.05)).abs() < 1e-12);
}

#[test]
fn p0003_top_edge_crossing_left_the_torus_face_past_its_rim() {
    // v15: A's top edge × B torus facet, relocated onto the exact line ×
    // torus root at 18.7° — inside the revolve's open wedge, past the 20° rim.
    let pre = p(0.010253981508214166, 0.05, -0.03211385686583834);
    let post = p(0.009403056801138402, 0.05, -0.03426240209981525);
    let fire = boundary_crease_crossed(pre, post, &[cutter_rim_crease()])
        .expect("the step crossed the rim circle");
    assert!((fire.f_pre < 0.0) != (fire.f_post < 0.0), "{fire:?}");
    assert!(
        fire.f_pre.abs() > 1e-4 && fire.f_post.abs() > 1e-3,
        "{fire:?}"
    );
}

#[test]
fn a_vertex_riding_the_crease_is_exempt() {
    // Gliding along the top edge itself (both ends on the crease plane): a
    // boundary point on its boundary curve, Fig. 13's population — no fire.
    let pre = p(
        0.012,
        0.05,
        -0.015697444364386564 - (0.016755650999796708 - 0.012) * 2.5248,
    );
    let post = p(
        0.010,
        0.05,
        -0.015697444364386564 - (0.016755650999796708 - 0.010) * 2.5248,
    );
    assert!(boundary_crease_crossed(pre, post, &[lateral_top_edge_crease()]).is_none());
    // Landing exactly ON the crease from below is a legitimate junction.
    let pre = p(0.010135225815110085, 0.0497, -0.032413709457148374);
    let post = p(0.010092, 0.05, -0.032522);
    assert!(boundary_crease_crossed(pre, post, &[lateral_top_edge_crease()]).is_none());
}

#[test]
fn the_extension_of_an_edge_past_its_ends_is_not_a_crease() {
    // A step on the lateral face's plane crossing y = 0.05 far beyond the
    // top edge's ends (x ≈ 0.10, three edge lengths away): the divider plane
    // extends there, the crease does not.
    let pre = p(
        0.10,
        0.0497,
        -0.015697444364386564 - (0.016755650999796708 - 0.10) * 2.5248,
    );
    let post = p(
        0.10,
        0.0510,
        -0.015697444364386564 - (0.016755650999796708 - 0.10) * 2.5248,
    );
    assert!(boundary_crease_crossed(pre, post, &[lateral_top_edge_crease()]).is_none());
}

#[test]
fn the_opposite_meridian_of_a_rim_plane_is_not_the_rim() {
    // The end-cap plane cuts the tube again half a turn away (angle 200°);
    // a step crossing it there is interior to the torus face.
    let a = 200.0_f64.to_radians();
    let at = |rho: f64, x: f64| p(x, 0.12 - rho * a.cos(), -0.058 + rho * a.sin());
    let da = 0.5_f64.to_radians();
    let pre = {
        let b = a - da;
        p(0.042, 0.12 - 0.1105 * b.cos(), -0.058 + 0.1105 * b.sin())
    };
    let post = {
        let b = a + da;
        p(0.042, 0.12 - 0.1105 * b.cos(), -0.058 + 0.1105 * b.sin())
    };
    let _ = at;
    assert!(boundary_crease_crossed(pre, post, &[cutter_rim_crease()]).is_none());
}

/// A box corner: two planar faces sharing one physical edge, emitted once
/// per half-edge (the m1 convention) — the neighbour is found by vertex pair.
/// A third face on the SAME surface as its neighbour (a split plane) bounds
/// no domain and contributes no crease.
#[test]
fn crease_index_pairs_neighbours_by_vertex_pair_and_skips_same_surface_splits() {
    let verts = vec![
        BRepVertex {
            point: p(0.0, 0.0, 0.0),
        },
        BRepVertex {
            point: p(1.0, 0.0, 0.0),
        },
        BRepVertex {
            point: p(1.0, 1.0, 0.0),
        },
        BRepVertex {
            point: p(0.0, 1.0, 0.0),
        },
        BRepVertex {
            point: p(0.0, 0.0, 1.0),
        },
        BRepVertex {
            point: p(1.0, 0.0, 1.0),
        },
        BRepVertex {
            point: p(2.0, 0.0, 0.0),
        },
        BRepVertex {
            point: p(2.0, 1.0, 0.0),
        },
    ];
    let seg = |s: u32, e: u32| BRepEdge {
        start: s,
        end: e,
        curve: Curve::LineSegment,
    };
    let edges = vec![
        // face 0 (z = 0 square, +z normal)
        seg(0, 1),
        seg(1, 2),
        seg(2, 3),
        seg(3, 0),
        // face 1 (y = 0 rectangle, −y normal), its own copy of edge 0→1
        seg(1, 0),
        seg(0, 4),
        seg(4, 5),
        seg(5, 1),
        // face 2: a second z = 0 square next to face 0 (same surface)
        seg(2, 1),
        seg(1, 6),
        seg(6, 7),
        seg(7, 2),
    ];
    let z0 = Surface::Plane {
        normal: Vector3::new(0.0, 0.0, 1.0),
        d: 0.0,
    };
    let y0 = Surface::Plane {
        normal: Vector3::new(0.0, -1.0, 0.0),
        d: 0.0,
    };
    let face = |s: Surface, l: Vec<u32>| BRepFace {
        surface: s,
        outer_loop: l,
        inner_loops: Vec::new(),
        reversed: false,
    };
    let brep = BRep::new(
        verts,
        edges,
        vec![
            face(z0, vec![0, 1, 2, 3]),
            face(y0, vec![4, 5, 6, 7]),
            face(z0, vec![8, 9, 10, 11]),
        ],
    )
    .expect("box corner");
    let idx = CreaseIndex::build(&brep);
    assert_eq!(idx.faces.len(), 3);
    // Face 0: edge 0 (0→1) is the crease with face 1; edge 1 (1→2) is shared
    // with face 2 on the SAME surface — skipped; edges 2, 3 have no neighbour
    // and remain creases (an open sheet still ends there).
    let f0: Vec<u32> = idx.faces[0].iter().map(|c| c.edge).collect();
    assert_eq!(f0, vec![0, 2, 3]);
    assert_eq!(idx.faces[0][0].s_other, Some(y0));
    // Face 1's copy of the shared edge names face 0's surface across it.
    let shared = idx.faces[1]
        .iter()
        .find(|c| c.edge == 4)
        .expect("face 1 keeps its shared edge");
    assert_eq!(shared.s_other, Some(z0));
    // The divider of face 0's edge 0→1 is the plane y = 0 (spanned by the
    // edge and face 0's normal).
    let Surface::Plane { normal, d } = idx.faces[0][0].divider else {
        panic!()
    };
    let n = normal.as_array();
    assert!(n[0].abs() < 1e-12 && n[2].abs() < 1e-12 && (n[1].abs() - 1.0).abs() < 1e-12);
    assert!(d.abs() < 1e-12);
}
