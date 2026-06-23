//! Hand-built cone/frustum fixtures for KV6c tests (validation, volume,
//! tessellation), shared until the KV6c revolve sweep (increment 4) can
//! construct cones for real. Topology mirrors `construct::extrude_circle`:
//! 2 seam vertices, 6 half-edges, 3 faces (base disk, top disk, cone band).

use crate::arena::{
    BrepArena, Curve, Face, FaceId, HalfEdge, HalfEdgeId, Loop, LoopBoundary, LoopId, LoopKind,
    Plane, Shell, ShellId, Solid, SolidId, Surface, UnitVector3, Vertex, VertexId,
};
use cad_primitives::Point3;

/// Build a closed right-circular frustum (truncated cone). The apex sits at
/// `apex`; the axis runs along unit `axis_dir`; the two rims sit at axial
/// coordinates `tau0` and `tau1` from the apex (`0 < tau0 < tau1`), each with
/// radius `tau · tan(half_angle)`. Returns `(arena, solid, lateral_face)`.
///
/// The stored `Surface::Cone` uses `surface_half_angle`, which normally equals
/// `half_angle` (a consistent solid) but can be set wrong to drive negative
/// validation tests.
pub(crate) fn build_frustum(
    apex: Point3,
    axis_dir: UnitVector3,
    tau0: f64,
    tau1: f64,
    half_angle: f64,
    surface_half_angle: f64,
) -> (BrepArena, SolidId, FaceId) {
    assert!(0.0 < tau0 && tau0 < tau1, "need 0 < tau0 < tau1");
    let a = axis_dir;
    let neg_a = UnitVector3 {
        x: -a.x,
        y: -a.y,
        z: -a.z,
    };
    // In-plane basis e1 ⊥ axis (for placing the seam vertices on the rims).
    let t = if a.x.abs() < 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let ta = t[0] * a.x + t[1] * a.y + t[2] * a.z;
    let e1v = [t[0] - ta * a.x, t[1] - ta * a.y, t[2] - ta * a.z];
    let e1n = (e1v[0] * e1v[0] + e1v[1] * e1v[1] + e1v[2] * e1v[2]).sqrt();
    let e1 = [e1v[0] / e1n, e1v[1] / e1n, e1v[2] / e1n];

    let tan = half_angle.tan();
    let (r0, r1) = (tau0 * tan, tau1 * tan);
    let center = |tau: f64| {
        Point3::new(
            apex.x() + tau * a.x,
            apex.y() + tau * a.y,
            apex.z() + tau * a.z,
        )
    };
    let on_rim =
        |c: Point3, r: f64| Point3::new(c.x() + r * e1[0], c.y() + r * e1[1], c.z() + r * e1[2]);
    let (c0, c1) = (center(tau0), center(tau1));
    let (v0, v1) = (on_rim(c0, r0), on_rim(c1, r1));

    let mut arena = BrepArena::new();
    let (vid0, vid1) = (VertexId(0), VertexId(1));
    arena.vertices.push(Some(Vertex { point: v0 }));
    arena.vertices.push(Some(Vertex { point: v1 }));

    let (cap_b, lat_b, seam_up, lat_t, cap_t, seam_dn) = (
        HalfEdgeId(0),
        HalfEdgeId(1),
        HalfEdgeId(2),
        HalfEdgeId(3),
        HalfEdgeId(4),
        HalfEdgeId(5),
    );
    let (loop_base, loop_top, loop_lat) = (LoopId(0), LoopId(1), LoopId(2));
    let (f_base, f_top, f_lat) = (FaceId(0), FaceId(1), FaceId(2));
    let shell = ShellId(0);
    let solid = SolidId(0);

    let circle = |center: Point3, normal: UnitVector3, radius: f64| Curve::Circle {
        center,
        normal,
        radius,
    };
    // Base cap: closed circle CCW around the cap normal −a.
    arena.half_edges.push(Some(HalfEdge {
        twin: lat_b,
        next: cap_b,
        prev: cap_b,
        origin: vid0,
        loop_id: loop_base,
        curve: circle(c0, neg_a, r0),
    }));
    // Lateral: bottom rim (toward top, +a), seam up, top rim (toward bottom,
    // −a), seam down.
    arena.half_edges.push(Some(HalfEdge {
        twin: cap_b,
        next: seam_up,
        prev: seam_dn,
        origin: vid0,
        loop_id: loop_lat,
        curve: circle(c0, a, r0),
    }));
    arena.half_edges.push(Some(HalfEdge {
        twin: seam_dn,
        next: lat_t,
        prev: lat_b,
        origin: vid0,
        loop_id: loop_lat,
        curve: Curve::LineSegment,
    }));
    arena.half_edges.push(Some(HalfEdge {
        twin: cap_t,
        next: seam_dn,
        prev: seam_up,
        origin: vid1,
        loop_id: loop_lat,
        curve: circle(c1, neg_a, r1),
    }));
    // Top cap: CCW around the cap normal +a.
    arena.half_edges.push(Some(HalfEdge {
        twin: lat_t,
        next: cap_t,
        prev: cap_t,
        origin: vid1,
        loop_id: loop_top,
        curve: circle(c1, a, r1),
    }));
    arena.half_edges.push(Some(HalfEdge {
        twin: seam_up,
        next: lat_b,
        prev: lat_t,
        origin: vid1,
        loop_id: loop_lat,
        curve: Curve::LineSegment,
    }));

    for (face, boundary) in [(f_base, cap_b), (f_top, cap_t), (f_lat, lat_b)] {
        arena.loops.push(Some(Loop {
            face,
            boundary: LoopBoundary::Edges(boundary),
            kind: LoopKind::Outer,
        }));
    }
    arena.faces.push(Some(Face {
        surface: Some(Surface::Plane(Plane {
            point: c0,
            normal: neg_a,
        })),
        outer_loop: loop_base,
        inner_loops: Vec::new(),
        shell,
    }));
    arena.faces.push(Some(Face {
        surface: Some(Surface::Plane(Plane {
            point: c1,
            normal: a,
        })),
        outer_loop: loop_top,
        inner_loops: Vec::new(),
        shell,
    }));
    arena.faces.push(Some(Face {
        surface: Some(Surface::Cone {
            apex,
            axis_dir: a,
            half_angle: surface_half_angle,
            reversed: false,
        }),
        outer_loop: loop_lat,
        inner_loops: Vec::new(),
        shell,
    }));
    arena.shells.push(Some(Shell {
        solid,
        faces: vec![f_base, f_top, f_lat],
        genus: 0,
    }));
    arena.solids.push(Some(Solid {
        shells: vec![shell],
    }));
    (arena, solid, f_lat)
}
