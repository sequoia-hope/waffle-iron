# Waffle Iron — System Architecture

## Vision

Waffle Iron is the "KiCad of mechanical CAD" — an open-source parametric CAD system that replaces Onshape for daily mechanical design work. GPL-3.0 licensed, community-driven, built for the workflow engineers actually use: sketch on plane → constrain sketch → extrude/revolve → fillet/chamfer → pattern → assemble. The architecture prioritizes determinism, testability, and autonomous agent development.

## Architecture Overview

The system has four layers:

### Kernel Layer (Rust, compiled to WASM)

**truck fork** — BREP geometry, NURBS surfaces, boolean operations, tessellation. This is our fork of the [truck](https://github.com/ricosjp/truck) crate ecosystem. The kernel provides the geometric foundation: constructing solids via sweeps, computing booleans, tessellating BREP to triangle meshes, and introspecting topology (listing faces, edges, vertices and their relationships). All truck types are wrapped behind the `Kernel` and `KernelIntrospect` traits — no truck types leak to other layers.

### Engine Layer (Rust, compiled to WASM, runs in Web Worker)

Three crates that implement the parametric modeling logic:

- **feature-engine** — The parametric modeling brain. Manages the feature tree (ordered list of modeling operations), persistent naming (GeomRef system for stable geometry references across rebuilds), rebuild algorithm (replay features from change point), and undo/redo.
- **modeling-ops** — Individual operation implementations (extrude, revolve, fillet, chamfer, shell, boolean combine). Each operation calls the Kernel trait, introspects the result, assigns semantic roles to created geometry, and returns a complete OpResult with provenance for persistent naming.
- **sketch-solver** — Wraps the `slvs` crate (SolveSpace's libslvs) for 2D geometric constraint solving. Maps Waffle Iron sketch types to libslvs calls, runs the solver, extracts solved positions and closed profiles.

### Bridge Layer (Rust/WASM + JavaScript glue, runs in Web Worker)

**wasm-bridge** — Protocol between the WASM engine and the JS presentation layer. Commands flow JS → WASM (postMessage to Worker → dispatch to engine). Tessellated mesh data flows WASM → JS as TypedArray views into WASM linear memory for near-zero-copy transfer. Only model changes trigger mesh transfer, not per-frame. BREP topology stays entirely in WASM — JS sees only opaque handles and tessellated output.

### Presentation Layer (Svelte + three.js/Threlte, runs on main thread)

- **3d-viewport** — three.js rendering via Threlte (declarative three.js for Svelte). Receives tessellated mesh data from wasm-bridge. Handles camera controls, entity picking via raycasting with face-range metadata, hover/selection highlighting, sketch-mode transparency.
- **sketch-ui** — 2D sketch editing interface. Drawing tools, constraint application, dimension editing, auto-constraining, visual feedback for constraint status.
- **ui-chrome** — Application shell. Feature tree panel, toolbar, property editor, status bar. All communication with the engine via wasm-bridge messages.

## Data Flow

```
User Input
    │
    ▼
┌─────────────────────────────────────────────────────────────┐
│  PRESENTATION LAYER (Main Thread)                           │
│                                                             │
│  Svelte UI (ui-chrome)                                      │
│    ├── Feature Tree Panel ──┐                               │
│    ├── Toolbar              │                               │
│    ├── Property Editor      ├── UiToEngine messages         │
│    └── Status Bar           │                               │
│                             ▼                               │
│  sketch-ui ────────────► UiToEngine messages                │
│                             │                               │
│  3d-viewport (Threlte) ◄── EngineToUi (RenderMesh,         │
│    ├── Shaded faces         │  selection, status)           │
│    ├── Edge overlays        │                               │
│    ├── Picking/selection    │                               │
│    └── Camera controls      │                               │
└─────────────────────────────┼───────────────────────────────┘
                              │ postMessage
                              ▼
┌─────────────────────────────────────────────────────────────┐
│  BRIDGE LAYER (Web Worker)                                  │
│                                                             │
│  wasm-bridge                                                │
│    ├── Deserialize UiToEngine ──► dispatch to engine        │
│    ├── Serialize EngineToUi ────► postMessage to main       │
│    └── TypedArray mesh transfer (near-zero-copy)            │
└─────────────────────────────┼───────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│  ENGINE LAYER (WASM in Web Worker)                          │
│                                                             │
│  feature-engine                                             │
│    ├── Feature tree management                              │
│    ├── Rebuild algorithm (replay from change point)         │
│    ├── GeomRef resolution (persistent naming)               │
│    └── Undo/redo                                            │
│         │                                                   │
│         ▼                                                   │
│  modeling-ops                                               │
│    ├── Extrude, Revolve, Fillet, Chamfer, Shell             │
│    ├── Topology diff (before/after)                         │
│    └── Provenance + role assignment → OpResult              │
│         │                                                   │
│         ▼                                                   │
│  sketch-solver (slvs/libslvs)                               │
│    ├── Constraint solving (Newton-Raphson)                   │
│    ├── Solve status (fully/under/over-constrained)          │
│    └── Profile extraction (closed loops for extrusion)      │
└─────────────────────────────┼───────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│  KERNEL LAYER (WASM)                                        │
│                                                             │
│  kernel-fork (truck)                                        │
│    ├── BREP construction (tsweep, rsweep)                   │
│    ├── Boolean operations (union, subtract, intersect)      │
│    ├── Topology introspection (faces, edges, vertices)      │
│    ├── Tessellation → RenderMesh with face-range metadata   │
│    └── STEP I/O (AP203, limited)                            │
└─────────────────────────────────────────────────────────────┘
```

Sketch data flow (during sketch mode):

```
sketch-ui (draw/constrain)
    │ UiToEngine::AddSketchEntity / AddConstraint / SolveSketch
    ▼
wasm-bridge → sketch-solver (slvs)
    │ SolvedSketch (positions + status)
    ▼
wasm-bridge → sketch-ui (update display, color by status)
```

## Sub-Project Map

| # | Project | Purpose | Technology | Dependencies | Status |
|---|---------|---------|------------|-------------|--------|
| 01 | kernel-fork | BREP geometry via truck fork | Rust | None | Not started |
| 02 | sketch-solver | 2D constraint solving via slvs | Rust + C (libslvs) | None | Not started |
| 03 | wasm-bridge | WASM↔JS communication protocol | Rust + JS | 01 | Not started |
| 04 | 3d-viewport | three.js rendering via Threlte | Svelte + JS | 01 | Not started |
| 05 | sketch-ui | 2D sketch editing interface | Svelte + JS | 02, 03, 04 | Not started |
| 06 | feature-engine | Parametric feature tree + persistent naming | Rust | 01 | Not started |
| 07 | modeling-ops | Operation implementations with provenance | Rust | 01 | Not started |
| 08 | ui-chrome | Application shell (panels, toolbar, tree) | Svelte | 05, 06, 07 | Not started |
| 09 | file-format | Save/load/export | Rust | 06 | Not started |
| 10 | assemblies | Multi-part assembly (deferred) | Rust + Svelte | All | Not started |

### Dependency Graph

```
Phase 1 (parallel):  01-kernel-fork + 02-sketch-solver
Phase 2 (parallel):  03-wasm-bridge + 04-3d-viewport        (depend on 01)
Phase 3:             05-sketch-ui                            (depends on 02, 03, 04)
Phase 4 (parallel):  06-feature-engine + 07-modeling-ops     (depend on 01)
Phase 5:             08-ui-chrome                            (depends on 05, 06, 07)
Phase 6:             09-file-format                          (depends on 06)
Phase 7:             10-assemblies                           (deferred)
```

## Key Design Principles

### Interfaces First

All cross-crate contracts are defined as Rust traits and types in `INTERFACES.md` before implementation begins. All cross-language contracts (WASM ↔ JS) are defined as message schemas before implementation. No crate may depend on another crate's internal types — only on shared interface types.

### Deterministic Outputs

Same inputs must always produce the same results. This is critical for testing and for agent-driven development where reproducibility enables debugging. truck-meshalgo uses deterministic hashing for tessellation — this must be preserved. No random values, no system-time dependencies, no non-deterministic iteration orders.

### Mock-Driven Development

Every Rust crate can be tested against mock implementations of its dependencies. The `MockKernel` (which implements the `Kernel` and `KernelIntrospect` traits with deterministic synthetic topology) is as important as `TruckKernel`. Agents can develop and test feature-engine and modeling-ops without a working truck build.

### Session-Independent

Every agent session starts from docs + code + tests. No implicit knowledge is required. An agent reading ARCHITECTURE.md, INTERFACES.md, and their sub-project's CLAUDE.md has everything needed to contribute.

### Test as Ratchet

The test suite only grows. Passing tests must never be deleted. If a test is wrong, fix the test. Tests are the permanent record of what the system does.

### three.js for Rendering

All 3D rendering happens in JavaScript via three.js/Threlte on the main thread. Rust/WASM produces tessellated meshes with face-range metadata for picking. Rust does NOT render anything. This boundary is absolute.

## Known Kernel Gaps

These are limitations of the truck crate ecosystem that every sub-project must account for:

### Boolean Performance Crisis

GitHub Issue #68 documents a cube-cylinder boolean taking **13–15 seconds on an M1 MacBook**. Tolerance parameter strongly affects speed and stability (tol=0.9 → ~4s, tol=0.2 → ~15s, tol=1.0 → panic). Commercial kernels (Parasolid, ACIS) perform equivalent operations in single-digit milliseconds. This is the single largest technical risk.

### Boolean Robustness

Known boolean failures: cone apex (degenerate normal), coplanar faces, near-tangent surfaces, small slivers. Operations return `Option<Solid>` — `None` on failure with no diagnostic information.

### No Subtraction Primitive

Boolean subtraction requires `solid.not()` (flip normals) + `and` (intersection). No dedicated difference function exists.

### Boolean Results Cannot Be Exported to STEP

The STEP writer does not handle post-boolean solids. This limits export capabilities.

### No Fillets or Chamfers

Docs explicitly state: "Now, one cannot make a fillet... by truck." `RbfSurface` and `ApproxFilletSurface` exist as geometry surface types only — not modeling operations. No `solid.fillet(edge, radius)` API. No chamfer. No variable radius. No vertex blending. Fillets require building: surface generation, face trimming, topology reconstruction.

### No Persistent Naming Infrastructure

Topology entities (Vertex, Edge, Face) have runtime IDs based on `Arc` reference counting. IDs are stable within a session but NOT persistent through topological modifications. Booleans create new objects with new IDs. No old → new mapping is provided. Solid has no ID at all. No operation journals, no entity mapping, no history tracking. Waffle Iron must build its own persistent naming system (GeomRef).

### No Dedicated Primitives

No `box()`, `cylinder()`, `sphere()` functions. Everything is built via successive sweeps. A cube is four `tsweep` calls from a vertex. A sphere is `rsweep` on a semicircle.

### Tessellation: Single Control Knob

`MeshableShape::triangulation(tol)` provides chordal tolerance as the only parameter. No adaptive refinement, no LOD, no curvature-based density.

### STEP I/O Limitations

AP203 only. No AP214 (colors/layers) or AP242 (modern standard). README warns: "DO NOT USE FOR PRODUCT." Import is incomplete. Assembly structure reading is in progress.

### Assembly Support Not Ready

truck-assembly is recently added, not published to crates.io, and provides only positional grouping from STEP files. No constraint solving, no mates.

## Architectural Precedent: CADmium

[CADmium](https://github.com/CADmium-Co/CADmium) (archived September 2025, 1.6k GitHub stars) used exactly the stack we're targeting: truck (Rust) + SvelteKit + Tailwind + three.js via Threlte + Tauri + JSON feature storage. It validated the pattern of BREP kernel in WASM with three.js rendering, but did not reach production quality.

CADmium was released under Elastic License 2.0 (incompatible with our GPL-3.0 goals). A relicense conversation with the author may be pursued. Regardless of outcome, our architecture is self-sufficient — CADmium's code is reference material, not a dependency.

The same pattern (BREP in WASM, tessellate in WASM, render in three.js) is used by OpenCascade.js-based tools, Replicad, and Chili3D. It is the industry-standard approach for browser-based CAD.
