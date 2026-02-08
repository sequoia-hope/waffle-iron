use super::curves::{Line3d, Ray};
use super::point::Point3d;
use super::surfaces::{Cone, Cylinder, Plane, Sphere, Surface, Torus};
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

// ─── Ray-Cone Intersection ─────────────────────────────────────────────────

/// Intersect a ray with an infinite double cone.
///
/// The cone is defined by apex, axis direction, and half-angle.
/// The equation is: (P-apex)·axis)² = |P-apex|² cos²(half_angle)
/// which rearranges to a standard quadratic in t.
pub fn ray_cone(ray: &Ray, cone: &Cone) -> Vec<RaySurfaceHit> {
    let co = ray.origin - cone.apex;
    let cos_a = cone.half_angle.cos();
    let cos2 = cos_a * cos_a;

    let d_dot_a = ray.direction.dot(&cone.axis);
    let co_dot_a = co.dot(&cone.axis);

    let a = d_dot_a * d_dot_a - cos2 * ray.direction.dot(&ray.direction);
    let b = 2.0 * (d_dot_a * co_dot_a - cos2 * co.dot(&ray.direction));
    let c = co_dot_a * co_dot_a - cos2 * co.dot(&co);

    let discriminant = b * b - 4.0 * a * c;

    if discriminant < -1e-12 {
        return vec![];
    }
    let discriminant = discriminant.max(0.0);

    // Handle degenerate case where ray is along the cone surface (a ≈ 0)
    if a.abs() < 1e-15 {
        if b.abs() < 1e-15 {
            return vec![];
        }
        let t = -c / b;
        if t >= 0.0 {
            let point = ray.at(t);
            let normal = cone_normal(cone, &point);
            return vec![RaySurfaceHit { point, t, normal }];
        }
        return vec![];
    }

    let sqrt_disc = discriminant.sqrt();
    let mut hits = vec![];

    for sign in [-1.0, 1.0] {
        let t = (-b + sign * sqrt_disc) / (2.0 * a);
        if t >= 0.0 {
            let point = ray.at(t);
            let normal = cone_normal(cone, &point);
            hits.push(RaySurfaceHit { point, t, normal });
        }
    }

    hits.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap());
    hits
}

/// Compute the outward normal of a cone at a given point.
fn cone_normal(cone: &Cone, point: &Point3d) -> Vec3 {
    let v = *point - cone.apex;
    let v_along = cone.axis * v.dot(&cone.axis);
    let v_radial = v - v_along;
    let radial_len = v_radial.length();
    if radial_len < 1e-15 {
        // At the apex — normal is undefined; return axis as fallback
        return cone.axis;
    }
    let radial_dir = v_radial * (1.0 / radial_len);
    // Normal is perpendicular to the cone surface: tilt radial outward by (90° - half_angle)
    let cos_a = cone.half_angle.cos();
    let sin_a = cone.half_angle.sin();
    // Determine which side of apex the point is on
    let along_dist = v.dot(&cone.axis);
    let axis_sign = if along_dist >= 0.0 { 1.0 } else { -1.0 };
    (radial_dir * cos_a - cone.axis * sin_a * axis_sign).normalize()
}

// ─── Ray-Torus Intersection ────────────────────────────────────────────────

/// Intersect a ray with a torus.
///
/// The torus is centered at `center` with axis `axis`, major radius `R`, and
/// minor radius `r`. The implicit equation in the torus local frame (axis = Z) is:
///   (x² + y² + z² + R² - r²)² = 4R²(x² + y²)
///
/// Substituting the parametric ray P = O + tD gives a quartic in t.
/// We solve using the depressed quartic / Ferrari's method.
pub fn ray_torus(ray: &Ray, torus: &Torus) -> Vec<RaySurfaceHit> {
    let big_r = torus.major_radius;
    let small_r = torus.minor_radius;

    // Transform ray into torus local frame where axis = Z and center = origin
    let (local_origin, local_dir) = to_torus_local(ray, torus);

    let ox = local_origin.x;
    let oy = local_origin.y;
    let oz = local_origin.z;
    let dx = local_dir.x;
    let dy = local_dir.y;
    let dz = local_dir.z;

    // Precompute dot products in local frame
    let sum_d2 = dx * dx + dy * dy + dz * dz; // should be 1 for normalized ray
    let sum_od = ox * dx + oy * dy + oz * dz;
    let sum_o2 = ox * ox + oy * oy + oz * oz;

    let r2 = big_r * big_r;
    let s2 = small_r * small_r;
    let k = sum_o2 - r2 - s2;

    // Quartic coefficients: a4*t^4 + a3*t^3 + a2*t^2 + a1*t + a0 = 0
    let a4 = sum_d2 * sum_d2;
    let a3 = 4.0 * sum_d2 * sum_od;
    let a2 = 2.0 * sum_d2 * k + 4.0 * sum_od * sum_od + 4.0 * r2 * dz * dz;
    let a1 = 4.0 * k * sum_od + 8.0 * r2 * oz * dz;
    let a0 = k * k - 4.0 * r2 * (s2 - oz * oz);

    let roots = solve_quartic(a4, a3, a2, a1, a0);

    let mut hits = Vec::new();
    for t in roots {
        if t >= -1e-10 {
            let t = t.max(0.0);
            let point = ray.at(t);
            let normal = torus_normal(torus, &point);
            hits.push(RaySurfaceHit { point, t, normal });
        }
    }

    hits.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap());
    // Deduplicate near-coincident hits (tangent touches)
    hits.dedup_by(|a, b| (a.t - b.t).abs() < 1e-8);
    hits
}

/// Transform a ray into the torus local coordinate frame (center=origin, axis=Z).
fn to_torus_local(ray: &Ray, torus: &Torus) -> (Point3d, Vec3) {
    let z_axis = torus.axis;
    let x_axis = if z_axis.x.abs() < 0.9 {
        Vec3::X.cross(&z_axis).normalize()
    } else {
        Vec3::Y.cross(&z_axis).normalize()
    };
    let y_axis = z_axis.cross(&x_axis);

    let rel = ray.origin - torus.center;
    let local_origin = Point3d::new(
        rel.dot(&x_axis),
        rel.dot(&y_axis),
        rel.dot(&z_axis),
    );
    let local_dir = Vec3::new(
        ray.direction.dot(&x_axis),
        ray.direction.dot(&y_axis),
        ray.direction.dot(&z_axis),
    );
    (local_origin, local_dir)
}

/// Compute the outward normal of a torus at a given point.
fn torus_normal(torus: &Torus, point: &Point3d) -> Vec3 {
    let v = *point - torus.center;
    let along_axis = v.dot(&torus.axis);
    let radial = v - torus.axis * along_axis;
    let radial_len = radial.length();
    if radial_len < 1e-15 {
        return torus.axis;
    }
    let radial_dir = radial * (1.0 / radial_len);
    // Center of the tube circle nearest to the point
    let tube_center = torus.center + radial_dir * torus.major_radius;
    (*point - tube_center).normalize()
}

// ─── Quartic Solver (Ferrari's method) ─────────────────────────────────────

/// Solve a quartic equation: a*x^4 + b*x^3 + c*x^2 + d*x + e = 0
/// Returns all real roots.
fn solve_quartic(a: f64, b: f64, c: f64, d: f64, e: f64) -> Vec<f64> {
    if a.abs() < 1e-15 {
        return solve_cubic(b, c, d, e);
    }

    // Normalize: x^4 + px^3 + qx^2 + rx + s = 0
    let p = b / a;
    let q = c / a;
    let r = d / a;
    let s = e / a;

    // Depressed quartic via substitution x = t - p/4:
    // t^4 + alpha*t^2 + beta*t + gamma = 0
    let p2 = p * p;
    let alpha = q - 3.0 * p2 / 8.0;
    let beta = r - p * q / 2.0 + p2 * p / 8.0;
    let gamma = s - p * r / 4.0 + p2 * q / 16.0 - 3.0 * p2 * p2 / 256.0;

    let shift = -p / 4.0;

    if beta.abs() < 1e-15 {
        // Biquadratic: t^4 + alpha*t^2 + gamma = 0
        let disc = alpha * alpha - 4.0 * gamma;
        if disc < -1e-15 {
            return vec![];
        }
        let disc = disc.max(0.0).sqrt();
        let mut roots = Vec::new();
        for u2 in [(-alpha + disc) / 2.0, (-alpha - disc) / 2.0] {
            if u2 >= -1e-15 {
                let u = u2.max(0.0).sqrt();
                roots.push(u + shift);
                if u > 1e-10 {
                    roots.push(-u + shift);
                }
            }
        }
        return roots;
    }

    // Ferrari's method: find a root of the resolvent cubic
    // y^3 - alpha/2 * y^2 - gamma * y + (alpha*gamma - beta^2)/2 = 0
    // which we write as: y^3 + c2*y^2 + c1*y + c0 = 0
    let c2 = -alpha / 2.0;
    let c1 = -gamma;
    let c0 = (alpha * gamma - beta * beta) / 2.0;

    let cubic_roots = solve_cubic(1.0, c2, c1, c0);

    // Pick the resolvent root that gives a positive discriminant
    let mut y = cubic_roots[0];
    for &yr in &cubic_roots {
        if 2.0 * yr - alpha > 1e-15 {
            y = yr;
            break;
        }
    }

    let w2 = 2.0 * y - alpha;
    if w2 < -1e-12 {
        return vec![];
    }
    let w = w2.max(0.0).sqrt();

    if w.abs() < 1e-12 {
        // Degenerate case
        return vec![];
    }

    let mut roots = Vec::new();

    // Two quadratics: t^2 + w*t + (y + beta/(2w)) = 0
    //                 t^2 - w*t + (y - beta/(2w)) = 0
    let bw = beta / (2.0 * w);
    for (sign_w, offset) in [(1.0, y + bw), (-1.0, y - bw)] {
        let disc = sign_w * sign_w * w * w / 4.0 - offset;
        if disc >= -1e-12 {
            let sq = disc.max(0.0).sqrt();
            roots.push(-sign_w * w / 2.0 + sq + shift);
            roots.push(-sign_w * w / 2.0 - sq + shift);
        }
    }

    roots
}

/// Solve a cubic equation: a*x^3 + b*x^2 + c*x + d = 0
/// Returns all real roots using Cardano's method.
fn solve_cubic(a: f64, b: f64, c: f64, d: f64) -> Vec<f64> {
    if a.abs() < 1e-15 {
        return solve_quadratic(b, c, d);
    }

    // Normalize: x^3 + px^2 + qx + r = 0
    let p = b / a;
    let q = c / a;
    let r = d / a;

    // Depressed cubic via x = t - p/3: t^3 + at + b = 0
    let a_dep = q - p * p / 3.0;
    let b_dep = r - p * q / 3.0 + 2.0 * p * p * p / 27.0;
    let shift = -p / 3.0;

    let disc = -4.0 * a_dep * a_dep * a_dep - 27.0 * b_dep * b_dep;

    if disc > 1e-15 {
        // Three distinct real roots (trigonometric method)
        let m = (-a_dep / 3.0).sqrt();
        let theta = (-b_dep / (2.0 * m * m * m)).acos() / 3.0;
        let two_pi_3 = 2.0 * std::f64::consts::PI / 3.0;
        vec![
            2.0 * m * theta.cos() + shift,
            2.0 * m * (theta - two_pi_3).cos() + shift,
            2.0 * m * (theta + two_pi_3).cos() + shift,
        ]
    } else {
        // One real root (Cardano's formula)
        let half_b = b_dep / 2.0;
        let q3_over_27 = a_dep * a_dep * a_dep / 27.0;
        let inner = half_b * half_b + q3_over_27;
        let sqrt_inner = inner.max(0.0).sqrt();

        let u = cbrt(-half_b + sqrt_inner);
        let v = cbrt(-half_b - sqrt_inner);
        vec![u + v + shift]
    }
}

fn cbrt(x: f64) -> f64 {
    if x >= 0.0 {
        x.cbrt()
    } else {
        -(-x).cbrt()
    }
}

/// Solve a quadratic equation: a*x^2 + b*x + c = 0
fn solve_quadratic(a: f64, b: f64, c: f64) -> Vec<f64> {
    if a.abs() < 1e-15 {
        if b.abs() < 1e-15 {
            return vec![];
        }
        return vec![-c / b];
    }
    let disc = b * b - 4.0 * a * c;
    if disc < -1e-15 {
        return vec![];
    }
    let disc = disc.max(0.0).sqrt();
    vec![(-b + disc) / (2.0 * a), (-b - disc) / (2.0 * a)]
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

    // ── Ray-Cone Tests ─────────────────────────────────────────────

    #[test]
    fn test_ray_cone_through_axis() {
        // Cone with apex at origin, axis along Z, half-angle 45°
        let cone = Cone::new(Point3d::ORIGIN, Vec3::Z, std::f64::consts::FRAC_PI_4);
        // Ray along X at z=5 — should hit at x=±5
        let ray = Ray::new(Point3d::new(-10.0, 0.0, 5.0), Vec3::X);
        let hits = ray_cone(&ray, &cone);
        assert_eq!(hits.len(), 2, "Ray should hit cone twice, got {}", hits.len());
        // At z=5 with half_angle=45°, radius = 5
        assert!((hits[0].point.x - (-5.0)).abs() < 1e-8, "First hit x={}", hits[0].point.x);
        assert!((hits[1].point.x - 5.0).abs() < 1e-8, "Second hit x={}", hits[1].point.x);
    }

    #[test]
    fn test_ray_cone_along_axis() {
        // Ray along the cone axis — hits at apex only (tangent)
        let cone = Cone::new(Point3d::ORIGIN, Vec3::Z, std::f64::consts::FRAC_PI_4);
        let ray = Ray::new(Point3d::new(0.0, 0.0, -10.0), Vec3::Z);
        let hits = ray_cone(&ray, &cone);
        // Ray along axis of double cone: passes through apex
        assert!(!hits.is_empty(), "Ray along axis should hit cone");
        // One of the hits should be near the apex
        let near_apex = hits.iter().any(|h| h.point.distance_to(&Point3d::ORIGIN) < 1e-6);
        assert!(near_apex, "One hit should be near apex");
    }

    #[test]
    fn test_ray_cone_miss() {
        let cone = Cone::new(Point3d::ORIGIN, Vec3::Z, 0.1); // narrow cone
        // Ray perpendicular to axis but not intersecting: a ray parallel to Z that is
        // far from the axis will always hit the double cone (which extends infinitely).
        // Instead, use a ray perpendicular to the axis at a height where the cone
        // radius is smaller than the ray offset.
        // At z=1, radius = tan(0.1) ≈ 0.1003. Ray at y=5, z=1 along X misses.
        let ray = Ray::new(Point3d::new(-10.0, 5.0, 1.0), Vec3::X);
        let hits = ray_cone(&ray, &cone);
        assert!(hits.is_empty(), "Ray should miss narrow cone, got {} hits", hits.len());
    }

    #[test]
    fn test_ray_cone_tangent() {
        // Cone with half-angle 45°, apex at origin, axis along Z
        let cone = Cone::new(Point3d::ORIGIN, Vec3::Z, std::f64::consts::FRAC_PI_4);
        // Ray tangent to cone: at z=1, the cone radius is 1.
        // A ray at y=1 parallel to X at z=1 should be tangent
        let ray = Ray::new(Point3d::new(-10.0, 1.0, 1.0), Vec3::X);
        let hits = ray_cone(&ray, &cone);
        // Tangent means discriminant ≈ 0, so we get 1 or 2 very close hits
        assert!(!hits.is_empty(), "Tangent ray should produce hits");
        if hits.len() == 2 {
            assert!((hits[0].t - hits[1].t).abs() < 1e-4,
                "Tangent hits should be nearly coincident");
        }
    }

    #[test]
    fn test_ray_cone_perpendicular_to_axis() {
        // Cone apex at (0,0,0), axis Z, half-angle 30°
        let angle = std::f64::consts::PI / 6.0; // 30°
        let cone = Cone::new(Point3d::ORIGIN, Vec3::Z, angle);
        // Ray perpendicular to axis at z=10
        let ray = Ray::new(Point3d::new(-20.0, 0.0, 10.0), Vec3::X);
        let hits = ray_cone(&ray, &cone);
        assert_eq!(hits.len(), 2, "Should get 2 hits");
        // At z=10, radius = 10 * tan(30°)
        let expected_r = 10.0 * angle.tan();
        assert!((hits[0].point.x.abs() - expected_r).abs() < 1e-6,
            "Hit at x={}, expected ±{}", hits[0].point.x, expected_r);
    }

    #[test]
    fn test_ray_cone_double_cone() {
        // Double cone: a ray perpendicular to axis that passes through both nappes.
        // Apex at (0,0,5), axis Z, half-angle 45°.
        // At z=8 (3 above apex), upper nappe radius = 3. At z=2 (3 below apex), lower nappe radius = 3.
        // A vertical ray at x=1, y=0 should hit the upper nappe at z=5+1=6 and z=5-1=4.
        let cone = Cone::new(Point3d::new(0.0, 0.0, 5.0), Vec3::Z, std::f64::consts::FRAC_PI_4);
        let ray = Ray::new(Point3d::new(1.0, 0.0, -10.0), Vec3::Z);
        let hits = ray_cone(&ray, &cone);
        assert_eq!(hits.len(), 2, "Ray should hit both nappes of double cone, got {}", hits.len());
        let mut z_vals: Vec<f64> = hits.iter().map(|h| h.point.z).collect();
        z_vals.sort_by(|a, b| a.partial_cmp(b).unwrap());
        // At x=1 with half-angle 45°, the cone surface is at z = apex_z ± 1
        assert!((z_vals[0] - 4.0).abs() < 1e-6, "Lower nappe hit z={}, expected 4.0", z_vals[0]);
        assert!((z_vals[1] - 6.0).abs() < 1e-6, "Upper nappe hit z={}, expected 6.0", z_vals[1]);
    }

    #[test]
    fn test_ray_cone_normals_point_outward() {
        let cone = Cone::new(Point3d::ORIGIN, Vec3::Z, std::f64::consts::FRAC_PI_4);
        let ray = Ray::new(Point3d::new(-10.0, 0.0, 5.0), Vec3::X);
        let hits = ray_cone(&ray, &cone);
        for hit in &hits {
            let to_point = hit.point - cone.apex;
            let radial = to_point - cone.axis * to_point.dot(&cone.axis);
            // Normal should have a component in the radial direction
            assert!(hit.normal.dot(&radial) > -1e-6,
                "Normal should point outward from cone axis");
        }
    }

    // ── Ray-Torus Tests ────────────────────────────────────────────

    #[test]
    fn test_ray_torus_through_center() {
        // Torus at origin, axis Z, R=5, r=1
        let torus = Torus::new(Point3d::ORIGIN, Vec3::Z, 5.0, 1.0);
        // Ray along X axis — should hit 4 times (enter outer, exit inner, enter inner, exit outer)
        let ray = Ray::new(Point3d::new(-10.0, 0.0, 0.0), Vec3::X);
        let hits = ray_torus(&ray, &torus);
        assert_eq!(hits.len(), 4, "Ray through torus center should hit 4 times, got {}", hits.len());
        // Hits at x = -6, -4, 4, 6
        let expected_x = [-6.0, -4.0, 4.0, 6.0];
        for (hit, &ex) in hits.iter().zip(expected_x.iter()) {
            assert!((hit.point.x - ex).abs() < 1e-6,
                "Expected x={}, got x={}", ex, hit.point.x);
        }
    }

    #[test]
    fn test_ray_torus_miss() {
        let torus = Torus::new(Point3d::ORIGIN, Vec3::Z, 5.0, 1.0);
        // Ray far above the torus
        let ray = Ray::new(Point3d::new(-10.0, 0.0, 10.0), Vec3::X);
        let hits = ray_torus(&ray, &torus);
        assert!(hits.is_empty(), "Ray far above torus should miss");
    }

    #[test]
    fn test_ray_torus_along_axis() {
        let torus = Torus::new(Point3d::ORIGIN, Vec3::Z, 5.0, 1.0);
        // Ray along the torus axis (Z axis)
        let ray = Ray::new(Point3d::new(0.0, 0.0, -10.0), Vec3::Z);
        let hits = ray_torus(&ray, &torus);
        // The torus tube doesn't cross the axis (R=5 > r=1), so ray along axis misses
        assert!(hits.is_empty(), "Ray along axis should miss torus when R > r");
    }

    #[test]
    fn test_ray_torus_two_hits() {
        // Ray that grazes just one tube of the torus
        let torus = Torus::new(Point3d::ORIGIN, Vec3::Z, 5.0, 1.0);
        // Ray at y=0, z=0 hitting from above at x=5 (center of tube)
        let ray = Ray::new(Point3d::new(5.0, 0.0, 10.0), -Vec3::Z);
        let hits = ray_torus(&ray, &torus);
        assert_eq!(hits.len(), 2, "Ray through one tube should hit twice, got {}", hits.len());
        // Hits at z = ±1 (tube radius)
        assert!((hits[0].point.z - 1.0).abs() < 1e-6, "First hit z={}", hits[0].point.z);
        assert!((hits[1].point.z - (-1.0)).abs() < 1e-6, "Second hit z={}", hits[1].point.z);
    }

    #[test]
    fn test_ray_torus_normals_point_outward() {
        let torus = Torus::new(Point3d::ORIGIN, Vec3::Z, 5.0, 1.0);
        let ray = Ray::new(Point3d::new(-10.0, 0.0, 0.0), Vec3::X);
        let hits = ray_torus(&ray, &torus);
        for hit in &hits {
            // Normal at hit point should point away from the nearest tube center
            let v = hit.point - torus.center;
            let along = torus.axis * v.dot(&torus.axis);
            let radial = v - along;
            let radial_dir = radial.normalize();
            let tube_center = torus.center + radial_dir * torus.major_radius;
            let to_surface = hit.point - tube_center;
            // Normal should be roughly aligned with to_surface direction
            let dot = hit.normal.dot(&to_surface.normalize());
            assert!(dot > 0.5, "Normal should point outward from tube center, dot={}", dot);
        }
    }

    #[test]
    fn test_ray_torus_offset_center() {
        // Torus not at origin
        let torus = Torus::new(Point3d::new(10.0, 0.0, 0.0), Vec3::Z, 3.0, 0.5);
        let ray = Ray::new(Point3d::new(0.0, 0.0, 0.0), Vec3::X);
        let hits = ray_torus(&ray, &torus);
        // Should hit: enter at x=10-3-0.5=6.5, exit at x=10-3+0.5=7.5,
        //             enter at x=10+3-0.5=12.5, exit at x=10+3+0.5=13.5
        assert_eq!(hits.len(), 4, "Should get 4 hits, got {}", hits.len());
        let expected_x = [6.5, 7.5, 12.5, 13.5];
        for (hit, &ex) in hits.iter().zip(expected_x.iter()) {
            assert!((hit.point.x - ex).abs() < 1e-4,
                "Expected x={}, got x={}", ex, hit.point.x);
        }
    }

    #[test]
    fn test_ray_torus_tangent_outer() {
        // Ray tangent to the outer edge of torus
        let torus = Torus::new(Point3d::ORIGIN, Vec3::Z, 5.0, 1.0);
        // Outer radius = R + r = 6. Ray at y=6, parallel to X
        let ray = Ray::new(Point3d::new(-10.0, 6.0, 0.0), Vec3::X);
        let hits = ray_torus(&ray, &torus);
        // Tangent to outer surface — should get 0 or 2 very close hits
        assert!(hits.len() <= 2, "Tangent ray should produce <= 2 hits, got {}", hits.len());
    }

    // ── Quartic/Cubic Solver Tests ─────────────────────────────────

    #[test]
    fn test_solve_quadratic() {
        // x^2 - 5x + 6 = 0 -> x = 2, 3
        let roots = solve_quadratic(1.0, -5.0, 6.0);
        assert_eq!(roots.len(), 2);
        let mut sorted = roots.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert!((sorted[0] - 2.0).abs() < 1e-10);
        assert!((sorted[1] - 3.0).abs() < 1e-10);
    }

    #[test]
    fn test_solve_cubic_three_roots() {
        // (x-1)(x-2)(x-3) = x^3 - 6x^2 + 11x - 6
        let roots = solve_cubic(1.0, -6.0, 11.0, -6.0);
        assert_eq!(roots.len(), 3, "Should have 3 roots, got {:?}", roots);
        let mut sorted = roots.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert!((sorted[0] - 1.0).abs() < 1e-8, "Root 0: {}", sorted[0]);
        assert!((sorted[1] - 2.0).abs() < 1e-8, "Root 1: {}", sorted[1]);
        assert!((sorted[2] - 3.0).abs() < 1e-8, "Root 2: {}", sorted[2]);
    }

    #[test]
    fn test_solve_quartic_four_roots() {
        // (x-1)(x-2)(x-3)(x-4) = x^4 - 10x^3 + 35x^2 - 50x + 24
        let roots = solve_quartic(1.0, -10.0, 35.0, -50.0, 24.0);
        let mut sorted = roots.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        sorted.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
        assert_eq!(sorted.len(), 4, "Should have 4 roots, got {:?}", sorted);
        assert!((sorted[0] - 1.0).abs() < 1e-6, "Root 0: {}", sorted[0]);
        assert!((sorted[1] - 2.0).abs() < 1e-6, "Root 1: {}", sorted[1]);
        assert!((sorted[2] - 3.0).abs() < 1e-6, "Root 2: {}", sorted[2]);
        assert!((sorted[3] - 4.0).abs() < 1e-6, "Root 3: {}", sorted[3]);
    }

    #[test]
    fn test_solve_quartic_biquadratic() {
        // x^4 - 5x^2 + 4 = (x^2-1)(x^2-4) -> x = ±1, ±2
        let roots = solve_quartic(1.0, 0.0, -5.0, 0.0, 4.0);
        let mut sorted = roots.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        sorted.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
        assert_eq!(sorted.len(), 4, "Should have 4 roots, got {:?}", sorted);
        assert!((sorted[0] - (-2.0)).abs() < 1e-6);
        assert!((sorted[1] - (-1.0)).abs() < 1e-6);
        assert!((sorted[2] - 1.0).abs() < 1e-6);
        assert!((sorted[3] - 2.0).abs() < 1e-6);
    }
}
