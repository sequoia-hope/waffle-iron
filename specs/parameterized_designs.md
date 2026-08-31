# Parameterized Designs — Design Variables & Expression-Driven Measurements

Status: **LANDED** (2026-08-31). This spec documents the shipped design.

## 1. What it is

The user defines named **variables** in the feature tree ("Variables" panel)
and assigns **measurements** to expressions over them:

- Sketch dimensions: `Distance`, `PointLineDistance`, `HDistance`,
  `VDistance`, `Angle`, `Radius`, `Diameter`
- Extrude depth, revolve angle, datum-plane offset distance

Editing a variable rebuilds the model: every expression re-evaluates,
affected sketches re-solve, and downstream features regenerate.

## 2. Data model

`FeatureTree.parameters: Vec<DesignParameter>` (feature-engine `types.rs`):

```rust
pub struct DesignParameter {
    pub id: Uuid,          // stable identity (error routing, undo)
    pub name: String,      // [A-Za-z_][A-Za-z0-9_]*, not reserved
    pub expression: String,
    pub value: f64,        // cached last-good evaluation (mm-space)
    pub error: Option<String>,
}
```

Measurement expressions are **optional fields next to the numeric value they
drive**; the numeric field always holds the last evaluated result, so the
kernel, old readers, and every existing consumer see a plain number:

- `SketchConstraint::{Distance,…}::expression: Option<String>` (waffle-types)
- `ExtrudeParams::depth_expr`, `RevolveParams::angle_expr`,
  `PlaneDefinition::{Offset,OffsetFromFace}::distance_expr`

The same pass added `reference: bool` to the seven dimension constraints:
the UI's driven-dimension flag previously lived only in JS and was silently
dropped on save/re-edit; the engine-side re-solve needs it (reference dims
must never constrain), so it is now persisted.

All new fields are serde-defaulted and skipped when empty ⇒ purely additive;
old `.waffle` files load unchanged and untouched documents serialize
byte-identically (no `MIN_READER_VERSION` bump — see file-format `save.rs`).

## 3. Expression language (feature-engine `expr.rs`)

- Operators `+ - * / % ^` (`^` right-assoc, binds tighter than unary minus),
  parentheses, `pi`, functions `sqrt abs floor ceil round min max` and
  `sin cos tan` (**degrees**).
- Identifiers resolve against the parameter table (any definition order;
  cycles/dupes/bad names are per-parameter errors).
- **mm-space convention:** an expression evaluates to a plain number read as
  MILLIMETERS in length contexts and DEGREES in angle contexts. Literals may
  carry a unit suffix (`25mm`, `1.5in`, `2 cm`, `90deg`) that scales into
  mm-space. Deliberately independent of the document *display* unit: switching
  mm↔in must never rescale expression-driven geometry. (FreeCAD's rule.)
- All failures are typed and loud; results must be finite.

## 4. The apply pass (feature-engine `params.rs`)

`Engine::rebuild` runs `apply_parameters(&mut tree)` FIRST on every rebuild:

1. Evaluate the parameter table (fixpoint iteration; per-parameter
   `value`/`error` caches refreshed; last-good value kept on error).
2. For every feature, re-evaluate its expressions into the numeric fields
   (compare-and-set: unchanged values touch nothing, so the pass is
   idempotent and incremental rebuilds stay incremental).
3. A sketch whose dimension values changed is **re-solved** from its current
   geometry (`sketch_solver::solve_sketch`, driving constraints only), the
   solution written back into entities, and derived data recomputed via
   `Sketch::recompute_derived()` — the proven projected-sketch pattern.
   A failed re-solve restores the previous dimension values (geometry and
   labels stay consistent) and raises a per-feature error.
4. The rebuild start index widens to the earliest changed feature.

Parameter errors surface ahead of rebuild errors in `Engine::errors`
(toasts in the app). `Engine::set_parameters` replaces the whole table,
pushes `Command::SetParameters` (undo/redo restores the table and rebuilds
from 0), and rebuilds.

## 5. Bridge & UI

Messages: `SetParameters { parameters }` → `ModelUpdated` (tree carries
evaluated values/errors); stateless `EvaluateExpression { expression }` →
`ExpressionEvaluated { value, error }` for live validation/preview
(evaluated against `params::cached_env`, matching what the next rebuild
computes).

UI surfaces:

- **FeatureTree.svelte**: Variables panel (add/edit/delete rows,
  `name = expression → value`, error badge). Sends the complete list.
- **Dimension input & labels**: input that is not a plain
  number-with-unit (`units.isPlainMeasurement`) is treated as an expression:
  engine-evaluated, stored on the constraint, label shows a `ƒ` prefix.
  A plain numeric edit **detaches** the expression. Diameter constraints are
  edited as radii, so a radius expression is stored wrapped as `2*(…)` (and
  unwrapped for re-editing).
- **Extrude / Revolve / SketchPlane dialogs**: depth/angle/offset accept
  expressions with a live `= value` hint (engine round trip); invalid
  expressions block Apply with a toast. The revolve angle became a text
  input — non-positive angles are rejected at apply instead of by HTML
  min/max attributes.

## 6. Known limitations (v1)

- Sub-region (`region`/`regions`) extrudes capture explicit 2D boundaries at
  dialog time; a variable-driven sketch change does not re-derive them (same
  as any dim edit today). Whole-profile extrudes follow fully.
- The first expression-driven re-solve of a sketch switches its
  `solved_profiles` from the JS extraction to the Rust `recompute_derived`
  extraction (identical for line/circle/arc/gear cases; both already feed
  extrude via the projected-sketch path). Profile ORDER is deterministic per
  extractor; a mismatch surfaces as a loud `ProfileOutOfRange`.
- Second-direction extrude depth (`SecondDirection::Blind`) and
  fillet/chamfer/shell (deferred ops) take no expressions.
- Trig is degrees-only; `sin(pi)` is sin of π DEGREES.

## 7. Tests

- `feature-engine`: `expr.rs` unit tests; `params.rs` table/apply/re-solve
  tests (incl. reference-dim exclusion, failed-solve restore, idempotence);
  `tests/parameters.rs` engine-level MockKernel flow (set → rebuild → undo).
- `wasm-bridge/tests/bridge_tests.rs`: message round trips.
- `file-format/tests/format_tests.rs`: persistence + old-file compat.
- `waffle-types`: serde round-trip/back-compat for the constraint fields.
- GUI `app/tests/gui/parameterized-designs.spec.js`: panel CRUD, chained
  variables, error rows, expression extrude + variable edit rebuild,
  expression sketch dimension + re-solve on rebuild.
