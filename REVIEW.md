# Waffle Iron — Comprehensive Design Review

**Date:** 2026-02-10
**Scope:** Full system assessment after Phase 1 sprint (sub-projects 01–09)
**Purpose:** Honest gap analysis before building the test harness (sub-project 11)

---

## Section 1: System Status Matrix

Status codes: **W** = Working, **S** = Stubbed, **B** = Broken/Fragile, **M** = Missing, **P** = Partial

| Feature | Rust Engine | WASM Bridge | UI | End-to-End |
|---------|:-----------:|:-----------:|:--:|:----------:|
| **Sketch drawing (line/rect/circle/arc)** | W | W | W | **W** |
| **Sketch constraints (all types)** | W | W (via libslvs JS) | W | **W** |
| **Sketch profile extraction** | W | W | W | **W** |
| **Construction geometry** | W | W | W | **W** |
| **Extrude** | **S** (hardcoded 1x1 square) | W (message dispatch) | **M** (no dialog) | **B** |
| **Revolve** | **S** (hardcoded 1x1 square) | W (message dispatch) | **M** (no dialog) | **B** |
| **Fillet** | W (MockKernel) / **M** (TruckKernel) | W (message dispatch) | **M** (no dialog) | **B** |
| **Chamfer** | W (MockKernel) / **M** (TruckKernel) | W (message dispatch) | **M** (no dialog) | **B** |
| **Shell** | W (MockKernel) / **M** (TruckKernel) | W (message dispatch) | **M** (no dialog) | **B** |
| **Boolean union** | W (MockKernel) / **B** (TruckKernel) | W | **M** (no dialog) | **B** |
| **Boolean subtract** | W (MockKernel) / **B** (TruckKernel) | W | **M** (no dialog) | **B** |
| **Boolean intersect** | W (MockKernel) / **B** (TruckKernel) | W | **M** (no dialog) | **B** |
| **Feature tree CRUD** | W | W | W | **W** |
| **Undo/redo** | W | W | W | **W** |
| **Rollback slider** | W | W | W | **W** |
| **Feature rename** | W | W | W | **W** |
| **Feature reorder** | W | W | W | **W** |
| **Feature suppress** | W | W | W | **W** |
| **GeomRef persistent naming** | W | W | N/A | **P** (tested with MockKernel only) |
| **3D viewport rendering** | N/A | N/A | W | **W** |
| **Face picking (hover + click)** | N/A | N/A | W | **W** |
| **Edge overlay** | N/A | N/A | W | **W** |
| **View cube gizmo** | N/A | N/A | W | **W** |
| **Camera controls (orbit/snap/fit)** | N/A | N/A | W | **W** |
| **Sketch-on-face** | **M** | **M** | **P** (button disabled) | **M** |
| **File save (JSON)** | W | W | **M** (no dialog) | **P** |
| **File load (JSON)** | W | W | **M** (no dialog) | **P** |
| **STEP export** | W (native only) | **M** (returns NotImplemented) | **M** | **M** |
| **STL export** | **M** | **M** | **M** | **M** |
| **Units system** | **M** | **M** | **M** | **M** |

### Summary

- **9 features fully working end-to-end** (sketch drawing, constraints, profiles, construction, feature tree CRUD, undo/redo, rollback, rename, reorder/suppress)
- **5 viewport features working** (rendering, face picking, edge overlay, view cube, camera)
- **6 features stubbed/broken** (extrude, revolve, fillet, chamfer, shell, boolean)
- **4 features partially working** (save, load, GeomRef, sketch-on-face)
- **4 features completely missing** (STEP export in browser, STL export, units, feature creation dialogs)

---

## Section 2: Critical Path to "Sketch → Extrude → Solid"

The core parametric CAD workflow — draw a sketch, extrude it into a 3D solid — is **currently broken**. All three pieces exist individually but aren't connected.

### Gap 1: Hardcoded Geometry in rebuild.rs

**File:** `crates/feature-engine/src/rebuild.rs`

**Extrude (lines 93–126):**
```rust
Operation::Extrude { params } => {
    let _sketch_result = find_sketch_result(params.sketch_id, feature_results)?;
    // ^^^ Fetched but UNUSED (underscore prefix)

    let profiles = vec![waffle_types::ClosedProfile {
        entity_ids: vec![1, 2, 3, 4], // placeholder
        is_outer: true,
    }];
    let mut positions = HashMap::new();
    positions.insert(1, (0.0, 0.0));  // hardcoded 1x1 square
    positions.insert(2, (1.0, 0.0));
    positions.insert(3, (1.0, 1.0));
    positions.insert(4, (0.0, 1.0));
```

**Revolve (lines 128–166):** Identical hardcoded 1x1 square.

**Root cause:** The `execute_feature()` function calls `find_sketch_result()` to get the sketch's `OpResult`, but sketches produce **empty OpResults** (lines 77–91: `outputs: Vec::new()`). The actual sketch data lives in `Operation::Sketch { sketch }` in the feature tree, but `execute_feature()` only passes `feature_results` (the OpResult map), not the full tree.

**Fix needed:**
1. When executing Extrude/Revolve, look up the referenced sketch's `Operation::Sketch { sketch }` from the feature tree (which IS available — `rebuild()` receives `tree: &FeatureTree` at line 27)
2. Use the sketch's solved profiles and positions instead of hardcoded values
3. Select the correct profile via `params.profile_index`

### Gap 2: No Feature Creation Dialogs

**File:** `app/src/lib/ui/Toolbar.svelte` (lines 40–56)

Clicking "Extrude" only calls `setActiveTool('extrude')` — no dialog captures parameters. The entire feature creation UI is missing:

| Feature | What Exists | What's Missing |
|---------|------------|----------------|
| Extrude | Toolbar button (line 24) | Depth input, profile selector, direction, cut toggle |
| Revolve | Toolbar button (line 25) | Angle input, axis definition, profile selector |
| Fillet | Toolbar button (line 26) | Edge selection, radius input |
| Chamfer | Toolbar button (line 27) | Edge selection, distance input |
| Shell | Toolbar button (line 28) | Face selection, thickness input |
| Boolean | No button | Everything |

The `PropertyEditor.svelte` (lines 46–79) can **edit** parameters of existing features but has no creation workflow. The `AddFeature` message exists in the WASM bridge (`dispatch.rs:73–77`) but is **never sent** by the UI (confirmed: `grep -r "AddFeature" app/src/` returns zero results).

Only `FinishSketch` (`Toolbar.svelte:65–69`) creates features.

### Gap 3: Missing pointerup in SketchInteraction

**File:** `app/src/lib/sketch/SketchInteraction.svelte` (lines 82–84)

```svelte
<T.Mesh ...
    onpointerdown={(e) => onPointerEvent('pointerdown', e)}
    onpointermove={(e) => onPointerEvent('pointermove', e)}
/>
```

No `onpointerup` handler. The tools system (`tools.js:115`) declares it accepts `'pointerup'` events, but the SketchInteraction component never dispatches them. This means tools that depend on pointerup (e.g., rectangle completion, arc endpoint) may only work via pointerdown.

### Gap 4: Sketch Always on XY Plane

**File:** `app/src/lib/ui/Toolbar.svelte` (line 46)

```javascript
enterSketchMode([0, 0, 0], [0, 0, 1]);  // Always XY plane at origin
```

The "Sketch on Face" button exists in the viewport context menu (`ViewportContextMenu.svelte:44`) but is gated on `hasSelection` which checks `selectedRefs` — a face must be selected first. The flow to extract a plane from a selected face's GeomRef and pass it to `enterSketchMode` is **not implemented**.

### Minimum Fix Sequence

1. **Fix rebuild.rs** (~2 hours): Read actual sketch data from `tree.features` instead of hardcoded values
2. **Add extrude dialog** (~4 hours): Modal or property-panel UI that captures depth/direction/profile, sends `AddFeature` with `Operation::Extrude`
3. **Add pointerup** (~15 min): Add `onpointerup` handler to SketchInteraction.svelte
4. **Wire sketch-on-face** (~4 hours): Extract plane from selected face GeomRef, pass to enterSketchMode

---

## Section 3: Architecture Assessment

### truck as BREP Kernel — Mixed

**Strengths:**
- Rust-native, compiles to WASM without FFI
- Good tessellation with face-range metadata for GPU picking
- tsweep/rsweep work reliably for extrude/revolve
- STEP export (AP203) works for simple solids (`truck_kernel.rs:55–70`)
- Clean trait abstraction (`Kernel` + `KernelIntrospect`) means swapping kernels doesn't affect other crates

**Weaknesses verified in code:**
- **Fillet:** `TruckKernel::fillet_edges()` returns `NotSupported` (`truck_kernel.rs:222–231`)
- **Chamfer:** `TruckKernel::chamfer_edges()` returns `NotSupported` (`truck_kernel.rs:233–242`)
- **Shell:** `TruckKernel::shell()` returns `NotSupported` (`truck_kernel.rs:244–253`)
- **Boolean fragility:** `boolean_union/subtract/intersect` are implemented (`truck_kernel.rs:136–220`) but wrap `truck_shapeops::and()` which returns `Option<Solid>` — `None` on failure with no diagnostics. Known failures: box-cylinder, coplanar faces, near-tangent surfaces.
- **No primitives:** No `make_box()`, `make_cylinder()` — everything via successive sweeps

**Options:**
- **A) Stay with truck:** Accept no fillet/chamfer/shell. Focus on extrude/revolve pipeline. Defer advanced operations.
- **B) OpenCascade-rs:** Would provide fillet/chamfer/shell/robust booleans. But: C++ FFI, WASM build complexity, larger binary.
- **C) Hybrid:** Use truck for extrude/revolve/tessellation. Add targeted native implementations for fillet (cylindrical surface construction) and chamfer (planar cut).

### Two-WASM-Module Solver — Working Well

**Architecture:** Rust engine via wasm-pack (1.9MB) + libslvs via Emscripten (226KB)

- Rust WASM handles feature tree, rebuild, tessellation
- libslvs WASM handles constraint solving via dedicated JS bridge (`worker.js:173–176`)
- SolveSketch is feature-gated: native builds use Rust-linked libslvs, WASM builds use the Emscripten module
- Performance: 1.6ms–8.7ms for 14–301 constraints (verified in `projects/02-sketch-solver/PLAN.md:65–70`)

**Assessment:** Clean separation, works correctly. Long-term, a pure Rust constraint solver would simplify the build but is not urgent.

### GeomRef Persistent Naming — Well-Designed, Under-Tested

**Implementation:** Complete 3-tier resolution in `crates/feature-engine/src/resolve.rs`

1. **Role-based** (fast, stable) — semantic roles from modeling-ops (`EndCapPositive`, `SideFace`, etc.)
2. **Signature-based** (fallback) — weighted similarity across surface type (3.0), area (2.0), centroid (2.0), normal (2.0), edge length (2.0). Threshold: 0.7 for operations, 0.5 for fallback resolution.
3. **Query-based** — stub, not implemented

**Gap:** All testing uses MockKernel, which produces deterministic synthetic topology. GeomRef has never been tested against real TruckKernel geometry where IDs change unpredictably after booleans. The MockKernel deliberately re-IDs entities on fillet/chamfer/shell (with `signature_similarity` threshold 0.7), which is a good simulation but not a substitute for real topology changes.

### Message-Based Bridge — Clean and Extensible

**19 message types** in `crates/wasm-bridge/src/dispatch.rs`:

| Category | Messages | Status |
|----------|----------|--------|
| Sketch | BeginSketch, AddSketchEntity, AddConstraint, SolveSketch, FinishSketch | All working |
| Features | AddFeature, EditFeature, DeleteFeature, SuppressFeature, ReorderFeature, RenameFeature | All working |
| History | Undo, Redo, SetRollbackIndex | All working |
| Selection | SelectEntity, HoverEntity | All working |
| File | SaveProject, LoadProject | Working |
| Export | ExportStep | Returns NotImplemented in WASM |

**Assessment:** Well-designed protocol. Near-zero-copy mesh transfer via TypedArray views into WASM linear memory. Easy to extend.

### Svelte 5 Runes Store — Functional but Large

**File:** `app/src/lib/engine/store.svelte.js` (~720 lines)

Single file managing: engine state, feature tree, meshes, selection, hover, sketch mode, tool state, undo/redo dispatch, all WASM bridge communication.

**Assessment:** Works, but should eventually be split into focused modules (engine, selection, sketch, tools). Not urgent — the runes reactivity model keeps it manageable.

---

## Section 4: Gap Analysis

### Showstoppers (blocks core parametric workflow)

| Gap | Impact | Location | Fix Complexity |
|-----|--------|----------|---------------|
| **Hardcoded sketch data in rebuild.rs** | Extrude/revolve ignore actual sketch | `crates/feature-engine/src/rebuild.rs:99–107, 132–140` | Medium — need to thread sketch data through |
| **No feature creation dialogs** | Cannot create extrude/revolve/fillet/chamfer/shell from UI | `app/src/lib/ui/Toolbar.svelte:40–56` | Medium — need modal/panel + AddFeature dispatch |
| **Missing pointerup in SketchInteraction** | Tool completion events lost | `app/src/lib/sketch/SketchInteraction.svelte:82–84` | Trivial — add one event handler |

### High Impact (blocks important features)

| Gap | Impact | Location | Fix Complexity |
|-----|--------|----------|---------------|
| **Boolean reliability** | Cut extrude, boolean combine fail on real geometry | `crates/kernel-fork/src/truck_kernel.rs:136–220` | Hard — truck limitation |
| **Fillet/chamfer/shell NotSupported** | Three key modeling operations unavailable with real kernel | `crates/kernel-fork/src/truck_kernel.rs:222–253` | Hard — truck doesn't have these |
| **No sketch-on-face** | Can only sketch on XY plane at origin | `app/src/lib/ui/Toolbar.svelte:46` | Medium — need plane extraction from GeomRef |
| **No file save/open UI** | Save/load works in engine but no browser dialogs | No file dialog components found in `app/src/lib/ui/` | Medium — need file picker + download |

### Medium Impact (gaps a user would notice)

| Gap | Impact | Location | Fix Complexity |
|-----|--------|----------|---------------|
| **No units system** | All dimensions are unitless floats | No `units` references found anywhere in codebase | Medium — pervasive change |
| **No STEP/STL export from browser** | ExportStep returns NotImplemented in WASM | `crates/wasm-bridge/src/dispatch.rs:163–165` | Hard — needs TruckKernel in WASM or server-side |
| **No construction plane selector** | Cannot create datums for sketch planes | No datum creation UI found | Medium |
| **Query-based GeomRef selector not implemented** | Falls back to error for complex selections | `crates/feature-engine/src/resolve.rs` (stub) | Medium |

### Low Impact (polish)

| Gap | Impact | Location | Fix Complexity |
|-----|--------|----------|---------------|
| **No tangent/perpendicular snap detection during drawing** | Snap system has coincident + H/V only | `app/src/lib/sketch/snap.js` | Low |
| **Revolve role detection heuristic** | RevStartFace/RevEndFace assignment unreliable on non-planar results | `crates/modeling-ops/src/revolve.rs` | Low |
| **WASM binary size (1.9MB)** | Slow initial load | `crates/wasm-bridge/` | Low — can add wasm-opt |
| **Store.svelte.js is 720 lines** | Maintainability concern | `app/src/lib/engine/store.svelte.js` | Low — split when needed |

---

## Section 5: Proposed Directions

### Direction A: "Connect the Dots" (smallest scope)

**Goal:** Make the core sketch → extrude → 3D solid pipeline work end-to-end.

**Steps:**
1. Fix `rebuild.rs` to read actual sketch profiles from `FeatureTree` (not hardcoded)
2. Add `onpointerup` to `SketchInteraction.svelte`
3. Build extrude dialog (modal or property panel): depth, direction, profile index
4. Wire dialog to send `AddFeature { operation: Extrude { ... } }` to WASM bridge
5. Test: draw rectangle → finish sketch → extrude → see 3D box

**Defers:** Booleans, fillet, chamfer, shell, sketch-on-face, file dialogs, export

**Scope:** ~3 focused sessions

**Risk:** Low. All pieces exist; this is wiring work.

### Direction B: "Functional MVP" (medium scope)

**Goal:** A usable parametric modeler for simple parts (single-body, extrude/revolve only).

**Steps (includes all of Direction A, plus):**
6. Add revolve dialog
7. Wire sketch-on-face (extract plane from selected face GeomRef)
8. Add file save/open dialogs (browser download + file picker)
9. Add remaining feature dialogs (fillet, chamfer, shell) — will work with MockKernel for testing even if TruckKernel can't do them
10. Investigate boolean tolerance tuning for simple cases (box-box works)

**Defers:** Kernel swap, STL export, units, assemblies

**Scope:** ~8 focused sessions

**Risk:** Medium. Boolean reliability is unpredictable.

### Direction C: "Kernel Decision" (strategic)

**Goal:** Resolve the truck limitation permanently before building more on top.

**Steps:**
1. Evaluate [opencascade-rs](https://github.com/bschwind/opencascade-rs) maturity
   - Does it compile to WASM?
   - Does it have fillet, chamfer, shell operations?
   - What's the binary size?
   - How do boolean operations perform?
2. If viable: implement `OpenCascadeKernel` behind the existing `Kernel` + `KernelIntrospect` traits
3. If not viable: document truck limitations formally, ship with workarounds
4. Proceed with Direction B on whichever kernel

**Defers:** Everything else until kernel decision is made

**Scope:** ~2 sessions for evaluation, ~6 sessions for kernel swap if viable

**Risk:** High. OpenCascade-rs may not be WASM-ready. Kernel swap is a large change even with the trait abstraction.

### Recommendation

**Start with Direction A.** It delivers the most critical user-visible improvement (a working parametric pipeline) with the lowest risk. Direction B can follow incrementally. Direction C should be a parallel investigation that doesn't block progress.

---

## Section 6: Test Coverage Analysis

### What 216 Tests Cover

| Crate | Tests | Coverage Focus |
|-------|:-----:|---------------|
| kernel-fork | 35 (+2 ignored) | MockKernel topology, TruckKernel extrude/tessellate/STEP export, edge extraction |
| sketch-solver | 31 | Entity/constraint mapping, solve status, profile extraction, dragged constraint, performance |
| wasm-bridge | 21 | Message dispatch for all 19 types, error handling, sketch workflow |
| feature-engine | 49 | Feature tree CRUD, rebuild algorithm, GeomRef resolution, undo/redo, rollback, persistent naming stress |
| modeling-ops | 54 | Extrude/revolve/fillet/chamfer/shell/boolean provenance, topology diff, role assignment, signature similarity |
| file-format | 26 | Save/load round-trip, format validation, version migration, STEP export |

**Total: 216 passing, 2 ignored** (ignored tests are TruckKernel boolean tests marked as known-fragile)

### What Tests Miss

| Gap | Why It Matters |
|-----|---------------|
| **Actual sketch → extrude pipeline** | The core user workflow is untested end-to-end because rebuild.rs uses hardcoded geometry |
| **Real TruckKernel topology changes** | GeomRef resolution only tested with MockKernel's predictable IDs |
| **UI interactions** | No Svelte component tests, no browser-based testing |
| **WASM-in-browser** | Tests run via `cargo test` (native), not in actual WebWorker/WASM context |
| **Cross-crate integration** | Each crate is tested in isolation; no test exercises the full stack (UI → bridge → engine → kernel → tessellation → render) |
| **Error recovery** | What happens when rebuild fails mid-tree? When GeomRef resolution fails with BestEffort? |
| **Performance under load** | MockKernel rebuild is ~180µs for 10 features; TruckKernel may be seconds per boolean |

### MockKernel vs TruckKernel Test Coverage

| Operation | MockKernel Tests | TruckKernel Tests |
|-----------|:---:|:---:|
| make_faces + extrude | 1 | 1 |
| tessellate | 1 | 1 |
| edge extraction | 2 | 2 |
| STEP export | 0 | 2 |
| boolean union | 1 | 0 (ignored) |
| boolean subtract | 1 | 0 (ignored) |
| boolean intersect | 1 | 0 (ignored) |
| fillet | 3 | 0 (NotSupported) |
| chamfer | 2 | 0 (NotSupported) |
| shell | 3 | 0 (NotSupported) |
| topology introspect | 6 | 0 |
| signature compute | 2 | 0 |

**Key insight:** MockKernel has excellent test coverage (21 tests). TruckKernel has minimal coverage (6 tests + 2 STEP export). The gap is intentional (MockKernel is deterministic and fast) but means real geometry operations are largely untested.

### Recommendations for Test Harness (Sub-Project 11)

1. **End-to-end pipeline test:** Create a sketch with known geometry → add extrude feature → verify resulting solid has expected topology (requires fixing rebuild.rs first)
2. **TruckKernel integration tests:** Run the same modeling-ops tests against TruckKernel where operations are supported (extrude, revolve, boolean)
3. **Cross-crate integration:** Test the full path from `UiToEngine` message through dispatch → engine → modeling-ops → kernel → tessellation
4. **GeomRef with real topology:** Create a box, fillet an edge, verify the fillet face can be referenced by GeomRef in a subsequent operation (MockKernel already tests this; needs TruckKernel validation when fillet is implemented)
5. **WASM build verification:** At minimum, verify `wasm-pack build` succeeds and the 8 exported functions are present

---

## Appendix: Key File Reference

| File | Lines | Purpose |
|------|------:|---------|
| `crates/feature-engine/src/rebuild.rs` | ~200 | Feature rebuild algorithm; **hardcoded geometry at lines 99–107, 132–140** |
| `crates/feature-engine/src/resolve.rs` | ~110 | GeomRef resolution with 3-tier fallback |
| `crates/feature-engine/src/lib.rs` | ~300 | Engine public API (add/edit/remove/undo/redo) |
| `crates/kernel-fork/src/truck_kernel.rs` | ~270 | TruckKernel; **NotSupported at lines 222–253** |
| `crates/kernel-fork/src/mock_kernel.rs` | ~500 | MockKernel with full operation support |
| `crates/modeling-ops/src/extrude.rs` | ~100 | Extrude with role assignment |
| `crates/modeling-ops/src/diff.rs` | ~220 | Topology diff + signature similarity |
| `crates/wasm-bridge/src/dispatch.rs` | ~170 | 19 message handlers; **ExportStep NotImplemented at line 163** |
| `crates/wasm-bridge/src/messages.rs` | ~80 | UiToEngine/EngineToUi message definitions |
| `crates/file-format/src/step_export.rs` | 42 | STEP export (native TruckKernel only) |
| `app/src/lib/engine/store.svelte.js` | ~720 | Central state management (runes) |
| `app/src/lib/ui/Toolbar.svelte` | ~120 | Tool buttons; **no creation dialogs** |
| `app/src/lib/ui/PropertyEditor.svelte` | ~140 | Parameter editing (read: editing, not creation) |
| `app/src/lib/sketch/SketchInteraction.svelte` | 86 | Pointer events; **missing onpointerup at line 84** |
| `app/src/lib/sketch/tools.js` | ~464 | Drawing tool implementations |
| `app/src/lib/viewport/CadModel.svelte` | ~210 | Face picking via binary search on faceRanges |
| `app/src/lib/viewport/CameraControls.svelte` | ~160 | Orbit, snap, fit, align |
