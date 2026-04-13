//! Triangle refinement tree for the Cherchi mesh arrangement.
//!
//! Ported from Cherchi tree.h
//! MIT License (c) 2020 Cherchi, Livesu, Scateni, Attene

/// A node in the triangle refinement tree.
/// Stores the three vertex IDs of a triangle and up to 3 children.
/// Ported from tree.h:46-58
#[derive(Debug, Clone)]
pub(crate) struct Node {
    pub v: [usize; 3],
    pub children: [Option<usize>; 3],
}

impl Node {
    pub fn new(v0: usize, v1: usize, v2: usize) -> Self {
        Self {
            v: [v0, v1, v2],
            children: [None, None, None],
        }
    }
}

/// Triangle refinement tree. Each node records a triangle and its
/// children after splits.
/// Ported from tree.h:62-108
#[derive(Debug, Clone)]
pub(crate) struct Tree {
    nodes: Vec<Node>,
}

impl Tree {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self { nodes: Vec::new() }
    }

    #[allow(dead_code)]
    pub fn with_capacity(size: usize) -> Self {
        Self {
            nodes: Vec::with_capacity(size),
        }
    }

    /// Add a new leaf node and return its index.
    /// Ported from tree.h:73-77
    pub fn add_node(&mut self, v0: usize, v1: usize, v2: usize) -> usize {
        let id = self.nodes.len();
        self.nodes.push(Node::new(v0, v1, v2));
        id
    }

    /// Retrieve a node by ID.
    /// Ported from tree.h:79-83
    #[allow(dead_code)]
    pub fn get_node(&self, node_id: usize) -> &Node {
        assert!(node_id < self.nodes.len(), "out of range node id");
        &self.nodes[node_id]
    }

    /// Assign 2 children to a node (edge split produces 2).
    /// Ported from tree.h:85-92
    pub fn add_children_2(&mut self, node_id: usize, c0: usize, c1: usize) {
        assert!(node_id < self.nodes.len(), "out of range node id");
        assert!(
            self.nodes[node_id].children[0].is_none(),
            "assigning non-empty children list"
        );
        self.nodes[node_id].children[0] = Some(c0);
        self.nodes[node_id].children[1] = Some(c1);
    }

    /// Assign 3 children to a node (triangle split produces 3).
    /// Ported from tree.h:94-102
    pub fn add_children_3(&mut self, node_id: usize, c0: usize, c1: usize, c2: usize) {
        assert!(node_id < self.nodes.len(), "out of range node id");
        assert!(
            self.nodes[node_id].children[0].is_none(),
            "assigning non-empty children list"
        );
        self.nodes[node_id].children[0] = Some(c0);
        self.nodes[node_id].children[1] = Some(c1);
        self.nodes[node_id].children[2] = Some(c2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tree_add_and_get() {
        let mut tree = Tree::new();
        let n0 = tree.add_node(0, 1, 2);
        let n1 = tree.add_node(3, 4, 5);
        assert_eq!(n0, 0);
        assert_eq!(n1, 1);
        assert_eq!(tree.get_node(n0).v, [0, 1, 2]);
        assert_eq!(tree.get_node(n1).v, [3, 4, 5]);
    }

    #[test]
    fn test_tree_children() {
        let mut tree = Tree::new();
        let root = tree.add_node(0, 1, 2);
        let c0 = tree.add_node(0, 1, 3);
        let c1 = tree.add_node(1, 2, 3);
        let c2 = tree.add_node(2, 0, 3);
        tree.add_children_3(root, c0, c1, c2);
        let node = tree.get_node(root);
        assert_eq!(node.children, [Some(c0), Some(c1), Some(c2)]);
    }
}
