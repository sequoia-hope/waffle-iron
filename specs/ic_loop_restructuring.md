# IC Loop Restructuring Specification

**Author:** Spec-Writer Agent
**Created:** 2026-03-03
**Status:** Approved for Implementation

---

## 1. Summary

This refactor restructures Pass 2 of the IC loop integration pipeline (`truck-shapeops/src/transversal/loops_store/mod.rs`) to replace interleaved vertex insertion + edge weaving with a cleaner two-phase approach:

**Current (Pass 2 Interleaved):**
1. Insert IC vertex into face boundary wire at correct position
2. Simultaneously weave IC edges back into boundary wires
3. Create derived boundary wires (complements)
4. FBG processes merged wires

**New (Direct Injection):**
1. **Vertex Rebinding Phase** — Replace pre-split sub-edge endpoint vertices with canonical IC vertices (no edge splitting)
2. **IC Injection Phase** — Push IC edges as separate `BoundaryWire` objects (not woven into boundary wires)
3. **FBG Enhancement** — Handle disconnected components and single-edge wires correctly
4. FBG processes all wires together (boundary + IC as separate wires)

**Key Insight:** FBG's `from_loops` doesn't care whether IC edges are woven into boundary wires or arrive as separate wires. It builds a half-edge graph from ALL edges across ALL wires, mapping vertices by `search_parameter`. Vertex identity (same `VertexID`) is what matters.

**Benefits:**
- Simpler logic (vertex rebinding is a linear pass, IC injection is straightforward)
- Cleaner separation of concerns (vertex → edge → face division)
- Easier to test and debug (each phase is independent)
- Fewer edge cases (no complex wire splicing)

---

## 2. Branch Tables

### 2.1 Vertex Rebinding (rebind_presplit_vertices)

**Purpose:** Replace IC vertex positions on pre-split sub-edge endpoints with the canonical IC vertex.

| RB# | Condition | Action | Code |
|-----|-----------|--------|------|
| RB1 | Sub-edge endpoint within `tol` of canonical IC vertex | Replace via `change_vertex(sub_edge_id, canonical_vertex_id)` | Deterministic VertexID match |
| RB2 | No matching sub-edge endpoint (boundary vertex, no presplit) | Skip (no-op, vertex is already correct) | Log trace |
| RB3 | PRESPLIT_MISS: canonical IC vertex not at any sub-edge boundary | Split the sub-edge at IC position first, then rebind new endpoint | Requires edge splitting |

**Algorithm:**
```
for each (ic_vertex, position) in canonical_ic_vertices:
  for each sub_edge in face.boundary_wires[*].edges():
    if within_tol(sub_edge.endpoint_position, position):
      change_vertex(sub_edge, ic_vertex)  // RB1
      mark_as_rebinded(sub_edge, ic_vertex)
    else if sub_edge needs splitting at position:
      // RB3: PRESPLIT_MISS
      split_edge(sub_edge, position)
      rebind_new_endpoint(split_edge, ic_vertex)
```

**Invariants:**
- Every IC vertex endpoint has at least one corresponding sub-edge endpoint rebinded
- No duplicate vertex assignments (deterministic first-match-wins)
- Edge splitting only when necessary (performance)

---

### 2.2 IC Injection (inject_ic_edges_direct)

**Purpose:** Insert IC edges into the wire collection as independent `BoundaryWire` objects.

| IJ# | Condition | Action | Code |
|-----|-----------|--------|------|
| IJ1 | IC is closed loop | Add as independent loop via `add_independent_loop(bw)` | Existing code path |
| IJ2 | IC is open, both endpoints at pre-split boundaries (RB1 rebinded) | Create single-edge `BoundaryWire`, push to `boundary_wires` | New code path |
| IJ3 | IC is open, PRESPLIT_MISS endpoint (RB3 triggered) | Split sub-edge in RB3, then inject via IJ2 | Coordinated RB3+IJ3 |
| IJ4 | IC status = And (face0 inside face1) | face0 wire = And; face1 wire = Or (complement) | Status-dependent wiring |

**Algorithm:**
```
for each ic_edge in ic_polyline:
  if ic_edge.is_closed():
    add_independent_loop(ic_edge)  // IJ1
  else:
    endpoints = (ic_edge.start(), ic_edge.end())
    if all_endpoints_rebinded(endpoints):
      bw = BoundaryWire::new(ic_edge, status=AND)
      face0.boundary_wires.push(bw)
      bw_complement = BoundaryWire::new(reverse(ic_edge), status=OR)
      face1.boundary_wires.push(bw_complement)  // IJ2 + IJ4
    else:
      // IJ3: PRESPLIT_MISS — should not happen if RB3 runs first
      panic!("PRESPLIT_MISS in injection: endpoint not rebinded")
```

**Status Logic (IJ4):**
- If IC endpoint is on face0 boundary AND face1 interior → face0 wire is And
- If both conditions reversed for face1, its wire is Or (complement of face0's wire)
- Prevents double-counting edges in FBG connectivity

---

### 2.3 FBG Enhancement (enhance_fbg_for_ic_edges)

**Purpose:** Handle disconnected components and single-edge wires in FBG processing.

| FG# | Condition | Action | Code |
|-----|-----------|--------|------|
| FG1 | Graph has disconnected components | Process each component independently, extract fragments from each | New algorithm |
| FG2 | Single-edge `BoundaryWire` (IC edge) | Forward half-edge takes wire status (And/Or), reverse HE takes complement | Status assignment logic |
| FG3 | IC vertex matches boundary vertex by VertexID | Connected in graph (normal graph connectivity) | `search_parameter` + VertexID match |

**Algorithm:**
```
graph = from_loops(all_boundary_wires + all_ic_wires)

// FG1: Handle disconnected components
components = graph.connected_components()
for each component in components:
  cycles = component.extract_cycles()  // Normal FBG algorithm
  fragments = cycles.to_fragments()

  // FG2: Assign status to fragments
  for each fragment in fragments:
    if fragment.edges.len() == 1 and fragment.edges[0].is_ic_edge():
      // Single IC edge: status already set in IJ4
      status = fragment.edges[0].status
    else:
      // Multi-edge or boundary-only fragment: infer from cycle structure
      status = infer_status_from_neighbors(fragment, graph)

    fragments.set_status(fragment, status)
```

**Vertex Matching (FG3):**
- When building half-edge pairs in `from_loops`, rebinded vertices have identical `VertexID`
- No fuzzy `search_parameter` matching needed for IC endpoints (exact match via RB1/RB3)
- Boundary-to-IC edge pairing works correctly because both endpoints have canonical IDs

---

## 3. Invariants

**INV-1: Vertex Rebinding Completeness**
- Every IC edge endpoint has its `VertexID` matching a rebinded sub-edge endpoint
- Enforced by: RB1 rebinding + RB3 edge splitting covers all cases
- Checked by: `assert_all_ic_vertices_rebinded(face)` in test harness

**INV-2: FBG Fragment Coverage**
- FBG produces ≥2 fragments for every face with IC crossings
- A face with N IC crossings must have ≥N+1 fragments (one per region)
- Checked by: Fragment counting in face division tests

**INV-3: Boolean Semantics Preservation**
- Fragment status (And/Or/Unknown) correctly represents boolean result
- And fragments = intersection geometry
- Or fragments = union geometry
- Checked by: Classification tests (union vol > A+B, intersection vol < min(A,B))

**INV-4: Truck Test Suite Determinism**
- All 331+ truck-shapeops tests pass deterministically
- Zero flaky tests introduced by restructuring
- Checked by: `cargo test -p truck-shapeops -- --test-threads=1` (serial run)

**INV-5: Test Harness Coverage**
- All test-harness boolean tests pass (excluding pre-existing failures: ec3, q4, q5, rb2/5/8, mo4)
- No regressions on currently-passing tests
- Checked by: `cargo test -p test-harness` (boolean_* modules)

**INV-6: Deterministic Output**
- Identical input → identical output (vertex order, edge order, fragment order)
- No use of HashMap iteration or timestamp-based randomness
- Checked by: Determinism tests (repeated runs comparison)

---

## 4. Failure Modes & Handling

### FM1: Vertex Rebinding Misses Crossings
**Symptom:** IC edge endpoint not found in any pre-split sub-edge (PRESPLIT_MISS in RB2).

**Root Cause:** IC vertex position lies between two sub-edge endpoints (should have been split in Pass 1).

**Handling:**
- Trigger RB3: Split sub-edge at IC position before rebinding
- Log warning with face ID, IC position, nearest sub-edge
- Fallback: Revert to legacy Pass 2 (interleaved) if RB3 fails

**Test:** `test_rb3_edge_split_fallback()`

---

### FM2: FBG Disconnected Component Wrong Fragments
**Symptom:** FBG produces wrong number of fragments or incorrect topology for disconnected regions.

**Root Cause:** Graph algorithm assumes connectivity, fails when component is isolated.

**Handling:**
- Implement FG1: Extract each connected component separately
- Process each component with normal FBG algorithm
- Merge fragment lists from all components
- Log component count and fragment distribution

**Test:** `test_fg1_multi_component_face()`

---

### FM3: IC Edge Status Logic Incorrect
**Symptom:** Face0 and face1 both have And status, or both have Or.

**Root Cause:** IJ4 status assignment logic is backwards.

**Handling:**
- Unit test each IC edge pair (And on face0 → Or on face1)
- Validate status logic against boolean operation semantics
- Geometric verification: And fragments should have volume, Or should too

**Test:** `test_ij4_status_complement()`

---

### FM4: PRESPLIT_MISS Batch Effect
**Symptom:** Multiple IC edges trigger RB3, causing cascade of edge splits.

**Root Cause:** Cumulative edge splitting from many IC crossings.

**Handling:**
- Batch all RB3 splits before any rebinding (sort by edge ID)
- Update sub-edge indices after each split to avoid stale references
- Cap total splits per face to prevent runaway complexity

**Test:** `test_rb3_batch_splits_many_ic_edges()`

---

## 5. Oracles (Test & Validation)

**O1: Truck Test Suite**
```bash
cargo test -p truck-shapeops
```
- **Target:** ≥331 passing tests, ≤3 pre-existing failures
- **Threshold:** 0 new test failures introduced
- **Execution:** ~90s

**O2: Test Harness Boolean Tests**
```bash
cargo test -p test-harness boolean_
```
- **Target:** All boolean_* modules pass (excluding pre-existing: ec3, q4, q5, rb2/5/8, mo4)
- **Threshold:** No new failures
- **Execution:** ~30s

**O3: Shadow Mode Monitoring**
- Track counters: `RB1_rebinds`, `RB3_splits`, `IJ2_ic_injections`, `FG1_components`, `FG2_single_edge_wires`
- Log per-face statistics (how many rebinds, how many components, etc.)
- Validate against baseline (prior Pass 2) to detect anomalies

**O4: Clippy & Formatting**
```bash
cargo clippy -p truck-shapeops -- -D warnings
cargo fmt --check -p truck-shapeops
```
- **Threshold:** 0 new warnings, proper formatting

**O5: Determinism Tests**
```bash
cargo test -p test-harness determinism::
```
- Run same boolean operation 10 times, compare output files byte-for-byte
- **Threshold:** 100% reproducibility

---

## 6. Architecture & Data Flow

### 6.1 Current Pass 2 (Interleaved Vertex Insertion + Weaving)

```
IC Polyline (vertices + edges)
    ↓
insert_ic_vertex_into_boundary_wire()
    ├─ Find insertion position in face boundary wire
    ├─ Split boundary wire at position
    ├─ Insert IC vertex
    ↓
weave_ic_edges_back_into_boundary_wires()
    ├─ Create derived wires (complements)
    ├─ Splice IC edges into boundary wire sequence
    └─ Interleave with boundary geometry
    ↓
FBG Processes Merged Wires
    ├─ from_loops(boundary_wires + derived_wires)
    ├─ build_half_edge_graph()
    ├─ radial_sort()
    ├─ extract_cycles()
    ├─ build_fragments()
    └─ Result: face divided into fragments
```

**Problem:** Interleaved insertion + weaving is complex. Wire splicing, vertex insertion, and edge weaving are tightly coupled. Difficult to reason about correctness.

---

### 6.2 New Direct Injection Architecture

```
Phase 1: Prepare (Pass 1 — unchanged)
    ├─ Compute IC polylines (mesh or analytical)
    ├─ Clip to face domain
    ├─ Pre-split sub-edges at IC crossings
    └─ Create canonical IC vertices

Phase 2: Rebind Vertices (NEW Pass 2.1)
    IC Polyline + Pre-split Sub-edges
    ↓
rebind_presplit_vertices()
    ├─ For each IC vertex position:
    │  ├─ Find nearest pre-split sub-edge endpoint (RB1)
    │  ├─ If no match, edge-split at IC position (RB3)
    │  └─ change_vertex(sub_edge, ic_vertex)
    └─ Result: All IC vertex endpoints share VertexID with boundary edges

Phase 3: Inject IC Edges (NEW Pass 2.2)
    Rebinded Sub-edges + IC Edges
    ↓
inject_ic_edges_direct()
    ├─ For each IC edge:
    │  ├─ If closed: add_independent_loop()
    │  ├─ If open: create BoundaryWire with status (And/Or)
    │  └─ Push to face.boundary_wires
    └─ Result: face.boundary_wires = [boundary edges] + [IC edges as separate wires]

Phase 4: FBG Enhancement (UPDATED Pass 3)
    All Boundary Wires + IC Wires
    ↓
enhance_fbg_for_ic_edges()
    ├─ from_loops(all_wires)
    │  ├─ Build half-edge graph from all wires
    │  ├─ Vertex mapping: VertexID (exact match, not fuzzy search)
    │  └─ Create half-edge pairs
    ├─ Handle disconnected components (FG1)
    ├─ Assign status to single-edge IC wires (FG2)
    ├─ extract_cycles()
    ├─ build_fragments()
    └─ Result: face divided into fragments with correct topology
```

**Key Insight:** FBG's `from_loops` is **vertex-agnostic**. It doesn't care whether edges are pre-woven or arrive separately. It builds a half-edge graph using vertex VertexID as the key. So long as all vertex endpoints have matching VertexIDs (guaranteed by RB1/RB3), the graph structure is correct.

---

### 6.3 IC Vertex Identity Through the Pipeline

```
Canonical IC Vertex (created in Pass 1)
    ├─ VertexID = global unique ID
    └─ Position = (u, v) on face parameter space

Sub-edge Split at IC Crossing (Pass 1)
    ├─ Split sub-edge at IC parameter value
    ├─ Create new endpoint vertex (temporary, local copy)
    └─ Position ≈ canonical IC vertex position (tolerance=tol)

Vertex Rebinding (Pass 2.1 — RB1)
    ├─ Find rebinding target: sub-edge endpoint within tol of canonical position
    ├─ change_vertex(sub_edge, canonical_ic_vertex_id)
    └─ Result: sub-edge.v2 = canonical_ic_vertex_id (deterministic)

FBG Graph Construction (Pass 4)
    ├─ from_loops(all_wires)
    ├─ Vertex key = VertexID (numeric, deterministic)
    ├─ Pair half-edges by VertexID match
    └─ Result: IC endpoint edges pair with boundary edges by ID, not position
```

**Why This Works:**
- Vertex rebinding ensures all edges incident to the same position have the same `VertexID`
- FBG uses `VertexID` as the canonical key (not fuzzy `search_parameter`)
- No need for complex position-based matching in FBG (removed fuzzy tolerance)
- Result is deterministic and invariant to floating-point drift

---

## 7. Implementation Phases

### Phase 1: Vertex Rebinding (RB1 + RB3)
**Goal:** Replace pre-split sub-edge endpoints with canonical IC vertices.

**Tasks:**
1. Implement `rebind_presplit_vertices(face, ic_vertices) → Result`
2. Implement RB1 branch (distance check + change_vertex)
3. Implement RB3 branch (edge splitting + rebinding)
4. Add unit tests: RB1, RB3, RB3+RB1 sequence
5. Run truck-shapeops tests (should still pass with new code path)

**Checklist:**
- [ ] RB1 rebinding logic correct
- [ ] RB3 edge splitting logic correct
- [ ] No vertex ID collisions
- [ ] All 331 truck tests still pass
- [ ] Deterministic results (same input → same output)

---

### Phase 2: IC Edge Injection (IJ1 + IJ2 + IJ4)
**Goal:** Add IC edges as separate BoundaryWires with correct status.

**Tasks:**
1. Implement `inject_ic_edges_direct(face, ic_edges, ic_status) → Result`
2. Implement IJ1 branch (closed IC → independent loop)
3. Implement IJ2 branch (open IC → single-edge BoundaryWire)
4. Implement IJ4 branch (status complement logic)
5. Add unit tests: IJ1, IJ2, IJ4, IJ2+IJ4 pair
6. Run truck-shapeops tests

**Checklist:**
- [ ] IJ1 closed IC handling correct
- [ ] IJ2 open IC insertion correct
- [ ] IJ4 status complement correct (And ↔ Or)
- [ ] All 331 truck tests still pass
- [ ] No regressions on test-harness

---

### Phase 3: FBG Enhancement (FG1 + FG2 + FG3)
**Goal:** Update FBG to handle disconnected components and single-edge IC wires.

**Tasks:**
1. Implement `enhance_fbg_for_ic_edges(all_wires) → Vec<Fragment>`
2. Implement FG1 branch (connected component decomposition)
3. Implement FG2 branch (single-edge IC wire status assignment)
4. Implement FG3 branch (exact VertexID matching in graph construction)
5. Add unit tests: FG1, FG2, FG3, multi-component faces
6. Run full test suite (truck + test-harness)

**Checklist:**
- [ ] Disconnected components handled correctly
- [ ] Single-edge IC wires get correct status
- [ ] Graph construction uses VertexID (not fuzzy matching)
- [ ] All 331+ truck tests still pass
- [ ] All test-harness boolean tests pass (excl. pre-existing)

---

### Phase 4: Integration & Fallback
**Goal:** Wire new code into loops_store pipeline with fallback to legacy path.

**Tasks:**
1. Update `create_loops_stores` to call new phases in sequence
2. Add shadow mode flag (run both new + legacy, compare results)
3. Implement fallback: if new path fails, revert to legacy
4. Add integration tests
5. Run full test suite

**Checklist:**
- [ ] New path integrated into main pipeline
- [ ] Shadow mode produces correct diagnostics
- [ ] Fallback works correctly
- [ ] All 331+ truck tests pass
- [ ] All test-harness boolean tests pass
- [ ] Zero flaky tests

---

## 8. Testing Strategy

### 8.1 Unit Tests

**RB1 (Simple Rebinding):**
```
test_rb1_simple_rebind
  • Create face with boundary wire
  • Create pre-split sub-edge at IC position
  • Create canonical IC vertex
  • Call rebind_presplit_vertices()
  • Assert: sub-edge endpoint VertexID = canonical vertex ID
```

**RB3 (Edge Split + Rebind):**
```
test_rb3_edge_split_for_missing_presplit
  • Create face with boundary wire (NOT pre-split)
  • Create canonical IC vertex at position on boundary wire
  • Call rebind_presplit_vertices()
  • Assert: edge was split at IC position
  • Assert: new endpoint rebinded to canonical vertex
```

**IJ2 (Open IC Injection):**
```
test_ij2_open_ic_injection
  • Create face with rebinded boundary endpoints
  • Create open IC edge with endpoints at rebinded positions
  • Call inject_ic_edges_direct(ic_edge, status=And)
  • Assert: BoundaryWire created with 1 edge
  • Assert: status = And
  • Assert: boundary_wires contains the IC wire
```

**IJ4 (Status Complement):**
```
test_ij4_status_complement
  • Create IC edge with status=And for face0
  • Call inject_ic_edges_direct(face0, face1, ic_edge)
  • Assert: face0 wire status = And
  • Assert: face1 wire status = Or
```

**FG1 (Disconnected Components):**
```
test_fg1_multi_component_face
  • Create face with two disconnected regions (e.g., ring shape)
  • Add IC edges to each region
  • Call enhance_fbg_for_ic_edges()
  • Assert: ≥2 connected components found
  • Assert: fragments extracted per component
  • Assert: total fragments = sum of component fragments
```

**FG2 (Single-Edge IC Wire):**
```
test_fg2_single_edge_ic_wire_status
  • Create BoundaryWire with 1 IC edge, status=And
  • Build graph with this wire
  • Assert: half-edge pair created
  • Assert: forward HE status = And
  • Assert: reverse HE status = Or
```

---

### 8.2 Integration Tests

**Full Face Division:**
```
test_ic_loop_restructuring_full_flow
  • Create sphere + cube intersection (simple geometry)
  • Extract IC polylines
  • Run full pipeline: rebind → inject → enhance_fbg → divide_face
  • Assert: face divided into correct number of fragments
  • Assert: fragment status correct (And/Or)
  • Assert: volume properties hold (union > A+B, intersection < min(A,B))
```

**Test Suite Coverage:**
```
cargo test -p truck-shapeops
  • All 331+ tests pass
  • No new failures
  • No flaky tests (run --test-threads=1)

cargo test -p test-harness boolean_
  • All boolean_* modules pass
  • Excluding pre-existing: ec3, q4, q5, rb2/5/8, mo4
  • No regressions
```

---

### 8.3 Shadow Mode Validation

Run new code path in parallel with legacy path for every boolean operation:

```
struct ShadowModeStats {
  rb1_count: u64,          // RB1 rebinds performed
  rb3_count: u64,          // RB3 edge splits performed
  ij2_count: u64,          // IJ2 IC injections (open)
  ij1_count: u64,          // IJ1 IC injections (closed)
  fg1_components: u64,     // FG1 disconnected components found
  fg2_single_edges: u64,   // FG2 single-edge IC wires
  new_path_result: bool,   // Did new path succeed?
  legacy_path_result: bool,// Did legacy path succeed?
  results_match: bool,     // Do outputs match?
}
```

**Log per-operation, compare stats across test suite.**

---

## 9. Acceptance Criteria

**AC1:** All 331+ truck-shapeops tests pass with new code path (Phase 1-3 integration).

**AC2:** All test-harness boolean tests pass (excluding pre-existing failures).

**AC3:** Zero regressions on currently-passing tests.

**AC4:** Code compiles with `cargo clippy -D warnings` (zero new warnings).

**AC5:** Code is properly formatted (`cargo fmt --check`).

**AC6:** Determinism tests pass (10 repeated runs of same operation → identical output).

**AC7:** Shadow mode diagnostics logged and analyzed (stats match expectations).

**AC8:** Fallback to legacy path works correctly if new path fails (tested explicitly).

---

## 10. References

- **Pass 1 (IC Computation):** `truck-shapeops/src/transversal/loops_store/mod.rs:1-600` (IC extraction + clipping + pre-split)
- **Current Pass 2 (Interleaved):** `truck-shapeops/src/transversal/loops_store/mod.rs:600-900` (legacy code to be replaced)
- **FBG Algorithm:** `truck-shapeops/src/transversal/face_boundary_graph.rs` (from_loops, extract_cycles, build_fragments)
- **Deterministic IDs:** `truck-base/src/id.rs`, `truck-topology/src/vertex.rs` (SequentialID, VertexID)
- **Test Suite:** `crates/test-harness/tests/assay_*.rs`, `crates/test-harness/src/cases/` (boolean_* test modules)

---

## End of Specification
