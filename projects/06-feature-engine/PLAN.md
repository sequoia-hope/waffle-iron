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
- [x] A-M3 (2026-09-23): **query chains → `TopoQuery`** — `Query` carries
      `filters` + `tie_break`; `.faces()/.edges()`, `.surface_type(s)`,
      `.normal_near(dir, tol_deg)`, `.near_point(pt, d)`, `.area_between(a, b)`,
      `.largest_area()`, `.nearest_to(pt)`, `.farthest_along(dir)` (new
      `TieBreak::FarthestAlong`), `.first()`; `.role()` stays `Selector::Role`
      and refuses to mix with filters; an unnarrowed face/edge query is loud.
      **Named outputs** — the return value `#{ main, hub, top }` keys the node:
      `Main`, `OutputKey::Named{name}` (bodies), `Role::Named{name}` (faces /
      edges, resolved at execution); a bare feature ref is `main`; `@output
      name: main|body|face|edge|connector` is a contract (missing / wrong kind /
      unplaced connector ⇒ loud, no outputs). **Mate connectors from scripts** —
      `ctx.mate_connector(#{ name, on: face_or_edge_query | frame, x_axis,
      anchor, flip_z, rotation_deg, offset_m })` records a `MateConnector` child;
      the node exposes the evaluated frames (`Engine::script_connectors`,
      appended to `Engine::connectors` in tree order; carried across a rebuild
      that does not re-execute the node; dropped with suppression / deletion).
      **OUTER references** — `@param x: body|face|edge` takes a `GeomRef` JSON
      argument (kind-checked, unscoped, feature-anchored); a child that targets
      it consumes it on the node's behalf: `script::execute` returns a
      `ScriptOutcome { result, consumed_outer, connectors }`, the rebuild loop
      calls it directly for `Script` nodes and applies the consumption; a carried
      node re-applies its previous `consumed_by` entry (`rebuild::Carried`).
      **Found on the way:** `Selector::Query` resolved over the feature's
      provenance DIFF, so a query on a merged/cut body saw only the seam faces
      (and could name deleted ones) — `resolve::resolve_geom_ref_live` answers a
      query over the anchor body's CURRENT entities via `compute_all_signatures`
      and is now what sketch-plane, connector and script-output resolution use.
      Tests: `tests/script.rs` (+5, MockKernel), `resolve.rs` (FarthestAlong),
      `header.rs` (typed outputs), `test-harness/tests/script_kv2.rs` (+2, real
      kernel: boss-on-top via a query chain has the exact summed volume with the
      named face and both connectors at z = 0.014; a script cuts an outer body
      parameter to the exact remaining volume and consumes it).
- [x] A-M4 (2026-09-23): `script::check(text, entry)` (header + compile + entry,
      no kernel) and `script::display_name`; the header types serialize
      (`ScriptInterface` → `{name, version, params[{name, type, default?, min?,
      max?}], outputs[{name, kind}]}`) — what generates the app's Script dialog and
      the `script_run_check` answer. Bridge: `AddScriptSource` (text or the built-in
      `gear`/`sprocket` library; any text — the editor saves work in progress),
      `SetScriptSource` (replace + rebuild; Script kind only), `CheckScript` (with
      `args` ⇒ a `record` dry run), `ReadSource`; a `Script` node added through
      `AddFeature` takes the header's `@feature name`. Tools `script_run_check`,
      `script_source_add` (refuses a script that does not check), `script_source_get`,
      `script_source_update` (a node the text newly breaks ⇒ the previous text is
      re-set and verified, `rolled_back`), `script_feature_add`. App:
      `ScriptDialog.svelte` + `ScriptEditor.svelte`, toolbar "Script", double-click
      edit, Sources-panel "edit", property-editor argument rows. Docs:
      `docs/CUSTOM_FEATURE_SCRIPTS.md`. Tests: `wasm-bridge/tests/tool_script.rs`
      (15), `app/tests/gui/script-dialog.spec.js`, `agent-script-tools.spec.js`.
- [ ] Known limits: `module` is a Rhai keyword (the gear param is `module_m`);
      `ctx.log` lines surface as warnings (`log: …`); child roles concatenate (no
      `Role::ScriptChild`); `tree.clone()` + `feature_results.clone()` per script rebuild;
      a script's `.nth(i)` bodies are not separately nameable (name a child's main);
      `Named` bodies render as "Feature (n)" (no name-derived display label yet).

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
- [x] Bore with COPLANAR caps through a sprocket STOPped in yang Stage 0 (see Blockers);
      was pinned `#[ignore = "M8 …"]` in `sprocket_kv2.rs`; RESOLVED 2026-09-21
      (`specs/m8_rim_override_provenance.md`).

### M15: Pipe sweep — B2 checkpoint 2 of `specs/custom_features_and_modeling_roadmap.md` ✅ (2026-09-21)
- [x] `Kernel::pipe` (defaulted `NotSupported`; `waffle_types::kernel::PipePathSegment`),
      `KernelV2Adapter::pipe` → `kernel_v2::pipe` (ONE directly assembled solid, spec
      `specs/b2_pipe_sweep.md`), `MockKernel::pipe` (typed refusals + box topology).
- [x] `waffle_types::path::extract_open_chain`: sketch lines/arcs (construction allowed)
      → one open, oriented, G1 chain by shared point ids; typed `PathError` naming the
      entity/point (branching, disconnected, closed, non-tangent, degenerate).
- [x] `Operation::Pipe { PipeParams { sketch_id, entity_ids, radius(_expr),
      inner_radius(_expr), combine, targets } }` (5 sites in `types.rs`, rebuild arm,
      consumed ids, tree-position dependence, `params.rs` length expressions);
      `modeling_ops::execute_pipe` (roles: `EndCapNegative`/`EndCapPositive` by end
      tangents, `SideFace{i}`); script `ctx.pipe(sketch, [ids], #{ radius, inner_radius,
      combine, targets })`; `AUTHORABLE` + dispatch name; `docs/FILE_FORMAT.md` §7.11 (no
      reader-floor bump); golden schema + agent manifest regenerated; `ModelBuilder::pipe`.
- [x] Tests: `tests/pipe.rs` (6, MockKernel), `waffle-types` path (2),
      `test-harness/tests/pipe_kv2.rs` (4, real kernel: exact `π r² L` / `π (r² − rᵢ²) L`
      to 1e-9, χ = 2 / 0, box cut through the lead-in, loud malformed path).
- [x] Checkpoint 3 (same day): `PipeDialog.svelte` (path pick box — a viewport click on
      an inactive-sketch line/arc brings its whole connected chain, `setPipePath` test
      API; radius + wall inputs in the display unit with expressions; combine/targets),
      toolbar "Pipe", feature list double-click / context edit, property editor fields,
      `showEditFeatureDialog` routing; GUI spec `pipe-dialog.spec.js` (5, gui-fast).
- [ ] Known walls (typed): closed loops, mitred (non-G1) joints, non-planar chains; a bend
      over ≈149° that SURVIVES a boolean (the recovered seam must be one sub-π arc).

### M16: Union all — B4 of `specs/custom_features_and_modeling_roadmap.md` ✅ (2026-09-23)
- [x] `Operation::UnionAll { UnionAllParams { targets: All | Selected { bodies } } }`
      (`specs/b4_balanced_union.md`): 5 sites in `types.rs`, rebuild arm, consumed ids,
      name inheritance via the new `Engine::consumed_by` (consumer → consumed, in order),
      `migrate.rs`, harness name tables, `AUTHORABLE` + dispatch name, FILE_FORMAT §7.12,
      golden schema + agent manifest regenerated.
- [x] `crate::union_all`: `All` = every live solid output before the feature (tree order);
      `Selected` honors `ResolvePolicy` on consumed bodies, refuses duplicates and self;
      balanced fold (`union_balanced` / `fold_into`) gated by the new
      `KernelIntrospect::solid_aabb` (MockKernel: vertex hull; kernel-v2:
      `introspect::conservative_aabb` — curve/sphere/torus bulge + cylinder/cone slab).
      The pattern's `fold_union` reuses `fold_into` (same order, gate added).
- [x] `crate::progress`: thread-local sink; `UnionAll` reports one frame per union RUN.
      `wasm_api::set_progress_sink` → worker bare `Progress` frames → `bridge.on('progress')`
      → store status bar + `subscribeRebuildProgress` → `link.js` `progress` frames →
      relay `on_progress` → MCP `notifications/progress` (only with a `progressToken`).
- [x] P10: `BooleanCombine` refuses a consumed operand (either policy); explicit combine
      targets apply the pattern's rule (Strict error / BestEffort drop + warning);
      `ModelUpdated.consumed_features`; the Boolean dialog lists live bodies only and
      gained "Union all bodies"; script `ctx.union_all()`.
- [x] Tests: `tests/union_all.rs` (8, MockKernel), `progress` unit test,
      `test-harness/tests/union_all_kv2.rs` (4, real kernel: 5-box chain exact
      inclusion–exclusion volume to 1e-9, χ = 2, far cluster skipped by the gate, tree ≡
      chain, bit-identical rebuilds, consumed operand loud), relay
      `test_progress_frames_reach_the_calls_consumer_while_in_flight`, GUI
      `boolean-two-body.spec.js` "union all bodies". Six `engine_tests` boolean fixtures
      moved to a NewBody second extrude (they re-targeted a consumed body).
- [ ] Follow-ups: spatially sorted body order for a better tree (order is tree order today);
      a `UnionAll` edit dialog for `Selected` (authoring is `feature_add`/script only);
      progress frames for other long features (chained pattern folds).

## Blockers

- ~~**M8 Stage-0 mixed-loop coplanar caps (found 2026-09-19 boring a 20T
  sprocket with a through-cut whose caps are coplanar with the sprocket's):**
  `face N: holed lateral CDT failed: duplicate (coincident) loop vertex`.~~
  **RESOLVED 2026-09-21** (spec `specs/m8_rim_override_provenance.md`): the
  measured mechanism was three f64 spellings of ONE geometric split point —
  each cap's own overlay emission (independent frames) plus each one's f64
  mirror onto the other cap's rim — kept apart by the bit-exact dedup
  (probe `[mixed-cross]`: 13 vs 12 overrides on the flank pair, strip
  chains 15 vs 16; the bore's own full-circle rims 121 vs 124 by the same
  latent). Fix: `RimSplitMap` carries PROVENANCE — a cap's own sample
  replaces a near-twin (`TAU_WORK`) mirror in place and a mirror is absorbed
  by a near own sample; own-vs-own and mirror-vs-mirror stay bit-exact.
  `sprocket_bore_with_coplanar_caps` un-quarantined (mesh volume, χ = 0,
  watertight). Not done: the two overlays are still frame-independent by
  design (bit-consistency was never the contract; the rim carrying each
  cap's own bits is).

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
