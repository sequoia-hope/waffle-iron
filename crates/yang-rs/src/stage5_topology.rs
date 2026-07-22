//! Stage 5/6 — B-Rep topology extraction and emission from the
//! corrected mesh: patch flood fill, boundary cycles, face/loop
//! emission (extracted verbatim from lib.rs — spec
//! `specs/yang_rs_lib_decomposition.md`, increment 8).

#[allow(clippy::wildcard_imports)]
use crate::*;

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
    let (infos, _incidence, intersection_curves) = compute_phase_a(mesh, attribution, a, b)?;
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
pub(crate) fn reconstruct_topology_stage4(
    mesh: &mut Mesh,
    attribution: &mut TriangleAttributionMap,
    a: &BRep,
    b: &BRep,
    op: BoolOp,
    minted_junction_keys: &std::collections::BTreeMap<[u64; 3], crate::boolean::MintProvenance>,
) -> Result<ReconstructedTopology, YangError> {
    // (4) Phase A: per-patch ordered loops + inherited surface (`infos`), and the
    // exact per-edge intersection `Curve` map.
    let (mut infos, incidence, mut intersection_curves) = compute_phase_a(mesh, attribution, a, b)?;

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
    if has_conic {
        let (relocs, collapsed) =
            stage4_relocate_and_correct(mesh, attribution, a, b, minted_junction_keys)?;
        relocations = relocs;
        // A §4.5.3 collapse mutated the mesh topology + attribution, so the
        // pre-collapse Phase-A loops are stale (spec §4.1 note). Recompute them
        // before the Phase-B emission re-validates the corrected mesh.
        if collapsed {
            // PR-YR11: drop the vertices the collapse left unreferenced (and
            // remap triangle indices + `relocations`) BEFORE recomputing Phase A,
            // so the emitted output mesh carries no dangling vertices (a global
            // V−E+F = 2 for a single closed shell). Strict no-op when there were
            // no danglers.
            compact_unreferenced_verts(mesh, &mut relocations);
            let (i2, _inc2, cv2) = compute_phase_a(mesh, attribution, a, b)?;
            infos = i2;
            intersection_curves = cv2;
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
            compact_unreferenced_verts(mesh, &mut relocations);
            let (i3, _inc3, cv3) = compute_phase_a(mesh, attribution, a, b)?;
            infos = i3;
            intersection_curves = cv3;
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
            compact_unreferenced_verts(mesh, &mut relocations);
            let (i4, _inc4, cv4) = compute_phase_a(mesh, attribution, a, b)?;
            infos = i4;
            intersection_curves = cv4;
        }
    }

    // (#173 / N6) §4.5.4 illegal-self-intersection PROBE on the FINAL mesh
    // (verts 1:1 with the emitted output vertices). Gated on
    // `YANG_SELFX_PROBE`, byte-identical when unset. Measurement-first per
    // `specs/yang_173_selfx_detector.md` §4 — the always-on loud STOP ships
    // only after the corpus-wide false-positive measurement passes.
    if std::env::var_os("YANG_SELFX_PROBE").is_some() {
        let t0 = std::time::Instant::now();
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
                                eprintln!(
                                    "  cyc{ci} v={vv} p=({:.6},{:.6},{:.6}) dist={dd:.4e} reloc={reloc:?}",
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
        face_attribution.push(info_attr);
        faces.push(BRepFace {
            surface,
            outer_loop,
            inner_loops,
            reversed: false,
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
