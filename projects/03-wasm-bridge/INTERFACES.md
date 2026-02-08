# 03 — WASM Bridge: Interfaces

## Types This Crate IMPLEMENTS

| Type | Role |
|------|------|
| Bridge logic | Message routing between JS and WASM engine |
| TypedArray mesh transfer | Near-zero-copy mesh data from WASM to JS |
| Worker setup | Web Worker lifecycle management |

## Types This Crate CONSUMES

| Type | Source | Purpose |
|------|--------|---------|
| `UiToEngine` | INTERFACES.md | Commands from UI to engine |
| `EngineToUi` | INTERFACES.md | Results from engine to UI |
| `RenderMesh` | INTERFACES.md (kernel-fork) | Mesh data to transfer to JS |
| `EdgeRenderData` | INTERFACES.md (kernel-fork) | Edge data to transfer to JS |
| `FaceRange` | INTERFACES.md (kernel-fork) | Face-to-triangle mapping |
| `SketchEntity` | INTERFACES.md | Sketch data for solver bridge |
| `SketchConstraint` | INTERFACES.md | Constraint data for solver bridge |
| `SolvedSketch` | INTERFACES.md (sketch-solver) | Solver results |
| `FeatureTree` | INTERFACES.md (feature-engine) | Feature tree state |
| `Operation` | INTERFACES.md | Operation parameters |
| `GeomRef` | INTERFACES.md | Geometry references |

## JS-Facing API

The bridge exposes a JavaScript API consumed by Svelte components:

```typescript
// Sent from main thread to Worker
interface EngineCommand {
  type: string;  // UiToEngine variant name
  payload: any;  // Variant-specific data (JSON)
}

// Received from Worker on main thread
interface EngineResult {
  type: string;          // EngineToUi variant name
  payload: any;          // Variant-specific metadata (JSON)
  meshes?: ArrayBuffer[];  // Transferred mesh buffers (if ModelUpdated)
}
```

## Notes

- This crate sits at the language boundary. It depends on both Rust engine crates and JS/WASM interop (wasm-bindgen, js-sys, web-sys).
- All Rust types from INTERFACES.md that need to cross the WASM boundary must have serde Serialize/Deserialize.
