/// A node in the symbolic-split refinement tree.
///
/// Stores three triangle vertices (the triangle's geometry at the time
/// of creation) and up to three children (set later via
/// [`Tree::add_children`]). 2-child variant = edge-split; 3-child
/// variant = tri-split.
#[derive(Debug, Clone)]
pub struct Node {
    v: [u32; 3],
    children: [Option<u32>; 3],
}

impl Node {
    pub fn verts(&self) -> [u32; 3] {
        self.v
    }

    pub fn children(&self) -> [Option<u32>; 3] {
        self.children
    }
}

/// Append-only refinement tree tracking split provenance.
///
/// Each node represents a triangle that existed at some point in the
/// arrangement. When the triangle is split, the new triangles get new
/// child nodes; the parent's `children` field records the link. The
/// tree forms a DAG of arrangement history.
#[derive(Debug, Default)]
pub struct Tree {
    nodes: Vec<Node>,
}

impl Tree {
    pub fn new() -> Self {
        Self { nodes: Vec::new() }
    }

    pub fn with_capacity(cap: usize) -> Self {
        Self {
            nodes: Vec::with_capacity(cap),
        }
    }

    pub fn num_nodes(&self) -> u32 {
        self.nodes.len() as u32
    }

    /// Append a new node with the given triangle vertices. Children
    /// initialize to `[None, None, None]`. Returns the new node's u32 ID.
    pub fn add_node(&mut self, _v0: u32, _v1: u32, _v2: u32) -> u32 {
        // RED stub
        0
    }

    /// Read-only access to a node by ID.
    pub fn get_node(&self, id: u32) -> &Node {
        debug_assert!(id < self.num_nodes(), "get_node: id {id} out of range");
        &self.nodes[id as usize]
    }

    /// Set the children of `parent`. `children` must have length 2
    /// (edge-split) or 3 (tri-split). Panics in debug if the parent
    /// already has children set.
    pub fn add_children(&mut self, _parent: u32, _children: &[u32]) {
        // RED stub
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------
    // PR-CR12c — Group T: Tree basics
    // -----------------------------------------------------------------

    #[test]
    fn new_tree_is_empty() {
        let t = Tree::new();
        assert_eq!(t.num_nodes(), 0);
    }

    #[test]
    fn with_capacity_is_empty() {
        let t = Tree::with_capacity(16);
        assert_eq!(t.num_nodes(), 0);
    }

    #[test]
    fn add_node_returns_sequential_ids() {
        let mut t = Tree::new();
        assert_eq!(t.add_node(0, 1, 2), 0);
        assert_eq!(t.add_node(3, 4, 5), 1);
        assert_eq!(t.add_node(6, 7, 8), 2);
        assert_eq!(t.num_nodes(), 3);
    }

    #[test]
    fn get_node_returns_input_verts() {
        let mut t = Tree::new();
        let id = t.add_node(10, 20, 30);
        let n = t.get_node(id);
        assert_eq!(n.verts(), [10, 20, 30]);
    }

    #[test]
    fn new_node_has_no_children() {
        let mut t = Tree::new();
        let id = t.add_node(0, 1, 2);
        assert_eq!(t.get_node(id).children(), [None, None, None]);
    }

    #[test]
    fn add_children_two_round_trips() {
        let mut t = Tree::new();
        let p = t.add_node(0, 1, 2);
        let c0 = t.add_node(3, 4, 5);
        let c1 = t.add_node(6, 7, 8);
        t.add_children(p, &[c0, c1]);
        assert_eq!(t.get_node(p).children(), [Some(c0), Some(c1), None]);
    }

    #[test]
    fn add_children_three_round_trips() {
        let mut t = Tree::new();
        let p = t.add_node(0, 1, 2);
        let c0 = t.add_node(3, 4, 5);
        let c1 = t.add_node(6, 7, 8);
        let c2 = t.add_node(9, 10, 11);
        t.add_children(p, &[c0, c1, c2]);
        assert_eq!(t.get_node(p).children(), [Some(c0), Some(c1), Some(c2)]);
    }
}
