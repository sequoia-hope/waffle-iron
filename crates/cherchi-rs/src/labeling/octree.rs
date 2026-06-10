//! Octree candidate-set producer for the BL2 ray-cast (PR-CR-BL2 Cycle C).
//!
//! Ported from Cherchi et al. 2020 / 2022 (MIT).
//! © Gianmarco Cherchi et al.
//! https://github.com/gcherchi/FastAndRobustMeshArrangements
//! https://github.com/gcherchi/InteractiveAndRobustMeshBooleans
//! Source: the C++ boolean pipeline builds a `cinolib::Octree` over the
//! prepped input triangles and queries it per-ray with the custom stack
//! walk `code/booleans.cpp::intersects_box` (booleans.cpp:580), feeding
//! `computeInsideOut` (booleans.cpp:621). This file ports the
//! cinolib-octree-equivalent SEMANTICS (an octree of triangle AABBs whose
//! box query returns every stored item whose AABB touches the query box),
//! not cinolib's source.
//!
//! NOTE: upstream also ships `code/foctree.h` ("fast octree"), but it is
//! NOT used by booleans.cpp — the boolean pipeline queries the plain
//! `cinolib::Octree` via the custom `intersects_box` walk above. We do not
//! port foctree.h.
//!
//! ## Design invariant (correctness is parameter-independent)
//!
//! The BL2 prune (`inside_out.rs::prune_intersections_and_sort_along_ray`)
//! applies an EXACT per-triangle ray-AABB filter (`in_ray_aabb`) to every
//! candidate it is offered — that filter is the semantically load-bearing
//! piece (it excludes behind-origin triangles; see the Cycle A notes in
//! `inside_out.rs`). The octree therefore only has to produce a SUPERSET
//! of `{t : tri_AABB ∩ query_box ≠ ∅}` for any query box: its internal
//! parameters (max depth, leaf split threshold) can change which extra
//! candidates are offered but can never change which candidates survive
//! the exact filter, so they cannot affect labeling output. The superset
//! oracle below pins exactly this contract.
//!
//! Port deviations from cinolib:
//! - Fixed deterministic construction: `MAX_DEPTH = 8`, leaf split
//!   threshold 16 items, children visited in a fixed octant order, item
//!   ids stored ascending. cinolib's defaults differ; by the invariant
//!   above the choice is correctness-neutral.
//! - No-progress guard: a node whose split would place ALL its items in
//!   ALL eight children (coincident AABBs) stays a leaf instead of
//!   recursing to `MAX_DEPTH`. Superset-trivial, avoids pathological
//!   node blowup.
//! - All AABB overlap tests are INCLUSIVE (`lo <= hi' && hi >= lo'`),
//!   matching the prune's `in_ray_aabb`, so degenerate (zero-thickness)
//!   query boxes — the actual production shape: an axis-aligned ray's
//!   AABB is a line segment — work.

use crate::arrangements::fast_trimesh::VertexCoords;
use crate::arrangements::soup::ArrangementSoup;
use cad_primitives::Point3;

/// Construction parameters (deterministic; correctness-neutral per the
/// module-level design invariant).
const MAX_DEPTH: u32 = 8;
const LEAF_SPLIT_THRESHOLD: usize = 16;

/// Inclusive axis-aligned bounding box.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Aabb {
    lo: [f64; 3],
    hi: [f64; 3],
}

impl Aabb {
    /// Inclusive overlap on all three axes (same semantics as the prune's
    /// exact `in_ray_aabb` filter).
    fn intersects(&self, other: &Aabb) -> bool {
        (0..3).all(|k| self.lo[k] <= other.hi[k] && self.hi[k] >= other.lo[k])
    }
}

#[derive(Debug)]
struct OctreeNode {
    bbox: Aabb,
    /// `Some` for inner nodes (eight children in fixed octant order:
    /// bit 0 = +x half, bit 1 = +y half, bit 2 = +z half).
    children: Option<[u32; 8]>,
    /// Leaf payload: item ids, ascending.
    item_ids: Vec<u32>,
}

/// Octree over the AABBs of the soup's prepped input triangles
/// (`soup.in_tris`, explicit soup-scaled coordinates).
#[derive(Debug)]
pub struct TriOctree {
    /// `nodes[0]` is the root when non-empty.
    nodes: Vec<OctreeNode>,
    /// Per-`in_tris` AABB, indexed by triangle id.
    items: Vec<Aabb>,
}

impl TriOctree {
    /// Build the octree over `soup.in_tris`. Empty `in_tris` yields an
    /// empty octree whose queries return nothing. Deterministic: fixed
    /// `MAX_DEPTH` / `LEAF_SPLIT_THRESHOLD`, children created in fixed
    /// octant order, item ids stored ascending.
    pub fn build(soup: &ArrangementSoup) -> TriOctree {
        let items: Vec<Aabb> = (0..soup.in_tris.len() as u32)
            .map(|t| tri_aabb(soup, t))
            .collect();
        if items.is_empty() {
            return TriOctree {
                nodes: Vec::new(),
                items,
            };
        }

        // Root box = global AABB of all items.
        let mut root_box = items[0];
        for it in &items[1..] {
            for k in 0..3 {
                root_box.lo[k] = root_box.lo[k].min(it.lo[k]);
                root_box.hi[k] = root_box.hi[k].max(it.hi[k]);
            }
        }

        let mut tree = TriOctree {
            nodes: vec![OctreeNode {
                bbox: root_box,
                children: None,
                item_ids: (0..items.len() as u32).collect(), // ascending
            }],
            items,
        };
        tree.split(0, 0);
        tree
    }

    /// Recursively split node `node_idx` (depth `depth`) into eight
    /// octants. A node stays a leaf when it is small enough, deep enough,
    /// or splitting makes no progress (every child would inherit every
    /// item — coincident AABBs; deviation documented in the module docs).
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

        // Fixed octant order: bit 0 = +x half, bit 1 = +y half, bit 2 = +z.
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

        let mut children = [0u32; 8];
        for (oct, ids) in child_items.into_iter().enumerate() {
            children[oct] = self.nodes.len() as u32;
            self.nodes.push(OctreeNode {
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
    /// Returns ids SORTED ascending and deduped (determinism, and the
    /// prune's candidate-visit-order contract; an item straddling octant
    /// planes is stored in several leaves, hence the dedup — matching the
    /// C++ `flat_hash_set` accumulator).
    pub fn query_aabb(&self, lo: [f64; 3], hi: [f64; 3]) -> Vec<u32> {
        let q = Aabb { lo, hi };
        let mut out: Vec<u32> = Vec::new();
        if self.nodes.is_empty() || !self.nodes[0].bbox.intersects(&q) {
            return out;
        }
        let mut stack: Vec<u32> = vec![0];
        while let Some(ni) = stack.pop() {
            let node = &self.nodes[ni as usize];
            match &node.children {
                Some(children) => {
                    for &c in children {
                        if self.nodes[c as usize].bbox.intersects(&q) {
                            stack.push(c);
                        }
                    }
                }
                None => {
                    for &id in &node.item_ids {
                        if self.items[id as usize].intersects(&q) {
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
}

/// AABB of one prepped input triangle (explicit verts only — `in_tris`
/// vertices are always explicit; see `inside_out.rs`).
fn tri_aabb(soup: &ArrangementSoup, t: u32) -> Aabb {
    let tri = soup.in_tris[t as usize];
    let p = |v: u32| -> Point3 {
        match &soup.verts[v as usize] {
            VertexCoords::Explicit(p) => *p,
            other => unreachable!("in_tris vertex is implicit: {other:?}"),
        }
    };
    let pts = [p(tri[0]), p(tri[1]), p(tri[2])];
    let c = |q: &Point3, k: usize| match k {
        0 => q.x(),
        1 => q.y(),
        _ => q.z(),
    };
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for q in &pts {
        for k in 0..3 {
            lo[k] = lo[k].min(c(q, k));
            hi[k] = hi[k].max(c(q, k));
        }
    }
    Aabb { lo, hi }
}

// =========================================================================
// RED oracle tests (PR-CR-BL2 Cycle C)
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arrangements::soup::{mesh_arrangement, Label};
    use crate::labeled_arrangement::InputId;

    const A: InputId = InputId(0);
    const B: InputId = InputId(1);

    // ----- fixtures (local copies, per the labeling test convention) ------

    type Solid = (Vec<f64>, Vec<[u32; 3]>, Vec<Label>);

    fn boxx(ox: f64, oy: f64, oz: f64, sx: f64, sy: f64, sz: f64, label: InputId) -> Solid {
        let p = |x: f64, y: f64, z: f64| (ox + x * sx, oy + y * sy, oz + z * sz);
        let corners = [
            p(0.0, 0.0, 0.0),
            p(1.0, 0.0, 0.0),
            p(1.0, 1.0, 0.0),
            p(0.0, 1.0, 0.0),
            p(0.0, 0.0, 1.0),
            p(1.0, 0.0, 1.0),
            p(1.0, 1.0, 1.0),
            p(0.0, 1.0, 1.0),
        ];
        let mut coords = Vec::with_capacity(24);
        for (x, y, z) in corners {
            coords.push(x);
            coords.push(y);
            coords.push(z);
        }
        let tris = vec![
            [0, 2, 1],
            [0, 3, 2],
            [4, 5, 6],
            [4, 6, 7],
            [0, 1, 5],
            [0, 5, 4],
            [2, 3, 7],
            [2, 7, 6],
            [1, 2, 6],
            [1, 6, 5],
            [3, 0, 4],
            [3, 4, 7],
        ];
        let labels = vec![vec![label]; tris.len()];
        (coords, tris, labels)
    }

    fn cube(ox: f64, oy: f64, oz: f64, s: f64, label: InputId) -> Solid {
        boxx(ox, oy, oz, s, s, s, label)
    }

    /// Cube rotated 45° about z (off-grid coordinates).
    fn rotated_cube(s: f64, label: InputId) -> Solid {
        let (coords, tris, labels) = cube(0.0, 0.0, 0.0, s, label);
        let (c, sn) = (
            std::f64::consts::FRAC_1_SQRT_2,
            std::f64::consts::FRAC_1_SQRT_2,
        );
        let coords = coords
            .chunks(3)
            .flat_map(|p| [p[0] * c - p[1] * sn, p[0] * sn + p[1] * c, p[2]])
            .collect();
        (coords, tris, labels)
    }

    fn concat(s0: Solid, s1: Solid) -> Solid {
        let (mut coords, mut tris, mut labels) = s0;
        let off = (coords.len() / 3) as u32;
        coords.extend_from_slice(&s1.0);
        for t in s1.1 {
            tris.push([t[0] + off, t[1] + off, t[2] + off]);
        }
        labels.extend(s1.2);
        (coords, tris, labels)
    }

    fn arrange(s0: Solid, s1: Solid) -> ArrangementSoup {
        let (coords, tris, labels) = concat(s0, s1);
        mesh_arrangement(&coords, &tris, &labels).expect("arrangement")
    }

    fn fixture_soups() -> Vec<(&'static str, ArrangementSoup)> {
        vec![
            (
                "corner-overlap cubes",
                arrange(cube(0.0, 0.0, 0.0, 2.0, A), cube(1.0, 1.0, 1.0, 2.0, B)),
            ),
            (
                "through-cut peg",
                arrange(
                    cube(0.0, 0.0, 0.0, 2.0, A),
                    boxx(0.5, 0.5, -1.0, 1.0, 1.0, 4.0, B),
                ),
            ),
            (
                "45°-rotated cube vs cube",
                arrange(rotated_cube(2.0, A), cube(0.0, 0.5, 0.5, 1.0, B)),
            ),
        ]
    }

    // ----- independent brute reference (oracle-side, no TriOctree code) ---

    fn brute_tri_aabbs(soup: &ArrangementSoup) -> Vec<([f64; 3], [f64; 3])> {
        let p = |v: u32| -> [f64; 3] {
            match &soup.verts[v as usize] {
                VertexCoords::Explicit(p) => [p.x(), p.y(), p.z()],
                other => panic!("in_tris vertex is implicit: {other:?}"),
            }
        };
        soup.in_tris
            .iter()
            .map(|tri| {
                let pts = [p(tri[0]), p(tri[1]), p(tri[2])];
                let mut lo = [f64::INFINITY; 3];
                let mut hi = [f64::NEG_INFINITY; 3];
                for q in &pts {
                    for k in 0..3 {
                        lo[k] = lo[k].min(q[k]);
                        hi[k] = hi[k].max(q[k]);
                    }
                }
                (lo, hi)
            })
            .collect()
    }

    /// Brute `{t : tri_AABB ∩ box ≠ ∅}` with inclusive bounds.
    fn brute_query(soup: &ArrangementSoup, lo: [f64; 3], hi: [f64; 3]) -> Vec<u32> {
        brute_tri_aabbs(soup)
            .iter()
            .enumerate()
            .filter(|(_, (tlo, thi))| (0..3).all(|k| tlo[k] <= hi[k] && thi[k] >= lo[k]))
            .map(|(t, _)| t as u32)
            .collect()
    }

    /// Global AABB of the soup's input triangles, plus a margin.
    fn global_aabb(soup: &ArrangementSoup) -> ([f64; 3], [f64; 3]) {
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for (tlo, thi) in brute_tri_aabbs(soup) {
            for k in 0..3 {
                lo[k] = lo[k].min(tlo[k]);
                hi[k] = hi[k].max(thi[k]);
            }
        }
        (lo, hi)
    }

    /// Deterministic query boxes for one soup: degenerate line-like boxes
    /// (the production shape — an axis-aligned ray's AABB is degenerate in
    /// the two off-axis coordinates), a point box, a thin slab, the global
    /// box, and a box past `max_coords` that misses everything.
    fn query_boxes(soup: &ArrangementSoup) -> Vec<([f64; 3], [f64; 3])> {
        let (lo, hi) = global_aabb(soup);
        let mid = [
            (lo[0] + hi[0]) / 2.0,
            (lo[1] + hi[1]) / 2.0,
            (lo[2] + hi[2]) / 2.0,
        ];
        let max_c = [hi[0] + 0.5, hi[1] + 0.5, hi[2] + 0.5];
        vec![
            // X-ray AABB from the middle: degenerate in y and z.
            ([mid[0], mid[1], mid[2]], [max_c[0], mid[1], mid[2]]),
            // Y-ray and Z-ray analogs.
            ([mid[0], mid[1], mid[2]], [mid[0], max_c[1], mid[2]]),
            ([mid[0], mid[1], mid[2]], [mid[0], mid[1], max_c[2]]),
            // X-ray grazing the lower corner (on-boundary inclusivity).
            ([lo[0], lo[1], lo[2]], [max_c[0], lo[1], lo[2]]),
            // Point box at a vertex of the global AABB.
            ([lo[0], lo[1], lo[2]], [lo[0], lo[1], lo[2]]),
            // Thin z-slab through the middle.
            ([lo[0], lo[1], mid[2]], [hi[0], hi[1], mid[2]]),
            // The whole global box.
            (lo, hi),
            // Past max_coords: misses everything.
            (
                [max_c[0] + 1.0, max_c[1] + 1.0, max_c[2] + 1.0],
                [max_c[0] + 2.0, max_c[1] + 2.0, max_c[2] + 2.0],
            ),
        ]
    }

    fn is_sorted_dedup(v: &[u32]) -> bool {
        v.windows(2).all(|w| w[0] < w[1])
    }

    // ════════════════════════════════════════════════════════════════
    // Oracle #1 — superset: on every fixture and every query box (incl.
    // the degenerate zero-thickness ray boxes), the octree result
    // contains every triangle whose AABB touches the box. This is THE
    // contract that makes octree parameters correctness-neutral.
    // ════════════════════════════════════════════════════════════════
    #[test]
    fn query_is_superset_of_brute() {
        for (name, soup) in fixture_soups() {
            let octree = TriOctree::build(&soup);
            for (lo, hi) in query_boxes(&soup) {
                let got = octree.query_aabb(lo, hi);
                let want = brute_query(&soup, lo, hi);
                assert!(
                    is_sorted_dedup(&got),
                    "{name}: query result sorted + deduped, got {got:?}"
                );
                let missing: Vec<u32> = want.iter().copied().filter(|t| !got.contains(t)).collect();
                assert!(
                    missing.is_empty(),
                    "{name}: query box {lo:?}..{hi:?} missing brute candidates \
                     {missing:?} (got {got:?}, want ⊇ {want:?})"
                );
            }
        }
    }

    // ════════════════════════════════════════════════════════════════
    // Oracle #2 — completeness at the root: the global box returns
    // EVERY item id exactly once; only stored ids are ever returned.
    // ════════════════════════════════════════════════════════════════
    #[test]
    fn global_box_returns_every_item() {
        for (name, soup) in fixture_soups() {
            let octree = TriOctree::build(&soup);
            let (lo, hi) = global_aabb(&soup);
            let got = octree.query_aabb(lo, hi);
            let all: Vec<u32> = (0..soup.in_tris.len() as u32).collect();
            assert_eq!(
                got,
                all,
                "{name}: global-box query must return all {} input tris",
                all.len()
            );
        }
    }

    // ════════════════════════════════════════════════════════════════
    // Oracle #3 — determinism: two independent builds answer every
    // query box identically.
    // ════════════════════════════════════════════════════════════════
    #[test]
    fn two_builds_answer_identically() {
        for (name, soup) in fixture_soups() {
            let o1 = TriOctree::build(&soup);
            let o2 = TriOctree::build(&soup);
            for (lo, hi) in query_boxes(&soup) {
                assert_eq!(
                    o1.query_aabb(lo, hi),
                    o2.query_aabb(lo, hi),
                    "{name}: builds disagree on box {lo:?}..{hi:?}"
                );
            }
        }
    }

    // ════════════════════════════════════════════════════════════════
    // Oracle #4 — degenerate input: empty in_tris → empty octree,
    // every query returns empty.
    // ════════════════════════════════════════════════════════════════
    #[test]
    fn empty_in_tris_yields_empty_queries() {
        let soup = arrange(cube(0.0, 0.0, 0.0, 1.0, A), cube(5.0, 5.0, 5.0, 1.0, B));
        let empty = ArrangementSoup {
            in_tris: Vec::new(),
            in_labels: Vec::new(),
            ..soup
        };
        let octree = TriOctree::build(&empty);
        assert!(octree.query_aabb([0.0; 3], [10.0; 3]).is_empty());
        assert!(octree.query_aabb([0.0; 3], [0.0; 3]).is_empty());
    }

    // ════════════════════════════════════════════════════════════════
    // Oracle #5 — the miss box (past max_coords) returns nothing and
    // returned ids are always valid item ids.
    // ════════════════════════════════════════════════════════════════
    #[test]
    fn miss_box_returns_nothing_and_ids_are_valid() {
        for (name, soup) in fixture_soups() {
            let octree = TriOctree::build(&soup);
            let (_, hi) = global_aabb(&soup);
            let miss = octree.query_aabb(
                [hi[0] + 1.5, hi[1] + 1.5, hi[2] + 1.5],
                [hi[0] + 2.5, hi[1] + 2.5, hi[2] + 2.5],
            );
            assert!(miss.is_empty(), "{name}: miss box returned {miss:?}");
            for (lo, hi) in query_boxes(&soup) {
                for t in octree.query_aabb(lo, hi) {
                    assert!(
                        (t as usize) < soup.in_tris.len(),
                        "{name}: returned id {t} out of range"
                    );
                }
            }
        }
    }
}
