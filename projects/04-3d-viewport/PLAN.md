# 04 — 3D Viewport: Plan

## Milestones

### M1: Basic Setup
- [ ] Svelte + Threlte project setup
- [ ] Render a static triangle mesh (hardcoded)
- [ ] Verify three.js rendering works in browser

### M2: Mesh from Bridge
- [ ] Receive RenderMesh from wasm-bridge
- [ ] Create BufferGeometry from Float32Array/Uint32Array
- [ ] Apply MeshStandardMaterial
- [ ] Render shaded solid

### M3: Camera Controls
- [ ] OrbitControls setup
- [ ] Orbit (middle drag)
- [ ] Pan (shift + middle drag)
- [ ] Zoom (scroll wheel)
- [ ] Fit all (double-click or key)
- [ ] Smooth transitions

### M4: Edge Overlays
- [ ] Receive EdgeRenderData from wasm-bridge
- [ ] Render as LineSegments on top of faces
- [ ] Correct depth handling (slight offset to avoid z-fighting)

### M5: Raycaster Picking
- [ ] Set up Raycaster on mousemove/click
- [ ] Map intersected triangle index to face-range → GeomRef
- [ ] Binary search in face_ranges for efficient lookup
- [ ] Test: pick faces on a box

### M6: Hover Highlighting
- [ ] Highlight face on mousemove (change color/emissive)
- [ ] Unhighlight on mouseout
- [ ] Smooth color transitions
- [ ] Test: hover over different faces, verify correct highlighting

### M7: Click Selection
- [ ] Select face on click
- [ ] Persistent selection highlight (different color from hover)
- [ ] Multi-select with Shift+click
- [ ] Send SelectEntity message via wasm-bridge
- [ ] Clear selection on background click

### M8: Sketch-Mode Transparency
- [ ] Transition to sketch mode: solid becomes transparent
- [ ] Display sketch plane
- [ ] Transition back on sketch finish
- [ ] Smooth opacity animation

### M9: Datum Visualization
- [ ] Render datum planes (semi-transparent with border)
- [ ] Render datum axes (lines with arrows)
- [ ] Origin triad

### M10: Coordinate Gizmo
- [ ] Small axis triad in viewport corner
- [ ] Shows camera orientation
- [ ] Click to snap to standard views

## Blockers

- Depends on kernel-fork for tessellation output format (RenderMesh)
- Depends on wasm-bridge for mesh transfer protocol

## Interface Change Requests

(None yet)

## Notes

- Start with Raycaster picking. Only switch to GPU color-coding if performance requires it.
- Edge rendering needs z-fighting prevention (polygon offset or slight vertex displacement).
- CADmium used Threlte successfully — reference their viewport setup.
