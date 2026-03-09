//! Half-edge B-Rep data structures.
//!
//! A solid is bounded by shells, each shell contains faces, each face has
//! one or more loops (outer + inner), each loop is a ring of half-edges.

/// Strongly-typed indices into the topology arena.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VertexIdx(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HalfEdgeIdx(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EdgeIdx(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LoopIdx(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FaceIdx(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ShellIdx(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SolidIdx(pub usize);

/// A vertex in the B-Rep.
#[derive(Debug, Clone)]
pub struct Vertex {
    pub position: [f64; 3],
    /// One of the half-edges originating at this vertex.
    pub half_edge: Option<HalfEdgeIdx>,
}

/// A directed half-edge. Two half-edges share an Edge (one for each direction).
#[derive(Debug, Clone)]
pub struct HalfEdge {
    /// The vertex this half-edge points FROM.
    pub origin: VertexIdx,
    /// The parent edge (shared with the twin half-edge).
    pub edge: EdgeIdx,
    /// The twin half-edge (opposite direction on the same edge).
    pub twin: HalfEdgeIdx,
    /// Next half-edge in the same loop (counter-clockwise around the face).
    pub next: HalfEdgeIdx,
    /// Previous half-edge in the same loop.
    pub prev: HalfEdgeIdx,
    /// The loop this half-edge belongs to.
    pub loop_: LoopIdx,
}

/// An undirected edge connecting two vertices.
#[derive(Debug, Clone)]
pub struct Edge {
    /// One of the two half-edges.
    pub half_edge: HalfEdgeIdx,
}

/// A loop (ring of half-edges) bounding a face.
#[derive(Debug, Clone)]
pub struct Loop {
    /// One of the half-edges in this loop.
    pub half_edge: HalfEdgeIdx,
    /// The face this loop belongs to.
    pub face: FaceIdx,
}

/// A face bounded by one outer loop and zero or more inner loops (holes).
#[derive(Debug, Clone)]
pub struct Face {
    /// The outer loop.
    pub outer_loop: LoopIdx,
    /// Inner loops (holes).
    pub inner_loops: Vec<LoopIdx>,
    /// The shell this face belongs to.
    pub shell: ShellIdx,
}

/// A shell: a connected, oriented set of faces forming a closed surface.
#[derive(Debug, Clone)]
pub struct Shell {
    /// One of the faces in this shell.
    pub face: FaceIdx,
    /// The solid this shell belongs to.
    pub solid: SolidIdx,
}

/// A solid: one outer shell and zero or more void shells (cavities).
#[derive(Debug, Clone)]
pub struct Solid {
    /// The outer shell.
    pub outer_shell: ShellIdx,
    /// Inner shells (voids/cavities).
    pub inner_shells: Vec<ShellIdx>,
}
