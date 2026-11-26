//! Yang 2025 §4.4.1 Fig. 11(b) (`refs/text/yang2025_hybrid_boolean.txt:562-563`):
//! "If an endpoint p of the split edge is too close to q, we merge p with q."
//!
//! The merge acts on the update's CDT INPUT: p and q are points of the
//! constrained boundary, and the triangulation is built once they are one
//! point. So the merge is decided per CLASS of vertices (the connected
//! components of the eligible pairs), which no order of reading the pairs can
//! change — unlike the one-pair-at-a-time index rewrite it replaces (ledger
//! N55 amendment 2026-11-25: that order decides F0060).
//! Spec `specs/yang_f2_fig11b_cdt_input_merge.md` §3.
//!
//! "Too close" is the owner's ruling of 2026-11-25 (ledger N89 item 7): the
//! same point within roundoff, `TAU_WORK·(1 + scale)`
//! ([`crate::stage4_correct::is_relocation_coincidence`]).
//!
//! This module only DECIDES the classes; it does not change the mesh.
#![cfg_attr(not(test), allow(dead_code))]

use crate::brep::InputId;
use crate::param_surface::carried::{FaceRef, VertexParamMap};
use crate::{Mesh, TriangleAttribution};
use std::collections::{BTreeMap, BTreeSet};

/// One merge class: vertices that are one point.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Fig11bClass {
    /// Members, ascending index (the survivor among them).
    pub members: Vec<u32>,
    /// The vertex every member becomes: an intersection point.
    pub survivor: u32,
    /// Largest distance between two members.
    pub diameter: f64,
}

/// The classes of one mesh.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Fig11bClasses {
    /// Classes that merge, ordered by their least member.
    pub classes: Vec<Fig11bClass>,
    /// Components whose diameter exceeds the band: a chain of close
    /// vertices, not one point. Left alone (spec §3.2).
    pub too_wide: Vec<Fig11bClass>,
}

impl Fig11bClasses {
    /// `victim -> survivor` for every merging member other than its survivor.
    pub(crate) fn survivor_map(&self) -> BTreeMap<u32, u32> {
        self.classes
            .iter()
            .flat_map(|c| {
                c.members
                    .iter()
                    .filter(move |&&m| m != c.survivor)
                    .map(move |&m| (m, c.survivor))
            })
            .collect()
    }
}

/// Which operands' meshes each vertex lies on: bit 0 = A, bit 1 = B.
///
/// Read from the arrangement's record — the faces a vertex carries
/// parameters on (§4.2.3 `:510-519`) — and from its triangle star, for a
/// mesh that carries no parameters. The star alone is not enough: where the
/// operation keeps one operand's face whole, the star of a point of both
/// meshes holds triangles of one.
pub(crate) fn operand_bits(
    mesh: &Mesh,
    attribution: &[Option<TriangleAttribution>],
    vertex_params: Option<&VertexParamMap>,
) -> Vec<u8> {
    let bit_of = |input: InputId| -> u8 {
        match input {
            InputId::A => 1,
            InputId::B => 2,
        }
    };
    let mut bits = vec![0u8; mesh.verts.len()];
    if let Some(vp) = vertex_params {
        for (v, entries) in vp.params.iter().enumerate().take(bits.len()) {
            for e in entries {
                bits[v] |= bit_of(e.face.input);
            }
        }
    }
    for (tri, at) in mesh.tris.iter().zip(attribution.iter()) {
        let Some(at) = at else { continue };
        for &v in tri {
            bits[v as usize] |= bit_of(at.input);
        }
    }
    bits
}

fn dist(mesh: &Mesh, u: u32, v: u32) -> (f64, f64) {
    let p = mesh.verts[u as usize].as_array();
    let q = mesh.verts[v as usize].as_array();
    let d = ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt();
    let scale = p
        .iter()
        .chain(q.iter())
        .fold(0.0f64, |m, &c| m.max(c.abs()));
    (d, scale)
}

/// Two indices at ONE position are one point too: the N55 scan merges them
/// (11 of its pairs over the corpus and the sweep are exact duplicates,
/// census 2026-11-25), and §4.4.1 sets `r_A = r_B = r`.
fn same_point(mesh: &Mesh, u: u32, v: u32) -> bool {
    let (d, scale) = dist(mesh, u, v);
    crate::stage4_correct::is_relocation_coincidence(d, scale)
}

/// The Fig. 11(b) merge classes of `mesh` (spec §3.1–§3.2).
///
/// A pair is eligible when a mesh edge joins it, at least one end is an
/// intersection point (on both operands' meshes, [`operand_bits`]), and the
/// two are the same point within the band. Two vertices of ONE operand's mesh
/// are never eligible: the paper's update never merges two original mesh
/// vertices (that population is a Stage-1 defect, F5b).
///
/// The survivor of a class is one of its intersection points: the highest
/// `rank` (the caller's exactness rank: a closed-form junction outranks a
/// conic endpoint, which outranks any other), then the lexicographically
/// least position, then the lowest index. Each is a function of the class, so neither the partition nor the
/// survivor's position depends on the order the edges are read in.
pub(crate) fn fig11b_vertex_classes(
    mesh: &Mesh,
    attribution: &[Option<TriangleAttribution>],
    vertex_params: Option<&VertexParamMap>,
    rank: &dyn Fn(u32) -> u8,
) -> Fig11bClasses {
    let bits = operand_bits(mesh, attribution, vertex_params);
    let is_isect = |v: u32| bits[v as usize] == 3;

    let n = mesh.verts.len();
    let mut parent: Vec<u32> = (0..n as u32).collect();
    fn find(parent: &mut [u32], mut x: u32) -> u32 {
        while parent[x as usize] != x {
            let up = parent[parent[x as usize] as usize];
            parent[x as usize] = up;
            x = up;
        }
        x
    }
    let mut touched = false;
    for tri in &mesh.tris {
        for k in 0..3 {
            let (u, v) = (tri[k], tri[(k + 1) % 3]);
            if u == v || !(is_isect(u) || is_isect(v)) || !same_point(mesh, u, v) {
                continue;
            }
            let (ru, rv) = (find(&mut parent, u), find(&mut parent, v));
            if ru != rv {
                // Union by least root: any rule works, the partition is the same.
                let (lo, hi) = (ru.min(rv), ru.max(rv));
                parent[hi as usize] = lo;
                touched = true;
            }
        }
    }
    if !touched {
        return Fig11bClasses::default();
    }
    let mut comps: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for v in 0..n as u32 {
        let r = find(&mut parent, v);
        comps.entry(r).or_default().push(v);
    }
    let mut out = Fig11bClasses::default();
    for (_, mut members) in comps {
        if members.len() < 2 {
            continue;
        }
        members.sort_unstable();
        let mut diameter = 0.0f64;
        for (i, &u) in members.iter().enumerate() {
            for &v in &members[i + 1..] {
                diameter = diameter.max(dist(mesh, u, v).0);
            }
        }
        let survivor = members
            .iter()
            .copied()
            .filter(|&v| is_isect(v))
            .max_by(|&x, &y| {
                let px = mesh.verts[x as usize].as_array();
                let py = mesh.verts[y as usize].as_array();
                rank(x)
                    .cmp(&rank(y))
                    // max_by keeps the GREATEST: reverse so the least position,
                    // then the least index, wins.
                    .then_with(|| {
                        py[0]
                            .total_cmp(&px[0])
                            .then(py[1].total_cmp(&px[1]))
                            .then(py[2].total_cmp(&px[2]))
                    })
                    .then_with(|| y.cmp(&x))
            })
            .expect("an eligible pair has an intersection point");
        // The band of the class: its widest pair must itself be one point.
        let wide = members.iter().enumerate().any(|(i, &u)| {
            members[i + 1..].iter().any(|&v| {
                let (d, scale) = dist(mesh, u, v);
                !crate::stage4_correct::is_relocation_coincidence(d, scale)
            })
        });
        let class = Fig11bClass {
            members,
            survivor,
            diameter,
        };
        if wide {
            out.too_wide.push(class);
        } else {
            out.classes.push(class);
        }
    }
    out
}

/// Which statement of the paper a close pair falls under, read from the
/// faces each of its two points lies on ([`VertexParamMap`], §4.2.3). The
/// classes above are one union-find over all of them; the paper has a
/// different rule for each (spec §8).
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum PairKind {
    /// Fig. 11(b) (`:562-563`): p is a vertex of one operand's mesh on a
    /// boundary curve of that operand (two or more of its faces), and q is an
    /// intersection point on that curve — "an endpoint p of the split edge".
    BoundaryEndpoint,
    /// §4.4.1 (`:563-564`): p is a vertex of one operand's mesh inside a
    /// face, and q an intersection point on that face — "we remove a mesh
    /// vertex if it is too close to the intersection curve".
    NearCurveInterior,
    /// §4.3.3 (`:535-536`): two intersection points on the same faces —
    /// "we remove a point if it is too close to another point on the same
    /// loop".
    SameCurve,
    /// §4.3.3, at the curve's end: two intersection points, one lying on
    /// every face of the other and on more (where the curve meets a
    /// boundary curve or another intersection curve).
    CurveEnd,
    /// Each point is recorded on a face the other is not. Within the
    /// roundoff band every measured pair (9 over the rotation sweep, census
    /// 2026-11-26) is the end of two curves at one corner of both operands —
    /// A's rim on a face of B beside B's rim on a face of A — which the paper
    /// holds as one intersection point (`r_A = r_B = r`, `:550-554`); neither
    /// §4.3.3 nor Fig. 11 names the pair. A wider band would make this two
    /// points of the model (the containment rule of the §4.3 weld, §4-I8).
    DifferentFaces,
    /// A point with no recorded face, or two points of one operand's mesh.
    Unread,
}

impl PairKind {
    pub(crate) fn tag(self) -> &'static str {
        match self {
            PairKind::BoundaryEndpoint => "fig11b",
            PairKind::NearCurveInterior => "near-curve",
            PairKind::SameCurve => "same-curve",
            PairKind::CurveEnd => "curve-end",
            PairKind::DifferentFaces => "different-faces",
            PairKind::Unread => "unread",
        }
    }
}

/// The faces vertex `v` carries parameters on.
pub(crate) fn faces_of(vertex_params: Option<&VertexParamMap>, v: u32) -> BTreeSet<FaceRef> {
    vertex_params
        .and_then(|vp| vp.params.get(v as usize))
        .map(|entries| entries.iter().map(|e| e.face).collect())
        .unwrap_or_default()
}

/// The [`PairKind`] of two points lying on the faces `fu` and `fv`. No
/// surface type and no curve type is read: only which faces, of which
/// operand, each point lies on.
pub(crate) fn pair_kind(fu: &BTreeSet<FaceRef>, fv: &BTreeSet<FaceRef>) -> PairKind {
    let on_both = |f: &BTreeSet<FaceRef>| {
        f.iter().any(|x| x.input == InputId::A) && f.iter().any(|x| x.input == InputId::B)
    };
    if fu.is_empty() || fv.is_empty() {
        return PairKind::Unread;
    }
    match (on_both(fu), on_both(fv)) {
        (true, true) => {
            if fu == fv {
                PairKind::SameCurve
            } else if fu.is_subset(fv) || fv.is_subset(fu) {
                PairKind::CurveEnd
            } else {
                PairKind::DifferentFaces
            }
        }
        (false, false) => PairKind::Unread,
        (p_is_u, _) => {
            // `p`: the vertex of one operand's mesh; `q`: the intersection point.
            let (p, q) = if p_is_u { (fv, fu) } else { (fu, fv) };
            if !p.is_subset(q) {
                PairKind::DifferentFaces
            } else if p.len() >= 2 {
                PairKind::BoundaryEndpoint
            } else {
                PairKind::NearCurveInterior
            }
        }
    }
}

/// Append one census line for the current case (`YANG_FIG11B_CENSUS=<file>`,
/// keyed by `ASSAY_CASE` like `repair_census`, subprocess-safe). No effect on
/// any result.
pub(crate) fn log(line: &str) {
    let Some(path) = std::env::var_os("YANG_FIG11B_CENSUS") else {
        return;
    };
    let case = std::env::var("ASSAY_CASE").unwrap_or_else(|_| "-".into());
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        // One `write` per line: `writeln!` on a `File` issues several, and
        // parallel children's appends then interleave mid-line.
        use std::io::Write;
        let _ = f.write_all(format!("{case}\t{line}\n").as_bytes());
    }
}

/// Whether the census is on.
pub(crate) fn census_on() -> bool {
    std::env::var_os("YANG_FIG11B_CENSUS").is_some()
}

/// Print-only census (`YANG_FIG11B_CENSUS=<file>`, spec §4 increment 0): the
/// classes [`fig11b_vertex_classes`] forms at `site`, with member positions
/// as f64 bit patterns so an offline join can match them to the pairs the
/// existing merges take in the same build.
pub(crate) fn census(
    site: &str,
    mesh: &Mesh,
    attribution: &[Option<TriangleAttribution>],
    vertex_params: Option<&VertexParamMap>,
    rank: &dyn Fn(u32) -> u8,
    moved: &dyn Fn(u32) -> bool,
) {
    if !census_on() {
        return;
    }
    let cls = fig11b_vertex_classes(mesh, attribution, vertex_params, rank);
    let bits = operand_bits(mesh, attribution, vertex_params);
    let faces_tag = |v: u32| -> String {
        let f = faces_of(vertex_params, v);
        if f.is_empty() {
            return "-".into();
        }
        f.iter()
            .map(|x| {
                format!(
                    "{}{}",
                    match x.input {
                        InputId::A => 'A',
                        InputId::B => 'B',
                    },
                    x.face
                )
            })
            .collect::<Vec<_>>()
            .join("+")
    };
    // The mesh edges that join two members of one class, each with the
    // paper statement it falls under.
    let class_of: BTreeMap<u32, u32> = cls
        .classes
        .iter()
        .chain(cls.too_wide.iter())
        .flat_map(|c| c.members.iter().map(move |&m| (m, c.members[0])))
        .collect();
    let mut class_edges: BTreeMap<u32, BTreeSet<(u32, u32)>> = BTreeMap::new();
    for tri in &mesh.tris {
        for k in 0..3 {
            let (u, v) = (tri[k].min(tri[(k + 1) % 3]), tri[k].max(tri[(k + 1) % 3]));
            if u == v || !(bits[u as usize] == 3 || bits[v as usize] == 3) {
                continue;
            }
            if let (Some(cu), Some(cv)) = (class_of.get(&u), class_of.get(&v)) {
                if cu == cv && same_point(mesh, u, v) {
                    class_edges.entry(*cu).or_default().insert((u, v));
                }
            }
        }
    }
    for (kind, list) in [("class", &cls.classes), ("too_wide", &cls.too_wide)] {
        for c in list {
            let members: Vec<String> = c
                .members
                .iter()
                .map(|&m| {
                    format!(
                        "{m}:{}:r{}:b{}:{}:f{}",
                        pos_key(mesh, m),
                        rank(m),
                        bits[m as usize],
                        if moved(m) { 'm' } else { 's' },
                        faces_tag(m),
                    )
                })
                .collect();
            let edges: Vec<String> = class_edges
                .get(&c.members[0])
                .into_iter()
                .flatten()
                .map(|&(u, v)| {
                    format!(
                        "{u}-{v}:{}:{:.3e}",
                        pair_kind(&faces_of(vertex_params, u), &faces_of(vertex_params, v)).tag(),
                        dist(mesh, u, v).0,
                    )
                })
                .collect();
            log(&format!(
                "site={site}\t{kind}\tsize={}\tdiam={:.3e}\tsurvivor={}:{}\tmembers=[{}]\tedges=[{}]",
                c.members.len(),
                c.diameter,
                c.survivor,
                pos_key(mesh, c.survivor),
                members.join(" "),
                edges.join(" ")
            ));
        }
    }
    log(&format!(
        "site={site}\tsummary\tclasses={}\ttoo_wide={}",
        cls.classes.len(),
        cls.too_wide.len()
    ));
}

/// A vertex's position as three f64 bit patterns, hex, `x/y/z`.
pub(crate) fn pos_key(mesh: &Mesh, v: u32) -> String {
    let p = mesh.verts[v as usize].as_array();
    format!(
        "{:016x}/{:016x}/{:016x}",
        p[0].to_bits(),
        p[1].to_bits(),
        p[2].to_bits()
    )
}
