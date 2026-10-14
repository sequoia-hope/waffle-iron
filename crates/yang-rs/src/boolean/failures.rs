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
//! Phase 1b landed the record; Phase 1c's `boolean::refine_loop` reads it
//! through [`failures`] and refines [`region`] (spec §6).

use crate::brep::{BRep, InputId, TriangleAttributionMap};
use crate::stage4_correct::DomainFire;
use crate::{Mesh, YangError};
use std::collections::BTreeSet;

/// The paper's trigger set (spec §3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
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
    /// T6 — §4.3 `:525-530`: "the two meshes do not intersect but are
    /// within the distance tolerance d_ε" — a cross-operand pair of DISJOINT
    /// mesh triangles whose exact surfaces cross deeper than the model band
    /// (`shell_contact::contact_pairs`), joined by the arrangement nowhere
    /// in the two triangles' one-rings: the mesh missed a real intersection.
    MissedIntersection,
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
    for site in nonmanifold_sites(mesh, attribution) {
        record(site);
    }
}

/// T4 as a pure function: one site per non-manifold seam region of `mesh`,
/// naming the faces at its unpaired edges. Read at the Stage-4 gate (the
/// recorder above) AND on every EMITTED body (`failures`): a mesh that
/// passed the Stage-4 gate can still leave Stage 5/6 non-manifold, and the
/// paper's output is watertight by construction (§4.4.3) — measured
/// 2026-10-14 on the `n2_rim_mint_adversary` ×1e-2 fixture, whose refined
/// body passed the gate and emitted 18 unpaired half-edges.
pub(crate) fn nonmanifold_sites(
    mesh: &Mesh,
    attribution: &TriangleAttributionMap,
) -> Vec<FailureSite> {
    let attr_of = |ti: usize| -> Option<(bool, u32)> {
        attribution
            .attributions
            .get(ti)
            .copied()
            .flatten()
            .map(|at| (matches!(at.input, InputId::A), at.face))
    };
    let regions = crate::stage4_project::detect_nonmanifold_seams(&mesh.tris, &attr_of);
    regions
        .into_iter()
        .map(|r| FailureSite {
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
        })
        .collect()
}

/// The input-triangle pairs the arrangement JOINED: for every intersection
/// edge (`LabeledArrangement::intersection_edges`, the producer's record of
/// the input×input polylines), the A-parent × B-parent input triangles of the
/// output triangles incident to it (`la.source`). A pair listed here is one
/// where "the two meshes intersect" in the paper's sense (§4.3 `:525-530`);
/// T6 asks the complement.
pub(crate) fn joined_triangle_pairs(la: &cherchi_rs::LabeledArrangement) -> BTreeSet<(u32, u32)> {
    use cherchi_rs::InputId as LaInputId;
    let mut by_edge: std::collections::BTreeMap<(u32, u32), (Vec<u32>, Vec<u32>)> =
        Default::default();
    for (ot, tri) in la.mesh.tris.iter().enumerate() {
        for k in 0..3 {
            let (i, j) = (tri[k], tri[(k + 1) % 3]);
            let key = (i.min(j), i.max(j));
            if !la.intersection_edges.contains(&key) {
                continue;
            }
            let entry = by_edge.entry(key).or_default();
            for &(inp, raw) in la.source.get(ot).map(|v| v.as_slice()).unwrap_or(&[]) {
                if inp == LaInputId(0) {
                    entry.0.push(raw);
                } else {
                    entry.1.push(raw);
                }
            }
        }
    }
    let mut joined = BTreeSet::new();
    for (pa, pb) in by_edge.values() {
        for &x in pa {
            for &y in pb {
                joined.insert((x, y));
            }
        }
    }
    joined
}

/// Per-vertex incident triangles of an input mesh — the one-ring reader
/// behind the T6 capture test.
fn vertex_stars(mesh: &Mesh) -> Vec<Vec<u32>> {
    let mut stars = vec![Vec::new(); mesh.verts.len()];
    for (t, tri) in mesh.tris.iter().enumerate() {
        for &v in tri {
            if let Some(s) = stars.get_mut(v as usize) {
                s.push(t as u32);
            }
        }
    }
    stars
}

/// T6's capture test, prepared once per run: is an A×B input-triangle pair
/// joined by the arrangement anywhere in the two triangles' one-rings?
pub(crate) struct RingCapture<'m> {
    mesh_a: &'m Mesh,
    mesh_b: &'m Mesh,
    stars_a: Vec<Vec<u32>>,
    stars_b: Vec<Vec<u32>>,
    joined: &'m BTreeSet<(u32, u32)>,
}

impl<'m> RingCapture<'m> {
    pub(crate) fn new(
        mesh_a: &'m Mesh,
        mesh_b: &'m Mesh,
        joined: &'m BTreeSet<(u32, u32)>,
    ) -> Self {
        RingCapture {
            mesh_a,
            mesh_b,
            stars_a: vertex_stars(mesh_a),
            stars_b: vertex_stars(mesh_b),
            joined,
        }
    }

    fn ring(mesh: &Mesh, stars: &[Vec<u32>], t: u32) -> Vec<u32> {
        let mut r: Vec<u32> = mesh
            .tris
            .get(t as usize)
            .into_iter()
            .flat_map(|tri| tri.iter())
            .flat_map(|&v| stars.get(v as usize).into_iter().flatten().copied())
            .collect();
        r.push(t);
        r.sort_unstable();
        r.dedup();
        r
    }

    /// The two one-rings, and whether any of their pairs is joined.
    pub(crate) fn rings(&self, tri_a: u32, tri_b: u32) -> (Vec<u32>, Vec<u32>, bool) {
        let ra = Self::ring(self.mesh_a, &self.stars_a, tri_a);
        let rb = Self::ring(self.mesh_b, &self.stars_b, tri_b);
        let hit = ra
            .iter()
            .any(|&x| rb.iter().any(|&y| self.joined.contains(&(x, y))));
        (ra, rb, hit)
    }

    pub(crate) fn captured(&self, tri_a: u32, tri_b: u32) -> bool {
        self.rings(tri_a, tri_b).2
    }
}

/// T6 as a pure function (spec §3 "T6"): a contact pair the arrangement did
/// not join anywhere in the two triangles' one-rings — no intersection edge
/// whose A-parent shares a vertex with `tri_a` AND whose B-parent shares a
/// vertex with `tri_b` (`joined`, from [`joined_triangle_pairs`]) — is a
/// missed intersection; each names the two triangles' input faces, one site
/// per distinct face pair (the first pair's point as its detail).
///
/// The test is topological, as the paper's is ("the two meshes do not
/// intersect"): no distance to the arrangement's edges is measured. A
/// metric capture at d_ε was measured WRONG on C0058 (2026-10-14): a point
/// on the exact curve is within d_ε of each operand's facet, but the facets'
/// intersection LINE sits up to (dev_A + dev_B) / sin θ away for a dihedral
/// θ between the surfaces — 1.07 d_ε and 1.38 d_ε at two true crossings of
/// that tangency pair, unbounded at its tangent point. The one-ring absorbs
/// that offset wherever θ is not degenerate; a degenerate θ is the §4.3.3
/// tangent-point mint's business, not T6's.
pub(crate) fn missed_intersection_sites(
    contacts: &[crate::shell_contact::ContactPair],
    mesh_a: &Mesh,
    tri_face_a: &[u32],
    mesh_b: &Mesh,
    tri_face_b: &[u32],
    joined: &BTreeSet<(u32, u32)>,
) -> Vec<FailureSite> {
    let capture = RingCapture::new(mesh_a, mesh_b, joined);
    let probe = std::env::var_os("YANG_T6_PROBE").is_some();
    let mut seen: BTreeSet<(u32, u32)> = BTreeSet::new();
    let mut out = Vec::new();
    for c in contacts {
        let (Some(&fa), Some(&fb)) = (
            tri_face_a.get(c.tri_a as usize),
            tri_face_b.get(c.tri_b as usize),
        ) else {
            continue;
        };
        if seen.contains(&(fa, fb)) {
            continue;
        }
        let (ring_a, ring_b, captured) = capture.rings(c.tri_a, c.tri_b);
        if captured {
            continue;
        }
        if probe {
            eprintln!(
                "[t6-site] faces {fa}×{fb} tris {}×{} point {:?} reach {:.3e}; rings {}×{} \
                 triangles, {} joined pairs in the arrangement, none between the rings",
                c.tri_a,
                c.tri_b,
                c.point,
                c.reach,
                ring_a.len(),
                ring_b.len(),
                joined.len()
            );
        }
        seen.insert((fa, fb));
        out.push(FailureSite {
            trigger: FailureTrigger::MissedIntersection,
            faces: vec![(InputId::A, fa), (InputId::B, fb)],
            detail: format!(
                "tris {}×{} touch within {:.3e} at ({:.6e}, {:.6e}, {:.6e}); the arrangement \
                 joins no triangle pair of their one-rings",
                c.tri_a, c.tri_b, c.reach, c.point[0], c.point[1], c.point[2]
            ),
        });
    }
    out
}

/// T6 producer: the Stage-2 input meshes against the arrangement's own
/// intersection edges. Record-only here; the loop reads the record.
pub(crate) fn record_missed_intersections(
    mesh_a: &Mesh,
    tri_face_a: &[u32],
    faces_a: &[crate::BRepFace],
    mesh_b: &Mesh,
    tri_face_b: &[u32],
    faces_b: &[crate::BRepFace],
    la: &cherchi_rs::LabeledArrangement,
) {
    // The capture test first: a candidate the arrangement already joined in
    // its one-rings is never refined (spec §3 "T6 as built", F0055).
    let joined = joined_triangle_pairs(la);
    let capture = RingCapture::new(mesh_a, mesh_b, &joined);
    let Ok(contacts) = crate::shell_contact::contact_pairs(
        mesh_a,
        tri_face_a,
        faces_a,
        mesh_b,
        tri_face_b,
        faces_b,
        &|ta, tb| capture.captured(ta, tb),
    ) else {
        return; // no lineage: nothing to say (the census is loud on its own path)
    };
    if contacts.is_empty() {
        return;
    }
    for site in
        missed_intersection_sites(&contacts, mesh_a, tri_face_a, mesh_b, tri_face_b, &joined)
    {
        record(site);
    }
}

/// T3: the §4.5.4 illegal intersections of an EMITTED mesh — every improper
/// (non-index-sharing, intersecting or touching) triangle pair, named by the
/// two triangles' input faces. One site per pair; an unresolved pair (the
/// census could not classify it) is a site with no faces.
// T3 (mesh chords) becomes the loop's trigger when Phase 2 (§4.4.1 mesh
// updating) makes the Stage-4 mesh valid (spec §3, measured 2026-10-14);
// until then it is the census the probes and pins read.
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

/// Yang §4.5.2's refinement step for a T1 site that carries the
/// under-resolution certificate `f` (a `d_ε` DIVISOR: the factor by which
/// the far surface's chord band must shrink): the DENSITY factor
/// `k = next power of two ≥ √f` (chord sagitta ∝ 1/N²), at least 2 — one
/// doubling — so a certificate the first doubling already satisfies, a NaN
/// or `None` all ask for one doubling. R0085's 18.2 → 8, P0039's 2.24 → 2.
/// Capped at `FACE_REFINE_MAX` (8, the old d_ε/64 ceiling): P0015's 159
/// (√159 ≈ 12.6) starts AT the ceiling — a demand past it certifies nothing
/// about the rungs inside it (P0015 converged at d_ε/32 while its
/// certificate read 159), so the loop's first rung is the last one.
pub(crate) fn density_for_demand(demand: Option<f64>) -> u32 {
    let f = match demand {
        Some(f) if f.is_finite() && f > 1.0 => f,
        _ => return 2,
    };
    let mut k: u32 = 2;
    while (f64::from(k) * f64::from(k)) < f && k < crate::stage1_tessellate::FACE_REFINE_MAX {
        k *= 2;
    }
    k
}

/// One output edge's exact curve sampled from its start vertex to its end
/// vertex, as kernel-v2 reads it (`from_yang/classify`): a straight edge by
/// its endpoints; an open circle or ellipse edge along its MINOR arc (`None`
/// within `1e-9` of a half sweep — kernel-v2 refuses that edge loudly); a
/// closed conic over its whole period; a parabola / hyperbola between its
/// endpoint parameters; a surface-pair edge by its chord (no parameter — a
/// named gap). Samples hold the curve to `tol` (the sagitta of a chord), at
/// most `MAX_EDGE_SAMPLES` segments.
fn sample_output_edge(
    curve: &crate::Curve,
    ps: crate::Point3,
    pe: crate::Point3,
    closed: bool,
    tol: f64,
) -> Option<Vec<[f64; 3]>> {
    use crate::stage4_correct::{conic_param, conic_param_periodic};
    use crate::Curve;
    const MAX_EDGE_SAMPLES: usize = 2048;
    let two_pi = 2.0 * std::f64::consts::PI;
    let (start, end) = (ps.as_array(), pe.as_array());
    let radius = match curve {
        Curve::Circle { radius, .. } => *radius,
        Curve::Ellipse { major_radius, .. } => *major_radius,
        Curve::Parabola { .. } | Curve::Hyperbola { .. } => 0.0,
        Curve::LineSegment | Curve::SurfacePair { .. } => return Some(vec![start, end]),
    };
    let ts = conic_param(curve, ps)?;
    let sweep = if conic_param_periodic(curve) {
        if closed {
            two_pi
        } else {
            let te = conic_param(curve, pe)?;
            let ccw = (te - ts).rem_euclid(two_pi);
            if (ccw - std::f64::consts::PI).abs() <= 1e-9 {
                return None;
            }
            if ccw < std::f64::consts::PI {
                ccw
            } else {
                ccw - two_pi
            }
        }
    } else {
        conic_param(curve, pe)? - ts
    };
    let n = if radius > 0.0 && tol > 0.0 && tol < radius {
        // Chord sagitta `r(1 − cos(Δ/2)) ≤ tol` ⇒ `Δ ≤ 2·acos(1 − tol/r)`.
        let step = 2.0 * (1.0 - tol / radius).acos();
        ((sweep.abs() / step).ceil() as usize).clamp(1, MAX_EDGE_SAMPLES)
    } else {
        128
    };
    let mut pts = Vec::with_capacity(n + 1);
    pts.push(start);
    for k in 1..n {
        let t = ts + sweep * (k as f64) / (n as f64);
        pts.push(crate::geom::conic_eval(curve, t)?.as_array());
    }
    pts.push(end);
    Some(pts)
}

/// The OUTPUT-level §4.5.4 sites of an emitted body: T3b (two faces on one
/// quadric whose boundary conics cross) and T3c (one face whose loops cross
/// or wind against its material side). A body carrying one is not a valid
/// B-Rep, whatever produced it.
pub(crate) fn output_illegal_sites(out: &BRep) -> Vec<FailureSite> {
    let mut v = output_curve_crossings(out);
    v.extend(output_loop_crossings(out));
    v
}

/// T3c — the OUTPUT-level §4.5.4 check WITHIN one face (spec
/// `yang_452_refinement_loop.md` §3 "T3c"): a face whose loops, read on
/// their EXACT curves in the face's own chart, cross each other or wind
/// against the face's material side. *"The self-intersections that may
/// occur during surface discretization and the new intersections that may
/// arise during mesh updating can lead to topological inconsistencies
/// between the mesh and the B-Rep model … We detect these illegal
/// intersections and perform local refinement"* (`:752-758`). T3b checks
/// two DISTINCT faces on one quadric; this is the same mechanism inside one
/// face. Anchors, measured 2026-10-14 with the §4.3.3 arms off: P0047 — a
/// prism side whose cut-cylinder ellipse arc bulges past the face's own
/// straight edge and crosses back (kernel-v2's CDT refused the ring); P0044
/// — a bounded patch of the cut cylinder whose one loop winds against its
/// material side (kernel-v2: "exactly one material-CCW loop"). In both,
/// yang's failure set was empty.
///
/// Charts: a plane's own right-handed frame (`ortho_basis`, `e1 × e2 = n`);
/// a cylinder UNROLLED to `(R·θ, h)`, right-handed about the outward radial
/// normal. The outer loop must enclose positive oriented area and every
/// hole negative, the sign flipped for a `reversed` face; two non-adjacent
/// segments cross when each straddles the other's line by more than the
/// model band (a tangency or a shared corner is not a crossing). Edges are
/// sampled as kernel-v2 reads them ([`sample_output_edge`]). Named gaps: a
/// face with a `SurfacePair` edge (no parameter — its chord could invent a
/// crossing), a cylinder face with a loop that wraps the axis or a closed rim
/// edge (a periodic strip — its chart has a seam), and
/// the cone / sphere / torus charts. Each site names the face's input
/// lineage and the lineage of the faces across the offending edges, so the
/// loop refines both operands there.
pub(crate) fn output_loop_crossings(out: &BRep) -> Vec<FailureSite> {
    use crate::Surface;
    let faces = out.faces();
    let edges = out.edges();
    let verts = out.vertices();
    let scale = verts
        .iter()
        .flat_map(|v| v.point.as_array())
        .fold(0.0f64, |m, c| m.max(c.abs()));
    let band = cad_primitives::TAU_MODEL * (1.0 + scale);
    let two_pi = 2.0 * std::f64::consts::PI;
    let lineage = |fi: usize| -> Option<(InputId, u32)> {
        out.face_attribution.get(fi).map(|at| (at.input, at.face))
    };
    // The face across a directed edge: the face whose loop walks the same
    // two vertices the other way.
    let mut owner: std::collections::BTreeMap<(u32, u32), usize> = Default::default();
    for (fi, f) in faces.iter().enumerate() {
        for &ei in f.outer_loop.iter().chain(f.inner_loops.iter().flatten()) {
            if let Some(e) = edges.get(ei as usize) {
                owner.entry((e.start, e.end)).or_insert(fi);
            }
        }
    }
    let across = |ei: u32| -> Option<usize> {
        let e = edges.get(ei as usize)?;
        owner.get(&(e.end, e.start)).copied()
    };
    let dot3 = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let probe = std::env::var_os("YANG_T3C_PROBE").is_some();
    let mut sites = Vec::new();
    'faces: for (fi, face) in faces.iter().enumerate() {
        // The face's chart.
        enum Chart {
            Plane([f64; 3], [f64; 3]),
            Cylinder {
                a: [f64; 3],
                u: [f64; 3],
                e1: [f64; 3],
                e2: [f64; 3],
                r: f64,
            },
        }
        let (chart, sense) = match face.surface {
            Surface::Plane { normal, .. } => {
                let (e1, e2) = crate::ortho_basis(normal);
                (Chart::Plane(e1.as_array(), e2.as_array()), 1.0)
            }
            Surface::Cylinder {
                axis_point,
                axis_dir,
                radius,
            } => {
                let un = crate::normalize3(axis_dir.as_array());
                let (e1, e2) = crate::ortho_basis(axis_dir);
                (
                    Chart::Cylinder {
                        a: axis_point.as_array(),
                        u: un,
                        e1: e1.as_array(),
                        e2: e2.as_array(),
                        r: radius,
                    },
                    if face.reversed { -1.0 } else { 1.0 },
                )
            }
            _ => continue,
        };
        // Each loop as exact-curve samples, each sample tagged with the edge
        // that leaves it.
        let loops_3d: Vec<Vec<([f64; 3], u32)>> = {
            let mut all = Vec::new();
            for lp in std::iter::once(&face.outer_loop).chain(face.inner_loops.iter()) {
                let mut pts: Vec<([f64; 3], u32)> = Vec::new();
                for &ei in lp {
                    let Some(e) = edges.get(ei as usize) else {
                        if probe {
                            eprintln!("[t3c] face {fi} skipped: missing edge");
                        }
                        continue 'faces;
                    };
                    if matches!(e.curve, crate::Curve::SurfacePair { .. }) {
                        if probe {
                            eprintln!("[t3c] face {fi} skipped: surface-pair edge");
                        }
                        continue 'faces;
                    }
                    let (Some(vs), Some(ve)) =
                        (verts.get(e.start as usize), verts.get(e.end as usize))
                    else {
                        if probe {
                            eprintln!("[t3c] face {fi} skipped: missing vertex");
                        }
                        continue 'faces;
                    };
                    let closed = e.start == e.end || lp.len() == 1;
                    if closed && matches!(chart, Chart::Cylinder { .. }) {
                        // A full rim: the face is a periodic strip whose
                        // chart has a seam — the wrapping-loop named gap.
                        if probe {
                            eprintln!("[t3c] face {fi} skipped: closed rim on a cylinder");
                        }
                        continue 'faces;
                    }
                    let Some(samples) =
                        sample_output_edge(&e.curve, vs.point, ve.point, closed, band)
                    else {
                        if probe {
                            eprintln!(
                                "[t3c] face {fi} skipped: unsampleable edge (half-sweep arc)"
                            );
                        }
                        continue 'faces;
                    };
                    for p in &samples[..samples.len() - 1] {
                        pts.push((*p, ei));
                    }
                }
                if pts.len() < 2 {
                    if probe {
                        eprintln!("[t3c] face {fi} skipped: loop under 2 samples");
                    }
                    continue 'faces;
                }
                all.push(pts);
            }
            all
        };
        // Into the chart; a cylinder loop is unwrapped along its walk and
        // must close (no net turn about the axis).
        let mut loops_2d: Vec<Vec<([f64; 2], u32)>> = Vec::new();
        for lp in &loops_3d {
            let mut out2 = Vec::with_capacity(lp.len());
            match chart {
                Chart::Plane(e1, e2) => {
                    for &(p, ei) in lp {
                        out2.push(([dot3(p, e1), dot3(p, e2)], ei));
                    }
                }
                Chart::Cylinder { a, u, e1, e2, r } => {
                    let theta = |p: [f64; 3]| {
                        let w = [p[0] - a[0], p[1] - a[1], p[2] - a[2]];
                        (dot3(w, e2).atan2(dot3(w, e1)), dot3(w, u))
                    };
                    let (mut prev, _) = theta(lp[0].0);
                    let mut acc = prev;
                    for &(p, ei) in lp {
                        let (th, h) = theta(p);
                        let mut d = th - prev;
                        if d > std::f64::consts::PI {
                            d -= two_pi;
                        } else if d < -std::f64::consts::PI {
                            d += two_pi;
                        }
                        acc += d;
                        prev = th;
                        out2.push(([r * acc, h], ei));
                    }
                    // Closing step back to the first sample.
                    let (th0, _) = theta(lp[0].0);
                    let mut d = th0 - prev;
                    if d > std::f64::consts::PI {
                        d -= two_pi;
                    } else if d < -std::f64::consts::PI {
                        d += two_pi;
                    }
                    let net = acc + d - out2[0].0[0] / r;
                    if net.abs() > std::f64::consts::PI {
                        if probe {
                            eprintln!("[t3c] face {fi} skipped: wrapping loop");
                        }
                        continue 'faces; // a wrapping loop — named gap
                    }
                }
            }
            loops_2d.push(out2);
        }
        let mut named: BTreeSet<(InputId, u32)> = BTreeSet::new();
        let mut detail: Option<String> = None;
        // Orientation: outer positive, holes negative (× sense).
        for (li, lp) in loops_2d.iter().enumerate() {
            let n = lp.len();
            let area2: f64 = (0..n)
                .map(|i| {
                    let (p, q) = (lp[i].0, lp[(i + 1) % n].0);
                    p[0] * q[1] - q[0] * p[1]
                })
                .sum::<f64>()
                * sense;
            let want_positive = li == 0;
            if probe {
                eprintln!(
                    "[t3c] face {fi} {:?} loop {li}: {} samples, oriented area {:.3e} (sense {sense})",
                    face.surface,
                    n,
                    0.5 * area2
                );
            }
            if (area2 > 0.0) != want_positive {
                detail = Some(format!(
                    "output face {fi}: loop {li} winds against the face's material side \
                     (oriented area {:.3e})",
                    0.5 * area2
                ));
                named.extend(lineage(fi));
                for &(_, ei) in lp {
                    named.extend(across(ei).and_then(lineage));
                }
                break;
            }
        }
        // Crossings.
        if detail.is_none() {
            // `(a, b, edge, loop, index-in-loop)`.
            type Seg = ([f64; 2], [f64; 2], u32, usize, usize);
            let mut segs: Vec<Seg> = Vec::new();
            for (li, lp) in loops_2d.iter().enumerate() {
                let n = lp.len();
                for i in 0..n {
                    segs.push((lp[i].0, lp[(i + 1) % n].0, lp[i].1, li, i));
                }
            }
            let side = |a: [f64; 2], b: [f64; 2], p: [f64; 2]| -> f64 {
                let d = [b[0] - a[0], b[1] - a[1]];
                let l = (d[0] * d[0] + d[1] * d[1]).sqrt();
                if l == 0.0 {
                    return 0.0;
                }
                (d[0] * (p[1] - a[1]) - d[1] * (p[0] - a[0])) / l
            };
            let straddles = |a: [f64; 2], b: [f64; 2], p: [f64; 2], q: [f64; 2]| -> bool {
                let (sp, sq) = (side(a, b, p), side(a, b, q));
                (sp > band && sq < -band) || (sp < -band && sq > band)
            };
            let adjacent = |x: &Seg, y: &Seg| -> bool {
                if x.3 != y.3 {
                    return false;
                }
                let n = loops_2d[x.3].len();
                let d = x.4.abs_diff(y.4);
                d <= 1 || d == n - 1
            };
            let lo_x = |i: usize| segs[i].0[0].min(segs[i].1[0]);
            let hi_x = |i: usize| segs[i].0[0].max(segs[i].1[0]);
            let mut order: Vec<usize> = (0..segs.len()).collect();
            order.sort_by(|&i, &j| lo_x(i).total_cmp(&lo_x(j)));
            'sweep: for (k, &i) in order.iter().enumerate() {
                for &j in &order[k + 1..] {
                    if lo_x(j) > hi_x(i) + band {
                        break;
                    }
                    let (x, y) = (&segs[i], &segs[j]);
                    if adjacent(x, y) {
                        continue;
                    }
                    let (ylo, yhi) = (x.0[1].min(x.1[1]), x.0[1].max(x.1[1]));
                    if y.0[1].max(y.1[1]) < ylo - band || y.0[1].min(y.1[1]) > yhi + band {
                        continue;
                    }
                    if straddles(x.0, x.1, y.0, y.1) && straddles(y.0, y.1, x.0, x.1) {
                        detail = Some(format!(
                            "output face {fi}: its loops cross on their exact curves \
                             (edges {} × {}, loops {} / {})",
                            x.2, y.2, x.3, y.3
                        ));
                        named.extend(lineage(fi));
                        for ei in [x.2, y.2] {
                            named.extend(across(ei).and_then(lineage));
                        }
                        break 'sweep;
                    }
                }
            }
        }
        if let Some(detail) = detail {
            sites.push(FailureSite {
                trigger: FailureTrigger::IllegalIntersection,
                faces: named.into_iter().collect(),
                detail,
            });
        }
    }
    sites
}

/// T3b — the OUTPUT-level §4.5.4 check (spec `yang_452_refinement_loop.md`
/// §6.3): two DISTINCT faces of the emitted B-Rep on ONE quadric whose
/// planar boundary curves cross. Every planar section of a quadric is a
/// conic in its own plane, so two such curves meet exactly where their two
/// planes' common line meets the quadric — a line × quadric root pair
/// (`segment_surface_roots`) — and a root is an illegal intersection iff
/// `conic_param` places it strictly inside BOTH edges' parameter spans and
/// clear of their endpoints (a corner two faces share is not a crossing).
/// Coincident or parallel planes never cross transversally (a shared edge).
/// Planar faces, straight edges and `SurfacePair` curves are outside this
/// check (named gap). Each hit names the two faces' INPUT lineage
/// (`BRep::face_attribution`), so the loop can refine the operands.
pub(crate) fn output_curve_crossings(out: &BRep) -> Vec<FailureSite> {
    use crate::geom::curve_plane;
    use crate::stage4_correct::conic_param;
    use crate::Surface;
    let faces = out.faces();
    let edges = out.edges();
    let verts = out.vertices();
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let cross = |a: [f64; 3], b: [f64; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let scale = verts
        .iter()
        .flat_map(|v| v.point.as_array())
        .fold(0.0f64, |m, c| m.max(c.abs()));
    let band = cad_primitives::TAU_MODEL * (1.0 + scale);
    let dist = |a: [f64; 3], b: [f64; 3]| {
        ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
    };
    // Is `q` strictly inside edge `e`'s span of its own curve, clear of both
    // endpoints? `None` when the curve has no parameter (line, pair).
    let inside_span = |e: u32, q: [f64; 3]| -> Option<bool> {
        let ed = edges.get(e as usize)?;
        let ps = verts.get(ed.start as usize)?.point.as_array();
        let pe = verts.get(ed.end as usize)?.point.as_array();
        if dist(q, ps) <= band || dist(q, pe) <= band {
            return Some(false);
        }
        let qp = crate::Point3::new(q[0], q[1], q[2]);
        let t = conic_param(&ed.curve, qp)?;
        if ed.start == ed.end {
            return Some(true); // a closed curve: everywhere but the seam
        }
        let ts = conic_param(&ed.curve, verts[ed.start as usize].point)?;
        let te = conic_param(&ed.curve, verts[ed.end as usize].point)?;
        if crate::stage4_correct::conic_param_periodic(&ed.curve) {
            let two_pi = 2.0 * std::f64::consts::PI;
            let sweep = (te - ts).rem_euclid(two_pi);
            let off = (t - ts).rem_euclid(two_pi);
            Some(off > 0.0 && off < sweep)
        } else {
            Some(t > ts.min(te) && t < ts.max(te))
        }
    };
    let lineage = |fi: usize| -> Option<(InputId, u32)> {
        out.face_attribution.get(fi).map(|at| (at.input, at.face))
    };
    let mut sites = Vec::new();
    for fi in 0..faces.len() {
        let surf = faces[fi].surface;
        if matches!(surf, Surface::Plane { .. }) {
            continue;
        }
        for fj in (fi + 1)..faces.len() {
            if faces[fj].surface != surf {
                continue;
            }
            let loops = |f: &crate::BRepFace| -> Vec<u32> {
                f.outer_loop
                    .iter()
                    .chain(f.inner_loops.iter().flatten())
                    .copied()
                    .collect()
            };
            for &ei in &loops(&faces[fi]) {
                let Some((n1, d1)) = edges.get(ei as usize).and_then(|e| curve_plane(&e.curve))
                else {
                    continue;
                };
                for &ej in &loops(&faces[fj]) {
                    let Some((n2, d2)) = edges.get(ej as usize).and_then(|e| curve_plane(&e.curve))
                    else {
                        continue;
                    };
                    let dir = cross(n1, n2);
                    let dd = dot(dir, dir);
                    if dd <= 1e-18 {
                        continue; // parallel or coincident planes: no transversal crossing
                    }
                    // The point of the planes' common line nearest the origin.
                    let a = cross(n2, dir);
                    let b = cross(dir, n1);
                    let p0 = [
                        (-d1 * a[0] - d2 * b[0]) / dd,
                        (-d1 * a[1] - d2 * b[1]) / dd,
                        (-d1 * a[2] - d2 * b[2]) / dd,
                    ];
                    let p1 = [p0[0] + dir[0], p0[1] + dir[1], p0[2] + dir[2]];
                    let Some(roots) = crate::stage4_phantom::segment_surface_roots(p0, p1, surf)
                    else {
                        continue;
                    };
                    for t in roots {
                        if !t.is_finite() {
                            continue;
                        }
                        let q = [p0[0] + t * dir[0], p0[1] + t * dir[1], p0[2] + t * dir[2]];
                        if inside_span(ei, q) == Some(true) && inside_span(ej, q) == Some(true) {
                            let faces_named: Vec<(InputId, u32)> =
                                [lineage(fi), lineage(fj)].into_iter().flatten().collect();
                            sites.push(FailureSite {
                                trigger: FailureTrigger::IllegalIntersection,
                                faces: faces_named,
                                detail: format!(
                                    "output faces {fi} × {fj} on {surf:?}: edges {ei} × {ej} cross at \
                                     ({:.6e}, {:.6e}, {:.6e})",
                                    q[0], q[1], q[2]
                                ),
                            });
                        }
                    }
                }
            }
        }
    }
    sites
}

/// The failure set of one `boolean_once` result (spec §3): the recorded
/// sites, plus the §4.5.4 scan of an emitted body, plus — for an error no
/// producer recorded a site for — one `ReassemblyStop` with no faces, so a
/// STOP is never silently an empty set. Empty ⇔ the run is adopted.
pub(crate) fn failures(result: &Result<BRep, YangError>) -> Vec<FailureSite> {
    let mut out = recorded();
    match result {
        Ok(brep) => {
            // T4 on the EMITTED body (§4.4.3: the output is watertight by
            // construction; Stage 5/6 can still leave it non-manifold after
            // the Stage-4 gate passed).
            out.extend(nonmanifold_sites(&brep.mesh, &brep.triangle_attribution));
            // T3 (mesh chords) is a RECORD until Phase 2 makes the mesh
            // valid (spec §3, measured 2026-10-14: every improper pair on
            // CORRECT outputs is an un-retriangulated chord crossing); the
            // output-level T3b is the trigger.
            out.extend(output_illegal_sites(brep));
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
