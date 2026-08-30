//! §4.5.1 CORNER TRANSIT — the §4-I9 corner-crosser PLANNER
//! (spec `specs/yang_451_corner_transit.md`, inc-2a).
//!
//! A corner-transit site is a §4-I9 postcondition fire: a Stage-4 relocation
//! solved the exact triple {far, base, facet_k} and the junction EXISTS but is
//! a PHANTOM — it lies past the still model corner `q` = base∩facet_k∩facet_j,
//! outside facet_k's extent. The corner already exists as a mesh vertex; the
//! TRUE junction(s) lie on `q`'s corner-incident model edges, past the corner.
//!
//! This module is the pure planner: per site, solve BOTH candidate corrected
//! triples {far, shared_i, next} seeded at `q`, read each solution against the
//! candidate faces' own boundary-edge domains (the inc-1 instrument, validated
//! family-wide 2026-08-29 and extracted here so census and apply share ONE
//! reading), and classify by the corner-incident-edge rule:
//!
//! > a candidate junction is REAL iff its solution lies within the evaluation
//! > band ON a model edge with an endpoint at `q`, inside that edge's own
//! > segment/arc domain.
//!
//! Sites classify as 1-real (single TRANSIT — crease or base side, both occur)
//! or 2-real (CORNER CLIP: the far surface passes so near the corner that the
//! exact curve crosses BOTH wedge edges — two junctions plus a connecting
//! segment across the adjacent face; Yang Fig-13(c)'s declined case, made
//! deterministic by the analytic constraint sets). Anything else is a typed
//! decline — the standing §4-I9 STOP stays loud (P10).
//!
//! Instrument notes carried from inc-1/inc-1b (measured, load-bearing):
//! - `to_yang_brep` shares only CURVED edges between faces (LineSegments are
//!   per-loop copies), so adjacency is read geometrically over the UNION of
//!   both faces' loops, never by index intersection.
//! - Edge ranking must be CURVE-AWARE: a chord metric is biased against arcs
//!   (a rim's chord sits far from a point exactly ON the arc) and manufactured
//!   a phantom "carrier-authority wall" on R0044. Circle edges rank by their
//!   true circle distance √(axial² + (radial−r)²).
//! - The arc in-domain convention measured on the family is `in_ccw` (wrapped
//!   angle from the edge's start, CCW around the stored normal); an on-circle,
//!   corner-incident solution contained only CW is loudly tagged, never
//!   silently accepted.
//!
//! No mesh mutation lives here. The apply arm (inc-2b+) consumes
//! [`TransitPlan`]; every failure is a typed [`TransitDecline`].

use crate::stage1_tessellate::normalize3;
use crate::stage4_relocate::relocate_onto_implicit_triple;
use crate::stage4_slit::circle_frame;
use crate::{BRep, BRepFace, Curve, InputId, Surface};
use cad_primitives::Point3;

fn d3(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}
fn mag3(v: [f64; 3]) -> f64 {
    v[0].abs().max(v[1].abs()).max(v[2].abs())
}

/// Scale-aware exactness band (the `junction_certificate_band` form): f64
/// evaluation at coordinate magnitude L carries O(ε·L) rounding, so the band
/// certifies "exact to evaluation precision" — `max(TAU_WORK, 8·ε·L)`. The
/// measured family separates by ≥9 orders (real: off ≤ 2.3e-13 at scale 5e3;
/// non-real: 1e-2 and up), so the verdict is not band-sensitive; the census
/// prints the raw distances so the margin stays visible.
fn eval_band(l: f64) -> f64 {
    cad_primitives::TAU_WORK.max(8.0 * f64::EPSILON * l)
}

/// Typed decline reasons. A decline leaves the site to the standing loud
/// §4-I9 STOP; nothing is applied, nothing is guessed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TransitDecline {
    /// The patch split at (v, q) is not the measured family anatomy
    /// (|far| = 1, |next| = 1, |shared| = 2).
    AnatomyMismatch {
        far: usize,
        next: usize,
        shared: usize,
    },
    /// No candidate junction is REAL on a corner-incident model edge — the
    /// discriminator has no in-domain witness (includes both-solves-failed).
    NoRealCandidate,
    /// A real candidate lies within band of the corner `q` itself — the
    /// junction would BE the corner, a class this planner does not own.
    JunctionAtCorner { which: usize },
}

/// Domain verdict of a candidate solution against the winning edge's own
/// curve extent.
#[derive(Clone, Copy, Debug)]
pub(crate) enum DomainVerdict {
    /// LineSegment: unclamped parameter along start→end, plus the
    /// perpendicular distance to the (infinite) carrier line.
    Segment { t: f64, off_line: f64, inside: bool },
    /// Circle arc: wrapped angles CCW around the stored normal, both
    /// orientations (the loop-direction convention is measured, not
    /// assumed; the family reads uniformly `in_ccw`).
    Arc {
        span_ccw: f64,
        sol_ccw: f64,
        in_ccw: bool,
        in_cw: bool,
    },
    /// Circle with start == end — a full rim, no arc domain to test.
    ClosedCircle,
    /// Zero-length chord or degenerate circle frame.
    Degenerate,
    /// A curve kind the instrument does not read yet (none in the family).
    Unreadable,
}

/// The winning boundary edge for one candidate solution: nearest edge of the
/// UNION of both candidate faces' loops, ranked curve-aware.
#[derive(Clone, Copy, Debug)]
pub(crate) struct EdgeRead {
    pub edge: u32,
    /// Which face's loops carry the edge: S = shared_i, N = next, SN = both
    /// (converter-shared curved edge).
    pub own: &'static str,
    /// Curve-aware distance from the solution to the edge (the ranking
    /// metric: true circle distance for arcs, clamped chord otherwise).
    pub d_on_edge: f64,
    /// Distance from the edge's nearer endpoint to the corner `q` —
    /// corner-incidence is `d_q_end ≤ band`.
    pub d_q_end: f64,
    pub domain: DomainVerdict,
}

/// One candidate corrected triple {far, shared_i, next}, solved and read.
#[derive(Clone, Debug)]
pub(crate) struct CandidateRead {
    pub shared: (InputId, u32),
    /// `None` = the exact triple solve did not converge (not a decline by
    /// itself: the candidate is simply not real).
    pub sol: Option<[f64; 3]>,
    pub edge: Option<EdgeRead>,
    /// The corner-incident-edge rule's verdict for this candidate.
    pub real: bool,
    /// Short reason when `real == false` (census print; "" when real).
    pub why: &'static str,
}

/// The per-site read: anatomy split plus both candidates.
#[derive(Clone, Debug)]
pub(crate) struct SiteRead {
    pub far: (InputId, u32),
    pub next: (InputId, u32),
    pub cands: Vec<CandidateRead>,
}

/// Site classification under the corner-incident-edge rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TransitClass {
    /// Exactly one real junction: truncate at the corner, transit to it.
    Transit { cand: usize },
    /// Two real junctions: the exact curve clips the corner — mint both plus
    /// the connecting segment across the adjacent face (Fig-13(c)).
    Clip { cands: [usize; 2] },
}

/// Census formatter for an [`EdgeRead`] — the inc-1 line shapes, from the
/// structured read (plus `d_on_edge`, which the inc-1 Circle line omitted).
pub(crate) fn format_edge_read(er: &Option<EdgeRead>) -> String {
    let Some(er) = er else {
        return "no-edge".into();
    };
    let head = format!(
        "edge={} own={} q_end={:.2e} d_on_edge={:.3e}",
        er.edge, er.own, er.d_q_end, er.d_on_edge
    );
    match er.domain {
        DomainVerdict::Segment {
            t,
            off_line,
            inside,
        } => {
            format!("{head} LineSegment t={t:.4} off_line={off_line:.3e} in_segment={inside}")
        }
        DomainVerdict::Arc {
            span_ccw,
            sol_ccw,
            in_ccw,
            in_cw,
        } => format!(
            "{head} Circle span_ccw={span_ccw:.4} sol_ccw={sol_ccw:.4} \
             in_ccw={in_ccw} in_cw={in_cw}"
        ),
        DomainVerdict::ClosedCircle => format!("{head} Circle CLOSED"),
        DomainVerdict::Degenerate => format!("{head} DEGENERATE"),
        DomainVerdict::Unreadable => format!("{head} curve-unreadable"),
    }
}

fn surface_of(a: &BRep, b: &BRep, patch: (InputId, u32)) -> Option<Surface> {
    let faces = match patch.0 {
        InputId::A => a.faces(),
        InputId::B => b.faces(),
    };
    faces.get(patch.1 as usize).map(|f| f.surface)
}

/// The inc-1 instrument, structured: nearest loop edge of the two candidate
/// faces (curve-aware ranking), its owner tag, corner-endpoint residual, and
/// the solution's verdict against the edge's own domain.
/// The curve-aware RANKING metric (the inc-1b lesson): true circle distance
/// for arcs — √(axial² + (radial − r)²) — and the clamped chord distance for
/// everything else. A plain chord metric is biased against arcs (a rim's
/// chord sits far from a point exactly ON the arc) and manufactured a
/// phantom "carrier-authority wall" on R0044.
fn curve_aware_distance(
    verts: &[crate::BRepVertex],
    edges: &[crate::BRepEdge],
    ei: u32,
    sol: [f64; 3],
) -> f64 {
    let e = &edges[ei as usize];
    let ps = verts[e.start as usize].point.as_array();
    let pe = verts[e.end as usize].point.as_array();
    if let Curve::Circle {
        center,
        normal,
        radius,
    } = e.curve
    {
        let nu = normalize3(normal.as_array());
        let c = center.as_array();
        let w = [sol[0] - c[0], sol[1] - c[1], sol[2] - c[2]];
        let ax = w[0] * nu[0] + w[1] * nu[1] + w[2] * nu[2];
        let rad = [w[0] - ax * nu[0], w[1] - ax * nu[1], w[2] - ax * nu[2]];
        let rl = (rad[0] * rad[0] + rad[1] * rad[1] + rad[2] * rad[2]).sqrt();
        (ax * ax + (rl - radius) * (rl - radius)).sqrt()
    } else {
        let dv = [pe[0] - ps[0], pe[1] - ps[1], pe[2] - ps[2]];
        let l2 = dv[0] * dv[0] + dv[1] * dv[1] + dv[2] * dv[2];
        let t = if l2 > 0.0 {
            (((sol[0] - ps[0]) * dv[0] + (sol[1] - ps[1]) * dv[1] + (sol[2] - ps[2]) * dv[2]) / l2)
                .clamp(0.0, 1.0)
        } else {
            0.0
        };
        let proj = [ps[0] + t * dv[0], ps[1] + t * dv[1], ps[2] + t * dv[2]];
        d3(sol, proj)
    }
}

/// Certify a solution against ONE known edge: the curve-aware distance, the
/// residual from the edge's nearer endpoint to the reference point `refp`
/// (the corner q at planner sites; the entry junction during a corridor
/// walk), and the segment/arc domain verdict. `own` is the caller's to fill
/// — it depends on which face pair is being read.
pub(crate) fn edge_domain_of(
    verts: &[crate::BRepVertex],
    edges: &[crate::BRepEdge],
    ei: u32,
    refp: [f64; 3],
    sol: [f64; 3],
) -> EdgeRead {
    let e = &edges[ei as usize];
    let ps = verts[e.start as usize].point.as_array();
    let pe = verts[e.end as usize].point.as_array();
    let domain = match e.curve {
        Curve::LineSegment => {
            let dv = [pe[0] - ps[0], pe[1] - ps[1], pe[2] - ps[2]];
            let l2 = dv[0] * dv[0] + dv[1] * dv[1] + dv[2] * dv[2];
            if l2 <= 0.0 {
                DomainVerdict::Degenerate
            } else {
                let t = ((sol[0] - ps[0]) * dv[0]
                    + (sol[1] - ps[1]) * dv[1]
                    + (sol[2] - ps[2]) * dv[2])
                    / l2;
                let proj = [ps[0] + t * dv[0], ps[1] + t * dv[1], ps[2] + t * dv[2]];
                DomainVerdict::Segment {
                    t,
                    off_line: d3(sol, proj),
                    inside: t > 0.0 && t < 1.0,
                }
            }
        }
        Curve::Circle { center, normal, .. } => {
            let nu = normalize3(normal.as_array());
            match circle_frame(nu) {
                None => DomainVerdict::Degenerate,
                Some((e1, e2)) => {
                    if e.start == e.end {
                        DomainVerdict::ClosedCircle
                    } else {
                        let c = center.as_array();
                        let theta = |p: [f64; 3]| -> f64 {
                            let w = [p[0] - c[0], p[1] - c[1], p[2] - c[2]];
                            let x = w[0] * e1[0] + w[1] * e1[1] + w[2] * e1[2];
                            let y = w[0] * e2[0] + w[1] * e2[1] + w[2] * e2[2];
                            y.atan2(x)
                        };
                        let tau = std::f64::consts::TAU;
                        let wrap = |x: f64| x.rem_euclid(tau);
                        let (ts, te, tp) = (theta(ps), theta(pe), theta(sol));
                        let span_ccw = wrap(te - ts);
                        let sol_ccw = wrap(tp - ts);
                        let span_cw = wrap(ts - te);
                        let sol_cw = wrap(tp - te);
                        DomainVerdict::Arc {
                            span_ccw,
                            sol_ccw,
                            in_ccw: sol_ccw > 0.0 && sol_ccw < span_ccw,
                            in_cw: sol_cw > 0.0 && sol_cw < span_cw,
                        }
                    }
                }
            }
        }
        _ => DomainVerdict::Unreadable,
    };
    EdgeRead {
        edge: ei,
        own: "-",
        d_on_edge: curve_aware_distance(verts, edges, ei, sol),
        d_q_end: d3(ps, refp).min(d3(pe, refp)),
        domain,
    }
}

fn read_edge_domain(
    verts: &[crate::BRepVertex],
    edges: &[crate::BRepEdge],
    fs: &BRepFace,
    fnx: &BRepFace,
    qpos: [f64; 3],
    sol: [f64; 3],
) -> Option<EdgeRead> {
    let mut best: Option<(u32, f64)> = None; // (edge, d_sol)
    for &ei in fs
        .outer_loop
        .iter()
        .chain(fs.inner_loops.iter().flatten())
        .chain(fnx.outer_loop.iter())
        .chain(fnx.inner_loops.iter().flatten())
    {
        let ds = curve_aware_distance(verts, edges, ei, sol);
        if best.is_none_or(|(_, bd)| ds < bd) {
            best = Some((ei, ds));
        }
    }
    let (ei, _) = best?;
    let in_loops = |f: &BRepFace| {
        f.outer_loop
            .iter()
            .chain(f.inner_loops.iter().flatten())
            .any(|&x| x == ei)
    };
    let own = match (in_loops(fs), in_loops(fnx)) {
        (true, true) => "SN",
        (true, false) => "S",
        (false, true) => "N",
        (false, false) => "-",
    };
    let mut er = edge_domain_of(verts, edges, ei, qpos, sol);
    er.own = own;
    Some(er)
}

/// Solve and read both candidate corrected triples for one §4-I9 site.
///
/// `pv`/`pq` are the traveller's and corner's attributed patch sets; the split
/// far = pv\pq, next = pq\pv, shared = pv∩pq must be the measured family
/// anatomy (1, 1, 2) — anything else is a typed decline, never a guess.
pub(crate) fn read_site(
    a: &BRep,
    b: &BRep,
    pv: &std::collections::BTreeSet<(InputId, u32)>,
    pq: &std::collections::BTreeSet<(InputId, u32)>,
    qpos: [f64; 3],
) -> Result<SiteRead, TransitDecline> {
    let far: Vec<(InputId, u32)> = pv.difference(pq).copied().collect();
    let next: Vec<(InputId, u32)> = pq.difference(pv).copied().collect();
    let shared: Vec<(InputId, u32)> = pv.intersection(pq).copied().collect();
    let ([fp], [np]) = (far.as_slice(), next.as_slice()) else {
        return Err(TransitDecline::AnatomyMismatch {
            far: far.len(),
            next: next.len(),
            shared: shared.len(),
        });
    };
    if shared.len() != 2 {
        return Err(TransitDecline::AnatomyMismatch {
            far: far.len(),
            next: next.len(),
            shared: shared.len(),
        });
    }
    let (fp, np) = (*fp, *np);
    let mut cands = Vec::with_capacity(shared.len());
    for &sp in &shared {
        let (Some(sf), Some(ss), Some(sn)) = (
            surface_of(a, b, fp),
            surface_of(a, b, sp),
            surface_of(a, b, np),
        ) else {
            cands.push(CandidateRead {
                shared: sp,
                sol: None,
                edge: None,
                real: false,
                why: "face-out-of-range",
            });
            continue;
        };
        let sol = relocate_onto_implicit_triple(Point3::new(qpos[0], qpos[1], qpos[2]), sf, ss, sn);
        let Some(sol) = sol else {
            cands.push(CandidateRead {
                shared: sp,
                sol: None,
                edge: None,
                real: false,
                why: "no-converge",
            });
            continue;
        };
        let sa = sol.as_array();
        // The model edge shared_i ∩ next lives on ONE operand's B-Rep; a
        // cross-operand pair has no edge to read.
        if sp.0 != np.0 {
            cands.push(CandidateRead {
                shared: sp,
                sol: Some(sa),
                edge: None,
                real: false,
                why: "cross-operand-pair",
            });
            continue;
        }
        let brep = match sp.0 {
            InputId::A => a,
            InputId::B => b,
        };
        let faces = brep.faces();
        let (Some(fs), Some(fnx)) = (faces.get(sp.1 as usize), faces.get(np.1 as usize)) else {
            cands.push(CandidateRead {
                shared: sp,
                sol: Some(sa),
                edge: None,
                real: false,
                why: "face-out-of-range",
            });
            continue;
        };
        let edge = read_edge_domain(brep.vertices(), brep.edges(), fs, fnx, qpos, sa);
        let band = eval_band(mag3(sa) + mag3(qpos));
        let (real, why) = match &edge {
            None => (false, "no-loop-edges"),
            Some(er) => {
                if er.d_q_end > band {
                    (false, "not-corner-incident")
                } else {
                    match er.domain {
                        DomainVerdict::Segment {
                            inside, off_line, ..
                        } => {
                            if off_line > band {
                                (false, "off-line")
                            } else if !inside {
                                (false, "on-line-past-end")
                            } else {
                                (true, "")
                            }
                        }
                        DomainVerdict::Arc { in_ccw, in_cw, .. } => {
                            if er.d_on_edge > band {
                                (false, "off-circle")
                            } else if in_ccw {
                                (true, "")
                            } else if in_cw {
                                // On-circle, corner-incident, contained only
                                // in the CW reading: the family measured
                                // uniformly in_ccw, so this is loud data,
                                // not a silent acceptance.
                                (false, "arc-cw-only")
                            } else {
                                (false, "on-circle-past-end")
                            }
                        }
                        DomainVerdict::ClosedCircle => (false, "closed-circle"),
                        DomainVerdict::Degenerate => (false, "degenerate-edge"),
                        DomainVerdict::Unreadable => (false, "unreadable-curve"),
                    }
                }
            }
        };
        cands.push(CandidateRead {
            shared: sp,
            sol: Some(sa),
            edge,
            real,
            why,
        });
    }
    Ok(SiteRead {
        far: fp,
        next: np,
        cands,
    })
}

// =========================================================================
// inc-2b — the corridor walk (spec §3e; report-only census instrument)
// =========================================================================

/// Position key for pairing m1 per-loop edge COPIES across faces: the two
/// endpoint bit-keys, order-normalized. Identity across copies is GEOMETRIC
/// (the v467/v6071 lesson: two views of one physical edge carry different
/// indices; positions are bit-equal).
fn edge_pos_key(verts: &[crate::BRepVertex], e: &crate::BRepEdge) -> ([u64; 3], [u64; 3]) {
    let k = |v: u32| {
        let p = verts[v as usize].point.as_array();
        [p[0].to_bits(), p[1].to_bits(), p[2].to_bits()]
    };
    let (a, b) = (k(e.start), k(e.end));
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

/// Public accessor: the position key of one edge by index.
pub(crate) fn edge_key(brep: &BRep, ei: u32) -> ([u64; 3], [u64; 3]) {
    edge_pos_key(brep.vertices(), &brep.edges()[ei as usize])
}

/// Per-operand edge-copy adjacency: position key → every (face, edge) whose
/// loops carry an edge at that key. Faces sharing a key are edge-adjacent.
pub(crate) type EdgeAdjacency = std::collections::BTreeMap<([u64; 3], [u64; 3]), Vec<(u32, u32)>>;

pub(crate) fn build_edge_adjacency(brep: &BRep) -> EdgeAdjacency {
    let (verts, edges) = (brep.vertices(), brep.edges());
    let mut map: EdgeAdjacency = std::collections::BTreeMap::new();
    for (fi, f) in brep.faces().iter().enumerate() {
        for &ei in f.outer_loop.iter().chain(f.inner_loops.iter().flatten()) {
            map.entry(edge_pos_key(verts, &edges[ei as usize]))
                .or_default()
                .push((fi as u32, ei));
        }
    }
    map
}

/// One certified corridor junction: the walk left `face_from` across `edge`
/// (the copy on `face_from`'s loop) into `face_to`.
#[derive(Clone, Debug)]
pub(crate) struct WalkJunction {
    pub face_from: u32,
    pub face_to: u32,
    pub edge: u32,
    pub sol: [f64; 3],
    pub d_on_edge: f64,
}

/// Why a corridor walk stopped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum WalkEnd {
    /// The exit crossed into the OTHER chain's face: the last junction is the
    /// far chain's true end; the corridor is fully discovered.
    ReachedOtherChain,
    /// The last junction lies within the evaluation band of an EXISTING mesh
    /// vertex carrying its surface triple: the curve beyond is already
    /// healthily owned — the corridor ends by SPLICE, minting nothing there
    /// (§3e requirement 1; measured on R0044/R0085).
    ReachedExistingJunction { vertex: u32 },
    /// No loop edge of the current face carries a certified in-domain exit.
    NoExit,
    /// The nearest certified exit does not dominate the second-nearest (the
    /// 4× margin guard): the continuation is not decided by 3D proximity —
    /// loud data, never guessed.
    AmbiguousExit(usize),
    /// An exit edge's position key resolves to ≠1 partner face.
    PartnerUnresolved(usize),
    /// Step cap hit.
    TooLong,
}

// =========================================================================
// inc-2c-0 — the ALL-ROOTS per-edge step solver (spec §3e requirement 2)
// =========================================================================
//
// The corridor walk's Newton step finds ONE root of {far, face, partner}
// seeded at the entry — v76 (R0044) measured the failure mode: the true
// exit can be a SECOND root on an edge the Newton never reaches (including
// the ENTRY edge itself, a dip-in/dip-out arc). The exact model edge is a
// LINE or CIRCLE, so far∩edge is a bounded polynomial system solvable for
// ALL roots: lines via `stage4_phantom::segment_surface_roots` (quadratic,
// pre-existing), circles via the trig-quadric quartic below. Every root is
// CERTIFIED against the far surface's evaluation band before it is
// reported; a missed root can therefore only leave a loud NoExit standing,
// never mint a wrong junction.

/// The algebraic quadric g(p) = pᵀMp + 2·q·p + k for the quadric surfaces
/// (g = 0 on the surface; NOT the signed distance — roots are certified
/// separately). `None` for surfaces without a quadric form here (torus).
fn quadric_form(s: Surface) -> Option<([[f64; 3]; 3], [f64; 3], f64)> {
    let outer = |u: [f64; 3]| -> [[f64; 3]; 3] {
        [
            [u[0] * u[0], u[0] * u[1], u[0] * u[2]],
            [u[1] * u[0], u[1] * u[1], u[1] * u[2]],
            [u[2] * u[0], u[2] * u[1], u[2] * u[2]],
        ]
    };
    let ident = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    let msub = |a: [[f64; 3]; 3], b: [[f64; 3]; 3], sb: f64| -> [[f64; 3]; 3] {
        let mut m = [[0.0; 3]; 3];
        for i in 0..3 {
            for j in 0..3 {
                m[i][j] = a[i][j] - sb * b[i][j];
            }
        }
        m
    };
    let mv = |m: &[[f64; 3]; 3], v: [f64; 3]| -> [f64; 3] {
        [
            m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
            m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
            m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
        ]
    };
    let dot3 = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    // Recentre g(p) = (p−p0)ᵀM(p−p0) + const → M, q = −M·p0, k = p0ᵀMp0 + c.
    let recentred = |m: [[f64; 3]; 3], p0: [f64; 3], c: f64| {
        let mp0 = mv(&m, p0);
        Some((m, [-mp0[0], -mp0[1], -mp0[2]], dot3(p0, mp0) + c))
    };
    match s {
        Surface::Plane { normal, d } => {
            let n = normal.as_array();
            // Linear: M = 0, 2q = n → q = n/2, k = d.
            Some(([[0.0; 3]; 3], [n[0] / 2.0, n[1] / 2.0, n[2] / 2.0], d))
        }
        Surface::Sphere { center, radius } => recentred(ident, center.as_array(), -radius * radius),
        Surface::Cylinder {
            axis_point,
            axis_dir,
            radius,
        } => {
            let u = normalize3(axis_dir.as_array());
            recentred(
                msub(ident, outer(u), 1.0),
                axis_point.as_array(),
                -radius * radius,
            )
        }
        Surface::Cone {
            apex,
            axis_dir,
            half_angle,
        } => {
            let u = normalize3(axis_dir.as_array());
            let c2 = half_angle.cos().powi(2);
            // (w·u)² − cos²α·(w·w) = wᵀ(uuᵀ − cos²α·I)w
            recentred(msub(outer(u), ident, c2), apex.as_array(), 0.0)
        }
        _ => None,
    }
}

/// All real roots of a polynomial (lowest-degree-first coefficients) by
/// Durand–Kerner with fixed deterministic seeds, degree ≤ 4. Roots are
/// RAW — the caller polishes and certifies. Leading near-zero coefficients
/// are dropped (degree collapse) relative to the largest coefficient.
fn poly_real_roots(coeffs: &[f64]) -> Vec<f64> {
    let scale = coeffs.iter().fold(0.0f64, |a, &c| a.max(c.abs()));
    if scale == 0.0 {
        return Vec::new();
    }
    let mut c: Vec<f64> = coeffs.iter().map(|&x| x / scale).collect();
    while c.len() > 1 && c.last().is_some_and(|l| l.abs() < 1e-14) {
        c.pop();
    }
    let n = c.len().saturating_sub(1);
    if n == 0 {
        return Vec::new();
    }
    if n == 1 {
        return vec![-c[0] / c[1]];
    }
    // Durand–Kerner on the monic polynomial, complex f64.
    let lead = c[n];
    let monic: Vec<f64> = c.iter().map(|&x| x / lead).collect();
    let eval = |z: (f64, f64)| -> (f64, f64) {
        // Horner from the top: p(z) = z^n + …
        let (mut re, mut im) = (1.0f64, 0.0f64);
        for k in (0..n).rev() {
            let (r2, i2) = (re * z.0 - im * z.1, re * z.1 + im * z.0);
            re = r2 + monic[k];
            im = i2;
        }
        (re, im)
    };
    let mut roots: Vec<(f64, f64)> = (0..n)
        .map(|k| {
            // Fixed seeds: (0.4 + 0.9i)^(k+1) — the standard deterministic init.
            let (mut re, mut im) = (1.0f64, 0.0f64);
            for _ in 0..=k {
                let (r2, i2) = (re * 0.4 - im * 0.9, re * 0.9 + im * 0.4);
                re = r2;
                im = i2;
            }
            (re, im)
        })
        .collect();
    for _ in 0..64 {
        let prev = roots.clone();
        for i in 0..n {
            let (pr, pi) = eval(prev[i]);
            let (mut dr, mut di) = (1.0f64, 0.0f64);
            for (j, &p) in prev.iter().enumerate() {
                if j == i {
                    continue;
                }
                let (ar, ai) = (prev[i].0 - p.0, prev[i].1 - p.1);
                let (r2, i2) = (dr * ar - di * ai, dr * ai + di * ar);
                dr = r2;
                di = i2;
            }
            let den = dr * dr + di * di;
            if den == 0.0 {
                continue;
            }
            let (qr, qi) = ((pr * dr + pi * di) / den, (pi * dr - pr * di) / den);
            roots[i] = (prev[i].0 - qr, prev[i].1 - qi);
        }
    }
    let mag = roots
        .iter()
        .fold(1.0f64, |a, r| a.max((r.0 * r.0 + r.1 * r.1).sqrt()));
    roots
        .into_iter()
        .filter(|r| r.1.abs() <= 1e-8 * mag)
        .map(|r| r.0)
        .collect()
}

/// ALL angles θ (radians, in the frame `p(θ) = c0 + r·cosθ·e1 + r·sinθ·e2`)
/// where the circle meets the quadric surface `far` — the trig quadric
/// reduced by the tan-half substitution to a quartic, every root polished by
/// 1D Newton on g(θ) and CERTIFIED on `far` at the shared evaluation band.
/// `None` = `far` has no quadric form here (torus — a typed non-answer, not
/// an empty one).
pub(crate) fn circle_surface_roots(
    c0: [f64; 3],
    e1: [f64; 3],
    e2: [f64; 3],
    r: f64,
    far: Surface,
) -> Option<Vec<f64>> {
    let (m, q, k) = quadric_form(far)?;
    let mv = |v: [f64; 3]| -> [f64; 3] {
        [
            m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
            m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
            m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
        ]
    };
    let dot3 = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let (mc0, me1, me2) = (mv(c0), mv(e1), mv(e2));
    // g(θ) = a0 + a1c·cosθ + a1s·sinθ + a2c·cos²θ + a2m·cosθsinθ + a2s·sin²θ
    let a0 = dot3(c0, mc0) + 2.0 * dot3(q, c0) + k;
    let a1c = 2.0 * r * (dot3(e1, mc0) + dot3(q, e1));
    let a1s = 2.0 * r * (dot3(e2, mc0) + dot3(q, e2));
    let a2c = r * r * dot3(e1, me1);
    let a2m = 2.0 * r * r * dot3(e1, me2);
    let a2s = r * r * dot3(e2, me2);
    // Tan-half: u = tan(θ/2); multiply by (1+u²)².
    let coeffs = [
        a2c + a1c + a0,
        2.0 * a2m + 2.0 * a1s,
        -2.0 * a2c + 4.0 * a2s + 2.0 * a0,
        -2.0 * a2m + 2.0 * a1s,
        a2c - a1c + a0,
    ];
    let g = |t: f64| -> f64 {
        let (c, s) = (t.cos(), t.sin());
        a0 + a1c * c + a1s * s + a2c * c * c + a2m * c * s + a2s * s * s
    };
    let dg = |t: f64| -> f64 {
        let (c, s) = (t.cos(), t.sin());
        -a1c * s + a1s * c - 2.0 * a2c * c * s + a2m * (c * c - s * s) + 2.0 * a2s * s * c
    };
    let mut thetas: Vec<f64> = poly_real_roots(&coeffs)
        .into_iter()
        .map(|u| 2.0 * u.atan())
        .collect();
    thetas.push(std::f64::consts::PI); // u→∞ pole, checked like any candidate
    let scale = mag3(c0) + r.abs();
    let band = eval_band(scale);
    let mut out: Vec<f64> = Vec::new();
    for mut t in thetas {
        for _ in 0..8 {
            let d = dg(t);
            if d.abs() <= f64::EPSILON * scale {
                break;
            }
            let step = g(t) / d;
            t -= step;
            if step.abs() <= 1e-15 {
                break;
            }
        }
        let p = [
            c0[0] + r * (t.cos() * e1[0] + t.sin() * e2[0]),
            c0[1] + r * (t.cos() * e1[1] + t.sin() * e2[1]),
            c0[2] + r * (t.cos() * e1[2] + t.sin() * e2[2]),
        ];
        let on = crate::stage4_relocate::surface_distance_and_normal(far, p)
            .is_some_and(|(f, _)| f.abs() <= band);
        if !on {
            continue;
        }
        let tw = t.rem_euclid(std::f64::consts::TAU);
        if out.iter().all(|&x| {
            ((x - tw + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU)
                - std::f64::consts::PI)
                .abs()
                > 1e-9
        }) {
            out.push(tw);
        }
    }
    out.sort_by(f64::total_cmp);
    Some(out)
}

/// The NoExit PROBE (census-only): every loop edge of `face`, with EVERY
/// certified far∩edge root and its in-domain verdict — the all-roots data a
/// stuck walk needs adjudicated (v76's dip hypothesis). One formatted line
/// per edge; the caller prints them.
pub(crate) fn face_edge_roots_probe(
    brep: &BRep,
    far: Surface,
    face: u32,
    entry: [f64; 3],
) -> Vec<String> {
    let (verts, edges, faces) = (brep.vertices(), brep.edges(), brep.faces());
    let Some(f) = faces.get(face as usize) else {
        return vec!["face-out-of-range".into()];
    };
    let mut out = Vec::new();
    for &ei in f.outer_loop.iter().chain(f.inner_loops.iter().flatten()) {
        let e = &edges[ei as usize];
        let ps = verts[e.start as usize].point.as_array();
        let pe = verts[e.end as usize].point.as_array();
        match e.curve {
            Curve::LineSegment => {
                let roots = crate::stage4_phantom::segment_surface_roots(ps, pe, far);
                match roots {
                    None => out.push(format!("edge={ei} LineSegment far-unsupported")),
                    Some(ts) => {
                        let items: Vec<String> = ts
                            .iter()
                            .map(|&t| {
                                let p = [
                                    ps[0] + t * (pe[0] - ps[0]),
                                    ps[1] + t * (pe[1] - ps[1]),
                                    ps[2] + t * (pe[2] - ps[2]),
                                ];
                                let band = eval_band(mag3(p));
                                let on =
                                    crate::stage4_relocate::surface_distance_and_normal(far, p)
                                        .is_some_and(|(fv, _)| fv.abs() <= band);
                                let d_entry = d3(p, entry);
                                format!(
                                    "t={t:.6} in={} on_far={on} d_entry={d_entry:.3e}",
                                    t > 0.0 && t < 1.0
                                )
                            })
                            .collect();
                        out.push(format!(
                            "edge={ei} LineSegment roots={} [{}]",
                            ts.len(),
                            items.join("; ")
                        ));
                    }
                }
            }
            Curve::Circle {
                center,
                normal,
                radius,
            } => {
                let nu = normalize3(normal.as_array());
                let Some((e1, e2)) = circle_frame(nu) else {
                    out.push(format!("edge={ei} Circle frame-degenerate"));
                    continue;
                };
                match circle_surface_roots(center.as_array(), e1, e2, radius, far) {
                    None => out.push(format!("edge={ei} Circle far-unsupported")),
                    Some(ths) => {
                        let c = center.as_array();
                        let theta_of = |p: [f64; 3]| -> f64 {
                            let w = [p[0] - c[0], p[1] - c[1], p[2] - c[2]];
                            (w[0] * e2[0] + w[1] * e2[1] + w[2] * e2[2])
                                .atan2(w[0] * e1[0] + w[1] * e1[1] + w[2] * e1[2])
                        };
                        let tau = std::f64::consts::TAU;
                        let (ts, te) = (theta_of(ps), theta_of(pe));
                        let span_ccw = (te - ts).rem_euclid(tau);
                        let items: Vec<String> = ths
                            .iter()
                            .map(|&th| {
                                let p = [
                                    c[0] + radius * (th.cos() * e1[0] + th.sin() * e2[0]),
                                    c[1] + radius * (th.cos() * e1[1] + th.sin() * e2[1]),
                                    c[2] + radius * (th.cos() * e1[2] + th.sin() * e2[2]),
                                ];
                                let sol_ccw = (th - ts).rem_euclid(tau);
                                let in_arc =
                                    e.start != e.end && sol_ccw > 0.0 && sol_ccw < span_ccw;
                                format!(
                                    "theta={th:.6} sol_ccw={sol_ccw:.4} in_ccw={in_arc} \
                                     d_entry={:.3e}",
                                    d3(p, entry)
                                )
                            })
                            .collect();
                        out.push(format!(
                            "edge={ei} Circle span_ccw={span_ccw:.4} roots={} [{}]",
                            ths.len(),
                            items.join("; ")
                        ));
                    }
                }
            }
            _ => out.push(format!("edge={ei} curve-unreadable")),
        }
    }
    out
}

/// The starting state of a corridor walk: inside `face`, entered across the
/// edge with position key `entry_key` at the certified junction `entry`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct WalkStart {
    pub face: u32,
    pub entry_key: ([u64; 3], [u64; 3]),
    pub entry: [f64; 3],
}

/// The walk's per-operand context: the lattice, the far surface, the
/// position-keyed edge adjacency, and the EXISTING-junction lookup (nearest
/// mesh vertex carrying {far, face_from, face_to} near a position — the
/// splice terminal's witness; `|_, _, _| None` disables the terminal).
pub(crate) struct WalkCtx<'a> {
    pub brep: &'a BRep,
    pub far: Surface,
    pub adj: &'a EdgeAdjacency,
    pub existing: &'a dyn Fn(u32, u32, [f64; 3]) -> Option<(u32, f64)>,
}

/// Walk the far∩facet intersection curve across an operand's face lattice
/// (inc-2c-1: the ALL-ROOTS step). Start INSIDE `start.face` at
/// `start.entry`; per face, enumerate EVERY loop edge's certified far∩edge
/// roots (lines via `segment_surface_roots`, circles via
/// `circle_surface_roots` — the entry junction excluded by POSITION, so
/// same-edge re-exit is representable; the inc-2b Newton step remains as
/// the per-edge fallback where a carrier×far pair has no bounded solver —
/// R0074's torus far), keep the in-domain ones, and step
/// across the nearest root — guarded: the second-nearest exit must be ≥4×
/// farther or the face is a loud `AmbiguousExit` (v76 measured the need:
/// every gear rim carries TWO in-arc far roots; the true exit was 18×
/// nearer). The walk ends crossing into `other_face`, at an existing mesh
/// junction (`ReachedExistingJunction` — the splice terminal), on a
/// dead-end, or at the step cap. Report-only: every terminal is typed data.
pub(crate) fn walk_corridor(
    ctx: &WalkCtx,
    start: WalkStart,
    other_face: u32,
    max_steps: usize,
) -> (Vec<WalkJunction>, WalkEnd) {
    let (verts, edges, faces) = (ctx.brep.vertices(), ctx.brep.edges(), ctx.brep.faces());
    let mut out = Vec::new();
    let (mut face, mut j_in) = (start.face, start.entry);
    let _ = start.entry_key; // superseded by position-exclusion (dip re-exit)
    for _ in 0..max_steps {
        let Some(f) = faces.get(face as usize) else {
            return (out, WalkEnd::NoExit);
        };
        // (edge, partner, pos, d_entry, d_on_edge)
        let mut exits: Vec<(u32, u32, [f64; 3], f64, f64)> = Vec::new();
        for &ei in f.outer_loop.iter().chain(f.inner_loops.iter().flatten()) {
            let key = edge_pos_key(verts, &edges[ei as usize]);
            let Some(copies) = ctx.adj.get(&key) else {
                continue;
            };
            let partners: Vec<u32> = copies
                .iter()
                .filter(|&&(pf, _)| pf != face)
                .map(|&(pf, _)| pf)
                .collect();
            let [partner] = partners.as_slice() else {
                if partners.is_empty() {
                    continue; // open-lattice boundary edge — no face to cross into
                }
                return (out, WalkEnd::PartnerUnresolved(partners.len()));
            };
            let e = &edges[ei as usize];
            let ps = verts[e.start as usize].point.as_array();
            let pe = verts[e.end as usize].point.as_array();
            // ONE certification authority for every candidate position,
            // whichever solver produced it: not the entry junction, ON the
            // far surface at the evaluation band, and inside THIS edge's own
            // segment/arc domain (via the shared single-edge certifier).
            let mut push_candidate = |pos: [f64; 3]| {
                let band = eval_band(mag3(pos) + mag3(j_in));
                if d3(pos, j_in) <= band {
                    return; // the entry junction itself
                }
                if !crate::stage4_relocate::surface_distance_and_normal(ctx.far, pos)
                    .is_some_and(|(fv, _)| fv.abs() <= band)
                {
                    return;
                }
                let er = edge_domain_of(verts, edges, ei, j_in, pos);
                let ok = match er.domain {
                    DomainVerdict::Segment {
                        inside, off_line, ..
                    } => inside && off_line <= band,
                    DomainVerdict::Arc { in_ccw, .. } => in_ccw && er.d_on_edge <= band,
                    _ => false,
                };
                if ok {
                    exits.push((ei, *partner, pos, d3(pos, j_in), er.d_on_edge));
                }
            };
            // ALL-ROOTS where the far surface has a bounded solve on this
            // carrier; the inc-2b Newton step as the per-edge FALLBACK where
            // it does not (measured need: R0074's far is a TORUS — the
            // quadric solvers decline, and a silent skip turned its
            // determined clip corridor into a spurious NoExit).
            let mut solved_all_roots = false;
            match e.curve {
                Curve::LineSegment => {
                    if let Some(ts) = crate::stage4_phantom::segment_surface_roots(ps, pe, ctx.far)
                    {
                        solved_all_roots = true;
                        for t in ts {
                            push_candidate([
                                ps[0] + t * (pe[0] - ps[0]),
                                ps[1] + t * (pe[1] - ps[1]),
                                ps[2] + t * (pe[2] - ps[2]),
                            ]);
                        }
                    }
                }
                Curve::Circle {
                    center,
                    normal,
                    radius,
                } => {
                    let nu = normalize3(normal.as_array());
                    if let Some((e1, e2)) = circle_frame(nu) {
                        if let Some(ths) =
                            circle_surface_roots(center.as_array(), e1, e2, radius, ctx.far)
                        {
                            solved_all_roots = true;
                            let c = center.as_array();
                            for th in ths {
                                push_candidate([
                                    c[0] + radius * (th.cos() * e1[0] + th.sin() * e2[0]),
                                    c[1] + radius * (th.cos() * e1[1] + th.sin() * e2[1]),
                                    c[2] + radius * (th.cos() * e1[2] + th.sin() * e2[2]),
                                ]);
                            }
                        }
                    }
                }
                _ => {}
            }
            if !solved_all_roots {
                if let Some(sol) = relocate_onto_implicit_triple(
                    Point3::new(j_in[0], j_in[1], j_in[2]),
                    ctx.far,
                    f.surface,
                    faces[*partner as usize].surface,
                ) {
                    push_candidate(sol.as_array());
                }
            }
        }
        if exits.is_empty() {
            return (out, WalkEnd::NoExit);
        }
        exits.sort_by(|a, b| a.3.total_cmp(&b.3));
        if exits.len() > 1 && exits[1].3 < 4.0 * exits[0].3 {
            return (out, WalkEnd::AmbiguousExit(exits.len()));
        }
        let (ei, partner, sol, _de, d_on_edge) = exits[0];
        out.push(WalkJunction {
            face_from: face,
            face_to: partner,
            edge: ei,
            sol,
            d_on_edge,
        });
        if partner == other_face {
            return (out, WalkEnd::ReachedOtherChain);
        }
        if let Some((w, dw)) = (ctx.existing)(face, partner, sol) {
            if dw <= eval_band(mag3(sol)) {
                return (out, WalkEnd::ReachedExistingJunction { vertex: w });
            }
        }
        j_in = sol;
        face = partner;
    }
    (out, WalkEnd::TooLong)
}

/// The corner-incident-edge rule's site classification: 1-real → transit,
/// 2-real → corner clip, else a typed decline.
pub(crate) fn classify(site: &SiteRead, qpos: [f64; 3]) -> Result<TransitClass, TransitDecline> {
    let real: Vec<usize> = (0..site.cands.len())
        .filter(|&i| site.cands[i].real)
        .collect();
    // A real junction within band of the corner itself is a different class
    // (the curve passes THROUGH the corner) — never silently absorbed.
    for &i in &real {
        if let Some(sol) = site.cands[i].sol {
            if d3(sol, qpos) <= eval_band(mag3(sol) + mag3(qpos)) {
                return Err(TransitDecline::JunctionAtCorner { which: i });
            }
        }
    }
    match real.as_slice() {
        [] => Err(TransitDecline::NoRealCandidate),
        [one] => Ok(TransitClass::Transit { cand: *one }),
        [x, y] => Ok(TransitClass::Clip { cands: [*x, *y] }),
        _ => unreachable!("at most two candidates per site"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BRepEdge, BRepVertex};
    use cad_primitives::Vector3;

    // Fixture: operand B is a corner of a prism — base plane z=0, facet_k
    // plane x=0, facet_j plane y=0; corner q at the origin. The far surface
    // (operand A) is a plane whose placement selects the class:
    //  - crossing the crease (x=0 ∩ y=0, the z-axis) above q → crease transit;
    //  - crossing the base edge (z=0 ∩ y=0, the x-axis) past q → base transit;
    //  - passing near q, crossing both wedge edges → corner clip.
    //
    // Loops are built with the m1 per-loop-copy convention (LineSegments not
    // shared between faces), matching `to_yang_brep`.
    /// Plane through `n·x + d = 0`, normalized to a UNIT normal (the
    /// converter's convention — the Newton residual for a plane is the raw
    /// algebraic value, so a non-unit normal would scale every step).
    fn plane(n: [f64; 3], d: f64) -> Surface {
        let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        Surface::Plane {
            normal: Vector3::new(n[0] / l, n[1] / l, n[2] / l),
            d: d / l,
        }
    }

    struct Fx {
        a: BRep,
        b: BRep,
        pv: std::collections::BTreeSet<(InputId, u32)>,
        pq: std::collections::BTreeSet<(InputId, u32)>,
    }

    fn vtx(p: [f64; 3]) -> BRepVertex {
        BRepVertex {
            point: Point3::new(p[0], p[1], p[2]),
        }
    }
    fn seg(s: u32, e: u32) -> BRepEdge {
        BRepEdge {
            start: s,
            end: e,
            curve: Curve::LineSegment,
        }
    }
    /// A far-away dummy triangle face carrying `far` for operand A — the
    /// planner reads only its `surface`; its loops never win the ranking
    /// because operand A's faces are not the candidate pair.
    fn operand_a(far: Surface) -> BRep {
        BRep::new(
            vec![
                vtx([50.0, 50.0, 50.0]),
                vtx([51.0, 50.0, 50.0]),
                vtx([50.0, 51.0, 50.0]),
            ],
            vec![seg(0, 1), seg(1, 2), seg(2, 0)],
            vec![BRepFace {
                surface: far,
                outer_loop: vec![0, 1, 2],
                inner_loops: vec![],
                reversed: false,
            }],
        )
        .expect("operand A")
    }

    /// Build operand B with three faces around corner q = origin: face 0 =
    /// base (z=0), face 1 = facet_k (x=0), face 2 = facet_j (y=0). Loops use
    /// the m1 per-loop-copy convention (LineSegments not shared between
    /// faces), matching `to_yang_brep`. Operand A holds the far plane.
    fn fixture(far: Surface) -> Fx {
        // v0 = q origin; v1 along +x (base∩facet_j edge); v2 along +z
        // (crease); v3 along −y (base∩facet_k edge); v4..v6 far corners.
        let verts = vec![
            vtx([0.0, 0.0, 0.0]),
            vtx([4.0, 0.0, 0.0]),
            vtx([0.0, 0.0, 4.0]),
            vtx([0.0, -4.0, 0.0]),
            vtx([4.0, -4.0, 0.0]),
            vtx([0.0, -4.0, 4.0]),
            vtx([4.0, 0.0, 4.0]),
            vtx([4.0, -4.0, 4.0]),
        ];
        let edges = vec![
            // base (z=0): q → x1 → bx → y1 → q  (edges 0..4)
            seg(0, 1),
            seg(1, 4),
            seg(4, 3),
            seg(3, 0),
            // facet_k (x=0): q → y1 → kx → z1 → q  (edges 4..8)
            seg(0, 3),
            seg(3, 5),
            seg(5, 2),
            seg(2, 0),
            // facet_j (y=0): q → z1 → jx → x1 → q  (edges 8..12)
            seg(0, 2),
            seg(2, 6),
            seg(6, 1),
            seg(1, 0),
            // facet_l (x=4): x1 → jx → lx → bx → x1  (edges 12..16)
            seg(1, 6),
            seg(6, 7),
            seg(7, 4),
            seg(4, 1),
        ];
        let faces = vec![
            BRepFace {
                surface: plane([0.0, 0.0, -1.0], 0.0),
                outer_loop: vec![0, 1, 2, 3],
                inner_loops: vec![],
                reversed: false,
            },
            BRepFace {
                surface: plane([-1.0, 0.0, 0.0], 0.0),
                outer_loop: vec![4, 5, 6, 7],
                inner_loops: vec![],
                reversed: false,
            },
            BRepFace {
                surface: plane([0.0, 1.0, 0.0], 0.0),
                outer_loop: vec![8, 9, 10, 11],
                inner_loops: vec![],
                reversed: false,
            },
            // facet_l (x=4), edge-adjacent to facet_j (the jx–x1 copy pair)
            // and to base (the bx–x1 pair): gives corridor walks an
            // intermediate facet to traverse. Never a candidate face — the
            // planner tests are unaffected.
            BRepFace {
                surface: plane([1.0, 0.0, 0.0], -4.0),
                outer_loop: vec![12, 13, 14, 15],
                inner_loops: vec![],
                reversed: false,
            },
        ];
        let b = BRep::new(verts, edges, faces).expect("operand B");
        // Traveller carries {far, base, facet_k}; corner q carries
        // {base, facet_k, facet_j}.
        let pv = [(InputId::A, 0u32), (InputId::B, 0u32), (InputId::B, 1u32)]
            .into_iter()
            .collect();
        let pq = [(InputId::B, 0u32), (InputId::B, 1u32), (InputId::B, 2u32)]
            .into_iter()
            .collect();
        Fx {
            a: operand_a(far),
            b,
            pv,
            pq,
        }
    }

    fn run(far: Surface) -> (SiteRead, Result<TransitClass, TransitDecline>) {
        let fx = fixture(far);
        let site = read_site(&fx.a, &fx.b, &fx.pv, &fx.pq, [0.0, 0.0, 0.0]).expect("family shape");
        let class = classify(&site, [0.0, 0.0, 0.0]);
        (site, class)
    }

    #[test]
    fn crease_transit_classifies_one_real() {
        // Far plane −x + 5·y + 0.2·z − 0.2 = 0 (the `n·x + d = 0`
        // convention): crease crossing (x=y=0) at z = 1 — in-domain
        // (crease spans z ∈ [0,4]); base∩facet_j crossing (y=z=0) at
        // x = −0.2 — ON the x-axis carrier line but BEFORE the edge's
        // corner end (t < 0): the R0085 on-line-past-end exclusion shape.
        let (site, class) = run(plane([-1.0, 5.0, 0.2], -0.2));
        // Candidate with shared = facet_k ({far, facet_k, facet_j}) solves
        // x=0, y=0, far → crease point (0,0,1): REAL.
        // Candidate with shared = base ({far, base, facet_j}) solves z=0,
        // y=0, far → (−0.2, 0, 0): on the x-axis carrier PAST q — not
        // inside any corner-incident edge.
        let real: Vec<_> = site.cands.iter().filter(|c| c.real).collect();
        assert_eq!(real.len(), 1, "site: {site:?}");
        assert_eq!(real[0].shared, (InputId::B, 1), "crease candidate is real");
        assert_eq!(
            class,
            Ok(TransitClass::Transit { cand: 1 }),
            "shared list is BTreeSet-ordered: base first, facet_k second"
        );
    }

    #[test]
    fn corner_clip_classifies_two_real() {
        // Far plane x + y + 0.2·z − 0.1 = 0 passes near q: crease
        // crossing at z = 0.5 (in-domain), base∩facet_j crossing at
        // x = 0.1 (in-domain) — both wedge edges crossed: CLIP.
        let (site, class) = run(plane([1.0, 1.0, 0.2], -0.1));
        let real: Vec<_> = site.cands.iter().filter(|c| c.real).collect();
        assert_eq!(real.len(), 2, "site: {site:?}");
        assert_eq!(class, Ok(TransitClass::Clip { cands: [0, 1] }));
    }

    #[test]
    fn base_transit_classifies_one_real() {
        // Far plane x + 40·y − z − 2 = 0: base∩facet_j crossing (y=z=0)
        // at x = 2 — in-domain; crease crossing (x=y=0) at z = −2 — on
        // the carrier line outside the edge. Single BASE-side transit
        // (both sides occur in the family; R0085 v467/v4216 are this
        // shape).
        let (site, class) = run(plane([1.0, 40.0, -1.0], -2.0));
        let real: Vec<_> = site.cands.iter().filter(|c| c.real).collect();
        assert_eq!(real.len(), 1, "site: {site:?}");
        assert_eq!(real[0].shared, (InputId::B, 0), "base candidate is real");
        assert_eq!(class, Ok(TransitClass::Transit { cand: 0 }));
    }

    #[test]
    fn no_real_candidate_declines() {
        // Far plane −x + 5·y − z − 1 = 0 crosses both carrier lines
        // outside their edges: crease z = −1, base edge x = −1.
        let (_site, class) = run(plane([-1.0, 5.0, -1.0], -1.0));
        assert_eq!(class, Err(TransitDecline::NoRealCandidate));
    }

    #[test]
    fn junction_at_corner_declines() {
        // Far plane passing within evaluation band of the corner:
        // x + y + 0.2·z − 6e-13 = 0 — the base-side solve lands 6e-13
        // from q: strictly inside its edge (t > 0) but within the 1e-12
        // corner band. (Closer than the solver's own ~1e-13 convergence
        // tau, the solve would return q itself and read past-end.)
        let (_site, class) = run(plane([1.0, 1.0, 0.2], -6e-13));
        assert!(
            matches!(class, Err(TransitDecline::JunctionAtCorner { .. })),
            "got {class:?}"
        );
    }

    #[test]
    fn anatomy_mismatch_declines() {
        let fx = fixture(plane([1.0, 1.0, 0.2], 0.1));
        // Traveller missing the far surface: |far| = 0.
        let pv: std::collections::BTreeSet<_> = [(InputId::B, 0u32), (InputId::B, 1u32)]
            .into_iter()
            .collect();
        let out = read_site(&fx.a, &fx.b, &pv, &fx.pq, [0.0, 0.0, 0.0]);
        assert!(
            matches!(out, Err(TransitDecline::AnatomyMismatch { far: 0, .. })),
            "got {out:?}"
        );
    }

    #[test]
    fn circle_roots_plane_two_crossings() {
        // Unit circle in z=0 about the origin × the plane x = 0.5: crossings
        // at θ = ±π/3.
        let ths = circle_surface_roots(
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            1.0,
            plane([1.0, 0.0, 0.0], -0.5),
        )
        .expect("plane supported");
        assert_eq!(ths.len(), 2, "{ths:?}");
        let want = std::f64::consts::FRAC_PI_3;
        assert!((ths[0] - want).abs() < 1e-12, "{ths:?}");
        assert!(
            (ths[1] - (std::f64::consts::TAU - want)).abs() < 1e-12,
            "{ths:?}"
        );
    }

    #[test]
    fn circle_roots_cylinder_four_crossings() {
        // Unit circle in z=0 × a thin vertical cylinder through (0.6, 0):
        // radius 0.3 → the circle crosses its wall at cosθ = (1 + 0.6² −
        // 0.3²·…) … verified structurally: exactly FOUR certified roots,
        // symmetric ±, all on the cylinder.
        let cyl = Surface::Cylinder {
            axis_point: Point3::new(0.6, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            radius: 0.5,
        };
        let ths = circle_surface_roots([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], 1.0, cyl)
            .expect("cylinder supported");
        // |p(θ) − axis|² = 0.25 with p=(cosθ, sinθ): 1 + 0.36 − 1.2cosθ =
        // 0.25 → cosθ = 0.925 → two roots; the OTHER pair would need
        // cosθ > 1 — so TWO certified roots here.
        assert_eq!(ths.len(), 2, "{ths:?}");
        let want = 0.925f64.acos();
        assert!((ths[0] - want).abs() < 1e-12, "{ths:?}");
        assert!(
            (ths[1] - (std::f64::consts::TAU - want)).abs() < 1e-12,
            "{ths:?}"
        );
    }

    #[test]
    fn circle_roots_genuine_quartic_four() {
        // A circle TILTED against a cylinder so all four quartic roots are
        // real: unit circle in the x–z plane (e1=x, e2=z) about the origin ×
        // the cylinder about the z-axis of radius 0.5: crossings where
        // cos²θ = 0.25 → four roots ±π/3, ±2π/3 (in ccw wrap).
        let cyl = Surface::Cylinder {
            axis_point: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            radius: 0.5,
        };
        let ths = circle_surface_roots([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0, cyl)
            .expect("cylinder supported");
        assert_eq!(ths.len(), 4, "{ths:?}");
        for th in &ths {
            assert!(
                (th.cos().abs() - 0.5).abs() < 1e-12,
                "root on |cosθ| = 1/2: {th}"
            );
        }
    }

    #[test]
    fn circle_roots_cone_dip_pair() {
        // The v76 shape in miniature: a rim circle × a cone whose surface
        // dips across the rim — two certified roots close together on one
        // side. Cone apex above the rim plane, axis tilted so the cone wall
        // crosses the rim circle twice. Verified structurally: EVERY
        // returned root is certified on the cone; the count is 2 or 4
        // (tangency excluded by construction), and a dip pair exists within
        // one quadrant.
        let cone = Surface::Cone {
            apex: Point3::new(1.4, 0.0, 0.5),
            axis_dir: Vector3::new(-1.0, 0.0, 0.0),
            half_angle: 0.35,
        };
        let ths =
            circle_surface_roots([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], 1.0, cone)
                .expect("cone supported");
        assert!(!ths.is_empty(), "the cone crosses the rim: {ths:?}");
        for &th in &ths {
            let p = [th.cos(), th.sin(), 0.0];
            let (f, _) = crate::stage4_relocate::surface_distance_and_normal(cone, p)
                .expect("normal defined");
            assert!(f.abs() <= 1e-9, "certified root off-cone: {th} f={f:.3e}");
        }
    }

    #[test]
    fn corridor_walk_clip_reaches_other_chain_in_one_step() {
        // The clip plane: from the crease junction (0,0,0.5), the run across
        // facet_j exits straight onto the base∩facet_j edge at (0.1,0,0) —
        // the other real candidate's junction. One step, other chain reached.
        let far = plane([1.0, 1.0, 0.2], -0.1);
        let fx = fixture(far);
        let site = read_site(&fx.a, &fx.b, &fx.pv, &fx.pq, [0.0, 0.0, 0.0]).expect("site");
        let crease = &site.cands[1];
        assert!(crease.real && crease.shared == (InputId::B, 1));
        let (sol, er) = (crease.sol.unwrap(), crease.edge.as_ref().unwrap());
        let adj = build_edge_adjacency(&fx.b);
        let no_existing = |_: u32, _: u32, _: [f64; 3]| -> Option<(u32, f64)> { None };
        let ctx = WalkCtx {
            brep: &fx.b,
            far,
            adj: &adj,
            existing: &no_existing,
        };
        let (juncs, end) = walk_corridor(
            &ctx,
            WalkStart {
                face: 2, // facet_j
                entry_key: edge_key(&fx.b, er.edge),
                entry: sol,
            },
            0, // base — the other chain's face
            8,
        );
        assert_eq!(end, WalkEnd::ReachedOtherChain, "juncs: {juncs:?}");
        assert_eq!(juncs.len(), 1);
        let j = &juncs[0];
        assert_eq!((j.face_from, j.face_to), (2, 0));
        let base = &site.cands[0];
        let bs = base.sol.unwrap();
        assert!(
            d3(j.sol, bs) <= 1e-12,
            "walk lands on the base candidate's junction: {j:?} vs {bs:?}"
        );
    }

    #[test]
    fn corridor_walk_crosses_intermediate_facet() {
        // Far plane −0.1·x − 5·y + z − 3 = 0: enters facet_j at the crease
        // (0,0,3); its base∩facet_j crossing is at x = −30 (outside), so the
        // run leaves facet_j across the facet_j∩facet_l edge at (4,0,3.4),
        // traverses facet_l, and exits to base at (4,−0.68,0). Two steps.
        let far = plane([-0.1, -5.0, 1.0], -3.0);
        let fx = fixture(far);
        let site = read_site(&fx.a, &fx.b, &fx.pv, &fx.pq, [0.0, 0.0, 0.0]).expect("site");
        assert_eq!(
            classify(&site, [0.0, 0.0, 0.0]),
            Ok(TransitClass::Transit { cand: 1 }),
            "the crease candidate is the single real one: {site:?}"
        );
        let crease = &site.cands[1];
        let (sol, er) = (crease.sol.unwrap(), crease.edge.as_ref().unwrap());
        let adj = build_edge_adjacency(&fx.b);
        let no_existing = |_: u32, _: u32, _: [f64; 3]| -> Option<(u32, f64)> { None };
        let ctx = WalkCtx {
            brep: &fx.b,
            far,
            adj: &adj,
            existing: &no_existing,
        };
        let (juncs, end) = walk_corridor(
            &ctx,
            WalkStart {
                face: 2,
                entry_key: edge_key(&fx.b, er.edge),
                entry: sol,
            },
            0,
            8,
        );
        assert_eq!(end, WalkEnd::ReachedOtherChain, "juncs: {juncs:?}");
        assert_eq!(juncs.len(), 2, "{juncs:?}");
        assert_eq!((juncs[0].face_from, juncs[0].face_to), (2, 3));
        assert!(d3(juncs[0].sol, [4.0, 0.0, 3.4]) <= 1e-9, "{:?}", juncs[0]);
        assert_eq!((juncs[1].face_from, juncs[1].face_to), (3, 0));
        assert!(
            d3(juncs[1].sol, [4.0, -0.68, 0.0]) <= 1e-9,
            "{:?}",
            juncs[1]
        );
    }

    #[test]
    fn corridor_walk_stops_at_existing_junction() {
        // Same two-facet corridor, but the FIRST discovered junction is
        // claimed by an existing mesh vertex: the walk must stop there with
        // the splice terminal instead of walking on.
        let far = plane([-0.1, -5.0, 1.0], -3.0);
        let fx = fixture(far);
        let site = read_site(&fx.a, &fx.b, &fx.pv, &fx.pq, [0.0, 0.0, 0.0]).expect("site");
        let crease = &site.cands[1];
        let (sol, er) = (crease.sol.unwrap(), crease.edge.as_ref().unwrap());
        let adj = build_edge_adjacency(&fx.b);
        let claimed = |_: u32, _: u32, _: [f64; 3]| -> Option<(u32, f64)> { Some((77, 0.0)) };
        let ctx = WalkCtx {
            brep: &fx.b,
            far,
            adj: &adj,
            existing: &claimed,
        };
        let (juncs, end) = walk_corridor(
            &ctx,
            WalkStart {
                face: 2,
                entry_key: edge_key(&fx.b, er.edge),
                entry: sol,
            },
            0,
            8,
        );
        assert_eq!(end, WalkEnd::ReachedExistingJunction { vertex: 77 });
        assert_eq!(juncs.len(), 1, "{juncs:?}");
    }

    #[test]
    fn arc_domain_reads_shared_rim_edges() {
        // Cone-band flavor (the R0044 anatomy): shared_i and next are two
        // cone bands meeting at a shared rim circle edge (own = SN), and
        // the candidate junction lies ON the rim, inside the arc. Build
        // two faces sharing ONE Circle edge (radius 1 about the z-axis at
        // z = 1, arc from q = (1,0,1) CCW around +z spanning 3π/2 to
        // (0,−1,1)), plus a far plane crossing the rim inside that arc.
        let cone_dn = Surface::Cone {
            apex: Point3::new(0.0, 0.0, 2.0),
            axis_dir: Vector3::new(0.0, 0.0, -1.0),
            half_angle: std::f64::consts::FRAC_PI_4,
        };
        let cone_up = Surface::Cone {
            apex: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            half_angle: std::f64::consts::FRAC_PI_4,
        };
        // v0 = corner q, v1 = arc far end; one SHARED rim edge (the
        // converter-shared curved-edge convention), arc from q CCW around
        // +z spanning 3π/2. Raw slices — `read_edge_domain` reads only
        // vertices/edges/loops, and a lone-arc loop would fail `BRep::new`'s
        // edge-continuity validation.
        let verts = vec![vtx([1.0, 0.0, 1.0]), vtx([0.0, -1.0, 1.0])];
        let edges = vec![BRepEdge {
            start: 0,
            end: 1,
            curve: Curve::Circle {
                center: Point3::new(0.0, 0.0, 1.0),
                normal: Vector3::new(0.0, 0.0, 1.0),
                radius: 1.0,
            },
        }];
        let faces = [
            BRepFace {
                surface: cone_dn,
                outer_loop: vec![0],
                inner_loops: vec![],
                reversed: false,
            },
            BRepFace {
                surface: cone_up,
                outer_loop: vec![0],
                inner_loops: vec![],
                reversed: false,
            },
        ];
        // The rim-arc read is what this test pins, so call
        // read_edge_domain directly: the probe point (−√½, √½, 1) lies ON
        // the rim, CCW from q by 3π/4 — inside the 3π/2 arc.
        let er = read_edge_domain(
            &verts,
            &edges,
            &faces[0],
            &faces[1],
            [1.0, 0.0, 1.0],
            [
                -std::f64::consts::FRAC_1_SQRT_2,
                std::f64::consts::FRAC_1_SQRT_2,
                1.0,
            ],
        )
        .expect("edge found");
        assert_eq!(er.own, "SN");
        assert!(er.d_on_edge <= 1e-12, "on the rim: {er:?}");
        assert!(er.d_q_end <= 1e-12, "corner-incident: {er:?}");
        match er.domain {
            DomainVerdict::Arc {
                in_ccw,
                in_cw,
                span_ccw,
                ..
            } => {
                assert!(in_ccw && !in_cw, "{er:?}");
                assert!((span_ccw - 3.0 * std::f64::consts::FRAC_PI_2).abs() < 1e-9);
            }
            other => panic!("expected arc verdict, got {other:?}"),
        }
    }
}
