//! Stage 5/6 — B-Rep topology extraction and emission from the
//! corrected mesh: patch flood fill, boundary cycles, face/loop
//! emission (extracted verbatim from lib.rs — spec
//! `specs/yang_rs_lib_decomposition.md`, increment 8).

#[allow(clippy::wildcard_imports)]
use crate::*;

/// Diagnostic only: per-vertex incident surfaces, each with its operand-qualified
/// label (`A:Plane`) and the `Surface` itself so residuals can be evaluated.
type VertSurfMap = std::collections::HashMap<u32, Vec<(String, Surface)>>;

thread_local! {
    /// Diagnostic only (`YANG_S5_FOLD_PROBE`): the set of mesh vertices whose
    /// POSITION changed across Stage 4, captured by diffing `mesh.verts`
    /// before/after. Written only under the env gate.
    ///
    /// This is the correct "was it relocated?" oracle. The `relocations` vector
    /// is NOT — it carries conic `(vertex, circle-frame angle t)` retags only,
    /// and the torus arm (`stage4_correct.rs`, `vert_torus`) moves vertices via
    /// `relocate_onto_implicit_pair` WITHOUT pushing to it (torus edges are
    /// degree-4 polylines: no analytic curve, no `t` retag). Keying a probe on
    /// `relocations` is therefore blind on any torus model.
    ///
    /// `None` = unavailable (Stage 4 was not entered).
    ///
    /// Holds each vertex's PRE-Stage-4 POSITION, keyed by its index in the CURRENT
    /// mesh — not a displacement, and not a moved-only set. Displacement is then
    /// `post − pre` at report time, "moved" is `post != pre`, and a vertex with no
    /// entry was minted during Stage 4 (reported as `new`, never as `still`).
    ///
    /// Storing the position rather than the displacement is what makes this
    /// survive the rest of the pipeline. `pre = post − disp` is only valid if
    /// nothing moves the vertex again, but the KV15b collapse, the #194 sub-TAU
    /// edge collapse and the N50 f32 render weld all run AFTER Stage 4 and can
    /// move or drop a vertex; each is followed by a `compact_unreferenced_verts`
    /// that RENUMBERS. So the map is re-keyed through every one of those remaps
    /// (`probe_remap_pre_pos`), and the pre positions stay exact regardless.
    static S4_PRE_POS: std::cell::RefCell<Option<std::collections::HashMap<u32, [f64; 3]>>> =
        const { std::cell::RefCell::new(None) };

    /// Diagnostic only (`YANG_S5_FOLD_PROBE`): per-vertex set of incident
    /// surfaces from the pre-Stage-4 incidence map, qualified by OPERAND
    /// (`A:Plane`, `B:Torus` — the `YANG_S4_RIM_SNAP_TARGET` format).
    ///
    /// The operand qualifier is load-bearing, not cosmetic. `incidence` is built
    /// from EVERY patch boundary-cycle edge (`stage4_correct.rs`
    /// `compute_phase_a`), so an operand's OWN rim — A's plane patch meeting A's
    /// torus patch — lands in it with the same {Plane, Torus} kind signature as a
    /// cross-input A×B intersection edge. Recording kinds alone therefore cannot
    /// distinguish "on the A∩B curve" from "on A's own rim", and only the former
    /// is a Stage-4 relocation candidate (`build_intersection_curves` skips
    /// `input0 == input1`).
    /// The `Surface` itself is retained alongside the label so the probe can
    /// evaluate each one's implicit residual at the vertex's FINAL position. That
    /// residual is the only way to tell a vertex that never reached its curve from
    /// one that reached it at the WRONG POINT along it — the two have opposite
    /// fixes, and no displacement statistic distinguishes them.
    static S4_VERT_SURF: std::cell::RefCell<Option<VertSurfMap>> =
        const { std::cell::RefCell::new(None) };

    /// Diagnostic only (`YANG_S5_FOLD_PROBE`): per-vertex names of the incident
    /// edges that are KEYS of the pre-Stage-4 `intersection_curves` map.
    ///
    /// This — not the surface-kind set — is the exact "was this vertex a
    /// relocation candidate?" oracle, because that map is what Stage 4 relocates
    /// onto. Empty means Stage 4 had no analytic curve through the vertex at all.
    static S4_VERT_CURVE: std::cell::RefCell<
        Option<std::collections::HashMap<u32, std::collections::BTreeSet<&'static str>>>,
    > = const { std::cell::RefCell::new(None) };
}

/// Is the [`S4_PRE_POS`] pre-position oracle wanted this run?
///
/// TWO consumers now: the `YANG_S5_FOLD_PROBE` diagnostic columns and the N2-3b
/// `YANG_S4_FOLD_RISK` planner pass. They must share ONE predicate, because the
/// map is captured at one site, RE-KEYED at four compaction sites, and read at
/// several more — enabling the capture without the re-keys would leave the
/// planner reading indices that name different vertices, which is worse than
/// having no map at all (every column still populated, just wrong). That is the
/// `probe_remap_pre_pos` contract; this function is what keeps the two gates
/// from drifting apart.
fn s4_pre_pos_enabled() -> bool {
    std::env::var_os("YANG_S5_FOLD_PROBE").is_some()
        || std::env::var_os("YANG_S4_FOLD_RISK").is_some()
}

/// Diagnostic only (`YANG_S5_FOLD_PROBE`): re-key [`S4_PRE_POS`] through a
/// `compact_unreferenced_verts` remap, dropping vertices that did not survive.
///
/// MUST be called at EVERY compaction site. There are four (§4.5.3, KV15b, #194
/// sub-TAU edge collapse, N50 f32 weld), and the last three run even when Stage 4
/// itself did not collapse — so skipping them leaves the map keyed on indices that
/// name different vertices in the emitted loops, which is worse than having no
/// map: every column would still be populated, just wrong.
fn probe_remap_pre_pos(site: &str, remap: Option<&Vec<Option<u32>>>) {
    if !s4_pre_pos_enabled() {
        return;
    }
    let Some(remap) = remap else { return };
    S4_PRE_POS.with(|c| {
        let mut slot = c.borrow_mut();
        if let Some(old) = slot.take() {
            let before = old.len();
            let mut new: std::collections::HashMap<u32, [f64; 3]> =
                std::collections::HashMap::with_capacity(before);
            for (v, p) in old {
                if let Some(Some(nv)) = remap.get(v as usize) {
                    new.insert(*nv, p);
                }
            }
            // Report every re-key: which site fired is exactly what decides
            // whether a previous run's columns were index-aligned.
            eprintln!(
                "YANG_S5_REMAP site={site} kept={} dropped={} (pre-position map re-keyed)",
                new.len(),
                before - new.len(),
            );
            *slot = Some(new);
        }
    });
}

/// Diagnostic only (`YANG_S5_FOLD_PROBE`): record the per-vertex incidence and
/// intersection-curve maps the fold probe reports.
///
/// MUST be re-called after any §4.5.3 / KV15b collapse. A collapse renumbers
/// vertices (`compact_unreferenced_verts`), so maps captured before it are keyed
/// on indices that no longer name the same vertices — and the fold probe looks
/// its columns up by the POST-collapse index. Reporting the stale map would
/// attribute one vertex's incidence to a different vertex, which reads as
/// evidence rather than as a missing measurement.
fn probe_record_incidence(
    incidence: &std::collections::BTreeMap<(u32, u32), Vec<(InputId, Surface)>>,
    curves: &std::collections::BTreeMap<(u32, u32), Curve>,
) {
    if std::env::var_os("YANG_S5_FOLD_PROBE").is_none() {
        return;
    }
    let mut vs: VertSurfMap = Default::default();
    for (&(s, e), es) in incidence {
        for &(input, surf) in es {
            let n = match surf {
                Surface::Plane { .. } => "Plane",
                Surface::Cylinder { .. } => "Cylinder",
                Surface::Cone { .. } => "Cone",
                Surface::Sphere { .. } => "Sphere",
                Surface::Torus { .. } => "Torus",
            };
            // Operand-qualified: an own-rim edge (A:Plane|A:Torus) is NOT a
            // relocation candidate but is kind-indistinguishable from a
            // cross-input A×B edge. See the S4_VERT_SURF doc comment.
            let q = format!(
                "{}:{n}",
                match input {
                    InputId::A => "A",
                    InputId::B => "B",
                }
            );
            for v in [s, e] {
                let slot = vs.entry(v).or_default();
                // Dedup on the qualified label AND the surface, so two distinct
                // co-labelled surfaces (a vertex on two different `A:Plane`s) are
                // both kept — collapsing them would hide a junction.
                if !slot.iter().any(|(l, sf)| l == &q && *sf == surf) {
                    slot.push((q.clone(), surf));
                }
            }
        }
    }
    // Deterministic report order.
    for slot in vs.values_mut() {
        slot.sort_by(|a, b| a.0.cmp(&b.0));
    }
    S4_VERT_SURF.with(|c| *c.borrow_mut() = Some(vs));
    // The precise relocation-candidate oracle: which incident edges actually
    // carry an entry in the map Stage 4 relocates onto.
    let mut vc: std::collections::HashMap<u32, std::collections::BTreeSet<&'static str>> =
        Default::default();
    for (&(s, e), c) in curves {
        let n = match c {
            Curve::LineSegment => "LineSegment",
            Curve::Circle { .. } => "Circle",
            Curve::Ellipse { .. } => "Ellipse",
            Curve::Parabola { .. } => "Parabola",
            Curve::Hyperbola { .. } => "Hyperbola",
            Curve::SurfacePair { .. } => "SurfacePair",
        };
        vc.entry(s).or_default().insert(n);
        vc.entry(e).or_default().insert(n);
    }
    S4_VERT_CURVE.with(|c| *c.borrow_mut() = Some(vc));
}

/// Diagnostic only: one vertex's Stage-4 provenance, in the
/// `YANG_S6_NONPLANAR_PROBE` column format (pre position, displacement, incident
/// surfaces, intersection-curve keys). Populated only under
/// `YANG_S5_FOLD_PROBE`; `pre=NEW` means the vertex was minted during Stage 4.
fn probe_vertex_prov(vv: u32, q: [f64; 3]) -> String {
    let pre_s = match S4_PRE_POS.with(|c| c.borrow().as_ref().and_then(|m| m.get(&vv).copied())) {
        Some(p) => {
            let disp =
                ((q[0] - p[0]).powi(2) + (q[1] - p[1]).powi(2) + (q[2] - p[2]).powi(2)).sqrt();
            format!(
                "pre=({:.12},{:.12},{:.12}) disp={disp:.4e}",
                p[0], p[1], p[2]
            )
        }
        None => "pre=NEW".to_string(),
    };
    let inc_s = S4_VERT_SURF.with(|c| {
        c.borrow().as_ref().map_or_else(
            || "?".to_string(),
            |m| {
                m.get(&vv).map_or_else(
                    || "-".to_string(),
                    // NOT deduped by label: two distinct surfaces of one operand
                    // share one label, and that multiplicity is load-bearing.
                    |v| {
                        let mut l: Vec<&str> = v.iter().map(|(s, _)| s.as_str()).collect();
                        l.sort_unstable();
                        l.join(",")
                    },
                )
            },
        )
    });
    let cur_s = S4_VERT_CURVE.with(|c| {
        c.borrow().as_ref().map_or_else(
            || "?".to_string(),
            |m| {
                m.get(&vv).map_or_else(
                    || "-".to_string(),
                    |v| v.iter().copied().collect::<Vec<_>>().join(","),
                )
            },
        )
    });
    format!("{pre_s} inc=[{inc_s}] curve=[{cur_s}]")
}

/// PR-YR5: rebuild output `BRep` topology (`vertices`, `edges`,
/// `faces`) from the per-triangle attribution map.
///
/// Algorithm:
/// 1. Build per-triangle adjacency via canonical-edge BTreeMap.
/// 2. Flood-fill same-attribution patches. Skip None-attributed
///    triangles (cut surfaces → PR-YR6).
/// 3. For each patch, walk ALL directed boundary cycles (edges in
///    exactly one patch triangle, ordered).
/// 4. Classify cycles outer (signed area > 0) vs inner (< 0) along the
///    face normal; build `BRepFace { outer_loop, inner_loops }` (PR-YR5c).
/// 5. Inherit `surface` from `input.faces()[attribution.face]`.
/// 6. Output `vertices` is 1:1 with `mesh.verts`.
///
/// Errors:
/// - `NonManifoldOutput`: cycle walking dead-ends / T-junctions (E1),
///   a degenerate loop (E2), or not exactly one positive-area cycle
///   (E3 — disconnected / nested patch, out of scope).
/// - `MalformedTopology`: defensive; `attribution.face` out of range
///   in the input BRep.
///
/// PR-YR10: the production boolean path now goes through
/// [`reconstruct_topology_stage4`] (which runs Stage 4 then shares the same
/// [`emit_topology`]). This `&Mesh` / 3-tuple form is retained for the PR-YR5/9
/// unit-test callers (no-conic fixtures where Stage 4 would be a strict no-op),
/// hence `#[cfg(test)]`.
#[cfg(test)]
pub(crate) fn reconstruct_topology(
    mesh: &Mesh,
    attribution: &TriangleAttributionMap,
    a: &BRep,
    b: &BRep,
) -> Result<LegacyTopology, YangError> {
    // PR-YR9 path (unchanged signature, used by the unit tests): build Phase A
    // and emit with NO Stage-4 relocation (these fixtures carry no conic edges,
    // so Stage 4 would be a strict no-op anyway). The Stage-4-aware entry point
    // is `reconstruct_topology_stage4`, called by `boolean()`.
    let (infos, _incidence, intersection_curves) = compute_phase_a(
        mesh,
        attribution,
        a,
        b,
        &crate::stage3_ssi::NO_EDGE_PROVENANCE,
    )?;
    let (vertices, edges, faces, _sources, _face_attr) =
        emit_topology(mesh, &infos, &intersection_curves, &[], BoolOp::Union)?;
    Ok((vertices, edges, faces))
}

/// PR-YR10: the Stage-4-aware reconstruction `boolean()` calls. Builds Phase A,
/// runs Stage 4 (relocate intersection points onto the exact curves + §4.5.3
/// reversed-point correction), recomputes Phase A after any §4.5.3 collapse,
/// then runs the SAME Phase-B emission as `reconstruct_topology` (via the shared
/// [`emit_topology`]). Returns the 4-tuple including the per-output-vertex
/// `TessellationSource` vector (relocated verts → `BRepEdge { edge, t }`).
/// N2-3b step 2 (Yang §4.4.1) — drive the splice loop over the mesh's
/// non-manifold seam regions.
///
/// Gated by the CALLER on `YANG_MESHUP_ENABLE`; this function is only reached
/// when the gate is set, so a gate-OFF run never enters it and is byte-identical.
///
/// Each pass detects the seam regions, resolves ONE of them to its patch pair,
/// re-triangulates both sides against their shared curve
/// ([`crate::stage4_splice::splice_seam_pair`]), and writes the result back.
/// One splice per pass, because `apply_splice` renumbers triangles — every
/// other patch's `tri_indices` goes stale the moment it lands, exactly like the
/// §4.5.3 collapse path. So each pass ends with the same recompute that path
/// does: compact, `compute_phase_a`, re-key the probe maps.
///
/// Reports every skip with its reason. A region we decline is left EXACTLY as
/// it was — the loud stop, not a partial repair.
#[allow(clippy::too_many_arguments)]
fn run_meshup_splice_passes(
    mesh: &mut Mesh,
    attribution: &mut TriangleAttributionMap,
    a: &BRep,
    b: &BRep,
    infos: &mut Vec<crate::stage4_correct::PatchInfo>,
    intersection_curves: &mut std::collections::BTreeMap<(u32, u32), Curve>,
    relocations: &mut Vec<(u32, f64)>,
) -> Result<(), YangError> {
    use crate::stage4_project::detect_nonmanifold_seams;
    use crate::stage4_splice::{apply_splice, patches_on_seam, splice_seam_pair, SplicePatch};
    use crate::stage4_update::MeshUpdateOpts;

    // A pass either applies one splice or stops; the cap is a runaway guard,
    // not an expected limit (each pass strictly removes one seam region).
    const MAX_PASSES: usize = 32;

    // `d_eps` is the Stage-1 chord budget — the primitive's own documented
    // meaning for it, not a number chosen here.
    let Some(d_eps) = crate::stage4_correct::stage4_chord_band(a, b) else {
        eprintln!("[s4-meshup] SKIPPED (no Stage-1 chord band for this input pair)");
        return Ok(());
    };
    // `merge_tol` is TAU_MODEL: this codebase's "these are the same point"
    // scale. It has to be loose enough that a vertex the arrangement MINTED on
    // the partner's boundary edge is recognized as lying on it (otherwise the
    // primitive would treat a boundary point as a free interior one), and tight
    // enough never to fuse distinct features. The primitive additionally
    // requires it below `d_eps`; if the chord band is tighter than TAU_MODEL we
    // decline rather than shrink the tolerance to fit.
    let merge_tol = cad_primitives::TAU_MODEL;
    if merge_tol >= d_eps {
        eprintln!(
            "[s4-meshup] SKIPPED (merge_tol {merge_tol:.3e} >= chord band d_eps {d_eps:.3e})"
        );
        return Ok(());
    }
    let opts = MeshUpdateOpts { merge_tol, d_eps };
    // The driver verifies seam conformality in 3D; its contract asks for a tol
    // no looser than `merge_tol` in world units.
    let conformal_tol = merge_tol;

    let mut applied_total = 0usize;
    for pass in 0..MAX_PASSES {
        // Patches parallel to `infos`: `compute_phase_a` builds `infos` from
        // exactly these three calls in this order, so index i corresponds.
        // Verified, not assumed — a length mismatch stops the pass.
        let adjacency = triangle_adjacency(mesh);
        let raw = crate::stage4_correct::merge_same_plane_patches(
            flood_fill_patches(mesh, attribution, &adjacency),
            &adjacency,
            a,
            b,
        );
        if raw.len() != infos.len() {
            eprintln!(
                "[s4-meshup] STOP pass={pass}: patch/info correspondence broken \
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
                // `PatchInfo` stores directed edge pairs; the `s` of each is
                // the same ordered vertex chain.
                cycles: i
                    .cycles
                    .iter()
                    .map(|c| c.iter().map(|&(s, _)| s).collect())
                    .collect(),
                tris: p.tri_indices.clone(),
                surface: i.inherited,
            })
            .collect();

        let regions = detect_nonmanifold_seams(&mesh.tris, &|t| {
            attribution
                .lookup(t as u32)
                .map(|x| (x.input == InputId::A, x.face))
        });
        if regions.is_empty() {
            eprintln!(
                "[s4-meshup] pass={pass}: no non-manifold seam regions remain \
                 (applied_total={applied_total})"
            );
            break;
        }

        let mut applied_this_pass = false;
        for (ri, region) in regions.iter().enumerate() {
            let edges: std::collections::BTreeSet<(u32, u32)> =
                region.edges.iter().copied().collect();
            let cand = patches_on_seam(&patches, &edges);
            if cand.len() != 2 {
                eprintln!(
                    "[s4-meshup] pass={pass} region={ri}: SKIP — {} patches carry \
                     these {} edges (need exactly 2)",
                    cand.len(),
                    edges.len(),
                );
                continue;
            }
            // §4.4.1's AUTHORITY: the seam's own exact analytic curve. All of
            // the seam's edges that carry one must agree on it — a region whose
            // edges name two different curves is not one seam, so we hand the
            // splice `None` and it keeps the mesh order.
            let mut curves = edges.iter().filter_map(|e| intersection_curves.get(e));
            let first = curves.next().copied();
            let seam_curve = match first {
                Some(c) if curves.all(|o| *o == c) => Some(c),
                Some(_) => {
                    eprintln!(
                        "[s4-meshup] pass={pass} region={ri}: seam edges name >1 curve \
                         — no curve authority, keeping mesh order"
                    );
                    None
                }
                None => None,
            };
            match splice_seam_pair(
                mesh,
                &patches[cand[0]],
                &patches[cand[1]],
                &edges,
                seam_curve.as_ref(),
                opts,
                conformal_tol,
            ) {
                Ok(out) => {
                    let seam_len = out.seam.len();
                    let new_v = out.new_verts.len();
                    let reordered = out.seam_reordered;
                    match apply_splice(mesh, attribution, &out) {
                        Ok(()) => {
                            eprintln!(
                                "[s4-meshup] pass={pass} region={ri}: APPLIED patches \
                                 {}+{} seam_len={seam_len} new_verts={new_v} \
                                 curve_reordered_seam={reordered}",
                                cand[0], cand[1],
                            );
                            applied_total += 1;
                            applied_this_pass = true;
                        }
                        Err(e) => eprintln!(
                            "[s4-meshup] pass={pass} region={ri}: WRITE-BACK REFUSED {e:?}"
                        ),
                    }
                    if applied_this_pass {
                        break;
                    }
                }
                Err(e) => eprintln!(
                    "[s4-meshup] pass={pass} region={ri}: DECLINED patches {}+{} — {e:?}",
                    cand[0], cand[1],
                ),
            }
        }
        if !applied_this_pass {
            eprintln!(
                "[s4-meshup] STOP pass={pass}: {} region(s) remain, none spliceable \
                 (applied_total={applied_total})",
                regions.len(),
            );
            break;
        }

        // A splice changed the mesh, so every Phase-A structure downstream is
        // stale. Same recompute the §4.5.3 collapse and Fig-11 merge paths do.
        let remap = compact_unreferenced_verts(mesh, relocations);
        let (i2, inc2, cv2) = compute_phase_a(
            mesh,
            attribution,
            a,
            b,
            &crate::stage3_ssi::NO_EDGE_PROVENANCE,
        )?;
        probe_remap_pre_pos("meshup", remap.as_ref());
        probe_record_incidence(&inc2, &cv2);
        *infos = i2;
        *intersection_curves = cv2;
    }
    Ok(())
}

/// §4.4.1 AS WRITTEN, increment I1 (spec `specs/yang_441_trim_cdt_construction.md`
/// §4): the UNCONDITIONAL curve-seam construction for planar patch pairs on
/// `Curve::LineSegment` seams.
///
/// Gated by the CALLER on `YANG_441_CONSTRUCT`; gate-OFF runs never enter and
/// are byte-identical. Each pass enumerates every intersection-curve seam
/// (`stage4_construct::seam_groups` — no defect detector; the paper applies
/// §4.4.1 to every intersected patch), collapses ONE seam's relocated
/// fold-back chain to its junction endpoints
/// (`stage4_construct::replace_seam_run`), re-triangulates the pair against
/// the clean seam (`splice_seam_pair` — dropped chain vertices become planar
/// interior vertices and are discarded, the paper's collinear "remove a mesh
/// vertex" case), and writes back. One application per pass (`apply_splice`
/// renumbers triangles), then the same Phase-A recompute the other passes do.
///
/// Out-of-scope seams (curved patch, non-line curve, closed run) are LOUD
/// skips — increment I2's worklist, never a silent partial repair.
#[allow(clippy::too_many_arguments)]
fn run_construct_passes(
    mesh: &mut Mesh,
    attribution: &mut TriangleAttributionMap,
    a: &BRep,
    b: &BRep,
    infos: &mut Vec<crate::stage4_correct::PatchInfo>,
    intersection_curves: &mut std::collections::BTreeMap<(u32, u32), Curve>,
    relocations: &mut Vec<(u32, f64)>,
) -> Result<(), YangError> {
    use crate::stage4_construct::{replace_seam_run, seam_groups};
    use crate::stage4_splice::{
        apply_splice, ordered_seam_side, splice_seam_pair, Side, SplicePatch,
    };
    use crate::stage4_update::MeshUpdateOpts;

    // Every seam collapses at most once (its chain drops below 3 vertices),
    // so the pass count is bounded by the seam count; the cap is a runaway
    // guard, not an expected limit.
    const MAX_PASSES: usize = 64;

    let Some(d_eps) = crate::stage4_correct::stage4_chord_band(a, b) else {
        eprintln!("[s4-construct] SKIPPED (no Stage-1 chord band for this input pair)");
        return Ok(());
    };
    let merge_tol = cad_primitives::TAU_MODEL;
    if merge_tol >= d_eps {
        eprintln!(
            "[s4-construct] SKIPPED (merge_tol {merge_tol:.3e} >= chord band d_eps {d_eps:.3e})"
        );
        return Ok(());
    }
    let opts = MeshUpdateOpts { merge_tol, d_eps };
    let conformal_tol = merge_tol;

    let mut applied_total = 0usize;
    for pass in 0..MAX_PASSES {
        let adjacency = triangle_adjacency(mesh);
        let raw = crate::stage4_correct::merge_same_plane_patches(
            flood_fill_patches(mesh, attribution, &adjacency),
            &adjacency,
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
        let mut applied_this_pass = false;
        // Skip census: (non-line, curved-patch, closed, minimal, unorderable,
        // run-not-contiguous, declined/refused). The I2 worklist is measured,
        // not inferred.
        let mut skip = [0usize; 7];
        for (gi, g) in groups.iter().enumerate() {
            let (pi, qi) = g.pair;
            // ---- I1 scope filters, each loud. ---------------------------
            if !matches!(g.curve, Curve::LineSegment) {
                skip[0] += 1;
                eprintln!(
                    "[s4-construct] pass={pass} seam={gi}: SKIP non-line curve \
                     (patches {pi}+{qi}, {} edges) — I2 scope",
                    g.edges.len()
                );
                continue;
            }
            let planar = |s: &Surface| matches!(s, Surface::Plane { .. });
            if !planar(&patches[pi].surface) || !planar(&patches[qi].surface) {
                skip[1] += 1;
                eprintln!(
                    "[s4-construct] pass={pass} seam={gi}: SKIP curved patch \
                     (patches {pi}+{qi}) — I2 scope"
                );
                continue;
            }
            let (chain, closed) = match ordered_seam_side(&patches[pi].cycles, &g.edges, Side::A) {
                Ok(x) => x,
                Err(e) => {
                    skip[4] += 1;
                    eprintln!(
                        "[s4-construct] pass={pass} seam={gi}: SKIP unorderable chain \
                         (patches {pi}+{qi}) — {e:?}"
                    );
                    continue;
                }
            };
            if closed {
                skip[2] += 1;
                eprintln!(
                    "[s4-construct] pass={pass} seam={gi}: SKIP closed seam \
                     (patches {pi}+{qi}) — I2 scope"
                );
                continue;
            }
            if chain.len() < 3 {
                skip[3] += 1;
                continue; // already minimal — the construction's fixed point
            }
            let Some(cyc_a) = replace_seam_run(&patches[pi].cycles, &chain) else {
                skip[5] += 1;
                eprintln!(
                    "[s4-construct] pass={pass} seam={gi}: SKIP — run not contiguous \
                     in patch {pi}'s cycles"
                );
                continue;
            };
            let Some(cyc_b) = replace_seam_run(&patches[qi].cycles, &chain) else {
                skip[5] += 1;
                eprintln!(
                    "[s4-construct] pass={pass} seam={gi}: SKIP — run not contiguous \
                     in patch {qi}'s cycles"
                );
                continue;
            };
            let mod_a = SplicePatch {
                cycles: cyc_a,
                tris: patches[pi].tris.clone(),
                surface: patches[pi].surface,
            };
            let mod_b = SplicePatch {
                cycles: cyc_b,
                tris: patches[qi].tris.clone(),
                surface: patches[qi].surface,
            };
            let (e0, e1) = (chain[0], *chain.last().expect("chain len >= 3"));
            let seam_edges: std::collections::BTreeSet<(u32, u32)> =
                [(e0.min(e1), e0.max(e1))].into();
            match splice_seam_pair(
                mesh,
                &mod_a,
                &mod_b,
                &seam_edges,
                Some(&g.curve),
                opts,
                conformal_tol,
            ) {
                Ok(out) => match apply_splice(mesh, attribution, &out) {
                    Ok(()) => {
                        eprintln!(
                            "[s4-construct] pass={pass} seam={gi}: APPLIED patches \
                             {pi}+{qi} — chain {} -> 2 verts",
                            chain.len()
                        );
                        applied_total += 1;
                        applied_this_pass = true;
                    }
                    Err(e) => {
                        skip[6] += 1;
                        eprintln!("[s4-construct] pass={pass} seam={gi}: WRITE-BACK REFUSED {e:?}")
                    }
                },
                Err(e) => {
                    skip[6] += 1;
                    eprintln!(
                        "[s4-construct] pass={pass} seam={gi}: DECLINED patches {pi}+{qi} — {e:?}"
                    )
                }
            }
            if applied_this_pass {
                break;
            }
        }
        if !applied_this_pass {
            eprintln!(
                "[s4-construct] STOP pass={pass}: no collapsible seam remains \
                 (applied_total={applied_total}; seams={} skips: nonline={} curved={} \
                 closed={} minimal={} unorderable={} noncontig={} declined={})",
                groups.len(),
                skip[0],
                skip[1],
                skip[2],
                skip[3],
                skip[4],
                skip[5],
                skip[6],
            );
            break;
        }

        let remap = compact_unreferenced_verts(mesh, relocations);
        let (i2, inc2, cv2) = compute_phase_a(
            mesh,
            attribution,
            a,
            b,
            &crate::stage3_ssi::NO_EDGE_PROVENANCE,
        )?;
        probe_remap_pre_pos("construct", remap.as_ref());
        probe_record_incidence(&inc2, &cv2);
        *infos = i2;
        *intersection_curves = cv2;
    }
    Ok(())
}

pub(crate) fn reconstruct_topology_stage4(
    mesh: &mut Mesh,
    attribution: &mut TriangleAttributionMap,
    a: &BRep,
    b: &BRep,
    op: BoolOp,
    minted_junction_keys: &std::collections::BTreeMap<[u64; 3], crate::boolean::MintProvenance>,
    edge_provenance: &crate::stage3_ssi::PosKeyedEdgeSet,
) -> Result<ReconstructedTopology, YangError> {
    // (4) Phase A: per-patch ordered loops + inherited surface (`infos`), and the
    // exact per-edge intersection `Curve` map.
    let (mut infos, incidence, mut intersection_curves) =
        compute_phase_a(mesh, attribution, a, b, edge_provenance)?;

    // KV9-F1 diagnosis probe (read-only, env-gated): kept-set attribution
    // census + per-patch summary at Stage-6 entry.
    if std::env::var_os("YANG_S6_PATCH_PROBE").is_some() {
        let (mut na, mut nb, mut none) = (0usize, 0usize, 0usize);
        for att in &attribution.attributions {
            match att {
                Some(TriangleAttribution {
                    input: InputId::A, ..
                }) => na += 1,
                Some(TriangleAttribution {
                    input: InputId::B, ..
                }) => nb += 1,
                None => none += 1,
            }
        }
        eprintln!(
            "[s6-patch-probe] kept tris: A={na} B={nb} none={none} (mesh tris {})",
            mesh.tris.len()
        );
        for (i, info) in infos.iter().enumerate() {
            eprintln!(
                "[s6-patch-probe] patch {i}: input {:?} face {} cycles {:?} fold_sliver {}",
                info.input,
                info.face_idx,
                info.cycles.iter().map(|c| c.len()).collect::<Vec<_>>(),
                info.had_fold_sliver
            );
            if std::env::var_os("YANG_S6_CYCLE_DUMP").is_some() {
                for (ci, cycle) in info.cycles.iter().enumerate() {
                    eprintln!(
                        "[s6-cycle-dump] patch {i} cycle {ci}: {:?}",
                        cycle.iter().map(|&(s, _)| s).collect::<Vec<_>>()
                    );
                    let dump_sel = std::env::var("YANG_S6_CYCLE_DUMP");
                    if dump_sel.as_deref() == Ok("all") || dump_sel.as_deref() == Ok(&i.to_string())
                    {
                        for &(s, _) in cycle {
                            eprintln!(
                                "[s6-cycle-pos] patch {i} cycle {ci} v{s} {:?}",
                                mesh.verts.get(s as usize)
                            );
                        }
                    }
                }
            }
        }
    }

    // (4a) Stage 4 (seam A1): relocate onto the exact analytical curves
    // (Yang §4.4.1) + §4.5.3 reversal correction. Entered on ANY analytic conic
    // (Circle OR Ellipse) so an ellipse-only fixture reaches the loud
    // `EllipseProjectionUnsupported` STOP rather than silently passing an
    // un-relocated mesh. No conic edges ⇒ Stage 4 is a strict no-op (planar
    // byte-identity).
    // PR-YR22: include `Parabola` so a parabola-only fixture enters Stage 4 and
    // its cone-parabola seam is relocated onto the exact section.
    // PR-YR23: include `Hyperbola` likewise so a hyperbola edge enters Stage 4.
    // PR-F3: ALSO enter Stage 4 when a `LineSegment` intersection edge has a
    // CURVED surface in its incidence — that is a plane∥axis × cylinder ruling
    // line (ssi C3a/C3b) whose arrangement points sit on Stage-1 facet chords
    // and need relocation onto the exact line. A plane∩plane segment is exact
    // and does NOT trigger Stage 4 (planar byte-identity preserved).
    let has_conic = intersection_curves.iter().any(|(key, c)| {
        matches!(
            c,
            Curve::Circle { .. }
                | Curve::Ellipse { .. }
                | Curve::Parabola { .. }
                | Curve::Hyperbola { .. }
                // M5: a surface-pair edge's endpoints sit on Stage-1 chords off
                // the exact degree-4 curve and MUST be relocated in Stage 4 —
                // register it so Stage 4 runs for pure surface-pair results.
                | Curve::SurfacePair { .. }
        ) || (matches!(c, Curve::LineSegment)
            && incidence.get(key).is_some_and(|entries| {
                entries
                    .iter()
                    .any(|&(_, s)| !matches!(s, Surface::Plane { .. }))
            }))
    });
    // KV6d Tier B: a TORUS intersection edge is degree-4 (never conic), so it
    // does not register above — but its endpoints sit on Stage-1 chords off the
    // analytic torus and need the implicit-pair Newton relocation in Stage 4.
    let has_torus = incidence
        .values()
        .any(|es| es.iter().any(|(_, s)| matches!(s, Surface::Torus { .. })));
    let has_conic = has_conic || has_torus;
    // (vertex, circle-frame angle t) for every relocated / retagged intersection
    // vertex. Mapped to `BRepEdge { edge, t }` sources in `emit_topology` once
    // the output edges exist.
    let mut relocations: Vec<(u32, f64)> = Vec::new();
    if std::env::var_os("YANG_S5_FOLD_PROBE").is_some() {
        let mut kinds: std::collections::BTreeMap<&'static str, usize> = Default::default();
        for c in intersection_curves.values() {
            *kinds
                .entry(match c {
                    Curve::LineSegment => "LineSegment",
                    Curve::Circle { .. } => "Circle",
                    Curve::Ellipse { .. } => "Ellipse",
                    Curve::Parabola { .. } => "Parabola",
                    Curve::Hyperbola { .. } => "Hyperbola",
                    Curve::SurfacePair { .. } => "SurfacePair",
                })
                .or_default() += 1;
        }
        let mut surfs: std::collections::BTreeMap<&'static str, usize> = Default::default();
        for es in incidence.values() {
            for (_, s) in es {
                *surfs
                    .entry(match s {
                        Surface::Plane { .. } => "Plane",
                        Surface::Cylinder { .. } => "Cylinder",
                        Surface::Cone { .. } => "Cone",
                        Surface::Sphere { .. } => "Sphere",
                        Surface::Torus { .. } => "Torus",
                    })
                    .or_default() += 1;
            }
        }
        eprintln!(
            "YANG_S5_STAGE4_GATE has_conic={has_conic} has_torus={has_torus} \
             n_intersection_curves={} curve_kinds={kinds:?} incidence_keys={} \
             incidence_surfaces={surfs:?}",
            intersection_curves.len(),
            incidence.len(),
        );
        probe_record_incidence(&incidence, &intersection_curves);
    }
    // Positional snapshot for the S4_MOVED oracle (env-gated; the clone is not
    // taken on the production path).
    let s4_probe = s4_pre_pos_enabled();
    let verts_pre: Option<Vec<Point3>> = if s4_probe && has_conic {
        Some(mesh.verts.clone())
    } else {
        None
    };
    // Diagnostic only (`YANG_S4_COINCIDENT_PROBE`, read-only): the duplicate-vertex
    // census at STAGE-4 ENTRY — the point at which every Stage-4 arm still sees the
    // mesh Stage 0/2/3 handed it.
    //
    // Two columns, because they answer different questions. The bit-exact
    // `coincident_sites` count is the cheap corpus-wide screen. The optional
    // `=x,y,z` target then lists EVERY vertex within 1e-9 of one point and prints
    // their pairwise separations at full precision — which is the measurement that
    // matters, because the defect class this was built for is NOT bit-exact:
    // F0067's triple-point twins sit 1.35e-15 apart, three orders BELOW `TAU_WORK`,
    // so they are invisible to the bit-exact count and to every identification the
    // pipeline runs, yet Stage 4 relocates them by two different rules and blows
    // them 4.1e-5 apart. A femto pair is a duplicate; only the separation says so.
    //
    // Placed BEFORE `stage4_relocate_and_correct` on purpose: comparing this census
    // across an upstream switch is what separates a defect Stage 0 MINTED from one
    // it merely un-MASKED (F0067: identical under `YANG_A19_OFF` ⇒ pre-existing).
    if let Ok(spec) = std::env::var("YANG_S4_COINCIDENT_PROBE") {
        let mut by_pos: std::collections::HashMap<[u64; 3], Vec<u32>> = Default::default();
        for (i, p) in mesh.verts.iter().enumerate() {
            let a = p.as_array();
            by_pos
                .entry([a[0].to_bits(), a[1].to_bits(), a[2].to_bits()])
                .or_default()
                .push(i as u32);
        }
        let dup: usize = by_pos.values().filter(|v| v.len() > 1).count();
        let dup_verts: usize = by_pos.values().filter(|v| v.len() > 1).map(Vec::len).sum();
        eprintln!(
            "YANG_S4_COINCIDENT n_verts={} coincident_sites={dup} coincident_verts={dup_verts}",
            mesh.verts.len()
        );
        let t: Vec<f64> = spec
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();
        if let [tx, ty, tz] = t[..] {
            let mut near: Vec<u32> = Vec::new();
            for (i, p) in mesh.verts.iter().enumerate() {
                let a = p.as_array();
                let dd = ((a[0] - tx).powi(2) + (a[1] - ty).powi(2) + (a[2] - tz).powi(2)).sqrt();
                if dd < 1.0e-9 {
                    near.push(i as u32);
                    eprintln!(
                        "YANG_S4_COINCIDENT near v={i} d={dd:.3e} p=({:.17e},{:.17e},{:.17e})",
                        a[0], a[1], a[2]
                    );
                }
            }
            for w in near.windows(2) {
                let (p, q) = (
                    mesh.verts[w[0] as usize].as_array(),
                    mesh.verts[w[1] as usize].as_array(),
                );
                let s =
                    ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt();
                eprintln!(
                    "YANG_S4_COINCIDENT sep v{}-v{} = {s:.6e}  (TAU_WORK={:.1e})",
                    w[0],
                    w[1],
                    cad_primitives::TAU_WORK
                );
            }
        }
    }
    if has_conic {
        let (relocs, collapsed) = stage4_relocate_and_correct(
            mesh,
            attribution,
            a,
            b,
            minted_junction_keys,
            edge_provenance,
        )?;
        relocations = relocs;
        // A §4.5.3 collapse mutated the mesh topology + attribution, so the
        // pre-collapse Phase-A loops are stale (spec §4.1 note). Recompute them
        // before the Phase-B emission re-validates the corrected mesh.
        // Snapshot the pre-Stage-4 positions BEFORE the first compaction can
        // renumber. `collapse_vertex` itself never touches `mesh.verts` (it
        // rewrites triangle indices and drops degenerate tris), so indices are
        // still 1:1 with `verts_pre` at this point even when `collapsed`.
        if s4_probe {
            if let Some(pre) = &verts_pre {
                let map: std::collections::HashMap<u32, [f64; 3]> = pre
                    .iter()
                    .enumerate()
                    .map(|(i, p)| (i as u32, p.as_array()))
                    .collect();
                let n_moved = pre
                    .iter()
                    .zip(mesh.verts.iter())
                    .filter(|(p, q)| p.as_array() != q.as_array())
                    .count();
                eprintln!(
                    "YANG_S5_MOVED_SET n_moved={n_moved} n_verts={} collapsed={collapsed} \
                     (pre-Stage-4 positions, re-keyed through every compaction)",
                    pre.len(),
                );
                S4_PRE_POS.with(|c| *c.borrow_mut() = Some(map));
            } else {
                eprintln!("YANG_S5_MOVED_SET UNAVAILABLE (Stage 4 not entered)");
            }
        }
        if collapsed {
            // PR-YR11: drop the vertices the collapse left unreferenced (and
            // remap triangle indices + `relocations`) BEFORE recomputing Phase A,
            // so the emitted output mesh carries no dangling vertices (a global
            // V−E+F = 2 for a single closed shell). Strict no-op when there were
            // no danglers.
            let remap = compact_unreferenced_verts(mesh, &mut relocations);
            let (i2, inc2, cv2) = compute_phase_a(
                mesh,
                attribution,
                a,
                b,
                &crate::stage3_ssi::NO_EDGE_PROVENANCE,
            )?;
            // The collapse renumbered vertices: re-key the probe maps or their
            // columns name the wrong vertices (no-op when the gate is off).
            probe_remap_pre_pos("s453", remap.as_ref());
            probe_record_incidence(&inc2, &cv2);
            infos = i2;
            intersection_curves = cv2;
        }

        // N2-3b step 1 (Yang §4.4.1) — the fold-risk PLAN, reported, not
        // applied. `YANG_S4_FOLD_RISK` is read-only: it neither mutates the
        // mesh nor changes control flow, so a gate-ON run is byte-identical to
        // gate-OFF on every output.
        //
        // Placed HERE, after the §4.5.3 collapse and its `compute_phase_a`
        // recompute, because both inputs must describe the SAME mesh:
        // `intersection_curves` is the post-recompute `cv2` and `S4_PRE_POS`
        // has been re-keyed through the compaction. Reading either from before
        // this point would pair a pre-collapse chain with post-collapse
        // indices.
        //
        // Why report before wiring the Fig-11 merge arm onto this plan: the
        // criterion was validated on R0074, whose folds lie ON the intersection
        // chain, so its neighbours are chain vertices. F0067's crossing is
        // between a relocated curve vertex and the OTHER operand's profile
        // corners, which are not chain vertices at all — so its neighbourhood
        // here may be much wider than the loop segment the 08-03 census
        // measured, and the ratio correspondingly smaller. Whether the merge
        // arm even fires for F0067 is a measurement, not an assumption, and
        // applying a merge that the plan mis-scopes would fuse a notch corner
        // that is rightly immovable.
        // Filled by the gated Fig-11 arm below; applied after the probe's
        // `S4_PRE_POS` borrow is released.
        let mut fig11_merge_plan: Vec<(u32, u32)> = Vec::new();
        if std::env::var_os("YANG_S4_FOLD_RISK").is_some() {
            let curve_edges: std::collections::BTreeSet<(u32, u32)> =
                intersection_curves.keys().copied().collect();
            // Widened 2026-08-05: the BOUNDARY CYCLE is the structure the
            // 07-29 census walked; curve keys alone are a strict subset and
            // scored 0 on R0074. `infos` here is post-recompute (`i2`) when a
            // §4.5.3 collapse ran, so the cycles name current indices.
            let cyc_verts: Vec<Vec<u32>> = infos
                .iter()
                .flat_map(|i| i.cycles.iter())
                .map(|c| c.iter().map(|&(v, _)| v).collect())
                .collect();
            let chain = crate::stage4_fold_risk::cycle_adjacency(
                cyc_verts.iter().map(Vec::as_slice),
                &curve_edges,
            );
            let post: Vec<[f64; 3]> = mesh.verts.iter().map(Point3::as_array).collect();
            S4_PRE_POS.with(|c| {
                let borrow = c.borrow();
                let Some(pre) = borrow.as_ref() else {
                    eprintln!(
                        "[s4-fold-risk] UNAVAILABLE (no pre-position map — Stage 4 not entered)"
                    );
                    return;
                };
                let risks = crate::stage4_fold_risk::rank_fold_risks(pre, &post, &chain);
                let minting = crate::stage4_fold_risk::minting_risks(&risks);
                let folds = crate::stage4_fold_risk::classify_folds(
                    cyc_verts.iter().map(Vec::as_slice),
                    pre,
                    &post,
                );
                let n_minted = folds
                    .iter()
                    .filter(|f| f.class == crate::stage4_fold_risk::FoldClass::Minted)
                    .count();
                let n_inherited = folds
                    .iter()
                    .filter(|f| f.class == crate::stage4_fold_risk::FoldClass::Inherited)
                    .count();
                let customers = crate::stage4_fold_risk::merge_customers(&risks, &folds);
                eprintln!(
                    "[s4-fold-risk] FOLDS measured={} minted={n_minted} \
                     inherited={n_inherited} | MERGE_CUSTOMERS={} \
                     (= ratio>=1 AND Stage-4-minted fold)",
                    folds.len(),
                    customers.len(),
                );
                // The 07-29 census's MINTED signature is `turn_pre 0.00 ->
                // 179.9x`. Bucketing turn_pre tells a population matching that
                // signature from one that merely crossed the threshold from an
                // already-bent corner — the discriminator for the 38-vs-16 gap.
                let cust: std::collections::BTreeSet<u32> =
                    customers.iter().map(|r| r.vertex).collect();
                // Second, THRESHOLD-FREE signal (the F0067 anchor's own
                // certificate). Reported beside the turn test so their
                // agreement is a measurement, not an assumption.
                let inv = crate::stage4_fold_risk::chord_order_inversions(
                    cyc_verts.iter().map(Vec::as_slice),
                    pre,
                    &post,
                );
                let minted_set: std::collections::BTreeSet<u32> = folds
                    .iter()
                    .filter(|f| f.class == crate::stage4_fold_risk::FoldClass::Minted)
                    .map(|f| f.vertex)
                    .collect();
                eprintln!(
                    "[s4-fold-risk] CHORD inversions={} | agree_with_minted_turn={} \
                     turn_only={} chord_only={}",
                    inv.len(),
                    inv.intersection(&minted_set).count(),
                    minted_set.difference(&inv).count(),
                    inv.difference(&minted_set).count(),
                );
                let mut buckets = [0usize; 4];
                for f in folds
                    .iter()
                    .filter(|f| f.class == crate::stage4_fold_risk::FoldClass::Minted)
                {
                    let b = match f.turn_pre_deg {
                        t if t < 1.0 => 0,
                        t if t < 30.0 => 1,
                        t if t < 90.0 => 2,
                        _ => 3,
                    };
                    buckets[b] += 1;
                }
                eprintln!(
                    "[s4-fold-risk] MINTED turn_pre buckets: <1deg={} 1-30={} \
                     30-90={} 90-120={} (census signature is turn_pre~0)",
                    buckets[0], buckets[1], buckets[2], buckets[3],
                );
                for f in folds
                    .iter()
                    .filter(|f| f.class == crate::stage4_fold_risk::FoldClass::Minted)
                    .take(40)
                {
                    eprintln!(
                        "[s4-fold-risk]   MINTED v={} pre={:.3} post={:.3} prev={} next={}{}",
                        f.vertex,
                        f.turn_pre_deg,
                        f.turn_post_deg,
                        f.prev,
                        f.next,
                        if cust.contains(&f.vertex) {
                            "  CUSTOMER"
                        } else {
                            ""
                        },
                    );
                }
                eprintln!(
                    "[s4-fold-risk] SUMMARY scored={} minting={} adj_edges={} \
                     (curve_only={}) n_verts={} (ratio = displacement / \
                     pre-relocation BOUNDARY-CYCLE spacing; >=1 is the Fig-11 \
                     merge class)",
                    risks.len(),
                    minting.len(),
                    chain.len(),
                    curve_edges.len(),
                    post.len(),
                );
                // ---- Fig-11 merge TRIAL (N2-3b step 2), GATED OFF --------
                // `YANG_S4_FIG11_MERGE` fuses each chosen customer with the
                // neighbour it overran.
                //
                // MEASURED NEGATIVE on F0067 (33/33 applied): the wall moves
                // from "ring rejected by CDT" to "reassembled output would be
                // non-2-manifold". **A bare `collapse_vertex` is NOT Yang's
                // Fig-11 merge** — the paper's merge happens inside the §4.4.1
                // parametric re-triangulation (`stage4_update`), which rebuilds
                // the affected patch; collapsing a real-length (3.7e-3) edge
                // with an index rewrite leaves the surrounding fan
                // inconsistent. Kept gated OFF as scaffolding for the correct
                // wiring and as the record of what the shortcut does.
                //
                // The RELOCATED vertex survives, never the neighbour: it is
                // the one carrying Stage 4's exact curve position, so keeping
                // it preserves the analytic certificate by construction.
                // (`collapse_vertex` rewrites triangle indices only — it never
                // touches `mesh.verts` — so the survivor's position is exactly
                // the relocated one.)
                //
                // Applied worst-ratio-first, skipping any vertex already
                // consumed by an earlier merge, so the result does not depend
                // on iteration order.
                let plan = crate::stage4_fold_risk::merge_customers_chord(&risks, &inv);
                eprintln!(
                    "[s4-fold-risk] CHORD_CUSTOMERS={} (ratio>=1 AND chord inversion)",
                    plan.len(),
                );
                if std::env::var_os("YANG_S4_FIG11_MERGE").is_some() {
                    fig11_merge_plan = plan
                        .iter()
                        .map(|r| (r.vertex, r.nearest_neighbour))
                        .collect();
                }
                for r in risks.iter().take(12) {
                    eprintln!(
                        "[s4-fold-risk]   v={} ratio={:.4} disp={:.4e} pre_spacing={:.4e} \
                         nbr={}{}",
                        r.vertex,
                        r.ratio,
                        r.displacement,
                        r.min_pre_spacing,
                        r.nearest_neighbour,
                        if r.ratio >= 1.0 { "  MINTING" } else { "" },
                    );
                }
            });
        }

        // Apply the Fig-11 merges (empty unless `YANG_S4_FIG11_MERGE` is set,
        // so this is a strict no-op on the production path). Mirrors the
        // §4.5.3 collapse path above: collapse, compact, recompute Phase A,
        // re-key the probe maps — a collapse renumbers, and every consumer
        // downstream reads the recomputed structures.
        if !fig11_merge_plan.is_empty() {
            let mut consumed: std::collections::BTreeSet<u32> = Default::default();
            let mut applied = 0usize;
            for (keep, drop) in &fig11_merge_plan {
                // Skip anything an earlier merge already folded away, so the
                // plan stays order-independent rather than cascading.
                if consumed.contains(keep) || consumed.contains(drop) || keep == drop {
                    continue;
                }
                collapse_vertex(mesh, &mut attribution.attributions, *drop, *keep);
                consumed.insert(*drop);
                applied += 1;
            }
            eprintln!(
                "[s4-fig11] MERGES applied={applied} planned={} skipped={}",
                fig11_merge_plan.len(),
                fig11_merge_plan.len() - applied,
            );
            let remap = compact_unreferenced_verts(mesh, &mut relocations);
            let (i3, inc3, cv3) = compute_phase_a(
                mesh,
                attribution,
                a,
                b,
                &crate::stage3_ssi::NO_EDGE_PROVENANCE,
            )?;
            probe_remap_pre_pos("fig11", remap.as_ref());
            probe_record_incidence(&inc3, &cv3);
            infos = i3;
            intersection_curves = cv3;
        }

        // N2-3b step 2 (Yang §4.4.1 "Mesh updating") — the SPLICE LOOP, wired.
        //
        // Placed here, at the end of Stage 4, for the same reason the fold-risk
        // planner is: `infos` and `intersection_curves` are the post-§4.5.3,
        // post-Fig-11 recomputes, so they and `mesh` describe the SAME mesh.
        //
        // The whole block is inside the env gate, so a gate-OFF run neither
        // reads nor writes anything here and is byte-identical by construction
        // — the same shape as every prior increment in this epic.
        //
        // The SELECTOR is `detect_nonmanifold_seams`: the splice repairs a seam
        // whose two sides subdivide it differently, and an imbalanced directed
        // edge is exactly that defect's signature. The `stage4_fold_risk` plan
        // remains the other candidate driver; which one converts cases is a
        // measurement, not a choice to bake in here.
        if std::env::var_os("YANG_MESHUP_ENABLE").is_some() {
            run_meshup_splice_passes(
                mesh,
                attribution,
                a,
                b,
                &mut infos,
                &mut intersection_curves,
                &mut relocations,
            )?;
        }
        // §4.4.1 AS WRITTEN, increment I1 (spec
        // `specs/yang_441_trim_cdt_construction.md`): the unconditional
        // curve-seam construction. Gate-OFF is byte-identical.
        if std::env::var_os("YANG_441_CONSTRUCT").is_some() {
            run_construct_passes(
                mesh,
                attribution,
                a,
                b,
                &mut infos,
                &mut intersection_curves,
                &mut relocations,
            )?;
        }
    }

    // EXPERIMENTAL probe (task #121 increment 1, read-only, env-gated):
    // post-Stage-4 duplicate-triangle scan. The I6 guard proves the kept
    // submesh entered Stage 3/4 with no duplicate sorted vertex triple, so
    // any duplicate found HERE was minted by Stage-4 collapse machinery —
    // localizes the F0059 double-cover origin. Also reports POSITION-level
    // coincidence (distinct indices, bit-identical coordinates).
    if std::env::var_os("YANG_DOUBLECOVER_PROBE").is_some() {
        use std::collections::HashMap;
        let mut by_triple: HashMap<[u32; 3], Vec<usize>> = HashMap::new();
        let mut by_pos: HashMap<[[u64; 3]; 3], Vec<usize>> = HashMap::new();
        for (t, tri) in mesh.tris.iter().enumerate() {
            let mut s = *tri;
            s.sort_unstable();
            by_triple.entry(s).or_default().push(t);
            let mut ps: [[u64; 3]; 3] = [[0; 3]; 3];
            for (k, &v) in s.iter().enumerate() {
                let p = mesh.verts[v as usize];
                ps[k] = [p.x().to_bits(), p.y().to_bits(), p.z().to_bits()];
            }
            ps.sort_unstable();
            by_pos.entry(ps).or_default().push(t);
        }
        for (key, ts) in &by_triple {
            if ts.len() > 1 {
                eprintln!("[doublecover] INDEX dup triple {key:?} tris {ts:?}");
                for &t in ts {
                    eprintln!(
                        "[doublecover]   tri {t} = {:?} coords {:?}",
                        mesh.tris[t],
                        mesh.tris[t]
                            .iter()
                            .map(|&v| mesh.verts[v as usize])
                            .collect::<Vec<_>>()
                    );
                }
            }
        }
        for ts in by_pos.values() {
            if ts.len() > 1 {
                let mut idx: Vec<[u32; 3]> = ts
                    .iter()
                    .map(|&t| {
                        let mut s = mesh.tris[t];
                        s.sort_unstable();
                        s
                    })
                    .collect();
                idx.dedup();
                if idx.len() > 1 {
                    eprintln!("[doublecover] POSITION dup (distinct indices) tris {ts:?}");
                }
            }
        }
    }

    // KV15b (spec `kv15b_mint_site_subresolution_collapse`): emission
    // hygiene — collapse sub-`TAU_MODEL` intersection segments so the
    // emitted B-Rep never carries a sub-resolution twin pair (I5). Runs on
    // EVERY path (the R0076 minting subtract is all-planar, so the Stage-4
    // §4.4.1(b) merge above never sees it). Byte-identical no-op when no
    // such segment exists (B6).
    {
        // #169 N56: the KV15b sub-resolution collapse is a COMPLIANT always-on
        // Yang §4.3 operation ("remove a point too close to another on the same
        // loop") — it collapses an intersection-curve segment below the
        // scale-relative model-coincidence resolution (both endpoints on the
        // curve ⇒ faithful redundant-sample removal). Retightened from the
        // absolute floor and un-gated (was `weld_enabled("subres")`); recovers
        // R0076/R0088/F0078/F0079/F0084, 0 WRONG. Byte-identical no-op when no
        // such segment exists (B6).
        let kv15b_collapsed = {
            let mut attr_vec = std::mem::take(&mut attribution.attributions);
            let c = collapse_subresolution_intersection_segments(
                mesh,
                &mut attr_vec,
                &intersection_curves,
                a,
                b,
            );
            attribution.attributions = attr_vec;
            c
        };
        if kv15b_collapsed {
            let remap = compact_unreferenced_verts(mesh, &mut relocations);
            let (i3, inc3, cv3) = compute_phase_a(
                mesh,
                attribution,
                a,
                b,
                &crate::stage3_ssi::NO_EDGE_PROVENANCE,
            )?;
            // Same re-keying as the §4.5.3 site above. NOTE this site runs even
            // when Stage 4 did not collapse, so omitting it silently misaligns
            // the probe columns on cases that looked unaffected.
            probe_remap_pre_pos("kv15b", remap.as_ref());
            probe_record_incidence(&inc3, &cv3);
            infos = i3;
            intersection_curves = cv3;
        }
        // #194 (spec `yang_194_subtauwork_edge_collapse`): collapse mesh
        // edges below WORKING precision — the operand-self-graze twin class
        // (F0082 Extrude-12: same junction minted twice with swapped LPI
        // roles, 5.5e-14 apart, edge-connected, zero-area flap → χ=3 book
        // edge). Runs AFTER KV15b at a five-orders-tighter band with no
        // provenance restriction (the band does the scoping); KV9's
        // unconnected ring duplicates carry no joining edge and cannot be
        // touched. Byte-identical no-op when no such edge exists.
        let s194_collapsed = {
            let mut attr_vec = std::mem::take(&mut attribution.attributions);
            let c = collapse_subtauwork_mesh_edges(mesh, &mut attr_vec);
            attribution.attributions = attr_vec;
            c
        };
        if s194_collapsed {
            let remap = compact_unreferenced_verts(mesh, &mut relocations);
            let (i4, inc4, cv4) = compute_phase_a(
                mesh,
                attribution,
                a,
                b,
                &crate::stage3_ssi::NO_EDGE_PROVENANCE,
            )?;
            probe_remap_pre_pos("s194", remap.as_ref());
            probe_record_incidence(&inc4, &cv4);
            infos = i4;
            intersection_curves = cv4;
        }
        // EXPERIMENTAL probe (task #121): duplicate-triple scan AFTER the
        // KV15b collapse — if a dup appears here but not post-Stage-4, the
        // KV15b collapse is the mint site.
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
                        "[doublecover-postkv15b] INDEX dup triple {key:?} tris {ts:?} \
                         (kv15b_collapsed={kv15b_collapsed})"
                    );
                }
            }
        }
    }

    // N50 (spec `yang_n50_f32_render_twin_weld`, deviation N50): weld two
    // distinct output vertices that are bitwise-identical after rounding to f32 —
    // the kernel-v2 G1 render-collapse criterion at the OUTPUT (world) magnitude.
    // The R0012/R0098 render-collapse twins are NON-relocated arrangement
    // vertices minted by near-coincident Stage-0 overlay sweep-event columns
    // (N48/N49); after the final Stage-4 relocation above they converge to within
    // f32 render precision and survive every earlier (model-band) merge. This
    // runs LAST, on the final mesh whose verts are 1:1 with the emitted output
    // vertices, so it measures the same magnitude G1 does. Byte-identical no-op
    // when no two live verts share an f32 render cell (the fast path).
    if weld_enabled("f32") {
        let mut attr_vec = std::mem::take(&mut attribution.attributions);
        let f32_welded = weld_f32_render_twins(mesh, &mut attr_vec);
        attribution.attributions = attr_vec;
        if f32_welded {
            let remap = compact_unreferenced_verts(mesh, &mut relocations);
            let (i5, inc5, cv5) = compute_phase_a(
                mesh,
                attribution,
                a,
                b,
                &crate::stage3_ssi::NO_EDGE_PROVENANCE,
            )?;
            probe_remap_pre_pos("f32weld", remap.as_ref());
            probe_record_incidence(&inc5, &cv5);
            infos = i5;
            intersection_curves = cv5;
        }
    }

    // (#173 / N6) §4.5.4 illegal-self-intersection PROBE on the FINAL mesh
    // (verts 1:1 with the emitted output vertices). Gated on
    // `YANG_SELFX_PROBE`, byte-identical when unset. Measurement-first per
    // `specs/yang_173_selfx_detector.md` §4 — the always-on loud STOP ships
    // only after the corpus-wide false-positive measurement passes.
    if std::env::var_os("YANG_SELFX_PROBE").is_some() {
        let t0 = std::time::Instant::now(); // wasm-ok: env-gated (env vars are unset in wasm)
        let contacts = cherchi_rs::detect_improper_contacts(&mesh.verts, &mesh.tris);
        eprintln!(
            "YANG_SELFX_CHECKED tris={} improper={} unresolved={} ms={}",
            mesh.tris.len(),
            contacts.improper_pairs.len(),
            contacts.unresolved_pairs.len(),
            t0.elapsed().as_millis()
        );
        if !contacts.is_clean() {
            eprintln!(
                "YANG_SELFX improper={} unresolved={}",
                contacts.improper_pairs.len(),
                contacts.unresolved_pairs.len()
            );
            let attr_of = |t: u32| {
                attribution
                    .attributions
                    .get(t as usize)
                    .and_then(|o| o.as_ref().map(|at| (at.input, at.face)))
            };
            for &(ta, tb) in contacts
                .improper_pairs
                .iter()
                .chain(contacts.unresolved_pairs.iter())
                .take(8)
            {
                eprintln!(
                    "  pair ({ta},{tb}) attr=({:?},{:?}) A={:?} B={:?}",
                    attr_of(ta),
                    attr_of(tb),
                    mesh.tris
                        .get(ta as usize)
                        .map(|t| t.map(|v| mesh.verts[v as usize])),
                    mesh.tris
                        .get(tb as usize)
                        .map(|t| t.map(|v| mesh.verts[v as usize])),
                );
            }
        }
    }

    emit_topology(mesh, &infos, &intersection_curves, &relocations, op)
}

/// PR-YR5/YR9 Phase-B emission (factored out in PR-YR10 so both
/// [`reconstruct_topology`] and [`reconstruct_topology_stage4`] share ONE copy):
/// walk `infos`, emit `edges`/`faces`, and build the per-vertex
/// `TessellationSource` vector (relocated verts → `BRepEdge { edge, t }`).
///
/// The Newell / flip / E2 / E3 machinery is UNCHANGED from PR-YR8/YR9 (it reads
/// `cycles` / `signed_areas`, never the per-edge curve). The per-edge `curve`
/// comes from `intersection_curves` (an intersection edge gets its exact conic;
/// all others stay `LineSegment`).
/// Task #133 (spec `yang_stage6_arc_orientation` B1–B3): orient a periodic
/// intersection curve for one DIRECTED edge copy `s → e`. The Stage-1 input
/// convention is "the arc is the CCW sweep around the stored normal from
/// start to end"; the undirected curve map carries ONE normal, so the copy
/// whose face-loop traversal is CLOCKWISE around it would declare the
/// COMPLEMENTARY (≈ 2π) arc. Every Stage-6 output edge spans a single mesh
/// chord — the geometric piece is always the minor side — so a CCW sweep
/// exceeding π means THIS copy needs the negated normal (the kernel-v2
/// twin convention: same point set, opposite traversal). A sweep within
/// 1e-6 of π is left unchanged (the kernel-v2 `ARC_MINOR_AMBIGUITY_BAND`
/// posture; mesh chords are orders of magnitude below π).
fn orient_directed_curve(curve: Curve, s: u32, e: u32, verts: &[Point3]) -> Curve {
    let sweep_ccw = |center: Point3, normal: Vector3| -> Option<f64> {
        let (e1, e2) = ortho_basis(normal);
        let (e1, e2, c) = (e1.as_array(), e2.as_array(), center.as_array());
        let ang = |vi: u32| -> Option<f64> {
            let q = verts.get(vi as usize)?.as_array();
            let w = [q[0] - c[0], q[1] - c[1], q[2] - c[2]];
            let x = w[0] * e1[0] + w[1] * e1[1] + w[2] * e1[2];
            let y = w[0] * e2[0] + w[1] * e2[1] + w[2] * e2[2];
            Some(y.atan2(x))
        };
        Some((ang(e)? - ang(s)?).rem_euclid(2.0 * std::f64::consts::PI))
    };
    let needs_flip = |sweep: Option<f64>| -> bool {
        matches!(sweep, Some(sw) if sw > std::f64::consts::PI && (sw - std::f64::consts::PI).abs() > 1e-6)
    };
    match curve {
        Curve::Circle {
            center,
            normal,
            radius,
        } if s != e => {
            if needs_flip(sweep_ccw(center, normal)) {
                let n = normal.as_array();
                Curve::Circle {
                    center,
                    normal: Vector3::new(-n[0], -n[1], -n[2]),
                    radius,
                }
            } else {
                curve
            }
        }
        Curve::Ellipse {
            center,
            normal,
            major_axis,
            major_radius,
            minor_radius,
        } if s != e => {
            // Same test in the ellipse's own angular parameterization: the
            // param angle is measured CCW around the stored normal, so the
            // frame-angle sweep sign matches the param sweep sign.
            if needs_flip(sweep_ccw(center, normal)) {
                let n = normal.as_array();
                Curve::Ellipse {
                    center,
                    normal: Vector3::new(-n[0], -n[1], -n[2]),
                    major_axis,
                    major_radius,
                    minor_radius,
                }
            } else {
                curve
            }
        }
        _ => curve,
    }
}

pub(crate) fn emit_topology(
    mesh: &Mesh,
    infos: &[PatchInfo],
    intersection_curves: &std::collections::BTreeMap<(u32, u32), Curve>,
    relocations: &[(u32, f64)],
    op: BoolOp,
) -> Result<ReconstructedTopology, YangError> {
    if std::env::var_os("YANG_S5_FOLD_PROBE").is_some() {
        eprintln!(
            "YANG_S5_RELOC_SET n_relocations={} n_verts={} n_patches={}",
            relocations.len(),
            mesh.verts.len(),
            infos.len(),
        );
    }
    // `YANG_S6_LOOP_SIMPLICITY` census counters (read-only; see
    // `stage5_loop_simplicity`). Reported as one SUMMARY line before returning
    // so the sweep can tell "no self-intersecting loop" from "nothing was
    // measured" — the curved branch has no exact 2D projection and is counted,
    // never silently dropped.
    let mut simp_planar_loops = 0usize;
    let mut simp_nonsimple = 0usize;
    let mut simp_unmeasurable = 0usize;
    let mut simp_curved_faces = 0usize;
    // 2026-08-06 census: of the non-simple loops, how many had Stage 4 CREATE
    // the crossing vs inherit it. Only the minted ones are evidence for a
    // relocation-side repair.
    let mut simp_cross_minted = 0usize;
    let mut simp_cross_inherited = 0usize;
    // (1) Vertices: 1:1 with the (possibly relocated) mesh.verts.
    let vertices: Vec<BRepVertex> = mesh
        .verts
        .iter()
        .map(|&p| BRepVertex { point: p })
        .collect();

    let mut edges: Vec<BRepEdge> = Vec::new();
    let mut faces: Vec<BRepFace> = Vec::new();
    // PR-KV13 F2: per-output-face attribution, pushed in lockstep with `faces`.
    let mut face_attribution: Vec<TriangleAttribution> = Vec::new();
    // Spec yang_stage6_sliver_topology §4B: T-subdivide each loop edge at
    // foreign on-segment output vertices BEFORE emission, so a shared solid
    // edge subdivided differently on its two sides pairs segment-by-segment.
    // A strict no-op (byte-identical cycles) for output with no such vertices.
    let mut subdivided_cycles = subdivide_loops_at_shared_vertices(infos, mesh);
    // #188 inc-3 (spec §10.4): gated boundary-envelope PRE-PASS. Owner
    // rebuilds + neighbor-chain propagation + pairing/planarity audits run
    // together BEFORE emission; on success the rewritten cycles replace the
    // originals for every affected patch and `env_overrides` carries the one
    // global curve map both branches consult. Gate off (or any bail) ⇒
    // byte-identical emission.
    let mut env_overrides: std::collections::BTreeMap<(usize, usize, (u32, u32)), Curve> =
        std::collections::BTreeMap::new();
    // inc-5 (spec §10.8): notch seal patches to emit as STANDALONE
    // cavity-sense faces after the main per-info emission.
    let mut env_extra_faces: Vec<crate::stage5_envelope::ExtraFace> = Vec::new();
    if crate::stage5_envelope::envelope_gate_enabled() {
        if let Some(rw) = crate::stage5_envelope::envelope_prepass(
            mesh,
            infos,
            &subdivided_cycles,
            intersection_curves,
            op,
        )? {
            for (i, cyc) in rw.cycles {
                subdivided_cycles[i] = cyc;
            }
            env_overrides = rw.curve_overrides;
            env_extra_faces = rw.extra_faces;
        }
    }
    for (info_index, info) in infos.iter().enumerate() {
        let cycles = &subdivided_cycles[info_index];
        let inherited = info.inherited;
        let face_idx = info.face_idx;
        let info_attr = TriangleAttribution {
            input: info.input,
            face: info.face_idx as u32,
        };

        // PR-YR8 (P2c) Blocker 2, spec §4: curved-surface branch BEFORE the
        // planar normal/Newell/flip machinery. A `Cylinder` patch is a barrel
        // and a `Sphere` patch is a cap (PR-YR15) — for either, a single plane
        // normal + signed-area classification is meaningless, so we DROP the
        // E3/`positive_count` check and the inherited-normal
        // flip. We INHERIT the surface UNCHANGED (the canonical params must stay
        // exact for downstream SSI / kernel-v2 — we never perturb them to signal
        // sense). Instead, cavity-sense is recorded out-of-band in
        // `BRepFace.reversed`, set from `op == Subtract && info.input == B` — the
        // same `flip_for_op` signal the mesh winding used, so face sense and mesh
        // winding are provably consistent (Union → no cavity → `reversed`
        // false). `patch_boundary_cycle` (called above) is surface-agnostic, so
        // we reuse `cycles`. We KEEP the E2 degenerate-loop guard.
        if matches!(
            inherited,
            Surface::Cylinder { .. }
                | Surface::Sphere { .. }
                | Surface::Cone { .. }
                | Surface::Torus { .. }
        ) {
            // #188 inc-0 (spec yang_188_f0082_j3_envelope_selection §5):
            // read-only osculating-boundary-pair probe. Byte-identical unset.
            if std::env::var_os("YANG_S5_OSCULATION_PROBE").is_some() {
                crate::stage5_osculation_probe::osculation_probe_for_patch(
                    mesh,
                    infos,
                    info_index,
                    &subdivided_cycles,
                    intersection_curves,
                );
            }

            let push_loop =
                |edges: &mut Vec<BRepEdge>, cycle_idx: usize, cycle: &[(u32, u32)]| -> Vec<u32> {
                    let start_idx = edges.len() as u32;
                    for &(s, e) in cycle {
                        let key = if s < e { (s, e) } else { (e, s) };
                        let curve = env_overrides
                            .get(&(info_index, cycle_idx, key))
                            .or_else(|| intersection_curves.get(&key))
                            .copied()
                            .unwrap_or(Curve::LineSegment);
                        edges.push(BRepEdge {
                            start: s,
                            end: e,
                            // Task #133: the undirected curve's normal oriented
                            // for THIS traversal (spec `yang_stage6_arc_orientation`).
                            curve: orient_directed_curve(curve, s, e, &mesh.verts),
                        });
                    }
                    (start_idx..edges.len() as u32).collect()
                };

            // E2 degenerate-loop guard: each cycle's Newell area-vector
            // magnitude must exceed MIN_FEATURE_SIZE² (A14.3 shared constant).
            for cycle in cycles {
                let mut nx = 0.0f64;
                let mut ny = 0.0f64;
                let mut nz = 0.0f64;
                let m = cycle.len();
                for i in 0..m {
                    let a_pt = mesh.verts[cycle[i].0 as usize].as_array();
                    let b_pt = mesh.verts[cycle[(i + 1) % m].0 as usize].as_array();
                    nx += a_pt[1] * b_pt[2] - a_pt[2] * b_pt[1];
                    ny += a_pt[2] * b_pt[0] - a_pt[0] * b_pt[2];
                    nz += a_pt[0] * b_pt[1] - a_pt[1] * b_pt[0];
                }
                let nrm_mag = (nx * nx + ny * ny + nz * nz).sqrt();
                if nrm_mag < cad_primitives::MIN_FEATURE_SIZE * cad_primitives::MIN_FEATURE_SIZE {
                    return Err(non_manifold_at(
                        "s6-curved-degenerate-loop",
                        format_args!(
                            "face {face_idx} cycle len {} |N|={nrm_mag:.3e}",
                            cycle.len()
                        ),
                    ));
                }
            }

            // Empty-cycles guard (PR-CF1 case#23): a kept curved patch can come out with
            // no boundary cycle for the box-as-subtrahend direction (prim − box), which
            // is a DEFERRED, out-of-scope op direction (spec §2) — the reassembly leaves
            // the curved patch with no intersection-boundary loop even though the solid
            // result is non-empty. Such a patch cannot form a bounded face; refuse loudly,
            // mirroring the E2/E3 degenerate-reassembly guards. Without this, the
            // `cycles[outer_idx]` index below panics on the empty set.
            if cycles.is_empty() {
                return Err(non_manifold_at(
                    "s6-curved-empty-cycles",
                    format_args!("face {face_idx}"),
                ));
            }

            // Deterministic loop assignment: outer = the cycle with the MOST
            // edges; tie-break = lowest min start-vertex index within the
            // cycle. All other cycles = inner_loops.
            let cycle_min_vert = |c: &[(u32, u32)]| c.iter().map(|&(s, _)| s).min().unwrap_or(0);
            let mut outer_idx = 0usize;
            for i in 1..cycles.len() {
                let cur_len = cycles[i].len();
                let best_len = cycles[outer_idx].len();
                if cur_len > best_len
                    || (cur_len == best_len
                        && cycle_min_vert(&cycles[i]) < cycle_min_vert(&cycles[outer_idx]))
                {
                    outer_idx = i;
                }
            }

            let outer_loop = push_loop(&mut edges, outer_idx, &cycles[outer_idx]);
            let mut inner_loops: Vec<Vec<u32>> = Vec::new();
            for (i, cycle) in cycles.iter().enumerate() {
                if i != outer_idx {
                    inner_loops.push(push_loop(&mut edges, i, cycle));
                }
            }

            face_attribution.push(info_attr);
            faces.push(BRepFace {
                surface: inherited,
                outer_loop,
                inner_loops,
                // PR-KV6b-1: compose the input face's own cavity sense with
                // the Subtract-B flip (XOR). A no-op for every pre-KV6b
                // fixture (inputs always carried `reversed == false`).
                reversed: info.input_reversed
                    ^ (op == BoolOp::Subtract && info.input == InputId::B),
            });
            // A curved face's loop has no exact 2D projection, so the scan
            // does not cover it. Counted, not silently skipped.
            simp_curved_faces += 1;
            continue;
        }

        let (normal, d) = match inherited {
            Surface::Plane { normal, d } => (normal, d),
            // Cylinder, Sphere, Cone, and Torus are all handled by the curved
            // branch above (PR-YR17 added Cone; KV6d-5b2 added Torus), so these
            // arms are unreachable-defensive. Kept LOUD (P9) for any genuinely
            // unexpected surface.
            Surface::Sphere { .. }
            | Surface::Cylinder { .. }
            | Surface::Cone { .. }
            | Surface::Torus { .. } => {
                return Err(YangError::CurvedSurfaceNotYetSupported { face: face_idx });
            }
        };
        let n = normal.as_array();

        // Per-cycle Newell area-vector `N = Σ v_i × v_{i+1}` and its signed
        // area along the inherited face normal. The kept tris are outward-
        // oriented w.r.t. the RESULT solid, but for Subtract the B-surface
        // tris are flipped (`flip_for_op`) so a B-face patch winds OPPOSITE
        // its inherited normal. So we cannot assume the inherited normal
        // already agrees with the winding: instead, take the largest-area
        // cycle as the patch's outer boundary, let ITS winding define the
        // face's true outward normal (flip the inherited normal if the
        // winding opposes it — a subtracted B-face becomes a cavity wall
        // whose outward normal points into the cavity), then classify the
        // remaining cycles relative to that corrected orientation.
        let mut signed_areas: Vec<f64> = Vec::with_capacity(cycles.len());
        for cycle in cycles {
            let mut nx = 0.0f64;
            let mut ny = 0.0f64;
            let mut nz = 0.0f64;
            let m = cycle.len();
            for i in 0..m {
                let a_pt = mesh.verts[cycle[i].0 as usize].as_array();
                let b_pt = mesh.verts[cycle[(i + 1) % m].0 as usize].as_array();
                nx += a_pt[1] * b_pt[2] - a_pt[2] * b_pt[1];
                ny += a_pt[2] * b_pt[0] - a_pt[0] * b_pt[2];
                nz += a_pt[0] * b_pt[1] - a_pt[1] * b_pt[0];
            }
            // E2: degenerate loop — Newell area-vector magnitude below the
            // minimum feature area (MIN_FEATURE_SIZE²; A14.3 shared constant).
            let nrm_mag = (nx * nx + ny * ny + nz * nz).sqrt();
            if nrm_mag < cad_primitives::MIN_FEATURE_SIZE * cad_primitives::MIN_FEATURE_SIZE {
                return Err(non_manifold_at(
                    "s6-planar-degenerate-loop",
                    format_args!(
                        "face {face_idx} cycle len {} |N|={nrm_mag:.3e}",
                        cycle.len()
                    ),
                ));
            }
            signed_areas.push(nx * n[0] + ny * n[1] + nz * n[2]);
        }

        // Empty-cycles guard (PR-CF1 defensive mirror of the curved branch):
        // a kept planar patch with no boundary cycle cannot form a bounded
        // face. Latent here (the all-planar fuzz never produces empty cycles)
        // but structurally identical to the curved-branch panic — the
        // `signed_areas[outer_idx]` / `cycles[outer_idx]` index below would
        // panic on the empty set. Mirrors the E2/E3 degenerate guards.
        if cycles.is_empty() {
            return Err(non_manifold_at(
                "s6-planar-empty-cycles",
                format_args!("face {face_idx}"),
            ));
        }

        // Outer boundary = the largest-|area| cycle. Its sign (relative to
        // the inherited normal) tells us whether the winding agrees with the
        // inherited normal; if not, flip the stored normal so the output
        // face's normal matches its outward winding.
        let mut outer_idx = 0usize;
        for (i, &s) in signed_areas.iter().enumerate() {
            if s.abs() > signed_areas[outer_idx].abs() {
                outer_idx = i;
            }
        }
        let flip = signed_areas[outer_idx] < 0.0;
        let surface = if flip {
            Surface::Plane {
                normal: Vector3::new(-n[0], -n[1], -n[2]),
                d: -d,
            }
        } else {
            inherited
        };
        // After any flip, the outer cycle's signed area is positive and the
        // holes are negative. E3: a connected outward-oriented patch has
        // EXACTLY one cycle whose corrected sign is positive (its outer
        // boundary). 0 or ≥2 ⇒ disconnected / nested, out of scope.
        let orient = if flip { -1.0 } else { 1.0 };
        let positive_count = signed_areas.iter().filter(|&&s| s * orient > 0.0).count();
        if positive_count != 1 {
            return Err(non_manifold_at(
                "s6-planar-positive-count",
                format_args!("face {face_idx} positive_count={positive_count}"),
            ));
        }

        // Loop-simplicity census (`YANG_S6_LOOP_SIMPLICITY`, read-only — spec
        // in `stage5_loop_simplicity`). Deliberately placed BEFORE the
        // non-planarity gate below: the class it measures is loops that are
        // perfectly PLANAR and self-intersecting, so gating the scan on any
        // wall would make it blind to exactly its own subject. It is also the
        // reason this cannot be a column on an existing probe — every current
        // Stage-6 check is per-vertex, and simplicity is a property of the
        // whole cycle.
        //
        // Set to any value: report only NON-simple loops. Set to `all`: report
        // every loop, so a case with zero findings is distinguishable from a
        // case where emission never ran.
        if let Ok(mode) = std::env::var("YANG_S6_LOOP_SIMPLICITY") {
            let report_all = mode == "all";
            for (ci, cyc) in cycles.iter().enumerate() {
                let pts: Vec<[f64; 3]> = cyc
                    .iter()
                    .map(|&(v, _)| mesh.verts[v as usize].as_array())
                    .collect();
                simp_planar_loops += 1;
                let Some(s) = crate::stage5_loop_simplicity::scan_cycle(&pts, n) else {
                    simp_unmeasurable += 1;
                    eprintln!(
                        "[s6-simplicity] face={face_idx} input={:?} cycle={ci} len={} \
                         UNMEASURABLE (fewer than 3 points, non-finite coordinate, \
                         or degenerate normal)",
                        info.input,
                        cyc.len(),
                    );
                    continue;
                };
                if !s.is_simple() {
                    simp_nonsimple += 1;
                }
                if !s.is_simple() || report_all {
                    // The ratio is the number that made F0067 fatal: a Stage-4
                    // per-vertex displacement LARGER than the local segment it
                    // belongs to cannot stay on its own side of the outline.
                    // `disp` needs the pre-Stage-4 positions, so it is `-`
                    // unless `YANG_S5_FOLD_PROBE` is also set — the sweep sets
                    // both.
                    let disp = S4_PRE_POS.with(|c| {
                        c.borrow().as_ref().map(|m| {
                            cyc.iter().fold(0.0f64, |acc, &(v, _)| {
                                m.get(&v).map_or(acc, |p| {
                                    let q = mesh.verts[v as usize].as_array();
                                    acc.max(
                                        ((q[0] - p[0]).powi(2)
                                            + (q[1] - p[1]).powi(2)
                                            + (q[2] - p[2]).powi(2))
                                        .sqrt(),
                                    )
                                })
                            })
                        })
                    });
                    let (disp_s, ratio_s) = match disp {
                        Some(dp) if s.min_seg.is_finite() && s.min_seg > 0.0 => {
                            (format!("{dp:.4e}"), format!("{:.2}", dp / s.min_seg))
                        }
                        Some(dp) => (format!("{dp:.4e}"), "-".to_string()),
                        None => ("-".to_string(), "-".to_string()),
                    };
                    // 2026-08-06 census: re-scan THIS SAME cycle at the
                    // PRE-Stage-4 positions. `cross` alone cannot say whether
                    // Stage 4 created the crossing or merely inherited one from
                    // Stage 0/2/3 — and that is the whole question, because only
                    // a MINTED crossing is evidence for a relocation-side
                    // repair. Same minted-vs-inherited discipline the fold work
                    // used; vertices absent from the map did not move, so their
                    // current position IS their pre position.
                    //
                    // Uses the POST normal deliberately: `scan_cycle` is
                    // projection-axis invariant (its own test pins that), so the
                    // axis only has to be non-degenerate, and reusing one axis
                    // keeps the two scans comparable.
                    let pre_scan = S4_PRE_POS.with(|c| {
                        c.borrow().as_ref().and_then(|m| {
                            let pre: Vec<[f64; 3]> = cyc
                                .iter()
                                .map(|&(v, _)| {
                                    m.get(&v)
                                        .copied()
                                        .unwrap_or_else(|| mesh.verts[v as usize].as_array())
                                })
                                .collect();
                            crate::stage5_loop_simplicity::scan_cycle(&pre, n)
                        })
                    });
                    let n_moved = S4_PRE_POS.with(|c| {
                        c.borrow().as_ref().map(|m| {
                            cyc.iter()
                                .filter(|&&(v, _)| {
                                    m.get(&v)
                                        .is_some_and(|p| *p != mesh.verts[v as usize].as_array())
                                })
                                .count()
                        })
                    });
                    let (cross_pre_s, class_s) = match &pre_scan {
                        Some(ps) => (
                            format!("{}", ps.crossings),
                            if s.crossings > 0 && ps.crossings == 0 {
                                "MINTED_BY_S4"
                            } else if s.crossings > 0 {
                                "INHERITED"
                            } else {
                                "simple"
                            },
                        ),
                        None => ("-".to_string(), "-"),
                    };
                    match class_s {
                        "MINTED_BY_S4" => simp_cross_minted += 1,
                        "INHERITED" => simp_cross_inherited += 1,
                        _ => {}
                    }
                    let moved_s = n_moved.map_or("-".to_string(), |k| k.to_string());
                    // §4.5.1 step-truncation MEASUREMENT (read-only): for a
                    // crossing Stage-4 MINTED, how far could each moved vertex
                    // actually have travelled before its own loop crossed?
                    //
                    // Measured for the MAX-DISPLACEMENT moved vertex only,
                    // against the otherwise PRE-relocation loop. Two reasons,
                    // both deliberate: it is the same vertex the `max_s4_disp` /
                    // `disp_over_min_seg` columns already track, so the numbers
                    // line up; and scanning every moved vertex is O(roots x m^2)
                    // each, which does not finish on the larger cases.
                    //
                    // So this answers "how far could the WORST vertex have gone
                    // on its own", NOT "what would a joint truncation of all
                    // moved vertices give". Several vertices moving together is
                    // a different question, and this does not claim to answer
                    // it.
                    let trunc_s = if class_s == "MINTED_BY_S4" {
                        S4_PRE_POS.with(|c| {
                            c.borrow().as_ref().map_or("-".to_string(), |m| {
                                let pre: Vec<[f64; 3]> = cyc
                                    .iter()
                                    .map(|&(v, _)| {
                                        m.get(&v)
                                            .copied()
                                            .unwrap_or_else(|| mesh.verts[v as usize].as_array())
                                    })
                                    .collect();
                                // The max-displacement moved vertex.
                                let mut pick: Option<(f64, usize, [f64; 3])> = None;
                                for (i, &(v, _)) in cyc.iter().enumerate() {
                                    let post = mesh.verts[v as usize].as_array();
                                    let Some(p) = m.get(&v) else { continue };
                                    if *p == post {
                                        continue; // did not move
                                    }
                                    let d = ((post[0] - p[0]).powi(2)
                                        + (post[1] - p[1]).powi(2)
                                        + (post[2] - p[2]).powi(2))
                                    .sqrt();
                                    if pick.is_none_or(|(bd, _, _)| d > bd) {
                                        pick = Some((d, i, post));
                                    }
                                }
                                let Some((_, i, post)) = pick else {
                                    return "nomoved".to_string();
                                };
                                use crate::stage4_truncate::StepTruncation as ST;
                                // `YANG_S451_ALL_MOVED`: test EVERY moved vertex
                                // solo, not just the worst one. Answers the
                                // sharper question — is there ANY single vertex
                                // whose own step causes the crossing, or is the
                                // crossing only produced by several moving
                                // together? O(n_moved) more work, so it is opt-in
                                // and only affordable on the smaller cases.
                                // `YANG_S451_JOINT`: scale EVERY moved vertex by
                                // one common factor t and find the largest
                                // crossing-free prefix. Answers whether a JOINT
                                // truncation (rather than §4.5.1's per-point one)
                                // would make the loop simple, and how much of the
                                // step survives.
                                //
                                // A uniform grid scan, NOT bisection: crossings
                                // are not known to be monotone in t, so bisection
                                // could report a safe prefix that is not one. The
                                // resolution is stated in the output (1/200) —
                                // this is a measurement, not a shipped repair.
                                if std::env::var_os("YANG_S451_JOINT").is_some() {
                                    const STEPS: usize = 200;
                                    let moved_idx: Vec<(usize, [f64; 3], [f64; 3])> = cyc
                                        .iter()
                                        .enumerate()
                                        .filter_map(|(j, &(v, _))| {
                                            let q = mesh.verts[v as usize].as_array();
                                            let pp = *m.get(&v)?;
                                            (pp != q).then_some((j, pp, q))
                                        })
                                        .collect();
                                    let mut last_ok = 0usize;
                                    for k in 1..=STEPS {
                                        let t = k as f64 / STEPS as f64;
                                        let mut probe = pre.clone();
                                        for &(j, pp, q) in &moved_idx {
                                            probe[j] = [
                                                pp[0] + t * (q[0] - pp[0]),
                                                pp[1] + t * (q[1] - pp[1]),
                                                pp[2] + t * (q[2] - pp[2]),
                                            ];
                                        }
                                        match crate::stage5_loop_simplicity::scan_cycle(&probe, n) {
                                            Some(sc) if sc.crossings == 0 => last_ok = k,
                                            _ => break,
                                        }
                                    }
                                    return format!(
                                        "joint{:.3}(1/{STEPS})",
                                        last_ok as f64 / STEPS as f64
                                    );
                                }
                                if std::env::var_os("YANG_S451_ALL_MOVED").is_some() {
                                    let mut best: Option<f64> = None;
                                    let mut n_trunc = 0usize;
                                    for (j, &(v, _)) in cyc.iter().enumerate() {
                                        let q = mesh.verts[v as usize].as_array();
                                        let Some(pp) = m.get(&v) else { continue };
                                        if *pp == q {
                                            continue;
                                        }
                                        if let ST::Truncate { t } =
                                            crate::stage4_truncate::max_simple_step(&pre, j, q, n)
                                        {
                                            n_trunc += 1;
                                            best = Some(best.map_or(t, |b: f64| b.min(t)));
                                        }
                                    }
                                    return match best {
                                        Some(t) => format!("{t:.4}/solo{n_trunc}"),
                                        None => "ALLSOLOSAFE".to_string(),
                                    };
                                }
                                match crate::stage4_truncate::max_simple_step(&pre, i, post, n) {
                                    ST::Truncate { t } => format!("{t:.4}"),
                                    ST::FullStepSafe => "safe".to_string(),
                                    ST::AlreadyCrossing => "alreadycrossing".to_string(),
                                    ST::Unmeasurable => "unmeasurable".to_string(),
                                }
                            })
                        })
                    } else {
                        "-".to_string()
                    };
                    eprintln!(
                        "[s6-simplicity] face={face_idx} input={:?} cycle={ci} \
                         role={} len={} cross={} touch={} spike={} degen={} \
                         min_seg={:.4e} max_seg={:.4e} max_s4_disp={disp_s} \
                         disp_over_min_seg={ratio_s} cross_pre={cross_pre_s} \
                         class={class_s} n_moved={moved_s} trunc_t={trunc_s} \
                         first_cross={:?}",
                        info.input,
                        if ci == outer_idx { "outer" } else { "hole" },
                        cyc.len(),
                        s.crossings,
                        s.touches,
                        s.spikes,
                        s.degenerate_segments,
                        s.min_seg,
                        s.max_seg,
                        s.first_crossing,
                    );
                }
            }
        }

        // Task #146 (F0064/R0051 off-plane planar-face emission class):
        // GROSS-non-planarity self-check (yang HARD RULE #4 — the producer
        // validates its own output). A PLANAR output face whose loop vertex is
        // beyond the MODEL coplanarity tolerance `TAU_MODEL` off the inherited
        // plane is not planar at all — it is a topology/grouping defect (a
        // cylinder wall sliver grouped into a floor patch, an over-determined
        // junction relocated onto the wrong surface subset), invalid output
        // that a downstream consumer's Newell / planarity gate would otherwise
        // catch far from its source (kernel-v2's
        // `validate_boolean_output_planarity` at the stricter `TAU_EVAL`, or
        // its `from_yang` Newell wall). Rejecting here gives the class a
        // self-localizing wall at its PRODUCER.
        //
        // Band = `TAU_MODEL` (1e-7), NOT `TAU_EVAL`: a boolean output is only
        // planar to the model coplanarity tolerance, because Stage-0
        // near-coplanar preprocessing legitimately merges faces whose seam
        // vertices carry a residual up to `TAU_MODEL` (see
        // `yr27_face_resolution::near_partial_overlap_residual_1e8` — a DESIGNED
        // 1e-8-residual near-coplanar union whose output is valid, volume
        // includes the residual). `TAU_EVAL` here would false-positive on that
        // designed class. So this is the GROSS-defect producer wall (a real
        // topology bug is ≥ `MIN_FEATURE_SIZE`-scale, ≥10× `TAU_MODEL`);
        // kernel-v2's F1 gate remains the stricter `TAU_EVAL` FINAL check that
        // adjudicates sub-`TAU_MODEL` slips (e.g. F0069's 3e-8). `|n·p + d|` is
        // flip-invariant, so the pre-flip `(n, d)` is used. A REJECT, never a
        // snap (P9): a > `TAU_MODEL` off-plane vertex is a real defect, not
        // f64 noise.
        for cycle in cycles {
            for &(v, _) in cycle {
                let pt = mesh.verts[v as usize].as_array();
                let dist = pt[0] * n[0] + pt[1] * n[1] + pt[2] * n[2] + d;
                let band = cad_primitives::TAU_MODEL
                    * (1.0 + pt[0].abs().max(pt[1].abs()).max(pt[2].abs()));
                if dist.abs() > band {
                    if std::env::var_os("YANG_S6_NONPLANAR_PROBE").is_some() {
                        eprintln!(
                            "YANG_S6_NONPLANAR_PROBE face={face_idx} input={:?} inherited={:?} \
                             n=({:.6},{:.6},{:.6}) d={d:.6} cycles={}",
                            info.input,
                            inherited,
                            n[0],
                            n[1],
                            n[2],
                            cycles.len()
                        );
                        for (ci, cyc) in cycles.iter().enumerate() {
                            for &(vv, _) in cyc {
                                let q = mesh.verts[vv as usize].as_array();
                                let dd = q[0] * n[0] + q[1] * n[1] + q[2] * n[2] + d;
                                let reloc = relocations.iter().find(|(rv, _)| *rv == vv);
                                // Provenance columns (populated only under
                                // `YANG_S5_FOLD_PROBE`): the PRE-Stage-4 position
                                // and its own off-plane residual answer the
                                // masked-vs-minted question directly — an equal
                                // residual before and after means Stage 4 did not
                                // mint this defect. `inc`/`curve` say whether the
                                // vertex was a relocation CANDIDATE at all.
                                let pre = S4_PRE_POS.with(|c| {
                                    c.borrow().as_ref().and_then(|m| m.get(&vv).copied())
                                });
                                let pre_s = match pre {
                                    Some(p) => {
                                        let pd = p[0] * n[0] + p[1] * n[1] + p[2] * n[2] + d;
                                        let disp = ((q[0] - p[0]).powi(2)
                                            + (q[1] - p[1]).powi(2)
                                            + (q[2] - p[2]).powi(2))
                                        .sqrt();
                                        format!(
                                            "pre=({:.12},{:.12},{:.12}) pre_dist={pd:.4e} disp={disp:.4e}",
                                            p[0], p[1], p[2]
                                        )
                                    }
                                    None => "pre=NEW".to_string(),
                                };
                                let inc_s = S4_VERT_SURF.with(|c| {
                                    c.borrow().as_ref().map_or_else(
                                        || "?".to_string(),
                                        |m| {
                                            m.get(&vv).map_or_else(
                                                || "-".to_string(),
                                                // NOT deduped: two DISTINCT
                                                // surfaces of one operand share
                                                // a label (`A:Plane`), and at a
                                                // flush junction that
                                                // multiplicity is the whole
                                                // point — deduping it reads as
                                                // "one plane" and hides the
                                                // coplanar duplicate.
                                                |v| {
                                                    let mut l: Vec<&str> =
                                                        v.iter().map(|(s, _)| s.as_str()).collect();
                                                    l.sort_unstable();
                                                    l.join(",")
                                                },
                                            )
                                        },
                                    )
                                });
                                let cur_s = S4_VERT_CURVE.with(|c| {
                                    c.borrow().as_ref().map_or_else(
                                        || "?".to_string(),
                                        |m| {
                                            m.get(&vv).map_or_else(
                                                || "-".to_string(),
                                                |v| v.iter().copied().collect::<Vec<_>>().join(","),
                                            )
                                        },
                                    )
                                });
                                eprintln!(
                                    "  cyc{ci} v={vv} p=({:.12},{:.12},{:.12}) dist={dd:.4e} \
                                     reloc={reloc:?} {pre_s} inc=[{inc_s}] curve=[{cur_s}]",
                                    q[0], q[1], q[2]
                                );
                            }
                        }
                    }
                    return Err(non_manifold_at(
                        "s6-planar-loop-nonplanar",
                        format_args!(
                            "face {face_idx} vert {v} off-plane d={dist:.3e} band={band:.3e}"
                        ),
                    ));
                }
            }
        }

        // Diagnosis probe (read-only, env-gated): `YANG_S6_LOOP_PROV=x,y,z,r`
        // dumps every emitted loop that passes within `r` of the target point,
        // each vertex with its Stage-4 provenance. Unlike the nonplanar probe's
        // columns, this one is NOT gated on a wall firing — a loop can be
        // geometrically invalid (self-intersecting, so refused downstream by an
        // exact CDT) while every one of its vertices sits perfectly ON the
        // inherited plane, and that class has no producer-side gate at all.
        if let Ok(spec) = std::env::var("YANG_S6_LOOP_PROV") {
            let f: Vec<f64> = spec
                .split(',')
                .filter_map(|s| s.trim().parse().ok())
                .collect();
            if f.len() == 4 {
                let (tx, ty, tz, tr) = (f[0], f[1], f[2], f[3]);
                for (ci, cyc) in cycles.iter().enumerate() {
                    let near = cyc.iter().any(|&(v, _)| {
                        let p = mesh.verts[v as usize].as_array();
                        ((p[0] - tx).powi(2) + (p[1] - ty).powi(2) + (p[2] - tz).powi(2)).sqrt()
                            <= tr
                    });
                    if !near {
                        continue;
                    }
                    eprintln!(
                        "[s6-loop-prov] face={face_idx} input={:?} cycle={ci} len={} \
                         n=({:.9},{:.9},{:.9}) d={d:.12}",
                        info.input,
                        cyc.len(),
                        n[0],
                        n[1],
                        n[2]
                    );
                    for (k, &(v, _)) in cyc.iter().enumerate() {
                        let p = mesh.verts[v as usize].as_array();
                        eprintln!(
                            "[s6-loop-prov]   {k:3} v={v} p=({:.15},{:.15},{:.15}) {}",
                            p[0],
                            p[1],
                            p[2],
                            probe_vertex_prov(v, p)
                        );
                    }
                }
            }
        }

        let outer_cycle = &cycles[outer_idx];
        let inner_cycles: Vec<(usize, &Vec<(u32, u32)>)> = cycles
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != outer_idx)
            .collect();

        // Emit the outer loop's edges first, then each inner loop's edges.
        let push_loop =
            |edges: &mut Vec<BRepEdge>, cycle_idx: usize, cycle: &[(u32, u32)]| -> Vec<u32> {
                let start_idx = edges.len() as u32;
                for &(s, e) in cycle {
                    let key = if s < e { (s, e) } else { (e, s) };
                    let curve = env_overrides
                        .get(&(info_index, cycle_idx, key))
                        .or_else(|| intersection_curves.get(&key))
                        .copied()
                        .unwrap_or(Curve::LineSegment);
                    edges.push(BRepEdge {
                        start: s,
                        end: e,
                        // Task #133: the undirected curve's normal oriented for
                        // THIS traversal (spec `yang_stage6_arc_orientation`).
                        curve: orient_directed_curve(curve, s, e, &mesh.verts),
                    });
                }
                (start_idx..edges.len() as u32).collect()
            };

        let outer_loop = push_loop(&mut edges, outer_idx, outer_cycle);
        let mut inner_loops: Vec<Vec<u32>> = Vec::with_capacity(inner_cycles.len());
        for (i, inner) in &inner_cycles {
            inner_loops.push(push_loop(&mut edges, *i, inner));
        }

        // Task #146 diagnosis probe (read-only, env-gated): a planar output
        // face whose loop vertices sit off the inherited plane at real scale
        // — dump the offenders so the producing relocation can be identified.
        if std::env::var_os("YANG_T146_PROBE").is_some() {
            for cycle in cycles {
                for &(s, _) in cycle {
                    let p = mesh.verts[s as usize].as_array();
                    let dist = (p[0] * n[0] + p[1] * n[1] + p[2] * n[2] + d).abs();
                    if dist > 1.0e-6 {
                        eprintln!(
                            "[t146] face {face_idx} ({:?}) off-plane vert {s} dist={dist:.3e} \
                             p={p:?} plane_n={n:?} d={d}",
                            info.input
                        );
                    }
                }
            }
        }
        // §4.3.4 monotone-re-sample test (read-only, env-gated `YANG_S5_CHAIN`).
        // Spec §8j: the remaining Phase-C hypothesis is that a relocated chain must
        // be re-derived as a MONOTONE polyline along its analytic curve. That is
        // only a fix if the chain is currently NON-monotone, so measure it: for each
        // maximal run of consecutive loop edges carrying the SAME ellipse, report
        // every vertex's exact ellipse parameter in traversal order.
        //
        // Monotone (mod the 2pi wrap) ⇒ re-sampling that chain at the same vertex
        // count reproduces the same order and CANNOT clear the fold; the defect is
        // then at the chain's junction with its neighbour, not inside it.
        if std::env::var_os("YANG_S5_CHAIN").is_some() {
            for (tag, lp) in std::iter::once(("outer", &outer_loop)).chain(
                inner_loops
                    .iter()
                    .enumerate()
                    .map(|(i, l)| (if i == 0 { "inner0" } else { "innerN" }, l)),
            ) {
                let n_e = lp.len();
                let mut k = 0usize;
                while k < n_e {
                    let e = &edges[lp[k] as usize];
                    let Curve::Ellipse {
                        center,
                        normal,
                        major_axis,
                        major_radius,
                        minor_radius,
                    } = e.curve
                    else {
                        k += 1;
                        continue;
                    };
                    // Extend while the next edge carries a bit-identical ellipse.
                    let same = |c: &Curve| -> bool {
                        matches!(c, Curve::Ellipse { center: c2, normal: n2, major_axis: m2, major_radius: a2, minor_radius: b2 }
                            if c2.as_array() == center.as_array()
                                && n2.as_array() == normal.as_array()
                                && m2.as_array() == major_axis.as_array()
                                && *a2 == major_radius
                                && *b2 == minor_radius)
                    };
                    let mut run = vec![e.start, e.end];
                    let mut j = k + 1;
                    while j < n_e && same(&edges[lp[j] as usize].curve) {
                        run.push(edges[lp[j] as usize].end);
                        j += 1;
                    }
                    let params: Vec<f64> = run
                        .iter()
                        .map(|&v| {
                            crate::geom::ellipse_param(
                                mesh.verts[v as usize],
                                center,
                                normal,
                                major_axis,
                                major_radius,
                                minor_radius,
                            )
                        })
                        .collect();
                    // Monotone test on the UNWRAPPED sequence: lift each successive
                    // parameter into the branch nearest its predecessor, so a chain
                    // crossing the atan2 seam is not misreported as a reversal.
                    let mut lifted = Vec::with_capacity(params.len());
                    let two_pi = std::f64::consts::TAU;
                    for (i, &t) in params.iter().enumerate() {
                        if i == 0 {
                            lifted.push(t);
                            continue;
                        }
                        let prev: f64 = lifted[i - 1];
                        let mut u = t;
                        while u - prev > std::f64::consts::PI {
                            u -= two_pi;
                        }
                        while prev - u > std::f64::consts::PI {
                            u += two_pi;
                        }
                        lifted.push(u);
                    }
                    let deltas: Vec<f64> = lifted.windows(2).map(|w| w[1] - w[0]).collect();
                    let n_pos = deltas.iter().filter(|d| **d > 0.0).count();
                    let n_neg = deltas.iter().filter(|d| **d < 0.0).count();
                    let monotone = n_pos == 0 || n_neg == 0;
                    eprintln!(
                        "YANG_S5_CHAIN face={face_idx} input={:?} loop={tag} k={k} \
                         len={} MONOTONE={monotone} n_pos={n_pos} n_neg={n_neg} \
                         verts={run:?} params={:?} deltas={:?}",
                        info.input,
                        run.len(),
                        lifted.iter().map(|t| format!("{t:.9}")).collect::<Vec<_>>(),
                        deltas
                            .iter()
                            .map(|d| format!("{d:.3e}"))
                            .collect::<Vec<_>>(),
                    );
                    k = j.max(k + 1);
                }
            }
        }
        // Ring-fold probe (read-only, env-gated) — the yang-side counterpart of
        // kernel-v2's `KV2_RING_PROVENANCE`. The planar seam-overlap class
        // (R0074/R0011/F0045) shows up downstream as a near-180 deg fold in the
        // rendered ring; this asks whether the fold is ALREADY present in the
        // loop this stage emits, and if so which mesh verts and curves carry it.
        if std::env::var_os("YANG_S5_FOLD_PROBE").is_some() {
            let cname = |c: &Curve| -> &'static str {
                match c {
                    Curve::LineSegment => "LineSegment",
                    Curve::Circle { .. } => "Circle",
                    Curve::Ellipse { .. } => "Ellipse",
                    Curve::Parabola { .. } => "Parabola",
                    Curve::Hyperbola { .. } => "Hyperbola",
                    Curve::SurfacePair { .. } => "SurfacePair",
                }
            };
            for (tag, lp) in std::iter::once(("outer", &outer_loop)).chain(
                inner_loops
                    .iter()
                    .enumerate()
                    .map(|(i, l)| (if i == 0 { "inner0" } else { "innerN" }, l)),
            ) {
                let n_e = lp.len();
                for k in 0..n_e {
                    let e_prev = &edges[lp[(k + n_e - 1) % n_e] as usize];
                    let e_cur = &edges[lp[k] as usize];
                    let a = mesh.verts[e_prev.start as usize].as_array();
                    let b = mesh.verts[e_cur.start as usize].as_array();
                    let c = mesh.verts[e_cur.end as usize].as_array();
                    let v1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
                    let v2 = [c[0] - b[0], c[1] - b[1], c[2] - b[2]];
                    let l1 = (v1[0] * v1[0] + v1[1] * v1[1] + v1[2] * v1[2]).sqrt();
                    let l2 = (v2[0] * v2[0] + v2[1] * v2[1] + v2[2] * v2[2]).sqrt();
                    if l1 == 0.0 || l2 == 0.0 {
                        continue;
                    }
                    let dot = (v1[0] * v2[0] + v1[1] * v2[1] + v1[2] * v2[2]) / (l1 * l2);
                    let turn = dot.clamp(-1.0, 1.0).acos().to_degrees();
                    if turn > 120.0 {
                        // Was the fold APEX (and its neighbours) moved by the
                        // Stage-4 relocation, or is the spur inherited from the
                        // Stage-2/3 patch boundary cycle? This is the
                        // discriminator between the two candidate mints.
                        // MOVED = positional diff across Stage 4 (the correct
                        // oracle; covers the torus arm). reloc = conic t-retag
                        // only, kept for the conic cases but BLIND on tori.
                        let disp = |v: u32| -> Option<[f64; 3]> {
                            S4_PRE_POS
                                .with(|c| c.borrow().as_ref().and_then(|s| s.get(&v).copied()))
                        };
                        // Displacement is derived (post − pre) rather than stored,
                        // so it stays exact even for a vertex a later collapse or
                        // weld moved again. A vertex with no pre position was
                        // minted during Stage 4 — reported as `new`, not `still`.
                        let mv = |v: u32, post: [f64; 3]| -> String {
                            S4_PRE_POS.with(|c| match &*c.borrow() {
                                Some(set) => match set.get(&v) {
                                    Some(p) if *p == post => "still".to_string(),
                                    Some(p) => {
                                        let m = ((post[0] - p[0]).powi(2)
                                            + (post[1] - p[1]).powi(2)
                                            + (post[2] - p[2]).powi(2))
                                        .sqrt();
                                        format!("MOVED({m:.3e})")
                                    }
                                    None => "new".to_string(),
                                },
                                None => "n/a".to_string(),
                            })
                        };
                        let inc_of = |v: u32| -> String {
                            S4_VERT_SURF.with(|c| match &*c.borrow() {
                                Some(m) => match m.get(&v) {
                                    Some(set) => set
                                        .iter()
                                        .map(|(l, _)| l.clone())
                                        .collect::<Vec<_>>()
                                        .join("+"),
                                    None => "NO-INCIDENCE".to_string(),
                                },
                                None => "n/a".to_string(),
                            })
                        };
                        // THE DISCRIMINATOR (spec §8g): the implicit residual of the
                        // vertex's FINAL position against every surface it is
                        // incident to. A relocated vertex that satisfies all of them
                        // is ON its curve and any fold is a point-SELECTION defect
                        // (it arrived at the wrong place along the curve); one that
                        // fails a surface never arrived at all. Displacement
                        // magnitude and direction cannot tell these apart.
                        let resid_of = |v: u32, post: [f64; 3]| -> String {
                            S4_VERT_SURF.with(|c| match &*c.borrow() {
                                Some(m) => match m.get(&v) {
                                    Some(set) => set
                                        .iter()
                                        .map(|(l, sf)| {
                                            match crate::stage4_relocate::surface_value_and_normal(
                                                *sf, post,
                                            ) {
                                                Some((f, _)) => format!("{l}={f:.3e}"),
                                                None => format!("{l}=?"),
                                            }
                                        })
                                        .collect::<Vec<_>>()
                                        .join(","),
                                    None => "NO-INCIDENCE".to_string(),
                                },
                                None => "n/a".to_string(),
                            })
                        };
                        // The precise candidate oracle (see S4_VERT_CURVE): does
                        // an analytic intersection curve pass through the vertex
                        // at all? `inc_of` cannot answer this — an own-rim
                        // A:Plane|A:Torus vertex has curved incidence and NO curve.
                        let cv_of = |v: u32| -> String {
                            S4_VERT_CURVE.with(|c| match &*c.borrow() {
                                Some(m) => match m.get(&v) {
                                    Some(set) => set.iter().copied().collect::<Vec<_>>().join("+"),
                                    None => "NO-CURVE".to_string(),
                                },
                                None => "n/a".to_string(),
                            })
                        };
                        // Control: re-evaluate this fold's turn angle at the
                        // PRE-Stage-4 positions. A fold that is already >120 deg
                        // there is INHERITED from the Stage-2/3 boundary cycle;
                        // only one that is clean before and folded after was minted
                        // by relocation.
                        // A vertex with no recorded pre position (minted during
                        // Stage 4) contributes its post position — it has no
                        // "before" to compare against.
                        let pre_of =
                            |v: u32, post: [f64; 3]| -> [f64; 3] { disp(v).unwrap_or(post) };
                        let (a_pre, b_pre, c_pre) = (
                            pre_of(e_prev.start, a),
                            pre_of(e_cur.start, b),
                            pre_of(e_cur.end, c),
                        );
                        // Honest unavailability: with no moved-set the "pre"
                        // positions ARE the post positions, so a computed number
                        // would equal `turn` exactly and read as "inherited fold"
                        // — a measurement that was never taken must not look like
                        // one that was.
                        let have_moved = S4_PRE_POS.with(|c| c.borrow().is_some());
                        // The PRE-Stage-4 spacing of the fold triple. This — not
                        // the post-relocation edge length — is the denominator the
                        // relocation had to respect: two vertices that start
                        // `len_pre` apart cannot be independently displaced by
                        // more than that without risking an order inversion.
                        let seg = |p: [f64; 3], q: [f64; 3]| -> f64 {
                            ((q[0] - p[0]).powi(2) + (q[1] - p[1]).powi(2) + (q[2] - p[2]).powi(2))
                                .sqrt()
                        };
                        let (l1_pre, l2_pre) = if have_moved {
                            (seg(a_pre, b_pre), seg(b_pre, c_pre))
                        } else {
                            (f64::NAN, f64::NAN)
                        };
                        let turn_pre = if !have_moved {
                            f64::NAN
                        } else {
                            let w1 = [
                                b_pre[0] - a_pre[0],
                                b_pre[1] - a_pre[1],
                                b_pre[2] - a_pre[2],
                            ];
                            let w2 = [
                                c_pre[0] - b_pre[0],
                                c_pre[1] - b_pre[1],
                                c_pre[2] - b_pre[2],
                            ];
                            let m1 = (w1[0] * w1[0] + w1[1] * w1[1] + w1[2] * w1[2]).sqrt();
                            let m2 = (w2[0] * w2[0] + w2[1] * w2[1] + w2[2] * w2[2]).sqrt();
                            if m1 == 0.0 || m2 == 0.0 {
                                f64::NAN
                            } else {
                                ((w1[0] * w2[0] + w1[1] * w2[1] + w1[2] * w2[2]) / (m1 * m2))
                                    .clamp(-1.0, 1.0)
                                    .acos()
                                    .to_degrees()
                            }
                        };
                        // Decompose the apex displacement against the chain
                        // direction it was moved along (measured PRE-Stage-4, so
                        // the frame is not itself contaminated by the move).
                        // Relocation that merely removes off-curve tessellation
                        // error is NORMAL to the chain and preserves local order;
                        // only a TANGENTIAL component can slide a vertex past its
                        // neighbour and invert the traversal.
                        let (d_tan, d_nrm) = match disp(e_cur.start) {
                            Some(p0) => {
                                let d = [b[0] - p0[0], b[1] - p0[1], b[2] - p0[2]];
                                let t = [
                                    c_pre[0] - a_pre[0],
                                    c_pre[1] - a_pre[1],
                                    c_pre[2] - a_pre[2],
                                ];
                                let tl = (t[0] * t[0] + t[1] * t[1] + t[2] * t[2]).sqrt();
                                if tl == 0.0 {
                                    (f64::NAN, f64::NAN)
                                } else {
                                    let u = [t[0] / tl, t[1] / tl, t[2] / tl];
                                    let dt = d[0] * u[0] + d[1] * u[1] + d[2] * u[2];
                                    let r = [d[0] - dt * u[0], d[1] - dt * u[1], d[2] - dt * u[2]];
                                    (dt.abs(), (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt())
                                }
                            }
                            None => (f64::NAN, f64::NAN),
                        };
                        let rl = |v: u32| -> String {
                            match relocations.iter().find(|(rv, _)| *rv == v) {
                                Some((_, t)) => format!("reloc(t={t:.6e})"),
                                None => "none".to_string(),
                            }
                        };
                        eprintln!(
                            "YANG_S5_FOLD face={face_idx} input={:?} loop={tag} k={k} \
                             turn={turn:.2} turn_pre={turn_pre:.2} \
                             apex_tan={d_tan:.6e} apex_nrm={d_nrm:.6e} \
                             len_pre_prev={l1_pre:.6e} len_pre_cur={l2_pre:.6e} \
                             verts=({},{},{}) prev_curve={:?} cur_curve={:?} \
                             len_prev={l1:.6e} len_cur={l2:.6e} \
                             moved=({},{},{}) \
                             inc=[{} | {} | {}] \
                             resid=[{} | {} | {}] \
                             resid_pre=[{} | {} | {}] \
                             curve=[{} | {} | {}] \
                             reloc_prev={} reloc_apex={} reloc_next={} \
                             p=[{:.12},{:.12},{:.12}]",
                            info.input,
                            e_prev.start,
                            e_cur.start,
                            e_cur.end,
                            cname(&e_prev.curve),
                            cname(&e_cur.curve),
                            mv(e_prev.start, a),
                            mv(e_cur.start, b),
                            mv(e_cur.end, c),
                            inc_of(e_prev.start),
                            inc_of(e_cur.start),
                            inc_of(e_cur.end),
                            resid_of(e_prev.start, a),
                            resid_of(e_cur.start, b),
                            resid_of(e_cur.end, c),
                            // Same residuals at the PRE-relocation position. If a
                            // vertex was ALREADY on its surfaces before Stage 4 and
                            // then moved far, the move went to a DIFFERENT root of
                            // the same constraints — a selection defect the
                            // exactness certificate cannot see, because both points
                            // satisfy it. If instead the pre-residual is large, the
                            // mesh really was that far off and the move is earned.
                            resid_of(e_prev.start, a_pre),
                            resid_of(e_cur.start, b_pre),
                            resid_of(e_cur.end, c_pre),
                            cv_of(e_prev.start),
                            cv_of(e_cur.start),
                            cv_of(e_cur.end),
                            rl(e_prev.start),
                            rl(e_cur.start),
                            rl(e_cur.end),
                            b[0],
                            b[1],
                            b[2],
                        );
                        // Fig-11 q nearest-root check (epic spec §8k "next
                        // measurement"): an apex on a Cylinder + two DISTINCT
                        // Planes sits on `cylinder ∩ (plane∩plane line)`,
                        // which has ≤2 roots. §8h proved the FINAL position
                        // is ON all three surfaces (~1e-13) — exactly ON one
                        // of the two roots — but the residual cannot say
                        // WHICH. Solve both in closed form and compare each
                        // to the PRE-Stage-4 position: a vertex seated at the
                        // root FARTHER from where it started is a
                        // point-selection defect invisible to every residual
                        // test (both roots satisfy the certificate).
                        S4_VERT_SURF.with(|cell| {
                            let borrow = cell.borrow();
                            let Some(m) = &*borrow else { return };
                            let Some(set) = m.get(&e_cur.start) else { return };
                            let cyls: Vec<(String, Surface)> = set
                                .iter()
                                .filter(|(_, s)| matches!(s, Surface::Cylinder { .. }))
                                .cloned()
                                .collect();
                            let planes: Vec<(String, Surface)> = set
                                .iter()
                                .filter(|(_, s)| matches!(s, Surface::Plane { .. }))
                                .cloned()
                                .collect();
                            if cyls.len() != 1 || planes.len() < 2 {
                                return;
                            }
                            let Surface::Cylinder {
                                axis_point,
                                axis_dir,
                                radius,
                            } = cyls[0].1
                            else {
                                return;
                            };
                            let al = axis_dir.as_array();
                            let alen = (al[0] * al[0] + al[1] * al[1] + al[2] * al[2]).sqrt();
                            if alen <= 0.0 {
                                return;
                            }
                            let ah = [al[0] / alen, al[1] / alen, al[2] / alen];
                            let pre = disp(e_cur.start).unwrap_or(b);
                            for i in 0..planes.len() {
                                for j in (i + 1)..planes.len() {
                                    let (
                                        Surface::Plane { normal: n1, d: d1 },
                                        Surface::Plane { normal: n2, d: d2 },
                                    ) = (planes[i].1, planes[j].1)
                                    else {
                                        continue;
                                    };
                                    let (n1, n2) = (n1.as_array(), n2.as_array());
                                    let dir = [
                                        n1[1] * n2[2] - n1[2] * n2[1],
                                        n1[2] * n2[0] - n1[0] * n2[2],
                                        n1[0] * n2[1] - n1[1] * n2[0],
                                    ];
                                    let dl2 = dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2];
                                    let g11 = n1[0] * n1[0] + n1[1] * n1[1] + n1[2] * n1[2];
                                    let g22 = n2[0] * n2[0] + n2[1] * n2[1] + n2[2] * n2[2];
                                    let g12 = n1[0] * n2[0] + n1[1] * n2[1] + n1[2] * n2[2];
                                    let det = g11 * g22 - g12 * g12;
                                    if dl2.sqrt() <= 1e-12 * (g11 * g22).sqrt() || det <= 0.0 {
                                        continue; // same/parallel plane pair — no line
                                    }
                                    let alpha = (-d1 * g22 + d2 * g12) / det;
                                    let beta = (-d2 * g11 + d1 * g12) / det;
                                    let q0 = [
                                        alpha * n1[0] + beta * n2[0],
                                        alpha * n1[1] + beta * n2[1],
                                        alpha * n1[2] + beta * n2[2],
                                    ];
                                    let dl = dl2.sqrt();
                                    let dh = [dir[0] / dl, dir[1] / dl, dir[2] / dl];
                                    let ap = axis_point.as_array();
                                    let w = [q0[0] - ap[0], q0[1] - ap[1], q0[2] - ap[2]];
                                    let wa = w[0] * ah[0] + w[1] * ah[1] + w[2] * ah[2];
                                    let wp =
                                        [w[0] - wa * ah[0], w[1] - wa * ah[1], w[2] - wa * ah[2]];
                                    let da = dh[0] * ah[0] + dh[1] * ah[1] + dh[2] * ah[2];
                                    let dp = [
                                        dh[0] - da * ah[0],
                                        dh[1] - da * ah[1],
                                        dh[2] - da * ah[2],
                                    ];
                                    let qa = dp[0] * dp[0] + dp[1] * dp[1] + dp[2] * dp[2];
                                    let qb = 2.0 * (wp[0] * dp[0] + wp[1] * dp[1] + wp[2] * dp[2]);
                                    let qc = wp[0] * wp[0] + wp[1] * wp[1] + wp[2] * wp[2]
                                        - radius * radius;
                                    if qa <= 0.0 {
                                        eprintln!(
                                            "YANG_S5_QROOT apex={} pair=({},{})x{} AXIS-PARALLEL",
                                            e_cur.start, planes[i].0, planes[j].0, cyls[0].0
                                        );
                                        continue;
                                    }
                                    let disc = qb * qb - 4.0 * qa * qc;
                                    if disc < 0.0 {
                                        eprintln!(
                                            "YANG_S5_QROOT apex={} pair=({},{})x{} MISS disc={disc:.3e}",
                                            e_cur.start, planes[i].0, planes[j].0, cyls[0].0
                                        );
                                        continue;
                                    }
                                    let sq = disc.sqrt();
                                    let (t0, t1) =
                                        ((-qb - sq) / (2.0 * qa), (-qb + sq) / (2.0 * qa));
                                    let root = |t: f64| {
                                        [q0[0] + t * dh[0], q0[1] + t * dh[1], q0[2] + t * dh[2]]
                                    };
                                    let (x0, x1) = (root(t0), root(t1));
                                    let dist = |p: [f64; 3], q: [f64; 3]| {
                                        ((p[0] - q[0]).powi(2)
                                            + (p[1] - q[1]).powi(2)
                                            + (p[2] - q[2]).powi(2))
                                        .sqrt()
                                    };
                                    let sep = dist(x0, x1);
                                    let (d_post0, d_post1) = (dist(b, x0), dist(b, x1));
                                    let (d_pre0, d_pre1) = (dist(pre, x0), dist(pre, x1));
                                    let post_at = if d_post0 <= d_post1 { 0 } else { 1 };
                                    let pre_near = if d_pre0 <= d_pre1 { 0 } else { 1 };
                                    eprintln!(
                                        "YANG_S5_QROOT apex={} pair=({},{})x{} sep={sep:.6e} \
                                         d_post=({d_post0:.3e},{d_post1:.3e}) \
                                         d_pre=({d_pre0:.6e},{d_pre1:.6e}) \
                                         post_at=root{post_at} pre_near=root{pre_near} verdict={}",
                                        e_cur.start,
                                        planes[i].0,
                                        planes[j].0,
                                        cyls[0].0,
                                        if post_at == pre_near {
                                            "NEAREST"
                                        } else {
                                            "FAR-ROOT"
                                        },
                                    );
                                }
                            }
                        });
                    }
                }
            }
        }
        face_attribution.push(info_attr);
        faces.push(BRepFace {
            surface,
            outer_loop,
            inner_loops,
            reversed: false,
        });
    }

    // inc-5 (spec §10.8): emit each notch seal patch as a STANDALONE face
    // of the owner's surface with CAVITY sense — the opposite of the owner
    // face's. The seal faces the sub-observable void pocket (F0082: the
    // crevice-slot end between plate top, plate side, and floating cap),
    // so its outward normal points INTO the owner's surface (a reversed
    // cylinder patch, the washer-inner-tube vocabulary). Its edges pair
    // the strip boundary with three DIFFERENT planar neighbors — real
    // topology; only the face bookkeeping differs from inc-3's inner-loop
    // form (which spliced a phantom handle and escaped the owner's outer
    // cycle — the inc-4a containment refutation).
    for (owner, cycle, curves) in env_extra_faces {
        let info = &infos[owner];
        let start_idx = edges.len() as u32;
        for &(s, e) in &cycle {
            let key = if s < e { (s, e) } else { (e, s) };
            let curve = curves
                .get(&key)
                .or_else(|| intersection_curves.get(&key))
                .copied()
                .unwrap_or(Curve::LineSegment);
            edges.push(BRepEdge {
                start: s,
                end: e,
                curve: orient_directed_curve(curve, s, e, &mesh.verts),
            });
        }
        let outer_loop: Vec<u32> = (start_idx..edges.len() as u32).collect();
        face_attribution.push(TriangleAttribution {
            input: info.input,
            face: info.face_idx as u32,
        });
        faces.push(BRepFace {
            surface: info.inherited,
            outer_loop,
            inner_loops: Vec::new(),
            reversed: !(info.input_reversed ^ (op == BoolOp::Subtract && info.input == InputId::B)),
        });
    }

    // Tessellation sources (PR-YR10): default `BRepVertex(i)`; each relocated /
    // retagged intersection vertex overrides to `BRepEdge { edge, t }` where
    // `edge` is the FIRST output Circle edge incident to the vertex (the output
    // edges exist only after the emission pass above). The angle `t` is the
    // circle-frame parameter Stage 4 computed, so `eval_source` reproduces the
    // relocated position exactly.
    let mut sources: Vec<TessellationSource> = (0..mesh.num_verts() as u32)
        .map(TessellationSource::BRepVertex)
        .collect();
    for &(vid, t) in relocations {
        if (vid as usize) >= sources.len() {
            continue;
        }
        let edge_idx = edges.iter().position(|e| {
            matches!(
                e.curve,
                Curve::Circle { .. }
                    | Curve::Ellipse { .. }
                    | Curve::Parabola { .. }
                    | Curve::Hyperbola { .. }
            ) && (e.start == vid || e.end == vid)
        });
        if let Some(ei) = edge_idx {
            // Task #133 (spec `yang_stage6_arc_orientation`): Stage 4
            // computed `t` in the intersection curve's ORIGINAL frame; the
            // emitted copy's normal may be the per-traversal negation
            // (`orient_directed_curve`), which MIRRORS the angular
            // parameterization. Recompute `t` against the CHOSEN edge's
            // stored curve from the relocated mesh position, so the
            // `eval_source` round-trip holds by construction. Parabola /
            // Hyperbola frames are never flipped — keep Stage-4's `t`.
            let t_edge = match edges[ei].curve {
                Curve::Circle { center, normal, .. } => {
                    let (e1, e2) = ortho_basis(normal);
                    let (c, e1a, e2a) = (center.as_array(), e1.as_array(), e2.as_array());
                    let q = mesh.verts[vid as usize].as_array();
                    let w = [q[0] - c[0], q[1] - c[1], q[2] - c[2]];
                    let x = w[0] * e1a[0] + w[1] * e1a[1] + w[2] * e1a[2];
                    let y = w[0] * e2a[0] + w[1] * e2a[1] + w[2] * e2a[2];
                    y.atan2(x)
                }
                Curve::Ellipse {
                    center,
                    normal,
                    major_axis,
                    major_radius,
                    minor_radius,
                } => ellipse_param(
                    mesh.verts[vid as usize],
                    center,
                    normal,
                    major_axis,
                    major_radius,
                    minor_radius,
                ),
                _ => t,
            };
            sources[vid as usize] = TessellationSource::BRepEdge {
                edge: ei as u32,
                t: t_edge,
            };
        }
    }

    if std::env::var_os("YANG_S6_LOOP_SIMPLICITY").is_some() {
        eprintln!(
            "[s6-simplicity] SUMMARY planar_loops={simp_planar_loops} \
             nonsimple={simp_nonsimple} unmeasurable={simp_unmeasurable} \
             curved_faces_not_scanned={simp_curved_faces} \
             cross_minted_by_s4={simp_cross_minted} \
             cross_inherited={simp_cross_inherited} \
             (minted+inherited < nonsimple means the pre-position map was \
             unavailable for the rest — set YANG_S5_FOLD_PROBE)"
        );
    }
    Ok((vertices, edges, faces, sources, face_attribution))
}

/// PR-YR5 internal: grouped patch of same-attribution triangles.
pub(crate) struct Patch {
    pub(crate) attribution: TriangleAttribution,
    pub(crate) tri_indices: Vec<u32>,
}

/// PR-YR5 helper: per-triangle neighbor list via canonical-edge
/// BTreeMap (deterministic insertion + iteration order).
pub(crate) fn triangle_adjacency(mesh: &Mesh) -> Vec<Vec<u32>> {
    use std::collections::BTreeMap;
    let mut edge_to_tris: BTreeMap<(u32, u32), Vec<u32>> = BTreeMap::new();
    for (t, tri) in mesh.tris.iter().enumerate() {
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            let (va, vb) = (tri[i], tri[j]);
            let key = if va < vb { (va, vb) } else { (vb, va) };
            edge_to_tris.entry(key).or_default().push(t as u32);
        }
    }
    let mut neighbors: Vec<Vec<u32>> = vec![Vec::new(); mesh.tris.len()];
    for sharing in edge_to_tris.values() {
        for &t1 in sharing {
            for &t2 in sharing {
                if t1 != t2 && !neighbors[t1 as usize].contains(&t2) {
                    neighbors[t1 as usize].push(t2);
                }
            }
        }
    }
    neighbors
}

/// PR-YR5 helper: BFS flood-fill same-attribution triangles into
/// patches. Skip None-attributed triangles. Deterministic seed order:
/// lowest unvisited tri index first.
pub(crate) fn flood_fill_patches(
    mesh: &Mesh,
    attribution: &TriangleAttributionMap,
    adjacency: &[Vec<u32>],
) -> Vec<Patch> {
    use std::collections::VecDeque;
    let mut visited = vec![false; mesh.tris.len()];
    let mut patches: Vec<Patch> = Vec::new();
    for seed in 0..mesh.tris.len() as u32 {
        if visited[seed as usize] {
            continue;
        }
        let Some(seed_attr) = attribution.lookup(seed) else {
            visited[seed as usize] = true;
            continue;
        };
        let mut queue: VecDeque<u32> = VecDeque::from([seed]);
        let mut tri_indices: Vec<u32> = Vec::new();
        while let Some(t) = queue.pop_front() {
            if visited[t as usize] {
                continue;
            }
            let Some(t_attr) = attribution.lookup(t) else {
                continue;
            };
            if t_attr != seed_attr {
                continue;
            }
            visited[t as usize] = true;
            tri_indices.push(t);
            for &n in &adjacency[t as usize] {
                if !visited[n as usize] {
                    queue.push_back(n);
                }
            }
        }
        patches.push(Patch {
            attribution: seed_attr,
            tri_indices,
        });
    }
    patches
}

/// PR-YR5c helper: recover ALL directed boundary cycles of a patch.
/// Boundary edges = edges in exactly one patch triangle (canonical
/// (min, max) test). Walk each cycle from the lowest remaining
/// start-vertex; follow start→end chain via `BTreeMap` (deterministic).
///
/// A simple face yields 1 cycle; an annulus (holed face) yields 2 (the
/// outer boundary + one hole); etc. Classification of which cycle is
/// outer vs inner happens in `reconstruct_topology`.
///
/// Returns `Err(NonManifoldOutput)` on dead-end or T-junction (a genuine
/// non-manifold patch).
pub(crate) fn patch_boundary_cycle(
    patch: &Patch,
    mesh: &Mesh,
) -> Result<Vec<Vec<(u32, u32)>>, YangError> {
    use std::collections::{BTreeMap, HashSet};

    let patch_set: HashSet<u32> = patch.tri_indices.iter().copied().collect();

    // Precompute edge → tris-in-patch count for O(T) total cost.
    //
    // Spec yang_stage6_sliver_topology §4A (walk robustness): EXCLUDE the
    // patch's FOLD slivers (`patch_fold_slivers`) from BOTH the edge-count
    // preamble and the directed-boundary collection. A fold sliver is a
    // zero-area triangle whose sign-of-zero winding duplicates a real
    // triangle's directed edge (the measured F0016 chord fold); its spurious
    // directed edges would unbalance the walk into a false `NonManifoldOutput`.
    // The fold slivers keep their attribution and stay in the output mesh; they
    // simply carry no boundary of their own. NON-fold degenerate slivers (e.g.
    // a femto-twin membrane welding two coincident vertices, whose edges all
    // pair anti-parallel with real neighbours) are KEPT — excluding them would
    // promote a legitimately-interior real edge to a false boundary and diverge
    // from the reference arrangement. A patch that is ALL fold slivers derives
    // an empty boundary here → the caller's empty-cycles guard raises the loud
    // `NonManifoldOutput` (S5), never a silent degenerate face.
    let excluded_slivers = patch_fold_slivers(patch, mesh);
    let mut patch_edge_count: BTreeMap<(u32, u32), u32> = BTreeMap::new();
    for &t in &patch.tri_indices {
        if excluded_slivers.contains(&t) {
            continue;
        }
        let tri = &mesh.tris[t as usize];
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            let (va, vb) = (tri[i], tri[j]);
            let key = if va < vb { (va, vb) } else { (vb, va) };
            *patch_edge_count.entry(key).or_insert(0) += 1;
        }
    }

    // Collect directed boundary edges in triangle CCW order (fold slivers
    // excluded — see §4A note above). Also record each directed boundary
    // edge's OWNING triangle and the patch's undirected edge→triangle
    // adjacency — the figure-eight wedge walk below needs both.
    let mut directed_boundary: Vec<(u32, u32)> = Vec::new();
    let mut dir_tri: BTreeMap<(u32, u32), u32> = BTreeMap::new();
    let mut edge_tris: BTreeMap<(u32, u32), Vec<u32>> = BTreeMap::new();
    for &t in &patch.tri_indices {
        if excluded_slivers.contains(&t) {
            continue;
        }
        let tri = &mesh.tris[t as usize];
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            let (va, vb) = (tri[i], tri[j]);
            let key = if va < vb { (va, vb) } else { (vb, va) };
            edge_tris.entry(key).or_default().push(t);
            if patch_edge_count.get(&key).copied().unwrap_or(0) == 1 {
                directed_boundary.push((va, vb));
                dir_tri.insert((va, vb), t);
            }
        }
    }
    let _ = patch_set; // patch_set was kept for readability; not needed after precompute

    if directed_boundary.is_empty() {
        return Ok(Vec::new());
    }

    // Build start → ends adjacency (sorted ascending for determinism)
    let mut by_start: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for &(s, e) in &directed_boundary {
        by_start.entry(s).or_default().push(e);
    }
    for ends in by_start.values_mut() {
        ends.sort_unstable();
    }

    let mut cycles: Vec<Vec<(u32, u32)>> = Vec::new();

    // Figure-eight wedge walk (spec `yang_tangency_pinch_split.md` I4, the
    // KV9-F1 union follow-up): at a boundary vertex with SEVERAL outgoing
    // boundary edges (a patch pinched at a mesh-manifold vertex — e.g. the
    // tangency point of C0058's union, where the patch touches the vertex
    // in two opposite wedges), naive lowest-first chaining can join the
    // two lobes into one self-crossing cycle whose Newell cancels. The
    // wedge-correct continuation of an incoming boundary edge is found by
    // rotating through the patch's triangle fan at `current`, starting at
    // the incoming edge's triangle and crossing interior edges, until the
    // wedge's far boundary edge appears. Engaged ONLY at ambiguous vertices
    // (out-degree > 1) — everywhere else the walk is byte-identical.
    let wedge_continuation = |current: u32, prev: u32| -> Result<u32, YangError> {
        let mut t = *dir_tri.get(&(prev, current)).ok_or_else(|| {
            non_manifold_at(
                "s6-wedge-walk-no-owner",
                format_args!("incoming boundary edge ({prev}, {current}) has no owning triangle"),
            )
        })?;
        let mut via = prev;
        let mut hops = 0usize;
        loop {
            hops += 1;
            if hops > patch.tri_indices.len() + 1 {
                return Err(non_manifold_at(
                    "s6-wedge-walk-diverged",
                    format_args!("vertex {current} wedge walk did not terminate"),
                ));
            }
            // The triangle's OTHER current-incident edge (current, x).
            let tri = &mesh.tris[t as usize];
            let Some(&x) = tri.iter().find(|&&u| u != current && u != via) else {
                return Err(non_manifold_at(
                    "s6-wedge-walk-degenerate",
                    format_args!("triangle {t} degenerate at vertex {current}"),
                ));
            };
            let key = if current < x {
                (current, x)
            } else {
                (x, current)
            };
            if patch_edge_count.get(&key).copied().unwrap_or(0) == 1 {
                // The wedge's far boundary edge — the continuation.
                return Ok(x);
            }
            // Interior edge: cross to the wedge's next triangle.
            let Some(pair) = edge_tris.get(&key) else {
                return Err(non_manifold_at(
                    "s6-wedge-walk-missing-edge",
                    format_args!("edge ({current}, {x}) has no adjacency"),
                ));
            };
            let Some(&other) = pair.iter().find(|&&tj| tj != t) else {
                return Err(non_manifold_at(
                    "s6-wedge-walk-open-fan",
                    format_args!("interior edge ({current}, {x}) has one triangle"),
                ));
            };
            t = other;
            via = x;
        }
    };

    // PRIMARY (#169 P3b inc-4a, the R0061 4-strand crossing): extract every
    // cycle as an ORBIT of the wedge-consistent successor map — pair EVERY
    // directed boundary edge (u,v) with its unique continuation (v,w) (the
    // sole outgoing edge at v when v is unambiguous, else the wedge-consistent
    // one), require the map to be a bijection, and take its orbits. No start
    // heuristic; closure is at the EDGE level, so a crossing vertex is
    // traversed once per strand, each along its own wedge. This fixes the
    // legacy chain walk's two crossing defects: a cycle STARTING at a crossing
    // picked its first edge without wedge pairing, and every cycle CLOSED on
    // first return to its start VERTEX even when the arrival strand's wedge
    // continuation was a different lobe — both stitch lobes wrongly at a
    // 4-strand crossing (two section curves crossing at a minted pierce
    // corner), leaving a later cycle's wedge continuation already consumed.
    //
    // FALLBACK: the fan rotation inside `wedge_continuation` is only reliable
    // where every interior edge of the fan has exactly TWO patch triangles. At
    // a tangency generator (the KV9-F1 Steinmetz pair) FOUR sheets share the
    // tangency edge and all four are mutually tangent (first-order dihedral
    // sorting degenerates), so the rotation can emerge in the wrong sheet and
    // the successor map fails to resolve. Until a curvature-aware radial sort
    // exists, an unresolvable patch falls back to the legacy consumption walk
    // (which the KV9-F1 volume oracles validate on those fixtures); a patch
    // that fails BOTH paths keeps the legacy loud error taxonomy.
    let orbit_attempt = || -> Result<Vec<Vec<(u32, u32)>>, YangError> {
        use std::collections::BTreeSet;
        let mut succ: BTreeMap<(u32, u32), (u32, u32)> = BTreeMap::new();
        let mut claimed: BTreeMap<(u32, u32), u32> = BTreeMap::new();
        for &(u, v) in &directed_boundary {
            let outs = by_start.get(&v).map(|o| o.as_slice()).unwrap_or(&[]);
            let w = match outs {
                [] => {
                    // Dead-end / T-junction: a genuine non-manifold patch.
                    if std::env::var_os("NONMANIFOLD_SITE_PROBE").is_some() {
                        eprintln!("[wedge-dump] deadend incoming=({u},{v}) no outgoing");
                        for &(s, e) in &directed_boundary {
                            if s == v || e == v {
                                eprintln!(
                                    "[wedge-dump] dir boundary ({s},{e}) tri {:?}",
                                    dir_tri.get(&(s, e))
                                );
                            }
                        }
                        for &t in &patch.tri_indices {
                            let tri = &mesh.tris[t as usize];
                            if tri.contains(&v) {
                                eprintln!(
                                    "[wedge-dump] patch tri {t} = {tri:?} sliver={} coords {:?}",
                                    excluded_slivers.contains(&t),
                                    tri.iter()
                                        .map(|&vv| mesh.verts[vv as usize])
                                        .collect::<Vec<_>>()
                                );
                            }
                        }
                    }
                    return Err(non_manifold_at(
                        "s6-boundary-walk-deadend",
                        format_args!("vertex {v} has incoming boundary but no outgoing"),
                    ));
                }
                [only] => *only,
                _ => {
                    // Ambiguous crossing: take the wedge-consistent edge.
                    let cont = wedge_continuation(v, u)?;
                    if !outs.contains(&cont) {
                        // EXPERIMENTAL dump (task #121): full local state at
                        // the failed crossing, env-gated with the site probe.
                        if std::env::var_os("NONMANIFOLD_SITE_PROBE").is_some() {
                            eprintln!("[wedge-dump] incoming=({u},{v}) cont={cont} outs={outs:?}");
                            for &(s, e) in &directed_boundary {
                                if s == v || e == v || s == cont || e == cont {
                                    eprintln!(
                                        "[wedge-dump] dir boundary ({s},{e}) tri {:?}",
                                        dir_tri.get(&(s, e))
                                    );
                                }
                            }
                            for &t in &patch.tri_indices {
                                let tri = &mesh.tris[t as usize];
                                if tri.contains(&v) {
                                    eprintln!(
                                        "[wedge-dump] patch tri {t} = {tri:?} sliver={} coords {:?}",
                                        excluded_slivers.contains(&t),
                                        tri.iter()
                                            .map(|&vv| mesh.verts[vv as usize])
                                            .collect::<Vec<_>>()
                                    );
                                }
                            }
                        }
                        return Err(non_manifold_at(
                            "s6-wedge-walk-not-outgoing",
                            format_args!(
                                "vertex {v}: wedge continuation {cont} is not an \
                                 available outgoing boundary edge"
                            ),
                        ));
                    }
                    cont
                }
            };
            succ.insert((u, v), (v, w));
            *claimed.entry((v, w)).or_insert(0) += 1;
        }
        // The successor map must be a bijection: every directed boundary edge
        // is claimed as a continuation exactly once (counts sum to the edge
        // count, so all-once ⇔ every edge claimed). Two strands claiming one
        // outgoing edge is an unresolvable pairing.
        for (&(s, e), &n) in &claimed {
            if n != 1 {
                return Err(non_manifold_at(
                    "s6-wedge-succ-collision",
                    format_args!("edge ({s}, {e}) claimed by {n} strands"),
                ));
            }
        }
        // Orbits partition the boundary; extract each starting at its lowest
        // unvisited edge (deterministic; for a simple boundary this matches
        // the old lowest-start-vertex rotation).
        let mut ordered: Vec<(u32, u32)> = directed_boundary.clone();
        ordered.sort_unstable();
        let mut visited: BTreeSet<(u32, u32)> = BTreeSet::new();
        let mut orbit_cycles: Vec<Vec<(u32, u32)>> = Vec::new();
        for &e0 in &ordered {
            if visited.contains(&e0) {
                continue;
            }
            let mut cycle: Vec<(u32, u32)> = Vec::new();
            let mut e = e0;
            loop {
                visited.insert(e);
                cycle.push(e);
                e = succ[&e];
                if e == e0 {
                    break;
                }
            }
            orbit_cycles.push(cycle);
        }
        Ok(orbit_cycles)
    };
    match orbit_attempt() {
        Ok(orbit_cycles) => return Ok(orbit_cycles),
        Err(orbit_err) => {
            if std::env::var_os("NONMANIFOLD_SITE_PROBE").is_some() {
                eprintln!("[wedge-orbit] unresolvable, legacy fallback: {orbit_err:?}");
            }
        }
    }

    // LEGACY consumption walk (the pre-inc-4a algorithm, byte-identical):
    // while any start vertex still has an outgoing edge, begin a new cycle at
    // the LOWEST such start vertex and chain-walk it, consuming edges as we
    // go, closing on first return to the start vertex.
    let mut remaining = directed_boundary.len();
    while let Some((&start, _)) = by_start.iter().find(|(_, ends)| !ends.is_empty()) {
        let budget = remaining;
        let mut current = start;
        let mut prev: Option<u32> = None;
        let mut cycle: Vec<(u32, u32)> = Vec::new();
        loop {
            let next = {
                let next_vec = by_start.get_mut(&current).ok_or_else(|| {
                    non_manifold_at(
                        "s6-boundary-walk-no-start",
                        format_args!("vertex {current}"),
                    )
                })?;
                if next_vec.is_empty() {
                    // Dead-end / T-junction: a genuine non-manifold patch.
                    return Err(non_manifold_at(
                        "s6-boundary-walk-deadend",
                        format_args!("vertex {current} cycle so far {}", cycle.len()),
                    ));
                }
                if next_vec.len() == 1 {
                    next_vec.remove(0)
                } else if let Some(p) = prev {
                    // Ambiguous crossing: take the wedge-consistent edge.
                    let cont = wedge_continuation(current, p)?;
                    let Some(pos) = next_vec.iter().position(|&e| e == cont) else {
                        return Err(non_manifold_at(
                            "s6-wedge-walk-not-outgoing",
                            format_args!(
                                "vertex {current}: wedge continuation {cont} is not an \
                                 available outgoing boundary edge"
                            ),
                        ));
                    };
                    next_vec.remove(pos)
                } else {
                    // Cycle START at a crossing: no incoming edge yet — take
                    // the lowest (deterministic).
                    next_vec.remove(0)
                }
            };
            cycle.push((current, next));
            remaining -= 1;
            prev = Some(current);
            current = next;
            if current == start {
                break;
            }
            // Per-cycle safety: a single cycle cannot be longer than the
            // edges that remained when it started (else the walk escaped).
            if cycle.len() > budget {
                return Err(non_manifold_at(
                    "s6-boundary-walk-escaped",
                    format_args!("start {start} budget {budget}"),
                ));
            }
        }
        cycles.push(cycle);
    }

    Ok(cycles)
}

/// A mesh triangle is a degenerate (zero-area) sliver when twice its area
/// `‖(p1−p0)×(p2−p0)‖` falls below `MIN_FEATURE_SIZE²` — the SAME shared
/// threshold the Stage-6 attribution degenerate branch uses (governance A14.3,
/// no ad-hoc epsilon). The exact arrangement keeps such slivers along shared
/// collinear solid edges for watertightness; spec `yang_stage6_sliver_topology`
/// §4A excludes them from boundary derivation.
pub(crate) fn triangle_is_degenerate(mesh: &Mesh, t: u32) -> bool {
    let tri = &mesh.tris[t as usize];
    let p0 = mesh.verts[tri[0] as usize].as_array();
    let p1 = mesh.verts[tri[1] as usize].as_array();
    let p2 = mesh.verts[tri[2] as usize].as_array();
    let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
    let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
    let cross = [
        e1[1] * e2[2] - e1[2] * e2[1],
        e1[2] * e2[0] - e1[0] * e2[2],
        e1[0] * e2[1] - e1[1] * e2[0],
    ];
    let twice_area = (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt();
    twice_area < cad_primitives::MIN_FEATURE_SIZE * cad_primitives::MIN_FEATURE_SIZE
}

/// The patch's FOLD slivers — the degenerate zero-area triangles that spec
/// `yang_stage6_sliver_topology` §4A excludes from boundary derivation.
///
/// A fold sliver has at least one DIRECTED edge `(a→b)` that coincides,
/// SAME-direction, with a directed edge of another triangle in the patch
/// (directed multiplicity ≥ 2). That is the measured F0016 signature: a
/// zero-area shim whose sign-of-zero winding duplicates the real triangle's
/// chord edge, unbalancing the boundary walk into a false `NonManifoldOutput`.
///
/// A degenerate sliver whose edges instead pair ANTI-parallel with their
/// neighbours — e.g. a femto-twin membrane welding two coincident vertices,
/// where every directed edge is unique — is NOT a fold. Such a sliver carries a
/// legitimate (if zero-length) boundary; excluding it would drop a real
/// neighbour edge from interior to boundary and diverge from the reference
/// arrangement (curved / twin parity). So it is KEPT.
pub(crate) fn patch_fold_slivers(patch: &Patch, mesh: &Mesh) -> std::collections::HashSet<u32> {
    use std::collections::{HashMap, HashSet};
    // Directed edge multiplicity over ALL patch triangles (real + degenerate).
    let mut dir_count: HashMap<(u32, u32), u32> = HashMap::new();
    for &t in &patch.tri_indices {
        let tri = &mesh.tris[t as usize];
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            *dir_count.entry((tri[i], tri[j])).or_insert(0) += 1;
        }
    }
    let mut folds: HashSet<u32> = HashSet::new();
    for &t in &patch.tri_indices {
        if !triangle_is_degenerate(mesh, t) {
            continue;
        }
        let tri = &mesh.tris[t as usize];
        let is_fold = [(0usize, 1usize), (1, 2), (2, 0)]
            .iter()
            .any(|&(i, j)| dir_count.get(&(tri[i], tri[j])).copied().unwrap_or(0) >= 2);
        if is_fold {
            folds.insert(t);
        }
    }
    folds
}

/// Exact 3D on-open-segment test for the Stage-6 loop T-subdivision (spec
/// `yang_stage6_sliver_topology` §4B). Returns `Some(t_num)` — the exact
/// numerator of the segment parameter `t = ((v−a)·(b−a)) / |b−a|²` — when `v`
/// lies STRICTLY between `a` and `b` (exactly collinear AND `0 < t < 1`);
/// `None` otherwise. All candidate vertices on one edge share the `|b−a|²`
/// denominator, so `t_num` alone orders them. Pure rational (`f64 → RBig` is
/// lossless); no tolerance — a vertex only ULP-near the segment does NOT split
/// it (that residue class stays loud downstream, spec §5 S6).
pub(crate) fn on_open_segment_param(
    a: [f64; 3],
    b: [f64; 3],
    v: [f64; 3],
) -> Option<dashu::rational::RBig> {
    use crate::coplanar_overlay::rat;
    use dashu::rational::RBig;
    let r = |x: f64| rat(x).ok();
    let (ax, ay, az) = (r(a[0])?, r(a[1])?, r(a[2])?);
    let (bx, by, bz) = (r(b[0])?, r(b[1])?, r(b[2])?);
    let (vx, vy, vz) = (r(v[0])?, r(v[1])?, r(v[2])?);
    let (abx, aby, abz) = (&bx - &ax, &by - &ay, &bz - &az);
    let (dax, day, daz) = (&vx - &ax, &vy - &ay, &vz - &az);
    // Exactly collinear: cross(ab, da) == 0 in all three components.
    let cx = &aby * &daz - &abz * &day;
    let cy = &abz * &dax - &abx * &daz;
    let cz = &abx * &day - &aby * &dax;
    if cx != RBig::ZERO || cy != RBig::ZERO || cz != RBig::ZERO {
        return None;
    }
    let t_num = &dax * &abx + &day * &aby + &daz * &abz;
    let len2 = &abx * &abx + &aby * &aby + &abz * &abz;
    // Strict betweenness: 0 < t_num < len2 (len2 > 0 for a real edge; a
    // degenerate zero-length edge yields len2 == 0 and no split).
    if t_num > RBig::ZERO && t_num < len2 {
        Some(t_num)
    } else {
        None
    }
}

/// Stage-6 loop T-subdivision (spec `yang_stage6_sliver_topology` §4B). After
/// §4A excludes degenerate slivers from boundary derivation, a face whose patch
/// carried a whole shared solid edge as ONE chord `(a,b)` must split that chord
/// at every output vertex that (i) lies STRICTLY on segment `a–b` (exact
/// rational collinearity + betweenness) AND (ii) is used by SOME OTHER output
/// loop. The other side of the shared edge subdivides it at those same
/// vertices, so after the split every segment of the solid edge is used by
/// exactly two directed loop edges — the 2-manifold seam kernel-v2's
/// edge-pairing check demands. Self-pairs within one weakly-simple loop are
/// legitimate (matching the existing kernel-v2 self-pair handling).
///
/// Returns per-`info` subdivided cycles (same outer shape as `info.cycles`).
/// Determinism: split vertices ordered by exact segment parameter, ties by
/// vertex index. A no-op (byte-identical cycles) when no loop edge has an
/// on-segment foreign vertex (spec §5 S3), so non-sliver output is unaffected.
pub(crate) fn subdivide_loops_at_shared_vertices(
    infos: &[PatchInfo],
    mesh: &Mesh,
) -> Vec<Vec<Vec<(u32, u32)>>> {
    use dashu::rational::RBig;
    use std::collections::BTreeMap;

    // (1) Assign a global loop id to every (info, cycle); record which loop ids
    // use each vertex. A vertex used repeatedly within ONE loop counts that
    // loop once (dedup) so the "used by some OTHER loop" test is exact.
    let mut next_loop = 0usize;
    let mut cycle_loop_ids: Vec<Vec<usize>> = Vec::with_capacity(infos.len());
    let mut vertex_loops: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    for info in infos {
        let mut ids = Vec::with_capacity(info.cycles.len());
        for cycle in &info.cycles {
            for &(s, _e) in cycle {
                vertex_loops.entry(s).or_default().push(next_loop);
            }
            ids.push(next_loop);
            next_loop += 1;
        }
        cycle_loop_ids.push(ids);
    }
    for ids in vertex_loops.values_mut() {
        ids.sort_unstable();
        ids.dedup();
    }

    // (2) Split each loop edge at its foreign on-segment vertices — but ONLY
    // for patches that had a degenerate sliver excluded (spec §2/§4B). A patch
    // with no excluded sliver keeps byte-identical loops: the measured
    // un-subdivided chord lives on the sliver-bearing side, and subdividing a
    // benign T-junction on a clean curved/planar patch would diverge from the
    // C++ reference arrangement (which does not carry that vertex on the edge),
    // breaking reference parity. The vertex-loop map above still spans ALL
    // loops, so the sliver side splits at the foreign (fine-side) vertices.
    // Diagnosis probe (read-only, env-gated): report every loop edge with a
    // foreign vertex that is ULP-NEAR the open segment (f64 perpendicular
    // distance < 1e-9) but NOT exactly collinear — the spec §5 S6 residue —
    // plus whether the owning patch passes the fold-sliver gate at all.
    if std::env::var_os("YANG_S6_SPLIT_PROBE").is_some() {
        for (ii, info) in infos.iter().enumerate() {
            for (ci, cycle) in info.cycles.iter().enumerate() {
                let lid = cycle_loop_ids[ii][ci];
                for &(s, e) in cycle {
                    let pa = mesh.verts[s as usize].as_array();
                    let pb = mesh.verts[e as usize].as_array();
                    let ab = [pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]];
                    let len2 = ab[0] * ab[0] + ab[1] * ab[1] + ab[2] * ab[2];
                    if len2 == 0.0 {
                        continue;
                    }
                    for (&v, luse) in &vertex_loops {
                        if v == s || v == e || !luse.iter().any(|&l| l != lid) {
                            continue;
                        }
                        let pv = mesh.verts[v as usize].as_array();
                        let av = [pv[0] - pa[0], pv[1] - pa[1], pv[2] - pa[2]];
                        let t = (av[0] * ab[0] + av[1] * ab[1] + av[2] * ab[2]) / len2;
                        if t <= 0.0 || t >= 1.0 {
                            continue;
                        }
                        let proj = [pa[0] + t * ab[0], pa[1] + t * ab[1], pa[2] + t * ab[2]];
                        let d = [pv[0] - proj[0], pv[1] - proj[1], pv[2] - proj[2]];
                        let dist = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
                        if dist < 1e-9 && on_open_segment_param(pa, pb, pv).is_none() {
                            eprintln!(
                                "[s6-split-probe] info {ii} (face_idx {}) cycle {ci} edge ({s},{e}) \
                                 near-vertex {v} dist={dist:e} t={t:.6} gate(had_fold_sliver)={}",
                                info.face_idx, info.had_fold_sliver
                            );
                        }
                    }
                }
            }
        }
    }
    // Companion probe: dump every mesh triangle touching a comma-separated
    // vertex list (env `YANG_S6_VERT_PROBE=842,843,845`) with its area and
    // which info/cycle (if any) carries each directed edge — to see whether
    // the mesh is conformal at a suspect femto-twin site.
    if let Some(list) = std::env::var_os("YANG_S6_VERT_PROBE") {
        let want: std::collections::BTreeSet<u32> = list
            .to_string_lossy()
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();
        for (t, tri) in mesh.tris.iter().enumerate() {
            if !tri.iter().any(|v| want.contains(v)) {
                continue;
            }
            let deg = triangle_is_degenerate(mesh, t as u32);
            eprintln!(
                "[s6-vert-probe] tri {t} ({},{},{}) degenerate={deg}",
                tri[0], tri[1], tri[2]
            );
        }
        for (ii, info) in infos.iter().enumerate() {
            for (ci, cycle) in info.cycles.iter().enumerate() {
                for &(s, e) in cycle {
                    if want.contains(&s) || want.contains(&e) {
                        eprintln!(
                            "[s6-vert-probe] info {ii} face_idx {} cycle {ci} edge {s}->{e}",
                            info.face_idx
                        );
                    }
                }
            }
        }
    }
    // Spec amendment 1 (S7): undirected segment-use census over ALL loops.
    // A loop edge with use-count 1 is CERTAIN to fail kernel-v2's manifold
    // edge pairing — the S7 split can therefore never alter a passing
    // output (every valid output uses each undirected segment exactly
    // twice), preserving reference parity structurally rather than via the
    // fold-sliver scope.
    let mut seg_use: BTreeMap<(u32, u32), usize> = BTreeMap::new();
    for info in infos {
        for cycle in &info.cycles {
            for &(s, e) in cycle {
                *seg_use.entry((s.min(e), s.max(e))).or_default() += 1;
            }
        }
    }

    let mut out: Vec<Vec<Vec<(u32, u32)>>> = Vec::with_capacity(infos.len());
    for (ii, info) in infos.iter().enumerate() {
        if !info.had_fold_sliver {
            // S7 (spec `yang_stage6_sliver_topology` amendment 1): the
            // certainly-fatal chord repair. Split a use-count-1 loop edge
            // (a,b) at a foreign vertex v strictly inside it (0<t<1, within
            // TAU_WORK of the open segment — the spec §4 "band for the
            // last-ulp case"; F0079's site is f64-dist 0.0 but sub-ULP off
            // the exact segment) when BOTH complementary sub-segments (a,v)
            // and (v,b) are walked by some loop. Any currently-valid output
            // has use == 2 everywhere → byte-identical (S1/S3).
            let mut info_cycles: Vec<Vec<(u32, u32)>> = Vec::with_capacity(info.cycles.len());
            for (ci, cycle) in info.cycles.iter().enumerate() {
                let lid = cycle_loop_ids[ii][ci];
                let mut new_cycle: Vec<(u32, u32)> = Vec::with_capacity(cycle.len());
                let mut inserted: std::collections::BTreeSet<(u32, u32)> =
                    std::collections::BTreeSet::new();
                for &(s, e) in cycle {
                    if seg_use.get(&(s.min(e), s.max(e))).copied() != Some(1) {
                        new_cycle.push((s, e));
                        continue;
                    }
                    let pa = mesh.verts[s as usize].as_array();
                    let pb = mesh.verts[e as usize].as_array();
                    let ab = [pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]];
                    let len2 = ab[0] * ab[0] + ab[1] * ab[1] + ab[2] * ab[2];
                    if len2 == 0.0 {
                        new_cycle.push((s, e));
                        continue;
                    }
                    let mut splits: Vec<(f64, u32)> = Vec::new();
                    for (&v, luse) in &vertex_loops {
                        if v == s || v == e || !luse.iter().any(|&l| l != lid) {
                            continue;
                        }
                        if seg_use.get(&(s.min(v), s.max(v))).copied().unwrap_or(0) == 0
                            || seg_use.get(&(e.min(v), e.max(v))).copied().unwrap_or(0) == 0
                        {
                            continue;
                        }
                        let pv = mesh.verts[v as usize].as_array();
                        let av = [pv[0] - pa[0], pv[1] - pa[1], pv[2] - pa[2]];
                        let t = (av[0] * ab[0] + av[1] * ab[1] + av[2] * ab[2]) / len2;
                        if t <= 0.0 || t >= 1.0 {
                            continue;
                        }
                        let proj = [pa[0] + t * ab[0], pa[1] + t * ab[1], pa[2] + t * ab[2]];
                        let d = [pv[0] - proj[0], pv[1] - proj[1], pv[2] - proj[2]];
                        let dist2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
                        if dist2 <= cad_primitives::TAU_WORK * cad_primitives::TAU_WORK {
                            splits.push((t, v));
                        }
                    }
                    splits.sort_by(|x, y| {
                        x.0.partial_cmp(&y.0)
                            .unwrap_or(std::cmp::Ordering::Equal)
                            .then(x.1.cmp(&y.1))
                    });
                    let mut prev = s;
                    for (_, v) in splits {
                        inserted.insert((prev.min(v), prev.max(v)));
                        new_cycle.push((prev, v));
                        prev = v;
                    }
                    if prev != s {
                        inserted.insert((prev.min(e), prev.max(e)));
                    }
                    new_cycle.push((prev, e));
                }
                // Amendment 1a: cancel null excursions (adjacent inverse
                // directed pairs, wrap-around included) in which at least one
                // member is a split-inserted segment — a spur made
                // self-pairing by the split is a zero-width slit that leaves
                // χ odd (E+1, no face). Restricting to split-inserted members
                // keeps non-S7 loops byte-identical and legitimate bigons
                // untouched. Iterate to a fixed point (cancellation can make
                // a new pair adjacent).
                if !inserted.is_empty() {
                    loop {
                        let m = new_cycle.len();
                        let mut cancelled = false;
                        'scan: for i in 0..m {
                            let j = (i + 1) % m;
                            if m < 2 {
                                break;
                            }
                            let (a1, b1) = new_cycle[i];
                            let (a2, b2) = new_cycle[j];
                            if a1 == b2
                                && b1 == a2
                                && (inserted.contains(&(a1.min(b1), a1.max(b1))))
                            {
                                let (hi, lo) = if i < j { (j, i) } else { (i, j) };
                                new_cycle.remove(hi);
                                new_cycle.remove(lo);
                                cancelled = true;
                                break 'scan;
                            }
                        }
                        if !cancelled {
                            break;
                        }
                    }
                }
                info_cycles.push(new_cycle);
            }
            out.push(info_cycles);
            continue;
        }
        let mut info_cycles: Vec<Vec<(u32, u32)>> = Vec::with_capacity(info.cycles.len());
        for (ci, cycle) in info.cycles.iter().enumerate() {
            let lid = cycle_loop_ids[ii][ci];
            let mut new_cycle: Vec<(u32, u32)> = Vec::with_capacity(cycle.len());
            for &(s, e) in cycle {
                let pa = mesh.verts[s as usize].as_array();
                let pb = mesh.verts[e as usize].as_array();
                let mut splits: Vec<(RBig, u32)> = Vec::new();
                for (&v, luse) in &vertex_loops {
                    if v == s || v == e {
                        continue;
                    }
                    // Must be used by a loop OTHER than this one.
                    if !luse.iter().any(|&l| l != lid) {
                        continue;
                    }
                    let pv = mesh.verts[v as usize].as_array();
                    if let Some(t_num) = on_open_segment_param(pa, pb, pv) {
                        splits.push((t_num, v));
                    }
                }
                splits.sort_by(|x, y| x.0.cmp(&y.0).then(x.1.cmp(&y.1)));
                let mut prev = s;
                for (_, v) in splits {
                    new_cycle.push((prev, v));
                    prev = v;
                }
                new_cycle.push((prev, e));
            }
            info_cycles.push(new_cycle);
        }
        out.push(info_cycles);
    }
    out
}
