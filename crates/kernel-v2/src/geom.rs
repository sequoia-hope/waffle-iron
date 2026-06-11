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
    let mut three_pi = RBig::ZERO; // exact coefficient of π/3
    let solid_ref = arena.solid(solid)?;
    for &sh in &solid_ref.shells {
        for &f in &arena.shell(sh)?.faces {
            let face = arena.face(f)?;

            // Gather each loop's circle half-edges (with curve data).
            let mut loops = vec![face.outer_loop];
            loops.extend(face.inner_loops.iter().copied());
            let mut loop_data = Vec::with_capacity(loops.len());
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
                        // PR-KV5b partial patches: no analytic closed form
                        // is implemented for arc-bounded loops — loud,
                        // never a silent polygonal fan over arc chords.
                        Curve::Arc { .. } => {
                            return Err(crate::error::KernelV2Error::CurvedGeometryMismatch {
                                face: f,
                                reason: "signed_volume: analytic volume not implemented for \
                                     arc-bounded faces (KV5b partial patches)",
                            });
                        }
                        Curve::LineSegment => {}
                    }
                }
                loop_data.push((lid, hes.len(), circles));
            }

            if let Some(Surface::Cylinder { reversed: true, .. }) = face.surface {
                return Err(crate::error::KernelV2Error::CurvedGeometryMismatch {
                    face: f,
                    reason: "signed_volume: cavity-sense (reversed) cylinder faces are \
                             KV5b partial patches with no analytic closed form",
                });
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
    Ok(six_v / 6.0 + pi_coeff * std::f64::consts::PI)
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
