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
    _outcome: &SolveOutcome,
    _rank: &RankAnalysis,
    _eq_to_constraint: &[usize],
    _num_params: usize,
) -> SolveStatus {
    todo!("Fork B: status classification")
}
