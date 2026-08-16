//! I5-1b — §4.4.2 Stage-6 conic seam chain-merge (gated `YANG_434_MERGE`;
//! spec `specs/yang_441_trim_cdt_construction.md` §4-I5-1b, task #89).
//!
//! The paper's B-Rep Boolean output restores "parameter surfaces and their
//! boundary curves" (`refs/text/yang2025_hybrid_boolean.txt:581-605`) — the
//! B-Rep edge is the CURVE; the dense polyline belongs to the mesh. The
//! shipped emission instead pushes one `BRepEdge` per mesh seam segment
//! (task #88: F0059 E 124→16848 gate-ON, 44–110× render-mesh inflation).
//!
//! This post-pass walks every emitted loop and coalesces maximal runs of
//! consecutive edges on the SAME undirected conic into single analytic arc
//! edges. It is certification-driven, never trusting (P10):
//!
//! - an interior vertex is elidable only if, across the WHOLE output, it has
//!   exactly 4 loop-edge uses on exactly 2 faces, all on the run's conic
//!   (the `recover.rs` Steiner/T rule made global — junctions, curve
//!   changes, §4B T-subdivision vertices and pinches all fail the count);
//! - every elided vertex must lie ON the canonical conic within the
//!   from_yang classify band (`TAU_EVAL`-scaled), and the chain's
//!   `conic_param` sequence must be strictly monotone (wrap-aware,
//!   consistent sign);
//! - pieces are capped at [`SWEEP_MAX`] rad and re-verified below
//!   π − [`PIECE_SWEEP_GUARD`] after split selection (the from_yang
//!   minor-side vocabulary); closed runs split into 4 arcs (≥3-edge loop
//!   floor; no `Full`/closed-ellipse vocabulary needed).
//!
//! Any failed check DECLINES the run — the per-segment status quo is kept
//! and censused, never a new failure mode. Twin conformance is by
//! construction: candidacy (global counts), the canonical undirected curve,
//! params and split selection all derive from undirected data cached per
//! canonical chain, so both owners emit identical piece boundaries; each
//! side orients its copy per traversal (`orient_directed_curve`).
//!
//! The witness `mesh` is untouched — density stays in the mesh layer, where
//! §4.4.1 wants it. `TessellationSource::BRepEdge` entries are remapped
//! (surviving edges) or retagged onto the covering piece (deleted run
//! edges); `BRepVertex` sources stay valid (the vertex array is unchanged).

use std::collections::{BTreeMap, BTreeSet};

use cad_primitives::Point3;

use crate::brep::{BRepEdge, BRepFace, TessellationSource};
use crate::geom::Curve;
use crate::stage4_correct::conic_param;

/// Max sweep per merged piece, radians. Comfortably below π so that even
/// with the post-selection guard the minor-side derivation in
/// `orient_directed_curve` and from_yang's `ARC_MINOR_AMBIGUITY_BAND`
/// (1e-6) are never approached.
const SWEEP_MAX: f64 = 1.8;

/// A selected piece must sweep strictly less than π − this guard, or the
/// run declines. 1e-3 rad ≫ the 1e-6 ambiguity band.
const PIECE_SWEEP_GUARD: f64 = 1e-3;

pub(crate) fn merge_gate_enabled() -> bool {
    std::env::var_os("YANG_434_MERGE").is_some()
}

#[derive(Default, Debug)]
pub(crate) struct MergeStats {
    pub runs_merged: usize,
    pub verts_elided: usize,
    pub edges_before: usize,
    pub edges_after: usize,
    pub declined_offcurve: usize,
    pub declined_nonmonotone: usize,
    pub declined_short: usize,
    pub declined_param: usize,
    pub declined_sweep: usize,
    /// Loops skipped because stored-direction continuity did not hold
    /// (defensive; emitted output loops are stored-continuous).
    pub skipped_discontinuous_loops: usize,
}

/// Canonical undirected representative of a conic: the stored normal's sign
/// is fixed so its first nonzero component is positive (negating a conic's
/// normal preserves the point set — `conics_equal_up_to_normal_sign`).
/// `None` for every non-conic payload (out of merge scope).
fn canonical_conic(c: &Curve) -> Option<Curve> {
    let lex_flip = |n: cad_primitives::Vector3| -> (cad_primitives::Vector3, bool) {
        let a = n.as_array();
        let first_nonzero = a.iter().copied().find(|&x| x != 0.0).unwrap_or(0.0);
        if first_nonzero < 0.0 {
            (cad_primitives::Vector3::new(-a[0], -a[1], -a[2]), true)
        } else {
            (n, false)
        }
    };
    match *c {
        Curve::Circle {
            center,
            normal,
            radius,
        } => {
            let (normal, _) = lex_flip(normal);
            Some(Curve::Circle {
                center,
                normal,
                radius,
            })
        }
        Curve::Ellipse {
            center,
            normal,
            major_axis,
            major_radius,
            minor_radius,
        } => {
            let (normal, _) = lex_flip(normal);
            Some(Curve::Ellipse {
                center,
                normal,
                major_axis,
                major_radius,
                minor_radius,
            })
        }
        _ => None,
    }
}

/// Bit-exact key of a canonical conic, usable as a `BTreeMap`/`BTreeSet`
/// key. Two edges share a key iff their curves are equal up to normal sign.
fn conic_ukey(c: &Curve) -> Option<Vec<u64>> {
    let canon = canonical_conic(c)?;
    let mut k: Vec<u64> = Vec::with_capacity(12);
    let push_p = |k: &mut Vec<u64>, p: Point3| {
        for x in p.as_array() {
            k.push(x.to_bits());
        }
    };
    let push_v = |k: &mut Vec<u64>, v: cad_primitives::Vector3| {
        for x in v.as_array() {
            k.push(x.to_bits());
        }
    };
    match canon {
        Curve::Circle {
            center,
            normal,
            radius,
        } => {
            k.push(1);
            push_p(&mut k, center);
            push_v(&mut k, normal);
            k.push(radius.to_bits());
        }
        Curve::Ellipse {
            center,
            normal,
            major_axis,
            major_radius,
            minor_radius,
        } => {
            k.push(2);
            push_p(&mut k, center);
            push_v(&mut k, normal);
            push_v(&mut k, major_axis);
            k.push(major_radius.to_bits());
            k.push(minor_radius.to_bits());
        }
        _ => return None,
    }
    Some(k)
}

/// The classify-band on-curve residual check (mirrors from_yang's endpoint
/// band: `TAU_EVAL · (1 + local_scale)` with the conic's radius and the
/// point's coordinate magnitude as the scale).
fn on_curve(canon: &Curve, p: Point3, t: f64) -> bool {
    let Some(q) = crate::geom::conic_eval(canon, t) else {
        return false;
    };
    let (pa, qa) = (p.as_array(), q.as_array());
    let d = ((pa[0] - qa[0]).powi(2) + (pa[1] - qa[1]).powi(2) + (pa[2] - qa[2]).powi(2)).sqrt();
    let r = match canon {
        Curve::Circle { radius, .. } => *radius,
        Curve::Ellipse { major_radius, .. } => *major_radius,
        _ => 0.0,
    };
    let coord = pa[0].abs().max(pa[1].abs()).max(pa[2].abs());
    d <= cad_primitives::TAU_EVAL * (1.0 + r.max(coord))
}

/// Wrap a parameter delta to (−π, π].
fn wrap_delta(mut d: f64) -> f64 {
    let two_pi = 2.0 * std::f64::consts::PI;
    while d > std::f64::consts::PI {
        d -= two_pi;
    }
    while d <= -std::f64::consts::PI {
        d += two_pi;
    }
    d
}

/// One run's merge decision, cached per canonical chain so the twin loop
/// reuses (and provably matches) it.
enum RunDecision {
    /// Piece boundaries as indices INTO THE CANONICAL CHAIN, strictly
    /// increasing, first == 0 and last == chain.len()-1 for open runs;
    /// for closed runs the boundaries wrap (last piece runs from the final
    /// boundary back to boundary 0).
    Merge {
        boundaries: Vec<usize>,
        canon: Curve,
    },
    Decline,
}

/// Decide whether the canonical chain merges, and where it splits.
/// `closed` chains have `chain[0]`'s predecessor equal to the final vertex
/// (the chain lists each vertex once).
fn decide_run(
    chain: &[u32],
    closed: bool,
    canon: &Curve,
    verts: &[Point3],
    stats: &mut MergeStats,
) -> RunDecision {
    let k = chain.len();
    // Params along the canonical curve.
    let mut ts: Vec<f64> = Vec::with_capacity(k);
    for &v in chain {
        match conic_param(canon, verts[v as usize]) {
            Some(t) => ts.push(t),
            None => {
                stats.declined_param += 1;
                return RunDecision::Decline;
            }
        }
    }
    // Interior vertices must lie ON the conic (endpoints are junction/run
    // boundaries that stay regardless; from_yang re-verifies them).
    let interior: Box<dyn Iterator<Item = usize>> = if closed {
        Box::new(0..k)
    } else {
        Box::new(1..k.saturating_sub(1))
    };
    for i in interior {
        if !on_curve(canon, verts[chain[i] as usize], ts[i]) {
            stats.declined_offcurve += 1;
            return RunDecision::Decline;
        }
    }
    // Strict wrap-aware monotonicity with a consistent sign; cumulative
    // sweep. For closed chains the wrap step (last → first) is included.
    let steps = if closed { k } else { k - 1 };
    let mut cum: Vec<f64> = Vec::with_capacity(steps + 1);
    cum.push(0.0);
    let mut sign = 0.0f64;
    for s in 0..steps {
        let d = wrap_delta(ts[(s + 1) % k] - ts[s]);
        if d == 0.0 {
            stats.declined_nonmonotone += 1;
            return RunDecision::Decline;
        }
        if sign == 0.0 {
            sign = d.signum();
        } else if d.signum() != sign {
            stats.declined_nonmonotone += 1;
            return RunDecision::Decline;
        }
        cum.push(cum[s] + d.abs());
    }
    let total = *cum.last().unwrap_or(&0.0);
    let two_pi = 2.0 * std::f64::consts::PI;
    if closed {
        // A monotone closed chain must sweep exactly one turn.
        if (total - two_pi).abs() > 1e-6 {
            stats.declined_nonmonotone += 1;
            return RunDecision::Decline;
        }
    } else if total >= two_pi {
        stats.declined_nonmonotone += 1;
        return RunDecision::Decline;
    }

    let n_pieces = if closed {
        4
    } else {
        ((total / SWEEP_MAX).ceil() as usize).max(1)
    };
    // Enough chain vertices to host the splits?
    if (closed && k < 4) || (!closed && n_pieces > k - 1) {
        stats.declined_short += 1;
        return RunDecision::Decline;
    }

    // Boundaries at the chain vertices where the cumulative sweep first
    // crosses each equal fraction of the total.
    let mut boundaries: Vec<usize> = Vec::with_capacity(n_pieces + 1);
    boundaries.push(0);
    for m in 1..n_pieces {
        let thr = total * (m as f64) / (n_pieces as f64);
        let lo = boundaries.last().copied().unwrap_or(0);
        let mut pick = None;
        // Search strictly after the previous boundary, keeping room for the
        // remaining pieces (interior positions only for open chains).
        let hi = if closed {
            k - 1
        } else {
            k - 1 - (n_pieces - m)
        };
        for (i, c) in cum.iter().enumerate().take(hi + 1).skip(lo + 1) {
            if *c >= thr {
                pick = Some(i);
                break;
            }
        }
        match pick {
            Some(i) => boundaries.push(i),
            None => {
                stats.declined_short += 1;
                return RunDecision::Decline;
            }
        }
    }
    if !closed {
        boundaries.push(k - 1);
    }
    // Post-selection certification: every piece must sweep strictly below
    // π − guard (the minor-side regime `orient_directed_curve` and
    // from_yang assume). Coarse chains can overshoot the threshold by one
    // long step; verify instead of reasoning about it.
    let piece_sweep = |a: usize, b: usize| -> f64 {
        if b > a {
            cum[b] - cum[a]
        } else {
            // closed wrap piece: from a through the wrap step back to the
            // first boundary
            total - cum[a] + cum[b]
        }
    };
    let limit = std::f64::consts::PI - PIECE_SWEEP_GUARD;
    let n_b = boundaries.len();
    let pieces: Vec<(usize, usize)> = if closed {
        (0..n_b)
            .map(|i| (boundaries[i], boundaries[(i + 1) % n_b]))
            .collect()
    } else {
        (0..n_b - 1)
            .map(|i| (boundaries[i], boundaries[i + 1]))
            .collect()
    };
    for &(a, b) in &pieces {
        let sw = piece_sweep(a, b);
        if !(sw > 0.0 && sw < limit) {
            stats.declined_sweep += 1;
            return RunDecision::Decline;
        }
    }
    RunDecision::Merge {
        boundaries,
        canon: *canon,
    }
}

/// The directed vertex cycle of a stored loop: `cycle[i]` is the start of
/// `edges[lp[i]]`. `None` if stored-direction continuity or closure fails
/// (defensive — emitted output loops satisfy both).
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

/// A loop's edge positions partitioned into stretches: `None` key = keep
/// verbatim; `Some(key)` = a conic stretch (candidate run). Positions are
/// rotated so no run wraps the vector end; the rotation offset is returned
/// so the rebuilt loop can preserve a deterministic start.
struct LoopPartition {
    /// (start position in ROTATED order, length, Option<ukey>)
    stretches: Vec<(usize, usize, Option<Vec<u64>>)>,
    /// Rotation applied: rotated position 0 = original position `offset`.
    offset: usize,
    /// The whole loop is a single closed conic run.
    whole_closed: bool,
}

fn partition_loop(
    edges: &[BRepEdge],
    lp: &[u32],
    cycle: &[u32],
    elidable: &BTreeSet<u32>,
) -> LoopPartition {
    let m = lp.len();
    let keys: Vec<Option<Vec<u64>>> = lp
        .iter()
        .map(|&ei| conic_ukey(&edges[ei as usize].curve))
        .collect();
    // Boundary after position i (between edge i and i+1, wrapping) is
    // mergeable iff both edges carry the same conic key and the shared
    // vertex is elidable.
    let mergeable_after = |i: usize| -> bool {
        let j = (i + 1) % m;
        keys[i].is_some() && keys[i] == keys[j] && elidable.contains(&cycle[j])
    };
    let first_break = (0..m).find(|&i| !mergeable_after(i));
    let Some(first_break) = first_break else {
        return LoopPartition {
            stretches: vec![(0, m, keys[0].clone())],
            offset: 0,
            whole_closed: true,
        };
    };
    // Rotate so position 0 starts just after the first break — every run is
    // then contiguous in rotated order.
    let offset = (first_break + 1) % m;
    let key_at = |r: usize| -> &Option<Vec<u64>> { &keys[(offset + r) % m] };
    let mut stretches: Vec<(usize, usize, Option<Vec<u64>>)> = Vec::new();
    let mut r = 0usize;
    while r < m {
        let k = key_at(r).clone();
        let mut len = 1usize;
        while r + len < m && *key_at(r + len) == k && {
            // the boundary between rotated r+len-1 and r+len
            let orig = (offset + r + len - 1) % m;
            mergeable_after(orig)
        } {
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

/// Canonicalize an OPEN run chain (first vertex ≤ last vertex; reverse
/// otherwise). Returns (canonical chain, reversed?).
fn canonical_open_chain(chain: &[u32]) -> (Vec<u32>, bool) {
    if chain.first() <= chain.last() {
        (chain.to_vec(), false)
    } else {
        (chain.iter().rev().copied().collect(), true)
    }
}

/// Canonicalize a CLOSED run chain: rotate the minimum vertex first, then
/// run toward the smaller-index neighbor. Returns (canonical chain,
/// reversed relative to input order?).
fn canonical_closed_chain(chain: &[u32]) -> (Vec<u32>, bool) {
    let k = chain.len();
    let (min_pos, _) = chain
        .iter()
        .enumerate()
        .min_by_key(|&(_, v)| *v)
        .expect("closed chain is non-empty");
    let fwd = chain[(min_pos + 1) % k];
    let bwd = chain[(min_pos + k - 1) % k];
    if fwd <= bwd {
        let rotated: Vec<u32> = (0..k).map(|i| chain[(min_pos + i) % k]).collect();
        (rotated, false)
    } else {
        let rotated: Vec<u32> = (0..k).map(|i| chain[(min_pos + k - i) % k]).collect();
        (rotated, true)
    }
}

/// Run the merge over the emitted topology. Never fails — every structural
/// or geometric surprise declines the affected run and keeps the
/// per-segment status quo.
pub(crate) fn merge_conic_seam_runs(
    verts: &[Point3],
    edges: &mut Vec<BRepEdge>,
    faces: &mut [BRepFace],
    sources: &mut [TessellationSource],
) -> MergeStats {
    let mut stats = MergeStats {
        edges_before: edges.len(),
        ..MergeStats::default()
    };

    // ── Census: global loop-edge uses per vertex ─────────────────────────
    // count, owning faces, and the set of undirected curve keys (None for
    // non-conic edges) over every loop incidence.
    struct VertUse {
        count: u32,
        faces: BTreeSet<u32>,
        keys: BTreeSet<Option<Vec<u64>>>,
    }
    let mut uses: BTreeMap<u32, VertUse> = BTreeMap::new();
    for (fi, f) in faces.iter().enumerate() {
        for lp in std::iter::once(&f.outer_loop).chain(f.inner_loops.iter()) {
            for &ei in lp {
                let Some(e) = edges.get(ei as usize) else {
                    continue;
                };
                let key = conic_ukey(&e.curve);
                for v in [e.start, e.end] {
                    let u = uses.entry(v).or_insert_with(|| VertUse {
                        count: 0,
                        faces: BTreeSet::new(),
                        keys: BTreeSet::new(),
                    });
                    u.count += 1;
                    u.faces.insert(fi as u32);
                    u.keys.insert(key.clone());
                }
            }
        }
    }
    let elidable: BTreeSet<u32> = uses
        .iter()
        .filter(|(_, u)| {
            u.count == 4
                && u.faces.len() == 2
                && u.keys.len() == 1
                && u.keys.iter().next().is_some_and(|k| k.is_some())
        })
        .map(|(&v, _)| v)
        .collect();

    // ── Decisions per canonical chain (cached: the twin reuses them) ─────
    let mut decisions: BTreeMap<Vec<u32>, RunDecision> = BTreeMap::new();
    // First-copy piece assignment for sources retagging.
    let mut vert_to_piece: BTreeMap<u32, u32> = BTreeMap::new();

    // ── Rebuild ──────────────────────────────────────────────────────────
    let mut new_edges: Vec<BRepEdge> = Vec::with_capacity(edges.len());
    let mut old2new: Vec<Option<u32>> = vec![None; edges.len()];
    let mut elided_this_pass: BTreeSet<u32> = BTreeSet::new();

    // Per (face, loop-slot) rebuilt loops; applied after the borrow ends.
    let mut rebuilt: Vec<Vec<Vec<u32>>> = Vec::with_capacity(faces.len());

    for f in faces.iter() {
        let mut face_loops: Vec<Vec<u32>> = Vec::new();
        for lp in std::iter::once(&f.outer_loop).chain(f.inner_loops.iter()) {
            let Some(cycle) = loop_cycle(edges, lp) else {
                stats.skipped_discontinuous_loops += 1;
                // Keep the loop verbatim (copy its edges unmerged).
                let mut kept = Vec::with_capacity(lp.len());
                for &ei in lp {
                    let ni = match old2new[ei as usize] {
                        Some(n) => n,
                        None => {
                            let n = new_edges.len() as u32;
                            new_edges.push(edges[ei as usize].clone());
                            old2new[ei as usize] = Some(n);
                            n
                        }
                    };
                    kept.push(ni);
                }
                face_loops.push(kept);
                continue;
            };
            let part = partition_loop(edges, lp, &cycle, &elidable);
            let m = lp.len();
            let mut new_lp: Vec<u32> = Vec::with_capacity(lp.len());

            // Emit one stretch: either verbatim copies or a decided merge.
            let mut emit_stretch = |start_r: usize,
                                    len: usize,
                                    key: &Option<Vec<u64>>,
                                    closed_whole: bool,
                                    new_lp: &mut Vec<u32>,
                                    new_edges: &mut Vec<BRepEdge>,
                                    stats: &mut MergeStats| {
                let orig_pos = |r: usize| (part.offset + r) % m;
                let mut verbatim = |new_lp: &mut Vec<u32>, new_edges: &mut Vec<BRepEdge>| {
                    for r in start_r..start_r + len {
                        let ei = lp[orig_pos(r)] as usize;
                        let ni = match old2new[ei] {
                            Some(n) => n,
                            None => {
                                let n = new_edges.len() as u32;
                                new_edges.push(edges[ei].clone());
                                old2new[ei] = Some(n);
                                n
                            }
                        };
                        new_lp.push(ni);
                    }
                };
                // Non-conic stretch, or too short to merge anything.
                let mergeable = key.is_some() && (closed_whole || len >= 2);
                if !mergeable {
                    verbatim(new_lp, new_edges);
                    return;
                }
                // The traversal chain of this stretch: vertices
                // cycle[orig_pos(start_r)] .. through len edges.
                let mut chain: Vec<u32> = Vec::with_capacity(len + 1);
                for r in start_r..start_r + len {
                    chain.push(cycle[orig_pos(r)]);
                }
                if !closed_whole {
                    // end vertex of the final edge
                    chain.push(cycle[orig_pos(start_r + len) % m]);
                }
                let (canon_chain, reversed) = if closed_whole {
                    canonical_closed_chain(&chain)
                } else {
                    canonical_open_chain(&chain)
                };
                // Canonical curve from the first edge of the stretch.
                let canon_curve = canonical_conic(&edges[lp[orig_pos(start_r)] as usize].curve);
                let Some(canon_curve) = canon_curve else {
                    verbatim(new_lp, new_edges);
                    return;
                };
                let decision = decisions.entry(canon_chain.clone()).or_insert_with(|| {
                    decide_run(&canon_chain, closed_whole, &canon_curve, verts, stats)
                });
                let RunDecision::Merge { boundaries, canon } = decision else {
                    verbatim(new_lp, new_edges);
                    return;
                };
                // Piece boundary vertex ids in canonical order.
                let b_verts: Vec<u32> = boundaries.iter().map(|&i| canon_chain[i]).collect();
                // Pieces as (start_vert, end_vert) in THIS loop's traversal
                // order.
                let piece_pairs: Vec<(u32, u32)> = if closed_whole {
                    let n = b_verts.len();
                    let fwd: Vec<(u32, u32)> =
                        (0..n).map(|i| (b_verts[i], b_verts[(i + 1) % n])).collect();
                    if reversed {
                        fwd.iter().rev().map(|&(a, b)| (b, a)).collect()
                    } else {
                        fwd
                    }
                } else {
                    let n = b_verts.len();
                    let fwd: Vec<(u32, u32)> =
                        (0..n - 1).map(|i| (b_verts[i], b_verts[i + 1])).collect();
                    if reversed {
                        fwd.iter().rev().map(|&(a, b)| (b, a)).collect()
                    } else {
                        fwd
                    }
                };
                let first_copy = !vert_to_piece.contains_key(&canon_chain[0])
                    || !elided_this_pass.contains(&canon_chain[0]);
                // Track stats once per canonical chain (first copy only).
                let count_stats = piece_pairs
                    .iter()
                    .all(|&(s, e)| !merged_edge_exists(new_edges, s, e, canon));
                for &(s, e) in &piece_pairs {
                    let ni = new_edges.len() as u32;
                    new_edges.push(BRepEdge {
                        start: s,
                        end: e,
                        curve: crate::stage5_topology::orient_directed_curve(*canon, s, e, verts),
                    });
                    new_lp.push(ni);
                    // First-copy piece assignment for every chain vertex the
                    // piece covers (interior + boundaries).
                    if count_stats {
                        assign_chain_to_piece(
                            &canon_chain,
                            boundaries,
                            closed_whole,
                            s,
                            e,
                            ni,
                            &mut vert_to_piece,
                        );
                    }
                }
                if count_stats {
                    stats.runs_merged += 1;
                    // Interior vertices elided by this run.
                    let interior: Vec<u32> = if closed_whole {
                        canon_chain
                            .iter()
                            .copied()
                            .filter(|v| !b_verts.contains(v))
                            .collect()
                    } else {
                        canon_chain[1..canon_chain.len() - 1]
                            .iter()
                            .copied()
                            .filter(|v| !b_verts.contains(v))
                            .collect()
                    };
                    stats.verts_elided += interior.len();
                    elided_this_pass.extend(interior);
                }
                let _ = first_copy;
            };

            if part.whole_closed {
                let key = part.stretches[0].2.clone();
                emit_stretch(0, m, &key, true, &mut new_lp, &mut new_edges, &mut stats);
                // A declined whole-closed run emits verbatim; a merged one
                // emits its pieces. Either way the loop is complete.
            } else {
                for (start_r, len, key) in part.stretches.clone() {
                    emit_stretch(
                        start_r,
                        len,
                        &key,
                        false,
                        &mut new_lp,
                        &mut new_edges,
                        &mut stats,
                    );
                }
            }
            face_loops.push(new_lp);
        }
        rebuilt.push(face_loops);
    }

    // Apply rebuilt loops.
    for (f, mut loops) in faces.iter_mut().zip(rebuilt) {
        f.outer_loop = loops.remove(0);
        f.inner_loops = loops;
    }

    // ── Sources: remap surviving indices; retag deleted run edges ────────
    for (v, src) in sources.iter_mut().enumerate() {
        if let TessellationSource::BRepEdge { edge, t } = *src {
            match old2new.get(edge as usize).copied().flatten() {
                Some(n) => {
                    *src = TessellationSource::BRepEdge { edge: n, t };
                }
                None => {
                    // The source edge was replaced by a merged piece. Retag
                    // to the covering piece with the parameter recomputed in
                    // the EMITTED curve's frame (eval_source round-trip).
                    let retag = vert_to_piece.get(&(v as u32)).and_then(|&piece| {
                        conic_param(&new_edges[piece as usize].curve, verts[v])
                            .map(|t2| TessellationSource::BRepEdge { edge: piece, t: t2 })
                    });
                    *src = retag.unwrap_or(TessellationSource::BRepVertex(v as u32));
                }
            }
        }
    }

    *edges = new_edges;
    stats.edges_after = edges.len();
    stats
}

/// Has an identical merged piece already been emitted? (Used only to count
/// stats/piece-assignment once per canonical chain — the twin's copies are
/// distinct edges by design.)
fn merged_edge_exists(new_edges: &[BRepEdge], s: u32, e: u32, canon: &Curve) -> bool {
    new_edges.iter().any(|ne| {
        ((ne.start == s && ne.end == e) || (ne.start == e && ne.end == s))
            && crate::stage4_correct::conics_equal_up_to_normal_sign(&ne.curve, canon)
    })
}

/// Record `piece` (a new edge index) as the covering piece of every chain
/// vertex in the canonical span `s → e`, first copy wins.
fn assign_chain_to_piece(
    canon_chain: &[u32],
    boundaries: &[usize],
    closed: bool,
    s: u32,
    e: u32,
    piece: u32,
    vert_to_piece: &mut BTreeMap<u32, u32>,
) {
    // Locate the canonical span [a, b] whose boundary vertices are {s, e}.
    let n_b = boundaries.len();
    let spans: Vec<(usize, usize)> = if closed {
        (0..n_b)
            .map(|i| (boundaries[i], boundaries[(i + 1) % n_b]))
            .collect()
    } else {
        (0..n_b - 1)
            .map(|i| (boundaries[i], boundaries[i + 1]))
            .collect()
    };
    for (a, b) in spans {
        let (va, vb) = (canon_chain[a], canon_chain[b]);
        if (va == s && vb == e) || (va == e && vb == s) {
            let k = canon_chain.len();
            let mut i = a;
            loop {
                vert_to_piece.entry(canon_chain[i]).or_insert(piece);
                if i == b {
                    break;
                }
                i = (i + 1) % k;
                if !closed && i == 0 {
                    break; // defensive: open spans never wrap
                }
            }
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cad_primitives::Vector3;

    fn circle(center: [f64; 3], normal: [f64; 3], radius: f64) -> Curve {
        Curve::Circle {
            center: Point3::new(center[0], center[1], center[2]),
            normal: Vector3::new(normal[0], normal[1], normal[2]),
            radius,
        }
    }

    /// A ring of `n` vertices on the unit circle in the z=0 plane starting
    /// at vertex index `base`, plus the per-segment edges of two twin faces
    /// (A forward, B reversed).
    fn ring_fixture(n: usize) -> (Vec<Point3>, Vec<BRepEdge>, Vec<BRepFace>) {
        let c = circle([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0);
        let verts: Vec<Point3> = (0..n)
            .map(|i| {
                let t = 2.0 * std::f64::consts::PI * (i as f64) / (n as f64);
                Point3::new(t.cos(), t.sin(), 0.0)
            })
            .collect();
        let mut edges = Vec::new();
        // Face A: forward per-segment ring.
        let a_loop: Vec<u32> = (0..n)
            .map(|i| {
                edges.push(BRepEdge {
                    start: i as u32,
                    end: ((i + 1) % n) as u32,
                    curve: c,
                });
                (edges.len() - 1) as u32
            })
            .collect();
        // Face B: reversed per-segment ring (twin traversal).
        let b_loop: Vec<u32> = (0..n)
            .map(|i| {
                let s = (n - i) % n;
                let e = (n - i - 1) % n;
                edges.push(BRepEdge {
                    start: s as u32,
                    end: e as u32,
                    curve: c,
                });
                (edges.len() - 1) as u32
            })
            .collect();
        let plane = crate::geom::Surface::Plane {
            normal: Vector3::new(0.0, 0.0, 1.0),
            d: 0.0,
        };
        let faces = vec![
            BRepFace {
                surface: plane,
                outer_loop: a_loop,
                inner_loops: Vec::new(),
                reversed: false,
            },
            BRepFace {
                surface: plane,
                outer_loop: b_loop,
                inner_loops: Vec::new(),
                reversed: false,
            },
        ];
        (verts, edges, faces)
    }

    #[test]
    fn closed_ring_merges_to_four_arcs_per_side() {
        let (verts, mut edges, mut faces) = ring_fixture(16);
        let mut sources: Vec<TessellationSource> = (0..verts.len() as u32)
            .map(TessellationSource::BRepVertex)
            .collect();
        let stats = merge_conic_seam_runs(&verts, &mut edges, &mut faces, &mut sources);
        assert_eq!(stats.runs_merged, 1, "{stats:?}");
        assert_eq!(edges.len(), 8, "4 arcs per twin: {stats:?}");
        assert_eq!(faces[0].outer_loop.len(), 4);
        assert_eq!(faces[1].outer_loop.len(), 4);
        // Twin conformance: identical undirected piece sets.
        let pair = |lp: &[u32], edges: &[BRepEdge]| -> BTreeSet<(u32, u32)> {
            lp.iter()
                .map(|&ei| {
                    let e = &edges[ei as usize];
                    (e.start.min(e.end), e.start.max(e.end))
                })
                .collect()
        };
        assert_eq!(
            pair(&faces[0].outer_loop, &edges),
            pair(&faces[1].outer_loop, &edges)
        );
        // Loops stay stored-direction continuous.
        for f in &faces {
            for (i, &ei) in f.outer_loop.iter().enumerate() {
                let nxt = f.outer_loop[(i + 1) % f.outer_loop.len()];
                assert_eq!(edges[ei as usize].end, edges[nxt as usize].start);
            }
        }
        // Every piece sweeps well below π.
        for e in &edges {
            let (s, en) = (verts[e.start as usize], verts[e.end as usize]);
            let ang = |p: Point3| p.as_array()[1].atan2(p.as_array()[0]);
            let sw = wrap_delta(ang(en) - ang(s)).abs();
            assert!(sw < std::f64::consts::PI - 1e-3, "sweep {sw}");
        }
    }

    #[test]
    fn junction_vertex_splits_the_run() {
        let (verts, mut edges, mut faces) = ring_fixture(16);
        // A third face's loop touching vertex 5 makes it non-elidable
        // (6 uses / 3 faces). Give it a degenerate-but-continuous triangle
        // loop of segments 5→0→8→5 (content irrelevant — only the census
        // counts matter).
        let seg = |s: u32, e: u32| BRepEdge {
            start: s,
            end: e,
            curve: Curve::LineSegment,
        };
        let base = edges.len() as u32;
        edges.push(seg(5, 0));
        edges.push(seg(0, 8));
        edges.push(seg(8, 5));
        faces_push_triangle(&mut faces, base);
        let mut sources: Vec<TessellationSource> = (0..verts.len() as u32)
            .map(TessellationSource::BRepVertex)
            .collect();
        let stats = merge_conic_seam_runs(&verts, &mut edges, &mut faces, &mut sources);
        // Vertices 0, 5, 8 are pinned; the ring becomes 3 open runs.
        assert!(stats.runs_merged >= 2, "{stats:?}");
        for f in &faces[..2] {
            for &ei in &f.outer_loop {
                let e = &edges[ei as usize];
                assert!(matches!(e.curve, Curve::Circle { .. }));
            }
            // Pinned vertices survive in the loop.
            let vs: BTreeSet<u32> = f
                .outer_loop
                .iter()
                .flat_map(|&ei| [edges[ei as usize].start, edges[ei as usize].end])
                .collect();
            for pinned in [0u32, 5, 8] {
                assert!(vs.contains(&pinned), "pinned {pinned} missing");
            }
        }
    }

    fn faces_push_triangle(faces: &mut Vec<BRepFace>, base: u32) {
        faces.push(BRepFace {
            surface: crate::geom::Surface::Plane {
                normal: Vector3::new(0.0, 0.0, 1.0),
                d: 0.0,
            },
            outer_loop: vec![base, base + 1, base + 2],
            inner_loops: Vec::new(),
            reversed: false,
        });
    }

    #[test]
    fn off_curve_vertex_declines_the_run() {
        let (mut verts, mut edges, mut faces) = ring_fixture(16);
        // Perturb an interior vertex well off the circle.
        verts[3] = Point3::new(verts[3].as_array()[0] + 1e-3, verts[3].as_array()[1], 0.0);
        let mut sources: Vec<TessellationSource> = (0..verts.len() as u32)
            .map(TessellationSource::BRepVertex)
            .collect();
        let before = edges.len();
        let stats = merge_conic_seam_runs(&verts, &mut edges, &mut faces, &mut sources);
        assert_eq!(stats.runs_merged, 0, "{stats:?}");
        assert_eq!(stats.declined_offcurve, 1, "{stats:?}");
        assert_eq!(edges.len(), before);
        assert_eq!(faces[0].outer_loop.len(), 16);
    }

    #[test]
    fn non_monotone_chain_declines() {
        let (mut verts, mut edges, mut faces) = ring_fixture(16);
        // Swap two adjacent ring vertices' positions: the chain folds.
        verts.swap(6, 7);
        let mut sources: Vec<TessellationSource> = (0..verts.len() as u32)
            .map(TessellationSource::BRepVertex)
            .collect();
        let stats = merge_conic_seam_runs(&verts, &mut edges, &mut faces, &mut sources);
        assert_eq!(stats.runs_merged, 0, "{stats:?}");
        assert!(stats.declined_nonmonotone >= 1, "{stats:?}");
    }

    #[test]
    fn segment_loops_untouched() {
        // A plain square (all LineSegment) must pass through unchanged.
        let verts = vec![
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(1.0, 0.0, 0.0),
            Point3::new(1.0, 1.0, 0.0),
            Point3::new(0.0, 1.0, 0.0),
        ];
        let seg = |s: u32, e: u32| BRepEdge {
            start: s,
            end: e,
            curve: Curve::LineSegment,
        };
        let mut edges = vec![seg(0, 1), seg(1, 2), seg(2, 3), seg(3, 0)];
        let mut faces = vec![BRepFace {
            surface: crate::geom::Surface::Plane {
                normal: Vector3::new(0.0, 0.0, 1.0),
                d: 0.0,
            },
            outer_loop: vec![0, 1, 2, 3],
            inner_loops: Vec::new(),
            reversed: false,
        }];
        let mut sources: Vec<TessellationSource> =
            (0..4u32).map(TessellationSource::BRepVertex).collect();
        let stats = merge_conic_seam_runs(&verts, &mut edges, &mut faces, &mut sources);
        assert_eq!(stats.runs_merged, 0);
        assert_eq!(edges.len(), 4);
        assert_eq!(faces[0].outer_loop, vec![0, 1, 2, 3]);
    }

    #[test]
    fn sources_retag_onto_pieces_and_round_trip() {
        let (verts, mut edges, mut faces) = ring_fixture(16);
        // Tag every ring vertex as relocated onto its incident segment edge
        // (as the emission does), with an arbitrary t.
        let mut sources: Vec<TessellationSource> = (0..16u32)
            .map(|v| TessellationSource::BRepEdge { edge: v, t: 0.0 })
            .collect();
        let stats = merge_conic_seam_runs(&verts, &mut edges, &mut faces, &mut sources);
        assert_eq!(stats.runs_merged, 1, "{stats:?}");
        for (v, src) in sources.iter().enumerate() {
            let TessellationSource::BRepEdge { edge, t } = *src else {
                panic!("vertex {v} lost its edge source: {src:?}");
            };
            assert!((edge as usize) < edges.len(), "dangling edge index");
            let q = crate::geom::conic_eval(&edges[edge as usize].curve, t).expect("conic eval");
            let (pa, qa) = (verts[v].as_array(), q.as_array());
            let d = ((pa[0] - qa[0]).powi(2) + (pa[1] - qa[1]).powi(2) + (pa[2] - qa[2]).powi(2))
                .sqrt();
            assert!(d < 1e-9, "vertex {v} round-trip {d}");
        }
    }

    #[test]
    fn open_run_splits_by_sweep_cap() {
        // Two junction-pinned vertices 120° apart split the ring into a
        // 240° major run (splits into 2 pieces at the 1.8 rad cap) and a
        // 120° minor run (1 piece).
        let (verts, mut edges, mut faces) = ring_fixture(18);
        // Pin verts 0 and 6 (120°) via a third face.
        let seg = |s: u32, e: u32| BRepEdge {
            start: s,
            end: e,
            curve: Curve::LineSegment,
        };
        let base = edges.len() as u32;
        edges.push(seg(0, 6));
        edges.push(seg(6, 9));
        edges.push(seg(9, 0));
        faces_push_triangle(&mut faces, base);
        // vertex 9 is also pinned by the triangle — so runs are 0→6 (120°),
        // 6→9 (60°), 9→0 (180°... exactly π! → must split into 2).
        let mut sources: Vec<TessellationSource> = (0..verts.len() as u32)
            .map(TessellationSource::BRepVertex)
            .collect();
        let stats = merge_conic_seam_runs(&verts, &mut edges, &mut faces, &mut sources);
        assert!(stats.runs_merged >= 3, "{stats:?}");
        // No merged arc may sweep ≥ π − guard.
        for e in &edges {
            if matches!(e.curve, Curve::Circle { .. }) && e.start != e.end {
                let ang = |v: u32| {
                    let p = verts[v as usize].as_array();
                    p[1].atan2(p[0])
                };
                let sw = wrap_delta(ang(e.end) - ang(e.start)).abs();
                assert!(sw < std::f64::consts::PI - 1e-3, "sweep {sw}");
            }
        }
    }
}
