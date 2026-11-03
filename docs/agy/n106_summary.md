# N106 Groundwork Summary (`agy/n106-dt-bound`)

**Worktree:** `/home/claude/wt/agy-n106`  
**Branch:** `agy/n106-dt-bound` (base commit `a1932058`)  
**Paper references:** Yang et al. 2025 §4.1.2 (`refs/text/yang2025_hybrid_boolean.txt:357-390`) and §4.4.1 (`refs/text/yang2025_hybrid_boolean.txt:546-572`)  
**Production impact:** Byte-identical output. Neither `docs/yang_deviations.md` nor `docs/audits/yang_2025_audit.md` was modified on this branch.

---

## 1. Deliverable D1 — Consumer & Gate Inventory (`docs/agy/n106_dt_consumers.md`)

`docs/agy/n106_dt_consumers.md` inventories every Stage-4 and Stage-5 site in `crates/yang-rs/src` (plus cross-stage helpers invoked alongside them) that reads `d_ε`, `d(T)`, or derived chord/sagitta quantities, organized into 10 budget/aggregator helpers (Section 2) and 37 categorized decision rows (`A1`–`A9`, `B1`–`B6`, `C1`–`C13`, `D1a`–`D1d`, `E1`–`E5`, plus Category F contrast checks on `TAU_MODEL` / `TAU_WORK` / `TAU_EVAL`):

- **Category A (`A1`–`A9`) — Live N106 `d(T)` / `ChordDegradation` / `ChordBudget` gates (should become `d_ε(T)` producers, not rejection gates, under §4.1.2 `:375-382` and §4.4.1 `:568-571`):**
  - `A1` / `A2`: `stage4_construct.rs:624-1030` (`rebuild_patch_planar_bounded_with_metric`, wrapped by `rebuild_patch_planar` `:475`, `rebuild_patch_planar_bounded` `:492`, and `rebuild_patch_planar_bounded_ex` `:505`) — `old_max` clamped by `d_eps` (`:808-811`), `new_max > budget -> ConstructError::ChordDegradation` (`:945-951`), and the `I2e` interior Steiner grid (`:980-1030`).
  - `A3`: `stage4_construct.rs:3269, 3363-3368` (`update_census_with_side_labels`, `YANG_441_UPDATE_CENSUS`).
  - `A4`: `stage4_construct.rs:4216, 4316, 4396, 4443-4475` (`update_patches_ex_with_bounds` + `local_fig11a_patch_plans` `:3911`).
  - `A5`: `stage5_topology/construct.rs:812, 819-830` (`run_construct_passes` passing `stage4_chord_band(a, b)` into `rebuild_patch_planar_bounded`).
  - `A6`: `stage4_chain_decimate.rs:719, 1192, 1413` (`rebuild_decimated_patch_fans` calling `refill_fan_hole_seeded_with_bounds`, plus `decimate_patch_chains_441_with_bounds` falling back to `rebuild_patch_planar_bounded_ex` with `d_eps = stage4_chord_band(a, b)`).
  - `A7`: `stage4_construct.rs:1928-2262` (`refill_fan_hole`, `refill_fan_hole_with_bounds`, `refill_fan_hole_seeded`, `refill_fan_hole_seeded_with_bounds`) — **live in production** at `stage5_topology/fold_merge.rs:2147` (`rehome_attempt`), `stage4_chain_decimate.rs:719` (`rebuild_decimated_patch_fans`), and `stage4_correct/corner_transit.rs:1393, 1531, 1793` (`corner_transit_apply`), enforcing `new_max > budget -> ConstructError::ChordDegradation` (`:2229-2236`).
  - `A8`: `stage4_correct/corner_transit.rs:1467-1484` (`corner_transit_apply` non-far wholesale `rebuild_patch_planar`, falling back on `ConstructError::ChordDegradation` at `:1474` to `refill_fan_hole_seeded_with_bounds` at `:1531`).
  - `A9`: `stage4_boundary_curve.rs:3895-3899, 4625-4698, 4915-4948, 5204-5235` (`transit_emission_fill` + `transit_emission_refine` + `transit_emission_write`, called in production at `stage4_correct.rs:3241`), where `transit_emission_write` (`:5230-5235`) refuses with `Err(EmissionWriteFailure::CertificateFailed { what: "chord" })` unless `(c.old_max, c.new_max)` is `(Some(old), Some(new))` with `new <= old`.
- **Category B (`B1`–`B6`) — `< 2d_ε` proximity consumers & §4.5.2 demand ratios:** `shell_contact.rs:994-1033, 1205-1269` (`TriSet::from_parts`, `shell_contact_census`, `pieces_may_touch`, `contact_pairs`), `stage4_small_loop.rs:327-391` (`discover_small_loop_candidate_areas`), `stage4_tangency.rs:1887-1960` (`discover_operand_pair_tangent_points_newton`), `stage4_correct/domain_census.rs:838-880` (`under_resolution_ratio`), `stage4_phantom.rs:600-667` (`certify_phantom_loops`, reading `chord_tol_for_curved_owner` at `:653`), and `boolean/failures.rs:775-785` (`density_for_demand`).
- **Categories C, D, E (`C1`–`C13`, `D1a`–`D1d`, `E1`–`E5`):** Pre-F3 3D vertex-relocation corridor gates (`stage4_correct.rs`, `stage4_relocate.rs`, `stage4_boundary_curve.rs`), substitute topology/reversal/transit-sampling bands (`collapse.rs`, `reversal.rs`, `corner_transit.rs`, `stage4_corridor.rs`), and Stage-5/6/pre-Stage-1 auxiliary chord helpers (`stage5_output_refine.rs`, `stage5_envelope.rs`, `boolean.rs`, `boolean/rim_junction.rs`), with explicit trade-offs and proposed governance wording in Sections 4–5.

---

## 2. Deliverable D2 — Gated-Off Per-Triangle Error Bound (`TriangleErrorBoundMap`)

### 2.1 Data Structure & Uniform Bound Rule

Defined `pub(crate) struct TriangleErrorBoundMap { pub(crate) bounds: Vec<f64> }` in `crates/yang-rs/src/brep.rs:596-771`, carried in hard 1:1 lockstep with `mesh.tris`, `TriangleAttributionMap`, and `TriangleSideLabelMap`:

- **Release-enforced lockstep (`D2.1`):** `TriangleErrorBoundMap::assert_lockstep` (`brep.rs:630-637`) and `TriangleErrorBoundMap::lookup` (`brep.rs:621-627`) use unconditional `assert_eq!` / `assert!` (active in both debug and release builds), and mutator writeback loops in `repair_fan_cluster_ex_with_bounds` (`collapse.rs:2104, 2126, 2280`), `apply_rebuild_batch` (`stage4_construct.rs:2945, 2962, 2982`), and `transit_emission_write` (`stage4_boundary_curve.rs:5272-5280, 5330, 5352`) iterate by triangle index over `mesh.tris` with explicit `assert_eq!` checks rather than truncating via `.zip(...)`.
- **`f64::NAN` on unknown / uncertified bounds (`D2.2`):** Whenever face attribution or the `BRepFace` is missing, or `stage4_dt::d_of_t` returns `Err(_)`, `recompute_tri_bound_from_world` (`brep.rs:762-771`), `TriangleErrorBoundMap::from_stage2_kept` (`brep.rs:675-688`), `rebuild_merge_fan` (`stage4_construct.rs:1460-1463`), `rebuild_run_fan` (`stage4_construct.rs:2511-2514`), `rebuild_group_fan` (`stage4_construct.rs:2839-2842`), `split_boundary_edge` (`stage4_construct.rs:1720-1827`), `repair_fan_cluster_ex_with_bounds` (`collapse.rs:1180-2282`), and `transit_emission_fill` (`stage4_boundary_curve.rs:4661, 4678`) store `f64::NAN` — never `0.0` (which is reserved strictly for certified exact planar triangles where `d_of_t == Ok(0.0)`).
- **Uniform Stage-1 vs Stage-4/5 bound rule (`D2.3`, F2 owner decision):**
  1. **Stage-2 seeding (`TriangleErrorBoundMap::from_stage2_kept`, `brep.rs:650-735`, called in `boolean.rs:3095`):**
     - **Unsplit Stage-1 inner triangle** (`unsplit_raw = Some(raw_t)` via `sorted_tri_bits(pts) == sorted_tri_bits(rpts)` when `!stage0_active && !la.source.is_empty()`, and `operand.is_stage1_inner_tri(raw_t)`): stores `face_error_budget(operand, face as usize).unwrap_or(operand.tri_dt[raw_t])` (`d_ε`).
     - **Unsplit Stage-1 boundary triangle** (`unsplit_raw = Some(raw_t)` and `!operand.is_stage1_inner_tri(raw_t)`): stores `operand.tri_dt[raw_t]` (`d(T)` from Stage 1).
     - **Split Stage-2 arrangement sub-triangle (or Stage-0 / lineage-less triangle)** (`unsplit_raw.is_none()`): recomputes `recompute_tri_bound_from_world(brep_face.surface, pts)` (`stage4_dt::d_of_t` on its own 3D vertices projected onto `SurfaceChart`, `0.0` on `Plane`, `f64::NAN` if unchartable or `d_of_t` fails).
  2. **Stage-4 and Stage-5 mutators:** **Every** triangle created in Stage 4 or Stage 5 — boundary or inner, including carried-interior triangles, `I2e`-seeded triangles, fan-refill triangles, transit-fill triangles, and edge-split triangles — computes its own `stage4_dt::d_of_t` (`0.0` on exact planes; `f64::NAN` on failure). `face_error_budget` (`d_ε`) is never assigned to a Stage-4/5-created triangle.

### 2.2 Pipeline Threading

`tri_bounds: &mut TriangleErrorBoundMap` is threaded from `boolean.rs:3095` through:
- **Stage 4 (`crates/yang-rs/src/stage4_correct.rs`):** `stage4_relocate_and_correct` (`:978`) / `stage4_relocate_and_correct_ex` (`:1004`), `stage4_relocate_and_correct_inner` (`:1225`, calling `transit_emission_write` at `:3241`, `weld_coincident_relocated_ex_with_bounds`, and `retriangulate_collapsed_fan_regions_ex_with_bounds`), `sweep_reversed_intersections` (`reversal.rs:18`), `collapse_subresolution_intersection_segments_with_bounds` (`collapse.rs:2397`), `collapse_subtauwork_mesh_edges_with_bounds` (`collapse.rs:2574`), `trim_beyond_corner_phantoms_ex_with_bounds` (`collapse.rs:598`), `update_patches_ex_with_bounds` (`stage4_construct.rs:4216`), `remove_doubled_membranes_scoped_with_bounds` (`stage4_relocate.rs:2192`), `split_pinch_vertices_with_bounds` (`stage4_relocate.rs:2497`), and `corner_transit_apply` (`corner_transit.rs:5`).
- **Stage 5 (`crates/yang-rs/src/stage5_topology.rs`):** `reconstruct_topology_stage4` (`:536`), `run_construct_passes` (`construct.rs:6`, covering `rebuild_patch_planar_bounded`, `rebuild_merge_fan`, `rebuild_run_fan`, `rebuild_group_fan`, and `apply_rebuild_batch` at `stage4_construct.rs:2884`), `run_fold_merge_passes` (`fold_merge.rs:52`, covering `refill_fan_hole_with_bounds`, `split_boundary_edge`, `collapse_vertex_scoped_with_bounds`, `remove_doubled_membranes_scoped_with_bounds`, and `split_pinch_vertices_scoped_with_bounds`), `decimate_patch_chains_441_with_bounds` (`stage4_chain_decimate.rs:789`), and `compact_unreferenced_verts_with_bounds` (`collapse.rs:2706`).

### 2.3 D2 Unit Test Pins (`crates/yang-rs/src/tests_unit/n106_dt_bound.rs`)

9 unit tests in `n106_dt_bound.rs` (4 value/survival pins + 5 release-active `#[should_panic]` lockstep pins):
1. `n106_d2_pin_a_patch_cdt_all_new_tris_carry_recomputed_d_of_t` (`:63`):
   - **(1) Cylinder patch (`r = 10.0`, `θ ∈ [0, 0.8]`, `z ∈ [0, 6.0]`) with 8 boundary + 4 interior vertices:** asserts `n_boundary == 12` and `n_inner == 2` (`:160-161`), and asserts every emitted triangle (both boundary and inner) has `bound > 0.0` and `bound.to_bits() == exact_dt.to_bits()` (`stage4_dt::d_of_t`), distinct from `chord_band`.
   - **(2) I2e-seeded notched cylinder patch (`r = 2.0`, `θ ∈ [0, π/2]`, `z ∈ [0, 3.0]`):** triggers `I2e` Steiner insertion (`!seeded_rebuild.new_verts.is_empty()` and `n_seed_inner > 0`), asserting both boundary triangles and pure-seed interior triangles carry bit-exact `d_of_t` (never `gate_d_eps`).
   - **(3) Seeded `refill_fan_hole_seeded_with_bounds` on `r = 10.0` cylinder 6-gon (`seed_budget = 6`):** triggers centroid seed insertion (`!fan_seeds.is_empty()`), asserting every emitted fan triangle carries bit-exact `d_of_t`.
   - **(4) Cone patch (`half_angle = π/6`, `z ∈ [2.0, 5.0]`, `θ ∈ [0.0, 0.6]`):** asserts every emitted cone triangle carries positive bit-exact `d_of_t`.
2. `n106_d2_pin_b_bounds_survive_stage4_mutators_and_boolean_pipeline` (`:371`): verifies sentinel survival and 1:1 lockstep across `apply_rebuild_batch`, `collapse_vertex_scoped_with_bounds`, `remove_doubled_membranes_scoped_with_bounds`, `split_pinch_vertices_with_bounds`, `collapse_subtauwork_mesh_edges_with_bounds`, `collapse_subresolution_intersection_segments_with_bounds`, `weld_coincident_relocated_ex_with_bounds`, `retriangulate_collapsed_fan_regions_ex_with_bounds` (planar `z_plane`), `update_patches_ex_with_bounds` (planar rebuild + untouched sentinel `0.77`), and end-to-end curved `boolean()` runs (`cyl × box`, `cyl × cyl`).
3. `n106_d2_pin_b_curved_mutators_recompute_exact_d_of_t` (`:1218`): directly calls **curved** fixtures across all 7 required bound-producing mutators and asserts `bound > 0.0` and `bound.to_bits() == exact_dt.to_bits()` (`d_of_t`) on newly created curved triangles:
   - `TriangleErrorBoundMap::from_stage2_kept` on closed `Surface::Torus` (`R = 3.0, r = 1.0`) × box and `Surface::Cylinder` (`r = 1.5`) × box (`total_inner > 0` = `face_error_budget`, `total_bnd > 0` = `tri_dt`, `total_split > 0` = `recompute_tri_bound_from_world`),
   - `retriangulate_collapsed_fan_regions_ex_with_bounds` on curved `r = 5.0` cylinder × plane (`n_new_curved >= 8`),
   - `update_patches_ex_with_bounds` on curved `r = 5.0` cylinder (`n_rebuilt_curved >= 4` new curved triangles matching `d_of_t`, while untouched `(B, 0)` keeps `0.77`),
   - `corner_transit_apply` on curved `r = 20.0` cylinder × box (`Ok(true)`, verifying refilled curved cylinder triangles match `d_of_t`),
   - `decimate_patch_chains_441_with_bounds` on a 20-vertex `5e-5` generator chain on curved `r = 5.0` cylinder (`st.verts_dropped > 0`, refilled curved cylinder triangles match `d_of_t`),
   - `rebuild_merge_fan` + `apply_rebuild_batch` on curved `r = 5.0` cylinder,
   - `split_boundary_edge` + `apply_rebuild_batch` on curved `r = 5.0` cylinder.
4. `n106_d2_failed_d_of_t_and_unattributed_tris_store_nan_never_zero` (`:1090`): verifies `f64::NAN` (never `0.0`) is stored for `recompute_tri_bound_from_world` on `Surface::Sphere` (uncharted) and seam-straddling `Δθ > 2π` cylinder triangles, unattributed / out-of-range-face Stage-2 triangles in `from_stage2_kept`, `rebuild_merge_fan` / `rebuild_run_fan` / `rebuild_group_fan` on radius-0 invalid cylinders, and `transit_emission_fill` + `transit_emission_write` on `Surface::Sphere`.
5. Release-active `#[should_panic]` lockstep pins (`:903-1088`):
   - `n106_d2_pin_c_assert_lockstep_panics_on_length_mismatch` (`:903`),
   - `n106_d2_pin_c_apply_rebuild_batch_panics_on_tri_bounds_mismatch` (`:912`),
   - `n106_d2_pin_c_apply_rebuild_batch_panics_on_rebuild_bounds_mismatch` (`:938`),
   - `n106_d2_pin_c_repair_fan_cluster_panics_on_short_tri_bounds` (`:974`),
   - `n106_d2_pin_c_transit_emission_write_panics_on_short_polygon_bounds` (`:1009`).

### 2.4 Measured Runtime Cost (`D2.5`, `YANG_STAGE_TIME_PROBE=1`)

Measured under `reserve -c 6` on `loaf` (`ASSAY_JOBS=1 YANG_STAGE_TIME_PROBE=1 ASSAY_CASE=<ID> cargo test --release -p test-harness --test assay_kv2 single_case -- --ignored --exact --nocapture`, binaries pre-built with `--no-run --release`):

| Case | Metric | Before (`a1932058`) | After (`5bc3abdb`, identical kernel code to `HEAD`) |
| :--- | :--- | :--- | :--- |
| **`F0067`** | `single_case` total (`finished in ...`) | `204.03s` (`SUPPORTED_CORRECT (204.0s)`) | `203.30s` (`SUPPORTED_CORRECT (203.3s)`) |
| `F0067` (primary run, `a=4062 b=832`, loop iter 3 `k=2`) | `[stage-time]` | `total=21.34s pre_s2=13.83 s2=6.75 keep_resolve=0.25 s4_emit=0.50` | `total=21.15s pre_s2=13.76 s2=6.64 keep_resolve=0.25 s4_emit=0.50` |
| `F0067` (final monotonicity pass, `a=4062 b=832`, loop iter 3 `k=2`) | `[stage-time]` | `total=21.16s pre_s2=13.73 s2=6.67 keep_resolve=0.24 s4_emit=0.52` | `total=21.25s pre_s2=13.84 s2=6.66 keep_resolve=0.25 s4_emit=0.50` |
| **`R0044`** | `single_case` total (`finished in ...`) | `300.13s` (`TIMEOUT (300.1s)`) | `300.14s` (`TIMEOUT (300.1s)`) |
| `R0044` (natural STOP, `a=68 b=100348`) | `[stage-time]` | `STOPped total=29.63s pre_s2=0.21 s2=1.29 keep_resolve=0.12` | `STOPped total=28.75s pre_s2=0.20 s2=1.32 keep_resolve=0.13` |
| `R0044` (loop iter 0 `k=4`, `a=236 b=398716`, `tris=396058`) | `[stage-time]` + `[s4-lap] 441-update` | `total=80.65s pre_s2=0.75 s2=5.18 keep_resolve=0.51 s4_emit=74.16` (`441-update 2.24s`) | `total=81.44s pre_s2=0.76 s2=5.05 keep_resolve=0.54 s4_emit=75.04` (`441-update 2.22s`) |
| `R0044` (loop iter 1 `k=2`, `a=460 b=795644`, `tris=790068`) | `[stage-time]` + `[s4-lap] 441-update` | `total=117.14s pre_s2=1.55 s2=36.44 keep_resolve=1.15 s4_emit=77.89` (`441-update 5.87s`) | `total=115.97s pre_s2=1.50 s2=33.53 keep_resolve=1.21 s4_emit=79.62` (`441-update 5.77s`) |

The extra `d_of_t` calls add ~`0.03s`–`0.06s` in `keep_resolve` at `398k`–`795k` operand triangles and remain within run-to-run noise (`-0.73s` on `F0067`, `+0.01s` on `R0044`).

---

## 3. Deliverable D3 — Gated-Off Isometric-Chart Delaunay (`ChartMetric::Isometric`)

### 3.1 Primitive Implementation (`crates/yang-rs/src/stage4_construct.rs`)

- `pub(crate) enum ChartMetric { Parametric, Isometric }` (`stage4_construct.rs:539-542`).
- `pub(crate) fn isometric_cdt_pool(chart: &SurfaceChart, pool: &[Point2]) -> Vec<Point2>` (`stage4_construct.rs:556-596`):
  - `SurfaceChart::Cylinder { radius, .. }`: `(θ, y) ↦ (radius · θ, y)` (`ds² = r² dθ² + dy²`).
  - `SurfaceChart::Cone { tan_half, .. }`: developed circular sector around `(θ_0, z_0)` with `s = z · √(1 + tan²α)` and `φ = (θ − θ_0) · sin α`, mapping `(θ, z) ↦ (s · sin φ, (s − s_0) · cos φ − 2 · s_0 · sin²(φ / 2))` (`dX² + dY² = sec²α dz² + z² tan²α dθ²`).
  - `SurfaceChart::Plane` / `SurfaceChart::Torus`: unchanged.
- `rebuild_patch_planar_with_metric` (`stage4_construct.rs:601`) and `rebuild_patch_planar_bounded_with_metric` (`stage4_construct.rs:624`) run `cdt_with_interior_constraints` on `isometric_cdt_pool(&chart, &pool_ext)` when `metric == ChartMetric::Isometric`, while keeping vertex lifting, orientation matching, and `stage4_dt::d_of_t` certification in `(θ, y)` / `(θ, z)` unchanged. Production `rebuild_patch_planar_bounded_ex` (`stage4_construct.rs:505-520`) passes `ChartMetric::Parametric`.

### 3.2 D3 Unit Test Pins (`crates/yang-rs/src/tests_unit/n106_isometric_cdt.rs`)

3 unit tests in `n106_isometric_cdt.rs`:
1. `n106_d3_cylinder_r50_long_axial_span_eliminates_circumferential_slivers_and_reduces_dt` (`:88`):
   - Cylinder `r = 50.0`, `y ∈ [0, 60]`, `24` steps of `δθ = 0.05` (`θ ∈ [-0.6, +0.6]`):
   - Test output:
     - `[n106_d3_cylinder_r50] parametric (θ, y): max_aspect_3d=79.919423, pocket_slivers=46, max_dtheta=1.200000, min_dt=0.062615, max_dt=40.892456`
     - `[n106_d3_cylinder_r50] isometric (r·θ, y): max_aspect_3d=24.044166, pocket_slivers=0, max_dtheta=0.050000, min_dt=2.497526, max_dt=2.497573`
   - Asserts `iso_pocket_slivers == 0`, `(iso_max_dtheta - d_theta).abs() <= 1e-12`, `iso_max_ar < param_max_ar`, and `dt <= param_max_dt` for every triangle with `iso_max_dt < param_max_dt` (plus byte-identical `rebuild_patch_planar_bounded_ex` vs `ChartMetric::Parametric`).
2. `n106_d3_cone_developed_chart_isometry_and_cdt_eliminates_circumferential_slivers` (`:255`):
   - Verifies 2D vs 3D generator ruling and azimuthal arc-element isometry to `<= 1e-12`, and compares `ChartMetric::Parametric` vs `ChartMetric::Isometric` on a `half_angle = π/6`, `z ∈ [80, 130]` cone patch:
   - Test output:
     - `[n106_d3_cone] parametric (θ, z): max_aspect_3d=81.953345, pocket_slivers=46, min_dt=0.059293, max_dt=52.715040`
     - `[n106_d3_cone] developed sector:  max_aspect_3d=25.066275, pocket_slivers=0, max_dt=3.747225`
   - Asserts `iso_slivers == 0`, `iso_max_ar < param_max_ar`, and `dt <= param_max_dt` for every triangle with `iso_max_dt < param_max_dt`.
3. `n106_d3_cone_developed_chart_triangles_not_inverted_in_theta_z_or_3d` (`:433`):
   - Tests `ChartMetric::Isometric` across cone half-angles `π/6` and `π/3`, wide azimuthal span `θ ∈ [-1.2, +1.2]` (`Δθ = 2.4 rad`), `z ∈ [15, 75]`, curved boundary arcs, and 15 carried interior vertices; asserts every emitted triangle has strictly positive 2D signed area in the developed chart `(X, Y)`, strictly positive 2D signed area when mapped back to `(θ, z)`, and strictly positive 3D normal dot product with the analytic outward cone normal at its `(θ, z)` centroid and at all three vertices.

---

## 4. Verification & Exact Rewrite-Tier Result

- `cargo fmt --check -p yang-rs`: clean.
- `cargo clippy --all-targets -p yang-rs -- -D warnings`: clean (`0` warnings).
- `./scripts/build-wasm.sh`: rebuilt and committed in `app/static/pkg/`.
- `./scripts/test.sh rewrite` (run under `CARGO_TARGET_DIR=/home/claude/workspace/target/wt-agy-n106 ~/.claude/bin/reserve -c 6 -l "agy n106 rewrite tier" -- ./scripts/test.sh rewrite`):

```text
  ✓ cad-primitives (0s)
  ✓ cherchi-rs (22s)
  ✓ ssi-rs (0s)
  ✓ yang-rs (249s)
  ✓ kernel-v2 (1138s)
  ✓ cherchi-sidecar-rs (28s)
  ✓ indirect-predicates-sidecar-rs (0s)
  ✓ predicate-gen (0s)

  Kernel Rewrite tier completed in 1437s

All tests passed.
  Recorded rewrite-tier pass for kernel fingerprint f9c1da675661c01a820e6bef184ddd28abe3b61a (pre-push gate).
```

(`yang-rs` unit-test binary line within that run: `test result: ok. 1162 passed; 0 failed; 3 ignored; 0 measured; 0 filtered out; finished in 14.70s`.)

---

## 5. Open Items / Limitations Left Intentionally Unwired

1. **Production gates remain untouched (`AGY_BRIEF_N106.md`):** `TriangleErrorBoundMap` (`D2`) and `ChartMetric::Isometric` (`D3`) are unwired from control-flow decisions on this branch; all `ChordDegradation` / `CertificateFailed { what: "chord" }` gates (`A1`–`A9`), `I2e` seeding, and `local_fig11a_patch_plans` remain active for the F2 owner's flip.
2. **Torus chart metric in `D3`:** Per `AGY_BRIEF_N106.md`, `isometric_cdt_pool` implements intrinsic isometric charts for developable `Cylinder` and `Cone` surfaces; doubly-curved `Torus` (nonzero Gaussian curvature, no exact Euclidean isometry) is left unchanged.
3. **Developed-chart cone chords at very coarse sampling:** Straight segments in the developed cone chart `(X, Y)` curve slightly downward in `(θ, z)` (`z(θ) = c / cos((θ − θ_0)·sin α − φ_*)`). Pin 3 (`n106_d3_cone_developed_chart_triangles_not_inverted_in_theta_z_or_3d`) verifies non-inversion in both `(θ, z)` and 3D across `α ∈ {π/6, π/3}` and `Δθ = 2.4 rad` (`δθ = 0.1`); on pathologically coarse reflex cone chains where a boundary step spans a wide angle with sub-sagitta reflex dip (`z_1 cos(Δθ sin α) < z_mid < z_1`), an `(X, Y)` chord can cross `(θ, z)` even when valid in `(X, Y)`, which the F2 owner may keep in mind when un-gating `ChartMetric::Isometric` on cones.
4. **Full 374-case categorized assay:** Not run here per `AGY_BRIEF_N106.md` ("never run the full corpus (the owner does)").
