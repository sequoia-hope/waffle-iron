#[allow(clippy::wildcard_imports)]
use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn run_construct_passes(
    mesh: &mut Mesh,
    attribution: &mut TriangleAttributionMap,
    side_labels: &mut crate::brep::TriangleSideLabelMap,
    a: &BRep,
    b: &BRep,
    infos: &mut Vec<crate::stage4_correct::PatchInfo>,
    intersection_curves: &mut std::collections::BTreeMap<(u32, u32), Curve>,
    relocations: &mut Vec<(u32, f64)>,
    full_arrangement: bool,
    mut tangential_verts: Option<&mut std::collections::BTreeSet<u32>>,
) -> Result<usize, YangError> {
    use crate::stage4_construct::{apply_rebuild_batch, replace_seam_run, seam_groups};
    use crate::stage4_splice::{ordered_seam_side, Side, SplicePatch};
    use std::collections::{BTreeMap, BTreeSet};

    // Every seam collapses at most once (its chain drops below 3 vertices),
    // so the pass count is bounded by the seam count; the cap is a runaway
    // guard, not an expected limit. The batch path consumes NO tolerance:
    // collapse is pure index rewriting and the rebuild is a plain CDT.
    const MAX_PASSES: usize = 64;
    // I5-1 (spec §4-I5): per-seam §4.3.4 insert budget — the I2e-precedent
    // runaway backstop (a seam pricing above it declines to reorder-only,
    // censused; the shipped coarse chain is the pre-I5 status quo).
    const I5_INSERT_CAP: u64 = 4096;
    // The §4.3.4 insert is ALWAYS-ON since the I5-2 flip (2026-08-19, spec
    // §4-I5-2). I5-1 orphan floor: appended on-curve vertices below this index are
    // referenced by applied rebuilds (or predate the pass); anything above
    // it at an exit path was appended for a seam whose batch never applied
    // and is truncated (appends are always a contiguous unreferenced tail
    // until a batch applies; the floor is re-snapshotted after each applied
    // batch's compact).
    let mut verts_floor = mesh.verts.len();

    // Bisection probes (env-gated, deterministic): cap which BOOLEANS may
    // apply (process-order index) and the TOTAL applied-seam budget. For
    // localizing a gate-ON regression to one boolean / one seam prefix;
    // census still runs in full either way. They PERTURB a result, so they
    // are debug-build adversary knobs only (cleanup spec §4 class 1): a
    // release build never reads them.
    static BOOL_INDEX: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let bool_idx = BOOL_INDEX.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let env_cap = |name: &str| -> Option<usize> {
        if !cfg!(debug_assertions) {
            return None;
        }
        std::env::var(name).ok().and_then(|v| v.parse().ok())
    };
    let apply_enabled = env_cap("YANG_441_APPLY_BOOL_CAP").is_none_or(|c| bool_idx < c);
    let seam_budget = env_cap("YANG_441_APPLY_SEAM_CAP");
    if !apply_enabled {
        c441_log!("[s4-construct] CENSUS-ONLY (boolean index {bool_idx} at/above bool cap)");
    }

    let mut applied_total = 0usize;
    for pass in 0..MAX_PASSES {
        let adjacency = triangle_adjacency(mesh);
        let sl_opt = full_arrangement.then_some(&*side_labels);
        let raw = crate::stage4_correct::merge_same_plane_patches_with_side_labels(
            flood_fill_patches_with_side_labels(mesh, attribution, sl_opt, &adjacency),
            &adjacency,
            sl_opt,
            a,
            b,
        );
        if raw.len() != infos.len() {
            eprintln!(
                "[s4-construct] STOP pass={pass}: patch/info correspondence broken \
                 ({} patches vs {} infos)",
                raw.len(),
                infos.len(),
            );
            break;
        }
        let patches: Vec<SplicePatch> = raw
            .iter()
            .zip(infos.iter())
            .map(|(p, i)| SplicePatch {
                cycles: i
                    .cycles
                    .iter()
                    .map(|c| c.iter().map(|&(s, _)| s).collect())
                    .collect(),
                tris: p.tri_indices.clone(),
                surface: i.inherited,
            })
            .collect();

        let groups = seam_groups(&patches, intersection_curves);
        // Skip census: (non-line, curved-patch, closed, minimal, unorderable,
        // run-not-contiguous, declined/refused, non-straight). The I2
        // worklist is measured, not inferred.
        let mut skip = [0usize; 8];

        // ---- Eligibility: per seam, each filter loud. -----------------------
        // Two actions (I2b): a LINE seam COLLAPSES to its junction endpoints
        // (the resample of a straight curve IS the two endpoints); a CONIC
        // seam REORDERS its run to the curve's parameter order (the on-curve
        // vertex set is the §4.3.4 sample chain — dropping it would coarsen
        // the curve; the defect is only the scrambled ORDER).
        enum SeamAction {
            CollapseLine,
            ReorderConic {
                ordered: Vec<u32>,
            },
            // I5-1 (always on): the §4.3.4-refined chain —
            // `ordered` with the paper's midpoint samples interleaved as
            // freshly appended on-curve vertices, shared by both owners.
            RefineConic {
                ordered: Vec<u32>,
                refined: Vec<u32>,
            },
        }
        struct EligibleSeam {
            gi: usize,
            owners: Vec<usize>,
            chain: Vec<u32>,
            action: SeamAction,
        }
        let mut eligible: Vec<EligibleSeam> = Vec::new();
        for (gi, g) in groups.iter().enumerate() {
            let owners = &g.owners;
            let pi = owners[0];
            // I2a: Plane AND Cylinder owners rebuild single-sided (interior
            // carry + θ-unwrap); I13a adds Cone;
            // Sphere/Torus stay a loud skip.
            let chartable = |s: &Surface| crate::stage4_project::SurfaceChart::supports(s);
            if owners.iter().any(|&oi| !chartable(&patches[oi].surface)) {
                skip[1] += 1;
                c441_log!(
                    "[s4-construct] pass={pass} seam={gi}: SKIP unchartable patch \
                     (patches {owners:?}) — I2 scope"
                );
                continue;
            }
            let (chain, closed) = match ordered_seam_side(&patches[pi].cycles, &g.edges, Side::A) {
                Ok(x) => x,
                Err(e) => {
                    skip[4] += 1;
                    c441_log!(
                        "[s4-construct] pass={pass} seam={gi}: SKIP unorderable chain \
                         (patches {owners:?}) — {e:?}"
                    );
                    continue;
                }
            };
            if closed {
                skip[2] += 1;
                c441_log!(
                    "[s4-construct] pass={pass} seam={gi}: SKIP closed seam \
                     (patches {owners:?}) — I2b tail",
                );
                continue;
            }
            if chain.len() < 3 {
                skip[3] += 1;
                continue; // already minimal — the construction's fixed point
            }
            let action = if matches!(g.curve, Curve::LineSegment) {
                // Straightness identity: `Curve::LineSegment` is a unit
                // variant, so one group can hold TWO different lines meeting
                // at a real corner (R0095's coplanar-contact class) —
                // collapsing that chain would CUT THE CORNER. On-line
                // scrambled chains pass; a corner is macroscopic.
                match crate::stage4_construct::chain_straightness(&mesh.verts, &chain) {
                    Some(s) if s <= 1e-9 => {}
                    s => {
                        skip[7] += 1;
                        c441_log!(
                            "[s4-construct] pass={pass} seam={gi}: SKIP non-straight chain \
                             (patches {owners:?}, off-line {s:?}) — not one line's seam",
                        );
                        continue;
                    }
                }
                if owners
                    .iter()
                    .any(|&oi| replace_seam_run(&patches[oi].cycles, &chain).is_none())
                {
                    skip[5] += 1;
                    c441_log!(
                        "[s4-construct] pass={pass} seam={gi}: SKIP — run not contiguous \
                         (patches {owners:?})"
                    );
                    continue;
                }
                SeamAction::CollapseLine
            } else {
                // I2b: the conic's parameter-ordered resample.
                let ordered = match crate::stage4_splice::order_along_curve(
                    &g.curve,
                    &mesh.verts,
                    &chain,
                    false,
                ) {
                    Ok(o) => o,
                    Err(crate::stage4_splice::SpliceError::SeamCurveNotConic) => {
                        skip[0] += 1;
                        c441_log!(
                            "[s4-construct] pass={pass} seam={gi}: SKIP non-conic curve \
                             (patches {owners:?}, {} edges) — no closed-form parameter",
                            g.edges.len()
                        );
                        continue;
                    }
                    Err(e) => {
                        skip[6] += 1;
                        c441_log!(
                            "[s4-construct] pass={pass} seam={gi}: DECLINED conic order \
                             (patches {owners:?}) — {e:?}"
                        );
                        continue;
                    }
                };
                // Carrier pre-check: `reorder_cycles_to_curve` and the I5-1
                // splice silently no-op when no cycle carries the whole seam.
                let carries = |cycles: &[Vec<u32>]| {
                    cycles.iter().any(|c| ordered.iter().all(|v| c.contains(v)))
                };
                // I5-1 (gated): attempt the §4.3.4 density refinement BEFORE
                // the fixed-point skip — an already-ordered chain may still
                // need densification. A decline (no closed-form param, or
                // pricing above the insert cap) falls back to the shipped
                // reorder-only behavior, logged.
                let refinement = match crate::stage4_construct::refine_conic_chain(
                    &mesh.verts,
                    &ordered,
                    &g.curve,
                    u32::try_from(mesh.verts.len()).expect("mesh vertex count fits u32"),
                    I5_INSERT_CAP,
                ) {
                    Some((pts, refined)) if !pts.is_empty() => Some((pts, refined)),
                    Some(_) => None, // already paper-dense — plain I2b
                    None => {
                        c441_log!(
                            "[s4-construct] pass={pass} seam={gi}: REFINE DECLINED \
                             (patches {owners:?}, {} verts — no param or over the \
                             {I5_INSERT_CAP} insert cap) — reorder only",
                            ordered.len()
                        );
                        None
                    }
                };
                if let Some((pts, refined)) = refinement {
                    if owners.iter().any(|&oi| !carries(&patches[oi].cycles)) {
                        skip[5] += 1;
                        c441_log!(
                            "[s4-construct] pass={pass} seam={gi}: SKIP — conic run not \
                             carried whole (patches {owners:?})"
                        );
                        continue;
                    }
                    c441_log!(
                        "[s4-construct] pass={pass} seam={gi}: REFINE patches {owners:?} — \
                         chain {} -> {} (+{} on-curve inserts, §4.3.4)",
                        ordered.len(),
                        refined.len(),
                        pts.len()
                    );
                    mesh.verts.extend_from_slice(&pts);
                    SeamAction::RefineConic { ordered, refined }
                } else {
                    // The construction's fixed point is a chain that is
                    // parameter-monotone in EITHER direction:
                    // `order_along_curve` returns the ASCENDING chain, and
                    // `reorder_cycles_to_curve` splices it REVERSED into a
                    // descending-traversal cycle — a NO-OP rewrite. Testing
                    // ascending equality only re-fired that no-op every
                    // pass until MAX_PASSES (the F0059 64/64 livelock,
                    // spec §4-I5 FINDING; each no-op still re-CDT'd the
                    // patch pair and re-derived Phase A).
                    let descending = ordered.iter().rev().eq(chain.iter());
                    if ordered == chain || descending {
                        skip[3] += 1; // already in curve order — the fixed point
                        continue;
                    }
                    if owners.iter().any(|&oi| !carries(&patches[oi].cycles)) {
                        skip[5] += 1;
                        c441_log!(
                            "[s4-construct] pass={pass} seam={gi}: SKIP — conic run not \
                             carried whole (patches {owners:?})"
                        );
                        continue;
                    }
                    SeamAction::ReorderConic { ordered }
                }
            };
            eligible.push(EligibleSeam {
                gi,
                owners: owners.clone(),
                chain,
                action,
            });
        }

        // ---- J1-0 (read-only, env-gated `YANG_441_J1_CENSUS`): the
        // boundary-exit junction census (spec §4-J1). A seam terminal that
        // overshoots the kept boundary leaves the true corner as a FOLD
        // vertex exactly on the junction segment — measure the corner's
        // local picture (holders, pinch multiplicity, incident curve
        // chains, surfaces) before any authority change is coded. Pass 0
        // only: the pre-apply state is the authoritative one, and later
        // passes would reprint the same declined seams. Covers ALL open
        // line-curve groups (already-minimal seams included), not just the
        // eligible set.
        if pass == 0 && std::env::var_os("YANG_441_J1_CENSUS").is_some() {
            for (gi, g) in groups.iter().enumerate() {
                if !matches!(g.curve, Curve::LineSegment) {
                    continue;
                }
                if let Ok((chain, false)) =
                    ordered_seam_side(&patches[g.owners[0]].cycles, &g.edges, Side::A)
                {
                    if chain.len() >= 2 {
                        census_j1_boundary_exit(
                            mesh,
                            &patches,
                            attribution,
                            intersection_curves,
                            gi,
                            &g.owners,
                            &chain,
                        );
                    }
                }
            }
        }

        // ---- I5-0 (read-only, env-gated `YANG_434_CENSUS`): §4.3.4
        // seam-polyline density census (spec §4-I5). The conic seam chains
        // are mesh-inherited density (I2b reorders, never inserts); measure
        // per seam how far the chain is from the paper's h/l/α acceptance
        // and what insertion count §4.3.4-as-written implies, BEFORE any
        // insert machinery is coded. Pass 0 only (the pre-apply state is
        // authoritative); covers ALL conic-curve groups — already-ordered,
        // minimal, and skipped seams included, not just the eligible set.
        if pass == 0 && std::env::var_os("YANG_434_CENSUS").is_some() {
            use crate::stage4_construct::census_conic_seam_density;
            for (gi, g) in groups.iter().enumerate() {
                if matches!(g.curve, Curve::LineSegment) {
                    continue;
                }
                let curve_desc = match &g.curve {
                    Curve::Circle { radius, .. } => format!("Circle(r={radius:.5})"),
                    Curve::Ellipse {
                        major_radius,
                        minor_radius,
                        ..
                    } => format!("Ellipse(a={major_radius:.5},b={minor_radius:.5})"),
                    c => format!("{c:?}"),
                };
                let (chain, closed) =
                    match ordered_seam_side(&patches[g.owners[0]].cycles, &g.edges, Side::A) {
                        Ok(x) => x,
                        Err(e) => {
                            eprintln!(
                                "[s434-census] seam={gi} owners={:?} {curve_desc}: \
                                 UNORDERABLE ({e:?})",
                                g.owners
                            );
                            continue;
                        }
                    };
                let ordered = match crate::stage4_splice::order_along_curve(
                    &g.curve,
                    &mesh.verts,
                    &chain,
                    closed,
                ) {
                    Ok(o) => o,
                    Err(e) => {
                        eprintln!(
                            "[s434-census] seam={gi} owners={:?} {curve_desc}: \
                             NO-CURVE-ORDER ({e:?}) n={}",
                            g.owners,
                            chain.len()
                        );
                        continue;
                    }
                };
                match census_conic_seam_density(&mesh.verts, &ordered, &g.curve, closed) {
                    Some(c) => eprintln!(
                        "[s434-census] seam={gi} owners={:?} {curve_desc} closed={closed} \
                         n={} pairs={} fail_h={} fail_l={} fail_alpha={} fail_any={} \
                         max_h={:.3e} max_l={:.3e} max_alpha_deg={:.2} dp={:.3e} \
                         implied_inserts={}{}",
                        g.owners,
                        ordered.len(),
                        c.pairs,
                        c.fail_h,
                        c.fail_l,
                        c.fail_alpha,
                        c.fail_any,
                        c.max_h,
                        c.max_l,
                        c.max_alpha.to_degrees(),
                        c.dp_max,
                        c.implied_inserts,
                        if c.capped { " CAPPED" } else { "" },
                    ),
                    None => eprintln!(
                        "[s434-census] seam={gi} owners={:?} {curve_desc}: \
                         NO-PARAM (degenerate projection or n<2, n={})",
                        g.owners,
                        ordered.len()
                    ),
                }
            }
        }

        // I1g corner unification now lives INSIDE the batch (substitution
        // + holder rebuild) — see the merge phase in the assembly loop.
        // The inc-3 mesh-space weld was measured negative (bare collapse
        // pinches; the 2026-08-05 pattern) and removed.

        // ---- Batch assembly: every removal is loud and re-assembles. --------
        // Bounded: each iteration either breaks with a batch or removes at
        // least one seam from `active`.
        let mut active: BTreeSet<usize> = (0..eligible.len()).collect();
        if let Some(budget) = seam_budget {
            let allowed = budget.saturating_sub(applied_total);
            while active.len() > allowed {
                let last = *active.last().expect("non-empty when over budget");
                active.remove(&last);
            }
        }
        let chain_interior_holds =
            |e: &EligibleSeam, v: u32| -> bool { e.chain[1..e.chain.len() - 1].contains(&v) };
        let rebuilds = 'assemble: loop {
            if active.is_empty() {
                break Vec::new();
            }
            // Group each patch's eligible chains (all owners of every seam).
            let mut chains_of: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
            for &ei in &active {
                let e = &eligible[ei];
                for &oi in &e.owners {
                    chains_of.entry(oi).or_default().push(ei);
                }
            }
            // Apply ALL of each patch's seam actions simultaneously: line
            // runs COLLAPSE to their endpoints; conic runs REORDER to curve
            // parameter (I2b). Actions on distinct seams touch disjoint runs
            // (shared junction endpoints at most), so sequential application
            // is order-independent.
            let mut mod_cycles: BTreeMap<usize, Vec<Vec<u32>>> = BTreeMap::new();
            for (&pi, eis) in &chains_of {
                let mut cur = patches[pi].cycles.clone();
                let mut failed: Option<usize> = None;
                for &ei in eis {
                    let e = &eligible[ei];
                    match &e.action {
                        SeamAction::CollapseLine => match replace_seam_run(&cur, &e.chain) {
                            Some(next) if next.iter().all(|c| c.len() >= 3) => cur = next,
                            _ => {
                                failed = Some(ei);
                                break;
                            }
                        },
                        SeamAction::ReorderConic { ordered } => {
                            match crate::stage4_splice::reorder_cycles_to_curve(
                                &cur,
                                ordered,
                                Side::A,
                            ) {
                                Ok(next) => cur = next,
                                Err(_) => {
                                    failed = Some(ei);
                                    break;
                                }
                            }
                        }
                        SeamAction::RefineConic { ordered, refined } => {
                            match crate::stage4_splice::splice_refined_run_into_cycles(
                                &cur,
                                ordered,
                                refined,
                                Side::A,
                            ) {
                                Ok(next) => cur = next,
                                Err(_) => {
                                    failed = Some(ei);
                                    break;
                                }
                            }
                        }
                    }
                }
                if let Some(ei) = failed {
                    skip[6] += 1;
                    c441_log!(
                        "[s4-construct] pass={pass} seam={}: DECLINED batch action \
                         in patch {pi} (mid-batch non-contiguity, degenerate cycle, \
                         or conic reorder refusal)",
                        eligible[ei].gi
                    );
                    active.remove(&ei);
                    continue 'assemble;
                }
                mod_cycles.insert(pi, cur);
            }
            // I1f — §4.4.1 NEAR-CURVE VERTEX REMOVAL (spec §3 step 2): drop
            // boundary vertices lying EXACTLY on a collapsed seam's segment
            // strictly between its junction endpoints (the F0067 walk-back
            // class: exact pp vertices inside the run whose edges are plain
            // mesh boundary, so the run collapse never swallows them and the
            // boundary folds back over them). Conformal by the same
            // both-owner rule as the collapse: a vertex is removed only if
            // EVERY patch holding it on a cycle is rebuilt in this batch
            // (and at most two hold it); otherwise it stays, loudly.
            {
                use crate::stage4_construct::on_segment_interior;
                let mut candidates: BTreeSet<u32> = BTreeSet::new();
                for &ei in &active {
                    let e = &eligible[ei];
                    // Line seams only: a conic seam's chord is NOT its curve —
                    // on-chord vertices of a circle run are legitimate
                    // geometry, never removal candidates.
                    if !matches!(e.action, SeamAction::CollapseLine) {
                        continue;
                    }
                    let (e0, e1) = (e.chain[0], *e.chain.last().expect("chain len >= 3"));
                    for (&pi, cycles) in &mod_cycles {
                        if !e.owners.contains(&pi) {
                            continue;
                        }
                        for cyc in cycles {
                            for &v in cyc {
                                if v != e0 && v != e1 && on_segment_interior(&mesh.verts, e0, e1, v)
                                {
                                    candidates.insert(v);
                                }
                            }
                        }
                    }
                }
                let mut removed_total = 0usize;
                let max_holders = if full_arrangement { 4 } else { 2 };
                for &v in &candidates {
                    // Every holder patch must be in the batch (≤2 holders on a
                    // post-keep submesh; ≤4 on the full arrangement), and no
                    // holder's cycle may degenerate below 3 vertices — the
                    // removal is all-holders-or-none (a one-sided removal
                    // would BE a T-junction).
                    let holders: Vec<usize> = patches
                        .iter()
                        .enumerate()
                        .filter(|&(pj, p)| {
                            let cycles = mod_cycles.get(&pj).unwrap_or(&p.cycles);
                            cycles.iter().any(|c| c.contains(&v))
                        })
                        .map(|(pj, _)| pj)
                        .collect();
                    if holders.len() > max_holders
                        || !holders.iter().all(|h| mod_cycles.contains_key(h))
                    {
                        c441_log!(
                            "[s4-construct] pass={pass}: NEAR-CURVE REMOVAL BLOCKED v{v} — \
                             holders {holders:?} not all rebuilt (or >{max_holders}); vertex stays"
                        );
                        continue;
                    }
                    let degenerates = holders.iter().any(|h| {
                        mod_cycles[h].iter().any(|c| {
                            let occ = c.iter().filter(|&&x| x == v).count();
                            occ > 0 && c.len() - occ < 3
                        })
                    });
                    if degenerates {
                        c441_log!(
                            "[s4-construct] pass={pass}: NEAR-CURVE REMOVAL BLOCKED v{v} — \
                             a holder cycle would degenerate below 3 vertices"
                        );
                        continue;
                    }
                    for h in &holders {
                        for cyc in mod_cycles.get_mut(h).expect("holder is batched") {
                            cyc.retain(|&x| x != v);
                        }
                    }
                    removed_total += 1;
                }
                if removed_total > 0 {
                    c441_log!(
                        "[s4-construct] pass={pass}: NEAR-CURVE REMOVED {removed_total} \
                         on-seam vertices across the batch"
                    );
                }
            }
            // Consecutive-duplicate cleanup after merge + removal (a merged
            // corner adjacent to a removed spur leaves q twice in a row;
            // wraparound included). A cycle degenerating below 3 vertices
            // declines that patch's seams, loudly.
            {
                let mut degenerate: Vec<usize> = Vec::new();
                for (&pi, cycles) in mod_cycles.iter_mut() {
                    for cyc in cycles.iter_mut() {
                        let mut out: Vec<u32> = Vec::with_capacity(cyc.len());
                        for &v in cyc.iter() {
                            if out.last() != Some(&v) {
                                out.push(v);
                            }
                        }
                        while out.len() >= 2 && out.first() == out.last() {
                            out.pop();
                        }
                        *cyc = out;
                        if cyc.len() < 3 {
                            degenerate.push(pi);
                        }
                    }
                }
                if let Some(&pi) = degenerate.first() {
                    skip[6] += 1;
                    // A patch with no attributable seam (the 2026-08-10
                    // 18k-restart livelock shape) refuses the whole batch.
                    let drop: Vec<usize> = active
                        .iter()
                        .copied()
                        .filter(|&ei| eligible[ei].owners.contains(&pi))
                        .collect();
                    if !drop.is_empty() {
                        c441_log!(
                            "[s4-construct] pass={pass}: DECLINED patch {pi} — cycle \
                             degenerated below 3 vertices after corner merge/removal; \
                             dropping its seams"
                        );
                        for ei in drop {
                            active.remove(&ei);
                        }
                        continue 'assemble;
                    }
                    eprintln!(
                        "[s4-construct] STOP pass={pass}: patch {pi} degenerated with \
                         no attributable seam or merge — refusing the whole batch"
                    );
                    break 'assemble Vec::new();
                }
            }
            // Dropped vertices: chain interiors + planar flood interiors.
            let batch_own: BTreeSet<u32> = mod_cycles
                .keys()
                .flat_map(|&pi| patches[pi].tris.iter().copied())
                .collect();
            let mut dropped_of: BTreeMap<usize, BTreeSet<u32>> = BTreeMap::new();
            for (&pi, cycles) in &mod_cycles {
                let kept: BTreeSet<u32> = cycles.iter().flatten().copied().collect();
                let mut dr = BTreeSet::new();
                for &t in &patches[pi].tris {
                    for &v in &mesh.tris[t as usize] {
                        if !kept.contains(&v) {
                            dr.insert(v);
                        }
                    }
                }
                dropped_of.insert(pi, dr);
            }
            // A vertex one batched patch drops but another batched patch KEEPS
            // on its modified cycles would be a T-junction between them.
            for (&pi, dr) in &dropped_of {
                for (&qi, cycles) in &mod_cycles {
                    if qi == pi {
                        continue;
                    }
                    if let Some(&v) = cycles.iter().flatten().find(|v| dr.contains(v)) {
                        if let Some(ei) = active
                            .iter()
                            .copied()
                            .find(|&ei| chain_interior_holds(&eligible[ei], v))
                        {
                            skip[6] += 1;
                            c441_log!(
                                "[s4-construct] pass={pass} seam={}: DECLINED — dropped \
                                 vertex {v} (patch {pi}) kept by batched patch {qi}",
                                eligible[ei].gi
                            );
                            active.remove(&ei);
                        } else {
                            skip[6] += 1;
                            c441_log!(
                                "[s4-construct] pass={pass}: DECLINED patch {pi} — interior \
                                 vertex {v} kept by batched patch {qi}; dropping its seams"
                            );
                            let drop: Vec<usize> = active
                                .iter()
                                .copied()
                                .filter(|&ei| eligible[ei].owners.contains(&pi))
                                .collect();
                            if drop.is_empty() {
                                eprintln!(
                                    "[s4-construct] STOP pass={pass}: patch {pi} conflict \
                                     with no attributable seam, merge, or trim — refusing \
                                     the whole batch"
                                );
                                break 'assemble Vec::new();
                            }
                            for ei in drop {
                                active.remove(&ei);
                            }
                        }
                        continue 'assemble;
                    }
                }
            }
            // A dropped vertex referenced by a triangle OUTSIDE the batch
            // would be orphaned into a T-junction.
            let mut owner_of_dropped: BTreeMap<u32, usize> = BTreeMap::new();
            for (&pi, dr) in &dropped_of {
                for &v in dr {
                    owner_of_dropped.entry(v).or_insert(pi);
                }
            }
            for (t, tri) in mesh.tris.iter().enumerate() {
                if batch_own.contains(&(t as u32)) {
                    continue;
                }
                if let Some((&v, &pi)) = tri.iter().find_map(|v| owner_of_dropped.get_key_value(v))
                {
                    if let Some(ei) = active
                        .iter()
                        .copied()
                        .find(|&ei| chain_interior_holds(&eligible[ei], v))
                    {
                        skip[6] += 1;
                        c441_log!(
                            "[s4-construct] pass={pass} seam={}: DECLINED — dropped vertex \
                             {v} referenced outside the batch (tri {t})",
                            eligible[ei].gi
                        );
                        active.remove(&ei);
                    } else {
                        skip[6] += 1;
                        c441_log!(
                            "[s4-construct] pass={pass}: DECLINED patch {pi} — interior \
                             vertex {v} referenced outside the batch (tri {t}); dropping \
                             its seams"
                        );
                        let drop: Vec<usize> = active
                            .iter()
                            .copied()
                            .filter(|&ei| eligible[ei].owners.contains(&pi))
                            .collect();
                        if drop.is_empty() {
                            eprintln!(
                                "[s4-construct] STOP pass={pass}: patch {pi} orphan \
                                 conflict with no attributable seam, merge, or trim — \
                                 refusing the whole batch"
                            );
                            break 'assemble Vec::new();
                        }
                        for ei in drop {
                            active.remove(&ei);
                        }
                    }
                    continue 'assemble;
                }
            }
            // Anchoring dump (env-gated, read-only): per active seam the chain
            // geometry and whether the direct edge pre-exists; per patch the
            // pre/post cycles. For localizing a gate-ON regression.
            if std::env::var_os("YANG_441_VERBOSE").is_some() {
                for &ei in &active {
                    let e = &eligible[ei];
                    let (e0, e1) = (e.chain[0], *e.chain.last().expect("chain len >= 3"));
                    let pre = mesh
                        .tris
                        .iter()
                        .filter(|t| {
                            (0..3).any(|k| {
                                let (a, b) = (t[k], t[(k + 1) % 3]);
                                (a == e0 && b == e1) || (a == e1 && b == e0)
                            })
                        })
                        .count();
                    eprintln!(
                        "[s4-verbose] seam={} owners={:?} chain={:?} direct-edge-pre-tris={pre}",
                        e.gi, e.owners, e.chain
                    );
                    for &v in &e.chain {
                        let p = mesh.verts[v as usize];
                        eprintln!(
                            "[s4-verbose]   v{v} = ({:.9}, {:.9}, {:.9})",
                            p.x(),
                            p.y(),
                            p.z()
                        );
                    }
                }
                for (&pi, cycles) in &mod_cycles {
                    let pre: Vec<usize> = patches[pi].cycles.iter().map(Vec::len).collect();
                    let post: Vec<usize> = cycles.iter().map(Vec::len).collect();
                    eprintln!(
                        "[s4-verbose] patch {pi}: cycles pre={pre:?} post={post:?} tris={}",
                        patches[pi].tris.len()
                    );
                    for (k, cyc) in patches[pi].cycles.iter().enumerate() {
                        eprintln!("[s4-verbose]   patch {pi} pre-cycle {k}: {cyc:?}");
                    }
                    for (k, cyc) in cycles.iter().enumerate() {
                        eprintln!("[s4-verbose]   patch {pi} post-cycle {k}: {cyc:?}");
                    }
                }
            }
            // Single-sided rebuilds. A patch that declines takes all its seams
            // out of the batch (its partners must not collapse one-sidedly).
            let mut out = Vec::with_capacity(mod_cycles.len());
            for (&pi, cycles) in &mod_cycles {
                let modp = SplicePatch {
                    cycles: cycles.clone(),
                    tris: patches[pi].tris.clone(),
                    surface: patches[pi].surface,
                };
                match crate::stage4_construct::rebuild_patch_planar_bounded(
                    mesh,
                    pi,
                    &modp,
                    crate::stage4_correct::stage4_chord_band(a, b),
                ) {
                    Ok(r) => out.push(r),
                    Err(e) => {
                        skip[6] += 1;
                        c441_log!("[s4-construct] pass={pass}: DECLINED patch {pi} — {e:?}");
                        // Decline census: name what the declined boundary is
                        // MADE OF and WHICH edges cross — the I2 worklist and
                        // the femto-pair anchor are measured here, not
                        // inferred later.
                        if let crate::stage4_construct::ConstructError::Cdt { ref error, .. } = e {
                            let collapsed: BTreeSet<(u32, u32)> = active
                                .iter()
                                .copied()
                                .filter(|&ei| eligible[ei].owners.contains(&pi))
                                .map(|ei| {
                                    let ch = &eligible[ei].chain;
                                    let (a, b) = (ch[0], *ch.last().expect("chain len >= 3"));
                                    (a.min(b), a.max(b))
                                })
                                .collect();
                            census_construct_decline(
                                mesh,
                                patches[pi].surface,
                                cycles,
                                error,
                                intersection_curves,
                                &collapsed,
                                pi,
                                relocations,
                            );
                        }
                        // A declining patch drops its own seams; with nothing
                        // attributable, refuse the whole batch.
                        let drop: Vec<usize> = active
                            .iter()
                            .copied()
                            .filter(|&ei| eligible[ei].owners.contains(&pi))
                            .collect();
                        if drop.is_empty() {
                            eprintln!(
                                "[s4-construct] STOP pass={pass}: patch {pi} declined \
                                 with no attributable seam or merge — refusing the \
                                 whole batch"
                            );
                            break 'assemble Vec::new();
                        }
                        for ei in drop {
                            active.remove(&ei);
                        }
                        continue 'assemble;
                    }
                }
            }
            break out;
        };

        if rebuilds.is_empty() {
            c441_log!(
                "[s4-construct] STOP pass={pass}: no collapsible seam remains \
                 (applied_total={applied_total}; seams={} skips: nonline={} curved={} \
                 closed={} minimal={} unorderable={} noncontig={} declined={} \
                 nonstraight={})",
                groups.len(),
                skip[0],
                skip[1],
                skip[2],
                skip[3],
                skip[4],
                skip[5],
                skip[6],
                skip[7],
            );
            break;
        }
        if !apply_enabled {
            c441_log!(
                "[s4-construct] pass={pass}: APPLY SKIPPED (census-only) — {} seams over \
                 {} patches",
                active.len(),
                rebuilds.len()
            );
            break;
        }
        // §4.4.1 batch postcondition (Phase 2b + F1-I3): a seam edit changes
        // all of its owner patches' boundaries, and the changes must cancel
        // inside the batch — on the post-keep submesh across the whole batch,
        // and on the full arrangement independently on each operand's shell
        // (`M_A` and `M_B`, reversing B's `[true, true]` opposite-normal
        // overlap sheets) so an A-hole can never cancel a B-hole (§4.4.3).
        {
            let n_verts = mesh.verts.len() as u32;
            let boundary_leaks = if full_arrangement {
                [crate::brep::InputId::A, crate::brep::InputId::B]
                    .into_iter()
                    .any(|op_id| {
                        let mut old_op: Vec<[u32; 3]> = Vec::new();
                        let mut new_op: Vec<[u32; 3]> = Vec::new();
                        for (ri, r) in rebuilds.iter().enumerate() {
                            for &t_idx in &r.old_tris {
                                let lbl = side_labels
                                    .labels
                                    .get(t_idx as usize)
                                    .copied()
                                    .unwrap_or_default();
                                if lbl.in_operand(op_id) {
                                    let mut tri = mesh.tris[t_idx as usize];
                                    if lbl.operand_winding_reversed(op_id) {
                                        tri.swap(1, 2);
                                    }
                                    old_op.push(tri);
                                }
                            }
                            let lbl = r
                                .old_tris
                                .first()
                                .and_then(|&t| side_labels.labels.get(t as usize).copied())
                                .unwrap_or_default();
                            if lbl.in_operand(op_id) {
                                let base = n_verts + (ri as u32) * 1_000_000;
                                for t in &r.new_tris {
                                    let mut tri = t.map(|v| {
                                        if v >= r.plan_verts {
                                            base + (v - r.plan_verts)
                                        } else {
                                            v
                                        }
                                    });
                                    if lbl.operand_winding_reversed(op_id) {
                                        tri.swap(1, 2);
                                    }
                                    new_op.push(tri);
                                }
                            }
                        }
                        crate::stage4_construct::directed_boundary(&old_op)
                            != crate::stage4_construct::directed_boundary(&new_op)
                    })
            } else {
                let old_all: Vec<[u32; 3]> = rebuilds
                    .iter()
                    .flat_map(|r| r.old_tris.iter().map(|&t| mesh.tris[t as usize]))
                    .collect();
                let new_all: Vec<[u32; 3]> = rebuilds
                    .iter()
                    .enumerate()
                    .flat_map(|(ri, r)| {
                        // Seed vertices get batch-unique ids past the mesh so
                        // two rebuilds' seeds never alias.
                        let base = n_verts + (ri as u32) * 1_000_000;
                        r.new_tris.iter().map(move |t| {
                            t.map(|v| {
                                if v >= r.plan_verts {
                                    base + (v - r.plan_verts)
                                } else {
                                    v
                                }
                            })
                        })
                    })
                    .collect();
                crate::stage4_construct::directed_boundary(&old_all)
                    != crate::stage4_construct::directed_boundary(&new_all)
            };
            if boundary_leaks {
                c441_log!(
                    "[s4-construct] STOP pass={pass}: batch of {} rebuilds would change \
                     its outer boundary — not applied",
                    rebuilds.len()
                );
                break;
            }
        }
        match apply_rebuild_batch(mesh, attribution, side_labels, &rebuilds) {
            Ok(()) => {
                for &ei in &active {
                    let e = &eligible[ei];
                    match &e.action {
                        SeamAction::CollapseLine => c441_log!(
                            "[s4-construct] pass={pass} seam={}: APPLIED patches {:?} — \
                             chain {} -> 2 verts",
                            e.gi,
                            e.owners,
                            e.chain.len()
                        ),
                        SeamAction::ReorderConic { ordered } => c441_log!(
                            "[s4-construct] pass={pass} seam={}: REORDERED patches {:?} — \
                             {} verts to curve order",
                            e.gi,
                            e.owners,
                            ordered.len()
                        ),
                        SeamAction::RefineConic { ordered, refined } => c441_log!(
                            "[s4-construct] pass={pass} seam={}: REFINED patches {:?} — \
                             chain {} -> {} (§4.3.4 density)",
                            e.gi,
                            e.owners,
                            ordered.len(),
                            refined.len()
                        ),
                    }
                }
                c441_log!(
                    "[s4-construct] pass={pass}: BATCH APPLIED — {} seams over {} patches",
                    active.len(),
                    rebuilds.len()
                );
                applied_total += active.len();
            }
            Err(e) => {
                eprintln!("[s4-construct] STOP pass={pass}: WRITE-BACK REFUSED {e:?}");
                break;
            }
        }

        let remap = compact_unreferenced_verts(mesh, relocations, &side_labels.labels);
        if let (Some(tv), Some(map)) = (tangential_verts.as_deref_mut(), remap.as_ref()) {
            crate::stage4_correct::remap_vert_set(tv, map);
        }
        let (i2, inc2, cv2) = crate::stage4_correct::compute_phase_a_with_side_labels(
            mesh,
            attribution,
            full_arrangement.then_some(&*side_labels),
            a,
            b,
            &crate::stage3_ssi::NO_EDGE_PROVENANCE,
        )?;
        probe_remap_pre_pos("construct", remap.as_ref());
        crate::stage4_correct::coincident_vertex_census("s5-construct", mesh);
        probe_record_incidence(&inc2, &cv2);
        *infos = i2;
        *intersection_curves = cv2;
        // I5-1: everything at or below this index is now either referenced
        // by an applied rebuild or compacted away; re-snapshot the orphan
        // floor for the next pass.
        verts_floor = mesh.verts.len();
    }
    // I5-1: a refined seam whose batch never applied (declined mid-assembly,
    // census-only, write-back refusal, or the no-collapsible STOP) leaves
    // its appended on-curve vertices as a contiguous unreferenced tail
    // above the floor — drop them so no orphan vertex outlives the pass.
    // A truncation, not a compact: indices at or below the floor are
    // untouched, so `infos`/`intersection_curves` stay valid.
    if mesh.verts.len() > verts_floor {
        c441_log!(
            "[s4-construct] I5 cleanup: dropping {} unapplied on-curve inserts",
            mesh.verts.len() - verts_floor
        );
        mesh.verts.truncate(verts_floor);
    }
    Ok(applied_total)
}

/// J1-0 (spec §4-J1): read-only boundary-exit junction census for ONE open
/// line seam, env-gated by the caller on `YANG_441_J1_CENSUS`.
///
/// A seam terminal that OVERSHOOTS the kept boundary leaves the true corner
/// as a FOLD vertex lying exactly on the open segment between the seam's
/// junction endpoints (the F0067 TF-8 signature; Fig-11(a) computes q ON
/// the kept boundary instead). Before any authority change is coded, this
/// measures the corner's full local picture per seam:
/// - each fold vertex: segment parameter, position, holder patches
///   (surface + attribution) with per-holder cycle occurrence counts;
/// - each junction endpoint: holders, occurrence multiplicity (a boundary
///   PINCH appears twice in one cycle), and cycle windows with edge
///   classes (curve type / plain);
/// - every intersection-curve edge incident to the junction and fold
///   vertices (rim-chain identity, cap-seam fragment existence).
pub(super) fn census_j1_boundary_exit(
    mesh: &Mesh,
    patches: &[crate::stage4_splice::SplicePatch],
    attribution: &TriangleAttributionMap,
    intersection_curves: &std::collections::BTreeMap<(u32, u32), Curve>,
    gi: usize,
    owners: &[usize],
    chain: &[u32],
) {
    use crate::stage4_construct::on_segment_interior;
    use std::collections::BTreeSet;

    let (e0, e1) = (chain[0], *chain.last().expect("chain is non-empty"));
    let chain_set: BTreeSet<u32> = chain.iter().copied().collect();
    let mut cands: Vec<u32> = Vec::new();
    for &pi in owners {
        for cyc in &patches[pi].cycles {
            for &v in cyc {
                if !chain_set.contains(&v)
                    && !cands.contains(&v)
                    && on_segment_interior(&mesh.verts, e0, e1, v)
                {
                    cands.push(v);
                }
            }
        }
    }
    if cands.is_empty() {
        return;
    }

    let pos = |v: u32| mesh.verts[v as usize];
    let pstr = |v: u32| {
        let p = pos(v);
        format!("({:.9}, {:.9}, {:.9})", p.x(), p.y(), p.z())
    };
    let seg_t = |v: u32| -> f64 {
        let (a, b, x) = (pos(e0), pos(e1), pos(v));
        let d = [b.x() - a.x(), b.y() - a.y(), b.z() - a.z()];
        let r = [x.x() - a.x(), x.y() - a.y(), x.z() - a.z()];
        let len2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
        if len2 == 0.0 {
            f64::NAN
        } else {
            (r[0] * d[0] + r[1] * d[1] + r[2] * d[2]) / len2
        }
    };
    let surf = |s: &Surface| -> String {
        match *s {
            Surface::Plane { normal, d } => format!(
                "Plane n=({:.6},{:.6},{:.6}) d={d:.9}",
                normal.x(),
                normal.y(),
                normal.z()
            ),
            Surface::Cylinder {
                axis_point,
                axis_dir,
                radius,
            } => format!(
                "Cyl r={radius:.9} p=({:.6},{:.6},{:.6}) d=({:.6},{:.6},{:.6})",
                axis_point.x(),
                axis_point.y(),
                axis_point.z(),
                axis_dir.x(),
                axis_dir.y(),
                axis_dir.z()
            ),
            ref other => format!("{other:?}"),
        }
    };
    let desc = |pj: usize| -> String {
        let p = &patches[pj];
        let mut it = p.tris.iter().map(|&t| attribution.attributions[t as usize]);
        let attr = match it.next().flatten() {
            Some(f)
                if p.tris
                    .iter()
                    .all(|&t| attribution.attributions[t as usize] == Some(f)) =>
            {
                format!("{:?}f{}", f.input, f.face)
            }
            _ => "mixed".into(),
        };
        format!("p{pj}<{attr} {}>", surf(&p.surface))
    };
    let holders = |v: u32| -> Vec<(usize, usize)> {
        patches
            .iter()
            .enumerate()
            .filter_map(|(pj, p)| {
                let occ: usize = p
                    .cycles
                    .iter()
                    .map(|c| c.iter().filter(|&&x| x == v).count())
                    .sum();
                (occ > 0).then_some((pj, occ))
            })
            .collect()
    };
    let eclass = |a: u32, b: u32| -> String {
        match intersection_curves.get(&(a.min(b), a.max(b))) {
            None => "plain".into(),
            Some(Curve::LineSegment) => "line".into(),
            Some(Curve::Circle { radius, .. }) => format!("circle(r={radius:.9})"),
            Some(Curve::Ellipse { .. }) => "ellipse".into(),
            Some(Curve::Parabola { .. }) => "parabola".into(),
            Some(Curve::Hyperbola { .. }) => "hyperbola".into(),
            Some(Curve::SurfacePair { .. }) => "surface-pair".into(),
        }
    };

    c441_log!("[j1-census] seam={gi} owners={owners:?} chain={chain:?}");
    for &oi in owners {
        c441_log!("[j1-census]   owner {}", desc(oi));
    }
    c441_log!(
        "[j1-census]   e0 v{e0} {}   e1 v{e1} {}",
        pstr(e0),
        pstr(e1)
    );
    for &c in &cands {
        let hs: Vec<String> = holders(c)
            .iter()
            .map(|&(pj, occ)| format!("{}x{occ}", desc(pj)))
            .collect();
        c441_log!(
            "[j1-census]   FOLD v{c} t={:.6} {} holders: {}",
            seg_t(c),
            pstr(c),
            hs.join(" | ")
        );
    }
    for &j in &[e0, e1] {
        for (pj, occ) in holders(j) {
            c441_log!("[j1-census]   END v{j} in {} occ={occ}", desc(pj));
            for cyc in &patches[pj].cycles {
                let n = cyc.len();
                for i in 0..n {
                    if cyc[i] != j {
                        continue;
                    }
                    let mut s = String::new();
                    for k in 0..5usize {
                        let vi = cyc[(i + 2 * n + k - 2) % n];
                        if k > 0 {
                            let prev = cyc[(i + 2 * n + k - 3) % n];
                            s.push_str(&format!(" -{}- ", eclass(prev, vi)));
                        }
                        s.push_str(&format!("v{vi}"));
                    }
                    c441_log!("[j1-census]     cycle-window {s}");
                }
            }
        }
    }
    for &v in [e0, e1].iter().chain(cands.iter()) {
        for &(a, b) in intersection_curves.keys() {
            if a != v && b != v {
                continue;
            }
            let o = if a == v { b } else { a };
            let (p1, p2) = (pos(v), pos(o));
            let d =
                ((p1.x() - p2.x()).powi(2) + (p1.y() - p2.y()).powi(2) + (p1.z() - p2.z()).powi(2))
                    .sqrt();
            c441_log!(
                "[j1-census]   CURVE-EDGE v{v} -{}- v{o} len={d:.3e} other={}",
                eclass(a, b),
                pstr(o)
            );
        }
    }
}

/// Diagnostic census for a patch the I1b construct pass DECLINED at CDT:
/// per-cycle edge composition (collapsed-seam / line-seam / curved-seam /
/// plain), the crossing edge pairs for `TriangulationFailed` (brute-force
/// UV segment test — declined cycles are small), and the coincident chart
/// pair for `DuplicateVertex`. Read-only; f64 signs are diagnostic labels,
/// not gates.
#[allow(clippy::too_many_arguments)]
pub(super) fn census_construct_decline(
    mesh: &Mesh,
    surface: Surface,
    cycles: &[Vec<u32>],
    error: &cherchi_rs::CdtError,
    intersection_curves: &std::collections::BTreeMap<(u32, u32), Curve>,
    collapsed: &std::collections::BTreeSet<(u32, u32)>,
    pi: usize,
    relocations: &[(u32, f64)],
) {
    use std::collections::BTreeMap;
    let reloc_of = |v: u32| -> Option<f64> {
        relocations
            .iter()
            .find(|&&(rv, _)| rv == v)
            .map(|&(_, d)| d)
    };

    let edge_tag = |s: u32, t: u32| -> &'static str {
        let key = (s.min(t), s.max(t));
        if collapsed.contains(&key) {
            "collapsed-seam"
        } else {
            match intersection_curves.get(&key) {
                Some(Curve::LineSegment) => "line-seam",
                Some(_) => "curved-seam",
                None => "plain",
            }
        }
    };
    for (k, cyc) in cycles.iter().enumerate() {
        let n = cyc.len();
        let mut counts: BTreeMap<&'static str, usize> = BTreeMap::new();
        for i in 0..n {
            *counts
                .entry(edge_tag(cyc[i], cyc[(i + 1) % n]))
                .or_default() += 1;
        }
        let fmt: Vec<String> = counts.iter().map(|(t, c)| format!("{t}={c}")).collect();
        c441_log!(
            "[s4-construct]   patch {pi} cycle {k}: {n} edges — {}",
            fmt.join(" ")
        );
        if n <= 24 {
            for (i, &v) in cyc.iter().enumerate() {
                let p = mesh.verts[v as usize];
                let moved = match reloc_of(v) {
                    Some(d) => format!("relocated d={d:.3e}"),
                    None => "unmoved".to_string(),
                };
                c441_log!(
                    "[s4-construct]     cyc[{i}] v{v} ({:.12}, {:.12}, {:.12}) [{moved}] --[{}]--",
                    p.x(),
                    p.y(),
                    p.z(),
                    edge_tag(v, cyc[(i + 1) % n])
                );
            }
        }
    }

    // Rim-weave census (spec §4 next increment; env-gated
    // `YANG_441_RIM_CENSUS`): for a declining cycle that carries circle
    // curve edges, print every vertex with its SIGNED radial delta to the
    // cycle's trim circle (+ outside, − inside), chain membership, and
    // relocation identity. The 2026-08-02 CDT-ring anchor says the cycle
    // visits both sides of its own trim circle (chord-sliver content the
    // exact trim removes); this measures the sliver runs the trim must
    // drop and the junctions it must keep, per cycle, in the current
    // (post-J1) state.
    if std::env::var_os("YANG_441_RIM_CENSUS").is_some() {
        for (k, cyc) in cycles.iter().enumerate() {
            let n = cyc.len();
            let mut circles: Vec<(cad_primitives::Point3, cad_primitives::Vector3, f64)> =
                Vec::new();
            for i in 0..n {
                let (s, t) = (cyc[i], cyc[(i + 1) % n]);
                let key = (s.min(t), s.max(t));
                if let Some(Curve::Circle {
                    center,
                    normal,
                    radius,
                }) = intersection_curves.get(&key)
                {
                    if !circles
                        .iter()
                        .any(|&(c, _, r)| c.as_array() == center.as_array() && r == *radius)
                    {
                        circles.push((*center, *normal, *radius));
                    }
                }
            }
            if circles.is_empty() {
                continue;
            }
            for (ci, &(c, nrm, r)) in circles.iter().enumerate() {
                c441_log!(
                    "[rim-census] patch {pi} cycle {k} circle[{ci}] \
                     c=({:.9},{:.9},{:.9}) n=({:.3},{:.3},{:.3}) r={r:.9}",
                    c.x(),
                    c.y(),
                    c.z(),
                    nrm.x(),
                    nrm.y(),
                    nrm.z()
                );
            }
            let (c, nrm, r) = circles[0];
            let on_chain = |i: usize| -> bool {
                let prev = cyc[(i + n - 1) % n];
                let (v, next) = (cyc[i], cyc[(i + 1) % n]);
                let is_circ = |a: u32, b: u32| {
                    matches!(
                        intersection_curves.get(&(a.min(b), a.max(b))),
                        Some(Curve::Circle { .. })
                    )
                };
                is_circ(prev, v) || is_circ(v, next)
            };
            for (i, &v) in cyc.iter().enumerate() {
                let p = mesh.verts[v as usize];
                let d = [p.x() - c.x(), p.y() - c.y(), p.z() - c.z()];
                let along = d[0] * nrm.x() + d[1] * nrm.y() + d[2] * nrm.z();
                let radial = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2] - along * along).sqrt();
                let dr = radial - r;
                let moved = match reloc_of(v) {
                    Some(t) => format!("reloc t={t:.4}"),
                    None => "unmoved".to_string(),
                };
                c441_log!(
                    "[rim-census]   [{i}] v{v} dr={dr:+.3e} h={along:+.3e} \
                     {} [{moved}] --[{}]--",
                    if on_chain(i) { "CHAIN" } else { "plain" },
                    edge_tag(v, cyc[(i + 1) % n])
                );
            }
        }
    }

    let Some(chart) = crate::stage4_project::SurfaceChart::new(surface) else {
        return;
    };
    match error {
        cherchi_rs::CdtError::TriangulationFailed => {
            // Proper-crossing scan between non-adjacent boundary edges.
            let cross = |p: cad_primitives::Point2,
                         q: cad_primitives::Point2,
                         r: cad_primitives::Point2,
                         s: cad_primitives::Point2|
             -> bool {
                let orient = |a: cad_primitives::Point2,
                              b: cad_primitives::Point2,
                              c: cad_primitives::Point2|
                 -> f64 {
                    (b.x() - a.x()) * (c.y() - a.y()) - (b.y() - a.y()) * (c.x() - a.x())
                };
                let (o1, o2) = (orient(p, q, r), orient(p, q, s));
                let (o3, o4) = (orient(r, s, p), orient(r, s, q));
                o1 * o2 < 0.0 && o3 * o4 < 0.0
            };
            for cyc in cycles {
                let n = cyc.len();
                let uv: Vec<cad_primitives::Point2> = cyc
                    .iter()
                    .map(|&v| chart.project(mesh.verts[v as usize]))
                    .collect();
                for i in 0..n {
                    for j in (i + 1)..n {
                        let (a0, a1) = (cyc[i], cyc[(i + 1) % n]);
                        let (b0, b1) = (cyc[j], cyc[(j + 1) % n]);
                        if a0 == b0 || a0 == b1 || a1 == b0 || a1 == b1 {
                            continue; // shared endpoint — not a proper crossing
                        }
                        if cross(uv[i], uv[(i + 1) % n], uv[j], uv[(j + 1) % n]) {
                            c441_log!(
                                "[s4-construct]   patch {pi} CROSSING ({a0},{a1})[{}] x \
                                 ({b0},{b1})[{}]",
                                edge_tag(a0, a1),
                                edge_tag(b0, b1)
                            );
                            for v in [a0, a1, b0, b1] {
                                let p = mesh.verts[v as usize];
                                c441_log!(
                                    "[s4-construct]     x-vert v{v} = ({:.12}, {:.12}, {:.12})",
                                    p.x(),
                                    p.y(),
                                    p.z()
                                );
                            }
                        }
                    }
                }
            }
        }
        cherchi_rs::CdtError::DuplicateVertex => {
            let mut seen: BTreeMap<[u64; 2], u32> = BTreeMap::new();
            let mut found = 0usize;
            for &v in cycles.iter().flatten() {
                let uv = chart.project(mesh.verts[v as usize]);
                let key = [uv.x().to_bits(), uv.y().to_bits()];
                match seen.get(&key) {
                    Some(&w) if w != v => {
                        found += 1;
                        let (pw, pv) = (mesh.verts[w as usize], mesh.verts[v as usize]);
                        let d = ((pw.x() - pv.x()).powi(2)
                            + (pw.y() - pv.y()).powi(2)
                            + (pw.z() - pv.z()).powi(2))
                        .sqrt();
                        c441_log!(
                            "[s4-construct]   patch {pi} FEMTO-PAIR verts {w}+{v} — \
                             3D dist {d:.3e}"
                        );
                    }
                    _ => {
                        seen.insert(key, v);
                    }
                }
            }
            if found == 0 {
                c441_log!(
                    "[s4-construct]   patch {pi}: no bit-identical chart pair \
                     (spade-level merge)"
                );
            }
        }
        _ => {}
    }
}
