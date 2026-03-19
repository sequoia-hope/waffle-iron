//! Status classification: maps solver outcome + rank analysis → SolveStatus.

use crate::types::SolveStatus;

use super::rank::RankAnalysis;
use super::types::SolveOutcome;

/// Classify the solve result into a user-facing SolveStatus.
///
/// Uses convergence from `SolveOutcome` and rank from `RankAnalysis`:
/// - Converged + rank == num_params → FullyConstrained
/// - Converged + rank < num_params → UnderConstrained { dof }
/// - Not converged + conflicting rows → OverConstrained { conflicts }
/// - Not converged otherwise → SolveFailed
///
/// `eq_to_constraint`: maps equation row → parent constraint index
///   (for reporting which constraints conflict, not which equation rows).
pub fn classify_solve(
    outcome: &SolveOutcome,
    rank: &RankAnalysis,
    eq_to_constraint: &[usize],
    _num_params: usize,
) -> SolveStatus {
    // Conflicts override convergence — an over-constrained contradictory system
    // may report converged=true (stuck at augmented equilibrium) but rank analysis
    // detects the contradiction.
    if !rank.conflicting_rows.is_empty() {
        let mut conflict_ids: Vec<u32> = rank
            .conflicting_rows
            .iter()
            .filter_map(|&row| eq_to_constraint.get(row).map(|&id| id as u32))
            .collect();
        conflict_ids.sort_unstable();
        conflict_ids.dedup();
        SolveStatus::OverConstrained {
            conflicts: conflict_ids,
        }
    } else if outcome.converged {
        if rank.dof == 0 {
            SolveStatus::FullyConstrained
        } else {
            SolveStatus::UnderConstrained {
                dof: rank.dof as u32,
            }
        }
    } else {
        SolveStatus::SolveFailed {
            reason: format!(
                "failed to converge after {} iterations (residual norm: {:.2e})",
                outcome.iterations, outcome.final_residual_norm
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nalgebra::{DMatrix, DVector};

    fn make_outcome(converged: bool, iterations: usize, norm: f64) -> SolveOutcome {
        SolveOutcome {
            params: vec![],
            converged,
            iterations,
            final_residual_norm: norm,
            jacobian_scaled: DMatrix::zeros(0, 0),
            residual_scaled: DVector::zeros(0),
        }
    }

    #[test]
    fn fully_constrained() {
        let outcome = make_outcome(true, 5, 1e-10);
        let rank = RankAnalysis { rank: 4, dof: 0, conflicting_rows: vec![] };
        let result = classify_solve(&outcome, &rank, &[], 4);
        assert!(matches!(result, SolveStatus::FullyConstrained));
    }

    #[test]
    fn under_constrained() {
        let outcome = make_outcome(true, 3, 1e-10);
        let rank = RankAnalysis { rank: 2, dof: 2, conflicting_rows: vec![] };
        let result = classify_solve(&outcome, &rank, &[], 4);
        assert!(matches!(result, SolveStatus::UnderConstrained { dof: 2 }));
    }

    #[test]
    fn over_constrained_overrides_convergence() {
        // Conflicts override convergence — even if converged=true, conflicts win
        let outcome = make_outcome(true, 10, 1e-10);
        let rank = RankAnalysis { rank: 3, dof: 0, conflicting_rows: vec![0, 2] };
        // eq_to_constraint: eq0 → constraint 0, eq1 → constraint 0, eq2 → constraint 1
        let eq_map = vec![0, 0, 1];
        let result = classify_solve(&outcome, &rank, &eq_map, 3);
        if let SolveStatus::OverConstrained { conflicts } = result {
            assert_eq!(conflicts, vec![0, 1]);
        } else {
            panic!("expected OverConstrained, got {:?}", result);
        }
    }

    #[test]
    fn solve_failed() {
        let outcome = make_outcome(false, 50, 0.5);
        let rank = RankAnalysis { rank: 2, dof: 0, conflicting_rows: vec![] };
        let result = classify_solve(&outcome, &rank, &[], 2);
        assert!(matches!(result, SolveStatus::SolveFailed { .. }));
    }
}
