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
    // only `stop` (debug-only strengthener) turns a fire into the hard STOP here.
    let census = mode == "census";
    let stop = cfg!(debug_assertions) && (mode == "stop" || mode == "1" || mode == "on");
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
    let census = cfg!(debug_assertions)
        && std::env::var("YANG_S4_CARRIER_DOMAIN").as_deref() == Ok("census");

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
                // The patches of the traveller and of the corner it crossed:
                // `far` are the traveller's alone, `next` the corner's alone.
                {
                    let (pv, pq) = (patches_of(v), patches_of(q));
                    let far: Vec<_> = pv.difference(pq).collect();
                    let next: Vec<_> = pq.difference(pv).collect();
                    let shared: Vec<_> = pv.intersection(pq).collect();
                    eprintln!(
                        "YANG_S4_CARRIER_DOMAIN-PATCHES v{v} q=v{q} far={far:?} \
                         next={next:?} shared={shared:?}"
                    );
                    // §4.5.2 UNDER-RESOLUTION certificate (census; spec
                    // `specs/yang_452_local_refinement.md` §6). The FAR
                    // surface's own Stage-1 chord bound separates two kinds
                    // of fire:
                    //
                    //  * RESOLVED: the far surface's zero lies MACROSCOPICALLY
                    //    past the corner, `|d_far(q)| >> d_eps(far)`. The mesh
                    //    knows which side of `far` the corner is on, and the
                    //    relocation crossed a corner where more than two
                    //    surfaces meet.
                    //  * UNDER-RESOLVED: the corner sits WITHIN the far face's
                    //    own chord band, `|d_far(q)| <= d_eps(far)`, so the
                    //    far mesh CANNOT decide which side of `far` the corner
                    //    lies on and the discrete crossing may not exist at
                    //    all.
                    //
                    // The paper sends both to §4.5.2 refinement (§4.5.1,
                    // `:653-657`: "for other cases, we apply the second
                    // strategy"); the ratio tells the loop how far.
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
                            (Some(_), Some(_)) => "RESOLVED",
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

/// One curve-interior domain fire: the arc Stage 3 typed on the intersection
/// edge `edge` passes `depth` beyond crease `crease_edge` of face
/// `(input, face)`, on the side that face does not occupy.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CurveDomainFire {
    pub(crate) edge: (u32, u32),
    pub(crate) input: InputId,
    pub(crate) face: u32,
    pub(crate) crease_edge: u32,
    pub(crate) depth: f64,
    pub(crate) at: [f64; 3],
    /// Every `(input, face)` of the live triangles on the edge.
    pub(crate) incident: Vec<(InputId, u32)>,
}

/// The side of each crease's divider that ONE input face occupies, and the
/// margin within which that reading holds.
///
/// A crease's divider (`FaceCrease::divider`) is the surface a point must not
/// cross to leave the face through that crease. The face's Stage-1 mesh
/// vertices are points of the face; when every one of them that is off the
/// divider lies on ONE side, the face lies on that side, up to how far the
/// face can be from its own mesh — the largest deviation of its triangles
/// from its surface, and, for a face with a curved boundary, the operand's
/// rim chord bound (Yang §4.2.1: "the original surface is within a distance
/// tolerance d_ε from the mesh", `refs/text/yang2025_hybrid_boolean.txt:449-450`).
/// A divider with witnesses on both sides (a face that is not convex toward
/// that crease) gets no side: no certificate, never a guess.
pub(crate) struct FaceSides {
    /// Per crease of the face (the `CreaseIndex` order): `Some(±1.0)`.
    pub(crate) sign: Vec<Option<f64>>,
    pub(crate) margin: f64,
}

/// Metric signed distance of `p` to `s` (a cone's radial residual × cos α,
/// as `shell_contact` and T8 convert it).
fn metric_distance(s: Surface, p: [f64; 3]) -> Option<f64> {
    let scale = match s {
        Surface::Cone { half_angle, .. } => half_angle.cos(),
        _ => 1.0,
    };
    crate::geom::signed_distance_to_surface(s, Point3::new(p[0], p[1], p[2]))
        .ok()
        .map(|d| d * scale)
        .filter(|d| d.is_finite())
}

fn crease_band(c: &crate::stage4_boundary_curve::FaceCrease, x: [f64; 3]) -> f64 {
    junction_certificate_band(x, c.divider)
        + junction_certificate_band(x, c.s_own)
        + c.s_other.map_or(0.0, |s| junction_certificate_band(x, s))
}

pub(crate) fn face_sides(
    brep: &BRep,
    face: u32,
    creases: &[crate::stage4_boundary_curve::FaceCrease],
) -> FaceSides {
    let mesh = brep.as_mesh();
    let tri_face = brep.tri_face();
    let mut witnesses: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
    let mut dev = 0.0f64;
    if tri_face.len() == mesh.tris.len() {
        for (ti, tri) in mesh.tris.iter().enumerate() {
            if tri_face[ti] != face {
                continue;
            }
            witnesses.extend(tri.iter().copied());
            if let Some(f) = brep.faces().get(face as usize) {
                let pts = tri.map(|v| mesh.verts[v as usize].as_array());
                dev = dev.max(crate::shell_contact::surface_deviation(&f.surface, pts));
            }
        }
    }
    let curved_boundary = brep.faces().get(face as usize).is_some_and(|f| {
        f.outer_loop
            .iter()
            .chain(f.inner_loops.iter().flatten())
            .filter_map(|&ei| brep.edges().get(ei as usize))
            .any(|e| !matches!(e.curve, Curve::LineSegment))
    });
    let rim = if curved_boundary {
        input_curved_chord_bound(brep).unwrap_or(f64::INFINITY)
    } else {
        0.0
    };
    let sign = creases
        .iter()
        .map(|c| {
            let (mut pos, mut neg) = (false, false);
            for &v in &witnesses {
                let p = mesh.verts[v as usize].as_array();
                let g = metric_distance(c.divider, p)?;
                if g.abs() <= crease_band(c, p) {
                    continue;
                }
                if g > 0.0 {
                    pos = true;
                } else {
                    neg = true;
                }
            }
            match (pos, neg) {
                (true, false) => Some(1.0),
                (false, true) => Some(-1.0),
                _ => None,
            }
        })
        .collect();
    FaceSides {
        sign,
        margin: dev.max(rim),
    }
}

/// Yang §4.3 curve-interior DOMAIN certificate, as a Stage-4 postcondition.
///
/// The paper optimizes every intersection point onto both surfaces and
/// treats a point with "no solution in one of the two parametric domains" as
/// a solving failure (§4.3.3, `refs/text/yang2025_hybrid_boolean.txt:528-532`);
/// §4.3.4 (`:579-583`) then inserts, between two consecutive optimized
/// points `p` and `q`, the intersection point `m` their parameter midpoints
/// optimize to — an intersection point like any other, held to the same
/// domains. It is the paper's Fig. 12 (`:626-637`): "the intersection of the
/// meshes is shifted onto S2, completely bypassing S1". Stage 3 types a mesh
/// intersection edge with the SSI
/// curve through its two endpoints and never asks where that curve runs
/// BETWEEN them. This pass asks: the arc the output will carry on each typed
/// intersection edge is sampled, and each sample is read against the creases
/// of every input face the edge's triangles are attributed to
/// ([`face_sides`]). A sample beyond a crease's divider, on the side its face
/// does not occupy, by more than the face's own margin, is a point of the
/// intersection curve outside that face's domain.
///
/// Measured on F0076 moved rigidly (2026-11-14): the Stage-1 junction mints
/// (ledger N82) put a cylinder's rim samples exactly on a box wall's plane at
/// both ends of a 12° window with no regular sample inside it, so the meshes
/// touch along the chord between the two mints and the arrangement types
/// that chord as wall ∩ cylinder — an arc that runs 41 mm below the
/// cylinder's base rim, where the cylinder face does not exist. The body was
/// closed, on-surface, and held material neither operand has; the failure
/// set was empty.
///
/// A fire is a §4.5 site (`:656-659`: "we collect the point pairs that
/// cannot converge to a distance of 0 within their domains"), recorded for
/// `boolean::refine_loop` (trigger T9). `curves` is Stage 3's curve per
/// intersection edge of the mesh as Stage 4 leaves it. Tracing only:
/// `YANG_452_PROBE` prints each fire, and `YANG_S43_CURVE_DOMAIN_LOG=<path>`
/// appends one line per fire (the corpus runner discards stderr).
pub(crate) fn curve_domain_postcondition(
    mesh: &Mesh,
    attribution: &TriangleAttributionMap,
    curves: &std::collections::BTreeMap<(u32, u32), Curve>,
    a: &BRep,
    b: &BRep,
) {
    use crate::stage4_boundary_curve::CreaseIndex;
    if curves.values().all(|c| matches!(c, Curve::LineSegment)) {
        return;
    }
    // Typed edge → the (input, face) of every live triangle on it.
    let mut on_edge: std::collections::BTreeMap<
        (u32, u32),
        std::collections::BTreeSet<(InputId, u32)>,
    > = std::collections::BTreeMap::new();
    for (ti, tri) in mesh.tris.iter().enumerate() {
        let Some(Some(att)) = attribution.attributions.get(ti) else {
            continue;
        };
        for k in 0..3 {
            let (s, e) = (tri[k], tri[(k + 1) % 3]);
            let key = (s.min(e), s.max(e));
            if curves
                .get(&key)
                .is_some_and(|c| !matches!(c, Curve::LineSegment))
            {
                on_edge
                    .entry(key)
                    .or_default()
                    .insert((att.input, att.face));
            }
        }
    }
    let mut index: [Option<CreaseIndex>; 2] = [None, None];
    let mut sides: std::collections::BTreeMap<(InputId, u32), FaceSides> =
        std::collections::BTreeMap::new();
    let mut fires: Vec<CurveDomainFire> = Vec::new();
    for (&(s, e), faces) in &on_edge {
        let Some(curve) = curves.get(&(s, e)) else {
            continue;
        };
        let (ps, pe) = (mesh.verts[s as usize], mesh.verts[e as usize]);
        let chord = {
            let (x, y) = (ps.as_array(), pe.as_array());
            ((x[0] - y[0]).powi(2) + (x[1] - y[1]).powi(2) + (x[2] - y[2]).powi(2)).sqrt()
        };
        if chord.is_nan() || chord <= 0.0 {
            continue;
        }
        // The arc as the output reads it, to a hundredth of its chord; at
        // least eight interior samples.
        let mut tol = 1e-2 * chord;
        let mut samples: Vec<[f64; 3]> = Vec::new();
        for _ in 0..4 {
            match crate::boolean::failures::sample_output_edge(curve, ps, pe, false, tol) {
                Ok(pts) => samples = pts,
                Err(_) => {
                    samples.clear();
                    break;
                }
            }
            if samples.len() >= 10 {
                break;
            }
            tol /= 16.0;
        }
        if samples.len() < 3 {
            continue;
        }
        let interior = &samples[1..samples.len() - 1];
        for &(input, face) in faces {
            let (slot, brep) = match input {
                InputId::A => (0usize, a),
                InputId::B => (1usize, b),
            };
            let idx = index[slot].get_or_insert_with(|| CreaseIndex::build(brep));
            let Some(creases) = idx.faces.get(face as usize) else {
                continue;
            };
            let fs = sides
                .entry((input, face))
                .or_insert_with(|| face_sides(brep, face, creases));
            if !fs.margin.is_finite() {
                continue;
            }
            let mut worst: Option<(f64, u32, [f64; 3])> = None;
            for (c, sign) in creases.iter().zip(fs.sign.iter()) {
                let Some(sign) = *sign else {
                    continue;
                };
                for &m in interior {
                    let Some(g) = metric_distance(c.divider, m) else {
                        continue;
                    };
                    let depth = -(sign * g) - (crease_band(c, m) + fs.margin);
                    if depth > 0.0 && worst.is_none_or(|w| depth > w.0) {
                        worst = Some((depth, c.edge, m));
                    }
                }
            }
            if let Some((depth, crease_edge, at)) = worst {
                fires.push(CurveDomainFire {
                    edge: (s, e),
                    input,
                    face,
                    crease_edge,
                    depth,
                    at,
                    incident: faces.iter().copied().collect(),
                });
            }
        }
    }
    if fires.is_empty() {
        return;
    }
    if std::env::var_os("YANG_452_PROBE").is_some() {
        for f in &fires {
            eprintln!(
                "YANG_S43_CURVE_DOMAIN edge ({},{}) leaves {:?}:{} across edge {} by {:.4e} at \
                 ({:.9},{:.9},{:.9})",
                f.edge.0,
                f.edge.1,
                f.input,
                f.face,
                f.crease_edge,
                f.depth,
                f.at[0],
                f.at[1],
                f.at[2]
            );
        }
    }
    if let Some(path) = std::env::var_os("YANG_S43_CURVE_DOMAIN_LOG") {
        use std::io::Write as _;
        let case = std::env::var("ASSAY_CASE").unwrap_or_default();
        let rung = crate::stage1_tessellate::chord_refine_scale();
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            for f in &fires {
                let _ = writeln!(
                    file,
                    "{case} rung={rung} edge=({},{}) {:?}:{} crease={} depth={:.4e}",
                    f.edge.0, f.edge.1, f.input, f.face, f.crease_edge, f.depth
                );
            }
        }
    }
    crate::boolean::failures::record_curve_domain_fires(&fires);
}
