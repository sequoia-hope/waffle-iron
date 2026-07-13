//! Minimal vector helpers for the KV1 slice (Newell normal, dot/cross).
//!
//! Local to kernel-v2 until `cad-primitives` grows vector arithmetic.
//! These are plain f64 computations: the **polygon walk direction is the
//! source of truth** for face orientation (crate hard rule 5); the Newell
//! normal is the standard robust way to extract that orientation from the
//! walk (Stroud 2006 §E.9 "polyareavec" computes the same quantity as a
//! cross-product area sum).

use crate::arena::UnitVector3;
use cad_primitives::Point3;

mod conics;
mod flux;
pub(crate) use conics::*;
pub(crate) use flux::*;

/// A loop's Newell normal must exceed this (squared-norm) floor before it is
/// considered orientable and a `Plane` is stored on the face. Below the
/// floor the face keeps `surface: None` ("under construction").
///
/// The floor is effectively an exact-zero test: the degenerate loops that
/// arise during Euler construction (lone vertices; spur paths walked out and
/// back) cancel **exactly** in the Newell sum, because each doubled edge
/// contributes two term-wise-identical products of opposite sign.
pub const NEWELL_MIN_SQ_NORM: f64 = 1e-60;

/// Raw (unnormalized) Newell normal of a polygon walk:
/// `N = Σ_i P_i × P_{i+1}` (cyclic).
///
/// For a planar CCW loop this is `2·area·n̂`. Defined for non-planar loops
/// too (best-fit orientation), which is what faces mid-construction have.
pub fn newell(points: &[Point3]) -> [f64; 3] {
    let mut n = [0.0f64; 3];
    for (i, p) in points.iter().enumerate() {
        let q = points[(i + 1) % points.len()];
        n[0] += p.y() * q.z() - p.z() * q.y();
        n[1] += p.z() * q.x() - p.x() * q.z();
        n[2] += p.x() * q.y() - p.y() * q.x();
    }
    n
}

/// Normalized Newell normal, or `None` if the loop is degenerate
/// (fewer than 3 points, or squared norm below [`NEWELL_MIN_SQ_NORM`]).
pub fn newell_unit(points: &[Point3]) -> Option<UnitVector3> {
    if points.len() < 3 {
        return None;
    }
    let n = newell(points);
    let sq = n[0] * n[0] + n[1] * n[1] + n[2] * n[2];
    if sq < NEWELL_MIN_SQ_NORM {
        return None;
    }
    let len = sq.sqrt();
    Some(UnitVector3 {
        x: n[0] / len,
        y: n[1] / len,
        z: n[2] / len,
    })
}

/// Dot product of two unit vectors.
pub fn dot(a: UnitVector3, b: UnitVector3) -> f64 {
    a.x * b.x + a.y * b.y + a.z * b.z
}

/// Radial distance from the axis to a right-circular-cone surface at axial
/// coordinate `tau` (the signed distance from the apex measured along the
/// axis): `tau · tan(half_angle)`. `validate_cone_face` uses it to check rim
/// radii and vertex-on-surface residuals (KV6c). `half_angle ∈ (0, π/2)`, so
/// the value is positive exactly on the `+axis` nappe (`tau > 0`).
pub fn cone_radius_at(tau: f64, half_angle: f64) -> f64 {
    tau * half_angle.tan()
}

/// Signed tube residual of a point on/near a torus (KV6d): with `tau` the axial
/// coordinate `(p − center)·axis_dir` and `rho` the radial distance from the
/// axis, returns `(rho − major)² + tau² − minor²` — zero on the surface,
/// negative inside the tube, positive outside. `validate_torus_face` and the
/// boolean in/out test use it.
pub fn torus_residual(tau: f64, rho: f64, major: f64, minor: f64) -> f64 {
    let d = rho - major;
    d * d + tau * tau - minor * minor
}

/// Signed radial residual of a point on/near a sphere (KV6d increment 2):
/// `|p − center| − radius` — zero on the surface, negative inside, positive
/// outside (plain length units, unlike the length² [`torus_residual`]).
/// `validate_sphere_face` and the sphere tessellators use it.
pub fn sphere_residual(p: Point3, center: Point3, radius: f64) -> f64 {
    let d = [p.x() - center.x(), p.y() - center.y(), p.z() - center.z()];
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() - radius
}

/// Wrap an angle to the principal interval `(−π, π]`.
pub(crate) fn wrap_to_pi(mut x: f64) -> f64 {
    use std::f64::consts::PI;
    while x > PI {
        x -= 2.0 * PI;
    }
    while x <= -PI {
        x += 2.0 * PI;
    }
    x
}

/// CCW sweep angle in `(0, 2π]` around unit `normal` from `start` to `end`
/// (both on the circle about `center`; `start == end` geometrically yields
/// `2π`). `None` when either endpoint has no radial component (degenerate
/// input). Shared by the KV5b boolean conversion (arc sense derivation),
/// the curved validation (unrolled winding), and arc tessellation.
pub(crate) fn ccw_sweep(
    center: Point3,
    normal: [f64; 3],
    start: Point3,
    end: Point3,
) -> Option<f64> {
    let ds = [
        start.x() - center.x(),
        start.y() - center.y(),
        start.z() - center.z(),
    ];
    let t = ds[0] * normal[0] + ds[1] * normal[1] + ds[2] * normal[2];
    let e1_raw = [
        ds[0] - t * normal[0],
        ds[1] - t * normal[1],
        ds[2] - t * normal[2],
    ];
    let l1 = (e1_raw[0] * e1_raw[0] + e1_raw[1] * e1_raw[1] + e1_raw[2] * e1_raw[2]).sqrt();
    if !(l1.is_finite() && l1 > 0.0) {
        return None;
    }
    let e1 = [e1_raw[0] / l1, e1_raw[1] / l1, e1_raw[2] / l1];
    let e2 = [
        normal[1] * e1[2] - normal[2] * e1[1],
        normal[2] * e1[0] - normal[0] * e1[2],
        normal[0] * e1[1] - normal[1] * e1[0],
    ];
    let de = [
        end.x() - center.x(),
        end.y() - center.y(),
        end.z() - center.z(),
    ];
    let x = de[0] * e1[0] + de[1] * e1[1] + de[2] * e1[2];
    let y = de[0] * e2[0] + de[1] * e2[1] + de[2] * e2[2];
    if !(x.is_finite() && y.is_finite()) || (x == 0.0 && y == 0.0) {
        return None;
    }
    let mut theta = y.atan2(x);
    if theta <= 0.0 {
        theta += 2.0 * std::f64::consts::PI;
    }
    Some(theta)
}

/// Signed volume of a solid via the divergence theorem
/// (`V = (1/3) ∮ x · n dA`), evaluated **analytically per surface type** —
/// the value is a property of the B-Rep geometry, independent of any
/// tessellation:
///
/// - **Planar faces with polygonal loops**: a sum of signed tetrahedron
///   determinants `det[r, pᵢ, pᵢ₊₁] / 6` fanned from an in-plane reference
///   point over ALL of the face's loops — rings wind opposite the outer
///   loop, so holes subtract automatically. (Bit-identical to the KV2
///   implementation for all-planar solids.)
/// - **Planar disks bounded by a full-circle edge** (PR-KV5a): the exact
///   flux through a disk with boundary traversed CCW around the circle's
///   directional normal `ν` is `(1/3)(c · ν) π r²` — reference-free, and
///   ring disks subtract automatically because their `ν` opposes the face
///   normal.
/// - **Cylinder laterals**: `x · n = c₀ · r̂ + ρ` on the surface and
///   `∮ r̂ dθ = 0`, so the exact flux is `(1/3) · 2π ρ² ℓ` with
///   `ℓ = (c_other − c_this) · ν` taken from the bottom rim (positive for
///   outward laterals).
/// - **Cone (frustum) laterals** (PR-KV6c): with outward normal
///   `cos α·r̂ − sin α·axis`, the exact flux is
///   `−(1/3)·π·(apex·axis)·(ρ_hi² − ρ_lo²)` (rim radii ordered by axial
///   coordinate). The `apex·axis` terms of the lateral and its two caps
///   cancel to the analytic frustum volume `(π·H/3)(ρ₀² + ρ₀ρ₁ + ρ₁²)`.
///
/// The π-terms are accumulated as an **exact `dashu` rational coefficient**
/// (every `f64` converts losslessly), so algebraic cancellations — e.g. the
/// cap terms `(c₁·a − c₀·a) r²/3` combining with the lateral `2r²ℓ/3` into
/// exactly `r²ℓ` — happen exactly, and the result rounds ONCE:
/// `volume = six_v/6 + to_f64(coeff) · π`. For an axis-aligned cylinder
/// whose `r²h` is exactly representable this yields **bitwise** `π·r²·h`.
///
/// Positive for outward-oriented closed solids; this is the orientation
/// oracle for the constructors and the tessellation/boolean checks.
///
/// Production code: returns `Err` on dead ids / corrupted loops, and
/// `Err(CurvedGeometryMismatch)` on curved configurations outside the
/// validated vocabulary (mixed circle/segment loops, ≠ 2 cylinder rims). It
/// does NOT validate closedness — call `validate_solid` for that (an open
/// or inward-oriented surface simply yields a meaningless / negative
/// value).
pub fn signed_volume(
    arena: &crate::arena::BrepArena,
    solid: crate::arena::SolidId,
) -> Result<f64, crate::error::KernelV2Error> {
    use crate::arena::{Curve, Surface};
    use crate::exact2d::r as rq;
    use dashu::rational::RBig;

    let mut six_v = 0.0f64; // polygonal-loop fan determinants
    let mut flux_f64 = 0.0f64; // arc-loop closed forms (PR-KV6a, plain f64)
    let mut three_pi = RBig::ZERO; // exact coefficient of π/3
    let solid_ref = arena.solid(solid)?;
    for &sh in &solid_ref.shells {
        for &f in &arena.shell(sh)?.faces {
            let face = arena.face(f)?;

            // Gather each loop's circle half-edges (with curve data) and
            // whether any loop carries arcs.
            let mut loops = vec![face.outer_loop];
            loops.extend(face.inner_loops.iter().copied());
            let mut loop_data = Vec::with_capacity(loops.len());
            let mut has_arcs = false;
            for &lid in &loops {
                let hes = arena.loop_half_edges(lid)?;
                let mut circles = Vec::new();
                for &h in &hes {
                    match arena.half_edge(h)?.curve {
                        Curve::Circle {
                            center,
                            normal,
                            radius,
                        } => circles.push((center, normal, radius)),
                        Curve::Arc { .. }
                        | Curve::EllipseArc { .. }
                        | Curve::HyperbolaArc { .. }
                        | Curve::SurfacePair { .. } => has_arcs = true,
                        Curve::LineSegment => {}
                    }
                }
                loop_data.push((lid, hes.len(), circles));
            }

            // PR-KV6a closed forms for arc-bearing faces (the revolve
            // vocabulary). Anything arc-bearing OUTSIDE that vocabulary
            // (e.g. boolean-output patches whose segments are chords, not
            // rulings) stays a loud typed rejection — never a silent
            // chord-fan approximation.
            if has_arcs {
                match face.surface {
                    Some(Surface::Plane(plane)) => {
                        flux_f64 += planar_arc_face_flux(arena, f, face, plane)?;
                    }
                    Some(Surface::Cylinder {
                        axis_point,
                        axis_dir,
                        radius,
                        ..
                    }) => {
                        // The traversal direction + rim normals already
                        // encode the material sense; no explicit `reversed`
                        // factor (see the derivation on the helper).
                        flux_f64 +=
                            cylinder_arc_patch_flux(arena, f, face, axis_point, axis_dir, radius)?;
                    }
                    Some(Surface::Cone {
                        apex,
                        axis_dir,
                        half_angle,
                        ..
                    }) => {
                        // KV6c increment 5: the partial-revolve oblique wall.
                        // Traversal + arc senses encode the material sense; no
                        // explicit `reversed` factor (see the helper's docs).
                        flux_f64 +=
                            cone_arc_patch_flux(arena, f, face, apex, axis_dir, half_angle)?;
                    }
                    _ => {
                        return Err(crate::error::KernelV2Error::CurvedGeometryMismatch {
                            face: f,
                            reason: "signed_volume: arc-bounded face without a typed surface",
                        });
                    }
                }
                continue;
            }

            if let Some(Surface::Cylinder { .. }) = face.surface {
                // Lateral: exactly two full-circle rims (validated shape).
                let rims: Vec<_> = loop_data
                    .iter()
                    .flat_map(|(_, _, c)| c.iter().copied())
                    .collect();
                if rims.len() != 2 {
                    return Err(crate::error::KernelV2Error::CurvedGeometryMismatch {
                        face: f,
                        reason: "signed_volume: cylinder face without exactly two rims",
                    });
                }
                let (c0, nu, rad) = rims[0];
                let (c1, _, _) = rims[1];
                let ell = (rq(c1.x()) - rq(c0.x())) * rq(nu.x)
                    + (rq(c1.y()) - rq(c0.y())) * rq(nu.y)
                    + (rq(c1.z()) - rq(c0.z())) * rq(nu.z);
                three_pi += RBig::from(2) * rq(rad) * rq(rad) * ell;
                continue;
            }

            if let Some(Surface::Cone {
                apex,
                axis_dir,
                reversed,
                ..
            }) = face.surface
            {
                // Frustum lateral: exactly two full-circle rims (validated
                // shape). With the outward normal `cos α·r̂ − sin α·axis`, the
                // divergence-theorem flux is
                // `(1/3)∮ x·n̂ dA = −(π/3)·(apex·axis)·(ρ_hi² − ρ_lo²)`, the rim
                // radii ordered by axial coordinate `τ = (center − apex)·axis`
                // (axis points away from the apex ⇒ τ > 0). A `reversed` bore
                // wall has the opposite outward normal, negating the flux. The
                // `apex·axis` terms of an outward frustum's lateral and its two
                // flat caps cancel to the analytic frustum volume
                // `(π·H/3)(ρ₀² + ρ₀ρ₁ + ρ₁²)` — so a sign error here shows up as
                // a wrong total on any apex off the coordinate origin.
                let rims: Vec<_> = loop_data
                    .iter()
                    .flat_map(|(_, _, c)| c.iter().copied())
                    .collect();
                let tau = |c: Point3| {
                    (c.x() - apex.x()) * axis_dir.x
                        + (c.y() - apex.y()) * axis_dir.y
                        + (c.z() - apex.z()) * axis_dir.z
                };
                // KV6 slice 2B: the APEX form has a single base rim — the
                // apex is the ρ = 0 end (τ = 0), so the same flux formula
                // applies with ρ_lo = 0.
                let (rlo, rhi) = match rims[..] {
                    [(c0, _, r0), (c1, _, r1)] => {
                        if tau(c0) <= tau(c1) {
                            (r0, r1)
                        } else {
                            (r1, r0)
                        }
                    }
                    [(_, _, r0)] => (0.0, r0),
                    _ => {
                        return Err(crate::error::KernelV2Error::CurvedGeometryMismatch {
                            face: f,
                            reason: "signed_volume: cone face without one or two rims",
                        });
                    }
                };
                let a_dot = rq(apex.x()) * rq(axis_dir.x)
                    + rq(apex.y()) * rq(axis_dir.y)
                    + rq(apex.z()) * rq(axis_dir.z);
                let signed = if reversed { a_dot } else { -a_dot };
                three_pi += signed * (rq(rhi) * rq(rhi) - rq(rlo) * rq(rlo));
                continue;
            }

            // Planar(-ish) face: per-loop dispatch. Reference point for
            // polygonal fans: the outer loop's first vertex if polygonal,
            // else the outer circle's center (both lie in the face plane).
            let ref_pt = if loop_data[0].2.is_empty() {
                arena.loop_points(face.outer_loop)?.first().copied()
            } else {
                Some(loop_data[0].2[0].0)
            };
            for (lid, he_count, circles) in &loop_data {
                if circles.is_empty() {
                    let pts = arena.loop_points(*lid)?;
                    if let Some(rp) = ref_pt {
                        six_v += loop_fan_determinants(rp, &pts);
                    }
                } else if *he_count == 1 {
                    let (c, nu, rad) = circles[0];
                    three_pi +=
                        (rq(c.x()) * rq(nu.x) + rq(c.y()) * rq(nu.y) + rq(c.z()) * rq(nu.z))
                            * rq(rad)
                            * rq(rad);
                } else {
                    return Err(crate::error::KernelV2Error::CurvedGeometryMismatch {
                        face: f,
                        reason: "signed_volume: loop mixes circle and segment edges",
                    });
                }
            }
        }
    }
    let pi_coeff = (three_pi / RBig::from(3)).to_f64().value();
    Ok(six_v / 6.0 + flux_f64 + pi_coeff * std::f64::consts::PI)
}

/// Divergence-theorem flux `(1/3)∮ x·n dA` through a PLANAR face whose
/// loops mix [`Curve::Arc`] and [`Curve::LineSegment`] edges (PR-KV6a —
/// revolve annular sectors; also sound for KV5b boolean outputs).
///
/// On the plane `x·n̂ = d` is constant, so the flux is `(d/3)·A` with `A`
/// the signed area of all loops as traversed: the chord-polygon shoelace
/// (projected on n̂) plus, per arc, the circular-segment correction
/// `½r²(Δθ − sin Δθ)` signed by the arc's traversal sense relative to the
/// face normal. Plain f64 closed form (`sin` is transcendental — no
/// rational trick applies); exact areas at ~1e-15 relative.
fn planar_arc_face_flux(
    arena: &crate::arena::BrepArena,
    f: crate::arena::FaceId,
    face: &crate::arena::Face,
    plane: crate::arena::Plane,
) -> Result<f64, crate::error::KernelV2Error> {
    let n = [plane.normal.x, plane.normal.y, plane.normal.z];
    let d = plane.point.x() * n[0] + plane.point.y() * n[1] + plane.point.z() * n[2];
    let area2 = planar_face_signed_area2(arena, f, face, n)?;
    Ok(d * area2 / 6.0) // (d/3) · (area2/2)
}

/// Twice the signed area of a planar face's loops (outer + rings) as
/// traversed, projected on the face normal: chord shoelace plus, per
/// [`Curve::Arc`], the circular-segment correction `½r²(Δθ − sin Δθ)`
/// signed by the arc's traversal sense. Exact closed form for arc-free
/// AND arc-bearing loops (PR-KV6a); shared by `signed_volume` and the
/// adapter's face signatures.
pub(crate) fn planar_face_signed_area2(
    arena: &crate::arena::BrepArena,
    f: crate::arena::FaceId,
    face: &crate::arena::Face,
    n: [f64; 3],
) -> Result<f64, crate::error::KernelV2Error> {
    use crate::arena::Curve;
    let mut loops = vec![face.outer_loop];
    loops.extend(face.inner_loops.iter().copied());
    let mut area2 = 0.0f64; // twice the signed area
    for lid in loops {
        let hes = arena.loop_half_edges(lid)?;
        for &h in &hes {
            let he = arena.half_edge(h)?;
            let p0 = arena.vertex(he.origin)?.point;
            let p1 = arena.vertex(arena.half_edge(he.next)?.origin)?.point;
            // Chord shoelace term (projected on n̂): n̂ · (p0 × p1).
            area2 += n[0] * (p0.y() * p1.z() - p0.z() * p1.y())
                + n[1] * (p0.z() * p1.x() - p0.x() * p1.z())
                + n[2] * (p0.x() * p1.y() - p0.y() * p1.x());
            match he.curve {
                Curve::LineSegment => {}
                Curve::Arc {
                    center,
                    normal,
                    radius,
                } => {
                    let nu = [normal.x, normal.y, normal.z];
                    let Some(sweep) = ccw_sweep(center, nu, p0, p1) else {
                        return Err(crate::error::KernelV2Error::CurvedGeometryMismatch {
                            face: f,
                            reason: "signed_volume: degenerate arc endpoints",
                        });
                    };
                    // Segment area sign: + when the arc's CCW axis agrees
                    // with the face normal (it bulges material-outward),
                    // − against.
                    let sign = if nu[0] * n[0] + nu[1] * n[1] + nu[2] * n[2] >= 0.0 {
                        1.0
                    } else {
                        -1.0
                    };
                    area2 += sign * radius * radius * (sweep - sweep.sin());
                }
                Curve::EllipseArc {
                    center,
                    normal,
                    major_axis,
                    major_radius,
                    minor_radius,
                } => {
                    // PR-KV9: elliptical-segment correction. With the
                    // parametric sweep Δt (CCW around the stored traversal
                    // normal), sector = (ab/2)·Δt and the inscribed triangle
                    // is (ab/2)·sin Δt (cross of the two parametric radius
                    // vectors), so the chord-to-arc correction to TWICE the
                    // area is `ab·(Δt − sin Δt)` — the exact ellipse analog
                    // of the circular `r²(Δθ − sin Δθ)`.
                    let nu = [normal.x, normal.y, normal.z];
                    let Some(sweep) = ellipse_ccw_sweep(
                        center,
                        nu,
                        [major_axis.x, major_axis.y, major_axis.z],
                        major_radius,
                        minor_radius,
                        p0,
                        p1,
                    ) else {
                        return Err(crate::error::KernelV2Error::CurvedGeometryMismatch {
                            face: f,
                            reason: "signed_volume: degenerate ellipse-arc endpoints",
                        });
                    };
                    let sign = if nu[0] * n[0] + nu[1] * n[1] + nu[2] * n[2] >= 0.0 {
                        1.0
                    } else {
                        -1.0
                    };
                    area2 += sign * major_radius * minor_radius * (sweep - sweep.sin());
                }
                Curve::HyperbolaArc {
                    center,
                    normal,
                    major_axis,
                    semi_transverse,
                    semi_conjugate,
                } => {
                    // KV16: hyperbolic-segment correction. Sector (center to
                    // arc) = (ab/2)·Δt and the inscribed triangle's cross is
                    // (ab/2)·sinh Δt, so the chord-to-arc correction to TWICE
                    // the area is `ab·(Δt − sinh Δt)` — the hyperbolic analog
                    // of the ellipse's `ab·(Δt − sin Δt)`. `Δt` is signed in
                    // the STORED frame (traversal is endpoint-determined) and
                    // the correction is odd in Δt, so the frame-agreement
                    // sign multiplies through exactly as for the ellipse.
                    let nu = [normal.x, normal.y, normal.z];
                    let mr = [major_axis.x, major_axis.y, major_axis.z];
                    let (Some(t0), Some(t1)) = (
                        hyperbola_param(center, nu, mr, semi_conjugate, p0),
                        hyperbola_param(center, nu, mr, semi_conjugate, p1),
                    ) else {
                        return Err(crate::error::KernelV2Error::CurvedGeometryMismatch {
                            face: f,
                            reason: "signed_volume: degenerate hyperbola-arc endpoints",
                        });
                    };
                    let dt = t1 - t0;
                    let sign = if nu[0] * n[0] + nu[1] * n[1] + nu[2] * n[2] >= 0.0 {
                        1.0
                    } else {
                        -1.0
                    };
                    area2 += sign * semi_transverse * semi_conjugate * (dt - dt.sinh());
                }
                Curve::Circle { .. } => {
                    return Err(crate::error::KernelV2Error::CurvedGeometryMismatch {
                        face: f,
                        reason: "signed_volume: loop mixes full circles with arcs",
                    });
                }
                // M5: a transversal quadric-pair curve is never planar
                // (degenerate configurations produce conics upstream) —
                // its presence on a planar face is a defect, not a
                // missing closed form.
                Curve::SurfacePair { .. } => {
                    return Err(crate::error::KernelV2Error::CurvedGeometryMismatch {
                        face: f,
                        reason: "signed_volume: surface-pair edge on a planar face",
                    });
                }
            }
        }
    }
    Ok(area2)
}

/// Signed implicit residual and unit gradient of a [`PairSurface`] at `p`
/// (M5 surface-pair curve, `specs/m5_surface_pair_curve.md`). For a
/// cylinder: `f(p) = dist(p, axis) − r`, `∇f = radial unit direction`
/// ([#24] Yang et al. 2025 §4.3 — the local Newton projection operates on
/// exactly these implicit forms). `None` when the gradient is undefined
/// (point on the axis).
pub(crate) fn pair_surface_residual_gradient(
    s: &crate::arena::PairSurface,
    p: [f64; 3],
) -> Option<(f64, [f64; 3])> {
    match *s {
        crate::arena::PairSurface::Cylinder {
            axis_point,
            axis_dir,
            radius,
        } => {
            let a = [axis_dir.x, axis_dir.y, axis_dir.z];
            let d = [
                p[0] - axis_point.x(),
                p[1] - axis_point.y(),
                p[2] - axis_point.z(),
            ];
            let t = d[0] * a[0] + d[1] * a[1] + d[2] * a[2];
            let r = [d[0] - t * a[0], d[1] - t * a[1], d[2] - t * a[2]];
            let rl = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt();
            if !(rl.is_finite() && rl > 0.0) {
                return None;
            }
            Some((rl - radius, [r[0] / rl, r[1] / rl, r[2] / rl]))
        }
        // Cone (M5 cone-pair): the TRUE signed distance to the nearest
        // generator, `f = radial·cosα − |h|·sinα` (zero on both nappes), whose
        // gradient `cosα·r̂ − sign(h)·sinα·â` is ALREADY unit — so the shared
        // Gauss-Newton step (`x -= f·ĝ`, which assumes `f` is a distance along
        // the unit gradient) is exact, exactly as for the cylinder's radial
        // form. `None` on the apex/axis (radial direction undefined).
        crate::arena::PairSurface::Cone {
            apex,
            axis_dir,
            half_angle,
        } => {
            let a = [axis_dir.x, axis_dir.y, axis_dir.z];
            let d = [p[0] - apex.x(), p[1] - apex.y(), p[2] - apex.z()];
            let h = d[0] * a[0] + d[1] * a[1] + d[2] * a[2];
            let r = [d[0] - h * a[0], d[1] - h * a[1], d[2] - h * a[2]];
            let rl = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt();
            if !(rl.is_finite() && rl > 0.0) {
                return None;
            }
            let sgn = if h >= 0.0 { 1.0 } else { -1.0 };
            let (sa, ca) = half_angle.sin_cos();
            let residual = rl * ca - h.abs() * sa;
            // Unit gradient cosα·r̂ − sign(h)·sinα·â (|·| = √(cos²+sin²) = 1).
            let g = [
                ca * r[0] / rl - sgn * sa * a[0],
                ca * r[1] / rl - sgn * sa * a[1],
                ca * r[2] / rl - sgn * sa * a[2],
            ];
            Some((residual, g))
        }
        // Sphere (F10): the exact signed distance `|x − center| − radius`,
        // whose gradient is the unit radial `(x − center)/|x − center|` — so
        // the shared Gauss-Newton step is exact, exactly as for the cylinder.
        // `None` at the center (radial direction undefined).
        crate::arena::PairSurface::Sphere { center, radius } => {
            let d = [p[0] - center.x(), p[1] - center.y(), p[2] - center.z()];
            let dl = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
            if !(dl.is_finite() && dl > 0.0) {
                return None;
            }
            Some((dl - radius, [d[0] / dl, d[1] / dl, d[2] / dl]))
        }
    }
}

/// The characteristic length of a [`PairSurface`] — the scale that enters
/// residual bands, mirroring the circle/ellipse import bands. A cylinder's is
/// its radius; a cone has no constant radius (its local radius grows with axial
/// distance), so it contributes no fixed length and the band is left to the
/// point's own coordinate magnitude (`m` in [`import_band`]), which tracks the
/// cone's local scale.
pub(crate) fn pair_surface_scale(s: &crate::arena::PairSurface) -> f64 {
    match *s {
        crate::arena::PairSurface::Cylinder { radius, .. } => radius,
        crate::arena::PairSurface::Cone { .. } => 0.0,
        // A sphere has a constant radius, like the cylinder (F10).
        crate::arena::PairSurface::Sphere { radius, .. } => radius,
    }
}

/// `Σᵢ det[r, pᵢ, pᵢ₊₁]` (cyclic) — six times the signed volume contribution
/// of the triangle fan from `r` over one loop.
fn loop_fan_determinants(r: Point3, pts: &[Point3]) -> f64 {
    let mut sum = 0.0f64;
    for (i, p) in pts.iter().enumerate() {
        let q = pts[(i + 1) % pts.len()];
        // det[r, p, q] = r · (p × q)
        sum += r.x() * (p.y() * q.z() - p.z() * q.y())
            + r.y() * (p.z() * q.x() - p.x() * q.z())
            + r.z() * (p.x() * q.y() - p.y() * q.x());
    }
    sum
}

/// Arithmetic-mean centroid of a face's outer-loop vertices. Sufficient for
/// the outward-normal oracles (`normal · (face_centroid − solid_centroid)`)
/// and for tessellation seeding in later slices; NOT an area centroid.
pub fn face_centroid(
    arena: &crate::arena::BrepArena,
    face: crate::arena::FaceId,
) -> Result<Point3, crate::error::KernelV2Error> {
    let outer = arena.face(face)?.outer_loop;
    match arena.loop_(outer)?.boundary {
        crate::arena::LoopBoundary::Lone(v) => Ok(arena.vertex(v)?.point),
        crate::arena::LoopBoundary::Edges(_) => {
            let pts = arena.loop_points(outer)?;
            let n = pts.len() as f64;
            let mut s = [0.0f64; 3];
            for p in &pts {
                s[0] += p.x();
                s[1] += p.y();
                s[2] += p.z();
            }
            Ok(Point3::new(s[0] / n, s[1] / n, s[2] / n))
        }
    }
}

/// Rodrigues rotation of `p` about the axis (`center`, unit `axis`) by
/// `theta` (right-handed).
pub(crate) fn rotate_about_axis(center: Point3, axis: [f64; 3], p: Point3, theta: f64) -> Point3 {
    let v = [p.x() - center.x(), p.y() - center.y(), p.z() - center.z()];
    let (c, s) = (theta.cos(), theta.sin());
    let dot = axis[0] * v[0] + axis[1] * v[1] + axis[2] * v[2];
    let cx = [
        axis[1] * v[2] - axis[2] * v[1],
        axis[2] * v[0] - axis[0] * v[2],
        axis[0] * v[1] - axis[1] * v[0],
    ];
    Point3::new(
        center.x() + v[0] * c + cx[0] * s + axis[0] * dot * (1.0 - c),
        center.y() + v[1] * c + cx[1] * s + axis[1] * dot * (1.0 - c),
        center.z() + v[2] * c + cx[2] * s + axis[2] * dot * (1.0 - c),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_volume_of_frustum_matches_analytic() {
        use crate::arena::UnitVector3;
        use crate::cone_fixtures::build_frustum;
        use std::f64::consts::{FRAC_PI_4, PI};
        // 45° frustum, rims at radii 1 and 2 (τ = 1..2 ⇒ H = 1). Analytic
        // frustum volume = (π·H/3)(r0² + r0·r1 + r1²) = (π/3)(1 + 2 + 4) = 7π/3.
        // Apex deliberately OFF the origin (z = 3) so the cone lateral's
        // (apex·axis) flux term is non-zero and must cancel against the caps.
        let plus_z = UnitVector3 {
            x: 0.0,
            y: 0.0,
            z: 1.0,
        };
        let (arena, solid, _lat) = build_frustum(
            Point3::new(5.0, -2.0, 3.0),
            plus_z,
            1.0,
            2.0,
            FRAC_PI_4,
            FRAC_PI_4,
        );
        let v = signed_volume(&arena, solid).expect("frustum volume");
        assert!(
            (v - 7.0 * PI / 3.0).abs() < 1e-9,
            "got {v}, want {}",
            7.0 * PI / 3.0
        );
    }

    #[test]
    fn hyperbola_point_param_round_trip() {
        // KV16: a tilted frame (unit, orthogonal), a = 1.5, b = 0.7.
        let n = {
            let l = (1.0f64 + 4.0 + 9.0).sqrt();
            [1.0 / l, 2.0 / l, 3.0 / l]
        };
        // m ⟂ n via Gram–Schmidt of x̂.
        let mut m = [1.0 - n[0] * n[0], -n[0] * n[1], -n[0] * n[2]];
        let ml = (m[0] * m[0] + m[1] * m[1] + m[2] * m[2]).sqrt();
        m = [m[0] / ml, m[1] / ml, m[2] / ml];
        let c = Point3::new(0.3, -0.8, 2.1);
        let (a, b) = (1.5, 0.7);
        for &t in &[-2.0, -0.6, 0.0, 0.35, 1.9] {
            let p = hyperbola_point_at(c, n, m, a, b, t);
            let tr = hyperbola_param(c, n, m, b, p).expect("param");
            assert!((tr - t).abs() < 1e-12, "t={t} round-trips to {tr}");
            let (in_plane, out_of_plane, u) = hyperbola_branch_residual(c, n, m, a, b, p);
            assert!(in_plane < 1e-12, "t={t}: in-plane residual {in_plane}");
            assert!(out_of_plane.abs() < 1e-12, "t={t}: oop {out_of_plane}");
            assert!(u > 0.0, "t={t}: on the +m̂ branch");
        }
    }

    #[test]
    fn hyperbola_branch_residual_flags_off_branch_points() {
        let n = [0.0, 0.0, 1.0];
        let m = [1.0, 0.0, 0.0];
        let c = Point3::new(0.0, 0.0, 0.0);
        let (a, b) = (2.0, 1.0);
        // The WRONG nappe (u < 0): mirrored vertex.
        let (_, _, u) = hyperbola_branch_residual(c, n, m, a, b, Point3::new(-2.0, 0.0, 0.0));
        assert!(u < 0.0);
        // Off-curve in-plane: the center (gradient-degenerate) → +∞.
        let (d, _, _) = hyperbola_branch_residual(c, n, m, a, b, c);
        assert!(d.is_infinite());
        // A point 1e-3 outside the vertex measures ≈ 1e-3.
        let (d, _, _) = hyperbola_branch_residual(c, n, m, a, b, Point3::new(2.001, 0.0, 0.0));
        assert!((d - 1e-3).abs() < 1e-6, "near-vertex distance {d}");
        // Out-of-plane offset reported directly.
        let (_, oop, _) = hyperbola_branch_residual(c, n, m, a, b, Point3::new(2.0, 0.0, 0.5));
        assert!((oop - 0.5).abs() < 1e-12);
    }

    #[test]
    fn torus_residual_zero_on_surface() {
        // Torus major R=3, minor r=1: tube center circle at ρ=3, τ=0.
        // On-surface points: (ρ,τ) ∈ {(4,0),(2,0),(3,1),(3,-1)} (the tube).
        for (rho, tau) in [(4.0, 0.0), (2.0, 0.0), (3.0, 1.0), (3.0, -1.0)] {
            assert!(
                torus_residual(tau, rho, 3.0, 1.0).abs() < 1e-12,
                "({rho},{tau})"
            );
        }
        // Inside the tube (ρ=3, τ=0 is the tube center) → negative.
        assert!(torus_residual(0.0, 3.0, 3.0, 1.0) < 0.0);
        // Outside (ρ=5, τ=0) → positive.
        assert!(torus_residual(0.0, 5.0, 3.0, 1.0) > 0.0);
    }

    #[test]
    fn cone_radius_at_is_tau_times_tan_half_angle() {
        // 45° cone: radius == axial distance from the apex.
        let q = std::f64::consts::FRAC_PI_4;
        assert!((cone_radius_at(1.0, q) - 1.0).abs() < 1e-12);
        assert!((cone_radius_at(3.0, q) - 3.0).abs() < 1e-12);
        // 30° cone: radius == τ·tan(30°) == τ/√3.
        let h = std::f64::consts::FRAC_PI_6;
        assert!((cone_radius_at(2.0, h) - 2.0 / 3.0_f64.sqrt()).abs() < 1e-12);
        // Behind the apex (τ < 0) yields a negative radius — callers reject it.
        assert!(cone_radius_at(-1.0, q) < 0.0);
    }

    #[test]
    fn newell_of_ccw_unit_square_is_plus_z() {
        let pts = [
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(1.0, 0.0, 0.0),
            Point3::new(1.0, 1.0, 0.0),
            Point3::new(0.0, 1.0, 0.0),
        ];
        assert_eq!(newell(&pts), [0.0, 0.0, 2.0]);
        let u = newell_unit(&pts).expect("orientable");
        assert_eq!((u.x, u.y, u.z), (0.0, 0.0, 1.0));
    }

    #[test]
    fn newell_of_out_and_back_path_cancels_exactly() {
        // v1 -> v2 -> v1: the doubled edge cancels term-wise.
        let pts = [Point3::new(0.3, 0.7, 0.1), Point3::new(1.9, -2.3, 4.4)];
        assert_eq!(newell(&pts), [0.0, 0.0, 0.0]);
        assert!(newell_unit(&pts).is_none());
    }

    #[test]
    fn sphere_pair_surface_residual_and_gradient() {
        // F10: PairSurface::Sphere residual = |x − c| − r, gradient = unit
        // radial. A point exactly on the surface has ~0 residual; a point at
        // radius+d has residual d; the gradient is the outward unit radial and
        // the Gauss-Newton step (x -= f·ĝ) lands back on the surface.
        use crate::arena::PairSurface;
        let s = PairSurface::Sphere {
            center: Point3::new(1.0, -2.0, 0.5),
            radius: 3.0,
        };
        // On surface: point at center + 3·x̂.
        let on = [4.0, -2.0, 0.5];
        let (res_on, g_on) = pair_surface_residual_gradient(&s, on).unwrap();
        assert!(res_on.abs() < 1e-12, "on-surface residual {res_on:e}");
        assert!((g_on[0] - 1.0).abs() < 1e-12 && g_on[1].abs() < 1e-12 && g_on[2].abs() < 1e-12);
        // Off surface by +0.5 along −ŷ from center: point = c + 3.5·(−ŷ).
        let off = [1.0, -5.5, 0.5];
        let (res_off, g_off) = pair_surface_residual_gradient(&s, off).unwrap();
        assert!((res_off - 0.5).abs() < 1e-12, "off residual {res_off}");
        // Gauss-Newton step returns to the surface.
        let stepped = [
            off[0] - res_off * g_off[0],
            off[1] - res_off * g_off[1],
            off[2] - res_off * g_off[2],
        ];
        let (res2, _) = pair_surface_residual_gradient(&s, stepped).unwrap();
        assert!(res2.abs() < 1e-12, "post-step residual {res2:e}");
        // Center is degenerate (radial undefined).
        assert!(pair_surface_residual_gradient(&s, [1.0, -2.0, 0.5]).is_none());
        // Scale is the radius.
        assert_eq!(pair_surface_scale(&s), 3.0);
    }
}
