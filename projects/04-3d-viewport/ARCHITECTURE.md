# 04 — 3D Viewport: Architecture

The viewport is the Svelte/three.js code in `app/src/lib/viewport/`. It draws
the meshes the engine produces, moves the camera, and turns pointer events
into selections. It uses [Threlte](https://threlte.xyz/) (declarative three.js
for Svelte 5). Rust produces data only; all rendering is here.

## Data in

The store (`app/src/lib/engine/store.svelte.js`) holds the meshes from the
latest model update. Each body's mesh has `Float32Array` vertices and normals,
a `Uint32Array` of indices, `face_ranges` (triangle ranges, each with the
`GeomRef` of the face that owns it), edge polylines with their refs, and, in
an Assembly tab, the solved `transform` of the owning instance.

## Rendering

- **Faces** (`CadModel.svelte`) — one `BufferGeometry` per body with a group
  per face range, so each face can take its own material (default, hovered,
  selected, ghost). Colors come from CSS custom properties so they follow the
  theme. In sketch mode solids are drawn at 20 % opacity.
- **Edges** (`EdgeOverlay.svelte`) — line segments over the faces, with a
  screen-constant depth bias (`edgeDepthBias.js`) so they do not z-fight the
  faces they bound.
- **Vertices** (`VertexOverlay.svelte`, `bodyVertices.js`) — topological
  vertices as points.
- **Assembly placement** (`placement.js`) — every layer that draws a mesh's
  data applies that mesh's instance transform.
- **Datums** (`DatumVis.svelte`) — the three origin planes, axes and origin.
- **Sketch plane** (`SketchPlane.svelte`) — grid and border on the active
  sketch plane.
- **Ghosts** (`GhostPreview.svelte`, `faceGeometry.js`) — preview of a pending
  operation.
- **Section view** (`SectionCap.svelte`, `sectionPlane.js`) — clipping plane
  with a stencil-buffer cap.
- **Mate connectors** (`ConnectorFrames.svelte`) — a triad at each connector
  frame.
- **Lighting** (`Lighting.svelte`) — lights are aimed in camera space.
- **KiCad board data** (`KicadHoverCard.svelte`, `KicadDetailPanel.svelte`) —
  component and board records for bodies derived from a linked board.

## Picking

Picking uses the three.js raycaster through Threlte's interactivity plugin.

- A face hit gives a triangle index; a binary search of `face_ranges` gives
  the owning `GeomRef`.
- Edge pick distance is 6 screen pixels, converted to world units each frame
  (`picking.js`, `worldPerPixel`), so picking behaves the same at any part
  scale or zoom.
- A face suppresses an edge only when it is strictly closer by more than 1.5
  pixels of depth (`faceOccludes`).
- Hover is arbitrated per pixel with priority Vertex, then Edge, then Face
  (`proposeHoverRef` in the store). A click uses the hovered ref only if it
  was arbitrated at the click pixel (`getFreshHoveredRef`).
- `BoxSelect.svelte` does drag-rectangle selection.

## Camera

`CameraControls.svelte` wraps `OrbitControls` and replaces its update with a
quaternion rotation so the camera can orbit past the poles.

| Action | Input |
|---|---|
| Orbit | Right mouse drag; one-finger drag |
| Pan | Middle mouse drag |
| Zoom | Scroll wheel (to the point under the cursor); two-finger pinch |
| Select / sketch tools | Left mouse |
| Fit all | `F` |

The mapping is the same in modeling and sketch modes. Perspective and
orthographic cameras are both supported. Wheel deltas are normalized across
browsers in `wheel.js`. Other components drive the camera through window
events: `waffle-snap-view`, `waffle-snap-view-and-fit`, `waffle-fit-all`,
`waffle-align-to-plane`, `waffle-zoom-to-face`, `waffle-save-camera`,
`waffle-restore-camera`.

`framing.js` holds the standard views and the fit math. `ViewCube.svelte` is
the orientation cube: click to snap to a standard view, drag to orbit.

## Agent capture

`AgentCapture.svelte` and `capture.js` answer the agent's `viewport_capture`
tool: they render a frame off the live view at a requested size, view and
style, and return a PNG with a legend. They reuse `framing.js`, so a captured
view is framed exactly as the View Cube and `F` frame it.

## Files

| File | Purpose |
|---|---|
| `Viewport.svelte` | Canvas and HTML overlays |
| `Scene.svelte` | Scene root; enables interactivity |
| `CadModel.svelte` | Faces, face picking, selection |
| `EdgeOverlay.svelte`, `VertexOverlay.svelte` | Edge and vertex layers and their picking |
| `CameraControls.svelte` | Camera, navigation, fit, snap |
| `ViewCube.svelte` | Orientation cube |
| `BoxSelect.svelte` | Box selection |
| `ViewportContextMenu.svelte` | Right-click menu |
| `RotationCenter.svelte` | Debug marker for the orbit pivot |
| `picking.js`, `framing.js`, `placement.js`, `wheel.js` | Shared math |
