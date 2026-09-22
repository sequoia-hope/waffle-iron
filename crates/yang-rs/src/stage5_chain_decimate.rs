//! §4.3.4 chain decimation of the emitted intersection runs (spec
//! `specs/yang_434_output_chord_refinement.md` "Stage-5 chain decimation",
//! 2026-09-22, R0085; deviation N58's paper-criterion form applied at the
//! OUTPUT).
//!
//! The paper's B-Rep Boolean output restores "parameter surfaces and their
//! boundary curves" (`refs/text/yang2025_hybrid_boolean.txt:581-605`) and
//! its intersection polylines carry exactly the samples its §4.3.4
//! refinement loop produces — a chord `p → q` is final once
//! `h < d_p·10², l < d_p·10³, α < π/18` (`:586-592`). The exact arrangement
//! mints intersection vertices far denser than that wherever a mesh grazes
//! the other operand (R0085 op 3: a plane∩cone generator split 69 times,
//! 2.6e-6 … 1.1e-2 apart; a torus × cone chain with vertices 3e-7 apart),
//! and every such vertex reached the output B-Rep as an edge endpoint.
//! Downstream that density is poison, not fidelity: kernel-v2's chart CDT
//! legitimately forms an ear over three consecutive collinear-by-noise
//! boundary vertices on BOTH faces of the run (a zero-thickness pleat the
//! watertight oracle reads as two 4-use edges), and the render weld grid
//! (1e-5 · scale) fuses neighbours 2e-5 apart.
//!
//! This post-pass walks every emitted loop and drops the interior vertices
//! of INTERSECTION runs that the paper's own criterion calls redundant, and
//! the sub-resolution SUBDIVISION vertices of straight operand edges.
//! Certification-driven, never trusting (P10):
//!
//! - candidates have exactly 4 loop-edge uses on exactly 2 faces (the I5-1b
//!   global count — junctions, curve changes and pinches all fail it) and
//!   consecutive edges of the SAME descriptor (`LineSegment` ↔
//!   `LineSegment`, identical `SurfacePair`), in one of two classes:
//!   (a) both edges KEYS of `intersection_curves` (A×B intersection edges),
//!   or (b) neither edge a key and both `LineSegment` — a split point of an
//!   operand's straight edge (R0085 op 2 keeps such points 2e-5 … 1e-4
//!   apart on a gear cap∩flank edge with no intersection edge attached).
//!   A vertex mixing the classes is where an intersection curve meets an
//!   operand edge and stays; an operand's own profile CORNER is never a
//!   candidate of class (b) because the strict test below refuses any bend
//!   above working precision — real geometry at any size ≥
//!   `MIN_FEATURE_SIZE` is untouched;
//! - a class-(a) candidate is dropped iff [`paper_chain_sample_redundant`]
//!   holds for (previous KEPT vertex, candidate, next ORIGINAL vertex) — the
//!   greedy walk of the I5 seam-reorder cleanup, so a kept pair is always
//!   ≥ d_p·10³ apart along the run and the surviving polyline is one the
//!   paper's refinement would itself terminate at; a class-(b) candidate
//!   iff it lies on the segment through its kept neighbours to WORKING
//!   precision (`h ≤ TAU_WORK·(1+scale)`: dropping it changes no geometry
//!   the model resolves) within the same chord bound (only the
//!   render-sub-resolution class moves);
//! - decisions are made ONCE per canonical undirected chain and reused by
//!   the twin loop, so both owners emit identical piece boundaries; a loop
//!   that would fall below the 3-edge floor declines every chain it owns
//!   (and so does the twin, through the cache), iterated to a fixpoint.
//!
//! Conic runs are the I5-1b merge's (`stage5_seam_merge`) — it ran before
//! this pass and coalesced them into analytic arcs; a run it declined keeps
//! its per-segment shape here too (conics are excluded, their pieces would
//! need `orient_directed_curve`). Merged pieces keep the run's own
//! descriptor: a `LineSegment` piece is the same line, a `SurfacePair` piece
//! the same pair — kernel-v2 resamples both at render density.
//!
//! `YANG_434_DECIMATE=0|off` is the dev A/B off-knob (byte-identical
//! emission).

use std::collections::{BTreeMap, BTreeSet};

use cad_primitives::Point3;

use crate::brep::{BRepEdge, BRepFace, TessellationSource};
use crate::geom::Curve;
use crate::stage4_correct::paper_chain_sample_redundant;

/// ALWAYS-ON; `YANG_434_DECIMATE=0|off` is the dev A/B off-knob.
pub(crate) fn decimate_gate_enabled() -> bool {
    !matches!(std::env::var("YANG_434_DECIMATE"), Ok(v) if v == "0" || v == "off")
}

/// Pass statistics (log line at the call site).
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct DecimateStats {
    pub edges_before: usize,
    pub edges_after: usize,
    /// Canonical chains that dropped at least one vertex.
    pub runs_decimated: usize,
    /// Interior vertices dropped (counted once per canonical chain).
    pub verts_dropped: usize,
    /// Chains declined because an owner loop would fall below 3 edges.
    pub declined_floor: usize,
    /// Loops skipped because their stored edge directions do not chain.
    pub skipped_discontinuous_loops: usize,
}

/// The undirected run key of an edge: `Some` for the curve kinds this pass
/// coalesces (whose descriptor is direction-free), `None` otherwise.
fn run_key(c: &Curve) -> Option<RunKey> {
    match c {
        Curve::LineSegment => Some(RunKey::Line),
        Curve::SurfacePair { a, b } => Some(RunKey::Pair(*a, *b)),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum RunKey {
    Line,
    Pair(crate::geom::Surface, crate::geom::Surface),
}

impl RunKey {
    /// Same run: identical descriptor (a pair in either argument order).
    fn same(&self, other: &RunKey) -> bool {
        match (self, other) {
            (RunKey::Line, RunKey::Line) => true,
            (RunKey::Pair(a, b), RunKey::Pair(c, d)) => (a == c && b == d) || (a == d && b == c),
            _ => false,
        }
    }
}

/// The directed vertex cycle of a stored loop (`cycle[i]` = start of
/// `edges[lp[i]]`), or `None` if the stored directions do not chain.
fn loop_cycle(edges: &[BRepEdge], lp: &[u32]) -> Option<Vec<u32>> {
    if lp.is_empty() {
        return None;
    }
    let mut cycle = Vec::with_capacity(lp.len());
    for (i, &ei) in lp.iter().enumerate() {
        let e = edges.get(ei as usize)?;
        let next = edges.get(lp[(i + 1) % lp.len()] as usize)?;
        if e.end != next.start {
            return None;
        }
        cycle.push(e.start);
    }
    Some(cycle)
}

/// Canonical form of an OPEN chain: first vertex ≤ last vertex.
fn canonical_open_chain(chain: &[u32]) -> (Vec<u32>, bool) {
    if chain.first() <= chain.last() {
        (chain.to_vec(), false)
    } else {
        (chain.iter().rev().copied().collect(), true)
    }
}

/// A loop's edge positions cut into maximal runs. Each stretch is
/// `(start in ROTATED order, length, key)`; `offset` maps rotated position 0
/// to the original position. `whole_closed` = the whole loop is one run
/// (every vertex droppable, one key).
struct LoopPartition {
    stretches: Vec<(usize, usize, Option<RunKey>)>,
    offset: usize,
    whole_closed: bool,
}

fn partition_loop(
    edges: &[BRepEdge],
    lp: &[u32],
    cycle: &[u32],
    droppable: &BTreeSet<u32>,
) -> LoopPartition {
    let m = lp.len();
    let keys: Vec<Option<RunKey>> = lp
        .iter()
        .map(|&ei| run_key(&edges[ei as usize].curve))
        .collect();
    let joinable_after = |i: usize| -> bool {
        let j = (i + 1) % m;
        match (&keys[i], &keys[j]) {
            (Some(a), Some(b)) => a.same(b) && droppable.contains(&cycle[j]),
            _ => false,
        }
    };
    let Some(first_break) = (0..m).find(|&i| !joinable_after(i)) else {
        return LoopPartition {
            stretches: vec![(0, m, keys[0])],
            offset: 0,
            whole_closed: true,
        };
    };
    let offset = (first_break + 1) % m;
    let key_at = |r: usize| keys[(offset + r) % m];
    let mut stretches = Vec::new();
    let mut r = 0usize;
    while r < m {
        let k = key_at(r);
        let mut len = 1usize;
        while r + len < m
            && matches!((key_at(r + len), k), (Some(a), Some(b)) if a.same(&b))
            && joinable_after((offset + r + len - 1) % m)
        {
            len += 1;
        }
        stretches.push((r, len, k));
        r += len;
    }
    LoopPartition {
        stretches,
        offset,
        whole_closed: false,
    }
}

/// STRICT redundancy for a subdivision vertex of a straight operand edge:
/// `m` lies on the segment `a → b` to working precision
/// (`h ≤ TAU_WORK·(1+scale)`, so dropping it changes no geometry the model
/// can resolve) and within the paper's chord bound (`l < d_p·10³`, so only
/// the render-sub-resolution class moves).
fn strict_subdivision_redundant(a: [f64; 3], m: [f64; 3], b: [f64; 3]) -> bool {
    let mt = crate::stage4_correct::paper_chain_metrics(a, m, b);
    let scale = a
        .iter()
        .chain(m.iter())
        .chain(b.iter())
        .fold(0.0f64, |acc, &c| acc.max(c.abs()));
    mt.l < mt.dp * 1e3 && mt.h <= cad_primitives::TAU_WORK * (1.0 + scale)
}

/// The greedy §4.3.4 walk over a canonical chain: indices INTO THE CHAIN of
/// the kept vertices (endpoints always kept; `anchors` are extra positions
/// that must survive — the closed-loop tripod). A vertex in `strict` (a
/// straight operand edge's subdivision point) is judged by
/// [`strict_subdivision_redundant`] instead of the paper's chain test.
fn kept_positions(
    chain: &[u32],
    verts: &[Point3],
    anchors: &[usize],
    strict: &BTreeSet<u32>,
) -> Vec<usize> {
    let k = chain.len();
    let mut kept: Vec<usize> = vec![0];
    for i in 1..k.saturating_sub(1) {
        let last = *kept.last().expect("nonempty");
        let (a, m, b) = (
            verts[chain[last] as usize].as_array(),
            verts[chain[i] as usize].as_array(),
            verts[chain[i + 1] as usize].as_array(),
        );
        let redundant = if strict.contains(&chain[i]) {
            strict_subdivision_redundant(a, m, b)
        } else {
            paper_chain_sample_redundant(a, m, b)
        };
        let drop = !anchors.contains(&i) && redundant;
        if !drop {
            kept.push(i);
        }
    }
    if k >= 2 {
        kept.push(k - 1);
    }
    kept
}

/// Run the decimation over the emitted topology. Never fails — every
/// structural surprise declines the affected run and keeps the per-segment
/// status quo.
pub(crate) fn decimate_intersection_runs(
    verts: &[Point3],
    edges: &mut Vec<BRepEdge>,
    faces: &mut [BRepFace],
    sources: &mut [TessellationSource],
    intersection_curves: &BTreeMap<(u32, u32), Curve>,
) -> DecimateStats {
    let mut stats = DecimateStats {
        edges_before: edges.len(),
        ..DecimateStats::default()
    };
    let is_intersection =
        |e: &BRepEdge| intersection_curves.contains_key(&(e.start.min(e.end), e.start.max(e.end)));

    // ── Census: global loop-edge uses per vertex ─────────────────────────
    struct VertUse {
        count: u32,
        faces: BTreeSet<u32>,
        /// Loop-edge uses that are A×B intersection edges.
        n_intersection: u32,
        /// Every loop-edge use is a `LineSegment`.
        all_line: bool,
    }
    let mut uses: BTreeMap<u32, VertUse> = BTreeMap::new();
    for (fi, f) in faces.iter().enumerate() {
        for lp in std::iter::once(&f.outer_loop).chain(f.inner_loops.iter()) {
            for &ei in lp {
                let Some(e) = edges.get(ei as usize) else {
                    continue;
                };
                let inter = is_intersection(e);
                let line = matches!(e.curve, Curve::LineSegment);
                for v in [e.start, e.end] {
                    let u = uses.entry(v).or_insert_with(|| VertUse {
                        count: 0,
                        faces: BTreeSet::new(),
                        n_intersection: 0,
                        all_line: true,
                    });
                    u.count += 1;
                    u.faces.insert(fi as u32);
                    u.n_intersection += u32::from(inter);
                    u.all_line &= line;
                }
            }
        }
    }
    // Two candidate classes, both with exactly 4 loop-edge uses on exactly
    // 2 faces: (a) INTERSECTION-run vertices (every use an A×B edge) —
    // judged by the paper's §4.3.4 chain acceptance; (b) SUBDIVISION
    // vertices of a straight operand edge (no use an A×B edge, every use a
    // `LineSegment`) — the split points an operand edge keeps after the
    // arrangement crossings that made them were ruled out (R0085 op 2:
    // 2e-5 … 1e-4 apart on a gear cap∩flank edge, valence 2 on both faces,
    // no intersection edge attached) — judged by the STRICT test below: on
    // the line through their kept neighbours to working precision, so the
    // edge's geometry is unchanged, and within the paper's length bound so
    // only the render-sub-resolution class moves. A vertex mixing the two
    // classes is the junction of an intersection curve with an operand
    // edge and stays.
    let droppable: BTreeSet<u32> = uses
        .iter()
        .filter(|(_, u)| {
            u.count == 4
                && u.faces.len() == 2
                && (u.n_intersection == 4 || (u.n_intersection == 0 && u.all_line))
        })
        .map(|(&v, _)| v)
        .collect();
    let strict: BTreeSet<u32> = uses
        .iter()
        .filter(|(&v, u)| droppable.contains(&v) && u.n_intersection == 0)
        .map(|(&v, _)| v)
        .collect();
    // Dev probe: `YANG_434_DECIMATE_PROBE=x,y,z[;x,y,z…]` — for the output
    // vertex nearest each position, print its loop-use census and every
    // incident loop edge (curve kind, intersection membership, length).
    if let Ok(spec) = std::env::var("YANG_434_DECIMATE_PROBE") {
        for site in spec.split(';') {
            let c: Vec<f64> = site
                .split(',')
                .filter_map(|s| s.trim().parse().ok())
                .collect();
            if c.len() != 3 {
                continue;
            }
            let Some((&v, u)) = uses.iter().min_by(|(a, _), (b, _)| {
                let d = |i: u32| {
                    let p = verts[i as usize];
                    (p.x() - c[0]).powi(2) + (p.y() - c[1]).powi(2) + (p.z() - c[2]).powi(2)
                };
                d(**a)
                    .partial_cmp(&d(**b))
                    .unwrap_or(std::cmp::Ordering::Equal)
            }) else {
                continue;
            };
            let p = verts[v as usize];
            eprintln!(
                "[s434-decimate-probe] site=({},{},{}) nearest v{v}=({:.9e},{:.9e},{:.9e}) \
                 count={} faces={:?} n_intersection={} all_line={} droppable={}",
                c[0],
                c[1],
                c[2],
                p.x(),
                p.y(),
                p.z(),
                u.count,
                u.faces,
                u.n_intersection,
                u.all_line,
                droppable.contains(&v)
            );
            for (fi, f) in faces.iter().enumerate() {
                for (li, lp) in std::iter::once(&f.outer_loop)
                    .chain(f.inner_loops.iter())
                    .enumerate()
                {
                    for &ei in lp {
                        let e = &edges[ei as usize];
                        if e.start != v && e.end != v {
                            continue;
                        }
                        let q = verts[if e.start == v { e.end } else { e.start } as usize];
                        let len = ((q.x() - p.x()).powi(2)
                            + (q.y() - p.y()).powi(2)
                            + (q.z() - p.z()).powi(2))
                        .sqrt();
                        let kind = match e.curve {
                            Curve::LineSegment => "line",
                            Curve::Circle { .. } => "circle",
                            Curve::Ellipse { .. } => "ellipse",
                            Curve::Parabola { .. } => "parabola",
                            Curve::Hyperbola { .. } => "hyperbola",
                            Curve::SurfacePair { .. } => "pair",
                        };
                        let other = if e.start == v { e.end } else { e.start };
                        let ou = uses.get(&other);
                        eprintln!(
                            "    face {fi} loop {li} edge {ei}: {}→{} kind={kind} \
                             intersection={} len={len:.3e} other_count={:?} other_faces={:?}",
                            e.start,
                            e.end,
                            is_intersection(e),
                            ou.map(|u| u.count),
                            ou.map(|u| &u.faces)
                        );
                    }
                }
            }
        }
    }
    if droppable.is_empty() {
        stats.edges_after = edges.len();
        return stats;
    }

    // ── Decisions per canonical chain ────────────────────────────────────
    // `Some(kept)` = kept chain positions (canonical order); `None` =
    // declined (verbatim). Closed whole-loop runs keep a tripod of anchors
    // so the rebuilt loop has ≥ 3 edges.
    let mut decisions: BTreeMap<Vec<u32>, Option<Vec<usize>>> = BTreeMap::new();
    // Per (face, loop): the partition and the canonical chains of its
    // stretches, so the floor pass and the rebuild see the same data.
    struct LoopPlan {
        part: LoopPartition,
        /// Per stretch: (canonical chain, reversed) — `None` for a stretch
        /// that is not a run.
        chains: Vec<Option<(Vec<u32>, bool)>>,
    }
    let mut plans: Vec<Vec<Option<LoopPlan>>> = Vec::with_capacity(faces.len());
    for f in faces.iter() {
        let mut face_plans = Vec::new();
        for lp in std::iter::once(&f.outer_loop).chain(f.inner_loops.iter()) {
            let Some(cycle) = loop_cycle(edges, lp) else {
                stats.skipped_discontinuous_loops += 1;
                face_plans.push(None);
                continue;
            };
            let part = partition_loop(edges, lp, &cycle, &droppable);
            let m = lp.len();
            let mut chains = Vec::with_capacity(part.stretches.len());
            for &(start_r, len, key) in &part.stretches {
                if key.is_none() || (!part.whole_closed && len < 2) {
                    chains.push(None);
                    continue;
                }
                let orig_pos = |r: usize| (part.offset + r) % m;
                let mut chain: Vec<u32> = Vec::with_capacity(len + 1);
                for r in start_r..start_r + len {
                    chain.push(cycle[orig_pos(r)]);
                }
                if part.whole_closed {
                    // Canonical closed chain: rotate the min vertex first,
                    // run toward the smaller neighbour; the chain lists each
                    // vertex once and the rebuild closes it.
                    let k = chain.len();
                    let (min_pos, _) = chain
                        .iter()
                        .enumerate()
                        .min_by_key(|&(_, v)| *v)
                        .expect("nonempty");
                    let fwd = chain[(min_pos + 1) % k];
                    let bwd = chain[(min_pos + k - 1) % k];
                    let reversed = fwd > bwd;
                    let rotated: Vec<u32> = (0..k)
                        .map(|i| {
                            if reversed {
                                chain[(min_pos + k - i) % k]
                            } else {
                                chain[(min_pos + i) % k]
                            }
                        })
                        .collect();
                    let k_c = rotated.len();
                    decisions.entry(rotated.clone()).or_insert_with(|| {
                        if k_c < 4 {
                            return None;
                        }
                        // Tripod anchors at thirds; the walk treats the
                        // chain as open with the closing vertex appended.
                        let mut closed = rotated.clone();
                        closed.push(rotated[0]);
                        let anchors = [k_c / 3, (2 * k_c) / 3];
                        let kept = kept_positions(&closed, verts, &anchors, &strict);
                        // Drop the appended closing position; keep at
                        // least 3 distinct vertices.
                        let kept: Vec<usize> = kept.into_iter().filter(|&i| i < k_c).collect();
                        if kept.len() < 3 {
                            None
                        } else {
                            Some(kept)
                        }
                    });
                    chains.push(Some((rotated, reversed)));
                } else {
                    chain.push(cycle[orig_pos(start_r + len) % m]);
                    let (canon, reversed) = canonical_open_chain(&chain);
                    decisions
                        .entry(canon.clone())
                        .or_insert_with(|| Some(kept_positions(&canon, verts, &[], &strict)));
                    chains.push(Some((canon, reversed)));
                }
            }
            face_plans.push(Some(LoopPlan { part, chains }));
        }
        plans.push(face_plans);
    }

    // ── Loop floor: a loop must keep ≥ 3 edges; decline its chains if not,
    // to a fixpoint (declines only ever raise counts elsewhere).
    loop {
        let mut changed = false;
        for face_plans in &plans {
            for plan in face_plans.iter().flatten() {
                let mut after = 0usize;
                for (si, &(_, len, _)) in plan.part.stretches.iter().enumerate() {
                    match plan.chains[si].as_ref().and_then(|(c, _)| decisions.get(c)) {
                        Some(Some(kept)) => {
                            after += if plan.part.whole_closed {
                                kept.len()
                            } else {
                                kept.len().saturating_sub(1)
                            };
                        }
                        _ => after += len,
                    }
                }
                if after < 3 {
                    for (c, _) in plan.chains.iter().flatten() {
                        if let Some(d) = decisions.get_mut(c) {
                            if d.is_some() {
                                *d = None;
                                stats.declined_floor += 1;
                                changed = true;
                            }
                        }
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }

    // ── Rebuild ──────────────────────────────────────────────────────────
    // Verbatim edges keep their indices (the vector starts as a clone,
    // pieces are appended, a final order-preserving compaction drops the
    // edges no loop references), and every rebuilt loop is rotated back to
    // start at the entry covering original position 0 — a pass that drops
    // nothing leaves emission byte-identical.
    let mut new_edges: Vec<BRepEdge> = edges.clone();
    let mut counted: BTreeSet<Vec<u32>> = BTreeSet::new();
    // First-copy piece assignment for the sources retag.
    let mut vert_to_piece: BTreeMap<u32, u32> = BTreeMap::new();
    let mut piece_index: BTreeMap<(u32, u32), u32> = BTreeMap::new();
    let mut rebuilt: Vec<Vec<Vec<u32>>> = Vec::with_capacity(faces.len());
    for (fi, f) in faces.iter().enumerate() {
        let mut face_loops: Vec<Vec<u32>> = Vec::new();
        for (li, lp) in std::iter::once(&f.outer_loop)
            .chain(f.inner_loops.iter())
            .enumerate()
        {
            let Some(plan) = plans[fi][li].as_ref() else {
                face_loops.push(lp.clone());
                continue;
            };
            let m = lp.len();
            let part = &plan.part;
            let orig_pos = |r: usize| (part.offset + r) % m;
            let mut new_lp: Vec<u32> = Vec::with_capacity(m);
            let mut covers: Vec<Vec<usize>> = Vec::with_capacity(m);
            for (si, &(start_r, len, _)) in part.stretches.iter().enumerate() {
                let decision = plan.chains[si]
                    .as_ref()
                    .and_then(|(c, rev)| decisions.get(c).map(|d| (c, *rev, d)));
                let Some((canon, reversed, Some(kept))) = decision else {
                    for r in start_r..start_r + len {
                        new_lp.push(lp[orig_pos(r)]);
                        covers.push(vec![orig_pos(r)]);
                    }
                    continue;
                };
                if kept.len() == canon.len() && !part.whole_closed {
                    // Nothing dropped: verbatim, byte-identical.
                    for r in start_r..start_r + len {
                        new_lp.push(lp[orig_pos(r)]);
                        covers.push(vec![orig_pos(r)]);
                    }
                    continue;
                }
                if part.whole_closed && kept.len() == canon.len() {
                    for r in start_r..start_r + len {
                        new_lp.push(lp[orig_pos(r)]);
                        covers.push(vec![orig_pos(r)]);
                    }
                    continue;
                }
                let curve = edges[lp[orig_pos(start_r)] as usize].curve;
                // Pieces in canonical order as (start pos, end pos) into
                // the canonical chain.
                let k_c = canon.len();
                let spans: Vec<(usize, usize)> = if part.whole_closed {
                    (0..kept.len())
                        .map(|i| (kept[i], kept[(i + 1) % kept.len()]))
                        .collect()
                } else {
                    (0..kept.len() - 1)
                        .map(|i| (kept[i], kept[i + 1]))
                        .collect()
                };
                // This loop's traversal order of the pieces.
                let ordered: Vec<(usize, usize)> = if reversed {
                    spans.iter().rev().map(|&(a, b)| (b, a)).collect()
                } else {
                    spans.clone()
                };
                // Traversal chain (this loop's direction) for position
                // coverage: canonical index → traversal index.
                let trav_index = |ci: usize| -> usize {
                    if reversed {
                        (k_c - 1 - ci) % k_c
                    } else {
                        ci
                    }
                };
                let first_copy = !counted.contains(canon);
                for &(a, b) in &ordered {
                    let (s, e) = (canon[a], canon[b]);
                    let key = (s.min(e), s.max(e));
                    let ni = match piece_index.get(&key) {
                        Some(&ni) if new_edges[ni as usize].start == s => ni,
                        _ => {
                            let ni = new_edges.len() as u32;
                            new_edges.push(BRepEdge {
                                start: s,
                                end: e,
                                curve,
                            });
                            piece_index.entry(key).or_insert(ni);
                            ni
                        }
                    };
                    new_lp.push(ni);
                    // Original positions covered: traversal edges from
                    // trav(a) up to trav(b).
                    let (ta, tb) = (trav_index(a), trav_index(b));
                    let mut cov: Vec<usize> = Vec::new();
                    let mut i = ta;
                    loop {
                        cov.push(orig_pos(start_r + i));
                        i = (i + 1) % k_c;
                        if i == tb || (!part.whole_closed && i == 0) {
                            break;
                        }
                    }
                    covers.push(cov);
                    if first_copy {
                        // Every canonical vertex in [a, b] maps to this piece.
                        let mut i = a;
                        loop {
                            vert_to_piece.entry(canon[i]).or_insert(ni);
                            if i == b {
                                break;
                            }
                            i = (i + 1) % k_c;
                            if !part.whole_closed && i == 0 {
                                break;
                            }
                        }
                    }
                }
                if first_copy {
                    counted.insert(canon.clone());
                    stats.runs_decimated += 1;
                    stats.verts_dropped += k_c - kept.len();
                }
            }
            if let Some(k) = covers.iter().position(|c| c.contains(&0)) {
                new_lp.rotate_left(k);
            }
            face_loops.push(new_lp);
        }
        rebuilt.push(face_loops);
    }
    for (f, mut loops) in faces.iter_mut().zip(rebuilt) {
        f.outer_loop = loops.remove(0);
        f.inner_loops = loops;
    }

    // ── Compaction ───────────────────────────────────────────────────────
    let mut used = vec![false; new_edges.len()];
    for f in faces.iter() {
        for lp in std::iter::once(&f.outer_loop).chain(f.inner_loops.iter()) {
            for &ei in lp {
                if let Some(u) = used.get_mut(ei as usize) {
                    *u = true;
                }
            }
        }
    }
    let mut compact: Vec<Option<u32>> = vec![None; new_edges.len()];
    let mut compacted: Vec<BRepEdge> = Vec::with_capacity(new_edges.len());
    for (i, e) in new_edges.iter().enumerate() {
        if used[i] {
            compact[i] = Some(compacted.len() as u32);
            compacted.push(e.clone());
        }
    }
    for f in faces.iter_mut() {
        for lp in std::iter::once(&mut f.outer_loop).chain(f.inner_loops.iter_mut()) {
            for ei in lp.iter_mut() {
                if let Some(Some(n)) = compact.get(*ei as usize) {
                    *ei = *n;
                }
            }
        }
    }

    // ── Sources: remap surviving edge indices; a dropped run vertex (or a
    // vertex whose edge source was a replaced run edge) becomes a point on
    // its covering piece — both piece kinds evaluate as the endpoint lerp
    // (`eval_source` LineSegment / SurfacePair arms), so `t` is the chord
    // fraction of the vertex's own position.
    let lerp_t = |piece: &BRepEdge, p: Point3| -> f64 {
        let s = verts[piece.start as usize].as_array();
        let e = verts[piece.end as usize].as_array();
        let d = [e[0] - s[0], e[1] - s[1], e[2] - s[2]];
        let w = [p.x() - s[0], p.y() - s[1], p.z() - s[2]];
        let dd = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
        if dd > 0.0 {
            ((w[0] * d[0] + w[1] * d[1] + w[2] * d[2]) / dd).clamp(0.0, 1.0)
        } else {
            0.0
        }
    };
    let dropped: BTreeSet<u32> = vert_to_piece
        .keys()
        .copied()
        .filter(|v| !compacted.iter().any(|e| e.start == *v || e.end == *v))
        .collect();
    for (v, src) in sources.iter_mut().enumerate() {
        let vid = v as u32;
        let retag_to_piece = |src: &mut TessellationSource| {
            if let Some(piece) = vert_to_piece
                .get(&vid)
                .and_then(|&p| compact.get(p as usize).copied().flatten())
            {
                let t = lerp_t(&compacted[piece as usize], verts[v]);
                *src = TessellationSource::BRepEdge { edge: piece, t };
            }
        };
        match *src {
            TessellationSource::BRepEdge { edge, t } => {
                match compact.get(edge as usize).copied().flatten() {
                    Some(n) => *src = TessellationSource::BRepEdge { edge: n, t },
                    None => {
                        *src = TessellationSource::BRepVertex(vid);
                        retag_to_piece(src);
                    }
                }
            }
            TessellationSource::BRepVertex(_) if dropped.contains(&vid) => retag_to_piece(src),
            _ => {}
        }
    }

    *edges = compacted;
    stats.edges_after = edges.len();
    stats
}
