//! QR/SVD rank analysis for DOF counting and conflict detection.
//!
//! After solving, analyzes the scaled Jacobian to determine:
//! - Rank (number of independent constraints)
//! - DOF (degrees of freedom = num_params - rank)
//! - Conflicting constraint rows (if over-constrained)

use nalgebra::{DMatrix, DVector};

/// Result of rank analysis on the scaled Jacobian.
pub struct RankAnalysis {
    /// Number of independent constraint equations.
    pub rank: usize,
    /// Degrees of freedom remaining (num_params - rank).
    pub dof: usize,
    /// Indices of conflicting equation rows (empty if not over-constrained).
    pub conflicting_rows: Vec<usize>,
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
        };
    }

    // Compute SVD: J = U * S * V^T
    // Note: nalgebra's DMatrix::svd() computes a thin SVD. For conflict detection,
    // we need the full left null space (U columns for k >= rank) when nrows > ncols.
    // We force a full U by padding J to be square if necessary.
    let svd = if nrows > ncols {
        let mut j_square = DMatrix::zeros(nrows, nrows);
        j_square
            .view_mut((0, 0), (nrows, ncols))
            .copy_from(jacobian_scaled);
        j_square.svd(true, false)
    } else {
        jacobian_scaled.clone().svd(true, false)
    };

    let s = &svd.singular_values;
    let u = svd.u.as_ref().unwrap();

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
    // These vectors form a basis for the left null space (for k >= rank).
    // If the residual has a non-zero projection onto this space, the system is inconsistent.
    for k in rank..nrows {
        let u_k = u.column(k);
        let r_k = u_k.dot(residual_scaled);

        if r_k.abs() > tolerance {
            // These rows are conflicting. Involved rows have significant entries in u_k.
            for j in 0..nrows {
                if u_k[j].abs() > 0.01 {
                    conflicting_rows.insert(j);
                }
            }
        }
    }

    RankAnalysis {
        rank,
        dof,
        conflicting_rows: conflicting_rows.into_iter().collect(),
    }
}
