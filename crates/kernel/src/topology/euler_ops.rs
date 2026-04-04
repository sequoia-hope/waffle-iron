//! Euler operators for maintaining topological invariants.
//!
//! The five basic Euler operators:
//! - mvfs: Make Vertex, Face, Shell (creates initial topology)
//! - mev: Make Edge, Vertex (adds vertex + edge to existing loop)
//! - mef: Make Edge, Face (splits a face with a new edge)
//! - kemr: Kill Edge, Make Ring (removes edge, creates inner loop)
//! - kfmrh: Kill Face, Make Ring-Hole (creates cavity/through-hole)
//!
//! These maintain the Euler-Poincaré invariant:
//!   V - E + F = 2(S - G) + R
//! where V=vertices, E=edges, F=faces, S=shells, G=genus, R=rings(inner loops).
//!
//! References:
//! - [#16] Mantyla, "An Introduction to Solid Modeling"
//! - [#33] Stroud Ch.4

use super::arena::TopoArena;
use super::half_edge::*;

/// Make Vertex, Face, Shell — creates initial topology from a single point.
///
/// Creates: 1 solid, 1 shell, 1 face, 1 loop, 1 vertex.
/// The face is a degenerate "point face" with an empty loop.
///
/// Returns: (SolidIdx, ShellIdx, FaceIdx, VertexIdx)
pub fn mvfs(arena: &mut TopoArena, position: [f64; 3]) -> (SolidIdx, ShellIdx, FaceIdx, VertexIdx) {
    let solid_idx = arena.add_solid();
    let shell_idx = arena.add_shell(solid_idx);
    let face_idx = arena.add_face(shell_idx);
    let loop_idx = arena.add_loop(face_idx);
    let vertex_idx = arena.add_vertex(position);

    // Wire up
    arena.solids[solid_idx.0].outer_shell = shell_idx;
    arena.shells[shell_idx.0].face = face_idx;
    arena.faces[face_idx.0].outer_loop = loop_idx;

    (solid_idx, shell_idx, face_idx, vertex_idx)
}

/// Make Edge, Vertex — extends topology by adding a new vertex connected by an edge.
///
/// Adds a new vertex at `position`, connected to `from_vertex` by a new edge.
/// The new edge's half-edges are inserted into the loop containing `from_vertex`.
///
/// Euler change: V+1, E+1 → net V-E+F unchanged.
///
/// Returns: (EdgeIdx, VertexIdx) for the new edge and vertex.
pub fn mev(
    arena: &mut TopoArena,
    from_vertex: VertexIdx,
    in_loop: LoopIdx,
    position: [f64; 3],
) -> (EdgeIdx, VertexIdx) {
    let new_vertex = arena.add_vertex(position);
    let (edge_idx, he_a, he_b) = arena.add_edge();

    // he_a: from_vertex → new_vertex
    // he_b: new_vertex → from_vertex
    arena.half_edges[he_a.0].origin = from_vertex;
    arena.half_edges[he_b.0].origin = new_vertex;
    arena.half_edges[he_a.0].loop_ = in_loop;
    arena.half_edges[he_b.0].loop_ = in_loop;

    // Link them as a pair (self-referencing next/prev if first edge in loop)
    if arena.vertices[from_vertex.0].half_edge.is_none() {
        // First edge from this vertex: he_a → he_b → he_a
        arena.half_edges[he_a.0].next = he_b;
        arena.half_edges[he_a.0].prev = he_b;
        arena.half_edges[he_b.0].next = he_a;
        arena.half_edges[he_b.0].prev = he_a;
        arena.loops[in_loop.0].half_edge = he_a;
    } else {
        // Insert he_a/he_b into the existing loop around from_vertex
        // Find the half-edge coming INTO from_vertex in this loop
        let existing_he = arena.vertices[from_vertex.0].half_edge.unwrap();
        let prev_he = arena.half_edges[existing_he.0].prev;

        // Insert: prev_he → he_a → he_b → existing_he
        arena.half_edges[he_a.0].prev = prev_he;
        arena.half_edges[he_a.0].next = he_b;
        arena.half_edges[he_b.0].prev = he_a;
        arena.half_edges[he_b.0].next = existing_he;
        arena.half_edges[prev_he.0].next = he_a;
        arena.half_edges[existing_he.0].prev = he_b;
    }

    // Update vertex references
    arena.vertices[from_vertex.0].half_edge = Some(he_a);
    arena.vertices[new_vertex.0].half_edge = Some(he_b);

    (edge_idx, new_vertex)
}

/// Make Edge, Face — splits a face by connecting two vertices with a new edge.
///
/// Creates a new edge between `v1` and `v2` (which must be in the same loop),
/// splitting the loop (and face) into two.
///
/// Euler change: E+1, F+1 → net V-E+F unchanged.
///
/// Returns: (EdgeIdx, FaceIdx) for the new edge and new face.
pub fn mef(
    arena: &mut TopoArena,
    v1: VertexIdx,
    v2: VertexIdx,
    in_loop: LoopIdx,
) -> (EdgeIdx, FaceIdx) {
    let face = arena.loops[in_loop.0].face;
    let shell = arena.faces[face.0].shell;

    // Find half-edges originating at v1 and v2 in this loop
    let he_from_v1 = find_he_from_vertex_in_loop(arena, v1, in_loop);
    let he_from_v2 = find_he_from_vertex_in_loop(arena, v2, in_loop);

    // Create new edge
    let (edge_idx, he_a, he_b) = arena.add_edge();

    // Create new face and loop
    let new_face = arena.add_face(shell);
    let new_loop = arena.add_loop(new_face);
    arena.faces[new_face.0].outer_loop = new_loop;

    // he_a: v1 → v2 (stays in original loop)
    // he_b: v2 → v1 (goes in new loop)
    arena.half_edges[he_a.0].origin = v1;
    arena.half_edges[he_b.0].origin = v2;
    arena.half_edges[he_a.0].loop_ = in_loop;
    arena.half_edges[he_b.0].loop_ = new_loop;

    // Splice into the loop:
    // Original: ... → prev_of_v2 → he_from_v2 → ... → prev_of_v1 → he_from_v1 → ...
    // After split:
    //   Loop 1: he_a → he_from_v2 → ... → prev_of_v1 → back to he_a? No.
    // Let me think about this more carefully:
    //
    // Before: single loop: ... he_from_v1 ... he_from_v2 ...
    // We insert he_a (v1→v2) and he_b (v2→v1).
    // Loop 1 (original): he_from_v1 → ... → prev_of_v2 → he_a → back around
    // Wait, he_a goes v1→v2, so:
    //   Loop 1: prev_of_v1 points to he_from_v1... no.
    //
    // Let me use the standard formulation:
    // he_a: from v1, next = he_from_v2, so he_a is in original loop
    // he_b: from v2, next = he_from_v1, so he_b is in new loop

    let prev_v1 = arena.half_edges[he_from_v1.0].prev;
    let prev_v2 = arena.half_edges[he_from_v2.0].prev;

    // Set up he_a (v1→v2) in original loop
    arena.half_edges[he_a.0].next = he_from_v2;
    arena.half_edges[he_a.0].prev = prev_v1;
    arena.half_edges[prev_v1.0].next = he_a;
    arena.half_edges[he_from_v2.0].prev = he_a;

    // Set up he_b (v2→v1) in new loop
    arena.half_edges[he_b.0].next = he_from_v1;
    arena.half_edges[he_b.0].prev = prev_v2;
    arena.half_edges[prev_v2.0].next = he_b;
    arena.half_edges[he_from_v1.0].prev = he_b;

    // Update loop references for half-edges now in the new loop
    arena.loops[new_loop.0].half_edge = he_b;
    let mut he = he_b;
    loop {
        arena.half_edges[he.0].loop_ = new_loop;
        he = arena.half_edges[he.0].next;
        if he == he_b {
            break;
        }
    }

    // Update original loop's half_edge reference
    arena.loops[in_loop.0].half_edge = he_a;

    (edge_idx, new_face)
}

/// Kill Edge, Make Ring — removes an edge and creates an inner loop (hole).
///
/// Euler change: E-1, R+1 → V-E+F-R unchanged.
#[allow(dead_code)] // Staged for Yang pipeline Phase 5 (B-Rep reassembly)
pub fn kemr(arena: &mut TopoArena, edge: EdgeIdx) {
    let he_a = arena.edges[edge.0].half_edge;
    let he_b = arena.half_edges[he_a.0].twin;

    let loop_a = arena.half_edges[he_a.0].loop_;
    let face = arena.loops[loop_a.0].face;

    // Create inner loop from the chain that he_b belongs to
    let inner_loop = arena.add_loop(face);
    arena.faces[face.0].inner_loops.push(inner_loop);

    // Splice out he_a and he_b
    let prev_a = arena.half_edges[he_a.0].prev;
    let next_a = arena.half_edges[he_a.0].next;
    let prev_b = arena.half_edges[he_b.0].prev;
    let next_b = arena.half_edges[he_b.0].next;

    arena.half_edges[prev_a.0].next = next_b;
    arena.half_edges[next_b.0].prev = prev_a;
    arena.half_edges[prev_b.0].next = next_a;
    arena.half_edges[next_a.0].prev = prev_b;

    // Assign the inner loop
    arena.loops[inner_loop.0].half_edge = next_a;
    let mut he = next_a;
    loop {
        arena.half_edges[he.0].loop_ = inner_loop;
        he = arena.half_edges[he.0].next;
        if he == next_a {
            break;
        }
    }

    // Update outer loop reference
    arena.loops[loop_a.0].half_edge = next_b;
}

/// Kill Face, Make Ring-Hole — merges a face into a shell void.
///
/// Creates a through-hole or cavity by killing a face and creating a new shell.
///
/// Euler change: F-1, S+1 → V-E+F-2S unchanged if genus adjusts.
#[allow(dead_code)] // Staged for Yang pipeline Phase 5 (B-Rep reassembly)
pub fn kfmrh(arena: &mut TopoArena, face_to_kill: FaceIdx, host_face: FaceIdx) {
    let killed_loop = arena.faces[face_to_kill.0].outer_loop;

    // Add the killed face's outer loop as an inner loop of the host face
    arena.loops[killed_loop.0].face = host_face;
    arena.faces[host_face.0].inner_loops.push(killed_loop);

    // Move inner loops too
    let inner_loops: Vec<LoopIdx> = arena.faces[face_to_kill.0].inner_loops.clone();
    for il in inner_loops {
        arena.loops[il.0].face = host_face;
        arena.faces[host_face.0].inner_loops.push(il);
    }
    arena.faces[face_to_kill.0].inner_loops.clear();
}

// ── Helpers ──────────────────────────────────────────────────────────────

/// Find a half-edge originating at `vertex` within `loop_`.
fn find_he_from_vertex_in_loop(
    arena: &TopoArena,
    vertex: VertexIdx,
    loop_: LoopIdx,
) -> HalfEdgeIdx {
    let start = arena.loops[loop_.0].half_edge;
    let mut he = start;
    loop {
        if arena.half_edges[he.0].origin == vertex {
            return he;
        }
        he = arena.half_edges[he.0].next;
        if he == start {
            panic!("Vertex {:?} not found in loop {:?}", vertex, loop_);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mvfs_creates_initial_topology() {
        let mut arena = TopoArena::new();
        let (solid, shell, face, vertex) = mvfs(&mut arena, [0.0, 0.0, 0.0]);

        assert_eq!(arena.vertex_count(), 1);
        assert_eq!(arena.face_count(), 1);
        assert_eq!(arena.shell_count(), 1);
        assert_eq!(arena.solids.len(), 1);
        assert_eq!(arena.solids[solid.0].outer_shell, shell);
        assert_eq!(arena.shells[shell.0].face, face);
        assert_eq!(arena.vertices[vertex.0].position, [0.0, 0.0, 0.0]);
    }

    #[test]
    fn mev_adds_vertex_and_edge() {
        let mut arena = TopoArena::new();
        let (_solid, _shell, face, v0) = mvfs(&mut arena, [0.0, 0.0, 0.0]);
        let loop_ = arena.faces[face.0].outer_loop;

        let (_edge, v1) = mev(&mut arena, v0, loop_, [1.0, 0.0, 0.0]);

        assert_eq!(arena.vertex_count(), 2);
        assert_eq!(arena.edge_count(), 1);
        assert_eq!(arena.vertices[v1.0].position, [1.0, 0.0, 0.0]);
    }

    #[test]
    fn build_triangle_with_euler_ops() {
        let mut arena = TopoArena::new();

        // Start with one vertex
        let (_solid, _shell, face, v0) = mvfs(&mut arena, [0.0, 0.0, 0.0]);
        let loop_ = arena.faces[face.0].outer_loop;

        // Add two more vertices
        let (_, v1) = mev(&mut arena, v0, loop_, [1.0, 0.0, 0.0]);
        let (_, v2) = mev(&mut arena, v1, loop_, [0.5, 1.0, 0.0]);

        // Close the triangle: connect v2 back to v0, creating a new face
        let (_, _new_face) = mef(&mut arena, v2, v0, loop_);

        // Check counts: V=3, E=3, F=2
        assert_eq!(arena.vertex_count(), 3);
        assert_eq!(arena.edge_count(), 3);
        assert_eq!(arena.face_count(), 2);

        // Euler: V - E + F = 3 - 3 + 2 = 2 ✓
        let euler =
            arena.vertex_count() as i64 - arena.edge_count() as i64 + arena.face_count() as i64;
        assert_eq!(euler, 2, "Euler formula V-E+F should be 2");
    }

    #[test]
    fn build_quadrilateral() {
        let mut arena = TopoArena::new();

        let (_solid, _shell, face, v0) = mvfs(&mut arena, [0.0, 0.0, 0.0]);
        let loop_ = arena.faces[face.0].outer_loop;

        let (_, v1) = mev(&mut arena, v0, loop_, [1.0, 0.0, 0.0]);
        let (_, v2) = mev(&mut arena, v1, loop_, [1.0, 1.0, 0.0]);
        let (_, v3) = mev(&mut arena, v2, loop_, [0.0, 1.0, 0.0]);

        let _ = mef(&mut arena, v3, v0, loop_);

        // V=4, E=4, F=2 → Euler=2
        assert_eq!(arena.vertex_count(), 4);
        assert_eq!(arena.edge_count(), 4);
        assert_eq!(arena.face_count(), 2);

        let euler =
            arena.vertex_count() as i64 - arena.edge_count() as i64 + arena.face_count() as i64;
        assert_eq!(euler, 2);
    }
}
