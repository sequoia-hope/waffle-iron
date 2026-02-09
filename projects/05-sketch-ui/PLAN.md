# 05 — Sketch UI: Plan

## Milestones

### M1: Sketch Mode Activation ✅
- [x] Click face → enter sketch mode (via toolbar Sketch button)
- [x] Solid becomes transparent (CadModel sketch-mode opacity)
- [x] Sketch plane displayed (SketchPlane.svelte grid overlay)
- [ ] Camera aligns to sketch plane — deferred
- [ ] 2D coordinate overlay — deferred
- [x] Exit sketch mode (Finish button / Escape key)

### M2: Line Tool
- [ ] Click-click line drawing
- [ ] Point creation at endpoints
- [ ] Rubberband preview
- [ ] Auto-reuse coincident points

### M3: Rectangle Tool
- [ ] Two-click rectangle
- [ ] Auto-generate 4 points, 4 lines
- [ ] Auto-apply Coincident + Horizontal + Vertical constraints
- [ ] Preview while drawing

### M4: Circle Tool
- [ ] Click center + click/drag radius
- [ ] Center point + Circle entity creation
- [ ] Radius preview

### M5: Arc Tool
- [ ] Click center + start + end
- [ ] 3 points + Arc entity creation
- [ ] Arc direction from click order

### M6: Constraint Application UI
- [ ] Select entities → show applicable constraints
- [ ] Right-click context menu with constraint options
- [ ] Apply constraint → trigger re-solve
- [ ] Visual indicator of applied constraints

### M7: Dimension Editing
- [ ] Display dimension labels near constrained entities
- [ ] Click label → edit value
- [ ] Re-solve on value change
- [ ] Leader lines from label to entity

### M8: Auto-Constraining (Snap Detection)
- [ ] Horizontal/Vertical snap (angle threshold)
- [ ] Coincident snap (distance threshold)
- [ ] Tangent snap
- [ ] On-entity snap
- [ ] Visual snap indicator (snap lines/dots)
- [ ] Configurable threshold settings

### M9: Visual Feedback
- [ ] Color coding by constraint status (green/blue/red)
- [ ] DOF counter in status bar
- [ ] Failed constraint highlighting
- [ ] Construction geometry dashed display

### M10: Profile Selection
- [ ] Click inside closed loop → identify profile
- [ ] Highlight selected profile
- [ ] Point-in-polygon test on solved geometry
- [ ] Ready for extrusion/revolution

### M11: Construction Geometry
- [ ] Toggle entity as construction
- [ ] Visual distinction (dashed lines)
- [ ] Exclude from profile extraction

## Blockers

- Depends on sketch-solver (for solving + SolvedSketch output)
- Depends on wasm-bridge (for communication with solver)
- Depends on 3d-viewport (for sketch-mode transitions)

## Interface Change Requests

(None yet)

## Notes

- Auto-constraining is critical for UX. Without it, sketching is painful.
- The Dragged constraint workflow enables interactive geometry manipulation.
- Dimension editing should feel like Onshape: click label, type value, Enter.
