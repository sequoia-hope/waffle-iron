# Sketch Drag Stability — Proximal Regularization + Failed-Solve Position Policy

Amends `specs/clean_room_constraint_solver.md` (the parent solver spec).
Bug-fix cycle per FIP §8; reproduction and mechanism documented below.

## 1. Goal

Interactive dragging must never explode sketch geometry. Solving an
under-constrained sketch must return the solution **nearest to the current
configuration**, and a solve that fails must not replace the user's geometry
with the solver's diverged iterate.

### Reproduced defect (2026-07-04)

Fixture: two origin-centered centerpoint rectangles (15mm/20mm), `Equal` on
two adjacent inner edges, inner corner dragged (temporary `Dragged` pin per
pointermove, per parent spec §Dragged-point handling). Observed: coordinates
grow 10mm → 4×10⁸ within two pointermoves.

Mechanism (probe-verified): the second rectangle's width/height are free DOF
— exactly flat directions of the weighted least-squares cost. During a solve
whose drag target is momentarily inconsistent, LM takes huge **accepted,
cost-decreasing** steps along the near-null valley (nearly all residuals are
linear) and terminates `Converged{ftol}` with the free rectangle at ±6×10⁴,
inside-out. `solve_sketch` then returns the final iterate's positions
unconditionally. Nothing in the cost prefers the nearby solution.

## 2. Parameters

| Name | Value | Units | Rationale |
|---|---|---|---|
| `PROXIMAL_WEIGHT` | `1e-5` | 1/length (weight on residual rows `ε·(xᵢ−x₀ᵢ)`) | The proximal pull biases a w-weighted anchor by `(ε/w)²·D` (D = correction distance); the binding case is the weakest anchor, `Dragged` at w=1/20. ε=1e-5 → bias `4e-8·D`, below `SOLVE_TOL=1e-6` for D up to 25 length units (units are meters, A14.1 — far beyond any real sketch correction). ε=1e-4 was tried first and measurably displaced Dragged anchors (2e-5 at D=5; 9 pre-existing suite tests caught it — tests were NOT weakened, the parameter was re-derived). Empirical sweep (ε ∈ [1e-6, 1e-2], mm & m scale): all values suppress the runaway; 1e-5 keeps ≥1 decade of margin above the validated floor for *near*-null valleys. |

No new user-facing inputs. No configuration surface.

## 3. Branch table

| # | Condition | Behavior |
|---|---|---|
| B1 | any solve, `n_params > 0` | proximal rows appended to the LM problem, anchored at the pre-solve parameter vector `x₀` |
| B2 | status classification | computed from **constraint rows only** (residual slice + Jacobian row slice `0..n_constraint_rows`); proximal rows excluded |
| B3 | classified `SolveFailed` | returned `positions`/`radii` are the **initial** (input) values, not the LM iterate |
| B4 | classified anything else | returned positions are the LM iterate (unchanged behavior) |

There is no mode/toggle: proximal regularization is unconditional (Constitution
§7 — no branch where none is needed).

## 4. Invariants

- I1 (**boundedness**): for the reproduction fixture driven through a
  120-step simulated drag loop (solve output fed back as next input, as the
  UI does), every solved coordinate stays within 10× the drag path's own
  extent. No coordinate is non-finite.
- I2 (**nearest solution**): a satisfiable under-constrained solve moves
  entities not coupled to the violated constraints by at most solver
  tolerance (the free rectangle must not move when the other rectangle's
  corner is dragged and the drag target is consistent).
- I3 (**classification unchanged**): `FullyConstrained` / `UnderConstrained
  { dof }` / `OverConstrained` / `SolveFailed` classification is identical to
  the pre-change solver for satisfiable systems: dof counting uses the
  constraint-only Jacobian; `‖r‖∞` uses constraint rows only.
- I4 (**failed solve is inert**): when status is `SolveFailed`, output
  positions == input positions exactly.
- I5 (**exact solutions still exact**): fully-constrained canonical cases
  (parent spec Group 4) solve to the same positions within `SOLVE_TOL`.

## 5. Oracles

- Drag-loop regression test (`tests/drag_stability_tests.rs`): production
  `solve_sketch` in the UI feedback loop on the two-rectangle fixture, mm and
  meter scale → asserts I1 numerically (max coordinate bound) and finiteness.
- Free-body isolation test: consistent single-step drag → outer rectangle
  corner displacement < 1e-6 (I2).
- Existing suite (`solve_tests.rs`, unit tests in `constraint_mapping.rs` /
  `solver.rs`) → I3, I5. No test may be weakened.
- `SolveFailed` case with impossible constraints → positions echo input (I4).

## 6. Failure modes

- Unsatisfiable constraint sets: unchanged — `OverConstrained` /
  `SolveFailed` classification, but with B3 the geometry no longer jumps.
- Pathological scale (coordinates ≫ 1): proximal bias grows with correction
  distance; at 1e2 units the classification bias reaches ~1e-6·D. Accepted:
  sketch coordinates are meters (A14.2 feature floor 1e-6 m, practical
  sketches ≪ 1e2 m).

## 7. Research basis

- **#40 Bouma et al. 1993** — solution-redirecting: among the exponentially
  many solutions, return the one "intuitive to an untrained user", i.e. the
  solution *nearest the current configuration*. Proximal anchoring is the
  least-squares realization of that selection rule.
- **#43 Moré 1978 / #44 Nocedal & Wright ch. 10** — Levenberg-Marquardt
  damping is Tikhonov regularization of the *step*; it does not regularize
  the *problem*: along exact null directions of J the cost is flat and the
  iterate can drift unboundedly. Adding `ε·(x−x₀)` rows makes the
  Gauss-Newton system full-rank (ridge regression), selecting the minimum-
  distance solution.
- **#46 rust-cv levenberg-marquardt (MINPACK port)** — the solver in use;
  step acceptance requires only `ratio ≥ 1e-4`, so cost-decreasing null-space
  drift is accepted by design. Prior art: SolveSpace solves for minimal
  deviation from the current sketch (parent spec already adopts its 1/20 drag
  weighting).

## 8. Non-goals (this increment)

- Origin-pin semantics (`WhereDragged` target dropped at the bridge) —
  separate increment, `specs/pinned_constraint.md`.
- UI-side application policy for failed solves and viewport/undo — UI
  increments 3–4 (store guard, camera restore).
