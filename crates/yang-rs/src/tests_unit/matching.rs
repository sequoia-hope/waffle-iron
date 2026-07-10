#[allow(unused_imports)]
use super::*;

// ----- PR-YR1 backward-compat: existing boolean dispatch tests -----

#[test]
pub(crate) fn brep_from_mesh_as_mesh_round_trip() {
    let m = sample_mesh();
    let b = BRep::from_mesh(m.clone());
    assert_eq!(b.as_mesh(), &m);
}

#[test]
pub(crate) fn brep_into_mesh_returns_wrapped() {
    let m = sample_mesh();
    let b = BRep::from_mesh(m.clone());
    assert_eq!(b.into_mesh(), m);
}

#[test]
pub(crate) fn brep_counts_delegate_to_mesh() {
    let m = sample_mesh();
    let b = BRep::from_mesh(m.clone());
    assert_eq!(b.num_verts(), m.num_verts());
    assert_eq!(b.num_tris(), m.num_tris());
}

#[test]
pub(crate) fn yang_error_display_non_empty() {
    for e in [
        YangError::NonManifoldInput,
        YangError::NonManifoldOutput,
        YangError::MeshBooleanFailed(Box::from("test")),
        YangError::MalformedTopology("test".to_string()),
    ] {
        let msg = format!("{}", e);
        assert!(!msg.is_empty(), "empty Display for {e:?}");
    }
}

#[test]
pub(crate) fn yang_error_source_propagates() {
    let inner: Box<dyn Error + Send + Sync> = Box::from("inner");
    let e = YangError::MeshBooleanFailed(inner);
    let src = e.source().expect("source should be Some");
    assert_eq!(src.to_string(), "inner");
}

#[test]
pub(crate) fn boolean_with_ok_backend() {
    // M3: boolean() consumes a LabeledArrangement. An empty arrangement
    // (0 tris) keeps nothing → empty output BRep, Ok.
    let a = BRep::from_mesh(sample_mesh());
    let b = BRep::from_mesh(sample_mesh());
    let backend = LabelMockBackend::new(empty_arrangement());
    let r = boolean(&a, &b, BoolOp::Union, &backend).unwrap();
    assert_eq!(r.num_verts(), 0);
}

#[test]
pub(crate) fn boolean_with_err_backend() {
    let a = BRep::from_mesh(sample_mesh());
    let b = BRep::from_mesh(sample_mesh());
    let mock = MockBackend;
    match boolean(&a, &b, BoolOp::Union, &mock) {
        Err(YangError::MeshBooleanFailed(_)) => {}
        other => panic!("expected MeshBooleanFailed, got {:?}", other),
    }
}

#[test]
pub(crate) fn boolean_dispatches_all_four_ops() {
    // M3: an empty arrangement is keep-set-empty for every op → Ok.
    let a = BRep::from_mesh(sample_mesh());
    let b = BRep::from_mesh(sample_mesh());
    for op in [
        BoolOp::Union,
        BoolOp::Intersect,
        BoolOp::Subtract,
        BoolOp::Xor,
    ] {
        let backend = LabelMockBackend::new(empty_arrangement());
        assert!(boolean(&a, &b, op, &backend).is_ok(), "op {op:?}");
    }
}

// ----- PR-YR3: Group 1 — TessellationSource::Intersection variant -----

#[test]
pub(crate) fn intersection_variant_constructs_and_matches() {
    let s = TessellationSource::Intersection;
    match s {
        TessellationSource::Intersection => {}
        _ => panic!("wrong variant"),
    }
}

#[test]
pub(crate) fn intersection_distinct_from_unknown() {
    assert_ne!(
        TessellationSource::Intersection,
        TessellationSource::Unknown
    );
}

// ----- PR-YR3: Group 2 — MATCH_TOLERANCE constant -----

#[test]
pub(crate) fn match_tolerance_is_1e_minus_9() {
    assert_eq!(MATCH_TOLERANCE, 1e-9);
}

// ----- PR-YR3: Group 3 — Spatial matching via mock backend -----

/// Build a BRep with explicit topology (triangle) so its mesh has
/// non-trivial TessellationMap entries (`BRepVertex(i)` for each i).
pub(crate) fn triangle_brep() -> BRep {
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
        surface: Surface::Plane {
            normal: Vector3::new(0.0, 0.0, 1.0),
            d: 0.0,
        },
        outer_loop: vec![0, 1, 2],
        inner_loops: Vec::new(),
        reversed: false,
    }];
    BRep::new(verts, edges, faces).unwrap()
}

// PR-YR3 spatial-vertex-provenance was REMOVED from production by M3
// (production tessellation_map is now BRepVertex(i) 1:1 with the kept
// sub-mesh). Per Manager policy (a), these tests are reworked to call
// the now-#[cfg(test)] substitute helper `match_with_input` DIRECTLY,
// preserving the substitute's coverage as the M4 oracle rather than
// routing through production `boolean()`.

#[test]
pub(crate) fn boolean_input_a_verbatim_copies_a_map() {
    let a = triangle_brep();
    let b = triangle_brep();
    // Each of A's mesh verts matches input A's BRepVertex(i).
    for (i, &target) in a.as_mesh().verts.iter().enumerate() {
        let (input, src) = match_with_input(&a, &b, target);
        assert_eq!(input, Some(InputId::A), "vert {i} should match A");
        assert_eq!(
            src,
            TessellationSource::BRepVertex(i as u32),
            "output vertex {i}"
        );
    }
}

#[test]
pub(crate) fn boolean_input_b_verbatim_copies_b_map() {
    let a = triangle_brep();
    // B has different vertices so A's spatial match fails first.
    let mut b_verts = a.vertices().to_vec();
    for v in &mut b_verts {
        v.point = Point3::new(v.point.x() + 10.0, v.point.y(), v.point.z());
    }
    let b = BRep::new(b_verts, a.edges().to_vec(), a.faces().to_vec()).unwrap();
    for (i, &target) in b.as_mesh().verts.iter().enumerate() {
        let (input, src) = match_with_input(&a, &b, target);
        assert_eq!(input, Some(InputId::B), "vert {i} should match B");
        assert_eq!(
            src,
            TessellationSource::BRepVertex(i as u32),
            "output vertex {i} — should match input B's BRepVertex({i})"
        );
    }
}

#[test]
pub(crate) fn boolean_all_new_coords_are_intersection() {
    let a = triangle_brep();
    let b = triangle_brep();
    // Coords far from both inputs → no match → Intersection.
    for target in [
        p(100.0, 100.0, 100.0),
        p(101.0, 100.0, 100.0),
        p(100.0, 101.0, 100.0),
    ] {
        let (input, src) = match_with_input(&a, &b, target);
        assert_eq!(input, None);
        assert_eq!(
            src,
            TessellationSource::Intersection,
            "novel coord should be Intersection"
        );
    }
}

#[test]
pub(crate) fn boolean_mixed_match_and_intersection() {
    let a = triangle_brep();
    let b = triangle_brep();
    // 2 verts from A + 2 new coords.
    let expectations = [
        (p(0.0, 0.0, 0.0), TessellationSource::BRepVertex(0)),
        (p(1.0, 0.0, 0.0), TessellationSource::BRepVertex(1)),
        (p(99.0, 99.0, 0.0), TessellationSource::Intersection),
        (p(98.0, 98.0, 0.0), TessellationSource::Intersection),
    ];
    for (i, (target, expect)) in expectations.into_iter().enumerate() {
        let (_input, src) = match_with_input(&a, &b, target);
        assert_eq!(src, expect, "vertex {i}");
    }
}
