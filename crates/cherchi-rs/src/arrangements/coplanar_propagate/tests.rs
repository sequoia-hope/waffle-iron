//! PR-3 unit tests for the coplanar propagate step + its bucketing.
//!
//! These exercise the standalone `build_coplanar_adjacency` /
//! `bucket_coplanar_intersections` / `propagate_coplanar_intersections`
//! pipeline directly (nothing in `mesh_arrangement` calls them yet, so the
//! corpus assay is byte-identical). The gate is the overlap-matching
//! property from the plan's PR-3 row: after build + bucket + propagate, BOTH
//! coplanar triangles must carry the SAME set of overlap intersection points
//! in their interior buckets AND the SAME overlap segments.
//!
//! All coordinates are hard-coded (determinism). Fixtures match PR-2's
//! `classify_coplanar_pair` fixtures so the plane/coords agree.

use crate::arrangements::aux_structure::exact_point_coords;
use crate::arrangements::coplanar_propagate::{
    bucket_coplanar_intersections, build_coplanar_adjacency, propagate_coplanar_intersections,
};
use crate::arrangements::fast_trimesh::VertexCoords;
use crate::arrangements::{classify_pair, FastTrimesh, PairClassification, Plane, TypedPoint};
use cad_primitives::Point3;
use dashu::rational::RBig;
use std::collections::BTreeSet;

/// Build a 2-triangle soup. Triangle A = index 0 (verts 0,1,2),
/// triangle B = index 1 (verts 3,4,5). Mirrors PR-2's `soup_pair`.
fn soup_pair(a: [Point3; 3], b: [Point3; 3]) -> FastTrimesh {
    let verts = vec![a[0], a[1], a[2], b[0], b[1], b[2]];
    let tris = vec![[0u32, 1, 2], [3u32, 4, 5]];
    FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap()
}

/// Classify one pair (0, 1) and assert it is fully coplanar.
fn classify_coplanar(soup: &FastTrimesh) -> Vec<((u32, u32), PairClassification)> {
    let c = classify_pair(soup, 0, 1);
    assert!(
        matches!(c, PairClassification::Coplanar { .. }),
        "fixture must classify Coplanar, got {c:?}"
    );
    vec![((0u32, 1u32), c)]
}

/// Exact rational coords of an interned point (panics on a degenerate
/// generator — none of these fixtures produce one).
fn xc(points: &[TypedPoint], id: u32) -> [RBig; 3] {
    exact_point_coords(&points[id as usize].coords).expect("fixture points have exact coords")
}

/// The set of exact coordinates of a triangle's interior bucket.
fn interior_coord_set(points: &[TypedPoint], interior: &[u32]) -> BTreeSet<[RBig; 3]> {
    interior.iter().map(|&id| xc(points, id)).collect()
}

/// The set of exact endpoint-coordinate PAIRS of a triangle's segments
/// (each pair canonicalized so direction does not matter).
fn segment_coord_set(
    points: &[TypedPoint],
    segs: &[(u32, u32)],
) -> BTreeSet<([RBig; 3], [RBig; 3])> {
    segs.iter()
        .map(|&(a, b)| {
            let ca = xc(points, a);
            let cb = xc(points, b);
            if ca <= cb {
                (ca, cb)
            } else {
                (cb, ca)
            }
        })
        .collect()
}

fn r(x: f64, y: f64, z: f64) -> [RBig; 3] {
    exact_point_coords(&VertexCoords::Explicit(Point3::new(x, y, z))).unwrap()
}

// ════════════════════════════════════════════════════════════════
// build_coplanar_adjacency
// ════════════════════════════════════════════════════════════════

#[test]
fn adjacency_built_from_coplanar_classification() {
    // The PR-2 nested fixture is fully coplanar → one symmetric pair.
    let big = [
        Point3::new(0.0, 0.0, 0.0),
        Point3::new(10.0, 0.0, 0.0),
        Point3::new(0.0, 10.0, 0.0),
    ];
    let small = [
        Point3::new(2.0, 2.0, 0.0),
        Point3::new(5.0, 2.0, 0.0),
        Point3::new(2.0, 5.0, 0.0),
    ];
    let soup = soup_pair(big, small);
    let classified = classify_coplanar(&soup);
    let adj = build_coplanar_adjacency(&soup, &classified);
    assert!(adj.triangle_has_coplanars(0));
    assert!(adj.triangle_has_coplanars(1));
    assert_eq!(adj.coplanar_triangles(0), &[1]);
    assert_eq!(adj.coplanar_triangles(1), &[0]);
}

#[test]
fn adjacency_empty_when_no_coplanar_pairs() {
    // A transversal (non-coplanar) pair contributes no coplanar adjacency.
    let a = [
        Point3::new(0.0, 0.0, 0.0),
        Point3::new(4.0, 0.0, 0.0),
        Point3::new(0.0, 4.0, 0.0),
    ];
    let b = [
        Point3::new(1.0, 1.0, -1.0),
        Point3::new(1.5, 0.5, 1.0),
        Point3::new(0.5, 1.5, 1.0),
    ];
    let soup = soup_pair(a, b);
    let c = classify_pair(&soup, 0, 1);
    assert!(matches!(c, PairClassification::Transversal { .. }));
    let adj = build_coplanar_adjacency(&soup, &[((0u32, 1u32), c)]);
    assert!(!adj.triangle_has_coplanars(0));
    assert!(!adj.triangle_has_coplanars(1));
}

// ════════════════════════════════════════════════════════════════
// THE PR-3 GATE — NESTED: small triangle strictly inside the big one.
// ════════════════════════════════════════════════════════════════

/// PR-2's `coplanar_nested_small_inside_big` fixture, propagated.
///
/// The small triangle's 3 corners are STRICTLY inside the big triangle and
/// each is a corner of the small triangle. After bucketing, the small triangle
/// (id 1) owns its 3 corners ON its own edges (its boundary), with the 3
/// boundary segments held by the big triangle (id 0), which has them only as
/// boundary geometry it does not own (they fall on no big-edge / no big-corner).
///
/// After PROPAGATE the big triangle ABSORBS the small's 3 corners into its
/// INTERIOR (they are strictly inside big, not big corners) and carries the
/// small's 3 boundary segments. The small triangle gains nothing new from
/// big (big's own corners are OUTSIDE the small triangle).
///
/// Gate assertion: big.interior gains exactly the 3 small corners; small
/// gains nothing in interior; and big absorbs the small's segments.
#[test]
fn nested_big_absorbs_small_corners_and_segments() {
    let big = [
        Point3::new(0.0, 0.0, 0.0),
        Point3::new(10.0, 0.0, 0.0),
        Point3::new(0.0, 10.0, 0.0),
    ];
    let small = [
        Point3::new(2.0, 2.0, 0.0),
        Point3::new(5.0, 2.0, 0.0),
        Point3::new(2.0, 5.0, 0.0),
    ];
    let soup = soup_pair(big, small);
    let classified = classify_coplanar(&soup);
    let adj = build_coplanar_adjacency(&soup, &classified);
    let mut cb = bucket_coplanar_intersections(&soup, &classified);

    // Snapshot the small triangle's interior BEFORE propagate (should stay
    // empty: its 3 corners are on its own boundary, not interior, and are
    // its own corners so dropped from its interior anyway).
    let small_interior_before = interior_coord_set(&cb.points, &cb.buckets[1].interior);

    propagate_coplanar_intersections(
        &soup,
        &adj,
        &cb.points,
        &mut cb.buckets,
        &mut cb.tri_segments,
    );

    // Big triangle's interior must now hold EXACTLY the 3 small corners.
    let big_interior = interior_coord_set(&cb.points, &cb.buckets[0].interior);
    let expected: BTreeSet<[RBig; 3]> = [r(2.0, 2.0, 0.0), r(5.0, 2.0, 0.0), r(2.0, 5.0, 0.0)]
        .into_iter()
        .collect();
    assert_eq!(
        big_interior, expected,
        "big triangle must absorb the 3 small corners into its interior, got {:?}",
        cb.buckets[0]
    );

    // The small triangle gains NOTHING in its interior (big's corners are
    // outside small).
    let small_interior_after = interior_coord_set(&cb.points, &cb.buckets[1].interior);
    assert_eq!(
        small_interior_after, small_interior_before,
        "small triangle interior must be unchanged by propagate, got {:?}",
        cb.buckets[1]
    );

    // The small triangle's 3 overlap segments coincide with the small
    // triangle's OWN mesh edges (its corners are the overlap boundary). The
    // C++ `addSymbolicSegment` / `!triContainsEdge` rule therefore does NOT
    // add them to the small triangle's segment list (they are already its
    // edges) — but DOES add all three to the big triangle (big has none of
    // them as an edge). Gate: big absorbed the 3 boundary segments; small's
    // segment list is empty.
    let big_segs = segment_coord_set(&cb.points, &cb.tri_segments[0]);
    let small_segs = segment_coord_set(&cb.points, &cb.tri_segments[1]);
    let expected_segs: BTreeSet<([RBig; 3], [RBig; 3])> = [
        (r(2.0, 2.0, 0.0), r(5.0, 2.0, 0.0)),
        (r(5.0, 2.0, 0.0), r(2.0, 5.0, 0.0)),
        (r(2.0, 5.0, 0.0), r(2.0, 2.0, 0.0)),
    ]
    .into_iter()
    .map(|(a, b)| if a <= b { (a, b) } else { (b, a) })
    .collect();
    assert_eq!(
        big_segs, expected_segs,
        "big triangle must carry the 3 small-boundary overlap segments, got {:?}",
        cb.tri_segments[0]
    );
    assert!(
        small_segs.is_empty(),
        "small triangle's overlap segments are its own mesh edges → not in its segment list, got {:?}",
        cb.tri_segments[1]
    );
}

// ════════════════════════════════════════════════════════════════
// THE PR-3 GATE — OVERLAP MATCHING: the gear right-triangle pair.
// ════════════════════════════════════════════════════════════════

/// PR-2's exact gear fixture: A = (0,-5),(1,-5),(1,5); B = (1,-2),(0,-2),(0,2)
/// in z=0. Their thin wedges overlap. The plan's PR-3 gate: after build +
/// bucket + propagate, BOTH triangles must carry the SAME set of overlap
/// intersection points in their interior buckets AND the SAME overlap
/// segments — i.e. each triangle absorbed its partner's interior-of-overlap
/// points/segments.
///
/// "Same set" here is over the OVERLAP geometry that is interior to BOTH
/// triangles (a point on triangle X's own boundary edge is NOT interior to X
/// even though it is interior to its partner; the propagate is symmetric only
/// on the strictly-shared interior, which is what the C++ guarantees and what
/// the pocket dedup in PR-4 keys on). We therefore assert the STRONGER,
/// well-defined property the C++ propagate produces: the two triangles'
/// segment sets are EQUAL (every overlap segment appears in both, because
/// `addSymbolicSegment` writes both triangles AND propagate re-derives the
/// partner's), and every point that is strictly interior to BOTH triangles is
/// in BOTH interior buckets.
#[test]
fn gear_overlap_points_and_segments_match_in_both_triangles() {
    let a = [
        Point3::new(0.0, -5.0, 0.0),
        Point3::new(1.0, -5.0, 0.0),
        Point3::new(1.0, 5.0, 0.0),
    ];
    let b = [
        Point3::new(1.0, -2.0, 0.0),
        Point3::new(0.0, -2.0, 0.0),
        Point3::new(0.0, 2.0, 0.0),
    ];
    let soup = soup_pair(a, b);
    let classified = classify_coplanar(&soup);
    let adj = build_coplanar_adjacency(&soup, &classified);
    let mut cb = bucket_coplanar_intersections(&soup, &classified);

    propagate_coplanar_intersections(
        &soup,
        &adj,
        &cb.points,
        &mut cb.buckets,
        &mut cb.tri_segments,
    );

    // (1) The two triangles' overlap SEGMENT sets must be EQUAL.
    let segs_a = segment_coord_set(&cb.points, &cb.tri_segments[0]);
    let segs_b = segment_coord_set(&cb.points, &cb.tri_segments[1]);
    assert_eq!(
        segs_a, segs_b,
        "both coplanar triangles must carry the SAME overlap segment set:\n  A = {segs_a:?}\n  B = {segs_b:?}"
    );
    assert!(
        !segs_a.is_empty(),
        "the wedges overlap → at least one overlap segment, got none"
    );

    // (2) Every point STRICTLY interior to BOTH triangles must be present in
    //     BOTH interior buckets (the symmetric absorb). We compute the set of
    //     interned points strictly inside both, then check membership.
    let int_a = interior_coord_set(&cb.points, &cb.buckets[0].interior);
    let int_b = interior_coord_set(&cb.points, &cb.buckets[1].interior);

    // Points strictly inside both triangles, taken from the union of both
    // interior buckets (propagate should have made these symmetric).
    let strictly_inside_both: BTreeSet<[RBig; 3]> = cb
        .points
        .iter()
        .enumerate()
        .filter_map(|(i, _)| {
            let id = i as u32;
            let in_a = super::generic_point_inside_triangle(&soup, 0, id, &cb.points, true);
            let in_b = super::generic_point_inside_triangle(&soup, 1, id, &cb.points, true);
            if in_a && in_b {
                Some(xc(&cb.points, id))
            } else {
                None
            }
        })
        .collect();

    for p in &strictly_inside_both {
        assert!(
            int_a.contains(p),
            "point {p:?} strictly inside both must be in A.interior; A={int_a:?}"
        );
        assert!(
            int_b.contains(p),
            "point {p:?} strictly inside both must be in B.interior; B={int_b:?}"
        );
    }
}

// ════════════════════════════════════════════════════════════════
// Idempotence / no-op safety: propagate over a transversal-only set
// does nothing (no coplanar adjacency).
// ════════════════════════════════════════════════════════════════

#[test]
fn propagate_noop_without_coplanar_pairs() {
    let a = [
        Point3::new(0.0, 0.0, 0.0),
        Point3::new(4.0, 0.0, 0.0),
        Point3::new(0.0, 4.0, 0.0),
    ];
    let b = [
        Point3::new(1.0, 1.0, -1.0),
        Point3::new(1.5, 0.5, 1.0),
        Point3::new(0.5, 1.5, 1.0),
    ];
    let soup = soup_pair(a, b);
    let c = classify_pair(&soup, 0, 1);
    let classified = vec![((0u32, 1u32), c)];
    let adj = build_coplanar_adjacency(&soup, &classified);
    let mut cb = bucket_coplanar_intersections(&soup, &classified);
    // No coplanar pairs → empty buckets/segments and propagate is a no-op.
    assert!(cb.points.is_empty());
    assert!(cb
        .buckets
        .iter()
        .all(|t| t.interior.is_empty() && t.edges.iter().all(|e| e.is_empty())));
    assert!(cb.tri_segments.iter().all(|s| s.is_empty()));
    propagate_coplanar_intersections(
        &soup,
        &adj,
        &cb.points,
        &mut cb.buckets,
        &mut cb.tri_segments,
    );
    assert!(cb.buckets.iter().all(|t| t.interior.is_empty()));
    assert!(cb.tri_segments.iter().all(|s| s.is_empty()));
}
