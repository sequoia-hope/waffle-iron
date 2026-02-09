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
- [x] Transition to sketch mode: solid becomes transparent (opacity 0.2, depthWrite off)
- [x] Display sketch plane (grid + border + semi-transparent background)
- [x] Transition back on sketch finish (enterSketchMode/exitSketchMode in store)
- [x] Sketch plane orientation from normal vector

### M9: Datum Visualization
- [x] Render datum planes (XY/XZ/YZ, semi-transparent with colored borders)
- [x] Render datum axes (lines with arrowhead cones, X=red Y=green Z=blue)
- [x] Origin triad (sphere at origin + colored axis lines + cone tips)

### M10: Coordinate Gizmo
- [x] Small axis triad in viewport corner (orthographic overlay, bottom-right)
- [x] Shows camera orientation (gizmo rotation synced with main camera)
- [x] Click to snap to standard views (Front/Back/Top/Bottom/Left/Right/Iso)

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
- Scene.svelte wraps all 3D content and initializes interactivity plugin.
- ViewCube split into ViewCubeGizmo (three.js overlay in Canvas) and ViewCubeButtons (HTML overlay outside Canvas). Communication via CustomEvent 'waffle-snap-view'.
- Sketch plane grid/border rendered as LineSegments in a Group oriented by plane normal.
- Datum planes at 6% opacity to avoid cluttering the view.

## Implementation Files

| File | Purpose |
|------|---------|
| `app/src/lib/viewport/Viewport.svelte` | Canvas wrapper + HTML overlays |
| `app/src/lib/viewport/Scene.svelte` | Scene root, initializes interactivity |
| `app/src/lib/viewport/CadModel.svelte` | Mesh rendering with face-range picking + sketch transparency |
| `app/src/lib/viewport/CameraControls.svelte` | OrbitControls + fit-all + snap-to-view |
| `app/src/lib/viewport/Lighting.svelte` | Ambient + directional + hemisphere |
| `app/src/lib/viewport/EdgeOverlay.svelte` | Edge line segments overlay |
| `app/src/lib/viewport/GridFloor.svelte` | Infinite grid floor |
| `app/src/lib/viewport/SketchPlane.svelte` | Sketch-mode plane with grid and border |
| `app/src/lib/viewport/DatumVis.svelte` | XY/XZ/YZ datum planes + origin triad |
| `app/src/lib/viewport/ViewCubeGizmo.svelte` | Orientation gizmo (three.js overlay) |
| `app/src/lib/viewport/ViewCubeButtons.svelte` | Standard view buttons (HTML overlay) |
| `app/src/lib/engine/store.svelte.js` | Engine state + hover/selection/sketch-mode state |
