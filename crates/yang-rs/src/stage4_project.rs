//! #169 Phase B — per-operand parametric charts for the mesh-update splice.
//!
//! The frame-agnostic mesh-update driver
//! ([`crate::stage4_update::two_sided_conformal_update_lifted`]) re-triangulates
//! each operand's trimmed patch in its OWN parametric domain and verifies the
//! seam in 3D. To feed it, Phase B needs, per analytic surface, a chart:
//!
//! * `project`: world `Point3` (on/near the surface) → parametric `Point2`,
//! * `lift`:    parametric `Point2` → world `Point3`,
//!
//! that are mutual inverses for points ON the surface. Then the ONE shared 3D
//! intersection curve projects into each patch's chart, the patch re-CDTs in
//! param space, and the result lifts back conformally (both sides land on the
//! same world curve → 2-manifold seam, Yang 2025 §4.4.1).
//!
//! Charts are provided for **Plane** and **Cylinder** — the pair that dominates
//! the non-2-manifold reassembly bucket and #168's degenerate-cylinder case
//! (`replan_degenerate_cylinder_patches` already uses the same cylinder (θ,z)
//! frame). Sphere / Cone / Torus return `None` for now, so the Phase-B wiring
//! simply does not engage those patches and leaves them byte-identical.
//!
//! UNWIRED: this is the projection layer Phase B's splice loop consumes; the
//! loop itself lands in a later increment behind `YANG_MESHUP_ENABLE`.

use crate::{normalize3, ortho_basis, Surface};
use cad_primitives::{Point2, Point3};

/// A parametric chart for one analytic surface: `project` world→param and `lift`
/// param→world, mutual inverses for points on the surface.
///
/// * `Plane`: param = signed coordinates in an orthonormal in-plane basis
///   `(e1, e2)` rooted at the plane's foot-of-origin. An isometry, so the CDT in
///   param space is faithful and `lift(project(p)) == p` exactly for on-plane p.
/// * `Cylinder`: param = `(θ, z)`, the unrolled surface — `θ = atan2` in the
///   axis's ortho-basis, `z` = axial coordinate. `lift` is `2π`-periodic in `θ`,
///   so it inverts `project` for on-cylinder points regardless of branch. A
///   patch that STRADDLES the `θ = ±π` seam must be unwrapped by the caller
///   before CDT (the projected boundary would otherwise self-cross); `lift`
///   itself is seam-agnostic.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(not(test), allow(dead_code))] // UNWIRED until Phase B's splice loop.
pub(crate) enum SurfaceChart {
    Plane {
        origin: [f64; 3],
        e1: [f64; 3],
        e2: [f64; 3],
    },
    Cylinder {
        axis_point: [f64; 3],
        axis: [f64; 3],
        e1: [f64; 3],
        e2: [f64; 3],
        radius: f64,
    },
}

#[cfg_attr(not(test), allow(dead_code))] // UNWIRED until Phase B's splice loop.
impl SurfaceChart {
    /// Build a chart for `surface`, or `None` for a surface type Phase B does not
    /// yet re-triangulate (Sphere / Cone / Torus). The caller keeps its existing
    /// behaviour for those (byte-identical).
    pub(crate) fn new(surface: Surface) -> Option<Self> {
        match surface {
            Surface::Plane { normal, d } => {
                let n = normalize3(normal.as_array());
                // A point on the plane: the foot of the world origin. With the
                // stored (unit) normal, `n·x + d = 0` ⇒ `x = -d·n`.
                let origin = [-d * n[0], -d * n[1], -d * n[2]];
                let (e1v, e2v) = ortho_basis(normal);
                Some(SurfaceChart::Plane {
                    origin,
                    e1: e1v.as_array(),
                    e2: e2v.as_array(),
                })
            }
            Surface::Cylinder {
                axis_point,
                axis_dir,
                radius,
            } => {
                let (e1v, e2v) = ortho_basis(axis_dir);
                Some(SurfaceChart::Cylinder {
                    axis_point: axis_point.as_array(),
                    axis: normalize3(axis_dir.as_array()),
                    e1: e1v.as_array(),
                    e2: e2v.as_array(),
                    radius,
                })
            }
            Surface::Sphere { .. } | Surface::Cone { .. } | Surface::Torus { .. } => None,
        }
    }

    /// Project a world point (assumed on/near the surface) to parametric space.
    pub(crate) fn project(&self, p: Point3) -> Point2 {
        let x = p.as_array();
        match *self {
            SurfaceChart::Plane { origin, e1, e2 } => {
                let w = [x[0] - origin[0], x[1] - origin[1], x[2] - origin[2]];
                Point2::new(dot(w, e1), dot(w, e2))
            }
            SurfaceChart::Cylinder {
                axis_point,
                axis,
                e1,
                e2,
                ..
            } => {
                let w = [
                    x[0] - axis_point[0],
                    x[1] - axis_point[1],
                    x[2] - axis_point[2],
                ];
                let z = dot(w, axis);
                let radial = [w[0] - z * axis[0], w[1] - z * axis[1], w[2] - z * axis[2]];
                let theta = dot(radial, e2).atan2(dot(radial, e1));
                Point2::new(theta, z)
            }
        }
    }

    /// Lift a parametric point back to world space (the exact inverse of
    /// [`project`](Self::project) for on-surface points).
    pub(crate) fn lift(&self, uv: Point2) -> Point3 {
        match *self {
            SurfaceChart::Plane { origin, e1, e2 } => {
                let (u, v) = (uv.x(), uv.y());
                Point3::new(
                    origin[0] + u * e1[0] + v * e2[0],
                    origin[1] + u * e1[1] + v * e2[1],
                    origin[2] + u * e1[2] + v * e2[2],
                )
            }
            SurfaceChart::Cylinder {
                axis_point,
                axis,
                e1,
                e2,
                radius,
            } => {
                let (theta, z) = (uv.x(), uv.y());
                let (ct, st) = (theta.cos(), theta.sin());
                Point3::new(
                    axis_point[0] + radius * (ct * e1[0] + st * e2[0]) + z * axis[0],
                    axis_point[1] + radius * (ct * e1[1] + st * e2[1]) + z * axis[1],
                    axis_point[2] + radius * (ct * e1[2] + st * e2[2]) + z * axis[2],
                )
            }
        }
    }
}

#[cfg_attr(not(test), allow(dead_code))] // UNWIRED until Phase B's splice loop.
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stage4_update::{
        two_sided_conformal_update_lifted, MeshUpdateOpts, Patch, Polyline, TwoSidedUpdate,
    };
    use crate::Vector3;

    fn dist3(a: Point3, b: Point3) -> f64 {
        let d = [a.x() - b.x(), a.y() - b.y(), a.z() - b.z()];
        (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
    }

    // ---- Round-trip: lift ∘ project = identity for on-surface points. -------

    #[test]
    fn plane_chart_round_trips_on_surface_points() {
        // A tilted plane through (1,0,0) with a non-axis normal.
        let n = Vector3::new(1.0, 2.0, 2.0); // |n| = 3
        let surf = Surface::Plane {
            normal: Vector3::new(1.0 / 3.0, 2.0 / 3.0, 2.0 / 3.0),
            d: -(1.0 / 3.0), // plane passes through (1,0,0): n·x + d = 1/3 - 1/3 = 0
        };
        let _ = n;
        let chart = SurfaceChart::new(surf).unwrap();
        // Points ON the plane (built by lifting arbitrary params).
        for &(u, v) in &[(0.0, 0.0), (1.5, -2.0), (-3.0, 4.0), (10.0, 10.0)] {
            let w = chart.lift(Point2::new(u, v));
            let uv2 = chart.project(w);
            assert!(
                (uv2.x() - u).abs() < 1e-12 && (uv2.y() - v).abs() < 1e-12,
                "plane project∘lift must be identity: ({u},{v}) -> {uv2:?}"
            );
            // And the lifted point lies on the plane.
            let sd = crate::signed_distance_to_surface(surf, w).unwrap();
            assert!(sd.abs() < 1e-12, "lifted point off plane by {sd}");
        }
    }

    #[test]
    fn cylinder_chart_round_trips_on_surface_points() {
        let surf = Surface::Cylinder {
            axis_point: Point3::new(0.5, -0.5, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 2.0),
            radius: 2.0,
        };
        let chart = SurfaceChart::new(surf).unwrap();
        for &(theta, z) in &[(0.0, 0.0), (1.0, 3.0), (-2.5, -1.0), (3.0, 5.0)] {
            let w = chart.lift(Point2::new(theta, z));
            // On the cylinder.
            let sd = crate::signed_distance_to_surface(surf, w).unwrap();
            assert!(sd.abs() < 1e-12, "lifted point off cylinder by {sd}");
            // project returns the same (θ mod 2π, z); lift(project(w)) == w.
            let w2 = chart.lift(chart.project(w));
            assert!(
                dist3(w, w2) < 1e-12,
                "cyl lift∘project∘lift drift {w:?} {w2:?}"
            );
        }
    }

    #[test]
    fn unsupported_surfaces_have_no_chart() {
        assert!(SurfaceChart::new(Surface::Sphere {
            center: Point3::new(0.0, 0.0, 0.0),
            radius: 1.0
        })
        .is_none());
        assert!(SurfaceChart::new(Surface::Cone {
            apex: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            half_angle: 0.5
        })
        .is_none());
        assert!(SurfaceChart::new(Surface::Torus {
            center: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            major_radius: 2.0,
            minor_radius: 0.5
        })
        .is_none());
    }

    // ---- Integration: chart + frame-agnostic driver on a REAL surface pair. --

    /// Build a rectangular patch in `chart` param space that straddles the chord
    /// `p0 → p2` (given already in param space): P0 and P2 sit on the two short
    /// boundary edges (an edge-split each), the chord runs through the interior.
    /// Works for any (possibly diagonal) chord.
    fn rect_around_chord(p0: Point2, p2: Point2, halfwidth: f64) -> Patch {
        let dir = [p2.x() - p0.x(), p2.y() - p0.y()];
        let len = (dir[0] * dir[0] + dir[1] * dir[1]).sqrt();
        let (dx, dy) = (dir[0] / len, dir[1] / len);
        // Perpendicular.
        let (px, py) = (-dy * halfwidth, dx * halfwidth);
        Patch {
            verts: vec![
                Point2::new(p0.x() - px, p0.y() - py), // 0
                Point2::new(p0.x() + px, p0.y() + py), // 1  (edge 0-1 hosts P0 at t=.5)
                Point2::new(p2.x() + px, p2.y() + py), // 2
                Point2::new(p2.x() - px, p2.y() - py), // 3  (edge 2-3 hosts P2 at t=.5)
            ],
            boundary: vec![0, 1, 2, 3],
            holes: vec![],
        }
    }

    /// A plane tangent to a cylinder shares ONE generator line (the #168 R0038
    /// geometry). Re-triangulating the plane patch and the cylinder patch against
    /// that shared generator, each in its OWN chart, produces a seam that
    /// coincides in 3D — the Phase-B two-sided update on a genuine surface pair.
    #[test]
    fn plane_tangent_cylinder_generator_is_conformal() {
        // Cylinder: axis = z, radius 2 → tangent generator at θ=0 is the line
        // x=2, y=0, z free.
        let cyl = Surface::Cylinder {
            axis_point: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            radius: 2.0,
        };
        // Tangent plane at that generator: x = 2, i.e. normal (1,0,0), d = -2.
        let plane = Surface::Plane {
            normal: Vector3::new(1.0, 0.0, 0.0),
            d: -2.0,
        };
        let chart_p = SurfaceChart::new(plane).unwrap();
        let chart_c = SurfaceChart::new(cyl).unwrap();

        // The ONE shared 3D curve: the generator (2,0,z), z = 0, 0.5, 1.
        let world: Vec<Point3> = vec![
            Point3::new(2.0, 0.0, 0.0),
            Point3::new(2.0, 0.0, 0.5),
            Point3::new(2.0, 0.0, 1.0),
        ];
        // Sanity: every world point lies on BOTH surfaces.
        for &w in &world {
            assert!(crate::signed_distance_to_surface(plane, w).unwrap().abs() < 1e-12);
            assert!(crate::signed_distance_to_surface(cyl, w).unwrap().abs() < 1e-12);
        }

        // Project into each chart.
        let pa: Vec<Point2> = world.iter().map(|&w| chart_p.project(w)).collect();
        let pc: Vec<Point2> = world.iter().map(|&w| chart_c.project(w)).collect();
        let curve_a = Polyline {
            points: pa.clone(),
            closed: false,
        };
        let curve_c = Polyline {
            points: pc.clone(),
            closed: false,
        };

        // A patch around the chord in each chart.
        let patch_a = rect_around_chord(pa[0], pa[2], 1.0);
        let patch_c = rect_around_chord(pc[0], pc[2], 0.5);

        let opts = MeshUpdateOpts {
            merge_tol: 1e-6,
            d_eps: 1e-2,
        };
        let ts: TwoSidedUpdate = two_sided_conformal_update_lifted(
            &patch_a,
            |q| chart_p.lift(q),
            &curve_a,
            &patch_c,
            |q| chart_c.lift(q),
            &curve_c,
            opts,
            1e-9,
        )
        .expect("plane-tangent-cylinder seam must be conformal");

        assert_eq!(ts.seam.len(), 3);
        // Every paired seam vertex lifts to the SAME world point — and it is the
        // original shared curve point.
        for (i, &(ia, ib)) in ts.seam.iter().enumerate() {
            let wa = chart_p.lift(ts.a.verts[ia as usize]);
            let wc = chart_c.lift(ts.b.verts[ib as usize]);
            assert!(dist3(wa, wc) < 1e-9, "seam pair {i} diverges in world");
            assert!(
                dist3(wa, world[i]) < 1e-9,
                "seam pair {i} off the shared curve"
            );
        }
    }
}
