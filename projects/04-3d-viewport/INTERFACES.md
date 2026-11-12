# 04 — 3D Viewport: Interfaces

The viewport is JavaScript. It imports no Rust types; it reads data the bridge
has already deserialized.

## Consumes (from the store)

| Data | Shape | Use |
|---|---|---|
| Body mesh | `vertices`, `normals` (`Float32Array`), `indices` (`Uint32Array`) | Face geometry |
| `face_ranges` | `[{ geom_ref, start_index, end_index }]`, sorted by `start_index` | Triangle → face |
| Edge data | Edge polyline vertices plus a ref per edge | Edge overlay and picking |
| `transform` | Instance placement (Assembly tabs) | `placement.js` |
| Selection, hover, sketch mode, section plane, camera projection | Store state | Highlighting and modes |

## Produces

- Hover proposals and selections, written to the store, which forwards them
  to the engine.
- Camera snapshots for sketch undo (`waffle-save-camera` /
  `waffle-restore-camera`).
- PNG captures for the agent's `viewport_capture` tool.

## Component

`Viewport.svelte` takes no mesh props; it and its children read the store
directly.
