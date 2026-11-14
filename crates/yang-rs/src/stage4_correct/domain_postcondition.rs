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
