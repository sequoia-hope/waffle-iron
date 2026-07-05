# Pinned Constraint — Explicit-Target Point Lock

Amends `specs/clean_room_constraint_solver.md`. Companion to
`specs/sketch_drag_stability.md` (increment 2 of the drag-stability cycle).

## 1. Goal

"Snapped to origin" (and reference-point snaps) must be a real lock: dragging
other geometry may never move a pinned point beyond solver tolerance, and the
pin's target must survive any number of solves.

### Reproduced defect

The UI stores an origin snap as JS `WhereDragged {point, x: 0, y: 0}` but the
bridge mapping (`store.svelte.js mapConstraintForBridge`) lowers it to Rust
`Dragged {point}` — **the (x, y) target is dropped**. `Dragged` compiles its
fixed position from the point's *current* entity coordinates
(`constraint_mapping.rs`), at weight 1/20 — the same weight as the live drag
pin. Net effect: the "lock" is a soft anchor that re-anchors to wherever the
point drifted after every solve. Probe-measured: dragging one corner of an
origin-centered equal-sided square walks the "locked" center to (23, −23)
over 120 pointermoves.

## 2. Parameters

| Name | Type | Description |
|---|---|---|
| `point` | `u32` | entity id of the pinned point |
| `x`, `y` | `f64` | pin target, sketch plane coordinates (meters, A14.1) |

Constraint weight: **1.0** (a pin is a real constraint, not an interaction
hint — unlike `Dragged`'s 1/20).

## 3. Branch table

| # | Condition | Behavior |
|---|---|---|
| B1 | `Pinned` present, no conflicting constraint | point solves to exactly (x, y) within `SOLVE_TOL` |
| B2 | `Pinned` + live `Dragged` on OTHER geometry pulling against it | pin wins: point sags ≤ `(w_drag/w_pin)²·offset` during the drag (bounded, non-accumulating), returns to (x, y) when the drag constraint is removed |
| B3 | `Pinned` + genuinely conflicting constraint (e.g. second `Pinned` elsewhere on same point) | standard `OverConstrained` classification — no silent override |
| B4 | JS mapping: `WhereDragged` with `_isDrag` flag | still lowers to `Dragged {point}` (live drag semantics unchanged) |
| B5 | JS mapping: `WhereDragged` without `_isDrag` (origin / reference-point snap pins) | lowers to `Pinned {point, x, y}` |
| B6 | FinishSketch persistence (added 2026-07-05) | persistent pins are SAVED into the feature as `Pinned {point,x,y}` (previously ALL WhereDragged were filtered out — pins silently vanished on re-edit, observed in the user's repro document). Transient `_isDrag` pins and targetless legacy entries are still dropped. Re-edit upconverts stored `Pinned` → in-session `WhereDragged {point,x,y}` so badges/snap/deletion operate on the single JS-side pin format; round-trips are idempotent (no duplication, no loss). Pre-`Pinned` documents load unchanged. |

## 4. Invariants

- I1 (**target is authoritative**): the solved position of a pinned point is
  (x, y) — even when its current entity coordinates differ (that is the exact
  failure of the `Dragged` lowering, which snapshots current coordinates).
- I2 (**no ratchet**): N successive solves with geometry dragged around leave
  the pinned point within a bounded envelope of (x, y); envelope does not
  grow with N.
- I3 (**release exactness**): after the temporary drag constraint is
  removed, the pinned point returns to (x, y) within `SOLVE_TOL`.
- I4 (**dof accounting**): `Pinned` contributes 2 residual rows / removes 2
  dof, like `Coincident` against a fixed location.

## 5. Oracles

- Solver unit tests: residual = 0 iff point at target; Jacobian = identity
  rows on (px, py).
- Regression test (drag loop, production `solve_sketch`): origin-pinned
  shared center of the two-rectangle fixture; 120-step corner drag →
  max center excursion < 1mm-equivalent AND final release solve returns
  center to (0,0) within `SOLVE_TOL` (this fails under the `Dragged`
  lowering: unbounded 33mm ratchet).
- Canonical: single point + `Pinned(5, 7)` from a different start →
  `FullyConstrained`, position (5, 7).
- Edge: `Pinned` + `Coincident` to a point pinned elsewhere →
  `OverConstrained`, conflicts reported.

## 6. Failure modes

- Unknown point id → compile error string (existing `SolveFailed` path).
- Non-finite target: rejected at compile with `SolveFailed` (defensive; the
  UI never produces one).
- Old documents: none contain `Pinned` (additive tagged-enum variant; file
  round-trip unaffected). JS documents storing `WhereDragged` are unchanged
  on disk — only the bridge lowering changes.

## 7. Research basis

- **#40 Bouma et al. 1993** — constraint model includes fixed/ground
  incidences; a pin is incidence-to-ground, a first-class constraint (weight
  1.0), distinct from the drag interaction hint (parent spec §dragged, 1/20
  weighting per SolveSpace prior art).
- Residual form is the `Dragged` primitive with an explicit target
  (parent spec §Dragged residual table) — no new mathematics.

## 8. Non-goals

- Hard elimination of pinned parameters from the parameter vector (exact
  zero sag during drags). Deferred; the soft weight-1.0 pin meets the UX
  requirement (sag `(1/20)² = 0.25%` of drag offset, invisible at drag
  scale). Revisit if users report visible pin sag.
