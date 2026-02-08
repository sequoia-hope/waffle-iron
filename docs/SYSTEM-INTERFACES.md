# System Interfaces Map

Visual map of ALL cross-crate data flows. For each interface type: which crate produces it, which crates consume it, and what data flows through it.

## Crate Dependency Graph

```
                    ┌──────────────┐
                    │  INTERFACES  │  (shared types, no crate)
                    └──────┬───────┘
                           │ defines types for all
           ┌───────────────┼───────────────────────┐
           │               │                       │
           ▼               ▼                       ▼
  ┌─────────────┐  ┌──────────────┐  ┌───────────────────┐
  │ kernel-fork │  │sketch-solver │  │ (Svelte/JS layer) │
  │  (01)       │  │  (02)        │  │ 04, 05, 08        │
  └──────┬──────┘  └──────┬───────┘  └────────┬──────────┘
         │                │                    │
         ▼                │                    │ consumes
  ┌──────────────┐        │                    │ EngineToUi
  │ modeling-ops │        │                    │
  │  (07)        │        │                    │
  └──────┬───────┘        │                    │
         │                │                    │
         ▼                ▼                    │
  ┌─────────────────────────────┐              │
  │      feature-engine (06)    │              │
  │  (orchestrates 07 + 02)     │              │
  └──────────────┬──────────────┘              │
                 │                             │
                 ▼                             │
  ┌─────────────────────────────┐              │
  │      wasm-bridge (03)       │◄─────────────┘
  │  (Rust↔JS boundary)        │  produces
  └──────────────┬──────────────┘  UiToEngine
                 │
                 ▼
  ┌─────────────────────────────┐
  │      file-format (09)       │
  │  (save/load/export)         │
  └─────────────────────────────┘
```

## Type Flow Matrix

### Kernel Types (produced by kernel-fork)

| Type | Producer | Consumers | Flow |
|------|----------|-----------|------|
| `KernelSolidHandle` | kernel-fork | modeling-ops, feature-engine | Runtime handle to kernel solid. Never crosses WASM boundary. |
| `KernelId` | kernel-fork | modeling-ops, feature-engine | Transient entity ID. Never persisted. |
| `RenderMesh` | kernel-fork (tessellation) | wasm-bridge → 3d-viewport | Triangle mesh. Crosses WASM boundary as TypedArray. |
| `EdgeRenderData` | kernel-fork | wasm-bridge → 3d-viewport | Edge line segments. Crosses WASM boundary as TypedArray. |
| `FaceRange` | kernel-fork | wasm-bridge → 3d-viewport | Maps triangles to logical faces. JSON across boundary. |
| `KernelError` | kernel-fork | modeling-ops → feature-engine → wasm-bridge → ui-chrome | Error propagation chain. |

### Operation Types (produced by modeling-ops)

| Type | Producer | Consumers | Flow |
|------|----------|-----------|------|
| `OpResult` | modeling-ops | feature-engine | Complete operation result. Stays in WASM. |
| `Provenance` | modeling-ops | feature-engine | Entity tracking for persistent naming. Stays in WASM. |
| `EntityRecord` | modeling-ops | feature-engine | Entity with ID + kind + signature. Stays in WASM. |
| `Rewrite` | modeling-ops | feature-engine | Modified entity mapping. Stays in WASM. |
| `Diagnostics` | modeling-ops | feature-engine → wasm-bridge → ui-chrome | Timing/warnings. JSON across boundary. |

### Sketch Types

| Type | Producer | Consumers | Flow |
|------|----------|-----------|------|
| `Sketch` | sketch-ui (via bridge) | feature-engine, sketch-solver | Sketch definition. JSON across WASM boundary. |
| `SketchEntity` | sketch-ui | sketch-solver, feature-engine | Individual entity. JSON across boundary. |
| `SketchConstraint` | sketch-ui | sketch-solver, feature-engine | Constraint definition. JSON across boundary. |
| `SolvedSketch` | sketch-solver | feature-engine, sketch-ui (via bridge) | Solved positions. JSON across boundary. |
| `SolveStatus` | sketch-solver | sketch-ui (via bridge), ui-chrome | Constraint status. JSON across boundary. |
| `ClosedProfile` | sketch-solver | feature-engine, modeling-ops | Closed loop for extrusion. Stays in WASM. |

### Geometry Reference Types

| Type | Producer | Consumers | Flow |
|------|----------|-----------|------|
| `GeomRef` | feature-engine | modeling-ops, file-format, wasm-bridge, ui-chrome | Persistent naming reference. JSON for persistence/bridge. |
| `Anchor` | feature-engine | feature-engine (resolver) | Feature output reference. Persisted in JSON. |
| `Selector` | feature-engine | feature-engine (resolver) | Entity selection strategy. Persisted in JSON. |
| `Role` | modeling-ops | feature-engine | Semantic entity label. Persisted via GeomRef. |
| `TopoSignature` | kernel-fork | feature-engine, modeling-ops | Geometric signature. Persisted in GeomRef (Signature selector). |

### Feature Tree Types

| Type | Producer | Consumers | Flow |
|------|----------|-----------|------|
| `FeatureTree` | feature-engine | wasm-bridge → ui-chrome, file-format | Feature list. JSON across boundary and to disk. |
| `Feature` | feature-engine | ui-chrome (display), file-format (persistence) | Individual feature. JSON. |
| `Operation` | feature-engine / ui-chrome | modeling-ops (execution), file-format (persistence) | Operation params. JSON across boundary. |

### Bridge Protocol Types

| Type | Producer | Consumers | Flow |
|------|----------|-----------|------|
| `UiToEngine` | ui-chrome, sketch-ui (JS) | wasm-bridge → feature-engine | Commands. JSON via postMessage. |
| `EngineToUi` | feature-engine (WASM) | wasm-bridge → ui-chrome, sketch-ui, 3d-viewport | Results. JSON metadata + TypedArray meshes via postMessage. |

## Data Boundary: WASM ↔ JS

The wasm-bridge is the ONLY crossing point between Rust/WASM and JavaScript:

```
WASM side (Rust)                    JS side (Svelte/three.js)
────────────────                    ────────────────────────
KernelSolidHandle ──(never crosses)
KernelId ──────────(never crosses)
OpResult ──────────(never crosses)
Provenance ────────(never crosses)

RenderMesh.vertices ──(TypedArray)──► Float32Array → BufferGeometry
RenderMesh.normals ───(TypedArray)──► Float32Array → BufferGeometry
RenderMesh.indices ───(TypedArray)──► Uint32Array → BufferGeometry
FaceRange ────────────(JSON)────────► JS object for picking
EdgeRenderData ───────(TypedArray)──► Float32Array → LineSegments

FeatureTree ──────────(JSON)────────► JS object for feature tree UI
SolveStatus ──────────(JSON)────────► JS object for status display
GeomRef ──────────────(JSON)────────► JS object for selection
Operation ────────────(JSON)────────► JS object for property editor

UiToEngine ◄──────────(JSON)──────── JS object from UI events
```
