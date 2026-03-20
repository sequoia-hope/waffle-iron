//! QR/SVD rank analysis for DOF counting and conflict detection.
//!
//! After solving, analyzes the scaled Jacobian to determine:
//! - Rank (number of independent constraints)
//! - DOF (degrees of freedom = num_params - rank)
//! - Conflicting constraint rows (if over-constrained)
//!
//! Includes restricted Hessian analysis to correct phantom DOF at
//! cardinal positions (LICQ failures).

use super::constraint::ConstraintEq;
use nalgebra::{DMatrix, DVector};

/// Result of rank analysis on the scaled Jacobian.
pub struct RankAnalysis {
    /// Number of independent constraint equations.
    pub rank: usize,
    /// Degrees of freedom remaining (num_params - rank).
    pub dof: usize,
    /// Indices of conflicting equation rows (empty if not over-constrained).
    pub conflicting_rows: Vec<usize>,
    /// Right null space columns (directions SVD thinks are free). None if dof == 0.
    pub v_null: Option<DMatrix<f64>>,
}

/// Analyze the rank of the scaled Jacobian using SVD.
///
/// - `jacobian_scaled`: the D_row-scaled, un-augmented Jacobian
/// - `residual_scaled`: the scaled residual vector
/// - `tolerance`: singular value threshold (values below this are "zero")
pub fn analyze_rank(
    jacobian_scaled: &DMatrix<f64>,
    residual_scaled: &DVector<f64>,
    tolerance: f64,
) -> RankAnalysis {
    let (nrows, ncols) = jacobian_scaled.shape();

    if nrows == 0 || ncols == 0 {
        return RankAnalysis {
            rank: 0,
            dof: ncols,
            conflicting_rows: Vec::new(),
            v_null: None,
        };
    }

    // Compute SVD: J = U * S * V^T
    // We need U for conflict detection and V for null space (Hessian refinement).
    // When nrows > ncols, pad to square to get full left null space.
    let svd = if nrows > ncols {
        let mut j_square = DMatrix::zeros(nrows, nrows);
        j_square
            .view_mut((0, 0), (nrows, ncols))
            .copy_from(jacobian_scaled);
        j_square.svd(true, true)
    } else {
        jacobian_scaled.clone().svd(true, true)
    };

    let s = &svd.singular_values;
    let u = svd.u.as_ref().unwrap();
    let v_t = svd.v_t.as_ref().unwrap();

    let sigma_max = s[0];
    let eps = (nrows.max(ncols) as f64) * f64::EPSILON * sigma_max;

    let mut rank = 0;
    for &sigma in s.iter() {
        if sigma > eps {
            rank += 1;
        } else {
            break;
        }
    }

    let dof = ncols.saturating_sub(rank);

    let mut conflicting_rows = std::collections::BTreeSet::new();

    // Conflicting row detection: for each left singular vector u_k where sigma_k <= eps.
    for k in rank..nrows {
        let u_k = u.column(k);
        let r_k = u_k.dot(residual_scaled);

        if r_k.abs() > tolerance {
            for j in 0..nrows {
                if u_k[j].abs() > 0.01 {
                    conflicting_rows.insert(j);
                }
            }
        }
    }

    // Extract right null space: columns of V where σ <= eps.
    // V^T rows correspond to V columns, so V_null columns = V^T rows for k >= rank.
    let v_null = if dof > 0 {
        // V^T has shape (ncols_or_nrows × ncols_or_nrows) due to padding.
        // We need columns rank..ncols of the original V, which are rows rank..ncols of V^T.
        let v_t_rows = v_t.nrows().min(ncols);
        let null_cols = v_t_rows.saturating_sub(rank);
        if null_cols > 0 {
            let mut v_null_mat = DMatrix::zeros(ncols, null_cols);
            for (j, k) in (rank..v_t_rows).enumerate() {
                // V^T row k → V column k, but we only want first ncols entries
                for i in 0..ncols {
                    v_null_mat[(i, j)] = v_t[(k, i)];
                }
            }
            Some(v_null_mat)
        } else {
            None
        }
    } else {
        None
    };

    RankAnalysis {
        rank,
        dof,
        conflicting_rows: conflicting_rows.into_iter().collect(),
        v_null,
    }
}

/// Refine DOF count using restricted Hessian analysis.
///
/// At cardinal positions, first-order linearization can be rank-deficient
/// (LICQ failure) even when constraints are sufficient. This projects
/// per-constraint Hessians onto the SVD null space to detect directions
/// that are quadratically bound (phantom DOF).
///
/// Algorithm: for each equation with a non-trivial Hessian, project it onto
/// the null space to get a k×k matrix Z_i = V^T H_i V. Stack all Z_i
/// (flattened as rows) into a matrix Q. rank(Q) gives the number of
/// independently constrained directions in the null space — these are
/// phantom DOF that should be subtracted from the SVD count.
///
/// This avoids the cancellation problem of summing Hessians: even if
/// two constraints have opposite curvature, they both independently
/// constrain the same direction.
///
/// Returns the refined DOF count (≤ rank.dof).
pub fn refine_dof_with_hessian<C: ConstraintEq>(
    rank: &RankAnalysis,
    constraints: &[C],
    params: &[f64],
    d_row: &DVector<f64>,
) -> usize {
    let v_null = match &rank.v_null {
        Some(v) => v,
        None => return rank.dof, // dof == 0, nothing to refine
    };

    let k = v_null.ncols(); // dimension of null space
    if k == 0 {
        return rank.dof;
    }

    let n = params.len();

    // Step 1: Collect per-equation Hessian entries grouped by equation row
    let mut eq_hessians: std::collections::HashMap<usize, Vec<(usize, usize, f64)>> =
        std::collections::HashMap::new();
    let m = d_row.len();

    let mut eq_offset = 0;
    for c in constraints {
        let num_eq = c.num_equations();
        if num_eq == 0 {
            continue;
        }
        let h = c.hessian(params, eq_offset);
        for (eq, ci, cj, val) in h {
            if eq < m && ci < n && cj < n {
                eq_hessians.entry(eq).or_default().push((ci, cj, val));
            }
        }
        eq_offset += num_eq;
    }

    if eq_hessians.is_empty() {
        return rank.dof;
    }

    // Step 2: For each null direction, check if ANY constraint equation has
    // nonzero second-order curvature along that direction. If so, the direction
    // is quadratically bound (phantom DOF, not truly free).
    //
    // For direction v_a, compute v_a^T · H_eq · v_a for each equation.
    // If any equation has |v_a^T H_eq v_a| > threshold, direction a is phantom.
    let mut phantom_count = 0;
    let threshold = 1e-8;
    for a in 0..k {
        // Check if direction a is bound by any constraint
        let mut bound = false;
        for entries in eq_hessians.values() {
            // Compute v_a^T H v_a for this equation
            let mut quadratic = 0.0;
            for &(ci, cj, val) in entries {
                quadratic += val * v_null[(ci, a)] * v_null[(cj, a)];
            }
            if quadratic.abs() > threshold {
                bound = true;
                break;
            }
        }
        if bound {
            phantom_count += 1;
        }
    }

    rank.dof.saturating_sub(phantom_count)
}
