# 04 — 3D Viewport: Plan

> **Status:** the milestones below are an accurate **as-built** snapshot (all
> closed). **Active forward work** (per-body STL export, face→feature highlight)
> is tracked in `docs/prototype_release_roadmap.md` (Phases C, D).

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

### M11: Scale-Aware Picking + Hover Arbitration ✅ (2026-07-05)

Spec: `specs/projected_sketch_geometry.md` Cycle 2 (invariants I2/I3). New
shared module `app/src/lib/viewport/picking.js`.

- [x] Edge pick threshold was absolute world units (0.06 — ~5× a default-drawn
      part!), making faces of default-scale bodies unpickable everywhere.
      Now screen-calibrated: 6px × `worldPerPixel(camera, canvasH)` per frame.
- [x] Edge-vs-face occlusion is a depth comparison (face suppresses edge only
      if strictly closer beyond 1.5px-equivalent), replacing the existence
      check that made edges unhoverable in the straight-on sketch view.
- [x] Deterministic hover priority Vertex ≻ Edge ≻ Face via pixel-keyed
      arbitration (`proposeHoverRef`), replacing last-writer-wins listeners.
- [x] Click reads hover only if arbitrated within 4px of the click pixel
      (`getFreshHoveredRef`) — kills the stale-edge-selected-on-bare-click race.
- Guards: `projection-select-first-adversarial.spec.js` SR1–SR3 (small default
  box modeling + sketch picking, proportioned large body).

## Blockers

- ~~Depends on kernel-fork for tessellation output format (RenderMesh)~~ Resolved
- ~~Depends on wasm-bridge for mesh transfer protocol~~ Resolved
- **Follow-up (2026-07-05, not picking): auto-fit degenerates on high-aspect
  "needle" bodies.** Sketch-XY and extrude-Z resolve at different world-unit
  scales (~14× apart; likely same family as the known dimension-tool mm/m
  red), so a large extrude depth on a default-drawn sketch yields a ~1700:1
  needle, and auto-fit parks the camera INSIDE the body looking down its long
  axis — the center ray misses all faces (looks like a picking bug; is not).
  Repro + analysis in `specs/projected_sketch_geometry.md` Cycle 2 validation
  notes. Owner: viewport auto-fit + sketch/extrude unit handling; needs its
  own cycle.

## Interface Change Requests

(None yet)

## Notes

- Raycaster picking via Threlte interactivity plugin (built-in raycaster with event system).
- Edge rendering uses polygonOffset to prevent z-fighting.
- Per-face highlighting uses BufferGeometry groups with material arrays.
- GridFloor.svelte was intentionally removed — grid rendering is handled differently.
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
| `app/src/lib/viewport/SketchPlane.svelte` | Sketch-mode plane with grid and border |
| `app/src/lib/viewport/BoxSelect.svelte` | Box selection overlay for multi-entity selection |
| `app/src/lib/viewport/GhostPreview.svelte` | Ghost preview for pending operations |
| `app/src/lib/viewport/VertexOverlay.svelte` | Vertex point overlay rendering |
| `app/src/lib/viewport/ViewportContextMenu.svelte` | Right-click context menu in viewport |
| `app/src/lib/viewport/DatumVis.svelte` | XY/XZ/YZ datum planes + origin triad |
| `app/src/lib/viewport/ViewCubeGizmo.svelte` | Orientation gizmo (three.js overlay) |
| `app/src/lib/viewport/ViewCubeButtons.svelte` | Standard view buttons (HTML overlay) |
| `app/src/lib/engine/store.svelte.js` | Engine state + hover/selection/sketch-mode state |
