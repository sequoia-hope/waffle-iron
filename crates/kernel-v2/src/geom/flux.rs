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

/// Divergence-theorem flux `(1/3)∮ x·n dA` through a TORUS BAND — the
/// bent-tube lateral of `build_torus_revolve` / the pipe (spec
/// `b2_pipe_sweep.md` §3): the outer loop is two closed profile rims
/// (`Curve::Circle`, radius = minor) plus the seam-arc twin pair.
///
/// Derivation: the band, its two disc caps and the tube they bound satisfy
/// `3V = Φ_band + Φ_cap₀ + Φ_cap_α` with `V = α·R·π·r²` (Pappus) and, on a
/// planar cap, `Φ_cap = (c · n_out)·π r²`. The cap's outward normal is the
/// negation of the band's rim directional normal `ν` (the rim traverses
/// TOWARD the opposite rim, i.e. into the tube at that cap), so
/// `Φ_band = 3αRπr² + πr²(c₀·ν₀ + c_α·ν_α)`. For a cavity band
/// (`reversed`, the bore of a hollow pipe) the material sense flips: the
/// Pappus term negates while the rim normals — already reversed by the
/// builder — keep the cap terms' signs, giving `−3αRπr² + πr²(Σ c·ν)`.
/// The sweep `α` is read off the seam arc about the `+axis`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn torus_band_flux(
    arena: &crate::arena::BrepArena,
    f: crate::arena::FaceId,
    face: &crate::arena::Face,
    center: Point3,
    axis_dir: crate::arena::UnitVector3,
    major: f64,
    minor: f64,
    reversed: bool,
) -> Result<f64, crate::error::KernelV2Error> {
    use crate::arena::Curve;
    use std::f64::consts::PI;
    let mismatch = |reason: &'static str| crate::error::KernelV2Error::CurvedGeometryMismatch {
        face: f,
        reason,
    };
    if !face.inner_loops.is_empty() {
        return Err(mismatch(
            "signed_volume: torus band with inner loops has no closed form",
        ));
    }
    let a = [axis_dir.x, axis_dir.y, axis_dir.z];
    let hes = arena.loop_half_edges(face.outer_loop)?;
    let mut rim_dots = 0.0f64;
    let mut rims = 0usize;
    let mut alpha: Option<f64> = None;
    for &h in &hes {
        let he = arena.half_edge(h)?;
        match he.curve {
            Curve::Circle {
                center: c,
                normal,
                radius,
            } => {
                if (radius - minor).abs() > 1e-9 * minor {
                    return Err(mismatch(
                        "signed_volume: torus band rim radius disagrees with the minor radius",
                    ));
                }
                rims += 1;
                rim_dots += c.x() * normal.x + c.y() * normal.y + c.z() * normal.z;
            }
            Curve::Arc {
                center: cs,
                normal,
                radius: _,
            } => {
                let nu = [normal.x, normal.y, normal.z];
                let along = nu[0] * a[0] + nu[1] * a[1] + nu[2] * a[2];
                if along.abs() < 1.0 - 1e-9 {
                    return Err(mismatch(
                        "signed_volume: torus band seam arc is not about the torus axis",
                    ));
                }
                let d = [
                    cs.x() - center.x(),
                    cs.y() - center.y(),
                    cs.z() - center.z(),
                ];
                let tau = d[0] * a[0] + d[1] * a[1] + d[2] * a[2];
                let off = [d[0] - tau * a[0], d[1] - tau * a[1], d[2] - tau * a[2]];
                if (off[0] * off[0] + off[1] * off[1] + off[2] * off[2]).sqrt()
                    > 1e-9 * (1.0 + major + minor)
                {
                    return Err(mismatch(
                        "signed_volume: torus band seam arc is not centred on the axis",
                    ));
                }
                if along > 0.0 && alpha.is_none() {
                    let p0 = arena.vertex(he.origin)?.point;
                    let p1 = arena.vertex(arena.half_edge(he.next)?.origin)?.point;
                    alpha = Some(
                        ccw_sweep(cs, nu, p0, p1)
                            .ok_or(mismatch("signed_volume: degenerate torus seam arc"))?,
                    );
                }
            }
            _ => {
                return Err(mismatch(
                    "signed_volume: torus band loop outside the rim + seam vocabulary",
                ));
            }
        }
    }
    let Some(alpha) = alpha else {
        return Err(mismatch(
            "signed_volume: torus band without a +axis seam arc",
        ));
    };
    if rims != 2 {
        return Err(mismatch(
            "signed_volume: torus band must be bounded by exactly two profile rims",
        ));
    }
    let sigma = if reversed { -1.0 } else { 1.0 };
    let r2 = minor * minor;
    Ok(sigma * alpha * major * PI * r2 + PI * r2 * rim_dots / 3.0)
}

/// Divergence-theorem flux `(1/3)∮ x·n dA` through a torus LATITUDE BAND —
/// SI5 C5a (spec `si5_c5_sphere_torus_tier.md` §3): a torus face between two
/// closed circles COAXIAL with the torus, the fillet around a boss or a hole.
/// The outer loop is the two latitude rims plus the poloidal seam-arc twin
/// pair the ingest path mints; the seam is a fake edge and contributes nothing.
///
/// Derivation. With `x = C + (R + r cos φ) ŵ(θ) + r sin φ â` and the torus's
/// outward normal `n = cos φ ŵ + sin φ â`, `x·n = C·ŵ(θ) cos φ + (C·â) sin φ +
/// R cos φ + r` and `dA = r (R + r cos φ) dθ dφ`. Over the full turn in θ the
/// `C·ŵ` term integrates to zero, so with `G` the antiderivative of
/// `[(C·â) sin φ + R cos φ + r](R + r cos φ)`:
///
/// ```text
/// Φ = ±(2πr/3)·[G(φ_e) − G(φ_s)],
/// G(φ) = (C·â)(−R cos φ + (r/2) sin²φ) + (R² + r²) sin φ
///        + Rr(φ/2 + sin 2φ/4) + Rr φ.
/// ```
///
/// The region is the +φ sweep from the START rim — the one traversed CCW
/// about `+â` (about `−â` when `reversed`), the fact 1d of the ingest path
/// established — to the end rim, `φ_e − φ_s ∈ (0, 2π)`. `reversed` negates
/// the whole flux (the face's outward normal is `−n`). Check: a full turn has
/// `G(2π) − G(0) = 3πRr`, giving the torus volume `2π²Rr²`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn torus_latitude_band_flux(
    arena: &crate::arena::BrepArena,
    f: crate::arena::FaceId,
    face: &crate::arena::Face,
    center: Point3,
    axis_dir: crate::arena::UnitVector3,
    major: f64,
    minor: f64,
    reversed: bool,
) -> Result<f64, crate::error::KernelV2Error> {
    use std::f64::consts::PI;
    let mismatch = |reason: &'static str| crate::error::KernelV2Error::CurvedGeometryMismatch {
        face: f,
        reason,
    };
    if !face.inner_loops.is_empty() {
        return Err(mismatch(
            "signed_volume: torus latitude band with inner loops has no closed form",
        ));
    }
    let a = [axis_dir.x, axis_dir.y, axis_dir.z];
    let (phi_s, phi_e) = torus_latitude_band_phis(arena, face, center, a, major, minor, reversed)
        .ok_or(mismatch(
        "signed_volume: torus latitude band is not two coaxial rims on the torus with \
             opposite senses",
    ))?;
    let c_dot_a = center.x() * a[0] + center.y() * a[1] + center.z() * a[2];
    let g = |phi: f64| {
        let (s, c) = phi.sin_cos();
        c_dot_a * (-major * c + 0.5 * minor * s * s)
            + (major * major + minor * minor) * s
            + major * minor * (0.5 * phi + 0.25 * (2.0 * phi).sin())
            + major * minor * phi
    };
    let sigma = if reversed { -1.0 } else { 1.0 };
    Ok(sigma * (2.0 * PI * minor / 3.0) * (g(phi_e) - g(phi_s)))
}

/// The poloidal interval `(φ_s, φ_e)` of a torus latitude band's outer loop,
/// read from its two `Curve::Circle` rims: each rim's `φ` from its radius and
/// axial offset (`ρ = R + r cos φ`, `τ = r sin φ`), the start rim by its
/// traversal sense (CCW about `+â`, `−â` when reversed), and `φ_e` unwrapped
/// to lie within one turn after `φ_s`. `None` for any other loop shape, or
/// when the rims do not lie on the torus, are not coaxial, or are not
/// traversed oppositely — the callers name the refusal. Shared by the volume
/// term and the render tessellator so the two read ONE region.
pub(crate) fn torus_latitude_band_phis(
    arena: &crate::arena::BrepArena,
    face: &crate::arena::Face,
    center: Point3,
    a: [f64; 3],
    major: f64,
    minor: f64,
    reversed: bool,
) -> Option<(f64, f64)> {
    use crate::arena::Curve;
    use std::f64::consts::PI;
    let hes = arena.loop_half_edges(face.outer_loop).ok()?;
    let mut rims: Vec<(f64, f64)> = Vec::new(); // (φ, sign about +â)
    for &h in &hes {
        let he = arena.half_edge(h).ok()?;
        if let Curve::Circle {
            center: cc,
            normal,
            radius,
        } = he.curve
        {
            let along = normal.x * a[0] + normal.y * a[1] + normal.z * a[2];
            if along.abs() < 1.0 - 1e-9 {
                return None;
            }
            let d = [
                cc.x() - center.x(),
                cc.y() - center.y(),
                cc.z() - center.z(),
            ];
            let tau = d[0] * a[0] + d[1] * a[1] + d[2] * a[2];
            let off = [d[0] - tau * a[0], d[1] - tau * a[1], d[2] - tau * a[2]];
            if (off[0] * off[0] + off[1] * off[1] + off[2] * off[2]).sqrt() > 1e-9 * (1.0 + major) {
                return None;
            }
            if ((radius - major).hypot(tau) - minor).abs() > 1e-9 * (1.0 + minor) {
                return None;
            }
            rims.push((tau.atan2(radius - major), along.signum()));
        }
    }
    let [(p0, s0), (p1, s1)] = rims[..] else {
        return None;
    };
    if s0 == s1 {
        return None;
    }
    let start_sign = if reversed { -1.0 } else { 1.0 };
    let (phi_s, phi_e_raw) = if s0 == start_sign { (p0, p1) } else { (p1, p0) };
    let dphi = (phi_e_raw - phi_s).rem_euclid(2.0 * PI);
    if !(dphi > 0.0 && dphi < 2.0 * PI) {
        return None;
    }
    Some((phi_s, phi_s + dphi))
}

/// Divergence-theorem flux `(1/3)∮ x·n dA` through a torus PATCH — SI5 C5b
/// (spec `si5_c5_sphere_torus_tier.md` §3): a [`Surface::Torus`] face bounded
/// by OPEN arcs, each either a LATITUDE arc (axis ∥ `â`, centre on the axis —
/// `φ` constant along it) or a POLOIDAL arc (radius `r`, centre on the tube
/// centre circle — `θ` constant along it). The corpus writes every torus
/// patch as the parameter rectangle `CCCC`, two of each (spec §2.4); the
/// Green identity below takes any region of that vocabulary, holes included.
///
/// Derivation. With `x = C + (R + r cos φ) ŵ(θ) + r sin φ â`,
/// `ŵ(θ) = cos θ ê₁ + sin θ ê₂`, outward `n = cos φ ŵ + sin φ â` and
/// `dA = r (R + r cos φ) dθ dφ`, the integrand over the parameter domain is
///
/// ```text
/// f(θ, φ) = r (R + r cos φ) [ (c₁ cos θ + c₂ sin θ) cos φ + c_a sin φ + R cos φ + r ],
///           c₁ = C·ê₁,  c₂ = C·ê₂,  c_a = C·â.
/// ```
///
/// Green's theorem with `∂F/∂θ = f` turns `∬ f dθ dφ` into `∮ F dφ`, to which
/// a latitude arc (`dφ = 0`) contributes nothing and a poloidal arc at azimuth
/// `θ_k` contributes `H(θ_k, φ_end) − H(θ_k, φ_start)`, `∂H/∂φ = F`:
///
/// ```text
/// H(θ, φ) = r [ (c₁ sin θ − c₂ cos θ) K(φ) + θ G(φ) ],
/// K(φ)    = R sin φ + r (φ/2 + sin 2φ / 4),
/// G(φ)    = c_a (−R cos φ + (r/2) sin²φ) + (R² + r²) sin φ + Rr (φ/2 + sin 2φ / 4) + Rr φ.
/// ```
///
/// `G` is the latitude band's own antiderivative ([`torus_latitude_band_flux`]):
/// the band is the rectangle `[0, 2π] × [φ_s, φ_e]`, and this function
/// reproduces its closed form term for term. The `(θ, φ)` frame is
/// right-handed about `n`, and a `reversed` face's boundary is walked with
/// material on its left about `−n` — clockwise in `(θ, φ)` — so the loop
/// integral's sign flips with the integrand's and the flux is `(1/3) Σ ΔH` for
/// BOTH senses (the cylinder-patch argument). Each loop's `θ` is unwrapped
/// along its own walk, and its absolute offset is immaterial: `Σ_loop ΔG = 0`
/// for a closed loop, and the other term is `θ`-periodic — so the torus's
/// branch cut is not a case. Anything outside the vocabulary — a Villarceau
/// arc, a chord, an arc off the surface, a poloidal arc away from the walk's
/// current azimuth, a loop that winds around the axis or the tube — is a loud
/// mismatch, never a chord approximation (P9).
pub(crate) fn torus_arc_patch_flux(
    arena: &crate::arena::BrepArena,
    f: crate::arena::FaceId,
    face: &crate::arena::Face,
    center: Point3,
    axis_dir: crate::arena::UnitVector3,
    major: f64,
    minor: f64,
) -> Result<f64, crate::error::KernelV2Error> {
    use crate::arena::Curve;
    use std::f64::consts::PI;
    let mismatch = |reason: &'static str| crate::error::KernelV2Error::CurvedGeometryMismatch {
        face: f,
        reason,
    };
    let a = [axis_dir.x, axis_dir.y, axis_dir.z];
    let c = [center.x(), center.y(), center.z()];
    let dot = |u: [f64; 3], v: [f64; 3]| u[0] * v[0] + u[1] * v[1] + u[2] * v[2];
    let cross = |u: [f64; 3], v: [f64; 3]| {
        [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ]
    };
    // Any frame ⊥ â; the flux is frame-invariant.
    let e1 = {
        let t = if a[0].abs() < 0.9 {
            [1.0, 0.0, 0.0]
        } else {
            [0.0, 1.0, 0.0]
        };
        let raw = cross(a, t);
        let l = dot(raw, raw).sqrt();
        [raw[0] / l, raw[1] / l, raw[2] / l]
    };
    let e2 = cross(a, e1);
    let (c1, c2, ca) = (dot(c, e1), dot(c, e2), dot(c, a));
    let gfun = |phi: f64| {
        let (s, co) = phi.sin_cos();
        ca * (-major * co + 0.5 * minor * s * s)
            + (major * major + minor * minor) * s
            + major * minor * (0.5 * phi + 0.25 * (2.0 * phi).sin())
            + major * minor * phi
    };
    let kfun = |phi: f64| major * phi.sin() + minor * (0.5 * phi + 0.25 * (2.0 * phi).sin());
    let hfun = |theta: f64, phi: f64| {
        minor * ((c1 * theta.sin() - c2 * theta.cos()) * kfun(phi) + theta * gfun(phi))
    };
    let band = 1e-9 * (1.0 + major + minor);
    // Cylindrical coordinates about the torus axis: (τ, radial vector).
    let cyl = |p: [f64; 3]| {
        let d = [p[0] - c[0], p[1] - c[1], p[2] - c[2]];
        let tau = dot(d, a);
        (
            tau,
            [d[0] - tau * a[0], d[1] - tau * a[1], d[2] - tau * a[2]],
        )
    };

    let mut loops = vec![face.outer_loop];
    loops.extend(face.inner_loops.iter().copied());
    let mut sum = 0.0f64;
    for lid in loops {
        let hes = arena.loop_half_edges(lid)?;
        let Some(&h0) = hes.first() else {
            return Err(mismatch("signed_volume: torus patch with an empty loop"));
        };
        let p_first = arena.vertex(arena.half_edge(h0)?.origin)?.point;
        let (_, rad0) = cyl([p_first.x(), p_first.y(), p_first.z()]);
        if dot(rad0, rad0).sqrt() <= band {
            return Err(mismatch(
                "signed_volume: torus patch vertex on the torus axis",
            ));
        }
        // θ of the walk's current vertex, unwrapped along the loop.
        let mut theta_acc = dot(rad0, e2).atan2(dot(rad0, e1));
        let mut dtheta_total = 0.0f64;
        let mut dphi_total = 0.0f64;
        for &h in &hes {
            let he = arena.half_edge(h)?;
            let p0 = arena.vertex(he.origin)?.point;
            let p1 = arena.vertex(arena.half_edge(he.next)?.origin)?.point;
            let Curve::Arc {
                center: cs,
                normal,
                radius: ra,
            } = he.curve
            else {
                return Err(mismatch(
                    "signed_volume: torus patch edge is not a circular arc (latitude or \
                     poloidal) — no closed form",
                ));
            };
            let nu = [normal.x, normal.y, normal.z];
            let along = dot(nu, a);
            let (tau, rad) = cyl([cs.x(), cs.y(), cs.z()]);
            let rho = dot(rad, rad).sqrt();
            let Some(sweep) = ccw_sweep(cs, nu, p0, p1) else {
                return Err(mismatch(
                    "signed_volume: degenerate torus patch arc endpoints",
                ));
            };
            if along.abs() > 1.0 - 1e-9 {
                // LATITUDE arc: centred on the axis, its (ρ, τ) on the tube.
                if rho > band {
                    return Err(mismatch(
                        "signed_volume: torus patch latitude arc is not centred on the axis",
                    ));
                }
                if ((ra - major).hypot(tau) - minor).abs() > band {
                    return Err(mismatch(
                        "signed_volume: torus patch latitude arc does not lie on the torus",
                    ));
                }
                let dtheta = along.signum() * sweep;
                theta_acc += dtheta;
                dtheta_total += dtheta;
            } else if along.abs() <= 1e-9 {
                // POLOIDAL arc: the tube circle at one azimuth.
                if tau.abs() > band || (rho - major).abs() > band || (ra - minor).abs() > band {
                    return Err(mismatch(
                        "signed_volume: torus patch poloidal arc is not the tube circle at its \
                         azimuth",
                    ));
                }
                let ghat = [rad[0] / rho, rad[1] / rho, rad[2] / rho];
                if dot(nu, ghat).abs() > 1e-9 {
                    return Err(mismatch(
                        "signed_volume: torus patch arc is neither latitude nor poloidal \
                         (a Villarceau circle has no closed form here)",
                    ));
                }
                let theta_c = dot(ghat, e2).atan2(dot(ghat, e1));
                let wrap = (theta_c - theta_acc + PI).rem_euclid(2.0 * PI) - PI;
                if wrap.abs() > 1e-9 {
                    return Err(mismatch(
                        "signed_volume: torus patch poloidal arc is not at the walk's current \
                         azimuth",
                    ));
                }
                // CCW about ĝ × â is +φ (∂x/∂φ at φ = 0 is r·â).
                let m = cross(ghat, a);
                let s = dot(nu, m).signum();
                let q0 = [p0.x() - cs.x(), p0.y() - cs.y(), p0.z() - cs.z()];
                let phi0 = dot(q0, a).atan2(dot(q0, ghat));
                let phi1 = phi0 + s * sweep;
                sum += hfun(theta_acc, phi1) - hfun(theta_acc, phi0);
                dphi_total += s * sweep;
            } else {
                return Err(mismatch(
                    "signed_volume: torus patch arc is neither latitude nor poloidal \
                     (a Villarceau circle has no closed form here)",
                ));
            }
        }
        if dtheta_total.abs() > 1e-9 || dphi_total.abs() > 1e-9 {
            return Err(mismatch(
                "signed_volume: torus patch loop winds around the axis or the tube — not a \
                 parameter-plane region",
            ));
        }
    }
    Ok(sum / 3.0)
}

/// Divergence-theorem flux `(1/3)∮ x·n dA` through a SPHERE PATCH — SI5 C5b
/// (spec `si5_c5_sphere_torus_tier.md` §3): a [`Surface::Sphere`] face bounded
/// by circular arcs, great or small (the corpus's three-fillet corner blend is
/// `CCC`, spec §2.4).
///
/// Derivation. On the sphere `x = C + ρ n` with `n` the unit normal away from
/// the centre, so `x·n = C·n + ρ` and, with `σ = −1` for a cavity (`reversed`),
///
/// ```text
/// Φ = (1/3) [ σ ρ A + C · ∫ n_out dA ],   ∫ n_out dA = ½ ∮ x × dx.
/// ```
///
/// `A` is the patch area; the vector area is a boundary quantity, walked with
/// material on the left about `n_out`, so it already carries the face's sense.
/// Both are closed forms over circular arcs:
///
/// - **Vector area.** An arc of radius `a` about centre `c` sweeping `Δ` CCW
///   about `m̂` from `p₀` to `p₁` contributes `½ [ c × (p₁ − p₀) + a² Δ m̂ ]`.
/// - **Area by Gauss–Bonnet.** `A = ρ² (2π χ − T)`, `χ = 1 − #holes`, where
///   `T` is the boundary's total turning about `n_out`. Along an arc the
///   geodesic curvature is constant: with `h = m̂ · (c − C)` the signed height
///   of the arc's plane along its own axis (`h² + a² = ρ²`), the binormal of
///   the CCW-about-`m̂` traversal is `m̂` and `m̂ · n_out = σ h / ρ` at every
///   point of the arc, so `κ_g = σ h / (a ρ)` and `∫ κ_g ds = σ h Δ / ρ`. At
///   each vertex the exterior angle is `atan2((T_in × T_out) · n_out, T_in · T_out)`.
///   A great circle has `h = 0` and turns only at its vertices; a hemisphere
///   (`T = 0`) gives `A = 2πρ²`, a polar cap at height `h` gives `2πρ(ρ − h)`.
///
/// An exterior angle of `±π` — the boundary doubling back on itself, which is
/// what the closed modeling sphere's seam slit does — has no sign and would
/// silently measure the wrong region, so it is a loud mismatch; so is a
/// non-arc edge, an arc off the sphere, or an area outside `(0, 4πρ²)`.
pub(crate) fn sphere_arc_patch_flux(
    arena: &crate::arena::BrepArena,
    f: crate::arena::FaceId,
    face: &crate::arena::Face,
    center: Point3,
    radius: f64,
    reversed: bool,
) -> Result<f64, crate::error::KernelV2Error> {
    use crate::arena::Curve;
    use std::f64::consts::PI;
    let mismatch = |reason: &'static str| crate::error::KernelV2Error::CurvedGeometryMismatch {
        face: f,
        reason,
    };
    let dot = |u: [f64; 3], v: [f64; 3]| u[0] * v[0] + u[1] * v[1] + u[2] * v[2];
    let cross = |u: [f64; 3], v: [f64; 3]| {
        [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ]
    };
    let unit = |u: [f64; 3]| -> Option<[f64; 3]> {
        let l = dot(u, u).sqrt();
        (l.is_finite() && l > 0.0).then(|| [u[0] / l, u[1] / l, u[2] / l])
    };
    let sigma = if reversed { -1.0 } else { 1.0 };
    let c = [center.x(), center.y(), center.z()];
    let band = 1e-9 * (1.0 + radius);

    let mut loops = vec![face.outer_loop];
    loops.extend(face.inner_loops.iter().copied());
    let n_holes = face.inner_loops.len() as f64;
    let mut turning = 0.0f64;
    let mut vec_area = [0.0f64; 3];
    for lid in loops {
        let hes = arena.loop_half_edges(lid)?;
        // (arc centre, CCW axis, start, end) per edge, for the vertex angles.
        let mut arcs: Vec<([f64; 3], [f64; 3], Point3, Point3)> = Vec::with_capacity(hes.len());
        for &h in &hes {
            let he = arena.half_edge(h)?;
            let p0 = arena.vertex(he.origin)?.point;
            let p1 = arena.vertex(arena.half_edge(he.next)?.origin)?.point;
            let Curve::Arc {
                center: cs,
                normal,
                radius: ra,
            } = he.curve
            else {
                return Err(mismatch(
                    "signed_volume: sphere patch edge is not a circular arc — no closed form",
                ));
            };
            let nu = [normal.x, normal.y, normal.z];
            let csv = [cs.x(), cs.y(), cs.z()];
            let d = [csv[0] - c[0], csv[1] - c[1], csv[2] - c[2]];
            let hgt = dot(d, nu);
            let off = [d[0] - hgt * nu[0], d[1] - hgt * nu[1], d[2] - hgt * nu[2]];
            if dot(off, off).sqrt() > band {
                return Err(mismatch(
                    "signed_volume: sphere patch arc's circle is not centred on the sphere's \
                     diameter along its own axis",
                ));
            }
            if (hgt * hgt + ra * ra - radius * radius).abs() > band * radius {
                return Err(mismatch(
                    "signed_volume: sphere patch arc does not lie on the sphere",
                ));
            }
            let Some(sweep) = ccw_sweep(cs, nu, p0, p1) else {
                return Err(mismatch(
                    "signed_volume: degenerate sphere patch arc endpoints",
                ));
            };
            turning += sigma * hgt * sweep / radius;
            let dp = [p1.x() - p0.x(), p1.y() - p0.y(), p1.z() - p0.z()];
            let cx = cross(csv, dp);
            for k in 0..3 {
                vec_area[k] += 0.5 * (cx[k] + ra * ra * sweep * nu[k]);
            }
            arcs.push((csv, nu, p0, p1));
        }
        for i in 0..arcs.len() {
            let (ci, ni, _, pe) = arcs[i];
            let (cj, nj, ps, _) = arcs[(i + 1) % arcs.len()];
            let (Some(t_in), Some(t_out), Some(n_at)) = (
                unit(cross(ni, [pe.x() - ci[0], pe.y() - ci[1], pe.z() - ci[2]])),
                unit(cross(nj, [ps.x() - cj[0], ps.y() - cj[1], ps.z() - cj[2]])),
                unit([pe.x() - c[0], pe.y() - c[1], pe.z() - c[2]]),
            ) else {
                return Err(mismatch(
                    "signed_volume: degenerate tangent at a sphere patch vertex",
                ));
            };
            let n_out = [sigma * n_at[0], sigma * n_at[1], sigma * n_at[2]];
            let ext = dot(cross(t_in, t_out), n_out).atan2(dot(t_in, t_out));
            if PI - ext.abs() <= 1e-9 {
                return Err(mismatch(
                    "signed_volume: sphere patch boundary reverses direction at a vertex (a \
                     seam slit has no area sign)",
                ));
            }
            turning += ext;
        }
    }
    let area = radius * radius * (2.0 * PI * (1.0 - n_holes) - turning);
    let full = 4.0 * PI * radius * radius;
    if !(area > 0.0 && area < full * (1.0 + 1e-9)) {
        return Err(mismatch(
            "signed_volume: sphere patch boundary turning gives no area in (0, 4πr²)",
        ));
    }
    Ok((sigma * radius * area + dot(c, vec_area)) / 3.0)
}

/// Public alias of [`torus_latitude_band_phis`] for the SI5 harness probes.
#[allow(clippy::too_many_arguments)]
pub fn torus_latitude_band_phis_pub(
    arena: &crate::arena::BrepArena,
    face: &crate::arena::Face,
    center: Point3,
    a: [f64; 3],
    major: f64,
    minor: f64,
    reversed: bool,
) -> Option<(f64, f64)> {
    torus_latitude_band_phis(arena, face, center, a, major, minor, reversed)
}
