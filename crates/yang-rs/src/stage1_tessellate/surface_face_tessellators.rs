//! Stage-1 closed-surface face tessellators: solid-sphere lat/long grid and
//! the cone face + cone frustum band (theta grids with pole/apex closure).
//! Extracted move-only from stage1_tessellate.rs (#159 F9 decomposition).

#[allow(clippy::wildcard_imports)]
use crate::*;

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
