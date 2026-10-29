//! Geometric conformance of a solid's B-Rep: how far each face's boundary
//! strays from the face's own surface (remediation plan O1, review
//! 2026-10-28 §2.1).
//!
//! This is a MEASUREMENT, not a verdict. It reports, per face, the largest
//! distance from a boundary point (every loop vertex plus interior samples of
//! every bounding edge curve) to that face's surface, and leaves the band to
//! the consumer — the assay's oracle must not inherit the kernel's own
//! tolerances, or it could not catch a tolerance that is wrong.
//!
//! Each edge bounds two faces through its two half-edges, so walking every
//! face's loops measures every edge against BOTH adjacent surfaces. That is
//! the case a vertex-only check cannot see: an arc shipped as a straight
//! chord (ledger N100 item 3) has both endpoints on the cylinder and its
//! interior off it.
//!
//! Distances are closed-form for the five surface kinds the arena carries.

use crate::arena::{BrepArena, Surface, UnitVector3};
use crate::error::KernelV2Error;
use cad_primitives::Point3;

/// Samples per full turn of a curved edge. Interior samples are what catch a
/// chord standing in for an arc; 32 per turn puts a sample within 1/64 turn
/// of any point, far denser than any defect this oracle is meant to see.
const CONFORMANCE_SAMPLES_PER_TURN: u32 = 32;

/// Interior samples on a straight edge (see [`surface_conformance`]).
const LINE_INTERIOR_SAMPLES: u32 = 7;

/// One face's measurement.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FaceSurfaceDeviation {
    /// The face.
    pub face: crate::arena::FaceId,
    /// Boundary points measured (vertices + edge samples, with repeats).
    pub samples: usize,
    /// Largest point-to-surface distance, meters.
    pub max_distance: f64,
    /// Where it was measured.
    pub at: Point3,
}

/// Measure every face of `solid` (see the module docs).
pub fn surface_conformance(
    arena: &BrepArena,
    solid: crate::arena::SolidId,
) -> Result<Vec<FaceSurfaceDeviation>, KernelV2Error> {
    let mut out = Vec::new();
    let solid_ref = arena.solid(solid)?;
    for &sh in &solid_ref.shells {
        for &f in &arena.shell(sh)?.faces {
            let face = arena.face(f)?;
            let surface = face
                .surface
                .as_ref()
                .ok_or(KernelV2Error::FaceWithoutSurface { face: f })?;
            let mut dev = FaceSurfaceDeviation {
                face: f,
                samples: 0,
                max_distance: 0.0,
                at: Point3::new(f64::NAN, f64::NAN, f64::NAN),
            };
            let loops = std::iter::once(face.outer_loop).chain(face.inner_loops.iter().copied());
            for lid in loops {
                for h in arena.loop_half_edges(lid)? {
                    let mut pts =
                        crate::introspect::edge_polyline(arena, h, CONFORMANCE_SAMPLES_PER_TURN)?;
                    // A straight edge yields only its endpoints, which is
                    // exactly how a chord standing in for an arc hides (both
                    // ends on the cylinder, its middle off it). A legitimate
                    // straight edge lies on its surface along its whole length
                    // (a plane, a cylinder or cone ruling), so its interior is
                    // fair to sample. Curved edges are NOT densified: the
                    // midpoint between two samples of an arc is off the surface
                    // by design.
                    if matches!(arena.half_edge(h)?.curve, crate::arena::Curve::LineSegment)
                        && pts.len() == 2
                    {
                        let (a, b) = (pts[0], pts[1]);
                        let interior = (1..LINE_INTERIOR_SAMPLES + 1).map(|i| {
                            let t = i as f64 / (LINE_INTERIOR_SAMPLES + 1) as f64;
                            Point3::new(
                                a.x() + t * (b.x() - a.x()),
                                a.y() + t * (b.y() - a.y()),
                                a.z() + t * (b.z() - a.z()),
                            )
                        });
                        pts = std::iter::once(a).chain(interior).chain([b]).collect();
                    }
                    for p in pts {
                        let d = point_surface_distance(surface, p);
                        dev.samples += 1;
                        // NaN never compares greater: a non-finite distance
                        // must still surface, so it wins outright.
                        if d.is_nan() || d > dev.max_distance || dev.at.x().is_nan() {
                            dev.max_distance = if d.is_nan() { f64::INFINITY } else { d };
                            dev.at = p;
                        }
                    }
                }
            }
            out.push(dev);
        }
    }
    Ok(out)
}

fn sub(a: Point3, b: Point3) -> [f64; 3] {
    [a.x() - b.x(), a.y() - b.y(), a.z() - b.z()]
}

fn dot(a: [f64; 3], u: &UnitVector3) -> f64 {
    a[0] * u.x + a[1] * u.y + a[2] * u.z
}

fn norm(a: [f64; 3]) -> f64 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}

/// Axial coordinate `h` and radial distance `ρ` of `p` about the axis
/// through `origin` along `axis`.
fn axial_radial(p: Point3, origin: Point3, axis: &UnitVector3) -> (f64, f64) {
    let v = sub(p, origin);
    let h = dot(v, axis);
    let r = [v[0] - h * axis.x, v[1] - h * axis.y, v[2] - h * axis.z];
    (h, norm(r))
}

/// Unsigned Euclidean distance from `p` to the (infinite, untrimmed) surface.
/// The cone is its single nappe (`(p − apex)·axis_dir ≥ 0`), as the arena
/// defines it, so a point on the opposite nappe measures its true distance
/// to this one rather than zero.
pub fn point_surface_distance(surface: &Surface, p: Point3) -> f64 {
    match surface {
        Surface::Plane(pl) => dot(sub(p, pl.point), &pl.normal).abs(),
        Surface::Cylinder {
            axis_point,
            axis_dir,
            radius,
            ..
        } => {
            let (_, rho) = axial_radial(p, *axis_point, axis_dir);
            (rho - radius).abs()
        }
        Surface::Sphere { center, radius, .. } => (norm(sub(p, *center)) - radius).abs(),
        Surface::Torus {
            center,
            axis_dir,
            major_radius,
            minor_radius,
            ..
        } => {
            let (h, rho) = axial_radial(p, *center, axis_dir);
            ((rho - major_radius).hypot(h) - minor_radius).abs()
        }
        Surface::Cone {
            apex,
            axis_dir,
            half_angle,
            ..
        } => {
            // In the (ρ, h) half-plane the generator is the ray from the
            // origin along (sin α, cos α). Project onto it; behind the apex
            // the nearest point is the apex itself.
            let (h, rho) = axial_radial(p, *apex, axis_dir);
            let (s, c) = half_angle.sin_cos();
            if rho * s + h * c >= 0.0 {
                (rho * c - h * s).abs()
            } else {
                rho.hypot(h)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arena::Plane;

    fn z() -> UnitVector3 {
        UnitVector3 {
            x: 0.0,
            y: 0.0,
            z: 1.0,
        }
    }
    fn o() -> Point3 {
        Point3::new(0.0, 0.0, 0.0)
    }

    #[test]
    fn plane_distance_is_the_normal_offset() {
        let s = Surface::Plane(Plane {
            point: Point3::new(0.0, 0.0, 1.0),
            normal: z(),
        });
        assert_eq!(point_surface_distance(&s, Point3::new(5.0, -3.0, 1.0)), 0.0);
        assert!((point_surface_distance(&s, Point3::new(2.0, 2.0, 1.25)) - 0.25).abs() < 1e-15);
        assert!((point_surface_distance(&s, Point3::new(2.0, 2.0, 0.5)) - 0.5).abs() < 1e-15);
    }

    #[test]
    fn cylinder_sphere_and_torus_distances_are_radial() {
        let cyl = Surface::Cylinder {
            axis_point: o(),
            axis_dir: z(),
            radius: 2.0,
            reversed: false,
        };
        assert!(point_surface_distance(&cyl, Point3::new(2.0, 0.0, 7.0)).abs() < 1e-15);
        assert!((point_surface_distance(&cyl, Point3::new(0.0, 3.0, -1.0)) - 1.0).abs() < 1e-15);
        assert!((point_surface_distance(&cyl, o()) - 2.0).abs() < 1e-15);

        let sph = Surface::Sphere {
            center: o(),
            radius: 1.0,
            reversed: true,
        };
        assert!(point_surface_distance(&sph, Point3::new(0.0, 0.0, -1.0)).abs() < 1e-15);
        assert!((point_surface_distance(&sph, Point3::new(3.0, 0.0, 0.0)) - 2.0).abs() < 1e-15);

        let tor = Surface::Torus {
            center: o(),
            axis_dir: z(),
            major_radius: 3.0,
            minor_radius: 1.0,
            reversed: false,
        };
        // On the tube: outer equator, and the top of the tube.
        assert!(point_surface_distance(&tor, Point3::new(4.0, 0.0, 0.0)).abs() < 1e-15);
        assert!(point_surface_distance(&tor, Point3::new(0.0, 3.0, 1.0)).abs() < 1e-15);
        // The tube's centre circle is minor_radius from the surface.
        assert!((point_surface_distance(&tor, Point3::new(-3.0, 0.0, 0.0)) - 1.0).abs() < 1e-15);
    }

    #[test]
    fn cone_distance_respects_the_single_nappe() {
        // 45° cone opening along +z from the origin.
        let cone = Surface::Cone {
            apex: o(),
            axis_dir: z(),
            half_angle: std::f64::consts::FRAC_PI_4,
            reversed: false,
        };
        assert!(point_surface_distance(&cone, Point3::new(1.0, 0.0, 1.0)).abs() < 1e-15);
        // On the axis at height 2: perpendicular distance 2·sin45°.
        let d = point_surface_distance(&cone, Point3::new(0.0, 0.0, 2.0));
        assert!((d - 2.0 * std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-14);
        // The mirror point on the OTHER nappe is not on this surface: its
        // nearest point is the apex.
        let back = Point3::new(1.0, 0.0, -1.0);
        assert!((point_surface_distance(&cone, back) - std::f64::consts::SQRT_2).abs() < 1e-14);
    }
}
