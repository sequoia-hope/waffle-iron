#[allow(clippy::wildcard_imports)]
use super::*;

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
/// The intersection polyline of the kept submesh as a graph: an edge is an
/// intersection edge when its live triangles carry BOTH inputs (one triangle
/// per operand on the kept submesh; a Stage-0 overlap boundary qualifies, as
/// the paper's "overlap boundaries become intersection curves"). Built once
/// per vantage and walked by [`IsectGraph::cp_span`].
pub(crate) struct IsectGraph {
    adj: std::collections::BTreeMap<u32, Vec<u32>>,
}

/// Most vertices one C_p walk visits (a run where nothing converges stays
/// bounded; the cap is reported).
pub(crate) const CP_SPAN_CAP: usize = 512;

impl IsectGraph {
    pub(crate) fn new(mesh: &Mesh, attribution: &[Option<TriangleAttribution>]) -> Self {
        let mut sides: std::collections::BTreeMap<(u32, u32), (bool, bool)> =
            std::collections::BTreeMap::new();
        for (ti, tri) in mesh.tris.iter().enumerate() {
            let Some(Some(att)) = attribution.get(ti) else {
                continue;
            };
            for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
                let (u, w) = (tri[i], tri[j]);
                if u == w {
                    continue;
                }
                let e = sides.entry((u.min(w), u.max(w))).or_default();
                match att.input {
                    InputId::A => e.0 = true,
                    InputId::B => e.1 = true,
                }
            }
        }
        let mut adj: std::collections::BTreeMap<u32, Vec<u32>> = std::collections::BTreeMap::new();
        for (&(u, w), &(ha, hb)) in &sides {
            if ha && hb {
                adj.entry(u).or_default().push(w);
                adj.entry(w).or_default().push(u);
            }
        }
        IsectGraph { adj }
    }

    /// Yang §4.5.2's erroneous region around the failing point `v` (spec
    /// `yang_452_cp_region.md` §3): breadth-first along intersection edges
    /// through vertices that are not `good` (not "successfully optimized"),
    /// stopping at `good` ones. Returns `(members, bounds, capped)` — the
    /// members are C_p's failing points (`v` first), the bounds are p_f, p_b
    /// (more than two where the span passes a corner).
    pub(crate) fn cp_span(
        &self,
        v: u32,
        good: &dyn Fn(u32) -> bool,
    ) -> (Vec<u32>, std::collections::BTreeSet<u32>, bool) {
        let mut members: Vec<u32> = vec![v];
        let mut seen: std::collections::BTreeSet<u32> = std::collections::BTreeSet::from([v]);
        let mut bounds: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
        let mut queue: std::collections::VecDeque<u32> = std::collections::VecDeque::from([v]);
        let mut capped = false;
        while let Some(u) = queue.pop_front() {
            for &w in self.adj.get(&u).map(Vec::as_slice).unwrap_or(&[]) {
                if !seen.insert(w) {
                    continue;
                }
                if good(w) {
                    bounds.insert(w);
                } else if members.len() >= CP_SPAN_CAP {
                    capped = true;
                } else {
                    members.push(w);
                    queue.push_back(w);
                }
            }
        }
        (members, bounds, capped)
    }
}

/// The faces at every member and bound of a C_p span — "the surfaces
/// traversed by C_p" (Yang §4.5.2), in `(input, face)` order.
pub(crate) fn cp_span_faces(
    patches: &std::collections::BTreeMap<u32, std::collections::BTreeSet<(InputId, u32)>>,
    members: &[u32],
    bounds: &std::collections::BTreeSet<u32>,
) -> Vec<(InputId, u32)> {
    let mut out: std::collections::BTreeSet<(bool, u32)> = std::collections::BTreeSet::new();
    for w in members.iter().chain(bounds.iter()) {
        if let Some(fs) = patches.get(w) {
            for &(input, face) in fs {
                out.insert((matches!(input, InputId::B), face));
            }
        }
    }
    out.into_iter()
        .map(|(is_b, f)| (if is_b { InputId::B } else { InputId::A }, f))
        .collect()
}

pub(crate) fn build_patch_map(
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
pub(crate) fn build_live_adjacency(
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
pub(crate) fn vertex_on_curve(
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
pub(crate) fn vertex_converged(
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
pub(crate) fn census_failure_population(
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
pub(crate) fn carrier_counts(
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
pub(crate) fn carrier_surface_sets(
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
pub(crate) type CandidateRow = (
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
pub(crate) fn failure_population_census(
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
pub(crate) fn vertex_crossed_domain_endpoint(
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
/// points **on the same surface**"* (`refs/text/yang2025_hybrid_boolean.txt:651-657`).
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
pub(crate) fn selector_clause2_walk(
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
/// Yang 2025 §4.5 (`refs/text/yang2025_hybrid_boolean.txt:651-657`): *"we only
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
pub(crate) fn strategy_selection_census(
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

// inc-2c-3b-1 (spec `specs/yang_451_corner_transit.md` §3j): the
// corner-transit corridor mutation is ALWAYS-ON since inc-3a (2026-09-03;
// R0011 and, with the torus chart, R0074 converted). §4.5.1 inc-2c-3b-12b-8 —
// the EMISSION apply arm (write the certified
// [`crate::stage4_boundary_curve::TransitEmissionFill`] at a determined
// out-of-domain relocation site and move the site to its corrected junction)
// is ALWAYS-ON since inc-3c (2026-09-04; R0044 moved to the honest Stage-1
// partial-patch wall). Their `YANG_451_TRANSIT` / `YANG_451_TRANSIT_EMIT`
// off-knobs were deleted 2026-10-16 (cleanup spec §4).

/// The §4-I9 two-leg detection as a quiet fire list — the SAME rule
/// [`relocation_domain_postcondition`] STOPs on (a moved vertex crossed a
/// STILL neighbour that carries a surface the traveller is off), kept in
/// lockstep with it: any change there must land here too.
pub(crate) fn corner_crossing_fires(
    mesh: &Mesh,
    attribution: &TriangleAttributionMap,
    a: &BRep,
    b: &BRep,
    entry: &[[f64; 3]],
) -> Vec<(u32, u32)> {
    let n = entry.len().min(mesh.verts.len());
    let adj = build_live_adjacency(mesh);
    let mut fires = Vec::new();
    for (&v, neighbours) in &adj {
        let i = v as usize;
        if i >= n {
            continue;
        }
        let (pre, post) = (entry[i], mesh.verts[i].as_array());
        if pre == post {
            continue;
        }
        for &q in neighbours {
            let qi = q as usize;
            if qi >= n || entry[qi] != mesh.verts[qi].as_array() {
                continue;
            }
            let qpos = mesh.verts[qi].as_array();
            if !crate::stage4_construct::point_on_segment_interior(pre, post, qpos) {
                continue;
            }
            let lost = carried_surfaces(mesh, &attribution.attributions, a, b, q, qpos)
                .into_iter()
                .any(|surf| {
                    !surface_distance_and_normal(surf, post)
                        .is_some_and(|(f, _)| f.abs() <= junction_certificate_band(post, surf))
                });
            if lost {
                fires.push((v, q));
            }
        }
    }
    fires
}

/// Yang §4.5.2's UNDER-RESOLUTION certificate for a §4-I9 STOP (spec
/// `specs/yang_452_local_refinement.md` §8; the census verdict of
/// [`relocation_domain_postcondition`]'s `-RESOLUTION` line made a production
/// quantity). For every fire `(v, q)` and every FAR patch of the traveller
/// (a patch `v` carries that the crossed corner `q` does not), the far
/// surface's own Stage-1 chord band `d_ε(far)` against the corner's distance
/// to that surface `|d_far(q)|`: while `d_ε(far) ≥ |d_far(q)|` the far mesh
/// cannot decide which side of the far surface the corner lies on, so the
/// discrete crossing the arrangement produced may not exist at all and no
/// transit can be planned — refinement is the paper's remedy, and the ratio
/// is the factor the band must shrink by. The MAX over the invocation's
/// fires is the demand (R0085: the rider at the tooth corner nearest the
/// exact exit reads 18.2 while the STOP'd site itself reads 0.54 — the
/// ladder must resolve every corner the crossing chain touches). A corner
/// exactly ON the far surface has no finite demand and contributes nothing
/// (that is the transit planner's `JunctionAtCorner` class).
pub(crate) fn under_resolution_ratio(
    mesh: &Mesh,
    attribution: &TriangleAttributionMap,
    a: &BRep,
    b: &BRep,
    fires: &[(u32, u32)],
) -> Option<f64> {
    let patches = build_patch_map(mesh, &attribution.attributions);
    let empty: std::collections::BTreeSet<(InputId, u32)> = Default::default();
    let mut worst: Option<f64> = None;
    for &(v, q) in fires {
        let pv = patches.get(&v).unwrap_or(&empty);
        let pq = patches.get(&q).unwrap_or(&empty);
        let Some(qv) = mesh.verts.get(q as usize) else {
            continue;
        };
        let qpos = qv.as_array();
        for fp in pv.difference(pq) {
            let brep = match fp.0 {
                InputId::A => a,
                InputId::B => b,
            };
            let Some(ff) = brep.faces().get(fp.1 as usize) else {
                continue;
            };
            let Some(de) =
                face_chord_bound(ff, brep.edges()).or_else(|| input_curved_chord_bound(brep))
            else {
                continue;
            };
            let Some(dq) = surface_distance_and_normal(ff.surface, qpos).map(|(f, _)| f.abs())
            else {
                continue;
            };
            if dq <= 0.0 || dq.is_nan() || !de.is_finite() {
                continue;
            }
            let ratio = de / dq;
            worst = Some(worst.map_or(ratio, |w| w.max(ratio)));
        }
    }
    worst
}

/// inc-2c-3b-1 — the gated corner-transit CORRIDOR MUTATION (spec §3i).
///
/// Re-runs the measured pipeline quietly — site planner → contract-band
/// walks → corridor assembly → corrected-cycle planning — and, only when
/// EVERY admission certificate holds (every fired site consumed by an
/// applyable corridor, zero plan declines, every affected patch key
/// planned), mints the pool and replaces each planned component's
/// triangles with a wholesale re-CDT from its corrected cycles, as one
/// batch. Any refusal rolls back completely (appended mints truncated)
/// and returns `Ok(false)` — the standing §4-I9 STOP then fires exactly
/// as today. Never guesses (P10).
/// inc-2c-3b-6 — stitch a deleted fan region's link RUNS with the
/// corrected cycle's all-New arcs into one refill polygon (spec §3m). The
/// region's rim alternates surviving link chains with boundary gaps whose
/// replacement is the minted path; between consecutive runs exactly ONE
/// corrected-cycle arc with an all-New interior must connect them (an Old
/// inside an arc means the selection left the region — refused, typed by
/// message). `base` maps `CycleRef::New(i)` to mesh id `base + i`.
pub(crate) fn stitch_fan_polygon(
    corrected: &[Vec<crate::stage4_corridor::CycleRef>],
    runs: &[Vec<u32>],
    base: u32,
) -> Result<Vec<u32>, String> {
    use crate::stage4_corridor::CycleRef;
    let hits: Vec<&Vec<CycleRef>> = corrected
        .iter()
        .filter(|cy| cy.contains(&CycleRef::Old(runs[0][0])))
        .collect();
    let cyc: Vec<CycleRef> = match hits[..] {
        [one] => one.clone(),
        _ => {
            return Err(format!(
                "run end v{} on {} corrected cycles",
                runs[0][0],
                hits.len()
            ))
        }
    };
    let arc_from = |v: u32, forward: bool| -> Option<(Vec<u32>, u32)> {
        let n = cyc.len();
        let i = cyc.iter().position(|&r| r == CycleRef::Old(v))?;
        let mut interior = Vec::new();
        for step in 1..n {
            let idx = if forward {
                (i + step) % n
            } else {
                (i + n - step) % n
            };
            match cyc[idx] {
                CycleRef::New(m) => interior.push(base + m),
                CycleRef::Old(o) => return Some((interior, o)),
            }
        }
        None
    };
    let mut polygon: Vec<u32> = runs[0].clone();
    let mut used = vec![false; runs.len()];
    used[0] = true;
    let start_v = runs[0][0];
    loop {
        let cur = *polygon.last().expect("runs are non-empty");
        let all_used = used.iter().all(|&u| u);
        let mut cands: Vec<(Vec<u32>, u32)> = Vec::new();
        for forward in [true, false] {
            if let Some((interior, term)) = arc_from(cur, forward) {
                let wanted = if all_used {
                    term == start_v
                } else {
                    runs.iter()
                        .enumerate()
                        .any(|(ri, r)| !used[ri] && (r[0] == term || r.last() == Some(&term)))
                };
                if wanted && !cands.iter().any(|(i2, t2)| *t2 == term && *i2 == interior) {
                    cands.push((interior, term));
                }
            }
        }
        let (interior, term) = match cands.len() {
            1 => cands.remove(0),
            n2 => return Err(format!("{n2} candidate arcs from v{cur}")),
        };
        polygon.extend(interior);
        if all_used {
            return Ok(polygon); // the arc closed the ring back to start_v
        }
        let (ri, rev) = runs
            .iter()
            .enumerate()
            .find_map(|(ri, r)| {
                if used[ri] {
                    None
                } else if r[0] == term {
                    Some((ri, false))
                } else if r.last() == Some(&term) {
                    Some((ri, true))
                } else {
                    None
                }
            })
            .expect("wanted implies an unused run with this end");
        used[ri] = true;
        if rev {
            polygon.extend(runs[ri].iter().rev().copied());
        } else {
            polygon.extend(runs[ri].iter().copied());
        }
    }
}
