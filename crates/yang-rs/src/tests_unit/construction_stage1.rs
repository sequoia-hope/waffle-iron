#[allow(unused_imports)]
use super::*;

// ----- Group 2: yang-rs type construction -----

#[test]
pub(crate) fn surface_plane_construction() {
    let s = Surface::Plane {
        normal: Vector3::new(0.0, 0.0, 1.0),
        d: -1.0,
    };
    match s {
        Surface::Plane { normal, d } => {
            assert_eq!(normal, Vector3::new(0.0, 0.0, 1.0));
            assert_eq!(d, -1.0);
        }
        // `s` is constructed as `Plane`, so this arm is never hit; it
        // only satisfies exhaustiveness once curved variants are added.
        _ => panic!("expected Plane"),
    }
}

// ----- PR-YR6: curved Surface / Curve construction round-trips -----

#[test]
pub(crate) fn surface_sphere_construction() {
    let s = Surface::Sphere {
        center: p(1.0, 2.0, 3.0),
        radius: 5.0,
    };
    match s {
        Surface::Sphere { center, radius } => {
            assert_eq!(center, p(1.0, 2.0, 3.0));
            assert_eq!(radius, 5.0);
        }
        _ => panic!("expected Sphere"),
    }
}

#[test]
pub(crate) fn surface_cylinder_construction() {
    let s = Surface::Cylinder {
        axis_point: p(1.0, 2.0, 3.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 4.0,
    };
    match s {
        Surface::Cylinder {
            axis_point,
            axis_dir,
            radius,
        } => {
            assert_eq!(axis_point, p(1.0, 2.0, 3.0));
            assert_eq!(axis_dir, Vector3::new(0.0, 0.0, 1.0));
            assert_eq!(radius, 4.0);
        }
        _ => panic!("expected Cylinder"),
    }
}

#[test]
pub(crate) fn surface_cone_construction() {
    let s = Surface::Cone {
        apex: p(0.0, 0.0, 10.0),
        axis_dir: Vector3::new(0.0, 0.0, -1.0),
        half_angle: 0.5,
    };
    match s {
        Surface::Cone {
            apex,
            axis_dir,
            half_angle,
        } => {
            assert_eq!(apex, p(0.0, 0.0, 10.0));
            assert_eq!(axis_dir, Vector3::new(0.0, 0.0, -1.0));
            assert_eq!(half_angle, 0.5);
        }
        _ => panic!("expected Cone"),
    }
}

#[test]
pub(crate) fn curve_circle_construction() {
    let c = Curve::Circle {
        center: p(1.0, 2.0, 3.0),
        normal: Vector3::new(0.0, 0.0, 1.0),
        radius: 2.5,
    };
    match c {
        Curve::Circle {
            center,
            normal,
            radius,
        } => {
            assert_eq!(center, p(1.0, 2.0, 3.0));
            assert_eq!(normal, Vector3::new(0.0, 0.0, 1.0));
            assert_eq!(radius, 2.5);
        }
        _ => panic!("expected Circle"),
    }
}

#[test]
pub(crate) fn curve_ellipse_construction() {
    let c = Curve::Ellipse {
        center: p(1.0, 2.0, 3.0),
        normal: Vector3::new(0.0, 0.0, 1.0),
        major_axis: Vector3::new(1.0, 0.0, 0.0),
        major_radius: 6.0,
        minor_radius: 3.0,
    };
    match c {
        Curve::Ellipse {
            center,
            normal,
            major_axis,
            major_radius,
            minor_radius,
        } => {
            assert_eq!(center, p(1.0, 2.0, 3.0));
            assert_eq!(normal, Vector3::new(0.0, 0.0, 1.0));
            assert_eq!(major_axis, Vector3::new(1.0, 0.0, 0.0));
            assert_eq!(major_radius, 6.0);
            assert_eq!(minor_radius, 3.0);
        }
        _ => panic!("expected Ellipse"),
    }
}

// ----- PR-YR6: BRep::new loud-rejects curved surfaces -----

/// Minimal well-formed single-triangle topology (3 verts, 3 edges, one
/// face with a 3-edge outer loop). Mirrors the `brep_new_single_triangle`
/// fixture exactly except the single face's surface is caller-supplied,
/// so the ONLY variable across the loud-rejection tests is the surface.
pub(crate) fn single_triangle_topology(
    surface: Surface,
) -> (Vec<BRepVertex>, Vec<BRepEdge>, Vec<BRepFace>) {
    let verts = vec![
        BRepVertex {
            point: p(0.0, 0.0, 0.0),
        },
        BRepVertex {
            point: p(1.0, 0.0, 0.0),
        },
        BRepVertex {
            point: p(0.0, 1.0, 0.0),
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
        reversed: false,
    }];
    (verts, edges, faces)
}

#[test]
pub(crate) fn brep_new_rejects_sphere_face() {
    // PR-YR12 migration: the sphere path is now implemented, but a sphere
    // face on a single *triangle* (no Circle meridian seam edge) lacks the
    // seam the sphere tessellation requires, so it is rejected as
    // MalformedTopology rather than CurvedSurfaceNotYetSupported. It must
    // STILL error loudly; only the error kind changed (mirrors the cylinder
    // migration above).
    let (verts, edges, faces) = single_triangle_topology(Surface::Sphere {
        center: p(0.0, 0.0, 0.0),
        radius: 1.0,
    });
    let result = BRep::new(verts, edges, faces);
    assert!(
        matches!(result, Err(YangError::MalformedTopology(_))),
        "expected MalformedTopology (sphere on a triangle lacks its meridian \
             seam Circle edge), got {result:?}"
    );
}

#[test]
pub(crate) fn brep_new_rejects_cylinder_face() {
    // PR-YR7 migration: the cylinder lateral path is now implemented, but a
    // cylinder face on a single *triangle* (no Circle rim edges) lacks the
    // lateral's 2 required Circle rims, so it is rejected as
    // MalformedTopology rather than CurvedSurfaceNotYetSupported. It must
    // STILL error loudly; only the error kind changed.
    let (verts, edges, faces) = single_triangle_topology(Surface::Cylinder {
        axis_point: p(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 1.0,
    });
    let result = BRep::new(verts, edges, faces);
    assert!(
        matches!(result, Err(YangError::MalformedTopology(_))),
        "expected MalformedTopology (cylinder lateral on a triangle lacks its \
             2 Circle rim edges), got {result:?}"
    );
}

#[test]
pub(crate) fn brep_new_rejects_cone_face() {
    // PR-YR16 migration: a Cone face on a *triangle* (no base-rim Circle the
    // cone tessellation path requires) is now MalformedTopology, mirroring the
    // cylinder/sphere-on-a-triangle rejection. It must STILL error loudly
    // (never silently succeed); only the error *kind* changed.
    let (verts, edges, faces) = single_triangle_topology(Surface::Cone {
        apex: p(0.0, 0.0, 1.0),
        axis_dir: Vector3::new(0.0, 0.0, -1.0),
        half_angle: 0.5,
    });
    let result = BRep::new(verts, edges, faces);
    assert!(
        matches!(result, Err(YangError::MalformedTopology(_))),
        "expected MalformedTopology (cone lateral on a triangle lacks its \
             base-rim Circle edge), got {result:?}"
    );
}

#[test]
pub(crate) fn curve_line_segment_construction() {
    let c = Curve::LineSegment;
    assert_eq!(c, Curve::LineSegment);
}

#[test]
pub(crate) fn brep_topology_construction() {
    let v = BRepVertex {
        point: p(0.0, 0.0, 0.0),
    };
    let e = BRepEdge {
        start: 0,
        end: 1,
        curve: Curve::LineSegment,
    };
    let f = BRepFace {
        surface: Surface::Plane {
            normal: Vector3::new(0.0, 0.0, 1.0),
            d: 0.0,
        },
        outer_loop: vec![0, 1, 2],
        inner_loops: Vec::new(),
        reversed: false,
    };
    assert_eq!(v.point, p(0.0, 0.0, 0.0));
    assert_eq!(e.start, 0);
    assert_eq!(f.outer_loop.len(), 3);
}

#[test]
pub(crate) fn tessellation_source_round_trip() {
    let src = TessellationSource::BRepVertex(7);
    match src {
        TessellationSource::BRepVertex(i) => assert_eq!(i, 7),
        _ => panic!("wrong variant"),
    }
}

#[test]
pub(crate) fn tessellation_map_empty() {
    let m = TessellationMap::empty();
    assert_eq!(m.len(), 0);
    assert!(m.is_empty());
}

// ----- Group 3: from_mesh degenerate path -----

#[test]
pub(crate) fn from_mesh_preserves_mesh() {
    let m = sample_mesh();
    let b = BRep::from_mesh(m.clone());
    assert_eq!(b.as_mesh(), &m);
}

#[test]
pub(crate) fn from_mesh_map_length_matches_verts() {
    let m = sample_mesh();
    let b = BRep::from_mesh(m.clone());
    assert_eq!(b.tessellation_map().len(), m.num_verts());
}

#[test]
pub(crate) fn from_mesh_map_entries_all_unknown() {
    let m = sample_mesh();
    let b = BRep::from_mesh(m.clone());
    for i in 0..b.tessellation_map().len() as u32 {
        assert_eq!(b.tessellation_map().lookup(i), TessellationSource::Unknown);
    }
}

// ----- Group 4: BRep::new Stage 1 happy paths -----

pub(crate) fn plane_z_up() -> Surface {
    Surface::Plane {
        normal: Vector3::new(0.0, 0.0, 1.0),
        d: 0.0,
    }
}

#[test]
pub(crate) fn brep_new_single_triangle() {
    let verts = vec![
        BRepVertex {
            point: p(0.0, 0.0, 0.0),
        },
        BRepVertex {
            point: p(1.0, 0.0, 0.0),
        },
        BRepVertex {
            point: p(0.0, 1.0, 0.0),
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
        surface: plane_z_up(),
        outer_loop: vec![0, 1, 2],
        inner_loops: Vec::new(),
        reversed: false,
    }];
    let b = BRep::new(verts, edges, faces).unwrap();
    assert_eq!(b.num_verts(), 3);
    assert_eq!(b.num_tris(), 1);
    for i in 0..3u32 {
        assert_eq!(
            b.tessellation_map().lookup(i),
            TessellationSource::BRepVertex(i)
        );
    }
}

#[test]
pub(crate) fn brep_new_quad_face() {
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
            end: 3,
            curve: Curve::LineSegment,
        },
        BRepEdge {
            start: 3,
            end: 0,
            curve: Curve::LineSegment,
        },
    ];
    let faces = vec![BRepFace {
        surface: plane_z_up(),
        outer_loop: vec![0, 1, 2, 3],
        inner_loops: Vec::new(),
        reversed: false,
    }];
    let b = BRep::new(verts, edges, faces).unwrap();
    assert_eq!(b.num_verts(), 4);
    assert_eq!(b.num_tris(), 2); // 4-vert fan: 2 tris
}

#[test]
pub(crate) fn brep_new_tetrahedron() {
    let verts = vec![
        BRepVertex {
            point: p(0.0, 0.0, 0.0),
        },
        BRepVertex {
            point: p(1.0, 0.0, 0.0),
        },
        BRepVertex {
            point: p(0.0, 1.0, 0.0),
        },
        BRepVertex {
            point: p(0.0, 0.0, 1.0),
        },
    ];
    // Edges of a tetrahedron: 6 edges between 4 vertices.
    let edges = vec![
        BRepEdge {
            start: 0,
            end: 1,
            curve: Curve::LineSegment,
        }, // 0
        BRepEdge {
            start: 1,
            end: 2,
            curve: Curve::LineSegment,
        }, // 1
        BRepEdge {
            start: 2,
            end: 0,
            curve: Curve::LineSegment,
        }, // 2
        BRepEdge {
            start: 0,
            end: 3,
            curve: Curve::LineSegment,
        }, // 3
        BRepEdge {
            start: 1,
            end: 3,
            curve: Curve::LineSegment,
        }, // 4
        BRepEdge {
            start: 2,
            end: 3,
            curve: Curve::LineSegment,
        }, // 5
        // Reverse-direction edges for the loops (each tet face has 3 edges)
        BRepEdge {
            start: 3,
            end: 0,
            curve: Curve::LineSegment,
        }, // 6
        BRepEdge {
            start: 3,
            end: 1,
            curve: Curve::LineSegment,
        }, // 7
        BRepEdge {
            start: 3,
            end: 2,
            curve: Curve::LineSegment,
        }, // 8
        BRepEdge {
            start: 1,
            end: 0,
            curve: Curve::LineSegment,
        }, // 9
        BRepEdge {
            start: 2,
            end: 1,
            curve: Curve::LineSegment,
        }, // 10
        BRepEdge {
            start: 0,
            end: 2,
            curve: Curve::LineSegment,
        }, // 11
    ];
    // 4 triangular faces. Each loop is 3 edges. Note: outer_loop's
    // start vertices must form a coherent cycle for fan-triangulation
    // to produce correct tris; we use edges 0,1,2 for the "bottom"
    // (verts 0→1→2), etc.
    let faces = vec![
        BRepFace {
            surface: plane_z_up(),
            outer_loop: vec![0, 1, 2],
            inner_loops: Vec::new(),
            reversed: false,
        }, // bottom (verts 0,1,2)
        BRepFace {
            surface: plane_z_up(),
            outer_loop: vec![9, 3, 7],
            inner_loops: Vec::new(),
            reversed: false,
        }, // back (verts 1,0,3) - using 1→0,0→3,3→1
        BRepFace {
            surface: plane_z_up(),
            outer_loop: vec![10, 4, 8],
            inner_loops: Vec::new(),
            reversed: false,
        }, // right (verts 2,1,3)
        BRepFace {
            surface: plane_z_up(),
            outer_loop: vec![11, 5, 6],
            inner_loops: Vec::new(),
            reversed: false,
        }, // left (verts 0,2,3)
    ];
    let b = BRep::new(verts, edges, faces).unwrap();
    assert_eq!(b.num_verts(), 4);
    assert_eq!(b.num_tris(), 4);
}

#[test]
pub(crate) fn brep_new_unit_cube() {
    // 8 verts of a unit cube at origin.
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
            point: p(1.0, 1.0, 1.0),
        },
        BRepVertex {
            point: p(0.0, 1.0, 1.0),
        },
    ];
    // For PR-YR2 we don't need real edge dedup; just enumerate the
    // 24 directed edges we'll need (one per face boundary).
    // bottom face vertices: 0→3→2→1, edges 0:0→3, 1:3→2, 2:2→1, 3:1→0
    // (we just need fan_verts[0] to be the starting vertex of each
    // outer_loop)
    let edges: Vec<BRepEdge> = vec![
        // bottom face: 0, 3, 2, 1
        (0, 3),
        (3, 2),
        (2, 1),
        (1, 0),
        // top face: 4, 5, 6, 7
        (4, 5),
        (5, 6),
        (6, 7),
        (7, 4),
        // south face: 0, 1, 5, 4
        (0, 1),
        (1, 5),
        (5, 4),
        (4, 0),
        // north face: 3, 7, 6, 2
        (3, 7),
        (7, 6),
        (6, 2),
        (2, 3),
        // east face: 1, 2, 6, 5
        (1, 2),
        (2, 6),
        (6, 5),
        (5, 1),
        // west face: 0, 4, 7, 3
        (0, 4),
        (4, 7),
        (7, 3),
        (3, 0),
    ]
    .into_iter()
    .map(|(s, e)| BRepEdge {
        start: s,
        end: e,
        curve: Curve::LineSegment,
    })
    .collect();
    let plane = plane_z_up();
    let faces = vec![
        BRepFace {
            surface: plane,
            outer_loop: vec![0, 1, 2, 3],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: plane,
            outer_loop: vec![4, 5, 6, 7],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: plane,
            outer_loop: vec![8, 9, 10, 11],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: plane,
            outer_loop: vec![12, 13, 14, 15],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: plane,
            outer_loop: vec![16, 17, 18, 19],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: plane,
            outer_loop: vec![20, 21, 22, 23],
            inner_loops: Vec::new(),
            reversed: false,
        },
    ];
    let b = BRep::new(verts, edges, faces).unwrap();
    assert_eq!(b.num_verts(), 8);
    assert_eq!(b.num_tris(), 12); // 6 quads × 2 tris each
}

#[test]
pub(crate) fn brep_new_bijection_is_one_to_one() {
    // Build a tetrahedron and confirm every mesh vertex i maps to
    // TessellationSource::BRepVertex(i).
    let verts = vec![
        BRepVertex {
            point: p(0.0, 0.0, 0.0),
        },
        BRepVertex {
            point: p(1.0, 0.0, 0.0),
        },
        BRepVertex {
            point: p(0.0, 1.0, 0.0),
        },
        BRepVertex {
            point: p(0.0, 0.0, 1.0),
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
        surface: plane_z_up(),
        outer_loop: vec![0, 1, 2],
        inner_loops: Vec::new(),
        reversed: false,
    }];
    let b = BRep::new(verts, edges, faces).unwrap();
    for i in 0..b.num_verts() as u32 {
        assert_eq!(
            b.tessellation_map().lookup(i),
            TessellationSource::BRepVertex(i),
            "vertex {i} should map to BRepVertex({i})"
        );
    }
}

// ----- Group 5: Error paths -----

#[test]
pub(crate) fn brep_new_face_with_too_few_edges_errors() {
    let verts = vec![
        BRepVertex {
            point: p(0.0, 0.0, 0.0),
        },
        BRepVertex {
            point: p(1.0, 0.0, 0.0),
        },
    ];
    let edges = vec![BRepEdge {
        start: 0,
        end: 1,
        curve: Curve::LineSegment,
    }];
    // 1-edge face — degenerate
    let faces = vec![BRepFace {
        surface: plane_z_up(),
        outer_loop: vec![0],
        inner_loops: Vec::new(),
        reversed: false,
    }];
    let err = BRep::new(verts, edges, faces).unwrap_err();
    match err {
        YangError::MalformedTopology(_) => {}
        other => panic!("expected MalformedTopology, got {:?}", other),
    }
}

#[test]
pub(crate) fn brep_new_out_of_range_edge_index_errors() {
    let verts = vec![
        BRepVertex {
            point: p(0.0, 0.0, 0.0),
        },
        BRepVertex {
            point: p(1.0, 0.0, 0.0),
        },
        BRepVertex {
            point: p(0.0, 1.0, 0.0),
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
    // Face references edge 99 — out of range
    let faces = vec![BRepFace {
        surface: plane_z_up(),
        outer_loop: vec![0, 1, 99],
        inner_loops: Vec::new(),
        reversed: false,
    }];
    let err = BRep::new(verts, edges, faces).unwrap_err();
    match err {
        YangError::MalformedTopology(_) => {}
        other => panic!("expected MalformedTopology, got {:?}", other),
    }
}
