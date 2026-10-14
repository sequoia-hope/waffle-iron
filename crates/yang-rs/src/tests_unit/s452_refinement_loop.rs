//! Yang §4.5 — the one refinement loop's OUTPUT-level §4.5.4 check, T3b
//! (spec `specs/yang_452_refinement_loop.md` §6.3): two distinct faces on one
//! quadric whose planar boundary curves cross. Phase 1c of
//! `specs/yang_faithful_cleanup.md`. The loop itself is proven on the
//! corpus (§6.5); its pure parts are pinned here and in
//! `s452_under_resolution_ladder.rs` (`density_for_demand`).

use crate::boolean::failures::{output_curve_crossings, FailureTrigger};
use crate::brep::{InputId, TriangleAttribution, TriangleAttributionMap};
use crate::{BRep, BRepEdge, BRepFace, BRepVertex, Curve, Mesh, Point3, Surface, Vector3};

/// An "output" B-Rep shell with no mesh: faces, edges, vertices and the
/// per-face input lineage are all T3b reads.
fn output_shell(
    verts: Vec<BRepVertex>,
    edges: Vec<BRepEdge>,
    faces: Vec<BRepFace>,
    lineage: Vec<(InputId, u32)>,
) -> BRep {
    BRep {
        vertices: verts,
        edges,
        faces,
        mesh: Mesh::new(Vec::new(), Vec::new()),
        tessellation: crate::TessellationMap {
            sources: Vec::new(),
        },
        triangle_attribution: TriangleAttributionMap::empty(),
        face_attribution: lineage
            .into_iter()
            .map(|(input, face)| TriangleAttribution { input, face })
            .collect(),
        tri_face: Vec::new(),
        forced_rim_n: None,
        standing_rim: Default::default(),
        standing_face: Default::default(),
        standing_face_constraints: Default::default(),
        face_refine: Default::default(),
    }
}

const CYL: Surface = Surface::Cylinder {
    axis_point: Point3::new(0.0, 0.0, 0.0),
    axis_dir: Vector3::new(0.0, 0.0, 1.0),
    radius: 1.0,
};

/// The unit cylinder's section by the plane through the origin with normal
/// `(±sin θ, 0, cos θ)`: an ellipse with major axis along `(cos θ, 0, ∓sin θ)`,
/// semi-axes `1/cos θ` and 1. `sign = +1` tilts one way, `−1` the other; the
/// two cross at `(0, ±1, 0)`.
fn tilted_ellipse(theta: f64, sign: f64) -> Curve {
    let (s, c) = theta.sin_cos();
    Curve::Ellipse {
        center: Point3::new(0.0, 0.0, 0.0),
        normal: Vector3::new(sign * s, 0.0, c),
        major_axis: Vector3::new(c, 0.0, -sign * s),
        major_radius: 1.0 / c,
        minor_radius: 1.0,
    }
}

/// A point of `tilted_ellipse(theta, sign)` at parameter `t` (the shared
/// ellipse frame: `center + a·cos t·major + b·sin t·(normal × major)`).
fn ellipse_pt(theta: f64, sign: f64, t: f64) -> Point3 {
    let Curve::Ellipse {
        center,
        normal,
        major_axis,
        major_radius,
        minor_radius,
    } = tilted_ellipse(theta, sign)
    else {
        unreachable!()
    };
    crate::geom::ellipse_point(center, normal, major_axis, major_radius, minor_radius, t)
}

fn band_face(loop_edges: Vec<u32>) -> BRepFace {
    BRepFace {
        surface: CYL,
        outer_loop: loop_edges,
        inner_loops: Vec::new(),
        reversed: false,
    }
}

#[test]
fn two_full_ellipses_of_two_faces_on_one_cylinder_cross_twice() {
    let theta = 0.5f64;
    let verts = vec![
        BRepVertex {
            point: ellipse_pt(theta, 1.0, 0.0),
        },
        BRepVertex {
            point: ellipse_pt(theta, -1.0, 0.0),
        },
    ];
    let edges = vec![
        BRepEdge {
            start: 0,
            end: 0,
            curve: tilted_ellipse(theta, 1.0),
        },
        BRepEdge {
            start: 1,
            end: 1,
            curve: tilted_ellipse(theta, -1.0),
        },
    ];
    let out = output_shell(
        verts,
        edges,
        vec![band_face(vec![0]), band_face(vec![1])],
        vec![(InputId::A, 5), (InputId::B, 9)],
    );
    let sites = output_curve_crossings(&out);
    assert_eq!(sites.len(), 2, "{sites:?}");
    for s in &sites {
        assert_eq!(s.trigger, FailureTrigger::IllegalIntersection);
        assert_eq!(s.faces, vec![(InputId::A, 5), (InputId::B, 9)]);
    }
}

#[test]
fn parallel_sections_and_a_single_face_never_cross() {
    // Two parallel circles (z = 0, z = 1) on one cylinder: parallel planes,
    // no transversal crossing.
    let circle = |z: f64| Curve::Circle {
        center: Point3::new(0.0, 0.0, z),
        normal: Vector3::new(0.0, 0.0, 1.0),
        radius: 1.0,
    };
    let verts = vec![
        BRepVertex {
            point: Point3::new(1.0, 0.0, 0.0),
        },
        BRepVertex {
            point: Point3::new(1.0, 0.0, 1.0),
        },
    ];
    let edges = vec![
        BRepEdge {
            start: 0,
            end: 0,
            curve: circle(0.0),
        },
        BRepEdge {
            start: 1,
            end: 1,
            curve: circle(1.0),
        },
    ];
    let out = output_shell(
        verts.clone(),
        edges.clone(),
        vec![band_face(vec![0]), band_face(vec![1])],
        vec![(InputId::A, 1), (InputId::A, 2)],
    );
    assert!(output_curve_crossings(&out).is_empty());
    // One face carrying both tilted ellipses: a face does not cross itself
    // under this check (same-face folds are the mesh census's, Phase 2).
    let theta = 0.5f64;
    let one = output_shell(
        vec![
            BRepVertex {
                point: ellipse_pt(theta, 1.0, 0.0),
            },
            BRepVertex {
                point: ellipse_pt(theta, -1.0, 0.0),
            },
        ],
        vec![
            BRepEdge {
                start: 0,
                end: 0,
                curve: tilted_ellipse(theta, 1.0),
            },
            BRepEdge {
                start: 1,
                end: 1,
                curve: tilted_ellipse(theta, -1.0),
            },
        ],
        vec![band_face(vec![0, 1])],
        vec![(InputId::A, 1)],
    );
    assert!(output_curve_crossings(&one).is_empty());
}

#[test]
fn an_arc_crosses_only_when_the_crossing_lies_strictly_inside_its_span() {
    use std::f64::consts::FRAC_PI_4;
    let theta = 0.5f64;
    // The crossings of the two tilted ellipses sit at parameter π/2 and
    // 3π/2 of each (x = 0). Face 1 carries the FULL second ellipse; face 0
    // carries an ARC of the first: [π/4, 3π/4] contains π/2 (one crossing),
    // [−π/4, π/4] contains none.
    let full_start = ellipse_pt(theta, -1.0, 0.0);
    for (t0, t1, expect) in [
        (FRAC_PI_4, 3.0 * FRAC_PI_4, 1usize),
        (-FRAC_PI_4, FRAC_PI_4, 0usize),
    ] {
        let verts = vec![
            BRepVertex {
                point: ellipse_pt(theta, 1.0, t0),
            },
            BRepVertex {
                point: ellipse_pt(theta, 1.0, t1),
            },
            BRepVertex { point: full_start },
        ];
        let edges = vec![
            BRepEdge {
                start: 0,
                end: 1,
                curve: tilted_ellipse(theta, 1.0),
            },
            BRepEdge {
                start: 2,
                end: 2,
                curve: tilted_ellipse(theta, -1.0),
            },
        ];
        let out = output_shell(
            verts,
            edges,
            vec![band_face(vec![0]), band_face(vec![1])],
            vec![(InputId::A, 3), (InputId::B, 4)],
        );
        let sites = output_curve_crossings(&out);
        assert_eq!(sites.len(), expect, "arc [{t0}, {t1}]: {sites:?}");
    }
}

#[test]
fn a_crossing_at_a_shared_endpoint_is_a_corner_not_a_crossing() {
    use std::f64::consts::FRAC_PI_2;
    let theta = 0.5f64;
    // Both arcs END exactly at the crossing (0, 1, 0): a corner the two
    // faces share, within the model band of both endpoints.
    let verts = vec![
        BRepVertex {
            point: ellipse_pt(theta, 1.0, 0.0),
        },
        BRepVertex {
            point: ellipse_pt(theta, 1.0, FRAC_PI_2),
        },
        BRepVertex {
            point: ellipse_pt(theta, -1.0, 0.0),
        },
        BRepVertex {
            point: ellipse_pt(theta, -1.0, FRAC_PI_2),
        },
    ];
    let edges = vec![
        BRepEdge {
            start: 0,
            end: 1,
            curve: tilted_ellipse(theta, 1.0),
        },
        BRepEdge {
            start: 2,
            end: 3,
            curve: tilted_ellipse(theta, -1.0),
        },
    ];
    let out = output_shell(
        verts,
        edges,
        vec![band_face(vec![0]), band_face(vec![1])],
        vec![(InputId::A, 3), (InputId::B, 4)],
    );
    assert!(output_curve_crossings(&out).is_empty());
}
