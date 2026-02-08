# 04 — 3D Viewport: Agent Instructions

You are working on **3d-viewport**. Read ARCHITECTURE.md in this directory first.

## Your Job

Build a Svelte component using Threlte (declarative three.js for Svelte) that renders 3D CAD geometry. You receive tessellated mesh data from the WASM engine via wasm-bridge. You handle camera controls, entity picking, hover/selection highlighting, and sketch-mode transitions.

## Critical Rules

1. **No Rust/WASM in this component.** You receive data from wasm-bridge as TypedArrays and JSON objects. You do NOT call any WASM functions for rendering.
2. **Picking must map to GeomRef.** When the user clicks or hovers, you must identify which logical face/edge they're pointing at — not just the raw triangle. Use face-range metadata from the mesh.
3. **Onshape-style camera controls.** Users expect scroll-to-zoom, middle-drag-orbit, shift-middle-pan.
4. **Edge overlays are essential.** CAD visualization requires sharp edge lines over shaded faces.

## Technology

- **Svelte** — component framework
- **Threlte** — declarative three.js for Svelte
- **three.js** — 3D rendering (BufferGeometry, MeshStandardMaterial, Raycaster, OrbitControls)

## Key Files

- `src/lib/Viewport.svelte` — Main viewport component
- `src/lib/MeshRenderer.svelte` — Renders solid faces from RenderMesh
- `src/lib/EdgeRenderer.svelte` — Renders edge overlays from EdgeRenderData
- `src/lib/Picker.svelte` — Raycaster picking + hover/selection logic
- `src/lib/CameraControls.svelte` — Orbit/pan/zoom controls
- `src/lib/Gizmo.svelte` — Coordinate gizmo
- `src/lib/SketchPlane.svelte` — Sketch-mode plane visualization

## Dependencies

- @threlte/core, @threlte/extras
- three
- No Rust/WASM dependencies
