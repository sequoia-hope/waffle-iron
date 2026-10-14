//! Yang §4.5 — the failure set of one Boolean run, with the FACES at each
//! site (spec `specs/yang_452_refinement_loop.md` §3–§4; Phase 1b of
//! `specs/yang_faithful_cleanup.md`).
//!
//! The paper refines "the parametric surfaces associated with the erroneous
//! regions … those traversed by C_p and the neighbors of a ring of them"
//! (`refs/text/yang2025_hybrid_boolean.txt:659-668`), so every trigger must
//! name faces. Today's STOPs name a vertex (`Stage4RegionInvalid`), a
//! triangle (`FaceResolutionFailed`) or nothing (`NonManifoldOutput`), and
//! the mesh they were read on is gone by the time `boolean` sees the error.
//! This module is the channel that keeps the site: a thread-local record,
//! reset at every `boolean_once`, pushed to at the point each failure is
//! DETECTED — where mesh and attribution are still in hand — exactly as the
//! boundary-domain certificate already records its `DomainFire`s. One
//! Boolean op runs on one thread (the only pool in the stack is cherchi-rs's,
//! below this crate), so the thread-local is the op's scope.
//!
//! Phase 1b is RECORD-ONLY: nothing in production reads the record yet (the
//! one §4.5.2 loop, Phase 1c, will). Pushing to a thread-local changes no
//! output, so the corpus is byte-identical by construction.

use crate::brep::{BRep, InputId, TriangleAttributionMap};
use crate::stage4_correct::DomainFire;
use crate::{Mesh, YangError};
use std::collections::BTreeSet;

/// The paper's trigger set (spec §3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
// Phase 1b is record-only: the readers below are consumed by the one §4.5.2
// loop of Phase 1c; pinned by `tests_unit/s452_failure_sites.rs`.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) enum FailureTrigger {
    /// T1 — §4.5 `:646-648`: a relocation did not converge in its domain
    /// (`Stage4RegionInvalid`, every reason).
    RelocationDiverged,
    /// T2 — §4.5 / Fig. 13: a boundary point left its face across a crease
    /// (a `DomainFire`).
    BoundaryDomainFire,
    /// T3 — §4.5.4 `:752-758`: an illegal intersection in the emitted mesh
    /// (an improper triangle-pair contact).
    IllegalIntersection,
    /// T4 — §4.4.3: the Stage-4 mesh is not a watertight 2-manifold (an
    /// unpaired or unbalanced edge region).
    NonManifoldMesh,
    /// T5 — a Stage-5/6 reassembly STOP, or any other typed error the
    /// pipeline raised without a recorded site.
    ReassemblyStop,
}

/// One failure site: what fired and the input faces it names. `faces` may be
/// empty when the producer could name none (a sentinel vertex, a provenance
/// miss, a Stage-6 text STOP) — such a site has NO region to refine and the
/// loop must let the STOP stand, honestly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FailureSite {
    pub(crate) trigger: FailureTrigger,
    pub(crate) faces: Vec<(InputId, u32)>,
    pub(crate) detail: String,
}

thread_local! {
    static FAILURE_SITES: std::cell::RefCell<Vec<FailureSite>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Forget the previous run's sites. Called at every `boolean_once` entry.
pub(crate) fn reset() {
    FAILURE_SITES.with(|c| c.borrow_mut().clear());
}

/// Push one site.
pub(crate) fn record(site: FailureSite) {
    FAILURE_SITES.with(|c| c.borrow_mut().push(site));
}

/// The sites recorded since the last [`reset`] (a clone; the record stays
/// readable until the next reset).
// Phase 1b is record-only: the readers below are consumed by the one §4.5.2
// loop of Phase 1c; pinned by `tests_unit/s452_failure_sites.rs`.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn recorded() -> Vec<FailureSite> {
    FAILURE_SITES.with(|c| c.borrow().clone())
}

/// The distinct input faces of every live triangle incident to vertex `v`,
/// in `(input, face)` order — the `DomainFire::incident` rule.
pub(crate) fn faces_at_vertex(
    mesh: &Mesh,
    attribution: &TriangleAttributionMap,
    v: u32,
) -> Vec<(InputId, u32)> {
    // Keyed `(is_b, face)` so operand A's faces list first.
    let mut out: BTreeSet<(bool, u32)> = BTreeSet::new();
    for (ti, tri) in mesh.tris.iter().enumerate() {
        if !tri.contains(&v) {
            continue;
        }
        if let Some(Some(at)) = attribution.attributions.get(ti) {
            out.insert((matches!(at.input, InputId::B), at.face));
        }
    }
    out.into_iter()
        .map(|(is_b, f)| (if is_b { InputId::B } else { InputId::A }, f))
        .collect()
}

/// T1 producer: the Stage-4 wrapper records the STOP'd vertex's incident
/// faces on the mesh as the inner pass left it. A sentinel vertex
/// (`u32::MAX`, no vertex) records an empty face list.
pub(crate) fn record_relocation_divergence(
    mesh: &Mesh,
    attribution: &TriangleAttributionMap,
    vertex: u32,
    reason: &str,
) {
    let faces = if (vertex as usize) < mesh.verts.len() {
        faces_at_vertex(mesh, attribution, vertex)
    } else {
        Vec::new()
    };
    record(FailureSite {
        trigger: FailureTrigger::RelocationDiverged,
        faces,
        detail: format!("v{vertex} {reason}"),
    });
}

/// T2 producer: every boundary-domain fire of the run, with its incident
/// faces (the certificate already computes them).
pub(crate) fn record_domain_fires(fires: &[DomainFire]) {
    for f in fires {
        record(FailureSite {
            trigger: FailureTrigger::BoundaryDomainFire,
            faces: f.incident.clone(),
            detail: format!(
                "v{} left {:?}:{} across edge {} f_post={:.3e}",
                f.v, f.input, f.face, f.edge, f.f_post
            ),
        });
    }
}

/// T4 producer: the watertightness gate records every non-manifold seam
/// region's attribution keys as one site.
pub(crate) fn record_nonmanifold_mesh(mesh: &Mesh, attribution: &TriangleAttributionMap) {
    let attr_of = |ti: usize| -> Option<(bool, u32)> {
        attribution
            .attributions
            .get(ti)
            .copied()
            .flatten()
            .map(|at| (matches!(at.input, InputId::A), at.face))
    };
    let regions = crate::stage4_project::detect_nonmanifold_seams(&mesh.tris, &attr_of);
    for r in regions {
        record(FailureSite {
            trigger: FailureTrigger::NonManifoldMesh,
            faces: r
                .keys
                .iter()
                .map(|&(is_a, f)| (if is_a { InputId::A } else { InputId::B }, f))
                .collect(),
            detail: format!(
                "{} unpaired edge(s), first {:?}",
                r.edges.len(),
                r.edges.first()
            ),
        });
    }
}

/// T3: the §4.5.4 illegal intersections of an EMITTED mesh — every improper
/// (non-index-sharing, intersecting or touching) triangle pair, named by the
/// two triangles' input faces. One site per pair; an unresolved pair (the
/// census could not classify it) is a site with no faces.
// Phase 1b is record-only: the readers below are consumed by the one §4.5.2
// loop of Phase 1c; pinned by `tests_unit/s452_failure_sites.rs`.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn illegal_intersection_sites(
    mesh: &Mesh,
    attribution: &TriangleAttributionMap,
) -> Vec<FailureSite> {
    let report = cherchi_rs::detect_improper_contacts(&mesh.verts, &mesh.tris);
    let face_of = |t: u32| -> Option<(InputId, u32)> {
        attribution
            .attributions
            .get(t as usize)
            .copied()
            .flatten()
            .map(|at| (at.input, at.face))
    };
    let mut out = Vec::new();
    for &(s, t) in &report.improper_pairs {
        let mut faces: Vec<(InputId, u32)> =
            [face_of(s), face_of(t)].into_iter().flatten().collect();
        faces.sort_by_key(|&(i, f)| (matches!(i, InputId::B), f));
        faces.dedup();
        out.push(FailureSite {
            trigger: FailureTrigger::IllegalIntersection,
            faces,
            detail: format!("tris {s} × {t}"),
        });
    }
    for &(s, t) in &report.unresolved_pairs {
        out.push(FailureSite {
            trigger: FailureTrigger::IllegalIntersection,
            faces: Vec::new(),
            detail: format!("unresolved pair {s} × {t}"),
        });
    }
    out
}

/// The failure set of one `boolean_once` result (spec §3): the recorded
/// sites, plus the §4.5.4 scan of an emitted body, plus — for an error no
/// producer recorded a site for — one `ReassemblyStop` with no faces, so a
/// STOP is never silently an empty set. Empty ⇔ the run is adopted.
// Phase 1b is record-only: the readers below are consumed by the one §4.5.2
// loop of Phase 1c; pinned by `tests_unit/s452_failure_sites.rs`.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn failures(result: &Result<BRep, YangError>) -> Vec<FailureSite> {
    let mut out = recorded();
    match result {
        Ok(brep) => {
            out.extend(illegal_intersection_sites(
                &brep.mesh,
                &brep.triangle_attribution,
            ));
        }
        Err(e) => {
            if out.is_empty() {
                out.push(FailureSite {
                    trigger: FailureTrigger::ReassemblyStop,
                    faces: Vec::new(),
                    detail: format!("{e}"),
                });
            }
        }
    }
    out
}

/// The paper's region (spec §4): for each operand, the faces the sites name
/// plus every face sharing a B-Rep VERTEX with one of them — the "ring of
/// neighbors" (Fig. 14's orange). Vertex adjacency, not edge adjacency,
/// because yang's `LineSegment` edges are one directed copy per loop (two
/// faces sharing a straight edge carry different edge indices), while every
/// shared rim or corner shares a vertex. Faces named on neither operand
/// contribute nothing.
// Phase 1b is record-only: the readers below are consumed by the one §4.5.2
// loop of Phase 1c; pinned by `tests_unit/s452_failure_sites.rs`.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn region(sites: &[FailureSite], a: &BRep, b: &BRep) -> (BTreeSet<u32>, BTreeSet<u32>) {
    let mut named_a: BTreeSet<u32> = BTreeSet::new();
    let mut named_b: BTreeSet<u32> = BTreeSet::new();
    for s in sites {
        for &(input, f) in &s.faces {
            match input {
                InputId::A => named_a.insert(f),
                InputId::B => named_b.insert(f),
            };
        }
    }
    (with_one_ring(a, &named_a), with_one_ring(b, &named_b))
}

/// `named` plus every face of `brep` sharing a B-Rep vertex with a named
/// face. Out-of-range names are dropped.
// Phase 1b is record-only: the readers below are consumed by the one §4.5.2
// loop of Phase 1c; pinned by `tests_unit/s452_failure_sites.rs`.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn with_one_ring(brep: &BRep, named: &BTreeSet<u32>) -> BTreeSet<u32> {
    let faces = brep.faces();
    let edges = brep.edges();
    let verts_of = |f: &crate::BRepFace| -> BTreeSet<u32> {
        f.outer_loop
            .iter()
            .chain(f.inner_loops.iter().flatten())
            .filter_map(|&e| edges.get(e as usize))
            .flat_map(|e| [e.start, e.end])
            .collect()
    };
    let mut out: BTreeSet<u32> = named
        .iter()
        .copied()
        .filter(|&f| (f as usize) < faces.len())
        .collect();
    let mut touched: BTreeSet<u32> = BTreeSet::new();
    for &f in &out {
        touched.extend(verts_of(&faces[f as usize]));
    }
    if touched.is_empty() {
        return out;
    }
    for (fi, f) in faces.iter().enumerate() {
        if out.contains(&(fi as u32)) {
            continue;
        }
        if verts_of(f).iter().any(|v| touched.contains(v)) {
            out.insert(fi as u32);
        }
    }
    out
}
