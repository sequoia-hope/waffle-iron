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
    a: &BRep,
    b: &BRep,
    infos: &mut Vec<crate::stage4_correct::PatchInfo>,
    intersection_curves: &mut std::collections::BTreeMap<(u32, u32), Curve>,
    relocations: &mut Vec<(u32, f64)>,
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
    // I13e: victims of a GROUP absorption a holder refused — separate from
    // `blocked` because group membership is exactly the population `blocked`
    // already holds (each member's per-site repair was refused first); this
    // set is what keeps a refused GROUP from being re-proposed.
    let mut group_blocked: BTreeSet<u32> = BTreeSet::new();
    // §I13(f) f2: PAIRS a re-homing certificate refused — the arm's own
    // livelock guard (decline paths must guarantee progress), keyed by the
    // unordered id pair: a site's two W↔K-mirrored views SHARE the phantom
    // vertex, so blocking a refused view's VERTICES would silently skip
    // its sibling (the true view).
    let mut rehome_blocked: BTreeSet<(u32, u32)> = BTreeSet::new();
    let mut applied_total = 0usize;

    for pass in 0..max_passes {
        let adjacency = triangle_adjacency(mesh);
        let raw = crate::stage4_correct::merge_same_plane_patches(
            flood_fill_patches(mesh, attribution, &adjacency),
            &adjacency,
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
        // An empty corner-site set falls through: the I13d run-level arm
        // below is consulted before the pass loop concludes.

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
        let mut progressed = false;
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
            progressed = true;
            break;
        }
        let mut run_merged: Option<Vec<(u32, Vec<u32>)>> = None;
        let mut rehome_reloc: Option<(u32, Point3)> = None;
        if !progressed {
            // Every corner-level site this pass was refused (or none existed);
            // those refusals are persistent, so the corner arm is at its fixed
            // point. Consult the I13d run-level arm before concluding.
            if std::env::var_os("YANG_441_RUN_PROBE").is_some() {
                // I13d anchor probe: the all-blocked fixed point IS the
                // residual the run-level absorption must own — dump each
                // refused site's cycle neighbourhood (per-edge curve typing,
                // per-vertex carriers and moved flags, curve parameters).
                for site in &sites {
                    probe_run_neighborhood(
                        mesh,
                        attribution,
                        a,
                        b,
                        &patches,
                        intersection_curves,
                        site.victim,
                        site.survivor,
                    );
                }
            }
            match run_absorption_attempt(
                mesh,
                attribution,
                a,
                b,
                &patches,
                &cyc_refs,
                intersection_curves,
                &mut blocked,
                &mut group_blocked,
                &mut rehome_blocked,
                pass,
            ) {
                Some((plan, absorbed, reloc)) => {
                    rebuilds = plan;
                    run_merged = Some(absorbed);
                    rehome_reloc = reloc;
                }
                None => break,
            }
        }
        // f2c apply-time MESH edge-use audit (rehome applies only): the
        // undirected-edge use counts before vs after the batch — any edge
        // torn open (2 → ≠2 uses) or healed is printed with positions.
        // This is the mesh-level truth the output emission's run-collapsed
        // loops obscure (measured 2026-08-28: the ON-run Stage-6 pairing
        // STOP decodes to exactly these tears).
        let edge_uses = |mesh: &Mesh| -> std::collections::BTreeMap<(u32, u32), u32> {
            let mut m: std::collections::BTreeMap<(u32, u32), u32> = Default::default();
            for tri in &mesh.tris {
                for k in 0..3 {
                    let (x, y) = (tri[k], tri[(k + 1) % 3]);
                    *m.entry((x.min(y), x.max(y))).or_default() += 1;
                }
            }
            m
        };
        let pre_uses = rehome_reloc.map(|_| edge_uses(mesh));
        if let Some(_pre) = &pre_uses {
            let (v, e, f, chi, comps, chis) = tris_complex_stats(&mesh.tris);
            eprintln!(
                "[i13f-rehome]   audit: PRE  V({v}) - E({e}) + F({f}) = {chi} \
                 components={comps} per-component-chi={chis:?}"
            );
        }
        // f2c-3 χ-localization (`YANG_CHI_AUDIT`): bracket EVERY fold-merge/
        // absorb/rehome apply with the id-complex invariants — the genus-2
        // measured at the rehome applies (2026-08-28) predates them, and the
        // ~135 passes of earlier applies are suspects exactly like the
        // upstream stages.
        let chi_pre =
            (chi_audit_enabled() && pre_uses.is_none()).then(|| tris_complex_stats(&mesh.tris));
        match apply_rebuild_batch(mesh, attribution, side_labels, &rebuilds) {
            Ok(()) => {
                // §I13(f) f2: the re-homed corner's mint position — the fans
                // in `rebuilds` were planned against it, so it lands only
                // with them (a refused batch leaves the mesh untouched).
                if let Some((v, p)) = rehome_reloc {
                    mesh.verts[v as usize] = p;
                }
                if let Some(pre) = &pre_uses {
                    let post = edge_uses(mesh);
                    let mut torn = 0usize;
                    for (&e, &n) in &post {
                        let was = pre.get(&e).copied().unwrap_or(0);
                        if n == was || (n == 2) == (was == 2) {
                            continue;
                        }
                        torn += 1;
                        if torn <= 24 {
                            let p0 = mesh.verts[e.0 as usize].as_array();
                            let p1 = mesh.verts[e.1 as usize].as_array();
                            eprintln!(
                                "[i13f-rehome]   audit: edge (v{},v{}) uses {was}->{n} \
                                 a=({:.6},{:.6},{:.6}) b=({:.6},{:.6},{:.6})",
                                e.0, e.1, p0[0], p0[1], p0[2], p1[0], p1[1], p1[2]
                            );
                        }
                    }
                    for (&e, &was) in pre {
                        if was != 2 || post.contains_key(&e) {
                            continue;
                        }
                        torn += 1;
                        if torn <= 24 {
                            eprintln!(
                                "[i13f-rehome]   audit: edge (v{},v{}) uses {was}->0 (gone)",
                                e.0, e.1
                            );
                        }
                    }
                    eprintln!(
                        "[i13f-rehome]   audit: {torn} edge(s) changed 2-manifold status \
                         across the apply"
                    );
                    let (v, e, f, chi, comps, chis) = tris_complex_stats(&mesh.tris);
                    eprintln!(
                        "[i13f-rehome]   audit: POST V({v}) - E({e}) + F({f}) = {chi} \
                         components={comps} per-component-chi={chis:?}"
                    );
                }
                // f2c-3: report ONLY the applies that CHANGE an invariant —
                // pass, kind, and the before/after (χ, components,
                // per-component χ). Silence means every apply was neutral.
                if let Some(pre_stats) = &chi_pre {
                    let post_stats = tris_complex_stats(&mesh.tris);
                    if pre_stats.3 != post_stats.3
                        || pre_stats.4 != post_stats.4
                        || pre_stats.5 != post_stats.5
                    {
                        let kind = if run_merged.is_some() {
                            "run/group"
                        } else {
                            "fold-merge"
                        };
                        eprintln!(
                            "[chi-audit] pass={pass} kind={kind}: chi {} -> {} \
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
                if let Some(absorbed) = &run_merged {
                    if let (Some((rv, rp)), [(_, victims)]) = (&rehome_reloc, &absorbed[..]) {
                        c441_log!(
                            "[i13f-rehome] pass={pass}: APPLIED re-homing {victims:?} -> \
                             v{rv}@({:.6},{:.6},{:.6}) over {} fans",
                            rp.as_array()[0],
                            rp.as_array()[1],
                            rp.as_array()[2],
                            rebuilds.len()
                        );
                    } else if let [(survivor, victims)] = &absorbed[..] {
                        c441_log!(
                            "[i13d-absorb] pass={pass}: APPLIED run {victims:?} -> v{survivor} \
                             over {} fans",
                            rebuilds.len()
                        );
                    } else {
                        c441_log!(
                            "[i13e-group] pass={pass}: APPLIED group {:?} over {} fans",
                            absorbed
                                .iter()
                                .map(|(s, vs)| format!("{vs:?}->v{s}"))
                                .collect::<Vec<_>>(),
                            rebuilds.len()
                        );
                    }
                } else {
                    let (victim, survivor) = merged.expect("progressed implies a merge");
                    c441_log!(
                        "[s4-fold-merge] pass={pass}: APPLIED v{victim} -> v{survivor} over {} fans",
                        rebuilds.len()
                    );
                }
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
                if let Some((rv, rp)) = &rehome_reloc {
                    eprintln!("[s4-fold-merge]   reloc v{rv} -> {:?}", rp.as_array());
                }
                break;
            }
        }
        let remap = compact_unreferenced_verts(mesh, relocations, &side_labels.labels);
        let (i2, inc2, cv2) = compute_phase_a(
            mesh,
            attribution,
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
            group_blocked = group_blocked
                .iter()
                .filter_map(|&v| map.get(v as usize).copied().flatten())
                .collect();
            rehome_blocked = rehome_blocked
                .iter()
                .filter_map(|&(a2, b2)| {
                    let (a2, b2) = (
                        map.get(a2 as usize).copied().flatten()?,
                        map.get(b2 as usize).copied().flatten()?,
                    );
                    Some((a2.min(b2), a2.max(b2)))
                })
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

/// A committed absorption: the per-holder rebuild plan plus each absorbed
/// site's `(survivor, victims)` — one entry from the I13d run arm, several
/// from the I13e group arm — plus, for the §I13(f) re-homing arm only, the
/// survivor's mint position (applied by the caller together with the batch;
/// `None` for every absorption).
pub(super) type AbsorptionPlan = (
    Vec<crate::stage4_construct::PatchRebuild>,
    Vec<(u32, Vec<u32>)>,
    Option<(u32, Point3)>,
);

/// I13d (**ALWAYS-ON since the 2026-08-25 flip** — see
/// `stage4_fold_risk::run_absorb_mode`; `YANG_441_RUN_ABSORB=0|off` is the
/// dev off-knob): at the corner arm's fixed point — no corner-level site
/// found, or every one refused — select the run-level junction-absorption
/// sites, report them (the census ledger prints in both census and on
/// modes), and in ON mode plan the first applicable site's rebuild batch.
/// Census mode never applies. Off = no work at all (the pre-flip
/// byte-identical pipeline).
///
/// The strictly-richer oracle is carrier containment measured on the mesh:
/// `carried(victim) ⊆ carried(junction)` with the junction carrying at least
/// one surface the victim does not — the I8 containment plus junction-hood,
/// certified at `junction_certificate_band` like every carrier question.
/// Application re-checks each victim through [`carrier_lost_by_merge`] (the
/// same I8 gate the corner arm uses), closes over every holder patch, and
/// verifies no triangle outside the planned fans references a victim — the
/// bare-collapse guard, unchanged.
///
/// When every per-site proposal is refused, the I13e group arm
/// (**ALWAYS-ON since the 2026-08-26 flip**; `YANG_441_GROUP_ABSORB=0|off`
/// is the dev off-knob) takes the residue: mutually interlocked
/// sites — each one's fan polygon containing a partner's still-folded
/// victim, so the per-site CDTs refuse in every order — are absorbed as one
/// interference GROUP per pass via [`rebuild_group_fan`], under the same
/// I8/holder-closure/bare-collapse gates (spec §I13(e)).
#[allow(clippy::too_many_arguments)]
pub(super) fn run_absorption_attempt(
    mesh: &Mesh,
    attribution: &TriangleAttributionMap,
    a: &BRep,
    b: &BRep,
    patches: &[crate::stage4_splice::SplicePatch],
    cyc_refs: &[Vec<u32>],
    curves: &std::collections::BTreeMap<(u32, u32), Curve>,
    blocked: &mut std::collections::BTreeSet<u32>,
    group_blocked: &mut std::collections::BTreeSet<u32>,
    rehome_blocked: &mut std::collections::BTreeSet<(u32, u32)>,
    pass: usize,
) -> Option<AbsorptionPlan> {
    use crate::stage4_construct::{rebuild_group_fan, rebuild_run_fan};
    use crate::stage4_fold_risk::{
        group_absorb_mode, interlock_groups, run_absorb_mode, GroupAbsorbMode, RunAbsorbMode,
    };
    use crate::stage4_fold_risk::{run_absorption_sites, RunAbsorptionSite};
    let mode = run_absorb_mode();
    // Position-keyed anchor probe: `YANG_441_RUN_PROBE_AT=x,y,z` dumps the
    // cycle neighbourhood of the mesh vertex nearest that position at this
    // fixed point — for walking BACK from a downstream artifact (a rejected
    // tessellation ring's 3D node) to the Stage-4/5 cycle that produced it.
    if let Some(spec) = std::env::var_os("YANG_441_RUN_PROBE_AT") {
        let parts: Vec<f64> = spec
            .to_string_lossy()
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();
        if let [x, y, z] = parts[..] {
            let d2 = |p: &Point3| {
                let q = p.as_array();
                (q[0] - x).powi(2) + (q[1] - y).powi(2) + (q[2] - z).powi(2)
            };
            let on_cycle: std::collections::BTreeSet<u32> =
                cyc_refs.iter().flatten().copied().collect();
            if let Some(&v) = on_cycle.iter().min_by(|&&p, &&q| {
                d2(&mesh.verts[p as usize]).total_cmp(&d2(&mesh.verts[q as usize]))
            }) {
                eprintln!(
                    "[i13d-run] PROBE_AT ({x},{y},{z}) nearest cycle vertex v{v} \
                     dist={:.6e}",
                    d2(&mesh.verts[v as usize]).sqrt()
                );
                probe_run_neighborhood(mesh, attribution, a, b, patches, curves, v, v);
            }
        }
    }
    let richer = |j: u32, v: u32| -> bool {
        let cs = |x: u32| {
            crate::stage4_correct::carried_surfaces(
                mesh,
                &attribution.attributions,
                a,
                b,
                x,
                mesh.verts[x as usize].as_array(),
            )
        };
        let (cj, cv) = (cs(j), cs(v));
        let ok = cv.iter().all(|s| cj.contains(s)) && cj.iter().any(|s| !cv.contains(s));
        // I13f census companion print (spec §I13(f)): the carrier sets of a
        // refused pair — a TRUE-CORNER pair (both ≥3 carriers, sets
        // incomparable) is the inverted-junction family; everything else in
        // the not_richer bucket is an ordinary refusal.
        if !ok && std::env::var("YANG_441_I13F").is_ok_and(|x| x == "census") {
            let kinds = |c: &[crate::Surface]| -> String {
                c.iter()
                    .map(|s| match s {
                        crate::Surface::Plane { .. } => "P",
                        crate::Surface::Cylinder { .. } => "Y",
                        crate::Surface::Cone { .. } => "C",
                        crate::Surface::Sphere { .. } => "S",
                        crate::Surface::Torus { .. } => "T",
                    })
                    .collect::<Vec<_>>()
                    .join("")
            };
            let shared = cv.iter().filter(|s| cj.contains(s)).count();
            eprintln!(
                "[i13f-census] CARRIERS j=v{j} n={} kinds={} vs v{v} n={} kinds={} shared={}",
                cj.len(),
                kinds(&cj),
                cv.len(),
                kinds(&cv),
                shared
            );
        }
        // f1 planner, report-only (`YANG_441_REHOME=census`): run the
        // §I13(f) corner-rehoming planner on every refused pair and print
        // the plan or its typed decline — the corpus-honest measurement of
        // the planner's certificates before anything applies.
        if !ok && std::env::var("YANG_441_REHOME").is_ok_and(|x| x == "census") {
            let out = crate::stage4_rehome::plan_corner_rehoming(
                j,
                mesh.verts[j as usize].as_array(),
                &cj,
                v,
                mesh.verts[v as usize].as_array(),
                &cv,
            );
            match out {
                Ok(p) => eprintln!(
                    "[i13f-rehome] PLAN j_cut=v{} j_rim=v{} new_wall=({:.9e},{:.9e},{:.9e}) \
                     new_rim=({:.9e},{:.9e},{:.9e}) residual={:.3e} rim_side_of_cut={:.3e}",
                    p.j_cut,
                    p.j_rim,
                    p.new_wall[0],
                    p.new_wall[1],
                    p.new_wall[2],
                    p.new_rim[0],
                    p.new_rim[1],
                    p.new_rim[2],
                    p.residual,
                    p.rim_side_of_cut
                ),
                Err(d) => eprintln!("[i13f-rehome] DECLINE j=v{j} v=v{v} reason={d:?}"),
            }
        }
        ok
    };
    let (sites, census, rehome_cands) = S4_PRE_POS.with(|c| {
        let borrow = c.borrow();
        match borrow.as_ref() {
            Some(pre) => {
                let post = mesh_positions(mesh);
                run_absorption_sites(
                    cyc_refs.iter().map(Vec::as_slice),
                    pre,
                    &post,
                    curves,
                    richer,
                )
            }
            None => (Vec::new(), Default::default(), Vec::new()),
        }
    });
    eprintln!(
        "[i13d-absorb] SELECT runs={} terminals={} no_param={} no_flip={} \
         no_inversion={} not_richer={} ambiguous={} -> sites={}",
        census.runs,
        census.terminals,
        census.no_param,
        census.no_flip,
        census.no_inversion,
        census.not_richer,
        census.ambiguous,
        census.sites,
    );
    for s in &sites {
        eprintln!(
            "[i13d-absorb] SITE survivor=v{} victims={:?}",
            s.survivor,
            s.victims
                .iter()
                .map(|&v| format!("v{v}"))
                .collect::<Vec<_>>(),
        );
    }
    if mode == RunAbsorbMode::Census {
        return None;
    }
    let chartable = |s: &Surface| crate::stage4_project::SurfaceChart::supports(s);
    'site: for site in &sites {
        if site.victims.iter().any(|v| blocked.contains(v)) {
            continue;
        }
        // §4-I8 per merge: each victim must lie on nothing the junction is
        // off. The selector's richness oracle asked the same question; this
        // re-check keeps the apply path under the exact gate the corner arm
        // uses, in refusing-direction form.
        for &v in &site.victims {
            if let Some(surf) = carrier_lost_by_merge(mesh, attribution, a, b, v, site.survivor) {
                c441_log!(
                    "[i13d-absorb] pass={pass}: NOT-A-MERGE v{v} -> v{} — the victim \
                     carries {surf:?}, which the junction is off",
                    site.survivor,
                );
                blocked.extend(site.victims.iter().copied());
                continue 'site;
            }
        }
        let vic: std::collections::BTreeSet<u32> = site.victims.iter().copied().collect();
        let holders: Vec<usize> = patches
            .iter()
            .enumerate()
            .filter(|&(_pj, pat)| {
                pat.tris
                    .iter()
                    .any(|&t| mesh.tris[t as usize].iter().any(|v| vic.contains(v)))
            })
            .map(|(pj, _)| pj)
            .collect();
        if holders.is_empty() || holders.iter().any(|&h| !chartable(&patches[h].surface)) {
            c441_log!(
                "[i13d-absorb] pass={pass}: REFUSED run {:?} -> v{} — unchartable (or no) \
                 holder in {holders:?}; blocked",
                site.victims,
                site.survivor
            );
            blocked.extend(site.victims.iter().copied());
            continue;
        }
        let mut plan = Vec::with_capacity(holders.len());
        for &h in &holders {
            match rebuild_run_fan(mesh, h, &patches[h], &vic, site.survivor) {
                Ok(r) => plan.push(r),
                Err(e) => {
                    c441_log!(
                        "[i13d-absorb] pass={pass}: DECLINED patch {h} for run {:?} — {e:?}",
                        site.victims
                    );
                    blocked.extend(site.victims.iter().copied());
                    continue 'site;
                }
            }
        }
        // Bare-collapse guard: the victims must not survive anywhere outside
        // the rebuilt fans.
        let planned: std::collections::BTreeSet<u32> = plan
            .iter()
            .flat_map(|r| r.old_tris.iter().copied())
            .collect();
        if let Some(t) = (0..mesh.tris.len() as u32).find(|t| {
            !planned.contains(t) && mesh.tris[*t as usize].iter().any(|v| vic.contains(v))
        }) {
            c441_log!(
                "[i13d-absorb] pass={pass}: BLOCKED run {:?} -> v{} — triangle {t} holds a \
                 victim outside every holder fan",
                site.victims,
                site.survivor
            );
            blocked.extend(site.victims.iter().copied());
            continue;
        }
        c441_log!(
            "[i13d-absorb] pass={pass}: ABSORB run {:?} -> v{} holders={holders:?}",
            site.victims,
            site.survivor
        );
        return Some((plan, vec![(site.survivor, site.victims.clone())], None));
    }

    // ---- I13e: cross-site group absorption ------------------------------
    // Every per-site proposal above is at its fixed point (an applied one
    // returned; the rest are blocked). The residual family: mutually
    // INTERLOCKED sites — adjacent strips' deep overruns crossing each
    // other's territory, so each site's fan polygon contains a partner's
    // still-folded victim and the per-site CDTs refuse in EVERY repair
    // order (R0003 wall patch 475, measured 2026-08-25). The repair unit is
    // the interference group: one region rebuild per holder, one closure
    // edge per site.
    let gmode = group_absorb_mode();
    // Candidates: this pass's still-certified sites — their per-site repair
    // is what was refused, so `blocked` membership is expected, but a group
    // already refused stays refused (`group_blocked`, the livelock guard)
    // and I8 holds unchanged: a not-a-merge victim disqualifies its SITE
    // (dropping the site, not the whole component — the rest may still
    // form a repairable group).
    let mut cand: Vec<&RunAbsorptionSite> = Vec::new();
    let mut group_blocked_n = 0usize;
    let mut i8_dropped = 0usize;
    for site in &sites {
        if site.victims.iter().any(|v| group_blocked.contains(v)) {
            group_blocked_n += 1;
            continue;
        }
        let off = site
            .victims
            .iter()
            .find_map(|&v| carrier_lost_by_merge(mesh, attribution, a, b, v, site.survivor));
        if let Some(surf) = off {
            c441_log!(
                "[i13e-group] pass={pass}: NOT-A-MERGE run {:?} -> v{} — a victim carries \
                 {surf:?}, which the junction is off; site dropped from grouping",
                site.victims,
                site.survivor,
            );
            i8_dropped += 1;
            continue;
        }
        cand.push(site);
    }
    let victim_sets: Vec<std::collections::BTreeSet<u32>> = cand
        .iter()
        .map(|s| s.victims.iter().copied().collect())
        .collect();
    let groups = interlock_groups(&victim_sets, &mesh.tris);
    eprintln!(
        "[i13e-group] SELECT candidates={} group_blocked={group_blocked_n} \
         i8_dropped={i8_dropped} groups={} sizes={:?}",
        cand.len(),
        groups.len(),
        groups.iter().map(Vec::len).collect::<Vec<_>>(),
    );
    for g in &groups {
        eprintln!(
            "[i13e-group] GROUP {:?}",
            g.iter()
                .map(|&i| format!("{:?}->v{}", cand[i].victims, cand[i].survivor))
                .collect::<Vec<_>>(),
        );
    }
    if gmode == GroupAbsorbMode::Census {
        return None;
    }
    'group: for g in &groups {
        let gsites: Vec<(std::collections::BTreeSet<u32>, u32)> = g
            .iter()
            .map(|&i| (victim_sets[i].clone(), cand[i].survivor))
            .collect();
        let vic_all: std::collections::BTreeSet<u32> = gsites
            .iter()
            .flat_map(|(vs, _)| vs.iter().copied())
            .collect();
        let holders: Vec<usize> = patches
            .iter()
            .enumerate()
            .filter(|&(_pj, pat)| {
                pat.tris
                    .iter()
                    .any(|&t| mesh.tris[t as usize].iter().any(|v| vic_all.contains(v)))
            })
            .map(|(pj, _)| pj)
            .collect();
        if holders.is_empty() || holders.iter().any(|&h| !chartable(&patches[h].surface)) {
            c441_log!(
                "[i13e-group] pass={pass}: REFUSED group {vic_all:?} — unchartable (or no) \
                 holder in {holders:?}; blocked",
            );
            group_blocked.extend(vic_all.iter().copied());
            continue;
        }
        let mut plan = Vec::with_capacity(holders.len());
        for &h in &holders {
            match rebuild_group_fan(mesh, h, &patches[h], &gsites) {
                Ok(r) => plan.push(r),
                Err(e) => {
                    c441_log!(
                        "[i13e-group] pass={pass}: DECLINED patch {h} for group {vic_all:?} \
                         — {e:?}",
                    );
                    group_blocked.extend(vic_all.iter().copied());
                    continue 'group;
                }
            }
        }
        // Bare-collapse guard: no group victim may survive outside the
        // rebuilt fans.
        let planned: std::collections::BTreeSet<u32> = plan
            .iter()
            .flat_map(|r| r.old_tris.iter().copied())
            .collect();
        if let Some(t) = (0..mesh.tris.len() as u32).find(|t| {
            !planned.contains(t) && mesh.tris[*t as usize].iter().any(|v| vic_all.contains(v))
        }) {
            c441_log!(
                "[i13e-group] pass={pass}: BLOCKED group {vic_all:?} — triangle {t} holds a \
                 victim outside every holder fan",
            );
            group_blocked.extend(vic_all.iter().copied());
            continue;
        }
        c441_log!(
            "[i13e-group] pass={pass}: ABSORB group of {} sites {vic_all:?} \
             holders={holders:?}",
            gsites.len(),
        );
        return Some((
            plan,
            gsites
                .iter()
                .map(|(vs, s)| (*s, vs.iter().copied().collect()))
                .collect(),
            None,
        ));
    }

    rehome_attempt(
        mesh,
        attribution,
        a,
        b,
        patches,
        curves,
        &rehome_cands,
        rehome_blocked,
        pass,
    )
}

/// §I13(f) f2 — the inverted-junction-pair RE-HOMING arm (wall-cycle half;
/// spec `specs/yang_441_trim_cdt_construction.md` §I13(f) f2). Gated OFF by
/// default (`YANG_441_REHOME`); runs strictly at the absorption fixed point
/// (no I13d site applied, no I13e group applied).
///
/// The candidates are the selector's single-victim `not_richer` refusals —
/// TRUE-corner pairs whose mesh cycle order contradicts their exact order
/// (both I13d certificates fired; richness refused CORRECTLY: absorption
/// would delete a true corner). The repair is re-homing the cut corner
/// across the shared rim: relocate `j_cut` onto the planner's `newJ_wall`
/// mint {S_j, W, K} and absorb `j_rim` (on K's waste side) into it. The
/// wall cycle re-routes cut-line → newJ_wall → C0, the neighbor band gains
/// its true wall corner (the survivor JOINS the S_j patch —
/// [`rebuild_rehome_fan`]), and the cone-side chains meet at the
/// RECOGNIZED rim×cut junction (carrier-identity, never minted twice).
///
/// Certificate chain, every decline loud and typed: the f1 planner's
/// construction certificates → the selector's inversion re-verified from
/// its own plumbed t-params → rim×cut junction recognized by carrier
/// identity → per-holder fan plans → bare-collapse guard → ride-along
/// triangle orientation guard. `census` mode runs the whole chain and
/// reports without mutating.
#[allow(clippy::too_many_arguments)]
pub(super) fn rehome_attempt(
    mesh: &Mesh,
    attribution: &TriangleAttributionMap,
    a: &BRep,
    b: &BRep,
    patches: &[crate::stage4_splice::SplicePatch],
    curves: &std::collections::BTreeMap<(u32, u32), Curve>,
    cands: &[crate::stage4_fold_risk::RehomeCandidate],
    rehome_blocked: &mut std::collections::BTreeSet<(u32, u32)>,
    pass: usize,
) -> Option<AbsorptionPlan> {
    use crate::stage4_rehome::{
        inversion_still_holds, plan_corner_rehoming, recognize_rim_junction, rehome_mode,
        RehomeDecline, RehomeMode,
    };
    let mode = rehome_mode();
    if cands.is_empty() {
        return None;
    }
    let mut would_apply = 0usize;
    let mut skipped_blocked = 0usize;
    let pair_key = |a: u32, b: u32| (a.min(b), a.max(b));
    let mut survivors: Vec<(
        &crate::stage4_fold_risk::RehomeCandidate,
        crate::stage4_rehome::RehomePlan,
        Vec<crate::stage4_construct::PatchRebuild>,
        Point3,
    )> = Vec::new();
    // f2c: material-KEPT views (margin < 0) are not declined per-view —
    // they are collected and grouped by phantom below: a site presenting
    // BOTH mirrored views as kept is the measured inverted-junction-pair
    // signature, and its repair re-homes the phantom, absorbing nothing.
    struct KeptView<'a> {
        cand: &'a crate::stage4_fold_risk::RehomeCandidate,
        plan: crate::stage4_rehome::RehomePlan,
        rim_j: u32,
        kept_edge: Option<(u32, &'a Curve)>,
    }
    let mut kept_views: Vec<KeptView> = Vec::new();
    'cand: for cand in cands {
        if rehome_blocked.contains(&pair_key(cand.j, cand.v)) {
            skipped_blocked += 1;
            continue;
        }
        let carried = |x: u32, pos: [f64; 3]| {
            crate::stage4_correct::carried_surfaces(mesh, &attribution.attributions, a, b, x, pos)
        };
        let pos = |x: u32| mesh.verts[x as usize].as_array();
        let (cj, cv) = (carried(cand.j, pos(cand.j)), carried(cand.v, pos(cand.v)));
        let decline = |d: RehomeDecline, blocked: &mut std::collections::BTreeSet<(u32, u32)>| {
            eprintln!(
                "[i13f-rehome] pass={pass}: DECLINE pair v{}/v{} reason={d:?}",
                cand.j, cand.v
            );
            blocked.insert(pair_key(cand.j, cand.v));
        };
        let plan = match plan_corner_rehoming(cand.j, pos(cand.j), &cj, cand.v, pos(cand.v), &cv) {
            Ok(p) => p,
            Err(d) => {
                decline(d, rehome_blocked);
                continue;
            }
        };
        // The selector's inversion authority, re-verified from its own
        // plumbed t-params (guards the plumb, not the math).
        if !inversion_still_holds(cand) {
            decline(RehomeDecline::InversionUnverified, rehome_blocked);
            continue;
        }
        // Recognize the existing rim×cut junction {S_i, S_j, K} by carrier
        // IDENTITY. The distance scan is a search pre-filter at the pair's
        // own scale (the planner's window formula), not an acceptance band.
        let gap = {
            let (p, q) = (pos(plan.j_cut), pos(plan.j_rim));
            ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt()
        };
        let scale = plan.new_rim.iter().fold(0.0f64, |m, &c| m.max(c.abs()));
        let win = 16.0 * gap + 1e3 * cad_primitives::TAU_EVAL * (1.0 + scale);
        let near: Vec<(u32, Vec<Surface>)> = (0..mesh.verts.len() as u32)
            .filter(|&x| x != plan.j_cut && x != plan.j_rim)
            .filter(|&x| {
                let p = pos(x);
                let d2 = (p[0] - plan.new_rim[0]).powi(2)
                    + (p[1] - plan.new_rim[1]).powi(2)
                    + (p[2] - plan.new_rim[2]).powi(2);
                d2 <= win * win
            })
            .map(|x| (x, carried(x, pos(x))))
            .collect();
        let rim_j = match recognize_rim_junction(&near, &[plan.s_i, plan.s_j, plan.cut]) {
            Ok(v) => v,
            Err(d) => {
                decline(d, rehome_blocked);
                continue;
            }
        };
        let rim_dist = {
            let p = pos(rim_j);
            ((p[0] - plan.new_rim[0]).powi(2)
                + (p[1] - plan.new_rim[1]).powi(2)
                + (p[2] - plan.new_rim[2]).powi(2))
            .sqrt()
        };
        // Mint-interposition REPORT (feeds the f3 chain-split work order,
        // not a certificate): both old corners' continuing chains straddle
        // the mint's exact parameter — the missed crossing was never
        // inserted into EITHER the s_j∩wall conic chain or the s_j∩cut
        // trace chain, so the f3 surgery must split both there. Measured
        // symmetric across a site's two mirrored views (censuses 7–8,
        // 2026-08-28), which is why this is a report and not a view
        // discriminator: every pair-local order/side test is symmetric at
        // this defect — the defective chains are the arrangement's
        // pre-relocation order fossilized, and the exact relocation
        // swapped the pair along the rim. View discrimination (which old
        // corner is waste) is kept/waste information and stands OPEN as
        // f2b; until it lands, the cross-view exclusivity guard below
        // refuses both-certified sites loudly.
        let far: std::collections::BTreeSet<u32> = patches
            .iter()
            .filter(|pat| pat.surface == plan.cut)
            .flat_map(|pat| pat.cycles.iter())
            .flat_map(|cyc| {
                let n = cyc.len();
                (0..n)
                    .filter(move |&i| cyc[i] == rim_j)
                    .map(move |i| [cyc[(i + n - 1) % n], cyc[(i + 1) % n]])
            })
            .flatten()
            .filter(|&x| x != plan.j_cut && x != plan.j_rim && x != rim_j)
            .collect();
        let mut kept_edge: Option<(u32, &Curve)> = None;
        if let (1, Some(&w)) = (far.len(), far.iter().next()) {
            let key = (rim_j.min(w), rim_j.max(w));
            let verdict = curves.get(&key).and_then(|c| {
                let t = |p: [f64; 3]| {
                    crate::stage4_correct::conic_param(c, Point3::new(p[0], p[1], p[2]))
                };
                crate::stage4_rehome::mint_interposes(
                    c,
                    t(pos(rim_j))?,
                    t(plan.new_wall)?,
                    t(pos(w))?,
                )
            });
            eprintln!(
                "[i13f-rehome]   kept-edge report: v{rim_j}-v{w} mint_interposes={verdict:?}"
            );
            // f2c consumes the kept edge only when the mint provably
            // interposes on its conic — the chain-split/insert target.
            if verdict == Some(true) {
                kept_edge = curves.get(&key).map(|c| (w, c));
            }
        }
        // f2b — kept/waste view discrimination by MATERIAL evidence (the
        // stage-2 labels, read as the corner witness patch's result-outward
        // winding on the view's cut plane). Strictly positive margin =
        // the victim sits in void at corner scale (this view is the true
        // mirror); strictly negative = it sits against kept material (the
        // wrong mirror — typed decline). Anything unreadable declines and
        // the site keeps its loud standing wall.
        let mat = match crate::stage4_rehome::view_material_verdict(
            mesh,
            patches,
            &plan.cut,
            rim_j,
            pos(plan.j_rim),
        ) {
            Ok(m) => m,
            Err(d) => {
                decline(d, rehome_blocked);
                continue;
            }
        };
        eprintln!(
            "[i13f-rehome]   f2b material: pair v{}/v{} victim=v{} margin={:+.3e} \
             floor={:.1e} cut_patches={}",
            cand.j, cand.v, plan.j_rim, mat.margin, mat.floor, mat.cut_patches
        );
        // f2c precondition census (report-only, census mode): the corrected
        // surgery's input anatomy, measured before building it — which
        // patches hold the phantom `j_cut` (by frame role), its cycle
        // neighbors with each edge typed against the pair's own fossil
        // conic vs the kept-edge conic (the retype/split targets), the
        // S_i-patch fan link of `j_cut` in order (does it run from the
        // victim-side corner to the recognized junction — the rim stub's
        // endpoints?), and whether the S_j patch already holds it (it must
        // NOT — the seam-insert is the join).
        if mode == RehomeMode::Census {
            use crate::stage4_correct::conics_equal_up_to_normal_sign;
            let role = |s: &crate::Surface| {
                if *s == plan.s_i {
                    "s_i"
                } else if *s == plan.s_j {
                    "s_j"
                } else if *s == plan.wall {
                    "wall"
                } else if *s == plan.cut {
                    "cut"
                } else {
                    "other"
                }
            };
            let kept_curve = curves.get(&{
                let far: std::collections::BTreeSet<u32> = patches
                    .iter()
                    .filter(|pat| pat.surface == plan.cut)
                    .flat_map(|pat| pat.cycles.iter())
                    .flat_map(|cyc| {
                        let n = cyc.len();
                        (0..n)
                            .filter(move |&i| cyc[i] == rim_j)
                            .map(move |i| [cyc[(i + n - 1) % n], cyc[(i + 1) % n]])
                    })
                    .flatten()
                    .filter(|&x| x != plan.j_cut && x != plan.j_rim && x != rim_j)
                    .collect();
                match (far.len(), far.iter().next()) {
                    (1, Some(&w)) => (rim_j.min(w), rim_j.max(w)),
                    _ => (u32::MAX, u32::MAX),
                }
            });
            let type_of = |x: u32, y: u32| -> &'static str {
                match curves.get(&(x.min(y), x.max(y))) {
                    None => "untyped",
                    Some(c)
                        if *c == cand.curve || conics_equal_up_to_normal_sign(c, &cand.curve) =>
                    {
                        "pair-conic"
                    }
                    Some(c)
                        if kept_curve
                            .is_some_and(|k| k == c || conics_equal_up_to_normal_sign(k, c)) =>
                    {
                        "kept-conic"
                    }
                    Some(_) => "other",
                }
            };
            for (pi, pat) in patches.iter().enumerate() {
                let holds = pat
                    .tris
                    .iter()
                    .any(|&t| mesh.tris[t as usize].contains(&plan.j_cut));
                let adj: Vec<(u32, u32)> = pat
                    .cycles
                    .iter()
                    .flat_map(|cyc| {
                        let n = cyc.len();
                        (0..n)
                            .filter(move |&i| cyc[i] == plan.j_cut)
                            .map(move |i| (cyc[(i + n - 1) % n], cyc[(i + 1) % n]))
                    })
                    .collect();
                if !holds && adj.is_empty() {
                    continue;
                }
                let adj_s: Vec<String> = adj
                    .iter()
                    .map(|&(p, q)| {
                        format!(
                            "(v{p}[{}] .. v{q}[{}])",
                            type_of(p, plan.j_cut),
                            type_of(plan.j_cut, q)
                        )
                    })
                    .collect();
                // Ordered fan link of `j_cut` on this patch: boundary chain
                // of its triangle region, excluding the vertex itself (the
                // fan_rebuild_core walk, read-only).
                let mut directed: std::collections::BTreeSet<(u32, u32)> = Default::default();
                for &t in &pat.tris {
                    let tri = mesh.tris[t as usize];
                    if !tri.contains(&plan.j_cut) {
                        continue;
                    }
                    for k in 0..3 {
                        directed.insert((tri[k], tri[(k + 1) % 3]));
                    }
                }
                let mut nexts: std::collections::BTreeMap<u32, u32> = Default::default();
                for &(x, y) in &directed {
                    if !directed.contains(&(y, x)) && x != plan.j_cut && y != plan.j_cut {
                        nexts.insert(x, y);
                    }
                }
                let start = nexts
                    .keys()
                    .find(|x| !nexts.values().any(|v| v == *x))
                    .copied();
                let mut link: Vec<u32> = Vec::new();
                let mut cur = start;
                while let Some(v) = cur {
                    if link.contains(&v) {
                        break;
                    }
                    link.push(v);
                    cur = nexts.get(&v).copied();
                }
                eprintln!(
                    "[i13f-rehome]   f2c precond: pair v{}/v{} patch={pi} role={} \
                     holds_j_cut={holds} cyc_adj={} link={:?} has_j_rim={} has_rim_j={}",
                    cand.j,
                    cand.v,
                    role(&pat.surface),
                    adj_s.join(" "),
                    link,
                    pat.cycles.iter().any(|c| c.contains(&plan.j_rim)),
                    pat.cycles.iter().any(|c| c.contains(&rim_j)),
                );
            }
            // The S_j-side JOIN edge: the victim's cycle adjacency on the
            // S_j patch (in the victim-is-B view this is B's C0 chain — the
            // boundary edge the moved phantom must be seam-inserted into).
            for (pi, pat) in patches.iter().enumerate() {
                if pat.surface != plan.s_j {
                    continue;
                }
                let adj: Vec<String> = pat
                    .cycles
                    .iter()
                    .flat_map(|cyc| {
                        let n = cyc.len();
                        (0..n)
                            .filter(move |&i| cyc[i] == plan.j_rim)
                            .map(move |i| (cyc[(i + n - 1) % n], cyc[(i + 1) % n]))
                    })
                    .map(|(p, q)| {
                        format!(
                            "(v{p}[{}] .. v{q}[{}])",
                            type_of(p, plan.j_rim),
                            type_of(plan.j_rim, q)
                        )
                    })
                    .collect();
                if !adj.is_empty() {
                    eprintln!(
                        "[i13f-rehome]   f2c s_j-join: pair v{}/v{} patch={pi} \
                         victim_cyc_adj={} holds_j_cut={}",
                        cand.j,
                        cand.v,
                        adj.join(" "),
                        pat.tris
                            .iter()
                            .any(|&t| mesh.tris[t as usize].contains(&plan.j_cut)),
                    );
                }
            }
        }
        if mat.margin < 0.0 {
            // NOT a per-view decline: a site whose BOTH mirrored views read
            // material-kept is the family's measured signature, and the f2c
            // site pass below owns it. Single-kept-view sites decline there.
            kept_views.push(KeptView {
                cand,
                plan,
                rim_j,
                kept_edge,
            });
            continue;
        }
        // Per-holder fan plans: every patch holding the victim rebuilds; the
        // survivor is evaluated at its mint and may JOIN the neighbor band.
        let new_wall = Point3::new(plan.new_wall[0], plan.new_wall[1], plan.new_wall[2]);
        let vic: std::collections::BTreeSet<u32> = [plan.j_rim].into_iter().collect();
        let holders: Vec<usize> = patches
            .iter()
            .enumerate()
            .filter(|&(_pj, pat)| {
                pat.tris
                    .iter()
                    .any(|&t| mesh.tris[t as usize].contains(&plan.j_rim))
            })
            .map(|(pj, _)| pj)
            .collect();
        let chartable = |s: &Surface| crate::stage4_project::SurfaceChart::supports(s);
        if holders.is_empty() || holders.iter().any(|&h| !chartable(&patches[h].surface)) {
            eprintln!(
                "[i13f-rehome] pass={pass}: REFUSED pair v{}/v{} — unchartable (or no) \
                 holder in {holders:?}",
                cand.j, cand.v
            );
            rehome_blocked.insert(pair_key(cand.j, cand.v));
            continue;
        }
        let mut fans = Vec::with_capacity(holders.len());
        for &h in &holders {
            match crate::stage4_construct::rebuild_rehome_fan(
                mesh,
                h,
                &patches[h],
                &vic,
                plan.j_cut,
                new_wall,
            ) {
                Ok(r) => fans.push(r),
                Err(e) => {
                    eprintln!(
                        "[i13f-rehome] pass={pass}: DECLINED patch {h} for pair v{}/v{} — {e:?}",
                        cand.j, cand.v
                    );
                    rehome_blocked.insert(pair_key(cand.j, cand.v));
                    continue 'cand;
                }
            }
        }
        // Bare-collapse guard: the victim must not survive anywhere outside
        // the rebuilt fans.
        let planned: std::collections::BTreeSet<u32> = fans
            .iter()
            .flat_map(|r| r.old_tris.iter().copied())
            .collect();
        if let Some(t) = (0..mesh.tris.len() as u32)
            .find(|t| !planned.contains(t) && mesh.tris[*t as usize].contains(&plan.j_rim))
        {
            eprintln!(
                "[i13f-rehome] pass={pass}: BLOCKED pair v{}/v{} — triangle {t} holds the \
                 victim outside every holder fan",
                cand.j, cand.v
            );
            rehome_blocked.insert(pair_key(cand.j, cand.v));
            continue;
        }
        // Ride-along orientation guard: every triangle that keeps the moved
        // vertex OUTSIDE the rebuilt fans must keep its orientation sense
        // under the relocation (sign-only, no band).
        for (t, tri) in mesh.tris.iter().enumerate() {
            if planned.contains(&(t as u32)) || !tri.contains(&plan.j_cut) {
                continue;
            }
            let at = |v: u32, moved: bool| -> Point3 {
                if moved && v == plan.j_cut {
                    new_wall
                } else {
                    mesh.verts[v as usize]
                }
            };
            let av =
                |moved: bool| crate::stage4_splice::area_vector(&[*tri], &|v: u32| at(v, moved));
            let d = crate::stage4_splice::dot3(av(false), av(true));
            if !(d.is_finite() && d > 0.0) {
                decline(RehomeDecline::TriangleFlip, rehome_blocked);
                continue 'cand;
            }
        }
        eprintln!(
            "[i13f-rehome] pass={pass}: CERTIFIED pair v{}/v{} j_cut=v{} -> \
             ({:.9e},{:.9e},{:.9e}) absorb j_rim=v{} rim_junction=v{rim_j} \
             rim_dist={rim_dist:.3e} residual={:.3e} holders={holders:?}",
            cand.j,
            cand.v,
            plan.j_cut,
            plan.new_wall[0],
            plan.new_wall[1],
            plan.new_wall[2],
            plan.j_rim,
            plan.residual,
        );
        survivors.push((cand, plan, fans, new_wall));
    }
    // Cross-view EXCLUSIVITY: a site (keyed by the phantom `j_cut`) whose
    // TWO mirrored views both survive the full certificate chain is never
    // guessed between — decline both, loudly. Exactly one surviving view
    // is the apply/report unit.
    let mut exclusive: Vec<&(_, _, _, _)> = Vec::new();
    for sv in &survivors {
        let twins = survivors.iter().filter(|o| o.1.j_cut == sv.1.j_cut).count();
        if twins == 1 {
            exclusive.push(sv);
        } else {
            let (cand, plan, ..) = sv;
            eprintln!(
                "[i13f-rehome] pass={pass}: DECLINE pair v{}/v{} reason=AmbiguousViews \
                 ({} certified views share j_cut=v{})",
                cand.j, cand.v, twins, plan.j_cut
            );
            rehome_blocked.insert(pair_key(cand.j, cand.v));
        }
    }
    for sv in &exclusive {
        let (cand, plan, fans, new_wall) = sv;
        eprintln!(
            "[i13f-rehome] pass={pass}: {} pair v{}/v{} j_cut=v{} absorb j_rim=v{}",
            if mode == RehomeMode::On {
                "APPLY"
            } else {
                "WOULD-APPLY"
            },
            cand.j,
            cand.v,
            plan.j_cut,
            plan.j_rim,
        );
        if mode == RehomeMode::On {
            return Some((
                fans.clone(),
                vec![(plan.j_cut, vec![plan.j_rim])],
                Some((plan.j_cut, *new_wall)),
            ));
        }
        would_apply += 1;
    }
    // f2c — the corrected surgery for BOTH-CORNERS-KEPT sites (the family's
    // measured signature, censuses 10–12): the only false object is the
    // phantom. Relocate it to the mint, delete its S_i fan (the fossil
    // sliver overhanging the rim — the link IS the existing rim chain,
    // whose ends are the two true corners), and seam-insert it into BOTH
    // S_j fragments' boundary chains at its interposed parameter (one
    // fragment per view's kept corner). Nothing is absorbed. Every
    // certificate failure declines the whole site loudly and pair-blocks
    // both views.
    {
        use crate::stage4_construct::{delete_boundary_fan, split_boundary_edge};
        use crate::stage4_correct::{conic_param, conics_equal_up_to_normal_sign};
        let pos = |x: u32| mesh.verts[x as usize].as_array();
        let mut by_site: std::collections::BTreeMap<u32, Vec<&KeptView>> = Default::default();
        for kv in &kept_views {
            by_site.entry(kv.plan.j_cut).or_default().push(kv);
        }
        'site: for (&jc, views) in &by_site {
            let decline_site =
                |d: crate::stage4_rehome::RehomeDecline,
                 blocked: &mut std::collections::BTreeSet<(u32, u32)>| {
                    for v in views {
                        eprintln!(
                            "[i13f-rehome] pass={pass}: DECLINE pair v{}/v{} reason={d:?} \
                         (f2c site j_cut=v{jc})",
                            v.cand.j, v.cand.v
                        );
                        blocked.insert(pair_key(v.cand.j, v.cand.v));
                    }
                };
            let [va, vb] = match views[..] {
                [a2, b2] => [a2, b2],
                _ => {
                    decline_site(RehomeDecline::NotAKeptPair, rehome_blocked);
                    continue;
                }
            };
            // One site, one frame: shared bands, distinct kept corners, and
            // ONE mint (independently solved per view; they must agree at
            // the evaluation-noise floor).
            let corners: std::collections::BTreeSet<u32> =
                [va.plan.j_rim, vb.plan.j_rim].into_iter().collect();
            if va.plan.s_i != vb.plan.s_i || va.plan.s_j != vb.plan.s_j || corners.len() != 2 {
                decline_site(RehomeDecline::NotAKeptPair, rehome_blocked);
                continue;
            }
            let mint = va.plan.new_wall;
            let scale = mint.iter().fold(0.0f64, |m, &c| m.max(c.abs()));
            let floor = 64.0 * cad_primitives::TAU_EVAL * (1.0 + scale);
            let mint_gap = {
                let o = vb.plan.new_wall;
                ((mint[0] - o[0]).powi(2) + (mint[1] - o[1]).powi(2) + (mint[2] - o[2]).powi(2))
                    .sqrt()
            };
            if !(mint_gap.is_finite() && mint_gap <= floor) {
                decline_site(RehomeDecline::MintMismatch, rehome_blocked);
                continue;
            }
            let mint_pt = Point3::new(mint[0], mint[1], mint[2]);
            // Measured precondition: no S_j patch holds the phantom yet.
            if patches.iter().any(|p| {
                p.surface == va.plan.s_j
                    && p.tris.iter().any(|&t| mesh.tris[t as usize].contains(&jc))
            }) {
                decline_site(RehomeDecline::AlreadyJoined, rehome_blocked);
                continue;
            }
            // The S_i fossil fan: exactly one holder patch, deletion link
            // ends = the two true corners.
            let si: Vec<usize> = patches
                .iter()
                .enumerate()
                .filter(|(_, p)| {
                    p.surface == va.plan.s_i
                        && p.tris.iter().any(|&t| mesh.tris[t as usize].contains(&jc))
                })
                .map(|(pi, _)| pi)
                .collect();
            let [si_pi] = si[..] else {
                eprintln!(
                    "[i13f-rehome]   f2c site j_cut=v{jc}: {} S_i holder patches {si:?}",
                    si.len()
                );
                decline_site(RehomeDecline::SiFanUnresolved, rehome_blocked);
                continue;
            };
            let (si_del, link) = match delete_boundary_fan(mesh, si_pi, &patches[si_pi], jc) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("[i13f-rehome]   f2c site j_cut=v{jc}: fan delete — {e:?}");
                    decline_site(RehomeDecline::SiFanUnresolved, rehome_blocked);
                    continue;
                }
            };
            let ends: std::collections::BTreeSet<u32> =
                [link[0], link[link.len() - 1]].into_iter().collect();
            if ends != corners {
                eprintln!(
                    "[i13f-rehome]   f2c site j_cut=v{jc}: link ends {ends:?} != corners \
                     {corners:?} (link {link:?})"
                );
                decline_site(RehomeDecline::SiFanUnresolved, rehome_blocked);
                continue;
            }
            // f2c-2 HOLE-ANATOMY census (read-only): everything the
            // junction-layer re-fill must know, measured before building it.
            // (a) Every hole-boundary edge (link + the two jc spokes) with
            // its SURVIVING partner triangle — patch role, attribution, and
            // whether the partner already references the phantom (the
            // all-carrier split feasibility question). (b) Every hole vertex
            // in the shared-axis frame (θ, station, radius) against both
            // cones' expected radii — which band each link stretch lies on,
            // and where the true rim arc runs. (c) Patch attribution
            // uniformity for every patch the fill will touch.
            if mode == RehomeMode::Census {
                let frame = (|| {
                    let Surface::Cone { apex, axis_dir, .. } = va.plan.s_i else {
                        return None;
                    };
                    let ax = axis_dir.as_array();
                    let l = (ax[0] * ax[0] + ax[1] * ax[1] + ax[2] * ax[2]).sqrt();
                    let u = [ax[0] / l, ax[1] / l, ax[2] / l];
                    let (e1v, e2v) = crate::ortho_basis(axis_dir);
                    Some((apex.as_array(), u, e1v.as_array(), e2v.as_array()))
                })();
                if let Some((apex, u, e1, e2)) = frame {
                    let dot3 = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
                    let cyl = |p: [f64; 3]| -> (f64, f64, f64) {
                        let d = [p[0] - apex[0], p[1] - apex[1], p[2] - apex[2]];
                        let s = dot3(d, u);
                        let rad = [d[0] - s * u[0], d[1] - s * u[1], d[2] - s * u[2]];
                        let r = dot3(rad, rad).sqrt();
                        (dot3(rad, e2).atan2(dot3(rad, e1)), s, r)
                    };
                    let tan_of = |surf: &Surface| -> Option<(f64, f64)> {
                        let Surface::Cone {
                            apex: a2,
                            half_angle,
                            ..
                        } = surf
                        else {
                            return None;
                        };
                        let d = a2.as_array();
                        let d = [d[0] - apex[0], d[1] - apex[1], d[2] - apex[2]];
                        Some((dot3(d, u), half_angle.tan()))
                    };
                    let (a_i, t_i) = tan_of(&va.plan.s_i).unwrap_or((0.0, f64::NAN));
                    let (a_j, t_j) = tan_of(&va.plan.s_j).unwrap_or((0.0, f64::NAN));
                    let (s_rim, r_rim) = match va.plan.rim {
                        Curve::Circle { center, radius, .. } => {
                            let c = center.as_array();
                            (
                                dot3([c[0] - apex[0], c[1] - apex[1], c[2] - apex[2]], u),
                                radius,
                            )
                        }
                        _ => (f64::NAN, f64::NAN),
                    };
                    let (th_m, s_m, r_m) = cyl(mint);
                    eprintln!(
                        "[i13f-rehome]   f2c-2 frame: site j_cut=v{jc} s_rim={s_rim:.9} \
                         r_rim={r_rim:.9} a_i={a_i:.6} tan_i={t_i:.6} a_j={a_j:.6} \
                         tan_j={t_j:.6} mint(th,s,r)=({th_m:.9},{s_m:.9},{r_m:.9})"
                    );
                    let hole: Vec<u32> = std::iter::once(jc).chain(link.iter().copied()).collect();
                    for &v2 in &hole {
                        let p = pos(v2);
                        let (th, s, r) = cyl(p);
                        eprintln!(
                            "[i13f-rehome]   f2c-2 vert: v{v2} th={th:+.9} s={s:.9} \
                             r={r:.9} r_i={:.9} r_j={:.9} p=({:.6},{:.6},{:.6})",
                            (s - a_i).abs() * t_i,
                            (s - a_j).abs() * t_j,
                            p[0],
                            p[1],
                            p[2]
                        );
                    }
                    let fan_set: std::collections::BTreeSet<u32> =
                        si_del.old_tris.iter().copied().collect();
                    let n_hole = hole.len();
                    for k in 0..n_hole {
                        let (x, y) = (hole[k], hole[(k + 1) % n_hole]);
                        let users: Vec<u32> = (0..mesh.tris.len() as u32)
                            .filter(|&t| {
                                let tri = mesh.tris[t as usize];
                                tri.contains(&x) && tri.contains(&y)
                            })
                            .collect();
                        for &t in &users {
                            if fan_set.contains(&t) {
                                continue;
                            }
                            let tri = mesh.tris[t as usize];
                            let pi = patches.iter().position(|p| p.tris.contains(&t));
                            let role = pi.map(|pi| {
                                let s = &patches[pi].surface;
                                if *s == va.plan.s_i {
                                    "s_i"
                                } else if *s == va.plan.s_j {
                                    "s_j"
                                } else if *s == va.plan.wall {
                                    "wall"
                                } else if *s == va.plan.cut {
                                    "cut"
                                } else {
                                    "other"
                                }
                            });
                            eprintln!(
                                "[i13f-rehome]   f2c-2 hole-edge (v{x},v{y}): partner \
                                 t{t}={tri:?} patch={pi:?} role={role:?} attr={:?} \
                                 has_jc={}",
                                attribution.attributions[t as usize],
                                tri.contains(&jc)
                            );
                        }
                        if users.len() != 2 {
                            eprintln!(
                                "[i13f-rehome]   f2c-2 hole-edge (v{x},v{y}): {} users \
                                 {users:?} (expected 2)",
                                users.len()
                            );
                        }
                    }
                    let mut attr_pats: Vec<usize> = vec![si_pi];
                    for (pi, pat) in patches.iter().enumerate() {
                        if pat.surface == va.plan.s_j
                            && pat
                                .cycles
                                .iter()
                                .any(|c| corners.iter().any(|cn| c.contains(cn)))
                        {
                            attr_pats.push(pi);
                        }
                    }
                    for pi in attr_pats {
                        let mut attrs: Vec<_> = patches[pi]
                            .tris
                            .iter()
                            .map(|&t| attribution.attributions[t as usize])
                            .collect();
                        attrs.sort();
                        attrs.dedup();
                        eprintln!(
                            "[i13f-rehome]   f2c-2 patch-attr: patch={pi} tris={} \
                             attrs={attrs:?}",
                            patches[pi].tris.len()
                        );
                    }
                    // The arrangement's OWN local topology claim is the fill's
                    // authority — probe every hole vertex's cycle windows
                    // (carried surfaces, neighbors, per-edge curve refs).
                    for &v2 in &hole {
                        probe_run_neighborhood(mesh, attribution, a, b, patches, curves, v2, jc);
                    }
                }
            }
            // The two seam-inserts: per view, the S_j fragment whose cycle
            // holds the view's KEPT corner (its recognized rim junction),
            // on the boundary edge carrying the view's kept conic with the
            // mint's parameter interposed.
            // f2c-2 — resolve BOTH views' seam-insert targets first (the
            // corrected surgery needs the fragment identities before the
            // chord/fill construction below).
            let mut insert_targets: Vec<(usize, u32)> = Vec::new();
            for v in [va, vb] {
                let Some((_, kc)) = v.kept_edge else {
                    decline_site(RehomeDecline::KeptEdgeUnresolved, rehome_blocked);
                    continue 'site;
                };
                let mut targets: Vec<(usize, u32)> = Vec::new();
                for (pi, pat) in patches.iter().enumerate() {
                    if pat.surface != v.plan.s_j {
                        continue;
                    }
                    for cyc in &pat.cycles {
                        let n = cyc.len();
                        for i in 0..n {
                            if cyc[i] != v.rim_j {
                                continue;
                            }
                            for nb in [cyc[(i + n - 1) % n], cyc[(i + 1) % n]] {
                                let key = (v.rim_j.min(nb), v.rim_j.max(nb));
                                let Some(c) = curves.get(&key) else { continue };
                                if !(c == kc || conics_equal_up_to_normal_sign(c, kc)) {
                                    continue;
                                }
                                let t = |p: [f64; 3]| conic_param(c, Point3::new(p[0], p[1], p[2]));
                                let inter = (|| {
                                    crate::stage4_rehome::mint_interposes(
                                        c,
                                        t(pos(v.rim_j))?,
                                        t(mint)?,
                                        t(pos(nb))?,
                                    )
                                })();
                                if inter == Some(true) {
                                    targets.push((pi, nb));
                                }
                            }
                        }
                    }
                }
                targets.sort_unstable();
                targets.dedup();
                // f2c-2 census: every user of the chosen insert edge — the
                // measured 2→1 partner tears name the OTHER carriers the
                // all-carrier split must also split.
                if mode == RehomeMode::Census {
                    if let [(_, nb)] = targets[..] {
                        let (x, y) = (v.rim_j, nb);
                        let users: Vec<u32> = (0..mesh.tris.len() as u32)
                            .filter(|&t| {
                                let tri = mesh.tris[t as usize];
                                tri.contains(&x) && tri.contains(&y)
                            })
                            .collect();
                        for &t in &users {
                            let tri = mesh.tris[t as usize];
                            let pi = patches.iter().position(|p| p.tris.contains(&t));
                            let role = pi.map(|pi| {
                                let s = &patches[pi].surface;
                                if *s == v.plan.s_i {
                                    "s_i"
                                } else if *s == v.plan.s_j {
                                    "s_j"
                                } else if *s == v.plan.wall {
                                    "wall"
                                } else if *s == v.plan.cut {
                                    "cut"
                                } else {
                                    "other"
                                }
                            });
                            eprintln!(
                                "[i13f-rehome]   f2c-2 insert-edge (v{x},v{y}): user \
                                 t{t}={tri:?} patch={pi:?} role={role:?} attr={:?} \
                                 has_jc={}",
                                attribution.attributions[t as usize],
                                tri.contains(&jc)
                            );
                        }
                    }
                }
                let [(frag_pi, nb)] = targets[..] else {
                    eprintln!(
                        "[i13f-rehome]   f2c site j_cut=v{jc}: {} insert targets at \
                         corner v{} — {targets:?}",
                        targets.len(),
                        v.rim_j
                    );
                    decline_site(RehomeDecline::InsertEdgeUnresolved, rehome_blocked);
                    continue 'site;
                };
                insert_targets.push((frag_pi, nb));
            }
            // f2c-2 — corner roles by the deterministic link walk: the
            // link's END corner stays on the band patch's rim; the START
            // corner tucks below onto the neighbor-band side (the measured
            // chord-overshoot anatomy: keeping both corners on the band
            // boundary would claim the rim-window edge three times — the
            // fossil's own stacked-chord slit — so exactly one stays).
            let dropped = link[0];
            let kept = *link.last().expect("link ends were certified");
            // A view's seam-insert happens AT its RECOGNIZED junction
            // (`rim_j`) — the OPPOSITE corner of its victim `j_rim` — so the
            // fragment owning a corner belongs to the view whose `rim_j` IS
            // that corner.
            let view_ix = |corner: u32| [va, vb].into_iter().position(|v| v.rim_j == corner);
            let (Some(vd_ix), Some(vk_ix)) = (view_ix(dropped), view_ix(kept)) else {
                decline_site(RehomeDecline::NotAKeptPair, rehome_blocked);
                continue;
            };
            let (v_dropped, v_kept) = ([va, vb][vd_ix], [va, vb][vk_ix]);
            let (frag_d_pi, _) = insert_targets[vd_ix];
            let (frag_k_pi, _) = insert_targets[vk_ix];
            // Certificate: the kept corner interposes on the dropped
            // corner's link chord ALONG THE PLANNER'S OWN RIM CIRCLE (the
            // overshoot the chord split undoes; wrapped-Δ convention).
            let rim_t = |p: [f64; 3]| {
                crate::stage4_correct::conic_param(&va.plan.rim, Point3::new(p[0], p[1], p[2]))
            };
            let overshoot = (|| {
                crate::stage4_rehome::mint_interposes(
                    &va.plan.rim,
                    rim_t(pos(dropped))?,
                    rim_t(pos(kept))?,
                    rim_t(pos(link[1]))?,
                )
            })();
            if overshoot != Some(true) {
                eprintln!(
                    "[i13f-rehome]   f2c site j_cut=v{jc}: kept corner v{kept} does not \
                     interpose on chord (v{dropped},v{}) — overshoot={overshoot:?}",
                    link[1]
                );
                decline_site(RehomeDecline::HoleFillUnresolved, rehome_blocked);
                continue;
            }
            // Certificate: each end chord's surviving user is its own view's
            // fragment (the fill mates them by construction).
            let chord_user = |x: u32, y: u32| -> Option<u32> {
                let users: Vec<u32> = (0..mesh.tris.len() as u32)
                    .filter(|&t| {
                        let tri = mesh.tris[t as usize];
                        tri.contains(&x) && tri.contains(&y) && !si_del.old_tris.contains(&t)
                    })
                    .collect();
                match users[..] {
                    [t] => Some(t),
                    _ => None,
                }
            };
            let dropped_chord = chord_user(dropped, link[1]);
            let kept_chord = chord_user(kept, link[link.len() - 2]);
            let frag_holds =
                |pi: usize, t: Option<u32>| t.is_some_and(|t| patches[pi].tris.contains(&t));
            if !frag_holds(frag_d_pi, dropped_chord) || !frag_holds(frag_k_pi, kept_chord) {
                eprintln!(
                    "[i13f-rehome]   f2c site j_cut=v{jc}: end-chord users not on their \
                     views' fragments — dropped={dropped_chord:?} kept={kept_chord:?}"
                );
                decline_site(RehomeDecline::HoleFillUnresolved, rehome_blocked);
                continue;
            }
            // The S_i-side fill: seedless chart CDT of the link polygon
            // minus the dropped corner (the phantom leaves the band patch —
            // its true carriers are {S_j, W, K}).
            let mut si_fill = si_del;
            match crate::stage4_construct::refill_fan_hole(
                mesh,
                si_pi,
                &patches[si_pi],
                &link[1..],
                &si_fill.old_tris,
            ) {
                Ok(tris) => si_fill.new_tris = tris,
                Err(e) => {
                    eprintln!("[i13f-rehome]   f2c site j_cut=v{jc}: hole re-fill — {e:?}");
                    decline_site(RehomeDecline::HoleFillUnresolved, rehome_blocked);
                    continue;
                }
            }
            // The dropped corner's chord split: the kept corner (at its
            // CURRENT exact position) enters the dropped view's fragment
            // boundary chain — the rim chain reads true-order afterwards.
            let kept_pos = Point3::new(pos(kept)[0], pos(kept)[1], pos(kept)[2]);
            let mut chord_split = match split_boundary_edge(
                mesh,
                frag_d_pi,
                &patches[frag_d_pi],
                dropped,
                link[1],
                kept,
                kept_pos,
            ) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("[i13f-rehome]   f2c site j_cut=v{jc}: chord split — {e:?}");
                    decline_site(RehomeDecline::HoleFillUnresolved, rehome_blocked);
                    continue;
                }
            };
            // The bite triangle [dropped, kept, jc@mint] — all three exactly
            // on the neighbor band (both corners on the rim circle, the mint
            // on S_j by construction). It reconnects the two fragments and
            // mates the chord-split window edge, both spokes' insert
            // children, and nothing else. Oriented to the fragment's sense.
            let frag_sense = {
                let old: Vec<[u32; 3]> = patches[frag_d_pi]
                    .tris
                    .iter()
                    .map(|&t| mesh.tris[t as usize])
                    .collect();
                crate::stage4_splice::area_vector(&old, &|v2: u32| mesh.verts[v2 as usize])
            };
            let bite_pos = |v2: u32| -> Point3 {
                if v2 == jc {
                    mint_pt
                } else {
                    mesh.verts[v2 as usize]
                }
            };
            let mut bite = [dropped, kept, jc];
            let bd = crate::stage4_splice::dot3(
                frag_sense,
                crate::stage4_splice::area_vector(&[bite], &bite_pos),
            );
            if !(bd.is_finite() && bd != 0.0) {
                eprintln!(
                    "[i13f-rehome]   f2c site j_cut=v{jc}: bite orientation degenerate \
                     (d={bd:.3e})"
                );
                decline_site(RehomeDecline::HoleFillUnresolved, rehome_blocked);
                continue;
            }
            if bd < 0.0 {
                bite.swap(1, 2);
            }
            chord_split.new_tris.push(bite);
            let mut rebuilds = vec![si_fill, chord_split];
            let mut insert_report: Vec<String> = Vec::new();
            // The two seam-inserts (fragment side).
            let mut needle_corner = false;
            for (v, &(frag_pi, nb)) in [va, vb].into_iter().zip(&insert_targets) {
                match split_boundary_edge(
                    mesh,
                    frag_pi,
                    &patches[frag_pi],
                    v.rim_j,
                    nb,
                    jc,
                    mint_pt,
                ) {
                    // The NEEDLE corner (measured R0003 at the thin-band rim
                    // density): the dropped view's seam insert and the chord
                    // split claim the SAME fragment triangle — the corner's
                    // whole fan. One composite rebuild replaces both (and
                    // the bite, which is that triangle's own tip).
                    Ok(r) if v.rim_j == dropped && r.old_tris == rebuilds[1].old_tris => {
                        match crate::stage4_construct::drop_needle_corner(
                            mesh,
                            frag_pi,
                            &patches[frag_pi],
                            (dropped, nb, link[1]),
                            (jc, mint_pt),
                            kept,
                        ) {
                            Ok(c) => {
                                insert_report.push(format!(
                                    "patch={frag_pi} needle corner v{dropped} dropped, quad \
                                     (v{jc},v{nb},v{},v{kept})",
                                    link[1]
                                ));
                                rebuilds[1] = c;
                                needle_corner = true;
                            }
                            Err(e) => {
                                eprintln!(
                                    "[i13f-rehome]   f2c site j_cut=v{jc}: needle corner \
                                     v{dropped} — {e:?}"
                                );
                                decline_site(RehomeDecline::HoleFillUnresolved, rehome_blocked);
                                continue 'site;
                            }
                        }
                    }
                    Ok(r) => {
                        insert_report.push(format!("patch={frag_pi} edge=(v{},v{nb})", v.rim_j));
                        rebuilds.push(r);
                    }
                    Err(e) => {
                        eprintln!(
                            "[i13f-rehome]   f2c site j_cut=v{jc}: split at corner v{} \
                             — {e:?}",
                            v.rim_j
                        );
                        decline_site(RehomeDecline::InsertEdgeUnresolved, rehome_blocked);
                        continue 'site;
                    }
                }
            }
            // Each view's OWN corner leaves its OWN plane patch — the
            // mirrored absorb halves, each restricted to the plane its
            // corner actually terminates on (the f2b AmbiguousViews
            // symmetry resolved: both were right, one plane each). The fan
            // re-homes to the mint, which subsumes the insert edges' plane-
            // side partners (the measured 2→1 partner tears).
            if v_dropped.plan.wall == v_kept.plan.wall {
                decline_site(RehomeDecline::PlaneAbsorbUnresolved, rehome_blocked);
                continue;
            }
            for v in [v_dropped, v_kept] {
                let wall_pis: Vec<usize> = patches
                    .iter()
                    .enumerate()
                    .filter(|(_, p)| {
                        p.surface == v.plan.wall
                            && p.tris
                                .iter()
                                .any(|&t| mesh.tris[t as usize].contains(&v.plan.j_rim))
                    })
                    .map(|(pi, _)| pi)
                    .collect();
                let [wpi] = wall_pis[..] else {
                    eprintln!(
                        "[i13f-rehome]   f2c site j_cut=v{jc}: {} wall patches hold \
                         corner v{} — {wall_pis:?}",
                        wall_pis.len(),
                        v.plan.j_rim
                    );
                    decline_site(RehomeDecline::PlaneAbsorbUnresolved, rehome_blocked);
                    continue 'site;
                };
                let vic: std::collections::BTreeSet<u32> = [v.plan.j_rim].into_iter().collect();
                match crate::stage4_construct::rebuild_rehome_fan(
                    mesh,
                    wpi,
                    &patches[wpi],
                    &vic,
                    jc,
                    mint_pt,
                ) {
                    Ok(r) => {
                        insert_report.push(format!("absorb v{} on patch={wpi}", v.plan.j_rim));
                        rebuilds.push(r);
                    }
                    Err(e) => {
                        eprintln!(
                            "[i13f-rehome]   f2c site j_cut=v{jc}: plane absorb of v{} \
                             — {e:?}",
                            v.plan.j_rim
                        );
                        decline_site(RehomeDecline::PlaneAbsorbUnresolved, rehome_blocked);
                        continue 'site;
                    }
                }
            }
            // Ride-along orientation guard: every triangle keeping the moved
            // vertex OUTSIDE the planned rebuilds must keep its sense under
            // the relocation (sign-only, no band).
            let planned: std::collections::BTreeSet<u32> = rebuilds
                .iter()
                .flat_map(|r| r.old_tris.iter().copied())
                .collect();
            // Needle-corner certificate: the dropped corner leaves the mesh
            // only if EVERY triangle referencing it is rebuilt in this
            // batch (its S_i fossil fan, its plane fan, its needle) — a
            // surviving reference would be a dangling vertex use.
            if needle_corner {
                let foreign: Vec<u32> = mesh
                    .tris
                    .iter()
                    .enumerate()
                    .filter(|(t, tri)| tri.contains(&dropped) && !planned.contains(&(*t as u32)))
                    .map(|(t, _)| t as u32)
                    .collect();
                if !foreign.is_empty() {
                    eprintln!(
                        "[i13f-rehome]   f2c site j_cut=v{jc}: needle corner v{dropped} still \
                         referenced outside the batch by {foreign:?}"
                    );
                    decline_site(RehomeDecline::HoleFillUnresolved, rehome_blocked);
                    continue 'site;
                }
            }
            for (t, tri) in mesh.tris.iter().enumerate() {
                if planned.contains(&(t as u32)) || !tri.contains(&jc) {
                    continue;
                }
                let at = |v2: u32, moved: bool| -> Point3 {
                    if moved && v2 == jc {
                        mint_pt
                    } else {
                        mesh.verts[v2 as usize]
                    }
                };
                let av = |moved: bool| {
                    crate::stage4_splice::area_vector(&[*tri], &|v2: u32| at(v2, moved))
                };
                let d = crate::stage4_splice::dot3(av(false), av(true));
                if !(d.is_finite() && d > 0.0) {
                    decline_site(RehomeDecline::TriangleFlip, rehome_blocked);
                    continue 'site;
                }
            }
            eprintln!(
                "[i13f-rehome] pass={pass}: {} f2c site j_cut=v{jc} -> \
                 ({:.9e},{:.9e},{:.9e}) corners={corners:?} fan_delete{{patch={si_pi}, \
                 tris={}, link={link:?}}} inserts[{}]",
                if mode == RehomeMode::On {
                    "APPLY"
                } else {
                    "WOULD-APPLY"
                },
                mint[0],
                mint[1],
                mint[2],
                rebuilds[0].old_tris.len(),
                insert_report.join(", "),
            );
            for v in views {
                rehome_blocked.insert(pair_key(v.cand.j, v.cand.v));
            }
            if mode == RehomeMode::On {
                return Some((rebuilds, vec![(jc, vec![])], Some((jc, mint_pt))));
            }
            would_apply += 1;
        }
    }
    if mode == RehomeMode::Census {
        eprintln!(
            "[i13f-rehome] pass={pass}: census fixed point — {} candidate pair(s), \
             {would_apply} would apply, {skipped_blocked} skipped (blocked)",
            cands.len()
        );
    }
    None
}

/// I13d anchor probe (`YANG_441_RUN_PROBE`): the cycle neighbourhood of one
/// refused Fig-11 site, printed at the fold-merge all-blocked fixed point.
/// For every patch cycle containing the victim: a ±6 window of vertices
/// (id, moved flag + displacement, carried-surface refs, position) and the
/// window's edges (typed-curve ref + both endpoints' curve parameters).
/// Surfaces and curves are deduplicated per call and printed once as a
/// legend, so two windows sharing a conic share its ref — which curve a
/// chain is ON is exactly what the I13d selector must read.
#[allow(clippy::too_many_arguments)]
pub(super) fn probe_run_neighborhood(
    mesh: &Mesh,
    attribution: &TriangleAttributionMap,
    a: &BRep,
    b: &BRep,
    patches: &[crate::stage4_splice::SplicePatch],
    curves: &std::collections::BTreeMap<(u32, u32), Curve>,
    victim: u32,
    survivor: u32,
) {
    use crate::stage4_correct::{carried_surfaces, conic_param};
    let mut surf_legend: Vec<Surface> = Vec::new();
    let mut curve_legend: Vec<Curve> = Vec::new();
    fn legend_ref<T: PartialEq + Clone>(legend: &mut Vec<T>, item: &T) -> usize {
        match legend.iter().position(|x| x == item) {
            Some(i) => i,
            None => {
                legend.push(item.clone());
                legend.len() - 1
            }
        }
    }
    for (pj, pat) in patches.iter().enumerate() {
        for cyc in &pat.cycles {
            let n = cyc.len();
            let Some(k) = cyc.iter().position(|&v| v == victim) else {
                continue;
            };
            let s = legend_ref(&mut surf_legend, &pat.surface);
            eprintln!(
                "[i13d-run] victim=v{victim} survivor=v{survivor} patch={pj} surf=S{s} \
                 cyc_len={n} at={k}"
            );
            for off in -6i64..=6 {
                let idx = (k as i64 + off).rem_euclid(n as i64) as usize;
                let v = cyc[idx];
                let p = mesh.verts[v as usize].as_array();
                let carried: Vec<String> =
                    carried_surfaces(mesh, &attribution.attributions, a, b, v, p)
                        .into_iter()
                        .map(|surf| format!("S{}", legend_ref(&mut surf_legend, &surf)))
                        .collect();
                let pre_pos =
                    S4_PRE_POS.with(|c| c.borrow().as_ref().and_then(|pre| pre.get(&v).copied()));
                let moved = pre_pos.map(|q| {
                    ((q[0] - p[0]).powi(2) + (q[1] - p[1]).powi(2) + (q[2] - p[2]).powi(2)).sqrt()
                });
                eprintln!(
                    "[i13d-run]   [{off:+}] v{v} moved={moved:?} carried={carried:?} \
                     p=({:.9},{:.9},{:.9})",
                    p[0], p[1], p[2]
                );
                if let (Some(d), Some(q)) = (moved, pre_pos) {
                    if d > 0.0 {
                        eprintln!(
                            "[i13d-run]        pre=({:.9},{:.9},{:.9})",
                            q[0], q[1], q[2]
                        );
                    }
                }
                if off < 6 {
                    let w = cyc[(idx + 1) % n];
                    let key = (v.min(w), v.max(w));
                    match curves.get(&key) {
                        Some(curve) => {
                            let c = legend_ref(&mut curve_legend, curve);
                            let t_v = conic_param(curve, mesh.verts[v as usize]);
                            let t_w = conic_param(curve, mesh.verts[w as usize]);
                            // Pre-relocation params: the pre/post ORDER of a
                            // (sample, junction) pair along the shared curve
                            // is the I13d flip certificate's raw material.
                            let tp = |x: u32| {
                                S4_PRE_POS.with(|cell| {
                                    cell.borrow().as_ref().and_then(|pre| {
                                        pre.get(&x)
                                            .and_then(|&p| conic_param(curve, Point3::from(p)))
                                    })
                                })
                            };
                            eprintln!(
                                "[i13d-run]   edge v{v}-v{w}: C{c} t_v={t_v:?} t_w={t_w:?} \
                                 tp_v={:?} tp_w={:?}",
                                tp(v),
                                tp(w),
                            );
                        }
                        None => eprintln!("[i13d-run]   edge v{v}-v{w}: untyped"),
                    }
                }
            }
        }
    }
    for (i, surf) in surf_legend.iter().enumerate() {
        eprintln!("[i13d-run] legend S{i}={surf:?}");
    }
    for (i, curve) in curve_legend.iter().enumerate() {
        eprintln!("[i13d-run] legend C{i}={curve:?}");
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
