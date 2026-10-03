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
use waffle_types::sketch_state::{
    ConstraintResidual, Convergence, FreeComponent, FreeDirection, MovedPoint, SketchSolveReport,
    Termination,
};

/// Default tolerance for residual satisfiability. 1e-6 m = 1 micrometer,
/// which is the kernel's feature-size floor per A14.2 (TAU_MODEL = 1e-7 m).
/// A solve tolerance one order of magnitude above the model tolerance is
/// sufficient to distinguish "constraints satisfied" from "constraints
/// violated" without false positives from floating-point noise.
/// **Decision banked: sub-micron precision is acceptable.**
const SOLVE_TOL: f64 = 1e-6;

/// Rank-revealing QR tolerance: a column whose pivoted R diagonal falls below
/// this FRACTION of the largest one is treated as rank-deficient. Relative to
/// the matrix, not absolute, so the same sketch authored at 0.001 m and at
/// 1000 m gets the same rank (measured: rank 8, dof 0 at both, and a dof-1
/// variant returns a bit-identical free direction at both).
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

/// Displacement below which a point counts as NOT moved by the solve
/// (`report.moved`). One nanometre: three orders below the solve tolerance,
/// so at the metre-ish scales sketches are usually authored at, a satisfied
/// solve's own numerical settling does not read as a move, while any
/// displacement a user or a dimension could have asked for does.
///
/// **It is ABSOLUTE, and the settling it is meant to sit under is not.**
/// Measured: the same satisfied rectangle authored at 1000 m settles ~4.5e-7
/// and therefore lists its own PINNED origin in `moved`. The threshold wants
/// to scale with the parameter magnitude; `SOLVE_TOL` has the same shape of
/// problem one scale further out (that rectangle at 1e4 m flips to
/// `SolveFailed`), so the two should be fixed together rather than separately.
const MOVED_EPS: f64 = 1e-9;

/// Component magnitude below which a free direction does not name a piece of
/// geometry (`report.free`). The basis vectors are unit-norm, so this is a
/// relative floor: a point contributing less than 1e-9 of a unit direction is
/// numerical dust, not a freedom the user can see.
const FREE_EPS: f64 = 1e-9;

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
        let weights: Vec<f64> = compiled.iter().map(weight).collect();
        let n_constraint_rows: usize = compiled.iter().map(residual_count).sum();

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
///
/// **Reference (driven) dimensions never drive.** They are filtered out of the
/// constraint set HERE, once, and every index the result reports —
/// `OverConstrained { conflicts }`, `report.conflicts`, `report.redundant`,
/// `report.residuals[].index` — indexes `sketch.constraints`, the caller's own
/// full array. Before S2 the filter lived in three places (the sketch UI, the
/// `sketch_create` tool and `feature_engine::params`) and the conflict indices
/// came back in the FILTERED space, so every consumer had to undo a mapping it
/// had applied itself (`specs/agent_mechanical_design.md` §2.2 item 10, §10.2).
pub fn solve_sketch(sketch: &Sketch) -> SolvedSketch {
    let layout = ParamLayout::build(&sketch.entities);

    // Compile the DRIVING constraints, remembering each one's index in the
    // caller's array. If any fail, return SolveFailed.
    let mut compiled = Vec::new();
    let mut driving_index: Vec<u32> = Vec::new();
    for (i, constraint) in sketch.constraints.iter().enumerate() {
        if constraint.is_reference() {
            continue;
        }
        match CompiledConstraint::compile(constraint, &layout) {
            Ok(cc) => {
                compiled.push(cc);
                driving_index.push(i as u32);
            }
            Err(reason) => {
                return failed_result(&layout, reason);
            }
        }
    }

    // Edge case: no driving constraints. Nothing to solve, but the state
    // report is still real — every parameter is a free direction.
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
        let report = SketchSolveReport {
            dof,
            params: layout.n_params() as u32,
            rank: 0,
            rows: 0,
            residuals: reference_residuals(sketch, &layout, &layout.params),
            conflicts: Vec::new(),
            redundant: Vec::new(),
            moved: Vec::new(),
            free: unconstrained_free_directions(&layout),
            convergence: Convergence {
                termination: Termination::NotRun,
                successful: false,
                evaluations: 0,
                residual_inf: 0.0,
                tolerance: SOLVE_TOL,
            },
        };
        return SolvedSketch {
            positions,
            radii,
            profiles,
            status,
            report,
        };
    }

    // Row → owning-constraint map for conflict reporting, in the CALLER's
    // index space: driving constraint k owns residual_count(k) consecutive
    // rows and reports as `driving_index[k]`. Built before `compiled` moves
    // into the problem.
    let mut row_owner: Vec<u32> = Vec::new();
    for (k, cc) in compiled.iter().enumerate() {
        for _ in 0..residual_count(cc) {
            row_owner.push(driving_index[k]);
        }
    }

    // Build and run LM.
    let problem = SketchProblem::new(&layout, compiled);
    let n_constraint_rows = problem.n_constraint_rows;
    let n_params = problem.n_params;
    // xtol is a RELATIVE step-size stop (delta ≤ xtol·‖x‖). At SOLVE_TOL it
    // halts a sketch with ‖x‖≈80 once steps shrink below 8e-5 — before the
    // residual reaches SOLVE_TOL — misclassifying a satisfiable solve as
    // SolveFailed (observed: release solve after a long drag stopped at
    // ‖r‖∞ = 8e-6, xtol:true). Keep xtol as a numerical-dawdle backstop only,
    // well below any step that could still move a residual past SOLVE_TOL;
    // ftol and patience govern convergence.
    let lm = LevenbergMarquardt::new()
        .with_ftol(SOLVE_TOL)
        .with_xtol(1e-12)
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

    let rank = matrix_rank(&final_jacobian, RANK_TOL);
    let status = classify_status(
        &final_residuals,
        rank,
        n_constraint_rows,
        n_params,
        &row_owner,
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

    // Solver state as data (§10.2 S2). Computed from the SAME final Jacobian
    // and rank the verdict is made on, so `report.dof` and `report.free`
    // cannot disagree with `status`.
    // nalgebra's `max` folds with `if a >= b { a } else { b }`, which DROPS a
    // NaN followed by any finite row — so a sketch whose first residual is NaN
    // reported `inf = 0` and came back green (`UnderConstrained`, every row
    // satisfied). A non-finite residual means the system was not solved, and
    // the verdict has to say so, so NaN propagates into `residual_inf` and
    // `satisfied` goes false.
    let residual_inf = if final_residuals.iter().any(|v| v.is_nan()) {
        f64::NAN
    } else {
        final_residuals.abs().max()
    };
    let satisfied = residual_inf < SOLVE_TOL;
    let dof = n_params.saturating_sub(rank);
    let state_report = SketchSolveReport {
        dof: dof as u32,
        params: n_params as u32,
        rank: rank as u32,
        rows: n_constraint_rows as u32,
        residuals: constraint_residual_rows(
            sketch,
            &layout,
            &solved.constraints,
            &driving_index,
            final_params,
        ),
        conflicts: find_conflict_constraints(&final_residuals, &row_owner),
        redundant: if satisfied && rank < n_constraint_rows {
            redundant_constraints(&solved.constraints, &driving_index, final_params, n_params)
        } else {
            Vec::new()
        },
        moved: moved_points(&layout, final_params),
        free: free_directions(&layout, &final_jacobian, n_params, dof),
        convergence: Convergence {
            termination: termination_of(&report),
            successful: report.termination.was_successful(),
            evaluations: report.number_of_evaluations as u32,
            residual_inf,
            tolerance: SOLVE_TOL,
        },
    };

    SolvedSketch {
        positions,
        radii,
        profiles,
        status,
        report: state_report,
    }
}

/// Classify the solve result using the deterministic decision tree from the
/// spec (amendment G2):
///
/// 1. Compute rank(J) via rank-revealing QR (the caller's `rank`).
/// 2. dof = n_params - rank.
/// 3. If ‖r‖∞ < tol (satisfiable):
///    - dof == 0 → FullyConstrained
///    - dof > 0  → UnderConstrained { dof }
/// 4. If ‖r‖∞ ≥ tol (unsatisfiable):
///    - rank(J) < n_residuals → OverConstrained { conflicts }
///    - rank(J) == n_residuals → SolveFailed { reason }
fn classify_status(
    residuals: &DVector<f64>,
    rank: usize,
    n_residuals: usize,
    n_params: usize,
    row_owner: &[u32],
    report: &MinimizationReport<f64>,
) -> SolveStatus {
    let residual_inf = residuals.abs().max();
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
            // Map offending residual rows to their owning CONSTRAINT indices.
            let conflicts = find_conflict_constraints(residuals, row_owner);
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

/// Find CONSTRAINT indices whose residuals exceed the tolerance, using the
/// row → owning-constraint map (multi-row constraints like Coincident /
/// Midpoint / Dragged / Pinned own 2 rows each). Deduplicated per
/// constraint, ordered by that constraint's largest |residual| descending —
/// these index the sketch's driving constraint list and feed the UI's
/// over-constraint badge highlighting.
fn find_conflict_constraints(residuals: &DVector<f64>, row_owner: &[u32]) -> Vec<u32> {
    // Aggregate the worst offending residual per owning constraint.
    let mut worst: std::collections::HashMap<u32, f64> = std::collections::HashMap::new();
    for (row, &v) in residuals.iter().enumerate() {
        // `NaN > tol` is false, so a non-finite row used to name NOBODY: the
        // verdict said `OverConstrained` and handed the UI an EMPTY conflict
        // set, which is the one thing this list exists to rule out. A row that
        // is not provably within tolerance is an offender.
        if v.is_nan() || v.abs() > SOLVE_TOL {
            let owner = row_owner[row];
            let entry = worst.entry(owner).or_insert(0.0);
            if v.abs() > *entry {
                *entry = v.abs();
            }
        }
    }
    let mut indexed: Vec<(u32, f64)> = worst.into_iter().collect();
    indexed.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.0.cmp(&b.0))
    });
    indexed.into_iter().map(|(i, _)| i).collect()
}

/// Build a SolveFailed result with initial positions.
///
/// A constraint that does not compile is an authoring error, not a search
/// failure: nothing was solved, so the state report carries the layout's own
/// facts (`params`, every parameter free) and a `NotRun` termination rather
/// than a fabricated residual set.
fn failed_result(layout: &ParamLayout, reason: String) -> SolvedSketch {
    let positions = layout.extract_positions(&layout.params);
    let radii = layout.extract_radii(&layout.params);
    SolvedSketch {
        positions,
        radii,
        profiles: Vec::new(),
        status: SolveStatus::SolveFailed {
            reason: reason.clone(),
        },
        report: SketchSolveReport {
            dof: layout.n_params() as u32,
            params: layout.n_params() as u32,
            rank: 0,
            rows: 0,
            residuals: Vec::new(),
            conflicts: Vec::new(),
            redundant: Vec::new(),
            moved: Vec::new(),
            free: unconstrained_free_directions(layout),
            convergence: Convergence {
                termination: Termination::Degenerate { detail: reason },
                successful: false,
                evaluations: 0,
                residual_inf: 0.0,
                tolerance: SOLVE_TOL,
            },
        },
    }
}

/// Per-constraint residuals for EVERY constraint in the sketch, in the
/// caller's declaration order.
///
/// Driving constraints are evaluated from the compiled form the solve used;
/// reference (driven) dimensions are compiled here on the side and evaluated
/// at the same solved parameters — a reference dimension's residual IS its
/// measurement error against the value it displays, which is the whole point
/// of a driven dimension. `residual: None` means the constraint could not be
/// compiled at all (only reachable for a reference dimension: a driving one
/// that fails to compile fails the solve).
fn constraint_residual_rows(
    sketch: &Sketch,
    layout: &ParamLayout,
    compiled: &[CompiledConstraint],
    driving_index: &[u32],
    params: &[f64],
) -> Vec<ConstraintResidual> {
    let worst = |cc: &CompiledConstraint| -> f64 {
        let r = cc.residuals(params);
        // NaN must PROPAGATE. `f64::max` returns the non-NaN operand, so a
        // plain `acc.max(v.abs())` fold reports a NaN row as `0.0` — and
        // `0.0` then reads as `satisfied: true`, which is the fabricated zero
        // the `Option<f64>` on this field exists to avoid. A NaN residual
        // (a zero-length line under an `OnEntity`, a dimension whose
        // expression evaluated to NaN) is a real, computed non-finite value:
        // reporting it keeps `satisfied` false, because `NaN < tol` is false.
        if r.iter().any(|v| v.is_nan()) {
            return f64::NAN;
        }
        r.iter().fold(0.0f64, |acc, v| acc.max(v.abs()))
    };
    sketch
        .constraints
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let residual = match driving_index.iter().position(|&d| d as usize == i) {
                Some(k) => Some(worst(&compiled[k])),
                None => CompiledConstraint::compile(c, layout)
                    .ok()
                    .map(|cc| worst(&cc)),
            };
            ConstraintResidual {
                index: i as u32,
                kind: c.kind().to_string(),
                residual,
                satisfied: matches!(residual, Some(r) if r < SOLVE_TOL),
                reference: c.is_reference(),
            }
        })
        .collect()
}

/// Residual rows for a sketch with no driving constraints: every constraint
/// present is a reference dimension (a driving one would have compiled).
fn reference_residuals(
    sketch: &Sketch,
    layout: &ParamLayout,
    params: &[f64],
) -> Vec<ConstraintResidual> {
    constraint_residual_rows(sketch, layout, &[], &[], params)
}

/// Dependent driving constraints of a SATISFIED, over-determined system
/// (`rank(J) < rows`): walk the constraints in declaration order and name
/// every one whose rows add no rank to the rows before it.
///
/// Declaration order makes the answer stable and names the LATER duplicate,
/// which is the one the author just added.
///
/// **COST: this is super-cubic and it is on the interactive solve path.** The
/// loop rebuilds a `DMatrix` from scratch and runs a full column-pivoted QR
/// once per constraint, so it is O(rows² · params²). Measured, native release,
/// pinned-point sketches: 6.1 ms at 25 points, 79.8 ms at 50, **669 ms at
/// 100** — ~180× the no-walk path and still climbing. Its gate
/// (`satisfied && rank < rows`) is not the rare case the wording above might
/// suggest: `rows > rank` holds for any consistent sketch carrying one more
/// constraint row than it has independent ones, which is ordinary once rails
/// and dimensions coexist. In WASM this runs on every `pointermove` of a drag.
///
/// The fix is algorithmic, not a tolerance or a cap: row dependence falls out
/// of ONE rank-revealing factorization of the stacked Jacobian's transpose,
/// and a cap would silently stop reporting redundancy on exactly the large
/// sketches that need it. Left as-is deliberately rather than band-aided.
fn redundant_constraints(
    compiled: &[CompiledConstraint],
    driving_index: &[u32],
    params: &[f64],
    n_params: usize,
) -> Vec<u32> {
    let mut accumulated: Vec<Vec<f64>> = Vec::new();
    let mut rank_so_far = 0usize;
    let mut dependent = Vec::new();
    for (k, cc) in compiled.iter().enumerate() {
        let j = cc.jacobian(params, n_params);
        let before = accumulated.len();
        for row in 0..j.nrows() {
            accumulated.push((0..n_params).map(|c| j[(row, c)]).collect());
        }
        let m = DMatrix::from_fn(accumulated.len(), n_params, |r, c| accumulated[r][c]);
        let rank = matrix_rank(&m, RANK_TOL);
        if rank == rank_so_far && accumulated.len() > before {
            dependent.push(driving_index[k]);
        }
        rank_so_far = rank;
    }
    dependent
}

/// Points the solve displaced, against the positions the caller sent in,
/// worst first.
fn moved_points(layout: &ParamLayout, final_params: &[f64]) -> Vec<MovedPoint> {
    let mut moved: Vec<MovedPoint> = layout
        .point_indices
        .iter()
        .filter_map(|(&id, &(xi, yi))| {
            let dx = final_params[xi] - layout.params[xi];
            let dy = final_params[yi] - layout.params[yi];
            let distance = dx.hypot(dy);
            (distance > MOVED_EPS).then_some(MovedPoint {
                id,
                dx,
                dy,
                distance,
            })
        })
        .collect();
    moved.sort_by(|a, b| {
        b.distance
            .partial_cmp(&a.distance)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.id.cmp(&b.id))
    });
    moved
}

/// Null-space basis of the final constraint Jacobian as named geometry moves.
///
/// `dof` directions, by construction: the basis is the `dof` eigenvectors of
/// `JᵗJ` with the smallest eigenvalues, where `dof` comes from the same
/// column-pivoted QR rank the verdict uses. Taking the count from the QR
/// rather than from a second rank test on `JᵗJ` is what keeps
/// `free.len() == dof` — two independent rank decisions on the same matrix can
/// differ by one near the tolerance, and a report whose freedom count
/// contradicts its own `dof` would be worse than no report.
///
/// `JᵗJ` rather than an SVD of `J`: with fewer constraint rows than
/// parameters (every under-constrained sketch) nalgebra's thin `v_t` has only
/// `min(m, n)` rows and cannot span the null space at all.
fn free_directions(
    layout: &ParamLayout,
    jacobian: &DMatrix<f64>,
    n_params: usize,
    dof: usize,
) -> Vec<FreeDirection> {
    if dof == 0 || n_params == 0 {
        return Vec::new();
    }
    if jacobian.nrows() == 0 {
        return unconstrained_free_directions(layout);
    }
    let jtj = jacobian.transpose() * jacobian;
    let eigen = nalgebra::SymmetricEigen::new(jtj);
    let mut order: Vec<usize> = (0..n_params).collect();
    order.sort_by(|&a, &b| {
        eigen.eigenvalues[a]
            .partial_cmp(&eigen.eigenvalues[b])
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.cmp(&b))
    });
    order
        .into_iter()
        .take(dof)
        .enumerate()
        .map(|(basis, col)| {
            let v = eigen.eigenvectors.column(col);
            // An eigenvector's sign is arbitrary; fix it so the first
            // significant component is positive and the report reads the same
            // way every time.
            let sign = v.iter().find(|x| x.abs() > FREE_EPS).map_or(1.0, |x| {
                if *x < 0.0 {
                    -1.0
                } else {
                    1.0
                }
            });
            FreeDirection {
                basis: basis as u32,
                components: components_of(layout, |i| sign * v[i]),
            }
        })
        .collect()
}

/// A sketch with no constraint rows at all: every parameter is its own free
/// direction, in a deterministic order (points by id, then radii by id).
fn unconstrained_free_directions(layout: &ParamLayout) -> Vec<FreeDirection> {
    let mut axes: Vec<usize> = Vec::new();
    let mut points: Vec<(u32, (usize, usize))> =
        layout.point_indices.iter().map(|(&k, &v)| (k, v)).collect();
    points.sort_by_key(|(id, _)| *id);
    for (_, (xi, yi)) in &points {
        axes.push(*xi);
        axes.push(*yi);
    }
    let mut radii: Vec<(u32, usize)> = layout
        .radius_indices
        .iter()
        .map(|(&k, &v)| (k, v))
        .collect();
    radii.sort_by_key(|(id, _)| *id);
    for (_, ri) in &radii {
        axes.push(*ri);
    }
    axes.into_iter()
        .enumerate()
        .map(|(basis, axis)| FreeDirection {
            basis: basis as u32,
            components: components_of(layout, |i| if i == axis { 1.0 } else { 0.0 }),
        })
        .collect()
}

/// Turn a parameter-space direction into the geometry it moves: one component
/// per point that translates and per radius that grows, above the noise floor,
/// ordered by id so the report is comparable between solves.
fn components_of(layout: &ParamLayout, at: impl Fn(usize) -> f64) -> Vec<FreeComponent> {
    let mut points: Vec<(u32, (usize, usize))> =
        layout.point_indices.iter().map(|(&k, &v)| (k, v)).collect();
    points.sort_by_key(|(id, _)| *id);
    let mut components: Vec<FreeComponent> = points
        .into_iter()
        .filter_map(|(id, (xi, yi))| {
            let (dx, dy) = (at(xi), at(yi));
            (dx.hypot(dy) > FREE_EPS).then_some(FreeComponent::Point { id, dx, dy })
        })
        .collect();
    let mut radii: Vec<(u32, usize)> = layout
        .radius_indices
        .iter()
        .map(|(&k, &v)| (k, v))
        .collect();
    radii.sort_by_key(|(id, _)| *id);
    for (entity, ri) in radii {
        let dr = at(ri);
        if dr.abs() > FREE_EPS {
            components.push(FreeComponent::Radius { entity, dr });
        }
    }
    components
}

/// LM's termination reason, typed, so no consumer parses a Debug string.
fn termination_of(report: &MinimizationReport<f64>) -> Termination {
    use levenberg_marquardt::TerminationReason as T;
    match report.termination {
        T::Converged { ftol, xtol } => Termination::Converged { ftol, xtol },
        T::ResidualsZero => Termination::ResidualsZero,
        T::Orthogonal => Termination::Orthogonal,
        T::LostPatience => Termination::LostPatience,
        T::NoImprovementPossible(detail) => Termination::NoImprovementPossible {
            detail: detail.to_string(),
        },
        T::Numerical(detail) => Termination::Numerical {
            detail: detail.to_string(),
        },
        T::User(detail) | T::WrongDimensions(detail) => Termination::Degenerate {
            detail: detail.to_string(),
        },
        T::NoParameters => Termination::Degenerate {
            detail: "no parameters".to_string(),
        },
        T::NoResiduals => Termination::Degenerate {
            detail: "no residuals".to_string(),
        },
    }
}
