//! Yang 2025 §4.5.3 (`refs/text/yang2025_hybrid_boolean.txt:679-689, 718-751`,
//! Fig. 15): the reversal correction walks "the intersection curve C between
//! the two surfaces S_A and S_B" point by point — p_b, p_r, p_n.
//!
//! This module reads each such curve C from the mesh as what it is there: the
//! CHAIN of intersection edges one face of A and one face of B share. An
//! intersection edge is a mesh edge with a triangle of each operand on it, so
//! the reading needs the triangles' face attribution and nothing else — no
//! patch, no patch boundary walk, no surface type.
//!
//! The §4.5.3 sweep (`stage4_correct::reversal::sweep_reversed_intersections`)
//! does not read its curves this way today: it walks every patch's boundary
//! cycle that holds an intersection edge, after re-deriving all patches on
//! each pass (ledger N81). This module only READS the chains and, under
//! `YANG_CHAIN_CENSUS`, prints how the two readings compare; it does not
//! change the mesh. Spec `specs/yang_f2_fig11b_cdt_input_merge.md` §8.8.

use crate::brep::InputId;
use crate::{Mesh, TriangleAttribution};
use std::collections::{BTreeMap, BTreeSet};

/// The two faces of one curve C: `(face of A, face of B)`.
pub(crate) type FacePair = (u32, u32);

/// An undirected mesh edge, lower vertex first.
pub(crate) type EdgeKey = (u32, u32);

fn key(u: u32, v: u32) -> EdgeKey {
    if u < v {
        (u, v)
    } else {
        (v, u)
    }
}

/// Every intersection edge of the mesh with the face pairs it lies on.
///
/// An edge with triangles of faces `f1, f2` of A and `g` of B on it — an edge
/// of A lying in a face of B — is an edge of both curves `(f1, g)` and
/// `(f2, g)`.
#[derive(Debug, Default)]
pub(crate) struct IntersectionEdges {
    pub(crate) pairs: BTreeMap<EdgeKey, BTreeSet<FacePair>>,
    /// Per face pair, how many of its intersection edges meet at each vertex.
    degree: BTreeMap<(FacePair, u32), u32>,
}

/// How the three consecutive points `p_b, p_r, p_n` of a walk sit on the
/// chains.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum SiteKind {
    /// `p_b–p_r` and `p_r–p_n` are consecutive edges of one curve C, and no
    /// third edge of C meets them at `p_r`: `p_r` is a point of C between its
    /// endpoints.
    Interior,
    /// Both are edges of one curve C, and C has more than two edges at `p_r`.
    Branch,
    /// Both are intersection edges and no curve C holds both: `p_r` is where
    /// one curve ends and another begins.
    Straddle,
    /// One of the two is an intersection edge.
    OneEdge,
    /// Neither is an intersection edge.
    Off,
}

impl SiteKind {
    pub(crate) fn tag(self) -> &'static str {
        match self {
            SiteKind::Interior => "interior",
            SiteKind::Branch => "branch",
            SiteKind::Straddle => "straddle",
            SiteKind::OneEdge => "one-edge",
            SiteKind::Off => "off",
        }
    }
}

impl IntersectionEdges {
    pub(crate) fn read(mesh: &Mesh, attribution: &[Option<TriangleAttribution>]) -> Self {
        let mut faces: BTreeMap<EdgeKey, (BTreeSet<u32>, BTreeSet<u32>)> = BTreeMap::new();
        for (ti, t) in mesh.tris.iter().enumerate() {
            let Some(at) = attribution.get(ti).copied().flatten() else {
                continue;
            };
            for k in 0..3 {
                let (u, v) = (t[k], t[(k + 1) % 3]);
                if u == v {
                    continue;
                }
                let slot = faces.entry(key(u, v)).or_default();
                match at.input {
                    InputId::A => slot.0.insert(at.face),
                    InputId::B => slot.1.insert(at.face),
                };
            }
        }
        let mut pairs: BTreeMap<EdgeKey, BTreeSet<FacePair>> = BTreeMap::new();
        let mut degree: BTreeMap<(FacePair, u32), u32> = BTreeMap::new();
        for (edge, (fa, fb)) in faces {
            if fa.is_empty() || fb.is_empty() {
                continue;
            }
            let set: BTreeSet<FacePair> = fa
                .iter()
                .flat_map(|&f| fb.iter().map(move |&g| (f, g)))
                .collect();
            for &pair in &set {
                *degree.entry((pair, edge.0)).or_default() += 1;
                *degree.entry((pair, edge.1)).or_default() += 1;
            }
            pairs.insert(edge, set);
        }
        IntersectionEdges { pairs, degree }
    }

    pub(crate) fn is_intersection_edge(&self, u: u32, v: u32) -> bool {
        self.pairs.contains_key(&key(u, v))
    }

    /// Where `p_r` sits on the chains, read with its two neighbours.
    pub(crate) fn site(&self, p_b: u32, p_r: u32, p_n: u32) -> SiteKind {
        let (eb, en) = (
            self.pairs.get(&key(p_b, p_r)),
            self.pairs.get(&key(p_r, p_n)),
        );
        let (Some(eb), Some(en)) = (eb, en) else {
            return if eb.is_some() || en.is_some() {
                SiteKind::OneEdge
            } else {
                SiteKind::Off
            };
        };
        let mut shared = eb.intersection(en).peekable();
        if shared.peek().is_none() {
            return SiteKind::Straddle;
        }
        if shared.any(|&pair| self.degree.get(&(pair, p_r)) == Some(&2)) {
            SiteKind::Interior
        } else {
            SiteKind::Branch
        }
    }

    /// The curves C, each as its ordered chain of vertices.
    ///
    /// A chain runs between two vertices where its face pair has one edge or
    /// more than two (its endpoints) through vertices where it has exactly
    /// two; a pair's edges that meet no such vertex form a closed ring. The
    /// order is fixed by vertex index alone: face pairs ascending, open
    /// chains from their lowest endpoint, rings from their lowest vertex
    /// toward its lower neighbour.
    pub(crate) fn chains(&self) -> Vec<Chain> {
        let mut by_pair: BTreeMap<FacePair, BTreeMap<u32, BTreeSet<u32>>> = BTreeMap::new();
        for (&(u, v), set) in &self.pairs {
            for &pair in set {
                let adj = by_pair.entry(pair).or_default();
                adj.entry(u).or_default().insert(v);
                adj.entry(v).or_default().insert(u);
            }
        }
        let mut out = Vec::new();
        for (faces, adj) in by_pair {
            let mut used: BTreeSet<EdgeKey> = BTreeSet::new();
            let walk = |start: u32, first: u32, used: &mut BTreeSet<EdgeKey>| -> Vec<u32> {
                let mut verts = vec![start, first];
                used.insert(key(start, first));
                loop {
                    let at = verts[verts.len() - 1];
                    if at == start || adj[&at].len() != 2 {
                        return verts;
                    }
                    let Some(&next) = adj[&at].iter().find(|&&w| !used.contains(&key(at, w)))
                    else {
                        return verts;
                    };
                    used.insert(key(at, next));
                    verts.push(next);
                }
            };
            for (&v, nbrs) in &adj {
                if nbrs.len() == 2 {
                    continue;
                }
                for &w in nbrs {
                    if used.contains(&key(v, w)) {
                        continue;
                    }
                    let verts = walk(v, w, &mut used);
                    out.push(Chain {
                        faces,
                        verts,
                        closed: false,
                    });
                }
            }
            for (&v, nbrs) in &adj {
                let Some(&w) = nbrs.iter().find(|&&w| !used.contains(&key(v, w))) else {
                    continue;
                };
                let mut verts = walk(v, w, &mut used);
                // A ring comes back to its start; the start is listed once.
                if verts.len() > 1 && verts[verts.len() - 1] == v {
                    verts.pop();
                }
                out.push(Chain {
                    faces,
                    verts,
                    closed: true,
                });
            }
        }
        out
    }
}

/// One curve C between a face of A and a face of B, as its ordered chain of
/// mesh vertices. An open chain lists both endpoints; a closed one lists each
/// vertex once and its last vertex joins its first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Chain {
    pub(crate) faces: FacePair,
    pub(crate) verts: Vec<u32>,
    pub(crate) closed: bool,
}

/// One line of the `YANG_CHAIN_CENSUS=<file>` census, tagged with
/// `ASSAY_CASE`. One `write` per line, so parallel children's appends do not
/// interleave.
pub(crate) fn log(line: &str) {
    let Some(path) = std::env::var_os("YANG_CHAIN_CENSUS") else {
        return;
    };
    let case = std::env::var("ASSAY_CASE").unwrap_or_else(|_| "-".into());
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        use std::io::Write;
        let _ = f.write_all(format!("{case}\t{line}\n").as_bytes());
    }
}

pub(crate) fn census_on() -> bool {
    std::env::var_os("YANG_CHAIN_CENSUS").is_some()
}

/// What the §4.5.3 sweep's loops and the chains each read at one pass, for
/// the census. `loop_cross` are the sweep's own intersection edges: the
/// edges of its loops that carry a surface of each operand in Phase A's
/// incidence.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EdgeAgreement {
    pub(crate) loop_cross: usize,
    pub(crate) chain_edges: usize,
    pub(crate) only_loop: usize,
    pub(crate) only_chain: usize,
}

pub(crate) fn edge_agreement(
    edges: &IntersectionEdges,
    loop_cross: &BTreeSet<EdgeKey>,
) -> EdgeAgreement {
    EdgeAgreement {
        loop_cross: loop_cross.len(),
        chain_edges: edges.pairs.len(),
        only_loop: loop_cross
            .iter()
            .filter(|e| !edges.pairs.contains_key(e))
            .count(),
        only_chain: edges
            .pairs
            .keys()
            .filter(|e| !loop_cross.contains(e))
            .count(),
    }
}

/// The face of every attributed triangle on the edge `key`, as `A12` / `B3`,
/// one entry per triangle (census detail).
pub(crate) fn faces_on_edge(
    mesh: &Mesh,
    attribution: &[Option<TriangleAttribution>],
    edge: EdgeKey,
) -> Vec<String> {
    let mut out = Vec::new();
    for (ti, t) in mesh.tris.iter().enumerate() {
        if !(t.contains(&edge.0) && t.contains(&edge.1)) {
            continue;
        }
        out.push(match attribution.get(ti).copied().flatten() {
            Some(at) => format!("{:?}{}", at.input, at.face),
            None => "none".to_string(),
        });
    }
    out.sort();
    out
}

/// The variant name of a `Debug` value: its text up to the first space,
/// brace or parenthesis (census detail; no surface type is matched).
pub(crate) fn variant_name(value: &impl std::fmt::Debug) -> String {
    let text = format!("{value:?}");
    text.split([' ', '{', '('])
        .next()
        .unwrap_or_default()
        .to_string()
}
