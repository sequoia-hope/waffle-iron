//! Per-surface face-validation routines for [`validate_solid`]: the planar,
//! cylinder, cone, torus, and sphere face/patch invariant checks (invariants
//! 4+5 — on-surface geometry and orientation consistency). Extracted verbatim
//! from `validate.rs` (move-only, F9; the five entry points are `pub(crate)`
//! so `validate_solid` can dispatch to them, the two `*_patch` helpers stay
//! private to this module).

use crate::arena::{BrepArena, Curve, FaceId, HalfEdgeId, LoopId};
use crate::error::KernelV2Error;
use crate::geom;
use cad_primitives::Point3;

#[allow(clippy::wildcard_imports)]
use super::*;

mod cone;
pub(crate) use cone::*;

/// Invariants 4+5 for a planar face: surface agreement with the boundary
/// walk.
///
/// - Polygonal loops use the Newell normal (hard rule 2) exactly as in KV1.
/// - Loops bounded by a single closed circle half-edge (PR-KV5a) use the
///   directional [`Curve::Circle`] normal as the orientation source — a
///   cap's circle must traverse CCW around the face normal, a circle ring
///   CCW around its negation (the ring-winding analog).
/// - Loops mixing [`Curve::Arc`] and segment edges (PR-KV5b yang boolean
///   outputs; PR-KV6a revolve sectors with sweeps anywhere in (0, 2π))
///   use the midpoint-augmented winding polyline ([`winding_points`]) for
///   the Newell normal, which winds identically to the true boundary for
///   any sweep < 2π. Each arc additionally must lie in the face plane
///   (its circle axis parallel to the face normal — sign-free, since a
///   loop legitimately walks arcs both ways around their centers).
pub(crate) fn validate_planar_face(
    arena: &BrepArena,
    f: FaceId,
    face: &crate::arena::Face,
    plane: crate::arena::Plane,
) -> Result<(), KernelV2Error> {
    // Arc-in-plane production rule, shared by all loops of this face.
    let arcs_in_plane = |hes: &[HalfEdgeId]| -> Result<(), KernelV2Error> {
        for &(_, nu, _) in &loop_arcs(arena, hes)? {
            if geom::dot(nu, plane.normal).abs() < 1.0 - NORMAL_AGREEMENT_TOLERANCE {
                return Err(KernelV2Error::CurvedGeometryMismatch {
                    face: f,
                    reason: "planar-face arc's circle axis is not parallel to the face normal",
                });
            }
        }
        Ok(())
    };

    // ---- outer loop orientation -------------------------------------------
    let outer_hes = arena.loop_half_edges(face.outer_loop)?;
    let outer_circles = loop_circles(arena, &outer_hes)?;
    if outer_circles.is_empty() {
        // Stored normal ≡ Newell(outer loop) — hard rule 2. Arc-bearing
        // loops use the midpoint-augmented winding polyline (see
        // `winding_points`) so ANY sweep < 2π winds correctly.
        arcs_in_plane(&outer_hes)?;
        let pts = winding_points(arena, &outer_hes)?;
        let Some(newell) = geom::newell_unit(&pts) else {
            return Err(KernelV2Error::NewellMismatch { face: f });
        };
        if geom::dot(plane.normal, newell) < 1.0 - NORMAL_AGREEMENT_TOLERANCE {
            return Err(KernelV2Error::NewellMismatch { face: f });
        }
    } else {
        // Full-circle boundary: exactly ONE closed circle half-edge.
        if outer_hes.len() != 1 {
            return Err(KernelV2Error::CurvedGeometryMismatch {
                face: f,
                reason: "planar loop mixes a full circle with other edges",
            });
        }
        let (_, nu, _) = outer_circles[0];
        if geom::dot(nu, plane.normal) < 1.0 - NORMAL_AGREEMENT_TOLERANCE {
            return Err(KernelV2Error::CurvedGeometryMismatch {
                face: f,
                reason: "cap circle half-edge must traverse CCW around the face normal",
            });
        }
    }

    // ---- ring winding -----------------------------------------------------
    for &rid in &face.inner_loops {
        let hes = arena.loop_half_edges(rid)?;
        let circles = loop_circles(arena, &hes)?;
        if circles.is_empty() {
            arcs_in_plane(&hes)?;
            let ring_pts = winding_points(arena, &hes)?;
            if ring_pts.is_empty() {
                continue; // lone-vertex ring has no winding
            }
            let rn = geom::newell(&ring_pts);
            let d = rn[0] * plane.normal.x + rn[1] * plane.normal.y + rn[2] * plane.normal.z;
            if d >= 0.0 {
                return Err(KernelV2Error::RingWindingMismatch { face: f, ring: rid });
            }
        } else {
            if hes.len() != 1 {
                return Err(KernelV2Error::CurvedGeometryMismatch {
                    face: f,
                    reason: "planar ring mixes a full circle with other edges",
                });
            }
            let (_, nu, _) = circles[0];
            if geom::dot(nu, plane.normal) > -(1.0 - NORMAL_AGREEMENT_TOLERANCE) {
                return Err(KernelV2Error::RingWindingMismatch { face: f, ring: rid });
            }
        }
    }

    // ---- strict-tier geometric tripwires (see module docs) ----------------
    #[cfg(any(debug_assertions, feature = "strict-validation"))]
    {
        let mut loops = vec![face.outer_loop];
        loops.extend(face.inner_loops.iter().copied());
        for lid in loops {
            // Every loop vertex (incl. circle anchors) on the plane.
            if std::env::var_os("KV2_PLANARITY_PROBE").is_some() {
                for p in arena.loop_points(lid)? {
                    let d = (p.x() - plane.point.x()) * plane.normal.x
                        + (p.y() - plane.point.y()) * plane.normal.y
                        + (p.z() - plane.point.z()) * plane.normal.z;
                    eprintln!(
                        "[planarity-probe] face={f:?} loop={lid:?} p=({:.17e},{:.17e},{:.17e}) \
                         d={d:.3e} band={:.3e} viol={} plane n=({:.17},{:.17},{:.17})",
                        p.x(),
                        p.y(),
                        p.z(),
                        planarity_band(p),
                        d.abs() > planarity_band(p),
                        plane.normal.x,
                        plane.normal.y,
                        plane.normal.z,
                    );
                }
            }
            for p in arena.loop_points(lid)? {
                let d = (p.x() - plane.point.x()) * plane.normal.x
                    + (p.y() - plane.point.y()) * plane.normal.y
                    + (p.z() - plane.point.z()) * plane.normal.z;
                if d.abs() > planarity_band(p) {
                    return Err(KernelV2Error::NonPlanarFace { face: f });
                }
            }
            // Circle/arc centers on the plane; endpoints on their circles.
            // Full circles keep the exact-construction band; arcs (imported
            // yang output, PR-KV5b) use the import band.
            let hes = arena.loop_half_edges(lid)?;
            for &h in &hes {
                let he = arena.half_edge(h)?;
                // PR-KV9: ellipse arcs check center-on-plane + endpoints on
                // the ellipse (frame residual scaled by the minor radius,
                // the conservative in-plane length conversion) at the import
                // band, then continue — the circle logic below is
                // radius-based and does not apply.
                if let Curve::EllipseArc {
                    center,
                    normal,
                    major_axis,
                    major_radius,
                    minor_radius,
                } = he.curve
                {
                    let band = import_band(major_radius, center);
                    let d = (center.x() - plane.point.x()) * plane.normal.x
                        + (center.y() - plane.point.y()) * plane.normal.y
                        + (center.z() - plane.point.z()) * plane.normal.z;
                    if d.abs() > band {
                        return Err(KernelV2Error::NonPlanarFace { face: f });
                    }
                    let nu = [normal.x, normal.y, normal.z];
                    let mr = [major_axis.x, major_axis.y, major_axis.z];
                    for p in [
                        arena.vertex(he.origin)?.point,
                        arena.vertex(arena.half_edge(he.next)?.origin)?.point,
                    ] {
                        let w = [
                            nu[1] * mr[2] - nu[2] * mr[1],
                            nu[2] * mr[0] - nu[0] * mr[2],
                            nu[0] * mr[1] - nu[1] * mr[0],
                        ];
                        let dv = [p.x() - center.x(), p.y() - center.y(), p.z() - center.z()];
                        let u = (dv[0] * mr[0] + dv[1] * mr[1] + dv[2] * mr[2]) / major_radius;
                        let v = (dv[0] * w[0] + dv[1] * w[1] + dv[2] * w[2]) / minor_radius;
                        let band = import_band(major_radius, p);
                        let residual = (u.hypot(v) - 1.0).abs() * minor_radius;
                        if residual > band {
                            return Err(vertex_off_surface(
                                f,
                                "planar-ellipse-arc-endpoint",
                                p,
                                residual,
                                band,
                                &format!(
                                    "plane; ellipse center=({:.17e},{:.17e},{:.17e}) \
                                     major_r={major_radius:.17e} minor_r={minor_radius:.17e}",
                                    center.x(),
                                    center.y(),
                                    center.z()
                                ),
                            ));
                        }
                    }
                    continue;
                }
                // KV16: hyperbola arcs check center-on-plane + endpoints on
                // the branch (first-order in-plane distance + out-of-plane
                // component, `geom::hyperbola_branch_residual`) at the
                // import band, then continue.
                if let Curve::HyperbolaArc {
                    center,
                    normal,
                    major_axis,
                    semi_transverse,
                    semi_conjugate,
                } = he.curve
                {
                    let band = import_band(semi_transverse.max(semi_conjugate), center);
                    let d = (center.x() - plane.point.x()) * plane.normal.x
                        + (center.y() - plane.point.y()) * plane.normal.y
                        + (center.z() - plane.point.z()) * plane.normal.z;
                    if d.abs() > band {
                        return Err(KernelV2Error::NonPlanarFace { face: f });
                    }
                    let nu = [normal.x, normal.y, normal.z];
                    let mr = [major_axis.x, major_axis.y, major_axis.z];
                    for p in [
                        arena.vertex(he.origin)?.point,
                        arena.vertex(arena.half_edge(he.next)?.origin)?.point,
                    ] {
                        let (in_plane, out_of_plane, u) = geom::hyperbola_branch_residual(
                            center,
                            nu,
                            mr,
                            semi_transverse,
                            semi_conjugate,
                            p,
                        );
                        let band = import_band(semi_transverse.max(semi_conjugate), p);
                        if u <= 0.0 || in_plane > band || out_of_plane.abs() > band {
                            return Err(vertex_off_surface(
                                f,
                                "planar-hyperbola-arc-endpoint",
                                p,
                                in_plane.max(out_of_plane.abs()),
                                band,
                                &format!(
                                    "plane; hyperbola center=({:.17e},{:.17e},{:.17e}) \
                                     a={semi_transverse:.17e} b={semi_conjugate:.17e} u={u:.3e}",
                                    center.x(),
                                    center.y(),
                                    center.z()
                                ),
                            ));
                        }
                    }
                    continue;
                }
                let (center, radius, is_arc) = match he.curve {
                    Curve::Circle { center, radius, .. } => (center, radius, false),
                    Curve::Arc { center, radius, .. } => (center, radius, true),
                    // EllipseArc/HyperbolaArc handled (and continued) above.
                    Curve::LineSegment | Curve::EllipseArc { .. } | Curve::HyperbolaArc { .. } => {
                        continue
                    }
                    // M5 K8: a transversal quadric-pair curve is never
                    // planar — degenerate configurations produce conics
                    // upstream in ssi-rs. Placement on a plane face is a
                    // defect, typed and loud.
                    Curve::SurfacePair { .. } => {
                        return Err(KernelV2Error::CurvedGeometryMismatch {
                            face: f,
                            reason: "surface-pair edge on a planar face (a transversal \
                                     quadric-pair curve is never planar)",
                        });
                    }
                };
                let plane_band = if is_arc {
                    import_band(radius, center)
                } else {
                    planarity_band(center)
                };
                let d = (center.x() - plane.point.x()) * plane.normal.x
                    + (center.y() - plane.point.y()) * plane.normal.y
                    + (center.z() - plane.point.z()) * plane.normal.z;
                if d.abs() > plane_band {
                    return Err(KernelV2Error::NonPlanarFace { face: f });
                }
                let mut endpoints = vec![arena.vertex(he.origin)?.point];
                if is_arc {
                    endpoints.push(arena.vertex(arena.half_edge(he.next)?.origin)?.point);
                }
                for p in endpoints {
                    let band = if is_arc {
                        import_band(radius, p)
                    } else {
                        CURVED_SURFACE_DEBUG_TOLERANCE
                    };
                    let dr = ((p.x() - center.x()).powi(2)
                        + (p.y() - center.y()).powi(2)
                        + (p.z() - center.z()).powi(2))
                    .sqrt();
                    if (dr - radius).abs() > band {
                        return Err(vertex_off_surface(
                            f,
                            if is_arc {
                                "planar-arc-endpoint"
                            } else {
                                "planar-circle-anchor"
                            },
                            p,
                            (dr - radius).abs(),
                            band,
                            &format!(
                                "plane; circle center=({:.17e},{:.17e},{:.17e}) r={radius:.17e}",
                                center.x(),
                                center.y(),
                                center.z()
                            ),
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}

/// Invariants 4+5 for a cylinder lateral face. Two vocabularies:
///
/// **Canonical full lateral (PR-KV5a)** — any loop carries a full-circle
/// half-edge. Production tier (the curved Newell analog, decided from
/// stored data, no geometric tolerance beyond unit-vector rounding):
///
/// - finite positive radius; unit axis; outward sense (`reversed` is the
///   KV5b cavity vocabulary and never canonical);
/// - no inner loops, and exactly TWO full-circle rim half-edges in the
///   outer loop (the Stroud single-fake-edge lateral);
/// - each rim's radius equals the surface radius and its normal is along
///   the axis;
/// - each rim's traversal axis points TOWARD the opposite rim — this is
///   what makes the boundary walk consistent with the radially-outward
///   surface orientation (walking a rim with the face on your left, viewed
///   from outside, runs CCW around the axis pointing into the lateral).
///
/// **Partial patch (PR-KV5b, yang boolean outputs)** — loops of
/// [`Curve::Arc`] and segment edges. Production tier (see
/// [`validate_cylinder_patch`]): per-arc surface agreement (radius, axis
/// parallelism) plus the UNROLLED-WINDING orientation analysis — the
/// developable-surface generalization of the Newell rule: in the unrolled
/// `(θ·r, h)` frame (mirrored for `reversed`), the boundary loops must
/// wind material-CCW: either exactly one non-wrapping loop is CCW with all
/// others CW (a bounded patch with windows), or exactly two loops wrap the
/// axis (±1) with the `+1` wrap at the lower axial height and every
/// non-wrapping loop CW (a barrel segment with windows).
///
/// Debug tier: loop vertices on the surface, rim/arc centers on the axis —
/// at [`CURVED_SURFACE_DEBUG_TOLERANCE`] for exact-constructed canonical
/// solids, at the scale-relative [`import_band`] for imported patches;
/// canonical seam segments parallel to the axis (partial patches carry
/// genuine chord segments, which are NOT rulings, so no seam rule there).
#[allow(clippy::too_many_arguments)]
pub(crate) fn validate_cylinder_face(
    arena: &BrepArena,
    f: FaceId,
    face: &crate::arena::Face,
    axis_point: Point3,
    axis_dir: crate::arena::UnitVector3,
    radius: f64,
    reversed: bool,
) -> Result<(), KernelV2Error> {
    let mismatch = |reason: &'static str| KernelV2Error::CurvedGeometryMismatch { face: f, reason };
    if !radius.is_finite() || radius <= 0.0 {
        return Err(mismatch("cylinder radius must be finite and positive"));
    }
    let alen = (axis_dir.x * axis_dir.x + axis_dir.y * axis_dir.y + axis_dir.z * axis_dir.z).sqrt();
    if (alen - 1.0).abs() > NORMAL_AGREEMENT_TOLERANCE {
        return Err(mismatch("cylinder axis_dir must be unit-length"));
    }

    // Vocabulary dispatch: any full-circle edge anywhere → canonical.
    let mut all_loops = vec![face.outer_loop];
    all_loops.extend(face.inner_loops.iter().copied());
    let mut has_full = false;
    for &lid in &all_loops {
        if !loop_circles(arena, &arena.loop_half_edges(lid)?)?.is_empty() {
            has_full = true;
        }
    }
    if !has_full {
        return validate_cylinder_patch(arena, f, face, axis_point, axis_dir, radius, reversed);
    }

    if !face.inner_loops.is_empty() {
        return Err(mismatch(
            "cylinder face with inner loops is outside the KV5a vocabulary",
        ));
    }
    let hes = arena.loop_half_edges(face.outer_loop)?;
    if !loop_arcs(arena, &hes)?.is_empty() {
        return Err(mismatch(
            "cylinder face mixes full-circle rims with arc edges",
        ));
    }
    let rims = loop_circles(arena, &hes)?;
    if rims.len() != 2 {
        return Err(mismatch(
            "cylinder face must be bounded by exactly two full-circle rims (KV5a)",
        ));
    }
    for (i, &(c, nu, r)) in rims.iter().enumerate() {
        if (r - radius).abs() > 1e-9 * radius {
            return Err(mismatch("rim circle radius disagrees with the surface"));
        }
        if (geom::dot(nu, axis_dir)).abs() < 1.0 - NORMAL_AGREEMENT_TOLERANCE {
            return Err(mismatch(
                "rim circle normal must be along the cylinder axis",
            ));
        }
        let other = rims[1 - i].0;
        let toward =
            (other.x() - c.x()) * nu.x + (other.y() - c.y()) * nu.y + (other.z() - c.z()) * nu.z;
        // Outward lateral: each rim's traversal axis points TOWARD the
        // opposite rim. Cavity wall (reversed, PR-KV6a — the washer's
        // inner bore): the mirrored material sense, AWAY from it. (The twin
        // structure forces this: each rim twin lives in an adjacent face
        // whose own winding rules fix the sign.)
        if (!reversed && toward <= 0.0) || (reversed && toward >= 0.0) {
            return Err(mismatch(
                "rim traversal axis disagrees with the lateral's material sense",
            ));
        }
    }

    #[cfg(any(debug_assertions, feature = "strict-validation"))]
    {
        let dist_to_axis = |p: Point3| {
            let d = [
                p.x() - axis_point.x(),
                p.y() - axis_point.y(),
                p.z() - axis_point.z(),
            ];
            let t = d[0] * axis_dir.x + d[1] * axis_dir.y + d[2] * axis_dir.z;
            let r = [
                d[0] - t * axis_dir.x,
                d[1] - t * axis_dir.y,
                d[2] - t * axis_dir.z,
            ];
            (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt()
        };
        let cyl_desc = || {
            format!(
                "cylinder axis_point=({:.17e},{:.17e},{:.17e}) \
                 axis=({:.17e},{:.17e},{:.17e}) r={radius:.17e}",
                axis_point.x(),
                axis_point.y(),
                axis_point.z(),
                axis_dir.x,
                axis_dir.y,
                axis_dir.z
            )
        };
        for p in arena.loop_points(face.outer_loop)? {
            if (dist_to_axis(p) - radius).abs() > CURVED_SURFACE_DEBUG_TOLERANCE {
                return Err(vertex_off_surface(
                    f,
                    "cyl-canonical-vertex",
                    p,
                    (dist_to_axis(p) - radius).abs(),
                    CURVED_SURFACE_DEBUG_TOLERANCE,
                    &cyl_desc(),
                ));
            }
        }
        for &(c, _, _) in &rims {
            if dist_to_axis(c) > CURVED_SURFACE_DEBUG_TOLERANCE {
                return Err(vertex_off_surface(
                    f,
                    "cyl-rim-center-off-axis",
                    c,
                    dist_to_axis(c),
                    CURVED_SURFACE_DEBUG_TOLERANCE,
                    &cyl_desc(),
                ));
            }
        }
        // Seam segments must be rulings (parallel to the axis).
        for &h in &hes {
            let he = arena.half_edge(h)?;
            if matches!(he.curve, Curve::LineSegment) {
                let p0 = arena.vertex(he.origin)?.point;
                let p1 = arena.vertex(arena.half_edge(he.next)?.origin)?.point;
                let d = [p1.x() - p0.x(), p1.y() - p0.y(), p1.z() - p0.z()];
                let cx = [
                    d[1] * axis_dir.z - d[2] * axis_dir.y,
                    d[2] * axis_dir.x - d[0] * axis_dir.z,
                    d[0] * axis_dir.y - d[1] * axis_dir.x,
                ];
                let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
                let off = (cx[0] * cx[0] + cx[1] * cx[1] + cx[2] * cx[2]).sqrt();
                if off > CURVED_SURFACE_DEBUG_TOLERANCE * len.max(1.0) {
                    return Err(vertex_off_surface(
                        f,
                        "cyl-seam-not-ruling",
                        p0,
                        off,
                        CURVED_SURFACE_DEBUG_TOLERANCE * len.max(1.0),
                        &cyl_desc(),
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Validate a [`Surface::Torus`] face (KV6d increment 1 — foundation).
///
/// Checks the analytic parameters (a ring torus needs `major > minor > 0` and a
/// unit axis) and, in debug builds, that every loop vertex (outer + inner) lies
/// on the torus surface via [`geom::torus_residual`]. The detailed boundary
/// topology (profile-circle rims + longitude seam arcs for a partial torus, or
/// the seam loops of a full torus) is pinned and exercised end to end when the
/// KV6d revolve constructor (increment 3) produces it; this foundation
/// validator is deliberately topology-agnostic so it accepts whatever shape the
/// constructor settles on while still guarding the surface geometry.
pub(crate) fn validate_torus_face(
    arena: &BrepArena,
    f: FaceId,
    face: &crate::arena::Face,
    center: Point3,
    axis_dir: crate::arena::UnitVector3,
    major_radius: f64,
    minor_radius: f64,
) -> Result<(), KernelV2Error> {
    let mismatch = |reason: &'static str| KernelV2Error::CurvedGeometryMismatch { face: f, reason };
    if !minor_radius.is_finite() || minor_radius <= 0.0 {
        return Err(mismatch("torus minor_radius must be finite and positive"));
    }
    if !major_radius.is_finite() || major_radius <= minor_radius {
        return Err(mismatch(
            "torus major_radius must be finite and exceed minor_radius (ring torus)",
        ));
    }
    let alen = (axis_dir.x * axis_dir.x + axis_dir.y * axis_dir.y + axis_dir.z * axis_dir.z).sqrt();
    if (alen - 1.0).abs() > NORMAL_AGREEMENT_TOLERANCE {
        return Err(mismatch("torus axis_dir must be unit-length"));
    }

    #[cfg(any(debug_assertions, feature = "strict-validation"))]
    {
        let on_torus_residual = |p: Point3| {
            let d = [p.x() - center.x(), p.y() - center.y(), p.z() - center.z()];
            let tau = d[0] * axis_dir.x + d[1] * axis_dir.y + d[2] * axis_dir.z;
            let radial = [
                d[0] - tau * axis_dir.x,
                d[1] - tau * axis_dir.y,
                d[2] - tau * axis_dir.z,
            ];
            let rho =
                (radial[0] * radial[0] + radial[1] * radial[1] + radial[2] * radial[2]).sqrt();
            geom::torus_residual(tau, rho, major_radius, minor_radius).abs()
        };
        let mut loops = vec![face.outer_loop];
        loops.extend(face.inner_loops.iter().copied());
        for lid in loops {
            for p in arena.loop_points(lid)? {
                // The residual is in length², so compare against a band scaled
                // by the minor radius (a length·length tolerance).
                if on_torus_residual(p) > CURVED_SURFACE_DEBUG_TOLERANCE * minor_radius.max(1.0) {
                    return Err(vertex_off_surface(
                        f,
                        "torus-vertex",
                        p,
                        on_torus_residual(p),
                        CURVED_SURFACE_DEBUG_TOLERANCE * minor_radius.max(1.0),
                        &format!(
                            "torus center=({:.17e},{:.17e},{:.17e}) \
                             major_r={major_radius:.17e} minor_r={minor_radius:.17e}",
                            center.x(),
                            center.y(),
                            center.z()
                        ),
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Validate a [`Surface::Sphere`] face (KV6d increment 2, spec
/// `kv6d_sphere_revolve.md`).
///
/// Checks the analytic parameters (finite `radius > 0`, finite center) and,
/// in debug builds, that every loop vertex (outer + inner) lies on the
/// sphere via [`geom::sphere_residual`]. Deliberately topology-agnostic
/// (the torus-validator precedent): it accepts both the closed seam-arc
/// loop the revolve constructor builds and boolean-output trimmed patches.
pub(crate) fn validate_sphere_face(
    arena: &BrepArena,
    f: FaceId,
    face: &crate::arena::Face,
    center: Point3,
    radius: f64,
) -> Result<(), KernelV2Error> {
    let mismatch = |reason: &'static str| KernelV2Error::CurvedGeometryMismatch { face: f, reason };
    if !radius.is_finite() || radius <= 0.0 {
        return Err(mismatch("sphere radius must be finite and positive"));
    }
    if !(center.x().is_finite() && center.y().is_finite() && center.z().is_finite()) {
        return Err(mismatch("sphere center must be finite"));
    }

    #[cfg(any(debug_assertions, feature = "strict-validation"))]
    {
        // `sphere_residual` is a plain length; scale the band by the radius
        // (a length·length tolerance, matching the torus convention).
        let band = CURVED_SURFACE_DEBUG_TOLERANCE * radius.max(1.0);
        let mut loops = vec![face.outer_loop];
        loops.extend(face.inner_loops.iter().copied());
        for lid in loops {
            for p in arena.loop_points(lid)? {
                let res = geom::sphere_residual(p, center, radius).abs();
                if res > band {
                    return Err(vertex_off_surface(
                        f,
                        "sphere-vertex",
                        p,
                        res,
                        band,
                        &format!(
                            "sphere center=({:.17e},{:.17e},{:.17e}) radius={radius:.17e}",
                            center.x(),
                            center.y(),
                            center.z()
                        ),
                    ));
                }
            }
        }
    }
    #[cfg(not(any(debug_assertions, feature = "strict-validation")))]
    {
        let _ = (arena, face);
    }
    Ok(())
}

/// Per-loop unrolled measurements over a cylinder patch (PR-KV5b): net
/// axis wrap, mean axial height, and (for non-wrapping loops) twice the
/// signed shoelace area in the unrolled `(θ, h)` frame.
struct LoopMeasure {
    loop_id: LoopId,
    wrap: i64,
    mean_h: f64,
    area2: f64,
}

/// Invariants 4+5 for a PARTIAL cylinder patch (PR-KV5b): boundary loops
/// of [`Curve::Arc`] and [`Curve::LineSegment`] edges, as assembled from
/// yang-rs boolean outputs. See [`validate_cylinder_face`]'s doc comment
/// for the rule set; this is the unrolled-winding orientation analysis —
/// the developable generalization of the Newell invariant.
///
/// Soundness of the per-edge angular steps: arcs carry their exact signed
/// sweep (their circle axis is parallel to the cylinder axis — checked);
/// segment chords take the principal-value step, sound while no single
/// chord subtends ≥ π around the axis (yang facet chords subtend one
/// Stage-1 facet, ≤ 2π/8). A violated assumption breaks the integrality
/// of the loop's net winding, which IS checked, loudly.
#[allow(clippy::too_many_arguments)]
fn validate_cylinder_patch(
    arena: &BrepArena,
    f: FaceId,
    face: &crate::arena::Face,
    axis_point: Point3,
    axis_dir: crate::arena::UnitVector3,
    radius: f64,
    reversed: bool,
) -> Result<(), KernelV2Error> {
    use std::f64::consts::PI;
    let mismatch = |reason: &'static str| KernelV2Error::CurvedGeometryMismatch { face: f, reason };
    let a = [axis_dir.x, axis_dir.y, axis_dir.z];
    let ap = [axis_point.x(), axis_point.y(), axis_point.z()];
    // The mirror sense: a cavity wall (reversed) is validated in the
    // mirrored frame u = −θ, where its boundary winds material-CCW again.
    let sense = if reversed { -1.0 } else { 1.0 };

    let mut all_loops = vec![face.outer_loop];
    all_loops.extend(face.inner_loops.iter().copied());

    // Shared angular frame: e1 from the first outer-loop vertex's radial
    // direction (each loop only needs internal consistency, but one shared
    // frame keeps the analysis deterministic and debuggable).
    let radial_theta_h = |p: Point3, e1: [f64; 3], e2: [f64; 3]| -> Option<(f64, f64)> {
        let d = [p.x() - ap[0], p.y() - ap[1], p.z() - ap[2]];
        let h = d[0] * a[0] + d[1] * a[1] + d[2] * a[2];
        let r = [d[0] - h * a[0], d[1] - h * a[1], d[2] - h * a[2]];
        let rl = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt();
        if !(rl.is_finite() && rl > 0.0) {
            return None;
        }
        let x = r[0] * e1[0] + r[1] * e1[1] + r[2] * e1[2];
        let y = r[0] * e2[0] + r[1] * e2[1] + r[2] * e2[2];
        Some((y.atan2(x), h))
    };
    let first_hes = arena.loop_half_edges(face.outer_loop)?;
    if first_hes.is_empty() {
        return Err(mismatch("cylinder patch with an empty boundary loop"));
    }
    let p0 = arena.vertex(arena.half_edge(first_hes[0])?.origin)?.point;
    let d0 = [p0.x() - ap[0], p0.y() - ap[1], p0.z() - ap[2]];
    let h0 = d0[0] * a[0] + d0[1] * a[1] + d0[2] * a[2];
    let r0 = [d0[0] - h0 * a[0], d0[1] - h0 * a[1], d0[2] - h0 * a[2]];
    let r0l = (r0[0] * r0[0] + r0[1] * r0[1] + r0[2] * r0[2]).sqrt();
    if !(r0l.is_finite() && r0l > 0.0) {
        return Err(mismatch("cylinder patch anchor vertex lies on the axis"));
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
            return Err(mismatch("cylinder patch loop with fewer than 3 edges"));
        }
        let mut us: Vec<f64> = Vec::with_capacity(hes.len());
        let mut hs: Vec<f64> = Vec::with_capacity(hes.len());
        let mut u_cur = f64::NAN; // set from the first vertex below
        let mut total = 0.0f64;
        for (i, &h) in hes.iter().enumerate() {
            let he = arena.half_edge(h)?;
            let p = arena.vertex(he.origin)?.point;
            let q = arena.vertex(arena.half_edge(he.next)?.origin)?.point;
            let Some((theta_p, hp)) = radial_theta_h(p, e1, e2) else {
                return Err(mismatch("cylinder patch vertex lies on the axis"));
            };
            if i == 0 {
                u_cur = theta_p;
            }
            us.push(u_cur);
            hs.push(hp);

            let delta = match he.curve {
                Curve::LineSegment => {
                    let Some((theta_q, _)) = radial_theta_h(q, e1, e2) else {
                        return Err(mismatch("cylinder patch vertex lies on the axis"));
                    };
                    geom::wrap_to_pi(theta_q - theta_p)
                }
                Curve::Arc {
                    center,
                    normal,
                    radius: r_arc,
                } => {
                    // Production-tier per-arc surface agreement.
                    if (r_arc - radius).abs() > 1e-9 * radius {
                        return Err(mismatch("patch arc radius disagrees with the surface"));
                    }
                    let nd = geom::dot(normal, axis_dir);
                    if nd.abs() < 1.0 - NORMAL_AGREEMENT_TOLERANCE {
                        return Err(mismatch(
                            "patch arc's circle axis is not parallel to the cylinder axis",
                        ));
                    }
                    #[cfg(any(debug_assertions, feature = "strict-validation"))]
                    {
                        // Arc center on the axis (import band — see fn docs).
                        let dc = [center.x() - ap[0], center.y() - ap[1], center.z() - ap[2]];
                        let hc = dc[0] * a[0] + dc[1] * a[1] + dc[2] * a[2];
                        let rc = [dc[0] - hc * a[0], dc[1] - hc * a[1], dc[2] - hc * a[2]];
                        let off = (rc[0] * rc[0] + rc[1] * rc[1] + rc[2] * rc[2]).sqrt();
                        if off > import_band(radius, center) {
                            return Err(vertex_off_surface(
                                f,
                                "cylpatch-arc-center-off-axis",
                                center,
                                off,
                                import_band(radius, center),
                                &format!(
                                    "cylinder axis_point=({:.17e},{:.17e},{:.17e}) \
                                     axis=({:.17e},{:.17e},{:.17e}) r={radius:.17e}",
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
                Curve::EllipseArc {
                    center,
                    normal,
                    major_axis,
                    major_radius,
                    minor_radius,
                } => {
                    // PR-KV9: oblique-section arc on this cylinder. The
                    // azimuth advance equals the SIGNED parametric sweep:
                    // the axis-⊥ projection of a cylinder-section ellipse is
                    // the radius-r circle itself (minor radius = r, minor
                    // direction ⊥ axis), so Δazimuth = s_w·Δt with
                    // s_w = sign((n̂×m̂)·(â×ê1)) the frame handedness.
                    if (minor_radius - radius).abs() > 1e-9 * (1.0 + radius) {
                        return Err(mismatch(
                            "patch ellipse-arc minor radius disagrees with the surface",
                        ));
                    }
                    let nu = [normal.x, normal.y, normal.z];
                    let mr = [major_axis.x, major_axis.y, major_axis.z];
                    let m_dot_a = mr[0] * a[0] + mr[1] * a[1] + mr[2] * a[2];
                    let e1r = [
                        mr[0] - m_dot_a * a[0],
                        mr[1] - m_dot_a * a[1],
                        mr[2] - m_dot_a * a[2],
                    ];
                    let e1l = (e1r[0] * e1r[0] + e1r[1] * e1r[1] + e1r[2] * e1r[2]).sqrt();
                    if e1l < 1e-12 {
                        return Err(mismatch(
                            "patch ellipse-arc major axis parallel to the cylinder axis",
                        ));
                    }
                    let e1v = [e1r[0] / e1l, e1r[1] / e1l, e1r[2] / e1l];
                    let e2v = [
                        a[1] * e1v[2] - a[2] * e1v[1],
                        a[2] * e1v[0] - a[0] * e1v[2],
                        a[0] * e1v[1] - a[1] * e1v[0],
                    ];
                    let w = [
                        nu[1] * mr[2] - nu[2] * mr[1],
                        nu[2] * mr[0] - nu[0] * mr[2],
                        nu[0] * mr[1] - nu[1] * mr[0],
                    ];
                    let s_w = if w[0] * e2v[0] + w[1] * e2v[1] + w[2] * e2v[2] >= 0.0 {
                        1.0
                    } else {
                        -1.0
                    };
                    let Some(sweep) =
                        geom::ellipse_ccw_sweep(center, nu, mr, major_radius, minor_radius, p, q)
                    else {
                        return Err(mismatch("patch ellipse-arc endpoint degenerate"));
                    };
                    s_w * sweep
                }
                Curve::Circle { .. } => {
                    // Unreachable: the dispatcher sends full-circle faces to
                    // the canonical path. Loud, defensively.
                    return Err(mismatch("full-circle edge inside a partial cylinder patch"));
                }
                // KV16: a plane∩cylinder section is never a hyperbola — its
                // presence on a cylinder patch is a defect (no producer),
                // typed and loud.
                Curve::HyperbolaArc { .. } => {
                    return Err(mismatch(
                        "hyperbola arc on a cylinder patch (a plane∩cylinder section is \
                         never a hyperbola)",
                    ));
                }
                // M5: a surface-pair boundary piece advances the walk by its
                // endpoint azimuths (endpoint-determined traversal; on-curve
                // certification is invariant 1b, per-vertex surface agreement
                // is the shared off-surface sweep).
                Curve::SurfacePair { .. } => {
                    let Some((theta_q, _)) = radial_theta_h(q, e1, e2) else {
                        return Err(mismatch("cylinder patch vertex lies on the axis"));
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
                "cylinder patch loop's net axis winding is not integral",
            ));
        }
        let wraps = wraps as i64;
        if wraps.abs() > 1 {
            return Err(mismatch(
                "cylinder patch loop wraps the axis more than once",
            ));
        }
        let m = us.len();
        let mut area2 = 0.0f64;
        for i in 0..m {
            let j = (i + 1) % m;
            area2 += us[i] * hs[j] - us[j] * hs[i];
        }
        measures.push(LoopMeasure {
            loop_id: lid,
            wrap: if sense < 0.0 { -wraps } else { wraps },
            mean_h: hs.iter().sum::<f64>() / m as f64,
            area2: sense * area2,
        });
    }

    // ---- face-level orientation rules (material-CCW in the unrolled frame)
    let wrapping: Vec<&LoopMeasure> = measures.iter().filter(|mm| mm.wrap != 0).collect();
    match wrapping.len() {
        0 => {
            // Bounded patch: exactly one CCW (material) loop, others CW
            // (windows).
            let mut positive = 0usize;
            for mm in &measures {
                if mm.area2 == 0.0 {
                    return Err(mismatch("cylinder patch loop has zero unrolled area"));
                }
                if mm.area2 > 0.0 {
                    positive += 1;
                }
            }
            if positive != 1 {
                return Err(mismatch(
                    "bounded cylinder patch must have exactly one material-CCW loop",
                ));
            }
        }
        2 => {
            // Barrel segment: a +1 and a −1 wrap, the +1 at the lower axial
            // height (the generalization of the KV5a rim rule "traversal
            // axis points toward the opposite rim"); windows wind CW.
            let (w0, w1) = (wrapping[0], wrapping[1]);
            if w0.wrap + w1.wrap != 0 {
                return Err(mismatch(
                    "cylinder patch wrapping loops do not wind oppositely",
                ));
            }
            let (plus, minus) = if w0.wrap > 0 { (w0, w1) } else { (w1, w0) };
            if plus.mean_h >= minus.mean_h {
                return Err(mismatch(
                    "cylinder patch wrapping loops are oriented away from the material",
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
            // Diagnostic probe (env-gated, zero-cost off): dump the per-loop
            // wrap/height/area measures so a wrapping-count wall
            // self-localizes (which loop is unexpected, and where).
            if std::env::var_os("KV2_CYLPATCH_PROBE").is_some() {
                eprintln!(
                    "[cylpatch-probe] face {f:?} radius={radius} axis_point={ap:?} axis={a:?} \
                     wrapping={} loops={}",
                    wrapping.len(),
                    measures.len()
                );
                for mm in &measures {
                    eprintln!(
                        "  loop {:?} wrap={} mean_h={} area2={}",
                        mm.loop_id, mm.wrap, mm.mean_h, mm.area2
                    );
                    if let Ok(hes) = arena.loop_half_edges(mm.loop_id) {
                        for &h in &hes {
                            if let Ok(he) = arena.half_edge(h) {
                                let p = arena.vertex(he.origin).map(|v| v.point);
                                eprintln!("    he {h:?} curve={:?} origin={p:?}", he.curve);
                            }
                        }
                    }
                }
            }
            return Err(mismatch(
                "cylinder patch must have exactly 0 or 2 axis-wrapping loops",
            ));
        }
    }

    // ---- strict-tier geometric tripwire: loop vertices on the surface -----
    #[cfg(any(debug_assertions, feature = "strict-validation"))]
    {
        for &lid in &all_loops {
            for p in arena.loop_points(lid)? {
                let d = [p.x() - ap[0], p.y() - ap[1], p.z() - ap[2]];
                let h = d[0] * a[0] + d[1] * a[1] + d[2] * a[2];
                let r = [d[0] - h * a[0], d[1] - h * a[1], d[2] - h * a[2]];
                let rl = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt();
                if (rl - radius).abs() > import_band(radius, p) {
                    // Under `KV2_OFFSURF_PROBE`, dump the WHOLE failing loop
                    // alongside the single offending point. The off-surface
                    // classes met so far are all "one intermediate loop vertex
                    // left at its Stage-1 CHORD position while its neighbours
                    // are exact", and they are only identifiable from the
                    // NEIGHBOURS: equal axial height `h` plus a residual equal
                    // to the chord sagitta of the neighbours' angular span is
                    // the fingerprint (#195 characterization, 2026-07-28).
                    // The lone point the tripwire names cannot show that.
                    if std::env::var_os("KV2_OFFSURF_PROBE").is_some() {
                        for &l2 in &all_loops {
                            for (i, q) in arena.loop_points(l2)?.into_iter().enumerate() {
                                let dq = [q.x() - ap[0], q.y() - ap[1], q.z() - ap[2]];
                                let hq = dq[0] * a[0] + dq[1] * a[1] + dq[2] * a[2];
                                let rq = [dq[0] - hq * a[0], dq[1] - hq * a[1], dq[2] - hq * a[2]];
                                let rlq = (rq[0] * rq[0] + rq[1] * rq[1] + rq[2] * rq[2]).sqrt();
                                eprintln!(
                                    "[offsurf-loop] face {f:?} loop {l2:?} i={i} \
                                     p=({:.17e},{:.17e},{:.17e}) h={hq:.17e} resid={:.6e}",
                                    q.x(),
                                    q.y(),
                                    q.z(),
                                    (rlq - radius).abs()
                                );
                            }
                        }
                    }
                    return Err(vertex_off_surface(
                        f,
                        "cylpatch-vertex",
                        p,
                        (rl - radius).abs(),
                        import_band(radius, p),
                        &format!(
                            "cylinder axis_point=({:.17e},{:.17e},{:.17e}) \
                             axis=({:.17e},{:.17e},{:.17e}) r={radius:.17e}",
                            ap[0], ap[1], ap[2], a[0], a[1], a[2]
                        ),
                    ));
                }
            }
        }
    }
    Ok(())
}
