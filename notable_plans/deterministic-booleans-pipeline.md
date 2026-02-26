# Waffle Iron: Deterministic Booleans & Correct-by-Construction Pipeline

> Archived: 2026-02-26 | Status: Implementation in progress

## Context

After 14 sprints of patching truck-shapeops, we own ~85% of the boolean pipeline code (10.9k lines). The remaining failures — MV3 (face division figure-8 wire), S3 (50% non-determinism), q4/q5 (complex multi-cut cascade exhaustion) — are structural. They cannot be fixed with more recovery levels or cascade strategies.

The current pipeline is approximate-then-fix: compute floating-point intersections, split faces, hope the topology works, and if it doesn't, retry with perturbations up to 50 times across 9 strategies with 7 recovery levels. This ceiling is permanent.

The literature converges on a fundamentally different approach: **exact predicates + topology-first invariants + winding number classification**. No perturbation. No cascade. Correct by construction.

**Directive**: No more workarounds. Fix the architecture. This is our library now.

## What We Keep

- **truck-topology types** (Vertex, Edge, Wire, Face, Shell, Solid) — solid B-rep representation
- **truck-geometry** (NURBS curves/surfaces) — standard, works
- **truck-modeling** (builders, sweep, revolve) — adequate
- **Analytical SSI layer** (plane-cylinder, plane-cone, sphere-plane, cylinder-cylinder) — exact IC curves
- **Test infrastructure** (ModelBuilder, oracles, 400+ tests) — excellent foundation
- **IC healing layer** (NURBS arc replacement) — reduces chained-boolean drift

## What We Replace

| Current | Problem | Replacement |
|---------|---------|-------------|
| Pointer-based `ID<T>` | Non-deterministic across runs (S3 50%) | Sequential monotonic IDs |
| `create_loops_stores` boundary insertion | Projects IC endpoints onto edges with tolerance, ambiguous at corners | Pave blocks: explicit IC-edge intersections |
| `divide_one_face` wire splitting | Produces figure-8 non-simple wires (MV3) | Topology-first: radial sort + minimal cycle traversal |
| Ray-cast classification (8 dirs, majority vote) | Fails on degenerate geometry, needs edge-neighbor fallback | Generalized winding numbers (continuous, no rays) |
| 7-level shell recovery + 50-attempt cascade | Band-aid for approximate pipeline | Single-pass deterministic assembly (no recovery needed) |

---

## Phase 0: Deterministic Sequential IDs

**Goal**: Eliminate non-determinism at the root. All topology entity IDs become creation-order sequential integers.

**Current mechanism** (`vendor/truck/truck-base/src/id.rs:6-13`):
```rust
pub struct ID<T>(usize, PhantomData<T>);
impl<T> ID<T> {
    pub fn new(ptr: *const T) -> ID<T> { ID(ptr as usize, PhantomData) }
}
```
Used by Vertex (`vertex.rs:95`), Edge (`edge.rs:291`), Face (`face.rs:610`) via `Arc::as_ptr()`.

**New mechanism**: Embed a sequential ID inside each topology element's `Arc` wrapper. A global `AtomicU64` counter assigns IDs at creation time. The counter is deterministic because creation order is deterministic (single-threaded WASM, sequential Rust).

**Approach**: Add a `seq_id: u64` field to the inner data behind each `Arc<Mutex<T>>`. Change `Vertex`, `Edge`, `Face` structs to use a wrapper that carries both the `seq_id` and the geometric data. The `id()` methods return `SequentialID(seq_id)` instead of `ID::new(Arc::as_ptr(...))`.

`DetId`/`DetContext` remain during transition, then get removed.

**Files**:
| File | Change |
|------|--------|
| `vendor/truck/truck-base/src/id.rs` | Add `SequentialID` type + global atomic counter + `reset_id_sequence()` for test isolation |
| `vendor/truck/truck-topology/src/lib.rs` | New inner wrapper types; update `Vertex<P>`, `Edge<P,C>`, `Face<P,C,S>` struct definitions; change type aliases `VertexID`, `EdgeID`, `FaceID` to `SequentialID` |
| `vendor/truck/truck-topology/src/vertex.rs` | `new()` allocates with `NEXT_SEQ_ID.fetch_add(1)`. `id()` returns `SequentialID`. Update `PartialEq`, `Hash`, `Clone`. |
| `vendor/truck/truck-topology/src/edge.rs` | Same pattern as vertex |
| `vendor/truck/truck-topology/src/face.rs` | Same pattern as vertex |
| `vendor/truck/truck-topology/src/shell.rs` | `mapped()` now creates new sequential IDs (deterministic because mapping order is deterministic) |

**Acceptance criteria**:
1. S3 passes 10/10 runs (currently ~5/10)
2. All 212 truck-shapeops tests pass
3. New test: same boolean 10 times → identical V/E/F ID sequences
4. `DetId` overlay still works (backward compat) but ordering matches `SequentialID` ordering

---

## Phase 1: Pave Blocks (Topologically Exact Face Splitting)

**Goal**: Replace the lossy "project IC polyline endpoints onto face boundary edges" with explicit IC-edge intersection records. Eliminates the MV3 figure-8 bug.

**Current problem**: `create_loops_stores` (`loops_store/mod.rs`) projects IC endpoints onto boundary edges using `SearchParameter` with tolerance. When the projection is ambiguous (vertex near a corner where edges meet), the wrong edge receives the vertex. `divide_one_face` then produces a non-simple wire.

**New data structures**:

```
PaveBlock: a segment of a boundary edge between two IC crossing points
  - original_edge_id, param_range, start_vertex, end_vertex
  - start_ic / end_ic (which intersection curve created each endpoint)

InterferenceTable: all IC-edge crossings for a shell pair
  - edge_pave_blocks: BTreeMap<EdgeID, Vec<PaveBlock>>  (ordered by parameter)
  - face_ics: BTreeMap<FaceID, Vec<IcSegment>>  (IC segments crossing each face)
```

**How it replaces create_loops_stores**:
1. **SSI phase** (keep): Compute IC curves for intersecting face pairs
2. **Interference phase** (new): For each IC, compute exact crossings with face boundary edges. For analytical ICs (ellipse, circle), solve the exact equation. For BSpline ICs, use high-precision Newton.
3. **Pave block construction** (new): Sort IC vertices by parameter on each edge. Create pave blocks. No boundary wire mutation.

**Files**:
| File | Change |
|------|--------|
| `vendor/truck/truck-shapeops/src/transversal/pave_block.rs` | New: PaveBlock, InterferenceTable, IcVertex, IcSegment types |
| `vendor/truck/truck-shapeops/src/transversal/interference.rs` | New: `build_interference_table()` — compute IC-edge crossings |
| `vendor/truck/truck-shapeops/src/transversal/loops_store/mod.rs` | Extract IC computation; boundary insertion delegated to interference.rs |

**Acceptance criteria**:
1. MV3 figure-8 bug structurally impossible (no boundary wire mutation)
2. All existing tests pass
3. Property test: every pave block sequence covers its original edge exactly (no gaps, no overlaps)

---

## Phase 2: Winding Number Classification

**Goal**: Replace ray-cast majority voting with generalized winding numbers. Eliminates classification brittleness.

**Current problem**: `ray_cast_classify` (`integrate/mod.rs`) uses 8 irrational ray directions with bidirectional perturbation, 1000x escalation, face-normal fallback, and edge-neighbor propagation. Despite this complexity, it fails when rays graze edges or pass through vertices.

**New approach**: Generalized winding number (Jacobson 2013). For a query point P and triangulated shell S:
```
w(P) = (1/4pi) sum solid_angle(P, triangle_i)
```
w > 0.5 → inside. w < 0.5 → outside. Smooth transition at boundary — no rays, no perturbation.

Uses the same `poly_shell` triangulation that ray-cast currently uses. No additional tessellation needed.

**Files**:
| File | Change |
|------|--------|
| `vendor/truck/truck-shapeops/src/transversal/winding.rs` | New: `winding_number()`, `solid_angle()`, `classify_point()` |
| `vendor/truck/truck-shapeops/src/transversal/integrate/mod.rs` | Replace `ray_cast_classify` calls with `winding_number_classify` |

**Acceptance criteria**:
1. All existing passing tests still pass
2. New test: 1000 random points against unit cube → 100% correct classification
3. No edge-neighbor propagation fallback needed (winding numbers handle all cases)

---

## Phase 3: Topology-First Face Division

**Goal**: Replace `divide_one_face` (779 lines of wire splitting + 5 recovery paths) with a graph traversal that is correct by construction.

**Algorithm** (Sugihara & Iri + Levy/Cherchi radial sort):

1. Build face boundary graph: nodes = all vertices (original + IC), edges = pave blocks + IC segments
2. At each node, sort outgoing edges by angle in face parametric space (radial sort)
3. Traverse graph to find minimal cycles: at each node, follow "next counterclockwise" edge
4. Each cycle = one face fragment with And/Or status from its IC segment statuses
5. Validate invariants: every fragment wire is simple, closed, non-degenerate, consistent

**Why this is correct**: Radial sort in a planar graph with the "next CCW edge" rule provably finds all minimal faces. No recovery needed because the graph is constructed from exact pave blocks (Phase 1) and the traversal has no floating-point-dependent branch point.

**Files**:
| File | Change |
|------|--------|
| `vendor/truck/truck-shapeops/src/transversal/face_graph.rs` | New: `build_face_graph()`, `radial_sort()`, `find_minimal_cycles()` |
| `vendor/truck/truck-shapeops/src/transversal/divide_face/mod.rs` | Rewrite: graph traversal over pave blocks + IC segments |

**Acceptance criteria**:
1. MV3 passes (figure-8 structurally impossible)
2. All existing tests pass
3. Property test: no face fragment ever has a non-simple wire
4. Property test: fragment areas sum to original face area

---

## Phase 4: Single-Pass Shell Assembly

**Goal**: Replace 7-level shell recovery + 50-attempt perturbation cascade with single-pass deterministic assembly.

**Why recovery becomes unnecessary**: With Phases 0-3:
- Deterministic IDs → no ordering bugs
- Pave blocks → IC-edge intersections exact, no wire mutation
- Winding numbers → classification robust
- Topology-first division → fragments correct by construction

Shell assembly becomes: collect And/Or fragments → concatenate → verify closed → `Solid::try_new`. If it fails, structured error — no retry.

**Migration**: New pipeline first. Legacy cascade as fallback. Track fallback rate. When fallback rate = 0 for 2 sprints → delete legacy code.

**Files**:
| File | Change |
|------|--------|
| `vendor/truck/truck-shapeops/src/transversal/integrate/mod.rs` | New `assemble_boolean_shell()` |
| `crates/kernel-fork/src/healing.rs` | New pipeline entry + legacy fallback with tracking |
| `crates/kernel-fork/src/truck_kernel.rs` | Boolean methods use new pipeline with fallback |

**Acceptance criteria**:
1. q4, q5 pass without cascade
2. No test needs more than 1 attempt
3. Legacy fallback rate tracked and trending to 0

---

## Phase 5: Assay — Continuous Property-Testing System

**Name**: **Assay** (to test a material for quality and purity — what you do to iron after forging).

Not a separate system. A module within `test-harness` extending ModelBuilder + oracle infrastructure with proptest.

**Components**:
- **proptest strategies**: Composable generators — dims → sketch profiles → solid bodies → boolean scenarios → degeneracy families → chains. Built-in shrinking produces minimal repro cases.
- **Oracle-based validation**: Volume monotonicity, Euler invariant, manifoldness, watertightness, bbox containment. Uses existing `OracleVerdict` infrastructure.
- **Determinism testing**: Same scenario twice → identical topology. Catches pointer-ID non-determinism.
- **Regression corpus**: Git-tracked JSON files in `crates/test-harness/corpus/`. Each: minimized scenario + expected results + bug ID. Replayed on every CI run. Seeded with 5 currently-ignored tests.
- **Coverage matrix**: Degeneracy family x operation x primitive pair. Identifies blind spots.

**Files**:
| File | Change |
|------|--------|
| `crates/test-harness/Cargo.toml` | Add `proptest = "1.10"` dev-dependency |
| `crates/test-harness/src/assay/mod.rs` | Module root |
| `crates/test-harness/src/assay/strategies.rs` | proptest strategies (Level 0-5) |
| `crates/test-harness/src/assay/properties.rs` | Oracle-based invariant checkers |
| `crates/test-harness/src/assay/determinism.rs` | Determinism harness |
| `crates/test-harness/src/assay/corpus.rs` | Regression corpus load/save |
| `crates/test-harness/src/assay/coverage.rs` | Coverage matrix |
| `crates/test-harness/tests/assay_box_box.rs` | proptest: box-box properties |
| `crates/test-harness/tests/assay_determinism.rs` | proptest: determinism |
| `crates/test-harness/tests/assay_regression.rs` | Corpus replay |
| `crates/test-harness/corpus/*.json` | Regression corpus |
| `scripts/test.sh` | New tiers: `assay-quick` (<30s), `assay` (~3min), `assay-deep` (nightly) |

---

## Phase Dependencies & Execution Order

```
Phase 0: Deterministic IDs ──────────────────┐
                                              │
Phase 5: Assay (independent) ─────────────────┤
                                              │
Phase 1: Pave Blocks ─── depends on Phase 0 ──┤
                                              │
Phase 2: Winding Numbers (parallel w/ Ph.1) ──┤
                                              │
Phase 3: Topology-First Division ─────────────┤ depends on Phase 1 + 2
                                              │
Phase 4: Single-Pass Assembly ────────────────┘ depends on Phase 3
```

Start: Phase 0 + Phase 5 in parallel (both independent).
Then: Phase 1 + Phase 2 in parallel (both depend only on Phase 0).
Then: Phase 3 → Phase 4 sequentially.

---

## How Each Root Cause (D1-D5) Is Eliminated

| Root Cause | Current | New Pipeline |
|-----------|---------|-------------|
| **D1: Coplanar** | Overlay + fragment classification (approximate) | Pave blocks handle coplanar as 2D polygon overlap in parameter space |
| **D2: Coincident edges** | Tolerance-based welding | Pave blocks make coincidence explicit — IC on existing edge recorded directly |
| **D3: Vertex-on-face** | `tau_boundary` projection, ambiguous | InterferenceTable records exact IC-edge crossings. Sequential IDs eliminate ordering ambiguity |
| **D4: Tangential** | Short ICs filtered by TOLERANCE | Analytical SSI + face graph area invariant filter for zero-area cycles |
| **D5: Chained drift** | BSpline IC accumulates ~5e-6 per boolean | Each boolean computes fresh SSI on original surfaces, not prior artifacts |

---

## Documentation Harmonization

After implementation, update:

| Document | Changes |
|----------|---------|
| `ARCHITECTURE.md` | New boolean pipeline stages; remove cascade description |
| `CLAUDE.md` | Update priorities: boolean reliability → boolean correctness |
| `specs/boolean_algorithm.md` | New: full spec for pave block + winding number + topology-first pipeline |
| `specs/autosolver.md` | Rename/replace with `specs/assay.md` |
| `docs/TESTING.md` | Add assay test tiers |
| Memory files | Update boolean pipeline architecture, add Phase 0-5 status tracking |

---

## Verification Strategy

Each phase has acceptance criteria above. Additionally:

- **Cross-phase**: After each phase, `./scripts/test.sh full` + `./scripts/test.sh assay`. No regressions.
- **Legacy fallback metric**: After Phase 4, track cascade invocations. Target: 0 → delete legacy.
- **Ignored test progression**:
  - After Phase 0: S3 un-ignored (deterministic)
  - After Phase 1+3: MV3 un-ignored (no figure-8)
  - EC3 (multi-body) and MO4 (torus) remain until respective support added

---

## Literature References

- **Sugihara & Iri (2000)** — Topology-oriented implementation
- **Jacobson et al. (2013)** — Generalized winding numbers
- **Zhou et al. (2016)** — Mesh arrangements for solid geometry
- **Cherchi et al. (2020, 2025)** — Fast/robust/exact mesh arrangements
- **ESOLID (Keyser et al. 2004)** — Exact booleans on curved solids
- **Barton et al. (2018)** — Hybrid NURBS/mesh booleans
- **Shewchuk (1997)** — Adaptive precision predicates
- **Edelsbrunner & Mucke (1990)** — Simulation of Simplicity
