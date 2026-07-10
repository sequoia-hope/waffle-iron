#[allow(unused_imports)]
use super::*;

// ----- PR-YR5: topology reconstruction -----
//
// `reconstruct_topology` is UNCHANGED production. Per Manager policy
// (b), these tests previously routed through `boolean()` via the
// boolean-only MockBackend (which M3 no longer drives); they are
// reworked to build a `TriangleAttributionMap` via the #[cfg(test)]
// substitute and call `reconstruct_topology` DIRECTLY — exercising the
// same durable reconstruction logic without the removed substitute
// production path.

#[test]
pub(crate) fn yr5_single_triangle_round_trip_produces_one_face() {
    // Pure-A on triangle_brep (1 face, 1 fan tri) → 1 face with 3
    // boundary edges + 3 vertices forming a closed cycle.
    let a = triangle_brep();
    let b = triangle_brep();
    let mesh = a.as_mesh().clone();
    let attr = substitute_attribution(&mesh, &a, &b);
    let (verts, edges, faces) = reconstruct_topology(&mesh, &attr, &a, &b).unwrap();
    assert_eq!(faces.len(), 1, "expected 1 BRepFace");
    assert_eq!(faces[0].outer_loop.len(), 3, "expected 3-edge loop");
    assert_eq!(edges.len(), 3, "expected 3 BRepEdges");
    assert_eq!(verts.len(), 3, "expected 3 BRepVertices");
    // Cycle closure
    let f = &faces[0];
    for i in 0..3 {
        let e_curr = &edges[f.outer_loop[i] as usize];
        let e_next = &edges[f.outer_loop[(i + 1) % 3] as usize];
        assert_eq!(
            e_curr.end, e_next.start,
            "cycle break at edge {i}: {} != {}",
            e_curr.end, e_next.start
        );
    }
}

#[test]
pub(crate) fn yr5_two_face_round_trip_produces_two_faces() {
    // two_face_shared_vertex_brep has 2 triangular faces sharing only
    // V0; 2 output tris with different attributions (F0 vs F1) → 2
    // BRepFaces.
    let a = two_face_shared_vertex_brep();
    let b = two_face_shared_vertex_brep();
    let mesh = a.as_mesh().clone();
    let attr = substitute_attribution(&mesh, &a, &b);
    let (_v, _e, faces) = reconstruct_topology(&mesh, &attr, &a, &b).unwrap();
    assert_eq!(faces.len(), 2, "expected 2 BRepFaces");
    for f in &faces {
        assert_eq!(f.outer_loop.len(), 3);
    }
}

#[test]
pub(crate) fn yr5_disconnected_components_become_separate_faces() {
    // Two tris with the SAME attribution but NO shared vertex →
    // flood-fill leaves them as 2 patches → 2 faces. Regression guard
    // vs. naive attribution-bucketing.
    let a = triangle_brep();
    let b = triangle_brep();
    // 6 vertices = TWO copies of A's 3 verts at distinct indices.
    let dup = Mesh::new(
        vec![
            p(0.0, 0.0, 0.0), // matches A.V0
            p(1.0, 0.0, 0.0), // matches A.V1
            p(0.0, 1.0, 0.0), // matches A.V2
            p(0.0, 0.0, 0.0), // duplicate matching A.V0 (different idx)
            p(1.0, 0.0, 0.0), // duplicate matching A.V1
            p(0.0, 1.0, 0.0), // duplicate matching A.V2
        ],
        vec![[0, 1, 2], [3, 4, 5]],
    );
    let attr = substitute_attribution(&dup, &a, &b);
    let (_v, _e, faces) = reconstruct_topology(&dup, &attr, &a, &b).unwrap();
    assert_eq!(
        faces.len(),
        2,
        "disconnected same-attribution tris should be separate faces"
    );
}

#[test]
pub(crate) fn yr5_none_attributed_tris_omitted_from_faces() {
    // tri 0 matches A's verts (Some(A, F0)); tri 1 is all novel coords
    // (None). reconstruct_topology should yield 1 face.
    let a = triangle_brep();
    let b = triangle_brep();
    let mixed = Mesh::new(
        vec![
            p(0.0, 0.0, 0.0), // matches A.V0
            p(1.0, 0.0, 0.0), // matches A.V1
            p(0.0, 1.0, 0.0), // matches A.V2
            p(1000.0, 0.0, 0.0),
            p(1001.0, 0.0, 0.0),
            p(1000.0, 1.0, 0.0),
        ],
        vec![[0, 1, 2], [3, 4, 5]],
    );
    let attr = substitute_attribution(&mixed, &a, &b);
    let (_v, _e, faces) = reconstruct_topology(&mixed, &attr, &a, &b).unwrap();
    assert_eq!(
        faces.len(),
        1,
        "None-attributed tris should not contribute faces"
    );
}

#[test]
pub(crate) fn yr5_vertex_count_matches_mesh() {
    let a = triangle_brep();
    let b = triangle_brep();
    let mesh = a.as_mesh().clone();
    let attr = substitute_attribution(&mesh, &a, &b);
    let (verts, _e, _f) = reconstruct_topology(&mesh, &attr, &a, &b).unwrap();
    assert_eq!(verts.len(), mesh.num_verts());
    for (i, v) in verts.iter().enumerate() {
        assert_eq!(v.point, mesh.verts[i]);
    }
}

#[test]
pub(crate) fn yr5_surface_inherited_from_input() {
    let a = triangle_brep();
    let b = triangle_brep();
    let mesh = a.as_mesh().clone();
    let attr = substitute_attribution(&mesh, &a, &b);
    let (_v, _e, faces) = reconstruct_topology(&mesh, &attr, &a, &b).unwrap();
    assert_eq!(faces.len(), 1);
    assert_eq!(
        faces[0].surface,
        a.faces()[0].surface,
        "output face should inherit input A's surface"
    );
}

#[test]
pub(crate) fn yr5_empty_input_produces_empty_face_set() {
    // Both inputs from_mesh → all-None attribution → no faces/edges.
    let a = BRep::from_mesh(sample_mesh());
    let b = BRep::from_mesh(sample_mesh());
    let mesh = sample_mesh();
    let attr = substitute_attribution(&mesh, &a, &b);
    let (verts, edges, faces) = reconstruct_topology(&mesh, &attr, &a, &b).unwrap();
    assert!(
        faces.is_empty(),
        "all-None attribution should yield empty faces"
    );
    assert!(
        edges.is_empty(),
        "all-None attribution should yield empty edges"
    );
    // Vertices still populated 1:1 with mesh.
    assert_eq!(verts.len(), mesh.num_verts());
}

// ----- Stage-6 degenerate-sliver topology (spec yang_stage6_sliver_topology) -----
//
// Reproduces §2's measured structure at the unit level: a shared collinear
// solid-edge chain a–c–d–b where two abutting faces subdivide it
// DIFFERENTLY, and the arrangement keeps ZERO-AREA shim slivers along the
// chord to stay watertight. One sliver is wound so its directed chord edge
// DUPLICATES the real triangle's chord edge (sign-of-zero winding is
// arbitrary) — the measured fold. Today `reconstruct_topology` dead-ends in
// `patch_boundary_cycle` at `NonManifoldOutput`; the Stage-6 design (spec §4:
// exclude degenerate tris from boundary derivation + loop T-subdivision) must
// reassemble a 2-manifold output whose shared segments are each 2-covered.

/// The shared solid edge is the y-axis (x=0, z=0): the intersection of the
/// two abutting faces' planes z=0 (face 0, apex off +y in z=0) and x=0
/// (face 1, apex off +y in x=0). Chain vertices a<c<d<b sit on the y-axis,
/// exactly collinear, so every sliver along it is exactly zero-area.
///
/// Vertex indices: 0=a 1=b 2=c 3=d 4=x1(face-0 apex) 5=x2(face-1 apex).
pub(crate) fn sliver_fixture_mesh() -> Mesh {
    Mesh::new(
        vec![
            p(0.0, 0.0, 0.0), // 0 = a  (chain end)
            p(0.0, 3.0, 0.0), // 1 = b  (chain end)
            p(0.0, 1.0, 0.0), // 2 = c  (between a,b)
            p(0.0, 2.0, 0.0), // 3 = d  (between a,b)
            p(1.0, 1.5, 0.0), // 4 = x1 (face 0 apex, z=0 plane)
            p(0.0, 1.5, 1.0), // 5 = x2 (face 1 apex, x=0 plane)
        ],
        vec![
            // face 0 (z=0 plane, normal +z): ONE real triangle carrying the
            // whole chord b→a, plus two zero-area shim slivers wound so each
            // DUPLICATES the real directed chord edge b→a (1→0).
            [0, 4, 1], // T1 real: edges a→x1, x1→b, b→a
            [1, 0, 2], // S1 sliver: edges b→a (dup!), a→c, c→b
            [1, 0, 3], // S2 sliver: edges b→a (dup!), a→d, d→b
            // face 1 (x=0 plane, normal +x): the OTHER side subdivides the
            // chain a→c→d→b (opposite direction) via a fan from x2.
            [0, 2, 5], // edges a→c, c→x2, x2→a
            [2, 3, 5], // edges c→d, d→x2, x2→c
            [3, 1, 5], // edges d→b, b→x2, x2→d
        ],
    )
}

/// Attribution for `sliver_fixture_mesh`: face-0 patch = {T1,S1,S2},
/// face-1 patch = {the three fan tris}. Built directly (in-module access to
/// the private field) so the slivers land in face 0's patch deterministically
/// — this is the measured N4-provenance placement (§2.3), not a geometric
/// guess.
pub(crate) fn sliver_fixture_attr() -> TriangleAttributionMap {
    let f0 = Some(TriangleAttribution {
        input: InputId::A,
        face: 0,
    });
    let f1 = Some(TriangleAttribution {
        input: InputId::A,
        face: 1,
    });
    TriangleAttributionMap {
        attributions: vec![f0, f0, f0, f1, f1, f1],
    }
}

/// Canonical undirected key.
pub(crate) fn und(x: u32, y: u32) -> (u32, u32) {
    if x < y {
        (x, y)
    } else {
        (y, x)
    }
}

/// Multiset of undirected loop edges across ALL output faces, derived from
/// each face's `outer_loop` (edge indices) via the returned edge table.
pub(crate) fn loop_edge_counts(
    edges: &[BRepEdge],
    faces: &[BRepFace],
) -> std::collections::BTreeMap<(u32, u32), u32> {
    let mut counts: std::collections::BTreeMap<(u32, u32), u32> = std::collections::BTreeMap::new();
    for f in faces {
        for &ei in &f.outer_loop {
            let e = &edges[ei as usize];
            *counts.entry(und(e.start, e.end)).or_insert(0) += 1;
        }
        for hole in &f.inner_loops {
            for &ei in hole {
                let e = &edges[ei as usize];
                *counts.entry(und(e.start, e.end)).or_insert(0) += 1;
            }
        }
    }
    counts
}

/// TARGET (spec §5 S2/S4). RED today: `reconstruct_topology` dead-ends at
/// `NonManifoldOutput` because sliver S1's directed edge b→a duplicates
/// real T1's b→a, unbalancing face 0's boundary walk. GREEN: slivers are
/// excluded from boundary derivation (A) and face 0's chord is T-subdivided
/// at c,d (B) so every shared segment is 2-covered.
#[test]
pub(crate) fn stage6_sliver_fold_reassembles_with_subdivided_chord() {
    let a = two_face_shared_vertex_brep();
    let b = two_face_shared_vertex_brep();
    let mesh = sliver_fixture_mesh();
    let attr = sliver_fixture_attr();

    let (_verts, edges, faces) = reconstruct_topology(&mesh, &attr, &a, &b).expect(
        "Stage-6 sliver RED: reconstruction must succeed once zero-area slivers are \
             excluded from boundary derivation (spec §4A) — today it dead-ends at \
             NonManifoldOutput on the duplicated chord edge b→a",
    );

    // S2: both real faces survive (slivers carry no boundary of their own).
    assert_eq!(
        faces.len(),
        2,
        "expected 2 output faces (chord side + chain side)"
    );

    let counts = loop_edge_counts(&edges, &faces);

    // S4: the full chord (a,b) must NOT remain a raw loop edge — it is
    // T-subdivided at c,d.
    assert_eq!(
        counts.get(&und(0, 1)).copied().unwrap_or(0),
        0,
        "chord (a,b) must be subdivided at c,d, not carried as a single loop edge; \
             loop edges: {counts:?}"
    );
    // S4: every shared segment of the solid edge is used by exactly two
    // directed loop edges (2-manifold seam).
    for (name, key) in [("a–c", und(0, 2)), ("c–d", und(2, 3)), ("d–b", und(3, 1))] {
        assert_eq!(
            counts.get(&key).copied().unwrap_or(0),
            2,
            "shared segment {name} must be 2-covered across output loops; \
                 loop edges: {counts:?}"
        );
    }
}

/// S5 (spec §5): a patch made ENTIRELY of zero-area slivers cannot bound a
/// face — it must stay loudly `NonManifoldOutput`, never silently emit a
/// degenerate face. Passes today (the fold errors) and must remain Err
/// through the fix (excluding all its triangles leaves no boundary).
#[test]
pub(crate) fn stage6_all_degenerate_patch_stays_loud() {
    let a = two_face_shared_vertex_brep();
    let b = two_face_shared_vertex_brep();
    // A single patch of ONLY collinear slivers on the y-axis (no real tri).
    let mesh = Mesh::new(
        vec![
            p(0.0, 0.0, 0.0), // 0 = a
            p(0.0, 3.0, 0.0), // 1 = b
            p(0.0, 1.0, 0.0), // 2 = c
            p(0.0, 2.0, 0.0), // 3 = d
        ],
        vec![[1, 0, 2], [1, 0, 3]], // two zero-area slivers sharing (a,b)
    );
    let f0 = Some(TriangleAttribution {
        input: InputId::A,
        face: 0,
    });
    let attr = TriangleAttributionMap {
        attributions: vec![f0, f0],
    };
    assert!(
        reconstruct_topology(&mesh, &attr, &a, &b).is_err(),
        "an all-degenerate patch must stay loud (NonManifoldOutput) — it cannot bound a face"
    );
}
