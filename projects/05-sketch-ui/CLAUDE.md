# 05 — Sketch UI: Agent Instructions

You are working on **sketch-ui**. Read ARCHITECTURE.md in this directory first.

## Your Job

Build the 2D sketch editing interface as a Svelte component. This overlays on the 3D viewport during sketch mode. Users draw geometry (lines, rectangles, circles, arcs), apply constraints, and select profiles for extrusion.

## Critical Rules

1. **Communicate via wasm-bridge only.** Send UiToEngine messages to add entities/constraints and trigger re-solves. Receive SolvedSketch results. Never call the sketch solver directly.
2. **Auto-constraining is critical for UX.** Onshape does this aggressively — snap to horizontal/vertical/coincident/tangent when the user draws near these conditions. Without this, sketching is painful.
3. **The Dragged constraint enables interactive drag.** When user drags a point: set position → add Dragged constraint → solve → read result → remove Dragged constraint. This is how unconstrained geometry moves.
4. **Color by constraint status.** Green = fully constrained, blue = under-constrained, red = over-constrained. Users rely on this visual feedback.
5. **Dimension labels are editable.** Click a distance label → type new value → re-solve. This is core to parametric design.

## Key Files

- `src/lib/SketchEditor.svelte` — Main sketch editing component
- `src/lib/tools/LineTool.svelte` — Line drawing tool
- `src/lib/tools/RectangleTool.svelte` — Rectangle drawing tool
- `src/lib/tools/CircleTool.svelte` — Circle drawing tool
- `src/lib/tools/ArcTool.svelte` — Arc drawing tool
- `src/lib/constraints/ConstraintMenu.svelte` — Constraint application UI
- `src/lib/constraints/DimensionLabel.svelte` — Editable dimension display
- `src/lib/SnapDetector.ts` — Auto-constraining logic
- `src/lib/ProfileSelector.svelte` — Closed loop selection

## Dependencies

- 3d-viewport (sketch mode transitions, plane overlay)
- wasm-bridge (communication with solver)
- No direct Rust/WASM dependencies
