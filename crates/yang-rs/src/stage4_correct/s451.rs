#[allow(clippy::wildcard_imports)]
use super::*;

/// §4.5.1 inc-0 (spec `specs/yang_451_optimize_across_boundaries.md`): the
/// shared constructor for the relocation sweep's `OffCurveBeyondChordBand`
/// STOPs — behaviour identical to constructing the error inline. Under
/// `YANG_451=census` it additionally prints the firing SITE (`#[track_caller]`,
/// so sites don't lie — the `YANG_LRR_PROBE` pattern) and the vertex's
/// incident edge-level curve assignments, the data that decides the spec's
/// Q1: at a curve-graph junction, can the branches be paired by the vertex's
/// own assigned conic?
#[track_caller]
pub(crate) fn offcurve_beyond_chord_band(
    v: u32,
    curves0: &std::collections::BTreeMap<(u32, u32), Curve>,
    inc0: &std::collections::BTreeMap<(u32, u32), Vec<(InputId, Surface)>>,
) -> YangError {
    if std::env::var("YANG_451").as_deref() == Ok("census") {
        let loc = std::panic::Location::caller();
        eprintln!("YANG_451_SITE v{v} at {}:{}", loc.file(), loc.line());
        for (&(s, e), curve) in curves0.iter() {
            if s == v || e == v {
                eprintln!("YANG_451_EDGE v{v} edge=({s},{e}) curve={curve:?}");
            }
        }
        for (&(s, e), entries) in inc0.iter() {
            if s == v || e == v {
                eprintln!("YANG_451_INC v{v} edge=({s},{e}) surfs={entries:?}");
            }
        }
        // Own-curve CHAIN (spec Q1's other half): follow edges assigned the
        // SAME analytic curve VALUE outward from `v`, both directions. This
        // supplies the region's topology; which chain vertices are CONVERGED
        // is adjudicated against the I12 walk's log at the outer STOP catch.
        // `LineSegment` is a unit variant (every segment compares equal), so
        // it cannot key a chain and is skipped — the §4.5 population is conic.
        let mut own_curves: Vec<Curve> = Vec::new();
        for (&(s, e), &c) in curves0.iter() {
            if (s == v || e == v) && c != Curve::LineSegment && !own_curves.contains(&c) {
                own_curves.push(c);
            }
        }
        for curve in &own_curves {
            let nbrs_on = |w: u32| -> Vec<u32> {
                curves0
                    .iter()
                    .filter(|(&(s, e), c)| (s == w || e == w) && *c == curve)
                    .map(|(&(s, e), _)| if s == w { e } else { s })
                    .collect()
            };
            for start in nbrs_on(v) {
                let mut chain = vec![v, start];
                let (mut prev, mut cur) = (v, start);
                let end: &str = loop {
                    if chain.len() > 32 {
                        break "cap";
                    }
                    let next: Vec<u32> = nbrs_on(cur).into_iter().filter(|&x| x != prev).collect();
                    match next.len() {
                        0 => break "chain ends (no continuing same-curve edge)",
                        1 => {
                            prev = cur;
                            cur = next[0];
                            chain.push(cur);
                        }
                        _ => break "own-curve BRANCHES",
                    }
                };
                eprintln!("YANG_451_CHAIN v{v} via v{start}: {chain:?} end={end} curve={curve:?}");
            }
        }
    }
    YangError::stage4_region_invalid(v, Stage4InvalidReason::OffCurveBeyondChordBand)
}

/// §4.5.1 inc-1 census (spec `specs/yang_451_optimize_across_boundaries.md`
/// §7): the record-and-continue valve on the `OffCurveBeyondChordBand` gates.
///
/// Gate OFF (`YANG_451` unset/`0`): returns `Some(err)` and the caller aborts
/// exactly as today — byte-identical. `YANG_451=census`: RECORDS the failure
/// and returns `None`, and the caller SKIPS the vertex's relocation (the
/// paper's "cannot converge" state — the point keeps its mesh position) so
/// the sweep completes and the post-sweep selector measures at the paper's
/// own vantage (§4.5 `:652-656`, failures collected AFTER optimization —
/// inc-0 measured WHY: a mid-sweep refusal cannot see a bound the sweep has
/// not reached). The stage still cannot complete: after the census the FIRST
/// recorded error returns unchanged (P10).
#[track_caller]
pub(crate) fn s451_stop(
    census: bool,
    failures: &mut Vec<(u32, YangError)>,
    v: u32,
    curves0: &std::collections::BTreeMap<(u32, u32), Curve>,
    inc0: &std::collections::BTreeMap<(u32, u32), Vec<(InputId, Surface)>>,
) -> Option<YangError> {
    let err = offcurve_beyond_chord_band(v, curves0, inc0);
    if census {
        failures.push((v, err));
        None
    } else {
        Some(err)
    }
}

/// §4.5 trigger T1 for COLLECTED failures (spec `yang_452_cp_region.md` §3):
/// one site per distinct failed vertex, naming the faces its C_p span
/// traverses — the run "cannot converge to a distance of 0" from the first
/// failure to the nearest successfully optimized points either way. The
/// vantage is the census's: every non-failed vertex is relocated, so `good`
/// is `vertex_converged` minus the failed set minus §4-I9's domain-endpoint
/// crossers. The STOP returned is unchanged (the first error).
pub(crate) fn record_collected_spans(
    mesh: &Mesh,
    attribution: &[Option<TriangleAttribution>],
    a: &BRep,
    b: &BRep,
    entry: &[[f64; 3]],
    failures: &[(u32, YangError)],
) {
    let patches = build_patch_map(mesh, attribution);
    let adj = build_live_adjacency(mesh);
    let graph = IsectGraph::new(mesh, attribution);
    let failed: std::collections::BTreeSet<u32> = failures.iter().map(|&(v, _)| v).collect();
    let good = |w: u32| -> bool {
        !failed.contains(&w)
            && vertex_converged(mesh, &patches, a, b, w)
            && !vertex_crossed_domain_endpoint(mesh, attribution, a, b, &adj, entry, w)
    };
    let mut seen: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
    for (v, err) in failures {
        if !seen.insert(*v) || (*v as usize) >= mesh.verts.len() {
            continue;
        }
        let (members, bounds, capped) = graph.cp_span(*v, &good);
        if std::env::var_os("YANG_452_PROBE").is_some() {
            eprintln!(
                "[cp-span] T1 v{v} members={} bounds={:?}{} faces at v {} → span {}",
                members.len(),
                bounds.iter().take(4).collect::<Vec<_>>(),
                if capped { " capped" } else { "" },
                patches.get(v).map_or(0, |f| f.len()),
                cp_span_faces(&patches, &members, &bounds).len()
            );
        }
        crate::boolean::failures::record(crate::boolean::failures::FailureSite {
            trigger: crate::boolean::failures::FailureTrigger::RelocationDiverged,
            faces: cp_span_faces(&patches, &members, &bounds),
            detail: format!(
                "v{v} C_p span: {} failing point(s), bounds {:?}{} — {err}",
                members.len(),
                bounds.iter().take(6).collect::<Vec<_>>(),
                if capped { " (capped)" } else { "" }
            ),
        });
    }
}

/// §4.5.1 inc-1: the post-sweep selector census at the paper's vantage —
/// every non-failed vertex is relocated, so "successfully optimized" is
/// finally well-posed. Per recorded failure (deduped by vertex, sweep
/// order): clause 1 (Fig-13 carrier reading), I12's all-curve clause-2 walk
/// (`failed` = the recorded set), and the OWN-CURVE chain with per-vertex
/// convergence — the region's true extent and its bounds, the §4.5.1/§4.5.2
/// verdict data. Census-only prints; the caller returns the first error
/// after this regardless.
#[allow(clippy::too_many_arguments)]
pub(crate) fn s451_post_sweep_census(
    mesh: &Mesh,
    attribution: &[Option<TriangleAttribution>],
    a: &BRep,
    b: &BRep,
    curves0: &std::collections::BTreeMap<(u32, u32), Curve>,
    entry: &[[f64; 3]],
    failures: &[(u32, YangError)],
) {
    let patches = build_patch_map(mesh, attribution);
    let adj = build_live_adjacency(mesh);
    let failed: std::collections::BTreeSet<u32> = failures.iter().map(|&(v, _)| v).collect();
    let on_curve = |w: u32| vertex_on_curve(&patches, w);
    let good = |w: u32| -> bool {
        !failed.contains(&w)
            && vertex_converged(mesh, &patches, a, b, w)
            && !vertex_crossed_domain_endpoint(mesh, attribution, a, b, &adj, entry, w)
    };
    eprintln!(
        "YANG_451_POSTSWEEP failures={} distinct={}",
        failures.len(),
        failed.len()
    );
    let mut seen: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
    // Repair previews are per REGION, not per member — dedup by bound pair.
    let mut previewed: std::collections::BTreeSet<(u32, u32)> = std::collections::BTreeSet::new();
    for &(v, _) in failures {
        if !seen.insert(v) {
            continue;
        }
        let pos = mesh.verts[v as usize].as_array();
        let (ca, cb) = carrier_counts(&patches, a, b, v, pos);
        let near = ca.max(cb);
        let class = match near {
            0 => "unlocated",
            1 => "INTERIOR",
            _ => "BOUNDARY",
        };
        eprintln!("YANG_451_POSTSWEEP v{v} carrier=(A{ca},B{cb}) clause1={class}");
        let all_curve_data =
            selector_clause2_walk(mesh, attribution, a, b, &adj, v, &on_curve, &good);
        let all_curve = all_curve_data.is_some();
        // OWN-CURVE region: follow edges assigned the SAME analytic curve
        // VALUE from `v`, each direction, until a `good` vertex (the bound)
        // or the chain gives out. `LineSegment` is a unit variant (every
        // segment equal) and cannot key a chain — skipped.
        let mut own_curves: Vec<Curve> = Vec::new();
        for (&(s, e), &c) in curves0.iter() {
            if (s == v || e == v) && c != Curve::LineSegment && !own_curves.contains(&c) {
                own_curves.push(c);
            }
        }
        let mut bounds_found = 0usize;
        let mut directions = 0usize;
        let mut dir_bounds: Vec<Option<u32>> = Vec::new();
        for curve in &own_curves {
            let nbrs_on = |w: u32| -> Vec<u32> {
                curves0
                    .iter()
                    .filter(|(&(s, e), c)| (s == w || e == w) && *c == curve)
                    .map(|(&(s, e), _)| if s == w { e } else { s })
                    .collect()
            };
            for start in nbrs_on(v) {
                directions += 1;
                let mut this_bound: Option<u32> = None;
                let mut region: Vec<u32> = vec![v];
                let (mut prev, mut cur) = (v, start);
                let outcome: String = loop {
                    if good(cur) {
                        bounds_found += 1;
                        this_bound = Some(cur);
                        break format!("bound v{cur} (converged)");
                    }
                    region.push(cur);
                    if region.len() > 128 {
                        break "cap(128)".to_string();
                    }
                    let next: Vec<u32> = nbrs_on(cur).into_iter().filter(|&x| x != prev).collect();
                    match next.len() {
                        0 => break "chain ends".to_string(),
                        1 => {
                            prev = cur;
                            cur = next[0];
                        }
                        _ => break "own-curve branches".to_string(),
                    }
                };
                eprintln!(
                    "YANG_451_REGION v{v} via v{start}: len={} members={:?} outcome={outcome}",
                    region.len(),
                    &region[..region.len().min(24)]
                );
                dir_bounds.push(this_bound);
            }
        }
        // §4.5.1 inc-2a repair PREVIEW (census-only, spec §9): the data that
        // decides the repair VARIANT per region. The paper replaces the region
        // with the midpoint of the bounds and re-optimizes; whether that
        // optimization stays on ONE surface pair (a DRIFT region — simple
        // projection) or must cross a patch boundary (a STRADDLE region —
        // Fig-12's full cross-boundary mechanism) is readable from the bounds'
        // carried far-operand surfaces. For a shared cone+plane pair the
        // simple repair's outcome is computable outright: project the midpoint
        // onto the section and check the shared certificate on both surfaces —
        // no repair code involved.
        if let [Some(w0), Some(w1)] = dir_bounds[..] {
            let key = (w0.min(w1), w0.max(w1));
            if previewed.insert(key) {
                let p0 = mesh.verts[w0 as usize].as_array();
                let p1 = mesh.verts[w1 as usize].as_array();
                let (sa0, sb0) = carrier_surface_sets(&patches, a, b, w0, p0);
                let (sa1, sb1) = carrier_surface_sets(&patches, a, b, w1, p1);
                // The FAR operand is the one the failed traveller is OFF
                // (count 0). A hull-check failure can be within band on both
                // operands; B is taken as far there, and both sets print.
                let (far0, far1) = if ca == 0 { (&sa0, &sa1) } else { (&sb0, &sb1) };
                let shared_far: Vec<Surface> =
                    far0.iter().filter(|s| far1.contains(s)).copied().collect();
                let all_shared: Vec<Surface> = sa0
                    .iter()
                    .chain(sb0.iter())
                    .filter(|s| sa1.contains(s) || sb1.contains(s))
                    .copied()
                    .collect();
                let mid = [
                    (p0[0] + p1[0]) * 0.5,
                    (p0[1] + p1[1]) * 0.5,
                    (p0[2] + p1[2]) * 0.5,
                ];
                eprintln!(
                    "YANG_451_PREVIEW v{v} bounds=(v{w0},v{w1}) far0={} far1={} \
                     shared_far={} kind={}",
                    far0.len(),
                    far1.len(),
                    shared_far.len(),
                    if shared_far.is_empty() {
                        "STRADDLE (cross-boundary needed)"
                    } else {
                        "DRIFT (single pair)"
                    }
                );
                let cone = all_shared
                    .iter()
                    .copied()
                    .find(|s| matches!(s, Surface::Cone { .. }));
                let plane = all_shared
                    .iter()
                    .copied()
                    .find(|s| matches!(s, Surface::Plane { .. }));
                if let (
                    Some(
                        sc @ Surface::Cone {
                            apex,
                            axis_dir,
                            half_angle,
                        },
                    ),
                    Some(sp @ Surface::Plane { normal, d }),
                ) = (cone, plane)
                {
                    match project_onto_cone_section(
                        Point3::new(mid[0], mid[1], mid[2]),
                        apex,
                        axis_dir,
                        half_angle,
                        normal,
                        d,
                    ) {
                        Ok(proj) => {
                            let pa = proj.as_array();
                            let mut cert = true;
                            let mut dists = String::new();
                            for s in [sc, sp] {
                                let dd = surface_distance_and_normal(s, pa).map(|(x, _)| x.abs());
                                let band = junction_certificate_band(pa, s);
                                cert &= dd.is_some_and(|x| x <= band);
                                dists.push_str(&format!(" d={dd:?}/band={band:.3e}"));
                            }
                            eprintln!(
                                "YANG_451_PREVIEW v{v} simple_projection \
                                 proj=({:.9},{:.9},{:.9}) certificate={cert}{dists}",
                                pa[0], pa[1], pa[2]
                            );
                        }
                        Err(reason) => {
                            eprintln!("YANG_451_PREVIEW v{v} simple_projection FAILED: {reason:?}")
                        }
                    }
                } else {
                    eprintln!(
                        "YANG_451_PREVIEW v{v} shared pair is not cone+plane \
                         ({} shared surfaces) — no closed-form preview in census",
                        all_shared.len()
                    );
                }
            }
        }
        // §4.5.1 inc-3 preview (spec §12): failures with NO own-curve chain
        // (torus-carried — their pair traces no `Curve` conic) preview via the
        // ALL-CURVE walk's bounds and the implicit-pair Newton, the same arm
        // the torus block relocates with. The pair: the bounds' two common
        // surfaces when there are two; else the single common surface plus a
        // carrier surface of the traveller. Certificate + region-scale sanity
        // only — the owner-face hull verdict (C0065's own gate) is NOT
        // previewed and stays with the repair increment.
        if directions == 0 {
            if let Some((walk_bounds, common)) = &all_curve_data {
                if let [w0, w1] = walk_bounds[..] {
                    let key = (w0.min(w1), w0.max(w1));
                    if previewed.insert(key) {
                        let p0 = mesh.verts[w0 as usize].as_array();
                        let p1 = mesh.verts[w1 as usize].as_array();
                        let mid = [
                            (p0[0] + p1[0]) * 0.5,
                            (p0[1] + p1[1]) * 0.5,
                            (p0[2] + p1[2]) * 0.5,
                        ];
                        let pair: Option<(Surface, Surface)> = match common[..] {
                            [s0, s1] => Some((s0, s1)),
                            [s0] => {
                                let (va, vb) = carrier_surface_sets(&patches, a, b, v, pos);
                                va.into_iter()
                                    .chain(vb)
                                    .find(|s| *s != s0)
                                    .map(|s1| (s0, s1))
                            }
                            _ => None,
                        };
                        match pair {
                            Some((s0, s1)) => {
                                match relocate_onto_implicit_pair(
                                    Point3::new(mid[0], mid[1], mid[2]),
                                    s0,
                                    s1,
                                ) {
                                    Some(proj) => {
                                        let pn = proj.as_array();
                                        let mut cert = true;
                                        let mut dists = String::new();
                                        for s in [s0, s1] {
                                            let dd = surface_distance_and_normal(s, pn)
                                                .map(|(x, _)| x.abs());
                                            let band = junction_certificate_band(pn, s);
                                            cert &= dd.is_some_and(|x| x <= band);
                                            dists.push_str(&format!(" d={dd:?}/band={band:.3e}"));
                                        }
                                        let chord = dist3(p0, p1);
                                        let scale_ok = dist3(pn, mid) <= chord;
                                        eprintln!(
                                            "YANG_451_PREVIEW v{v} pair_newton \
                                             bounds=(v{w0},v{w1}) \
                                             proj=({:.9},{:.9},{:.9}) certificate={cert} \
                                             scale_ok={scale_ok}{dists}",
                                            pn[0], pn[1], pn[2]
                                        );
                                    }
                                    None => eprintln!(
                                        "YANG_451_PREVIEW v{v} pair_newton \
                                         bounds=(v{w0},v{w1}) NEWTON DIVERGED"
                                    ),
                                }
                            }
                            None => eprintln!(
                                "YANG_451_PREVIEW v{v} pair_newton: no usable pair \
                                 (common={})",
                                common.len()
                            ),
                        }
                    }
                } else {
                    eprintln!(
                        "YANG_451_PREVIEW v{v} pair_newton: {} all-curve bounds (need 2)",
                        walk_bounds.len()
                    );
                }
            }
        }
        let confirmed = near == 1 && directions == 2 && bounds_found == 2;
        eprintln!(
            "YANG_451_POSTSWEEP v{v} VERDICT clause1={class} allcurve_clause2={all_curve} \
             owncurve_bounds={bounds_found}/{directions} => {}",
            if confirmed {
                "§4.5.1 (region bounded both ways on its own curve)"
            } else {
                "§4.5.2 (a selector condition fails at the paper's vantage)"
            }
        );
    }
}

/// One planned §4.5.1 DRIFT-region repair (spec
/// `specs/yang_451_optimize_across_boundaries.md` §9–10): the region's
/// members collapse onto `survivor`, which moves to `proj` (the closed-form
/// projection of the bounds' midpoint onto the region's shared cone∩plane
/// section) with retag param `t`.
pub(crate) struct S451PlannedRepair {
    pub(crate) survivor: u32,
    pub(crate) victims: Vec<u32>,
    pub(crate) proj: Point3,
    /// Conic repairs carry the curve param for the `relocations` retag; the
    /// torus-region arm carries `None` — the torus block records no retag
    /// (its bookkeeping is `moved` only), and the repair mirrors its arm.
    pub(crate) retag: Option<f64>,
}

/// §4.5.1 inc-2b — the DRIFT-region repair, planned read-only.
///
/// The variant was SELECTED by the inc-2a preview (spec §9): on the measured
/// population every bounded region is a DRIFT region (both bounds carry the
/// SAME far-operand surface) whose midpoint projection certificates on both
/// surfaces — the cross-boundary half of §4.5.1 (truncation, neighbour-patch
/// continuation, q1/q2) has NO measured customer and stays unbuilt until one
/// appears.
///
/// Per distinct recorded failure, walk its own-curve chain both ways to the
/// nearest `good` vertex (converged ∧ not recorded ∧ not an I9-style
/// crosser). A region repairs only when ALL of:
/// - clause 1: every position reading is INTERIOR (the Fig-13 exclusion);
/// - two DISTINCT converged bounds exist (the paper's clause 2);
/// - the bounds share a cone+plane pair and the midpoint's projection onto
///   that section lies within the shared certificate band of BOTH surfaces;
/// - the projection stays at the region's own scale (`|proj − mid| ≤
///   |p0 − p1|` — a sanity STOP that turns a far-side conic landing into a
///   loud decline, never an acceptance band);
/// - §4-I8's containment holds for every collapse (`carried(victim) ⊆
///   carried(survivor at proj)`);
/// - the retag param is computable (Ellipse / Hyperbola — the measured
///   kinds).
///
/// Any condition failing for any recorded failure ⇒ `Err(())` and the caller
/// returns the FIRST recorded error unchanged (P10: no partial acceptance,
/// no band widening — the certificate and the collapse rule are the shared
/// ones).
///
/// Planning is READ-ONLY and complete before the first mutation, so region
/// walks never see a half-collapsed mesh; bounds may be shared between
/// adjacent regions (they are converged, never victims).
/// §4.5.1 inc-4a (spec §14): the DISTINCT surfaces carried by the inc0 edges
/// incident to any vertex in `verts` — the full constraint set a repair's
/// relocation must satisfy. A collapse rewires victims' edges onto the
/// survivor, so the survivor inherits every member's constraints; a repaired
/// position off ANY of them is the R0028 v64 mint (a q-point solved on 2 of
/// its 3 surfaces), never acceptable.
pub(crate) fn s451_constraint_surfaces(
    inc0: &std::collections::BTreeMap<(u32, u32), Vec<(InputId, Surface)>>,
    verts: &[u32],
) -> Vec<Surface> {
    let mut out: Vec<Surface> = Vec::new();
    for (&(s, e), entries) in inc0 {
        if !(verts.contains(&s) || verts.contains(&e)) {
            continue;
        }
        for &(_input, surf) in entries {
            if !out.contains(&surf) {
                out.push(surf);
            }
        }
    }
    out
}

#[allow(clippy::too_many_arguments)] // the READ-ONLY planner takes the stage's shared context
pub(crate) fn s451_plan_repairs(
    mesh: &Mesh,
    attribution: &[Option<TriangleAttribution>],
    a: &BRep,
    b: &BRep,
    curves0: &std::collections::BTreeMap<(u32, u32), Curve>,
    inc0: &std::collections::BTreeMap<(u32, u32), Vec<(InputId, Surface)>>,
    entry: &[[f64; 3]],
    failures: &[(u32, YangError)],
) -> Result<Vec<S451PlannedRepair>, ()> {
    let patches = build_patch_map(mesh, attribution);
    let adj = build_live_adjacency(mesh);
    let failed: std::collections::BTreeSet<u32> = failures.iter().map(|&(v, _)| v).collect();
    let good = |w: u32| -> bool {
        !failed.contains(&w)
            && vertex_converged(mesh, &patches, a, b, w)
            && !vertex_crossed_domain_endpoint(mesh, attribution, a, b, &adj, entry, w)
    };
    let mut consumed: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
    let mut plans: Vec<S451PlannedRepair> = Vec::new();

    for &(v, _) in failures {
        if consumed.contains(&v) {
            continue;
        }
        // Clause 1 at the failure itself.
        let pos = mesh.verts[v as usize].as_array();
        let (ca, cb) = carrier_counts(&patches, a, b, v, pos);
        if ca.max(cb) != 1 {
            eprintln!("YANG_451_REPAIR v{v} DECLINE clause1 carrier=(A{ca},B{cb})");
            return Err(());
        }
        // The failure's own conic (LineSegment cannot key a chain).
        let mut own_curves: Vec<Curve> = Vec::new();
        for (&(s, e), &c) in curves0.iter() {
            if (s == v || e == v) && c != Curve::LineSegment && !own_curves.contains(&c) {
                own_curves.push(c);
            }
        }
        if own_curves.is_empty() {
            // §4.5.1 inc-3 TORUS-REGION arm (spec §12–13): the failure's pair
            // traces no `Curve` conic (torus-carried), so region and bounds
            // come from the intersection-curve graph and the re-optimization
            // is the torus block's own implicit-pair Newton — accepted by the
            // SAME three-part reading its gate uses: shared certificate,
            // region scale, and the owner-face hull
            // (`planar_partner_hull_contains`, the extracted gate reading).
            // k = 1 only: both bounds must be DIRECT curve neighbours of the
            // failure — no measured k>1 torus customer.
            let on_curve = |w: u32| vertex_on_curve(&patches, w);
            let Some((walk_bounds, _common)) =
                selector_clause2_walk(mesh, attribution, a, b, &adj, v, &on_curve, &good)
            else {
                eprintln!("YANG_451_REPAIR v{v} DECLINE torus arm: clause 2 fails");
                return Err(());
            };
            let [w0, w1] = walk_bounds[..] else {
                eprintln!(
                    "YANG_451_REPAIR v{v} DECLINE torus arm: {} bounds (need 2)",
                    walk_bounds.len()
                );
                return Err(());
            };
            let direct = |w: u32| adj.get(&v).is_some_and(|nb| nb.contains(&w)) && on_curve(w);
            if !(direct(w0) && direct(w1)) {
                eprintln!("YANG_451_REPAIR v{v} DECLINE torus arm: region k > 1");
                return Err(());
            }
            let p0 = mesh.verts[w0 as usize].as_array();
            let p1 = mesh.verts[w1 as usize].as_array();
            let mid = [
                (p0[0] + p1[0]) * 0.5,
                (p0[1] + p1[1]) * 0.5,
                (p0[2] + p1[2]) * 0.5,
            ];
            // §4.5.1 inc-4b (spec §14): the solve set is the failure's FULL
            // inc0 constraint set, not a pair completed from carrier order.
            // 3 distinct surfaces = the paper's q-point on C_b (`:665-668`,
            // "solve the intersection points q1 and q2 on C_b using Newton's
            // method") computed as 3 implicits — the same primitive the
            // sweep's own triple block uses. The walk still supplies the
            // bounds; `common` is no longer the solve authority.
            let constraints = s451_constraint_surfaces(inc0, &[v]);
            let midp = Point3::new(mid[0], mid[1], mid[2]);
            let proj = match constraints[..] {
                [s0, s1] => relocate_onto_implicit_pair(midp, s0, s1),
                [s0, s1, s2] => relocate_onto_implicit_triple(midp, s0, s1, s2),
                _ => {
                    eprintln!(
                        "YANG_451_REPAIR v{v} DECLINE torus arm: {} constraint surfaces \
                         (need 2 or 3)",
                        constraints.len()
                    );
                    return Err(());
                }
            };
            let Some(proj) = proj else {
                eprintln!(
                    "YANG_451_REPAIR v{v} DECLINE torus arm: Newton diverged on {} surfaces",
                    constraints.len()
                );
                return Err(());
            };
            let pa2 = proj.as_array();
            // inc-4a: certificate on EVERY constraint surface — a projection
            // off any carried surface is the v64 mint, a loud decline.
            for &s in &constraints {
                let ok = surface_distance_and_normal(s, pa2)
                    .is_some_and(|(x, _)| x.abs() <= junction_certificate_band(pa2, s));
                if !ok {
                    eprintln!("YANG_451_REPAIR v{v} DECLINE torus arm: certificate fails on {s:?}");
                    return Err(());
                }
            }
            let chord = dist3(p0, p1);
            if dist3(pa2, mid) > chord {
                eprintln!(
                    "YANG_451_REPAIR v{v} DECLINE torus arm: projection left the region                      scale (|proj-mid|={:.3e} > |bounds|={:.3e})",
                    dist3(pa2, mid),
                    chord
                );
                return Err(());
            }
            let Some(d_eps) = stage4_chord_band(a, b) else {
                eprintln!("YANG_451_REPAIR v{v} DECLINE torus arm: no chord band");
                return Err(());
            };
            for &s in &constraints {
                if planar_partner_hull_contains(a, b, s, pa2, d_eps) == Some(false) {
                    eprintln!(
                        "YANG_451_REPAIR v{v} DECLINE torus arm: owner-face hull refuses                          on {s:?}"
                    );
                    return Err(());
                }
            }
            eprintln!(
                "YANG_451_REPAIR torus region k=1 survivor=v{v} bounds=(v{w0},v{w1})                  n_surfs={} proj=({:.9},{:.9},{:.9})",
                constraints.len(),
                pa2[0],
                pa2[1],
                pa2[2]
            );
            consumed.insert(v);
            plans.push(S451PlannedRepair {
                survivor: v,
                victims: Vec::new(),
                proj,
                retag: None,
            });
            continue;
        }
        let [curve] = own_curves[..] else {
            eprintln!(
                "YANG_451_REPAIR v{v} DECLINE own-curve count {} != 1",
                own_curves.len()
            );
            return Err(());
        };
        let nbrs_on = |w: u32| -> Vec<u32> {
            curves0
                .iter()
                .filter(|(&(s, e), c)| (s == w || e == w) && **c == curve)
                .map(|(&(s, e), _)| if s == w { e } else { s })
                .collect()
        };
        let mut members: Vec<u32> = vec![v];
        let mut bounds: Vec<u32> = Vec::new();
        for start in nbrs_on(v) {
            let (mut prev, mut cur) = (v, start);
            loop {
                if good(cur) {
                    bounds.push(cur);
                    break;
                }
                if members.len() > 128 {
                    eprintln!("YANG_451_REPAIR v{v} DECLINE region cap");
                    return Err(());
                }
                if !members.contains(&cur) {
                    members.push(cur);
                }
                let next: Vec<u32> = nbrs_on(cur).into_iter().filter(|&x| x != prev).collect();
                match next.len() {
                    1 => {
                        prev = cur;
                        cur = next[0];
                    }
                    n => {
                        eprintln!(
                            "YANG_451_REPAIR v{v} DECLINE chain via v{start}: {} continuations",
                            n
                        );
                        return Err(());
                    }
                }
            }
        }
        let [w0, w1] = bounds[..] else {
            eprintln!("YANG_451_REPAIR v{v} DECLINE bounds {} != 2", bounds.len());
            return Err(());
        };
        if w0 == w1 {
            eprintln!("YANG_451_REPAIR v{v} DECLINE bounds coincide (v{w0})");
            return Err(());
        }
        // The shared cone+plane pair, read from the bounds' carried surfaces.
        let p0 = mesh.verts[w0 as usize].as_array();
        let p1 = mesh.verts[w1 as usize].as_array();
        let (sa0, sb0) = carrier_surface_sets(&patches, a, b, w0, p0);
        let (sa1, sb1) = carrier_surface_sets(&patches, a, b, w1, p1);
        let shared: Vec<Surface> = sa0
            .iter()
            .chain(sb0.iter())
            .filter(|s| sa1.contains(s) || sb1.contains(s))
            .copied()
            .collect();
        let cone = shared
            .iter()
            .copied()
            .find(|s| matches!(s, Surface::Cone { .. }));
        let plane = shared
            .iter()
            .copied()
            .find(|s| matches!(s, Surface::Plane { .. }));
        let (
            Some(
                sc @ Surface::Cone {
                    apex,
                    axis_dir,
                    half_angle,
                },
            ),
            Some(sp @ Surface::Plane { normal, d }),
        ) = (cone, plane)
        else {
            eprintln!(
                "YANG_451_REPAIR v{v} DECLINE shared pair not cone+plane ({} shared)",
                shared.len()
            );
            return Err(());
        };
        let mid = [
            (p0[0] + p1[0]) * 0.5,
            (p0[1] + p1[1]) * 0.5,
            (p0[2] + p1[2]) * 0.5,
        ];
        let proj = match project_onto_cone_section(
            Point3::new(mid[0], mid[1], mid[2]),
            apex,
            axis_dir,
            half_angle,
            normal,
            d,
        ) {
            Ok(p) => p,
            Err(reason) => {
                eprintln!("YANG_451_REPAIR v{v} DECLINE projection failed: {reason:?}");
                return Err(());
            }
        };
        let pa = proj.as_array();
        // Shared certificate on BOTH surfaces — the same reading the selector
        // and §4-I9 use; no new band.
        for s in [sc, sp] {
            let ok = surface_distance_and_normal(s, pa)
                .is_some_and(|(x, _)| x.abs() <= junction_certificate_band(pa, s));
            if !ok {
                eprintln!("YANG_451_REPAIR v{v} DECLINE certificate fails on {s:?}");
                return Err(());
            }
        }
        // §4.5.1 inc-4a (spec §14): the survivor inherits every member's inc0
        // edges under the collapse, so the projection must ALSO certificate on
        // every surface the whole region carries — a third surface here is a
        // boundary-crossing region (the R0028 v64 shape) and declines loudly.
        for s in s451_constraint_surfaces(inc0, &members) {
            let ok = surface_distance_and_normal(s, pa)
                .is_some_and(|(x, _)| x.abs() <= junction_certificate_band(pa, s));
            if !ok {
                eprintln!(
                    "YANG_451_REPAIR v{v} DECLINE region constraint surface fails at proj: {s:?}"
                );
                return Err(());
            }
        }
        // Region-scale sanity: a projection landing on a FAR part of the same
        // infinite conic is a wrong answer — STOP, never accept.
        let chord = dist3(p0, p1);
        if dist3(pa, mid) > chord {
            eprintln!(
                "YANG_451_REPAIR v{v} DECLINE projection left the region scale \
                 (|proj-mid|={:.3e} > |bounds|={:.3e})",
                dist3(pa, mid),
                chord
            );
            return Err(());
        }
        // Retag param on the region's own conic (the measured kinds).
        let t = match curve {
            Curve::Ellipse {
                center,
                normal: en,
                major_axis,
                major_radius,
                minor_radius,
            } => ellipse_param(proj, center, en, major_axis, major_radius, minor_radius),
            Curve::Hyperbola {
                center,
                normal: hn,
                major_axis,
                semi_conjugate,
                ..
            } => {
                // t = asinh(v_coord / b) in the stored frame — the same
                // round-trip the cone-hyperbola arm records.
                let n = normalize3(hn.as_array());
                let maj = normalize3(major_axis.as_array());
                let conj = [
                    n[1] * maj[2] - n[2] * maj[1],
                    n[2] * maj[0] - n[0] * maj[2],
                    n[0] * maj[1] - n[1] * maj[0],
                ];
                let ctr = center.as_array();
                let vc = (pa[0] - ctr[0]) * conj[0]
                    + (pa[1] - ctr[1]) * conj[1]
                    + (pa[2] - ctr[2]) * conj[2];
                (vc / semi_conjugate).asinh()
            }
            other => {
                eprintln!("YANG_451_REPAIR v{v} DECLINE unretaggable curve kind {other:?}");
                return Err(());
            }
        };
        // §4-I8 containment per collapse, against the survivor's FINAL
        // position: every victim's carried set must be a subset of what the
        // repaired survivor carries.
        let surv_carried: Vec<Surface> = {
            let (xa, xb) = carrier_surface_sets(&patches, a, b, v, pa);
            xa.into_iter().chain(xb).collect()
        };
        members.sort_unstable();
        members.dedup();
        let survivor = members[0];
        for &m in &members {
            let mp = mesh.verts[m as usize].as_array();
            let (ma, mb) = carrier_surface_sets(&patches, a, b, m, mp);
            if !ma.iter().chain(mb.iter()).all(|s| surv_carried.contains(s)) {
                eprintln!("YANG_451_REPAIR v{v} DECLINE §4-I8 containment fails for member v{m}");
                return Err(());
            }
        }
        eprintln!(
            "YANG_451_REPAIR region k={} survivor=v{survivor} bounds=(v{w0},v{w1}) \
             proj=({:.9},{:.9},{:.9}) t={t:.6} cone_tan={:.6} chord={chord:.4e}",
            members.len(),
            pa[0],
            pa[1],
            pa[2],
            half_angle.tan()
        );
        let victims: Vec<u32> = members.iter().copied().filter(|&m| m != survivor).collect();
        consumed.extend(members.iter().copied());
        plans.push(S451PlannedRepair {
            survivor,
            victims,
            proj,
            retag: Some(t),
        });
    }
    // Every recorded failure must belong to a planned region — a failure the
    // walk could not reach (e.g. a torus-carried vertex with no conic edges)
    // means the repair set is incomplete and the stage must STOP.
    for &(v, _) in failures {
        if !consumed.contains(&v) {
            eprintln!("YANG_451_REPAIR v{v} DECLINE not reachable by any planned region");
            return Err(());
        }
    }
    Ok(plans)
}

pub(crate) fn dist3(p: [f64; 3], q: [f64; 3]) -> f64 {
    ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt()
}
