# 05 — Sketch UI: Agent Instructions

You are working on the sketch UI, `app/src/lib/sketch/`. Read ARCHITECTURE.md
in this directory first.

## Rules

1. **Talk to the engine through the store and the bridge.** The UI never
   solves; it sends `SolveSketch`, `ApplySketchOps` and `QuerySketch`.
2. **Geometry decisions belong in Rust.** Trim, extend, offset, fillet,
   mirror and projection are `sketch_solver::ops`. The UI handles pointer
   input, snapping and drawing.
3. **Every constraint goes through `mapConstraintForBridge`.** An unmapped
   UI-only spelling breaks every later solve in the sketch.
4. **A cancelled gesture leaves nothing behind.**
5. **Units are meters.** A 100 px line is about 0.005. Assert relative
   tolerances in tests.
6. **`tools.js` is a plain module.** Reactive state it shares with components
   lives in `sketchToolState.svelte.js`.

## Tests

GUI tests are in `app/tests/gui/`. The rules in the root `CLAUDE.md` ("GUI
Test Rules") apply; the ones that matter most here:

- Drawing tests use real pointer events (`drawLine()`, `dragLine()`), never
  `__waffle.addSketchEntity()`.
- Every drawing mode has both a click-click and a click-drag test.
- Assert that geometry satisfies a constraint, not only that the constraint
  was recorded.
- Run `sketch-drawing-regression.spec.js` before every commit that touches
  sketch code.

## Dependencies

The store and bridge (`app/src/lib/engine/`), the viewport
(`app/src/lib/viewport/`), `three`.
