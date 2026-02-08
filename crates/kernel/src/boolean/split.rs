use crate::geometry::curves::{Curve, Line3d};
use crate::geometry::point::Point3d;
use crate::geometry::surfaces::Surface;
use crate::topology::brep::*;
use crate::Tolerance;

/// Result of splitting a planar face along a line.
#[derive(Debug)]
pub struct SplitResult {
    /// The two faces produced by the split.
    pub face_a: FaceId,
    pub face_b: FaceId,
}

/// Split a planar face along an infinite line that lies in the face's plane.
///
/// The line must intersect the face boundary at exactly 2 points (entry and exit).
/// This creates two new faces that together cover the original face, and removes
/// the original face from the shell.
///
/// Returns `None` if the line does not cross the face boundary at exactly 2 points,
/// or if the face is not planar.
pub fn split_planar_face_by_line(
    store: &mut EntityStore,
    face_id: FaceId,
    line: &Line3d,
    tol: &Tolerance,
) -> Option<SplitResult> {
    let face = &store.faces[face_id];
    let plane = match &face.surface {
        Surface::Plane(p) => *p,
        _ => return None,
    };
    let shell_id = face.shell;
    let same_sense = face.same_sense;

    // Step 1: Find where the splitting line intersects the face boundary edges.
    let outer_loop_id = face.outer_loop;
    let half_edges: Vec<HalfEdgeId> = store.loops[outer_loop_id].half_edges.clone();

    let mut intersections: Vec<EdgeIntersection> = Vec::new();

    for (idx, &he_id) in half_edges.iter().enumerate() {
        let he = &store.half_edges[he_id];
        let start = store.vertices[he.start_vertex].point;
        let end = store.vertices[he.end_vertex].point;

        if let Some(hit) = line_segment_intersection(line, &start, &end, tol) {
            intersections.push(EdgeIntersection {
                he_index: idx,
                he_id,
                point: hit.point,
                t_on_segment: hit.t_segment,
            });
        }
    }

    // Deduplicate intersections at shared vertices. When the line passes through
    // a vertex, both adjacent edges report a hit (one at t_seg~1, the next at t_seg~0).
    // Keep only one intersection per geometric point (preferring the one at a segment start).
    intersections.sort_by(|a, b| {
        a.he_index
            .cmp(&b.he_index)
            .then(a.t_on_segment.partial_cmp(&b.t_on_segment).unwrap())
    });
    intersections.dedup_by(|a, b| a.point.distance_to(&b.point) < tol.coincidence);

    if intersections.len() != 2 {
        return None;
    }

    // Sort by position around the loop (half-edge index, then t_on_segment).
    intersections.sort_by(|a, b| {
        a.he_index
            .cmp(&b.he_index)
            .then(a.t_on_segment.partial_cmp(&b.t_on_segment).unwrap())
    });

    let int0 = &intersections[0];
    let int1 = &intersections[1];

    // Step 2: Create split vertices at intersection points (or reuse existing vertices).
    let split_v0 = find_or_create_vertex(store, &int0.point, tol);
    let split_v1 = find_or_create_vertex(store, &int1.point, tol);

    // Step 3: Build the two new loops.
    //
    // Loop A: from split_v0, along the original boundary (split half-edges)
    //         to split_v1, then back to split_v0 along the splitting line.
    //
    // Loop B: from split_v1, along the remaining boundary to split_v0,
    //         then back to split_v1 along the splitting line (reversed).
    //
    // The original loop goes: ... he[i0] ... he[i1] ...
    // We split he[i0] at int0 into (he_before_0, he_after_0)
    // We split he[i1] at int1 into (he_before_1, he_after_1)
    //
    // Loop A = [he_after_0, he[i0+1], ..., he[i1-1], he_before_1, split_line_forward]
    // Loop B = [he_after_1, he[i1+1], ..., he[i0-1], he_before_0, split_line_reverse]

    let i0 = int0.he_index;
    let i1 = int1.he_index;
    let n = half_edges.len();

    // Split the two boundary half-edges.
    let (before_0, after_0) = split_half_edge_at_point(store, int0.he_id, split_v0, &int0.point, tol);
    let (before_1, after_1) = split_half_edge_at_point(store, int1.he_id, split_v1, &int1.point, tol);

    // Create the splitting edge (line segment from split_v0 to split_v1).
    let split_line = Line3d::from_points(
        store.vertices[split_v0].point,
        store.vertices[split_v1].point,
    );
    let split_dist = store.vertices[split_v0].point.distance_to(&store.vertices[split_v1].point);

    // Create two faces with new loops.
    let loop_a_id = store.loops.insert(Loop {
        half_edges: vec![],
        face: FaceId::default(),
    });
    let face_a_id = store.faces.insert(Face {
        surface: Surface::Plane(plane),
        outer_loop: loop_a_id,
        inner_loops: vec![],
        same_sense,
        shell: shell_id,
    });
    store.loops[loop_a_id].face = face_a_id;

    let loop_b_id = store.loops.insert(Loop {
        half_edges: vec![],
        face: FaceId::default(),
    });
    let face_b_id = store.faces.insert(Face {
        surface: Surface::Plane(plane),
        outer_loop: loop_b_id,
        inner_loops: vec![],
        same_sense,
        shell: shell_id,
    });
    store.loops[loop_b_id].face = face_b_id;

    // Build loop A half-edges: after_0, middle edges, before_1, split_line_forward
    let mut loop_a_hes: Vec<HalfEdgeId> = Vec::new();
    if let Some(a0) = after_0 {
        reassign_half_edge(store, a0, face_a_id, loop_a_id);
        loop_a_hes.push(a0);
    }
    // Add original half-edges between i0+1 and i1-1 (exclusive of i0 and i1).
    for idx in circular_range(i0 + 1, i1, n) {
        let he = half_edges[idx];
        reassign_half_edge(store, he, face_a_id, loop_a_id);
        loop_a_hes.push(he);
    }
    if let Some(b1) = before_1 {
        reassign_half_edge(store, b1, face_a_id, loop_a_id);
        loop_a_hes.push(b1);
    }

    // Create the forward splitting half-edge (split_v1 -> split_v0 would be reverse).
    // For loop A: we go from the end of before_1 (split_v1) back to split_v0.
    let split_he_a = store.half_edges.insert_with_key(|_| HalfEdge {
        edge: EdgeId::default(),
        twin: HalfEdgeId::default(),
        face: face_a_id,
        loop_id: loop_a_id,
        start_vertex: split_v1,
        end_vertex: split_v0,
        t_start: 0.0,
        t_end: split_dist,
        forward: false,
    });
    loop_a_hes.push(split_he_a);

    // Build loop B half-edges: after_1, remaining edges, before_0, split_line_reverse
    let mut loop_b_hes: Vec<HalfEdgeId> = Vec::new();
    if let Some(a1) = after_1 {
        reassign_half_edge(store, a1, face_b_id, loop_b_id);
        loop_b_hes.push(a1);
    }
    for idx in circular_range(i1 + 1, i0, n) {
        let he = half_edges[idx];
        reassign_half_edge(store, he, face_b_id, loop_b_id);
        loop_b_hes.push(he);
    }
    if let Some(b0) = before_0 {
        reassign_half_edge(store, b0, face_b_id, loop_b_id);
        loop_b_hes.push(b0);
    }

    // For loop B: we go from split_v0 to split_v1 (forward direction).
    let split_he_b = store.half_edges.insert_with_key(|_| HalfEdge {
        edge: EdgeId::default(),
        twin: HalfEdgeId::default(),
        face: face_b_id,
        loop_id: loop_b_id,
        start_vertex: split_v0,
        end_vertex: split_v1,
        t_start: 0.0,
        t_end: split_dist,
        forward: true,
    });
    loop_b_hes.push(split_he_b);

    // Create the splitting edge and link twins.
    let split_edge_id = store.edges.insert(Edge {
        curve: Curve::Line(split_line),
        half_edges: (split_he_a, split_he_b),
        start_vertex: split_v0,
        end_vertex: split_v1,
    });
    store.half_edges[split_he_a].edge = split_edge_id;
    store.half_edges[split_he_b].edge = split_edge_id;
    store.half_edges[split_he_a].twin = split_he_b;
    store.half_edges[split_he_b].twin = split_he_a;

    // Assign half-edges to loops.
    store.loops[loop_a_id].half_edges = loop_a_hes;
    store.loops[loop_b_id].half_edges = loop_b_hes;

    // Step 4: Update the shell: remove original face, add new faces.
    let shell = &mut store.shells[shell_id];
    shell.faces.retain(|&f| f != face_id);
    shell.faces.push(face_a_id);
    shell.faces.push(face_b_id);

    Some(SplitResult {
        face_a: face_a_id,
        face_b: face_b_id,
    })
}

// ─── Internal helpers ────────────────────────────────────────────────────────

#[derive(Debug)]
struct EdgeIntersection {
    he_index: usize,
    he_id: HalfEdgeId,
    point: Point3d,
    t_on_segment: f64,
}

struct SegmentLineHit {
    point: Point3d,
    t_segment: f64,
}

/// Intersect an infinite line with a line segment [start, end].
/// Returns the intersection point if it lies within the segment (t in [0, 1]).
fn line_segment_intersection(
    line: &Line3d,
    start: &Point3d,
    end: &Point3d,
    tol: &Tolerance,
) -> Option<SegmentLineHit> {
    let seg_dir = *end - *start;
    let seg_len = seg_dir.length();
    if seg_len < tol.coincidence {
        return None;
    }

    // Use the same closest-points-on-two-lines approach as intersection.rs.
    // Line 1 (infinite): P1(t) = line.origin + t * line.direction
    // Line 2 (segment):  P2(s) = start + s * seg_dir, s in [0,1]
    // w = l1.origin - l2.origin (same convention as line_line_closest)
    let w = line.origin - *start;
    let d1 = line.direction;
    let d2 = seg_dir;

    let a = d1.dot(&d1); // always 1.0 since line.direction is normalized
    let b = d1.dot(&d2);
    let c = d2.dot(&d2);
    let d = d1.dot(&w);
    let e = d2.dot(&w);

    let denom = a * c - b * b;
    if denom.abs() < tol.angular * tol.angular {
        // Lines are parallel.
        return None;
    }

    let t_line = (b * e - c * d) / denom;
    let t_seg = (a * e - b * d) / denom; // parameter on segment in [0,1] range

    // Check that the intersection is within the segment bounds.
    if t_seg < -tol.coincidence / seg_len || t_seg > 1.0 + tol.coincidence / seg_len {
        return None;
    }

    // Verify the intersection is close (lines might be skew in 3D).
    let p_on_line = line.evaluate(t_line);
    let p_on_seg = *start + seg_dir * t_seg;
    let dist = p_on_line.distance_to(&p_on_seg);

    if dist > tol.coincidence {
        return None;
    }

    let point = p_on_line.midpoint(&p_on_seg);
    Some(SegmentLineHit {
        point,
        t_segment: t_seg.clamp(0.0, 1.0),
    })
}

/// Find an existing vertex at the given point, or create a new one.
fn find_or_create_vertex(store: &mut EntityStore, point: &Point3d, tol: &Tolerance) -> VertexId {
    for (vid, v) in &store.vertices {
        if v.point.distance_to(point) < tol.coincidence {
            return vid;
        }
    }
    store.vertices.insert(Vertex {
        point: *point,
        tolerance: tol.coincidence,
    })
}

/// Split a half-edge at a point, returning (before, after) half-edge IDs.
/// If the split point is at the start vertex, returns (None, original_he).
/// If the split point is at the end vertex, returns (original_he, None).
fn split_half_edge_at_point(
    store: &mut EntityStore,
    he_id: HalfEdgeId,
    split_vertex: VertexId,
    split_point: &Point3d,
    tol: &Tolerance,
) -> (Option<HalfEdgeId>, Option<HalfEdgeId>) {
    let he = store.half_edges[he_id];
    let start = store.vertices[he.start_vertex].point;
    let end = store.vertices[he.end_vertex].point;
    let seg_len = start.distance_to(&end);

    // Check if split is at an existing vertex.
    if split_point.distance_to(&start) < tol.coincidence {
        return (None, Some(he_id));
    }
    if split_point.distance_to(&end) < tol.coincidence {
        return (Some(he_id), None);
    }

    // Compute parameter for the split.
    let t_split = start.distance_to(split_point) / seg_len;
    let t_mid = he.t_start + (he.t_end - he.t_start) * t_split;

    // Create the "before" half-edge: start_vertex -> split_vertex
    let before_he = store.half_edges.insert_with_key(|_| HalfEdge {
        edge: he.edge,
        twin: HalfEdgeId::default(),
        face: he.face,
        loop_id: he.loop_id,
        start_vertex: he.start_vertex,
        end_vertex: split_vertex,
        t_start: he.t_start,
        t_end: t_mid,
        forward: he.forward,
    });

    // Create the "before" edge.
    let before_line = Line3d::from_points(start, *split_point);
    let before_edge = store.edges.insert(Edge {
        curve: Curve::Line(before_line),
        half_edges: (before_he, before_he),
        start_vertex: he.start_vertex,
        end_vertex: split_vertex,
    });
    store.half_edges[before_he].edge = before_edge;

    // Create the "after" half-edge: split_vertex -> end_vertex
    let after_he = store.half_edges.insert_with_key(|_| HalfEdge {
        edge: he.edge,
        twin: HalfEdgeId::default(),
        face: he.face,
        loop_id: he.loop_id,
        start_vertex: split_vertex,
        end_vertex: he.end_vertex,
        t_start: t_mid,
        t_end: he.t_end,
        forward: he.forward,
    });

    // Create the "after" edge.
    let after_line = Line3d::from_points(*split_point, end);
    let after_edge = store.edges.insert(Edge {
        curve: Curve::Line(after_line),
        half_edges: (after_he, after_he),
        start_vertex: split_vertex,
        end_vertex: he.end_vertex,
    });
    store.half_edges[after_he].edge = after_edge;

    (Some(before_he), Some(after_he))
}

/// Reassign a half-edge to a new face and loop.
fn reassign_half_edge(store: &mut EntityStore, he_id: HalfEdgeId, face: FaceId, loop_id: LoopId) {
    store.half_edges[he_id].face = face;
    store.half_edges[he_id].loop_id = loop_id;
}

/// Generate indices in a circular range [start, end) modulo n.
/// If start == end, returns empty.
fn circular_range(start: usize, end: usize, n: usize) -> Vec<usize> {
    let mut result = Vec::new();
    let mut i = start % n;
    let end = end % n;
    while i != end {
        result.push(i);
        i = (i + 1) % n;
    }
    result
}

// ═════════════════════════════════════════════════════════════════════════════
// Tests
// ═════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::vector::Vec3;
    use crate::topology::primitives::make_box;

    /// Helper: collect all face IDs of a shell.
    fn shell_faces(store: &EntityStore, shell_id: ShellId) -> Vec<FaceId> {
        store.shells[shell_id].faces.clone()
    }

    /// Helper: count vertices in a face's outer loop.
    fn loop_vertex_count(store: &EntityStore, face_id: FaceId) -> usize {
        let face = &store.faces[face_id];
        store.loops[face.outer_loop].half_edges.len()
    }

    /// Helper: check that a loop is closed (last he's end == first he's start).
    fn is_face_loop_closed(store: &EntityStore, face_id: FaceId) -> bool {
        let face = &store.faces[face_id];
        let loop_data = &store.loops[face.outer_loop];
        if loop_data.half_edges.is_empty() {
            return false;
        }
        let first = &store.half_edges[loop_data.half_edges[0]];
        let last = &store.half_edges[*loop_data.half_edges.last().unwrap()];
        first.start_vertex == last.end_vertex
    }

    /// Helper: compute the approximate area of a planar face by summing
    /// the signed area of triangles from the centroid.
    fn face_area(store: &EntityStore, face_id: FaceId) -> f64 {
        let face = &store.faces[face_id];
        let plane = match &face.surface {
            Surface::Plane(p) => p,
            _ => return 0.0,
        };
        let loop_data = &store.loops[face.outer_loop];
        let vertices: Vec<Point3d> = loop_data
            .half_edges
            .iter()
            .map(|&he_id| store.vertices[store.half_edges[he_id].start_vertex].point)
            .collect();

        if vertices.len() < 3 {
            return 0.0;
        }

        // Use the Shoelace formula in 2D (project onto the plane).
        let uvs: Vec<(f64, f64)> = vertices.iter().map(|p| plane.parameters_of(p)).collect();
        let n = uvs.len();
        let mut area = 0.0;
        for i in 0..n {
            let j = (i + 1) % n;
            area += uvs[i].0 * uvs[j].1;
            area -= uvs[j].0 * uvs[i].1;
        }
        area.abs() / 2.0
    }

    #[test]
    fn split_box_face_through_center() {
        // Create a box [0,0,0] to [10,10,10] and split one face (e.g., the top
        // face z=10) with a line along x=5.
        let mut store = EntityStore::new();
        let solid = make_box(&mut store, 0.0, 0.0, 0.0, 10.0, 10.0, 10.0);

        // Find the top face (z=10, normal +Z).
        let shell_id = store.solids[solid].shells[0];
        let top_face = shell_faces(&store, shell_id)
            .into_iter()
            .find(|&fid| {
                let f = &store.faces[fid];
                if let Surface::Plane(p) = &f.surface {
                    p.normal.z > 0.9 && p.origin.z > 9.0
                } else {
                    false
                }
            })
            .expect("Should find top face");

        let original_area = face_area(&store, top_face);
        assert!(original_area > 0.0, "Top face should have positive area");

        let tol = Tolerance::default();

        // Split with a line at x=5, parallel to Y, in the z=10 plane.
        let split_line = Line3d::new(Point3d::new(5.0, 0.0, 10.0), Vec3::Y);
        let result = split_planar_face_by_line(&mut store, top_face, &split_line, &tol);

        assert!(result.is_some(), "Split should succeed");
        let split = result.unwrap();

        // Both new faces should have closed loops.
        assert!(is_face_loop_closed(&store, split.face_a), "Face A loop should be closed");
        assert!(is_face_loop_closed(&store, split.face_b), "Face B loop should be closed");

        // Each new face should have at least 3 vertices.
        let va = loop_vertex_count(&store, split.face_a);
        let vb = loop_vertex_count(&store, split.face_b);
        assert!(va >= 3, "Face A should have >= 3 vertices, got {}", va);
        assert!(vb >= 3, "Face B should have >= 3 vertices, got {}", vb);

        // The areas should sum to the original area.
        let area_a = face_area(&store, split.face_a);
        let area_b = face_area(&store, split.face_b);
        assert!(
            (area_a + area_b - original_area).abs() < 1e-6,
            "Areas should sum to original: {} + {} = {} vs {}",
            area_a, area_b, area_a + area_b, original_area
        );

        // Each half should be approximately half the area.
        assert!(
            (area_a - 50.0).abs() < 1e-6,
            "Face A area should be ~50, got {}",
            area_a
        );
        assert!(
            (area_b - 50.0).abs() < 1e-6,
            "Face B area should be ~50, got {}",
            area_b
        );

        // The original face should no longer be in the shell.
        let current_faces = shell_faces(&store, shell_id);
        assert!(
            !current_faces.contains(&top_face),
            "Original face should be removed from shell"
        );
        assert!(current_faces.contains(&split.face_a));
        assert!(current_faces.contains(&split.face_b));

        // Shell should now have 7 faces (6 original - 1 + 2 new).
        assert_eq!(current_faces.len(), 7, "Shell should have 7 faces after split");
    }

    #[test]
    fn split_box_face_off_center() {
        // Split the top face at x=3 (asymmetric).
        let mut store = EntityStore::new();
        let solid = make_box(&mut store, 0.0, 0.0, 0.0, 10.0, 10.0, 10.0);

        let shell_id = store.solids[solid].shells[0];
        let top_face = shell_faces(&store, shell_id)
            .into_iter()
            .find(|&fid| {
                let f = &store.faces[fid];
                if let Surface::Plane(p) = &f.surface {
                    p.normal.z > 0.9 && p.origin.z > 9.0
                } else {
                    false
                }
            })
            .expect("Should find top face");

        let tol = Tolerance::default();
        let split_line = Line3d::new(Point3d::new(3.0, 0.0, 10.0), Vec3::Y);
        let result = split_planar_face_by_line(&mut store, top_face, &split_line, &tol);

        assert!(result.is_some());
        let split = result.unwrap();

        let area_a = face_area(&store, split.face_a);
        let area_b = face_area(&store, split.face_b);

        // One face should be 30 (3*10), the other 70 (7*10).
        let (small, large) = if area_a < area_b {
            (area_a, area_b)
        } else {
            (area_b, area_a)
        };
        assert!(
            (small - 30.0).abs() < 1e-6,
            "Smaller face area should be ~30, got {}",
            small
        );
        assert!(
            (large - 70.0).abs() < 1e-6,
            "Larger face area should be ~70, got {}",
            large
        );
    }

    #[test]
    fn split_preserves_twin_on_splitting_edge() {
        let mut store = EntityStore::new();
        let solid = make_box(&mut store, 0.0, 0.0, 0.0, 10.0, 10.0, 10.0);

        let shell_id = store.solids[solid].shells[0];
        let top_face = shell_faces(&store, shell_id)
            .into_iter()
            .find(|&fid| {
                let f = &store.faces[fid];
                if let Surface::Plane(p) = &f.surface {
                    p.normal.z > 0.9 && p.origin.z > 9.0
                } else {
                    false
                }
            })
            .expect("Should find top face");

        let tol = Tolerance::default();
        let split_line = Line3d::new(Point3d::new(5.0, 0.0, 10.0), Vec3::Y);
        let result = split_planar_face_by_line(&mut store, top_face, &split_line, &tol).unwrap();

        // Find the splitting half-edges (they should be twins of each other).
        let loop_a = &store.loops[store.faces[result.face_a].outer_loop];
        let loop_b = &store.loops[store.faces[result.face_b].outer_loop];

        // The last half-edge in each loop should be the splitting edge.
        let split_he_a = *loop_a.half_edges.last().unwrap();
        let split_he_b = *loop_b.half_edges.last().unwrap();

        assert_eq!(
            store.half_edges[split_he_a].twin,
            split_he_b,
            "Split half-edges should be twins"
        );
        assert_eq!(
            store.half_edges[split_he_b].twin,
            split_he_a,
            "Split half-edges should be twins (reverse)"
        );

        // Their vertices should be swapped.
        assert_eq!(
            store.half_edges[split_he_a].start_vertex,
            store.half_edges[split_he_b].end_vertex
        );
        assert_eq!(
            store.half_edges[split_he_a].end_vertex,
            store.half_edges[split_he_b].start_vertex
        );
    }

    #[test]
    fn split_returns_none_for_non_intersecting_line() {
        let mut store = EntityStore::new();
        let solid = make_box(&mut store, 0.0, 0.0, 0.0, 10.0, 10.0, 10.0);

        let shell_id = store.solids[solid].shells[0];
        let top_face = shell_faces(&store, shell_id)
            .into_iter()
            .find(|&fid| {
                let f = &store.faces[fid];
                if let Surface::Plane(p) = &f.surface {
                    p.normal.z > 0.9 && p.origin.z > 9.0
                } else {
                    false
                }
            })
            .expect("Should find top face");

        let tol = Tolerance::default();

        // Line at x=20, completely outside the face.
        let split_line = Line3d::new(Point3d::new(20.0, 0.0, 10.0), Vec3::Y);
        let result = split_planar_face_by_line(&mut store, top_face, &split_line, &tol);
        assert!(result.is_none(), "Should return None for non-intersecting line");
    }

    #[test]
    fn split_at_vertex_diagonal() {
        // Split the top face diagonally from corner to corner.
        let mut store = EntityStore::new();
        let solid = make_box(&mut store, 0.0, 0.0, 0.0, 10.0, 10.0, 10.0);

        let shell_id = store.solids[solid].shells[0];
        let top_face = shell_faces(&store, shell_id)
            .into_iter()
            .find(|&fid| {
                let f = &store.faces[fid];
                if let Surface::Plane(p) = &f.surface {
                    p.normal.z > 0.9 && p.origin.z > 9.0
                } else {
                    false
                }
            })
            .expect("Should find top face");

        let tol = Tolerance::default();

        // Diagonal line from (0,0,10) to (10,10,10).
        let dir = Vec3::new(1.0, 1.0, 0.0).normalize();
        let split_line = Line3d::new(Point3d::new(0.0, 0.0, 10.0), dir);
        let result = split_planar_face_by_line(&mut store, top_face, &split_line, &tol);

        assert!(result.is_some(), "Diagonal split should succeed");
        let split = result.unwrap();

        let area_a = face_area(&store, split.face_a);
        let area_b = face_area(&store, split.face_b);

        // Each triangle is half the rectangle = 50.
        assert!(
            (area_a - 50.0).abs() < 1e-5,
            "Triangle area should be ~50, got {}",
            area_a
        );
        assert!(
            (area_b - 50.0).abs() < 1e-5,
            "Triangle area should be ~50, got {}",
            area_b
        );

        // Both loops should be closed.
        assert!(is_face_loop_closed(&store, split.face_a));
        assert!(is_face_loop_closed(&store, split.face_b));
    }
}
