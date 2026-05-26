//! `FastTrimesh` — adjacency-aware triangle soup for mesh arrangement.
//!
//! Ported from Cherchi 2022 `arrangements/code/fast_trimesh.{h,cpp}` (MIT).
//! © 2022 G. Cherchi, M. Livesu, R. Scateni, M. Attene, F. Pellacini.
//! See ../../LICENSE-THIRD-PARTY.md for full attribution.
//!
//! Cherchi 2020 §4 (mesh arrangement data structure).
//!
//! ## Scope (cumulative through PR-CR12a)
//!
//! - **PR-CR11**: bulk constructor + every topology / adjacency query
//!   the arrangement read phase needs. Immutable after construction.
//!   See `specs/cherchi_rs_fast_trimesh_mvp.md`.
//! - **PR-CR12a**: addition mutator family (`add_vert`,
//!   `add_vert_with_orig_id`, `add_tri` with rotation-invariant dedup
//!   via `tri_id`), info/flag setters (`set_vert_info`, `set_tri_info`,
//!   `set_edge_constr`, `edge_set_visited`), bulk resetters
//!   (`reset_vertices_info`, `reset_triangles_info`), `rev_vtx_map` +
//!   `vert_orig_id` / `vert_new_id` queries, and derived adjacency
//!   (`adj_t2t`, `adj_v2t`). See
//!   `specs/cherchi_rs_fast_trimesh_mutators.md`.
//!
//! ## Deliberate deviations from upstream
//!
//! 1. **Explicit points only.** Upstream stores `const genericPoint*`
//!    to support implicit (LPI/TPI) points from the LGPL
//!    `Indirect_Predicates` library. cherchi-rs does NOT depend on
//!    LGPL code (paused; see project memory). We store `Point3`
//!    by value — explicit-only. When the LGPL decision resolves,
//!    `Vertex` will gain an implicit-point variant; the topology
//!    layer is unaffected.
//!
//! 2. **No parallel constructor.** cherchi-rs `CLAUDE.md` Hard Rule #5
//!    is single-threaded by default. We use the same sorted-unique
//!    algorithm the upstream parallel path uses (cpp:78-128), minus
//!    TBB. Rayon parallelism is a future opt-in feature flag.
//!
//! 3. **`Point3` stored by value**, not by reference. Upstream uses
//!    pointer for `genericPoint*` polymorphism; `Point3` is `Copy`
//!    (24 B) and we have no polymorphism, so by-value is cleaner
//!    and avoids self-referential lifetimes on `FastTrimesh`.
//!
//! 4. **`edge_id` / `tri_vert_offset` / `tri_id` return
//!    `Option<u32>`**, where upstream returns `int` with `-1` for
//!    missing. We use `Option` for type safety; consumers must
//!    explicitly handle the missing case.
//!
//! 5. **Adjacency is `Vec<Vec<u32>>`**, not upstream's
//!    `absl::InlinedVector<uint, 16>`. Allocator churn on small
//!    adjacencies is a known v1 cost; `smallvec` optimization is
//!    deferred (currently not in workspace deps).
//!
//! 6. **Separate `Vertex.orig_id: Option<u32>` field** (PR-CR12a).
//!    Upstream's `addVert(p, orig_id)` overloads `iVtx.info` to
//!    store `orig_id` with 0 as the "no orig_id" sentinel
//!    (cpp:603-612) — but 0 is a valid input vertex ID, foot-gun.
//!    We use a separate `Option<u32>` field; `info` stays
//!    user-controlled.
//!
//! 7. **`std::collections::HashMap<u32, u32>` for `rev_vtx_map`**
//!    (PR-CR12a). Upstream uses `phmap::flat_hash_map`. Swap to a
//!    faster map later is a one-field-type change; phmap is not
//!    a workspace dep.
//!
//! 8. **Separate method names instead of overloading** (PR-CR12a):
//!    `add_vert` (no orig_id) vs `add_vert_with_orig_id`. Rust has
//!    no method overloading.
//!
//! 9. **`set_edge_constr` matches upstream "set to true only"**
//!    (PR-CR12a, cpp:320-324). No `(e, bool)` form. If clearing is
//!    needed in CR12b/c, add `clear_edge_constr` then.
//!
//! 10. **`edge_set_visited` writes to a separate `visited` field**
//!     (PR-CR11/CR12a). Upstream cpp:371 reuses the `constr` storage
//!     for both `constr` and `visited` flags; cherchi-rs splits them
//!     to remove the foot-gun. PR-CR12a includes regression tests.
//!
//! ## Deferred to PR-CR12b (removal swap-pop)
//!
//! `remove_tri`, `remove_tris`, `remove_edge`, plus the private
//! helpers `remove_tri_unref`, `remove_edge_unref`, `tri_switch`,
//! `edge_switch`, `remove_from_vec`, `edge_contains_vert`. The
//! algorithmically interesting work — index remapping on swap-pop
//! is fragile and deserves isolated review.
//!
//! ## Deferred to PR-CR12c (re-triangulation + Tree + Plane queries)
//!
//! `split_edge` (with/without Tree), `split_tri` (with/without
//! Tree), `flip_tri`, the `Tree` data structure, `tri_node_id` /
//! `set_tri_node_id`, `tri_orientation` (needs CR10 `orient2d` +
//! axis-drop projection), `tri_verts_are_ccw`. Also: parallel
//! constructor (rayon opt-in).

use std::collections::HashMap;

use cad_primitives::Point3;

// =========================================================================
// Public types
// =========================================================================

/// Reference projection plane for the triangles in a `FastTrimesh`.
///
/// Stored from the constructor; consumed by PR-CR12+ 2D-orientation queries
/// (`tri_orientation`, `tri_verts_are_ccw`). PR-CR11 only stores it.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Plane {
    XY,
    YZ,
    ZX,
}

/// Bulk-load error returned by [`FastTrimesh::from_soup`].
///
/// All variants describe caller-supplied data errors. Out-of-range query
/// indices are programmer bugs, not data errors — they trip `debug_assert!`
/// in debug builds.
#[derive(Debug, PartialEq, Eq)]
pub enum FastTrimeshError {
    /// `tris[tri][slot] = vid` but `vid >= verts.len()`.
    VertexIndexOutOfRange {
        tri: u32,
        slot: u8,
        vid: u32,
        n_verts: u32,
    },
    /// `tris[tri]` has two equal vertex indices.
    DegenerateTriangle { tri: u32, vids: [u32; 3] },
    /// `verts.len() > u32::MAX`.
    TooManyVertices { count: usize },
    /// `tris.len() > u32::MAX`.
    TooManyTriangles { count: usize },
}

/// Adjacency-aware triangle soup for mesh arrangement.
///
/// Build via [`FastTrimesh::from_soup`]; query via the methods on this
/// struct. PR-CR12a added the addition mutator family (`add_vert*`,
/// `add_tri`, info/flag setters, resetters) and derived adjacency
/// (`adj_t2t`, `adj_v2t`). Removal (PR-CR12b) and splits (PR-CR12c)
/// are still deferred.
#[derive(Debug)]
pub struct FastTrimesh {
    vertices: Vec<Vertex>,
    edges: Vec<Edge>,
    triangles: Vec<Triangle>,
    /// Vertex → incident edges.
    v2e: Vec<Vec<u32>>,
    /// Edge → incident triangles. May exceed length 2 on non-manifold edges.
    e2t: Vec<Vec<u32>>,
    plane: Plane,
    /// `orig_id → new_v_id` map populated by `add_vert_with_orig_id`.
    /// PR-CR11's `from_soup` initializes empty (no orig-mesh-ID source).
    rev_vtx_map: HashMap<u32, u32>,
}

// =========================================================================
// Internal storage types
// =========================================================================

#[derive(Copy, Clone, Debug)]
pub(crate) struct Vertex {
    point: Point3,
    info: u32,
    /// Original mesh vertex ID, populated by `add_vert_with_orig_id`.
    /// `None` for vertices added via `add_vert` (no orig ID source) or
    /// the `from_soup` bulk constructor.
    orig_id: Option<u32>,
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct Edge {
    v0: u32,
    v1: u32,
    constr: bool,
    visited: bool,
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct Triangle {
    v: [u32; 3],
    info: u32,
}

// =========================================================================
// Implementation
// =========================================================================

impl FastTrimesh {
    /// Build a `FastTrimesh` from raw vertex + triangle arrays.
    ///
    /// Validates input (vertex count, triangle count, per-triangle index
    /// range, no repeated indices within a triangle), derives the
    /// sorted-unique edge list, and builds V→E + E→T adjacency.
    ///
    /// Algorithm mirrors upstream `fast_trimesh.cpp:78-128` (parallel
    /// branch, sequential here): collect all `3 * T` edges as
    /// `(min(v0,v1), max(v0,v1))`; sort + unique; assign IDs by index;
    /// populate `v2e` and `e2t` by binary searching for each triangle's
    /// edges.
    pub fn from_soup(
        verts: &[Point3],
        tris: &[[u32; 3]],
        plane: Plane,
    ) -> Result<Self, FastTrimeshError> {
        let n_verts: u32 = verts
            .len()
            .try_into()
            .map_err(|_| FastTrimeshError::TooManyVertices { count: verts.len() })?;
        // Validate triangle count fits in u32 for `t_id as u32` casts below.
        let _n_tris: u32 = tris
            .len()
            .try_into()
            .map_err(|_| FastTrimeshError::TooManyTriangles { count: tris.len() })?;

        // ----- Validate every triangle (range + non-degeneracy) -----
        for (ti, tri) in tris.iter().enumerate() {
            for (slot, &vid) in tri.iter().enumerate() {
                if vid >= n_verts {
                    return Err(FastTrimeshError::VertexIndexOutOfRange {
                        tri: ti as u32,
                        slot: slot as u8,
                        vid,
                        n_verts,
                    });
                }
            }
            if tri[0] == tri[1] || tri[1] == tri[2] || tri[0] == tri[2] {
                return Err(FastTrimeshError::DegenerateTriangle {
                    tri: ti as u32,
                    vids: *tri,
                });
            }
        }

        // ----- Build vertices + triangles -----
        let vertices: Vec<Vertex> = verts
            .iter()
            .map(|&p| Vertex {
                point: p,
                info: 0,
                orig_id: None,
            })
            .collect();
        let triangles: Vec<Triangle> = tris.iter().map(|&v| Triangle { v, info: 0 }).collect();

        // ----- Collect + sort + dedup edges -----
        // Each triangle contributes 3 edges as sorted (min, max) pairs.
        let mut sorted_edges: Vec<[u32; 2]> = Vec::with_capacity(3 * tris.len());
        for tri in tris {
            sorted_edges.push(sort_pair(tri[0], tri[1]));
            sorted_edges.push(sort_pair(tri[1], tri[2]));
            sorted_edges.push(sort_pair(tri[2], tri[0]));
        }
        sorted_edges.sort_unstable();
        sorted_edges.dedup();

        let edges: Vec<Edge> = sorted_edges
            .iter()
            .map(|&[v0, v1]| Edge {
                v0,
                v1,
                constr: false,
                visited: false,
            })
            .collect();

        // ----- Build v2e (vertex → edges) -----
        let mut v2e: Vec<Vec<u32>> = vec![Vec::new(); n_verts as usize];
        for (e_id, &[v0, v1]) in sorted_edges.iter().enumerate() {
            let e_id = e_id as u32;
            v2e[v0 as usize].push(e_id);
            v2e[v1 as usize].push(e_id);
        }

        // ----- Build e2t (edge → triangles) via binary search -----
        let mut e2t: Vec<Vec<u32>> = vec![Vec::new(); sorted_edges.len()];
        for (t_id, tri) in tris.iter().enumerate() {
            let t_id = t_id as u32;
            let e0 = lookup_edge(&sorted_edges, sort_pair(tri[0], tri[1]));
            let e1 = lookup_edge(&sorted_edges, sort_pair(tri[1], tri[2]));
            let e2 = lookup_edge(&sorted_edges, sort_pair(tri[2], tri[0]));
            e2t[e0 as usize].push(t_id);
            e2t[e1 as usize].push(t_id);
            e2t[e2 as usize].push(t_id);
        }

        Ok(Self {
            vertices,
            edges,
            triangles,
            v2e,
            e2t,
            plane,
            rev_vtx_map: HashMap::new(),
        })
    }

    // ----- Counts -----

    pub fn num_verts(&self) -> u32 {
        self.vertices.len() as u32
    }

    pub fn num_edges(&self) -> u32 {
        self.edges.len() as u32
    }

    pub fn num_tris(&self) -> u32 {
        self.triangles.len() as u32
    }

    pub fn ref_plane(&self) -> Plane {
        self.plane
    }

    // ----- Vertex queries -----

    pub fn vert(&self, v: u32) -> Point3 {
        debug_assert!(v < self.num_verts(), "vert: id {v} out of range");
        self.vertices[v as usize].point
    }

    pub fn vert_info(&self, v: u32) -> u32 {
        debug_assert!(v < self.num_verts(), "vert_info: id {v} out of range");
        self.vertices[v as usize].info
    }

    pub fn vert_valence(&self, v: u32) -> u32 {
        debug_assert!(v < self.num_verts(), "vert_valence: id {v} out of range");
        self.v2e[v as usize].len() as u32
    }

    pub fn adj_v2e(&self, v: u32) -> &[u32] {
        debug_assert!(v < self.num_verts(), "adj_v2e: id {v} out of range");
        &self.v2e[v as usize]
    }

    // ----- Edge queries -----

    pub fn edge(&self, e: u32) -> (u32, u32) {
        debug_assert!(e < self.num_edges(), "edge: id {e} out of range");
        let edge = &self.edges[e as usize];
        (edge.v0, edge.v1)
    }

    pub fn edge_vert_id(&self, e: u32, off: u32) -> u32 {
        debug_assert!(e < self.num_edges(), "edge_vert_id: id {e} out of range");
        debug_assert!(off < 2, "edge_vert_id: off {off} not in {{0, 1}}");
        let edge = &self.edges[e as usize];
        if off == 0 {
            edge.v0
        } else {
            edge.v1
        }
    }

    pub fn edge_id(&self, u: u32, v: u32) -> Option<u32> {
        if u == v || u >= self.num_verts() || v >= self.num_verts() {
            return None;
        }
        let (a, b) = if u < v { (u, v) } else { (v, u) };
        // Linear search through edges incident to the lower-indexed vertex.
        for &e in &self.v2e[a as usize] {
            let edge = &self.edges[e as usize];
            if edge.v0 == a && edge.v1 == b {
                return Some(e);
            }
        }
        None
    }

    pub fn edge_is_constr(&self, e: u32) -> bool {
        debug_assert!(e < self.num_edges(), "edge_is_constr: id {e} out of range");
        self.edges[e as usize].constr
    }

    pub fn edge_is_visited(&self, e: u32) -> bool {
        debug_assert!(e < self.num_edges(), "edge_is_visited: id {e} out of range");
        self.edges[e as usize].visited
    }

    pub fn edge_is_boundary(&self, e: u32) -> bool {
        debug_assert!(
            e < self.num_edges(),
            "edge_is_boundary: id {e} out of range"
        );
        self.e2t[e as usize].len() == 1
    }

    pub fn edge_is_manifold(&self, e: u32) -> bool {
        debug_assert!(
            e < self.num_edges(),
            "edge_is_manifold: id {e} out of range"
        );
        self.e2t[e as usize].len() <= 2
    }

    pub fn adj_e2t(&self, e: u32) -> &[u32] {
        debug_assert!(e < self.num_edges(), "adj_e2t: id {e} out of range");
        &self.e2t[e as usize]
    }

    // ----- Triangle queries -----

    pub fn tri(&self, t: u32) -> [u32; 3] {
        debug_assert!(t < self.num_tris(), "tri: id {t} out of range");
        self.triangles[t as usize].v
    }

    pub fn tri_vert_id(&self, t: u32, off: u32) -> u32 {
        debug_assert!(t < self.num_tris(), "tri_vert_id: id {t} out of range");
        debug_assert!(off < 3, "tri_vert_id: off {off} not in {{0, 1, 2}}");
        self.triangles[t as usize].v[off as usize]
    }

    pub fn tri_vert(&self, t: u32, off: u32) -> Point3 {
        debug_assert!(t < self.num_tris(), "tri_vert: id {t} out of range");
        debug_assert!(off < 3, "tri_vert: off {off} not in {{0, 1, 2}}");
        let vid = self.triangles[t as usize].v[off as usize];
        self.vertices[vid as usize].point
    }

    pub fn tri_vert_offset(&self, t: u32, v: u32) -> Option<u32> {
        debug_assert!(t < self.num_tris(), "tri_vert_offset: id {t} out of range");
        let tri = &self.triangles[t as usize].v;
        for (off, &vid) in tri.iter().enumerate() {
            if vid == v {
                return Some(off as u32);
            }
        }
        None
    }

    pub fn tri_contains_vert(&self, t: u32, v: u32) -> bool {
        self.tri_vert_offset(t, v).is_some()
    }

    pub fn tri_edges(&self, t: u32) -> [u32; 3] {
        debug_assert!(t < self.num_tris(), "tri_edges: id {t} out of range");
        let tri = &self.triangles[t as usize].v;
        // Each edge MUST exist (invariant); unwrap is safe here.
        let e0 = self.edge_id(tri[0], tri[1]).expect("tri edge 0 missing");
        let e1 = self.edge_id(tri[1], tri[2]).expect("tri edge 1 missing");
        let e2 = self.edge_id(tri[2], tri[0]).expect("tri edge 2 missing");
        [e0, e1, e2]
    }

    pub fn tri_info(&self, t: u32) -> u32 {
        debug_assert!(t < self.num_tris(), "tri_info: id {t} out of range");
        self.triangles[t as usize].info
    }
}

// =========================================================================
// PR-CR12a — mutator API (addition + setters + derived adjacency)
// =========================================================================

impl FastTrimesh {
    // ----- Vertex addition -----

    /// Append a new vertex with no original mesh ID. Returns the new
    /// vertex's u32 ID (= previous `num_verts()`).
    pub fn add_vert(&mut self, p: Point3) -> u32 {
        let v_id = self.vertices.len() as u32;
        self.vertices.push(Vertex {
            point: p,
            info: 0,
            orig_id: None,
        });
        self.v2e.push(Vec::new());
        v_id
    }

    /// Append a new vertex carrying its original mesh ID. Populates
    /// `rev_vtx_map[orig_id] = new_v_id`. Returns the new vertex's ID.
    pub fn add_vert_with_orig_id(&mut self, p: Point3, orig_id: u32) -> u32 {
        let v_id = self.vertices.len() as u32;
        self.vertices.push(Vertex {
            point: p,
            info: 0,
            orig_id: Some(orig_id),
        });
        self.v2e.push(Vec::new());
        self.rev_vtx_map.insert(orig_id, v_id);
        v_id
    }

    /// Original mesh ID of a vertex, or `None` if not assigned.
    pub fn vert_orig_id(&self, v: u32) -> Option<u32> {
        debug_assert!(v < self.num_verts(), "vert_orig_id: id {v} out of range");
        self.vertices[v as usize].orig_id
    }

    /// Reverse lookup: original mesh ID → new vertex ID, or `None`
    /// if no vertex carries that orig_id.
    pub fn vert_new_id(&self, orig_id: u32) -> Option<u32> {
        self.rev_vtx_map.get(&orig_id).copied()
    }

    // ----- Triangle addition + lookup -----

    /// Append a triangle with the given vertex IDs. Returns the new
    /// triangle's ID. If a triangle with the same 3-vertex set already
    /// exists (any rotation), returns its existing ID instead of adding
    /// a duplicate.
    pub fn add_tri(&mut self, v0: u32, v1: u32, v2: u32) -> u32 {
        debug_assert!(
            v0 != v1 && v1 != v2 && v0 != v2,
            "add_tri: degenerate triangle [{v0}, {v1}, {v2}]"
        );
        debug_assert!(
            v0 < self.num_verts() && v1 < self.num_verts() && v2 < self.num_verts(),
            "add_tri: vertex id out of range"
        );
        if let Some(t) = self.tri_id(v0, v1, v2) {
            return t;
        }
        let t_id = self.triangles.len() as u32;
        let e0 = self.add_edge(v0, v1);
        let e1 = self.add_edge(v1, v2);
        let e2 = self.add_edge(v2, v0);
        self.triangles.push(Triangle {
            v: [v0, v1, v2],
            info: 0,
        });
        self.e2t[e0 as usize].push(t_id);
        self.e2t[e1 as usize].push(t_id);
        self.e2t[e2 as usize].push(t_id);
        t_id
    }

    /// Look up a triangle by its 3-vertex set (any rotation matches).
    /// Returns `None` if no such triangle exists.
    ///
    /// Algorithm mirrors upstream `triID` (cpp:395-407): find the
    /// edge `(v0, v1)`; among triangles touching that edge, return the
    /// one that also contains `v2`.
    pub fn tri_id(&self, v0: u32, v1: u32, v2: u32) -> Option<u32> {
        let e = self.edge_id(v0, v1)?;
        self.adj_e2t(e)
            .iter()
            .copied()
            .find(|&t| self.tri_contains_vert(t, v2))
    }

    /// Private helper: deduplicating edge insertion. If an edge with
    /// endpoints `{u, v}` already exists, returns its ID. Otherwise
    /// appends a new edge (with sorted endpoints), updates `v2e` for
    /// both endpoints, and seeds an empty `e2t` slot.
    fn add_edge(&mut self, u: u32, v: u32) -> u32 {
        if let Some(e) = self.edge_id(u, v) {
            return e;
        }
        let (v0, v1) = if u < v { (u, v) } else { (v, u) };
        let e_id = self.edges.len() as u32;
        self.edges.push(Edge {
            v0,
            v1,
            constr: false,
            visited: false,
        });
        self.v2e[v0 as usize].push(e_id);
        self.v2e[v1 as usize].push(e_id);
        self.e2t.push(Vec::new());
        e_id
    }

    // ----- Info / flag setters -----

    /// Set the user-controlled `info` field on a vertex.
    pub fn set_vert_info(&mut self, v: u32, info: u32) {
        debug_assert!(v < self.num_verts(), "set_vert_info: id {v} out of range");
        self.vertices[v as usize].info = info;
    }

    /// Set the user-controlled `info` field on a triangle.
    pub fn set_tri_info(&mut self, t: u32, info: u32) {
        debug_assert!(t < self.num_tris(), "set_tri_info: id {t} out of range");
        self.triangles[t as usize].info = info;
    }

    /// Mark an edge as constrained (`constr = true`). No clearing
    /// API — matches upstream cpp:320-324.
    pub fn set_edge_constr(&mut self, e: u32) {
        debug_assert!(e < self.num_edges(), "set_edge_constr: id {e} out of range");
        self.edges[e as usize].constr = true;
    }

    /// Set the `visited` flag on an edge. Writes to the separate
    /// `visited` field (CR11 deviation: upstream reuses `constr`
    /// storage for both flags; cherchi-rs splits them).
    pub fn edge_set_visited(&mut self, e: u32, vis: bool) {
        debug_assert!(
            e < self.num_edges(),
            "edge_set_visited: id {e} out of range"
        );
        self.edges[e as usize].visited = vis;
    }

    // ----- Bulk resetters -----

    /// Zero the `info` field on all vertices. Does NOT touch
    /// `orig_id`, edges, or geometry.
    pub fn reset_vertices_info(&mut self) {
        for v in &mut self.vertices {
            v.info = 0;
        }
    }

    /// Zero the `info` field on all triangles.
    pub fn reset_triangles_info(&mut self) {
        for t in &mut self.triangles {
            t.info = 0;
        }
    }

    // ----- Derived adjacency -----

    /// Triangles sharing an edge with `t`. Derived via double-hop
    /// over `tri_edges(t)` + `adj_e2t(e)`; excludes `t` itself.
    /// Mirrors upstream `adjT2T` (cpp:520-535).
    pub fn adj_t2t(&self, t: u32) -> Vec<u32> {
        debug_assert!(t < self.num_tris(), "adj_t2t: id {t} out of range");
        let mut result = Vec::new();
        for e in self.tri_edges(t) {
            for &nbr_t in self.adj_e2t(e) {
                if nbr_t != t {
                    result.push(nbr_t);
                }
            }
        }
        result
    }

    /// Triangles incident to `v`. Derived via double-hop over
    /// `adj_v2e(v)` + `adj_e2t(e)`; deduplicated (a triangle sharing
    /// two edges with `v` appears once). Mirrors upstream `adjV2T`
    /// (cpp:238-251) which uses `remove_duplicates` at the end.
    pub fn adj_v2t(&self, v: u32) -> Vec<u32> {
        debug_assert!(v < self.num_verts(), "adj_v2t: id {v} out of range");
        let mut result = Vec::new();
        for &e in self.adj_v2e(v) {
            for &t in self.adj_e2t(e) {
                result.push(t);
            }
        }
        result.sort_unstable();
        result.dedup();
        result
    }
}

// =========================================================================
// PR-CR12b — removal API (swap-pop index remapping)
// =========================================================================

impl FastTrimesh {
    // ----- Public removal -----

    /// Remove a triangle, cascading any newly-dangling edges. After
    /// return, no stale reference to `t` exists in any `e2t[*]` list.
    /// Mirrors upstream `removeTri` (cpp:658-688).
    pub fn remove_tri(&mut self, t: u32) {
        debug_assert!(t < self.num_tris(), "remove_tri: id {t} out of range");
        // 1. Get the 3 edges (well-formed at this entry point).
        let edges = self.tri_edges(t);
        // 2. Remove t from each edge's e2t list.
        for &e in &edges {
            self.e2t[e as usize].retain(|&x| x != t);
        }
        // 3. Identify dangling edges (e2t now empty).
        let mut dangling: Vec<u32> = edges
            .iter()
            .copied()
            .filter(|&e| self.e2t[e as usize].is_empty())
            .collect();
        // 4. Sort dangling edges DESCENDING. Critical: ensures we
        //    always process the tail of `edges` first, so swap-pop
        //    inside remove_edge_unref doesn't shift remaining
        //    dangling IDs.
        dangling.sort_unstable_by(|a, b| b.cmp(a));
        // 5. For each dangling edge: clear v2e refs and swap-pop.
        for e in dangling {
            let edge = &self.edges[e as usize];
            let (v0, v1) = (edge.v0, edge.v1);
            self.v2e[v0 as usize].retain(|&x| x != e);
            self.v2e[v1 as usize].retain(|&x| x != e);
            self.remove_edge_unref(e);
        }
        // 6. Swap-pop the triangle.
        self.remove_tri_unref(t);
    }

    /// Remove all triangles in `ts`. Sorts descending internally so
    /// swap-pop indexing stays consistent (mirrors upstream cpp:695).
    pub fn remove_tris(&mut self, mut ts: Vec<u32>) {
        ts.sort_unstable_by(|a, b| b.cmp(a));
        for t in ts {
            self.remove_tri(t);
        }
    }

    /// Remove an edge by removing all triangles incident to it. The
    /// edge itself becomes dangling and is auto-removed during the
    /// cascade. Mirrors upstream `removeEdge` (cpp:650-654).
    pub fn remove_edge(&mut self, e: u32) {
        debug_assert!(e < self.num_edges(), "remove_edge: id {e} out of range");
        // Clone is mandatory: we mutate self.e2t inside the loop.
        let ts = self.e2t[e as usize].clone();
        self.remove_tris(ts);
    }

    // ----- Private helpers -----

    /// Partial-dismantle-tolerant version of `tri_edges`. Returns
    /// `None` for edges that have already been popped during an
    /// in-flight `remove_tri`. Used exclusively by `tri_switch`.
    /// Mirrors upstream's `-1` sentinel idiom (cpp:858-862).
    fn tri_edges_opt(&self, t: u32) -> [Option<u32>; 3] {
        debug_assert!(t < self.num_tris(), "tri_edges_opt: id {t} out of range");
        let v = self.triangles[t as usize].v;
        [
            self.edge_id(v[0], v[1]),
            self.edge_id(v[1], v[2]),
            self.edge_id(v[2], v[0]),
        ]
    }

    /// Swap-pop the triangle at slot `t`. The last triangle moves
    /// into slot `t`; `tri_switch` rewrites all e2t references.
    /// Mirrors upstream `removeTriUnref` (cpp:907-911).
    fn remove_tri_unref(&mut self, t: u32) {
        let last = self.num_tris() - 1;
        self.tri_switch(t, last);
        self.triangles.pop();
    }

    /// Swap-pop the edge at slot `e`. Edges and e2t pop in lockstep.
    /// Mirrors upstream `removeEdgeUnref` (cpp:897-903).
    fn remove_edge_unref(&mut self, e: u32) {
        self.e2t[e as usize].clear(); // sanity (must already be empty)
        let last = self.num_edges() - 1;
        self.edge_switch(e, last);
        self.edges.pop();
        self.e2t.pop();
    }

    /// Swap the triangles at slots `t0` and `t1`, rewriting all
    /// `e2t[*]` references accordingly. After return: every e2t entry
    /// that pointed to `t0` now points to `t1`, and vice versa.
    /// Mirrors upstream `triSwitch` (cpp:847-867).
    fn tri_switch(&mut self, t0: u32, t1: u32) {
        if t0 == t1 {
            return;
        }
        self.triangles.swap(t0 as usize, t1 as usize);
        // Collect up to 6 edges via tri_edges_opt (partial-dismantle
        // tolerant — some of t0's edges may already be popped).
        let mut edges_to_fix: Vec<u32> = self
            .tri_edges_opt(t0)
            .iter()
            .chain(self.tri_edges_opt(t1).iter())
            .filter_map(|&e| e)
            .collect();
        edges_to_fix.sort_unstable();
        edges_to_fix.dedup();
        for e in edges_to_fix {
            for slot in &mut self.e2t[e as usize] {
                if *slot == t0 {
                    *slot = t1;
                } else if *slot == t1 {
                    *slot = t0;
                }
            }
        }
    }

    /// Swap the edges at slots `e0` and `e1` (and their e2t lists),
    /// rewriting all `v2e[*]` references accordingly. Mirrors
    /// upstream `edgeSwitch` (cpp:871-893).
    fn edge_switch(&mut self, e0: u32, e1: u32) {
        if e0 == e1 {
            return;
        }
        self.edges.swap(e0 as usize, e1 as usize);
        self.e2t.swap(e0 as usize, e1 as usize);
        // Collect up to 4 vertex IDs from the post-swap edges.
        let e0_v = self.edges[e0 as usize];
        let e1_v = self.edges[e1 as usize];
        let mut verts_to_fix = vec![e0_v.v0, e0_v.v1, e1_v.v0, e1_v.v1];
        verts_to_fix.sort_unstable();
        verts_to_fix.dedup();
        for v in verts_to_fix {
            for slot in &mut self.v2e[v as usize] {
                if *slot == e0 {
                    *slot = e1;
                } else if *slot == e1 {
                    *slot = e0;
                }
            }
        }
    }

}

// =========================================================================
// Internal helpers
// =========================================================================

fn sort_pair(a: u32, b: u32) -> [u32; 2] {
    if a < b {
        [a, b]
    } else {
        [b, a]
    }
}

/// Binary search in a pre-sorted edge list. Panics if not found —
/// internal helper, only called with edges known to exist.
fn lookup_edge(sorted_edges: &[[u32; 2]], key: [u32; 2]) -> u32 {
    sorted_edges
        .binary_search(&key)
        .expect("lookup_edge: key missing in sorted_edges (invariant violation)") as u32
}

// =========================================================================
// Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------
    // Fixtures
    // -----------------------------------------------------------------

    fn p(x: f64, y: f64, z: f64) -> Point3 {
        Point3::new(x, y, z)
    }

    /// Single triangle in the XY plane.
    fn single_tri() -> (Vec<Point3>, Vec<[u32; 3]>) {
        (
            vec![p(0.0, 0.0, 0.0), p(1.0, 0.0, 0.0), p(0.0, 1.0, 0.0)],
            vec![[0, 1, 2]],
        )
    }

    /// Two-tri quad: a unit square in the XY plane, diagonal 0→2.
    fn two_tri_quad() -> (Vec<Point3>, Vec<[u32; 3]>) {
        (
            vec![
                p(0.0, 0.0, 0.0),
                p(1.0, 0.0, 0.0),
                p(1.0, 1.0, 0.0),
                p(0.0, 1.0, 0.0),
            ],
            vec![[0, 1, 2], [0, 2, 3]],
        )
    }

    /// Closed tetrahedron: 4 verts, 6 edges, 4 tris.
    fn tetrahedron() -> (Vec<Point3>, Vec<[u32; 3]>) {
        (
            vec![
                p(0.0, 0.0, 0.0),
                p(1.0, 0.0, 0.0),
                p(0.0, 1.0, 0.0),
                p(0.0, 0.0, 1.0),
            ],
            // 4 outward-normal triangles
            vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]],
        )
    }

    /// Closed icosahedron: 12 verts, 30 edges, 20 tris.
    /// Vertices on golden-ratio scaled cuboctahedron.
    fn icosahedron() -> (Vec<Point3>, Vec<[u32; 3]>) {
        let phi = (1.0 + 5.0_f64.sqrt()) / 2.0;
        let verts = vec![
            p(-1.0, phi, 0.0),  // 0
            p(1.0, phi, 0.0),   // 1
            p(-1.0, -phi, 0.0), // 2
            p(1.0, -phi, 0.0),  // 3
            p(0.0, -1.0, phi),  // 4
            p(0.0, 1.0, phi),   // 5
            p(0.0, -1.0, -phi), // 6
            p(0.0, 1.0, -phi),  // 7
            p(phi, 0.0, -1.0),  // 8
            p(phi, 0.0, 1.0),   // 9
            p(-phi, 0.0, -1.0), // 10
            p(-phi, 0.0, 1.0),  // 11
        ];
        let tris = vec![
            [0, 11, 5],
            [0, 5, 1],
            [0, 1, 7],
            [0, 7, 10],
            [0, 10, 11],
            [1, 5, 9],
            [5, 11, 4],
            [11, 10, 2],
            [10, 7, 6],
            [7, 1, 8],
            [3, 9, 4],
            [3, 4, 2],
            [3, 2, 6],
            [3, 6, 8],
            [3, 8, 9],
            [4, 9, 5],
            [2, 4, 11],
            [6, 2, 10],
            [8, 6, 7],
            [9, 8, 1],
        ];
        (verts, tris)
    }

    /// Non-manifold: 3 triangles sharing edge (0, 1).
    fn non_manifold_3_tris() -> (Vec<Point3>, Vec<[u32; 3]>) {
        (
            vec![
                p(0.0, 0.0, 0.0),
                p(1.0, 0.0, 0.0),
                p(0.0, 1.0, 0.0),
                p(0.0, -1.0, 0.0),
                p(0.0, 0.0, 1.0),
            ],
            vec![[0, 1, 2], [0, 1, 3], [0, 1, 4]],
        )
    }

    // -----------------------------------------------------------------
    // Group 1: Construction & basic counts
    // -----------------------------------------------------------------

    #[test]
    fn empty_input() {
        let ft = FastTrimesh::from_soup(&[], &[], Plane::XY).unwrap();
        assert_eq!(ft.num_verts(), 0);
        assert_eq!(ft.num_edges(), 0);
        assert_eq!(ft.num_tris(), 0);
        assert_eq!(ft.ref_plane(), Plane::XY);
    }

    #[test]
    fn single_triangle_counts() {
        let (v, t) = single_tri();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        assert_eq!(ft.num_verts(), 3);
        assert_eq!(ft.num_edges(), 3);
        assert_eq!(ft.num_tris(), 1);
    }

    #[test]
    fn two_tri_quad_counts() {
        let (v, t) = two_tri_quad();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        assert_eq!(ft.num_verts(), 4);
        assert_eq!(ft.num_edges(), 5);
        assert_eq!(ft.num_tris(), 2);
    }

    #[test]
    fn tetrahedron_counts() {
        let (v, t) = tetrahedron();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        assert_eq!(ft.num_verts(), 4);
        assert_eq!(ft.num_edges(), 6);
        assert_eq!(ft.num_tris(), 4);
    }

    #[test]
    fn icosahedron_counts() {
        let (v, t) = icosahedron();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        assert_eq!(ft.num_verts(), 12);
        assert_eq!(ft.num_edges(), 30);
        assert_eq!(ft.num_tris(), 20);
    }

    #[test]
    fn ref_plane_is_stored() {
        let ft = FastTrimesh::from_soup(&[], &[], Plane::YZ).unwrap();
        assert_eq!(ft.ref_plane(), Plane::YZ);
        let ft = FastTrimesh::from_soup(&[], &[], Plane::ZX).unwrap();
        assert_eq!(ft.ref_plane(), Plane::ZX);
    }

    // -----------------------------------------------------------------
    // Group 2: Vertex / triangle accessors
    // -----------------------------------------------------------------

    #[test]
    fn vert_returns_input_point() {
        let (v, t) = single_tri();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        assert_eq!(ft.vert(0), v[0]);
        assert_eq!(ft.vert(1), v[1]);
        assert_eq!(ft.vert(2), v[2]);
    }

    #[test]
    fn tri_returns_input_triple_in_order() {
        let (v, t) = two_tri_quad();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        assert_eq!(ft.tri(0), [0, 1, 2]);
        assert_eq!(ft.tri(1), [0, 2, 3]);
    }

    #[test]
    fn tri_vert_id_indirects_through_tri() {
        let (v, t) = single_tri();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        for off in 0..3 {
            assert_eq!(ft.tri_vert_id(0, off), t[0][off as usize]);
        }
    }

    #[test]
    fn tri_vert_returns_point() {
        let (v, t) = single_tri();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        for off in 0..3 {
            assert_eq!(ft.tri_vert(0, off), v[t[0][off as usize] as usize]);
        }
    }

    #[test]
    fn tri_contains_vert_matrix() {
        let (v, t) = tetrahedron();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        // Tri 0 = [0, 2, 1]
        assert!(ft.tri_contains_vert(0, 0));
        assert!(ft.tri_contains_vert(0, 1));
        assert!(ft.tri_contains_vert(0, 2));
        assert!(!ft.tri_contains_vert(0, 3));
    }

    #[test]
    fn tri_vert_offset_returns_offset_or_none() {
        let (v, t) = tetrahedron();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        // Tri 0 = [0, 2, 1]
        assert_eq!(ft.tri_vert_offset(0, 0), Some(0));
        assert_eq!(ft.tri_vert_offset(0, 2), Some(1));
        assert_eq!(ft.tri_vert_offset(0, 1), Some(2));
        assert_eq!(ft.tri_vert_offset(0, 3), None);
    }

    #[test]
    fn vert_info_is_zero_in_pr_cr11() {
        let (v, t) = single_tri();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        for i in 0..ft.num_verts() {
            assert_eq!(ft.vert_info(i), 0);
        }
    }

    #[test]
    fn tri_info_is_zero_in_pr_cr11() {
        let (v, t) = single_tri();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        for i in 0..ft.num_tris() {
            assert_eq!(ft.tri_info(i), 0);
        }
    }

    // -----------------------------------------------------------------
    // Group 3: Edge derivation correctness
    // -----------------------------------------------------------------

    #[test]
    fn single_tri_edges_sorted() {
        let (v, t) = single_tri();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        assert_eq!(ft.num_edges(), 3);
        for e in 0..ft.num_edges() {
            let (v0, v1) = ft.edge(e);
            assert!(v0 < v1, "edge {e} = ({v0}, {v1}) not sorted");
        }
    }

    #[test]
    fn tetrahedron_has_six_edges() {
        let (v, t) = tetrahedron();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        assert_eq!(ft.num_edges(), 6);
    }

    #[test]
    fn two_tri_quad_shares_one_edge() {
        let (v, t) = two_tri_quad();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        // 2 tris × 3 edges = 6 incidences. 5 unique edges means 1 shared.
        assert_eq!(ft.num_edges(), 5);
    }

    #[test]
    fn edge_id_is_argument_order_independent() {
        let (v, t) = tetrahedron();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        for u in 0..ft.num_verts() {
            for w in 0..ft.num_verts() {
                if u != w {
                    assert_eq!(
                        ft.edge_id(u, w),
                        ft.edge_id(w, u),
                        "edge_id({u}, {w}) != edge_id({w}, {u})"
                    );
                }
            }
        }
    }

    #[test]
    fn edge_id_returns_none_for_missing_edge() {
        let (v, t) = single_tri();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        // No vertex 99
        assert_eq!(ft.edge_id(0, 99), None);
    }

    #[test]
    fn edge_vert_id_matches_edge() {
        let (v, t) = single_tri();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        for e in 0..ft.num_edges() {
            let (a, b) = ft.edge(e);
            assert_eq!(ft.edge_vert_id(e, 0), a);
            assert_eq!(ft.edge_vert_id(e, 1), b);
        }
    }

    #[test]
    fn edge_is_constr_false_in_pr_cr11() {
        let (v, t) = single_tri();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        for e in 0..ft.num_edges() {
            assert!(!ft.edge_is_constr(e));
        }
    }

    #[test]
    fn edge_is_visited_false_in_pr_cr11() {
        let (v, t) = single_tri();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        for e in 0..ft.num_edges() {
            assert!(!ft.edge_is_visited(e));
        }
    }

    // -----------------------------------------------------------------
    // Group 4: Adjacency correctness
    // -----------------------------------------------------------------

    #[test]
    fn tetrahedron_vertex_valences_all_three() {
        let (v, t) = tetrahedron();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        for i in 0..ft.num_verts() {
            assert_eq!(ft.vert_valence(i), 3, "vertex {i}");
        }
    }

    #[test]
    fn tetrahedron_every_edge_has_two_tris() {
        let (v, t) = tetrahedron();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        for e in 0..ft.num_edges() {
            assert_eq!(ft.adj_e2t(e).len(), 2, "edge {e}");
            assert!(ft.edge_is_manifold(e));
            assert!(!ft.edge_is_boundary(e));
        }
    }

    #[test]
    fn icosahedron_every_edge_has_two_tris() {
        let (v, t) = icosahedron();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        for e in 0..ft.num_edges() {
            assert_eq!(ft.adj_e2t(e).len(), 2);
            assert!(ft.edge_is_manifold(e));
        }
    }

    #[test]
    fn two_tri_quad_diagonal_is_shared() {
        let (v, t) = two_tri_quad();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        // Diagonal is edge (0, 2)
        let e = ft.edge_id(0, 2).expect("diagonal edge");
        assert_eq!(ft.adj_e2t(e).len(), 2);
        assert!(!ft.edge_is_boundary(e));
    }

    #[test]
    fn two_tri_quad_boundary_has_one_tri() {
        let (v, t) = two_tri_quad();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        // Edge (0, 1) is boundary
        let e = ft.edge_id(0, 1).expect("boundary edge");
        assert_eq!(ft.adj_e2t(e).len(), 1);
        assert!(ft.edge_is_boundary(e));
    }

    #[test]
    fn non_manifold_edge_detected() {
        let (v, t) = non_manifold_3_tris();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        // Edge (0, 1) is shared by 3 tris
        let e = ft.edge_id(0, 1).expect("non-manifold edge");
        assert_eq!(ft.adj_e2t(e).len(), 3);
        assert!(!ft.edge_is_manifold(e));
    }

    #[test]
    fn valence_sum_equals_twice_num_edges() {
        let (v, t) = icosahedron();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        let total_valence: u32 = (0..ft.num_verts()).map(|i| ft.vert_valence(i)).sum();
        assert_eq!(total_valence, 2 * ft.num_edges());
    }

    #[test]
    fn e2t_sum_equals_thrice_num_tris() {
        let (v, t) = icosahedron();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        let total_incidences: usize = (0..ft.num_edges()).map(|e| ft.adj_e2t(e).len()).sum();
        assert_eq!(total_incidences, 3 * ft.num_tris() as usize);
    }

    #[test]
    fn tri_edges_reference_their_tri() {
        let (v, t) = tetrahedron();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        for ti in 0..ft.num_tris() {
            let es = ft.tri_edges(ti);
            for &e in &es {
                assert!(
                    ft.adj_e2t(e).contains(&ti),
                    "tri {ti} edge {e} doesn't list it back"
                );
            }
        }
    }

    #[test]
    fn adj_v2e_contains_only_incident_edges() {
        let (v, t) = tetrahedron();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        for vi in 0..ft.num_verts() {
            for &e in ft.adj_v2e(vi) {
                let (a, b) = ft.edge(e);
                assert!(
                    a == vi || b == vi,
                    "edge {e} listed in v2e[{vi}] but doesn't touch it"
                );
            }
        }
    }

    // -----------------------------------------------------------------
    // Group 5: Error / edge cases
    // -----------------------------------------------------------------

    #[test]
    fn out_of_range_vertex_index_is_err() {
        let verts = vec![p(0.0, 0.0, 0.0), p(1.0, 0.0, 0.0), p(0.0, 1.0, 0.0)];
        // Triangle references vertex 99 (doesn't exist)
        let tris = vec![[0u32, 1, 99]];
        let err = FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap_err();
        assert_eq!(
            err,
            FastTrimeshError::VertexIndexOutOfRange {
                tri: 0,
                slot: 2,
                vid: 99,
                n_verts: 3,
            }
        );
    }

    #[test]
    fn degenerate_triangle_is_err() {
        let verts = vec![p(0.0, 0.0, 0.0), p(1.0, 0.0, 0.0), p(0.0, 1.0, 0.0)];
        // Triangle [0, 1, 0] has a repeated vertex
        let tris = vec![[0u32, 1, 0]];
        let err = FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap_err();
        assert_eq!(
            err,
            FastTrimeshError::DegenerateTriangle {
                tri: 0,
                vids: [0, 1, 0],
            }
        );
    }

    #[test]
    fn empty_tris_with_isolated_verts_is_ok() {
        let verts = vec![p(0.0, 0.0, 0.0), p(1.0, 0.0, 0.0)];
        let ft = FastTrimesh::from_soup(&verts, &[], Plane::XY).unwrap();
        assert_eq!(ft.num_verts(), 2);
        assert_eq!(ft.num_edges(), 0);
        assert_eq!(ft.num_tris(), 0);
        assert_eq!(ft.vert_valence(0), 0);
        assert_eq!(ft.vert_valence(1), 0);
    }

    #[test]
    fn isolated_vertex_alongside_triangle_is_ok() {
        let verts = vec![
            p(0.0, 0.0, 0.0),
            p(1.0, 0.0, 0.0),
            p(0.0, 1.0, 0.0),
            p(5.0, 5.0, 5.0), // isolated
        ];
        let tris = vec![[0u32, 1, 2]];
        let ft = FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap();
        assert_eq!(ft.num_verts(), 4);
        assert_eq!(ft.vert_valence(3), 0);
    }

    // -----------------------------------------------------------------
    // PR-CR12a — Group 1: Vertex addition + rev_vtx_map
    // -----------------------------------------------------------------

    #[test]
    fn add_vert_returns_new_id() {
        let mut ft = FastTrimesh::from_soup(&[], &[], Plane::XY).unwrap();
        let id0 = ft.add_vert(p(1.0, 2.0, 3.0));
        assert_eq!(id0, 0);
        let id1 = ft.add_vert(p(4.0, 5.0, 6.0));
        assert_eq!(id1, 1);
        assert_eq!(ft.num_verts(), 2);
    }

    #[test]
    fn add_vert_stores_point() {
        let mut ft = FastTrimesh::from_soup(&[], &[], Plane::XY).unwrap();
        let pt = p(1.5, -2.5, 3.5);
        let id = ft.add_vert(pt);
        assert_eq!(ft.vert(id), pt);
    }

    #[test]
    fn add_vert_does_not_populate_rev_map() {
        let mut ft = FastTrimesh::from_soup(&[], &[], Plane::XY).unwrap();
        let id = ft.add_vert(p(0.0, 0.0, 0.0));
        assert_eq!(ft.vert_orig_id(id), None);
    }

    #[test]
    fn add_vert_with_orig_id_round_trip() {
        let mut ft = FastTrimesh::from_soup(&[], &[], Plane::XY).unwrap();
        let id = ft.add_vert_with_orig_id(p(0.0, 0.0, 0.0), 42);
        assert_eq!(ft.vert_orig_id(id), Some(42));
        assert_eq!(ft.vert_new_id(42), Some(id));
    }

    #[test]
    fn vert_new_id_returns_none_for_unknown_orig() {
        let ft = FastTrimesh::from_soup(&[], &[], Plane::XY).unwrap();
        assert_eq!(ft.vert_new_id(99), None);
    }

    #[test]
    fn add_vert_initializes_empty_valence() {
        let mut ft = FastTrimesh::from_soup(&[], &[], Plane::XY).unwrap();
        let id = ft.add_vert(p(0.0, 0.0, 0.0));
        assert_eq!(ft.vert_valence(id), 0);
        assert_eq!(ft.adj_v2e(id), &[] as &[u32]);
    }

    #[test]
    fn add_vert_orig_id_zero_is_distinct_from_no_orig() {
        // The whole point of Option<u32>: orig_id 0 should be
        // distinguishable from "no orig_id assigned."
        let mut ft = FastTrimesh::from_soup(&[], &[], Plane::XY).unwrap();
        let no_orig = ft.add_vert(p(1.0, 0.0, 0.0));
        let with_zero = ft.add_vert_with_orig_id(p(2.0, 0.0, 0.0), 0);
        assert_eq!(ft.vert_orig_id(no_orig), None);
        assert_eq!(ft.vert_orig_id(with_zero), Some(0));
        assert_eq!(ft.vert_new_id(0), Some(with_zero));
    }

    // -----------------------------------------------------------------
    // PR-CR12a — Group 2: Triangle addition + dedup
    // -----------------------------------------------------------------

    #[test]
    fn add_tri_returns_new_id() {
        let mut ft = FastTrimesh::from_soup(&[], &[], Plane::XY).unwrap();
        ft.add_vert(p(0.0, 0.0, 0.0));
        ft.add_vert(p(1.0, 0.0, 0.0));
        ft.add_vert(p(0.0, 1.0, 0.0));
        let t = ft.add_tri(0, 1, 2);
        assert_eq!(t, 0);
        assert_eq!(ft.num_tris(), 1);
        assert_eq!(ft.tri(t), [0, 1, 2]);
    }

    #[test]
    fn add_tri_dedups_exact_repeat() {
        let mut ft = FastTrimesh::from_soup(&[], &[], Plane::XY).unwrap();
        ft.add_vert(p(0.0, 0.0, 0.0));
        ft.add_vert(p(1.0, 0.0, 0.0));
        ft.add_vert(p(0.0, 1.0, 0.0));
        let t0 = ft.add_tri(0, 1, 2);
        let t1 = ft.add_tri(0, 1, 2);
        assert_eq!(t0, t1);
        assert_eq!(ft.num_tris(), 1);
    }

    #[test]
    fn add_tri_dedups_rotation() {
        let mut ft = FastTrimesh::from_soup(&[], &[], Plane::XY).unwrap();
        ft.add_vert(p(0.0, 0.0, 0.0));
        ft.add_vert(p(1.0, 0.0, 0.0));
        ft.add_vert(p(0.0, 1.0, 0.0));
        let t0 = ft.add_tri(0, 1, 2);
        let t1 = ft.add_tri(1, 2, 0);
        let t2 = ft.add_tri(2, 0, 1);
        assert_eq!(t0, t1);
        assert_eq!(t1, t2);
        assert_eq!(ft.num_tris(), 1);
    }

    #[test]
    fn add_tri_creates_three_edges() {
        let mut ft = FastTrimesh::from_soup(&[], &[], Plane::XY).unwrap();
        ft.add_vert(p(0.0, 0.0, 0.0));
        ft.add_vert(p(1.0, 0.0, 0.0));
        ft.add_vert(p(0.0, 1.0, 0.0));
        ft.add_tri(0, 1, 2);
        assert_eq!(ft.num_edges(), 3);
        assert!(ft.edge_id(0, 1).is_some());
        assert!(ft.edge_id(1, 2).is_some());
        assert!(ft.edge_id(0, 2).is_some());
    }

    #[test]
    fn add_tri_shares_existing_edges() {
        let mut ft = FastTrimesh::from_soup(&[], &[], Plane::XY).unwrap();
        ft.add_vert(p(0.0, 0.0, 0.0));
        ft.add_vert(p(1.0, 0.0, 0.0));
        ft.add_vert(p(0.0, 1.0, 0.0));
        ft.add_vert(p(0.0, -1.0, 0.0));
        ft.add_tri(0, 1, 2);
        ft.add_tri(0, 1, 3); // shares edge (0,1)
        assert_eq!(ft.num_edges(), 5); // 3 + 2 new = 5
        let e = ft.edge_id(0, 1).unwrap();
        assert_eq!(ft.adj_e2t(e).len(), 2);
    }

    #[test]
    fn add_tri_preserves_sum_invariants() {
        let mut ft = FastTrimesh::from_soup(&[], &[], Plane::XY).unwrap();
        for i in 0..4 {
            ft.add_vert(p(i as f64, 0.0, 0.0));
        }
        ft.add_tri(0, 1, 2);
        ft.add_tri(0, 1, 3);
        let sum_v2e: u32 = (0..ft.num_verts()).map(|v| ft.vert_valence(v)).sum();
        let sum_e2t: usize = (0..ft.num_edges()).map(|e| ft.adj_e2t(e).len()).sum();
        assert_eq!(sum_v2e, 2 * ft.num_edges());
        assert_eq!(sum_e2t, 3 * ft.num_tris() as usize);
    }

    #[test]
    fn tri_id_returns_some_after_add() {
        let mut ft = FastTrimesh::from_soup(&[], &[], Plane::XY).unwrap();
        ft.add_vert(p(0.0, 0.0, 0.0));
        ft.add_vert(p(1.0, 0.0, 0.0));
        ft.add_vert(p(0.0, 1.0, 0.0));
        assert_eq!(ft.tri_id(0, 1, 2), None);
        let t = ft.add_tri(0, 1, 2);
        assert_eq!(ft.tri_id(0, 1, 2), Some(t));
        assert_eq!(ft.tri_id(1, 2, 0), Some(t)); // rotation
    }

    // -----------------------------------------------------------------
    // PR-CR12a — Group 3: Info setters round-trip
    // -----------------------------------------------------------------

    #[test]
    fn set_vert_info_round_trip() {
        let (v, t) = single_tri();
        let mut ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        ft.set_vert_info(0, 42);
        assert_eq!(ft.vert_info(0), 42);
    }

    #[test]
    fn set_tri_info_round_trip() {
        let (v, t) = single_tri();
        let mut ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        ft.set_tri_info(0, 42);
        assert_eq!(ft.tri_info(0), 42);
    }

    #[test]
    fn set_edge_constr_round_trip() {
        let (v, t) = single_tri();
        let mut ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        assert!(!ft.edge_is_constr(0));
        ft.set_edge_constr(0);
        assert!(ft.edge_is_constr(0));
    }

    #[test]
    fn edge_set_visited_round_trip() {
        let (v, t) = single_tri();
        let mut ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        assert!(!ft.edge_is_visited(0));
        ft.edge_set_visited(0, true);
        assert!(ft.edge_is_visited(0));
        ft.edge_set_visited(0, false);
        assert!(!ft.edge_is_visited(0));
    }

    #[test]
    fn set_edge_constr_does_not_change_visited() {
        let (v, t) = single_tri();
        let mut ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        ft.set_edge_constr(0);
        assert!(!ft.edge_is_visited(0));
    }

    #[test]
    fn edge_set_visited_does_not_change_constr() {
        let (v, t) = single_tri();
        let mut ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        ft.edge_set_visited(0, true);
        assert!(!ft.edge_is_constr(0));
    }

    // -----------------------------------------------------------------
    // PR-CR12a — Group 4: Reset semantics
    // -----------------------------------------------------------------

    #[test]
    fn reset_vertices_info_zeroes_all() {
        let (v, t) = tetrahedron();
        let mut ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        for vi in 0..ft.num_verts() {
            ft.set_vert_info(vi, 42);
        }
        ft.reset_vertices_info();
        for vi in 0..ft.num_verts() {
            assert_eq!(ft.vert_info(vi), 0);
        }
    }

    #[test]
    fn reset_triangles_info_zeroes_all() {
        let (v, t) = tetrahedron();
        let mut ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        for ti in 0..ft.num_tris() {
            ft.set_tri_info(ti, 42);
        }
        ft.reset_triangles_info();
        for ti in 0..ft.num_tris() {
            assert_eq!(ft.tri_info(ti), 0);
        }
    }

    #[test]
    fn reset_vertices_info_does_not_touch_orig_id() {
        let mut ft = FastTrimesh::from_soup(&[], &[], Plane::XY).unwrap();
        let id = ft.add_vert_with_orig_id(p(0.0, 0.0, 0.0), 42);
        ft.set_vert_info(id, 99);
        ft.reset_vertices_info();
        assert_eq!(ft.vert_info(id), 0);
        assert_eq!(ft.vert_orig_id(id), Some(42)); // preserved
    }

    #[test]
    fn reset_triangles_info_does_not_touch_edge_flags() {
        let (v, t) = single_tri();
        let mut ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        ft.set_edge_constr(0);
        ft.edge_set_visited(1, true);
        ft.reset_triangles_info();
        assert!(ft.edge_is_constr(0));
        assert!(ft.edge_is_visited(1));
    }

    // -----------------------------------------------------------------
    // PR-CR12a — Group 5: Derived adjacency (adj_t2t, adj_v2t)
    // -----------------------------------------------------------------

    #[test]
    fn tetrahedron_adj_t2t_all_three() {
        let (v, t) = tetrahedron();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        for ti in 0..ft.num_tris() {
            assert_eq!(ft.adj_t2t(ti).len(), 3, "tri {ti}");
        }
    }

    #[test]
    fn tetrahedron_adj_v2t_all_three() {
        let (v, t) = tetrahedron();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        for vi in 0..ft.num_verts() {
            assert_eq!(ft.adj_v2t(vi).len(), 3, "vertex {vi}");
        }
    }

    #[test]
    fn two_tri_quad_adj_t2t_one_neighbor() {
        let (v, t) = two_tri_quad();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        assert_eq!(ft.adj_t2t(0).len(), 1);
        assert_eq!(ft.adj_t2t(1).len(), 1);
        assert_eq!(ft.adj_t2t(0)[0], 1);
        assert_eq!(ft.adj_t2t(1)[0], 0);
    }

    #[test]
    fn isolated_vertex_adj_v2t_empty() {
        let verts = vec![
            p(0.0, 0.0, 0.0),
            p(1.0, 0.0, 0.0),
            p(0.0, 1.0, 0.0),
            p(5.0, 5.0, 5.0), // isolated
        ];
        let tris = vec![[0u32, 1, 2]];
        let ft = FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap();
        assert_eq!(ft.adj_v2t(3).len(), 0);
    }

    #[test]
    fn icosahedron_adj_t2t_all_three() {
        let (v, t) = icosahedron();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        for ti in 0..ft.num_tris() {
            assert_eq!(ft.adj_t2t(ti).len(), 3, "tri {ti}");
        }
    }

    #[test]
    fn icosahedron_adj_v2t_valence_five() {
        let (v, t) = icosahedron();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        for vi in 0..ft.num_verts() {
            assert_eq!(ft.adj_v2t(vi).len(), 5, "vertex {vi}");
        }
    }

    #[test]
    fn adj_v2t_dedups_multi_edge_incidence() {
        // Vertex 0 of two-tri quad: touches both edges (0,1) and (0,3)
        // and (0,2). Each edge contributes triangles; vertex 0 is in
        // both triangles 0 and 1 — but each via different edges,
        // so the result should still list each tri once.
        let (v, t) = two_tri_quad();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        let adj = ft.adj_v2t(0);
        // Sort for deterministic comparison (HashMap-derived order).
        let mut sorted = adj.clone();
        sorted.sort();
        assert_eq!(sorted, vec![0, 1]);
    }

    // -----------------------------------------------------------------
    // PR-CR12a — Group 6: Mutator + query interaction
    // -----------------------------------------------------------------

    #[test]
    fn add_tri_then_tri_edges_works() {
        let mut ft = FastTrimesh::from_soup(&[], &[], Plane::XY).unwrap();
        ft.add_vert(p(0.0, 0.0, 0.0));
        ft.add_vert(p(1.0, 0.0, 0.0));
        ft.add_vert(p(0.0, 1.0, 0.0));
        let t = ft.add_tri(0, 1, 2);
        let edges = ft.tri_edges(t);
        for &e in &edges {
            assert!(ft.adj_e2t(e).contains(&t));
        }
    }

    #[test]
    fn from_soup_then_add_vert_then_add_tri() {
        let (v, t) = single_tri();
        let mut ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        assert_eq!(ft.num_verts(), 3);
        assert_eq!(ft.num_tris(), 1);
        let v3 = ft.add_vert(p(2.0, 0.0, 0.0));
        ft.add_tri(0, 1, v3);
        assert_eq!(ft.num_verts(), 4);
        assert_eq!(ft.num_tris(), 2);
    }

    #[test]
    fn add_vert_with_orig_id_round_trip_via_queries() {
        let mut ft = FastTrimesh::from_soup(&[], &[], Plane::XY).unwrap();
        let id = ft.add_vert_with_orig_id(p(0.0, 0.0, 0.0), 7);
        let orig = ft.vert_orig_id(id).unwrap();
        let new_id = ft.vert_new_id(orig).unwrap();
        assert_eq!(new_id, id);
    }

    #[test]
    fn from_soup_initializes_empty_rev_vtx_map() {
        let (v, t) = single_tri();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        for vi in 0..ft.num_verts() {
            assert_eq!(ft.vert_orig_id(vi), None);
        }
        // Reverse lookup for any orig_id should be None.
        assert_eq!(ft.vert_new_id(0), None);
        assert_eq!(ft.vert_new_id(1), None);
        assert_eq!(ft.vert_new_id(2), None);
    }

    // -----------------------------------------------------------------
    // PR-CR12b — canonical-shape helper for order-independence tests
    // -----------------------------------------------------------------

    /// Captures the "shape" of a `FastTrimesh` in a form that's
    /// independent of HashMap iteration order and swap-pop slot
    /// assignment. Two meshes with equal `canonical_shape` are
    /// considered topologically equivalent.
    type Shape = (u32, u32, u32, Vec<[u32; 2]>, Vec<[u32; 3]>, Vec<u32>);

    fn canonical_shape(ft: &FastTrimesh) -> Shape {
        let n_v = ft.num_verts();
        let n_e = ft.num_edges();
        let n_t = ft.num_tris();
        let mut edges: Vec<[u32; 2]> = (0..n_e)
            .map(|e| {
                let (a, b) = ft.edge(e);
                [a, b]
            })
            .collect();
        edges.sort();
        let mut tris: Vec<[u32; 3]> = (0..n_t)
            .map(|t| {
                let mut v = ft.tri(t);
                v.sort();
                v
            })
            .collect();
        tris.sort();
        let mut valences: Vec<u32> = (0..n_v).map(|v| ft.vert_valence(v)).collect();
        valences.sort();
        (n_v, n_e, n_t, edges, tris, valences)
    }

    // -----------------------------------------------------------------
    // PR-CR12b — Group 1: single-triangle removal cascade
    // -----------------------------------------------------------------

    #[test]
    fn remove_only_tri_isolates_verts() {
        let (v, t) = single_tri();
        let mut ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        ft.remove_tri(0);
        assert_eq!(ft.num_verts(), 3);
        assert_eq!(ft.num_edges(), 0);
        assert_eq!(ft.num_tris(), 0);
        for vi in 0..ft.num_verts() {
            assert_eq!(ft.vert_valence(vi), 0);
        }
    }

    #[test]
    fn remove_only_tri_cascades_three_edges() {
        let (v, t) = single_tri();
        let mut ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        assert_eq!(ft.num_edges(), 3);
        ft.remove_tri(0);
        assert_eq!(ft.num_edges(), 0);
    }

    // -----------------------------------------------------------------
    // PR-CR12b — Group 2: manifold (tetrahedron) removal
    // -----------------------------------------------------------------

    #[test]
    fn remove_one_from_tetrahedron_leaves_three() {
        let (v, t) = tetrahedron();
        let mut ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        ft.remove_tri(0);
        assert_eq!(ft.num_verts(), 4);
        assert_eq!(ft.num_tris(), 3);
        // All 6 edges still present — none became dangling because
        // each edge of tri 0 is shared with another tri.
        assert_eq!(ft.num_edges(), 6);
    }

    #[test]
    fn remove_all_tetra_tris_one_by_one() {
        let (v, t) = tetrahedron();
        let mut ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        // Remove descending — swap-pop disciplined order.
        for _ in 0..4 {
            ft.remove_tri(ft.num_tris() - 1);
        }
        assert_eq!(ft.num_verts(), 4);
        assert_eq!(ft.num_edges(), 0);
        assert_eq!(ft.num_tris(), 0);
    }

    #[test]
    fn tetra_sum_invariants_hold_after_remove() {
        let (v, t) = tetrahedron();
        let mut ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        ft.remove_tri(0);
        let sum_v2e: u32 = (0..ft.num_verts()).map(|v| ft.vert_valence(v)).sum();
        let sum_e2t: usize = (0..ft.num_edges()).map(|e| ft.adj_e2t(e).len()).sum();
        assert_eq!(sum_v2e, 2 * ft.num_edges());
        assert_eq!(sum_e2t, 3 * ft.num_tris() as usize);
    }

    // -----------------------------------------------------------------
    // PR-CR12b — Group 3: non-manifold + boundary
    // -----------------------------------------------------------------

    #[test]
    fn non_manifold_remove_one_keeps_edge() {
        let (v, t) = non_manifold_3_tris();
        let mut ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        let e = ft.edge_id(0, 1).expect("shared edge");
        assert_eq!(ft.adj_e2t(e).len(), 3);
        ft.remove_tri(0);
        // Edge (0,1) should still exist — 2 tris remain on it.
        let e = ft.edge_id(0, 1).expect("shared edge should still exist");
        assert_eq!(ft.adj_e2t(e).len(), 2);
    }

    #[test]
    fn quad_remove_one_keeps_diagonal_as_boundary() {
        let (v, t) = two_tri_quad();
        let mut ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        ft.remove_tri(0);
        assert_eq!(ft.num_verts(), 4);
        assert_eq!(ft.num_tris(), 1);
        // 3 edges of remaining tri: 2 quad-boundary + 1 diagonal.
        assert_eq!(ft.num_edges(), 3);
    }

    #[test]
    fn quad_remove_both_tris_clears_everything() {
        let (v, t) = two_tri_quad();
        let mut ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        ft.remove_tri(1);
        ft.remove_tri(0);
        assert_eq!(ft.num_verts(), 4);
        assert_eq!(ft.num_edges(), 0);
        assert_eq!(ft.num_tris(), 0);
    }

    // -----------------------------------------------------------------
    // PR-CR12b — Group 4: order independence via canonical_shape
    // -----------------------------------------------------------------

    #[test]
    fn tetra_removal_order_independent() {
        let (v, t) = tetrahedron();
        let mut a = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        let mut b = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        // Remove higher-id first in both (descending discipline).
        a.remove_tri(2);
        a.remove_tri(0);
        b.remove_tri(2);
        b.remove_tri(0);
        assert_eq!(canonical_shape(&a), canonical_shape(&b));
    }

    #[test]
    fn icosa_removal_two_orders_same_shape() {
        let (v, t) = icosahedron();
        let mut a = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        let mut b = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        // Remove the same tris in different valid orders.
        // (Use descending so swap-pop doesn't shift the targets.)
        a.remove_tri(15);
        a.remove_tri(10);
        a.remove_tri(5);
        b.remove_tri(15);
        b.remove_tri(10);
        b.remove_tri(5);
        assert_eq!(canonical_shape(&a), canonical_shape(&b));
    }

    // -----------------------------------------------------------------
    // PR-CR12b — Group 5: batch consistency
    // -----------------------------------------------------------------

    #[test]
    fn remove_tris_matches_sequential_removes() {
        let (v, t) = tetrahedron();
        let mut batch = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        let mut seq = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        batch.remove_tris(vec![0, 1]);
        // Sequential: descending discipline.
        seq.remove_tri(1);
        seq.remove_tri(0);
        assert_eq!(canonical_shape(&batch), canonical_shape(&seq));
    }

    #[test]
    fn remove_tris_empty_is_noop() {
        let (v, t) = tetrahedron();
        let mut ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        let before = canonical_shape(&ft);
        ft.remove_tris(vec![]);
        assert_eq!(canonical_shape(&ft), before);
    }

    #[test]
    fn remove_tris_handles_ascending_input() {
        // Caller passes ascending order; the method must sort
        // descending internally to keep swap-pop indexing safe.
        let (v, t) = tetrahedron();
        let mut ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        ft.remove_tris(vec![0, 1, 2]); // ascending intentionally
        assert_eq!(ft.num_tris(), 1);
        // The surviving tri must be coherent with adjacency invariants.
        let sum_v2e: u32 = (0..ft.num_verts()).map(|v| ft.vert_valence(v)).sum();
        let sum_e2t: usize = (0..ft.num_edges()).map(|e| ft.adj_e2t(e).len()).sum();
        assert_eq!(sum_v2e, 2 * ft.num_edges());
        assert_eq!(sum_e2t, 3 * ft.num_tris() as usize);
    }

    // -----------------------------------------------------------------
    // PR-CR12b — Group 6: edge removal
    // -----------------------------------------------------------------

    #[test]
    fn remove_edge_on_tetra_drops_two_tris() {
        let (v, t) = tetrahedron();
        let mut ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        // Pick an interior edge — every tetra edge has 2 incident tris.
        let e = ft.edge_id(0, 1).expect("edge (0,1)");
        assert_eq!(ft.adj_e2t(e).len(), 2);
        ft.remove_edge(e);
        assert_eq!(ft.num_tris(), 2);
        // Edge (0,1) itself should be gone (it became dangling).
        assert_eq!(ft.edge_id(0, 1), None);
    }

    #[test]
    fn remove_diagonal_on_two_tri_quad_drops_both() {
        let (v, t) = two_tri_quad();
        let mut ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        let diag = ft.edge_id(0, 2).expect("quad diagonal");
        ft.remove_edge(diag);
        assert_eq!(ft.num_tris(), 0);
        assert_eq!(ft.num_edges(), 0);
        assert_eq!(ft.num_verts(), 4);
    }

    // -----------------------------------------------------------------
    // PR-CR12b — Group 7: index remapping correctness (load-bearing)
    // -----------------------------------------------------------------

    #[test]
    fn swap_into_zero_remaps_e2t_refs() {
        // Tetrahedron: 4 tris. Remove tri 0. Swap-pop puts OLD tri 3
        // into slot 0. For every edge incident to OLD tri 3, adj_e2t
        // should now contain 0 (the new slot) and NOT 3 (the popped
        // slot).
        let (v, t) = tetrahedron();
        let mut ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        let old_tri_3_verts = ft.tri(3);
        ft.remove_tri(0);
        assert_eq!(ft.tri(0), old_tri_3_verts);
        // Every edge incident to tri 0 (which is OLD tri 3) should
        // reference 0 in its adj_e2t.
        for e in ft.tri_edges(0) {
            let adj = ft.adj_e2t(e);
            assert!(adj.contains(&0), "edge {e} missing new tri 0");
            assert!(!adj.contains(&3), "edge {e} still references popped tri 3");
        }
    }

    #[test]
    fn cascading_swap_pop_matches_ground_truth() {
        // Tetrahedron: remove tri 0, then tri 0 again. Final state
        // should be canonically equivalent to building a 2-tri mesh
        // fresh from from_soup (the remaining tetra faces). If the
        // e2t remap after the first remove was wrong, the second
        // remove panics or silently corrupts.
        let (v, t) = tetrahedron();
        let mut victim = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        victim.remove_tri(0);
        victim.remove_tri(0);
        // The 2 remaining tris are the original tris 1, 2 (in some
        // slot assignment). canonical_shape captures their topology.
        let ground = FastTrimesh::from_soup(&v, &[t[1], t[2]], Plane::XY).unwrap();
        assert_eq!(canonical_shape(&victim), canonical_shape(&ground));
    }

    #[test]
    fn sum_invariants_after_multi_remove_on_icosa() {
        let (v, t) = icosahedron();
        let mut ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        // Remove several tris in descending order.
        for victim in [15u32, 10, 5] {
            ft.remove_tri(victim);
            let sum_v2e: u32 = (0..ft.num_verts()).map(|v| ft.vert_valence(v)).sum();
            let sum_e2t: usize = (0..ft.num_edges()).map(|e| ft.adj_e2t(e).len()).sum();
            assert_eq!(
                sum_v2e,
                2 * ft.num_edges(),
                "sum |v2e| != 2·E after removing {victim}"
            );
            assert_eq!(
                sum_e2t,
                3 * ft.num_tris() as usize,
                "sum |e2t| != 3·T after removing {victim}"
            );
        }
    }
}
