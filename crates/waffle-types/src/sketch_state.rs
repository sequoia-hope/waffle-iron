//! Solver state as DATA (`specs/agent_mechanical_design.md` §10.2, S2).
//!
//! [`SolveStatus`](crate::SolveStatus) answers "is this sketch green?" and
//! nothing else: a conflict list that exists only on failure, a `dof` number
//! with no indication of WHICH freedoms are left, and no residuals at all. The
//! UI compensated with heuristics — a constraint-COUNT over-constraint badge
//! that overrides the solver and an under-constrained marker that flags
//! unreferenced points rather than the null space (§2.2 item 10). Both are
//! guesses about data the solver already has in hand and threw away.
//!
//! [`SketchSolveReport`] is that data, computed once in Rust, in the caller's
//! own index space:
//!
//! - **`residuals`** — one entry per constraint the caller handed in, carrying
//!   the UNWEIGHTED residual magnitude, so "by how much is this dimension
//!   violated" is a number and not an inference from the geometry.
//! - **`conflicts`** — always populated, not only on an `OverConstrained`
//!   verdict. A `Keep`-path solve that ends unsatisfied still names its
//!   offenders.
//! - **`redundant`** — the §10.2 verdict: a SATISFIED system whose row rank is
//!   below its row count is over-determined without being contradictory, and
//!   the dependent constraints are named. (Carried here rather than as a
//!   `SolveStatus` variant: every consumer of `SolveStatus` treats "satisfied,
//!   zero dof" as the green state, and a new variant would silently un-green
//!   every fully constrained sketch that happens to carry a duplicate.)
//! - **`moved`** — which points the solve actually displaced, against the
//!   positions the caller sent. A drag that moved nothing and a solve that
//!   quietly relocated half the sketch are indistinguishable without it.
//! - **`free`** — a null-space basis of the final constraint Jacobian: `dof`
//!   directions, each naming the points that move (and the radii that grow)
//!   along it. This is the real under-constrained answer.
//! - **`convergence`** — why LM stopped, typed.
//!
//! Index space: every index in this report indexes the constraint array of the
//! [`Sketch`](crate::Sketch) that was solved — the FULL array, reference
//! (driven) dimensions included. The solver filters those out of the driving
//! set itself and maps back, once, here, so neither the UI nor a tool repeats
//! the mapping (§10.2).

use serde::{Deserialize, Serialize};

/// Per-constraint residual, in the solved sketch's constraint index space.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct ConstraintResidual {
    /// Index into the solved sketch's `constraints` array.
    pub index: u32,
    /// The constraint's serde tag (`"Distance"`, `"Coincident"`, …), so a
    /// caller can present the row without re-reading the sketch.
    pub kind: String,
    /// Largest UNWEIGHTED residual magnitude over the constraint's rows. A
    /// multi-row constraint (`Coincident`, `Midpoint`, `Pinned`) reports its
    /// worst row. Units follow the constraint's own residual definition —
    /// meters for a length, radians-scaled for an angle.
    ///
    /// `None` when the constraint could not be compiled against the sketch's
    /// parameters at all — only reachable for a reference dimension, because a
    /// DRIVING constraint that fails to compile fails the whole solve. Never a
    /// stand-in zero: a number nobody computed is worse than an absence
    /// (`memory` `feedback_never_record_an_uncomputed_measurement`).
    pub residual: Option<f64>,
    /// `residual < tol`. The solver's own satisfiability test, per constraint.
    pub satisfied: bool,
    /// A reference (driven) dimension: measured, never driving, and so never
    /// an offender. Its `residual` is the measured violation of the value it
    /// displays, which is exactly what a reference dimension is for.
    pub reference: bool,
}

/// One component of a free (null-space) direction: how a single degree of
/// freedom moves one point, or grows one radius, per unit of that direction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum FreeComponent {
    /// A point that translates along this direction.
    Point { id: u32, dx: f64, dy: f64 },
    /// A circle whose radius parameter grows along this direction.
    Radius { entity: u32, dr: f64 },
}

/// A direction the geometry can still move in without violating a constraint —
/// one basis vector of the final constraint Jacobian's null space, normalized.
///
/// `free.len() == dof` by construction: the basis is computed from the same
/// rank-revealing decomposition that produces `dof`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct FreeDirection {
    /// Which basis vector this is (0-based, deterministic ordering).
    pub basis: u32,
    /// Components with a magnitude above the report's noise floor. A component
    /// that is numerically zero is omitted, so a rectangle free only in width
    /// names the two points that slide and not the two that stay.
    pub components: Vec<FreeComponent>,
}

/// A point the solve displaced, against the position the caller sent in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct MovedPoint {
    pub id: u32,
    pub dx: f64,
    pub dy: f64,
    /// `hypot(dx, dy)` — precomputed because every caller wants it and the
    /// ordering is by it.
    pub distance: f64,
}

/// Why the Levenberg-Marquardt loop stopped, typed.
///
/// Mirrors `levenberg_marquardt::TerminationReason` without leaking the
/// dependency into every consumer's types. `Converged` is the only reason that
/// claims a minimizer was found; the rest say what got in the way.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum Termination {
    /// The `ftol` or `xtol` step/objective criterion was met.
    Converged { ftol: bool, xtol: bool },
    /// Residuals are literally zero.
    ResidualsZero,
    /// Residual vector and Jacobian columns are near-orthogonal (`gtol`).
    Orthogonal,
    /// Hit the evaluation budget (`with_patience`).
    LostPatience,
    /// A tolerance was set below what the arithmetic can resolve.
    NoImprovementPossible { detail: String },
    /// `NaN` or infinity in the residuals or Jacobian.
    Numerical { detail: String },
    /// The problem had no parameters, no residuals, or a shape the solver
    /// rejected — degenerate input, not a failed search.
    Degenerate { detail: String },
    /// No LM run happened: the sketch had no driving constraint to solve.
    NotRun,
}

/// Convergence facts about the solve itself, separate from the verdict about
/// the sketch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct Convergence {
    pub termination: Termination,
    /// `termination.was_successful()` — LM's own view, which is NOT the same
    /// as "the sketch solved": an orthogonal stop at a non-zero residual is a
    /// successful search for an unsatisfiable system.
    pub successful: bool,
    /// Residual/Jacobian evaluations LM spent.
    pub evaluations: u32,
    /// `‖r‖∞` over the WEIGHTED constraint rows at the returned iterate — the
    /// number the satisfiability verdict is made on. (Weights are 1.0 for
    /// every constraint but `Dragged`, whose 1/20 interaction hint is not
    /// meant to be satisfied.) Per-constraint `residuals` are UNWEIGHTED, so
    /// they read as physical violations.
    pub residual_inf: f64,
    /// The satisfiability tolerance the verdict used.
    pub tolerance: f64,
}

/// Solver state for one solve, in the caller's constraint index space.
///
/// See the module docs for what each field is for and why it is here rather
/// than inferred downstream.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct SketchSolveReport {
    /// Degrees of freedom left: `n_params - rank(J)`. Present for EVERY
    /// verdict, including `FullyConstrained` (0) and the failures, where
    /// `SolveStatus` carries no number at all.
    pub dof: u32,
    /// Parameters the layout allocated (2 per point, 1 per circle radius).
    pub params: u32,
    /// Rank of the final constraint Jacobian.
    pub rank: u32,
    /// Constraint residual ROWS handed to the solver (a multi-row constraint
    /// contributes several). `rank < rows` on a satisfied system is the
    /// redundancy condition.
    pub rows: u32,
    /// One entry per constraint in the solved sketch, in declaration order.
    pub residuals: Vec<ConstraintResidual>,
    /// Driving constraints whose residual exceeds tolerance, worst first.
    /// Always populated — not only on an `OverConstrained` verdict.
    pub conflicts: Vec<u32>,
    /// Dependent driving constraints on a SATISFIED, over-determined system:
    /// each one whose rows add no rank to the constraints before it. Empty
    /// unless the system is satisfied with `rank < rows`.
    pub redundant: Vec<u32>,
    /// Points the solve displaced, worst first.
    pub moved: Vec<MovedPoint>,
    /// Null-space basis of the final constraint Jacobian: `dof` directions.
    pub free: Vec<FreeDirection>,
    /// Why LM stopped.
    pub convergence: Convergence,
}

impl Default for SketchSolveReport {
    fn default() -> Self {
        SketchSolveReport {
            dof: 0,
            params: 0,
            rank: 0,
            rows: 0,
            residuals: Vec::new(),
            conflicts: Vec::new(),
            redundant: Vec::new(),
            moved: Vec::new(),
            free: Vec::new(),
            convergence: Convergence {
                termination: Termination::NotRun,
                successful: false,
                evaluations: 0,
                residual_inf: 0.0,
                tolerance: 0.0,
            },
        }
    }
}
