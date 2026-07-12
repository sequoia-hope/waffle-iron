# Waffle Iron — System Architecture

> **⚠ Kernel sections below describe the DELETED legacy kernel.** The Phase 6
> migration (2026-06-11) removed `crates/kernel/` entirely; the app runs on
> the layered kernel stack — `cad-primitives` → `cherchi-rs` / `ssi-rs` (+ the
> two non-WASM C++ sidecar crates, dev-only oracles) → `yang-rs` →
> `kernel-v2` — behind the `Kernel`/`KernelIntrospect` traits in
> `waffle_types::kernel` (implemented by `kernel_v2::KernelV2Adapter`). See
> root `CLAUDE.md` §"Kernel: kernel-v2" and `docs/yang_functional_roadmap.md`
> (the plan of record). Historical references to `crates/kernel/`, its test
> counts and assay targets below are retained as history; the non-kernel
> layers (engine, bridge, presentation) remain accurate.

## Vision

Waffle Iron is the "KiCad of mechanical CAD" — an open-source parametric CAD system that replaces Onshape for daily mechanical design work. MIT licensed, community-driven, built for the workflow engineers actually use: sketch on plane → constrain sketch → extrude/revolve → fillet/chamfer → pattern → assemble. The architecture prioritizes determinism, testability, and autonomous agent development.

## Architecture Overview

The system has four layers:

### Kernel Layer (Rust, compiled to WASM)

**The kernel is a layered Rust stack** (the monolithic `crates/kernel/` was deleted at the Phase-6 migration, 2026-06-11):

```
cad-primitives  — shared geometry types & constants (Point3, Vector3, BoolOp, …)
      │
waffle-types    — public types + the Kernel/KernelIntrospect contract (traits, units, MockKernel)
      │
cherchi-rs      — Cherchi 2020+2022 exact mesh boolean (clean-room predicates, WASM-clean)
ssi-rs          — analytical SSI solvers (Patrikalakis Ch.5)
      │
yang-rs         — Yang 2025 hybrid B-Rep/mesh boolean pipeline (deps cherchi-rs + ssi-rs)
      │
kernel-v2       — clean B-Rep + Euler ops + tessellation + the trait adapter (deps yang-rs)
```

The app, feature-engine, and all tests reach geometry through the `Kernel` /
`KernelIntrospect` traits in `waffle_types::kernel`, implemented by
`kernel_v2::KernelV2Adapter`. Dependency layering is compiler-enforced per
`Cargo.toml`; no kernel internals leak to other layers. **Analytical primacy**:
boolean operations on quadric surfaces (plane, cylinder, cone, sphere, torus)
survive as exact analytical geometry through the hybrid pipeline [Ref #24: Yang
et al. 2025] — the mesh path is an *exact computational tool* for deriving
correct B-Rep topology, not a degradation (see
governance/ARCHITECTURAL_INVARIANTS.md A15). The two non-WASM C++ sidecar
crates (`cherchi-sidecar-rs`, `indirect-predicates-sidecar-rs`) are dev-only
reference-parity oracles, never shipped. Progress is tracked via the
categorized kernel-v2 assay; see "Current Kernel Status" below. The previous
truck-based kernel and the clean-sheet `crates/kernel/` that replaced it both
served historically (see git history).

### Engine Layer (Rust, compiled to WASM, runs in Web Worker)

Three crates that implement the parametric modeling logic:

- **feature-engine** — The parametric modeling brain. Manages the feature tree (ordered list of modeling operations), persistent naming (GeomRef system for stable geometry references across rebuilds), rebuild algorithm (replay features from change point), and undo/redo.
- **modeling-ops** — Individual operation implementations (extrude, revolve, boolean combine). Each operation calls the Kernel trait, introspects the result, assigns semantic roles to created geometry, and returns a complete OpResult with provenance for persistent naming. (Fillet, chamfer, and shell operations exist experimentally but are deferred indefinitely pending boolean reliability.)
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
│    ├── Extrude, Revolve, Boolean Combine                    │
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
│  kernel-v2 (via KernelV2Adapter → yang-rs → cherchi/ssi)    │
│    ├── Topology: Euler operators (half-edge B-Rep)          │
│    ├── Boolean operations (Yang hybrid B-Rep/mesh pipeline) │
│    ├── Topology introspection (faces, edges, vertices)      │
│    ├── Tessellation → RenderMesh with face-range metadata   │
│    └── STEP import: separate crates/step-import (truck as   │
│        parser only); STEP export: NotSupported (roadmap)    │
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
| 01 | kernel-v2 stack | Layered B-Rep kernel: `cad-primitives` + `waffle-types` → `cherchi-rs` / `ssi-rs` → `yang-rs` → `kernel-v2` (adapter `KernelV2Adapter`) | Rust | None | Live (Yang M0–M2/M6/M7 + Phase-6 migration COMPLETE 2026-06-11; assay 240/295 CORRECT, 0 WRONG; remaining walls M8 coplanar / §4.3.3 near-tangency / non-convex CDT) |
| 02 | sketch-solver | 2D constraint solving via slvs | Rust + C (libslvs) | None | Complete (M1-M10 + Emscripten WASM) |
| 03 | wasm-bridge | WASM↔JS communication protocol | Rust + JS | 01 | Complete (M1-M8) |
| 04 | 3d-viewport | three.js rendering via Threlte | Svelte + JS | 01 | Complete |
| 05 | sketch-ui | 2D sketch editing interface | Svelte + JS | 02, 03, 04 | Complete |
| 06 | feature-engine | Parametric feature tree + persistent naming | Rust | 01 | Complete (M1-M10; fillet/chamfer/shell deferred) |
| 07 | modeling-ops | Operation implementations with provenance | Rust | 01 | Complete (M1-M10) |
| 08 | ui-chrome | Application shell (panels, toolbar, tree) | Svelte | 05, 06, 07 | Complete |
| 09 | file-format | Save/load/export | Rust | 06 | Complete (M1-M6) |
| 10 | assemblies | Multi-part assembly (deferred) | Rust + Svelte | All | Deferred |

### Dependency Graph

```
Phase 1 (parallel):  01-kernel + 02-sketch-solver
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

Same inputs must always produce the same results. This is critical for testing and for agent-driven development where reproducibility enables debugging. Tessellation uses deterministic hashing — this must be preserved. No random values, no system-time dependencies, no non-deterministic iteration orders.

### Mock-Driven Development

Every Rust crate can be tested against mock implementations of its dependencies. The `MockKernel` (which implements the `Kernel` and `KernelIntrospect` traits with deterministic synthetic topology, behind the `mock-kernel` feature) is as important as the real `KernelV2Adapter`. Agents can develop and test feature-engine and modeling-ops without a working kernel build.

### Session-Independent

Every agent session starts from docs + code + tests. No implicit knowledge is required. An agent reading ARCHITECTURE.md, INTERFACES.md, and their sub-project's CLAUDE.md has everything needed to contribute.

### Test as Ratchet

The test suite only grows. Passing tests must never be deleted. If a test is wrong, fix the test. Tests are the permanent record of what the system does.

### three.js for Rendering

All 3D rendering happens in JavaScript via three.js/Threlte on the main thread. Rust/WASM produces tessellated meshes with face-range metadata for picking. Rust does NOT render anything. This boundary is absolute.

## Current Kernel Status

The live kernel is the layered `kernel-v2` stack (see "Kernel Layer" above);
the monolithic `crates/kernel/` was deleted at the Phase-6 migration
(2026-06-11). The app, feature-engine, and every test run on this stack
through the `Kernel` / `KernelIntrospect` traits on stable Rust with standard
`wasm-pack`. The plan of record is **`docs/yang_functional_roadmap.md`**.

The correctness oracle is the categorized kernel-v2 assay
(`cargo test -p test-harness --test assay_kv2 -- --ignored --nocapture`) plus
reference parity against the Cherchi C++ sidecars. The assay corpus is
**295 cases** with analytical ground truth and Euler-characteristic oracles,
scored in five exhaustive buckets:

**240 CORRECT · 0 WRONG · 51 ERROR · 4 UNSUPPORTED · 0 TIMEOUT** (2026-07-12).

### What exists (live):
- Half-edge B-Rep topology (arena-based storage, tombstoned ids) with Euler
  operators (mvfs, mev, mef, kemr, kfmrh) and whole-arena invariant validation
  at every op exit (`kernel-v2`)
- Yang 2025 hybrid B-Rep/mesh boolean pipeline (`yang-rs`): coplanar
  preprocessing (Stage 0) → bijective tessellation → exact mesh boolean
  (`cherchi-rs`: Cherchi 2020 indirect predicates + 2022 ray-cast in/out) →
  topology extraction → SSI refinement → B-Rep assembly. **Milestones M0, M1,
  M2, M6, M7 and the Phase-6 migration are COMPLETE**; the kernel is live in
  the app
- Analytical SSI solvers (`ssi-rs`, Ref: Patrikalakis Ch.5) for the non-torus
  quadric pairs, feeding Stage-4 geometry refinement; closed-form conics + the
  M5 procedural `SurfacePair` for general-position cyl×cyl / cyl×cone /
  cone×cone. Torus-pair geometry is produced above `ssi-rs` (coaxial-rim
  recovery + Stage-4 implicit relocation). See `/specs/ssi_solver_matrix.md`
- Extrude, revolve (incl. sphere/torus profiles), and boolean union/subtract/
  intersect over box/cyl/cone/sphere/torus operands
- Geometry-driven tessellation for planar, cylindrical, conical, spherical,
  and toroidal faces
- `MockKernel` (deterministic test double, `waffle-types` `mock-kernel` feature)
  for consumer-crate unit tests; `KernelV2Adapter` for real-geometry tests

### Remaining capability walls (typed `NotSupported`/error, loud — ROADMAP, not bugs):
- **M8 coplanar-boolean tail** — the last flush/stacked-face coplanar cases
  (Stage 0); the bulk of the class has shipped, a small residue remains
- **§4.3.3 near-tangency** — face-gap-under-sagitta cases where mesh topology
  can diverge from exact topology
- **Non-convex CDT profile tail** — gear / arc-segment profiles
- **STEP export** — trait-default `NotSupported` (STEP *import* is the separate
  `crates/step-import`, truck pinned as parser only)

The dominant remaining ERROR class is Stage-4 `LocalRefinementRequired` (the N2
mesh-updating gap). When a milestone lands, its `#[ignore]`/`test.skip`
quarantines are un-gated in the same PR (grep the milestone tag, e.g. `KV6`,
`M8`, `M5`).

### Deferred indefinitely:
- Fillet, chamfer, shell operations
- Assembly support

---

### HISTORICAL (pre-2026-06-11, `crates/kernel/` — retained for context)

The deleted clean-sheet kernel (`crates/kernel/`) tracked a **980-test** suite
(28 ignored) against a **190-case** randomized assay corpus (seed 42) with a
`WaffleKernel` implementation and an S-H clipping + tolerance-escalation
boolean path. That path — and its failure mode of masking classification
errors with tolerance widening and synthetic fill triangles — was deleted with
the crate at the Phase-6 migration and is the cautionary tale behind
Constitution P9/P10. The Yang "Phase 1–5 switchover" language that used to
live here is superseded by the milestone roadmap above. Before it,
`crates/kernel/` had itself replaced a truck-based kernel (through Sprint 67).

## Architectural Precedent: CADmium

[CADmium](https://github.com/CADmium-Co/CADmium) (archived September 2025, 1.6k GitHub stars) used exactly the stack we're targeting: truck (Rust) + SvelteKit + Tailwind + three.js via Threlte + Tauri + JSON feature storage. It validated the pattern of BREP kernel in WASM with three.js rendering, but did not reach production quality.

CADmium was released under Elastic License 2.0 (incompatible with our MIT goals). A relicense conversation with the author may be pursued. Regardless of outcome, our architecture is self-sufficient — CADmium's code is reference material, not a dependency.

The same pattern (BREP in WASM, tessellate in WASM, render in three.js) is used by OpenCascade.js-based tools, Replicad, and Chili3D. It is the industry-standard approach for browser-based CAD.
