//! TopoArena — typed storage for all topology entities.

use super::half_edge::*;

/// Arena-based storage for the half-edge B-Rep data structure.
/// All entities are stored contiguously and referenced by typed indices.
#[derive(Debug, Clone, Default)]
pub struct TopoArena {
    pub vertices: Vec<Vertex>,
    pub half_edges: Vec<HalfEdge>,
    pub edges: Vec<Edge>,
    pub loops: Vec<Loop>,
    pub faces: Vec<Face>,
    pub shells: Vec<Shell>,
    pub solids: Vec<Solid>,

    /// PR-Y24: construction-time directed-edge mapping per half-edge,
    /// populated at the close of `topology_extract::extract_topology`
    /// Step 7 from `directed_he` keys. The validator's NMM-vs-missing-edge
    /// predicate consults this rather than re-deriving from arena
    /// traversal (which is polluted on open-chain wrap-backs at
    /// topology_extract.rs L1131-1146; banked Layer-2 residual PR-Y25+).
    /// Empty for arenas constructed via paths other than yang topology
    /// extraction (e.g. legacy S-H builders); validator falls back to
    /// arena-traversal keying when empty (preserves byte-identity for
    /// non-yang code paths). Indexed by `HalfEdgeIdx.0`; entry is the
    /// `(origin, dest)` `BrepVIdx`-equivalent `VertexIdx` pair from the
    /// chain element at Step 7.
    pub constructed_directed_edge: Vec<Option<(VertexIdx, VertexIdx)>>,
}

impl TopoArena {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_vertex(&mut self, position: [f64; 3]) -> VertexIdx {
        let idx = VertexIdx(self.vertices.len());
        self.vertices.push(Vertex {
            position,
            half_edge: None,
        });
        idx
    }

    pub fn add_edge(&mut self) -> (EdgeIdx, HalfEdgeIdx, HalfEdgeIdx) {
        let he_a_idx = HalfEdgeIdx(self.half_edges.len());
        let he_b_idx = HalfEdgeIdx(self.half_edges.len() + 1);
        let edge_idx = EdgeIdx(self.edges.len());

        // Placeholder half-edges — caller must set origin, next, prev, loop_
        let placeholder_vertex = VertexIdx(0);
        let placeholder_loop = LoopIdx(0);

        self.half_edges.push(HalfEdge {
            origin: placeholder_vertex,
            edge: edge_idx,
            twin: Some(he_b_idx),
            next: he_a_idx, // self-loop placeholder
            prev: he_a_idx,
            loop_: placeholder_loop,
        });
        self.half_edges.push(HalfEdge {
            origin: placeholder_vertex,
            edge: edge_idx,
            twin: Some(he_a_idx),
            next: he_b_idx,
            prev: he_b_idx,
            loop_: placeholder_loop,
        });
        self.edges.push(Edge {
            half_edge: he_a_idx,
        });

        (edge_idx, he_a_idx, he_b_idx)
    }

    pub fn add_loop(&mut self, face: FaceIdx) -> LoopIdx {
        let idx = LoopIdx(self.loops.len());
        self.loops.push(Loop {
            half_edge: HalfEdgeIdx(0), // placeholder
            face,
        });
        idx
    }

    pub fn add_face(&mut self, shell: ShellIdx) -> FaceIdx {
        let idx = FaceIdx(self.faces.len());
        self.faces.push(Face {
            outer_loop: LoopIdx(0), // placeholder
            inner_loops: vec![],
            shell,
        });
        idx
    }

    pub fn add_shell(&mut self, solid: SolidIdx) -> ShellIdx {
        let idx = ShellIdx(self.shells.len());
        self.shells.push(Shell {
            face: FaceIdx(0), // placeholder
            solid,
        });
        idx
    }

    pub fn add_solid(&mut self) -> SolidIdx {
        let idx = SolidIdx(self.solids.len());
        self.solids.push(Solid {
            outer_shell: ShellIdx(0), // placeholder
            inner_shells: vec![],
        });
        idx
    }

    // ── Counts ────────────────────────────────────────────────────────

    pub fn vertex_count(&self) -> usize {
        self.vertices.len()
    }

    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    pub fn face_count(&self) -> usize {
        self.faces.len()
    }

    pub fn shell_count(&self) -> usize {
        self.shells.len()
    }
}
