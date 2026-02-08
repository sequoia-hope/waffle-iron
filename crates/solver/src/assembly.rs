//! 3D assembly constraint solver.
//!
//! Positions rigid bodies relative to each other using a Gauss-Newton solver
//! with Levenberg-Marquardt damping, reusing the same algorithm as the 2D sketch solver.
//!
//! Each rigid body has 6 DOF: position (x, y, z) + orientation as axis-angle (rx, ry, rz).
//! The axis-angle vector encodes both axis (direction) and angle (magnitude) via Rodrigues' formula.

use nalgebra::{DMatrix, Matrix3, Vector3, SVD};
use crate::solver::{SolverConfig, SolverResult, SolverError, SolverWarning};

// ── Data Structures ──────────────────────────────────────────────────────────

/// A rigid body in the assembly.
#[derive(Debug, Clone)]
pub struct AssemblyBody {
    /// Human-readable name.
    pub name: String,
    /// Index into `Assembly::params` where this body's 6 parameters start.
    /// Layout: [x, y, z, rx, ry, rz]
    pub param_offset: usize,
}

/// A face definition in body-local coordinates (point + outward normal).
#[derive(Debug, Clone)]
pub struct LocalFace {
    /// A point on the face, in body-local coordinates.
    pub point: [f64; 3],
    /// The outward normal of the face, in body-local coordinates.
    pub normal: [f64; 3],
}

/// An axis definition in body-local coordinates (point + direction).
#[derive(Debug, Clone)]
pub struct LocalAxis {
    /// A point on the axis, in body-local coordinates.
    pub point: [f64; 3],
    /// The direction of the axis, in body-local coordinates (should be unit-length).
    pub direction: [f64; 3],
}

/// Assembly constraint types.
#[derive(Debug, Clone)]
pub enum AssemblyConstraint {
    /// Lock a body at a specific position and orientation.
    /// Produces 6 residuals.
    FixedPosition {
        body: usize,
        position: [f64; 3],
        orientation: [f64; 3],
    },

    /// Two planar faces are coplanar with opposite normals (face-to-face contact).
    /// Produces 2 residuals: plane distance + normal anti-alignment.
    Mate {
        body_a: usize,
        face_a: LocalFace,
        body_b: usize,
        face_b: LocalFace,
    },

    /// Two planar faces are coplanar with same normals (back-to-back alignment).
    /// Produces 2 residuals: plane distance + normal alignment.
    Align {
        body_a: usize,
        face_a: LocalFace,
        body_b: usize,
        face_b: LocalFace,
    },

    /// Two planar faces are parallel with opposite normals at a specified distance.
    /// Produces 2 residuals: (plane distance - target) + normal anti-alignment.
    DistanceMate {
        body_a: usize,
        face_a: LocalFace,
        body_b: usize,
        face_b: LocalFace,
        distance: f64,
    },

    /// Two cylinder axes are collinear.
    /// Produces 3 residuals: distance between axes (2) + angle between directions (1).
    AxisAlign {
        body_a: usize,
        axis_a: LocalAxis,
        body_b: usize,
        axis_b: LocalAxis,
    },
}

/// A 3D assembly of rigid bodies with constraints.
#[derive(Debug, Clone)]
pub struct Assembly {
    /// Flat parameter vector: [x0, y0, z0, rx0, ry0, rz0, x1, y1, z1, rx1, ry1, rz1, ...]
    pub params: Vec<f64>,
    /// Rigid bodies in the assembly.
    pub bodies: Vec<AssemblyBody>,
    /// Constraints between bodies.
    pub constraints: Vec<AssemblyConstraint>,
}

impl Assembly {
    /// Create an empty assembly.
    pub fn new() -> Self {
        Self {
            params: Vec::new(),
            bodies: Vec::new(),
            constraints: Vec::new(),
        }
    }

    /// Add a rigid body with initial position and orientation (axis-angle).
    /// Returns the body index.
    pub fn add_body(&mut self, name: &str, position: [f64; 3], orientation: [f64; 3]) -> usize {
        let offset = self.params.len();
        self.params.extend_from_slice(&position);
        self.params.extend_from_slice(&orientation);
        let idx = self.bodies.len();
        self.bodies.push(AssemblyBody {
            name: name.to_string(),
            param_offset: offset,
        });
        idx
    }

    /// Add a constraint to the assembly.
    pub fn add_constraint(&mut self, constraint: AssemblyConstraint) {
        self.constraints.push(constraint);
    }

    /// Get the position of a body.
    pub fn body_position(&self, body_idx: usize) -> [f64; 3] {
        let off = self.bodies[body_idx].param_offset;
        [self.params[off], self.params[off + 1], self.params[off + 2]]
    }

    /// Get the orientation (axis-angle) of a body.
    pub fn body_orientation(&self, body_idx: usize) -> [f64; 3] {
        let off = self.bodies[body_idx].param_offset;
        [self.params[off + 3], self.params[off + 4], self.params[off + 5]]
    }
}

impl Default for Assembly {
    fn default() -> Self {
        Self::new()
    }
}

// ── Rotation Utilities (Rodrigues' Formula) ──────────────────────────────────

/// Compute rotation matrix from axis-angle vector using Rodrigues' formula.
/// v = (rx, ry, rz), angle = ||v||, axis = v / ||v||.
/// R = I + sin(theta) * K + (1 - cos(theta)) * K^2
/// where K is the skew-symmetric matrix of the axis.
fn rotation_matrix(v: &Vector3<f64>) -> Matrix3<f64> {
    let theta = v.norm();
    if theta < 1e-14 {
        return Matrix3::identity();
    }
    let axis = v / theta;
    let k = skew(&axis);
    let sin_t = theta.sin();
    let cos_t = theta.cos();
    Matrix3::identity() + sin_t * k + (1.0 - cos_t) * (k * k)
}

/// Skew-symmetric matrix for cross product: skew(a) * b = a x b.
fn skew(v: &Vector3<f64>) -> Matrix3<f64> {
    Matrix3::new(
        0.0, -v.z, v.y,
        v.z, 0.0, -v.x,
        -v.y, v.x, 0.0,
    )
}

/// Transform a body-local point to world coordinates.
/// world_point = R(orientation) * local_point + position
fn transform_point(params: &[f64], offset: usize, local: &[f64; 3]) -> Vector3<f64> {
    let pos = Vector3::new(params[offset], params[offset + 1], params[offset + 2]);
    let rot_v = Vector3::new(params[offset + 3], params[offset + 4], params[offset + 5]);
    let r = rotation_matrix(&rot_v);
    let lp = Vector3::new(local[0], local[1], local[2]);
    r * lp + pos
}

/// Transform a body-local direction to world coordinates (rotation only).
/// world_dir = R(orientation) * local_dir
fn transform_direction(params: &[f64], offset: usize, local: &[f64; 3]) -> Vector3<f64> {
    let rot_v = Vector3::new(params[offset + 3], params[offset + 4], params[offset + 5]);
    let r = rotation_matrix(&rot_v);
    let ld = Vector3::new(local[0], local[1], local[2]);
    r * ld
}

// ── Residual Computation ─────────────────────────────────────────────────────

/// Collect all residuals from constraints.
fn collect_residuals(assembly: &Assembly, params: &[f64]) -> Vec<f64> {
    let mut residuals = Vec::new();
    for c in &assembly.constraints {
        constraint_residuals(c, params, &assembly.bodies, &mut residuals);
    }
    residuals
}

/// Compute residuals for a single constraint.
fn constraint_residuals(
    c: &AssemblyConstraint,
    params: &[f64],
    bodies: &[AssemblyBody],
    out: &mut Vec<f64>,
) {
    match c {
        AssemblyConstraint::FixedPosition { body, position, orientation } => {
            let off = bodies[*body].param_offset;
            // 6 residuals: difference from target
            out.push(params[off] - position[0]);
            out.push(params[off + 1] - position[1]);
            out.push(params[off + 2] - position[2]);
            out.push(params[off + 3] - orientation[0]);
            out.push(params[off + 4] - orientation[1]);
            out.push(params[off + 5] - orientation[2]);
        }

        AssemblyConstraint::Mate { body_a, face_a, body_b, face_b } => {
            let off_a = bodies[*body_a].param_offset;
            let off_b = bodies[*body_b].param_offset;

            let pt_a = transform_point(params, off_a, &face_a.point);
            let n_a = transform_direction(params, off_a, &face_a.normal);
            let pt_b = transform_point(params, off_b, &face_b.point);
            let n_b = transform_direction(params, off_b, &face_b.normal);

            // Residual 1: signed distance between the two planes along n_a.
            // For coplanarity: (pt_b - pt_a) . n_a = 0
            let diff = pt_b - pt_a;
            out.push(diff.dot(&n_a));

            // Residual 2: normals should be anti-parallel: n_a . n_b = -1
            // (assuming both normals are unit-length in local coords)
            out.push(n_a.dot(&n_b) + 1.0);
        }

        AssemblyConstraint::DistanceMate { body_a, face_a, body_b, face_b, distance } => {
            let off_a = bodies[*body_a].param_offset;
            let off_b = bodies[*body_b].param_offset;

            let pt_a = transform_point(params, off_a, &face_a.point);
            let n_a = transform_direction(params, off_a, &face_a.normal);
            let pt_b = transform_point(params, off_b, &face_b.point);
            let n_b = transform_direction(params, off_b, &face_b.normal);

            // Residual 1: signed distance between planes along n_a should equal target distance.
            // (pt_b - pt_a) . n_a = distance
            let diff = pt_b - pt_a;
            out.push(diff.dot(&n_a) - distance);

            // Residual 2: normals should be anti-parallel: n_a . n_b = -1
            out.push(n_a.dot(&n_b) + 1.0);
        }

        AssemblyConstraint::Align { body_a, face_a, body_b, face_b } => {
            let off_a = bodies[*body_a].param_offset;
            let off_b = bodies[*body_b].param_offset;

            let pt_a = transform_point(params, off_a, &face_a.point);
            let n_a = transform_direction(params, off_a, &face_a.normal);
            let pt_b = transform_point(params, off_b, &face_b.point);
            let n_b = transform_direction(params, off_b, &face_b.normal);

            // Residual 1: coplanarity
            let diff = pt_b - pt_a;
            out.push(diff.dot(&n_a));

            // Residual 2: normals should be parallel: n_a . n_b = 1
            out.push(n_a.dot(&n_b) - 1.0);
        }

        AssemblyConstraint::AxisAlign { body_a, axis_a, body_b, axis_b } => {
            let off_a = bodies[*body_a].param_offset;
            let off_b = bodies[*body_b].param_offset;

            let pt_a = transform_point(params, off_a, &axis_a.point);
            let d_a = transform_direction(params, off_a, &axis_a.direction);
            let pt_b = transform_point(params, off_b, &axis_b.point);
            let d_b = transform_direction(params, off_b, &axis_b.direction);

            // For collinear axes:
            // 1. Directions must be parallel: cross product = 0 (2 residuals, use x and y of cross)
            let cross = d_a.cross(&d_b);
            out.push(cross.x);
            out.push(cross.y);

            // 2. Points must be on the same line: (pt_b - pt_a) x d_a should be zero.
            //    We use the magnitude of this cross product.
            let sep = pt_b - pt_a;
            let sep_cross = sep.cross(&d_a);
            // Use the norm as a single residual
            out.push(sep_cross.norm());
        }
    }
}

// ── Jacobian Computation ─────────────────────────────────────────────────────

/// Build Jacobian via finite differences. Row-major [m x n].
///
/// While analytic Jacobians would be more efficient, the rotation matrix
/// derivatives (dR/drx, dR/dry, dR/drz) are complex with axis-angle.
/// Finite differences are reliable and the assembly systems are small enough
/// that performance is not a concern.
fn build_jacobian(assembly: &Assembly, params: &[f64], m: usize, n: usize) -> Vec<f64> {
    let h = 1e-7;
    let base = collect_residuals(assembly, params);
    debug_assert_eq!(base.len(), m);

    let mut jac = vec![0.0; m * n];
    for j in 0..n {
        let mut p_plus = params.to_vec();
        p_plus[j] += h;
        let r_plus = collect_residuals(assembly, &p_plus);
        for i in 0..m {
            jac[i * n + j] = (r_plus[i] - base[i]) / h;
        }
    }
    jac
}

/// Compute the numerical rank of a Jacobian matrix using SVD.
fn jacobian_rank(jac_flat: &[f64], m: usize, n: usize) -> usize {
    let mat = DMatrix::from_row_slice(m, n, jac_flat);
    let svd = SVD::new(mat, false, true);
    let sv = &svd.singular_values;
    let max_sv = sv.iter().cloned().fold(0.0_f64, f64::max);
    let threshold = max_sv * (m.max(n) as f64) * f64::EPSILON;
    sv.iter().filter(|&&s| s > threshold).count()
}

/// Analyze constraints for over/under-constrained conditions.
fn analyze_constraints(assembly: &Assembly, params: &[f64]) -> (usize, Vec<SolverWarning>) {
    let n = params.len();
    let mut warnings = Vec::new();
    if n == 0 || assembly.constraints.is_empty() {
        return (n, warnings);
    }

    let residuals = collect_residuals(assembly, params);
    let m = residuals.len();
    if m == 0 {
        return (n, warnings);
    }

    let jac_flat = build_jacobian(assembly, params, m, n);
    let rank = jacobian_rank(&jac_flat, m, n);
    let dof = n.saturating_sub(rank);

    if m > rank {
        warnings.push(SolverWarning::OverConstrained {
            redundant_constraints: m - rank,
        });
    }
    if dof > 0 {
        warnings.push(SolverWarning::UnderConstrained {
            dof,
            free_params: Vec::new(),
        });
    }
    (dof, warnings)
}

// ── Linear System Solver ─────────────────────────────────────────────────────

/// Solve A*x = b via Gaussian elimination with partial pivoting. A is n x n row-major.
fn solve_linear_system(a: &[f64], b: &[f64], n: usize) -> Option<Vec<f64>> {
    let mut aug = vec![0.0; n * (n + 1)];
    for i in 0..n {
        for j in 0..n {
            aug[i * (n + 1) + j] = a[i * n + j];
        }
        aug[i * (n + 1) + n] = b[i];
    }

    for col in 0..n {
        let mut max_val = aug[col * (n + 1) + col].abs();
        let mut max_row = col;
        for row in (col + 1)..n {
            let val = aug[row * (n + 1) + col].abs();
            if val > max_val {
                max_val = val;
                max_row = row;
            }
        }
        if max_val < 1e-15 {
            return None;
        }
        if max_row != col {
            for j in 0..=n {
                let tmp = aug[col * (n + 1) + j];
                aug[col * (n + 1) + j] = aug[max_row * (n + 1) + j];
                aug[max_row * (n + 1) + j] = tmp;
            }
        }
        let pivot = aug[col * (n + 1) + col];
        for row in (col + 1)..n {
            let factor = aug[row * (n + 1) + col] / pivot;
            for j in col..=n {
                aug[row * (n + 1) + j] -= factor * aug[col * (n + 1) + j];
            }
        }
    }

    let mut x = vec![0.0; n];
    for i in (0..n).rev() {
        let mut sum = aug[i * (n + 1) + n];
        for j in (i + 1)..n {
            sum -= aug[i * (n + 1) + j] * x[j];
        }
        let diag = aug[i * (n + 1) + i];
        if diag.abs() < 1e-15 {
            return None;
        }
        x[i] = sum / diag;
    }
    Some(x)
}

// ── Public API ───────────────────────────────────────────────────────────────

/// Compute the degrees of freedom of an assembly.
/// DOF = total_params - rank(Jacobian).
pub fn assembly_dof(assembly: &Assembly) -> usize {
    let n = assembly.params.len();
    if n == 0 {
        return 0;
    }
    if assembly.constraints.is_empty() {
        return n;
    }

    let residuals = collect_residuals(assembly, &assembly.params);
    let m = residuals.len();
    if m == 0 {
        return n;
    }

    let jac_flat = build_jacobian(assembly, &assembly.params, m, n);
    let rank = jacobian_rank(&jac_flat, m, n);
    n.saturating_sub(rank)
}

/// Solve assembly constraints using Gauss-Newton with Levenberg-Marquardt damping.
///
/// Follows the same algorithm as `solve_sketch` in `solver.rs`:
/// each constraint produces scalar residual equations r_i(x) where r_i = 0
/// when satisfied. We build the Jacobian J and solve:
///   (J^T J + lambda * I) * dx = -J^T * r
pub fn solve_assembly(
    assembly: &mut Assembly,
    config: &SolverConfig,
) -> Result<SolverResult, SolverError> {
    let mut params = assembly.params.clone();
    let n = params.len();

    if n == 0 || assembly.constraints.is_empty() {
        return Ok(SolverResult {
            converged: true,
            iterations: 0,
            final_residual: 0.0,
            dof: n,
            warnings: if n > 0 {
                vec![SolverWarning::UnderConstrained {
                    dof: n,
                    free_params: (0..n).collect(),
                }]
            } else {
                vec![]
            },
            params: params.clone(),
        });
    }

    let mut lambda = config.lambda_initial;

    for iteration in 0..config.max_iterations {
        let residuals = collect_residuals(assembly, &params);
        let m = residuals.len();

        let total_sq: f64 = residuals.iter().map(|r| r * r).sum();
        if total_sq < config.tolerance {
            assembly.params = params.clone();
            let (dof, warnings) = analyze_constraints(assembly, &assembly.params);
            return Ok(SolverResult {
                converged: true,
                iterations: iteration,
                final_residual: total_sq,
                dof,
                warnings,
                params: assembly.params.clone(),
            });
        }

        let jacobian = build_jacobian(assembly, &params, m, n);

        // J^T * r
        let mut jtr = vec![0.0; n];
        for j in 0..n {
            for i in 0..m {
                jtr[j] += jacobian[i * n + j] * residuals[i];
            }
        }

        // J^T * J
        let mut jtj = vec![0.0; n * n];
        for ii in 0..n {
            for jj in 0..n {
                let mut sum = 0.0;
                for k in 0..m {
                    sum += jacobian[k * n + ii] * jacobian[k * n + jj];
                }
                jtj[ii * n + jj] = sum;
            }
        }

        // LM step with adaptive damping
        let mut found_better = false;
        for _ in 0..10 {
            let mut damped = jtj.clone();
            for i in 0..n {
                damped[i * n + i] += lambda;
            }

            if let Some(dx) = solve_linear_system(&damped, &jtr, n) {
                let mut new_params = params.clone();
                for i in 0..n {
                    new_params[i] -= dx[i];
                }

                let new_residuals = collect_residuals(assembly, &new_params);
                let new_sq: f64 = new_residuals.iter().map(|r| r * r).sum();

                if new_sq < total_sq {
                    params = new_params;
                    lambda = (lambda / config.lambda_factor).max(1e-15);
                    found_better = true;
                    break;
                }
            }
            lambda *= config.lambda_factor;
        }

        if !found_better {
            let grad_norm_sq: f64 = jtr.iter().map(|g| g * g).sum();
            if grad_norm_sq > 1e-20 {
                let step = 0.01 / grad_norm_sq.sqrt();
                for i in 0..n {
                    params[i] -= step * jtr[i];
                }
            }
            lambda *= config.lambda_factor;
        }
    }

    let residuals = collect_residuals(assembly, &params);
    let final_residual: f64 = residuals.iter().map(|r| r * r).sum();
    assembly.params = params.clone();

    if final_residual < config.tolerance {
        let (dof, warnings) = analyze_constraints(assembly, &assembly.params);
        Ok(SolverResult {
            converged: true,
            iterations: config.max_iterations,
            final_residual,
            dof,
            warnings,
            params,
        })
    } else {
        Err(SolverError::DidNotConverge {
            max_iterations: config.max_iterations,
            residual: final_residual,
        })
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    fn face(point: [f64; 3], normal: [f64; 3]) -> LocalFace {
        LocalFace { point, normal }
    }

    fn axis(point: [f64; 3], direction: [f64; 3]) -> LocalAxis {
        LocalAxis { point, direction }
    }

    #[test]
    fn test_rotation_matrix_identity() {
        let r = rotation_matrix(&Vector3::zeros());
        let id: Matrix3<f64> = Matrix3::identity();
        for i in 0..3 {
            for j in 0..3 {
                assert!((r[(i, j)] - id[(i, j)]).abs() < 1e-12);
            }
        }
    }

    #[test]
    fn test_rotation_matrix_90_deg_z() {
        // 90 degrees about z-axis: (0, 0, pi/2)
        let v = Vector3::new(0.0, 0.0, PI / 2.0);
        let r = rotation_matrix(&v);
        // Should map x -> y, y -> -x
        let x = r * Vector3::new(1.0, 0.0, 0.0);
        assert!((x.x).abs() < 1e-10);
        assert!((x.y - 1.0).abs() < 1e-10);
        assert!((x.z).abs() < 1e-10);
    }

    #[test]
    fn test_fixed_position_converges_immediately() {
        // Body already at target position
        let mut asm = Assembly::new();
        asm.add_body("A", [1.0, 2.0, 3.0], [0.0, 0.0, 0.0]);
        asm.add_constraint(AssemblyConstraint::FixedPosition {
            body: 0,
            position: [1.0, 2.0, 3.0],
            orientation: [0.0, 0.0, 0.0],
        });

        let config = SolverConfig::default();
        let result = solve_assembly(&mut asm, &config).unwrap();
        assert!(result.converged);
        assert_eq!(result.iterations, 0);
        assert!(result.final_residual < 1e-10);
    }

    #[test]
    fn test_fixed_position_moves_body() {
        let mut asm = Assembly::new();
        asm.add_body("A", [0.0, 0.0, 0.0], [0.0, 0.0, 0.0]);
        asm.add_constraint(AssemblyConstraint::FixedPosition {
            body: 0,
            position: [5.0, 10.0, 15.0],
            orientation: [0.0, 0.0, 0.0],
        });

        let config = SolverConfig {
            max_iterations: 200,
            ..SolverConfig::default()
        };
        let result = solve_assembly(&mut asm, &config).unwrap();
        assert!(result.converged, "residual: {}", result.final_residual);
        let pos = asm.body_position(0);
        assert!((pos[0] - 5.0).abs() < 1e-4, "x = {}", pos[0]);
        assert!((pos[1] - 10.0).abs() < 1e-4, "y = {}", pos[1]);
        assert!((pos[2] - 15.0).abs() < 1e-4, "z = {}", pos[2]);
    }

    #[test]
    fn test_unconstrained_dof() {
        let mut asm = Assembly::new();
        asm.add_body("A", [0.0, 0.0, 0.0], [0.0, 0.0, 0.0]);
        asm.add_body("B", [1.0, 0.0, 0.0], [0.0, 0.0, 0.0]);
        assert_eq!(assembly_dof(&asm), 12); // 2 bodies * 6 DOF
    }

    #[test]
    fn test_fully_constrained_two_bodies_dof_zero() {
        let mut asm = Assembly::new();
        asm.add_body("A", [0.0, 0.0, 0.0], [0.0, 0.0, 0.0]);
        asm.add_body("B", [1.0, 0.0, 0.0], [0.0, 0.0, 0.0]);

        // Fix both bodies completely -> 12 residuals, 12 params -> DOF = 0
        asm.add_constraint(AssemblyConstraint::FixedPosition {
            body: 0,
            position: [0.0, 0.0, 0.0],
            orientation: [0.0, 0.0, 0.0],
        });
        asm.add_constraint(AssemblyConstraint::FixedPosition {
            body: 1,
            position: [1.0, 0.0, 0.0],
            orientation: [0.0, 0.0, 0.0],
        });

        let dof = assembly_dof(&asm);
        assert_eq!(dof, 0);
    }

    #[test]
    fn test_mate_b_on_top_of_a() {
        // Body A is fixed at origin. Body B should be mated on top of A.
        // A's top face: point (0,0,1), normal (0,0,1) in local coords (unit cube centered at origin).
        // B's bottom face: point (0,0,-1), normal (0,0,-1) in local coords.
        // After mating: B's bottom face touches A's top face, normals are anti-parallel.
        let mut asm = Assembly::new();
        asm.add_body("A", [0.0, 0.0, 0.0], [0.0, 0.0, 0.0]);
        asm.add_body("B", [0.0, 0.0, 5.0], [0.0, 0.0, 0.0]); // start B above

        // Fix A
        asm.add_constraint(AssemblyConstraint::FixedPosition {
            body: 0,
            position: [0.0, 0.0, 0.0],
            orientation: [0.0, 0.0, 0.0],
        });

        // Mate: A's top face with B's bottom face
        asm.add_constraint(AssemblyConstraint::Mate {
            body_a: 0,
            face_a: face([0.0, 0.0, 1.0], [0.0, 0.0, 1.0]),
            body_b: 1,
            face_b: face([0.0, 0.0, -1.0], [0.0, 0.0, -1.0]),
        });

        let config = SolverConfig {
            max_iterations: 200,
            ..SolverConfig::default()
        };
        let result = solve_assembly(&mut asm, &config).unwrap();
        assert!(result.converged, "Mate should converge, residual: {}", result.final_residual);

        // B's z should position such that B's bottom face at z_b + (-1) = A's top face at 0 + 1
        // => z_b = 2
        let pos_b = asm.body_position(1);
        assert!(
            (pos_b[2] - 2.0).abs() < 0.1,
            "B should sit on top of A: expected z=2, got z={}",
            pos_b[2]
        );
    }

    #[test]
    fn test_axis_align_bodies() {
        // Fix A at origin with identity orientation.
        // A has axis along z: point (0,0,0), dir (0,0,1).
        // B starts tilted, should align its axis with A's.
        let mut asm = Assembly::new();
        asm.add_body("A", [0.0, 0.0, 0.0], [0.0, 0.0, 0.0]);
        asm.add_body("B", [0.0, 0.0, 3.0], [0.3, 0.2, 0.0]); // slightly tilted

        asm.add_constraint(AssemblyConstraint::FixedPosition {
            body: 0,
            position: [0.0, 0.0, 0.0],
            orientation: [0.0, 0.0, 0.0],
        });

        asm.add_constraint(AssemblyConstraint::AxisAlign {
            body_a: 0,
            axis_a: axis([0.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
            body_b: 1,
            axis_b: axis([0.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
        });

        let config = SolverConfig {
            max_iterations: 200,
            ..SolverConfig::default()
        };
        let result = solve_assembly(&mut asm, &config).unwrap();
        assert!(result.converged, "AxisAlign should converge, residual: {}", result.final_residual);

        // B's z-axis should now align with world z-axis
        let orient_b = asm.body_orientation(1);
        let rot_v = Vector3::new(orient_b[0], orient_b[1], orient_b[2]);
        let r = rotation_matrix(&rot_v);
        let z_axis = r * Vector3::new(0.0, 0.0, 1.0);

        // z_axis should be parallel to (0,0,1): cross product should be near zero
        let cross = z_axis.cross(&Vector3::new(0.0, 0.0, 1.0));
        assert!(
            cross.norm() < 0.1,
            "Axes should be aligned, cross product norm: {}",
            cross.norm()
        );
    }

    #[test]
    fn test_align_constraint() {
        let mut asm = Assembly::new();
        asm.add_body("A", [0.0, 0.0, 0.0], [0.0, 0.0, 0.0]);
        asm.add_body("B", [0.0, 0.0, 5.0], [0.0, 0.0, 0.0]);

        asm.add_constraint(AssemblyConstraint::FixedPosition {
            body: 0,
            position: [0.0, 0.0, 0.0],
            orientation: [0.0, 0.0, 0.0],
        });

        // Align: A's top face normal (0,0,1) should match B's top face normal (0,0,1)
        // and the faces should be coplanar.
        asm.add_constraint(AssemblyConstraint::Align {
            body_a: 0,
            face_a: face([0.0, 0.0, 1.0], [0.0, 0.0, 1.0]),
            body_b: 1,
            face_b: face([0.0, 0.0, 1.0], [0.0, 0.0, 1.0]),
        });

        let config = SolverConfig {
            max_iterations: 200,
            ..SolverConfig::default()
        };
        let result = solve_assembly(&mut asm, &config).unwrap();
        assert!(result.converged);

        // B's face at z_b + 1 should equal A's face at 0 + 1 = 1
        // => z_b = 0
        let pos_b = asm.body_position(1);
        assert!(
            pos_b[2].abs() < 0.1,
            "Aligned faces should be coplanar: expected z=0, got z={}",
            pos_b[2]
        );
    }

    #[test]
    fn test_single_body_dof() {
        let mut asm = Assembly::new();
        asm.add_body("A", [0.0, 0.0, 0.0], [0.0, 0.0, 0.0]);
        assert_eq!(assembly_dof(&asm), 6);
    }

    #[test]
    fn test_transform_point_with_rotation() {
        // 90 degrees about z: local (1,0,0) -> world (0,1,0) + position
        let params = vec![10.0, 20.0, 30.0, 0.0, 0.0, PI / 2.0];
        let p = transform_point(&params, 0, &[1.0, 0.0, 0.0]);
        assert!((p.x - 10.0).abs() < 1e-10);
        assert!((p.y - 21.0).abs() < 1e-10);
        assert!((p.z - 30.0).abs() < 1e-10);
    }

    #[test]
    fn test_mate_with_rotated_body() {
        // A is fixed, B is rotated 180 degrees about x.
        // A's top face: (0,0,1), normal (0,0,1)
        // B's top face: (0,0,1), normal (0,0,1) in local coords.
        // After 180-degree x rotation, B's local (0,0,1) becomes (0,0,-1) in world,
        // and normal (0,0,1) becomes (0,0,-1) in world -> anti-parallel to A's.
        let mut asm = Assembly::new();
        asm.add_body("A", [0.0, 0.0, 0.0], [0.0, 0.0, 0.0]);
        asm.add_body("B", [0.0, 0.0, 5.0], [PI, 0.0, 0.0]); // 180 deg about x

        asm.add_constraint(AssemblyConstraint::FixedPosition {
            body: 0,
            position: [0.0, 0.0, 0.0],
            orientation: [0.0, 0.0, 0.0],
        });

        // Mate A's top with B's top (B is flipped, so B's top normal points down in world)
        asm.add_constraint(AssemblyConstraint::Mate {
            body_a: 0,
            face_a: face([0.0, 0.0, 1.0], [0.0, 0.0, 1.0]),
            body_b: 1,
            face_b: face([0.0, 0.0, 1.0], [0.0, 0.0, 1.0]), // becomes (0,0,-1) in world due to rotation
        });

        let config = SolverConfig {
            max_iterations: 200,
            ..SolverConfig::default()
        };
        let result = solve_assembly(&mut asm, &config).unwrap();
        assert!(result.converged, "Rotated mate should converge");

        // B's world top face point = R_b * (0,0,1) + pos_b.
        // With 180-deg x rotation: (0,0,1) -> (0,0,-1), so world point = (0, 0, z_b - 1).
        // A's top face world point = (0, 0, 1).
        // Coplanarity: (z_b - 1) should equal 1 along the normal direction.
        // Actually the residual is (pt_b - pt_a) . n_a = 0, so z_b - 1 - 1 = 0 => z_b = 2.
        let pos_b = asm.body_position(1);
        assert!(
            (pos_b[2] - 2.0).abs() < 0.1,
            "Rotated B should mate at z=2, got z={}",
            pos_b[2]
        );
    }

    #[test]
    fn test_distance_mate() {
        // A fixed at origin. B should be positioned so its bottom face is 3.0 units
        // above A's top face (measured along the normal direction).
        let mut asm = Assembly::new();
        asm.add_body("A", [0.0, 0.0, 0.0], [0.0, 0.0, 0.0]);
        asm.add_body("B", [0.0, 0.0, 10.0], [0.0, 0.0, 0.0]); // start far above

        asm.add_constraint(AssemblyConstraint::FixedPosition {
            body: 0,
            position: [0.0, 0.0, 0.0],
            orientation: [0.0, 0.0, 0.0],
        });

        // DistanceMate: A's top face at z=1 with normal (0,0,1),
        // B's bottom face at z=-1 with normal (0,0,-1),
        // distance = 3.0 along A's normal.
        // (pt_b - pt_a) . n_a = distance => (z_b - 1) - 1 = 3 => z_b = 5
        // Wait: pt_b in world = R_b * (0,0,-1) + pos_b = (0, 0, z_b - 1)
        // pt_a in world = (0, 0, 1)
        // (pt_b - pt_a) . n_a = (z_b - 1 - 1) = z_b - 2 = 3 => z_b = 5
        asm.add_constraint(AssemblyConstraint::DistanceMate {
            body_a: 0,
            face_a: face([0.0, 0.0, 1.0], [0.0, 0.0, 1.0]),
            body_b: 1,
            face_b: face([0.0, 0.0, -1.0], [0.0, 0.0, -1.0]),
            distance: 3.0,
        });

        let config = SolverConfig {
            max_iterations: 200,
            ..SolverConfig::default()
        };
        let result = solve_assembly(&mut asm, &config).unwrap();
        assert!(result.converged, "DistanceMate should converge, residual: {}", result.final_residual);

        let pos_b = asm.body_position(1);
        assert!(
            (pos_b[2] - 5.0).abs() < 0.1,
            "B should be at z=5 for 3-unit gap: got z={}",
            pos_b[2]
        );
    }
}
