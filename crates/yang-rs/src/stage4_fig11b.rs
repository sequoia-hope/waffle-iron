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
use crate::param_surface::carried::VertexParamMap;
use crate::{Mesh, TriangleAttribution};
use std::collections::BTreeMap;

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
) {
    if !census_on() {
        return;
    }
    let cls = fig11b_vertex_classes(mesh, attribution, vertex_params, rank);
    let bits = operand_bits(mesh, attribution, vertex_params);
    for (kind, list) in [("class", &cls.classes), ("too_wide", &cls.too_wide)] {
        for c in list {
            let members: Vec<String> = c
                .members
                .iter()
                .map(|&m| {
                    format!(
                        "{m}:{}:r{}:b{}",
                        pos_key(mesh, m),
                        rank(m),
                        bits[m as usize]
                    )
                })
                .collect();
            log(&format!(
                "site={site}\t{kind}\tsize={}\tdiam={:.3e}\tsurvivor={}:{}\tmembers=[{}]",
                c.members.len(),
                c.diameter,
                c.survivor,
                pos_key(mesh, c.survivor),
                members.join(" ")
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
