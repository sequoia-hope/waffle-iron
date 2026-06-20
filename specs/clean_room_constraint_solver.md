# Clean-room 2D variational constraint solver — spec (PR-SS1)

## Goal

Replace the **libslvs** dependency (SolveSpace's constraint solver, GPL-3.0)
with a clean-room Rust implementation built on permissively-licensed numerical
primitives. **License motivation**: the project's intended license is MIT;
libslvs is the sole persistent-copyleft forcing function in the shipped WASM
binary. Every other component (kernel stack, three.js, Threlte, Svelte, spade,
dashu, nalgebra) is MIT / MIT-Apache / BSD. Removing libslvs unblocks MIT
relicensing.

**No code from libslvs or SolveSpace is consulted.** Implementation derives
from published algorithms (papers + Nocedal & Wright). The existing
`crates/slvs-patch/` vendored source and `app/src/lib/engine/slvs-solver.js`
are treated as a **dev-only parity oracle** during transition and deleted in
the cutover PR — the same dev-oracle pattern the project used for the LGPL
Attene predicates in M7 (see `specs/indirect_predicates_sidecar_scaffold.md`).

PR-SS1 ships the **core solver + parity harness** for the 4-entity / 13-constraint
subset Waffle Iron exposes in its UI today. The remaining 8 mapped-but-unexposed
constraints and ellipse/Bezier support are banked to follow-up PRs.

## License status of candidate dependencies (verified)

| Component | License | Role |
|---|---|---|
| **`levenberg-marquardt`** crate (rust-cv, v0.15.0) | MIT/Apache-2.0 | LM numerical core — port of MINPACK (public domain) |
| **`nalgebra`** | BSD-3-Clause | Linear algebra + rank-revealing QR for DOF analysis |
| **argmin** (optional) | MIT/Apache-2.0 | ~~Alternative optimizer framework (BFGS, dogleg) if needed~~ **DROPPED from PR-SS1 scope.** `levenberg-marquardt` + `nalgebra` cover the PR-SS1 feature set. If BFGS/dogleg is ever needed for PR-SS2+, add it then. |
| libslvs / SolveSpace | GPL-3.0 | **REMOVED** — current dep |
| FreeCAD PlaneGCS | LGPL-2.1+ | **REJECTED** — static link into WASM copylefts combined work |
| NoteCAD (C#) | No LICENSE file / "contact for licensing" | **REJECTED** — proprietary |
| GeoSolver (Python) | GPL | **REJECTED** |

Survey of Wikipedia's "Geometric constraint solving" implementations list and
Rust crates.io found **no permissively-licensed 2D geometric constraint solver**
suitable for adoption. A clean-room implementation is the only path to MIT.

## Theoretical basis

The implementation is a **variational solver**: model entities as parameter
vectors and constraints as residual equations, then minimize the sum of squared
residuals with damped least-squares (Levenberg-Marquardt). This is the same
algorithmic family SolveSpace and FreeCAD PlaneGCS use; we implement from the
published literature, not from their source.

Primary sources (all open-access or standard textbook). Reference numbers
refer to entries in `REFERENCES.md`:

1. **Moré, J.J. (1978)** [REF #43] "The Levenberg-Marquardt algorithm:
   Implementation and theory" — the original MINPACK LM paper. Public-domain
   via MINPACK. Local copy: none (paywalled at Springer LN); algorithm is in
   full in Nocedal-Wright #44 and MINPACK source #45.
2. **Nocedal & Wright, *Numerical Optimization*** [REF #44], ch. 4 (LM) &
   ch. 10 (nonlinear least-squares) — the canonical textbook description. The
   algorithm is free to implement. Local copy: none (textbook).
3. **Bouma, Fudos, Hoffmann, Cai, Paige (1993)** [REF #40] "A Geometric
   Constraint Solver" — Purdue CSD-TR, open access. Constructive decomposition
   framework; used as conceptual reference for DOF analysis, not for the
   numerical core. Local copy: `refs/bouma1993_geometric_constraint_solver.pdf`.
4. **Hoffmann, Lomonosov, Sitharam (2001)** [REF #47] "Decomposition Plans
   for Geometric Constraint Systems, Parts I & II" — *J. Symbolic Computation*
   31(4). Cluster decomposition canon. Local copy: none (paywalled).
5. **Haller, Lee-St.John, Sitharam, Streinu, White (2010)** [REF #42]
   "Body-and-cad Geometric Constraint Systems" — **arXiv:1006.1126**, fully
   open. Local copy: `refs/haller2010_body_and_cad.pdf`.
6. **Joan-Arinyo** [REF #49] "Basics on Geometric Constraint Solving" —
   survey, CiteSeerX. Local copy: none (browser-only access).
7. **Gao, Lin, Zhang (2006)** [REF #41] "A C-Tree Decomposition Algorithm
   for 2D and 3D GCS" — HAL open archive. Local copy:
   `refs/gao2006_ctree_decomposition.pdf`.
8. **MINPACK** [REF #45] — the public-domain reference LM implementation.
   Local copy: `refs/minpack_lmdif_source.txt` (Fortran source of `lmdif`).
9. **`levenberg-marquardt` crate** [REF #46] — MIT/Apache Rust port of
   MINPACK, the numerical core we build on. Local copy:
   `refs/levenberg_marquardt_crate_readme.md`.

**The dragged-point 1/20-scaling trick** (SolveSpace's signature interaction
behavior) is implemented from the algorithmic *description* in
`docs/SKETCH-SYSTEM-PLAN.md:48` and the SolveSpace wiki, **not from SolveSpace
source**. The technique is documented in user-facing material and is not
patented.

## Public API (unchanged — contract preserved)

The crate's public surface is the existing one. No consumer of `sketch-solver`
sees any change:

```rust
// crates/sketch-solver/src/lib.rs (unchanged exports)
pub mod constraint_mapping;  // rewritten
pub mod entity_mapping;      // rewritten
pub mod profiles;
pub mod solver;
pub mod status;
pub mod types;

pub use profiles::extract_profiles;
pub use solver::solve_sketch;
pub use types::*;
```

```rust
// Existing signature preserved verbatim — crates/sketch-solver/src/solver.rs
pub fn solve_sketch(sketch: &Sketch) -> SolvedSketch;
```

Input/output types are the existing ones in `waffle-types/src/sketch.rs`:
`Sketch`, `SketchEntity` (Point/Line/Circle/Arc/Spline/Gear),
`SketchConstraint` (21 variants), `SolveStatus` (FullyConstrained /
UnderConstrained{dof} / OverConstrained{conflicts} / SolveFailed{reason}),
`SolvedSketch` (positions + profiles + status).

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│ sketch-solver crate (Rust, MIT after cutover)               │
│                                                             │
│  solve_sketch(&Sketch) -> SolvedSketch                      │
│     │                                                       │
│     ├─ entity_mapping.rs   ─► ParamLayout                   │
│     │    map SketchEntity → flat parameter vector           │
│     │    Point  → (x, y)        : 2 params                   │
│     │    Circle → (r)           : 1 param (center is point) │
│     │    Line   → no extra params (defined by its 2 points) │
│     │    Arc    → no extra params (defined by 3 points)     │
│     │                                                       │
│     ├─ constraint_mapping.rs ─► ResidualFn + Jacobian       │
│     │    each SketchConstraint variant → residual block     │
│     │    analytic Jacobian per constraint (unit-tested      │
│     │    against levenberg-marquardt::differentiate_        │
│     │    numerically)                                       │
│     │                                                       │
│     ├─ solver.rs ─► LM minimization                        │
│     │    build LeastSquaresProblem impl, hand to            │
│     │    levenberg_marquardt::LevenbergMarquardt::new()     │
│     │    .minimize(problem)                                 │
│     │                                                       │
│     ├─ status.rs ─► SolveStatus classification              │
│     │    rank-revealing QR on final Jacobian (nalgebra)     │
│     │    → rank == #params : FullyConstrained              │
│     │    → rank <  #params : UnderConstrained { dof = n-r} │
│     │    → residual ||r|| > tol after convergence:          │
│     │       classify as OverConstrained (conflicts =        │
│     │       constraints with largest residual contribution)│
│     │       or SolveFailed (non-convergence)                │
│     │                                                       │
│     └─ profiles.rs (unchanged) ─► ClosedProfile extraction   │
└─────────────────────────────────────────────────────────────┘
                            ▲
                            │ depends on
                            ▼
┌─────────────────────────────────────────────────────────────┐
│ levenberg-marquardt 0.15  (MIT/Apache)  ─ MINPACK port      │
│ nalgebra 0.34             (BSD-3)       ─ LA + RRQR          │
└─────────────────────────────────────────────────────────────┘
```

### Dragged-point handling

A `Dragged { point }` constraint (sketch.rs:302) fixes a point's position.
For interactive drag, the JS layer adds a temporary `Dragged` constraint on
the dragged point, re-solves, removes it on mouseup. SolveSpace's
1/20-scaling trick — where the dragged point's residual weight is reduced so
it absorbs the displacement while the rest of the sketch adjusts minimally —
is implemented in the residual-weighting layer, not in the LM core:

- Each residual `r_i` has a weight `w_i` (default 1.0).
- For the dragged point's `WhereDragged` residual, `w_i = 1/20`.
- Weight is **fixed per solve** — no per-iteration reweighting. This matches
  SolveSpace's published behavior (the 1/20 scaling is a constant residual
  weight, not an adaptive scheme).
- The LM problem becomes `min Σ (w_i · r_i)²`. This is a standard weighted
  least-squares formulation; the `levenberg-marquardt` crate supports it via
  residual scaling in `set_params` / `residuals`. If the crate's API does not
  natively expose per-residual weights, apply weighting by pre-multiplying the
  residual vector by `diag(w)` before handing to LM — mathematically
  equivalent.

This is implemented from the description in `docs/SKETCH-SYSTEM-PLAN.md:48` and
the SolveSpace user-facing wiki, **not from SolveSpace source code**.

## Constraint residual + Jacobian table

Each constraint contributes one or more rows to the residual vector and the
Jacobian. The table below is the **complete specification** of the
`constraint_mapping.rs` implementation for PR-SS1. Notation:

- Point `P_i` has params `(x_i, y_i)`.
- Line `L` from `P_a` to `P_b`: direction `d = (x_b−x_a, y_b−y_a)`, length `ℓ = ‖d‖`.
- Circle `C` with center `P_c` and radius `r`.
- Arc `A` with center `P_c`, start `P_s`, end `P_e`.

| Constraint | Residual `r` | ∂r/∂params (Jacobian row) |
|---|---|---|
| **Coincident** (P_a, P_b) | `(x_a−x_b, y_a−y_b)` | 2 rows; ±1 on x_a/x_b, ±1 on y_a/y_b |
| **Horizontal** (L) | `y_b − y_a` | +1 on y_b, −1 on y_a |
| **Vertical** (L) | `x_b − x_a` | +1 on x_b, −1 on x_a |
| **Parallel** (L_a, L_b) | `d_a × d_b = (x_b−x_a)(y_d−y_c) − (y_b−y_a)(x_d−x_c)` | expand per product rule |
| **Perpendicular** (L_a, L_b) | `d_a · d_b = (x_b−x_a)(x_d−x_c) + (y_b−y_a)(y_d−y_c)` | expand per product rule |
| **Equal** (L_a, L_b) | `ℓ_a² − ℓ_b²` (use squared to avoid sqrt) | `2(x_b−x_a)(∂...)` etc. |
| **Equal** (C_a, C_b) | `r_a − r_b` | ±1 on r_a, r_b |
| **Distance** (P_a, P_b, v) | `‖P_b−P_a‖ − v` | `(P_b−P_a)/‖P_b−P_a‖` |
| **Distance** (P, L, v) | `((P−P_a)×d)/ℓ − v` | signed perpendicular distance |
| **Angle** (L_a, L_b, θ°) | `atan2(d_a×d_b, d_a·d_b) − θ` | derivative of atan2 |
| **Radius** (C/A, v) | `r − v` (or `2r − v` for diameter form) | ±1 or ±2 on r |
| **Diameter** (C/A, v) | `2r − v` | ±2 on r |
| **OnEntity** (P, L) | signed perpendicular distance `((P−P_a)×d)/ℓ` | as Distance |
| **OnEntity** (P, C) | `‖P−P_c‖ − r` | as Distance |
| **Midpoint** (P, L) | `P − (P_a+P_b)/2` | 2 rows; ±1/2 on each endpoint |
| **Dragged** (P) | `P − P_fixed` | 2 rows; ±1; weight 1/20 |

**PR-SS1 scope**: the 13 constraints above (Coincident, Horizontal, Vertical,
Parallel, Perpendicular, Equal, Distance, Angle, Radius, Diameter, OnEntity,
Midpoint, Dragged) — these cover the 11 UI-exposed constraints plus Distance
and OnEntity which the UI uses today.

**Banked to PR-SS2** (8 mapped-but-unexposed constraints): Symmetric,
SymmetricH, SymmetricV, Tangent (arc-line), EqualAngle, Ratio,
EqualPointToLine, SameOrientation. Each needs its own residual+Jacobian entry
following the same pattern; straightforward once PR-SS1 lands.

**Banked to PR-SS3** (capabilities libslvs lacks): ellipse entity, B-spline /
cubic Bezier, PointOnLine with non-workplane 3D. These are stretches that
libslvs also doesn't do well; defer per `docs/SKETCH-SYSTEM-PLAN.md` Phase 6.

## Status classification algorithm

After LM converges (or terminates), classify via the Jacobian `J ∈ R^{m×n}`
evaluated at the solution:

1. Compute rank-revealing QR of `J` (nalgebra `QR` or `ColPivQR`).
2. `rank = number of independent constraint directions`.
3. `dof = n_params − rank`.
4. Determine satisfiability: `‖residual‖∞ < tol` (constraints satisfiable) vs
   `‖residual‖∞ ≥ tol` (constraints unsatisfiable).
5. **If satisfiable** (`‖residual‖∞ < tol`):
   - `dof == 0` → `SolveStatus::FullyConstrained`
   - `dof > 0`  → `SolveStatus::UnderConstrained { dof }`
6. **If unsatisfiable** (`‖residual‖∞ ≥ tol`) — apply the deterministic
   decision tree below. The two LM termination conditions (rank-deficiency
   vs max-iter/step-too-small) can overlap; the decision is driven by
   `rank(J)`, not by LM's reported termination reason:

   - **(a) `rank(J) < #constraints`** (a redundant or conflicting constraint
     direction exists at the solution) → `OverConstrained`. Populate
     `conflicts` with the constraint indices whose residual row magnitude
     exceeds `tol`, sorted by descending residual magnitude. This is the
     canonical case for contradictory constraints such as
     `Distance(P_a, P_b, 10)` + `Distance(P_a, P_b, 20)`: the Jacobian has
     rank 1 (two constraints on the same 1-DOF direction), and both residuals
     are non-zero at the LM midpoint solution `(10+20)/2 = 15`.
   - **(b) `rank(J) == #constraints`** (no redundant direction; constraints are
     independent but LM could not satisfy them) → `SolveFailed { reason }`.
     Reason string records the LM termination cause (max-iterations or
     step-too-small) for diagnostics.
   - **(c) Degenerate-input carve-out:** if the Jacobian cannot be assembled
     (e.g., a constraint references a zero-length line where direction is
     undefined) → `SolveFailed { reason: "constraint {i} references
     degenerate geometry: {detail}" }`.

`tol = 1e-9` (matches SolveSpace's published tolerance; configurable via a
`SolverTolerance` param in `solver.rs`).

## Error contract

Per `crates/cherchi-rs/CLAUDE.md` Hard Rule #6 (no `panic!` in production
paths), `solve_sketch` returns `SolvedSketch` with a `SolveStatus` variant on
failure — never panics. The current `entity_mapping.rs` / `constraint_mapping.rs`
use `.expect("failed to add ...")` which **violates this rule**; the rewrite
replaces those with graceful `SolveFailed { reason }` returns. This is a
correctness improvement over the current code, not a regression.

Unknown entity IDs in constraints (referencing a point/line not in the
entities list) → `SolveFailed { reason: "constraint references unknown entity
{id}" }`, not a panic.

**`thiserror` dependency:** retained only if the existing `status.rs` defines
an error type that derives `thiserror::Error` for inter-crate conversion
(`wasm-bridge` may rely on `From` impls). PR-SS1a verifies this during the
dep swap — if no such type exists, `thiserror` is dropped from the AFTER
`Cargo.toml`. If it exists, it stays. This is a verification step, not an
open question.

## Determinism

Per governance `ARCHITECTURAL_INVARIANTS.md` and `AGENTS.md` rule 13:
- No random initialization. Initial parameter values come from the sketch
  entity declarations (`SketchEntity::Point { x, y }`, `Circle { radius }`).
- No system time, no `HashMap` iteration in the solve path. The residual /
  Jacobian assembly order is determined by a stable sort of constraint
  indices.
- LM iteration count and termination are deterministic given identical inputs
  and floating-point environment. The `levenberg-marquardt` crate documents
  floating-point-identical output to MINPACK — this satisfies the determinism
  invariant.

## Crate dependency change

```toml
# crates/sketch-solver/Cargo.toml — BEFORE
[dependencies]
waffle-types = { path = "../waffle-types" }
slvs = "0.6"                          # ← REMOVED
serde = { version = "1", features = ["derive"] }
uuid = { version = "1", features = ["v4", "serde"] }
thiserror = "1"

# crates/sketch-solver/Cargo.toml — AFTER
[dependencies]
waffle-types = { path = "../waffle-types" }
levenberg-marquardt = "0.15"          # MIT/Apache
nalgebra = "0.34"                     # BSD-3
serde = { version = "1", features = ["derive"] }
uuid = { version = "1", features = ["v4", "serde"] }
thiserror = "1"
```

`nalgebra` is already transitively present via `levenberg-marquardt`; listing
it explicitly is for the rank-revealing QR used in status classification.

## WASM build change

```toml
# crates/wasm-bridge/Cargo.toml — BEFORE
[features]
default = ["native-solver"]
native-solver = ["sketch-solver"]   # native build: Rust solver

# crates/wasm-bridge/Cargo.toml — AFTER
[features]
default = ["native-solver"]         # ← now the ONLY path
native-solver = ["sketch-solver"]
# The #[cfg(not(feature = "native-solver"))] branch in dispatch.rs:58
# (which returned NotImplemented "use JS bridge to libslvs WASM")
# is DELETED. The WASM build uses --features native-solver (the default),
# and sketch solving runs in Rust/WASM, not JS.
```

`crates/wasm-bridge/src/dispatch.rs:48-66` simplifies to the single
`#[cfg(feature = "native-solver")]` block; the `#[cfg(not(...))]` arm is
removed. Sketch solving now goes through the same Rust path in both native
and WASM builds.

## Parity harness (dev-only, deleted at cutover)

During PR-SS1 development, libslvs remains available as a **dev-only oracle**.
The parity harness lives in `crates/sketch-solver/tests/parity.rs` and is
`#[cfg(feature = "libslvs-oracle")]`-gated:

```rust
// crates/sketch-solver/Cargo.toml — dev-only
[features]
libslvs-oracle = ["slvs"]   # dev-only, never default, not in release

[dev-dependencies]
slvs = "0.6"                # dev-only oracle
```

The harness constructs a `Sketch` from each test fixture, runs both
`solve_sketch` (new) and the old libslvs path, and asserts:

- **Position agreement** to within `1e-6` on each solved point coordinate.
- **Status agreement**: same `SolveStatus` variant (Fully/Under/Over/Failed).
- **DOF agreement** for `UnderConstrained` cases.

Fixtures are the existing `test-harness` assay corpus (seed 42) plus a
hand-curated set of degenerate cases libslvs handles well that a clean-room
implementation must match:

- Zero-length line (P_a == P_b).
- Two parallel lines that should be coincident under `Distance(P, L, 0)`.
- Circle with `Radius = 0`.
- Tangent arc whose start/end collapse to the same point.
- Over-constrained but consistent sketch (redundant constraints).
- Under-constrained sketch with `WhereDragged` pinning one point.

When the parity harness is green across the full corpus, the cutover PR
deletes `crates/slvs-patch/`, `app/src/lib/engine/slvs-solver.js`, the
Emscripten step in `Dockerfile`, the `slvs.wasm` fetch in
`app/src/lib/engine/worker.js:18-31`, and the `libslvs-oracle` feature.
At that point the only persistent copyleft in the shipped binary is gone.

## Test plan

6 groups, ~50 tests, in `#[cfg(test)] mod tests` within each module plus the
parity harness under `tests/`.

### Group 1 — Entity mapping (`entity_mapping.rs`)

- Empty sketch → 0 params, status FullyConstrained.
- Single point → 2 params.
- Line (2 points) → 4 params.
- Circle (center + radius param) → 3 params.
- Arc (3 points) → 6 params.
- Mixed sketch (5 points, 2 lines, 1 circle, 1 arc) → correct param count.
- Determinism: same input → identical param layout across runs.

### Group 2 — Residual + Jacobian correctness

Each constraint variant: unit test the residual function AND the analytic
Jacobian against `levenberg_marquardt::differentiate_numerically` (the
crate's built-in numerical-differentiation helper, used for exactly this
verification purpose — see `docs.rs/levenberg-marquardt`). Tolerance `1e-9`.

- Coincident: residual is 2D zero vector when points coincide.
- Horizontal/Vertical: residual zero when line is axis-aligned.
- Parallel/Perpendicular: residual zero at the expected relative angles.
- Equal (lines): residual zero when lengths match; Jacobian matches numeric.
- Equal (circles): residual zero when radii match.
- Distance: residual zero at target distance; Jacobian is unit direction.
- Angle: residual zero at target angle; Jacobian verified numerically.
- Radius/Diameter: residual zero at target.
- OnEntity (line): residual = perpendicular distance.
- OnEntity (circle): residual = radial distance.
- Midpoint: residual zero when point is at line midpoint.
- Dragged: residual zero when point is at fixed position.

### Group 3 — Solver convergence

- Single point fixed by `Dragged` → stays put, FullyConstrained.
- Two points + `Distance(10)` → solved distance == 10, `UnderConstrained {
  dof: 3 }` (2 translational + 1 rotational).
- Two points + `Distance(10)` + `Horizontal` → FullyConstrained, positions
  (0,0) and (10,0) (modulo initial-condition offset).
- Rectangle (4 points, 4 lines, 4 coincident, 2 Horizontal, 2 Vertical, 1
  Equal) → FullyConstrained, expected positions.
- Circle + `Radius(5)` → radius solved to 5, UnderConstrained (center free).
- Circle + `Radius(5)` + `Dragged(center)` → FullyConstrained.

### Group 4 — Status classification

- Fully-constrained rectangle → `FullyConstrained`, dof 0.
- Single unconstrained point → `UnderConstrained { dof: 2 }`.
- Two points + `Distance` only → `UnderConstrained { dof: 3 }` (translation +
  rotation).
- Contradictory: `Distance(P_a, P_b, 10)` AND `Distance(P_a, P_b, 20)` →
  `OverConstrained { conflicts: [both constraint indices] }`.
- Degenerate: `Distance(P, P, 5)` (point to itself, nonzero distance) →
  `OverConstrained` or `SolveFailed` (per convergence path).
- LM non-convergence (pathological init) → `SolveFailed { reason }`.

### Group 5 — Dragged-point interaction

- Pin one point of a 2-point `Distance(10)` sketch → dragged point stays
  fixed, other point moves to satisfy distance.
- Weight verification: dragged residual weight is 1/20, others are 1.
- Drag in a rectangle: one corner pinned, opposite corner dragged → sketch
  rescales while preserving constraints.

### Group 6 — Parity vs libslvs (feature `libslvs-oracle`)

- For each hand-curated fixture: position agreement < 1e-6, status agreement.
- For 20 random sketches from the assay corpus: position agreement < 1e-6.
- Degenerate fixtures list (see Parity harness section above): all green.

### Determinism invariant test

- Run `solve_sketch` twice on identical input → byte-identical `SolvedSketch`
  (positions, profiles, status).
- Run with `RUSTFLAGS="--cfg debug_assertions"` to confirm no hidden
  non-determinism (no HashMap iteration in solve path).

## Implementation order

PR-SS1 is split into 4 sub-PRs for review tractability (per `AGENTS.md` rule
7: "if stuck for more than 15 minutes without a commit, the task scope is too
broad"). These land as sequential commits on a single feature branch
(`new-solver`); the project does not use PRs. Merge to `main` occurs once
PR-SS1d is green and adversary-validated.

### Feature-cycle classification (DoD compliance)

PR-SS1 is **not** a single feature cycle. Per `DEFINITION_OF_DONE.md` §0, the
sub-PRs are classified by change type, which determines whether FIP applies:

| Sub-PR | Change type (DoD §) | FIP? | Roles |
|--------|---------------------|------|-------|
| PR-SS1a | Infrastructure / Tooling (§6) | No | Solo agent (role A, assigned by Manager) |
| PR-SS1b | Infrastructure / Tooling (§6) | No | Solo agent (role A) |
| PR-SS1c | Modeling Feature (§1) | **Yes — full FIP** | Test Author (B), Implementer (C), Adversary (D) — role separation enforced per P5 |
| PR-SS1d | Bug-fix / cutover variant (§2/§8) | Yes (FIP §8 variant) | Test Author (B), Implementer (C), Adversary (D) |

PR-SS1a/1b are infrastructure (dep swap, scaffolding, Jacobian unit
verification) — no FIP required, just tests-pass + determinism preserved.
PR-SS1c is the modeling feature that ships the working solver: full FIP with
role separation (B writes Groups 3/4/5, C implements, D validates). PR-SS1d
is the cutover bug-fix variant: parity tests first, then deletion.

### Legacy-solver feature flag

To keep `main` green at every stage, the old slvs path is retained behind a
non-default `legacy-solver` feature through PR-SS1a/1b/1c. `solve_sketch`
dispatches to the legacy path when `legacy-solver` is on, the new path when
off. Production builds keep `legacy-solver` as the default until PR-SS1d
flips the default and removes the flag entirely. This means **no intermediate
commit regresses the solver** — the new path is opt-in via
`--no-default-features` during development.

### PR-SS1a — Scaffold + entity mapping

- Replace `Cargo.toml` deps (slvs → levenberg-marquardt + nalgebra); keep
  `slvs` behind the `legacy-solver` feature.
- Rewrite `entity_mapping.rs` as `ParamLayout` (no slvs types).
- Add `libslvs-oracle` dev-only feature and empty parity harness stub.
- Verify `thiserror` retention per §"Error contract".
- Tests: Group 1.
- The new path's `solve_sketch` returns positions unchanged with
  `SolveFailed { reason: "not yet implemented" }`; legacy path unchanged.

### PR-SS1b — Constraint residuals + Jacobians

- Implement `constraint_mapping.rs` for the 13 PR-SS1 constraints.
- Each constraint: residual fn + analytic Jacobian + numerical-Jacobian unit
  test.
- Tests: Group 2.
- No end-to-end solving yet.

### PR-SS1c — Solver + status classification

- Implement `solver.rs` using `levenberg_marquardt::LevenbergMarquardt`.
- Implement `status.rs` rank-revealing QR classification.
- Wire dragged-point 1/20 weighting.
- Tests: Groups 3, 4, 5.

### PR-SS1d — Parity + code cutover

- Populate parity harness fixtures (Group 6).
- Run against full assay corpus.
- When green: flip the default (new path is the only path; remove
  `legacy-solver` feature). Delete the full slvs removal checklist (see
  §"slvs removal checklist" below).
- Adversary phase: full workspace `cargo test`; WASM smoke build
  (`cargo build --target wasm32-unknown-unknown -p wasm-bridge`); confirm no
  slvs symbol remains in active code (historical-doc header notes excepted).
- **Relicense is NOT part of PR-SS1d.** See §"Relicense (Stage 5)" below.

### slvs removal checklist

The slvs/libslvs footprint spans ~20 files. PR-SS1d processes this list in
full. Files are grouped by action.

**Code/artifacts to delete:**
- `crates/slvs-patch/` (entire vendored tree)
- `app/src/lib/engine/slvs-solver.js`
- `app/static/pkg/slvs/` (built Emscripten artifacts)
- Root `Cargo.toml` `[patch.crates-io]` slvs entry (lines 34-38)
- `crates/sketch-solver/Cargo.toml` — drop `slvs`, add `levenberg-marquardt` +
  `nalgebra`; remove `legacy-solver` feature
- `crates/wasm-bridge/Cargo.toml` — `native-solver` becomes the only path;
  remove the `legacy-solver`-conditional dep

**Code to edit:**
- `crates/wasm-bridge/src/dispatch.rs:48-66` — delete the
  `#[cfg(not(feature = "native-solver"))]` arm (the "use JS bridge to libslvs"
  NotImplemented branch)
- `crates/wasm-bridge/tests/bridge_tests.rs:512` — un-`#[ignore]` the test
  (now runs on all builds)
- `Dockerfile` — remove the Emscripten SDK install step (slvs-specific).
  **Verify before removing clang/cmake**: `cherchi-sidecar-rs` still needs a
  C++ toolchain. Only remove the Emscripten step.

**Non-protected docs to update (agent-doable in PR-SS1d):**
- `README.md` — stack table line 16 (remove slvs attribution); line 36
  (license text change deferred to Stage 5, but remove slvs mention now)
- `STATUS.md` — lines 12, 54
- `DEVELOPMENT.md` — lines 26-27 (toolchain notes; verify clang/cmake still
  needed for cherchi-sidecar before removing)
- `CLAUDE.md` — lines 286-288 (native-solver feature explanation)
- `INTERFACES.md` — lines 437-440 (doc comment referencing libslvs)
- `architecture.html` — lines 133-134, 170-171, 267, 409, 560, 562, 590
  (diagram nodes + table refs)
- `docs/SYSTEM_DESIGN.md` — lines 22, 156
- `docs/SKETCH-SYSTEM-PLAN.md` — add header note: "Historical planning record;
  solver replaced by clean-room implementation per
  `specs/clean_room_constraint_solver.md`." Do not rewrite — it is a planning
  record, not an active doc.
- `projects/02-sketch-solver/{ARCHITECTURE,CLAUDE,INTERFACES,PLAN,HANDOFF,WASM_STRATEGY}.md`
  — add "superseded by clean-room solver" header notes
- `projects/03-wasm-bridge/{ARCHITECTURE,CLAUDE,PLAN}.md` — remove libslvs
  WASM module section
- `projects/05-sketch-ui/PLAN.md` — lines 64, 124
- `projects/10-assemblies/{ARCHITECTURE,PLAN}.md` — lines 33, 22

**Protected docs (deferred to Stage 5 — require human approval per
Constitution §11):**
- `ARCHITECTURE.md` — lines 16, 32, 95, 120, 131, 209 (GPL-3.0 + slvs
  references). PR-SS1d adds a TODO comment; actual edit is Stage 5.
- `LICENSE` — currently GPL-3.0; Stage 5 replaces with MIT.
- Root `Cargo.toml` `license =` fields (if any) — Stage 5.

### Relicense (Stage 5) — HUMAN-GATED, separate commit on `main`

**This stage is blocked on a human legal decision and cannot proceed without
it.** It lands on `main` after `new-solver` is merged, not on `new-solver`
itself.

Removing slvs clears the only *external* GPL-3.0 *dependency* from the
shipped binary. The project's *own* code (kernel stack, engine, bridge) is
GPL-3.0 by the authors' choice (`README.md` line 3, `ARCHITECTURE.md` line
16, `LICENSE` file). Relicensing own-code to MIT is a separate decision that
requires amending protected files per Constitution §11.

**Decision (confirmed): relicense to MIT.** Stage 5 applies:

- `LICENSE` → MIT text.
- `README.md` lines 3, 15, 36 → MIT attribution; remove GPL-3.0 references.
- `ARCHITECTURE.md` lines 16, 209 → MIT; remove GPL-3.0 references.
- `CONTRIBUTING.md` → update license references if any.
- All `Cargo.toml` `license =` fields → `"MIT"`.
- `architecture.html` line 90 → MIT.
- `INTERFACES.md`, `STATUS.md`, project docs → update license references.
- Commit titled: "Amend Architecture: relicense own-code GPL-3.0 → MIT"
  with rationale + migration plan per Constitution §11.

## Invariants

1. **No slvs source consulted.** Implementation derives from papers +
   Nocedal-Wright + the algorithmic descriptions in
   `docs/SKETCH-SYSTEM-PLAN.md`. The `crates/slvs-patch/` tree is treated as a
   black-box binary oracle during the parity phase and deleted at cutover.
   This is the same clean-room discipline M7 applied to the LGPL Attene
   predicates (`crates/predicate-gen/`, `crates/cherchi-rs/CLAUDE.md:48`).
2. **Public API unchanged.** `solve_sketch(&Sketch) -> SolvedSketch` signature
   is preserved verbatim. No consumer of `sketch-solver` (wasm-bridge,
   test-harness) sees a type change.
3. **Determinism mandatory.** Same inputs → byte-identical outputs across
   runs and platforms (per `governance/ARCHITECTURAL_INVARIANTS.md`).
4. **No panics in production paths.** `solve_sketch` returns
   `SolveStatus::SolveFailed` on any internal failure; never `panic!` / `expect`
   / `unwrap` on data-dependent paths (corrects an existing violation in the
   current `entity_mapping.rs` / `constraint_mapping.rs`).
5. **Tests are permanent.** Per `AGENTS.md` rule 12, passing tests are never
   deleted. The parity-harness tests (Group 6) are deleted in the cutover PR
   only because their `libslvs-oracle` feature gate removes the dependency
   they exercise — and the cutover PR is the explicit "delete the oracle"
   step the harness was built to enable.
6. **Protected files deferred.** Edits to `ARCHITECTURE.md`, `/governance/*`,
   and `/agents/*` require explicit human approval per Constitution §11.
   PR-SS1d does not touch these; the relicense edits land in Stage 5 as a
   separate human-gated commit on `main`. `AGENTS.md` is in the `/agents/*`
   tree and is therefore protected — if it references slvs, the edit is
   deferred to Stage 5, not bundled into PR-SS1d.

## Deliberate deviations from libslvs behavior

These are acceptable divergences documented for the parity harness:

1. **Status classification on degenerate input.** libslvs returns specific
   `FailReason` variants (Inconsistent, DidntConverge, TooManyUnknowns) that
   don't map 1:1 to our `SolveStatus`. The parity harness asserts variant
   *class* (satisfiable vs not), not exact `FailReason` string. Documented in
   `status.rs`.
2. **Floating-point convergence path.** LM is not bit-identical to libslvs's
   dogleg/NR hybrid even with identical inputs. Position agreement is
   asserted at `1e-6` (user-visible precision), not bit-identical. This is the
   same standard the cherchi-rs sidecar uses for parity
   (`specs/indirect_predicates_sidecar_scaffold.md`).
3. **Constraint ordering.** libslvs sorts constraints internally; our residual
   order is determined by `SketchConstraint` enum declaration order + stable
   sort. Output positions are unaffected; only intermediate iteration paths
   differ.
4. **WhereDragged weight.** SolveSpace uses 1/20; we use 1/20. If parity
   reveals a different effective weight is needed, this is a tunable in
   `solver.rs`, not a contract change. Weight is fixed per solve (no
   per-iteration reweighting).
5. **Unsquared-length residual for `Equal(L_a, L_b)`.** We use `ℓ_a − ℓ_b`
   (with a `sqrt`). The spec originally proposed `ℓ²_a − ℓ²_b` to avoid `sqrt`,
   but the squared form amplifies position errors by a factor of ~2ℓ (≈120 for
   60mm lines), making the `SOLVE_TOL = 1e-6` tolerance unreachable. The
   `sqrt` singularity at zero-length lines is handled by a `dist > 1e-15`
   guard that zeroes the Jacobian for degenerate cases. Parity asserts
   position agreement at `1e-6`, matching the unsquared form's natural scale.

## References

### Algorithm references

See `REFERENCES.md` entries #40–#50 for full citations, access info, and local
copies. Summary:

- **#43** Moré (1978) — original LM paper. Local: none (paywalled at Springer
  LN); algorithm is in full in #44/#45.
- **#44** Nocedal & Wright — *Numerical Optimization* textbook, ch. 4 & 10.
  Local: `refs/nocedal_wright_numerical_optimization.pdf` (683 pages) +
  `refs/text/nocedal_wright_numerical_optimization.txt`.
- **#45** MINPACK — public-domain reference LM implementation. Local:
  `refs/minpack_lmdif_source.txt` (Fortran source of `lmdif`).
- **#40** Bouma et al. (1993) — foundational constructive GCS solver. Local:
  `refs/bouma1993_geometric_constraint_solver.pdf` +
  `refs/text/bouma1993_geometric_constraint_solver.txt`.
- **#41** Gao, Lin, Zhang (2006) — C-tree decomposition. Local:
  `refs/gao2006_ctree_decomposition.pdf` +
  `refs/text/gao2006_ctree_decomposition.txt`.
- **#42** Haller et al. (2010) — body-and-cad rigidity theory. Local:
  `refs/haller2010_body_and_cad.pdf` +
  `refs/text/haller2010_body_and_cad.txt`.
- **#47** Hoffmann-Lomonosov-Sitharam (2001) — cluster decomposition, both
  parts. Local: `refs/hoffmann2001_decomposition_plans_part1.pdf` +
  `refs/hoffmann2001_decomposition_plans_part2.pdf` (+ text extractions).
- **#48** Fudos-Hoffmann (1997) — graph-constructive GCS. Local:
  `refs/fudos_hoffmann1997_graph_constructive.pdf` +
  `refs/text/fudos_hoffmann1997_graph_constructive.txt`.
- **#49** Hoffmann-Joan-Arinyo (2005) — "A Brief on Constraint Solving" survey.
  Local: `refs/hoffmann2005_brief_on_constraint_solving.pdf` +
  `refs/text/hoffmann2005_brief_on_constraint_solving.txt`.
- **#50** Jermann-Trombettoni-Neveu-Mathis (2006) — decomposition survey.
  Local: `refs/trombetton2006_gcs_survey.pdf` +
  `refs/text/trombetton2006_gcs_survey.txt`.

### Dependency references

- **#46** `levenberg-marquardt` crate, v0.15.0, MIT/Apache-2.0:
  `github.com/rust-cv/levenberg-marquardt`. MINPACK port; reports
  floating-point-identical output to MINPACK on rank-deficient problems.
  Local: `refs/levenberg_marquardt_crate_readme.md`.
- `nalgebra` crate, BSD-3-Clause: `nalgebra.org`. Provides `ColPivQR` for
  rank-revealing decomposition used in status classification.
- MINPACK (the LM reference implementation): public domain (US government
  work). `netlib.org/minpack`. Local: `refs/minpack_lmdif_source.txt` +
  `refs/minpack_readme.txt`.

### Codebase references

- `crates/waffle-types/src/sketch.rs:169` — `SketchEntity` enum (the input
  contract).
- `crates/waffle-types/src/sketch.rs:236` — `SketchConstraint` enum (the 21
  variants; PR-SS1 implements 13).
- `crates/waffle-types/src/sketch.rs:330` — `SolveStatus` enum (the output
  contract).
- `crates/sketch-solver/src/solver.rs:10` — `solve_sketch` signature (preserved).
- `crates/wasm-bridge/src/dispatch.rs:48` — `SolveSketch` dispatch (simplifies).
- `crates/wasm-bridge/Cargo.toml:11` — `native-solver` feature (becomes the
  only path).
- `app/src/lib/engine/slvs-solver.js` — JS bridge (deleted at cutover).
- `app/src/lib/engine/worker.js:18` — slvs.wasm loader (deleted at cutover).
- `docs/SKETCH-SYSTEM-PLAN.md:48` — dragged-point 1/20-scaling description.
- `specs/indirect_predicates_sidecar_scaffold.md` — precedent for the
  dev-only-oracle-then-clean-room-cutover pattern (M7).
- `governance/ARCHITECTURAL_INVARIANTS.md` — determinism rule (A-* series).

### License survey

- SolveSpace / libslvs: GPL-3.0 — `solvespace.com`.
- FreeCAD PlaneGCS: LGPL-2.1+ — `github.com/FreeCAD/FreeCAD/blob/main/LICENSE`.
- NoteCAD: no LICENSE file, "contact for licensing" — proprietary.
- GeoSolver: GPL — `geosolver.sourceforge.net`.
- D-Cubed 2D DCM, LGS, C3D Solver: commercial / closed.
- No permissively-licensed 2D geometric constraint solver found in survey of
  Wikipedia's "Geometric constraint solving" implementations list or crates.io.
