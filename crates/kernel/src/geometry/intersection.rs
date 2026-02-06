use super::curves::{Line3d, Ray};
use super::point::Point3d;
use super::surfaces::{Plane, Sphere, Cylinder, Surface};
use super::vector::Vec3;

/// Result of a curve-curve intersection.
#[derive(Debug, Clone)]
pub struct CurveCurveHit {
    pub point: Point3d,
    pub t1: f64,
    pub t2: f64,
}

/// Result of a ray-surface intersection.
#[derive(Debug, Clone)]
pub struct RaySurfaceHit {
    pub point: Point3d,
    pub t: f64,
    pub normal: Vec3,
}

// ─── Line-Line Intersection ──────────────────────────────────────────────────

/// Find the closest points between two lines in 3D.
/// Returns None if lines are parallel.
/// Returns (point_on_l1, t1, point_on_l2, t2, distance).
pub fn line_line_closest(l1: &Line3d, l2: &Line3d) -> Option<(Point3d, f64, Point3d, f64, f64)> {
    let w = l1.origin - l2.origin;
    let a = l1.direction.dot(&l1.direction);
    let b = l1.direction.dot(&l2.direction);
    let c = l2.direction.dot(&l2.direction);
    let d = l1.direction.dot(&w);
    let e = l2.direction.dot(&w);

    let denom = a * c - b * b;
    if denom.abs() < 1e-15 {
        return None; // parallel
    }

    let t1 = (b * e - c * d) / denom;
    let t2 = (a * e - b * d) / denom;

    let p1 = l1.evaluate(t1);
    let p2 = l2.evaluate(t2);
    let dist = p1.distance_to(&p2);

    Some((p1, t1, p2, t2, dist))
}

/// Intersect two lines. Returns intersection point if distance < tolerance.
pub fn line_line_intersection(l1: &Line3d, l2: &Line3d, tol: f64) -> Vec<CurveCurveHit> {
    match line_line_closest(l1, l2) {
        Some((p1, t1, _p2, t2, dist)) if dist < tol => {
            vec![CurveCurveHit {
                point: p1,
                t1,
                t2,
            }]
        }
        _ => vec![],
    }
}

// ─── Ray-Plane Intersection ─────────────────────────────────────────────────

pub fn ray_plane(ray: &Ray, plane: &Plane) -> Option<RaySurfaceHit> {
    let denom = ray.direction.dot(&plane.normal);
    if denom.abs() < 1e-15 {
        return None; // parallel
    }
    let t = (plane.origin - ray.origin).dot(&plane.normal) / denom;
    if t < 0.0 {
        return None; // behind ray
    }
    Some(RaySurfaceHit {
        point: ray.at(t),
        t,
        normal: if denom < 0.0 {
            plane.normal
        } else {
            -plane.normal
        },
    })
}

// ─── Ray-Sphere Intersection ────────────────────────────────────────────────

pub fn ray_sphere(ray: &Ray, sphere: &Sphere) -> Vec<RaySurfaceHit> {
    let oc = ray.origin - sphere.center;
    let a = ray.direction.dot(&ray.direction);
    let b = 2.0 * oc.dot(&ray.direction);
    let c = oc.dot(&oc) - sphere.radius * sphere.radius;
    let discriminant = b * b - 4.0 * a * c;

    if discriminant < 0.0 {
        return vec![];
    }

    let sqrt_disc = discriminant.sqrt();
    let mut hits = vec![];

    for sign in [-1.0, 1.0] {
        let t = (-b + sign * sqrt_disc) / (2.0 * a);
        if t >= 0.0 {
            let point = ray.at(t);
            let normal = (point - sphere.center).normalize();
            hits.push(RaySurfaceHit { point, t, normal });
        }
    }

    hits.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap());
    hits
}

// ─── Ray-Cylinder Intersection ──────────────────────────────────────────────

/// Intersect a ray with an infinite cylinder.
pub fn ray_cylinder(ray: &Ray, cyl: &Cylinder) -> Vec<RaySurfaceHit> {
    let oc = ray.origin - cyl.origin;

    // Project onto plane perpendicular to axis
    let d_proj = ray.direction - cyl.axis * ray.direction.dot(&cyl.axis);
    let oc_proj = oc - cyl.axis * oc.dot(&cyl.axis);

    let a = d_proj.dot(&d_proj);
    let b = 2.0 * d_proj.dot(&oc_proj);
    let c = oc_proj.dot(&oc_proj) - cyl.radius * cyl.radius;
    let discriminant = b * b - 4.0 * a * c;

    if discriminant < 0.0 || a.abs() < 1e-15 {
        return vec![];
    }

    let sqrt_disc = discriminant.sqrt();
    let mut hits = vec![];

    for sign in [-1.0, 1.0] {
        let t = (-b + sign * sqrt_disc) / (2.0 * a);
        if t >= 0.0 {
            let point = ray.at(t);
            // Normal is the radial direction from the axis
            let to_point = point - cyl.origin;
            let along_axis = to_point.dot(&cyl.axis);
            let radial = to_point - cyl.axis * along_axis;
            let normal = radial.normalized().unwrap_or(Vec3::X);
            hits.push(RaySurfaceHit { point, t, normal });
        }
    }

    hits.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap());
    hits
}

// ─── Ray-AABB Intersection (for BVH) ──────────────────────────────────────

pub fn ray_aabb(ray: &Ray, bb_min: &Point3d, bb_max: &Point3d) -> Option<f64> {
    let mut tmin = f64::NEG_INFINITY;
    let mut tmax = f64::INFINITY;

    let ray_origin = [ray.origin.x, ray.origin.y, ray.origin.z];
    let ray_dir = [ray.direction.x, ray.direction.y, ray.direction.z];
    let min = [bb_min.x, bb_min.y, bb_min.z];
    let max = [bb_max.x, bb_max.y, bb_max.z];

    for i in 0..3 {
        if ray_dir[i].abs() < 1e-15 {
            if ray_origin[i] < min[i] || ray_origin[i] > max[i] {
                return None;
            }
        } else {
            let inv_d = 1.0 / ray_dir[i];
            let mut t0 = (min[i] - ray_origin[i]) * inv_d;
            let mut t1 = (max[i] - ray_origin[i]) * inv_d;
            if inv_d < 0.0 {
                std::mem::swap(&mut t0, &mut t1);
            }
            tmin = tmin.max(t0);
            tmax = tmax.min(t1);
            if tmax < tmin {
                return None;
            }
        }
    }

    if tmax < 0.0 {
        None
    } else {
        Some(tmin.max(0.0))
    }
}

// ─── Plane-Plane Intersection ───────────────────────────────────────────────

/// Intersect two planes. Returns the line of intersection, or None if parallel.
pub fn plane_plane(p1: &Plane, p2: &Plane) -> Option<Line3d> {
    let dir = p1.normal.cross(&p2.normal);
    let len = dir.length();
    if len < 1e-12 {
        return None; // parallel
    }
    let dir = dir / len;

    // Find a point on the line: solve the two plane equations
    let d1 = p1.origin.to_vec3().dot(&p1.normal);
    let d2 = p2.origin.to_vec3().dot(&p2.normal);

    let n1n2 = p1.normal.dot(&p2.normal);
    let denom = 1.0 - n1n2 * n1n2;
    if denom.abs() < 1e-15 {
        return None;
    }

    let c1 = (d1 - d2 * n1n2) / denom;
    let c2 = (d2 - d1 * n1n2) / denom;
    let origin = Point3d::ORIGIN + p1.normal * c1 + p2.normal * c2;

    Some(Line3d { origin, direction: dir })
}

// ─── General surface-surface intersection via marching ──────────────────────

/// Find intersection points between two surfaces by sampling and Newton refinement.
/// This is a simplified marching method for analytic surfaces.
pub fn surface_surface_intersection_points(
    s1: &Surface,
    s2: &Surface,
    u1_range: (f64, f64),
    v1_range: (f64, f64),
    u2_range: (f64, f64),
    v2_range: (f64, f64),
    num_samples: usize,
    tolerance: f64,
) -> Vec<Point3d> {
    let mut hits = Vec::new();

    // Brute force sampling on s1, project onto s2
    for i in 0..num_samples {
        for j in 0..num_samples {
            let u1 = u1_range.0 + (u1_range.1 - u1_range.0) * (i as f64 / (num_samples - 1) as f64);
            let v1 = v1_range.0 + (v1_range.1 - v1_range.0) * (j as f64 / (num_samples - 1) as f64);
            let p1 = s1.evaluate(u1, v1);

            // For each point on s1, find the closest point on s2 by sampling
            let mut min_dist = f64::MAX;
            let mut best_point = p1;

            for k in 0..num_samples {
                for l in 0..num_samples {
                    let u2 = u2_range.0
                        + (u2_range.1 - u2_range.0) * (k as f64 / (num_samples - 1) as f64);
                    let v2 = v2_range.0
                        + (v2_range.1 - v2_range.0) * (l as f64 / (num_samples - 1) as f64);
                    let p2 = s2.evaluate(u2, v2);
                    let dist = p1.distance_to(&p2);
                    if dist < min_dist {
                        min_dist = dist;
                        best_point = p1.midpoint(&p2);
                    }
                }
            }

            if min_dist < tolerance {
                // Check if this point is far enough from existing hits
                let is_new = hits.iter().all(|h: &Point3d| h.distance_to(&best_point) > tolerance * 10.0);
                if is_new {
                    hits.push(best_point);
                }
            }
        }
    }

    hits
}

#[cfg(test)]
mod tests {
    use super::*;
    

    #[test]
    fn test_line_line_intersection() {
        let l1 = Line3d::new(Point3d::ORIGIN, Vec3::X);
        let l2 = Line3d::new(Point3d::new(0.0, 0.0, 0.0), Vec3::Y);
        let hits = line_line_intersection(&l1, &l2, 1e-7);
        assert_eq!(hits.len(), 1);
        assert!(hits[0].point.distance_to(&Point3d::ORIGIN) < 1e-7);
    }

    #[test]
    fn test_line_line_skew() {
        let l1 = Line3d::new(Point3d::ORIGIN, Vec3::X);
        let l2 = Line3d::new(Point3d::new(0.0, 0.0, 5.0), Vec3::Y);
        let hits = line_line_intersection(&l1, &l2, 1e-7);
        assert!(hits.is_empty());
    }

    #[test]
    fn test_line_line_parallel() {
        let l1 = Line3d::new(Point3d::ORIGIN, Vec3::X);
        let l2 = Line3d::new(Point3d::new(0.0, 1.0, 0.0), Vec3::X);
        let hits = line_line_intersection(&l1, &l2, 1e-7);
        assert!(hits.is_empty());
    }

    #[test]
    fn test_ray_plane() {
        let ray = Ray::new(Point3d::new(0.0, 0.0, 10.0), -Vec3::Z);
        let plane = Plane::xy();
        let hit = ray_plane(&ray, &plane).unwrap();
        assert!((hit.t - 10.0).abs() < 1e-12);
        assert!(hit.point.distance_to(&Point3d::ORIGIN) < 1e-12);
    }

    #[test]
    fn test_ray_sphere() {
        let ray = Ray::new(Point3d::new(0.0, 0.0, 10.0), -Vec3::Z);
        let sphere = Sphere::new(Point3d::ORIGIN, 1.0);
        let hits = ray_sphere(&ray, &sphere);
        assert_eq!(hits.len(), 2);
        assert!((hits[0].point.z - 1.0).abs() < 1e-10);
        assert!((hits[1].point.z - (-1.0)).abs() < 1e-10);
    }

    #[test]
    fn test_ray_sphere_miss() {
        let ray = Ray::new(Point3d::new(5.0, 0.0, 10.0), -Vec3::Z);
        let sphere = Sphere::new(Point3d::ORIGIN, 1.0);
        let hits = ray_sphere(&ray, &sphere);
        assert!(hits.is_empty());
    }

    #[test]
    fn test_ray_cylinder() {
        let ray = Ray::new(Point3d::new(10.0, 0.0, 0.0), -Vec3::X);
        let cyl = Cylinder::new(Point3d::ORIGIN, Vec3::Z, 3.0);
        let hits = ray_cylinder(&ray, &cyl);
        assert_eq!(hits.len(), 2);
        // Entry and exit at x=3 and x=-3
        assert!((hits[0].point.x - 3.0).abs() < 1e-10);
        assert!((hits[1].point.x - (-3.0)).abs() < 1e-10);
    }

    #[test]
    fn test_plane_plane_intersection() {
        let p1 = Plane::xy(); // z=0 plane
        let p2 = Plane::xz(); // y=0 plane
        let line = plane_plane(&p1, &p2).unwrap();
        // Should be the X axis
        assert!(line.direction.is_parallel_to(&Vec3::X, 1e-10));
    }

    #[test]
    fn test_plane_plane_parallel() {
        let p1 = Plane::new(Point3d::ORIGIN, Vec3::Z);
        let p2 = Plane::new(Point3d::new(0.0, 0.0, 5.0), Vec3::Z);
        assert!(plane_plane(&p1, &p2).is_none());
    }

    #[test]
    fn test_ray_aabb_hit() {
        let ray = Ray::new(Point3d::new(-5.0, 0.5, 0.5), Vec3::X);
        let t = ray_aabb(
            &ray,
            &Point3d::ORIGIN,
            &Point3d::new(1.0, 1.0, 1.0),
        );
        assert!(t.is_some());
        assert!((t.unwrap() - 5.0).abs() < 1e-10);
    }

    #[test]
    fn test_ray_aabb_miss() {
        let ray = Ray::new(Point3d::new(-5.0, 5.0, 5.0), Vec3::X);
        let t = ray_aabb(
            &ray,
            &Point3d::ORIGIN,
            &Point3d::new(1.0, 1.0, 1.0),
        );
        assert!(t.is_none());
    }
}
