//! Deterministic octree over axis-aligned bounding boxes — the broad phase
//! shared by the BL2 ray-cast candidate producer ([`crate::labeling::TriOctree`])
//! and the improper-contact census ([`crate::inputcheck::detect_improper_contacts`]).
//!
//! Ported semantics from Cherchi et al. 2020 / 2022 (MIT).
//! © Gianmarco Cherchi et al.
//! https://github.com/gcherchi/FastAndRobustMeshArrangements
//! https://github.com/gcherchi/InteractiveAndRobustMeshBooleans
//! The C++ boolean pipeline builds a `cinolib::Octree` over triangle AABBs
//! and walks it with `code/booleans.cpp::intersects_box` (booleans.cpp:580).
//! This is the cinolib-octree-EQUIVALENT (an octree of AABBs whose box
//! query returns every stored item whose AABB touches the query box), not
//! cinolib's source.
//!
//! ## Design invariant (correctness is parameter-independent)
//!
//! Every consumer applies an EXACT per-candidate test after the query (the
//! BL2 prune's `in_ray_aabb`, the census's `classify_pair`), so the tree
//! only has to return a SUPERSET of `{i : item_i ∩ query ≠ ∅}`. Its
//! parameters (`MAX_DEPTH`, `LEAF_SPLIT_THRESHOLD`, the duplication budget)
//! can change which extra candidates are offered, never which survive the
//! exact test, so they cannot change any consumer's output. The superset
//! oracle below pins exactly this contract.
//!
//! Deterministic construction: fixed depth and split threshold, children in
//! a fixed octant order, item ids stored ascending, query results sorted and
//! deduped. All overlap tests are INCLUSIVE (`lo <= hi' && hi >= lo'`), so
//! touching boxes are candidates (exact contact matters to both consumers)
//! and degenerate (zero-thickness) query boxes work.

/// Construction parameters (deterministic; correctness-neutral per the
/// module-level design invariant).
const MAX_DEPTH: u32 = 8;
const LEAF_SPLIT_THRESHOLD: usize = 16;
/// Most a single split may inflate the stored item count (summed over the
/// eight children, relative to the parent). Bounds total stored ids by
/// `MAX_SPLIT_DUPLICATION^MAX_DEPTH · n` rather than `8^MAX_DEPTH · n`. See
/// the duplication-budget comment in [`AabbTree::split`].
const MAX_SPLIT_DUPLICATION: usize = 2;

/// Inclusive axis-aligned bounding box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb {
    pub lo: [f64; 3],
    pub hi: [f64; 3],
}

impl Aabb {
    /// The box of a point set (`None` for an empty set).
    pub fn of_points<'a>(pts: impl IntoIterator<Item = &'a [f64; 3]>) -> Option<Aabb> {
        let mut out: Option<Aabb> = None;
        for p in pts {
            match &mut out {
                None => {
                    out = Some(Aabb { lo: *p, hi: *p });
                }
                Some(b) => {
                    for ((lo, hi), &c) in b.lo.iter_mut().zip(b.hi.iter_mut()).zip(p.iter()) {
                        *lo = lo.min(c);
                        *hi = hi.max(c);
                    }
                }
            }
        }
        out
    }

    /// Inclusive overlap on all three axes.
    pub fn intersects(&self, other: &Aabb) -> bool {
        (0..3).all(|k| self.lo[k] <= other.hi[k] && self.hi[k] >= other.lo[k])
    }
}

#[derive(Debug)]
struct Node {
    bbox: Aabb,
    /// `Some` for inner nodes (eight children in fixed octant order:
    /// bit 0 = +x half, bit 1 = +y half, bit 2 = +z half).
    children: Option<[u32; 8]>,
    /// Leaf payload: item ids, ascending.
    item_ids: Vec<u32>,
}

/// Octree over a list of AABBs, indexed by item id (the input position).
#[derive(Debug)]
pub struct AabbTree {
    /// `nodes[0]` is the root when non-empty.
    nodes: Vec<Node>,
    items: Vec<Aabb>,
}

impl AabbTree {
    /// Build over `items`. An empty list yields an empty tree whose queries
    /// return nothing.
    pub fn build(items: Vec<Aabb>) -> AabbTree {
        if items.is_empty() {
            return AabbTree {
                nodes: Vec::new(),
                items,
            };
        }
        let mut root_box = items[0];
        for it in &items[1..] {
            for k in 0..3 {
                root_box.lo[k] = root_box.lo[k].min(it.lo[k]);
                root_box.hi[k] = root_box.hi[k].max(it.hi[k]);
            }
        }
        let mut tree = AabbTree {
            nodes: vec![Node {
                bbox: root_box,
                children: None,
                item_ids: (0..items.len() as u32).collect(), // ascending
            }],
            items,
        };
        tree.split(0, 0);
        tree
    }

    /// The stored items, by id.
    pub fn items(&self) -> &[Aabb] {
        &self.items
    }

    /// Recursively split node `node_idx` (depth `depth`) into eight
    /// octants. A node stays a leaf when it is small enough, deep enough,
    /// or splitting makes no progress (every child would inherit every
    /// item — coincident AABBs).
    fn split(&mut self, node_idx: usize, depth: u32) {
        let n_items = self.nodes[node_idx].item_ids.len();
        if depth >= MAX_DEPTH || n_items <= LEAF_SPLIT_THRESHOLD {
            return;
        }
        let bbox = self.nodes[node_idx].bbox;
        let mid = [
            (bbox.lo[0] + bbox.hi[0]) / 2.0,
            (bbox.lo[1] + bbox.hi[1]) / 2.0,
            (bbox.lo[2] + bbox.hi[2]) / 2.0,
        ];
        let mut child_boxes = [bbox; 8];
        let mut child_items: [Vec<u32>; 8] = Default::default();
        for (oct, cb) in child_boxes.iter_mut().enumerate() {
            for (k, &m) in mid.iter().enumerate() {
                if oct >> k & 1 == 0 {
                    cb.hi[k] = m;
                } else {
                    cb.lo[k] = m;
                }
            }
        }
        for &id in &self.nodes[node_idx].item_ids {
            let item = &self.items[id as usize];
            for (oct, cb) in child_boxes.iter().enumerate() {
                if cb.intersects(item) {
                    child_items[oct].push(id); // ascending (source order)
                }
            }
        }
        // No-progress guard: all items land in all eight children.
        if child_items.iter().all(|c| c.len() == n_items) {
            return;
        }
        // DUPLICATION BUDGET. An item straddling the midplanes lands in
        // several children; a node full of large boxes duplicates ~6× per
        // level and compounds to 6^8 over the depth (measured on assay R0088
        // before the budget: 7.8 M nodes holding 1.1 G ids for a 7.5 k-tri
        // input — the whole of a 6 GiB peak and a wasm32 crash). A split is
        // worth making only if it divides more than it copies: a node may
        // hand its children at most `MAX_SPLIT_DUPLICATION ×` its own item
        // count in total. Correctness-neutral by construction — bailing
        // makes the node a leaf, and a leaf returns its whole list, so the
        // query stays a superset.
        let total_child_items: usize = child_items.iter().map(Vec::len).sum();
        if total_child_items > MAX_SPLIT_DUPLICATION * n_items {
            return;
        }
        let mut children = [0u32; 8];
        for (oct, ids) in child_items.into_iter().enumerate() {
            children[oct] = self.nodes.len() as u32;
            self.nodes.push(Node {
                bbox: child_boxes[oct],
                children: None,
                item_ids: ids,
            });
        }
        self.nodes[node_idx].children = Some(children);
        self.nodes[node_idx].item_ids = Vec::new();
        for &c in &children {
            self.split(c as usize, depth + 1);
        }
    }

    /// Port of `intersects_box` (booleans.cpp:580): stack walk collecting
    /// every stored item whose AABB intersects the query box (inclusive).
    /// Ids SORTED ascending and deduped (an item straddling octant planes is
    /// stored in several leaves).
    pub fn query(&self, q: &Aabb) -> Vec<u32> {
        let mut out: Vec<u32> = Vec::new();
        if self.nodes.is_empty() || !self.nodes[0].bbox.intersects(q) {
            return out;
        }
        let mut stack: Vec<u32> = vec![0];
        while let Some(ni) = stack.pop() {
            let node = &self.nodes[ni as usize];
            match &node.children {
                Some(children) => {
                    for &c in children {
                        if self.nodes[c as usize].bbox.intersects(q) {
                            stack.push(c);
                        }
                    }
                }
                None => {
                    for &id in &node.item_ids {
                        if self.items[id as usize].intersects(q) {
                            out.push(id);
                        }
                    }
                }
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    /// Every unordered pair `(i, j)`, `i < j`, of stored items whose boxes
    /// overlap (inclusive) — the broad phase of a pairwise census. Sorted
    /// ascending lexicographically, each pair once; identical to the brute
    /// O(n²) box test by construction (the query is exact on boxes once the
    /// superset is filtered by `intersects`, which it is).
    pub fn overlapping_pairs(&self) -> Vec<(u32, u32)> {
        let mut out: Vec<(u32, u32)> = Vec::new();
        for (i, item) in self.items.iter().enumerate() {
            let i = i as u32;
            for j in self.query(item) {
                if j > i {
                    out.push((i, j));
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic LCG boxes in the unit cube, extent `scale`.
    fn boxes(n: usize, seed: u64, scale: f64) -> Vec<Aabb> {
        let mut s = seed;
        let mut next = || {
            s = s
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((s >> 11) as f64) / ((1u64 << 53) as f64)
        };
        (0..n)
            .map(|_| {
                let lo = [next(), next(), next()];
                let ext = [next() * scale, next() * scale, next() * scale];
                Aabb {
                    lo,
                    hi: [lo[0] + ext[0], lo[1] + ext[1], lo[2] + ext[2]],
                }
            })
            .collect()
    }

    fn brute_pairs(items: &[Aabb]) -> Vec<(u32, u32)> {
        let mut out = Vec::new();
        for i in 0..items.len() {
            for j in (i + 1)..items.len() {
                if items[i].intersects(&items[j]) {
                    out.push((i as u32, j as u32));
                }
            }
        }
        out
    }

    #[test]
    fn query_is_exact_on_boxes_and_sorted() {
        let items = boxes(2000, 7, 0.05);
        let tree = AabbTree::build(items.clone());
        for q in boxes(50, 99, 0.2) {
            let got = tree.query(&q);
            let want: Vec<u32> = (0..items.len() as u32)
                .filter(|&i| items[i as usize].intersects(&q))
                .collect();
            assert_eq!(got, want);
        }
    }

    #[test]
    fn overlapping_pairs_equal_brute_force_at_three_densities() {
        for (n, scale, seed) in [(500usize, 0.01f64, 1u64), (1500, 0.08, 2), (300, 0.6, 3)] {
            let items = boxes(n, seed, scale);
            let tree = AabbTree::build(items.clone());
            assert_eq!(
                tree.overlapping_pairs(),
                brute_pairs(&items),
                "n={n} scale={scale}"
            );
        }
    }

    #[test]
    fn touching_boxes_are_candidates_and_degenerate_queries_work() {
        let items = vec![
            Aabb {
                lo: [0.0, 0.0, 0.0],
                hi: [1.0, 1.0, 1.0],
            },
            Aabb {
                lo: [1.0, 0.0, 0.0],
                hi: [2.0, 1.0, 1.0],
            },
            Aabb {
                lo: [3.0, 0.0, 0.0],
                hi: [4.0, 1.0, 1.0],
            },
        ];
        let tree = AabbTree::build(items);
        assert_eq!(tree.overlapping_pairs(), vec![(0, 1)]);
        let seg = Aabb {
            lo: [0.5, 0.5, -5.0],
            hi: [0.5, 0.5, 5.0],
        };
        assert_eq!(tree.query(&seg), vec![0]);
    }

    #[test]
    fn empty_and_coincident_inputs_are_fine() {
        assert!(AabbTree::build(Vec::new())
            .query(&Aabb {
                lo: [0.0; 3],
                hi: [1.0; 3]
            })
            .is_empty());
        let same = Aabb {
            lo: [0.0; 3],
            hi: [1.0; 3],
        };
        let tree = AabbTree::build(vec![same; 100]);
        assert_eq!(tree.query(&same).len(), 100);
        assert_eq!(tree.overlapping_pairs().len(), 100 * 99 / 2);
    }

    #[test]
    fn two_builds_answer_identically() {
        let items = boxes(1000, 42, 0.1);
        let a = AabbTree::build(items.clone());
        let b = AabbTree::build(items);
        assert_eq!(a.overlapping_pairs(), b.overlapping_pairs());
    }
}
