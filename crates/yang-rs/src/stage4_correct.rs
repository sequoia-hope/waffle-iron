//! Stage 4 — mesh correction: Phase-A patch census, vertex collapse,
//! sub-resolution segment collapse, relocation application + reversal
//! sweeps, relocated-triangle validation (extracted verbatim from
//! lib.rs — spec `specs/yang_rs_lib_decomposition.md`, increment 7).

#[allow(clippy::wildcard_imports)]
use crate::*;

/// §4.4.2 tangent-plane corridor half-width. The two surfaces at a
/// relocation site, linearized as tangent planes P_A, P_B meeting at angle
/// θ, admit a corridor of `2·budget/sinθ` around L = P_A ∩ P_B — the
/// Stage-1 budget mapped through the wedge (paper
/// refs/text/yang2025_hybrid_boolean.txt:494-537). `divergence` is the
/// unit-vector cross-product magnitude (= sin θ), or the gradient magnitude
/// for implicit forms. At exact tangency the corridor is unbounded →
/// `INFINITY` (the circle-junction gate precedent: the projection is still
/// the local nearest point; callers that need a finite band gate on a
/// tangency cutoff FIRST). Extracted from five duplicated sites (design
/// review 2026-07-12 F8) — the formula must never be re-inlined: a future
/// correction has to land HERE once.
pub(crate) fn tangent_plane_corridor(budget: f64, divergence: f64) -> f64 {
    if divergence > 0.0 {
        2.0 * budget / divergence
    } else {
        f64::INFINITY
    }
}

// =========================================================================
// PR-YR5 — topology reconstruction
// =========================================================================

/// PR-YR5 internal: the triple `(vertices, edges, faces)` produced
/// by `reconstruct_topology` to populate the output `BRep`.
///
/// PR-YR10: extended with a fourth component — the per-output-mesh-vertex
/// `Vec<TessellationSource>` (default `BRepVertex(i)`, overridden to
/// `BRepEdge { edge, t }` for Stage-4-relocated intersection vertices).
pub(crate) type ReconstructedTopology = (
    Vec<BRepVertex>,
    Vec<BRepEdge>,
    Vec<BRepFace>,
    Vec<TessellationSource>,
    // PR-KV13 F2: per-output-face attribution, parallel to `faces` — the
    // `(input, face)` the patch descends from (the kernel maps it to the
    // operand's persistent face id for boolean provenance).
    Vec<TriangleAttribution>,
);

/// PR-YR5/9 `(vertices, edges, faces)` triple — the pre-PR-YR10 reconstruction
/// shape retained for the `#[cfg(test)]` unit-test callers.
#[cfg(test)]
pub(crate) type LegacyTopology = (Vec<BRepVertex>, Vec<BRepEdge>, Vec<BRepFace>);

/// PR-YR9 (lifted to module scope in PR-YR10 so `stage4_relocate_and_correct`
/// can consume the same ordered, oriented patch loops + inherited surface that
/// the Phase-B emission uses — no re-derivation, no classification drift).
pub(crate) struct PatchInfo {
    pub(crate) cycles: Vec<Vec<(u32, u32)>>,
    pub(crate) input: InputId,
    pub(crate) inherited: Surface,
    pub(crate) face_idx: usize,
    /// The INPUT face's cavity sense (PR-KV6b-1): a kept patch of an
    /// already-reversed input wall (e.g. a washer's inner tube) must keep
    /// its sense in the output — composed by XOR with the Subtract-B flip.
    pub(crate) input_reversed: bool,
    /// Spec yang_stage6_sliver_topology §2/§4B: this patch contained ≥1 FOLD
    /// sliver that §4A excluded from boundary derivation (`patch_fold_slivers`).
    /// Such a patch may carry a whole shared solid edge as ONE un-subdivided
    /// chord (the collapsed subdivision the slivers used to represent), so it
    /// — and ONLY it — is eligible for the §4B loop T-subdivision. Patches
    /// with no excluded fold sliver keep byte-identical loops (the measured
    /// chord lives on the fold-bearing side; the other side already
    /// subdivides), which keeps curved / benign-T-junction output at exact
    /// reference parity.
    pub(crate) had_fold_sliver: bool,
}

/// PR-YR10: the Phase-A structures `reconstruct_topology` derives before the
/// Phase-B emission: per-patch ordered loops + inherited surface (`infos`), the
/// edge→incident-(input,surface) map (`incidence`), and the exact per-edge
/// analytical `Curve` map (`curves`). Recomputed after a §4.5.3 collapse.
pub(crate) type PhaseA = (
    Vec<PatchInfo>,
    std::collections::BTreeMap<(u32, u32), Vec<(InputId, Surface)>>,
    std::collections::BTreeMap<(u32, u32), Curve>,
);

/// §4.5.1 inc-0 (spec `specs/yang_451_optimize_across_boundaries.md`): the
/// shared constructor for the relocation sweep's `OffCurveBeyondChordBand`
/// STOPs — behaviour identical to constructing the error inline. Under
/// `YANG_451=census` it additionally prints the firing SITE (`#[track_caller]`,
/// so sites don't lie — the `YANG_LRR_PROBE` pattern) and the vertex's
/// incident edge-level curve assignments, the data that decides the spec's
/// Q1: at a curve-graph junction, can the branches be paired by the vertex's
/// own assigned conic?
#[track_caller]
fn offcurve_beyond_chord_band(
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
fn s451_stop(
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

/// §4.5.1 inc-1: the post-sweep selector census at the paper's vantage —
/// every non-failed vertex is relocated, so "successfully optimized" is
/// finally well-posed. Per recorded failure (deduped by vertex, sweep
/// order): clause 1 (Fig-13 carrier reading), I12's all-curve clause-2 walk
/// (`failed` = the recorded set), and the OWN-CURVE chain with per-vertex
/// convergence — the region's true extent and its bounds, the §4.5.1/§4.5.2
/// verdict data. Census-only prints; the caller returns the first error
/// after this regardless.
#[allow(clippy::too_many_arguments)]
fn s451_post_sweep_census(
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
struct S451PlannedRepair {
    survivor: u32,
    victims: Vec<u32>,
    proj: Point3,
    /// Conic repairs carry the curve param for the `relocations` retag; the
    /// torus-region arm carries `None` — the torus block records no retag
    /// (its bookkeeping is `moved` only), and the repair mirrors its arm.
    retag: Option<f64>,
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
fn s451_constraint_surfaces(
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
fn s451_plan_repairs(
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

fn dist3(p: [f64; 3], q: [f64; 3]) -> f64 {
    ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt()
}

/// The torus block's owner-face containment reading, EXTRACTED (§4.5.1
/// inc-3) so the torus-region repair applies the SAME acceptance its gate
/// uses — one reading, two callers, no drift.
///
/// For a PLANAR `partner` surface: a conservative AABB over every matching
/// face of either input — loop vertices plus each boundary CURVE's own
/// extent (a disk's loop is one closed circle through a single anchor
/// vertex, so vertex hulls under-bound curved loops — the t134 trap).
/// Returns `Some(inside)` when a bounded hull exists; `None` = NO VERDICT
/// (non-planar partner, a loop curve without a cheap conservative bound, or
/// no matching input face) — callers must treat `None` as "no wall",
/// exactly as the gate always has (defensive: never a false wall).
fn planar_partner_hull_contains(
    a: &BRep,
    b: &BRep,
    partner: Surface,
    pos: [f64; 3],
    d_eps: f64,
) -> Option<bool> {
    let Surface::Plane { .. } = partner else {
        return None;
    };
    let mut hull: Option<[f64; 6]> = None;
    for brep in [a, b] {
        for face in brep.faces() {
            if face.surface != partner {
                continue;
            }
            let mut lo = [f64::MAX; 3];
            let mut hi = [f64::MIN; 3];
            for &e in face
                .outer_loop
                .iter()
                .chain(face.inner_loops.iter().flatten())
            {
                let ed = &brep.edges()[e as usize];
                for vid in [ed.start, ed.end] {
                    let q = brep.vertices()[vid as usize].point.as_array();
                    for k in 0..3 {
                        lo[k] = lo[k].min(q[k]);
                        hi[k] = hi[k].max(q[k]);
                    }
                }
                match ed.curve {
                    Curve::LineSegment => {}
                    Curve::Circle {
                        center,
                        normal,
                        radius,
                    } => {
                        let c = center.as_array();
                        let n = normalize3(normal.as_array());
                        for k in 0..3 {
                            let ext = radius * (1.0 - n[k] * n[k]).max(0.0).sqrt();
                            lo[k] = lo[k].min(c[k] - ext);
                            hi[k] = hi[k].max(c[k] + ext);
                        }
                    }
                    Curve::Ellipse {
                        center,
                        major_radius,
                        ..
                    } => {
                        let c = center.as_array();
                        for k in 0..3 {
                            lo[k] = lo[k].min(c[k] - major_radius);
                            hi[k] = hi[k].max(c[k] + major_radius);
                        }
                    }
                    _ => return None,
                }
            }
            let h =
                hull.get_or_insert([f64::MAX, f64::MAX, f64::MAX, f64::MIN, f64::MIN, f64::MIN]);
            for k in 0..3 {
                h[k] = h[k].min(lo[k]);
                h[3 + k] = h[3 + k].max(hi[k]);
            }
        }
    }
    let h = hull?;
    Some((0..3).all(|k| pos[k] >= h[k] - d_eps && pos[k] <= h[3 + k] + d_eps))
}

/// PR-YR10: compute the Phase-A structures (adjacency → patches → cycles →
/// incidence → exact intersection curves) from the current mesh + attribution.
/// Factored out of `reconstruct_topology` so it can be re-run after a §4.5.3
/// collapse mutates the mesh.
pub(crate) fn compute_phase_a(
    mesh: &Mesh,
    attribution: &TriangleAttributionMap,
    a: &BRep,
    b: &BRep,
    edge_provenance: &crate::stage3_ssi::PosKeyedEdgeSet,
) -> Result<PhaseA, YangError> {
    let adjacency = triangle_adjacency(mesh);
    let patches = flood_fill_patches(mesh, attribution, &adjacency);
    // PR-YR27 (Finding 1a): merge edge-adjacent patches lying on the SAME
    // plane with the SAME orientation into one output face — a coplanar
    // boolean otherwise emits e.g. A's and B's side fragments as two faces
    // on one bit-identical plane, and the NEXT boolean in a chain
    // exact-ties between them. Non-adjacent same-plane patches stay
    // separate faces (their union is not a single connected face).
    let patches = merge_same_plane_patches(patches, &adjacency, a, b);

    let mut infos: Vec<PatchInfo> = Vec::with_capacity(patches.len());
    for patch in &patches {
        let cycles = patch_boundary_cycle(patch, mesh)?;
        let input = patch.attribution.input;
        let input_brep = match input {
            InputId::A => a,
            InputId::B => b,
        };
        let face_idx = patch.attribution.face as usize;
        if face_idx >= input_brep.faces().len() {
            return Err(YangError::MalformedTopology(format!(
                "attribution.face = {face_idx} out of range (input has {} faces)",
                input_brep.faces().len()
            )));
        }
        let inherited = input_brep.faces()[face_idx].surface;
        let input_reversed = input_brep.faces()[face_idx].reversed;
        let had_fold_sliver = !patch_fold_slivers(patch, mesh).is_empty();
        infos.push(PatchInfo {
            cycles,
            input,
            inherited,
            face_idx,
            input_reversed,
            had_fold_sliver,
        });
    }

    let mut incidence: std::collections::BTreeMap<(u32, u32), Vec<(InputId, Surface)>> =
        std::collections::BTreeMap::new();
    for info in &infos {
        for cycle in &info.cycles {
            for &(s, e) in cycle {
                let key = if s < e { (s, e) } else { (e, s) };
                incidence
                    .entry(key)
                    .or_default()
                    .push((info.input, info.inherited));
            }
        }
    }
    // §4.2.3 incidence probe (`YANG_S423_INCIDENCE`, READ-ONLY). Diff the
    // cycle-derived incidence built just above against the paper's own route —
    // "querying the triangles that intersect at that point", i.e. the
    // per-triangle provenance map N4 established. Nothing here feeds Stage 3/4;
    // gate-OFF is byte-identical by construction. See `stage4_incidence`.
    if std::env::var_os("YANG_S423_INCIDENCE").is_some() {
        let prov = crate::stage4_incidence::provenance_edge_incidence(mesh, attribution);
        let cyc = crate::stage4_incidence::cycle_edge_incidence(
            infos
                .iter()
                .map(|i| (i.input, i.face_idx as u32, i.cycles.as_slice())),
        );
        // Edges on an IMPURE (merged) patch's cycle: divergence there is
        // PR-YR27's merge doing its job, not a provenance disagreement.
        let mut explained: std::collections::BTreeSet<(u32, u32)> =
            std::collections::BTreeSet::new();
        let mut n_impure = 0usize;
        for (patch, info) in patches.iter().zip(infos.iter()) {
            if crate::stage4_incidence::patch_is_impure(
                &patch.tri_indices,
                attribution,
                patch.attribution,
            ) {
                n_impure += 1;
                for cycle in &info.cycles {
                    for &(s, e) in cycle {
                        explained.insert(if s < e { (s, e) } else { (e, s) });
                    }
                }
            }
        }
        let d = crate::stage4_incidence::diff_incidence(&cyc, &prov, &explained);
        eprintln!(
            "[s423-incidence] boundary_edges={} agree={} prov_richer={} \
             cycle_unsupported={} disjointish={} missing_in_prov={} \
             patches={} impure_patches={} merge_explained={} UNEXPLAINED={}",
            d.boundary_edges,
            d.agree,
            d.prov_richer,
            d.cycle_unsupported,
            d.disjointish,
            d.missing_in_prov,
            infos.len(),
            n_impure,
            d.divergent_merge_explained,
            d.divergent_unexplained,
        );
        for (k, v, conly, ponly) in &d.unexplained_samples {
            eprintln!(
                "[s423-incidence]   UNEXPLAINED edge {k:?} {v:?} cycle-only {conly:?} prov-only {ponly:?}"
            );
        }
    }

    let curves = build_intersection_curves(&incidence, mesh, a, b, edge_provenance)?;
    Ok((infos, incidence, curves))
}

/// PR-YR27 (Finding 1a): merge edge-adjacent output patches whose inherited
/// planes are the same plane with the same orientation (bit-identical or
/// within `TAU_WORK` on the UNIT-normalized `(n̂, d̂)`) into ONE patch, so
/// Stage 6 emits one face per connected same-plane region of the output
/// solid.
///
/// Why: a coplanar boolean's output legitimately carries triangles from
/// BOTH inputs' faces on one geometric plane (e.g. exactly stacked boxes:
/// each side plane has an A fragment and a B fragment, edge-adjacent along
/// the seam). `flood_fill_patches` groups by attribution, so those
/// fragments emit as TWO faces on a bit-identical plane — a fragmented
/// B-Rep whose NEXT boolean exact-ties Stage-6 membership between them
/// (assay F0066). Merging is keyed to edge adjacency: non-adjacent
/// same-plane patches (genuinely separate faces) are NOT merged.
///
/// Safety / blast radius:
/// - Only `Surface::Plane` patches participate; the orientation test
///   (component-wise `|n̂ᵢ−n̂ⱼ| ≤ TAU_WORK`) means an opposite-normal pair
///   (e.g. a subtract cavity wall against an outer wall) NEVER merges.
/// - Distinct input faces on one plane only exist when an input itself
///   carries same-plane faces or the two inputs share a plane — exactly
///   the coplanar classes; every other fixture has zero mergeable pairs
///   and is byte-identical.
/// - The merged patch's attribution is the lexicographically smallest
///   member `(input, face)` (deterministic); the members' inherited
///   surfaces agree within `TAU_WORK`, so the choice is geometric noise.
/// - The seam edges become patch-INTERIOR (they vanish from the boundary
///   cycles and therefore from the output edge set) — the merged region's
///   single outer cycle is exactly the §4.5.5 result-face boundary.
pub(crate) fn merge_same_plane_patches(
    mut patches: Vec<Patch>,
    adjacency: &[Vec<u32>],
    a: &BRep,
    b: &BRep,
) -> Vec<Patch> {
    if patches.len() < 2 {
        return patches;
    }

    // Inherited surface key per patch (`None` = unmergeable surface kind or
    // degenerate — never merged). A `Plane` keys on its unit `(n̂, d̂)`; a
    // `Cylinder` keys on its unit axis, an axis-line anchor (the axis point
    // projected to remove the free axial slide), the radius, AND the effective
    // outward sense (`reversed`) — two coincident cylinders of OPPOSITE sense
    // (a bore wall vs an outer wall) must NEVER merge (PR-5; mirrors the planar
    // opposite-normal guard). Spheres/cones keep `None` (not yet needed).
    enum SurfKey {
        Plane {
            n: [f64; 3],
            d: f64,
        },
        Cyl {
            axis: [f64; 3],
            anchor: [f64; 3],
            radius: f64,
            reversed: bool,
        },
    }
    let keys: Vec<Option<SurfKey>> = patches
        .iter()
        .map(|p| {
            let brep = match p.attribution.input {
                InputId::A => a,
                InputId::B => b,
            };
            let f = brep.faces().get(p.attribution.face as usize)?;
            match f.surface {
                Surface::Plane { normal, d } => {
                    let n = normal.as_array();
                    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
                    if len < cad_primitives::MIN_FEATURE_SIZE {
                        return None;
                    }
                    Some(SurfKey::Plane {
                        n: [n[0] / len, n[1] / len, n[2] / len],
                        d: d / len,
                    })
                }
                Surface::Cylinder {
                    axis_point,
                    axis_dir,
                    radius,
                } => {
                    let ad = axis_dir.as_array();
                    let len = (ad[0] * ad[0] + ad[1] * ad[1] + ad[2] * ad[2]).sqrt();
                    if len < cad_primitives::MIN_FEATURE_SIZE {
                        return None;
                    }
                    let axis = [ad[0] / len, ad[1] / len, ad[2] / len];
                    // Anchor = axis_point with its axial component removed, so
                    // two cylinders sharing one axis LINE but with axis points at
                    // different axial offsets get an identical anchor.
                    let ap = axis_point.as_array();
                    let t = ap[0] * axis[0] + ap[1] * axis[1] + ap[2] * axis[2];
                    let anchor = [
                        ap[0] - t * axis[0],
                        ap[1] - t * axis[1],
                        ap[2] - t * axis[2],
                    ];
                    Some(SurfKey::Cyl {
                        axis,
                        anchor,
                        radius,
                        reversed: f.reversed,
                    })
                }
                _ => None,
            }
        })
        .collect();
    let mergeable = |i: usize, j: usize| -> bool {
        match (&keys[i], &keys[j]) {
            (Some(SurfKey::Plane { n: ni, d: di }), Some(SurfKey::Plane { n: nj, d: dj })) => {
                (di - dj).abs() <= cad_primitives::TAU_WORK
                    && (0..3).all(|k| (ni[k] - nj[k]).abs() <= cad_primitives::TAU_WORK)
            }
            (
                Some(SurfKey::Cyl {
                    axis: ai,
                    anchor: anchi,
                    radius: ri,
                    reversed: revi,
                }),
                Some(SurfKey::Cyl {
                    axis: aj,
                    anchor: anchj,
                    radius: rj,
                    reversed: revj,
                }),
            ) => {
                // Same effective sense, equal radius, parallel axes, same axis
                // line (anchors agree up to TAU_WORK; axes may be antiparallel —
                // a cylinder's axis_dir sign is free — so compare |aᵢ·aⱼ|≈1).
                revi == revj
                    && (ri - rj).abs() <= cad_primitives::TAU_WORK
                    && (ai[0] * aj[0] + ai[1] * aj[1] + ai[2] * aj[2]).abs()
                        >= 1.0 - cad_primitives::TAU_WORK
                    && (0..3).all(|k| (anchi[k] - anchj[k]).abs() <= cad_primitives::TAU_WORK)
            }
            _ => false,
        }
    };

    // patch index per mesh triangle.
    let mut patch_of: Vec<usize> = vec![usize::MAX; adjacency.len()];
    for (pi, p) in patches.iter().enumerate() {
        for &t in &p.tri_indices {
            patch_of[t as usize] = pi;
        }
    }

    // Union-find over patches, united on (edge-adjacent AND same-plane).
    let mut parent: Vec<usize> = (0..patches.len()).collect();
    fn find(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]]; // path halving
            x = parent[x];
        }
        x
    }
    for (pi, p) in patches.iter().enumerate() {
        for &t in &p.tri_indices {
            for &u in &adjacency[t as usize] {
                let pj = patch_of[u as usize];
                if pj == usize::MAX || pj == pi {
                    continue;
                }
                if mergeable(pi, pj) {
                    let (ri, rj) = (find(&mut parent, pi), find(&mut parent, pj));
                    if ri != rj {
                        parent[ri.max(rj)] = ri.min(rj);
                    }
                }
            }
        }
    }

    // Rebuild merged patches in first-member order (deterministic; a strict
    // no-op — same patches, same order — when nothing merged).
    let roots: Vec<usize> = (0..patches.len()).map(|i| find(&mut parent, i)).collect();
    let mut out: Vec<Patch> = Vec::with_capacity(patches.len());
    let mut taken = vec![false; patches.len()];
    for i in 0..patches.len() {
        if taken[i] {
            continue;
        }
        let members: Vec<usize> = (i..patches.len())
            .filter(|&j| roots[j] == roots[i])
            .collect();
        for &m in &members {
            taken[m] = true;
        }
        let attribution = members
            .iter()
            .map(|&m| patches[m].attribution)
            .min()
            .expect("members is non-empty");
        let mut tri_indices: Vec<u32> = Vec::new();
        for &m in &members {
            tri_indices.append(&mut patches[m].tri_indices);
        }
        out.push(Patch {
            attribution,
            tri_indices,
        });
    }
    out
}

/// PR-YR15 helper: the Stage-1 curved chord bound of ONE input, choosing the
/// surface's OWN bound (A14.3 / I-sphere-band). A `Surface::Sphere` face's
/// tessellation vertices sit off the exact great circle by up to the sphere's
/// own `sphere_chord_bound(radius) = 1e-2·2r√3`, which is LARGER than the
/// rim-AABB `curved_chord_bound` (2r√2) — so a sphere-bearing input must report
/// its sphere bound, NOT the rim band (which would underestimate and reject
/// valid sphere-rim vertices). Cylinder/all-planar inputs keep the rim-AABB
/// `curved_chord_bound` byte-for-byte. When both are present we take the MAX
/// (the budget must admit every curved-surface vertex). `None` only for an
/// all-planar input (zero chord error). This is the SINGLE source consulted by
/// both `build_intersection_curves` (selection tol) and `stage4_chord_band`
/// (relocation budget); it is NOT tolerance widening.
pub(crate) fn input_curved_chord_bound(brep: &BRep) -> Option<f64> {
    // Spec `yang_s3_ellipse_rim_chord_bound` amendment 1: an ellipse-rim-only
    // input (obliquely-trimmed cylinder re-entering from a prior boolean)
    // carries the Stage-1 ellipse chain bound — fallback-only composition,
    // byte-identical whenever a Circle rim exists.
    let rim = curved_chord_bound(brep.edges()).or_else(|| ellipse_rim_chord_bound(brep.edges()));
    let sphere = brep
        .faces()
        .iter()
        .filter_map(|f| match f.surface {
            Surface::Sphere { radius, .. } => Some(sphere_chord_bound(radius)),
            _ => None,
        })
        .fold(None, |acc: Option<f64>, b| {
            Some(acc.map_or(b, |a| a.max(b)))
        });
    match (rim, sphere) {
        (Some(r), Some(s)) => Some(r.max(s)),
        (Some(r), None) => Some(r),
        (None, s) => s,
    }
}

/// PR-YR10 helper: the Stage-4 chord-band relocation budget `d_ε` — the
/// Stage-1 chord bound of whichever input bears a curved surface (the curved
/// solid). Uses [`input_curved_chord_bound`] so a sphere input reports its OWN
/// (larger) 2r√3 bound, not the rim-AABB 2r√2 (I-sphere-band). `None` only if
/// NEITHER input has a curved surface, which cannot happen when a conic
/// intersection edge exists (a conic edge implies a curved input).
pub(crate) fn stage4_chord_band(a: &BRep, b: &BRep) -> Option<f64> {
    // PR-KV7: the MAX of the two inputs' Stage-1 bounds, not A-with-B-
    // fallback. An arrangement vertex on an A×B intersection curve sits on
    // the curved OWNER's facet chord, off the exact curve by up to that
    // owner's OWN sagitta — and with chainable boolean outputs the owner
    // can be EITHER input (a recovered body re-entering as A can have a
    // much tighter rim AABB than the fresh operand B whose curves are
    // being relocated). `max` admits exactly up to the looser owner's
    // honest Stage-1 bound for this model pair — a derived bound, not
    // tolerance widening. (Per-curve owner resolution, as Stage-3's
    // `chord_tol_for_curved_owner` does for selection, is the M5-era
    // refinement; `max` is its conservative envelope.)
    match (input_curved_chord_bound(a), input_curved_chord_bound(b)) {
        (Some(x), Some(y)) => Some(x.max(y)),
        (x, y) => x.or(y),
    }
}

/// PR-YR10 helper: edge-collapse `victim` onto `survivor` in `mesh` + the
/// parallel `attribution`. Replaces every `victim` index with `survivor`, then
/// drops the now-degenerate triangles (two equal indices) from BOTH the mesh
/// and the attribution in lockstep. A proper edge-collapse preserves the
/// watertight half-edge pairing (the two collapsed slivers' surviving directed
/// edges are mutual opposites that cancel — spec §4.5.3 / boolean() sliver rule
/// at the compaction step). The cancellation also covers the COINCIDENT-PAIR
/// form (spec `yang_collapse_membrane_cancellation`): an exact duplicate
/// triangle pair with opposite windings — the pleat spanning the twin gap —
/// is a zero-volume flap whose directed edges pair with each other; both
/// copies are dropped. Returns the number of triangles dropped.
/// N2 §4.4.1 mesh-updating: re-triangulate a degenerate CYLINDER patch in its
/// `(θ, z)` parametric domain, KEEPING every vertex (no geometry moves — the
/// re-CDT only re-connects existing vertices), so a collinear-generator sliver
/// band becomes valid triangles. Returns `Ok(true)` if any patch was re-meshed
/// (the caller re-scans), `Ok(false)` if no eligible patch exists (caller keeps
/// its loud STOP). SCOPED: `Surface::Cylinder` only, and only patches whose θ-span
/// is `< π` (no seam wrap — the full-ring / seam-straddling case is deferred to
/// the periodic-θ closer, spec §5c.5). Any malformed boundary / CDT failure is a
/// loud STOP (`LocalRefinementRequired`), never a silent accept (P9/P10).
///
/// Faithful-ness: this is §4.4.1 CDT re-triangulation. It moves NO vertex, drops
/// none, adds no Steiner point — so it cannot distort neighbour geometry (the
/// R0091 silent-wrong the tolerance-collapse would risk). The watertight/validity
/// re-gate the caller runs after this is the proof gate.
#[allow(clippy::too_many_lines)]
pub(crate) fn replan_degenerate_cylinder_patches(
    mesh: &mut Mesh,
    attr_vec: &mut Vec<Option<TriangleAttribution>>,
    moved: &std::collections::HashSet<u32>,
    brep_a: &BRep,
    brep_b: &BRep,
) -> Result<bool, YangError> {
    use std::collections::BTreeSet;
    let pi = std::f64::consts::PI;
    let is_degen = |t: [u32; 3], mesh: &Mesh| -> bool {
        if !t.iter().any(|v| moved.contains(v)) {
            return false;
        }
        tri_is_degenerate(
            mesh.verts[t[0] as usize].as_array(),
            mesh.verts[t[1] as usize].as_array(),
            mesh.verts[t[2] as usize].as_array(),
        )
    };
    let attr_of =
        |ti: usize| -> Option<TriangleAttribution> { attr_vec.get(ti).copied().flatten() };
    let key_of = |at: TriangleAttribution| (matches!(at.input, InputId::A), at.face);
    let surf_of = |at: TriangleAttribution| -> Surface {
        let br = match at.input {
            InputId::A => brep_a,
            InputId::B => brep_b,
        };
        br.faces()[at.face as usize].surface
    };

    // Attributions carrying a degenerate triangle on a Cylinder face.
    let mut targets: BTreeSet<(bool, u32)> = BTreeSet::new();
    for ti in 0..mesh.tris.len() {
        if is_degen(mesh.tris[ti], mesh) {
            if let Some(at) = attr_of(ti) {
                if matches!(surf_of(at), Surface::Cylinder { .. }) {
                    targets.insert(key_of(at));
                }
            }
        }
    }
    if targets.is_empty() {
        return Ok(false);
    }

    // Copy through every triangle NOT in a target patch; remesh each target.
    let mut new_tris: Vec<[u32; 3]> = Vec::new();
    let mut new_attr: Vec<Option<TriangleAttribution>> = Vec::new();
    for ti in 0..mesh.tris.len() {
        let keep = attr_of(ti).is_none_or(|at| !targets.contains(&key_of(at)));
        if keep {
            new_tris.push(mesh.tris[ti]);
            new_attr.push(attr_of(ti));
        }
    }

    // Global undirected edge → incident-triangle attribution keys (whole mesh).
    // Used to define a patch's TRUE seam: an edge is a patch boundary iff it is
    // shared with a triangle of a DIFFERENT attribution (or is a mesh boundary) —
    // so the re-mesh reproduces the neighbour's chain verbatim (spec §5c.7). This
    // is robust to the zero-area caps: a cap edge shared with another SAME-patch
    // triangle is interior (dropped), only cap edges facing a neighbour are seam.
    type AttrKey = Option<(bool, u32)>;
    let mut global_edge_attrs: std::collections::HashMap<(u32, u32), Vec<AttrKey>> =
        std::collections::HashMap::new();
    for ti in 0..mesh.tris.len() {
        let k = attr_of(ti).map(key_of);
        let tri = mesh.tris[ti];
        for (i, j) in [(0, 1), (1, 2), (2, 0)] {
            let (u, v) = (tri[i], tri[j]);
            let e = if u < v { (u, v) } else { (v, u) };
            global_edge_attrs.entry(e).or_default().push(k);
        }
    }

    let mut remeshed = false;
    for &(is_a, face) in &targets {
        let at = TriangleAttribution {
            input: if is_a { InputId::A } else { InputId::B },
            face,
        };
        let Surface::Cylinder {
            axis_point,
            axis_dir,
            ..
        } = surf_of(at)
        else {
            continue;
        };
        let patch_tris: Vec<u32> = (0..mesh.tris.len() as u32)
            .filter(|&t| attr_of(t as usize).is_some_and(|a| key_of(a) == (is_a, face)))
            .collect();

        // (θ, z) frame.
        let (e1, e2) = ortho_basis(axis_dir);
        let au = normalize3(axis_dir.as_array());
        let o = axis_point.as_array();
        let proj = |v: u32| -> (f64, f64) {
            let p = mesh.verts[v as usize].as_array();
            let r = [p[0] - o[0], p[1] - o[1], p[2] - o[2]];
            let x = r[0] * e1.x() + r[1] * e1.y() + r[2] * e1.z();
            let y = r[0] * e2.x() + r[1] * e2.y() + r[2] * e2.z();
            let z = r[0] * au[0] + r[1] * au[1] + r[2] * au[2];
            (y.atan2(x), z)
        };

        // Shared vertices: incident to ≥1 neighbour (different-attribution)
        // triangle — i.e. genuinely ON the intersection curve, present on both
        // sides. A generator-θ vertex that is NOT shared is a cylinder-only
        // tessellation vertex lying on the (straight) intersection line; the
        // neighbour's coarser chain skips it, so keeping it on our seam tears the
        // seam. Such vertices are collinear-redundant on the generator → DROP.
        let mut shared: std::collections::HashSet<u32> = std::collections::HashSet::new();
        for ti in 0..mesh.tris.len() {
            if attr_of(ti).map(key_of) != Some((is_a, face)) {
                for &v in &mesh.tris[ti] {
                    shared.insert(v);
                }
            }
        }
        // Generator θ values (where the degenerate caps sit).
        let mut gen_theta: Vec<f64> = Vec::new();
        for &t in &patch_tris {
            if is_degen(mesh.tris[t as usize], mesh) {
                for &v in &mesh.tris[t as usize] {
                    let th = proj(v).0;
                    if !gen_theta.iter().any(|g| (g - th).abs() < 1e-9) {
                        gen_theta.push(th);
                    }
                }
            }
        }
        let on_generator = |v: u32| {
            let th = proj(v).0;
            gen_theta.iter().any(|g| (g - th).abs() < 1e-9)
        };

        // Unique patch vertices → local 2D pool (θ unwrapped near a reference).
        // Drop cylinder-only generator vertices (collinear-redundant on the seam).
        let mut vset: BTreeSet<u32> = BTreeSet::new();
        for &t in &patch_tris {
            for &v in &mesh.tris[t as usize] {
                if on_generator(v) && !shared.contains(&v) {
                    continue;
                }
                vset.insert(v);
            }
        }
        if std::env::var_os("YANG_RECDT_PROBE").is_some() {
            for &t in &patch_tris {
                for &v in &mesh.tris[t as usize] {
                    if on_generator(v) {
                        eprintln!(
                            "YANG_RECDT_GENV v={v} shared={} z={:.4}",
                            shared.contains(&v),
                            proj(v).1
                        );
                    }
                }
            }
        }
        let th_ref = proj(*vset.iter().next().unwrap()).0;
        let mut verts2d: Vec<cad_primitives::Point2> = Vec::new();
        let mut global_of_local: Vec<u32> = Vec::new();
        let mut local_of_global: std::collections::HashMap<u32, u32> =
            std::collections::HashMap::new();
        let (mut th_lo, mut th_hi) = (f64::INFINITY, f64::NEG_INFINITY);
        for &v in &vset {
            let (mut th, z) = proj(v);
            th -= th_ref;
            while th > pi {
                th -= 2.0 * pi;
            }
            while th < -pi {
                th += 2.0 * pi;
            }
            th_lo = th_lo.min(th);
            th_hi = th_hi.max(th);
            let l = verts2d.len() as u32;
            local_of_global.insert(v, l);
            global_of_local.push(v);
            verts2d.push(cad_primitives::Point2::new(th, z));
        }
        // Seam-wrap guard: only LOCAL (θ-span < π) patches are in scope.
        if th_hi - th_lo >= pi {
            return Ok(remeshed);
        }

        // TRUE seam boundary via GLOBAL cross-attribution edge sharing (spec
        // §5c.7): an edge of this patch is a boundary edge iff it is NOT shared by
        // exactly two triangles that both carry THIS patch's attribution — i.e. it
        // faces a different-attribution neighbour (the seam) or is a mesh boundary.
        // This takes the neighbour's chain verbatim, so the re-mesh stays exactly
        // conformal, and the zero-area caps' internal edges (both sides this patch)
        // are correctly interior. Collect the seam edges of THIS patch's tris.
        let mykey = (is_a, face);
        let mut seam_edges: std::collections::BTreeSet<(u32, u32)> =
            std::collections::BTreeSet::new();
        for &t in &patch_tris {
            let tri = mesh.tris[t as usize];
            for (i, j) in [(0, 1), (1, 2), (2, 0)] {
                let (u, v) = (tri[i], tri[j]);
                let e = if u < v { (u, v) } else { (v, u) };
                let inc = &global_edge_attrs[&e];
                let all_mine = inc.len() == 2 && inc.iter().all(|k| *k == Some(mykey));
                if !all_mine {
                    seam_edges.insert(e);
                }
            }
        }
        // NO generator-chain reconstruction. An earlier increment z-reconstructed
        // each generator's seam into the fine z-consecutive chain, on the theory
        // that the neighbour shares those vertices and so uses that chain. That is
        // FALSE for the tangency/pinch configuration (R0038, refuted in §5c.10):
        // when a plane is tangent to the cylinder along a generator, the plane's
        // seam edges are NOT the fine z-consecutive chain — the plane connects
        // 14→18 (skipping 21,23), and verts 18,19 are DEGREE-3 on the seam (a
        // pinch where two boundary strands meet). The conformal cylinder seam there
        // is carried by ZERO-AREA caps, which the re-CDT necessarily drops. So the
        // z-reconstruction produced a seam the neighbour does not have (edge 14→21,
        // fwd=1 rev=0) → a non-manifold output caught only downstream.
        //
        // The verbatim cross-attribution `seam_edges` above IS the neighbour's
        // seam (every seam edge is incident to a patch triangle — cap or real — so
        // the patch-edge scan captures all of them). Use it directly. A genuine
        // simple degenerate-cylinder strip yields a clean degree-2 boundary and
        // re-CDTs. A pinched tangency (R0038) yields degree-3 seam vertices and is
        // rejected by the degree-2 boundary gate below — a clean, self-validating
        // LOUD STOP at the right place, never a downstream non-manifold surprise.
        // Local-index boundary adjacency; each boundary vertex must have exactly
        // two boundary neighbours (manifold boundary) or we bail (loud STOP).
        let mut bnd_adj: std::collections::BTreeMap<u32, Vec<u32>> =
            std::collections::BTreeMap::new();
        for &(u, v) in &seam_edges {
            let (lu, lv) = (local_of_global[&u], local_of_global[&v]);
            bnd_adj.entry(lu).or_default().push(lv);
            bnd_adj.entry(lv).or_default().push(lu);
        }
        if std::env::var_os("YANG_RECDT_PROBE").is_some() {
            let bad: Vec<(u32, usize)> = bnd_adj
                .iter()
                .filter(|(_, n)| n.len() != 2)
                .map(|(&v, n)| (global_of_local[v as usize], n.len()))
                .collect();
            eprintln!(
                "YANG_RECDT_SEAM face={face} nverts={} nseam_edges={} nbnd={} bad_degree={:?}",
                verts2d.len(),
                seam_edges.len(),
                bnd_adj.len(),
                bad
            );
            for (&lv, n) in &bnd_adj {
                if n.len() != 2 {
                    let gv = global_of_local[lv as usize];
                    let nbrs: Vec<(u32, f64, f64)> = n
                        .iter()
                        .map(|&ln| {
                            let g = global_of_local[ln as usize];
                            (g, verts2d[ln as usize].x(), verts2d[ln as usize].y())
                        })
                        .collect();
                    eprintln!(
                        "  bad v{gv} (θ,z)={:?} seam_nbrs={nbrs:?}",
                        verts2d[lv as usize]
                    );
                }
            }
        }
        if bnd_adj.is_empty() || bnd_adj.values().any(|n| n.len() != 2) {
            return Err(YangError::stage4_region_invalid(
                u32::MAX,
                Stage4InvalidReason::LocalRefinementRequired,
            ));
        }
        // Walk the boundary edges into closed loops.
        let mut loops_local: Vec<Vec<u32>> = Vec::new();
        let mut seen: std::collections::HashSet<u32> = std::collections::HashSet::new();
        for &start in bnd_adj.keys() {
            if seen.contains(&start) {
                continue;
            }
            let mut lp = vec![start];
            seen.insert(start);
            let mut prev = start;
            let mut cur = bnd_adj[&start][0];
            while cur != start {
                if !seen.insert(cur) {
                    // revisited a non-start vertex → tangled boundary, bail.
                    return Err(YangError::stage4_region_invalid(
                        u32::MAX,
                        Stage4InvalidReason::LocalRefinementRequired,
                    ));
                }
                lp.push(cur);
                let nb = &bnd_adj[&cur];
                let next = if nb[0] == prev { nb[1] } else { nb[0] };
                prev = cur;
                cur = next;
            }
            loops_local.push(lp);
        }
        // Fig-11(a): a shared generator/intersection vertex whose ONLY incident
        // triangles were degenerate caps is missing from the non-degenerate
        // boundary above, yet it lies ON a boundary edge and is shared with the
        // neighbouring patch across the intersection curve — it MUST stay on the
        // boundary or the seam tears (non-manifold). Insert every such
        // interior-but-on-a-boundary-edge vertex into the boundary chain (split
        // the constraint edge at it), iterating so multiple collinear inserts on
        // one edge each find their sub-edge.
        loop {
            let on_bnd: BTreeSet<u32> = loops_local.iter().flatten().copied().collect();
            let mut inserted = false;
            for vi in 0..verts2d.len() as u32 {
                if on_bnd.contains(&vi) {
                    continue;
                }
                let p = verts2d[vi as usize];
                'find: for lp in &mut loops_local {
                    for i in 0..lp.len() {
                        let a = verts2d[lp[i] as usize];
                        let b = verts2d[lp[(i + 1) % lp.len()] as usize];
                        let ab = (b.x() - a.x(), b.y() - a.y());
                        let ap = (p.x() - a.x(), p.y() - a.y());
                        let cross = ab.0 * ap.1 - ab.1 * ap.0;
                        let len2 = ab.0 * ab.0 + ab.1 * ab.1;
                        let dot = ab.0 * ap.0 + ab.1 * ap.1;
                        // Collinear (area of a-b-p ≈ 0 vs the edge length) AND
                        // strictly between a and b.
                        if len2 > 0.0 && cross.abs() <= 1e-9 * len2 && dot > 0.0 && dot < len2 {
                            lp.insert(i + 1, vi);
                            inserted = true;
                            break 'find;
                        }
                    }
                }
            }
            if !inserted {
                break;
            }
        }
        let signed_area = |lp: &[u32]| -> f64 {
            let mut a = 0.0;
            for i in 0..lp.len() {
                let p = verts2d[lp[i] as usize];
                let q = verts2d[lp[(i + 1) % lp.len()] as usize];
                a += p.x() * q.y() - q.x() * p.y();
            }
            a * 0.5
        };
        let outer_i = (0..loops_local.len())
            .max_by(|&x, &y| {
                signed_area(&loops_local[x])
                    .abs()
                    .partial_cmp(&signed_area(&loops_local[y]).abs())
                    .unwrap()
            })
            .unwrap();
        let outer = loops_local[outer_i].clone();
        let holes: Vec<Vec<u32>> = loops_local
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != outer_i)
            .map(|(_, l)| l.clone())
            .collect();
        let bnd: BTreeSet<u32> = loops_local.iter().flatten().copied().collect();
        let interior: Vec<u32> = (0..verts2d.len() as u32)
            .filter(|l| !bnd.contains(l))
            .collect();
        if std::env::var_os("YANG_RECDT_PROBE").is_some() {
            let interior_g: Vec<u32> = interior
                .iter()
                .map(|&l| global_of_local[l as usize])
                .collect();
            eprintln!(
                "YANG_RECDT face={face} nverts={} nloops={} outer_len={} n_interior={} interior_g={:?}",
                verts2d.len(),
                loops_local.len(),
                loops_local.iter().map(|l| l.len()).max().unwrap_or(0),
                interior.len(),
                interior_g,
            );
        }

        let tris_local =
            cherchi_rs::cdt_polygon_with_holes_keep_interior(&verts2d, &outer, &holes, &interior)
                .map_err(|_| {
                YangError::stage4_region_invalid(
                    u32::MAX,
                    Stage4InvalidReason::LocalRefinementRequired,
                )
            })?;

        // Reference winding sign: align to the patch's existing non-degenerate
        // triangles (robust to inward/outward cylinder faces).
        let radial_at = |g: [u32; 3]| -> [f64; 3] {
            let c = [
                (mesh.verts[g[0] as usize].x()
                    + mesh.verts[g[1] as usize].x()
                    + mesh.verts[g[2] as usize].x())
                    / 3.0,
                (mesh.verts[g[0] as usize].y()
                    + mesh.verts[g[1] as usize].y()
                    + mesh.verts[g[2] as usize].y())
                    / 3.0,
                (mesh.verts[g[0] as usize].z()
                    + mesh.verts[g[1] as usize].z()
                    + mesh.verts[g[2] as usize].z())
                    / 3.0,
            ];
            let r = [c[0] - o[0], c[1] - o[1], c[2] - o[2]];
            let axl = r[0] * au[0] + r[1] * au[1] + r[2] * au[2];
            [r[0] - axl * au[0], r[1] - axl * au[1], r[2] - axl * au[2]]
        };
        let mut ref_sign = 0.0_f64;
        for &t in &patch_tris {
            let g = mesh.tris[t as usize];
            if is_degen(g, mesh) {
                continue;
            }
            let av = tri_area_vector(
                mesh.verts[g[0] as usize].as_array(),
                mesh.verts[g[1] as usize].as_array(),
                mesh.verts[g[2] as usize].as_array(),
            );
            let rad = radial_at(g);
            ref_sign += av[0] * rad[0] + av[1] * rad[1] + av[2] * rad[2];
        }
        let ref_sign = if ref_sign >= 0.0 { 1.0 } else { -1.0 };

        for tl in tris_local {
            let mut g = [
                global_of_local[tl[0] as usize],
                global_of_local[tl[1] as usize],
                global_of_local[tl[2] as usize],
            ];
            let av = tri_area_vector(
                mesh.verts[g[0] as usize].as_array(),
                mesh.verts[g[1] as usize].as_array(),
                mesh.verts[g[2] as usize].as_array(),
            );
            let rad = radial_at(g);
            let dot = av[0] * rad[0] + av[1] * rad[1] + av[2] * rad[2];
            if dot * ref_sign < 0.0 {
                g.swap(1, 2);
            }
            new_tris.push(g);
            new_attr.push(Some(at));
        }
        remeshed = true;
    }

    if remeshed {
        mesh.tris = new_tris;
        *attr_vec = new_attr;
    }
    Ok(remeshed)
}

/// Probe-only (`YANG_S4_BALANCE_PROBE`): census the mesh's unbalanced undirected
/// edges (`fwd != rev`) at a named checkpoint. Reports the count and the sorted
/// edge list, so the SAME census can be compared across checkpoints — the only
/// way to tell a defect a downstream gate MASKED from one an intervening pass
/// MINTED. Read-only; no effect when the env is unset.
fn balance_census(mesh: &Mesh, checkpoint: &str) {
    if std::env::var_os("YANG_S4_BALANCE_PROBE").is_none() {
        return;
    }
    let mut dir: std::collections::BTreeMap<(u32, u32), i32> = std::collections::BTreeMap::new();
    for tri in &mesh.tris {
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            *dir.entry((tri[i], tri[j])).or_insert(0) += 1;
        }
    }
    let mut bad: Vec<String> = Vec::new();
    for (&(s, e), &fwd) in &dir {
        if s >= e {
            continue;
        }
        let rev = dir.get(&(e, s)).copied().unwrap_or(0);
        if fwd != rev {
            bad.push(format!("({s},{e}):{fwd}/{rev}"));
        }
    }
    eprintln!(
        "YANG_S4_BALANCE at={checkpoint} tris={} verts={} unbalanced={} {}",
        mesh.tris.len(),
        mesh.verts.len(),
        bad.len(),
        bad.join(" ")
    );
}

/// A mutual degenerate pair (spec `yang_n2_stage4_cdt_mesh_updating.md` §5c.11):
/// both incident triangles of one long edge are degenerate and report that SAME
/// edge as their long edge — a zero-area quad astride `a–c` with the two
/// off-vertices `bl`/`bh` interleaved strictly inside the segment
/// (`0 < t(bl) < t(bh) < 1` along `a→c`). `nl`/`nh` are the OUTER neighbours
/// across the two insertion edges `(bl,c)` / `(a,bh)`, both non-degenerate.
struct MutualPair {
    t1: usize,
    t2: usize,
    nl: usize,
    nh: usize,
    a: u32,
    c: u32,
    bl: u32,
    bh: u32,
}

/// Validate the mutual-pair configuration for degenerate triangle `ti` whose
/// long-edge neighbour `n` is also degenerate. `None` unless: `n`'s long edge is
/// the SAME edge `(a,c)`; the off vertices are distinct and strictly interleaved
/// inside the open segment; and both insertion edges have exactly two incident
/// triangles whose outer member (not `ti`/`n`) is non-degenerate. Anything else
/// keeps the loud STOP (honest deferral, no partial action).
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn mutual_pair_candidate(
    mesh: &Mesh,
    edge_tris: &std::collections::HashMap<(u32, u32), Vec<u32>>,
    is_degen: &dyn Fn(usize, &Mesh) -> bool,
    long_edge_off: &dyn Fn(&[u32; 3], &Mesh) -> (u32, u32, u32),
    ti: usize,
    n: usize,
    a: u32,
    c: u32,
    b: u32,
) -> Option<MutualPair> {
    let (na, nc, nb) = long_edge_off(&mesh.tris[n], mesh);
    let key = if a < c { (a, c) } else { (c, a) };
    let nkey = if na < nc { (na, nc) } else { (nc, na) };
    if nkey != key || nb == b {
        return None;
    }
    let pa = mesh.verts[a as usize].as_array();
    let pc = mesh.verts[c as usize].as_array();
    let e = [pc[0] - pa[0], pc[1] - pa[1], pc[2] - pa[2]];
    let l2 = e[0] * e[0] + e[1] * e[1] + e[2] * e[2];
    if l2 == 0.0 {
        return None;
    }
    let tof = |v: u32| {
        let p = mesh.verts[v as usize].as_array();
        ((p[0] - pa[0]) * e[0] + (p[1] - pa[1]) * e[1] + (p[2] - pa[2]) * e[2]) / l2
    };
    let (tb, tnb) = (tof(b), tof(nb));
    let (bl, bh, tl, th) = if tb <= tnb {
        (b, nb, tb, tnb)
    } else {
        (nb, b, tnb, tb)
    };
    // Equal parameters give no deterministic chain order — keep the STOP.
    if !(0.0 < tl && tl < th && th < 1.0) {
        return None;
    }
    let outer = |u: u32, v: u32| -> Option<usize> {
        let k = if u < v { (u, v) } else { (v, u) };
        let list = edge_tris.get(&k)?;
        if list.len() != 2 {
            return None;
        }
        let o = list
            .iter()
            .map(|&x| x as usize)
            .find(|&x| x != ti && x != n)?;
        if is_degen(o, mesh) {
            return None;
        }
        Some(o)
    };
    let nl = outer(bl, c)?;
    let nh = outer(a, bh)?;
    // Two-sidedness. The §5c.11 watertightness argument assumes `nl` and `nh`
    // lie on OPPOSITE sides of the degenerate quad, so the four split pieces are
    // distinct and every chain edge pairs one piece from each side. When both
    // outer neighbours share their third vertex they are instead the SAME fan
    // over the chain: `nl`'s first piece `[bl,bh,dd]` and `nh`'s second piece
    // `[bl,bh,dd]` are then the identical triangle, so the update emits it TWICE
    // — a double cover that does not bound a 2-manifold. Measured on R0038
    // (dd = 17 on both sides): the doubled piece propagated a foreign vertex
    // into an unrelated planar face's loop and surfaced as `NonManifoldOutput`
    // two stages downstream, 7.5e0 off that face's plane. Exact index equality,
    // no tolerance — it is precisely the condition under which the four pieces
    // fail to be distinct. Keep the loud STOP (honest deferral, no partial
    // action): the same-apex fan is a DIFFERENT configuration needing its own
    // repair (a 3-triangle refan of the chain), not this arm.
    let third = |t: usize, u: u32, v: u32| -> Option<u32> {
        mesh.tris[t].iter().copied().find(|&x| x != u && x != v)
    };
    if third(nl, bl, c)? == third(nh, a, bh)? {
        return None;
    }
    Some(MutualPair {
        t1: ti,
        t2: n,
        nl,
        nh,
        a,
        c,
        bl,
        bh,
    })
}

/// Execute the mutual-pair update (spec §5c.11): drop both zero-area members and
/// Fig-11(a)-split the two outer neighbours — `nl` (across `(bl,c)`) at `bh`,
/// `nh` (across `(a,bh)`) at `bl` — so both sides of the former quad carry the
/// identical vertex chain `a–bl–bh–c`. Pure connectivity: no vertex moves, none
/// is added or dropped; each split piece inherits its parent's winding (restored
/// against the parent's area normal) and attribution. Watertight by
/// construction: the long edge `(a,c)` and both insertion edges vanish together
/// with their two incident triangles each, and every chain edge pairs one piece
/// from each side.
fn resolve_mutual_degenerate_pair(
    mesh: &mut Mesh,
    attr_vec: &mut Vec<Option<TriangleAttribution>>,
    m: &MutualPair,
) {
    let split = |parent: [u32; 3], u: u32, v: u32, ins: u32| -> ([u32; 3], [u32; 3]) {
        let dd = parent
            .iter()
            .copied()
            .find(|&x| x != u && x != v)
            .expect("split parent shares the insertion edge, has a third vertex");
        let norm = tri_area_vector(
            mesh.verts[parent[0] as usize].as_array(),
            mesh.verts[parent[1] as usize].as_array(),
            mesh.verts[parent[2] as usize].as_array(),
        );
        let mut p1 = [u, ins, dd];
        let mut p2 = [ins, v, dd];
        orient_tri(&mesh.verts, &mut p1, norm);
        orient_tri(&mesh.verts, &mut p2, norm);
        (p1, p2)
    };
    let (l1, l2) = split(mesh.tris[m.nl], m.bl, m.c, m.bh);
    let (h1, h2) = split(mesh.tris[m.nh], m.a, m.bh, m.bl);
    let nl_attr = attr_vec.get(m.nl).copied().flatten();
    let nh_attr = attr_vec.get(m.nh).copied().flatten();
    let drop = [m.t1, m.t2, m.nl, m.nh];
    let mut new_tris: Vec<[u32; 3]> = Vec::with_capacity(mesh.tris.len());
    let mut new_attr: Vec<Option<TriangleAttribution>> = Vec::with_capacity(attr_vec.len());
    for (i, t) in mesh.tris.iter().enumerate() {
        if drop.contains(&i) {
            continue;
        }
        new_tris.push(*t);
        new_attr.push(attr_vec.get(i).copied().flatten());
    }
    for (t, at) in [(l1, nl_attr), (l2, nl_attr), (h1, nh_attr), (h2, nh_attr)] {
        new_tris.push(t);
        new_attr.push(at);
    }
    *mesh = Mesh::new(std::mem::take(&mut mesh.verts), new_tris);
    *attr_vec = new_attr;
}

/// #169 Phase B — the §4.4.1 mesh-update splice for the non-manifold reassembly
/// bucket. For each patch flagged by [`detect_nonmanifold_seams`] whose defect is
/// a spurious/overlapping triangle (F0082: `tri1217` doubling a seam edge inside
/// one planar patch), re-triangulate that patch's INTERIOR while keeping its
/// TRUE boundary verbatim — dropping the overlap. This is `replan`'s keep-interior
/// CDT generalized from degenerate-cylinder-caps to any charted patch, triggered
/// by the detector.
///
/// The boundary is built from the patch edges shared with a DIFFERENT-attribution
/// triangle (the genuine cross-face seam); a spurious single-incidence edge has
/// no different-key partner and is excluded — that is exactly what removes the
/// overlap. Keep-interior re-CDT moves NO geometry (the shared seam verts stay
/// put → the neighbour still pairs, so it is inherently two-sided-conformal and
/// P10-safe: a malformed boundary is a loud STOP, never a silent-wrong).
///
/// Scope of this increment: PLANE patches only (the F0082/R0095-plane subset).
/// Regions with >2 patches (3-patch junctions like C0044), a non-plane patch, or
/// a chartless surface are skipped — the mesh is left as-is for the loud gate.
/// Returns `Ok(true)` iff at least one patch was re-triangulated.
pub(crate) fn remesh_nonmanifold_patches(
    mesh: &mut Mesh,
    attr_vec: &mut Vec<Option<TriangleAttribution>>,
    brep_a: &BRep,
    brep_b: &BRep,
) -> Result<bool, YangError> {
    use std::collections::{BTreeMap, BTreeSet, HashMap};
    let probe = std::env::var_os("YANG_MESHUP_RECDT").is_some();

    let attr_key = |ti: usize| -> Option<(bool, u32)> {
        attr_vec
            .get(ti)
            .copied()
            .flatten()
            .map(|at| (matches!(at.input, InputId::A), at.face))
    };
    let surf_of = |k: (bool, u32)| -> Surface {
        let br = if k.0 { brep_a } else { brep_b };
        br.faces()[k.1 as usize].surface
    };

    // (1) Failure regions → target patch keys. Only 2-patch regions whose BOTH
    // patches are Planes (this increment's scope) contribute; junctions (>2) and
    // non-plane/chartless patches are skipped (left for the loud gate).
    let regions = crate::stage4_project::detect_nonmanifold_seams(&mesh.tris, &attr_key);
    if regions.is_empty() {
        return Ok(false);
    }
    let mut targets: BTreeSet<(bool, u32)> = BTreeSet::new();
    for r in &regions {
        if r.keys.len() != 2 {
            continue;
        }
        if !r
            .keys
            .iter()
            .all(|&k| matches!(surf_of(k), Surface::Plane { .. }))
        {
            continue;
        }
        for &k in &r.keys {
            targets.insert(k);
        }
    }
    if targets.is_empty() {
        return Ok(false);
    }

    // (2) Global undirected edge → incident-triangle attribution keys (whole
    // mesh), for the cross-attribution seam test.
    type AttrKey = Option<(bool, u32)>;
    let mut edge_keys: HashMap<(u32, u32), Vec<AttrKey>> = HashMap::new();
    for ti in 0..mesh.tris.len() {
        let k = attr_key(ti);
        let tri = mesh.tris[ti];
        for (i, j) in [(0, 1), (1, 2), (2, 0)] {
            let (u, v) = (tri[i], tri[j]);
            let e = if u < v { (u, v) } else { (v, u) };
            edge_keys.entry(e).or_default().push(k);
        }
    }

    // (3) Copy through every triangle NOT in a target patch; remesh each target.
    let mut new_tris: Vec<[u32; 3]> = Vec::new();
    let mut new_attr: Vec<Option<TriangleAttribution>> = Vec::new();
    for ti in 0..mesh.tris.len() {
        if attr_key(ti).is_none_or(|k| !targets.contains(&k)) {
            new_tris.push(mesh.tris[ti]);
            new_attr.push(attr_vec.get(ti).copied().flatten());
        }
    }

    let mut remeshed = false;
    for &mykey in &targets {
        let surf = surf_of(mykey);
        let Some(chart) = crate::stage4_project::SurfaceChart::new(surf) else {
            // Chartless: leave the patch's triangles in place (copy them back).
            for ti in 0..mesh.tris.len() {
                if attr_key(ti) == Some(mykey) {
                    new_tris.push(mesh.tris[ti]);
                    new_attr.push(attr_vec.get(ti).copied().flatten());
                }
            }
            continue;
        };
        let at = TriangleAttribution {
            input: if mykey.0 { InputId::A } else { InputId::B },
            face: mykey.1,
        };
        let patch_tris: Vec<u32> = (0..mesh.tris.len() as u32)
            .filter(|&t| attr_key(t as usize) == Some(mykey))
            .collect();

        // Local 2D pool via the chart (unique patch verts).
        let mut vset: BTreeSet<u32> = BTreeSet::new();
        for &t in &patch_tris {
            for &v in &mesh.tris[t as usize] {
                vset.insert(v);
            }
        }
        let mut verts2d: Vec<cad_primitives::Point2> = Vec::with_capacity(vset.len());
        let mut global_of_local: Vec<u32> = Vec::with_capacity(vset.len());
        let mut local_of_global: HashMap<u32, u32> = HashMap::new();
        for &v in &vset {
            local_of_global.insert(v, verts2d.len() as u32);
            global_of_local.push(v);
            verts2d.push(chart.project(mesh.verts[v as usize]));
        }

        // TRUE seam boundary: a patch edge shared with a DIFFERENT-attribution
        // triangle. A spurious single-incidence edge (the overlap's dangling
        // edge) has no different-key partner → excluded → the overlap is dropped.
        let mut seam_edges: BTreeSet<(u32, u32)> = BTreeSet::new();
        for &t in &patch_tris {
            let tri = mesh.tris[t as usize];
            for (i, j) in [(0, 1), (1, 2), (2, 0)] {
                let (u, v) = (tri[i], tri[j]);
                let e = if u < v { (u, v) } else { (v, u) };
                if edge_keys[&e].iter().any(|k| *k != Some(mykey)) {
                    seam_edges.insert(e);
                }
            }
        }

        // Boundary adjacency; every boundary vertex must have exactly two
        // boundary neighbours (a manifold boundary) or bail (loud STOP).
        let mut bnd_adj: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
        for &(u, v) in &seam_edges {
            let (lu, lv) = (local_of_global[&u], local_of_global[&v]);
            bnd_adj.entry(lu).or_default().push(lv);
            bnd_adj.entry(lv).or_default().push(lu);
        }
        if probe {
            let bad: Vec<(u32, usize)> = bnd_adj
                .iter()
                .filter(|(_, n)| n.len() != 2)
                .map(|(&v, n)| (global_of_local[v as usize], n.len()))
                .collect();
            eprintln!(
                "YANG_MESHUP_RECDT face={:?} nverts={} nseam={} nbnd={} bad_degree={:?}",
                mykey,
                verts2d.len(),
                seam_edges.len(),
                bnd_adj.len(),
                bad
            );
        }
        if bnd_adj.is_empty() || bnd_adj.values().any(|n| n.len() != 2) {
            return Err(YangError::stage4_region_invalid(
                u32::MAX,
                Stage4InvalidReason::LocalRefinementRequired,
            ));
        }

        // Walk the boundary edges into closed loops.
        let mut loops_local: Vec<Vec<u32>> = Vec::new();
        let mut seen: std::collections::HashSet<u32> = std::collections::HashSet::new();
        for &start in bnd_adj.keys() {
            if seen.contains(&start) {
                continue;
            }
            let mut lp = vec![start];
            seen.insert(start);
            let mut prev = start;
            let mut cur = bnd_adj[&start][0];
            while cur != start {
                if !seen.insert(cur) {
                    return Err(YangError::stage4_region_invalid(
                        u32::MAX,
                        Stage4InvalidReason::LocalRefinementRequired,
                    ));
                }
                lp.push(cur);
                let nb = &bnd_adj[&cur];
                let next = if nb[0] == prev { nb[1] } else { nb[0] };
                prev = cur;
                cur = next;
            }
            loops_local.push(lp);
        }

        // Probe: flag near-collinear boundary triples (a spike/notch that a
        // keep-interior CDT cannot triangulate cleanly — the F0082 588/591/601
        // diagnosis). Reports the sharpest triple per patch with its 2D coords.
        if probe {
            let mut worst: Option<(f64, [u32; 3], [cad_primitives::Point2; 3])> = None;
            for lp in &loops_local {
                let m = lp.len();
                for i in 0..m {
                    let (a, b, c) = (
                        lp[i] as usize,
                        lp[(i + 1) % m] as usize,
                        lp[(i + 2) % m] as usize,
                    );
                    let (pa, pb, pc) = (verts2d[a], verts2d[b], verts2d[c]);
                    let area2 = ((pb.x() - pa.x()) * (pc.y() - pa.y())
                        - (pc.x() - pa.x()) * (pb.y() - pa.y()))
                    .abs();
                    if worst.is_none_or(|(w, _, _)| area2 < w) {
                        worst = Some((
                            area2,
                            [global_of_local[a], global_of_local[b], global_of_local[c]],
                            [pa, pb, pc],
                        ));
                    }
                }
            }
            if let Some((area2, gv, p2)) = worst {
                eprintln!(
                    "YANG_MESHUP_RECDT face={mykey:?} sharpest_triple gverts={gv:?} 2xarea={area2:.3e} p2d={p2:?}"
                );
            }
        }

        // Outer loop = the largest |signed area|; the rest are holes.
        let signed_area = |lp: &[u32]| -> f64 {
            let mut a = 0.0;
            for i in 0..lp.len() {
                let p = verts2d[lp[i] as usize];
                let q = verts2d[lp[(i + 1) % lp.len()] as usize];
                a += p.x() * q.y() - q.x() * p.y();
            }
            a * 0.5
        };
        let outer_i = (0..loops_local.len())
            .max_by(|&x, &y| {
                signed_area(&loops_local[x])
                    .abs()
                    .partial_cmp(&signed_area(&loops_local[y]).abs())
                    .unwrap()
            })
            .unwrap();
        let outer = loops_local[outer_i].clone();
        let holes: Vec<Vec<u32>> = loops_local
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != outer_i)
            .map(|(_, l)| l.clone())
            .collect();
        let bnd: BTreeSet<u32> = loops_local.iter().flatten().copied().collect();
        let interior: Vec<u32> = (0..verts2d.len() as u32)
            .filter(|l| !bnd.contains(l))
            .collect();

        let tris_local =
            cherchi_rs::cdt_polygon_with_holes_keep_interior(&verts2d, &outer, &holes, &interior)
                .map_err(|_| {
                YangError::stage4_region_invalid(
                    u32::MAX,
                    Stage4InvalidReason::LocalRefinementRequired,
                )
            })?;

        // Winding: align new triangles to the patch's existing net normal, so the
        // re-meshed patch keeps the operand's outward orientation.
        let plane_n = match surf {
            Surface::Plane { normal, .. } => normal.as_array(),
            _ => unreachable!("only Plane patches reach here"),
        };
        let mut ref_sign = 0.0_f64;
        for &t in &patch_tris {
            let g = mesh.tris[t as usize];
            let av = tri_area_vector(
                mesh.verts[g[0] as usize].as_array(),
                mesh.verts[g[1] as usize].as_array(),
                mesh.verts[g[2] as usize].as_array(),
            );
            ref_sign += av[0] * plane_n[0] + av[1] * plane_n[1] + av[2] * plane_n[2];
        }
        let ref_sign = if ref_sign >= 0.0 { 1.0 } else { -1.0 };
        for tl in tris_local {
            let mut g = [
                global_of_local[tl[0] as usize],
                global_of_local[tl[1] as usize],
                global_of_local[tl[2] as usize],
            ];
            let av = tri_area_vector(
                mesh.verts[g[0] as usize].as_array(),
                mesh.verts[g[1] as usize].as_array(),
                mesh.verts[g[2] as usize].as_array(),
            );
            let dot = av[0] * plane_n[0] + av[1] * plane_n[1] + av[2] * plane_n[2];
            if dot * ref_sign < 0.0 {
                g.swap(1, 2);
            }
            new_tris.push(g);
            new_attr.push(Some(at));
        }
        remeshed = true;
    }

    if remeshed {
        mesh.tris = new_tris;
        *attr_vec = new_attr;
    }
    Ok(remeshed)
}

pub(crate) fn collapse_vertex(
    mesh: &mut Mesh,
    attribution: &mut Vec<Option<TriangleAttribution>>,
    victim: u32,
    survivor: u32,
) -> usize {
    let mut new_tris: Vec<[u32; 3]> = Vec::with_capacity(mesh.tris.len());
    let mut new_attr: Vec<Option<TriangleAttribution>> = Vec::with_capacity(attribution.len());
    let mut dropped = 0usize;
    for (t, tri) in mesh.tris.iter().enumerate() {
        let mapped = [
            if tri[0] == victim { survivor } else { tri[0] },
            if tri[1] == victim { survivor } else { tri[1] },
            if tri[2] == victim { survivor } else { tri[2] },
        ];
        if mapped[0] == mapped[1] || mapped[1] == mapped[2] || mapped[2] == mapped[0] {
            dropped += 1;
            continue;
        }
        new_tris.push(mapped);
        new_attr.push(attribution.get(t).copied().flatten());
    }
    // Membrane cancellation (spec `yang_collapse_membrane_cancellation`):
    // identifying `victim` with `survivor` can turn the two-triangle pleat
    // that spanned the twin gap into an EXACT duplicate pair with OPPOSITE
    // windings — a zero-volume doubled flap whose 6 directed edges are 3
    // mutual-reverse pairs (they pair with EACH OTHER). Dropping BOTH
    // preserves the watertight half-edge pairing and restores manifold
    // count-2 on the shared fan edges (the measured F0059 mint: the PR-KV9
    // junction-twin collapse at the Steinmetz seam apex derailed the Stage-6
    // wedge walk). Same-winding duplicates and ≥3-copy groups are genuine
    // non-manifold configurations — left untouched for the downstream loud
    // STOPs (P9: never silently pick).
    {
        let mut by_triple: std::collections::HashMap<[u32; 3], Vec<usize>> =
            std::collections::HashMap::new();
        for (t, tri) in new_tris.iter().enumerate() {
            let mut s = *tri;
            s.sort_unstable();
            by_triple.entry(s).or_default().push(t);
        }
        // Cyclic-winding key: rotate the smallest index to the front; equal
        // keys ⇔ same winding.
        let winding_key = |tri: [u32; 3]| -> [u32; 3] {
            let k = (0..3).min_by_key(|&i| tri[i]).expect("3 verts");
            [tri[k], tri[(k + 1) % 3], tri[(k + 2) % 3]]
        };
        let mut cancel: std::collections::BTreeSet<usize> = std::collections::BTreeSet::new();
        for ts in by_triple.values() {
            if ts.len() != 2 {
                continue;
            }
            let (x, y) = (ts[0], ts[1]);
            if winding_key(new_tris[x]) != winding_key(new_tris[y]) {
                cancel.insert(x);
                cancel.insert(y);
                if std::env::var_os("YANG_DOUBLECOVER_PROBE").is_some() {
                    eprintln!(
                        "[membrane-cancel] dropping opposite-winding dup pair tris {x},{y} = \
                         {:?}/{:?} (victim={victim} survivor={survivor})",
                        new_tris[x], new_tris[y]
                    );
                }
            }
        }
        if !cancel.is_empty() {
            let keep: Vec<usize> = (0..new_tris.len())
                .filter(|t| !cancel.contains(t))
                .collect();
            new_tris = keep.iter().map(|&t| new_tris[t]).collect();
            new_attr = keep.iter().map(|&t| new_attr[t]).collect();
            dropped += cancel.len();
        }
    }
    *mesh = Mesh::new(std::mem::take(&mut mesh.verts), new_tris);
    *attribution = new_attr;
    // EXPERIMENTAL probe (task #121, read-only, env-gated): did THIS collapse
    // mint a duplicate (double-cover) triangle pair?
    if std::env::var_os("YANG_DOUBLECOVER_PROBE").is_some() {
        use std::collections::HashMap;
        let mut by_triple: HashMap<[u32; 3], Vec<usize>> = HashMap::new();
        for (t, tri) in mesh.tris.iter().enumerate() {
            let mut s = *tri;
            s.sort_unstable();
            by_triple.entry(s).or_default().push(t);
        }
        for (key, ts) in &by_triple {
            if ts.len() > 1 {
                eprintln!(
                    "[doublecover-collapse] victim={victim} survivor={survivor} \
                     dup triple {key:?} tris {ts:?} windings {:?}",
                    ts.iter().map(|&t| mesh.tris[t]).collect::<Vec<_>>()
                );
            }
        }
    }
    dropped
}

// N47 (spec `yang_n47_coincident_moved_weld`): weld coincident RELOCATED
// vertices before topology emission.
//
// Two vertices this pipeline pushed onto an analytic curve (`moved`) can
// Newton-converge to within the MODEL coincidence tolerance
// `TAU_MODEL·(1+scale)` — they are the SAME geometric point emitted twice (a
// near-tangent seam crossing whose two arrangement vertices both project onto
// one intersection point). Emitted distinct, they become a sub-render-precision
// output edge that trips kernel-v2's G1 render-collapse gate far downstream.
//
// The four non-compliant vertex welds are RETIRED (#169 weld-retirement track,
// audit 2026-07-16): they are OFF in production. Each was a tolerance hack
// (violating the Cherchi B6 "never a tolerance weld" invariant) that masked
// upstream near-coincident minting; a case that only stayed CORRECT via such a
// weld was a false green by project intent (Yang-paper compliance is the north
// star), and retiring it can only expose a loud STOP, never a silent-wrong. The
// measured cost of turning all four off was 13 cases (241C → 228C, 0 WRONG).
//
// **Update (N55/N56): the audit was wrong for THREE of the four.** The correct
// test is "is it a Yang paper operation?", not "does it use a tolerance." The
// paper prescribes tolerance-gated merges, and those are desired:
// - `subfeature` = Yang §4.4.1(b) (Fig-11(b) "merge p with q if too close"),
//   retightened to `TAU_WORK·(1+scale)` (`is_relocation_coincidence`) →
//   compliant always-on merge (N55). Recovers R0055/F0056/F0057/F0059.
// - `coincident` = Yang §4.3 ("remove a point too close to another on the same
//   loop"; both verts relocated onto the curve) → reinstated always-on (N56).
//   0-conversion (near-tangency infra for #137) but genuine paper machinery.
// - `subres` = Yang §4.3 (sub-resolution intersection-curve segment collapse),
//   retightened from the absolute floor to `TAU_MODEL·(1+scale)` → reinstated
//   always-on (N56). Recovers R0076/R0088/F0078/F0079/F0084 — and, combined
//   with `coincident`, the render twins R0012/R0098/F0090.
//
// Net: 12 of the 13 retired cases recover COMPLIANTLY (228C → 240C, 0 WRONG);
// only R0072 stays a loud STOP (a real ~1e-7 micro-scale collapse → curved
// re-CDT). The last gated arm — `f32`, the sole confirmed hack (it keyed on
// f32 RENDER precision, not geometry; it is nowhere in the paper; it
// REGRESSES C0036; it was redundant since the §4.3 dedup recovers its
// cases) — was kept callable behind `YANG_WELD_ENABLE` as a historical A/B
// artifact until the §4.4.1 epic's **I4-1 (2026-08-15) removed the arm and
// the `weld_enabled` gate entirely** (production had it off by default
// since the audit, so the removal is byte-identical by construction).
// `weld_f32_render_twins` survives below as a unit-tested banked primitive.

/// Yang §4.4.1(b) same-point test (deviation N55): two relocated endpoints
/// `len` apart at local magnitude `scale` (= max |coord| of the pair) are the
/// SAME intersection point — a numerical coincidence eligible for the Fig-11(b)
/// merge — iff their separation is below the scale-relative WORKING tolerance
/// `TAU_WORK·(1+scale)`.
///
/// This is the COMPLIANT criterion that replaced the retired `subfeature` weld's
/// absolute `MIN_FEATURE_SIZE` floor. The distinction is load-bearing: the
/// absolute floor merged BOTH machine-ε relocation twins (exact duplicates —
/// which the compliance ratchet keeps) AND genuine sub-feature edges at
/// micro-scale (R0072's ~1e-7 collapse = 0.4 % of a ~2e-4 span — the R0091
/// silent-wrong hazard). The `TAU_WORK` band (5 orders tighter than
/// `MIN_FEATURE_SIZE`) admits only the former: a numerically-identical pair
/// merges, a real sub-feature edge stays a loud STOP (→ curved re-CDT).
pub(crate) fn is_relocation_coincidence(len: f64, scale: f64) -> bool {
    len < cad_primitives::TAU_WORK * (1.0 + scale)
}

/// Band: the scale-relative model coincidence tolerance (`scale` = max |coord| of
/// the pair) — the SAME band every other coincidence test uses, 10× tighter than
/// the `MIN_FEATURE_SIZE·(1+scale)` feature floor, so it admits ONLY
/// sub-(feature/10) coincidences. Restricted to `moved`×`moved` and
/// `moved`×`minted` pairs: it never touches un-relocated arrangement geometry
/// `boolean()` kept for watertightness (cf. the §4.4.1(b) micro-scale R0091
/// revert — P9/P10). `collapse_vertex` is the proven watertight-preserving
/// edge-collapse; iterate to a fixed point over live (still-referenced)
/// vertices. Returns whether any pair welded.
///
/// P3b inc-4a (R0061): `minted` = Stage-1 minted junction vertices. A Stage-4
/// relocation arm can converge a chord-crossing vertex onto the SAME geometric
/// junction a Stage-1 mint carries (R0061: the `ell_junction` plane-pair ×
/// cylinder junction IS the minted line×cylinder pierce corner, two exact-intent
/// computations landing ~1e-15 apart), and the mint is unmoved so a moved×moved
/// restriction misses the pair. Eligibility: at least one member `moved` (a
/// minted×minted sub-band pair is a mint-multiplicity contract violation and
/// must stay LOUD). Survivor: the minted vertex ALWAYS — its bits are the
/// shared cross-operand junction identity; the mint never moves (N54).
pub(crate) fn weld_coincident_relocated(
    mesh: &mut Mesh,
    attribution: &mut Vec<Option<TriangleAttribution>>,
    moved: &std::collections::HashSet<u32>,
    minted: &std::collections::HashSet<u32>,
) -> bool {
    let mut welded = false;
    loop {
        // Live moved/minted verts (still referenced by some triangle), ascending.
        let mut live: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
        for tri in &mesh.tris {
            for &v in tri {
                if moved.contains(&v) || minted.contains(&v) {
                    live.insert(v);
                }
            }
        }
        let live: Vec<u32> = live.into_iter().collect();
        let mut pair: Option<(u32, u32)> = None;
        'scan: for (i, &u) in live.iter().enumerate() {
            let pu = mesh.verts[u as usize].as_array();
            for &w in &live[i + 1..] {
                // A vert in BOTH sets counts as minted (identity outranks
                // relocation). Pairs need ≥1 moved member; minted×minted is
                // ineligible (multiplicity stays loud).
                let (u_minted, w_minted) = (minted.contains(&u), minted.contains(&w));
                let (u_moved, w_moved) = (
                    !u_minted && moved.contains(&u),
                    !w_minted && moved.contains(&w),
                );
                if !(u_moved || w_moved) {
                    continue;
                }
                let pw = mesh.verts[w as usize].as_array();
                let d =
                    ((pu[0] - pw[0]).powi(2) + (pu[1] - pw[1]).powi(2) + (pu[2] - pw[2]).powi(2))
                        .sqrt();
                let scale = pu[0]
                    .abs()
                    .max(pu[1].abs())
                    .max(pu[2].abs())
                    .max(pw[0].abs())
                    .max(pw[1].abs())
                    .max(pw[2].abs());
                let band = cad_primitives::TAU_MODEL * (1.0 + scale);
                if d < band {
                    // Survivor: the minted member if any (the mint's bits are
                    // the cross-operand junction identity); else lower index
                    // (matches every other collapse's survivor rule — both are
                    // already exact on their curve, so no exactness ranking is
                    // needed).
                    pair = Some(if u_minted {
                        (w, u) // (victim, survivor = mint)
                    } else if w_minted {
                        (u, w)
                    } else {
                        (w, u) // (victim = higher, survivor = lower)
                    });
                    break 'scan;
                }
            }
        }
        match pair {
            Some((victim, survivor)) => {
                if std::env::var_os("YANG_MOVED_WELD_PROBE").is_some() {
                    eprintln!(
                        "[moved-weld] victim={victim} survivor={survivor} p={:?}",
                        mesh.verts[survivor as usize]
                    );
                }
                collapse_vertex(mesh, attribution, victim, survivor);
                welded = true;
            }
            None => break,
        }
    }
    welded
}

/// P3b inc-4b (spec `yang_169_p3b_curved_partner_pierce.md` §5 inc-4b):
/// beyond-corner conformal TRIM. A Stage-4 relocation can land a
/// section-curve sample OUTSIDE the bounded owner face, past a Stage-1
/// minted corner junction on the same curve (F0082's phantom: the chord-ring
/// crossing vertex relocated to the ellipse's canonical t≈π/2, 1.29e-3
/// beyond the wall the minted corner terminates at). Such a sample has ZERO
/// kept content — the curve stops being an output boundary at the corner —
/// so it is removed TOPOLOGICALLY: edge-collapse phantom→mint (survivor =
/// the mint, `collapse_vertex` watertight-preserving), justified by the
/// out-of-face + beyond-corner predicate, never by distance.
///
/// Predicate, per mesh edge (m, v) with `m` minted and `v` moved (and not
/// itself a mint) — all bands derived, no new tolerance:
/// - beyond-corner: signed distance to an owner plane i with a CONVEX
///   pierce-time verdict (`trim_beyond`) exceeds `TAU_MODEL·(1+scale)`;
/// - on-the-other-plane: |signed distance to plane j| ≤ `TAU_EVAL·(1+scale)`
///   (v is a section-curve sample of partner×plane j ⇒ the segment m→v
///   leaves the bounded face AT the corner);
/// - corridor cap: |v−m| ≤ `tangent_plane_corridor(d_ε, sinθ)`,
///   sinθ = dᵢ(v)/|v−m| — the chord-crossing displacement bound. Beyond it
///   the vert may be LEGITIMATE far-side geometry (the owner plane is
///   infinite; a non-convex face can re-enter its positive half-space away
///   from this corner): NO fire, status quo — the #173/ring gates downstream
///   stay loud. A missed trim is never worse; a false trim would be
///   silent-wrong, so every leg fails closed.
pub(crate) fn trim_beyond_corner_phantoms(
    mesh: &mut Mesh,
    attribution: &mut Vec<Option<TriangleAttribution>>,
    moved: &std::collections::HashSet<u32>,
    minted: &std::collections::BTreeMap<u32, crate::boolean::MintProvenance>,
    d_eps: f64,
) -> bool {
    let probe = std::env::var_os("YANG_P3B_TRIM_PROBE").is_some();
    let mut trimmed = false;
    'fixed_point: loop {
        // Patch-subset guard (the F0082 cap-ring lesson, measured 2026-07-19):
        // collapsing v→m reroutes EVERY patch incident to v onto m, so the
        // zero-content justification must hold for all of them — if v carries
        // a patch m does not touch (F0082: the phantom is also a boundary
        // vertex of B's near-coplanar CAP face, which the mint is 1e-4 off),
        // the collapse would drag that face's ring onto a foreign point
        // (s6-planar-loop-nonplanar, silent-wrong were the band looser).
        // Eligibility therefore requires attributed-patch(v) ⊆
        // attributed-patch(m); unattributed (`None`) intersection-strip
        // triangles are neutral (they belong to the junction itself).
        let mut patches: std::collections::BTreeMap<
            u32,
            std::collections::BTreeSet<(InputId, u32)>,
        > = std::collections::BTreeMap::new();
        for (ti, tri) in mesh.tris.iter().enumerate() {
            if let Some(Some(att)) = attribution.get(ti) {
                for &tv in tri {
                    if moved.contains(&tv) || minted.contains_key(&tv) {
                        patches.entry(tv).or_default().insert((att.input, att.face));
                    }
                }
            }
        }
        let empty: std::collections::BTreeSet<(InputId, u32)> = std::collections::BTreeSet::new();
        let mut seen: std::collections::BTreeSet<(u32, u32)> = std::collections::BTreeSet::new();
        for tri in &mesh.tris {
            for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
                let (u, w) = (tri[i], tri[j]);
                if u == w || !seen.insert((u.min(w), u.max(w))) {
                    continue;
                }
                for (m, v) in [(u, w), (w, u)] {
                    let Some(prov) = minted.get(&m) else {
                        continue;
                    };
                    if minted.contains_key(&v) || !moved.contains(&v) {
                        continue;
                    }
                    let pv_patches = patches.get(&v).unwrap_or(&empty);
                    let pm_patches = patches.get(&m).unwrap_or(&empty);
                    if !pv_patches.is_subset(pm_patches) {
                        if probe {
                            eprintln!(
                                "[p3b-trim] patch-guard NO-FIRE mint v{m} ~ v{v}: \
                                 v patches {pv_patches:?} ⊄ m patches {pm_patches:?}"
                            );
                        }
                        continue;
                    }
                    let pv = mesh.verts[v as usize].as_array();
                    let pm = mesh.verts[m as usize].as_array();
                    let dist = ((pv[0] - pm[0]).powi(2)
                        + (pv[1] - pm[1]).powi(2)
                        + (pv[2] - pm[2]).powi(2))
                    .sqrt();
                    if dist <= 0.0 {
                        continue; // coincidence is the weld's territory
                    }
                    let scale = pv
                        .iter()
                        .chain(pm.iter())
                        .fold(0.0f64, |acc, &c| acc.max(c.abs()));
                    let beyond_band = cad_primitives::TAU_MODEL * (1.0 + scale);
                    let on_band = cad_primitives::TAU_EVAL * (1.0 + scale);
                    for k in 0..2 {
                        let pi = prov.owner_planes[k];
                        let pj = prov.owner_planes[1 - k];
                        if !pi.trim_beyond {
                            continue; // reflex/ambiguous/default — fail closed
                        }
                        let d_i = pi.n[0] * pv[0] + pi.n[1] * pv[1] + pi.n[2] * pv[2] + pi.d;
                        let d_j = pj.n[0] * pv[0] + pj.n[1] * pv[1] + pj.n[2] * pv[2] + pj.d;
                        if d_i <= beyond_band || d_j.abs() > on_band {
                            continue;
                        }
                        let sin_theta = d_i / dist;
                        if dist > tangent_plane_corridor(d_eps, sin_theta) {
                            if probe {
                                eprintln!(
                                    "[p3b-trim] over-corridor NO-FIRE mint v{m} ~ v{v} \
                                     dist={dist:.3e} d_i={d_i:.3e} d_eps={d_eps:.3e}"
                                );
                            }
                            continue;
                        }
                        if probe {
                            eprintln!(
                                "[p3b-trim] TRIM v{v} -> mint v{m} dist={dist:.3e} \
                                 d_i={d_i:.3e} d_j={:.3e} sin={sin_theta:.3}",
                                d_j.abs()
                            );
                        }
                        collapse_vertex(mesh, attribution, v, m);
                        trimmed = true;
                        continue 'fixed_point;
                    }
                }
            }
        }
        break;
    }
    trimmed
}

/// P3b inc-4c (spec `yang_169_p3b_inc4c_fan_retriangulation.md`): the Yang
/// §4.4.1 "update the triangulation accordingly" half of the Stage-4 merge
/// ops. The weld/trim passes above collapse clusters of victims onto Stage-1
/// minted junction vertices; a victim cluster spanning ADJACENT mints maps
/// every pre-mesh edge crossing the victim partition onto the single
/// mint-pair edge, stacking surviving triangles there (R0061 measured:
/// edge (186,211) 1A+1B pre → 4A+2B post; six edges total-use ≠ 2, all
/// mint-anchored). The survivors have DISTINCT near-dup tips, so no
/// exact-duplicate rule (membrane cancellation, i6 wedge dedup) can fire —
/// and no deletion-only rule is correct (dropping a copy leaves its tip
/// unpaired). The repair is connectivity-only local RE-TRIANGULATION of the
/// merged fan regions, per attribution key, keeping every healthy edge
/// verbatim:
///
/// - detect: undirected edges with total incident-triangle count ≠ 2 and ≥1
///   minted endpoint (the mint anchor keeps this away from legitimate
///   mint-free 4-sheet structure, e.g. Steinmetz tangency generators);
/// - cluster defective edges by shared vertices; per cluster, per
///   attribution key: region = that key's triangles incident to a cluster
///   vertex;
/// - per region, classify edges: pinned (use 2) with exactly 1 region tri →
///   BOUNDARY (kept verbatim; the outside/other-side triangle keeps it
///   paired); pinned with 2 region tris → interior (CDT may rewire);
///   defective with all tris inside the cluster's regions → interior
///   (the fold being dissolved); anything else → bail the cluster;
/// - keep-boundary re-CDT in the region's `SurfaceChart` (Plane/Cylinder;
///   cylinder θ re-centred, quarter-turn straddle guard). NO vertex is
///   created, moved, or removed — both operands land on the identical 3D
///   seam polyline by construction (the degenerate-but-sufficient case of
///   the Phase-A two-sided update);
/// - postcondition, all-loud: after splicing the cluster, every edge of the
///   new triangles has total-use exactly 2 and every formerly-defective
///   edge has total-use 0 or 2 — else the cluster's ORIGINAL triangles are
///   restored (a bail may never trade one non-manifold shape for another).
///
/// Bails are per-cluster and leave the mesh untouched for the downstream
/// loud gates (P10: this pass can only convert a loud STOP into a correct
/// result or leave it standing). Probe: `YANG_P3B_FANFIX_PROBE`.
pub(crate) fn retriangulate_collapsed_fan_regions(
    mesh: &mut Mesh,
    attr_vec: &mut Vec<Option<TriangleAttribution>>,
    brep_a: &BRep,
    brep_b: &BRep,
    moved: &std::collections::HashSet<u32>,
    minted: &std::collections::HashSet<u32>,
) -> bool {
    use std::collections::{BTreeMap, BTreeSet};
    let probe = std::env::var_os("YANG_P3B_FANFIX_PROBE").is_some();
    if minted.is_empty() {
        return false;
    }
    let mut changed = false;
    // Clusters already attempted (by sorted vertex set) — successful repairs
    // recompute from the mutated mesh; bailed clusters are not retried.
    let mut attempted: BTreeSet<Vec<u32>> = BTreeSet::new();
    'passes: loop {
        // Undirected edge → incident triangle indices, whole mesh.
        let mut edge_use: BTreeMap<(u32, u32), Vec<usize>> = BTreeMap::new();
        for (ti, tri) in mesh.tris.iter().enumerate() {
            for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
                let (u, w) = (tri[i], tri[j]);
                let e = if u < w { (u, w) } else { (w, u) };
                edge_use.entry(e).or_default().push(ti);
            }
        }
        // Defective edges: total use ≠ 2, ≥1 minted endpoint.
        let defective: Vec<(u32, u32)> = edge_use
            .iter()
            .filter(|(&(u, w), ts)| ts.len() != 2 && (minted.contains(&u) || minted.contains(&w)))
            .map(|(&e, _)| e)
            .collect();
        if defective.is_empty() {
            break;
        }
        // Cluster by shared vertices (deterministic union-find over BTree order).
        let mut root_of: BTreeMap<u32, u32> = BTreeMap::new();
        fn find(m: &mut BTreeMap<u32, u32>, v: u32) -> u32 {
            let mut r = v;
            while m.get(&r).copied().unwrap_or(r) != r {
                r = m[&r];
            }
            let mut c = v;
            while m.get(&c).copied().unwrap_or(c) != r {
                let n = m[&c];
                m.insert(c, r);
                c = n;
            }
            r
        }
        for &(u, w) in &defective {
            let (ru, rw) = (find(&mut root_of, u), find(&mut root_of, w));
            if ru != rw {
                root_of.insert(ru.max(rw), ru.min(rw));
            }
        }
        let mut clusters: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
        for &(u, w) in &defective {
            let r = find(&mut root_of, u);
            clusters.entry(r).or_default().extend([u, w]);
        }
        for cluster in clusters.values() {
            let sig: Vec<u32> = cluster.iter().copied().collect();
            if attempted.contains(&sig) {
                continue;
            }
            attempted.insert(sig.clone());
            match repair_fan_cluster(
                mesh, attr_vec, brep_a, brep_b, cluster, &edge_use, moved, minted, probe,
            ) {
                Some(()) => {
                    if probe {
                        eprintln!("[p3b-fanfix] cluster {sig:?} REPAIRED");
                    }
                    changed = true;
                    continue 'passes; // mesh mutated — recompute maps
                }
                None => {
                    if probe {
                        eprintln!("[p3b-fanfix] cluster {sig:?} bailed (loud gates stand)");
                    }
                }
            }
        }
        break; // all remaining clusters attempted (bailed) — done
    }
    changed
}

/// One cluster's repair attempt for [`retriangulate_collapsed_fan_regions`].
/// `Some(())` = the mesh was mutated and the postcondition verified;
/// `None` = bail, mesh guaranteed untouched.
#[allow(clippy::too_many_lines)]
/// Yang §4.3.4 curve-refinement acceptance test
/// (`refs/text/yang2025_hybrid_boolean.txt:586-592`): for consecutive curve
/// points p, q with an intermediate point m, no further subdivision is
/// needed — i.e. m is REDUNDANT and the chord p→q suffices — iff
///
///   h < d_p·10²,  l < d_p·10³,  α < π/18
///
/// with h = the arc height (distance from m to segment pq), l = the chord
/// length max(|pm|, |mq|), and α = the turning angle between p→m and m→q.
/// The paper pins d_p = 1e-7 (`:744-745`), which is exactly this port's
/// `TAU_MODEL`; scale-relative as everywhere else: d_p = TAU_MODEL·(1+scale).
/// Used by the inc-4c-2 chain decimation: a sample the paper's own
/// refinement loop would never have inserted may be removed (deviation N58,
/// paper-criterion form). The measurements themselves are factored into
/// [`paper_chain_metrics`], shared with the I5-0 seam-density census
/// (spec `yang_441_trim_cdt_construction.md` §4-I5).
pub(crate) fn paper_chain_sample_redundant(a: [f64; 3], m: [f64; 3], b: [f64; 3]) -> bool {
    let mt = paper_chain_metrics(a, m, b);
    if mt.l >= mt.dp * 1e3 {
        return false;
    }
    if mt.h >= mt.dp * 1e2 {
        return false;
    }
    if mt.degenerate {
        return true; // coincident with a neighbour: trivially redundant
    }
    mt.alpha < std::f64::consts::PI / 18.0
}

/// I5-0: the §4.3.4 measurements of one (a, m, b) sample triple — h (arc
/// height of m over chord ab), l = max(|am|, |mb|), α (turning angle a→m→b,
/// 0 when a leg is degenerate), and the triple's own scale-relative
/// d_p = TAU_MODEL·(1+scale). Pure measurement; the acceptance thresholds
/// (h < d_p·10², l < d_p·10³, α < π/18) live in the consumers so the
/// predicate stays byte-identical to its pre-factoring form.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ChainSampleMetrics {
    pub h: f64,
    pub l: f64,
    pub alpha: f64,
    pub degenerate: bool,
    pub dp: f64,
}

/// See [`ChainSampleMetrics`]. Extracted verbatim from
/// [`paper_chain_sample_redundant`]; same float operations on the same
/// inputs, so the predicate's decisions are unchanged.
pub(crate) fn paper_chain_metrics(a: [f64; 3], m: [f64; 3], b: [f64; 3]) -> ChainSampleMetrics {
    let scale = a
        .iter()
        .chain(m.iter())
        .chain(b.iter())
        .fold(0.0f64, |acc, &c| acc.max(c.abs()));
    let dp = cad_primitives::TAU_MODEL * (1.0 + scale);
    let am = [m[0] - a[0], m[1] - a[1], m[2] - a[2]];
    let mb = [b[0] - m[0], b[1] - m[1], b[2] - m[2]];
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let n2 = |v: [f64; 3]| v[0] * v[0] + v[1] * v[1] + v[2] * v[2];
    let dot = |x: [f64; 3], y: [f64; 3]| x[0] * y[0] + x[1] * y[1] + x[2] * y[2];
    // l = max(|pm|, |mq|).
    let (lam, lmb) = (n2(am).sqrt(), n2(mb).sqrt());
    let l = lam.max(lmb);
    // h = distance from m to segment ab.
    let lab2 = n2(ab);
    let h = if lab2 > 0.0 {
        let t = (dot(am, ab) / lab2).clamp(0.0, 1.0);
        n2([am[0] - t * ab[0], am[1] - t * ab[1], am[2] - t * ab[2]]).sqrt()
    } else {
        lam
    };
    // α = turning angle between a→m and m→b.
    let degenerate = lam <= 0.0 || lmb <= 0.0;
    let alpha = if degenerate {
        0.0
    } else {
        let cos_a = (dot(am, mb) / (lam * lmb)).clamp(-1.0, 1.0);
        cos_a.acos()
    };
    ChainSampleMetrics {
        h,
        l,
        alpha,
        degenerate,
        dp,
    }
}

/// inc-4c-2: analytic curve parameter for a seam run between the two faces'
/// surfaces, or `None` for an unsupported pair (the run is then left as-is
/// and the CDT stays the loud verifier).
///
/// * Plane × Plane — the section is a line: t = p · d̂ with d = n̂₁×n̂₂
///   (near-parallel pair → `None`).
/// * Plane × Cylinder — the section is an ellipse (θ injective along it):
///   t = θ re-centred on the run's circular mean; a plane ∥ to the axis cuts
///   generator lines instead, where the axial coordinate orders the run. A
///   quarter-turn straddle after re-centring → `None`.
pub(crate) fn seam_run_params(
    s1: Surface,
    s2: Surface,
    path: &[u32],
    mesh: &Mesh,
) -> Option<Vec<f64>> {
    let dot3 = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    match (s1, s2) {
        (Surface::Plane { normal: n1, .. }, Surface::Plane { normal: n2, .. }) => {
            let (a, b) = (normalize3(n1.as_array()), normalize3(n2.as_array()));
            let d = [
                a[1] * b[2] - a[2] * b[1],
                a[2] * b[0] - a[0] * b[2],
                a[0] * b[1] - a[1] * b[0],
            ];
            let len = dot3(d, d).sqrt();
            if len < 1e-9 {
                return None; // near-parallel planes: no line direction
            }
            let d = [d[0] / len, d[1] / len, d[2] / len];
            Some(
                path.iter()
                    .map(|&v| dot3(mesh.verts[v as usize].as_array(), d))
                    .collect(),
            )
        }
        (
            Surface::Plane { normal, .. },
            Surface::Cylinder {
                axis_point,
                axis_dir,
                ..
            },
        )
        | (
            Surface::Cylinder {
                axis_point,
                axis_dir,
                ..
            },
            Surface::Plane { normal, .. },
        ) => {
            let ax = normalize3(axis_dir.as_array());
            let nn = normalize3(normal.as_array());
            let ap = axis_point.as_array();
            if dot3(ax, nn).abs() < 1e-6 {
                // Generator-line section: order axially.
                return Some(
                    path.iter()
                        .map(|&v| {
                            let p = mesh.verts[v as usize].as_array();
                            dot3([p[0] - ap[0], p[1] - ap[1], p[2] - ap[2]], ax)
                        })
                        .collect(),
                );
            }
            let (e1v, e2v) = ortho_basis(cad_primitives::Vector3::new(ax[0], ax[1], ax[2]));
            let (e1, e2) = (e1v.as_array(), e2v.as_array());
            let thetas: Vec<f64> = path
                .iter()
                .map(|&v| {
                    let p = mesh.verts[v as usize].as_array();
                    let w = [p[0] - ap[0], p[1] - ap[1], p[2] - ap[2]];
                    let z = dot3(w, ax);
                    let r = [w[0] - z * ax[0], w[1] - z * ax[1], w[2] - z * ax[2]];
                    dot3(r, e2).atan2(dot3(r, e1))
                })
                .collect();
            let (mut sx, mut sy) = (0.0f64, 0.0f64);
            for &t in &thetas {
                sx += t.cos();
                sy += t.sin();
            }
            let t0 = sy.atan2(sx);
            let mut out = Vec::with_capacity(thetas.len());
            for &t in &thetas {
                let mut dt = t - t0;
                while dt > std::f64::consts::PI {
                    dt -= 2.0 * std::f64::consts::PI;
                }
                while dt < -std::f64::consts::PI {
                    dt += 2.0 * std::f64::consts::PI;
                }
                if dt.abs() > std::f64::consts::FRAC_PI_2 {
                    return None; // quarter-turn straddle
                }
                out.push(dt);
            }
            Some(out)
        }
        _ => None,
    }
}

/// An unordered attribution-key pair naming one seam chain's two faces.
type SeamKeyPair = ((InputId, u32), (InputId, u32));

#[allow(clippy::too_many_arguments)]
fn repair_fan_cluster(
    mesh: &mut Mesh,
    attr_vec: &mut Vec<Option<TriangleAttribution>>,
    brep_a: &BRep,
    brep_b: &BRep,
    cluster: &std::collections::BTreeSet<u32>,
    edge_use: &std::collections::BTreeMap<(u32, u32), Vec<usize>>,
    moved: &std::collections::HashSet<u32>,
    minted: &std::collections::HashSet<u32>,
    probe: bool,
) -> Option<()> {
    use std::collections::{BTreeMap, BTreeSet, HashMap};
    let bail = |reason: &str| -> Option<()> {
        if probe {
            eprintln!("[p3b-fanfix] bail: {reason}");
        }
        None
    };
    let surf_of = |k: (InputId, u32)| -> Surface {
        let br = if matches!(k.0, InputId::A) {
            brep_a
        } else {
            brep_b
        };
        br.faces()[k.1 as usize].surface
    };

    // Seam-pinned defective edges: a defective edge where some attribution
    // key contributes EXACTLY ONE triangle is a live seam edge — that side is
    // unfolded (a fold contributes >=2 same-side triangles), its single
    // triangle is legitimate kept surface, and the closed output must pair it
    // across the edge from the other side. Such an edge is constrained as a
    // BOUNDARY edge in every region touching it (each side supplies exactly
    // one triangle -> total 2). Balanced fold chords (every key count != 1,
    // e.g. R0061's (186,211) 4A+2B and (193,211) 2A+2B) stay free: the CDTs
    // may dissolve or re-mint them; the postcondition verifies the total.
    let seam_pinned = |e: &(u32, u32)| -> bool {
        let ts = &edge_use[e];
        let mut per_key: BTreeMap<(InputId, u32), usize> = BTreeMap::new();
        for &ti in ts {
            if let Some(at) = attr_vec.get(ti).copied().flatten() {
                *per_key.entry((at.input, at.face)).or_default() += 1;
            }
        }
        per_key.values().any(|&n| n == 1)
    };
    // Regions: attribution key → triangles incident to an ANCHOR vertex
    // (initially the cluster; inc-4c-2 grows the anchor set when a seam
    // disorder reaches the region rim). An UNATTRIBUTED triangle touching
    // the anchors leaves the repair without a surface to re-CDT in — bail.
    //
    // inc-4c-2 seam-run canonicalization (spec §5): Stage-4 relocation can
    // land near-dup seam samples OUT OF ORDER along their analytic section
    // curve (the chain reflects stale chordal positions), so the region
    // boundaries self-cross in-chart and no keep-boundary CDT can run. The
    // chain is connectivity: for every seam run (path of pinned/seam-pinned
    // edges between exactly two attribution keys, fully inside the cluster
    // regions — both sides re-CDT, so the chain is rewireable), sort the
    // run's vertices by the pair's analytic curve parameter and constrain
    // BOTH sides to the sorted chain. No vertex moves; ties bail; a
    // disorder whose parameter extremes are not the run's path ends reaches
    // beyond the current regions → grow the anchors by the run ends and
    // rebuild (bounded).
    struct RegionPlan {
        key: (InputId, u32),
        boundary: std::collections::BTreeSet<(u32, u32)>,
        new_tris: Vec<[u32; 3]>,
    }
    let mut anchors: BTreeSet<u32> = cluster.clone();
    let mut grow_rounds = 0usize;
    let (plans, in_regions, removed_chain, added_chain) = 'grow: loop {
        let mut regions: BTreeMap<(InputId, u32), BTreeSet<usize>> = BTreeMap::new();
        for (ti, tri) in mesh.tris.iter().enumerate() {
            if !tri.iter().any(|v| anchors.contains(v)) {
                continue;
            }
            match attr_vec.get(ti).copied().flatten() {
                Some(at) => {
                    regions.entry((at.input, at.face)).or_default().insert(ti);
                }
                None => return bail("unattributed triangle in cluster"),
            }
        }
        if regions.is_empty() {
            return bail("empty cluster");
        }
        let in_regions: BTreeSet<usize> = regions.values().flatten().copied().collect();
        // Every defective cluster edge must be fully inside the cluster's regions.
        for (&(u, w), ts) in edge_use {
            if ts.len() != 2
                && (cluster.contains(&u) || cluster.contains(&w))
                && !ts.iter().all(|t| in_regions.contains(t))
            {
                return bail("defective edge reaches outside the cluster regions");
            }
        }
        // Rewireable seam edges by key pair.
        let mut by_pair: BTreeMap<SeamKeyPair, BTreeSet<(u32, u32)>> = BTreeMap::new();
        // ALL same-pair seam edges mesh-wide (anchor detection: a component
        // vertex touching a same-pair seam edge OUTSIDE the rewireable set
        // continues the chain beyond the regions).
        let mut full_pair: BTreeMap<SeamKeyPair, BTreeSet<(u32, u32)>> = BTreeMap::new();
        for (&e, ts) in edge_use {
            if ts.is_empty() {
                continue;
            }
            let mut keys: BTreeSet<(InputId, u32)> = BTreeSet::new();
            let mut attributed = true;
            for &ti in ts {
                match attr_vec.get(ti).copied().flatten() {
                    Some(at) => {
                        keys.insert((at.input, at.face));
                    }
                    None => attributed = false,
                }
            }
            if !attributed || keys.len() != 2 {
                continue;
            }
            if !(ts.len() == 2 || seam_pinned(&e)) {
                continue; // balanced fold chords are not chain members
            }
            let mut it = keys.into_iter();
            let pair = (it.next().expect("2 keys"), it.next().expect("2 keys"));
            full_pair.entry(pair).or_default().insert(e);
            if ts.iter().all(|t| in_regions.contains(t)) {
                by_pair.entry(pair).or_default().insert(e);
            }
        }
        let mut removed_chain: BTreeSet<(u32, u32)> = BTreeSet::new();
        let mut added_chain: Vec<(SeamKeyPair, (u32, u32))> = Vec::new();
        // The FULL rewritten chains (before the unchanged-segment cancel) —
        // used for component merging: an unchanged chain segment still ties
        // the components it touches together.
        let mut chain_merge_edges: Vec<(SeamKeyPair, (u32, u32))> = Vec::new();
        let mut dropped_verts: BTreeSet<u32> = BTreeSet::new();
        for (pair, edges) in &by_pair {
            let mut adj: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
            for &(u, w) in edges {
                adj.entry(u).or_default().push(w);
                adj.entry(w).or_default().push(u);
            }
            let mut seen: BTreeSet<u32> = BTreeSet::new();
            for &start in adj.keys() {
                if seen.contains(&start) {
                    continue;
                }
                let mut stack = vec![start];
                let mut comp: BTreeSet<u32> = BTreeSet::new();
                while let Some(v) = stack.pop() {
                    if !comp.insert(v) {
                        continue;
                    }
                    for &n in &adj[&v] {
                        if !comp.contains(&n) {
                            stack.push(n);
                        }
                    }
                }
                seen.extend(comp.iter().copied());
                if comp.len() < 3 {
                    continue; // a 2-vert run cannot be disordered
                }
                let verts: Vec<u32> = comp.iter().copied().collect();
                let Some(params) = seam_run_params(surf_of(pair.0), surf_of(pair.1), &verts, mesh)
                else {
                    if probe {
                        eprintln!("[p3b-fanfix] seam run {pair:?} {verts:?}: no parameter");
                    }
                    continue; // unsupported pair: left as-is (the CDT verifies)
                };
                let mut order: Vec<usize> = (0..verts.len()).collect();
                order.sort_by(|&a, &b| {
                    params[a]
                        .partial_cmp(&params[b])
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                if order.windows(2).any(|w| params[w[1]] == params[w[0]]) {
                    return bail("seam run parameter tie");
                }
                // The sorted chain's edge set; if it matches the existing run
                // (already a parameter-ordered path), nothing to do.
                let sorted_edges_raw: BTreeSet<(u32, u32)> = order
                    .windows(2)
                    .map(|w| {
                        let (a, b) = (verts[w[0]], verts[w[1]]);
                        if a < b {
                            (a, b)
                        } else {
                            (b, a)
                        }
                    })
                    .collect();
                if &sorted_edges_raw == edges {
                    continue; // already in curve order
                }
                // External connections (same-pair seam edges leaving the
                // rewireable set) may only attach at the parameter extremes;
                // otherwise the disorder reaches past the regions — grow.
                let outside_pair: BTreeSet<(u32, u32)> =
                    full_pair[pair].difference(edges).copied().collect();
                let (lo, hi) = (verts[order[0]], verts[*order.last().expect("nonempty")]);
                let mut grow_verts: BTreeSet<u32> = BTreeSet::new();
                for &(u, w) in &outside_pair {
                    for (a, b) in [(u, w), (w, u)] {
                        if comp.contains(&a) && a != lo && a != hi {
                            grow_verts.insert(b); // pull the chain outward
                        }
                    }
                }
                if !grow_verts.is_empty() {
                    grow_rounds += 1;
                    if grow_rounds > 16 {
                        return bail("seam disorder growth bound exceeded");
                    }
                    if probe {
                        eprintln!(
                            "[p3b-fanfix] seam run {pair:?} disorder reaches its rim — \
                             growing anchors by {grow_verts:?} (round {grow_rounds})"
                        );
                    }
                    anchors.extend(grow_verts);
                    continue 'grow;
                }
                if probe {
                    eprintln!(
                        "[p3b-fanfix] seam run reorder {pair:?}: {verts:?} -> {:?}",
                        order.iter().map(|&i| verts[i]).collect::<Vec<_>>()
                    );
                }
                // §4.3/§4.3.4 loop cleanup on the sorted chain: a RELOCATED
                // sample (moved, non-mint, every triangle inside the regions,
                // on no other seam pair) is dropped iff the PAPER's own
                // curve-refinement acceptance test says the chord between its
                // kept neighbours suffices without it (h/l/α against
                // d_p = TAU_MODEL·(1+scale), `paper_chain_sample_redundant`) —
                // the resulting polyline is one the paper's refinement loop
                // would itself terminate at. Without this cleanup the output
                // ring carries needle samples and the render tessellation
                // mints a degenerate sliver downstream (measured: R0061
                // SUPPORTED_WRONG). Deviation N58, paper-criterion form.
                let on_other_pair = |v: u32| {
                    full_pair
                        .iter()
                        .any(|(op, oes)| op != pair && oes.iter().any(|&(a, b)| a == v || b == v))
                };
                let tris_all_in = |v: u32| {
                    edge_use
                        .iter()
                        .filter(|(&(a, b), _)| a == v || b == v)
                        .all(|(_, ts)| ts.iter().all(|t| in_regions.contains(t)))
                };
                let sorted_verts: Vec<u32> = order.iter().map(|&i| verts[i]).collect();
                let mut kept: Vec<u32> = vec![sorted_verts[0]];
                for i in 1..sorted_verts.len() - 1 {
                    let v = sorted_verts[i];
                    let droppable = moved.contains(&v)
                        && !minted.contains(&v)
                        && !on_other_pair(v)
                        && tris_all_in(v);
                    let drop = droppable
                        && paper_chain_sample_redundant(
                            mesh.verts[*kept.last().expect("nonempty") as usize].as_array(),
                            mesh.verts[v as usize].as_array(),
                            mesh.verts[sorted_verts[i + 1] as usize].as_array(),
                        );
                    if drop {
                        if probe {
                            eprintln!(
                                "[p3b-fanfix] chain sample v{v} dropped (Yang §4.3.4 \
                                 h/l/α redundant, pair {pair:?})"
                            );
                        }
                        dropped_verts.insert(v);
                    } else {
                        kept.push(v);
                    }
                }
                kept.push(*sorted_verts.last().expect("nonempty"));
                let sorted_edges: BTreeSet<(u32, u32)> = kept
                    .windows(2)
                    .map(|w| {
                        if w[0] < w[1] {
                            (w[0], w[1])
                        } else {
                            (w[1], w[0])
                        }
                    })
                    .collect();
                removed_chain.extend(edges.iter().copied());
                for e in &sorted_edges {
                    added_chain.push((*pair, *e));
                    chain_merge_edges.push((*pair, *e));
                }
            }
        }
        // Unchanged segments (present in both sets) stay classified normally.
        let common: Vec<(u32, u32)> = added_chain
            .iter()
            .map(|&(_, e)| e)
            .filter(|e| removed_chain.contains(e))
            .collect();
        for e in &common {
            removed_chain.remove(e);
        }
        added_chain.retain(|(_, e)| !common.contains(e));
        let mut plans: Vec<RegionPlan> = Vec::new();
        // Regions split into edge-connected COMPONENTS: at a 4-strand crossing
        // mint the kept surface of one face meets the cluster in two sectors
        // pinched at the mint; each sector re-triangulates as its own disc.
        let mut components: Vec<((InputId, u32), BTreeSet<usize>)> = Vec::new();
        for (&key, rtris) in &regions {
            let tlist: Vec<usize> = rtris.iter().copied().collect();
            let mut parent: Vec<usize> = (0..tlist.len()).collect();
            fn cfind(p: &mut [usize], x: usize) -> usize {
                let mut r = x;
                while p[r] != r {
                    r = p[r];
                }
                let mut c = x;
                while p[c] != r {
                    let n = p[c];
                    p[c] = r;
                    c = n;
                }
                r
            }
            let mut edge_first: BTreeMap<(u32, u32), usize> = BTreeMap::new();
            for (li, &ti) in tlist.iter().enumerate() {
                let tri = mesh.tris[ti];
                for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
                    let (u, w) = (tri[i], tri[j]);
                    let e = if u < w { (u, w) } else { (w, u) };
                    if let Some(&lj) = edge_first.get(&e) {
                        let (ra, rb) = (cfind(&mut parent, li), cfind(&mut parent, lj));
                        if ra != rb {
                            parent[ra] = rb;
                        }
                    } else {
                        edge_first.insert(e, li);
                    }
                }
            }
            let mut by_root: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
            for (li, &ti) in tlist.iter().enumerate() {
                by_root
                    .entry(cfind(&mut parent, li))
                    .or_default()
                    .insert(ti);
            }
            for comp in by_root.into_values() {
                components.push((key, comp));
            }
        }
        // inc-4c-2: a rewired chain can connect two components of the same key
        // (the old chords defined the old sector split; the sorted chain defines
        // the new one). Merge components joined by an added chain edge so the
        // edge lands inside one region's vertex pool; the pinch fan-chain
        // pairing still resolves the (possibly pinched) merged region.
        if !chain_merge_edges.is_empty() {
            let mut cparent: Vec<usize> = (0..components.len()).collect();
            fn cfind2(p: &mut [usize], x: usize) -> usize {
                let mut r = x;
                while p[r] != r {
                    r = p[r];
                }
                let mut c = x;
                while p[c] != r {
                    let n = p[c];
                    p[c] = r;
                    c = n;
                }
                r
            }
            let comp_verts: Vec<BTreeSet<u32>> = components
                .iter()
                .map(|(_, tris)| {
                    let mut vs = BTreeSet::new();
                    for &ti in tris {
                        vs.extend(mesh.tris[ti]);
                    }
                    vs
                })
                .collect();
            for &(pair, e) in &chain_merge_edges {
                for want_key in [pair.0, pair.1] {
                    let holders: Vec<usize> = (0..components.len())
                        .filter(|&ci| {
                            components[ci].0 == want_key
                                && (comp_verts[ci].contains(&e.0) || comp_verts[ci].contains(&e.1))
                        })
                        .collect();
                    for w in holders.windows(2) {
                        let (ra, rb) = (cfind2(&mut cparent, w[0]), cfind2(&mut cparent, w[1]));
                        if ra != rb {
                            cparent[ra] = rb;
                        }
                    }
                }
            }
            let mut merged: BTreeMap<usize, ((InputId, u32), BTreeSet<usize>)> = BTreeMap::new();
            for (ci, (ckey, ctris)) in components.iter().enumerate() {
                let root = cfind2(&mut cparent, ci);
                let entry = merged
                    .entry(root)
                    .or_insert_with(|| (*ckey, BTreeSet::new()));
                entry.1.extend(ctris.iter().copied());
            }
            components = merged.into_values().collect();
            if probe {
                for (k, tris) in &components {
                    let mut vs: BTreeSet<u32> = BTreeSet::new();
                    for &ti in tris {
                        vs.extend(mesh.tris[ti]);
                    }
                    eprintln!(
                        "[p3b-fanfix] post-merge comp {k:?}: {} tris, verts {vs:?}",
                        tris.len()
                    );
                }
            }
        }
        for (key, rtris) in &components {
            let key = *key;
            // Classify this component's edges.
            let mut boundary: BTreeSet<(u32, u32)> = BTreeSet::new();
            for &ti in rtris {
                let tri = mesh.tris[ti];
                for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
                    let (u, w) = (tri[i], tri[j]);
                    let e = if u < w { (u, w) } else { (w, u) };
                    let ts = &edge_use[&e];
                    let n_in = ts.iter().filter(|t| rtris.contains(t)).count();
                    match (ts.len(), n_in) {
                        (2, 1) => {
                            if !removed_chain.contains(&e) {
                                boundary.insert(e);
                            }
                        }
                        (2, 2) => {} // healthy interior — CDT may rewire
                        (n, _) if n != 2 && ts.iter().all(|t| in_regions.contains(t)) => {
                            if seam_pinned(&e) && !removed_chain.contains(&e) {
                                boundary.insert(e);
                            }
                        }
                        _ => return bail("unclassifiable region edge"),
                    }
                }
            }
            // Region vertex pool (minus §4.3-dropped chain samples — they are
            // sub-render-redundant and must not re-enter via the CDT).
            let mut vset: BTreeSet<u32> = BTreeSet::new();
            for &ti in rtris {
                vset.extend(mesh.tris[ti]);
            }
            for v in &dropped_verts {
                vset.remove(v);
            }
            // inc-4c-2: the rewritten (parameter-sorted) chain edges of any run
            // touching this region's key are constrained boundary edges here.
            for &(pair, e) in &added_chain {
                if pair.0 != key && pair.1 != key {
                    continue;
                }
                if vset.contains(&e.0) && vset.contains(&e.1) {
                    boundary.insert(e);
                } else if probe {
                    eprintln!(
                        "[p3b-fanfix] region {key:?} added edge {e:?} NOT in vset \
                     ({} {})",
                        vset.contains(&e.0),
                        vset.contains(&e.1)
                    );
                }
            }
            if boundary.is_empty() {
                return bail("region has no boundary");
            }
            // Chart (Plane / Cylinder only).
            let surf = surf_of(key);
            let Some(chart) = crate::stage4_project::SurfaceChart::new(surf) else {
                return bail("chartless surface");
            };
            // Project. Cylinder: re-centre θ on the region's circular mean and use
            // the isometric u = r·Δθ; bail if the region straddles a quarter turn
            // (the inc-2 branch-cut guard shape).
            let is_cyl = matches!(surf, Surface::Cylinder { .. });
            let radius = match surf {
                Surface::Cylinder { radius, .. } => radius,
                _ => 1.0,
            };
            let raw: Vec<(u32, cad_primitives::Point2)> = vset
                .iter()
                .map(|&v| (v, chart.project(mesh.verts[v as usize])))
                .collect();
            let theta0 = if is_cyl {
                let (mut sx, mut sy) = (0.0f64, 0.0f64);
                for (_, p) in &raw {
                    sx += p.x().cos();
                    sy += p.x().sin();
                }
                sy.atan2(sx)
            } else {
                0.0
            };
            let mut verts2d: Vec<cad_primitives::Point2> = Vec::with_capacity(raw.len());
            let mut global_of_local: Vec<u32> = Vec::with_capacity(raw.len());
            let mut local_of_global: HashMap<u32, u32> = HashMap::new();
            for (v, p) in &raw {
                let uv = if is_cyl {
                    let mut dt = p.x() - theta0;
                    while dt > std::f64::consts::PI {
                        dt -= 2.0 * std::f64::consts::PI;
                    }
                    while dt < -std::f64::consts::PI {
                        dt += 2.0 * std::f64::consts::PI;
                    }
                    if dt.abs() > std::f64::consts::FRAC_PI_2 {
                        return bail("cylinder region straddles a quarter turn");
                    }
                    cad_primitives::Point2::new(radius * dt, p.y())
                } else {
                    *p
                };
                local_of_global.insert(*v, verts2d.len() as u32);
                global_of_local.push(*v);
                verts2d.push(uv);
            }
            // Boundary loops. Every boundary vertex needs EVEN degree; a
            // degree-2 vertex continues to its other neighbour; a higher even
            // degree is a PINCH (a 4-strand crossing mint: two kept sectors of
            // one face meeting at the vertex). At a pinch the walk pairs each
            // incoming boundary edge with the other bounding edge of the
            // INSIDE angular sector between them — the sector holding region
            // triangles in the chart (folds only ever double-cover inside
            // sectors, so the containment test is fold-tolerant).
            let mut bnd_adj: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
            for &(u, w) in &boundary {
                let (lu, lw) = (local_of_global[&u], local_of_global[&w]);
                bnd_adj.entry(lu).or_default().push(lw);
                bnd_adj.entry(lw).or_default().push(lu);
            }
            // inc-4c-2 guard: a rewritten chain edge is synthetic (it has no mesh
            // triangles yet), so the pinch fan-chain pairing cannot rotate
            // through it — every vertex of an added edge must sit on a plain
            // degree-2 boundary. Extra edges at such a vertex belong to OTHER
            // chains that still carry stale chords through it (measured: R0061's
            // arc chain detouring through the wall-chain vert v195) — grow the
            // anchors by those edges' far endpoints so the offending chains
            // become rewireable next round; bail only when growth is exhausted.
            {
                let mut grow_verts: BTreeSet<u32> = BTreeSet::new();
                for &(pair, e) in &added_chain {
                    if pair.0 != key && pair.1 != key {
                        continue;
                    }
                    for v in [e.0, e.1] {
                        if let Some(&lv) = local_of_global.get(&v) {
                            let nbrs: Vec<u32> = bnd_adj
                                .get(&lv)
                                .map(|n| n.iter().map(|&l| global_of_local[l as usize]).collect())
                                .unwrap_or_default();
                            if nbrs.len() != 2 {
                                if probe {
                                    eprintln!(
                                        "[p3b-fanfix] rewritten seam vert v{v} degree {} \
                                     nbrs {nbrs:?}",
                                        nbrs.len()
                                    );
                                }
                                grow_verts.insert(v);
                                grow_verts.extend(nbrs);
                            }
                        }
                    }
                }
                if !grow_verts.is_empty() {
                    grow_rounds += 1;
                    if grow_rounds > 16 {
                        return bail("seam disorder growth bound exceeded");
                    }
                    if probe {
                        eprintln!(
                            "[p3b-fanfix] chain-junction disorder — growing anchors by \
                         {grow_verts:?} (round {grow_rounds})"
                        );
                    }
                    anchors.extend(grow_verts);
                    continue 'grow;
                }
            }
            if bnd_adj.values().any(|n| n.len() % 2 != 0) {
                // An odd-degree boundary vertex is a chain truncated by the
                // region rim (the disorder continues into un-rewired seam
                // segments) — grow the anchors by the odd vertices and their
                // boundary neighbours; bail only when growth is exhausted.
                let mut grow_verts: BTreeSet<u32> = BTreeSet::new();
                for (&lv, nbrs) in &bnd_adj {
                    if nbrs.len() % 2 != 0 {
                        grow_verts.insert(global_of_local[lv as usize]);
                        for &ln in nbrs {
                            grow_verts.insert(global_of_local[ln as usize]);
                        }
                    }
                }
                grow_rounds += 1;
                let stagnant = grow_verts.iter().all(|v| anchors.contains(v));
                if grow_rounds > 16 || stagnant {
                    if probe {
                        eprintln!(
                            "[p3b-fanfix] region {key:?} odd boundary degrees persist \
                         ({}) at {grow_verts:?}",
                            if stagnant { "stagnant" } else { "growth bound" }
                        );
                        for (&lv, nbrs) in &bnd_adj {
                            if nbrs.len() % 2 != 0 {
                                let gv = global_of_local[lv as usize];
                                let es: Vec<u32> =
                                    nbrs.iter().map(|&l| global_of_local[l as usize]).collect();
                                eprintln!("[p3b-fanfix]   odd v{gv} bnd nbrs {es:?}");
                            }
                        }
                    }
                    return bail("region boundary has an odd-degree vertex");
                }
                if probe {
                    eprintln!(
                        "[p3b-fanfix] region {key:?} odd boundary degree — growing anchors \
                     by {grow_verts:?} (round {grow_rounds})"
                    );
                }
                anchors.extend(grow_verts);
                continue 'grow;
            }
            // Pinch pairing: local vert -> (incoming nbr -> outgoing nbr), by
            // COMBINATORIAL fan chains — at the pinch vertex, rotate through the
            // component's triangles via shared at-vertex interior edges; each
            // chain runs boundary-edge -> ... -> boundary-edge and pairs its two
            // ends. Pure connectivity (fold geometry cannot confuse it); an
            // at-vertex edge whose comp-triangle count is not 1 (boundary) or 2
            // (interior) makes rotation ill-defined -> bail.
            let mut pinch_pair: HashMap<(u32, u32), u32> = HashMap::new();
            for (&v, nbrs) in &bnd_adj {
                if nbrs.len() == 2 {
                    continue;
                }
                let gv = global_of_local[v as usize];
                // Edges at v (by OTHER endpoint, global) -> comp tris incident.
                let mut at_v: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
                for &ti in rtris.iter() {
                    let tri = mesh.tris[ti];
                    let Some(pos) = tri.iter().position(|&g| g == gv) else {
                        continue;
                    };
                    for other in [tri[(pos + 1) % 3], tri[(pos + 2) % 3]] {
                        at_v.entry(other).or_default().push(ti);
                    }
                }
                let is_bnd = |other: u32| -> bool {
                    let e = if gv < other { (gv, other) } else { (other, gv) };
                    boundary.contains(&e)
                };
                for (&other, ts) in &at_v {
                    let want = if is_bnd(other) { 1 } else { 2 };
                    if ts.len() != want {
                        return bail("pinch fan rotation ill-defined");
                    }
                }
                let bnd_others: Vec<u32> = at_v.keys().copied().filter(|&o| is_bnd(o)).collect();
                let mut paired: BTreeSet<u32> = BTreeSet::new();
                for &start_o in &bnd_others {
                    if paired.contains(&start_o) {
                        continue;
                    }
                    // Rotate: current (edge-other, tri) -> tri's third at-v edge.
                    let mut cur_o = start_o;
                    let mut cur_t = at_v[&start_o][0];
                    let mut hops = 0usize;
                    let end_o = loop {
                        hops += 1;
                        if hops > 2 * at_v.len() + 4 {
                            return bail("pinch fan chain does not terminate");
                        }
                        let tri = mesh.tris[cur_t];
                        let pos = tri.iter().position(|&g| g == gv).expect("has v");
                        let (o1, o2) = (tri[(pos + 1) % 3], tri[(pos + 2) % 3]);
                        let next_o = if o1 == cur_o { o2 } else { o1 };
                        if is_bnd(next_o) {
                            break next_o;
                        }
                        let ts = &at_v[&next_o];
                        let next_t = if ts[0] == cur_t { ts[1] } else { ts[0] };
                        cur_o = next_o;
                        cur_t = next_t;
                    };
                    if end_o == start_o || paired.contains(&end_o) {
                        return bail("pinch fan chain closed on itself");
                    }
                    paired.insert(start_o);
                    paired.insert(end_o);
                    let (ls, le) = (local_of_global[&start_o], local_of_global[&end_o]);
                    pinch_pair.insert((v, ls), le);
                    pinch_pair.insert((v, le), ls);
                }
                if paired.len() != bnd_others.len() {
                    return bail("pinch pairing incomplete");
                }
            }
            let mut loops_local: Vec<Vec<u32>> = Vec::new();
            let mut used: BTreeSet<(u32, u32)> = BTreeSet::new(); // undirected, (min,max)
            let bnd_local: Vec<(u32, u32)> = boundary
                .iter()
                .map(|&(u, w)| (local_of_global[&u], local_of_global[&w]))
                .collect();
            let continuation = |prev: u32, cur: u32| -> Option<u32> {
                let nb = &bnd_adj[&cur];
                if nb.len() == 2 {
                    Some(if nb[0] == prev { nb[1] } else { nb[0] })
                } else {
                    pinch_pair.get(&(cur, prev)).copied()
                }
            };
            for &(su, sw) in &bnd_local {
                if used.contains(&(su.min(sw), su.max(sw))) {
                    continue;
                }
                let mut lp = vec![su];
                let (mut prev, mut cur) = (su, sw);
                used.insert((su.min(sw), su.max(sw)));
                let mut steps = 0usize;
                loop {
                    steps += 1;
                    if steps > 2 * bnd_local.len() + 4 {
                        return bail("boundary walk does not close");
                    }
                    let Some(next) = continuation(prev, cur) else {
                        return bail("pinch pairing missing");
                    };
                    if (cur, next) == (su, sw) {
                        break; // closed: back at the starting directed edge
                    }
                    lp.push(cur);
                    if !used.insert((cur.min(next), cur.max(next))) {
                        return bail("boundary walk re-traverses an edge");
                    }
                    prev = cur;
                    cur = next;
                }
                loops_local.push(lp);
            }
            // inc-4c-2: 2D self-crossing scan over this component's boundary
            // loops. A crossing means a seam chain bounding the region is
            // disordered but not (yet) rewireable — typically its other side's
            // face has no triangles in the regions. Grow the anchors by the
            // crossing vertices so that face joins the regions next round and
            // the chain canonicalization can reorder it.
            {
                let mut loop_edges: Vec<(u32, u32)> = Vec::new();
                for lp in &loops_local {
                    let m = lp.len();
                    for i in 0..m {
                        loop_edges.push((lp[i], lp[(i + 1) % m]));
                    }
                }
                let mut grow_verts: BTreeSet<u32> = BTreeSet::new();
                for i in 0..loop_edges.len() {
                    for j in (i + 1)..loop_edges.len() {
                        let (a, b) = loop_edges[i];
                        let (c, d) = loop_edges[j];
                        if a == c || a == d || b == c || b == d {
                            continue;
                        }
                        let (pa, pb, pc, pd) = (
                            verts2d[a as usize],
                            verts2d[b as usize],
                            verts2d[c as usize],
                            verts2d[d as usize],
                        );
                        let cr = |o: cad_primitives::Point2,
                                  p: cad_primitives::Point2,
                                  q: cad_primitives::Point2| {
                            (p.x() - o.x()) * (q.y() - o.y()) - (p.y() - o.y()) * (q.x() - o.x())
                        };
                        let (d1, d2) = (cr(pc, pd, pa), cr(pc, pd, pb));
                        let (d3, d4) = (cr(pa, pb, pc), cr(pa, pb, pd));
                        if (d1 > 0.0) != (d2 > 0.0) && (d3 > 0.0) != (d4 > 0.0) {
                            for &l in &[a, b, c, d] {
                                grow_verts.insert(global_of_local[l as usize]);
                            }
                        }
                    }
                }
                if !grow_verts.is_empty() {
                    grow_rounds += 1;
                    if grow_rounds > 16 {
                        return bail("seam disorder growth bound exceeded");
                    }
                    if probe {
                        eprintln!(
                            "[p3b-fanfix] region {key:?} boundary self-crosses — growing \
                         anchors by {grow_verts:?} (round {grow_rounds})"
                        );
                    }
                    anchors.extend(grow_verts);
                    continue 'grow;
                }
            }
            let signed_area = |lp: &[u32]| -> f64 {
                let mut a2 = 0.0;
                for i in 0..lp.len() {
                    let p = verts2d[lp[i] as usize];
                    let q = verts2d[lp[(i + 1) % lp.len()] as usize];
                    a2 += p.x() * q.y() - q.x() * p.y();
                }
                a2 * 0.5
            };
            let outer_i = (0..loops_local.len()).max_by(|&x, &y| {
                signed_area(&loops_local[x])
                    .abs()
                    .partial_cmp(&signed_area(&loops_local[y]).abs())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })?;
            let outer = loops_local[outer_i].clone();
            let holes: Vec<Vec<u32>> = loops_local
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != outer_i)
                .map(|(_, l)| l.clone())
                .collect();
            let on_loop: BTreeSet<u32> = loops_local.iter().flatten().copied().collect();
            let interior: Vec<u32> = (0..verts2d.len() as u32)
                .filter(|l| !on_loop.contains(l))
                .collect();
            let tris_local = match cherchi_rs::cdt_polygon_with_holes_keep_interior(
                &verts2d, &outer, &holes, &interior,
            ) {
                Ok(t) => t,
                Err(e) => {
                    if probe {
                        eprintln!(
                        "[p3b-fanfix] region {key:?} CDT error {e:?}: outer {} holes {} interior {} \
                         loop_verts {:?}",
                        outer.len(),
                        holes.len(),
                        interior.len(),
                        loops_local
                            .iter()
                            .map(|lp| lp.iter().map(|&l| global_of_local[l as usize]).collect::<Vec<_>>())
                            .collect::<Vec<_>>()
                    );
                        for lp in &loops_local {
                            for &l in lp {
                                let p = verts2d[l as usize];
                                eprintln!(
                                    "[p3b-fanfix]   v{} 2d=({:.9},{:.9})",
                                    global_of_local[l as usize],
                                    p.x(),
                                    p.y()
                                );
                            }
                        }
                    }
                    return bail("keep-interior CDT failed");
                }
            };
            // Winding: align to the region's pre-repair net orientation.
            let dir_at = |g: &[u32; 3]| -> [f64; 3] {
                match surf {
                    Surface::Plane { normal, .. } => normal.as_array(),
                    Surface::Cylinder {
                        axis_point,
                        axis_dir,
                        ..
                    } => {
                        let ap = axis_point.as_array();
                        let ax = normalize3(axis_dir.as_array());
                        let c = [
                            (mesh.verts[g[0] as usize].x()
                                + mesh.verts[g[1] as usize].x()
                                + mesh.verts[g[2] as usize].x())
                                / 3.0
                                - ap[0],
                            (mesh.verts[g[0] as usize].y()
                                + mesh.verts[g[1] as usize].y()
                                + mesh.verts[g[2] as usize].y())
                                / 3.0
                                - ap[1],
                            (mesh.verts[g[0] as usize].z()
                                + mesh.verts[g[1] as usize].z()
                                + mesh.verts[g[2] as usize].z())
                                / 3.0
                                - ap[2],
                        ];
                        let z = c[0] * ax[0] + c[1] * ax[1] + c[2] * ax[2];
                        [c[0] - z * ax[0], c[1] - z * ax[1], c[2] - z * ax[2]]
                    }
                    _ => [0.0, 0.0, 0.0],
                }
            };
            let mut ref_sign = 0.0f64;
            for &ti in rtris {
                let g = mesh.tris[ti];
                let av = tri_area_vector(
                    mesh.verts[g[0] as usize].as_array(),
                    mesh.verts[g[1] as usize].as_array(),
                    mesh.verts[g[2] as usize].as_array(),
                );
                let d = dir_at(&g);
                ref_sign += av[0] * d[0] + av[1] * d[1] + av[2] * d[2];
            }
            let ref_sign = if ref_sign >= 0.0 { 1.0 } else { -1.0 };
            let mut new_tris: Vec<[u32; 3]> = Vec::with_capacity(tris_local.len());
            for tl in &tris_local {
                let mut g = [
                    global_of_local[tl[0] as usize],
                    global_of_local[tl[1] as usize],
                    global_of_local[tl[2] as usize],
                ];
                let av = tri_area_vector(
                    mesh.verts[g[0] as usize].as_array(),
                    mesh.verts[g[1] as usize].as_array(),
                    mesh.verts[g[2] as usize].as_array(),
                );
                let d = dir_at(&g);
                if (av[0] * d[0] + av[1] * d[1] + av[2] * d[2]) * ref_sign < 0.0 {
                    g.swap(1, 2);
                }
                new_tris.push(g);
            }
            plans.push(RegionPlan {
                key,
                boundary,
                new_tris,
            });
        }

        break (plans, in_regions, removed_chain, added_chain);
    };
    // Splice all regions at once; the postcondition below runs on the
    // CANDIDATE triangle list before any mutation, so a violation simply
    // bails with the mesh untouched.
    let mut next_tris: Vec<[u32; 3]> = Vec::with_capacity(mesh.tris.len());
    let mut next_attr: Vec<Option<TriangleAttribution>> = Vec::with_capacity(attr_vec.len());
    for ti in 0..mesh.tris.len() {
        if !in_regions.contains(&ti) {
            next_tris.push(mesh.tris[ti]);
            next_attr.push(attr_vec.get(ti).copied().flatten());
        }
    }
    let mut new_edge_set: BTreeSet<(u32, u32)> = BTreeSet::new();
    for plan in &plans {
        let at = TriangleAttribution {
            input: plan.key.0,
            face: plan.key.1,
        };
        for &g in &plan.new_tris {
            for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
                let (u, w) = (g[i], g[j]);
                new_edge_set.insert(if u < w { (u, w) } else { (w, u) });
            }
            next_tris.push(g);
            next_attr.push(Some(at));
        }
    }
    // Postcondition. Expected total multiplicity per edge:
    // - an edge on >=1 region boundary: (# untouched outside triangles) + 1
    //   per bounding region (each keep-boundary CDT emits its loop edges
    //   exactly once) — in a closed production mesh that is 2; the general
    //   form also holds at pre-existing mesh boundaries;
    // - any other new-triangle edge (an interior CDT chord): exactly 2.
    // Every formerly-defective cluster edge must end at its expected count or
    // vanish entirely. Any violation bails with the mesh untouched.
    let mut post_use: BTreeMap<(u32, u32), usize> = BTreeMap::new();
    for tri in &next_tris {
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            let (u, w) = (tri[i], tri[j]);
            let e = if u < w { (u, w) } else { (w, u) };
            *post_use.entry(e).or_default() += 1;
        }
    }
    let expected = |e: &(u32, u32)| -> usize {
        let bcount = plans.iter().filter(|p| p.boundary.contains(e)).count();
        if bcount > 0 {
            let outside = edge_use
                .get(e)
                .map(|ts| ts.iter().filter(|t| !in_regions.contains(t)).count())
                .unwrap_or(0);
            outside + bcount
        } else {
            2
        }
    };
    for &e in &new_edge_set {
        if post_use.get(&e).copied().unwrap_or(0) != expected(&e) {
            return bail("postcondition: new edge off its expected multiplicity");
        }
    }
    for (&(u, w), ts) in edge_use {
        if ts.len() != 2 && (cluster.contains(&u) || cluster.contains(&w)) {
            let n = post_use.get(&(u, w)).copied().unwrap_or(0);
            if n != 0 && n != expected(&(u, w)) {
                return bail("postcondition: defective edge not resolved");
            }
        }
    }
    // inc-4c-2: every removed chain edge either vanished or persists as an
    // ordinary manifold chord; every rewritten chain edge is claimed by
    // exactly TWO plans (one triangle per side — a chain constrained on one
    // side only would leave an unpaired seam).
    for &e in &removed_chain {
        let n = post_use.get(&e).copied().unwrap_or(0);
        if n != 0 && n != 2 {
            return bail("postcondition: removed chain edge unresolved");
        }
    }
    for &(_, e) in &added_chain {
        let claims = plans.iter().filter(|p| p.boundary.contains(&e)).count();
        if claims != 2 {
            return bail("postcondition: rewritten chain edge not two-sided");
        }
        if post_use.get(&e).copied().unwrap_or(0) != 2 {
            return bail("postcondition: rewritten chain edge not use-2");
        }
    }
    // The repair may never mint a RENDER-DEGENERATE triangle (height below
    // the render channel's resolution — the assay `no_degenerate_triangles`
    // criterion): the re-CDT of a chain still carrying sub-render needle
    // verts can be forced into such a sliver, which would ship as a
    // silent-wrong. Fail closed instead (the loud STOP stands); the §4.3
    // sub-render sample cleanup is its own increment.
    {
        let max_abs = mesh
            .verts
            .iter()
            .flat_map(|p| p.as_array())
            .fold(0.0f64, |m, c| m.max(c.abs()));
        let height_floor = 4.0 * max_abs * (f32::EPSILON as f64);
        for plan in &plans {
            for g in &plan.new_tris {
                let (a, b, c) = (
                    mesh.verts[g[0] as usize].as_array(),
                    mesh.verts[g[1] as usize].as_array(),
                    mesh.verts[g[2] as usize].as_array(),
                );
                let av = tri_area_vector(a, b, c);
                let area = (av[0] * av[0] + av[1] * av[1] + av[2] * av[2]).sqrt() / 2.0;
                let d2 = |p: [f64; 3], q: [f64; 3]| {
                    (p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)
                };
                let longest = d2(a, b).max(d2(b, c)).max(d2(c, a)).sqrt();
                if longest <= 0.0 || 2.0 * area / longest < height_floor {
                    if probe {
                        eprintln!(
                            "[p3b-fanfix] render-degenerate new tri {g:?} \
                             height {:.3e} < floor {height_floor:.3e}",
                            if longest > 0.0 {
                                2.0 * area / longest
                            } else {
                                0.0
                            }
                        );
                    }
                    return bail("postcondition: repair minted a render-degenerate triangle");
                }
            }
        }
    }
    *mesh = Mesh::new(std::mem::take(&mut mesh.verts), next_tris);
    *attr_vec = next_attr;
    Some(())
}

/// N50 (spec `yang_n50_f32_render_twin_weld`, deviation N50): collapse two
/// DISTINCT output vertices that are **bitwise-identical after rounding to
/// f32** — the exact G1 render-collapse criterion (kernel-v2
/// `f32_render_degenerate`, B2 clause). This is the 3D, output-magnitude
/// completion of N47's `weld_coincident_relocated`:
///
/// - N47 reaches only `moved`×`moved` relocated pairs; the R0012/R0098 twins are
///   NON-relocated Cherchi arrangement vertices minted by near-coincident
///   Stage-0 overlay sweep-event columns (N48/N49). After the FINAL Stage-4
///   relocation onto the exact curves the pair converges to within f32 render
///   precision at the OUTPUT (world) magnitude, surviving every earlier merge and
///   tripping G1 downstream (`planar triangle collapsed at render precision`).
/// - The criterion is the f32 **bit-key** `[(x as f32).to_bits(), …]`, not a
///   model band. Two vertices that round to the same f32 bits are the same
///   rendered point — collapsing them is render-invariant. The key is LOCAL by
///   construction (f32 ulp ≈ `|coord|·2⁻²³`), so it never over-merges a
///   near-origin pair in a far-flung model the way a global-`scale` `TAU_MODEL`
///   band does (the refuted N49 approach). Grouping by exact f32 cell is an
///   equivalence relation, so the weld never single-linkages across distinct
///   render cells (the N49 fault-1 / F0090 rim-drop hazard).
///
/// Runs on the FINAL mesh (after Stage-4 relocation and the KV15b collapse,
/// immediately before `emit_topology`, whose output vertices are 1:1 with
/// `mesh.verts`). `collapse_vertex` is the proven watertight-preserving
/// edge-collapse; iterate to a fixed point (one pair per BTreeMap-ordered sweep,
/// min-index survivor). Byte-identical no-op when no two live verts share an f32
/// cell (the overwhelming-majority fast path). Returns whether any pair welded.
///
/// RETIRED from the production path (§4.4.1 epic I4-1, 2026-08-15) — the sole
/// confirmed hack of the weld family; see the retirement history at
/// `weld_enabled`'s former site and `docs/yang_deviations.md` §N50. Banked as
/// a unit-tested primitive (`tests_unit/n50_f32_render_twin.rs`).
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn weld_f32_render_twins(
    mesh: &mut Mesh,
    attribution: &mut Vec<Option<TriangleAttribution>>,
) -> bool {
    let f32_key = |p: &Point3| -> [u32; 3] {
        let a = p.as_array();
        [
            (a[0] as f32).to_bits(),
            (a[1] as f32).to_bits(),
            (a[2] as f32).to_bits(),
        ]
    };
    let mut welded = false;
    loop {
        // Group live (still triangle-referenced) verts by f32 render cell.
        let mut buckets: std::collections::BTreeMap<[u32; 3], std::collections::BTreeSet<u32>> =
            std::collections::BTreeMap::new();
        for tri in &mesh.tris {
            for &v in tri {
                buckets
                    .entry(f32_key(&mesh.verts[v as usize]))
                    .or_default()
                    .insert(v);
            }
        }
        // First cell (deterministic key order) holding two distinct verts.
        let pair = buckets.values().find(|g| g.len() > 1).map(|g| {
            let mut it = g.iter();
            let survivor = *it.next().expect("len > 1"); // min index (BTreeSet)
            let victim = *it.next().expect("len > 1");
            (victim, survivor)
        });
        match pair {
            Some((victim, survivor)) => {
                if std::env::var_os("YANG_F32_WELD_PROBE").is_some() {
                    eprintln!(
                        "[f32-weld] victim={victim} survivor={survivor} p={:?}",
                        mesh.verts[survivor as usize]
                    );
                }
                collapse_vertex(mesh, attribution, victim, survivor);
                welded = true;
            }
            None => break,
        }
    }
    welded
}

/// How many DISTINCT analytic surfaces `pos` lies on, counted over the faces
/// carried by the triangles incident to `vi` — the KV15b I1b "richness" measure
/// (spec `kv15b_mint_site_subresolution_collapse` §I1b-curved).
///
/// Incidence is certified at [`junction_certificate_band`], the same band that
/// certifies a Stage-4 exact junction, so a chord-level (un-relocated) curved
/// sample contributes nothing: the count is strictly richer only for positions
/// Stage 4 actually placed ON the surface. Counting EVERY surface, not planes
/// alone, is the 2026-08-19 R0047 amendment.
///
/// Richness ranks two candidate positions for one model point: the richer one
/// carries more analytic authority, so it is the one a merge must keep — merging
/// into the poorer one evicts a face-loop vertex off a surface it lies on.
pub(crate) fn surface_incidence_count(
    mesh: &Mesh,
    attribution: &[Option<TriangleAttribution>],
    a: &BRep,
    b: &BRep,
    vi: u32,
    pos: [f64; 3],
) -> usize {
    carried_surfaces(mesh, attribution, a, b, vi, pos).len()
}

/// The DISTINCT analytic surfaces `pos` lies on, over the faces carried by the
/// triangles incident to `vi` — [`surface_incidence_count`]'s underlying set.
///
/// The set, not just its size, is what decides whether two positions may be
/// IDENTIFIED: a merge is authority-preserving exactly when the victim's set is
/// contained in the survivor's (see §4-I8 in
/// `specs/yang_441_trim_cdt_construction.md`). Two sets of equal size that
/// differ are two DISTINCT model points, which no merge can join.
pub(crate) fn carried_surfaces(
    mesh: &Mesh,
    attribution: &[Option<TriangleAttribution>],
    a: &BRep,
    b: &BRep,
    vi: u32,
    pos: [f64; 3],
) -> Vec<Surface> {
    let mut seen: Vec<Surface> = Vec::new();
    for (t, tri) in mesh.tris.iter().enumerate() {
        if !tri.contains(&vi) {
            continue;
        }
        let Some(att) = attribution.get(t).copied().flatten() else {
            continue;
        };
        let faces = match att.input {
            InputId::A => a.faces(),
            InputId::B => b.faces(),
        };
        let Some(face) = faces.get(att.face as usize) else {
            continue;
        };
        let surf = face.surface;
        let on = surface_distance_and_normal(surf, pos)
            .is_some_and(|(f, _)| f.abs() <= junction_certificate_band(pos, surf));
        if !on {
            continue;
        }
        if !seen.contains(&surf) {
            seen.push(surf);
        }
    }
    seen
}

/// KV15b (spec `kv15b_mint_site_subresolution_collapse`): collapse
/// sub-resolution intersection segments before Phase-B emission.
///
/// The exact arrangement legitimately mints two crossings of near-parallel
/// geometry closer than the model tolerance (R0076: gear flank grazing a box
/// edge, 3.999e-8 / 6.472e-8 pairs). Emitted as two distinct output vertices,
/// the pair is POISON downstream: the Stage-0 coplanar clustering band floor
/// is exactly `TAU_MODEL`, and Stage-6 patch walks of the next boolean
/// disagree over the twin (the measured F0070/KV15 mechanism at sub-floor
/// scale). Per A8.1/A14 `TAU_MODEL` is the single central vertex-merge
/// resolution — two points closer than it ARE one model point — so emission
/// hygiene collapses the segment at the mint site.
///
/// Eligibility is FULL-PROVENANCE (I3): only consecutive intersection-curve
/// vertices — keys of `intersection_curves` — are candidates; inherited
/// operand geometry (e.g. legitimately sub-floor micro-profile corners) is
/// never touched. This is one order TIGHTER than the reverted-R0091
/// `MIN_FEATURE_SIZE` global widening and scoped to the increment-4
/// provenance pattern. One sweep over the ORIGINAL segment set in
/// deterministic `BTreeMap` order; endpoints resolve through prior collapses
/// (min-index survivor, I1 — the survivor keeps its own exact coordinates,
/// never an average), and a segment whose RESOLVED length is ≥ `TAU_MODEL`
/// stays (I2/B5 — no chain drift). Exact-zero pairs are the M-B
/// emission-identification class and stay untouched here (B3).
pub(crate) fn collapse_subresolution_intersection_segments(
    mesh: &mut Mesh,
    attribution: &mut Vec<Option<TriangleAttribution>>,
    intersection_curves: &std::collections::BTreeMap<(u32, u32), Curve>,
    a: &BRep,
    b: &BRep,
) -> bool {
    let mut redirect: std::collections::BTreeMap<u32, u32> = std::collections::BTreeMap::new();
    fn resolve(redirect: &std::collections::BTreeMap<u32, u32>, mut v: u32) -> u32 {
        while let Some(&n) = redirect.get(&v) {
            v = n;
        }
        v
    }
    // C0036 amendment (spec `kv15b_mint_site_subresolution_collapse` I1b):
    // the surviving POSITION is the pair's SURFACE-incidence-richer endpoint.
    // A sub-floor pair often joins the TRUE junction of k carried surfaces
    // with a near-degenerate crossing OFF one of them by the sub-floor gap
    // (the C0036 near-coplanar seam corner: the exact 3-plane corner vs a
    // crossing 1.75e-8 off the tilted wall). Keeping the min-index position
    // blindly evicts a face-loop vertex off its carried analytic surface,
    // twisting the loop (the fitted Newell then misses the exact input
    // corners — the debug-tier NonPlanarFace red). The topological survivor
    // stays min-index (I1 determinism); only its COORDINATES may adopt the
    // strictly richer endpoint. Ties keep the survivor's own coordinates
    // (byte-identical to the shipped behavior).
    //
    // 2026-08-19 (R0047 anchor, spec §I1b-curved): the measure counts EVERY
    // distinct analytic surface the endpoint lies on (within the same
    // `junction_certificate_band` that certifies Stage-4 exact junctions),
    // not planes alone. The planar-only count read a certified
    // plane∩cone∩cone crease junction (3 surfaces) and its cone∩plane
    // interior neighbour (2 surfaces) as a 1–1 TIE, kept the neighbour's
    // coordinates, and emitted the merged vertex ON cone-1's ellipse but
    // 1.4e-9 OFF cone-2's — kernel-v2's "output ellipse-arc endpoint does
    // not lie on its ellipse". Chord-level (un-relocated) curved samples are
    // NOT on their surface at certificate precision, so they contribute
    // nothing — the count is strictly richer only for positions Stage 4
    // actually placed on the surface.
    let plane_count = |mesh: &Mesh,
                       attribution: &[Option<TriangleAttribution>],
                       vi: u32,
                       pos: [f64; 3]|
     -> usize { surface_incidence_count(mesh, attribution, a, b, vi, pos) };
    let mut any = false;
    for &(u, v) in intersection_curves.keys() {
        let (ru, rv) = (resolve(&redirect, u), resolve(&redirect, v));
        if ru == rv {
            continue;
        }
        let p = mesh.verts[ru as usize].as_array();
        let q = mesh.verts[rv as usize].as_array();
        let d2 = (p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2);
        // #169 N56: scale-relative sub-resolution band `TAU_MODEL·(1+scale)`
        // (was the absolute `TAU_MODEL²` floor N53 flagged). Both endpoints lie
        // on the intersection curve, so an edge below the model-coincidence
        // resolution is a redundant curve sample — Yang §4.3 "remove a point too
        // close to another on the same loop." Scale-relative because a fixed
        // gap is numerical noise at large coordinates; the SAME band coincident
        // and the stage-5 planarity wall use. Measured collapses are ~1e-8…1e-7
        // (genuinely sub-resolution); recovers R0076/R0088/F0078/F0079/F0084.
        let scale = p
            .iter()
            .chain(q.iter())
            .fold(0.0f64, |m, &c| m.max(c.abs()));
        let band = cad_primitives::TAU_MODEL * (1.0 + scale);
        if d2 == 0.0 || d2 >= band * band {
            continue;
        }
        let survivor = ru.min(rv);
        let victim = ru.max(rv);
        // I1b: adopt the plane-incidence-richer position onto the surviving
        // index (strictly richer only).
        {
            let sp = mesh.verts[survivor as usize].as_array();
            let vp = mesh.verts[victim as usize].as_array();
            let cs = plane_count(mesh, attribution, survivor, sp);
            let cv = plane_count(mesh, attribution, victim, vp);
            if cv > cs {
                mesh.verts[survivor as usize] = mesh.verts[victim as usize];
            }
            if std::env::var_os("YANG_DOUBLECOVER_PROBE").is_some() {
                eprintln!("[collapse-site] kv15b plane_count survivor={survivor} cs={cs} victim={victim} cv={cv}");
            }
        }
        if std::env::var_os("YANG_DOUBLECOVER_PROBE").is_some() {
            eprintln!(
                "[collapse-site] kv15b victim={victim} p=({:.17e},{:.17e},{:.17e}) \
                 survivor={survivor} q=({:.17e},{:.17e},{:.17e}) dist={:.3e}",
                mesh.verts[victim as usize].as_array()[0],
                mesh.verts[victim as usize].as_array()[1],
                mesh.verts[victim as usize].as_array()[2],
                mesh.verts[survivor as usize].as_array()[0],
                mesh.verts[survivor as usize].as_array()[1],
                mesh.verts[survivor as usize].as_array()[2],
                d2.sqrt(),
            );
        }
        collapse_vertex(mesh, attribution, victim, survivor);
        redirect.insert(victim, survivor);
        any = true;
    }
    any
}

/// #194 (spec `yang_194_subtauwork_edge_collapse`): collapse mesh EDGES
/// shorter than working precision before Phase-B emission.
///
/// The exact arrangement can mint the SAME junction twice with swapped LPI
/// roles when an operand's own tessellation self-grazes (the F0082
/// Extrude-12 seal corner: two verts 5.5e-14 apart joined by a mesh edge,
/// spawning a zero-area flap whose third edge use is the χ=3 book edge).
/// Nothing existing owns the pair: the I6/KV15 near-weld excludes
/// curved-incident verts (the KV9 lens-tip record), KV15b is
/// provenance-restricted to `intersection_curves` keys (A×B junctions), and
/// Stage-4's KV9 collapse reconciles only this op's curve junctions.
///
/// Domain = ALL undirected mesh edges (deterministic `BTreeSet` order); the
/// band does the scoping: resolved length in the OPEN interval
/// `(0, TAU_WORK·(1+scale))` — five orders TIGHTER than KV15b. An edge
/// below working precision is not a representable segment; collapsing it is
/// not proximity welding. Min-resolved-index survivor keeps its own bits
/// (I1); resolved re-measure prevents chain drift (I2/B5); exact-zero edges
/// are the M-B identification class and stay (B3). KV9's UNCONNECTED ring
/// duplicates are untouched by construction — no edge joins them.
pub(crate) fn collapse_subtauwork_mesh_edges(
    mesh: &mut Mesh,
    attribution: &mut Vec<Option<TriangleAttribution>>,
) -> bool {
    let mut edges: std::collections::BTreeSet<(u32, u32)> = std::collections::BTreeSet::new();
    for tri in &mesh.tris {
        for k in 0..3 {
            let (u, v) = (tri[k], tri[(k + 1) % 3]);
            edges.insert((u.min(v), u.max(v)));
        }
    }
    let mut redirect: std::collections::BTreeMap<u32, u32> = std::collections::BTreeMap::new();
    fn resolve(redirect: &std::collections::BTreeMap<u32, u32>, mut v: u32) -> u32 {
        while let Some(&n) = redirect.get(&v) {
            v = n;
        }
        v
    }
    let mut any = false;
    for &(u, v) in &edges {
        let (ru, rv) = (resolve(&redirect, u), resolve(&redirect, v));
        if ru == rv {
            continue;
        }
        let p = mesh.verts[ru as usize].as_array();
        let q = mesh.verts[rv as usize].as_array();
        let d2 = (p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2);
        let scale = p
            .iter()
            .chain(q.iter())
            .fold(0.0f64, |m, &c| m.max(c.abs()));
        let band = cad_primitives::TAU_WORK * (1.0 + scale);
        if d2 == 0.0 || d2 >= band * band {
            continue;
        }
        let survivor = ru.min(rv);
        let victim = ru.max(rv);
        if std::env::var_os("YANG_DOUBLECOVER_PROBE").is_some() {
            eprintln!(
                "[collapse-site] s194 victim={victim} survivor={survivor} dist={:.3e}",
                d2.sqrt(),
            );
        }
        collapse_vertex(mesh, attribution, victim, survivor);
        redirect.insert(victim, survivor);
        any = true;
    }
    any
}

/// PR-YR11 helper: drop mesh vertices no surviving triangle references and remap
/// triangle indices + the Stage-4 `relocations` keys to the dense vertex set.
///
/// A §4.5.3 [`collapse_vertex`] keeps the full vertex array (it only drops the
/// now-degenerate triangles), leaving the collapsed-away vertices DANGLING. The
/// internal per-shell `check_watertight_2manifold` gate ignores them (it sums V
/// over triangle-referenced verts only), but they inflate a caller's GLOBAL
/// `V − E + F`. An output mesh must carry no unreferenced vertices, so this
/// compaction runs after Stage 4. It is a strict NO-OP (returns early, mesh and
/// `relocations` untouched) when every vertex is already referenced — so the
/// no-collapse paths (planar / perpendicular-circle / on-curve mock) stay
/// byte-identical.
///
/// Returns the old→new index remap when it compacted (`None` on the no-op path),
/// so callers holding vertex-keyed side tables can re-key them. `None` in a slot
/// means that vertex did not survive.
pub(crate) fn compact_unreferenced_verts(
    mesh: &mut Mesh,
    relocations: &mut Vec<(u32, f64)>,
) -> Option<Vec<Option<u32>>> {
    let n = mesh.verts.len();
    let mut referenced = vec![false; n];
    for tri in &mesh.tris {
        for &v in tri {
            referenced[v as usize] = true;
        }
    }
    if referenced.iter().all(|&r| r) {
        return None; // no danglers — byte-identical no-op.
    }
    // Dense remap preserving the relative order of surviving vertices.
    let mut remap: Vec<Option<u32>> = vec![None; n];
    let mut new_verts: Vec<Point3> = Vec::with_capacity(n);
    for (i, &r) in referenced.iter().enumerate() {
        if r {
            remap[i] = Some(new_verts.len() as u32);
            new_verts.push(mesh.verts[i]);
        }
    }
    let new_tris: Vec<[u32; 3]> = mesh
        .tris
        .iter()
        .map(|tri| {
            // Invariant: `referenced` was built from this same triangle list
            // above, so every triangle vertex has a `Some` remap entry.
            tri.map(|v| {
                remap[v as usize]
                    .expect("compact_unreferenced_verts: triangle vertex not marked referenced")
            })
        })
        .collect();
    *mesh = Mesh::new(new_verts, new_tris);
    // Remap (and drop) relocation keys: a relocation referencing a collapsed-away
    // (now-unreferenced) vertex is no longer in the mesh, so it is dropped.
    let remapped: Vec<(u32, f64)> = relocations
        .iter()
        .filter_map(|&(v, t)| remap[v as usize].map(|nv| (nv, t)))
        .collect();
    *relocations = remapped;
    Some(remap)
}

/// PR-YR10 (Yang §4.4.1 + §4.5.3): Stage 4 — relocate the mesh intersection
/// points onto the exact analytical `Circle` curves, then correct any reversed
/// intersection points by the §4.5.3 polyline-tangent sweep.
///
/// Returns `(relocations, collapsed)` where `relocations` is the list of
/// `(vertex, t)` pairs (the circle-frame angle `t` for every relocated OR
/// already-on-curve intersection vertex — the caller maps these to
/// `BRepEdge { edge, t }` tessellation sources once the output edges exist), and
/// `collapsed` is `true` iff the §4.5.3 sweep edge-collapsed at least one
/// vertex (so the caller must recompute Phase A).
///
/// LOUD STOPs (P9/P10), never a silent snap / tolerance widening / no-op:
/// - `Stage4RegionInvalid { OnAxis }` — a point projects onto the circle/cylinder
///   axis.
/// - `Stage4RegionInvalid { OffCurveBeyondChordBand }` — residual `ρ > d_ε`.
/// - `Stage4RegionInvalid { LoopTooSmall }` — a loop shrank below 3 verts.
/// - `Stage4RegionInvalid { InvertedTriangle / DegenerateTriangle }` — a
///   relocated triangle is inverted / degenerate after correction.
/// - `Stage4ReversalUnresolved` — the §4.5.3 sweep could not resolve a reversal.
/// - `Stage4RegionInvalid { LocalRefinementRequired }` — relocate + §4.5.3 left
///   a region invalid (genuine §4.5.2 territory, out of scope).
///
/// No-skip audit (anti-disproven-attempt): a `processed` set tracks EVERY conic
/// edge endpoint; it must equal the relocation-key set at the end. The function
/// NEVER `continue`s past a `Circle` edge endpoint.
/// §4-I9 — the RELOCATION-DOMAIN postcondition (spec
/// `yang_441_trim_cdt_construction.md` §4-I9).
///
/// A Stage-4 relocation slides a vertex onto the exact analytic solution its arm
/// converged to. That solution is computed against SURFACES, which are
/// unbounded; the vertex lives on a bounded FACE. When the exact solution lies
/// beyond the face, the arm still converges — and the vertex slides straight
/// past the carrier's own endpoint, the model corner where a third face joins.
/// The mesh is then folded: the boundary walks out to the relocated vertex and
/// back over the corner, which Stage 6 emits and the render CDT rejects three
/// stages later with a message about a ring, naming neither the stage nor the
/// defect.
///
/// The certificate has TWO legs, and needs both. (1) The crossed neighbour lies
/// ON the travel segment, strictly inside it, at the project's shared 1e-9
/// relative collinearity identity
/// ([`crate::stage4_construct::point_on_segment_interior`]) — measured
/// 2026-08-20 at 6.4e-13 / 1.4e-17 / 0.0 / 6.2e-17 on the four ring-reject
/// sites, against 5.0 %–6.6 % of travel for the two Fig-11 merges that are
/// LEGITIMATE (F0045, R0090). Seven orders separate the populations, so this
/// does not preempt the §4-I6 merge. (2) That neighbour is a domain ENDPOINT,
/// not a sample: it carries a surface the relocated position is OFF, so a third
/// face joins there and the carrier STOPS. Without leg (2) the check would also
/// fire on a plain sample of the traveller's own carrier, which Yang's near-curve
/// vertex removal legitimately owns — measured 2026-08-20: leg (2) is what keeps
/// F0064's coplanar-boundary case out of the fire list.
///
/// This is a POSTCONDITION over the whole stage rather than a check at each
/// arm: relocation happens at more than a dozen sites here plus
/// `apply_boundary_relocations`, and every repair that might dissolve the
/// configuration (the P3b beyond-corner trim, the collapsed-fan
/// re-triangulation, the reversal sweep, the sub-feature merge) runs before the
/// end. Only what SURVIVES all of them is a genuine domain violation.
///
/// Gate `YANG_S4_CARRIER_DOMAIN`: unset = **ON** (STOP on the first violation,
/// in deterministic vertex order); `0`/`off` = the dev A/B off-knob; `census` =
/// report every violation and return Ok, a measurement mode with no behaviour
/// change. Full-corpus census 2026-08-20: fires on R0004/R0011/R0044/R0074/R0085
/// only — every one already an ERROR, so arming cannot cost a correct case, and
/// no category moves.
/// Vertex → the `(input, face)` patches its incident attributed triangles carry.
/// Built once per census; every predicate below reads it instead of rescanning
/// the triangle list, so a whole-mesh census is O(V·deg) and not O(V·T).
fn build_patch_map(
    mesh: &Mesh,
    attribution: &[Option<TriangleAttribution>],
) -> std::collections::BTreeMap<u32, std::collections::BTreeSet<(InputId, u32)>> {
    let mut out: std::collections::BTreeMap<u32, std::collections::BTreeSet<(InputId, u32)>> =
        std::collections::BTreeMap::new();
    for (ti, tri) in mesh.tris.iter().enumerate() {
        if let Some(Some(att)) = attribution.get(ti) {
            for &tv in tri {
                out.entry(tv).or_default().insert((att.input, att.face));
            }
        }
    }
    out
}

/// Adjacency over LIVE triangles only: a vertex the collapses orphaned is no
/// longer anyone's neighbour and cannot bound (or fold) anything.
fn build_live_adjacency(
    mesh: &Mesh,
) -> std::collections::BTreeMap<u32, std::collections::BTreeSet<u32>> {
    let mut adj: std::collections::BTreeMap<u32, std::collections::BTreeSet<u32>> =
        std::collections::BTreeMap::new();
    for tri in &mesh.tris {
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            let (u, w) = (tri[i], tri[j]);
            if u == w {
                continue;
            }
            adj.entry(u).or_default().insert(w);
            adj.entry(w).or_default().insert(u);
        }
    }
    adj
}

/// A vertex of the intersection curve: its incident attributed patches span BOTH
/// inputs.
fn vertex_on_curve(
    patches: &std::collections::BTreeMap<u32, std::collections::BTreeSet<(InputId, u32)>>,
    v: u32,
) -> bool {
    let Some(p) = patches.get(&v) else {
        return false;
    };
    p.iter().any(|x| x.0 == InputId::A) && p.iter().any(|x| x.0 == InputId::B)
}

/// "Converged to a distance of 0" in Yang §4.5's sense: the vertex's current
/// position lies on a surface of EACH operand, at the shared certificate band.
/// Whether it did so WITHIN ITS DOMAIN is the separate question §4-I9 answers,
/// so callers subtract that fire list themselves.
///
/// One definition, shared by the postcondition, the strategy selector and the
/// failure-population census — a second copy would be free to drift.
fn vertex_converged(
    mesh: &Mesh,
    patches: &std::collections::BTreeMap<u32, std::collections::BTreeSet<(InputId, u32)>>,
    a: &BRep,
    b: &BRep,
    v: u32,
) -> bool {
    let (mut ok_a, mut ok_b) = (false, false);
    let pos = mesh.verts[v as usize].as_array();
    let Some(faces_of_v) = patches.get(&v) else {
        return false;
    };
    for &(input, face) in faces_of_v {
        let faces = match input {
            InputId::A => a.faces(),
            InputId::B => b.faces(),
        };
        let Some(f) = faces.get(face as usize) else {
            continue;
        };
        let surf = f.surface;
        if surface_distance_and_normal(surf, pos)
            .is_some_and(|(d, _)| d.abs() <= junction_certificate_band(pos, surf))
        {
            match input {
                InputId::A => ok_a = true,
                InputId::B => ok_b = true,
            }
        }
    }
    ok_a && ok_b
}

/// Run the §4-I11 failure-population census against a mesh in whatever state it
/// is in, building the shared predicates first.
///
/// `tag` names the vantage point, because the two are NOT equivalent: `ok` is
/// the end of a Stage 4 that completed, `stopped` is the mesh frozen at a STOP,
/// where later repairs never ran.
fn census_failure_population(
    mesh: &Mesh,
    attribution: &[Option<TriangleAttribution>],
    a: &BRep,
    b: &BRep,
    entry: &[[f64; 3]],
    i9_sites: &[(u32, u32)],
    tag: &str,
) {
    let patches = build_patch_map(mesh, attribution);
    let on_curve = |v: u32| vertex_on_curve(&patches, v);
    let converged = |v: u32| vertex_converged(mesh, &patches, a, b, v);
    failure_population_census(
        mesh, a, b, &patches, i9_sites, entry, &on_curve, &converged, tag,
    );
}

/// How many DISTINCT analytic surfaces of each operand does `pos` lie on, over
/// the faces carried by `vid`'s incident triangles?
///
/// This is the reading Yang's Fig-13 clause turns on
/// (`refs/text/yang2025_hybrid_boolean.txt:637-651`): a point lying on ONE
/// surface per operand is INTERIOR to that surface; a point lying on TWO
/// adjacent surfaces of the same operand is on their shared BOUNDARY CURVE, and
/// a point on THREE is the corner `s` "where more than two surfaces meet".
///
/// Takes the precomputed vertex→patches map rather than rescanning the triangle
/// list, so a whole-mesh census is O(V·deg) and not O(V·T).
fn carrier_counts(
    patches: &std::collections::BTreeMap<u32, std::collections::BTreeSet<(InputId, u32)>>,
    a: &BRep,
    b: &BRep,
    vid: u32,
    pos: [f64; 3],
) -> (usize, usize) {
    let (sa, sb) = carrier_surface_sets(patches, a, b, vid, pos);
    (sa.len(), sb.len())
}

/// The SETS behind [`carrier_counts`]: which distinct analytic surfaces of each
/// operand `pos` lies on, over the faces carried by `vid`'s incident triangles.
/// Split out (§4.5.1 inc-2a) because the repair-preview census needs the
/// surfaces themselves — the bounds' shared far-operand surface is what decides
/// a drift region (one pair, simple projection) from a straddle region
/// (cross-boundary continuation).
fn carrier_surface_sets(
    patches: &std::collections::BTreeMap<u32, std::collections::BTreeSet<(InputId, u32)>>,
    a: &BRep,
    b: &BRep,
    vid: u32,
    pos: [f64; 3],
) -> (Vec<Surface>, Vec<Surface>) {
    let (mut sa, mut sb): (Vec<Surface>, Vec<Surface>) = (Vec::new(), Vec::new());
    let Some(faces_of_v) = patches.get(&vid) else {
        return (sa, sb);
    };
    for &(input, face) in faces_of_v {
        let faces = match input {
            InputId::A => a.faces(),
            InputId::B => b.faces(),
        };
        let Some(f) = faces.get(face as usize) else {
            continue;
        };
        let surf = f.surface;
        if !surface_distance_and_normal(surf, pos)
            .is_some_and(|(d, _)| d.abs() <= junction_certificate_band(pos, surf))
        {
            continue;
        }
        let bucket = match input {
            InputId::A => &mut sa,
            InputId::B => &mut sb,
        };
        if !bucket.contains(&surf) {
            bucket.push(surf);
        }
    }
    (sa, sb)
}

/// One reported member of the §4.5 failure population: the vertex, which half of
/// the population it belongs to, its Fig-13 class, and its carrier counts at the
/// two ends of its step.
type CandidateRow = (
    u32,
    &'static str,
    &'static str,
    (usize, usize),
    (usize, usize),
);

/// §4-I11 — does §4.5.1 have ANY customer here?
///
/// §4-I10 (f) measured the §4-I9 fire list as 24/24 EXCLUDED from the paper's
/// first strategy by the Fig-13 clause. That answers the question for one
/// population. This answers it for the whole of Stage 4's output, so the epic
/// can tell whether §4.5.1 is worth building at all or whether §4.5.2 absorbs
/// the entire §4.5 budget.
///
/// **The population is the paper's, not ours.** §4.5: *"we collect the point
/// pairs that cannot converge to a distance of 0 within their domains"*
/// (`:652-656`). That has two halves, and both are enumerated here:
///
/// - **in-domain non-convergence** — the optimization ran on the point and its
///   final position does not lie on a surface of EACH operand;
/// - **out-of-domain convergence** — it does lie on both, but past its carrier's
///   own endpoint: the §4-I9 fire list.
///
/// Each member is then classified by the Fig-13 discriminator: INTERIOR (one
/// surface per operand at both ends of its step ⇒ §4.5.1's stated scope) or
/// BOUNDARY-GLIDING (two or more of one operand at both ends ⇒ excluded, §4.5.2).
///
/// **Honest limits, stated because they bound the conclusion.** (1) "The
/// optimization ran on it" is proxied by "Stage 4 MOVED it" — a relocation that
/// failed without writing a position is invisible here, so the in-domain half is
/// a LOWER bound. (2) This is a postcondition, so cases whose Stage 4 STOPs
/// earlier never report. (3) Denominators are printed with every count; a bare
/// count from this instrument is not a finding.
#[allow(clippy::too_many_arguments)]
fn failure_population_census(
    mesh: &Mesh,
    a: &BRep,
    b: &BRep,
    patches: &std::collections::BTreeMap<u32, std::collections::BTreeSet<(InputId, u32)>>,
    i9_sites: &[(u32, u32)],
    entry: &[[f64; 3]],
    on_curve: &dyn Fn(u32) -> bool,
    converged: &dyn Fn(u32) -> bool,
    tag: &str,
) {
    let i9: std::collections::BTreeSet<u32> = i9_sites.iter().map(|&(v, _)| v).collect();
    let n = entry.len().min(mesh.verts.len());

    // Tallies per half, bucketed by the point's INITIAL location — §4.5.1's own
    // wording is "the surface `S2` where the point is initially located", and
    // Fig-13's excluded category is the "boundary intersection points", i.e.
    // points LOCATED on boundary curves. Index: 0 = interior at pre,
    // 1 = boundary at pre, 2 = on neither (off its own carrier at pre).
    let (mut in_dom, mut out_dom) = ([0usize; 3], [0usize; 3]);
    // Of the boundary-at-pre members, how many also end on a boundary curve
    // (Fig-13's "glide ALONG") versus leave it? Reported so the exclusion can be
    // read either way and the answer does not depend on which reading is taken.
    let (mut glides, mut left_boundary) = (0usize, 0usize);
    let (mut curve_verts, mut moved_curve) = (0usize, 0usize);
    let mut interior_examples: Vec<CandidateRow> = Vec::new();

    for v in 0..n as u32 {
        if !on_curve(v) {
            continue;
        }
        curve_verts += 1;
        let (pre, post) = (entry[v as usize], mesh.verts[v as usize].as_array());
        let moved = pre != post;
        if moved {
            moved_curve += 1;
        }
        let is_i9 = i9.contains(&v);
        // Membership in the paper's failure population.
        let half = if is_i9 {
            "out-of-domain"
        } else if moved && !converged(v) {
            "in-domain"
        } else {
            continue;
        };
        let cpre = carrier_counts(patches, a, b, v, pre);
        let cpost = carrier_counts(patches, a, b, v, post);
        let (near_pre, near_post) = (cpre.0.max(cpre.1), cpost.0.max(cpost.1));
        let bucket = if near_pre >= 2 {
            // A BOUNDARY intersection point — Fig-13's excluded category.
            if near_post >= 2 {
                glides += 1;
            } else {
                left_boundary += 1;
            }
            1
        } else if near_pre == 1 {
            0 // INTERIOR to its surface at its initial location — §4.5.1's scope
        } else {
            2 // on no surface of either operand at pre — not a located point
        };
        // Examples are printed for INTERIOR (a §4.5.1 customer would be one) and
        // for the unclassified bucket, because a customer could hide in there
        // behind a classification that is too strict rather than behind a real
        // absence.
        if bucket != 1 && interior_examples.len() < 12 {
            interior_examples.push((
                v,
                half,
                if bucket == 0 { "INTERIOR" } else { "unlocated" },
                cpre,
                cpost,
            ));
        }
        if is_i9 {
            out_dom[bucket] += 1;
        } else {
            in_dom[bucket] += 1;
        }
    }

    eprintln!(
        "YANG_S45_POP at={tag} curve_verts={curve_verts} moved={moved_curve} \
         | in-domain: interior={} boundary={} unlocated={} \
         | out-of-domain(I9): interior={} boundary={} unlocated={} \
         | of-boundary: glides={glides} left={left_boundary}",
        in_dom[0], in_dom[1], in_dom[2], out_dom[0], out_dom[1], out_dom[2]
    );
    for (v, half, kind, cpre, cpost) in &interior_examples {
        eprintln!(
            "YANG_S45_POP   CANDIDATE v{v} half={half} class={kind} \
             pre=(A{},B{}) post=(A{},B{})",
            cpre.0, cpre.1, cpost.0, cpost.1
        );
    }
}

/// §4-I9's two-leg out-of-domain reading as a per-vertex predicate: did `w`
/// travel across a STILL neighbour lying ON its pre→post segment (leg 1, the
/// shared collinearity identity inside `point_on_segment_interior`) that
/// carries a surface the final position is OFF (leg 2 — a domain ENDPOINT,
/// not a sample)?
///
/// Extracted so the STOP-vantage walk (§4-I12) and the postcondition's census
/// cross-check share ONE reading; the postcondition's STOP path keeps its
/// richer inline diagnostics. Census-only callers.
fn vertex_crossed_domain_endpoint(
    mesh: &Mesh,
    attribution: &[Option<TriangleAttribution>],
    a: &BRep,
    b: &BRep,
    adj: &std::collections::BTreeMap<u32, std::collections::BTreeSet<u32>>,
    entry: &[[f64; 3]],
    w: u32,
) -> bool {
    let n = entry.len().min(mesh.verts.len());
    let wi = w as usize;
    if wi >= n {
        return false; // minted during Stage 4: no pre, travelled nowhere
    }
    let (wpre, wpost) = (entry[wi], mesh.verts[wi].as_array());
    if wpre == wpost {
        return false;
    }
    adj.get(&w).is_some_and(|nbrs| {
        nbrs.iter().any(|&q| {
            let qi = q as usize;
            if qi >= n {
                return false;
            }
            let qpos = mesh.verts[qi].as_array();
            if entry[qi] != qpos {
                return false; // both travelled — not a still carrier vertex
            }
            if !crate::stage4_construct::point_on_segment_interior(wpre, wpost, qpos) {
                return false;
            }
            carried_surfaces(mesh, attribution, a, b, q, qpos)
                .into_iter()
                .any(|surf| {
                    !surface_distance_and_normal(surf, wpost)
                        .is_some_and(|(d, _)| d.abs() <= junction_certificate_band(wpost, surf))
                })
        })
    })
}

/// §4.5's SECOND selector clause, measured: *"we only use the first strategy
/// in cases where the failure points are bounded by two successfully optimized
/// points **on the same surface**"* (`refs/text/yang2025_hybrid_boolean.txt:740-744`).
///
/// Walks the intersection curve outward from `v` along every branch to the
/// nearest vertex `good` accepts, then reports whether all such bounds share a
/// surface — and whether `v` itself lies on it. What "successfully optimized"
/// means is the CALLER's claim, because the two vantage points cannot compute
/// it the same way: the end-of-stage selector subtracts its §4-I9 fire list,
/// while the STOP-vantage caller (§4-I12) has no fire list — the postcondition
/// never ran — and re-takes I9's two-leg reading per candidate bound instead.
///
/// Bounds are DISTINCT VERTICES: two branches reaching the same converged
/// vertex (a loop around the erroneous region) are ONE bound — the paper's
/// clause names two POINTS `v0` and `v1`, not two arrivals. (The pre-I12
/// instrument deduped `(vertex, hops)` pairs, which could have double-counted
/// such a bound; no recorded measurement is affected — every §4-I10 site
/// reported distinct bound ids.)
///
/// Census-only; prints under `YANG_S45_SELECT`. Returns `Some((bounds,
/// common))` — the distinct bound vertices and their common surfaces — when
/// the first strategy's clause holds (≥2 distinct bounds sharing ≥1
/// surface), else `None`. The §4.5.1 inc-3 pair-Newton preview consumes the
/// bounds for failures whose own-curve chain is empty (torus-carried).
#[allow(clippy::too_many_arguments)]
fn selector_clause2_walk(
    mesh: &Mesh,
    attribution: &[Option<TriangleAttribution>],
    a: &BRep,
    b: &BRep,
    adj: &std::collections::BTreeMap<u32, std::collections::BTreeSet<u32>>,
    v: u32,
    on_curve: &dyn Fn(u32) -> bool,
    good: &dyn Fn(u32) -> bool,
) -> Option<(Vec<u32>, Vec<Surface>)> {
    let empty_adj: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
    let curve_nbrs = |w: u32| -> Vec<u32> {
        adj.get(&w)
            .unwrap_or(&empty_adj)
            .iter()
            .copied()
            .filter(|&x| on_curve(x))
            .collect()
    };
    // Walk one branch of the curve polyline, away from `v` through `start`,
    // until a successfully-optimized vertex is reached. Bounded, and it stops
    // rather than guessing wherever the curve is not locally a simple polyline.
    let walk = |v: u32, start: u32| -> Result<(u32, usize), &'static str> {
        let (mut prev, mut cur) = (v, start);
        for hop in 1..=64usize {
            if good(cur) {
                return Ok((cur, hop));
            }
            let nbrs: Vec<u32> = curve_nbrs(cur).into_iter().filter(|&x| x != prev).collect();
            match nbrs.len() {
                0 => return Err("curve ends"),
                1 => {
                    prev = cur;
                    cur = nbrs[0];
                }
                _ => return Err("curve branches"),
            }
        }
        Err("64 hops, no converged bound")
    };

    let branches = curve_nbrs(v);
    let mut bounds: Vec<(u32, usize)> = Vec::new();
    for &start in &branches {
        match walk(v, start) {
            Ok((w, hop)) => {
                eprintln!("YANG_S45_SELECT   v{v} branch via v{start}: bound v{w} at {hop} hop(s)");
                bounds.push((w, hop));
            }
            Err(why) => {
                eprintln!("YANG_S45_SELECT   v{v} branch via v{start}: NO BOUND ({why})");
            }
        }
    }
    bounds.sort_unstable();
    bounds.dedup_by_key(|&mut (w, _)| w);
    if bounds.len() < 2 {
        eprintln!(
            "YANG_S45_SELECT   v{v} VERDICT=SECOND_STRATEGY (§4.5.2) — \
             only {} distinct converged bound(s); the paper's first strategy \
             requires two",
            bounds.len()
        );
        return None;
    }
    // "on the same surface". Where the curve neighbourhood has degree > 2
    // the choice of WHICH two bounds must not decide the verdict, so
    // intersect over ALL of them: a surface common to every bound is common
    // to any pair of them, and its absence is reported as such.
    let mut common: Option<Vec<Surface>> = None;
    for &(w, _) in &bounds {
        let sw = carried_surfaces(
            mesh,
            attribution,
            a,
            b,
            w,
            mesh.verts[w as usize].as_array(),
        );
        common = Some(match common {
            None => sw,
            Some(prev) => prev.into_iter().filter(|x| sw.contains(x)).collect(),
        });
    }
    let common = common.unwrap_or_default();
    // Is the traveller itself on that shared surface? §4.5.1 replaces the
    // erroneous region with the MIDPOINT of the two bounds and re-optimizes
    // it, so the region and its bounds must live on one surface together.
    let v_surfs = carried_surfaces(
        mesh,
        attribution,
        a,
        b,
        v,
        mesh.verts[v as usize].as_array(),
    );
    let v_on_common = common.iter().filter(|x| v_surfs.contains(x)).count();
    eprintln!(
        "YANG_S45_SELECT   v{v} bounds={} common_surfaces={} \
         traveller_on_common={v_on_common} VERDICT={}",
        bounds.len(),
        common.len(),
        if common.is_empty() {
            "SECOND_STRATEGY (§4.5.2) — bounds share no surface"
        } else {
            "FIRST_STRATEGY (§4.5.1) — all bounds on a common surface"
        }
    );
    for surf in &common {
        eprintln!("YANG_S45_SELECT     v{v} common surface {surf:?}");
    }
    if common.is_empty() {
        None
    } else {
        Some((bounds.iter().map(|&(w, _)| w).collect(), common))
    }
}

/// §4-I10 (d) — the paper's §4.5 STRATEGY SELECTOR, measured.
///
/// Yang 2025 §4.5 (`refs/text/yang2025_hybrid_boolean.txt:740-744`): *"we only
/// use the first strategy in cases where the failure points are bounded by two
/// successfully optimized points **on the same surface**. For other cases, we
/// apply the second strategy"* — §4.5.1 "optimize across boundaries" versus
/// §4.5.2 "local refinement". Which one the §4-I9 fire list needs is a
/// MEASUREMENT, not a preference, and this is the instrument that takes it.
///
/// For each failing traveller, walk the intersection curve outward in each
/// direction to the nearest successfully-optimized curve vertex, then report
/// whether the two bounds share a surface. Census-only; no behaviour.
///
/// A curve vertex is a "failure" exactly when it is in the §4-I9 fire list:
/// converged as an equation, not within its domain. "Successfully optimized" is
/// therefore `converged(w) && !failed(w)`.
#[allow(clippy::too_many_arguments)]
fn strategy_selection_census(
    mesh: &Mesh,
    attribution: &[Option<TriangleAttribution>],
    a: &BRep,
    b: &BRep,
    adj: &std::collections::BTreeMap<u32, std::collections::BTreeSet<u32>>,
    patches: &std::collections::BTreeMap<u32, std::collections::BTreeSet<(InputId, u32)>>,
    sites: &[(u32, u32)],
    entry: &[[f64; 3]],
    on_curve: &dyn Fn(u32) -> bool,
    converged: &dyn Fn(u32) -> bool,
) {
    let failed: std::collections::BTreeSet<u32> = sites.iter().map(|&(v, _)| v).collect();
    // How many DISTINCT surfaces of each operand does `pos` lie on, over the
    // faces carried by `vid`'s incident triangles? The Fig-13 discriminator
    // reads this: a point on ONE surface per operand is INTERIOR to that
    // surface; a point on TWO adjacent surfaces of the same operand lies on
    // their shared boundary curve.
    let per_input =
        |vid: u32, pos: [f64; 3]| -> (usize, usize) { carrier_counts(patches, a, b, vid, pos) };
    let empty_adj: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
    let curve_nbrs = |w: u32| -> Vec<u32> {
        adj.get(&w)
            .unwrap_or(&empty_adj)
            .iter()
            .copied()
            .filter(|&x| on_curve(x))
            .collect()
    };
    let good = |w: u32| -> bool { converged(w) && !failed.contains(&w) };

    for &(v, q) in sites {
        let branches = curve_nbrs(v);
        eprintln!(
            "YANG_S45_SELECT v{v} (crossed v{q}) curve_degree={} on_curve={}",
            branches.len(),
            on_curve(v)
        );

        // ---- the Fig-13 EXCLUSION, which the paper states before its selector
        //
        // "We note that the first strategy only applies to the INTERIOR points
        // but not to the BOUNDARY POINTS THAT GLIDE ALONG THE BOUNDARY CURVES.
        // … s is a corner point where more than two surfaces meet … the points
        // may glide toward s … after reaching s, it is difficult to predict in
        // which direction each vertex goes … this may lead to topology errors"
        // (`refs/text/yang2025_hybrid_boolean.txt:637-651`).
        //
        // Measured, not assumed: a traveller on TWO surfaces of one operand at
        // BOTH ends of its step is riding that operand's boundary curve, and a
        // crossed vertex on THREE is the corner `s`.
        let (pre_a, pre_b) = per_input(v, entry[v as usize]);
        let (post_a, post_b) = per_input(v, mesh.verts[v as usize].as_array());
        let (q_a, q_b) = per_input(q, mesh.verts[q as usize].as_array());
        let near_pre = pre_a.max(pre_b);
        let near_post = post_a.max(post_b);
        let glides = near_pre >= 2 && near_post >= 2;
        let corner_s = q_a.max(q_b) >= 3;
        eprintln!(
            "YANG_S45_SELECT   v{v} carrier: pre=(A{pre_a},B{pre_b}) post=(A{post_a},B{post_b}) \
             q=(A{q_a},B{q_b}) glides_on_boundary={glides} q_is_corner_s={corner_s}"
        );
        if glides && corner_s {
            eprintln!(
                "YANG_S45_SELECT   v{v} VERDICT=SECOND_STRATEGY (§4.5.2) — EXCLUDED by the \
                 Fig-13 clause: a BOUNDARY point gliding along a boundary curve, past a \
                 corner where more than two surfaces meet. §4.5.1 does not apply."
            );
            continue;
        }
        let _ = selector_clause2_walk(mesh, attribution, a, b, adj, v, on_curve, &good);
    }
}

fn relocation_domain_postcondition(
    mesh: &Mesh,
    attribution: &TriangleAttributionMap,
    a: &BRep,
    b: &BRep,
    entry: &[[f64; 3]],
) -> Result<(), YangError> {
    let mode = std::env::var("YANG_S4_CARRIER_DOMAIN").unwrap_or_default();
    if mode == "0" || mode == "off" {
        return Ok(());
    }
    let census = mode == "census";

    // Census-only: the FACE-level picture the §4-I10 measurements need, built
    // ONCE (one pass over the triangles) rather than per query.
    //
    // Two uses. (1) inc-4b's trim eligibility is `patches(v) subset-of
    // patches(q)` — a collapse v -> q reroutes every patch incident to v onto q.
    // (2) the paper's §4.5 strategy selector needs to walk the intersection
    // CURVE, and a vertex is on that curve exactly when its incident attributed
    // patches span BOTH inputs.
    let mut patch_map: std::collections::BTreeMap<u32, std::collections::BTreeSet<(InputId, u32)>> =
        std::collections::BTreeMap::new();
    if census {
        for (ti, tri) in mesh.tris.iter().enumerate() {
            if let Some(Some(att)) = attribution.attributions.get(ti) {
                for &tv in tri {
                    patch_map
                        .entry(tv)
                        .or_default()
                        .insert((att.input, att.face));
                }
            }
        }
    }
    let empty_patches: std::collections::BTreeSet<(InputId, u32)> =
        std::collections::BTreeSet::new();
    let patches_of = |target: u32| -> &std::collections::BTreeSet<(InputId, u32)> {
        patch_map.get(&target).unwrap_or(&empty_patches)
    };
    // A vertex of the intersection curve: its patches span both inputs.
    let on_curve = |target: u32| -> bool { vertex_on_curve(&patch_map, target) };
    // "Successfully optimized" in §4.5's sense, split into its two halves. A
    // curve vertex CONVERGED when its current position actually lies on a
    // surface of each operand (distance 0 to both, at the shared certificate
    // band) — that is "converged to a distance of 0". Whether it did so WITHIN
    // ITS DOMAIN is the separate question this postcondition answers, so the
    // caller subtracts the fire list.
    let converged = |target: u32| -> bool { vertex_converged(mesh, &patch_map, a, b, target) };

    // Still-ness is judged against the ENTRY snapshot. Vertices are only
    // appended during Stage 4, so an index below the snapshot's length names the
    // same vertex throughout; appended vertices have no pre position and cannot
    // have travelled.
    let n = entry.len().min(mesh.verts.len());
    let moved_at = |v: u32| -> Option<([f64; 3], [f64; 3])> {
        let i = v as usize;
        if i >= n {
            return None;
        }
        let post = mesh.verts[i].as_array();
        (entry[i] != post).then_some((entry[i], post))
    };

    let adj = build_live_adjacency(mesh);

    let mut first: Option<u32> = None;
    let mut fires = 0usize;
    // Census-only: the (traveller, crossed corner) pairs, for the §4.5
    // strategy-selection walk after the loop (it needs the WHOLE fire list to
    // know which curve vertices are the failures).
    let mut census_sites: Vec<(u32, u32)> = Vec::new();
    // Census-only: every REAL corner-transit junction read (site, corner,
    // operand, model edge, solution) — grouped after the loop to surface
    // shared-mint pairs (two views, one junction; R0044 v142/v144).
    let mut mint_reads: Vec<(u32, u32, InputId, u32, [f64; 3])> = Vec::new();
    // Census-only (inc-2c-2, spec §3h): per-site walks under APPLY semantics
    // (contract-band splice terminal), assembled into CORRIDOR repair units
    // after the loop.
    let mut transit_walks: Vec<crate::stage4_transit::SiteWalk> = Vec::new();
    for (&v, neighbours) in &adj {
        let Some((pre, post)) = moved_at(v) else {
            continue;
        };
        for &q in neighbours {
            if moved_at(q).is_some() {
                continue; // both travelled — not a crossing of a STILL carrier vertex
            }
            let (qi, qpos) = (q as usize, mesh.verts[q as usize].as_array());
            if qi >= n {
                continue; // minted during Stage 4; it bounds no pre-existing carrier
            }
            if !crate::stage4_construct::point_on_segment_interior(pre, post, qpos) {
                continue;
            }
            // SECOND LEG — is `q` a DOMAIN ENDPOINT, or just a sample?
            //
            // Crossing a still collinear neighbour is not by itself a domain
            // violation. If `q` lies only on surfaces the traveller also lies
            // on, it is a plain sample of the SAME carrier, and Yang's own
            // remedy applies ("we remove a mesh vertex if it is too close to
            // the intersection curve", §4.4.1) — the Fig-11 merge and the
            // near-curve removal own that, and a STOP here would preempt them.
            //
            // A domain ENDPOINT is different: it carries a surface the
            // traveller is OFF, i.e. a third face joins there and the carrier
            // STOPS. This is §4-I8's containment rule read in the other
            // direction, and it is what makes the relocated position
            // unreachable by any mesh update.
            let lost: Vec<Surface> =
                carried_surfaces(mesh, &attribution.attributions, a, b, q, qpos)
                    .into_iter()
                    .filter(|&surf| {
                        !surface_distance_and_normal(surf, post)
                            .is_some_and(|(f, _)| f.abs() <= junction_certificate_band(post, surf))
                    })
                    .collect();
            let travel = ((post[0] - pre[0]).powi(2)
                + (post[1] - pre[1]).powi(2)
                + (post[2] - pre[2]).powi(2))
            .sqrt();
            if lost.is_empty() {
                if census {
                    eprintln!(
                        "YANG_S4_CARRIER_DOMAIN-SAMPLE v{v} crossed still v{q} \
                         travel={travel:.4e} — q is on the traveller's own carrier \
                         (§4.4.1 near-curve removal owns it, no STOP)"
                    );
                }
                continue;
            }
            fires += 1;
            first.get_or_insert(v);
            if census {
                census_sites.push((v, q));
                let (pv, pq) = (patches_of(v), patches_of(q));
                let subset = pv.is_subset(pq);
                let dqv = {
                    let d = [qpos[0] - post[0], qpos[1] - post[1], qpos[2] - post[2]];
                    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
                };
                eprintln!(
                    "YANG_S4_CARRIER_DOMAIN v{v} crossed still v{q} travel={travel:.4e} \
                     overrun={dqv:.4e} lost={} subset={subset} \
                     pre=({:.9},{:.9},{:.9}) post=({:.9},{:.9},{:.9}) first_lost={:?}",
                    lost.len(),
                    pre[0],
                    pre[1],
                    pre[2],
                    post[0],
                    post[1],
                    post[2],
                    lost[0],
                );
                eprintln!(
                    "YANG_S4_CARRIER_DOMAIN-PATCH   v{v} patches={pv:?}\n\
                     YANG_S4_CARRIER_DOMAIN-PATCH   v{q} patches={pq:?}"
                );
                // Is the relocation a SMALL correction on a coarse mesh, or a
                // jump to a different root? Report, per incident face of the
                // traveller, |distance| at pre and at post; plus the local mesh
                // edge scale at `v` (entry positions). A legitimate refinement
                // has travel of the order of the pre-distances.
                let mut seen_face: std::collections::BTreeSet<(InputId, u32)> =
                    std::collections::BTreeSet::new();
                for (ti, tri) in mesh.tris.iter().enumerate() {
                    if !tri.contains(&v) {
                        continue;
                    }
                    let Some(Some(att)) = attribution.attributions.get(ti) else {
                        continue;
                    };
                    if !seen_face.insert((att.input, att.face)) {
                        continue;
                    }
                    let faces = match att.input {
                        InputId::A => a.faces(),
                        InputId::B => b.faces(),
                    };
                    let Some(face) = faces.get(att.face as usize) else {
                        continue;
                    };
                    let surf = face.surface;
                    let dpre = surface_distance_and_normal(surf, pre).map(|(f, _)| f.abs());
                    let dpost = surface_distance_and_normal(surf, post).map(|(f, _)| f.abs());
                    let dq = surface_distance_and_normal(surf, qpos).map(|(f, _)| f.abs());
                    let band = junction_certificate_band(post, surf);
                    eprintln!(
                        "YANG_S4_CARRIER_DOMAIN-SURF    v{v} face={:?}:{} \
                         d_pre={dpre:?} d_post={dpost:?} d_q={dq:?} band={band:.3e}",
                        att.input, att.face
                    );
                }
                let (mut emin, mut esum, mut ecount) = (f64::INFINITY, 0.0f64, 0usize);
                for &w in neighbours {
                    let wi = w as usize;
                    if wi >= n {
                        continue;
                    }
                    let (e0, e1) = (entry[v as usize], entry[wi]);
                    let d = ((e0[0] - e1[0]).powi(2)
                        + (e0[1] - e1[1]).powi(2)
                        + (e0[2] - e1[2]).powi(2))
                    .sqrt();
                    emin = emin.min(d);
                    esum += d;
                    ecount += 1;
                }
                eprintln!(
                    "YANG_S4_CARRIER_DOMAIN-SCALE   v{v} travel={travel:.4e} \
                     edge_min={emin:.4e} edge_mean={:.4e} deg={ecount}",
                    esum / (ecount.max(1) as f64)
                );
                // §4-I10 (d2): exercise the §4.5.1 DOMAIN truncation primitive
                // on the real site, so it is measured on live data before it is
                // ever load-bearing. Report-only — the answer is printed, not
                // applied; §4.5.1's continuation (re-parameterize on the
                // neighbouring surface, solve q1/q2 on C_b) does not exist yet,
                // so the §4-I9 STOP stays the answer.
                eprintln!(
                    "YANG_S45_TRUNCATE v{v} -> {:?}",
                    crate::stage4_truncate::max_in_domain_step(pre, post, &[(q, qpos)])
                );
                // Corner-transit epic (spec `specs/yang_451_corner_transit.md`):
                // inc-0 solved both candidate corrected triples (feasibility,
                // 46/46 converge); inc-1 read each solution against the
                // candidate faces' own edge domains and VALIDATED the
                // corner-incident-edge rule; inc-2a extracts that instrument
                // into the pure planner (`stage4_transit`) so census and the
                // eventual apply arm share ONE reading, and adds the site
                // ANATOMY the apply design needs (curve-neighbour chains,
                // per-attribution fans, the v–q wedge). Report-only.
                {
                    let (pv, pq) = (patches_of(v), patches_of(q));
                    let far: Vec<_> = pv.difference(pq).collect();
                    let next: Vec<_> = pq.difference(pv).collect();
                    let shared: Vec<_> = pv.intersection(pq).collect();
                    eprintln!(
                        "YANG_S4_CARRIER_DOMAIN-TRANSIT v{v} q=v{q} far={far:?} \
                         next={next:?} shared={shared:?}"
                    );
                    match crate::stage4_transit::read_site(a, b, pv, pq, qpos) {
                        Err(d) => eprintln!(
                            "YANG_S4_CARRIER_DOMAIN-TRANSIT   PLAN v{v} q=v{q} DECLINE {d:?}"
                        ),
                        Ok(site) => {
                            let dist = |x: [f64; 3], y: [f64; 3]| {
                                ((x[0] - y[0]).powi(2)
                                    + (x[1] - y[1]).powi(2)
                                    + (x[2] - y[2]).powi(2))
                                .sqrt()
                            };
                            for c in &site.cands {
                                let label =
                                    format!("[{:?},{:?},{:?}]", site.far, c.shared, site.next);
                                let Some(sa) = c.sol else {
                                    eprintln!(
                                        "YANG_S4_CARRIER_DOMAIN-TRANSIT   triple{label} -> \
                                         NO-CONVERGE"
                                    );
                                    continue;
                                };
                                // inc-0 continuity: the planar-hull reading
                                // (planes only; None = no verdict) at the
                                // Stage-1 chord band.
                                let sn = {
                                    let faces = match site.next.0 {
                                        InputId::A => a.faces(),
                                        InputId::B => b.faces(),
                                    };
                                    faces.get(site.next.1 as usize).map(|f| f.surface)
                                };
                                let hull = sn.and_then(|sn| {
                                    stage4_chord_band(a, b).and_then(|de| {
                                        planar_partner_hull_contains(a, b, sn, sa, de)
                                    })
                                });
                                eprintln!(
                                    "YANG_S4_CARRIER_DOMAIN-TRANSIT   triple{label} -> \
                                     CONVERGED d_from_q={:.4e} d_from_post={:.4e} \
                                     overrun={:.4e} next_hull={hull:?} sol=({:.9},{:.9},{:.9})",
                                    dist(sa, qpos),
                                    dist(sa, post),
                                    dist(post, qpos),
                                    sa[0],
                                    sa[1],
                                    sa[2],
                                );
                                eprintln!(
                                    "YANG_S4_CARRIER_DOMAIN-TRANSIT     edge{label} -> {} \
                                     real={}{}{}",
                                    crate::stage4_transit::format_edge_read(&c.edge),
                                    c.real,
                                    if c.why.is_empty() { "" } else { " why=" },
                                    c.why,
                                );
                            }
                            match crate::stage4_transit::classify(&site, qpos) {
                                Ok(class) => {
                                    eprintln!(
                                        "YANG_S4_CARRIER_DOMAIN-TRANSIT   PLAN v{v} q=v{q} \
                                         {class:?}"
                                    );
                                    // Collect every REAL junction read for the
                                    // post-loop shared-mint grouping (the I13f
                                    // two-views-one-mint anatomy; R0044
                                    // v142/v144).
                                    for c in &site.cands {
                                        if let (true, Some(sa), Some(er)) =
                                            (c.real, c.sol, c.edge.as_ref())
                                        {
                                            mint_reads.push((v, q, c.shared.0, er.edge, sa));
                                        }
                                    }
                                }
                                Err(d) => eprintln!(
                                    "YANG_S4_CARRIER_DOMAIN-TRANSIT   PLAN v{v} q=v{q} \
                                     DECLINE {d:?}"
                                ),
                            }
                            // inc-2b (spec §3e): the CORRIDOR WALK — from
                            // each real junction, walk the far∩facet curve
                            // across `next`'s lattice toward the OTHER
                            // chain's face, and annotate every discovered
                            // junction with the nearest existing mesh vertex
                            // carrying its surface triple (the re-anchor
                            // candidates). Report-only.
                            'walks: {
                                let brep_n = match site.next.0 {
                                    InputId::A => a,
                                    InputId::B => b,
                                };
                                let far_surf = {
                                    let faces = match site.far.0 {
                                        InputId::A => a.faces(),
                                        InputId::B => b.faces(),
                                    };
                                    match faces.get(site.far.1 as usize) {
                                        Some(f) => f.surface,
                                        None => break 'walks,
                                    }
                                };
                                let walk_adj = crate::stage4_transit::build_edge_adjacency(brep_n);
                                let d3w = |x: [f64; 3], y: [f64; 3]| {
                                    ((x[0] - y[0]).powi(2)
                                        + (x[1] - y[1]).powi(2)
                                        + (x[2] - y[2]).powi(2))
                                    .sqrt()
                                };
                                // Existing-junction lookup (the splice
                                // terminal's witness AND the per-junction
                                // annotation): nearest mesh vertex whose
                                // attributed patches contain the junction's
                                // full triple {far, from, to}.
                                let far_patch = site.far;
                                let next_op = site.next.0;
                                let existing =
                                    |ff: u32, ft: u32, pos: [f64; 3]| -> Option<(u32, f64)> {
                                        let want = [far_patch, (next_op, ff), (next_op, ft)];
                                        patch_map
                                            .iter()
                                            .filter(|(_, ps)| want.iter().all(|t| ps.contains(t)))
                                            .map(|(&w, _)| {
                                                (w, d3w(mesh.verts[w as usize].as_array(), pos))
                                            })
                                            .min_by(|x, y| x.1.total_cmp(&y.1))
                                    };
                                let walk_ctx = crate::stage4_transit::WalkCtx {
                                    brep: brep_n,
                                    far: far_surf,
                                    adj: &walk_adj,
                                    existing: &existing,
                                    splice_band: crate::stage4_transit::WalkBand::Eval,
                                };
                                // inc-2c-2 (spec §3h): the same walk under
                                // APPLY semantics — the splice terminal at
                                // the junction-CONTRACT band, so the
                                // corridor ends at the FIRST owned junction.
                                let walk_ctx_apply = crate::stage4_transit::WalkCtx {
                                    brep: brep_n,
                                    far: far_surf,
                                    adj: &walk_adj,
                                    existing: &existing,
                                    splice_band: crate::stage4_transit::WalkBand::Contract,
                                };
                                for c in &site.cands {
                                    if !c.real {
                                        continue;
                                    }
                                    let (Some(sa), Some(er)) = (c.sol, c.edge.as_ref()) else {
                                        continue;
                                    };
                                    let Some(other) =
                                        site.cands.iter().find(|o| o.shared != c.shared)
                                    else {
                                        continue;
                                    };
                                    if c.shared.0 != site.next.0 || other.shared.0 != site.next.0 {
                                        eprintln!(
                                            "YANG_S4_CARRIER_DOMAIN-WALK    v{v} SKIP \
                                             cross-operand shape"
                                        );
                                        continue;
                                    }
                                    let (juncs, end) = crate::stage4_transit::walk_corridor(
                                        &walk_ctx,
                                        crate::stage4_transit::WalkStart {
                                            face: site.next.1,
                                            entry_key: crate::stage4_transit::edge_key(
                                                brep_n, er.edge,
                                            ),
                                            entry: sa,
                                        },
                                        other.shared.1,
                                        64,
                                    );
                                    eprintln!(
                                        "YANG_S4_CARRIER_DOMAIN-WALK    v{v} from \
                                         real{:?} into {:?} toward {:?}: steps={} end={end:?}",
                                        c.shared,
                                        site.next,
                                        other.shared,
                                        juncs.len(),
                                    );
                                    // inc-2c-0: a stuck walk gets the
                                    // ALL-ROOTS probe on its dead-end face —
                                    // every loop edge's certified far∩edge
                                    // roots (the v76 dip-hypothesis data).
                                    if matches!(
                                        end,
                                        crate::stage4_transit::WalkEnd::NoExit
                                            | crate::stage4_transit::WalkEnd::AmbiguousExit(_)
                                    ) {
                                        let (stuck_face, probe_entry) = match juncs.last() {
                                            Some(j) => (j.face_to, j.sol),
                                            None => (site.next.1, sa),
                                        };
                                        for line in crate::stage4_transit::face_edge_roots_probe(
                                            brep_n,
                                            far_surf,
                                            stuck_face,
                                            probe_entry,
                                        ) {
                                            eprintln!(
                                                "YANG_S4_CARRIER_DOMAIN-WALKROOTS v{v} \
                                                 face={:?} {line}",
                                                (site.next.0, stuck_face),
                                            );
                                        }
                                    }
                                    for (k, wj) in juncs.iter().enumerate() {
                                        // Nearest existing mesh vertex carrying
                                        // this junction's triple {far, from, to}
                                        // — the re-anchor candidate.
                                        let near = existing(wj.face_from, wj.face_to, wj.sol);
                                        let near_s = match near {
                                            Some((w, dw)) => {
                                                let moved = (w as usize) < n
                                                    && entry[w as usize]
                                                        != mesh.verts[w as usize].as_array();
                                                format!("near_mesh=v{w} d={dw:.3e} moved={moved}")
                                            }
                                            None => "near_mesh=NONE".into(),
                                        };
                                        eprintln!(
                                            "YANG_S4_CARRIER_DOMAIN-WALK      step{k} \
                                             {:?}->{:?} edge={} d_on_edge={:.3e} \
                                             sol=({:.9},{:.9},{:.9}) {near_s}",
                                            (site.next.0, wj.face_from),
                                            (site.next.0, wj.face_to),
                                            wj.edge,
                                            wj.d_on_edge,
                                            wj.sol[0],
                                            wj.sol[1],
                                            wj.sol[2],
                                        );
                                    }
                                    // inc-2c-2: record the APPLY-semantics
                                    // walk for the post-loop corridor
                                    // assembly (contract-band terminal; the
                                    // census walk above stays byte-stable).
                                    let (ajuncs, aend) = crate::stage4_transit::walk_corridor(
                                        &walk_ctx_apply,
                                        crate::stage4_transit::WalkStart {
                                            face: site.next.1,
                                            entry_key: crate::stage4_transit::edge_key(
                                                brep_n, er.edge,
                                            ),
                                            entry: sa,
                                        },
                                        other.shared.1,
                                        64,
                                    );
                                    transit_walks.push(crate::stage4_transit::SiteWalk {
                                        site: v,
                                        corner: q,
                                        far: site.far,
                                        far_surf,
                                        walk_op: site.next.0,
                                        entry: crate::stage4_transit::WalkJunction {
                                            face_from: c.shared.1,
                                            face_to: site.next.1,
                                            edge: er.edge,
                                            sol: sa,
                                            d_on_edge: er.d_on_edge,
                                        },
                                        juncs: ajuncs,
                                        end: aend,
                                    });
                                }
                            }
                        }
                    }
                    // Site ANATOMY for the inc-2 apply design: which mesh
                    // curve chains arrive at the traveller, what each incident
                    // fan is attributed to, and which triangles ride the v–q
                    // wedge. Report-only.
                    let d3 = |x: [f64; 3], y: [f64; 3]| {
                        ((x[0] - y[0]).powi(2) + (x[1] - y[1]).powi(2) + (x[2] - y[2]).powi(2))
                            .sqrt()
                    };
                    if let Some(nbrs) = adj.get(&v) {
                        for &w in nbrs {
                            let wpos = mesh.verts[w as usize].as_array();
                            let moved = (w as usize) < n && entry[w as usize] != wpos;
                            eprintln!(
                                "YANG_S4_CARRIER_DOMAIN-ANAT    v{v} nbr v{w} d={:.4e} \
                                 moved={moved} on_curve={} patches={:?}",
                                d3(post, wpos),
                                on_curve(w),
                                patches_of(w),
                            );
                        }
                    }
                    for target in [v, q] {
                        let mut fan: std::collections::BTreeMap<Option<(InputId, u32)>, usize> =
                            std::collections::BTreeMap::new();
                        for (ti, tri) in mesh.tris.iter().enumerate() {
                            if tri.contains(&target) {
                                let key = match attribution.attributions.get(ti) {
                                    Some(Some(att)) => Some((att.input, att.face)),
                                    _ => None,
                                };
                                *fan.entry(key).or_default() += 1;
                            }
                        }
                        eprintln!("YANG_S4_CARRIER_DOMAIN-ANAT    v{v} fan v{target}: {fan:?}");
                    }
                    let wedge: Vec<(usize, Option<(InputId, u32)>)> = mesh
                        .tris
                        .iter()
                        .enumerate()
                        .filter(|(_, tri)| tri.contains(&v) && tri.contains(&q))
                        .map(|(ti, _)| {
                            (
                                ti,
                                match attribution.attributions.get(ti) {
                                    Some(Some(att)) => Some((att.input, att.face)),
                                    _ => None,
                                },
                            )
                        })
                        .collect();
                    eprintln!("YANG_S4_CARRIER_DOMAIN-ANAT    v{v} vq-wedge tris: {wedge:?}");
                }
            } else {
                return Err(YangError::stage4_region_invalid(
                    v,
                    Stage4InvalidReason::RelocationCrossedCarrierVertex,
                ));
            }
        }
    }
    if census && !mint_reads.is_empty() {
        // Shared-mint grouping: REAL junctions from different sites landing on
        // the SAME model edge at the SAME position are two views of ONE mint —
        // the apply arm must mint once and share by identity (the junction
        // contract), never once per view.
        let d3 = |x: [f64; 3], y: [f64; 3]| {
            ((x[0] - y[0]).powi(2) + (x[1] - y[1]).powi(2) + (x[2] - y[2]).powi(2)).sqrt()
        };
        // Identity is POSITION on one operand, never the edge index: the m1
        // convention emits one directed edge copy per half-edge, so two views
        // of one physical junction can name DIFFERENT edge indices for the
        // same physical edge (R0085 v467/v6071: edges 351/2520, t and 1−t).
        let mut grouped = vec![false; mint_reads.len()];
        for i in 0..mint_reads.len() {
            if grouped[i] {
                continue;
            }
            let (vi, _, op, edge, sol) = mint_reads[i];
            let scale = sol[0].abs().max(sol[1].abs()).max(sol[2].abs());
            let band = 1e-9 * (1.0 + scale);
            let mut members = vec![vi];
            let mut edges = std::collections::BTreeSet::from([edge]);
            let mut spread = 0.0f64;
            for j in (i + 1)..mint_reads.len() {
                let (vj, _, opj, edgej, solj) = mint_reads[j];
                if !grouped[j] && opj == op && d3(sol, solj) <= band {
                    grouped[j] = true;
                    members.push(vj);
                    edges.insert(edgej);
                    spread = spread.max(d3(sol, solj));
                }
            }
            if members.len() > 1 {
                eprintln!(
                    "YANG_S4_CARRIER_DOMAIN-MINTGROUP op={op:?} edge-copies={edges:?} \
                     sites={members:?} spread={spread:.3e} — ONE mint, {} views",
                    members.len()
                );
            }
        }
    }
    if census && !transit_walks.is_empty() {
        // inc-2c-2 (spec §3h): assemble the APPLY-semantics walks into
        // canonical CORRIDOR repair units — merged by position identity,
        // junctions dispositioned (mint vs contract-band splice), runs
        // sourced (existing healthy chain vs fresh chord-density samples).
        // Report-only; the mutation (inc-2c-3) consumes these units.
        use crate::stage4_transit::{JunctionDisposition, RunSource};
        let d3c = |x: [f64; 3], y: [f64; 3]| {
            ((x[0] - y[0]).powi(2) + (x[1] - y[1]).powi(2) + (x[2] - y[2]).powi(2)).sqrt()
        };
        // Splices must target HEALTHY vertices: the fired travellers are the
        // objects being repaired, never splice or chain anchors.
        let fired: std::collections::BTreeSet<u32> = census_sites.iter().map(|&(v, _)| v).collect();
        let existing_g = |far: (InputId, u32),
                          op: InputId,
                          ff: u32,
                          ft: u32,
                          pos: [f64; 3]|
         -> Option<(u32, f64)> {
            let want = [far, (op, ff), (op, ft)];
            patch_map
                .iter()
                .filter(|(w, ps)| !fired.contains(w) && want.iter().all(|t| ps.contains(t)))
                .map(|(&w, _)| (w, d3c(mesh.verts[w as usize].as_array(), pos)))
                .min_by(|x, y| x.1.total_cmp(&y.1))
        };
        // Existing far∩facet carrier components (v80-class), each ordered as
        // a path when the induced live adjacency is one.
        let facet_chain =
            |far: (InputId, u32), op: InputId, f: u32| -> Vec<crate::stage4_transit::FacetChain> {
                let want = [far, (op, f)];
                let members: std::collections::BTreeSet<u32> = patch_map
                    .iter()
                    .filter(|(w, ps)| !fired.contains(w) && want.iter().all(|t| ps.contains(t)))
                    .map(|(&w, _)| w)
                    .collect();
                let ind_nbrs = |x: u32| -> Vec<u32> {
                    adj.get(&x)
                        .into_iter()
                        .flatten()
                        .copied()
                        .filter(|y| members.contains(y))
                        .collect()
                };
                let mut seen: std::collections::BTreeSet<u32> = Default::default();
                let mut comps = Vec::new();
                for &s in &members {
                    if seen.contains(&s) {
                        continue;
                    }
                    let mut comp = std::collections::BTreeSet::from([s]);
                    let mut stack = vec![s];
                    seen.insert(s);
                    while let Some(x) = stack.pop() {
                        for y in ind_nbrs(x) {
                            if comp.insert(y) {
                                seen.insert(y);
                                stack.push(y);
                            }
                        }
                    }
                    let degs: Vec<(u32, usize)> = comp
                        .iter()
                        .map(|&x| (x, ind_nbrs(x).iter().filter(|y| comp.contains(y)).count()))
                        .collect();
                    let ends: Vec<u32> = degs
                        .iter()
                        .filter(|&&(_, d)| d <= 1)
                        .map(|&(x, _)| x)
                        .collect();
                    let is_path =
                        degs.iter().all(|&(_, d)| d <= 2) && (comp.len() == 1 || ends.len() == 2);
                    let verts: Vec<(u32, [f64; 3])> = if is_path && comp.len() > 1 {
                        // Walk from the smaller-id end — deterministic.
                        let mut order = vec![*ends.iter().min().expect("two ends")];
                        let mut prev: Option<u32> = None;
                        while order.len() < comp.len() {
                            let x = *order.last().expect("non-empty");
                            let nxt = ind_nbrs(x)
                                .into_iter()
                                .find(|&y| comp.contains(&y) && Some(y) != prev);
                            match nxt {
                                Some(y) => {
                                    prev = Some(x);
                                    order.push(y);
                                }
                                None => break,
                            }
                        }
                        order
                            .iter()
                            .map(|&x| (x, mesh.verts[x as usize].as_array()))
                            .collect()
                    } else {
                        comp.iter()
                            .map(|&x| (x, mesh.verts[x as usize].as_array()))
                            .collect()
                    };
                    comps.push(crate::stage4_transit::FacetChain {
                        path: is_path && verts.len() == comp.len(),
                        verts,
                    });
                }
                comps
            };
        let face_surface = |op: InputId, f: u32| -> Option<Surface> {
            let faces = match op {
                InputId::A => a.faces(),
                InputId::B => b.faces(),
            };
            faces.get(f as usize).map(|x| x.surface)
        };
        match stage4_chord_band(a, b) {
            None => eprintln!(
                "YANG_S4_CARRIER_DOMAIN-CORRIDOR no chord band (planar-only inputs) — \
                 assembly skipped"
            ),
            Some(d_eps) => {
                let actx = crate::stage4_transit::AssembleCtx {
                    existing: &existing_g,
                    facet_chain: &facet_chain,
                    face_surface: &face_surface,
                    d_eps,
                };
                let (corridors, declines) =
                    crate::stage4_transit::assemble_corridors(&actx, &transit_walks);
                for (k, c) in corridors.iter().enumerate() {
                    let mints = c
                        .junctions
                        .iter()
                        .filter(|j| j.disposition == JunctionDisposition::Mint)
                        .count();
                    eprintln!(
                        "YANG_S4_CARRIER_DOMAIN-CORRIDOR #{k} op={:?} far={:?} \
                         phantoms={:?} corners={:?} juncs={} mints={mints} splices={} \
                         applyable={}",
                        c.walk_op,
                        c.far,
                        c.phantoms,
                        c.corners,
                        c.junctions.len(),
                        c.junctions.len() - mints,
                        c.applyable(),
                    );
                    for (i, j) in c.junctions.iter().enumerate() {
                        eprintln!(
                            "YANG_S4_CARRIER_DOMAIN-CORRIDOR   #{k} junc{i} \
                             faces={:?} edge={} {:?} sol=({:.9},{:.9},{:.9})",
                            j.faces, j.edge, j.disposition, j.sol[0], j.sol[1], j.sol[2],
                        );
                    }
                    for (i, (f, src)) in c.runs.iter().enumerate() {
                        let (p0, p1) = (c.junctions[i].sol, c.junctions[i + 1].sol);
                        match src {
                            Ok(RunSource::Samples(s)) => eprintln!(
                                "YANG_S4_CARRIER_DOMAIN-CORRIDOR   #{k} run{i} facet={f} \
                                 SAMPLES n={}",
                                s.len()
                            ),
                            Ok(RunSource::Spliced { head, chain, tail }) => eprintln!(
                                "YANG_S4_CARRIER_DOMAIN-CORRIDOR   #{k} run{i} facet={f} \
                                 SPLICED head_n={} chain={chain:?} tail_n={}",
                                head.len(),
                                tail.len(),
                            ),
                            Err(issue) => {
                                eprintln!(
                                    "YANG_S4_CARRIER_DOMAIN-CORRIDOR   #{k} run{i} \
                                     facet={f} ISSUE {issue:?}"
                                );
                                // The measured anatomy the mutation's
                                // re-anchoring rule needs: each carrier
                                // component's ends against the bounding
                                // junctions, and their phantom adjacency.
                                for (ci, comp) in
                                    facet_chain(c.far, c.walk_op, *f).iter().enumerate()
                                {
                                    let ids: Vec<u32> =
                                        comp.verts.iter().map(|&(x, _)| x).collect();
                                    eprintln!(
                                        "YANG_S4_CARRIER_DOMAIN-CORRIDOR     #{k} run{i} \
                                         comp{ci} path={} verts={ids:?}",
                                        comp.path
                                    );
                                    if comp.path && !comp.verts.is_empty() {
                                        for &(x, xp) in
                                            [comp.verts[0], comp.verts[comp.verts.len() - 1]].iter()
                                        {
                                            let phn: Vec<u32> = adj
                                                .get(&x)
                                                .into_iter()
                                                .flatten()
                                                .copied()
                                                .filter(|y| fired.contains(y))
                                                .collect();
                                            eprintln!(
                                                "YANG_S4_CARRIER_DOMAIN-CORRIDOR     #{k} \
                                                 run{i} comp{ci} end v{x} d_j0={:.3e} \
                                                 d_j1={:.3e} phantom_nbrs={phn:?}",
                                                d3c(xp, p0),
                                                d3c(xp, p1),
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                for d in &declines {
                    use crate::stage4_transit::AssemblyDecline as AD;
                    let s = match d {
                        AD::WalkFailed { site, end } => {
                            format!("site=v{site} WALK-FAILED end={end:?}")
                        }
                        AD::FaceChainBroken { site, at } => {
                            format!("site=v{site} FACE-CHAIN-BROKEN at={at}")
                        }
                        AD::CorridorConflict { sites } => {
                            format!("sites={sites:?} CORRIDOR-CONFLICT")
                        }
                    };
                    eprintln!("YANG_S4_CARRIER_DOMAIN-CORRIDOR DECLINE {s}");
                }
                // Shared endpoint mints across corridors (the v142/v144
                // ONE-mint contract): the apply must mint such a junction
                // once and share it between both corridors.
                for i in 0..corridors.len() {
                    for j in (i + 1)..corridors.len() {
                        let (ci, cj) = (&corridors[i], &corridors[j]);
                        if ci.walk_op != cj.walk_op || ci.far != cj.far {
                            continue;
                        }
                        let ends = |c: &crate::stage4_transit::CorridorRepair| {
                            [0, c.junctions.len() - 1]
                                .map(|k| c.junctions[k].sol)
                                .to_vec()
                        };
                        for ea in ends(ci) {
                            for &eb in &ends(cj) {
                                let scale = ea.iter().fold(0.0f64, |m, &x| m.max(x.abs()));
                                if d3c(ea, eb) <= crate::stage4_transit::contract_band(scale) {
                                    eprintln!(
                                        "YANG_S4_CARRIER_DOMAIN-CORRIDOR SHARED-MINT \
                                         corridors=(#{i},#{j}) \
                                         sol=({:.9},{:.9},{:.9})",
                                        ea[0], ea[1], ea[2],
                                    );
                                }
                            }
                        }
                    }
                }
                let walked: std::collections::BTreeSet<u32> =
                    transit_walks.iter().map(|w| w.site).collect();
                let consumed: std::collections::BTreeSet<u32> = corridors
                    .iter()
                    .filter(|c| c.applyable())
                    .flat_map(|c| c.phantoms.iter().copied())
                    .collect();
                let unconsumed: Vec<u32> = walked.difference(&consumed).copied().collect();
                eprintln!(
                    "YANG_S4_CARRIER_DOMAIN-CORRIDOR TOTAL corridors={} applyable={} \
                     consumed={}/{} unconsumed={unconsumed:?}",
                    corridors.len(),
                    corridors.iter().filter(|c| c.applyable()).count(),
                    consumed.len(),
                    walked.len(),
                );
                // inc-2c-3a (spec §3h): the CYCLE-SURGERY planner census —
                // per applyable corridor, each affected patch's connected
                // component and boundary cycles with the surgery sites
                // marked (phantom position, junction HOST edges, chain
                // neighbours + their junction attachment by patch
                // membership). Report-only; the mutation (3b) consumes
                // measured cycles, never sketched ones.
                let adjacency_t = crate::stage5_topology::triangle_adjacency(mesh);
                let raw_patches =
                    crate::stage5_topology::flood_fill_patches(mesh, attribution, &adjacency_t);
                for (k, c) in corridors.iter().enumerate() {
                    if !c.applyable() {
                        continue;
                    }
                    // The attachment certificate, measured live: each
                    // phantom's on-curve mesh neighbours name their
                    // junction by patch membership (§3h) — unique or the
                    // mutation declines.
                    for &p in &c.phantoms {
                        for &w in adj.get(&p).into_iter().flatten() {
                            if !on_curve(w) {
                                continue;
                            }
                            let pw = patches_of(w);
                            let hits: Vec<usize> = c
                                .junctions
                                .iter()
                                .enumerate()
                                .filter(|(_, j)| {
                                    pw.contains(&(c.walk_op, j.faces.0))
                                        || pw.contains(&(c.walk_op, j.faces.1))
                                })
                                .map(|(i, _)| i)
                                .collect();
                            eprintln!(
                                "YANG_S4_CARRIER_DOMAIN-CYCLES  #{k} phantom v{p} nbr v{w} \
                                 patches={pw:?} attaches_to_juncs={hits:?}{}",
                                if hits.len() == 1 { "" } else { " NOT-UNIQUE" },
                            );
                        }
                    }
                    // Affected patch keys: far ∪ run facets ∪ the two
                    // terminal-outer patches.
                    let mut keys: Vec<(InputId, u32)> = vec![
                        c.far,
                        (c.walk_op, c.junctions[0].faces.0),
                        (c.walk_op, c.junctions[c.junctions.len() - 1].faces.1),
                    ];
                    keys.extend(c.runs.iter().map(|&(f, _)| (c.walk_op, f)));
                    keys.sort_unstable();
                    keys.dedup();
                    for key in keys {
                        for (pi, patch) in raw_patches
                            .iter()
                            .enumerate()
                            .filter(|(_, p)| (p.attribution.input, p.attribution.face) == key)
                        {
                            let holds_phantom = patch.tri_indices.iter().any(|&t| {
                                let tri = mesh.tris[t as usize];
                                c.phantoms.iter().any(|&p| tri.contains(&p))
                            });
                            let cycles =
                                match crate::stage5_topology::patch_boundary_cycle(patch, mesh) {
                                    Ok(cy) => cy,
                                    Err(e) => {
                                        eprintln!(
                                            "YANG_S4_CARRIER_DOMAIN-CYCLES  #{k} patch={key:?} \
                                         comp={pi} CYCLE-WALK-FAILED {e:?}"
                                        );
                                        continue;
                                    }
                                };
                            // Junction HOST edges: the boundary edge whose
                            // segment carries the junction solution.
                            let mut hosts: Vec<(usize, usize, usize)> = Vec::new();
                            for (ji, j) in c.junctions.iter().enumerate() {
                                let scale = j.sol.iter().fold(0.0f64, |m, &x| m.max(x.abs()));
                                let band = cad_primitives::TAU_WORK.max(8.0 * f64::EPSILON * scale);
                                for (ci, cy) in cycles.iter().enumerate() {
                                    for (ei, &(x, y)) in cy.iter().enumerate() {
                                        let d = crate::stage4_transit::dist_point_segment(
                                            j.sol,
                                            mesh.verts[x as usize].as_array(),
                                            mesh.verts[y as usize].as_array(),
                                        );
                                        if d <= band {
                                            hosts.push((ji, ci, ei));
                                        }
                                    }
                                }
                            }
                            if !holds_phantom && hosts.is_empty() {
                                continue; // unrelated component (e.g. the other crossing region)
                            }
                            let lens: Vec<usize> = cycles.iter().map(|cy| cy.len()).collect();
                            eprintln!(
                                "YANG_S4_CARRIER_DOMAIN-CYCLES  #{k} patch={key:?} comp={pi} \
                                 tris={} cycles={lens:?} phantom_in_comp={holds_phantom} \
                                 junc_hosts={hosts:?}",
                                patch.tri_indices.len(),
                            );
                            // Windows: ±4 boundary vertices around each mark,
                            // tagged P (phantom), Q (corner), N (phantom
                            // curve-neighbour).
                            let tag = |v: u32| -> String {
                                if c.phantoms.contains(&v) {
                                    format!("v{v}[P]")
                                } else if c.corners.contains(&v) {
                                    format!("v{v}[Q]")
                                } else if c
                                    .phantoms
                                    .iter()
                                    .any(|&p| adj.get(&p).is_some_and(|ns| ns.contains(&v)))
                                    && on_curve(v)
                                {
                                    format!("v{v}[N]")
                                } else {
                                    format!("v{v}")
                                }
                            };
                            let mut windows: Vec<(usize, usize, String)> = Vec::new();
                            for (ci, cy) in cycles.iter().enumerate() {
                                for (ei, &(x, _)) in cy.iter().enumerate() {
                                    let hit = c.phantoms.contains(&x)
                                        || hosts.iter().any(|&(_, hc, he)| {
                                            hc == ci && (he == ei || (he + 1) % cy.len() == ei)
                                        });
                                    if hit {
                                        let n = cy.len();
                                        let w = 9.min(n);
                                        let start = if n <= 9 { 0 } else { (ei + n - 4) % n };
                                        let s: Vec<String> =
                                            (0..w).map(|o| tag(cy[(start + o) % n].0)).collect();
                                        windows.push((ci, ei, s.join(" ")));
                                    }
                                }
                            }
                            for (ci, ei, w) in windows {
                                eprintln!(
                                    "YANG_S4_CARRIER_DOMAIN-CYCLES  #{k} patch={key:?} \
                                     comp={pi} cycle={ci} at={ei}: {w}"
                                );
                            }
                        }
                    }
                }
                // inc-2c-3b-0 (`-PLAN3B`): the corrected-cycle PLANNER —
                // compute, per affected component, the exact boundary the
                // gated mutation will re-CDT from. Runs only under the
                // admission rule (every fired site consumed by an
                // applyable corridor); report-only.
                if unconsumed.is_empty()
                    && !corridors.is_empty()
                    && corridors.iter().all(|c| c.applyable())
                {
                    use crate::stage4_corridor as s4c;
                    // Selected components (marks: holds a phantom, or
                    // hosts a junction of some affecting corridor).
                    let vband = |v: u32| -> f64 {
                        let p = mesh.verts[v as usize].as_array();
                        cad_primitives::TAU_WORK
                            .max(8.0 * f64::EPSILON * p[0].abs().max(p[1].abs()).max(p[2].abs()))
                    };
                    let cycles_of = |pi: usize| -> Option<Vec<Vec<u32>>> {
                        crate::stage5_topology::patch_boundary_cycle(&raw_patches[pi], mesh)
                            .ok()
                            .map(|cy| {
                                cy.iter()
                                    .map(|c| c.iter().map(|&(s, _)| s).collect())
                                    .collect()
                            })
                    };
                    let hosts_on = |k: usize, cycles_v: &[Vec<u32>]| -> Vec<s4c::HostEdge> {
                        let c = &corridors[k];
                        let mut out = Vec::new();
                        for (ji, j) in c.junctions.iter().enumerate() {
                            if !matches!(
                                j.disposition,
                                crate::stage4_transit::JunctionDisposition::Mint
                            ) {
                                continue;
                            }
                            let scale = j.sol.iter().fold(0.0f64, |m, &x| m.max(x.abs()));
                            let band = cad_primitives::TAU_WORK.max(8.0 * f64::EPSILON * scale);
                            for cy in cycles_v {
                                let n = cy.len();
                                for i in 0..n {
                                    let (x, y) = (cy[i], cy[(i + 1) % n]);
                                    let d = crate::stage4_transit::dist_point_segment(
                                        j.sol,
                                        mesh.verts[x as usize].as_array(),
                                        mesh.verts[y as usize].as_array(),
                                    );
                                    if d <= band {
                                        out.push((ji, (x, y)));
                                    }
                                }
                            }
                        }
                        out
                    };
                    let mut comp_map: std::collections::BTreeMap<u32, s4c::ComponentInput> =
                        Default::default();
                    let mut host_map: std::collections::BTreeMap<(usize, u32), Vec<s4c::HostEdge>> =
                        Default::default();
                    let mut inputs_ok = true;
                    for (k, c) in corridors.iter().enumerate() {
                        for key in s4c::affected_keys(c) {
                            for (pi, patch) in raw_patches
                                .iter()
                                .enumerate()
                                .filter(|(_, p)| (p.attribution.input, p.attribution.face) == key)
                            {
                                let holds_phantom = patch.tri_indices.iter().any(|&t| {
                                    let tri = mesh.tris[t as usize];
                                    c.phantoms.iter().any(|&p| tri.contains(&p))
                                });
                                let Some(cycles_v) = cycles_of(pi) else {
                                    inputs_ok = false;
                                    continue;
                                };
                                let hosts = hosts_on(k, &cycles_v);
                                if holds_phantom || !hosts.is_empty() {
                                    comp_map.entry(pi as u32).or_insert(s4c::ComponentInput {
                                        key,
                                        comp: pi as u32,
                                        cycles: cycles_v,
                                    });
                                    host_map.insert((k, pi as u32), hosts);
                                }
                            }
                        }
                    }
                    if inputs_ok {
                        let comp_vec: Vec<s4c::ComponentInput> = comp_map.into_values().collect();
                        let far_value = |k: usize, v: u32| -> Option<f64> {
                            let (fi, ff) = corridors[k].far;
                            let faces = match fi {
                                InputId::A => a.faces(),
                                InputId::B => b.faces(),
                            };
                            let s = faces.get(ff as usize)?.surface;
                            crate::stage4_relocate::surface_value_and_normal(
                                s,
                                mesh.verts[v as usize].as_array(),
                            )
                            .map(|(f, _)| f)
                        };
                        let attachments = |k: usize, p: u32| -> Vec<(u32, usize)> {
                            let c = &corridors[k];
                            let mut out = Vec::new();
                            for &w in adj.get(&p).into_iter().flatten() {
                                if !on_curve(w) {
                                    continue;
                                }
                                let pw = patches_of(w);
                                let hits: Vec<usize> = c
                                    .junctions
                                    .iter()
                                    .enumerate()
                                    .filter(|(_, j)| {
                                        pw.contains(&(c.walk_op, j.faces.0))
                                            || pw.contains(&(c.walk_op, j.faces.1))
                                    })
                                    .map(|(i, _)| i)
                                    .collect();
                                if let [one] = hits.as_slice() {
                                    out.push((w, *one));
                                }
                            }
                            out
                        };
                        let hosts = |k: usize, comp: u32| -> Vec<s4c::HostEdge> {
                            host_map.get(&(k, comp)).cloned().unwrap_or_default()
                        };
                        let pctx = s4c::PlanCtx {
                            far_value: &far_value,
                            band: &vband,
                            attachments: &attachments,
                            hosts: &hosts,
                        };
                        let mut pool = s4c::MintPool::default();
                        let (plans, pdeclines) =
                            s4c::plan_invocation(&corridors, &comp_vec, &pctx, &mut pool);
                        let fmt = |r: &s4c::CycleRef| match *r {
                            s4c::CycleRef::Old(v) => format!("v{v}"),
                            s4c::CycleRef::New(i) => format!("N{i}"),
                        };
                        for pl in &plans {
                            eprintln!(
                                "YANG_S4_CARRIER_DOMAIN-PLAN3B comp={} key={:?} \
                                 cycles={:?} removed={:?}",
                                pl.comp,
                                pl.key,
                                pl.corrected.iter().map(|c| c.len()).collect::<Vec<_>>(),
                                pl.removed,
                            );
                            for (ci, cy) in pl.corrected.iter().enumerate() {
                                if cy.len() <= 40 {
                                    let s: Vec<String> = cy.iter().map(fmt).collect();
                                    eprintln!(
                                        "YANG_S4_CARRIER_DOMAIN-PLAN3B   comp={} \
                                         cycle{ci}: {}",
                                        pl.comp,
                                        s.join(" ")
                                    );
                                } else {
                                    // Windows of ±4 around each NEW vertex.
                                    let n = cy.len();
                                    for (i, r) in cy.iter().enumerate() {
                                        if matches!(r, s4c::CycleRef::New(_)) {
                                            let s: Vec<String> = (0..9)
                                                .map(|o| fmt(&cy[(i + n + o - 4) % n]))
                                                .collect();
                                            eprintln!(
                                                "YANG_S4_CARRIER_DOMAIN-PLAN3B   comp={} \
                                                 cycle{ci} at={i}: {}",
                                                pl.comp,
                                                s.join(" ")
                                            );
                                        }
                                    }
                                }
                            }
                        }
                        for (k, d) in &pdeclines {
                            eprintln!("YANG_S4_CARRIER_DOMAIN-PLAN3B DECLINE corridor=#{k} {d:?}");
                        }
                        eprintln!(
                            "YANG_S4_CARRIER_DOMAIN-PLAN3B TOTAL plans={} mints={} \
                             declines={}",
                            plans.len(),
                            pool.verts.len(),
                            pdeclines.len(),
                        );
                    } else {
                        eprintln!(
                            "YANG_S4_CARRIER_DOMAIN-PLAN3B input-collection failed \
                             (cycle walk) — no plans"
                        );
                    }
                }
            }
        }
    }
    if census {
        failure_population_census(
            mesh,
            a,
            b,
            &patch_map,
            &census_sites,
            entry,
            &on_curve,
            &converged,
            "postcondition",
        );
    }
    if census && fires > 0 {
        eprintln!(
            "YANG_S4_CARRIER_DOMAIN TOTAL fires={fires} first=v{:?}",
            first
        );
        // §4-I12 instrument validation: the STOP-vantage walk excludes candidate
        // bounds via `vertex_crossed_domain_endpoint`, a predicate that read
        // ZERO on every vertex walked in its first measurement. A zero from an
        // instrument is a claim about its vantage, so verify the predicate
        // FIRES where this postcondition's own inline two-leg detection just
        // did — the two are meant to be one reading.
        for &(v, _) in &census_sites {
            let xc = vertex_crossed_domain_endpoint(
                mesh,
                &attribution.attributions,
                a,
                b,
                &adj,
                entry,
                v,
            );
            eprintln!(
                "YANG_S45_XCHECK v{v} crossed_domain_endpoint={xc} \
                 (postcondition fire site; expect true)"
            );
        }
        strategy_selection_census(
            mesh,
            &attribution.attributions,
            a,
            b,
            &adj,
            &patch_map,
            &census_sites,
            entry,
            &on_curve,
            &converged,
        );
    }
    Ok(())
}

/// §4-I11: the failure-population census must also see the runs that STOP.
///
/// `relocation_domain_postcondition` sits at the END of Stage 4, so a run that
/// refuses earlier — and the hardest cases all do — reports nothing at all. The
/// corpus census measured 114 of 187 curved cases from the postcondition alone;
/// six of the missing ones STOP inside Stage 4, and a §4.5.1 customer could only
/// have hidden there. So the census is taken on BOTH exits, and the print is
/// tagged with which one, because they are not equivalent: at a STOP the mesh is
/// frozen mid-repair and the later passes never ran.
///
/// This is the same argument that made §4-I9 a postcondition rather than a check
/// at each of thirteen relocation sites — one vantage point that covers every
/// exit, present and future, instead of an edit per site that the next site will
/// forget.
pub(crate) fn stage4_relocate_and_correct(
    mesh: &mut Mesh,
    attribution: &mut TriangleAttributionMap,
    a: &BRep,
    b: &BRep,
    minted_junction_keys: &std::collections::BTreeMap<[u64; 3], crate::boolean::MintProvenance>,
    edge_provenance: &crate::stage3_ssi::PosKeyedEdgeSet,
) -> Result<(Vec<(u32, f64)>, bool), YangError> {
    let census = std::env::var("YANG_S4_CARRIER_DOMAIN").as_deref() == Ok("census");
    // Taken before the inner call, so it is the mesh exactly as Stage 4 received
    // it — strictly earlier than the inner snapshot and never later.
    let entry: Option<Vec<[f64; 3]>> =
        census.then(|| mesh.verts.iter().map(Point3::as_array).collect());
    let out = stage4_relocate_and_correct_inner(
        mesh,
        attribution,
        a,
        b,
        minted_junction_keys,
        edge_provenance,
    );
    if let (Some(entry), Err(e)) = (entry.as_ref(), out.as_ref()) {
        eprintln!("YANG_S45_POP stop_reason={e}");
        // No §4-I9 site list here: that STOP is disabled in census mode, so a run
        // that refused did so for some OTHER reason and its I9 population is
        // whatever the in-domain half reports.
        census_failure_population(mesh, &attribution.attributions, a, b, entry, &[], "stopped");
        // The STOP'd vertex itself is the one member the population census
        // CANNOT see: the refusal happens where the answer is rejected, so the
        // vertex is never written and "Stage 4 moved it" — the proxy for "the
        // optimization ran on it" — is false. It is nonetheless the clearest
        // §4.5 failure in the run, so classify it directly.
        if let YangError::Stage4RegionInvalid { vertex, reason } = e {
            if (*vertex as usize) >= mesh.verts.len() {
                // Sentinel STOPs (u32::MAX) name no vertex; nothing to classify.
                eprintln!(
                    "YANG_S45_POP STOP-VERTEX v{vertex} reason={reason:?} — sentinel, \
                     no vertex to classify"
                );
            } else {
                let patches = build_patch_map(mesh, &attribution.attributions);
                let pos = mesh.verts[*vertex as usize].as_array();
                let (ca, cb) = carrier_counts(&patches, a, b, *vertex, pos);
                let near = ca.max(cb);
                let class = match near {
                    0 => "unlocated (on no surface of either operand)",
                    1 => "INTERIOR — a §4.5.1 customer",
                    _ => "BOUNDARY — excluded from §4.5.1 by Fig-13",
                };
                eprintln!(
                    "YANG_S45_POP STOP-VERTEX v{vertex} reason={reason:?} on_curve={} \
                     carrier=(A{ca},B{cb}) class={class}",
                    vertex_on_curve(&patches, *vertex),
                );
                // §4-I12 — §4.5's SECOND selector clause, from the STOP vantage.
                //
                // §4-I11 classified the STOP vertex by the Fig-13 clause alone and
                // recorded the second clause — "bounded by two successfully
                // optimized points on the same surface" — as the untested,
                // deciding half. It is taken here, where §4.5's repair would
                // actually run: the mesh is frozen at the refusal, mid-repair.
                //
                // "Successfully optimized" at this vantage: CONVERGED (on a
                // surface of each operand at the shared certificate band), not
                // the STOP vertex itself, and not an out-of-domain crosser in
                // §4-I9's sense. The postcondition that computes I9's fire list
                // never runs on a STOP'd run, so its two-leg reading is re-taken
                // per candidate bound: the vertex travelled across a STILL
                // neighbour lying ON its pre→post segment (leg 1) that carries a
                // surface the final position is OFF (leg 2) — a domain ENDPOINT,
                // not a sample. Skipped crossers are counted and printed so a
                // bound reached PAST one is visible as such.
                let adj = build_live_adjacency(mesh);
                let crossers = std::cell::Cell::new(0usize);
                let crossed_endpoint = |w: u32| -> bool {
                    vertex_crossed_domain_endpoint(
                        mesh,
                        &attribution.attributions,
                        a,
                        b,
                        &adj,
                        entry,
                        w,
                    )
                };
                let on_curve = |w: u32| vertex_on_curve(&patches, w);
                let good = |w: u32| -> bool {
                    if w == *vertex || !vertex_converged(mesh, &patches, a, b, w) {
                        return false;
                    }
                    if crossed_endpoint(w) {
                        crossers.set(crossers.get() + 1);
                        return false;
                    }
                    true
                };
                eprintln!(
                    "YANG_S45_SELECT v{vertex} vantage=stopped — §4.5 clause-2 walk from \
                     the STOP vertex (§4-I12)"
                );
                let first = selector_clause2_walk(
                    mesh,
                    &attribution.attributions,
                    a,
                    b,
                    &adj,
                    *vertex,
                    &on_curve,
                    &good,
                )
                .is_some();
                eprintln!(
                    "YANG_S45_SELECT v{vertex} vantage=stopped i9_style_crossers_skipped={} \
                     COMBINED clause1={class} clause2_first_strategy={first} => {}",
                    crossers.get(),
                    if near == 1 && first {
                        "§4.5.1 CONFIRMED customer (both clauses hold)"
                    } else {
                        "§4.5.2 (a selector clause fails)"
                    }
                );
            }
        }
    }
    out
}

fn stage4_relocate_and_correct_inner(
    mesh: &mut Mesh,
    attribution: &mut TriangleAttributionMap,
    a: &BRep,
    b: &BRep,
    minted_junction_keys: &std::collections::BTreeMap<[u64; 3], crate::boolean::MintProvenance>,
    edge_provenance: &crate::stage3_ssi::PosKeyedEdgeSet,
) -> Result<(Vec<(u32, f64)>, bool), YangError> {
    use std::collections::{BTreeMap, HashSet};

    // Non-shadowed aliases for the input BReps (the loops below rebind `a`/`b`
    // to per-triangle vertex indices, so diagnostics/lookups that need the BReps
    // use these).
    let (brep_a, brep_b) = (a, b);

    // I1d probe (read-only): tag every `relocations.push` with its SOURCE LINE
    // so a relocated vertex's AUTHORITY (circle projection, line foot,
    // junction closed-form, …) is attributable offline by position match.
    let i1d_probe = std::env::var_os("YANG_I1D_RELOC_PROBE").is_some();
    let probe_push = |site: u32, v: u32, t: f64, p: Point3| {
        if i1d_probe {
            eprintln!(
                "[i1d-reloc] site=L{site} v{v} t={t:.6} pos=({:.12}, {:.12}, {:.12})",
                p.x(),
                p.y(),
                p.z()
            );
        }
    };

    balance_census(mesh, "s4-entry");

    // §4-I9: positions as Stage 4 found them, for the relocation-domain
    // postcondition at the end. Taken before ANY arm runs, so every relocation
    // path — including `apply_boundary_relocations` far below — is covered by
    // one check rather than a dozen.
    let s4_entry_pos: Vec<[f64; 3]> = mesh.verts.iter().map(Point3::as_array).collect();

    // §4.5.1 inc-1 census (spec §7): `census` flips every OffCurve gate from
    // abort-at-first-fire to record-and-skip; the post-sweep census then
    // reports at the paper's vantage and returns the FIRST recorded error
    // unchanged. Default: gates abort exactly as today.
    let s451_mode = std::env::var("YANG_451").unwrap_or_default();
    let s451_census = s451_mode == "census";
    // FLIPPED ALWAYS-ON 2026-08-22 (spec `yang_451_optimize_across_boundaries.md`
    // §11): the corpus under the gate is category-identical with EXACTLY ONE
    // explained detail delta (R0003: Stage-4 OffCurve → the pre-existing
    // KV9-F2 developable fold it unmasks). `0`/`off` restores the
    // abort-at-first-fire behaviour; `census` measures.
    let s451_repair = !s451_census && s451_mode != "0" && s451_mode != "off";
    let s451_collect = s451_census || s451_repair;
    let mut s45_failures: Vec<(u32, YangError)> = Vec::new();

    // d_ε relocation budget (a conic edge implies a curved input ⇒ Some).
    let d_eps = match stage4_chord_band(a, b) {
        Some(de) => de,
        None => {
            // A conic edge with no circle-bearing input is a producer fault;
            // never default to TAU_WORK for a curved relocation (P10).
            if std::env::var_os("YANG_LRR_PROBE").is_some() {
                eprintln!("YANG_LRR_STOP site=chord_band_none");
            }
            return Err(YangError::stage4_region_invalid(
                u32::MAX,
                Stage4InvalidReason::LocalRefinementRequired,
            ));
        }
    };

    // (1) Collect + classify every conic-edge endpoint from the CURRENT Phase A.
    // PR-YR11: the incidence map (no longer discarded) supplies the TRUE cylinder
    // + cutting plane per Ellipse edge for the closed-form cylinder relocation.
    // This scan runs BEFORE any relocation moves a vertex, so the
    // position-keyed provenance is valid here — this is the classification
    // the relocation maps are built from (spec inc-2).
    let (_infos0, inc0, curves0) = compute_phase_a(mesh, attribution, a, b, edge_provenance)?;

    // Provenance-vouched vertices (spec inc-2 §3c): endpoints of a
    // producer-confirmed intersection edge. Their curve assignment is
    // vouched by the arrangement itself, so the chord-band gates below —
    // whose role is catching WRONG-curve assignments — do not apply to
    // them: a beyond-band vouched vertex is a DRIFTED intersection vertex,
    // and moving it onto its exact curve is the very relocation obligation
    // the witness selection created. Empty (and every gate byte-identical)
    // unless `YANG_S3_EDGE_PROVENANCE_ENABLE` supplied provenance.
    let prov_verts: HashSet<u32> = if edge_provenance.is_empty() {
        HashSet::new()
    } else {
        curves0
            .keys()
            .filter(|&&(s, e)| {
                let ka = crate::stage3_ssi::pos_key(mesh.verts[s as usize]);
                let kb = crate::stage3_ssi::pos_key(mesh.verts[e as usize]);
                edge_provenance.contains(&(ka.min(kb), ka.max(kb)))
            })
            .flat_map(|&(s, e)| [s, e])
            .collect()
    };

    // Per-vertex Circle assignment (deterministic via BTreeMap). PR-YR19: the
    // 4th tuple element carries the originating sphere radius `Some(R)` for a
    // sphere section circle (else `None`) so the relocation guard can scale the
    // in-plane radial band by `(R/r_c)` (spec §2/§4 Site 2).
    let mut vert_circle: BTreeMap<u32, (Point3, Vector3, f64, Option<f64>)> = BTreeMap::new();
    // PR-YR11: per-vertex Ellipse relocation data (the true cylinder + plane +
    // stored ellipse), analogous to `vert_circle`.
    let mut vert_ellipse: BTreeMap<u32, EllipseReloc> = BTreeMap::new();
    // PR-YR21: per-vertex cone-ellipse relocation data (the true cone + plane +
    // stored ellipse + the cone's OWN chord budget), for a `cone ∩ plane`
    // oblique section. Kept separate from `vert_ellipse` (cylinder) so the
    // cylinder path stays byte-identical.
    let mut vert_cone_ellipse: BTreeMap<u32, ConeEllipseReloc> = BTreeMap::new();
    // PR-YR22: per-vertex cone-parabola relocation data for a `cone ∩ plane` θ=α
    // (generator-parallel) section. Kept separate from the ellipse maps so the
    // ellipse/cylinder paths stay byte-identical.
    let mut vert_parabola: BTreeMap<u32, ConeParabolaReloc> = BTreeMap::new();
    // PR-YR23: per-vertex cone-hyperbola relocation data for a `cone ∩ plane`
    // axis-parallel (HYPE) section. Kept separate from the other conic maps so
    // the ellipse/cylinder/parabola paths stay byte-identical.
    let mut vert_cone_hyperbola: BTreeMap<u32, ConeHyperbolaReloc> = BTreeMap::new();
    // KV16 (spec `kv16_hyperbola_arc_vocabulary`): a vertex receiving TWO
    // DIFFERENT cone-hyperbola descriptors (the prism-edge × cone-lateral
    // pierce — same cone, two steep planes, BOTH sections hyperbolas; R0017
    // v47) collapses into the ONE map above, so the increment-5 "≥2 maps"
    // trigger cannot see the junction and the vertex would be relocated
    // onto only one curve (an off-branch endpoint on the other's output
    // edge). Detected at insert time — the vert_ell_junction precedent —
    // and force-fed to the triple-junction relocation below.
    let mut same_type_junction: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
    // PR-F3: per-vertex ruling-LINE relocation data for a plane∥axis ×
    // cylinder intersection edge (ssi C3a/C3b). A `Curve::LineSegment`
    // intersection edge whose incidence carries a CYLINDER is such a line; its
    // arrangement points sit on Stage-1 facet chords, off the exact line (and
    // off the cylinder) by up to the sagitta — they need relocation exactly
    // like the conic arms. Plane∩plane segments are exact and stay skipped.
    let mut vert_line: BTreeMap<u32, LineReloc> = BTreeMap::new();
    // M5 (Y4): per-vertex procedural surface-pair relocation data — the TWO
    // defining surfaces of a `Curve::SurfacePair` edge, carried on the curve
    // itself (no incidence scan needed). Each endpoint is Newton-projected
    // onto BOTH surfaces (`relocate_onto_implicit_pair`), the analog of the
    // torus implicit-pair block but with the pair supplied directly.
    let mut vert_surface_pair: BTreeMap<u32, (Surface, Surface)> = BTreeMap::new();
    // PR-KV9: a vertex shared by TWO DIFFERENT ellipse edges (the crossing
    // points of the Steinmetz cyl×cyl pair) must land on BOTH curves — the
    // exact junction is `(plane₁ ∩ plane₂) line ∩ cylinder`. Detected at
    // insert time (a silent overwrite would relocate one ellipse's endpoint
    // onto the other, collapsing the seam).
    let mut vert_ell_junction: BTreeMap<u32, (EllipseReloc, EllipseReloc)> = BTreeMap::new();
    // M8 disc∩disc CROSSING: a vertex shared by TWO DIFFERENT coplanar CIRCLE
    // edges (the lens corners of two overlapping coplanar cap rims) must land on
    // BOTH circles — the exact junction is the closed-form circle∩circle
    // intersection in their shared plane. Detected at insert time (a silent
    // overwrite would relocate it onto only the last-scanned circle, leaving the
    // other arc's endpoint off-circle by the lens displacement — the kernel-v2
    // "output arc endpoint does not lie on its circle" reject). The circle analog
    // of `vert_ell_junction`.
    let mut vert_circle_junction: BTreeMap<u32, (CircleAssign, CircleAssign)> = BTreeMap::new();
    // PR-KV11: per-vertex plane∩plane intersection-LINE incidences. The pp
    // segments themselves are exact (skipped), but their ENDPOINT on a
    // chordized curved lateral is a TRIPLE point (e.g. capA∩faceB line ×
    // lateral ellipse): the arrangement vertex lies exactly ON the line but
    // only chord-close to the cylinder, so relocating it onto the conic
    // alone slides it OFF the line (off the cap plane — the F0046 Newell
    // disagreement). Collected here; resolved into `vert_ell_junction`
    // after the scan (the junction is `(plane ∩ plane) ∩ cylinder`, the
    // same closed form as the ellipse×ellipse box-edge junction).
    let mut vert_pp_planes: BTreeMap<u32, Vec<(Vector3, f64, Vector3, f64)>> = BTreeMap::new();
    // PR-KV11: junction-aware insertion, shared by BOTH ellipse arms
    // (cylinder+plane AND cylinder×cylinder). A vertex already assigned a
    // DIFFERENT ellipse (the box-edge crossing of two cylinder∩plane
    // sections, or the Steinmetz cyl×cyl crossing) is demoted to the
    // junction map; a silent overwrite would relocate it onto only the
    // last-scanned ellipse, leaving it off the first by the Stage-1 chord
    // error (the F0046-class "endpoint does not lie on its ellipse").
    fn insert_ellipse_or_junction(
        v: u32,
        er: EllipseReloc,
        vert_ellipse: &mut BTreeMap<u32, EllipseReloc>,
        vert_ell_junction: &mut BTreeMap<u32, (EllipseReloc, EllipseReloc)>,
        endpoints: &mut Vec<u32>,
    ) {
        if let Ok(list) = std::env::var("YANG_V_PROBE") {
            if list.split(',').any(|t| t.trim().parse::<u32>() == Ok(v)) {
                eprintln!(
                    "YANG_V_PROBE insert_ellipse v={v} plane_n={:?} plane_d={:.17e} center={:?}",
                    er.plane_n, er.plane_d, er.center,
                );
            }
        }
        if let Some(prev) = vert_ellipse.get(&v).copied() {
            let same = prev.plane_d == er.plane_d
                && prev.plane_n.as_array() == er.plane_n.as_array()
                && prev.center.as_array() == er.center.as_array();
            if !same {
                vert_ellipse.remove(&v);
                vert_ell_junction.insert(v, (prev, er));
                endpoints.push(v);
                return;
            }
        } else if vert_ell_junction.contains_key(&v) {
            // Already a junction of two ellipses; a third co-incident
            // section adds no relocation freedom (the junction point is
            // fully determined by line ∩ cylinder).
            endpoints.push(v);
            return;
        }
        vert_ellipse.insert(v, er);
        endpoints.push(v);
    }
    // M8 disc∩disc: insert a CIRCLE assignment, demoting to `vert_circle_junction`
    // when the vertex already carries a DIFFERENT circle (the lens corner of two
    // coplanar cap rims). Mirrors `insert_ellipse_or_junction`.
    fn insert_circle_or_junction(
        v: u32,
        ca: CircleAssign,
        vert_circle: &mut BTreeMap<u32, CircleAssign>,
        vert_circle_junction: &mut BTreeMap<u32, (CircleAssign, CircleAssign)>,
        endpoints: &mut Vec<u32>,
    ) {
        if let Some(prev) = vert_circle.get(&v).copied() {
            // Same circle (two arcs of ONE split circle meet here) → keep single.
            let same = prev.0.as_array() == ca.0.as_array()
                && prev.1.as_array() == ca.1.as_array()
                && prev.2 == ca.2;
            if !same {
                vert_circle.remove(&v);
                vert_circle_junction.insert(v, (prev, ca));
                endpoints.push(v);
                return;
            }
        } else if vert_circle_junction.contains_key(&v) {
            // Already a circle∩circle junction; a third co-incident circle adds
            // no relocation freedom (the junction is fully determined by the
            // first two), so don't overwrite — just keep it an endpoint.
            endpoints.push(v);
            return;
        }
        vert_circle.insert(v, ca);
        endpoints.push(v);
    }
    let mut endpoints: Vec<u32> = Vec::new();
    if let Ok(list) = std::env::var("YANG_V_PROBE") {
        let probed: Vec<u32> = list
            .split(',')
            .filter_map(|t| t.trim().parse::<u32>().ok())
            .collect();
        for (&(s, e), curve) in &curves0 {
            if probed.contains(&s) || probed.contains(&e) {
                eprintln!("YANG_V_PROBE curves0 edge ({s},{e}) curve={curve:?}");
            }
        }
    }
    // Increment 3 (spec `yang_rim_junction_insertion` §Failure modes):
    // PRE-SCAN EXACTNESS CERTIFICATE for over-determined junction vertices.
    // A vertex whose incidence (inc0) carries ≥3 DISTINCT surfaces and whose
    // position is ALREADY within TAU_WORK of EVERY one of them is a fully
    // determined junction point that needs no relocation — the Stage-1 rim
    // junction insertion (increment 2) mints exactly this population (the
    // truncated-Steinmetz lobe corners, on 4 surfaces bit-exactly). Today
    // such a vertex trips one of the scan's insert-time junction detectors
    // (e.g. the line∩line "out of scope" STOP) or the post-scan
    // over-determined audits. Certified vertices are skipped by EVERY map
    // insertion below: they enter no conic map, no junction map, and no
    // `endpoints`, so every detector, audit, and relocation loop is
    // unchanged for all other vertices (the no-skip audit stays balanced).
    // `surface_value_and_normal`'s F is a signed DISTANCE (shared with
    // `signed_distance_to_surface`), so this is a genuine exactness
    // certificate — never a silent pick (P9): anything inexact keeps
    // today's loud walls. Ordinary 2-surface curve vertices are NOT
    // certified (they keep their retag/`t` bookkeeping).
    // Per-vertex DISTINCT incident surfaces (inc0 dedup) — shared by the
    // increment-3 exactness certificate below and the increment-5 conic
    // triple-junction relocation (spec `yang_stage4_conic_triple_junction`).
    let vert_surfs: BTreeMap<u32, Vec<Surface>> = {
        let mut vert_surfs: BTreeMap<u32, Vec<Surface>> = BTreeMap::new();
        for (&(s, e), entries) in &inc0 {
            for v in [s, e] {
                let list = vert_surfs.entry(v).or_default();
                for &(_input, surf) in entries {
                    if !list.contains(&surf) {
                        list.push(surf);
                    }
                }
            }
        }
        vert_surfs
    };
    let exact_junctions: HashSet<u32> = {
        let mut set = HashSet::new();
        for (&v, surfs) in &vert_surfs {
            if surfs.len() < 3 {
                continue;
            }
            let p = mesh.verts[v as usize].as_array();
            // Increment 4 §4d: scale-aware band (was the absolute
            // TAU_WORK, ~2 ULP at coordinate magnitude 4000 — see
            // `junction_certificate_band`).
            let exact_on_all = surfs.iter().all(|&s| {
                surface_value_and_normal(s, p)
                    .is_some_and(|(f, _)| f.abs() <= junction_certificate_band(p, s))
            });
            if std::env::var_os("YANG_RIM_JUNCTION_PROBE").is_some() {
                eprintln!(
                    "[s4-exact-junction] v={v} surfs={} exact={exact_on_all} p={:?}",
                    surfs.len(),
                    p,
                );
                for &s in surfs {
                    let f = surface_value_and_normal(s, p).map(|(f, _)| f);
                    eprintln!("[s4-exact-junction]   v={v} f={f:?} surf={s:?}");
                }
            }
            if exact_on_all {
                set.insert(v);
            }
        }
        set
    };

    for (&(s, e), curve) in &curves0 {
        match *curve {
            Curve::Parabola {
                vertex,
                normal,
                axis_dir,
                focal_length: _, // recovered from the output edge in eval_source.
            } => {
                // PR-YR22: identify the TRUE cone + cutting plane from this edge's
                // incidence (the θ=α generator-parallel section), mirroring the
                // cone-ellipse arm. Carry the cone's owning `InputId` so its chord
                // budget can be derived from its rim Circle.
                let key = if s < e { (s, e) } else { (e, s) };
                let entries = inc0.get(&key);
                let mut cone: Option<(InputId, Point3, Vector3, f64)> = None;
                let mut plane: Option<(Vector3, f64)> = None;
                if let Some(entries) = entries {
                    for &(input, surf) in entries {
                        match surf {
                            Surface::Cone {
                                apex,
                                axis_dir: cone_axis,
                                half_angle,
                            } => cone = Some((input, apex, cone_axis, half_angle)),
                            Surface::Plane { normal: pn, d: pd } => plane = Some((pn, pd)),
                            _ => {}
                        }
                    }
                }
                let (Some((cone_input, apex, cone_axis_dir, half_angle)), Some((plane_n, plane_d))) =
                    (cone, plane)
                else {
                    // A parabola section that is not a cone+plane pair is out of
                    // scope (producer fault). Loud STOP (P9/P10), mirroring the
                    // cone-ellipse `_ =>` arm.
                    if std::env::var_os("YANG_LRR_PROBE").is_some() {
                        eprintln!(
                            "YANG_LRR_SITE site=parabola_pair_miss edge=({s},{e}) p={:?} \
                             entries={entries:?}",
                            mesh.verts.get(s as usize)
                        );
                    }
                    return Err(YangError::stage4_region_invalid(
                        s,
                        Stage4InvalidReason::LocalRefinementRequired,
                    ));
                };
                let owner = match cone_input {
                    InputId::A => a,
                    InputId::B => b,
                };
                let Some(cone_d_eps) =
                    cone_chord_budget_from_owner(apex, cone_axis_dir, half_angle, owner)
                else {
                    if std::env::var_os("YANG_LRR_PROBE").is_some() {
                        eprintln!(
                            "YANG_LRR_SITE site=parabola_cone_budget edge=({s},{e}) p={:?} \
                             apex={apex:?} half_angle={half_angle}",
                            mesh.verts.get(s as usize)
                        );
                    }
                    return Err(YangError::stage4_region_invalid(
                        s,
                        Stage4InvalidReason::LocalRefinementRequired,
                    ));
                };
                let cpr = ConeParabolaReloc {
                    apex,
                    cone_axis_dir,
                    half_angle,
                    plane_n,
                    plane_d,
                    vertex,
                    normal,
                    para_axis_dir: axis_dir,
                    cone_d_eps,
                };
                for v in [s, e] {
                    // Increment 3: certified exact junction — enters no map (see above).
                    if exact_junctions.contains(&v) {
                        continue;
                    }
                    vert_parabola.insert(v, cpr);
                    endpoints.push(v);
                }
            }
            Curve::Hyperbola {
                center,
                normal,
                major_axis,
                semi_transverse: _, // recovered from the output edge in eval_source.
                semi_conjugate,
            } => {
                // PR-YR23: identify the TRUE cone + cutting plane from this edge's
                // incidence (the axis-parallel HYPE section), mirroring the
                // cone-parabola arm. Carry the cone's owning `InputId` so its
                // chord budget can be derived from its rim Circle.
                let key = if s < e { (s, e) } else { (e, s) };
                let entries = inc0.get(&key);
                let mut cone: Option<(InputId, Point3, Vector3, f64)> = None;
                let mut plane: Option<(Vector3, f64)> = None;
                if let Some(entries) = entries {
                    for &(input, surf) in entries {
                        match surf {
                            Surface::Cone {
                                apex,
                                axis_dir: cone_axis,
                                half_angle,
                            } => cone = Some((input, apex, cone_axis, half_angle)),
                            Surface::Plane { normal: pn, d: pd } => plane = Some((pn, pd)),
                            _ => {}
                        }
                    }
                }
                let (Some((cone_input, apex, cone_axis_dir, half_angle)), Some((plane_n, plane_d))) =
                    (cone, plane)
                else {
                    // A hyperbola section that is not a cone+plane pair is out of
                    // scope (producer fault). Loud STOP (P9/P10), mirroring the
                    // cone-parabola arm.
                    if std::env::var_os("YANG_LRR_PROBE").is_some() {
                        eprintln!(
                            "YANG_LRR_SITE site=hyperbola_pair_miss edge=({s},{e}) p={:?} \
                             entries={entries:?}",
                            mesh.verts.get(s as usize)
                        );
                    }
                    return Err(YangError::stage4_region_invalid(
                        s,
                        Stage4InvalidReason::LocalRefinementRequired,
                    ));
                };
                let owner = match cone_input {
                    InputId::A => a,
                    InputId::B => b,
                };
                let Some(cone_d_eps) =
                    cone_chord_budget_from_owner(apex, cone_axis_dir, half_angle, owner)
                else {
                    if std::env::var_os("YANG_LRR_PROBE").is_some() {
                        eprintln!(
                            "YANG_LRR_SITE site=hyperbola_cone_budget edge=({s},{e}) p={:?} \
                             apex={apex:?} half_angle={half_angle}",
                            mesh.verts.get(s as usize)
                        );
                    }
                    return Err(YangError::stage4_region_invalid(
                        s,
                        Stage4InvalidReason::LocalRefinementRequired,
                    ));
                };
                let chr = ConeHyperbolaReloc {
                    apex,
                    cone_axis_dir,
                    half_angle,
                    plane_n,
                    plane_d,
                    center,
                    normal,
                    major_axis,
                    semi_conjugate,
                    cone_d_eps,
                };
                for v in [s, e] {
                    // Increment 3: certified exact junction — enters no map (see above).
                    if exact_junctions.contains(&v) {
                        continue;
                    }
                    // KV16: a SECOND, DIFFERENT descriptor for the same
                    // vertex is a same-type conic junction (two hyperbolas
                    // meeting) — never silently overwrite-and-relocate onto
                    // one curve; route to the triple-junction pass.
                    if let Some(prev) = vert_cone_hyperbola.get(&v) {
                        let differs = prev.apex != chr.apex
                            || prev.cone_axis_dir != chr.cone_axis_dir
                            || prev.half_angle != chr.half_angle
                            || prev.plane_n != chr.plane_n
                            || prev.plane_d != chr.plane_d;
                        if differs {
                            same_type_junction.insert(v);
                            if std::env::var_os("YANG_SAMETYPE_PROBE").is_some() {
                                let pv = mesh.verts[v as usize].as_array();
                                eprintln!(
                                    "[sametype-probe] v={v} p=({:.6},{:.6},{:.6}) hyperbola \
                                     junction: prev apex={:?} ha={:.6} plane_n={:?} d={:.6} \
                                     -> new apex={:?} ha={:.6} plane_n={:?} d={:.6}",
                                    pv[0],
                                    pv[1],
                                    pv[2],
                                    prev.apex,
                                    prev.half_angle,
                                    prev.plane_n,
                                    prev.plane_d,
                                    chr.apex,
                                    chr.half_angle,
                                    chr.plane_n,
                                    chr.plane_d,
                                );
                            }
                        }
                    }
                    vert_cone_hyperbola.insert(v, chr);
                    endpoints.push(v);
                }
            }
            Curve::Circle {
                center,
                normal,
                radius,
            } => {
                // PR-YR19: scan this edge's incidence for a `Surface::Sphere`
                // owner → `Some(R)`; else `None`. Uses the SAME canonical key as
                // the Ellipse arm below.
                let key = if s < e { (s, e) } else { (e, s) };
                let mut source_radius: Option<f64> = None;
                if let Some(entries) = inc0.get(&key) {
                    for &(_input, surf) in entries {
                        if let Surface::Sphere { radius: sr, .. } = surf {
                            source_radius = Some(sr);
                        }
                    }
                }
                for v in [s, e] {
                    // Increment 3: certified exact junction — enters no map (see above).
                    if exact_junctions.contains(&v) {
                        continue;
                    }
                    if i1d_probe {
                        // I1e census: which edge chained this vertex onto the
                        // circle, and how far off the exact circle it sits.
                        let p = mesh.verts[v as usize];
                        let pa = p.as_array();
                        let ca = center.as_array();
                        let na = normalize3(normal.as_array());
                        let w = [pa[0] - ca[0], pa[1] - ca[1], pa[2] - ca[2]];
                        let h = w[0] * na[0] + w[1] * na[1] + w[2] * na[2];
                        let inplane = [w[0] - h * na[0], w[1] - h * na[1], w[2] - h * na[2]];
                        let rad = (inplane[0] * inplane[0]
                            + inplane[1] * inplane[1]
                            + inplane[2] * inplane[2])
                            .sqrt();
                        let resid = (h * h + (rad - radius) * (rad - radius)).sqrt();
                        let inc: Vec<String> = inc0
                            .get(&key)
                            .map(|entries| {
                                entries
                                    .iter()
                                    .map(|&(input, surf)| {
                                        let tag = match surf {
                                            Surface::Plane { .. } => "Plane",
                                            Surface::Cylinder { .. } => "Cyl",
                                            Surface::Sphere { .. } => "Sph",
                                            Surface::Cone { .. } => "Cone",
                                            Surface::Torus { .. } => "Torus",
                                        };
                                        format!("{input:?}:{tag}")
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();
                        eprintln!(
                            "[i1e-circle-edge] edge=({s},{e}) v{v} resid={resid:.6e} \
                             pos=({:.12}, {:.12}, {:.12}) r={radius:.9} inc=[{}]",
                            pa[0],
                            pa[1],
                            pa[2],
                            inc.join(","),
                        );
                    }
                    insert_circle_or_junction(
                        v,
                        (center, normal, radius, source_radius),
                        &mut vert_circle,
                        &mut vert_circle_junction,
                        &mut endpoints,
                    );
                }
            }
            Curve::Ellipse {
                center,
                normal,
                major_axis,
                major_radius,
                minor_radius,
            } => {
                // PR-YR11: identify the TRUE cylinder + cutting plane from this
                // edge's incidence (the two incident surfaces of DIFFERENT
                // inputs). A conic Ellipse edge is, by construction, one cylinder
                // lateral + one cutting plane.
                let key = if s < e { (s, e) } else { (e, s) };
                let entries = inc0.get(&key);
                let mut cyl: Option<(Point3, Vector3, f64)> = None;
                // PR-KV9: ALL cylinder entries with their owning inputs —
                // a cylinder×cylinder ellipse needs both for the per-point
                // gradient band + the combined chord budget.
                let mut cyls: Vec<(InputId, Point3, Vector3, f64)> = Vec::new();
                let mut plane: Option<(Vector3, f64)> = None;
                // PR-YR21: additionally scan for a `Surface::Cone` owner (the
                // cone+plane oblique section). Carry the owning `InputId` so the
                // cone's chord budget can be derived from its rim Circle.
                let mut cone: Option<(InputId, Point3, Vector3, f64)> = None;
                if let Some(entries) = entries {
                    for &(input, surf) in entries {
                        match surf {
                            Surface::Cylinder {
                                axis_point,
                                axis_dir,
                                radius,
                            } => {
                                cyl = Some((axis_point, axis_dir, radius));
                                cyls.push((input, axis_point, axis_dir, radius));
                            }
                            Surface::Plane { normal: pn, d: pd } => plane = Some((pn, pd)),
                            Surface::Cone {
                                apex,
                                axis_dir,
                                half_angle,
                            } => cone = Some((input, apex, axis_dir, half_angle)),
                            _ => {}
                        }
                    }
                }
                match (cyl, cone, plane) {
                    // YR11 cylinder + plane: the EXISTING path, byte-for-byte.
                    (Some((axis_point, axis_dir, radius)), _, Some((plane_n, plane_d))) => {
                        let er = EllipseReloc {
                            axis_point,
                            axis_dir,
                            radius,
                            plane_n,
                            plane_d,
                            center,
                            normal,
                            major_axis,
                            major_radius,
                            minor_radius,
                            second_cyl: None,
                        };
                        for v in [s, e] {
                            // Increment 3: certified exact junction — enters no map (see above).
                            if exact_junctions.contains(&v) {
                                continue;
                            }
                            insert_ellipse_or_junction(
                                v,
                                er,
                                &mut vert_ellipse,
                                &mut vert_ell_junction,
                                &mut endpoints,
                            );
                        }
                    }
                    // PR-YR21 cone + plane (no cylinder): the new cone-ellipse
                    // path. Derive the cone's OWN chord budget from the cone
                    // owner's rim Circle (spec §3.3); a cone owner with no rim
                    // Circle is a producer fault → loud STOP (never TAU_WORK).
                    (
                        None,
                        Some((cone_input, apex, axis_dir, half_angle)),
                        Some((plane_n, plane_d)),
                    ) => {
                        let owner = match cone_input {
                            InputId::A => a,
                            InputId::B => b,
                        };
                        let Some(cone_d_eps) =
                            cone_chord_budget_from_owner(apex, axis_dir, half_angle, owner)
                        else {
                            if std::env::var_os("YANG_LRR_PROBE").is_some() {
                                eprintln!(
                                    "YANG_LRR_SITE site=cone_ellipse_budget edge=({s},{e}) \
                                     p={:?} apex={apex:?} half_angle={half_angle}",
                                    mesh.verts.get(s as usize)
                                );
                            }
                            return Err(YangError::stage4_region_invalid(
                                s,
                                Stage4InvalidReason::LocalRefinementRequired,
                            ));
                        };
                        let cer = ConeEllipseReloc {
                            apex,
                            axis_dir,
                            half_angle,
                            plane_n,
                            plane_d,
                            center,
                            normal,
                            major_axis,
                            major_radius,
                            minor_radius,
                            cone_d_eps,
                        };
                        for v in [s, e] {
                            // Increment 3: certified exact junction — enters no map (see above).
                            if exact_junctions.contains(&v) {
                                continue;
                            }
                            // KV16b (spec `kv16b_cone_ellipse_same_type_junction`):
                            // a SECOND, DIFFERENT descriptor for the same vertex
                            // is a same-type conic junction (two cone-ellipses
                            // meeting) — never silently overwrite-and-relocate
                            // onto one curve; route to the triple-junction pass
                            // (the KV16 hyperbola recipe, sibling map).
                            if let Some(prev) = vert_cone_ellipse.get(&v) {
                                let differs = prev.apex != cer.apex
                                    || prev.axis_dir != cer.axis_dir
                                    || prev.half_angle != cer.half_angle
                                    || prev.plane_n != cer.plane_n
                                    || prev.plane_d != cer.plane_d;
                                if differs {
                                    same_type_junction.insert(v);
                                    if std::env::var_os("YANG_SAMETYPE_PROBE").is_some() {
                                        let pv = mesh.verts[v as usize].as_array();
                                        eprintln!(
                                            "[sametype-probe] v={v} p=({:.6},{:.6},{:.6}) \
                                             cone-ellipse junction: prev apex={:?} ha={:.6} \
                                             plane_n={:?} d={:.6} -> new apex={:?} ha={:.6} \
                                             plane_n={:?} d={:.6}",
                                            pv[0],
                                            pv[1],
                                            pv[2],
                                            prev.apex,
                                            prev.half_angle,
                                            prev.plane_n,
                                            prev.plane_d,
                                            cer.apex,
                                            cer.half_angle,
                                            cer.plane_n,
                                            cer.plane_d,
                                        );
                                    }
                                }
                            }
                            vert_cone_ellipse.insert(v, cer);
                            endpoints.push(v);
                        }
                    }
                    // PR-KV9: cylinder × CYLINDER ellipse (the equal-radius
                    // intersecting-axes Steinmetz section, ssi cyl∩cyl). The
                    // ellipse lies in a KNOWN plane — its own stored frame —
                    // and it equals `cylinder ∩ that-plane` for EITHER owner
                    // (the curve is on both), so the existing cylinder+plane
                    // relocation closed form applies verbatim with the plane
                    // derived from the stored curve: n̂ from the ellipse
                    // normal, d = −n̂·center. `cyl` here holds the LAST
                    // cylinder scanned; with two cylinder entries either is
                    // exact, and the incidence order is deterministic.
                    (Some(_), None, None) if cyls.len() == 2 => {
                        // Deterministic owner order: sort by InputId (A first).
                        let mut cs = cyls.clone();
                        cs.sort_by_key(|&(i, ..)| matches!(i, InputId::B));
                        let (i1, axis_point, axis_dir, radius) = cs[0];
                        let (i2, ap2, ad2, _) = cs[1];
                        let budget = chord_tol_for_curved_owner(i1, a, b, 0, (s, e))?
                            + chord_tol_for_curved_owner(i2, a, b, 0, (s, e))?;
                        let nn = normalize3(normal.as_array());
                        let plane_n = Vector3::new(nn[0], nn[1], nn[2]);
                        let c = center.as_array();
                        let plane_d = -(nn[0] * c[0] + nn[1] * c[1] + nn[2] * c[2]);
                        let er = EllipseReloc {
                            axis_point,
                            axis_dir,
                            radius,
                            plane_n,
                            plane_d,
                            center,
                            normal,
                            major_axis,
                            major_radius,
                            minor_radius,
                            second_cyl: Some((ap2, ad2, budget)),
                        };
                        for v in [s, e] {
                            // Increment 3: certified exact junction — enters no map (see above).
                            if exact_junctions.contains(&v) {
                                continue;
                            }
                            insert_ellipse_or_junction(
                                v,
                                er,
                                &mut vert_ellipse,
                                &mut vert_ell_junction,
                                &mut endpoints,
                            );
                        }
                    }
                    // Anything else (sphere, coplanar multi-solid): out of
                    // scope. Loud STOP (P9/P10).
                    _ => {
                        if std::env::var_os("YANG_LRR_PROBE").is_some() {
                            eprintln!(
                                "YANG_LRR_SITE site=ellipse_combo edge=({s},{e}) p={:?} \
                                 n_cyls={} entries={entries:?}",
                                mesh.verts.get(s as usize),
                                cyls.len()
                            );
                        }
                        return Err(YangError::stage4_region_invalid(
                            s,
                            Stage4InvalidReason::LocalRefinementRequired,
                        ));
                    }
                }
            }
            // M5 (Y4): a procedural surface-pair edge carries its two defining
            // surfaces directly. Like the TORUS block, its endpoints are an
            // implicit-pair (degree-4) relocation handled AFTER the conic
            // audit below — NOT part of the conic `endpoints`/`relocations`
            // bookkeeping (a procedural curve has no `t`). Only record the
            // pair here.
            Curve::SurfacePair { a, b } => {
                for v in [s, e] {
                    // Increment 3: certified exact junction — enters no map (see above).
                    if exact_junctions.contains(&v) {
                        continue;
                    }
                    // KV16 precedent for the PROCEDURAL map (2026-08-19,
                    // R0044 anchor): a SECOND, DIFFERENT surface pair at the
                    // same vertex is a same-type surface-pair junction — e.g.
                    // cyl_A × cone_B1 meeting cyl_A × cone_B2 on the gear's
                    // tooth-flank crease circle: THREE surfaces. The one-slot
                    // map kept the LAST pair, the triple block's `n_maps < 2`
                    // skipped the vertex, the pair loop relocated it onto
                    // (cyl, cone_B2) alone, and the OTHER edge's endpoint was
                    // 0.35 off cone_B1 (kernel-v2's surface-pair endpoint
                    // check caught it loudly). Route to the triple pass.
                    if let Some(&(pa, pb)) = vert_surface_pair.get(&v) {
                        let same = (pa == a && pb == b) || (pa == b && pb == a);
                        if !same {
                            same_type_junction.insert(v);
                            if std::env::var_os("YANG_SAMETYPE_PROBE").is_some() {
                                eprintln!(
                                    "[sametype-probe] v={v} p={:?} surface-pair junction: \
                                     prev=({pa:?}, {pb:?}) -> new=({a:?}, {b:?})",
                                    mesh.verts.get(v as usize)
                                );
                            }
                        }
                    }
                    vert_surface_pair.insert(v, (a, b));
                }
            }
            Curve::LineSegment => {
                // PR-F3: a LineSegment intersection edge between a PLANE and a
                // CYLINDER is a ruling LINE of the cylinder (ssi plane_cylinder
                // C3a/C3b). Recompute the exact line from the incidence and
                // re-select the unique candidate through both endpoints (the
                // SAME rule Stage 3's `build_intersection_curves` used).
                // Plane∩plane segments are exact → skip. Any OTHER curved
                // surface on a LineSegment edge is out of scope → loud STOP
                // (P9).
                //
                // The `cone × plane` GENERATOR line is the third convertible
                // pair (the fixture the PR-F3 note deferred has arrived:
                // corpus R0008 + R0085-op2). A cutting plane through the cone
                // APEX degenerates the conic section into generator lines
                // (`ssi_rs::plane_cone` AP-line / AP-lines) — the same
                // recompute-and-reselect rule, with the CONE owner's Stage-1
                // band (`cone_chord_tol_for_owner`, PR-YR17) as `tol`, exactly
                // as Stage 3 derives it for a cone-owning edge. See the
                // band note at `line_tol` below for why the pair takes the
                // FLAT band and why that is the derived value, not a default.
                let key = if s < e { (s, e) } else { (e, s) };
                let Some(entries) = inc0.get(&key) else {
                    continue;
                };
                // KV6d Tier B: a TORUS-bearing LineSegment edge is a degree-4
                // intersection handled by the implicit-pair Newton relocation
                // block after this scan — defer it here (the conic LineSegment
                // arm has no closed form for it). Skip rather than STOP.
                if entries
                    .iter()
                    .any(|&(_, s)| matches!(s, Surface::Torus { .. }))
                {
                    continue;
                }
                let mut cyls: Vec<(InputId, Surface)> = Vec::new();
                let mut cones: Vec<(InputId, Surface)> = Vec::new();
                let mut plane_surf: Option<Surface> = None;
                let mut pp: Vec<(Vector3, f64)> = Vec::new();
                let mut other_curved = false;
                for &(input, surf) in entries {
                    match surf {
                        Surface::Cylinder { .. } => cyls.push((input, surf)),
                        Surface::Cone { .. } => cones.push((input, surf)),
                        Surface::Plane { normal, d } => {
                            plane_surf = Some(surf);
                            pp.push((normal, d));
                        }
                        _ => other_curved = true,
                    }
                }
                // Three convertible pairs: cylinder × ⊥plane (F3), PARALLEL
                // cylinder × cylinder (PR-KV9, ssi cyl∥cyl ruling lines), and
                // cone × through-apex plane (the generator arm). Other
                // curved-bearing line edges stay a loud STOP. Every arm is
                // guarded on the OTHER curved kind being absent so a
                // three-surface incidence (cyl + cone + plane) cannot be
                // silently read as a two-surface pair.
                let (surf_a, surf_b, tol) = match (cyls.as_slice(), plane_surf) {
                    // Cone × plane FIRST: `cones` is not part of the scrutinee,
                    // so the `([], _)` plane∩plane arm below would otherwise
                    // swallow a cone-bearing edge as an exact pp segment.
                    ([], Some(pl)) if !other_curved && cones.len() == 1 => {
                        let (ci, cs) = cones[0];
                        (cs, pl, cone_chord_tol_for_owner(cs, ci, a, b, 0, (s, e))?)
                    }
                    ([(ci, cs)], Some(pl)) if !other_curved && cones.is_empty() => {
                        (*cs, pl, chord_tol_for_curved_owner(*ci, a, b, 0, (s, e))?)
                    }
                    ([(i1, c1), (i2, c2)], None) if !other_curved && cones.is_empty() => {
                        // Both meshes' facet chords contribute to the crossing
                        // vertex — the combined band is the SUM of the two
                        // owners' Stage-1 bounds (derived, not widening).
                        let t = chord_tol_for_curved_owner(*i1, a, b, 0, (s, e))?
                            + chord_tol_for_curved_owner(*i2, a, b, 0, (s, e))?;
                        (*c1, *c2, t)
                    }
                    ([], _) if !other_curved && cones.is_empty() => {
                        // plane∩plane — the segment is exact, but record the
                        // line's planes per endpoint for the PR-KV11 triple-
                        // point pass below.
                        if pp.len() == 2 {
                            let entry = (pp[0].0, pp[0].1, pp[1].0, pp[1].1);
                            for v in [s, e] {
                                // Increment 3: certified exact junction — enters no map (see above).
                                if exact_junctions.contains(&v) {
                                    continue;
                                }
                                vert_pp_planes.entry(v).or_default().push(entry);
                            }
                        }
                        continue;
                    }
                    _ => {
                        if std::env::var_os("YANG_LRR_PROBE").is_some() {
                            eprintln!(
                                "YANG_LRR_SITE site=lineseg_combo edge=({s},{e}) p={:?} \
                                 n_cyls={} n_cones={} n_pp={} other_curved={other_curved} \
                                 entries={entries:?}",
                                mesh.verts.get(s as usize),
                                cyls.len(),
                                cones.len(),
                                pp.len()
                            );
                        }
                        return Err(YangError::stage4_region_invalid(
                            s,
                            Stage4InvalidReason::LocalRefinementRequired,
                        ));
                    }
                };
                let to_ssi_err = |reason| YangError::SsiRefinementFailed {
                    edge: (s, e),
                    reason,
                };
                let q0 = surface_to_quadric(surf_a).map_err(to_ssi_err)?;
                let q1 = surface_to_quadric(surf_b).map_err(to_ssi_err)?;
                let returned =
                    ssi_rs::intersect(&q0, &q1).map_err(|err| YangError::SsiRefinementFailed {
                        edge: (s, e),
                        reason: SsiRefinementError::IntersectFailed(err),
                    })?;
                let p_s = mesh.verts[s as usize];
                let p_e = mesh.verts[e as usize];
                // PR-F3b: the SAME propagated band as Stage-3 matching (the
                // metric is shared, so every gate carries the factor).
                // N46 (task #164): a `cylinder ∩ plane` generator uses the EXACT
                // worst-case band `√(B_in² + tol²)` (superseding the first-order
                // `line_band_amplification`, which under-admits near tangency —
                // R0026's `AmbiguousCurve{2,0}` reaches THIS Stage-4 relocation
                // once Stage-3 selection passes). Non-cyl/plane pairs keep the
                // linear factor (cyl∩cyl Steinmetz, cone-apex lines).
                //
                // CONE-APEX GENERATORS take the flat band, and that is the
                // DERIVED value rather than a fallback default. Both helpers
                // return `None` for a cone pair, so the amplification is 1.0 —
                // which is exactly right here: the general membership factor is
                // `1/‖ĝ_plane × ĝ_cone‖` (the form the cyl×plane `r/√(r²−d²)`
                // specializes), and along a generator d̂ = â·cosα + û·sinα the
                // cone's unit normal is `n̂_c = û·cosα − â·sinα`. A plane that
                // meets the cone in TWO crossed generators contains the axis
                // (`k = n̂·â = 0` — the AP-lines branch), so û = ±(n̂ × â) ⊥ n̂
                // and â ⊥ n̂ give `n̂ · n̂_c = 0`: the gradients are exactly
                // orthogonal, sin = 1, amplification = 1. The TANGENT-generator
                // case (AP-line, one candidate) has n̂ ∥ n̂_c and a diverging
                // factor; the flat band UNDER-admits there, so such an edge
                // fails `matched_n == 1` and STOPs loud — the P9-correct
                // posture, never a silent match. This matches Stage 3's Line
                // band for the same pair byte-for-byte, so selection here
                // cannot disagree with the selection that produced the edge.
                let line_tol = cyl_plane_generator_band(surf_a, surf_b, tol).unwrap_or_else(|| {
                    line_band_amplification(surf_a, surf_b).unwrap_or(1.0) * tol
                });
                let mut matched: Option<LineReloc> = None;
                let mut matched_n = 0usize;
                let mut matched_lines: Vec<(Point3, Vector3)> = Vec::new();
                for c in &returned {
                    if let ssi_rs::SsiCurve::Line { point, dir } = *c {
                        if line_perp_distance(p_s, point, dir) <= line_tol
                            && line_perp_distance(p_e, point, dir) <= line_tol
                        {
                            matched_n += 1;
                            matched_lines.push((point, dir));
                            matched = Some(LineReloc {
                                point,
                                dir,
                                band_budget: line_tol,
                            });
                        }
                    }
                }
                // R0072: near-tangent plane∩cylinder yields two near-coincident
                // parallel generators that both pass the band; the edge lies on
                // exactly one. Break the tie by position (the disjoint-lowest
                // endpoint-distance interval) — the SAME rule Stage 3 uses. If no
                // unambiguous winner (overlapping intervals), the loud
                // `AmbiguousCurve` below stands.
                //
                // R0008: this site used the R0072-only
                // `select_disjoint_parallel_line` wrapper, whose mutual-parallelism
                // precheck rejects the two CROSSING generators of a cone sectioned
                // through its apex. Stage 3 was generalized to the
                // parallelism-free core by N45 (#163, commit 9fca8393) and this
                // site was not, so the two stages have been running DIFFERENT
                // tie-breaks — a latent violation of the "selection here cannot
                // disagree with Stage 3" contract this arm rests on. It was
                // unobservable while every cone-apex edge STOPped earlier, in the
                // pair match above. Calling the core restores the invariant; the
                // criterion is identical for parallel candidates (the wrapper
                // delegates to it), so the R0072 path is unchanged.
                if matched_n > 1 {
                    if let Some(wk) = select_disjoint_line_by_distance(&matched_lines, p_s, p_e) {
                        let (point, dir) = matched_lines[wk];
                        matched_n = 1;
                        matched = Some(LineReloc {
                            point,
                            dir,
                            band_budget: line_tol,
                        });
                    }
                }
                let Some(lr) = (if matched_n == 1 { matched } else { None }) else {
                    return Err(YangError::SsiRefinementFailed {
                        edge: (s, e),
                        reason: SsiRefinementError::AmbiguousCurve {
                            candidates: returned.len(),
                            matched: matched_n,
                        },
                    });
                };
                for v in [s, e] {
                    // Increment 3: certified exact junction — enters no map (see above).
                    if exact_junctions.contains(&v) {
                        continue;
                    }
                    // A vertex on TWO DIFFERENT lines (e.g. a box corner ruling
                    // piercing the cylinder) would need a line∩line junction —
                    // out of scope, loud STOP rather than silently overwriting
                    // (the same defect class F3 fixes for line+circle).
                    if let Some(prev) = vert_line.get(&v) {
                        let same = line_perp_distance(prev.point, lr.point, lr.dir)
                            <= cad_primitives::TAU_MODEL
                            && {
                                let d1 = normalize3(prev.dir.as_array());
                                let d2 = normalize3(lr.dir.as_array());
                                let cx = [
                                    d1[1] * d2[2] - d1[2] * d2[1],
                                    d1[2] * d2[0] - d1[0] * d2[2],
                                    d1[0] * d2[1] - d1[1] * d2[0],
                                ];
                                (cx[0] * cx[0] + cx[1] * cx[1] + cx[2] * cx[2]).sqrt()
                                    <= cad_primitives::TAU_MODEL
                            };
                        if !same {
                            if std::env::var_os("YANG_LRR_PROBE").is_some() {
                                eprintln!(
                                    "YANG_LRR_SITE site=line_line_junction v={v} p={:?} \
                                     prev={prev:?} new={lr:?}",
                                    mesh.verts.get(v as usize)
                                );
                            }
                            return Err(YangError::stage4_region_invalid(
                                v,
                                Stage4InvalidReason::LocalRefinementRequired,
                            ));
                        }
                    }
                    vert_line.insert(v, lr);
                    endpoints.push(v);
                }
            }
        }
    }

    // PR-KV11: resolve ellipse × (plane∩plane line) TRIPLE points. An ellipse
    // endpoint that also terminates an exact pp-segment (the cap∩face trace
    // crossing the lateral) must land on `(plane ∩ plane) ∩ cylinder`, not on
    // the ellipse alone — reuse the ellipse-junction closed form with a
    // synthetic second member carrying the line's OTHER plane (the one that
    // is not the ellipse's own cutting plane; bit identity — both come from
    // the same incidence `Surface::Plane` values).
    {
        let shared: Vec<u32> = vert_ellipse
            .keys()
            .filter(|v| vert_pp_planes.contains_key(v))
            .copied()
            .collect();
        for v in shared {
            let e_a = vert_ellipse[&v];
            let mut others: Vec<(Vector3, f64)> = Vec::new();
            for &(n1, d1, n2, d2) in &vert_pp_planes[&v] {
                let m1 = n1.as_array() == e_a.plane_n.as_array() && d1 == e_a.plane_d;
                let m2 = n2.as_array() == e_a.plane_n.as_array() && d2 == e_a.plane_d;
                let other = if m1 {
                    Some((n2, d2))
                } else if m2 {
                    Some((n1, d1))
                } else {
                    None
                };
                if let Some(o) = other {
                    if !others
                        .iter()
                        .any(|&(n, d)| n.as_array() == o.0.as_array() && d == o.1)
                    {
                        others.push(o);
                    }
                }
            }
            match others.len() {
                // A pp-line through an ellipse endpoint whose pair does not
                // include the ellipse's own plane, or more than one distinct
                // crossing line: relocating onto any single curve leaves the
                // vertex off the others — loud STOP, never a silent pick
                // (P9/P10).
                0 | 2.. => {
                    return Err(YangError::stage4_region_invalid(
                        v,
                        Stage4InvalidReason::LocalRefinementRequired,
                    ));
                }
                1 => {
                    let (on, od) = others[0];
                    let e_b = EllipseReloc {
                        plane_n: on,
                        plane_d: od,
                        ..e_a
                    };
                    vert_ellipse.remove(&v);
                    vert_ell_junction.insert(v, (e_a, e_b));
                }
            }
        }
    }

    // PR-F3: a vertex shared by a LINE edge and a CIRCLE edge is a TRIPLE
    // point — it must end up on BOTH curves. Relocating onto either alone
    // leaves it off the other (the KV6b-F3 probe defect: radius exactly r,
    // axial coordinate off by the sagitta → output-face plane vs Newell
    // disagreement). The exact junction is `line ∩ plane-of-circle`: the line
    // lies ON the cylinder and the circle IS `cylinder ∩ circle-plane`, so the
    // line's piercing of the circle plane lies exactly on the circle. Pull
    // such vertices OUT of both single-curve maps into a junction map.
    let mut vert_junction: BTreeMap<u32, (LineReloc, CircleAssign)> = BTreeMap::new();
    {
        let shared: Vec<u32> = vert_line
            .keys()
            .filter(|v| vert_circle.contains_key(v))
            .copied()
            .collect();
        for v in shared {
            let lr = vert_line.remove(&v).expect("key from vert_line");
            let circ = vert_circle.remove(&v).expect("checked contains_key");
            vert_junction.insert(v, (lr, circ));
        }
    }

    // Task #146 (spec `yang_stage4_circle_pp_line_junction` branches 1–3):
    // resolve CIRCLE × (plane∩plane line) TRIPLE points — the circle analog
    // of the PR-KV11 ellipse×pp pass above. A vertex on both a section
    // circle and an exact pp-line is their junction; relocating onto the
    // circle alone slides it off the line's planes at real scale (the F0064
    // Newell-normal class). Exactly ONE distinct pp-line reroutes; zero or
    // several distinct lines (or an ellipse junction already claimed) is
    // over-determined — loud STOP, never a silent pick (P9/P10).
    let mut vert_pp_circle_junction: BTreeMap<u32, ((Point3, Vector3), CircleAssign)> =
        BTreeMap::new();
    {
        let shared: Vec<u32> = vert_circle
            .keys()
            .filter(|v| vert_pp_planes.contains_key(v))
            .copied()
            .collect();
        for v in shared {
            if vert_ell_junction.contains_key(&v) {
                return Err(YangError::stage4_region_invalid(
                    v,
                    Stage4InvalidReason::LocalRefinementRequired,
                ));
            }
            let Some((n1, d1, n2, d2)) = dedup_single_pp_line(&vert_pp_planes[&v]) else {
                return Err(YangError::stage4_region_invalid(
                    v,
                    Stage4InvalidReason::LocalRefinementRequired,
                ));
            };
            let Some((lp, ld)) = pp_line(n1, d1, n2, d2) else {
                return Err(YangError::stage4_region_invalid(
                    v,
                    Stage4InvalidReason::LocalRefinementRequired,
                ));
            };
            let circ = vert_circle.remove(&v).expect("checked contains_key");
            if i1d_probe {
                let p = mesh.verts[v as usize];
                eprintln!(
                    "[i1d-classify] v{v} pre=({:.12}, {:.12}, {:.12})",
                    p.x(),
                    p.y(),
                    p.z()
                );
            }
            vert_pp_circle_junction.insert(v, ((lp, ld), circ));
        }
    }

    // Increment 5 (spec `yang_stage4_conic_triple_junction`, WIRED): a
    // vertex on ≥2 single-curve maps whose inc0 incidence dedups to EXACTLY
    // 3 distinct surfaces is NOT ambiguous — it is the unique transversal
    // common point of those surfaces (the R0017-class prism-edge ×
    // cone-lateral junction: exact on both planes, chord-inexact on the
    // cone). Relocate it onto all three via the torus-block triple primitive
    // instead of letting the over-determined audits below STOP. Newton
    // failure leaves the vertex in its maps — the audits then STOP exactly
    // as today (spec branch table). 2- or ≥4-surface configurations are
    // untouched (spec I2).
    //
    // The R0044 BUCKET (R0044, R0020, R0035): `vert_surface_pair` joins the
    // six conic maps as a curve-bearing map here. A procedural M5 surface-pair
    // curve is a curve through the vertex exactly as a conic is — it is held
    // apart from the conic bookkeeping only because it has no parameter `t`,
    // not because it is a lesser claim on the vertex. Omitting it made every
    // ellipse × surface-pair junction score `n_maps == 1`, fall out of this
    // block, and reach the surface-pair loop's `endpoint_set` guard as the
    // "out of v1 scope" endpoint-MIX STOP — while its incidence was the plain
    // 3-surface triple this block already resolves. Probed: R0044 v8
    // {cyl_A, plane_B, cone_B}, R0020 v44 {plane_A, cone_A, cyl_B}, R0035
    // v194/195 {cyl_A, cyl_B, plane_B} — every one exactly 3, every one
    // ellipse + pair. Nothing about the mix needed new machinery; the mix was
    // never the difficulty.
    let mut triple_moved: Vec<u32> = Vec::new();
    {
        let mut cand: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
        for v in vert_circle
            .keys()
            .chain(vert_ellipse.keys())
            .chain(vert_cone_ellipse.keys())
            .chain(vert_parabola.keys())
            .chain(vert_cone_hyperbola.keys())
            .chain(vert_line.keys())
            .chain(vert_surface_pair.keys())
        {
            cand.insert(*v);
        }
        for v in cand {
            let n_maps = [
                vert_circle.contains_key(&v),
                vert_ellipse.contains_key(&v),
                vert_cone_ellipse.contains_key(&v),
                vert_parabola.contains_key(&v),
                vert_cone_hyperbola.contains_key(&v),
                vert_line.contains_key(&v),
                vert_surface_pair.contains_key(&v),
            ]
            .iter()
            .filter(|b| **b)
            .count();
            // KV16: a same-type conic junction (two hyperbolas in the ONE
            // `vert_cone_hyperbola` slot) counts as multi-curve even though
            // only one map sees the vertex.
            if n_maps < 2 && !same_type_junction.contains(&v) {
                continue;
            }
            let probe_v = std::env::var_os("YANG_SAMETYPE_PROBE").is_some();
            let Some(surfs) = vert_surfs.get(&v) else {
                if probe_v {
                    eprintln!("[triple-bail] v={v} no vert_surfs entry");
                }
                continue;
            };
            if surfs.len() != 3 {
                if probe_v {
                    eprintln!("[triple-bail] v={v} surfs={}", surfs.len());
                }
                continue; // 2 / ≥4 surfaces keep the loud audits (I2)
            }
            let p = mesh.verts[v as usize];
            let Some(proj) = relocate_onto_implicit_triple(p, surfs[0], surfs[1], surfs[2]) else {
                if probe_v {
                    eprintln!("[triple-bail] v={v} newton diverged");
                }
                continue; // Newton diverged → the audits STOP loudly
            };
            let qa = proj.as_array();
            let (Some((_, n0)), Some((_, n1))) = (
                surface_value_and_normal(surfs[0], qa),
                surface_value_and_normal(surfs[1], qa),
            ) else {
                continue; // evaluation failed → the audits STOP loudly
            };
            // Derived displacement gate: a chord vertex moves to the exact
            // junction by ≤ 2·d_ε / sin θ (the torus-block metric — NOT a
            // tolerance widening). Beyond it is a real off-curve error.
            let pa = p.as_array();
            let rho = ((qa[0] - pa[0]).powi(2) + (qa[1] - pa[1]).powi(2) + (qa[2] - pa[2]).powi(2))
                .sqrt();
            let cx = [
                n0[1] * n1[2] - n0[2] * n1[1],
                n0[2] * n1[0] - n0[0] * n1[2],
                n0[0] * n1[1] - n0[1] * n1[0],
            ];
            let sin_theta = (cx[0] * cx[0] + cx[1] * cx[1] + cx[2] * cx[2]).sqrt();
            let gate = tangent_plane_corridor(d_eps, sin_theta);
            if rho > gate {
                if let Some(e) = s451_stop(s451_collect, &mut s45_failures, v, &curves0, &inc0) {
                    return Err(e);
                }
                continue;
            }
            if std::env::var_os("YANG_RIM_JUNCTION_PROBE").is_some() {
                eprintln!(
                    "[s4-triple-junction] v={v} rho={rho:.4e} gate={gate:.4e} surfs=3 relocated"
                );
            }
            // Bookkeeping (spec I3/I4): out of every single-curve map and
            // out of `endpoints` (all occurrences — one push per incident
            // curve), so the audits and the no-skip balance never see it;
            // NOT added to `processed`/`relocations` (source stays
            // `BRepVertex`, position now exact).
            vert_circle.remove(&v);
            vert_ellipse.remove(&v);
            vert_cone_ellipse.remove(&v);
            vert_parabola.remove(&v);
            vert_cone_hyperbola.remove(&v);
            vert_line.remove(&v);
            // R0044 bucket: also out of the procedural map, so the M5
            // surface-pair loop below neither re-relocates the vertex onto
            // only two of its three surfaces nor STOPs on the endpoint mix.
            // (`vert_surface_pair` verts never enter `endpoints` — a
            // procedural curve has no `t` — so the retain above is a no-op
            // for a pair-only vertex, which by `n_maps < 2` never gets here.)
            vert_surface_pair.remove(&v);
            endpoints.retain(|&u| u != v);
            if rho > cad_primitives::TAU_WORK {
                mesh.verts[v as usize] = proj;
                triple_moved.push(v);
            }
        }
    }

    // M8 disc∩disc no-skip audit (P10): a circle∩circle lens corner that is ALSO
    // on any OTHER curve type (a line, ellipse, cone conic, or line+circle
    // junction) is an over-determined junction this arm does not resolve — loud
    // STOP rather than relocate it onto only the two circles. (Cannot arise for a
    // pure disc∩disc lens, but never silently pick.)
    for v in vert_circle_junction.keys() {
        if vert_line.contains_key(v)
            || vert_ellipse.contains_key(v)
            || vert_cone_ellipse.contains_key(v)
            || vert_parabola.contains_key(v)
            || vert_cone_hyperbola.contains_key(v)
            || vert_junction.contains_key(v)
            || vert_pp_circle_junction.contains_key(v)
        {
            return Err(YangError::stage4_region_invalid(
                *v,
                Stage4InvalidReason::LocalRefinementRequired,
            ));
        }
    }

    // A vertex shared by BOTH a circle and an ellipse edge (two distinct curves
    // through one vertex) is a genuine ambiguity — relocating it twice would be
    // wrong, so loud STOP rather than silently picking one (spec §4 no-skip
    // audit / P10).
    // PR-F3: the line+circle junction is HANDLED (vert_junction above); a line
    // meeting any OTHER conic is still a loud STOP, folded into each audit.
    for v in vert_ellipse.keys() {
        if vert_circle.contains_key(v)
            || vert_line.contains_key(v)
            || vert_junction.contains_key(v)
            || vert_pp_circle_junction.contains_key(v)
        {
            return Err(YangError::stage4_region_invalid(
                *v,
                Stage4InvalidReason::LocalRefinementRequired,
            ));
        }
    }
    // PR-YR21: a vertex shared by a cone-ellipse edge AND any other conic edge
    // (cylinder-ellipse or circle) is a genuine ambiguity — loud STOP (spec
    // §3.2 / P10), the same no-skip audit extended to the cone map.
    for v in vert_cone_ellipse.keys() {
        if vert_circle.contains_key(v)
            || vert_ellipse.contains_key(v)
            || vert_line.contains_key(v)
            || vert_junction.contains_key(v)
            || vert_pp_circle_junction.contains_key(v)
        {
            return Err(YangError::stage4_region_invalid(
                *v,
                Stage4InvalidReason::LocalRefinementRequired,
            ));
        }
    }
    // PR-YR22: a vertex shared by a cone-parabola edge AND any other conic edge
    // (circle / cylinder-ellipse / cone-ellipse) is a genuine ambiguity — loud
    // STOP (P10), the same no-skip audit extended to the parabola map.
    for v in vert_parabola.keys() {
        if vert_circle.contains_key(v)
            || vert_ellipse.contains_key(v)
            || vert_cone_ellipse.contains_key(v)
            || vert_cone_hyperbola.contains_key(v)
            || vert_line.contains_key(v)
            || vert_junction.contains_key(v)
            || vert_pp_circle_junction.contains_key(v)
        {
            return Err(YangError::stage4_region_invalid(
                *v,
                Stage4InvalidReason::LocalRefinementRequired,
            ));
        }
    }
    // PR-YR23: a vertex shared by a cone-hyperbola edge AND any other conic edge
    // (circle / cylinder-ellipse / cone-ellipse / cone-parabola) is a genuine
    // ambiguity — loud STOP (P10), the same no-skip audit extended to the
    // hyperbola map.
    for v in vert_cone_hyperbola.keys() {
        if vert_circle.contains_key(v)
            || vert_ellipse.contains_key(v)
            || vert_cone_ellipse.contains_key(v)
            || vert_parabola.contains_key(v)
            || vert_line.contains_key(v)
            || vert_junction.contains_key(v)
            || vert_pp_circle_junction.contains_key(v)
        {
            return Err(YangError::stage4_region_invalid(
                *v,
                Stage4InvalidReason::LocalRefinementRequired,
            ));
        }
    }

    // (2) Relocate / retag every endpoint. `processed` is the no-skip audit set;
    // `moved` is the subset whose position actually changed (ρ > TAU_WORK) — the
    // triangles touching THOSE verts are the ones Stage-4 validation gates
    // (spec §4.5 step 4: validate per RELOCATED triangle, not pre-existing
    // arrangement slivers that `boolean()` legitimately kept for watertightness).
    // `YANG_V_PROBE=<ids>` selects by Stage-4 vertex id;
    // `YANG_V_PROBE_NEAR=x,y,z,r` additionally selects every vertex within
    // `r` of a position (Stage-4 ids are renumbered by the later
    // compactions, so a probe driven from an OUTPUT-side report — e.g.
    // kernel-v2's `KV_ELLIPSE_PROBE` — has only the position to go on).
    let mut v_probe_ids: Vec<u32> = std::env::var("YANG_V_PROBE")
        .ok()
        .map(|list| {
            list.split(',')
                .filter_map(|t| t.trim().parse::<u32>().ok())
                .collect()
        })
        .unwrap_or_default();
    if let Ok(spec) = std::env::var("YANG_V_PROBE_NEAR") {
        let nums: Vec<f64> = spec
            .split(',')
            .filter_map(|t| t.trim().parse::<f64>().ok())
            .collect();
        if nums.len() == 4 {
            for (vi, q) in mesh.verts.iter().enumerate() {
                let qa = q.as_array();
                let d2 = (qa[0] - nums[0]).powi(2)
                    + (qa[1] - nums[1]).powi(2)
                    + (qa[2] - nums[2]).powi(2);
                if d2 <= nums[3] * nums[3] {
                    v_probe_ids.push(vi as u32);
                }
            }
        }
    }
    if !v_probe_ids.is_empty() {
        for v in v_probe_ids {
            let inc_curves: Vec<String> = curves0
                .iter()
                .filter(|(&(s, e), _)| s == v || e == v)
                .map(|(&(s, e), c)| {
                    let n = match c {
                        Curve::LineSegment => "Line",
                        Curve::Circle { .. } => "Circle",
                        Curve::Ellipse { .. } => "Ellipse",
                        Curve::Parabola { .. } => "Parabola",
                        Curve::Hyperbola { .. } => "Hyperbola",
                        Curve::SurfacePair { .. } => "SurfacePair",
                    };
                    format!("({s},{e}):{n}")
                })
                .collect();
            eprintln!(
                "YANG_V_PROBE v={v} same_type_junction={} exact_junction={} incident_curves=[{}]",
                same_type_junction.contains(&v),
                exact_junctions.contains(&v),
                inc_curves.join(",")
            );
            if let Some(er) = vert_ellipse.get(&v) {
                eprintln!(
                    "YANG_V_PROBE v={v} er plane_n={:?} plane_d={:.17e} center={:?} \
                     normal={:?} major_axis={:?} a={:.17e} b={:.17e} second_cyl={:?}",
                    er.plane_n,
                    er.plane_d,
                    er.center,
                    er.normal,
                    er.major_axis,
                    er.major_radius,
                    er.minor_radius,
                    er.second_cyl,
                );
            }
            // `torus` / `surface_pair` are the two `inc0`-driven implicit-pair
            // relocation paths (the KV6d Tier B torus block and the M5
            // surface-pair block, both AFTER the conic audit). They do NOT
            // populate the conic `vert_*` maps above, so a vertex handled by
            // them shows every conic flag `false`. Print them here so a reader
            // never mistakes "all conic flags false" for "unhandled" — the
            // exact trap that produced the wrong "#137 missing solver" reframe.
            // `torus` mirrors the block's own detection: an `inc0` edge incident
            // to `v` whose attributed surfaces include a `Torus`.
            let torus_v = inc0.iter().any(|(&(s, e), entries)| {
                (s == v || e == v)
                    && entries
                        .iter()
                        .any(|(_i, surf)| matches!(surf, Surface::Torus { .. }))
            });
            eprintln!(
                "YANG_V_PROBE v={v} p={:?} circle={} ellipse={} cone_ell={} parab={} hyp={} \
                 line={} ell_junction={} circle_junction={} line_circle_junction={} \
                 pp_planes={} pp_circle_junction={} endpoint={} torus={torus_v} surface_pair={}",
                mesh.verts.get(v as usize),
                vert_circle.contains_key(&v),
                vert_ellipse.contains_key(&v),
                vert_cone_ellipse.contains_key(&v),
                vert_parabola.contains_key(&v),
                vert_cone_hyperbola.contains_key(&v),
                vert_line.contains_key(&v),
                vert_ell_junction.contains_key(&v),
                vert_circle_junction.contains_key(&v),
                vert_junction.contains_key(&v),
                vert_pp_planes.contains_key(&v),
                vert_pp_circle_junction.contains_key(&v),
                endpoints.contains(&v),
                vert_surface_pair.contains_key(&v),
            );
        }
    }
    let mut processed: HashSet<u32> = HashSet::new();
    let mut moved: HashSet<u32> = HashSet::new();
    // Increment 5: triple-junction relocations count as moved (their
    // incident triangles get the Stage-4 fold validation) but are NOT in
    // `processed`/`relocations` — the no-skip audit balance is untouched
    // because they left `endpoints` too (spec I3).
    moved.extend(triple_moved.iter().copied());
    let mut relocations: Vec<(u32, f64)> = Vec::new();
    // Deterministic order: BTreeMap iteration.
    for (&v, &(center, normal, radius, src_r)) in &vert_circle {
        let p = mesh.verts[v as usize];
        // PR-YR19 (spec §4 Site 2): split the residual so the in-plane RADIAL
        // band is the propagated `(R/r_c)·d_ε` for a sphere section circle while
        // the AXIAL band stays `d_ε`. For `None`/non-sphere this is identical to
        // `max(axial, radial_dev) > d_eps`, i.e. byte-identical to the prior
        // `circle_residual > d_eps`. Near-tangent (`radius ≤ MIN_FEATURE_SIZE`)
        // fails closed (keeps the unscaled band).
        let (axial, radial_dev) = circle_residual_split(p, center, normal, radius);
        let radial_band = match src_r {
            Some(big_r) if radius > cad_primitives::MIN_FEATURE_SIZE => (big_r / radius) * d_eps,
            _ => d_eps,
        };
        if (axial > d_eps || radial_dev > radial_band) && !prov_verts.contains(&v) {
            // (Provenance-vouched vertices are exempt — spec inc-2 §3c: the
            // assignment is producer-confirmed, and `project_onto_circle`
            // below is already the distance-minimizing projection onto the
            // exact curve, a certificate the band cannot strengthen.)
            if let Some(e) = s451_stop(s451_collect, &mut s45_failures, v, &curves0, &inc0) {
                return Err(e);
            }
            continue;
        }
        // Preserve the original combined-max `rho` for the `> TAU_WORK`
        // move-gate so its semantics are unchanged.
        let rho = axial.max(radial_dev);
        // Always project to obtain the circle-frame angle `t` (and the exact
        // on-curve position). For ρ ≤ TAU_WORK the projection is a no-op move
        // but still yields the retag `t`; for the relocate band it moves the
        // vertex onto the curve.
        let (proj, t) = project_onto_circle(p, center, normal, radius)
            .map_err(|reason| YangError::stage4_region_invalid(v, reason))?;
        if rho > cad_primitives::TAU_WORK {
            mesh.verts[v as usize] = proj;
            moved.insert(v);
        }
        probe_push(line!(), v, t, mesh.verts[v as usize]);
        relocations.push((v, t));
        processed.insert(v);
    }

    // M8 disc∩disc CROSSING: relocate each lens-corner vertex onto the EXACT
    // circle∩circle intersection (on BOTH coplanar circles). The vertex sits on
    // a Stage-1 chord, off each circle radially by ≤ d_eps; the displacement to
    // the exact corner is amplified by `1/sin θ`, θ = angle between the two
    // circles' radial directions at the corner (the same derived gradient metric
    // as the cyl×cyl ellipse junction — NOT tolerance widening). A grazing/
    // tangent crossing (θ → 0) has no well-defined corner and `coplanar_circle_
    // circle_intersection` returns `None` → loud STOP.
    for (&v, &(ca, cb)) in &vert_circle_junction {
        let p = mesh.verts[v as usize];
        let (c_a, n_a, r_a, _) = ca;
        let (c_b, n_b, r_b, _) = cb;
        let Some(j) = coplanar_circle_circle_intersection(c_a, n_a, r_a, c_b, n_b, r_b, p) else {
            return Err(YangError::stage4_region_invalid(
                v,
                Stage4InvalidReason::LocalRefinementRequired,
            ));
        };
        let pa = p.as_array();
        let ja = j.as_array();
        let rho =
            ((ja[0] - pa[0]).powi(2) + (ja[1] - pa[1]).powi(2) + (ja[2] - pa[2]).powi(2)).sqrt();
        // sin θ = |r̂_a × r̂_b| at the corner (both radial vectors are in-plane).
        let ra_v = [ja[0] - c_a.x(), ja[1] - c_a.y(), ja[2] - c_a.z()];
        let rb_v = [ja[0] - c_b.x(), ja[1] - c_b.y(), ja[2] - c_b.z()];
        let ra_h = normalize3(ra_v);
        let rb_h = normalize3(rb_v);
        let cr = [
            ra_h[1] * rb_h[2] - ra_h[2] * rb_h[1],
            ra_h[2] * rb_h[0] - ra_h[0] * rb_h[2],
            ra_h[0] * rb_h[1] - ra_h[1] * rb_h[0],
        ];
        let sin_theta = (cr[0] * cr[0] + cr[1] * cr[1] + cr[2] * cr[2]).sqrt();
        let gate = tangent_plane_corridor(d_eps, sin_theta);
        if rho > gate && !prov_verts.contains(&v) {
            // (Provenance-vouched exemption — spec inc-2 §3c; `j` is the
            // exact circle∩circle corner on both curves by construction.)
            if let Some(e) = s451_stop(s451_collect, &mut s45_failures, v, &curves0, &inc0) {
                return Err(e);
            }
            continue;
        }
        // `j` is on circle_a by construction; project to get its frame angle `t`
        // for the source retag (positionally exact on both circles either way).
        let (proj, t) = project_onto_circle(j, c_a, n_a, r_a)
            .map_err(|reason| YangError::stage4_region_invalid(v, reason))?;
        if rho > cad_primitives::TAU_WORK {
            mesh.verts[v as usize] = proj;
            moved.insert(v);
        }
        probe_push(line!(), v, t, mesh.verts[v as usize]);
        relocations.push((v, t));
        processed.insert(v);
    }

    // PR-YR11: ellipse relocation loop, mirroring the circle loop above. Closed
    // form via the cylinder parameterization (spec §2). Same `d_eps` chord band.
    for (&v, er) in &vert_ellipse {
        let p = mesh.verts[v as usize];
        let rho = ellipse_residual(p, er);
        // PR-KV9: cylinder×cylinder sections gate against the per-point
        // gradient band (combined budget × 1/sin α); at tangency grade the
        // metric is unbounded and the Stage-3 surface-membership gate is
        // the backstop. The cylinder×plane path keeps the global d_ε
        // byte-for-byte.
        let gate = match er.second_cyl {
            Some((ap2, ad2, budget)) => {
                cyl_cyl_point_amplification(p, (er.axis_point, er.axis_dir), (ap2, ad2))
                    .map_or(f64::INFINITY, |amp| amp * budget)
            }
            None => d_eps,
        };
        if rho > gate {
            if prov_verts.contains(&v) {
                // Provenance-vouched relocation obligation (spec inc-2 §3c):
                // the producer confirmed this vertex's edge lies on the
                // pair's intersection curve, so the band's wrong-assignment
                // role is covered and the vertex is a DRIFTED intersection
                // vertex — this move IS the §4.4.1 correction the witness
                // selection created. Take the distance-minimizing projection
                // onto the exact ellipse (on cylinder∩plane by construction
                // — a certificate, not a band), bypassing the azimuth path
                // whose tangential amplification is meaningless this far off.
                let (proj, t) = project_onto_ellipse_nearest(p, er)
                    .map_err(|reason| YangError::stage4_region_invalid(v, reason))?;
                if std::env::var("KV11_PROBE").is_ok() {
                    eprintln!(
                        "KV11_PROBE ellipse provenance reloc: v={v} rho={rho:.3e} \
                         gate={gate:.3e} p={p:?} proj={proj:?}"
                    );
                }
                mesh.verts[v as usize] = proj;
                moved.insert(v);
                probe_push(line!(), v, t, mesh.verts[v as usize]);
                relocations.push((v, t));
                processed.insert(v);
                continue;
            }
            if std::env::var("KV11_PROBE").is_ok() {
                eprintln!(
                    "KV11_PROBE ellipse band reject: v={v} rho={rho:.3e} gate={gate:.3e} p={p:?}"
                );
            }
            if let Some(e) = s451_stop(s451_collect, &mut s45_failures, v, &curves0, &inc0) {
                return Err(e);
            }
            continue;
        }
        let (proj, t) = project_onto_ellipse_via_cylinder(p, er)
            .map_err(|reason| YangError::stage4_region_invalid(v, reason))?;
        // Task #145 mechanism 2 (spec `yang_453_mixed_cycle_conic_backtrack`
        // §3b, I6): the azimuth projection amplifies by 1/(n·â) ALONG a
        // near-tangent section — a §4.4.1 relocation is bounded by the same
        // band the ρ gate uses. Move within band → keep the closed form
        // byte-identically (R1); beyond → in-plane nearest point (R2);
        // still beyond → loud STOP (R3), never a silent macro slide.
        let move_len = |q: Point3| -> f64 {
            let qa = q.as_array();
            let pa = p.as_array();
            ((qa[0] - pa[0]).powi(2) + (qa[1] - pa[1]).powi(2) + (qa[2] - pa[2]).powi(2)).sqrt()
        };
        let (proj, t) = if er.second_cyl.is_some() || move_len(proj) <= gate {
            // R1 (and the cyl×cyl arm, whose per-point-amplified `gate`
            // already carries the KV9 gradient machinery): byte-identical
            // closed-form azimuth projection.
            (proj, t)
        } else {
            let (near_proj, near_t) = project_onto_ellipse_nearest(p, er)
                .map_err(|reason| YangError::stage4_region_invalid(v, reason))?;
            // R2/R3 budget: the vertex's surface residuals are ≤ `gate` each,
            // and distance-to-curve amplifies by 1/sin θ (θ = angle between
            // the two surface normals AT the relocated point) — the same
            // derived gradient-band the circle-junction and pp-plane gates
            // use (never widening). Evaluated at `near_proj`, where the
            // transversality of the accepted position is what matters.
            let budget = {
                let np = near_proj.as_array();
                let q = er.axis_point.as_array();
                let a_hat = normalize3(er.axis_dir.as_array());
                let w = [np[0] - q[0], np[1] - q[1], np[2] - q[2]];
                let along = w[0] * a_hat[0] + w[1] * a_hat[1] + w[2] * a_hat[2];
                let radial = normalize3([
                    w[0] - along * a_hat[0],
                    w[1] - along * a_hat[1],
                    w[2] - along * a_hat[2],
                ]);
                let n_pl = normalize3(er.plane_n.as_array());
                let cr = [
                    radial[1] * n_pl[2] - radial[2] * n_pl[1],
                    radial[2] * n_pl[0] - radial[0] * n_pl[2],
                    radial[0] * n_pl[1] - radial[1] * n_pl[0],
                ];
                let sin_theta = (cr[0] * cr[0] + cr[1] * cr[1] + cr[2] * cr[2]).sqrt();
                // Exact tangency → unbounded corridor (see
                // `tangent_plane_corridor`); the projection is still the
                // local nearest point.
                tangent_plane_corridor(gate, sin_theta)
            };
            if std::env::var_os("YANG_T145_RELOC_PROBE").is_some() {
                eprintln!(
                    "[t145-reloc] v={v} rho={rho:.3e} gate={gate:.3e} budget={budget:.3e} \
                     az_move={:.3e} near_move={:.3e} p={p:?} az={proj:?} near={near_proj:?}",
                    move_len(proj),
                    move_len(near_proj),
                );
            }
            if move_len(near_proj) > budget {
                if let Some(e) = s451_stop(s451_collect, &mut s45_failures, v, &curves0, &inc0) {
                    return Err(e);
                }
                continue;
            }
            (near_proj, near_t)
        };
        if rho > cad_primitives::TAU_WORK {
            mesh.verts[v as usize] = proj;
            moved.insert(v);
        }
        probe_push(line!(), v, t, mesh.verts[v as usize]);
        relocations.push((v, t));
        processed.insert(v);
    }

    // PR-KV9: ellipse×ellipse JUNCTION relocation. The exact junction lies
    // on `(plane₁ ∩ plane₂) ∩ cylinder` (the crossing point of the two
    // Steinmetz sections — on the cylinder and in BOTH cutting planes,
    // hence on both ellipses). The plane–plane line is exact; intersecting
    // it with the relocation cylinder is a quadratic with ≤ 2 roots; the
    // root nearest the current vertex is the junction (the two crossing
    // points are 2r apart — far outside any chord band, so nearest-pick is
    // deterministic and unambiguous). Gate at 2·d_ε (each constituent
    // membership is within its own propagated band; the junction inherits
    // both, mirroring the line+circle junction's derivation).
    for (&v, &(e_a, e_b)) in &vert_ell_junction {
        let p = mesh.verts[v as usize];
        let n1 = normalize3(e_a.plane_n.as_array());
        let n2 = normalize3(e_b.plane_n.as_array());
        let dir = [
            n1[1] * n2[2] - n1[2] * n2[1],
            n1[2] * n2[0] - n1[0] * n2[2],
            n1[0] * n2[1] - n1[1] * n2[0],
        ];
        let dl = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
        if dl < cad_primitives::MIN_FEATURE_SIZE {
            return Err(YangError::stage4_region_invalid(
                v,
                Stage4InvalidReason::LocalRefinementRequired,
            ));
        }
        let d = [dir[0] / dl, dir[1] / dl, dir[2] / dl];
        // A point on both planes: solve n1·x = −d1, n2·x = −d2 in the span
        // of {n1, n2} (x = α·n1 + β·n2; Gram system with g = n1·n2).
        let g = n1[0] * n2[0] + n1[1] * n2[1] + n1[2] * n2[2];
        let det = 1.0 - g * g;
        if det.abs() < cad_primitives::MIN_FEATURE_SIZE {
            return Err(YangError::stage4_region_invalid(
                v,
                Stage4InvalidReason::LocalRefinementRequired,
            ));
        }
        let (r1, r2) = (-e_a.plane_d, -e_b.plane_d);
        let alpha = (r1 - g * r2) / det;
        let beta = (r2 - g * r1) / det;
        let p0 = [
            alpha * n1[0] + beta * n2[0],
            alpha * n1[1] + beta * n2[1],
            alpha * n1[2] + beta * n2[2],
        ];
        // Intersect the line p0 + t·d with the relocation cylinder of e_a.
        let ax = normalize3(e_a.axis_dir.as_array());
        let ap = e_a.axis_point.as_array();
        let rel = [p0[0] - ap[0], p0[1] - ap[1], p0[2] - ap[2]];
        let perp = |w: [f64; 3]| -> [f64; 3] {
            let h = w[0] * ax[0] + w[1] * ax[1] + w[2] * ax[2];
            [w[0] - h * ax[0], w[1] - h * ax[1], w[2] - h * ax[2]]
        };
        let rp = perp(rel);
        let dp = perp(d);
        let aa = dp[0] * dp[0] + dp[1] * dp[1] + dp[2] * dp[2];
        let bb = 2.0 * (rp[0] * dp[0] + rp[1] * dp[1] + rp[2] * dp[2]);
        let cc = rp[0] * rp[0] + rp[1] * rp[1] + rp[2] * rp[2] - e_a.radius * e_a.radius;
        let disc = bb * bb - 4.0 * aa * cc;
        if !(aa > cad_primitives::MIN_FEATURE_SIZE && disc >= 0.0) {
            return Err(YangError::stage4_region_invalid(
                v,
                Stage4InvalidReason::LocalRefinementRequired,
            ));
        }
        let sq = disc.sqrt();
        let pa = p.as_array();
        let mut best: Option<([f64; 3], f64)> = None;
        for t in [(-bb - sq) / (2.0 * aa), (-bb + sq) / (2.0 * aa)] {
            let x = [p0[0] + t * d[0], p0[1] + t * d[1], p0[2] + t * d[2]];
            let dd =
                ((x[0] - pa[0]).powi(2) + (x[1] - pa[1]).powi(2) + (x[2] - pa[2]).powi(2)).sqrt();
            if best.map(|(_, b)| dd < b).unwrap_or(true) {
                best = Some((x, dd));
            }
        }
        let (j, rho) = best.expect("two real roots checked");
        // PR-KV11: the vertex moves ALONG the junction line to reach the
        // cylinder, so its radial chord residual (≤ the combined band) is
        // amplified by `1/|d̂·r̂|` — the directional derivative of the
        // radial distance along the line at the junction (the same derived
        // metric propagation as the KV9 cyl×cyl `1/sin α` gradient band; a
        // grazing line ⇒ unbounded metric, backstopped by the Stage-3
        // surface-membership gates, mirroring the cyl×cyl arm).
        let rel_j = [j[0] - ap[0], j[1] - ap[1], j[2] - ap[2]];
        let rp_j = perp(rel_j);
        let rp_j_len = (rp_j[0] * rp_j[0] + rp_j[1] * rp_j[1] + rp_j[2] * rp_j[2]).sqrt();
        let grad = if rp_j_len > 0.0 {
            ((d[0] * rp_j[0] + d[1] * rp_j[1] + d[2] * rp_j[2]) / rp_j_len).abs()
        } else {
            0.0
        };
        // KV9-F1 E-L2 (spec §2c, branch row J1): a junction of two sections of
        // the SAME unordered cylinder pair is ALWAYS the pair's surface-tangency
        // point (the two decomposition planes intersect in the line through both
        // tangency points; that line meets the cylinder exactly where the two
        // radial gradients align). There the vertex is the PINCH of the two
        // faceted-surface intersection polylines, whose standoff from the exact
        // crossing is SECOND-order-controlled: in tangent-plane coordinates the
        // cylinders are the graphs y = r − x²/2r and y = r − z²/2r; facet
        // displacements a ∈ [0, ε_A], b ∈ [0, ε_B] perturb the intersection to
        // the hyperbola x² − z² = 2r(b−a), standoff √(2r·|b−a|) ≤ √(2r·B) with
        // B the combined chord budget carried by `second_cyl`, plus ≤ B
        // normal-direction offset. A derived metric conversion (the
        // single-ellipse arm's 1/sin α analog at tangency grade), NOT tolerance
        // widening — the relocation target stays the EXACT junction. Every
        // other junction (row J2 — the KV11 box-edge class) keeps the
        // first-order 2·d_ε/|d̂·r̂| line metric byte-identical.
        let same_pair_budget = match (e_a.second_cyl, e_b.second_cyl) {
            (Some((sa_p, sa_d, ba)), Some((sb_p, sb_d, bb))) => {
                let same = e_a.axis_point.as_array() == e_b.axis_point.as_array()
                    && e_a.axis_dir.as_array() == e_b.axis_dir.as_array()
                    && sa_p.as_array() == sb_p.as_array()
                    && sa_d.as_array() == sb_d.as_array();
                if same {
                    Some(ba.max(bb))
                } else {
                    None
                }
            }
            _ => None,
        };
        let gate = if let Some(budget) = same_pair_budget {
            (2.0 * e_a.radius * budget).sqrt() + budget
        } else {
            tangent_plane_corridor(d_eps, grad)
        };
        // KV9-F1 Increment 0c census: per-junction second_cyl provenance +
        // first-order gate state (kept env-gated, like the other Stage-4 probes).
        if std::env::var("KV9_JUNCTION_PROBE").is_ok() {
            eprintln!(
                "KV9_JUNCTION_PROBE v={v} p={p:?} j={j:?} rho={rho:.4e} grad={grad:.4e} \
                 gate={gate:.4e} d_eps={d_eps:.4e} \
                 a_axis=({:?},{:?}) a_second={:?} b_axis=({:?},{:?}) b_second={:?}",
                e_a.axis_point.as_array(),
                e_a.axis_dir.as_array(),
                e_a.second_cyl
                    .map(|(sp, sd, bud)| (sp.as_array(), sd.as_array(), bud)),
                e_b.axis_point.as_array(),
                e_b.axis_dir.as_array(),
                e_b.second_cyl
                    .map(|(sp, sd, bud)| (sp.as_array(), sd.as_array(), bud)),
            );
        }
        if rho > gate && !prov_verts.contains(&v) {
            // (Provenance-vouched vertices are exempt from this displacement
            // gate — spec inc-2 §3c: their assignment is producer-confirmed
            // and the destination `j` is the EXACT nearest-root junction on
            // all defining surfaces, a certificate the displacement
            // magnitude cannot strengthen.)
            if std::env::var("KV11_PROBE").is_ok() {
                eprintln!(
                    "KV11_PROBE junction band reject: v={v} rho={rho:.3e} gate={gate:.3e} p={p:?} j={j:?}"
                );
            }
            if let Some(e) = s451_stop(s451_collect, &mut s45_failures, v, &curves0, &inc0) {
                return Err(e);
            }
            continue;
        }
        let proj = Point3::new(j[0], j[1], j[2]);
        // Param on e_a's ellipse for the source retag (output edges of BOTH
        // ellipses touch this vertex; the position is exact on both, so the
        // retag curve choice is positional-exact either way).
        let t = ellipse_param(
            proj,
            e_a.center,
            e_a.normal,
            e_a.major_axis,
            e_a.major_radius,
            e_a.minor_radius,
        );
        if rho > cad_primitives::TAU_WORK {
            mesh.verts[v as usize] = proj;
            moved.insert(v);
        }
        probe_push(line!(), v, t, mesh.verts[v as usize]);
        relocations.push((v, t));
        processed.insert(v);
    }

    // LRR diagnostic (read-only): unified over-band run-structure across ALL
    // conic relocation maps. Per curve identity, sort by parameter `t`, flag
    // over-band vertices, and classify each over-band vertex as INTERIOR-bounded
    // (a within-band vertex exists at both a smaller AND larger t on the same
    // curve — the paper §4.5.1 condition) vs END/UNBOUNDED. Emits a per-case
    // verdict on whether EVERY over-band conic vertex is interior-bounded.
    if std::env::var_os("YANG_LRR_PROBE").is_some() {
        // Per (curve-kind, curve-identity) → sorted samples `(t, v, rho, band)`.
        type LrrGroups = BTreeMap<(&'static str, [u64; 3]), Vec<(f64, u32, f64, f64)>>;
        let mut groups: LrrGroups = BTreeMap::new();
        let kb = |p: Point3| [p.x().to_bits(), p.y().to_bits(), p.z().to_bits()];
        let mut push = |kind: &'static str, key: [u64; 3], t: f64, v: u32, rho: f64, band: f64| {
            groups
                .entry((kind, key))
                .or_default()
                .push((t, v, rho, band));
        };
        for (&v, &(center, normal, radius, src_r)) in &vert_circle {
            let p = mesh.verts[v as usize];
            let (axial, radial_dev) = circle_residual_split(p, center, normal, radius);
            let band = match src_r {
                Some(big_r) if radius > cad_primitives::MIN_FEATURE_SIZE => {
                    (big_r / radius) * d_eps
                }
                _ => d_eps,
            };
            let t = project_onto_circle(p, center, normal, radius)
                .map(|(_, t)| t)
                .unwrap_or(0.0);
            push(
                "circle",
                kb(center),
                t,
                v,
                axial.max(radial_dev),
                band.max(d_eps),
            );
        }
        for (&v, er) in &vert_ellipse {
            let p = mesh.verts[v as usize];
            push(
                "ellipse",
                kb(er.center),
                0.0,
                v,
                ellipse_residual(p, er),
                d_eps,
            );
        }
        for (&v, cer) in &vert_cone_ellipse {
            let p = mesh.verts[v as usize];
            let t = ellipse_param(
                p,
                cer.center,
                cer.normal,
                cer.major_axis,
                cer.major_radius,
                cer.minor_radius,
            );
            push(
                "cone_ell",
                kb(cer.center),
                t,
                v,
                cone_ellipse_residual(p, cer),
                cer.cone_d_eps,
            );
        }
        for (&v, cpr) in &vert_parabola {
            let p = mesh.verts[v as usize];
            let rho = cone_plane_residual(
                p,
                cpr.apex,
                cpr.cone_axis_dir,
                cpr.half_angle,
                cpr.plane_n,
                cpr.plane_d,
            );
            push("parabola", kb(cpr.vertex), 0.0, v, rho, cpr.cone_d_eps);
        }
        for (&v, chr) in &vert_cone_hyperbola {
            let p = mesh.verts[v as usize];
            let rho = cone_plane_residual(
                p,
                chr.apex,
                chr.cone_axis_dir,
                chr.half_angle,
                chr.plane_n,
                chr.plane_d,
            );
            push("hyperbola", kb(chr.apex), 0.0, v, rho, chr.cone_d_eps);
        }
        let mut all_interior = true;
        let mut n_over = 0usize;
        for ((kind, _key), list) in &mut groups {
            list.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
            let within: Vec<f64> = list.iter().filter(|r| r.2 <= r.3).map(|r| r.0).collect();
            let (tmin, tmax) = (
                within.iter().cloned().fold(f64::INFINITY, f64::min),
                within.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
            );
            let mut seq = String::new();
            for (t, _v, rho, band) in list.iter() {
                if rho > band {
                    n_over += 1;
                    let interior = *t > tmin && *t < tmax;
                    if !interior {
                        all_interior = false;
                    }
                    seq.push(if interior { 'I' } else { 'E' });
                } else {
                    seq.push('.');
                }
            }
            if seq.contains('I') || seq.contains('E') {
                eprintln!(
                    "YANG_LRR_PROBE {kind} n={} within={} seq={seq}",
                    list.len(),
                    within.len()
                );
            }
        }
        eprintln!(
            "YANG_LRR_VERDICT n_over={n_over} all_over_band_interior_bounded={}",
            n_over > 0 && all_interior
        );
    }

    // PR-YR21: cone-ellipse relocation loop, mirroring the cylinder-ellipse loop.
    // Closed form via the cone GENERATOR parameterization (spec §3.1). Gated
    // against the cone's OWN chord budget `cone_d_eps` (NOT the rim-AABB `d_eps`)
    // so a tall-thin cone's residual is checked against the honest cone bound.
    for (&v, cer) in &vert_cone_ellipse {
        let p = mesh.verts[v as usize];
        let rho = cone_ellipse_residual(p, cer);
        if rho > cer.cone_d_eps {
            if let Some(e) = s451_stop(s451_collect, &mut s45_failures, v, &curves0, &inc0) {
                return Err(e);
            }
            continue;
        }
        let proj = project_onto_cone_section(
            p,
            cer.apex,
            cer.axis_dir,
            cer.half_angle,
            cer.plane_n,
            cer.plane_d,
        )
        .map_err(|reason| YangError::stage4_region_invalid(v, reason))?;
        // Round-trip param `t` in the stored ellipse frame so the unchanged
        // `eval_source` Ellipse arm reproduces the relocated position.
        let t = ellipse_param(
            proj,
            cer.center,
            cer.normal,
            cer.major_axis,
            cer.major_radius,
            cer.minor_radius,
        );
        if rho > cad_primitives::TAU_WORK {
            mesh.verts[v as usize] = proj;
            moved.insert(v);
        }
        probe_push(line!(), v, t, mesh.verts[v as usize]);
        relocations.push((v, t));
        processed.insert(v);
    }

    // PR-YR22: cone-parabola relocation loop, mirroring the cone-ellipse loop.
    // Closed form via the cone GENERATOR parameterization (the section TYPE does
    // not change the relocation — `project_onto_cone_section` is type-agnostic;
    // its `s ≤ 0` / generator-parallel guards correctly reject the out-of-scope
    // parabola tail, which the fixture's finite arc avoids). Gated against the
    // cone's OWN chord budget `cone_d_eps`.
    for (&v, cpr) in &vert_parabola {
        let p = mesh.verts[v as usize];
        let rho = cone_plane_residual(
            p,
            cpr.apex,
            cpr.cone_axis_dir,
            cpr.half_angle,
            cpr.plane_n,
            cpr.plane_d,
        );
        if rho > cpr.cone_d_eps {
            if let Some(e) = s451_stop(s451_collect, &mut s45_failures, v, &curves0, &inc0) {
                return Err(e);
            }
            continue;
        }
        let proj = project_onto_cone_section(
            p,
            cpr.apex,
            cpr.cone_axis_dir,
            cpr.half_angle,
            cpr.plane_n,
            cpr.plane_d,
        )
        .map_err(|reason| YangError::stage4_region_invalid(v, reason))?;
        // Round-trip param `t` = the conjugate-axis coordinate of the parabola
        // parameterization `(proj − vertex)·(normal × axis_dir)`, so the unchanged
        // `eval_source` Parabola arm reproduces the relocated position (oracle3).
        let n = normalize3(cpr.normal.as_array());
        let ax = normalize3(cpr.para_axis_dir.as_array());
        let conj = [
            n[1] * ax[2] - n[2] * ax[1],
            n[2] * ax[0] - n[0] * ax[2],
            n[0] * ax[1] - n[1] * ax[0],
        ];
        let vtx = cpr.vertex.as_array();
        let pr = proj.as_array();
        let t =
            (pr[0] - vtx[0]) * conj[0] + (pr[1] - vtx[1]) * conj[1] + (pr[2] - vtx[2]) * conj[2];
        if rho > cad_primitives::TAU_WORK {
            mesh.verts[v as usize] = proj;
            moved.insert(v);
        }
        probe_push(line!(), v, t, mesh.verts[v as usize]);
        relocations.push((v, t));
        processed.insert(v);
    }

    // PR-YR23: cone-hyperbola relocation loop, mirroring the cone-parabola loop.
    // Closed form via the same type-agnostic cone GENERATOR parameterization
    // (`project_onto_cone_section`); its `s ≤ 0` / generator-parallel guards
    // correctly reject the out-of-scope asymptote, which the fixture's finite arc
    // avoids. Gated against the cone's OWN chord budget `cone_d_eps`.
    for (&v, chr) in &vert_cone_hyperbola {
        let p = mesh.verts[v as usize];
        let rho = cone_plane_residual(
            p,
            chr.apex,
            chr.cone_axis_dir,
            chr.half_angle,
            chr.plane_n,
            chr.plane_d,
        );
        if rho > chr.cone_d_eps {
            if let Some(e) = s451_stop(s451_collect, &mut s45_failures, v, &curves0, &inc0) {
                return Err(e);
            }
            continue;
        }
        let proj = project_onto_cone_section(
            p,
            chr.apex,
            chr.cone_axis_dir,
            chr.half_angle,
            chr.plane_n,
            chr.plane_d,
        )
        .map_err(|reason| YangError::stage4_region_invalid(v, reason))?;
        // Round-trip param `t = asinh(v_coord / b)` where `v_coord` is the
        // conjugate-axis coordinate `(proj − center)·(normal × major_axis)` and
        // `b = semi_conjugate`. The eval is
        // `center + a·cosh(t)·major + b·sinh(t)·(normal×major)`, so
        // `v_coord = b·sinh(t) ⇒ t = asinh(v_coord/b)` (sinh is the bijective
        // coordinate; well-defined ∀ v_coord). The unchanged `eval_source`
        // Hyperbola arm reproduces the relocated position (oracle3).
        let n = normalize3(chr.normal.as_array());
        let maj = normalize3(chr.major_axis.as_array());
        let conj = [
            n[1] * maj[2] - n[2] * maj[1],
            n[2] * maj[0] - n[0] * maj[2],
            n[0] * maj[1] - n[1] * maj[0],
        ];
        let ctr = chr.center.as_array();
        let pr = proj.as_array();
        let v_coord =
            (pr[0] - ctr[0]) * conj[0] + (pr[1] - ctr[1]) * conj[1] + (pr[2] - ctr[2]) * conj[2];
        let t = (v_coord / chr.semi_conjugate).asinh();
        if rho > cad_primitives::TAU_WORK {
            mesh.verts[v as usize] = proj;
            moved.insert(v);
        }
        probe_push(line!(), v, t, mesh.verts[v as usize]);
        relocations.push((v, t));
        processed.insert(v);
    }

    // PR-F3: ruling-line relocation loop. The residual is the perpendicular
    // distance to the exact line (the sagitta of the Stage-1 facet chord the
    // arrangement point sits on), gated at the same global `d_eps` band as the
    // circle loop. The relocated position is the foot of the perpendicular —
    // exactly on the line, hence exactly on BOTH the cutting plane and the
    // cylinder. `t` is the along-line parameter; no conic OUTPUT edge claims a
    // line vertex in `emit_topology`, so its source stays `BRepVertex` and
    // `eval_source` returns the relocated mesh position directly.
    for (&v, lr) in &vert_line {
        let p = mesh.verts[v as usize];
        let rho = line_perp_distance(p, lr.point, lr.dir);
        // PR-F3b/PR-KV9: the residual is the line-distance metric, so the
        // gate is the ABSOLUTE propagated budget computed at collection (the
        // owner chord band(s) converted into this metric) — not the raw
        // radial band, and not the global d_ε (whose owner mix is wrong for
        // cylinder×cylinder lines).
        if rho > lr.band_budget {
            if let Some(e) = s451_stop(s451_collect, &mut s45_failures, v, &curves0, &inc0) {
                return Err(e);
            }
            continue;
        }
        let d = normalize3(lr.dir.as_array());
        let pt = lr.point.as_array();
        let x = p.as_array();
        let w = [x[0] - pt[0], x[1] - pt[1], x[2] - pt[2]];
        let along = w[0] * d[0] + w[1] * d[1] + w[2] * d[2];
        let proj = Point3::new(
            pt[0] + along * d[0],
            pt[1] + along * d[1],
            pt[2] + along * d[2],
        );
        if rho > cad_primitives::TAU_WORK {
            mesh.verts[v as usize] = proj;
            moved.insert(v);
        }
        probe_push(line!(), v, along, mesh.verts[v as usize]);
        relocations.push((v, along));
        processed.insert(v);
    }

    // PR-F3: line+circle JUNCTION relocation loop. The exact junction is
    // `line ∩ plane-of-circle` (which lies exactly on the circle, since the
    // line is on the cylinder and the circle is cylinder ∩ circle-plane). The
    // residual gate is `2·d_eps`: the vertex is off the line radially by ≤ one
    // sagitta AND off the circle plane along the line by ≤ another
    // sagitta-order term (it sits on the crossing of the cutting plane with a
    // rim-chord facet edge), so the combined displacement to the junction is
    // bounded by 2·d_eps — a derived bound, not tolerance widening. The final
    // position is `project_onto_circle(j)` so the vertex's `BRepEdge { edge, t }`
    // source round-trips bitwise through the unchanged `eval_source` Circle arm.
    for (&v, &(lr, (center, normal, radius, _src_r))) in &vert_junction {
        let p = mesh.verts[v as usize];
        let n = normalize3(normal.as_array());
        let d = normalize3(lr.dir.as_array());
        let denom = n[0] * d[0] + n[1] * d[1] + n[2] * d[2];
        if denom.abs() < cad_primitives::TAU_MODEL {
            // Line parallel to the circle plane: no transversal junction.
            return Err(YangError::stage4_region_invalid(
                v,
                Stage4InvalidReason::LocalRefinementRequired,
            ));
        }
        let pt = lr.point.as_array();
        let c = center.as_array();
        let s_par = (n[0] * (c[0] - pt[0]) + n[1] * (c[1] - pt[1]) + n[2] * (c[2] - pt[2])) / denom;
        let j = Point3::new(
            pt[0] + s_par * d[0],
            pt[1] + s_par * d[1],
            pt[2] + s_par * d[2],
        );
        let pj = [
            p.as_array()[0] - j.as_array()[0],
            p.as_array()[1] - j.as_array()[1],
            p.as_array()[2] - j.as_array()[2],
        ];
        let rho = (pj[0] * pj[0] + pj[1] * pj[1] + pj[2] * pj[2]).sqrt();
        // PR-F3b: line-band component carries the propagated budget; the
        // along-line crossing component stays at the raw d_ε.
        if rho > lr.band_budget + d_eps {
            if let Some(e) = s451_stop(s451_collect, &mut s45_failures, v, &curves0, &inc0) {
                return Err(e);
            }
            continue;
        }
        let (proj, t) = project_onto_circle(j, center, normal, radius)
            .map_err(|reason| YangError::stage4_region_invalid(v, reason))?;
        if rho > cad_primitives::TAU_WORK {
            mesh.verts[v as usize] = proj;
            moved.insert(v);
        }
        probe_push(line!(), v, t, mesh.verts[v as usize]);
        relocations.push((v, t));
        processed.insert(v);
    }

    // Task #146 (spec `yang_stage4_circle_pp_line_junction` branches 4–6):
    // relocate each circle×pp-line junction vertex onto the exact
    // line∩circle point (line∩sphere quadratic + circle-plane residual
    // certificate — valid for the in-plane AND transversal configurations).
    for (&v, &((lp, ld), (center, normal, radius, _src_r))) in &vert_pp_circle_junction {
        let p = mesh.verts[v as usize];
        let Some(j) = pp_line_circle_junction(lp, ld, center, normal, radius, p, d_eps) else {
            // Branch 5: the line misses the circle (or no root is on the
            // circle's plane) — not a resolvable junction here.
            return Err(YangError::stage4_region_invalid(
                v,
                Stage4InvalidReason::LocalRefinementRequired,
            ));
        };
        let pa = p.as_array();
        let ja = j.as_array();
        let rho =
            ((ja[0] - pa[0]).powi(2) + (ja[1] - pa[1]).powi(2) + (ja[2] - pa[2]).powi(2)).sqrt();
        // Branch 6: crossing amplification — the vertex sits within its
        // chord bands of BOTH curves; the displacement to their junction is
        // amplified by 1/sin θ, θ = angle between the line direction and the
        // circle tangent at the junction (the vert_circle_junction pattern;
        // derived, not widening).
        let n = normalize3(normal.as_array());
        let dh = normalize3(ld.as_array());
        let c = center.as_array();
        let rvec = normalize3([ja[0] - c[0], ja[1] - c[1], ja[2] - c[2]]);
        let tangent = [
            n[1] * rvec[2] - n[2] * rvec[1],
            n[2] * rvec[0] - n[0] * rvec[2],
            n[0] * rvec[1] - n[1] * rvec[0],
        ];
        let cross = [
            dh[1] * tangent[2] - dh[2] * tangent[1],
            dh[2] * tangent[0] - dh[0] * tangent[2],
            dh[0] * tangent[1] - dh[1] * tangent[0],
        ];
        let sin_theta = (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt();
        let gate = tangent_plane_corridor(d_eps, sin_theta);
        if rho > gate {
            if let Some(e) = s451_stop(s451_collect, &mut s45_failures, v, &curves0, &inc0) {
                return Err(e);
            }
            continue;
        }
        // Branch 4: `j` is exactly on the line and on the circle's sphere;
        // the circle projection yields the frame angle `t` for the retag
        // (positionally a no-op up to f64 — `j` is on the circle).
        let (proj, t) = project_onto_circle(j, center, normal, radius)
            .map_err(|reason| YangError::stage4_region_invalid(v, reason))?;
        if i1d_probe {
            let ca = center.as_array();
            let lpa = lp.as_array();
            let lda = ld.as_array();
            eprintln!(
                "[i1d-junction] v{v} rho={rho:.6e} sin_theta={sin_theta:.6e} gate={gate:.6e} \
                 circle c=({:.9},{:.9},{:.9}) r={radius:.9} line p=({:.9},{:.9},{:.9}) \
                 d=({:.6},{:.6},{:.6})",
                ca[0], ca[1], ca[2], lpa[0], lpa[1], lpa[2], lda[0], lda[1], lda[2],
            );
        }
        if rho > cad_primitives::TAU_WORK {
            mesh.verts[v as usize] = proj;
            moved.insert(v);
        }
        probe_push(line!(), v, t, mesh.verts[v as usize]);
        relocations.push((v, t));
        processed.insert(v);
    }

    // No-skip audit (anti-disproven-attempt): every conic endpoint was handled.
    let relocation_keys: HashSet<u32> = relocations.iter().map(|&(v, _)| v).collect();
    let endpoint_set: HashSet<u32> = endpoints.iter().copied().collect();
    if std::env::var_os("YANG_LRR_PROBE").is_some() && processed != endpoint_set {
        for &v in endpoint_set.difference(&processed) {
            let mut curs: Vec<String> = Vec::new();
            for (&(s, e), curve) in &curves0 {
                if s == v || e == v {
                    curs.push(format!("({s},{e})={curve:?}"));
                }
            }
            eprintln!(
                "YANG_LRR_UNCLAIMED endpoint v={v} on curves: {}",
                curs.join(" | ")
            );
        }
        for &v in processed.difference(&endpoint_set) {
            eprintln!("YANG_LRR_EXTRA processed-but-not-endpoint v={v}");
        }
    }
    // §4.5.1 inc-1 census: a RECORDED OffCurve failure is a HANDLED endpoint —
    // the paper's collected "cannot converge" state — not a silent skip. The
    // audit's job is catching UNRECORDED skips, so recorded vertices are
    // subtracted from the expectation. Gate off: the set is empty and the
    // audit is byte-identical.
    let s45_failed_set: HashSet<u32> = s45_failures.iter().map(|&(v, _)| v).collect();
    let endpoint_expectation: HashSet<u32> =
        endpoint_set.difference(&s45_failed_set).copied().collect();
    if processed != endpoint_expectation || processed != relocation_keys {
        if std::env::var_os("YANG_LRR_PROBE").is_some() {
            eprintln!(
                "YANG_LRR_STOP site=no_skip_audit ep_ne_proc={} proc_ne_reloc={}",
                processed != endpoint_expectation,
                processed != relocation_keys
            );
        }
        return Err(YangError::stage4_region_invalid(
            u32::MAX,
            Stage4InvalidReason::LocalRefinementRequired,
        ));
    }

    // M5 (Y4): degree-4 surface-pair relocation via Newton on the two defining
    // surfaces — a sibling of the TORUS block below (both are implicit-pair,
    // not conic, so they are relocated AFTER the conic audit and are NOT part
    // of the conic `endpoints`/`relocations` bookkeeping). Each endpoint keeps
    // its `BRepVertex` source (a procedural curve has no `t`). A surface-pair
    // endpoint that is ALSO a conic endpoint mixes closed-form and
    // implicit-pair relocations — out of v1 scope, loud STOP (mirrors the
    // torus block's `endpoint_set` guard). `None` is a loud STOP (tangency /
    // parallel normals or non-convergence — never a partial move, P9).
    for (&v, &(sa, sb)) in &vert_surface_pair {
        if endpoint_set.contains(&v) {
            // Endpoint-mix diagnosis probe (read-only, env-gated): the R0044
            // bucket's single STOP site. Dumps the surface pair carried by the
            // procedural edge PLUS the deduped surface set over every `inc0`
            // edge incident to `v` — the true junction incidence, which is what
            // decides whether the mix is a 3-surface triple point (solvable by
            // `relocate_onto_implicit_triple`) or something else.
            if std::env::var_os("YANG_LRR_PROBE").is_some() {
                let mut inc_surfs: Vec<(InputId, Surface)> = Vec::new();
                for (&(s, e), entries) in inc0.iter() {
                    if s != v && e != v {
                        continue;
                    }
                    for &(input, surf) in entries {
                        if !inc_surfs.iter().any(|&(i, t)| i == input && t == surf) {
                            inc_surfs.push((input, surf));
                        }
                    }
                }
                eprintln!(
                    "YANG_LRR_SITE site=surface_pair_endpoint_mix v={v} p={:?} \
                     pair=({sa:?}, {sb:?}) n_inc_surfs={} inc_surfs={inc_surfs:?} \
                     circle={} ellipse={} cone_ell={} parab={} hyp={} line={} \
                     ell_junction={} circle_junction={} line_circle_junction={}",
                    mesh.verts.get(v as usize),
                    inc_surfs.len(),
                    vert_circle.contains_key(&v),
                    vert_ellipse.contains_key(&v),
                    vert_cone_ellipse.contains_key(&v),
                    vert_parabola.contains_key(&v),
                    vert_cone_hyperbola.contains_key(&v),
                    vert_line.contains_key(&v),
                    vert_ell_junction.contains_key(&v),
                    vert_circle_junction.contains_key(&v),
                    vert_junction.contains_key(&v),
                );
            }
            return Err(YangError::stage4_region_invalid(
                v,
                Stage4InvalidReason::LocalRefinementRequired,
            ));
        }
        let p = mesh.verts[v as usize];
        let proj = relocate_onto_implicit_pair(p, sa, sb).ok_or_else(|| {
            YangError::stage4_region_invalid(v, Stage4InvalidReason::LocalRefinementRequired)
        })?;
        mesh.verts[v as usize] = proj;
        moved.insert(v);
    }

    // (2t) KV6d Tier B — degree-4 (TORUS) relocation via Newton on the implicit
    // surface pair. A torus's intersections are not conics, so these edges never
    // reach the `curves0` conic scan above; they arrive as untyped chord
    // segments and would otherwise stay off the analytic torus (the proven KV6d
    // blocker). For each intersection edge bearing exactly one torus and one
    // transversal partner, relocate both endpoints onto {F_torus=0, F_other=0}.
    // Kept SEPARATE from the conic bookkeeping (processed / endpoints /
    // relocations) — the output torus-intersection edges stay LineSegment
    // polylines (no analytic curve, no `t` retag), which validation and
    // `tessellate_torus_patch` already accept — so the conic no-skip audit above
    // is unaffected. Moved vertices join `moved` for the relocated-triangle
    // validation. Scope: one or two tori + one partner per edge (torus∩other
    // AND torus∩torus lateral, M5 #172); ≥3-surface junctions beyond the
    // triple arm and torus×conic endpoint mixing are loud STOPs (P9).
    {
        // Aggregate, per torus-edge endpoint, the base incident torus and the
        // DISTINCT partner surfaces across all its torus edges. One partner is a
        // plain torus∩surface edge (2-equation Newton) — the partner may itself
        // be a torus (torus×torus lateral, R0096); two partners is a
        // 3-surface JUNCTION — a box edge (two planes) piercing the torus, a
        // torus∩plane meeting a torus∩plane′, or torus×torus meeting a plane —
        // relocated onto all three. More than two partners is out of scope (STOP).
        let mut vert_torus: BTreeMap<u32, Surface> = BTreeMap::new();
        let mut vert_partners: BTreeMap<u32, Vec<Surface>> = BTreeMap::new();
        for (&(s, e), entries) in &inc0 {
            let mut tori: Vec<Surface> = Vec::new();
            let mut others: Vec<Surface> = Vec::new();
            for &(_input, surf) in entries {
                if matches!(surf, Surface::Torus { .. }) {
                    tori.push(surf);
                } else {
                    others.push(surf);
                }
            }
            if tori.is_empty() {
                continue; // not a torus edge — conic scan / exact handles it
            }
            if tori.len() > 2 {
                // ≥3 distinct tori at one edge — out of scope. Loud STOP.
                return Err(YangError::stage4_region_invalid(
                    s,
                    Stage4InvalidReason::LocalRefinementRequired,
                ));
            }
            // M5 #172: a torus∩torus lateral edge (two incident tori) joins
            // the SAME implicit-pair relocation as torus∩other — Newton on
            // {F_a=0, F_b=0} needs no closed form, so the degree-8 torus×torus
            // curve needs no special casing (the P8 procedural-curve model,
            // spec `m5_surface_pair_curve.md`; corpus customer R0096). The
            // base is the FIRST torus recorded at the vertex (`or_insert` —
            // stable across the vertex's edges); every OTHER distinct incident
            // surface, second torus included, joins the partner set, so a
            // torus×torus∩plane junction resolves via the triple arm below.
            // Coincident tori self-guard: the pair Newton's tangential rank
            // gate (det ≤ rank_eps) returns None → loud STOP.
            for v in [s, e] {
                let base = *vert_torus.entry(v).or_insert(tori[0]);
                let entry = vert_partners.entry(v).or_default();
                for o in tori.iter().chain(others.iter()) {
                    if *o != base && !entry.contains(o) {
                        entry.push(*o);
                    }
                }
            }
        }
        'torus_verts: for (&v, &t_surf) in &vert_torus {
            // A torus-edge endpoint that is also a CONIC endpoint mixes the
            // implicit-pair and closed-form relocations — out of v1 scope, STOP.
            if endpoint_set.contains(&v) {
                return Err(YangError::stage4_region_invalid(
                    v,
                    Stage4InvalidReason::LocalRefinementRequired,
                ));
            }
            let partners = &vert_partners[&v];
            let p = mesh.verts[v as usize];
            let (proj, n0, n1) = match partners.as_slice() {
                [s1] => {
                    if std::env::var_os("YANG_TORUS_PROBE").is_some()
                        && relocate_onto_implicit_pair(p, t_surf, *s1).is_none()
                    {
                        eprintln!(
                            "YANG_TORUS_STOP site=pair_newton_none v={v} p={p:?} \
                             t_surf={t_surf:?} partner={s1:?}"
                        );
                    }
                    let proj = relocate_onto_implicit_pair(p, t_surf, *s1).ok_or_else(|| {
                        YangError::stage4_region_invalid(
                            v,
                            Stage4InvalidReason::LocalRefinementRequired,
                        )
                    })?;
                    let qa = proj.as_array();
                    let (_, n0) = surface_value_and_normal(t_surf, qa).ok_or_else(|| {
                        YangError::stage4_region_invalid(
                            v,
                            Stage4InvalidReason::LocalRefinementRequired,
                        )
                    })?;
                    let (_, n1) = surface_value_and_normal(*s1, qa).ok_or_else(|| {
                        YangError::stage4_region_invalid(
                            v,
                            Stage4InvalidReason::LocalRefinementRequired,
                        )
                    })?;
                    (proj, n0, n1)
                }
                [s1, s2] => {
                    // 3-surface junction: relocate onto {torus, s1, s2}. The
                    // displacement gate uses the torus∩s1 angle (the junction is
                    // a point; any incident curve's metric bounds the move).
                    if std::env::var_os("YANG_TORUS_PROBE").is_some()
                        && relocate_onto_implicit_triple(p, t_surf, *s1, *s2).is_none()
                    {
                        eprintln!(
                            "YANG_TORUS_STOP site=triple_newton_none v={v} p={p:?} \
                             t_surf={t_surf:?} s1={s1:?} s2={s2:?}"
                        );
                    }
                    let proj =
                        relocate_onto_implicit_triple(p, t_surf, *s1, *s2).ok_or_else(|| {
                            YangError::stage4_region_invalid(
                                v,
                                Stage4InvalidReason::LocalRefinementRequired,
                            )
                        })?;
                    let qa = proj.as_array();
                    let (_, n0) = surface_value_and_normal(t_surf, qa).ok_or_else(|| {
                        YangError::stage4_region_invalid(
                            v,
                            Stage4InvalidReason::LocalRefinementRequired,
                        )
                    })?;
                    let (_, n1) = surface_value_and_normal(*s1, qa).ok_or_else(|| {
                        YangError::stage4_region_invalid(
                            v,
                            Stage4InvalidReason::LocalRefinementRequired,
                        )
                    })?;
                    (proj, n0, n1)
                }
                _ => {
                    if std::env::var_os("YANG_TORUS_PROBE").is_some() {
                        eprintln!(
                            "YANG_TORUS_STOP site=gt2_partners v={v} p={p:?} \
                             t_surf={t_surf:?} partners={partners:?}"
                        );
                    }
                    return Err(YangError::stage4_region_invalid(
                        v,
                        Stage4InvalidReason::LocalRefinementRequired,
                    ));
                }
            };
            // Derived displacement gate: a chord point moves to the exact curve
            // by ≤ 2·d_ε / sin θ, θ the angle between two incident surface
            // normals at the relocated point (the same metric as the disc∩disc /
            // cyl×cyl junction bands — NOT tolerance widening). Beyond it is a
            // real off-curve error, not a Stage-1 chord artifact → STOP.
            let pa = p.as_array();
            let qa = proj.as_array();
            let rho = ((qa[0] - pa[0]).powi(2) + (qa[1] - pa[1]).powi(2) + (qa[2] - pa[2]).powi(2))
                .sqrt();
            let cx = [
                n0[1] * n1[2] - n0[2] * n1[1],
                n0[2] * n1[0] - n0[0] * n1[2],
                n0[0] * n1[1] - n0[1] * n1[0],
            ];
            let sin_theta = (cx[0] * cx[0] + cx[1] * cx[1] + cx[2] * cx[2]).sqrt();
            let gate = tangent_plane_corridor(d_eps, sin_theta);
            if std::env::var_os("YANG_TORUS_PROBE").is_some() {
                let fv = surface_value_and_normal(t_surf, proj.as_array())
                    .map(|(f, _)| f)
                    .unwrap_or(f64::NAN);
                eprintln!(
                    "YANG_TORUS_PROBE v={v} rho={rho:.4e} gate={gate:.4e} d_eps={d_eps:.4e} \
                     sin_theta={sin_theta:.4e} F_torus(proj)={fv:.2e} p={p:?} proj={proj:?} \
                     t_surf={t_surf:?} partners={partners:?}"
                );
            }
            if rho > gate {
                if let Some(e) = s451_stop(s451_collect, &mut s45_failures, v, &curves0, &inc0) {
                    return Err(e);
                }
                continue;
            }
            // Bounded-face containment (KV6d closed torus, spec
            // `kv6d_closed_torus_revolve.md` failure modes): the wedge gate
            // bounds the TRANSVERSAL chord error but balloons (2d_ε/sinθ)
            // when the pair is near-tangential — exactly where an inscribed
            // mesh can close an intersection loop EARLY (entirely inside the
            // partner's bounded face) and the implicit-pair Newton then drags
            // the loop onto the infinite-surface curve, OUTSIDE the partner
            // FACE (C0065: wall x=1.45 vs outer equator 1.5, gap ≈ sagitta —
            // relocated points at |y| = 0.384 vs the wall's |y| ≤ 0.25). A
            // correctly resolved intersection vertex lies on both BOUNDED
            // faces, so a relocation escaping every matching partner face's
            // vertex hull (+d_ε) is a mesh-resolution artifact, not a chord
            // correction → loud STOP (the §4.3.3 near-tangency increment owns
            // the honest fix). Planes only: a planar face's loop hull bounds
            // the face (curved hulls under-bound — closed seam loops).
            for partner in partners {
                // The containment reading lives in `planar_partner_hull_contains`
                // (extracted for the §4.5.1 torus-region repair — one reading,
                // two callers). `None` = no verdict = no wall, as always.
                if planar_partner_hull_contains(a, b, *partner, proj.as_array(), d_eps)
                    == Some(false)
                {
                    if let Some(e) = s451_stop(s451_collect, &mut s45_failures, v, &curves0, &inc0)
                    {
                        return Err(e);
                    }
                    // Recorded: skip THIS TORUS VERTEX — a plain `continue`
                    // would only skip the current partner-face check and fall
                    // through to the relocation write below.
                    continue 'torus_verts;
                }
            }
            if rho > cad_primitives::TAU_WORK {
                mesh.verts[v as usize] = proj;
                moved.insert(v);
            }
        }
    }

    // §4.5.1 inc-1 census (spec §7): the sweep completed past recorded
    // OffCurve failures. Measure the selector at the paper's own vantage —
    // every non-failed vertex is now relocated — then return the FIRST
    // recorded error unchanged: the stage cannot complete with unrepaired
    // failures (P10), and every later pass's precondition (a fully-relocated
    // mesh) stays intact because none of them run.
    let mut s451_repaired_any = false;
    if !s45_failures.is_empty() {
        if s451_repair {
            // §4.5.1 inc-2b: plan every region READ-ONLY, then apply. Any
            // decline returns the FIRST recorded error unchanged (P10 — no
            // partial acceptance), so a case whose failures the repair cannot
            // own keeps today's exact STOP.
            match s451_plan_repairs(
                mesh,
                &attribution.attributions,
                brep_a,
                brep_b,
                &curves0,
                &inc0,
                &s4_entry_pos,
                &s45_failures,
            ) {
                Ok(plans) => {
                    for plan in &plans {
                        for &victim in &plan.victims {
                            collapse_vertex(
                                mesh,
                                &mut attribution.attributions,
                                victim,
                                plan.survivor,
                            );
                        }
                        mesh.verts[plan.survivor as usize] = plan.proj;
                        moved.insert(plan.survivor);
                        if let Some(t) = plan.retag {
                            processed.insert(plan.survivor);
                            relocations.push((plan.survivor, t));
                        }
                    }
                    eprintln!(
                        "YANG_451_REPAIR applied {} region repair(s); stage continues",
                        plans.len()
                    );
                    s451_repaired_any = !plans.is_empty();
                    s45_failures.clear();
                }
                Err(()) => {
                    let (_, first) = s45_failures.swap_remove(0);
                    return Err(first);
                }
            }
        } else {
            s451_post_sweep_census(
                mesh,
                &attribution.attributions,
                brep_a,
                brep_b,
                &curves0,
                &s4_entry_pos,
                &s45_failures,
            );
            let (_, first) = s45_failures.swap_remove(0);
            return Err(first);
        }
    }

    // (3) §4.5.3 reversed-intersection correction sweep.
    // (`collapsed_any` starts true when §4.5.1 repairs collapsed vertices
    // above — the post-collapse Phase-A recompute must run for those too.)
    let mut collapsed_any = s451_repaired_any;
    let mut attr_vec = std::mem::take(&mut attribution.attributions);
    // PR-KV9: junction vertices that landed on the SAME exact point are
    // duplicates of one geometric junction (near a tangency-grade curve
    // crossing the two chord polylines can intersect several times, giving
    // several arrangement vertices for ONE junction). Collapse the extras
    // onto the lowest index — the standard edge-collapse, which drops the
    // degenerate slivers between them and keeps the half-edge pairing
    // watertight.
    {
        let mut by_pos: std::collections::BTreeMap<[u64; 3], Vec<u32>> =
            std::collections::BTreeMap::new();
        for &v in vert_ell_junction.keys() {
            let p = mesh.verts[v as usize];
            by_pos
                .entry([p.x().to_bits(), p.y().to_bits(), p.z().to_bits()])
                .or_default()
                .push(v);
        }
        for (_, group) in by_pos {
            if group.len() < 2 {
                continue;
            }
            let survivor = *group.iter().min().expect("non-empty");
            for &victim in group.iter().filter(|&&v| v != survivor) {
                if std::env::var_os("YANG_DOUBLECOVER_PROBE").is_some() {
                    eprintln!(
                        "[collapse-site] PR-KV9 junction-twin victim={victim} survivor={survivor}"
                    );
                }
                collapse_vertex(mesh, &mut attr_vec, victim, survivor);
                collapsed_any = true;
            }
        }
    }
    // P3b inc-4a (R0061): weld relocated verts that converged onto a Stage-1
    // MINTED junction BEFORE any pass that walks patch boundaries — the §4.5.3
    // sweep below recomputes Phase A, whose figure-eight wedge walk dies on a
    // machine-ε moved×mint twin pair (s6-wedge-walk-not-outgoing at R0061's
    // v173/v186). Same §4.3 op and band as the (3b′) weld; survivor = the mint
    // (bits are the shared cross-operand junction identity, N54). The (3b′)
    // call stays as the residual catch after the sweep + §4.4.1(b) merge.
    let probe_minted_verts: HashSet<u32> = if std::env::var_os("YANG_P3B_FOLD_PROBE").is_some() {
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
    } else {
        HashSet::new()
    };
    // inc-4c fold probe (read-only, `YANG_P3B_FOLD_PROBE=x,y,z,r`): dump the
    // local triangle complex near a point BEFORE the moved×minted weld and
    // AFTER the trim — measures how the stacked collapses restructure the
    // corner fan (the R0061 over-used minted×minted edge). Fires in both gate
    // states (gate-OFF has no mints; the dump is then the correct-baseline
    // local structure).
    let fold_probe = |tag: &str,
                      mesh: &Mesh,
                      attr: &[Option<TriangleAttribution>],
                      minted_verts: &HashSet<u32>| {
        let Ok(spec) = std::env::var("YANG_P3B_FOLD_PROBE") else {
            return;
        };
        let parts: Vec<f64> = spec.split(',').filter_map(|s| s.parse().ok()).collect();
        let [px, py, pz, pr] = parts.as_slice() else {
            return;
        };
        let near = |v: u32| {
            let p = mesh.verts[v as usize].as_array();
            ((p[0] - px).powi(2) + (p[1] - py).powi(2) + (p[2] - pz).powi(2)).sqrt() <= *pr
        };
        for (t, tri) in mesh.tris.iter().enumerate() {
            if !tri.iter().any(|&v| near(v)) {
                continue;
            }
            let flags: Vec<String> = tri
                .iter()
                .map(|&v| {
                    let mut s = format!("v{v}");
                    if minted_verts.contains(&v) {
                        s.push('M');
                    } else if moved.contains(&v) {
                        s.push('m');
                    }
                    s
                })
                .collect();
            eprintln!(
                "[p3b-fold {tag}] tri {t} {} att={:?} coords {:?}",
                flags.join(","),
                attr.get(t).copied().flatten().map(|a| (a.input, a.face)),
                tri.iter()
                    .map(|&v| mesh.verts[v as usize])
                    .collect::<Vec<_>>()
            );
        }
    };
    fold_probe("pre", mesh, &attr_vec, &probe_minted_verts);
    if !minted_junction_keys.is_empty() {
        let minted_verts: HashSet<u32> = mesh
            .verts
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
            .collect();
        if std::env::var_os("YANG_MOVED_WELD_PROBE").is_some() {
            eprintln!(
                "[moved-weld] pre-sweep: moved={} minted_verts={}",
                moved.len(),
                minted_verts.len()
            );
        }
        // Restrict the pre-sweep pass to moved×minted pairs only (empty
        // `moved` complement): pass `moved` as-is but rely on the weld's
        // pairing rule — a moved×moved weld here would reorder the
        // established (3b′)-after-sweep behavior, so filter to pairs
        // involving a mint by handing the weld ONLY the moved verts within
        // the coincidence band of some mint.
        if !minted_verts.is_empty() {
            let mut near_mint_moved: HashSet<u32> = HashSet::new();
            for &mv in moved.iter() {
                let pm = mesh.verts[mv as usize].as_array();
                for &jv in &minted_verts {
                    let pj = mesh.verts[jv as usize].as_array();
                    let d = ((pm[0] - pj[0]).powi(2)
                        + (pm[1] - pj[1]).powi(2)
                        + (pm[2] - pj[2]).powi(2))
                    .sqrt();
                    let scale = pm
                        .iter()
                        .chain(pj.iter())
                        .fold(0.0f64, |m, &c| m.max(c.abs()));
                    if d < cad_primitives::TAU_MODEL * (1.0 + scale) {
                        near_mint_moved.insert(mv);
                        break;
                    }
                }
            }
            if !near_mint_moved.is_empty()
                && weld_coincident_relocated(mesh, &mut attr_vec, &near_mint_moved, &minted_verts)
            {
                collapsed_any = true;
            }
        }
        // P3b inc-4b: beyond-corner conformal trim, immediately AFTER the
        // moved×minted weld (the weld owns coincidence ≤ TAU_MODEL band; the
        // trim owns band→corridor beyond-corner phantoms — F0082's 2.76e-3).
        // Re-resolve mint vert ids WITH provenance: the weld above may have
        // collapsed vertices, but mint coordinates are never mutated.
        let minted_prov: std::collections::BTreeMap<u32, crate::boolean::MintProvenance> = mesh
            .verts
            .iter()
            .enumerate()
            .filter_map(|(v, p)| {
                minted_junction_keys
                    .get(&[p.x().to_bits(), p.y().to_bits(), p.z().to_bits()])
                    .map(|prov| (v as u32, *prov))
            })
            .collect();
        // I1c census (probe-only): the TF-8 anchor needs the IDENTITY of the
        // overshot seam junctions at trim time — minted? moved? neither? —
        // matched offline by position (ids differ across passes; mint
        // coordinates never mutate).
        if std::env::var_os("YANG_P3B_TRIM_PROBE").is_some() {
            for (&v, prov) in &minted_prov {
                let p = mesh.verts[v as usize];
                eprintln!(
                    "[p3b-minted] v{v} ({:.12}, {:.12}, {:.12}) planes trim_beyond=[{},{}]",
                    p.x(),
                    p.y(),
                    p.z(),
                    prov.owner_planes[0].trim_beyond,
                    prov.owner_planes[1].trim_beyond,
                );
            }
            for &v in &moved {
                let p = mesh.verts[v as usize];
                eprintln!(
                    "[p3b-moved] v{v} ({:.12}, {:.12}, {:.12})",
                    p.x(),
                    p.y(),
                    p.z()
                );
            }
        }
        if !minted_prov.is_empty()
            && trim_beyond_corner_phantoms(mesh, &mut attr_vec, &moved, &minted_prov, d_eps)
        {
            collapsed_any = true;
        }
        // P3b inc-4c: the §4.4.1 triangulation-update half of the merges
        // above — dissolve the fan folds the stacked collapses manufacture
        // (spec `yang_169_p3b_inc4c_fan_retriangulation.md`). Connectivity
        // only; per-cluster fail-closed; must run before ANY boundary-walking
        // pass (the sweep below recomputes Phase A — the inc-4a placement
        // lesson).
        if retriangulate_collapsed_fan_regions(
            mesh,
            &mut attr_vec,
            brep_a,
            brep_b,
            &moved,
            &minted_verts,
        ) {
            collapsed_any = true;
        }
    }
    fold_probe("post", mesh, &attr_vec, &probe_minted_verts);
    let sweep_result = sweep_reversed_intersections(mesh, &mut attr_vec, a, b, d_eps);
    attribution.attributions = attr_vec;
    let any_collapse = sweep_result?;
    collapsed_any |= any_collapse;

    // (3c) §4.4.1(b) sub-feature-size vertex merge (Yang Fig. 11(b): "if an
    // endpoint p of the split edge is too close to q, we merge p with q"). After
    // relocation a degenerate triangle can have two vertices nearer than
    // MIN_FEATURE_SIZE — the governance feature floor (A14.2): two points closer
    // than the smallest representable feature ARE the same point. This is the
    // curved-input analog of the I6 near-weld (which is bit-exact-only for curved
    // inputs — "Stage-4 owns junction-duplicate collapse"). Merge such a pair via
    // the watertight-preserving `collapse_vertex` (higher index → lower, dropping
    // the now-degenerate slivers), iterating to a fixed point. P9/P10: the gate is
    // the GOVERNANCE feature floor, not a tuned tolerance, and a genuinely-spread
    // degenerate (vertices ≥ the floor apart — e.g. a monotonic-collinear sliver
    // on a curved patch) is left UNTOUCHED for `validate_relocated_triangles` to
    // STOP loudly / the curved-patch re-CDT (N2-2) to handle. Spec
    // `specs/yang_n2_stage4_cdt_mesh_updating.md` §5 increment N2-1.
    //
    // SCOPE NOTE (M8 holed-disc increment 3, 2026-07-06): a GLOBAL widening of
    // this scan (all triangles + a Stage-4 ENTRY pass) was tried and REVERTED —
    // at micro model scale (R0091, 1.6e-4) the ABSOLUTE floor collapses
    // legitimately-distinct arrangement geometry (Euler flipped to −4,
    // SUPPORTED_WRONG). The relocation/conic-adjacent eligibility below is
    // LOAD-BEARING: it keeps the merge away from pre-existing arrangement
    // slivers that `boolean()` legitimately kept for watertightness.
    // #169 N55: Yang §4.4.1(b) numerical-duplicate merge — COMPLIANT and
    // ALWAYS-ON (replaces the retired absolute-floor `subfeature` weld). The
    // paper's Fig-11(b) "if p is too close to q, merge p with q" is a
    // NUMERICAL-COINCIDENCE test (two relocated points that Newton-converged to
    // the SAME intersection point), not a feature-size floor. The criterion is
    // therefore the scale-relative working tolerance `TAU_WORK·(1+scale)` at the
    // edge gate below (an exact-dedup merge — the kind the compliance ratchet
    // KEEPS), NOT the absolute `MIN_FEATURE_SIZE` the weld used (which also
    // collapsed genuine sub-resolution edges at micro-scale — R0072's ~1e-7
    // merges, the R0091 hazard — now correctly refused → curved re-CDT). `floor`
    // here is only the DEGENERACY DETECTOR (a triangle below the feature floor
    // is a merge candidate); the actual same-point decision is the tighter
    // numerical band. Deviation N55.
    {
        let floor = cad_primitives::MIN_FEATURE_SIZE;
        let mut attr_vec = std::mem::take(&mut attribution.attributions);
        // KV9-F3 (spec `kv9_f3_output_vertex_identity` E-V2): junction
        // duplicates that are ALREADY on their exact curve (rho ≤ TAU_WORK)
        // are never `moved`, yet they are precisely the population the I6
        // weld delegates to Stage-4 ("Stage-4 owns junction-duplicate
        // collapse" — curved inputs weld bit-exact only). Scan eligibility
        // therefore includes triangles touching any CONIC-ENDPOINT vertex;
        // the merge criterion below is unchanged (the governance
        // MIN_FEATURE_SIZE floor, A14.2 — never a tuned tolerance).
        let conic_endpoint: std::collections::BTreeSet<u32> = vert_circle
            .keys()
            .chain(vert_line.keys())
            .chain(vert_ellipse.keys())
            .chain(vert_cone_ellipse.keys())
            .chain(vert_parabola.keys())
            .chain(vert_cone_hyperbola.keys())
            .chain(vert_ell_junction.keys())
            .chain(vert_circle_junction.keys())
            .copied()
            .collect();
        // Spec `yang_453_junction_protected_collapse` §3b: closed-form junction
        // vertices (exact on TWO curves) outrank single-curve conic endpoints,
        // which outrank plain mesh vertices, in merge-survivor selection.
        let junction_verts: std::collections::BTreeSet<u32> = vert_ell_junction
            .keys()
            .chain(vert_circle_junction.keys())
            .chain(vert_junction.keys())
            .copied()
            .collect();
        // Each pass collapses ≤1 sub-feature edge; bounded by the triangle count.
        let max_merge_passes = mesh.tris.len() + 1;
        let mut merge_passes = 0usize;
        let mut last_merge: Option<(u32, u32, f64, usize)> = None;
        loop {
            merge_passes += 1;
            if merge_passes > max_merge_passes {
                // §4.4.1(b) diagnosis probe (read-only, env-gated): the budget
                // guard should be unreachable if every pass drops ≥1 triangle
                // — print the terminal state to localize a livelock.
                if std::env::var_os("YANG_S4_MERGE_PROBE").is_some() {
                    eprintln!(
                        "[s4-merge-probe] BUDGET EXHAUSTED: passes={merge_passes} \
                         max={max_merge_passes} tris_now={} last_merge={last_merge:?}",
                        mesh.tris.len()
                    );
                }
                if std::env::var_os("YANG_LRR_PROBE").is_some() {
                    eprintln!("YANG_LRR_STOP site=merge_budget");
                }
                attribution.attributions = attr_vec;
                return Err(YangError::stage4_region_invalid(
                    u32::MAX,
                    Stage4InvalidReason::LocalRefinementRequired,
                ));
            }
            let mut to_merge: Option<(u32, u32)> = None;
            for tri in &mesh.tris {
                if !tri
                    .iter()
                    .any(|v| moved.contains(v) || conic_endpoint.contains(v))
                {
                    continue;
                }
                let p0 = mesh.verts[tri[0] as usize].as_array();
                let p1 = mesh.verts[tri[1] as usize].as_array();
                let p2 = mesh.verts[tri[2] as usize].as_array();
                let nrm = tri_area_vector(p0, p1, p2);
                let twice_area = (nrm[0] * nrm[0] + nrm[1] * nrm[1] + nrm[2] * nrm[2]).sqrt();
                if twice_area * 0.5 >= floor * floor {
                    continue; // not degenerate — leave it
                }
                // Degenerate relocated triangle: if its SHORTEST edge is below the
                // feature floor, those two endpoints are the same point → merge.
                let dist = |a: [f64; 3], b: [f64; 3]| {
                    let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
                    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
                };
                let edges = [
                    (tri[0], tri[1], dist(p0, p1)),
                    (tri[1], tri[2], dist(p1, p2)),
                    (tri[2], tri[0], dist(p2, p0)),
                ];
                let (u, v, len) = edges
                    .iter()
                    .copied()
                    .min_by(|x, y| x.2.partial_cmp(&y.2).unwrap_or(std::cmp::Ordering::Equal))
                    .expect("3 edges");
                // §4.4.1(b) same-point test: the shortest edge is a numerical
                // coincidence iff below the scale-relative working tolerance
                // `TAU_WORK·(1+scale)` (`scale` = max |coord| of the pair). This
                // is the model's own "numerically identical" threshold — 5 orders
                // tighter than `MIN_FEATURE_SIZE` — so it merges only relocation
                // twins that converged onto one point (~machine ε; R0055/F0056/
                // F0057/F0059) and never a genuine sub-feature edge (R0072's ~1e-7
                // collapse at micro-scale is refused → loud STOP → curved re-CDT).
                let scale = {
                    let pu = mesh.verts[u as usize].as_array();
                    let pv = mesh.verts[v as usize].as_array();
                    pu.iter()
                        .chain(pv.iter())
                        .fold(0.0f64, |m, &c| m.max(c.abs()))
                };
                let merge = is_relocation_coincidence(len, scale);
                if std::env::var_os("YANG_S44B_MEASURE").is_some() {
                    eprintln!(
                        "[s44b] cand u={u} v={v} len={len:.4e} scale={scale:.4e} \
                         band={:.4e} merge={merge}",
                        cad_primitives::TAU_WORK * (1.0 + scale)
                    );
                }
                if merge {
                    // Spec `yang_453_junction_protected_collapse` §3b: the
                    // exactness-ranked survivor (Yang Fig. 11(b) — "merge p
                    // with q": the exact intersection point q survives).
                    // WIRED 2026-07-21 (task #186): the §3b blocker was the
                    // unverified R0091 χ — resolved by verifying the output's
                    // true χ = −4 via Cherchi sidecar reference parity on the
                    // exact operand meshes + an independent voxel-CSG
                    // derivation from the authored numbers (the meta's naive
                    // 3-op default χ=2 was the authoring error; corrected).
                    let (victim, survivor) =
                        sub_feature_merge_direction(&junction_verts, &conic_endpoint, u, v);
                    to_merge = Some((victim, survivor));
                    break;
                }
            }
            match to_merge {
                Some((victim, survivor)) => {
                    if std::env::var_os("YANG_DOUBLECOVER_PROBE").is_some() {
                        eprintln!(
                            "[collapse-site] s4.4.1b-merge victim={victim} survivor={survivor}"
                        );
                    }
                    let dropped = collapse_vertex(mesh, &mut attr_vec, victim, survivor);
                    last_merge = Some((victim, survivor, dropped as f64, mesh.tris.len()));
                    collapsed_any = true;
                }
                None => break,
            }
        }
        attribution.attributions = attr_vec;
    }

    // (3b′) Coincident RELOCATED-vertex weld (spec `yang_n47_coincident_moved_weld`,
    // deviation N47). Two vertices this pipeline RELOCATED (`moved`: pushed onto an
    // analytic circle/ellipse/line/torus/surface-pair) can converge to within the
    // MODEL coincidence tolerance `TAU_MODEL·(1+scale)` — they are the SAME
    // geometric point emitted twice (a near-tangent seam crossing whose two
    // arrangement points both Newton-project onto one intersection point). The
    // §4.4.1(b) merge above misses them: it scans TRIANGLE edges and gates on the
    // triangle AREA (`floor²`), so a NEEDLE (two coincident verts + one far vert:
    // large area, sub-floor edge) is skipped, and a coincident pair that is only
    // LOOP-adjacent (not a shared triangle edge) is never examined. Left in place,
    // the twins survive into `emit_topology` (vertices 1:1 with `mesh.verts`) as a
    // sub-render-precision output edge, tripping kernel-v2's G1 render-collapse
    // gate FAR downstream (R0012 face 1023 @ 7e-7 / scale 100; R0098 face 599 @
    // 4e-6 / scale 1900). Welding here is a self-localizing PRODUCER fix.
    //
    // Band: the scale-relative MODEL coincidence tolerance `TAU_MODEL·(1+scale)`
    // (`scale` = max |coord| of the pair) — the SAME band the stage-5 planarity
    // wall and every other coincidence test uses; it is 10× TIGHTER than the
    // MIN_FEATURE_SIZE feature floor, so it admits ONLY sub-(feature/10)
    // coincidences (a genuine feature is ≥ `MIN_FEATURE_SIZE·(1+scale)` apart).
    // NOT tolerance widening (P9): it is the model's own definition of "same
    // point," and it only ever COLLAPSES an already-degenerate output edge.
    // Restricted to `moved`×`moved` pairs (the relocation-convergence mechanism) —
    // it never touches un-relocated arrangement geometry `boolean()` kept for
    // watertightness (cf. the §4.4.1(b) micro-scale R0091 revert). `collapse_vertex`
    // is the proven watertight-preserving edge-collapse (with membrane
    // cancellation); iterate to a fixed point over live (still-referenced) verts.
    // #169 N56: reinstated as a COMPLIANT always-on Yang §4.3 operation ("we
    // remove a point if it is too close to another point on the same loop",
    // paper line 535). Both verts are `moved` = relocated onto the analytic
    // curve, so merging one into the other is faithful redundant-curve-point
    // removal, not a tolerance hack. Measured 0-conversion on the current
    // corpus (the R0012/R0098 render twins named above are NOT reached by this
    // §4.3 merge — they are un-relocated arrangement verts needing the Stage-0
    // fix); it is kept as paper machinery for near-tangency (#137). Genuine
    // Yang ⇒ un-gated (was `weld_enabled("coincident")`).
    {
        // P3b inc-4a: resolve the Stage-1 minted junction points (exact bits,
        // threaded from `boolean()`) to mesh vertex ids. Bit-exact match only —
        // the mint contract preserves the bits through Stage 1 + arrangement,
        // and coordinates are never mutated by the collapses above.
        let minted_verts: HashSet<u32> = if minted_junction_keys.is_empty() {
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
        if std::env::var_os("YANG_MOVED_WELD_PROBE").is_some() {
            eprintln!(
                "[moved-weld] entry: moved={} minted_keys={} minted_verts={:?}",
                moved.len(),
                minted_junction_keys.len(),
                minted_verts
            );
        }
        let mut attr_vec = std::mem::take(&mut attribution.attributions);
        if weld_coincident_relocated(mesh, &mut attr_vec, &moved, &minted_verts) {
            collapsed_any = true;
        }
        attribution.attributions = attr_vec;
    }

    // Twin-scan probe (read-only, env-gated `YANG_TWIN_SCAN`): dump every
    // sub-feature-floor mesh edge surviving the §4.4.1(b) merge, with
    // eligibility flags — self-localizes a surviving ULP-twin pair (the
    // F0047 render-collapse diagnosis tool).
    if std::env::var_os("YANG_TWIN_SCAN").is_some() {
        let floor = cad_primitives::MIN_FEATURE_SIZE;
        let mut seen: std::collections::BTreeSet<(u32, u32)> = std::collections::BTreeSet::new();
        for tri in &mesh.tris {
            for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
                let (u, v) = (tri[i].min(tri[j]), tri[i].max(tri[j]));
                if u == v || !seen.insert((u, v)) {
                    continue;
                }
                let pu = mesh.verts[u as usize].as_array();
                let pv = mesh.verts[v as usize].as_array();
                let d = [pu[0] - pv[0], pu[1] - pv[1], pu[2] - pv[2]];
                let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
                if len < floor {
                    eprintln!(
                        "[twin-scan] edge ({u},{v}) len={len:.3e} \
                         exact_j=({},{}) moved=({},{}) pu={pu:?}",
                        exact_junctions.contains(&u),
                        exact_junctions.contains(&v),
                        moved.contains(&u),
                        moved.contains(&v),
                    );
                }
            }
        }
    }

    // KV9-F1 Increment 0c census: post-merge junction-twin state — coincident
    // junction vertices that SURVIVED the §4.4.1(b) merge, and whether the
    // survivors are edge-adjacent in the current mesh (kept env-gated).
    if std::env::var("KV9_JUNCTION_PROBE").is_ok() {
        let keys: Vec<u32> = vert_ell_junction.keys().copied().collect();
        for (i, &u) in keys.iter().enumerate() {
            for &w in &keys[i + 1..] {
                let (pu, pw) = (mesh.verts[u as usize], mesh.verts[w as usize]);
                if pu.as_array() != pw.as_array() {
                    continue;
                }
                let adjacent = mesh.tris.iter().any(|t| t.contains(&u) && t.contains(&w));
                let (du, dw) = (
                    mesh.tris.iter().filter(|t| t.contains(&u)).count(),
                    mesh.tris.iter().filter(|t| t.contains(&w)).count(),
                );
                eprintln!(
                    "KV9_JUNCTION_PROBE post-merge coincident twins: v{u} v{w} at {:?} \
                     edge_adjacent={adjacent} deg({u})={du} deg({w})={dw}",
                    pu.as_array()
                );
            }
        }
    }

    // (3d) §4.4.1(a) edge-split (Yang Fig. 11(a): "locate the constrained edge
    // containing q, split it at q"). A degenerate relocated triangle D=[a,b,c] is
    // collinear: the vertex OFF its longest edge (`b`) lies on that long edge
    // `a-c` (a redundant intersection point on the constraint curve). The faithful
    // fix inserts `b` into the triangle ON THE OTHER SIDE of `a-c` — split that
    // neighbour N=[a,c,d] into [a,b,d]+[b,c,d] — and drops D. This is a LOCAL,
    // watertight-preserving operation (D's edges a-b/b-c re-pair with the split
    // halves; the long edge a-c, shared only by D and N, vanishes): no re-CDT, no
    // parametric domain, no cylinder θ-seam. Iterate, each step acting on a
    // degenerate triangle whose long-edge neighbour is NON-degenerate (so the
    // strip unzips from its non-degenerate margin inward); a remaining degenerate
    // triangle with no non-degenerate neighbour is a genuine §4.5.2 STOP. Spec
    // `specs/yang_n2_stage4_cdt_mesh_updating.md`.
    //
    // DEGENERACY IS THE SCALE-FREE COLLINEARITY IDENTITY (`tri_is_degenerate`,
    // 2026-08-19): the former absolute `MIN_FEATURE_SIZE²` area floor flagged
    // HEALTHY micro-scale triangles (R0009/R0047 at 1e-4 m: h/l 0.007–0.4),
    // so this arm flipped real triangles (silent geometry change on R0091/
    // R0072/R0063) and ping-ponged to the pass cap on R0009. The unzip's own
    // precondition — dropping D preserves geometry — holds ONLY when D is
    // numerically zero-area relative to its extent; that is the test now.
    {
        balance_census(mesh, "pre-degen-loop");
        let is_degen = |ti: usize, mesh: &Mesh| -> bool {
            let t = mesh.tris[ti];
            if !t.iter().any(|v| moved.contains(v)) {
                return false;
            }
            tri_is_degenerate(
                mesh.verts[t[0] as usize].as_array(),
                mesh.verts[t[1] as usize].as_array(),
                mesh.verts[t[2] as usize].as_array(),
            )
        };
        // The off-longest-edge vertex `b` (the collinear middle) + extremes a,c.
        let long_edge_off = |t: &[u32; 3], mesh: &Mesh| -> (u32, u32, u32) {
            let d = |i: usize, j: usize| {
                let p = mesh.verts[t[i] as usize].as_array();
                let q = mesh.verts[t[j] as usize].as_array();
                let e = [p[0] - q[0], p[1] - q[1], p[2] - q[2]];
                e[0] * e[0] + e[1] * e[1] + e[2] * e[2]
            };
            let (e01, e12, e20) = (d(0, 1), d(1, 2), d(2, 0));
            if e01 >= e12 && e01 >= e20 {
                (t[0], t[1], t[2]) // long a-c = v0-v1, off b = v2
            } else if e12 >= e20 {
                (t[1], t[2], t[0])
            } else {
                (t[2], t[0], t[1])
            }
        };
        let mut attr_vec = std::mem::take(&mut attribution.attributions);
        let max_passes = mesh.tris.len() + 1;
        let mut passes = 0usize;
        // Progress certificate (findings Q3's per-pass monitor, applied here):
        // the loop is deterministic in the mesh state, so a simple action
        // re-firing on the SAME (D, N, a, c, b) tuple means an earlier action
        // was undone — a ping-pong that would spin to the pass cap (R0009:
        // a 4-action cycle; R0047: 5168 actions / 62 s under the old floor).
        // STOP on the first repeat instead of burning O(T) passes × O(T).
        let mut seen_actions: std::collections::HashSet<(usize, usize, u32, u32, u32)> =
            std::collections::HashSet::new();
        loop {
            passes += 1;
            if passes > max_passes {
                if std::env::var_os("YANG_LRR_PROBE").is_some() {
                    eprintln!("YANG_LRR_STOP site=split_max_passes");
                }
                attribution.attributions = attr_vec;
                return Err(YangError::stage4_region_invalid(
                    u32::MAX,
                    Stage4InvalidReason::LocalRefinementRequired,
                ));
            }
            // Edge → incident triangle indices (for the across-edge neighbour).
            let mut edge_tris: std::collections::HashMap<(u32, u32), Vec<u32>> =
                std::collections::HashMap::new();
            for (ti, tri) in mesh.tris.iter().enumerate() {
                for (i, j) in [(0, 1), (1, 2), (2, 0)] {
                    let (u, v) = (tri[i], tri[j]);
                    let key = if u < v { (u, v) } else { (v, u) };
                    edge_tris.entry(key).or_default().push(ti as u32);
                }
            }
            // Pick a degenerate triangle whose long-edge neighbour is non-degenerate.
            let mut action: Option<(usize, usize, u32, u32, u32)> = None;
            // Fallback: a MUTUAL pair — both incident triangles of one long edge
            // degenerate with that same edge as their long edge (a zero-area quad
            // astride the edge, off-vertices interleaved along it). No simple
            // action can ever fire on either member (each is the other's
            // neighbour), so when only such pairs remain the loop STOPs. The
            // mutual arm resolves the quad: drop both, and Fig-11(a)-split the
            // two OUTER neighbours so both sides carry the identical fine chain
            // a–bL–bH–c (two-sided conformal by construction, no geometry moved).
            // ALWAYS-ON since 2026-07-31 (was `YANG_S4_MUTUAL_PAIR_ENABLE`;
            // spec §5c.11 — corpus sweep: zero category deltas, F0067/R0038
            // advance to their deeper pre-existing walls).
            let mut mutual: Option<MutualPair> = None;
            let mut any_degen = false;
            for ti in 0..mesh.tris.len() {
                if !is_degen(ti, mesh) {
                    continue;
                }
                any_degen = true;
                let (a, c, b) = long_edge_off(&mesh.tris[ti], mesh);
                let key = if a < c { (a, c) } else { (c, a) };
                let inc = match edge_tris.get(&key) {
                    Some(v) if v.len() == 2 => v,
                    _ => continue, // boundary / non-manifold long edge — skip
                };
                let n = if inc[0] as usize == ti {
                    inc[1]
                } else {
                    inc[0]
                } as usize;
                if is_degen(n, mesh) {
                    // Simple arm defers to let the neighbour resolve first; if the
                    // neighbour's long edge is the SAME edge, neither ever will —
                    // record the pair as a mutual-arm candidate (executed only when
                    // no simple action exists anywhere this pass).
                    if mutual.is_none() {
                        mutual = mutual_pair_candidate(
                            mesh,
                            &edge_tris,
                            &is_degen,
                            &long_edge_off,
                            ti,
                            n,
                            a,
                            c,
                            b,
                        );
                    }
                    continue; // defer until the neighbour is resolved
                }
                action = Some((ti, n, a, c, b));
                break;
            }
            let (d_idx, n_idx, a, c, b) = match action {
                Some(x) => x,
                None => {
                    if let Some(m) = mutual.take() {
                        if std::env::var_os("YANG_LRR_PROBE").is_some() {
                            eprintln!(
                                "YANG_LRR_ACTION mutual t1={} t2={} nl={} nh={} \
                                 a={} c={} bl={} bh={}",
                                m.t1, m.t2, m.nl, m.nh, m.a, m.c, m.bl, m.bh
                            );
                        }
                        resolve_mutual_degenerate_pair(mesh, &mut attr_vec, &m);
                        collapsed_any = true;
                        continue;
                    }
                    if any_degen {
                        // Degenerate triangles remain but none has a non-degenerate
                        // long-edge neighbour — genuine local-refinement territory.
                        if std::env::var_os("YANG_LRR_PROBE").is_some() {
                            let mut ndeg = 0usize;
                            for ti in 0..mesh.tris.len() {
                                if !is_degen(ti, mesh) {
                                    continue;
                                }
                                ndeg += 1;
                                let (a, c, b) = long_edge_off(&mesh.tris[ti], mesh);
                                let surf = attr_vec.get(ti).and_then(|o| o.as_ref()).map(|at| {
                                    let br = match at.input {
                                        InputId::A => brep_a,
                                        InputId::B => brep_b,
                                    };
                                    br.faces()[at.face as usize].surface
                                });
                                eprintln!("YANG_LRR_DEGEN_SURF tri={ti} surface={surf:?}");
                                let key = if a < c { (a, c) } else { (c, a) };
                                let inc = edge_tris.get(&key).map(|v| v.len()).unwrap_or(0);
                                let nbr_degen = edge_tris.get(&key).is_some_and(|v| {
                                    v.iter()
                                        .any(|&n| n as usize != ti && is_degen(n as usize, mesh))
                                });
                                eprintln!(
                                    "YANG_LRR_DEGEN tri={ti} verts={:?} long_edge=({a},{c}) off={b} \
                                     inc_count={inc} nbr_degen={nbr_degen} moved_a={} moved_c={} moved_b={} \
                                     pa={:?} pc={:?} pb={:?}",
                                    mesh.tris[ti],
                                    moved.contains(&a),
                                    moved.contains(&c),
                                    moved.contains(&b),
                                    mesh.verts[a as usize].as_array(),
                                    mesh.verts[c as usize].as_array(),
                                    mesh.verts[b as usize].as_array(),
                                );
                                // Mutual-pair anatomy: both incident triangles of the
                                // long edge are degenerate and report the SAME long
                                // edge (the quad-astride-one-edge configuration). For
                                // that configuration the candidate update is: drop
                                // both, insert bH into (bL,c) and bL into (a,bH) —
                                // so dump the off-vertex edge parameters plus the
                                // OUTER neighbours across those two insertion edges
                                // (their attribution decides the cross-face risk).
                                if let Some(incv) = edge_tris.get(&key) {
                                    if incv.len() == 2 {
                                        let n = if incv[0] as usize == ti {
                                            incv[1]
                                        } else {
                                            incv[0]
                                        } as usize;
                                        let (na, nc, nb) = long_edge_off(&mesh.tris[n], mesh);
                                        let nkey = if na < nc { (na, nc) } else { (nc, na) };
                                        if ti < n && is_degen(n, mesh) && nkey == key {
                                            let pa = mesh.verts[a as usize].as_array();
                                            let pc = mesh.verts[c as usize].as_array();
                                            let e = [pc[0] - pa[0], pc[1] - pa[1], pc[2] - pa[2]];
                                            let l2 = e[0] * e[0] + e[1] * e[1] + e[2] * e[2];
                                            let tof = |v: u32| {
                                                let p = mesh.verts[v as usize].as_array();
                                                ((p[0] - pa[0]) * e[0]
                                                    + (p[1] - pa[1]) * e[1]
                                                    + (p[2] - pa[2]) * e[2])
                                                    / l2
                                            };
                                            let (tb, tnb) = (tof(b), tof(nb));
                                            let (bl, bh) =
                                                if tb <= tnb { (b, nb) } else { (nb, b) };
                                            let attr_of = |t: usize| {
                                                attr_vec.get(t).and_then(|o| o.as_ref()).map(|at| {
                                                    (matches!(at.input, InputId::A), at.face)
                                                })
                                            };
                                            let probe_edge = |u: u32, v: u32| {
                                                let k = if u < v { (u, v) } else { (v, u) };
                                                match edge_tris.get(&k) {
                                                    Some(list) => {
                                                        let others: Vec<_> = list
                                                            .iter()
                                                            .map(|&x| x as usize)
                                                            .filter(|&x| x != ti && x != n)
                                                            .map(|x| {
                                                                (x, is_degen(x, mesh), attr_of(x))
                                                            })
                                                            .collect();
                                                        format!(
                                                            "inc={} others={others:?}",
                                                            list.len()
                                                        )
                                                    }
                                                    None => "missing".into(),
                                                }
                                            };
                                            eprintln!(
                                                "YANG_LRR_MUTUAL tri={ti} partner={n} \
                                                 long=({a},{c}) offs=({b}@{tb:.4},{nb}@{tnb:.4}) \
                                                 pair_attr=({:?},{:?}) \
                                                 edge_bLc[{}] edge_abH[{}]",
                                                attr_of(ti),
                                                attr_of(n),
                                                probe_edge(bl, c),
                                                probe_edge(a, bh),
                                            );
                                        }
                                    }
                                }
                            }
                            eprintln!("YANG_LRR_STOP site=degenerate_no_longedge ndeg={ndeg}");
                            // Grounding: for each attribution carrying a degenerate
                            // triangle, size the same-attribution tri set and count
                            // its boundary edges (undirected edges used exactly once).
                            let mut deg_attrs: std::collections::BTreeSet<(u8, u32)> =
                                std::collections::BTreeSet::new();
                            for ti in 0..mesh.tris.len() {
                                if is_degen(ti, mesh) {
                                    if let Some(at) = attr_vec.get(ti).and_then(|o| o.as_ref()) {
                                        let ik = matches!(at.input, InputId::A) as u8;
                                        deg_attrs.insert((ik, at.face));
                                    }
                                }
                            }
                            for (ik, face) in &deg_attrs {
                                let want = |ti: usize| {
                                    attr_vec.get(ti).and_then(|o| o.as_ref()).is_some_and(|at| {
                                        (matches!(at.input, InputId::A) as u8) == *ik
                                            && at.face == *face
                                    })
                                };
                                let patch_tris: Vec<u32> = (0..mesh.tris.len() as u32)
                                    .filter(|&t| want(t as usize))
                                    .collect();
                                let mut edge_ct: std::collections::HashMap<(u32, u32), u32> =
                                    std::collections::HashMap::new();
                                let mut ndeg_in = 0usize;
                                for &t in &patch_tris {
                                    if is_degen(t as usize, mesh) {
                                        ndeg_in += 1;
                                    }
                                    let tri = mesh.tris[t as usize];
                                    for (i, j) in [(0, 1), (1, 2), (2, 0)] {
                                        let (u, v) = (tri[i], tri[j]);
                                        let k = if u < v { (u, v) } else { (v, u) };
                                        *edge_ct.entry(k).or_insert(0) += 1;
                                    }
                                }
                                let bnd = edge_ct.values().filter(|&&c| c == 1).count();
                                let nonmanifold = edge_ct.values().filter(|&&c| c > 2).count();
                                // θ/z span of the patch in the cylinder frame (if
                                // this face is a Cylinder), to decide seam-wrap.
                                let br = if *ik == 1 { brep_a } else { brep_b };
                                let mut span_str = String::from("(not cylinder)");
                                if let Surface::Cylinder {
                                    axis_point,
                                    axis_dir,
                                    ..
                                } = br.faces()[*face as usize].surface
                                {
                                    let (e1, e2) = ortho_basis(axis_dir);
                                    let au = normalize3(axis_dir.as_array());
                                    let o = axis_point.as_array();
                                    let mut verts_set: std::collections::BTreeSet<u32> =
                                        std::collections::BTreeSet::new();
                                    for &t in &patch_tris {
                                        for &v in &mesh.tris[t as usize] {
                                            verts_set.insert(v);
                                        }
                                    }
                                    let th_ref = {
                                        let p = mesh.verts
                                            [*verts_set.iter().next().unwrap() as usize]
                                            .as_array();
                                        let r = [p[0] - o[0], p[1] - o[1], p[2] - o[2]];
                                        let x = r[0] * e1.x() + r[1] * e1.y() + r[2] * e1.z();
                                        let y = r[0] * e2.x() + r[1] * e2.y() + r[2] * e2.z();
                                        y.atan2(x)
                                    };
                                    let (mut th_lo, mut th_hi, mut z_lo, mut z_hi) = (
                                        f64::INFINITY,
                                        f64::NEG_INFINITY,
                                        f64::INFINITY,
                                        f64::NEG_INFINITY,
                                    );
                                    for &v in &verts_set {
                                        let p = mesh.verts[v as usize].as_array();
                                        let r = [p[0] - o[0], p[1] - o[1], p[2] - o[2]];
                                        let x = r[0] * e1.x() + r[1] * e1.y() + r[2] * e1.z();
                                        let y = r[0] * e2.x() + r[1] * e2.y() + r[2] * e2.z();
                                        let z = r[0] * au[0] + r[1] * au[1] + r[2] * au[2];
                                        // Unwrap θ near th_ref.
                                        let mut th = y.atan2(x) - th_ref;
                                        while th > std::f64::consts::PI {
                                            th -= 2.0 * std::f64::consts::PI;
                                        }
                                        while th < -std::f64::consts::PI {
                                            th += 2.0 * std::f64::consts::PI;
                                        }
                                        th_lo = th_lo.min(th);
                                        th_hi = th_hi.max(th);
                                        z_lo = z_lo.min(z);
                                        z_hi = z_hi.max(z);
                                    }
                                    span_str = format!(
                                        "theta_span={:.4} (pi={:.4}) z_span={:.4} nverts={}",
                                        th_hi - th_lo,
                                        std::f64::consts::PI,
                                        z_hi - z_lo,
                                        verts_set.len()
                                    );
                                }
                                eprintln!(
                                    "YANG_LRR_PATCH input={} face={face} n_tris={} n_degen={ndeg_in} \
                                     boundary_edges={bnd} nonmanifold_edges={nonmanifold} {span_str}",
                                    if *ik == 1 { "A" } else { "B" },
                                    patch_tris.len()
                                );
                            }
                        }
                        // N2 §4.4.1: try re-meshing degenerate CYLINDER patches
                        // in their (θ,z) parametric domain (keep-interior CDT — no
                        // geometry moves). If it re-meshed, re-scan the loop; the
                        // `max_passes` guard bounds any pathological repeat.
                        //
                        // BANKED (task #168): gated OFF by default — proven safe
                        // (gate-ON corpus twice measured 0-WRONG and per-case
                        // identical: 2026-07-01 at 295 cases, 2026-08-15 post-I2e
                        // at 312 cases BYTE-IDENTICAL) but 0 conversions: the sole
                        // firing case (R0038) is a plane-tangent-cylinder whose
                        // degenerate caps are load-bearing conformal seam triangles,
                        // so the re-CDT self-rejects at the degree-2 boundary gate
                        // (spec yang_n2_stage4_cdt_mesh_updating.md §5c.10, §5c.12).
                        // Re-entry: a genuine simple degenerate-cylinder strip case,
                        // or the two-sided junction-aware machinery (epic #169 C/D).
                        // Enable with `YANG_N2_RECDT_ENABLE` for development.
                        if std::env::var_os("YANG_N2_RECDT_ENABLE").is_some() {
                            match replan_degenerate_cylinder_patches(
                                mesh,
                                &mut attr_vec,
                                &moved,
                                brep_a,
                                brep_b,
                            ) {
                                Ok(true) => continue,
                                Ok(false) => {}
                                Err(e) => {
                                    attribution.attributions = attr_vec;
                                    return Err(e);
                                }
                            }
                        }
                        attribution.attributions = attr_vec;
                        return Err(YangError::stage4_region_invalid(
                            u32::MAX,
                            Stage4InvalidReason::LocalRefinementRequired,
                        ));
                    }
                    break; // no degenerate relocated triangles remain
                }
            };
            // Split N=[a,c,d] at b → [a,b,d] + [b,c,d], wound like N; drop D.
            if !seen_actions.insert((d_idx, n_idx, a, c, b)) {
                if std::env::var_os("YANG_LRR_PROBE").is_some() {
                    eprintln!(
                        "YANG_LRR_STOP site=split_cycle d={d_idx} n={n_idx} a={a} c={c} b={b} \
                         pass={passes}"
                    );
                }
                attribution.attributions = attr_vec;
                return Err(YangError::stage4_region_invalid(
                    u32::MAX,
                    Stage4InvalidReason::LocalRefinementRequired,
                ));
            }
            if std::env::var_os("YANG_LRR_PROBE").is_some() {
                // Shape census of the acted-on pair: D's absolute area, the
                // off-vertex height over the long edge, the long edge length,
                // and N's area — separates "collinear" (height ≪ edge) from
                // "merely small at model scale" (height ~ edge, area < floor).
                let pa = mesh.verts[a as usize].as_array();
                let pc = mesh.verts[c as usize].as_array();
                let pb = mesh.verts[b as usize].as_array();
                let ac = [pc[0] - pa[0], pc[1] - pa[1], pc[2] - pa[2]];
                let l_ac = (ac[0] * ac[0] + ac[1] * ac[1] + ac[2] * ac[2]).sqrt();
                let av = tri_area_vector(pa, pc, pb);
                let area_d = (av[0] * av[0] + av[1] * av[1] + av[2] * av[2]).sqrt() * 0.5;
                let height_b = if l_ac > 0.0 {
                    2.0 * area_d / l_ac
                } else {
                    f64::NAN
                };
                let nt = mesh.tris[n_idx];
                let avn = tri_area_vector(
                    mesh.verts[nt[0] as usize].as_array(),
                    mesh.verts[nt[1] as usize].as_array(),
                    mesh.verts[nt[2] as usize].as_array(),
                );
                let area_n = (avn[0] * avn[0] + avn[1] * avn[1] + avn[2] * avn[2]).sqrt() * 0.5;
                eprintln!(
                    "YANG_LRR_ACTION simple d={d_idx} n={n_idx} a={a} c={c} b={b} \
                     area_d={area_d:.3e} l_ac={l_ac:.3e} height_b={height_b:.3e} \
                     h_over_l={:.3e} area_n={area_n:.3e} band={DEGENERACY_IDENTITY_REL:.1e}",
                    if l_ac > 0.0 {
                        height_b / l_ac
                    } else {
                        f64::NAN
                    }
                );
            }
            let nt = mesh.tris[n_idx];
            let dd = nt
                .iter()
                .copied()
                .find(|&v| v != a && v != c)
                .expect("neighbour shares edge a-c, has a third vertex");
            let n_norm = tri_area_vector(
                mesh.verts[nt[0] as usize].as_array(),
                mesh.verts[nt[1] as usize].as_array(),
                mesh.verts[nt[2] as usize].as_array(),
            );
            let mut t1 = [a, b, dd];
            let mut t2 = [b, c, dd];
            orient_tri(&mesh.verts, &mut t1, n_norm);
            orient_tri(&mesh.verts, &mut t2, n_norm);
            let n_attr = attr_vec.get(n_idx).copied().flatten();
            // Rebuild tris + attribution, dropping D and N, appending the split.
            let mut new_tris: Vec<[u32; 3]> = Vec::with_capacity(mesh.tris.len() + 1);
            let mut new_attr: Vec<Option<TriangleAttribution>> =
                Vec::with_capacity(attr_vec.len() + 1);
            for (i, t) in mesh.tris.iter().enumerate() {
                if i == d_idx || i == n_idx {
                    continue;
                }
                new_tris.push(*t);
                new_attr.push(attr_vec.get(i).copied().flatten());
            }
            new_tris.push(t1);
            new_attr.push(n_attr);
            new_tris.push(t2);
            new_attr.push(n_attr);
            *mesh = Mesh::new(std::mem::take(&mut mesh.verts), new_tris);
            attr_vec = new_attr;
            collapsed_any = true;
        }
        attribution.attributions = attr_vec;
        balance_census(mesh, "post-degen-loop");
    }

    // KV9-F3 diagnosis probe (read-only, env-gated): census near-twin mesh
    // vertex pairs at Stage-4 exit with their merge-eligibility context —
    // `moved` membership, shared-triangle adjacency, curve assignments.
    if std::env::var_os("YANG_S4_TWIN_PROBE").is_some() {
        let n = mesh.verts.len();
        let scale = mesh
            .verts
            .iter()
            .flat_map(|p| p.as_array())
            .fold(1.0_f64, |m, c| m.max(c.abs()));
        let band = 1.0e-9 * scale;
        for i in 0..n {
            for j in (i + 1)..n {
                let (p, q) = (mesh.verts[i].as_array(), mesh.verts[j].as_array());
                let d2 = (p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2);
                if d2 > band * band || d2 == 0.0 {
                    continue;
                }
                let (iu, ju) = (i as u32, j as u32);
                let shared_tri = mesh
                    .tris
                    .iter()
                    .position(|t| t.contains(&iu) && t.contains(&ju));
                eprintln!(
                    "[s4-twin-probe] verts {i}/{j} dist={:e} moved=({},{}) shared_tri={:?}\n  \
                     circle=({},{}) line=({},{}) ell=({},{}) junction=({},{})\n  \
                     {i}: ({},{},{})\n  {j}: ({},{},{})",
                    d2.sqrt(),
                    moved.contains(&iu),
                    moved.contains(&ju),
                    shared_tri,
                    vert_circle.contains_key(&iu),
                    vert_circle.contains_key(&ju),
                    vert_line.contains_key(&iu),
                    vert_line.contains_key(&ju),
                    vert_ellipse.contains_key(&iu),
                    vert_ellipse.contains_key(&ju),
                    vert_ell_junction.contains_key(&iu),
                    vert_ell_junction.contains_key(&ju),
                    p[0],
                    p[1],
                    p[2],
                    q[0],
                    q[1],
                    q[2]
                );
            }
        }
    }

    // (4) Validate every RELOCATED triangle (one touching a moved vertex) for
    // non-degeneracy (Yang §4.5 step 4). Reversed intersections are handled by
    // the §4.5.3 sweep above; watertightness by the global gate below (§4.4.3).
    validate_relocated_triangles(mesh, attribution, &moved)?;
    // (4a1) Doubled-membrane removal (spec `yang_doubled_membrane_removal.md`,
    // task #146 χ=3 sub-layer): drop opposite-winding coincident-triangle fins
    // (a zero-volume artifact of a backtrack-spike / near-tangent junction)
    // BEFORE the shell gate reads χ. Volume- and edge-balance-preserving; it
    // leaves the spur apex dangling for `compact_unreferenced_verts`, so it
    // rides the same Phase-A recompute path as a §4.5.3 collapse.
    let membranes_removed = remove_doubled_membranes(mesh);
    if membranes_removed > 0 {
        collapsed_any = true;
    }
    // (4a2) Tangency pinch-vertex split (spec `yang_tangency_pinch_split.md`):
    // uniform per-sheet representation of self-touching union boundaries
    // BEFORE the shell gate reads χ. Splitting appends vertices (a topology
    // change), so it rides the same Phase-A recompute path as a §4.5.3
    // collapse via the returned flag.
    let pinch_splits = split_pinch_vertices(mesh, &mut relocations);
    if pinch_splits > 0 {
        collapsed_any = true;
    }
    // (4b') #169 Phase-0 failure-region probe: before the gate fires, report the
    // non-manifold seam regions + their patch pairs + whether each patch has a
    // SurfaceChart (Plane/Cylinder) — the §4.4.1 mesh-update worklist. Gated on
    // `YANG_MESHUP_REGION`, so byte-identical when unset (no production change).
    if std::env::var_os("YANG_MESHUP_REGION").is_some() {
        let regions = crate::stage4_project::detect_nonmanifold_seams(&mesh.tris, &|ti| {
            attribution
                .lookup(ti as u32)
                .map(|at| (matches!(at.input, InputId::A), at.face))
        });
        for r in &regions {
            eprintln!(
                "YANG_MESHUP_REGION n_edges={} keys={:?} edges={:?}",
                r.edges.len(),
                r.keys,
                r.edges
            );
            for &(is_a, face) in &r.keys {
                let br = if is_a { brep_a } else { brep_b };
                let surf = br.faces()[face as usize].surface;
                eprintln!(
                    "  key ({is_a},{face}) surface={surf:?} has_chart={}",
                    crate::stage4_project::SurfaceChart::new(surf).is_some()
                );
            }
            // Local topology dump: for each region vertex, its coords + every
            // incident triangle (verts + attribution) — reveals whether the
            // mismatch is a shared-seam subdivision, a T-junction, or a floating
            // triangle, so the mesh-update operation can be chosen correctly.
            let rverts: std::collections::BTreeSet<u32> =
                r.edges.iter().flat_map(|&(s, e)| [s, e]).collect();
            for &v in &rverts {
                eprintln!("  v{v} = {:?}", mesh.verts[v as usize]);
            }
            for ti in 0..mesh.tris.len() {
                let t = mesh.tris[ti];
                if t.iter().any(|v| rverts.contains(v)) {
                    let k = attribution
                        .lookup(ti as u32)
                        .map(|at| (matches!(at.input, InputId::A), at.face));
                    eprintln!("  tri{ti} {t:?} attr={k:?}");
                }
            }
        }
    }
    // (4b') #169 Phase B §4.4.1 mesh-update: re-triangulate the non-manifold
    // planar patches (keep-boundary re-CDT — drops spurious overlapping triangles
    // like F0082's tri1217) BEFORE the gate. Gated on `YANG_MESHUP_ENABLE`, so
    // byte-identical when unset (production keeps the loud STOP). Any malformed
    // boundary is a loud STOP inside the remesh, never a silent-wrong.
    if std::env::var_os("YANG_MESHUP_ENABLE").is_some() {
        let mut attr_vec = std::mem::take(&mut attribution.attributions);
        let r = remesh_nonmanifold_patches(mesh, &mut attr_vec, brep_a, brep_b);
        attribution.attributions = attr_vec;
        r?;
    }
    // #194 (spec `yang_194_subtauwork_edge_collapse`): collapse mesh edges
    // below WORKING precision BEFORE the watertightness gate — the F0082
    // Extrude-12 operand-self-graze twin (same junction minted twice with
    // swapped LPI roles, 5.5e-14 apart, edge-connected; its zero-area flap's
    // third edge use is the χ=3 book edge THIS gate stops on). Byte-identical
    // no-op when no such edge exists; `collapsed_any` routes the caller into
    // the standard compact + Phase-A recompute.
    {
        let mut attr_vec = std::mem::take(&mut attribution.attributions);
        let c = collapse_subtauwork_mesh_edges(mesh, &mut attr_vec);
        attribution.attributions = attr_vec;
        collapsed_any |= c;
    }

    // (4b) Explicit Stage-4 watertightness gate (§4.4.3).
    if let Err(gate_err) = check_watertight_2manifold(mesh) {
        // #195 probe-only forensics: attribute every double-cover-edge triangle
        // to its input B-Rep face (operand + face id + surface) so the
        // self-overlap self-localizes to the producing emission. Byte-identical
        // when the probe env is unset (the gate error is returned unchanged).
        if std::env::var("NONMANIFOLD_SITE_PROBE").is_ok() {
            let mut dir: std::collections::BTreeMap<(u32, u32), i32> =
                std::collections::BTreeMap::new();
            for tri in &mesh.tris {
                for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
                    *dir.entry((tri[i], tri[j])).or_insert(0) += 1;
                }
            }
            for (&(s, e), &fwd) in &dir {
                // Report an undirected edge once (canonical `s < e`) when it is
                // either double-covered (`fwd >= 2`) or UNBALANCED (`fwd != rev`,
                // the gate's own failure condition — the doubling may sit on the
                // reverse direction, which the old `s < e && fwd >= 2` form
                // silently skipped).
                let rev = dir.get(&(e, s)).copied().unwrap_or(0);
                if s < e && (fwd >= 2 || rev >= 2 || fwd != rev) {
                    eprintln!(
                        "NONMANIFOLD_SITE_PROBE s4-dc-attr edge ({s},{e}) fwd={fwd} rev={rev} \
                         p{s}={:?} p{e}={:?}",
                        mesh.verts[s as usize].as_array(),
                        mesh.verts[e as usize].as_array()
                    );
                    for (ti, tri) in mesh.tris.iter().enumerate() {
                        let uses = tri.contains(&s) && tri.contains(&e);
                        if uses {
                            let attr = attribution.lookup(ti as u32);
                            let surf = attr.map(|at| {
                                let br = if matches!(at.input, InputId::A) {
                                    brep_a
                                } else {
                                    brep_b
                                };
                                (at.input, at.face, br.faces()[at.face as usize].surface)
                            });
                            // Direction this triangle presents the edge in, plus
                            // its off-vertex position — two pages of a book edge
                            // whose off-vertices COINCIDE are a duplicated sheet
                            // (the #146 upstream class), which the ids alone hide.
                            let dirn = if tri
                                .iter()
                                .zip([tri[1], tri[2], tri[0]])
                                .any(|(&u, v)| u == s && v == e)
                            {
                                "fwd"
                            } else {
                                "rev"
                            };
                            let off = tri.iter().copied().find(|&v| v != s && v != e);
                            let offp = off.map(|v| mesh.verts[v as usize].as_array());
                            eprintln!(
                                "NONMANIFOLD_SITE_PROBE s4-dc-attr   tri {ti}: {tri:?} {dirn} \
                                 off={off:?} offp={offp:?} attr={surf:?}"
                            );
                        }
                    }
                }
            }
        }
        return Err(gate_err);
    }

    // §4.4.1 boundary-curve relocation (spec `yang_s4_boundary_curve_relocation.md`,
    // inc-2). Yang Fig. 11 requires the trimmed triangulation to "map boundary
    // curves to boundary curves", which includes an operand's OWN rim — the
    // case `build_intersection_curves` never claims (`input0 == input1`). Runs
    // LAST so every cross-input junction is already seated and can be excluded
    // by construction.
    //
    // ALWAYS-ON since inc-5 (was `YANG_S4_RIM_SNAP_ENABLE`); flipped together
    // with the §4.5.4 rim×plane graze refinement in `boolean`, which depends on
    // it — see that function's note for the corpus measurement.
    //
    // Phase A is recomputed here rather than reusing `inc0`/`curves0`: the mesh
    // has been relocated and possibly collapsed since, so the earlier maps can
    // reference stale vertices.
    {
        let (_infos_bc, inc_bc, curves_bc) = compute_phase_a(
            mesh,
            attribution,
            brep_a,
            brep_b,
            &crate::stage3_ssi::NO_EDGE_PROVENANCE,
        )?;
        let rim_curves = crate::stage4_boundary_curve::collect_rim_curves(&inc_bc);
        // Per-vertex exclusion diagnosis: `YANG_S4_RIM_SNAP_TARGET=x,y,z,r`
        // reports, for every mesh vertex within `r` of the given point, each
        // incident incidence edge and WHICH of the pass's filters dropped it.
        // The pass claiming rim edges but moving nothing says nothing about
        // WHY; this does.
        if let Ok(spec) = std::env::var("YANG_S4_RIM_SNAP_TARGET") {
            let f: Vec<f64> = spec
                .split(',')
                .filter_map(|s| s.trim().parse().ok())
                .collect();
            if f.len() == 4 {
                let (tx, ty, tz, tr) = (f[0], f[1], f[2], f[3]);
                let mut cross: std::collections::BTreeSet<u32> = Default::default();
                for &(s, e) in curves_bc.keys() {
                    cross.insert(s);
                    cross.insert(e);
                }
                for (vi, p) in mesh.verts.iter().enumerate() {
                    let pa = p.as_array();
                    let d =
                        ((pa[0] - tx).powi(2) + (pa[1] - ty).powi(2) + (pa[2] - tz).powi(2)).sqrt();
                    if d > tr {
                        continue;
                    }
                    let v = vi as u32;
                    eprintln!(
                        "[rim-target] v={v} dist_to_target={d:.6e} p={pa:?} \
                         cross_excluded={}",
                        cross.contains(&v)
                    );
                    // Which surfaces does this vertex ACTUALLY satisfy? A
                    // triple point must satisfy all three; the implicit value
                    // separates "on it" from "near it".
                    {
                        let mut surfs: Vec<(InputId, Surface)> = Vec::new();
                        for (&(s2, e2), entries) in &inc_bc {
                            if s2 != v && e2 != v {
                                continue;
                            }
                            for &(i, sf) in entries {
                                if !surfs.iter().any(|(i2, s3)| *i2 == i && *s3 == sf) {
                                    surfs.push((i, sf));
                                }
                            }
                        }
                        for (i, sf) in surfs {
                            let f = surface_value_and_normal(sf, p.as_array()).map(|(f, _)| f);
                            eprintln!(
                                "[rim-target]   SURFACE {i:?}:{} implicit_value={f:?}",
                                surface_kind_name(sf)
                            );
                        }
                    }
                    let mut seen_any = false;
                    for (&(s, e), entries) in &inc_bc {
                        if s != v && e != v {
                            continue;
                        }
                        seen_any = true;
                        let kinds: Vec<String> = entries
                            .iter()
                            .map(|(i, sf)| format!("{i:?}:{}", surface_kind_name(*sf)))
                            .collect();
                        let same_input = entries.len() == 2 && entries[0].0 == entries[1].0;
                        let diff_surf = entries.len() == 2 && entries[0].1 != entries[1].1;
                        let circle = if entries.len() == 2 {
                            crate::stage4_boundary_curve::rim_circle_from_pair(
                                entries[0].1,
                                entries[1].1,
                            )
                        } else {
                            None
                        };
                        let claimed = rim_curves.contains_key(&(s, e));
                        eprintln!(
                            "[rim-target]   edge=({s},{e}) entries={kinds:?} \
                             same_input={same_input} diff_surf={diff_surf} \
                             circle={} claimed={claimed}",
                            circle.is_some()
                        );
                        if let Some(c) = circle {
                            // Per-edge self-derived bound: this chord's OWN
                            // sagitta, r*(1-cos(dtheta/2)) over its endpoints'
                            // angular span — the guarantee Stage 1 makes for
                            // THIS chord, not a global aggregate over the
                            // owner's rims.
                            if let (Some(&p0), Some(&p1)) =
                                (mesh.verts.get(s as usize), mesh.verts.get(e as usize))
                            {
                                if let Curve::Circle {
                                    center,
                                    normal,
                                    radius,
                                } = c
                                {
                                    let cc = center.as_array();
                                    let nn = normal.as_array();
                                    let radial = |p: Point3| {
                                        let a = p.as_array();
                                        let d = [a[0] - cc[0], a[1] - cc[1], a[2] - cc[2]];
                                        let h = d[0] * nn[0] + d[1] * nn[1] + d[2] * nn[2];
                                        [d[0] - h * nn[0], d[1] - h * nn[1], d[2] - h * nn[2]]
                                    };
                                    let (r0, r1) = (radial(p0), radial(p1));
                                    let n0 = (r0[0] * r0[0] + r0[1] * r0[1] + r0[2] * r0[2]).sqrt();
                                    let n1 = (r1[0] * r1[0] + r1[1] * r1[1] + r1[2] * r1[2]).sqrt();
                                    let cosang = ((r0[0] * r1[0] + r0[1] * r1[1] + r0[2] * r1[2])
                                        / (n0 * n1))
                                        .clamp(-1.0, 1.0);
                                    let span = cosang.acos();
                                    let sagitta = radius * (1.0 - (span / 2.0).cos());
                                    eprintln!(
                                        "[rim-target]     chord span={:.6}deg own_sagitta={sagitta:.6e}                                          global_bound={:.6e}",
                                        span.to_degrees(),
                                        bound_probe(brep_a, brep_b)
                                    );
                                }
                            }
                            for w in [s, e] {
                                if let Some(&wp) = mesh.verts.get(w as usize) {
                                    let proj =
                                        crate::stage4_boundary_curve::project_onto_curve(wp, &c);
                                    let dd = proj.map(|q| {
                                        let (x, y) = (wp.as_array(), q.as_array());
                                        ((x[0] - y[0]).powi(2)
                                            + (x[1] - y[1]).powi(2)
                                            + (x[2] - y[2]).powi(2))
                                        .sqrt()
                                    });
                                    eprintln!(
                                        "[rim-target]     endpoint v={w} resid={dd:?} \
                                         in_band={:?}",
                                        dd.map(|x| x <= bound_probe(brep_a, brep_b))
                                    );
                                }
                            }
                        }
                    }
                    if !seen_any {
                        eprintln!("[rim-target]   NO incidence edge contains this vertex");
                    }
                }
            }
        }
        // §4.3.3 Case-IV corner-phantom census (spec
        // `specs/yang_433_case_iv_corner_phantom.md` inc-0): read-only,
        // print-only, gated. Runs at this postcondition because `inc_bc` is
        // the recomputed POST-relocation incidence — the vantage where the
        // paper's "no solution in the parametric domains" clause is testable.
        if std::env::var_os("YANG_433_PHANTOM").is_some() {
            crate::stage4_phantom::census_case_iv_phantom(mesh, brep_a, brep_b, &inc_bc);
        }
        if !rim_curves.is_empty() {
            // Vertices claimed by a CROSS-input curve are A×B junctions that
            // must lie on BOTH curves; moving one would break that.
            let mut cross_endpoints: std::collections::BTreeSet<u32> = Default::default();
            for &(s, e) in curves_bc.keys() {
                cross_endpoints.insert(s);
                cross_endpoints.insert(e);
            }
            // The bound is the owner's own Stage-1 chord guarantee. Both
            // operands' rims are candidates, so take the larger of the two
            // budgets — a vertex beyond even that is not this class and STOPs.
            let bound = [InputId::A, InputId::B]
                .into_iter()
                .filter_map(|i| {
                    crate::stage3_ssi::chord_tol_for_curved_owner(i, brep_a, brep_b, 0, (0, 0)).ok()
                })
                .fold(0.0f64, f64::max);
            if bound > 0.0 {
                let moves = crate::stage4_boundary_curve::plan_boundary_relocations(
                    mesh,
                    &rim_curves,
                    &inc_bc,
                    &cross_endpoints,
                    bound,
                );
                // Census (spec §19): which of these snaps move a vertex that
                // carries a surface the projection never consumed? Read-only,
                // and taken BEFORE the apply so `mesh` still holds the pre-snap
                // positions.
                if std::env::var_os("YANG_S4_UNCONSUMED_PROBE").is_some() {
                    crate::stage4_boundary_curve::census_unconsumed_surfaces(
                        mesh,
                        &moves,
                        &inc_bc,
                        &rim_curves,
                    );
                }
                let n = crate::stage4_boundary_curve::apply_boundary_relocations(mesh, &moves);
                // inc-3 (spec §11): the Fig-11 point q — a vertex on the
                // operand's own rim AND on an A×B curve — must be re-seated at
                // the TRIPLE point, not projected onto either curve alone.
                // Separate gate so the two classes measure independently.
                if crate::stage4_boundary_curve::triple_point_enabled() {
                    let tp = crate::stage4_boundary_curve::plan_triple_point_reseats(
                        mesh,
                        &inc_bc,
                        &rim_curves,
                        &cross_endpoints,
                    );
                    let tn = crate::stage4_boundary_curve::apply_boundary_relocations(mesh, &tp);
                    if std::env::var_os("YANG_S4_RIM_SNAP_PROBE").is_some() {
                        eprintln!("[s4-triple-point] candidates={} reseated={tn}", tp.len());
                        for (v, q) in &tp {
                            eprintln!("[s4-triple-point]   v={v} -> {:?}", q.as_array());
                        }
                    }
                }
                if std::env::var_os("YANG_S4_RIM_SNAP_PROBE").is_some() {
                    eprintln!(
                        "[s4-rim-snap] rim_edges={} cross_excluded={} bound={bound:.6e} moved={n}",
                        rim_curves.len(),
                        cross_endpoints.len()
                    );
                    for (v, q) in &moves {
                        eprintln!("[s4-rim-snap]   v={v} -> {:?}", q.as_array());
                    }
                }
            }
        }
    }

    // After a collapse the vertex set may have lost some relocated verts; keep
    // only relocations whose vertex still carries a conic output edge. The
    // caller resolves the output-edge index; relocations referencing a
    // now-absent vertex are simply not emitted (the caller guards the index).
    relocation_domain_postcondition(mesh, attribution, brep_a, brep_b, &s4_entry_pos)?;

    Ok((relocations, collapsed_any))
}

mod reversal;
pub(crate) use reversal::*;

mod validate;
pub(crate) use validate::*;

/// Diagnostic helper for `YANG_S4_RIM_SNAP_TARGET`: the same bound the rim-snap
/// pass uses (the larger of the two operands' Stage-1 chord budgets).
fn bound_probe(a: &BRep, b: &BRep) -> f64 {
    [InputId::A, InputId::B]
        .into_iter()
        .filter_map(|i| crate::stage3_ssi::chord_tol_for_curved_owner(i, a, b, 0, (0, 0)).ok())
        .fold(0.0f64, f64::max)
}

/// Diagnostic helper: short surface-kind name for probe output.
pub(crate) fn surface_kind_name(s: Surface) -> &'static str {
    match s {
        Surface::Plane { .. } => "Plane",
        Surface::Cylinder { .. } => "Cylinder",
        Surface::Cone { .. } => "Cone",
        Surface::Sphere { .. } => "Sphere",
        Surface::Torus { .. } => "Torus",
    }
}

#[cfg(test)]
mod mutual_pair_tests {
    use super::*;

    /// The §5c.11 "pillow" fixture: a closed, coherently wound 8-triangle
    /// surface whose equator carries a mutual degenerate pair — T1=[c,bl,a] and
    /// T2=[bh,c,a] astride the shared long edge (a,c), off-vertices bl/bh
    /// interleaved strictly inside the segment (t = 0.25 / 0.6, off-line height
    /// 1e-13 → areas 5e-14 < MIN_FEATURE_SIZE²).
    fn pillow() -> (Mesh, Vec<Option<TriangleAttribution>>, MutualPair) {
        let d = 1.0e-13;
        let verts = vec![
            Point3::new(0.0, 0.0, 0.0),  // 0 = a
            Point3::new(1.0, 0.0, 0.0),  // 1 = c
            Point3::new(0.25, d, 0.0),   // 2 = bl
            Point3::new(0.6, -d, 0.0),   // 3 = bh
            Point3::new(0.5, 1.0, 0.0),  // 4 = x (NL apex)
            Point3::new(0.5, -1.0, 0.0), // 5 = y (NH apex)
        ];
        let tris = vec![
            [0, 2, 4], // 0 M1
            [1, 4, 2], // 1 NL (outer across (bl,c))
            [1, 2, 0], // 2 T1 (degenerate, off = bl)
            [3, 1, 0], // 3 T2 (degenerate, off = bh)
            [3, 0, 5], // 4 NH (outer across (a,bh))
            [1, 3, 5], // 5 M2
            [5, 0, 4], // 6 E1
            [4, 1, 5], // 7 E2
        ];
        let mut attrs: Vec<Option<TriangleAttribution>> = vec![None; 8];
        attrs[1] = Some(TriangleAttribution {
            input: InputId::A,
            face: 7,
        });
        attrs[4] = Some(TriangleAttribution {
            input: InputId::B,
            face: 3,
        });
        let m = MutualPair {
            t1: 2,
            t2: 3,
            nl: 1,
            nh: 4,
            a: 0,
            c: 1,
            bl: 2,
            bh: 3,
        };
        (Mesh::new(verts, tris), attrs, m)
    }

    fn directed_edge_census(tris: &[[u32; 3]]) -> std::collections::HashMap<(u32, u32), usize> {
        let mut m = std::collections::HashMap::new();
        for t in tris {
            for (i, j) in [(0, 1), (1, 2), (2, 0)] {
                *m.entry((t[i], t[j])).or_insert(0) += 1;
            }
        }
        m
    }

    fn assert_closed_coherent(tris: &[[u32; 3]]) {
        let m = directed_edge_census(tris);
        for (&(u, v), &n) in &m {
            assert_eq!(n, 1, "directed edge ({u},{v}) used {n} times");
            assert_eq!(m.get(&(v, u)), Some(&1), "edge ({u},{v}) has no reverse");
        }
    }

    fn is_degen(mesh: &Mesh, t: [u32; 3]) -> bool {
        tri_is_degenerate(
            mesh.verts[t[0] as usize].as_array(),
            mesh.verts[t[1] as usize].as_array(),
            mesh.verts[t[2] as usize].as_array(),
        )
    }

    fn edge_incidence(tris: &[[u32; 3]]) -> std::collections::HashMap<(u32, u32), Vec<u32>> {
        let mut m: std::collections::HashMap<(u32, u32), Vec<u32>> =
            std::collections::HashMap::new();
        for (ti, tri) in tris.iter().enumerate() {
            for (i, j) in [(0, 1), (1, 2), (2, 0)] {
                let (u, v) = (tri[i], tri[j]);
                let key = if u < v { (u, v) } else { (v, u) };
                m.entry(key).or_default().push(ti as u32);
            }
        }
        m
    }

    #[test]
    fn pillow_fixture_is_closed_and_carries_the_pair() {
        let (mesh, _, m) = pillow();
        assert_closed_coherent(&mesh.tris);
        assert!(is_degen(&mesh, mesh.tris[m.t1]));
        assert!(is_degen(&mesh, mesh.tris[m.t2]));
    }

    #[test]
    fn resolve_drops_the_quad_and_stays_watertight_with_the_fine_chain() {
        let (mut mesh, mut attrs, m) = pillow();
        resolve_mutual_degenerate_pair(&mut mesh, &mut attrs, &m);
        assert_eq!(mesh.tris.len(), 8);
        assert_eq!(attrs.len(), 8);
        assert_closed_coherent(&mesh.tris);
        for &t in &mesh.tris {
            assert!(!is_degen(&mesh, t), "degenerate tri {t:?} survived");
        }
        // The long edge is gone; both sides carry the fine chain a–bl–bh–c.
        let und = edge_incidence(&mesh.tris);
        assert!(!und.contains_key(&(0, 1)), "long edge (a,c) survived");
        for k in [(0, 2), (2, 3), (1, 3)] {
            assert_eq!(
                und.get(&k).map(Vec::len),
                Some(2),
                "chain edge {k:?} not paired"
            );
        }
        // Split pieces inherit their parents' attributions.
        let n_a7 = attrs
            .iter()
            .flatten()
            .filter(|at| at.input == InputId::A && at.face == 7)
            .count();
        let n_b3 = attrs
            .iter()
            .flatten()
            .filter(|at| at.input == InputId::B && at.face == 3)
            .count();
        assert_eq!((n_a7, n_b3), (2, 2));
    }

    #[test]
    fn candidate_accepts_the_pair_and_rejects_equal_parameters() {
        let (mesh, _, want) = pillow();
        let edge_tris = edge_incidence(&mesh.tris);
        let is_degen = |ti: usize, mesh: &Mesh| is_degen(mesh, mesh.tris[ti]);
        let long_edge_off = |t: &[u32; 3], mesh: &Mesh| -> (u32, u32, u32) {
            let d = |i: usize, j: usize| {
                let p = mesh.verts[t[i] as usize].as_array();
                let q = mesh.verts[t[j] as usize].as_array();
                (p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)
            };
            let (e01, e12, e20) = (d(0, 1), d(1, 2), d(2, 0));
            if e01 >= e12 && e01 >= e20 {
                (t[0], t[1], t[2])
            } else if e12 >= e20 {
                (t[1], t[2], t[0])
            } else {
                (t[2], t[0], t[1])
            }
        };
        let (a, c, b) = long_edge_off(&mesh.tris[2], &mesh);
        let got =
            mutual_pair_candidate(&mesh, &edge_tris, &is_degen, &long_edge_off, 2, 3, a, c, b)
                .expect("mutual pair accepted");
        assert_eq!((got.t1, got.t2, got.nl, got.nh), (2, 3, want.nl, want.nh));
        assert_eq!((got.bl, got.bh), (want.bl, want.bh));
        // Same parameter along a→c (no deterministic chain order) → rejected.
        let mut mesh2 = pillow().0;
        mesh2.verts[2] = Point3::new(0.6, 1.0e-13, 0.0);
        let (a2, c2, b2) = long_edge_off(&mesh2.tris[2], &mesh2);
        assert!(mutual_pair_candidate(
            &mesh2,
            &edge_tris,
            &is_degen,
            &long_edge_off,
            2,
            3,
            a2,
            c2,
            b2
        )
        .is_none());
    }

    /// The SAME-APEX fan (measured on R0038): both outer neighbours across the
    /// two insertion edges share their third vertex, so they are one fan over
    /// the chain rather than two opposite sides. `nl`'s piece `[bl,bh,dd]` and
    /// `nh`'s piece `[bl,bh,dd]` are then the identical triangle and the update
    /// would emit it TWICE — a double cover. The candidate must reject, keeping
    /// the loud STOP. The second half proves the rejection is caused by the
    /// shared apex specifically: give `nh` its own apex and the SAME
    /// configuration is accepted.
    #[test]
    fn candidate_rejects_the_same_apex_fan() {
        let is_degen = |ti: usize, mesh: &Mesh| is_degen(mesh, mesh.tris[ti]);
        let long_edge_off = |t: &[u32; 3], mesh: &Mesh| -> (u32, u32, u32) {
            let d = |i: usize, j: usize| {
                let p = mesh.verts[t[i] as usize].as_array();
                let q = mesh.verts[t[j] as usize].as_array();
                (p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)
            };
            let (e01, e12, e20) = (d(0, 1), d(1, 2), d(2, 0));
            if e01 >= e12 && e01 >= e20 {
                (t[0], t[1], t[2])
            } else if e12 >= e20 {
                (t[1], t[2], t[0])
            } else {
                (t[2], t[0], t[1])
            }
        };
        // Chain a–bl–bh–c collinear (t = 0.41 / 0.68, off-line height 1e-13 so
        // both quad members are degenerate), apex 4 shared by BOTH neighbours.
        let dy = 1.0e-13;
        let verts = vec![
            Point3::new(0.0, 0.0, 0.0),  // 0 = a
            Point3::new(1.0, 0.0, 0.0),  // 1 = c
            Point3::new(0.41, dy, 0.0),  // 2 = bl
            Point3::new(0.68, -dy, 0.0), // 3 = bh
            Point3::new(0.5, 1.0, 0.0),  // 4 = shared apex
            Point3::new(0.5, -1.0, 0.0), // 5 = distinct apex (two-sided case)
        ];
        let same_apex = vec![
            [1, 2, 0], // 0 T1 degenerate, off = bl
            [3, 1, 0], // 1 T2 degenerate, off = bh
            [1, 4, 2], // 2 NL across (bl,c), third = 4
            [3, 0, 4], // 3 NH across (a,bh), third = 4  ← same apex
        ];
        let mesh = Mesh::new(verts.clone(), same_apex);
        let edge_tris = edge_incidence(&mesh.tris);
        let (a, c, b) = long_edge_off(&mesh.tris[0], &mesh);
        assert!(
            is_degen(0, &mesh) && is_degen(1, &mesh),
            "quad is degenerate"
        );
        assert!(
            !is_degen(2, &mesh) && !is_degen(3, &mesh),
            "both outer neighbours are non-degenerate"
        );
        assert!(
            mutual_pair_candidate(&mesh, &edge_tris, &is_degen, &long_edge_off, 0, 1, a, c, b)
                .is_none(),
            "same-apex fan must be rejected — the arm would double-cover [bl,bh,dd]"
        );

        // Identical configuration, `nh` re-apexed to its own vertex: accepted,
        // and the four split pieces are then distinct.
        let two_sided = vec![
            [1, 2, 0], // T1
            [3, 1, 0], // T2
            [1, 4, 2], // NL, third = 4
            [3, 0, 5], // NH, third = 5  ← distinct
        ];
        let mesh2 = Mesh::new(verts, two_sided);
        let edge_tris2 = edge_incidence(&mesh2.tris);
        let (a2, c2, b2) = long_edge_off(&mesh2.tris[0], &mesh2);
        let got = mutual_pair_candidate(
            &mesh2,
            &edge_tris2,
            &is_degen,
            &long_edge_off,
            0,
            1,
            a2,
            c2,
            b2,
        )
        .expect("two-sided pair still accepted");
        assert_eq!((got.nl, got.nh), (2, 3));
        assert_eq!((got.bl, got.bh), (2, 3));
    }
}
