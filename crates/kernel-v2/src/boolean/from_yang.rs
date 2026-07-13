//! yang-rs *output* `BRep` → kernel-v2 solid reassembly (PR-KV5b vocabulary):
//! the direct arena assembler, edge classification, and the surface/curve key
//! machinery. Move-only split from the boolean god-module (design review
//! 2026-07-12 F9); byte-identical. See `super`'s module docs for the reassembly
//! strategy and the circle-edge sense derivation.

use super::*;

mod classify;
mod keys;
use classify::*;
use keys::*;

/// One validated loop of the yang output: owning yang face, kind, the
/// vertex cycle in walk order (yang vertex indices), and the per-edge
/// curve classification.
struct LoopSpec {
    face: usize,
    kind: LoopKind,
    cycle: Vec<u32>,
    edges: Vec<EdgeKind>,
}

/// Surface classification of a yang output face.
enum FaceSurf {
    Plane {
        normal: [f64; 3],
    },
    Cylinder {
        axis_point: Point3,
        axis_dir: [f64; 3],
        radius: f64,
        reversed: bool,
    },
    Cone {
        apex: Point3,
        axis_dir: [f64; 3],
        half_angle: f64,
        reversed: bool,
    },
    Torus {
        center: Point3,
        axis_dir: [f64; 3],
        major_radius: f64,
        minor_radius: f64,
        reversed: bool,
    },
    Sphere {
        center: Point3,
        radius: f64,
        reversed: bool,
    },
}

/// Reassemble a yang-rs *output* `BRep` into a kernel-v2 solid.
///
/// PR-KV5b vocabulary (see module docs): planar faces with polygonal /
/// arc-bearing loops or single full-circle caps; cylinder faces — the
/// canonical full lateral or partial patches with arc/segment boundary
/// loops, including `reversed` cavity walls. The output is validated
/// structurally BEFORE the first arena mutation (loop continuity and
/// closure, twin pairing with curve agreement, orientable planar Newell
/// normals, named-curve vocabulary walls), assembled directly into the
/// arena (see module docs for why a direct assembler rather than an Euler
/// sequence), split into connected shells with per-shell genus derived
/// from the Euler–Poincaré formula, and then re-checked by the full
/// [`crate::validate::validate_solid`] — whose curved orientation analysis
/// (unrolled winding, wrap pairing) is the production gate for the curved
/// faces assembled here.
pub fn from_yang_brep(
    arena: &mut BrepArena,
    brep: &yang_rs::BRep,
) -> Result<SolidId, KernelV2Error> {
    Ok(from_yang_brep_indexed(arena, brep)?.0)
}

/// [`from_yang_brep`] plus the **yang-output-face-index → kernel `FaceId`**
/// mapping (`None` where a yang face produced no kernel face). KV13 F2 uses it
/// to attach the boolean's per-output-face attribution to the output faces'
/// persistent ids.
pub fn from_yang_brep_indexed(
    arena: &mut BrepArena,
    brep: &yang_rs::BRep,
) -> Result<(SolidId, Vec<Option<FaceId>>), KernelV2Error> {
    // PR-KV7: recover B-Rep granularity (output curve tagging) before
    // classification — chord runs on recovered exact circles become arcs /
    // full rims, canonical-pairable cylinder faces become the 4-edge
    // [rim, seam, rim, seam] form. Conservative: bails to the original
    // lists on any structural anomaly, so pass-1 below stays the single
    // validation authority.
    let (rverts, redges, rfaces) = crate::recover::recover_output_curves(brep);
    let yverts: &[yang_rs::BRepVertex] = &rverts;
    let yedges: &[yang_rs::BRepEdge] = &redges;
    let yfaces: &[yang_rs::BRepFace] = &rfaces;

    // ---- pass 1 (NO arena mutation): validate the yang structure ---------
    if yfaces.is_empty() {
        return Err(KernelV2Error::EmptyBooleanResult);
    }

    // 1a. Surface vocabulary. Planar output faces never carry `reversed`
    //     (sense belongs in the plane normal); cylinder faces may.
    let mut surfs: Vec<FaceSurf> = Vec::with_capacity(yfaces.len());
    for f in yfaces.iter() {
        match f.surface {
            yang_rs::Surface::Plane { normal, .. } => {
                if f.reversed {
                    return Err(KernelV2Error::InvalidBooleanOutput(
                        "planar output face with reversed = true (sense belongs in the plane normal)",
                    ));
                }
                let n = normal.as_array();
                if (norm3(n) - 1.0).abs() > YANG_NORMAL_AGREEMENT_TOLERANCE {
                    return Err(KernelV2Error::InvalidBooleanOutput(
                        "output face plane normal is not unit-length",
                    ));
                }
                surfs.push(FaceSurf::Plane { normal: n });
            }
            yang_rs::Surface::Cylinder {
                axis_point,
                axis_dir,
                radius,
            } => {
                let a = axis_dir.as_array();
                if (norm3(a) - 1.0).abs() > YANG_NORMAL_AGREEMENT_TOLERANCE {
                    return Err(KernelV2Error::InvalidBooleanOutput(
                        "output cylinder axis_dir is not unit-length",
                    ));
                }
                if !(radius.is_finite() && radius > 0.0) {
                    return Err(KernelV2Error::InvalidBooleanOutput(
                        "output cylinder radius is not finite and positive",
                    ));
                }
                surfs.push(FaceSurf::Cylinder {
                    axis_point,
                    axis_dir: a,
                    radius,
                    reversed: f.reversed,
                });
            }
            yang_rs::Surface::Cone {
                apex,
                axis_dir,
                half_angle,
            } => {
                let a = axis_dir.as_array();
                if (norm3(a) - 1.0).abs() > YANG_NORMAL_AGREEMENT_TOLERANCE {
                    return Err(KernelV2Error::InvalidBooleanOutput(
                        "output cone axis_dir is not unit-length",
                    ));
                }
                if !(half_angle.is_finite()
                    && half_angle > 0.0
                    && half_angle < std::f64::consts::FRAC_PI_2)
                {
                    return Err(KernelV2Error::InvalidBooleanOutput(
                        "output cone half_angle is not in (0, π/2)",
                    ));
                }
                surfs.push(FaceSurf::Cone {
                    apex,
                    axis_dir: a,
                    half_angle,
                    reversed: f.reversed,
                });
            }
            yang_rs::Surface::Torus {
                center,
                axis_dir,
                major_radius,
                minor_radius,
            } => {
                let a = axis_dir.as_array();
                if (norm3(a) - 1.0).abs() > YANG_NORMAL_AGREEMENT_TOLERANCE {
                    return Err(KernelV2Error::InvalidBooleanOutput(
                        "output torus axis_dir is not unit-length",
                    ));
                }
                if !(major_radius.is_finite() && major_radius > 0.0) {
                    return Err(KernelV2Error::InvalidBooleanOutput(
                        "output torus major_radius is not finite and positive",
                    ));
                }
                if !(minor_radius.is_finite() && minor_radius > 0.0 && minor_radius < major_radius)
                {
                    return Err(KernelV2Error::InvalidBooleanOutput(
                        "output torus minor_radius is not finite, positive, and below the major radius",
                    ));
                }
                surfs.push(FaceSurf::Torus {
                    center,
                    axis_dir: a,
                    major_radius,
                    minor_radius,
                    reversed: f.reversed,
                });
            }
            yang_rs::Surface::Sphere { center, radius } => {
                if !(radius.is_finite() && radius > 0.0) {
                    return Err(KernelV2Error::InvalidBooleanOutput(
                        "output sphere radius is not finite and positive",
                    ));
                }
                surfs.push(FaceSurf::Sphere {
                    center,
                    radius,
                    reversed: f.reversed,
                });
            }
        }
    }

    // 1b. Loops: vocabulary, continuity, closure, per-edge classification.
    let mut loops: Vec<LoopSpec> = Vec::new();
    for (fi, f) in yfaces.iter().enumerate() {
        for (li, loop_edges) in std::iter::once(&f.outer_loop)
            .chain(f.inner_loops.iter())
            .enumerate()
        {
            if loop_edges.is_empty() {
                return Err(KernelV2Error::InvalidBooleanOutput("empty output loop"));
            }
            // Walk the loop, inferring each edge's traversal direction by
            // chaining: yang OUTPUT loops are directed-continuous
            // (`e.end == next.start`), but the canonical M5 INPUT shape
            // (the round-trip) reuses one shared seam edge in BOTH
            // directions within the lateral loop, so an edge may be
            // traversed against its stored (start, end).
            let mut cycle = Vec::with_capacity(loop_edges.len());
            let mut kinds = Vec::with_capacity(loop_edges.len());
            let mut has_full = false;
            let mut cur: Option<u32> = None;
            for &ei in loop_edges.iter() {
                let Some(e) = yedges.get(ei as usize) else {
                    return Err(KernelV2Error::InvalidBooleanOutput(
                        "output loop references an out-of-range edge",
                    ));
                };
                if (e.start as usize) >= yverts.len() || (e.end as usize) >= yverts.len() {
                    return Err(KernelV2Error::InvalidBooleanOutput(
                        "output edge references an out-of-range vertex",
                    ));
                }
                // The first edge is taken as stored; later edges chain off
                // the current exit vertex.
                let (from, to) = match cur {
                    None => (e.start, e.end),
                    Some(c) if e.start == c => (e.start, e.end),
                    Some(c) if e.end == c => (e.end, e.start),
                    Some(_) => {
                        return Err(KernelV2Error::InvalidBooleanOutput(
                            "output loop is not edge-continuous",
                        ));
                    }
                };
                let kind = classify_edge(e, yverts, from, to)?;
                if matches!(kind, EdgeKind::Full { .. }) {
                    has_full = true;
                }
                cycle.push(from);
                kinds.push(kind);
                cur = Some(to);
            }
            if cur != Some(cycle[0]) {
                return Err(KernelV2Error::InvalidBooleanOutput(
                    "output loop does not close",
                ));
            }
            if !has_full && cycle.len() < 3 {
                // KV9-F3 (spec `kv9_f3_output_vertex_identity` §2 amendment,
                // E-V5): a TWO-edge loop whose edges are conic arcs on
                // DISTINCT curves is a genuine LENS BIGON — e.g. the
                // parallel cyl×cyl bite's cap, bounded by one arc of each
                // cylinder's section circle meeting at the two ruling
                // points. The femto-twin artifact used to subdivide these
                // loops spuriously; with output identity fixed they arrive
                // as true bigons, which the CurveKey manifold pairing (the
                // M8 disc∩disc lens machinery) supports downstream. Two
                // edges on the SAME curve (or any line segment) remain a
                // degenerate reject.
                let lens_bigon = cycle.len() == 2
                    && kinds.iter().all(|k| !matches!(k, EdgeKind::Seg))
                    && curve_key(&kinds[0]) != curve_key(&kinds[1]);
                // Spec kv9_f3 §4 row E-V6 (ERROR-census campaign 4): a
                // 2-edge loop with exactly ONE `Seg` and one conic arc is a
                // genuine D-FACE — a circular/elliptic SEGMENT bounded by a
                // chord and its arc (R0046's plane∩cylinder cap fragment).
                // `classify_edge` already validated the arc's endpoints on
                // its conic; the chord shares those vertices by loop
                // closure. Two `Seg`s (a zero-area double edge) and
                // same-curve arc pairs remain the loud reject.
                let dface_bigon = cycle.len() == 2
                    && kinds.iter().filter(|k| matches!(k, EdgeKind::Seg)).count() == 1;
                if !(lens_bigon || dface_bigon) {
                    // KV9-F3 diagnosis probe (read-only, env-gated).
                    if std::env::var_os("KV2_OUT_TWIN_PROBE").is_some() {
                        eprintln!(
                            "[out-loop-probe] face {fi} loop {li} degenerate: \
                             edges {loop_edges:?} cycle {cycle:?} kinds {kinds:?}"
                        );
                        for &v in &cycle {
                            let p = yverts[v as usize].point.as_array();
                            eprintln!("    v{v}: ({},{},{})", p[0], p[1], p[2]);
                        }
                    }
                    return Err(KernelV2Error::InvalidBooleanOutput(
                        "output loop with fewer than 3 edges and no full-circle edge",
                    ));
                }
            }
            if has_full && cycle.len() != 1 && cycle.len() != 4 {
                // Full circles occur only in the canonical vocabulary: a
                // 1-edge cap loop or the 4-edge [rim, seam, rim, seam]
                // lateral.
                return Err(KernelV2Error::InvalidBooleanOutput(
                    "full-circle edge in a non-canonical loop",
                ));
            }
            loops.push(LoopSpec {
                face: fi,
                kind: if li == 0 {
                    LoopKind::Outer
                } else {
                    LoopKind::Inner
                },
                cycle,
                edges: kinds,
            });
        }
    }

    // 1c. Manifold edge pairing: every undirected vertex pair is used by
    //     exactly two directed loop edges with consistent curve geometry —
    //     opposite directions for ordinary edges; full-circle self-pairs
    //     get their per-use directional normal DERIVED here (see module
    //     docs, "Circle-edge sense derivation").
    struct EdgeUse {
        loop_idx: usize,
        pos: usize,
        forward: bool, // a < b for ordinary edges; unused for self-pairs
    }
    // Keyed by (undirected vertex pair, undirected curve identity) so a LENS
    // bigon (two arcs on different circles sharing both endpoints) pairs each
    // arc separately instead of collapsing to a spurious 4-use "non-manifold"
    // edge (M8 disc∩disc crossing).
    let mut pair_uses: BTreeMap<(u32, u32, CurveKey), Vec<EdgeUse>> = BTreeMap::new();
    for (si, spec) in loops.iter().enumerate() {
        let m = spec.cycle.len();
        for k in 0..m {
            let (a, b) = (spec.cycle[k], spec.cycle[(k + 1) % m]);
            let key = (a.min(b), a.max(b), curve_key(&spec.edges[k]));
            pair_uses.entry(key).or_default().push(EdgeUse {
                loop_idx: si,
                pos: k,
                forward: a < b,
            });
        }
    }
    // Per (loop, pos) directional normal for full-circle uses.
    let mut full_normals: BTreeMap<(usize, usize), UnitVector3> = BTreeMap::new();
    // KV9-F1 diagnosis probe (read-only, env-gated): report EVERY pairing
    // violation with curve identity + owning loops/faces, and every other
    // use touching the offending vertices, before the loud reject.
    if std::env::var_os("KV2_OUT_TWIN_PROBE").is_some() && pair_uses.values().any(|u| u.len() != 2)
    {
        let mut bad_verts: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
        for (&(a, b, ref ck), uses) in &pair_uses {
            if uses.len() != 2 {
                let pa = yverts[a as usize].point.as_array();
                let pb = yverts[b as usize].point.as_array();
                eprintln!(
                    "[edge-pair-probe] key ({a},{b}) uses={} curve={ck:?}\n  a: ({},{},{})\n  b: ({},{},{})",
                    uses.len(),
                    pa[0],
                    pa[1],
                    pa[2],
                    pb[0],
                    pb[1],
                    pb[2]
                );
                for u in uses {
                    eprintln!(
                        "    use: yang face {} loop_idx {} pos {} forward {}",
                        loops[u.loop_idx].face, u.loop_idx, u.pos, u.forward
                    );
                }
                bad_verts.insert(a);
                bad_verts.insert(b);
            }
        }
        for (&(a, b, ref ck), uses) in &pair_uses {
            if uses.len() == 2 && (bad_verts.contains(&a) || bad_verts.contains(&b)) {
                eprintln!(
                    "[edge-pair-probe]   context edge ({a},{b}) curve={ck:?} faces {:?}",
                    uses.iter()
                        .map(|u| loops[u.loop_idx].face)
                        .collect::<Vec<_>>()
                );
            }
        }
        // Full loop dumps for every face touching a bad vertex.
        let bad_faces: std::collections::BTreeSet<usize> =
            if std::env::var_os("KV2_OUT_ALL_LOOPS").is_some() {
                loops.iter().map(|s| s.face).collect()
            } else {
                loops
                    .iter()
                    .filter(|s| s.cycle.iter().any(|v| bad_verts.contains(v)))
                    .map(|s| s.face)
                    .collect()
            };
        for spec in &loops {
            if !bad_faces.contains(&spec.face) {
                continue;
            }
            eprintln!(
                "[edge-pair-probe] FACE {} loop ({} edges):",
                spec.face,
                spec.cycle.len()
            );
            let m = spec.cycle.len();
            for k in 0..m {
                let (va, vb) = (spec.cycle[k], spec.cycle[(k + 1) % m]);
                let tag = match &spec.edges[k] {
                    EdgeKind::Seg => "Seg",
                    EdgeKind::Full { .. } => "Full",
                    EdgeKind::Arc { .. } => "Arc",
                    EdgeKind::EllipseArc { .. } => "EllArc",
                    EdgeKind::HyperbolaArc { .. } => "HypArc",
                    EdgeKind::SurfacePair { .. } => "SurfPair",
                };
                let p = yverts[va as usize].point.as_array();
                eprintln!(
                    "    [{k}] {va}->{vb} {tag} from ({:.6},{:.6},{:.6})",
                    p[0], p[1], p[2]
                );
            }
        }
    }
    for (&(a, b, ref _ck), uses) in &pair_uses {
        if uses.len() != 2 {
            return Err(KernelV2Error::InvalidBooleanOutput(
                "an undirected output edge is not used by exactly two directed edges",
            ));
        }
        let (u0, u1) = (&uses[0], &uses[1]);
        let k0 = loops[u0.loop_idx].edges[u0.pos];
        let k1 = loops[u1.loop_idx].edges[u1.pos];
        if a != b {
            if u0.forward == u1.forward {
                return Err(KernelV2Error::InvalidBooleanOutput(
                    "an undirected output edge is not used by two OPPOSITE directed edges",
                ));
            }
            // Curve agreement between the two uses.
            match (k0, k1) {
                (EdgeKind::Seg, EdgeKind::Seg) => {}
                (
                    EdgeKind::EllipseArc {
                        center: c0,
                        forward_normal: n0,
                        major_axis: m0,
                        major_radius: a0,
                        minor_radius: b0,
                    },
                    EdgeKind::EllipseArc {
                        center: c1,
                        forward_normal: n1,
                        major_axis: m1,
                        major_radius: a1,
                        minor_radius: b1,
                    },
                ) => {
                    // PR-KV9: same frame, exactly negated traversal normals
                    // (each use's normal is derived from its own walk).
                    if c0 != c1
                        || a0 != a1
                        || b0 != b1
                        || m0 != m1
                        || n0[0] != -n1[0]
                        || n0[1] != -n1[1]
                        || n0[2] != -n1[2]
                    {
                        return Err(KernelV2Error::InvalidBooleanOutput(
                            "twin output edges carry inconsistent ellipse-arc curves",
                        ));
                    }
                }
                (
                    EdgeKind::Arc {
                        center: c0,
                        radius: r0,
                        forward_normal: n0,
                    },
                    EdgeKind::Arc {
                        center: c1,
                        radius: r1,
                        forward_normal: n1,
                    },
                ) => {
                    // The per-use forward normals are derived from each use's
                    // own (start, end), so the twin pair must come out as
                    // exact negations (same stored circle, opposite walks).
                    if c0 != c1 || r0 != r1 || n0[0] != -n1[0] || n0[1] != -n1[1] || n0[2] != -n1[2]
                    {
                        return Err(KernelV2Error::InvalidBooleanOutput(
                            "twin output edges carry inconsistent arc curves",
                        ));
                    }
                }
                (
                    EdgeKind::SurfacePair { a: a0, b: b0 },
                    EdgeKind::SurfacePair { a: a1, b: b1 },
                ) => {
                    // M5 (K5): surface-pair twins carry BIT-IDENTICAL defining
                    // surfaces (there is no directional normal to negate —
                    // traversal is endpoint-determined). The undirected pairing
                    // already keys by the ordered pair (CurveKey::SurfacePair),
                    // so this only re-affirms exact agreement.
                    if a0 != a1 || b0 != b1 {
                        return Err(KernelV2Error::InvalidBooleanOutput(
                            "twin output edges carry inconsistent surface-pair curves",
                        ));
                    }
                }
                (
                    EdgeKind::HyperbolaArc {
                        center: c0,
                        normal: n0,
                        major_axis: m0,
                        semi_transverse: a0,
                        semi_conjugate: b0,
                    },
                    EdgeKind::HyperbolaArc {
                        center: c1,
                        normal: n1,
                        major_axis: m1,
                        semi_transverse: a1,
                        semi_conjugate: b1,
                    },
                ) => {
                    // KV16: hyperbola twins carry BIT-IDENTICAL fields (the
                    // SurfacePair convention — endpoint-determined traversal,
                    // both uses copy the same yang edge descriptor).
                    if c0 != c1 || n0 != n1 || m0 != m1 || a0 != a1 || b0 != b1 {
                        return Err(KernelV2Error::InvalidBooleanOutput(
                            "twin output edges carry inconsistent hyperbola-arc curves",
                        ));
                    }
                }
                _ => {
                    return Err(KernelV2Error::InvalidBooleanOutput(
                        "twin output edges carry inconsistent curve kinds",
                    ));
                }
            }
        } else {
            // Full-circle self-pair: derive each use's directional normal.
            let (
                EdgeKind::Full {
                    center: c0,
                    radius: r0,
                    normal: n0,
                },
                EdgeKind::Full {
                    center: c1,
                    radius: r1,
                    normal: n1,
                },
            ) = (k0, k1)
            else {
                return Err(KernelV2Error::InvalidBooleanOutput(
                    "a self-loop output edge is not a full circle",
                ));
            };
            if c0 != c1 || r0 != r1 || (n0 != n1 && n0 != [-n1[0], -n1[1], -n1[2]]) {
                return Err(KernelV2Error::InvalidBooleanOutput(
                    "the two uses of a full-circle edge carry inconsistent circles",
                ));
            }
            // Planar use(s) take ±plane normal by loop kind; a cylinder use
            // takes the negation of its partner.
            let derive_planar = |u: &EdgeUse| -> Option<UnitVector3> {
                let spec = &loops[u.loop_idx];
                let FaceSurf::Plane { normal: pn } = &surfs[spec.face] else {
                    return None;
                };
                let stored = match spec.edges[u.pos] {
                    EdgeKind::Full { normal, .. } => normal,
                    _ => unreachable!("checked full above"),
                };
                let want_sign = match spec.kind {
                    LoopKind::Outer => 1.0,
                    LoopKind::Inner => -1.0,
                };
                let d = dot3(stored, *pn);
                if d.abs() < 1.0 - YANG_NORMAL_AGREEMENT_TOLERANCE {
                    return None; // circle axis disagrees with the face plane
                }
                let s = if d * want_sign > 0.0 { 1.0 } else { -1.0 };
                Some(UnitVector3 {
                    x: s * stored[0],
                    y: s * stored[1],
                    z: s * stored[2],
                })
            };
            let n_for = |u: &EdgeUse, partner: &EdgeUse| -> Result<UnitVector3, KernelV2Error> {
                if let Some(nu) = derive_planar(u) {
                    return Ok(nu);
                }
                if matches!(
                    surfs[loops[u.loop_idx].face],
                    FaceSurf::Cylinder { .. }
                        | FaceSurf::Cone { .. }
                        | FaceSurf::Torus { .. }
                        | FaceSurf::Sphere { .. }
                ) {
                    if let Some(nu) = derive_planar(partner) {
                        return Ok(neg_unit(nu));
                    }
                }
                Err(KernelV2Error::InvalidBooleanOutput(
                    "full-circle edge sense is underivable (no planar cap use with an aligned plane)",
                ))
            };
            let nu0 = n_for(u0, u1)?;
            let nu1 = n_for(u1, u0)?;
            if nu0.x != -nu1.x || nu0.y != -nu1.y || nu0.z != -nu1.z {
                return Err(KernelV2Error::InvalidBooleanOutput(
                    "the two uses of a full-circle edge do not traverse oppositely",
                ));
            }
            full_normals.insert((u0.loop_idx, u0.pos), nu0);
            full_normals.insert((u1.loop_idx, u1.pos), nu1);
        }
    }

    // 1d. Planar face orientation: outer-loop Newell normal orientable and
    //     in agreement with yang's stated plane normal; rings wind
    //     opposite. Single full-circle loops were checked against the
    //     plane in 1c. Cylinder faces are validated by `validate_solid`'s
    //     curved orientation analysis after assembly.
    let mut face_normals: Vec<Option<UnitVector3>> = vec![None; yfaces.len()];
    for spec in &loops {
        let FaceSurf::Plane { normal } = &surfs[spec.face] else {
            continue;
        };
        if spec.cycle.len() == 1 {
            // Single full-circle loop: plane agreement established in 1c
            // (the derived normal exists only when |dot| ≈ 1).
            if spec.kind == LoopKind::Outer {
                face_normals[spec.face] = Some(UnitVector3 {
                    x: normal[0],
                    y: normal[1],
                    z: normal[2],
                });
            }
            continue;
        }
        // PR-KV9: ARC-MIDPOINT-AUGMENTED loop points (the same mechanism as
        // `validate::winding_points`, KV6a). A chord-only polygon mis-signs
        // the Newell normal when concave arcs dominate the loop — e.g. the
        // CRESCENT cap of a parallel cylinder×cylinder boolean, whose only
        // interior vertex can sit on the concave arc. Each arc contributes
        // its midpoint, which restores the bulge's signed area.
        let m = spec.cycle.len();
        let mut pts: Vec<Point3> = Vec::with_capacity(2 * m);
        for k in 0..m {
            let p0 = yverts[spec.cycle[k] as usize].point;
            pts.push(p0);
            if let EdgeKind::Arc {
                center,
                forward_normal,
                radius: _,
            } = spec.edges[k]
            {
                let p1 = yverts[spec.cycle[(k + 1) % m] as usize].point;
                if let Some(sweep) = geom::ccw_sweep(center, forward_normal, p0, p1) {
                    pts.push(geom::rotate_about_axis(
                        center,
                        forward_normal,
                        p0,
                        sweep / 2.0,
                    ));
                }
            }
            // PR-KV11: the EllipseArc analog (same role as validate.rs
            // `winding_points`) — a planar face whose boundary is dominated
            // by a concave ELLIPSE arc (the box-face bite of an oblique
            // cylinder) mis-signs the chord-only Newell normal exactly like
            // the KV9 crescent did for circle arcs.
            if let EdgeKind::EllipseArc {
                center,
                forward_normal,
                major_axis,
                major_radius,
                minor_radius,
            } = spec.edges[k]
            {
                let p1 = yverts[spec.cycle[(k + 1) % m] as usize].point;
                if let (Some(t0), Some(sweep)) = (
                    geom::ellipse_param(
                        center,
                        forward_normal,
                        major_axis,
                        major_radius,
                        minor_radius,
                        p0,
                    ),
                    geom::ellipse_ccw_sweep(
                        center,
                        forward_normal,
                        major_axis,
                        major_radius,
                        minor_radius,
                        p0,
                        p1,
                    ),
                ) {
                    pts.push(geom::ellipse_point_at(
                        center,
                        forward_normal,
                        major_axis,
                        major_radius,
                        minor_radius,
                        t0 + sweep / 2.0,
                    ));
                }
            }
            // KV16: the HyperbolaArc analog — parametric midpoint (the
            // arc dips toward the hyperbola center relative to its chord;
            // same winding-restoration role as the arc/ellipse midpoints).
            if let EdgeKind::HyperbolaArc {
                center,
                normal,
                major_axis,
                semi_transverse,
                semi_conjugate,
            } = spec.edges[k]
            {
                let p1 = yverts[spec.cycle[(k + 1) % m] as usize].point;
                if let (Some(t0), Some(t1)) = (
                    geom::hyperbola_param(center, normal, major_axis, semi_conjugate, p0),
                    geom::hyperbola_param(center, normal, major_axis, semi_conjugate, p1),
                ) {
                    pts.push(geom::hyperbola_point_at(
                        center,
                        normal,
                        major_axis,
                        semi_transverse,
                        semi_conjugate,
                        0.5 * (t0 + t1),
                    ));
                }
            }
        }
        match spec.kind {
            LoopKind::Outer => {
                let Some(nu) = geom::newell_unit(&pts) else {
                    return Err(KernelV2Error::InvalidBooleanOutput(
                        "output face outer loop has a degenerate (zero) Newell normal",
                    ));
                };
                let dotn = nu.x * normal[0] + nu.y * normal[1] + nu.z * normal[2];
                if dotn < 1.0 - YANG_NORMAL_AGREEMENT_TOLERANCE {
                    if std::env::var("KV11_PROBE").is_ok() {
                        eprintln!(
                            "KV11_PROBE newell reject: face={} dotn={dotn:.6} plane_n={normal:?} \
                             newell=({:.6},{:.6},{:.6}) cycle_len={} kinds={:?} pts={:?}",
                            spec.face,
                            nu.x,
                            nu.y,
                            nu.z,
                            spec.cycle.len(),
                            spec.edges.iter().map(edge_kind_tag).collect::<Vec<_>>(),
                            pts
                        );
                    }
                    return Err(KernelV2Error::InvalidBooleanOutput(
                        "output face plane normal disagrees with its outer-loop Newell normal",
                    ));
                }
                face_normals[spec.face] = Some(nu);
            }
            LoopKind::Inner => {
                let nw = geom::newell(&pts);
                if nw[0] * normal[0] + nw[1] * normal[1] + nw[2] * normal[2] >= 0.0 {
                    return Err(KernelV2Error::InvalidBooleanOutput(
                        "output face ring does not wind opposite to its outer loop",
                    ));
                }
            }
        }
    }

    // 1e. Connected components over faces via shared undirected edges —
    //     one shell per component.
    let component = face_components(yfaces.len(), &loops);
    let mut shells_faces: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (fi, &c) in component.iter().enumerate() {
        shells_faces.entry(c).or_default().push(fi);
    }

    // 1f. Per-component genus from the Euler–Poincaré formula
    //     (V − E + F − R = 2 − 2g for one closed shell).
    let mut shell_genus: BTreeMap<usize, u32> = BTreeMap::new();
    for (&rep, faces) in &shells_faces {
        let mut vset: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
        // Key edges by (vertex pair, curve identity) so a LENS bigon's two arcs
        // count as TWO distinct edges (else E is undercounted and the
        // Euler–Poincaré parity check spuriously fails). Mirrors the manifold
        // pairing key above.
        let mut eset: std::collections::BTreeSet<(u32, u32, CurveKey)> =
            std::collections::BTreeSet::new();
        let mut rings = 0i64;
        for spec in loops.iter().filter(|s| component[s.face] == rep) {
            if spec.kind == LoopKind::Inner {
                rings += 1;
            }
            let m = spec.cycle.len();
            for k in 0..m {
                let (a, b) = (spec.cycle[k], spec.cycle[(k + 1) % m]);
                vset.insert(a);
                eset.insert((a.min(b), a.max(b), curve_key(&spec.edges[k])));
            }
        }
        let lhs = vset.len() as i64 - eset.len() as i64 + faces.len() as i64 - rings;
        if lhs % 2 != 0 || lhs > 2 {
            return Err(KernelV2Error::InvalidBooleanOutput(
                "output component's Euler characteristic is not genus-representable",
            ));
        }
        shell_genus.insert(rep, ((2 - lhs) / 2) as u32);
    }

    // ---- pass 2: assemble (validated input ⇒ infallible) -----------------
    // Vertices: referenced yang verts only, created in yang index order.
    let mut referenced = vec![false; yverts.len()];
    for spec in &loops {
        for &v in &spec.cycle {
            referenced[v as usize] = true;
        }
    }
    let mut vert_ids: Vec<Option<VertexId>> = vec![None; yverts.len()];
    for (i, yv) in yverts.iter().enumerate() {
        if referenced[i] {
            vert_ids[i] = Some(push_vertex(arena, yv.point));
        }
    }

    // Solid + shells (component order = ascending smallest face index).
    let solid_id = SolidId(arena.solids.len() as u32);
    arena.solids.push(Some(Solid { shells: Vec::new() }));
    let mut shell_of_face: Vec<ShellId> = vec![ShellId(0); yfaces.len()];
    for (&rep, faces) in &shells_faces {
        let shell_id = ShellId(arena.shells.len() as u32);
        arena.shells.push(Some(Shell {
            solid: solid_id,
            faces: Vec::new(),
            genus: shell_genus[&rep],
        }));
        if let Some(Some(solid)) = arena.solids.get_mut(solid_id.index()) {
            solid.shells.push(shell_id);
        }
        for &fi in faces {
            shell_of_face[fi] = shell_id;
        }
    }

    // Faces, loops, half-edges (faces in yang index order; loops outer
    // first then rings in yang order; half-edges in walk order).
    // Keyed by (vertex pair, curve identity) so a LENS bigon's two arcs twin
    // each within its own curve, not cross-twinned by shared endpoints (M8
    // disc∩disc crossing). Mirrors the manifold-pairing + Euler keys above.
    let mut twin_table: BTreeMap<(u32, u32, CurveKey), HalfEdgeId> = BTreeMap::new();
    let mut face_ids: Vec<Option<FaceId>> = vec![None; yfaces.len()];
    for (si, spec) in loops.iter().enumerate() {
        let fi = spec.face;
        let face_id = match face_ids[fi] {
            Some(id) => id,
            None => {
                let id = FaceId(arena.faces.len() as u32);
                let p0 = yverts[spec.cycle[0] as usize].point;
                let surface = match &surfs[fi] {
                    FaceSurf::Plane { .. } => Surface::Plane(Plane {
                        point: p0,
                        normal: face_normals[fi].expect("outer loop set the planar normal"),
                    }),
                    FaceSurf::Cylinder {
                        axis_point,
                        axis_dir,
                        radius,
                        reversed,
                    } => Surface::Cylinder {
                        axis_point: *axis_point,
                        axis_dir: UnitVector3 {
                            x: axis_dir[0],
                            y: axis_dir[1],
                            z: axis_dir[2],
                        },
                        radius: *radius,
                        reversed: *reversed,
                    },
                    FaceSurf::Cone {
                        apex,
                        axis_dir,
                        half_angle,
                        reversed,
                    } => Surface::Cone {
                        apex: *apex,
                        axis_dir: UnitVector3 {
                            x: axis_dir[0],
                            y: axis_dir[1],
                            z: axis_dir[2],
                        },
                        half_angle: *half_angle,
                        reversed: *reversed,
                    },
                    FaceSurf::Torus {
                        center,
                        axis_dir,
                        major_radius,
                        minor_radius,
                        reversed,
                    } => Surface::Torus {
                        center: *center,
                        axis_dir: UnitVector3 {
                            x: axis_dir[0],
                            y: axis_dir[1],
                            z: axis_dir[2],
                        },
                        major_radius: *major_radius,
                        minor_radius: *minor_radius,
                        reversed: *reversed,
                    },
                    FaceSurf::Sphere {
                        center,
                        radius,
                        reversed,
                    } => Surface::Sphere {
                        center: *center,
                        radius: *radius,
                        reversed: *reversed,
                    },
                };
                arena.faces.push(Some(Face {
                    surface: Some(surface),
                    outer_loop: LoopId(0), // patched below
                    inner_loops: Vec::new(),
                    shell: shell_of_face[fi],
                }));
                if let Some(Some(shell)) = arena.shells.get_mut(shell_of_face[fi].index()) {
                    shell.faces.push(id);
                }
                face_ids[fi] = Some(id);
                id
            }
        };

        // The loop slot and its half-edge cycle.
        let loop_id = LoopId(arena.loops.len() as u32);
        let m = spec.cycle.len();
        let he_base = arena.half_edges.len() as u32;
        for k in 0..m {
            let (a, b) = (spec.cycle[k], spec.cycle[(k + 1) % m]);
            let h = HalfEdgeId(he_base + k as u32);
            let key = (a.min(b), a.max(b), curve_key(&spec.edges[k]));
            // Twin pairing: the second visitor of an undirected (pair, curve)
            // links both directions (pass 1c proved exactly two consistent uses).
            let twin = match twin_table.get(&key) {
                Some(&other) => {
                    if let Some(Some(o)) = arena.half_edges.get_mut(other.index()) {
                        o.twin = h;
                    }
                    other
                }
                None => {
                    twin_table.insert(key, h);
                    h // placeholder; overwritten by the partner's visit
                }
            };
            let curve = match spec.edges[k] {
                EdgeKind::Seg => Curve::LineSegment,
                EdgeKind::Arc {
                    center,
                    forward_normal,
                    radius,
                } => Curve::Arc {
                    center,
                    normal: UnitVector3 {
                        x: forward_normal[0],
                        y: forward_normal[1],
                        z: forward_normal[2],
                    },
                    radius,
                },
                EdgeKind::EllipseArc {
                    center,
                    forward_normal,
                    major_axis,
                    major_radius,
                    minor_radius,
                } => Curve::EllipseArc {
                    center,
                    normal: UnitVector3 {
                        x: forward_normal[0],
                        y: forward_normal[1],
                        z: forward_normal[2],
                    },
                    major_axis: UnitVector3 {
                        x: major_axis[0],
                        y: major_axis[1],
                        z: major_axis[2],
                    },
                    major_radius,
                    minor_radius,
                },
                EdgeKind::HyperbolaArc {
                    center,
                    normal,
                    major_axis,
                    semi_transverse,
                    semi_conjugate,
                } => Curve::HyperbolaArc {
                    center,
                    normal: UnitVector3 {
                        x: normal[0],
                        y: normal[1],
                        z: normal[2],
                    },
                    major_axis: UnitVector3 {
                        x: major_axis[0],
                        y: major_axis[1],
                        z: major_axis[2],
                    },
                    semi_transverse,
                    semi_conjugate,
                },
                EdgeKind::Full { center, radius, .. } => {
                    let nu = full_normals[&(si, k)];
                    Curve::Circle {
                        center,
                        normal: nu,
                        radius,
                    }
                }
                EdgeKind::SurfacePair { a, b } => Curve::SurfacePair { a, b },
            };
            let origin = vert_ids[a as usize].expect("referenced vertex was created");
            arena.half_edges.push(Some(HalfEdge {
                twin,
                next: HalfEdgeId(he_base + ((k + 1) % m) as u32),
                prev: HalfEdgeId(he_base + ((k + m - 1) % m) as u32),
                origin,
                loop_id,
                curve,
            }));
        }
        arena.loops.push(Some(Loop {
            face: face_id,
            boundary: LoopBoundary::Edges(HalfEdgeId(he_base)),
            kind: spec.kind,
        }));
        if let Some(Some(face)) = arena.faces.get_mut(face_id.index()) {
            match spec.kind {
                LoopKind::Outer => face.outer_loop = loop_id,
                LoopKind::Inner => face.inner_loops.push(loop_id),
            }
        }
    }

    // ---- pass 3: full production validation (defense in depth) -----------
    // Validate, then stamp persistent ids on the boolean's output faces
    // (KV13 F1). Per-face lineage attribution (F2) is recorded by `boolean_op`,
    // which has the operand→Pid maps; here we return the output face mapping.
    finalize_solid(arena, solid_id)?;
    Ok((solid_id, face_ids))
}

fn push_vertex(arena: &mut BrepArena, point: Point3) -> VertexId {
    let id = VertexId(arena.vertices.len() as u32);
    arena.vertices.push(Some(Vertex { point }));
    id
}

/// Union-find over yang face indices, joined by shared undirected edges.
/// Returns each face's component representative (the smallest face index
/// in its component).
fn face_components(num_faces: usize, loops: &[LoopSpec]) -> Vec<usize> {
    let mut parent: Vec<usize> = (0..num_faces).collect();
    fn find(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }
    let mut edge_face: BTreeMap<(u32, u32), usize> = BTreeMap::new();
    for spec in loops {
        let m = spec.cycle.len();
        for k in 0..m {
            let (a, b) = (spec.cycle[k], spec.cycle[(k + 1) % m]);
            let key = (a.min(b), a.max(b));
            match edge_face.get(&key) {
                Some(&other) => {
                    let (ra, rb) = (find(&mut parent, spec.face), find(&mut parent, other));
                    let (lo, hi) = (ra.min(rb), ra.max(rb));
                    parent[hi] = lo;
                }
                None => {
                    edge_face.insert(key, spec.face);
                }
            }
        }
    }
    (0..num_faces).map(|f| find(&mut parent, f)).collect()
}
