//! Levenberg-Marquardt solver with weak spring augmentation.
//!
//! This is the primary solver (not a fallback). Per research R2/R4,
//! LM with Marquardt damping + weak springs provides both velocity-space
//! and position-space regularization for under-constrained systems.

use super::constraint::ConstraintEq;
use super::types::{ScaleType, SolveOptions, SolveOutcome};
use nalgebra::{DMatrix, DVector};

/// Solve a system of geometric constraints using augmented Levenberg-Marquardt.
///
/// - `x0`: starting guess (warm start from current entity positions)
/// - `x_anchor`: spring anchor (pre-edit state for drag, or same as x0)
/// - `constraints`: pre-built internal constraints from the builder
/// - `eq_scale_types`: Distance or Angle classification per equation row
/// - `num_equations`: total number of scalar equations
/// - `options`: solver tuning parameters
pub fn lm_solve<C: ConstraintEq>(
    x0: &[f64],
    x_anchor: &[f64],
    constraints: &[C],
    eq_scale_types: &[ScaleType],
    num_equations: usize,
    options: &SolveOptions,
) -> SolveOutcome {
    let n = x0.len();
    let m = num_equations;

    // Phase 1: Static scaling (compute once from x_anchor)
    let l_c = bbox_diagonal(x_anchor);
    let d_row = build_d_row(eq_scale_types, l_c);
    let mu = options.spring_mu;
    let sqrt_mu = mu.sqrt();

    // Phase 2: LM loop
    let mut x = DVector::from_column_slice(x0);
    let x_anchor_vec = DVector::from_column_slice(x_anchor);
    let mut lambda = options.lambda_init;
    let mut nu = 2.0;

    let mut converged = false;
    let mut iterations = 0;
    let mut final_residual_norm = 0.0;

    // Stagnation detection: track residual plateau
    let mut prev_f_inf_norm = f64::INFINITY;
    let mut stagnation_count = 0_usize;
    const STAGNATION_LIMIT: usize = 5;
    const STAGNATION_RATIO: f64 = 0.999;

    // Storage for final diagnostics
    let mut final_j_scaled = DMatrix::zeros(m, n);
    let mut final_r_scaled = DVector::zeros(m);

    for iter in 0..options.max_iterations {
        iterations = iter + 1;

        // a) Build raw residual F(x) and Jacobian J(x)
        let mut f = DVector::zeros(m);
        let mut triplets = Vec::new();
        let mut eq_offset = 0;
        for c in constraints {
            let num_eq = c.num_equations();
            if num_eq == 0 {
                continue;
            }
            let mut res = vec![0.0; num_eq];
            c.residuals(x.as_slice(), &mut res);
            for (i, &v) in res.iter().enumerate() {
                f[eq_offset + i] = v;
            }
            c.jacobian(x.as_slice(), eq_offset, &mut triplets);
            eq_offset += num_eq;
        }

        let mut j = DMatrix::zeros(m, n);
        for (row, col, val) in triplets {
            j[(row, col)] = val;
        }

        // b) Apply row scaling
        let f_s = f.component_mul(&d_row);
        let mut j_s = j.clone();
        for r in 0..m {
            let row_scale = d_row[r];
            j_s.row_mut(r).apply(|v| *v *= row_scale);
        }

        // c) Check convergence on scaled residuals
        let f_inf_norm = f_s.amax();
        if f_inf_norm < options.tolerance {
            converged = true;
            final_residual_norm = f_inf_norm;
            final_j_scaled = j_s;
            final_r_scaled = f_s;
            break;
        }

        // d) Augment with weak springs
        // J_aug = [J_s; sqrt(mu)*I_n]
        // F_aug = [F_s; sqrt(mu)*(x - x_anchor)]
        let mut j_aug = DMatrix::zeros(m + n, n);
        j_aug.rows_mut(0, m).copy_from(&j_s);
        for i in 0..n {
            j_aug[(m + i, i)] = sqrt_mu;
        }

        let mut f_aug = DVector::zeros(m + n);
        f_aug.rows_mut(0, m).copy_from(&f_s);
        for i in 0..n {
            f_aug[m + i] = sqrt_mu * (x[i] - x_anchor_vec[i]);
        }

        // e) Normal equations
        let h = j_aug.transpose() * &j_aug;
        let g = j_aug.transpose() * &f_aug;
        let grad_inf_norm = g.amax();

        // f) Marquardt damping: H_damped = H + lambda * diag(H)
        let mut h_damped = h.clone();
        for i in 0..n {
            h_damped[(i, i)] += lambda * h[(i, i)];
        }

        // g) Solve H_damped * delta = -g via Cholesky
        let delta = match h_damped.clone().cholesky() {
            Some(chol) => chol.solve(&-g),
            None => {
                // Fallback to QR
                h_damped
                    .qr()
                    .solve(&-g)
                    .unwrap_or_else(|| DVector::zeros(n))
            }
        };

        // h) Trial: x_new = x + delta
        let x_new = &x + &delta;

        // Evaluate F_new, scale, augment → F_aug_new
        let mut f_new = DVector::zeros(m);
        let mut eq_offset = 0;
        for c in constraints {
            let num_eq = c.num_equations();
            if num_eq == 0 {
                continue;
            }
            let mut res = vec![0.0; num_eq];
            c.residuals(x_new.as_slice(), &mut res);
            for (i, &v) in res.iter().enumerate() {
                f_new[eq_offset + i] = v;
            }
            eq_offset += num_eq;
        }
        let f_new_s = f_new.component_mul(&d_row);
        let mut f_aug_new = DVector::zeros(m + n);
        f_aug_new.rows_mut(0, m).copy_from(&f_new_s);
        for i in 0..n {
            f_aug_new[m + i] = sqrt_mu * (x_new[i] - x_anchor_vec[i]);
        }

        // i) Gain ratio: rho = (||F_aug||^2 - ||F_aug_new||^2) / (||F_aug||^2 - ||F_aug + J_aug*delta||^2)
        let norm_f_aug_sq = f_aug.norm_squared();
        let norm_f_aug_new_sq = f_aug_new.norm_squared();
        let denom = norm_f_aug_sq - (&f_aug + &j_aug * &delta).norm_squared();

        let rho = if denom.abs() > 1e-18 {
            (norm_f_aug_sq - norm_f_aug_new_sq) / denom
        } else {
            0.0
        };

        // j) Update
        if rho > 0.0 {
            x = x_new;
            let factor = 1.0 - (2.0 * rho - 1.0).powi(3);
            lambda *= factor.max(1.0 / 3.0);
            nu = 2.0;
        } else {
            lambda *= nu;
            nu *= 2.0;
        }

        // k) Stuck check — params not moving → at a fixed point of the augmented system.
        // Check the un-augmented constraint residual to decide convergence:
        // if constraints are satisfied (f_inf_norm small), mark converged.
        // If not (e.g. coincident points 5e-6 apart), don't claim convergence.
        // k) Stuck check — params not moving → at a fixed point of the augmented system.
        // Mark as converged. Under-constrained systems with springs settle to an
        // equilibrium where the constraint residual may not be exactly zero (spring
        // force balances constraint force). This is correct solver behavior.
        // Over-constrained contradictions are caught by rank analysis (conflicting_rows)
        // which overrides the convergence flag in classify_solve.
        // k) Fixed-point check: gradient of augmented cost → 0.
        // Unlike delta (which shrinks with large lambda), the gradient is
        // damping-independent and reliably detects genuine fixed points —
        // both satisfied solutions and contradictory equilibria.
        if grad_inf_norm < options.tolerance {
            converged = true;
            final_residual_norm = f_inf_norm;
            final_j_scaled = j_s;
            final_r_scaled = f_s;
            break;
        }

        // l) Stagnation detection: residual not improving over several iterations.
        // At a genuine fixed point (e.g., LICQ failure, contradiction equilibrium),
        // the solver stalls with constant residual. Mark as converged and let
        // rank analysis determine the status (UnderConstrained vs OverConstrained).
        if f_inf_norm >= prev_f_inf_norm * STAGNATION_RATIO {
            stagnation_count += 1;
            if stagnation_count >= STAGNATION_LIMIT {
                converged = true;
                final_residual_norm = f_inf_norm;
                final_j_scaled = j_s;
                final_r_scaled = f_s;
                break;
            }
        } else {
            stagnation_count = 0;
        }
        prev_f_inf_norm = f_inf_norm;

        // Final values for this iteration (in case we stop next)
        final_residual_norm = f_inf_norm;
        final_j_scaled = j_s;
        final_r_scaled = f_s;
    }

    SolveOutcome {
        params: x.as_slice().to_vec(),
        converged,
        iterations,
        final_residual_norm,
        jacobian_scaled: final_j_scaled,
        residual_scaled: final_r_scaled,
        d_row,
    }
}

/// Computes bounding box diagonal of point coordinates, clamped to 1.0.
/// Treats params as pairs of (x,y).
fn bbox_diagonal(params: &[f64]) -> f64 {
    if params.is_empty() {
        return 1.0;
    }
    let mut x_min = f64::INFINITY;
    let mut x_max = f64::NEG_INFINITY;
    let mut y_min = f64::INFINITY;
    let mut y_max = f64::NEG_INFINITY;

    for i in (0..params.len()).step_by(2) {
        if i + 1 < params.len() {
            let x = params[i];
            let y = params[i + 1];
            if x < x_min {
                x_min = x;
            }
            if x > x_max {
                x_max = x;
            }
            if y < y_min {
                y_min = y;
            }
            if y > y_max {
                y_max = y;
            }
        }
    }

    if x_min == f64::INFINITY {
        return 1.0;
    }

    let dx = x_max - x_min;
    let dy = y_max - y_min;
    (dx * dx + dy * dy).sqrt().max(1.0)
}

/// Builds diagonal scaling vector D_row.
fn build_d_row(eq_scale_types: &[ScaleType], l_c: f64) -> DVector<f64> {
    let mut d = DVector::zeros(eq_scale_types.len());
    for (i, &st) in eq_scale_types.iter().enumerate() {
        d[i] = match st {
            ScaleType::Distance => 1.0,
            ScaleType::Angle => l_c,
        };
    }
    d
}
