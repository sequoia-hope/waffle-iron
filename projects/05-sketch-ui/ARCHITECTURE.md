# 05 — Sketch UI: Architecture

The sketch UI is the JavaScript in `app/src/lib/sketch/`. It handles pointer
input on the sketch plane, runs the drawing tools, proposes snaps, and draws
sketch geometry, constraints and dimensions. The engine owns the sketch and
solves it; the UI holds a working copy in the store
(`app/src/lib/engine/store.svelte.js`) and resends it for every solve.

Sketch coordinates are meters in the plane's 2D frame.

## Sketch mode

1. The user picks a datum plane or a planar face and starts a sketch. The
   store sends `BeginSketch` with the plane.
2. The viewport draws solids at 20 % opacity, shows the sketch plane grid, and
   aligns the camera to the plane.
3. Each edit updates the store's entity and constraint lists and calls
   `triggerSolve()`, which sends `SolveSketch` with the full lists. At most
   one solve is in flight; while it is, only the newest request is kept.
4. The reply carries solved positions, radii, profiles and the solve status.
   The store applies it unless the solve failed or returned non-finite values.
5. Finish Sketch sends `FinishSketch` with the profile payload built by
   `finishProfiles.js`. Editing an existing sketch feature reopens the same
   flow on its stored entities.

Cancelling a tool mid-gesture (Escape, switching tools, right-click) leaves
nothing behind: no point, no constraint, no undo entry.

## Pointer path

`SketchInteraction.svelte` captures pointer events on the sketch plane,
converts screen → world → sketch coordinates (`sketchCoords.js`), and calls
`handleToolEvent` in `tools.js` with the active tool. Every drawing tool works
both click-click and click-drag.

## Tools (`tools.js`)

| Tool | Result |
|---|---|
| `select` | Select, drag points, pick constraint badges, select profiles |
| `point`, `line`, `polyline` | Points and lines; line and polyline chain |
| `rectangle`, `rectangle-center` | Four lines with coincident, horizontal and vertical constraints |
| `circle`, `arc`, `slot` | Circles, three-point-entity arcs, slots |
| `gear`, `sprocket`, `planetary` | Generated profiles |
| `dimension` | Places a driving dimension (`dimensionHeuristic.js` chooses what is measured) |
| `constraint` | Constraint-first pick loop (`constraintModalEngine.js`, `ConstraintModal.svelte`) |
| `project` | Projects body vertices, edges and faces onto the sketch plane (`projectGeometry.js`, `bodyChain.js`) |
| `offset` | Offsets a chain of sketch or body edges |
| `trim`, `sketch-fillet` | Trim at intersections; round a corner |

Trim, fillet and offset do not compute geometry in JS. They send `QuerySketch`
for a preview and `ApplySketchOps` to commit; the engine runs
`sketch_solver::ops`. Agents reach the same operations through the
`sketch_edit` tool.

## Snapping (`snap.js`)

While drawing, `snap.js` returns a snapped position, the constraints to add
on commit, and an indicator to draw. Coincident with an existing point has the
highest priority. The other kinds are the origin, reference points, midpoints,
quadrants, on-entity, horizontal/vertical alignment, tangent and
perpendicular. The snap thresholds (pixel distances and the
horizontal/vertical angle) are settings in the store.

Snaps to the origin or to a reference point add a pin (`WhereDragged` with a
target) rather than a coincident constraint.

## Constraints

- `constraintLogic.js` decides which constraints apply to the current
  selection. The right-click menu (`ConstraintMenu.svelte`) and the toolbar
  buttons both use it.
- `bridgeConstraint.js` maps the UI's constraint spellings to the engine's
  `SketchConstraint` variants. Every send goes through it: `SolveSketch`
  resends the whole list, so one constraint the engine cannot parse fails
  every later solve in the sketch.
- A live drag sends the dragged point as `Dragged`. A persistent pin sends
  `Pinned { point, x, y }`.
- An exact duplicate of an existing constraint is refused.
- `constraintBadges.js` places the badge glyphs; the renderer and the select
  tool's hit test share it.
- `SketchConstraintList.svelte` lists the active sketch's constraints in the
  Properties panel. A row or a badge selects a constraint; Delete removes it.
  When the solver reports `OverConstrained`, the conflicting constraints are
  shown in the over-constrained color without user action.

## Rendering

- `SketchRenderer.svelte` — the active sketch: entities, tool preview, snap
  indicators, profiles and badges. Colors are CSS tokens (`--sketch-*`):
  default, selected, hovered, construction, fully constrained,
  over-constrained, profile hover and select.
- `InactiveSketchRenderer.svelte` — finished sketches outside sketch mode,
  including profile hit-testing for region selection.
- `DimensionLabels.svelte`, `DimensionInput.svelte` — dimension labels as an
  HTML overlay; click a label, type a value, Enter re-solves.
- `profiles.js` — closed-loop extraction and point-in-region tests used for
  drawing and hit-testing profiles.
- `bspline.js` — B-spline evaluation and interpolation for sketch splines.
- `axisUtils.js` — revolve axis from a sketch line or a body edge.
- `sketchToolState.svelte.js` — reactive preview and snap state that
  `tools.js` (a plain module) writes and the renderer reads.
