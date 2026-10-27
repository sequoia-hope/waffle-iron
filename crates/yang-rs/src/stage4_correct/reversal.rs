//! Stage-4 reversed-intersection sweep and the curve-comparison helpers it
//! relies on: PR-YR10 §4.5.3 ordered-loop reversal correction
//! (`sweep_reversed_intersections`), conic parameterization/equality tests,
//! mixed-cycle shared-conic detection, and the collapse/merge direction
//! predicates. Extracted move-only from stage4_correct.rs (#159 F9).

#[allow(clippy::wildcard_imports)]
use crate::*;

/// PR-YR10 (§4.5.3): walk every ordered intersection loop and correct reversed
/// points in the paper's ORDER (`refs/text/yang2025_hybrid_boolean.txt:746-751`):
/// "for a reversal point p_r, we remove its next point and reconnect … until
/// there is no reversal. If no such next point can be found, we remove p_r and
/// restart the correction from p_b" — on each intersection curve C between its
/// endpoints, which are never removed ([`paper_order_removal`]). Returns
/// `true` iff any point was removed. LOUD STOP on an unresolvable reversal.
#[allow(clippy::too_many_arguments)] // the Stage-4 driver's own state, handed through once (§3e added the mint map)
pub(crate) fn sweep_reversed_intersections(
    mesh: &mut Mesh,
    attribution: &mut Vec<Option<TriangleAttribution>>,
    side_labels: &mut Vec<TriangleSideLabel>,
    a: &BRep,
    b: &BRep,
    d_eps: f64,
    moved: &std::collections::HashSet<u32>,
    entry_pos: &[[f64; 3]],
    minted_junction_keys: &std::collections::BTreeMap<[u64; 3], crate::boolean::MintProvenance>,
) -> Result<bool, YangError> {
    use std::collections::HashSet;
    const ANG_TOL: f64 = 1e-6; // radians (Yang §5).
    let lo = std::f64::consts::FRAC_PI_4 - ANG_TOL; // 45° − tol
    let hi = 3.0 * std::f64::consts::FRAC_PI_4 + ANG_TOL; // 135° + tol

    let mut collapsed_any = false;
    // Bound the outer restart loop by the initial triangle count (each pass
    // either makes progress by collapsing ≥1 triangle or terminates).
    let max_passes = mesh.tris.len() + 1;
    let mut passes = 0usize;
    // The reversed point of the latest removal: a sweep that cannot reach a
    // fixed point names it, so the loop has faces to refine (matrix F-3).
    let mut last_pr = u32::MAX;
    loop {
        passes += 1;
        if passes > max_passes {
            // Could not reach a fixed point — genuine §4.5.2 territory. The
            // Stage-4 wrapper records the faces at `last_pr` (T1).
            return Err(YangError::stage4_region_invalid(
                last_pr,
                Stage4InvalidReason::LocalRefinementRequired,
            ));
        }
        // Reversals this pass leaves in place at the 2·d_ε gate below.
        let mut refused: Vec<(u32, u32, f64)> = Vec::new();

        // Recompute Phase A so the loops reflect any prior collapse (spec §4.5.3
        // step 3 — re-sweep on fresh loops, never stale ones).
        let map = TriangleAttributionMap {
            attributions: std::mem::take(attribution),
        };
        // Post-relocation context: position keys are stale, pass no provenance.
        let phase_a = compute_phase_a(mesh, &map, a, b, &crate::stage3_ssi::NO_EDGE_PROVENANCE);
        *attribution = map.attributions;
        let (infos, incidence, curves) = phase_a?;
        // Spec §3e (P0057): the Stage-1 junction MINTS present in this pass,
        // resolved by position bits (mint coordinates are never mutated). A
        // mint is an exact corner by construction — an operand edge through
        // the partner's surface — whatever the labeling kept around it, so it
        // is an endpoint of every curve C through it.
        let minted: HashSet<u32> = if minted_junction_keys.is_empty() {
            HashSet::new()
        } else {
            mesh.verts
                .iter()
                .enumerate()
                .filter(|(_, p)| {
                    minted_junction_keys.contains_key(&[
                        p.x().to_bits(),
                        p.y().to_bits(),
                        p.z().to_bits(),
                    ])
                })
                .map(|(v, _)| v as u32)
                .collect()
        };

        // The distinct surfaces each vertex's incidence edges carry (the
        // §3d rank, [`distinct_surfaces_at`]), read once per pass.
        let surfaces_at = surfaces_by_vertex(&incidence);

        // Collect the ordered intersection loops. Dedup by sorted vertex set so
        // the cylinder-side and cap-side copies of the same ring are swept once.
        let mut seen: HashSet<Vec<u32>> = HashSet::new();
        let mut loops: Vec<(Vec<(u32, u32)>, bool)> = Vec::new();
        for info in &infos {
            for cycle in &info.cycles {
                if cycle.len() < 3 {
                    continue;
                }
                // Yang et al. 2025 §4.5.3 (`refs/text/yang2025_hybrid_boolean.txt:679-689,
                // 718-751`, Fig. 15; ledger N89 item 3): admit every boundary cycle that
                // contains at least one cross-operand (`A × B`) intersection edge by
                // Phase-A incidence alone, uniformly across all `Curve` variants (typed
                // `Circle`/`Ellipse`/`Parabola`/`Hyperbola`/`LineSegment`/`SurfacePair`
                // and untyped pair-relocated edges alike; `build_intersection_curves`
                // requires `A × B` incidence, so this subsumes both typed and untyped
                // intersection runs without enumerating curve types).
                let any_cross_incidence = cycle.iter().any(|&(s, e)| {
                    let key = if s < e { (s, e) } else { (e, s) };
                    incidence.get(&key).is_some_and(|entries| {
                        entries.iter().any(|&(i, _)| i == InputId::A)
                            && entries.iter().any(|&(i, _)| i == InputId::B)
                    })
                });
                if !any_cross_incidence {
                    continue;
                }
                // Spec §3c final scope + N89 item 3: ALL-CONIC cycles (every edge a
                // planar conic — `geom::curve_plane(c).is_some()`, covering closed
                // `Circle`/`Ellipse` rings and multi-arc `Parabola`/`Hyperbola` loops
                // alike without a curve-variant list) take the §4.5.3 angle band at
                // every single-curve site; in MIXED cycles only straight-run sites
                // (both incident edges `LineSegment`) take the band while conic sites
                // take exact parameter order (`mixed_cycle_shared_conic` — spec §3c P10
                // records: a 7-gon's 51° corners exceed it and overlay-adjacent conic
                // runs repair unsupported Stage-0 crossings into silent geometry).
                let all_conic = cycle.iter().all(|&(s, e)| {
                    let key = if s < e { (s, e) } else { (e, s) };
                    curves
                        .get(&key)
                        .is_some_and(|c| crate::geom::curve_plane(c).is_some())
                });
                let mut sorted: Vec<u32> = cycle.iter().map(|&(s, _)| s).collect();
                sorted.sort_unstable();
                if seen.insert(sorted) {
                    loops.push((cycle.clone(), all_conic));
                }
            }
        }

        // Find the FIRST reversal across all loops; remove one point, then
        // restart the whole sweep (re-deriving loops). Deterministic: loops are
        // in the deterministic patch/cycle order; within a loop the walk
        // starts at a curve endpoint (the paper walks C from its start) and
        // runs in loop order.
        let mut acted = false;
        'outer: for (cycle, all_conic) in &loops {
            let m = cycle.len();
            if m < 3 {
                return Err(YangError::stage4_region_invalid(
                    cycle.first().map(|&(s, _)| s).unwrap_or(u32::MAX),
                    Stage4InvalidReason::LoopTooSmall,
                ));
            }
            // Ordered vertex sequence of the loop (start vertices), rotated to
            // begin at the first endpoint of a curve C when the loop has one.
            let raw: Vec<u32> = cycle.iter().map(|&(s, _)| s).collect();
            let end_at = |vs: &[u32], k: usize| {
                let v = vs[k];
                let rank = surfaces_at.get(&v).map_or(0, Vec::len);
                let rank = if minted.contains(&v) {
                    rank.max(3)
                } else {
                    rank
                };
                curve_endpoint_ranked(
                    &curves,
                    &incidence,
                    rank,
                    vs[(k + m - 1) % m],
                    v,
                    vs[(k + 1) % m],
                )
            };
            let start_opt = (0..m).find(|&k| end_at(&raw, k));
            let (verts, endpoint) = if let Some(start) = start_opt {
                let verts: Vec<u32> = (0..m).map(|k| raw[(start + k) % m]).collect();
                let endpoint: Vec<bool> = (0..m).map(|k| end_at(&verts, k)).collect();
                (verts, endpoint)
            } else {
                // Zero topological corners (`end_at == false` around the entire
                // cycle: `rank < 3` everywhere and one shared curve `c0`). A
                // periodic conic (`conic_param_periodic` — `S¹`) legitimately
                // closes with no endpoints; a non-periodic open conic
                // (`!conic_param_periodic` — `ℝ`, cf. `stage4_splice::order_along_curve`)
                // cannot wrap periodically, so its parameter extrema `[t_min, t_max]`
                // bound the open arc (preserving any scaffold closing chord while
                // keeping interior arc vertices live in the §4.5.3 sweep).
                let e0 = cycle[0];
                let key0 = if e0.0 < e0.1 { e0 } else { (e0.1, e0.0) };
                if let Some(c0) = curves.get(&key0).filter(|c| !conic_param_periodic(c)) {
                    let params: Option<Vec<f64>> = raw
                        .iter()
                        .map(|&v| conic_param(c0, mesh.verts[v as usize]).filter(|t| t.is_finite()))
                        .collect();
                    if let Some(ts) = params {
                        let k_min = (0..m)
                            .min_by(|&a, &b| {
                                ts[a].total_cmp(&ts[b]).then_with(|| raw[a].cmp(&raw[b]))
                            })
                            .unwrap_or(0);
                        let k_max = (0..m)
                            .max_by(|&a, &b| {
                                ts[a].total_cmp(&ts[b]).then_with(|| raw[a].cmp(&raw[b]))
                            })
                            .unwrap_or(0);
                        let verts: Vec<u32> = (0..m).map(|k| raw[(k_min + k) % m]).collect();
                        let mut endpoint = vec![false; m];
                        endpoint[0] = true;
                        endpoint[(k_max + m - k_min) % m] = true;
                        (verts, endpoint)
                    } else {
                        (raw, vec![false; m])
                    }
                } else {
                    (raw, vec![false; m])
                }
            };
            for i in 0..m {
                let p_b = verts[(i + m - 1) % m];
                let p_r = verts[i];
                let p_n = verts[(i + 1) % m];
                let key_n = if p_r < p_n { (p_r, p_n) } else { (p_n, p_r) };
                let key_b = if p_b < p_r { (p_b, p_r) } else { (p_r, p_b) };
                // Spec `yang_453_line_overtake`: an OVERTAKE verdict (the
                // straight-run arm, or the carried-curve arm on an operand's
                // own B-Rep edge) names its own (victim = p_r, survivor = the
                // moved junction J) and is certified by J's displacement, so
                // the 2·d_ε resolution gate is not its bound.
                let mut overtake: Option<(u32, u32)> = None;
                // Otherwise the site's reversal test on curve C, evaluable
                // against any later point of C for the paper's lookahead.
                let mut test: Option<SiteTest> = None;
                if !all_conic {
                    if !curves.contains_key(&key_n) && !curves.contains_key(&key_b) {
                        // Spec `yang_453_line_overtake` §6 (P0036): a
                        // both-untyped site on ONE operand's own typed B-Rep
                        // edge (a carried rim circle) overtaken by a relocated
                        // junction — tested FIRST, since a single-input run
                        // is never a pair site.
                        overtake = carried_curve_overtake_site(
                            mesh, a, b, &curves, &incidence, moved, entry_pos, p_b, p_r, p_n,
                        );
                        if overtake.is_none() {
                            // Spec `yang_453_pair_chain_reversal` §3: BOTH
                            // incident edges untyped on one surface pair.
                            let Some(t) = pair_site_tangent(mesh, &incidence, p_b, p_r, p_n) else {
                                continue;
                            };
                            test = Some(SiteTest::Progression(t));
                        }
                    } else if !both_line_edges(&curves, key_b, key_n) {
                        // Task #145 branches 9–12: a shared-conic site in a
                        // mixed cycle is tested in exact PARAMETER order
                        // (never the angle band, whose coarse-chord false
                        // positives are P10-disproven here).
                        let Some(shared) = mixed_cycle_shared_conic(&curves, key_b, key_n) else {
                            continue;
                        };
                        test = Some(SiteTest::ParamOrder(shared));
                    }
                }
                // Spec `yang_453_line_overtake`: a relocated vertex that moved
                // ALONG its straight run past unmoved run points (R0059),
                // tested on the run's own geometry.
                if overtake.is_none() && test.is_none() {
                    overtake = line_overtake_site(
                        mesh, &curves, &incidence, moved, entry_pos, p_b, p_r, p_n,
                    );
                }
                if overtake.is_none() && test.is_none() {
                    let Some(tan) = angle_site_tangent(mesh, &curves, &incidence, p_b, p_r, p_n)
                    else {
                        continue;
                    };
                    test = Some(SiteTest::AngleBand(tan));
                }
                let (victim, survivor) = if let Some(vs) = overtake {
                    vs
                } else {
                    let Some(test) = test else { continue };
                    // The paper tests C's points between its endpoints; an
                    // endpoint (a corner — three surfaces, or a Stage-1 mint)
                    // is exact and is never p_r.
                    if endpoint[i] {
                        continue;
                    }
                    let reversed_with = |x: u32| site_reversed(mesh, &test, p_b, p_r, x, lo, hi);
                    if !reversed_with(p_n) {
                        continue;
                    }
                    let (victim, survivor) =
                        paper_order_removal(&verts, &endpoint, i, reversed_with, |x, y| {
                            let (px, py) = (
                                mesh.verts[x as usize].as_array(),
                                mesh.verts[y as usize].as_array(),
                            );
                            ((px[0] - py[0]).powi(2)
                                + (px[1] - py[1]).powi(2)
                                + (px[2] - py[2]).powi(2))
                            .sqrt()
                        });
                    // Spec §3c resolution gate: §4.5.3 corrects RESOLUTION
                    // artifacts ("the mesh resolution is not sufficient to
                    // maintain a one-to-one mapping") — both the removed
                    // point and the point it joins sit within their own
                    // Stage-1 chord band of the true curve position, so a
                    // legitimate correction moves at most 2·d_ε (the sum of
                    // the two bands — derived, not widening). A LARGER
                    // excursion is not a resolution artifact but wrong
                    // topology (e.g. an unsupported Stage-0 crossing) — leave
                    // the reversal in place and, at the fixed point, record it
                    // as a T7 site naming its faces, so the §4.5.2 loop
                    // refines it and a survivor STOPs loudly at the ceiling
                    // (P9; pinned by `annular_cap_hole_crossing_stays_loud`;
                    // matrix F-3 — until 2026-10-17 it left silently).
                    let pv = mesh.verts[victim as usize].as_array();
                    let ps = mesh.verts[survivor as usize].as_array();
                    let d = [pv[0] - ps[0], pv[1] - ps[1], pv[2] - ps[2]];
                    let dist = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
                    if dist > 2.0 * d_eps {
                        if std::env::var_os("YANG_T145_SWEEP_PROBE").is_some() {
                            eprintln!(
                                "[t145-gate] REFUSED victim={victim} survivor={survivor} \
                                 dist={dist:.3e} 2d_eps={:.3e}",
                                2.0 * d_eps
                            );
                        }
                        refused.push((p_r, survivor, dist));
                        continue;
                    }
                    (victim, survivor)
                };
                if std::env::var_os("YANG_V_PROBE").is_some() {
                    eprintln!(
                        "YANG_V_PROBE reversal collapse: p_b={p_b} p_r={p_r} p_n={p_n} \
                         victim={victim} survivor={survivor} at {:?} <- {:?} \
                         all_conic={all_conic} loop={verts:?}",
                        mesh.verts.get(survivor as usize),
                        mesh.verts.get(victim as usize),
                    );
                }
                if std::env::var_os("YANG_DOUBLECOVER_PROBE").is_some() {
                    eprintln!(
                        "[collapse-site] s4.5.3-reversal victim={victim} survivor={survivor}"
                    );
                }
                let dropped = collapse_vertex(mesh, attribution, side_labels, victim, survivor);
                if dropped == 0 {
                    // Nothing collapsed ⇒ cannot make progress on this
                    // reversal. LOUD STOP, naming the faces at the site (T7).
                    crate::boolean::failures::record_reversal_unresolved(
                        mesh,
                        attribution,
                        &[p_r, victim, survivor],
                        format!("v{p_r} reversal on edge {key_n:?}: removal made no progress"),
                    );
                    return Err(YangError::Stage4ReversalUnresolved {
                        edge: key_n,
                        vertex: p_r,
                    });
                }
                collapsed_any = true;
                acted = true;
                last_pr = p_r;
                break 'outer;
            }
        }

        if !acted {
            // Fixed point: no removable reversal remains. A reversal the
            // 2·d_ε gate left in place is not a resolution artifact this
            // sweep may remove; it is a failure the §4.5.2 loop refines
            // (T7), never a silent survivor (matrix F-3).
            for &(p_r, survivor, dist) in &refused {
                crate::boolean::failures::record_reversal_unresolved(
                    mesh,
                    attribution,
                    &[p_r, survivor],
                    format!(
                        "v{p_r} reversal left in place: removal toward v{survivor} \
                         moves {dist:.3e} > 2·d_eps {:.3e}",
                        2.0 * d_eps
                    ),
                );
            }
            return Ok(collapsed_any);
        }
    }
}

/// The reversal test of one §4.5.3 site `p_r` on its curve C, evaluable
/// against any later point `x` of C (the paper's "new tangent of the polyline
/// at p_r" after a removal, `:746-748`).
#[derive(Clone, Copy, Debug)]
pub(crate) enum SiteTest {
    /// Yang Fig. 15 verbatim: the discrete tangent `t̃` against the exact
    /// tangent of C at `p_r` (`None` when unavailable — only the collinear
    /// U-turn is then diagnosable).
    AngleBand(Option<[f64; 3]>),
    /// Task #145 / M5: the exact parameter order on the shared typed curve.
    ParamOrder(Curve),
    /// Spec `yang_453_pair_chain_reversal`: the progression sign along the
    /// unit tangent `n_A × n_B` of an untyped surface-pair chain.
    Progression([f64; 3]),
}

/// Is the polyline `p_b → p_r → x` reversed at `p_r` under `test`?
pub(crate) fn site_reversed(
    mesh: &Mesh,
    test: &SiteTest,
    p_b: u32,
    p_r: u32,
    x: u32,
    lo: f64,
    hi: f64,
) -> bool {
    let pb = mesh.verts[p_b as usize];
    let pr = mesh.verts[p_r as usize];
    let px = mesh.verts[x as usize];
    match test {
        SiteTest::AngleBand(tan) => angle_band_reversed(pb, pr, px, *tan, lo, hi),
        SiteTest::ParamOrder(curve) => {
            let Some((d1, d2)) = conic_param_deltas(curve, pb, pr, px) else {
                return false;
            };
            let reversed = d1 * d2 < 0.0;
            if std::env::var_os("YANG_T145_SWEEP_PROBE").is_some() {
                eprintln!(
                    "[t145-arm] site p_b={p_b} p_r={p_r} x={x} d1={d1:.3e} d2={d2:.3e} \
                     reversed={reversed} pos_b={pb:?} pos_r={pr:?} pos_x={px:?}"
                );
            }
            reversed
        }
        SiteTest::Progression(t) => {
            let (b, r, n) = (pb.as_array(), pr.as_array(), px.as_array());
            let d1 = (r[0] - b[0]) * t[0] + (r[1] - b[1]) * t[1] + (r[2] - b[2]) * t[2];
            let d2 = (n[0] - r[0]) * t[0] + (n[1] - r[1]) * t[1] + (n[2] - r[2]) * t[2];
            d1 * d2 < 0.0
        }
    }
}

/// Yang §4.5.3's correction ORDER (`refs/text/yang2025_hybrid_boolean.txt:746-751`)
/// for a reversal detected at `verts[i]` = `p_r`: returns `(victim,
/// survivor)` for [`collapse_vertex`].
///
/// "We repetitively remove the next point p_n and reconnect the curve until
/// there is no reversal. If no such next point can be found, we remove p_r and
/// restart the correction from p_b." The search runs over C's later points
/// `v_2, v_3, …` up to and including C's next endpoint (`endpoint[k]`: a
/// curve change, a run end, a corner on three surfaces or a Stage-1 mint —
/// exact, never removed). If some `v_j` resolves the reversal
/// (`!reversed_with(v_j)`), the next point `p_n` is removed onto `p_r` (the
/// sweep re-detects `p_r` on the fresh loop and removes the points up to
/// `v_j` one by one). If none does — `p_n` is C's endpoint, or every later
/// point of C stays reversed (`p_r` itself is the point out of order) — `p_r`
/// is removed and the sweep, restarting from the loop's start, reaches `p_b`
/// again. A removed point joins its NEARER polyline neighbour (`dist`): the
/// polyline after removal is the paper's `p_b → p_n` either way, and the
/// shorter collapse is the smaller mesh motion. The search is the paper's
/// "such next point": a removal is committed only once it is known to resolve
/// the reversal, so a reversal at `p_r` never strips C of its good points.
pub(crate) fn paper_order_removal(
    verts: &[u32],
    endpoint: &[bool],
    i: usize,
    reversed_with: impl Fn(u32) -> bool,
    dist: impl Fn(u32, u32) -> f64,
) -> (u32, u32) {
    let m = verts.len();
    let p_b = verts[(i + m - 1) % m];
    let p_r = verts[i];
    let p_n = verts[(i + 1) % m];
    if !endpoint[(i + 1) % m] {
        for j in 2..m - 1 {
            let k = (i + j) % m;
            if !reversed_with(verts[k]) {
                return (p_n, p_r);
            }
            if endpoint[k] {
                break;
            }
        }
    }
    if dist(p_r, p_b) <= dist(p_r, p_n) {
        (p_r, p_b)
    } else {
        (p_r, p_n)
    }
}

/// Is `v` an ENDPOINT of the intersection curve C that its loop edges
/// `(u, v)` and `(v, w)` lie on? Yang §4.5.3's correction runs on "the
/// intersection curve C between the two surfaces S_A and S_B" (`:709-745`);
/// C ends where the loop leaves that curve — the two edges carry different
/// Stage-3 curves, the run ends (an edge with no intersection curve beyond),
/// a straight run changes surface pair, an untyped pair chain changes pair —
/// or at a corner on three or more surfaces, which is the closed-form
/// endpoint of every curve through it whatever the loop's next edge says
/// (spec `yang_453_junction_protected_collapse` §3d, P0028: a cap sliver
/// bounded by one ellipse on every side), including a Stage-1 junction mint
/// (§3e, P0057, [`incidence_rank`]). An endpoint is exact and never removed.
#[cfg(test)]
pub(crate) fn curve_endpoint(
    curves: &std::collections::BTreeMap<(u32, u32), Curve>,
    incidence: &std::collections::BTreeMap<(u32, u32), Vec<(InputId, Surface)>>,
    minted: &std::collections::HashSet<u32>,
    u: u32,
    v: u32,
    w: u32,
) -> bool {
    curve_endpoint_ranked(
        curves,
        incidence,
        incidence_rank(incidence, minted, v),
        u,
        v,
        w,
    )
}

/// [`curve_endpoint`] with `v`'s [`incidence_rank`] already read.
pub(crate) fn curve_endpoint_ranked(
    curves: &std::collections::BTreeMap<(u32, u32), Curve>,
    incidence: &std::collections::BTreeMap<(u32, u32), Vec<(InputId, Surface)>>,
    rank: usize,
    u: u32,
    v: u32,
    w: u32,
) -> bool {
    if rank >= 3 {
        return true;
    }
    let key_a = if u < v { (u, v) } else { (v, u) };
    let key_b = if v < w { (v, w) } else { (w, v) };
    match (curves.get(&key_a), curves.get(&key_b)) {
        (Some(Curve::LineSegment), Some(Curve::LineSegment)) => {
            same_line_run(curves, incidence, u, v, w) != Some(true)
        }
        (Some(ca), Some(cb)) => ca != cb,
        (None, None) => match (
            both_input_pair(incidence, key_a),
            both_input_pair(incidence, key_b),
        ) {
            (Some(pa), Some(pb)) => !(pa == pb || (pa[0] == pb[1] && pa[1] == pb[0])),
            _ => true,
        },
        _ => true,
    }
}

/// The edge's Phase-A incidence deduped to EXACTLY two `(input, surface)`
/// entries with both inputs present — an intersection edge of one surface
/// pair (≥ 3 entries is a junction edge).
fn both_input_pair(
    incidence: &std::collections::BTreeMap<(u32, u32), Vec<(InputId, Surface)>>,
    k: (u32, u32),
) -> Option<[(InputId, Surface); 2]> {
    let entries = incidence.get(&k)?;
    let mut ded: Vec<(InputId, Surface)> = Vec::new();
    for &e in entries {
        if !ded.contains(&e) {
            ded.push(e);
        }
    }
    match ded[..] {
        [x, y] if x.0 != y.0 => Some([x, y]),
        _ => None,
    }
}

/// Spec `yang_453_line_overtake` (2026-09-11, R0059): the straight-run
/// OVERTAKE site. Yang §4.5.3's subject is a point sequence that reverses
/// AFTER convergence; on a straight run the generic shape is a relocated
/// vertex `J` (a line × surface junction moved ALONG its plane-pair line by
/// its own certified relocation — R0059: the extrude box's base edge exits
/// the coplanar revolve cap at a grazing rim crossing, and the Stage-0
/// overlay's chord crossing relocates 20.6 along the edge to the exact
/// circle) that overtakes UNMOVED run vertices lying between its old and new
/// positions (R0059: two Stage-0 subdivision vertices of that edge). Those
/// points are phantoms of the resolution artifact the relocation corrected:
/// the run walks forward-back-forward through them and the emitted face
/// loop self-overlaps (kernel-v2 `ring rejected by CDT`).
///
/// The site is `p_r` unmoved, flanked on ONE side by a moved vertex `J`
/// with an entry position, on a straight run (both edges `LineSegment`,
/// same unordered surface pair — `same_line_run`), with the paper's
/// degenerate collinear reversal at `p_r` (`|t̃| < TAU_WORK`, the polyline
/// doubles back exactly), and `p_r` INSIDE `J`'s displacement segment
/// `[J_old, J_new]` on that line (the OVERTAKE certificate: the parameter
/// along `J_new − J_old` strictly between 0 and |J_new − J_old|, off-line
/// distance ≤ `TAU_WORK·(1+scale)`). Returns `(victim = p_r, survivor = J)`:
/// the phantom collapses onto the exact junction, bounded by
/// `|J_new − J_old|` — a displacement Stage 4 already certified for `J`
/// (its junction gate), which is why the caller's 2·d_ε resolution gate is
/// not the bound for this arm. Tested on the run's own geometry only, so a
/// coplanar overlay seam (no `n_A × n_B` tangent — `is_reversed`'s branch
/// 5) is diagnosable here. `None` (the site falls through to `is_reversed`
/// unchanged) for: a non-straight or pair-changing site, no moved
/// neighbour, BOTH neighbours moved (ambiguous — fail closed), a moved
/// neighbour minted during Stage 4 (no entry position), a moved `p_r`, a
/// sub-band displacement, a non-collinear turn (a genuine corner), or `p_r`
/// outside `[J_old, J_new]` (not overtaken — an unrelated U-turn, which
/// stays loud downstream exactly as before, e.g. the annular hole-rim
/// crossing artifact pinned by `annular_cap_hole_crossing_stays_loud`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn line_overtake_site(
    mesh: &Mesh,
    curves: &std::collections::BTreeMap<(u32, u32), Curve>,
    incidence: &std::collections::BTreeMap<(u32, u32), Vec<(InputId, Surface)>>,
    moved: &std::collections::HashSet<u32>,
    entry_pos: &[[f64; 3]],
    p_b: u32,
    p_r: u32,
    p_n: u32,
) -> Option<(u32, u32)> {
    if same_line_run(curves, incidence, p_b, p_r, p_n) != Some(true) {
        return None;
    }
    if moved.contains(&p_r) {
        return None;
    }
    let j = match (moved.contains(&p_b), moved.contains(&p_n)) {
        (true, false) => p_b,
        (false, true) => p_n,
        _ => return None,
    };
    let old = *entry_pos.get(j as usize)?;
    let new = mesh.verts[j as usize].as_array();
    let pr = mesh.verts[p_r as usize].as_array();
    let scale = old
        .iter()
        .chain(new.iter())
        .chain(pr.iter())
        .fold(0.0f64, |m, &c| m.max(c.abs()));
    let band = cad_primitives::TAU_WORK * (1.0 + scale);
    let d = [new[0] - old[0], new[1] - old[1], new[2] - old[2]];
    let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    if len <= band {
        return None;
    }
    // The paper's degenerate collinear reversal at p_r: the polyline doubles
    // back exactly (unit(p_r − p_b) + unit(p_n − p_r) ≈ 0).
    let pb = mesh.verts[p_b as usize].as_array();
    let pn = mesh.verts[p_n as usize].as_array();
    let v1 = normalize3([pr[0] - pb[0], pr[1] - pb[1], pr[2] - pb[2]]);
    let v2 = normalize3([pn[0] - pr[0], pn[1] - pr[1], pn[2] - pr[2]]);
    let t = [v1[0] + v2[0], v1[1] + v2[1], v1[2] + v2[2]];
    if (t[0] * t[0] + t[1] * t[1] + t[2] * t[2]).sqrt() >= cad_primitives::TAU_WORK {
        return None;
    }
    // The OVERTAKE certificate: p_r strictly inside [J_old, J_new] on J's
    // displacement line.
    let u = [d[0] / len, d[1] / len, d[2] / len];
    let w = [pr[0] - old[0], pr[1] - old[1], pr[2] - old[2]];
    let s_par = w[0] * u[0] + w[1] * u[1] + w[2] * u[2];
    if s_par <= 0.0 || s_par >= len {
        return None;
    }
    let perp = [
        w[0] - s_par * u[0],
        w[1] - s_par * u[1],
        w[2] - s_par * u[2],
    ];
    if (perp[0] * perp[0] + perp[1] * perp[1] + perp[2] * perp[2]).sqrt() > band {
        return None;
    }
    Some((p_r, j))
}

/// Spec `yang_453_line_overtake` §6 (2026-10-12, P0036): the CARRIED-CURVE
/// OVERTAKE site — the straight-run arm's rule on an operand's own typed
/// B-Rep edge. P0036: a revolve's start cap (a disc, r 4e-4) is cut by a
/// cylinder (r 1.25e-3) whose Stage-1 12-gon recedes from its circle by a
/// 4.26e-5 sagitta; the cap rim's 13-gon vertex v7 sits 3.4e-5 INSIDE the
/// exact cutter and OUTSIDE its chord polygon, so Stage 2 keeps it, and the
/// arrangement's rim × cutter corner lands 1° beyond it. Stage 4 relocates
/// that corner in closed form onto the exact rim ∩ cutter point A, 5.2°
/// back along the rim — PAST v7. The cap loop then walks v5 → v7 → A
/// forward-back along one circle and kernel-v2 rejects the ring (its
/// grid sampler emits the same azimuth from both arcs, dedups the spike,
/// and the CDT sees the crossing). Yang §4.5.3 verbatim — "a reverse
/// sequence of points after convergence" — on a CARRIED curve, where the
/// straight arm's collinear test does not apply and the exact test is the
/// curve's own parameter order (the #145 shared-conic arm's criterion).
///
/// The site is `p_r` unmoved, flanked on ONE side by a moved vertex `J`
/// with an entry position; both incident edges UNTYPED (no Stage-3 curve —
/// not intersection edges) with the SAME two distinct surfaces of ONE input
/// (a solid edge of that operand); the operand's B-Rep edge shared by those
/// two faces and passing through `p_r` carries the run's curve. For a
/// periodic conic (`Circle` / `Ellipse`): the walk REVERSES at `p_r` in
/// parameter order (`conic_param_deltas`, `d1·d2 < 0`) AND `p_r` lies
/// strictly inside `J`'s displacement arc `[t(J_old), t(J_new)]` (the
/// OVERTAKE certificate, in the curve's parameter — the short way round,
/// never the complement). For a `LineSegment` carried edge the straight
/// arm's own geometry applies (`segment_overtake`). Returns `(victim = p_r,
/// survivor = J)`; the caller's 2·d_ε gate is bypassed exactly as for the
/// straight arm (I2: the collapse length is bounded by `J`'s certified
/// displacement). `None` leaves the site to the pair arm / `is_reversed`
/// byte-identically: a typed or mixed-input edge, a pair change at `p_r`,
/// no / both / self moved, no entry position, a sub-band displacement, no
/// shared B-Rep edge through `p_r` (or several with different curves), an
/// open or untyped curve, a monotone walk, or `p_r` outside the arc.
#[allow(clippy::too_many_arguments)]
pub(crate) fn carried_curve_overtake_site(
    mesh: &Mesh,
    a: &BRep,
    b: &BRep,
    curves: &std::collections::BTreeMap<(u32, u32), Curve>,
    incidence: &std::collections::BTreeMap<(u32, u32), Vec<(InputId, Surface)>>,
    moved: &std::collections::HashSet<u32>,
    entry_pos: &[[f64; 3]],
    p_b: u32,
    p_r: u32,
    p_n: u32,
) -> Option<(u32, u32)> {
    let key = |x: u32, y: u32| if x < y { (x, y) } else { (y, x) };
    let (key_b, key_n) = (key(p_b, p_r), key(p_r, p_n));
    if curves.contains_key(&key_b) || curves.contains_key(&key_n) {
        return None;
    }
    // A solid edge of ONE operand: the incidence dedups to exactly two
    // entries, both from the same input, on two distinct surfaces.
    let solid_edge = |k: (u32, u32)| -> Option<[(InputId, Surface); 2]> {
        let entries = incidence.get(&k)?;
        let mut ded: Vec<(InputId, Surface)> = Vec::new();
        for &e in entries {
            if !ded.contains(&e) {
                ded.push(e);
            }
        }
        match ded[..] {
            [x, y] if x.0 == y.0 && x.1 != y.1 => Some([x, y]),
            _ => None,
        }
    };
    let eb = solid_edge(key_b)?;
    let en = solid_edge(key_n)?;
    if !(eb == en || (eb[0] == en[1] && eb[1] == en[0])) {
        return None;
    }
    if moved.contains(&p_r) {
        return None;
    }
    let j = match (moved.contains(&p_b), moved.contains(&p_n)) {
        (true, false) => p_b,
        (false, true) => p_n,
        _ => return None,
    };
    let old = *entry_pos.get(j as usize)?;
    let new = mesh.verts[j as usize].as_array();
    let pr = mesh.verts[p_r as usize].as_array();
    let scale = old
        .iter()
        .chain(new.iter())
        .chain(pr.iter())
        .fold(0.0f64, |m, &c| m.max(c.abs()));
    let band = cad_primitives::TAU_WORK * (1.0 + scale);
    let d = [new[0] - old[0], new[1] - old[1], new[2] - old[2]];
    let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    if len <= band {
        return None;
    }
    let brep = match eb[0].0 {
        InputId::A => a,
        InputId::B => b,
    };
    let curve = carried_edge_curve(
        brep,
        eb[0].1,
        eb[1].1,
        Point3::new(pr[0], pr[1], pr[2]),
        band,
    )?;
    let pb = mesh.verts[p_b as usize].as_array();
    let pn = mesh.verts[p_n as usize].as_array();
    match curve {
        Curve::LineSegment => segment_overtake(old, new, pr, pb, pn, band).then_some((p_r, j)),
        c if conic_param_periodic(&c) => {
            // The walk reverses at p_r along the carried conic (the paper's
            // criterion in parameter order — never the angle band).
            let (d1, d2) = conic_param_deltas(
                &c,
                mesh.verts[p_b as usize],
                mesh.verts[p_r as usize],
                mesh.verts[p_n as usize],
            )?;
            let prod = d1 * d2;
            if !prod.is_finite() || prod >= 0.0 {
                return None;
            }
            // The OVERTAKE certificate: p_r strictly inside J's displacement
            // arc, measured the short way round from J_old.
            let t_old = conic_param(&c, Point3::new(old[0], old[1], old[2]))?;
            let t_new = conic_param(&c, Point3::new(new[0], new[1], new[2]))?;
            let t_r = conic_param(&c, Point3::new(pr[0], pr[1], pr[2]))?;
            let s = wrap_pi(t_new - t_old);
            let u = wrap_pi(t_r - t_old);
            if !(s.is_finite() && u.is_finite()) || s == 0.0 || u * s <= 0.0 || u.abs() >= s.abs() {
                return None;
            }
            Some((p_r, j))
        }
        _ => None,
    }
}

/// The straight-run OVERTAKE geometry shared by the carried `LineSegment`
/// arm: the polyline doubles back exactly at `p_r` (`|unit(p_r−p_b) +
/// unit(p_n−p_r)| < TAU_WORK`) and `p_r` lies strictly inside `[old, new]`
/// along `unit(new − old)`, within `band` of that line.
pub(crate) fn segment_overtake(
    old: [f64; 3],
    new: [f64; 3],
    pr: [f64; 3],
    pb: [f64; 3],
    pn: [f64; 3],
    band: f64,
) -> bool {
    let d = [new[0] - old[0], new[1] - old[1], new[2] - old[2]];
    let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    if !len.is_finite() || len <= band {
        return false;
    }
    let v1 = normalize3([pr[0] - pb[0], pr[1] - pb[1], pr[2] - pb[2]]);
    let v2 = normalize3([pn[0] - pr[0], pn[1] - pr[1], pn[2] - pr[2]]);
    let t = [v1[0] + v2[0], v1[1] + v2[1], v1[2] + v2[2]];
    if (t[0] * t[0] + t[1] * t[1] + t[2] * t[2]).sqrt() >= cad_primitives::TAU_WORK {
        return false;
    }
    let u = [d[0] / len, d[1] / len, d[2] / len];
    let w = [pr[0] - old[0], pr[1] - old[1], pr[2] - old[2]];
    let s_par = w[0] * u[0] + w[1] * u[1] + w[2] * u[2];
    if s_par <= 0.0 || s_par >= len {
        return false;
    }
    let perp = [
        w[0] - s_par * u[0],
        w[1] - s_par * u[1],
        w[2] - s_par * u[2],
    ];
    (perp[0] * perp[0] + perp[1] * perp[1] + perp[2] * perp[2]).sqrt() <= band
}

/// Wrap an angle difference into `(−π, π]`.
fn wrap_pi(mut d: f64) -> f64 {
    let two_pi = 2.0 * std::f64::consts::PI;
    while d > std::f64::consts::PI {
        d -= two_pi;
    }
    while d <= -std::f64::consts::PI {
        d += two_pi;
    }
    d
}

/// The typed curve of the operand's B-Rep edge shared by the faces carrying
/// surfaces `s0` and `s1` (bit-exact surface identity — the Phase-A
/// `inherited` surface IS the operand face's) that passes through `p`
/// (within `band`). A rim split into several edges of one circle is one
/// curve; shared edges through `p` with DIFFERENT curves are ambiguous and
/// decline (`None`), as does a shared edge whose curve has no point-distance
/// here (a `SurfacePair` / open conic).
pub(crate) fn carried_edge_curve(
    brep: &BRep,
    s0: Surface,
    s1: Surface,
    p: Point3,
    band: f64,
) -> Option<Curve> {
    use std::collections::BTreeSet;
    let mut on0: BTreeSet<u32> = BTreeSet::new();
    let mut on1: BTreeSet<u32> = BTreeSet::new();
    for f in brep.faces() {
        let edges = f.outer_loop.iter().chain(f.inner_loops.iter().flatten());
        if f.surface == s0 {
            on0.extend(edges.clone().copied());
        }
        if f.surface == s1 {
            on1.extend(edges.copied());
        }
    }
    let mut found: Option<Curve> = None;
    for &ei in on0.intersection(&on1) {
        let e = &brep.edges()[ei as usize];
        let dist = match e.curve {
            Curve::LineSegment => {
                let a = brep.vertices()[e.start as usize].point.as_array();
                let b = brep.vertices()[e.end as usize].point.as_array();
                let q = p.as_array();
                let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
                let l2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
                if !l2.is_finite() || l2 <= 0.0 {
                    continue;
                }
                let w = [q[0] - a[0], q[1] - a[1], q[2] - a[2]];
                let t = ((w[0] * d[0] + w[1] * d[1] + w[2] * d[2]) / l2).clamp(0.0, 1.0);
                let r = [w[0] - t * d[0], w[1] - t * d[1], w[2] - t * d[2]];
                (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt()
            }
            Curve::Circle {
                center,
                normal,
                radius,
            } => {
                let Ok((proj, _)) = project_onto_circle(p, center, normal, radius) else {
                    continue;
                };
                let (q, r) = (p.as_array(), proj.as_array());
                ((q[0] - r[0]).powi(2) + (q[1] - r[1]).powi(2) + (q[2] - r[2]).powi(2)).sqrt()
            }
            Curve::Ellipse {
                center,
                normal,
                major_axis,
                major_radius,
                minor_radius,
            } => {
                let t = ellipse_param(p, center, normal, major_axis, major_radius, minor_radius);
                let proj = ellipse_point(center, normal, major_axis, major_radius, minor_radius, t);
                let (q, r) = (p.as_array(), proj.as_array());
                ((q[0] - r[0]).powi(2) + (q[1] - r[1]).powi(2) + (q[2] - r[2]).powi(2)).sqrt()
            }
            _ => continue,
        };
        if dist > band {
            continue;
        }
        match found {
            None => found = Some(e.curve),
            Some(c) if c == e.curve => {}
            Some(_) => return None,
        }
    }
    found
}

fn both_line_edges(
    curves: &std::collections::BTreeMap<(u32, u32), Curve>,
    key_b: (u32, u32),
    key_n: (u32, u32),
) -> bool {
    matches!(curves.get(&key_n), Some(Curve::LineSegment))
        && matches!(curves.get(&key_b), Some(Curve::LineSegment))
}

/// Spec `yang_453_pair_chain_reversal` §3: the pair-site TEST at `p_r`.
///
/// Site eligibility: each incident edge's Phase-A incidence dedups to
/// EXACTLY 2 distinct `(input, surface)` entries with both inputs present
/// (an intersection edge — ≥3 entries is a junction edge, no verdict), and
/// the two edges carry the SAME pair. Returns the unit tangent `t = n₀ × n₁`
/// at `p_r` (|cross| < 1e-6 = Yang §5's angular tolerance ⇒ near-tangential,
/// no verdict, `None`); the reversal test (Yang Fig. 15 in its general-surface
/// form, as a progression-sign test — never the angle band, whose
/// coarse-chord false positives are P10-disproven for conics) is
/// `((p_r−p_b)·t) · ((x−p_r)·t) < 0`, [`SiteTest::Progression`].
fn pair_site_tangent(
    mesh: &Mesh,
    incidence: &std::collections::BTreeMap<(u32, u32), Vec<(InputId, Surface)>>,
    p_b: u32,
    p_r: u32,
    p_n: u32,
) -> Option<[f64; 3]> {
    const PAIR_RANK_FLOOR: f64 = 1e-6; // sin(angle) floor — Yang §5 angular tol.
    let key = |a: u32, b: u32| if a < b { (a, b) } else { (b, a) };
    let bp = both_input_pair(incidence, key(p_b, p_r))?;
    let np = both_input_pair(incidence, key(p_r, p_n))?;
    if !(bp == np || (bp[0] == np[1] && bp[1] == np[0])) {
        return None;
    }
    let pr = mesh.verts[p_r as usize].as_array();
    let (_, n0) = surface_value_and_normal(bp[0].1, pr)?;
    let (_, n1) = surface_value_and_normal(bp[1].1, pr)?;
    let t = [
        n0[1] * n1[2] - n0[2] * n1[1],
        n0[2] * n1[0] - n0[0] * n1[2],
        n0[0] * n1[1] - n0[1] * n1[0],
    ];
    let tl = (t[0] * t[0] + t[1] * t[1] + t[2] * t[2]).sqrt();
    if tl < PAIR_RANK_FLOOR {
        return None;
    }
    Some([t[0] / tl, t[1] / tl, t[2] / tl])
}

/// Task #146 (spec `yang_stage4_circle_pp_line_junction` branches 1–3): the
/// SINGLE distinct pp-line among a vertex's `vert_pp_planes` entries —
/// entries compare as UNORDERED plane pairs (exact bit equality, both plane
/// orders). `Some` iff exactly one distinct line remains after dedup.
pub(crate) fn dedup_single_pp_line(
    entries: &[(Vector3, f64, Vector3, f64)],
) -> Option<(Vector3, f64, Vector3, f64)> {
    let plane_eq =
        |a: &(Vector3, f64), b: &(Vector3, f64)| a.0.as_array() == b.0.as_array() && a.1 == b.1;
    let entry_eq = |x: &(Vector3, f64, Vector3, f64), y: &(Vector3, f64, Vector3, f64)| {
        let (x1, x2) = ((x.0, x.1), (x.2, x.3));
        let (y1, y2) = ((y.0, y.1), (y.2, y.3));
        (plane_eq(&x1, &y1) && plane_eq(&x2, &y2)) || (plane_eq(&x1, &y2) && plane_eq(&x2, &y1))
    };
    let mut distinct: Vec<(Vector3, f64, Vector3, f64)> = Vec::new();
    for e in entries {
        if !distinct.iter().any(|d| entry_eq(d, e)) {
            distinct.push(*e);
        }
    }
    if distinct.len() == 1 {
        Some(distinct[0])
    } else {
        None
    }
}

/// Task #145: are two conic curve payloads the SAME geometric point set,
/// differing only by the SIGN of the stored plane `normal`? A conic's normal
/// sign is a frame choice, not geometry: negating `normal` flips the derived
/// minor direction (`normal × major_axis`) with it, tracing the identical
/// ellipse/circle point set (the parameterization runs the other way). Exact
/// field comparison — f64 negation is exact, so no tolerance is involved.
pub(crate) fn conics_equal_up_to_normal_sign(a: &Curve, b: &Curve) -> bool {
    let neg = |v: Vector3| Vector3::new(-v.as_array()[0], -v.as_array()[1], -v.as_array()[2]);
    match (a, b) {
        (
            Curve::Circle {
                center: c0,
                normal: n0,
                radius: r0,
            },
            Curve::Circle {
                center: c1,
                normal: n1,
                radius: r1,
            },
        ) => c0 == c1 && r0 == r1 && (*n0 == *n1 || *n0 == neg(*n1)),
        (
            Curve::Ellipse {
                center: c0,
                normal: n0,
                major_axis: m0,
                major_radius: a0,
                minor_radius: b0,
            },
            Curve::Ellipse {
                center: c1,
                normal: n1,
                major_axis: m1,
                major_radius: a1,
                minor_radius: b1,
            },
        ) => c0 == c1 && m0 == m1 && a0 == a1 && b0 == b1 && (*n0 == *n1 || *n0 == neg(*n1)),
        // I13: same frame-choice fact for the parabola and hyperbola — negating
        // the stored normal flips the derived conjugate direction (`normal ×
        // axis_dir` / `normal × major`), tracing the identical point set with
        // the parameter running the other way. `axis_dir` / `major_axis` is
        // untouched, so the branch choice is preserved.
        (
            Curve::Parabola {
                vertex: v0,
                normal: n0,
                axis_dir: a0,
                focal_length: f0,
            },
            Curve::Parabola {
                vertex: v1,
                normal: n1,
                axis_dir: a1,
                focal_length: f1,
            },
        ) => v0 == v1 && a0 == a1 && f0 == f1 && (*n0 == *n1 || *n0 == neg(*n1)),
        (
            Curve::Hyperbola {
                center: c0,
                normal: n0,
                major_axis: m0,
                semi_transverse: a0,
                semi_conjugate: b0,
            },
            Curve::Hyperbola {
                center: c1,
                normal: n1,
                major_axis: m1,
                semi_transverse: a1,
                semi_conjugate: b1,
            },
        ) => c0 == c1 && m0 == m1 && a0 == a1 && b0 == b1 && (*n0 == *n1 || *n0 == neg(*n1)),
        _ => false,
    }
}

/// Task #145 (spec `yang_453_mixed_cycle_conic_backtrack`): the exact conic
/// parameter of `pt` on a `Circle`/`Ellipse`/`Parabola`/`Hyperbola`, in the
/// shared frame (`project_onto_circle` / `ellipse_param` / `parabola_param` /
/// `hyperbola_param` — the same parameterization Stage-4 relocation and
/// `is_reversed` use). `None` for non-conic payloads or a degenerate
/// projection (branch 11 — cannot diagnose).
///
/// Also the curve-AUTHORITY key for the §4.4.1 splice
/// ([`crate::stage4_splice::order_along_curve`]): §4.5.3's "points progressing
/// along the intersection curve in sequence" and §4.4.1's "no flipping
/// triangles since the intersection curves are regular" are the same
/// monotonicity, read through this parameter.
pub(crate) fn conic_param(curve: &Curve, pt: Point3) -> Option<f64> {
    match curve {
        Curve::Circle {
            center,
            normal,
            radius,
        } => project_onto_circle(pt, *center, *normal, *radius)
            .ok()
            .map(|(_, t)| t),
        Curve::Ellipse {
            center,
            normal,
            major_axis,
            major_radius,
            minor_radius,
        } => Some(ellipse_param(
            pt,
            *center,
            *normal,
            *major_axis,
            *major_radius,
            *minor_radius,
        )),
        // Open conics (Parabola, Hyperbola): strictly monotone along the
        // curve / branch and UNBOUNDED. Consumers whose math lives in the
        // angle domain (wraps, 2π sweeps, circular-gap cuts) gate on
        // [`conic_param_periodic`] and handle open params without wrapping.
        Curve::Parabola {
            vertex,
            normal,
            axis_dir,
            ..
        } => Some(crate::geom::parabola_param(pt, *vertex, *normal, *axis_dir)),
        Curve::Hyperbola {
            center,
            normal,
            major_axis,
            semi_conjugate,
            ..
        } => Some(crate::geom::hyperbola_param(
            pt,
            *center,
            *normal,
            *major_axis,
            *semi_conjugate,
        )),
        _ => None,
    }
}

/// Is [`conic_param`]'s parameterization of `curve` an ANGLE (2π-periodic)?
/// `true` for Circle/Ellipse — wraps, circular-gap cuts and 2π sweep budgets
/// are meaningful. `false` for the open conics (Parabola, Hyperbola), whose
/// parameter is unbounded: angle-domain consumers must DECLINE those (or take
/// unwrapped differences), never wrap them to `(−π, π]`.
pub(crate) fn conic_param_periodic(curve: &Curve) -> bool {
    matches!(curve, Curve::Circle { .. } | Curve::Ellipse { .. })
}

/// Task #145 branch 9/10: do `p_b`, `p_r`, `p_n` FAIL to progress along the
/// shared conic in parameter order? Yang §4.5.3's criterion is "points
/// progressing along the intersection curve in sequence"; for closed-form
/// conics the curve parameter tests this directly (the paper's discrete
/// tangent test is the general-surface proxy, whose 45° band is P10-disproven
/// at coarse-chord conic sites — spec §3c records). Consecutive deltas are
/// wrapped to (−π, π] for periodic conics (each Stage-1 chord subtends < π)
/// and taken directly for open conics; reversal ⟺ the two deltas have opposite
/// signs. A zero delta (coincident parameters) and a degenerate projection are
/// healthy — cannot diagnose (branch 11).
/// (The sweep consumes `conic_param_deltas` directly — it also needs the
/// survivor side; this equivalent boolean form is the branch-table oracle.)
#[cfg(test)]
pub(crate) fn conic_param_reversed(curve: &Curve, p_b: Point3, p_r: Point3, p_n: Point3) -> bool {
    conic_param_deltas(curve, p_b, p_r, p_n).is_some_and(|(d1, d2)| d1 * d2 < 0.0)
}

/// Task #145: the conic-parameter deltas `(t_r − t_b, t_n − t_r)` of a
/// shared-conic site (wrapped to `(−π, π]` for periodic conics, unwrapped for
/// open conics). `None` when any parameter is undefined (branch 11). The
/// reversal test is `d1·d2 < 0`; the collapse survivor is the parameter-NEARER
/// bracketing neighbor (the endpoint `p_r` overshot — its distance is the
/// actual overshoot, which the 2·d_ε resolution gate can honestly bound; the
/// FAR neighbor is a whole arc away and never a resolution artifact).
pub(crate) fn conic_param_deltas(
    curve: &Curve,
    p_b: Point3,
    p_r: Point3,
    p_n: Point3,
) -> Option<(f64, f64)> {
    // inc-2c-3b-10: a PROCEDURAL surface-pair site is tested against the
    // curve's OWN tangent at `p_r` — T = n_a × n_b, the exact intersection
    // direction from the two surface gradients (the paper's "progress along
    // the intersection curve", evaluated analytically at the site; a
    // coarse-but-monotone chain has its neighbours on OPPOSITE sides of T
    // and stays healthy whatever its turn angle, so the P10-disproven
    // angle-band false-positive class cannot arise). Deltas are the signed
    // tangent projections — lengths, not angles; no wrap. A degenerate or
    // unreadable tangent is branch 11 (`None`, cannot diagnose).
    if let Curve::SurfacePair { a, b } = curve {
        let pr = p_r.as_array();
        let (_, na) = crate::stage4_relocate::surface_value_and_normal(*a, pr)?;
        let (_, nb) = crate::stage4_relocate::surface_value_and_normal(*b, pr)?;
        let t = [
            na[1] * nb[2] - na[2] * nb[1],
            na[2] * nb[0] - na[0] * nb[2],
            na[0] * nb[1] - na[1] * nb[0],
        ];
        let t2 = t[0] * t[0] + t[1] * t[1] + t[2] * t[2];
        if !(t2.is_finite() && t2 > 0.0) {
            return None;
        }
        let (pb, pn) = (p_b.as_array(), p_n.as_array());
        let d1 = (pr[0] - pb[0]) * t[0] + (pr[1] - pb[1]) * t[1] + (pr[2] - pb[2]) * t[2];
        let d2 = (pn[0] - pr[0]) * t[0] + (pn[1] - pr[1]) * t[1] + (pn[2] - pr[2]) * t[2];
        return Some((d1, d2));
    }
    let (t_b, t_r, t_n) = (
        conic_param(curve, p_b)?,
        conic_param(curve, p_r)?,
        conic_param(curve, p_n)?,
    );
    if !conic_param_periodic(curve) {
        return Some((t_r - t_b, t_n - t_r));
    }
    let two_pi = 2.0 * std::f64::consts::PI;
    let wrap = |mut d: f64| -> f64 {
        while d > std::f64::consts::PI {
            d -= two_pi;
        }
        while d <= -std::f64::consts::PI {
            d += two_pi;
        }
        d
    };
    Some((wrap(t_r - t_b), wrap(t_n - t_r)))
}

/// Task #145 branch 12 (site eligibility): the shared conic of a mixed-cycle
/// site — `Some` iff BOTH incident edges carry the SAME conic (`Circle`,
/// `Ellipse`, `Parabola`, or `Hyperbola`, exact struct equality or equality up
/// to the stored normal's sign, I5). Junctions (different conics),
/// conic/LineSegment boundaries, and straight runs (the §3c both-line arm)
/// return `None`.
pub(crate) fn mixed_cycle_shared_conic(
    curves: &std::collections::BTreeMap<(u32, u32), Curve>,
    key_b: (u32, u32),
    key_n: (u32, u32),
) -> Option<Curve> {
    let cb = curves.get(&key_b)?;
    let cn = curves.get(&key_n)?;
    // inc-2c-3b-10 (spec `yang_451_corner_transit` §3r follow-up): the
    // PROCEDURAL surface-pair curve joins the shared-conic vocabulary —
    // identity is exact surface equality, unordered (the pair's storage
    // order is a frame choice). Its parameter test lives in
    // `conic_param_deltas`' tangent arm.
    if let (Curve::SurfacePair { a: na, b: nb }, Curve::SurfacePair { a: ba, b: bb }) = (cn, cb) {
        return ((na == ba && nb == bb) || (na == bb && nb == ba)).then_some(*cb);
    }
    if !matches!(
        cn,
        Curve::Circle { .. }
            | Curve::Ellipse { .. }
            | Curve::Parabola { .. }
            | Curve::Hyperbola { .. }
    ) {
        return None;
    }
    if cn == cb || conics_equal_up_to_normal_sign(cn, cb) {
        // Either storage is a valid frame for the parameter test (a normal
        // flip negates BOTH deltas, leaving their product invariant); return
        // the `key_b` representative deterministically.
        Some(*cb)
    } else {
        None
    }
}

/// Spec §3c: the UNORDERED incidence surface-pair equality that stands in for
/// curve identity on `Curve::LineSegment` intersection edges (the payload-less
/// variant cannot distinguish two different straight seams).
pub(crate) fn surface_pairs_equal(a: &[(InputId, Surface)], b: &[(InputId, Surface)]) -> bool {
    match (a, b) {
        ([a0, a1], [b0, b1]) => (a0 == b0 && a1 == b1) || (a0 == b1 && a1 == b0),
        _ => false,
    }
}

/// Spec §3c: are loop edges `(x,y)` and `(y,z)` on the SAME straight
/// intersection run? True only when BOTH carry `Curve::LineSegment` and their
/// unordered incidence surface pairs match. Conic edges are handled by curve
/// identity instead (byte-identical to the PR-KV11 guard).
pub(crate) fn same_line_run(
    curves: &std::collections::BTreeMap<(u32, u32), Curve>,
    incidence: &std::collections::BTreeMap<(u32, u32), Vec<(InputId, Surface)>>,
    x: u32,
    y: u32,
    z: u32,
) -> Option<bool> {
    let key_a = if x < y { (x, y) } else { (y, x) };
    let key_b = if y < z { (y, z) } else { (z, y) };
    match (curves.get(&key_a), curves.get(&key_b)) {
        (Some(Curve::LineSegment), Some(Curve::LineSegment)) => {
            match (incidence.get(&key_a), incidence.get(&key_b)) {
                (Some(a), Some(b)) => Some(surface_pairs_equal(a, b)),
                // Missing incidence — cannot establish run identity.
                _ => Some(false),
            }
        }
        // Not a double-LineSegment adjacency — line-run identity not applicable.
        _ => None,
    }
}

/// The DISTINCT analytic surfaces the incidence edges at each vertex carry —
/// the incidence rank of a vertex is its set's size (spec
/// `yang_453_junction_protected_collapse` §3d). Two is a point on one
/// intersection curve; three or more is a corner junction, exact by Stage 4's
/// closed forms. One pass over the incidence map (the sweep reads it once per
/// pass; per-vertex scans were quadratic on R0044).
pub(crate) fn surfaces_by_vertex(
    incidence: &std::collections::BTreeMap<(u32, u32), Vec<(InputId, Surface)>>,
) -> std::collections::HashMap<u32, Vec<Surface>> {
    let mut out: std::collections::HashMap<u32, Vec<Surface>> = std::collections::HashMap::new();
    for (&(s, e), entries) in incidence {
        for v in [s, e] {
            let set = out.entry(v).or_default();
            for &(_, surf) in entries {
                if !set.contains(&surf) {
                    set.push(surf);
                }
            }
        }
    }
    out
}

/// The size of `v`'s [`surfaces_by_vertex`] set.
#[cfg(test)]
pub(crate) fn distinct_surfaces_at(
    incidence: &std::collections::BTreeMap<(u32, u32), Vec<(InputId, Surface)>>,
    v: u32,
) -> usize {
    surfaces_by_vertex(incidence).get(&v).map_or(0, Vec::len)
}

/// The §3d incidence rank of `v`, read with Stage-1 mint PROVENANCE (spec
/// `yang_453_junction_protected_collapse` §3e, P0057): the distinct surfaces
/// the loop edges at `v` carry, or three when `v` is a Stage-1 junction mint
/// — an operand edge's exact crossing of the partner's surface, a corner by
/// construction. The edge-carried count can read a mint as a plain curve
/// point when the labeling kept no triangle of one of its three faces at it
/// (P0057: the prism's f5 piece ended at the chord-level pierce, so only f6
/// and the cylinder touched the exact corner); the mint record is the
/// authority the surviving triangles cannot withdraw.
#[cfg(test)]
pub(crate) fn incidence_rank(
    incidence: &std::collections::BTreeMap<(u32, u32), Vec<(InputId, Surface)>>,
    minted: &std::collections::HashSet<u32>,
    v: u32,
) -> usize {
    let by_edges = distinct_surfaces_at(incidence, v);
    if minted.contains(&v) {
        by_edges.max(3)
    } else {
        by_edges
    }
}

/// §4.4.1(b) merge direction (spec `yang_453_junction_protected_collapse`
/// §3b): which vertex of a sub-feature-floor edge `(u, v)` is REMOVED?
/// Returns `(victim, survivor)` for [`collapse_vertex`].
///
/// Yang Fig. 11(b) merges the split-edge endpoint INTO the existing exact
/// intersection point ("if an endpoint p of the split edge is too close to q,
/// we merge p with q") — the exact vertex survives. Rank: closed-form
/// junction (exact on TWO curves) > single-curve conic endpoint > plain mesh
/// vertex; equal ranks keep the lower-index-survives rule byte-identical to
/// the pre-fix behavior.
///
/// WIRED at the §4.4.1(b) merge call site 2026-07-21 (task #186). The §3b
/// bank condition is satisfied: R0091's output χ = −4 was VERIFIED (Cherchi
/// sidecar reference parity on the exact operand meshes + independent
/// voxel-CSG derivation from the authored numbers — the meta's naive 3-op
/// default χ=2 was the authoring error, corrected in `R0091.meta.json`).
pub(crate) fn sub_feature_merge_direction(
    junction_verts: &std::collections::BTreeSet<u32>,
    conic_endpoint: &std::collections::BTreeSet<u32>,
    u: u32,
    v: u32,
) -> (u32, u32) {
    let rank = |x: u32| -> u8 {
        if junction_verts.contains(&x) {
            2
        } else if conic_endpoint.contains(&x) {
            1
        } else {
            0
        }
    };
    match rank(u).cmp(&rank(v)) {
        std::cmp::Ordering::Greater => (v, u),
        std::cmp::Ordering::Less => (u, v),
        std::cmp::Ordering::Equal => (u.max(v), u.min(v)),
    }
}

/// PR-YR10 (§4.5.3): is `p_r` a reversed intersection point? Compares the
/// discrete polyline tangent `t̃ = unit(p_r − p_b) + unit(p_n − p_r)` against the
/// exact curve tangent at `p_r` ([`angle_site_tangent`] decides the site and
/// the tangent, [`angle_band_reversed`] the verdict).
#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn is_reversed(
    mesh: &Mesh,
    curves: &std::collections::BTreeMap<(u32, u32), Curve>,
    incidence: &std::collections::BTreeMap<(u32, u32), Vec<(InputId, Surface)>>,
    p_b: u32,
    p_r: u32,
    p_n: u32,
    lo: f64,
    hi: f64,
) -> bool {
    angle_site_tangent(mesh, curves, incidence, p_b, p_r, p_n).is_some_and(|tan| {
        angle_band_reversed(
            mesh.verts[p_b as usize],
            mesh.verts[p_r as usize],
            mesh.verts[p_n as usize],
            tan,
            lo,
            hi,
        )
    })
}

/// Yang Fig. 15's verdict on `p_b → p_r → p_n`: a collinear `t̃` (`|t̃| <
/// TAU_WORK`, the polyline doubles back) IS a reversal (Yang §4.5.3,
/// `:743-745`: "we directly detect the reversal, avoiding the angle
/// comparisons"); otherwise reversal ⟺ the unsigned angle between `t̃` and
/// the exact tangent ∈ (45°, 135°) (the 1e-6 rad slack baked into `lo`/`hi`).
/// No exact tangent (`None`) ⇒ only the U-turn is diagnosable.
pub(crate) fn angle_band_reversed(
    pb: Point3,
    pr: Point3,
    pn: Point3,
    tan: Option<[f64; 3]>,
    lo: f64,
    hi: f64,
) -> bool {
    let (pb, pr, pn) = (pb.as_array(), pr.as_array(), pn.as_array());
    let v1 = normalize3([pr[0] - pb[0], pr[1] - pb[1], pr[2] - pb[2]]);
    let v2 = normalize3([pn[0] - pr[0], pn[1] - pr[1], pn[2] - pr[2]]);
    let t_tilde = [v1[0] + v2[0], v1[1] + v2[1], v1[2] + v2[2]];
    let t_tilde_len =
        (t_tilde[0] * t_tilde[0] + t_tilde[1] * t_tilde[1] + t_tilde[2] * t_tilde[2]).sqrt();
    if t_tilde_len < cad_primitives::TAU_WORK {
        // Degenerate/collinear t̃ (|t̃| ≈ 0 ⟺ v1 ≈ −v2 ⟺ the polyline doubles
        // back at p_r): a U-turn IS a reversal. (Prior code returned
        // `false`/"healthy" — the N3 logic inversion; see docs/yang_deviations.md.)
        return true;
    }
    let Some(tan_c) = tan else {
        // No exact tangent available — cannot diagnose; treat as healthy
        // (the validation pass still guards inverted/degenerate triangles).
        return false;
    };
    let t_tilde_u = normalize3(t_tilde);
    let dotv = (t_tilde_u[0] * tan_c[0] + t_tilde_u[1] * tan_c[1] + t_tilde_u[2] * tan_c[2])
        .clamp(-1.0, 1.0);
    // Unsigned angle between t̃ and the exact tangent (sign of the tangent is
    // arbitrary, so fold to [0, π/2] via |dot|).
    let angle = dotv.abs().acos();
    angle > lo && angle < hi
}

/// The angle-band SITE at `p_r`: `None` when `p_r` is not a §4.5.3 site
/// (its two loop edges are not one intersection curve), else the exact unit
/// tangent of that curve at `p_r` (`Some(None)` when it has none here).
pub(crate) fn angle_site_tangent(
    mesh: &Mesh,
    curves: &std::collections::BTreeMap<(u32, u32), Curve>,
    incidence: &std::collections::BTreeMap<(u32, u32), Vec<(InputId, Surface)>>,
    p_b: u32,
    p_r: u32,
    p_n: u32,
) -> Option<Option<[f64; 3]>> {
    // PR-KV11: the §4.5.3 test is defined for points progressing along ONE
    // intersection curve C ("p_r is a point on the intersection curve C
    // between the two surfaces S_A and S_B", refs/text/yang2025_hybrid_
    // boolean.txt:709-745). A vertex where the loop TRANSITIONS between two
    // different conics (the ellipse×ellipse box-edge junction) is a genuine
    // corner — the discrete tangent legitimately kinks there and the angle
    // test against either single curve's tangent false-positives, collapsing
    // the junction loop vertex by vertex (the kv11 vanishing-bulge failure).
    {
        let key_n = if p_r < p_n { (p_r, p_n) } else { (p_n, p_r) };
        let key_b = if p_b < p_r { (p_b, p_r) } else { (p_r, p_b) };
        match (curves.get(&key_n), curves.get(&key_b)) {
            (Some(cn), Some(cb)) => {
                if cn != cb {
                    // Task #145 diagnosis probe (read-only, env-gated): does the
                    // junction guard skip a site whose two curves are the SAME
                    // geometric conic up to normal sign?
                    if std::env::var_os("YANG_T145_SWEEP_PROBE").is_some()
                        && conics_equal_up_to_normal_sign(cn, cb)
                    {
                        eprintln!(
                            "[t145-sweep] sign-flip junction skip: p_b={p_b} p_r={p_r} p_n={p_n} \
                             cn={cn:?} cb={cb:?}"
                        );
                    }
                    return None;
                }
            }
            // Spec §3c: PER-SITE eligibility — p_r is a §4.5.3 site only when
            // BOTH incident edges are intersection edges. A run boundary
            // (intersection meets solid edge) is a genuine topology corner.
            _ => return None,
        }
    }
    // Spec §3c branch 4: two straight seam edges compare curve-equal
    // (`LineSegment` carries no payload), so run identity uses the unordered
    // incidence surface pair — a pair change at p_r is a genuine corner
    // (including near-180° thin-wedge corners the U-turn test would
    // otherwise misread as reversals).
    let p_r_pt = mesh.verts[p_r as usize];
    let key = if p_r < p_n { (p_r, p_n) } else { (p_n, p_r) };
    match same_line_run(curves, incidence, p_b, p_r, p_n) {
        Some(false) => return None,
        Some(true) => {
            // Spec §3c branches 5 + straight-run arm: the §4.5.3 test needs
            // the exact tangent t_pr = n_A × n_B of the run's surface pair
            // (Yang Fig. 15, `:736-742`). A COINCIDENT/parallel pair (the
            // §4.5.5 overlay seams — both incident faces on the same two
            // planes) has no cross-product tangent, so NO reversal can be
            // diagnosed there at all — the overlay boundary legitimately
            // turns corners (including 180° crossing artifacts that must stay
            // loud downstream; pinned by `annular_cap_hole_crossing_stays_loud`).
            let entries = incidence.get(&key)?;
            let [(_, s0), (_, s1)] = entries[..] else {
                return None;
            };
            let n0 = surface_normal_at(s0, p_r_pt)?;
            let n1 = surface_normal_at(s1, p_r_pt)?;
            let cr = [
                n0[1] * n1[2] - n0[2] * n1[1],
                n0[2] * n1[0] - n0[0] * n1[2],
                n0[0] * n1[1] - n0[1] * n1[0],
            ];
            let m = (cr[0] * cr[0] + cr[1] * cr[1] + cr[2] * cr[2]).sqrt();
            if m < cad_primitives::TAU_WORK {
                return None;
            }
            return Some(Some([cr[0] / m, cr[1] / m, cr[2] / m]));
        }
        None => {}
    }
    // Exact conic tangent at p_r (Yang §4.5.3, Fig. 15; ledger N89 item 3).
    // Prefer the current edge `(p_r, p_n)`; fall back to `(p_b, p_r)` (the two
    // are equal at any surviving single-curve site after the junction guard
    // above). Admits all planar conics (`Circle`, `Ellipse`, `Parabola`,
    // `Hyperbola`) without filtering by `Curve` variant.
    let key2 = if p_b < p_r { (p_b, p_r) } else { (p_r, p_b) };
    let Some(&conic) = curves.get(&key).or_else(|| curves.get(&key2)) else {
        return Some(None);
    };
    let tan_c = match conic {
        Curve::Parabola {
            vertex,
            normal,
            axis_dir,
            focal_length,
        } => {
            // PR-YR22 / N89 item 3: parabola tangent `d/dt point(t) =
            // (t/(2f))·axis_dir + (normal × axis_dir)`, evaluated at the
            // conjugate-axis coordinate `t = (p_r − vertex)·(normal × axis_dir)`
            // matching `geom::parabola_point` / `geom::parabola_param`.
            if !(focal_length.is_finite() && focal_length > 0.0) {
                return Some(None);
            }
            let n = normalize3(normal.as_array());
            let ax = normalize3(axis_dir.as_array());
            let conj = [
                n[1] * ax[2] - n[2] * ax[1],
                n[2] * ax[0] - n[0] * ax[2],
                n[0] * ax[1] - n[1] * ax[0],
            ];
            let vtx = vertex.as_array();
            let pr = p_r_pt.as_array();
            let t = (pr[0] - vtx[0]) * conj[0]
                + (pr[1] - vtx[1]) * conj[1]
                + (pr[2] - vtx[2]) * conj[2];
            normalize3([
                (t / (2.0 * focal_length)) * ax[0] + conj[0],
                (t / (2.0 * focal_length)) * ax[1] + conj[1],
                (t / (2.0 * focal_length)) * ax[2] + conj[2],
            ])
        }
        Curve::Circle {
            center,
            normal,
            radius,
        } => {
            // Circle tangent: derivative of `center + r(cos t·e1 + sin t·e2)`
            // ⇒ `-sin t·e1 + cos t·e2`.
            let Ok((_proj, t)) = project_onto_circle(p_r_pt, center, normal, radius) else {
                return Some(None);
            };
            let (e1, e2) = ortho_basis(normal);
            let e1a = e1.as_array();
            let e2a = e2.as_array();
            let (st, ct) = (t.sin(), t.cos());
            normalize3([
                -st * e1a[0] + ct * e2a[0],
                -st * e1a[1] + ct * e2a[1],
                -st * e1a[2] + ct * e2a[2],
            ])
        }
        Curve::Ellipse {
            center,
            normal,
            major_axis,
            major_radius,
            minor_radius,
        } => {
            // PR-YR11: ellipse tangent `−a·sin t·major + b·cos t·minor_dir` at the
            // p_r parameter, in the shared ellipse frame (spec §3).
            let t = ellipse_param(
                p_r_pt,
                center,
                normal,
                major_axis,
                major_radius,
                minor_radius,
            );
            normalize3(ellipse_tangent(
                normal,
                major_axis,
                major_radius,
                minor_radius,
                t,
            ))
        }
        Curve::Hyperbola {
            center,
            normal,
            major_axis,
            semi_transverse,
            semi_conjugate,
        } => {
            // PR-YR23 / N89 item 3: hyperbola tangent `d/dt point(t) =
            // a·sinh(t)·major + b·cosh(t)·(normal × major_axis)`, evaluated at
            // `t = asinh(v_coord / b)` with `v_coord = (p_r − center)·
            // (normal × major_axis)` matching `geom::hyperbola_point` /
            // `geom::hyperbola_param`.
            if !(semi_transverse.is_finite()
                && semi_transverse > 0.0
                && semi_conjugate.is_finite()
                && semi_conjugate > 0.0)
            {
                return Some(None);
            }
            let n = normalize3(normal.as_array());
            let maj = normalize3(major_axis.as_array());
            let conj = [
                n[1] * maj[2] - n[2] * maj[1],
                n[2] * maj[0] - n[0] * maj[2],
                n[0] * maj[1] - n[1] * maj[0],
            ];
            let ctr = center.as_array();
            let pr = p_r_pt.as_array();
            let v_coord = (pr[0] - ctr[0]) * conj[0]
                + (pr[1] - ctr[1]) * conj[1]
                + (pr[2] - ctr[2]) * conj[2];
            let t = (v_coord / semi_conjugate).asinh();
            let (sh, ch) = (t.sinh(), t.cosh());
            normalize3([
                semi_transverse * sh * maj[0] + semi_conjugate * ch * conj[0],
                semi_transverse * sh * maj[1] + semi_conjugate * ch * conj[1],
                semi_transverse * sh * maj[2] + semi_conjugate * ch * conj[2],
            ])
        }
        Curve::LineSegment => return Some(None),
        // M5: a `SurfacePair` site is routed through `SiteTest::ParamOrder`'s
        // `n_a × n_b` tangent arm (`all_conic` is `false` for non-planar curves);
        // defensive `Some(None)` if `angle_site_tangent` is called directly.
        Curve::SurfacePair { .. } => return Some(None),
    };
    Some(Some(tan_c))
}
