# Spec: Point-pair Horizontal / Vertical constraints

> STATUS: IN PROGRESS — foundation for the center-rectangle tool
> (`center_rectangle.md`). Extends the existing `Horizontal`/`Vertical`
> constraints, which today act only on a single line, to also accept an
> arbitrary pair of points.

## Goal

Today `Horizontal` and `Vertical` constrain a **line's two endpoints** to share
a Y (horizontal) or X (vertical) coordinate. Users frequently want the same
relation between two points that are **not** the endpoints of one line — e.g.
"this point is directly above that one" (vertical alignment) or "level with"
(horizontal alignment). This spec adds that capability.

The compiled constraint is *identical* to the line case (equate two parameter
indices), so this is a pure front-of-pipeline extension: a new way to name the
same residual.

## Concepts & Data Model

`SketchConstraint` (`crates/waffle-types/src/sketch.rs`) gains two **additive**
variants — chosen over making `Horizontal { entity }` polymorphic so that
existing serialized `.waffle` files and all existing constructors remain
byte-for-byte unchanged (serde is internally tagged on `type`; a variant must
have one fixed shape). This mirrors the existing precedent of
`Symmetric` / `SymmetricH` / `SymmetricV` being distinct variants for the same
concept at different arities.

```rust
/// Two points share a Y coordinate (a horizontal line passes through both).
HorizontalPoints { point_a: u32, point_b: u32 },
/// Two points share an X coordinate (a vertical line passes through both).
VerticalPoints { point_a: u32, point_b: u32 },
```

### Compilation (`crates/sketch-solver/src/constraint_mapping.rs`)

Both compile to the **existing** `CompiledConstraint`:

- `HorizontalPoints { a, b }` → `CompiledConstraint::Horizontal { ay, by }`
  where `ay`, `by` are the Y-param indices of points `a`, `b`.
- `VerticalPoints { a, b }` → `CompiledConstraint::Vertical { ax, bx }`
  where `ax`, `bx` are the X-param indices.

No new `CompiledConstraint` variant, no new residual/jacobian, no change to
`residual_count` (already `_ => 1`). This reuse is the proof that the constraint
is genuinely the same one.

### JS / bridge

- `mapConstraintForBridge` already passes unknown constraint shapes through
  unchanged; dispatch (`AddConstraint`) deserializes generically. No bridge
  change.
- `constraintLogic.js`: in the **2-points** applicability branch, expose
  `horizontal → { type:'HorizontalPoints', point_a, point_b }` and
  `vertical → { type:'VerticalPoints', point_a, point_b }`. (The line case keeps
  emitting `{ type:'Horizontal', entity }`.)
- `constraintBadges.js`: render an `H` / `V` badge at the midpoint of the two
  points for the new variants.

## Parameters

| Constraint | Inputs | Units | Valid range | Error condition |
|---|---|---|---|---|
| `HorizontalPoints` | `point_a:u32`, `point_b:u32` | entity ids | must reference existing **points** | unknown point id → `compile` returns `Err` → `SolveFailed` |
| `VerticalPoints` | `point_a:u32`, `point_b:u32` | entity ids | must reference existing **points** | unknown point id → `compile` returns `Err` |

`point_a == point_b` is degenerate-but-harmless (residual ≡ 0); not an error.

## Branch Table

| Variant | Operand | Compiled to | Residual |
|---|---|---|---|
| `Horizontal { entity }` (existing) | line | `Horizontal { ay, by }` | `y_end - y_start = 0` |
| `Vertical { entity }` (existing) | line | `Vertical { ax, bx }` | `x_end - x_start = 0` |
| `HorizontalPoints { a, b }` (new) | 2 points | `Horizontal { ay, by }` | `y_b - y_a = 0` |
| `VerticalPoints { a, b }` (new) | 2 points | `Vertical { ax, bx }` | `x_b - x_a = 0` |

## Invariants

- **I1 (horizontal):** After solving with `HorizontalPoints{a,b}`, `y_a == y_b`
  (within solver tolerance).
- **I2 (vertical):** After solving with `VerticalPoints{a,b}`, `x_a == x_b`.
- **I3 (orthogonal DOF):** The constraint touches exactly one coordinate axis —
  `HorizontalPoints` does not alter either point's X; `VerticalPoints` does not
  alter either point's Y. (One residual row, one axis.)
- **I4 (serialization round-trip):** A sketch containing the new variants
  round-trips through `file-format` serialize→deserialize unchanged, and an
  old file with `Horizontal { entity }` still loads.

## Oracles

- Solver test: place two points at distinct (x,y); add `HorizontalPoints`; solve;
  assert `|y_a - y_b| < 1e-6` AND both X coordinates unchanged (I1, I3).
- Solver test: symmetric for `VerticalPoints` (I2, I3).
- Solver test: an unknown point id yields `SolveStatus::SolveFailed`.
- file-format test: serialize a sketch with both new variants, deserialize,
  assert structural equality (I4).
- Mutation check: swapping `HorizontalPoints`↔`VerticalPoints` in a test must
  flip which axis is equated (guards against the two arms being transposed).

## Failure Modes

- Unknown/non-point entity id → `compile` `Err(String)` → `SolveStatus::SolveFailed { reason }` (loud, surfaced as toast). No silent skip.
- Over-constraint (e.g. both `HorizontalPoints` and a `Distance` forcing a
  conflict) is handled by the existing LM solver / conflict detection — no new
  behavior.

## Research Basis

Standard geometric-constraint-solver relation (axis-alignment of two points);
the solver itself is Levenberg–Marquardt over residuals (nalgebra), consistent
with the existing constraint set. No new algorithm. This is a data-model
extension that reuses the existing compiled residual; no published technique is
implicated beyond the existing solver design.

### Analytical vs. Approximate Method Justification

- **Method:** Exact (algebraic equality residual). Not surface-surface
  intersection — A15 SSI rules do not apply.
