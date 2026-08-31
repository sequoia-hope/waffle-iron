//! §4.5.1 CORNER-TRANSIT APPLY — inc-2c-3b (spec
//! `specs/yang_451_corner_transit.md` §3h/§3h-3a).
//!
//! 3b-0 (this file's pure half): the CORRECTED-CYCLE planner. The -CYCLES
//! census measured every surgery as one uniform primitive — a DIRECTED
//! SUB-PATH REPLACEMENT on a component's boundary cycle: walk forward from
//! a surviving vertex, consume removable interior (the phantom, and
//! vertices strictly on the far surface's removed side), stop at a
//! surviving vertex, and splice the corridor path in with the same
//! endpoints and direction. Measured shapes it must reproduce (R0011):
//!
//! - FAR patch: the hole cycle [.. v43 v42[P] v39 ..] — replace the
//!   phantom between its two attachment neighbours with the full corridor
//!   path (junction mints + run vertices), oriented by the neighbours'
//!   junction attachments.
//! - TERMINAL-OUTER patch (base): [.. v39 v42[P] q687 v688 v686 | v682 ..]
//!   — from the attachment neighbour v39, through the phantom and the
//!   sign-removed crease corners, across the hosted crease edge
//!   (v686,v682): corrected [.. v39 J0 v682 ..]. q "staying a patch
//!   vertex" is q surviving on the OTHER patches' cycles.
//! - RUN facet: [v271 v682 | v686 | v273 ..] with J0 hosted on (v682,v686)
//!   and J1 on (v686,v273) — from v682 through the sign-removed corner
//!   v686 to v273, via [J0, run samples, J1].
//!
//! Every certificate failure is a typed decline: nothing is guessed, the
//! standing §4-I9 STOP remains the answer (P10). No mesh mutation lives in
//! the planner — 3b-1 consumes [`ComponentPlan`]s.

use crate::stage4_transit::{contract_band, CorridorRepair, JunctionDisposition, RunSource};
use crate::InputId;

fn d3(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}
fn mag3(v: [f64; 3]) -> f64 {
    v[0].abs().max(v[1].abs()).max(v[2].abs())
}

/// A vertex reference in a corrected cycle: an existing mesh vertex, or
/// the k-th NEW vertex of the invocation's mint pool.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CycleRef {
    Old(u32),
    New(u32),
}

/// The invocation-wide mint pool: junction mints and fresh run samples,
/// interned by POSITION at the contract band so a junction shared by two
/// corridors (the v142/v144 rim mint) is ONE new vertex.
#[derive(Default, Debug)]
pub(crate) struct MintPool {
    pub verts: Vec<[f64; 3]>,
}

impl MintPool {
    pub(crate) fn intern(&mut self, pos: [f64; 3]) -> u32 {
        for (i, &v) in self.verts.iter().enumerate() {
            if d3(v, pos) <= contract_band(mag3(v).max(mag3(pos))) {
                return i as u32;
            }
        }
        self.verts.push(pos);
        (self.verts.len() - 1) as u32
    }
}

/// The full corridor path as cycle references, END-JUNCTION to
/// END-JUNCTION: junction refs (mint-interned or spliced-existing)
/// interleaved with each run's vertices (existing chain ids or interned
/// fresh samples).
pub(crate) fn corridor_path(c: &CorridorRepair, pool: &mut MintPool) -> Vec<CycleRef> {
    let jref =
        |j: &crate::stage4_transit::CorridorJunction, pool: &mut MintPool| match j.disposition {
            JunctionDisposition::Splice { vertex, .. } => CycleRef::Old(vertex),
            JunctionDisposition::Mint => CycleRef::New(pool.intern(j.sol)),
        };
    let mut path = Vec::new();
    path.push(jref(&c.junctions[0], pool));
    for (i, (_, run)) in c.runs.iter().enumerate() {
        match run {
            Ok(RunSource::Samples(s)) => {
                for &p in s {
                    path.push(CycleRef::New(pool.intern(p)));
                }
            }
            Ok(RunSource::Spliced { head, chain, tail }) => {
                for &p in head {
                    path.push(CycleRef::New(pool.intern(p)));
                }
                for &v in chain {
                    path.push(CycleRef::Old(v));
                }
                for &p in tail {
                    path.push(CycleRef::New(pool.intern(p)));
                }
            }
            Err(_) => {} // unreachable behind `applyable()`; keep total
        }
        path.push(jref(&c.junctions[i + 1], pool));
    }
    path
}

/// Removability verdict for one interior vertex of a candidate sub-path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Removability {
    /// A fired phantom of this corridor.
    Phantom,
    /// A chain-end anchor absorbed into its corridor-end junction (spec
    /// §3j, the paper's §4.4.1 near-curve removal): its ring step into
    /// the junction doubles back — the vertex slid PAST the junction, a
    /// non-corner out-of-domain slide §4-I9 cannot fire on.
    Absorbed,
    /// Strictly on the far surface's REMOVED side (|value| above the
    /// evaluation band, removed sign).
    SignRemoved,
    /// Strictly on the KEPT side — walking through it would excise kept
    /// territory.
    SignKept,
    /// Within band of the far surface or unreadable — never decided by
    /// sign.
    Ambiguous,
}

/// One hosted junction on a component: (junction index, host edge).
pub(crate) type HostEdge = (usize, (u32, u32));

/// Typed planner declines.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PlanDecline {
    /// `from` does not appear exactly once across the component's cycles.
    FromNotUnique { from: u32, count: usize },
    /// The forward walk met a vertex that is not removable.
    NotRemovable { at: u32, verdict: Removability },
    /// The walk consumed the whole cycle without reaching `to`.
    ToNotFound { from: u32, to: u32 },
    /// A hosted junction's edge was not found on the component's cycles.
    HostNotFound { junction: usize },
    /// The host edge's endpoints straddle the walk inconsistently: the
    /// walk crossed a host edge that is not the expected one.
    HostMismatch { junction: usize },
    /// The phantom's cycle neighbours do not include exactly one
    /// attachment on each side.
    AttachmentMismatch { phantom: u32 },
}

/// Replace, on `cycles`, the directed sub-path that starts at the unique
/// occurrence of `from`, walks FORWARD consuming interior vertices (each
/// certified by `removable`), and ends at the first occurrence of `to` —
/// splicing `via` between them. Returns the removed interior (for the
/// dropped-vertex ledger). `from` and `to` survive.
pub(crate) fn replace_subpath(
    cycles: &mut [Vec<CycleRef>],
    from: u32,
    to: u32,
    via: &[CycleRef],
    removable: &dyn Fn(u32) -> Removability,
) -> Result<Vec<u32>, PlanDecline> {
    // Locate `from` (must be unique across all cycles).
    let mut hits: Vec<(usize, usize)> = Vec::new();
    for (ci, cy) in cycles.iter().enumerate() {
        for (i, &r) in cy.iter().enumerate() {
            if r == CycleRef::Old(from) {
                hits.push((ci, i));
            }
        }
    }
    let [(ci, start)] = hits.as_slice() else {
        return Err(PlanDecline::FromNotUnique {
            from,
            count: hits.len(),
        });
    };
    let (ci, start) = (*ci, *start);
    let cy = &cycles[ci];
    let n = cy.len();
    let mut removed: Vec<u32> = Vec::new();
    let mut end: Option<usize> = None;
    for step in 1..n {
        let idx = (start + step) % n;
        match cy[idx] {
            CycleRef::Old(v) if v == to => {
                end = Some(idx);
                break;
            }
            CycleRef::Old(v) => match removable(v) {
                Removability::Phantom | Removability::SignRemoved | Removability::Absorbed => {
                    removed.push(v)
                }
                verdict => return Err(PlanDecline::NotRemovable { at: v, verdict }),
            },
            CycleRef::New(_) => {
                // A previously spliced NEW vertex is never walked through.
                return Err(PlanDecline::NotRemovable {
                    at: u32::MAX,
                    verdict: Removability::Ambiguous,
                });
            }
        }
    }
    let Some(end) = end else {
        return Err(PlanDecline::ToNotFound { from, to });
    };
    // Rebuild the cycle: keep [end..=start] (the surviving arc, inclusive
    // of both endpoints), then splice `via` between `from` and `to`:
    // corrected = [from, via..., to, ...surviving interior...].
    let cy = &cycles[ci];
    let mut out: Vec<CycleRef> = Vec::with_capacity(n - removed.len() + via.len());
    out.push(CycleRef::Old(from));
    out.extend(via.iter().copied());
    let mut i = end;
    while i != (start % n) {
        out.push(cy[i]);
        i = (i + 1) % n;
    }
    cycles[ci] = out;
    Ok(removed)
}

/// One affected mesh component's measured boundary, as the census reads it.
#[derive(Clone, Debug)]
pub(crate) struct ComponentInput {
    pub key: (InputId, u32),
    /// The census component id (diagnostic + closure key).
    pub comp: u32,
    /// Directed boundary cycles as vertex chains.
    pub cycles: Vec<Vec<u32>>,
}

/// One component's corrected boundary — the exact input the gated
/// mutation (3b-1) re-CDTs from.
#[derive(Clone, Debug)]
pub(crate) struct ComponentPlan {
    pub key: (InputId, u32),
    pub comp: u32,
    pub corrected: Vec<Vec<CycleRef>>,
    /// Old vertices the surgeries removed from this component's boundary.
    pub removed: Vec<u32>,
}

/// The planner's mesh-side lookups.
pub(crate) struct PlanCtx<'a> {
    /// Signed far-surface value at a mesh vertex, for corridor `k`.
    pub far_value: &'a dyn Fn(usize, u32) -> Option<f64>,
    /// Evaluation band at a mesh vertex.
    pub band: &'a dyn Fn(u32) -> f64,
    /// Mesh vertex position (the §3j absorb certificate reads geometry).
    pub pos: &'a dyn Fn(u32) -> Option<[f64; 3]>,
    /// (corridor, phantom) → its on-curve neighbours with UNIQUE junction
    /// attachment (neighbour, junction index).
    pub attachments: &'a dyn Fn(usize, u32) -> Vec<(u32, usize)>,
    /// (corridor, component) → hosted junctions as (junction index, host
    /// edge (x, y)) — the boundary edge whose segment carries the
    /// junction. Vertex pairs, never positions (edits shift positions).
    pub hosts: &'a dyn Fn(usize, u32) -> Vec<HostEdge>,
}

/// The affected-key set of one corridor: far ∪ run facets ∪ the two
/// terminal-outer patches (spec §3h-3a).
pub(crate) fn affected_keys(c: &CorridorRepair) -> Vec<(InputId, u32)> {
    let mut keys = vec![c.far];
    // inc-2c-3b-5: a SPLICE-disposition end junction mints nothing — the
    // existing curve continues through its vertex, so that end's
    // terminal-outer patch is untouched (measured: R0044's v142/v144
    // corridors splice at j0; their (B,377)/(B,380) outers have no work,
    // and expecting a plan there refused the whole invocation). Mint ends
    // keep the expectation.
    if matches!(c.junctions[0].disposition, JunctionDisposition::Mint) {
        keys.push((c.walk_op, c.junctions[0].faces.0));
    }
    if matches!(
        c.junctions[c.junctions.len() - 1].disposition,
        JunctionDisposition::Mint
    ) {
        keys.push((c.walk_op, c.junctions[c.junctions.len() - 1].faces.1));
    }
    keys.extend(c.runs.iter().map(|&(f, _)| (c.walk_op, f)));
    keys.sort_unstable();
    keys.dedup();
    keys
}

/// inc-2c-3b-2 (spec §3j) — the §4.4.1 near-curve ABSORB at a corridor-end
/// anchor. On cycle `ci` the anchor at `idx` splices against the end
/// junction at `jp`; its chain continuation sits one step in `dir`
/// (−1 = the anchor precedes the phantom, +1 = it follows). While the
/// emitted ring step doubles back at the anchor —
/// dot(anchor − continuation, junction − anchor) < 0 — the anchor slid
/// PAST the junction (measured R0011: defects at exactly −1.0000
/// (v26/v46) vs ≥ +0.456 on every healthy end; d_eps would over-absorb —
/// healthy ends sit at 0.3–0.5·d_eps, so the certificate is the SIGN,
/// never a band): absorb it into the junction and step the anchor back.
/// Unreadable geometry, an absorb landing on a NEW-ref anchor (another
/// corridor's spliced mint — a genuine cross-corridor entanglement), or
/// the depth cap decline typed — nothing is guessed. The CONTINUATION may
/// be a NEW ref (the base curve dips between tooth corridors: a healthy
/// anchor like R0011's v39 sits between its own end junction and the
/// neighbouring corridor's already-spliced mint) — `refpos` resolves both.
fn absorb_anchor(
    cycle: &[CycleRef],
    idx: usize,
    dir: isize,
    jp: [f64; 3],
    refpos: &dyn Fn(CycleRef) -> Option<[f64; 3]>,
) -> Result<(u32, usize, Vec<u32>), PlanDecline> {
    let n = cycle.len() as isize;
    let at = |i: isize| cycle[((i % n + n) % n) as usize];
    let mut i = idx as isize;
    let mut absorbed: Vec<u32> = Vec::new();
    // The walk bound is the CYCLE, not a constant: every absorbed vertex
    // carries its own sign certificate, so the only job here is
    // termination. R0044's corridor #1 measured a 4-deep reversal chain
    // (v102 v90 v91 v92) that the original cap-4 declined one short —
    // overshoot depth is a property of the defect, never a tunable.
    for _ in 0..cycle.len() {
        let CycleRef::Old(w) = at(i) else {
            return Err(PlanDecline::NotRemovable {
                at: absorbed.last().copied().unwrap_or(u32::MAX),
                verdict: Removability::Ambiguous,
            });
        };
        let (Some(pw), Some(pu)) = ((refpos)(at(i)), (refpos)(at(i + dir))) else {
            return Err(PlanDecline::NotRemovable {
                at: w,
                verdict: Removability::Ambiguous,
            });
        };
        let arrive = [pw[0] - pu[0], pw[1] - pu[1], pw[2] - pu[2]];
        let connect = [jp[0] - pw[0], jp[1] - pw[1], jp[2] - pw[2]];
        let nrm = |x: [f64; 3]| (x[0] * x[0] + x[1] * x[1] + x[2] * x[2]).sqrt();
        let (na, nc) = (nrm(arrive), nrm(connect));
        if !(na > 0.0 && nc > 0.0 && na.is_finite() && nc.is_finite()) {
            return Err(PlanDecline::NotRemovable {
                at: w,
                verdict: Removability::Ambiguous,
            });
        }
        let dot = arrive[0] * connect[0] + arrive[1] * connect[1] + arrive[2] * connect[2];
        if dot >= 0.0 {
            return Ok((w, ((i % n + n) % n) as usize, absorbed));
        }
        absorbed.push(w);
        i += dir;
    }
    Err(PlanDecline::NotRemovable {
        at: absorbed.last().copied().unwrap_or(u32::MAX),
        verdict: Removability::Ambiguous,
    })
}

/// Plan one invocation's corrected cycles: every corridor's surgeries on
/// every affected component, as uniform directed sub-path replacements.
/// Declines are typed per (corridor, decline); a declined component emits
/// no plan (the caller's admission rule — every fire consumed, every
/// component planned — keeps the standing STOP otherwise).
pub(crate) fn plan_invocation(
    corridors: &[CorridorRepair],
    components: &[ComponentInput],
    ctx: &PlanCtx,
    pool: &mut MintPool,
) -> (Vec<ComponentPlan>, Vec<(usize, u32, PlanDecline)>) {
    let mut declines: Vec<(usize, u32, PlanDecline)> = Vec::new();
    // Corridor paths + per-junction path offsets (mints interned ONCE
    // across corridors — the SHARED-MINT contract).
    let mut paths: Vec<Vec<CycleRef>> = Vec::new();
    let mut jpos: Vec<Vec<usize>> = Vec::new();
    for c in corridors {
        let path = corridor_path(c, pool);
        let mut pos = vec![0usize];
        let mut at = 0usize;
        for (_, run) in &c.runs {
            let interior = match run {
                Ok(RunSource::Samples(s)) => s.len(),
                Ok(RunSource::Spliced { head, chain, tail }) => {
                    head.len() + chain.len() + tail.len()
                }
                Err(_) => 0,
            };
            at += interior + 1;
            pos.push(at);
        }
        paths.push(path);
        jpos.push(pos);
    }
    // Every mint is interned by the path building above; the snapshot lets
    // the §3j absorb certificate read NEW-ref continuations (a healthy
    // anchor can sit next to a neighbouring corridor's spliced mint).
    let mints: Vec<[f64; 3]> = pool.verts.clone();
    let refpos = |r: CycleRef| -> Option<[f64; 3]> {
        match r {
            CycleRef::Old(v) => (ctx.pos)(v),
            CycleRef::New(i) => mints.get(i as usize).copied(),
        }
    };
    // Removability per corridor: fired phantoms are removable; other
    // vertices by far-surface SIGN, anchored on the corridor's own crossed
    // CORNER (the corner is definitionally between the wrong and the true
    // curve — the removed side). |value| within band never decides.
    let fired: std::collections::BTreeSet<u32> = corridors
        .iter()
        .flat_map(|c| c.phantoms.iter().copied())
        .collect();
    let removed_sign = |k: usize| -> Option<f64> {
        let mut sign: Option<f64> = None;
        for &q in &corridors[k].corners {
            let v = (ctx.far_value)(k, q)?;
            if v.abs() <= (ctx.band)(q) {
                return None;
            }
            match sign {
                None => sign = Some(v.signum()),
                Some(s) if s == v.signum() => {}
                Some(_) => return None, // corners straddle — never guess
            }
        }
        sign
    };
    let mut plans: Vec<ComponentPlan> = Vec::new();
    'comp: for comp in components {
        let mut cycles: Vec<Vec<CycleRef>> = comp
            .cycles
            .iter()
            .map(|cy| cy.iter().map(|&v| CycleRef::Old(v)).collect())
            .collect();
        let mut removed_all: Vec<u32> = Vec::new();
        let mut edited = false;
        for (k, c) in corridors.iter().enumerate() {
            // inc-2c-3b-7: a component whose cycles carry this corridor's
            // PHANTOM is affected regardless of the key formula (the
            // crossed corner's third face — R0044's prism base B:0 holds
            // the twin phantoms; the phantom must vanish everywhere).
            let holds_phantom = c
                .phantoms
                .iter()
                .any(|&p| cycles.iter().any(|cy| cy.contains(&CycleRef::Old(p))));
            if !affected_keys(c).contains(&comp.key) && !holds_phantom {
                continue;
            }
            let Some(rsign) = removed_sign(k) else {
                declines.push((
                    k,
                    comp.comp,
                    PlanDecline::NotRemovable {
                        at: c.corners.first().copied().unwrap_or(u32::MAX),
                        verdict: Removability::Ambiguous,
                    },
                ));
                continue 'comp;
            };
            let removable = |v: u32| -> Removability {
                if fired.contains(&v) {
                    return Removability::Phantom;
                }
                match (ctx.far_value)(k, v) {
                    Some(f) if f.abs() > (ctx.band)(v) => {
                        if f.signum() == rsign {
                            Removability::SignRemoved
                        } else {
                            Removability::SignKept
                        }
                    }
                    _ => Removability::Ambiguous,
                }
            };
            let hosts = (ctx.hosts)(k, comp.comp);
            // Phantoms of this corridor present on this component's cycles.
            let present: Vec<u32> = c
                .phantoms
                .iter()
                .copied()
                .filter(|&p| cycles.iter().any(|cy| cy.contains(&CycleRef::Old(p))))
                .collect();
            let last = c.junctions.len() - 1;
            if !present.is_empty() {
                for &p in &present {
                    // Cycle neighbours of the phantom.
                    let mut pred_succ: Option<(usize, usize, u32, u32)> = None;
                    for (ci, cy) in cycles.iter().enumerate() {
                        if let Some(i) = cy.iter().position(|&r| r == CycleRef::Old(p)) {
                            let n = cy.len();
                            let (CycleRef::Old(a), CycleRef::Old(b)) =
                                (cy[(i + n - 1) % n], cy[(i + 1) % n])
                            else {
                                declines.push((
                                    k,
                                    comp.comp,
                                    PlanDecline::AttachmentMismatch { phantom: p },
                                ));
                                continue 'comp;
                            };
                            pred_succ = Some((ci, i, a, b));
                        }
                    }
                    let Some((p_ci, p_i, pred, succ)) = pred_succ else {
                        declines.push((
                            k,
                            comp.comp,
                            PlanDecline::AttachmentMismatch { phantom: p },
                        ));
                        continue 'comp;
                    };
                    let att = (ctx.attachments)(k, p);
                    let att_of = |v: u32| att.iter().find(|&&(n, _)| n == v).map(|&(_, j)| j);
                    match (att_of(pred), att_of(succ)) {
                        (Some(ja), Some(jb)) => {
                            // Generator A — the far patch: replace the
                            // phantom with the whole corridor path, oriented
                            // pred-junction first. Attachments must be the
                            // corridor's two ENDS.
                            if !((ja == 0 && jb == last) || (ja == last && jb == 0)) {
                                declines.push((
                                    k,
                                    comp.comp,
                                    PlanDecline::AttachmentMismatch { phantom: p },
                                ));
                                continue 'comp;
                            }
                            // §3j: absorb overshot anchors at BOTH ends
                            // before the splice.
                            let ncy = cycles[p_ci].len();
                            let (from_eff, _, abs_a) = match absorb_anchor(
                                &cycles[p_ci],
                                (p_i + ncy - 1) % ncy,
                                -1,
                                c.junctions[ja].sol,
                                &refpos,
                            ) {
                                Ok(x) => x,
                                Err(d) => {
                                    declines.push((k, comp.comp, d));
                                    continue 'comp;
                                }
                            };
                            let (to_eff, _, abs_b) = match absorb_anchor(
                                &cycles[p_ci],
                                (p_i + 1) % ncy,
                                1,
                                c.junctions[jb].sol,
                                &refpos,
                            ) {
                                Ok(x) => x,
                                Err(d) => {
                                    declines.push((k, comp.comp, d));
                                    continue 'comp;
                                }
                            };
                            let absorbed: std::collections::BTreeSet<u32> =
                                abs_a.into_iter().chain(abs_b).collect();
                            let removable_abs = |v: u32| -> Removability {
                                if absorbed.contains(&v) {
                                    Removability::Absorbed
                                } else {
                                    removable(v)
                                }
                            };
                            let via: Vec<CycleRef> = if ja == 0 {
                                paths[k].clone()
                            } else {
                                paths[k].iter().rev().copied().collect()
                            };
                            match replace_subpath(
                                &mut cycles,
                                from_eff,
                                to_eff,
                                &via,
                                &removable_abs,
                            ) {
                                Ok(mut r) => {
                                    edited = true;
                                    removed_all.append(&mut r);
                                }
                                Err(d) => {
                                    declines.push((k, comp.comp, d));
                                    continue 'comp;
                                }
                            }
                        }
                        (one, other) if one.is_some() != other.is_some() => {
                            // Generator B — a B-side patch: one attachment
                            // neighbour `n`, the walk crosses the attached
                            // junction's host edge (x, y). Exactly one of
                            // the two orientations must certify.
                            let (anchor_idx, dir, j) = if let Some(j) = one {
                                let ncy = cycles[p_ci].len();
                                ((p_i + ncy - 1) % ncy, -1isize, j)
                            } else {
                                let ncy = cycles[p_ci].len();
                                ((p_i + 1) % ncy, 1isize, other.expect("one side attached"))
                            };
                            // §3j: absorb an overshot connector anchor.
                            let (n, _, abs_n) = match absorb_anchor(
                                &cycles[p_ci],
                                anchor_idx,
                                dir,
                                c.junctions[j].sol,
                                &refpos,
                            ) {
                                Ok(x) => x,
                                Err(d) => {
                                    declines.push((k, comp.comp, d));
                                    continue 'comp;
                                }
                            };
                            // inc-2c-3b-5: absorb the UNATTACHED flank too —
                            // measured on R0044's v142 (far comp): v141 is an
                            // on-curve overshoot remnant between the mirrored
                            // pair's twin phantoms, past the mint with a
                            // doubled-back ring step — the same §4.4.1
                            // certificate as the connector anchor, on the
                            // other side. Only the absorbed SET is used (the
                            // host edge, not an anchor, bounds that side).
                            let ncy2 = cycles[p_ci].len();
                            let u_idx = if dir < 0 {
                                (p_i + 1) % ncy2
                            } else {
                                (p_i + ncy2 - 1) % ncy2
                            };
                            // BEST-EFFORT: the flank absorb only EXTENDS
                            // the certificate set — a flank that does not
                            // absorb (dot ≥ 0, unreadable, or a New ref)
                            // simply falls through to the sign walk, which
                            // declines loudly if it actually consumes an
                            // uncertified vertex. Never fail the plan here.
                            let abs_u = absorb_anchor(
                                &cycles[p_ci],
                                u_idx,
                                -dir,
                                c.junctions[j].sol,
                                &refpos,
                            )
                            .map(|(_, _, a)| a)
                            .unwrap_or_default();
                            let absorbed: std::collections::BTreeSet<u32> =
                                abs_n.into_iter().chain(abs_u).collect();
                            let removable_abs = |v: u32| -> Removability {
                                if absorbed.contains(&v) {
                                    Removability::Absorbed
                                } else {
                                    removable(v)
                                }
                            };
                            let jhosts: Vec<(u32, u32)> = hosts
                                .iter()
                                .filter(|&&(hj, _)| hj == j)
                                .map(|&(_, e)| e)
                                .collect();
                            if jhosts.is_empty() {
                                declines.push((
                                    k,
                                    comp.comp,
                                    PlanDecline::HostNotFound { junction: j },
                                ));
                                continue 'comp;
                            }
                            let jref = ref_of(&c.junctions[j], &paths[k], &jpos[k], j);
                            // Exactly ONE certifying orientation across all
                            // of j's host edges — anything else is loud.
                            let mut winner: Option<(Vec<Vec<CycleRef>>, Vec<u32>)> = None;
                            let mut count = 0usize;
                            for &(x, y) in &jhosts {
                                let mut try1 = cycles.clone();
                                if let Some(r) =
                                    replace_subpath(&mut try1, n, y, &[jref], &removable_abs)
                                        .ok()
                                        .filter(|r| r.last() == Some(&x) && r.contains(&p))
                                {
                                    count += 1;
                                    winner = Some((try1, r));
                                }
                                let mut try2 = cycles.clone();
                                if let Some(r) =
                                    replace_subpath(&mut try2, x, n, &[jref], &removable_abs)
                                        .ok()
                                        .filter(|r| r.first() == Some(&y) && r.contains(&p))
                                {
                                    count += 1;
                                    winner = Some((try2, r));
                                }
                            }
                            match (count, winner) {
                                (1, Some((cy, mut r))) => {
                                    cycles = cy;
                                    edited = true;
                                    removed_all.append(&mut r);
                                }
                                _ => {
                                    declines.push((
                                        k,
                                        comp.comp,
                                        PlanDecline::HostMismatch { junction: j },
                                    ));
                                    continue 'comp;
                                }
                            }
                        }
                        _ => {
                            // inc-2c-3b-8 (spec §3p): a phantom NEITHER of
                            // whose cycle neighbours resolves to a junction,
                            // on a component with no hosted junction for
                            // this corridor, is the WHOLLY-CONDEMNED
                            // third-face pocket (R0044's B:0 corner sliver
                            // at q=v513 — the rim-domain census REFUTED the
                            // base-leg reading: the candidate junctions sit
                            // out-of-domain beyond the corner, and the far
                            // body swallows the whole pocket). This
                            // component carries no anchor because it keeps
                            // NOTHING; every vertex is removed by the other
                            // components' certified plans. Leave it
                            // unplanned — the driver's closure sweep
                            // consumes it against the invocation's removed
                            // set, and the batch-integrity scan stays the
                            // loud backstop if anything survives. A
                            // component that DOES host this corridor's
                            // junctions keeps the typed decline.
                            if hosts.is_empty() {
                                continue;
                            }
                            declines.push((
                                k,
                                comp.comp,
                                PlanDecline::AttachmentMismatch { phantom: p },
                            ));
                            continue 'comp;
                        }
                    }
                }
            } else if !hosts.is_empty() {
                // Generator C — a run facet: consecutive hosted junctions
                // (j, j+1); the removed arc runs host-to-host through the
                // OUT side; via = the path slice between the junctions.
                let mut pairs: Vec<(HostEdge, HostEdge)> = Vec::new();
                let mut sorted = hosts.clone();
                sorted.sort_unstable_by_key(|&(j, _)| j);
                for w in sorted.windows(2) {
                    if w[1].0 == w[0].0 + 1 {
                        pairs.push((w[0], w[1]));
                    } else if w[1].0 == w[0].0 && w[1].1 != w[0].1 {
                        // inc-2c-3b-5 — the CORNER-CLIP pair (measured on
                        // R0044's v142/v144 mirrored corridors): the
                        // junction is a TRIPLE point whose component
                        // boundary passes it twice — once on each curve it
                        // terminates (the intersection-curve chord and the
                        // crease chord) — with no phantom on the component.
                        // The corner sliver between the two host edges is
                        // excised host-to-host through the SAME junction;
                        // the ja == jb slice below is the single mint. The
                        // sign walk certifies every consumed vertex (the
                        // crossed corner q reads removed, §3i's measured
                        // refutation), and orientation uniqueness holds
                        // exactly as for consecutive pairs.
                        pairs.push((w[0], w[1]));
                    }
                }
                if pairs.is_empty() {
                    declines.push((
                        k,
                        comp.comp,
                        PlanDecline::HostNotFound {
                            junction: sorted.first().map(|&(j, _)| j).unwrap_or(usize::MAX),
                        },
                    ));
                    continue 'comp;
                }
                for &((ja, (xa, ya)), (jb, (xb, yb))) in &pairs {
                    // inc-2c-3b-5 view dedup: a corner-clip whose MINTED
                    // junction is already spliced into this component's
                    // corrected cycle is the SAME excision seen from the
                    // mirrored corridor (the shared-mint identity — R0044's
                    // v142/v144 pair at q=v513); the work is done, and
                    // re-walking it would consume the splice itself. Old
                    // (spliced-existing) refs never skip — they pre-exist
                    // by construction.
                    if ja == jb {
                        let jr = ref_of(&c.junctions[ja], &paths[k], &jpos[k], ja);
                        if matches!(jr, CycleRef::New(_))
                            && cycles.iter().any(|cy| cy.contains(&jr))
                        {
                            continue;
                        }
                    }
                    let slice: Vec<CycleRef> = paths[k][jpos[k][ja]..=jpos[k][jb]].to_vec();
                    let rslice: Vec<CycleRef> = slice.iter().rev().copied().collect();
                    let mut try1 = cycles.clone();
                    let r1 = replace_subpath(&mut try1, xa, yb, &slice, &removable)
                        .ok()
                        .filter(|r| r.first() == Some(&ya) && r.last() == Some(&xb));
                    let mut try2 = cycles.clone();
                    let r2 = replace_subpath(&mut try2, xb, ya, &rslice, &removable)
                        .ok()
                        .filter(|r| r.first() == Some(&yb) && r.last() == Some(&xa));
                    match (r1, r2) {
                        (Some(mut r), None) => {
                            cycles = try1;
                            edited = true;
                            removed_all.append(&mut r);
                        }
                        (None, Some(mut r)) => {
                            cycles = try2;
                            edited = true;
                            removed_all.append(&mut r);
                        }
                        _ => {
                            declines.push((
                                k,
                                comp.comp,
                                PlanDecline::HostMismatch { junction: ja },
                            ));
                            continue 'comp;
                        }
                    }
                }
            }
        }
        if edited {
            removed_all.sort_unstable();
            removed_all.dedup();
            plans.push(ComponentPlan {
                key: comp.key,
                comp: comp.comp,
                corrected: cycles,
                removed: removed_all,
            });
        }
    }
    // inc-2c-3b-9 (A), spec §3q: a vertex removed by ANY plan vanishes from
    // EVERY corrected cycle — the I13 interference-group rule at the plan
    // level. Generator B keeps host-edge `to` survivors that the far plan's
    // absorb flood removes (measured R0044: v35/v107/v90 retained on comps
    // 167/13/391 while removed on their far comps — every retained-removed
    // vertex surfaced as an unpaired directed edge in the `[451-audit]`
    // watertightness census). Dropping an already-removed vertex can add
    // nothing to the union, so one pass suffices. A cycle degenerating
    // below 3 refs stays in the plan — the mutation's typed refusals stay
    // the loud guard on that shape.
    let removed_union: std::collections::BTreeSet<u32> = plans
        .iter()
        .flat_map(|p| p.removed.iter().copied())
        .collect();
    for pl in &mut plans {
        let mut extra: Vec<u32> = Vec::new();
        for cy in &mut pl.corrected {
            cy.retain(|r| {
                if let CycleRef::Old(v) = *r {
                    if removed_union.contains(&v) {
                        extra.push(v);
                        return false;
                    }
                }
                true
            });
        }
        if !extra.is_empty() {
            pl.removed.extend(extra);
            pl.removed.sort_unstable();
            pl.removed.dedup();
        }
    }
    (plans, declines)
}

/// The cycle reference of junction `j` — from the path (mint-interned)
/// so shared mints resolve identically everywhere.
fn ref_of(
    _j: &crate::stage4_transit::CorridorJunction,
    path: &[CycleRef],
    jpos: &[usize],
    j: usize,
) -> CycleRef {
    path[jpos[j]]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn olds(v: &[u32]) -> Vec<CycleRef> {
        v.iter().map(|&x| CycleRef::Old(x)).collect()
    }

    #[test]
    fn mint_pool_interns_by_contract_band() {
        let mut pool = MintPool::default();
        let a = pool.intern([1.0, 2.0, 3.0]);
        let b = pool.intern([1.0 + 1e-12, 2.0, 3.0]);
        let c = pool.intern([1.0, 2.0, 4.0]);
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(pool.verts.len(), 2);
    }

    #[test]
    fn replace_subpath_swaps_a_phantom_for_a_path() {
        // The far-patch hole shape: [41, 43, 42(P), 39, 37, 46, 45] —
        // replace 43→39's interior (the phantom 42) with two mints.
        let mut cycles = vec![olds(&[41, 43, 42, 39, 37, 46, 45])];
        let removable = |v: u32| {
            if v == 42 {
                Removability::Phantom
            } else {
                Removability::SignKept
            }
        };
        let removed = replace_subpath(
            &mut cycles,
            43,
            39,
            &[CycleRef::New(0), CycleRef::New(1)],
            &removable,
        )
        .expect("swap");
        assert_eq!(removed, vec![42]);
        assert_eq!(
            cycles[0],
            vec![
                CycleRef::Old(43),
                CycleRef::New(0),
                CycleRef::New(1),
                CycleRef::Old(39),
                CycleRef::Old(37),
                CycleRef::Old(46),
                CycleRef::Old(45),
                CycleRef::Old(41),
            ]
        );
    }

    #[test]
    fn replace_subpath_excises_sign_removed_corners() {
        // The base shape: [.. 39, 42(P), 687(q), 688, 686, 682 ..] — from
        // 39 through phantom + removed corners to 682 via [J0].
        let mut cycles = vec![olds(&[10, 39, 42, 687, 688, 686, 682, 11])];
        let removable = |v: u32| match v {
            42 => Removability::Phantom,
            686..=688 => Removability::SignRemoved,
            _ => Removability::SignKept,
        };
        let removed =
            replace_subpath(&mut cycles, 39, 682, &[CycleRef::New(7)], &removable).expect("base");
        assert_eq!(removed, vec![42, 687, 688, 686]);
        assert_eq!(
            cycles[0],
            vec![
                CycleRef::Old(39),
                CycleRef::New(7),
                CycleRef::Old(682),
                CycleRef::Old(11),
                CycleRef::Old(10),
            ]
        );
    }

    #[test]
    fn replace_subpath_declines_kept_interior() {
        let mut cycles = vec![olds(&[1, 2, 3, 4])];
        let removable = |_: u32| Removability::SignKept;
        assert_eq!(
            replace_subpath(&mut cycles, 1, 3, &[], &removable),
            Err(PlanDecline::NotRemovable {
                at: 2,
                verdict: Removability::SignKept
            })
        );
    }

    #[test]
    fn replace_subpath_declines_ambiguous_and_missing_targets() {
        let mut cycles = vec![olds(&[1, 2, 3, 4])];
        let amb = |_: u32| Removability::Ambiguous;
        assert!(matches!(
            replace_subpath(&mut cycles, 1, 3, &[], &amb),
            Err(PlanDecline::NotRemovable {
                verdict: Removability::Ambiguous,
                ..
            })
        ));
        let ok = |_: u32| Removability::SignRemoved;
        assert_eq!(
            replace_subpath(&mut cycles, 1, 99, &[], &ok),
            Err(PlanDecline::ToNotFound { from: 1, to: 99 })
        );
        let mut two = vec![olds(&[1, 2, 1, 3])];
        assert_eq!(
            replace_subpath(&mut two, 1, 3, &[], &ok),
            Err(PlanDecline::FromNotUnique { from: 1, count: 2 })
        );
    }

    fn r0011_like_invocation() -> (
        Vec<crate::stage4_transit::CorridorRepair>,
        Vec<ComponentInput>,
    ) {
        use crate::stage4_transit::{CorridorJunction, CorridorRepair};
        let j = |sol: [f64; 3]| CorridorJunction {
            sol,
            faces: (1, 7),
            edge: 0,
            disposition: JunctionDisposition::Mint,
        };
        let c = CorridorRepair {
            far: (InputId::A, 2),
            walk_op: InputId::B,
            phantoms: vec![42],
            corners: vec![687],
            junctions: vec![j([0.0, 0.0, 0.0]), j([1.0, 0.0, 0.0])],
            runs: vec![(7, Ok(RunSource::Samples(vec![])))],
        };
        let comps = vec![
            ComponentInput {
                key: (InputId::A, 2),
                comp: 0,
                cycles: vec![vec![43, 42, 39, 90]],
            },
            ComponentInput {
                key: (InputId::B, 1),
                comp: 1,
                cycles: vec![vec![39, 42, 687, 688, 686, 682, 91]],
            },
            ComponentInput {
                key: (InputId::B, 7),
                comp: 2,
                cycles: vec![vec![70, 71, 72, 73]],
            },
        ];
        (vec![c], comps)
    }

    /// Healthy fixture geometry: junctions J0 (0,0,0) / J1 (1,0,0); every
    /// chain anchor sits on the kept side with its continuation further
    /// out, so no §3j absorb fires (all ring dots positive).
    fn healthy_pos(v: u32) -> Option<[f64; 3]> {
        Some(match v {
            39 => [0.0, -1.0, 0.0],
            43 => [1.0, -1.0, 0.0],
            90 => [0.5, -3.0, 0.0],
            91 => [0.0, -2.0, 0.0],
            _ => [5.0, 5.0, 5.0],
        })
    }

    fn r0011_ctx<'a>(
        far_value: &'a dyn Fn(usize, u32) -> Option<f64>,
        attachments: &'a dyn Fn(usize, u32) -> Vec<(u32, usize)>,
        hosts: &'a dyn Fn(usize, u32) -> Vec<(usize, (u32, u32))>,
        band: &'a dyn Fn(u32) -> f64,
        pos: &'a dyn Fn(u32) -> Option<[f64; 3]>,
    ) -> PlanCtx<'a> {
        PlanCtx {
            far_value,
            band,
            pos,
            attachments,
            hosts,
        }
    }

    #[test]
    fn plan_invocation_reproduces_all_three_measured_generators() {
        let (corridors, comps) = r0011_like_invocation();
        let far_value = |_: usize, v: u32| -> Option<f64> {
            Some(match v {
                687 | 688 | 686 | 71 => -1.0, // the removed side (q anchors it)
                42 => 0.0,                    // the phantom (never sign-decided)
                _ => 1.0,
            })
        };
        let attachments = |_: usize, p: u32| -> Vec<(u32, usize)> {
            if p == 42 {
                vec![(39, 0), (43, 1)]
            } else {
                vec![]
            }
        };
        let hosts = |_: usize, comp: u32| -> Vec<(usize, (u32, u32))> {
            match comp {
                1 => vec![(0, (686, 682))],
                2 => vec![(0, (70, 71)), (1, (71, 72))],
                _ => vec![],
            }
        };
        let band = |_: u32| 1e-9;
        let ctx = r0011_ctx(&far_value, &attachments, &hosts, &band, &healthy_pos);
        let mut pool = MintPool::default();
        let (plans, declines) = plan_invocation(&corridors, &comps, &ctx, &mut pool);
        assert!(declines.is_empty(), "{declines:?}");
        assert_eq!(plans.len(), 3, "{plans:?}");
        assert_eq!(pool.verts.len(), 2);
        // Generator A (far hole): phantom → reversed path (pred 43 attaches
        // junction 1).
        assert_eq!(
            plans[0].corrected[0],
            vec![
                CycleRef::Old(43),
                CycleRef::New(1),
                CycleRef::New(0),
                CycleRef::Old(39),
                CycleRef::Old(90),
            ]
        );
        assert_eq!(plans[0].removed, vec![42]);
        // Generator B (base): connector at 39, turn at J0, excision through
        // the crease corners, host edge split keeps 682.
        assert_eq!(
            plans[1].corrected[0],
            vec![
                CycleRef::Old(39),
                CycleRef::New(0),
                CycleRef::Old(682),
                CycleRef::Old(91),
            ]
        );
        assert_eq!(plans[1].removed, vec![42, 686, 687, 688]);
        // Generator C (run facet): the OUT corner 71 excised, run spliced
        // host-to-host.
        assert_eq!(
            plans[2].corrected[0],
            vec![
                CycleRef::Old(70),
                CycleRef::New(0),
                CycleRef::New(1),
                CycleRef::Old(72),
                CycleRef::Old(73),
            ]
        );
        assert_eq!(plans[2].removed, vec![71]);
    }

    /// inc-2c-3b-5 — the CORNER-CLIP pair: a phantom-free component whose
    /// boundary passes the SAME minted junction twice (the R0044 v142/v144
    /// mirrored-pair anatomy). The corner sliver between the two host edges
    /// is excised through the single mint; the kept side refuses the other
    /// orientation via the sign walk, so exactly one arrangement certifies.
    #[test]
    fn plan_invocation_clips_a_corner_hosted_twice_at_one_junction() {
        let (corridors, _) = r0011_like_invocation();
        let comps = vec![ComponentInput {
            key: (InputId::B, 7),
            comp: 3,
            cycles: vec![vec![10, 11, 12, 13, 14, 15]],
        }];
        let far_value = |_: usize, v: u32| -> Option<f64> {
            Some(match v {
                686..=688 => -1.0, // the corners anchor the removed sign
                11..=13 => -1.0,   // the corner sliver
                42 => 0.0,
                _ => 1.0, // the kept side (10, 14, 15) blocks the mirror walk
            })
        };
        let attachments = |_: usize, _: u32| -> Vec<(u32, usize)> { vec![] };
        let hosts = |_: usize, comp: u32| -> Vec<(usize, (u32, u32))> {
            if comp == 3 {
                vec![(0, (10, 11)), (0, (13, 14))]
            } else {
                vec![]
            }
        };
        let band = |_: u32| 1e-9;
        let ctx = r0011_ctx(&far_value, &attachments, &hosts, &band, &healthy_pos);
        let mut pool = MintPool::default();
        let (plans, declines) = plan_invocation(&corridors, &comps, &ctx, &mut pool);
        assert!(declines.is_empty(), "{declines:?}");
        assert_eq!(plans.len(), 1, "{plans:?}");
        assert_eq!(
            plans[0].corrected[0],
            vec![
                CycleRef::Old(10),
                CycleRef::New(0),
                CycleRef::Old(14),
                CycleRef::Old(15),
            ]
        );
        assert_eq!(plans[0].removed, vec![11, 12, 13]);
    }

    /// inc-2c-3b-8 (spec §3p) — the WHOLLY-CONDEMNED third-face pocket
    /// (R0044's B:0 corner sliver): the phantom's cycle neighbours resolve
    /// to NO junction and the component hosts nothing for the corridor —
    /// the planner leaves it UNPLANNED (no decline; the driver's closure
    /// sweep consumes it against the invocation's removed set).
    #[test]
    fn plan_invocation_leaves_a_wholly_condemned_component_to_the_sweep() {
        let (corridors, mut comps) = r0011_like_invocation();
        // The pocket: phantom 42 between unattached neighbours, plus the
        // crossed corner — no vertex of it resolves to any junction.
        comps.push(ComponentInput {
            key: (InputId::B, 9),
            comp: 9,
            cycles: vec![vec![42, 60, 687, 61]],
        });
        let far_value = |_: usize, v: u32| -> Option<f64> {
            Some(match v {
                687 | 688 | 686 | 71 => -1.0,
                42 => 0.0,
                _ => 1.0,
            })
        };
        let attachments = |_: usize, p: u32| -> Vec<(u32, usize)> {
            if p == 42 {
                vec![(39, 0), (43, 1)]
            } else {
                vec![]
            }
        };
        let hosts = |_: usize, comp: u32| -> Vec<(usize, (u32, u32))> {
            match comp {
                1 => vec![(0, (686, 682))],
                2 => vec![(0, (70, 71)), (1, (71, 72))],
                _ => vec![],
            }
        };
        let band = |_: u32| 1e-9;
        let ctx = r0011_ctx(&far_value, &attachments, &hosts, &band, &healthy_pos);
        let mut pool = MintPool::default();
        let (plans, declines) = plan_invocation(&corridors, &comps, &ctx, &mut pool);
        assert!(declines.is_empty(), "{declines:?}");
        // The three measured generators still plan; the pocket emits none.
        assert_eq!(plans.len(), 3, "{plans:?}");
        assert!(plans.iter().all(|pl| pl.comp != 9));
    }

    /// inc-2c-3b-9 (A), spec §3q — the removed-union filter: a vertex one
    /// plan removes vanishes from every OTHER plan's corrected cycle (the
    /// R0044 anatomy: generator B keeps a host-edge `to` survivor that the
    /// far plan's absorb flood removes; the retained reference surfaced as
    /// an unpaired directed edge in the post-batch audit).
    #[test]
    fn plan_invocation_filters_vertices_removed_by_another_plan() {
        let (corridors, mut comps) = r0011_like_invocation();
        // A second B-side component whose generator-B splice would keep 71
        // as the host-edge survivor — while comp 2's generator-C plan
        // REMOVES 71 (the OUT corner of the run facet).
        comps.push(ComponentInput {
            key: (InputId::B, 9),
            comp: 9,
            cycles: vec![vec![39, 42, 687, 688, 686, 71, 91]],
        });
        let far_value = |_: usize, v: u32| -> Option<f64> {
            Some(match v {
                687 | 688 | 686 | 71 => -1.0,
                42 => 0.0,
                _ => 1.0,
            })
        };
        let attachments = |_: usize, p: u32| -> Vec<(u32, usize)> {
            if p == 42 {
                vec![(39, 0), (43, 1)]
            } else {
                vec![]
            }
        };
        let hosts = |_: usize, comp: u32| -> Vec<(usize, (u32, u32))> {
            match comp {
                1 => vec![(0, (686, 682))],
                2 => vec![(0, (70, 71)), (1, (71, 72))],
                9 => vec![(0, (686, 71))],
                _ => vec![],
            }
        };
        let band = |_: u32| 1e-9;
        let ctx = r0011_ctx(&far_value, &attachments, &hosts, &band, &healthy_pos);
        let mut pool = MintPool::default();
        let (plans, declines) = plan_invocation(&corridors, &comps, &ctx, &mut pool);
        assert!(declines.is_empty(), "{declines:?}");
        let p9 = plans.iter().find(|p| p.comp == 9).expect("comp 9 plans");
        // The splice kept [39, J0, 71, 91]; the filter then drops 71
        // (removed by comp 2's plan) and records it.
        assert_eq!(
            p9.corrected[0],
            vec![CycleRef::Old(39), CycleRef::New(0), CycleRef::Old(91)],
            "{plans:?}"
        );
        assert!(p9.removed.contains(&71));
        // The donor plan is untouched: comp 2 still records the removal.
        let p2 = plans.iter().find(|p| p.comp == 2).expect("comp 2 plans");
        assert_eq!(p2.removed, vec![71]);
    }

    /// The guard: the same unattached-phantom shape on a component that
    /// DOES host the corridor's junctions keeps the typed decline — a
    /// hosted component keeps territory, so an unresolvable anchor there
    /// is a genuine defect, never sweep fodder.
    #[test]
    fn plan_invocation_still_declines_an_unattached_phantom_on_a_hosted_component() {
        let (corridors, mut comps) = r0011_like_invocation();
        comps.push(ComponentInput {
            key: (InputId::B, 9),
            comp: 9,
            cycles: vec![vec![42, 60, 687, 61]],
        });
        let far_value = |_: usize, v: u32| -> Option<f64> {
            Some(match v {
                687 | 688 | 686 | 71 => -1.0,
                42 => 0.0,
                _ => 1.0,
            })
        };
        let attachments = |_: usize, p: u32| -> Vec<(u32, usize)> {
            if p == 42 {
                vec![(39, 0), (43, 1)]
            } else {
                vec![]
            }
        };
        let hosts = |_: usize, comp: u32| -> Vec<(usize, (u32, u32))> {
            match comp {
                1 => vec![(0, (686, 682))],
                2 => vec![(0, (70, 71)), (1, (71, 72))],
                9 => vec![(0, (60, 687))],
                _ => vec![],
            }
        };
        let band = |_: u32| 1e-9;
        let ctx = r0011_ctx(&far_value, &attachments, &hosts, &band, &healthy_pos);
        let mut pool = MintPool::default();
        let (plans, declines) = plan_invocation(&corridors, &comps, &ctx, &mut pool);
        assert!(plans.iter().all(|pl| pl.comp != 9));
        assert!(
            declines
                .iter()
                .any(|(_, c, d)| *c == 9
                    && matches!(d, PlanDecline::AttachmentMismatch { phantom: 42 })),
            "{declines:?}"
        );
    }

    #[test]
    fn plan_invocation_declines_on_an_ambiguous_corner_anchor() {
        let (corridors, comps) = r0011_like_invocation();
        // The corner's far value sits inside the band: the removed-side
        // sign has no anchor — every affected component declines.
        let far_value =
            |_: usize, v: u32| -> Option<f64> { Some(if v == 687 { 1e-15 } else { 1.0 }) };
        let attachments = |_: usize, _: u32| -> Vec<(u32, usize)> { vec![(39, 0), (43, 1)] };
        let hosts = |_: usize, _: u32| -> Vec<(usize, (u32, u32))> { vec![] };
        let band = |_: u32| 1e-9;
        let ctx = r0011_ctx(&far_value, &attachments, &hosts, &band, &healthy_pos);
        let mut pool = MintPool::default();
        let (plans, declines) = plan_invocation(&corridors, &comps, &ctx, &mut pool);
        assert!(plans.is_empty(), "{plans:?}");
        assert_eq!(declines.len(), 3, "{declines:?}");
        assert!(declines.iter().all(|(_, _, d)| matches!(
            d,
            PlanDecline::NotRemovable {
                verdict: Removability::Ambiguous,
                ..
            }
        )));
    }

    #[test]
    fn plan_invocation_declines_a_kept_interior_instead_of_cutting_it() {
        let (corridors, comps) = r0011_like_invocation();
        // 688 reads KEPT: the base walk must refuse (never excise kept
        // territory), and the far + run components still plan.
        let far_value = |_: usize, v: u32| -> Option<f64> {
            Some(match v {
                687 | 686 | 71 => -1.0,
                688 => 1.0,
                42 => 0.0,
                _ => 1.0,
            })
        };
        let attachments = |_: usize, p: u32| -> Vec<(u32, usize)> {
            if p == 42 {
                vec![(39, 0), (43, 1)]
            } else {
                vec![]
            }
        };
        let hosts = |_: usize, comp: u32| -> Vec<(usize, (u32, u32))> {
            match comp {
                1 => vec![(0, (686, 682))],
                2 => vec![(0, (70, 71)), (1, (71, 72))],
                _ => vec![],
            }
        };
        let band = |_: u32| 1e-9;
        let ctx = r0011_ctx(&far_value, &attachments, &hosts, &band, &healthy_pos);
        let mut pool = MintPool::default();
        let (plans, declines) = plan_invocation(&corridors, &comps, &ctx, &mut pool);
        assert_eq!(plans.len(), 2, "{plans:?}");
        assert!(
            declines
                .iter()
                .any(|(_, _, d)| matches!(d, PlanDecline::HostMismatch { junction: 0 })),
            "{declines:?}"
        );
    }

    // §3j — the absorb arm. Fixtures reuse the r0011-like corridor but
    // enlarge the affected cycles so an absorbed anchor has a healthy
    // chain continuation to re-anchor on (the measured R0011 shape: v26
    // absorbs into J0, the splice re-anchors on v25).
    fn absorb_invocation(
        far_cycle: Vec<u32>,
        base_cycle: Vec<u32>,
    ) -> (
        Vec<crate::stage4_transit::CorridorRepair>,
        Vec<ComponentInput>,
    ) {
        let (corridors, _) = r0011_like_invocation();
        let comps = vec![
            ComponentInput {
                key: (InputId::A, 2),
                comp: 0,
                cycles: vec![far_cycle],
            },
            ComponentInput {
                key: (InputId::B, 1),
                comp: 1,
                cycles: vec![base_cycle],
            },
            ComponentInput {
                key: (InputId::B, 7),
                comp: 2,
                cycles: vec![vec![70, 71, 72, 73]],
            },
        ];
        (corridors, comps)
    }

    fn absorb_far_value(_: usize, v: u32) -> Option<f64> {
        Some(match v {
            687 | 688 | 686 | 71 => -1.0,
            42 => 0.0,
            _ => 1.0,
        })
    }
    fn absorb_attachments(_: usize, p: u32) -> Vec<(u32, usize)> {
        if p == 42 {
            vec![(39, 0), (43, 1)]
        } else {
            vec![]
        }
    }
    fn absorb_hosts(_: usize, comp: u32) -> Vec<(usize, (u32, u32))> {
        match comp {
            1 => vec![(0, (686, 682))],
            2 => vec![(0, (70, 71)), (1, (71, 72))],
            _ => vec![],
        }
    }
    fn absorb_band(_: u32) -> f64 {
        1e-9
    }

    #[test]
    fn generator_a_absorbs_an_overshot_pred_anchor() {
        // 43 slid PAST J1 (its connector doubles back); its continuation
        // 92 is healthy: the far splice re-anchors from=92 and 43 joins
        // the removed set. The other end (39) stays healthy.
        let (corridors, comps) = absorb_invocation(
            vec![43, 42, 39, 90, 92],
            vec![39, 42, 687, 688, 686, 682, 91],
        );
        let pos = |v: u32| -> Option<[f64; 3]> {
            Some(match v {
                43 => [1.0, 0.4, 0.0], // past J1 = (1,0,0)
                92 => [1.0, -1.0, 0.0],
                39 => [0.0, -1.0, 0.0],
                90 => [0.5, -3.0, 0.0],
                91 => [0.0, -2.0, 0.0],
                _ => [5.0, 5.0, 5.0],
            })
        };
        let ctx = r0011_ctx(
            &absorb_far_value,
            &absorb_attachments,
            &absorb_hosts,
            &absorb_band,
            &pos,
        );
        let mut pool = MintPool::default();
        let (plans, declines) = plan_invocation(&corridors, &comps, &ctx, &mut pool);
        assert!(declines.is_empty(), "{declines:?}");
        assert_eq!(plans.len(), 3, "{plans:?}");
        // Far: pred 43 attaches junction 1 → reversed path, spliced from
        // the continuation 92.
        assert_eq!(
            plans[0].corrected[0],
            vec![
                CycleRef::Old(92),
                CycleRef::New(1),
                CycleRef::New(0),
                CycleRef::Old(39),
                CycleRef::Old(90),
            ]
        );
        assert_eq!(plans[0].removed, vec![42, 43]);
        // Base unchanged by the far absorb (39 healthy there).
        assert_eq!(plans[1].removed, vec![42, 686, 687, 688]);
    }

    #[test]
    fn generator_b_absorbs_an_overshot_connector_anchor() {
        // 39 slid past J0: the far splice re-anchors on 93 and the BASE
        // connector re-anchors on 91 — the same certificate decides both
        // cycles consistently (the coherence the mutation relies on).
        let (corridors, comps) = absorb_invocation(
            vec![43, 42, 39, 93, 90],
            vec![39, 42, 687, 688, 686, 682, 91],
        );
        let pos = |v: u32| -> Option<[f64; 3]> {
            Some(match v {
                39 => [0.0, 0.4, 0.0], // past J0 = (0,0,0)
                93 => [0.0, -1.5, 0.0],
                43 => [1.0, -1.0, 0.0],
                90 => [0.5, -3.0, 0.0],
                91 => [0.0, -2.0, 0.0],
                682 => [0.0, -3.0, 0.0],
                _ => [5.0, 5.0, 5.0],
            })
        };
        let ctx = r0011_ctx(
            &absorb_far_value,
            &absorb_attachments,
            &absorb_hosts,
            &absorb_band,
            &pos,
        );
        let mut pool = MintPool::default();
        let (plans, declines) = plan_invocation(&corridors, &comps, &ctx, &mut pool);
        assert!(declines.is_empty(), "{declines:?}");
        assert_eq!(plans.len(), 3, "{plans:?}");
        assert_eq!(
            plans[0].corrected[0],
            vec![
                CycleRef::Old(43),
                CycleRef::New(1),
                CycleRef::New(0),
                CycleRef::Old(93),
                CycleRef::Old(90),
            ]
        );
        assert_eq!(plans[0].removed, vec![39, 42]);
        assert_eq!(
            plans[1].corrected[0],
            vec![CycleRef::Old(91), CycleRef::New(0), CycleRef::Old(682)]
        );
        assert_eq!(plans[1].removed, vec![39, 42, 686, 687, 688]);
    }

    #[test]
    fn absorb_declines_at_the_depth_cap() {
        // Four consecutive overshot anchors exhaust the cap: the far
        // component declines typed instead of draining the chain.
        let (corridors, comps) = absorb_invocation(
            vec![46, 45, 44, 43, 42, 39, 90],
            vec![39, 42, 687, 688, 686, 682, 91],
        );
        let pos = |v: u32| -> Option<[f64; 3]> {
            Some(match v {
                43 => [1.0, 0.5, 0.0],
                44 => [1.0, 0.4, 0.0],
                45 => [1.0, 0.3, 0.0],
                46 => [1.0, 0.2, 0.0],
                39 => [0.0, -1.0, 0.0],
                90 => [0.5, -3.0, 0.0],
                91 => [0.0, -2.0, 0.0],
                _ => [5.0, 5.0, 5.0],
            })
        };
        let ctx = r0011_ctx(
            &absorb_far_value,
            &absorb_attachments,
            &absorb_hosts,
            &absorb_band,
            &pos,
        );
        let mut pool = MintPool::default();
        let (plans, declines) = plan_invocation(&corridors, &comps, &ctx, &mut pool);
        assert_eq!(plans.len(), 2, "{plans:?}");
        assert!(
            declines.iter().any(|(_, _, d)| matches!(
                d,
                PlanDecline::NotRemovable {
                    verdict: Removability::Ambiguous,
                    ..
                }
            )),
            "{declines:?}"
        );
    }

    #[test]
    fn corridor_path_interleaves_junctions_runs_and_splices() {
        use crate::stage4_transit::{CorridorJunction, CorridorRepair};
        let j = |sol: [f64; 3], d: JunctionDisposition| CorridorJunction {
            sol,
            faces: (0, 1),
            edge: 0,
            disposition: d,
        };
        let c = CorridorRepair {
            far: (InputId::A, 0),
            walk_op: InputId::B,
            phantoms: vec![9],
            corners: vec![10],
            junctions: vec![
                j([0.0, 0.0, 0.0], JunctionDisposition::Mint),
                j(
                    [1.0, 0.0, 0.0],
                    JunctionDisposition::Splice { vertex: 55, d: 0.0 },
                ),
                j([2.0, 0.0, 0.0], JunctionDisposition::Mint),
            ],
            runs: vec![
                (7, Ok(RunSource::Samples(vec![[0.5, 0.0, 0.0]]))),
                (
                    8,
                    Ok(RunSource::Spliced {
                        head: vec![],
                        chain: vec![70, 71],
                        tail: vec![[1.9, 0.0, 0.0]],
                    }),
                ),
            ],
        };
        let mut pool = MintPool::default();
        let path = corridor_path(&c, &mut pool);
        assert_eq!(
            path,
            vec![
                CycleRef::New(0),
                CycleRef::New(1),
                CycleRef::Old(55),
                CycleRef::Old(70),
                CycleRef::Old(71),
                CycleRef::New(2),
                CycleRef::New(3),
            ]
        );
        assert_eq!(pool.verts.len(), 4);
    }
}
