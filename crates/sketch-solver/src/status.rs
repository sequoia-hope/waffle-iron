use slvs::system::{FailReason, SolveResult};

use crate::types::SolveStatus;

/// Classify a slvs SolveResult into our SolveStatus.
pub fn classify_status(result: SolveResult) -> SolveStatus {
    match result {
        SolveResult::Ok { dof: 0 } => SolveStatus::FullyConstrained,
        SolveResult::Ok { dof } => SolveStatus::UnderConstrained { dof: dof as u32 },
        SolveResult::Fail {
            reason: FailReason::Inconsistent,
            ..
        } => SolveStatus::OverConstrained {
            conflicts: Vec::new(),
        },
        SolveResult::Fail { reason, .. } => SolveStatus::SolveFailed {
            reason: format!("{:?}", reason),
        },
    }
}
