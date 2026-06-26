# Spec: Constraint Modal (constraint-first application)

## Goal

Today constraints are **selection-first**: the user selects geometry, then picks
a constraint that fits the selection (right-click `ConstraintMenu` or the
Constraints toolbar dropdown which applies to the *current* selection).

This feature adds a **constraint-first modal** flow:

1. The user picks a constraint type (e.g. Coincident) from the Constraints
   palette. This opens the constraint modal and keeps select-mode picking on.
2. The user clicks sketch geometry one piece at a time. As soon as the running
   selection is sufficient for the active constraint, the constraint is applied
   and the sketch re-solves.
3. **Continued** selection keeps applying the constraint to the newly picked
   geometry (chaining), so the user can coincident-weld a whole row of points
   without re-choosing the tool.
4. A pick that cannot take the active constraint is **ignored** with a transient
   hint; the modal stays open and the already-valid selection is preserved.
5. The user ends the modal (Done button / Escape / choosing another tool).
   Everything deselects and they may start a new constraint.

This is the keystone for two downstream features: projected-geometry coincidence
and the dimension tool, which reuse the same pick-loop UX.

## Parameters (inputs)

- `constraintId` — the active constraint type. One of the modal-supported ids
  (see Branch Table). Set when the modal opens; immutable for the modal's life.
- `pickId` — the sketch entity id clicked in the viewport (point/line/circle/arc),
  or null on empty-space clicks.
- `entities`, `positions` — the current sketch entities and solved positions,
  used by the existing `getApplicableConstraints()` builders to construct the
  concrete `SketchConstraint`. The modal adds **no new** constraint math; it only
  decides *which* entities to feed to the existing builders and *when*.

## Branch Table

Each supported constraint has a **mode** that defines how running picks turn into
applied constraints. `accepts(kind)` gates which entity kinds are valid picks.

| constraintId   | mode      | accepts                     | apply rule |
|----------------|-----------|-----------------------------|-----------|
| horizontal     | unary     | Line                        | apply to each picked line immediately |
| vertical       | unary     | Line                        | apply to each picked line immediately |
| fix            | unary     | Point                       | apply to each picked point immediately |
| coincident     | chain     | Point                       | apply(prev, pick); anchor advances to pick |
| parallel       | chain     | Line                        | apply(prev, pick); anchor advances |
| perpendicular  | chain     | Line                        | apply(prev, pick); anchor advances |
| equal          | chain     | Line\|Circle\|Arc           | apply(prev, pick) if same family; else reject |
| symmetricH     | chain     | Point                       | apply(prev, pick); anchor advances |
| symmetricV     | chain     | Point                       | apply(prev, pick); anchor advances |
| tangent        | rolePair  | roleA=Line, roleB=Circle\|Arc | apply when both roles filled; reset |
| midpoint       | rolePair  | roleA=Point, roleB=Line     | apply when both roles filled; reset |
| pointOnLine | rolePair  | roleA=Point, roleB=Line\|Circle\|Arc | apply when both roles filled; reset |

**Modes**

- **unary**: every accepted pick applies the constraint to that single entity.
  `running` is always empty between picks.
- **chain**: the first accepted pick becomes the `anchor` (running = [anchor]).
  Each subsequent accepted pick applies `constraint(anchor, pick)` and advances
  `anchor := pick`, giving transitive closure (all picks coincident / parallel /
  …). A pick equal to the current anchor is ignored.
- **rolePair**: collect one entity for each of two distinct roles. When both are
  filled, apply and reset both roles so the next two picks form a new pair.

The concrete `SketchConstraint` for an apply is built by feeding the chosen
entity subset to the **existing** `getApplicableConstraints()` and reading the
first non-null builder among the constraint's candidate keys (e.g. `pointOn`
resolves to `pointOnLine` or `pointOnCircle` by the curve kind; `equal` resolves
to `equal` or `equalRadius`). If no candidate builder is non-null for the subset
(e.g. `equal` of a line and a circle), the pick is **rejected**.

**Step output** — `stepConstraintModal({constraintId, running, pickId, entities, positions})`
returns `{ action, constraints, nextRunning, message }` where:
- `action` ∈ `apply` | `collect` | `reject`.
- `constraints` — zero or more `SketchConstraint` objects to add (apply only).
- `nextRunning` — the running entity-id list after this pick.
- `message` — transient hint, set on `reject`/`collect`, else null.

## Invariants

- **No phantom constraints**: `action==='apply'` ⇒ `constraints.length ≥ 1`, and
  every returned constraint is exactly what `getApplicableConstraints()` builds
  for the chosen subset (modal adds no new constraint types or math).
- **Reject is inert**: `action==='reject'` ⇒ `constraints` is empty and
  `nextRunning === running` (selection preserved, nothing applied).
- **Self-pick inert**: picking an id already serving as the chain anchor (or a
  filled role) yields `collect`/`reject` with no new constraint.
- **Chain transitivity**: after picking points p1,p2,p3 under `coincident`, the
  applied constraints are Coincident(p1,p2) and Coincident(p2,p3) — the solver
  then drives all three to one position.
- **Determinism**: identical (constraintId, running, pickId, entities, positions)
  always yields identical output (no time/random).

## Oracles

- **Unit (pure engine, exercised in-browser via `__waffle`)**: feed scripted pick
  sequences to `stepConstraintModal` and assert `action`, the emitted constraint
  `type`/ids, and `nextRunning` for: a unary apply, a chain apply (2nd pick), a
  chain self-pick (inert), a rolePair completion, and an incompatible reject.
- **GUI end-to-end**: open the Coincident modal, click two distinct points →
  assert exactly one new `Coincident` constraint exists and the two points'
  solved positions converge (distance ≈ 0). Click a third point → assert a second
  Coincident exists and all three converge. Click a line (incompatible) → assert
  constraint count unchanged and a hint message is shown. Press Escape → assert
  the modal is closed and selection cleared.

## Failure Modes

- **Empty-space / no-entity click**: `pickId == null` → no-op (`collect`, message
  null), modal stays open.
- **Incompatible pick** (wrong kind, or a chain across incompatible families):
  `reject`, transient hint, selection preserved.
- **Stale entity id** (entity deleted mid-modal): builder returns null → treated
  as `reject`; modal stays usable.
- **Switching tools** while the modal is open closes the modal and clears the
  running selection (handled at the tool-mode boundary, not in the pure engine).

## Research Basis

No external geometry/algorithm reference — this is interaction logic over the
existing constraint solver (`sketch-solver`, Levenberg–Marquardt). It introduces
**no** new `SketchConstraint` variants and reuses `getApplicableConstraints()`
(the shared builder used by `ConstraintMenu` and the Toolbar). The UX pattern
(constraint-first, chained application with transient validation feedback)
mirrors mainstream parametric CAD (Onshape/SolidWorks constraint tools). Per the
user decision of record, invalid picks are ignored with a flashed hint rather
than gated behind an explicit accept/cancel.
