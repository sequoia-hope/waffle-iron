use tracing::{debug, instrument};

use super::brep::*;
use crate::geometry::curves::{Curve, Line3d};
use crate::geometry::point::Point3d;
use crate::geometry::surfaces::Surface;

/// Euler operators for topological modification.
/// These maintain the Euler-Poincaré formula: V - E + F = 2 * (S - G) + R
/// where S = shells, G = genus (handles), R = rings (inner loops).
///
/// For simple genus-0 closed solids: V - E + F = 2 per shell.

/// Make Vertex, Edge, Face, Shell, Solid — creates the initial topology.
/// Creates a degenerate solid with a single vertex, no real edges/faces.
/// Used as the seed for building up topology.
#[instrument(skip(store), fields(point = ?[point.x, point.y, point.z]))]
pub fn mvfs(store: &mut EntityStore, point: Point3d) -> (SolidId, ShellId, VertexId) {
    let solid_id = store.solids.insert(Solid { shells: vec![] });
    let shell_id = store.shells.insert(Shell {
        faces: vec![],
        orientation: ShellOrientation::Outward,
        solid: solid_id,
    });
    let vertex_id = store.vertices.insert(Vertex {
        point,
        tolerance: crate::default_tolerance().coincidence,
    });

    store.solids[solid_id].shells.push(shell_id);

    debug!(?solid_id, ?shell_id, ?vertex_id, "mvfs created seed topology");
    (solid_id, shell_id, vertex_id)
}

/// Make Edge, Vertex — splits a vertex by adding an edge and a new vertex.
/// The new edge connects the existing vertex to the new vertex.
#[instrument(skip(store), fields(new_point = ?[new_point.x, new_point.y, new_point.z]))]
pub fn mev(
    store: &mut EntityStore,
    existing_vertex: VertexId,
    new_point: Point3d,
    loop_id: LoopId,
) -> (EdgeId, VertexId) {
    let new_vertex = store.vertices.insert(Vertex {
        point: new_point,
        tolerance: crate::default_tolerance().coincidence,
    });

    let start_point = store.vertices[existing_vertex].point;
    let line = Line3d::from_points(start_point, new_point);

    // Create half-edges (will be properly linked later)
    let he1_id = store.half_edges.insert_with_key(|_| {
        // Placeholder — will be filled in
        HalfEdge {
            edge: EdgeId::default(),
            twin: HalfEdgeId::default(),
            face: FaceId::default(),
            loop_id,
            start_vertex: existing_vertex,
            end_vertex: new_vertex,
            t_start: 0.0,
            t_end: start_point.distance_to(&new_point),
            forward: true,
        }
    });

    let he2_id = store.half_edges.insert_with_key(|_| {
        HalfEdge {
            edge: EdgeId::default(),
            twin: HalfEdgeId::default(),
            face: FaceId::default(),
            loop_id,
            start_vertex: new_vertex,
            end_vertex: existing_vertex,
            t_start: start_point.distance_to(&new_point),
            t_end: 0.0,
            forward: false,
        }
    });

    let edge_id = store.edges.insert(Edge {
        curve: Curve::Line(line),
        half_edges: (he1_id, he2_id),
        start_vertex: existing_vertex,
        end_vertex: new_vertex,
    });

    // Link everything
    store.half_edges[he1_id].edge = edge_id;
    store.half_edges[he1_id].twin = he2_id;
    store.half_edges[he2_id].edge = edge_id;
    store.half_edges[he2_id].twin = he1_id;

    debug!(?edge_id, ?new_vertex, "mev created edge and vertex");
    (edge_id, new_vertex)
}

/// Make Edge, Face — adds an edge between two existing vertices in a loop,
/// splitting the loop into two, creating a new face.
#[instrument(skip(store, surface))]
pub fn mef(
    store: &mut EntityStore,
    v1: VertexId,
    v2: VertexId,
    existing_loop: LoopId,
    shell_id: ShellId,
    surface: Surface,
) -> (EdgeId, FaceId) {
    let p1 = store.vertices[v1].point;
    let p2 = store.vertices[v2].point;
    let line = Line3d::from_points(p1, p2);

    // Create the new face and loop
    let new_loop = store.loops.insert(Loop {
        half_edges: vec![],
        face: FaceId::default(), // will be set
    });

    let new_face = store.faces.insert(Face {
        surface,
        outer_loop: new_loop,
        inner_loops: vec![],
        same_sense: true,
        shell: shell_id,
    });

    store.loops[new_loop].face = new_face;
    store.shells[shell_id].faces.push(new_face);

    // Create half-edges for the new edge
    let he1_id = store.half_edges.insert_with_key(|_| HalfEdge {
        edge: EdgeId::default(),
        twin: HalfEdgeId::default(),
        face: store.loops[existing_loop].face,
        loop_id: existing_loop,
        start_vertex: v1,
        end_vertex: v2,
        t_start: 0.0,
        t_end: p1.distance_to(&p2),
        forward: true,
    });

    let he2_id = store.half_edges.insert_with_key(|_| HalfEdge {
        edge: EdgeId::default(),
        twin: HalfEdgeId::default(),
        face: new_face,
        loop_id: new_loop,
        start_vertex: v2,
        end_vertex: v1,
        t_start: p1.distance_to(&p2),
        t_end: 0.0,
        forward: false,
    });

    let edge_id = store.edges.insert(Edge {
        curve: Curve::Line(line),
        half_edges: (he1_id, he2_id),
        start_vertex: v1,
        end_vertex: v2,
    });

    store.half_edges[he1_id].edge = edge_id;
    store.half_edges[he1_id].twin = he2_id;
    store.half_edges[he2_id].edge = edge_id;
    store.half_edges[he2_id].twin = he1_id;

    // Add he1 to existing loop, he2 to new loop
    store.loops[existing_loop].half_edges.push(he1_id);
    store.loops[new_loop].half_edges.push(he2_id);

    debug!(?edge_id, ?new_face, "mef created edge and face");
    (edge_id, new_face)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mvfs() {
        let mut store = EntityStore::new();
        let (solid_id, shell_id, vertex_id) = mvfs(&mut store, Point3d::ORIGIN);

        assert_eq!(store.solids[solid_id].shells.len(), 1);
        assert_eq!(store.shells[shell_id].faces.len(), 0);
        assert!(store.vertices[vertex_id].point.distance_to(&Point3d::ORIGIN) < 1e-12);
    }

    #[test]
    fn test_tracing_does_not_affect_euler_ops() {
        let mut store = EntityStore::new();
        let (solid_id, _shell_id, v0) = mvfs(&mut store, Point3d::new(0.0, 0.0, 0.0));

        // Create a loop so we can use mev
        let loop_id = store.loops.insert(Loop {
            half_edges: vec![],
            face: FaceId::default(),
        });

        let (_e1, v1) = mev(&mut store, v0, Point3d::new(1.0, 0.0, 0.0), loop_id);

        // Verify topology is consistent
        assert_eq!(store.solids[solid_id].shells.len(), 1);
        assert!(store.vertices[v0].point.distance_to(&Point3d::new(0.0, 0.0, 0.0)) < 1e-12);
        assert!(store.vertices[v1].point.distance_to(&Point3d::new(1.0, 0.0, 0.0)) < 1e-12);
    }
}
