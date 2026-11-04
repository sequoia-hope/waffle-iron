# Board Shape Feature Summary (`agy/board-shape`)

**Worktree:** `/home/claude/wt/agy-board-shape`  
**Branch:** `agy/board-shape` (base commit `246757b1`)  
**Commits:**
- `1c9e6365` — `docs(specs): specify Operation::BoardShape authoring and evaluation (B1)`
- `c4184e8d` — `feat(feature-engine): implement Operation::BoardShape evaluation and plumbing (B2)`
- `e0a54998` — `docs(agy): add board-shape implementation and B3 summary (B4)`
- `f3b92360` — `fix(feature-engine): use TAU_MODEL for non-adjacent same-loop proximity`
- `1aeff136` — `fix(feature-engine): drop silent MIN_FEATURE_SIZE coincident welding`
- `a4c1aa1a` — `refactor(feature-engine): share ARC_SAMPLES, COS_FACE_NORMAL_ALIGNMENT, and fnv1a_128`
- `d7eb422a` — `docs(board-shape): align summary and spec tolerances, cutter span, and interface paths`

**Kernel / audit impact:** Zero changes to `yang-rs`, `kernel-v2`, `cherchi-rs`, or `ssi-rs`. Neither `docs/yang_deviations.md` nor `docs/audits/yang_2025_audit.md` was modified.

---

## 1. Deliverable B1 — Design Note (`specs/board_shape_feature.md`)

Committed before code (`1c9e6365`), `specs/board_shape_feature.md` specifies:

- **Operation & parameters:** `Operation::BoardShape { params: BoardShapeParams }` referencing an upstream `Sketch` feature (`sketch_id: Uuid`), logical board name (`board: String`), board thickness in meters (`thickness: f64`) with optional mm-space driving expression (`thickness_expr: Option<String>`), and sparse per-entity role map (`roles: BTreeMap<u32, BoardEntityRole>`) serialized with decimal string keys (`"12": { "type": "outline" }`) and loud deserialization refusal on non-`u32` or duplicate keys.
- **Entity roles:** `BoardEntityRole::Outline`, `BoardEntityRole::Cutout`, `BoardEntityRole::Hole { plated: bool, clearance: f64 }`, and `BoardEntityRole::Keepout { side: BoardSide, max_height: f64 }` (`BoardSide::{Front, Back, Both}`).
- **Exact `{Line, Arc, Circle}` contour contract:** Only non-construction `SketchEntity::{Line, Arc, Circle}` may carry board roles; `Point`, `Spline`, `Gear`, and `Sprocket` are refused with `BoardShapeError::UnsupportedEntityKind` (never chord-approximated). Circles and holes record exact **diameters** (`diameter_m = 2.0 * radius`) per `SUITE-SPEC-AGREED.md`.
- **Stable element & hole connector IDs:** Every evaluated element carries stable ID `waffle:<sketch_id>/<entity_id>` (`element_stable_id`). Each `Hole` emits a top-face `PartConnector` (`origin` at `plane_origin + u*x_axis + v*y_axis + thickness*normal`, `z_axis = normal`, `x_axis = sketch_x_axis`, `geometry = Some(ConnectorGeometry::CircularEdge(AxisKind::Cylindrical))`, `key = Some(stable_id)`) with deterministic 128-bit FNV-1a UUID `hole_connector_uuid(board_feature_id, &stable_id)` (`feature_engine::fnv1a_128`) so assembly mates survive rebuilds and hole edits.
- **Format compatibility (no version / reader-floor bump):** `FORMAT_VERSION` and `MIN_READER_VERSION` stay at `15`. Unlike `Projection` or `Annotation` variants inside `Drawing` tabs, `Operation` already uses `crate::opaque::known_or_unknown` (`Operation::Unknown(Value)`, v4 Phase 1b), so older v15 readers preserve `"type": "BoardShape"` features verbatim across load → save and report `EngineError::UnsupportedOperation` rather than failing document load or silently dropping data.

---

## 2. Deliverable B2 — What Was Built

### 2.1 Core Feature Evaluation (`crates/feature-engine`)

- **`crates/feature-engine/src/board_shape.rs`** (new module, exported at `crates/feature-engine/src/lib.rs:3`):
  - Authoring & error types: `BoardSide` (`:40`), `BoardEntityRole` (`:50`), `BoardShapeParams` (`:91`, custom `u32_role_map` serializer/deserializer at `:112-172`), and `BoardShapeError` (`:178`, covering validation failure classes B2–B26; B1 reuses `EngineError::SketchNotFound`).
  - Evaluated domain model: `BoardSegment2D` (`:278`, `Line` / `Arc` / `Circle` with `diameter_m`), `EvaluatedBoardLoop` (`:307`), `EvaluatedBoardHole` (`:320`, `diameter_m`), `EvaluatedBoardKeepout` (`:338`), `EvaluatedBoardShape` (`:348`), `element_stable_id` (`:362`), and `hole_connector_uuid` (`:369`).
  - `evaluate_board_shape` (`:440`): validates sketch solve status, finite orthogonal sketch-plane basis, non-empty board name, `thickness >= MIN_FEATURE_SIZE`, entity existence, non-construction flag, `{Line, Arc, Circle}` restriction, non-negative finite `clearance` and `max_height`, non-degenerate geometry, endpoint equivalence under shared `Point` IDs or `TAU_MODEL` (`1e-7 m`) welding (no silent `MIN_FEATURE_SIZE` bridge), degree-2 closed-loop chaining, exact line/arc/circle self-intersection refusal with `TAU_MODEL` proximity on non-adjacent segments of the same loop (`:1760`, splitting non-adjacent sweep `> π/2` arcs into exact minor sub-arcs), exact curved-boundary winding + `TAU_MODEL` clearance checks (`:640-706`: `Outline` encloses `Cutout`s and `Hole`s with gap `> TAU_MODEL`; `Cutout`s pairwise disjoint and un-nested; `Hole`s disjoint from `Cutout`s and other `Hole`s; `Keepout`s closed and simple), CCW outer / CW cutout orientation normalization, and deterministic top-face `PartConnector` synthesis.
  - `execute` (`:783`): builds exact analytical faces — `ClosedProfile` (`CircleProfile`) via `kb.make_faces_from_profiles` for `Circle` outlines/cutouts/holes (`extrude_circle`, `:962`) and `Region` with exact minor `RegionEdge::Arc` segments (`sweep <= PI / 2`, `ARC_SAMPLES` polygon samples from `waffle_types::profiles::ARC_SAMPLES` at `:396`, `loop_to_region` at `:1030`) via `kb.make_face_from_region` for `Line`/`Arc` loops (`extrude_board_loop`, `:1001`) — extrudes the outer blank `[0, +thickness]` along `+normal`, and subtracts symmetric through-cutters spanning `[-thickness, +2.0 * thickness]` (`origin - thickness * normal`, `depth = 3.0 * thickness`, `:879-888`) for each cutout and hole so cutter end-caps never coincide coplanarly with board caps, then assigns `Role::EndCapPositive` and `Role::EndCapNegative` to surviving planar board caps via `COS_FACE_NORMAL_ALIGNMENT` (`waffle_types::kernel::units` `:81`, shared with `modeling_ops::{pipe,sweep}` and `MockKernel`).
- **`crates/feature-engine/src/types.rs`:**
  - Re-exports `BoardEntityRole`, `BoardShapeError`, `BoardShapeParams`, `BoardSide` (`:10`).
  - Adds `Operation::BoardShape` (`:690`), `KnownOperation::BoardShape` (`:764`), `"BoardShape"` in `OPERATION_TAGS` (`:770-790`, length `19`), `EngineError::BoardShape` (`:2241`), and `ErrorKind::BoardShape` (`:2327`, mapped at `:2435`).
- **`crates/feature-engine/src/connector.rs` (`:63`) & `crates/feature-engine/src/script/mod.rs` (`:697`):**
  - Adds `pub key: Option<String>` to `PartConnector` (`None` on `Operation::MateConnector` and Rhai script connectors; `Some("waffle:<sketch_id>/<entity_id>")` on board-hole connectors).
- **`crates/feature-engine/src/params.rs`:**
  - Wires `BoardShapeParams::thickness_expr` (mm-space `Tag::Length`, multiplied by `0.001` into meters) in `apply_parameters_scoped` (`:801`) and `expression_sites` (`:1364`), and adds `BoardShape` to `every_expression_feature` (`:2624`, bumping `every_expression_field_is_enumerated` to `49` sites at `:2674` and `every_expression_field_can_measure_and_is_enumerated_as_one` to `50` sites at `:2720`).
- **`crates/feature-engine/src/rebuild.rs` & `crates/feature-engine/src/lib.rs`:**
  - Stores `board_shapes: HashMap<Uuid, EvaluatedBoardShape>` on `RebuildState` (`rebuild.rs:150`) and `Engine` (`lib.rs:135`), carries untouched entries across incremental rebuilds via `Carried::board_shapes` (`rebuild.rs:161, 277-280`, `lib.rs:1212`), populates `Engine::connectors` in tree order from both `Operation::MateConnector` and `Operation::BoardShape` (`lib.rs:1284`), exports `fnv1a_128` (`lib.rs:1377`, shared with `wasm_bridge::assembly_view::step_product_tree` at `crates/wasm-bridge/src/assembly_view.rs:578`), and adds `active_sketch` (`rebuild.rs:1579`) which resolves projected references without calling `expand_generators()` so `Gear` and `Sprocket` entities retain their authored IDs and fail with `BoardShapeError::UnsupportedEntityKind`.

### 2.2 File Format, Bridge, Harness & Schema Plumbing

- **`crates/file-format/src/migrate.rs` (`:152`):** Scales legacy v1-to-v2 `thickness`, `Hole::clearance`, and `Keepout::max_height` by `s`.
- **`crates/wasm-bridge/src/dispatch.rs` (`:4101-4104`):** Formats default feature names `"Board shape (<board>)"` / `"Board shape"`.
- **`crates/wasm-bridge/src/messages.rs` (`:2732`) & `crates/wasm-bridge/src/tools/summary.rs` (`:177-179`):** Exposes `key: Option<String>` on `PartConnectorInfo` and includes `"key"` on `model_summary` connector entries when present.
- **`crates/test-harness/src/{workflow.rs,assertions.rs,report.rs}`:** Adds `ModelBuilder::board_shape` (`workflow.rs:1065`), `ModelBuilder::dispatch` (`workflow.rs:1521`), `ModelBuilder::rebuild_from_scratch` (`workflow.rs:1526`), and `Operation::BoardShape` formatting in assertions/reports.
- **Schema golden, manifest & WASM bundle:** Regenerated `docs/schema/waffle-v5.schema.json`, synced `app/src/lib/agent/tools/engineSchemas.generated.js` and `relay/src/waffle_mcp_relay/agent-tools.manifest.json` via `node app/scripts/gen-agent-manifest.mjs`, and rebuilt `app/static/pkg/` via `./scripts/build-wasm.sh` (fingerprint `708a93c39b8be55d020ca651c5266cecee55d65cedbb1236e3b4ad325cf512d0`).

---

## 3. Tests Added and What Each Shows

### 3.1 Real-Kernel (`KernelV2Adapter`) Suite — `crates/test-harness/tests/board_shape_kv2.rs`

All 5 tests execute through `ModelBuilder::kernel_v2()` / `KernelV2Adapter` (`--release`):

1. **`rectangle_board_with_four_corner_holes_exact_volume_connectors_and_mates` (`:102`):**
   - Builds an `80 × 50 × 1.6 mm` rectangular board with four `Ø 3.2 mm` corner holes (mixed plated/unplated and clearances).
   - Verifies exact `KernelIntrospect::solid_volume` matches `(0.080 * 0.050 - 4 * π * 0.0016²) * 0.0016` within relative error `< 1e-10`, watertight mesh, genus-4 Euler characteristic `χ = -6`, `Role::EndCapPositive` / `Role::EndCapNegative` provenance, 4 `EvaluatedBoardHole`s with `diameter_m == 0.0032`, 4 top-face `PartConnector`s keyed by `waffle:<sketch_id>/<entity_id>` (including `model_summary` JSON output), `wasm_bridge::assembly_view::evaluate` fastening two standoff instances onto holes `21` and `25` via `hole_connector_uuid` (placing them at `[0.005, 0.005, 0.0016]` and `[0.075, 0.045, 0.0016]`), and downstream `Role::EndCapPositive` plane resolution + transverse boss boolean union into `MainBoard` with exact merged volume.
2. **`round_board_circle_outline_exact_volume` (`:512`):**
   - Builds a pure `Ø 60 mm × 1.6 mm` circular board (`SketchEntity::Circle` outline), verifying exact `solid_volume == π * 0.030² * 0.0016` (`< 1e-10`), watertightness, `χ = 2`, and `BoardSegment2D::Circle { diameter_m: 0.060, .. }`; then edits the feature via `UiToEngine::EditFeature` to add an eccentric `Ø 3.0 mm` mounting hole and verifies exact volume `π * (0.030² - 0.0015²) * 0.0016` (`< 1e-10`).
3. **`board_with_arc_corners_cutouts_keepouts_cw_winding_and_thickness_expr` (`:627`):**
   - Places a `60 × 40 mm` rounded-corner board (`R = 5 mm` 90° corner arcs, mixed CW/reversed line authoring) on an offset XZ plane (`origin = [0.01, -0.02, 0.005]`, `normal = [0, 1, 0]`) with a stadium slot cutout (two `10 mm` lines + two 180° `R = 3 mm` semicircular arcs), a circular cutout (`R = 2.5 mm`), a mounting hole (`R = 1.5 mm`), and `Front` + `Both` keepouts, driven by `thickness_expr = Some("pcb_t")`.
   - Verifies exact `solid_volume` matches `(w*h - (4 - π)*rc² - slot_area - circle_cut_area - hole_area) * t` (`< 1e-10`, proving exact cylindrical walls and proving keepouts do not subtract solid material), `χ = -4`, world-space hole frame `[0.018, -0.0184, -0.003]`, and live parameter update (`pcb_t = 2.4 mm`) updating both exact solid volume and top-face connector origin.
4. **`board_with_concave_arc_edge_notch_exact_volume` (`:1052`):**
   - Builds a `50 × 40 mm` board with a concave 180° `R = 5 mm` semicircular edge notch (`ccw: false` on the CCW outer loop) and an interior hole, verifying exact `solid_volume` (`w*h - 0.5*π*rn² - π*hole_r²`) * `t` (`< 1e-10`), then verifies that a hole placed inside the concave notch lune (`[0.025, 0.038]`) is refused with `BoardShapeError::HoleOutsideOutline { entity_id: 23 }`.
5. **`every_invalid_role_assignment_refused_with_typed_error_on_kv2` (`:1379`):**
   - Exercises all 26 invalid classes **B1–B26** (`specs/board_shape_feature.md` §4) on `Engine` + `KernelV2Adapter`: `SketchNotFound` (missing UUID and sketch ordered after `BoardShape`), `UnsolvedSketch` (`OverConstrained`, `SolveFailed`), `InvalidSketchPlane` (zero normal, parallel `plane_x_axis`), `EmptyBoardName`, `InvalidThickness`, `UnknownEntity`, `ConstructionEntity`, `UnsupportedEntityKind` (`Point`, `Spline`, `Gear`, `Sprocket`), `HoleNotCircle` (`Line`, `Arc`), `InvalidHoleClearance`, `InvalidKeepoutHeight`, `DegenerateEntity` (`Line`, `Circle`, mismatched-radii `Arc`), `MissingOutline`, `OutlineNotClosed`, `OutlineBranching`, `MultipleOutlines`, `SelfIntersectingLoop`, `CutoutNotClosed`, `CutoutBranching`, `CutoutOutsideOutline` (disjoint, boundary-crossing, outline-enclosing), `CutoutsOverlapOrNested` (crossing, nested), `HoleOutsideOutline` (outside, tangent to outline), `HoleOverlapsCutout` (inside cutout, grazing wall), `HolesOverlap` (overlapping, tangent), `KeepoutNotClosed`, and `KeepoutBranching` — asserting each produces zero solid outputs, no `Engine::board_shapes` entry, zero `PartConnector`s, and the exact typed `ErrorKind::BoardShape` / `ErrorKind::SketchNotFound` variant.

### 3.2 Engine Unit / Incremental-Rebuild & Format Suites

- **`crates/feature-engine/tests/board_shape.rs`:**
  1. `coincident_constraints_weld_distinct_endpoint_ids_into_closed_loop` (`:48`): 4 independent lines (8 distinct `Point` IDs drawn `0.5 mm` apart) solved via `sketch_solver::solve_sketch` converge to ~0 apart and weld into a single closed `40 × 30 mm` outline, while an unsolved `4e-7 m` endpoint gap (`> TAU_MODEL`, `<= MIN_FEATURE_SIZE`) with `SketchConstraint::Coincident` is refused loudly as `BoardShapeError::OutlineNotClosed`.
  2. `incremental_rebuild_carries_board_shape_and_connectors_when_not_reexecuted` (`:222`): editing a downstream feature after `Operation::BoardShape` preserves the exact `KernelSolidHandle::raw()` without re-executing `BoardShape` while carrying forward `Engine::board_shapes` and `Engine::connectors`.
  3. `role_map_serde_rejects_non_u32_and_duplicate_keys_and_round_trips` (`:484`): verifies JSON round-trip of `Operation::BoardShape` and loud serde errors on `"not_a_number"` and duplicate `"5"` vs `"05"` role keys.
- **`crates/feature-engine/tests/opaque_operation.rs` (`:67`):** Updates `OPERATION_TAGS.len()` pin to `19`.
- **`crates/file-format/tests/format_tests.rs`:**
  1. `board_shape_round_trips_through_waffle_v15_without_version_bump` (`:2419`): verifies `save_project_verified` → `load_project` round-trip of `Operation::BoardShape` (with `Outline`, `Cutout`, `Hole`, `Keepout`, and `thickness_expr`) at `version == 15` and `min_reader_version == 15`.

---

## 4. Exact Verification Commands and Observed `^test result:` Lines

All heavy jobs were booked under `CARGO_TARGET_DIR=/home/claude/workspace/target/wt-board-shape timeout 5400 ~/.claude/bin/reserve -c 6 --here`:

1. **`cargo fmt --check -p waffle-types -p modeling-ops -p feature-engine -p file-format -p wasm-bridge -p test-harness`** — exit `0`.
2. **`cargo clippy --all-targets -p waffle-types -p modeling-ops -p feature-engine -p file-format -p wasm-bridge -p test-harness -- -D warnings`** — exit `0` (`0` warnings).
3. **`cargo test -p waffle-types` & `cargo test -p modeling-ops`** (7 targets / 322 passed; 3 targets / 57 passed):
   ```text
   Running unittests src/lib.rs (/home/claude/workspace/target/wt-board-shape/debug/deps/waffle_types-2424e7691b22d5e2)
   test result: ok. 313 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.39s
   Running unittests src/lib.rs (/home/claude/workspace/target/wt-board-shape/debug/deps/modeling_ops-08bf4410650843e0)
   test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
   ```
4. **`cargo test -p feature-engine`** (43 test targets, 798 passed, 0 failed; key binaries quoted verbatim):
   ```text
   Running unittests src/lib.rs (/home/claude/workspace/target/wt-board-shape/debug/deps/feature_engine-1651993d849af874)
   test result: ok. 325 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s
   Running tests/board_shape.rs (/home/claude/workspace/target/wt-board-shape/debug/deps/board_shape-31e1f2f4ddc59e02)
   test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
   Running tests/opaque_operation.rs (/home/claude/workspace/target/wt-board-shape/debug/deps/opaque_operation-f57e8b5f85b1e4c1)
   test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
   ```
5. **`cargo test -p file-format --features json-schema`** (9 test targets, 126 passed, 0 failed; key binaries quoted verbatim):
   ```text
   Running unittests src/lib.rs (/home/claude/workspace/target/wt-board-shape/debug/deps/file_format-d3c01072da68c44c)
   test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
   Running tests/format_tests.rs (/home/claude/workspace/target/wt-board-shape/debug/deps/format_tests-fa1e1b75f42c9bff)
   test result: ok. 60 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
   Running tests/schema_golden.rs (/home/claude/workspace/target/wt-board-shape/debug/deps/schema_golden-a490506a4911644d)
   test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.08s
   ```
6. **`cargo test -p wasm-bridge`** (45 test targets, 536 passed, 0 failed, 5 ignored; unit test binary quoted verbatim):
   ```text
   Running unittests src/lib.rs (/home/claude/workspace/target/wt-board-shape/debug/deps/wasm_bridge-4f6bae9e3014c509)
   test result: ok. 45 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s
   ```
7. **`cargo test --release -p test-harness --test board_shape_kv2`**:
   ```text
   Running tests/board_shape_kv2.rs (/home/claude/workspace/target/wt-board-shape/release/deps/board_shape_kv2-0ba3b4db77e3326f)
   test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.43s
   ```
8. **`cargo test -p test-harness --lib`**:
   ```text
   Running unittests src/lib.rs (/home/claude/workspace/target/wt-board-shape/debug/deps/test_harness-5a74b187886bbf3b)
   test result: ok. 187 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 55.33s
   ```
9. **Relay manifest byte & SHA-256 verification** (`node app/scripts/gen-agent-manifest.mjs --stdout` vs `relay/src/waffle_mcp_relay/agent-tools.manifest.json`): byte-identical, `manifest_hash = a683fed71f414e6c5da1474feb1b26b1ab049e38ba8338ffdd628f578d3bd562`.
10. **WASM bundle** (`./scripts/build-wasm.sh`): exit `0`, `app/static/pkg/.build-fingerprint` matches `./scripts/wasm-fingerprint.sh` (`708a93c39b8be55d020ca651c5266cecee55d65cedbb1236e3b4ad325cf512d0`).

---

## 5. Deliverable B3 — What the Agent Tool and UI Would Need (Not Built in This Task)

### 5.1 MCP Agent Tool Plumbing

1. **Allow `"BoardShape"` in generic feature authoring (`crates/wasm-bridge`):**
   - Add `"BoardShape"` to `const AUTHORABLE: &[&str]` in `crates/wasm-bridge/src/tools/author.rs:45` so `feature_add` and `feature_edit` accept `"operation": { "type": "BoardShape", "params": ... }` (the JSON Schema for `BoardShapeParams` is already embedded in `docs/schema/waffle-v5.schema.json` and `app/src/lib/agent/tools/engineSchemas.generated.js`).
   - Update `const operationNote` in `app/src/lib/agent/tools/authoring.js:156` to document `BoardShape` units (`thickness`, `clearance`, and `max_height` in meters; `thickness_expr` in mm-space; `roles` keyed by decimal sketch entity ID strings) and the `{Line, Arc, Circle}` exact-primitive rule.
2. **Optional dedicated `board_shape_get` read/query tool (or `feature_get` enrichment):**
   - If agents need evaluated loop areas, hole diameters/stable IDs, and keepout bounds without parsing raw sketch entities, add a read-only tool in `crates/wasm-bridge/src/tools/` reading `state.engine.board_shapes.get(&feature_id)` (`EvaluatedBoardShape`), register its name in `MIGRATED` (`crates/wasm-bridge/src/tools/mod.rs:50`) — and if any new mutating board-role helper tool is added, list it in `wasm_bridge::tools::mutates` (`crates/wasm-bridge/src/tools/mod.rs:134`) so `ModelUpdated` is emitted — plus add its schema in `app/src/lib/agent/tools/` and route it in `app/src/lib/agent/executor.js`.
3. **Update `model_summary` output description (`app/src/lib/agent/tools/model_summary.js`):**
   - Document the optional `connectors[].key` field (`"waffle:<sketch_id>/<entity_id>"`) already emitted by `crates/wasm-bridge/src/tools/summary.rs:177-179`.
4. **Regenerate and verify manifests:**
   - Run `node app/scripts/gen-agent-manifest.mjs` and `pytest relay/tests/test_manifest.py` + `cargo test -p wasm-bridge`.

### 5.2 Svelte App UI Plumbing

1. **State & dialog lifecycle in `app/src/lib/engine/store.svelte.js`:**
   - Add `boardShapeDialogState` (`sketchId`, `board`, `thickness`, `thicknessExpr`, `roles`, `editingFeatureId`) plus `showBoardShapeDialog()`, `hideBoardShapeDialog()`, and `applyBoardShape()` dispatching `UiToEngine::AddFeature` / `UiToEngine::EditFeature` with `{ type: 'BoardShape', params: ... }` (converting UI mm inputs to meters).
   - Add `'BoardShape'` case to `showEditFeatureDialog(featureId)` (`app/src/lib/engine/store.svelte.js:10719`) so double-clicking or choosing Edit on a `BoardShape` feature re-opens the dialog populated with its `BoardShapeParams`.
2. **Dialog component (`app/src/lib/ui/BoardShapeDialog.svelte`) & mounting:**
   - Create `BoardShapeDialog.svelte` with: upstream `Sketch` selector, `board` name input, `thickness` / `thickness_expr` input, active role-brush mode (`Outline`, `Cutout`, `Hole { plated, clearance }`, `Keepout { side, max_height }`, `Clear`), entity-role summary table, and inline display of typed `ErrorKind::BoardShape` messages when validation refuses an invalid assignment.
   - Mount `<BoardShapeDialog />` in `app/src/lib/viewport/Viewport.svelte` and add a **Board Shape** trigger button in `app/src/lib/ui/Toolbar.svelte`.
3. **Feature tree integration (`app/src/lib/ui/FeatureTree.svelte`):**
   - Add `BoardShape` icon entry in `FEATURE_ICONS` (`app/src/lib/ui/FeatureTree.svelte:738`), wire double-click editing in `handleDblClick` (`:561`), and enable the Edit context-menu action for `BoardShape` features.
4. **Viewport sketch-entity picking & keepout/role overlays:**
   - While `BoardShapeDialog` is open, highlight entities of the referenced `sketch_id` by assigned role color (`Outline`, `Cutout`, `Hole`, `Keepout`) and toggle/assign roles in `roles[entityId]` on viewport click (including chain-select convenience for connected `Line`/`Arc` loops).
   - Optionally render translucent `Front` / `Back` / `Both` `max_height` keepout prisms in the 3D viewport from `BoardShapeParams` / `EvaluatedBoardShape` so enclosure authors can visually check component clearance.
5. **Playwright GUI specs (`app/tests/gui/`):**
   - Author specs covering creating a `BoardShape` from a sketch, editing hole/keepout roles, verifying error feedback on unclosed outlines or spline entities, and mating onto a board hole in an Assembly tab.

---

## 6. What Was NOT Finished & Open Uncertainties

### 6.1 Deliberately Out of Scope (Not Built)

Per `AGY_BRIEF_BOARD_SHAPE.md`:
- **Agent tool authoring & Svelte UI (`B3`):** Listed in Section 5 above; `"BoardShape"` is intentionally omitted from `AUTHORABLE` in `crates/wasm-bridge/src/tools/author.rs:45` in this task.
- **`board-interface` crate, DXF/JSON reader/writer, canonical geometry hash, publishing (`interface/<board>/board-shape.{dxf,json}`), pulling, `Derived` read-only provenance, and ownership take-over (`ProvenanceOrigin` severing with arrival ID preservation; there is no lock file):** Deferred until Boardroom's `board-interface` crate lands (`specs/board_shape_feature.md` §8).
- **Full 95-target `test-harness` integration suite & Playwright GUI runs:** We ran `cargo test --release -p test-harness --test board_shape_kv2` and `cargo test -p test-harness --lib` (plus full `cargo test` on `waffle-types`, `modeling-ops`, `feature-engine`, `file-format`, and `wasm-bridge`, and `cargo clippy --all-targets` across all six crates). Per brief rule 4, no Playwright tests were run from this worktree.

### 6.2 Uncertainties / Edge Cases for Reviewer Awareness

1. **Coincident tangent / vertex-sharing cutouts or holes:** `evaluate_board_shape` enforces a strict minimum clearance of `TAU_MODEL` (`1e-7 m`) between cutouts, holes, the outer outline, and non-adjacent edges of the same loop (`board_shape.rs:640-706, :1760`; `specs/board_shape_feature.md` §4 rows B17, B20–B24) so the B-Rep solid never develops sub-model-tolerance or non-manifold walls. Edge-biting notches (e.g. card-edge slots or mouse-bites that open through the board perimeter) should be drawn as part of the `Outline` loop itself (verified in `board_with_concave_arc_edge_notch_exact_volume`).
2. **Keepout overlap policy:** `Keepout` loops are validated for closure, simplicity, and non-degeneracy (rows B11, B12, B17, B25, B26), but are intentionally allowed to extend to or across the board edge and overlap holes/cutouts or other keepouts (e.g. a `Front` height restriction overlapping a `Back` restriction or overhang keepout). When `board-interface` lands, confirm whether its DXF schema imposes any stricter containment rule on keepout layers.
3. **Downstream coplanar face-on-face boss unions:** As throughout Waffle Iron, a downstream legacy `Extrude` sketched directly on `z = thickness` and swept only `+z` (`[thickness, thickness + d]`) meets the board cap purely face-on-face and hits Yang's Stage-0 coplanar boolean wall unless extruded bidirectionally into the board or combined once Stage-0 coplanar union support lands. `BoardShape` itself avoids Stage-0 coplanar contact internally by sweeping cutout and hole cutters symmetrically over `[-thickness, +2.0 * thickness]` (`origin - thickness * normal`, `depth = 3.0 * thickness`, `board_shape.rs:879-888`).

### 6.3 Consolidation Note for the Upcoming Derive Slice

`crates/feature-engine/src/board_shape.rs` (`2,317` lines) carries its own 2D `{Line, Arc, Circle}` endpoint chaining, curved-loop containment/distance checks, and `Region` construction alongside `kicad_pcb::outline` (`crates/kicad-pcb/src/outline.rs`) / `crates/feature-engine/src/kicad.rs` (`derive_board` `:352`) and the profile/region extraction in `crates/waffle-types/src/profiles.rs` and `crates/waffle-types/src/regions.rs`. When the `board-interface` derive slice lands, it should merge those 2D chaining/containment/region routines into one shared implementation rather than adding a third copy.

---

## For the project memory

- `Sketch::recompute_derived_checked()` calls `expand_generators()`, which replaces `SketchEntity::Gear` and `SketchEntity::Sprocket` in `sketch.entities` with expanded primitive arcs/lines at synthetic IDs (`50_000_000+`). Any feature like `Operation::BoardShape` that validates authored entity kinds by entity ID must read the sketch via `rebuild::active_sketch` (which resolves projected points without expanding generators) so `Gear`/`Sprocket` entities fail with a typed `UnsupportedEntityKind` error instead of `UnknownEntity`.
- Regenerating `docs/schema/waffle-v5.schema.json` (`UPDATE_GOLDEN=1 cargo test -p file-format --features json-schema --test schema_golden`) whenever `Operation` or any type reachable from `ENGINE_ROOTS` changes also requires running `node app/scripts/gen-agent-manifest.mjs` so `app/src/lib/agent/tools/engineSchemas.generated.js` and `relay/src/waffle_mcp_relay/agent-tools.manifest.json` stay byte-synchronized (`relay/tests/test_manifest.py` enforces this in CI).
