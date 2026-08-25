//! Stage-4 reversed-intersection sweep and the curve-comparison helpers it
//! relies on: PR-YR10 §4.5.3 ordered-loop reversal correction
//! (`sweep_reversed_intersections`), conic parameterization/equality tests,
//! mixed-cycle shared-conic detection, and the collapse/merge direction
//! predicates. Extracted move-only from stage4_correct.rs (#159 F9).

#[allow(clippy::wildcard_imports)]
use crate::*;

/// PR-YR10 (§4.5.3): walk every ordered intersection loop and correct reversed
/// points by edge-collapsing the offending next-point. Returns `true` iff any
/// collapse occurred. LOUD STOP on an unresolvable reversal.
pub(crate) fn sweep_reversed_intersections(
    mesh: &mut Mesh,
    attribution: &mut Vec<Option<TriangleAttribution>>,
    a: &BRep,
    b: &BRep,
    d_eps: f64,
) -> Result<bool, YangError> {
    use std::collections::HashSet;
    const ANG_TOL: f64 = 1e-6; // radians (Yang §5).
    let lo = std::f64::consts::FRAC_PI_4 - ANG_TOL; // 45° − tol
    let hi = 3.0 * std::f64::consts::FRAC_PI_4 + ANG_TOL; // 135° + tol

    // Spec `yang_453_pair_chain_reversal` §4 (FLIPPED ALWAYS-ON 2026-08-24
    // after the gated corpus run: R0028 ERROR→CORRECT, every other case
    // byte-identical): unset/`1` = act; `0`/`off` = the dev A/B off-knob;
    // `census` = report pair-site reversals, never act.
    let pair_arm = std::env::var("YANG_453_PAIR");
    let pair_arm_census = matches!(pair_arm.as_deref(), Ok("census"));
    let pair_arm_act = !pair_arm_census && !matches!(pair_arm.as_deref(), Ok("0") | Ok("off"));

    let mut collapsed_any = false;
    // Bound the outer restart loop by the initial triangle count (each pass
    // either makes progress by collapsing ≥1 triangle or terminates).
    let max_passes = mesh.tris.len() + 1;
    let mut passes = 0usize;
    loop {
        passes += 1;
        if passes > max_passes {
            // Could not reach a fixed point — genuine §4.5.2 territory.
            return Err(YangError::stage4_region_invalid(
                u32::MAX,
                Stage4InvalidReason::LocalRefinementRequired,
            ));
        }

        // Recompute Phase A so the loops reflect any prior collapse (spec §4.5.3
        // step 3 — re-sweep on fresh loops, never stale ones).
        let map = TriangleAttributionMap {
            attributions: std::mem::take(attribution),
        };
        // Post-relocation context: position keys are stale, pass no provenance.
        let phase_a = compute_phase_a(mesh, &map, a, b, &crate::stage3_ssi::NO_EDGE_PROVENANCE);
        *attribution = map.attributions;
        let (infos, incidence, curves) = phase_a?;

        // Collect the ordered intersection loops. Dedup by sorted vertex set so
        // the cylinder-side and cap-side copies of the same ring are swept once.
        let mut seen: HashSet<Vec<u32>> = HashSet::new();
        let mut loops: Vec<(Vec<(u32, u32)>, bool)> = Vec::new();
        for info in &infos {
            for cycle in &info.cycles {
                if cycle.len() < 3 {
                    continue;
                }
                // PR-YR11 widened Circle-only to `all_conic`; spec §3c widens
                // again to PER-SITE eligibility: any cycle containing at
                // least one intersection edge is scanned, and `is_reversed`
                // skips every position whose incident edges are not BOTH
                // intersection edges (real face boundaries mix solid edges
                // with seam runs — whole-cycle gates never fire on them).
                let any_intersection = cycle.iter().any(|&(s, e)| {
                    let key = if s < e { (s, e) } else { (e, s) };
                    matches!(
                        curves.get(&key),
                        Some(Curve::Circle { .. })
                            | Some(Curve::Ellipse { .. })
                            | Some(Curve::LineSegment)
                    )
                });
                // Spec `yang_453_pair_chain_reversal` §3: under the pair arm,
                // an UNTYPED both-input edge (a pair-relocated intersection
                // edge — e.g. torus∩cylinder) also qualifies the cycle; a
                // fully-quartic boundary has no typed edge at all and was
                // invisible to the sweep. `YANG_453_PAIR=0|off` restores the
                // pre-arm collection byte-identically.
                let any_pair = (pair_arm_act || pair_arm_census)
                    && cycle.iter().any(|&(s, e)| {
                        let key = if s < e { (s, e) } else { (e, s) };
                        !curves.contains_key(&key)
                            && incidence.get(&key).is_some_and(|entries| {
                                entries.iter().any(|&(i, _)| i == InputId::A)
                                    && entries.iter().any(|&(i, _)| i == InputId::B)
                            })
                    });
                if !(any_intersection || any_pair) {
                    continue;
                }
                // Spec §3c final scope: ALL-CONIC cycles keep the pre-§3c
                // semantics byte-identically; in MIXED cycles only
                // straight-run sites (both incident edges LineSegment) are
                // swept. Conic sites inside mixed cycles are DISPROVEN twice
                // (spec §3c P10 records): the reversal angle test
                // false-positives on coarse conic chords (a 7-gon's 51°
                // corners exceed the 45° band — `corner_in_band` adversary),
                // and overlay-adjacent conic runs repair unsupported Stage-0
                // crossings into silent geometry (the hole-rim pin).
                let all_conic = cycle.iter().all(|&(s, e)| {
                    let key = if s < e { (s, e) } else { (e, s) };
                    matches!(
                        curves.get(&key),
                        Some(Curve::Circle { .. }) | Some(Curve::Ellipse { .. })
                    )
                });
                let mut sorted: Vec<u32> = cycle.iter().map(|&(s, _)| s).collect();
                sorted.sort_unstable();
                if seen.insert(sorted) {
                    loops.push((cycle.clone(), all_conic));
                }
            }
        }

        // Find the FIRST reversal across all loops; collapse, then restart the
        // whole sweep (re-deriving loops). Deterministic: loops are in the
        // deterministic patch/cycle order; within a loop we scan in order.
        let mut acted = false;
        'outer: for (cycle, all_conic) in &loops {
            let m = cycle.len();
            if m < 3 {
                return Err(YangError::stage4_region_invalid(
                    cycle.first().map(|&(s, _)| s).unwrap_or(u32::MAX),
                    Stage4InvalidReason::LoopTooSmall,
                ));
            }
            // Ordered vertex sequence of the loop (start vertices).
            let verts: Vec<u32> = cycle.iter().map(|&(s, _)| s).collect();
            for i in 0..m {
                let p_b = verts[(i + m - 1) % m];
                let p_r = verts[i];
                let p_n = verts[(i + 1) % m];
                // Spec §3c site rule: in a MIXED cycle only straight-run
                // sites (both incident edges LineSegment) are eligible for
                // `is_reversed`; task #145 (spec
                // `yang_453_mixed_cycle_conic_backtrack`) adds SHARED-CONIC
                // sites, tested by exact parameter order instead of the
                // P10-disproven angle band.
                let mut conic_backtrack: Option<(u32, u32)> = None;
                if !all_conic {
                    let key_n = if p_r < p_n { (p_r, p_n) } else { (p_n, p_r) };
                    let key_b = if p_b < p_r { (p_b, p_r) } else { (p_r, p_b) };
                    // Spec `yang_453_pair_chain_reversal` §3: BOTH incident
                    // edges untyped ⇒ a candidate PAIR site (the torus-block
                    // population, never previously swept). Gated; every path
                    // here ends by setting the victim or skipping the site —
                    // the typed arms below never see untyped edges (their
                    // matches! on `Some(..)` are false for `None`, so today
                    // these sites die in the shared-conic `continue`).
                    if !curves.contains_key(&key_n) && !curves.contains_key(&key_b) {
                        if !(pair_arm_act || pair_arm_census) {
                            continue;
                        }
                        let Some((vic, sur)) = pair_site_reversal(mesh, &incidence, p_b, p_r, p_n)
                        else {
                            continue;
                        };
                        if pair_arm_census {
                            eprintln!(
                                "[s453-pair] REVERSAL p_b={p_b} p_r={p_r} p_n={p_n}                                  victim={vic} survivor={sur} (census: not acting)"
                            );
                            continue;
                        }
                        conic_backtrack = Some((vic, sur));
                    } else if !both_line_edges(&curves, key_b, key_n) {
                        // Task #145 diagnosis probe (read-only, env-gated): a
                        // conic site skipped by the mixed-cycle rule whose two
                        // incident edges carry the SAME conic AND whose discrete
                        // tangent U-turns (backtrack along the curve).
                        if std::env::var_os("YANG_T145_SWEEP_PROBE").is_some() {
                            if let (Some(cn), Some(cb)) = (curves.get(&key_n), curves.get(&key_b)) {
                                if cn == cb || conics_equal_up_to_normal_sign(cn, cb) {
                                    let pb = mesh.verts[p_b as usize].as_array();
                                    let pr = mesh.verts[p_r as usize].as_array();
                                    let pn = mesh.verts[p_n as usize].as_array();
                                    let v1 =
                                        normalize3([pr[0] - pb[0], pr[1] - pb[1], pr[2] - pb[2]]);
                                    let v2 =
                                        normalize3([pn[0] - pr[0], pn[1] - pr[1], pn[2] - pr[2]]);
                                    let tt = [v1[0] + v2[0], v1[1] + v2[1], v1[2] + v2[2]];
                                    let ttl =
                                        (tt[0] * tt[0] + tt[1] * tt[1] + tt[2] * tt[2]).sqrt();
                                    if ttl < 0.5 {
                                        eprintln!(
                                            "[t145-sweep] mixed-cycle conic U-turn skip: \
                                             p_b={p_b} p_r={p_r} p_n={p_n} |t~|={ttl:.3e} \
                                             same_struct={} cn={cn:?}",
                                            cn == cb
                                        );
                                    }
                                }
                            }
                        }
                        // Task #145 branches 9–12: a shared-conic site in a
                        // mixed cycle is a §4.5.3 site iff the three points
                        // fail to progress along the conic in PARAMETER
                        // order (the paper's criterion, tested exactly —
                        // never the angle band, whose coarse-chord false
                        // positives are P10-disproven here). Victim
                        // selection and the 2·d_ε resolution gate below are
                        // the existing shared path.
                        let Some(shared) = mixed_cycle_shared_conic(&curves, key_b, key_n) else {
                            continue;
                        };
                        let Some((d1, d2)) = conic_param_deltas(
                            &shared,
                            mesh.verts[p_b as usize],
                            mesh.verts[p_r as usize],
                            mesh.verts[p_n as usize],
                        ) else {
                            continue;
                        };
                        let reversed = d1 * d2 < 0.0;
                        if std::env::var_os("YANG_T145_SWEEP_PROBE").is_some() {
                            eprintln!(
                                "[t145-arm] site p_b={p_b} p_r={p_r} p_n={p_n} \
                                 d1={d1:.3e} d2={d2:.3e} reversed={reversed} \
                                 pos_b={:?} pos_r={:?} pos_n={:?}",
                                mesh.verts[p_b as usize],
                                mesh.verts[p_r as usize],
                                mesh.verts[p_n as usize],
                            );
                        }
                        if !reversed {
                            continue;
                        }
                        // Victim = p_r (the same-conic site vertex whose
                        // relocation overshot); survivor = the parameter-
                        // NEARER bracketing neighbor, so the collapse length
                        // equals the actual overshoot and the 2·d_ε gate
                        // bounds it honestly (spec branches 9a/9b).
                        let survivor = if d1.abs() <= d2.abs() { p_b } else { p_n };
                        conic_backtrack = Some((p_r, survivor));
                    }
                }
                if conic_backtrack.is_some()
                    || is_reversed(mesh, &curves, &incidence, p_b, p_r, p_n, lo, hi)
                {
                    // Spec `yang_453_junction_protected_collapse` §3: pick the
                    // collapse victim so a curve-junction vertex (the exact
                    // endpoint shared by two different conic sections, or the
                    // §3c surface-pair change on a straight run) always
                    // survives — Yang §4.5.3 removes points progressing along
                    // ONE curve C, never C's endpoints. The task-#145
                    // shared-conic arm carries its own (victim, survivor):
                    // p_r onto its parameter-nearer neighbor (spec
                    // `yang_453_mixed_cycle_conic_backtrack` branches 9a/9b).
                    let p_after = verts[(i + 2) % m];
                    let (victim, survivor) = conic_backtrack.unwrap_or_else(|| {
                        reversal_collapse_direction(&curves, &incidence, p_r, p_n, p_after)
                    });
                    // Spec §3c resolution gate: §4.5.3 corrects RESOLUTION
                    // artifacts ("the mesh resolution is not sufficient to
                    // maintain a one-to-one mapping") — both the reversed
                    // point and its survivor sit within their own Stage-1
                    // chord band of the true curve position, so a legitimate
                    // correction moves at most 2·d_ε (the sum of the two
                    // bands — derived, not widening; same derivation as the
                    // line+circle junction gate). A LARGER excursion is not a
                    // resolution artifact but wrong topology (e.g. an
                    // unsupported Stage-0 crossing) — leave the reversal for
                    // the downstream validation to reject loudly (P9: the
                    // sweep must never repair unsupported configurations
                    // into silent geometry; pinned by
                    // `annular_cap_hole_crossing_stays_loud`).
                    {
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
                            continue;
                        }
                    }
                    if std::env::var_os("YANG_V_PROBE").is_some() {
                        eprintln!(
                            "YANG_V_PROBE reversal collapse: p_b={p_b} p_r={p_r} p_n={p_n} \
                             victim={victim} survivor={survivor} at {:?} <- {:?}",
                            mesh.verts.get(survivor as usize),
                            mesh.verts.get(victim as usize),
                        );
                    }
                    if std::env::var_os("YANG_DOUBLECOVER_PROBE").is_some() {
                        eprintln!(
                            "[collapse-site] s4.5.3-reversal victim={victim} survivor={survivor}"
                        );
                    }
                    let dropped = collapse_vertex(mesh, attribution, victim, survivor);
                    if dropped == 0 {
                        // Nothing collapsed ⇒ cannot make progress on this
                        // reversal. LOUD STOP.
                        return Err(YangError::Stage4ReversalUnresolved {
                            edge: if p_r < p_n { (p_r, p_n) } else { (p_n, p_r) },
                            vertex: p_r,
                        });
                    }
                    collapsed_any = true;
                    acted = true;
                    break 'outer;
                }
            }
        }

        if !acted {
            // Fixed point: no reversal remains.
            return Ok(collapsed_any);
        }
    }
}

/// Spec `yang_453_pair_chain_reversal` §3 — are both incident edges typed
/// `LineSegment` (a straight run in a mixed cycle)? Extracted so the pair
/// arm's insertion keeps the original branch byte-identical.
fn both_line_edges(
    curves: &std::collections::BTreeMap<(u32, u32), Curve>,
    key_b: (u32, u32),
    key_n: (u32, u32),
) -> bool {
    matches!(curves.get(&key_n), Some(Curve::LineSegment))
        && matches!(curves.get(&key_b), Some(Curve::LineSegment))
}

/// Spec `yang_453_pair_chain_reversal` §3: pair-site reversal detection.
///
/// Site eligibility: each incident edge's Phase-A incidence dedups to
/// EXACTLY 2 distinct `(input, surface)` entries with both inputs present
/// (an intersection edge — ≥3 entries is a junction edge, no verdict), and
/// the two edges carry the SAME pair. p_r is then chain-INTERIOR by
/// construction — a junction vertex can never be the victim (the conic
/// arm's safety argument with pair identity in place of curve identity).
///
/// Reversal test (Yang Fig. 15 in its general-surface form, as a
/// progression-sign test — never the angle band, whose coarse-chord false
/// positives are P10-disproven for conics): `t = n₀ × n₁` at p_r
/// (|cross| < 1e-6 = Yang §5's angular tolerance ⇒ near-tangential, no
/// verdict); reversal ⇔ `((p_r−p_b)·t) · ((p_n−p_r)·t) < 0`. Victim = p_r,
/// survivor = the tangent-nearer bracketing neighbour (spec
/// `yang_453_mixed_cycle_conic_backtrack` branches 9a/9b, same shape).
fn pair_site_reversal(
    mesh: &Mesh,
    incidence: &std::collections::BTreeMap<(u32, u32), Vec<(InputId, Surface)>>,
    p_b: u32,
    p_r: u32,
    p_n: u32,
) -> Option<(u32, u32)> {
    const PAIR_RANK_FLOOR: f64 = 1e-6; // sin(angle) floor — Yang §5 angular tol.
    let key = |a: u32, b: u32| if a < b { (a, b) } else { (b, a) };
    let pair_of = |k: (u32, u32)| -> Option<[(InputId, Surface); 2]> {
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
    };
    let bp = pair_of(key(p_b, p_r))?;
    let np = pair_of(key(p_r, p_n))?;
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
    let pb = mesh.verts[p_b as usize].as_array();
    let pn = mesh.verts[p_n as usize].as_array();
    let d1 = ((pr[0] - pb[0]) * t[0] + (pr[1] - pb[1]) * t[1] + (pr[2] - pb[2]) * t[2]) / tl;
    let d2 = ((pn[0] - pr[0]) * t[0] + (pn[1] - pr[1]) * t[1] + (pn[2] - pr[2]) * t[2]) / tl;
    if d1 * d2 >= 0.0 {
        return None;
    }
    Some((p_r, if d1.abs() <= d2.abs() { p_b } else { p_n }))
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
        _ => false,
    }
}

/// Task #145 (spec `yang_453_mixed_cycle_conic_backtrack`): the exact conic
/// parameter of `pt` on a `Circle`/`Ellipse`, in the shared PR-YR11 frame
/// (`project_onto_circle` / `ellipse_param` — the same parameterization
/// Stage-4 relocation and `is_reversed` use). `None` for non-conic payloads
/// or a degenerate projection (branch 11 — cannot diagnose).
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
        _ => None,
    }
}

/// Task #145 branch 9/10: do `p_b`, `p_r`, `p_n` FAIL to progress along the
/// shared conic in parameter order? Yang §4.5.3's criterion is "points
/// progressing along the intersection curve in sequence"; for closed-form
/// conics the curve parameter tests this directly (the paper's discrete
/// tangent test is the general-surface proxy, whose 45° band is P10-disproven
/// at coarse-chord conic sites — spec §3c records). Consecutive deltas are
/// wrapped to (−π, π] (each Stage-1 chord subtends < π); reversal ⟺ the two
/// deltas have opposite signs. A zero delta (coincident parameters) and a
/// degenerate projection are healthy — cannot diagnose (branch 11).
/// (The sweep consumes `conic_param_deltas` directly — it also needs the
/// survivor side; this equivalent boolean form is the branch-table oracle.)
#[cfg(test)]
pub(crate) fn conic_param_reversed(curve: &Curve, p_b: Point3, p_r: Point3, p_n: Point3) -> bool {
    conic_param_deltas(curve, p_b, p_r, p_n).is_some_and(|(d1, d2)| d1 * d2 < 0.0)
}

/// Task #145: the wrapped conic-parameter deltas `(t_r − t_b, t_n − t_r)` of a
/// shared-conic site, each in (−π, π]. `None` when any parameter is undefined
/// (branch 11). The reversal test is `d1·d2 < 0`; the collapse survivor is the
/// parameter-NEARER bracketing neighbor (the endpoint `p_r` overshot — its
/// distance is the actual overshoot, which the 2·d_ε resolution gate can
/// honestly bound; the FAR neighbor is a whole arc away and never a resolution
/// artifact).
pub(crate) fn conic_param_deltas(
    curve: &Curve,
    p_b: Point3,
    p_r: Point3,
    p_n: Point3,
) -> Option<(f64, f64)> {
    let (t_b, t_r, t_n) = (
        conic_param(curve, p_b)?,
        conic_param(curve, p_r)?,
        conic_param(curve, p_n)?,
    );
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
/// site — `Some` iff BOTH incident edges carry the SAME `Circle`/`Ellipse`
/// (exact struct equality or equality up to the stored normal's sign, I5).
/// Junctions (different conics), conic/LineSegment boundaries, and straight
/// runs (the §3c both-line arm) return `None`.
pub(crate) fn mixed_cycle_shared_conic(
    curves: &std::collections::BTreeMap<(u32, u32), Curve>,
    key_b: (u32, u32),
    key_n: (u32, u32),
) -> Option<Curve> {
    let cb = curves.get(&key_b)?;
    let cn = curves.get(&key_n)?;
    if !matches!(cn, Curve::Circle { .. } | Curve::Ellipse { .. }) {
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

/// §4.5.3 collapse direction (spec `yang_453_junction_protected_collapse` §3):
/// which loop vertex is REMOVED for a reversal detected at `p_r` with next
/// point `p_n` (whose own next point is `p_after`)? Returns
/// `(victim, survivor)` for [`collapse_vertex`].
///
/// Yang §4.5.3 (Fig. 15, `refs/text/yang2025_hybrid_boolean.txt:709-745`)
/// removes `p_n` — but its setting is consecutive points progressing along ONE
/// intersection curve C. When `p_n` is a curve JUNCTION (the loop's curve
/// changes there: `curve(p_r,p_n) ≠ curve(p_n,p_after)`), `p_n` is C's exact
/// closed-form endpoint and must survive; the out-of-order point is `p_r`
/// itself, whose §4.4.1 relocation overshot C's end — so `p_r` collapses onto
/// the junction. `is_reversed` returning true implies both edges at `p_r`
/// carry the SAME curve (PR-KV11 guard), so `p_r` is never itself a junction
/// here, and the victim always lies on the survivor's curve (spec I3).
pub(crate) fn reversal_collapse_direction(
    curves: &std::collections::BTreeMap<(u32, u32), Curve>,
    incidence: &std::collections::BTreeMap<(u32, u32), Vec<(InputId, Surface)>>,
    p_r: u32,
    p_n: u32,
    p_after: u32,
) -> (u32, u32) {
    // Spec §3c branch 6: on a straight run, a surface-pair change at p_n is
    // the junction (LineSegment payloads compare equal, so curve identity
    // alone cannot see it).
    if same_line_run(curves, incidence, p_r, p_n, p_after) == Some(false) {
        return (p_r, p_n);
    }
    let key_n = if p_r < p_n { (p_r, p_n) } else { (p_n, p_r) };
    let key_after = if p_n < p_after {
        (p_n, p_after)
    } else {
        (p_after, p_n)
    };
    match (curves.get(&key_n), curves.get(&key_after)) {
        (Some(cn), Some(ca)) if cn != ca => (p_r, p_n),
        // Spec §3c: the run ENDS at p_n (its far edge is not an intersection
        // edge — a solid edge or curve-less seam). p_n is the run's exact
        // endpoint and must survive; the overshooting p_r is the victim.
        (Some(_), None) => (p_r, p_n),
        _ => (p_n, p_r),
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
/// exact circle tangent at `p_r`. Collinear `t̃` (`|t̃| < TAU_WORK`) is the
/// HEALTHY case — skip the angle test (Yang §4.5.3). Reversal ⟺ the unsigned
/// angle ∈ (45°, 135°) (with the supplied 1e-6 rad slack baked into `lo`/`hi`).
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
                    return false;
                }
            }
            // Spec §3c: PER-SITE eligibility — p_r is a §4.5.3 site only when
            // BOTH incident edges are intersection edges. A run boundary
            // (intersection meets solid edge) is a genuine topology corner.
            _ => return false,
        }
    }
    // Spec §3c branch 4: two straight seam edges compare curve-equal
    // (`LineSegment` carries no payload), so run identity uses the unordered
    // incidence surface pair — a pair change at p_r is a genuine corner
    // (including near-180° thin-wedge corners the U-turn test below would
    // otherwise misread as reversals).
    match same_line_run(curves, incidence, p_b, p_r, p_n) {
        Some(false) => return false,
        Some(true) => {
            // Spec §3c branch 5, checked BEFORE the U-turn arm: the §4.5.3
            // test needs the exact tangent t_pr = n_A × n_B (Yang Fig. 15).
            // A COINCIDENT/parallel pair (the §4.5.5 overlay seams — both
            // incident faces on the same two planes) has no cross-product
            // tangent, so NO reversal can be diagnosed there at all — the
            // overlay boundary legitimately turns corners (including 180°
            // crossing artifacts that must stay loud downstream; pinned by
            // `annular_cap_hole_crossing_stays_loud`).
            let key = if p_r < p_n { (p_r, p_n) } else { (p_n, p_r) };
            let tangent_defined = incidence.get(&key).is_some_and(|entries| {
                if let [(_, s0), (_, s1)] = entries[..] {
                    let p_r_pt = mesh.verts[p_r as usize];
                    if let (Some(n0), Some(n1)) =
                        (surface_normal_at(s0, p_r_pt), surface_normal_at(s1, p_r_pt))
                    {
                        let cr = [
                            n0[1] * n1[2] - n0[2] * n1[1],
                            n0[2] * n1[0] - n0[0] * n1[2],
                            n0[0] * n1[1] - n0[1] * n1[0],
                        ];
                        return (cr[0] * cr[0] + cr[1] * cr[1] + cr[2] * cr[2]).sqrt()
                            >= cad_primitives::TAU_WORK;
                    }
                }
                false
            });
            if !tangent_defined {
                return false;
            }
        }
        None => {}
    }
    let pb = mesh.verts[p_b as usize].as_array();
    let pr = mesh.verts[p_r as usize].as_array();
    let pn = mesh.verts[p_n as usize].as_array();
    let v1 = normalize3([pr[0] - pb[0], pr[1] - pb[1], pr[2] - pb[2]]);
    let v2 = normalize3([pn[0] - pr[0], pn[1] - pr[1], pn[2] - pr[2]]);
    let t_tilde = [v1[0] + v2[0], v1[1] + v2[1], v1[2] + v2[2]];
    let t_tilde_len =
        (t_tilde[0] * t_tilde[0] + t_tilde[1] * t_tilde[1] + t_tilde[2] * t_tilde[2]).sqrt();
    if t_tilde_len < cad_primitives::TAU_WORK {
        // Degenerate/collinear t̃ (|t̃| ≈ 0 ⟺ v1 ≈ −v2 ⟺ the polyline doubles
        // back at p_r). Yang §4.5.3 (lines 743-745) places this collinear case
        // WITHIN the reversal subset — the angle test is undefined here, so
        // "directly detect the reversal, avoiding the angle comparisons." A
        // U-turn IS a reversal. (Prior code returned `false`/"healthy" — the N3
        // logic inversion; see docs/yang_deviations.md.)
        return true;
    }

    // Exact conic tangent at p_r. Find the Circle OR Ellipse this edge carries
    // (PR-YR11: ellipse edges compute the ellipse tangent). Prefer the current
    // edge `(p_r, p_n)`; fall back to the previous edge `(p_b, p_r)`.
    let key = if p_r < p_n { (p_r, p_n) } else { (p_n, p_r) };
    let key2 = if p_b < p_r { (p_b, p_r) } else { (p_r, p_b) };
    let conic = match curves.get(&key) {
        Some(c @ (Curve::Circle { .. } | Curve::Ellipse { .. })) => Some(*c),
        _ => match curves.get(&key2) {
            Some(c @ (Curve::Circle { .. } | Curve::Ellipse { .. })) => Some(*c),
            _ => None,
        },
    };
    let p_r_pt = mesh.verts[p_r as usize];
    let Some(conic) = conic else {
        // Spec §3c: straight-run arm. When BOTH edges are `LineSegment` on the
        // SAME run (the branch-4 guard above already returned for pair
        // changes), the exact intersection-curve tangent at p_r is
        // `n_A × n_B` of the run's surface pair (Yang Fig. 15,
        // refs/text/yang2025_hybrid_boolean.txt:736-742).
        if same_line_run(curves, incidence, p_b, p_r, p_n) == Some(true) {
            if let Some(entries) = incidence.get(&key) {
                if let [(_, s0), (_, s1)] = entries[..] {
                    if let (Some(n0), Some(n1)) =
                        (surface_normal_at(s0, p_r_pt), surface_normal_at(s1, p_r_pt))
                    {
                        let cr = [
                            n0[1] * n1[2] - n0[2] * n1[1],
                            n0[2] * n1[0] - n0[0] * n1[2],
                            n0[0] * n1[1] - n0[1] * n1[0],
                        ];
                        let m = (cr[0] * cr[0] + cr[1] * cr[1] + cr[2] * cr[2]).sqrt();
                        // Spec §3c branch 5: tangent/parallel surface pair
                        // (|n_A × n_B| = sin ∠ ≈ 0, e.g. §4.5.5 coplanar
                        // seams) — the curve direction is undefined; healthy.
                        if m >= cad_primitives::TAU_WORK {
                            let tan_c = [cr[0] / m, cr[1] / m, cr[2] / m];
                            let t_tilde_u = normalize3(t_tilde);
                            let dotv = (t_tilde_u[0] * tan_c[0]
                                + t_tilde_u[1] * tan_c[1]
                                + t_tilde_u[2] * tan_c[2])
                                .clamp(-1.0, 1.0);
                            let angle = dotv.abs().acos();
                            return angle > lo && angle < hi;
                        }
                    }
                }
            }
        }
        // No exact tangent available — cannot diagnose; treat as healthy
        // (the validation pass still guards inverted/degenerate triangles).
        return false;
    };
    let tan_c = match conic {
        Curve::Parabola {
            vertex,
            normal,
            axis_dir,
            focal_length,
        } => {
            // PR-YR22: parabola tangent `d/dt point(t) = (t/(2f))·axis_dir +
            // (normal × axis_dir)`, evaluated at the conjugate-axis coordinate
            // `t = (p_r − vertex)·(normal × axis_dir)` (the same tag the Stage-4
            // parabola loop stores). Defensively correct even though the open-arc
            // parabola section is excluded from the closed-loop `all_conic` sweep.
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
                return false;
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
            // PR-YR23: hyperbola tangent `d/dt point(t) = a·sinh(t)·major +
            // b·cosh(t)·(normal × major_axis)`, evaluated at the tag
            // `t = asinh(v_coord / b)` with `v_coord = (p_r − center)·
            // (normal × major_axis)` (the same tag the Stage-4 hyperbola loop
            // stores). Defensively correct even though the open-arc hyperbola
            // section is excluded from the closed-loop `all_conic` sweep
            // (which selects only Circle/Ellipse), so this arm is never reached.
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
        Curve::LineSegment => return false,
        // M5: a surface-pair curve is pre-filtered out before this match (only
        // Circle/Ellipse reach here); defensive `false` like `LineSegment`.
        Curve::SurfacePair { .. } => return false,
    };
    let t_tilde_u = normalize3(t_tilde);
    let dotv = (t_tilde_u[0] * tan_c[0] + t_tilde_u[1] * tan_c[1] + t_tilde_u[2] * tan_c[2])
        .clamp(-1.0, 1.0);
    // Unsigned angle between t̃ and the exact tangent (sign of the tangent is
    // arbitrary, so fold to [0, π/2] via |dot|).
    let angle = dotv.abs().acos();
    angle > lo && angle < hi
}
