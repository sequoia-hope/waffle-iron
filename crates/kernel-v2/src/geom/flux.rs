//! Divergence-theorem flux integrators for curved patches (move-only F9 split
//! from `geom.rs`; byte-identical): the cylinder and cone arc-patch flux terms
//! that `super::signed_volume` sums. See `super`'s module docs for the
//! both-senses cancellation argument.

use super::*;

/// Divergence-theorem flux through a CYLINDER patch whose loops consist of
/// on-surface sweep arcs (circle axis ∥ cylinder axis) and axis-parallel
/// ruling segments — the revolve lateral shape (PR-KV6a).
///
/// Derivation: on the surface `x = a₀ + h·â + ρ·r̂(θ)` and the outward
/// normal is `σ·r̂(θ)` (σ = −1 for cavity walls), so
/// `x·n = σ(a₀·r̂ + ρ)` and `flux = (σρ/3) ∬ (ρ + a₀·r̂) dθ dh` over the
/// unrolled region. Green's theorem turns the region integral into the
/// loop integral `∮ −g(θ)·h dθ` (`g = ρ + a₀·r̂`), to which rulings
/// contribute nothing and each arc at height `h` contributes
/// `−h·(ρ·Δθ + a₀·(t̂_start − t̂_end))` with `t̂ = â × r̂` and `Δθ` signed
/// by the arc's traversal sense about `+â`. The boundary's material-CCW
/// orientation (mirrored for cavity walls) cancels σ, so the flux is
/// `(ρ/3)·Σ_arcs` for BOTH senses. Segments that are not rulings (boolean
/// chord facets) are rejected loudly.
pub(crate) fn cylinder_arc_patch_flux(
    arena: &crate::arena::BrepArena,
    f: crate::arena::FaceId,
    face: &crate::arena::Face,
    axis_point: Point3,
    axis_dir: crate::arena::UnitVector3,
    radius: f64,
) -> Result<f64, crate::error::KernelV2Error> {
    use crate::arena::Curve;
    let a = [axis_dir.x, axis_dir.y, axis_dir.z];
    let a0 = [axis_point.x(), axis_point.y(), axis_point.z()];
    let mismatch = |reason: &'static str| crate::error::KernelV2Error::CurvedGeometryMismatch {
        face: f,
        reason,
    };

    let mut loops = vec![face.outer_loop];
    loops.extend(face.inner_loops.iter().copied());
    let mut sum = 0.0f64;
    for lid in loops {
        let hes = arena.loop_half_edges(lid)?;
        for &h in &hes {
            let he = arena.half_edge(h)?;
            let p0 = arena.vertex(he.origin)?.point;
            let p1 = arena.vertex(arena.half_edge(he.next)?.origin)?.point;
            match he.curve {
                Curve::LineSegment => {
                    // Must be a ruling (no angular extent), or the Green's
                    // bookkeeping above would silently miss its dθ.
                    let dvec = [p1.x() - p0.x(), p1.y() - p0.y(), p1.z() - p0.z()];
                    let cx = [
                        dvec[1] * a[2] - dvec[2] * a[1],
                        dvec[2] * a[0] - dvec[0] * a[2],
                        dvec[0] * a[1] - dvec[1] * a[0],
                    ];
                    let len = (dvec[0] * dvec[0] + dvec[1] * dvec[1] + dvec[2] * dvec[2]).sqrt();
                    let off = (cx[0] * cx[0] + cx[1] * cx[1] + cx[2] * cx[2]).sqrt();
                    if off > 1e-9 * (1.0 + len) {
                        return Err(mismatch(
                            "signed_volume: cylinder-patch segment is not a ruling \
                             (boolean chord facets have no closed form)",
                        ));
                    }
                }
                Curve::Arc {
                    center,
                    normal,
                    radius: r_arc,
                } => {
                    let nu = [normal.x, normal.y, normal.z];
                    let along = nu[0] * a[0] + nu[1] * a[1] + nu[2] * a[2];
                    if along.abs() < 1.0 - 1e-9 {
                        return Err(mismatch(
                            "signed_volume: cylinder-patch arc axis not along the cylinder axis",
                        ));
                    }
                    let Some(sweep) = ccw_sweep(center, nu, p0, p1) else {
                        return Err(mismatch("signed_volume: degenerate arc endpoints"));
                    };
                    // Signed Δθ about +â.
                    let dtheta = if along >= 0.0 { sweep } else { -sweep };
                    let h = (center.x() - a0[0]) * a[0]
                        + (center.y() - a0[1]) * a[1]
                        + (center.z() - a0[2]) * a[2];
                    // t̂ = â × r̂ at each endpoint.
                    let t_hat = |p: Point3| {
                        let r = [
                            (p.x() - center.x()) / r_arc,
                            (p.y() - center.y()) / r_arc,
                            (p.z() - center.z()) / r_arc,
                        ];
                        [
                            a[1] * r[2] - a[2] * r[1],
                            a[2] * r[0] - a[0] * r[2],
                            a[0] * r[1] - a[1] * r[0],
                        ]
                    };
                    let ts = t_hat(p0);
                    let te = t_hat(p1);
                    let a0_dot =
                        a0[0] * (ts[0] - te[0]) + a0[1] * (ts[1] - te[1]) + a0[2] * (ts[2] - te[2]);
                    sum += -h * (radius * dtheta + a0_dot);
                }
                Curve::EllipseArc {
                    center,
                    normal,
                    major_axis,
                    major_radius,
                    minor_radius,
                } => {
                    // PR-KV9: an oblique-plane section arc ON this cylinder.
                    // For a cylinder section the axis-⊥ projection of the
                    // ellipse is the radius-r circle itself, so the ellipse
                    // parameter t IS the azimuth (up to frame handedness):
                    // with ê1 = unit(m̂ − (m̂·â)â), ê2 = â × ê1 and
                    // ŵ = n̂×m̂ (the stored frame's minor direction),
                    //   r̂(t) = cos t·ê1 + s_w·sin t·ê2,  s_w = sign(ŵ·ê2),
                    //   h(t) = h_c + k·cos t,             k = a·(m̂·â),
                    //   g(t) = ρ + p·cos t + q·s_w·sin t, p = a₀·ê1, q = a₀·ê2.
                    // The Green's-theorem loop term −∮ g·h dθ (dθ = s_w·dt)
                    // expands into elementary integrals; the antiderivative
                    //   F(t) = ρh_c·t + (ρk + p·h_c)·sin t − q·s_w·h_c·cos t
                    //          + p·k·(t/2 + sin 2t/4) − q·s_w·k·cos 2t/4
                    // gives the contribution −s_w·(F(t₁) − F(t₀)). The
                    // circle-arc branch above is the k = 0 special case
                    // (verified to agree term-for-term).
                    let mr = [major_axis.x, major_axis.y, major_axis.z];
                    let nu = [normal.x, normal.y, normal.z];
                    // Section-of-THIS-cylinder preconditions (loud).
                    if (minor_radius - radius).abs() > 1e-9 * (1.0 + radius) {
                        return Err(mismatch(
                            "signed_volume: ellipse-arc minor radius is not the cylinder radius",
                        ));
                    }
                    let c_rel = [center.x() - a0[0], center.y() - a0[1], center.z() - a0[2]];
                    let h_c = c_rel[0] * a[0] + c_rel[1] * a[1] + c_rel[2] * a[2];
                    let c_perp = [
                        c_rel[0] - h_c * a[0],
                        c_rel[1] - h_c * a[1],
                        c_rel[2] - h_c * a[2],
                    ];
                    if (c_perp[0] * c_perp[0] + c_perp[1] * c_perp[1] + c_perp[2] * c_perp[2])
                        .sqrt()
                        > 1e-9 * (1.0 + radius)
                    {
                        return Err(mismatch(
                            "signed_volume: ellipse-arc center is off the cylinder axis",
                        ));
                    }
                    let m_dot_a = mr[0] * a[0] + mr[1] * a[1] + mr[2] * a[2];
                    let e1_raw = [
                        mr[0] - m_dot_a * a[0],
                        mr[1] - m_dot_a * a[1],
                        mr[2] - m_dot_a * a[2],
                    ];
                    let e1_len =
                        (e1_raw[0] * e1_raw[0] + e1_raw[1] * e1_raw[1] + e1_raw[2] * e1_raw[2])
                            .sqrt();
                    if e1_len < 1e-12 {
                        return Err(mismatch(
                            "signed_volume: ellipse-arc major axis parallel to the cylinder axis",
                        ));
                    }
                    let e1 = [e1_raw[0] / e1_len, e1_raw[1] / e1_len, e1_raw[2] / e1_len];
                    let e2 = [
                        a[1] * e1[2] - a[2] * e1[1],
                        a[2] * e1[0] - a[0] * e1[2],
                        a[0] * e1[1] - a[1] * e1[0],
                    ];
                    let w = [
                        nu[1] * mr[2] - nu[2] * mr[1],
                        nu[2] * mr[0] - nu[0] * mr[2],
                        nu[0] * mr[1] - nu[1] * mr[0],
                    ];
                    let w_dot_a = w[0] * a[0] + w[1] * a[1] + w[2] * a[2];
                    if w_dot_a.abs() > 1e-9 {
                        return Err(mismatch(
                            "signed_volume: ellipse-arc minor axis not perpendicular to the                              cylinder axis",
                        ));
                    }
                    let s_w = if w[0] * e2[0] + w[1] * e2[1] + w[2] * e2[2] >= 0.0 {
                        1.0
                    } else {
                        -1.0
                    };
                    let Some(t0) = ellipse_param(center, nu, mr, major_radius, minor_radius, p0)
                    else {
                        return Err(mismatch("signed_volume: degenerate ellipse-arc endpoint"));
                    };
                    let Some(sweep) =
                        ellipse_ccw_sweep(center, nu, mr, major_radius, minor_radius, p0, p1)
                    else {
                        return Err(mismatch("signed_volume: degenerate ellipse-arc endpoints"));
                    };
                    let t1 = t0 + sweep;
                    let k = major_radius * m_dot_a;
                    let p_c = a0[0] * e1[0] + a0[1] * e1[1] + a0[2] * e1[2];
                    let q_c = a0[0] * e2[0] + a0[1] * e2[1] + a0[2] * e2[2];
                    let fterm = |t: f64| -> f64 {
                        radius * h_c * t + (radius * k + p_c * h_c) * t.sin()
                            - q_c * s_w * h_c * t.cos()
                            + p_c * k * (t / 2.0 + (2.0 * t).sin() / 4.0)
                            - q_c * s_w * k * (2.0 * t).cos() / 4.0
                    };
                    sum += -s_w * (fterm(t1) - fterm(t0));
                }
                Curve::Circle { .. } => {
                    return Err(mismatch(
                        "signed_volume: cylinder patch mixes full circles with arcs",
                    ));
                }
                // KV16: a plane∩cylinder section is never a hyperbola — its
                // presence on a cylinder patch is a defect, not a missing
                // closed form.
                Curve::HyperbolaArc { .. } => {
                    return Err(mismatch(
                        "signed_volume: hyperbola arc on a cylinder patch (a plane∩cylinder \
                         section is never a hyperbola)",
                    ));
                }
                // M5: the degree-4 surface-pair boundary has NO closed-form
                // flux (that is the point of the procedural representation)
                // — loud, never a chord-polyline approximation (P9).
                Curve::SurfacePair { .. } => {
                    return Err(mismatch(
                        "signed_volume: surface-pair (degree-4) patch boundary has no \
                         closed form",
                    ));
                }
            }
        }
    }
    Ok(radius * sum / 3.0)
}

/// Divergence-theorem flux through a CONE patch whose loops consist of
/// on-surface sweep arcs (circle axis ∥ cone axis, center at τ > 0) and
/// slant ruling segments — the partial-revolve oblique-wall shape (KV6c
/// increment 5, spec `kv6c_partial_revolve_cone_patch.md` §6).
///
/// Derivation: on the surface `x = apex + τ·â + τ·tan α·r̂(θ)` with outward
/// normal `σ·(cos α·r̂ − sin α·â)`, the position-flux integrand is
/// τ-INDEPENDENT: `x·n̂ = σ·(cos α·(apex·r̂) − sin α·(apex·â))` (the τ terms
/// cancel exactly since ρ = τ·tan α). With the area element
/// `dA = τ·tan α/cos α · dθ dτ`, `flux = (σ·tan α/3) ∬ τ·g(θ) dθ dτ`,
/// `g = apex·r̂ − tan α·(apex·â)`. Green's theorem turns the region integral
/// into the loop integral `∮ −g(θ)·τ²/2 dθ`, to which rulings contribute
/// nothing and each arc at axial coordinate `τ_c` contributes
/// `−(τ_c²/2)·(apex·(t̂_start − t̂_end) − tan α·(apex·â)·Δθ)` with
/// `t̂ = â × r̂` and `Δθ` signed by the arc's traversal sense about `+â`.
/// The boundary's material-CCW orientation (mirrored for cavity walls)
/// cancels σ — the same both-senses argument as [`cylinder_arc_patch_flux`].
/// The Δθ = ±2π limit reproduces the full-band closed form
/// `−(π/3)(apex·â)(ρ_hi² − ρ_lo²)` term-for-term. Segments that are not
/// rulings (boolean chord facets) are rejected loudly.
pub(crate) fn cone_arc_patch_flux(
    arena: &crate::arena::BrepArena,
    f: crate::arena::FaceId,
    face: &crate::arena::Face,
    apex: Point3,
    axis_dir: crate::arena::UnitVector3,
    half_angle: f64,
) -> Result<f64, crate::error::KernelV2Error> {
    use crate::arena::Curve;
    let a = [axis_dir.x, axis_dir.y, axis_dir.z];
    let ap = [apex.x(), apex.y(), apex.z()];
    let tan_a = half_angle.tan();
    let apex_dot_axis = ap[0] * a[0] + ap[1] * a[1] + ap[2] * a[2];
    let mismatch = |reason: &'static str| crate::error::KernelV2Error::CurvedGeometryMismatch {
        face: f,
        reason,
    };

    let mut loops = vec![face.outer_loop];
    loops.extend(face.inner_loops.iter().copied());
    let mut sum = 0.0f64;
    for lid in loops {
        let hes = arena.loop_half_edges(lid)?;
        for &h in &hes {
            let he = arena.half_edge(h)?;
            let p0 = arena.vertex(he.origin)?.point;
            let p1 = arena.vertex(arena.half_edge(he.next)?.origin)?.point;
            match he.curve {
                Curve::LineSegment => {
                    // Must be a slant ruling (zero angular extent): the
                    // segment lies in the meridian plane through its start,
                    // i.e. its direction is ⊥ t̂₀ = â × r̂₀. A chord with
                    // angular extent would carry dθ the Green's bookkeeping
                    // above would silently miss.
                    let d0 = [ap[0] - p0.x(), ap[1] - p0.y(), ap[2] - p0.z()];
                    let t0 = -(d0[0] * a[0] + d0[1] * a[1] + d0[2] * a[2]);
                    let r0 = [
                        p0.x() - ap[0] - t0 * a[0],
                        p0.y() - ap[1] - t0 * a[1],
                        p0.z() - ap[2] - t0 * a[2],
                    ];
                    let r0l = (r0[0] * r0[0] + r0[1] * r0[1] + r0[2] * r0[2]).sqrt();
                    if !(r0l.is_finite() && r0l > 0.0) {
                        return Err(mismatch(
                            "signed_volume: cone-patch segment endpoint on the axis",
                        ));
                    }
                    let t_hat0 = [
                        (a[1] * r0[2] - a[2] * r0[1]) / r0l,
                        (a[2] * r0[0] - a[0] * r0[2]) / r0l,
                        (a[0] * r0[1] - a[1] * r0[0]) / r0l,
                    ];
                    let dvec = [p1.x() - p0.x(), p1.y() - p0.y(), p1.z() - p0.z()];
                    let len = (dvec[0] * dvec[0] + dvec[1] * dvec[1] + dvec[2] * dvec[2]).sqrt();
                    let off =
                        (dvec[0] * t_hat0[0] + dvec[1] * t_hat0[1] + dvec[2] * t_hat0[2]).abs();
                    if off > 1e-9 * (1.0 + len) {
                        return Err(mismatch(
                            "signed_volume: cone-patch segment is not a slant ruling \
                             (boolean chord facets have no closed form)",
                        ));
                    }
                }
                Curve::Arc {
                    center,
                    normal,
                    radius: r_arc,
                } => {
                    let nu = [normal.x, normal.y, normal.z];
                    let along = nu[0] * a[0] + nu[1] * a[1] + nu[2] * a[2];
                    if along.abs() < 1.0 - 1e-9 {
                        return Err(mismatch(
                            "signed_volume: cone-patch arc axis not along the cone axis",
                        ));
                    }
                    let tau_c = (center.x() - ap[0]) * a[0]
                        + (center.y() - ap[1]) * a[1]
                        + (center.z() - ap[2]) * a[2];
                    if !(tau_c.is_finite() && tau_c > 0.0) {
                        return Err(mismatch(
                            "signed_volume: cone-patch arc lies at or behind the apex",
                        ));
                    }
                    let Some(sweep) = ccw_sweep(center, nu, p0, p1) else {
                        return Err(mismatch("signed_volume: degenerate arc endpoints"));
                    };
                    // Signed Δθ about +â.
                    let dtheta = if along >= 0.0 { sweep } else { -sweep };
                    // t̂ = â × r̂ at each endpoint.
                    let t_hat = |p: Point3| {
                        let r = [
                            (p.x() - center.x()) / r_arc,
                            (p.y() - center.y()) / r_arc,
                            (p.z() - center.z()) / r_arc,
                        ];
                        [
                            a[1] * r[2] - a[2] * r[1],
                            a[2] * r[0] - a[0] * r[2],
                            a[0] * r[1] - a[1] * r[0],
                        ]
                    };
                    let ts = t_hat(p0);
                    let te = t_hat(p1);
                    let ap_dot =
                        ap[0] * (ts[0] - te[0]) + ap[1] * (ts[1] - te[1]) + ap[2] * (ts[2] - te[2]);
                    sum += -(tau_c * tau_c / 2.0) * (ap_dot - tan_a * apex_dot_axis * dtheta);
                }
                Curve::EllipseArc { .. } => {
                    // An oblique-plane cone section — the conic-bounded cone
                    // patch vocabulary is a later slice (KV6c 5c note).
                    return Err(mismatch(
                        "signed_volume: cone-patch ellipse arcs have no closed form yet \
                         (oblique cone sections)",
                    ));
                }
                // KV16: same conic-bounded-cone-patch wall as EllipseArc —
                // typed and loud; the render mesh carries the volume oracle.
                Curve::HyperbolaArc { .. } => {
                    return Err(mismatch(
                        "signed_volume: cone-patch hyperbola arcs have no closed form yet \
                         (axis-steep cone sections)",
                    ));
                }
                Curve::Circle { .. } => {
                    return Err(mismatch(
                        "signed_volume: cone patch mixes full circles with arcs",
                    ));
                }
                // M5: no closed-form flux for the degree-4 boundary (see the
                // cylinder-patch arm).
                Curve::SurfacePair { .. } => {
                    return Err(mismatch(
                        "signed_volume: surface-pair (degree-4) patch boundary has no \
                         closed form",
                    ));
                }
            }
        }
    }
    Ok(tan_a * sum / 3.0)
}
