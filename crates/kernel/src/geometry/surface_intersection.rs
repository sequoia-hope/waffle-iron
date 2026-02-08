use crate::Tolerance;

use super::curves::{Circle3d, Curve, Ellipse3d, Line3d};
use super::point::Point3d;
use super::surfaces::{Cylinder, Plane, Sphere};

/// Result of intersecting two analytic surfaces.
#[derive(Debug, Clone)]
pub enum SurfaceIntersection {
    /// Surfaces do not intersect.
    None,
    /// Surfaces touch at a single point (degenerate/tangent case).
    Point(Point3d),
    /// Surfaces intersect along a curve.
    Curve(Curve),
    /// Surfaces intersect along two separate curves (e.g. plane cutting cylinder
    /// parallel to its axis yields two lines).
    TwoCurves(Curve, Curve),
    /// Surfaces are coincident (identical geometric locus).
    Coincident,
}

// ─── Plane–Plane ─────────────────────────────────────────────────────────────

/// Intersect two infinite planes.
///
/// - Parallel, non-coincident -> `None`
/// - Coincident -> `Coincident`
/// - Transverse -> `Curve(Line3d)`
pub fn plane_plane(p1: &Plane, p2: &Plane, tol: &Tolerance) -> SurfaceIntersection {
    let cross = p1.normal.cross(&p2.normal);
    let cross_len = cross.length();

    if cross_len < tol.angular {
        // Normals are parallel — check if the planes are coincident.
        let dist = p1.distance_to_point(&p2.origin).abs();
        if dist < tol.coincidence {
            return SurfaceIntersection::Coincident;
        }
        return SurfaceIntersection::None;
    }

    let dir = cross / cross_len;

    // Find a point on the intersection line by solving the two plane equations.
    // Plane i: n_i . P = d_i
    let d1 = p1.origin.to_vec3().dot(&p1.normal);
    let d2 = p2.origin.to_vec3().dot(&p2.normal);

    let n1n2 = p1.normal.dot(&p2.normal);
    let denom = 1.0 - n1n2 * n1n2;
    // denom cannot be ~0 here because cross_len is already checked above.
    let c1 = (d1 - d2 * n1n2) / denom;
    let c2 = (d2 - d1 * n1n2) / denom;
    let origin = Point3d::ORIGIN + p1.normal * c1 + p2.normal * c2;

    SurfaceIntersection::Curve(Curve::Line(Line3d { origin, direction: dir }))
}

// ─── Plane–Cylinder ──────────────────────────────────────────────────────────

/// Intersect an infinite plane with an infinite cylinder.
///
/// Cases (let `theta` be the angle between the plane normal and the cylinder axis):
///
/// - `theta ~ 0` (plane perpendicular to axis) -> `Circle3d`
/// - `theta ~ PI/2` (plane parallel to axis):
///   - distance from axis to plane > radius -> `None`
///   - distance == radius (tangent)  -> single `Line3d`
///   - distance < radius -> `TwoCurves(Line3d, Line3d)`
/// - otherwise -> `Ellipse3d`
pub fn plane_cylinder(plane: &Plane, cyl: &Cylinder, tol: &Tolerance) -> SurfaceIntersection {
    let cos_theta = plane.normal.dot(&cyl.axis).abs();
    let sin_theta = (1.0 - cos_theta * cos_theta).max(0.0).sqrt();

    // ── Case 1: plane perpendicular to cylinder axis (cos_theta ~ 1) ──
    if sin_theta < tol.angular {
        // The intersection is a circle whose center is the projection of the
        // cylinder origin onto the plane.  Since the plane is perpendicular to
        // the axis, the circle lies in the plane with the cylinder's radius.
        let center = plane.project_point(&cyl.origin);
        let circle = Circle3d::new(center, plane.normal, cyl.radius);
        return SurfaceIntersection::Curve(Curve::Circle(circle));
    }

    // ── Case 2: plane parallel to cylinder axis (cos_theta ~ 0) ──
    if cos_theta < tol.angular {
        // Distance from the cylinder axis to the plane.
        let dist = plane.distance_to_point(&cyl.origin).abs();

        if dist > cyl.radius + tol.coincidence {
            return SurfaceIntersection::None;
        }

        // Direction along which the lines run: the cylinder axis projected into
        // the plane.  Since the plane is parallel to the axis, the axis already
        // lies in the plane direction.
        let line_dir = cyl.axis;

        if (dist - cyl.radius).abs() < tol.coincidence {
            // Tangent — single line.
            let foot = plane.project_point(&cyl.origin);
            return SurfaceIntersection::Curve(Curve::Line(Line3d {
                origin: foot,
                direction: line_dir,
            }));
        }

        // Two lines.
        let half_chord = (cyl.radius * cyl.radius - dist * dist).max(0.0).sqrt();

        // A direction in the plane perpendicular to both the plane normal and
        // the axis-to-plane direction — this is along the cylinder axis cross
        // the radial direction, but we need the component in the plane that is
        // perpendicular to the line direction.  Since line_dir == cyl.axis, and
        // axis_to_plane is perpendicular to it, the offset direction is simply
        // axis_to_plane cross line_dir, but we can also get it more directly.
        //
        // Actually the two lines are offset from the foot (closest point on the
        // axis projected onto the plane) in the direction perpendicular to both
        // the axis and the plane normal, which is the cross product.
        let lateral = cyl.axis.cross(&plane.normal).normalize();
        let foot = plane.project_point(&cyl.origin);

        let p1 = foot + lateral * half_chord;
        let p2 = foot - lateral * half_chord;

        return SurfaceIntersection::TwoCurves(
            Curve::Line(Line3d {
                origin: p1,
                direction: line_dir,
            }),
            Curve::Line(Line3d {
                origin: p2,
                direction: line_dir,
            }),
        );
    }

    // ── Case 3: oblique plane -> ellipse ──
    //
    // The intersection of a plane with a circular cylinder at angle theta to
    // the axis is an ellipse whose minor radius equals the cylinder radius R
    // and whose major radius equals R / sin(theta).
    //
    // The center of the ellipse is the point on the cylinder axis closest to
    // the plane, projected onto the plane (equivalently, the point where the
    // axis pierces the plane).

    // Find where the cylinder axis meets the plane.
    let denom = plane.normal.dot(&cyl.axis);
    // denom != 0 because cos_theta != 0 (already handled above).
    let t = (plane.origin - cyl.origin).dot(&plane.normal) / denom;
    let center = cyl.origin + cyl.axis * t;

    let minor_radius = cyl.radius;
    let major_radius = cyl.radius / cos_theta;

    // The major axis direction lies in the plane and in the plane containing
    // the cylinder axis and the plane normal.
    // It is the component of the cylinder axis projected into the plane,
    // normalised.
    let axis_in_plane = cyl.axis - plane.normal * plane.normal.dot(&cyl.axis);
    let major_axis = match axis_in_plane.normalized() {
        Some(v) => v,
        // Degenerate — should not happen given sin_theta > angular tolerance.
        std::option::Option::None => return SurfaceIntersection::None,
    };

    let ellipse = Ellipse3d::new(center, plane.normal, major_axis, major_radius, minor_radius);
    SurfaceIntersection::Curve(Curve::Ellipse(ellipse))
}

// ─── Plane–Sphere ────────────────────────────────────────────────────────────

/// Intersect an infinite plane with a sphere.
///
/// - Plane outside sphere -> `None`
/// - Plane tangent to sphere -> `Point`
/// - Plane cutting sphere -> `Circle3d`
/// - (Coincidence is not geometrically meaningful for plane vs sphere.)
pub fn plane_sphere(plane: &Plane, sphere: &Sphere, tol: &Tolerance) -> SurfaceIntersection {
    let signed_dist = plane.distance_to_point(&sphere.center);
    let dist = signed_dist.abs();

    if dist > sphere.radius + tol.coincidence {
        return SurfaceIntersection::None;
    }

    if (dist - sphere.radius).abs() < tol.coincidence {
        // Tangent — single point.
        let point = sphere.center - plane.normal * signed_dist;
        return SurfaceIntersection::Point(point);
    }

    // The intersection circle.
    let circle_radius = (sphere.radius * sphere.radius - dist * dist).max(0.0).sqrt();
    let center = sphere.center - plane.normal * signed_dist;

    let circle = Circle3d::new(center, plane.normal, circle_radius);
    SurfaceIntersection::Curve(Curve::Circle(circle))
}

// ═════════════════════════════════════════════════════════════════════════════
// Tests
// ═════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::vector::Vec3;
    use std::f64::consts::{FRAC_PI_4, PI};

    fn default_tol() -> Tolerance {
        Tolerance::default()
    }

    // ── Helper: verify a point lies on a plane ──────────────────────────
    fn assert_on_plane(plane: &Plane, p: &Point3d, tol: f64) {
        let d = plane.distance_to_point(p).abs();
        assert!(
            d < tol,
            "Point {:?} not on plane (distance = {:.2e})",
            p,
            d
        );
    }

    // ── Helper: verify a point lies on a cylinder ───────────────────────
    fn assert_on_cylinder(cyl: &Cylinder, p: &Point3d, tol: f64) {
        let v = *p - cyl.origin;
        let along = v.dot(&cyl.axis);
        let radial = v - cyl.axis * along;
        let r = radial.length();
        assert!(
            (r - cyl.radius).abs() < tol,
            "Point {:?} not on cylinder (radial distance = {:.2e}, expected {:.2e})",
            p,
            r,
            cyl.radius
        );
    }

    // ── Helper: verify a point lies on a sphere ─────────────────────────
    fn assert_on_sphere(sphere: &Sphere, p: &Point3d, tol: f64) {
        let d = p.distance_to(&sphere.center);
        assert!(
            (d - sphere.radius).abs() < tol,
            "Point {:?} not on sphere (distance from center = {:.2e}, expected {:.2e})",
            p,
            d,
            sphere.radius
        );
    }

    // ── Helper: sample points on a curve ────────────────────────────────
    fn sample_curve(curve: &Curve, num: usize) -> Vec<Point3d> {
        let (t0, t1) = match curve {
            Curve::Line(_) => (-10.0, 10.0),
            Curve::Circle(_) | Curve::Ellipse(_) => (0.0, 2.0 * PI),
            Curve::Nurbs(_) => (0.0, 1.0),
        };
        (0..num)
            .map(|i| {
                let t = t0 + (t1 - t0) * (i as f64 / (num - 1) as f64);
                curve.evaluate(t)
            })
            .collect()
    }

    // ══════════════════════════════════════════════════════════════════════
    // Plane–Plane tests
    // ══════════════════════════════════════════════════════════════════════

    #[test]
    fn plane_plane_transverse_xy_xz() {
        let p1 = Plane::xy();
        let p2 = Plane::xz();
        let tol = default_tol();
        let result = plane_plane(&p1, &p2, &tol);

        match result {
            SurfaceIntersection::Curve(ref c) => {
                // Should be a line along X axis.
                if let Curve::Line(line) = c {
                    assert!(
                        line.direction.is_parallel_to(&Vec3::X, 1e-10),
                        "Expected line along X, got {:?}",
                        line.direction
                    );
                } else {
                    panic!("Expected Line, got {:?}", c.curve_type_name());
                }
                // All sampled points should lie on both planes.
                for p in sample_curve(c, 20) {
                    assert_on_plane(&p1, &p, 1e-7);
                    assert_on_plane(&p2, &p, 1e-7);
                }
            }
            other => panic!("Expected Curve, got {:?}", other),
        }
    }

    #[test]
    fn plane_plane_transverse_xy_yz() {
        let p1 = Plane::xy();
        let p2 = Plane::yz();
        let tol = default_tol();
        let result = plane_plane(&p1, &p2, &tol);

        match result {
            SurfaceIntersection::Curve(ref c) => {
                if let Curve::Line(line) = c {
                    assert!(
                        line.direction.is_parallel_to(&Vec3::Y, 1e-10),
                        "Expected line along Y, got {:?}",
                        line.direction
                    );
                } else {
                    panic!("Expected Line");
                }
                for p in sample_curve(c, 20) {
                    assert_on_plane(&p1, &p, 1e-7);
                    assert_on_plane(&p2, &p, 1e-7);
                }
            }
            other => panic!("Expected Curve, got {:?}", other),
        }
    }

    #[test]
    fn plane_plane_parallel_non_coincident() {
        let p1 = Plane::new(Point3d::ORIGIN, Vec3::Z);
        let p2 = Plane::new(Point3d::new(0.0, 0.0, 5.0), Vec3::Z);
        let tol = default_tol();
        let result = plane_plane(&p1, &p2, &tol);
        assert!(
            matches!(result, SurfaceIntersection::None),
            "Expected None for parallel planes, got {:?}",
            result
        );
    }

    #[test]
    fn plane_plane_coincident() {
        let p1 = Plane::new(Point3d::ORIGIN, Vec3::Z);
        let p2 = Plane::new(Point3d::new(3.0, 4.0, 0.0), Vec3::Z);
        let tol = default_tol();
        let result = plane_plane(&p1, &p2, &tol);
        assert!(
            matches!(result, SurfaceIntersection::Coincident),
            "Expected Coincident, got {:?}",
            result
        );
    }

    #[test]
    fn plane_plane_coincident_opposite_normals() {
        let p1 = Plane::new(Point3d::ORIGIN, Vec3::Z);
        let p2 = Plane::new(Point3d::new(1.0, 2.0, 0.0), -Vec3::Z);
        let tol = default_tol();
        let result = plane_plane(&p1, &p2, &tol);
        assert!(
            matches!(result, SurfaceIntersection::Coincident),
            "Expected Coincident for same plane with flipped normal, got {:?}",
            result
        );
    }

    #[test]
    fn plane_plane_oblique_45() {
        // z=0 plane vs. a plane tilted 45 degrees around the X axis.
        let p1 = Plane::xy();
        let normal2 = Vec3::new(0.0, -FRAC_PI_4.sin(), FRAC_PI_4.cos());
        let p2 = Plane::new(Point3d::ORIGIN, normal2);
        let tol = default_tol();
        let result = plane_plane(&p1, &p2, &tol);

        match result {
            SurfaceIntersection::Curve(ref c) => {
                for p in sample_curve(c, 20) {
                    assert_on_plane(&p1, &p, 1e-7);
                    assert_on_plane(&p2, &p, 1e-7);
                }
            }
            other => panic!("Expected Curve, got {:?}", other),
        }
    }

    // ══════════════════════════════════════════════════════════════════════
    // Plane–Cylinder tests
    // ══════════════════════════════════════════════════════════════════════

    #[test]
    fn plane_cylinder_perpendicular_circle() {
        // z=5 plane cuts a Z-axis cylinder -> circle at z=5.
        let plane = Plane::new(Point3d::new(0.0, 0.0, 5.0), Vec3::Z);
        let cyl = Cylinder::new(Point3d::ORIGIN, Vec3::Z, 3.0);
        let tol = default_tol();
        let result = plane_cylinder(&plane, &cyl, &tol);

        match result {
            SurfaceIntersection::Curve(ref c) => {
                assert_eq!(c.curve_type_name(), "Circle");
                for p in sample_curve(c, 40) {
                    assert_on_plane(&plane, &p, 1e-7);
                    assert_on_cylinder(&cyl, &p, 1e-7);
                }
                if let Curve::Circle(circ) = c {
                    assert!(
                        (circ.radius - 3.0).abs() < 1e-10,
                        "Circle radius should be cylinder radius"
                    );
                    assert!(
                        (circ.center.z - 5.0).abs() < 1e-10,
                        "Circle center z should be 5.0"
                    );
                }
            }
            other => panic!("Expected Circle curve, got {:?}", other),
        }
    }

    #[test]
    fn plane_cylinder_parallel_two_lines() {
        // The XZ plane (normal=Y) is parallel to a Z-axis cylinder at origin
        // with radius 3. Distance from axis to plane = 0 < 3, so two lines.
        let plane = Plane::xz(); // normal = Y, origin at (0,0,0)
        let cyl = Cylinder::new(Point3d::ORIGIN, Vec3::Z, 3.0);
        let tol = default_tol();
        let result = plane_cylinder(&plane, &cyl, &tol);

        match result {
            SurfaceIntersection::TwoCurves(ref c1, ref c2) => {
                assert_eq!(c1.curve_type_name(), "Line");
                assert_eq!(c2.curve_type_name(), "Line");
                for p in sample_curve(c1, 20) {
                    assert_on_plane(&plane, &p, 1e-7);
                    assert_on_cylinder(&cyl, &p, 1e-7);
                }
                for p in sample_curve(c2, 20) {
                    assert_on_plane(&plane, &p, 1e-7);
                    assert_on_cylinder(&cyl, &p, 1e-7);
                }
            }
            other => panic!("Expected TwoCurves, got {:?}", other),
        }
    }

    #[test]
    fn plane_cylinder_parallel_tangent_one_line() {
        // Plane y=3 is tangent to a Z-axis cylinder of radius 3 at origin.
        let plane = Plane::new(Point3d::new(0.0, 3.0, 0.0), Vec3::Y);
        let cyl = Cylinder::new(Point3d::ORIGIN, Vec3::Z, 3.0);
        let tol = default_tol();
        let result = plane_cylinder(&plane, &cyl, &tol);

        match result {
            SurfaceIntersection::Curve(ref c) => {
                assert_eq!(c.curve_type_name(), "Line");
                for p in sample_curve(c, 20) {
                    assert_on_plane(&plane, &p, 1e-7);
                    assert_on_cylinder(&cyl, &p, 1e-7);
                }
            }
            other => panic!("Expected single Line (tangent), got {:?}", other),
        }
    }

    #[test]
    fn plane_cylinder_parallel_no_intersection() {
        // Plane y=5 does not intersect a Z-axis cylinder of radius 3.
        let plane = Plane::new(Point3d::new(0.0, 5.0, 0.0), Vec3::Y);
        let cyl = Cylinder::new(Point3d::ORIGIN, Vec3::Z, 3.0);
        let tol = default_tol();
        let result = plane_cylinder(&plane, &cyl, &tol);
        assert!(
            matches!(result, SurfaceIntersection::None),
            "Expected None, got {:?}",
            result
        );
    }

    #[test]
    fn plane_cylinder_oblique_ellipse() {
        // A plane tilted at 45 degrees to the Z axis intersects a Z-axis
        // cylinder, producing an ellipse.
        let normal = Vec3::new(0.0, FRAC_PI_4.sin(), FRAC_PI_4.cos()).normalize();
        let plane = Plane::new(Point3d::ORIGIN, normal);
        let cyl = Cylinder::new(Point3d::ORIGIN, Vec3::Z, 2.0);
        let tol = default_tol();
        let result = plane_cylinder(&plane, &cyl, &tol);

        match result {
            SurfaceIntersection::Curve(ref c) => {
                assert_eq!(c.curve_type_name(), "Ellipse");
                for p in sample_curve(c, 40) {
                    assert_on_plane(&plane, &p, 1e-6);
                    assert_on_cylinder(&cyl, &p, 1e-6);
                }
                if let Curve::Ellipse(e) = c {
                    assert!(
                        (e.minor_radius - 2.0).abs() < 1e-10,
                        "Minor radius should equal cylinder radius"
                    );
                    assert!(
                        e.major_radius > e.minor_radius,
                        "Major radius should be larger than minor for oblique cut"
                    );
                }
            }
            other => panic!("Expected Ellipse, got {:?}", other),
        }
    }

    #[test]
    fn plane_cylinder_oblique_ellipse_radii() {
        // Verify the exact major radius for a 45-degree cut: R / sin(45) = R * sqrt(2).
        let plane = Plane::new(
            Point3d::ORIGIN,
            Vec3::new(0.0, 1.0, 1.0).normalize(),
        );
        let r = 5.0;
        let cyl = Cylinder::new(Point3d::ORIGIN, Vec3::Z, r);
        let tol = default_tol();
        let result = plane_cylinder(&plane, &cyl, &tol);

        if let SurfaceIntersection::Curve(Curve::Ellipse(e)) = result {
            let expected_major = r * 2.0_f64.sqrt();
            assert!(
                (e.major_radius - expected_major).abs() < 1e-8,
                "Major radius {:.6} != expected {:.6}",
                e.major_radius,
                expected_major
            );
            assert!(
                (e.minor_radius - r).abs() < 1e-10,
                "Minor radius should be cylinder radius"
            );
        } else {
            panic!("Expected Ellipse for 45-degree cut");
        }
    }

    #[test]
    fn plane_cylinder_perpendicular_at_offset() {
        // Perpendicular cut at z = -3, cylinder origin at (1,2,0), axis Z.
        let plane = Plane::new(Point3d::new(0.0, 0.0, -3.0), Vec3::Z);
        let cyl = Cylinder::new(Point3d::new(1.0, 2.0, 0.0), Vec3::Z, 4.0);
        let tol = default_tol();
        let result = plane_cylinder(&plane, &cyl, &tol);

        match result {
            SurfaceIntersection::Curve(ref c) => {
                assert_eq!(c.curve_type_name(), "Circle");
                if let Curve::Circle(circ) = c {
                    assert!((circ.center.x - 1.0).abs() < 1e-10);
                    assert!((circ.center.y - 2.0).abs() < 1e-10);
                    assert!((circ.center.z - (-3.0)).abs() < 1e-10);
                    assert!((circ.radius - 4.0).abs() < 1e-10);
                }
                for p in sample_curve(c, 40) {
                    assert_on_plane(&plane, &p, 1e-7);
                    assert_on_cylinder(&cyl, &p, 1e-7);
                }
            }
            other => panic!("Expected Circle, got {:?}", other),
        }
    }

    #[test]
    fn plane_cylinder_parallel_offset_two_lines() {
        // Cylinder axis along Z at (0,0,0), radius 5.
        // Plane y=2 (parallel to axis, distance 2 < 5).
        let plane = Plane::new(Point3d::new(0.0, 2.0, 0.0), Vec3::Y);
        let cyl = Cylinder::new(Point3d::ORIGIN, Vec3::Z, 5.0);
        let tol = default_tol();
        let result = plane_cylinder(&plane, &cyl, &tol);

        match result {
            SurfaceIntersection::TwoCurves(ref c1, ref c2) => {
                for p in sample_curve(c1, 20) {
                    assert_on_plane(&plane, &p, 1e-7);
                    assert_on_cylinder(&cyl, &p, 1e-7);
                }
                for p in sample_curve(c2, 20) {
                    assert_on_plane(&plane, &p, 1e-7);
                    assert_on_cylinder(&cyl, &p, 1e-7);
                }
                // The two lines should be at x = +/- sqrt(25-4) = +/- sqrt(21)
                if let (Curve::Line(l1), Curve::Line(l2)) = (c1, c2) {
                    let half_chord = 21.0_f64.sqrt();
                    let x1 = l1.origin.x.abs();
                    let x2 = l2.origin.x.abs();
                    assert!(
                        (x1 - half_chord).abs() < 1e-7 && (x2 - half_chord).abs() < 1e-7,
                        "Lines should be at x = +/- sqrt(21), got {:.6} and {:.6}",
                        l1.origin.x,
                        l2.origin.x
                    );
                }
            }
            other => panic!("Expected TwoCurves for parallel offset, got {:?}", other),
        }
    }

    // ══════════════════════════════════════════════════════════════════════
    // Plane–Sphere tests
    // ══════════════════════════════════════════════════════════════════════

    #[test]
    fn plane_sphere_great_circle() {
        // Plane through sphere center -> great circle.
        let plane = Plane::xy();
        let sphere = Sphere::new(Point3d::ORIGIN, 5.0);
        let tol = default_tol();
        let result = plane_sphere(&plane, &sphere, &tol);

        match result {
            SurfaceIntersection::Curve(ref c) => {
                assert_eq!(c.curve_type_name(), "Circle");
                if let Curve::Circle(circ) = c {
                    assert!(
                        (circ.radius - 5.0).abs() < 1e-10,
                        "Great circle radius should equal sphere radius"
                    );
                }
                for p in sample_curve(c, 40) {
                    assert_on_plane(&plane, &p, 1e-7);
                    assert_on_sphere(&sphere, &p, 1e-7);
                }
            }
            other => panic!("Expected Circle (great circle), got {:?}", other),
        }
    }

    #[test]
    fn plane_sphere_small_circle() {
        // Plane z=3 intersects sphere of radius 5 at origin -> small circle of
        // radius sqrt(25-9)=4.
        let plane = Plane::new(Point3d::new(0.0, 0.0, 3.0), Vec3::Z);
        let sphere = Sphere::new(Point3d::ORIGIN, 5.0);
        let tol = default_tol();
        let result = plane_sphere(&plane, &sphere, &tol);

        match result {
            SurfaceIntersection::Curve(ref c) => {
                assert_eq!(c.curve_type_name(), "Circle");
                if let Curve::Circle(circ) = c {
                    assert!(
                        (circ.radius - 4.0).abs() < 1e-10,
                        "Small circle radius should be 4, got {}",
                        circ.radius
                    );
                    assert!(
                        (circ.center.z - 3.0).abs() < 1e-10,
                        "Circle center z should be 3"
                    );
                }
                for p in sample_curve(c, 40) {
                    assert_on_plane(&plane, &p, 1e-7);
                    assert_on_sphere(&sphere, &p, 1e-7);
                }
            }
            other => panic!("Expected Circle (small circle), got {:?}", other),
        }
    }

    #[test]
    fn plane_sphere_tangent_point() {
        // Plane z=5 is tangent to sphere of radius 5 at (0,0,5).
        let plane = Plane::new(Point3d::new(0.0, 0.0, 5.0), Vec3::Z);
        let sphere = Sphere::new(Point3d::ORIGIN, 5.0);
        let tol = default_tol();
        let result = plane_sphere(&plane, &sphere, &tol);

        match result {
            SurfaceIntersection::Point(p) => {
                assert!(
                    p.distance_to(&Point3d::new(0.0, 0.0, 5.0)) < 1e-7,
                    "Tangent point should be (0,0,5), got {:?}",
                    p
                );
            }
            other => panic!("Expected Point (tangent), got {:?}", other),
        }
    }

    #[test]
    fn plane_sphere_no_intersection() {
        // Plane z=10 does not intersect sphere of radius 5 at origin.
        let plane = Plane::new(Point3d::new(0.0, 0.0, 10.0), Vec3::Z);
        let sphere = Sphere::new(Point3d::ORIGIN, 5.0);
        let tol = default_tol();
        let result = plane_sphere(&plane, &sphere, &tol);

        assert!(
            matches!(result, SurfaceIntersection::None),
            "Expected None, got {:?}",
            result
        );
    }

    #[test]
    fn plane_sphere_tangent_from_below() {
        // Plane z=-5 tangent from below.
        let plane = Plane::new(Point3d::new(0.0, 0.0, -5.0), Vec3::Z);
        let sphere = Sphere::new(Point3d::ORIGIN, 5.0);
        let tol = default_tol();
        let result = plane_sphere(&plane, &sphere, &tol);

        match result {
            SurfaceIntersection::Point(p) => {
                assert!(
                    p.distance_to(&Point3d::new(0.0, 0.0, -5.0)) < 1e-7,
                    "Tangent point should be (0,0,-5), got {:?}",
                    p
                );
            }
            other => panic!("Expected Point (tangent from below), got {:?}", other),
        }
    }

    #[test]
    fn plane_sphere_offset_center() {
        // Sphere centered at (3,4,5), radius 2. Plane z=6 -> small circle.
        let sphere = Sphere::new(Point3d::new(3.0, 4.0, 5.0), 2.0);
        let plane = Plane::new(Point3d::new(0.0, 0.0, 6.0), Vec3::Z);
        let tol = default_tol();
        let result = plane_sphere(&plane, &sphere, &tol);

        match result {
            SurfaceIntersection::Curve(ref c) => {
                assert_eq!(c.curve_type_name(), "Circle");
                if let Curve::Circle(circ) = c {
                    // dist = |6 - 5| = 1, r = sqrt(4 - 1) = sqrt(3)
                    let expected_r = 3.0_f64.sqrt();
                    assert!(
                        (circ.radius - expected_r).abs() < 1e-10,
                        "Expected radius sqrt(3), got {}",
                        circ.radius
                    );
                    assert!((circ.center.x - 3.0).abs() < 1e-10);
                    assert!((circ.center.y - 4.0).abs() < 1e-10);
                    assert!((circ.center.z - 6.0).abs() < 1e-10);
                }
                for p in sample_curve(c, 40) {
                    assert_on_plane(&plane, &p, 1e-7);
                    assert_on_sphere(&sphere, &p, 1e-7);
                }
            }
            other => panic!("Expected Circle for offset sphere, got {:?}", other),
        }
    }

    #[test]
    fn plane_sphere_near_miss() {
        // Plane z=5.001 should not intersect sphere of radius 5.
        let plane = Plane::new(Point3d::new(0.0, 0.0, 5.001), Vec3::Z);
        let sphere = Sphere::new(Point3d::ORIGIN, 5.0);
        let tol = default_tol();
        let result = plane_sphere(&plane, &sphere, &tol);

        assert!(
            matches!(result, SurfaceIntersection::None),
            "Expected None for near-miss, got {:?}",
            result
        );
    }

    // ══════════════════════════════════════════════════════════════════════
    // Cross-validation: ellipse points on both surfaces
    // ══════════════════════════════════════════════════════════════════════

    #[test]
    fn plane_cylinder_oblique_many_angles() {
        // Test multiple oblique angles to ensure the ellipse formula is robust.
        let cyl = Cylinder::new(Point3d::new(1.0, 2.0, 3.0), Vec3::Z, 4.0);
        let tol = default_tol();

        for angle_deg in &[15.0, 30.0, 45.0, 60.0, 75.0] {
            let angle_rad = (*angle_deg as f64).to_radians();
            let normal = Vec3::new(0.0, angle_rad.sin(), angle_rad.cos()).normalize();
            let plane = Plane::new(cyl.origin, normal);
            let result = plane_cylinder(&plane, &cyl, &tol);

            match result {
                SurfaceIntersection::Curve(ref c) => {
                    assert_eq!(c.curve_type_name(), "Ellipse", "At {} deg", angle_deg);
                    for p in sample_curve(c, 40) {
                        assert_on_plane(&plane, &p, 1e-5);
                        assert_on_cylinder(&cyl, &p, 1e-5);
                    }
                }
                other => panic!("Expected Ellipse at {} deg, got {:?}", angle_deg, other),
            }
        }
    }

    #[test]
    fn plane_cylinder_non_axis_aligned() {
        // Cylinder along a non-axis direction.
        let axis = Vec3::new(1.0, 1.0, 1.0).normalize();
        let cyl = Cylinder::new(Point3d::new(5.0, 5.0, 5.0), axis, 2.0);
        // Plane perpendicular to the cylinder axis at the cylinder origin.
        let plane = Plane::new(cyl.origin, axis);
        let tol = default_tol();
        let result = plane_cylinder(&plane, &cyl, &tol);

        match result {
            SurfaceIntersection::Curve(ref c) => {
                assert_eq!(c.curve_type_name(), "Circle");
                for p in sample_curve(c, 40) {
                    assert_on_plane(&plane, &p, 1e-7);
                    assert_on_cylinder(&cyl, &p, 1e-7);
                }
            }
            other => panic!("Expected Circle for non-axis-aligned cylinder, got {:?}", other),
        }
    }
}
