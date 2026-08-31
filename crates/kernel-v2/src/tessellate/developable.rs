//! Developable-patch tessellation — the single shared unroll engine
//! (`tessellate_developable_patch`) plus the cylinder/cone patch dispatchers
//! (crate hard rule 5: ONE engine, per-surface parameterization). Move-only
//! split from the tessellate god-module (design review 2026-07-12 F9).

use super::*;

/// Which developable surface the shared patch engine is developing (KV6c
/// increment 5, spec `kv6c_partial_revolve_cone_patch.md` §8 / I5 — ONE
/// engine, per-surface parameterization; crate hard rule 5). The engine's
/// (u, v) chart is `u = sense·θ·r_unroll`, `v` = the axial coordinate from
/// the anchor (`axis_point` height for cylinders, τ from the apex for
/// cones); only the on-surface radius at `v` differs.
enum DevSurface {
    Cylinder { radius: f64 },
    Cone { tan_half_angle: f64 },
}

/// The ISOMETRIC development of a developable patch chart: the plane in
/// which Euclidean distance equals surface (geodesic) distance.
///
/// The engine's working chart is `(u, v) = (sense·θ·r_unroll, axial)`. For a
/// CYLINDER that chart already IS the isometric development. For a CONE it
/// is not — there `|∂P/∂u| = v·tanα/r_unroll`, which both differs from 1 and
/// VARIES with `v`, while `|∂P/∂v| = 1/cos α`. The cone's isometric
/// development is the polar unroll about the apex: slant radius
/// `ρ = v/cos α`, developed angle `φ = θ·sin α`.
///
/// This matters because Rivara longest-edge bisection's non-degeneracy
/// guarantee (finitely many similarity classes ⇒ angles bounded below by the
/// initial mesh's) is a statement about ONE Euclidean structure — the one
/// "longest edge" and "midpoint" are both taken in. Run in the working chart
/// on a cone, it bounds chart angles while surface angles degrade freely
/// (KV9-F2: R0017 face 17 went from a worst 3D aspect of 204 to 947 and
/// folded, while its same-development control face 14 held at 109.80).
///
/// Both operations work in a frame ROTATED so that `a` sits on the +x axis,
/// i.e. from the RELATIVE angle Δφ only. No `atan2` is ever taken of an
/// absolute developed angle, so a patch whose u-window is unwrapped across
/// the seam (`|φ| > π`) stays well-defined.
#[derive(Clone, Copy, Debug)]
struct IsoDev {
    /// Developed angle per unit chart `u` (`sin α / r_unroll`).
    dphi_du: f64,
    /// Chart `v` per unit slant radius (`cos α`).
    cos_a: f64,
    /// False for a cylinder (chart already isometric) and for a cone whose
    /// developed angle is degenerate (α → 0); then both operations are the
    /// plain chart ones.
    active: bool,
}

impl IsoDev {
    fn new(dev: &DevSurface, r_unroll: f64) -> Self {
        match *dev {
            DevSurface::Cylinder { .. } => Self {
                dphi_du: 0.0,
                cos_a: 1.0,
                active: false,
            },
            DevSurface::Cone { tan_half_angle } => {
                let cos_a = 1.0 / (1.0 + tan_half_angle * tan_half_angle).sqrt();
                let dphi_du = tan_half_angle * cos_a / r_unroll;
                Self {
                    dphi_du,
                    cos_a,
                    active: dphi_du.is_finite() && dphi_du > 0.0 && cos_a > 0.0,
                }
            }
        }
    }

    /// The two developed slant radii, or `None` when this chart has no usable
    /// polar development for these points — a cylinder, a degenerate cone, or
    /// a point at/behind the apex (`ρ ≤ 0`, the other nappe). Callers then
    /// fall back to the plain chart, which is what they had before.
    fn radii(&self, a: Point2, b: Point2) -> Option<(f64, f64)> {
        if !self.active {
            return None;
        }
        let (ra, rb) = (a.y() / self.cos_a, b.y() / self.cos_a);
        (ra > 0.0 && rb > 0.0 && ra.is_finite() && rb.is_finite()).then_some((ra, rb))
    }

    /// Squared isometric (geodesic) distance between two chart points.
    fn dist2(&self, a: Point2, b: Point2) -> f64 {
        let Some((ra, rb)) = self.radii(a, b) else {
            let (dx, dy) = (a.x() - b.x(), a.y() - b.y());
            return dx * dx + dy * dy;
        };
        let dphi = (b.x() - a.x()) * self.dphi_du;
        // Law of cosines in the developed sector.
        (ra * ra + rb * rb - 2.0 * ra * rb * dphi.cos()).max(0.0)
    }
}

pub(crate) fn tessellate_cylinder_patch(
    arena: &BrepArena,
    fid: FaceId,
    n_seg: u32,
    out: &mut RenderMesh,
) -> Result<(), KernelV2Error> {
    let face = arena.face(fid)?;
    let Some(Surface::Cylinder {
        axis_point,
        axis_dir,
        radius,
        reversed,
    }) = face.surface
    else {
        return Err(KernelV2Error::FaceWithoutSurface { face: fid });
    };
    tessellate_developable_patch(
        arena,
        fid,
        n_seg,
        out,
        [axis_point.x(), axis_point.y(), axis_point.z()],
        [axis_dir.x, axis_dir.y, axis_dir.z],
        reversed,
        radius,
        DevSurface::Cylinder { radius },
    )
}

/// KV6c increment 5: the arc-bounded partial cone patch (the partial-revolve
/// oblique wall; boolean-output cone patches ride the same engine). The
/// unroll scale `r_unroll` is the face's maximum boundary radial distance —
/// deterministic and positive; the (θ, τ) chart is bijective on the single
/// nappe regardless of the u-scale, and CDT correctness needs bijectivity,
/// not isometry.
pub(crate) fn tessellate_cone_patch(
    arena: &BrepArena,
    fid: FaceId,
    n_seg: u32,
    out: &mut RenderMesh,
) -> Result<(), KernelV2Error> {
    let face = arena.face(fid)?;
    let Some(Surface::Cone {
        apex,
        axis_dir,
        half_angle,
        reversed,
    }) = face.surface
    else {
        return Err(KernelV2Error::FaceWithoutSurface { face: fid });
    };
    let ap = [apex.x(), apex.y(), apex.z()];
    let a = [axis_dir.x, axis_dir.y, axis_dir.z];
    let mut r_max = 0.0f64;
    let mut loops = vec![face.outer_loop];
    loops.extend(face.inner_loops.iter().copied());
    for lid in loops {
        for p in arena.loop_points(lid)? {
            let d = [p.x() - ap[0], p.y() - ap[1], p.z() - ap[2]];
            let t = d[0] * a[0] + d[1] * a[1] + d[2] * a[2];
            let r = [d[0] - t * a[0], d[1] - t * a[1], d[2] - t * a[2]];
            r_max = r_max.max((r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt());
        }
    }
    if !(r_max.is_finite() && r_max > 0.0) {
        return Err(KernelV2Error::TessellationFailed {
            face: fid,
            reason: "cone patch has no off-axis boundary vertex",
        });
    }
    tessellate_developable_patch(
        arena,
        fid,
        n_seg,
        out,
        ap,
        a,
        reversed,
        r_max,
        DevSurface::Cone {
            tan_half_angle: half_angle.tan(),
        },
    )
}

/// The shared developable-patch engine: unroll the boundary loops into the
/// surface's (u, v) development, CDT, chord-bound refinement, emit. See the
/// wrappers above for the per-surface charts. Body unchanged from the
/// original cylinder implementation except for the `DevSurface` chart
/// parameterization (spec `kv6c_partial_revolve_cone_patch.md` §8).
#[allow(clippy::too_many_arguments)]
fn tessellate_developable_patch(
    arena: &BrepArena,
    fid: FaceId,
    n_seg: u32,
    out: &mut RenderMesh,
    ap: [f64; 3],
    a: [f64; 3],
    reversed: bool,
    r_unroll: f64,
    dev: DevSurface,
) -> Result<(), KernelV2Error> {
    use std::f64::consts::PI;
    let fail = |reason: &'static str| KernelV2Error::TessellationFailed { face: fid, reason };

    let face = arena.face(fid)?;
    let sense = if reversed { -1.0 } else { 1.0 };
    // The outward-normal tilt: 0 for cylinders (pure radial), tan α for
    // cones (`n̂ ∝ r̂ − tan α·â`, ⊥ the generator).
    let tan_a = match dev {
        DevSurface::Cylinder { .. } => 0.0,
        DevSurface::Cone { tan_half_angle } => tan_half_angle,
    };
    let w_facet = 2.0 * PI * r_unroll / f64::from(n_seg);
    // §4.3.4 inc-0 census (spec `yang_434_output_chord_refinement.md` §3,
    // env-gated `KV2_CHORD_DEPTH_CENSUS`, print-only): per face, the depth of
    // the boundary `LineSegment` chords below this developable surface
    // (`max_chord_sag`, measured at each chord's midpoint against the ideal
    // development) and — after refinement — the materialized split deviation
    // (`max_split_dev`), the thinnest emitted 2D triangle (`min_h2d`), and
    // whether the KV9-F2 fold tripwire fired. Decides the refinement band
    // target and blast radius for the §4.3.4 output-polyline refinement.
    let census_on = std::env::var_os("KV2_CHORD_DEPTH_CENSUS").is_some();
    let mut census_n_chord = 0usize;
    let mut census_max_sag = 0.0f64;

    let mut all_loops = vec![face.outer_loop];
    all_loops.extend(face.inner_loops.iter().copied());

    // ---- shared angular frame (anchored at the first outer vertex) -------
    let theta_h = |p: Point3, e1: [f64; 3], e2: [f64; 3]| -> Result<(f64, f64), KernelV2Error> {
        let d = [p.x() - ap[0], p.y() - ap[1], p.z() - ap[2]];
        let h = d[0] * a[0] + d[1] * a[1] + d[2] * a[2];
        let r = [d[0] - h * a[0], d[1] - h * a[1], d[2] - h * a[2]];
        let rl = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt();
        if !(rl.is_finite() && rl > 0.0) {
            return Err(fail("patch vertex lies on the cylinder axis"));
        }
        let x = r[0] * e1[0] + r[1] * e1[1] + r[2] * e1[2];
        let y = r[0] * e2[0] + r[1] * e2[1] + r[2] * e2[2];
        Ok((y.atan2(x), h))
    };
    let outer_hes = arena.loop_half_edges(face.outer_loop)?;
    if outer_hes.is_empty() {
        return Err(fail("patch with an empty boundary loop"));
    }
    let p0 = arena.vertex(arena.half_edge(outer_hes[0])?.origin)?.point;
    let d0 = [p0.x() - ap[0], p0.y() - ap[1], p0.z() - ap[2]];
    let h00 = d0[0] * a[0] + d0[1] * a[1] + d0[2] * a[2];
    let r0 = [d0[0] - h00 * a[0], d0[1] - h00 * a[1], d0[2] - h00 * a[2]];
    let r0l = (r0[0] * r0[0] + r0[1] * r0[1] + r0[2] * r0[2]).sqrt();
    if !(r0l.is_finite() && r0l > 0.0) {
        return Err(fail("patch anchor vertex lies on the cylinder axis"));
    }
    let e1 = [r0[0] / r0l, r0[1] / r0l, r0[2] / r0l];
    let e2 = [
        a[1] * e1[2] - a[2] * e1[1],
        a[2] * e1[0] - a[0] * e1[2],
        a[0] * e1[1] - a[1] * e1[0],
    ];
    // (u, v) ← 3D, and 3D ← (u, v) for on-surface points. The on-surface
    // radius at v is the chart's only per-surface difference.
    let unroll_u = |theta: f64| sense * theta * r_unroll;
    let surface_point = |u: f64, v: f64| -> [f64; 3] {
        let theta = sense * u / r_unroll;
        let (s, c) = theta.sin_cos();
        let rr = match dev {
            DevSurface::Cylinder { radius } => radius,
            DevSurface::Cone { tan_half_angle } => v * tan_half_angle,
        };
        [
            ap[0] + v * a[0] + rr * (c * e1[0] + s * e2[0]),
            ap[1] + v * a[1] + rr * (c * e1[1] + s * e2[1]),
            ap[2] + v * a[2] + rr * (c * e1[2] + s * e2[2]),
        ]
    };

    let outward_at = |pos: [f64; 3]| -> Option<[f64; 3]> {
        let d = [pos[0] - ap[0], pos[1] - ap[1], pos[2] - ap[2]];
        let h = d[0] * a[0] + d[1] * a[1] + d[2] * a[2];
        let r = [d[0] - h * a[0], d[1] - h * a[1], d[2] - h * a[2]];
        let rl = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt();
        if !(rl.is_finite() && rl > 0.0) {
            return None;
        }
        let raw = [
            r[0] / rl - tan_a * a[0],
            r[1] / rl - tan_a * a[1],
            r[2] / rl - tan_a * a[2],
        ];
        let m = (raw[0] * raw[0] + raw[1] * raw[1] + raw[2] * raw[2]).sqrt();
        Some([sense * raw[0] / m, sense * raw[1] / m, sense * raw[2] / m])
    };
    let iso = IsoDev::new(&dev, r_unroll);

    // TEMP diagnostic (uncommitted): boundary feature-size survey.
    if std::env::var_os("KV2_PATCH_MINLEN_PROBE").is_some() {
        let mut min_edge = f64::INFINITY;
        let mut min_pair = f64::INFINITY;
        let mut all_pts: Vec<[f64; 3]> = Vec::new();
        for &lid in &all_loops {
            if let Ok(hes) = arena.loop_half_edges(lid) {
                for &h in &hes {
                    if let Ok(he) = arena.half_edge(h) {
                        if let (Ok(p), Ok(nx)) = (arena.vertex(he.origin), arena.half_edge(he.next))
                        {
                            if let Ok(q) = arena.vertex(nx.origin) {
                                let d = [
                                    q.point.x() - p.point.x(),
                                    q.point.y() - p.point.y(),
                                    q.point.z() - p.point.z(),
                                ];
                                let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
                                min_edge = min_edge.min(l);
                                all_pts.push([p.point.x(), p.point.y(), p.point.z()]);
                            }
                        }
                    }
                }
            }
        }
        for i in 0..all_pts.len() {
            for j in (i + 1)..all_pts.len() {
                let d = [
                    all_pts[i][0] - all_pts[j][0],
                    all_pts[i][1] - all_pts[j][1],
                    all_pts[i][2] - all_pts[j][2],
                ];
                let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
                if l < min_pair {
                    min_pair = l;
                }
            }
        }
        eprintln!(
            "[minlen-probe] face={fid:?} boundary_verts={} min_edge={min_edge:.3e} \
             min_pair={min_pair:.3e}",
            all_pts.len()
        );
    }

    // ---- pass 1: per-loop unrolled chains ---------------------------------
    struct Chain {
        /// (node index, kind of the edge to the NEXT chain entry, cyclic).
        entries: Vec<(usize, PatchEdgeKind)>,
        wrap: i64,
    }
    let mut nodes: Vec<PatchNode> = Vec::new();
    let mut chains: Vec<Chain> = Vec::new();
    // Dev-only ring provenance (env-gated, print-only): which half-edge minted
    // each ORIGIN node. Arc-sample nodes get no entry and print as "sample".
    // Companion to the planar `KV2_RING_PROVENANCE` probe in `sampled_loop_points`.
    // The map is only POPULATED under the env gate, so the production path keeps
    // its allocation profile unchanged.
    let prov_on = std::env::var_os("KV2_RING_PROVENANCE").is_some();
    let mut node_prov: std::collections::HashMap<
        usize,
        (crate::arena::HalfEdgeId, crate::arena::HalfEdgeId),
    > = std::collections::HashMap::new();
    for &lid in &all_loops {
        let hes = arena.loop_half_edges(lid)?;
        if hes.len() < 3 {
            return Err(fail("patch loop with fewer than 3 edges"));
        }
        let mut entries: Vec<(usize, PatchEdgeKind)> = Vec::new();
        let mut u_cur = f64::NAN;
        let mut total_theta = 0.0f64;
        for (i, &h) in hes.iter().enumerate() {
            let he = arena.half_edge(h)?;
            let p = arena.vertex(he.origin)?.point;
            let q = arena.vertex(arena.half_edge(he.next)?.origin)?.point;
            let (theta_p, hp) = theta_h(p, e1, e2)?;
            if i == 0 {
                u_cur = unroll_u(theta_p);
            }
            // Dev-only chain probe: per-half-edge curve kind + endpoints for
            // one face id — names WHICH boundary curve minted a fold's deep
            // ArcSample layer (Arc gets conforming inserts; the conic arms
            // do not).
            if std::env::var("KV2_PATCH_CHAIN_PROBE").is_ok_and(|v| v == format!("{}", fid.0)) {
                let kind = match &he.curve {
                    Curve::LineSegment => "LineSegment",
                    Curve::Arc { .. } => "Arc",
                    Curve::Circle { .. } => "Circle",
                    Curve::EllipseArc { .. } => "EllipseArc",
                    Curve::HyperbolaArc { .. } => "HyperbolaArc",
                    Curve::SurfacePair { .. } => "SurfacePair",
                };
                eprintln!(
                    "[chain-probe] face={} loop={lid:?} i={i} h={h:?} kind={kind} n0={} \
                     p=({:.9e},{:.9e},{:.9e}) q=({:.9e},{:.9e},{:.9e})",
                    fid.0,
                    nodes.len(),
                    p.x(),
                    p.y(),
                    p.z(),
                    q.x(),
                    q.y(),
                    q.z()
                );
                if let Curve::SurfacePair { a, b } = &he.curve {
                    eprintln!("[chain-probe]   spair a={a:?}");
                    eprintln!("[chain-probe]   spair b={b:?}");
                }
                if let Curve::HyperbolaArc { .. } = &he.curve {
                    eprintln!("[chain-probe]   hyp {:?}", he.curve);
                }
                if let Curve::Arc {
                    center,
                    normal,
                    radius,
                } = &he.curve
                {
                    eprintln!(
                        "[chain-probe]   arc center=({:.9e},{:.9e},{:.9e}) \
                         normal=({:.9e},{:.9e},{:.9e}) radius={radius:.9e} \
                         face_axis=({:.9e},{:.9e},{:.9e}) face_apex=({:.9e},{:.9e},{:.9e})",
                        center.x(),
                        center.y(),
                        center.z(),
                        normal.x,
                        normal.y,
                        normal.z,
                        a[0],
                        a[1],
                        a[2],
                        ap[0],
                        ap[1],
                        ap[2]
                    );
                }
            }
            let origin_node = nodes.len();
            nodes.push(PatchNode {
                p2: Point2::new(u_cur, hp),
                pos: [p.x(), p.y(), p.z()],
            });
            if prov_on {
                node_prov.insert(origin_node, (h, he.twin));
            }
            match he.curve {
                Curve::LineSegment => {
                    let (theta_q, hq) = theta_h(q, e1, e2)?;
                    let delta = crate::geom::wrap_to_pi(theta_q - theta_p);
                    entries.push((origin_node, PatchEdgeKind::Chord));
                    if census_on {
                        // Chord depth at the midpoint: 3D lerp vs the ideal
                        // development at the averaged work coords — exactly
                        // the deviation a future Chord split there would keep.
                        let u_mid = u_cur + sense * delta * r_unroll / 2.0;
                        let h_mid = (hp + hq) / 2.0;
                        let ideal = surface_point(u_mid, h_mid);
                        let mid = [
                            (p.x() + q.x()) / 2.0,
                            (p.y() + q.y()) / 2.0,
                            (p.z() + q.z()) / 2.0,
                        ];
                        let sag = ((mid[0] - ideal[0]).powi(2)
                            + (mid[1] - ideal[1]).powi(2)
                            + (mid[2] - ideal[2]).powi(2))
                        .sqrt();
                        census_n_chord += 1;
                        census_max_sag = census_max_sag.max(sag);
                    }
                    u_cur += sense * delta * r_unroll;
                    total_theta += delta;
                }
                Curve::Arc {
                    center,
                    normal,
                    radius: _,
                } => {
                    let n_arr = [normal.x, normal.y, normal.z];
                    let Some(sweep) = crate::geom::ccw_sweep(center, n_arr, p, q) else {
                        return Err(fail("degenerate patch arc (endpoint not radial)"));
                    };
                    let dir = if normal.x * a[0] + normal.y * a[1] + normal.z * a[2] > 0.0 {
                        1.0
                    } else {
                        -1.0
                    };
                    let samples = arc_interior_samples_frac(arena, h, n_seg)?;
                    entries.push((origin_node, PatchEdgeKind::ArcSample));
                    for (frac, sp) in &samples {
                        let su = u_cur + sense * dir * sweep * frac * r_unroll;
                        let (_, sh) = theta_h(*sp, e1, e2)?;
                        entries.push((nodes.len(), PatchEdgeKind::ArcSample));
                        nodes.push(PatchNode {
                            p2: Point2::new(su, sh),
                            pos: [sp.x(), sp.y(), sp.z()],
                        });
                    }
                    u_cur += sense * dir * sweep * r_unroll;
                    total_theta += dir * sweep;
                }
                Curve::EllipseArc { .. } => {
                    // Oblique-section arc: per-sample wrapped-Δθ walk (the
                    // SurfacePair/HyperbolaArc mechanism) — each sample's
                    // azimuth is derived from its POSITION, never from its
                    // index. A uniform-fraction shortcut existed here for
                    // cylinders (Δθ = s_w·Δt with uniform-in-parameter
                    // samples); inc-8's sag-bound ellipse sampling is
                    // NON-uniform in parameter, so the shortcut's premise
                    // is gone and it scrambled the chart (fold tripwire,
                    // `ellipse_bounded_tunnel_reentry`). The walk is
                    // kind-agnostic and was already the cone-section path
                    // (KV16). Samples are grid-step dense, far below the
                    // wrap_to_pi ambiguity at π.
                    let samples = ellipse_interior_samples(arena, h, n_seg)?;
                    let mut theta_prev = theta_p;
                    entries.push((origin_node, PatchEdgeKind::ArcSample));
                    for sp in &samples {
                        let (theta_s, sh) = theta_h(*sp, e1, e2)?;
                        let delta = crate::geom::wrap_to_pi(theta_s - theta_prev);
                        u_cur += sense * delta * r_unroll;
                        total_theta += delta;
                        theta_prev = theta_s;
                        entries.push((nodes.len(), PatchEdgeKind::ArcSample));
                        nodes.push(PatchNode {
                            p2: Point2::new(u_cur, sh),
                            pos: [sp.x(), sp.y(), sp.z()],
                        });
                    }
                    let (theta_q, _) = theta_h(q, e1, e2)?;
                    let delta = crate::geom::wrap_to_pi(theta_q - theta_prev);
                    u_cur += sense * delta * r_unroll;
                    total_theta += delta;
                }
                Curve::Circle { .. } => {
                    return Err(fail("full-circle edge inside a partial cylinder patch"))
                }
                // KV16: hyperbola-arc boundary piece (the axis-steep
                // plane∩cone section). Same mechanism as the SurfacePair
                // arm below: each closed-form sample advances the unroll
                // azimuth by its own small wrapped Δθ (samples are sag-bound
                // dense, every step far below π; the hyperbola's azimuth
                // advance is non-uniform in its parameter, so the
                // uniform-fraction shortcut of the circle/ellipse arms does
                // not apply).
                Curve::HyperbolaArc { .. } => {
                    let samples = hyperbola_interior_samples(arena, h, n_seg)?;
                    let mut theta_prev = theta_p;
                    entries.push((origin_node, PatchEdgeKind::ArcSample));
                    for sp in &samples {
                        let (theta_s, sh) = theta_h(*sp, e1, e2)?;
                        let delta = crate::geom::wrap_to_pi(theta_s - theta_prev);
                        u_cur += sense * delta * r_unroll;
                        total_theta += delta;
                        theta_prev = theta_s;
                        entries.push((nodes.len(), PatchEdgeKind::ArcSample));
                        nodes.push(PatchNode {
                            p2: Point2::new(u_cur, sh),
                            pos: [sp.x(), sp.y(), sp.z()],
                        });
                    }
                    let (theta_q, _) = theta_h(q, e1, e2)?;
                    let delta = crate::geom::wrap_to_pi(theta_q - theta_prev);
                    u_cur += sense * delta * r_unroll;
                    total_theta += delta;
                }
                // M5: quartic boundary piece. Each certified sample advances
                // the unroll azimuth by its own small wrapped Δθ (samples
                // are chord-bound dense — every step is far below π, so
                // `wrap_to_pi` is unambiguous; the uniform-fraction shortcut
                // of the arc arms does not apply because the quartic's
                // azimuth advance is non-uniform in arc length).
                Curve::SurfacePair { .. } => {
                    let samples = surface_pair_edge_samples(arena, h, n_seg)?;
                    let mut theta_prev = theta_p;
                    entries.push((origin_node, PatchEdgeKind::ArcSample));
                    for sp in &samples {
                        let (theta_s, sh) = theta_h(*sp, e1, e2)?;
                        let delta = crate::geom::wrap_to_pi(theta_s - theta_prev);
                        u_cur += sense * delta * r_unroll;
                        total_theta += delta;
                        theta_prev = theta_s;
                        entries.push((nodes.len(), PatchEdgeKind::ArcSample));
                        nodes.push(PatchNode {
                            p2: Point2::new(u_cur, sh),
                            pos: [sp.x(), sp.y(), sp.z()],
                        });
                    }
                    let (theta_q, _) = theta_h(q, e1, e2)?;
                    let delta = crate::geom::wrap_to_pi(theta_q - theta_prev);
                    u_cur += sense * delta * r_unroll;
                    total_theta += delta;
                }
            }
        }
        let wraps_f = total_theta / (2.0 * PI);
        let wraps = wraps_f.round();
        if (wraps_f - wraps).abs() > 1e-3 || wraps.abs() > 1.0 {
            return Err(fail("patch loop's net axis winding is not a valid integer"));
        }
        // Mirror the wrap into the (sense-applied) unrolled frame.
        if std::env::var_os("KV2_PATCH_PASS_PROBE").is_some() {
            let us: Vec<f64> = entries.iter().map(|(n, _)| nodes[*n].p2.x()).collect();
            let (umin, umax) = us
                .iter()
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), &u| {
                    (lo.min(u), hi.max(u))
                });
            eprintln!(
                "[pass-probe] face={fid:?} loop: entries={} total_theta={total_theta:.6} \
                 wraps={wraps} u_extent=[{umin:.6},{umax:.6}] w_facet={w_facet:.6}",
                entries.len()
            );
        }
        chains.push(Chain {
            entries,
            wrap: (sense * wraps) as i64,
        });
    }

    // ---- pass 2: assemble one simple polygon + holes ----------------------
    let span = 2.0 * PI * r_unroll; // |u| span of one full wrap (sense-free)

    // ---- pass 1.5: canonicalize shared boundary vertices across chains ----
    // (#188 inc-4, spec `yang_188_f0082_j3_envelope_selection.md` §10.6.)
    // A B-Rep vertex visited by TWO loops of this face — a pinched ring,
    // e.g. a notch hole touching the outer at one vertex — is minted once
    // per loop walk, and each walk accumulates its unroll azimuth `u`
    // independently: the copies land a few ulps apart (accumulation
    // rounding), or a full span apart (the later walk's atan2 anchor picks
    // the (−π, π] branch while the earlier walk accumulated past the cut).
    // The flood-fill CDT admits a vertex-touching hole ONLY by bitwise
    // coincidence (spec `kv2_cdt_triangulation_core` §6b M3b weld), so all
    // copies of one vertex must present ONE bit pattern. Identity is exact
    // — same 3D position bits — never a distance band (P9).
    //
    // Per chain: the FIRST entry matching an earlier chain's vertex fixes
    // the chain's window offset k = round(Δu/span); the whole chain is
    // translated RIGIDLY by −k·span (a pinned hole is congruent to its
    // host's frame), every matched entry is re-pointed to the earlier node
    // (bit-equal by construction), and any second match must agree on k —
    // a pinch spanning inconsistent seam windows is out of scope, loudly.
    // Wrap chains are never translated (their absolute window anchors the
    // seam cut): a wrap chain matching at k ≠ 0 is equally out of scope.
    {
        let mut canon_of: std::collections::BTreeMap<(u64, u64, u64), usize> =
            std::collections::BTreeMap::new();
        for c in &mut chains {
            let mut anchored = false;
            for i in 0..c.entries.len() {
                let e = c.entries[i].0;
                let p = nodes[e].pos;
                let key = (p[0].to_bits(), p[1].to_bits(), p[2].to_bits());
                let Some(&n0) = canon_of.get(&key) else {
                    canon_of.insert(key, e);
                    continue;
                };
                if n0 == e {
                    continue;
                }
                let k = ((nodes[e].p2.x() - nodes[n0].p2.x()) / span).round();
                if k != 0.0 {
                    if anchored || c.wrap != 0 {
                        return Err(fail("pinched loop spans inconsistent seam windows"));
                    }
                    // Rigid translate into the host's window; the matched
                    // entry is then re-seated exactly by the re-point below.
                    for j in 0..c.entries.len() {
                        let nj = c.entries[j].0;
                        let pj = nodes[nj].p2;
                        nodes[nj].p2 = Point2::new(pj.x() - k * span, pj.y());
                    }
                }
                anchored = true;
                c.entries[i].0 = n0;
            }
        }
    }
    // Nodes referenced by MORE than one chain (possible only through the
    // merge above). A chain sharing a node with another is frame-LOCKED to
    // it: shifting one by k·span would tear the shared vertex, so the
    // mid-window shifts below skip pinned chains (a pinned hole is already
    // in its host's window through the shared node).
    let shared_nodes: std::collections::BTreeSet<usize> = {
        let mut seen = std::collections::BTreeSet::new();
        let mut shared = std::collections::BTreeSet::new();
        for c in &chains {
            for n in c
                .entries
                .iter()
                .map(|&(n, _)| n)
                .collect::<std::collections::BTreeSet<usize>>()
            {
                if !seen.insert(n) {
                    shared.insert(n);
                }
            }
        }
        shared
    };
    let is_pinned =
        |c: &Chain| -> bool { c.entries.iter().any(|&(n, _)| shared_nodes.contains(&n)) };

    fn mid_u(c: &Chain, nodes: &[PatchNode]) -> f64 {
        let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
        for &(n, _) in &c.entries {
            let u = nodes[n].p2.x();
            lo = lo.min(u);
            hi = hi.max(u);
        }
        (lo + hi) / 2.0
    }

    let wrapping: Vec<usize> = (0..chains.len()).filter(|&i| chains[i].wrap != 0).collect();
    // Boundary-edge registry for refinement (node-index pairs, unordered).
    let mut boundary: std::collections::BTreeMap<(usize, usize), PatchEdgeKind> =
        std::collections::BTreeMap::new();
    let register_chain =
        |c: &Chain, boundary: &mut std::collections::BTreeMap<(usize, usize), PatchEdgeKind>| {
            let m = c.entries.len();
            for i in 0..m {
                let (n0, kind) = c.entries[i];
                let (n1, _) = c.entries[(i + 1) % m];
                let key = (n0.min(n1), n0.max(n1));
                boundary.insert(key, kind);
            }
        };

    // Shift a chain's u coordinates by k·span (re-pointing its nodes).
    let shift_chain = |c: &Chain, k: f64, nodes: &mut Vec<PatchNode>| {
        if k == 0.0 {
            return;
        }
        for &(n, _) in &c.entries {
            let p = nodes[n].p2;
            nodes[n].p2 = Point2::new(p.x() + k * span, p.y());
        }
    };

    let (poly, holes): (Vec<Node>, Vec<Vec<Node>>);
    match wrapping.len() {
        0 => {
            // Bounded patch: outer = the unique CCW loop (validated).
            let shoelace2 = |c: &Chain| -> f64 {
                let m = c.entries.len();
                let mut s = 0.0;
                for i in 0..m {
                    let p = nodes[c.entries[i].0].p2;
                    let q = nodes[c.entries[(i + 1) % m].0].p2;
                    s += p.x() * q.y() - q.x() * p.y();
                }
                s
            };
            let Some(outer_idx) = (0..chains.len()).find(|&i| shoelace2(&chains[i]) > 0.0) else {
                return Err(fail("bounded patch has no CCW loop in the unrolled frame"));
            };
            let outer_mid = mid_u(&chains[outer_idx], &nodes);
            for (i, c) in chains.iter().enumerate() {
                if i != outer_idx && !is_pinned(c) {
                    let k = ((outer_mid - mid_u(c, &nodes)) / span).round();
                    shift_chain(c, k, &mut nodes);
                }
            }
            for c in &chains {
                register_chain(c, &mut boundary);
            }
            poly = chains[outer_idx]
                .entries
                .iter()
                .map(|&(n, _)| Node {
                    p2: nodes[n].p2,
                    vid: n as u32,
                })
                .collect();
            holes = (0..chains.len())
                .filter(|&i| i != outer_idx)
                .map(|i| {
                    chains[i]
                        .entries
                        .iter()
                        .map(|&(n, _)| Node {
                            p2: nodes[n].p2,
                            vid: n as u32,
                        })
                        .collect()
                })
                .collect();
        }
        2 => {
            // Barrel segment: cut the two wrapping loops open along a seam
            // bridge pair (universal cover).
            let (ci_p, ci_m) = if chains[wrapping[0]].wrap > 0 {
                (wrapping[0], wrapping[1])
            } else {
                (wrapping[1], wrapping[0])
            };
            if chains[ci_p].wrap + chains[ci_m].wrap != 0 {
                return Err(fail("patch wrapping loops do not wind oppositely"));
            }
            // Place windows near the +wrap loop's span first (they are
            // re-checked by bridge validity below).
            let pmid = mid_u(&chains[ci_p], &nodes);
            for (i, c) in chains.iter().enumerate() {
                if i != ci_p && i != ci_m && c.wrap == 0 && !is_pinned(c) {
                    let k = ((pmid - mid_u(c, &nodes)) / span).round();
                    shift_chain(c, k, &mut nodes);
                }
            }
            for c in &chains {
                register_chain(c, &mut boundary);
            }

            // Candidate anchors: x over the +wrap chain's nodes; y = the
            // u-closest node of the −wrap chain (mod span).
            let pe = &chains[ci_p].entries;
            let me = &chains[ci_m].entries;
            type BuiltRing = (Vec<Node>, Vec<(usize, usize)>);
            let mut built: Option<BuiltRing> = None;
            'anchors: for xi in 0..pe.len() {
                let xu = nodes[pe[xi].0].p2.x();
                // y: minimize |principal Δu|.
                let mut best: Option<(usize, f64)> = None;
                for (yi, &(yn, _)) in me.iter().enumerate() {
                    let du = nodes[yn].p2.x() - xu;
                    let dpr = du - (du / span).round() * span;
                    if best.map(|(_, b)| dpr.abs() < b.abs()).unwrap_or(true) {
                        best = Some((yi, dpr));
                    }
                }
                let Some((yi, dpr)) = best else {
                    continue;
                };
                // Rotated + unwrapped polygon: x..x'(+span), bridge to
                // y'(x'+dpr), M walked from y (REVERSED in u: its wrap is
                // −1, so walking its stored order decreases u), back to y,
                // bridge to x.
                let mut ring: Vec<Node> = Vec::new();
                let m = pe.len();
                let base_x = nodes[pe[xi].0].p2.x();
                for j in 0..=m {
                    let (n, _) = pe[(xi + j) % m];
                    let mut u = nodes[n].p2.x();
                    if j > 0 && (xi + j) >= m {
                        u += span; // continued past the wrap
                    }
                    if j == m {
                        u = base_x + span; // the closing duplicate x'
                    }
                    ring.push(Node {
                        p2: Point2::new(u, nodes[n].p2.y()),
                        vid: n as u32,
                    });
                }
                let mm = me.len();
                let y_target = base_x + span + dpr;
                let y_base = nodes[me[yi].0].p2.x();
                for j in 0..=mm {
                    let (n, _) = me[(yi + j) % mm];
                    let mut u = nodes[n].p2.x();
                    if j > 0 && (yi + j) >= mm {
                        u += f64::from(chains[ci_m].wrap as i32) * span; // −span past the wrap
                    }
                    if j == mm {
                        u = y_base + f64::from(chains[ci_m].wrap as i32) * span;
                    }
                    ring.push(Node {
                        p2: Point2::new(u - y_base + y_target, nodes[n].p2.y()),
                        vid: n as u32,
                    });
                }
                // Bridge edges: (x' → y) at the right seam and (y_end → x)
                // at the left; check both against every boundary edge.
                let bridge_pairs = [
                    (ring[m].p2, ring[m + 1].p2),
                    (ring[m + 1 + mm].p2, ring[0].p2),
                ];
                // Hole chains must be tested (and later placed) at their
                // image INSIDE this candidate ring's window [base_x,
                // base_x+span): the pre-pass mid-window shift centered them
                // near the +wrap chain's MIDPOINT, but the assembled ring is
                // anchored at the CHOSEN bridge azimuth — a hole left at a
                // ±span image of its in-ring position is outside the outer
                // polygon, the flood-fill CDT ignores it, and the corridor
                // over its territory is silently FILLED (measured:
                // `curved_output_reentry_through_boss`, a slot window on a
                // recovered boss whose bridge anchor landed right of the
                // hole after the §4.4.2 restoration shortened the rim
                // chains; the selfx gate caught the filled notch). Pinned
                // chains are frame-locked to the ring and never shifted.
                let hole_window_k = |c: &Chain, base_x: f64| -> f64 {
                    ((base_x + span / 2.0 - mid_u(c, &nodes)) / span).round()
                };
                let blocked = |p: Point2, q: Point2, base_x: f64| -> bool {
                    let mut edges_iter: Vec<(Point2, Point2)> = Vec::new();
                    let rl = ring.len();
                    for i in 0..rl {
                        // The two seam-bridge edges themselves (at indices m
                        // and m+1+mm) are the candidates under test — they
                        // must not self-block.
                        if i == m || i == m + 1 + mm {
                            continue;
                        }
                        edges_iter.push((ring[i].p2, ring[(i + 1) % rl].p2));
                    }
                    for (ci, c) in chains.iter().enumerate() {
                        if ci == ci_p || ci == ci_m {
                            continue;
                        }
                        let k = if is_pinned(c) {
                            0.0
                        } else {
                            hole_window_k(c, base_x)
                        };
                        let cm = c.entries.len();
                        for i in 0..cm {
                            let a2 = nodes[c.entries[i].0].p2;
                            let b2 = nodes[c.entries[(i + 1) % cm].0].p2;
                            edges_iter.push((
                                Point2::new(a2.x() + k * span, a2.y()),
                                Point2::new(b2.x() + k * span, b2.y()),
                            ));
                        }
                    }
                    edges_iter
                        .into_iter()
                        .any(|(ea, eb)| exact2d::bridge_blocked_by(p, q, ea, eb))
                };
                if bridge_pairs
                    .iter()
                    .any(|&(p, q)| p == q || blocked(p, q, base_x))
                {
                    continue 'anchors;
                }
                // Accepted: place every un-pinned hole chain at its image
                // inside THIS ring's window (rigid k·span shift — the same
                // image `blocked` just validated against).
                let shifts: Vec<(usize, f64)> = chains
                    .iter()
                    .enumerate()
                    .filter(|&(ci, c)| ci != ci_p && ci != ci_m && !is_pinned(c))
                    .map(|(ci, c)| (ci, hole_window_k(c, base_x)))
                    .collect();
                for (ci, k) in shifts {
                    shift_chain(&chains[ci], k, &mut nodes);
                }
                // Register the bridge edges (Chord kind) for refinement.
                let xs = pe[xi].0;
                let ys = me[yi].0;
                let b1 = (xs.min(ys), xs.max(ys));
                built = Some((ring, vec![b1]));
                break;
            }
            let Some((ring, bridges)) = built else {
                return Err(fail(
                    "no unblocked seam bridge for the wrapping patch loops",
                ));
            };
            for key in bridges {
                boundary.insert(key, PatchEdgeKind::Chord);
            }
            poly = ring;
            let mut holes_v: Vec<Vec<Node>> = (0..chains.len())
                .filter(|&i| i != ci_p && i != ci_m)
                .map(|i| {
                    chains[i]
                        .entries
                        .iter()
                        .map(|&(n, _)| Node {
                            p2: nodes[n].p2,
                            vid: n as u32,
                        })
                        .collect()
                })
                .collect();
            // A hole pinned to the outer ring (shared node — pass 1.5) must
            // present its shared vertices at the ring copy's EXACT cut-frame
            // p2: the CDT weld needs bit equality, and the ring assembly
            // above may have moved the copy (±span past the wrap, or the
            // −wrap chain's seam re-anchor). The rest of the hole translates
            // rigidly by the first shared vertex's Δu so the hole stays
            // congruent. A hole with no ring-shared node is untouched.
            for hole in &mut holes_v {
                let ring_copy = |vid: u32, near_u: f64| -> Option<Point2> {
                    poly.iter()
                        .filter(|rn| rn.vid == vid)
                        .map(|rn| rn.p2)
                        .min_by(|a, b| (a.x() - near_u).abs().total_cmp(&(b.x() - near_u).abs()))
                };
                let Some(du) = hole
                    .iter()
                    .find_map(|hn| ring_copy(hn.vid, hn.p2.x()).map(|rp| rp.x() - hn.p2.x()))
                else {
                    continue;
                };
                for hn in hole.iter_mut() {
                    match ring_copy(hn.vid, hn.p2.x()) {
                        Some(rp) => hn.p2 = rp,
                        None => hn.p2 = Point2::new(hn.p2.x() + du, hn.p2.y()),
                    }
                }
            }
            holes = holes_v;
        }
        _ => return Err(fail("patch must have exactly 0 or 2 axis-wrapping loops")),
    }

    // ---- pass 3: constrained-Delaunay triangulation -----------------------
    // (spec kv2_cdt_triangulation_core §3, branches C1–C4; §6b M1/M2/M3). The
    // greedy exact ear-clip + f64 flip minted sub-f32 slivers from healthy
    // boundaries: the flip's plain-f64 incircle is catastrophically
    // ill-conditioned exactly on slivers, so it could not remove them and LEPP
    // then propagated them into dozens of B2 twins. `triangulate_ring` runs the
    // flood-fill CDT (M2, topological interior classification) + the M1
    // grid-degeneracy flip pass, so if any triangulation avoids the sliver, the
    // CDT avoids it and the flip pass repairs the residual grid-flat wedges.
    // Hole loops are passed NATIVELY (no bridge corridors — corridor-doubled
    // vertices would be rejected by the CDT as coincident).
    //
    // Register every outer-ring adjacency (covers the no-hole case and the
    // seam duplicates); hole adjacencies were registered by `register_chain`
    // above. Kinds set earlier (arc samples, seam bridges) survive `or_insert`.
    {
        let m = poly.len();
        for i in 0..m {
            let (a_id, b_id) = (poly[i].vid as usize, poly[(i + 1) % m].vid as usize);
            if a_id == b_id {
                continue;
            }
            let key = (a_id.min(b_id), a_id.max(b_id));
            boundary.entry(key).or_insert(PatchEdgeKind::Chord);
        }
    }

    // CDT vertex pool: the outer ring's per-ring Point2 (CUT frame — carrying
    // the seam-shifted u values and duplicate node ids at DISTINCT 2D
    // positions, which the CDT accepts), then each hole loop's Point2. Each
    // pool index keeps its original node-table id so the refinement and emit
    // keys below stay in node-id space.
    let mut pool_p2: Vec<Point2> = Vec::with_capacity(poly.len());
    let mut pool_node: Vec<usize> = Vec::with_capacity(poly.len());
    let mut outer_cdt: Vec<u32> = Vec::with_capacity(poly.len());
    for nd in &poly {
        outer_cdt.push(pool_p2.len() as u32);
        pool_p2.push(nd.p2);
        pool_node.push(nd.vid as usize);
    }
    let holes_cdt: Vec<Vec<u32>> = holes
        .iter()
        .map(|hole| {
            hole.iter()
                .map(|nd| {
                    let idx = pool_p2.len() as u32;
                    pool_p2.push(nd.p2);
                    pool_node.push(nd.vid as usize);
                    idx
                })
                .collect()
        })
        .collect();
    // 3D positions per pool index (for the M1 grid-degeneracy flip pass).
    let pool_p3: Vec<[f64; 3]> = pool_node.iter().map(|&nd| nodes[nd].pos).collect();
    // Branch C4: any CDT rejection (coincident verts / crossing constraints /
    // zero area) is a loud typed failure — never a fallback (P9). The M1
    // grid-degeneracy flip pass (spec §6b) runs BEFORE pass 4 refinement.
    if prov_on {
        for (i, nd) in poly.iter().enumerate() {
            let nid = nd.vid as usize;
            let (he_s, tw_s, ck) = match node_prov.get(&nid) {
                Some((h, t)) => (
                    format!("{h:?}"),
                    format!("{t:?}"),
                    arena.half_edge(*h).map_or("?", |he| match he.curve {
                        Curve::LineSegment => "Line",
                        Curve::SurfacePair { .. } => "SurfacePair",
                        Curve::Circle { .. } => "Circle",
                        Curve::Arc { .. } => "Arc",
                        Curve::EllipseArc { .. } => "EllipseArc",
                        Curve::HyperbolaArc { .. } => "HyperbolaArc",
                    }),
                ),
                None => ("sample".to_string(), "sample".to_string(), "sample"),
            };
            let p = nodes[nid].pos;
            eprintln!(
                "KV2_PATCH_PROV face={fid:?} idx={i} node={nid} he={he_s} twin={tw_s} \
                 curve={ck} p2=[{:.12},{:.12}] pos=[{:.12},{:.12},{:.12}]",
                nd.p2.x(),
                nd.p2.y(),
                p[0],
                p[1],
                p[2],
            );
        }
    }
    let cdt_tris =
        triangulate_with_pinch_split(&pool_p2, &pool_p3, &outer_cdt, &holes_cdt).map_err(fail)?;

    // ---- pass 4: conforming chord-bound refinement -------------------------
    // Triangles in "work" coordinates: each corner = (p2 in the CUT frame,
    // node id). Two corners may share a node id at different p2 (the seam);
    // refinement keys edges by (node-id pair + p2 pair bits) so the two
    // seam instances refine independently but their splits stay collinear
    // 3D chords (closure-safe).
    #[derive(Clone, Copy)]
    struct WNode {
        p2: Point2,
        node: usize,
    }
    let mut wnodes: Vec<WNode> = pool_p2
        .iter()
        .zip(pool_node.iter())
        .map(|(&p, &n)| WNode { p2: p, node: n })
        .collect();
    // Fold-probe context: nodes below this index existed before refinement
    // (ring/pool); at or above = minted by the LEPP splits.
    let n_prerefine = nodes.len();
    // Dev-only chain probe part 2: the FINAL work-node chart table (post
    // seam-window shifts), one row per work node — names which samples are
    // grid-aligned, which are conforming inserts, and where the ladders
    // interleave.
    if std::env::var("KV2_PATCH_CHAIN_PROBE").is_ok_and(|v| v == format!("{}", fid.0)) {
        for (wi, wn) in wnodes.iter().enumerate() {
            let p = nodes[wn.node].pos;
            eprintln!(
                "[chain-node] w={wi} node={} u={:.6} v={:.6} pos=({:.9e},{:.9e},{:.9e})",
                wn.node,
                wn.p2.x(),
                wn.p2.y(),
                p[0],
                p[1],
                p[2]
            );
        }
    }
    let mut wtris: Vec<[usize; 3]> = cdt_tris
        .iter()
        .map(|t| [t[0] as usize, t[1] as usize, t[2] as usize])
        .collect();
    // Edge kind lookup for work edges: boundary by node-id pair, else
    // Interior.
    let kind_of = |wa: &WNode,
                   wb: &WNode,
                   boundary: &std::collections::BTreeMap<(usize, usize), PatchEdgeKind>|
     -> PatchEdgeKind {
        let key = (wa.node.min(wb.node), wa.node.max(wb.node));
        *boundary.get(&key).unwrap_or(&PatchEdgeKind::Interior)
    };
    // Cache of split midpoints keyed by the WORK edge (p2-bit pair), so the
    // two triangles sharing an edge get the same midpoint node (conforming).
    // Midpoint cache: WORK edge (p2-bit pair) → wnode index of its split.
    let mut split_cache: std::collections::BTreeMap<EKey, usize> =
        std::collections::BTreeMap::new();
    let w_limit = w_facet * (1.0 + 1e-9);
    // Refinement by Rivara longest-edge propagation (LEPP), EUCLIDEAN
    // metric, with the chord-bound STOP criterion in Δu: a triangle needs
    // refinement while any of its edges spans more than one facet width in
    // `u`; the edge BISECTED is always a locally-longest (Euclidean) edge,
    // found by walking strictly-longer neighbor maxima to a terminal edge.
    // Euclidean longest-edge bisection is the classic quality-preserving
    // scheme (Rivara 1984: finitely many similarity classes, angles bounded
    // below by the initial mesh's) — a Δu-metric variant tried first
    // produced sliver cascades (degenerate metric ⇒ no angle bound) that
    // blew the triangle count up and emitted zero-area slivers. Convergence
    // of the stop criterion: bisection halves edge lengths geometrically,
    // and an edge's Δu is bounded by its length.
    // KV9-F2b: the SECOND refinement criterion — the chart→3D lift must be
    // orientation-faithful on every emitted triangle.
    //
    // The Δu criterion below bounds each chord's sagitta, which is what the
    // render band needs, but it says nothing about whether a triangle SURVIVES
    // the lift. A chart triangle is lifted by taking its three corners onto the
    // surface and spanning them flat; the chords cut INSIDE the surface, so a
    // triangle thin enough relative to the sagitta of its own edges comes out
    // facing inward. That is the KV9-F2 fold, and until now nothing upstream of
    // the emit tripwire could prevent it — the refinement was free to MINT a
    // folded triangle and the tripwire's only move was to fail the whole patch.
    //
    // Bisection converges on this quadratically: halving an edge quarters its
    // sagitta while only halving the triangle's height, so height/sagitta
    // doubles per level.
    //
    // But bisection can only ever remove SAGITTA. It cannot move a node that
    // sits off the ideal development, and a fold caused by such a node is not
    // this criterion's to fix — that is the F2a family, owned by
    // `yang_434_output_chord_refinement.md` (a `Chord` split inheriting its
    // parent chord's off-surface depth). The two are told apart by comparing
    // the two quantities directly, with no tuned constant between them:
    //
    //   dev  = how far the nodes sit OFF the ideal development  (immovable)
    //   sag  = the ideal chart-lift sagitta of the triangle's edges (removable)
    //
    // Refining is worth attempting only while `dev < sag`. This also makes the
    // arm SELF-TERMINATING: sag falls quadratically under bisection, so it
    // crosses any fixed dev within a few levels and the arm declines on its
    // own. Measured: R0003 face 577 (dev 8.2e-2 against a sag of 1.4e-9 — a
    // node further off-surface than the triangle is wide) declines immediately
    // instead of burning 28 104 splits on a fold refinement cannot reach.
    //
    // This is NOT a tolerance band. The predicate is `the lift inverts`
    // (dot ≤ 0), not a tuned margin; it is strictly INSIDE the emit tripwire's
    // own −0.1 verdict; and it silences nothing — a triangle it declines, or
    // fails to fix, reaches the tripwire and fails loudly exactly as before.
    let lift_inverts = |t: [usize; 3], wnodes: &[WNode], nodes: &[PatchNode]| -> bool {
        let p = |w: usize| nodes[wnodes[w].node].pos;
        let (pa, pb, pc) = (p(t[0]), p(t[1]), p(t[2]));
        let u = [pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]];
        let v = [pc[0] - pa[0], pc[1] - pa[1], pc[2] - pa[2]];
        let n3 = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        let nl = (n3[0] * n3[0] + n3[1] * n3[1] + n3[2] * n3[2]).sqrt();
        // Same sub-resolution skip the emit tripwire uses: below this the
        // normal carries no signal to refine towards. A NaN falls through to
        // the dot test below, which is false for NaN — so it is skipped too.
        if nl <= 1e-12 * (1.0 + r_unroll * r_unroll) {
            return false;
        }
        let cen = [
            (pa[0] + pb[0] + pc[0]) / 3.0,
            (pa[1] + pb[1] + pc[1]) / 3.0,
            (pa[2] + pb[2] + pc[2]) / 3.0,
        ];
        let inverted = match outward_at(cen) {
            Some(ow) => (n3[0] * ow[0] + n3[1] * ow[1] + n3[2] * ow[2]) / nl <= 0.0,
            None => false,
        };
        if !inverted {
            return false;
        }
        let dist = |x: [f64; 3], y: [f64; 3]| -> f64 {
            let d = [x[0] - y[0], x[1] - y[1], x[2] - y[2]];
            (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
        };
        // How far the nodes sit off the ideal development — what bisection
        // CANNOT remove.
        let mut dev = 0.0f64;
        for &w in &t {
            let q = wnodes[w].p2;
            dev = dev.max(dist(nodes[wnodes[w].node].pos, surface_point(q.x(), q.y())));
        }
        // The ideal chart-lift sagitta of the triangle's edges — what bisection
        // CAN remove. Taken between IDEAL surface points so it stays
        // independent of `dev` above.
        let mut sag = 0.0f64;
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            let (qa, qb) = (wnodes[t[i]].p2, wnodes[t[j]].p2);
            let (sa, sb) = (surface_point(qa.x(), qa.y()), surface_point(qb.x(), qb.y()));
            let chord_mid = [
                (sa[0] + sb[0]) / 2.0,
                (sa[1] + sb[1]) / 2.0,
                (sa[2] + sb[2]) / 2.0,
            ];
            let on_surf = surface_point((qa.x() + qb.x()) / 2.0, (qa.y() + qb.y()) / 2.0);
            sag = sag.max(dist(chord_mid, on_surf));
        }
        dev < sag
    };
    let max_du = |t: [usize; 3], wnodes: &[WNode]| -> f64 {
        let mut best = -1.0f64;
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            best = best.max((wnodes[t[i]].p2.x() - wnodes[t[j]].p2.x()).abs());
        }
        best
    };
    let longest_edge = |t: [usize; 3], wnodes: &[WNode]| -> (usize, usize, f64) {
        let mut best = (t[0], t[1], -1.0f64);
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            let dx = wnodes[t[i]].p2.x() - wnodes[t[j]].p2.x();
            let dy = wnodes[t[i]].p2.y() - wnodes[t[j]].p2.y();
            let l2 = dx * dx + dy * dy;
            if l2 > best.2 {
                best = (t[i], t[j], l2);
            }
        }
        best
    };
    // Edge → incident-triangle adjacency (by p2-bit key), kept current
    // across splits so LEPP walks and conforming splits are O(degree), not
    // full-mesh scans.
    type EKey = ((u64, u64), (u64, u64));
    let ekey = |wa: &WNode, wb: &WNode| -> EKey {
        let (ka, kb) = (
            (wa.p2.x().to_bits(), wa.p2.y().to_bits()),
            (wb.p2.x().to_bits(), wb.p2.y().to_bits()),
        );
        if ka <= kb {
            (ka, kb)
        } else {
            (kb, ka)
        }
    };
    let mut edge_tris: std::collections::BTreeMap<EKey, Vec<usize>> =
        std::collections::BTreeMap::new();
    for (ti, t) in wtris.iter().enumerate() {
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            edge_tris
                .entry(ekey(&wnodes[t[i]], &wnodes[t[j]]))
                .or_default()
                .push(ti);
        }
    }
    let mut work: std::collections::VecDeque<usize> = (0..wtris.len()).collect();
    let mut guard = 0usize;
    // Convergence budget for the orientation arm (NOT an accuracy band): how
    // far below the chord criterion it may keep bisecting before giving up and
    // letting the emit tripwire speak. Sagitta falls as Δu², so 2^-12 of a
    // facet width is ~1.7e7× less sag than the chord criterion allows — far
    // past where any real fold survives. A triangle that still inverts there
    // is chart-degenerate, and more bisection cannot help it.
    let du_floor = w_limit / 4096.0;
    let lift_refine = !matches!(
        std::env::var("KV2_PATCH_LIFT_REFINE").as_deref(),
        Ok("0") | Ok("off")
    );
    while let Some(seed) = work.pop_front() {
        let du = max_du(wtris[seed], &wnodes);
        if du <= w_limit
            && !(lift_refine && du > du_floor && lift_inverts(wtris[seed], &wnodes, &nodes))
        {
            continue;
        }
        guard += 1;
        if guard > 4_000_000 {
            return Err(fail("refinement did not converge (split budget exhausted)"));
        }
        // LEPP walk: follow strictly-longer neighbor maxima (Euclidean
        // length strictly increases each hop, so the walk is finite; the
        // inner guard is a loud tripwire, never a silent clamp).
        let mut cur = seed;
        let mut hops = 0usize;
        let (ia, ib) = loop {
            hops += 1;
            if hops > wtris.len() + 16 {
                return Err(fail("refinement LEPP walk did not terminate"));
            }
            let (ia, ib, l2) = longest_edge(wtris[cur], &wnodes);
            let ck = ekey(&wnodes[ia], &wnodes[ib]);
            let mut next = None;
            if let Some(tris) = edge_tris.get(&ck) {
                for &tj in tris {
                    if tj != cur && longest_edge(wtris[tj], &wnodes).2 > l2 {
                        next = Some(tj);
                        break;
                    }
                }
            }
            match next {
                Some(tj) => cur = tj,
                None => break (ia, ib),
            }
        };
        let (wa, wb) = (wnodes[ia], wnodes[ib]);
        let kind = kind_of(&wa, &wb, &boundary);
        let ckey = ekey(&wa, &wb);
        let mid_w = match split_cache.get(&ckey) {
            Some(&mi) => mi,
            None => {
                let mp2 = Point2::new((wa.p2.x() + wb.p2.x()) / 2.0, (wa.p2.y() + wb.p2.y()) / 2.0);
                let (pa, pb) = (nodes[wa.node].pos, nodes[wb.node].pos);
                let pos = match kind {
                    // Boundary edges split ON their own straight 3D
                    // geometry. An ArcSample sub-edge is the chord between
                    // two on-circle samples — already within the chord band;
                    // its lerped split point stays ON that chord (exactly
                    // collinear), so the neighboring face's unsplit copy of
                    // the chord remains closure-safe (T-junction).
                    PatchEdgeKind::Chord | PatchEdgeKind::ArcSample => [
                        (pa[0] + pb[0]) / 2.0,
                        (pa[1] + pb[1]) / 2.0,
                        (pa[2] + pb[2]) / 2.0,
                    ],
                    PatchEdgeKind::Interior => surface_point(mp2.x(), mp2.y()),
                };
                let node_idx = nodes.len();
                nodes.push(PatchNode { p2: mp2, pos });
                // Sub-edges inherit the kind so further splits stay on
                // the same geometry.
                if kind != PatchEdgeKind::Interior {
                    let k1 = (wa.node.min(node_idx), wa.node.max(node_idx));
                    let k2 = (wb.node.min(node_idx), wb.node.max(node_idx));
                    boundary.insert(k1, kind);
                    boundary.insert(k2, kind);
                }
                let mi = wnodes.len();
                wnodes.push(WNode {
                    p2: mp2,
                    node: node_idx,
                });
                split_cache.insert(ckey, mi);
                mi
            }
        };
        // Split EVERY triangle currently containing this work edge
        // (1 on boundary, 2 interior; corridor duplicates share the key) —
        // conforming. Adjacency is updated in place.
        let incident = edge_tris.remove(&ckey).unwrap_or_default();
        for tj in incident {
            let tt = wtris[tj];
            let mut found = None;
            for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
                if ekey(&wnodes[tt[i]], &wnodes[tt[j]]) == ckey {
                    found = Some((i, j));
                    break;
                }
            }
            let Some((i, j)) = found else {
                continue; // stale adjacency entry (triangle already replaced)
            };
            let k = 3 - i - j;
            let (na, nb, nc) = (tt[i], tt[j], tt[k]);
            // Unregister tj's old edges, register the two children's.
            for (x, y) in [(na, nb), (nb, nc), (nc, na)] {
                if let Some(v) = edge_tris.get_mut(&ekey(&wnodes[x], &wnodes[y])) {
                    v.retain(|&t| t != tj);
                }
            }
            let new_idx = wtris.len();
            // TEMP diagnostic (KV9-F2b, env-gated `KV2_PATCH_MINT_PROBE`):
            // the MINTING event. For each split, the parent's and the two
            // children's aspect in the surface metric, printed when a child
            // is materially worse than its parent. Names which bisection
            // degrades quality rather than inferring it from the end state.
            if std::env::var_os("KV2_PATCH_MINT_PROBE").is_some() {
                let asp = |x: usize, y: usize, z: usize| -> f64 {
                    let (p, q, r) = (wnodes[x].p2, wnodes[y].p2, wnodes[z].p2);
                    let (la, lb, lc) = (
                        iso.dist2(p, q).sqrt(),
                        iso.dist2(q, r).sqrt(),
                        iso.dist2(r, p).sqrt(),
                    );
                    let s = (la + lb + lc) / 2.0;
                    let ar =
                        (s * (s - la).max(0.0) * (s - lb).max(0.0) * (s - lc).max(0.0)).max(0.0);
                    let area2 = 2.0 * ar.sqrt();
                    let lmax = la.max(lb).max(lc);
                    if area2 <= 0.0 {
                        f64::INFINITY
                    } else {
                        lmax * lmax / area2
                    }
                };
                let parent = asp(na, nb, nc);
                let (c1, c2) = (asp(na, mid_w, nc), asp(mid_w, nb, nc));
                if c1.max(c2) > 2.0 * parent.max(20.0) {
                    eprintln!(
                        "[mint] face={fid:?} split#{} parent_asp={parent:.1} \
                         children={c1:.1},{c2:.1} kind={kind:?} hops={hops} \
                         edge=({:.3},{:.3})-({:.3},{:.3}) apex=({:.3},{:.3})",
                        split_cache.len(),
                        wa.p2.x(),
                        wa.p2.y(),
                        wb.p2.x(),
                        wb.p2.y(),
                        wnodes[nc].p2.x(),
                        wnodes[nc].p2.y()
                    );
                }
            }
            wtris[tj] = [na, mid_w, nc];
            wtris.push([mid_w, nb, nc]);
            for (ti2, tri) in [(tj, [na, mid_w, nc]), (new_idx, [mid_w, nb, nc])] {
                for (x, y) in [(tri[0], tri[1]), (tri[1], tri[2]), (tri[2], tri[0])] {
                    edge_tris
                        .entry(ekey(&wnodes[x], &wnodes[y]))
                        .or_default()
                        .push(ti2);
                }
                work.push_back(ti2);
            }
        }
        // The seed may still carry an over-limit edge — requeue it.
        work.push_back(seed);
    }

    // TEMP diagnostic (KV9-F2 anchor, env-gated `KV2_PATCH_ASPECT_PROBE`):
    // triangle quality measured in the TRUE SURFACE metric (3D chord aspect =
    // lmax^2 / (2*area3D)), reported for the initial CDT and for the
    // post-refinement mesh. Answers whether the refinement MINTS the KV9-F2
    // sliver or inherits it from the CDT. The chart metric is NOT the surface
    // metric on a cone, so a chart-benign triangle can be a 3D sliver.
    if std::env::var_os("KV2_PATCH_ASPECT_PROBE").is_some() {
        let aspect3 = |na: usize, nb: usize, nc: usize| -> f64 {
            let (pa, pb, pc) = (nodes[na].pos, nodes[nb].pos, nodes[nc].pos);
            let u = [pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]];
            let v = [pc[0] - pa[0], pc[1] - pa[1], pc[2] - pa[2]];
            let w = [pc[0] - pb[0], pc[1] - pb[1], pc[2] - pb[2]];
            let n3 = [
                u[1] * v[2] - u[2] * v[1],
                u[2] * v[0] - u[0] * v[2],
                u[0] * v[1] - u[1] * v[0],
            ];
            let area2 = (n3[0] * n3[0] + n3[1] * n3[1] + n3[2] * n3[2]).sqrt();
            let l = |d: [f64; 3]| (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
            let lmax = l(u).max(l(v)).max(l(w));
            if area2 <= 0.0 {
                f64::INFINITY
            } else {
                lmax * lmax / area2
            }
        };
        let aspect_iso = |na: usize, nb: usize, nc: usize| -> f64 {
            let (a2, b2, c2) = (nodes[na].p2, nodes[nb].p2, nodes[nc].p2);
            let (la, lb, lc) = (
                iso.dist2(a2, b2).sqrt(),
                iso.dist2(b2, c2).sqrt(),
                iso.dist2(c2, a2).sqrt(),
            );
            let s = (la + lb + lc) / 2.0;
            let ar = (s * (s - la).max(0.0) * (s - lb).max(0.0) * (s - lc).max(0.0)).max(0.0);
            let area2 = 2.0 * ar.sqrt();
            let lmax = la.max(lb).max(lc);
            if area2 <= 0.0 {
                f64::INFINITY
            } else {
                lmax * lmax / area2
            }
        };
        let mut cdt_worst_iso = 0.0f64;
        let mut ref_worst_iso = 0.0f64;
        let mut cdt_worst = 0.0f64;
        for t in &cdt_tris {
            let (i, j, k) = (t[0] as usize, t[1] as usize, t[2] as usize);
            cdt_worst = cdt_worst.max(aspect3(pool_node[i], pool_node[j], pool_node[k]));
            cdt_worst_iso = cdt_worst_iso.max(aspect_iso(pool_node[i], pool_node[j], pool_node[k]));
        }
        let mut ref_worst = 0.0f64;
        let mut n_over_100 = 0usize;
        for t in &wtris {
            let a3 = aspect3(wnodes[t[0]].node, wnodes[t[1]].node, wnodes[t[2]].node);
            ref_worst_iso = ref_worst_iso.max(aspect_iso(
                wnodes[t[0]].node,
                wnodes[t[1]].node,
                wnodes[t[2]].node,
            ));
            ref_worst = ref_worst.max(a3);
            if a3 > 100.0 {
                n_over_100 += 1;
            }
        }
        eprintln!(
            "[aspect-probe] face={fid:?} cdt_tris={} cdt_worst3={cdt_worst:.2} \
             cdt_worst_iso={cdt_worst_iso:.2} refined_tris={} refined_worst3={ref_worst:.2} \
             refined_worst_iso={ref_worst_iso:.2} n_over_100={n_over_100}",
            cdt_tris.len(),
            wtris.len()
        );
    }

    if std::env::var_os("KV2_PATCH_PASS_PROBE").is_some() {
        eprintln!(
            "[pass-probe] face={fid:?} cdt_tris={} refined_tris={} wnodes={} splits={}",
            cdt_tris.len(),
            wtris.len(),
            wnodes.len(),
            split_cache.len()
        );
        // Ring/hole window extents: a hole outside the outer ring's window
        // is the filled-corridor defect the barrel-arm hole re-windowing
        // guards against.
        for (hi_, hole) in holes.iter().enumerate() {
            let us: Vec<f64> = hole.iter().map(|n| n.p2.x()).collect();
            let (lo, hi2) = us
                .iter()
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &u| {
                    (a.min(u), b.max(u))
                });
            eprintln!("[hole-ring] {hi_}: u=[{lo:.4},{hi2:.4}] n={}", hole.len());
        }
        {
            let us: Vec<f64> = poly.iter().map(|n| n.p2.x()).collect();
            let (lo, hi2) = us
                .iter()
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &u| {
                    (a.min(u), b.max(u))
                });
            eprintln!("[outer-ring] u=[{lo:.4},{hi2:.4}] n={}", poly.len());
        }
    }

    // §4.3.4 inc-0 census: post-refinement stats. The row itself prints at
    // the emit exits (fold verdict known there); `None` when the census is
    // off — zero cost on the production path.
    let census_prefix: Option<String> = if census_on {
        let mut max_split_dev = 0.0f64;
        for wn in &wnodes {
            if wn.node >= n_prerefine {
                let p = nodes[wn.node].pos;
                let ideal = surface_point(wn.p2.x(), wn.p2.y());
                let dev = ((p[0] - ideal[0]).powi(2)
                    + (p[1] - ideal[1]).powi(2)
                    + (p[2] - ideal[2]).powi(2))
                .sqrt();
                max_split_dev = max_split_dev.max(dev);
            }
        }
        let mut min_h2d = f64::INFINITY;
        for t in &wtris {
            let (a2, b2, c2) = (wnodes[t[0]].p2, wnodes[t[1]].p2, wnodes[t[2]].p2);
            let area2 = ((b2.x() - a2.x()) * (c2.y() - a2.y())
                - (b2.y() - a2.y()) * (c2.x() - a2.x()))
            .abs();
            let mut lmax = 0.0f64;
            for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
                let (pi2, pj2) = (wnodes[t[i]].p2, wnodes[t[j]].p2);
                let l2 = (pi2.x() - pj2.x()).powi(2) + (pi2.y() - pj2.y()).powi(2);
                lmax = lmax.max(l2);
            }
            let lmax = lmax.sqrt();
            if area2 > 0.0 && lmax > 0.0 {
                min_h2d = min_h2d.min(area2 / lmax);
            }
        }
        Some(format!(
            "[chord-census] face={fid:?} kind=dev w_facet={w_facet:.6e} \
             r_unroll={r_unroll:.6e} n_chord={census_n_chord} \
             max_chord_sag={census_max_sag:.6e} n_split={} \
             max_split_dev={max_split_dev:.6e} min_h2d={min_h2d:.6e}",
            split_cache.len()
        ))
    } else {
        None
    };

    // ---- pass 5: emit ------------------------------------------------------
    // Sense-adjusted outward normal at a surface point: `unit(r̂ − tan α·â)`
    // — the generator-perpendicular cone normal, reducing EXACTLY to the
    // pure radial `r̂` for cylinders (tan α = 0). `None` on the axis.
    let range_start = out.indices.len() as u32;
    let base = out.num_vertices() as u32;
    // One render vertex per WORK node (seam duplicates emit twice at the
    // same position — per-face vertices are never shared anyway).
    for wn in &wnodes {
        let pos = nodes[wn.node].pos;
        let Some(nrm) = outward_at(pos) else {
            return Err(fail("patch render vertex has no radial direction"));
        };
        out.positions.extend_from_slice(&pos);
        out.normals.extend_from_slice(&nrm);
    }
    for t in &wtris {
        // PR-KV9 fold tripwire (KV7-F1 class): a folded unrolled
        // triangulation emits triangles whose 3D winding faces INTO the
        // surface. Each emitted triangle's normal must agree with the
        // sense-adjusted outward radial at its centroid — a clear-margin
        // check (unit dot < −0.1 is a fold, not sliver noise; slivers with
        // sub-resolution area are skipped). Loud failure beats silently
        // shipping inverted geometry (P9).
        let pnt = |w: usize| nodes[wnodes[w].node].pos;
        let (pa, pb, pc) = (pnt(t[0]), pnt(t[1]), pnt(t[2]));
        // Render-precision degeneracy gate (spec
        // `kv2_patch_render_degeneracy_gate`, the F0047 class): geometry
        // that is valid at f64 but COLLAPSED at f32 render precision must
        // fail loudly — the f64 ear-clip/refinement can converge while
        // emitting sub-f32 slivers whose render edges then pair wrong
        // (silent non-manifold output past every fold tripwire below).
        // B2: two vertices bitwise-identical after f32 rounding. B3: the
        // f32 cross product exactly zero (collinear at render precision).
        // Always-on (I3) — never debug-gated, never a skip/snap (P9).
        if f32_render_degenerate(pa, pb, pc) {
            return Err(fail("patch triangle collapsed at render precision"));
        }
        let u = [pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]];
        let v = [pc[0] - pa[0], pc[1] - pa[1], pc[2] - pa[2]];
        let n3 = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        let nl = (n3[0] * n3[0] + n3[1] * n3[1] + n3[2] * n3[2]).sqrt();
        if nl > 1e-12 * (1.0 + r_unroll * r_unroll) {
            let cen = [
                (pa[0] + pb[0] + pc[0]) / 3.0,
                (pa[1] + pb[1] + pc[1]) / 3.0,
                (pa[2] + pb[2] + pc[2]) / 3.0,
            ];
            if let Some(ow) = outward_at(cen) {
                let dot = (n3[0] * ow[0] + n3[1] * ow[1] + n3[2] * ow[2]) / nl;
                if dot < -0.1 {
                    // Diagnostic probe (env-gated, zero-cost off): dump the
                    // folded triangle's work/3D coordinates so a fold class
                    // self-localizes (which region of the development, which
                    // edge kinds, boundary or interior split points).
                    if std::env::var_os("KV2_PATCH_FOLD_PROBE").is_some() {
                        eprintln!(
                            "[fold-probe] face={fid:?} dot={dot:.4} sense={sense} \
                             tan_a={tan_a:.6e} r_unroll={r_unroll:.6e}"
                        );
                        for (label, w) in [("a", t[0]), ("b", t[1]), ("c", t[2])] {
                            let wn = &wnodes[w];
                            let p = nodes[wn.node].pos;
                            // Deviation from the ideal development: the 3D
                            // distance between the stored position and the
                            // surface point of the node's own work coords.
                            let ideal = surface_point(wn.p2.x(), wn.p2.y());
                            let dev = ((p[0] - ideal[0]).powi(2)
                                + (p[1] - ideal[1]).powi(2)
                                + (p[2] - ideal[2]).powi(2))
                            .sqrt();
                            eprintln!(
                                "  {label}: p2=({:.6},{:.6}) node={}{} dev={dev:.3e} \
                                 pos=({:.9e},{:.9e},{:.9e})",
                                wn.p2.x(),
                                wn.p2.y(),
                                wn.node,
                                if wn.node < n_prerefine {
                                    " (pool)"
                                } else {
                                    " (split)"
                                },
                                p[0],
                                p[1],
                                p[2]
                            );
                        }
                        for (la, lb, i, j) in [
                            ("a", "b", t[0], t[1]),
                            ("b", "c", t[1], t[2]),
                            ("c", "a", t[2], t[0]),
                        ] {
                            let kind = kind_of(&wnodes[i], &wnodes[j], &boundary);
                            eprintln!("  edge {la}-{lb}: kind={kind:?}");
                        }
                        // Deep-chord localizer: every Chord-kind boundary
                        // entry whose 3D midpoint sits off the ideal
                        // development names a candidate F2a parent. Print
                        // the deepest with node ids + 3D endpoints so the
                        // producing yang output edge is identifiable by
                        // coordinates.
                        let mut deep: Vec<(f64, usize, usize)> = boundary
                            .iter()
                            .filter(|(_, k)| **k == PatchEdgeKind::Chord)
                            .map(|(&(i, j), _)| {
                                let (a, b) = (&nodes[i], &nodes[j]);
                                let mp2x = (a.p2.x() + b.p2.x()) / 2.0;
                                let mp2y = (a.p2.y() + b.p2.y()) / 2.0;
                                let ideal = surface_point(mp2x, mp2y);
                                let mid = [
                                    (a.pos[0] + b.pos[0]) / 2.0,
                                    (a.pos[1] + b.pos[1]) / 2.0,
                                    (a.pos[2] + b.pos[2]) / 2.0,
                                ];
                                let dev = ((mid[0] - ideal[0]).powi(2)
                                    + (mid[1] - ideal[1]).powi(2)
                                    + (mid[2] - ideal[2]).powi(2))
                                .sqrt();
                                (dev, i, j)
                            })
                            .collect();
                        deep.sort_by(|x, y| y.0.total_cmp(&x.0));
                        for &(dev, i, j) in deep.iter().take(8) {
                            let (a, b) = (&nodes[i], &nodes[j]);
                            eprintln!(
                                "  [deep-chord] dev={dev:.3e} n{i}{}–n{j}{} \
                                 pa=({:.9e},{:.9e},{:.9e}) pb=({:.9e},{:.9e},{:.9e})",
                                if i < n_prerefine { "" } else { "*" },
                                if j < n_prerefine { "" } else { "*" },
                                a.pos[0],
                                a.pos[1],
                                a.pos[2],
                                b.pos[0],
                                b.pos[1],
                                b.pos[2]
                            );
                        }
                    }
                    if let Some(prefix) = &census_prefix {
                        eprintln!("{prefix} fold=inverted");
                    }
                    return Err(fail(
                        "patch triangulation folded (inverted triangle) — KV9-F2: the                          unrolled ear-clip/refinement produced inward-facing geometry;                          loud instead of silently-wrong render output",
                    ));
                }
            }
        }
        out.indices.extend_from_slice(&[
            base + t[0] as u32,
            base + t[1] as u32,
            base + t[2] as u32,
        ]);
    }
    // PR-KV11 fold tripwire extension (KV7-F1/KV9-F2 class): a folded
    // unrolled triangulation can keep its 3D winding within the −0.1 radial
    // margin yet stack jittered sliver layers over one boundary strip — the
    // render edges then triple up after seam quantization and the closed
    // mesh goes non-manifold (the F0046 class). In the unrolled 2D domain a
    // valid triangulation is a planar subdivision: every non-sliver work
    // triangle has the SAME orientation sign. Mixed signs ⇒ a fold. Loud
    // failure beats silently-wrong render output (P9). Sub-resolution
    // slivers are excluded with the scale-relative band (KV8b pattern).
    {
        let mut max_c = 0.0_f64;
        for wn in &wnodes {
            max_c = max_c.max(wn.p2.x().abs()).max(wn.p2.y().abs());
        }
        let area_eps = 1e-12 * (1.0 + max_c) * (1.0 + max_c);
        let (mut pos_n, mut neg_n) = (0usize, 0usize);
        for t in &wtris {
            let (a2, b2, c2) = (wnodes[t[0]].p2, wnodes[t[1]].p2, wnodes[t[2]].p2);
            let area2 =
                (b2.x() - a2.x()) * (c2.y() - a2.y()) - (b2.y() - a2.y()) * (c2.x() - a2.x());
            if area2 > area_eps {
                pos_n += 1;
            } else if area2 < -area_eps {
                neg_n += 1;
            }
        }
        if pos_n > 0 && neg_n > 0 {
            if let Some(prefix) = &census_prefix {
                eprintln!("{prefix} fold=mixed2d");
            }
            return Err(fail(
                "patch triangulation folded (mixed 2D orientation) — KV7-F1/KV9-F2: \
                 the unrolled ear-clip/refinement self-overlapped; loud instead of \
                 silently-wrong render output",
            ));
        }
    }
    if let Some(prefix) = &census_prefix {
        eprintln!("{prefix} fold=0");
    }
    out.face_ranges.push(FaceRange {
        face: fid,
        start: range_start,
        count: out.indices.len() as u32 - range_start,
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 30° half-angle cone, the R0017 anchor's shape.
    fn cone(r_unroll: f64) -> IsoDev {
        IsoDev::new(
            &DevSurface::Cone {
                tan_half_angle: (30.0f64).to_radians().tan(),
            },
            r_unroll,
        )
    }

    fn chart_dist2(a: Point2, b: Point2) -> f64 {
        let (dx, dy) = (a.x() - b.x(), a.y() - b.y());
        dx * dx + dy * dy
    }

    #[test]
    fn cylinder_chart_is_already_isometric() {
        // The (θ·r, h) chart of a cylinder IS its isometric development, so
        // the surface metric must be the plain chart one, exactly.
        let iso = IsoDev::new(&DevSurface::Cylinder { radius: 7.0 }, 7.0);
        assert!(!iso.active);
        let (a, b) = (Point2::new(-3.0, 11.0), Point2::new(5.0, 2.0));
        assert_eq!(iso.dist2(a, b), chart_dist2(a, b));
    }

    #[test]
    fn cone_same_generator_distance_is_slant_length() {
        // Two points on one generator (equal θ) are `Δv / cos α` apart on the
        // surface — the chart, which measures |Δv|, understates by 1/cos α.
        let iso = cone(4000.0);
        let (a, b) = (Point2::new(120.0, 3000.0), Point2::new(120.0, 5000.0));
        let want = 2000.0 / (30.0f64).to_radians().cos();
        let got = iso.dist2(a, b).sqrt();
        assert!((got - want).abs() < 1e-9, "got {got} want {want}");
        assert!(iso.dist2(a, b) > chart_dist2(a, b));
    }

    #[test]
    fn cone_same_height_short_arc_matches_circumferential_arc_length() {
        // At height v the on-surface radius is v·tanα, so a small Δθ spans an
        // arc of v·tanα·Δθ — while the chart calls it Δθ·r_unroll. The two
        // agree only where v·tanα == r_unroll; this is the varying u-scale
        // that makes the working chart non-isometric on a cone.
        let r_unroll = 4000.0;
        let iso = cone(r_unroll);
        let (v, dtheta) = (5000.0, 1.0e-4);
        let du = dtheta * r_unroll;
        let (a, b) = (Point2::new(0.0, v), Point2::new(du, v));
        let want = v * (30.0f64).to_radians().tan() * dtheta;
        let got = iso.dist2(a, b).sqrt();
        assert!((got - want).abs() < 1e-6 * want, "got {got} want {want}");
        // The chart overstates here: v·tanα (2886.8) < r_unroll (4000).
        assert!(got < chart_dist2(a, b).sqrt());
    }

    #[test]
    fn cone_distance_is_wrap_safe_beyond_half_a_turn() {
        // A patch unwrapped across the seam has |u| > π·r_unroll, so the
        // developed angle φ leaves (−π, π]. Working from the RELATIVE Δφ keeps
        // the metric continuous there: translating both points by a whole
        // extra turn must not change the distance.
        let r_unroll = 4000.0;
        let iso = cone(r_unroll);
        let turn = 2.0 * std::f64::consts::PI * r_unroll;
        let (a, b) = (Point2::new(100.0, 5000.0), Point2::new(900.0, 5200.0));
        let (a2, b2) = (
            Point2::new(a.x() + turn, a.y()),
            Point2::new(b.x() + turn, b.y()),
        );
        assert!((iso.dist2(a, b) - iso.dist2(a2, b2)).abs() < 1e-6);
    }

    #[test]
    fn cone_falls_back_to_the_chart_at_or_behind_the_apex() {
        // ρ ≤ 0 is the apex/other nappe: no usable polar development, so the
        // metric degrades to exactly the chart one rather than yielding a NaN.
        let iso = cone(4000.0);
        let (a, b) = (Point2::new(100.0, -50.0), Point2::new(900.0, 5200.0));
        assert_eq!(iso.dist2(a, b), chart_dist2(a, b));
    }

    #[test]
    fn degenerate_cone_falls_back_to_the_chart() {
        // α → 0 has no developed angle at all (dphi_du == 0).
        let iso = IsoDev::new(
            &DevSurface::Cone {
                tan_half_angle: 0.0,
            },
            4000.0,
        );
        assert!(!iso.active);
        let (a, b) = (Point2::new(1.0, 2.0), Point2::new(4.0, 6.0));
        assert_eq!(iso.dist2(a, b), 25.0);
    }
}
