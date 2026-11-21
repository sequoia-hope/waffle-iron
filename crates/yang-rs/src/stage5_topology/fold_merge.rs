#[allow(clippy::wildcard_imports)]
use super::*;

#[allow(clippy::too_many_arguments)]
/// Yang §4.4.1 **Fig-11(b)→(c)** — merge each boundary vertex the Stage-4
/// relocation OVERRAN into the relocated vertex that overran it, and
/// re-triangulate every holder patch.
///
/// # The defect this closes
///
/// Fig 11's `q` is "an intersection point on the boundary curve"; the paper
/// splits the constrained edge containing `q` and merges the too-close split
/// endpoint `p` into it. Our pipeline reaches that configuration from the other
/// side: the arrangement already put a vertex where the two MESHES cross, and
/// Stage 4 relocates it onto the exact analytic junction. Because the meshes are
/// inscribed approximations, the exact junction generally sits on the far side
/// of the neighbouring rim GRID vertex, so the relocation carries `q` PAST `p`.
/// `p` is then interior to the other solid, and the kept patch's boundary walks
/// out to `q` and back over it — a folded loop that Stage 6 emits and the render
/// CDT rejects (`ring rejected by CDT`).
///
/// Anchor (2026-08-19 census over the nine ring-reject cases): EVERY non-simple
/// output loop the `YANG_S6_LOOP_SIMPLICITY` scan can measure is
/// `class=MINTED_BY_S4` with `cross_pre=0` — `cross_inherited` is 0 across the
/// whole family. On F0045 the apex's turn goes `27.69° → 167.34°` (27.69° is
/// exactly the rim's own 360/13 grid step) when its junction neighbour moves
/// `2.382e-2` across a `1.283e-2` pre-spacing.
///
/// # Why this repair primitive
///
/// The 2026-08-05 trial (`YANG_S4_FIG11_MERGE`, since removed) applied the same
/// idea with `collapse_vertex` and
/// measured NEGATIVE: a bare index rewrite of a real-length edge leaves the
/// surrounding fan inconsistent, and F0067's wall merely moved from the ring
/// reject to a non-2-manifold STOP. **The paper's merge happens inside the
/// §4.4.1 parametric re-triangulation.** So this pass merges by SUBSTITUTION IN
/// THE CYCLES and then re-CDTs every holder patch through the same
/// [`rebuild_patch_planar`] / [`apply_rebuild_batch`] machinery the always-on
/// construct pass uses — no vertex is ever re-pointed without its patch being
/// rebuilt.
///
/// # Discipline
///
/// - **All-holders-or-none.** Every patch holding the victim on a cycle OR in a
///   triangle joins the batch. A one-sided merge would be a T-junction, and a
///   surviving un-rebuilt triangle would be the 08-05 bare collapse.
/// - **Unchartable holder ⇒ loud refusal**, persistent for that victim.
/// - **Degeneration ⇒ loud refusal**: a holder cycle dropping below 3 vertices,
///   or a patch declining its rebuild, blocks the victims that pulled it in and
///   the pass retries without them.
pub(super) fn run_fold_merge_passes(
    mesh: &mut Mesh,
    attribution: &mut TriangleAttributionMap,
    side_labels: &mut crate::brep::TriangleSideLabelMap,
    tri_bounds: &mut crate::brep::TriangleErrorBoundMap,
    a: &BRep,
    b: &BRep,
    infos: &mut Vec<crate::stage4_correct::PatchInfo>,
    intersection_curves: &mut std::collections::BTreeMap<(u32, u32), Curve>,
    relocations: &mut Vec<(u32, f64)>,
    full_arrangement: bool,
    mut tangential_verts: Option<&mut std::collections::BTreeSet<u32>>,
    mut vertex_params: Option<&mut crate::param_surface::carried::VertexParamMap>,
) -> Result<usize, YangError> {
    use crate::stage4_construct::{apply_rebuild_batch, rebuild_merge_fan};
    use crate::stage4_fold_risk::fold_merge_sites_censused;
    use crate::stage4_splice::SplicePatch;
    use std::collections::BTreeSet;

    // Each applied pass strictly removes at least one boundary vertex, so the
    // pass count is bounded by the mesh's own vertex count — that bound IS
    // the runaway guard. The historical fixed cap of 32 was tuned to the
    // still-apex family (1–2 sites per case) and BINDS on the I13c on-curve
    // population (R0003 measured ~190 legitimate terminal-overrun sites in
    // one boolean — one per strip×wall junction of a fine revolve profile);
    // an exhausted cap strands the family half-repaired.
    let max_passes: usize = mesh.verts.len().max(32);

    // Victims refused by a holder — persistent across passes so a refusal can
    // never livelock into re-proposing the same merge.
    let mut blocked: BTreeSet<u32> = BTreeSet::new();
    let mut applied_total = 0usize;

    for pass in 0..max_passes {
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
                "[s4-fold-merge] STOP pass={pass}: patch/info correspondence broken \
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

        let cyc_refs: Vec<Vec<u32>> = patches.iter().flat_map(|p| p.cycles.clone()).collect();
        let post = mesh_positions(mesh);
        let (all_sites, census) = S4_PRE_POS.with(|c| {
            let borrow = c.borrow();
            match borrow.as_ref() {
                Some(pre) => fold_merge_sites_censused(
                    cyc_refs.iter().map(Vec::as_slice),
                    pre,
                    &post,
                    intersection_curves,
                ),
                None => (Vec::new(), Default::default()),
            }
        });
        let mut sites: Vec<crate::stage4_fold_risk::FoldMergeSite> = all_sites
            .into_iter()
            .filter(|s| !blocked.contains(&s.victim))
            .collect();
        if std::env::var_os("YANG_441_MERGE_SITE_PROBE").is_some() {
            // Before the guard, so the sites it REJECTS are the ones the probe
            // can still be pointed at — they are the interesting ones.
            for site in &sites {
                probe_merge_site(mesh, attribution, a, b, &patches, site);
            }
        }
        // §4-I8 — CARRIER CONTAINMENT. A merge IDENTIFIES two positions, so
        // it is authority-preserving exactly when the victim lies on nothing
        // the survivor is off: `carried(victim) ⊆ carried(survivor)`. Then
        // the victim is a plain sample and the survivor is the richer point
        // on the same carriers (Fig-11's p and q — measured on F0045:
        // victim {B:0, B:2} ⊂ survivor {A:2, B:0, B:2}, and on R0090
        // likewise). If instead the victim carries a surface the survivor
        // is OFF, the two are DISTINCT model points and no merge can join
        // them: merging would evict a face-loop vertex off a surface it
        // lies on — the eviction KV15b I1b already forbids for the
        // sub-resolution collapse, here applied to the Fig-11 merge.
        //
        // Measured 2026-08-20 over the ring-reject family: R0011
        // {B:1, B:180, B:181} vs {A:2, B:1, B:181} (victim 3.43 off A:2,
        // survivor 0.42 off B:180), R0074 and R0085 the same shape —
        // equal-size sets that differ, i.e. a model CORNER and a curve
        // junction 5–7 local units apart. A count-only richness test calls
        // those a TIE; containment names them for what they are.
        sites.retain(|site| {
            let off = carrier_lost_by_merge(mesh, attribution, a, b, site.victim, site.survivor);
            match off {
                None => true,
                Some(surf) => {
                    c441_log!(
                        "[s4-fold-merge] pass={pass}: NOT-A-MERGE v{} -> v{} — the victim \
                         carries {surf:?}, which the survivor is off: two DISTINCT model \
                         points, not Fig-11's p and q",
                        site.victim,
                        site.survivor,
                    );
                    blocked.insert(site.victim);
                    false
                }
            }
        });
        c441_log!(
            "[s4-fold-merge] pass={pass}: SELECT corners={} inversions={} \
             apex_moved={} (on_curve={} oncurve_sites={}) apex_minted={} survivor_still={} \
             ambiguous={} -> sites={}",
            census.corners,
            census.inversions,
            census.apex_moved,
            census.apex_moved_on_curve,
            census.oncurve_sites,
            census.apex_minted,
            census.survivor_still,
            census.ambiguous,
            sites.len(),
        );
        // Holder closure. Every patch holding the victim in a TRIANGLE rebuilds
        // its fan — all-holders-or-none, so no triangle is ever re-pointed
        // without being re-triangulated (that is the 2026-08-05 bare-collapse
        // trap). A patch carrying the victim only on a cycle, with no triangle,
        // would be malformed input; it is refused loudly by the fan builder.
        //
        // Only ONE site is applied per pass. The fans of two sites can share a
        // triangle, and `apply_rebuild_batch` refuses an overlapping batch
        // outright; sequencing them one per pass keeps every repair a plain,
        // separately-attributable rebuild rather than a merged plan.
        let mut rebuilds: Vec<crate::stage4_construct::PatchRebuild> = Vec::new();
        let mut merged: Option<(u32, u32)> = None;
        let chartable = |s: &Surface| crate::stage4_project::SurfaceChart::supports(s);
        for site in &sites {
            let holders: Vec<usize> = patches
                .iter()
                .enumerate()
                .filter(|&(_pj, pat)| {
                    pat.tris
                        .iter()
                        .any(|&t| mesh.tris[t as usize].contains(&site.victim))
                })
                .map(|(pj, _)| pj)
                .collect();
            if holders.is_empty() || holders.iter().any(|&h| !chartable(&patches[h].surface)) {
                c441_log!(
                    "[s4-fold-merge] pass={pass}: REFUSED v{} -> v{} — unchartable (or no) \
                     holder in {holders:?}; blocked",
                    site.victim,
                    site.survivor
                );
                blocked.insert(site.victim);
                continue;
            }
            let mut plan = Vec::with_capacity(holders.len());
            let mut declined = None;
            for &h in &holders {
                match rebuild_merge_fan(mesh, h, &patches[h], site.victim, site.survivor, true) {
                    Ok(r) => plan.push(r),
                    Err(e) => {
                        c441_log!(
                            "[s4-fold-merge] pass={pass}: DECLINED patch {h} for v{} — {e:?}",
                            site.victim
                        );
                        declined = Some(h);
                        break;
                    }
                }
            }
            if let Some(h) = declined {
                c441_log!(
                    "[s4-fold-merge] pass={pass}: BLOCKED v{} -> v{} — holder {h} declined \
                     its fan rebuild",
                    site.victim,
                    site.survivor
                );
                blocked.insert(site.victim);
                continue;
            }
            // The victim must not survive anywhere outside the rebuilt fans.
            // If it did, `apply_rebuild_batch`'s `subs` would re-point that
            // triangle WITHOUT re-triangulating it — the 2026-08-05 bare
            // collapse. So the plan is verified to cover every occurrence and
            // then applied with an EMPTY substitution map: the merge is carried
            // entirely by the re-triangulated fans, never by a relabel.
            let planned: BTreeSet<u32> = plan
                .iter()
                .flat_map(|r| r.old_tris.iter().copied())
                .collect();
            if let Some(t) = (0..mesh.tris.len() as u32)
                .find(|t| !planned.contains(t) && mesh.tris[*t as usize].contains(&site.victim))
            {
                c441_log!(
                    "[s4-fold-merge] pass={pass}: BLOCKED v{} -> v{} — triangle {t} holds it \
                     outside every holder fan (would be re-pointed, not rebuilt)",
                    site.victim,
                    site.survivor
                );
                blocked.insert(site.victim);
                continue;
            }
            c441_log!(
                "[s4-fold-merge] pass={pass}: MERGE v{} -> v{} chord_t={:.4} holders={holders:?}",
                site.victim,
                site.survivor,
                site.chord_t
            );
            if c441_verbose() {
                let pv = mesh.verts[site.victim as usize].as_array();
                let ps = mesh.verts[site.survivor as usize].as_array();
                eprintln!(
                    "[s4-fold-merge]   victim v{} = ({:.12},{:.12},{:.12}) survivor v{} = \
                     ({:.12},{:.12},{:.12}) holders {:?}",
                    site.victim,
                    pv[0],
                    pv[1],
                    pv[2],
                    site.survivor,
                    ps[0],
                    ps[1],
                    ps[2],
                    holders
                        .iter()
                        .map(|&h| (h, infos[h].input, infos[h].face_idx, patches[h].tris.len()))
                        .collect::<Vec<_>>()
                );
            }
            rebuilds = plan;
            merged = Some((site.victim, site.survivor));
            break;
        }
        let Some((victim, survivor)) = merged else {
            // Every site this pass was refused (or none existed); those
            // refusals are persistent, so the pass is at its fixed point.
            break;
        };
        // `YANG_CHI_AUDIT`: bracket every apply with the id-complex
        // invariants.
        let chi_pre = chi_audit_enabled().then(|| tris_complex_stats(&mesh.tris));
        match apply_rebuild_batch(
            mesh,
            attribution,
            side_labels,
            tri_bounds,
            &rebuilds,
            vertex_params.as_deref_mut(),
        ) {
            Ok(()) => {
                // f2c-3: report ONLY the applies that CHANGE an invariant —
                // pass, kind, and the before/after (χ, components,
                // per-component χ). Silence means every apply was neutral.
                if let Some(pre_stats) = &chi_pre {
                    let post_stats = tris_complex_stats(&mesh.tris);
                    if pre_stats.3 != post_stats.3
                        || pre_stats.4 != post_stats.4
                        || pre_stats.5 != post_stats.5
                    {
                        eprintln!(
                            "[chi-audit] pass={pass} kind=fold-merge: chi {} -> {} \
                             components {} -> {} per-component {:?} -> {:?}",
                            pre_stats.3,
                            post_stats.3,
                            pre_stats.4,
                            post_stats.4,
                            pre_stats.5,
                            post_stats.5
                        );
                    }
                }
                c441_log!(
                    "[s4-fold-merge] pass={pass}: APPLIED v{victim} -> v{survivor} over {} fans",
                    rebuilds.len()
                );
                applied_total += 1;
            }
            Err(e) => {
                eprintln!("[s4-fold-merge] STOP pass={pass}: WRITE-BACK REFUSED {e:?}");
                // Print-only: the refused batch's units (patch, old tris,
                // new tri count) so an `OverlappingBatch` names WHICH two
                // units claim the shared triangle (measured R0003 at the
                // thin-band density: the f2c chord split and seam insert
                // of one fragment).
                for r in &rebuilds {
                    eprintln!(
                        "[s4-fold-merge]   unit patch={} old_tris={:?} new_tris={}",
                        r.patch,
                        r.old_tris,
                        r.new_tris.len()
                    );
                    for &t in &r.old_tris {
                        let tri = mesh.tris[t as usize];
                        eprintln!(
                            "[s4-fold-merge]     old t{t} {:?} {:?}",
                            tri,
                            tri.map(|v| mesh.verts[v as usize].as_array())
                        );
                    }
                    for nt in &r.new_tris {
                        eprintln!("[s4-fold-merge]     new {nt:?}");
                    }
                }
                break;
            }
        }
        let remap = compact_unreferenced_verts_with_bounds(
            mesh,
            relocations,
            &side_labels.labels,
            &tri_bounds.bounds,
            vertex_params.as_deref_mut(),
        );
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
        probe_remap_pre_pos("fold-merge", remap.as_ref());
        crate::stage4_correct::coincident_vertex_census("s5-fold-merge", mesh);
        probe_record_incidence(&inc2, &cv2);
        *infos = i2;
        *intersection_curves = cv2;
        // Vertex indices moved: a blocked victim's identity is stale, and the
        // next pass re-derives its site from the compacted mesh anyway.
        if let Some(map) = remap.as_ref() {
            blocked = blocked
                .iter()
                .filter_map(|&v| map.get(v as usize).copied().flatten())
                .collect();
        }
    }
    if applied_total > 0 {
        c441_log!("[s4-fold-merge] TOTAL {applied_total} Fig-11 merges applied");
    }
    Ok(applied_total)
}

/// `YANG_441_MERGE_SITE_PROBE` — the per-site configuration behind a Fig-11
/// merge decision, printed before the repair runs.
///
/// Three views, each of which answered a question the decline codes could not
/// (§4-I8): per HOLDER, its attribution / fan size / whether the survivor is in
/// that fan / each endpoint's distance to that holder's own surface; per SITE,
/// the surfaces each endpoint carries and how far the other endpoint is from
/// each; and the survivor's TRAVEL segment with the victim's parameter on it —
/// the certificate that separates "p too close to q" from a relocation that slid
/// along a carrier past that carrier's own endpoint.
pub(super) fn probe_merge_site(
    mesh: &Mesh,
    attribution: &TriangleAttributionMap,
    a: &BRep,
    b: &BRep,
    patches: &[crate::stage4_splice::SplicePatch],
    site: &crate::stage4_fold_risk::FoldMergeSite,
) {
    let holders: Vec<usize> = patches
        .iter()
        .enumerate()
        .filter(|&(_pj, pat)| {
            pat.tris
                .iter()
                .any(|&t| mesh.tris[t as usize].contains(&site.victim))
        })
        .map(|(pj, _)| pj)
        .collect();
    // Where the survivor CAME FROM, and whether the victim lies on
    // the path it travelled: a relocation that slid along a model
    // edge PAST that edge's own endpoint is a different defect from
    // Fig-11's "p is too close to q".
    S4_PRE_POS.with(|c| {
        if let Some(pre) = c.borrow().as_ref() {
            let post = |v: u32| mesh.verts[v as usize].as_array();
            let (sv, vi) = (site.survivor, site.victim);
            let rich = |v: u32| {
                crate::stage4_correct::surface_incidence_count(
                    mesh,
                    &attribution.attributions,
                    a,
                    b,
                    v,
                    mesh.verts[v as usize].as_array(),
                )
            };
            eprintln!(
                "[s4-merge-rich] victim=v{vi} rich={} survivor=v{sv} rich={}",
                rich(vi),
                rich(sv),
            );
            // Every surface each endpoint carries, and how far the
            // OTHER endpoint is from it. If the two carry the same
            // surfaces they are two samples of one thing; if one
            // carries a surface the other is far from, they are
            // distinct model points and no merge can identify them.
            for v in [vi, sv] {
                let mut seen: Vec<crate::geom::Surface> = Vec::new();
                for (t, tri) in mesh.tris.iter().enumerate() {
                    if !tri.contains(&v) {
                        continue;
                    }
                    let Some(att) = attribution.attributions[t] else {
                        continue;
                    };
                    let faces = match att.input {
                        crate::brep::InputId::A => a.faces(),
                        crate::brep::InputId::B => b.faces(),
                    };
                    let Some(face) = faces.get(att.face as usize) else {
                        continue;
                    };
                    if seen.contains(&face.surface) {
                        continue;
                    }
                    seen.push(face.surface);
                    let dv = |x: u32| {
                        crate::stage4_relocate::surface_distance_and_normal(
                            face.surface,
                            mesh.verts[x as usize].as_array(),
                        )
                        .map(|(f, _)| f)
                    };
                    eprintln!(
                        "[s4-merge-carrier] of v{v}: {:?}:{} d_victim={:?} \
                                 d_survivor={:?}",
                        att.input,
                        att.face,
                        dv(vi),
                        dv(sv),
                    );
                }
            }
            if let Some(&p0) = pre.get(&sv) {
                let p1 = post(sv);
                let q = post(vi);
                let d = |x: [f64; 3], y: [f64; 3]| {
                    ((x[0] - y[0]).powi(2) + (x[1] - y[1]).powi(2) + (x[2] - y[2]).powi(2)).sqrt()
                };
                // Parameter of the victim projected on the
                // survivor's travel segment, and how far off it.
                let seg = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
                let len2 = seg[0] * seg[0] + seg[1] * seg[1] + seg[2] * seg[2];
                let t = if len2 > 0.0 {
                    ((q[0] - p0[0]) * seg[0] + (q[1] - p0[1]) * seg[1] + (q[2] - p0[2]) * seg[2])
                        / len2
                } else {
                    f64::NAN
                };
                let foot = [p0[0] + t * seg[0], p0[1] + t * seg[1], p0[2] + t * seg[2]];
                eprintln!(
                    "[s4-merge-travel] v{vi} <- v{sv}: pre=({:.6},{:.6},{:.6}) \
                             post=({:.6},{:.6},{:.6}) travel={:.4e} victim_t={t:.4} \
                             victim_off_travel={:.4e} victim_pre_moved={:?}",
                    p0[0],
                    p0[1],
                    p0[2],
                    p1[0],
                    p1[1],
                    p1[2],
                    d(p0, p1),
                    d(q, foot),
                    pre.get(&vi).map(|&x| d(x, q)),
                );
            }
        }
    });
    // Per-holder configuration of ONE proposed site: what the
    // repair is actually being handed. A decline names its
    // condition; this names the geometry behind the condition.
    for &h in &holders {
        let fan: Vec<[u32; 3]> = patches[h]
            .tris
            .iter()
            .map(|&t| mesh.tris[t as usize])
            .filter(|tri| tri.contains(&site.victim))
            .collect();
        let touches = fan.iter().any(|tri| tri.contains(&site.survivor));
        // Does the SURVIVOR lie on this holder's own surface? A
        // merge re-anchors every holder's triangles onto it, so a
        // survivor off the holder's surface would evict that
        // patch's face off its analytic carrier — the question the
        // `FanSurvivorNotAdjacent` refusal is really asking.
        let dist = |v: u32| {
            crate::stage4_relocate::surface_distance_and_normal(
                patches[h].surface,
                mesh.verts[v as usize].as_array(),
            )
            .map(|(f, _)| f)
        };
        let att = patches[h]
            .tris
            .first()
            .and_then(|&t| attribution.attributions[t as usize])
            .map(|a| format!("{:?}:{}", a.input, a.face))
            .unwrap_or_else(|| "none".into());
        eprintln!(
            "[s4-merge-site] v{} -> v{} holder={h} att={att} tris={} fan={} \
                     survivor_in_fan={touches} d_victim={:?} d_survivor={:?} gap={:.4e} \
                     fan_tris={fan:?} surface={:?}",
            site.victim,
            site.survivor,
            patches[h].tris.len(),
            fan.len(),
            dist(site.victim),
            dist(site.survivor),
            {
                let (x, y) = (
                    mesh.verts[site.victim as usize].as_array(),
                    mesh.verts[site.survivor as usize].as_array(),
                );
                ((x[0] - y[0]).powi(2) + (x[1] - y[1]).powi(2) + (x[2] - y[2]).powi(2)).sqrt()
            },
            patches[h].surface,
        );
    }
}

/// §4-I8: the first surface the VICTIM lies on that the SURVIVOR does not —
/// `None` when `carried(victim) ⊆ carried(survivor)` and the merge therefore
/// discards no analytic authority.
///
/// Incidence is certified at `junction_certificate_band` on each side, the same
/// band that certifies a Stage-4 exact junction, so the test is tolerance-free
/// in the sense that matters: it asks "does the survivor lie on this surface at
/// junction precision?", never "is it close enough?". A surface whose distance
/// function declines to answer counts as LOST — the refusing direction, never
/// the merging one.
pub(super) fn carrier_lost_by_merge(
    mesh: &Mesh,
    attribution: &TriangleAttributionMap,
    a: &BRep,
    b: &BRep,
    victim: u32,
    survivor: u32,
) -> Option<Surface> {
    let vp = mesh.verts[victim as usize].as_array();
    let sp = mesh.verts[survivor as usize].as_array();
    crate::stage4_correct::carried_surfaces(mesh, &attribution.attributions, a, b, victim, vp)
        .into_iter()
        .find(|&surf| {
            !crate::stage4_relocate::surface_distance_and_normal(surf, sp).is_some_and(|(f, _)| {
                f.abs() <= crate::stage4_relocate::junction_certificate_band(sp, surf)
            })
        })
}

/// Every mesh vertex as a bare `[f64; 3]` — the position view the
/// `stage4_fold_risk` selectors index by vertex id.
pub(super) fn mesh_positions(mesh: &Mesh) -> Vec<[f64; 3]> {
    mesh.verts.iter().map(Point3::as_array).collect()
}
