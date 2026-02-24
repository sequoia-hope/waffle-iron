# Determinism and Stability in Truck Boolean Operations for Waffle Iron

## System map (short but precise)

**Scope note (evidence base):** Direct browsing of the fork’s Git tree under entity["organization","sequoia-hope","github organization"] / entity["company","GitHub","code hosting platform"] was not reliably available through the web tooling in this session. The concrete code-path evidence below therefore comes from the *published* `truck-shapeops` / `truck-topology` / `truck-base` sources on `docs.rs`, which mirror the upstream `Truck` crate structure closely enough to map the boolean pipeline and identity model. Where the fork may diverge, the remediation and instrumentation steps are written to *verify against the fork* with minimal changes. citeturn44view0turn56view0turn38view0turn30view0turn43view0

### Key crates / modules involved

**Boolean operations (shape ops)**
- `truck-shapeops` (`truck_shapeops` crate): boolean API surfaces `and(...)` and `or(...)` and the transversal boolean pipeline. citeturn35search0turn43view0turn44view0
- `truck_shapeops::transversal` module declares the boolean pipeline submodules:  
  `divide_face`, `faces_classification`, `integrate`, `intersection_curve`, `loops_store`, `polyline_construction`. citeturn56view0
- `truck_shapeops::transversal::integrate::and/or`: orchestrates the boolean between two `Solid`s shell-by-shell via `process_one_pair_of_shells`. citeturn44view0

**Topology identity and traversal**
- `truck-topology` (`truck_topology` crate): defines core topological entities (`Vertex`, `Edge`, `Face`, `Shell`, `Solid`) and explicitly states that topological elements have unique `id`s. citeturn30view0turn46search8
- `truck-base` (`truck_base` crate): contains an `id` module described as an “ID structure with `Copy`, `Hash` and `Eq` using raw pointers,” and a `hash` module described as “Deterministic hash functions.” citeturn38view0

**Hash-based collections / iteration-order sensitivity (system-level)**
- Standard `HashMap`’s ordering is explicitly described as randomized by default due to per-map random seeding (a general warning that relying on order is invalid). citeturn69search0
- For deterministic iteration, `BTreeMap` yields sorted-by-key iteration; `IndexSet` / `IndexMap` yield insertion-ordered iteration independent of hash values. citeturn69search4turn69search3

### High-level flow for boolean classification

At the highest level, `and()` / `or()` perform boolean operations by iterating shells and repeatedly combining results:

- `and(solid0, solid1, tol)`:
  1. Take first shells, compute `and_shell` via `process_one_pair_of_shells(shell0, shell1, tol)`.
  2. Fold additional shells from either solid into `and_shell` by repeatedly calling `process_one_pair_of_shells`. citeturn44view0
- The result shell’s `connected_components()` becomes the new `Solid` boundaries. citeturn44view0

Inside `process_one_pair_of_shells(shell0, shell1, tol)`:

1. **Triangulate each shell**: `shellX.triangulation(tol)` -> polygonal shell representation. citeturn44view0  
2. **Compute intersection loops**: `loops_store::create_loops_stores(shell0, poly_shell0, shell1, poly_shell1, tol)` returning a `LoopsStoreQuadruple` (two stores used downstream). citeturn44view0  
3. **Divide faces**: `divide_face::divide_faces(shellX, loops_storeX, tol)` -> classification structure `clsX`. citeturn44view0  
4. **Integrate per connected component**: `clsX.integrate_by_component()`. citeturn44view0  
5. **Extract buckets**: `clsX.and_or_unknown()` -> `[and, or, unknown]`. citeturn44view0  
6. **Resolve unknown faces via ray-crossing**: for each unknown face, pick a point `pt` from the first boundary’s first vertex and cast a direction `dir = hash::take_one_unit(pt)`; compute signed crossings against the opposite polygon shell; classify as `and` if `count >= 1` else `or`. citeturn44view0  
7. Merge both sides’ `and` buckets and both sides’ `or` buckets and return `[and_shell, or_shell]`. citeturn44view0  

### Where non-determinism can enter

This section distinguishes (A) *confirmed risk factors from the code/architecture evidence* vs (B) *likely hotspots inside the unexpanded modules (`divide_face`, `faces_classification`, `loops_store`) that should be verified on the fork*.

**Confirmed risk factors (evidence-backed):**

- **Pointer-derived identity exists in the core stack.** `truck_base::id` is explicitly described as using “raw pointers” for an `ID` that implements `Hash`/`Eq`. If topology IDs are built on top of this, then `Vertex/Edge/Face` IDs can depend on allocation addresses. citeturn38view0turn30view0turn46search8  
- **Hash-map iteration order is not a valid source of truth.** Even in standard Rust, `HashMap` ordering is randomized by default and should not be relied upon for stable ordering. citeturn69search0  
  Even if the fork uses `rustc_hash::FxHashMap` (common in these crates), *hash-table iteration order is still an implementation detail*; if keys vary across runs (e.g., pointer-based IDs), iteration order—and thus any “first element wins” logic—can still vary across runs. (This is the core of your hypothesis, and it is consistent with the documented raw-pointer ID model.) citeturn38view0turn69search0  
- **Boolean classification includes “pick one” and “fold over collections” patterns.** For unknown faces, the code selects a single point from `face.boundaries()[0].vertex_iter().next()` and uses it for a classification test. If the boundary/vertex iteration order is affected upstream (e.g., by hash-derived ordering in how wires are assembled), then classification inputs can change. citeturn44view0  

**Likely hotspots to confirm (fork verification required):**
- **`loops_store::create_loops_stores(...)`** likely builds maps keyed by topological elements / intersection entities and then iterates through them to emit loops; any hash-iteration dependence here can change the *loop wiring*, which then changes splitting/division and classification. citeturn44view0turn56view0  
- **`divide_face::divide_faces(...)`** likely constructs face subdivisions and may iterate adjacency graphs; if the traversal order affects “which side” or “which component first,” results will vary. citeturn44view0turn56view0  
- **`cls.integrate_by_component()` and `cls.and_or_unknown()`** strongly suggest graph/component integration and then bucketization—classic places where “order of exploring neighbors” can change outcomes if there are ambiguous ties. citeturn44view0  
- **Parallelism:** `truck-topology` lists `rayon` as a dependency, so parallel iteration is plausible somewhere in the topology utilities. If any boolean-phase data structure is built via parallel iterators without deterministic reduction/merge ordering, results can differ. citeturn30view0  
- **Floating-point tie behavior:** boolean uses geometric tests, tolerances, and meshing (`triangulation(tol)`), and then signed crossings. Borderline cases near tolerance can flip classifications due to small numeric differences or different evaluation orders. citeturn44view0  

**Hypothesis status:** *Partially confirmed, high-likelihood as a primary contributor; not fully proven for the fork without on-fork instrumentation.*  
- Confirmed: the stack explicitly supports raw-pointer-based IDs (`truck_base::id`), which are incompatible with cross-run determinism if used as identity keys for ordering decisions. citeturn38view0  
- Confirmed: classification pipeline contains order-sensitive selections (e.g., “first vertex” sampling) and likely contains graph integrations where traversal order matters. citeturn44view0  
- To fully confirm “Arc::as_ptr() → IDs → FxHashMap iteration order → face classification flips,” the fork needs targeted logs in `loops_store` / `divide_face` / `faces_classification` to show that a different key iteration order leads to different loop emission / face labeling.

## Reproduction and diagnosis

### Minimal reproducible scenarios

These are written as fork-facing repros. The intent is to produce deterministic “diffable” artifacts (e.g., stable hashes of topology) and to run the same boolean many times.

**Scenario A: Repeat the canonical `and/or` on a known example solid pair (punched cube style)**  
Rationale: upstream `truck-shapeops` includes `and/or` and an example named `punched-cube-shapeops`. Even if your fork differs, this pattern is typically present. citeturn35search1turn44view0  
- Command pattern:
  - Build a small binary (or test) that loads/constructs the same two solids, runs `and(...)` N times, serializes the resulting topology in a stable way (see instrumentation plan), and asserts the digest is constant.

**Scenario B: Multi-shell fold sensitivity (`Solid` with >1 shell on either side)**  
Rationale: `and/or` fold over shells and repeatedly call `process_one_pair_of_shells`. If a non-deterministic ordering affects intermediate shell content, it can compound across folds. citeturn44view0  
- Construct:
  - `solid0` with 2 shells (e.g., two disjoint closed shells)
  - `solid1` with 1 shell  
  - Run `and` repeatedly and compare digests.

**Scenario C: Near-tolerance ambiguity on coplanar / near-coplanar intersections**  
Rationale: unknown-face classification uses ray crossing counts against a triangulated shell produced with tolerance `tol`; near-degenerate intersections can create unstable classification sets if there is any order-dependent tie-breaking earlier in the pipeline. citeturn44view0  
- Construct:
  - Two boxes where one face is within ~`tol` of touching another face (offset by 0.5–1.5×`tol`).
  - Run `and/or` repeatedly.

**Scenario D: Stress randomized allocator addresses within one process**  
Rationale: even within a single process, repeated create/free patterns can yield different addresses over iterations; if IDs derive from pointers, repeated runs in the same test can still diverge. This attempts to amplify pointer-layout variability. citeturn38view0  
- Construct:
  - For i in 0..N: allocate some dummy geometry/topology objects, drop them, then run boolean.
  - Compare digests across i.

### Instrumentation plan (what to log, where, invariants)

**Goal:** pinpoint the *first divergent decision point* across runs.

**Log format recommendation:** structured logs (e.g., `tracing`) with compact, stable “keys.” For determinism debugging, logs must not include raw pointer addresses *as the only identity*; they should include (a) pointer-based IDs for correlation and (b) a deterministic key candidate (see remediation) for cross-run comparison.

#### Where to log (exact symbols / file paths)

1. `truck_shapeops/transversal/integrate/mod.rs`
   - At entry/exit of `process_one_pair_of_shells`
   - After `divide_face::divide_faces` and after `cls.integrate_by_component()`
   - After `cls.and_or_unknown()` (sizes + stable digests of each bucket)
   - During unknown-face resolution loop: for each unknown face, log:
     - Face “identity” (current ID + boundary signature)
     - Chosen point `pt` and direction selection method
     - Crossing `count` and final bucket decision  
   Evidence: these calls exist in this file and are the top-level orchestrator. citeturn44view0  

2. `truck_shapeops/transversal/mod.rs` (module wiring)
   - Add a feature-gated debug module export to centralize determinism checks across `divide_face`, `loops_store`, and `faces_classification`. citeturn56view0  

3. Fork-local: `loops_store::create_loops_stores`, `divide_face::divide_faces`, and `faces_classification` internals
   - Log iteration orders whenever iterating:
     - `HashMap` / `FxHashMap` (keys in the order visited)
     - `HashSet` / `FxHashSet`
     - adjacency traversals where a “first neighbor” is chosen  
   These are the *primary suspects* based on the pipeline structure. citeturn44view0turn56view0  

#### Invariants to check

- **Intersection loops determinism:** the multiset of loop “signatures” (e.g., length, quantized vertex sequence hash, or stable edge-key sequence) must be identical run-to-run immediately after `create_loops_stores`. Divergence here implicates loops_store ordering or floating ties.
- **Face subdivision determinism:** after `divide_faces`, the set of produced faces and their boundary signatures must match.
- **Component integration determinism:** after `integrate_by_component`, the component IDs and face membership sets must match.
- **Final AND/OR output determinism:** the digests of the final shells must match.

### What outcomes typically change across runs (what to diff)

Based on the orchestrator code structure, the most meaningful “stage outputs” are:

- The partition sizes of `[and0, or0, unknown0]` and `[and1, or1, unknown1]` and how many unknowns migrate to AND vs OR after ray casting. citeturn44view0  
- The resulting `and_shell` / `or_shell` face lists that are later fed into `connected_components()`. citeturn44view0  
- The final `boundaries` list returned by `connected_components()` and ultimately the returned `Solid`. citeturn44view0  

## Remediation plan (step-by-step)

This plan is explicit about a “fast path” that reduces nondeterminism risk quickly, and a “correct fix” that eliminates pointer/hash-order dependence structurally. The steps are prioritized so you can land them incrementally.

### Step one: Add a determinism harness and stage-level digests

**Objective:** make nondeterminism measurable and bisectable.

**Code locations to modify**
- Add a test module alongside `truck_shapeops/transversal/integrate/mod.rs` (there is already a `#[cfg(test)] mod tests;` marker at the end of the file). citeturn44view0  
- In the fork, add a `determinism` test helper that:
  - Runs the same boolean N times
  - Produces a stable digest of the resulting topology

**Expected effect**
- A single failing test that reproduces nondeterminism reliably, enabling regression protection.

**Validation**
- New test (and optional CLI) that:
  - Repeats `and/or` N times in-process
  - Asserts identical digest across all runs
  - Optionally runs in a subprocess loop in CI for cross-process confirmation

### Step two: Instrument iteration-order hotspots and assert ordering invariants

**Objective:** confirm/refute the pointer-ID → hash iteration → classification divergence hypothesis in *your fork*.

**Code locations**
- `truck_shapeops/transversal/integrate/mod.rs::process_one_pair_of_shells` for top-level stage boundaries. citeturn44view0  
- Fork-local:
  - `loops_store::create_loops_stores`
  - `divide_face::divide_faces`
  - `faces_classification::*`

**Expected effect**
- Log comparison shows the first stage where output digests diverge.
- Logs show whether divergence correlates with different hash-iteration orders / pointer-derived key order.

**Validation**
- Run the determinism harness with logging enabled; compare run A vs run B logs:
  - If “same inputs → different iteration order → different emitted loops/faces,” hypothesis confirmed.
  - If “same iteration order but different numeric outcomes,” focus shifts to floating tie/robustness.

### Step three: Fast-path mitigation — ban hash iteration order from influencing decisions

**Objective:** stop “iteration order decides semantics” quickly, even before ID refactors.

**Code locations**
- Any code in `loops_store`, `divide_face`, `faces_classification`, and integration code that:
  - Iterates `HashMap` / `FxHashMap` and makes decisions depending on traversal order
  - Picks the “first” element from `HashSet`
  - Uses `sort_by` without a total tie-breaker

**Implementation rule**
- Replace any semantic dependence on unordered collection iteration with:
  - **Sorted key iteration** (collect keys, sort by a stable ordering key, then iterate)
  - Or use `BTreeMap/BTreeSet` when key type has a meaningful `Ord` that is deterministic
  - Or use `IndexMap/IndexSet` if insertion order is the desired stable order and insertion order is itself controlled deterministically citeturn69search4turn69search3turn69search0  

**Critical caveat (why this is only a mitigation):**  
If your “stable ordering key” is a pointer-derived ID, you will still be deterministic *within a run* but not *across runs*. This is why the next step (deterministic identity) is the correct fix. citeturn38view0  

**Validation**
- Determinism harness should improve (fewer divergent outcomes) but may not fully fix cross-run differences until IDs are stabilized.

### Step four: Correct fix — introduce deterministic topology identity for boolean classification

**Objective:** remove allocator/pointer influence entirely from the boolean pipeline.

**Strategy (minimal surface-area, fork-friendly)**
- Do **not** try to replace `truck_topology`’s internal `id()` system immediately (likely large refactor).
- Instead, introduce a **per-boolean-operation deterministic identity layer** inside `truck-shapeops`:
  - Map each encountered topology entity to a **Deterministic ID** (`DetId`) that is independent of memory addresses.
  - Use these `DetId`s for all:
    - map/set keys
    - graph traversal ordering
    - tie-breakers in sorting
    - reproducible logging/digests

**Where IDs live**
- In a new `truck_shapeops::determinism` (or similar) module, instantiated per `and/or` call (per operation).
- IDs are ephemeral (do not need to be serialization-stable) unless you explicitly need cross-process artifact equivalence beyond determinism. (You do need determinism across runs; ephemeral sequential IDs are fine as long as assignment order is deterministic.)

**Validation**
- The determinism harness becomes stable across:
  - repeated in-process runs
  - repeated process runs
  - different machines (within floating tolerance expectations)

### Step five: Deterministic ordering and geometric tie-break rules

**Objective:** ensure that when geometry is used to break ties, it does so deterministically down to micron-scale features.

**Implementation details**
- Define a layered tolerance model:
  - `tol_op` (existing boolean tolerance, in meters)
  - `tol_sort` (deterministic ordering quantization step), e.g. `max(tol_op / 1024, 1e-12)` meters  
    This keeps ordering more granular than your operation tolerance while supporting 1 µm = 1e-6 m features.  
- Represent ordering keys using quantized integers:
  - `qx = round(x / tol_sort)` as `i64` (similarly `qy`, `qz`)
  - Include exact `f64::to_bits()` as a final tie-break if quantized coordinates collide (must define a total ordering, treating NaNs consistently)

**Validation**
- Property tests: small coordinate perturbations < `tol_sort/2` do not change ordering keys; perturbations > `tol_sort` may change but deterministically.

## Subsystem Spec

This spec targets the subsystem most likely required by your hypothesis: **Deterministic Topology Identity + Stable Ordering for Boolean Classification**.

### Goals and non-goals

**Goals**
- Deterministic boolean outputs across runs and machines, independent of:
  - allocator layout / pointer addresses (explicitly required)
  - hash iteration order (explicitly required)
- Maintain performance characteristics suitable for production boolean operations.
- Keep changes localized to `truck-shapeops` first (minimal surface area), with a staged path to deeper refactors if later desired.

**Non-goals**
- Redesign every topology type’s built-in `id()` in `truck-topology` on day one (large refactor risk).
- Make IDs stable across serialization formats unless you later decide outputs must be bit-identical across persistence (not required by the current request).

### Definitions

**Entities**
- `Vertex`, `Edge`, `Face`, `Shell`, `Solid` as defined in `truck-topology`. citeturn30view0turn46search8  

**Identity vs equality**
- *Identity* (for determinism and graph bookkeeping): “this specific topology instance in this operation.”
- *Equality* (geometric/topological): “represents the same geometry/topology under tolerance rules.”  
These are distinct; do not conflate.

**Deterministic ID (`DetId`)**
- A compact integer newtype:
  - Dense, comparable, hashable, orderable.
  - Assigned deterministically within the scope of a boolean operation.

### Deterministic ID requirements

**Core requirements**
- `DetId` assignment must not depend on pointer addresses (addresses may vary). This directly counters the raw-pointer ID model described in `truck_base::id`. citeturn38view0  
- `DetId` assignment must not depend on unordered collection iteration. citeturn69search0  
- `DetId` must be stable given the same inputs and tolerance parameters.

**Where IDs live**
- Per boolean operation (`and/or` call) context:
  - `DetContext` created at the start of the operation.
  - Dropped at the end.

**Clone/copy behavior**
- If a topology entity is cloned into a distinct instance during the boolean pipeline, it receives a new `DetId` unless you explicitly canonicalize it by deterministic structural keys.
- If you need “canonical ID reuse” (e.g., after wire splitting), that reuse must be driven by deterministic structural keys computed from topology + quantized geometry.

### Ordering requirements

**No semantic dependence on HashMap / HashSet iteration**
- Any time you iterate a `HashMap`/`FxHashMap`/`HashSet`/`FxHashSet`, you must either:
  - sort keys first (by deterministic key), or
  - use deterministic containers (`BTreeMap/BTreeSet`, or `IndexMap/IndexSet` depending on needed semantics). citeturn69search4turn69search3turn69search0  

**Stable traversal and tie-breaking**
- Graph traversals (BFS/DFS of adjacency):
  - neighbor lists must be sorted by `(DetId, secondary_key...)`
- “Pick a representative point” logic:
  - must choose a representative vertex/edge deterministically, not “first in container”

### Hashing requirements

- Hashing must be performed over:
  - `DetId` (preferred) and/or deterministic structural keys
- If geometric hashing is needed:
  - hash quantized integer tuples `(qx, qy, qz)` not raw `f64` values
  - if you must include `f64`, include `to_bits()` and specify NaN handling

### API surface changes (proposed)

Add a new internal module under `truck-shapeops` (exact module name up to you):

```rust
// truck_shapeops::determinism

/// Dense deterministic identifier, stable within a boolean operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DetId(pub u64);

/// Determinism context, created per boolean operation.
pub struct DetContext {
    next: u64,
    // Maps pointer-based (or existing) IDs to deterministic IDs.
    // Key type intentionally abstracted behind a trait in the implementation.
    // Implementation should avoid depending on iteration order for assignments.
}

impl DetContext {
    pub fn new() -> Self;

    /// Assign or retrieve a deterministic ID for a topology entity.
    pub fn id_of_vertex<P>(&mut self, v: &truck_topology::Vertex<P>) -> DetId;
    pub fn id_of_edge<P, C>(&mut self, e: &truck_topology::Edge<P, C>) -> DetId;
    pub fn id_of_face<P, C, S>(&mut self, f: &truck_topology::Face<P, C, S>) -> DetId;

    /// Deterministic sorting key for a vertex using quantized geometry (Point3 case).
    pub fn vertex_key_point3(&self, v: &truck_topology::Vertex<truck_base::cgmath64::Point3>, tol_sort: f64)
        -> (i64, i64, i64, DetId);
}
```

And in the boolean pipeline (`process_one_pair_of_shells`), require a `&mut DetContext` and use it whenever:
- storing entities in maps/sets
- sorting
- selecting representative points

This is intentionally additive and can be feature-gated (e.g., `cfg(feature = "deterministic-bool")`) during migration.

### Backward compatibility / migration plan

**Stage one (low risk):**
- Introduce `DetContext` and deterministic digests without changing semantics.
- Use it only for logging and determinism harness assertions.

**Stage two (medium risk):**
- Convert the most order-sensitive internal structures in `loops_store`, `divide_face`, `faces_classification`:
  - Replace hash-iteration-dependent traversals with deterministic traversal using `DetId` sorted order.

**Stage three (optional, larger refactor):**
- Consider pushing deterministic IDs into `truck-topology` as an alternate `id` backend if you need determinism beyond boolean ops. This is explicitly not required to fix the current blocker and should be treated as a separate project due to blast radius. citeturn38view0turn30view0  

### Performance constraints

- Prefer `Vec`-backed storage keyed by dense `DetId` where possible (O(1) indexing, deterministic iteration).
- Use `BTreeMap` only where:
  - keys are sparse or unknown up front
  - deterministic iteration is required and conversion to dense IDs is inconvenient citeturn69search4  
- Use `IndexMap/IndexSet` only where insertion order is the desired stable order and you can guarantee deterministic insertion order. citeturn69search3  

### Test requirements

**Unit tests**
- Deterministic assignment:
  - same input topology → same `DetId` assignment sequence
- Stable ordering:
  - neighbor traversal order is stable given the same inputs

**Property tests**
- Generate random solids (within reasonable bounds) and check:
  - boolean result digest stable across N runs
  - no panics and consistent partition sizes `[and, or, unknown]`

**Determinism test harness**
- Must run `and/or` N times and compare digests.
- Must include at least one “near tolerance” scenario.

### Failure modes and expected behavior

- If topology contains NaNs or invalid geometry leading to undefined comparisons:
  - determinism layer must define a total ordering fallback (e.g., treat NaN as greater-than all numbers via bit ordering) rather than panicking.
- If tolerance is non-positive:
  - existing code already rejects nonpositive tolerance (`nonpositive_tolerance!(tol);`). Determinism layer should respect the same invariants. citeturn44view0  

## Acceptance criteria

### Definition of “fixed”

- **Repeatable outputs across runs:** For a fixed pair of input solids and `tol`, `and/or` must produce identical:
  - face classification (AND/OR membership)
  - shell connected components
  - stable digest of final topology  
  across N repeated runs, including separate process invocations.
- **No dependence on allocator/pointers/hash iteration:** verified by:
  - removing any use of pointer-derived values as ordering keys
  - eliminating unordered iteration as a semantic input (sorting or deterministic containers) citeturn38view0turn69search0turn69search3turn69search4  
- **Determinism harness exists and passes** in CI:
  - same boolean run N times → all digests identical
- **All existing tests pass** (or any remaining ignored tests are explicitly justified and reference tracked issues).

### Determinism harness checklist

- A `#[test]` that:
  - constructs (or loads) a fixed solid pair
  - runs `and/or` N times (e.g., N=50–200)
  - computes a stable digest (defined in your codebase; must not include pointer IDs)
  - asserts all digests equal
- A CI script that:
  - runs that test in a loop under separate processes (e.g., 20 times) and diffs output logs/artifacts

### Merge sequencing checklist (what must land first)

1. Determinism harness + stage-level digest utilities (no semantic change).
2. Instrumentation in `process_one_pair_of_shells` (and fork-local hotspots) to identify first divergence stage. citeturn44view0  
3. Fast-path removal of unordered-iteration-dependent semantics in the *first divergence stage* (as proven by logs).
4. Introduction of `DetContext` + conversion of internal keys for that stage to use deterministic IDs.
5. Expansion of deterministic IDs + stable ordering to the remaining stages (`loops_store` → `divide_face` → `faces_classification` → integration).
6. Tighten: remove/forbid pointer-ID-based ordering in boolean code paths (lint or `deny` patterns in CI).

