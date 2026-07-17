//! Native five-axiom input census — the diagnostic analog of the C++
//! `mesh_booleans_inputcheck` (localizing, where the reference binary is
//! pass/fail-only per axiom).
//!
//! Ported behavior from Cherchi et al. 2020 / 2022 (MIT).
//! © Gianmarco Cherchi et al.
//! <https://github.com/gcherchi/FastAndRobustMeshArrangements>
//! <https://github.com/gcherchi/InteractiveAndRobustMeshBooleans>
//!
//! The reference `main-inputcheck.cpp` prints five verdicts per mesh:
//! Manifold, Watertight, Local Orientation (adjacent triangles traverse
//! their shared edge in OPPOSITE directions), Global Orientation (signed
//! volume), Intersection (cinolib `find_intersections` empty). This module
//! reproduces those checks natively AND reports the offending elements, so
//! a defective mesh can be traced back to its producer (the M8 Stage-0
//! emission diagnosis, spec `m8_stage0_inputcheck_clean_emission` §6).
//!
//! **`census` IS A DIAGNOSTIC ORACLE, NOT A GATE.** Do not wire the full
//! census into a production boolean path: (a) the five-axiom sweep is
//! expensive; (b) legitimately-chained INPUT operands whose collinear edge
//! chains subdivide differently (the N22 fold-sliver class) violate
//! mesh-level coverage forms on VALID data — the measured false-positive
//! population that P10-aborted the yang-rs kept-mesh gate
//! (`specs/yang_kept_mesh_manifold_gate.md` §2b). Enforcement of the
//! Stage-0 operand contract lives in dev-only tests and trackers.
//!
//! The Intersection tier alone is separately exported as
//! [`detect_improper_contacts`] for the §4.5.4 illegal-self-intersection
//! detector on boolean OUTPUT shells (task #173, spec
//! `specs/yang_173_selfx_detector.md`), where the N22 false-positive
//! legitimacy does not apply: an output shell is one arrangement-derived
//! indexed mesh whose conformality contract is index-level, so
//! index-disjoint contact is a genuine defect. That consumer gates on a
//! corpus-wide false-positive measurement recorded in its spec.
//!
//! The Intersection tier reuses [`classify_pair`] — the same exact tri-tri
//! classification the native arrangement itself runs — so "improper" here
//! means precisely "the arrangement will construct intersection structure
//! for this pair". Pairs sharing a vertex INDEX are proper adjacency and
//! are skipped; bit-identical coordinates at DISTINCT indices are reported
//! separately as vertex twins (a twin-mediated "shared" edge is improper
//! contact for the arrangement, which keys exact identity per index).

use std::collections::BTreeMap;

use cad_primitives::Point3;

use crate::arrangements::intersection_points::{classify_pair, PairClassification};
use crate::arrangements::{FastTrimesh, Plane};
use crate::predicates::points_are_collinear_3d;

/// One defective undirected edge: its vertex indices and every incident
/// triangle (soup ids).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EdgeDefect {
    pub verts: (u32, u32),
    pub tris: Vec<u32>,
}

/// Localized result of the five-axiom census. Empty defect lists (and
/// `global_orientation_ok`) ⇔ the mesh passes the reference input contract
/// as this module measures it (the sidecar binary stays the binding
/// reference; oracle-vs-oracle agreement is pinned by tests on the banked
/// operand fixtures).
#[derive(Clone, Debug, Default)]
pub struct NativeInputCheck {
    /// Triangles with a repeated vertex index (combinatorially degenerate).
    pub index_degenerate_tris: Vec<u32>,
    /// Distinct-index triangles whose corners are exactly collinear
    /// (zero-area; excluded from the Intersection tier — recorded here).
    pub collinear_degenerate_tris: Vec<u32>,
    /// Pairs of triangles over the same vertex-index triple (any order).
    pub duplicate_tris: Vec<(u32, u32)>,
    /// Distinct vertex indices carrying bit-identical coordinates.
    pub coincident_vert_twins: Vec<(u32, u32)>,
    /// Undirected edges used by exactly one triangle (open boundary).
    pub boundary_edges: Vec<EdgeDefect>,
    /// Undirected edges used by more than two triangles.
    pub nonmanifold_edges: Vec<EdgeDefect>,
    /// 2-cover edges whose two triangles traverse them in the SAME
    /// direction (a fold — the reference Local Orientation failure).
    pub misoriented_pairs: Vec<(u32, u32)>,
    /// Vertices whose incident-triangle link splits into >1 edge-connected
    /// component (bowtie/pinch — the reference Manifold failure beyond
    /// edge-level defects).
    pub nonmanifold_verts: Vec<u32>,
    /// Edge-connected triangle components.
    pub component_count: usize,
    /// Per-component signed volume, `Σ v0·(v1×v2)/6` (positive ⇔ outward
    /// CCW winding, right-handed). f64 accumulation, matching the
    /// reference's own f64 tet-volume sum.
    pub component_signed_volumes: Vec<f64>,
    /// Non-index-sharing triangle pairs the exact classification reports
    /// as intersecting/touching (transversal or coplanar contact).
    pub improper_pairs: Vec<(u32, u32)>,
    /// Pairs `classify_pair` deferred (degenerate configuration) — loud,
    /// never silently dropped.
    pub unresolved_pairs: Vec<(u32, u32)>,
    /// Vertices no triangle references. Not one of the five printed axioms —
    /// the reference binary CRASHES on them (cinolib segfault, measured on
    /// the M8 Stage-0 dropped-sliver emission), so they must be reported
    /// natively and never handed to the reference contract.
    pub unreferenced_verts: Vec<u32>,
}

impl NativeInputCheck {
    /// Reference "Manifold check": every edge ≤2-cover, every vertex link
    /// a single fan.
    pub fn manifold_ok(&self) -> bool {
        self.nonmanifold_edges.is_empty() && self.nonmanifold_verts.is_empty()
    }

    /// Reference "Watertight check": no boundary edge.
    pub fn watertight_ok(&self) -> bool {
        self.boundary_edges.is_empty()
    }

    /// Reference "Local Orientation check": every 2-cover edge traversed
    /// oppositely by its two triangles.
    pub fn local_orientation_ok(&self) -> bool {
        self.misoriented_pairs.is_empty()
    }

    /// Reference "Global Orientation check": every component's signed
    /// volume positive under the outward-CCW convention. (Sign convention
    /// pinned against the sidecar by the banked-fixture calibration test.)
    pub fn global_orientation_ok(&self) -> bool {
        self.component_signed_volumes.iter().all(|&v| v > 0.0)
    }

    /// Reference "Intersection check": no improper contact. Duplicate and
    /// degenerate triangles count as improper (they overlap something by
    /// construction); deferred pairs count as failures (loud).
    pub fn intersection_ok(&self) -> bool {
        self.improper_pairs.is_empty()
            && self.unresolved_pairs.is_empty()
            && self.duplicate_tris.is_empty()
            && self.index_degenerate_tris.is_empty()
            && self.collinear_degenerate_tris.is_empty()
    }

    /// All five axioms hold (vertex twins included: the arrangement keys
    /// exact identity per index, so twins violate the intent of the
    /// conformality contract even when geometry looks closed; unreferenced
    /// verts included: the reference binary crashes on them).
    pub fn clean(&self) -> bool {
        self.manifold_ok()
            && self.watertight_ok()
            && self.local_orientation_ok()
            && self.global_orientation_ok()
            && self.intersection_ok()
            && self.coincident_vert_twins.is_empty()
            && self.unreferenced_verts.is_empty()
    }

    /// Human-readable multi-line report (five-axiom verdicts + counts +
    /// first offenders), for the diagnosis harness output.
    pub fn summary(&self) -> String {
        fn verdict(ok: bool) -> &'static str {
            if ok {
                "passed"
            } else {
                "FAILED"
            }
        }
        fn head<T: std::fmt::Debug>(v: &[T]) -> String {
            let shown: Vec<String> = v.iter().take(6).map(|x| format!("{x:?}")).collect();
            let ell = if v.len() > 6 { ", …" } else { "" };
            format!("[{}{}]", shown.join(", "), ell)
        }
        let mut s = String::new();
        s.push_str(&format!(
            "Manifold:           {} (nonmanifold edges {} {}, verts {} {})\n",
            verdict(self.manifold_ok()),
            self.nonmanifold_edges.len(),
            head(&self.nonmanifold_edges),
            self.nonmanifold_verts.len(),
            head(&self.nonmanifold_verts),
        ));
        s.push_str(&format!(
            "Watertight:         {} (boundary edges {} {})\n",
            verdict(self.watertight_ok()),
            self.boundary_edges.len(),
            head(&self.boundary_edges),
        ));
        s.push_str(&format!(
            "Local Orientation:  {} (misoriented pairs {} {})\n",
            verdict(self.local_orientation_ok()),
            self.misoriented_pairs.len(),
            head(&self.misoriented_pairs),
        ));
        s.push_str(&format!(
            "Global Orientation: {} (components {}, volumes {:?})\n",
            verdict(self.global_orientation_ok()),
            self.component_count,
            self.component_signed_volumes,
        ));
        s.push_str(&format!(
            "Intersection:       {} (improper {} {}, unresolved {} {}, dup {} {}, degen {}+{} {})\n",
            verdict(self.intersection_ok()),
            self.improper_pairs.len(),
            head(&self.improper_pairs),
            self.unresolved_pairs.len(),
            head(&self.unresolved_pairs),
            self.duplicate_tris.len(),
            head(&self.duplicate_tris),
            self.index_degenerate_tris.len(),
            self.collinear_degenerate_tris.len(),
            head(&self.collinear_degenerate_tris),
        ));
        s.push_str(&format!(
            "Vertex twins:       {} {}\n",
            self.coincident_vert_twins.len(),
            head(&self.coincident_vert_twins),
        ));
        s.push_str(&format!(
            "Unreferenced verts: {} {}\n",
            self.unreferenced_verts.len(),
            head(&self.unreferenced_verts),
        ));
        s
    }
}

/// Result of the standalone improper-contact sweep
/// ([`detect_improper_contacts`]).
#[derive(Clone, Debug, Default)]
pub struct ImproperContacts {
    /// Non-index-sharing triangle pairs the exact classification reports
    /// as intersecting/touching (transversal or coplanar contact).
    /// Sorted ascending, original triangle ids.
    pub improper_pairs: Vec<(u32, u32)>,
    /// Pairs `classify_pair` deferred (degenerate configuration) — loud,
    /// never silently dropped. `(u32::MAX, u32::MAX)` if the soup itself
    /// could not be constructed (out-of-range index).
    pub unresolved_pairs: Vec<(u32, u32)>,
}

impl ImproperContacts {
    /// No improper or unresolved contact anywhere.
    pub fn is_clean(&self) -> bool {
        self.improper_pairs.is_empty() && self.unresolved_pairs.is_empty()
    }
}

/// Exact improper-contact sweep over an indexed triangle soup — the census
/// Intersection tier as a standalone primitive (census delegates here).
///
/// Reports every pair of triangles that share **no vertex index** yet
/// classify as non-`Disjoint` under [`classify_pair`] — the same exact
/// tri-tri classification the native arrangement runs, so "improper" means
/// precisely "the arrangement would construct intersection structure for
/// this pair". Index-degenerate and exactly-collinear triangles are
/// excluded (out of `classify_pair`'s contract; census reports them
/// separately).
///
/// **Pruning:** per-triangle AABBs, sorted by min-x with a sweep window
/// (only strictly-separated boxes are pruned — touching boxes stay
/// candidates because exact contact matters). Deterministic output order
/// (sorted pairs).
pub fn detect_improper_contacts(verts: &[Point3], tris: &[[u32; 3]]) -> ImproperContacts {
    let mut r = ImproperContacts::default();

    // Eligibility: distinct-index, non-collinear triangles.
    let mut eligible: Vec<u32> = Vec::with_capacity(tris.len());
    for (t, tri) in tris.iter().enumerate() {
        if tri[0] == tri[1] || tri[1] == tri[2] || tri[0] == tri[2] {
            continue;
        }
        if tri.iter().any(|&v| v as usize >= verts.len()) {
            // Out-of-range index: report the whole tier unresolved rather
            // than silently skipping (mirrors census's construction-failure
            // path) — but keep scanning eligibility so the count is stable.
            r.unresolved_pairs = vec![(u32::MAX, u32::MAX)];
            return r;
        }
        let (a, b, c) = (
            verts[tri[0] as usize],
            verts[tri[1] as usize],
            verts[tri[2] as usize],
        );
        if points_are_collinear_3d(a, b, c) {
            continue;
        }
        eligible.push(t as u32);
    }
    let el_tris: Vec<[u32; 3]> = eligible.iter().map(|&t| tris[t as usize]).collect();
    let soup = match FastTrimesh::from_soup(verts, &el_tris, Plane::XY) {
        Ok(s) => s,
        Err(_) => {
            if !el_tris.is_empty() {
                r.unresolved_pairs.push((u32::MAX, u32::MAX));
            }
            return r;
        }
    };

    // Per-triangle AABBs.
    let boxes: Vec<[f64; 6]> = el_tris
        .iter()
        .map(|tri| {
            let mut bb = [
                f64::INFINITY,
                f64::INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
            ];
            for &v in tri {
                let p = verts[v as usize];
                for (k, c) in [p.x(), p.y(), p.z()].into_iter().enumerate() {
                    bb[k] = bb[k].min(c);
                    bb[k + 3] = bb[k + 3].max(c);
                }
            }
            bb
        })
        .collect();

    // Sweep on min-x: after sorting, j's box can only overlap i's if
    // min_x[j] <= max_x[i]. Strictly-separated boxes never reach the
    // exact predicate; touching boxes do.
    let mut order: Vec<u32> = (0..el_tris.len() as u32).collect();
    order.sort_by(|&a, &b| {
        boxes[a as usize][0]
            .total_cmp(&boxes[b as usize][0])
            .then(a.cmp(&b))
    });

    for (si, &i) in order.iter().enumerate() {
        let ba = &boxes[i as usize];
        'pair: for &j in &order[si + 1..] {
            let bb = &boxes[j as usize];
            if bb[0] > ba[3] {
                break; // sorted by min-x: no later j can overlap i
            }
            if ba[4] < bb[1] || bb[4] < ba[1] || ba[5] < bb[2] || bb[5] < ba[2] {
                continue;
            }
            // Proper adjacency: any shared vertex INDEX.
            for &va in &el_tris[i as usize] {
                if el_tris[j as usize].contains(&va) {
                    continue 'pair;
                }
            }
            let orig = (
                eligible[i as usize].min(eligible[j as usize]),
                eligible[i as usize].max(eligible[j as usize]),
            );
            match classify_pair(&soup, i.min(j), i.max(j)) {
                PairClassification::Disjoint => {}
                PairClassification::Transversal { vertices } => {
                    if !vertices.is_empty() {
                        r.improper_pairs.push(orig);
                    }
                }
                PairClassification::Coplanar { vertices, segments } => {
                    if !vertices.is_empty() || !segments.is_empty() {
                        r.improper_pairs.push(orig);
                    }
                }
                PairClassification::Deferred(_) => r.unresolved_pairs.push(orig),
            }
        }
    }
    r.improper_pairs.sort_unstable();
    r.unresolved_pairs.sort_unstable();
    r
}

/// Run the five-axiom census over an indexed triangle soup.
pub fn census(verts: &[Point3], tris: &[[u32; 3]]) -> NativeInputCheck {
    let mut r = NativeInputCheck::default();

    // ── Tier 2a: combinatorial degeneracy + duplicates ──────────────────
    let mut by_triple: BTreeMap<[u32; 3], u32> = BTreeMap::new();
    for (t, tri) in tris.iter().enumerate() {
        let t = t as u32;
        if tri[0] == tri[1] || tri[1] == tri[2] || tri[0] == tri[2] {
            r.index_degenerate_tris.push(t);
            continue;
        }
        let mut key = *tri;
        key.sort_unstable();
        match by_triple.get(&key) {
            Some(&first) => r.duplicate_tris.push((first, t)),
            None => {
                by_triple.insert(key, t);
            }
        }
    }

    // ── Tier 2b: bit-identical vertex twins ─────────────────────────────
    let mut by_bits: BTreeMap<[u64; 3], u32> = BTreeMap::new();
    for (v, p) in verts.iter().enumerate() {
        let key = [p.x().to_bits(), p.y().to_bits(), p.z().to_bits()];
        match by_bits.get(&key) {
            Some(&first) => r.coincident_vert_twins.push((first, v as u32)),
            None => {
                by_bits.insert(key, v as u32);
            }
        }
    }

    // ── Tier 2b': unreferenced vertices (reference binary crashes) ──────
    let mut used = vec![false; verts.len()];
    for tri in tris {
        for &v in tri {
            if let Some(slot) = used.get_mut(v as usize) {
                *slot = true;
            }
        }
    }
    r.unreferenced_verts = used
        .iter()
        .enumerate()
        .filter(|(_, &u)| !u)
        .map(|(v, _)| v as u32)
        .collect();

    // ── Tier 2c: exact collinear (zero-area) triangles ──────────────────
    for (t, tri) in tris.iter().enumerate() {
        if tri[0] == tri[1] || tri[1] == tri[2] || tri[0] == tri[2] {
            continue; // already index-degenerate
        }
        let (a, b, c) = (
            verts[tri[0] as usize],
            verts[tri[1] as usize],
            verts[tri[2] as usize],
        );
        if points_are_collinear_3d(a, b, c) {
            r.collinear_degenerate_tris.push(t as u32);
        }
    }

    // ── Tier 1: undirected edge map with traversal direction ────────────
    // (index-degenerate triangles excluded — their edge multiset is
    // ill-formed; they are already reported above.)
    #[derive(Default)]
    struct EdgeUse {
        tris: Vec<u32>,
        forward: u32,
        backward: u32,
    }
    let mut edges: BTreeMap<(u32, u32), EdgeUse> = BTreeMap::new();
    for (t, tri) in tris.iter().enumerate() {
        if r.index_degenerate_tris.contains(&(t as u32)) {
            continue;
        }
        for k in 0..3 {
            let (u, v) = (tri[k], tri[(k + 1) % 3]);
            let key = (u.min(v), u.max(v));
            let e = edges.entry(key).or_default();
            e.tris.push(t as u32);
            if u < v {
                e.forward += 1;
            } else {
                e.backward += 1;
            }
        }
    }
    for (&verts_key, e) in &edges {
        match e.tris.len() {
            1 => r.boundary_edges.push(EdgeDefect {
                verts: verts_key,
                tris: e.tris.clone(),
            }),
            2 => {
                if e.forward != 1 {
                    // Both traverse the edge the same way: a fold.
                    r.misoriented_pairs.push((e.tris[0], e.tris[1]));
                }
            }
            _ => r.nonmanifold_edges.push(EdgeDefect {
                verts: verts_key,
                tris: e.tris.clone(),
            }),
        }
    }

    // ── Tier 1b: vertex-link manifoldness (bowtie/pinch) ────────────────
    // For each vertex: incident triangles must form ONE component under
    // adjacency across incident edges.
    let mut incident: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for (t, tri) in tris.iter().enumerate() {
        if r.index_degenerate_tris.contains(&(t as u32)) {
            continue;
        }
        for &v in tri {
            incident.entry(v).or_default().push(t as u32);
        }
    }
    for (&v, inc) in &incident {
        if inc.len() < 2 {
            continue;
        }
        // Union-find over the incident triangles, joined when they share
        // an edge through `v`.
        let mut parent: Vec<usize> = (0..inc.len()).collect();
        fn find(parent: &mut Vec<usize>, i: usize) -> usize {
            if parent[i] != i {
                let root = find(parent, parent[i]);
                parent[i] = root;
            }
            parent[i]
        }
        let mut other_end: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
        for (i, &t) in inc.iter().enumerate() {
            let tri = tris[t as usize];
            for k in 0..3 {
                let (a, b) = (tri[k], tri[(k + 1) % 3]);
                let w = if a == v {
                    b
                } else if b == v {
                    a
                } else {
                    continue;
                };
                other_end.entry(w).or_default().push(i);
            }
        }
        for group in other_end.values() {
            for pair in group.windows(2) {
                let (ra, rb) = (find(&mut parent, pair[0]), find(&mut parent, pair[1]));
                if ra != rb {
                    parent[ra] = rb;
                }
            }
        }
        let mut roots: Vec<usize> = (0..inc.len()).map(|i| find(&mut parent, i)).collect();
        roots.sort_unstable();
        roots.dedup();
        if roots.len() > 1 {
            r.nonmanifold_verts.push(v);
        }
    }

    // ── Components + per-component signed volume ────────────────────────
    let live: Vec<u32> = (0..tris.len() as u32)
        .filter(|t| !r.index_degenerate_tris.contains(t))
        .collect();
    let index_of: BTreeMap<u32, usize> = live.iter().enumerate().map(|(i, &t)| (t, i)).collect();
    let mut parent: Vec<usize> = (0..live.len()).collect();
    fn find(parent: &mut Vec<usize>, i: usize) -> usize {
        if parent[i] != i {
            let root = find(parent, parent[i]);
            parent[i] = root;
        }
        parent[i]
    }
    for e in edges.values() {
        for pair in e.tris.windows(2) {
            let (ia, ib) = (index_of[&pair[0]], index_of[&pair[1]]);
            let (ra, rb) = (find(&mut parent, ia), find(&mut parent, ib));
            if ra != rb {
                parent[ra] = rb;
            }
        }
    }
    let mut vol_by_root: BTreeMap<usize, f64> = BTreeMap::new();
    for (i, &t) in live.iter().enumerate() {
        let tri = tris[t as usize];
        let (a, b, c) = (
            verts[tri[0] as usize],
            verts[tri[1] as usize],
            verts[tri[2] as usize],
        );
        // Tet volume against the origin: a·(b×c)/6.
        let cross = [
            b.y() * c.z() - b.z() * c.y(),
            b.z() * c.x() - b.x() * c.z(),
            b.x() * c.y() - b.y() * c.x(),
        ];
        let tet = (a.x() * cross[0] + a.y() * cross[1] + a.z() * cross[2]) / 6.0;
        let root = find(&mut parent, i);
        *vol_by_root.entry(root).or_insert(0.0) += tet;
    }
    r.component_count = vol_by_root.len();
    r.component_signed_volumes = vol_by_root.into_values().collect();

    // ── Tier 3: exact improper-intersection sweep ───────────────────────
    // Delegates to the standalone primitive (identical eligibility: it
    // excludes index-degenerate and exactly-collinear triangles itself).
    let contacts = detect_improper_contacts(verts, tris);
    r.improper_pairs = contacts.improper_pairs;
    r.unresolved_pairs = contacts.unresolved_pairs;

    r
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f64, y: f64, z: f64) -> Point3 {
        Point3::new(x, y, z)
    }

    /// Closed outward-wound tetrahedron: every axiom passes.
    fn tet() -> (Vec<Point3>, Vec<[u32; 3]>) {
        let verts = vec![
            p(0.0, 0.0, 0.0),
            p(1.0, 0.0, 0.0),
            p(0.0, 1.0, 0.0),
            p(0.0, 0.0, 1.0),
        ];
        let tris = vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]];
        (verts, tris)
    }

    #[test]
    fn closed_tet_is_clean() {
        let (v, t) = tet();
        let c = census(&v, &t);
        assert!(c.clean(), "expected clean, got:\n{}", c.summary());
        assert_eq!(c.component_count, 1);
        assert!((c.component_signed_volumes[0] - 1.0 / 6.0).abs() < 1e-15);
    }

    #[test]
    fn missing_face_fails_watertight_only_at_the_hole() {
        let (v, mut t) = tet();
        t.pop(); // drop [1,2,3]
        let c = census(&v, &t);
        assert!(!c.watertight_ok());
        assert_eq!(c.boundary_edges.len(), 3);
        assert!(c.local_orientation_ok());
        assert!(c.manifold_ok());
    }

    #[test]
    fn flipped_face_fails_local_orientation() {
        let (v, mut t) = tet();
        t[3] = [2, 1, 3]; // reverse the slanted face
        let c = census(&v, &t);
        assert!(!c.local_orientation_ok());
        assert_eq!(c.misoriented_pairs.len(), 3);
        assert!(c.watertight_ok());
    }

    #[test]
    fn inverted_solid_fails_global_orientation() {
        let (v, t) = tet();
        let flipped: Vec<[u32; 3]> = t.iter().map(|&[a, b, c]| [a, c, b]).collect();
        let c = census(&v, &flipped);
        assert!(!c.global_orientation_ok());
        assert!(c.watertight_ok() && c.local_orientation_ok());
    }

    #[test]
    fn piercing_triangle_is_improper() {
        let (mut v, mut t) = tet();
        // A free triangle stabbing through the slanted face region.
        let base = v.len() as u32;
        v.extend([p(0.5, 0.5, -0.5), p(0.5, 0.5, 1.0), p(0.6, 0.4, -0.5)]);
        t.push([base, base + 1, base + 2]);
        let c = census(&v, &t);
        assert!(
            !c.improper_pairs.is_empty(),
            "expected improper pairs:\n{}",
            c.summary()
        );
    }

    #[test]
    fn coplanar_disjoint_with_overlapping_boxes_is_ok() {
        // Two coplanar z=0 triangles whose AABBs overlap but whose point
        // sets are disjoint — pins Coplanar{empty} ⇒ no defect.
        let v = vec![
            p(0.0, 0.0, 0.0),
            p(2.0, 0.0, 0.0),
            p(0.0, 2.0, 0.0),
            p(2.0, 2.0, 0.0),
            p(1.6, 2.0, 0.0),
            p(2.0, 1.6, 0.0),
        ];
        let t = vec![[0, 1, 2], [3, 4, 5]];
        let c = census(&v, &t);
        assert!(c.improper_pairs.is_empty(), "got:\n{}", c.summary());
        assert!(c.unresolved_pairs.is_empty());
    }

    #[test]
    fn coplanar_overlapping_pair_is_improper() {
        // Same plane, genuinely overlapping interiors, no shared indices.
        let v = vec![
            p(0.0, 0.0, 0.0),
            p(2.0, 0.0, 0.0),
            p(0.0, 2.0, 0.0),
            p(0.5, 0.5, 0.0),
            p(2.5, 0.5, 0.0),
            p(0.5, 2.5, 0.0),
        ];
        let t = vec![[0, 1, 2], [3, 4, 5]];
        let c = census(&v, &t);
        assert!(!c.improper_pairs.is_empty(), "got:\n{}", c.summary());
    }

    #[test]
    fn duplicate_triple_and_vertex_twin_reported() {
        let (mut v, mut t) = tet();
        t.push([1, 3, 0]); // same triple as [0,1,3], rotated
        v.push(p(0.0, 0.0, 0.0)); // bit-identical twin of vert 0
        let c = census(&v, &t);
        assert_eq!(c.duplicate_tris, vec![(1, 4)]);
        assert_eq!(c.coincident_vert_twins, vec![(0, 4)]);
        assert!(!c.intersection_ok());
    }

    #[test]
    fn unreferenced_vertex_reported() {
        let (mut v, t) = tet();
        v.push(p(9.0, 9.0, 9.0));
        let c = census(&v, &t);
        assert_eq!(c.unreferenced_verts, vec![4]);
        assert!(!c.clean());
    }

    #[test]
    fn bowtie_vertex_is_nonmanifold() {
        // Two fans sharing only vertex 0.
        let v = vec![
            p(0.0, 0.0, 0.0),
            p(1.0, 0.0, 0.0),
            p(1.0, 1.0, 0.0),
            p(-1.0, 0.0, 0.0),
            p(-1.0, -1.0, 0.0),
        ];
        let t = vec![[0, 1, 2], [0, 3, 4]];
        let c = census(&v, &t);
        assert_eq!(c.nonmanifold_verts, vec![0]);
    }

    // ── detect_improper_contacts (standalone primitive, #173) ───────────

    #[test]
    fn contacts_clean_tet_is_clean() {
        let (v, t) = tet();
        let c = detect_improper_contacts(&v, &t);
        assert!(c.is_clean(), "got: {c:?}");
    }

    #[test]
    fn contacts_piercing_pair_flagged() {
        let (mut v, mut t) = tet();
        let base = v.len() as u32;
        v.extend([p(0.5, 0.5, -0.5), p(0.5, 0.5, 1.0), p(0.6, 0.4, -0.5)]);
        t.push([base, base + 1, base + 2]);
        let c = detect_improper_contacts(&v, &t);
        assert!(!c.improper_pairs.is_empty(), "got: {c:?}");
        assert!(c.unresolved_pairs.is_empty());
    }

    #[test]
    fn contacts_coplanar_overlap_flagged() {
        let v = vec![
            p(0.0, 0.0, 0.0),
            p(2.0, 0.0, 0.0),
            p(0.0, 2.0, 0.0),
            p(0.5, 0.5, 0.0),
            p(2.5, 0.5, 0.0),
            p(0.5, 2.5, 0.0),
        ];
        let t = vec![[0, 1, 2], [3, 4, 5]];
        let c = detect_improper_contacts(&v, &t);
        assert_eq!(c.improper_pairs, vec![(0, 1)]);
    }

    #[test]
    fn contacts_index_shared_adjacency_skipped() {
        // Two triangles sharing an edge BY INDEX: proper adjacency.
        let v = vec![
            p(0.0, 0.0, 0.0),
            p(1.0, 0.0, 0.0),
            p(0.0, 1.0, 0.0),
            p(1.0, 1.0, 0.0),
        ];
        let t = vec![[0, 1, 2], [1, 3, 2]];
        let c = detect_improper_contacts(&v, &t);
        assert!(c.is_clean(), "got: {c:?}");
    }

    #[test]
    fn contacts_twin_mediated_edge_flagged() {
        // Same two triangles, but the second references bit-identical TWIN
        // vertices instead of shared indices — improper for the arrangement
        // (keys exact identity per index).
        let v = vec![
            p(0.0, 0.0, 0.0),
            p(1.0, 0.0, 0.0),
            p(0.0, 1.0, 0.0),
            p(1.0, 1.0, 0.0),
            p(1.0, 0.0, 0.0), // twin of 1
            p(0.0, 1.0, 0.0), // twin of 2
        ];
        let t = vec![[0, 1, 2], [4, 3, 5]];
        let c = detect_improper_contacts(&v, &t);
        assert_eq!(c.improper_pairs, vec![(0, 1)]);
    }

    #[test]
    fn contacts_degenerates_excluded_not_crashing() {
        let v = vec![
            p(0.0, 0.0, 0.0),
            p(1.0, 0.0, 0.0),
            p(2.0, 0.0, 0.0),
            p(0.0, 1.0, 0.0),
        ];
        let t = vec![[0, 1, 1], [0, 1, 2], [0, 1, 3]];
        let c = detect_improper_contacts(&v, &t);
        assert!(c.is_clean(), "degenerates are census's report, got: {c:?}");
    }

    #[test]
    fn contacts_out_of_range_index_is_unresolved_loudly() {
        let v = vec![p(0.0, 0.0, 0.0), p(1.0, 0.0, 0.0), p(0.0, 1.0, 0.0)];
        let t = vec![[0, 1, 9]];
        let c = detect_improper_contacts(&v, &t);
        assert_eq!(c.unresolved_pairs, vec![(u32::MAX, u32::MAX)]);
    }

    /// Sweep pruning ≡ full double loop: same pairs on a mixed scene
    /// (piercing + coplanar overlap + adjacency + far-apart tris) and
    /// agreement with census's tier on every census fixture above.
    #[test]
    fn contacts_sweep_matches_census_tier() {
        let scenes: Vec<(Vec<Point3>, Vec<[u32; 3]>)> = vec![
            tet(),
            {
                let (mut v, mut t) = tet();
                let base = v.len() as u32;
                v.extend([p(0.5, 0.5, -0.5), p(0.5, 0.5, 1.0), p(0.6, 0.4, -0.5)]);
                t.push([base, base + 1, base + 2]);
                (v, t)
            },
            (
                vec![
                    p(0.0, 0.0, 0.0),
                    p(2.0, 0.0, 0.0),
                    p(0.0, 2.0, 0.0),
                    p(0.5, 0.5, 0.0),
                    p(2.5, 0.5, 0.0),
                    p(0.5, 2.5, 0.0),
                    p(10.0, 0.0, 0.0),
                    p(11.0, 0.0, 0.0),
                    p(10.0, 1.0, 0.0),
                ],
                vec![[0, 1, 2], [3, 4, 5], [6, 7, 8]],
            ),
        ];
        for (v, t) in &scenes {
            let c = detect_improper_contacts(v, t);
            let cen = census(v, t);
            assert_eq!(c.improper_pairs, cen.improper_pairs);
            assert_eq!(c.unresolved_pairs, cen.unresolved_pairs);
        }
    }

    #[test]
    fn index_degenerate_and_collinear_reported_not_crashing() {
        let v = vec![
            p(0.0, 0.0, 0.0),
            p(1.0, 0.0, 0.0),
            p(2.0, 0.0, 0.0),
            p(0.0, 1.0, 0.0),
        ];
        let t = vec![[0, 1, 1], [0, 1, 2], [0, 1, 3]];
        let c = census(&v, &t);
        assert_eq!(c.index_degenerate_tris, vec![0]);
        assert_eq!(c.collinear_degenerate_tris, vec![1]);
        assert!(!c.intersection_ok());
    }
}
