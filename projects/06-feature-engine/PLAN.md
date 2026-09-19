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

### M14: Sprocket sketch entity — B3 of `specs/custom_features_and_modeling_roadmap.md` ✅ (2026-09-19)
- [x] `SketchEntity::Sprocket { params: SprocketParams }` + `waffle_types::sprocket`:
      ISO 606 tooth gap form (pitch `d = p/sin(π/z)`, seating arc `ri`, tangent convex
      flank arcs `re`, tip arc at `da`), mid-range defaults with `seatingRadius` /
      `flankRadius` / `tipDiameter` / `seatingAngleDeg` overrides; points + arcs only,
      finished through `build_finish_profiles` (exact `arc_segments`). Every refusal is a
      typed `SprocketError` naming the value (too few teeth, bad value, seat < roller,
      flank never reaching the tip, flanks crossing — with the largest tip diameter that
      still leaves a tip arc, by bisection). `SprocketStandard::Iso606` only — ANSI B29.1
      needs the standard's text (not in `refs/`), left as an enum extension.
- [x] Expansion: `Sketch::expand_generators` (gears + sprockets; `expand_gears` kept as
      the gear-only alias) offsets a sprocket's primitives into `generated_entity_id_base`
      (the same range the bridge / app use), so plain entities drawn beside it keep their
      ids and loops (`recompute_derived_checked` extracts the plain loops from a
      pre-expansion snapshot). Extrude/Revolve report a sprocket that cannot expand as
      `EngineError::SketchGenerator` → `ErrorKind::InvalidParameter`.
- [x] Bridge: `GenerateSprocketPreview` / `GenerateSprocketProfile` (stateless, the gear
      pair's shape + `dimensions`); `sketch_regions` / `sketch_create` expand sprockets
      like gears; `sketch_create` refuses bad params at `/entities/i/params`.
- [x] Script API: `sk.sprocket(#{ tooth_count, pitch, roller_diameter, … })`.
- [x] App: `createSprocket` (store, `__waffle.createSprocket`), display through the gear
      registry/display maps (`kind: 'Sprocket'`), inactive-sketch rendering and region
      computation; no Sprocket dialog / toolbar tool yet (checkpoint 2), double-click on
      a sprocket is absorbed rather than opening the gear dialog.
- [x] Oracles: `waffle-types` (roller clearance + pitch/tooth round-trip over 9 chains ×
      9 tooth counts, tangency, rigidity, typed failures), `feature-engine/tests/sprocket.rs`
      (7, MockKernel), `test-harness/tests/sprocket_kv2.rs` (real kernel: 4z+2 faces, exact
      volume = analytic arc area × depth to 1e-9, watertight χ=2 for 9/20/52 T; bore cut via
      the general boolean path; script route = entity route), GUI
      `sketch-sprocket-entity.spec.js` (gui-fast).
- [x] Found on the way (kernel-v2 `exact2d`): the Tier-2 arc validator lifted each arc's
      f64 centre/radius verbatim, so two arcs meeting TRANSVERSALLY at a shared vertex
      (a sprocket flank into its tip arc) crossed exactly a few ulps off the corner and
      inside both open arcs ~half the time — a valid loop rejected as non-simple by
      rounding luck (and the exact extrude silently fell back to the 16-facet chord
      polygon: 578 faces for a 9T sprocket). Every arc predicate now lifts to the rational
      circle through BOTH endpoints (centre snapped onto the chord bisector); fixtures with
      a centre already on the bisector are unchanged (regression test
      `transversal_corner_is_not_a_crossing`).
- [x] Checkpoint 2 (2026-09-19): `SprocketDialog.svelte` (teeth, chain preset — ISO
      06B…16B, bicycle 1/2″×7.75, ANSI 25…60 — pitch, roller Ø, derived pitch Ø; live
      preview via `GenerateSprocketPreview`; the engine's typed refusal is shown in
      the dialog and disables Apply), `sprocket` placement tool (toolbar "Sprkt",
      shortcut K, hover preview, click reuses a point as centre), double-click on a
      sprocket opens the dialog in edit mode (`updateGear` keeps `kind`). GUI spec
      `sketch-sprocket-dialog.spec.js` (8, gui-fast).
- [x] A-M5 (2026-09-19): `scripts/sprocket.rhai` — line-for-line port of
      `generate_sprocket_profile` (dimensions, gap template, placement, 4 arcs per gap);
      `tests/script_sprocket_parity.rs` pins entities + positions BIT-IDENTICAL over a
      41-case matrix (9 tooth counts × 4 chains + placement + overrides; refusals must
      match the generator's too) plus the `sk.sprocket` entity route (id-offset range,
      profile-for-profile). `test-harness/tests/script_kv2.rs` builds both routes on
      kernel-v2: equal exact volumes, 4z+2 faces. Needed `cbrt` registered in the
      Rhai engine (no cube root in Rhai's math package).
- [ ] Bore with COPLANAR caps through a sprocket STOPs in yang Stage 0 (see Blockers);
      pinned `#[ignore = "M8 …"]` in `sprocket_kv2.rs`.

## Blockers

- **M8 Stage-0 mixed-loop coplanar caps (found 2026-09-19 boring a 20T
  sprocket with a through-cut whose caps are coplanar with the sprocket's):**
  `face N: holed lateral CDT failed: duplicate (coincident) loop vertex`.
  Both sprocket caps pair with the tool's caps; `collect_mixed_crossings`
  (`yang-rs/src/stage0/rim_chords.rs`) inserts each cap's overlay split
  points into its arc's chain AND mirrors them by an f64 axial projection
  onto the opposite arc of the shared partial strip. The two caps' overlays
  run in independent frames, so the mirrored points are ULP-twins of the
  points the opposite arc already carries from its own overlay; the
  bit-exact `contains` dedup keeps both (12 vs 13 overrides on one flank
  pair), the two chains come out 15 vs 16 long, the strip cannot pair, the
  face is routed to the chart CDT, and the twins collapse to exactly equal
  `(u, v)` there. The disc path avoided this with the exact opposite-rim
  projection + intra-opposite plane canonicalization
  (`specs/m8_exact_opposite_rim_projection.md`,
  `specs/m8_intra_opposite_plane_canonicalization.md`); the mixed-arc path
  needs the same bit-consistency (or: skip the mirror when the opposite cap
  is itself in a pair). The non-coplanar bore goes through the general
  pipeline and is correct. Repro: `sprocket_bore_with_coplanar_caps`
  (`test-harness/tests/sprocket_kv2.rs`, ignored).

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
