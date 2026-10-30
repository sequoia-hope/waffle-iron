#[allow(clippy::wildcard_imports)]
use super::*;

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
    // Exact-coincident OUTPUT-vertex census (read-only, `YANG_COINCIDENT_PROBE`).
    // Output vertices are 1:1 with `mesh.verts` (§(1) just below), so a
    // DISTINCT pair at bit-identical positions leaves the pipeline as two
    // B-Rep vertices at one point — the honest image of a PINCHED face, which
    // the next boolean's Stage-1 chart cannot carry (`Stage1SelfTouchingLoop`,
    // P0020 / N74). Every sub-resolution collapse deliberately excludes the
    // exact-zero pair (KV15b B3, #194 B3), so no gate names it. Here the
    // census is extended with each coincident vertex's RELOCATION tag and
    // Stage-4 pre-position: pinch-split copies carry the original's tag, which
    // is what identifies the group as a split rather than two independent
    // arrivals.
    crate::stage4_correct::coincident_vertex_census("s6-emit", mesh);
    if std::env::var_os("YANG_COINCIDENT_PROBE").is_some() {
        let mut by_pos: std::collections::BTreeMap<[u64; 3], Vec<u32>> =
            std::collections::BTreeMap::new();
        for (v, p) in mesh.verts.iter().enumerate() {
            let a = p.as_array();
            by_pos
                .entry([a[0].to_bits(), a[1].to_bits(), a[2].to_bits()])
                .or_default()
                .push(v as u32);
        }
        for g in by_pos.values().filter(|g| g.len() > 1) {
            let reloc: Vec<(u32, f64)> = relocations
                .iter()
                .copied()
                .filter(|&(v, _)| g.contains(&v))
                .collect();
            let pre: Vec<(u32, Option<[f64; 3]>)> = g
                .iter()
                .map(|&v| {
                    (
                        v,
                        S4_PRE_POS.with(|c| c.borrow().as_ref().and_then(|m| m.get(&v).copied())),
                    )
                })
                .collect();
            eprintln!("[coincident s6-emit]   {g:?} relocations {reloc:?} pre-positions {pre:?}");
        }
    }
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
    for (info_index, info) in infos.iter().enumerate() {
        let cycles = &subdivided_cycles[info_index];
        let inherited = info.inherited;
        let face_idx = info.face_idx;
        let info_attr = TriangleAttribution {
            input: info.input,
            face: info.face_idx as u32,
        };
        // §4.5 T5: an emission STOP names this patch's input face.
        let _scope = crate::boolean::failures::nonmanifold_scope(info.input, info.face_idx as u32);

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

            // Empty-cycles guard (PR-CF1 case#23): a kept curved patch with no
            // boundary cycle cannot form a bounded face; refuse loudly,
            // mirroring the E2/E3 degenerate-reassembly guards. Without this,
            // the `cycles[outer_idx]` index below panics on the empty set.
            //
            // The common customer is a WHOLE closed sphere/torus face — a shell
            // on its own, which no intersection reached (P0030). The host
            // answers those before the pipeline when NO shell of either operand
            // is reached (`shell_contact_census`, spec
            // `untouched_shell_passthrough.md`); what still arrives here is a
            // clear closed shell beside a touched one (that spec's inc-2) or a
            // patch the meshes failed to cut.
            if cycles.is_empty() {
                if std::env::var_os("NONMANIFOLD_SITE_PROBE").is_some() {
                    eprintln!(
                        "NONMANIFOLD_SITE_PROBE s6-curved-empty-cycles detail: face {face_idx} \
                         input {:?} surface {inherited:?} fold_sliver {} of {} patches",
                        info.input,
                        info.had_fold_sliver,
                        infos.len()
                    );
                }
                return Err(non_manifold_at(
                    "s6-curved-empty-cycles",
                    format_args!("face {face_idx}"),
                ));
            }

            // Loop assignment: outer = the cycle of LARGEST |Newell area|
            // (the planar branch's own rule, `select_outer_cycle`); ties →
            // most edges → lowest min start-vertex index. All other cycles
            // = inner_loops. Formerly the MOST-EDGES cycle: R0070
            // (2026-09-11) — a cylinder lateral of the gear-cut output whose
            // 226-edge ellipse-chain HOLE outnumbered its 14-edge outer rim
            // loop (chart areas 3.7e-5 vs 7.3e-4) was labelled inside-out,
            // and the next op's holed chart CDT emptied it ("degenerate CDT
            // input"). Edge count is sampling density, not extent.
            let outer_idx = select_outer_cycle(cycles, &mesh.verts, inherited);
            // N100 item 4 census: a non-developable face (sphere, torus) with
            // several cycles takes the most-edges rule; log whether the
            // largest-|Newell-area| cycle agrees.
            if crate::n100_census::on()
                && cycles.len() > 1
                && matches!(inherited, Surface::Sphere { .. } | Surface::Torus { .. })
            {
                let area = |c: &[(u32, u32)]| -> f64 {
                    let m = c.len();
                    let (mut nx, mut ny, mut nz) = (0.0f64, 0.0f64, 0.0f64);
                    for i in 0..m {
                        let p = mesh.verts[c[i].0 as usize].as_array();
                        let q = mesh.verts[c[(i + 1) % m].0 as usize].as_array();
                        nx += (p[1] - q[1]) * (p[2] + q[2]);
                        ny += (p[2] - q[2]) * (p[0] + q[0]);
                        nz += (p[0] - q[0]) * (p[1] + q[1]);
                    }
                    (nx * nx + ny * ny + nz * nz).sqrt()
                };
                let by_area = (0..cycles.len())
                    .max_by(|&i, &j| area(&cycles[i]).total_cmp(&area(&cycles[j])))
                    .unwrap_or(0);
                crate::n100_census::log(
                    4,
                    &format!(
                        "face {face_idx} {} cycles={} outer={outer_idx} by_area={by_area} {}",
                        if matches!(inherited, Surface::Sphere { .. }) {
                            "sphere"
                        } else {
                            "torus"
                        },
                        cycles.len(),
                        if by_area == outer_idx {
                            "agree"
                        } else {
                            "DISAGREE"
                        }
                    ),
                );
            }
            if std::env::var_os("YANG_S6_OUTER_PROBE").is_some() && cycles.len() > 1 {
                let most_edges = (0..cycles.len())
                    .max_by_key(|&i| {
                        (
                            cycles[i].len(),
                            std::cmp::Reverse(cycles[i].iter().map(|&(s, _)| s).min().unwrap_or(0)),
                        )
                    })
                    .unwrap_or(0);
                let mags: Vec<String> = cycles
                    .iter()
                    .map(|c| {
                        let m = c.len();
                        let (mut nx, mut ny, mut nz) = (0.0f64, 0.0f64, 0.0f64);
                        for i in 0..m {
                            let a = mesh.verts[c[i].0 as usize].as_array();
                            let b = mesh.verts[c[(i + 1) % m].0 as usize].as_array();
                            nx += a[1] * b[2] - a[2] * b[1];
                            ny += a[2] * b[0] - a[0] * b[2];
                            nz += a[0] * b[1] - a[1] * b[0];
                        }
                        format!("len={} |N|={:.3e}", m, (nx * nx + ny * ny + nz * nz).sqrt())
                    })
                    .collect();
                eprintln!(
                    "[s6-outer] face {face_idx} surface={:?} outer_by_extent={outer_idx} outer_by_edges={most_edges} {} cycles: {mags:?}",
                    crate::stage4_correct::surface_kind_name(inherited),
                    if outer_idx == most_edges { "AGREE" } else { "DISAGREE" }
                );
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
            "[s434-restore] eligible={} typed={} (ellipses {} hyperbolas {} pairs {}; by surfaces {}) no_cand={} off={} ambig={} sweep={} mid={} ssi_unavailable={}",
            st.eligible,
            st.typed_chords,
            st.typed_ellipses,
            st.typed_hyperbolas,
            st.typed_pairs,
            st.typed_by_surfaces,
            st.no_candidate,
            st.declined_offcurve,
            st.declined_ambiguous,
            st.declined_sweep,
            st.declined_midpoint,
            st.ssi_unavailable
        );
    }
    // I5-1b (ALWAYS-ON since the I5-2 flip; spec §4-I5-1b/§4-I5-2):
    // coalesce per-segment
    // conic seam runs into single analytic arc edges — the paper's §4.4.2
    // B-Rep output shape ("surfaces and their boundary curves"). Certifying
    // and self-declining.
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
    // §4.3.4 chain decimation (ALWAYS-ON; spec
    // `yang_434_output_chord_refinement.md` "Stage-5
    // chain decimation", R0085 2026-09-22): drop the interior vertices of
    // LineSegment / SurfacePair INTERSECTION runs that the paper's own
    // refinement acceptance (h/l/α) calls redundant — the arrangement's
    // grazing-mesh density never reaches the output B-Rep as edge
    // endpoints. Twin-conformant per canonical chain; declines to the
    // per-segment status quo.
    let st = crate::stage5_chain_decimate::decimate_intersection_runs(
        &mesh.verts,
        &mut edges,
        &mut faces,
        &mut sources,
        intersection_curves,
    );
    c441_log!(
        "[s434-decimate] runs={} dropped={} edges {}->{} declined_floor={} disc_loops={}",
        st.runs_decimated,
        st.verts_dropped,
        st.edges_before,
        st.edges_after,
        st.declined_floor,
        st.skipped_discontinuous_loops
    );
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
