# 05 — Sketch UI: Plan

## Milestones

### M1: Sketch Mode Activation ✅
- [x] Click face → enter sketch mode (via toolbar Sketch button)
- [x] Solid becomes transparent (CadModel sketch-mode opacity)
- [x] Sketch plane displayed (SketchPlane.svelte grid overlay)
- [x] Camera aligns to sketch plane (dispatches waffle-align-to-plane event)
- [x] 2D coordinate overlay (X/Y axes + origin marker + cursor readout in status bar)
- [x] Exit sketch mode (Finish button / Escape key)

### M2: Line Tool ✅
- [x] Click-click line drawing
- [x] Point creation at endpoints
- [x] Rubberband preview
- [x] Auto-reuse coincident points
- [x] Continuous line chaining (end → start of next)

### M3: Rectangle Tool ✅
- [x] Two-click rectangle
- [x] Auto-generate 4 points, 4 lines
- [x] Auto-apply Horizontal + Vertical constraints
- [x] Preview while drawing

### M4: Circle Tool ✅
- [x] Click center + click radius
- [x] Center point + Circle entity creation
- [x] Radius preview

### M5: Arc Tool ✅
- [x] Click center + start + end
- [x] 3 points + Arc entity creation
- [x] Arc direction from click order
- [x] Preview during each step

### M6: Constraint Application UI ✅
- [x] Select entities → show applicable constraints
- [x] Right-click context menu with constraint options
- [x] Apply constraint → send to engine
- [x] Applicable constraints determined by selection composition
- [x] Constraint type indicators rendered on sketch plane

### M7: Dimension Editing ✅
- [x] Display dimension labels near constrained entities (Distance, Radius, Diameter, Angle)
- [x] Click label → inline edit value
- [x] Update constraint value on Enter/blur
- [x] Leader lines from label to entity
- [x] Uses @threlte/extras HTML component for in-viewport labels

### M8: Auto-Constraining (Snap Detection) ✅
- [x] Horizontal/Vertical snap (3° angle threshold)
- [x] Coincident snap (8px adaptive threshold)
- [x] On-entity snap (lines, circles — 5px threshold)
- [x] Visual snap indicators (green dot for coincident/on-entity, dashed line for H/V)
- [x] Auto-constraint application (H/V constraints auto-added on line creation)
- [x] Tangent snap (line-to-circle tangent point computation)
- [x] Perpendicular snap (point projection onto line foot)
- [x] Configurable threshold settings (snap settings in Property Editor)

### M9: Visual Feedback
- [x] Color coding: blue (default), yellow (selected), light blue (hovered)
- [x] Entity/constraint counts in status bar
- [ ] Full DOF counter (requires solver — currently NotImplemented in WASM)
- [x] Failed constraint highlighting (over-constrained entities shown in red)
- [x] Construction geometry dashed display (done in M11)

### M10: Profile Selection ✅
- [x] Click inside closed loop → identify profile
- [x] Highlight selected profile (green) and hovered profile (light green)
- [x] Point-in-polygon test via ray casting on sketch geometry
- [x] Client-side profile extraction (half-edge minimal face algorithm)
- [x] Semi-transparent fill for hovered/selected profiles
- [x] Ready for extrusion/revolution (selectedProfileIndex in store)

### M11: Construction Geometry ✅
- [x] Toggle entity as construction (toggleConstruction in store)
- [x] Visual distinction (LineDashedMaterial, dimmer color 0x6677aa)
- [x] Exclude from profile extraction
- [x] Toolbar button ("Constr") + keyboard shortcut (X)
- [x] Works on all entity types (lines, circles, arcs, points)

## Implementation Summary

### New files created
| File | Purpose |
|------|---------|
| `app/src/lib/sketch/sketchCoords.js` | Screen→3D→2D coordinate projection |
| `app/src/lib/sketch/tools.js` | Tool state machines (line, rect, circle, arc, select) + profile hit-test |
| `app/src/lib/sketch/snap.js` | Auto-constraining: coincident, H/V, on-entity snap |
| `app/src/lib/sketch/profiles.js` | Client-side closed-loop extraction (half-edge algorithm) |
| `app/src/lib/sketch/SketchRenderer.svelte` | Renders sketch entities + preview + snap + profiles |
| `app/src/lib/sketch/SketchInteraction.svelte` | Invisible plane capturing pointer events |
| `app/src/lib/sketch/ConstraintMenu.svelte` | Right-click popup for manual constraints |
| `app/src/lib/sketch/DimensionLabels.svelte` | Editable dimension labels via HTML overlay |

### Modified files
| File | Changes |
|------|---------|
| `app/src/lib/engine/store.svelte.js` | Sketch entity/constraint state, ID allocator, hit-test helpers, construction toggle, profile state |
| `app/src/lib/viewport/Scene.svelte` | Import SketchRenderer + SketchInteraction + DimensionLabels |
| `app/src/lib/viewport/Viewport.svelte` | Import ConstraintMenu HTML overlay |
| `app/src/lib/ui/StatusBar.svelte` | Show entity/constraint counts in sketch mode |

## Blockers

- SolveSketch returns NotImplemented in WASM (libslvs C++ can't compile to wasm32)
  - Entities/constraints accumulate correctly and are sent to engine
  - UI uses as-placed positions; solver would update positions when available
  - Profile extraction uses client-side JS implementation (bypasses solver)

## Interface Change Requests

(None)

## Notes

- Auto-constraining is critical for UX. Without it, sketching is painful.
- The Dragged constraint workflow enables interactive geometry manipulation.
- Dimension editing should feel like Onshape: click label, type value, Enter.
- All sketch state resets when entering/exiting sketch mode.
- Tool state machines support continuous chaining (line tool) and multi-click flows (arc tool).
