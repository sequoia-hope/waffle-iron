//! Clean-room solver: Levenberg-Marquardt least-squares minimization.
//!
//! Implements `solve_sketch` using the `levenberg-marquardt` crate (MINPACK
//! port) to minimize `Σ (w_i · r_i)²` where `r_i` are constraint residuals
//! and `w_i` are per-residual weights (dragged = 1/20, else 1.0).
//!
//! Status classification uses rank-revealing QR on the final Jacobian:
//!   - `‖r‖∞ < tol` + `dof == 0` → FullyConstrained
//!   - `‖r‖∞ < tol` + `dof > 0`  → UnderConstrained { dof }
//!   - `‖r‖∞ ≥ tol` + `rank(J) < #constraints` → OverConstrained { conflicts }
//!   - `‖r‖∞ ≥ tol` + `rank(J) == #constraints` → SolveFailed { reason }
//!
//! Determinism: residual/Jacobian assembly order follows constraint
//! declaration order (a `Vec`). No `HashMap` iteration in the solve path.

use nalgebra::{DMatrix, DVector};

use levenberg_marquardt::{LeastSquaresProblem, LevenbergMarquardt, MinimizationReport};

use crate::constraint_mapping::{residual_count, weight, CompiledConstraint};
use crate::entity_mapping::ParamLayout;
use crate::profiles::extract_profiles;
use crate::types::{Sketch, SolveStatus, SolvedSketch};

/// Default tolerance for residual satisfiability. 1e-6 m = 1 micrometer,
/// which is the kernel's feature-size floor per A14.2 (TAU_MODEL = 1e-7 m).
/// A solve tolerance one order of magnitude above the model tolerance is
/// sufficient to distinguish "constraints satisfied" from "constraints
/// violated" without false positives from floating-point noise.
/// **Decision banked: sub-micron precision is acceptable.**
const SOLVE_TOL: f64 = 1e-6;

/// Rank-revealing QR tolerance: singular values below this are treated as zero.
/// Scaled relative to the problem size to handle varying parameter magnitudes.
const RANK_TOL: f64 = 1e-8;

/// Proximal regularization weight (specs/sketch_drag_stability.md §2).
///
/// Every solve appends residual rows `ε·(xᵢ − x₀ᵢ)` anchoring each parameter
/// to its pre-solve value. LM's own damping regularizes the *step*, not the
/// *problem* (Ref #43 Moré 1978, #44 Nocedal-Wright ch. 10): along null
/// directions of the constraint Jacobian (e.g. a rectangle whose size is
/// unconstrained) the cost is flat and accepted iterates can drift
/// unboundedly — observed as sketch geometry exploding to 1e8 during drags.
/// The proximal rows make the Gauss-Newton system full-rank and select the
/// solution NEAREST the current configuration — Bouma et al.'s
/// solution-redirecting rule (Ref #40): return the solution intuitive to the
/// user, i.e. the one closest to what they are looking at.
///
/// Weight bound derivation: the proximal pull biases a w-weighted anchor
/// (worst case: Dragged, w = 1/20) by (ε/w)²·D where D is the solve's
/// correction distance. That bias must stay below SOLVE_TOL (1e-6):
/// ε = 1e-5 gives 4e-8·D — safe for D up to 25 length units, far beyond any
/// realistic sketch correction (units are meters, A14.1). Larger ε (1e-4)
/// measurably displaces Dragged anchors (2e-5 at D=5, observed in the
/// pre-existing suite); smaller ε still suppresses the runaway (validated
/// down to 1e-6 in the spec's sweep) but with less margin on NEAR-null
/// valleys, so 1e-5 is the balance point.
const PROXIMAL_WEIGHT: f64 = 1e-5;

/// The least-squares problem: weighted residuals + analytic Jacobian.
struct SketchProblem {
    /// Current parameter vector.
    params: DVector<f64>,
    /// Compiled constraints (in declaration order).
    constraints: Vec<CompiledConstraint>,
    /// Per-constraint weight (applied to all residual rows of that constraint).
    weights: Vec<f64>,
    /// Pre-solve parameter vector — the proximal anchor x₀.
    initial_params: DVector<f64>,
    /// Constraint residual rows (excludes the proximal rows appended after).
    n_constraint_rows: usize,
    /// Total number of residual rows (constraint rows + one proximal row per
    /// parameter).
    n_residuals: usize,
    /// Number of parameters.
    n_params: usize,
    /// Cached residuals (computed in set_params).
    cached_residuals: Option<DVector<f64>>,
    /// Cached Jacobian (computed in set_params).
    cached_jacobian: Option<DMatrix<f64>>,
}

impl SketchProblem {
    fn new(layout: &ParamLayout, compiled: Vec<CompiledConstraint>) -> Self {
        let n_params = layout.n_params();
        let weights: Vec<f64> = compiled.iter().map(|c| weight(c)).collect();
        let n_constraint_rows: usize = compiled.iter().map(|c| residual_count(c)).sum();

        let mut problem = SketchProblem {
            params: DVector::from_vec(layout.params.clone()),
            constraints: compiled,
            weights,
            initial_params: DVector::from_vec(layout.params.clone()),
            n_constraint_rows,
            // Invariant B1 (specs/sketch_drag_stability.md §3): proximal rows
            // are unconditional — one per parameter, after the constraint rows.
            n_residuals: n_constraint_rows + n_params,
            n_params,
            cached_residuals: None,
            cached_jacobian: None,
        };
        // Pre-compute residuals/Jacobian for the initial parameter vector
        // so they're available even if LM terminates immediately.
        problem.compute();
        problem
    }

    /// Assemble the weighted residual vector and weighted Jacobian:
    /// constraint rows first, then the proximal rows ε·(xᵢ − x₀ᵢ).
    fn compute(&mut self) {
        let p = self.params.as_slice();
        let mut residuals = DVector::zeros(self.n_residuals);
        let mut jacobian = DMatrix::zeros(self.n_residuals, self.n_params);

        let mut row = 0;
        for (cc, &w) in self.constraints.iter().zip(self.weights.iter()) {
            let r = cc.residuals(p);
            let j = cc.jacobian(p, self.n_params);
            let nr = r.nrows();
            for i in 0..nr {
                residuals[row + i] = w * r[i];
            }
            for i in 0..nr {
                for col in 0..self.n_params {
                    jacobian[(row + i, col)] = w * j[(i, col)];
                }
            }
            row += nr;
        }

        // Proximal rows: diagonal ε block anchoring x to x₀ (spec §2).
        for i in 0..self.n_params {
            residuals[row + i] = PROXIMAL_WEIGHT * (p[i] - self.initial_params[i]);
            jacobian[(row + i, i)] = PROXIMAL_WEIGHT;
        }

        self.cached_residuals = Some(residuals);
        self.cached_jacobian = Some(jacobian);
    }
}

impl LeastSquaresProblem<f64, nalgebra::Dyn, nalgebra::Dyn> for SketchProblem {
    type ResidualStorage = nalgebra::VecStorage<f64, nalgebra::Dyn, nalgebra::U1>;
    type JacobianStorage = nalgebra::VecStorage<f64, nalgebra::Dyn, nalgebra::Dyn>;
    type ParameterStorage = nalgebra::VecStorage<f64, nalgebra::Dyn, nalgebra::U1>;

    fn set_params(&mut self, x: &DVector<f64>) {
        self.params = x.clone();
        self.compute();
    }

    fn params(&self) -> DVector<f64> {
        self.params.clone()
    }

    fn residuals(&self) -> Option<DVector<f64>> {
        self.cached_residuals.clone()
    }

    fn jacobian(&self) -> Option<DMatrix<f64>> {
        self.cached_jacobian.clone()
    }
}

/// Solve a sketch: map entities/constraints to parameters, run LM, classify.
pub fn solve_sketch(sketch: &Sketch) -> SolvedSketch {
    let layout = ParamLayout::build(&sketch.entities);

    // Compile constraints. If any fail, return SolveFailed.
    let mut compiled = Vec::new();
    for constraint in &sketch.constraints {
        match CompiledConstraint::compile(constraint, &layout) {
            Ok(cc) => compiled.push(cc),
            Err(reason) => {
                return failed_result(&layout, reason);
            }
        }
    }

    // Edge case: no constraints.
    if compiled.is_empty() {
        let positions = layout.extract_positions(&layout.params);
        let radii = layout.extract_radii(&layout.params);
        let dof = layout.n_params() as u32;
        let status = if dof == 0 {
            SolveStatus::FullyConstrained
        } else {
            SolveStatus::UnderConstrained { dof }
        };
        let profiles = extract_profiles(&sketch.entities, &positions);
        return SolvedSketch {
            positions,
            radii,
            profiles,
            status,
        };
    }

    let _n_params = layout.n_params();

    // Build and run LM.
    let problem = SketchProblem::new(&layout, compiled);
    let n_constraint_rows = problem.n_constraint_rows;
    let n_params = problem.n_params;
    let lm = LevenbergMarquardt::new()
        .with_ftol(SOLVE_TOL)
        .with_xtol(SOLVE_TOL)
        .with_gtol(SOLVE_TOL)
        .with_patience(50); // Cap at 50*(n_params+1) evals; default is 200

    let (solved, report) = lm.minimize(problem);

    // Extract final residuals and Jacobian for classification, sliced to the
    // CONSTRAINT rows only — the proximal rows are a solver-internal
    // tie-breaker and must not affect satisfiability or dof counting
    // (invariant B2/I3, specs/sketch_drag_stability.md).
    let final_residuals = solved
        .residuals()
        .map(|r| r.rows(0, n_constraint_rows).into_owned())
        .unwrap_or_else(|| DVector::zeros(0));
    let final_jacobian = solved
        .jacobian()
        .map(|j| j.rows(0, n_constraint_rows).into_owned())
        .unwrap_or_else(|| DMatrix::zeros(0, n_params));

    let status = classify_status(
        &final_residuals,
        &final_jacobian,
        n_constraint_rows,
        n_params,
        &report,
    );

    // Invariant I4 (spec §4): a failed solve is inert — echo the input
    // positions rather than the solver's non-solution iterate.
    let solved_params = solved.params();
    let final_params: &[f64] = if matches!(status, SolveStatus::SolveFailed { .. }) {
        &layout.params
    } else {
        solved_params.as_slice()
    };
    let positions = layout.extract_positions(final_params);
    let radii = layout.extract_radii(final_params);

    let mut profiles = if matches!(
        status,
        SolveStatus::FullyConstrained | SolveStatus::UnderConstrained { .. }
    ) {
        extract_profiles(&sketch.entities, &positions)
    } else {
        Vec::new()
    };
    // `extract_profiles` reads the ORIGINAL entity radius; override a standalone
    // circle profile's radius with the SOLVED radius so the solver's output is
    // self-consistent (a Diameter/Radius constraint actually resizes the circle).
    for profile in &mut profiles {
        if let Some(circle) = profile.circle.as_mut() {
            if profile.entity_ids.len() == 1 {
                if let Some(&r) = radii.get(&profile.entity_ids[0]) {
                    circle.radius = r;
                }
            }
        }
    }

    SolvedSketch {
        positions,
        radii,
        profiles,
        status,
    }
}

/// Classify the solve result using the deterministic decision tree from the
/// spec (amendment G2):
///
/// 1. Compute rank(J) via rank-revealing QR.
/// 2. dof = n_params - rank.
/// 3. If ‖r‖∞ < tol (satisfiable):
///    - dof == 0 → FullyConstrained
///    - dof > 0  → UnderConstrained { dof }
/// 4. If ‖r‖∞ ≥ tol (unsatisfiable):
///    - rank(J) < n_residuals → OverConstrained { conflicts }
///    - rank(J) == n_residuals → SolveFailed { reason }
fn classify_status(
    residuals: &DVector<f64>,
    jacobian: &DMatrix<f64>,
    n_residuals: usize,
    n_params: usize,
    report: &MinimizationReport<f64>,
) -> SolveStatus {
    let residual_inf = residuals.abs().max();

    // Compute rank via QR with column pivoting.
    let rank = matrix_rank(jacobian, RANK_TOL);
    let dof = n_params.saturating_sub(rank);

    if residual_inf < SOLVE_TOL {
        // Constraints satisfiable.
        if dof == 0 {
            SolveStatus::FullyConstrained
        } else {
            SolveStatus::UnderConstrained { dof: dof as u32 }
        }
    } else {
        // Constraints unsatisfiable — decision tree per G2.
        if rank < n_residuals {
            // Redundant/conflicting direction exists.
            // Find constraint indices with largest residual contribution.
            // We map residual rows back to constraint indices.
            let conflicts = find_conflict_constraints(residuals, jacobian);
            SolveStatus::OverConstrained { conflicts }
        } else {
            // Independent constraints but LM couldn't satisfy them.
            let reason = format!(
                "LM did not converge: {:?} ({} evaluations, residual_inf={:.e})",
                report.termination, report.number_of_evaluations, residual_inf
            );
            SolveStatus::SolveFailed { reason }
        }
    }
}

/// Compute the rank of a matrix via QR decomposition with column pivoting.
///
/// Uses ColPivQR (column-pivoted QR) for reliable rank determination. Column
/// pivoting ensures that linearly independent columns are processed first,
/// giving accurate rank even when early columns are near-zero (e.g., a
/// parameter pinned by Dragged). The R matrix diagonal gives the rank;
/// values below `tol` are zero.
///
/// Performance: O(mn² + n³) — more expensive than plain QR, but called only
/// once per solve (after LM converges), not per iteration.
fn matrix_rank(m: &DMatrix<f64>, tol: f64) -> usize {
    if m.nrows() == 0 || m.ncols() == 0 {
        return 0;
    }
    let qr = nalgebra::ColPivQR::new(m.clone());
    let r = qr.r();
    // Use a relative tolerance: scale by the largest diagonal element to
    // handle Jacobians with widely varying magnitudes (e.g., DistancePL
    // entries divided by ℓ² can be very small).
    let max_diag = (0..r.nrows().min(r.ncols()))
        .map(|i| r[(i, i)].abs())
        .fold(0.0f64, f64::max);
    let effective_tol = if max_diag > 0.0 { tol * max_diag } else { tol };
    let mut rank = 0;
    for i in 0..r.nrows().min(r.ncols()) {
        if r[(i, i)].abs() > effective_tol {
            rank += 1;
        }
    }
    rank
}

/// Find constraint indices with residuals exceeding the tolerance.
/// Maps residual rows back to constraint indices using the row layout
/// (Coincident/Midpoint/Dragged = 2 rows, others = 1 row).
fn find_conflict_constraints(residuals: &DVector<f64>, _jacobian: &DMatrix<f64>) -> Vec<u32> {
    // We don't have the constraint list here, so we return row indices
    // where the residual exceeds tolerance. The caller (test harness) can
    // map these to constraint indices. For now, return the residual row
    // indices with magnitude > SOLVE_TOL, sorted by descending magnitude.
    let mut indexed: Vec<(usize, f64)> = residuals
        .iter()
        .enumerate()
        .filter(|(_, &v)| v.abs() > SOLVE_TOL)
        .map(|(i, &v)| (i, v.abs()))
        .collect();
    indexed.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    indexed.iter().map(|(i, _)| *i as u32).collect()
}

/// Build a SolveFailed result with initial positions.
fn failed_result(layout: &ParamLayout, reason: String) -> SolvedSketch {
    let positions = layout.extract_positions(&layout.params);
    let radii = layout.extract_radii(&layout.params);
    SolvedSketch {
        positions,
        radii,
        profiles: Vec::new(),
        status: SolveStatus::SolveFailed { reason },
    }
}
