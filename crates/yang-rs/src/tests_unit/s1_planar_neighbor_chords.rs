//! P0008 / P0009 (2026-09-30) — the planar-neighbor chord rule of the
//! holed-lateral CDT (`stage1_tessellate/planar_neighbor_chords.rs`).
//!
//! A cone (or cylinder) face bounded by a plane's section curve is
//! triangulated boundary-only; where the curve is locally convex in the
//! chart the CDT emits an interior chord between two samples of it. Both
//! ends lie in the neighbor's plane, so the chord is a segment of the
//! neighbor's sheet — the doubled triangle Stage 0 rejects as
//! `i6-edge-overuse` one boolean later (P0009), and the render double cover
//! P0008 pins on the kernel-v2 side. The pass splits every such chord at
//! its midpoint lifted onto the surface. Exercised here on the smallest
//! configuration that carries one: two triangles sharing a chord whose
//! ends both lie on the same planar neighbor.

use crate::*;

/// Cylinder of radius `r` about the z axis: the point at azimuth `theta`,
/// height `z`.
fn cyl_point(r: f64, theta: f64, z: f64) -> Point3 {
    Point3::new(r * theta.cos(), r * theta.sin(), z)
}

/// Two triangles `[a, b, c]` / `[b, a, d]` sharing the chord `a–b`; `a` and
/// `b` are boundary samples of a section curve against planar face 7, `c`
/// and `d` interior-ish vertices tagged by nothing. The chord is INTERIOR
/// (not a boundary edge) and its ends share tag 7 ⇒ exactly one split: a
/// fifth vertex ON the cylinder at the chord's mid-azimuth and mid-height,
/// four triangles, the chord gone.
#[test]
fn shared_planar_neighbor_chord_is_split_on_the_surface() {
    let r = 2.0;
    let mut verts = vec![
        cyl_point(r, 0.10, 1.0), // 0 = a (on the neighbor plane's curve)
        cyl_point(r, 0.50, 1.4), // 1 = b (on the same curve)
        cyl_point(r, 0.30, 0.2), // 2 = c
        cyl_point(r, 0.30, 2.5), // 3 = d
    ];
    let mut sources: Vec<TessellationSource> = (0..4).map(TessellationSource::BRepVertex).collect();
    let mut tris = vec![[0u32, 1, 2], [1, 0, 3]];
    let mut tags: std::collections::HashMap<u32, [Option<u32>; 2]> = Default::default();
    tags.insert(0, [Some(7), None]);
    tags.insert(1, [Some(7), Some(9)]);
    // Only the outer ring's own segments are boundary; the chord a–b is not.
    let is_boundary =
        |x: u32, y: u32| matches!((x.min(y), x.max(y)), (0, 2) | (1, 2) | (0, 3) | (1, 3));
    let n = split_planar_neighbor_chords(
        5,
        &mut tris,
        &tags,
        &is_boundary,
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(0.0, 0.0, 1.0),
        &mut verts,
        &mut sources,
    );
    assert_eq!(n, 1, "exactly one shared-neighbor chord to split");
    assert_eq!(verts.len(), 5);
    assert_eq!(tris.len(), 4);
    // The minted vertex is ON the cylinder at the chord's mid-azimuth and
    // mid-height (radius interpolated: constant here).
    let m = verts[4].as_array();
    let rm = (m[0] * m[0] + m[1] * m[1]).sqrt();
    assert!((rm - r).abs() < 1e-12, "lifted radius {rm} vs {r}");
    assert!((m[1].atan2(m[0]) - 0.30).abs() < 1e-12, "mid-azimuth");
    assert!((m[2] - 1.2).abs() < 1e-12, "mid-height");
    assert!(matches!(
        sources[4],
        TessellationSource::BRepFace { face: 5, .. }
    ));
    // No triangle keeps the chord; every triangle is a half of an original.
    for t in &tris {
        assert!(
            !(t.contains(&0) && t.contains(&1)),
            "chord 0–1 survived in {t:?}"
        );
        assert!(
            t.contains(&4),
            "every triangle is incident to the split vertex: {t:?}"
        );
    }
    // Idempotent: a second pass finds nothing (the minted vertex is untagged).
    let n2 = split_planar_neighbor_chords(
        5,
        &mut tris,
        &tags,
        &is_boundary,
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(0.0, 0.0, 1.0),
        &mut verts,
        &mut sources,
    );
    assert_eq!(n2, 0);
}

/// A chord whose ends lie on DIFFERENT planar neighbors (a corner between
/// two flanks) is a legitimate interior edge and stays; so does a boundary
/// segment between two tagged samples (the shared boundary itself).
#[test]
fn chords_to_a_different_neighbor_and_boundary_segments_stay() {
    let r = 2.0;
    let mut verts = vec![
        cyl_point(r, 0.10, 1.0),
        cyl_point(r, 0.50, 1.4),
        cyl_point(r, 0.30, 0.2),
        cyl_point(r, 0.30, 2.5),
    ];
    let mut sources: Vec<TessellationSource> = (0..4).map(TessellationSource::BRepVertex).collect();
    let mut tris = vec![[0u32, 1, 2], [1, 0, 3]];
    let mut tags: std::collections::HashMap<u32, [Option<u32>; 2]> = Default::default();
    tags.insert(0, [Some(7), None]);
    tags.insert(1, [Some(8), None]);
    let never = |_: u32, _: u32| false;
    let n = split_planar_neighbor_chords(
        5,
        &mut tris,
        &tags,
        &never,
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(0.0, 0.0, 1.0),
        &mut verts,
        &mut sources,
    );
    assert_eq!(n, 0, "different neighbors: no split");
    // Same neighbor, but the chord IS a boundary segment.
    tags.insert(1, [Some(7), None]);
    let chord_is_boundary = |x: u32, y: u32| (x.min(y), x.max(y)) == (0, 1);
    let n = split_planar_neighbor_chords(
        5,
        &mut tris,
        &tags,
        &chord_is_boundary,
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(0.0, 0.0, 1.0),
        &mut verts,
        &mut sources,
    );
    assert_eq!(
        n, 0,
        "a boundary segment is the shared edge itself: no split"
    );
    assert_eq!(tris.len(), 2);
    assert_eq!(verts.len(), 4);
}

/// The edge → planar-face table and the per-vertex tags: an edge bounding a
/// planar and a curved face tags the curved face's boundary vertices with
/// the planar one; loop corners carry both incident edges' neighbors.
#[test]
fn edge_planar_faces_and_vertex_tags() {
    let plane = |edges: Vec<u32>| BRepFace {
        surface: Surface::Plane {
            normal: Vector3::new(0.0, 0.0, 1.0),
            d: 0.0,
        },
        outer_loop: edges,
        inner_loops: vec![],
        reversed: false,
    };
    let cyl = |edges: Vec<u32>| BRepFace {
        surface: Surface::Cylinder {
            axis_point: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            radius: 1.0,
        },
        outer_loop: edges,
        inner_loops: vec![],
        reversed: false,
    };
    // Faces: 0 = planar A (edges 0,1), 1 = the cylinder (edges 0,2,3),
    // 2 = planar B (edges 2,4), 3 = another curved face (edge 3).
    let faces = vec![
        plane(vec![0, 1]),
        cyl(vec![0, 2, 3]),
        plane(vec![2, 4]),
        cyl(vec![3]),
    ];
    let table = edge_planar_faces(&faces, 5);
    assert_eq!(table[0], [Some(0), None]);
    assert_eq!(table[1], [Some(0), None]);
    assert_eq!(table[2], [Some(2), None]);
    assert_eq!(table[3], [None, None]);
    assert_eq!(table[4], [Some(2), None]);
    // The cylinder's outer polyline: vertex 10 → 11 on edge 0, 11 → 12 on
    // edge 2, 12 → 10 on edge 3 (attributed as `loop_polyline_attributed`
    // reports it: each vertex with the edge its FOLLOWING segment lies on).
    let loops = vec![vec![(10u32, 0u32), (11, 2), (12, 3)]];
    let tags = vertex_planar_tags(1, &loops, &table);
    assert_eq!(
        tags[&10],
        [Some(0), None],
        "corner of edges 3 (curved) and 0 (plane A)"
    );
    assert_eq!(
        tags[&11],
        [Some(2), Some(0)],
        "corner of edges 0 (plane A) and 2 (plane B)"
    );
    assert_eq!(
        tags[&12],
        [Some(2), None],
        "corner of edges 2 (plane B) and 3 (curved)"
    );
}
