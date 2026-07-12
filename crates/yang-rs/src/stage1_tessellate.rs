//! Stage 1 — tessellation with bijective mapping: per-surface
//! tessellators, rim/band construction, chord bounds, orientation
//! helpers (extracted verbatim from lib.rs — spec
//! `specs/yang_rs_lib_decomposition.md`, increment 4).

#[allow(clippy::wildcard_imports)]
use crate::*;

/// Stage-1 tessellation output (PR-YR26 extraction of the `BRep::new` body):
/// the mesh vertex pool (B-Rep vertices first, Steiner appended), the 1:1
/// `TessellationSource` per vertex, the triangles, and per input face the
/// range of `tris` it produced (consumed by the Stage-0 overlay
/// re-tessellation to splice per-face replacements).
pub(crate) struct Stage1Tess {
    pub(crate) verts: Vec<Point3>,
    pub(crate) sources: Vec<TessellationSource>,
    pub(crate) tris: Vec<[u32; 3]>,
    pub(crate) face_tri_ranges: Vec<std::ops::Range<usize>>,
    /// Per curved-edge sample chains (`rim_rings`): full-circle/full-ellipse
    /// closed rings and open arc chains, keyed by B-Rep edge index — the
    /// SHARED boundary sampling. Consumed by the Stage-0 mixed-face overlay
    /// arm (spec `m8_mixed_loop_coplanar_overlay`) to splice loop polylines
    /// bit-identical to this tessellation's own face boundaries.
    pub(crate) chains: std::collections::BTreeMap<u32, Vec<u32>>,
}

/// Stage 1 tessellation (PR-YR7: planar Newell-fan + curved cylinder;
/// PR-YR12: sphere lat/long grid; PR-NC1: CDT for non-convex/holed planar
/// faces). Extracted verbatim from `BRep::new` in PR-YR26 (plus per-face
/// triangle-range recording) so Stage-0 coplanar preprocessing can
/// re-tessellate with snapped vertex coordinates.
///
/// Mesh vertices start 1:1 with the B-Rep vertices (the planar box path
/// emits no Steiner points). The curved path appends rim-ring + cap-
/// center Steiner vertices and indexes the SHARED cached rings so the
/// cylinder mesh is watertight.
pub(crate) fn stage1_tessellate(
    verts: &[BRepVertex],
    edges: &[BRepEdge],
    faces: &[BRepFace],
) -> Result<Stage1Tess, YangError> {
    stage1_tessellate_with_rim_overrides(
        verts,
        edges,
        faces,
        &std::collections::BTreeMap::new(),
        None,
    )
}

/// Stage 1 tessellation with optional per-circle-edge RIM CROSSING points
/// (PR-M8 disc-rim crossing). `rim_overrides[e]` lists extra 3D points to
/// insert into edge `e`'s full-circle rim ring as additional Steiner
/// vertices — the §4.5.5 shared-boundary sampling for a coplanar disc whose
/// rim the overlap boundary CROSSES. Each inserted point is placed at its
/// angle-from-seam sorted position; the edge is recorded in the returned
/// `inserted_rims` set so [`tessellate_lateral_face`] routes that rim's
/// lateral through the AZIMUTH-MERGE strip (uniform index-pairing no longer
/// holds once a rim carries non-uniform samples).
///
/// An EMPTY `rim_overrides` map yields byte-identical `verts` and `tris` to
/// [`stage1_tessellate`] — the uniform-rim path is left 100% untouched (see
/// the `rim_override_empty_is_byte_identical` unit test).
///
/// An override that coincides ANGULARLY with a uniform sample MERGES when it
/// is a sub-TAU_MODEL twin of that sample (task #143, spec
/// `m8_rim_override_uniform_merge`): the slot takes the override's exact bits
/// — ring length unchanged, no azimuth-merge routing — so cap overlay, rim,
/// and lateral share the one fused point (§4.5.5).
///
/// Loud errors (`MalformedTopology`):
/// - a coinciding override ≥ TAU_MODEL from the uniform sample (real-scale
///   graze — fail closed),
/// - a coinciding override that differs in bits from the SEAM vertex / an
///   arc endpoint (B-Rep vertices are authoritative),
/// - two DISTINCT overrides claiming one uniform slot, or
/// - an override point not on the rim circle (off-radius / off-plane).
///
/// An override point coinciding with an already-inserted override (or a
/// bit-identical repeat of a merged one) is deduplicated (skipped).
pub(crate) fn stage1_tessellate_with_rim_overrides(
    verts: &[BRepVertex],
    edges: &[BRepEdge],
    faces: &[BRepFace],
    rim_overrides: &std::collections::BTreeMap<u32, Vec<Point3>>,
    min_n_seg: Option<usize>,
) -> Result<Stage1Tess, YangError> {
    stage1_tessellate_inner(verts, edges, faces, rim_overrides, min_n_seg).map(|(t, _)| t)
}

/// Stage 1 tessellation forcing the circle-rim segment count to AT LEAST
/// `min_n_seg` (M8-cyl Increment 1). The cylinder rim N is normally derived
/// from this solid's own chord-error AABB; for two COINCIDENT cylinders to get
/// identical overlap meshes (§4.5.5) BOTH must sample at the SAME N. The caller
/// passes the max of both solids' N so each gets a rim sampling that satisfies
/// BOTH chord bounds (a FINER tessellation is always chord-valid — it can only
/// REDUCE the sagitta, never widen it; this is not a tolerance relaxation).
/// `min_n_seg = None` is byte-identical to [`stage1_tessellate`].
pub(crate) fn stage1_tessellate_min_segments(
    verts: &[BRepVertex],
    edges: &[BRepEdge],
    faces: &[BRepFace],
    min_n_seg: Option<usize>,
) -> Result<Stage1Tess, YangError> {
    stage1_tessellate_inner(
        verts,
        edges,
        faces,
        &std::collections::BTreeMap::new(),
        min_n_seg,
    )
    .map(|(t, _)| t)
}

/// Inner implementation returning the tessellation AND the set of rim edges
/// that received inserted crossing points (consumed by the lateral dispatch).
#[allow(clippy::type_complexity)]
pub(crate) fn stage1_tessellate_inner(
    verts: &[BRepVertex],
    edges: &[BRepEdge],
    faces: &[BRepFace],
    rim_overrides: &std::collections::BTreeMap<u32, Vec<Point3>>,
    min_n_seg: Option<usize>,
) -> Result<(Stage1Tess, std::collections::BTreeSet<u32>), YangError> {
    {
        let mut inserted_rims: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
        let mut out_verts: Vec<Point3> = verts.iter().map(|v| v.point).collect();
        let mut sources: Vec<TessellationSource> = (0..verts.len() as u32)
            .map(TessellationSource::BRepVertex)
            .collect();
        let mut out_tris: Vec<[u32; 3]> = Vec::new();

        // ---- Curved pre-pass: choose N (chord error) + build shared rim rings.
        //
        // N is chosen once from the analytic AABB of ALL `Curve::Circle` rim
        // edges combined (spec §3), and shared by every circle. The minimal
        // cylinder has exactly two rims of equal radius, so a single N applies.
        //
        // PR-YR12: a `Surface::Sphere` face is self-contained — it builds its
        // own latitude/longitude grid in `tessellate_sphere_face` and does NOT
        // participate in the cylinder rim-ring pre-pass. Exclude any Circle edge
        // that belongs to a sphere face's loops so the cylinder path stays
        // byte-for-byte unchanged (with a pure-sphere B-Rep `circle_edges` ends
        // up empty and the whole rim pre-pass is skipped).
        let sphere_seam_edges: std::collections::BTreeSet<u32> = faces
            .iter()
            .filter(|f| matches!(f.surface, Surface::Sphere { .. }))
            .flat_map(|f| {
                f.outer_loop
                    .iter()
                    .chain(f.inner_loops.iter().flatten())
                    .copied()
            })
            .collect();
        let circle_edges: Vec<(usize, Point3, Vector3, f64, u32, u32)> = edges
            .iter()
            .enumerate()
            .filter_map(|(i, e)| match e.curve {
                Curve::Circle {
                    center,
                    normal,
                    radius,
                } if !sphere_seam_edges.contains(&(i as u32)) => {
                    Some((i, center, normal, radius, e.start, e.end))
                }
                _ => None,
            })
            .collect();

        // edge_idx -> the cached ring of mesh-vertex indices (ring[0] reuses the
        // circle's seam B-Rep vertex; ring[1..N] are new Steiner verts).
        let mut rim_rings: std::collections::BTreeMap<u32, Vec<u32>> =
            std::collections::BTreeMap::new();

        if !circle_edges.is_empty() {
            // Stage-1 chord bound `d_ε = 1e-2 × analytic-AABB-diag` over all rim
            // circles, from the SINGLE shared source (governance A14.3). Since
            // `circle_edges` is non-empty, `curved_chord_bound` returns `Some`;
            // the `unwrap_or(0.0)` is an unreachable no-panic guard (P9 — a 0.0
            // band is already handled by the `d_eps > 0.0` floor below, keeping
            // the N=3 floor rather than panicking).
            let mut d_eps = curved_chord_bound(edges).unwrap_or(0.0);
            // PR-YR16 (spec §3): the rim-AABB `curved_chord_bound` ignores the
            // cone height and can EXCEED the cone's honest bound for wide-short
            // cones (`h < 2R`), which would permit a residual larger than
            // `cone_chord_bound`. When ANY `Surface::Cone` face is present,
            // tighten `d_eps` by folding in each cone's own bound via min().
            // Cylinder / sphere / all-planar inputs have no cone face, so this
            // branch is never entered and those paths stay byte-for-byte.
            if faces
                .iter()
                .any(|f| matches!(f.surface, Surface::Cone { .. }))
            {
                for f in faces.iter() {
                    if let Surface::Cone {
                        apex,
                        axis_dir,
                        half_angle,
                    } = f.surface
                    {
                        let au = normalize3(axis_dir.as_array());
                        let ap = apex.as_array();
                        // Derive height_f from this cone's rim Circle (the
                        // single Circle edge in its outer loop).
                        for &e_idx in &f.outer_loop {
                            if let Curve::Circle { center, .. } = edges[e_idx as usize].curve {
                                let c = center.as_array();
                                let height_f = ((c[0] - ap[0]) * au[0]
                                    + (c[1] - ap[1]) * au[1]
                                    + (c[2] - ap[2]) * au[2])
                                    .abs();
                                d_eps = d_eps.min(cone_chord_bound(height_f, half_angle));
                            }
                        }
                    }
                }
            }
            // Smallest N >= 3 with max_radius·(1 − cos(π/N)) ≤ d_eps.
            let max_r = circle_edges
                .iter()
                .map(|&(_, _, _, r, _, _)| r)
                .fold(0.0f64, f64::max);
            let mut n_seg = 3usize;
            // d_eps > 0 for any non-degenerate cylinder; if it is somehow zero
            // (a degenerate AABB), keep the floor N=3 rather than loop forever.
            if d_eps > 0.0 {
                while max_r * (1.0 - (std::f64::consts::PI / n_seg as f64).cos()) > d_eps {
                    n_seg += 1;
                }
            }
            // M8-cyl Increment 1: a coincident-cylinder pair forces BOTH solids
            // to the same (max) N so their overlap rings are identically sampled
            // (a finer N only shrinks the sagitta — chord-valid for this solid).
            if let Some(force) = min_n_seg {
                n_seg = n_seg.max(force);
            }
            // Case-IV INTRA-solid phantom fold (spec
            // `yang_case_iv_phantom_guard`, M8 increment 16): two of THIS
            // solid's own analytically-disjoint cylinders closer than the
            // chord bands (a hole lateral 0.0115 from the plate wall — the
            // chained F0088 output) would make the cap's outer-rim chords
            // dip across the hole rim, so the planar CDT gets CROSSING
            // constraints and fails loud at CONSTRUCTION time. Fold each
            // disjoint pair's derived N in here, where every tessellation
            // (conversion, Stage-0 rebuilds, the guard's cross-pair rebuild)
            // picks it up natively. Far pairs derive a tiny N the natural
            // bound absorbs — self-limiting, no mode branch.
            let cyls: Vec<(Point3, Vector3, f64)> = faces
                .iter()
                .filter_map(|f| match f.surface {
                    Surface::Cylinder {
                        axis_point,
                        axis_dir,
                        radius,
                    } => Some((axis_point, axis_dir, radius)),
                    _ => None,
                })
                .collect();
            for i in 0..cyls.len() {
                for j in (i + 1)..cyls.len() {
                    if let Some(n) = cyl_pair_phantom_n(cyls[i], cyls[j]) {
                        n_seg = n_seg.max(n);
                    }
                }
            }
            // Diagnostic experiment knob (M8 increment-8 spec phase, task
            // #62): force a global rim-N floor to measure whether/where the
            // mint-displacement fold class is a pure sampling artifact and
            // what finer N costs. Dev-only, like TIEBREAK_NEUTER /
            // YANG_SHIFT_NEUTER — never set in production or tests.
            if let Some(floor) = std::env::var("YANG_NSEG_FLOOR")
                .ok()
                .and_then(|s| s.parse::<usize>().ok())
            {
                n_seg = n_seg.max(floor);
            }
            if std::env::var_os("YANG_SPLIT_PROBE").is_some() {
                eprintln!(
                    "[stage1-nseg] n_seg={n_seg} d_eps={d_eps:e} max_r={max_r} \
                     min_n_seg={min_n_seg:?} circles={}",
                    circle_edges.len()
                );
            }

            // Build the shared sample CHAIN for each circle edge (PR-KV6b-1):
            // a full circle (`start == end`) gets the closed seam-anchored
            // ring exactly as before; an ARC (`start != end` — the new input
            // convention: the CCW sweep around `curve.normal` from `start`
            // to `end`, unique in (0, 2π)) gets an OPEN chain
            // `[start, Steiner…, end]` sampled at the same per-chord angle
            // bound, so the global `d_eps` holds for arcs too. Chains are
            // shared between the two faces incident to the edge — the
            // watertightness mechanism, unchanged.
            for &(e_idx, center, normal, radius, e_start, e_end) in &circle_edges {
                let (e1, e2) = ortho_basis(normal);
                let c = center.as_array();
                let e1a = e1.as_array();
                let e2a = e2.as_array();
                let seam_vertex = e_start;

                // Angle of a vertex in this circle's frame + on-circle
                // validation (the arc convention makes endpoints
                // load-bearing, so off-circle endpoints are loud).
                let angle_of = |v: u32| -> Result<f64, YangError> {
                    let sp = match verts.get(v as usize) {
                        Some(vv) => vv.point.as_array(),
                        None => {
                            return Err(YangError::MalformedTopology(format!(
                                "circle edge {e_idx}: vertex {v} out of range"
                            )))
                        }
                    };
                    let w = [sp[0] - c[0], sp[1] - c[1], sp[2] - c[2]];
                    let nu = normalize3(normal.as_array());
                    let along = w[0] * nu[0] + w[1] * nu[1] + w[2] * nu[2];
                    let wx = w[0] * e1a[0] + w[1] * e1a[1] + w[2] * e1a[2];
                    let wy = w[0] * e2a[0] + w[1] * e2a[1] + w[2] * e2a[2];
                    let r = (wx * wx + wy * wy).sqrt();
                    let band = 1e-9 * (1.0 + radius);
                    if (r - radius).abs() > band || along.abs() > band {
                        return Err(YangError::MalformedTopology(format!(
                            "circle edge {e_idx}: endpoint vertex {v} is not on the circle                              (radial {r} vs radius {radius}, axial offset {along})"
                        )));
                    }
                    Ok(wy.atan2(wx))
                };

                if e_start != e_end {
                    // ---- ARC chain (PR-KV6b-1) ----
                    let phi0 = angle_of(e_start)?;
                    let phi1 = angle_of(e_end)?;
                    let two_pi = 2.0 * std::f64::consts::PI;
                    let sweep = (phi1 - phi0).rem_euclid(two_pi);
                    if sweep <= 0.0 || !sweep.is_finite() {
                        return Err(YangError::MalformedTopology(format!(
                            "circle edge {e_idx}: degenerate arc sweep {sweep}"
                        )));
                    }
                    // Same per-chord angle as the full-circle ring; floor 2
                    // segments so a π arc never degenerates to one diameter
                    // chord.
                    let m = ((sweep * n_seg as f64) / two_pi).ceil() as usize;
                    let m = m.max(2);
                    // Interior slots: (start-relative angle offset, override
                    // point or None = uniform Steiner). M8-mixed (spec
                    // `m8_mixed_loop_coplanar_overlay` amendment 1): an ARC
                    // takes chord-split overrides exactly like a full rim —
                    // same on-chord radial band, same uniform-coincidence
                    // refusal, same exact CCW ULP-twin tie-break. The mixed
                    // propagation inserts matched points into BOTH arcs of a
                    // partial strip, so its index-pairing stays conformal
                    // (arcs are deliberately NOT added to `inserted_rims`,
                    // which routes FULL-rim laterals only).
                    let mut slots: Vec<(f64, Option<Point3>)> = (1..m)
                        .map(|k| (sweep * (k as f64) / (m as f64), None))
                        .collect();
                    if let Some(extra) = rim_overrides.get(&(e_idx as u32)) {
                        let uni_step = sweep / (m as f64);
                        let merge_tol = uni_step * 1.0e-6;
                        let sagitta = radius * (1.0 - (uni_step / 2.0).cos());
                        let mut inserted_keys: Vec<[u64; 3]> = Vec::new();
                        for &pt in extra {
                            let sp = pt.as_array();
                            let w = [sp[0] - c[0], sp[1] - c[1], sp[2] - c[2]];
                            let nu = normalize3(normal.as_array());
                            let along = w[0] * nu[0] + w[1] * nu[1] + w[2] * nu[2];
                            let wx = w[0] * e1a[0] + w[1] * e1a[1] + w[2] * e1a[2];
                            let wy = w[0] * e2a[0] + w[1] * e2a[1] + w[2] * e2a[2];
                            let r = (wx * wx + wy * wy).sqrt();
                            let band = 1e-9 * (1.0 + radius);
                            if r - radius > band
                                || radius - r > sagitta + band
                                || along.abs() > band
                            {
                                return Err(YangError::MalformedTopology(format!(
                                    "circle edge {e_idx}: arc-chord override ({},{},{}) is off the \
                                     arc (radial {r} vs radius {radius} sagitta {sagitta}, axial \
                                     {along})",
                                    sp[0], sp[1], sp[2]
                                )));
                            }
                            let off = (wy.atan2(wx) - phi0).rem_euclid(two_pi);
                            if off >= sweep - merge_tol {
                                return Err(YangError::MalformedTopology(format!(
                                    "circle edge {e_idx}: arc-chord override at angle-offset {off} \
                                     is outside the arc sweep {sweep}"
                                )));
                            }
                            let key = [sp[0].to_bits(), sp[1].to_bits(), sp[2].to_bits()];
                            // Coincides angularly with a uniform slot? Same
                            // merge/refuse policy as the full rim (task #143,
                            // spec `m8_rim_override_uniform_merge`): a
                            // sub-TAU_MODEL twin takes the slot, real-scale
                            // grazes and endpoint collisions stay loud.
                            let k_near = (off / uni_step).round();
                            if (off - k_near * uni_step).abs() <= merge_tol {
                                let k_slot = k_near as usize;
                                if k_slot == 0 {
                                    // The arc START is a B-Rep vertex, not a
                                    // Steiner slot (k=m, the END, is caught by
                                    // the outside-sweep refusal above).
                                    let ev = out_verts[e_start as usize].as_array();
                                    if key == [ev[0].to_bits(), ev[1].to_bits(), ev[2].to_bits()] {
                                        continue;
                                    }
                                    return Err(YangError::MalformedTopology(format!(
                                        "circle edge {e_idx}: arc-chord override at angle-offset \
                                         {off} coincides with the arc-start endpoint but differs \
                                         in bits (B-Rep vertex is authoritative; merge refused)"
                                    )));
                                }
                                // Interior slots sit at indices k−1 (built from
                                // 1..m before any override is appended).
                                if let Some(prev) = slots[k_slot - 1].1 {
                                    let pv = prev.as_array();
                                    if key == [pv[0].to_bits(), pv[1].to_bits(), pv[2].to_bits()] {
                                        continue; // bit-identical repeat — dedup
                                    }
                                    return Err(YangError::MalformedTopology(format!(
                                        "circle edge {e_idx}: two distinct arc-chord overrides \
                                         claim uniform sample k={k_slot}"
                                    )));
                                }
                                let theta = phi0 + (k_slot as f64) * uni_step;
                                let (st, ct) = theta.sin_cos();
                                let up = [
                                    c[0] + radius * (ct * e1a[0] + st * e2a[0]),
                                    c[1] + radius * (ct * e1a[1] + st * e2a[1]),
                                    c[2] + radius * (ct * e1a[2] + st * e2a[2]),
                                ];
                                let d2 = (sp[0] - up[0]) * (sp[0] - up[0])
                                    + (sp[1] - up[1]) * (sp[1] - up[1])
                                    + (sp[2] - up[2]) * (sp[2] - up[2]);
                                let tau = cad_primitives::TAU_MODEL;
                                if d2 >= tau * tau {
                                    return Err(YangError::MalformedTopology(format!(
                                        "circle edge {e_idx}: arc-chord override at angle-offset \
                                         {off} coincides with uniform sample k={k_slot} but is {} \
                                         away (≥ TAU_MODEL {tau}) — real-scale coincidence (merge \
                                         refused)",
                                        d2.sqrt()
                                    )));
                                }
                                // MERGE: slot keeps its uniform angular key.
                                slots[k_slot - 1].1 = Some(pt);
                                continue;
                            }
                            if inserted_keys.contains(&key) {
                                continue;
                            }
                            inserted_keys.push(key);
                            slots.push((off, Some(pt)));
                        }
                    }
                    slots.sort_by(|a, b| match a.0.total_cmp(&b.0) {
                        std::cmp::Ordering::Equal => match (&a.1, &b.1) {
                            (Some(pa), Some(pb)) => exact_rim_ccw_tiebreak(c, e1a, e2a, *pa, *pb),
                            _ => std::cmp::Ordering::Equal,
                        },
                        o => o,
                    });
                    let mut chain: Vec<u32> = Vec::with_capacity(slots.len() + 2);
                    chain.push(e_start);
                    for &(off, ov) in &slots {
                        let theta = phi0 + off;
                        let pt = match ov {
                            Some(p) => p,
                            None => {
                                let (st, ct) = theta.sin_cos();
                                Point3::new(
                                    c[0] + radius * (ct * e1a[0] + st * e2a[0]),
                                    c[1] + radius * (ct * e1a[1] + st * e2a[1]),
                                    c[2] + radius * (ct * e1a[2] + st * e2a[2]),
                                )
                            }
                        };
                        let vi = out_verts.len() as u32;
                        out_verts.push(pt);
                        sources.push(TessellationSource::BRepEdge {
                            edge: e_idx as u32,
                            t: theta,
                        });
                        chain.push(vi);
                    }
                    chain.push(e_end);
                    rim_rings.insert(e_idx as u32, chain);
                    continue;
                }
                // The seam B-Rep vertex is NOT required to lie at angle 0 of
                // this circle's `ortho_basis` frame — the fixture chooses its
                // own angle-0 convention. Recover the seam's ACTUAL angle `phi0`
                // in this frame so the Steiner verts are placed at evenly-spaced
                // angles STARTING FROM the seam (`phi0 + 2πk/N`). Then `ring[0]`
                // (the seam) is consistent with `ring[1..N]` (chord spacing is
                // uniform) and — crucially for the lateral — the two rims, whose
                // seams sit at the same geometric azimuth, stay azimuth-aligned
                // under the `(N−k)` opposite-rim mapping (spec §6).
                let phi0 = angle_of(seam_vertex)?;
                // Uniform sample angles RELATIVE to the seam, in [0, 2π): the
                // seam at offset 0, the k-th Steiner at 2πk/N. A rim-crossing
                // override is inserted by its OWN seam-relative angle, sorted in.
                let two_pi = 2.0 * std::f64::consts::PI;
                // Build the (angle_offset, kind) list. kind: Uniform(k) places a
                // uniform Steiner (or reuses the seam for k==0); Override(point)
                // inserts a crossing point.
                enum RimSlot {
                    Uniform(usize),
                    Override(Point3),
                }
                let mut slots: Vec<(f64, RimSlot)> = Vec::with_capacity(n_seg);
                for k in 0..n_seg {
                    slots.push((two_pi * (k as f64) / (n_seg as f64), RimSlot::Uniform(k)));
                }
                if let Some(extra) = rim_overrides.get(&(e_idx as u32)) {
                    // Angular margin around a uniform sample: a crossing landing
                    // within this of an existing uniform vertex either MERGES
                    // (task #143, spec `m8_rim_override_uniform_merge`: the
                    // fused-emission survivor of a ULP-split mirrored rim IS the
                    // rim's own sample — the slot takes the override's exact
                    // bits) or is refused loudly (real-scale coincidence).
                    let uni_step = two_pi / (n_seg as f64);
                    let merge_tol = uni_step * 1.0e-6;
                    let mut inserted_keys: Vec<[u64; 3]> = Vec::new();
                    // Uniform slots taken by a merged override (slot → bits).
                    // Kept SEPARATE from `inserted_keys`: a pure merge never
                    // changes the ring length, so it must not route the lateral
                    // to azimuth-merge via `inserted_rims` (spec I1).
                    let mut merged_slots: std::collections::BTreeMap<usize, [u64; 3]> =
                        std::collections::BTreeMap::new();
                    for &pt in extra {
                        // Resolve the point's seam-relative angle + validate it
                        // lies on the rim circle.
                        let sp = pt.as_array();
                        let w = [sp[0] - c[0], sp[1] - c[1], sp[2] - c[2]];
                        let nu = normalize3(normal.as_array());
                        let along = w[0] * nu[0] + w[1] * nu[1] + w[2] * nu[2];
                        let wx = w[0] * e1a[0] + w[1] * e1a[1] + w[2] * e1a[2];
                        let wy = w[0] * e2a[0] + w[1] * e2a[1] + w[2] * e2a[2];
                        let r = (wx * wx + wy * wy).sqrt();
                        let band = 1e-9 * (1.0 + radius);
                        // A rim-crossing override lies on the tessellated rim
                        // POLYGON — a CHORD between two on-circle samples — so it
                        // sits up to the Stage-1 chord sagitta
                        // `radius·(1 − cos(π/N))` INSIDE the analytic circle.
                        // That chord is the rim's own Stage-1 representation
                        // (A14.3 chord bound), the SAME points the cap overlay
                        // triangles use, so the radial deficit is expected, not a
                        // fault — validating against it keeps the override
                        // bit-identical with the overlap mesh (no T-junction) and
                        // is NOT a tolerance widening. A point OUTSIDE the circle,
                        // or inside by MORE than the sagitta (a bad projection),
                        // or off the cap plane (axial) is still a loud fault.
                        let sagitta = radius * (1.0 - (std::f64::consts::PI / n_seg as f64).cos());
                        if r - radius > band || radius - r > sagitta + band || along.abs() > band {
                            return Err(YangError::MalformedTopology(format!(
                                "circle edge {e_idx}: rim-crossing override ({},{},{}) is off the \
                                 rim (radial {r} vs radius {radius} sagitta {sagitta}, axial {along})",
                                sp[0], sp[1], sp[2]
                            )));
                        }
                        let off = (wy.atan2(wx) - phi0).rem_euclid(two_pi);
                        let key = [sp[0].to_bits(), sp[1].to_bits(), sp[2].to_bits()];
                        // Coincides angularly with a uniform sample?
                        let k_near = (off / uni_step).round();
                        if (off - k_near * uni_step).abs() <= merge_tol {
                            let k_slot = (k_near as usize) % n_seg;
                            if k_slot == 0 {
                                // The SEAM is a B-Rep vertex, not a Steiner slot
                                // — replacing its bits in one ring would desync
                                // every other edge/face sharing the vertex.
                                // Bit-exact = the point is already in the ring.
                                let sv = out_verts[seam_vertex as usize].as_array();
                                if key == [sv[0].to_bits(), sv[1].to_bits(), sv[2].to_bits()] {
                                    continue;
                                }
                                return Err(YangError::MalformedTopology(format!(
                                    "circle edge {e_idx}: rim-crossing override at angle-offset \
                                     {off} coincides with the seam vertex but differs in bits \
                                     (B-Rep vertex is authoritative; merge refused)"
                                )));
                            }
                            if let Some(prev) = merged_slots.get(&k_slot) {
                                if *prev == key {
                                    continue; // bit-identical repeat — dedup
                                }
                                return Err(YangError::MalformedTopology(format!(
                                    "circle edge {e_idx}: two distinct rim-crossing overrides \
                                     claim uniform sample k={k_slot}"
                                )));
                            }
                            // Identity ceiling (A14.2/A14.3, the task-#142
                            // fused-emission constant): only a sub-TAU_MODEL
                            // twin of the uniform sample may merge; a REAL-scale
                            // graze stays the loud wall (fail closed).
                            let theta = phi0 + (k_slot as f64) * uni_step;
                            let (st, ct) = theta.sin_cos();
                            let up = [
                                c[0] + radius * (ct * e1a[0] + st * e2a[0]),
                                c[1] + radius * (ct * e1a[1] + st * e2a[1]),
                                c[2] + radius * (ct * e1a[2] + st * e2a[2]),
                            ];
                            let d2 = (sp[0] - up[0]) * (sp[0] - up[0])
                                + (sp[1] - up[1]) * (sp[1] - up[1])
                                + (sp[2] - up[2]) * (sp[2] - up[2]);
                            let tau = cad_primitives::TAU_MODEL;
                            if d2 >= tau * tau {
                                return Err(YangError::MalformedTopology(format!(
                                    "circle edge {e_idx}: rim-crossing override at angle-offset \
                                     {off} coincides with uniform sample k={k_slot} but is {} \
                                     away (≥ TAU_MODEL {tau}) — real-scale coincidence (merge \
                                     refused)",
                                    d2.sqrt()
                                )));
                            }
                            // MERGE: the slot keeps its uniform angular key (sort
                            // order + emission theta unchanged) and takes the
                            // override's exact bits — the ONE shared point the
                            // cap overlay already carries (§4.5.5 conformality).
                            slots[k_slot] = (slots[k_slot].0, RimSlot::Override(pt));
                            merged_slots.insert(k_slot, key);
                            continue;
                        }
                        // Bit-identical to an already-inserted override? dedup
                        // (the SAME point re-arriving from adjacent sub-chords).
                        // M-C fix (spec `m8_stage0_band_scale_crossing_verts`
                        // E-C1/E-C1b): identity is EXACT coordinate bits — never
                        // an angular tolerance. Genuinely distinct band-close
                        // crossings (the R0088/R0070 twin population) must BOTH
                        // enter the ring, or it desynchronizes from the cap
                        // override that carries both points (T-junction holes).
                        if inserted_keys.contains(&key) {
                            continue;
                        }
                        inserted_keys.push(key);
                        slots.push((off, RimSlot::Override(pt)));
                    }
                    if !inserted_keys.is_empty() {
                        inserted_rims.insert(e_idx as u32);
                    }
                }
                // Sort by seam-relative angle (the seam, offset 0, leads).
                // ULP-twin overrides collide on the f64 angle key (spec
                // `m8_holed_disc_coplanar_overlay` §8 F2): break the tie by
                // the EXACT angular order — never by insertion order, which
                // is frame-direction-dependent and desynchronises the ring
                // from the cap overlay boundary / the opposite rim.
                // Override-vs-uniform ties are impossible (merge_tol guard).
                slots.sort_by(|a, b| match a.0.total_cmp(&b.0) {
                    std::cmp::Ordering::Equal => match (&a.1, &b.1) {
                        (RimSlot::Override(pa), RimSlot::Override(pb)) => {
                            exact_rim_ccw_tiebreak(c, e1a, e2a, *pa, *pb)
                        }
                        _ => std::cmp::Ordering::Equal,
                    },
                    ord => ord,
                });
                let mut ring: Vec<u32> = Vec::with_capacity(slots.len());
                for (off, slot) in slots {
                    match slot {
                        RimSlot::Uniform(0) => {
                            // ring[0] = the seam B-Rep vertex (keep its source).
                            ring.push(seam_vertex);
                        }
                        RimSlot::Uniform(_) => {
                            let theta = phi0 + off;
                            let (ct, st) = (theta.cos(), theta.sin());
                            let pt = [
                                c[0] + radius * (ct * e1a[0] + st * e2a[0]),
                                c[1] + radius * (ct * e1a[1] + st * e2a[1]),
                                c[2] + radius * (ct * e1a[2] + st * e2a[2]),
                            ];
                            let vi = out_verts.len() as u32;
                            out_verts.push(Point3::new(pt[0], pt[1], pt[2]));
                            sources.push(TessellationSource::BRepEdge {
                                edge: e_idx as u32,
                                t: theta,
                            });
                            ring.push(vi);
                        }
                        RimSlot::Override(pt) => {
                            let theta = phi0 + off;
                            let vi = out_verts.len() as u32;
                            out_verts.push(pt);
                            sources.push(TessellationSource::BRepEdge {
                                edge: e_idx as u32,
                                t: theta,
                            });
                            ring.push(vi);
                        }
                    }
                }
                rim_rings.insert(e_idx as u32, ring);
            }
        }

        // ---- Ellipse chain pre-pass (KV14 ellipse-arc re-entry, spec
        // `kv14_ellipse_arc_reentry`). A `Curve::Ellipse` boundary edge (the
        // oblique plane∩cylinder/cone section a prior boolean leaves on a
        // re-entering body) builds a shared sample chain in `rim_rings`,
        // mirroring the Circle block above: an ARC (`start != end` — the
        // kernel-v2 input convention guarantees a MINOR arc, sweep < π, CCW
        // around the stored `normal` from `start` to `end`) gets an open
        // chain `[start, Steiner…, end]`; a FULL ellipse (`start == end`)
        // gets the closed seam-anchored ring. The chord bound is
        // self-contained (`d_ε = 1e-2 · major_radius`, the circle chord rule
        // applied at the ellipse's worst-case curvature scale) because an
        // ellipse-bounded cap may carry no Circle edge to derive a shared
        // bound from. Chains are shared between the two incident faces — the
        // watertightness mechanism, unchanged.
        for (e_idx, e) in edges.iter().enumerate() {
            let Curve::Ellipse {
                center,
                normal,
                major_axis,
                major_radius,
                minor_radius,
            } = e.curve
            else {
                continue;
            };
            let d_eps = ellipse_chord_bound(major_radius);
            let mut n_seg = 3usize;
            if d_eps > 0.0 {
                while major_radius * (1.0 - (std::f64::consts::PI / n_seg as f64).cos()) > d_eps {
                    n_seg += 1;
                }
            }
            let two_pi = 2.0 * std::f64::consts::PI;
            // Param of an endpoint vertex + on-ellipse validation (the arc
            // convention makes endpoints load-bearing, so off-ellipse
            // endpoints are loud).
            let param_of = |v: u32| -> Result<f64, YangError> {
                let p = verts.get(v as usize).map(|vv| vv.point).ok_or_else(|| {
                    YangError::MalformedTopology(format!(
                        "ellipse edge {e_idx}: vertex {v} out of range"
                    ))
                })?;
                let t = ellipse_param(p, center, normal, major_axis, major_radius, minor_radius);
                let q = ellipse_point(center, normal, major_axis, major_radius, minor_radius, t);
                let pa = p.as_array();
                let qa = q.as_array();
                let dist =
                    ((pa[0] - qa[0]).powi(2) + (pa[1] - qa[1]).powi(2) + (pa[2] - qa[2]).powi(2))
                        .sqrt();
                let band = 1e-9 * (1.0 + major_radius);
                if dist > band {
                    return Err(YangError::MalformedTopology(format!(
                        "ellipse edge {e_idx}: endpoint vertex {v} is not on the ellipse \
                         (deviation {dist})"
                    )));
                }
                Ok(t)
            };
            if e.start != e.end {
                // ---- Minor-arc chain ----
                let t0 = param_of(e.start)?;
                let t1 = param_of(e.end)?;
                let sweep = (t1 - t0).rem_euclid(two_pi);
                if sweep <= 0.0 || !sweep.is_finite() {
                    return Err(YangError::MalformedTopology(format!(
                        "ellipse edge {e_idx}: degenerate arc sweep {sweep}"
                    )));
                }
                let m = ((sweep * n_seg as f64) / two_pi).ceil() as usize;
                let m = m.max(2);
                let mut chain: Vec<u32> = Vec::with_capacity(m + 1);
                chain.push(e.start);
                for k in 1..m {
                    let t = t0 + sweep * (k as f64) / (m as f64);
                    let vi = out_verts.len() as u32;
                    out_verts.push(ellipse_point(
                        center,
                        normal,
                        major_axis,
                        major_radius,
                        minor_radius,
                        t,
                    ));
                    sources.push(TessellationSource::BRepEdge {
                        edge: e_idx as u32,
                        t,
                    });
                    chain.push(vi);
                }
                chain.push(e.end);
                rim_rings.insert(e_idx as u32, chain);
            } else {
                // ---- Full ellipse: closed seam-anchored ring ----
                let t0 = param_of(e.start)?;
                let mut ring: Vec<u32> = Vec::with_capacity(n_seg);
                ring.push(e.start);
                for k in 1..n_seg {
                    let t = t0 + two_pi * (k as f64) / (n_seg as f64);
                    let vi = out_verts.len() as u32;
                    out_verts.push(ellipse_point(
                        center,
                        normal,
                        major_axis,
                        major_radius,
                        minor_radius,
                        t,
                    ));
                    sources.push(TessellationSource::BRepEdge {
                        edge: e_idx as u32,
                        t,
                    });
                    ring.push(vi);
                }
                rim_rings.insert(e_idx as u32, ring);
            }
        }

        // ---- Hyperbola chain pre-pass (KV16 hyperbola-arc re-entry, spec
        // `kv16_hyperbola_arc_vocabulary`). A `Curve::Hyperbola` boundary
        // edge (the axis-steep plane∩cone section a prior boolean leaves on
        // a re-entering body) builds a shared OPEN sample chain
        // `[start, Steiner…, end]` in `rim_rings`, mirroring the Ellipse
        // block above. The branch is unbounded, so `start == end` is
        // impossible and loud. Sampling is closed-form recursive parameter
        // bisection to the chord-sag bound `d_ε = 1e-2·max(a,b)` (the KV14
        // self-contained rule at the conic's scale); each Steiner vertex is
        // tagged with its exact parameter for the eval_source round-trip.
        for (e_idx, e) in edges.iter().enumerate() {
            let Curve::Hyperbola {
                center,
                normal,
                major_axis,
                semi_transverse,
                semi_conjugate,
            } = e.curve
            else {
                continue;
            };
            if e.start == e.end {
                return Err(YangError::MalformedTopology(format!(
                    "hyperbola edge {e_idx}: a closed hyperbola loop edge is impossible \
                     (the branch is unbounded)"
                )));
            }
            let d_eps = 1e-2 * semi_transverse.max(semi_conjugate);
            let param_of = |v: u32| -> Result<f64, YangError> {
                let p = verts.get(v as usize).map(|vv| vv.point).ok_or_else(|| {
                    YangError::MalformedTopology(format!(
                        "hyperbola edge {e_idx}: vertex {v} out of range"
                    ))
                })?;
                let t = hyperbola_param(p, center, normal, major_axis, semi_conjugate);
                let q = hyperbola_point(
                    center,
                    normal,
                    major_axis,
                    semi_transverse,
                    semi_conjugate,
                    t,
                );
                let pa = p.as_array();
                let qa = q.as_array();
                let dist =
                    ((pa[0] - qa[0]).powi(2) + (pa[1] - qa[1]).powi(2) + (pa[2] - qa[2]).powi(2))
                        .sqrt();
                let band = 1e-9 * (1.0 + semi_transverse.max(semi_conjugate));
                if dist > band {
                    return Err(YangError::MalformedTopology(format!(
                        "hyperbola edge {e_idx}: endpoint vertex {v} is not on the branch \
                         (deviation {dist})"
                    )));
                }
                Ok(t)
            };
            let t0 = param_of(e.start)?;
            let t1 = param_of(e.end)?;
            // Recursive chord-sag bisection (endpoints excluded, in
            // t0 → t1 order). Depth 12 ⇒ ≤ 4096 sub-chords — far beyond any
            // sane arc at the 1e-2 bound; exceeding it is loud.
            #[allow(clippy::too_many_arguments)]
            fn refine(
                e_idx: usize,
                center: Point3,
                normal: Vector3,
                major_axis: Vector3,
                a: f64,
                b: f64,
                seg: (f64, Point3, f64, Point3),
                d_eps: f64,
                depth: u32,
                out: &mut Vec<(f64, Point3)>,
            ) -> Result<(), YangError> {
                let (ta, pa, tb, pb) = seg;
                let tm = 0.5 * (ta + tb);
                let pm = hyperbola_point(center, normal, major_axis, a, b, tm);
                let mid = [
                    0.5 * (pa.as_array()[0] + pb.as_array()[0]),
                    0.5 * (pa.as_array()[1] + pb.as_array()[1]),
                    0.5 * (pa.as_array()[2] + pb.as_array()[2]),
                ];
                let pma = pm.as_array();
                let sag = ((pma[0] - mid[0]).powi(2)
                    + (pma[1] - mid[1]).powi(2)
                    + (pma[2] - mid[2]).powi(2))
                .sqrt();
                if sag <= d_eps {
                    return Ok(());
                }
                if depth == 0 || sag.is_nan() {
                    return Err(YangError::MalformedTopology(format!(
                        "hyperbola edge {e_idx}: chain refinement depth cap exceeded"
                    )));
                }
                refine(
                    e_idx,
                    center,
                    normal,
                    major_axis,
                    a,
                    b,
                    (ta, pa, tm, pm),
                    d_eps,
                    depth - 1,
                    out,
                )?;
                out.push((tm, pm));
                refine(
                    e_idx,
                    center,
                    normal,
                    major_axis,
                    a,
                    b,
                    (tm, pm, tb, pb),
                    d_eps,
                    depth - 1,
                    out,
                )
            }
            let p0 = verts[e.start as usize].point;
            let p1 = verts[e.end as usize].point;
            let mut steiner: Vec<(f64, Point3)> = Vec::new();
            refine(
                e_idx,
                center,
                normal,
                major_axis,
                semi_transverse,
                semi_conjugate,
                (t0, p0, t1, p1),
                d_eps,
                12,
                &mut steiner,
            )?;
            let mut chain: Vec<u32> = Vec::with_capacity(steiner.len() + 2);
            chain.push(e.start);
            for (t, p) in steiner {
                let vi = out_verts.len() as u32;
                out_verts.push(p);
                sources.push(TessellationSource::BRepEdge {
                    edge: e_idx as u32,
                    t,
                });
                chain.push(vi);
            }
            chain.push(e.end);
            rim_rings.insert(e_idx as u32, chain);
        }

        // ---- Per-face dispatch.
        let mut face_tri_ranges: Vec<std::ops::Range<usize>> = Vec::with_capacity(faces.len());
        for (f_idx, f) in faces.iter().enumerate() {
            let range_start = out_tris.len();
            // PR-KV7: ALL loops (outer + rings) must be segments for the
            // pure-planar paths — a seg-bounded face with a CIRCLE ring (a
            // box top with a recovered round hole re-entering the pipeline)
            // must route to the generalized curved CDT below, or the
            // seg-only CDT silently covers the hole.
            let all_line = f
                .outer_loop
                .iter()
                .chain(f.inner_loops.iter().flatten())
                .all(|&e_idx| matches!(edges[e_idx as usize].curve, Curve::LineSegment));

            match f.surface {
                Surface::Plane { normal, d } if all_line => {
                    // Route non-convex (reflex-vertex) outer loops, loops with
                    // COLLINEAR boundary runs (PR-YR27 — the fan would emit
                    // zero-area glue triangles the next arrangement drops),
                    // and any face with inner loops (holes) to the CDT path;
                    // strictly-convex hole-free faces keep the existing
                    // byte-for-byte fan path. (PR-NC1: a fan is valid only for
                    // strictly-convex, hole-free polygons; CDT handles the
                    // rest with exact coverage and no Steiner points.)
                    let needs_cdt = !f.inner_loops.is_empty()
                        || planar_outer_loop_fan_unsafe(f, edges, &out_verts, normal);

                    if needs_cdt {
                        tessellate_planar_cdt_face(
                            f_idx,
                            f,
                            edges,
                            normal,
                            &out_verts,
                            &mut out_tris,
                        )?;
                    } else {
                        // ===== Planar box path (UNCHANGED — Newell fan). =====
                        let mut face_verts: Vec<u32> = f
                            .outer_loop
                            .iter()
                            .map(|&e_idx| edges[e_idx as usize].start)
                            .collect();

                        let mut newell = [0.0f64; 3];
                        let m = face_verts.len();
                        for i in 0..m {
                            let vi = out_verts[face_verts[i] as usize].as_array();
                            let vj = out_verts[face_verts[(i + 1) % m] as usize].as_array();
                            newell[0] += (vi[1] - vj[1]) * (vi[2] + vj[2]);
                            newell[1] += (vi[2] - vj[2]) * (vi[0] + vj[0]);
                            newell[2] += (vi[0] - vj[0]) * (vi[1] + vj[1]);
                        }
                        let mag =
                            (newell[0] * newell[0] + newell[1] * newell[1] + newell[2] * newell[2])
                                .sqrt();
                        if mag < cad_primitives::MIN_FEATURE_SIZE * cad_primitives::MIN_FEATURE_SIZE
                        {
                            return Err(YangError::DegenerateFace { face: f_idx });
                        }
                        let n = normal.as_array();
                        let dot = newell[0] * n[0] + newell[1] * n[1] + newell[2] * n[2];
                        if dot < 0.0 {
                            face_verts.reverse();
                        }
                        for i in 1..face_verts.len() - 1 {
                            out_tris.push([face_verts[0], face_verts[i], face_verts[i + 1]]);
                        }
                        let _ = d;
                    }
                }
                Surface::Plane { normal, .. } => {
                    // ===== Curved-bounded planar faces. =====
                    // The full-circle DISK (single closed Circle outer loop,
                    // no rings) keeps the byte-for-byte cap fan. Everything
                    // else — annular-sector walls ([seg, arc, seg, arc]),
                    // holed circle caps (full-circle outer + full-circle
                    // ring), arbitrary mixed loops — goes through the
                    // generalized CDT over the spliced sample chains
                    // (PR-KV6b-1).
                    let is_disk = f.inner_loops.is_empty()
                        && f.outer_loop.len() == 1
                        && matches!(
                            &edges[f.outer_loop[0] as usize],
                            BRepEdge {
                                start,
                                end,
                                curve: Curve::Circle { .. },
                            } if start == end
                        );
                    if is_disk {
                        tessellate_cap_face(
                            f_idx,
                            f,
                            edges,
                            &rim_rings,
                            normal,
                            &mut out_verts,
                            &mut sources,
                            &mut out_tris,
                        )?;
                    } else {
                        tessellate_planar_curved_cdt_face(
                            f_idx,
                            f,
                            edges,
                            &rim_rings,
                            normal,
                            &out_verts,
                            &mut out_tris,
                        )?;
                    }
                }
                Surface::Cylinder {
                    axis_point,
                    axis_dir,
                    radius,
                } => {
                    tessellate_lateral_face(
                        f_idx,
                        f,
                        edges,
                        &rim_rings,
                        &inserted_rims,
                        &out_verts,
                        axis_point,
                        axis_dir,
                        radius,
                        &mut out_tris,
                    )?;
                }
                Surface::Sphere { center, radius } => {
                    tessellate_sphere_face(
                        f_idx,
                        f,
                        edges,
                        verts,
                        center,
                        radius,
                        &mut out_verts,
                        &mut sources,
                        &mut out_tris,
                    )?;
                }
                Surface::Cone {
                    apex,
                    axis_dir,
                    half_angle,
                } => {
                    tessellate_cone_face(
                        f_idx,
                        f,
                        edges,
                        &rim_rings,
                        &inserted_rims,
                        verts,
                        apex,
                        axis_dir,
                        half_angle,
                        &mut out_verts,
                        &mut sources,
                        &mut out_tris,
                    )?;
                }
                Surface::Torus {
                    center,
                    axis_dir,
                    major_radius,
                    minor_radius,
                } => {
                    tessellate_torus_face(
                        f_idx,
                        f,
                        edges,
                        &rim_rings,
                        center,
                        axis_dir,
                        major_radius,
                        minor_radius,
                        &mut out_verts,
                        &mut sources,
                        &mut out_tris,
                    )?;
                }
            }
            face_tri_ranges.push(range_start..out_tris.len());
        }

        Ok((
            Stage1Tess {
                verts: out_verts,
                sources,
                tris: out_tris,
                face_tri_ranges,
                chains: rim_rings,
            },
            inserted_rims,
        ))
    }
}

// =========================================================================
// PR-YR7 — curved Stage-1 geometry helpers
// =========================================================================

/// Normalize a `[f64; 3]`; returns the input unchanged if its length is below
/// `TAU_WORK` (defensive — callers pass real surface normals / axes).
pub(crate) fn normalize3(v: [f64; 3]) -> [f64; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len < cad_primitives::TAU_WORK {
        return v;
    }
    [v[0] / len, v[1] / len, v[2] / len]
}

/// Deterministic orthonormal in-plane basis `(e1, e2)` for the plane with
/// (not-necessarily-unit) normal `n` (PR-YR7, spec §2 "critical coupling").
///
/// USED BY BOTH Stage-1 sampling AND [`BRep::eval_source`] — if these two
/// disagree, the bijection round-trip fails. Construction:
/// 1. `nu = normalize(n)`.
/// 2. Seed = the world axis with the SMALLEST `|nu_i|` (ties broken x<y<z) —
///    the axis least aligned with `nu`, for numerical stability.
/// 3. `e1 = normalize(seed − (seed·nu)·nu)` (Gram–Schmidt).
/// 4. `e2 = nu × e1`.
///
/// `e1` and `e2` are unit and orthogonal to `nu` (and to each other). Note
/// `ortho_basis(-n)` and `ortho_basis(n)` share the SAME `e1` (the projection
/// is invariant to flipping `nu`) but have OPPOSITE `e2` (since `e2 = nu × e1`)
/// — the opposite-rim twist the lateral tessellation must compensate for.
pub(crate) fn ortho_basis(n: Vector3) -> (Vector3, Vector3) {
    let nu = normalize3(n.as_array());
    let abs = [nu[0].abs(), nu[1].abs(), nu[2].abs()];
    // Seed = world axis with smallest |component| (tie-break x < y < z).
    let seed = if abs[0] <= abs[1] && abs[0] <= abs[2] {
        [1.0, 0.0, 0.0]
    } else if abs[1] <= abs[2] {
        [0.0, 1.0, 0.0]
    } else {
        [0.0, 0.0, 1.0]
    };
    let sdotn = seed[0] * nu[0] + seed[1] * nu[1] + seed[2] * nu[2];
    let e1_raw = [
        seed[0] - sdotn * nu[0],
        seed[1] - sdotn * nu[1],
        seed[2] - sdotn * nu[2],
    ];
    let e1 = normalize3(e1_raw);
    // e2 = nu × e1.
    let e2 = [
        nu[1] * e1[2] - nu[2] * e1[1],
        nu[2] * e1[0] - nu[0] * e1[2],
        nu[0] * e1[1] - nu[1] * e1[0],
    ];
    (
        Vector3::new(e1[0], e1[1], e1[2]),
        Vector3::new(e2[0], e2[1], e2[2]),
    )
}

/// EXACT CCW tie-break for two rim points whose f64 frame angles COLLIDE
/// (ULP twins — spec `m8_holed_disc_coplanar_overlay` §8 increment 3): the
/// sign of the exact 2D cross product of their in-frame coordinates, computed
/// in rational arithmetic over the raw f64 inputs (products and sums of f64
/// values are exact in `RBig` — no rounding anywhere). `cross(a,b) > 0` means
/// `b` lies counterclockwise of `a` in the `(e1, e2)` frame, i.e. `a` orders
/// FIRST along increasing frame angle → `Less`. A zero cross (identical exact
/// direction; distinct rim points cannot subtend it) compares `Equal`.
///
/// Only valid for points whose angular separation is far below π (the callers
/// invoke it exclusively on bit-equal f64 angle keys, where the separation is
/// sub-ULP), since a bare cross sign cannot totally order antipodal points.
pub(crate) fn exact_rim_ccw_tiebreak(
    center: [f64; 3],
    e1: [f64; 3],
    e2: [f64; 3],
    pa: Point3,
    pb: Point3,
) -> std::cmp::Ordering {
    use crate::coplanar_overlay::rat;
    use dashu::rational::RBig;
    if std::env::var_os("TIEBREAK_NEUTER").is_some() {
        return std::cmp::Ordering::Equal;
    }
    let frame_coords = |p: Point3| -> Option<(RBig, RBig)> {
        let a = p.as_array();
        let w = [
            rat(a[0]).ok()? - rat(center[0]).ok()?,
            rat(a[1]).ok()? - rat(center[1]).ok()?,
            rat(a[2]).ok()? - rat(center[2]).ok()?,
        ];
        let dot = |v: &[f64; 3]| -> Option<RBig> {
            Some(&w[0] * rat(v[0]).ok()? + &w[1] * rat(v[1]).ok()? + &w[2] * rat(v[2]).ok()?)
        };
        Some((dot(&e1)?, dot(&e2)?))
    };
    match (frame_coords(pa), frame_coords(pb)) {
        (Some((xa, ya)), Some((xb, yb))) => {
            let cross = &xa * &yb - &ya * &xb;
            if cross > RBig::ZERO {
                std::cmp::Ordering::Less
            } else if cross < RBig::ZERO {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        }
        // Non-finite input (never produced by the tessellation) — keep stable.
        _ => std::cmp::Ordering::Equal,
    }
}

/// PR-NC1: is the outer loop of a planar, all-LineSegment face **non-convex**
/// (does it have a reflex vertex)?
///
/// Builds `face_verts` from each outer-loop edge's `.start` (the same vertex
/// order the fan path uses), projects them into the plane's intrinsic 2D frame
/// (`ortho_basis(normal)` — the SAME projection the CDT path uses, so the
/// reflex test and the triangulation agree), then walks consecutive 2D cross
/// products. The loop's overall orientation is the sign of its signed area; any
/// turn whose cross product has the OPPOSITE sign is a reflex vertex ⇒
/// non-convex. A near-zero cross (collinear vertices) is not reflex — but it
/// IS fan-unsafe, see below.
///
/// PR-YR27 (unmasked latent, found by the yr5c chained-subtract adversary
/// once Finding 3 let the chain proceed): a CONVEX loop with a COLLINEAR
/// boundary run is also routed to the CDT. A previous boolean's output face
/// legitimately carries collinear boundary subdivisions (arrangement
/// vertices on a straight face edge, e.g. a tunnel wall's rim subdivided by
/// the neighbor cap's mesh); re-fed as input, the fan from vertex 0 emits a
/// ZERO-AREA triangle whenever a collinear chain includes vertex 0's own
/// boundary edge (`fan(v0, c, b)` over collinear `v0—c—b`). That degenerate
/// glue triangle pairs the mesh locally, but the NEXT exact arrangement
/// drops it (zero-area tris cannot be embedded), leaving a T-junction and a
/// NON-watertight kept set. The CDT triangulates the same ring with every
/// boundary sub-segment as a constraint and emits positive-area triangles
/// only. Strictly-convex hole-free loops (every fixture box) keep the
/// byte-for-byte fan path.
pub(crate) fn planar_outer_loop_fan_unsafe(
    f: &BRepFace,
    edges: &[BRepEdge],
    out_verts: &[Point3],
    normal: Vector3,
) -> bool {
    let pts2d = project_loop_2d(&f.outer_loop, edges, out_verts, normal);
    let m = pts2d.len();
    if m < 4 {
        // A triangle is always convex.
        return false;
    }

    // Loop orientation = sign of the 2D signed (shoelace) area.
    let mut area2 = 0.0;
    for i in 0..m {
        let a = pts2d[i];
        let b = pts2d[(i + 1) % m];
        area2 += a[0] * b[1] - b[0] * a[1];
    }
    // Degenerate (zero-area) projection: treat as convex (the fan path's
    // own degeneracy guard will reject it downstream).
    if area2.abs() < cad_primitives::TAU_WORK {
        return false;
    }
    let orient = area2.signum();

    // Tolerance scaled to the loop's area so it is invariant to model scale.
    let eps = area2.abs() * 1e-9;
    for i in 0..m {
        let prev = pts2d[(i + m - 1) % m];
        let cur = pts2d[i];
        let next = pts2d[(i + 1) % m];
        let d1 = [cur[0] - prev[0], cur[1] - prev[1]];
        let d2 = [next[0] - cur[0], next[1] - cur[1]];
        let cross = d1[0] * d2[1] - d1[1] * d2[0];
        // A turn opposite the loop orientation is a reflex vertex.
        if cross * orient < -eps {
            return true;
        }
        // PR-YR27: a (near-)zero turn is a collinear boundary run — convex,
        // but fan-UNSAFE (see the function docs): route to the CDT.
        if cross.abs() <= eps {
            return true;
        }
    }
    false
}

/// PR-NC1: project an edge-index loop's vertices (each loop edge's `.start`)
/// into the plane's intrinsic 2D frame `ortho_basis(normal)`. Returns the 2D
/// coordinates in loop order. The 3D point of vertex `v` projects to
/// `(p·e1, p·e2)` (the origin offset cancels for in-plane analysis).
pub(crate) fn project_loop_2d(
    loop_edges: &[u32],
    edges: &[BRepEdge],
    out_verts: &[Point3],
    normal: Vector3,
) -> Vec<[f64; 2]> {
    let (e1, e2) = ortho_basis(normal);
    let e1a = e1.as_array();
    let e2a = e2.as_array();
    loop_edges
        .iter()
        .map(|&e_idx| {
            let p = out_verts[edges[e_idx as usize].start as usize].as_array();
            [
                p[0] * e1a[0] + p[1] * e1a[1] + p[2] * e1a[2],
                p[0] * e2a[0] + p[1] * e2a[1] + p[2] * e2a[2],
            ]
        })
        .collect()
}

/// PR-NC1: tessellate a planar, all-LineSegment face that is **non-convex** or
/// has **inner loops** via a constrained Delaunay triangulation
/// (`cherchi_rs::cdt_polygon_with_holes`).
///
/// Projects the outer loop + every inner loop into the plane's intrinsic 2D
/// frame (`ortho_basis(normal)`, matching the reflex test), builds a *local*
/// `Point2` pool with a `local → global out_verts index` map, triangulates, and
/// maps the local tri indices back to global indices. Each output triangle is
/// wound to agree with the plane normal (reusing `orient_tri`, the same sign
/// rule the fan path uses).
///
/// Pushes **no** new vertices — the output indexes only into existing
/// `out_verts`, so the `TessellationMap` 1:1-on-boundary bijection is preserved
/// (no Steiner points, no boundary subdivision).
/// PR-KV6b-1: expand a B-Rep edge-index loop into its mesh-vertex polyline,
/// splicing each `Curve::Circle` edge's cached sample chain (arc chains are
/// open `[start … end]`, full circles closed seam rings). Edge traversal
/// direction is derived from loop continuity; the returned polyline lists
/// each boundary vertex ONCE (no closing duplicate).
pub(crate) fn loop_polyline(
    f_idx: usize,
    loop_edges: &[u32],
    edges: &[BRepEdge],
    chains: &std::collections::BTreeMap<u32, Vec<u32>>,
) -> Result<Vec<u32>, YangError> {
    Ok(loop_polyline_attributed(f_idx, loop_edges, edges, chains)?
        .into_iter()
        .map(|(v, _)| v)
        .collect())
}

/// [`loop_polyline`] with per-vertex EDGE ATTRIBUTION: each emitted polyline
/// vertex is paired with the index of the loop edge that emitted it (so the
/// polyline segment starting at vertex *i* lies on edge `out[i].1`). The
/// Stage-0 mixed-face overlay arm uses this to mark curved sub-chords (spec
/// `m8_mixed_loop_coplanar_overlay` §8).
pub(crate) fn loop_polyline_attributed(
    f_idx: usize,
    loop_edges: &[u32],
    edges: &[BRepEdge],
    chains: &std::collections::BTreeMap<u32, Vec<u32>>,
) -> Result<Vec<(u32, u32)>, YangError> {
    let malformed = |msg: String| YangError::MalformedTopology(format!("face {f_idx}: {msg}"));

    // Single full-circle / full-ellipse loop: the chain IS the (closed)
    // polyline.
    if loop_edges.len() == 1 {
        let e = &edges[loop_edges[0] as usize];
        if matches!(e.curve, Curve::Circle { .. } | Curve::Ellipse { .. }) && e.start == e.end {
            return chains
                .get(&loop_edges[0])
                .map(|c| c.iter().map(|&v| (v, loop_edges[0])).collect())
                .ok_or_else(|| malformed(format!("chain for edge {} not built", loop_edges[0])));
        }
    }

    // Expansion of one directed edge: the vertex sequence from its
    // traversal origin up to (EXCLUDING) its destination.
    let expand = |e_idx: u32, forward: bool| -> Result<Vec<u32>, YangError> {
        let e = &edges[e_idx as usize];
        match e.curve {
            Curve::LineSegment => Ok(vec![if forward { e.start } else { e.end }]),
            Curve::Circle { .. } | Curve::Ellipse { .. } | Curve::Hyperbola { .. } => {
                let chain = chains
                    .get(&e_idx)
                    .ok_or_else(|| malformed(format!("chain for edge {e_idx} not built")))?;
                if e.start == e.end {
                    return Err(malformed(format!(
                        "full-circle/full-ellipse edge {e_idx} inside a multi-edge loop"
                    )));
                }
                let mut seq: Vec<u32> = if forward {
                    chain[..chain.len() - 1].to_vec()
                } else {
                    chain[1..].iter().rev().copied().collect()
                };
                if seq.is_empty() {
                    seq.push(if forward { e.start } else { e.end });
                }
                Ok(seq)
            }
            _ => Err(malformed(format!(
                "loop edge {e_idx} carries an unsupported curve for Stage-1 ingestion"
            ))),
        }
    };

    // Walk with continuity, trying the first edge forward then backward.
    'attempt: for first_forward in [true, false] {
        let e0 = &edges[loop_edges[0] as usize];
        let mut cur = if first_forward { e0.start } else { e0.end };
        let mut poly: Vec<(u32, u32)> = Vec::new();
        for &e_idx in loop_edges {
            let e = &edges[e_idx as usize];
            let forward = if e.start == cur {
                true
            } else if e.end == cur {
                false
            } else {
                continue 'attempt;
            };
            poly.extend(expand(e_idx, forward)?.into_iter().map(|v| (v, e_idx)));
            cur = if forward { e.end } else { e.start };
        }
        // Closure: the walk must return to its origin.
        if cur == poly[0].0 {
            return Ok(poly);
        }
    }
    Err(malformed("loop is not edge-continuous".to_string()))
}

/// PR-KV6b-1: CDT tessellation of a planar face whose loops mix straight and
/// `Curve::Circle` edges (annular sectors, holed circle caps, …). The
/// boundary polylines splice the SHARED per-edge sample chains
/// ([`loop_polyline`]), so faces meeting along an arc emit identical sample
/// vertices — the watertightness mechanism. Triangulation + orientation are
/// exactly the all-segment CDT path's (no Steiner points, no boundary
/// subdivision).
#[allow(clippy::too_many_arguments)]
pub(crate) fn tessellate_planar_curved_cdt_face(
    f_idx: usize,
    f: &BRepFace,
    edges: &[BRepEdge],
    chains: &std::collections::BTreeMap<u32, Vec<u32>>,
    normal: Vector3,
    out_verts: &[Point3],
    out_tris: &mut Vec<[u32; 3]>,
) -> Result<(), YangError> {
    if f.reversed {
        return Err(YangError::MalformedTopology(format!(
            "face {f_idx}: a planar face must carry its sense in the plane normal,              not `reversed`"
        )));
    }
    let (e1, e2) = ortho_basis(normal);
    let e1a = e1.as_array();
    let e2a = e2.as_array();
    let project = |g: u32| -> cad_primitives::Point2 {
        let p = out_verts[g as usize].as_array();
        cad_primitives::Point2::new(
            p[0] * e1a[0] + p[1] * e1a[1] + p[2] * e1a[2],
            p[0] * e2a[0] + p[1] * e2a[1] + p[2] * e2a[2],
        )
    };

    let mut local_verts: Vec<cad_primitives::Point2> = Vec::new();
    let mut global_of_local: Vec<u32> = Vec::new();
    let mut local_of_global: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
    let mut intern = |g: u32,
                      local_verts: &mut Vec<cad_primitives::Point2>,
                      global_of_local: &mut Vec<u32>|
     -> u32 {
        if let Some(&l) = local_of_global.get(&g) {
            return l;
        }
        let l = local_verts.len() as u32;
        local_verts.push(project(g));
        global_of_local.push(g);
        local_of_global.insert(g, l);
        l
    };

    let outer_poly = loop_polyline(f_idx, &f.outer_loop, edges, chains)?;
    let outer_local: Vec<u32> = outer_poly
        .iter()
        .map(|&g| intern(g, &mut local_verts, &mut global_of_local))
        .collect();
    let mut holes_local: Vec<Vec<u32>> = Vec::new();
    for inner in &f.inner_loops {
        let poly = loop_polyline(f_idx, inner, edges, chains)?;
        holes_local.push(
            poly.iter()
                .map(|&g| intern(g, &mut local_verts, &mut global_of_local))
                .collect(),
        );
    }

    // Diagnostic probe (env-gated, zero-cost off): dump the exact CDT inputs
    // (bit-precise) + outputs for one face, to extract minimal repros of
    // boundary-conformality failures.
    let cdt_probe = std::env::var("YANG_CDT_PROBE")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .map(|want| want == f_idx)
        .unwrap_or(false);
    if cdt_probe {
        eprintln!("[cdt-probe] face {f_idx}: verts {}", local_verts.len());
        for (i, p) in local_verts.iter().enumerate() {
            eprintln!(
                "[cdt-probe] v {i} = ({:?}, {:?}) bits=({:#x},{:#x})",
                p.x(),
                p.y(),
                p.x().to_bits(),
                p.y().to_bits()
            );
        }
        eprintln!("[cdt-probe] outer = {outer_local:?}");
        eprintln!("[cdt-probe] holes = {holes_local:?}");
    }
    // FLOOD-FILL variant (M8 holed-disc increment 3, spec
    // `m8_holed_disc_coplanar_overlay` §8): rim-override ULP twins put femto
    // slivers along the boundary chords, and the plain variant's f64 centroid
    // parity misclassifies them (the F0047 "parity slitting" class) — the cap
    // then disagrees with the shared rim ring and the Stage-0 mesh goes
    // non-manifold. The flood-fill variant classifies the outer region
    // topologically and (since increment 3) hole parity exactly; kernel-v2's
    // render cores made the same migration in `kv2_cdt_triangulation_core`.
    let local_tris = cherchi_rs::triangulation::cdt_polygon_with_holes_floodfill(
        &local_verts,
        &outer_local,
        &holes_local,
    )
    .map_err(|e| {
        YangError::MalformedTopology(format!("face {f_idx}: CDT triangulation failed: {e}"))
    })?;
    if cdt_probe {
        eprintln!("[cdt-probe] tris = {local_tris:?}");
    }

    let nu = normalize3(normal.as_array());
    for t in &local_tris {
        let mut tri = [
            global_of_local[t[0] as usize],
            global_of_local[t[1] as usize],
            global_of_local[t[2] as usize],
        ];
        orient_tri(out_verts, &mut tri, nu);
        out_tris.push(tri);
    }
    Ok(())
}

pub(crate) fn tessellate_planar_cdt_face(
    f_idx: usize,
    f: &BRepFace,
    edges: &[BRepEdge],
    normal: Vector3,
    out_verts: &[Point3],
    out_tris: &mut Vec<[u32; 3]>,
) -> Result<(), YangError> {
    // Build local 2D pool + local→global map. Each loop vertex is keyed by its
    // global `out_verts` index so shared vertices map to one local index.
    let (e1, e2) = ortho_basis(normal);
    let e1a = e1.as_array();
    let e2a = e2.as_array();
    let project = |g: u32| -> cad_primitives::Point2 {
        let p = out_verts[g as usize].as_array();
        cad_primitives::Point2::new(
            p[0] * e1a[0] + p[1] * e1a[1] + p[2] * e1a[2],
            p[0] * e2a[0] + p[1] * e2a[1] + p[2] * e2a[2],
        )
    };

    let mut local_verts: Vec<cad_primitives::Point2> = Vec::new();
    let mut global_of_local: Vec<u32> = Vec::new();
    let mut local_of_global: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();

    let intern = |g: u32,
                  local_verts: &mut Vec<cad_primitives::Point2>,
                  global_of_local: &mut Vec<u32>,
                  local_of_global: &mut std::collections::HashMap<u32, u32>|
     -> u32 {
        if let Some(&l) = local_of_global.get(&g) {
            return l;
        }
        let l = local_verts.len() as u32;
        local_verts.push(project(g));
        global_of_local.push(g);
        local_of_global.insert(g, l);
        l
    };

    let loop_to_local = |loop_edges: &[u32],
                         local_verts: &mut Vec<cad_primitives::Point2>,
                         global_of_local: &mut Vec<u32>,
                         local_of_global: &mut std::collections::HashMap<u32, u32>|
     -> Vec<u32> {
        loop_edges
            .iter()
            .map(|&e_idx| {
                let g = edges[e_idx as usize].start;
                intern(g, local_verts, global_of_local, local_of_global)
            })
            .collect()
    };

    let outer_local = loop_to_local(
        &f.outer_loop,
        &mut local_verts,
        &mut global_of_local,
        &mut local_of_global,
    );
    let holes_local: Vec<Vec<u32>> = f
        .inner_loops
        .iter()
        .map(|inner| {
            loop_to_local(
                inner,
                &mut local_verts,
                &mut global_of_local,
                &mut local_of_global,
            )
        })
        .collect();

    let local_tris = cherchi_rs::cdt_polygon_with_holes(&local_verts, &outer_local, &holes_local)
        .map_err(|e| {
        YangError::MalformedTopology(format!("face {f_idx}: CDT triangulation failed: {e}"))
    })?;

    let nu = normalize3(normal.as_array());
    for t in &local_tris {
        let mut tri = [
            global_of_local[t[0] as usize],
            global_of_local[t[1] as usize],
            global_of_local[t[2] as usize],
        ];
        orient_tri(out_verts, &mut tri, nu);
        out_tris.push(tri);
    }
    Ok(())
}

/// PR-YR7: tessellate a planar disk cap bounded by a single `Curve::Circle`
/// edge. A new center Steiner vertex (source `BRepFace { face, u: 0, v: 0 }`,
/// which `eval_source` maps to the plane origin = the rim center) fans over the
/// cached rim ring → `N` triangles, wound to agree with the cap plane normal.
#[allow(clippy::too_many_arguments)]
pub(crate) fn tessellate_cap_face(
    f_idx: usize,
    f: &BRepFace,
    edges: &[BRepEdge],
    rim_rings: &std::collections::BTreeMap<u32, Vec<u32>>,
    normal: Vector3,
    out_verts: &mut Vec<Point3>,
    sources: &mut Vec<TessellationSource>,
    out_tris: &mut Vec<[u32; 3]>,
) -> Result<(), YangError> {
    // Find the (single) Circle boundary edge.
    let circle_edges: Vec<u32> = f
        .outer_loop
        .iter()
        .copied()
        .filter(|&e| matches!(edges[e as usize].curve, Curve::Circle { .. }))
        .collect();
    if circle_edges.len() != 1 {
        return Err(YangError::MalformedTopology(format!(
            "face {f_idx}: planar cap must be bounded by exactly one Circle edge, found {}",
            circle_edges.len()
        )));
    }
    let ring = rim_rings.get(&circle_edges[0]).ok_or_else(|| {
        YangError::MalformedTopology(format!(
            "face {f_idx}: rim ring for edge {} not built",
            circle_edges[0]
        ))
    })?;
    let nseg = ring.len();
    if nseg < 3 {
        return Err(YangError::MalformedTopology(format!(
            "face {f_idx}: cap rim ring has {nseg} samples (< 3)"
        )));
    }

    // Center Steiner vertex = the rim center. For a `Curve::Circle` boundary the
    // center equals the cap plane origin; we read it from the circle to keep it
    // exact, and tag its source so `eval_source` reproduces it.
    let Curve::Circle { center, .. } = edges[circle_edges[0] as usize].curve else {
        return Err(YangError::MalformedTopology(format!(
            "face {f_idx}: cap boundary edge is not a Circle"
        )));
    };
    // The center Steiner vertex sits at the rim center. Its source is the cap
    // face's surface params `(u, v)` such that `eval_source` reproduces it:
    // `center = O + u·e1 + v·e2`, `O = −d·n_unit`. Solve `u = (center−O)·e1`,
    // `v = (center−O)·e2` (e1,e2 orthonormal). For a rim center that already
    // lies on the world-origin normal line (the unit cylinder) `O == center`
    // and `u = v = 0`, but the general off-origin cap needs the offset.
    let (e1c, e2c) = ortho_basis(normal);
    let nuc = normalize3(normal.as_array());
    let dval = match f.surface {
        Surface::Plane { d, .. } => d,
        _ => 0.0,
    };
    let o = [-dval * nuc[0], -dval * nuc[1], -dval * nuc[2]];
    let cc = center.as_array();
    let rel = [cc[0] - o[0], cc[1] - o[1], cc[2] - o[2]];
    let e1ca = e1c.as_array();
    let e2ca = e2c.as_array();
    let u_param = rel[0] * e1ca[0] + rel[1] * e1ca[1] + rel[2] * e1ca[2];
    let v_param = rel[0] * e2ca[0] + rel[1] * e2ca[1] + rel[2] * e2ca[2];
    let center_vi = out_verts.len() as u32;
    out_verts.push(center);
    sources.push(TessellationSource::BRepFace {
        face: f_idx as u32,
        u: u_param,
        v: v_param,
    });

    let nu = normalize3(normal.as_array());
    // Fan: triangle (center, ring[k], ring[k+1]); orient to the plane normal.
    for k in 0..nseg {
        let a = ring[k];
        let bnext = ring[(k + 1) % nseg];
        let mut tri = [center_vi, a, bnext];
        orient_tri(out_verts, &mut tri, nu);
        out_tris.push(tri);
    }
    Ok(())
}

/// KV14: the analytic surface a holed/partial curved lateral lives on. It
/// selects the param-space u-scale (`u = r·θ'`) and the outward normal used
/// when [`tessellate_lateral_holed_cdt`] maps triangles back to 3D: a cylinder
/// has a constant radius, a cone's radius grows linearly with the axial
/// distance from its apex (`r = |v|·tan α`), so its u-scale is per-vertex.
#[derive(Clone, Copy)]
pub(crate) enum LateralKind {
    Cylinder { radius: f64 },
    Cone { half_angle: f64 },
}

/// KV14 Slice A (spec `yang_stage1_curved_holed_patch`): tessellate a cylinder
/// lateral **holed patch** — a curved lateral whose boundary carries one or
/// more inner loops (a hole punched by a previous boolean) — via an isometric
/// **unroll to (u = r·θ, v = axial) parameter space** followed by the same
/// boundary-only constrained Delaunay triangulation the planar curved path uses
/// (`cdt_polygon_with_holes_floodfill`).
///
/// This is the general path: it makes no assumption about the outer loop being
/// a canonical tube or a 2-arc strip. It requires only that every boundary edge
/// (outer + inner) samples into a polyline via [`loop_polyline`] — Line, Arc
/// (`Curve::Circle` with `start != end`), Ellipse arc / full ellipse (KV14
/// ellipse-arc re-entry; a single-edge full-circle or full-ellipse loop takes
/// the closed-chain head case). A multi-edge loop containing a FULL-circle rim
/// or a `SurfacePair` (true degree-4) edge is a later slice and errors loudly
/// here (`loop_polyline` rejects both).
///
/// The θ **branch cut** is placed in the largest angular gap of the patch's
/// boundary so the unroll is contiguous (Slice A handles bounded patches with a
/// gap; a patch that wraps a full 2π has no gap and is a later slice).
///
/// KV14 Slice E: the same path serves CONES. A cone's radius grows linearly with
/// axial distance from its apex, so the `u = r·θ'` scale is per-vertex
/// (`r = |v|·tan α`, `v` = axial-from-apex) rather than constant, and the map-back
/// normal is the tilted cone normal. [`LateralKind`] selects between the two.
#[allow(clippy::too_many_arguments)]
pub(crate) fn tessellate_lateral_holed_cdt(
    f_idx: usize,
    f: &BRepFace,
    edges: &[BRepEdge],
    chains: &std::collections::BTreeMap<u32, Vec<u32>>,
    out_verts: &[Point3],
    axis_point: Point3,
    axis_dir: Vector3,
    kind: LateralKind,
    out_tris: &mut Vec<[u32; 3]>,
) -> Result<(), YangError> {
    let au = normalize3(axis_dir.as_array());
    let ap = axis_point.as_array();
    // Frame ⟂ the axis for the azimuth angle. `ortho_basis` is deterministic.
    let (e1, e2) = ortho_basis(axis_dir);
    let e1a = e1.as_array();
    let e2a = e2.as_array();
    // Raw (θ, v) of a global vertex in the axis frame; θ ∈ (−π, π].
    let raw = |g: u32| -> (f64, f64) {
        let p = out_verts[g as usize].as_array();
        let w = [p[0] - ap[0], p[1] - ap[1], p[2] - ap[2]];
        let v = w[0] * au[0] + w[1] * au[1] + w[2] * au[2];
        let x = w[0] * e1a[0] + w[1] * e1a[1] + w[2] * e1a[2];
        let y = w[0] * e2a[0] + w[1] * e2a[1] + w[2] * e2a[2];
        (y.atan2(x), v)
    };

    // Sample every boundary loop into a global-vertex polyline. `loop_polyline`
    // handles Line + Arc (Circle start≠end); a full-circle rim or degree-4 edge
    // is rejected there (loud, typed — the later-slice boundary).
    let outer_poly = loop_polyline(f_idx, &f.outer_loop, edges, chains)?;
    let mut inner_polys: Vec<Vec<u32>> = Vec::with_capacity(f.inner_loops.len());
    for inner in &f.inner_loops {
        inner_polys.push(loop_polyline(f_idx, inner, edges, chains)?);
    }

    let two_pi = 2.0 * std::f64::consts::PI;

    // Classify each boundary loop by axial winding BEFORE choosing the seam. An
    // ENCIRCLING loop (|Σ Δθ| ≈ 2π) is a rim / v-extent boundary of the periodic
    // cylindrical strip — unrolled it degenerates to a monotone u-chain with ~0
    // enclosed area, so it cannot be a CDT "hole" (Slice A's polygon-with-holes
    // model only fits a non-wrapping partial patch). A loop that nets ~0 winding
    // is a genuine interior window. This is KV14 Slice B — the wrapping topology
    // the corpus actually produces (spec `yang_stage1_curved_holed_patch`).
    let winding = |poly: &[u32]| -> f64 {
        let n = poly.len();
        let mut sum = 0.0;
        for i in 0..n {
            let (t0, _) = raw(poly[i]);
            let (t1, _) = raw(poly[(i + 1) % n]);
            let mut d = t1 - t0;
            while d > std::f64::consts::PI {
                d -= two_pi;
            }
            while d < -std::f64::consts::PI {
                d += two_pi;
            }
            sum += d;
        }
        sum
    };
    let encircles = |poly: &[u32]| winding(poly).abs() > 1.5 * std::f64::consts::PI;
    let mut encircling: Vec<&Vec<u32>> = Vec::new();
    let mut windows: Vec<&Vec<u32>> = Vec::new();
    for poly in std::iter::once(&outer_poly).chain(inner_polys.iter()) {
        if encircles(poly) {
            encircling.push(poly);
        } else {
            windows.push(poly);
        }
    }

    // Branch cut: place the seam in the largest angular gap so the unroll is
    // contiguous AND — for a periodic strip — the seam AVOIDS the interior
    // windows (a window straddling the cut would split into two u-fragments and
    // break the CDT). When windows exist inside a wrapping strip, choose the cut
    // from the WINDOW vertices' angular coverage (the seam lands in the widest
    // window-free wedge); otherwise (a pure strip, or a Slice A partial patch)
    // fall back to the full boundary.
    let mut angles: Vec<f64> = if !windows.is_empty() && !encircling.is_empty() {
        windows
            .iter()
            .flat_map(|w| w.iter())
            .map(|&g| raw(g).0)
            .collect()
    } else {
        std::iter::once(&outer_poly)
            .chain(inner_polys.iter())
            .flatten()
            .map(|&g| raw(g).0)
            .collect()
    };
    angles.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mut cut = std::f64::consts::PI; // fallback (no vertices ⇒ unreachable)
    let mut max_gap = -1.0f64;
    for w in angles.windows(2) {
        let gap = w[1] - w[0];
        if gap > max_gap {
            max_gap = gap;
            cut = 0.5 * (w[0] + w[1]);
        }
    }
    if let (Some(&first), Some(&last)) = (angles.first(), angles.last()) {
        // Wrap-around gap (across ±π): from the largest angle back to the
        // smallest through the branch of atan2.
        let wrap = (first + two_pi) - last;
        if wrap > max_gap {
            cut = last + 0.5 * wrap; // may exceed π; only used mod 2π below
        }
    }

    // Unroll into u = r·θ' where θ' ∈ [0, 2π) measured from the cut, v = axial.
    // Unroll a boundary vertex to 2D parameter space. A CYLINDER develops to a
    // rectangular strip (`u = r·θ'`, `v = axial`) — isometric, so the CDT sees
    // isotropic geometry. A CONE develops to an annular sector via its ISOMETRIC
    // development: slant `ℓ = |v|/cosα` (v = axial-from-apex), flattened angle
    // `ψ = θ'·sinα`, laid out as Cartesian `(ℓ cos ψ, ℓ sin ψ)`. The naive
    // `u = (v·tanα)·θ'` rectangular map is ANISOTROPIC (the u-scale grows with v),
    // which makes the CDT emit a skewed fan whose flat facets inflate the mapped
    // 3D area (a Schwarz-lantern artefact); the isometric development preserves
    // the cone's intrinsic metric so Delaunay yields well-shaped grid triangles.
    let project = |g: u32| -> cad_primitives::Point2 {
        let (theta, v) = raw(g);
        let un = (theta - cut).rem_euclid(two_pi);
        match kind {
            LateralKind::Cylinder { radius } => cad_primitives::Point2::new(radius * un, v),
            LateralKind::Cone { half_angle } => {
                let ell = v.abs() / half_angle.cos();
                let psi = un * half_angle.sin();
                cad_primitives::Point2::new(ell * psi.cos(), ell * psi.sin())
            }
        }
    };

    // Intern boundary vertices into a local param-space pool (each global vertex
    // maps 1:1 — the CDT is boundary-only, no Steiner points, so map-back is
    // just `global_of_local`). Mirrors `tessellate_planar_curved_cdt_face`.
    let mut local_verts: Vec<cad_primitives::Point2> = Vec::new();
    let mut global_of_local: Vec<u32> = Vec::new();
    let mut local_of_global: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
    let mut intern = |g: u32,
                      local_verts: &mut Vec<cad_primitives::Point2>,
                      global_of_local: &mut Vec<u32>|
     -> u32 {
        if let Some(&l) = local_of_global.get(&g) {
            return l;
        }
        let l = local_verts.len() as u32;
        local_verts.push(project(g));
        global_of_local.push(g);
        local_of_global.insert(g, l);
        l
    };

    // `encircling` / `windows` were partitioned by axial winding above (before
    // the seam was chosen). The kernel-v2 outer/inner labeling does not
    // distinguish rims from windows, so dispatch on the encircling count.
    let (outer_local, holes_local): (Vec<u32>, Vec<Vec<u32>>) = match encircling.len() {
        // Non-wrapping partial patch (Slice A): the labeled outer bounds the
        // patch, the labeled inners are genuine interior holes.
        0 => {
            let outer = outer_poly
                .iter()
                .map(|&g| intern(g, &mut local_verts, &mut global_of_local))
                .collect();
            let holes = inner_polys
                .iter()
                .map(|inner| {
                    inner
                        .iter()
                        .map(|&g| intern(g, &mut local_verts, &mut global_of_local))
                        .collect()
                })
                .collect();
            (outer, holes)
        }
        // Periodic strip (Slice B): the two encircling loops are the strip's
        // lower/upper v-boundaries. Open each at the seam into an ascending-u
        // chain, then lay the lower one forward and the upper one reversed →
        // ONE simple ribbon polygon. Any non-encircling loop is an interior
        // window carried as a CDT hole.
        2 => {
            // The ribbon unroll below assumes the CYLINDER's rectangular
            // (u = r·θ') layout — the seam wedge is a fixed 2π·r u-shift and the
            // strip is u-monotone. A cone develops to an annular sector where a
            // full-2π rim closes on itself (ψ spans 2π·sinα), so its periodic
            // frustum band (with an encircling window) needs polar seam handling
            // — a later Slice-E sub-slice. Fail loud rather than lay a cone into
            // the rectangular ribbon (which would fold).
            let radius = match kind {
                LateralKind::Cylinder { radius } => radius,
                LateralKind::Cone { .. } => {
                    return Err(YangError::MalformedTopology(format!(
                        "face {f_idx}: cone periodic strip (2 encircling rims) not yet \
                         supported (KV14 Slice E holed frustum band — later sub-slice)"
                    )));
                }
            };
            // Open a closed encircling loop into a u-ASCENDING chain. The loop
            // is u-monotone with a single seam wrap, but its traversal sense
            // depends on the winding sign (a +2π rim ascends in u, a −2π rim
            // descends), so anchor at the global-min-u vertex and walk toward
            // whichever neighbor continues upward — orientation-agnostic.
            let open_chain = |poly: &[u32]| -> Vec<u32> {
                let n = poly.len();
                let us: Vec<f64> = poly.iter().map(|&g| project(g).x()).collect();
                let m = (0..n)
                    .min_by(|&a, &b| {
                        us[a]
                            .partial_cmp(&us[b])
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .unwrap_or(0);
                let succ = us[(m + 1) % n];
                let pred = us[(m + n - 1) % n];
                if succ <= pred {
                    // Successor continues the ascending run.
                    (0..n).map(|k| poly[(m + k) % n]).collect()
                } else {
                    // Predecessor is the ascending run — walk backward from m.
                    (0..n).map(|k| poly[(m + n - k) % n]).collect()
                }
            };
            let mean_v = |poly: &[u32]| -> f64 {
                poly.iter().map(|&g| raw(g).1).sum::<f64>() / poly.len() as f64
            };
            let (lower, upper) = if mean_v(encircling[0]) <= mean_v(encircling[1]) {
                (encircling[0], encircling[1])
            } else {
                (encircling[1], encircling[0])
            };
            let bottom = open_chain(lower);
            let top = open_chain(upper);
            // Close the wrap: the ascending chain spans [u_min, u_max] but the
            // strip is periodic, so the seam segment from u_max back to
            // u_min+2πr is missing. Re-emit each chain's FIRST vertex as a
            // param-space DUPLICATE at u += 2πr (same global vertex, so it maps
            // back to the seam point) — the chain now spans a full 2π and the
            // ribbon's final quad covers the seam wedge.
            let seam_shift = two_pi * radius;
            let push_seam_dup = |g: u32,
                                 local_verts: &mut Vec<cad_primitives::Point2>,
                                 global_of_local: &mut Vec<u32>|
             -> u32 {
                let p = project(g);
                let l = local_verts.len() as u32;
                local_verts.push(cad_primitives::Point2::new(p.x() + seam_shift, p.y()));
                global_of_local.push(g);
                l
            };
            let mut outer: Vec<u32> = Vec::with_capacity(bottom.len() + top.len() + 2);
            for &g in &bottom {
                outer.push(intern(g, &mut local_verts, &mut global_of_local));
            }
            outer.push(push_seam_dup(
                bottom[0],
                &mut local_verts,
                &mut global_of_local,
            ));
            outer.push(push_seam_dup(
                top[0],
                &mut local_verts,
                &mut global_of_local,
            ));
            for &g in top.iter().rev() {
                outer.push(intern(g, &mut local_verts, &mut global_of_local));
            }
            let holes = windows
                .iter()
                .map(|window| {
                    window
                        .iter()
                        .map(|&g| intern(g, &mut local_verts, &mut global_of_local))
                        .collect()
                })
                .collect();
            (outer, holes)
        }
        n => {
            return Err(YangError::MalformedTopology(format!(
                "face {f_idx}: holed cylinder strip has {n} encircling boundaries \
                 (expected 0 for a partial patch or 2 for a periodic strip)"
            )));
        }
    };

    // T133 diagnosis probe (read-only, env-gated): dump the unrolled ribbon
    // polygon + holes for offline simplicity analysis.
    if std::env::var_os("YANG_T133_PROBE").is_some() {
        let dump = |tag: &str, lp: &[u32]| {
            let pts: Vec<(f64, f64, u32)> = lp
                .iter()
                .map(|&l| {
                    let p = local_verts[l as usize];
                    (p.x(), p.y(), global_of_local[l as usize])
                })
                .collect();
            eprintln!("[t133] face {f_idx} {tag}: {pts:?}");
        };
        dump("outer", &outer_local);
        for (k, h) in holes_local.iter().enumerate() {
            dump(&format!("hole{k}"), h);
        }
    }

    let local_tris = cherchi_rs::triangulation::cdt_polygon_with_holes_floodfill(
        &local_verts,
        &outer_local,
        &holes_local,
    )
    .map_err(|e| {
        YangError::MalformedTopology(format!("face {f_idx}: holed lateral CDT failed: {e}"))
    })?;

    // Map back to global 3D and orient by the analytic radial-outward normal
    // (inward if `reversed` — a cavity wall), matching `tessellate_lateral_face`.
    for t in &local_tris {
        let mut tri = [
            global_of_local[t[0] as usize],
            global_of_local[t[1] as usize],
            global_of_local[t[2] as usize],
        ];
        let mut n = match kind {
            LateralKind::Cylinder { .. } => radial_outward_normal(out_verts, &tri, ap, au),
            LateralKind::Cone { half_angle } => {
                cone_outward_normal(out_verts, &tri, axis_point, axis_dir, half_angle)
            }
        };
        if f.reversed {
            n = [-n[0], -n[1], -n[2]];
        }
        orient_tri(out_verts, &mut tri, n);
        out_tris.push(tri);
    }
    Ok(())
}

/// PR-YR7: tessellate the lateral tube of a cylinder (2 axial rings → `2N`
/// triangles, watertight via the shared cached rim rings).
///
/// HAZARD (spec §6): the bottom rim circle has `normal = −axis_dir`, the top
/// `+axis_dir`. `ortho_basis(−d)` and `ortho_basis(+d)` share `e1` but have
/// OPPOSITE `e2`, so the two rings — built at the same parameter angle `θ_k` in
/// their OWN frames — counter-rotate. To align quads by GEOMETRIC azimuth, the
/// bottom ring index for top azimuth `θ_k` is `(N − k) mod N` (its stored angle
/// is `2π − θ_k`). `ring[0]` of each rim is its seam vertex at azimuth 0, so
/// quad 0 aligns.
#[allow(clippy::too_many_arguments)]
pub(crate) fn tessellate_lateral_face(
    f_idx: usize,
    f: &BRepFace,
    edges: &[BRepEdge],
    rim_rings: &std::collections::BTreeMap<u32, Vec<u32>>,
    inserted_rims: &std::collections::BTreeSet<u32>,
    out_verts: &[Point3],
    axis_point: Point3,
    axis_dir: Vector3,
    _radius: f64,
    out_tris: &mut Vec<[u32; 3]>,
) -> Result<(), YangError> {
    // KV14 Slice A (spec `yang_stage1_curved_holed_patch`): a curved lateral
    // carrying inner loops (a hole from a previous boolean) has no structured
    // rim/strip pairing — route it to the general unroll+CDT path, which lays
    // the boundary chains flat in (u = r·θ, v = axial) parameter space and
    // triangulates the polygon-with-holes exactly (reusing the same CDT the
    // planar curved path uses). The hole-free structured arms below are left
    // 100% untouched.
    if !f.inner_loops.is_empty() {
        return tessellate_lateral_holed_cdt(
            f_idx,
            f,
            edges,
            rim_rings,
            out_verts,
            axis_point,
            axis_dir,
            LateralKind::Cylinder { radius: _radius },
            out_tris,
        );
    }
    // Dispatch on the boundary vocabulary:
    // - 2 FULL-circle rims (+ seam rulings)         → the canonical tube
    // - 2 ARCS + ruling segments (PR-KV6b-1)        → the partial patch strip
    // Anything else is MalformedTopology (loud).
    let full_rims: Vec<u32> = f
        .outer_loop
        .iter()
        .copied()
        .filter(|&e| {
            let ed = &edges[e as usize];
            matches!(ed.curve, Curve::Circle { .. }) && ed.start == ed.end
        })
        .collect();
    let arcs: Vec<u32> = f
        .outer_loop
        .iter()
        .copied()
        .filter(|&e| {
            let ed = &edges[e as usize];
            matches!(ed.curve, Curve::Circle { .. }) && ed.start != ed.end
        })
        .collect();

    let au = normalize3(axis_dir.as_array());
    let ap = axis_point.as_array();
    let rim_param = |e: u32| -> f64 {
        if let Curve::Circle { center, .. } = edges[e as usize].curve {
            let c = center.as_array();
            (c[0] - ap[0]) * au[0] + (c[1] - ap[1]) * au[1] + (c[2] - ap[2]) * au[2]
        } else {
            0.0
        }
    };
    // The stored normal's sense along the axis determines each chain's
    // angular direction (its frame's e2 = normal × e1 mirrors with the
    // normal). Two chains co-rotate when their normal signs agree.
    let rim_sense = |e: u32| -> f64 {
        if let Curve::Circle { normal, .. } = edges[e as usize].curve {
            let n = normalize3(normal.as_array());
            (n[0] * au[0] + n[1] * au[1] + n[2] * au[2]).signum()
        } else {
            1.0
        }
    };
    // Orientation target: outward radial for a solid lateral, inward for a
    // cavity wall (`reversed` — PR-KV6b-1, the washer's inner tube and the
    // partial revolve's inner-bore wall).
    let orient_target = |verts: &[Point3], tri: &[u32; 3]| -> [f64; 3] {
        let n = radial_outward_normal(verts, tri, ap, au);
        if f.reversed {
            [-n[0], -n[1], -n[2]]
        } else {
            n
        }
    };

    if full_rims.len() == 2 && arcs.is_empty() {
        // ===== Canonical tube (KV5a/M5 shape) =====
        let (mut bottom_e, mut top_e) = (full_rims[0], full_rims[1]);
        if rim_param(bottom_e) > rim_param(top_e) {
            std::mem::swap(&mut bottom_e, &mut top_e);
        }

        // PR-M8 disc-rim crossing: when EITHER rim carries inserted crossing
        // points the uniform index-pairing (`b_index`) no longer holds — pair
        // the two rings by GEOMETRIC azimuth instead. The uniform path is left
        // 100% untouched for all other cylinders.
        if inserted_rims.contains(&bottom_e) || inserted_rims.contains(&top_e) {
            return tessellate_lateral_azimuth_merge(
                f_idx, f, rim_rings, bottom_e, top_e, out_verts, axis_point, axis_dir, out_tris,
            );
        }

        let bottom_ring = rim_rings.get(&bottom_e).ok_or_else(|| {
            YangError::MalformedTopology(format!(
                "face {f_idx}: bottom rim ring {bottom_e} not built"
            ))
        })?;
        let top_ring = rim_rings.get(&top_e).ok_or_else(|| {
            YangError::MalformedTopology(format!("face {f_idx}: top rim ring {top_e} not built"))
        })?;
        let nseg = top_ring.len();
        if nseg < 3 || bottom_ring.len() != nseg {
            return Err(YangError::MalformedTopology(format!(
                "face {f_idx}: cylinder rims have mismatched / too-few samples"
            )));
        }

        // Connect by geometric azimuth. The classic shape stores bottom
        // normal = −axis, top = +axis (counter-rotating frames) ⇒ bottom
        // index (N−k). A washer INNER tube stores the mirrored senses ⇒
        // the rings co-rotate and align index-for-index. Generalize via the
        // product of the stored-normal senses (PR-KV6b-1).
        let co_rotating = rim_sense(bottom_e) * rim_sense(top_e) > 0.0;
        let b_index = |k: usize| -> usize {
            if co_rotating {
                k
            } else {
                (nseg - k) % nseg
            }
        };
        for k in 0..nseg {
            let kn = (k + 1) % nseg;
            let t0 = top_ring[k];
            let t1 = top_ring[kn];
            let b0 = bottom_ring[b_index(k)];
            let b1 = bottom_ring[b_index(kn)];
            for mut tri in [[b0, b1, t1], [b0, t1, t0]] {
                let n = orient_target(out_verts, &tri);
                orient_tri(out_verts, &mut tri, n);
                out_tris.push(tri);
            }
        }
        return Ok(());
    }

    if arcs.len() == 2
        && full_rims.is_empty()
        && f.outer_loop.iter().all(|&e| {
            matches!(
                edges[e as usize].curve,
                Curve::Circle { .. } | Curve::LineSegment
            )
        })
    {
        // ===== Partial patch (PR-KV6b-1): 2 sweep arcs + ruling segments =====
        let (mut bottom_e, mut top_e) = (arcs[0], arcs[1]);
        if rim_param(bottom_e) > rim_param(top_e) {
            std::mem::swap(&mut bottom_e, &mut top_e);
        }
        let bottom = rim_rings.get(&bottom_e).ok_or_else(|| {
            YangError::MalformedTopology(format!("face {f_idx}: arc chain {bottom_e} not built"))
        })?;
        let top = rim_rings.get(&top_e).ok_or_else(|| {
            YangError::MalformedTopology(format!("face {f_idx}: arc chain {top_e} not built"))
        })?;
        if bottom.len() != top.len() || bottom.len() < 2 {
            return Err(YangError::MalformedTopology(format!(
                "face {f_idx}: partial-cylinder arc chains have mismatched sample counts                  ({} vs {})",
                bottom.len(),
                top.len()
            )));
        }
        // Chains are open polylines [start … end]; with agreeing stored
        // senses they are azimuth-aligned index-for-index, with mirrored
        // senses index k pairs with (M−k).
        let m = bottom.len() - 1;
        let co_rotating = rim_sense(bottom_e) * rim_sense(top_e) > 0.0;
        let b_index = |k: usize| -> usize {
            if co_rotating {
                k
            } else {
                m - k
            }
        };
        for k in 0..m {
            let t0 = top[k];
            let t1 = top[k + 1];
            let b0 = bottom[b_index(k)];
            let b1 = bottom[b_index(k + 1)];
            for mut tri in [[b0, b1, t1], [b0, t1, t0]] {
                let n = orient_target(out_verts, &tri);
                orient_tri(out_verts, &mut tri, n);
                out_tris.push(tri);
            }
        }
        return Ok(());
    }

    // KV14 Slice D (spec `yang_stage1_curved_holed_patch`): a non-canonical
    // outer loop — no full-circle rims, only Line + Arc edges, but NOT the
    // structured 2-rim / 2-arc pattern (e.g. a partial patch bitten into an
    // irregular boundary by a prior boolean: R0053 = [L,A,A,A,L,A,A,A]). Route
    // it through the same general unroll + CDT path as the holed patch, with an
    // empty hole set: `tessellate_lateral_holed_cdt` classifies the single outer
    // loop by axial winding and lays it flat in (u = r·θ, v = axial) param space.
    // Full-circle rims (start == end) are excluded — those are the structured
    // arms above. Ellipse edges (the oblique-section boundary, KV14
    // ellipse-arc re-entry) sample into chains like arcs and route through
    // the same CDT; surface-pair (true degree-4) edges stay a loud wall.
    if full_rims.is_empty()
        && !f.outer_loop.is_empty()
        && f.outer_loop.iter().all(|&e| {
            matches!(
                edges[e as usize].curve,
                Curve::Circle { .. }
                    | Curve::LineSegment
                    | Curve::Ellipse { .. }
                    | Curve::Hyperbola { .. }
            )
        })
    {
        return tessellate_lateral_holed_cdt(
            f_idx,
            f,
            edges,
            rim_rings,
            out_verts,
            axis_point,
            axis_dir,
            LateralKind::Cylinder { radius: _radius },
            out_tris,
        );
    }

    Err(YangError::MalformedTopology(format!(
        "face {f_idx}: cylinder lateral must be bounded by exactly 2 full-circle rims          (canonical tube) or 2 arcs + ruling segments (partial patch); found {} full          rims and {} arcs",
        full_rims.len(),
        arcs.len()
    )))
}

/// PR-M8 disc-rim crossing: tessellate the canonical tube when one or both
/// rims carry inserted crossing points, so the uniform `(N−k)` index-pairing
/// no longer aligns the two rings.
///
/// Both rings are projected into ONE SHARED `ortho_basis(axis)` frame (so
/// their azimuths are GLOBAL, not per-rim-frame), sorted by azimuth, and
/// verified to present the SAME azimuth multiset within `tol = (2π/n)·0.25`
/// (the crossing point is shared between both rims by construction — Stage 0
/// projects each cap crossing onto the opposite rim — so a missing match is a
/// malformed input, NOT something to fudge). The quad strip then pairs
/// consecutive-by-azimuth vertices, reusing `orient_target`/`orient_tri`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn tessellate_lateral_azimuth_merge(
    f_idx: usize,
    f: &BRepFace,
    rim_rings: &std::collections::BTreeMap<u32, Vec<u32>>,
    bottom_e: u32,
    top_e: u32,
    out_verts: &[Point3],
    axis_point: Point3,
    axis_dir: Vector3,
    out_tris: &mut Vec<[u32; 3]>,
) -> Result<(), YangError> {
    // Orientation target (outward radial; inward for a cavity wall) — the
    // only cylinder-specific piece; the strip itself is shared with the
    // cone frustum band (increment 4 §4c).
    let au = normalize3(axis_dir.as_array());
    let ap = axis_point.as_array();
    let reversed = f.reversed;
    let orient = move |verts: &[Point3], tri: &[u32; 3]| -> [f64; 3] {
        let nrm = radial_outward_normal(verts, tri, ap, au);
        if reversed {
            [-nrm[0], -nrm[1], -nrm[2]]
        } else {
            nrm
        }
    };
    tessellate_band_azimuth_merge(
        f_idx, rim_rings, bottom_e, top_e, out_verts, axis_point, axis_dir, &orient, out_tris,
    )
}

/// Increment 4 §4c: per-triangle orientation target for the shared
/// azimuth-merge band strip (outward radial for a cylinder tube, tilted
/// cone normal for a frustum band; negated for a cavity wall).
type OrientTarget<'a> = &'a dyn Fn(&[Point3], &[u32; 3]) -> [f64; 3];

/// Increment 4 §4c: the shared azimuth-merge band strip — the cylinder
/// tube's inserted-rim pairing generalized over the orientation target so
/// the cone frustum band reuses it verbatim (multiset verification
/// included). Byte-identical for the cylinder path (dispatch-only
/// refactor, spec I6).
#[allow(clippy::too_many_arguments)]
pub(crate) fn tessellate_band_azimuth_merge(
    f_idx: usize,
    rim_rings: &std::collections::BTreeMap<u32, Vec<u32>>,
    bottom_e: u32,
    top_e: u32,
    out_verts: &[Point3],
    axis_point: Point3,
    axis_dir: Vector3,
    orient_target: OrientTarget<'_>,
    out_tris: &mut Vec<[u32; 3]>,
) -> Result<(), YangError> {
    let bottom_ring = rim_rings.get(&bottom_e).ok_or_else(|| {
        YangError::MalformedTopology(format!(
            "face {f_idx}: bottom rim ring {bottom_e} not built (azimuth merge)"
        ))
    })?;
    let top_ring = rim_rings.get(&top_e).ok_or_else(|| {
        YangError::MalformedTopology(format!(
            "face {f_idx}: top rim ring {top_e} not built (azimuth merge)"
        ))
    })?;
    let n = top_ring.len();
    if n < 3 || bottom_ring.len() != n {
        return Err(YangError::MalformedTopology(format!(
            "face {f_idx}: azimuth-merge rims have mismatched / too-few samples ({} vs {})",
            bottom_ring.len(),
            top_ring.len()
        )));
    }

    // ONE shared frame for both rims → global azimuth.
    let au = normalize3(axis_dir.as_array());
    let (e1, e2) = ortho_basis(Vector3::new(au[0], au[1], au[2]));
    let (e1, e2) = (e1.as_array(), e2.as_array());
    let ap = axis_point.as_array();
    let azimuth = |vi: u32| -> f64 {
        let p = out_verts[vi as usize].as_array();
        let w = [p[0] - ap[0], p[1] - ap[1], p[2] - ap[2]];
        let x = w[0] * e1[0] + w[1] * e1[1] + w[2] * e1[2];
        let y = w[0] * e2[0] + w[1] * e2[1] + w[2] * e2[2];
        y.atan2(x).rem_euclid(2.0 * std::f64::consts::PI)
    };

    // Sort each ring's vertex indices by global azimuth. ULP-twin crossing
    // points collide on the f64 azimuth key (spec
    // `m8_holed_disc_coplanar_overlay` §8 F3): break ties by the EXACT
    // angular order in the SHARED frame, so both rings sort identically and
    // the positional pairing below cannot twist the strip between twins.
    let sort_by_az = |ring: &[u32]| -> Vec<(f64, u32)> {
        let mut v: Vec<(f64, u32)> = ring.iter().map(|&vi| (azimuth(vi), vi)).collect();
        v.sort_by(|a, b| match a.0.total_cmp(&b.0) {
            std::cmp::Ordering::Equal => {
                exact_rim_ccw_tiebreak(ap, e1, e2, out_verts[a.1 as usize], out_verts[b.1 as usize])
            }
            ord => ord,
        });
        v
    };
    let bot = sort_by_az(bottom_ring);
    let top = sort_by_az(top_ring);

    // The two sorted rings are CIRCULAR sequences: the 0/2π cut can split
    // geometrically-identical azimuths across the wrap (a RECOVERED rim's
    // seam vertex at y = −ε maps to 2π−ε under `rem_euclid` while the other
    // rim's sits at exactly 0), shifting one sorted array by a slot — the
    // F0086 chained swiss-cheese wall (task #62). Align cyclically: pair
    // bot[k] ↔ top[(k+shift) % n], with the shift chosen so top[shift] is
    // circularly nearest bot[0]. The multiset check below is unchanged in
    // strength (pairwise within tol, no silent fudge) — it just compares the
    // cyclically aligned pairs.
    let two_pi = 2.0 * std::f64::consts::PI;
    let circ = |x: f64, y: f64| {
        let d = (x - y).abs();
        d.min(two_pi - d)
    };
    let mut shift = 0usize;
    let mut best = f64::INFINITY;
    for (j, t) in top.iter().enumerate() {
        let d = circ(bot[0].0, t.0);
        if d < best {
            best = d;
            shift = j;
        }
    }
    if std::env::var_os("YANG_SHIFT_NEUTER").is_some() {
        shift = 0;
    }

    // Verify the SAME azimuth multiset (no silent fudge): pairwise within tol.
    let tol = (two_pi / n as f64) * 0.25;
    for k in 0..n {
        let d = circ(bot[k].0, top[(k + shift) % n].0);
        if d > tol {
            // Diagnostic probe (env-gated): dump both sorted rings so a
            // multiset mismatch self-localizes (phase shift vs count skew vs
            // stray point).
            if std::env::var_os("YANG_AZMERGE_PROBE").is_some() {
                eprintln!(
                    "[azmerge-probe] face {f_idx}: n={n} tol={tol} shift={shift}\n  \
                     bottom: {:?}\n  top:    {:?}",
                    bot.iter().map(|(az, _)| *az).collect::<Vec<_>>(),
                    top.iter().map(|(az, _)| *az).collect::<Vec<_>>(),
                );
            }
            return Err(YangError::MalformedTopology(format!(
                "face {f_idx}: azimuth-merge rims disagree at index {k} (bottom {} vs top {}, \
                 shift {shift}, tol {tol})",
                bot[k].0,
                top[(k + shift) % n].0
            )));
        }
    }

    for k in 0..n {
        let kn = (k + 1) % n;
        let t0 = top[(k + shift) % n].1;
        let t1 = top[(kn + shift) % n].1;
        let b0 = bot[k].1;
        let b1 = bot[kn].1;
        for mut tri in [[b0, b1, t1], [b0, t1, t0]] {
            let nrm = orient_target(out_verts, &tri);
            orient_tri(out_verts, &mut tri, nrm);
            out_tris.push(tri);
        }
    }
    Ok(())
}

/// PR-YR12 (P2b): tessellate a closed solid-sphere face (one `Surface::Sphere`
/// bounded by a single `Curve::Circle` meridian seam) into a watertight
/// latitude/longitude grid mesh with a bijective `TessellationMap`.
///
/// Mirrors `tessellate_lateral_face` / `tessellate_cap_face` in style:
/// - Fixed z-up parameterization (spec §2):
///   `face_eval(u, v) = center + r·(cos v·cos u, cos v·sin u, sin v)`,
///   `u = 2π·i/n_lon`, `v = −π/2 + π·j/n_lat`, seam at `u = 0`.
/// - Chord bound `d_ε = 1e-2 × 2r√3` (the AABB space diagonal of the sphere,
///   spec §3) — `n_lon` / `n_lat` are refined honestly; the bound is fixed.
/// - The two pole vertices are the B-Rep verts `seam.start` (south) /
///   `seam.end` (north), already seeded 1:1 into `out_verts`/`sources`, so they
///   are SHARED (single vertex each → watertight pole closure). The seam column
///   (`i = 0`) is REUSED via the modular wrap `(i+1)%n_lon` (no welding).
/// - Sources: poles → `BRepVertex`; seam column → `BRepEdge { seam, t }` (the
///   recovered seam-frame angle); interior columns → `BRepFace { f_idx, u, v }`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn tessellate_sphere_face(
    f_idx: usize,
    f: &BRepFace,
    edges: &[BRepEdge],
    verts: &[BRepVertex],
    center: Point3,
    radius: f64,
    out_verts: &mut Vec<Point3>,
    sources: &mut Vec<TessellationSource>,
    out_tris: &mut Vec<[u32; 3]>,
) -> Result<(), YangError> {
    use std::f64::consts::PI;

    // ---- Find the single Circle meridian seam edge in the outer loop.
    let circle_edges: Vec<u32> = f
        .outer_loop
        .iter()
        .copied()
        .filter(|&e| matches!(edges[e as usize].curve, Curve::Circle { .. }))
        .collect();
    if circle_edges.len() != 1 {
        return Err(YangError::MalformedTopology(format!(
            "face {f_idx}: sphere must be bounded by exactly one Circle seam edge, found {}",
            circle_edges.len()
        )));
    }
    let seam_edge_index = circle_edges[0];
    let seam = &edges[seam_edge_index as usize];
    let Curve::Circle { normal, .. } = seam.curve else {
        return Err(YangError::MalformedTopology(format!(
            "face {f_idx}: sphere seam edge {seam_edge_index} is not a Circle"
        )));
    };

    // ---- Pole B-Rep vertices (south = seam.start, north = seam.end). These are
    // already mesh verts 0..verts.len() (seeded 1:1), so REUSE the indices — no
    // duplicate pushes. Bounds-check the indices (P9: no panic on B-Rep data).
    let south_vi = seam.start;
    let north_vi = seam.end;
    if verts.get(south_vi as usize).is_none() {
        return Err(YangError::MalformedTopology(format!(
            "face {f_idx}: sphere seam south pole vertex {south_vi} out of range"
        )));
    }
    if verts.get(north_vi as usize).is_none() {
        return Err(YangError::MalformedTopology(format!(
            "face {f_idx}: sphere seam north pole vertex {north_vi} out of range"
        )));
    }

    // ---- Chord bound + honest grid refinement (spec §3). The bound `d_ε` is
    // FIXED at `1e-2·2r√3`; we only raise N (never widen the tolerance, P9/P10).
    //
    // The per-segment **arc** sagitta `r·(1−cos θ)` bounds deviation at edge
    // midpoints, but oracle 1 also samples each triangle's CENTROID, and a flat
    // triangle inscribed in the sphere dips inward more than its edge midpoints
    // (worst at the long, thin pole-fan triangles). To keep the centroid within
    // `d_ε` we size each segment to half the budget (`d_ε/2`) — this is honest
    // refinement (more triangles), NOT tolerance widening. The factor 2 leaves a
    // comfortable margin across the corpus (verified: worst centroid deviation
    // ≈ 0.82·d_ε), and the ratio is scale-invariant so one N pair fits all radii.
    let d_eps = sphere_chord_bound(radius);
    let seg_budget = d_eps / 2.0;
    // n_lon: smallest N ≥ 3 with r·(1 − cos(π/N)) ≤ d_ε/2 (equator chord).
    let mut n_lon = 3usize;
    if seg_budget > 0.0 {
        while radius * (1.0 - (PI / n_lon as f64).cos()) > seg_budget {
            n_lon += 1;
        }
    }
    // n_lat: smallest N ≥ 2 with r·(1 − cos(π/(2N))) ≤ d_ε/2 (meridian
    // half-circle of total turn π split into N segments → half-angle π/(2N)).
    let mut n_lat = 2usize;
    if seg_budget > 0.0 {
        while radius * (1.0 - (PI / (2.0 * n_lat as f64)).cos()) > seg_budget {
            n_lat += 1;
        }
    }

    // ---- Seam frame (for per-sample seam-angle recovery, mirroring the
    // cylinder `phi0`) and the z-up surface evaluator.
    let (e1, e2) = ortho_basis(normal);
    let e1a = e1.as_array();
    let e2a = e2.as_array();
    let cen = center.as_array();
    let face_eval = |u: f64, v: f64| -> [f64; 3] {
        let (cu, su) = (u.cos(), u.sin());
        let (cv, sv) = (v.cos(), v.sin());
        [
            cen[0] + radius * cv * cu,
            cen[1] + radius * cv * su,
            cen[2] + radius * sv,
        ]
    };

    // ---- Interior latitude rings j = 1..n_lat (n_lat-1 rings strictly between
    // the poles). rings[j-1] is the ring at latitude index j.
    let mut rings: Vec<Vec<u32>> = Vec::with_capacity(n_lat - 1);
    for j in 1..n_lat {
        let v_j = -PI / 2.0 + PI * (j as f64) / (n_lat as f64);
        let mut ring: Vec<u32> = Vec::with_capacity(n_lon);
        for i in 0..n_lon {
            let u_i = 2.0 * PI * (i as f64) / (n_lon as f64);
            let pos = face_eval(u_i, v_j);
            let vi = out_verts.len() as u32;
            out_verts.push(Point3::new(pos[0], pos[1], pos[2]));
            let src = if i == 0 {
                // Seam column → recover its angle in the seam circle's frame so
                // `eval_source(BRepEdge{seam, t})` reproduces this point exactly.
                let w = [pos[0] - cen[0], pos[1] - cen[1], pos[2] - cen[2]];
                let wx = w[0] * e1a[0] + w[1] * e1a[1] + w[2] * e1a[2];
                let wy = w[0] * e2a[0] + w[1] * e2a[1] + w[2] * e2a[2];
                TessellationSource::BRepEdge {
                    edge: seam_edge_index,
                    t: wy.atan2(wx),
                }
            } else {
                TessellationSource::BRepFace {
                    face: f_idx as u32,
                    u: u_i,
                    v: v_j,
                }
            };
            sources.push(src);
            ring.push(vi);
        }
        rings.push(ring);
    }

    // ---- Triangles, each oriented by the full outward radial normal.
    let mut push_oriented = |mut tri: [u32; 3], out_verts: &[Point3]| {
        let n = sphere_outward_normal(out_verts, &tri, center);
        orient_tri(out_verts, &mut tri, n);
        out_tris.push(tri);
    };

    // South fan (poles share a single vertex; seam column reused via wrap).
    let first = &rings[0];
    for i in 0..n_lon {
        push_oriented([south_vi, first[i], first[(i + 1) % n_lon]], out_verts);
    }
    // North fan.
    let last_idx = rings.len() - 1;
    let last = &rings[last_idx];
    for i in 0..n_lon {
        push_oriented([north_vi, last[(i + 1) % n_lon], last[i]], out_verts);
    }
    // Middle bands between consecutive interior rings (empty when n_lat == 2).
    for j in 0..rings.len() - 1 {
        let lo = rings[j].clone();
        let up = rings[j + 1].clone();
        for i in 0..n_lon {
            let inext = (i + 1) % n_lon;
            let (a, b, c, d) = (lo[i], lo[inext], up[inext], up[i]);
            push_oriented([a, b, c], out_verts);
            push_oriented([a, c, d], out_verts);
        }
    }

    Ok(())
}

/// PR-YR16 (P2c): tessellate a closed solid-cone lateral face (one
/// `Surface::Cone` bounded by a single base-rim `Curve::Circle`) into a
/// watertight apex fan with a bijective `TessellationMap`.
///
/// Spec §1/§2: the cone lateral is topologically a DISK — its only boundary is
/// the base circle, the apex a single interior singular point (no seam edge).
/// Because the cone is ruled (straight generators apex→rim, exactly on the
/// surface), the lateral is a PURE fan with NO interior rings: `N` triangles
/// (apex, `ring[k]`, `ring[(k+1) % N]`) over the cached base-rim ring. The apex
/// is the pre-seeded B-Rep vertex (`verts` are seeded 1:1 into `out_verts` at
/// the top of `BRep::new`), located by exact position match to
/// `Surface::Cone.apex` within `TAU_MODEL` and REUSED (no duplicate keeps
/// watertight + Euler valid). The base cap is tessellated by the existing
/// `tessellate_cap_face` over the SAME ring (the watertightness mechanism), and
/// each triangle is oriented outward via `cone_outward_normal` + `orient_tri`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn tessellate_cone_face(
    f_idx: usize,
    f: &BRepFace,
    edges: &[BRepEdge],
    rim_rings: &std::collections::BTreeMap<u32, Vec<u32>>,
    inserted_rims: &std::collections::BTreeSet<u32>,
    verts: &[BRepVertex],
    apex: Point3,
    axis_dir: Vector3,
    half_angle: f64,
    out_verts: &mut [Point3],
    _sources: &mut [TessellationSource],
    out_tris: &mut Vec<[u32; 3]>,
) -> Result<(), YangError> {
    // KV14 Slice E (spec `yang_stage1_curved_holed_patch`): a cone lateral
    // carrying inner loops (a hole from a previous boolean) has no structured
    // rim/strip pairing — route it to the shared unroll+CDT path, which lays the
    // boundary chains flat in (u = |v|·tanα·θ, v = axial-from-apex) parameter
    // space and triangulates the polygon-with-holes exactly. The hole-free
    // structured arms below are left 100% untouched.
    if !f.inner_loops.is_empty() {
        return tessellate_lateral_holed_cdt(
            f_idx,
            f,
            edges,
            rim_rings,
            out_verts,
            apex,
            axis_dir,
            LateralKind::Cone { half_angle },
            out_tris,
        );
    }
    // ---- Find the base-rim Circle edges, split CLOSED rims (start == end)
    // from ARCS (start != end — a partial cone patch boundary, KV6c
    // increment 5). Treating an open arc chain as a closed rim would
    // phantom-wrap the strip into silent wrong geometry, so the split is
    // load-bearing, mirroring the cylinder's canonical-vs-partial dispatch.
    let circle_edges: Vec<u32> = f
        .outer_loop
        .iter()
        .copied()
        .filter(|&e| {
            let ed = &edges[e as usize];
            matches!(ed.curve, Curve::Circle { .. }) && ed.start == ed.end
        })
        .collect();
    let arc_edges: Vec<u32> = f
        .outer_loop
        .iter()
        .copied()
        .filter(|&e| {
            let ed = &edges[e as usize];
            matches!(ed.curve, Curve::Circle { .. }) && ed.start != ed.end
        })
        .collect();
    // KV6c increment 5 (task #82): the partial cone STRIP — 2 sweep arcs at
    // the wall's two radii + slant ruling segments, the ruled analog of
    // `tessellate_lateral_face`'s partial cylinder arm. Arc chains are
    // sampled by SWEEP fraction of the shared n_seg (radius-independent), so
    // the two chains of one wall always carry identical counts. NEVER the
    // frustum-band arm below (whose % nseg wrap assumes closed rings).
    if arc_edges.len() == 2
        && circle_edges.is_empty()
        && f.outer_loop.iter().all(|&e| {
            matches!(
                edges[e as usize].curve,
                Curve::Circle { .. } | Curve::LineSegment
            )
        })
    {
        let au = normalize3(axis_dir.as_array());
        let ap = apex.as_array();
        // Axial coordinate (from the apex) and stored-normal sense of an arc.
        let rim_param = |e: u32| -> f64 {
            if let Curve::Circle { center, .. } = edges[e as usize].curve {
                let c = center.as_array();
                (c[0] - ap[0]) * au[0] + (c[1] - ap[1]) * au[1] + (c[2] - ap[2]) * au[2]
            } else {
                0.0
            }
        };
        let rim_sense = |e: u32| -> f64 {
            if let Curve::Circle { normal, .. } = edges[e as usize].curve {
                let n = normalize3(normal.as_array());
                (n[0] * au[0] + n[1] * au[1] + n[2] * au[2]).signum()
            } else {
                1.0
            }
        };
        let (mut bottom_e, mut top_e) = (arc_edges[0], arc_edges[1]);
        if rim_param(bottom_e) > rim_param(top_e) {
            std::mem::swap(&mut bottom_e, &mut top_e);
        }
        let bottom = rim_rings.get(&bottom_e).ok_or_else(|| {
            YangError::MalformedTopology(format!(
                "face {f_idx}: cone arc chain {bottom_e} not built"
            ))
        })?;
        let top = rim_rings.get(&top_e).ok_or_else(|| {
            YangError::MalformedTopology(format!("face {f_idx}: cone arc chain {top_e} not built"))
        })?;
        if bottom.len() != top.len() || bottom.len() < 2 {
            return Err(YangError::MalformedTopology(format!(
                "face {f_idx}: partial-cone arc chains have mismatched sample counts \
                 ({} vs {})",
                bottom.len(),
                top.len()
            )));
        }
        // Chains are open polylines [start … end]; with agreeing stored
        // senses they are azimuth-aligned index-for-index, with mirrored
        // senses index k pairs with (M−k) — the cylinder partial-arm rule.
        let m = bottom.len() - 1;
        let co_rotating = rim_sense(bottom_e) * rim_sense(top_e) > 0.0;
        let b_index = |k: usize| -> usize {
            if co_rotating {
                k
            } else {
                m - k
            }
        };
        for k in 0..m {
            let t0 = top[k];
            let t1 = top[k + 1];
            let b0 = bottom[b_index(k)];
            let b1 = bottom[b_index(k + 1)];
            for mut tri in [[b0, b1, t1], [b0, t1, t0]] {
                let mut n = cone_outward_normal(out_verts, &tri, apex, axis_dir, half_angle);
                if f.reversed {
                    n = [-n[0], -n[1], -n[2]];
                }
                orient_tri(out_verts, &mut tri, n);
                out_tris.push(tri);
            }
        }
        return Ok(());
    }
    // KV14 Slice E (spec `yang_stage1_curved_holed_patch`): a non-canonical cone
    // outer loop — no full-circle rims, only Line + Arc edges, but NOT the
    // structured 2-arc strip above (e.g. a partial patch bitten into a multi-arc
    // boundary by a prior boolean: R0020 = [L,A,A,A,L,A,A,A], R0093 =
    // [L,A,A,L,A,A]). Route it through the shared unroll + CDT path with an empty
    // hole set; the 0-encircling branch lays the single outer loop flat in cone
    // param space. Ellipse edges (oblique-section boundaries, KV14 ellipse-arc
    // re-entry) sample into chains like arcs; surface-pair (true degree-4)
    // edges are rejected loudly by `loop_polyline` inside.
    if circle_edges.is_empty()
        && !f.outer_loop.is_empty()
        && f.outer_loop.iter().all(|&e| {
            matches!(
                edges[e as usize].curve,
                Curve::Circle { .. }
                    | Curve::LineSegment
                    | Curve::Ellipse { .. }
                    | Curve::Hyperbola { .. }
            )
        })
    {
        return tessellate_lateral_holed_cdt(
            f_idx,
            f,
            edges,
            rim_rings,
            out_verts,
            apex,
            axis_dir,
            LateralKind::Cone { half_angle },
            out_tris,
        );
    }
    // Any other arc-bearing cone boundary (mixed arcs+rims, a single arc, a
    // trim-loop patch) is outside the strip vocabulary — typed and loud.
    if !arc_edges.is_empty() {
        return Err(YangError::MalformedTopology(format!(
            "face {f_idx}: arc-bounded cone face outside the partial-strip vocabulary \
             (KV6c increment 5; {} arc edges, {} closed rims)",
            arc_edges.len(),
            circle_edges.len()
        )));
    }
    // A cone face is bounded by EITHER one base rim (an apex-pointed cone — a
    // topological disk fanned from the apex, PR-YR16) OR two rims at different
    // radii (a FRUSTUM band — the ruled analog of the cylinder tube, KV6c 5b;
    // kernel-v2 revolve produces these, since the profile cannot reach the
    // axis). Anything else is MalformedTopology (loud).
    match circle_edges.as_slice() {
        [rim_e] => {
            let ring = rim_rings.get(rim_e).ok_or_else(|| {
                YangError::MalformedTopology(format!(
                    "face {f_idx}: rim ring for edge {rim_e} not built"
                ))
            })?;
            let nseg = ring.len();
            if nseg < 3 {
                return Err(YangError::MalformedTopology(format!(
                    "face {f_idx}: cone rim ring has {nseg} samples (< 3)"
                )));
            }
            // Locate the pre-seeded apex mesh vertex by exact position match to
            // the cone's `apex` (within `TAU_MODEL`). The B-Rep verts are seeded
            // 1:1 into `out_verts` at the top of `BRep::new`, so a vertex's
            // B-Rep index IS its mesh index. REUSE it (no duplicate apex push →
            // watertight). No match → loud MalformedTopology.
            let ap = apex.as_array();
            let apex_vi = verts
                .iter()
                .position(|bv| {
                    let p = bv.point.as_array();
                    let dx = p[0] - ap[0];
                    let dy = p[1] - ap[1];
                    let dz = p[2] - ap[2];
                    (dx * dx + dy * dy + dz * dz).sqrt() <= cad_primitives::TAU_MODEL
                })
                .map(|i| i as u32)
                .ok_or_else(|| {
                    YangError::MalformedTopology(format!(
                        "face {f_idx}: cone apex {ap:?} matches no pre-seeded B-Rep vertex"
                    ))
                })?;
            // Apex fan: triangle (apex, ring[k], ring[(k+1) % N]); orient each
            // outward via the tilted cone normal.
            for k in 0..nseg {
                let mut tri = [apex_vi, ring[k], ring[(k + 1) % nseg]];
                let n = cone_outward_normal(out_verts, &tri, apex, axis_dir, half_angle);
                orient_tri(out_verts, &mut tri, n);
                out_tris.push(tri);
            }
            Ok(())
        }
        [rim_a, rim_b] => tessellate_cone_frustum_band(
            f_idx,
            f,
            edges,
            *rim_a,
            *rim_b,
            rim_rings,
            inserted_rims,
            apex,
            axis_dir,
            half_angle,
            out_verts,
            out_tris,
        ),
        other => Err(YangError::MalformedTopology(format!(
            "face {f_idx}: cone lateral must be bounded by ONE base rim (apex cone) or TWO \
             rims (frustum band); found {} circle edges (a cone on a non-circular boundary is \
             malformed topology)",
            other.len()
        ))),
    }
}

/// KV6c increment 5b: tessellate a FRUSTUM-band cone face — two full-circle
/// rims at different radii — as a ruled quad strip, the tilted-normal analog of
/// the cylinder canonical tube ([`tessellate_lateral_face`]). The rings pair by
/// azimuth exactly as the tube does (counter-rotating stored senses ⇒ `N − k`);
/// each triangle is oriented by [`cone_outward_normal`], negated for a cavity
/// bore (`f.reversed`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn tessellate_cone_frustum_band(
    f_idx: usize,
    f: &BRepFace,
    edges: &[BRepEdge],
    rim_a: u32,
    rim_b: u32,
    rim_rings: &std::collections::BTreeMap<u32, Vec<u32>>,
    inserted_rims: &std::collections::BTreeSet<u32>,
    apex: Point3,
    axis_dir: Vector3,
    half_angle: f64,
    out_verts: &[Point3],
    out_tris: &mut Vec<[u32; 3]>,
) -> Result<(), YangError> {
    // Increment 4 §4c: a frustum band with inserted rim-crossing points
    // routes through the shared azimuth-merge strip (multiset-verified),
    // exactly as the cylinder tube does — the uniform index-pairing below
    // assumes uniform sampling. The hole-free uniform path stays
    // byte-identical.
    if inserted_rims.contains(&rim_a) || inserted_rims.contains(&rim_b) {
        let reversed = f.reversed;
        let orient = move |verts: &[Point3], tri: &[u32; 3]| -> [f64; 3] {
            let mut n = cone_outward_normal(verts, tri, apex, axis_dir, half_angle);
            if reversed {
                n = [-n[0], -n[1], -n[2]];
            }
            n
        };
        // Order the rims by axial coordinate so bottom/top is
        // deterministic (mirrors the uniform arm's rim_param swap).
        let au0 = normalize3(axis_dir.as_array());
        let ap0 = apex.as_array();
        let rim_z = |e: u32| -> f64 {
            if let Curve::Circle { center, .. } = edges[e as usize].curve {
                let c = center.as_array();
                (c[0] - ap0[0]) * au0[0] + (c[1] - ap0[1]) * au0[1] + (c[2] - ap0[2]) * au0[2]
            } else {
                0.0
            }
        };
        let (mut bottom_e, mut top_e) = (rim_a, rim_b);
        if rim_z(bottom_e) > rim_z(top_e) {
            std::mem::swap(&mut bottom_e, &mut top_e);
        }
        return tessellate_band_azimuth_merge(
            f_idx, rim_rings, bottom_e, top_e, out_verts, apex, axis_dir, &orient, out_tris,
        );
    }
    let au = normalize3(axis_dir.as_array());
    let ap = apex.as_array();
    // Axial coordinate (from the apex) and stored-normal sense of a rim.
    let rim_param = |e: u32| -> f64 {
        if let Curve::Circle { center, .. } = edges[e as usize].curve {
            let c = center.as_array();
            (c[0] - ap[0]) * au[0] + (c[1] - ap[1]) * au[1] + (c[2] - ap[2]) * au[2]
        } else {
            0.0
        }
    };
    let rim_sense = |e: u32| -> f64 {
        if let Curve::Circle { normal, .. } = edges[e as usize].curve {
            let n = normalize3(normal.as_array());
            (n[0] * au[0] + n[1] * au[1] + n[2] * au[2]).signum()
        } else {
            1.0
        }
    };
    let (mut bottom_e, mut top_e) = (rim_a, rim_b);
    if rim_param(bottom_e) > rim_param(top_e) {
        std::mem::swap(&mut bottom_e, &mut top_e);
    }
    let bottom_ring = rim_rings.get(&bottom_e).ok_or_else(|| {
        YangError::MalformedTopology(format!(
            "face {f_idx}: bottom rim ring {bottom_e} not built"
        ))
    })?;
    let top_ring = rim_rings.get(&top_e).ok_or_else(|| {
        YangError::MalformedTopology(format!("face {f_idx}: top rim ring {top_e} not built"))
    })?;
    let nseg = top_ring.len();
    if nseg < 3 || bottom_ring.len() != nseg {
        return Err(YangError::MalformedTopology(format!(
            "face {f_idx}: cone frustum rims have mismatched / too-few samples ({} vs {})",
            bottom_ring.len(),
            top_ring.len()
        )));
    }
    let co_rotating = rim_sense(bottom_e) * rim_sense(top_e) > 0.0;
    let b_index = |k: usize| -> usize {
        if co_rotating {
            k
        } else {
            (nseg - k) % nseg
        }
    };
    for k in 0..nseg {
        let kn = (k + 1) % nseg;
        let t0 = top_ring[k];
        let t1 = top_ring[kn];
        let b0 = bottom_ring[b_index(k)];
        let b1 = bottom_ring[b_index(kn)];
        for mut tri in [[b0, b1, t1], [b0, t1, t0]] {
            let mut n = cone_outward_normal(out_verts, &tri, apex, axis_dir, half_angle);
            if f.reversed {
                n = [-n[0], -n[1], -n[2]];
            }
            orient_tri(out_verts, &mut tri, n);
            out_tris.push(tri);
        }
    }
    Ok(())
}

/// KV6d 4b: tessellate a partial-torus `Surface::Torus` face — a bent tube —
/// as a watertight (θ × φ) bijective grid. Rows are meridians (constant sweep
/// angle θ), columns are longitudes (constant profile angle φ). The two θ-end
/// meridians REUSE the profile-circle rim rings (so they match the disk caps
/// bit-for-bit → watertight); the φ=0 column REUSES the seam-arc chain; the
/// interior points are fresh `BRepFace { u=φ, v=θ }` (so `eval_source`
/// round-trips via the torus `face_eval` arm). Rings are aligned by intrinsic
/// φ (`atan2(τ, ρ−R)`), so a counter-rotating end meridian still lines up
/// column-for-column.
#[allow(clippy::too_many_arguments)]
pub(crate) fn tessellate_torus_face(
    f_idx: usize,
    f: &BRepFace,
    edges: &[BRepEdge],
    rim_rings: &std::collections::BTreeMap<u32, Vec<u32>>,
    center: Point3,
    axis_dir: Vector3,
    major: f64,
    minor: f64,
    out_verts: &mut Vec<Point3>,
    sources: &mut Vec<TessellationSource>,
    out_tris: &mut Vec<[u32; 3]>,
) -> Result<(), YangError> {
    use std::collections::BTreeSet;
    use std::f64::consts::PI;
    let malformed = |m: String| YangError::MalformedTopology(m);
    // KV14 Slice F: a boolean-result torus lateral carrying inner loops is a
    // POLOIDAL PERIODIC BAND — the boundary wraps fully around the tube (poloidal
    // φ) while the toroidal θ is bounded (probe KV14_TORUS_PROBE:
    // R0028/R0059/R0026/R0051). A torus is NOT ruled in the toroidal direction,
    // so it needs interior toroidal rings sampled onto the surface — a STRUCTURED
    // (θ × φ) grid, not a flat unroll+CDT (which would chord the sweep). The
    // hole-free structured (2 profiles + seam) arm below is left untouched.
    if !f.inner_loops.is_empty() {
        return tessellate_torus_band(
            f_idx, f, edges, rim_rings, center, axis_dir, major, minor, out_verts, sources,
            out_tris,
        );
    }
    let cen = center.as_array();
    let ax = normalize3(axis_dir.as_array());
    let (e1v, e2v) = ortho_basis(axis_dir);
    let (e1, e2) = (e1v.as_array(), e2v.as_array());
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let band = 1e-9 * (1.0 + major + minor);

    // Classify the boundary edges: 2 profile circles (closed, radius ≈ minor)
    // + 1 seam arc (open, radius ≈ major+minor) — the partial tube — OR
    // 1 profile circle + 1 closed outer-equator circle (the CLOSED torus,
    // KV6d full turn, spec `kv6d_closed_torus_revolve.md`).
    let mut profiles: Vec<u32> = Vec::new();
    let mut seam: Option<u32> = None;
    let mut equator: Option<u32> = None;
    let mut seen = BTreeSet::new();
    for &e in f.outer_loop.iter() {
        if !seen.insert(e) {
            continue;
        }
        if let Curve::Circle { radius, .. } = edges[e as usize].curve {
            let ed = &edges[e as usize];
            if ed.start == ed.end && (radius - minor).abs() <= band {
                profiles.push(e);
            } else if ed.start != ed.end && (radius - (major + minor)).abs() <= band {
                seam = Some(e);
            } else if ed.start == ed.end && (radius - (major + minor)).abs() <= band {
                equator = Some(e);
            }
        }
    }
    if let (None, 1, Some(eq_e)) = (seam, profiles.len(), equator) {
        return tessellate_torus_closed(
            f_idx,
            f,
            edges,
            rim_rings,
            center,
            axis_dir,
            major,
            minor,
            profiles[0],
            eq_e,
            out_verts,
            sources,
            out_tris,
        );
    }
    let (Some(seam_e), 2) = (seam, profiles.len()) else {
        return Err(malformed(format!(
            "face {f_idx}: torus face needs 2 profile circles + 1 seam arc (got {} circles)",
            profiles.len()
        )));
    };
    let seam_chain = rim_rings
        .get(&seam_e)
        .ok_or_else(|| malformed(format!("face {f_idx}: seam chain {seam_e} not built")))?;
    let n_theta = seam_chain.len() - 1;
    if n_theta < 1 {
        return Err(malformed(format!(
            "face {f_idx}: torus seam chain too short"
        )));
    }
    let (seam_start, seam_end) = (edges[seam_e as usize].start, edges[seam_e as usize].end);
    let prof0_e = *profiles
        .iter()
        .find(|&&p| edges[p as usize].start == seam_start)
        .ok_or_else(|| malformed(format!("face {f_idx}: no θ=0 profile at the seam start")))?;
    let profa_e = *profiles
        .iter()
        .find(|&&p| edges[p as usize].start == seam_end)
        .ok_or_else(|| malformed(format!("face {f_idx}: no θ=α profile at the seam end")))?;
    let ring0 = rim_rings
        .get(&prof0_e)
        .ok_or_else(|| malformed(format!("face {f_idx}: θ=0 profile ring not built")))?;
    let ringa = rim_rings
        .get(&profa_e)
        .ok_or_else(|| malformed(format!("face {f_idx}: θ=α profile ring not built")))?;
    let n_phi = ring0.len();
    if ringa.len() != n_phi || n_phi < 3 {
        return Err(malformed(format!(
            "face {f_idx}: torus profile rings mismatched / too few ({n_phi} vs {})",
            ringa.len()
        )));
    }

    // Intrinsic profile angle φ of a mesh vertex (the shared poloidal
    // convention — also `collect_ring_crossings`' torus projection).
    let phi_of = |out_verts: &[Point3], vi: u32| -> f64 {
        let p = out_verts[vi as usize].as_array();
        let d = [p[0] - cen[0], p[1] - cen[1], p[2] - cen[2]];
        let tau = dot(d, ax);
        let radial = [d[0] - tau * ax[0], d[1] - tau * ax[1], d[2] - tau * ax[2]];
        let rho = dot(radial, radial).sqrt();
        tau.atan2(rho - major)
    };
    // Historical UNIFORM path first, byte-identical (spec
    // `m8_torus_profile_rim_crossing` B5): both rings slot-align on uniform
    // 2π/n_phi rounding ⇒ the pre-#131 grid, bit-for-bit. Only when a ring
    // carries NON-uniform samples (a Stage-0 rim-crossing override) does
    // the φ-value column path below take over (B6).
    let phi_slot = |out_verts: &[Point3], vi: u32| -> usize {
        let phi = phi_of(out_verts, vi).rem_euclid(2.0 * PI);
        ((phi / (2.0 * PI / n_phi as f64)).round() as usize) % n_phi
    };
    let uniform_rows: Option<(Vec<u32>, Vec<u32>)> = {
        let assign = |ring: &[u32]| -> Option<Vec<u32>> {
            let mut row = vec![u32::MAX; n_phi];
            for &v in ring {
                let s = phi_slot(out_verts, v);
                if row[s] != u32::MAX {
                    return None; // slot collision — non-uniform sampling
                }
                row[s] = v;
            }
            (!row.contains(&u32::MAX)).then_some(row)
        };
        match (assign(ring0), assign(ringa)) {
            (Some(r0), Some(ra)) => Some((r0, ra)),
            _ => None,
        }
    };
    let (row0, rowa, interior_phi): (Vec<u32>, Vec<u32>, Vec<f64>) =
        if let Some((row0, rowa)) = uniform_rows {
            // Historical interior column angles (bit-identical to pre-#131).
            let phis = (0..n_phi)
                .map(|j| 2.0 * PI * (j as f64) / (n_phi as f64))
                .collect();
            (row0, rowa, phis)
        } else {
            // Task #131 (spec `m8_torus_profile_rim_crossing` §1.3): the grid
            // columns are ring0's ACTUAL intrinsic φ values, anchored at the
            // seam (ring0[0] — the seam-arc endpoint on the outer equator,
            // offset 0 by construction) — a Stage-0 rim-crossing override
            // inserts non-uniform profile samples the uniform slots cannot
            // represent.
            let base = phi_of(out_verts, ring0[0]);
            let offset_of =
                |out_verts: &[Point3], vi: u32| (phi_of(out_verts, vi) - base).rem_euclid(2.0 * PI);
            let mut cols: Vec<(f64, u32)> = ring0
                .iter()
                .map(|&v| (offset_of(out_verts, v), v))
                .collect();
            cols.sort_by(|a, b| a.0.total_cmp(&b.0));
            if cols[0].1 != ring0[0] || cols[0].0 != 0.0 {
                return Err(malformed(format!(
                    "face {f_idx}: torus profile ring seam anchor is not at φ-offset 0"
                )));
            }
            let col_angles: Vec<f64> = cols.iter().map(|&(o, _)| o).collect();
            let row0: Vec<u32> = cols.iter().map(|&(_, v)| v).collect();
            // Match the θ=α ring to the columns INDEX-WISE on the sorted
            // offsets: both rings are seam-anchored (`ring[0]` is the seam-arc
            // endpoint — force ITS offset to exactly 0, since its own atan2
            // can compute 2π−ε across the wrap) and carry one sample per
            // column (the paired Stage-0 insertion), so the sorted sequences
            // correspond 1:1. A fixed ULP band guards each pair — NOT a
            // min-gap-derived tolerance, which a femto-close crossing twin
            // pair would collapse below legitimate cross-ring rounding
            // (R0050: Δφ ≈ 9e-16 vs a 4e-16 twin gap).
            let band = 1e-9 * (1.0 + 2.0 * PI);
            let mut a_off: Vec<(f64, u32)> = ringa
                .iter()
                .enumerate()
                .map(|(k, &v)| {
                    let o = if k == 0 { 0.0 } else { offset_of(out_verts, v) };
                    (o, v)
                })
                .collect();
            a_off.sort_by(|x, y| x.0.total_cmp(&y.0));
            let mut rowa: Vec<u32> = Vec::with_capacity(n_phi);
            for (j, &(o, v)) in a_off.iter().enumerate() {
                let d0 = (o - col_angles[j]).abs();
                let d = d0.min(2.0 * PI - d0);
                if d > band {
                    return Err(malformed(format!(
                        "face {f_idx}: torus profile rings are not φ-aligned \
                     (vertex offset {o} vs column {} beyond band {band})",
                        col_angles[j]
                    )));
                }
                rowa.push(v);
            }
            let phis = col_angles.iter().map(|&o| base + o).collect();
            (row0, rowa, phis)
        };

    // Build the full (n_theta+1) × n_phi grid of vertex indices.
    let mut grid: Vec<Vec<u32>> = Vec::with_capacity(n_theta + 1);
    #[allow(clippy::needless_range_loop)]
    for i in 0..=n_theta {
        if i == 0 {
            grid.push(row0.clone());
        } else if i == n_theta {
            grid.push(rowa.clone());
        } else {
            let s = seam_chain[i];
            let p = out_verts[s as usize].as_array();
            let d = [p[0] - cen[0], p[1] - cen[1], p[2] - cen[2]];
            let theta = dot(d, e2).atan2(dot(d, e1));
            let (st, ct) = theta.sin_cos();
            let mut row = vec![0u32; n_phi];
            row[0] = s; // φ=0 column reuses the seam chain
            for (j, slot) in row.iter_mut().enumerate().skip(1) {
                let phi = interior_phi[j];
                let rad = major + minor * phi.cos();
                let sp = minor * phi.sin();
                let pt = [
                    cen[0] + rad * (ct * e1[0] + st * e2[0]) + sp * ax[0],
                    cen[1] + rad * (ct * e1[1] + st * e2[1]) + sp * ax[1],
                    cen[2] + rad * (ct * e1[2] + st * e2[2]) + sp * ax[2],
                ];
                let vi = out_verts.len() as u32;
                out_verts.push(Point3::new(pt[0], pt[1], pt[2]));
                sources.push(TessellationSource::BRepFace {
                    face: f_idx as u32,
                    u: phi,
                    v: theta,
                });
                *slot = vi;
            }
            grid.push(row);
        }
    }

    // Emit quads, each triangle wound to agree with the torus outward normal
    // (direction from the nearest tube-center-circle point).
    let emit = |a: u32, b: u32, c: u32, out_verts: &[Point3], out_tris: &mut Vec<[u32; 3]>| {
        let pa = out_verts[a as usize].as_array();
        let pb = out_verts[b as usize].as_array();
        let pc = out_verts[c as usize].as_array();
        let e_a = [pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]];
        let e_b = [pc[0] - pa[0], pc[1] - pa[1], pc[2] - pa[2]];
        let gn = [
            e_a[1] * e_b[2] - e_a[2] * e_b[1],
            e_a[2] * e_b[0] - e_a[0] * e_b[2],
            e_a[0] * e_b[1] - e_a[1] * e_b[0],
        ];
        let ctr = [
            (pa[0] + pb[0] + pc[0]) / 3.0,
            (pa[1] + pb[1] + pc[1]) / 3.0,
            (pa[2] + pb[2] + pc[2]) / 3.0,
        ];
        let d = [ctr[0] - cen[0], ctr[1] - cen[1], ctr[2] - cen[2]];
        let tau = dot(d, ax);
        let rv = [d[0] - tau * ax[0], d[1] - tau * ax[1], d[2] - tau * ax[2]];
        let rl = dot(rv, rv).sqrt().max(1e-300);
        let rhat = [rv[0] / rl, rv[1] / rl, rv[2] / rl];
        let on = [
            ctr[0] - (cen[0] + major * rhat[0]),
            ctr[1] - (cen[1] + major * rhat[1]),
            ctr[2] - (cen[2] + major * rhat[2]),
        ];
        if dot(gn, on) >= 0.0 {
            out_tris.push([a, b, c]);
        } else {
            out_tris.push([a, c, b]);
        }
    };
    for i in 0..n_theta {
        for j in 0..n_phi {
            let jn = (j + 1) % n_phi;
            let (a, b, c, d) = (grid[i][j], grid[i][jn], grid[i + 1][jn], grid[i + 1][j]);
            emit(a, b, c, out_verts, out_tris);
            emit(a, c, d, out_verts, out_tris);
        }
    }
    Ok(())
}

/// KV6d closed torus (spec `kv6d_closed_torus_revolve.md`): tessellate the
/// CLOSED `Surface::Torus` face — outer loop = 1 closed poloidal PROFILE
/// circle (radius ≈ minor) + 1 closed toroidal OUTER-EQUATOR circle (radius
/// ≈ major+minor), both anchored at the shared seam vertex — as a doubly
/// periodic (θ × φ) bijective grid (both index directions wrap; no ring is
/// duplicated, so the mesh is watertight by construction).
///
/// Rows: the profile ring is the θ = θ₀ meridian; the equator ring supplies
/// one row per sample (its Steiner points are exactly the φ = 0 column).
/// Columns: the profile ring's ACTUAL intrinsic φ values, seam-anchored
/// (the #131 φ-value convention — correct for both uniform and overridden
/// rings; this arm is new, so there is no historical uniform grid to
/// reproduce byte-for-byte). Interior points are fresh
/// `BRepFace { u = φ, v = θ }` sources (the torus `face_eval` arm).
#[allow(clippy::too_many_arguments)]
pub(crate) fn tessellate_torus_closed(
    f_idx: usize,
    _f: &BRepFace,
    _edges: &[BRepEdge],
    rim_rings: &std::collections::BTreeMap<u32, Vec<u32>>,
    center: Point3,
    axis_dir: Vector3,
    major: f64,
    minor: f64,
    prof_e: u32,
    eq_e: u32,
    out_verts: &mut Vec<Point3>,
    sources: &mut Vec<TessellationSource>,
    out_tris: &mut Vec<[u32; 3]>,
) -> Result<(), YangError> {
    use std::f64::consts::PI;
    let malformed = |m: String| YangError::MalformedTopology(m);
    let cen = center.as_array();
    let ax = normalize3(axis_dir.as_array());
    let (e1v, e2v) = ortho_basis(axis_dir);
    let (e1, e2) = (e1v.as_array(), e2v.as_array());
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];

    let ring0 = rim_rings
        .get(&prof_e)
        .ok_or_else(|| malformed(format!("face {f_idx}: closed-torus profile ring not built")))?;
    let eq_ring = rim_rings
        .get(&eq_e)
        .ok_or_else(|| malformed(format!("face {f_idx}: closed-torus equator ring not built")))?;
    let n_phi = ring0.len();
    let n_theta = eq_ring.len();
    if n_phi < 3 || n_theta < 3 {
        return Err(malformed(format!(
            "face {f_idx}: closed-torus rings too short ({n_phi} × {n_theta})"
        )));
    }
    if ring0[0] != eq_ring[0] {
        return Err(malformed(format!(
            "face {f_idx}: closed-torus rings do not share the seam anchor"
        )));
    }

    // Intrinsic poloidal φ (the shared convention with `tessellate_torus_face`).
    let phi_of = |out_verts: &[Point3], vi: u32| -> f64 {
        let p = out_verts[vi as usize].as_array();
        let d = [p[0] - cen[0], p[1] - cen[1], p[2] - cen[2]];
        let tau = dot(d, ax);
        let radial = [d[0] - tau * ax[0], d[1] - tau * ax[1], d[2] - tau * ax[2]];
        let rho = dot(radial, radial).sqrt();
        tau.atan2(rho - major)
    };
    // Column table: seam-anchored sorted intrinsic offsets (#131 φ-value path).
    let base = phi_of(out_verts, ring0[0]);
    let mut cols: Vec<(f64, u32)> = ring0
        .iter()
        .enumerate()
        .map(|(k, &v)| {
            let o = if k == 0 {
                0.0
            } else {
                (phi_of(out_verts, v) - base).rem_euclid(2.0 * PI)
            };
            (o, v)
        })
        .collect();
    cols.sort_by(|a, b| a.0.total_cmp(&b.0));
    if cols[0].1 != ring0[0] {
        return Err(malformed(format!(
            "face {f_idx}: closed-torus profile ring seam anchor is not at φ-offset 0"
        )));
    }
    let col_angles: Vec<f64> = cols.iter().map(|&(o, _)| o).collect();
    let row0: Vec<u32> = cols.iter().map(|&(_, v)| v).collect();

    // Rows: profile ring at the anchor azimuth, then one row per equator
    // sample (in ring order — cyclically monotone by construction).
    let mut grid: Vec<Vec<u32>> = Vec::with_capacity(n_theta);
    grid.push(row0);
    for &s in &eq_ring[1..] {
        let p = out_verts[s as usize].as_array();
        let d = [p[0] - cen[0], p[1] - cen[1], p[2] - cen[2]];
        let theta = dot(d, e2).atan2(dot(d, e1));
        let (st, ct) = theta.sin_cos();
        let mut row = vec![0u32; n_phi];
        row[0] = s; // φ = 0 column reuses the equator ring
        for (j, slot) in row.iter_mut().enumerate().skip(1) {
            let phi = base + col_angles[j];
            let rad = major + minor * phi.cos();
            let sp = minor * phi.sin();
            let pt = [
                cen[0] + rad * (ct * e1[0] + st * e2[0]) + sp * ax[0],
                cen[1] + rad * (ct * e1[1] + st * e2[1]) + sp * ax[1],
                cen[2] + rad * (ct * e1[2] + st * e2[2]) + sp * ax[2],
            ];
            let vi = out_verts.len() as u32;
            out_verts.push(Point3::new(pt[0], pt[1], pt[2]));
            sources.push(TessellationSource::BRepFace {
                face: f_idx as u32,
                u: phi,
                v: theta,
            });
            *slot = vi;
        }
        grid.push(row);
    }

    // Emit quads with BOTH directions wrapped, each triangle wound to agree
    // with the torus outward normal (same rule as `tessellate_torus_face`).
    let emit = |a: u32, b: u32, c: u32, out_verts: &[Point3], out_tris: &mut Vec<[u32; 3]>| {
        let pa = out_verts[a as usize].as_array();
        let pb = out_verts[b as usize].as_array();
        let pc = out_verts[c as usize].as_array();
        let e_a = [pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]];
        let e_b = [pc[0] - pa[0], pc[1] - pa[1], pc[2] - pa[2]];
        let gn = [
            e_a[1] * e_b[2] - e_a[2] * e_b[1],
            e_a[2] * e_b[0] - e_a[0] * e_b[2],
            e_a[0] * e_b[1] - e_a[1] * e_b[0],
        ];
        let ctr = [
            (pa[0] + pb[0] + pc[0]) / 3.0,
            (pa[1] + pb[1] + pc[1]) / 3.0,
            (pa[2] + pb[2] + pc[2]) / 3.0,
        ];
        let d = [ctr[0] - cen[0], ctr[1] - cen[1], ctr[2] - cen[2]];
        let tau = dot(d, ax);
        let rv = [d[0] - tau * ax[0], d[1] - tau * ax[1], d[2] - tau * ax[2]];
        let rl = dot(rv, rv).sqrt().max(1e-300);
        let rhat = [rv[0] / rl, rv[1] / rl, rv[2] / rl];
        let on = [
            ctr[0] - (cen[0] + major * rhat[0]),
            ctr[1] - (cen[1] + major * rhat[1]),
            ctr[2] - (cen[2] + major * rhat[2]),
        ];
        if dot(gn, on) >= 0.0 {
            out_tris.push([a, b, c]);
        } else {
            out_tris.push([a, c, b]);
        }
    };
    for i in 0..n_theta {
        let inx = (i + 1) % n_theta;
        for j in 0..n_phi {
            let jn = (j + 1) % n_phi;
            let (a, b, c, d) = (grid[i][j], grid[i][jn], grid[inx][jn], grid[inx][j]);
            emit(a, b, c, out_verts, out_tris);
            emit(a, c, d, out_verts, out_tris);
        }
    }
    Ok(())
}

/// Unwrap a periodic angle sequence in place: each value is shifted by a
/// multiple of 2π so successive entries never jump by more than π (standard
/// phase unwrapping). Turns a torus patch's `atan2` parameters into a simple
/// (non-self-crossing) polygon as long as the patch does not wrap the whole way
/// around a seam.
pub(crate) fn unwrap_seq(a: &mut [f64]) {
    use std::f64::consts::{PI, TAU};
    for k in 1..a.len() {
        while a[k] - a[k - 1] > PI {
            a[k] -= TAU;
        }
        while a[k - 1] - a[k] > PI {
            a[k] += TAU;
        }
    }
}

/// KV6d UV-CDT consumer: tessellate an arbitrary torus PATCH from its ordered
/// closed 3D boundary loop `boundary` (the mesh-boolean intersection curve +
/// seam, finely sampled) via the interior-Steiner CDT
/// ([`cherchi_rs::cdt_polygon_with_holes_refined`]).
///
/// Pipeline: invert each boundary point into the `(u = meridian, v = longitude)`
/// parameter plane — `face_eval(u,v) = center + (R + r·cos u)·(cos v·ê1 +
/// sin v·ê2) + r·sin u·â`, so `v = atan2(w·ê2, w·ê1)` and `u = atan2(τ, ρ − R)`
/// (τ axial, ρ radial-from-axis) — angle-UNWRAP `u` and `v` to a simple polygon,
/// SCALE to ~arc-length isotropy (`u·r`, `v·R`) so a Delaunay-quality triangle
/// maps to a well-shaped 3D one, REFINE to `max_3d_area`, then MAP BACK: BOUNDARY
/// verts keep their EXACT input 3D coordinates (conformal with the neighbouring
/// patch — the boolean's shared intersection-curve samples), interior STEINER
/// verts are `face_eval` of their unscaled `(u,v)` (exactly on the torus).
///
/// Returns `(verts, tris)` with `verts[0..boundary.len()] == boundary` bit-for-
/// bit. `None` if the boundary degenerates or the CDT rejects a self-intersecting
/// projection (a seam-CROSSING / self-overlapping patch — out of this v1 scope;
/// the deferral is loud at the caller).
///
/// Consumed by kernel-v2's render-time torus-patch tessellation (the owner of
/// output recovery re-tessellation); exposed `pub` for that cross-crate call.
/// One boundary loop of a torus patch projected into the continuous, scaled
/// meridian/longitude plane (`su = u·minor`, `sv = v·major`), with its net
/// MERIDIAN winding `wu` (in 2π units). `wu != 0` ⇒ the loop wraps the meridian
/// seam (a band edge); the longitude is required non-wrapping (partial torus).
pub(crate) struct PLoop {
    su: Vec<f64>,
    sv: Vec<f64>,
    pts: Vec<Point3>,
    wu: i64,
}

/// KV6d seam-wrapping render — the cylinder patch's pass-2 case-2 ported to the
/// torus meridian. Unrolls a BAND bounded by two oppositely-meridian-wrapping
/// loops `pc` (wrap +1) and `mc` (wrap −1) into a single simple ring in the
/// universal cover: walk `pc` over one full period (`su` increasing by `span`),
/// bridge to `mc`, walk `mc` reversed (`su` decreasing), bridge back. The seam
/// bridges connect an anchor pair chosen so neither bridge crosses a boundary
/// edge; the duplicated anchor vertices coincide in 3D (the meridian is
/// periodic) so the result is watertight. Returns the ring as `(su, sv, 3d)` per
/// vertex, or `None` if no unblocked bridge exists.
pub(crate) fn band_seam_bridge<F: Fn(f64, f64) -> Point3>(
    pc: &PLoop,
    mc: &PLoop,
    span: f64,
    minor: f64,
    major: f64,
    windows: &[(f64, f64)],
    eval: F,
) -> Option<Vec<(f64, f64, Point3)>> {
    let (m, mm) = (pc.su.len(), mc.su.len());
    if m < 3 || mm < 3 {
        return None;
    }
    let principal = |d: f64| d - (d / span).round() * span;
    // Proper (open) segment crossing in the (su, sv) plane.
    let seg_cross = |a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]| -> bool {
        let o = |p: [f64; 2], q: [f64; 2], r: [f64; 2]| {
            (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0])
        };
        o(a, c, d) * o(b, c, d) < 0.0 && o(a, b, c) * o(a, b, d) < 0.0
    };
    // Each seam bridge spans the full longitude gap (v0 → v1) as a SINGLE
    // segment; left unsplit, the CDT cannot place Steiner near it and the edge
    // region stays coarse → large chord error on the curved tube. Subdivide it
    // into `k` segments matching the meridian sampling density, with each
    // intermediate point ON the torus at the seam meridian (so the left- and
    // right-seam copies, one period apart in u, coincide in 3D → watertight).
    let mean_step = span / m as f64;
    // Interpolate a bridge from `(su_a, sv_a)` to `(su_b, sv_b)` at the seam
    // meridian `u_seam`, returning the K−1 interior points (2D + on-torus 3D).
    let bridge_pts =
        |su_a: f64, sv_a: f64, su_b: f64, sv_b: f64, u_seam: f64| -> Vec<(f64, f64, Point3)> {
            let dsv = (sv_b - sv_a).abs();
            let k = (dsv / mean_step).ceil().max(1.0) as usize;
            let mut out = Vec::with_capacity(k.saturating_sub(1));
            for i in 1..k {
                let t = i as f64 / k as f64;
                let su = su_a + (su_b - su_a) * t;
                let sv = sv_a + (sv_b - sv_a) * t;
                out.push((su, sv, eval(u_seam, sv / major)));
            }
            out
        };
    let build_ring = |xi: usize, yi: usize, dpr: f64| -> Vec<(f64, f64, Point3)> {
        let mut ring: Vec<(f64, f64, Point3)> = Vec::with_capacity(m + mm + 2);
        let base_x = pc.su[xi];
        for j in 0..=m {
            let idx = (xi + j) % m;
            let mut u = pc.su[idx];
            if j > 0 && (xi + j) >= m {
                u += span;
            }
            if j == m {
                u = base_x + span;
            }
            ring.push((u, pc.sv[idx], pc.pts[idx]));
        }
        let y_target = base_x + span + dpr;
        let y_base = mc.su[yi];
        let wsign = mc.wu as f64; // −1
                                  // Bridge 1: pc closing point (base_x+span, v0) → mc start (y_target, v1),
                                  // at the right-seam meridian (base_x+span)/minor.
        ring.extend(bridge_pts(
            base_x + span,
            pc.sv[xi],
            y_target,
            mc.sv[yi],
            (base_x + span) / minor,
        ));
        for j in 0..=mm {
            let idx = (yi + j) % mm;
            let mut u = mc.su[idx];
            if j > 0 && (yi + j) >= mm {
                u += wsign * span;
            }
            if j == mm {
                u = y_base + wsign * span;
            }
            ring.push((u - y_base + y_target, mc.sv[idx], mc.pts[idx]));
        }
        // Bridge 2: mc closing point (≈ base_x+dpr, v1) → pc start (base_x, v0),
        // at the left-seam meridian base_x/minor (= right meridian − 2π in u, so
        // `eval` returns the SAME 3D points as bridge 1 → watertight seam).
        ring.extend(bridge_pts(
            base_x + dpr,
            mc.sv[yi],
            base_x,
            pc.sv[xi],
            base_x / minor,
        ));
        ring
    };
    // A non-wrapping window straddles the seam cut (at u ≡ base_x mod span) iff a
    // seam line falls strictly inside its u-interval [wlo, whi] — i.e. Slice F-2's
    // "window on the seam" split (the R0063/cylinder wall on the torus). Prefer an
    // anchor whose seam splits no window; if none exists, fall back to the first
    // non-crossing anchor (a genuinely seam-crossing patch, out of scope).
    let straddles_window = |base_x: f64| -> bool {
        windows.iter().any(|&(wlo, whi)| {
            let d = (base_x - wlo).rem_euclid(span);
            d > 1e-12 && d < (whi - wlo) - 1e-12
        })
    };
    let mut fallback: Option<(usize, usize, f64)> = None;
    // Pick the anchor pair (xi on pc, yi on mc) whose two seam-bridge SPANS
    // (validated as single segments, before subdivision) cross no boundary edge.
    for xi in 0..m {
        let xu = pc.su[xi];
        let mut best: Option<(usize, f64)> = None;
        for (yi, &syu) in mc.su.iter().enumerate() {
            let dpr = principal(syu - xu);
            if best.is_none_or(|(_, b)| dpr.abs() < b.abs()) {
                best = Some((yi, dpr));
            }
        }
        let (yi, dpr) = best?;
        let base_x = pc.su[xi];
        // The two seam spans (right: pc→mc; left: mc→pc), in the unrolled frame.
        let spans = [
            ([base_x + span, pc.sv[xi]], [base_x + span + dpr, mc.sv[yi]]),
            ([base_x + dpr, mc.sv[yi]], [base_x, pc.sv[xi]]),
        ];
        // Boundary edges to test against: the pc bottom chain (at sv=pc.sv) and
        // the mc top chain (at sv=mc.sv), in the bridge's local frame.
        let bottom: Vec<[f64; 2]> = (0..=m)
            .map(|j| {
                let idx = (xi + j) % m;
                let mut u = pc.su[idx];
                if (j > 0 && (xi + j) >= m) || j == m {
                    u = if j == m { base_x + span } else { u + span };
                }
                [u, pc.sv[idx]]
            })
            .collect();
        let y_target = base_x + span + dpr;
        let y_base = mc.su[yi];
        let wsign = mc.wu as f64;
        let top: Vec<[f64; 2]> = (0..=mm)
            .map(|j| {
                let idx = (yi + j) % mm;
                let mut u = mc.su[idx];
                if (j > 0 && (yi + j) >= mm) || j == mm {
                    u = if j == mm {
                        y_base + wsign * span
                    } else {
                        u + wsign * span
                    };
                }
                [u - y_base + y_target, mc.sv[idx]]
            })
            .collect();
        let crosses = |p: [f64; 2], q: [f64; 2]| -> bool {
            if p == q {
                return true;
            }
            let chain_hit =
                |chain: &[[f64; 2]]| chain.windows(2).any(|w| seg_cross(p, q, w[0], w[1]));
            chain_hit(&bottom) || chain_hit(&top)
        };
        if spans.iter().any(|&(p, q)| crosses(p, q)) {
            continue;
        }
        // Non-crossing anchor: take it immediately if it also splits no window;
        // otherwise remember it as the fallback and keep looking for a clean seam.
        if !straddles_window(base_x) {
            return Some(build_ring(xi, yi, dpr));
        }
        if fallback.is_none() {
            fallback = Some((xi, yi, dpr));
        }
    }
    fallback.map(|(xi, yi, dpr)| build_ring(xi, yi, dpr))
}

#[allow(clippy::too_many_arguments)]
pub fn tessellate_torus_patch(
    center: Point3,
    axis_dir: Vector3,
    major: f64,
    minor: f64,
    boundary: &[Point3],
    holes: &[Vec<Point3>],
    max_3d_area: f64,
) -> Option<(Vec<Point3>, Vec<[u32; 3]>)> {
    if boundary.len() < 3 || major <= 0.0 || minor <= 0.0 {
        return None;
    }
    let ax = normalize3(axis_dir.as_array());
    let (e1, e2) = ortho_basis(axis_dir);
    let (e1a, e2a) = (e1.as_array(), e2.as_array());
    let c = center.as_array();
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let eval = |u: f64, v: f64| -> Point3 {
        let (cu, su) = (u.cos(), u.sin());
        let (cv, sv) = (v.cos(), v.sin());
        let rad = major + minor * cu;
        Point3::new(
            c[0] + rad * (cv * e1a[0] + sv * e2a[0]) + minor * su * ax[0],
            c[1] + rad * (cv * e1a[1] + sv * e2a[1]) + minor * su * ax[1],
            c[2] + rad * (cv * e1a[2] + sv * e2a[2]) + minor * su * ax[2],
        )
    };
    let snap = |a: f64| (a / 1e-12).round() * 1e-12;
    // Invert ONE loop into (u, v): project, unwrap each angle to a simple
    // polygon, and condition to the TAU_WORK (1e-12 rad) grid. The atan2/cos/sin
    // inversion introduces ~1 ULP noise; on a straight seam run (constant u or
    // v) that faint kink makes spade over-refine into a sliver storm — snapping
    // removes it without perturbing output geometry (boundary verts are emitted
    // from the EXACT input 3D below; Steiner verts are `face_eval`, on-torus).
    let invert = |loop_pts: &[Point3]| -> (Vec<f64>, Vec<f64>) {
        let mut us = Vec::with_capacity(loop_pts.len());
        let mut vs = Vec::with_capacity(loop_pts.len());
        for &p in loop_pts {
            let pa = p.as_array();
            let w = [pa[0] - c[0], pa[1] - c[1], pa[2] - c[2]];
            let tau = dot(w, ax);
            let radial = [w[0] - tau * ax[0], w[1] - tau * ax[1], w[2] - tau * ax[2]];
            let wx = dot(radial, e1a);
            let wy = dot(radial, e2a);
            let rho = (wx * wx + wy * wy).sqrt();
            vs.push(wy.atan2(wx));
            us.push(tau.atan2(rho - major));
        }
        unwrap_seq(&mut us);
        unwrap_seq(&mut vs);
        for a in us.iter_mut().chain(vs.iter_mut()) {
            *a = snap(*a);
        }
        (us, vs)
    };

    // Net winding of a closed loop's already-unwrapped angle sequence: the span
    // plus the closure step, in units of 2π. Nonzero ⇒ the loop wraps that
    // periodic coordinate (a meridian- or longitude-wrapping patch).
    let net_wrap = |a: &[f64]| -> i64 {
        if a.len() < 2 {
            return 0;
        }
        let raw_closure = a[0] - a[a.len() - 1];
        // wrap_to_pi(raw_closure)
        let mut c = raw_closure % std::f64::consts::TAU;
        if c > std::f64::consts::PI {
            c -= std::f64::consts::TAU;
        } else if c <= -std::f64::consts::PI {
            c += std::f64::consts::TAU;
        }
        let net = (a[a.len() - 1] - a[0]) + c;
        (net / std::f64::consts::TAU).round() as i64
    };

    use std::f64::consts::TAU;
    // ---- Pass 1: project every loop into the continuous, scaled (su, sv)
    // meridian/longitude plane with its net meridian winding `wu`. ----------
    let project = |pts: &[Point3]| -> Option<PLoop> {
        if pts.len() < 3 {
            return None;
        }
        let (us, vs) = invert(pts);
        if net_wrap(&vs) != 0 {
            return None; // a LONGITUDE wrap is a full-torus seam — out of scope
        }
        let wu = net_wrap(&us);
        Some(PLoop {
            su: us.iter().map(|u| u * minor).collect(),
            sv: vs.iter().map(|v| v * major).collect(),
            pts: pts.to_vec(),
            wu,
        })
    };
    let mut ploops: Vec<PLoop> = Vec::with_capacity(1 + holes.len());
    ploops.push(project(boundary)?);
    for h in holes {
        ploops.push(project(h)?);
    }
    let span = TAU * minor; // one full meridian wrap in scaled u
    let span_v = TAU * major;

    // ---- Pass 2: assemble one simple (su, sv) outer polygon + holes. -------
    let wrapping: Vec<usize> = (0..ploops.len()).filter(|&i| ploops[i].wu != 0).collect();
    let mut verts2d: Vec<cad_primitives::Point2> = Vec::new();
    let mut vert3d: Vec<Point3> = Vec::new();
    let mut hole_idx: Vec<Vec<u32>> = Vec::new();
    let umean = |l: &PLoop| l.su.iter().sum::<f64>() / l.su.len() as f64;
    let vmean = |l: &PLoop| l.sv.iter().sum::<f64>() / l.sv.len() as f64;
    let outer: Vec<u32> = match wrapping.len() {
        0 => {
            // DISK: boundary is the outer loop; holes are the rest, each shifted
            // by whole periods so it sits in the outer's period (a hole atan2
            // placed on the opposite branch would project outside the outer).
            let (u_ref, v_ref) = (umean(&ploops[0]), vmean(&ploops[0]));
            let mut o = Vec::with_capacity(ploops[0].su.len());
            for k in 0..ploops[0].su.len() {
                o.push(verts2d.len() as u32);
                verts2d.push(cad_primitives::Point2::new(
                    ploops[0].su[k],
                    ploops[0].sv[k],
                ));
                vert3d.push(ploops[0].pts[k]);
            }
            for l in &ploops[1..] {
                let du = ((umean(l) - u_ref) / span).round() * span;
                let dv = ((vmean(l) - v_ref) / span_v).round() * span_v;
                let mut hi = Vec::with_capacity(l.su.len());
                for k in 0..l.su.len() {
                    hi.push(verts2d.len() as u32);
                    verts2d.push(cad_primitives::Point2::new(l.su[k] - du, l.sv[k] - dv));
                    vert3d.push(l.pts[k]);
                }
                hole_idx.push(hi);
            }
            o
        }
        2 => {
            // BAND (KV14 Slice F/F-2): two oppositely-meridian-wrapping loops are
            // the band's meridian edges; any REMAINING non-wrapping loops are
            // window holes in the tube wall. The universal-cover seam bridge lays
            // the outer ring across one meridian period; each window is then
            // shifted by whole periods (in both scaled params) to land inside the
            // band's unrolled (su, sv) region and carved as a CDT hole.
            let (a, b) = (wrapping[0], wrapping[1]);
            let (pc, mc) = if ploops[a].wu > 0 {
                (&ploops[a], &ploops[b])
            } else {
                (&ploops[b], &ploops[a])
            };
            if pc.wu + mc.wu != 0 {
                return None;
            }
            // Window u-intervals (the non-wrapping loops) so the seam bridge can
            // place its cut where it splits no window (Slice F-2).
            let windows: Vec<(f64, f64)> = ploops
                .iter()
                .filter(|l| l.wu == 0)
                .map(|l| {
                    (
                        l.su.iter().cloned().fold(f64::INFINITY, f64::min),
                        l.su.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
                    )
                })
                .collect();
            let ring = band_seam_bridge(pc, mc, span, minor, major, &windows, eval)?;
            let mut o = Vec::with_capacity(ring.len());
            let (mut umin, mut umax) = (f64::INFINITY, f64::NEG_INFINITY);
            let (mut vmin, mut vmax) = (f64::INFINITY, f64::NEG_INFINITY);
            for (su, sv, p) in &ring {
                umin = umin.min(*su);
                umax = umax.max(*su);
                vmin = vmin.min(*sv);
                vmax = vmax.max(*sv);
                o.push(verts2d.len() as u32);
                verts2d.push(cad_primitives::Point2::new(*su, *sv));
                vert3d.push(*p);
            }
            // Window holes = the non-wrapping loops (band edges already consumed).
            let (u_center, v_center) = (0.5 * (umin + umax), 0.5 * (vmin + vmax));
            for l in ploops.iter().filter(|l| l.wu == 0) {
                let du = ((umean(l) - u_center) / span).round() * span;
                let dv = ((vmean(l) - v_center) / span_v).round() * span_v;
                let mut hi = Vec::with_capacity(l.su.len());
                for k in 0..l.su.len() {
                    hi.push(verts2d.len() as u32);
                    verts2d.push(cad_primitives::Point2::new(l.su[k] - du, l.sv[k] - dv));
                    vert3d.push(l.pts[k]);
                }
                hole_idx.push(hi);
            }
            o
        }
        // 1 wrapping loop (degenerate) or > 2 wraps: out of scope.
        _ => return None,
    };

    let (ref_verts, tris) =
        cherchi_rs::cdt_polygon_with_holes_refined(&verts2d, &outer, &hole_idx, max_3d_area)
            .ok()?;

    // Map back: boundary verts → EXACT input 3D; refined Steiner verts (appended
    // after) → `face_eval`. A band's duplicated seam vertices carry the same 3D
    // point on both sides (the meridian is periodic) ⇒ watertight.
    let n = vert3d.len();
    let mut verts3d: Vec<Point3> = Vec::with_capacity(ref_verts.len());
    for (i, sp) in ref_verts.iter().enumerate() {
        if i < n {
            verts3d.push(vert3d[i]);
        } else {
            verts3d.push(eval(sp.x() / minor, sp.y() / major));
        }
    }
    Some((verts3d, tris))
}

/// KV6d increment 2 (spec `kv6d_sphere_revolve.md`): UV-CDT tessellation of a
/// boolean-output SPHERE patch — the render consumer for a trimmed
/// `Surface::Sphere` face (the sphere analog of [`tessellate_torus_patch`]).
///
/// Projects every loop into the scaled `(u·r, v·r)` longitude/latitude plane
/// (the FIXED z-up sphere frame of `tessellate_sphere_face`: `u = atan2`
/// about `ẑ`, `v = asin((z − c_z)/r)`), then:
///
/// - **all loops non-wrapping** → disk + period-shifted holes → refined CDT
///   (boundary vertices pass through bit-for-bit — conformal with the
///   neighboring planar patches);
/// - **the OUTER loop wraps longitude once (`wu = ±1`)** → the patch
///   contains exactly one pole (`wu = +1` → north for an outward face,
///   flipped when `reversed` — the region lies LEFT of a loop traversed CCW
///   around the face's outward normal): bridge the unwrapped boundary to
///   the pole with a two-sided meridian seam whose two copies carry
///   BIT-IDENTICAL 3D sample points, plus a UV bottom edge that degenerates
///   to the single 3D pole point; after the CDT, bit-identical positions
///   are WELDED and 3D-degenerate triangles dropped, closing the seam and
///   the pole fan watertight;
/// - anything else (multi-wrap, wrapping holes, boundary within band of a
///   pole) → `None` (the caller's typed wall; later slice).
///
/// Steiner refinement is interior-only (`keep_constraint_edges`), budgeted
/// by `max_3d_area` in arc-length² (the caller matches its structured-grid
/// chord spacing).
pub fn tessellate_sphere_patch(
    center: Point3,
    radius: f64,
    reversed: bool,
    boundary: &[Point3],
    holes: &[Vec<Point3>],
    max_3d_area: f64,
) -> Option<(Vec<Point3>, Vec<[u32; 3]>)> {
    use std::f64::consts::{FRAC_PI_2, TAU};
    if boundary.len() < 3 || !(radius.is_finite() && radius > 0.0) {
        return None;
    }
    let c = center.as_array();
    let r = radius;
    let eval = |u: f64, v: f64| -> Point3 {
        let (su, cu) = u.sin_cos();
        let (sv, cv) = v.sin_cos();
        Point3::new(c[0] + r * cv * cu, c[1] + r * cv * su, c[2] + r * sv)
    };
    let snap = |a: f64| (a / 1e-12).round() * 1e-12;
    // A boundary vertex too close to a pole makes its longitude meaningless
    // (atan2 of a sub-tolerance radial) — out of scope, loud upstream.
    let polar_band = 1e-9 * r;

    // Invert ONE loop into unwrapped, snapped, SCALED (su, sv) + net u-wrap.
    let invert = |pts: &[Point3]| -> Option<(Vec<f64>, Vec<f64>, i64)> {
        if pts.len() < 3 {
            return None;
        }
        let mut us = Vec::with_capacity(pts.len());
        let mut vs = Vec::with_capacity(pts.len());
        for p in pts {
            let pa = p.as_array();
            let w = [pa[0] - c[0], pa[1] - c[1], pa[2] - c[2]];
            let rho = (w[0] * w[0] + w[1] * w[1]).sqrt();
            if rho < polar_band {
                return None; // boundary touches a pole — later slice
            }
            us.push(w[1].atan2(w[0]));
            vs.push((w[2] / r).clamp(-1.0, 1.0).asin());
        }
        unwrap_seq(&mut us);
        let raw_closure = {
            // wrap_to_pi of the closing step; nonzero net ⇒ longitude wrap.
            let mut cl = (us[0] - us[us.len() - 1]) % TAU;
            if cl > std::f64::consts::PI {
                cl -= TAU;
            } else if cl <= -std::f64::consts::PI {
                cl += TAU;
            }
            cl
        };
        let wu = (((us[us.len() - 1] - us[0]) + raw_closure) / TAU).round() as i64;
        for a in us.iter_mut().chain(vs.iter_mut()) {
            *a = snap(*a);
        }
        Some((
            us.iter().map(|u| u * r).collect(),
            vs.iter().map(|v| v * r).collect(),
            wu,
        ))
    };

    let (b_su, b_sv, b_wu) = invert(boundary)?;
    let mut h_loops: Vec<(Vec<f64>, Vec<f64>)> = Vec::with_capacity(holes.len());
    for h in holes {
        let (hu, hv, hwu) = invert(h)?;
        if hwu != 0 {
            return None; // a wrapping HOLE — later slice
        }
        h_loops.push((hu, hv));
    }
    let span = TAU * r;

    let mut verts2d: Vec<cad_primitives::Point2> = Vec::new();
    let mut vert3d: Vec<Point3> = Vec::new();
    let mut hole_idx: Vec<Vec<u32>> = Vec::new();
    let umean = |su: &[f64]| su.iter().sum::<f64>() / su.len() as f64;

    let outer: Vec<u32> = match b_wu {
        0 => {
            // DISK: boundary is the outer loop; holes shift by whole u-periods
            // into its period (latitude does not wrap — no v shift).
            //
            // The face region must be the polygon INTERIOR (the CDT
            // triangulates inside the outer ring): a walk with the region on
            // its LEFT is CCW in the right-handed (u, v) frame for an
            // outward face and CW for a cavity (`reversed`) face. A
            // non-wrapping boundary bounding the COMPLEMENT (a sphere minus
            // a small side cap — both poles inside the region) fails this
            // test — out of scope, loud at the caller (later slice).
            let area2: f64 = (0..b_su.len())
                .map(|k| {
                    let j = (k + 1) % b_su.len();
                    b_su[k] * b_sv[j] - b_su[j] * b_sv[k]
                })
                .sum();
            if area2 == 0.0 || (area2 > 0.0) == reversed {
                return None;
            }
            let u_ref = umean(&b_su);
            let mut o = Vec::with_capacity(b_su.len());
            for k in 0..b_su.len() {
                o.push(verts2d.len() as u32);
                verts2d.push(cad_primitives::Point2::new(b_su[k], b_sv[k]));
                vert3d.push(boundary[k]);
            }
            for (li, (hu, hv)) in h_loops.iter().enumerate() {
                let du = ((umean(hu) - u_ref) / span).round() * span;
                let mut hi = Vec::with_capacity(hu.len());
                for k in 0..hu.len() {
                    hi.push(verts2d.len() as u32);
                    verts2d.push(cad_primitives::Point2::new(hu[k] - du, hv[k]));
                    vert3d.push(holes[li][k]);
                }
                hole_idx.push(hi);
            }
            o
        }
        1 | -1 => {
            // POLE CAP (possibly the complement — most of the sphere): the
            // wrapping outer loop encloses exactly one pole. Region-left of a
            // CCW-around-outward-normal loop: wu = +1 → north, flipped for a
            // cavity (reversed) face.
            let north = (b_wu > 0) != reversed;
            let pole_sv = if north { FRAC_PI_2 * r } else { -FRAC_PI_2 * r };
            let pole_3d = eval(0.0, if north { FRAC_PI_2 } else { -FRAC_PI_2 });
            let wsign = b_wu as f64;
            let m = b_su.len();

            // Hole u-extents (shifted near the boundary period below) so the
            // seam can avoid splitting one; and a segment-crossing test
            // against every boundary + hole edge.
            let seg_cross = |p: [f64; 2], q: [f64; 2], a: [f64; 2], b: [f64; 2]| -> bool {
                let d = [q[0] - p[0], q[1] - p[1]];
                let e = [b[0] - a[0], b[1] - a[1]];
                let denom = d[0] * e[1] - d[1] * e[0];
                if denom == 0.0 {
                    return false; // parallel: endpoint contact is handled by candidate choice
                }
                let w = [a[0] - p[0], a[1] - p[1]];
                let t = (w[0] * e[1] - w[1] * e[0]) / denom;
                let s = (w[0] * d[1] - w[1] * d[0]) / denom;
                let eps = 1e-12;
                t > eps && t < 1.0 - eps && s > eps && s < 1.0 - eps
            };

            // Unwrapped boundary chain starting at candidate xi (index copy
            // of xi appended at +wu·span).
            let chain_at = |xi: usize| -> Vec<[f64; 2]> {
                (0..=m)
                    .map(|j| {
                        let idx = (xi + j) % m;
                        let off = if xi + j >= m { wsign * span } else { 0.0 };
                        [b_su[idx] + off, b_sv[idx]]
                    })
                    .collect()
            };

            let mut choice: Option<usize> = None;
            'candidates: for xi in 0..m {
                let chain = chain_at(xi);
                let left = [chain[0][0], pole_sv];
                let right = [chain[m][0], pole_sv];
                // The two seam verticals and the pole bottom edge must cross
                // no boundary edge (interior crossings only; the shared
                // chain endpoints are excluded by the open interval).
                let seam_segs = [(chain[0], left), (chain[m], right), (left, right)];
                for w in chain.windows(2) {
                    for &(p, q) in &seam_segs {
                        if seg_cross(p, q, w[0], w[1]) {
                            continue 'candidates;
                        }
                    }
                }
                // Holes sit inside the region; the seam must not split one.
                // (Hole u-extents are compared in the chain's period.)
                let u_lo = chain[0][0].min(chain[m][0]);
                let u_hi = chain[0][0].max(chain[m][0]);
                let u_center = 0.5 * (u_lo + u_hi);
                let splits_hole = h_loops.iter().any(|(hu, _)| {
                    let du = ((umean(hu) - u_center) / span).round() * span;
                    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
                    for &u in hu {
                        lo = lo.min(u - du);
                        hi = hi.max(u - du);
                    }
                    for &x in &[chain[0][0], chain[m][0]] {
                        if x > lo && x < hi {
                            return true;
                        }
                    }
                    false
                });
                if splits_hole {
                    continue 'candidates;
                }
                choice = Some(xi);
                break;
            }
            let xi = choice?;
            let chain = chain_at(xi);

            // Meridian seam subdivision: shared 3D samples for BOTH copies
            // (bit-identical — the weld below closes the seam watertight).
            let seg = max_3d_area.sqrt().max(1e-9 * r);
            let v_top = chain[0][1];
            let k_sub = (((v_top - pole_sv).abs() / seg).ceil() as usize).max(1);
            let u_seam = b_su[xi] / r; // seam meridian angle (period-agnostic)
            let seam_pts: Vec<(f64, Point3)> = (1..k_sub)
                .map(|k| {
                    let sv = v_top + (pole_sv - v_top) * (k as f64) / (k_sub as f64);
                    (sv, eval(u_seam, sv / r))
                })
                .collect();
            // The UV pole line must ALSO be subdivided at `seg`: as a single
            // 2π·r-long constraint edge its diametral (encroachment) circle
            // covers most of the domain, and spade's `keep_constraint_edges`
            // refinement then refuses nearly every Steiner insertion —
            // leaving equator-to-pole chord triangles (measured: 19% area
            // deficit on a hemisphere). Every subdivision point carries the
            // SAME 3D pole point; the weld collapses them into the fan apex.
            let k_bot = (((chain[m][0] - chain[0][0]).abs() / seg).ceil() as usize).max(1);

            // Ring: B_0..B_m (unwrapped, B_m = B_0's period copy), seam down
            // at u = chain[m][0], the subdivided pole line (right → left, one
            // 3D point), seam up at u = chain[0][0].
            let mut o: Vec<u32> = Vec::with_capacity(m + 2 + k_bot + 2 * seam_pts.len());
            for (j, uv) in chain.iter().enumerate() {
                o.push(verts2d.len() as u32);
                verts2d.push(cad_primitives::Point2::new(uv[0], uv[1]));
                vert3d.push(boundary[(xi + j) % m]);
            }
            for &(sv, p3) in &seam_pts {
                o.push(verts2d.len() as u32);
                verts2d.push(cad_primitives::Point2::new(chain[m][0], sv));
                vert3d.push(p3);
            }
            for k in 0..=k_bot {
                let u = chain[m][0] + (chain[0][0] - chain[m][0]) * (k as f64) / (k_bot as f64);
                o.push(verts2d.len() as u32);
                verts2d.push(cad_primitives::Point2::new(u, pole_sv));
                vert3d.push(pole_3d);
            }
            for &(sv, p3) in seam_pts.iter().rev() {
                o.push(verts2d.len() as u32);
                verts2d.push(cad_primitives::Point2::new(chain[0][0], sv));
                vert3d.push(p3);
            }

            // Holes, period-shifted into the unwrapped chain's u-window.
            let u_center = 0.5 * (chain[0][0] + chain[m][0]);
            for (li, (hu, hv)) in h_loops.iter().enumerate() {
                let du = ((umean(hu) - u_center) / span).round() * span;
                let mut hi = Vec::with_capacity(hu.len());
                for k in 0..hu.len() {
                    hi.push(verts2d.len() as u32);
                    verts2d.push(cad_primitives::Point2::new(hu[k] - du, hv[k]));
                    vert3d.push(holes[li][k]);
                }
                hole_idx.push(hi);
            }
            o
        }
        _ => return None, // |wu| ≥ 2: multi-wrap — out of scope
    };

    let (ref_verts, tris) =
        cherchi_rs::cdt_polygon_with_holes_refined(&verts2d, &outer, &hole_idx, max_3d_area)
            .ok()?;

    // Map back: ring/hole verts → EXACT input 3D (seam copies and pole
    // corners repeat one bit-identical Point3); Steiner verts → `eval`.
    let n = vert3d.len();
    let mut verts3d: Vec<Point3> = Vec::with_capacity(ref_verts.len());
    for (i, sp) in ref_verts.iter().enumerate() {
        if i < n {
            verts3d.push(vert3d[i]);
        } else {
            verts3d.push(eval(sp.x() / r, sp.y() / r));
        }
    }

    // Weld bit-identical positions (the two seam copies + the two pole
    // corners) and drop triangles that degenerate under the weld — this is
    // what closes the pole fan and the seam watertight.
    let mut first_at: std::collections::BTreeMap<[u64; 3], u32> = std::collections::BTreeMap::new();
    let mut remap: Vec<u32> = Vec::with_capacity(verts3d.len());
    for p in &verts3d {
        let key = [p.x().to_bits(), p.y().to_bits(), p.z().to_bits()];
        let id = *first_at.entry(key).or_insert(remap.len() as u32);
        remap.push(id);
    }
    let welded: Vec<[u32; 3]> = tris
        .iter()
        .map(|t| {
            [
                remap[t[0] as usize],
                remap[t[1] as usize],
                remap[t[2] as usize],
            ]
        })
        .filter(|t| t[0] != t[1] && t[1] != t[2] && t[2] != t[0])
        .collect();
    if welded.is_empty() {
        return None;
    }
    Some((verts3d, welded))
}

/// KV14 Slice F: Stage-1 tessellation of a boolean-result TORUS lateral carrying
/// inner loops — a POLOIDAL PERIODIC BAND (the boundary wraps the tube; the
/// toroidal sweep is bounded). Delegates to [`tessellate_torus_patch`] (the same
/// UV-CDT the render path uses: it projects the boundary into the (meridian,
/// longitude) plane, seam-bridges the two meridian-wrapping profiles with
/// on-surface subdivision, and refines interior Steiner points onto the torus —
/// exactly what a non-developable band needs).
///
/// The patch returns a FRESH vertex pool (boundary verts bit-exact, seam/Steiner
/// verts on-surface). Map back by QUANTIZED position: a profile-boundary vert
/// recovers its EXISTING shared global (watertight with adjacent caps); a seam
/// duplicate or Steiner vert welds by position (the two seam copies are one
/// meridian, ULP-apart, so an exact key would leave a crack — Cherchi needs the
/// band watertight). A holed band (a window in the tube, Slice F-2) carves each
/// non-wrapping window as a CDT hole; only seam-crossing / longitude-wrapping
/// patches return `None` from the patch tessellator → a typed wall.
#[allow(clippy::too_many_arguments)]
pub(crate) fn tessellate_torus_band(
    f_idx: usize,
    f: &BRepFace,
    edges: &[BRepEdge],
    chains: &std::collections::BTreeMap<u32, Vec<u32>>,
    center: Point3,
    axis_dir: Vector3,
    major: f64,
    minor: f64,
    out_verts: &mut Vec<Point3>,
    sources: &mut Vec<TessellationSource>,
    out_tris: &mut Vec<[u32; 3]>,
) -> Result<(), YangError> {
    // Gather boundary + hole loops as ordered global-vertex chains, then 3D.
    let outer_g = loop_polyline(f_idx, &f.outer_loop, edges, chains)?;
    let mut holes_g: Vec<Vec<u32>> = Vec::with_capacity(f.inner_loops.len());
    for inner in &f.inner_loops {
        holes_g.push(loop_polyline(f_idx, inner, edges, chains)?);
    }
    let boundary: Vec<Point3> = outer_g.iter().map(|&g| out_verts[g as usize]).collect();
    let holes: Vec<Vec<Point3>> = holes_g
        .iter()
        .map(|h| h.iter().map(|&g| out_verts[g as usize]).collect())
        .collect();

    // Quantized position key (1e-9 m ≪ TAU_MODEL 1e-7 ≪ MIN_FEATURE_SIZE 1e-6):
    // welds ULP-apart seam twins without merging distinct model features.
    let q = 1e-9_f64;
    let key = |p: Point3| -> (i64, i64, i64) {
        let a = p.as_array();
        (
            (a[0] / q).round() as i64,
            (a[1] / q).round() as i64,
            (a[2] / q).round() as i64,
        )
    };
    // Pre-seed with the EXISTING boundary/hole globals so they stay shared.
    let mut pos_to_global: std::collections::HashMap<(i64, i64, i64), u32> =
        std::collections::HashMap::new();
    for &g in outer_g.iter().chain(holes_g.iter().flatten()) {
        pos_to_global.entry(key(out_verts[g as usize])).or_insert(g);
    }

    // Triangle-area budget in arc-length² — a 1% chord tolerance on the tube
    // sets the meridian spacing; the patch scales (u,v) to arc-length so this is
    // a true area cap.
    let d_eps = 1e-2 * (major + minor);
    let dphi = (8.0 * d_eps / minor).sqrt().min(0.5);
    let n_seg = ((2.0 * std::f64::consts::PI / dphi).ceil() as u32).max(12);
    let seg = 2.0 * std::f64::consts::PI * minor / f64::from(n_seg);
    let max_area = seg * seg;

    let Some((verts, tris)) =
        tessellate_torus_patch(center, axis_dir, major, minor, &boundary, &holes, max_area)
    else {
        return Err(YangError::MalformedTopology(format!(
            "face {f_idx}: torus band UV-CDT unsupported (seam-crossing / \
             longitude-wrapping patch — KV14 Slice F later sub-slice)"
        )));
    };

    // Map every patch vertex to a global index (existing boundary shared,
    // seam/Steiner welded + freshly created with an on-surface source).
    let au = normalize3(axis_dir.as_array());
    let cen = center.as_array();
    let (e1v, e2v) = ortho_basis(axis_dir);
    let (e1a, e2a) = (e1v.as_array(), e2v.as_array());
    let mut global_of_patch: Vec<u32> = Vec::with_capacity(verts.len());
    for &p in &verts {
        if let Some(&g) = pos_to_global.get(&key(p)) {
            global_of_patch.push(g);
            continue;
        }
        // Fresh vertex: invert to (φ poloidal, θ toroidal) for the source so
        // `eval_source` round-trips through the torus arm.
        let pa = p.as_array();
        let w = [pa[0] - cen[0], pa[1] - cen[1], pa[2] - cen[2]];
        let tau = w[0] * au[0] + w[1] * au[1] + w[2] * au[2];
        let rv = [w[0] - tau * au[0], w[1] - tau * au[1], w[2] - tau * au[2]];
        let wx = rv[0] * e1a[0] + rv[1] * e1a[1] + rv[2] * e1a[2];
        let wy = rv[0] * e2a[0] + rv[1] * e2a[1] + rv[2] * e2a[2];
        let rho = (wx * wx + wy * wy).sqrt();
        let phi = tau.atan2(rho - major);
        let theta = wy.atan2(wx);
        let g = out_verts.len() as u32;
        out_verts.push(p);
        sources.push(TessellationSource::BRepFace {
            face: f_idx as u32,
            u: phi,
            v: theta,
        });
        pos_to_global.insert(key(p), g);
        global_of_patch.push(g);
    }

    // Emit, orienting each triangle by the analytic torus outward normal (inward
    // if `reversed` — a cavity wall).
    for t in &tris {
        let mut tri = [
            global_of_patch[t[0] as usize],
            global_of_patch[t[1] as usize],
            global_of_patch[t[2] as usize],
        ];
        // Outward = centroid − nearest tube-centre-circle point.
        let cn = {
            let a = out_verts[tri[0] as usize].as_array();
            let b = out_verts[tri[1] as usize].as_array();
            let c = out_verts[tri[2] as usize].as_array();
            [
                (a[0] + b[0] + c[0]) / 3.0,
                (a[1] + b[1] + c[1]) / 3.0,
                (a[2] + b[2] + c[2]) / 3.0,
            ]
        };
        let w = [cn[0] - cen[0], cn[1] - cen[1], cn[2] - cen[2]];
        let tau = w[0] * au[0] + w[1] * au[1] + w[2] * au[2];
        let rv = [w[0] - tau * au[0], w[1] - tau * au[1], w[2] - tau * au[2]];
        let rl = (rv[0] * rv[0] + rv[1] * rv[1] + rv[2] * rv[2])
            .sqrt()
            .max(1e-300);
        let rhat = [rv[0] / rl, rv[1] / rl, rv[2] / rl];
        let mut n = [
            cn[0] - (cen[0] + major * rhat[0]),
            cn[1] - (cen[1] + major * rhat[1]),
            cn[2] - (cen[2] + major * rhat[2]),
        ];
        if f.reversed {
            n = [-n[0], -n[1], -n[2]];
        }
        orient_tri(out_verts, &mut tri, n);
        out_tris.push(tri);
    }
    Ok(())
}

#[cfg(test)]
mod torus_patch_tests {
    use super::*;
    use std::collections::BTreeMap;

    #[allow(clippy::too_many_arguments)]
    fn eval(
        center: Point3,
        ax: [f64; 3],
        e1a: [f64; 3],
        e2a: [f64; 3],
        major: f64,
        minor: f64,
        u: f64,
        v: f64,
    ) -> Point3 {
        let c = center.as_array();
        let (cu, su) = (u.cos(), u.sin());
        let (cv, sv) = (v.cos(), v.sin());
        let rad = major + minor * cu;
        Point3::new(
            c[0] + rad * (cv * e1a[0] + sv * e2a[0]) + minor * su * ax[0],
            c[1] + rad * (cv * e1a[1] + sv * e2a[1]) + minor * su * ax[1],
            c[2] + rad * (cv * e1a[2] + sv * e2a[2]) + minor * su * ax[2],
        )
    }

    #[test]
    fn torus_patch_roundtrip_on_surface_watertight() {
        // Torus: center origin, axis +Z, R=3, r=1.
        let center = Point3::new(0.0, 0.0, 0.0);
        let axis = Vector3::new(0.0, 0.0, 1.0);
        let (major, minor) = (3.0_f64, 1.0_f64);
        let ax = normalize3(axis.as_array());
        let (e1, e2) = ortho_basis(axis);
        let (e1a, e2a) = (e1.as_array(), e2.as_array());

        // A sub-(u,v)-rectangle patch boundary, finely sampled along its 4 edges.
        let (u0, u1, v0, v1) = (0.2_f64, 1.2_f64, 0.5_f64, 1.8_f64);
        let ns = 8;
        let mut boundary: Vec<Point3> = Vec::new();
        let mut push =
            |u: f64, v: f64| boundary.push(eval(center, ax, e1a, e2a, major, minor, u, v));
        for k in 0..ns {
            let t = k as f64 / ns as f64;
            push(u0 + (u1 - u0) * t, v0);
        }
        for k in 0..ns {
            let t = k as f64 / ns as f64;
            push(u1, v0 + (v1 - v0) * t);
        }
        for k in 0..ns {
            let t = k as f64 / ns as f64;
            push(u1 - (u1 - u0) * t, v1);
        }
        for k in 0..ns {
            let t = k as f64 / ns as f64;
            push(u0, v1 - (v1 - v0) * t);
        }

        let n = boundary.len();
        let (verts, tris) =
            tessellate_torus_patch(center, axis, major, minor, &boundary, &[], 0.05)
                .expect("patch tessellation");

        // Interior Steiner points were added (refinement actually fired).
        assert!(verts.len() > n, "no Steiner points: {} verts", verts.len());

        // Boundary verts preserved bit-for-bit (conformal).
        for i in 0..n {
            assert_eq!(verts[i], boundary[i], "boundary vert {i} moved");
        }

        // Every vert lies on the torus surface.
        let surf = Surface::Torus {
            center,
            axis_dir: axis,
            major_radius: major,
            minor_radius: minor,
        };
        for (i, &p) in verts.iter().enumerate() {
            let d = signed_distance_to_surface(surf, p).expect("torus distance");
            assert!(d.abs() < 1e-9, "vert {i} off torus: d={d}");
        }

        // Manifold/watertight: every edge in 1 (boundary) or 2 (interior) tris.
        let mut edges: BTreeMap<(u32, u32), u32> = BTreeMap::new();
        for t in &tris {
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                *edges.entry((a.min(b), a.max(b))).or_default() += 1;
            }
        }
        assert!(
            edges.values().all(|&c| c == 1 || c == 2),
            "non-manifold edge present"
        );

        // The closed boundary loop is exactly the count-1 edges (no slits).
        let boundary_edges = edges.values().filter(|&&c| c == 1).count();
        assert_eq!(
            boundary_edges, n,
            "boundary edge count {boundary_edges} != {n}"
        );

        // The chorded 3D area matches the analytic patch area (a faithful, hole-
        // free, non-folded tessellation). Analytic area of the (u,v) rectangle:
        //   ∫∫ (R + r·cos u)·r du dv = r·(v1−v0)·[R·(u1−u0) + r·(sin u1 − sin u0)].
        let analytic = minor * (v1 - v0) * (major * (u1 - u0) + minor * (u1.sin() - u0.sin()));
        let mut area3d = 0.0;
        for t in &tris {
            let a = verts[t[0] as usize].as_array();
            let b = verts[t[1] as usize].as_array();
            let c = verts[t[2] as usize].as_array();
            let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let cr = [
                ab[1] * ac[2] - ab[2] * ac[1],
                ab[2] * ac[0] - ab[0] * ac[2],
                ab[0] * ac[1] - ab[1] * ac[0],
            ];
            area3d += 0.5 * (cr[0] * cr[0] + cr[1] * cr[1] + cr[2] * cr[2]).sqrt();
        }
        // Inscribed chords slightly under-shoot the smooth area; refinement keeps
        // it within ~1%. It must never exceed the smooth area (that would signal
        // folded/overlapping triangles).
        assert!(
            area3d <= analytic * (1.0 + 1e-6) && area3d >= analytic * 0.985,
            "area3d {area3d} vs analytic {analytic} (folds or holes?)"
        );
    }

    #[test]
    fn torus_patch_rejects_degenerate() {
        let center = Point3::new(0.0, 0.0, 0.0);
        let axis = Vector3::new(0.0, 0.0, 1.0);
        let too_few = [Point3::new(4.0, 0.0, 0.0), Point3::new(3.0, 0.0, 1.0)];
        assert!(tessellate_torus_patch(center, axis, 3.0, 1.0, &too_few, &[], 0.05).is_none());
    }

    /// Seam-wrapping (cylindrical) BAND render: a longitude slice v ∈ [v0, v1] of
    /// the tube wraps the full meridian (u ∈ [0, 2π)). Bounded by two meridian
    /// circles (opposite winding), it is triangulated via the universal-cover
    /// seam bridge into a watertight, on-tube mesh.
    #[test]
    fn torus_band_seam_wrapping_render() {
        let center = Point3::new(0.0, 0.0, 0.0);
        let axis = Vector3::new(0.0, 0.0, 1.0);
        let (major, minor) = (3.0_f64, 1.0_f64);
        let ax = normalize3(axis.as_array());
        let (e1, e2) = ortho_basis(axis);
        let (e1a, e2a) = (e1.as_array(), e2.as_array());
        // A FULL-quarter longitude slice (Δv = π/2): a large band whose seam
        // bridges must be subdivided, or the edge regions stay coarse and the
        // chorded area undershoots (the KV6d band-render regression).
        let (v0, v1) = (0.0_f64, std::f64::consts::FRAC_PI_2);
        let nu = 24;
        // Two meridian circles: v0 wound +u (wrap +1), v1 wound −u (wrap −1).
        let mut c0: Vec<Point3> = Vec::new();
        let mut c1: Vec<Point3> = Vec::new();
        for k in 0..nu {
            let u = std::f64::consts::TAU * (k as f64) / (nu as f64);
            c0.push(eval(center, ax, e1a, e2a, major, minor, u, v0));
            c1.push(eval(center, ax, e1a, e2a, major, minor, -u, v1));
        }
        let (verts, tris) = tessellate_torus_patch(center, axis, major, minor, &c0, &[c1], 0.05)
            .expect("band tessellation");
        assert!(!tris.is_empty(), "non-empty band mesh");

        // Every vertex on the tube.
        let surf = torus(major, minor);
        for (i, &p) in verts.iter().enumerate() {
            let d = signed_distance_to_surface(surf, p).unwrap();
            assert!(d.abs() < 1e-9, "band vert {i} off tube: {d:e}");
        }
        // Manifold + watertight across the SEAM: group edges by 3D POSITION (the
        // periodic seam's duplicated vertices coincide in 3D). Every edge is
        // shared by 2 tris (interior + the seam, where the universal-cover bridge
        // duplicates coincide) EXCEPT the band's two real meridian-circle
        // boundaries at v0 / v1 (shared by 1). No edge is shared by >2.
        let key = |p: Point3| {
            let a = p.as_array();
            [
                (a[0] * 1e7).round() as i64,
                (a[1] * 1e7).round() as i64,
                (a[2] * 1e7).round() as i64,
            ]
        };
        let mut edges: BTreeMap<([i64; 3], [i64; 3]), u32> = BTreeMap::new();
        for t in &tris {
            for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                let (ka, kb) = (key(verts[a as usize]), key(verts[b as usize]));
                let e = if ka < kb { (ka, kb) } else { (kb, ka) };
                *edges.entry(e).or_insert(0) += 1;
            }
        }
        assert!(
            edges.values().all(|&c| c == 1 || c == 2),
            "non-manifold edge (some positional edge in >2 tris) — seam not watertight"
        );
        // Exactly the two meridian-circle boundaries (nu edges each) are count-1;
        // the seam bridges coincide in 3D and are interior (count-2).
        let boundary_edges = edges.values().filter(|&&c| c == 1).count();
        assert_eq!(
            boundary_edges,
            2 * nu,
            "expected the two v-circle boundaries ({} edges), got {boundary_edges}",
            2 * nu
        );

        // The chorded area approaches the analytic band area
        // ∫∫ (R + r cos u)·r du dv = r·(v1−v0)·2π·R  (∫cos u over a full turn = 0).
        let analytic = minor * (v1 - v0) * std::f64::consts::TAU * major;
        let mut area = 0.0;
        for t in &tris {
            let a = verts[t[0] as usize].as_array();
            let b = verts[t[1] as usize].as_array();
            let c = verts[t[2] as usize].as_array();
            let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let cr = [
                ab[1] * ac[2] - ab[2] * ac[1],
                ab[2] * ac[0] - ab[0] * ac[2],
                ab[0] * ac[1] - ab[1] * ac[0],
            ];
            area += 0.5 * (cr[0] * cr[0] + cr[1] * cr[1] + cr[2] * cr[2]).sqrt();
        }
        assert!(
            area <= analytic * (1.0 + 1e-6) && area >= analytic * 0.97,
            "band area {area} vs analytic {analytic}"
        );
    }

    /// KV14 Slice F-2: a seam-wrapping torus BAND with a WINDOW HOLE in the tube
    /// wall — the two meridian-circle band edges wrap the full meridian, and a
    /// small non-wrapping (u,v) window is excluded. The band's universal-cover
    /// seam bridge must still triangulate the outer ring while the window is
    /// carved as a CDT hole (placed into the band's unrolled u-period).
    #[test]
    fn torus_band_with_window_hole_render() {
        let center = Point3::new(0.0, 0.0, 0.0);
        let axis = Vector3::new(0.0, 0.0, 1.0);
        let (major, minor) = (3.0_f64, 1.0_f64);
        let ax = normalize3(axis.as_array());
        let (e1, e2) = ortho_basis(axis);
        let (e1a, e2a) = (e1.as_array(), e2.as_array());
        // Band longitude slice v ∈ [0, π/2]; meridian wraps fully.
        let (v0, v1) = (0.0_f64, std::f64::consts::FRAC_PI_2);
        let nu = 24;
        let mut c0: Vec<Point3> = Vec::new();
        let mut c1: Vec<Point3> = Vec::new();
        for k in 0..nu {
            let u = std::f64::consts::TAU * (k as f64) / (nu as f64);
            c0.push(eval(center, ax, e1a, e2a, major, minor, u, v0));
            c1.push(eval(center, ax, e1a, e2a, major, minor, -u, v1));
        }
        // A small non-wrapping window inside the band: u ∈ [1.0, 2.0],
        // v ∈ [0.4, 1.0] (both well inside the band's ranges, non-wrapping).
        let (wu0, wu1, wv0, wv1) = (1.0_f64, 2.0_f64, 0.4_f64, 1.0_f64);
        let nw = 6;
        let mut win: Vec<Point3> = Vec::new();
        let mut wpush = |u: f64, v: f64| win.push(eval(center, ax, e1a, e2a, major, minor, u, v));
        for k in 0..nw {
            wpush(wu0 + (wu1 - wu0) * (k as f64 / nw as f64), wv0);
        }
        for k in 0..nw {
            wpush(wu1, wv0 + (wv1 - wv0) * (k as f64 / nw as f64));
        }
        for k in 0..nw {
            wpush(wu1 - (wu1 - wu0) * (k as f64 / nw as f64), wv1);
        }
        for k in 0..nw {
            wpush(wu0, wv1 - (wv1 - wv0) * (k as f64 / nw as f64));
        }
        let win_edges = win.len();

        let (verts, tris) =
            tessellate_torus_patch(center, axis, major, minor, &c0, &[c1, win], 0.05)
                .expect("holed band tessellation");
        assert!(!tris.is_empty(), "non-empty holed band mesh");

        // Every vertex on the tube.
        let surf = torus(major, minor);
        for (i, &p) in verts.iter().enumerate() {
            let d = signed_distance_to_surface(surf, p).unwrap();
            assert!(d.abs() < 1e-9, "holed band vert {i} off tube: {d:e}");
        }
        // Manifold + watertight by 3D position; the three boundaries (2 meridian
        // circles + 1 window) are count-1, everything else count-2, none > 2.
        let key = |p: Point3| {
            let a = p.as_array();
            [
                (a[0] * 1e7).round() as i64,
                (a[1] * 1e7).round() as i64,
                (a[2] * 1e7).round() as i64,
            ]
        };
        let mut edges: BTreeMap<([i64; 3], [i64; 3]), u32> = BTreeMap::new();
        for t in &tris {
            for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                let (ka, kb) = (key(verts[a as usize]), key(verts[b as usize]));
                let e = if ka < kb { (ka, kb) } else { (kb, ka) };
                *edges.entry(e).or_insert(0) += 1;
            }
        }
        assert!(
            edges.values().all(|&c| c == 1 || c == 2),
            "non-manifold edge (some positional edge in >2 tris)"
        );
        let boundary_edges = edges.values().filter(|&&c| c == 1).count();
        assert_eq!(
            boundary_edges,
            2 * nu + win_edges,
            "expected 2 meridian circles ({}) + 1 window ({win_edges}), got {boundary_edges}",
            2 * nu
        );

        // Chorded area ≈ analytic band area MINUS the excluded window area.
        //   band:   r·(v1−v0)·2π·R
        //   window: r·(wv1−wv0)·[R·(wu1−wu0) + r·(sin wu1 − sin wu0)]
        let band_area = minor * (v1 - v0) * std::f64::consts::TAU * major;
        let win_area =
            minor * (wv1 - wv0) * (major * (wu1 - wu0) + minor * (wu1.sin() - wu0.sin()));
        let analytic = band_area - win_area;
        let mut area = 0.0;
        for t in &tris {
            let a = verts[t[0] as usize].as_array();
            let b = verts[t[1] as usize].as_array();
            let c = verts[t[2] as usize].as_array();
            let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let cr = [
                ab[1] * ac[2] - ab[2] * ac[1],
                ab[2] * ac[0] - ab[0] * ac[2],
                ab[0] * ac[1] - ab[1] * ac[0],
            ];
            area += 0.5 * (cr[0] * cr[0] + cr[1] * cr[1] + cr[2] * cr[2]).sqrt();
        }
        assert!(
            area <= analytic * (1.0 + 1e-6) && area >= analytic * 0.97,
            "holed band area {area} vs analytic {analytic} (band {band_area} − window {win_area})"
        );
    }

    /// KV14 Slice F-2 seam-avoidance branch: the window straddles the DEFAULT
    /// seam (meridian u=0, where both band edges are anchored). `band_seam_bridge`
    /// must skip that anchor and cut the seam elsewhere, so the window projects as
    /// a simple interior hole (not split across the seam → CDT self-intersection).
    /// This is R0028's complement-band wall in miniature.
    #[test]
    fn torus_band_window_on_seam_render() {
        let center = Point3::new(0.0, 0.0, 0.0);
        let axis = Vector3::new(0.0, 0.0, 1.0);
        let (major, minor) = (3.0_f64, 1.0_f64);
        let ax = normalize3(axis.as_array());
        let (e1, e2) = ortho_basis(axis);
        let (e1a, e2a) = (e1.as_array(), e2.as_array());
        let (v0, v1) = (0.0_f64, std::f64::consts::FRAC_PI_2);
        let nu = 24;
        // Band edges anchored at meridian u=0 (k=0), same as the default seam.
        let mut c0: Vec<Point3> = Vec::new();
        let mut c1: Vec<Point3> = Vec::new();
        for k in 0..nu {
            let u = std::f64::consts::TAU * (k as f64) / (nu as f64);
            c0.push(eval(center, ax, e1a, e2a, major, minor, u, v0));
            c1.push(eval(center, ax, e1a, e2a, major, minor, -u, v1));
        }
        // Window centred ON the u=0 seam: u ∈ [−0.3, 0.3], v ∈ [0.4, 1.0].
        let (wu0, wu1, wv0, wv1) = (-0.3_f64, 0.3_f64, 0.4_f64, 1.0_f64);
        let nw = 6;
        let mut win: Vec<Point3> = Vec::new();
        let mut wpush = |u: f64, v: f64| win.push(eval(center, ax, e1a, e2a, major, minor, u, v));
        for k in 0..nw {
            wpush(wu0 + (wu1 - wu0) * (k as f64 / nw as f64), wv0);
        }
        for k in 0..nw {
            wpush(wu1, wv0 + (wv1 - wv0) * (k as f64 / nw as f64));
        }
        for k in 0..nw {
            wpush(wu1 - (wu1 - wu0) * (k as f64 / nw as f64), wv1);
        }
        for k in 0..nw {
            wpush(wu0, wv1 - (wv1 - wv0) * (k as f64 / nw as f64));
        }
        let win_edges = win.len();

        let (verts, tris) =
            tessellate_torus_patch(center, axis, major, minor, &c0, &[c1, win], 0.05)
                .expect("seam-straddling window band tessellates (seam avoided)");

        // Every vertex on the tube.
        let surf = torus(major, minor);
        for (i, &p) in verts.iter().enumerate() {
            let d = signed_distance_to_surface(surf, p).unwrap();
            assert!(d.abs() < 1e-9, "vert {i} off tube: {d:e}");
        }
        // Watertight/manifold by 3D position; the window survives as a boundary.
        let key = |p: Point3| {
            let a = p.as_array();
            [
                (a[0] * 1e7).round() as i64,
                (a[1] * 1e7).round() as i64,
                (a[2] * 1e7).round() as i64,
            ]
        };
        let mut edges: BTreeMap<([i64; 3], [i64; 3]), u32> = BTreeMap::new();
        for t in &tris {
            for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                let (ka, kb) = (key(verts[a as usize]), key(verts[b as usize]));
                let e = if ka < kb { (ka, kb) } else { (kb, ka) };
                *edges.entry(e).or_insert(0) += 1;
            }
        }
        assert!(
            edges.values().all(|&c| c == 1 || c == 2),
            "non-manifold edge (seam or window split)"
        );
        let boundary_edges = edges.values().filter(|&&c| c == 1).count();
        assert_eq!(
            boundary_edges,
            2 * nu + win_edges,
            "expected 2 meridian circles + 1 window, got {boundary_edges}"
        );
        // Area = band − window (a split window would leak area or fold).
        let band_area = minor * (v1 - v0) * std::f64::consts::TAU * major;
        let win_area =
            minor * (wv1 - wv0) * (major * (wu1 - wu0) + minor * (wu1.sin() - wu0.sin()));
        let analytic = band_area - win_area;
        let mut area = 0.0;
        for t in &tris {
            let a = verts[t[0] as usize].as_array();
            let b = verts[t[1] as usize].as_array();
            let c = verts[t[2] as usize].as_array();
            let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let cr = [
                ab[1] * ac[2] - ab[2] * ac[1],
                ab[2] * ac[0] - ab[0] * ac[2],
                ab[0] * ac[1] - ab[1] * ac[0],
            ];
            area += 0.5 * (cr[0] * cr[0] + cr[1] * cr[1] + cr[2] * cr[2]).sqrt();
        }
        assert!(
            area <= analytic * (1.0 + 1e-6) && area >= analytic * 0.97,
            "seam-window band area {area} vs analytic {analytic}"
        );
    }

    fn torus(major: f64, minor: f64) -> Surface {
        Surface::Torus {
            center: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            major_radius: major,
            minor_radius: minor,
        }
    }

    #[test]
    fn newton_relocates_onto_torus_plane_intersection() {
        // Torus R=3 r=1 axis +z; oblique-ish plane x = 3.4 (a spiric section,
        // NOT a conic). A chord point near the curve must land on BOTH surfaces.
        let t = torus(3.0, 1.0);
        let plane = Surface::Plane {
            normal: Vector3::new(1.0, 0.0, 0.0),
            d: -3.4,
        };
        // Seed: a torus surface point near x≈3.4, nudged off both surfaces.
        let (u, v) = (0.7_f64, 0.15_f64);
        let rad = 3.0 + 1.0 * u.cos();
        let seed = Point3::new(
            rad * v.cos() + 0.03,
            rad * v.sin() - 0.02,
            1.0 * u.sin() + 0.04,
        );
        let relocated = relocate_onto_implicit_pair(seed, t, plane).expect("converges");
        let ft = signed_distance_to_surface(t, relocated).unwrap();
        let fp = signed_distance_to_surface(plane, relocated).unwrap();
        assert!(ft.abs() <= cad_primitives::TAU_MODEL, "off torus: {ft:e}");
        assert!(fp.abs() <= cad_primitives::TAU_MODEL, "off plane: {fp:e}");
    }

    #[test]
    fn newton_relocates_onto_torus_cylinder_intersection() {
        let t = torus(3.0, 1.0);
        // Cylinder coaxial-offset: axis ∥ +y through (3,0,0), radius 0.6 — cuts
        // the tube near θ=0 in a degree-4 curve.
        let cyl = Surface::Cylinder {
            axis_point: Point3::new(3.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 1.0, 0.0),
            radius: 0.6,
        };
        let seed = Point3::new(3.5, 0.1, 0.45);
        let r = relocate_onto_implicit_pair(seed, t, cyl).expect("converges");
        assert!(signed_distance_to_surface(t, r).unwrap().abs() <= cad_primitives::TAU_MODEL);
        assert!(signed_distance_to_surface(cyl, r).unwrap().abs() <= cad_primitives::TAU_MODEL);
    }

    #[test]
    fn newton_stops_when_there_is_no_intersection() {
        // Plane x = 10 lies entirely outside the torus (max x = R+r = 4): no
        // common zero, so the relocation must REFUSE (no curve to land on)
        // rather than wander to a wrong point.
        let t = torus(3.0, 1.0);
        let far = Surface::Plane {
            normal: Vector3::new(1.0, 0.0, 0.0),
            d: -10.0,
        };
        let seed = Point3::new(3.5, 0.0, 0.2);
        assert!(
            relocate_onto_implicit_pair(seed, t, far).is_none(),
            "no intersection ⇒ STOP, not a guessed relocation"
        );
    }
}

/// PR-YR12 (P2b): full outward radial normal of a sphere face at the centroid of
/// `tri` — `normalize(centroid − center)`. The analog of `radial_outward_normal`
/// but with no axis projection (a sphere is isotropic). Used to orient sphere
/// triangle winding via `orient_tri`.
pub(crate) fn sphere_outward_normal(verts: &[Point3], tri: &[u32; 3], center: Point3) -> [f64; 3] {
    let a = verts[tri[0] as usize].as_array();
    let b = verts[tri[1] as usize].as_array();
    let c = verts[tri[2] as usize].as_array();
    let cen = [
        (a[0] + b[0] + c[0]) / 3.0,
        (a[1] + b[1] + c[1]) / 3.0,
        (a[2] + b[2] + c[2]) / 3.0,
    ];
    let ctr = center.as_array();
    normalize3([cen[0] - ctr[0], cen[1] - ctr[1], cen[2] - ctr[2]])
}

/// PR-YR7: outward radial normal of the cylinder surface at the centroid of
/// `tri` — the component of `(centroid − axis_point)` perpendicular to the
/// axis, normalized. Used to orient lateral triangle winding (governance
/// A15.5). Falls back to the raw radial vector if it is (near-)axial.
pub(crate) fn radial_outward_normal(
    verts: &[Point3],
    tri: &[u32; 3],
    axis_point: [f64; 3],
    axis_unit: [f64; 3],
) -> [f64; 3] {
    let a = verts[tri[0] as usize].as_array();
    let b = verts[tri[1] as usize].as_array();
    let c = verts[tri[2] as usize].as_array();
    let cen = [
        (a[0] + b[0] + c[0]) / 3.0,
        (a[1] + b[1] + c[1]) / 3.0,
        (a[2] + b[2] + c[2]) / 3.0,
    ];
    let w = [
        cen[0] - axis_point[0],
        cen[1] - axis_point[1],
        cen[2] - axis_point[2],
    ];
    let along = w[0] * axis_unit[0] + w[1] * axis_unit[1] + w[2] * axis_unit[2];
    let radial = [
        w[0] - along * axis_unit[0],
        w[1] - along * axis_unit[1],
        w[2] - along * axis_unit[2],
    ];
    normalize3(radial)
}

/// PR-YR16 (spec §4): outward normal of a cone lateral at the centroid of `tri`.
///
/// The cone normal is TILTED ⟂ the generator (NOT purely radial like the
/// cylinder). A cone point is `P = apex + s·â + s·tanα·r̂` with generator
/// `g = â + tanα·r̂`; the surface normal lies in `span{â, r̂}` ⟂ `g`. Imposing
/// `n·g = 0` on `n = a·r̂ + b·â` gives `b = −a·tanα`, so the outward
/// (positive-radial) normal is `n̂ = unit(r̂ − tanα·â)`. The analog of
/// `radial_outward_normal` / `sphere_outward_normal`, feeding `orient_tri`. The
/// fan-triangle centroid sits at ≈ 2/3 of the way to the rim, so its radial
/// component is never degenerate near the apex.
pub(crate) fn cone_outward_normal(
    verts: &[Point3],
    tri: &[u32; 3],
    apex: Point3,
    axis_dir: Vector3,
    half_angle: f64,
) -> [f64; 3] {
    let a = verts[tri[0] as usize].as_array();
    let b = verts[tri[1] as usize].as_array();
    let c = verts[tri[2] as usize].as_array();
    let cen = [
        (a[0] + b[0] + c[0]) / 3.0,
        (a[1] + b[1] + c[1]) / 3.0,
        (a[2] + b[2] + c[2]) / 3.0,
    ];
    let ax = normalize3(axis_dir.as_array());
    let ap = apex.as_array();
    let w = [cen[0] - ap[0], cen[1] - ap[1], cen[2] - ap[2]];
    let along = w[0] * ax[0] + w[1] * ax[1] + w[2] * ax[2];
    let radial_vec = [
        w[0] - along * ax[0],
        w[1] - along * ax[1],
        w[2] - along * ax[2],
    ];
    let rhat = normalize3(radial_vec);
    let t = half_angle.tan();
    normalize3([
        rhat[0] - t * ax[0],
        rhat[1] - t * ax[1],
        rhat[2] - t * ax[2],
    ])
}

/// PR-YR7: flip `tri`'s winding (swap last two verts) if its geometric normal
/// `(v1−v0)×(v2−v0)` opposes the analytic outward normal `target`.
pub(crate) fn orient_tri(verts: &[Point3], tri: &mut [u32; 3], target: [f64; 3]) {
    let a = verts[tri[0] as usize].as_array();
    let b = verts[tri[1] as usize].as_array();
    let c = verts[tri[2] as usize].as_array();
    let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let cross = [
        e1[1] * e2[2] - e1[2] * e2[1],
        e1[2] * e2[0] - e1[0] * e2[2],
        e1[0] * e2[1] - e1[1] * e2[0],
    ];
    let dot = cross[0] * target[0] + cross[1] * target[1] + cross[2] * target[2];
    if dot < 0.0 {
        tri.swap(1, 2);
    }
}

/// Stage-1 chord bound for an ELLIPSE rim chain (KV14 ellipse-arc re-entry):
/// `d_ε = 1e-2 · major_radius` — the circle chord rule applied at the
/// ellipse's worst-case curvature scale. SINGLE SOURCE (A14.3, spec
/// `yang_s3_ellipse_rim_chord_bound` I3): the Stage-1 ellipse chain pre-pass
/// derives its sampling from this, and Stage-3's
/// `chord_tol_for_curved_owner` fallback reuses the SAME bound for owners
/// whose only curved rims are ellipses.
pub(crate) fn ellipse_chord_bound(major_radius: f64) -> f64 {
    1e-2 * major_radius
}

/// Stage-3 fallback bound for a curved-owning input with NO Circle rim: the
/// largest Stage-1 ellipse-chain bound over the owner's `Curve::Ellipse`
/// edges (spec `yang_s3_ellipse_rim_chord_bound` T2 — an obliquely-trimmed
/// cylinder re-entering from a prior boolean carries ellipse rims only).
/// `None` when the owner has no ellipse edge either (T3 — the loud producer
/// fault stands).
pub(crate) fn ellipse_rim_chord_bound(edges: &[BRepEdge]) -> Option<f64> {
    edges
        .iter()
        .filter_map(|e| match e.curve {
            Curve::Ellipse { major_radius, .. } => Some(ellipse_chord_bound(major_radius)),
            // KV16: hyperbola rims use the same 1e-2 rule at the conic's
            // scale (the S3 tol-lookup vocabulary lesson — a curved owner
            // whose only curved rims are hyperbolas must resolve a bound).
            Curve::Hyperbola {
                semi_transverse,
                semi_conjugate,
                ..
            } => Some(ellipse_chord_bound(semi_transverse.max(semi_conjugate))),
            _ => None,
        })
        .fold(None, |acc: Option<f64>, b| {
            Some(acc.map_or(b, |a| a.max(b)))
        })
}

/// PR-YR8 (P2c): the Stage-1 chord-error bound `d_ε = 1e-2 × analytic-AABB-diag`
/// for a solid, derived from its `Curve::Circle` rim edges (spec §4 Blocker 1).
///
/// This is the **single source** (governance A14.3) of the `1e-2` chord-bound
/// constant: both `BRep::new` (which derives the cylinder tessellation `n_seg`
/// from it) and Stage-6 face resolution (which uses it as the per-curved-face
/// membership tolerance, degenerate and non-degenerate alike) call this — there
/// is no second copy of the math or the literal anywhere in the crate.
///
/// Per axis a circle of center `c`, unit normal `n`, radius `r` spans
/// `c_i ± r·√(max(0, 1 − n_i²))`; the AABB is the union of those spans over all
/// rim circles. Returns:
/// - `Some(1e-2 × diag)` when the solid has ≥1 `Curve::Circle` rim (it has a
///   tessellated curved face, so it exposes a chord band), or
/// - `None` when there are no circle rims (an all-planar solid has zero chord
///   error; its faces resolve at `TAU_WORK`, not at a curved band).
pub(crate) fn curved_chord_bound(edges: &[BRepEdge]) -> Option<f64> {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    let mut any = false;
    for e in edges {
        if let Curve::Circle {
            center,
            normal,
            radius,
        } = e.curve
        {
            any = true;
            let nu = normalize3(normal.as_array());
            let c = center.as_array();
            for i in 0..3 {
                let span = radius * (1.0 - nu[i] * nu[i]).max(0.0).sqrt();
                lo[i] = lo[i].min(c[i] - span);
                hi[i] = hi[i].max(c[i] + span);
            }
        }
    }
    if !any {
        return None;
    }
    let dx = hi[0] - lo[0];
    let dy = hi[1] - lo[1];
    let dz = hi[2] - lo[2];
    let diag = (dx * dx + dy * dy + dz * dz).sqrt();
    Some(1e-2 * diag)
}

/// PR-YR15: the Stage-1 chord bound for a `Surface::Sphere` tessellation,
/// `d_ε = 1e-2 · 2r√3` (the sphere's bounding-cube diagonal × 1e-2). SINGLE
/// SOURCE OF TRUTH (A14.3): both `tessellate_sphere_face` (which derives the
/// tessellation `n_lon`/`n_lat` from it) and Stage-6 face resolution (`tol_for`,
/// which uses it as the per-sphere-face membership tolerance) call this — there
/// is no second copy of the literal anywhere in the crate.
///
/// NOTE: this is NOT `curved_chord_bound` (the Circle-rim AABB × 1e-2). The
/// rim circle's AABB diagonal is `2r√2`, which UNDERESTIMATES the sphere's own
/// `2r√3` chord error, so a sphere face must use its own bound here — not the
/// rim band. This is A14.3/A15, not tolerance widening.
pub(crate) fn sphere_chord_bound(radius: f64) -> f64 {
    1e-2 * 2.0 * radius * 3f64.sqrt()
}

/// PR-YR16 (spec §3): the Stage-1 chord bound for a `Surface::Cone`
/// tessellation, `d_ε = 1e-2 · √((2R)² + h²)` with `R = height·tan(half_angle)`.
/// SINGLE SOURCE OF TRUTH (A14.3) of the cone's `1e-2` literal: both the
/// pre-pass N-sizing (folded in via `min()` whenever a cone face is present)
/// and the test-side oracle compute this exact value, so they agree by
/// construction.
///
/// NOTE: this is NOT `curved_chord_bound` (the Circle-rim AABB × 1e-2). The
/// rim's AABB diagonal `2R√2` IGNORES the cone height and can EXCEED the cone's
/// honest bound for wide-short cones (`h < 2R`), so a cone face must fold in its
/// own bound — not rely on the rim band alone. This is A14.3/A15, not tolerance
/// widening.
pub(crate) fn cone_chord_bound(height: f64, half_angle: f64) -> f64 {
    let r = height * half_angle.tan();
    1e-2 * ((2.0 * r).powi(2) + height.powi(2)).sqrt()
}
