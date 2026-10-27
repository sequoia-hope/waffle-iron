//! Stage 4 — mesh correction: Phase-A patch census, vertex collapse,
//! sub-resolution segment collapse, relocation application + reversal
//! sweeps, relocated-triangle validation (extracted verbatim from
//! lib.rs — spec `specs/yang_rs_lib_decomposition.md`, increment 7).

#[allow(clippy::wildcard_imports)]
use crate::*;

/// §4.4.2 tangent-plane corridor half-width. The two surfaces at a
/// relocation site, linearized as tangent planes P_A, P_B meeting at angle
/// θ, admit a corridor of `2·budget/sinθ` around L = P_A ∩ P_B — the
/// Stage-1 budget mapped through the wedge (paper
/// refs/text/yang2025_hybrid_boolean.txt:494-537). `divergence` is the
/// unit-vector cross-product magnitude (= sin θ), or the gradient magnitude
/// for implicit forms. At exact tangency the corridor is unbounded →
/// `INFINITY` (the circle-junction gate precedent: the projection is still
/// the local nearest point; callers that need a finite band gate on a
/// tangency cutoff FIRST). Extracted from five duplicated sites (design
/// review 2026-07-12 F8) — the formula must never be re-inlined: a future
/// correction has to land HERE once.
pub(crate) fn tangent_plane_corridor(budget: f64, divergence: f64) -> f64 {
    if divergence > 0.0 {
        2.0 * budget / divergence
    } else {
        f64::INFINITY
    }
}

// =========================================================================
// PR-YR5 — topology reconstruction
// =========================================================================

/// PR-YR5 internal: the triple `(vertices, edges, faces)` produced
/// by `reconstruct_topology` to populate the output `BRep`.
///
/// PR-YR10: extended with a fourth component — the per-output-mesh-vertex
/// `Vec<TessellationSource>` (default `BRepVertex(i)`, overridden to
/// `BRepEdge { edge, t }` for Stage-4-relocated intersection vertices).
pub(crate) type ReconstructedTopology = (
    Vec<BRepVertex>,
    Vec<BRepEdge>,
    Vec<BRepFace>,
    Vec<TessellationSource>,
    // PR-KV13 F2: per-output-face attribution, parallel to `faces` — the
    // `(input, face)` the patch descends from (the kernel maps it to the
    // operand's persistent face id for boolean provenance).
    Vec<TriangleAttribution>,
);

/// PR-YR5/9 `(vertices, edges, faces)` triple — the pre-PR-YR10 reconstruction
/// shape retained for the `#[cfg(test)]` unit-test callers.
#[cfg(test)]
pub(crate) type LegacyTopology = (Vec<BRepVertex>, Vec<BRepEdge>, Vec<BRepFace>);

/// PR-YR9 (lifted to module scope in PR-YR10 so `stage4_relocate_and_correct`
/// can consume the same ordered, oriented patch loops + inherited surface that
/// the Phase-B emission uses — no re-derivation, no classification drift).
pub(crate) struct PatchInfo {
    pub(crate) cycles: Vec<Vec<(u32, u32)>>,
    pub(crate) input: InputId,
    pub(crate) inherited: Surface,
    pub(crate) face_idx: usize,
    /// The INPUT face's cavity sense (PR-KV6b-1): a kept patch of an
    /// already-reversed input wall (e.g. a washer's inner tube) must keep
    /// its sense in the output — composed by XOR with the Subtract-B flip.
    pub(crate) input_reversed: bool,
    /// Spec yang_stage6_sliver_topology §2/§4B: this patch contained ≥1 FOLD
    /// sliver that §4A excluded from boundary derivation (`patch_fold_slivers`).
    /// Such a patch may carry a whole shared solid edge as ONE un-subdivided
    /// chord (the collapsed subdivision the slivers used to represent), so it
    /// — and ONLY it — is eligible for the §4B loop T-subdivision. Patches
    /// with no excluded fold sliver keep byte-identical loops (the measured
    /// chord lives on the fold-bearing side; the other side already
    /// subdivides), which keeps curved / benign-T-junction output at exact
    /// reference parity.
    pub(crate) had_fold_sliver: bool,
}

/// PR-YR10: the Phase-A structures `reconstruct_topology` derives before the
/// Phase-B emission: per-patch ordered loops + inherited surface (`infos`), the
/// edge→incident-(input,surface) map (`incidence`), and the exact per-edge
/// analytical `Curve` map (`curves`). Recomputed after a §4.5.3 collapse.
pub(crate) type PhaseA = (
    Vec<PatchInfo>,
    std::collections::BTreeMap<(u32, u32), Vec<(InputId, Surface)>>,
    std::collections::BTreeMap<(u32, u32), Curve>,
);

/// A `Plane` as its unit-normalized `(n̂, d̂)` — the key `merge_same_plane_patches`
/// merges on. `None` for a non-plane or a degenerate normal.
pub(crate) fn unit_plane(s: Surface) -> Option<([f64; 3], f64)> {
    let Surface::Plane { normal, d } = s else {
        return None;
    };
    let n = normal.as_array();
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if len < cad_primitives::MIN_FEATURE_SIZE {
        return None;
    }
    Some(([n[0] / len, n[1] / len, n[2] / len], d / len))
}

/// PR-YR27's same-plane-same-orientation identity: two unit-normalized planes
/// coincide iff `n̂` agrees component-wise and `d̂` agrees, each within
/// `TAU_WORK`. An opposite-normal pair never coincides. ONE reading for every
/// consumer that asks "is this face on that plane" — the patch merge and the
/// partner-hull containment (R0026, 2026-09-11: the box's bottom cap sat one
/// ULP of `d` from the base cap it had been merged with, so the hull saw only
/// the base cap and refused a junction outside the cylinder's disk).
pub(crate) fn unit_planes_coincide(p: ([f64; 3], f64), q: ([f64; 3], f64)) -> bool {
    (p.1 - q.1).abs() <= cad_primitives::TAU_WORK
        && (0..3).all(|k| (p.0[k] - q.0[k]).abs() <= cad_primitives::TAU_WORK)
}

/// Orientation-ignoring geometric locus equality of two analytic [`Surface`]s
/// within `TAU_WORK`. Used when deduplicating curve-incidence entries at the
/// boundary of a `[true, true]` common-surface sheet (§4.4.2 / §4.5.5): an
/// overlap sheet lies on both operands' geometrically identical surfaces even
/// when their outward normals oppose (`n_A = -n_B, d_A = -d_B`).
pub(crate) fn surfaces_coincide_unoriented(s1: Surface, s2: Surface) -> bool {
    let unit_vec = |v: [f64; 3]| -> Option<[f64; 3]> {
        let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        (len >= cad_primitives::MIN_FEATURE_SIZE).then(|| [v[0] / len, v[1] / len, v[2] / len])
    };
    let parallel = |u: [f64; 3], v: [f64; 3]| -> bool {
        (u[0] * v[0] + u[1] * v[1] + u[2] * v[2]).abs() >= 1.0 - cad_primitives::TAU_WORK
    };
    let pt_close = |p: [f64; 3], q: [f64; 3]| -> bool {
        (0..3).all(|k| (p[k] - q[k]).abs() <= cad_primitives::TAU_WORK)
    };
    let line_anchor = |pt: [f64; 3], axis: [f64; 3]| -> [f64; 3] {
        let t = pt[0] * axis[0] + pt[1] * axis[1] + pt[2] * axis[2];
        [
            pt[0] - t * axis[0],
            pt[1] - t * axis[1],
            pt[2] - t * axis[2],
        ]
    };
    match (s1, s2) {
        (Surface::Plane { .. }, Surface::Plane { .. }) => {
            let (Some(p), Some(q)) = (unit_plane(s1), unit_plane(s2)) else {
                return false;
            };
            unit_planes_coincide(p, q)
                || unit_planes_coincide(p, ([-q.0[0], -q.0[1], -q.0[2]], -q.1))
        }
        (
            Surface::Sphere {
                center: c1,
                radius: r1,
            },
            Surface::Sphere {
                center: c2,
                radius: r2,
            },
        ) => (r1 - r2).abs() <= cad_primitives::TAU_WORK && pt_close(c1.as_array(), c2.as_array()),
        (
            Surface::Cylinder {
                axis_point: p1,
                axis_dir: d1,
                radius: r1,
            },
            Surface::Cylinder {
                axis_point: p2,
                axis_dir: d2,
                radius: r2,
            },
        ) => {
            let (Some(a1), Some(a2)) = (unit_vec(d1.as_array()), unit_vec(d2.as_array())) else {
                return false;
            };
            (r1 - r2).abs() <= cad_primitives::TAU_WORK
                && parallel(a1, a2)
                && pt_close(
                    line_anchor(p1.as_array(), a1),
                    line_anchor(p2.as_array(), a1),
                )
        }
        (
            Surface::Cone {
                apex: p1,
                axis_dir: d1,
                half_angle: h1,
            },
            Surface::Cone {
                apex: p2,
                axis_dir: d2,
                half_angle: h2,
            },
        ) => {
            let (Some(a1), Some(a2)) = (unit_vec(d1.as_array()), unit_vec(d2.as_array())) else {
                return false;
            };
            (h1 - h2).abs() <= cad_primitives::TAU_WORK
                && parallel(a1, a2)
                && pt_close(p1.as_array(), p2.as_array())
        }
        (
            Surface::Torus {
                center: c1,
                axis_dir: d1,
                major_radius: maj1,
                minor_radius: min1,
            },
            Surface::Torus {
                center: c2,
                axis_dir: d2,
                major_radius: maj2,
                minor_radius: min2,
            },
        ) => {
            let (Some(a1), Some(a2)) = (unit_vec(d1.as_array()), unit_vec(d2.as_array())) else {
                return false;
            };
            (maj1 - maj2).abs() <= cad_primitives::TAU_WORK
                && (min1 - min2).abs() <= cad_primitives::TAU_WORK
                && parallel(a1, a2)
                && pt_close(c1.as_array(), c2.as_array())
        }
        _ => false,
    }
}

/// The torus block's owner-face containment reading, EXTRACTED (§4.5.1
/// inc-3) so the torus-region repair applies the SAME acceptance its gate
/// uses — one reading, two callers, no drift.
///
/// For a PLANAR `partner` surface: a conservative AABB over every matching
/// face of either input — loop vertices plus each boundary CURVE's own
/// extent (a disk's loop is one closed circle through a single anchor
/// vertex, so vertex hulls under-bound curved loops — the t134 trap).
/// Returns `Some(inside)` when a bounded hull exists; `None` = NO VERDICT
/// (non-planar partner, a loop curve without a cheap conservative bound, or
/// no matching input face) — callers must treat `None` as "no wall",
/// exactly as the gate always has (defensive: never a false wall).
pub(crate) fn planar_partner_hull_contains(
    a: &BRep,
    b: &BRep,
    partner: Surface,
    pos: [f64; 3],
    d_eps: f64,
) -> Option<bool> {
    let Surface::Plane { .. } = partner else {
        return None;
    };
    // Faces "on the partner plane" are the ones `merge_same_plane_patches`
    // folds into one output face — the same-plane-same-orientation reading
    // (`unit_planes_coincide`), NOT bit-equality: a coplanar pair's B face
    // keeps its own stored plane (Stage 0 snaps the loop, not the surface),
    // one ULP of `d` from A's, and the merged patch inherits A's surface.
    let partner_key = unit_plane(partner)?;
    let mut hull: Option<[f64; 6]> = None;
    for brep in [a, b] {
        for face in brep.faces() {
            if unit_plane(face.surface).is_none_or(|k| !unit_planes_coincide(k, partner_key)) {
                continue;
            }
            let mut lo = [f64::MAX; 3];
            let mut hi = [f64::MIN; 3];
            for &e in face
                .outer_loop
                .iter()
                .chain(face.inner_loops.iter().flatten())
            {
                let ed = &brep.edges()[e as usize];
                for vid in [ed.start, ed.end] {
                    let q = brep.vertices()[vid as usize].point.as_array();
                    for k in 0..3 {
                        lo[k] = lo[k].min(q[k]);
                        hi[k] = hi[k].max(q[k]);
                    }
                }
                match ed.curve {
                    Curve::LineSegment => {}
                    Curve::Circle {
                        center,
                        normal,
                        radius,
                    } => {
                        let c = center.as_array();
                        let n = normalize3(normal.as_array());
                        for k in 0..3 {
                            let ext = radius * (1.0 - n[k] * n[k]).max(0.0).sqrt();
                            lo[k] = lo[k].min(c[k] - ext);
                            hi[k] = hi[k].max(c[k] + ext);
                        }
                    }
                    Curve::Ellipse {
                        center,
                        major_radius,
                        ..
                    } => {
                        let c = center.as_array();
                        for k in 0..3 {
                            lo[k] = lo[k].min(c[k] - major_radius);
                            hi[k] = hi[k].max(c[k] + major_radius);
                        }
                    }
                    _ => return None,
                }
            }
            let h =
                hull.get_or_insert([f64::MAX, f64::MAX, f64::MAX, f64::MIN, f64::MIN, f64::MIN]);
            for k in 0..3 {
                h[k] = h[k].min(lo[k]);
                h[3 + k] = h[3 + k].max(hi[k]);
            }
        }
    }
    let h = hull?;
    let inside = (0..3).all(|k| pos[k] >= h[k] - d_eps && pos[k] <= h[3 + k] + d_eps);
    // Diagnosis probe (read-only, env-gated): the failing reading's anatomy —
    // the partner, the hull, the position and the per-axis escape (R0026 v677).
    if !inside && std::env::var_os("YANG_TORUS_PROBE").is_some() {
        let escape: Vec<f64> = (0..3)
            .map(|k| (h[k] - pos[k]).max(pos[k] - h[3 + k]).max(0.0))
            .collect();
        let mut faces: Vec<String> = Vec::new();
        for (tag, brep) in [("A", a), ("B", b)] {
            for (fi, face) in brep.faces().iter().enumerate() {
                if unit_plane(face.surface).is_none_or(|k| !unit_planes_coincide(k, partner_key)) {
                    continue;
                }
                let kinds: Vec<String> = face
                    .outer_loop
                    .iter()
                    .chain(face.inner_loops.iter().flatten())
                    .map(|&e| match brep.edges()[e as usize].curve {
                        Curve::LineSegment => "L".to_string(),
                        Curve::Circle { .. } => "C".to_string(),
                        Curve::Ellipse { .. } => "E".to_string(),
                        _ => "?".to_string(),
                    })
                    .collect();
                faces.push(format!(
                    "{tag}#{fi}[outer {} inner {}: {}]",
                    face.outer_loop.len(),
                    face.inner_loops.len(),
                    kinds.join("")
                ));
            }
        }
        eprintln!(
            "YANG_TORUS_STOP site=partner_hull partner={partner:?} pos={pos:?} \
             hull_lo={:?} hull_hi={:?} d_eps={d_eps:.4e} escape={escape:?} faces={faces:?}",
            &h[..3],
            &h[3..]
        );
        if let Surface::Plane { normal: pn, d: pd } = partner {
            let pnu = normalize3(pn.as_array());
            for (tag, brep) in [("A", a), ("B", b)] {
                for (fi, face) in brep.faces().iter().enumerate() {
                    if let Surface::Plane { normal, d } = face.surface {
                        let nu = normalize3(normal.as_array());
                        let c = nu[0] * pnu[0] + nu[1] * pnu[1] + nu[2] * pnu[2];
                        if c.abs() > 0.99 {
                            eprintln!(
                                "YANG_TORUS_STOP   near-parallel {tag}#{fi}: n={nu:?} d={d:e} \
                                 cos={c:.12} d_diff={:e} exact_eq={}",
                                d - pd,
                                face.surface == partner
                            );
                        }
                    }
                }
            }
        }
    }
    Some(inside)
}

/// PR-YR10: compute the Phase-A structures (adjacency → patches → cycles →
/// incidence → exact intersection curves) from the current mesh + attribution.
/// Factored out of `reconstruct_topology` so it can be re-run after a §4.5.3
/// collapse mutates the mesh.
/// Diagnosis probe (read-only, env-gated): `YANG_ATTR_TRACE=x,y,z,r` prints,
/// under `label`, every mesh vertex within `r` of the target point together
/// with every triangle that references it and that triangle's attribution —
/// keyed by POSITION so it survives the renumbering every collapse/compaction
/// performs. Answers "which step relabelled this triangle".
pub(crate) fn attr_trace(label: &str, mesh: &Mesh, attribution: &TriangleAttributionMap) {
    let Some(spec) = std::env::var_os("YANG_ATTR_TRACE") else {
        return;
    };
    let nums: Vec<f64> = spec
        .to_string_lossy()
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();
    let [x, y, z, r] = nums[..] else {
        return;
    };
    for (vi, p) in mesh.verts.iter().enumerate() {
        let q = p.as_array();
        let d = ((q[0] - x).powi(2) + (q[1] - y).powi(2) + (q[2] - z).powi(2)).sqrt();
        if d > r {
            continue;
        }
        eprintln!(
            "[attr-trace] {label}: vert {vi} ({:.12},{:.12},{:.12}) d={d:.3e} (mesh {} verts / {} tris)",
            q[0],
            q[1],
            q[2],
            mesh.verts.len(),
            mesh.tris.len()
        );
        for (ti, t) in mesh.tris.iter().enumerate() {
            if t.contains(&(vi as u32)) {
                let at = attribution.attributions.get(ti).copied().flatten();
                let pts: Vec<String> = t
                    .iter()
                    .map(|&w| {
                        let c = mesh.verts[w as usize].as_array();
                        format!("v{w}=({:.6e},{:.6e},{:.6e})", c[0], c[1], c[2])
                    })
                    .collect();
                eprintln!(
                    "[attr-trace]   tri {ti} {t:?} attr {at:?} {}",
                    pts.join(" ")
                );
            }
        }
    }
}

#[track_caller]
pub(crate) fn compute_phase_a(
    mesh: &Mesh,
    attribution: &TriangleAttributionMap,
    a: &BRep,
    b: &BRep,
    edge_provenance: &crate::stage3_ssi::PosKeyedEdgeSet,
) -> Result<PhaseA, YangError> {
    compute_phase_a_with_side_labels(mesh, attribution, None, a, b, edge_provenance)
}

#[track_caller]
pub(crate) fn compute_phase_a_with_side_labels(
    mesh: &Mesh,
    attribution: &TriangleAttributionMap,
    side_labels: Option<&TriangleSideLabelMap>,
    a: &BRep,
    b: &BRep,
    edge_provenance: &crate::stage3_ssi::PosKeyedEdgeSet,
) -> Result<PhaseA, YangError> {
    let adjacency = triangle_adjacency(mesh);
    if std::env::var_os("YANG_ATTR_TRACE").is_some() {
        let loc = std::panic::Location::caller();
        attr_trace(
            &format!("phase-a@{}:{}", loc.file(), loc.line()),
            mesh,
            attribution,
        );
    }
    let patches = flood_fill_patches_with_side_labels(mesh, attribution, side_labels, &adjacency);
    // PR-YR27 (Finding 1a): merge edge-adjacent patches lying on the SAME
    // plane with the SAME orientation into one output face — a coplanar
    // boolean otherwise emits e.g. A's and B's side fragments as two faces
    // on one bit-identical plane, and the NEXT boolean in a chain
    // exact-ties between them. Non-adjacent same-plane patches stay
    // separate faces (their union is not a single connected face). On the
    // full arrangement (`side_labels = Some`), merging also requires equal
    // `TriangleSideLabel` so patches never merge across operands or cells.
    let patches = merge_same_plane_patches_with_side_labels(patches, &adjacency, side_labels, a, b);

    let mut infos: Vec<PatchInfo> = Vec::with_capacity(patches.len());
    for patch in &patches {
        let cycles = patch_boundary_cycle(patch, mesh)?;
        let input = patch.attribution.input;
        let input_brep = match input {
            InputId::A => a,
            InputId::B => b,
        };
        let face_idx = patch.attribution.face as usize;
        if face_idx >= input_brep.faces().len() {
            return Err(YangError::MalformedTopology(format!(
                "attribution.face = {face_idx} out of range (input has {} faces)",
                input_brep.faces().len()
            )));
        }
        let inherited = input_brep.faces()[face_idx].surface;
        let input_reversed = input_brep.faces()[face_idx].reversed;
        let had_fold_sliver = !patch_fold_slivers(patch, mesh).is_empty();
        infos.push(PatchInfo {
            cycles,
            input,
            inherited,
            face_idx,
            input_reversed,
            had_fold_sliver,
        });
    }

    let mut incidence: std::collections::BTreeMap<(u32, u32), Vec<(InputId, Surface)>> =
        std::collections::BTreeMap::new();
    if let Some(sl) = side_labels {
        // Full-arrangement curve incidence (`specs/yang_442_classify_after_update.md`
        // §4): along a transverse intersection curve C = S_A ∩ S_B, up to four
        // sub-patches (two of S_A and two of S_B, split across C by Cherchi
        // patch labels) share each mesh edge. Deduplicate `(InputId, Surface)`
        // per edge so `build_intersection_curves` and `collect_rim_curves` see
        // the exact two-surface pair, and resolve `[true, true]` common-surface
        // sheets against pure operand sheets (§4.5.5).
        type RawEdgeInc = (InputId, Surface, bool);
        let mut raw_inc: std::collections::BTreeMap<(u32, u32), Vec<RawEdgeInc>> =
            std::collections::BTreeMap::new();
        for (patch, info) in patches.iter().zip(infos.iter()) {
            let is_overlap = patch
                .tri_indices
                .first()
                .and_then(|&t| sl.labels.get(t as usize))
                .is_some_and(|l| l.surface == [true, true]);
            for cycle in &info.cycles {
                for &(s, e) in cycle {
                    let key = if s < e { (s, e) } else { (e, s) };
                    raw_inc
                        .entry(key)
                        .or_default()
                        .push((info.input, info.inherited, is_overlap));
                }
            }
        }
        for (key, raw) in raw_inc {
            let mut pure: Vec<(InputId, Surface)> = Vec::new();
            let mut overlaps: Vec<(InputId, Surface)> = Vec::new();
            for (inp, surf, is_ov) in raw {
                let dst = if is_ov { &mut overlaps } else { &mut pure };
                if !dst.contains(&(inp, surf)) {
                    dst.push((inp, surf));
                }
            }
            for (ov_in, ov_surf) in overlaps {
                if pure
                    .iter()
                    .any(|&(_, ps)| surfaces_coincide_unoriented(ov_surf, ps))
                {
                    // Common surface is already represented by a pure continuation
                    // patch on this edge (e.g. cap overlap inside a larger face).
                    continue;
                }
                if pure.len() == 2 && pure[0].0 != pure[1].0 {
                    if surfaces_coincide_unoriented(pure[0].1, pure[1].1) {
                        // Both operands leave the common surface onto coincident
                        // walls (e.g. stacked equal-radius cylinders or flush box
                        // walls): replace the `ov_in` wall entry with the common
                        // surface so the edge names the transverse rim `S_common ∩ S_wall`.
                        if let Some(slot) = pure.iter_mut().find(|(i, _)| *i == ov_in) {
                            *slot = (ov_in, ov_surf);
                        }
                    }
                    // Otherwise the two pure walls already intersect transversely.
                    continue;
                }
                if !pure.contains(&(ov_in, ov_surf)) {
                    pure.push((ov_in, ov_surf));
                }
            }
            incidence.insert(key, pure);
        }
    } else {
        for info in &infos {
            for cycle in &info.cycles {
                for &(s, e) in cycle {
                    let key = if s < e { (s, e) } else { (e, s) };
                    incidence
                        .entry(key)
                        .or_default()
                        .push((info.input, info.inherited));
                }
            }
        }
    }
    // §4.2.3 incidence probe (`YANG_S423_INCIDENCE`, READ-ONLY). Diff the
    // cycle-derived incidence built just above against the paper's own route —
    // "querying the triangles that intersect at that point", i.e. the
    // per-triangle provenance map N4 established. Nothing here feeds Stage 3/4;
    // gate-OFF is byte-identical by construction. See `stage4_incidence`.
    if std::env::var_os("YANG_S423_INCIDENCE").is_some() {
        let prov = crate::stage4_incidence::provenance_edge_incidence(mesh, attribution);
        let cyc = crate::stage4_incidence::cycle_edge_incidence(
            infos
                .iter()
                .map(|i| (i.input, i.face_idx as u32, i.cycles.as_slice())),
        );
        // Edges on an IMPURE (merged) patch's cycle: divergence there is
        // PR-YR27's merge doing its job, not a provenance disagreement.
        let mut explained: std::collections::BTreeSet<(u32, u32)> =
            std::collections::BTreeSet::new();
        let mut n_impure = 0usize;
        for (patch, info) in patches.iter().zip(infos.iter()) {
            if crate::stage4_incidence::patch_is_impure(
                &patch.tri_indices,
                attribution,
                patch.attribution,
            ) {
                n_impure += 1;
                for cycle in &info.cycles {
                    for &(s, e) in cycle {
                        explained.insert(if s < e { (s, e) } else { (e, s) });
                    }
                }
            }
        }
        let d = crate::stage4_incidence::diff_incidence(&cyc, &prov, &explained);
        eprintln!(
            "[s423-incidence] boundary_edges={} agree={} prov_richer={} \
             cycle_unsupported={} disjointish={} missing_in_prov={} \
             patches={} impure_patches={} merge_explained={} UNEXPLAINED={}",
            d.boundary_edges,
            d.agree,
            d.prov_richer,
            d.cycle_unsupported,
            d.disjointish,
            d.missing_in_prov,
            infos.len(),
            n_impure,
            d.divergent_merge_explained,
            d.divergent_unexplained,
        );
        for (k, v, conly, ponly) in &d.unexplained_samples {
            eprintln!(
                "[s423-incidence]   UNEXPLAINED edge {k:?} {v:?} cycle-only {conly:?} prov-only {ponly:?}"
            );
        }
    }

    let curves =
        build_intersection_curves(&incidence, mesh, a, b, edge_provenance).inspect_err(|err| {
            if let YangError::SsiRefinementFailed { edge, .. } = err {
                crate::boolean::failures::note_ssi_failure(
                    mesh,
                    attribution,
                    *edge,
                    format!("{err}"),
                );
            }
        })?;
    Ok((infos, incidence, curves))
}

/// PR-YR27 (Finding 1a): merge edge-adjacent output patches whose inherited
/// planes are the same plane with the same orientation (bit-identical or
/// within `TAU_WORK` on the UNIT-normalized `(n̂, d̂)`) into ONE patch, so
/// Stage 6 emits one face per connected same-plane region of the output
/// solid.
///
/// Why: a coplanar boolean's output legitimately carries triangles from
/// BOTH inputs' faces on one geometric plane (e.g. exactly stacked boxes:
/// each side plane has an A fragment and a B fragment, edge-adjacent along
/// the seam). `flood_fill_patches` groups by attribution, so those
/// fragments emit as TWO faces on a bit-identical plane — a fragmented
/// B-Rep whose NEXT boolean exact-ties Stage-6 membership between them
/// (assay F0066). Merging is keyed to edge adjacency: non-adjacent
/// same-plane patches (genuinely separate faces) are NOT merged.
///
/// Safety / blast radius:
/// - Only `Surface::Plane` patches participate; the orientation test
///   (component-wise `|n̂ᵢ−n̂ⱼ| ≤ TAU_WORK`) means an opposite-normal pair
///   (e.g. a subtract cavity wall against an outer wall) NEVER merges.
/// - Distinct input faces on one plane only exist when an input itself
///   carries same-plane faces or the two inputs share a plane — exactly
///   the coplanar classes; every other fixture has zero mergeable pairs
///   and is byte-identical.
/// - The merged patch's attribution is the lexicographically smallest
///   member `(input, face)` (deterministic); the members' inherited
///   surfaces agree within `TAU_WORK`, so the choice is geometric noise.
/// - The seam edges become patch-INTERIOR (they vanish from the boundary
///   cycles and therefore from the output edge set) — the merged region's
///   single outer cycle is exactly the §4.5.5 result-face boundary.
#[allow(dead_code)]
pub(crate) fn merge_same_plane_patches(
    patches: Vec<Patch>,
    adjacency: &[Vec<u32>],
    a: &BRep,
    b: &BRep,
) -> Vec<Patch> {
    merge_same_plane_patches_with_side_labels(patches, adjacency, None, a, b)
}

pub(crate) fn merge_same_plane_patches_with_side_labels(
    mut patches: Vec<Patch>,
    adjacency: &[Vec<u32>],
    side_labels: Option<&TriangleSideLabelMap>,
    a: &BRep,
    b: &BRep,
) -> Vec<Patch> {
    if patches.len() < 2 {
        return patches;
    }

    let patch_side_label: Vec<Option<TriangleSideLabel>> = if let Some(sl) = side_labels {
        patches
            .iter()
            .map(|p| {
                p.tri_indices
                    .first()
                    .and_then(|&t| sl.labels.get(t as usize).copied())
            })
            .collect()
    } else {
        Vec::new()
    };

    // Inherited surface key per patch (`None` = unmergeable surface kind or
    // degenerate — never merged). A `Plane` keys on its unit `(n̂, d̂)`; a
    // `Cylinder` keys on its unit axis, an axis-line anchor (the axis point
    // projected to remove the free axial slide), the radius, AND the effective
    // outward sense (`reversed`) — two coincident cylinders of OPPOSITE sense
    // (a bore wall vs an outer wall) must NEVER merge (PR-5; mirrors the planar
    // opposite-normal guard). Spheres/cones keep `None` (not yet needed).
    enum SurfKey {
        Plane {
            n: [f64; 3],
            d: f64,
        },
        Cyl {
            axis: [f64; 3],
            anchor: [f64; 3],
            radius: f64,
            reversed: bool,
        },
    }
    let keys: Vec<Option<SurfKey>> = patches
        .iter()
        .map(|p| {
            let brep = match p.attribution.input {
                InputId::A => a,
                InputId::B => b,
            };
            let f = brep.faces().get(p.attribution.face as usize)?;
            match f.surface {
                Surface::Plane { normal, d } => {
                    let n = normal.as_array();
                    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
                    if len < cad_primitives::MIN_FEATURE_SIZE {
                        return None;
                    }
                    Some(SurfKey::Plane {
                        n: [n[0] / len, n[1] / len, n[2] / len],
                        d: d / len,
                    })
                }
                Surface::Cylinder {
                    axis_point,
                    axis_dir,
                    radius,
                } => {
                    let ad = axis_dir.as_array();
                    let len = (ad[0] * ad[0] + ad[1] * ad[1] + ad[2] * ad[2]).sqrt();
                    if len < cad_primitives::MIN_FEATURE_SIZE {
                        return None;
                    }
                    let axis = [ad[0] / len, ad[1] / len, ad[2] / len];
                    // Anchor = axis_point with its axial component removed, so
                    // two cylinders sharing one axis LINE but with axis points at
                    // different axial offsets get an identical anchor.
                    let ap = axis_point.as_array();
                    let t = ap[0] * axis[0] + ap[1] * axis[1] + ap[2] * axis[2];
                    let anchor = [
                        ap[0] - t * axis[0],
                        ap[1] - t * axis[1],
                        ap[2] - t * axis[2],
                    ];
                    Some(SurfKey::Cyl {
                        axis,
                        anchor,
                        radius,
                        reversed: f.reversed,
                    })
                }
                _ => None,
            }
        })
        .collect();
    let mergeable = |i: usize, j: usize| -> bool {
        if side_labels.is_some() && patch_side_label[i] != patch_side_label[j] {
            return false;
        }
        match (&keys[i], &keys[j]) {
            (Some(SurfKey::Plane { n: ni, d: di }), Some(SurfKey::Plane { n: nj, d: dj })) => {
                unit_planes_coincide((*ni, *di), (*nj, *dj))
            }
            (
                Some(SurfKey::Cyl {
                    axis: ai,
                    anchor: anchi,
                    radius: ri,
                    reversed: revi,
                }),
                Some(SurfKey::Cyl {
                    axis: aj,
                    anchor: anchj,
                    radius: rj,
                    reversed: revj,
                }),
            ) => {
                // Same effective sense, equal radius, parallel axes, same axis
                // line (anchors agree up to TAU_WORK; axes may be antiparallel —
                // a cylinder's axis_dir sign is free — so compare |aᵢ·aⱼ|≈1).
                revi == revj
                    && (ri - rj).abs() <= cad_primitives::TAU_WORK
                    && (ai[0] * aj[0] + ai[1] * aj[1] + ai[2] * aj[2]).abs()
                        >= 1.0 - cad_primitives::TAU_WORK
                    && (0..3).all(|k| (anchi[k] - anchj[k]).abs() <= cad_primitives::TAU_WORK)
            }
            _ => false,
        }
    };

    // patch index per mesh triangle.
    let mut patch_of: Vec<usize> = vec![usize::MAX; adjacency.len()];
    for (pi, p) in patches.iter().enumerate() {
        for &t in &p.tri_indices {
            patch_of[t as usize] = pi;
        }
    }

    // Union-find over patches, united on (edge-adjacent AND same-plane).
    let mut parent: Vec<usize> = (0..patches.len()).collect();
    fn find(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]]; // path halving
            x = parent[x];
        }
        x
    }
    for (pi, p) in patches.iter().enumerate() {
        for &t in &p.tri_indices {
            for &u in &adjacency[t as usize] {
                let pj = patch_of[u as usize];
                if pj == usize::MAX || pj == pi {
                    continue;
                }
                if mergeable(pi, pj) {
                    let (ri, rj) = (find(&mut parent, pi), find(&mut parent, pj));
                    if ri != rj {
                        parent[ri.max(rj)] = ri.min(rj);
                    }
                }
            }
        }
    }

    // Rebuild merged patches in first-member order (deterministic; a strict
    // no-op — same patches, same order — when nothing merged).
    let roots: Vec<usize> = (0..patches.len()).map(|i| find(&mut parent, i)).collect();
    let mut out: Vec<Patch> = Vec::with_capacity(patches.len());
    let mut taken = vec![false; patches.len()];
    for i in 0..patches.len() {
        if taken[i] {
            continue;
        }
        let members: Vec<usize> = (i..patches.len())
            .filter(|&j| roots[j] == roots[i])
            .collect();
        for &m in &members {
            taken[m] = true;
        }
        let attribution = members
            .iter()
            .map(|&m| patches[m].attribution)
            .min()
            .expect("members is non-empty");
        let mut tri_indices: Vec<u32> = Vec::new();
        for &m in &members {
            tri_indices.append(&mut patches[m].tri_indices);
        }
        out.push(Patch {
            attribution,
            tri_indices,
        });
    }
    out
}

/// PR-YR15 helper: the Stage-1 curved chord bound of ONE input, choosing the
/// surface's OWN bound (A14.3 / I-sphere-band). A `Surface::Sphere` face's
/// tessellation vertices sit off the exact great circle by up to the sphere's
/// own `sphere_chord_bound(radius) = 1e-2·2r√3`, which is LARGER than the
/// rim-AABB `curved_chord_bound` (2r√2) — so a sphere-bearing input must report
/// its sphere bound, NOT the rim band (which would underestimate and reject
/// valid sphere-rim vertices). Cylinder/all-planar inputs keep the rim-AABB
/// `curved_chord_bound` byte-for-byte. When both are present we take the MAX
/// (the budget must admit every curved-surface vertex). `None` only for an
/// all-planar input (zero chord error). This is the SINGLE source consulted by
/// both `build_intersection_curves` (selection tol) and `stage4_chord_band`
/// (relocation budget); it is NOT tolerance widening.
///
/// KV14 Slice F-3: a `Surface::Torus` face that tessellates through the PATCH
/// path (`torus_face_takes_patch_path`: a band, or a lone-loop DISK of chords)
/// folds in its own `torus_chord_bound(R, r)` — the budget the UV-CDT was
/// given. A re-entering torus disk bounded by a torus∩cone chord polyline has
/// no `Curve::Circle` rim anywhere, so without this arm the input reported NO
/// band and Stage 4 refused its conic edges as a producer fault. Structured
/// torus laterals sample at their rims' density and keep the rim band.
pub(crate) fn input_curved_chord_bound(brep: &BRep) -> Option<f64> {
    // Spec `yang_s3_ellipse_rim_chord_bound` amendment 1: an ellipse-rim-only
    // input (obliquely-trimmed cylinder re-entering from a prior boolean)
    // carries the Stage-1 ellipse chain bound — fallback-only composition,
    // byte-identical whenever a Circle rim exists.
    let rim = curved_chord_bound(brep.edges()).or_else(|| ellipse_rim_chord_bound(brep.edges()));
    let sphere = brep
        .faces()
        .iter()
        .filter_map(|f| match f.surface {
            Surface::Sphere { radius, .. } => Some(sphere_chord_bound(radius)),
            _ => None,
        })
        .fold(None, |acc: Option<f64>, b| {
            Some(acc.map_or(b, |a| a.max(b)))
        });
    let torus = brep
        .faces()
        .iter()
        .filter_map(|f| match f.surface {
            Surface::Torus {
                major_radius,
                minor_radius,
                ..
            } if torus_face_takes_patch_path(f, brep.edges(), major_radius, minor_radius) => {
                Some(torus_chord_bound(major_radius, minor_radius))
            }
            _ => None,
        })
        .fold(None, |acc: Option<f64>, b| {
            Some(acc.map_or(b, |a| a.max(b)))
        });
    // M5 K11: the Stage-1 surface-pair chains' own bound (the Slice F-3 torus
    // precedent — a band must cover every chain the tessellation carries).
    let pair = surface_pair_chord_bound(brep);
    [rim, sphere, torus, pair]
        .into_iter()
        .flatten()
        .fold(None, |acc: Option<f64>, b| {
            Some(acc.map_or(b, |a| a.max(b)))
        })
}

/// PR-YR10 helper: the Stage-4 chord-band relocation budget `d_ε` — the
/// Stage-1 chord bound of whichever input bears a curved surface (the curved
/// solid). Uses [`input_curved_chord_bound`] so a sphere input reports its OWN
/// (larger) 2r√3 bound, not the rim-AABB 2r√2 (I-sphere-band). `None` only if
/// NEITHER input has a curved surface, which cannot happen when a conic
/// intersection edge exists (a conic edge implies a curved input).
pub(crate) fn stage4_chord_band(a: &BRep, b: &BRep) -> Option<f64> {
    // PR-KV7: the MAX of the two inputs' Stage-1 bounds, not A-with-B-
    // fallback. An arrangement vertex on an A×B intersection curve sits on
    // the curved OWNER's facet chord, off the exact curve by up to that
    // owner's OWN sagitta — and with chainable boolean outputs the owner
    // can be EITHER input (a recovered body re-entering as A can have a
    // much tighter rim AABB than the fresh operand B whose curves are
    // being relocated). `max` admits exactly up to the looser owner's
    // honest Stage-1 bound for this model pair — a derived bound, not
    // tolerance widening. (Per-curve owner resolution, as Stage-3's
    // `chord_tol_for_curved_owner` does for selection, is the M5-era
    // refinement; `max` is its conservative envelope.)
    match (input_curved_chord_bound(a), input_curved_chord_bound(b)) {
        (Some(x), Some(y)) => Some(x.max(y)),
        (x, y) => x.or(y),
    }
}

/// §4-I11: the failure-population census must also see the runs that STOP.
///
/// `relocation_domain_postcondition` sits at the END of Stage 4, so a run that
/// refuses earlier — and the hardest cases all do — reports nothing at all. The
/// corpus census measured 114 of 187 curved cases from the postcondition alone;
/// six of the missing ones STOP inside Stage 4, and a §4.5.1 customer could only
/// have hidden there. So the census is taken on BOTH exits, and the print is
/// tagged with which one, because they are not equivalent: at a STOP the mesh is
/// frozen mid-repair and the later passes never ran.
///
/// This is the same argument that made §4-I9 a postcondition rather than a check
/// at each of thirteen relocation sites — one vantage point that covers every
/// exit, present and future, instead of an edit per site that the next site will
/// forget.
pub(crate) fn stage4_relocate_and_correct(
    mesh: &mut Mesh,
    attribution: &mut TriangleAttributionMap,
    side_labels: &mut TriangleSideLabelMap,
    a: &BRep,
    b: &BRep,
    minted_junction_keys: &std::collections::BTreeMap<[u64; 3], crate::boolean::MintProvenance>,
    edge_provenance: &crate::stage3_ssi::PosKeyedEdgeSet,
) -> Result<(Vec<(u32, f64)>, bool), YangError> {
    let mut dummy_tv = std::collections::BTreeSet::new();
    stage4_relocate_and_correct_ex(
        mesh,
        attribution,
        side_labels,
        a,
        b,
        minted_junction_keys,
        edge_provenance,
        false,
        &mut dummy_tv,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn stage4_relocate_and_correct_ex(
    mesh: &mut Mesh,
    attribution: &mut TriangleAttributionMap,
    side_labels: &mut TriangleSideLabelMap,
    a: &BRep,
    b: &BRep,
    minted_junction_keys: &std::collections::BTreeMap<[u64; 3], crate::boolean::MintProvenance>,
    edge_provenance: &crate::stage3_ssi::PosKeyedEdgeSet,
    full_arrangement: bool,
    tangential_verts: &mut std::collections::BTreeSet<u32>,
) -> Result<(Vec<(u32, f64)>, bool), YangError> {
    star_probe("s4-entry", mesh, attribution);
    nonmanifold_edge_census("s4-entry", mesh, attribution);
    coincident_vertex_census("s4-entry", mesh);
    // Diagnosis probe (read-only, env-gated): `YANG_V_PROBE_VERTS=a,b,c`
    // dumps the listed vertices' ENTRY positions and every incident triangle
    // with its attribution — the arrangement as Stage 4 first sees it.
    if let Ok(spec) = std::env::var("YANG_V_PROBE_VERTS") {
        let want: Vec<u32> = spec
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();
        for &v in &want {
            eprintln!(
                "YANG_V_PROBE_VERTS v={v} p={:?}",
                mesh.verts.get(v as usize).map(|p| p.as_array())
            );
        }
        for (ti, tri) in mesh.tris.iter().enumerate() {
            if tri.iter().any(|x| want.contains(x)) {
                eprintln!(
                    "YANG_V_PROBE_VERTS   tri {ti} {tri:?} attr={:?}",
                    attribution.attributions.get(ti).copied().flatten()
                );
            }
        }
    }
    // (0) EDGE-PINCH split, at ENTRY (spec `yang_tangency_pinch_split.md` §0a).
    // A face of one operand TANGENT to a face of the other along a whole LINE
    // reaches Stage 4 as a chain of 4-triangle edges — the arrangement's honest
    // boundary for a line-pinched solid, not a Stage-4 artefact (F0060: 14 such
    // edges here, all on its two tangent lines). It has to be separated per
    // sheet BEFORE §4.4.1(b), for a reason measured rather than assumed: the
    // merge legitimately fuses the chain's ULP twins, and in doing so it
    // collapses a whole chain into ONE edge whose four triangles then all carry
    // the SAME operand — destroying the 2+2 attribution certificate the split
    // reads. Run late, only 2 of F0060's 3 survivors are still certifiable, and
    // a half-certified chain does not separate at all: a chain-interior vertex
    // needs BOTH of its pinch edges paired before its fan ring falls into two
    // components. Splitting a 2-valent-only mesh is a no-op, so this call is
    // inert wherever there is no line pinch.
    //
    // ALWAYS-ON since 2026-09-13 (spec §0c; the `YANG_EDGE_PINCH_SPLIT`
    // off-switch was deleted 2026-10-16). The adjudication that kept it off — "the boundary it produces
    // reads χ = 6 against the authored χ = 2" — was an ORACLE artefact, not a
    // property of the output: yang emits F0060's `A − B` as four closed
    // shells, each χ = 2, with the two tangent points as per-sheet vertex
    // copies (Mäntylä duplication, identical bits), and the assay's
    // position-welded shell count fused each pair at that point. Counting
    // shells by EDGE adjacency reads the honest 4 shells / χ = 8.
    if full_arrangement {
        *tangential_verts = detect_arrangement_tangential_contact_verts(mesh, &side_labels.labels);
        if !tangential_verts.is_empty() && std::env::var_os("YANG_EDGE_PINCH_PROBE").is_some() {
            eprintln!(
                "[edge-pinch] full-arrangement recorded {} tangential contact verts for post-keep split",
                tangential_verts.len()
            );
        }
    } else {
        let mut no_relocs: Vec<(u32, f64)> = Vec::new();
        let splits = split_pinch_vertices(
            mesh,
            &mut no_relocs,
            &attribution.attributions,
            &side_labels.labels,
            true,
        );
        if splits > 0 && std::env::var_os("YANG_EDGE_PINCH_PROBE").is_some() {
            eprintln!("[edge-pinch] entry split {splits} vertex copies");
        }
    }
    let census = cfg!(debug_assertions)
        && std::env::var("YANG_S4_CARRIER_DOMAIN").as_deref() == Ok("census");
    // Taken before the inner call, so it is the mesh exactly as Stage 4 received
    // it — strictly earlier than the inner snapshot and never later.
    let entry: Option<Vec<[f64; 3]>> =
        census.then(|| mesh.verts.iter().map(Point3::as_array).collect());
    let out = stage4_relocate_and_correct_inner(
        mesh,
        attribution,
        side_labels,
        a,
        b,
        minted_junction_keys,
        edge_provenance,
        full_arrangement,
        tangential_verts,
    );
    // §4.5 trigger T1 (`boolean::failures`, spec `yang_452_refinement_loop.md`
    // §3): a relocation that did not converge in its domain names the faces
    // at its vertex, on the mesh as the inner pass left it. Record-only.
    if let Err(YangError::Stage4RegionInvalid { vertex, reason, .. }) = out.as_ref() {
        crate::boolean::failures::record_relocation_divergence(
            mesh,
            attribution,
            *vertex,
            &format!("{reason:?}"),
        );
    }
    if let (Some(entry), Err(e)) = (entry.as_ref(), out.as_ref()) {
        eprintln!("YANG_S45_POP stop_reason={e}");
        // No §4-I9 site list here: that STOP is disabled in census mode, so a run
        // that refused did so for some OTHER reason and its I9 population is
        // whatever the in-domain half reports.
        census_failure_population(mesh, &attribution.attributions, a, b, entry, &[], "stopped");
        // The STOP'd vertex itself is the one member the population census
        // CANNOT see: the refusal happens where the answer is rejected, so the
        // vertex is never written and "Stage 4 moved it" — the proxy for "the
        // optimization ran on it" — is false. It is nonetheless the clearest
        // §4.5 failure in the run, so classify it directly.
        if let YangError::Stage4RegionInvalid { vertex, reason, .. } = e {
            if (*vertex as usize) >= mesh.verts.len() {
                // Sentinel STOPs (u32::MAX) name no vertex; nothing to classify.
                eprintln!(
                    "YANG_S45_POP STOP-VERTEX v{vertex} reason={reason:?} — sentinel, \
                     no vertex to classify"
                );
            } else {
                let patches = build_patch_map(mesh, &attribution.attributions);
                let pos = mesh.verts[*vertex as usize].as_array();
                let (ca, cb) = carrier_counts(&patches, a, b, *vertex, pos);
                let near = ca.max(cb);
                let class = match near {
                    0 => "unlocated (on no surface of either operand)",
                    1 => "INTERIOR — a §4.5.1 customer",
                    _ => "BOUNDARY — excluded from §4.5.1 by Fig-13",
                };
                eprintln!(
                    "YANG_S45_POP STOP-VERTEX v{vertex} reason={reason:?} on_curve={} \
                     carrier=(A{ca},B{cb}) class={class}",
                    vertex_on_curve(&patches, *vertex),
                );
                // §4-I12 — §4.5's SECOND selector clause, from the STOP vantage.
                //
                // §4-I11 classified the STOP vertex by the Fig-13 clause alone and
                // recorded the second clause — "bounded by two successfully
                // optimized points on the same surface" — as the untested,
                // deciding half. It is taken here, where §4.5's repair would
                // actually run: the mesh is frozen at the refusal, mid-repair.
                //
                // "Successfully optimized" at this vantage: CONVERGED (on a
                // surface of each operand at the shared certificate band), not
                // the STOP vertex itself, and not an out-of-domain crosser in
                // §4-I9's sense. The postcondition that computes I9's fire list
                // never runs on a STOP'd run, so its two-leg reading is re-taken
                // per candidate bound: the vertex travelled across a STILL
                // neighbour lying ON its pre→post segment (leg 1) that carries a
                // surface the final position is OFF (leg 2) — a domain ENDPOINT,
                // not a sample. Skipped crossers are counted and printed so a
                // bound reached PAST one is visible as such.
                let adj = build_live_adjacency(mesh);
                let crossers = std::cell::Cell::new(0usize);
                let crossed_endpoint = |w: u32| -> bool {
                    vertex_crossed_domain_endpoint(
                        mesh,
                        &attribution.attributions,
                        a,
                        b,
                        &adj,
                        entry,
                        w,
                    )
                };
                let on_curve = |w: u32| vertex_on_curve(&patches, w);
                let good = |w: u32| -> bool {
                    if w == *vertex || !vertex_converged(mesh, &patches, a, b, w) {
                        return false;
                    }
                    if crossed_endpoint(w) {
                        crossers.set(crossers.get() + 1);
                        return false;
                    }
                    true
                };
                eprintln!(
                    "YANG_S45_SELECT v{vertex} vantage=stopped — §4.5 clause-2 walk from \
                     the STOP vertex (§4-I12)"
                );
                let first = selector_clause2_walk(
                    mesh,
                    &attribution.attributions,
                    a,
                    b,
                    &adj,
                    *vertex,
                    &on_curve,
                    &good,
                )
                .is_some();
                eprintln!(
                    "YANG_S45_SELECT v{vertex} vantage=stopped i9_style_crossers_skipped={} \
                     COMBINED clause1={class} clause2_first_strategy={first} => {}",
                    crossers.get(),
                    if near == 1 && first {
                        "§4.5.1 CONFIRMED customer (both clauses hold)"
                    } else {
                        "§4.5.2 (a selector clause fails)"
                    }
                );
            }
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn stage4_relocate_and_correct_inner(
    mesh: &mut Mesh,
    attribution: &mut TriangleAttributionMap,
    side_labels: &mut TriangleSideLabelMap,
    a: &BRep,
    b: &BRep,
    minted_junction_keys: &std::collections::BTreeMap<[u64; 3], crate::boolean::MintProvenance>,
    edge_provenance: &crate::stage3_ssi::PosKeyedEdgeSet,
    full_arrangement: bool,
    tangential_verts: &mut std::collections::BTreeSet<u32>,
) -> Result<(Vec<(u32, f64)>, bool), YangError> {
    use std::collections::{BTreeMap, HashSet};

    // Non-shadowed aliases for the input BReps (the loops below rebind `a`/`b`
    // to per-triangle vertex indices, so diagnostics/lookups that need the BReps
    // use these).
    let (brep_a, brep_b) = (a, b);

    s4_lap("s4-entry", mesh);
    balance_census(mesh, "s4-entry");

    // §4-I9: positions as Stage 4 found them, for the relocation-domain
    // postcondition at the end. Taken before ANY arm runs, so every relocation
    // path — including `apply_boundary_relocations` far below — is covered by
    // one check rather than a dozen.
    let s4_entry_pos: Vec<[f64; 3]> = mesh.verts.iter().map(Point3::as_array).collect();

    // I1d probe (read-only): tag every `relocations.push` with its SOURCE LINE
    // so a relocated vertex's AUTHORITY (circle projection, line foot,
    // junction closed-form, …) is attributable offline by position match.
    // The Stage-4 ENTRY position rides along (P0020): two relocations that
    // land on one point are a defect only if they STARTED as two distinct
    // model points, and the destination alone cannot say so.
    let i1d_probe = std::env::var_os("YANG_I1D_RELOC_PROBE").is_some();
    if i1d_probe {
        eprintln!(
            "[i1d-reloc] STAGE4 ENTRY verts={} tris={}",
            mesh.verts.len(),
            mesh.tris.len()
        );
    }
    let probe_push = |site: u32, v: u32, t: f64, p: Point3| {
        if i1d_probe {
            let e = s4_entry_pos[v as usize];
            eprintln!(
                "[i1d-reloc] site=L{site} v{v} t={t:.6} pos=({:.12}, {:.12}, {:.12}) \
                 entry=({:.12}, {:.12}, {:.12})",
                p.x(),
                p.y(),
                p.z(),
                e[0],
                e[1],
                e[2],
            );
        }
    };

    // §4.5.1 inc-1 census (spec §7, debug-only neuter): `census` flips every
    // OffCurve gate from abort-at-first-fire to record-and-skip; the post-sweep
    // census then reports at the paper's vantage and returns the FIRST recorded
    // error unchanged. Default (always in release): gates abort and `s451_repair`
    // is ON.
    let s451_mode = std::env::var("YANG_451").unwrap_or_default();
    let s451_census = cfg!(debug_assertions) && s451_mode == "census";
    // FLIPPED ALWAYS-ON 2026-08-22 (spec `yang_451_optimize_across_boundaries.md`
    // §11): the corpus under the gate is category-identical with EXACTLY ONE
    // explained detail delta (R0003: Stage-4 OffCurve → the pre-existing
    // KV9-F2 developable fold it unmasks); `census` measures (the `0`/`off`
    // abort-at-first-fire kill switch was deleted 2026-10-16).
    let s451_repair = !s451_census;
    let s451_collect = true;
    let mut s45_failures: Vec<(u32, YangError)> = Vec::new();

    // d_ε relocation budget (a conic edge implies a curved input ⇒ Some).
    let d_eps = match stage4_chord_band(a, b) {
        Some(de) => de,
        None => {
            // A conic edge with no circle-bearing input is a producer fault;
            // never default to TAU_WORK for a curved relocation (P10).
            if std::env::var_os("YANG_LRR_PROBE").is_some() {
                eprintln!("YANG_LRR_STOP site=chord_band_none");
            }
            return Err(YangError::stage4_region_invalid(
                u32::MAX,
                Stage4InvalidReason::LocalRefinementRequired,
            ));
        }
    };

    // (1) Collect + classify every conic-edge endpoint from the CURRENT Phase A.
    // PR-YR11: the incidence map (no longer discarded) supplies the TRUE cylinder
    // + cutting plane per Ellipse edge for the closed-form cylinder relocation.
    // This scan runs BEFORE any relocation moves a vertex, so the
    // position-keyed provenance is valid here — this is the classification
    // the relocation maps are built from (spec inc-2).
    let (_infos0, inc0, curves0) = compute_phase_a_with_side_labels(
        mesh,
        attribution,
        full_arrangement.then_some(&*side_labels),
        a,
        b,
        edge_provenance,
    )?;
    s4_lap("phase-a", mesh);

    // Provenance-vouched vertices (spec inc-2 §3c): endpoints of a
    // producer-confirmed intersection edge. Their curve assignment is
    // vouched by the arrangement itself, so the chord-band gates below —
    // whose role is catching WRONG-curve assignments — do not apply to
    // them: a beyond-band vouched vertex is a DRIFTED intersection vertex,
    // and moving it onto its exact curve is the very relocation obligation
    // the witness selection created. Empty (and every gate byte-identical)
    // unless `YANG_S3_EDGE_PROVENANCE_ENABLE` supplied provenance.
    let prov_verts: HashSet<u32> = if edge_provenance.is_empty() {
        HashSet::new()
    } else {
        curves0
            .keys()
            .filter(|&&(s, e)| {
                let ka = crate::stage3_ssi::pos_key(mesh.verts[s as usize]);
                let kb = crate::stage3_ssi::pos_key(mesh.verts[e as usize]);
                edge_provenance.contains(&(ka.min(kb), ka.max(kb)))
            })
            .flat_map(|&(s, e)| [s, e])
            .collect()
    };

    // Per-vertex Circle assignment (deterministic via BTreeMap). PR-YR19: the
    // 4th tuple element carries the originating sphere radius `Some(R)` for a
    // sphere section circle (else `None`) so the relocation guard can scale the
    // in-plane radial band by `(R/r_c)` (spec §2/§4 Site 2).
    let mut vert_circle: BTreeMap<u32, (Point3, Vector3, f64, Option<f64>)> = BTreeMap::new();
    // PR-YR11: per-vertex Ellipse relocation data (the true cylinder + plane +
    // stored ellipse), analogous to `vert_circle`.
    let mut vert_ellipse: BTreeMap<u32, EllipseReloc> = BTreeMap::new();
    // PR-YR21: per-vertex cone-ellipse relocation data (the true cone + plane +
    // stored ellipse + the cone's OWN chord budget), for a `cone ∩ plane`
    // oblique section. Kept separate from `vert_ellipse` (cylinder) so the
    // cylinder path stays byte-identical.
    let mut vert_cone_ellipse: BTreeMap<u32, ConeEllipseReloc> = BTreeMap::new();
    // PR-YR22: per-vertex cone-parabola relocation data for a `cone ∩ plane` θ=α
    // (generator-parallel) section. Kept separate from the ellipse maps so the
    // ellipse/cylinder paths stay byte-identical.
    let mut vert_parabola: BTreeMap<u32, ConeParabolaReloc> = BTreeMap::new();
    // PR-YR23: per-vertex cone-hyperbola relocation data for a `cone ∩ plane`
    // axis-parallel (HYPE) section. Kept separate from the other conic maps so
    // the ellipse/cylinder/parabola paths stay byte-identical.
    let mut vert_cone_hyperbola: BTreeMap<u32, ConeHyperbolaReloc> = BTreeMap::new();
    // KV16 (spec `kv16_hyperbola_arc_vocabulary`): a vertex receiving TWO
    // DIFFERENT cone-hyperbola descriptors (the prism-edge × cone-lateral
    // pierce — same cone, two steep planes, BOTH sections hyperbolas; R0017
    // v47) collapses into the ONE map above, so the increment-5 "≥2 maps"
    // trigger cannot see the junction and the vertex would be relocated
    // onto only one curve (an off-branch endpoint on the other's output
    // edge). Detected at insert time — the vert_ell_junction precedent —
    // and force-fed to the triple-junction relocation below.
    let mut same_type_junction: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
    // PR-F3: per-vertex ruling-LINE relocation data for a plane∥axis ×
    // cylinder intersection edge (ssi C3a/C3b). A `Curve::LineSegment`
    // intersection edge whose incidence carries a CYLINDER is such a line; its
    // arrangement points sit on Stage-1 facet chords, off the exact line (and
    // off the cylinder) by up to the sagitta — they need relocation exactly
    // like the conic arms. Plane∩plane segments are exact and stay skipped.
    let mut vert_line: BTreeMap<u32, LineReloc> = BTreeMap::new();
    // P0027 (2026-10-08, spec `yang_stage4_conic_triple_junction`,
    // "Junction-map candidates — the line pair"): a vertex claimed by a
    // SECOND, DIFFERENT exact line. Two generators of two different
    // cylinders crossing in ONE cutting plane (the cylinder×cylinder crease
    // of a previous union pierced by a face PARALLEL to both axes) is the
    // plain `{plane, cyl_A, cyl_B}` corner the triple block solves; two
    // generators of ONE cylinder in two planes is `{plane, plane, cyl}`,
    // likewise three surfaces. The line arm STOPped on the second record
    // (`line_line_junction`) before the block ever ran — the seventh
    // junction map found counting ZERO toward `n_maps`. `vert_line` keeps the
    // FIRST record (the KV16 one-slot precedent, inverted: first, not last —
    // the gate's `junction_line_curve_divergence` certifies either); this
    // map keeps the second, and any vertex still here after the block is
    // the SAME loud STOP as before (the residue audit below the block).
    let mut vert_line_junction: BTreeMap<u32, LineReloc> = BTreeMap::new();
    // M5 (Y4): per-vertex procedural surface-pair relocation data — the TWO
    // defining surfaces of a `Curve::SurfacePair` edge, carried on the curve
    // itself (no incidence scan needed). Each endpoint is Newton-projected
    // onto BOTH surfaces (`relocate_onto_implicit_pair`), the analog of the
    // torus implicit-pair block but with the pair supplied directly.
    let mut vert_surface_pair: BTreeMap<u32, (Surface, Surface)> = BTreeMap::new();
    // PR-KV9: a vertex shared by TWO DIFFERENT ellipse edges (the crossing
    // points of the Steinmetz cyl×cyl pair) must land on BOTH curves — the
    // exact junction is `(plane₁ ∩ plane₂) line ∩ cylinder`. Detected at
    // insert time (a silent overwrite would relocate one ellipse's endpoint
    // onto the other, collapsing the seam).
    let mut vert_ell_junction: BTreeMap<u32, (EllipseReloc, EllipseReloc)> = BTreeMap::new();
    // M8 disc∩disc CROSSING: a vertex shared by TWO DIFFERENT coplanar CIRCLE
    // edges (the lens corners of two overlapping coplanar cap rims) must land on
    // BOTH circles — the exact junction is the closed-form circle∩circle
    // intersection in their shared plane. Detected at insert time (a silent
    // overwrite would relocate it onto only the last-scanned circle, leaving the
    // other arc's endpoint off-circle by the lens displacement — the kernel-v2
    // "output arc endpoint does not lie on its circle" reject). The circle analog
    // of `vert_ell_junction`.
    let mut vert_circle_junction: BTreeMap<u32, (CircleAssign, CircleAssign)> = BTreeMap::new();
    // PR-KV11: per-vertex plane∩plane intersection-LINE incidences. The pp
    // segments themselves are exact (skipped), but their ENDPOINT on a
    // chordized curved lateral is a TRIPLE point (e.g. capA∩faceB line ×
    // lateral ellipse): the arrangement vertex lies exactly ON the line but
    // only chord-close to the cylinder, so relocating it onto the conic
    // alone slides it OFF the line (off the cap plane — the F0046 Newell
    // disagreement). Collected here; resolved into `vert_ell_junction`
    // after the scan (the junction is `(plane ∩ plane) ∩ cylinder`, the
    // same closed form as the ellipse×ellipse box-edge junction).
    let mut vert_pp_planes: BTreeMap<u32, Vec<(Vector3, f64, Vector3, f64)>> = BTreeMap::new();
    // PR-KV11: junction-aware insertion, shared by BOTH ellipse arms
    // (cylinder+plane AND cylinder×cylinder). A vertex already assigned a
    // DIFFERENT ellipse (the box-edge crossing of two cylinder∩plane
    // sections, or the Steinmetz cyl×cyl crossing) is demoted to the
    // junction map; a silent overwrite would relocate it onto only the
    // last-scanned ellipse, leaving it off the first by the Stage-1 chord
    // error (the F0046-class "endpoint does not lie on its ellipse").
    fn insert_ellipse_or_junction(
        v: u32,
        er: EllipseReloc,
        vert_ellipse: &mut BTreeMap<u32, EllipseReloc>,
        vert_ell_junction: &mut BTreeMap<u32, (EllipseReloc, EllipseReloc)>,
        endpoints: &mut Vec<u32>,
    ) {
        if let Ok(list) = std::env::var("YANG_V_PROBE") {
            if list.split(',').any(|t| t.trim().parse::<u32>() == Ok(v)) {
                eprintln!(
                    "YANG_V_PROBE insert_ellipse v={v} plane_n={:?} plane_d={:.17e} center={:?}",
                    er.plane_n, er.plane_d, er.center,
                );
            }
        }
        if let Some(prev) = vert_ellipse.get(&v).copied() {
            let same = prev.plane_d == er.plane_d
                && prev.plane_n.as_array() == er.plane_n.as_array()
                && prev.center.as_array() == er.center.as_array();
            if !same {
                vert_ellipse.remove(&v);
                vert_ell_junction.insert(v, (prev, er));
                endpoints.push(v);
                return;
            }
        } else if vert_ell_junction.contains_key(&v) {
            // Already a junction of two ellipses; a third co-incident
            // section adds no relocation freedom (the junction point is
            // fully determined by line ∩ cylinder).
            endpoints.push(v);
            return;
        }
        vert_ellipse.insert(v, er);
        endpoints.push(v);
    }
    // M8 disc∩disc: insert a CIRCLE assignment, demoting to `vert_circle_junction`
    // when the vertex already carries a DIFFERENT circle (the lens corner of two
    // coplanar cap rims). Mirrors `insert_ellipse_or_junction`.
    fn insert_circle_or_junction(
        v: u32,
        ca: CircleAssign,
        vert_circle: &mut BTreeMap<u32, CircleAssign>,
        vert_circle_junction: &mut BTreeMap<u32, (CircleAssign, CircleAssign)>,
        endpoints: &mut Vec<u32>,
    ) {
        if let Some(prev) = vert_circle.get(&v).copied() {
            // Same circle (two arcs of ONE split circle meet here) → keep single.
            let same = prev.0.as_array() == ca.0.as_array()
                && prev.1.as_array() == ca.1.as_array()
                && prev.2 == ca.2;
            if !same {
                vert_circle.remove(&v);
                vert_circle_junction.insert(v, (prev, ca));
                endpoints.push(v);
                return;
            }
        } else if vert_circle_junction.contains_key(&v) {
            // Already a circle∩circle junction; a third co-incident circle adds
            // no relocation freedom (the junction is fully determined by the
            // first two), so don't overwrite — just keep it an endpoint.
            endpoints.push(v);
            return;
        }
        vert_circle.insert(v, ca);
        endpoints.push(v);
    }
    let mut endpoints: Vec<u32> = Vec::new();
    if let Ok(list) = std::env::var("YANG_V_PROBE") {
        let probed: Vec<u32> = list
            .split(',')
            .filter_map(|t| t.trim().parse::<u32>().ok())
            .collect();
        for (&(s, e), curve) in &curves0 {
            if probed.contains(&s) || probed.contains(&e) {
                eprintln!("YANG_V_PROBE curves0 edge ({s},{e}) curve={curve:?}");
            }
        }
    }
    // Increment 3 (spec `yang_rim_junction_insertion` §Failure modes):
    // PRE-SCAN EXACTNESS CERTIFICATE for over-determined junction vertices.
    // A vertex whose incidence (inc0) carries ≥3 DISTINCT surfaces and whose
    // position is ALREADY within TAU_WORK of EVERY one of them is a fully
    // determined junction point that needs no relocation — the Stage-1 rim
    // junction insertion (increment 2) mints exactly this population (the
    // truncated-Steinmetz lobe corners, on 4 surfaces bit-exactly). Today
    // such a vertex trips one of the scan's insert-time junction detectors
    // (e.g. the line∩line "out of scope" STOP) or the post-scan
    // over-determined audits. Certified vertices are skipped by EVERY map
    // insertion below: they enter no conic map, no junction map, and no
    // `endpoints`, so every detector, audit, and relocation loop is
    // unchanged for all other vertices (the no-skip audit stays balanced).
    // `surface_value_and_normal`'s F is a signed DISTANCE (shared with
    // `signed_distance_to_surface`), so this is a genuine exactness
    // certificate — never a silent pick (P9): anything inexact keeps
    // today's loud walls. Ordinary 2-surface curve vertices are NOT
    // certified (they keep their retag/`t` bookkeeping).
    // Per-vertex DISTINCT incident surfaces (inc0 dedup) — shared by the
    // increment-3 exactness certificate below and the increment-5 conic
    // triple-junction relocation (spec `yang_stage4_conic_triple_junction`).
    let vert_surfs: BTreeMap<u32, Vec<Surface>> = {
        let mut vert_surfs: BTreeMap<u32, Vec<Surface>> = BTreeMap::new();
        for (&(s, e), entries) in &inc0 {
            for v in [s, e] {
                let list = vert_surfs.entry(v).or_default();
                for &(_input, surf) in entries {
                    if !list.contains(&surf) {
                        list.push(surf);
                    }
                }
            }
        }
        vert_surfs
    };
    let exact_junctions: HashSet<u32> = {
        let mut set = HashSet::new();
        for (&v, surfs) in &vert_surfs {
            if surfs.len() < 3 {
                continue;
            }
            let p = mesh.verts[v as usize].as_array();
            // Increment 4 §4d: scale-aware band (was the absolute
            // TAU_WORK, ~2 ULP at coordinate magnitude 4000 — see
            // `junction_certificate_band`).
            let exact_on_all = surfs.iter().all(|&s| {
                surface_value_and_normal(s, p)
                    .is_some_and(|(f, _)| f.abs() <= junction_certificate_band(p, s))
            });
            if std::env::var_os("YANG_RIM_JUNCTION_PROBE").is_some() {
                eprintln!(
                    "[s4-exact-junction] v={v} surfs={} exact={exact_on_all} p={:?}",
                    surfs.len(),
                    p,
                );
                for &s in surfs {
                    let f = surface_value_and_normal(s, p).map(|(f, _)| f);
                    eprintln!("[s4-exact-junction]   v={v} f={f:?} surf={s:?}");
                }
            }
            if exact_on_all {
                set.insert(v);
            }
        }
        set
    };

    for (&(s, e), curve) in &curves0 {
        match *curve {
            Curve::Parabola {
                vertex,
                normal,
                axis_dir,
                focal_length: _, // recovered from the output edge in eval_source.
            } => {
                // PR-YR22: identify the TRUE cone + cutting plane from this edge's
                // incidence (the θ=α generator-parallel section), mirroring the
                // cone-ellipse arm. Carry the cone's owning `InputId` so its chord
                // budget can be derived from its rim Circle.
                let key = if s < e { (s, e) } else { (e, s) };
                let entries = inc0.get(&key);
                let mut cone: Option<(InputId, Point3, Vector3, f64)> = None;
                let mut plane: Option<(Vector3, f64)> = None;
                if let Some(entries) = entries {
                    for &(input, surf) in entries {
                        match surf {
                            Surface::Cone {
                                apex,
                                axis_dir: cone_axis,
                                half_angle,
                            } => cone = Some((input, apex, cone_axis, half_angle)),
                            Surface::Plane { normal: pn, d: pd } => plane = Some((pn, pd)),
                            _ => {}
                        }
                    }
                }
                let (Some((cone_input, apex, cone_axis_dir, half_angle)), Some((plane_n, plane_d))) =
                    (cone, plane)
                else {
                    // A parabola section that is not a cone+plane pair is out of
                    // scope (producer fault). Loud STOP (P9/P10), mirroring the
                    // cone-ellipse `_ =>` arm.
                    if std::env::var_os("YANG_LRR_PROBE").is_some() {
                        eprintln!(
                            "YANG_LRR_SITE site=parabola_pair_miss edge=({s},{e}) p={:?} \
                             entries={entries:?}",
                            mesh.verts.get(s as usize)
                        );
                    }
                    return Err(YangError::stage4_region_invalid(
                        s,
                        Stage4InvalidReason::LocalRefinementRequired,
                    ));
                };
                let owner = match cone_input {
                    InputId::A => a,
                    InputId::B => b,
                };
                let Some(cone_d_eps) =
                    cone_chord_budget_from_owner(apex, cone_axis_dir, half_angle, owner)
                else {
                    if std::env::var_os("YANG_LRR_PROBE").is_some() {
                        eprintln!(
                            "YANG_LRR_SITE site=parabola_cone_budget edge=({s},{e}) p={:?} \
                             apex={apex:?} half_angle={half_angle}",
                            mesh.verts.get(s as usize)
                        );
                    }
                    return Err(YangError::stage4_region_invalid(
                        s,
                        Stage4InvalidReason::LocalRefinementRequired,
                    ));
                };
                let cpr = ConeParabolaReloc {
                    apex,
                    cone_axis_dir,
                    half_angle,
                    plane_n,
                    plane_d,
                    vertex,
                    normal,
                    para_axis_dir: axis_dir,
                    cone_d_eps,
                };
                for v in [s, e] {
                    // Increment 3: certified exact junction — enters no map (see above).
                    if exact_junctions.contains(&v) {
                        continue;
                    }
                    vert_parabola.insert(v, cpr);
                    endpoints.push(v);
                }
            }
            Curve::Hyperbola {
                center,
                normal,
                major_axis,
                semi_transverse: _, // recovered from the output edge in eval_source.
                semi_conjugate,
            } => {
                // PR-YR23: identify the TRUE cone + cutting plane from this edge's
                // incidence (the axis-parallel HYPE section), mirroring the
                // cone-parabola arm. Carry the cone's owning `InputId` so its
                // chord budget can be derived from its rim Circle.
                let key = if s < e { (s, e) } else { (e, s) };
                let entries = inc0.get(&key);
                let mut cone: Option<(InputId, Point3, Vector3, f64)> = None;
                let mut plane: Option<(Vector3, f64)> = None;
                if let Some(entries) = entries {
                    for &(input, surf) in entries {
                        match surf {
                            Surface::Cone {
                                apex,
                                axis_dir: cone_axis,
                                half_angle,
                            } => cone = Some((input, apex, cone_axis, half_angle)),
                            Surface::Plane { normal: pn, d: pd } => plane = Some((pn, pd)),
                            _ => {}
                        }
                    }
                }
                let (Some((cone_input, apex, cone_axis_dir, half_angle)), Some((plane_n, plane_d))) =
                    (cone, plane)
                else {
                    // A hyperbola section that is not a cone+plane pair is out of
                    // scope (producer fault). Loud STOP (P9/P10), mirroring the
                    // cone-parabola arm.
                    if std::env::var_os("YANG_LRR_PROBE").is_some() {
                        eprintln!(
                            "YANG_LRR_SITE site=hyperbola_pair_miss edge=({s},{e}) p={:?} \
                             entries={entries:?}",
                            mesh.verts.get(s as usize)
                        );
                    }
                    return Err(YangError::stage4_region_invalid(
                        s,
                        Stage4InvalidReason::LocalRefinementRequired,
                    ));
                };
                let owner = match cone_input {
                    InputId::A => a,
                    InputId::B => b,
                };
                let Some(cone_d_eps) =
                    cone_chord_budget_from_owner(apex, cone_axis_dir, half_angle, owner)
                else {
                    if std::env::var_os("YANG_LRR_PROBE").is_some() {
                        eprintln!(
                            "YANG_LRR_SITE site=hyperbola_cone_budget edge=({s},{e}) p={:?} \
                             apex={apex:?} half_angle={half_angle}",
                            mesh.verts.get(s as usize)
                        );
                    }
                    return Err(YangError::stage4_region_invalid(
                        s,
                        Stage4InvalidReason::LocalRefinementRequired,
                    ));
                };
                let chr = ConeHyperbolaReloc {
                    apex,
                    cone_axis_dir,
                    half_angle,
                    plane_n,
                    plane_d,
                    center,
                    normal,
                    major_axis,
                    semi_conjugate,
                    cone_d_eps,
                };
                for v in [s, e] {
                    // Increment 3: certified exact junction — enters no map (see above).
                    if exact_junctions.contains(&v) {
                        continue;
                    }
                    // KV16: a SECOND, DIFFERENT descriptor for the same
                    // vertex is a same-type conic junction (two hyperbolas
                    // meeting) — never silently overwrite-and-relocate onto
                    // one curve; route to the triple-junction pass.
                    if let Some(prev) = vert_cone_hyperbola.get(&v) {
                        let differs = prev.apex != chr.apex
                            || prev.cone_axis_dir != chr.cone_axis_dir
                            || prev.half_angle != chr.half_angle
                            || prev.plane_n != chr.plane_n
                            || prev.plane_d != chr.plane_d;
                        if differs {
                            same_type_junction.insert(v);
                            if std::env::var_os("YANG_SAMETYPE_PROBE").is_some() {
                                let pv = mesh.verts[v as usize].as_array();
                                eprintln!(
                                    "[sametype-probe] v={v} p=({:.6},{:.6},{:.6}) hyperbola \
                                     junction: prev apex={:?} ha={:.6} plane_n={:?} d={:.6} \
                                     -> new apex={:?} ha={:.6} plane_n={:?} d={:.6}",
                                    pv[0],
                                    pv[1],
                                    pv[2],
                                    prev.apex,
                                    prev.half_angle,
                                    prev.plane_n,
                                    prev.plane_d,
                                    chr.apex,
                                    chr.half_angle,
                                    chr.plane_n,
                                    chr.plane_d,
                                );
                            }
                        }
                    }
                    vert_cone_hyperbola.insert(v, chr);
                    endpoints.push(v);
                }
            }
            Curve::Circle {
                center,
                normal,
                radius,
            } => {
                // PR-YR19: scan this edge's incidence for a `Surface::Sphere`
                // owner → `Some(R)`; else `None`. Uses the SAME canonical key as
                // the Ellipse arm below.
                let key = if s < e { (s, e) } else { (e, s) };
                let mut source_radius: Option<f64> = None;
                if let Some(entries) = inc0.get(&key) {
                    for &(_input, surf) in entries {
                        if let Surface::Sphere { radius: sr, .. } = surf {
                            source_radius = Some(sr);
                        }
                    }
                }
                for v in [s, e] {
                    // Increment 3: certified exact junction — enters no map (see above).
                    if exact_junctions.contains(&v) {
                        continue;
                    }
                    if i1d_probe {
                        // I1e census: which edge chained this vertex onto the
                        // circle, and how far off the exact circle it sits.
                        let p = mesh.verts[v as usize];
                        let pa = p.as_array();
                        let ca = center.as_array();
                        let na = normalize3(normal.as_array());
                        let w = [pa[0] - ca[0], pa[1] - ca[1], pa[2] - ca[2]];
                        let h = w[0] * na[0] + w[1] * na[1] + w[2] * na[2];
                        let inplane = [w[0] - h * na[0], w[1] - h * na[1], w[2] - h * na[2]];
                        let rad = (inplane[0] * inplane[0]
                            + inplane[1] * inplane[1]
                            + inplane[2] * inplane[2])
                            .sqrt();
                        let resid = (h * h + (rad - radius) * (rad - radius)).sqrt();
                        let inc: Vec<String> = inc0
                            .get(&key)
                            .map(|entries| {
                                entries
                                    .iter()
                                    .map(|&(input, surf)| {
                                        let tag = match surf {
                                            Surface::Plane { .. } => "Plane",
                                            Surface::Cylinder { .. } => "Cyl",
                                            Surface::Sphere { .. } => "Sph",
                                            Surface::Cone { .. } => "Cone",
                                            Surface::Torus { .. } => "Torus",
                                        };
                                        format!("{input:?}:{tag}")
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();
                        eprintln!(
                            "[i1e-circle-edge] edge=({s},{e}) v{v} resid={resid:.6e} \
                             pos=({:.12}, {:.12}, {:.12}) r={radius:.9} inc=[{}]",
                            pa[0],
                            pa[1],
                            pa[2],
                            inc.join(","),
                        );
                    }
                    insert_circle_or_junction(
                        v,
                        (center, normal, radius, source_radius),
                        &mut vert_circle,
                        &mut vert_circle_junction,
                        &mut endpoints,
                    );
                }
            }
            Curve::Ellipse {
                center,
                normal,
                major_axis,
                major_radius,
                minor_radius,
            } => {
                // PR-YR11: identify the TRUE cylinder + cutting plane from this
                // edge's incidence (the two incident surfaces of DIFFERENT
                // inputs). A conic Ellipse edge is, by construction, one cylinder
                // lateral + one cutting plane.
                let key = if s < e { (s, e) } else { (e, s) };
                let entries = inc0.get(&key);
                let mut cyl: Option<(Point3, Vector3, f64)> = None;
                // PR-KV9: ALL cylinder entries with their owning inputs —
                // a cylinder×cylinder ellipse needs both for the per-point
                // gradient band + the combined chord budget.
                let mut cyls: Vec<(InputId, Point3, Vector3, f64)> = Vec::new();
                let mut plane: Option<(Vector3, f64)> = None;
                // PR-YR21: additionally scan for a `Surface::Cone` owner (the
                // cone+plane oblique section). Carry the owning `InputId` so the
                // cone's chord budget can be derived from its rim Circle.
                let mut cone: Option<(InputId, Point3, Vector3, f64)> = None;
                if let Some(entries) = entries {
                    for &(input, surf) in entries {
                        match surf {
                            Surface::Cylinder {
                                axis_point,
                                axis_dir,
                                radius,
                            } => {
                                cyl = Some((axis_point, axis_dir, radius));
                                cyls.push((input, axis_point, axis_dir, radius));
                            }
                            Surface::Plane { normal: pn, d: pd } => plane = Some((pn, pd)),
                            Surface::Cone {
                                apex,
                                axis_dir,
                                half_angle,
                            } => cone = Some((input, apex, axis_dir, half_angle)),
                            _ => {}
                        }
                    }
                }
                match (cyl, cone, plane) {
                    // YR11 cylinder + plane: the EXISTING path, byte-for-byte.
                    (Some((axis_point, axis_dir, radius)), _, Some((plane_n, plane_d))) => {
                        let er = EllipseReloc {
                            axis_point,
                            axis_dir,
                            radius,
                            plane_n,
                            plane_d,
                            center,
                            normal,
                            major_axis,
                            major_radius,
                            minor_radius,
                            second_cyl: None,
                        };
                        for v in [s, e] {
                            // Increment 3: certified exact junction — enters no map (see above).
                            if exact_junctions.contains(&v) {
                                continue;
                            }
                            insert_ellipse_or_junction(
                                v,
                                er,
                                &mut vert_ellipse,
                                &mut vert_ell_junction,
                                &mut endpoints,
                            );
                        }
                    }
                    // PR-YR21 cone + plane (no cylinder): the new cone-ellipse
                    // path. Derive the cone's OWN chord budget from the cone
                    // owner's rim Circle (spec §3.3); a cone owner with no rim
                    // Circle is a producer fault → loud STOP (never TAU_WORK).
                    (
                        None,
                        Some((cone_input, apex, axis_dir, half_angle)),
                        Some((plane_n, plane_d)),
                    ) => {
                        let owner = match cone_input {
                            InputId::A => a,
                            InputId::B => b,
                        };
                        let Some(cone_d_eps) =
                            cone_chord_budget_from_owner(apex, axis_dir, half_angle, owner)
                        else {
                            if std::env::var_os("YANG_LRR_PROBE").is_some() {
                                eprintln!(
                                    "YANG_LRR_SITE site=cone_ellipse_budget edge=({s},{e}) \
                                     p={:?} apex={apex:?} half_angle={half_angle}",
                                    mesh.verts.get(s as usize)
                                );
                            }
                            return Err(YangError::stage4_region_invalid(
                                s,
                                Stage4InvalidReason::LocalRefinementRequired,
                            ));
                        };
                        let cer = ConeEllipseReloc {
                            apex,
                            axis_dir,
                            half_angle,
                            plane_n,
                            plane_d,
                            center,
                            normal,
                            major_axis,
                            major_radius,
                            minor_radius,
                            cone_d_eps,
                        };
                        for v in [s, e] {
                            // Increment 3: certified exact junction — enters no map (see above).
                            if exact_junctions.contains(&v) {
                                continue;
                            }
                            // KV16b (spec `kv16b_cone_ellipse_same_type_junction`):
                            // a SECOND, DIFFERENT descriptor for the same vertex
                            // is a same-type conic junction (two cone-ellipses
                            // meeting) — never silently overwrite-and-relocate
                            // onto one curve; route to the triple-junction pass
                            // (the KV16 hyperbola recipe, sibling map).
                            if let Some(prev) = vert_cone_ellipse.get(&v) {
                                let differs = prev.apex != cer.apex
                                    || prev.axis_dir != cer.axis_dir
                                    || prev.half_angle != cer.half_angle
                                    || prev.plane_n != cer.plane_n
                                    || prev.plane_d != cer.plane_d;
                                if differs {
                                    same_type_junction.insert(v);
                                    if std::env::var_os("YANG_SAMETYPE_PROBE").is_some() {
                                        let pv = mesh.verts[v as usize].as_array();
                                        eprintln!(
                                            "[sametype-probe] v={v} p=({:.6},{:.6},{:.6}) \
                                             cone-ellipse junction: prev apex={:?} ha={:.6} \
                                             plane_n={:?} d={:.6} -> new apex={:?} ha={:.6} \
                                             plane_n={:?} d={:.6}",
                                            pv[0],
                                            pv[1],
                                            pv[2],
                                            prev.apex,
                                            prev.half_angle,
                                            prev.plane_n,
                                            prev.plane_d,
                                            cer.apex,
                                            cer.half_angle,
                                            cer.plane_n,
                                            cer.plane_d,
                                        );
                                    }
                                }
                            }
                            vert_cone_ellipse.insert(v, cer);
                            endpoints.push(v);
                        }
                    }
                    // PR-KV9: cylinder × CYLINDER ellipse (the equal-radius
                    // intersecting-axes Steinmetz section, ssi cyl∩cyl). The
                    // ellipse lies in a KNOWN plane — its own stored frame —
                    // and it equals `cylinder ∩ that-plane` for EITHER owner
                    // (the curve is on both), so the existing cylinder+plane
                    // relocation closed form applies verbatim with the plane
                    // derived from the stored curve: n̂ from the ellipse
                    // normal, d = −n̂·center. `cyl` here holds the LAST
                    // cylinder scanned; with two cylinder entries either is
                    // exact, and the incidence order is deterministic.
                    (Some(_), None, None) if cyls.len() == 2 => {
                        // Deterministic owner order: sort by InputId (A first).
                        let mut cs = cyls.clone();
                        cs.sort_by_key(|&(i, ..)| matches!(i, InputId::B));
                        let (i1, axis_point, axis_dir, radius) = cs[0];
                        let (i2, ap2, ad2, _) = cs[1];
                        let budget = chord_tol_for_curved_owner(i1, a, b, 0, (s, e))?
                            + chord_tol_for_curved_owner(i2, a, b, 0, (s, e))?;
                        let nn = normalize3(normal.as_array());
                        let plane_n = Vector3::new(nn[0], nn[1], nn[2]);
                        let c = center.as_array();
                        let plane_d = -(nn[0] * c[0] + nn[1] * c[1] + nn[2] * c[2]);
                        let er = EllipseReloc {
                            axis_point,
                            axis_dir,
                            radius,
                            plane_n,
                            plane_d,
                            center,
                            normal,
                            major_axis,
                            major_radius,
                            minor_radius,
                            second_cyl: Some((ap2, ad2, budget)),
                        };
                        for v in [s, e] {
                            // Increment 3: certified exact junction — enters no map (see above).
                            if exact_junctions.contains(&v) {
                                continue;
                            }
                            insert_ellipse_or_junction(
                                v,
                                er,
                                &mut vert_ellipse,
                                &mut vert_ell_junction,
                                &mut endpoints,
                            );
                        }
                    }
                    // Anything else (sphere, coplanar multi-solid): out of
                    // scope. Loud STOP (P9/P10).
                    _ => {
                        if std::env::var_os("YANG_LRR_PROBE").is_some() {
                            eprintln!(
                                "YANG_LRR_SITE site=ellipse_combo edge=({s},{e}) p={:?} \
                                 n_cyls={} entries={entries:?}",
                                mesh.verts.get(s as usize),
                                cyls.len()
                            );
                        }
                        return Err(YangError::stage4_region_invalid(
                            s,
                            Stage4InvalidReason::LocalRefinementRequired,
                        ));
                    }
                }
            }
            // M5 (Y4): a procedural surface-pair edge carries its two defining
            // surfaces directly. Like the TORUS block, its endpoints are an
            // implicit-pair (degree-4) relocation handled AFTER the conic
            // audit below — NOT part of the conic `endpoints`/`relocations`
            // bookkeeping (a procedural curve has no `t`). Only record the
            // pair here.
            Curve::SurfacePair { a, b } => {
                // M5 torus arm (YT5): a torus-bearing pair (Stage 3 tags it
                // as a `SurfacePair`) is relocated by the (2t) torus
                // block below, which keys on the edge's surface INCIDENCE,
                // not its curve tag — exactly as the same edge was when it
                // carried `LineSegment`. Recording it here too would relocate
                // it twice under two different junction rules. Defer.
                if matches!(a, Surface::Torus { .. }) || matches!(b, Surface::Torus { .. }) {
                    continue;
                }
                for v in [s, e] {
                    // Increment 3: certified exact junction — enters no map (see above).
                    if exact_junctions.contains(&v) {
                        continue;
                    }
                    // KV16 precedent for the PROCEDURAL map (2026-08-19,
                    // R0044 anchor): a SECOND, DIFFERENT surface pair at the
                    // same vertex is a same-type surface-pair junction — e.g.
                    // cyl_A × cone_B1 meeting cyl_A × cone_B2 on the gear's
                    // tooth-flank crease circle: THREE surfaces. The one-slot
                    // map kept the LAST pair, the triple block's `n_maps < 2`
                    // skipped the vertex, the pair loop relocated it onto
                    // (cyl, cone_B2) alone, and the OTHER edge's endpoint was
                    // 0.35 off cone_B1 (kernel-v2's surface-pair endpoint
                    // check caught it loudly). Route to the triple pass.
                    if let Some(&(pa, pb)) = vert_surface_pair.get(&v) {
                        let same = (pa == a && pb == b) || (pa == b && pb == a);
                        if !same {
                            same_type_junction.insert(v);
                            if std::env::var_os("YANG_SAMETYPE_PROBE").is_some() {
                                eprintln!(
                                    "[sametype-probe] v={v} p={:?} surface-pair junction: \
                                     prev=({pa:?}, {pb:?}) -> new=({a:?}, {b:?})",
                                    mesh.verts.get(v as usize)
                                );
                            }
                        }
                    }
                    vert_surface_pair.insert(v, (a, b));
                }
            }
            Curve::LineSegment => {
                // PR-F3: a LineSegment intersection edge between a PLANE and a
                // CYLINDER is a ruling LINE of the cylinder (ssi plane_cylinder
                // C3a/C3b). Recompute the exact line from the incidence and
                // re-select the unique candidate through both endpoints (the
                // SAME rule Stage 3's `build_intersection_curves` used).
                // Plane∩plane segments are exact → skip. Any OTHER curved
                // surface on a LineSegment edge is out of scope → loud STOP
                // (P9).
                //
                // The `cone × plane` GENERATOR line is the third convertible
                // pair (the fixture the PR-F3 note deferred has arrived:
                // corpus R0008 + R0085-op2). A cutting plane through the cone
                // APEX degenerates the conic section into generator lines
                // (`ssi_rs::plane_cone` AP-line / AP-lines) — the same
                // recompute-and-reselect rule, with the CONE owner's Stage-1
                // band (`cone_chord_tol_for_owner`, PR-YR17) as `tol`, exactly
                // as Stage 3 derives it for a cone-owning edge. See the
                // band note at `line_tol` below for why the pair takes the
                // FLAT band and why that is the derived value, not a default.
                let key = if s < e { (s, e) } else { (e, s) };
                let Some(entries) = inc0.get(&key) else {
                    continue;
                };
                // KV6d Tier B: a TORUS-bearing LineSegment edge is a degree-4
                // intersection handled by the implicit-pair Newton relocation
                // block after this scan — defer it here (the conic LineSegment
                // arm has no closed form for it). Skip rather than STOP.
                if entries
                    .iter()
                    .any(|&(_, s)| matches!(s, Surface::Torus { .. }))
                {
                    continue;
                }
                let mut cyls: Vec<(InputId, Surface)> = Vec::new();
                let mut cones: Vec<(InputId, Surface)> = Vec::new();
                let mut plane_surf: Option<Surface> = None;
                let mut pp: Vec<(Vector3, f64)> = Vec::new();
                let mut other_curved = false;
                for &(input, surf) in entries {
                    match surf {
                        Surface::Cylinder { .. } => cyls.push((input, surf)),
                        Surface::Cone { .. } => cones.push((input, surf)),
                        Surface::Plane { normal, d } => {
                            plane_surf = Some(surf);
                            pp.push((normal, d));
                        }
                        _ => other_curved = true,
                    }
                }
                // Three convertible pairs: cylinder × ⊥plane (F3), PARALLEL
                // cylinder × cylinder (PR-KV9, ssi cyl∥cyl ruling lines), and
                // cone × through-apex plane (the generator arm). Other
                // curved-bearing line edges stay a loud STOP. Every arm is
                // guarded on the OTHER curved kind being absent so a
                // three-surface incidence (cyl + cone + plane) cannot be
                // silently read as a two-surface pair.
                let (surf_a, surf_b, tol) = match (cyls.as_slice(), plane_surf) {
                    // Cone × plane FIRST: `cones` is not part of the scrutinee,
                    // so the `([], _)` plane∩plane arm below would otherwise
                    // swallow a cone-bearing edge as an exact pp segment.
                    ([], Some(pl)) if !other_curved && cones.len() == 1 => {
                        let (ci, cs) = cones[0];
                        (cs, pl, cone_chord_tol_for_owner(cs, ci, a, b, 0, (s, e))?)
                    }
                    ([(ci, cs)], Some(pl)) if !other_curved && cones.is_empty() => {
                        (*cs, pl, chord_tol_for_curved_owner(*ci, a, b, 0, (s, e))?)
                    }
                    ([(i1, c1), (i2, c2)], None) if !other_curved && cones.is_empty() => {
                        // Both meshes' facet chords contribute to the crossing
                        // vertex — the combined band is the SUM of the two
                        // owners' Stage-1 bounds (derived, not widening).
                        let t = chord_tol_for_curved_owner(*i1, a, b, 0, (s, e))?
                            + chord_tol_for_curved_owner(*i2, a, b, 0, (s, e))?;
                        (*c1, *c2, t)
                    }
                    ([], _) if !other_curved && cones.is_empty() => {
                        // plane∩plane — the segment is exact, but record the
                        // line's planes per endpoint for the PR-KV11 triple-
                        // point pass below.
                        if pp.len() == 2 {
                            let entry = (pp[0].0, pp[0].1, pp[1].0, pp[1].1);
                            for v in [s, e] {
                                // Increment 3: certified exact junction — enters no map (see above).
                                if exact_junctions.contains(&v) {
                                    continue;
                                }
                                vert_pp_planes.entry(v).or_default().push(entry);
                            }
                        }
                        continue;
                    }
                    _ => {
                        if std::env::var_os("YANG_LRR_PROBE").is_some() {
                            eprintln!(
                                "YANG_LRR_SITE site=lineseg_combo edge=({s},{e}) p={:?} \
                                 n_cyls={} n_cones={} n_pp={} other_curved={other_curved} \
                                 entries={entries:?}",
                                mesh.verts.get(s as usize),
                                cyls.len(),
                                cones.len(),
                                pp.len()
                            );
                        }
                        return Err(YangError::stage4_region_invalid(
                            s,
                            Stage4InvalidReason::LocalRefinementRequired,
                        ));
                    }
                };
                let to_ssi_err = |reason| YangError::SsiRefinementFailed {
                    edge: (s, e),
                    reason,
                };
                let q0 = surface_to_quadric(surf_a).map_err(to_ssi_err)?;
                let q1 = surface_to_quadric(surf_b).map_err(to_ssi_err)?;
                let returned =
                    ssi_rs::intersect(&q0, &q1).map_err(|err| YangError::SsiRefinementFailed {
                        edge: (s, e),
                        reason: SsiRefinementError::IntersectFailed(err),
                    })?;
                let p_s = mesh.verts[s as usize];
                let p_e = mesh.verts[e as usize];
                // PR-F3b: the SAME propagated band as Stage-3 matching (the
                // metric is shared, so every gate carries the factor).
                // N46 (task #164): a `cylinder ∩ plane` generator uses the EXACT
                // worst-case band `√(B_in² + tol²)` (superseding the first-order
                // `line_band_amplification`, which under-admits near tangency —
                // R0026's `AmbiguousCurve{2,0}` reaches THIS Stage-4 relocation
                // once Stage-3 selection passes). Non-cyl/plane pairs keep the
                // linear factor (cyl∩cyl Steinmetz, cone-apex lines).
                //
                // CONE-APEX GENERATORS take the flat band, and that is the
                // DERIVED value rather than a fallback default. Both helpers
                // return `None` for a cone pair, so the amplification is 1.0 —
                // which is exactly right here: the general membership factor is
                // `1/‖ĝ_plane × ĝ_cone‖` (the form the cyl×plane `r/√(r²−d²)`
                // specializes), and along a generator d̂ = â·cosα + û·sinα the
                // cone's unit normal is `n̂_c = û·cosα − â·sinα`. A plane that
                // meets the cone in TWO crossed generators contains the axis
                // (`k = n̂·â = 0` — the AP-lines branch), so û = ±(n̂ × â) ⊥ n̂
                // and â ⊥ n̂ give `n̂ · n̂_c = 0`: the gradients are exactly
                // orthogonal, sin = 1, amplification = 1. The TANGENT-generator
                // case (AP-line, one candidate) has n̂ ∥ n̂_c and a diverging
                // factor; the flat band UNDER-admits there, so such an edge
                // fails `matched_n == 1` and STOPs loud — the P9-correct
                // posture, never a silent match. This matches Stage 3's Line
                // band for the same pair byte-for-byte, so selection here
                // cannot disagree with the selection that produced the edge.
                let line_tol = cyl_plane_generator_band(surf_a, surf_b, tol).unwrap_or_else(|| {
                    line_band_amplification(surf_a, surf_b).unwrap_or(1.0) * tol
                });
                let mut matched: Option<LineReloc> = None;
                let mut matched_n = 0usize;
                let mut matched_lines: Vec<(Point3, Vector3)> = Vec::new();
                for c in &returned {
                    if let ssi_rs::SsiCurve::Line { point, dir } = *c {
                        if line_perp_distance(p_s, point, dir) <= line_tol
                            && line_perp_distance(p_e, point, dir) <= line_tol
                        {
                            matched_n += 1;
                            matched_lines.push((point, dir));
                            matched = Some(LineReloc {
                                point,
                                dir,
                                band_budget: line_tol,
                            });
                        }
                    }
                }
                // R0072: near-tangent plane∩cylinder yields two near-coincident
                // parallel generators that both pass the band; the edge lies on
                // exactly one. Break the tie by position (the disjoint-lowest
                // endpoint-distance interval) — the SAME rule Stage 3 uses. If no
                // unambiguous winner (overlapping intervals), the loud
                // `AmbiguousCurve` below stands.
                //
                // R0008: this site used the R0072-only
                // `select_disjoint_parallel_line` wrapper, whose mutual-parallelism
                // precheck rejects the two CROSSING generators of a cone sectioned
                // through its apex. Stage 3 was generalized to the
                // parallelism-free core by N45 (#163, commit 9fca8393) and this
                // site was not, so the two stages have been running DIFFERENT
                // tie-breaks — a latent violation of the "selection here cannot
                // disagree with Stage 3" contract this arm rests on. It was
                // unobservable while every cone-apex edge STOPped earlier, in the
                // pair match above. Calling the core restores the invariant; the
                // criterion is identical for parallel candidates (the wrapper
                // delegates to it), so the R0072 path is unchanged.
                if matched_n > 1 {
                    if let Some(wk) = select_disjoint_line_by_distance(&matched_lines, p_s, p_e) {
                        let (point, dir) = matched_lines[wk];
                        matched_n = 1;
                        matched = Some(LineReloc {
                            point,
                            dir,
                            band_budget: line_tol,
                        });
                    }
                }
                let Some(lr) = (if matched_n == 1 { matched } else { None }) else {
                    // Stage-4 diagnosis probe (read-only, env-gated): this
                    // site reports the SAME `AmbiguousCurve` text as Stage 3's
                    // three producers, and `Display` even labels it "Stage-3",
                    // so a `{0,0}` is otherwise unlocalizable (measured on
                    // P0016, 2026-10-03).
                    if std::env::var_os("YANG_S3_AMBIG_PROBE").is_some() {
                        eprintln!(
                            "[s3-ambig-probe] STAGE-4 LINE ARM edge ({s},{e}) \
                             candidates={} matched={matched_n} line_tol={line_tol:.3e}\n  \
                             surf_a={surf_a:?}\n  surf_b={surf_b:?}",
                            returned.len()
                        );
                    }
                    return Err(YangError::SsiRefinementFailed {
                        edge: (s, e),
                        reason: SsiRefinementError::AmbiguousCurve {
                            candidates: returned.len(),
                            matched: matched_n,
                        },
                    });
                };
                for v in [s, e] {
                    if let Ok(list) = std::env::var("YANG_V_PROBE") {
                        if list.split(',').any(|t| t.trim().parse::<u32>() == Ok(v)) {
                            eprintln!(
                                "YANG_V_PROBE line_arm v={v} edge=({s},{e}) p={:?} \
                                 surf_a={surf_a:?} surf_b={surf_b:?} line={lr:?} \
                                 exact_junction={}",
                                mesh.verts.get(v as usize),
                                exact_junctions.contains(&v)
                            );
                        }
                    }
                    // Increment 3: certified exact junction — enters no map (see above).
                    if exact_junctions.contains(&v) {
                        continue;
                    }
                    // A vertex on TWO DIFFERENT lines is a line∩line junction:
                    // two generators crossing where a crease pierces a plane —
                    // a THREE-surface corner ({plane, cyl_A, cyl_B} or
                    // {plane, plane, cyl}). P0027 (2026-10-08): route it to
                    // the triple block instead of STOPping here (the KV16
                    // same-type precedent); the block's residue audit below
                    // restores this exact STOP for a pair it cannot resolve.
                    // A THIRD distinct line (≥ 4 surfaces) stays the loud
                    // STOP, as does the pre-flip path under the knob — never
                    // a silent overwrite (the defect class F3 fixes for
                    // line+circle).
                    if let Some(prev) = vert_line.get(&v) {
                        let same = same_line(prev, &lr);
                        if !same {
                            let third = vert_line_junction
                                .get(&v)
                                .is_some_and(|second| !same_line(second, &lr));
                            if third {
                                if std::env::var_os("YANG_LRR_PROBE").is_some() {
                                    eprintln!(
                                        "YANG_LRR_SITE site=line_line_junction v={v} p={:?} \
                                         prev={prev:?} new={lr:?} third={third}",
                                        mesh.verts.get(v as usize)
                                    );
                                }
                                return Err(YangError::stage4_region_invalid(
                                    v,
                                    Stage4InvalidReason::LocalRefinementRequired,
                                ));
                            }
                            if std::env::var_os("YANG_SAMETYPE_PROBE").is_some() {
                                eprintln!(
                                    "[sametype-probe] v={v} p={:?} line junction: \
                                     prev={prev:?} -> new={lr:?}",
                                    mesh.verts.get(v as usize)
                                );
                            }
                            vert_line_junction.insert(v, lr);
                            endpoints.push(v);
                            continue;
                        }
                    }
                    vert_line.insert(v, lr);
                    endpoints.push(v);
                }
            }
        }
    }

    // PR-KV11: resolve ellipse × (plane∩plane line) TRIPLE points. An ellipse
    // endpoint that also terminates an exact pp-segment (the cap∩face trace
    // crossing the lateral) must land on `(plane ∩ plane) ∩ cylinder`, not on
    // the ellipse alone — reuse the ellipse-junction closed form with a
    // synthetic second member carrying the line's OTHER plane (the one that
    // is not the ellipse's own cutting plane; bit identity — both come from
    // the same incidence `Surface::Plane` values).
    {
        let shared: Vec<u32> = vert_ellipse
            .keys()
            .filter(|v| vert_pp_planes.contains_key(v))
            .copied()
            .collect();
        for v in shared {
            let e_a = vert_ellipse[&v];
            let mut others: Vec<(Vector3, f64)> = Vec::new();
            for &(n1, d1, n2, d2) in &vert_pp_planes[&v] {
                let m1 = n1.as_array() == e_a.plane_n.as_array() && d1 == e_a.plane_d;
                let m2 = n2.as_array() == e_a.plane_n.as_array() && d2 == e_a.plane_d;
                let other = if m1 {
                    Some((n2, d2))
                } else if m2 {
                    Some((n1, d1))
                } else {
                    None
                };
                if let Some(o) = other {
                    if !others
                        .iter()
                        .any(|&(n, d)| n.as_array() == o.0.as_array() && d == o.1)
                    {
                        others.push(o);
                    }
                }
            }
            match others.len() {
                // A pp-line through an ellipse endpoint whose pair does not
                // include the ellipse's own plane, or more than one distinct
                // crossing line: relocating onto any single curve leaves the
                // vertex off the others — loud STOP, never a silent pick
                // (P9/P10).
                0 | 2.. => {
                    return Err(YangError::stage4_region_invalid(
                        v,
                        Stage4InvalidReason::LocalRefinementRequired,
                    ));
                }
                1 => {
                    let (on, od) = others[0];
                    let e_b = EllipseReloc {
                        plane_n: on,
                        plane_d: od,
                        ..e_a
                    };
                    vert_ellipse.remove(&v);
                    vert_ell_junction.insert(v, (e_a, e_b));
                }
            }
        }
    }

    // PR-F3: a vertex shared by a LINE edge and a CIRCLE edge is a TRIPLE
    // point — it must end up on BOTH curves. Relocating onto either alone
    // leaves it off the other (the KV6b-F3 probe defect: radius exactly r,
    // axial coordinate off by the sagitta → output-face plane vs Newell
    // disagreement). The exact junction is `line ∩ plane-of-circle`: the line
    // lies ON the cylinder and the circle IS `cylinder ∩ circle-plane`, so the
    // line's piercing of the circle plane lies exactly on the circle. Pull
    // such vertices OUT of both single-curve maps into a junction map.
    let mut vert_junction: BTreeMap<u32, (LineReloc, CircleAssign)> = BTreeMap::new();
    {
        let shared: Vec<u32> = vert_line
            .keys()
            .filter(|v| vert_circle.contains_key(v))
            .copied()
            .collect();
        for v in shared {
            let lr = vert_line.remove(&v).expect("key from vert_line");
            let circ = vert_circle.remove(&v).expect("checked contains_key");
            vert_junction.insert(v, (lr, circ));
        }
    }

    // Task #146 (spec `yang_stage4_circle_pp_line_junction` branches 1–3):
    // resolve CIRCLE × (plane∩plane line) TRIPLE points — the circle analog
    // of the PR-KV11 ellipse×pp pass above. A vertex on both a section
    // circle and an exact pp-line is their junction; relocating onto the
    // circle alone slides it off the line's planes at real scale (the F0064
    // Newell-normal class). Exactly ONE distinct pp-line reroutes; zero or
    // several distinct lines (or an ellipse junction already claimed) is
    // over-determined — loud STOP, never a silent pick (P9/P10).
    let mut vert_pp_circle_junction: BTreeMap<u32, ((Point3, Vector3), CircleAssign)> =
        BTreeMap::new();
    {
        let shared: Vec<u32> = vert_circle
            .keys()
            .filter(|v| vert_pp_planes.contains_key(v))
            .copied()
            .collect();
        for v in shared {
            if vert_ell_junction.contains_key(&v) {
                return Err(YangError::stage4_region_invalid(
                    v,
                    Stage4InvalidReason::LocalRefinementRequired,
                ));
            }
            let Some((n1, d1, n2, d2)) = dedup_single_pp_line(&vert_pp_planes[&v]) else {
                return Err(YangError::stage4_region_invalid(
                    v,
                    Stage4InvalidReason::LocalRefinementRequired,
                ));
            };
            let Some((lp, ld)) = pp_line(n1, d1, n2, d2) else {
                return Err(YangError::stage4_region_invalid(
                    v,
                    Stage4InvalidReason::LocalRefinementRequired,
                ));
            };
            let circ = vert_circle.remove(&v).expect("checked contains_key");
            if i1d_probe {
                let p = mesh.verts[v as usize];
                eprintln!(
                    "[i1d-classify] v{v} pre=({:.12}, {:.12}, {:.12})",
                    p.x(),
                    p.y(),
                    p.z()
                );
            }
            vert_pp_circle_junction.insert(v, ((lp, ld), circ));
        }
    }

    // Increment 5 (spec `yang_stage4_conic_triple_junction`, WIRED): a
    // vertex on ≥2 single-curve maps whose inc0 incidence dedups to EXACTLY
    // 3 distinct surfaces is NOT ambiguous — it is the unique transversal
    // common point of those surfaces (the R0017-class prism-edge ×
    // cone-lateral junction: exact on both planes, chord-inexact on the
    // cone). Relocate it onto all three via the torus-block triple primitive
    // instead of letting the over-determined audits below STOP. Newton
    // failure leaves the vertex in its maps — the audits then STOP exactly
    // as today (spec branch table). 2- or ≥4-surface configurations are
    // untouched (spec I2).
    //
    // The R0044 BUCKET (R0044, R0020, R0035): `vert_surface_pair` joins the
    // six conic maps as a curve-bearing map here. A procedural M5 surface-pair
    // curve is a curve through the vertex exactly as a conic is — it is held
    // apart from the conic bookkeeping only because it has no parameter `t`,
    // not because it is a lesser claim on the vertex. Omitting it made every
    // ellipse × surface-pair junction score `n_maps == 1`, fall out of this
    // block, and reach the surface-pair loop's `endpoint_set` guard as the
    // "out of v1 scope" endpoint-MIX STOP — while its incidence was the plain
    // 3-surface triple this block already resolves. Probed: R0044 v8
    // {cyl_A, plane_B, cone_B}, R0020 v44 {plane_A, cone_A, cyl_B}, R0035
    // v194/195 {cyl_A, cyl_B, plane_B} — every one exactly 3, every one
    // ellipse + pair. Nothing about the mix needed new machinery; the mix was
    // never the difficulty.
    let mut triple_moved: Vec<u32> = Vec::new();
    // R0050 (2026-09-12): every vertex this block RESOLVED (moved or already
    // exact), so the torus block below neither STOPs on it as a torus×conic
    // endpoint mix nor re-relocates it onto two of its three surfaces.
    let mut triple_resolved: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
    // R0050: the endpoints of TORUS intersection edges. A torus edge is
    // untyped (no conic map; the torus block relocates it by implicit-pair
    // Newton), so a vertex where a torus edge meets a conic edge counted
    // ONE curve here and then hit the torus block's endpoint-mix STOP —
    // although with exactly three incident surfaces it is the plain
    // {torus, s1, s2} corner both blocks already solve (spec
    // `yang_stage4_conic_triple_junction`, "Junction-map candidates").
    let torus_edge_verts: std::collections::BTreeSet<u32> = inc0
        .iter()
        .filter(|(_, entries)| {
            entries
                .iter()
                .any(|(_i, surf)| matches!(surf, Surface::Torus { .. }))
        })
        .flat_map(|(&(s, e), _)| [s, e])
        .collect();
    {
        let mut cand: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
        for v in vert_circle
            .keys()
            .chain(vert_ellipse.keys())
            .chain(vert_cone_ellipse.keys())
            .chain(vert_parabola.keys())
            .chain(vert_cone_hyperbola.keys())
            .chain(vert_line.keys())
            .chain(vert_surface_pair.keys())
            .chain(vert_circle_junction.keys())
            .chain(vert_ell_junction.keys())
            .chain(torus_edge_verts.iter())
        {
            cand.insert(*v);
        }
        // §4.5.1 inc-2c-3b-12: the crease index, built ONCE (O(edges)); the
        // per-vertex certificate only looks up its own surfaces' creases.
        // inc-3c (2026-09-04): the domain check + the emission chain are
        // ALWAYS-ON — unset = act quietly, `census` = act and report every
        // stage to stderr, `1`/`on` = act and STOP typed on a site the
        // emission could not repair (the `0`/`off` pre-flip path was deleted
        // 2026-10-16, cleanup spec §4).
        let dom_mode = std::env::var("YANG_451_TRIPLE_DOMAIN").unwrap_or_default();
        let dom_stop = cfg!(debug_assertions) && (dom_mode == "1" || dom_mode == "on");
        // The anatomy → emission sub-chain is always on; the knob's pre-flip
        // role (any value = arm) survives as "set = report".
        let trace = dom_mode == "census" || std::env::var_os("YANG_451_TRANSIT_ANATOMY").is_some();
        macro_rules! say {
            ($($t:tt)*) => {
                if trace {
                    eprintln!($($t)*);
                }
            };
        }
        let creases_by_surf = crate::stage4_boundary_curve::creases_by_surface(&inc0);
        for v in cand {
            // Admission is by INCIDENCE, not by curve-type bookkeeping
            // (2026-10-13). A vertex incident to exactly three distinct
            // surfaces is a triple point whatever curves carry it (Yang
            // §4.5.1: the corner is the solution of three surface equations);
            // the former gate counted how many single-curve maps held the
            // vertex and had to be re-opened seven times — KV16 same-type,
            // R0044 pair, C0067 circle pair, R0070 / P0004 plane-pair conic,
            // R0050 torus mix, P0025 coplanar ellipse pair, P0027 line pair —
            // each a corner one map "counted zero" for. The surface count
            // below is the paper's predicate and the only one; 2 and ≥ 4
            // surfaces keep the loud audits (I2).
            let probe_v = std::env::var_os("YANG_SAMETYPE_PROBE").is_some();
            let Some(surfs) = vert_surfs.get(&v) else {
                if probe_v {
                    eprintln!("[triple-bail] v={v} no vert_surfs entry");
                }
                continue;
            };
            if surfs.len() != 3 {
                if probe_v {
                    eprintln!("[triple-bail] v={v} surfs={}", surfs.len());
                }
                continue; // 2 / ≥4 surfaces keep the loud audits (I2)
            }
            let p = mesh.verts[v as usize];
            let Some(mut proj) = relocate_onto_implicit_triple(p, surfs[0], surfs[1], surfs[2])
            else {
                if probe_v {
                    eprintln!("[triple-bail] v={v} newton diverged");
                }
                continue; // Newton diverged → the audits STOP loudly
            };
            let qa = proj.as_array();
            let (Some((_, n0)), Some((_, n1))) = (
                surface_value_and_normal(surfs[0], qa),
                surface_value_and_normal(surfs[1], qa),
            ) else {
                continue; // evaluation failed → the audits STOP loudly
            };
            // Derived displacement gate: a chord vertex moves to the exact
            // junction by ≤ 2·d_ε / sin θ (the torus-block metric — NOT a
            // tolerance widening). Beyond it is a real off-curve error. Two
            // planes among the three (a box edge piercing the third surface):
            // the vertex moves along their LINE and the bound is the PR-KV11
            // line metric (`junction_line_divergence`; the torus block's
            // R0077 amendment, one metric for every plane-pair junction).
            let pa = p.as_array();
            let rho = ((qa[0] - pa[0]).powi(2) + (qa[1] - pa[1]).powi(2) + (qa[2] - pa[2]).powi(2))
                .sqrt();
            let cx = [
                n0[1] * n1[2] - n0[2] * n1[1],
                n0[2] * n1[0] - n0[0] * n1[2],
                n0[0] * n1[1] - n0[1] * n1[0],
            ];
            // R0070 (2026-09-17): the line may also be an exact Stage-3 LINE
            // curve the vertex terminates (a plane∩cylinder generator, a
            // Steinmetz line) — the same along-the-line move, the line
            // supplied by `vert_line` instead of two planes
            // (`junction_line_curve_divergence`, certificate inside).
            let plane_pair_div = junction_line_divergence([surfs[0], surfs[1], surfs[2]], qa);
            let line_curve_div = if plane_pair_div.is_none() {
                vert_line.get(&v).and_then(|lr| {
                    junction_line_curve_divergence(
                        (lr.point, lr.dir),
                        pa,
                        [surfs[0], surfs[1], surfs[2]],
                        qa,
                    )
                })
            } else {
                None
            };
            let line_div = plane_pair_div.or(line_curve_div);
            // R0050 (2026-09-22): the THREE-SLAB arm — the chord contract on
            // all three surfaces bounds the move (`junction_slab_divergence`,
            // certificate inside); taken only where it admits MORE than the
            // pair corridor, so the gate is never below it.
            let pair_sin = (cx[0] * cx[0] + cx[1] * cx[1] + cx[2] * cx[2]).sqrt();
            let slab_div = if line_div.is_none() {
                crate::stage4_relocate::junction_slab_divergence([surfs[0], surfs[1], surfs[2]], qa)
                    .filter(|s| *s < pair_sin)
            } else {
                None
            };
            let sin_theta = line_div.or(slab_div).unwrap_or(pair_sin);
            let gate = tangent_plane_corridor(d_eps, sin_theta);
            if probe_v || std::env::var_os("YANG_LRR_PROBE").is_some() {
                let metric = if plane_pair_div.is_some() {
                    "line"
                } else if line_curve_div.is_some() {
                    "line-curve"
                } else if slab_div.is_some() {
                    "slab"
                } else {
                    "curve"
                };
                eprintln!(
                    "[triple-gate] v={v} rho={rho:.4e} gate={gate:.4e} d_eps={d_eps:.4e} \
                     sin_theta={sin_theta:.4e} metric={metric} p={:?} q={qa:?} surfs={surfs:?}",
                    pa
                );
            }
            if rho > gate {
                if let Some(e) = s451_stop(s451_collect, &mut s45_failures, v, &curves0, &inc0) {
                    return Err(e);
                }
                continue;
            }
            // §4.5.1 inc-2c-3b-12 (spec `specs/yang_451_corner_transit.md`):
            // the relocation DOMAIN certificate. The triple Newton solves the
            // three EXTENDED implicit surfaces; a solution is only a B-Rep
            // junction if it also lies within the TRIMMED domain of the faces
            // that claimed it. Crossing one of the operand's own creases is
            // the paper's own words for the defect — "a full step length that
            // takes the point to a position p1 outside the surface S2 where
            // the point is initially located" — and its prescription is to
            // truncate the step to the crease and transit onto the neighbour
            // (§4.5.1 Fig. 12 c/d/e, the q-points on C_b). This increment
            // lands the DETECTION half only; the repair is 3b-12b.
            //
            // Always-on since inc-3c: the emission repairs what it can certify
            // and the run continues either way (the pre-flip `census`
            // semantics, quiet); `1`/`on` STOPs typed at an unrepaired site
            // instead of letting an out-of-domain junction surface three
            // stages later as a rejected output ring on a different face.
            let creases =
                crate::stage4_boundary_curve::creases_for_surfaces(&creases_by_surf, surfs);
            // inc-2c-3b-12b-8: set when the emission fill was WRITTEN at
            // this site; then `proj` is the corrected junction and the
            // STOP below does not apply.
            let mut repaired = false;
            if let Some((ci, fp, fq)) =
                crate::stage4_boundary_curve::crease_crossed_by_step(p, proj, &creases)
            {
                // Census diagnostics: how far the PRE position sits from
                // the two surfaces forming the crossed crease, each against
                // its own certificate band — the discriminator between a
                // vertex that RIDES the crease (residual sign is noise) and
                // one that starts materially inside its face.
                let (cs0, cs1) = (creases[ci].1, creases[ci].2);
                let dsurf = |sf: Surface| -> (f64, f64) {
                    (
                        crate::stage4_boundary_curve::surface_distance_pub(sf, p)
                            .unwrap_or(f64::NAN),
                        crate::stage4_relocate::junction_certificate_band(p.as_array(), sf),
                    )
                };
                let (d0, b0) = dsurf(cs0);
                let (d1, b1) = dsurf(cs1);
                let line = format!(
                    "[s451-triple-domain] case={} v={v} rho={rho:.4e} crease={ci}/{} \
                     d_pre={fp:.6e} d_post={fq:.6e} pre_off_own={d0:.6e}/{b0:.3e} \
                     pre_off_other={d1:.6e}/{b1:.3e} pre={:?} post={:?} crease_curve={:?}",
                    std::env::var("ASSAY_CASE").unwrap_or_else(|_| "-".into()),
                    creases.len(),
                    p.as_array(),
                    proj.as_array(),
                    creases[ci].0
                );
                say!("{line}");
                // inc-2c-3b-12b: the REPAIR solve, reported alongside the
                // fire so the census shows whether each out-of-domain site
                // has a DETERMINED transit (and at what correction) before
                // any of it is wired to mutate.
                let nbr_creases = crate::stage4_boundary_curve::creases_for_surfaces(
                    &creases_by_surf,
                    &[creases[ci].2],
                );
                // P0040 (2026-10-12): the solver's own neighbour-domain
                // postcondition cannot see the crease it just crossed —
                // `p_trunc` lies ON it, and an on-crease start is exempt
                // from the sign-change test — so the SIDE the corrected
                // junction lands on is certified against the B-Rep face
                // (`transit_lands_inside_neighbour`). A junction outside
                // the neighbour's own domain is the same defect one face
                // over, and the transit is declined: the vertex keeps its
                // out-of-domain position, the §4.5 postcondition records
                // the fire, and the §4.5.2 ladder answers it.
                let solved = crate::stage4_boundary_curve::solve_crease_transit(
                    p,
                    proj,
                    surfs,
                    &creases[ci],
                    &nbr_creases,
                )
                .and_then(|t| {
                    match crate::stage4_boundary_curve::transit_lands_inside_neighbour(
                        brep_a,
                        brep_b,
                        t.s_nbr,
                        &creases[ci].0,
                        Some(creases[ci].2),
                        t.j,
                    ) {
                        Some((false, d)) => {
                            say!(
                                "[s451-transit-side] case={} v={v} DECLINE j={:?} s_nbr={:?} \
                                 lands OUTSIDE the neighbour face across the crossed \
                                 crease by {d:.6e}",
                                std::env::var("ASSAY_CASE").unwrap_or_else(|_| "-".into()),
                                t.j.as_array(),
                                t.s_nbr
                            );
                            Err(crate::stage4_boundary_curve::CreaseTransitFailure::TransitLeavesNeighbour {
                                d_pre: 0.0,
                                d_post: d,
                            })
                        }
                        Some((true, d)) => {
                            say!(
                                "[s451-transit-side] case={} v={v} inside by {d:.6e}",
                                std::env::var("ASSAY_CASE").unwrap_or_else(|_| "-".into())
                            );
                            Ok(t)
                        }
                        None => {
                            say!(
                                "[s451-transit-side] case={} v={v} UNDETERMINED (no B-Rep \
                                 face on s_nbr carries the crease) — transit kept",
                                std::env::var("ASSAY_CASE").unwrap_or_else(|_| "-".into())
                            );
                            Ok(t)
                        }
                    }
                });
                match solved {
                    Ok(t) => {
                        say!(
                            "[s451-transit-solve] case={} v={v} OK correction={:.6e} \
                             s_nbr={:?} j={:?} p_trunc={:?} q1={:?} q2={:?} q_margin={:?}",
                            std::env::var("ASSAY_CASE").unwrap_or_else(|_| "-".into()),
                            t.correction,
                            t.s_nbr,
                            t.j.as_array(),
                            t.p_trunc.as_array(),
                            t.q1.as_array(),
                            t.q2.as_array(),
                            t.q_margin
                        );
                        // inc-2c-3b-12b-1: the EMISSION-half anatomy. The
                        // solver says where the corner belongs; this says
                        // what the mesh has there today — the fan, its
                        // face attribution, which one-ring neighbours are
                        // already across the crease, and which crease edge
                        // hosts each q-point. Measured BEFORE any of the
                        // re-termination is designed.
                        let case = || std::env::var("ASSAY_CASE").unwrap_or_else(|_| "-".into());
                        match crate::stage4_boundary_curve::transit_site_anatomy(
                            mesh,
                            attribution,
                            v,
                            &creases[ci],
                            fq,
                            [t.q1, t.q2],
                        ) {
                            Some(an) => {
                                say!(
                                    "[s451-anatomy] case={} v={v} fan={} ring={} \
                                     sides(home/on/past)={:?} fan_faces={:?} \
                                     q_hosts={:?}",
                                    std::env::var("ASSAY_CASE").unwrap_or_else(|_| "-".into()),
                                    an.fan.len(),
                                    an.ring.len(),
                                    an.sides,
                                    an.fan_faces,
                                    an.q_hosts
                                );
                                for (u, d, sd) in &an.ring {
                                    say!("[s451-anatomy]   ring v={u} d={d:.6e} {sd:?}");
                                }
                                for ft in &an.fan {
                                    say!(
                                        "[s451-anatomy]   tri={} face={:?} other={:?} \
                                         d_other=[{:.6e},{:.6e}]",
                                        ft.tri,
                                        ft.face,
                                        ft.other,
                                        ft.d_other[0],
                                        ft.d_other[1]
                                    );
                                }
                                // inc-2c-3b-12b-2: the CUT the crease
                                // makes through this fan — which edges
                                // it crosses, which two of those are
                                // the CHAIN edges carrying the
                                // q-points, and which crossings are
                                // refinements of the crease's own mesh
                                // chain. The edit the emission half
                                // would make, before any of it mutates.
                                let face_surface = |i: crate::InputId, f: u32| {
                                    let br = match i {
                                        crate::InputId::A => brep_a,
                                        crate::InputId::B => brep_b,
                                    };
                                    br.faces().get(f as usize).map(|x| x.surface)
                                };
                                match crate::stage4_boundary_curve::transit_cut_path(
                                    mesh,
                                    &an,
                                    proj,
                                    &creases[ci],
                                    &t,
                                    &face_surface,
                                ) {
                                    Ok(cut) => {
                                        say!(
                                            "[s451-cut] case={} v={v} OK carrier={} \
                                         span={:.6e} q_gap={:.6e} \
                                         thetas={:?} \
                                         past_tris={:?} split_tris={:?} nodes={:?}",
                                            std::env::var("ASSAY_CASE")
                                                .unwrap_or_else(|_| "-".into()),
                                            cut.carrier,
                                            cut.span,
                                            {
                                                let (x, y) = (t.q1.as_array(), t.q2.as_array());
                                                ((x[0] - y[0]).powi(2)
                                                    + (x[1] - y[1]).powi(2)
                                                    + (x[2] - y[2]).powi(2))
                                                .sqrt()
                                            },
                                            cut.thetas
                                                .iter()
                                                .map(|x| format!("{x:.5}"))
                                                .collect::<Vec<_>>(),
                                            cut.past_tris,
                                            cut.split_tris,
                                            cut.nodes
                                        );
                                        // inc-2c-3b-12b-3: what the
                                        // mesh must ACQUIRE for the
                                        // corner to be representable
                                        // — the §4.5.2 refinement, at
                                        // the analytically determined
                                        // place rather than as a
                                        // density ladder. Measurement
                                        // only; nothing mutates.
                                        match crate::stage4_boundary_curve::transit_emission_plan(
                                            mesh,
                                            &an,
                                            &cut,
                                            &t,
                                            &creases[ci],
                                        ) {
                                            Ok(pl) => {
                                                say!(
                                                    "[s451-emit] case={} v={v} OK \
                                                     corner_deg={:.17e} \
                                                     fan_span_deg={:.6e} \
                                                     clear={} arc_sag={:.6e} \
                                                     overlap={:?} edges={:?} \
                                                     q_acquire={:?} \
                                                     crease_acquire={:?}",
                                                    std::env::var("ASSAY_CASE")
                                                        .unwrap_or_else(|_| "-".into()),
                                                    pl.corner_deg,
                                                    pl.fan_span_deg,
                                                    pl.corner_clear,
                                                    pl.arc_sag,
                                                    pl.chain_overlap,
                                                    pl.fan_crease_edges,
                                                    pl.q_acquire,
                                                    pl.crease_acquire
                                                );
                                                // inc-2c-3b-12b-4: the
                                                // EDIT LIST — the same
                                                // acquisition expressed
                                                // as vertices to mint
                                                // and triangles to
                                                // re-triangulate, with
                                                // the reach OUTSIDE the
                                                // fan measured. Still
                                                // pure; nothing mutates.
                                                match crate::stage4_boundary_curve::
                                                    transit_emission_edits(
                                                        mesh, &an, &cut, &pl, &t,
                                                    ) {
                                                    Ok(ed) => {
                                                        say!(
                                                            "[s451-edits] case={} \
                                                             v={v} OK site={} \
                                                             host={:?} \
                                                             crease_tris={:?} \
                                                             chain_tris={:?} \
                                                             touched={:?} \
                                                             outside_fan={:?} \
                                                             relabel={:?} \
                                                             inserts={:?}",
                                                            case(),
                                                            ed.site,
                                                            ed.crease_host,
                                                            ed.crease_tris,
                                                            ed.chain_tris,
                                                            ed.touched,
                                                            ed.outside_fan,
                                                            ed.relabel,
                                                            ed.inserts
                                                        );
                                                        // Which face
                                                        // the reach
                                                        // lands in: the
                                                        // notch goes to
                                                        // the patch on
                                                        // the far side
                                                        // of the shared
                                                        // chord, so its
                                                        // identity IS
                                                        // the repair's
                                                        // destination.
                                                        for x in ed
                                                            .outside_fan
                                                            .iter()
                                                            .chain(ed.relabel.iter())
                                                        {
                                                            say!(
                                                                "[s451-edits]   \
                                                                 tri={x} face={:?}",
                                                                attribution.lookup(*x).map(
                                                                    |a| (a.input, a.face)
                                                                )
                                                            );
                                                        }
                                                        // inc-2c-3b-12b-5: the REGION the
                                                        // edits land in, and whether
                                                        // applying them edge by edge —
                                                        // the natural implementation of
                                                        // an edit list stated per-edge —
                                                        // composes. Still pure.
                                                        match crate::stage4_boundary_curve::
                                                            transit_emission_region(
                                                                mesh, &ed,
                                                            ) {
                                                            Ok(rg) => {
                                                                say!(
                                                                "[s451-region] case={} \
                                                                 v={v} OK tris={:?} \
                                                                 boundary={:?} \
                                                                 site_on_boundary={} \
                                                                 mints={:?} \
                                                                 children={} \
                                                                 coincident={:?} \
                                                                 overfull={:?}",
                                                                case(),
                                                                rg.tris,
                                                                rg.boundary,
                                                                rg.site_on_boundary,
                                                                rg.mints,
                                                                rg.naive_children.len(),
                                                                rg.coincident,
                                                                rg.overfull
                                                                );
                                                                // inc-2c-3b-12b-6: the
                                                                // region spans both
                                                                // operands, so the fill's
                                                                // unit is the face-part —
                                                                // if each part is a disk.
                                                                let parts = crate::
                                                                    stage4_boundary_curve::
                                                                    transit_emission_parts(
                                                                        mesh,
                                                                        attribution,
                                                                        &rg,
                                                                        ed.site,
                                                                    );
                                                                for pt in &parts {
                                                                    let pinch = pt
                                                                        .boundary_closed
                                                                        .as_ref()
                                                                        .and_then(|b| {
                                                                        crate::
                                                                        stage4_boundary_curve::
                                                                        transit_boundary_pinch(
                                                                            &ed,
                                                                            rg.mints,
                                                                            b,
                                                                            ed.site,
                                                                        )
                                                                    });
                                                                    say!(
                                                                        "[s451-parts]   \
                                                                         {:?} pinch={:?}",
                                                                        pt, pinch
                                                                    );
                                                                }
                                                                // inc-2c-3b-12b-7: the
                                                                // FILL — the mutation
                                                                // planned end to end and
                                                                // certified against the
                                                                // whole mesh, still not
                                                                // written.
                                                                let face_creases = |f: (crate::InputId, u32)| {
                                                                    face_surface(f.0, f.1)
                                                                        .map(|sf| {
                                                                            crate::stage4_boundary_curve::creases_for_surfaces(
                                                                                &creases_by_surf,
                                                                                &[sf],
                                                                            )
                                                                        })
                                                                        .unwrap_or_default()
                                                                };
                                                                match crate::
                                                                    stage4_boundary_curve::
                                                                    transit_emission_refine(
                                                                        mesh,
                                                                        attribution,
                                                                        &ed,
                                                                        &rg,
                                                                        &parts,
                                                                        &t,
                                                                        &face_surface,
                                                                        &face_creases,
                                                                        16,
                                                                    ) {
                                                                    Ok(rf) => {
                                                                        let fl = &rf.fill;
                                                                        say!(
                                                                            "[s451-refine] case={} v={v} rounds={} \
                                                                             extra={}",
                                                                            case(),
                                                                            rf.rounds.len(),
                                                                            rf.extra.len()
                                                                        );
                                                                        for r in &rf.rounds {
                                                                            say!("[s451-refine]   {r:?}");
                                                                        }
                                                                        say!(
                                                                            "[s451-fill] case={} v={v} OK \
                                                                             removed={:?} touched_delta={:?} \
                                                                             own={:?} notch_face={:?} \
                                                                             notch_surface_agrees={:?} \
                                                                             polygons={} tris={} \
                                                                             edge_defects={:?} opposed={} \
                                                                             folded={} added_folds={} \
                                                                             chord={:?} site_at={:?} \
                                                                             mints={:?}",
                                                                            case(),
                                                                            fl.removed,
                                                                            fl.touched_delta,
                                                                            fl.own_face,
                                                                            fl.notch_face,
                                                                            fl.notch_surface_agrees,
                                                                            fl.polygons.len(),
                                                                            fl.polygons
                                                                                .iter()
                                                                                .map(|p| p.tris.len())
                                                                                .sum::<usize>(),
                                                                            fl.edge_defects,
                                                                            fl.opposed,
                                                                            fl.folded,
                                                                            fl.added_folds,
                                                                            fl.chord,
                                                                            fl.site_at.as_array(),
                                                                            fl.mints
                                                                                .iter()
                                                                                .map(|(m, p)| (*m, p.as_array()))
                                                                                .collect::<Vec<_>>()
                                                                        );
                                                                        say!(
                                                                            "[s451-fill]   lift_flips={} \
                                                                             lift_uncertified={} lift={:?}",
                                                                            fl.lift_flips,
                                                                            fl.lift_uncertified,
                                                                            fl.lift
                                                                        );
                                                                        say!(
                                                                            "[s451-fill]   bite={:?} dropped={:?}",
                                                                            fl.bite, fl.dropped
                                                                        );
                                                                        for p in &fl.polygons {
                                                                            say!(
                                                                                "[s451-fill]   face={:?} bite={} \
                                                                                 polygon={:?} tris={:?} lift={:?}",
                                                                                p.face, p.bite, p.polygon, p.tris, p.lift
                                                                            );
                                                                            let at = |u: u32| -> [f64; 3] {
                                                                                if let Some((_, q)) =
                                                                                    fl.mints.iter().find(|(m, _)| *m == u)
                                                                                {
                                                                                    q.as_array()
                                                                                } else if u == fl.site {
                                                                                    fl.site_at.as_array()
                                                                                } else {
                                                                                    mesh.verts[u as usize].as_array()
                                                                                }
                                                                            };
                                                                            for tri in &p.tris {
                                                                                say!(
                                                                                    "[s451-fill]     tri={:?} pos={:?}",
                                                                                    tri,
                                                                                    [at(tri[0]), at(tri[1]), at(tri[2])]
                                                                                );
                                                                            }
                                                                        }
                                                                        // inc-2c-3b-12b-8: WRITE it. The
                                                                        // write refuses on any unclean
                                                                        // certificate and leaves the mesh
                                                                        // untouched; then the standing STOP
                                                                        // applies exactly as today.
                                                                        match crate::stage4_boundary_curve::
                                                                            transit_emission_write(
                                                                                mesh,
                                                                                attribution,
                                                                                side_labels,
                                                                                fl,
                                                                            ) {
                                                                            Ok(rep) => {
                                                                                say!(
                                                                                    "[s451-apply] case={} v={v} \
                                                                                     WRITTEN {rep:?}",
                                                                                    case()
                                                                                );
                                                                                proj = fl.site_at;
                                                                                repaired = true;
                                                                            }
                                                                            Err(e) => say!(
                                                                                "[s451-apply] case={} v={v} \
                                                                                 REFUSE {e:?} — the standing \
                                                                                 STOP applies",
                                                                                case()
                                                                            ),
                                                                        }
                                                                    }
                                                                    Err(crate::stage4_boundary_curve::EmissionRefineFailure::CapReached {
                                                                        iterations,
                                                                        lift_flips,
                                                                        rounds,
                                                                        last,
                                                                    }) => {
                                                                        say!(
                                                                            "[s451-refine] case={} v={v} CAP iterations={iterations} \
                                                                             lift_flips={lift_flips} halvings={}",
                                                                            case(),
                                                                            rounds.len()
                                                                        );
                                                                        for r in &rounds {
                                                                            say!("[s451-refine]   {r:?}");
                                                                        }
                                                                        say!(
                                                                            "[s451-refine]   last lift={:?}",
                                                                            last.lift
                                                                        );
                                                                        for p in &last.polygons {
                                                                            say!(
                                                                                "[s451-refine]   face={:?} bite={} \
                                                                                 polygon={:?} tris={:?} lift={:?}",
                                                                                p.face, p.bite, p.polygon, p.tris, p.lift
                                                                            );
                                                                        }
                                                                    }
                                                                    Err(e) => say!(
                                                                        "[s451-fill] case={} v={v} DECLINE {e:?}",
                                                                        case()
                                                                    ),
                                                                }
                                                            }
                                                            Err(e) => say!(
                                                                "[s451-region] case={} \
                                                                 v={v} DECLINE {e:?}",
                                                                case()
                                                            ),
                                                        }
                                                    }
                                                    Err(e) => say!(
                                                        "[s451-edits] case={} v={v} \
                                                         DECLINE {e:?}",
                                                        case()
                                                    ),
                                                }
                                            }
                                            Err(e) => say!(
                                                "[s451-emit] case={} v={v} \
                                                 DECLINE {e:?}",
                                                std::env::var("ASSAY_CASE")
                                                    .unwrap_or_else(|_| "-".into())
                                            ),
                                        }
                                    }
                                    Err(e) => say!(
                                        "[s451-cut] case={} v={v} DECLINE {e:?}",
                                        std::env::var("ASSAY_CASE").unwrap_or_else(|_| "-".into())
                                    ),
                                }
                            }
                            None => say!(
                                "[s451-anatomy] case={} v={v} UNAVAILABLE",
                                std::env::var("ASSAY_CASE").unwrap_or_else(|_| "-".into())
                            ),
                        }
                    }
                    Err(e) => say!(
                        "[s451-transit-solve] case={} v={v} DECLINE {e:?}",
                        std::env::var("ASSAY_CASE").unwrap_or_else(|_| "-".into())
                    ),
                }
                // The categorized corpus runner nulls each child's stderr
                // (`assay_kv2.rs` header), so a whole-corpus census needs a
                // sink that survives it. Append-only, one short line per
                // fire — atomic under O_APPEND at this size, so parallel
                // children interleave cleanly.
                if let Ok(path) = std::env::var("YANG_451_DOMAIN_LOG") {
                    use std::io::Write as _;
                    if let Ok(mut f) = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&path)
                    {
                        let _ = writeln!(f, "{line}");
                    }
                }
                if dom_stop && !repaired {
                    return Err(YangError::stage4_region_invalid(
                        v,
                        Stage4InvalidReason::RelocationCrossedCrease,
                    ));
                }
            }
            if std::env::var_os("YANG_RIM_JUNCTION_PROBE").is_some() {
                eprintln!(
                    "[s4-triple-junction] v={v} rho={rho:.4e} gate={gate:.4e} surfs=3 relocated"
                );
            }
            // Bookkeeping (spec I3/I4): out of every single-curve map and
            // out of `endpoints` (all occurrences — one push per incident
            // curve), so the audits and the no-skip balance never see it;
            // NOT added to `processed`/`relocations` (source stays
            // `BRepVertex`, position now exact).
            vert_circle.remove(&v);
            vert_ellipse.remove(&v);
            vert_cone_ellipse.remove(&v);
            vert_parabola.remove(&v);
            vert_cone_hyperbola.remove(&v);
            vert_line.remove(&v);
            // R0044 bucket: also out of the procedural map, so the M5
            // surface-pair loop below neither re-relocates the vertex onto
            // only two of its three surfaces nor STOPs on the endpoint mix.
            // (`vert_surface_pair` verts never enter `endpoints` — a
            // procedural curve has no `t` — so the retain above is a no-op
            // for a pair-only vertex, which by `n_maps < 2` never gets here.)
            vert_surface_pair.remove(&v);
            // C0067: a non-coplanar circle∩circle corner resolved here must
            // not reach the M8 disc∩disc arm (whose closed form would STOP
            // on the non-coplanarity) nor its no-skip audit.
            vert_circle_junction.remove(&v);
            // P0025: a coplanar ellipse∩ellipse corner resolved here must not
            // reach the PR-KV9 junction arm (whose plane-pair line is
            // underivable for it — the very STOP this admission converts).
            vert_ell_junction.remove(&v);
            // P0027: a line∩line corner resolved here leaves the pair map, so
            // the residue audit below does not STOP on it.
            vert_line_junction.remove(&v);
            endpoints.retain(|&u| u != v);
            triple_resolved.insert(v);
            if rho > cad_primitives::TAU_WORK {
                mesh.verts[v as usize] = proj;
                triple_moved.push(v);
            }
        }
    }

    // P0027 residue audit (P10): a line∩line junction the triple block did
    // not resolve (≠ 3 incident surfaces, a diverged Newton, an over-gate
    // move already collected by §4.5.1) is the SAME loud STOP the line arm
    // raised before the admission — at the same site text, so the triage
    // probe keeps localizing it. Never relocate such a vertex onto one of
    // its two lines alone.
    if let Some((&v, second)) = vert_line_junction.iter().next() {
        if std::env::var_os("YANG_LRR_PROBE").is_some() {
            eprintln!(
                "YANG_LRR_SITE site=line_line_junction v={v} p={:?} prev={:?} new={second:?} \
                 unresolved_by_triple_block=true surfs={:?}",
                mesh.verts.get(v as usize),
                vert_line.get(&v),
                vert_surfs.get(&v).map(Vec::len),
            );
        }
        return Err(YangError::stage4_region_invalid(
            v,
            Stage4InvalidReason::LocalRefinementRequired,
        ));
    }

    // M8 disc∩disc no-skip audit (P10): a circle∩circle lens corner that is ALSO
    // on any OTHER curve type (a line, ellipse, cone conic, or line+circle
    // junction) is an over-determined junction this arm does not resolve — loud
    // STOP rather than relocate it onto only the two circles. (Cannot arise for a
    // pure disc∩disc lens, but never silently pick.)
    for v in vert_circle_junction.keys() {
        if vert_line.contains_key(v)
            || vert_ellipse.contains_key(v)
            || vert_cone_ellipse.contains_key(v)
            || vert_parabola.contains_key(v)
            || vert_cone_hyperbola.contains_key(v)
            || vert_junction.contains_key(v)
            || vert_pp_circle_junction.contains_key(v)
        {
            return Err(YangError::stage4_region_invalid(
                *v,
                Stage4InvalidReason::LocalRefinementRequired,
            ));
        }
    }

    // A vertex shared by BOTH a circle and an ellipse edge (two distinct curves
    // through one vertex) is a genuine ambiguity — relocating it twice would be
    // wrong, so loud STOP rather than silently picking one (spec §4 no-skip
    // audit / P10).
    // PR-F3: the line+circle junction is HANDLED (vert_junction above); a line
    // meeting any OTHER conic is still a loud STOP, folded into each audit.
    for v in vert_ellipse.keys() {
        if vert_circle.contains_key(v)
            || vert_line.contains_key(v)
            || vert_junction.contains_key(v)
            || vert_pp_circle_junction.contains_key(v)
        {
            return Err(YangError::stage4_region_invalid(
                *v,
                Stage4InvalidReason::LocalRefinementRequired,
            ));
        }
    }
    // PR-YR21: a vertex shared by a cone-ellipse edge AND any other conic edge
    // (cylinder-ellipse or circle) is a genuine ambiguity — loud STOP (spec
    // §3.2 / P10), the same no-skip audit extended to the cone map.
    for v in vert_cone_ellipse.keys() {
        if vert_circle.contains_key(v)
            || vert_ellipse.contains_key(v)
            || vert_line.contains_key(v)
            || vert_junction.contains_key(v)
            || vert_pp_circle_junction.contains_key(v)
        {
            return Err(YangError::stage4_region_invalid(
                *v,
                Stage4InvalidReason::LocalRefinementRequired,
            ));
        }
    }
    // PR-YR22: a vertex shared by a cone-parabola edge AND any other conic edge
    // (circle / cylinder-ellipse / cone-ellipse) is a genuine ambiguity — loud
    // STOP (P10), the same no-skip audit extended to the parabola map.
    for v in vert_parabola.keys() {
        if vert_circle.contains_key(v)
            || vert_ellipse.contains_key(v)
            || vert_cone_ellipse.contains_key(v)
            || vert_cone_hyperbola.contains_key(v)
            || vert_line.contains_key(v)
            || vert_junction.contains_key(v)
            || vert_pp_circle_junction.contains_key(v)
        {
            return Err(YangError::stage4_region_invalid(
                *v,
                Stage4InvalidReason::LocalRefinementRequired,
            ));
        }
    }
    // PR-YR23: a vertex shared by a cone-hyperbola edge AND any other conic edge
    // (circle / cylinder-ellipse / cone-ellipse / cone-parabola) is a genuine
    // ambiguity — loud STOP (P10), the same no-skip audit extended to the
    // hyperbola map.
    for v in vert_cone_hyperbola.keys() {
        if vert_circle.contains_key(v)
            || vert_ellipse.contains_key(v)
            || vert_cone_ellipse.contains_key(v)
            || vert_parabola.contains_key(v)
            || vert_line.contains_key(v)
            || vert_junction.contains_key(v)
            || vert_pp_circle_junction.contains_key(v)
        {
            return Err(YangError::stage4_region_invalid(
                *v,
                Stage4InvalidReason::LocalRefinementRequired,
            ));
        }
    }

    // (2) Relocate / retag every endpoint. `processed` is the no-skip audit set;
    // `moved` is the subset whose position actually changed (ρ > TAU_WORK) — the
    // triangles touching THOSE verts are the ones Stage-4 validation gates
    // (spec §4.5 step 4: validate per RELOCATED triangle, not pre-existing
    // arrangement slivers that `boolean()` legitimately kept for watertightness).
    // `YANG_V_PROBE=<ids>` selects by Stage-4 vertex id;
    // `YANG_V_PROBE_NEAR=x,y,z,r` additionally selects every vertex within
    // `r` of a position (Stage-4 ids are renumbered by the later
    // compactions, so a probe driven from an OUTPUT-side report — e.g.
    // kernel-v2's `KV_ELLIPSE_PROBE` — has only the position to go on).
    let mut v_probe_ids: Vec<u32> = std::env::var("YANG_V_PROBE")
        .ok()
        .map(|list| {
            list.split(',')
                .filter_map(|t| t.trim().parse::<u32>().ok())
                .collect()
        })
        .unwrap_or_default();
    if let Ok(spec) = std::env::var("YANG_V_PROBE_NEAR") {
        let nums: Vec<f64> = spec
            .split(',')
            .filter_map(|t| t.trim().parse::<f64>().ok())
            .collect();
        if nums.len() == 4 {
            for (vi, q) in mesh.verts.iter().enumerate() {
                let qa = q.as_array();
                let d2 = (qa[0] - nums[0]).powi(2)
                    + (qa[1] - nums[1]).powi(2)
                    + (qa[2] - nums[2]).powi(2);
                if d2 <= nums[3] * nums[3] {
                    v_probe_ids.push(vi as u32);
                }
            }
        }
    }
    if !v_probe_ids.is_empty() {
        for v in v_probe_ids {
            let inc_curves: Vec<String> = curves0
                .iter()
                .filter(|(&(s, e), _)| s == v || e == v)
                .map(|(&(s, e), c)| {
                    let n = match c {
                        Curve::LineSegment => "Line",
                        Curve::Circle { .. } => "Circle",
                        Curve::Ellipse { .. } => "Ellipse",
                        Curve::Parabola { .. } => "Parabola",
                        Curve::Hyperbola { .. } => "Hyperbola",
                        Curve::SurfacePair { .. } => "SurfacePair",
                    };
                    format!("({s},{e}):{n}")
                })
                .collect();
            eprintln!(
                "YANG_V_PROBE v={v} same_type_junction={} exact_junction={} incident_curves=[{}]",
                same_type_junction.contains(&v),
                exact_junctions.contains(&v),
                inc_curves.join(",")
            );
            if let Some(er) = vert_ellipse.get(&v) {
                eprintln!(
                    "YANG_V_PROBE v={v} er plane_n={:?} plane_d={:.17e} center={:?} \
                     normal={:?} major_axis={:?} a={:.17e} b={:.17e} second_cyl={:?}",
                    er.plane_n,
                    er.plane_d,
                    er.center,
                    er.normal,
                    er.major_axis,
                    er.major_radius,
                    er.minor_radius,
                    er.second_cyl,
                );
            }
            // `torus` / `surface_pair` are the two `inc0`-driven implicit-pair
            // relocation paths (the KV6d Tier B torus block and the M5
            // surface-pair block, both AFTER the conic audit). They do NOT
            // populate the conic `vert_*` maps above, so a vertex handled by
            // them shows every conic flag `false`. Print them here so a reader
            // never mistakes "all conic flags false" for "unhandled" — the
            // exact trap that produced the wrong "#137 missing solver" reframe.
            // `torus` mirrors the block's own detection: an `inc0` edge incident
            // to `v` whose attributed surfaces include a `Torus`.
            let torus_v = inc0.iter().any(|(&(s, e), entries)| {
                (s == v || e == v)
                    && entries
                        .iter()
                        .any(|(_i, surf)| matches!(surf, Surface::Torus { .. }))
            });
            eprintln!(
                "YANG_V_PROBE v={v} p={:?} circle={} ellipse={} cone_ell={} parab={} hyp={} \
                 line={} ell_junction={} circle_junction={} line_circle_junction={} \
                 pp_planes={} pp_circle_junction={} endpoint={} torus={torus_v} surface_pair={}",
                mesh.verts.get(v as usize),
                vert_circle.contains_key(&v),
                vert_ellipse.contains_key(&v),
                vert_cone_ellipse.contains_key(&v),
                vert_parabola.contains_key(&v),
                vert_cone_hyperbola.contains_key(&v),
                vert_line.contains_key(&v),
                vert_ell_junction.contains_key(&v),
                vert_circle_junction.contains_key(&v),
                vert_junction.contains_key(&v),
                vert_pp_planes.contains_key(&v),
                vert_pp_circle_junction.contains_key(&v),
                endpoints.contains(&v),
                vert_surface_pair.contains_key(&v),
            );
        }
    }
    let mut processed: HashSet<u32> = HashSet::new();
    let mut moved: HashSet<u32> = HashSet::new();
    // Increment 5: triple-junction relocations count as moved (their
    // incident triangles get the Stage-4 fold validation) but are NOT in
    // `processed`/`relocations` — the no-skip audit balance is untouched
    // because they left `endpoints` too (spec I3).
    moved.extend(triple_moved.iter().copied());
    let mut relocations: Vec<(u32, f64)> = Vec::new();
    // Deterministic order: BTreeMap iteration.
    for (&v, &(center, normal, radius, src_r)) in &vert_circle {
        let p = mesh.verts[v as usize];
        // PR-YR19 (spec §4 Site 2): split the residual so the in-plane RADIAL
        // band is the propagated `(R/r_c)·d_ε` for a sphere section circle while
        // the AXIAL band stays `d_ε`. For `None`/non-sphere this is identical to
        // `max(axial, radial_dev) > d_eps`, i.e. byte-identical to the prior
        // `circle_residual > d_eps`. Near-tangent (`radius ≤ MIN_FEATURE_SIZE`)
        // fails closed (keeps the unscaled band).
        let (axial, radial_dev) = circle_residual_split(p, center, normal, radius);
        let radial_band = match src_r {
            Some(big_r) if radius > cad_primitives::MIN_FEATURE_SIZE => (big_r / radius) * d_eps,
            _ => d_eps,
        };
        if (axial > d_eps || radial_dev > radial_band) && !prov_verts.contains(&v) {
            // (Provenance-vouched vertices are exempt — spec inc-2 §3c: the
            // assignment is producer-confirmed, and `project_onto_circle`
            // below is already the distance-minimizing projection onto the
            // exact curve, a certificate the band cannot strengthen.)
            if let Some(e) = s451_stop(s451_collect, &mut s45_failures, v, &curves0, &inc0) {
                return Err(e);
            }
            continue;
        }
        // Preserve the original combined-max `rho` for the `> TAU_WORK`
        // move-gate so its semantics are unchanged.
        let rho = axial.max(radial_dev);
        // Always project to obtain the circle-frame angle `t` (and the exact
        // on-curve position). For ρ ≤ TAU_WORK the projection is a no-op move
        // but still yields the retag `t`; for the relocate band it moves the
        // vertex onto the curve.
        let (proj, t) = project_onto_circle(p, center, normal, radius)
            .map_err(|reason| YangError::stage4_region_invalid(v, reason))?;
        if rho > cad_primitives::TAU_WORK {
            mesh.verts[v as usize] = proj;
            moved.insert(v);
        }
        probe_push(line!(), v, t, mesh.verts[v as usize]);
        relocations.push((v, t));
        processed.insert(v);
    }

    // M8 disc∩disc CROSSING: relocate each lens-corner vertex onto the EXACT
    // circle∩circle intersection (on BOTH coplanar circles). The vertex sits on
    // a Stage-1 chord, off each circle radially by ≤ d_eps; the displacement to
    // the exact corner is amplified by `1/sin θ`, θ = angle between the two
    // circles' radial directions at the corner (the same derived gradient metric
    // as the cyl×cyl ellipse junction — NOT tolerance widening). A grazing/
    // tangent crossing (θ → 0) has no well-defined corner and `coplanar_circle_
    // circle_intersection` returns `None` → loud STOP.
    for (&v, &(ca, cb)) in &vert_circle_junction {
        let p = mesh.verts[v as usize];
        let (c_a, n_a, r_a, _) = ca;
        let (c_b, n_b, r_b, _) = cb;
        let Some(j) = coplanar_circle_circle_intersection(c_a, n_a, r_a, c_b, n_b, r_b, p) else {
            return Err(YangError::stage4_region_invalid(
                v,
                Stage4InvalidReason::LocalRefinementRequired,
            ));
        };
        let pa = p.as_array();
        let ja = j.as_array();
        let rho =
            ((ja[0] - pa[0]).powi(2) + (ja[1] - pa[1]).powi(2) + (ja[2] - pa[2]).powi(2)).sqrt();
        // sin θ = |r̂_a × r̂_b| at the corner (both radial vectors are in-plane).
        let ra_v = [ja[0] - c_a.x(), ja[1] - c_a.y(), ja[2] - c_a.z()];
        let rb_v = [ja[0] - c_b.x(), ja[1] - c_b.y(), ja[2] - c_b.z()];
        let ra_h = normalize3(ra_v);
        let rb_h = normalize3(rb_v);
        let cr = [
            ra_h[1] * rb_h[2] - ra_h[2] * rb_h[1],
            ra_h[2] * rb_h[0] - ra_h[0] * rb_h[2],
            ra_h[0] * rb_h[1] - ra_h[1] * rb_h[0],
        ];
        let sin_theta = (cr[0] * cr[0] + cr[1] * cr[1] + cr[2] * cr[2]).sqrt();
        let gate = tangent_plane_corridor(d_eps, sin_theta);
        if rho > gate && !prov_verts.contains(&v) {
            // (Provenance-vouched exemption — spec inc-2 §3c; `j` is the
            // exact circle∩circle corner on both curves by construction.)
            if let Some(e) = s451_stop(s451_collect, &mut s45_failures, v, &curves0, &inc0) {
                return Err(e);
            }
            continue;
        }
        // `j` is on circle_a by construction; project to get its frame angle `t`
        // for the source retag (positionally exact on both circles either way).
        let (proj, t) = project_onto_circle(j, c_a, n_a, r_a)
            .map_err(|reason| YangError::stage4_region_invalid(v, reason))?;
        if rho > cad_primitives::TAU_WORK {
            mesh.verts[v as usize] = proj;
            moved.insert(v);
        }
        probe_push(line!(), v, t, mesh.verts[v as usize]);
        relocations.push((v, t));
        processed.insert(v);
    }

    // PR-YR11: ellipse relocation loop, mirroring the circle loop above. Closed
    // form via the cylinder parameterization (spec §2). Same `d_eps` chord band.
    for (&v, er) in &vert_ellipse {
        let p = mesh.verts[v as usize];
        let rho = ellipse_residual(p, er);
        // PR-KV9: cylinder×cylinder sections gate against the per-point
        // gradient band (combined budget × 1/sin α); at tangency grade the
        // metric is unbounded and the Stage-3 surface-membership gate is
        // the backstop. The cylinder×plane path keeps the global d_ε
        // byte-for-byte.
        let gate = match er.second_cyl {
            Some((ap2, ad2, budget)) => {
                cyl_cyl_point_amplification(p, (er.axis_point, er.axis_dir), (ap2, ad2))
                    .map_or(f64::INFINITY, |amp| amp * budget)
            }
            None => d_eps,
        };
        if rho > gate {
            if prov_verts.contains(&v) {
                // Provenance-vouched relocation obligation (spec inc-2 §3c):
                // the producer confirmed this vertex's edge lies on the
                // pair's intersection curve, so the band's wrong-assignment
                // role is covered and the vertex is a DRIFTED intersection
                // vertex — this move IS the §4.4.1 correction the witness
                // selection created. Take the distance-minimizing projection
                // onto the exact ellipse (on cylinder∩plane by construction
                // — a certificate, not a band), bypassing the azimuth path
                // whose tangential amplification is meaningless this far off.
                let (proj, t) = project_onto_ellipse_nearest(p, er)
                    .map_err(|reason| YangError::stage4_region_invalid(v, reason))?;
                if std::env::var("KV11_PROBE").is_ok() {
                    eprintln!(
                        "KV11_PROBE ellipse provenance reloc: v={v} rho={rho:.3e} \
                         gate={gate:.3e} p={p:?} proj={proj:?}"
                    );
                }
                mesh.verts[v as usize] = proj;
                moved.insert(v);
                probe_push(line!(), v, t, mesh.verts[v as usize]);
                relocations.push((v, t));
                processed.insert(v);
                continue;
            }
            if std::env::var("KV11_PROBE").is_ok() {
                eprintln!(
                    "KV11_PROBE ellipse band reject: v={v} rho={rho:.3e} gate={gate:.3e} p={p:?}"
                );
            }
            if let Some(e) = s451_stop(s451_collect, &mut s45_failures, v, &curves0, &inc0) {
                return Err(e);
            }
            continue;
        }
        let (proj, t) = project_onto_ellipse_via_cylinder(p, er)
            .map_err(|reason| YangError::stage4_region_invalid(v, reason))?;
        // Task #145 mechanism 2 (spec `yang_453_mixed_cycle_conic_backtrack`
        // §3b, I6): the azimuth projection amplifies by 1/(n·â) ALONG a
        // near-tangent section — a §4.4.1 relocation is bounded by the same
        // band the ρ gate uses. Move within band → keep the closed form
        // byte-identically (R1); beyond → in-plane nearest point (R2);
        // still beyond → loud STOP (R3), never a silent macro slide.
        let move_len = |q: Point3| -> f64 {
            let qa = q.as_array();
            let pa = p.as_array();
            ((qa[0] - pa[0]).powi(2) + (qa[1] - pa[1]).powi(2) + (qa[2] - pa[2]).powi(2)).sqrt()
        };
        //
        // 2026-09-13 (spec `yang_433_tangent_point_mesh_update.md` §5, C0058):
        // the cyl×cyl arm used to SKIP this check outright
        // (`er.second_cyl.is_some() || …`), on the reading that its
        // per-point-amplified `gate` "already carries the KV9 gradient
        // machinery". It does not: that gate bounds the RESIDUAL ρ, not the
        // MOVE, and `cyl_cyl_point_amplification` is `f64::INFINITY` at
        // tangency grade — so near a surface tangency the azimuth projection
        // could slide a vertex an unbounded distance ALONG the section.
        // Measured on C0058 (two equal-R cylinders tangent at (0, ±0.4, 1)):
        // v33 slid **0.4427** — 22 % of the model's own extent — onto a point
        // where two other vertices already sat, stacking three vertices on one
        // position and folding A's lateral. The move check now applies to the
        // cyl×cyl arm too; v33's move becomes 1.1506e-1 (the in-plane nearest
        // point, which lands 6.78e-2 from the tangency itself).
        // Corpus 2026-09-13: 287C / 0W / 18E / 4EE / 0T (+3 U), ZERO
        // category and ZERO detail moves — this closes a latent, it converts
        // nothing (C0058's own wall is the missing §4.4.1 mesh update, §6).
        //
        // 2026-09-13 (later, spec `yang_433_tangent_point_mesh_update.md` §7):
        // the check above is NOT sufficient on the cyl×cyl arm, and the
        // obvious repair was BUILT, MEASURED and REFUTED — do not re-try it
        // without reading §7. The band compared against is `amp · budget` with
        // `amp = 1/sin α`, and `sin α → 0` IS the tangency: measured on the 30°
        // symmetric Steinmetz pair (`tangency_pinch_split.rs`, r 0.4) the gate
        // reaches 6.986e-1 at v53 and 1.302e0 at v77 — 1.7× and 3.3× the
        // cylinder's own RADIUS — so the azimuth closed form is admitted while
        // it slides v53 by 3.782e-1 (95 % of r) onto the OPPOSITE arm of its
        // section, across the tangent point. Taking
        // `project_onto_ellipse_nearest` unconditionally here cuts that to
        // 9.765e-2 on the correct arm and is corpus-neutral (289C, zero
        // category and zero detail moves) — but it converts NOTHING and turns
        // `c0058_authored_geometry_union_mints_its_tangent_points` RED: pulling
        // these vertices toward the exact node is what the braid of junction
        // proxies cannot absorb (§8). Moving LESS is not the same as colliding
        // less. The owner is the §4.4.1 junction re-triangulation, not the
        // choice of projection.
        if std::env::var("KV11_PROBE").is_ok() {
            eprintln!(
                "KV11_PROBE ellipse reloc: v={v} rho={rho:.6e} gate={gate:.6e} \
                 az_move={:.6e} plane_n=({:.6},{:.6},{:.6}) p={p:?} az={proj:?}",
                move_len(proj),
                er.plane_n.x(),
                er.plane_n.y(),
                er.plane_n.z()
            );
        }
        let (proj, t) = if move_len(proj) <= gate {
            // R1: byte-identical closed-form azimuth projection.
            (proj, t)
        } else {
            let (near_proj, near_t) = project_onto_ellipse_nearest(p, er)
                .map_err(|reason| YangError::stage4_region_invalid(v, reason))?;
            // R2/R3 budget: the vertex's surface residuals are ≤ `gate` each,
            // and distance-to-curve amplifies by 1/sin θ (θ = angle between
            // the two surface normals AT the relocated point) — the same
            // derived gradient-band the circle-junction and pp-plane gates
            // use (never widening). Evaluated at `near_proj`, where the
            // transversality of the accepted position is what matters.
            let budget = {
                let np = near_proj.as_array();
                let q = er.axis_point.as_array();
                let a_hat = normalize3(er.axis_dir.as_array());
                let w = [np[0] - q[0], np[1] - q[1], np[2] - q[2]];
                let along = w[0] * a_hat[0] + w[1] * a_hat[1] + w[2] * a_hat[2];
                let radial = normalize3([
                    w[0] - along * a_hat[0],
                    w[1] - along * a_hat[1],
                    w[2] - along * a_hat[2],
                ]);
                let n_pl = normalize3(er.plane_n.as_array());
                let cr = [
                    radial[1] * n_pl[2] - radial[2] * n_pl[1],
                    radial[2] * n_pl[0] - radial[0] * n_pl[2],
                    radial[0] * n_pl[1] - radial[1] * n_pl[0],
                ];
                let sin_theta = (cr[0] * cr[0] + cr[1] * cr[1] + cr[2] * cr[2]).sqrt();
                // Exact tangency → unbounded corridor (see
                // `tangent_plane_corridor`); the projection is still the
                // local nearest point.
                tangent_plane_corridor(gate, sin_theta)
            };
            if std::env::var_os("YANG_T145_RELOC_PROBE").is_some() {
                eprintln!(
                    "[t145-reloc] v={v} rho={rho:.3e} gate={gate:.3e} budget={budget:.3e} \
                     az_move={:.3e} near_move={:.3e} p={p:?} az={proj:?} near={near_proj:?}",
                    move_len(proj),
                    move_len(near_proj),
                );
            }
            if move_len(near_proj) > budget {
                if let Some(e) = s451_stop(s451_collect, &mut s45_failures, v, &curves0, &inc0) {
                    return Err(e);
                }
                continue;
            }
            (near_proj, near_t)
        };
        if rho > cad_primitives::TAU_WORK {
            mesh.verts[v as usize] = proj;
            moved.insert(v);
        }
        probe_push(line!(), v, t, mesh.verts[v as usize]);
        relocations.push((v, t));
        processed.insert(v);
    }

    // PR-KV9: ellipse×ellipse JUNCTION relocation. The exact junction lies
    // on `(plane₁ ∩ plane₂) ∩ cylinder` (the crossing point of the two
    // Steinmetz sections — on the cylinder and in BOTH cutting planes,
    // hence on both ellipses). The plane–plane line is exact; intersecting
    // it with the relocation cylinder is a quadratic with ≤ 2 roots; the
    // root nearest the current vertex is the junction (the two crossing
    // points are 2r apart — far outside any chord band, so nearest-pick is
    // deterministic and unambiguous). Gate at 2·d_ε (each constituent
    // membership is within its own propagated band; the junction inherits
    // both, mirroring the line+circle junction's derivation).
    for (&v, &(e_a, e_b)) in &vert_ell_junction {
        let p = mesh.verts[v as usize];
        let n1 = normalize3(e_a.plane_n.as_array());
        let n2 = normalize3(e_b.plane_n.as_array());
        let dir = [
            n1[1] * n2[2] - n1[2] * n2[1],
            n1[2] * n2[0] - n1[0] * n2[2],
            n1[0] * n2[1] - n1[1] * n2[0],
        ];
        let dl = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
        if dl < cad_primitives::MIN_FEATURE_SIZE {
            return Err(YangError::stage4_region_invalid(
                v,
                Stage4InvalidReason::LocalRefinementRequired,
            ));
        }
        let d = [dir[0] / dl, dir[1] / dl, dir[2] / dl];
        // A point on both planes: solve n1·x = −d1, n2·x = −d2 in the span
        // of {n1, n2} (x = α·n1 + β·n2; Gram system with g = n1·n2).
        let g = n1[0] * n2[0] + n1[1] * n2[1] + n1[2] * n2[2];
        let det = 1.0 - g * g;
        if det.abs() < cad_primitives::MIN_FEATURE_SIZE {
            return Err(YangError::stage4_region_invalid(
                v,
                Stage4InvalidReason::LocalRefinementRequired,
            ));
        }
        let (r1, r2) = (-e_a.plane_d, -e_b.plane_d);
        let alpha = (r1 - g * r2) / det;
        let beta = (r2 - g * r1) / det;
        let p0 = [
            alpha * n1[0] + beta * n2[0],
            alpha * n1[1] + beta * n2[1],
            alpha * n1[2] + beta * n2[2],
        ];
        // Intersect the line p0 + t·d with the relocation cylinder of e_a.
        let ax = normalize3(e_a.axis_dir.as_array());
        let ap = e_a.axis_point.as_array();
        let rel = [p0[0] - ap[0], p0[1] - ap[1], p0[2] - ap[2]];
        let perp = |w: [f64; 3]| -> [f64; 3] {
            let h = w[0] * ax[0] + w[1] * ax[1] + w[2] * ax[2];
            [w[0] - h * ax[0], w[1] - h * ax[1], w[2] - h * ax[2]]
        };
        let rp = perp(rel);
        let dp = perp(d);
        let aa = dp[0] * dp[0] + dp[1] * dp[1] + dp[2] * dp[2];
        let bb = 2.0 * (rp[0] * dp[0] + rp[1] * dp[1] + rp[2] * dp[2]);
        let cc = rp[0] * rp[0] + rp[1] * rp[1] + rp[2] * rp[2] - e_a.radius * e_a.radius;
        let disc = bb * bb - 4.0 * aa * cc;
        if !(aa > cad_primitives::MIN_FEATURE_SIZE && disc >= 0.0) {
            return Err(YangError::stage4_region_invalid(
                v,
                Stage4InvalidReason::LocalRefinementRequired,
            ));
        }
        let sq = disc.sqrt();
        let pa = p.as_array();
        let mut best: Option<([f64; 3], f64)> = None;
        for t in [(-bb - sq) / (2.0 * aa), (-bb + sq) / (2.0 * aa)] {
            let x = [p0[0] + t * d[0], p0[1] + t * d[1], p0[2] + t * d[2]];
            let dd =
                ((x[0] - pa[0]).powi(2) + (x[1] - pa[1]).powi(2) + (x[2] - pa[2]).powi(2)).sqrt();
            if best.map(|(_, b)| dd < b).unwrap_or(true) {
                best = Some((x, dd));
            }
        }
        let (j, rho) = best.expect("two real roots checked");
        // PR-KV11: the vertex moves ALONG the junction line to reach the
        // cylinder, so its radial chord residual (≤ the combined band) is
        // amplified by `1/|d̂·r̂|` — the directional derivative of the
        // radial distance along the line at the junction (the same derived
        // metric propagation as the KV9 cyl×cyl `1/sin α` gradient band; a
        // grazing line ⇒ unbounded metric, backstopped by the Stage-3
        // surface-membership gates, mirroring the cyl×cyl arm).
        let rel_j = [j[0] - ap[0], j[1] - ap[1], j[2] - ap[2]];
        let rp_j = perp(rel_j);
        let rp_j_len = (rp_j[0] * rp_j[0] + rp_j[1] * rp_j[1] + rp_j[2] * rp_j[2]).sqrt();
        let grad = if rp_j_len > 0.0 {
            ((d[0] * rp_j[0] + d[1] * rp_j[1] + d[2] * rp_j[2]) / rp_j_len).abs()
        } else {
            0.0
        };
        // KV9-F1 E-L2 (spec §2c, branch row J1): a junction of two sections of
        // the SAME unordered cylinder pair is ALWAYS the pair's surface-tangency
        // point (the two decomposition planes intersect in the line through both
        // tangency points; that line meets the cylinder exactly where the two
        // radial gradients align). There the vertex is the PINCH of the two
        // faceted-surface intersection polylines, whose standoff from the exact
        // crossing is SECOND-order-controlled: in tangent-plane coordinates the
        // cylinders are the graphs y = r − x²/2r and y = r − z²/2r; facet
        // displacements a ∈ [0, ε_A], b ∈ [0, ε_B] perturb the intersection to
        // the hyperbola x² − z² = 2r(b−a), standoff √(2r·|b−a|) ≤ √(2r·B) with
        // B the combined chord budget carried by `second_cyl`, plus ≤ B
        // normal-direction offset. A derived metric conversion (the
        // single-ellipse arm's 1/sin α analog at tangency grade), NOT tolerance
        // widening — the relocation target stays the EXACT junction. Every
        // other junction (row J2 — the KV11 box-edge class) keeps the
        // first-order 2·d_ε/|d̂·r̂| line metric byte-identical.
        let same_pair_budget = match (e_a.second_cyl, e_b.second_cyl) {
            (Some((sa_p, sa_d, ba)), Some((sb_p, sb_d, bb))) => {
                let same = e_a.axis_point.as_array() == e_b.axis_point.as_array()
                    && e_a.axis_dir.as_array() == e_b.axis_dir.as_array()
                    && sa_p.as_array() == sb_p.as_array()
                    && sa_d.as_array() == sb_d.as_array();
                if same {
                    Some(ba.max(bb))
                } else {
                    None
                }
            }
            _ => None,
        };
        let gate = if let Some(budget) = same_pair_budget {
            (2.0 * e_a.radius * budget).sqrt() + budget
        } else {
            tangent_plane_corridor(d_eps, grad)
        };
        // KV9-F1 Increment 0c census: per-junction second_cyl provenance +
        // first-order gate state (kept env-gated, like the other Stage-4 probes).
        if std::env::var("KV9_JUNCTION_PROBE").is_ok() {
            eprintln!(
                "KV9_JUNCTION_PROBE v={v} p={p:?} j={j:?} rho={rho:.4e} grad={grad:.4e} \
                 gate={gate:.4e} d_eps={d_eps:.4e} \
                 a_axis=({:?},{:?}) a_second={:?} b_axis=({:?},{:?}) b_second={:?}",
                e_a.axis_point.as_array(),
                e_a.axis_dir.as_array(),
                e_a.second_cyl
                    .map(|(sp, sd, bud)| (sp.as_array(), sd.as_array(), bud)),
                e_b.axis_point.as_array(),
                e_b.axis_dir.as_array(),
                e_b.second_cyl
                    .map(|(sp, sd, bud)| (sp.as_array(), sd.as_array(), bud)),
            );
        }
        if rho > gate && !prov_verts.contains(&v) {
            // (Provenance-vouched vertices are exempt from this displacement
            // gate — spec inc-2 §3c: their assignment is producer-confirmed
            // and the destination `j` is the EXACT nearest-root junction on
            // all defining surfaces, a certificate the displacement
            // magnitude cannot strengthen.)
            if std::env::var("KV11_PROBE").is_ok() {
                eprintln!(
                    "KV11_PROBE junction band reject: v={v} rho={rho:.3e} gate={gate:.3e} p={p:?} j={j:?}"
                );
            }
            if let Some(e) = s451_stop(s451_collect, &mut s45_failures, v, &curves0, &inc0) {
                return Err(e);
            }
            continue;
        }
        let proj = Point3::new(j[0], j[1], j[2]);
        // Param on e_a's ellipse for the source retag (output edges of BOTH
        // ellipses touch this vertex; the position is exact on both, so the
        // retag curve choice is positional-exact either way).
        let t = ellipse_param(
            proj,
            e_a.center,
            e_a.normal,
            e_a.major_axis,
            e_a.major_radius,
            e_a.minor_radius,
        );
        if rho > cad_primitives::TAU_WORK {
            mesh.verts[v as usize] = proj;
            moved.insert(v);
        }
        probe_push(line!(), v, t, mesh.verts[v as usize]);
        relocations.push((v, t));
        processed.insert(v);
    }

    // LRR diagnostic (read-only): unified over-band run-structure across ALL
    // conic relocation maps. Per curve identity, sort by parameter `t`, flag
    // over-band vertices, and classify each over-band vertex as INTERIOR-bounded
    // (a within-band vertex exists at both a smaller AND larger t on the same
    // curve — the paper §4.5.1 condition) vs END/UNBOUNDED. Emits a per-case
    // verdict on whether EVERY over-band conic vertex is interior-bounded.
    if std::env::var_os("YANG_LRR_PROBE").is_some() {
        // Per (curve-kind, curve-identity) → sorted samples `(t, v, rho, band)`.
        type LrrGroups = BTreeMap<(&'static str, [u64; 3]), Vec<(f64, u32, f64, f64)>>;
        let mut groups: LrrGroups = BTreeMap::new();
        let kb = |p: Point3| [p.x().to_bits(), p.y().to_bits(), p.z().to_bits()];
        let mut push = |kind: &'static str, key: [u64; 3], t: f64, v: u32, rho: f64, band: f64| {
            groups
                .entry((kind, key))
                .or_default()
                .push((t, v, rho, band));
        };
        for (&v, &(center, normal, radius, src_r)) in &vert_circle {
            let p = mesh.verts[v as usize];
            let (axial, radial_dev) = circle_residual_split(p, center, normal, radius);
            let band = match src_r {
                Some(big_r) if radius > cad_primitives::MIN_FEATURE_SIZE => {
                    (big_r / radius) * d_eps
                }
                _ => d_eps,
            };
            let t = project_onto_circle(p, center, normal, radius)
                .map(|(_, t)| t)
                .unwrap_or(0.0);
            push(
                "circle",
                kb(center),
                t,
                v,
                axial.max(radial_dev),
                band.max(d_eps),
            );
        }
        for (&v, er) in &vert_ellipse {
            let p = mesh.verts[v as usize];
            push(
                "ellipse",
                kb(er.center),
                0.0,
                v,
                ellipse_residual(p, er),
                d_eps,
            );
        }
        for (&v, cer) in &vert_cone_ellipse {
            let p = mesh.verts[v as usize];
            let t = ellipse_param(
                p,
                cer.center,
                cer.normal,
                cer.major_axis,
                cer.major_radius,
                cer.minor_radius,
            );
            push(
                "cone_ell",
                kb(cer.center),
                t,
                v,
                cone_ellipse_residual(p, cer),
                cer.cone_d_eps,
            );
        }
        for (&v, cpr) in &vert_parabola {
            let p = mesh.verts[v as usize];
            let rho = cone_plane_residual(
                p,
                cpr.apex,
                cpr.cone_axis_dir,
                cpr.half_angle,
                cpr.plane_n,
                cpr.plane_d,
            );
            push("parabola", kb(cpr.vertex), 0.0, v, rho, cpr.cone_d_eps);
        }
        for (&v, chr) in &vert_cone_hyperbola {
            let p = mesh.verts[v as usize];
            let rho = cone_plane_residual(
                p,
                chr.apex,
                chr.cone_axis_dir,
                chr.half_angle,
                chr.plane_n,
                chr.plane_d,
            );
            push("hyperbola", kb(chr.apex), 0.0, v, rho, chr.cone_d_eps);
        }
        let mut all_interior = true;
        let mut n_over = 0usize;
        for ((kind, _key), list) in &mut groups {
            list.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
            let within: Vec<f64> = list.iter().filter(|r| r.2 <= r.3).map(|r| r.0).collect();
            let (tmin, tmax) = (
                within.iter().cloned().fold(f64::INFINITY, f64::min),
                within.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
            );
            let mut seq = String::new();
            for (t, _v, rho, band) in list.iter() {
                if rho > band {
                    n_over += 1;
                    let interior = *t > tmin && *t < tmax;
                    if !interior {
                        all_interior = false;
                    }
                    seq.push(if interior { 'I' } else { 'E' });
                } else {
                    seq.push('.');
                }
            }
            if seq.contains('I') || seq.contains('E') {
                eprintln!(
                    "YANG_LRR_PROBE {kind} n={} within={} seq={seq}",
                    list.len(),
                    within.len()
                );
            }
        }
        eprintln!(
            "YANG_LRR_VERDICT n_over={n_over} all_over_band_interior_bounded={}",
            n_over > 0 && all_interior
        );
    }

    // §4.5.1 (ledger N90): a cone-section step that leaves its plane face is
    // a failure to converge within the domain (`:652-654`) — RECORDED with
    // the other collected failures, and repaired after the sweep from the
    // midpoint of its converged bounds (`s451_plan_repairs`). The crease
    // index is built once, only when a cone section is relocated.
    let cone_sections = !(vert_cone_ellipse.is_empty()
        && vert_parabola.is_empty()
        && vert_cone_hyperbola.is_empty());
    let s451_patches = if cone_sections {
        domain_census::build_patch_map(mesh, &attribution.attributions)
    } else {
        BTreeMap::new()
    };
    let s451_creases = cone_sections.then(|| {
        [
            crate::stage4_boundary_curve::CreaseIndex::build(brep_a),
            crate::stage4_boundary_curve::CreaseIndex::build(brep_b),
        ]
    });
    // The creases of every face at `v`.
    let creases_at = |v: u32| -> Vec<&[crate::stage4_boundary_curve::FaceCrease]> {
        let Some(idx) = &s451_creases else {
            return Vec::new();
        };
        s451_patches
            .get(&v)
            .into_iter()
            .flatten()
            .filter_map(|&(input, face)| {
                let slot = match input {
                    InputId::A => 0,
                    InputId::B => 1,
                };
                idx[slot].faces.get(face as usize).map(Vec::as_slice)
            })
            .collect()
    };
    let leaves_face = |v: u32, p: Point3, proj: Point3, sec: &ConeSection| -> bool {
        creases_at(v)
            .into_iter()
            .any(|cr| cone_section_step_leaves_face(p, proj, sec.plane(), cr))
    };

    // PR-YR21 / YR22 / YR23: the cone-ellipse, cone-parabola and
    // cone-hyperbola relocation loops. §4.3.2's step on (cone, plane)
    // (`project_onto_cone_section`, ledger N97) — the section TYPE does not
    // change the relocation, only the parameter tag. Gated against the cone's
    // OWN chord budget `cone_d_eps` (NOT the rim-AABB `d_eps`) so a
    // tall-thin cone's residual is checked against the honest cone bound.
    let sections: Vec<(u32, ConeSection)> = vert_cone_ellipse
        .iter()
        .map(|(&v, &c)| (v, ConeSection::Ellipse(c)))
        .chain(
            vert_parabola
                .iter()
                .map(|(&v, &c)| (v, ConeSection::Parabola(c))),
        )
        .chain(
            vert_cone_hyperbola
                .iter()
                .map(|(&v, &c)| (v, ConeSection::Hyperbola(c))),
        )
        .collect();
    for (v, sec) in sections {
        let p = mesh.verts[v as usize];
        let rho = sec.residual(p);
        let cone_d_eps = match sec {
            ConeSection::Ellipse(c) => c.cone_d_eps,
            ConeSection::Parabola(c) => c.cone_d_eps,
            ConeSection::Hyperbola(c) => c.cone_d_eps,
        };
        if rho > cone_d_eps {
            if let Some(e) = s451_stop(s451_collect, &mut s45_failures, v, &curves0, &inc0) {
                return Err(e);
            }
            continue;
        }
        let proj = sec
            .step(p)
            .map_err(|reason| YangError::stage4_region_invalid(v, reason))?;
        if leaves_face(v, p, proj, &sec) {
            // Cannot converge within its domain (`:652-654`): a recorded
            // failure, repaired with the others after the sweep.
            s45_failures.push((
                v,
                YangError::stage4_region_invalid(v, Stage4InvalidReason::RelocationCrossedCrease),
            ));
            continue;
        }
        if rho > cad_primitives::TAU_WORK {
            mesh.verts[v as usize] = proj;
            moved.insert(v);
        }
        let t = sec.param(proj);
        probe_push(line!(), v, t, mesh.verts[v as usize]);
        relocations.push((v, t));
        processed.insert(v);
    }

    // PR-F3: ruling-line relocation loop. The residual is the perpendicular
    // distance to the exact line (the sagitta of the Stage-1 facet chord the
    // arrangement point sits on), gated at the same global `d_eps` band as the
    // circle loop. The relocated position is the foot of the perpendicular —
    // exactly on the line, hence exactly on BOTH the cutting plane and the
    // cylinder. `t` is the along-line parameter; no conic OUTPUT edge claims a
    // line vertex in `emit_topology`, so its source stays `BRepVertex` and
    // `eval_source` returns the relocated mesh position directly.
    for (&v, lr) in &vert_line {
        let p = mesh.verts[v as usize];
        let rho = line_perp_distance(p, lr.point, lr.dir);
        // PR-F3b/PR-KV9: the residual is the line-distance metric, so the
        // gate is the ABSOLUTE propagated budget computed at collection (the
        // owner chord band(s) converted into this metric) — not the raw
        // radial band, and not the global d_ε (whose owner mix is wrong for
        // cylinder×cylinder lines).
        if rho > lr.band_budget {
            if let Some(e) = s451_stop(s451_collect, &mut s45_failures, v, &curves0, &inc0) {
                return Err(e);
            }
            continue;
        }
        let d = normalize3(lr.dir.as_array());
        let pt = lr.point.as_array();
        let x = p.as_array();
        let w = [x[0] - pt[0], x[1] - pt[1], x[2] - pt[2]];
        let along = w[0] * d[0] + w[1] * d[1] + w[2] * d[2];
        let proj = Point3::new(
            pt[0] + along * d[0],
            pt[1] + along * d[1],
            pt[2] + along * d[2],
        );
        if rho > cad_primitives::TAU_WORK {
            mesh.verts[v as usize] = proj;
            moved.insert(v);
        }
        probe_push(line!(), v, along, mesh.verts[v as usize]);
        relocations.push((v, along));
        processed.insert(v);
    }

    // PR-F3: line+circle JUNCTION relocation loop. The exact junction is
    // `line ∩ plane-of-circle` (which lies exactly on the circle, since the
    // line is on the cylinder and the circle is cylinder ∩ circle-plane). The
    // residual gate is `2·d_eps`: the vertex is off the line radially by ≤ one
    // sagitta AND off the circle plane along the line by ≤ another
    // sagitta-order term (it sits on the crossing of the cutting plane with a
    // rim-chord facet edge), so the combined displacement to the junction is
    // bounded by 2·d_eps — a derived bound, not tolerance widening. The final
    // position is `project_onto_circle(j)` so the vertex's `BRepEdge { edge, t }`
    // source round-trips bitwise through the unchanged `eval_source` Circle arm.
    for (&v, &(lr, (center, normal, radius, _src_r))) in &vert_junction {
        let p = mesh.verts[v as usize];
        let n = normalize3(normal.as_array());
        let d = normalize3(lr.dir.as_array());
        let denom = n[0] * d[0] + n[1] * d[1] + n[2] * d[2];
        let pt = lr.point.as_array();
        let c = center.as_array();
        let (j, gate) = if denom.abs() < cad_primitives::TAU_MODEL {
            // M5 K11 inc-2 (2026-09-05, spec `m5_surface_pair_curve.md` "K11
            // inc-2"): the line is PARALLEL to the circle's plane. Two
            // configurations share this branch and only one is a junction:
            //
            //   (a) the line lies IN the circle's plane — the `pair curve ∩
            //       plane` junction of an INPUT surface-pair chain crossed by
            //       a cutting plane parallel to one cylinder's axis (section
            //       = a ruling) and perpendicular to the other's (section = a
            //       circle): both section curves lie in the cutting plane and
            //       their in-plane crossing IS the three-surface point the
            //       pair chain passes through. Exact closed form + derived
            //       gate: `ruling_circle_coplanar_junction`. Before this
            //       branch the arm STOPped here as "no transversal junction"
            //       (the kernel-v2 pin `unequal_perpendicular_union_reenters_
            //       with_crossing_cut`, v28, measured 2026-09-04/05);
            //   (b) the line is parallel but OFFSET from the plane, or
            //       tangent to the circle — no transversal junction; the
            //       loud stop stands (local refinement is the honest answer).
            match ruling_circle_coplanar_junction(
                (lr.point, lr.dir),
                (center, normal, radius),
                p,
                lr.band_budget,
                d_eps,
            ) {
                Ok((j, gate)) => {
                    if std::env::var_os("YANG_LRR_PROBE").is_some() {
                        eprintln!(
                            "[k11-inc2-junction] v={v} p={:?} j={:?} gate={gate:.6e} \
                             band={:.3e} d_eps={d_eps:.3e}",
                            p.as_array(),
                            j.as_array(),
                            lr.band_budget
                        );
                    }
                    (j, gate)
                }
                Err(decline) => {
                    if std::env::var_os("YANG_LRR_PROBE").is_some() {
                        eprintln!(
                            "YANG_LRR_SITE site=line_circle_coplanar_decline v={v} p={:?} \
                             line=({pt:?}, {d:?}) circle=(c={c:?} n={n:?} r={radius}) \
                             decline={decline:?}",
                            p.as_array()
                        );
                    }
                    return Err(YangError::stage4_region_invalid(
                        v,
                        Stage4InvalidReason::LocalRefinementRequired,
                    ));
                }
            }
        } else {
            // Transversal: the exact junction is `line ∩ plane-of-circle`.
            let s_par =
                (n[0] * (c[0] - pt[0]) + n[1] * (c[1] - pt[1]) + n[2] * (c[2] - pt[2])) / denom;
            let j = Point3::new(
                pt[0] + s_par * d[0],
                pt[1] + s_par * d[1],
                pt[2] + s_par * d[2],
            );
            // PR-F3b: line-band component carries the propagated budget; the
            // along-line crossing component stays at the raw d_ε.
            (j, lr.band_budget + d_eps)
        };
        let pj = [
            p.as_array()[0] - j.as_array()[0],
            p.as_array()[1] - j.as_array()[1],
            p.as_array()[2] - j.as_array()[2],
        ];
        let rho = (pj[0] * pj[0] + pj[1] * pj[1] + pj[2] * pj[2]).sqrt();
        if rho > gate {
            if let Some(e) = s451_stop(s451_collect, &mut s45_failures, v, &curves0, &inc0) {
                return Err(e);
            }
            continue;
        }
        let (proj, t) = project_onto_circle(j, center, normal, radius)
            .map_err(|reason| YangError::stage4_region_invalid(v, reason))?;
        if rho > cad_primitives::TAU_WORK {
            mesh.verts[v as usize] = proj;
            moved.insert(v);
        }
        probe_push(line!(), v, t, mesh.verts[v as usize]);
        relocations.push((v, t));
        processed.insert(v);
    }

    // Task #146 (spec `yang_stage4_circle_pp_line_junction` branches 4–6):
    // relocate each circle×pp-line junction vertex onto the exact
    // line∩circle point (line∩sphere quadratic + circle-plane residual
    // certificate — valid for the in-plane AND transversal configurations).
    for (&v, &((lp, ld), (center, normal, radius, _src_r))) in &vert_pp_circle_junction {
        let p = mesh.verts[v as usize];
        let Some(j) = pp_line_circle_junction(lp, ld, center, normal, radius, p, d_eps) else {
            // Branch 5: the line misses the circle (or no root is on the
            // circle's plane) — not a resolvable junction here.
            return Err(YangError::stage4_region_invalid(
                v,
                Stage4InvalidReason::LocalRefinementRequired,
            ));
        };
        let pa = p.as_array();
        let ja = j.as_array();
        let rho =
            ((ja[0] - pa[0]).powi(2) + (ja[1] - pa[1]).powi(2) + (ja[2] - pa[2]).powi(2)).sqrt();
        // Branch 6: crossing amplification — the vertex sits within its
        // chord bands of BOTH curves; the displacement to their junction is
        // amplified by 1/sin θ, θ = angle between the line direction and the
        // circle tangent at the junction (the vert_circle_junction pattern;
        // derived, not widening).
        let n = normalize3(normal.as_array());
        let dh = normalize3(ld.as_array());
        let c = center.as_array();
        let rvec = normalize3([ja[0] - c[0], ja[1] - c[1], ja[2] - c[2]]);
        let tangent = [
            n[1] * rvec[2] - n[2] * rvec[1],
            n[2] * rvec[0] - n[0] * rvec[2],
            n[0] * rvec[1] - n[1] * rvec[0],
        ];
        let cross = [
            dh[1] * tangent[2] - dh[2] * tangent[1],
            dh[2] * tangent[0] - dh[0] * tangent[2],
            dh[0] * tangent[1] - dh[1] * tangent[0],
        ];
        let sin_theta = (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt();
        let gate = tangent_plane_corridor(d_eps, sin_theta);
        if rho > gate {
            if let Some(e) = s451_stop(s451_collect, &mut s45_failures, v, &curves0, &inc0) {
                return Err(e);
            }
            continue;
        }
        // Branch 4: `j` is exactly on the line and on the circle's sphere;
        // the circle projection yields the frame angle `t` for the retag
        // (positionally a no-op up to f64 — `j` is on the circle).
        let (proj, t) = project_onto_circle(j, center, normal, radius)
            .map_err(|reason| YangError::stage4_region_invalid(v, reason))?;
        if i1d_probe {
            let ca = center.as_array();
            let lpa = lp.as_array();
            let lda = ld.as_array();
            eprintln!(
                "[i1d-junction] v{v} rho={rho:.6e} sin_theta={sin_theta:.6e} gate={gate:.6e} \
                 circle c=({:.9},{:.9},{:.9}) r={radius:.9} line p=({:.9},{:.9},{:.9}) \
                 d=({:.6},{:.6},{:.6})",
                ca[0], ca[1], ca[2], lpa[0], lpa[1], lpa[2], lda[0], lda[1], lda[2],
            );
        }
        if rho > cad_primitives::TAU_WORK {
            mesh.verts[v as usize] = proj;
            moved.insert(v);
        }
        probe_push(line!(), v, t, mesh.verts[v as usize]);
        relocations.push((v, t));
        processed.insert(v);
    }

    // No-skip audit (anti-disproven-attempt): every conic endpoint was handled.
    let relocation_keys: HashSet<u32> = relocations.iter().map(|&(v, _)| v).collect();
    let endpoint_set: HashSet<u32> = endpoints.iter().copied().collect();
    if std::env::var_os("YANG_LRR_PROBE").is_some() && processed != endpoint_set {
        for &v in endpoint_set.difference(&processed) {
            let mut curs: Vec<String> = Vec::new();
            for (&(s, e), curve) in &curves0 {
                if s == v || e == v {
                    curs.push(format!("({s},{e})={curve:?}"));
                }
            }
            eprintln!(
                "YANG_LRR_UNCLAIMED endpoint v={v} on curves: {}",
                curs.join(" | ")
            );
        }
        for &v in processed.difference(&endpoint_set) {
            eprintln!("YANG_LRR_EXTRA processed-but-not-endpoint v={v}");
        }
    }
    // §4.5.1 inc-1 census: a RECORDED OffCurve failure is a HANDLED endpoint —
    // the paper's collected "cannot converge" state — not a silent skip. The
    // audit's job is catching UNRECORDED skips, so recorded vertices are
    // subtracted from the expectation. Gate off: the set is empty and the
    // audit is byte-identical.
    let s45_failed_set: HashSet<u32> = s45_failures.iter().map(|&(v, _)| v).collect();
    let endpoint_expectation: HashSet<u32> =
        endpoint_set.difference(&s45_failed_set).copied().collect();
    if processed != endpoint_expectation || processed != relocation_keys {
        if std::env::var_os("YANG_LRR_PROBE").is_some() {
            eprintln!(
                "YANG_LRR_STOP site=no_skip_audit ep_ne_proc={} proc_ne_reloc={}",
                processed != endpoint_expectation,
                processed != relocation_keys
            );
        }
        return Err(YangError::stage4_region_invalid(
            u32::MAX,
            Stage4InvalidReason::LocalRefinementRequired,
        ));
    }

    // M5 (Y4): degree-4 surface-pair relocation via Newton on the two defining
    // surfaces — a sibling of the TORUS block below (both are implicit-pair,
    // not conic, so they are relocated AFTER the conic audit and are NOT part
    // of the conic `endpoints`/`relocations` bookkeeping). Each endpoint keeps
    // its `BRepVertex` source (a procedural curve has no `t`). A surface-pair
    // endpoint that is ALSO a conic endpoint mixes closed-form and
    // implicit-pair relocations — out of v1 scope, loud STOP (mirrors the
    // torus block's `endpoint_set` guard). `None` is a loud STOP (tangency /
    // parallel normals or non-convergence — never a partial move, P9).
    for (&v, &(sa, sb)) in &vert_surface_pair {
        if endpoint_set.contains(&v) {
            // Endpoint-mix diagnosis probe (read-only, env-gated): the R0044
            // bucket's single STOP site. Dumps the surface pair carried by the
            // procedural edge PLUS the deduped surface set over every `inc0`
            // edge incident to `v` — the true junction incidence, which is what
            // decides whether the mix is a 3-surface triple point (solvable by
            // `relocate_onto_implicit_triple`) or something else.
            if std::env::var_os("YANG_LRR_PROBE").is_some() {
                let mut inc_surfs: Vec<(InputId, Surface)> = Vec::new();
                for (&(s, e), entries) in inc0.iter() {
                    if s != v && e != v {
                        continue;
                    }
                    for &(input, surf) in entries {
                        if !inc_surfs.iter().any(|&(i, t)| i == input && t == surf) {
                            inc_surfs.push((input, surf));
                        }
                    }
                }
                eprintln!(
                    "YANG_LRR_SITE site=surface_pair_endpoint_mix v={v} p={:?} \
                     pair=({sa:?}, {sb:?}) n_inc_surfs={} inc_surfs={inc_surfs:?} \
                     circle={} ellipse={} cone_ell={} parab={} hyp={} line={} \
                     ell_junction={} circle_junction={} line_circle_junction={}",
                    mesh.verts.get(v as usize),
                    inc_surfs.len(),
                    vert_circle.contains_key(&v),
                    vert_ellipse.contains_key(&v),
                    vert_cone_ellipse.contains_key(&v),
                    vert_parabola.contains_key(&v),
                    vert_cone_hyperbola.contains_key(&v),
                    vert_line.contains_key(&v),
                    vert_ell_junction.contains_key(&v),
                    vert_circle_junction.contains_key(&v),
                    vert_junction.contains_key(&v),
                );
            }
            return Err(YangError::stage4_region_invalid(
                v,
                Stage4InvalidReason::LocalRefinementRequired,
            ));
        }
        let p = mesh.verts[v as usize];
        let proj = relocate_onto_implicit_pair(p, sa, sb).ok_or_else(|| {
            YangError::stage4_region_invalid(v, Stage4InvalidReason::LocalRefinementRequired)
        })?;
        mesh.verts[v as usize] = proj;
        moved.insert(v);
    }

    // (2t) KV6d Tier B — degree-4 (TORUS) relocation via Newton on the implicit
    // surface pair. A torus's intersections are not conics, so these edges never
    // reach the `curves0` conic scan above; they arrive as untyped chord
    // segments and would otherwise stay off the analytic torus (the proven KV6d
    // blocker). For each intersection edge bearing exactly one torus and one
    // transversal partner, relocate both endpoints onto {F_torus=0, F_other=0}.
    // Kept SEPARATE from the conic bookkeeping (processed / endpoints /
    // relocations) — the output torus-intersection edges stay LineSegment
    // polylines (no analytic curve, no `t` retag), which validation and
    // `tessellate_torus_patch` already accept — so the conic no-skip audit above
    // is unaffected. Moved vertices join `moved` for the relocated-triangle
    // validation. Scope: one or two tori + one partner per edge (torus∩other
    // AND torus∩torus lateral, M5 #172); ≥3-surface junctions beyond the
    // triple arm and torus×conic endpoint mixing are loud STOPs (P9).
    {
        // Aggregate, per torus-edge endpoint, the base incident torus and the
        // DISTINCT partner surfaces across all its torus edges. One partner is a
        // plain torus∩surface edge (2-equation Newton) — the partner may itself
        // be a torus (torus×torus lateral, R0096); two partners is a
        // 3-surface JUNCTION — a box edge (two planes) piercing the torus, a
        // torus∩plane meeting a torus∩plane′, or torus×torus meeting a plane —
        // relocated onto all three. More than two partners is out of scope (STOP).
        let mut vert_torus: BTreeMap<u32, Surface> = BTreeMap::new();
        let mut vert_partners: BTreeMap<u32, Vec<Surface>> = BTreeMap::new();
        // Which vertices lie ON the intersection curve (an `inc0` edge spanning
        // both operands). An operand's OWN boundary vertex (§4.4.2: original
        // boundary curves are restored as they are) is still projected onto
        // its two surfaces below when they are TRANSVERSAL there — Stage-1
        // samples of a recovered rim can sit off the exact torus by more than
        // the on-surface band (R0026), and the projection is the same
        // well-posed Newton the intersection vertices get. Where the two
        // surfaces are TANGENT at such a vertex (the G1 cylinder↔torus rim of
        // a pipe, spec `b2_pipe_sweep.md`) the pair Newton is rank-deficient
        // by construction, and the vertex lies on their shared curve exactly
        // (both faces carry the one rim circle): nothing to solve, skip. A
        // tangency at a vertex ON the intersection curve stays the loud STOP
        // it always was.
        let mut on_curve: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
        for (&(s, e), entries) in &inc0 {
            let has_a = entries.iter().any(|&(i, _)| i == InputId::A);
            let has_b = entries.iter().any(|&(i, _)| i == InputId::B);
            if has_a && has_b {
                on_curve.insert(s);
                on_curve.insert(e);
            }
        }
        for (&(s, e), entries) in &inc0 {
            let has_a = entries.iter().any(|&(i, _)| i == InputId::A);
            let has_b = entries.iter().any(|&(i, _)| i == InputId::B);
            if has_a && has_b {
                on_curve.insert(s);
                on_curve.insert(e);
            }
        }
        for (&(s, e), entries) in &inc0 {
            let mut tori: Vec<Surface> = Vec::new();
            let mut others: Vec<Surface> = Vec::new();
            for &(_input, surf) in entries {
                if matches!(surf, Surface::Torus { .. }) {
                    tori.push(surf);
                } else {
                    others.push(surf);
                }
            }
            if tori.is_empty() {
                continue; // not a torus edge — conic scan / exact handles it
            }
            if tori.len() > 2 {
                // ≥3 distinct tori at one edge — out of scope. Loud STOP.
                return Err(YangError::stage4_region_invalid(
                    s,
                    Stage4InvalidReason::LocalRefinementRequired,
                ));
            }
            // M5 #172: a torus∩torus lateral edge (two incident tori) joins
            // the SAME implicit-pair relocation as torus∩other — Newton on
            // {F_a=0, F_b=0} needs no closed form, so the degree-8 torus×torus
            // curve needs no special casing (the P8 procedural-curve model,
            // spec `m5_surface_pair_curve.md`; corpus customer R0096). The
            // base is the FIRST torus recorded at the vertex (`or_insert` —
            // stable across the vertex's edges); every OTHER distinct incident
            // surface, second torus included, joins the partner set, so a
            // torus×torus∩plane junction resolves via the triple arm below.
            // Coincident tori self-guard: the pair Newton's tangential rank
            // gate (det ≤ rank_eps) returns None → loud STOP.
            for v in [s, e] {
                let base = *vert_torus.entry(v).or_insert(tori[0]);
                let entry = vert_partners.entry(v).or_default();
                for o in tori.iter().chain(others.iter()) {
                    // A §4.5.5 coplanar pair's two faces are ONE plane carried
                    // twice (opposite normals when a union's caps touch): a
                    // second copy would make the junction Newton
                    // rank-deficient (a split point of a spiric chain on the
                    // overlap boundary, increment 3a). Keep one.
                    let dup = entry
                        .iter()
                        .any(|q| q == o || crate::geom::planes_coincide(*q, *o));
                    if *o != base && !dup {
                        entry.push(*o);
                    }
                }
            }
        }
        'torus_verts: for (&v, &t_surf) in &vert_torus {
            // R0050 (2026-09-12): a torus∩conic corner with exactly three
            // incident surfaces was resolved by the general triple block
            // (position exact on all three) — nothing left to relocate.
            if triple_resolved.contains(&v) {
                continue 'torus_verts;
            }
            // A torus-edge endpoint that is also a CONIC endpoint mixes the
            // implicit-pair and closed-form relocations — out of v1 scope, STOP
            // (since 2026-09-12 only the ≠ 3-surface mixes reach here; the
            // 3-surface corner is the triple block's, see `triple_resolved`).
            if endpoint_set.contains(&v) {
                return Err(YangError::stage4_region_invalid(
                    v,
                    Stage4InvalidReason::LocalRefinementRequired,
                ));
            }
            let partners = &vert_partners[&v];
            let p = mesh.verts[v as usize];
            // Is the torus TANGENT to `other` at this vertex (parallel
            // normals)? The note above: at an operand's OWN vertex such a pair
            // leaves the Newton rank-deficient by construction while the
            // vertex already lies on the shared rim exactly — nothing to solve.
            let tangent_to = |other: &Surface| -> bool {
                match (
                    surface_distance_and_normal(t_surf, p.as_array()),
                    surface_distance_and_normal(*other, p.as_array()),
                ) {
                    (Some((_, a)), Some((_, b))) => {
                        let d = a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
                        1.0 - d * d
                            <= cad_primitives::MIN_FEATURE_SIZE * cad_primitives::MIN_FEATURE_SIZE
                    }
                    _ => false,
                }
            };
            let (proj, n0, n1, line_div, slab_div) = match partners.as_slice() {
                [s1] => {
                    if !on_curve.contains(&v) && tangent_to(s1) {
                        continue 'torus_verts;
                    }
                    if std::env::var_os("YANG_TORUS_PROBE").is_some()
                        && relocate_onto_implicit_pair(p, t_surf, *s1).is_none()
                    {
                        eprintln!(
                            "YANG_TORUS_STOP site=pair_newton_none v={v} p={p:?} \
                             t_surf={t_surf:?} partner={s1:?}"
                        );
                    }
                    let proj = relocate_onto_implicit_pair(p, t_surf, *s1).ok_or_else(|| {
                        YangError::stage4_region_invalid(
                            v,
                            Stage4InvalidReason::LocalRefinementRequired,
                        )
                    })?;
                    let qa = proj.as_array();
                    let (_, n0) = surface_value_and_normal(t_surf, qa).ok_or_else(|| {
                        YangError::stage4_region_invalid(
                            v,
                            Stage4InvalidReason::LocalRefinementRequired,
                        )
                    })?;
                    let (_, n1) = surface_value_and_normal(*s1, qa).ok_or_else(|| {
                        YangError::stage4_region_invalid(
                            v,
                            Stage4InvalidReason::LocalRefinementRequired,
                        )
                    })?;
                    (proj, n0, n1, None, None)
                }
                [s1, s2] => {
                    // 3-surface junction: relocate onto {torus, s1, s2}. The
                    // displacement gate uses the torus∩s1 curve corridor —
                    // EXCEPT when s1 and s2 are two planes: the vertex then
                    // sits on their LINE and moves along it, and the bound is
                    // the PR-KV11 line metric `2·d_ε/|L̂·n_torus|`
                    // (`junction_line_divergence`; R0077, 2026-09-11 — the
                    // curve corridor refused an exact box-edge pierce).
                    //
                    // SI5 C5b (2026-10-02): the pair arm's own-vertex tangency
                    // rule applies here too. A FILLET corner — the torus
                    // patch's own vertex where it meets its G1 neighbour
                    // (cylinder, cone or plane) and a third face — is exactly
                    // such a junction: the triple Newton is rank-deficient at
                    // every one of them by construction, and the vertex is a
                    // Stage-1 B-Rep vertex lying on all three surfaces
                    // exactly. Measured on the ingested quarter puck ∪ a
                    // block that never touches the fillet: the first STOP was
                    // this corner. A tangency at a vertex ON the intersection
                    // curve stays the loud STOP it always was.
                    if !on_curve.contains(&v) && (tangent_to(s1) || tangent_to(s2)) {
                        continue 'torus_verts;
                    }
                    if std::env::var_os("YANG_TORUS_PROBE").is_some()
                        && relocate_onto_implicit_triple(p, t_surf, *s1, *s2).is_none()
                    {
                        eprintln!(
                            "YANG_TORUS_STOP site=triple_newton_none v={v} p={p:?} \
                             t_surf={t_surf:?} s1={s1:?} s2={s2:?}"
                        );
                    }
                    let proj =
                        relocate_onto_implicit_triple(p, t_surf, *s1, *s2).ok_or_else(|| {
                            YangError::stage4_region_invalid(
                                v,
                                Stage4InvalidReason::LocalRefinementRequired,
                            )
                        })?;
                    let qa = proj.as_array();
                    let (_, n0) = surface_value_and_normal(t_surf, qa).ok_or_else(|| {
                        YangError::stage4_region_invalid(
                            v,
                            Stage4InvalidReason::LocalRefinementRequired,
                        )
                    })?;
                    let (_, n1) = surface_value_and_normal(*s1, qa).ok_or_else(|| {
                        YangError::stage4_region_invalid(
                            v,
                            Stage4InvalidReason::LocalRefinementRequired,
                        )
                    })?;
                    // R0070 (2026-09-17): the line-curve arm, same as the
                    // conic triple block ("fix all gates sharing a metric").
                    let line_div = junction_line_divergence([t_surf, *s1, *s2], qa).or_else(|| {
                        vert_line.get(&v).and_then(|lr| {
                            junction_line_curve_divergence(
                                (lr.point, lr.dir),
                                p.as_array(),
                                [t_surf, *s1, *s2],
                                qa,
                            )
                        })
                    });
                    // R0050 (2026-09-22): the THREE-SLAB arm, same as the
                    // conic triple block ("fix all gates sharing a metric").
                    let slab_div = if line_div.is_none() {
                        crate::stage4_relocate::junction_slab_divergence([t_surf, *s1, *s2], qa)
                    } else {
                        None
                    };
                    (proj, n0, n1, line_div, slab_div)
                }
                _ => {
                    if std::env::var_os("YANG_TORUS_PROBE").is_some() {
                        eprintln!(
                            "YANG_TORUS_STOP site=gt2_partners v={v} p={p:?} \
                             t_surf={t_surf:?} partners={partners:?}"
                        );
                    }
                    return Err(YangError::stage4_region_invalid(
                        v,
                        Stage4InvalidReason::LocalRefinementRequired,
                    ));
                }
            };
            // Derived displacement gate: a chord point moves to the exact curve
            // by ≤ 2·d_ε / sin θ, θ the angle between two incident surface
            // normals at the relocated point (the same metric as the disc∩disc /
            // cyl×cyl junction bands — NOT tolerance widening). Beyond it is a
            // real off-curve error, not a Stage-1 chord artifact → STOP. A
            // plane×plane junction moves along the planes' LINE instead and
            // takes the line metric (`line_div`, see the `[s1, s2]` arm).
            let pa = p.as_array();
            let qa = proj.as_array();
            let rho = ((qa[0] - pa[0]).powi(2) + (qa[1] - pa[1]).powi(2) + (qa[2] - pa[2]).powi(2))
                .sqrt();
            let cx = [
                n0[1] * n1[2] - n0[2] * n1[1],
                n0[2] * n1[0] - n0[0] * n1[2],
                n0[0] * n1[1] - n0[1] * n1[0],
            ];
            let pair_sin = (cx[0] * cx[0] + cx[1] * cx[1] + cx[2] * cx[2]).sqrt();
            // The slab arm is taken only where it admits MORE than the pair
            // corridor (never below it — see `junction_slab_divergence`).
            let slab_div = slab_div.filter(|s| *s < pair_sin);
            let sin_theta = line_div.or(slab_div).unwrap_or(pair_sin);
            let gate = tangent_plane_corridor(d_eps, sin_theta);
            if std::env::var_os("YANG_TORUS_PROBE").is_some() {
                let fv = surface_value_and_normal(t_surf, proj.as_array())
                    .map(|(f, _)| f)
                    .unwrap_or(f64::NAN);
                let metric = if line_div.is_some() {
                    "line"
                } else if slab_div.is_some() {
                    "slab"
                } else {
                    "curve"
                };
                eprintln!(
                    "YANG_TORUS_PROBE v={v} rho={rho:.4e} gate={gate:.4e} d_eps={d_eps:.4e} \
                     sin_theta={sin_theta:.4e} metric={metric} F_torus(proj)={fv:.2e} p={p:?} \
                     proj={proj:?} t_surf={t_surf:?} partners={partners:?}"
                );
            }
            if rho > gate {
                if let Some(e) = s451_stop(s451_collect, &mut s45_failures, v, &curves0, &inc0) {
                    return Err(e);
                }
                continue;
            }
            // Bounded-face containment (KV6d closed torus, spec
            // `kv6d_closed_torus_revolve.md` failure modes): the wedge gate
            // bounds the TRANSVERSAL chord error but balloons (2d_ε/sinθ)
            // when the pair is near-tangential — exactly where an inscribed
            // mesh can close an intersection loop EARLY (entirely inside the
            // partner's bounded face) and the implicit-pair Newton then drags
            // the loop onto the infinite-surface curve, OUTSIDE the partner
            // FACE (C0065: wall x=1.45 vs outer equator 1.5, gap ≈ sagitta —
            // relocated points at |y| = 0.384 vs the wall's |y| ≤ 0.25). A
            // correctly resolved intersection vertex lies on both BOUNDED
            // faces, so a relocation escaping every matching partner face's
            // vertex hull (+d_ε) is a mesh-resolution artifact, not a chord
            // correction → loud STOP (the §4.3.3 near-tangency increment owns
            // the honest fix). Planes only: a planar face's loop hull bounds
            // the face (curved hulls under-bound — closed seam loops).
            for partner in partners {
                // The containment reading lives in `planar_partner_hull_contains`
                // (extracted for the §4.5.1 torus-region repair — one reading,
                // two callers). `None` = no verdict = no wall, as always.
                if planar_partner_hull_contains(a, b, *partner, proj.as_array(), d_eps)
                    == Some(false)
                {
                    if let Some(e) = s451_stop(s451_collect, &mut s45_failures, v, &curves0, &inc0)
                    {
                        return Err(e);
                    }
                    // Recorded: skip THIS TORUS VERTEX — a plain `continue`
                    // would only skip the current partner-face check and fall
                    // through to the relocation write below.
                    continue 'torus_verts;
                }
            }
            if rho > cad_primitives::TAU_WORK {
                mesh.verts[v as usize] = proj;
                moved.insert(v);
            }
        }
    }

    // §4.5.1 inc-1 census (spec §7): the sweep completed past recorded
    // OffCurve failures. Measure the selector at the paper's own vantage —
    // every non-failed vertex is now relocated — then return the FIRST
    // recorded error unchanged: the stage cannot complete with unrepaired
    // failures (P10), and every later pass's precondition (a fully-relocated
    // mesh) stays intact because none of them run.
    let mut s451_repaired_any = false;
    if !s45_failures.is_empty() {
        if s451_repair {
            // §4.5.1 inc-2b: plan every region READ-ONLY, then apply. Any
            // decline returns the FIRST recorded error unchanged (P10 — no
            // partial acceptance), so a case whose failures the repair cannot
            // own keeps today's exact STOP.
            match s451_plan_repairs(
                mesh,
                &attribution.attributions,
                brep_a,
                brep_b,
                &curves0,
                &inc0,
                &s4_entry_pos,
                &s45_failures,
            ) {
                Ok(plans) => {
                    for plan in &plans {
                        for &victim in &plan.victims {
                            collapse_vertex_scoped(
                                mesh,
                                &mut attribution.attributions,
                                &mut side_labels.labels,
                                victim,
                                plan.survivor,
                                full_arrangement,
                                Some(tangential_verts),
                            );
                        }
                        mesh.verts[plan.survivor as usize] = plan.proj;
                        moved.insert(plan.survivor);
                        if let Some(t) = plan.retag {
                            processed.insert(plan.survivor);
                            relocations.push((plan.survivor, t));
                        }
                    }
                    if std::env::var_os("YANG_451_PROBE").is_some() || s451_mode == "census" {
                        eprintln!(
                            "YANG_451_REPAIR applied {} region repair(s); stage continues",
                            plans.len()
                        );
                    }
                    s451_repaired_any = !plans.is_empty();
                    s45_failures.clear();
                }
                Err(()) => {
                    record_collected_spans(
                        mesh,
                        &attribution.attributions,
                        brep_a,
                        brep_b,
                        &s4_entry_pos,
                        &s45_failures,
                    );
                    let (_, first) = s45_failures.swap_remove(0);
                    return Err(first);
                }
            }
        } else {
            s451_post_sweep_census(
                mesh,
                &attribution.attributions,
                brep_a,
                brep_b,
                &curves0,
                &s4_entry_pos,
                &s45_failures,
            );
            record_collected_spans(
                mesh,
                &attribution.attributions,
                brep_a,
                brep_b,
                &s4_entry_pos,
                &s45_failures,
            );
            let (_, first) = s45_failures.swap_remove(0);
            return Err(first);
        }
    }

    star_probe("after-reloc", mesh, attribution);
    nonmanifold_edge_census("after-reloc", mesh, attribution);
    coincident_vertex_census("after-reloc", mesh);
    // (3) §4.5.3 reversed-intersection correction sweep.
    // (`collapsed_any` starts true when §4.5.1 repairs collapsed vertices
    // above — the post-collapse Phase-A recompute must run for those too.)
    let mut collapsed_any = s451_repaired_any;
    let mut attr_vec = std::mem::take(&mut attribution.attributions);
    // PR-KV9: junction vertices that landed on the SAME exact point are
    // duplicates of one geometric junction (near a tangency-grade curve
    // crossing the two chord polylines can intersect several times, giving
    // several arrangement vertices for ONE junction). Collapse the extras
    // onto the lowest index — the standard edge-collapse, which drops the
    // degenerate slivers between them and keeps the half-edge pairing
    // watertight.
    {
        let mut by_pos: std::collections::BTreeMap<[u64; 3], Vec<u32>> =
            std::collections::BTreeMap::new();
        for &v in vert_ell_junction.keys() {
            let p = mesh.verts[v as usize];
            by_pos
                .entry([p.x().to_bits(), p.y().to_bits(), p.z().to_bits()])
                .or_default()
                .push(v);
        }
        // P0020 probe (read-only): the group above is drawn ONLY from the
        // junction arm's own output, so a junction point that is ALSO
        // occupied by a vertex another arm placed (or that the arrangement
        // already put exactly there) is not a "twin" by this population and
        // is never identified. Report every such outside occupant, with
        // whether a triangle joins it to the junction vertex.
        if std::env::var_os("YANG_KV9_TWIN_PROBE").is_some() {
            for (key, group) in &by_pos {
                let outside: Vec<u32> = mesh
                    .verts
                    .iter()
                    .enumerate()
                    .filter(|(v, p)| {
                        !group.contains(&(*v as u32))
                            && [p.x().to_bits(), p.y().to_bits(), p.z().to_bits()] == *key
                    })
                    .map(|(v, _)| v as u32)
                    .collect();
                if outside.is_empty() {
                    eprintln!(
                        "[kv9-twin] junction group {group:?} at {:?} has no outside occupant",
                        mesh.verts[group[0] as usize].as_array(),
                    );
                    continue;
                }
                let joined: Vec<(u32, u32)> = mesh
                    .tris
                    .iter()
                    .flat_map(|t| {
                        let g: Vec<u32> = t.iter().copied().filter(|v| group.contains(v)).collect();
                        let o: Vec<u32> = t
                            .iter()
                            .copied()
                            .filter(|v| outside.contains(v))
                            .collect::<Vec<_>>();
                        g.into_iter()
                            .flat_map(move |a| o.clone().into_iter().map(move |b| (a, b)))
                    })
                    .collect();
                eprintln!(
                    "[kv9-twin] junction group {group:?} at {:?} shares its exact position \
                     with non-junction vert(s) {outside:?}; triangles joining them: {joined:?}",
                    mesh.verts[group[0] as usize].as_array(),
                );
            }
        }
        for (_, group) in by_pos {
            if group.len() < 2 {
                continue;
            }
            let survivor = *group.iter().min().expect("non-empty");
            for &victim in group.iter().filter(|&&v| v != survivor) {
                if std::env::var_os("YANG_DOUBLECOVER_PROBE").is_some() {
                    eprintln!(
                        "[collapse-site] PR-KV9 junction-twin victim={victim} survivor={survivor}"
                    );
                }
                collapse_vertex_scoped(
                    mesh,
                    &mut attr_vec,
                    &mut side_labels.labels,
                    victim,
                    survivor,
                    full_arrangement,
                    Some(tangential_verts),
                );
                collapsed_any = true;
            }
        }
    }
    // P3b inc-4a (R0061): weld relocated verts that converged onto a Stage-1
    // MINTED junction BEFORE any pass that walks patch boundaries — the §4.5.3
    // sweep below recomputes Phase A, whose figure-eight wedge walk dies on a
    // machine-ε moved×mint twin pair (s6-wedge-walk-not-outgoing at R0061's
    // v173/v186). Same §4.3 op and band as the (3b′) weld; survivor = the mint
    // (bits are the shared cross-operand junction identity, N54). The (3b′)
    // call stays as the residual catch after the sweep + §4.4.1(b) merge.
    let probe_minted_verts: HashSet<u32> = if std::env::var_os("YANG_P3B_FOLD_PROBE").is_some() {
        mesh.verts
            .iter()
            .enumerate()
            .filter(|(_, p)| {
                minted_junction_keys.contains_key(&[
                    p.x().to_bits(),
                    p.y().to_bits(),
                    p.z().to_bits(),
                ])
            })
            .map(|(v, _)| v as u32)
            .collect()
    } else {
        HashSet::new()
    };
    // inc-4c fold probe (read-only, `YANG_P3B_FOLD_PROBE=x,y,z,r`): dump the
    // local triangle complex near a point BEFORE the moved×minted weld and
    // AFTER the trim — measures how the stacked collapses restructure the
    // corner fan (the R0061 over-used minted×minted edge). Fires in both gate
    // states (gate-OFF has no mints; the dump is then the correct-baseline
    // local structure).
    let fold_probe = |tag: &str,
                      mesh: &Mesh,
                      attr: &[Option<TriangleAttribution>],
                      minted_verts: &HashSet<u32>| {
        let Ok(spec) = std::env::var("YANG_P3B_FOLD_PROBE") else {
            return;
        };
        let parts: Vec<f64> = spec.split(',').filter_map(|s| s.parse().ok()).collect();
        let [px, py, pz, pr] = parts.as_slice() else {
            return;
        };
        let near = |v: u32| {
            let p = mesh.verts[v as usize].as_array();
            ((p[0] - px).powi(2) + (p[1] - py).powi(2) + (p[2] - pz).powi(2)).sqrt() <= *pr
        };
        for (t, tri) in mesh.tris.iter().enumerate() {
            if !tri.iter().any(|&v| near(v)) {
                continue;
            }
            let flags: Vec<String> = tri
                .iter()
                .map(|&v| {
                    let mut s = format!("v{v}");
                    if minted_verts.contains(&v) {
                        s.push('M');
                    } else if moved.contains(&v) {
                        s.push('m');
                    }
                    s
                })
                .collect();
            eprintln!(
                "[p3b-fold {tag}] tri {t} {} att={:?} coords {:?}",
                flags.join(","),
                attr.get(t).copied().flatten().map(|a| (a.input, a.face)),
                tri.iter()
                    .map(|&v| mesh.verts[v as usize])
                    .collect::<Vec<_>>()
            );
        }
    };
    fold_probe("pre", mesh, &attr_vec, &probe_minted_verts);
    if !minted_junction_keys.is_empty() {
        let minted_verts: HashSet<u32> = mesh
            .verts
            .iter()
            .enumerate()
            .filter(|(_, p)| {
                minted_junction_keys.contains_key(&[
                    p.x().to_bits(),
                    p.y().to_bits(),
                    p.z().to_bits(),
                ])
            })
            .map(|(v, _)| v as u32)
            .collect();
        if std::env::var_os("YANG_MOVED_WELD_PROBE").is_some() {
            eprintln!(
                "[moved-weld] pre-sweep: moved={} minted_verts={}",
                moved.len(),
                minted_verts.len()
            );
        }
        // Restrict the pre-sweep pass to moved×minted pairs only (empty
        // `moved` complement): pass `moved` as-is but rely on the weld's
        // pairing rule — a moved×moved weld here would reorder the
        // established (3b′)-after-sweep behavior, so filter to pairs
        // involving a mint by handing the weld ONLY the moved verts within
        // the coincidence band of some mint.
        if !minted_verts.is_empty() {
            let mut near_mint_moved: HashSet<u32> = HashSet::new();
            for &mv in moved.iter() {
                let pm = mesh.verts[mv as usize].as_array();
                for &jv in &minted_verts {
                    let pj = mesh.verts[jv as usize].as_array();
                    let d = ((pm[0] - pj[0]).powi(2)
                        + (pm[1] - pj[1]).powi(2)
                        + (pm[2] - pj[2]).powi(2))
                    .sqrt();
                    let scale = pm
                        .iter()
                        .chain(pj.iter())
                        .fold(0.0f64, |m, &c| m.max(c.abs()));
                    if d < cad_primitives::TAU_MODEL * (1.0 + scale) {
                        near_mint_moved.insert(mv);
                        break;
                    }
                }
            }
            if !near_mint_moved.is_empty()
                && weld_coincident_relocated_ex(
                    mesh,
                    &mut attr_vec,
                    &mut side_labels.labels,
                    a,
                    b,
                    &near_mint_moved,
                    &minted_verts,
                    full_arrangement,
                    Some(tangential_verts),
                )
            {
                collapsed_any = true;
            }
        }
        // P0020 bisect (2026-10-03, read-only): the four passes between
        // `after-reloc` and `before-3c-merge` each get their own non-manifold
        // edge checkpoint, because P0020's over-2 edges are ABSENT at
        // `after-reloc` and PRESENT at `before-3c-merge` — unlike F0060's,
        // which the arrangement hands over at `s4-entry`.
        nonmanifold_edge_census_attrs("after-moved-mint-weld", mesh, &attr_vec);
        // P3b inc-4b: beyond-corner conformal trim, immediately AFTER the
        // moved×minted weld (the weld owns coincidence ≤ TAU_MODEL band; the
        // trim owns band→corridor beyond-corner phantoms — F0082's 2.76e-3).
        // Re-resolve mint vert ids WITH provenance: the weld above may have
        // collapsed vertices, but mint coordinates are never mutated.
        let minted_prov: std::collections::BTreeMap<u32, crate::boolean::MintProvenance> = mesh
            .verts
            .iter()
            .enumerate()
            .filter_map(|(v, p)| {
                minted_junction_keys
                    .get(&[p.x().to_bits(), p.y().to_bits(), p.z().to_bits()])
                    .map(|prov| (v as u32, *prov))
            })
            .collect();
        // I1c census (probe-only): the TF-8 anchor needs the IDENTITY of the
        // overshot seam junctions at trim time — minted? moved? neither? —
        // matched offline by position (ids differ across passes; mint
        // coordinates never mutate).
        if std::env::var_os("YANG_P3B_TRIM_PROBE").is_some() {
            for (&v, prov) in &minted_prov {
                let p = mesh.verts[v as usize];
                eprintln!(
                    "[p3b-minted] v{v} ({:.12}, {:.12}, {:.12}) planes trim_beyond=[{},{}]",
                    p.x(),
                    p.y(),
                    p.z(),
                    prov.owner_planes[0].trim_beyond,
                    prov.owner_planes[1].trim_beyond,
                );
            }
            for &v in &moved {
                let p = mesh.verts[v as usize];
                eprintln!(
                    "[p3b-moved] v{v} ({:.12}, {:.12}, {:.12})",
                    p.x(),
                    p.y(),
                    p.z()
                );
            }
        }
        if !minted_prov.is_empty()
            && trim_beyond_corner_phantoms_ex(
                mesh,
                &mut attr_vec,
                &mut side_labels.labels,
                &moved,
                &minted_prov,
                d_eps,
                full_arrangement,
                Some(tangential_verts),
            )
        {
            collapsed_any = true;
        }
        nonmanifold_edge_census_attrs("after-beyond-corner-trim", mesh, &attr_vec);
        // P3b inc-4c: the §4.4.1 triangulation-update half of the merges
        // above — dissolve the fan folds the stacked collapses manufacture
        // (spec `yang_169_p3b_inc4c_fan_retriangulation.md`). Connectivity
        // only; per-cluster fail-closed; must run before ANY boundary-walking
        // pass (the sweep below recomputes Phase A — the inc-4a placement
        // lesson).
        if retriangulate_collapsed_fan_regions_ex(
            mesh,
            &mut attr_vec,
            &mut side_labels.labels,
            brep_a,
            brep_b,
            &moved,
            &minted_verts,
            full_arrangement,
        ) {
            collapsed_any = true;
        }
        nonmanifold_edge_census_attrs("after-fan-retriangulation", mesh, &attr_vec);
    }
    fold_probe("post", mesh, &attr_vec, &probe_minted_verts);
    s4_lap("before-453-sweep", mesh);
    let sweep_result = sweep_reversed_intersections(
        mesh,
        &mut attr_vec,
        &mut side_labels.labels,
        a,
        b,
        d_eps,
        &moved,
        &s4_entry_pos,
        minted_junction_keys,
        full_arrangement,
        Some(tangential_verts),
    );
    attribution.attributions = attr_vec;
    let any_collapse = sweep_result?;
    collapsed_any |= any_collapse;

    star_probe("before-3c-merge", mesh, attribution);
    coincident_vertex_census("after-453-sweep", mesh);
    nonmanifold_edge_census("before-3c-merge", mesh, attribution);
    // (3c) §4.4.1(b) sub-feature-size vertex merge (Yang Fig. 11(b): "if an
    // endpoint p of the split edge is too close to q, we merge p with q"). After
    // relocation a degenerate triangle can have two vertices nearer than
    // MIN_FEATURE_SIZE — the governance feature floor (A14.2): two points closer
    // than the smallest representable feature ARE the same point. This is the
    // curved-input analog of the I6 near-weld (which is bit-exact-only for curved
    // inputs — "Stage-4 owns junction-duplicate collapse"). Merge such a pair via
    // the watertight-preserving `collapse_vertex` (higher index → lower, dropping
    // the now-degenerate slivers), iterating to a fixed point. P9/P10: the gate is
    // the GOVERNANCE feature floor, not a tuned tolerance, and a genuinely-spread
    // degenerate (vertices ≥ the floor apart — e.g. a monotonic-collinear sliver
    // on a curved patch) is left UNTOUCHED for `validate_relocated_triangles` to
    // STOP loudly / the curved-patch re-CDT (N2-2) to handle. Spec
    // `specs/yang_n2_stage4_cdt_mesh_updating.md` §5 increment N2-1.
    //
    // SCOPE NOTE (M8 holed-disc increment 3, 2026-07-06): a GLOBAL widening of
    // this scan (all triangles + a Stage-4 ENTRY pass) was tried and REVERTED —
    // at micro model scale (R0091, 1.6e-4) the ABSOLUTE floor collapses
    // legitimately-distinct arrangement geometry (Euler flipped to −4,
    // SUPPORTED_WRONG). The relocation/conic-adjacent eligibility below is
    // LOAD-BEARING: it keeps the merge away from pre-existing arrangement
    // slivers that `boolean()` legitimately kept for watertightness.
    // #169 N55: Yang §4.4.1(b) numerical-duplicate merge — COMPLIANT and
    // ALWAYS-ON (replaces the retired absolute-floor `subfeature` weld). The
    // paper's Fig-11(b) "if p is too close to q, merge p with q" is a
    // NUMERICAL-COINCIDENCE test (two relocated points that Newton-converged to
    // the SAME intersection point), not a feature-size floor. The criterion is
    // therefore the scale-relative working tolerance `TAU_WORK·(1+scale)` at the
    // edge gate below (an exact-dedup merge — the kind the compliance ratchet
    // KEEPS), NOT the absolute `MIN_FEATURE_SIZE` the weld used (which also
    // collapsed genuine sub-resolution edges at micro-scale — R0072's ~1e-7
    // merges, the R0091 hazard — now correctly refused → curved re-CDT). `floor`
    // here is only the DEGENERACY DETECTOR (a triangle below the feature floor
    // is a merge candidate); the actual same-point decision is the tighter
    // numerical band. Deviation N55.
    {
        let floor = cad_primitives::MIN_FEATURE_SIZE;
        let mut attr_vec = std::mem::take(&mut attribution.attributions);
        // KV9-F3 (spec `kv9_f3_output_vertex_identity` E-V2): junction
        // duplicates that are ALREADY on their exact curve (rho ≤ TAU_WORK)
        // are never `moved`, yet they are precisely the population the I6
        // weld delegates to Stage-4 ("Stage-4 owns junction-duplicate
        // collapse" — curved inputs weld bit-exact only). Scan eligibility
        // therefore includes triangles touching any CONIC-ENDPOINT vertex;
        // the merge criterion below is unchanged (the governance
        // MIN_FEATURE_SIZE floor, A14.2 — never a tuned tolerance).
        let conic_endpoint: std::collections::BTreeSet<u32> = vert_circle
            .keys()
            .chain(vert_line.keys())
            .chain(vert_ellipse.keys())
            .chain(vert_cone_ellipse.keys())
            .chain(vert_parabola.keys())
            .chain(vert_cone_hyperbola.keys())
            .chain(vert_ell_junction.keys())
            .chain(vert_circle_junction.keys())
            .copied()
            .collect();
        // Spec `yang_453_junction_protected_collapse` §3b: closed-form junction
        // vertices (exact on TWO curves) outrank single-curve conic endpoints,
        // which outrank plain mesh vertices, in merge-survivor selection.
        let junction_verts: std::collections::BTreeSet<u32> = vert_ell_junction
            .keys()
            .chain(vert_circle_junction.keys())
            .chain(vert_junction.keys())
            .copied()
            .collect();
        // Each pass collapses ≤1 sub-feature edge; bounded by the triangle count.
        let max_merge_passes = mesh.tris.len() + 1;
        let mut merge_passes = 0usize;
        let mut last_merge: Option<(u32, u32, f64, usize)> = None;
        loop {
            merge_passes += 1;
            if merge_passes > max_merge_passes {
                // §4.4.1(b) diagnosis probe (read-only, env-gated): the budget
                // guard should be unreachable if every pass drops ≥1 triangle
                // — print the terminal state to localize a livelock.
                if std::env::var_os("YANG_S4_MERGE_PROBE").is_some() {
                    eprintln!(
                        "[s4-merge-probe] BUDGET EXHAUSTED: passes={merge_passes} \
                         max={max_merge_passes} tris_now={} last_merge={last_merge:?}",
                        mesh.tris.len()
                    );
                }
                if std::env::var_os("YANG_LRR_PROBE").is_some() {
                    eprintln!("YANG_LRR_STOP site=merge_budget");
                }
                attribution.attributions = attr_vec;
                return Err(YangError::stage4_region_invalid(
                    u32::MAX,
                    Stage4InvalidReason::LocalRefinementRequired,
                ));
            }
            let mut to_merge: Option<(u32, u32)> = None;
            for tri in &mesh.tris {
                if !tri
                    .iter()
                    .any(|v| moved.contains(v) || conic_endpoint.contains(v))
                {
                    continue;
                }
                let p0 = mesh.verts[tri[0] as usize].as_array();
                let p1 = mesh.verts[tri[1] as usize].as_array();
                let p2 = mesh.verts[tri[2] as usize].as_array();
                let nrm = tri_area_vector(p0, p1, p2);
                let twice_area = (nrm[0] * nrm[0] + nrm[1] * nrm[1] + nrm[2] * nrm[2]).sqrt();
                if twice_area * 0.5 >= floor * floor {
                    continue; // not degenerate — leave it
                }
                // Degenerate relocated triangle: if its SHORTEST edge is below the
                // feature floor, those two endpoints are the same point → merge.
                let dist = |a: [f64; 3], b: [f64; 3]| {
                    let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
                    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
                };
                let edges = [
                    (tri[0], tri[1], dist(p0, p1)),
                    (tri[1], tri[2], dist(p1, p2)),
                    (tri[2], tri[0], dist(p2, p0)),
                ];
                let (u, v, len) = edges
                    .iter()
                    .copied()
                    .min_by(|x, y| x.2.partial_cmp(&y.2).unwrap_or(std::cmp::Ordering::Equal))
                    .expect("3 edges");
                // §4.4.1(b) same-point test: the shortest edge is a numerical
                // coincidence iff below the scale-relative working tolerance
                // `TAU_WORK·(1+scale)` (`scale` = max |coord| of the pair). This
                // is the model's own "numerically identical" threshold — 5 orders
                // tighter than `MIN_FEATURE_SIZE` — so it merges only relocation
                // twins that converged onto one point (~machine ε; R0055/F0056/
                // F0057/F0059) and never a genuine sub-feature edge (R0072's ~1e-7
                // collapse at micro-scale is refused → loud STOP → curved re-CDT).
                let scale = {
                    let pu = mesh.verts[u as usize].as_array();
                    let pv = mesh.verts[v as usize].as_array();
                    pu.iter()
                        .chain(pv.iter())
                        .fold(0.0f64, |m, &c| m.max(c.abs()))
                };
                let merge = is_relocation_coincidence(len, scale);
                if std::env::var_os("YANG_S44B_MEASURE").is_some() {
                    eprintln!(
                        "[s44b] cand u={u} v={v} len={len:.4e} scale={scale:.4e} \
                         band={:.4e} merge={merge}",
                        cad_primitives::TAU_WORK * (1.0 + scale)
                    );
                }
                if merge {
                    // Spec `yang_453_junction_protected_collapse` §3b: the
                    // exactness-ranked survivor (Yang Fig. 11(b) — "merge p
                    // with q": the exact intersection point q survives).
                    // WIRED 2026-07-21 (task #186): the §3b blocker was the
                    // unverified R0091 χ — resolved by verifying the output's
                    // true χ = −4 via Cherchi sidecar reference parity on the
                    // exact operand meshes + an independent voxel-CSG
                    // derivation from the authored numbers (the meta's naive
                    // 3-op default χ=2 was the authoring error; corrected).
                    let (victim, survivor) =
                        sub_feature_merge_direction(&junction_verts, &conic_endpoint, u, v);
                    to_merge = Some((victim, survivor));
                    break;
                }
            }
            match to_merge {
                Some((victim, survivor)) => {
                    if std::env::var_os("YANG_DOUBLECOVER_PROBE").is_some() {
                        eprintln!(
                            "[collapse-site] s4.4.1b-merge victim={victim} survivor={survivor}"
                        );
                    }
                    let dropped = collapse_vertex_scoped(
                        mesh,
                        &mut attr_vec,
                        &mut side_labels.labels,
                        victim,
                        survivor,
                        full_arrangement,
                        Some(tangential_verts),
                    );
                    last_merge = Some((victim, survivor, dropped as f64, mesh.tris.len()));
                    collapsed_any = true;
                }
                None => break,
            }
        }
        attribution.attributions = attr_vec;
    }

    star_probe("after-3c-merge", mesh, attribution);
    nonmanifold_edge_census("after-3c-merge", mesh, attribution);
    coincident_vertex_census("after-3c-merge", mesh);
    // (3b′) Coincident RELOCATED-vertex weld (spec `yang_n47_coincident_moved_weld`,
    // deviation N47). Two vertices this pipeline RELOCATED (`moved`: pushed onto an
    // analytic circle/ellipse/line/torus/surface-pair) can converge to within the
    // MODEL coincidence tolerance `TAU_MODEL·(1+scale)` — they are the SAME
    // geometric point emitted twice (a near-tangent seam crossing whose two
    // arrangement points both Newton-project onto one intersection point). The
    // §4.4.1(b) merge above misses them: it scans TRIANGLE edges and gates on the
    // triangle AREA (`floor²`), so a NEEDLE (two coincident verts + one far vert:
    // large area, sub-floor edge) is skipped, and a coincident pair that is only
    // LOOP-adjacent (not a shared triangle edge) is never examined. Left in place,
    // the twins survive into `emit_topology` (vertices 1:1 with `mesh.verts`) as a
    // sub-render-precision output edge, tripping kernel-v2's G1 render-collapse
    // gate FAR downstream (R0012 face 1023 @ 7e-7 / scale 100; R0098 face 599 @
    // 4e-6 / scale 1900). Welding here is a self-localizing PRODUCER fix.
    //
    // Band: the scale-relative MODEL coincidence tolerance `TAU_MODEL·(1+scale)`
    // (`scale` = max |coord| of the pair) — the SAME band the stage-5 planarity
    // wall and every other coincidence test uses; it is 10× TIGHTER than the
    // MIN_FEATURE_SIZE feature floor, so it admits ONLY sub-(feature/10)
    // coincidences (a genuine feature is ≥ `MIN_FEATURE_SIZE·(1+scale)` apart).
    // NOT tolerance widening (P9): it is the model's own definition of "same
    // point," and it only ever COLLAPSES an already-degenerate output edge.
    // Restricted to `moved`×`moved` pairs (the relocation-convergence mechanism) —
    // it never touches un-relocated arrangement geometry `boolean()` kept for
    // watertightness (cf. the §4.4.1(b) micro-scale R0091 revert). `collapse_vertex`
    // is the proven watertight-preserving edge-collapse (with membrane
    // cancellation); iterate to a fixed point over live (still-referenced) verts.
    // #169 N56: reinstated as a COMPLIANT always-on Yang §4.3 operation ("we
    // remove a point if it is too close to another point on the same loop",
    // paper line 535). Both verts are `moved` = relocated onto the analytic
    // curve, so merging one into the other is faithful redundant-curve-point
    // removal, not a tolerance hack. Measured 0-conversion on the current
    // corpus (the R0012/R0098 render twins named above are NOT reached by this
    // §4.3 merge — they are un-relocated arrangement verts needing the Stage-0
    // fix); it is kept as paper machinery for near-tangency (#137). Genuine
    // Yang ⇒ un-gated (was `weld_enabled("coincident")`).
    {
        // P3b inc-4a: resolve the Stage-1 minted junction points (exact bits,
        // threaded from `boolean()`) to mesh vertex ids. Bit-exact match only —
        // the mint contract preserves the bits through Stage 1 + arrangement,
        // and coordinates are never mutated by the collapses above.
        let minted_verts: HashSet<u32> = if minted_junction_keys.is_empty() {
            HashSet::new()
        } else {
            mesh.verts
                .iter()
                .enumerate()
                .filter(|(_, p)| {
                    minted_junction_keys.contains_key(&[
                        p.x().to_bits(),
                        p.y().to_bits(),
                        p.z().to_bits(),
                    ])
                })
                .map(|(v, _)| v as u32)
                .collect()
        };
        if std::env::var_os("YANG_MOVED_WELD_PROBE").is_some() {
            eprintln!(
                "[moved-weld] entry: moved={} minted_keys={} minted_verts={:?}",
                moved.len(),
                minted_junction_keys.len(),
                minted_verts
            );
        }
        let mut attr_vec = std::mem::take(&mut attribution.attributions);
        if weld_coincident_relocated_ex(
            mesh,
            &mut attr_vec,
            &mut side_labels.labels,
            a,
            b,
            &moved,
            &minted_verts,
            full_arrangement,
            Some(tangential_verts),
        ) {
            collapsed_any = true;
        }
        attribution.attributions = attr_vec;
    }

    // Phase 2b increment 0 (print-only, `YANG_441_UPDATE_CENSUS`): the
    // §4.4.1 per-patch update's outcome at its planned site — after the §4.3
    // dedups, before the unzip / validation repairs.
    crate::stage4_construct::update_census_with_side_labels(
        mesh,
        attribution,
        full_arrangement.then_some(&*side_labels),
        a,
        b,
        "pre-repair",
    );
    // Phase 2b (spec `yang_441_phase2b_patch_update.md`): the §4.4.1
    // per-patch update — every intersected patch re-triangulated in its
    // chart against its own cycles, before the repairs below.
    s4_lap("before-441-update", mesh);
    if crate::stage4_construct::update_patches_ex(
        mesh,
        attribution,
        side_labels,
        a,
        b,
        full_arrangement,
    ) > 0
    {
        collapsed_any = true;
    }
    s4_lap("441-update", mesh);

    // Twin-scan probe (read-only, env-gated `YANG_TWIN_SCAN`): dump every
    // sub-feature-floor mesh edge surviving the §4.4.1(b) merge, with
    // eligibility flags — self-localizes a surviving ULP-twin pair (the
    // F0047 render-collapse diagnosis tool).
    if std::env::var_os("YANG_TWIN_SCAN").is_some() {
        let floor = cad_primitives::MIN_FEATURE_SIZE;
        let mut seen: std::collections::BTreeSet<(u32, u32)> = std::collections::BTreeSet::new();
        for tri in &mesh.tris {
            for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
                let (u, v) = (tri[i].min(tri[j]), tri[i].max(tri[j]));
                if u == v || !seen.insert((u, v)) {
                    continue;
                }
                let pu = mesh.verts[u as usize].as_array();
                let pv = mesh.verts[v as usize].as_array();
                let d = [pu[0] - pv[0], pu[1] - pv[1], pu[2] - pv[2]];
                let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
                if len < floor {
                    eprintln!(
                        "[twin-scan] edge ({u},{v}) len={len:.3e} \
                         exact_j=({},{}) moved=({},{}) pu={pu:?}",
                        exact_junctions.contains(&u),
                        exact_junctions.contains(&v),
                        moved.contains(&u),
                        moved.contains(&v),
                    );
                }
            }
        }
    }

    // KV9-F1 Increment 0c census: post-merge junction-twin state — coincident
    // junction vertices that SURVIVED the §4.4.1(b) merge, and whether the
    // survivors are edge-adjacent in the current mesh (kept env-gated).
    if std::env::var("KV9_JUNCTION_PROBE").is_ok() {
        let keys: Vec<u32> = vert_ell_junction.keys().copied().collect();
        for (i, &u) in keys.iter().enumerate() {
            for &w in &keys[i + 1..] {
                let (pu, pw) = (mesh.verts[u as usize], mesh.verts[w as usize]);
                if pu.as_array() != pw.as_array() {
                    continue;
                }
                let adjacent = mesh.tris.iter().any(|t| t.contains(&u) && t.contains(&w));
                let (du, dw) = (
                    mesh.tris.iter().filter(|t| t.contains(&u)).count(),
                    mesh.tris.iter().filter(|t| t.contains(&w)).count(),
                );
                eprintln!(
                    "KV9_JUNCTION_PROBE post-merge coincident twins: v{u} v{w} at {:?} \
                     edge_adjacent={adjacent} deg({u})={du} deg({w})={dw}",
                    pu.as_array()
                );
            }
        }
    }

    star_probe("before-3d", mesh, attribution);
    nonmanifold_edge_census("before-3d", mesh, attribution);
    // (3d) §4.4.1(a) edge-split (Yang Fig. 11(a): "locate the constrained edge
    // containing q, split it at q"). A degenerate relocated triangle D=[a,b,c] is
    // collinear: the vertex OFF its longest edge (`b`) lies on that long edge
    // `a-c` (a redundant intersection point on the constraint curve). The faithful
    // fix inserts `b` into the triangle ON THE OTHER SIDE of `a-c` — split that
    // neighbour N=[a,c,d] into [a,b,d]+[b,c,d] — and drops D. This is a LOCAL,
    // watertight-preserving operation (D's edges a-b/b-c re-pair with the split
    // halves; the long edge a-c, shared only by D and N, vanishes): no re-CDT, no
    // parametric domain, no cylinder θ-seam. Iterate, each step acting on a
    // degenerate triangle whose long-edge neighbour is NON-degenerate (so the
    // strip unzips from its non-degenerate margin inward); a remaining degenerate
    // triangle with no non-degenerate neighbour is a genuine §4.5.2 STOP. Spec
    // `specs/yang_n2_stage4_cdt_mesh_updating.md`.
    //
    // DEGENERACY IS THE SCALE-FREE COLLINEARITY IDENTITY (`tri_is_degenerate`,
    // 2026-08-19): the former absolute `MIN_FEATURE_SIZE²` area floor flagged
    // HEALTHY micro-scale triangles (R0009/R0047 at 1e-4 m: h/l 0.007–0.4),
    // so this arm flipped real triangles (silent geometry change on R0091/
    // R0072/R0063) and ping-ponged to the pass cap on R0009. The unzip's own
    // precondition — dropping D preserves geometry — holds ONLY when D is
    // numerically zero-area relative to its extent; that is the test now.
    {
        balance_census(mesh, "pre-degen-loop");
        let is_degen = |ti: usize, mesh: &Mesh| -> bool {
            let t = mesh.tris[ti];
            if !t.iter().any(|v| moved.contains(v)) {
                return false;
            }
            tri_is_degenerate(
                mesh.verts[t[0] as usize].as_array(),
                mesh.verts[t[1] as usize].as_array(),
                mesh.verts[t[2] as usize].as_array(),
            )
        };
        // The off-longest-edge vertex `b` (the collinear middle) + extremes a,c.
        let long_edge_off = |t: &[u32; 3], mesh: &Mesh| -> (u32, u32, u32) {
            let d = |i: usize, j: usize| {
                let p = mesh.verts[t[i] as usize].as_array();
                let q = mesh.verts[t[j] as usize].as_array();
                let e = [p[0] - q[0], p[1] - q[1], p[2] - q[2]];
                e[0] * e[0] + e[1] * e[1] + e[2] * e[2]
            };
            let (e01, e12, e20) = (d(0, 1), d(1, 2), d(2, 0));
            if e01 >= e12 && e01 >= e20 {
                (t[0], t[1], t[2]) // long a-c = v0-v1, off b = v2
            } else if e12 >= e20 {
                (t[1], t[2], t[0])
            } else {
                (t[2], t[0], t[1])
            }
        };
        let mut attr_vec = std::mem::take(&mut attribution.attributions);
        let max_passes = mesh.tris.len() + 1;
        let mut passes = 0usize;
        // Progress certificate (findings Q3's per-pass monitor, applied here):
        // the loop is deterministic in the mesh state, so a simple action
        // re-firing on the SAME (D, N, a, c, b) tuple means an earlier action
        // was undone — a ping-pong that would spin to the pass cap (R0009:
        // a 4-action cycle; R0047: 5168 actions / 62 s under the old floor).
        // STOP on the first repeat instead of burning O(T) passes × O(T).
        let mut seen_actions: std::collections::HashSet<(usize, usize, u32, u32, u32)> =
            std::collections::HashSet::new();
        let mut seen_sync_actions: std::collections::HashSet<(
            usize,
            usize,
            [usize; 2],
            u32,
            u32,
            u32,
        )> = std::collections::HashSet::new();
        loop {
            passes += 1;
            if passes > max_passes {
                if std::env::var_os("YANG_LRR_PROBE").is_some() {
                    eprintln!("YANG_LRR_STOP site=split_max_passes");
                }
                attribution.attributions = attr_vec;
                return Err(YangError::stage4_region_invalid(
                    u32::MAX,
                    Stage4InvalidReason::LocalRefinementRequired,
                ));
            }
            // Edge → incident triangle indices (for the across-edge neighbour).
            let mut edge_tris: std::collections::HashMap<(u32, u32), Vec<u32>> =
                std::collections::HashMap::new();
            for (ti, tri) in mesh.tris.iter().enumerate() {
                for (i, j) in [(0, 1), (1, 2), (2, 0)] {
                    let (u, v) = (tri[i], tri[j]);
                    let key = if u < v { (u, v) } else { (v, u) };
                    edge_tris.entry(key).or_default().push(ti as u32);
                }
            }
            // Pick a degenerate triangle whose long-edge neighbour is non-degenerate.
            let mut action: Option<(usize, usize, u32, u32, u32)> = None;
            let mut sync_action: Option<(usize, usize, [usize; 2], u32, u32, u32)> = None;
            // Fallback: a MUTUAL pair — both incident triangles of one long edge
            // degenerate with that same edge as their long edge (a zero-area quad
            // astride the edge, off-vertices interleaved along it). No simple
            // action can ever fire on either member (each is the other's
            // neighbour), so when only such pairs remain the loop STOPs. The
            // mutual arm resolves the quad: drop both, and Fig-11(a)-split the
            // two OUTER neighbours so both sides carry the identical fine chain
            // a–bL–bH–c (two-sided conformal by construction, no geometry moved).
            // ALWAYS-ON since 2026-07-31 (was `YANG_S4_MUTUAL_PAIR_ENABLE`;
            // spec §5c.11 — corpus sweep: zero category deltas, F0067/R0038
            // advance to their deeper pre-existing walls).
            let mut mutual: Option<MutualPair> = None;
            let mut any_degen = false;
            for ti in 0..mesh.tris.len() {
                if !is_degen(ti, mesh) {
                    continue;
                }
                any_degen = true;
                let (a, c, b) = long_edge_off(&mesh.tris[ti], mesh);
                let key = if a < c { (a, c) } else { (c, a) };
                let Some(inc) = edge_tris.get(&key) else {
                    continue;
                };
                if full_arrangement && inc.len() == 4 {
                    let lbl_d = side_labels.labels[ti];
                    let pure_ops = match lbl_d.surface {
                        [true, false] => Some((InputId::A, InputId::B)),
                        [false, true] => Some((InputId::B, InputId::A)),
                        _ => None,
                    };
                    if let Some((op_d, other_op)) = pure_ops {
                        let mut same_partners: Vec<usize> = Vec::new();
                        let mut other_partners: Vec<usize> = Vec::new();
                        let mut valid_partition = true;
                        for &u in inc {
                            let ui = u as usize;
                            if ui == ti {
                                continue;
                            }
                            let ul = side_labels.labels[ui];
                            if ul.in_operand(op_d) && !ul.in_operand(other_op) {
                                same_partners.push(ui);
                            } else if ul.in_operand(other_op) && !ul.in_operand(op_d) {
                                other_partners.push(ui);
                            } else {
                                valid_partition = false;
                                break;
                            }
                        }
                        if valid_partition && same_partners.len() == 1 && other_partners.len() == 2
                        {
                            let n_same = same_partners[0];
                            let [o1, o2] = [other_partners[0], other_partners[1]];
                            let third_of = |idx: usize| {
                                mesh.tris[idx].iter().copied().find(|&v| v != a && v != c)
                            };
                            let o1_is_b = third_of(o1) == Some(b);
                            let o2_is_b = third_of(o2) == Some(b);
                            let same_ok = !is_degen(n_same, mesh) && third_of(n_same) != Some(b);
                            let o1_ok = o1_is_b || !is_degen(o1, mesh);
                            let o2_ok = o2_is_b || !is_degen(o2, mesh);
                            if same_ok && !(o1_is_b && o2_is_b) && o1_ok && o2_ok {
                                sync_action = Some((ti, n_same, [o1, o2], a, c, b));
                                break;
                            }
                        }
                    }
                    continue;
                }
                if inc.len() != 2 {
                    continue; // boundary / non-manifold long edge — skip
                }
                let n = if inc[0] as usize == ti {
                    inc[1]
                } else {
                    inc[0]
                } as usize;
                if full_arrangement
                    && (side_labels.labels[ti].surface != side_labels.labels[n].surface
                        || side_labels.labels[ti].inside != side_labels.labels[n].inside)
                {
                    continue;
                }
                if is_degen(n, mesh) {
                    // Simple arm defers to let the neighbour resolve first; if the
                    // neighbour's long edge is the SAME edge, neither ever will —
                    // record the pair as a mutual-arm candidate (executed only when
                    // no simple action exists anywhere this pass).
                    if mutual.is_none() {
                        mutual = mutual_pair_candidate(
                            mesh,
                            &edge_tris,
                            &is_degen,
                            &long_edge_off,
                            ti,
                            n,
                            a,
                            c,
                            b,
                        );
                    }
                    continue; // defer until the neighbour is resolved
                }
                action = Some((ti, n, a, c, b));
                break;
            }
            if let Some((d_idx, n_same, other_pair, a, c, b)) = sync_action {
                if !seen_sync_actions.insert((d_idx, n_same, other_pair, a, c, b)) {
                    if std::env::var_os("YANG_LRR_PROBE").is_some() {
                        eprintln!(
                            "YANG_LRR_STOP site=sync_split_cycle d={d_idx} n_same={n_same} \
                             other={other_pair:?} a={a} c={c} b={b} pass={passes}"
                        );
                    }
                    attribution.attributions = attr_vec;
                    return Err(YangError::stage4_region_invalid(
                        u32::MAX,
                        Stage4InvalidReason::LocalRefinementRequired,
                    ));
                }
                if resolve_synchronized_degenerate_split(
                    mesh,
                    &mut attr_vec,
                    &mut side_labels.labels,
                    d_idx,
                    n_same,
                    other_pair,
                    (a, c, b),
                    Some(tangential_verts),
                ) {
                    collapsed_any = true;
                    continue;
                }
            }
            let (d_idx, n_idx, a, c, b) = match action {
                Some(x) => x,
                None => {
                    if let Some(m) = mutual.take() {
                        if std::env::var_os("YANG_LRR_PROBE").is_some() {
                            eprintln!(
                                "YANG_LRR_ACTION mutual t1={} t2={} nl={} nh={} \
                                 a={} c={} bl={} bh={}",
                                m.t1, m.t2, m.nl, m.nh, m.a, m.c, m.bl, m.bh
                            );
                        }
                        resolve_mutual_degenerate_pair(
                            mesh,
                            &mut attr_vec,
                            &mut side_labels.labels,
                            &m,
                        );
                        collapsed_any = true;
                        continue;
                    }
                    if any_degen {
                        // Degenerate triangles remain but none has a non-degenerate
                        // long-edge neighbour — genuine local-refinement territory.
                        if std::env::var_os("YANG_LRR_PROBE").is_some() {
                            let mut ndeg = 0usize;
                            for ti in 0..mesh.tris.len() {
                                if !is_degen(ti, mesh) {
                                    continue;
                                }
                                ndeg += 1;
                                let (a, c, b) = long_edge_off(&mesh.tris[ti], mesh);
                                let surf = attr_vec.get(ti).and_then(|o| o.as_ref()).map(|at| {
                                    let br = match at.input {
                                        InputId::A => brep_a,
                                        InputId::B => brep_b,
                                    };
                                    br.faces()[at.face as usize].surface
                                });
                                eprintln!("YANG_LRR_DEGEN_SURF tri={ti} surface={surf:?}");
                                let key = if a < c { (a, c) } else { (c, a) };
                                let inc = edge_tris.get(&key).map(|v| v.len()).unwrap_or(0);
                                let nbr_degen = edge_tris.get(&key).is_some_and(|v| {
                                    v.iter()
                                        .any(|&n| n as usize != ti && is_degen(n as usize, mesh))
                                });
                                eprintln!(
                                    "YANG_LRR_DEGEN tri={ti} verts={:?} long_edge=({a},{c}) off={b} \
                                     inc_count={inc} nbr_degen={nbr_degen} moved_a={} moved_c={} moved_b={} \
                                     pa={:?} pc={:?} pb={:?}",
                                    mesh.tris[ti],
                                    moved.contains(&a),
                                    moved.contains(&c),
                                    moved.contains(&b),
                                    mesh.verts[a as usize].as_array(),
                                    mesh.verts[c as usize].as_array(),
                                    mesh.verts[b as usize].as_array(),
                                );
                                // Mutual-pair anatomy: both incident triangles of the
                                // long edge are degenerate and report the SAME long
                                // edge (the quad-astride-one-edge configuration). For
                                // that configuration the candidate update is: drop
                                // both, insert bH into (bL,c) and bL into (a,bH) —
                                // so dump the off-vertex edge parameters plus the
                                // OUTER neighbours across those two insertion edges
                                // (their attribution decides the cross-face risk).
                                if let Some(incv) = edge_tris.get(&key) {
                                    if incv.len() == 2 {
                                        let n = if incv[0] as usize == ti {
                                            incv[1]
                                        } else {
                                            incv[0]
                                        } as usize;
                                        let (na, nc, nb) = long_edge_off(&mesh.tris[n], mesh);
                                        let nkey = if na < nc { (na, nc) } else { (nc, na) };
                                        if ti < n && is_degen(n, mesh) && nkey == key {
                                            let pa = mesh.verts[a as usize].as_array();
                                            let pc = mesh.verts[c as usize].as_array();
                                            let e = [pc[0] - pa[0], pc[1] - pa[1], pc[2] - pa[2]];
                                            let l2 = e[0] * e[0] + e[1] * e[1] + e[2] * e[2];
                                            let tof = |v: u32| {
                                                let p = mesh.verts[v as usize].as_array();
                                                ((p[0] - pa[0]) * e[0]
                                                    + (p[1] - pa[1]) * e[1]
                                                    + (p[2] - pa[2]) * e[2])
                                                    / l2
                                            };
                                            let (tb, tnb) = (tof(b), tof(nb));
                                            let (bl, bh) =
                                                if tb <= tnb { (b, nb) } else { (nb, b) };
                                            let attr_of = |t: usize| {
                                                attr_vec.get(t).and_then(|o| o.as_ref()).map(|at| {
                                                    (matches!(at.input, InputId::A), at.face)
                                                })
                                            };
                                            let probe_edge = |u: u32, v: u32| {
                                                let k = if u < v { (u, v) } else { (v, u) };
                                                match edge_tris.get(&k) {
                                                    Some(list) => {
                                                        let others: Vec<_> = list
                                                            .iter()
                                                            .map(|&x| x as usize)
                                                            .filter(|&x| x != ti && x != n)
                                                            .map(|x| {
                                                                (x, is_degen(x, mesh), attr_of(x))
                                                            })
                                                            .collect();
                                                        format!(
                                                            "inc={} others={others:?}",
                                                            list.len()
                                                        )
                                                    }
                                                    None => "missing".into(),
                                                }
                                            };
                                            eprintln!(
                                                "YANG_LRR_MUTUAL tri={ti} partner={n} \
                                                 long=({a},{c}) offs=({b}@{tb:.4},{nb}@{tnb:.4}) \
                                                 pair_attr=({:?},{:?}) \
                                                 edge_bLc[{}] edge_abH[{}]",
                                                attr_of(ti),
                                                attr_of(n),
                                                probe_edge(bl, c),
                                                probe_edge(a, bh),
                                            );
                                        }
                                    }
                                }
                            }
                            eprintln!("YANG_LRR_STOP site=degenerate_no_longedge ndeg={ndeg}");
                            // Grounding: for each attribution carrying a degenerate
                            // triangle, size the same-attribution tri set and count
                            // its boundary edges (undirected edges used exactly once).
                            let mut deg_attrs: std::collections::BTreeSet<(u8, u32)> =
                                std::collections::BTreeSet::new();
                            for ti in 0..mesh.tris.len() {
                                if is_degen(ti, mesh) {
                                    if let Some(at) = attr_vec.get(ti).and_then(|o| o.as_ref()) {
                                        let ik = matches!(at.input, InputId::A) as u8;
                                        deg_attrs.insert((ik, at.face));
                                    }
                                }
                            }
                            for (ik, face) in &deg_attrs {
                                let want = |ti: usize| {
                                    attr_vec.get(ti).and_then(|o| o.as_ref()).is_some_and(|at| {
                                        (matches!(at.input, InputId::A) as u8) == *ik
                                            && at.face == *face
                                    })
                                };
                                let patch_tris: Vec<u32> = (0..mesh.tris.len() as u32)
                                    .filter(|&t| want(t as usize))
                                    .collect();
                                let mut edge_ct: std::collections::HashMap<(u32, u32), u32> =
                                    std::collections::HashMap::new();
                                let mut ndeg_in = 0usize;
                                for &t in &patch_tris {
                                    if is_degen(t as usize, mesh) {
                                        ndeg_in += 1;
                                    }
                                    let tri = mesh.tris[t as usize];
                                    for (i, j) in [(0, 1), (1, 2), (2, 0)] {
                                        let (u, v) = (tri[i], tri[j]);
                                        let k = if u < v { (u, v) } else { (v, u) };
                                        *edge_ct.entry(k).or_insert(0) += 1;
                                    }
                                }
                                let bnd = edge_ct.values().filter(|&&c| c == 1).count();
                                let nonmanifold = edge_ct.values().filter(|&&c| c > 2).count();
                                // θ/z span of the patch in the cylinder frame (if
                                // this face is a Cylinder), to decide seam-wrap.
                                let br = if *ik == 1 { brep_a } else { brep_b };
                                let mut span_str = String::from("(not cylinder)");
                                if let Surface::Cylinder {
                                    axis_point,
                                    axis_dir,
                                    ..
                                } = br.faces()[*face as usize].surface
                                {
                                    let (e1, e2) = ortho_basis(axis_dir);
                                    let au = normalize3(axis_dir.as_array());
                                    let o = axis_point.as_array();
                                    let mut verts_set: std::collections::BTreeSet<u32> =
                                        std::collections::BTreeSet::new();
                                    for &t in &patch_tris {
                                        for &v in &mesh.tris[t as usize] {
                                            verts_set.insert(v);
                                        }
                                    }
                                    let th_ref = {
                                        let p = mesh.verts
                                            [*verts_set.iter().next().unwrap() as usize]
                                            .as_array();
                                        let r = [p[0] - o[0], p[1] - o[1], p[2] - o[2]];
                                        let x = r[0] * e1.x() + r[1] * e1.y() + r[2] * e1.z();
                                        let y = r[0] * e2.x() + r[1] * e2.y() + r[2] * e2.z();
                                        y.atan2(x)
                                    };
                                    let (mut th_lo, mut th_hi, mut z_lo, mut z_hi) = (
                                        f64::INFINITY,
                                        f64::NEG_INFINITY,
                                        f64::INFINITY,
                                        f64::NEG_INFINITY,
                                    );
                                    for &v in &verts_set {
                                        let p = mesh.verts[v as usize].as_array();
                                        let r = [p[0] - o[0], p[1] - o[1], p[2] - o[2]];
                                        let x = r[0] * e1.x() + r[1] * e1.y() + r[2] * e1.z();
                                        let y = r[0] * e2.x() + r[1] * e2.y() + r[2] * e2.z();
                                        let z = r[0] * au[0] + r[1] * au[1] + r[2] * au[2];
                                        // Unwrap θ near th_ref.
                                        let mut th = y.atan2(x) - th_ref;
                                        while th > std::f64::consts::PI {
                                            th -= 2.0 * std::f64::consts::PI;
                                        }
                                        while th < -std::f64::consts::PI {
                                            th += 2.0 * std::f64::consts::PI;
                                        }
                                        th_lo = th_lo.min(th);
                                        th_hi = th_hi.max(th);
                                        z_lo = z_lo.min(z);
                                        z_hi = z_hi.max(z);
                                    }
                                    span_str = format!(
                                        "theta_span={:.4} (pi={:.4}) z_span={:.4} nverts={}",
                                        th_hi - th_lo,
                                        std::f64::consts::PI,
                                        z_hi - z_lo,
                                        verts_set.len()
                                    );
                                }
                                eprintln!(
                                    "YANG_LRR_PATCH input={} face={face} n_tris={} n_degen={ndeg_in} \
                                     boundary_edges={bnd} nonmanifold_edges={nonmanifold} {span_str}",
                                    if *ik == 1 { "A" } else { "B" },
                                    patch_tris.len()
                                );
                            }
                        }
                        attribution.attributions = attr_vec;
                        return Err(YangError::stage4_region_invalid(
                            u32::MAX,
                            Stage4InvalidReason::LocalRefinementRequired,
                        ));
                    }
                    break; // no degenerate relocated triangles remain
                }
            };
            // Split N=[a,c,d] at b → [a,b,d] + [b,c,d], wound like N; drop D.
            if !seen_actions.insert((d_idx, n_idx, a, c, b)) {
                if std::env::var_os("YANG_LRR_PROBE").is_some() {
                    eprintln!(
                        "YANG_LRR_STOP site=split_cycle d={d_idx} n={n_idx} a={a} c={c} b={b} \
                         pass={passes}"
                    );
                }
                attribution.attributions = attr_vec;
                return Err(YangError::stage4_region_invalid(
                    u32::MAX,
                    Stage4InvalidReason::LocalRefinementRequired,
                ));
            }
            if std::env::var_os("YANG_LRR_PROBE").is_some() {
                // Shape census of the acted-on pair: D's absolute area, the
                // off-vertex height over the long edge, the long edge length,
                // and N's area — separates "collinear" (height ≪ edge) from
                // "merely small at model scale" (height ~ edge, area < floor).
                let pa = mesh.verts[a as usize].as_array();
                let pc = mesh.verts[c as usize].as_array();
                let pb = mesh.verts[b as usize].as_array();
                let ac = [pc[0] - pa[0], pc[1] - pa[1], pc[2] - pa[2]];
                let l_ac = (ac[0] * ac[0] + ac[1] * ac[1] + ac[2] * ac[2]).sqrt();
                let av = tri_area_vector(pa, pc, pb);
                let area_d = (av[0] * av[0] + av[1] * av[1] + av[2] * av[2]).sqrt() * 0.5;
                let height_b = if l_ac > 0.0 {
                    2.0 * area_d / l_ac
                } else {
                    f64::NAN
                };
                let nt = mesh.tris[n_idx];
                let avn = tri_area_vector(
                    mesh.verts[nt[0] as usize].as_array(),
                    mesh.verts[nt[1] as usize].as_array(),
                    mesh.verts[nt[2] as usize].as_array(),
                );
                let area_n = (avn[0] * avn[0] + avn[1] * avn[1] + avn[2] * avn[2]).sqrt() * 0.5;
                eprintln!(
                    "YANG_LRR_ACTION simple d={d_idx} n={n_idx} a={a} c={c} b={b} \
                     area_d={area_d:.3e} l_ac={l_ac:.3e} height_b={height_b:.3e} \
                     h_over_l={:.3e} area_n={area_n:.3e} band={DEGENERACY_IDENTITY_REL:.1e}",
                    if l_ac > 0.0 {
                        height_b / l_ac
                    } else {
                        f64::NAN
                    }
                );
            }
            // Combinatorial Fig-11(a) split (P0005): the halves inherit N's
            // winding, attribution, and side label by splicing `b` into N's
            // cyclic order between `a` and `c`; drop D and N.
            resolve_simple_degenerate_split(
                mesh,
                &mut attr_vec,
                &mut side_labels.labels,
                d_idx,
                n_idx,
                (a, c, b),
            );
            collapsed_any = true;
        }
        attribution.attributions = attr_vec;
        balance_census(mesh, "post-degen-loop");
    }

    // KV9-F3 diagnosis probe (read-only, env-gated): census near-twin mesh
    // vertex pairs at Stage-4 exit with their merge-eligibility context —
    // `moved` membership, shared-triangle adjacency, curve assignments.
    if std::env::var_os("YANG_S4_TWIN_PROBE").is_some() {
        let n = mesh.verts.len();
        let scale = mesh
            .verts
            .iter()
            .flat_map(|p| p.as_array())
            .fold(1.0_f64, |m, c| m.max(c.abs()));
        let band = 1.0e-9 * scale;
        for i in 0..n {
            for j in (i + 1)..n {
                let (p, q) = (mesh.verts[i].as_array(), mesh.verts[j].as_array());
                let d2 = (p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2);
                if d2 > band * band || d2 == 0.0 {
                    continue;
                }
                let (iu, ju) = (i as u32, j as u32);
                let shared_tri = mesh
                    .tris
                    .iter()
                    .position(|t| t.contains(&iu) && t.contains(&ju));
                eprintln!(
                    "[s4-twin-probe] verts {i}/{j} dist={:e} moved=({},{}) shared_tri={:?}\n  \
                     circle=({},{}) line=({},{}) ell=({},{}) junction=({},{})\n  \
                     {i}: ({},{},{})\n  {j}: ({},{},{})",
                    d2.sqrt(),
                    moved.contains(&iu),
                    moved.contains(&ju),
                    shared_tri,
                    vert_circle.contains_key(&iu),
                    vert_circle.contains_key(&ju),
                    vert_line.contains_key(&iu),
                    vert_line.contains_key(&ju),
                    vert_ellipse.contains_key(&iu),
                    vert_ellipse.contains_key(&ju),
                    vert_ell_junction.contains_key(&iu),
                    vert_ell_junction.contains_key(&ju),
                    p[0],
                    p[1],
                    p[2],
                    q[0],
                    q[1],
                    q[2]
                );
            }
        }
    }

    star_probe("before-validate", mesh, attribution);
    coincident_vertex_census("before-validate", mesh);
    nonmanifold_edge_census("before-validate", mesh, attribution);
    // (4) Validate every RELOCATED triangle (one touching a moved vertex) for
    // non-degeneracy (Yang §4.5 step 4). Reversed intersections are handled by
    // the §4.5.3 sweep above; watertightness by the global gate below (§4.4.3).
    validate_relocated_triangles(mesh, attribution, &moved)?;
    // (4a1) Doubled-membrane removal (spec `yang_doubled_membrane_removal.md`,
    // task #146 χ=3 sub-layer): drop opposite-winding coincident-triangle fins
    // (a zero-volume artifact of a backtrack-spike / near-tangent junction)
    // BEFORE the shell gate reads χ. Volume- and edge-balance-preserving; it
    // leaves the spur apex dangling for `compact_unreferenced_verts`, so it
    // rides the same Phase-A recompute path as a §4.5.3 collapse. The
    // attribution vector is filtered in lockstep with the triangles (spec
    // I8 — R0051's Stage-6 nonplanar STOP was this pass dropping triangles
    // from the mesh alone).
    let membranes_removed = {
        let mut attr_vec = std::mem::take(&mut attribution.attributions);
        let n = remove_doubled_membranes_scoped(
            mesh,
            &mut attr_vec,
            &mut side_labels.labels,
            full_arrangement,
        );
        attribution.attributions = attr_vec;
        n
    };
    if membranes_removed > 0 {
        collapsed_any = true;
    }
    // (4a2) Tangency pinch-vertex split (spec `yang_tangency_pinch_split.md`):
    // uniform per-sheet representation of self-touching union boundaries
    // BEFORE the shell gate reads χ. Splitting appends vertices (a topology
    // change), so it rides the same Phase-A recompute path as a §4.5.3
    // collapse via the returned flag.
    // The EDGE-pinch arm belongs at Stage-4 ENTRY only (spec §0a.3 / §0b
    // "The placement"), where the over-2 edges are the ARRANGEMENT's. Armed
    // here too it reads the §0a certificate on a mesh Stage 4 has already
    // collapsed — P0020's two 4-valent edges are minted by §4.5.3, not handed
    // over (deviation N78), so this late site runs the VERTEX-fan arm only
    // (C0058's tangent points); entry-only since 2026-10-03, its
    // `YANG_EDGE_PINCH_ENTRY_ONLY` opt-out deleted 2026-10-16.
    // Under I3 (`full_arrangement == true`), pinch splitting runs post-keep on
    // the classified submesh (`specs/yang_442_classify_after_update.md` §4).
    let pinch_splits = if full_arrangement {
        0
    } else {
        split_pinch_vertices(
            mesh,
            &mut relocations,
            &attribution.attributions,
            &side_labels.labels,
            false,
        )
    };
    if pinch_splits > 0 {
        collapsed_any = true;
    }
    coincident_vertex_census("after-4a2-pinch-split", mesh);
    if std::env::var_os("YANG_EDGE_PINCH_PROBE").is_some() {
        eprintln!("[edge-pinch] 4a2 split {pinch_splits} vertex copies");
        nonmanifold_edge_census("after-4a2", mesh, attribution);
    }
    // (4b') #169 Phase-0 failure-region probe: before the gate fires, report the
    // non-manifold seam regions + their patch pairs + whether each patch has a
    // SurfaceChart (Plane/Cylinder) — the §4.4.1 mesh-update worklist. Gated on
    // `YANG_MESHUP_REGION`, so byte-identical when unset (no production change).
    if std::env::var_os("YANG_MESHUP_REGION").is_some() {
        let regions = crate::stage4_project::detect_nonmanifold_seams(&mesh.tris, &|ti| {
            attribution
                .lookup(ti as u32)
                .map(|at| (matches!(at.input, InputId::A), at.face))
        });
        for r in &regions {
            eprintln!(
                "YANG_MESHUP_REGION n_edges={} keys={:?} edges={:?}",
                r.edges.len(),
                r.keys,
                r.edges
            );
            for &(is_a, face) in &r.keys {
                let br = if is_a { brep_a } else { brep_b };
                let surf = br.faces()[face as usize].surface;
                eprintln!(
                    "  key ({is_a},{face}) surface={surf:?} has_chart={}",
                    crate::stage4_project::SurfaceChart::new(surf).is_some()
                );
            }
            // Local topology dump: for each region vertex, its coords + every
            // incident triangle (verts + attribution) — reveals whether the
            // mismatch is a shared-seam subdivision, a T-junction, or a floating
            // triangle, so the mesh-update operation can be chosen correctly.
            let rverts: std::collections::BTreeSet<u32> =
                r.edges.iter().flat_map(|&(s, e)| [s, e]).collect();
            for &v in &rverts {
                eprintln!("  v{v} = {:?}", mesh.verts[v as usize]);
            }
            for ti in 0..mesh.tris.len() {
                let t = mesh.tris[ti];
                if t.iter().any(|v| rverts.contains(v)) {
                    let k = attribution
                        .lookup(ti as u32)
                        .map(|at| (matches!(at.input, InputId::A), at.face));
                    eprintln!("  tri{ti} {t:?} attr={k:?}");
                }
            }
        }
    }
    // #194 (spec `yang_194_subtauwork_edge_collapse`): collapse mesh edges
    // below WORKING precision BEFORE the watertightness gate — the F0082
    // Extrude-12 operand-self-graze twin (same junction minted twice with
    // swapped LPI roles, 5.5e-14 apart, edge-connected; its zero-area flap's
    // third edge use is the χ=3 book edge THIS gate stops on). Byte-identical
    // no-op when no such edge exists; `collapsed_any` routes the caller into
    // the standard compact + Phase-A recompute.
    {
        let mut attr_vec = std::mem::take(&mut attribution.attributions);
        let c = collapse_subtauwork_mesh_edges_ex(
            mesh,
            &mut attr_vec,
            &mut side_labels.labels,
            full_arrangement,
            Some(tangential_verts),
        );
        attribution.attributions = attr_vec;
        collapsed_any |= c;
    }

    // (4b) Explicit Stage-4 watertightness gate (§4.4.3).
    // Under I3 (`full_arrangement == true`), global half-edge balance passes
    // vacuously across 2-forward / 2-reverse arrangement seams and opposite
    // operand holes, so M_A and M_B are checked independently as closed
    // 2-manifolds (`specs/yang_442_classify_after_update.md` §4).
    let gate_res = if full_arrangement {
        check_watertight_2manifold_per_operand(mesh, attribution, side_labels, true)
    } else {
        check_watertight_2manifold(mesh)
    };
    if let Err(gate_err) = gate_res {
        // §4.5 trigger T4 (`boolean::failures`, spec
        // `yang_452_refinement_loop.md` §3): every non-manifold seam region
        // names the faces at its unpaired edges. Record-only (`record_t4`
        // inside `check_watertight_2manifold_per_operand` already recorded on
        // the failing operand submesh when `full_arrangement == true`).
        if !full_arrangement {
            crate::boolean::failures::record_nonmanifold_mesh(mesh, attribution);
        }
        // #195 probe-only forensics: attribute every double-cover-edge triangle
        // to its input B-Rep face (operand + face id + surface) so the
        // self-overlap self-localizes to the producing emission. Byte-identical
        // when the probe env is unset (the gate error is returned unchanged).
        if std::env::var("NONMANIFOLD_SITE_PROBE").is_ok() {
            let mut dir: std::collections::BTreeMap<(u32, u32), i32> =
                std::collections::BTreeMap::new();
            for tri in &mesh.tris {
                for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
                    *dir.entry((tri[i], tri[j])).or_insert(0) += 1;
                }
            }
            for (&(s, e), &fwd) in &dir {
                // Report an undirected edge once (canonical `s < e`) when it is
                // either double-covered (`fwd >= 2`) or UNBALANCED (`fwd != rev`,
                // the gate's own failure condition — the doubling may sit on the
                // reverse direction, which the old `s < e && fwd >= 2` form
                // silently skipped).
                let rev = dir.get(&(e, s)).copied().unwrap_or(0);
                if s < e && (fwd >= 2 || rev >= 2 || fwd != rev) {
                    eprintln!(
                        "NONMANIFOLD_SITE_PROBE s4-dc-attr edge ({s},{e}) fwd={fwd} rev={rev} \
                         p{s}={:?} p{e}={:?}",
                        mesh.verts[s as usize].as_array(),
                        mesh.verts[e as usize].as_array()
                    );
                    for (ti, tri) in mesh.tris.iter().enumerate() {
                        let uses = tri.contains(&s) && tri.contains(&e);
                        if uses {
                            let attr = attribution.lookup(ti as u32);
                            let surf = attr.map(|at| {
                                let br = if matches!(at.input, InputId::A) {
                                    brep_a
                                } else {
                                    brep_b
                                };
                                (at.input, at.face, br.faces()[at.face as usize].surface)
                            });
                            // Direction this triangle presents the edge in, plus
                            // its off-vertex position — two pages of a book edge
                            // whose off-vertices COINCIDE are a duplicated sheet
                            // (the #146 upstream class), which the ids alone hide.
                            let dirn = if tri
                                .iter()
                                .zip([tri[1], tri[2], tri[0]])
                                .any(|(&u, v)| u == s && v == e)
                            {
                                "fwd"
                            } else {
                                "rev"
                            };
                            let off = tri.iter().copied().find(|&v| v != s && v != e);
                            let offp = off.map(|v| mesh.verts[v as usize].as_array());
                            eprintln!(
                                "NONMANIFOLD_SITE_PROBE s4-dc-attr   tri {ti}: {tri:?} {dirn} \
                                 off={off:?} offp={offp:?} attr={surf:?}"
                            );
                        }
                    }
                }
            }
        }
        return Err(gate_err);
    }

    // §4.4.1 boundary-curve relocation (spec `yang_s4_boundary_curve_relocation.md`,
    // inc-2). Yang Fig. 11 requires the trimmed triangulation to "map boundary
    // curves to boundary curves", which includes an operand's OWN rim — the
    // case `build_intersection_curves` never claims (`input0 == input1`). Runs
    // LAST so every cross-input junction is already seated and can be excluded
    // by construction.
    //
    // ALWAYS-ON since inc-5 (was `YANG_S4_RIM_SNAP_ENABLE`); flipped together
    // with the §4.5.4 rim×plane graze refinement in `boolean`, which depends on
    // it — see that function's note for the corpus measurement.
    //
    // Phase A is recomputed here rather than reusing `inc0`/`curves0`: the mesh
    // has been relocated and possibly collapsed since, so the earlier maps can
    // reference stale vertices.
    {
        let (_infos_bc, inc_bc, curves_bc) = compute_phase_a_with_side_labels(
            mesh,
            attribution,
            full_arrangement.then_some(&*side_labels),
            brep_a,
            brep_b,
            &crate::stage3_ssi::NO_EDGE_PROVENANCE,
        )?;
        let rim_curves = crate::stage4_boundary_curve::collect_rim_curves(&inc_bc);
        // Per-vertex exclusion diagnosis: `YANG_S4_RIM_SNAP_TARGET=x,y,z,r`
        // reports, for every mesh vertex within `r` of the given point, each
        // incident incidence edge and WHICH of the pass's filters dropped it.
        // The pass claiming rim edges but moving nothing says nothing about
        // WHY; this does.
        if let Ok(spec) = std::env::var("YANG_S4_RIM_SNAP_TARGET") {
            let f: Vec<f64> = spec
                .split(',')
                .filter_map(|s| s.trim().parse().ok())
                .collect();
            if f.len() == 4 {
                let (tx, ty, tz, tr) = (f[0], f[1], f[2], f[3]);
                let mut cross: std::collections::BTreeSet<u32> = Default::default();
                for &(s, e) in curves_bc.keys() {
                    cross.insert(s);
                    cross.insert(e);
                }
                for (vi, p) in mesh.verts.iter().enumerate() {
                    let pa = p.as_array();
                    let d =
                        ((pa[0] - tx).powi(2) + (pa[1] - ty).powi(2) + (pa[2] - tz).powi(2)).sqrt();
                    if d > tr {
                        continue;
                    }
                    let v = vi as u32;
                    eprintln!(
                        "[rim-target] v={v} dist_to_target={d:.6e} p={pa:?} \
                         cross_excluded={}",
                        cross.contains(&v)
                    );
                    // Which surfaces does this vertex ACTUALLY satisfy? A
                    // triple point must satisfy all three; the implicit value
                    // separates "on it" from "near it".
                    {
                        let mut surfs: Vec<(InputId, Surface)> = Vec::new();
                        for (&(s2, e2), entries) in &inc_bc {
                            if s2 != v && e2 != v {
                                continue;
                            }
                            for &(i, sf) in entries {
                                if !surfs.iter().any(|(i2, s3)| *i2 == i && *s3 == sf) {
                                    surfs.push((i, sf));
                                }
                            }
                        }
                        for (i, sf) in surfs {
                            let f = surface_value_and_normal(sf, p.as_array()).map(|(f, _)| f);
                            eprintln!(
                                "[rim-target]   SURFACE {i:?}:{} implicit_value={f:?}",
                                surface_kind_name(sf)
                            );
                        }
                    }
                    let mut seen_any = false;
                    for (&(s, e), entries) in &inc_bc {
                        if s != v && e != v {
                            continue;
                        }
                        seen_any = true;
                        let kinds: Vec<String> = entries
                            .iter()
                            .map(|(i, sf)| format!("{i:?}:{}", surface_kind_name(*sf)))
                            .collect();
                        let same_input = entries.len() == 2 && entries[0].0 == entries[1].0;
                        let diff_surf = entries.len() == 2 && entries[0].1 != entries[1].1;
                        let circle = if entries.len() == 2 {
                            crate::stage4_boundary_curve::rim_circle_from_pair(
                                entries[0].1,
                                entries[1].1,
                            )
                        } else {
                            None
                        };
                        let claimed = rim_curves.contains_key(&(s, e));
                        eprintln!(
                            "[rim-target]   edge=({s},{e}) entries={kinds:?} \
                             same_input={same_input} diff_surf={diff_surf} \
                             circle={} claimed={claimed}",
                            circle.is_some()
                        );
                        if let Some(c) = circle {
                            // Per-edge self-derived bound: this chord's OWN
                            // sagitta, r*(1-cos(dtheta/2)) over its endpoints'
                            // angular span — the guarantee Stage 1 makes for
                            // THIS chord, not a global aggregate over the
                            // owner's rims.
                            if let (Some(&p0), Some(&p1)) =
                                (mesh.verts.get(s as usize), mesh.verts.get(e as usize))
                            {
                                if let Curve::Circle {
                                    center,
                                    normal,
                                    radius,
                                } = c
                                {
                                    let cc = center.as_array();
                                    let nn = normal.as_array();
                                    let radial = |p: Point3| {
                                        let a = p.as_array();
                                        let d = [a[0] - cc[0], a[1] - cc[1], a[2] - cc[2]];
                                        let h = d[0] * nn[0] + d[1] * nn[1] + d[2] * nn[2];
                                        [d[0] - h * nn[0], d[1] - h * nn[1], d[2] - h * nn[2]]
                                    };
                                    let (r0, r1) = (radial(p0), radial(p1));
                                    let n0 = (r0[0] * r0[0] + r0[1] * r0[1] + r0[2] * r0[2]).sqrt();
                                    let n1 = (r1[0] * r1[0] + r1[1] * r1[1] + r1[2] * r1[2]).sqrt();
                                    let cosang = ((r0[0] * r1[0] + r0[1] * r1[1] + r0[2] * r1[2])
                                        / (n0 * n1))
                                        .clamp(-1.0, 1.0);
                                    let span = cosang.acos();
                                    let sagitta = radius * (1.0 - (span / 2.0).cos());
                                    eprintln!(
                                        "[rim-target]     chord span={:.6}deg own_sagitta={sagitta:.6e}                                          global_bound={:.6e}",
                                        span.to_degrees(),
                                        bound_probe(brep_a, brep_b)
                                    );
                                }
                            }
                            for w in [s, e] {
                                if let Some(&wp) = mesh.verts.get(w as usize) {
                                    let proj =
                                        crate::stage4_boundary_curve::project_onto_curve(wp, &c);
                                    let dd = proj.map(|q| {
                                        let (x, y) = (wp.as_array(), q.as_array());
                                        ((x[0] - y[0]).powi(2)
                                            + (x[1] - y[1]).powi(2)
                                            + (x[2] - y[2]).powi(2))
                                        .sqrt()
                                    });
                                    eprintln!(
                                        "[rim-target]     endpoint v={w} resid={dd:?} \
                                         in_band={:?}",
                                        dd.map(|x| x <= bound_probe(brep_a, brep_b))
                                    );
                                }
                            }
                        }
                    }
                    if !seen_any {
                        eprintln!("[rim-target]   NO incidence edge contains this vertex");
                    }
                }
            }
        }
        // §4.3.3 Case-IV corner-phantom census (spec
        // `specs/yang_433_case_iv_corner_phantom.md` inc-0): read-only,
        // print-only, gated. Runs at this postcondition because `inc_bc` is
        // the recomputed POST-relocation incidence — the vantage where the
        // paper's "no solution in the parametric domains" clause is testable.
        if std::env::var_os("YANG_433_PHANTOM").is_some() {
            crate::stage4_phantom::census_case_iv_phantom(mesh, brep_a, brep_b, &inc_bc);
        }
        // §4.3.3 Case-IV RULE-OUT (spec inc-2): a CLOSED loop of A×B edges whose every corner is a refuted
        // pierce claim is an intersection the surfaces do not have (the
        // paper's "no solution in one of the two parametric domains ⇒ rule
        // out Case IV", `refs/text:518-537`). Its relocated corners are
        // virtual, so no downstream stage can consume the loop; the typed
        // STOP names the site and carries the §4.5.2 under-resolution
        // certificate for the op-level refinement ladder (`boolean`), which
        // is the paper's remedy for a detected local error (`:659-670`).
        // Before inc-2 the loop rode into Stage 6 and surfaced as a misnamed
        // kernel-v2 CDT ring reject (R0100 face 15).
        if let Some(cert) =
            crate::stage4_phantom::certify_phantom_loops(brep_a, brep_b, &inc_bc, &mesh.verts)
        {
            let v = cert.loop_vertices[0];
            return Err(YangError::stage4_region_invalid_under_resolved(
                v,
                Stage4InvalidReason::PhantomIntersectionLoop,
                cert.under_resolution,
            ));
        }
        if !rim_curves.is_empty() {
            // Vertices claimed by a CROSS-input curve are A×B junctions that
            // must lie on BOTH curves; moving one would break that.
            let mut cross_endpoints: std::collections::BTreeSet<u32> = Default::default();
            for &(s, e) in curves_bc.keys() {
                cross_endpoints.insert(s);
                cross_endpoints.insert(e);
            }
            // The bound is the owner's own Stage-1 chord guarantee. Both
            // operands' rims are candidates, so take the larger of the two
            // budgets — a vertex beyond even that is not this class and STOPs.
            let bound = [InputId::A, InputId::B]
                .into_iter()
                .filter_map(|i| {
                    crate::stage3_ssi::chord_tol_for_curved_owner(i, brep_a, brep_b, 0, (0, 0)).ok()
                })
                .fold(0.0f64, f64::max);
            if bound > 0.0 {
                let moves = crate::stage4_boundary_curve::plan_boundary_relocations(
                    mesh,
                    &rim_curves,
                    &inc_bc,
                    &cross_endpoints,
                    bound,
                );
                // Census (spec §19): which of these snaps move a vertex that
                // carries a surface the projection never consumed? Read-only,
                // and taken BEFORE the apply so `mesh` still holds the pre-snap
                // positions.
                if std::env::var_os("YANG_S4_UNCONSUMED_PROBE").is_some() {
                    crate::stage4_boundary_curve::census_unconsumed_surfaces(
                        mesh,
                        &moves,
                        &inc_bc,
                        &rim_curves,
                    );
                }
                let n = crate::stage4_boundary_curve::apply_boundary_relocations(mesh, &moves);
                if std::env::var_os("YANG_S4_RIM_SNAP_PROBE").is_some() {
                    eprintln!(
                        "[s4-rim-snap] rim_edges={} cross_excluded={} bound={bound:.6e} moved={n}",
                        rim_curves.len(),
                        cross_endpoints.len()
                    );
                    for (v, q) in &moves {
                        eprintln!("[s4-rim-snap]   v={v} -> {:?}", q.as_array());
                    }
                }
            }
        }
    }

    // inc-2c-3b-1 (spec `specs/yang_451_corner_transit.md` §3i): the gated
    // corner-transit corridor mutation — repairs the §4-I9 corner-crosser
    // fires when every certificate holds; any refusal leaves the mesh
    // untouched and the postcondition STOP below fires exactly as today.
    s4_lap("before-transit", mesh);
    corner_transit_apply(
        mesh,
        attribution,
        side_labels,
        brep_a,
        brep_b,
        &s4_entry_pos,
        full_arrangement,
    )?;
    s4_lap("transit", mesh);
    // After a collapse the vertex set may have lost some relocated verts; keep
    // only relocations whose vertex still carries a conic output edge. The
    // caller resolves the output-edge index; relocations referencing a
    // now-absent vertex are simply not emitted (the caller guards the index).
    relocation_domain_postcondition(mesh, attribution, brep_a, brep_b, &s4_entry_pos)?;
    s4_lap("relocation-domain", mesh);
    // Yang §4.5 boundary-point domain certificate (spec
    // `specs/yang_45_boundary_point_domain_certificate.md`): a relocated
    // model-edge crossing whose exact root left its face across a crease is
    // §4.5.2's own trigger — raised here, typed, instead of surfacing as a
    // stray output edge the render tessellator declines (P0003).
    boundary_domain_postcondition(mesh, attribution, brep_a, brep_b, &s4_entry_pos)?;
    s4_lap("boundary-domain", mesh);

    Ok((relocations, collapsed_any))
}

mod reversal;
pub(crate) use reversal::*;

mod validate;
pub(crate) use validate::*;

mod collapse;
pub(crate) use collapse::*;
mod corner_transit;
pub(crate) use corner_transit::*;
mod domain_census;
pub(crate) use domain_census::*;
mod domain_postcondition;
pub(crate) use domain_postcondition::*;
mod probes;
pub(crate) use probes::*;
mod s451;
pub(crate) use s451::*;
