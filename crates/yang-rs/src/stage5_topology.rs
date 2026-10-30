//! Stage 5/6 — B-Rep topology extraction and emission from the
//! corrected mesh: patch flood fill, boundary cycles, face/loop
//! emission (extracted verbatim from lib.rs — spec
//! `specs/yang_rs_lib_decomposition.md`, increment 8).

#[allow(clippy::wildcard_imports)]
use crate::*;

/// Diagnostic only: per-vertex incident surfaces, each with its operand-qualified
/// label (`A:Plane`) and the `Surface` itself so residuals can be evaluated.
pub(super) type VertSurfMap = std::collections::HashMap<u32, Vec<(String, Surface)>>;

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

// The [`S4_PRE_POS`] pre-position map is captured on every run: the §4.4.1
// Fig-11 merge pass ([`run_fold_merge_passes`], ALWAYS-ON since the I6 flip,
// 2026-08-19d, spec §4-I6) reads it as production input, and the
// `YANG_S5_FOLD_PROBE` diagnostic columns read the same map. It is captured at
// one site, RE-KEYED at four compaction sites, and read at several more — the
// `probe_remap_pre_pos` contract. The §4-I8 fan-of-one rebuild and the §4-I8
// carrier-containment guard on that merge are ALWAYS-ON since 2026-08-20
// (byte-identical corpus; the guard is a P10 safety net refusing 7 sites over
// R0011/R0044/R0074/R0085 that would otherwise merge a model CORNER into a curve
// junction). Their `YANG_441_FOLD_MERGE` / `YANG_441_FAN_OF_ONE` /
// `YANG_441_MERGE_CARRIER` off-knobs were deleted 2026-10-16 (cleanup spec §4).

/// Is the pre-position map wanted for REPORTING (as opposed to being wanted at
/// all)? The map's `YANG_S5_MOVED_SET` / `YANG_S5_REMAP` lines are probe output;
/// the Fig-11 merge needs the map itself but not the chatter, so it must not
/// turn a repair pass into a per-boolean stderr writer.
pub(super) fn s4_pre_pos_diagnostic() -> bool {
    std::env::var_os("YANG_S5_FOLD_PROBE").is_some()
}

/// Diagnostic only (`YANG_S5_FOLD_PROBE`): re-key [`S4_PRE_POS`] through a
/// `compact_unreferenced_verts` remap, dropping vertices that did not survive.
///
/// MUST be called at EVERY compaction site. There are four (§4.5.3, KV15b, #194
/// sub-TAU edge collapse, N50 f32 weld), and the last three run even when Stage 4
/// itself did not collapse — so skipping them leaves the map keyed on indices that
/// name different vertices in the emitted loops, which is worse than having no
/// map: every column would still be populated, just wrong.
pub(super) fn probe_remap_pre_pos(site: &str, remap: Option<&Vec<Option<u32>>>) {
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
pub(super) fn probe_record_incidence(
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
pub(super) fn probe_vertex_prov(vv: u32, q: [f64; 3]) -> String {
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
        (&a.vertices, &a.faces, &a.edges),
        (&b.vertices, &b.faces, &b.edges),
    )?;
    Ok((vertices, edges, faces))
}

/// PR-YR10: the Stage-4-aware reconstruction `boolean()` calls. Builds Phase A,
/// runs Stage 4 (relocate intersection points onto the exact curves + §4.5.3
/// reversed-point correction), recomputes Phase A after any §4.5.3 collapse,
/// then runs the SAME Phase-B emission as `reconstruct_topology` (via the shared
/// [`emit_topology`]). Returns the 4-tuple including the per-output-vertex
/// `TessellationSource` vector (relocated verts → `BRepEdge { edge, t }`).
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

/// f2c-3 χ-localization probe gate (`YANG_CHI_AUDIT`, unset = off,
/// byte-identical): prints the id-complex invariants at the stage
/// boundaries (stage-2 kept submesh, stage-4/5 reconstruct entry, each
/// construct/fold-merge alternation round) and reports any single apply
/// that changes them. Built 2026-08-28 to localize R0003's measured
/// main-shell genus-2 (per-component χ [-2, 2, 2] at the rehome applies,
/// which are themselves neutral).
pub(crate) fn chi_audit_enabled() -> bool {
    std::env::var_os("YANG_CHI_AUDIT").is_some()
}

/// Index-complex invariants of a triangle soup, by VERTEX ID: (V, E
/// undirected, F, χ = V−E+F, connected components, per-component χ
/// sorted — the genus distribution, χ_c = 2 − 2g_c per closed
/// component). Pure counting; the composition oracle's position-welded
/// complex is a different instrument.
pub(crate) fn tris_complex_stats(tris: &[[u32; 3]]) -> (usize, usize, usize, i64, usize, Vec<i64>) {
    let mut ids: std::collections::BTreeMap<u32, usize> = Default::default();
    for tri in tris {
        for &v in tri {
            let n = ids.len();
            ids.entry(v).or_insert(n);
        }
    }
    let mut edges: std::collections::BTreeSet<(u32, u32)> = Default::default();
    for tri in tris {
        for k in 0..3 {
            let (x, y) = (tri[k], tri[(k + 1) % 3]);
            edges.insert((x.min(y), x.max(y)));
        }
    }
    let mut parent: Vec<usize> = (0..ids.len()).collect();
    fn find(p: &mut [usize], mut x: usize) -> usize {
        while p[x] != x {
            p[x] = p[p[x]];
            x = p[x];
        }
        x
    }
    for &(a, b) in &edges {
        let (ra, rb) = (find(&mut parent, ids[&a]), find(&mut parent, ids[&b]));
        if ra != rb {
            parent[ra.max(rb)] = ra.min(rb);
        }
    }
    let mut roots: std::collections::BTreeSet<usize> = Default::default();
    for i in 0..ids.len() {
        roots.insert(find(&mut parent, i));
    }
    let mut per: std::collections::BTreeMap<usize, (i64, i64, i64)> = Default::default();
    for &slot in ids.values() {
        let r = find(&mut parent, slot);
        per.entry(r).or_default().0 += 1;
    }
    for &(a, _) in &edges {
        let r = find(&mut parent, ids[&a]);
        per.entry(r).or_default().1 += 1;
    }
    for tri in tris {
        let r = find(&mut parent, ids[&tri[0]]);
        per.entry(r).or_default().2 += 1;
    }
    let mut chis: Vec<i64> = per.values().map(|&(v, e, f)| v - e + f).collect();
    chis.sort_unstable();
    let (v, e, f) = (ids.len(), edges.len(), tris.len());
    (v, e, f, v as i64 - e as i64 + f as i64, roots.len(), chis)
}

/// χ-audit vertex-link scan (no-op unless [`chi_audit_enabled`]): count
/// vertices whose link (the graph of their incident triangles' opposite
/// edges) has more than one connected component — vertex-level PINCHES,
/// the non-manifoldness an all-2 edge-use histogram cannot see. Each
/// pinch of multiplicity m costs χ exactly (m−1), so k simple pinches on
/// an otherwise-clean surface read as χ deficit k. Prints up to 8 with
/// positions — the anchors.
pub(crate) fn chi_audit_pinch_scan(label: &str, tris: &[[u32; 3]], verts: &[Point3]) {
    if !chi_audit_enabled() {
        return;
    }
    let mut inc: std::collections::BTreeMap<u32, Vec<(u32, u32)>> = Default::default();
    for tri in tris {
        for k in 0..3 {
            inc.entry(tri[k])
                .or_default()
                .push((tri[(k + 1) % 3], tri[(k + 2) % 3]));
        }
    }
    let mut pinches = 0usize;
    for (&v, opps) in &inc {
        let mut ids: std::collections::BTreeMap<u32, usize> = Default::default();
        for &(a, b) in opps {
            let n = ids.len();
            ids.entry(a).or_insert(n);
            let n = ids.len();
            ids.entry(b).or_insert(n);
        }
        let mut parent: Vec<usize> = (0..ids.len()).collect();
        fn find(p: &mut [usize], mut x: usize) -> usize {
            while p[x] != x {
                p[x] = p[p[x]];
                x = p[x];
            }
            x
        }
        for &(a, b) in opps {
            let (ra, rb) = (find(&mut parent, ids[&a]), find(&mut parent, ids[&b]));
            if ra != rb {
                parent[ra.max(rb)] = ra.min(rb);
            }
        }
        let mut roots: std::collections::BTreeSet<usize> = Default::default();
        for i in 0..ids.len() {
            roots.insert(find(&mut parent, i));
        }
        if roots.len() > 1 {
            pinches += 1;
            if pinches <= 8 {
                let p = verts[v as usize].as_array();
                eprintln!(
                    "[chi-audit] {label}: PINCH v{v} link-components={} fan={} \
                     p=({:.9},{:.9},{:.9})",
                    roots.len(),
                    opps.len(),
                    p[0],
                    p[1],
                    p[2]
                );
            }
        }
    }
    eprintln!("[chi-audit] {label}: {pinches} pinch vertex(es)");
}

/// One χ-audit boundary line (no-op unless [`chi_audit_enabled`]).
/// The edge-USE histogram discriminates a kept spurious MEMBRANE (edges
/// at ≠2 uses — invisible to the distinct-edge χ) from a genuinely
/// closed surface with wrong genus (all-2).
pub(crate) fn chi_audit_report(label: &str, tris: &[[u32; 3]]) {
    if !chi_audit_enabled() {
        return;
    }
    let (v, e, f, chi, comps, chis) = tris_complex_stats(tris);
    let mut uses: std::collections::BTreeMap<(u32, u32), u32> = Default::default();
    for tri in tris {
        for k in 0..3 {
            let (x, y) = (tri[k], tri[(k + 1) % 3]);
            *uses.entry((x.min(y), x.max(y))).or_default() += 1;
        }
    }
    let mut hist: std::collections::BTreeMap<u32, usize> = Default::default();
    for &n in uses.values() {
        *hist.entry(n).or_default() += 1;
    }
    eprintln!(
        "[chi-audit] {label}: V({v}) - E({e}) + F({f}) = {chi} components={comps} \
         per-component-chi={chis:?} edge-use-hist={hist:?}"
    );
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

mod construct;
mod emit;
mod fold_merge;
mod patches;

use construct::*;
pub(crate) use emit::*;
use fold_merge::*;
pub(crate) use patches::*;

#[allow(clippy::too_many_arguments)]
pub(crate) fn reconstruct_topology_stage4(
    mesh: &mut Mesh,
    attribution: &mut TriangleAttributionMap,
    side_labels: &mut crate::brep::TriangleSideLabelMap,
    a: &BRep,
    b: &BRep,
    op: BoolOp,
    minted_junction_keys: &std::collections::BTreeMap<[u64; 3], crate::boolean::MintProvenance>,
    edge_provenance: &crate::stage3_ssi::PosKeyedEdgeSet,
) -> Result<ReconstructedTopology, YangError> {
    debug_assert_eq!(
        side_labels.len(),
        mesh.tris.len(),
        "reconstruct_topology_stage4: side_labels ({}) must start 1:1 with mesh.tris ({})",
        side_labels.len(),
        mesh.tris.len(),
    );
    chi_audit_report("s4-reconstruct entry", &mesh.tris);
    crate::stage4_correct::mesh_dump_probe("s4-reconstruct entry", mesh, attribution);
    // §I13(f) f2c-3 — the read-only BRIDGE CENSUS (`YANG_441_SLIT`, unset =
    // byte-identical): lift kept triangles onto their analytic carriers and
    // test the lifts against the tool half-spaces. Its 2026-08-28
    // measurement REFUTED the slit repair (R0003's genus-2 is TRUE
    // topology — see stage4_slit's module docs); the instrument stays as
    // the future handle-certificate home.
    if crate::stage4_slit::slit_mode() != crate::stage4_slit::SlitMode::Off {
        crate::stage4_slit::bridge_census(mesh, attribution, a, b, op);
    }
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
    // Positional snapshot: the pre-Stage-4 position map the Fig-11 merge
    // pass reads (and the S4_MOVED oracle prints).
    let verts_pre: Option<Vec<Point3>> = if has_conic {
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
            side_labels,
            a,
            b,
            minted_junction_keys,
            edge_provenance,
        )?;
        crate::stage4_correct::s4_lap("s4-wrapper-returned", mesh);
        relocations = relocs;
        // A §4.5.3 collapse mutated the mesh topology + attribution, so the
        // pre-collapse Phase-A loops are stale (spec §4.1 note). Recompute them
        // before the Phase-B emission re-validates the corrected mesh.
        // Snapshot the pre-Stage-4 positions BEFORE the first compaction can
        // renumber. `collapse_vertex` itself never touches `mesh.verts` (it
        // rewrites triangle indices and drops degenerate tris), so indices are
        // still 1:1 with `verts_pre` at this point even when `collapsed`.
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
        if collapsed {
            // PR-YR11: drop the vertices the collapse left unreferenced (and
            // remap triangle indices + `relocations`) BEFORE recomputing Phase A,
            // so the emitted output mesh carries no dangling vertices (a global
            // V−E+F = 2 for a single closed shell). Strict no-op when there were
            // no danglers.
            let remap = compact_unreferenced_verts(mesh, &mut relocations, &side_labels.labels);
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
            crate::stage4_correct::coincident_vertex_census("s5-s453", mesh);
            probe_record_incidence(&inc2, &cv2);
            infos = i2;
            intersection_curves = cv2;
        }

        // §4.4.1 AS WRITTEN (spec `specs/yang_441_trim_cdt_construction.md`):
        // the unconditional curve-seam construction, per PATCH with ALL its
        // curves. ALWAYS-ON since the I3 flip (2026-08-15): the gate-ON
        // corpus measured category-identical to gate-OFF (259C/0W/49E/0T,
        // same 49-case ERROR set) under the always-on rim refinement, so the
        // per-wall-class flip census is satisfied for every class at once.
        // The historical `YANG_441_CONSTRUCT` env var now only re-enables
        // the pass's diagnostic chatter (`c441_verbose`).
        // I13c alternation (always on): the construct
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
                side_labels,
                a,
                b,
                &mut infos,
                &mut intersection_curves,
                &mut relocations,
                false,
                None,
            )?;
            if chi_audit_enabled() {
                chi_audit_report(&format!("round={round} post-construct"), &mesh.tris);
            }
            // §4.4.1 Fig-11(b)→(c) — merge the boundary vertices the
            // relocation overran. Placed AFTER the construct pass so it sees
            // the final cycles (a collapsed/refined seam changes which
            // vertices are on a boundary at all).
            let m_applied = run_fold_merge_passes(
                mesh,
                attribution,
                side_labels,
                a,
                b,
                &mut infos,
                &mut intersection_curves,
                &mut relocations,
                false,
                None,
            )?;
            if chi_audit_enabled() {
                chi_audit_report(&format!("round={round} post-fold-merge"), &mesh.tris);
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
                &mut side_labels.labels,
                &intersection_curves,
                a,
                b,
            );
            attribution.attributions = attr_vec;
            c
        };
        if kv15b_collapsed {
            let remap = compact_unreferenced_verts(mesh, &mut relocations, &side_labels.labels);
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
            crate::stage4_correct::coincident_vertex_census("s5-kv15b", mesh);
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
            let c = collapse_subtauwork_mesh_edges(mesh, &mut attr_vec, &mut side_labels.labels);
            attribution.attributions = attr_vec;
            c
        };
        if s194_collapsed {
            let remap = compact_unreferenced_verts(mesh, &mut relocations, &side_labels.labels);
            let (i4, inc4, cv4) = compute_phase_a(
                mesh,
                attribution,
                a,
                b,
                &crate::stage3_ssi::NO_EDGE_PROVENANCE,
            )?;
            probe_remap_pre_pos("s194", remap.as_ref());
            crate::stage4_correct::coincident_vertex_census("s5-s194", mesh);
            probe_record_incidence(&inc4, &cv4);
            infos = i4;
            intersection_curves = cv4;
        }
        // F2 / N81 (§4.4.1 curve-chain decimation, Yang et al. 2025 §4.3.4 / §4.4.1,
        // `refs/text/yang2025_hybrid_boolean.txt:575-593, 546-572`): apply forward §4.3.4
        // along LineSegment / SurfacePair intersection polylines (and sub-resolution
        // collinear straight-edge subdivisions, ledger N105) inside Stage 4 and update
        // every owner patch's triangulation conformally before topology emission.
        let dec_st = crate::stage4_chain_decimate::decimate_patch_chains_441(
            mesh,
            attribution,
            side_labels,
            &infos,
            &intersection_curves,
            a,
            b,
            false,
        )?;
        c441_log!(
            "[s434-decimate] runs={} dropped={} edges {}->{} declined_loop_floor={} declined_disc_patch={} declined_sphere={} declined_rebuild={} disc_loops={}",
            dec_st.runs_decimated,
            dec_st.verts_dropped,
            dec_st.edges_before,
            dec_st.edges_after,
            dec_st.declined_loop_floor,
            dec_st.declined_discontinuous_patch,
            dec_st.declined_sphere_unsupported,
            dec_st.declined_patch_rebuild,
            dec_st.skipped_discontinuous_loops
        );
        if dec_st.verts_dropped > 0 {
            let remap = compact_unreferenced_verts(mesh, &mut relocations, &side_labels.labels);
            let (i5, inc5, cv5) = compute_phase_a(
                mesh,
                attribution,
                a,
                b,
                &crate::stage3_ssi::NO_EDGE_PROVENANCE,
            )?;
            probe_remap_pre_pos("s434-dec", remap.as_ref());
            crate::stage4_correct::coincident_vertex_census("s5-s434-dec", mesh);
            probe_record_incidence(&inc5, &cv5);
            infos = i5;
            intersection_curves = cv5;
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
    // N55/N56 audit. History: `docs/yang_deviations.md` §N50.

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
                .take(256)
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

    // F1-I1 (`specs/yang_442_classify_after_update.md` §5 I1; Yang §4.4.2,
    // `refs/text/yang2025_hybrid_boolean.txt:572-578`): at the Stage-4 hand-off
    // to topology emission, `side_labels` must remain 1:1 with `mesh.tris` and
    // `attribution` and re-derive today's kept set `(0..mesh.tris.len())`
    // identically under `op`.
    debug_assert_eq!(
        side_labels.len(),
        mesh.tris.len(),
        "F1-I1 hand-off: side_labels ({}) drifted out of lockstep with mesh.tris ({})",
        side_labels.len(),
        mesh.tris.len(),
    );
    debug_assert_eq!(
        attribution.attributions.len(),
        mesh.tris.len(),
        "F1-I1 hand-off: attribution ({}) drifted out of lockstep with mesh.tris ({})",
        attribution.attributions.len(),
        mesh.tris.len(),
    );
    debug_assert_eq!(
        side_labels.keep_set(op),
        (0..mesh.tris.len()).collect::<Vec<_>>(),
        "F1-I1 hand-off: carried side labels failed to re-derive today's kept set for {op:?}",
    );

    emit_topology(
        mesh,
        &infos,
        &intersection_curves,
        &relocations,
        op,
        (&a.vertices, &a.faces, &a.edges),
        (&b.vertices, &b.faces, &b.edges),
    )
}
