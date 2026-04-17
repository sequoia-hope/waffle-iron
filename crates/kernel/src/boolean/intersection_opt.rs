//! Yang 2025 Section 4.3: Intersection optimization.
//!
//! After mesh intersection (Cherchi), each intersection point maps to approximate
//! parametric positions on both surfaces. These generally don't coincide in 3D.
//! This module iteratively refines them until they converge to the same point.
//!
//! Two methods per Yang Appendix C:
//! - Newton: algebraic root-finding on D(u,v,s,t) = S_A(u,v) - S_B(s,t)
//! - Geometric: tangent plane intersection → line projection → back-projection
//!
//! Ref [#24]: Yang et al. 2025 Section 4.3, Appendix C.

use crate::geometry::surface::SurfaceGeom;

/// Result of a successful intersection point optimization.
#[derive(Debug, Clone)]
pub(crate) struct OptimizedPoint {
    /// Refined parametric coordinates on surface A.
    pub params_a: (f64, f64),
    /// Refined parametric coordinates on surface B.
    pub params_b: (f64, f64),
    /// Converged 3D position (midpoint of the two surface evaluations).
    pub position: [f64; 3],
    /// Number of iterations to converge.
    pub iterations: usize,
}

/// Error during optimization.
#[derive(Debug)]
pub(crate) enum OptimError {
    /// Did not converge within max iterations.
    NotConverged { residual: f64, iterations: usize },
    /// Degenerate Jacobian (parallel tangent planes, singular matrix).
    DegenerateJacobian,
}

/// Newton's method for intersection point optimization.
///
/// Per Yang 2025 Appendix C:
/// D(u,v,s,t) = S_A(u,v) - S_B(s,t)
/// ∇D = [∂S_A/∂u, ∂S_A/∂v, -∂S_B/∂s, -∂S_B/∂t]  (3×4 Jacobian)
/// Solve ∇D∇Dᵀ aₖ = D  (3×3 normal equations)
/// Update x += ∇Dᵀ aₖ
///
/// Terminates when ||D|| < d_p.
pub(crate) fn newton_optimize(
    surface_a: &SurfaceGeom,
    surface_b: &SurfaceGeom,
    seed_a: (f64, f64),
    seed_b: (f64, f64),
    d_p: f64,
    max_iter: usize,
) -> Result<OptimizedPoint, OptimError> {
    let mut ua = seed_a.0;
    let mut va = seed_a.1;
    let mut sb = seed_b.0;
    let mut tb = seed_b.1;

    for iter in 0..max_iter {
        // Evaluate both surfaces at current parameters.
        let pa = surface_a.evaluate(ua, va);
        let pb = surface_b.evaluate(sb, tb);

        // Residual: we want to minimize ||S_A(u,v) - S_B(s,t)||.
        // Per Yang Appendix C: solve ∇D∇Dᵀ a = (S_B - S_A), then update x += ∇Dᵀ a.
        let d = [pb.x - pa.x, pb.y - pa.y, pb.z - pa.z]; // RHS = S_B - S_A
        let residual = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();

        if residual < d_p {
            return Ok(OptimizedPoint {
                params_a: (ua, va),
                params_b: (sb, tb),
                position: [
                    (pa.x + pb.x) * 0.5,
                    (pa.y + pb.y) * 0.5,
                    (pa.z + pb.z) * 0.5,
                ],
                iterations: iter,
            });
        }

        // Build Jacobian columns: [∂S_A/∂u, ∂S_A/∂v, -∂S_B/∂s, -∂S_B/∂t]
        let ta_u = surface_a.tangent_u(ua, va);
        let ta_v = surface_a.tangent_v(ua, va);
        let tb_s = surface_b.tangent_u(sb, tb);
        let tb_t = surface_b.tangent_v(sb, tb);

        // J is 3×4: columns are ta_u, ta_v, -tb_s, -tb_t
        // JJᵀ is 3×3
        let j = [
            [ta_u.x, ta_v.x, -tb_s.x, -tb_t.x],
            [ta_u.y, ta_v.y, -tb_s.y, -tb_t.y],
            [ta_u.z, ta_v.z, -tb_s.z, -tb_t.z],
        ];

        // Compute JJᵀ (3×3 symmetric)
        let mut jjt = [[0.0f64; 3]; 3];
        for i in 0..3 {
            for k in 0..3 {
                let mut sum = 0.0;
                for c in 0..4 {
                    sum += j[i][c] * j[k][c];
                }
                jjt[i][k] = sum;
            }
        }

        // Solve JJᵀ a = d via Cramer's rule (3×3)
        let a = solve_3x3_cramer(&jjt, &d);
        let a = match a {
            Some(a) => a,
            None => return Err(OptimError::DegenerateJacobian),
        };

        // Compute update: delta = Jᵀ a (4-vector)
        let delta = [
            j[0][0] * a[0] + j[1][0] * a[1] + j[2][0] * a[2], // δu
            j[0][1] * a[0] + j[1][1] * a[1] + j[2][1] * a[2], // δv
            j[0][2] * a[0] + j[1][2] * a[1] + j[2][2] * a[2], // δs
            j[0][3] * a[0] + j[1][3] * a[1] + j[2][3] * a[2], // δt
        ];

        ua += delta[0];
        va += delta[1];
        sb += delta[2];
        tb += delta[3];
    }

    // Did not converge
    let pa = surface_a.evaluate(ua, va);
    let pb = surface_b.evaluate(sb, tb);
    let d = [pa.x - pb.x, pa.y - pb.y, pa.z - pb.z];
    let residual = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    Err(OptimError::NotConverged {
        residual,
        iterations: max_iter,
    })
}

/// Geometric method for intersection point optimization.
///
/// Per Yang 2025 Section 4.3.2:
/// 1. Build tangent planes P_A, P_B at current params
/// 2. L = P_A ∩ P_B (intersection line)
/// 3. Project both surface points onto L → new 3D point
/// 4. Back-project to both surfaces → new params
///
/// "Used only when the two tangent planes are not parallel."
pub(crate) fn geometric_optimize(
    surface_a: &SurfaceGeom,
    surface_b: &SurfaceGeom,
    seed_a: (f64, f64),
    seed_b: (f64, f64),
    d_p: f64,
    max_iter: usize,
) -> Result<OptimizedPoint, OptimError> {
    let mut ua = seed_a.0;
    let mut va = seed_a.1;
    let mut sb = seed_b.0;
    let mut tb = seed_b.1;

    for iter in 0..max_iter {
        let pa = surface_a.evaluate(ua, va);
        let pb = surface_b.evaluate(sb, tb);

        let d = [pa.x - pb.x, pa.y - pb.y, pa.z - pb.z];
        let residual = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();

        if residual < d_p {
            return Ok(OptimizedPoint {
                params_a: (ua, va),
                params_b: (sb, tb),
                position: [
                    (pa.x + pb.x) * 0.5,
                    (pa.y + pb.y) * 0.5,
                    (pa.z + pb.z) * 0.5,
                ],
                iterations: iter,
            });
        }

        // Compute surface normals at current params
        let na = surface_a.normal_at(ua, va);
        let nb = surface_b.normal_at(sb, tb);

        // Intersection line direction: L = nA × nB
        let line_dir = [
            na.y * nb.z - na.z * nb.y,
            na.z * nb.x - na.x * nb.z,
            na.x * nb.y - na.y * nb.x,
        ];
        let line_len_sq =
            line_dir[0] * line_dir[0] + line_dir[1] * line_dir[1] + line_dir[2] * line_dir[2];

        if line_len_sq < 1e-30 {
            // Tangent planes are parallel — fall back to Newton
            return newton_optimize(
                surface_a,
                surface_b,
                (ua, va),
                (sb, tb),
                d_p,
                max_iter - iter,
            );
        }

        // Project midpoint of (pA, pB) onto line L.
        // Line passes through midpoint in direction line_dir.
        let mid = [
            (pa.x + pb.x) * 0.5,
            (pa.y + pb.y) * 0.5,
            (pa.z + pb.z) * 0.5,
        ];

        // Back-project the midpoint onto both surfaces to get new params.
        let proj_a =
            surface_a.project_point(crate::geometry::point::Point3::new(mid[0], mid[1], mid[2]));
        let proj_b =
            surface_b.project_point(crate::geometry::point::Point3::new(mid[0], mid[1], mid[2]));

        // Update parametric coordinates via inverse evaluation.
        if let Some((new_ua, new_va)) = surface_a.inverse_evaluate(proj_a) {
            ua = new_ua;
            va = new_va;
        }
        if let Some((new_sb, new_tb)) = surface_b.inverse_evaluate(proj_b) {
            sb = new_sb;
            tb = new_tb;
        }
    }

    let pa = surface_a.evaluate(ua, va);
    let pb = surface_b.evaluate(sb, tb);
    let d = [pa.x - pb.x, pa.y - pb.y, pa.z - pb.z];
    let residual = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    Err(OptimError::NotConverged {
        residual,
        iterations: max_iter,
    })
}

/// Solve 3×3 system Ax = b via Cramer's rule.
/// Returns None if determinant is near-zero (singular matrix).
fn solve_3x3_cramer(a: &[[f64; 3]; 3], b: &[f64; 3]) -> Option<[f64; 3]> {
    let det = a[0][0] * (a[1][1] * a[2][2] - a[1][2] * a[2][1])
        - a[0][1] * (a[1][0] * a[2][2] - a[1][2] * a[2][0])
        + a[0][2] * (a[1][0] * a[2][1] - a[1][1] * a[2][0]);

    if det.abs() < 1e-30 {
        return None;
    }

    let inv_det = 1.0 / det;

    let x0 = (b[0] * (a[1][1] * a[2][2] - a[1][2] * a[2][1])
        - a[0][1] * (b[1] * a[2][2] - a[1][2] * b[2])
        + a[0][2] * (b[1] * a[2][1] - a[1][1] * b[2]))
        * inv_det;

    let x1 = (a[0][0] * (b[1] * a[2][2] - a[1][2] * b[2])
        - b[0] * (a[1][0] * a[2][2] - a[1][2] * a[2][0])
        + a[0][2] * (a[1][0] * b[2] - b[1] * a[2][0]))
        * inv_det;

    let x2 = (a[0][0] * (a[1][1] * b[2] - b[1] * a[2][1])
        - a[0][1] * (a[1][0] * b[2] - b[1] * a[2][0])
        + b[0] * (a[1][0] * a[2][1] - a[1][1] * a[2][0]))
        * inv_det;

    Some([x0, x1, x2])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::point::{Point3, Vector3};
    use crate::geometry::surface::{Cylinder, Plane, Sphere};

    #[test]
    fn newton_plane_plane_converges() {
        // Two planes intersecting along a line.
        // Plane A: z = 0 (normal = +Z)
        // Plane B: y = 0 (normal = +Y)
        // Intersection line: y=0, z=0 (the x-axis).
        let sa = SurfaceGeom::Planar(Plane {
            origin: Point3::new(0.0, 0.0, 0.0),
            normal: Vector3::new(0.0, 0.0, 1.0),
        });
        let sb = SurfaceGeom::Planar(Plane {
            origin: Point3::new(0.0, 0.0, 0.0),
            normal: Vector3::new(0.0, 1.0, 0.0),
        });

        // Seeds: slightly off the intersection line.
        let result = newton_optimize(&sa, &sb, (1.0, 0.5), (1.0, 0.3), 1e-7, 50);
        let opt = result.expect("Newton should converge for plane-plane");
        assert!(opt.iterations <= 5, "Should converge quickly for planes");
        // Position should be on the x-axis (y≈0, z≈0)
        assert!(opt.position[1].abs() < 1e-6, "y should be ~0");
        assert!(opt.position[2].abs() < 1e-6, "z should be ~0");
    }

    #[test]
    fn newton_plane_sphere_converges() {
        // Plane z=0 and unit sphere at origin.
        // Intersection: circle x²+y²=1 at z=0.
        let sa = SurfaceGeom::Planar(Plane {
            origin: Point3::new(0.0, 0.0, 0.0),
            normal: Vector3::new(0.0, 0.0, 1.0),
        });
        let sb = SurfaceGeom::Spherical(Sphere {
            center: Point3::new(0.0, 0.0, 0.0),
            radius: 1.0,
        });

        // Seed: approximately on the circle at (1, 0, 0).
        // Plane params (u, v) → point (u, v, 0) with frame x=[1,0,0], y=[0,1,0]
        // Sphere params (u, v) → u=0, v=0 → (1, 0, 0)
        let result = newton_optimize(&sa, &sb, (1.0, 0.0), (0.0, 0.0), 1e-7, 50);
        let opt = result.expect("Newton should converge for plane-sphere");
        // Position should be on the unit circle at z=0.
        let r = (opt.position[0] * opt.position[0] + opt.position[1] * opt.position[1]).sqrt();
        assert!(
            (r - 1.0).abs() < 1e-5,
            "Distance from origin should be ~1.0, got {r}"
        );
        assert!(opt.position[2].abs() < 1e-5, "z should be ~0");
    }

    #[test]
    fn geometric_plane_cylinder_converges() {
        // Plane z=0.5 and cylinder radius=1 along z-axis.
        // Intersection: circle x²+y²=1 at z=0.5.
        let sa = SurfaceGeom::Planar(Plane {
            origin: Point3::new(0.0, 0.0, 0.5),
            normal: Vector3::new(0.0, 0.0, 1.0),
        });
        let sb = SurfaceGeom::Cylindrical(Cylinder {
            origin: Point3::new(0.0, 0.0, 0.0),
            axis: Vector3::new(0.0, 0.0, 1.0),
            radius: 1.0,
        });

        // Seed: approximately on the circle at (1, 0, 0.5).
        let result = geometric_optimize(&sa, &sb, (1.0, 0.0), (0.0, 0.5), 1e-7, 50);
        let opt = result.expect("Geometric should converge for plane-cylinder");
        let r = (opt.position[0] * opt.position[0] + opt.position[1] * opt.position[1]).sqrt();
        assert!((r - 1.0).abs() < 1e-4, "Radius should be ~1.0, got {r}");
        assert!(
            (opt.position[2] - 0.5).abs() < 1e-4,
            "z should be ~0.5, got {}",
            opt.position[2]
        );
    }

    #[test]
    fn cramer_solves_identity() {
        let a = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        let b = [3.0, 5.0, 7.0];
        let x = solve_3x3_cramer(&a, &b).unwrap();
        assert!((x[0] - 3.0).abs() < 1e-12);
        assert!((x[1] - 5.0).abs() < 1e-12);
        assert!((x[2] - 7.0).abs() < 1e-12);
    }

    #[test]
    fn cramer_returns_none_for_singular() {
        let a = [[1.0, 0.0, 0.0], [0.0, 0.0, 0.0], [0.0, 0.0, 1.0]];
        let b = [1.0, 1.0, 1.0];
        assert!(solve_3x3_cramer(&a, &b).is_none());
    }
}
