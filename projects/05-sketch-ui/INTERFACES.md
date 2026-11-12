# 05 — Sketch UI: Interfaces

## Messages sent (via the store)

| Message | Purpose |
|---|---|
| `BeginSketch { plane }` | Enter sketch mode on a plane or face |
| `AddSketchEntity { entity }` | Add an entity to the engine's active sketch |
| `SolveSketch { entities, constraints }` | Replace the engine's sketch state and solve |
| `ApplySketchOps { ops }` | Apply `SketchOp` values (trim, fillet, offset, …) |
| `QuerySketch { … }` | Read-only preview of an operation, chain discovery |
| `FinishSketch { … }` | Commit the sketch as a feature, with solved profiles and positions |

## Data received

`SolvedSketch`: `positions` (point id → `[x, y]`), `radii` (circle id →
radius), `profiles`, `status` (`FullyConstrained`, `UnderConstrained { dof }`,
`OverConstrained { conflicts }`, `SolveFailed { reason }`). `conflicts` index
the constraint list that was sent.

## Components

`SketchInteraction`, `SketchRenderer`, `InactiveSketchRenderer` and
`DimensionLabels` are mounted in the viewport's scene; `ConstraintMenu`,
`ConstraintModal` and `DimensionInput` are HTML overlays on the viewport;
`SketchConstraintList` is mounted by the Properties panel. All read the store
directly.

## Shared with the agent tools

`finishProfiles.js` builds the `FinishSketch` profile payload for the UI. The
agent path builds the same payload in Rust
(`waffle_types::profiles::build_finish_profiles`).
