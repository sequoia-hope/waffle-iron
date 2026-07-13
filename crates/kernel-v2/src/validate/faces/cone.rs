//! `Surface::Cone` face validators (move-only F9 split from `validate/faces.rs`;
//! byte-identical): the frustum/apex rim-geometry gate and the arc-patch cone
//! validator. See `super`'s per-surface validator docs.

use super::*;

/// Validate a [`Surface::Cone`] face (KV6c increment 1).
///
/// Structurally the curved analog of [`validate_cylinder_face`], but the
/// full-circle rims sit at DIFFERENT radii — each rim radius must equal
/// `τ · tan(half_angle)` for its axial coordinate `τ = (center − apex) ·
/// axis_dir` (the on-cone relation, [`geom::cone_radius_at`]). Two accepted
/// forms: the canonical FRUSTUM band (exactly two full-circle rims) and the
/// KV6-slice-2B APEX form (a single closed base rim; the apex is an interior
/// singular point, and only the outward solid sense has a producer). No
/// inner loops, no arc edges — arc-patch cones (boolean output) reject
/// loudly here and land in a later increment.
///
/// Orientation: rim circle normals run along the axis, and each rim's
/// traversal axis points TOWARD the opposite rim for an outward (solid)
/// frustum (`reversed == false`) — the same material-sense convention as the
/// cylinder lateral, so the swept frustum built by the KV6c revolve
/// (increment 4) validates by the same rule the cylinder sweep already obeys.
#[allow(clippy::too_many_arguments)]
pub(crate) fn validate_cone_face(
    arena: &BrepArena,
    f: FaceId,
    face: &crate::arena::Face,
    apex: Point3,
    axis_dir: crate::arena::UnitVector3,
    half_angle: f64,
    reversed: bool,
) -> Result<(), KernelV2Error> {
    let mismatch = |reason: &'static str| KernelV2Error::CurvedGeometryMismatch { face: f, reason };
    if !half_angle.is_finite() || half_angle <= 0.0 || half_angle >= std::f64::consts::FRAC_PI_2 {
        return Err(mismatch("cone half_angle must be finite in (0, π/2)"));
    }
    let alen = (axis_dir.x * axis_dir.x + axis_dir.y * axis_dir.y + axis_dir.z * axis_dir.z).sqrt();
    if (alen - 1.0).abs() > NORMAL_AGREEMENT_TOLERANCE {
        return Err(mismatch("cone axis_dir must be unit-length"));
    }

    // Vocabulary dispatch (KV6c increment 5, mirroring the cylinder): any
    // full-circle edge anywhere → the canonical frustum-band / apex forms
    // below; NO full-circle edge → the partial arc-bounded patch (the
    // partial-revolve oblique wall).
    let mut all_loops = vec![face.outer_loop];
    all_loops.extend(face.inner_loops.iter().copied());
    let mut has_full = false;
    for &lid in &all_loops {
        if !loop_circles(arena, &arena.loop_half_edges(lid)?)?.is_empty() {
            has_full = true;
        }
    }
    if !has_full {
        return validate_cone_patch(arena, f, face, apex, axis_dir, half_angle, reversed);
    }

    // Canonical frustum band only (increment 1).
    if !face.inner_loops.is_empty() {
        return Err(mismatch(
            "cone face with inner loops is outside the KV6c vocabulary",
        ));
    }
    let hes = arena.loop_half_edges(face.outer_loop)?;
    if !loop_arcs(arena, &hes)?.is_empty() {
        return Err(mismatch("cone face mixes full-circle rims with arc edges"));
    }
    let rims = loop_circles(arena, &hes)?;

    // Axial coordinate τ = (center − apex) · axis_dir.
    let tau = |c: Point3| {
        (c.x() - apex.x()) * axis_dir.x
            + (c.y() - apex.y()) * axis_dir.y
            + (c.z() - apex.z()) * axis_dir.z
    };

    // KV6 slice 2B: the APEX form — a single closed base rim, the apex an
    // interior singular point (yang's own cone model). Only the outward
    // solid sense has a producer (`build_on_axis_apex_cone`); a `reversed`
    // apex cavity is outside the vocabulary, typed.
    let apex_form = rims.len() == 1 && hes.len() == 1;
    if apex_form {
        if reversed {
            return Err(mismatch(
                "apex-cone cavity (reversed) is outside the KV6c vocabulary",
            ));
        }
    } else if rims.len() != 2 {
        return Err(mismatch(
            "cone face must be bounded by exactly two full-circle rims (KV6c)",
        ));
    }
    for (i, &(c, nu, r)) in rims.iter().enumerate() {
        let t = tau(c);
        if !t.is_finite() || t <= 0.0 {
            return Err(mismatch("cone rim lies at or behind the apex"));
        }
        let expected = geom::cone_radius_at(t, half_angle);
        if (r - expected).abs() > 1e-9 * expected.max(1.0) {
            return Err(mismatch(
                "rim circle radius disagrees with the cone surface",
            ));
        }
        if geom::dot(nu, axis_dir).abs() < 1.0 - NORMAL_AGREEMENT_TOLERANCE {
            return Err(mismatch("rim circle normal must be along the cone axis"));
        }
        if apex_form {
            // Material sense: the base rim's traversal axis points TOWARD
            // the apex (τ decreasing) — the apex analog of "toward the
            // opposite rim".
            if geom::dot(nu, axis_dir) >= 0.0 {
                return Err(mismatch(
                    "rim traversal axis disagrees with the apex cone's material sense",
                ));
            }
            continue;
        }
        let other = rims[1 - i].0;
        let toward =
            (other.x() - c.x()) * nu.x + (other.y() - c.y()) * nu.y + (other.z() - c.z()) * nu.z;
        // Outward frustum: each rim's traversal axis points TOWARD the
        // opposite rim; cavity (reversed) bore wall: AWAY (see the cylinder
        // analog in `validate_cylinder_face`).
        if (!reversed && toward <= 0.0) || (reversed && toward >= 0.0) {
            return Err(mismatch(
                "rim traversal axis disagrees with the cone's material sense",
            ));
        }
    }

    #[cfg(debug_assertions)]
    {
        let on_cone_residual = |p: Point3| {
            let d = [p.x() - apex.x(), p.y() - apex.y(), p.z() - apex.z()];
            let t = d[0] * axis_dir.x + d[1] * axis_dir.y + d[2] * axis_dir.z;
            let radial = [
                d[0] - t * axis_dir.x,
                d[1] - t * axis_dir.y,
                d[2] - t * axis_dir.z,
            ];
            let rho =
                (radial[0] * radial[0] + radial[1] * radial[1] + radial[2] * radial[2]).sqrt();
            (rho - geom::cone_radius_at(t, half_angle)).abs()
        };
        for p in arena.loop_points(face.outer_loop)? {
            if on_cone_residual(p) > CURVED_SURFACE_DEBUG_TOLERANCE {
                return Err(vertex_off_surface(
                    f,
                    "cone-vertex",
                    p,
                    on_cone_residual(p),
                    CURVED_SURFACE_DEBUG_TOLERANCE,
                    &format!(
                        "cone apex=({:.17e},{:.17e},{:.17e}) half_angle={half_angle:.17e}",
                        apex.x(),
                        apex.y(),
                        apex.z()
                    ),
                ));
            }
        }
    }
    Ok(())
}

/// Invariants 4+5 for a PARTIAL cone patch (KV6c increment 5, spec
/// `kv6c_partial_revolve_cone_patch.md` §3 I3): boundary loops of
/// [`Curve::Arc`] and [`Curve::LineSegment`] edges — the partial-revolve
/// oblique wall and, later, yang boolean outputs. This is
/// [`validate_cylinder_patch`]'s unrolled-winding orientation analysis in
/// the cone's (θ, τ) development (a cone is developable; the same
/// material-CCW Newell generalization applies): τ = (p − apex) · axis_dir
/// replaces the axial height, and per-arc surface agreement checks the
/// on-cone relation `r_arc = τ_c · tan(half_angle)` instead of a constant
/// radius. Ellipse arcs (oblique cone sections) are outside this slice —
/// typed and loud.
#[allow(clippy::too_many_arguments)]
fn validate_cone_patch(
    arena: &BrepArena,
    f: FaceId,
    face: &crate::arena::Face,
    apex: Point3,
    axis_dir: crate::arena::UnitVector3,
    half_angle: f64,
    reversed: bool,
) -> Result<(), KernelV2Error> {
    use std::f64::consts::PI;
    let mismatch = |reason: &'static str| KernelV2Error::CurvedGeometryMismatch { face: f, reason };
    let a = [axis_dir.x, axis_dir.y, axis_dir.z];
    let ap = [apex.x(), apex.y(), apex.z()];
    // The mirror sense: a conical bore wall (reversed) is validated in the
    // mirrored frame u = −θ, where its boundary winds material-CCW again.
    let sense = if reversed { -1.0 } else { 1.0 };

    let mut all_loops = vec![face.outer_loop];
    all_loops.extend(face.inner_loops.iter().copied());

    // Shared angular frame anchored at the first outer-loop vertex's radial
    // direction; returns (θ, τ) with τ the axial coordinate FROM THE APEX.
    let radial_theta_tau = |p: Point3, e1: [f64; 3], e2: [f64; 3]| -> Option<(f64, f64)> {
        let d = [p.x() - ap[0], p.y() - ap[1], p.z() - ap[2]];
        let tau = d[0] * a[0] + d[1] * a[1] + d[2] * a[2];
        let r = [d[0] - tau * a[0], d[1] - tau * a[1], d[2] - tau * a[2]];
        let rl = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt();
        if !(rl.is_finite() && rl > 0.0) {
            return None;
        }
        let x = r[0] * e1[0] + r[1] * e1[1] + r[2] * e1[2];
        let y = r[0] * e2[0] + r[1] * e2[1] + r[2] * e2[2];
        Some((y.atan2(x), tau))
    };
    let first_hes = arena.loop_half_edges(face.outer_loop)?;
    if first_hes.is_empty() {
        return Err(mismatch("cone patch with an empty boundary loop"));
    }
    let p0 = arena.vertex(arena.half_edge(first_hes[0])?.origin)?.point;
    let d0 = [p0.x() - ap[0], p0.y() - ap[1], p0.z() - ap[2]];
    let t0 = d0[0] * a[0] + d0[1] * a[1] + d0[2] * a[2];
    let r0 = [d0[0] - t0 * a[0], d0[1] - t0 * a[1], d0[2] - t0 * a[2]];
    let r0l = (r0[0] * r0[0] + r0[1] * r0[1] + r0[2] * r0[2]).sqrt();
    if !(r0l.is_finite() && r0l > 0.0) {
        return Err(mismatch("cone patch anchor vertex lies on the axis"));
    }
    let e1 = [r0[0] / r0l, r0[1] / r0l, r0[2] / r0l];
    let e2 = [
        a[1] * e1[2] - a[2] * e1[1],
        a[2] * e1[0] - a[0] * e1[2],
        a[0] * e1[1] - a[1] * e1[0],
    ];

    let mut measures: Vec<LoopMeasure> = Vec::with_capacity(all_loops.len());
    for &lid in &all_loops {
        let hes = arena.loop_half_edges(lid)?;
        if hes.len() < 3 {
            return Err(mismatch("cone patch loop with fewer than 3 edges"));
        }
        let mut us: Vec<f64> = Vec::with_capacity(hes.len());
        let mut vs: Vec<f64> = Vec::with_capacity(hes.len());
        let mut u_cur = f64::NAN; // set from the first vertex below
        let mut total = 0.0f64;
        for (i, &h) in hes.iter().enumerate() {
            let he = arena.half_edge(h)?;
            let p = arena.vertex(he.origin)?.point;
            let q = arena.vertex(arena.half_edge(he.next)?.origin)?.point;
            let Some((theta_p, tau_p)) = radial_theta_tau(p, e1, e2) else {
                return Err(mismatch("cone patch vertex lies on the axis"));
            };
            if i == 0 {
                u_cur = theta_p;
            }
            us.push(u_cur);
            vs.push(tau_p);

            let delta = match he.curve {
                Curve::LineSegment => {
                    let Some((theta_q, _)) = radial_theta_tau(q, e1, e2) else {
                        return Err(mismatch("cone patch vertex lies on the axis"));
                    };
                    geom::wrap_to_pi(theta_q - theta_p)
                }
                Curve::Arc {
                    center,
                    normal,
                    radius: r_arc,
                } => {
                    // Production-tier per-arc surface agreement: axis-parallel
                    // circle at τ_c > 0 whose radius satisfies the on-cone
                    // relation r = τ_c · tan(half_angle).
                    let nd = geom::dot(normal, axis_dir);
                    if nd.abs() < 1.0 - NORMAL_AGREEMENT_TOLERANCE {
                        return Err(mismatch(
                            "patch arc's circle axis is not parallel to the cone axis",
                        ));
                    }
                    let tau_c = (center.x() - ap[0]) * a[0]
                        + (center.y() - ap[1]) * a[1]
                        + (center.z() - ap[2]) * a[2];
                    if !(tau_c.is_finite() && tau_c > 0.0) {
                        return Err(mismatch("patch arc lies at or behind the apex"));
                    }
                    let expected = geom::cone_radius_at(tau_c, half_angle);
                    if (r_arc - expected).abs() > 1e-9 * expected.max(1.0) {
                        return Err(mismatch("patch arc radius disagrees with the cone surface"));
                    }
                    #[cfg(debug_assertions)]
                    {
                        // Arc center on the axis (import band — see
                        // `validate_cylinder_patch`'s arc-center check).
                        let dc = [center.x() - ap[0], center.y() - ap[1], center.z() - ap[2]];
                        let hc = dc[0] * a[0] + dc[1] * a[1] + dc[2] * a[2];
                        let rc = [dc[0] - hc * a[0], dc[1] - hc * a[1], dc[2] - hc * a[2]];
                        let off = (rc[0] * rc[0] + rc[1] * rc[1] + rc[2] * rc[2]).sqrt();
                        if off > import_band(r_arc, center) {
                            return Err(vertex_off_surface(
                                f,
                                "conepatch-arc-center-off-axis",
                                center,
                                off,
                                import_band(r_arc, center),
                                &format!(
                                    "cone apex=({:.17e},{:.17e},{:.17e}) \
                                     axis=({:.17e},{:.17e},{:.17e}) half_angle={half_angle:.17e}",
                                    ap[0], ap[1], ap[2], a[0], a[1], a[2]
                                ),
                            ));
                        }
                    }
                    let n_arr = [normal.x, normal.y, normal.z];
                    let Some(sweep) = geom::ccw_sweep(center, n_arr, p, q) else {
                        return Err(mismatch("patch arc endpoint has no radial direction"));
                    };
                    if nd > 0.0 {
                        sweep
                    } else {
                        -sweep
                    }
                }
                // KV16: a conic section piece ON this cone (EllipseArc =
                // the oblique plane∩cone section, HyperbolaArc = the
                // axis-steep one) advances the walk by its endpoint
                // azimuths, exactly like the SurfacePair arm below —
                // boolean-output pieces are sub-facet sized (Δθ far below
                // π). A cone-section ellipse has no constant-radius axis-⊥
                // projection (unlike the cylinder-section ellipse above),
                // so the parametric-sweep shortcut does NOT apply — the
                // endpoint-azimuth walk is the honest advance.
                Curve::EllipseArc { .. } | Curve::HyperbolaArc { .. } => {
                    let Some((theta_q, _)) = radial_theta_tau(q, e1, e2) else {
                        return Err(mismatch("cone patch vertex lies on the axis"));
                    };
                    geom::wrap_to_pi(theta_q - theta_p)
                }
                Curve::Circle { .. } => {
                    // Unreachable: the dispatcher sends full-circle faces to
                    // the canonical path. Loud, defensively.
                    return Err(mismatch("full-circle edge inside a partial cone patch"));
                }
                // M5: a surface-pair boundary piece advances the walk by its
                // endpoint azimuths (endpoint-determined traversal; the
                // on-surface certification is invariant 1b, and the endpoint
                // residual against THIS face's surface is the shared
                // off-surface sweep below).
                Curve::SurfacePair { .. } => {
                    let Some((theta_q, _)) = radial_theta_tau(q, e1, e2) else {
                        return Err(mismatch("cone patch vertex lies on the axis"));
                    };
                    geom::wrap_to_pi(theta_q - theta_p)
                }
            };
            u_cur += delta;
            total += delta;
        }
        let wraps_f = total / (2.0 * PI);
        let wraps = wraps_f.round();
        if (wraps_f - wraps).abs() > 1e-3 {
            return Err(mismatch(
                "cone patch loop's net axis winding is not integral",
            ));
        }
        let wraps = wraps as i64;
        if wraps.abs() > 1 {
            return Err(mismatch("cone patch loop wraps the axis more than once"));
        }
        let m = us.len();
        let mut area2 = 0.0f64;
        for i in 0..m {
            let j = (i + 1) % m;
            area2 += us[i] * vs[j] - us[j] * vs[i];
        }
        measures.push(LoopMeasure {
            loop_id: lid,
            wrap: if sense < 0.0 { -wraps } else { wraps },
            mean_h: vs.iter().sum::<f64>() / m as f64,
            area2: sense * area2,
        });
    }

    // ---- face-level orientation rules (material-CCW in the developed frame)
    let wrapping: Vec<&LoopMeasure> = measures.iter().filter(|mm| mm.wrap != 0).collect();
    match wrapping.len() {
        0 => {
            // Bounded patch: exactly one CCW (material) loop, others CW
            // (windows).
            let mut positive = 0usize;
            for mm in &measures {
                if mm.area2 == 0.0 {
                    return Err(mismatch("cone patch loop has zero developed area"));
                }
                if mm.area2 > 0.0 {
                    positive += 1;
                }
            }
            if positive != 1 {
                return Err(mismatch(
                    "bounded cone patch must have exactly one material-CCW loop",
                ));
            }
        }
        2 => {
            // Band segment: a +1 and a −1 wrap, the +1 at the lower axial
            // coordinate (the cylinder barrel rule with τ for height);
            // windows wind CW.
            let (w0, w1) = (wrapping[0], wrapping[1]);
            if w0.wrap + w1.wrap != 0 {
                return Err(mismatch("cone patch wrapping loops do not wind oppositely"));
            }
            let (plus, minus) = if w0.wrap > 0 { (w0, w1) } else { (w1, w0) };
            if plus.mean_h >= minus.mean_h {
                return Err(mismatch(
                    "cone patch wrapping loops are oriented away from the material",
                ));
            }
            for mm in &measures {
                if mm.wrap == 0 && mm.area2 >= 0.0 {
                    return Err(KernelV2Error::RingWindingMismatch {
                        face: f,
                        ring: mm.loop_id,
                    });
                }
            }
        }
        _ => {
            return Err(mismatch(
                "cone patch must have exactly 0 or 2 axis-wrapping loops",
            ));
        }
    }

    // ---- debug-tier geometric tripwire: loop vertices on the surface ------
    #[cfg(debug_assertions)]
    {
        for &lid in &all_loops {
            for p in arena.loop_points(lid)? {
                let d = [p.x() - ap[0], p.y() - ap[1], p.z() - ap[2]];
                let tau = d[0] * a[0] + d[1] * a[1] + d[2] * a[2];
                let r = [d[0] - tau * a[0], d[1] - tau * a[1], d[2] - tau * a[2]];
                let rl = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt();
                let expected = geom::cone_radius_at(tau.max(0.0), half_angle);
                if (rl - expected).abs() > import_band(expected.max(rl), p) {
                    return Err(vertex_off_surface(
                        f,
                        "conepatch-vertex",
                        p,
                        (rl - expected).abs(),
                        import_band(expected.max(rl), p),
                        &format!(
                            "cone apex=({:.17e},{:.17e},{:.17e}) \
                             axis=({:.17e},{:.17e},{:.17e}) half_angle={half_angle:.17e}",
                            ap[0], ap[1], ap[2], a[0], a[1], a[2]
                        ),
                    ));
                }
            }
        }
    }
    Ok(())
}
