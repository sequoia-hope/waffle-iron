#[allow(clippy::wildcard_imports)]
use super::*;

/// §4.5.2 UNDER-RESOLUTION certificate (census, spec
/// `specs/yang_452_local_refinement.md`): the Stage-1 chord bound of ONE face —
/// how far that face's own tessellation may deviate from its exact surface.
/// A `Plane` is tessellated exactly (0); every curved kind reports its own
/// Stage-1 budget, the SAME functions Stage 1 sized its density from
/// (`fix_all_gates_sharing_a_metric`). `None` = no bound derivable (a curved
/// face whose density came from the solid's rim AABB — the caller falls back
/// to `input_curved_chord_bound`).
pub(crate) fn face_chord_bound(f: &BRepFace, edges: &[crate::BRepEdge]) -> Option<f64> {
    use crate::stage1_tessellate::{
        cone_chord_bound, curved_chord_bound, sphere_chord_bound, torus_chord_bound,
    };
    match f.surface {
        Surface::Plane { .. } => Some(0.0),
        Surface::Sphere { radius, .. } => Some(sphere_chord_bound(radius)),
        Surface::Torus {
            major_radius,
            minor_radius,
            ..
        } => Some(torus_chord_bound(major_radius, minor_radius)),
        Surface::Cone {
            apex,
            axis_dir,
            half_angle,
            ..
        } => {
            // The band's own height: the largest axial extent of its rims.
            let h = f
                .outer_loop
                .iter()
                .chain(f.inner_loops.iter().flatten())
                .filter_map(|&ei| edges.get(ei as usize))
                .filter_map(|e| match e.curve {
                    Curve::Circle { center, .. } => {
                        let d = [
                            center.as_array()[0] - apex.as_array()[0],
                            center.as_array()[1] - apex.as_array()[1],
                            center.as_array()[2] - apex.as_array()[2],
                        ];
                        let a = axis_dir.as_array();
                        Some((d[0] * a[0] + d[1] * a[1] + d[2] * a[2]).abs())
                    }
                    _ => None,
                })
                .fold(0.0f64, f64::max);
            (h > 0.0).then(|| cone_chord_bound(h, half_angle))
        }
        // A cylinder lateral samples at the rim density, whose budget is the
        // solid's circle-rim AABB band — a per-SOLID quantity the caller owns.
        Surface::Cylinder { .. } => curved_chord_bound(edges),
    }
}

/// One Yang §4.5 boundary-point domain fire: a relocated vertex whose step
/// `pre → post` left face `face` of input `input` across its crease `edge`
/// (`f_pre` / `f_post` the signed divider distances, see
/// `boundary_crease_crossed`). `incident` is every `(input, face)` the
/// vertex's live triangles are attributed to — the faces whose surfaces
/// meet at the site.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DomainFire {
    pub(crate) v: u32,
    pub(crate) input: InputId,
    pub(crate) face: u32,
    pub(crate) edge: u32,
    pub(crate) f_pre: f64,
    pub(crate) f_post: f64,
    pub(crate) pre: [f64; 3],
    pub(crate) post: [f64; 3],
    pub(crate) incident: Vec<(InputId, u32)>,
}

/// Yang §4.5 boundary-point DOMAIN certificate, as a Stage-4 postcondition
/// (spec `specs/yang_45_boundary_point_domain_certificate.md`).
///
/// The paper collects "the point pairs that cannot converge to a distance of
/// 0 within their domains" (`refs/text/yang2025_hybrid_boolean.txt:656-659`)
/// and sends the boundary points among them — one operand's model edge
/// gliding across the other's face (Fig. 13's exclusion from §4.5.1) — to
/// §4.5.2 local refinement. The relocation arms solve such a vertex exactly
/// on the EXTENDED surfaces and never ask whether the root is on the bounded
/// face; this pass asks, for every vertex Stage 4 moved, against every face
/// its attributed triangles carry: did the step cross one of that face's
/// creases ([`crate::stage4_boundary_curve::boundary_crease_crossed`])?
///
/// A fire is the paper's own §4.5.2 trigger. Measured on P0003: two fires
/// (the B rim circle past the boss's top edge, 9.9e-4; the boss's top edge
/// past the torus rim, 1.6e-3); the ladder's `d_ε/2` rung emits with no
/// fire and is adopted: SUPPORTED_CORRECT.
///
/// Gate `YANG_S45_BOUNDARY_DOMAIN`: unset = **ON** — the fires are RECORDED
/// (`boundary_domain_fires`, trigger T2 of `boolean::failures`) and
/// `boolean::refine_loop` refines the faces they name; a run is adopted only
/// with an EMPTY failure set, and fires the loop cannot clear STOP as
/// `RefinementExhausted` (the loud ceiling, 2026-10-16, ledger N66 — before
/// it the natural output was kept, and R0049 / R0070 shipped with fires the
/// loop could not reach); `stop`/`1`/`on` = the hard STOP
/// (the A/B measurement knob); `census` = report only, no ladder (the
/// `0`/`off` certificate-off switch was deleted 2026-10-16).
pub(crate) fn boundary_domain_postcondition(
    mesh: &Mesh,
    attribution: &TriangleAttributionMap,
    a: &BRep,
    b: &BRep,
    entry: &[[f64; 3]],
) -> Result<(), YangError> {
    use crate::stage4_boundary_curve::{boundary_crease_crossed, CreaseIndex};
    let mode = std::env::var("YANG_S45_BOUNDARY_DOMAIN").unwrap_or_default();
    // `census` and the production default both REPORT (the default records
    // the count for the op-level ladder, `boolean::refine_452_domain`);
    // only `stop` turns a fire into the hard STOP here.
    let census = mode == "census";
    let stop = mode == "stop" || mode == "1" || mode == "on";
    let n = entry.len().min(mesh.verts.len());
    let patches = build_patch_map(mesh, &attribution.attributions);
    let mut index: [Option<CreaseIndex>; 2] = [None, None];
    let mut creases_of =
        |input: InputId, face: u32| -> Vec<crate::stage4_boundary_curve::FaceCrease> {
            let (slot, brep) = match input {
                InputId::A => (0usize, a),
                InputId::B => (1usize, b),
            };
            let idx = index[slot].get_or_insert_with(|| CreaseIndex::build(brep));
            idx.faces.get(face as usize).cloned().unwrap_or_default()
        };
    // (vertex, input, face, edge, f_pre, f_post) per fire, deterministic order.
    let mut fires: Vec<(u32, InputId, u32, u32, f64, f64)> = Vec::new();
    let mut records: Vec<DomainFire> = Vec::new();
    for (&v, faces) in &patches {
        let i = v as usize;
        if i >= n {
            continue; // minted during Stage 4: no pre position, no step
        }
        let (pre, post) = (entry[i], mesh.verts[i].as_array());
        if pre == post {
            continue;
        }
        let (p, q) = (
            Point3::new(pre[0], pre[1], pre[2]),
            Point3::new(post[0], post[1], post[2]),
        );
        for &(input, face) in faces {
            let creases = creases_of(input, face);
            if let Some(fire) = boundary_crease_crossed(p, q, &creases) {
                fires.push((v, input, face, fire.edge, fire.f_pre, fire.f_post));
                records.push(DomainFire {
                    v,
                    input,
                    face,
                    edge: fire.edge,
                    f_pre: fire.f_pre,
                    f_post: fire.f_post,
                    pre,
                    post,
                    incident: faces.iter().copied().collect(),
                });
                if census {
                    eprintln!(
                        "YANG_S45_BOUNDARY_DOMAIN v{v} left {input:?}:{face} across edge {} \
                         f_pre={:.4e} f_post={:.4e} travel={:.4e} \
                         pre=({:.9},{:.9},{:.9}) post=({:.9},{:.9},{:.9})",
                        fire.edge,
                        fire.f_pre,
                        fire.f_post,
                        ((post[0] - pre[0]).powi(2)
                            + (post[1] - pre[1]).powi(2)
                            + (post[2] - pre[2]).powi(2))
                        .sqrt(),
                        pre[0],
                        pre[1],
                        pre[2],
                        post[0],
                        post[1],
                        post[2],
                    );
                }
            }
        }
    }
    // §4.5 trigger T2 (`boolean::failures`, spec `yang_452_refinement_loop.md`
    // §3): every fire is a failure site naming the surfaces C_p traverses —
    // the span from the fire along the intersection polyline to the nearest
    // successfully optimized points (spec `yang_452_cp_region.md` §3; a fire
    // vertex lies ON its extended surfaces, so the fire set is excluded
    // from `good` explicitly). The one refinement loop reads them from there.
    if !records.is_empty() {
        let graph = IsectGraph::new(mesh, &attribution.attributions);
        let fired: std::collections::BTreeSet<u32> = records.iter().map(|r| r.v).collect();
        let good = |w: u32| !fired.contains(&w) && vertex_converged(mesh, &patches, a, b, w);
        let mut spanned = records.clone();
        for r in &mut spanned {
            let (members, bounds, capped) = graph.cp_span(r.v, &good);
            let span = cp_span_faces(&patches, &members, &bounds);
            if std::env::var_os("YANG_452_PROBE").is_some() {
                eprintln!(
                    "[cp-span] T2 v{} members={} bounds={:?}{} faces {} → {}",
                    r.v,
                    members.len(),
                    bounds.iter().take(4).collect::<Vec<_>>(),
                    if capped { " capped" } else { "" },
                    r.incident.len(),
                    span.len()
                );
            }
            if !span.is_empty() {
                r.incident = span;
            }
        }
        crate::boolean::failures::record_domain_fires(&spanned);
    }
    if fires.is_empty() {
        return Ok(());
    }
    let n_verts = fires
        .iter()
        .map(|f| f.0)
        .collect::<std::collections::BTreeSet<u32>>()
        .len();
    if census {
        eprintln!(
            "YANG_S45_BOUNDARY_DOMAIN SUMMARY fires={} verts={n_verts}",
            fires.len(),
        );
    }
    // Corpus inventory: the subprocess runner discards a worker's stderr, so
    // every mode also APPENDS one line per fire to the file named by
    // `YANG_S45_BOUNDARY_DOMAIN_LOG`, keyed by the worker's `ASSAY_CASE` and
    // the rung in force (best effort, never an error).
    if let Some(path) = std::env::var_os("YANG_S45_BOUNDARY_DOMAIN_LOG") {
        use std::io::Write as _;
        let case = std::env::var("ASSAY_CASE").unwrap_or_default();
        let rung = crate::stage1_tessellate::chord_refine_scale();
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            for &(v, input, face, edge, f_pre, f_post) in &fires {
                let _ = writeln!(
                    f,
                    "{case} rung={rung} v{v} {input:?}:{face} edge={edge} f_pre={f_pre:.4e} \
                     f_post={f_post:.4e}"
                );
            }
        }
    }
    if !stop {
        return Ok(());
    }
    // `stop` mode: the hard wall (the A/B knob the corpus measurement uses;
    // spec §7 records why it is not the production default — many-facet
    // operands carry out-of-domain relocations their oracles survive, and
    // the ladder cannot reach them). No under-resolution certificate: the
    // overrun past the crease is not the true junction's clearance from the
    // corner, so no rung can be named from it.
    let (v0, ..) = fires[0];
    Err(YangError::stage4_region_invalid(
        v0,
        Stage4InvalidReason::RelocationCrossedCrease,
    ))
}

pub(crate) fn relocation_domain_postcondition(
    mesh: &Mesh,
    attribution: &TriangleAttributionMap,
    a: &BRep,
    b: &BRep,
    entry: &[[f64; 3]],
) -> Result<(), YangError> {
    let census = std::env::var("YANG_S4_CARRIER_DOMAIN").as_deref() == Ok("census");

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
                    // §4.5.2 UNDER-RESOLUTION certificate (census; spec
                    // `specs/yang_452_local_refinement.md` §6). The §4-I9 fire
                    // has two mechanisms, and the FAR surface's own Stage-1
                    // chord bound separates them:
                    //
                    //  * REAL TRANSIT (the §4.5.1 corner-transit family): the
                    //    far surface's zero lies MACROSCOPICALLY past the
                    //    corner, so `|d_far(q)| >> d_eps(far)`. The mesh knows
                    //    which side of `far` the corner is on; the crossing is
                    //    real and must be re-routed onto the next model edge.
                    //  * UNDER-RESOLUTION (the paper's §4.5.2 customer): the
                    //    corner sits WITHIN the far face's own chord band,
                    //    `|d_far(q)| <= d_eps(far)`, so the far mesh CANNOT
                    //    decide which side of `far` the corner lies on and the
                    //    discrete crossing may not exist at all. Refinement is
                    //    the paper's remedy (§4.5.2); no transit exists to
                    //    plan.
                    for fp in &far {
                        let (brep, label) = match fp.0 {
                            InputId::A => (a, "A"),
                            InputId::B => (b, "B"),
                        };
                        let Some(ff) = brep.faces().get(fp.1 as usize) else {
                            continue;
                        };
                        let de = face_chord_bound(ff, brep.edges())
                            .or_else(|| input_curved_chord_bound(brep));
                        let dq =
                            surface_distance_and_normal(ff.surface, qpos).map(|(f, _)| f.abs());
                        let verdict = match (dq, de) {
                            (Some(dq), Some(de)) if dq <= de => "UNDER-RESOLVED(452)",
                            (Some(_), Some(_)) => "RESOLVED(real-transit)",
                            _ => "NO-VERDICT",
                        };
                        eprintln!(
                            "YANG_S4_CARRIER_DOMAIN-RESOLUTION v{v} q=v{q} far={label}:{} \
                             d_far_q={} d_eps_far={} ratio={} {verdict}",
                            fp.1,
                            dq.map_or("None".to_string(), |x| format!("{x:.6e}")),
                            de.map_or("None".to_string(), |x| format!("{x:.6e}")),
                            match (dq, de) {
                                (Some(dq), Some(de)) if de > 0.0 => format!("{:.4e}", dq / de),
                                _ => "inf".to_string(),
                            },
                        );
                    }
                    // CORNER PROBE (report-only, census): q's own MODEL
                    // incidence. The corner-incident-edge rule reads
                    // `d_q_end <= band` on the winning edge; when every
                    // candidate refuses as `not-corner-incident` the question
                    // is whether q is a model CORNER at all — so list, per
                    // operand, the nearest B-Rep vertex to q and every edge of
                    // the far/shared/next faces' loops with q's own reading
                    // against it (endpoint residual, curve-aware distance,
                    // domain verdict of q ITSELF on that edge).
                    if std::env::var("YANG_451_CORNER_PROBE").is_ok() {
                        let all: Vec<(InputId, u32)> = pv.union(pq).copied().collect();
                        for input in [InputId::A, InputId::B] {
                            let brep = match input {
                                InputId::A => a,
                                InputId::B => b,
                            };
                            let (verts, edges, faces) =
                                (brep.vertices(), brep.edges(), brep.faces());
                            if !all.iter().any(|&(i2, _)| i2 == input) {
                                continue;
                            }
                            let near = verts
                                .iter()
                                .enumerate()
                                .map(|(vi, bv)| {
                                    let pp = bv.point.as_array();
                                    (vi, {
                                        let d = [pp[0] - qpos[0], pp[1] - qpos[1], pp[2] - qpos[2]];
                                        (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
                                    })
                                })
                                .min_by(|x, y| x.1.total_cmp(&y.1));
                            eprintln!(
                                "YANG_S4_CARRIER_DOMAIN-CORNER v{v} q=v{q} {input:?} \
                                 nearest_brep_vertex={near:?} nverts={}",
                                verts.len()
                            );
                            for &(i2, fi) in &all {
                                if i2 != input {
                                    continue;
                                }
                                if let Some(f) = faces.get(fi as usize) {
                                    eprintln!(
                                        "YANG_S4_CARRIER_DOMAIN-CORNER   v{v} SURFACE \
                                         {input:?}:{fi} nloops={} nedges_outer={} {:?}",
                                        1 + f.inner_loops.len(),
                                        f.outer_loop.len(),
                                        f.surface,
                                    );
                                }
                            }
                            let mut seen = std::collections::BTreeSet::new();
                            for &(i2, fi) in &all {
                                if i2 != input {
                                    continue;
                                }
                                let Some(f) = faces.get(fi as usize) else {
                                    continue;
                                };
                                for &ei in f.outer_loop.iter().chain(f.inner_loops.iter().flatten())
                                {
                                    if !seen.insert(ei) {
                                        continue;
                                    }
                                    let e = &edges[ei as usize];
                                    let er = crate::stage4_transit::edge_domain_of(
                                        verts, edges, ei, qpos, qpos,
                                    );
                                    let ps = verts[e.start as usize].point.as_array();
                                    let pe = verts[e.end as usize].point.as_array();
                                    eprintln!(
                                        "YANG_S4_CARRIER_DOMAIN-CORNER   v{v} {input:?}:{fi} \
                                         edge={ei} v{}->v{} d_q_end={:.3e} d_q_edge={:.3e} \
                                         q_domain={:?} start=({:.6},{:.6},{:.6}) \
                                         end=({:.6},{:.6},{:.6}) curve={:?}",
                                        e.start,
                                        e.end,
                                        er.d_q_end,
                                        er.d_on_edge,
                                        er.domain,
                                        ps[0],
                                        ps[1],
                                        ps[2],
                                        pe[0],
                                        pe[1],
                                        pe[2],
                                        e.curve,
                                    );
                                }
                            }
                        }
                    }
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
                // The STOP carries the §4.5.2 under-resolution certificate
                // measured over EVERY fire of the invocation (not only this
                // first one): the op-level ladder reads it to pick the rung
                // that resolves every crossed corner at once instead of
                // climbing through rungs the certificate already predicts
                // futile (R0085 measured d_ε/2 … d_ε/16 all failing on the
                // same chain while its demand read 18.2 — d_ε/32 converges).
                let fires_all = corner_crossing_fires(mesh, attribution, a, b, entry);
                let ratio = under_resolution_ratio(mesh, attribution, a, b, &fires_all);
                return Err(YangError::stage4_region_invalid_under_resolved(
                    v,
                    Stage4InvalidReason::RelocationCrossedCarrierVertex,
                    ratio,
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
                    //
                    // inc-2c-3b-2 (spec §3j): the ABSORB measurements. The
                    // FaceId(402) ring spike is the chain-end neighbour
                    // itself sitting PAST its minted junction with an
                    // anti-parallel connector — a non-corner out-of-domain
                    // slide §4-I9 cannot fire on. Before the absorb arm is
                    // built, measure per chain-end neighbour at an END
                    // junction: distance to the junction vs d_eps and the
                    // contract band, the connector-direction dot (arrival
                    // = prev→w, connector = w→J; negative = the emitted
                    // ring doubles back at w), signed surface values at w
                    // and prev against the junction's three surfaces (far
                    // + the two walk-op faces — which sign names "past the
                    // crease"), the chain spacing d(prev,w), and the
                    // corner's far value for the removability-sign
                    // comparison.
                    let junc_surfs = |ji: usize| -> [Option<Surface>; 3] {
                        let j = &c.junctions[ji];
                        [
                            face_surface(c.far.0, c.far.1),
                            face_surface(c.walk_op, j.faces.0),
                            face_surface(c.walk_op, j.faces.1),
                        ]
                    };
                    let sval = |s: Option<Surface>, pos: [f64; 3]| -> Option<f64> {
                        s.and_then(|s| surface_value_and_normal(s, pos).map(|(f, _)| f))
                    };
                    let fmt3 = |v: [Option<f64>; 3]| -> String {
                        v.iter()
                            .map(|x| match x {
                                Some(f) => format!("{f:+.3e}"),
                                None => "?".to_string(),
                            })
                            .collect::<Vec<_>>()
                            .join(",")
                    };
                    for &q in &c.corners {
                        let qpos = mesh.verts[q as usize].as_array();
                        let f_far = sval(face_surface(c.far.0, c.far.1), qpos);
                        eprintln!(
                            "YANG_S4_CARRIER_DOMAIN-CYCLES  #{k} ABSORB corner v{q} \
                             far_q={}",
                            fmt3([f_far, None, None]),
                        );
                    }
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
                            let last = c.junctions.len() - 1;
                            let [ji] = hits.as_slice() else { continue };
                            if *ji != 0 && *ji != last {
                                continue;
                            }
                            let jpos = c.junctions[*ji].sol;
                            let wpos = mesh.verts[w as usize].as_array();
                            let scale = jpos.iter().fold(0.0f64, |m, &x| m.max(x.abs()));
                            let surfs = junc_surfs(*ji);
                            let f_w = [
                                sval(surfs[0], wpos),
                                sval(surfs[1], wpos),
                                sval(surfs[2], wpos),
                            ];
                            eprintln!(
                                "YANG_S4_CARRIER_DOMAIN-CYCLES  #{k} ABSORB v{w} junc{ji} \
                                 d_j={:.3e} d_eps={:.3e} contract={:.3e} \
                                 f_w(far,f0,f1)=({})",
                                d3c(wpos, jpos),
                                d_eps,
                                crate::stage4_transit::contract_band(scale),
                                fmt3(f_w),
                            );
                            // The chain continuation: w's own on-curve
                            // neighbours, excluding the corridor's
                            // phantoms and corners. Uniqueness is itself
                            // a measurement (the absorb re-splice needs
                            // exactly one).
                            for &u in adj.get(&w).into_iter().flatten() {
                                if !on_curve(u) || c.phantoms.contains(&u) || c.corners.contains(&u)
                                {
                                    continue;
                                }
                                let upos = mesh.verts[u as usize].as_array();
                                let arrive =
                                    [wpos[0] - upos[0], wpos[1] - upos[1], wpos[2] - upos[2]];
                                let connect =
                                    [jpos[0] - wpos[0], jpos[1] - wpos[1], jpos[2] - wpos[2]];
                                let norm =
                                    |x: [f64; 3]| (x[0] * x[0] + x[1] * x[1] + x[2] * x[2]).sqrt();
                                let (na, nc) = (norm(arrive), norm(connect));
                                let dot = if na > 0.0 && nc > 0.0 {
                                    (arrive[0] * connect[0]
                                        + arrive[1] * connect[1]
                                        + arrive[2] * connect[2])
                                        / (na * nc)
                                } else {
                                    f64::NAN
                                };
                                let f_u = [
                                    sval(surfs[0], upos),
                                    sval(surfs[1], upos),
                                    sval(surfs[2], upos),
                                ];
                                eprintln!(
                                    "YANG_S4_CARRIER_DOMAIN-CYCLES  #{k} ABSORB v{w} \
                                     prev v{u} dot={dot:+.4} d_prev_w={:.3e} \
                                     d_prev_j={:.3e} f_prev(far,f0,f1)=({})",
                                    na,
                                    d3c(upos, jpos),
                                    fmt3(f_u),
                                );
                            }
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
                    // Kept in LOCKSTEP with `corner_transit_apply`'s
                    // `hosts_on` — the inc-2c-3b-4 arc-host admission
                    // (`arc_host_admit`: eval arm verbatim + the certified
                    // arc arm, junction-on-the-chord's-own-curve at the
                    // contract band).
                    let hosts_on = |k: usize,
                                    patch: &crate::stage5_topology::Patch,
                                    cycles_v: &[Vec<u32>]|
                     -> Vec<s4c::HostEdge> {
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
                            let eval = cad_primitives::TAU_WORK.max(8.0 * f64::EPSILON * scale);
                            let contract = crate::stage4_transit::contract_band(scale);
                            let val_at = |inp: InputId, f: u32| -> Option<f64> {
                                let s = face_surface(inp, f)?;
                                surface_value_and_normal(s, j.sol).map(|(v, _)| v)
                            };
                            let on_curve = |x: u32, y: u32| -> bool {
                                val_at(patch.attribution.input, patch.attribution.face)
                                    .is_some_and(|v| v.abs() <= contract)
                                    && mesh.tris.iter().enumerate().any(|(ti, tri)| {
                                        tri.contains(&x)
                                            && tri.contains(&y)
                                            && !patch.tri_indices.contains(&(ti as u32))
                                            && attribution
                                                .attributions
                                                .get(ti)
                                                .and_then(|a| a.as_ref())
                                                .and_then(|att| val_at(att.input, att.face))
                                                .is_some_and(|v| v.abs() <= contract)
                                    })
                            };
                            for cy in cycles_v {
                                let n = cy.len();
                                for i in 0..n {
                                    let (x, y) = (cy[i], cy[(i + 1) % n]);
                                    if crate::stage4_transit::arc_host_admit(
                                        j.sol,
                                        mesh.verts[x as usize].as_array(),
                                        mesh.verts[y as usize].as_array(),
                                        eval,
                                        d_eps,
                                        || on_curve(x, y),
                                    ) {
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
                        // Lockstep with the driver: phantom-membership keys
                        // join the affected set (inc-2c-3b-7).
                        let mut keys = s4c::affected_keys(c);
                        for &ph in &c.phantoms {
                            keys.extend(patches_of(ph).iter().copied());
                        }
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
                                let Some(cycles_v) = cycles_of(pi) else {
                                    inputs_ok = false;
                                    continue;
                                };
                                let hosts = hosts_on(k, patch, &cycles_v);
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
                        let vpos = |v: u32| -> Option<[f64; 3]> {
                            mesh.verts.get(v as usize).map(|p| p.as_array())
                        };
                        // Report-only census mirror: corridors here are
                        // unextended, so the standing certificate never
                        // fires; keep it inert.
                        let never_standing = |_: usize, _: u32| -> bool { false };
                        let pctx = s4c::PlanCtx {
                            far_value: &far_value,
                            band: &vband,
                            pos: &vpos,
                            attachments: &attachments,
                            hosts: &hosts,
                            standing: &never_standing,
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
                        for (k, comp, d) in &pdeclines {
                            eprintln!(
                                "YANG_S4_CARRIER_DOMAIN-PLAN3B DECLINE corridor=#{k} \
                                 comp={comp} {d:?}"
                            );
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
