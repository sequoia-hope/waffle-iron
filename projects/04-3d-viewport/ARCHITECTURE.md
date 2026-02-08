# 04 — 3D Viewport: Architecture

## Purpose

Render 3D geometry using three.js via [Threlte](https://threlte.xyz/) (declarative three.js for Svelte). Receives tessellated mesh data from the WASM engine via wasm-bridge. Handles camera controls, entity picking, hover/selection highlighting, and sketch-mode transitions.

## Technology

- **three.js** — Industry-standard browser 3D rendering. Mature ecosystem with selection, transparency, post-processing.
- **Threlte** — Declarative three.js components for Svelte. Used successfully by CADmium.
- **No Rust rendering.** All 3D rendering is JavaScript/three.js. Rust/WASM produces data only.

## Rendering Pipeline

### Solid Face Rendering
- Receive `RenderMesh` from wasm-bridge (Float32Array vertices, normals; Uint32Array indices).
- Create `three.js BufferGeometry` from the TypedArrays.
- Apply `MeshStandardMaterial` with appropriate CAD colors (gray metallic default).
- Flat shading for planar faces, smooth for curved.

### Sharp Edge Overlay
- Receive `EdgeRenderData` from wasm-bridge.
- Render as `LineSegments` with `LineBasicMaterial` (dark lines over shaded faces).
- Edges are rendered on top of faces for CAD-style visualization.

### Entity Picking

Two approaches (evaluate both):

**Option A: three.js Raycaster + face-range metadata**
- Use three.js `Raycaster` to find the intersected triangle.
- Look up the triangle's index in `face_ranges` (binary search) to find the owning `GeomRef`.
- Pros: Simple, uses built-in three.js. Cons: Raycaster performance with large meshes.

**Option B: GPU color-coding**
- Render entity IDs to an offscreen `WebGLRenderTarget`.
- Encode each face-range with a unique color (R,G,B channels = entity index).
- Read pixel at cursor position → decode entity ID.
- Pros: O(1) regardless of mesh complexity. Cons: Extra render pass, precision limits.

**Recommended: Start with Option A** (Raycaster). Switch to Option B if performance is insufficient.

### Hover Highlighting
- On mousemove: raycast → identify face GeomRef → change material color/emissive for that face range.
- Use `BufferGeometry` groups to separate face ranges for independent material assignment.
- Or: use a custom shader that reads a hover mask uniform.

### Click Selection
- On click: identify face GeomRef → send `SelectEntity` message via wasm-bridge.
- Persistent highlight (different color from hover).
- Multi-select with Shift+click.

### Sketch-Mode Transparency
- When entering sketch mode:
  - Set solid mesh material to transparent (opacity ~0.2).
  - Display the sketch plane as an opaque grid/surface.
  - Sketch-ui overlays 2D drawing on the sketch plane.
- When exiting sketch mode:
  - Restore solid mesh to full opacity.

### Datum Visualization
- Datum planes: semi-transparent rectangles with border.
- Datum axes: line segments with arrowheads.
- Origin indicator: X/Y/Z axis triad at the origin.

## Camera Controls

Onshape-style controls (users expect these):

| Action | Input |
|--------|-------|
| Orbit | Middle mouse drag |
| Pan | Shift + middle mouse drag |
| Zoom | Scroll wheel |
| Fit all | Double-click middle mouse (or keyboard shortcut) |
| Rotate to face | Click face in the view cube |

Starting point: Threlte/three.js `OrbitControls` with customization for Onshape-style behavior.

### Coordinate Gizmo
- Small axis triad in a corner of the viewport (like Onshape's view cube).
- Shows current camera orientation.
- Clickable to snap to standard views (Front, Back, Top, Bottom, Left, Right, Iso).
