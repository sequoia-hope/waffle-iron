# 04 — 3D Viewport: Interfaces

## Types This Crate CONSUMES

| Type | Source | Purpose |
|------|--------|---------|
| `RenderMesh` | INTERFACES.md (kernel-fork via wasm-bridge) | Triangle mesh for rendering |
| `EdgeRenderData` | INTERFACES.md (kernel-fork via wasm-bridge) | Edge lines for overlay |
| `FaceRange` | INTERFACES.md | Maps triangles to logical faces for picking |
| `EdgeRange` | INTERFACES.md | Maps edge segments to logical edges for picking |
| `GeomRef` | INTERFACES.md | Identifies picked entities |

## Events This Crate PRODUCES

| Event | Consumer | Purpose |
|-------|----------|---------|
| `SelectEntity { geom_ref }` | wasm-bridge → feature-engine | User clicked a face/edge |
| `HoverEntity { geom_ref }` | wasm-bridge → feature-engine | User hovering over entity |

## Svelte Component API

```svelte
<!-- Main viewport component -->
<Viewport
  meshes={renderMeshes}
  edges={edgeRenderData}
  selectedRefs={selectedGeomRefs}
  hoveredRef={hoveredGeomRef}
  sketchMode={isInSketchMode}
  on:select={handleSelect}
  on:hover={handleHover}
/>
```

## Notes

- This is a Svelte/JS component. It does not import Rust types directly — it receives data that has already been deserialized by wasm-bridge.
- GeomRef objects arrive as plain JS objects (deserialized from JSON).
- Mesh data arrives as TypedArray views (Float32Array, Uint32Array).
