# Specification for a Production‑Robust B‑Rep Boolean Solver in Truck for Waffle Iron

## Research Basis

- **[#2] Hoffmann Ch.3** — B-rep boolean pipeline architecture: shell intersection, face classification via neighborhood analysis, face subdivision, result assembly
- **[#3] OpenCASCADE GFA** — Staged interference (VV→VE→EE→VF→EF→FF), pave blocks, FaceInfo In/On/Sc states, same-domain analysis
- **[#24] Yang et al. (2025)** — Target architecture: hybrid B-Rep/mesh boolean with bijective mapping, zero failures, 17x faster than OCCT
- **[#17] Requicha & Voelcker (1985)** — Regularized boolean semantics (closure-of-interior), set membership classification
- **[#33] Stroud §6.1** — Stepwise boolean assembly with Euler operators, ENTERS/LEAVES/INOSCUL classification
- **[#7] Jacobson (2013)** — Generalized winding numbers for inside/outside classification
- **[#6] Sugihara & Iri (2000)** — Topology-oriented implementation: topology-first, numerics only for branch selection

## Context and baseline in the Waffle Iron vendor fork

Waffle Iron is currently in a **documentation / planning** phase and has selected a **fork of Truck** as its geometry kernel. citeturn7view2 The Waffle Iron repository vendors that fork as a git submodule at `vendor/truck`, pointing to `sequoia-hope/truck`. citeturn7view0 Waffle Iron also patches Cargo dependencies so the workspace uses the vendored Truck crates (e.g., `truck-topology`, `truck-meshalgo`, `truck-shapeops`) specifically noting the local fork is for **boolean operation improvements** and is pinned to the `truck-shapeops` v0.4.0 baseline. citeturn7view1

Truck’s `truck-shapeops` crate explicitly positions itself as providing **boolean operations to `Solid`** and shape-healing utilities for imported CAD data. citeturn9view1turn8view0 In the forked Truck baseline, the public boolean API exported by `truck-shapeops` is limited to `and` (intersection) and `or` (union), and these return `Option<Solid<…>>` rather than a structured error. citeturn9view1turn9view2 Internally, the current pipeline (in `truck-shapeops/src/transversal`) is broadly:

- Tessellate the two shells to polygon meshes using `triangulation(tol)`. citeturn9view2turn16view0  
- Extract mesh interference line segments and assemble them into polylines (a graph-walk that quantizes endpoints using `truck_base::tolerance::TOLERANCE`). citeturn12view0turn14view0turn9view0  
- Project polyline samples back onto the two parametric surfaces to build an “intersection curve with parameters.” citeturn12view0  
- Use those intersection loops to split input faces in parameter space and tag pieces as `And`, `Or`, or `Unknown` via `ShapesOpStatus`. citeturn10view1turn13view0turn12view1  
- For `Unknown` pieces, cast a ray and count crossings against the other solid’s tessellated faces to classify them. citeturn9view2  

This is a credible seed implementation, but it is **not** a comprehensive production boolean system yet because: (a) it lacks difference/XOR operations, (b) it relies heavily on a single global tolerance constant (`TOLERANCE = 1e-6`) and `Option`-based failure signaling, and (c) it can panic indirectly because `Solid::new` panics if shells are not non-empty, connected, oriented, closed, and manifold (while `Solid::try_new` returns a diagnosable error instead). citeturn9view0turn9view2turn15view1turn15view0

This specification defines what the *correct* boolean subsystem must do, using the Waffle Iron Truck fork as the implementation base.

## Required behavior and API contract

### Operations and result form

The solver MUST support **four** solid set operations over closed B‑Rep solids `A` and `B`:

- **Union** `A ∪ B`
- **Intersection** `A ∩ B`
- **Difference** `A \ B` (CUT)
- **Symmetric difference (XOR)** `A ⊕ B`

Difference and XOR are explicitly required because production CAD workflows and downstream feature modeling depend on CUT/XOR semantics, not only UNION/INTERSECT.

The default output MUST be a Truck `Solid` whose boundary shells are **connected, closed, oriented, and manifold**, i.e., constructible via `Solid::try_new` without `NotClosedShell`/`NotManifold` errors. citeturn15view1turn15view0 The solver MUST NOT use `Solid::new` as the final construction step in production mode because it panics on invalid boundaries; instead it MUST use `Solid::try_new` and surface the error as a structured boolean failure. citeturn15view1turn9view2turn15view0

The solver MUST provide a “non-manifold boundary output” escape hatch for workflows that explicitly want it, because Truck `Solid` creation rejects non-manifold boundaries. citeturn15view0turn15view1 Concretely, the API MUST allow returning either:

- a valid manifold `Solid`, or
- a boundary representation (e.g., a set of shells) that may be non-manifold and therefore not representable as a `Solid`.

### Input validation and healing hooks

In strict mode, inputs MUST be rejected if they violate Truck’s topology rules (examples: `SameVertex`, `NotClosedWire`, `NotSimpleWire`, `NotClosedShell`, `NotManifold`). citeturn15view0turn15view1

In “heal-then-validate” mode, the solver SHOULD apply Truck’s existing **split closed edges/faces** healing on compressed shells/solids to repair imported data that violates Truck’s topological constraints (“endpoints of edges must be different” and “face boundaries must be a simple wire”). citeturn10view0turn15view0 The healing module explicitly notes known limitations (e.g., boundary simplification currently only for cylinders; singularity-heavy surfaces like spheres are still incomplete), so the boolean solver MUST treat healing as best-effort and must revalidate after healing. citeturn10view0

### Regularized (“solid modeling”) semantics

Default booleans MUST be **regularized** solids: lower-dimensional artifacts (dangling faces/edges/vertices that do not bound volume) are not considered acceptable default outputs. This is consistent with production CAD expectations and with tolerance-based robustness approaches in tolerant modeling literature, which explicitly frames booleans as producing a valid solid boundary within an appropriate tolerance. citeturn20view2

### Determinism

Given identical inputs and options, the solver MUST produce identical topology and geometry (modulo stable ID renumbering) across runs and across thread schedules. This is especially important because Waffle Iron’s stack targets WASM and potentially multi-threaded execution, and Truck meshing explicitly has different parallelization behavior depending on the target (`rayon` vs no parallelization on `wasm32`). citeturn7view2turn16view0

## Units and the layered tolerance model for meter + micron scale

### Unit convention and target resolution

All geometric distances in Waffle Iron MUST be interpreted in **meters**. The system MUST support intentional modeled features down to **one micron**:

- `1 µm = 1e-6 m`

This requirement is in direct tension with Truck’s current global tolerance constant:

- `truck_base::tolerance::TOLERANCE = 1e-6` citeturn9view0

Because that constant is used not only for “near comparisons” but also in boolean-relevant internal logic (e.g., polyline graph endpoint quantization divides by `2*TOLERANCE`), treating `1e-6` as a universal coincidence threshold will tend to **erase or merge** geometry right at the 1 µm scale. citeturn14view0turn9view0

Therefore, the boolean solver MUST use a **layered tolerance model** that separates:

- **kernel numeric floors** (for safe comparisons and robust predicates),
- **modeling tolerances** (what the CAD system considers coincident / joinable),
- **meshing / intersection approximation tolerances** (how closely approximations track exact surfaces),
- **topological welding tolerances** (how aggressively to snap/merge during stitching),
- and **local per-feature tolerances** (uncertainty attribution).

This approach is strongly supported by “tolerant modeling” work in commercial kernels, which argues that single global tolerances create data bloat, block robust exchange between systems with different tolerances, and reduce boolean reliability; it proposes attaching a tolerance to each face/edge/vertex and performing tests relative to combined tolerances. citeturn20view2

### Required tolerance layers and invariants

The solver MUST define (and expose in options) at least the following tolerances, all in meters:

- **Minimum feature size** `L_min_feature = 1e-6` (hard requirement for preserved details)
- **Model absolute tolerance** `τ_model`  
  Used for “coincidence” decisions, join/weld admissibility, and final geometric validity checks.
- **Working precision / numeric floor** `τ_work`  
  Used for iterative solvers, residual thresholds, and geometric computation convergence.
- **Meshing/intersection tolerance** `τ_mesh`  
  Used for tessellation and intersection polyline construction.
- **Weld/snap tolerance** `τ_weld`  
  Used for vertex/edge snapping during stitching and cleanup.
- **Coplanar/overlap tolerance** `τ_coplanar`  
  Used to decide “same plane / same surface” and to run 2D overlays in parameter space.

The solver MUST satisfy these invariants:

- `τ_work << τ_model` (the numeric floor must be much tighter than modeling tolerance so that modeling tolerance is not dominated by numeric noise). This is aligned with robust predicate practice: the goal is correct sign decisions even with floating inputs, not ad-hoc large epsilons. citeturn19view2turn18search3turn18search1  
- `τ_mesh ≤ τ_model` (approximation must not be looser than what the model claims is accurate). Truck meshing already ties tessellation to a tolerance and documents that a smaller tolerance increases work; it even panics if `tol < TOLERANCE`, so the boolean subsystem MUST either (a) ensure tolerances respect that constraint, or (b) modify the fork so boolean-required meshing tolerances are permitted below the current global constant. citeturn16view0turn9view0  
- `τ_weld` MUST be derived from `τ_model` (typically `k * τ_model`, with a bounded `k`) so that topology repair does not “heal” features away beyond modeling tolerance.

### Recommended default numbers for meter + micron requirement

This spec does not mandate numeric defaults globally, but it DOES mandate that the defaults must preserve 1 µm features. A widely used CAD practice is “smallest feature ≥ 10× absolute tolerance,” meaning that to preserve `1e-6 m` features, `τ_model` should be on the order of `1e-7 m`. McNeel’s Rhino documentation states the general rule that small features should be at least an order of magnitude larger than absolute tolerance, and that absolute tolerance is measured in model units. citeturn17search11turn17search3turn17search7

Accordingly, a reasonable starting point is:

- `τ_model ≈ 1e-7 m`  
- `τ_mesh ≈ (0.25–1.0) * τ_model` (adaptive per-case)
- `τ_weld ≈ 2 * τ_model`
- `τ_work ≈ max(1e-12 m, ε_machine * scale)` (scale-aware numeric floor)

The solver MUST compute a **scale estimate** (e.g., bounding-box diagonal of inputs), and SHOULD use a hybrid absolute+relative tolerance for some comparisons because model scales can vary widely (Rhino’s guidance explicitly ties best behavior to the interplay of model size, units, and tolerance). citeturn17search11turn7view2

### Local per-edge/per-feature tolerances

The solver MUST track **local tolerances** for newly created intersection edges and downstream trimmed edges. Two independent evidence lines support this requirement:

- Tolerant modeling (Parasolid) attaches a tolerance to each face/edge/vertex and performs intersection/coincidence relative to combined tolerances. citeturn20view2  
- Recent research on boolean result repair identifies “false intersection edges” as a major source of illegal results and proposes **local adaptive tolerance estimation for each intersection edge** based on geometry and origin, then inference procedures to detect when repair is needed. citeturn17search0turn20view3

In this spec, local tolerances must be first-class solver artifacts:

- Every generated intersection curve segment MUST carry `τ_local`.
- Stitching and classification decisions that depend on that curve MUST consume `τ_local`, not only global values.
- When two features disagree beyond tolerance budgets, the solver MUST escalate in a controlled way:
  - refine intersection computation,
  - expand local tolerance (bounded by a `τ_max_allowed`), or
  - fail with a structured diagnostic (not silent `None`).

This is the core “layered tolerance model” requirement: tolerances are not a single constant; they are **budgeted per stage and per feature**.

### Implications for current Truck boolean code

The baseline Truck boolean stack currently hardcodes global tolerance assumptions in multiple places:

- `TOLERANCE = 1e-6` is a global constant. citeturn9view0  
- Polyline construction quantizes endpoints based on `TOLERANCE` (point index = `(pt + TOLERANCE) / (2*TOLERANCE)`), meaning connectivity resolution is inherently tied to that global constant. citeturn14view0turn9view0  
- Meshing panics when `tol` is less than `TOLERANCE`. citeturn16view0turn9view0

To meet the meter + micron requirement, the boolean solver MUST NOT treat these as immutable “truth.” Instead it MUST introduce a tolerance context (or equivalent) that:

- decouples boolean stage tolerances from `truck_base::tolerance::TOLERANCE`, and
- either refactors the boolean path to use context tolerances, or adjusts the fork so `TOLERANCE` can act as a numeric floor rather than a modeling tolerance.

## Solver architecture and internal stages

### Architectural mandate: staged, inspectable pipeline

The solver MUST be implemented as an explicit, staged pipeline with stage-specific inputs/outputs and error reporting. This is required because the baseline code currently returns `Option` even when failures are diagnostic (projection failures, mesh intersection failures, classification ambiguity). citeturn9view2turn12view0

Stages MUST be:

- **Preprocess**
  - Validate inputs and options.
  - Optional healing (split closed edges/faces).
- **Broadphase**
  - Use bounding boxes or other pruning to select candidate face pairs.
- **Intersection construction**
  - Compute surface-surface intersection representations, producing:
    - transversal intersection curves, and
    - coincident/coplanar overlap data.
- **Corefinement (imprinting)**
  - Split faces/edges so intersection curves become explicit trim boundaries.
  - Preserve consistent edge/vertex identity across adjacent faces.
- **Classification**
  - Determine each face patch’s relation to the other solid: Inside / Outside / OnBoundary.
- **Selection**
  - Select patches based on boolean op (Union / Intersection / Difference / XOR).
  - Invert orientation for “subtracted” boundary contributions (Difference).
- **Stitching**
  - Assemble patches into shells and (when allowed) a solid.
  - Weld within `τ_weld` respecting local tolerances.
- **Postprocess & validate**
  - Sliver handling, coplanar merging (optional), final topology validation.
  - Return `Solid` or a non-manifold boundary result per policy.

The current Truck boolean internals already reflect several of these phases (intersection curves from mesh interference, loop store, divide faces, classify unknowns by ray crossings). citeturn9view2turn12view0turn13view0turn12view1 The architecture requirement is to **make them explicit, generalize them to all ops, harden them, and make them diagnosable**.

### Core representation: replace AND/OR tagging with relation tagging

The baseline `truck-shapeops` boolean keeps a `ShapesOpStatus` enum with only `Unknown | And | Or`, derived from intersection curve orientation tests and propagated into face splitting. citeturn10view1turn12view1turn13view0 This cannot express the information needed for Difference and XOR in a principled way, and it entangles “operation choice” with “classification state.”

The solver MUST replace this with an operation-independent classification scheme:

- `RelationToOther = Outside | Inside | OnBoundary | Unknown`
- `OnBoundary` MUST record *which* boundary condition:
  - coincident face overlap,
  - transversal intersection curve adjacency,
  - tangency contact.

Operation selection MUST be a pure function over `RelationToOther` plus patch orientation.

### Intersection construction requirements

#### Transversal intersections

The solver MAY use Truck’s existing tessellation-first intersection approach, which:

- tessellates shells with `triangulation(tol)`, citeturn9view2turn16view0  
- extracts interference segments (`extract_interference`), citeturn12view0  
- constructs polylines (graph walk with quantization), citeturn14view0  
- then projects samples back to surface parameter space using `SearchNearestParameter` to build intersection curves with parameters. citeturn12view0turn9view2

But for production robustness the solver MUST add:

- **Adaptive refinement:** if projection residuals exceed `τ_work`/`τ_model` budgets, reduce `τ_mesh` locally and recompute only the affected face pairs.
- **Local tolerance attribution:** each constructed intersection segment must carry `τ_local`, reflecting tessellation quality + parameter inversion quality. citeturn17search0turn20view3
- **Avoid global quantization floors:** polyline construction must use a context tolerance, not a hardwired global `TOLERANCE`. citeturn14view0turn9view0

#### Coincident / coplanar overlaps

The solver MUST explicitly detect and handle coincident/coplanar overlaps (planar faces on the same plane, or trimmed patches that lie on the same underlying surface within tolerance). This cannot be left to ray casting against tessellated shells because correct behavior requires 2D overlay logic on trimming loops.

For 2D overlays, the solver SHOULD use a dedicated polygon boolean library. Acceptable options include:

- **iOverlay** (Rust polygon overlay; union/intersection/difference/xor; supports float APIs and holes). citeturn18search2turn18search21  
- **Clipper2** (well-known polygon clipping; intersection/union/difference/xor; available in Rust via crates such as `clipper2`). citeturn18search0turn18search13  

The library choice MUST be documented alongside portability constraints (WASM, FFI policy, deterministic output, numeric range/behavior).

### Robust classification requirements

The baseline Truck boolean classifies “unknown” face pieces by ray casting against tessellated faces and counting signed crossings. citeturn9view2 Ray casting is a valid approach but is notoriously sensitive to degeneracies (ray hits vertices/edges, coplanar grazing, etc.).

For production robustness, the solver MUST:

- Use **robust geometric predicates** for load-bearing decisions (orientation/side tests), rather than relying on large epsilons. Robust adaptive predicates are designed for exactly this: computing correct signs despite floating-point roundoff, typically with fast filters and adaptive fallbacks. citeturn19view2turn18search3turn18search1  
- In Rust, acceptable predicate implementations include:
  - `robust` (direct transcript of Shewchuk predicates; orient2d/orient3d/incircle/insphere). citeturn18search3  
  - `robust-predicates` (FFI-compiled Shewchuk predicates with the classical API). citeturn18search1turn18search5  

Robust predicates MUST be used to:
- stabilize ray-triangle intersection classification and tie-breaking,
- classify points relative to planes (half-space tests),
- detect degeneracy conditions (collinearity/coplanarity) reliably.

When ray casting remains ambiguous, the solver MUST use a deterministic multi-ray policy (e.g., cast several reproducible directions derived from a stable hash of a face ID / point and vote), and MUST surface unresolved ambiguity as a structured error.

### Stitching and topology assembly requirements

The solver MUST assemble output by building shells/faces/wires that comply with Truck topological constraints:

- **Edges cannot be formed from identical vertices** (`SameVertex`). citeturn15view0  
- **Face boundary wires must be closed and simple** (`NotClosedWire`, `NotSimpleWire`). citeturn15view0  
- **Solid boundaries must be closed and manifold** (`NotClosedShell`, `NotManifold`). citeturn15view0turn15view1  

Stitching MUST be tolerance-aware and MUST consume:
- `τ_weld` (global) and `τ_local` (per-feature),
- and MUST avoid “over-welding” that would erase required 1 µm features.

The solver MUST produce stable error categories if stitching fails (e.g., unmatched edges, inconsistent orientation, non-manifold adjacency).

## Degenerate configurations and “fuzzy” behavior

Production boolean reliability is dominated by “almost coincident” and “touching” cases. The solver MUST implement explicit policies for these cases, rather than leaving them to numerical accident.

### Touching and near-coincident policy

The solver MUST provide a `TouchingPolicy` that controls behavior when solids touch but do not overlap volumetrically (or overlap only within tolerance), because such cases often yield non-manifold boundaries if naïvely merged, which Truck `Solid` cannot represent. citeturn15view0turn15view1

The policy MUST include at least:

- **ErrorNonManifold (default):** return a structured error stating that union/difference would produce a non-manifold boundary.
- **KeepSeparateComponents:** treat touching-only solids as separate shells/components rather than forcing a shared edge/vertex.
- **FuzzyMergeWithin(τ_fuzzy):** allow merging within a user-specified additional tolerance.

This “fuzzy boolean” concept mirrors a known robustness strategy: Open CASCADE explicitly documents a “Fuzzy” boolean mode where an additional user tolerance is used to robustly handle touching and near-coincident entities. citeturn17search29

### Tangency and coplanarity

- **Tangential intersection** (surfaces touch along a curve/area without clean transversal crossing) MUST be detected and treated as a special classification case.
- **Coplanar overlap** MUST be handled via 2D overlays (as specified earlier), not by triangulation heuristics alone.

### Sliver and micro-feature handling

Given the 1 µm feature requirement, the solver MUST define a clear rule for what gets removed as a sliver:

- Features ≥ `1e-6 m` MUST be preserved by default.
- Features < `L_sliver_cut` MAY be removed if configured, but `L_sliver_cut` MUST default to a value *below* `1e-6 m` (or be disabled), otherwise the solver will violate the “support down to one micron” requirement.

This sliver policy MUST be applied in postprocess and must not silently destroy features that the tolerance model claims are meaningful.

## Diagnostics, validation, and quality gates

### Structured errors and debug artifacts

The boolean API MUST return `Result<…>` and MUST NOT collapse failures into `Option::None`. This is required because the baseline code already contains multiple failure points that need actionable reporting (intersection curve parameter projection, loop creation, face division, ray-cast classification). citeturn9view2turn12view0turn13view0turn12view1

Errors MUST be categorized at least into:

- Invalid input topology (wrap Truck topology errors such as `NotManifold`, `NotClosedShell`). citeturn15view0turn15view1  
- Tolerance configuration errors (e.g., meshing tol below allowed minimum in the current fork). citeturn16view0turn9view0  
- Intersection construction failures (projection residual too large, inconsistent intersection topology).
- Classification ambiguity (unresolvable under configured predicate/tolerance strategy).
- Stitching/assembly failures (closure, mismatched edges, non-manifold adjacency).
- Postprocess/healing failures.

When debug is enabled, the solver SHOULD emit artifacts sufficient to reproduce and diagnose issues, such as:

- face/edge IDs involved,
- intersection polylines in 3D and in parameter space,
- tolerances used per stage and per feature,
- tessellated debug meshes for the failing region.

### Validation requirements

The output MUST be validated before returning:

- Final `Solid` outputs MUST be created via `Solid::try_new` and therefore satisfy non-empty, connected, closed, manifold boundary conditions. citeturn15view1turn15view0  
- The solver SHOULD run geometric consistency checks where possible (Truck exposes `is_geometric_consistent` on `Solid` when point types implement `Tolerance`). citeturn15view1turn9view0  

### Test suite requirements

The solver MUST be shipped with a test suite that includes:

- **Property tests** for algebraic identities (idempotence, commutativity for union/intersection/xor, difference non-commutativity).
- **Degenerate regression corpus** including:
  - coplanar overlaps,
  - tangency,
  - touching-only at vertex/edge/face,
  - near-coincident offsets on the order of `τ_model`,
  - micro-features around `1e-6 m`.
- **Fuzz tests** ensuring:
  - no panics (especially avoiding `Solid::new` and other panic paths),
  - failures are structured errors.

Benchmarks SHOULD exist for:
- intersection construction,
- corefinement,
- classification,
- stitching,
with both small and high-face-count models.

### Non-negotiable quality gates for production mode

A boolean operation in production mode MUST satisfy:

- No panics on valid inputs (strict mode or healed-then-validated mode).
- Deterministic results (independent of thread races and hash ordering).
- Output respects meter units and preserves ≥ 1 µm features under default tolerances.
- Failures are typed, diagnosable, and optionally reproducible via artifacts.

---

This document is intended to be a **coding-agent guide** describing correct behavior and architecture targets for the boolean solver built on the Waffle Iron Truck fork. The key differentiator required by the updated constraints is the **layered tolerance model**: the solver must not rely on a single kernel-wide `1e-6` comparison tolerance, because the product must operate in meters while faithfully representing geometry at the 1 µm scale. citeturn9view0turn14view0turn20view2