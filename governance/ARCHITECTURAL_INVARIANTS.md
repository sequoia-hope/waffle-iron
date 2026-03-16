# Architectural Invariants (v1)

This document defines **non-negotiable architectural constraints** for Waffle Iron.

It is **prescriptive**. It exists to prevent architecture erosion as autonomous agents develop the system.

- `ARCHITECTURE.md` is the descriptive source of intent.
- This file extracts the subset of architectural rules that must remain stable.
- In case of ambiguity, prefer interpretations that maximize determinism, testability, and separation of concerns.

Any change to these invariants requires explicit human approval (per ENGINEERING_CONSTITUTION.md).

---

## A0. Scope

These invariants apply to all changes touching:

- Rust crates in `/crates` (kernel, ops, engine, bridge, file format, tests)
- Frontend in `/app`
- The JS↔WASM boundary and messaging protocol
- Any persistence/serialization format
- Any infrastructure that affects runtime determinism or correctness

---

## A1. Layering and Dependency Direction

### A1.1 Layer model

The system is logically layered:

1. **Kernel layer** (geometry/topology/booleans/tessellation core)
2. **Engine layer** (feature tree, parametric rebuild, persistent naming, ops orchestration)
3. **Bridge layer** (WASM worker boundary + message protocol)
4. **Presentation layer** (Svelte UI + viewport rendering + interaction)

### A1.2 Allowed dependency direction

Dependencies may point only “down”:

- Presentation → Bridge → Engine → Kernel

Reverse dependencies are forbidden.

Specifically:

- Kernel must not depend on Engine, Bridge, or Presentation.
- Engine must not depend on Bridge or Presentation.
- Bridge must not depend on Presentation frameworks (Svelte, Threlte, etc.).
- Presentation must not implement modeling logic that belongs in Engine/Kernel.

### A1.3 No cross-layer shortcuts

Shortcuts that bypass layers are not allowed (even if convenient), including:

- UI reading kernel internals directly
- JS performing geometry computations that change modeling truth
- Modeling ops implemented in UI code

---

## A2. WASM Boundary and Worker Isolation

### A2.1 Geometry state lives in WASM

All authoritative model state (BREP/topology/feature graph state) must live in Rust/WASM.

JS must treat the engine as authoritative and must not duplicate state in a way that can diverge.

### A2.2 Worker boundary is real

The engine must run in a Web Worker (or equivalent isolation) and communicate via explicit messages.

- No direct synchronous calls from UI into the kernel state.
- No sharing of mutable model objects across the boundary.

### A2.3 Transfer only what is necessary

Across the boundary, transfer only:

- Mesh buffers (positions, normals, indices)
- Picking/selection metadata (IDs, primitive mapping)
- Compact UI-friendly summaries (feature list, parameter schemas, error diagnostics)

Do not transfer BREP graphs or kernel objects.

### A2.4 Typed message protocol

Bridge messages must be:

- Typed
- Versionable
- Backward-compatible when feasible

Breaking changes to protocol require explicit version increments and migration strategy.

---

## A3. Kernel Encapsulation and Vendor Containment

### A3.1 Vendor types must not leak

If the kernel uses vendored libraries (e.g., `truck`), types from vendor crates must not leak beyond the kernel boundary.

Engine and bridge APIs must use Waffle Iron-owned types (e.g., `waffle-types`).

### A3.2 Stable internal abstractions

Kernel must expose a stable interface via traits or façade APIs such as:

- Kernel operations (boolean, sweep, loft, fillet, tessellate)
- Introspection (faces/edges/vertices, surface types, adjacency)
- Robust error reporting

The goal is to allow kernel replacement without rewriting the engine.

### A3.3 Single ownership of tolerance policy

Tolerance/epsilon strategy must be centralized in kernel (or a dedicated shared crate), not scattered.

No module may introduce ad-hoc epsilons without using the shared policy.

---

## A4. Engine Responsibilities (Parametric Model Truth)

### A4.1 Feature graph is authoritative

The engine owns:

- Feature graph / feature tree
- Rebuild order
- Undo/redo
- Parameter schemas/defaults
- Persistent naming strategy (IDs that survive rebuild)

UI may display and edit, but the engine computes truth.

### A4.2 Deterministic rebuild

Given the same inputs and tolerances:

- Rebuild results must be deterministic
- The same features should produce the same topology IDs as much as the persistent naming system allows

Nondeterministic iteration orders are forbidden where they affect output.

### A4.3 No hidden state

Engine logic must not depend on hidden global state:

- No global mutable singletons
- No hidden caches that change results without explicit invalidation rules

Caches are allowed only if they are:
- deterministic
- invalidation is explicit and testable

---

## A5. Presentation Responsibilities (UI/UX Only)

### A5.1 UI does interaction, not modeling

The UI is responsible for:

- Interaction state (tool modes, hover state, selection UI)
- Viewport rendering
- Event handling and gestures
- Displaying feature tree and parameters
- Editing parameters and sending commands to engine

The UI is not responsible for:

- Computing geometry outcomes
- Resolving constraint systems
- Maintaining authoritative feature graph

### A5.2 Viewport is derived data

All rendered geometry is derived from:

- Mesh buffers produced by WASM
- Camera/lighting state in UI

If visual output is wrong, the fix belongs in either:
- meshing/tessellation logic, or
- UI rendering logic,

but not by inventing alternate geometry truth in JS.

---

## A6. Bridge Contract (Command/Query Discipline)

### A6.1 Separate commands from queries

Bridge API should distinguish:

- **Commands**: mutate model (add feature, edit param, delete feature)
- **Queries**: read model summaries (feature list, selection info, error messages)

This keeps behavior predictable and enables structured testing.

### A6.2 Errors are structured

All errors crossing the bridge must be:

- structured (codes/types + details)
- actionable when possible
- stable enough for UI to render consistently

Avoid raw string-only errors as the primary interface.

---

## A7. File Format and Persistence

### A7.1 Serialization is owned and versioned

The project owns its file format and versioning policy.

- All persisted formats must include version fields.
- Changes must include migration logic or explicit incompatibility policy.

### A7.2 Persist engine truth, not UI state

Persist:

- feature graph
- parameters
- persistent IDs
- references (selection intent) where supported

Do not persist:

- camera position
- transient tool states
- hover state

(Those may be saved optionally as user preferences, but not as modeling truth.)

---

## A8. Numeric Robustness and Tolerance Governance

### A8.1 Central tolerance policy

A single tolerance policy must govern:

- near-equality checks
- vertex merging
- edge coincidence
- face-plane comparisons
- snapping thresholds

Any operation introducing a new tolerance must:
- reuse the central policy, or
- explicitly extend it with documented reasoning

### A8.2 No silent “healing” without diagnostics

Geometry “healing” is allowed, but must be:

- explicit
- testable
- accompanied by diagnostics (what was healed, why)

Silent magic that masks errors is forbidden.

---

## A9. Observability and Debuggability

### A9.1 Operations must be diagnosable

Modeling ops must produce sufficient metadata to debug:

- operation inputs
- key intermediate decisions (where feasible)
- failure causes and locations

### A9.2 Deterministic debug artifacts

Where possible:

- failing cases should be reproducible by a minimal serialized fixture
- test cases should emit stable diagnostics

---

## A10. Performance and Responsiveness Constraints

### A10.1 UI thread must remain responsive

Heavy modeling work must not run on the UI thread.

- Use worker boundary
- Use message passing
- Use incremental updates where possible (e.g., streaming meshes)

### A10.2 Mesh generation is decoupled

Tessellation/meshing should be:

- stable (deterministic)
- parameterized by tolerance
- adjustable without changing modeling truth

---

## A11. Testing Implications of the Architecture

### A11.1 Kernel tests are pure

Kernel-level tests should:

- avoid UI dependencies
- operate on kernel APIs directly
- validate numeric invariants and robustness

### A11.2 Engine tests validate rebuild semantics

Engine tests should validate:

- feature tree rebuild correctness
- persistent naming stability
- undo/redo invariants
- deterministic rebuild

### A11.3 Bridge tests validate protocol and memory safety

Bridge tests should validate:

- message schema correctness
- backward compatibility (when applicable)
- no invalid memory exposure patterns
- deterministic data transfer

---

## A13. Research-Grounded Architecture

### A13.1 Algorithm selection must cite literature
When multiple approaches exist for a geometric algorithm (SSI, classification,
boolean pipeline, tessellation), the chosen approach must cite a reference from
REFERENCES.md with rationale for why it was selected.

### A13.2 SYSTEM_DESIGN.md is the architectural research map
The file `/docs/SYSTEM_DESIGN.md` maintains a module-level architecture diagram
with research annotations. Each major subsystem cites the references that inform
its design. This document must be updated when architectural decisions change.

### A13.3 No reinventing solved problems
If a peer-reviewed, published algorithm exists for a problem, it must be used
(or explicitly justified why not). Ad-hoc geometric algorithms that duplicate
published work are considered technical debt.

---

## A14. Units and Tolerance Policy

### A14.1 All distances in meters

The kernel's native unit for all geometric coordinates and distances is meters.
All external inputs are assumed to be in meters unless explicitly converted.

### A14.2 Feature size floor

The kernel must support features down to 1 micrometer (1e-6 m). The model
tolerance (TAU_MODEL = 1e-7 m) is one order of magnitude below this to prevent
features from being lost to numerical rounding.

### A14.3 Centralized tolerance constants

No absolute tolerance constants may exist outside the kernel's `units.rs` module.
All tolerances in the boolean pipeline, tessellation, and validation flow from
`BooleanOptions` or the constants in `units.rs` (TAU_MODEL, TAU_WORK, MIN_FEATURE_SIZE).
Ad-hoc epsilon values in other crates are forbidden.

---

## A15. Analytical Primacy

### A15.1 Exact SSI for analytical surface pairs

Boolean operations on quadric surfaces (plane, cylinder, cone, sphere, torus) MUST
use exact surface-surface intersection (SSI) algorithms. The mesh/polygon boolean
path exists solely for freeform surfaces (NURBS/BSpline) that lack closed-form SSI.

When a solver for a specific quadric pair is missing, implement it. Do not route
through the mesh/polygon fallback as a substitute.

### A15.2 No mesh fallback for quadric pairs

If a boolean operation encounters a quadric surface pair for which no SSI solver
exists, the kernel MUST return `KernelError::NotSupported` with a diagnostic
naming the missing pair (e.g., `"SSI solver missing: torus-cylinder"`). The caller
can then handle the limitation explicitly.

Adding a "temporary" mesh fallback for quadric pairs is prohibited. Each such
fallback becomes permanent technical debt that degrades geometric accuracy through
chained booleans. The correct fix is always to implement the solver.

### A15.3 Rationale

Routing quadric booleans through mesh/polygon approximation causes three
compounding problems:

1. **Geometric drift**: Tessellation introduces discretization error. Each chained
   boolean accumulates error, degrading accuracy geometrically. Analytical SSI
   preserves exact geometry through arbitrary chains. [#1] Patrikalakis Ch.5
   documents exact SSI algorithms for all quadric pairs.

2. **Surface type loss**: The mesh path assigns `SurfaceGeom::Planar` to all result
   faces regardless of their original surface type. Cylindrical, conical, spherical,
   and toroidal faces become faceted approximations. Subsequent operations cannot
   recover the original analytical geometry. [#25] Yang et al. show topology-guaranteed
   SSI preserves surface identity through boolean chains.

3. **Topology degradation**: Mesh-based face counts grow with each chained boolean
   as subdivision artifacts accumulate. Analytical SSI produces the minimal topology
   dictated by the actual intersection geometry. [#27] Li et al. recommend hybrid
   architecture: analytical for simple pairs, topology-guaranteed for complex pairs.

### A15.4 Implementation sequence

The 15 quadric surface pairs ordered by CAD frequency, with implementation status:

| # | Pair | SSI Curve Type | Status |
|---|------|----------------|--------|
| 1 | Plane–Plane | Line (or overlap) | done |
| 2 | Plane–Cylinder | Ellipse/circle | done |
| 3 | Plane–Cone | Conic section | done |
| 4 | Plane–Sphere | Circle | done |
| 5 | Cylinder–Cylinder | Degree ≤ 4 curve | in-progress |
| 6 | Plane–Torus | Degree-4 curve | todo |
| 7 | Cylinder–Cone | Degree ≤ 4 curve | todo |
| 8 | Cylinder–Sphere | Degree ≤ 4 curve | todo |
| 9 | Cone–Cone | Degree ≤ 4 curve | todo |
| 10 | Cylinder–Torus | Degree ≤ 8 curve | todo |
| 11 | Cone–Sphere | Degree ≤ 4 curve | todo |
| 12 | Sphere–Sphere | Circle | todo |
| 13 | Cone–Torus | Degree ≤ 8 curve | todo |
| 14 | Sphere–Torus | Degree ≤ 4 curve | todo |
| 15 | Torus–Torus | Degree ≤ 8 curve | todo |

---

## A12. Change Control

### A12.1 Protected invariants

Any proposed change that weakens these invariants requires:

- a written rationale
- an explicit tradeoff analysis
- a migration plan
- explicit human approval

### A12.2 Ambiguity rule

If a change introduces ambiguity about responsibility boundaries, reject it and redesign for clearer layering.

---

End of Architectural Invariants (v1)

