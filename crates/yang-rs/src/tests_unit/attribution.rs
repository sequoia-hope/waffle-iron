#[allow(unused_imports)]
use super::*;

// ----- PR-YR4: Group 1 — types -----

#[test]
pub(crate) fn input_id_ordering_and_derives() {
    assert!(InputId::A < InputId::B);
    assert_eq!(InputId::A, InputId::A);
    assert_ne!(InputId::A, InputId::B);
    assert_eq!(format!("{:?}", InputId::A), "A");
    assert_eq!(format!("{:?}", InputId::B), "B");
    // Copy
    let x = InputId::A;
    let y = x;
    assert_eq!(x, y);
}

#[test]
pub(crate) fn triangle_attribution_construct_and_equality() {
    let t1 = TriangleAttribution {
        input: InputId::A,
        face: 7,
    };
    let t2 = TriangleAttribution {
        input: InputId::A,
        face: 7,
    };
    let t3 = TriangleAttribution {
        input: InputId::B,
        face: 7,
    };
    assert_eq!(t1, t2);
    assert_ne!(t1, t3);
    // Copy + accessors
    let t4 = t1;
    assert_eq!(t4.input, InputId::A);
    assert_eq!(t4.face, 7);
}

#[test]
pub(crate) fn triangle_attribution_map_empty_and_len() {
    let m = TriangleAttributionMap::empty();
    assert_eq!(m.len(), 0);
    assert!(m.is_empty());
}

// ----- PR-YR4: Group 2 — algorithm via mock backend -----

/// Two-face B-Rep where V0 is shared by F0 and F1; V1, V2 only in F0;
/// V3, V4 only in F1. Used by tie-break + pure-input tests.
pub(crate) fn two_face_shared_vertex_brep() -> BRep {
    let verts = vec![
        BRepVertex {
            point: p(0.0, 0.0, 0.0),
        }, // 0 — shared (F0 & F1)
        BRepVertex {
            point: p(1.0, 0.0, 0.0),
        }, // 1 — F0 only
        BRepVertex {
            point: p(1.0, 1.0, 0.0),
        }, // 2 — F0 only (moved off x-axis: was (2,0,0)) so F0 is a real triangle in z=0
        BRepVertex {
            point: p(0.0, 1.0, 0.0),
        }, // 3 — F1 only
        BRepVertex {
            point: p(0.0, 1.0, 1.0),
        }, // 4 — F1 only (moved off y-axis: was (0,2,0)) so F1 is a real triangle in x=0
    ];
    // F0 edges (triangle V0-V1-V2):
    // E0 V0→V1, E1 V1→V2, E2 V2→V0
    // F1 edges (triangle V0-V3-V4):
    // E3 V0→V3, E4 V3→V4, E5 V4→V0
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
        BRepEdge {
            start: 0,
            end: 3,
            curve: Curve::LineSegment,
        },
        BRepEdge {
            start: 3,
            end: 4,
            curve: Curve::LineSegment,
        },
        BRepEdge {
            start: 4,
            end: 0,
            curve: Curve::LineSegment,
        },
    ];
    // F0 lies in z=0 (normal +z); F1 now lies in x=0 (normal +x).
    let f0_plane = Surface::Plane {
        normal: Vector3::new(0.0, 0.0, 1.0),
        d: 0.0,
    };
    let f1_plane = Surface::Plane {
        normal: Vector3::new(1.0, 0.0, 0.0),
        d: 0.0,
    };
    let faces = vec![
        BRepFace {
            surface: f0_plane,
            outer_loop: vec![0, 1, 2],
            inner_loops: Vec::new(),
            reversed: false,
        }, // F0
        BRepFace {
            surface: f1_plane,
            outer_loop: vec![3, 4, 5],
            inner_loops: Vec::new(),
            reversed: false,
        }, // F1
    ];
    BRep::new(verts, edges, faces).unwrap()
}

// PR-YR4 majority-vote ATTRIBUTION was REMOVED from production by M3
// (production attributes via real LabeledArrangement labels + geometric
// face resolution). Per Manager policy (a), these tests are reworked to
// exercise the now-#[cfg(test)] substitute via `substitute_attribution`
// DIRECTLY (not via production `boolean()`), preserving the substitute's
// coverage as the M4 differential oracle.

#[test]
pub(crate) fn boolean_pure_a_attributes_to_a_faces() {
    // Pure-A: substitute over A's mesh. Each tri's verts are
    // BRepVertex(i) of A → per-vertex face incidence → majority vote
    // attributes each tri to its source face.
    let a = two_face_shared_vertex_brep();
    let b = two_face_shared_vertex_brep();
    let attr = substitute_attribution(a.as_mesh(), &a, &b);
    assert_eq!(attr.len(), 2);
    assert_eq!(
        attr.lookup(0),
        Some(TriangleAttribution {
            input: InputId::A,
            face: 0
        }),
        "output tri 0 (F0 fan tri) should attribute to A's F0"
    );
    assert_eq!(
        attr.lookup(1),
        Some(TriangleAttribution {
            input: InputId::A,
            face: 1
        }),
        "output tri 1 (F1 fan tri) should attribute to A's F1"
    );
}

#[test]
pub(crate) fn boolean_pure_b_attributes_to_b_faces() {
    let a = two_face_shared_vertex_brep();
    // B is the same B-Rep, shifted so A's spatial match fails first.
    let mut b_verts = a.vertices().to_vec();
    for v in &mut b_verts {
        v.point = Point3::new(v.point.x() + 100.0, v.point.y(), v.point.z());
    }
    let b = BRep::new(b_verts, a.edges().to_vec(), a.faces().to_vec()).unwrap();
    let attr = substitute_attribution(b.as_mesh(), &a, &b);
    assert_eq!(
        attr.lookup(0),
        Some(TriangleAttribution {
            input: InputId::B,
            face: 0
        })
    );
    assert_eq!(
        attr.lookup(1),
        Some(TriangleAttribution {
            input: InputId::B,
            face: 1
        })
    );
}

#[test]
pub(crate) fn boolean_all_new_coords_attribute_to_none() {
    let a = two_face_shared_vertex_brep();
    let b = two_face_shared_vertex_brep();
    // A mesh with coords far from both inputs.
    let novel = Mesh::new(
        vec![
            p(1000.0, 1000.0, 1000.0),
            p(1001.0, 1000.0, 1000.0),
            p(1000.0, 1001.0, 1000.0),
        ],
        vec![[0, 1, 2]],
    );
    let attr = substitute_attribution(&novel, &a, &b);
    assert_eq!(attr.len(), 1);
    assert_eq!(
        attr.lookup(0),
        None,
        "all-new triangle should have None attribution"
    );
}

#[test]
pub(crate) fn boolean_mixed_majority_wins() {
    // 2 verts match A's F0 + 1 novel → F0 attribution.
    let a = two_face_shared_vertex_brep();
    let b = two_face_shared_vertex_brep();
    let mixed = Mesh::new(
        vec![
            p(1.0, 0.0, 0.0),       // matches a.verts[1] (F0 only)
            p(1.0, 1.0, 0.0),       // matches a.verts[2] (F0 only)
            p(1000.0, 0.0, 1000.0), // novel
        ],
        vec![[0, 1, 2]],
    );
    let attr = substitute_attribution(&mixed, &a, &b);
    assert_eq!(
        attr.lookup(0),
        Some(TriangleAttribution {
            input: InputId::A,
            face: 0
        }),
        "2 A-F0-verts + 1 novel → majority F0"
    );
}

#[test]
pub(crate) fn boolean_no_majority_returns_none() {
    // 1 A-vert + 1 B-vert + 1 novel → no majority, None.
    let a = two_face_shared_vertex_brep();
    let mut b_verts = a.vertices().to_vec();
    for v in &mut b_verts {
        v.point = Point3::new(v.point.x() + 100.0, v.point.y(), v.point.z());
    }
    let b = BRep::new(b_verts, a.edges().to_vec(), a.faces().to_vec()).unwrap();
    let mixed = Mesh::new(
        vec![
            p(1.0, 0.0, 0.0),     // matches a.verts[1] (A, F0)
            p(101.0, 0.0, 0.0),   // matches b.verts[1] (B, F0)
            p(500.0, 500.0, 0.0), // novel
        ],
        vec![[0, 1, 2]],
    );
    let attr = substitute_attribution(&mixed, &a, &b);
    assert_eq!(
        attr.lookup(0),
        None,
        "1 A + 1 B + 1 novel → no 2-of-3 majority"
    );
}

#[test]
pub(crate) fn boolean_tie_break_picks_lowest_face() {
    // Triangle (V0 shared, V1 F0-only, V3 F1-only) → candidates
    // {F0,F1}, {F0}, {F1}. Counts: F0=2, F1=2. Tie. Lowest face → F0.
    let a = two_face_shared_vertex_brep();
    let b = two_face_shared_vertex_brep();
    let tie_mesh = Mesh::new(
        vec![
            p(0.0, 0.0, 0.0), // V0 — shared
            p(1.0, 0.0, 0.0), // V1 — F0 only
            p(0.0, 1.0, 0.0), // V3 — F1 only
        ],
        vec![[0, 1, 2]],
    );
    let attr = substitute_attribution(&tie_mesh, &a, &b);
    assert_eq!(
        attr.lookup(0),
        Some(TriangleAttribution {
            input: InputId::A,
            face: 0
        }),
        "tie at count 2 between F0 and F1 → lowest face (F0)"
    );
}

// ----- PR-YR4: Group 3 — empty-topology degradation (substitute) -----

#[test]
pub(crate) fn boolean_both_inputs_from_mesh_all_none() {
    let a = BRep::from_mesh(sample_mesh());
    let b = BRep::from_mesh(sample_mesh());
    let attr = substitute_attribution(&sample_mesh(), &a, &b);
    assert_eq!(attr.len(), sample_mesh().num_tris());
    assert_eq!(
        attr.lookup(0),
        None,
        "from_mesh inputs have all-Unknown sources → all-None attribution"
    );
}

#[test]
pub(crate) fn boolean_mixed_from_mesh_and_topologized() {
    // a has topology, b is from_mesh. Substitute over a's mesh.
    // Attribution should reflect a's per-tri face ownership.
    let a = two_face_shared_vertex_brep();
    let b = BRep::from_mesh(sample_mesh());
    let attr = substitute_attribution(a.as_mesh(), &a, &b);
    assert_eq!(
        attr.lookup(0),
        Some(TriangleAttribution {
            input: InputId::A,
            face: 0
        })
    );
    assert_eq!(
        attr.lookup(1),
        Some(TriangleAttribution {
            input: InputId::A,
            face: 1
        })
    );
}
