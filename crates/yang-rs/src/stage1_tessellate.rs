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

/// Stage 1 tessellation with per-`LineSegment`-edge JUNCTION points (P3a
/// #146 increment 1b, spec `yang_146_conformal_junction_sampling.md` §3.3) —
/// the [`stage1_tessellate_with_rim_overrides`] pattern generalized to
/// straight edge polylines. `edge_overrides[e]` lists exact pierce points to
/// insert into `LineSegment` edge `e`'s polyline as extra Steiner vertices at
/// their chord-parameter-sorted positions.
///
/// `LineSegment` edges use the per-loop-copy convention (kernel-v2
/// `to_yang.rs` m1: one directed yang edge per half-edge), so the caller must
/// fan the IDENTICAL point list to EVERY copy of a geometric edge
/// (`junction_pierce_points` does). Interior Steiner vertices are minted ONCE
/// per geometric edge and every copy's polyline splices the same mesh vertex
/// indices — the two incident faces stay conformal by identity.
///
/// An EMPTY map is byte-identical to [`stage1_tessellate`] (see the
/// `edge_override_empty_is_byte_identical` unit test).
///
/// Loud errors (`MalformedTopology`, mirroring the rim-override contract):
/// - a target edge that is out of range or not a `Curve::LineSegment`,
/// - a geometric edge with a MISSING or MISMATCHED per-loop-copy list
///   (broken fan-out would silently desync the two incident faces —
///   the exact defect this machinery exists to prevent),
/// - an override point off the edge's line (beyond the `1e-9·(1+scale)`
///   on-curve band) or outside the edge span `t ∈ (0, 1)`,
/// - an override within `TAU_MODEL·(1+scale)` of an endpoint that differs in
///   bits (B-Rep vertices are authoritative; a near-corner pierce is P3b
///   stitch territory, never a mid-edge sample),
/// - an overridden edge incident to a NON-PLANAR face (increment-1b scope is
///   planar-incident edges only — fail closed).
///
/// Bit-identical endpoint repeats and bit-identical duplicate points
/// deduplicate (skipped).
///
/// Increment 2 adds `face_overrides[f]`: exact junction points to mint as
/// INTERIOR Steiner vertices of PLANAR face `f`'s CDT (the partner side of
/// a pierce, spec §3.3 second bullet — interiors are CDT-freedom, findings
/// Q2). Loud errors: a target face that is out of range or non-planar, a
/// point off the face's plane, or a point the face's CDT does not consume
/// (outside the bounded region — a silent one-sided mint is the exact
/// conformality break this machinery exists to prevent). Bit-identical
/// duplicates dedup.
pub(crate) fn stage1_tessellate_with_edge_overrides(
    verts: &[BRepVertex],
    edges: &[BRepEdge],
    faces: &[BRepFace],
    edge_overrides: &std::collections::BTreeMap<u32, Vec<Point3>>,
    face_overrides: &std::collections::BTreeMap<u32, Vec<Point3>>,
    min_n_seg: Option<usize>,
) -> Result<Stage1Tess, YangError> {
    stage1_tessellate_inner_overrides(
        verts,
        edges,
        faces,
        &std::collections::BTreeMap::new(),
        edge_overrides,
        face_overrides,
        min_n_seg,
    )
    .map(|(t, _)| t)
}

/// [`stage1_tessellate_inner_overrides`] with no straight-edge junction
/// overrides — the pre-1b signature, kept for the existing rim-override
/// callers and unit fixtures.
#[allow(clippy::type_complexity)]
pub(crate) fn stage1_tessellate_inner(
    verts: &[BRepVertex],
    edges: &[BRepEdge],
    faces: &[BRepFace],
    rim_overrides: &std::collections::BTreeMap<u32, Vec<Point3>>,
    min_n_seg: Option<usize>,
) -> Result<(Stage1Tess, std::collections::BTreeSet<u32>), YangError> {
    stage1_tessellate_inner_overrides(
        verts,
        edges,
        faces,
        rim_overrides,
        &std::collections::BTreeMap::new(),
        &std::collections::BTreeMap::new(),
        min_n_seg,
    )
}

/// Inner implementation returning the tessellation AND the set of rim edges
/// that received inserted crossing points (consumed by the lateral dispatch).
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub(crate) fn stage1_tessellate_inner_overrides(
    verts: &[BRepVertex],
    edges: &[BRepEdge],
    faces: &[BRepFace],
    rim_overrides: &std::collections::BTreeMap<u32, Vec<Point3>>,
    edge_overrides: &std::collections::BTreeMap<u32, Vec<Point3>>,
    face_overrides: &std::collections::BTreeMap<u32, Vec<Point3>>,
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
                .filter(|_| cfg!(debug_assertions)) // dev-only, gated out of release (F12)
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
                    if std::env::var_os("YANG_SPLIT_PROBE").is_some() {
                        eprintln!(
                            "[ring-build] edge={e_idx} n_seg={n_seg} overrides={} merged={} \
                             inserted={} ring_len={}",
                            extra.len(),
                            merged_slots.len(),
                            inserted_keys.len(),
                            slots.len()
                        );
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

        // ---- LineSegment junction-override chain pre-pass (P3a #146
        // increment 1b, spec `yang_146_conformal_junction_sampling.md` §3.3).
        //
        // Overrides are grouped by GEOMETRIC edge (canonical bitwise endpoint
        // pair — `LineSegment` edges are per-loop copies, kernel-v2
        // `to_yang.rs` m1); interior Steiner vertices are minted ONCE per
        // geometric edge and each copy's chain splices the SAME mesh vertex
        // indices, oriented copy-start → copy-end, so every face incident to
        // the geometric edge consumes an identical polyline (conformality by
        // identity — keying or minting per copy would silently break it).
        let mut line_chain_edges: std::collections::BTreeSet<u32> =
            std::collections::BTreeSet::new();
        if !edge_overrides.is_empty() {
            let kb =
                |p: Point3| -> [u64; 3] { [p.x().to_bits(), p.y().to_bits(), p.z().to_bits()] };
            for &e_idx in edge_overrides.keys() {
                match edges.get(e_idx as usize) {
                    None => {
                        return Err(YangError::MalformedTopology(format!(
                            "edge override targets out-of-range edge {e_idx}"
                        )))
                    }
                    Some(e) if e.curve != Curve::LineSegment => {
                        return Err(YangError::MalformedTopology(format!(
                            "edge override targets non-LineSegment edge {e_idx} (curved-edge \
                             junction points go through rim_overrides)"
                        )))
                    }
                    Some(_) => {}
                }
            }
            // Group the FACE-LOOP-REFERENCED LineSegment edges by canonical
            // endpoint identity; copies collect in ascending index order
            // (deterministic canon). The face-loop traversal mirrors the
            // pierce enumeration's own copy discovery — an ORPHAN edge (in
            // `edges` but in no loop) contributes nothing to the mesh, so it
            // must not demand a fan-out entry (a chained-output B-Rep may
            // carry such edges; a spurious loud STOP here would fail the
            // whole boolean for a copy that cannot desync anything).
            let mut referenced: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
            for f in faces {
                for &ei in f.outer_loop.iter().chain(f.inner_loops.iter().flatten()) {
                    referenced.insert(ei);
                }
            }
            let mut groups: std::collections::BTreeMap<([u64; 3], [u64; 3]), Vec<u32>> =
                std::collections::BTreeMap::new();
            for (i, e) in edges.iter().enumerate() {
                if e.curve != Curve::LineSegment || !referenced.contains(&(i as u32)) {
                    continue;
                }
                let k0 = kb(verts[e.start as usize].point);
                let k1 = kb(verts[e.end as usize].point);
                let key = if k0 <= k1 { (k0, k1) } else { (k1, k0) };
                groups.entry(key).or_default().push(i as u32);
            }
            for copies in groups.values() {
                if !copies.iter().any(|c| edge_overrides.contains_key(c)) {
                    continue;
                }
                // Fan-out contract: EVERY copy of a targeted geometric edge
                // must carry the identical (bitwise, same-order) point list.
                let canon = copies[0];
                let canon_list = edge_overrides.get(&canon).ok_or_else(|| {
                    YangError::MalformedTopology(format!(
                        "edge override fan-out broken: geometric edge copy {canon} has no \
                         override list while a sibling copy does"
                    ))
                })?;
                for &ci in &copies[1..] {
                    let list = edge_overrides.get(&ci).ok_or_else(|| {
                        YangError::MalformedTopology(format!(
                            "edge override fan-out broken: geometric edge copy {ci} has no \
                             override list while copy {canon} does"
                        ))
                    })?;
                    let same = list.len() == canon_list.len()
                        && list
                            .iter()
                            .zip(canon_list.iter())
                            .all(|(a, b)| kb(*a) == kb(*b));
                    if !same {
                        return Err(YangError::MalformedTopology(format!(
                            "edge override fan-out broken: geometric edge copies {canon} and \
                             {ci} carry different override lists"
                        )));
                    }
                }
                // Validate + collect interior points against the canonical
                // copy's geometry (all copies share it by construction).
                let e0 = &edges[canon as usize];
                let p0 = verts[e0.start as usize].point;
                let p1 = verts[e0.end as usize].point;
                let (a0, a1) = (p0.as_array(), p1.as_array());
                let dir = [a1[0] - a0[0], a1[1] - a0[1], a1[2] - a0[2]];
                let chord2 = dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2];
                if chord2 == 0.0 {
                    return Err(YangError::MalformedTopology(format!(
                        "edge override targets degenerate zero-length edge {canon}"
                    )));
                }
                let dist = |p: [f64; 3], q: [f64; 3]| -> f64 {
                    ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt()
                };
                let mut seen: Vec<[u64; 3]> = Vec::new();
                let mut interior: Vec<(f64, Point3)> = Vec::new();
                for &pt in canon_list {
                    let key = kb(pt);
                    if key == kb(p0) || key == kb(p1) {
                        continue; // bit-identical endpoint repeat — dedup
                    }
                    if seen.contains(&key) {
                        continue; // bit-identical repeat — dedup
                    }
                    let pa = pt.as_array();
                    let scale = pa
                        .iter()
                        .chain(a0.iter())
                        .chain(a1.iter())
                        .fold(0.0f64, |m, &c| m.max(c.abs()));
                    let t = ((pa[0] - a0[0]) * dir[0]
                        + (pa[1] - a0[1]) * dir[1]
                        + (pa[2] - a0[2]) * dir[2])
                        / chord2;
                    let q = [a0[0] + t * dir[0], a0[1] + t * dir[1], a0[2] + t * dir[2]];
                    let band = 1e-9 * (1.0 + scale);
                    if !t.is_finite() || dist(pa, q) > band {
                        return Err(YangError::MalformedTopology(format!(
                            "edge {canon}: junction override ({},{},{}) is off the edge's line \
                             (deviation {})",
                            pa[0],
                            pa[1],
                            pa[2],
                            dist(pa, q)
                        )));
                    }
                    if t <= 0.0 || t >= 1.0 {
                        return Err(YangError::MalformedTopology(format!(
                            "edge {canon}: junction override at chord parameter {t} is outside \
                             the edge span"
                        )));
                    }
                    // Identity ceiling (the rim-override contract): only a
                    // bit-exact endpoint repeat may merge; a sub-TAU_MODEL
                    // near-endpoint graze that differs in bits is a CORNER
                    // junction (P3b), refused loudly (fail closed).
                    let margin = cad_primitives::TAU_MODEL * (1.0 + scale);
                    if dist(pa, a0) <= margin || dist(pa, a1) <= margin {
                        return Err(YangError::MalformedTopology(format!(
                            "edge {canon}: junction override at chord parameter {t} coincides \
                             with an endpoint but differs in bits (B-Rep vertex is \
                             authoritative; near-corner pierce is P3b territory — merge refused)"
                        )));
                    }
                    seen.push(key);
                    interior.push((t, pt));
                }
                if interior.is_empty() {
                    continue; // everything deduped — no chain, byte-identical
                }
                // Chord-parameter order; ULP twins that collide on the f64
                // parameter break the tie by the EXACT along-edge coordinate
                // order on the direction's dominant axis (never insertion
                // order — the loop_polyline zigzag lesson, #145). On-line
                // points order along the line exactly as their dominant-axis
                // coordinates do.
                let dom = (0..3)
                    .max_by(|&i, &j| dir[i].abs().total_cmp(&dir[j].abs()))
                    .unwrap_or(0);
                interior.sort_by(|x, y| match x.0.total_cmp(&y.0) {
                    std::cmp::Ordering::Equal => {
                        let (xc, yc) = (x.1.as_array()[dom], y.1.as_array()[dom]);
                        if dir[dom] >= 0.0 {
                            xc.total_cmp(&yc)
                        } else {
                            yc.total_cmp(&xc)
                        }
                    }
                    ord => ord,
                });
                // Mint ONCE, in canonical-copy direction.
                let mut steiner: Vec<u32> = Vec::with_capacity(interior.len());
                for &(t, pt) in &interior {
                    let vi = out_verts.len() as u32;
                    out_verts.push(pt);
                    sources.push(TessellationSource::BRepEdge { edge: canon, t });
                    steiner.push(vi);
                }
                // Per-copy oriented chains splicing the shared Steiner verts.
                for &ci in copies {
                    let e = &edges[ci as usize];
                    let fwd = kb(verts[e.start as usize].point) == kb(p0);
                    let mut chain: Vec<u32> = Vec::with_capacity(steiner.len() + 2);
                    chain.push(e.start);
                    if fwd {
                        chain.extend(steiner.iter().copied());
                    } else {
                        chain.extend(steiner.iter().rev().copied());
                    }
                    chain.push(e.end);
                    rim_rings.insert(ci, chain);
                    line_chain_edges.insert(ci);
                }
            }
            // Increment-1b scope guard: an overridden line edge incident to a
            // NON-PLANAR face would need that face's tessellator to splice
            // line chains too — only the planar CDT paths do. Fail closed
            // rather than silently dropping the insertion on one side (the
            // exact conformality break this pass exists to prevent).
            for (f_idx, f) in faces.iter().enumerate() {
                if matches!(f.surface, Surface::Plane { .. }) {
                    continue;
                }
                if let Some(&e_idx) = f
                    .outer_loop
                    .iter()
                    .chain(f.inner_loops.iter().flatten())
                    .find(|e| line_chain_edges.contains(e))
                {
                    return Err(YangError::MalformedTopology(format!(
                        "face {f_idx}: junction override on line edge {e_idx} incident to a \
                         non-planar face — increment-1b scope is planar-incident edges only \
                         (fail closed)"
                    )));
                }
            }
        }

        // ---- Face-interior junction pre-pass (P3a #146 increment 2, spec
        // §3.3 second bullet): mint each partner-side junction point as an
        // INTERIOR Steiner vertex of its pierced PLANAR face. Interiors are
        // CDT-freedom (findings Q2) — inserting one moves no boundary sample
        // — but the point must actually be CONSUMED by the face's CDT; the
        // dispatch below routes any face with interior points through
        // [`tessellate_planar_curved_cdt_face`]'s keep-interior arm, which
        // errors loudly if a point falls outside the bounded region.
        let mut face_interior: std::collections::BTreeMap<u32, Vec<u32>> =
            std::collections::BTreeMap::new();
        for (&f_idx, pts) in face_overrides {
            let Some(f) = faces.get(f_idx as usize) else {
                return Err(YangError::MalformedTopology(format!(
                    "face override targets out-of-range face {f_idx}"
                )));
            };
            let Surface::Plane { normal, d } = f.surface else {
                return Err(YangError::MalformedTopology(format!(
                    "face override targets non-planar face {f_idx} — increment-2 scope is \
                     planar partner faces only (fail closed)"
                )));
            };
            // Plane frame for the on-plane check + the `BRepFace` source
            // params (`eval_source` reproduces `O + u·e1 + v·e2`).
            let nu = normalize3(normal.as_array());
            let n_len = {
                let r = normal.as_array();
                (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt()
            };
            if n_len < cad_primitives::TAU_WORK {
                return Err(YangError::MalformedTopology(format!(
                    "face override targets face {f_idx} with a degenerate plane normal"
                )));
            }
            let dn = d / n_len;
            let (e1f, e2f) = ortho_basis(normal);
            let (e1a, e2a) = (e1f.as_array(), e2f.as_array());
            let o = [-dn * nu[0], -dn * nu[1], -dn * nu[2]];
            let mut seen: Vec<[u64; 3]> = Vec::new();
            let mut minted: Vec<u32> = Vec::new();
            for &pt in pts {
                let pa = pt.as_array();
                let key = [pa[0].to_bits(), pa[1].to_bits(), pa[2].to_bits()];
                if seen.contains(&key) {
                    continue; // bit-identical repeat — dedup
                }
                let scale = pa.iter().fold(0.0f64, |m, &c| m.max(c.abs()));
                let off = (pa[0] * nu[0] + pa[1] * nu[1] + pa[2] * nu[2] + dn).abs();
                if off > 1e-9 * (1.0 + scale) {
                    return Err(YangError::MalformedTopology(format!(
                        "face {f_idx}: interior junction override ({},{},{}) is off the \
                         face plane (deviation {off})",
                        pa[0], pa[1], pa[2]
                    )));
                }
                let rel = [pa[0] - o[0], pa[1] - o[1], pa[2] - o[2]];
                let u = rel[0] * e1a[0] + rel[1] * e1a[1] + rel[2] * e1a[2];
                let v = rel[0] * e2a[0] + rel[1] * e2a[1] + rel[2] * e2a[2];
                let vi = out_verts.len() as u32;
                out_verts.push(pt);
                sources.push(TessellationSource::BRepFace { face: f_idx, u, v });
                seen.push(key);
                minted.push(vi);
            }
            if !minted.is_empty() {
                face_interior.insert(f_idx, minted);
            }
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

                    // P3a #146 increment 1b: a loop edge carrying inserted
                    // junction samples must SPLICE its chain — the fan and
                    // the all-segment CDT read only edge endpoints, so route
                    // through the chain-splicing curved CDT (identical
                    // triangulation + orientation, plus [`loop_polyline`]
                    // chain expansion). Increment 2: likewise a face carrying
                    // INTERIOR junction points (the keep-interior CDT arm).
                    // Unreachable when both override maps are empty — the
                    // fan/CDT dispatch stays byte-identical.
                    let has_line_chain = !line_chain_edges.is_empty()
                        && f.outer_loop
                            .iter()
                            .chain(f.inner_loops.iter().flatten())
                            .any(|e| line_chain_edges.contains(e));
                    let interior: &[u32] = face_interior
                        .get(&(f_idx as u32))
                        .map(|v| v.as_slice())
                        .unwrap_or(&[]);

                    if has_line_chain || !interior.is_empty() {
                        tessellate_planar_curved_cdt_face(
                            f_idx,
                            f,
                            edges,
                            &rim_rings,
                            normal,
                            &out_verts,
                            interior,
                            &mut out_tris,
                        )?;
                    } else if needs_cdt {
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
                    // P3a #146 increment 2: a face carrying interior junction
                    // points routes through the keep-interior CDT (the cap
                    // fan cannot consume interior Steiner points). Empty for
                    // every face when `face_overrides` is empty.
                    let interior: &[u32] = face_interior
                        .get(&(f_idx as u32))
                        .map(|v| v.as_slice())
                        .unwrap_or(&[]);
                    if is_disk && interior.is_empty() {
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
                            interior,
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

mod loop_geometry;
pub(crate) use loop_geometry::*;

/// PR-KV6b-1: CDT tessellation of a planar face whose loops mix straight and
/// `Curve::Circle` edges (annular sectors, holed circle caps, …). The
/// boundary polylines splice the SHARED per-edge sample chains
/// ([`loop_polyline`]), so faces meeting along an arc emit identical sample
/// vertices — the watertightness mechanism. Triangulation + orientation are
/// exactly the all-segment CDT path's (no Steiner points, no boundary
/// subdivision).
///
/// P3a #146 increment 2: `interior` lists pre-minted mesh vertices (partner-
/// side junction points) to keep as INTERIOR CDT vertices. Empty = the
/// flood-fill variant, byte-identical to the pre-increment path; non-empty
/// routes through [`cherchi_rs::triangulation::cdt_polygon_with_holes_keep_interior`]
/// and errors loudly if any interior point is NOT consumed by the emitted
/// triangles (a point outside the bounded region silently dropping is the
/// one-sided-mint conformality break this machinery exists to prevent).
#[allow(clippy::too_many_arguments)]
pub(crate) fn tessellate_planar_curved_cdt_face(
    f_idx: usize,
    f: &BRepFace,
    edges: &[BRepEdge],
    chains: &std::collections::BTreeMap<u32, Vec<u32>>,
    normal: Vector3,
    out_verts: &[Point3],
    interior: &[u32],
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
    //
    // P3a #146 increment 2: with interior junction points the keep-interior
    // variant triangulates them against the boundary constraints (no silent
    // Steiner splits, spade constraint-crossing refused loudly); the loud
    // consumed-postcondition below replaces flood-fill's topological
    // classification guarantee for these faces.
    let local_tris = if interior.is_empty() {
        cherchi_rs::triangulation::cdt_polygon_with_holes_floodfill(
            &local_verts,
            &outer_local,
            &holes_local,
        )
        .map_err(|e| {
            YangError::MalformedTopology(format!("face {f_idx}: CDT triangulation failed: {e}"))
        })?
    } else {
        let interior_local: Vec<u32> = interior
            .iter()
            .map(|&g| intern(g, &mut local_verts, &mut global_of_local))
            .collect();
        if cdt_probe {
            for (k, &li) in interior_local.iter().enumerate() {
                let p = &local_verts[li as usize];
                eprintln!(
                    "[cdt-probe] interior {k} -> v {li} = ({:?}, {:?}) bits=({:#x},{:#x})",
                    p.x(),
                    p.y(),
                    p.x().to_bits(),
                    p.y().to_bits()
                );
            }
        }
        let tris = cherchi_rs::triangulation::cdt_polygon_with_holes_keep_interior(
            &local_verts,
            &outer_local,
            &holes_local,
            &interior_local,
        )
        .map_err(|e| {
            YangError::MalformedTopology(format!(
                "face {f_idx}: keep-interior CDT triangulation failed: {e}"
            ))
        })?;
        // Consumed postcondition: every interior junction point must appear
        // in the emitted triangles — a point outside the bounded region
        // would silently drop and leave the junction minted on ONE side
        // only (the exact conformality break this pass exists to prevent).
        for (k, &li) in interior_local.iter().enumerate() {
            if !tris.iter().any(|t| t.contains(&li)) {
                let g = interior[k];
                let p = out_verts[g as usize].as_array();
                return Err(YangError::MalformedTopology(format!(
                    "face {f_idx}: interior junction point ({},{},{}) was not consumed by \
                     the face CDT (outside the bounded region — one-sided mint refused)",
                    p[0], p[1], p[2]
                )));
            }
        }
        tris
    };
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

    // #179 (spec `yang_stage1_cdt_parity_flap.md`): interior classification
    // must be the topological hull flood-fill, NOT f64 centroid parity — on
    // a near-collinear boundary triple the parity test coin-flips a
    // hair-sliver and can KEEP an exterior zero-area flap triangle (F0084's
    // octagon cap), emitting a non-2-manifold Stage-1 mesh. Same F0047
    // migration the curved-CDT path and kernel-v2 render already made.
    let local_tris = cherchi_rs::triangulation::cdt_polygon_with_holes_floodfill(
        &local_verts,
        &outer_local,
        &holes_local,
    )
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
    // Task #145 diagnosis probe (read-only, env-gated): per-vertex edge
    // attribution + loop edge kinds, to localize a chain zigzag to a stored
    // edge sequence vs a rebuilt arc chain.
    if std::env::var_os("YANG_T145_PROBE").is_some() {
        if let Ok(attr) = loop_polyline_attributed(f_idx, &f.outer_loop, edges, chains) {
            eprintln!("[t145] face {f_idx} outer attr (vert,edge): {attr:?}");
        }
        let kinds: Vec<(u32, String, u32, u32)> = f
            .outer_loop
            .iter()
            .map(|&e| {
                let ed = &edges[e as usize];
                (e, format!("{:?}", ed.curve), ed.start, ed.end)
            })
            .collect();
        eprintln!("[t145] face {f_idx} loop edges: {kinds:?}");
    }
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
    if cfg!(debug_assertions) && std::env::var_os("YANG_SHIFT_NEUTER").is_some() {
        shift = 0; // dev-only neuter, gated out of release (F12)
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

mod surface_face_tessellators;
pub(crate) use surface_face_tessellators::*;

mod patch_tessellators;
pub(crate) use patch_tessellators::*;
pub use patch_tessellators::{tessellate_sphere_patch, tessellate_torus_patch};

#[cfg(test)]
mod torus_patch_tests;

mod normals_chord_bounds;
pub(crate) use normals_chord_bounds::*;
