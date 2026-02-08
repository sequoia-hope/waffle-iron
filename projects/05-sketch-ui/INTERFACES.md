# 05 — Sketch UI: Interfaces

## Types This Crate CONSUMES

| Type | Source | Purpose |
|------|--------|---------|
| `SolvedSketch` | sketch-solver (via wasm-bridge) | Solved positions for display |
| `SolveStatus` | sketch-solver (via wasm-bridge) | Constraint status for visual feedback |
| `ClosedProfile` | sketch-solver (via wasm-bridge) | Profile identification for selection |
| `GeomRef` | INTERFACES.md | Face reference for sketch plane |
| `EngineToUi::SketchSolved` | wasm-bridge | Solver results message |

## Types/Events This Crate PRODUCES

| Type | Consumer | Purpose |
|-------|----------|---------|
| `UiToEngine::BeginSketch` | wasm-bridge → engine | Enter sketch mode on a face |
| `UiToEngine::AddSketchEntity` | wasm-bridge → engine | Add drawn entity to sketch |
| `UiToEngine::AddConstraint` | wasm-bridge → engine | Apply constraint |
| `UiToEngine::SolveSketch` | wasm-bridge → solver | Request re-solve |
| `UiToEngine::FinishSketch` | wasm-bridge → engine | Commit sketch as feature |
| Profile selection (GeomRef) | modeling-ops (via ui-chrome) | Selected profile for extrusion |

## Svelte Component API

```svelte
<SketchEditor
  solvedSketch={solvedSketch}
  solveStatus={solveStatus}
  activeTool={currentDrawingTool}
  on:addEntity={handleAddEntity}
  on:addConstraint={handleAddConstraint}
  on:selectProfile={handleProfileSelection}
  on:finishSketch={handleFinish}
/>
```

## Notes

- This is a Svelte component overlaid on the 3d-viewport during sketch mode.
- All communication with the solver is via wasm-bridge messages — never call solver directly.
- Solved positions arrive as JSON objects (HashMap<u32, [f64, f64]>).
