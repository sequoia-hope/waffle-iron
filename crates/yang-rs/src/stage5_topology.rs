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
        || fold_merge_enabled()
}

/// Is the §4.4.1 Fig-11 merge pass ([`run_fold_merge_passes`]) on?
///
/// **ALWAYS-ON since the I6 flip (2026-08-19d, spec §4-I6);
/// `YANG_441_FOLD_MERGE=0|off` is the dev A/B off-knob.** Flip bar: gate-off
/// byte-identical by construction (every line of the pass is inside this
/// predicate), the rewrite tier green with the pass on, and a gate-ON corpus of
/// 265C/0W/43E/1EE/0T — two honest conversions (F0045, R0090) and zero other
/// category or detail deltas.
///
/// Its selector reads [`S4_PRE_POS`], so this is a THIRD consumer of
/// [`s4_pre_pos_enabled`] — and the first non-diagnostic one. The map is
/// production input to a repair here, not a probe column, which is why the
/// capture and all four re-keys must follow the same predicate.
fn fold_merge_enabled() -> bool {
    !matches!(std::env::var("YANG_441_FOLD_MERGE"), Ok(v) if v == "0" || v == "off")
}

/// Is the §4-I8 FAN-OF-ONE rebuild on — the victim carrying a SINGLE triangle
/// in a holder patch, where the merge DELETES that triangle rather than
/// re-triangulating a 2-vertex link?
///
/// **ALWAYS-ON since 2026-08-20** — `YANG_441_FAN_OF_ONE=0|off` is the dev A/B
/// off-knob, which restores the pre-flip loud refusal. Flip bar: the full corpus
/// with this and [`merge_carrier_guard_enabled`] on measured BYTE-IDENTICAL to
/// the banked 265C/0W/43E/1EE/0T — zero category and zero detail deltas over all
/// 312 cases — so the repair costs nothing and stops declining a configuration
/// whose answer is known.
fn fan_of_one_enabled() -> bool {
    !matches!(std::env::var("YANG_441_FAN_OF_ONE"), Ok(v) if v == "0" || v == "off")
}

/// Is the §4-I8 carrier-containment precondition on the Fig-11 merge enforced?
///
/// **ALWAYS-ON since 2026-08-20** — `YANG_441_MERGE_CARRIER=0|off` is the dev
/// A/B off-knob. Flip bar: byte-identical corpus (see [`fan_of_one_enabled`]),
/// and the guard is a P10 safety net — gated off it protects nothing. It refuses
/// 7 sites over R0011/R0044/R0074/R0085 that today decline only by accident (a
/// small holder happens not to contain the survivor) and would otherwise merge a
/// model CORNER into a curve junction, and it refuses ZERO of the merges F0045
/// and R0090 rely on.
fn merge_carrier_guard_enabled() -> bool {
    !matches!(std::env::var("YANG_441_MERGE_CARRIER"), Ok(v) if v == "0" || v == "off")
}

/// Is the pre-position map wanted for REPORTING (as opposed to being wanted at
/// all)? The map's `YANG_S5_MOVED_SET` / `YANG_S5_REMAP` lines are probe output;
/// the Fig-11 merge needs the map itself but not the chatter, so it must not
/// turn a repair pass into a per-boolean stderr writer.
fn s4_pre_pos_diagnostic() -> bool {
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
            if s4_pre_pos_diagnostic() {
                eprintln!(
                    "YANG_S5_REMAP site={site} kept={} dropped={} (pre-position map re-keyed)",
                    new.len(),
                    before - new.len(),
                );
            }
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
    let (vertices, edges, faces, _sources, _face_attr) = emit_topology(
        mesh,
        &infos,
        &intersection_curves,
        &[],
        BoolOp::Union,
        (&a.faces, &a.edges),
        (&b.faces, &b.edges),
    )?;
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

/// §4.4.1 AS WRITTEN, increment I1b (spec `specs/yang_441_trim_cdt_construction.md`
/// §4): the UNCONDITIONAL curve-seam construction, per PATCH with ALL its
/// curves.
///
/// ALWAYS-ON since the I3 flip (2026-08-15) — the caller invokes this
/// unconditionally; the historical `YANG_441_CONSTRUCT` env gate now only
/// re-enables the diagnostic chatter (see [`c441_verbose`]). I1's
/// one-seam-per-pass slice measured sound with ZERO
/// conversions: mutually-blocked seams (a collapsed seam still crosses the
/// other not-yet-collapsed relocated chains of the same cycle) can never
/// collapse pairwise — the fixpoint decline census
/// (`SelfIntersectingPolyline` ×500 on F0067) named the paper's own plural,
/// "we trim and update the meshes using the intersection curveS".
///
/// Each pass now: enumerates every seam (`stage4_construct::seam_groups` —
/// no defect detector), collects the ELIGIBLE ones (LineSegment curve, both
/// owner patches planar, open orderable chain, contiguous in both owners'
/// cycles), collapses ALL of each patch's eligible runs simultaneously
/// (`collapse_patch_runs`), re-triangulates each modified patch SINGLE-SIDED
/// (`rebuild_patch_planar` — after collapse the seams are ordinary boundary
/// edges; a plain tolerance-free CDT of the cycle polygon, dropped chain
/// vertices become planar interior and are discarded, the paper's collinear
/// "remove a mesh vertex" case), and writes the whole batch back in one pass
/// (`apply_rebuild_batch`). Conformality is by construction: a collapsed
/// seam is the SAME `(e0, e1)` vertex pair on both owner patches, and every
/// untouched boundary chain is reproduced edge-for-edge by the CDT.
///
/// A seam collapses only if BOTH owners rebuild in the same batch — any
/// refusal (mid-batch non-contiguity, degenerate cycle, a dropped vertex
/// still referenced outside the batch or kept by another batched patch, CDT
/// decline) removes the responsible seam (or the whole patch's seams) from
/// the batch and re-assembles, loudly. Out-of-scope seams (curved patch,
/// non-line curve, closed run) are LOUD skips — increment I2's worklist,
/// never a silent partial repair.
/// Diagnostic verbosity for the §4.4.1 construct pass (I3 flip, 2026-08-15).
///
/// The pass itself is ALWAYS-ON; only its per-seam/per-pass diagnostic
/// chatter is gated here. Setting the HISTORICAL main gate
/// `YANG_441_CONSTRUCT` (every recorded spec workflow does) or
/// `YANG_441_VERBOSE` reproduces the pre-flip output byte-for-byte; the
/// default (no env — including every wasm32 run, where `var_os` is always
/// `None`) is quiet. Genuine anomaly signals — the whole-batch-refusal /
/// correspondence / write-back STOPs — stay unconditional `eprintln!`s.
pub(crate) fn c441_verbose() -> bool {
    std::env::var_os("YANG_441_CONSTRUCT").is_some()
        || std::env::var_os("YANG_441_VERBOSE").is_some()
}

/// `eprintln!` gated on [`c441_verbose`] — the construct pass's diagnostic
/// chatter (SKIP/DECLINED/APPLIED/REORDERED/census lines).
macro_rules! c441_log {
    ($($arg:tt)*) => {
        if c441_verbose() {
            eprintln!($($arg)*);
        }
    };
}

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
/// The 2026-08-05 trial (`YANG_S4_FIG11_MERGE`, kept gated off in
/// [`crate::stage4_fold_risk`]) applied the same idea with `collapse_vertex` and
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
/// - Gate `YANG_441_FOLD_MERGE`. Gate-off does not read or write anything.
fn run_fold_merge_passes(
    mesh: &mut Mesh,
    attribution: &mut TriangleAttributionMap,
    a: &BRep,
    b: &BRep,
    infos: &mut Vec<crate::stage4_correct::PatchInfo>,
    intersection_curves: &mut std::collections::BTreeMap<(u32, u32), Curve>,
    relocations: &mut Vec<(u32, f64)>,
) -> Result<usize, YangError> {
    use crate::stage4_construct::{apply_rebuild_batch, rebuild_merge_fan};
    use crate::stage4_fold_risk::fold_merge_sites_censused;
    use crate::stage4_splice::SplicePatch;
    use std::collections::{BTreeMap, BTreeSet};

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
        if merge_carrier_guard_enabled() {
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
                let off =
                    carrier_lost_by_merge(mesh, attribution, a, b, site.victim, site.survivor);
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
        }
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
                match rebuild_merge_fan(
                    mesh,
                    h,
                    &patches[h],
                    site.victim,
                    site.survivor,
                    fan_of_one_enabled(),
                ) {
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
        match apply_rebuild_batch(mesh, attribution, &rebuilds, &BTreeMap::new()) {
            Ok(()) => {
                // §I13(f) f2: the re-homed corner's mint position — the fans
                // in `rebuilds` were planned against it, so it lands only
                // with them (a refused batch leaves the mesh untouched).
                if let Some((v, p)) = rehome_reloc {
                    mesh.verts[v as usize] = p;
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
                break;
            }
        }
        let remap = compact_unreferenced_verts(mesh, relocations);
        let (i2, inc2, cv2) = compute_phase_a(
            mesh,
            attribution,
            a,
            b,
            &crate::stage3_ssi::NO_EDGE_PROVENANCE,
        )?;
        probe_remap_pre_pos("fold-merge", remap.as_ref());
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
fn probe_merge_site(
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
type AbsorptionPlan = (
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
fn run_absorption_attempt(
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
    if mode == RunAbsorbMode::Off {
        return None;
    }
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
    if gmode == GroupAbsorbMode::Off {
        return None;
    }
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
fn rehome_attempt(
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
    if mode == RehomeMode::Off || cands.is_empty() {
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
fn probe_run_neighborhood(
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
fn carrier_lost_by_merge(
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
fn mesh_positions(mesh: &Mesh) -> Vec<[f64; 3]> {
    mesh.verts.iter().map(Point3::as_array).collect()
}

fn run_construct_passes(
    mesh: &mut Mesh,
    attribution: &mut TriangleAttributionMap,
    a: &BRep,
    b: &BRep,
    infos: &mut Vec<crate::stage4_correct::PatchInfo>,
    intersection_curves: &mut std::collections::BTreeMap<(u32, u32), Curve>,
    relocations: &mut Vec<(u32, f64)>,
) -> Result<usize, YangError> {
    use crate::stage4_construct::{
        apply_rebuild_batch, rebuild_patch_planar, replace_seam_run, seam_groups,
    };
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
    // ALWAYS-ON since the I5-2 flip (2026-08-19, spec §4-I5-2);
    // `YANG_434_INSERT=0|off` is the dev A/B off-knob (the s434
    // instruments' gate-off legs).
    let insert_enabled =
        !matches!(std::env::var("YANG_434_INSERT"), Ok(v) if v == "0" || v == "off");
    // I5-1 orphan floor: appended on-curve vertices below this index are
    // referenced by applied rebuilds (or predate the pass); anything above
    // it at an exit path was appended for a seam whose batch never applied
    // and is truncated (appends are always a contiguous unreferenced tail
    // until a batch applies; the floor is re-snapshotted after each applied
    // batch's compact). Inert when the insert gate is off.
    let mut verts_floor = mesh.verts.len();

    // Bisection probes (env-gated, deterministic): cap which BOOLEANS may
    // apply (process-order index) and the TOTAL applied-seam budget. For
    // localizing a gate-ON regression to one boolean / one seam prefix;
    // census still runs in full either way.
    static BOOL_INDEX: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let bool_idx = BOOL_INDEX.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let env_cap =
        |name: &str| -> Option<usize> { std::env::var(name).ok().and_then(|v| v.parse().ok()) };
    let apply_enabled = env_cap("YANG_441_APPLY_BOOL_CAP").is_none_or(|c| bool_idx < c);
    let seam_budget = env_cap("YANG_441_APPLY_SEAM_CAP");
    if !apply_enabled {
        c441_log!("[s4-construct] CENSUS-ONLY (boolean index {bool_idx} at/above bool cap)");
    }

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
            // I5-1 (gated `YANG_434_INSERT`): the §4.3.4-refined chain —
            // `ordered` with the paper's midpoint samples interleaved as
            // freshly appended on-curve vertices, shared by both owners.
            RefineConic {
                ordered: Vec<u32>,
                refined: Vec<u32>,
            },
        }
        struct EligibleSeam {
            gi: usize,
            pair: (usize, usize),
            chain: Vec<u32>,
            action: SeamAction,
        }
        let mut eligible: Vec<EligibleSeam> = Vec::new();
        for (gi, g) in groups.iter().enumerate() {
            let (pi, qi) = g.pair;
            // I2a: Plane AND Cylinder owners rebuild single-sided (interior
            // carry + θ-unwrap); I13a adds Cone behind `YANG_441_CONE_CHART`;
            // Sphere/Torus stay a loud skip.
            let chartable = |s: &Surface| crate::stage4_project::SurfaceChart::supports(s);
            if !chartable(&patches[pi].surface) || !chartable(&patches[qi].surface) {
                skip[1] += 1;
                c441_log!(
                    "[s4-construct] pass={pass} seam={gi}: SKIP unchartable patch \
                     (patches {pi}+{qi}) — I2 scope"
                );
                continue;
            }
            let (chain, closed) = match ordered_seam_side(&patches[pi].cycles, &g.edges, Side::A) {
                Ok(x) => x,
                Err(e) => {
                    skip[4] += 1;
                    c441_log!(
                        "[s4-construct] pass={pass} seam={gi}: SKIP unorderable chain \
                         (patches {pi}+{qi}) — {e:?}"
                    );
                    continue;
                }
            };
            if closed {
                skip[2] += 1;
                c441_log!(
                    "[s4-construct] pass={pass} seam={gi}: SKIP closed seam \
                     (patches {pi}+{qi}) — I2b tail",
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
                             (patches {pi}+{qi}, off-line {s:?}) — not one line's seam",
                        );
                        continue;
                    }
                }
                if replace_seam_run(&patches[pi].cycles, &chain).is_none()
                    || replace_seam_run(&patches[qi].cycles, &chain).is_none()
                {
                    skip[5] += 1;
                    c441_log!(
                        "[s4-construct] pass={pass} seam={gi}: SKIP — run not contiguous \
                         (patches {pi}+{qi})"
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
                             (patches {pi}+{qi}, {} edges) — no closed-form parameter",
                            g.edges.len()
                        );
                        continue;
                    }
                    Err(e) => {
                        skip[6] += 1;
                        c441_log!(
                            "[s4-construct] pass={pass} seam={gi}: DECLINED conic order \
                             (patches {pi}+{qi}) — {e:?}"
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
                let refinement = if insert_enabled {
                    match crate::stage4_construct::refine_conic_chain(
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
                                 (patches {pi}+{qi}, {} verts — no param or over the \
                                 {I5_INSERT_CAP} insert cap) — reorder only",
                                ordered.len()
                            );
                            None
                        }
                    }
                } else {
                    None
                };
                if let Some((pts, refined)) = refinement {
                    if !carries(&patches[pi].cycles) || !carries(&patches[qi].cycles) {
                        skip[5] += 1;
                        c441_log!(
                            "[s4-construct] pass={pass} seam={gi}: SKIP — conic run not \
                             carried whole (patches {pi}+{qi})"
                        );
                        continue;
                    }
                    c441_log!(
                        "[s4-construct] pass={pass} seam={gi}: REFINE patches {pi}+{qi} — \
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
                    if !carries(&patches[pi].cycles) || !carries(&patches[qi].cycles) {
                        skip[5] += 1;
                        c441_log!(
                            "[s4-construct] pass={pass} seam={gi}: SKIP — conic run not \
                             carried whole (patches {pi}+{qi})"
                        );
                        continue;
                    }
                    SeamAction::ReorderConic { ordered }
                }
            };
            eligible.push(EligibleSeam {
                gi,
                pair: (pi, qi),
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
                    ordered_seam_side(&patches[g.pair.0].cycles, &g.edges, Side::A)
                {
                    if chain.len() >= 2 {
                        census_j1_boundary_exit(
                            mesh,
                            &patches,
                            attribution,
                            intersection_curves,
                            gi,
                            g.pair,
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
                    match ordered_seam_side(&patches[g.pair.0].cycles, &g.edges, Side::A) {
                        Ok(x) => x,
                        Err(e) => {
                            eprintln!(
                                "[s434-census] seam={gi} pair={:?} {curve_desc}: \
                                 UNORDERABLE ({e:?})",
                                g.pair
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
                            "[s434-census] seam={gi} pair={:?} {curve_desc}: \
                             NO-CURVE-ORDER ({e:?}) n={}",
                            g.pair,
                            chain.len()
                        );
                        continue;
                    }
                };
                match census_conic_seam_density(&mesh.verts, &ordered, &g.curve, closed) {
                    Some(c) => eprintln!(
                        "[s434-census] seam={gi} pair={:?} {curve_desc} closed={closed} \
                         n={} pairs={} fail_h={} fail_l={} fail_alpha={} fail_any={} \
                         max_h={:.3e} max_l={:.3e} max_alpha_deg={:.2} dp={:.3e} \
                         implied_inserts={}{}",
                        g.pair,
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
                        "[s434-census] seam={gi} pair={:?} {curve_desc}: \
                         NO-PARAM (degenerate projection or n<2, n={})",
                        g.pair,
                        ordered.len()
                    ),
                }
            }
        }

        // ---- J1-1 (sub-gated `YANG_441_BOUNDARY_EXIT`, spec §4-J1):
        // boundary-exit junction authority. Fig-11(a) computes q ON the kept
        // boundary — but the junction relocation keeps a line-seam terminal
        // J at the exact UNBOUNDED curve×curve junction, which can land
        // BEYOND the point where the seam exits the kept face (F0067: the
        // rim junction at r = 0.208846 vs the wall's designed corner at
        // r = 0.207507 — a 1.34e-3 DESIGN gap with zero kept content). The
        // signature is exact: an UNRELOCATED input-outline corner C
        // (≥3 holders) lying on the seam carrier strictly between J and the
        // seam's first sample. The authority change is the shared-index
        // substitution J -> C through the SAME holder closure as the corner
        // merge (every fused patch of J rebuilds; the circle chain sheds
        // its terminal chord and passes the corner cleanly — the exact
        // topology has the two curves DISJOINT). Selector guards, all loud:
        // straightness identity (one line only — the R0095 corner-cutting
        // hazard), exactly ONE corner candidate in the overshoot span, no
        // relocated vertex in the span (mixed authority).
        struct ExitFix {
            j: u32,
            c: u32,
            other: u32,
            pair: (usize, usize),
        }
        let mut exit_fixes: Vec<ExitFix> = Vec::new();
        if std::env::var_os("YANG_441_BOUNDARY_EXIT").is_some() {
            use crate::stage4_construct::{chain_straightness, on_segment_interior};
            let relocated: BTreeSet<u32> = relocations.iter().map(|&(v, _)| v).collect();
            let conic_end: BTreeSet<u32> = intersection_curves
                .iter()
                .filter(|(_, c)| !matches!(c, Curve::LineSegment))
                .flat_map(|(&(a, b), _)| [a, b])
                .collect();
            let holders_of = |v: u32| -> usize {
                patches
                    .iter()
                    .filter(|p| p.cycles.iter().any(|c| c.contains(&v)))
                    .count()
            };
            for (gi, g) in groups.iter().enumerate() {
                if !matches!(g.curve, Curve::LineSegment) {
                    continue;
                }
                let Ok((chain, false)) =
                    ordered_seam_side(&patches[g.pair.0].cycles, &g.edges, Side::A)
                else {
                    continue;
                };
                if chain.len() < 2 {
                    continue;
                }
                if chain.len() >= 3
                    && !matches!(chain_straightness(&mesh.verts, &chain), Some(s) if s <= 1e-9)
                {
                    continue; // two lines meeting at a real corner — not one seam
                }
                let chain_set: BTreeSet<u32> = chain.iter().copied().collect();
                let last = *chain.last().expect("chain len >= 2");
                for (j, inner, other) in [
                    (chain[0], chain[1], last),
                    (last, chain[chain.len() - 2], chain[0]),
                ] {
                    // Only a relocated conic-chain terminal can be an
                    // overshot junction; anything else is not this family.
                    if !relocated.contains(&j) || !conic_end.contains(&j) {
                        continue;
                    }
                    // The overshoot span is strictly between J and the
                    // seam's FIRST sample — kept content there is zero.
                    let mut corners: Vec<u32> = Vec::new();
                    let mut walkers = 0usize;
                    let mut mixed: Option<u32> = None;
                    for &pi in &[g.pair.0, g.pair.1] {
                        for cyc in &patches[pi].cycles {
                            for &v in cyc {
                                if chain_set.contains(&v)
                                    || corners.contains(&v)
                                    || !on_segment_interior(&mesh.verts, j, inner, v)
                                {
                                    continue;
                                }
                                if relocated.contains(&v) {
                                    mixed = Some(v);
                                } else if holders_of(v) >= 3 {
                                    corners.push(v);
                                } else {
                                    walkers += 1;
                                }
                            }
                        }
                    }
                    if corners.is_empty() && mixed.is_none() {
                        continue; // healthy terminal (walk-backs alone are I1f's)
                    }
                    if let Some(v) = mixed {
                        c441_log!(
                            "[j1-exit] pass={pass} seam={gi}: REFUSED terminal v{j} — \
                             relocated vertex v{v} inside the overshoot span (mixed \
                             authority)"
                        );
                        continue;
                    }
                    if corners.len() > 1 {
                        c441_log!(
                            "[j1-exit] pass={pass} seam={gi}: REFUSED terminal v{j} — \
                             {} corner candidates in one overshoot span ({:?})",
                            corners.len(),
                            corners
                        );
                        continue;
                    }
                    let c = corners[0];
                    c441_log!(
                        "[j1-exit] pass={pass} seam={gi}: EXIT v{j} -> corner v{c} \
                         (holders={}, walk-backs-in-span={walkers}, chain len {})",
                        holders_of(c),
                        chain.len()
                    );
                    exit_fixes.push(ExitFix {
                        j,
                        c,
                        other,
                        pair: g.pair,
                    });
                }
            }
        }

        // ---- I2c (sub-gated `YANG_441_INPUT_REFINE`): input-edge chain
        // refinement at seam-adjacent corners (spec §4-I2c). The Stage-1
        // discretization of a same-solid B-Rep edge is chord-anchored by the
        // curved owner's tessellation; where it meets a seam junction the two
        // authorities disagree by the chord gap (the 975-class corner vs the
        // 999-class junction) and the boundary folds back between them — the
        // F0067 wheel-corner family. Refine such chains onto the exact
        // plane∩cylinder ruling (Fig-13's boundary discipline applied to
        // input edges) and feed each refined corner endpoint to the Fig-11(b)
        // merge in the assembly loop. Census always prints; positions move
        // only when `apply_enabled`.
        let mut refine_pairs: Vec<(u32, u32)> = Vec::new();
        if std::env::var_os("YANG_441_INPUT_REFINE").is_some() && !eligible.is_empty() {
            match crate::stage4_correct::stage4_chord_band(a, b) {
                None => c441_log!("[s4-refine] pass={pass}: SKIP — no derivable chord band"),
                Some(band) => {
                    let patch_attr: Vec<Option<crate::brep::TriangleAttribution>> = patches
                        .iter()
                        .map(|p| {
                            let mut it =
                                p.tris.iter().map(|&t| attribution.attributions[t as usize]);
                            let first = it.next().flatten()?;
                            it.all(|a| a == Some(first)).then_some(first)
                        })
                        .collect();
                    let mut seam_edges: BTreeSet<(u32, u32)> = BTreeSet::new();
                    let mut junctions: BTreeSet<u32> = BTreeSet::new();
                    let mut scope: BTreeSet<usize> = BTreeSet::new();
                    for e in &eligible {
                        scope.insert(e.pair.0);
                        scope.insert(e.pair.1);
                        let (e0, e1) = (e.chain[0], *e.chain.last().expect("chain len >= 3"));
                        junctions.insert(e0);
                        junctions.insert(e1);
                        // The collapsed DIRECT edge is not in
                        // `intersection_curves` — exclude it explicitly.
                        seam_edges.insert((e0.min(e1), e0.max(e1)));
                        seam_edges
                            .extend(e.chain.windows(2).map(|w| (w[0].min(w[1]), w[0].max(w[1]))));
                        match &e.action {
                            SeamAction::ReorderConic { ordered } => seam_edges.extend(
                                ordered.windows(2).map(|w| (w[0].min(w[1]), w[0].max(w[1]))),
                            ),
                            SeamAction::RefineConic { refined, .. } => seam_edges.extend(
                                refined.windows(2).map(|w| (w[0].min(w[1]), w[0].max(w[1]))),
                            ),
                            SeamAction::CollapseLine => {}
                        }
                    }
                    let relocated: BTreeSet<u32> = relocations.iter().map(|&(v, _)| v).collect();
                    // A scoped patch without a uniform attribution cannot
                    // name its input solid — its runs are invisible to the
                    // identification. LOUD: this is a coverage hole, not a
                    // clean skip (merge_same_plane_patches can fuse patches
                    // of different faces).
                    for &pi in &scope {
                        if patch_attr[pi].is_none() {
                            c441_log!(
                                "[s4-refine] pass={pass}: SKIP scoped patch {pi} — mixed or \
                                 absent attribution ({} tris); its input-edge runs are \
                                 invisible",
                                patches[pi].tris.len()
                            );
                        }
                    }
                    // Verbose edge-disqualification census: for each scoped
                    // patch, why each plain-looking cycle edge is not part of
                    // an input-edge run — the coverage ledger for I2c.
                    if std::env::var_os("YANG_441_VERBOSE").is_some() {
                        let mut owners: BTreeMap<(u32, u32), Vec<usize>> = BTreeMap::new();
                        for (pi, p) in patches.iter().enumerate() {
                            for cyc in &p.cycles {
                                let n = cyc.len();
                                for i in 0..n {
                                    let (s, e) = (cyc[i], cyc[(i + 1) % n]);
                                    let key = (s.min(e), s.max(e));
                                    let v = owners.entry(key).or_default();
                                    if v.last() != Some(&pi) {
                                        v.push(pi);
                                    }
                                }
                            }
                        }
                        for &pi in &scope {
                            for cyc in &patches[pi].cycles {
                                let n = cyc.len();
                                for i in 0..n {
                                    let (s, e) = (cyc[i], cyc[(i + 1) % n]);
                                    let key = (s.min(e), s.max(e));
                                    let why = if intersection_curves.contains_key(&key) {
                                        "curve"
                                    } else if seam_edges.contains(&key) {
                                        "seam"
                                    } else {
                                        match owners.get(&key).map(Vec::as_slice) {
                                            Some([a, b]) => {
                                                let o = if a == &pi { *b } else { *a };
                                                match (patch_attr[pi], patch_attr[o]) {
                                                    (Some(x), Some(y))
                                                        if x.input == y.input && x != y =>
                                                    {
                                                        "RUN"
                                                    }
                                                    (Some(_), None) => "neighbor-attr-none",
                                                    (None, _) => "self-attr-none",
                                                    _ => "cross-input-or-same-face",
                                                }
                                            }
                                            _ => "owner-multiplicity",
                                        }
                                    };
                                    eprintln!("[s4-refine-edges] patch {pi} edge ({s},{e}): {why}");
                                }
                            }
                        }
                    }
                    let (chains, run_skips) = crate::stage4_construct::input_edge_chains(
                        &patches,
                        &patch_attr,
                        &mesh.verts,
                        intersection_curves,
                        &seam_edges,
                        &junctions,
                        &scope,
                        band,
                    );
                    if std::env::var_os("YANG_441_VERBOSE").is_some() {
                        for sk in &run_skips {
                            eprintln!(
                                "[s4-refine] pass={pass}: RUN-SKIP {:?} patches {}+{} verts {:?}",
                                sk.reason, sk.patch, sk.neighbor, sk.verts
                            );
                        }
                    }
                    for ch in &chains {
                        // Authority partition (Fig-13: corner points are
                        // PINNED, boundary points glide). A junction or
                        // curve-relocated vertex at a run ENDPOINT is kept
                        // fixed — it is already exact, and at cluster
                        // corners the input-edge chain is routinely TOPPED
                        // by a junction copy. One INSIDE the run is a mixed
                        // authority — refuse loud.
                        let pinned = |v: u32| junctions.contains(&v) || relocated.contains(&v);
                        if let Some(&v) =
                            ch.verts[1..ch.verts.len() - 1].iter().find(|&&v| pinned(v))
                        {
                            c441_log!(
                                "[s4-refine] pass={pass}: REFUSED chain patches {}+{} — \
                                 v{v} is a junction/relocated vertex INSIDE the run \
                                 (mixed authority) — verts {:?}",
                                ch.patch,
                                ch.neighbor,
                                ch.verts
                            );
                            continue;
                        }
                        let (ps, qs) = (patches[ch.patch].surface, patches[ch.neighbor].surface);
                        let (plane, cyl) = match (&ps, &qs) {
                            (Surface::Plane { .. }, Surface::Cylinder { .. }) => (ps, qs),
                            (Surface::Cylinder { .. }, Surface::Plane { .. }) => (qs, ps),
                            (Surface::Plane { .. }, Surface::Plane { .. }) => {
                                // A plane×plane input edge is exact — the
                                // chain already lies on it; nothing to refine.
                                if std::env::var_os("YANG_441_VERBOSE").is_some() {
                                    eprintln!(
                                        "[s4-refine] pass={pass}: chain patches {}+{} — \
                                         plane×plane input edge is exact; no-op",
                                        ch.patch, ch.neighbor
                                    );
                                }
                                continue;
                            }
                            _ => {
                                c441_log!(
                                    "[s4-refine] pass={pass}: SKIP chain patches {}+{} — \
                                     unsupported surface pair (I2c tail)",
                                    ch.patch,
                                    ch.neighbor
                                );
                                continue;
                            }
                        };
                        match crate::stage4_construct::refine_chain_to_ruling(
                            &mesh.verts,
                            &ch.verts,
                            &plane,
                            &cyl,
                            band,
                        ) {
                            Err(e) => c441_log!(
                                "[s4-refine] pass={pass}: REFUSED chain patches {}+{} \
                                 ({} verts) — {e:?}",
                                ch.patch,
                                ch.neighbor,
                                ch.verts.len()
                            ),
                            Ok(moves) => {
                                let max_disp = moves
                                    .iter()
                                    .map(|&(v, p)| {
                                        let w = mesh.verts[v as usize];
                                        ((p.x() - w.x()).powi(2)
                                            + (p.y() - w.y()).powi(2)
                                            + (p.z() - w.z()).powi(2))
                                        .sqrt()
                                    })
                                    .fold(0.0f64, f64::max);
                                c441_log!(
                                    "[s4-refine] pass={pass}: CHAIN patches {}+{} verts={} \
                                     corners={:?} max_disp={max_disp:.3e} band={band:.3e}{}",
                                    ch.patch,
                                    ch.neighbor,
                                    ch.verts.len(),
                                    ch.corner_pairs,
                                    if apply_enabled { "" } else { " (census-only)" }
                                );
                                if apply_enabled {
                                    for &(v, p) in &moves {
                                        if pinned(v) {
                                            continue; // junction authority keeps it
                                        }
                                        mesh.verts[v as usize] = p;
                                    }
                                    refine_pairs.extend(
                                        ch.corner_pairs
                                            .iter()
                                            .copied()
                                            .filter(|&(p, _)| !pinned(p)),
                                    );
                                }
                            }
                        }
                    }
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
        // Corner-merge pairs refused during assembly (unchartable holder, a
        // holder's rebuild declined, …) — persists across restarts so the
        // assembly converges.
        let mut merge_blocked: BTreeSet<u32> = BTreeSet::new();
        // Rim-trim refusals (vertex-keyed, persists across restarts like
        // merge_blocked): a trim-pulled holder that declines blocks the trim
        // vertices that pulled it, and the assembly converges.
        let mut trim_blocked: BTreeSet<u32> = BTreeSet::new();
        let (rebuilds, subs) = 'assemble: loop {
            // J1 exit fixes keep the batch alive even with no collapsible
            // seam — a minimal (2-vertex) seam's owners join through the
            // holder closure alone.
            let live_exit = exit_fixes.iter().any(|f| !merge_blocked.contains(&f.j));
            if active.is_empty() && !live_exit {
                break (Vec::new(), std::collections::BTreeMap::new());
            }
            // Group each patch's eligible chains (both owners of every seam).
            let mut chains_of: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
            for &ei in &active {
                let e = &eligible[ei];
                chains_of.entry(e.pair.0).or_default().push(ei);
                chains_of.entry(e.pair.1).or_default().push(ei);
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
            // I1g (sub-gated `YANG_441_CORNER_MERGE`) — Fig-11(a)–(c) corner
            // identification INSIDE the batch, unblocked by I2a: the merge is
            // a shared-INDEX substitution (p → q) in the batched cycles, and
            // every HOLDER patch of p joins the batch as a rebuild
            // participant (cylinder caps included — merge-only holders carry
            // no seams, they just re-CDT against the substituted cycles). A
            // holder that is unchartable, or whose rebuild later declines,
            // refuses the merge loudly (merge_blocked persists across
            // restarts). Selector: the inc-2 validated split-edge containment
            // (every hit at the exact corner gap; zero over-fire).
            let mut subs: std::collections::BTreeMap<u32, u32> = std::collections::BTreeMap::new();
            let mut merge_only: BTreeSet<usize> = BTreeSet::new();
            // Pull attribution: holder -> the pairs whose substitution
            // REQUIRED its rebuild. A declining holder blames exactly these
            // — blaming every pair whose p merely sits on its cycles blocks
            // the whole rim when the encircling lateral declines (measured
            // 2026-08-11: one corner pulled it, all 28 pairs blocked).
            let mut required_by: std::collections::BTreeMap<usize, Vec<u32>> =
                std::collections::BTreeMap::new();
            // Trim pull attribution: holder -> the rim-trim vertices whose
            // removal REQUIRED its rebuild (the removal's all-holders rule).
            let mut trim_pull: std::collections::BTreeMap<usize, Vec<u32>> =
                std::collections::BTreeMap::new();
            {
                let pos = |i: u32| -> [f64; 3] {
                    let w = mesh.verts[i as usize];
                    [w.x(), w.y(), w.z()]
                };
                let dist3 = |x: u32, y: u32| -> f64 {
                    let (a, b) = (pos(x), pos(y));
                    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
                };
                // p -> (q, dist); a p claimed for two different q's is
                // ambiguous — skipped loudly. Two arms feed it: the I2c
                // refine-anchored pairs and the I1g containment selector.
                let mut pairs: std::collections::BTreeMap<u32, (u32, f64)> =
                    std::collections::BTreeMap::new();
                let mut ambiguous: BTreeSet<u32> = BTreeSet::new();
                // I2c refine arm: the input-edge chain endpoint p was refined
                // onto the exact edge next to junction q — Fig-11(b) with the
                // split already done topologically (the q-already-a-vertex
                // case the containment selector cannot see: no edge CONTAINS
                // a q that is itself a chain vertex). The post-refinement
                // distance must confirm the corner gap actually closed.
                if !refine_pairs.is_empty() {
                    if let Some(band) = crate::stage4_correct::stage4_chord_band(a, b) {
                        for &(p, q) in &refine_pairs {
                            if p == q || merge_blocked.contains(&p) {
                                continue;
                            }
                            let d = dist3(p, q);
                            if d > band {
                                c441_log!(
                                    "[s4-refine] pass={pass}: REFINE-MERGE SKIPPED v{p} -> \
                                     v{q} — dist {d:.3e} above band after refinement"
                                );
                                continue;
                            }
                            match pairs.get(&p) {
                                Some(&(q0, _)) if q0 != q => {
                                    ambiguous.insert(p);
                                }
                                Some(_) => {}
                                None => {
                                    pairs.insert(p, (q, d));
                                }
                            }
                        }
                    }
                }
                if std::env::var_os("YANG_441_CORNER_MERGE").is_some() {
                    if let Some(band) = crate::stage4_correct::stage4_chord_band(a, b) {
                        let chain_verts: BTreeSet<u32> = active
                            .iter()
                            .flat_map(|&ei| eligible[ei].chain.iter().copied())
                            .collect();
                        let seg_perp = |q: u32, s: u32, t: u32| -> Option<(f64, f64)> {
                            let (a, b, x) = (pos(s), pos(t), pos(q));
                            let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
                            let len2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
                            if len2 == 0.0 || !len2.is_finite() {
                                return None;
                            }
                            let r = [x[0] - a[0], x[1] - a[1], x[2] - a[2]];
                            let tau = (r[0] * d[0] + r[1] * d[1] + r[2] * d[2]) / len2;
                            let perp = [r[0] - tau * d[0], r[1] - tau * d[1], r[2] - tau * d[2]];
                            Some((
                                tau,
                                (perp[0] * perp[0] + perp[1] * perp[1] + perp[2] * perp[2]).sqrt(),
                            ))
                        };
                        for &ei in &active {
                            let e = &eligible[ei];
                            let mut seam_edges: BTreeSet<(u32, u32)> = e
                                .chain
                                .windows(2)
                                .map(|w| (w[0].min(w[1]), w[0].max(w[1])))
                                .collect();
                            match &e.action {
                                SeamAction::ReorderConic { ordered } => seam_edges.extend(
                                    ordered.windows(2).map(|w| (w[0].min(w[1]), w[0].max(w[1]))),
                                ),
                                SeamAction::RefineConic { refined, .. } => seam_edges.extend(
                                    refined.windows(2).map(|w| (w[0].min(w[1]), w[0].max(w[1]))),
                                ),
                                SeamAction::CollapseLine => {}
                            }
                            for q in [e.chain[0], *e.chain.last().expect("chain len >= 3")] {
                                for (pj, pat) in patches.iter().enumerate() {
                                    let cycles = mod_cycles.get(&pj).unwrap_or(&pat.cycles);
                                    for cyc in cycles {
                                        let n = cyc.len();
                                        for i in 0..n {
                                            let (s, t) = (cyc[i], cyc[(i + 1) % n]);
                                            let key = (s.min(t), s.max(t));
                                            if s == q
                                                || t == q
                                                || seam_edges.contains(&key)
                                                || intersection_curves.get(&key).is_none()
                                            {
                                                continue;
                                            }
                                            let Some((tau, perp)) = seg_perp(q, s, t) else {
                                                continue;
                                            };
                                            if !(0.0..=1.0).contains(&tau) || perp > band {
                                                continue;
                                            }
                                            let p = if tau < 0.5 { s } else { t };
                                            if p == q
                                                || chain_verts.contains(&p)
                                                || merge_blocked.contains(&p)
                                            {
                                                continue;
                                            }
                                            let d = dist3(p, q);
                                            if d > band {
                                                continue; // the paper's split arm
                                            }
                                            match pairs.get(&p) {
                                                Some(&(q0, _)) if q0 != q => {
                                                    ambiguous.insert(p);
                                                }
                                                Some(_) => {}
                                                None => {
                                                    pairs.insert(p, (q, d));
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                // J1 boundary-exit arm: the junction terminal J substitutes
                // INTO the kept-boundary corner C — the REVERSE direction of
                // the containment merge, because here the junction is the
                // vertex with zero kept content. The boundary-exit predicate
                // is exact (on-carrier identity), so it wins a direction
                // conflict with the band-based containment selector.
                for f in &exit_fixes {
                    if merge_blocked.contains(&f.j) {
                        continue;
                    }
                    if let Some(&(q0, _)) = pairs.get(&f.c) {
                        if q0 == f.j {
                            c441_log!(
                                "[j1-exit] pass={pass}: dropping containment pair \
                                 v{} -> v{} — reversed by the boundary-exit fix",
                                f.c,
                                f.j
                            );
                            pairs.remove(&f.c);
                        }
                    }
                    if pairs.contains_key(&f.c) {
                        c441_log!(
                            "[j1-exit] pass={pass}: REFUSED v{} -> v{} — corner v{} is \
                             itself substituted this batch (chained substitution)",
                            f.j,
                            f.c,
                            f.c
                        );
                        continue;
                    }
                    let d = dist3(f.j, f.c);
                    match pairs.get(&f.j) {
                        Some(&(c0, _)) if c0 != f.c => {
                            ambiguous.insert(f.j);
                        }
                        Some(_) => {}
                        None => {
                            c441_log!(
                                "[j1-exit] pass={pass}: BOUNDARY-EXIT MERGE v{} -> v{} \
                                 dist={d:.3e}",
                                f.j,
                                f.c
                            );
                            pairs.insert(f.j, (f.c, d));
                        }
                    }
                }
                for p in &ambiguous {
                    c441_log!("[s4-construct] pass={pass}: CORNER-MERGE AMBIGUOUS v{p}; skipped");
                    pairs.remove(p);
                }
                // Holder closure: every patch whose cycles hold p and whose
                // rebuild the substitution REQUIRES must be chartable and
                // joins the batch. For a J1 boundary-exit pair, a holder
                // whose cycles keep p AWAY from q (no consecutive duplicate
                // after substitution) and which holds no triangle with both
                // p and q adopts the substitution by pure index re-point in
                // the write-back — it does not re-CDT. The encircling
                // drum-lateral holder (ThetaUnwrap-refused; measured
                // 2026-08-11 blocking EVERY exit fix when pulled in)
                // requires this path. I1g/I2c pairs keep the full closure —
                // their measured behavior is not this increment's to change.
                let exit_ps: BTreeSet<u32> = exit_fixes.iter().map(|f| f.j).collect();
                for (&p, &(q, d)) in &pairs {
                    let holders: Vec<usize> = patches
                        .iter()
                        .enumerate()
                        .filter(|&(pj, pat)| {
                            let cycles = mod_cycles.get(&pj).unwrap_or(&pat.cycles);
                            cycles.iter().any(|c| c.contains(&p))
                        })
                        .map(|(pj, _)| pj)
                        .collect();
                    // A holder is re-point-safe ONLY when q appears nowhere
                    // in its cycles or triangles — the substitution is then a
                    // pure relabel of p's slot. Mere non-adjacency is NOT
                    // enough: a wall whose boundary walks J .. walk-backs .. C
                    // re-pointed without a rebuild pinches at C (the inc-3
                    // bare-collapse shape, re-measured 2026-08-11 as
                    // rebuilt=[] on every walk-back corner).
                    let needs_rebuild = |pj: usize| -> bool {
                        let cycles = mod_cycles.get(&pj).unwrap_or(&patches[pj].cycles);
                        cycles.iter().any(|c| c.contains(&q))
                            || patches[pj]
                                .tris
                                .iter()
                                .any(|&t| mesh.tris[t as usize].contains(&q))
                    };
                    let pulled: Vec<usize> = if exit_ps.contains(&p) {
                        holders
                            .iter()
                            .copied()
                            .filter(|&h| needs_rebuild(h))
                            .collect()
                    } else {
                        holders.clone()
                    };
                    let chartable = |s: &Surface| crate::stage4_project::SurfaceChart::supports(s);
                    if pulled.iter().any(|&h| !chartable(&patches[h].surface)) {
                        c441_log!(
                            "[s4-construct] pass={pass}: CORNER-MERGE REFUSED v{p} -> \
                                 v{q} — an unchartable holder; blocked"
                        );
                        merge_blocked.insert(p);
                        continue 'assemble;
                    }
                    let why: Vec<String> = pulled
                        .iter()
                        .map(|&h| {
                            let cycles = mod_cycles.get(&h).unwrap_or(&patches[h].cycles);
                            let in_cyc = cycles.iter().any(|c| c.contains(&q));
                            format!("{h}:{}", if in_cyc { "cyc" } else { "tri" })
                        })
                        .collect();
                    c441_log!(
                        "[s4-construct] pass={pass}: CORNER-MERGE v{p} -> v{q} \
                             dist={d:.3e} holders={holders:?} rebuilt={why:?}"
                    );
                    for &h in &pulled {
                        required_by.entry(h).or_default().push(p);
                        if let std::collections::btree_map::Entry::Vacant(slot) =
                            mod_cycles.entry(h)
                        {
                            slot.insert(patches[h].cycles.clone());
                            merge_only.insert(h);
                        }
                    }
                    subs.insert(p, q);
                }
                if !subs.is_empty() {
                    for cycles in mod_cycles.values_mut() {
                        for cyc in cycles.iter_mut() {
                            for v in cyc.iter_mut() {
                                if let Some(&q) = subs.get(v) {
                                    *v = q;
                                }
                            }
                        }
                    }
                }
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
                        if e.pair.0 != pi && e.pair.1 != pi {
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
                // J1: an exit-fixed seam's remaining span (corner -> other
                // end) gets the same treatment — a minimal seam's owners are
                // in the batch as merge-only holders, and its walk-back
                // vertices weave the boundary exactly like a collapsed
                // seam's. (For an actively collapsed seam this adds a subset
                // of the segment above; `candidates` is a set.)
                for f in &exit_fixes {
                    if !subs.contains_key(&f.j) {
                        continue;
                    }
                    for &pi in &[f.pair.0, f.pair.1] {
                        let Some(cycles) = mod_cycles.get(&pi) else {
                            continue;
                        };
                        for cyc in cycles {
                            for &v in cyc {
                                if v != f.c
                                    && v != f.other
                                    && on_segment_interior(&mesh.verts, f.c, f.other, v)
                                {
                                    candidates.insert(v);
                                }
                            }
                        }
                    }
                }
                // RIM TRIM (sub-gated `YANG_441_RIM_TRIM`, spec §4 next
                // increment): §4.4.1's near-curve removal for CIRCLE chains,
                // side-aware. The rim-weave census (2026-08-11) measured the
                // A-top decline family: plain unmoved boundary vertices
                // dipping INSIDE the trim circle (chord-sliver ramps at
                // |dr| = 3e-4..1.3e-3) poke through the parameter-ordered
                // chain's shallow chords — content the exact trim removes.
                // A candidate must be: on a batched patch whose cycles carry
                // a circle chain; strictly on the NON-KEPT side of that
                // circle (kept side witnessed by the chain edges' own
                // triangles; both-sides/ambiguous => loud skip); within the
                // derived chord band but beyond the identity band; PLAIN
                // (no incident intersection-curve edge anywhere), unmoved,
                // and not part of any substitution. The I1f holder
                // discipline below (<=2 holders, all batched, no
                // degeneration) applies unchanged. I1f's conic guard — "a
                // conic's chord is not its curve" — is untouched: chain
                // members are curve-incident and never candidates here.
                let mut trim_cands: BTreeSet<u32> = BTreeSet::new();
                if std::env::var_os("YANG_441_RIM_TRIM").is_some() {
                    if let Some(band) = crate::stage4_correct::stage4_chord_band(a, b) {
                        let relocated: BTreeSet<u32> =
                            relocations.iter().map(|&(v, _)| v).collect();
                        let exit_corners: BTreeSet<u32> = exit_fixes.iter().map(|f| f.c).collect();
                        let mut curve_touched: BTreeSet<u32> = BTreeSet::new();
                        for &(s, t) in intersection_curves.keys() {
                            curve_touched.insert(s);
                            curve_touched.insert(t);
                        }
                        for (&pi, cycles) in &mod_cycles {
                            // Distinct circles among this patch's cycle edges.
                            let mut circles: Vec<(
                                cad_primitives::Point3,
                                cad_primitives::Vector3,
                                f64,
                            )> = Vec::new();
                            let mut chain_edges: Vec<(u32, u32, usize)> = Vec::new();
                            for cyc in cycles {
                                let n = cyc.len();
                                for i in 0..n {
                                    let (s, t) = (cyc[i], cyc[(i + 1) % n]);
                                    let key = (s.min(t), s.max(t));
                                    if let Some(Curve::Circle {
                                        center,
                                        normal,
                                        radius,
                                    }) = intersection_curves.get(&key)
                                    {
                                        let ci = circles
                                            .iter()
                                            .position(|&(c, _, r)| {
                                                c.as_array() == center.as_array() && r == *radius
                                            })
                                            .unwrap_or_else(|| {
                                                circles.push((*center, *normal, *radius));
                                                circles.len() - 1
                                            });
                                        chain_edges.push((s, t, ci));
                                    }
                                }
                            }
                            for (ci, &(c, nrm, r)) in circles.iter().enumerate() {
                                let dr = |v: u32| -> f64 {
                                    let p = mesh.verts[v as usize];
                                    let d = [p.x() - c.x(), p.y() - c.y(), p.z() - c.z()];
                                    let along = d[0] * nrm.x() + d[1] * nrm.y() + d[2] * nrm.z();
                                    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2] - along * along).sqrt()
                                        - r
                                };
                                // Kept side: the sign of dr at the third
                                // vertex of each triangle owning a chain
                                // edge — counting ONLY witnesses beyond the
                                // band. A within-band third vertex is
                                // near-curve content (potentially the very
                                // sliver the trim removes — measured: the
                                // declining tops carried 1-4 sliver
                                // witnesses inside, all within band) and is
                                // no evidence of kept side. Witnesses beyond
                                // the band on BOTH sides => the circle is
                                // not a one-sided trim boundary — loud skip.
                                let (mut pos_w, mut neg_w) = (0usize, 0usize);
                                for &t in &patches[pi].tris {
                                    let tri = mesh.tris[t as usize];
                                    for k in 0..3 {
                                        let (a3, b3) = (tri[k], tri[(k + 1) % 3]);
                                        if chain_edges.iter().any(|&(s, e, cj)| {
                                            cj == ci
                                                && ((s == a3 && e == b3) || (s == b3 && e == a3))
                                        }) {
                                            let w = dr(tri[(k + 2) % 3]);
                                            if w > band {
                                                pos_w += 1;
                                            } else if w < -band {
                                                neg_w += 1;
                                            }
                                        }
                                    }
                                }
                                let kept_positive = match (pos_w > 0, neg_w > 0) {
                                    (true, false) => Some(true),
                                    (false, true) => Some(false),
                                    (true, true) => None,
                                    (false, false) => {
                                        // Dense near-rim mesh: every chain
                                        // triangle's third vertex is within
                                        // band (measured on the 328-class
                                        // tops). Fall back to the BOUNDARY
                                        // majority beyond the band — the
                                        // outline is far outside; a
                                        // genuinely two-sided patch has
                                        // beyond-band boundary both sides.
                                        let (mut bp, mut bn) = (0usize, 0usize);
                                        for cyc in cycles {
                                            for &bv in cyc {
                                                let d = dr(bv);
                                                if d > band {
                                                    bp += 1;
                                                } else if d < -band {
                                                    bn += 1;
                                                }
                                            }
                                        }
                                        match (bp > 0, bn > 0) {
                                            (true, false) => Some(true),
                                            (false, true) => Some(false),
                                            _ => None,
                                        }
                                    }
                                };
                                let Some(kept_positive) = kept_positive else {
                                    c441_log!(
                                        "[s4-construct] pass={pass}: RIM-TRIM SKIP \
                                         patch {pi} circle[{ci}] — kept side \
                                         ambiguous (pos={pos_w} neg={neg_w})"
                                    );
                                    continue;
                                };
                                for cyc in cycles {
                                    for &v in cyc {
                                        if curve_touched.contains(&v)
                                            || relocated.contains(&v)
                                            || subs.contains_key(&v)
                                            || subs.values().any(|&q| q == v)
                                            || exit_corners.contains(&v)
                                            || trim_blocked.contains(&v)
                                        {
                                            continue;
                                        }
                                        let d = dr(v);
                                        let covered = if kept_positive { -d } else { d };
                                        if covered > 1e-9 && covered <= band {
                                            c441_log!(
                                                "[s4-construct] pass={pass}: RIM-TRIM \
                                                 candidate v{v} patch {pi} \
                                                 dr={d:+.3e} (covered side)"
                                            );
                                            trim_cands.insert(v);
                                        }
                                    }
                                }
                            }
                        }
                        // Holder closure for trim candidates: the removal's
                        // all-holders rule needs every holder in the batch.
                        // At the flush interface the ramp debris is shared
                        // with B-side fragment patches (measured holders up
                        // to 4-5, not batched) — pull chartable holders in
                        // as trim participants; an unchartable holder
                        // refuses the candidate loudly and persistently.
                        let mut refused: Vec<u32> = Vec::new();
                        for &v in &trim_cands {
                            let holders: Vec<usize> = patches
                                .iter()
                                .enumerate()
                                .filter(|&(pj, p)| {
                                    let cycles = mod_cycles.get(&pj).unwrap_or(&p.cycles);
                                    cycles.iter().any(|c| c.contains(&v))
                                })
                                .map(|(pj, _)| pj)
                                .collect();
                            let chartable =
                                |s: &Surface| crate::stage4_project::SurfaceChart::supports(s);
                            if holders.iter().any(|&h| !chartable(&patches[h].surface)) {
                                c441_log!(
                                    "[s4-construct] pass={pass}: RIM-TRIM REFUSED v{v} — \
                                     an unchartable holder; blocked"
                                );
                                trim_blocked.insert(v);
                                refused.push(v);
                                continue;
                            }
                            for &h in &holders {
                                trim_pull.entry(h).or_default().push(v);
                                if let std::collections::btree_map::Entry::Vacant(slot) =
                                    mod_cycles.entry(h)
                                {
                                    slot.insert(patches[h].cycles.clone());
                                    merge_only.insert(h);
                                }
                            }
                            candidates.insert(v);
                        }
                        for v in refused {
                            trim_cands.remove(&v);
                        }
                    }
                }
                let mut removed_total = 0usize;
                for &v in &candidates {
                    // Every holder patch must be in the batch (≤2 holders),
                    // and no holder's cycle may degenerate below 3 vertices —
                    // the removal is all-holders-or-none (a one-sided removal
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
                    // Rim-trim candidates are exempt from the ≤2-holder cap:
                    // flush-interface debris is legitimately high-degree
                    // (its holders were pulled into the batch by the trim
                    // closure), and design corners are excluded upstream
                    // (exit-corner set, curve incidence, substitutions).
                    if (holders.len() > 2 && !trim_cands.contains(&v))
                        || !holders.iter().all(|h| mod_cycles.contains_key(h))
                    {
                        c441_log!(
                            "[s4-construct] pass={pass}: NEAR-CURVE REMOVAL BLOCKED v{v} — \
                             holders {holders:?} not all rebuilt (or >2); vertex stays"
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
                        // Fragment census (env-gated): the 2026-08-11 rim-trim
                        // measurement attributed this block to triangle-scale
                        // flush-interface sliver fragments — dump each
                        // holder's identity (side, face, surface, size) so
                        // the fragment family is measured, not inferred.
                        if std::env::var_os("YANG_441_RIM_CENSUS").is_some()
                            && trim_cands.contains(&v)
                        {
                            for &h in &holders {
                                let info = &infos[h];
                                let tris = &patches[h].tris;
                                let mut area = 0.0f64;
                                let mut max_edge = 0.0f64;
                                for &t in tris {
                                    let tri = mesh.tris[t as usize];
                                    let p0 = mesh.verts[tri[0] as usize];
                                    let p1 = mesh.verts[tri[1] as usize];
                                    let p2 = mesh.verts[tri[2] as usize];
                                    let e01 = [p1.x() - p0.x(), p1.y() - p0.y(), p1.z() - p0.z()];
                                    let e02 = [p2.x() - p0.x(), p2.y() - p0.y(), p2.z() - p0.z()];
                                    let cx = e01[1] * e02[2] - e01[2] * e02[1];
                                    let cy = e01[2] * e02[0] - e01[0] * e02[2];
                                    let cz = e01[0] * e02[1] - e01[1] * e02[0];
                                    area += 0.5 * (cx * cx + cy * cy + cz * cz).sqrt();
                                    for (u, w) in [(p0, p1), (p1, p2), (p2, p0)] {
                                        let l = ((w.x() - u.x()).powi(2)
                                            + (w.y() - u.y()).powi(2)
                                            + (w.z() - u.z()).powi(2))
                                        .sqrt();
                                        max_edge = max_edge.max(l);
                                    }
                                }
                                let surf = match &patches[h].surface {
                                    Surface::Plane { normal, d } => format!(
                                        "Plane(n=({:+.3},{:+.3},{:+.3}) d={d:+.9})",
                                        normal.x(),
                                        normal.y(),
                                        normal.z()
                                    ),
                                    Surface::Cylinder { radius, .. } => {
                                        format!("Cyl(r={radius:.6})")
                                    }
                                    s => format!("{s:?}"),
                                };
                                let cyc_shape: Vec<(usize, usize)> = mod_cycles[&h]
                                    .iter()
                                    .map(|c| (c.len(), c.iter().filter(|&&x| x == v).count()))
                                    .collect();
                                let small_cycles: Vec<&Vec<u32>> =
                                    mod_cycles[&h].iter().filter(|c| c.len() <= 8).collect();
                                c441_log!(
                                    "[rim-frag] v{v} holder {h}: input={:?} face={} {surf} \
                                     tris={} area={area:.3e} max_edge={max_edge:.3e} \
                                     cycles(len,occ)={cyc_shape:?} small={small_cycles:?}",
                                    info.input,
                                    info.face_idx,
                                    tris.len(),
                                );
                                for cyc in mod_cycles[&h].iter().filter(|c| c.len() <= 8) {
                                    for &cv in cyc {
                                        let p = mesh.verts[cv as usize];
                                        let rad = (p.x() * p.x() + p.y() * p.y()).sqrt();
                                        c441_log!(
                                            "[rim-frag]     v{cv} r={rad:.9} z={:.9} \
                                             xyz=({:.6},{:.6},{:.6})",
                                            p.z(),
                                            p.x(),
                                            p.y(),
                                            p.z()
                                        );
                                    }
                                }
                                // Sibling patches on the SAME input face: is the
                                // face genuinely fragmented, and does a mesh
                                // edge connect the fragment to a sibling?
                                if tris.len() <= 8 {
                                    let frag_edges: BTreeSet<(u32, u32)> = tris
                                        .iter()
                                        .flat_map(|&t| {
                                            let tri = mesh.tris[t as usize];
                                            (0..3).map(move |k| {
                                                let (x, y) = (tri[k], tri[(k + 1) % 3]);
                                                (x.min(y), x.max(y))
                                            })
                                        })
                                        .collect();
                                    for (pj, pinfo) in infos.iter().enumerate() {
                                        if pj == h
                                            || pinfo.input != info.input
                                            || pinfo.face_idx != info.face_idx
                                        {
                                            continue;
                                        }
                                        let shared: Vec<(u32, u32)> = patches[pj]
                                            .tris
                                            .iter()
                                            .flat_map(|&t| {
                                                let tri = mesh.tris[t as usize];
                                                (0..3).map(move |k| {
                                                    let (x, y) = (tri[k], tri[(k + 1) % 3]);
                                                    (x.min(y), x.max(y))
                                                })
                                            })
                                            .filter(|e| frag_edges.contains(e))
                                            .collect();
                                        c441_log!(
                                            "[rim-frag]     sibling {pj}: tris={} \
                                             shared_edges={shared:?}",
                                            patches[pj].tris.len(),
                                        );
                                    }
                                    for &t in tris {
                                        c441_log!(
                                            "[rim-frag]     tri {t}: {:?}",
                                            mesh.tris[t as usize]
                                        );
                                    }
                                }
                            }
                        }
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
                    // Attribution order (measured 2026-08-11): a patch that
                    // degenerates while carrying a substitution cannot name
                    // the failure cleanly — block the MERGE pairs first so a
                    // seam the baseline applies is never sacrificed for a
                    // merge's sin; only a merge-free patch drops its seams.
                    // A holder with neither attributable (the 2026-08-10
                    // 18k-restart livelock shape) refuses the whole batch.
                    let required: Vec<u32> = required_by.get(&pi).cloned().unwrap_or_default();
                    if !required.is_empty() {
                        for p in required {
                            c441_log!(
                                "[s4-construct] pass={pass}: CORNER-MERGE BLOCKED v{p} — \
                                 holder {pi} cycle degenerated after merge/removal"
                            );
                            merge_blocked.insert(p);
                        }
                        continue 'assemble;
                    }
                    let trimmed: Vec<u32> = trim_pull.get(&pi).cloned().unwrap_or_default();
                    if !trimmed.is_empty() {
                        for v in trimmed {
                            c441_log!(
                                "[s4-construct] pass={pass}: RIM-TRIM BLOCKED v{v} — \
                                 holder {pi} cycle degenerated after merge/removal"
                            );
                            trim_blocked.insert(v);
                        }
                        continue 'assemble;
                    }
                    let drop: Vec<usize> = active
                        .iter()
                        .copied()
                        .filter(|&ei| eligible[ei].pair.0 == pi || eligible[ei].pair.1 == pi)
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
                    // Last resort: pairs whose p merely sits on the cycles.
                    let incidental: Vec<u32> = subs
                        .iter()
                        .filter(|&(&p, _)| patches[pi].cycles.iter().any(|c| c.contains(&p)))
                        .map(|(&p, _)| p)
                        .collect();
                    if incidental.is_empty() {
                        eprintln!(
                            "[s4-construct] STOP pass={pass}: patch {pi} degenerated with \
                             no attributable seam or merge — refusing the whole batch"
                        );
                        break 'assemble (Vec::new(), std::collections::BTreeMap::new());
                    }
                    for p in incidental {
                        c441_log!(
                            "[s4-construct] pass={pass}: CORNER-MERGE BLOCKED v{p} — \
                             incidental to degenerated holder {pi}"
                        );
                        merge_blocked.insert(p);
                    }
                    continue 'assemble;
                }
            }
            // Dropped vertices: chain interiors + planar flood interiors.
            // A merged corner (subs key) is NOT dropped — the write-back
            // re-points every surviving reference at its q.
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
                        if !kept.contains(&v) && !subs.contains_key(&v) {
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
                                .filter(|&ei| {
                                    eligible[ei].pair.0 == pi || eligible[ei].pair.1 == pi
                                })
                                .collect();
                            if drop.is_empty() {
                                // A merge-only holder: land the refusal on
                                // its merge pairs, then its trim vertices
                                // (the livelock shape otherwise).
                                let trimmed: Vec<u32> =
                                    trim_pull.get(&pi).cloned().unwrap_or_default();
                                let blocked: Vec<u32> =
                                    required_by.get(&pi).cloned().unwrap_or_else(|| {
                                        subs.iter()
                                            .filter(|&(&p, _)| {
                                                patches[pi].cycles.iter().any(|c| c.contains(&p))
                                            })
                                            .map(|(&p, _)| p)
                                            .collect()
                                    });
                                if blocked.is_empty() && trimmed.is_empty() {
                                    eprintln!(
                                        "[s4-construct] STOP pass={pass}: patch {pi} conflict \
                                         with no attributable seam, merge, or trim — refusing \
                                         the whole batch"
                                    );
                                    break 'assemble (
                                        Vec::new(),
                                        std::collections::BTreeMap::new(),
                                    );
                                }
                                if !trimmed.is_empty() {
                                    for v in trimmed {
                                        c441_log!(
                                            "[s4-construct] pass={pass}: RIM-TRIM BLOCKED \
                                             v{v} — holder {pi} dropped-vertex conflict"
                                        );
                                        trim_blocked.insert(v);
                                    }
                                } else {
                                    for p in blocked {
                                        c441_log!(
                                            "[s4-construct] pass={pass}: CORNER-MERGE BLOCKED \
                                             v{p} — holder {pi} dropped-vertex conflict"
                                        );
                                        merge_blocked.insert(p);
                                    }
                                }
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
                            .filter(|&ei| eligible[ei].pair.0 == pi || eligible[ei].pair.1 == pi)
                            .collect();
                        if drop.is_empty() {
                            // A merge-only holder: land the refusal on its
                            // merge pairs, then its trim vertices (the
                            // livelock shape otherwise).
                            let trimmed: Vec<u32> = trim_pull.get(&pi).cloned().unwrap_or_default();
                            let blocked: Vec<u32> =
                                required_by.get(&pi).cloned().unwrap_or_else(|| {
                                    subs.iter()
                                        .filter(|&(&p, _)| {
                                            patches[pi].cycles.iter().any(|c| c.contains(&p))
                                        })
                                        .map(|(&p, _)| p)
                                        .collect()
                                });
                            if blocked.is_empty() && trimmed.is_empty() {
                                eprintln!(
                                    "[s4-construct] STOP pass={pass}: patch {pi} orphan \
                                     conflict with no attributable seam, merge, or trim — \
                                     refusing the whole batch"
                                );
                                break 'assemble (Vec::new(), std::collections::BTreeMap::new());
                            }
                            if !trimmed.is_empty() {
                                for v in trimmed {
                                    c441_log!(
                                        "[s4-construct] pass={pass}: RIM-TRIM BLOCKED v{v} — \
                                         holder {pi} orphaned-vertex conflict"
                                    );
                                    trim_blocked.insert(v);
                                }
                            } else {
                                for p in blocked {
                                    c441_log!(
                                        "[s4-construct] pass={pass}: CORNER-MERGE BLOCKED v{p} — \
                                         holder {pi} orphaned-vertex conflict"
                                    );
                                    merge_blocked.insert(p);
                                }
                            }
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
                        "[s4-verbose] seam={} pair={:?} chain={:?} direct-edge-pre-tris={pre}",
                        e.gi, e.pair, e.chain
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
                match rebuild_patch_planar(mesh, pi, &modp) {
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
                                .filter(|&ei| {
                                    eligible[ei].pair.0 == pi || eligible[ei].pair.1 == pi
                                })
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
                        // Attribution order (measured 2026-08-11, twice): a
                        // declining patch blames the pairs that REQUIRED its
                        // rebuild first — a seam the baseline applies is
                        // never sacrificed for a merge's sin. A merge-free
                        // patch drops its own seams (the baseline path: the
                        // conic-reorder owner 1251 declines ThetaUnwrap at
                        // baseline too — blaming exit pairs whose junctions
                        // merely sit on its rim blocked all 28). Pairs whose
                        // p is incidentally on the cycles are last resort;
                        // with nothing attributable, refuse the whole batch.
                        let required: Vec<u32> = required_by.get(&pi).cloned().unwrap_or_default();
                        let trimmed: Vec<u32> = trim_pull.get(&pi).cloned().unwrap_or_default();
                        if !required.is_empty() {
                            for p in required {
                                c441_log!(
                                    "[s4-construct] pass={pass}: CORNER-MERGE BLOCKED v{p} — \
                                     holder {pi} declined"
                                );
                                merge_blocked.insert(p);
                            }
                        } else if !trimmed.is_empty() {
                            for v in trimmed {
                                c441_log!(
                                    "[s4-construct] pass={pass}: RIM-TRIM BLOCKED v{v} — \
                                     holder {pi} declined"
                                );
                                trim_blocked.insert(v);
                            }
                        } else {
                            let drop: Vec<usize> = active
                                .iter()
                                .copied()
                                .filter(|&ei| {
                                    eligible[ei].pair.0 == pi || eligible[ei].pair.1 == pi
                                })
                                .collect();
                            if drop.is_empty() {
                                let incidental: Vec<u32> = subs
                                    .iter()
                                    .filter(|&(&p, _)| {
                                        patches[pi].cycles.iter().any(|c| c.contains(&p))
                                    })
                                    .map(|(&p, _)| p)
                                    .collect();
                                if incidental.is_empty() {
                                    eprintln!(
                                        "[s4-construct] STOP pass={pass}: patch {pi} declined \
                                         with no attributable seam or merge — refusing the \
                                         whole batch"
                                    );
                                    break 'assemble (
                                        Vec::new(),
                                        std::collections::BTreeMap::new(),
                                    );
                                }
                                for p in incidental {
                                    c441_log!(
                                        "[s4-construct] pass={pass}: CORNER-MERGE BLOCKED \
                                         v{p} — incidental to declined holder {pi}"
                                    );
                                    merge_blocked.insert(p);
                                }
                            }
                            for ei in drop {
                                active.remove(&ei);
                            }
                        }
                        continue 'assemble;
                    }
                }
            }
            break (out, subs);
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
        match apply_rebuild_batch(mesh, attribution, &rebuilds, &subs) {
            Ok(()) => {
                for &ei in &active {
                    let e = &eligible[ei];
                    match &e.action {
                        SeamAction::CollapseLine => c441_log!(
                            "[s4-construct] pass={pass} seam={}: APPLIED patches {}+{} — \
                             chain {} -> 2 verts",
                            e.gi,
                            e.pair.0,
                            e.pair.1,
                            e.chain.len()
                        ),
                        SeamAction::ReorderConic { ordered } => c441_log!(
                            "[s4-construct] pass={pass} seam={}: REORDERED patches {}+{} — \
                             {} verts to curve order",
                            e.gi,
                            e.pair.0,
                            e.pair.1,
                            ordered.len()
                        ),
                        SeamAction::RefineConic { ordered, refined } => c441_log!(
                            "[s4-construct] pass={pass} seam={}: REFINED patches {}+{} — \
                             chain {} -> {} (§4.3.4 density)",
                            e.gi,
                            e.pair.0,
                            e.pair.1,
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
    // untouched, so `infos`/`intersection_curves` stay valid. Inert when
    // the insert gate is off (nothing is ever appended).
    if insert_enabled && mesh.verts.len() > verts_floor {
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
fn census_j1_boundary_exit(
    mesh: &Mesh,
    patches: &[crate::stage4_splice::SplicePatch],
    attribution: &TriangleAttributionMap,
    intersection_curves: &std::collections::BTreeMap<(u32, u32), Curve>,
    gi: usize,
    pair: (usize, usize),
    chain: &[u32],
) {
    use crate::stage4_construct::on_segment_interior;
    use std::collections::BTreeSet;

    let (e0, e1) = (chain[0], *chain.last().expect("chain is non-empty"));
    let chain_set: BTreeSet<u32> = chain.iter().copied().collect();
    let mut cands: Vec<u32> = Vec::new();
    for &pi in &[pair.0, pair.1] {
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

    c441_log!("[j1-census] seam={gi} pair={pair:?} chain={chain:?}");
    c441_log!("[j1-census]   owner {}", desc(pair.0));
    c441_log!("[j1-census]   owner {}", desc(pair.1));
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
fn census_construct_decline(
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
                if s4_pre_pos_diagnostic() {
                    eprintln!(
                        "YANG_S5_MOVED_SET n_moved={n_moved} n_verts={} collapsed={collapsed} \
                         (pre-Stage-4 positions, re-keyed through every compaction)",
                        pre.len(),
                    );
                }
                S4_PRE_POS.with(|c| *c.borrow_mut() = Some(map));
            } else if s4_pre_pos_diagnostic() {
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
        // §4.4.1 AS WRITTEN (spec `specs/yang_441_trim_cdt_construction.md`):
        // the unconditional curve-seam construction, per PATCH with ALL its
        // curves. ALWAYS-ON since the I3 flip (2026-08-15): the gate-ON
        // corpus measured category-identical to gate-OFF (259C/0W/49E/0T,
        // same 49-case ERROR set) under the always-on rim refinement, so the
        // per-wall-class flip census is satisfied for every class at once.
        // The historical `YANG_441_CONSTRUCT` env var now only re-enables
        // the pass's diagnostic chatter (`c441_verbose`); sub-gates
        // (`YANG_441_CORNER_MERGE`, `YANG_441_INPUT_REFINE`,
        // `YANG_441_BOUNDARY_EXIT`, `YANG_441_RIM_TRIM`) keep their own
        // opt-in reads.
        // I13c alternation (gated `YANG_441_ONCURVE_MERGE`): the construct
        // pass's ReorderConic and the Fig-11 merge repair INTERLEAVED defects
        // on the same chains — a reorder is refused while a terminal-overrun
        // vertex mis-roots the seam (`SeamEndpointsReordered`, R0003: 496
        // declines), and a merge's fan CDT is refused while neighbouring
        // interior crossings corrupt the fan polygon. Each round of merges
        // frees endpoints for the next round of reorders and vice versa, so
        // the pair runs to a joint fixed point, bounded. Gate off = exactly
        // one construct + one fold-merge invocation, byte-identical.
        const ALTERNATION_CAP: usize = 8;
        for round in 0..ALTERNATION_CAP {
            let c_applied = run_construct_passes(
                mesh,
                attribution,
                a,
                b,
                &mut infos,
                &mut intersection_curves,
                &mut relocations,
            )?;
            // §4.4.1 Fig-11(b)→(c) — merge the boundary vertices the
            // relocation overran. Placed AFTER the construct pass so it sees
            // the final cycles (a collapsed/refined seam changes which
            // vertices are on a boundary at all), and so a gate-off run is
            // byte-identical.
            let m_applied = if fold_merge_enabled() {
                run_fold_merge_passes(
                    mesh,
                    attribution,
                    a,
                    b,
                    &mut infos,
                    &mut intersection_curves,
                    &mut relocations,
                )?
            } else {
                0
            };
            if !crate::stage4_fold_risk::oncurve_merge_enabled() {
                break; // today's single sequence — the alternation is gated
            }
            let _ = c_applied; // consumed by this round's own fold-merge
            if m_applied == 0 {
                // The merge runs LAST in a round, so only ITS changes can
                // unlock the next round's reorders; a round with no merges
                // is the joint fixed point.
                break;
            }
            if round + 1 == ALTERNATION_CAP {
                c441_log!(
                    "[s4-construct] alternation CAP reached ({ALTERNATION_CAP} rounds) —                      leaving the residue to the loud downstream walls"
                );
            }
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

    // N50's f32 render-twin weld arm stood here until the §4.4.1 epic's I4-1
    // (2026-08-15) REMOVED it from the production path entirely: the sole
    // confirmed hack of the retired weld family (a non-geometric f32-render-
    // precision identity, nowhere in the paper, regresses C0036), redundant
    // since the N56 §4.3 dedup recovers its cases, and default-off since the
    // N55/N56 audit. `weld_f32_render_twins` survives as a unit-tested banked
    // primitive (`tests_unit/n50_f32_render_twin.rs`); history:
    // `docs/yang_deviations.md` §N50.

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

    emit_topology(
        mesh,
        &infos,
        &intersection_curves,
        &relocations,
        op,
        (&a.faces, &a.edges),
        (&b.faces, &b.edges),
    )
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
pub(crate) fn orient_directed_curve(curve: Curve, s: u32, e: u32, verts: &[Point3]) -> Curve {
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
    input_a: (&[BRepFace], &[BRepEdge]),
    input_b: (&[BRepFace], &[BRepEdge]),
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

            // E2 degenerate-loop guard: each cycle must be able to define an
            // orientation — its Newell area vector must not vanish RELATIVE
            // TO THE LOOP'S OWN EXTENT (`loop_is_degenerate`, the scale-free
            // `DEGENERACY_IDENTITY_REL` identity shared with the Stage-4
            // triangle gates). Formerly the ABSOLUTE `MIN_FEATURE_SIZE²`
            // Newell floor, which at micro model scale rejected healthy kept
            // faces (R0047 face 367: a 2.3e-6 × 1.2e-7 quad, ratio 0.086) —
            // a mesh loop is not a model feature. The genuine failure this
            // gate exists for — a Newell-cancelling figure-eight (C0058's
            // tangency neck, ratio 6e-16) — stays loud at every scale.
            for cycle in cycles {
                let cycle_pts = || {
                    cycle
                        .iter()
                        .map(|&(s, _)| mesh.verts[s as usize].as_array())
                };
                if crate::loop_is_degenerate(cycle_pts()) {
                    let ratio = crate::loop_degeneracy_ratio(cycle_pts());
                    if std::env::var_os("NONMANIFOLD_SITE_PROBE").is_some() {
                        eprintln!(
                            "NONMANIFOLD_SITE_PROBE s6-curved-degenerate-loop geometry: face {face_idx} verts {:?}",
                            cycle.iter().map(|&(s, _)| (s, mesh.verts[s as usize])).collect::<Vec<_>>()
                        );
                    }
                    return Err(non_manifold_at(
                        "s6-curved-degenerate-loop",
                        format_args!(
                            "face {face_idx} cycle len {} |N|/extent²={ratio:.3e}",
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
            // E2: degenerate loop — the Newell area vector vanishes RELATIVE
            // TO THE LOOP'S OWN EXTENT (`loop_degeneracy_ratio`, the shared
            // scale-free `DEGENERACY_IDENTITY_REL` identity; formerly the
            // absolute `MIN_FEATURE_SIZE²` floor — see the curved branch).
            let cycle_pts = || {
                cycle
                    .iter()
                    .map(|&(s, _)| mesh.verts[s as usize].as_array())
            };
            if crate::loop_is_degenerate(cycle_pts()) {
                let ratio = crate::loop_degeneracy_ratio(cycle_pts());
                return Err(non_manifold_at(
                    "s6-planar-degenerate-loop",
                    format_args!(
                        "face {face_idx} cycle len {} |N|/extent²={ratio:.3e}",
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
                    // The crossing's OWN vertices, with each one's Stage-4
                    // status. `first_cross=(i, j)` is a pair of segment
                    // indices, which names WHERE the loop crosses but not WHAT
                    // is at the crossing — and every repair has to act on the
                    // vertices, not the indices. Reported as
                    // `seg_i: v(status) -> v(status) x seg_j: ...`, where the
                    // status is the displacement, `still`, or `new` for a
                    // vertex Stage 4 minted (no pre position).
                    let cross_verts = s
                        .first_crossing
                        .map(|(i, j)| {
                            let at = |k: usize| -> u32 { cyc[k % cyc.len()].0 };
                            let tag = |v: u32| -> String {
                                S4_PRE_POS
                                    .with(|c| {
                                        c.borrow().as_ref().map(|m| match m.get(&v) {
                                            None => "new".to_string(),
                                            Some(&q) => {
                                                let w = mesh.verts[v as usize].as_array();
                                                let d = ((w[0] - q[0]).powi(2)
                                                    + (w[1] - q[1]).powi(2)
                                                    + (w[2] - q[2]).powi(2))
                                                .sqrt();
                                                if d == 0.0 {
                                                    "still".to_string()
                                                } else {
                                                    format!("{d:.3e}")
                                                }
                                            }
                                        })
                                    })
                                    .unwrap_or_else(|| "?".to_string())
                            };
                            format!(
                                "seg{i}:v{}({})->v{}({}) X seg{j}:v{}({})->v{}({})",
                                at(i),
                                tag(at(i)),
                                at(i + 1),
                                tag(at(i + 1)),
                                at(j),
                                tag(at(j)),
                                at(j + 1),
                                tag(at(j + 1)),
                            )
                        })
                        .unwrap_or_else(|| "-".to_string());
                    eprintln!(
                        "[s6-simplicity] face={face_idx} input={:?} cycle={ci} \
                         role={} len={} cross={} touch={} spike={} degen={} \
                         min_seg={:.4e} max_seg={:.4e} max_s4_disp={disp_s} \
                         disp_over_min_seg={ratio_s} cross_pre={cross_pre_s} \
                         class={class_s} n_moved={moved_s} trunc_t={trunc_s} \
                         first_cross={:?} at={cross_verts}",
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

    // §4.4.2 carried-edge curve restoration (gated `YANG_434_OUT=1`; spec
    // `yang_434_output_chord_refinement.md` inc-1, revised by the inc-0
    // census): re-type same-input boundary chords onto their carried input
    // circles so the merge below can coalesce them and kernel-v2 samples
    // them at render density. Pure in-place re-typing; gate off (or nothing
    // certified) leaves emission byte-identical.
    if crate::stage5_output_refine::restore_gate_enabled() {
        let st = crate::stage5_output_refine::restore_carried_edge_curves(
            &mesh.verts,
            &mut edges,
            &faces,
            &face_attribution,
            input_a,
            input_b,
        );
        c441_log!(
            "[s434-restore] eligible={} typed={} no_cand={} off={} ambig={} sweep={} mid={}",
            st.eligible,
            st.typed_chords,
            st.no_candidate,
            st.declined_offcurve,
            st.declined_ambiguous,
            st.declined_sweep,
            st.declined_midpoint
        );
    }
    // I5-1b (ALWAYS-ON since the I5-2 flip; `YANG_434_MERGE=0|off` is the
    // dev off-knob; spec §4-I5-1b/§4-I5-2): coalesce per-segment
    // conic seam runs into single analytic arc edges — the paper's §4.4.2
    // B-Rep output shape ("surfaces and their boundary curves"). Certifying
    // and self-declining; gate-off touches nothing.
    if crate::stage5_seam_merge::merge_gate_enabled() {
        let st = crate::stage5_seam_merge::merge_conic_seam_runs(
            &mesh.verts,
            &mut edges,
            &mut faces,
            &mut sources,
        );
        c441_log!(
            "[s6-merge] runs={} elided={} edges {}->{} declines: offcurve={} \
             nonmono={} short={} param={} sweep={} disc_loops={}",
            st.runs_merged,
            st.verts_elided,
            st.edges_before,
            st.edges_after,
            st.declined_offcurve,
            st.declined_nonmonotone,
            st.declined_short,
            st.declined_param,
            st.declined_sweep,
            st.skipped_discontinuous_loops
        );
    }
    // Output-chord census (`YANG_434_OUT=census`, read-only, apply off;
    // spec `yang_434_output_chord_refinement.md` inc-1): every untyped
    // seam chord's owner class, depth, and carried input-circle match —
    // the measurement that moved this family's owner from §4.3.4 to
    // §4.4.2 restoration.
    crate::stage5_output_refine::census_output_pair_chords(
        &mesh.verts,
        &edges,
        &faces,
        &face_attribution,
        intersection_curves,
        input_a,
        input_b,
    );
    // Read-only output-incidence probe (env `YANG_OUT_INCIDENCE_PROBE`,
    // 2026-08-19 R0047 anchor): for every conic output edge, the endpoint
    // residual against the STORED curve (what kernel-v2's import gate
    // measures), whether the endpoint carries a Stage-4 relocation record,
    // and whether the record still reproduces the position (`conic_eval` at
    // the recorded `t` vs the vertex) — a mismatch means the vertex was
    // moved AFTER its relocation onto this curve; a missing record means it
    // was never relocated onto it. Prints only endpoints beyond
    // `TAU_EVAL·(1+extent)`.
    if std::env::var_os("YANG_OUT_INCIDENCE_PROBE").is_some() {
        let reloc_of = |v: u32| {
            relocations
                .iter()
                .find(|&&(rv, _)| rv == v)
                .map(|&(_, t)| t)
        };
        for (ei, e) in edges.iter().enumerate() {
            let is_conic = matches!(e.curve, Curve::Circle { .. } | Curve::Ellipse { .. });
            if !is_conic {
                continue;
            }
            for v in [e.start, e.end] {
                let p = mesh.verts[v as usize];
                let Some(t) = crate::conic_param(&e.curve, p) else {
                    continue;
                };
                let Some(q) = crate::geom::conic_eval(&e.curve, t) else {
                    continue;
                };
                let (pa, qa) = (p.as_array(), q.as_array());
                let resid =
                    ((pa[0] - qa[0]).powi(2) + (pa[1] - qa[1]).powi(2) + (pa[2] - qa[2]).powi(2))
                        .sqrt();
                let extent = pa.iter().fold(0.0f64, |m, c| m.max(c.abs()));
                if resid <= cad_primitives::TAU_EVAL * (1.0 + extent) {
                    continue;
                }
                let rec = reloc_of(v);
                let rec_resid = rec
                    .and_then(|t| crate::geom::conic_eval(&e.curve, t))
                    .map(|r| {
                        let ra = r.as_array();
                        ((pa[0] - ra[0]).powi(2)
                            + (pa[1] - ra[1]).powi(2)
                            + (pa[2] - ra[2]).powi(2))
                        .sqrt()
                    });
                let incident: Vec<String> = edges
                    .iter()
                    .enumerate()
                    .filter(|(_, o)| o.start == v || o.end == v)
                    .map(|(oi, o)| {
                        let n = match o.curve {
                            Curve::LineSegment => "Line",
                            Curve::Circle { .. } => "Circle",
                            Curve::Ellipse { .. } => "Ellipse",
                            Curve::Parabola { .. } => "Parabola",
                            Curve::Hyperbola { .. } => "Hyperbola",
                            Curve::SurfacePair { .. } => "SurfacePair",
                        };
                        format!("e{oi}:{n}")
                    })
                    .collect();
                eprintln!(
                    "YANG_OUT_INCIDENCE_PROBE edge {ei} v={v} p={pa:?} resid={resid:.3e} rel={:.3e} \
                     reloc_record={rec:?} record_vs_pos={rec_resid:?} incident=[{}] curve={:?}",
                    resid / (1.0 + extent),
                    incident.join(","),
                    e.curve
                );
            }
        }
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

/// A mesh triangle is a degenerate (zero-area) sliver when it is numerically
/// collinear RELATIVE TO ITS OWN EXTENT — `tri_is_degenerate`, the scale-free
/// `DEGENERACY_IDENTITY_REL` identity shared with the Stage-4 degeneracy
/// gates and the attribution's degenerate branch (one metric, one
/// definition). Formerly the absolute `MIN_FEATURE_SIZE²` twice-area floor,
/// which at micro model scale classified healthy small triangles as slivers.
/// The exact arrangement keeps genuine slivers along shared collinear solid
/// edges for watertightness; spec `yang_stage6_sliver_topology` §4A excludes
/// them from boundary derivation.
pub(crate) fn triangle_is_degenerate(mesh: &Mesh, t: u32) -> bool {
    let tri = &mesh.tris[t as usize];
    crate::tri_is_degenerate(
        mesh.verts[tri[0] as usize].as_array(),
        mesh.verts[tri[1] as usize].as_array(),
        mesh.verts[tri[2] as usize].as_array(),
    )
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
