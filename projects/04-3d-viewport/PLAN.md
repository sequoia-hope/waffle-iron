# 04 — 3D Viewport: Plan

## Milestones

### M1: Basic Setup
- [x] Svelte + Threlte project setup
- [x] Render a static triangle mesh (hardcoded)
- [x] Verify three.js rendering works in browser

### M2: Mesh from Bridge
- [x] Receive RenderMesh from wasm-bridge
- [x] Create BufferGeometry from Float32Array/Uint32Array
- [x] Apply MeshStandardMaterial
- [x] Render shaded solid

### M3: Camera Controls
- [x] OrbitControls setup
- [x] Orbit (middle drag)
- [x] Pan (shift + middle drag)
- [x] Zoom (scroll wheel)
- [x] Fit all (double-click or key) — press 'f' to fit all
- [x] Smooth transitions (damping enabled)

### M4: Edge Overlays
- [x] Receive EdgeRenderData from wasm-bridge
- [x] Render as LineSegments on top of faces
- [x] Correct depth handling (polygonOffset -1/-1 to avoid z-fighting)

### M5: Raycaster Picking
- [x] Set up Raycaster on mousemove/click (Threlte interactivity plugin)
- [x] Map intersected triangle index to face-range → GeomRef
- [x] Binary search in face_ranges for efficient lookup
- [x] Picking wired to onpointermove/onclick/onpointerout events

### M6: Hover Highlighting
- [x] Highlight face on mousemove (per-face material via geometry groups)
- [x] Unhighlight on mouseout
- [x] Hover color distinct from default (0xaabbdd vs 0x8899aa)
- [x] Reactive material rebuild on hover state change

### M7: Click Selection
- [x] Select face on click
- [x] Persistent selection highlight (0x44aaff, different from hover)
- [x] Multi-select with Shift+click (toggle)
- [x] Send SelectEntity message via wasm-bridge
- [x] Clear selection on background click (onpointermissed)

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

- ~~Depends on kernel-fork for tessellation output format (RenderMesh)~~ Resolved
- ~~Depends on wasm-bridge for mesh transfer protocol~~ Resolved

## Interface Change Requests

(None yet)

## Notes

- Raycaster picking via Threlte interactivity plugin (built-in raycaster with event system).
- Edge rendering uses polygonOffset to prevent z-fighting.
- Per-face highlighting uses BufferGeometry groups with material arrays.
- Grid floor added via @threlte/extras Grid component for visual reference.
- CADmium used Threlte successfully — referenced their viewport setup.
- Scene.svelte wraps all 3D content and initializes interactivity plugin.

## Implementation Files

| File | Purpose |
|------|---------|
| `app/src/lib/viewport/Viewport.svelte` | Canvas wrapper |
| `app/src/lib/viewport/Scene.svelte` | Scene root, initializes interactivity |
| `app/src/lib/viewport/CadModel.svelte` | Mesh rendering with face-range picking |
| `app/src/lib/viewport/CameraControls.svelte` | OrbitControls + fit-all |
| `app/src/lib/viewport/Lighting.svelte` | Ambient + directional + hemisphere |
| `app/src/lib/viewport/EdgeOverlay.svelte` | Edge line segments overlay |
| `app/src/lib/viewport/GridFloor.svelte` | Infinite grid floor |
| `app/src/lib/engine/store.svelte.js` | Engine state + hover/selection state |
