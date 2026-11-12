# Waffle Iron — System Architecture

Waffle Iron is an open-source (MIT) parametric CAD system that runs in the
browser. The workflow is the one mechanical engineers use every day: sketch on
a plane, constrain the sketch, extrude or revolve it, combine bodies with
booleans, pattern them, assemble parts, and produce drawings.

The geometry kernel, the parametric engine and the file format are Rust,
compiled to WebAssembly. The user interface is Svelte with three.js. The same
Rust engine also runs natively, headless, for agents.

## Layers

```
Presentation   Svelte 5 + three.js (Threlte)         main thread
     │  postMessage (JSON) + typed-array mesh transfer
Bridge         wasm-bridge                            Web Worker
     │
Engine         feature-engine, modeling-ops, sketch-solver, file-format
     │  Kernel / KernelIntrospect / KernelMeasure / KernelProjection traits
Kernel         kernel-v2 → yang-rs → cherchi-rs, ssi-rs
```

Each layer talks only to the one below it, through the contracts in
`INTERFACES.md`.

### Kernel layer

The kernel is a stack of crates. Cargo dependencies enforce the layering: a
crate may not import one above it.

```
cad-primitives   shared geometry types and tolerance constants (Point3, Vector3, BoolOp, …)
   ├─ nurbs-rs       B-spline / NURBS math (Piegl & Tiller)
   ├─ cherchi-rs     exact mesh arrangement and mesh boolean (Cherchi 2020 + 2022)
   ├─ ssi-rs         closed-form surface–surface intersection (Patrikalakis ch. 5)
   │
waffle-types     shared types and the kernel contract (depends on cad-primitives, nurbs-rs)
   │
yang-rs          the Yang et al. 2025 hybrid B-Rep/mesh boolean pipeline
   │             (depends on cherchi-rs, ssi-rs)
kernel-v2        B-Rep, Euler operators, primitives, tessellation, STEP/DXF export,
                 and KernelV2Adapter (depends on yang-rs, waffle-types)
```

- **kernel-v2** stores solids as a half-edge B-Rep in an arena and builds them
  with Euler operators (`mvfs`, `mev`, `mef`, `kemr`, `kfmrh`), which keep the
  topology 2-manifold at every step. It constructs extrusions, revolves, pipes
  and sweeps directly; tessellates planar, cylindrical, conical, spherical and
  toroidal faces for rendering; measures volume, area, mass properties,
  distance, interference and wall thickness; projects views and sections for
  drawings; and writes STEP (AP214) and DXF.
- **yang-rs** performs booleans. Surfaces stay analytical throughout; the mesh
  is an exact computational tool for deriving the result's topology. The
  pipeline is: coplanar preprocessing (Stage 0) → tessellation with a bijective
  face mapping (Stage 1) → exact mesh boolean in `cherchi-rs` (Stage 2) →
  topology extraction (Stage 3) → refinement of intersection curves onto the
  exact surface–surface intersections from `ssi-rs` (Stage 4) → B-Rep assembly
  (Stage 5). See `governance/ARCHITECTURAL_INVARIANTS.md` A15.
- **waffle-types** holds the types every crate shares and the kernel contract
  in `waffle_types::kernel`: the `Kernel`, `KernelIntrospect`, `KernelMeasure`
  and `KernelProjection` traits. `kernel_v2::KernelV2Adapter` implements them.
  `MockKernel` (feature `mock-kernel`) is a deterministic test double.

Nothing above the kernel layer names a kernel-internal type. Operations the
kernel does not implement return a typed `KernelError::NotSupported`; they
are never approximated.

Two further crates, `cherchi-sidecar-rs` and `indirect-predicates-sidecar-rs`,
wrap the reference C++ implementations of the Cherchi papers. They are
development-only parity oracles and are not compiled to WASM.
`predicate-gen` generates `cherchi-rs`'s indirect-predicate code.

### Engine layer

- **feature-engine** owns the parametric model. A part is a feature tree: an
  ordered list of operations (sketch, extrude, revolve, boolean, pattern,
  pipe, sweep, datum plane, imported body, script, …). The engine rebuilds the
  tree from the point of change, resolves geometry references across rebuilds
  (persistent naming), evaluates design parameters and expressions, keeps
  undo/redo history, and also evaluates assemblies (instances, mate
  connectors, a mate solver), drawings (sheets, views, annotations) and
  custom feature scripts (Rhai).
- **modeling-ops** executes one operation against the kernel traits. It
  snapshots topology before and after, assigns semantic roles to the faces it
  created, and returns an `OpResult` with the provenance that persistent
  naming is built on.
- **sketch-solver** is a 2D geometric constraint solver: Levenberg-Marquardt
  with analytical Jacobians, degrees-of-freedom analysis, and sketch editing
  operations (trim, extend, offset, fillet, mirror). Closed-profile and region
  extraction live in `waffle-types`.
- **file-format** reads and writes `.waffle` documents (JSON), including
  migration from older format versions. `docs/FILE_FORMAT.md` is the
  specification.
- **step-import** converts STEP text into the neutral imported-body types of
  the kernel contract. It parses with the git-pinned `truck-stepio` crates,
  which do not appear in its public API.
- **kicad-pcb** reads `.kicad_pcb` boards so a PCB can be linked into a
  mechanical design.

### Bridge layer

**wasm-bridge** is the one door between the engine and any host.

- The host sends a `UiToEngine` message as JSON; the bridge dispatches it and
  answers with one `EngineToUi` message as JSON.
- Render meshes do not travel as JSON. After a model update the worker reads
  each body's vertices, normals and indices as typed-array views into WASM
  memory, and the per-face and per-edge geometry references as JSON.
- The bridge holds the open document: its tabs (parts, assemblies, drawings),
  the active tab's engine, the active sketch, selection and hover.
- Agent tools (model inspection, authoring, measurement, drawings, export)
  are implemented once here and reached through `UiToEngine::Tool`.

In the browser the bridge runs in a Web Worker (`app/src/lib/engine/worker.js`);
`app/src/lib/engine/bridge.js` is the main-thread side. Natively,
**waffle-host** wraps the same bridge over stdio as a headless engine for one
document session.

### Presentation layer

The app (`app/`, SvelteKit) renders and edits; it holds no modeling logic.

- `lib/viewport/` — three.js scene via Threlte: shaded bodies, edge overlays,
  picking by raycast against face ranges, camera controls, section caps.
- `lib/sketch/` — 2D sketch drawing tools, constraints, dimensions, snapping.
- `lib/ui/` — toolbar, feature tree, assembly and drawing panels, dialogs,
  tab bar, status bar.
- `lib/drawings/` — drawing sheet layout, SVG and PDF output.
- `lib/storage/` — document storage providers (IndexedDB, git hosts).
- `lib/agent/` — the page side of the agent link.

### Agent link

An MCP client drives Waffle Iron through `relay/` (`waffle-mcp-relay`, Python):
an MCP server on stdio that forwards each tool call either to a paired browser
tab over WebSocket or to a `waffle-host` process. Both run the tool through the
same bridge code, so a tool answers identically in either.

## Data flow

```
User input
    │
    ▼
┌──────────────────────────────────────────────────────────────┐
│  PRESENTATION (main thread)                                  │
│  UI chrome, sketch tools ──► UiToEngine (JSON)               │
│  Viewport ◄── meshes (typed arrays) + face/edge references   │
└──────────────────────────────┬───────────────────────────────┘
                               │ postMessage
                               ▼
┌──────────────────────────────────────────────────────────────┐
│  BRIDGE (Web Worker, or waffle-host natively)                │
│  wasm-bridge: document session, dispatch, agent tools        │
└──────────────────────────────┬───────────────────────────────┘
                               ▼
┌──────────────────────────────────────────────────────────────┐
│  ENGINE                                                      │
│  feature-engine: feature tree, rebuild, reference            │
│                  resolution, parameters, undo/redo,          │
│                  assemblies, drawings, scripts               │
│  modeling-ops:   one operation → OpResult with provenance    │
│  sketch-solver:  constraint solve                            │
└──────────────────────────────┬───────────────────────────────┘
                               │ kernel traits
                               ▼
┌──────────────────────────────────────────────────────────────┐
│  KERNEL                                                      │
│  kernel-v2 (KernelV2Adapter): B-Rep, construction,           │
│     tessellation, introspection, measurement, projection,    │
│     STEP / DXF export                                        │
│  yang-rs → cherchi-rs + ssi-rs: booleans                     │
└──────────────────────────────────────────────────────────────┘
```

During sketch editing the loop is shorter: the sketch tools send the entities
and constraints, the bridge runs `sketch-solver`, and the solved positions,
status and closed profiles come back for display.

## Crate map

| Crate | Purpose |
|---|---|
| `cad-primitives` | Shared geometry types and tolerance constants |
| `nurbs-rs` | B-spline / NURBS math |
| `waffle-types` | Shared types; the kernel contract; `MockKernel` |
| `cherchi-rs` | Exact mesh arrangement and mesh boolean |
| `ssi-rs` | Closed-form surface–surface intersection |
| `yang-rs` | Hybrid B-Rep/mesh boolean pipeline |
| `kernel-v2` | B-Rep kernel and `KernelV2Adapter` |
| `sketch-solver` | 2D constraint solver |
| `modeling-ops` | Operation execution with provenance |
| `feature-engine` | Feature tree, rebuild, persistent naming, assemblies, drawings, scripts |
| `file-format` | `.waffle` save, load, migration |
| `step-import` | STEP reader |
| `kicad-pcb` | KiCad board reader |
| `wasm-bridge` | Engine ↔ host protocol; agent tools |
| `waffle-host` | Native headless host of the engine |
| `test-harness` | Cross-crate integration tests and the boolean assay |
| `cherchi-sidecar-rs`, `indirect-predicates-sidecar-rs` | Reference-parity oracles (development only) |
| `predicate-gen` | Generator for `cherchi-rs` predicates (development only) |

Kernel-stack work is planned in `docs/kernel_1_0.md` and
`docs/yang_faithfulness_roadmap.md`. The other sub-projects keep a dossier
(architecture, plan, interfaces) under `projects/`.

## Design principles

**Interfaces first.** Crates depend on each other's contracts, not internals.
`INTERFACES.md` describes the contracts; the Rust source is normative.

**Deterministic output.** The same input produces the same result: no random
values, no system-time dependence, no unordered iteration that reaches output.

**Analytical primacy.** Planes, cylinders, cones, spheres and tori stay exact
through every operation, booleans included. Tessellation is an output, never
the stored form.

**Loud failure.** An operation that cannot produce a correct result returns a
typed error. There are no tolerance-widening retries and no fallback paths
that produce a plausible wrong answer (Constitution P9, P10).

**Mock-driven development.** Engine crates are unit-tested against
`MockKernel`, so they can be developed without touching the real kernel;
real-geometry tests use `KernelV2Adapter`.

**Tests are permanent.** The suite only grows. A wrong test is fixed, not
deleted.

**Rust produces data; three.js renders.** Rust crates return meshes, entity
lists and solve results. All drawing to the screen happens in the app.

## Kernel capability boundaries

What the kernel builds and combines today: extrusions, revolves (including
cones, spheres and tori), pipes, sweeps, rigid and mirrored copies, and
union / subtract / intersect over solids bounded by planes, cylinders, cones,
spheres and tori — including chained booleans and multi-body results. It
imports STEP bodies either exactly (analytic shells) or as mesh-backed bodies,
and exports STEP with exact surfaces.

What it refuses, with a typed error:

- **Fillet, chamfer, shell** — not implemented; `NotSupported`.
- **Coplanar boolean inputs the Stage-0 overlay cannot resolve.**
- **Holed revolve profiles**, and arc or spline profiles that carry no chord
  polygon.
- **Booleans on mesh-backed imported bodies.**
- Specific boolean geometries where a pipeline stage cannot certify its result
  stop with a typed stage error rather than emit a doubtful solid.

Boolean correctness is measured by the categorized assay
(`crates/test-harness/tests/assay_kv2.rs`, corpus in
`app/tests/cases/assay/`) and by parity against the reference C++ sidecars.
`docs/TESTING.md` explains how to run both. The latest per-case results are in
`app/tests/cases/assay/results.json`; the failing cases are triaged in
`docs/yang_tail_triage.md`.
