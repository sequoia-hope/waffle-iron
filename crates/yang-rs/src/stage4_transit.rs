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
fn read_edge_domain(
    verts: &[crate::BRepVertex],
    edges: &[crate::BRepEdge],
    fs: &BRepFace,
    fnx: &BRepFace,
    qpos: [f64; 3],
    sol: [f64; 3],
) -> Option<EdgeRead> {
    let mut best: Option<(u32, f64, f64)> = None; // (edge, d_sol, d_q_end)
    for &ei in fs
        .outer_loop
        .iter()
        .chain(fs.inner_loops.iter().flatten())
        .chain(fnx.outer_loop.iter())
        .chain(fnx.inner_loops.iter().flatten())
    {
        let e = &edges[ei as usize];
        let ps = verts[e.start as usize].point.as_array();
        let pe = verts[e.end as usize].point.as_array();
        let ds = if let Curve::Circle {
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
                (((sol[0] - ps[0]) * dv[0] + (sol[1] - ps[1]) * dv[1] + (sol[2] - ps[2]) * dv[2])
                    / l2)
                    .clamp(0.0, 1.0)
            } else {
                0.0
            };
            let proj = [ps[0] + t * dv[0], ps[1] + t * dv[1], ps[2] + t * dv[2]];
            d3(sol, proj)
        };
        let dq = d3(ps, qpos).min(d3(pe, qpos));
        if best.is_none_or(|(_, bd, _)| ds < bd) {
            best = Some((ei, ds, dq));
        }
    }
    let (ei, d_on_edge, d_q_end) = best?;
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
    Some(EdgeRead {
        edge: ei,
        own,
        d_on_edge,
        d_q_end,
        domain,
    })
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
