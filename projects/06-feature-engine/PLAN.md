# 06 — Feature Engine: Plan

## Milestones

### M1: Feature Tree Data Structure ✅
- [x] `FeatureTree` struct (ordered Vec<Feature> + active_index)
- [x] Add feature (append + insert at position)
- [x] Remove feature
- [x] Reorder features (move up/down)
- [x] Suppress/unsuppress feature
- [x] Set active_index (rollback)
- [x] Unit tests for all mutations (7 tree tests)

### M2: GeomRef + Anchor + Selector Types ✅
- [x] Implement GeomRef, Anchor, Selector, ResolvePolicy (from INTERFACES.md) — in waffle-types crate
- [x] GeomRef constructors for common cases (role-based, signature-based)
- [x] Serde serialization tests (round-trip) — covered in wasm-bridge tests

### M3: GeomRef Resolver — Role-Based ✅
- [x] Implement role-based resolution: anchor → OpResult → role_assignments → KernelId
- [x] Test: extrude produces EndCapPositive/Negative roles → resolve correctly
- [x] Test: fillet produces FilletFace roles → resolve correctly (via pipeline test)
- [x] Test: role not found → returns error

### M4: GeomRef Resolver — Signature-Based Fallback ✅
- [x] Implement signature similarity scoring (weighted fields: kind, area, normal, centroid, surface_type, adjacency_hash)
- [x] Implement signature matching: compute current signatures → find best match
- [ ] Test: after topology change, role fails → signature match succeeds (needs more complex test scenario)
- [x] Test: ambiguous signatures → BestEffort returns closest + warning
- [x] Test: no match → Strict returns error

### M5: Rebuild Algorithm ✅
- [x] Identify earliest dirty feature
- [x] Replay features from dirty point forward
- [x] Resolve GeomRefs before each operation (resolve_with_fallback in rebuild loop)
- [x] Store OpResult per feature
- [x] Handle resolve failures (Strict vs BestEffort)
- [ ] Trigger tessellation after rebuild (deferred: no UI consumer yet)
- [x] Test: change sketch dimension → verify rebuild produces correct geometry
- [x] Test: rebuild error on missing sketch reference

### M6: Undo/Redo ✅
- [x] Command pattern: AddFeature, RemoveFeature, EditFeature, ReorderFeature, SuppressFeature
- [x] Each command stores inverse (apply_inverse / apply_forward)
- [x] Undo stack + redo stack (UndoStack in src/undo.rs)
- [x] Redo stack cleared on new command
- [x] Rebuild after undo/redo
- [x] Test: add → undo → redo → verify state (8 undo/redo tests)

### M7: Rollback ✅
- [x] Set active_index → suppress features after index
- [x] Model state reflects the partial tree
- [ ] Slider UI integration (via EngineToUi messages — deferred to UI phase)
- [x] Test: set index → verify correct features are active (3 rollback tests)
- [x] Fix: active_features() panic on empty tree with rollback
- [x] Fix: rebuild clears results for features beyond rollback window

### M8: Integration Test — Full Pipeline ✅
- [x] Sketch → extrude → sketch → extrude → boolean union → verify all results
- [x] Edit early feature → verify downstream rebuild succeeds with no errors
- [x] Undo/redo edit → verify state roundtrips correctly
- [x] Rollback mid-tree → verify inactive features lose results, restore recovers
- [x] Fillet pipeline: sketch → extrude → fillet → verify FilletFace roles
- [x] Chamfer pipeline: sketch → extrude → chamfer → verify ChamferFace roles
- [x] Shell pipeline: sketch → extrude → shell → verify ShellInnerFace roles
- [x] Fillet survives extrude edit + downstream rebuild
- [x] Extrude provenance includes SideFace roles for edge resolution

### M9: Persistent Naming Stress Tests ✅
- [x] Add feature in middle of tree → verify downstream refs survive
- [x] Remove feature from middle → verify error on dependent features
- [x] Suppress feature → verify downstream errored; unsuppress recovers
- [x] Reorder features → verify UUID-based refs survive position changes
- [x] Reorder extrude before its sketch → verify failure
- [x] Multiple undo/redo cycle (3 adds, undo all, redo all)
- [ ] Change sketch that adds/removes edges → verify role fallback to signature (deferred: needs richer sketch editing)

### M10: Performance Benchmarks ✅
- [x] Rebuild time for 10-feature tree: ~180µs (with MockKernel)
- [x] Rebuild time for 20-feature tree: ~370µs
- [x] Rebuild time for 50-feature tree: ~1.3ms
- [x] All well under interactive thresholds; no hotspots at this scale

### M11: Parameterized Designs (design variables) ✅ (2026-08-31)
- [x] `DesignParameter` table on `FeatureTree` (name/expression/cached value/error)
- [x] Expression evaluator `expr.rs` (mm-space; unit suffixes; degrees trig; loud typed errors)
- [x] Apply pass `params.rs` runs at rebuild start: drives sketch dims (with re-solve via
      sketch-solver + `recompute_derived`), extrude depth, revolve angle, datum offsets
- [x] `Engine::set_parameters` + `Command::SetParameters` undo/redo
- [x] Bridge `SetParameters` / `EvaluateExpression` messages; Variables panel, expression
      dimension inputs, and dialog expression fields in the app
- [x] `reference` (driven-dim) flag now persists on dimension constraints (was JS-only, lost on save)
- Spec: `specs/parameterized_designs.md`; GUI spec `app/tests/gui/parameterized-designs.spec.js`

### M12: Circular and linear patterns — B1 of `specs/custom_features_and_modeling_roadmap.md` ✅ (2026-09-18)
- [x] `Kernel::transform_body` (rigid copy; kernel-v2 arena deep copy, exact on every
      surface/curve variant; reflection/scale refused; journal `OpTag::Transform`) —
      `crates/kernel-v2/src/transform.rs`, oracles `tests/transform_rigid_copy.rs`
- [x] `RigidPlacement::{translation, rotation_about, rotation_matrix, after}` in waffle-types
- [x] `modeling_ops::execute_pattern_instances` (seed re-emitted as instance 0, copies
      created, `Role::PatternInstance { index }` on every instance's faces)
- [x] `Operation::PatternCircular` / `PatternLinear` + `AxisRef { explicit | entity }`
      (`src/pattern.rs`): placements, explicit-only targets, custody of seeds, Add fold /
      Cut per piece / Intersect vs union-of-instances, sibling carry, expression fields
- [x] P10 guards: consumed seed/target refused (Strict) or dropped (BestEffort);
      seed==target refused; count/angle/spacing/axis validation; 10 000-instance budget
- [x] MCP: `feature_add` authors both kinds; schema golden + agent manifest regenerated;
      `docs/FILE_FORMAT.md` §7.9
- [x] Tests: `tests/pattern.rs` (13, MockKernel), `test-harness/tests/pattern_kv2.rs`
      (6, real kernel: N×seed volume, Add/Cut inclusion–exclusion exact to 1e-9 with
      χ = 2, grid, entity axis from a cylindrical face, determinism)
- [ ] App: no Pattern dialog / toolbar button yet (feature list icon, property editor
      fields and Boolean-dialog body filter are in). Authoring is agent-link first.
- [ ] Faces of instances that MERGE into a target lose `PatternInstance` roles (a
      boolean's outputs carry fresh ids); NewBody instances keep them.
- Note: torus/sphere render samplers derive their (u,v) grid from a world frame, so a
  rotated copy tessellates differently (same exact surface). Not a defect; noted in
  `transform_rigid_copy.rs`.

### M13: Custom feature scripts — A-M0 → A-M2 of `specs/custom_features_and_modeling_roadmap.md` ✅ (2026-09-19)
- [x] A-M0: `Operation::Script { source_id, entry, args, arg_exprs, arg_values }`;
      file-format `SourceKind::Script` (embedded text; no reader-floor bump);
      `EngineError::Script { stage, reason }` / `ErrorKind::Script` — `docs/FILE_FORMAT.md` §7.10
- [x] A-M1: Rhai interpreter (`src/script/`: `header.rs` `@feature`/`@param`/`@output`,
      `host.rs` recording host, `interp.rs` sandbox + API, `mod.rs` execution). Built with
      `rhai` default-features off (`std`, `no_time`): no clock, no runtime RNG, `eval`
      disabled; limits on operations / call depth / array / map / string; geometry budget
      2000 child ops. API: `ctx.sketch(plane)` → `point/line/circle/arc/spline/gear/
      polygon/rect/finish`, `regions()`, `ctx.extrude/revolve/boolean/log/fail/param`,
      `created_by/nth/role/side_face/faces/plane_of`, `mm()/inch()/plane()`.
      Model: API calls RECORD children (private sub-tree, sketches derived in Rust so
      the script sees regions); children then execute through the ordinary executor
      with the outer results + earlier children. Node outputs = unconsumed child bodies.
- [x] A-M2: `scripts/gear.rhai` — line-for-line port of `generate_gear_profile`;
      `tests/script_gear_parity.rs` pins entities + positions BIT-IDENTICAL over a
      56-case matrix (plus the `sk.gear` entity route incl. internal gears matching
      `expand_gears` profile-for-profile). Built-in `Gear` entity is kept.
- [x] Found on the way: the Rust port of `extractProfiles` omitted SPLINES from the
      edge graph (the JS includes them) — fixed in `waffle-types/src/profiles.rs`.
- [x] Tests: `tests/script.rs` (8, MockKernel — every failure class typed with no
      output, limits, args, expressions, determinism, undo), parity (3),
      `test-harness/tests/script_kv2.rs` (3, real kernel — exact box volume, gear both
      routes watertight χ=2 within 0.5 %, script body as a boolean operand).
- [ ] A-M3: query chains → `TopoQuery`, named outputs (`@output`), mate connectors,
      OUTER references (needs post-execution consumption reporting to the loop).
- [ ] A-M4: script editor panel, `@param` dialog generation, MCP `script_source_add` /
      `script_feature_add` / `script_run_check` (an agent can `feature_add` a Script
      today only if the document already carries the source).
- [ ] Known limits: `module` is a Rhai keyword (the gear param is `module_m`);
      `ctx.log` lines surface as warnings (`log: …`); child roles concatenate (no
      `Role::ScriptChild`); `tree.clone()` + `feature_results.clone()` per script rebuild.

## Blockers

- ~~Depends on kernel (Kernel + KernelIntrospect traits, especially MockKernel)~~ Resolved [SUPERSEDED by clean-sheet kernel]
- ~~Depends on modeling-ops (OpResult production with provenance)~~ Resolved
- ~~Can start M1-M4 with mock OpResults before modeling-ops is ready~~ Resolved (all milestones complete)
- Fillet, chamfer, and shell operations: MockKernel tests pass but WaffleKernel implementation is deferred indefinitely (see root CLAUDE.md)
- **DEFECT (found 2026-09-17 building a planetary gearbox over the agent
  link, `docs/notes/planetary_gearbox/`): an explicit `Strict` combine target
  that names an output ALREADY CONSUMED by an earlier combine is neither
  refused nor warned about.** `resolve_combine_targets` (`rebuild.rs`)
  documents `Strict ⇒ loud ResolutionFailed`, but `find_solid_handle` still
  finds the consumed feature's stale handle in `feature_results`, so the
  boolean runs against it and DUPLICATES the consumed body: plate → pin 1
  `Add` (targets plate; consumes it) → pin 2 `Add` (targets plate again)
  yields two bodies of 5127.1 mm³ each (plate 5026.5 + one pin), no
  warning, no error (`repro_consumed_target.mjs` in that notes folder).
  Expected per P10 and the function's own doc: `ResolutionFailed` for
  `Strict`, a warning and a dropped target for `BestEffort`. Fix: consult
  `already_consumed` in the `Explicit` arm (as `MostRecentLegacy` does) and
  add a test that chains two explicit combines onto one original output.

## Interface Change Requests

(None yet)

## Notes

- This is the hardest sub-project. GeomRef resolution is the core algorithm.
- Start with MockKernel. Do not wait for WaffleKernel.
- The rebuild algorithm must be correct before it's fast. Optimize later.
- Persistent naming is a simplified version of commercial approaches. Document limitations honestly.
