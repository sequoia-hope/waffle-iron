# Exact-Chain & Volume-Composition Oracle Fixes (`agy/oracle-extent`)

**Date:** 2026-11-04  
**Branch:** `agy/oracle-extent` (`/home/claude/wt/agy-oracle`)  
**Scope:** `crates/test-harness/` and `docs/agy/oracle_extent_summary.md` only (no kernel, engine, `INTERFACES.md`, or WASM changes).

---

## 1. Per-Commit Coverage Trajectory (`assay_exact_coverage::exact_coverage_census`)

Acceptance check (`crates/test-harness/tests/assay_exact_coverage.rs`):
- **Canonical corpus (`app/tests/cases/assay`, 374 `.waffle` files)**
- **Prospector seed-6 (`/home/claude/workspace/target/prospect/seed-6/candidates`, 198 `.waffle` files, via `COVERAGE_DIR`)**

| Revision | Description | Corpus (`374` cases) | Seed-6 (`198` cases) |
|---|---|---:|---:|
| `main` (`592c813f`) | Baseline before branch | **355** cov / `17` nc / `2` nochain | **195** cov / `3` nc |
| `ab63b201` | Initial 3-defect branch (pre-Review 1) | `290` cov / `82` nc / `2` nochain | `153` cov / `45` nc |
| `94faeefc` | Review 1 Fix 1: Sound outer bounds `outer_proj` replacing `+ h` sweep | `291` cov / `81` nc / `2` nochain | `152` cov / `46` nc |
| `26b5d972` | Review 1 Fix 2: Per-component cut reversal & explicit-anchor filtering | `293` cov / `79` nc / `2` nochain | `163` cov / `35` nc |
| `e2e74dbf` | Review 1 Fix 3: Coplanar prism contact, analytic `Disjoint`, consumer-scoped worlds | **355** cov / `17` nc / `2` nochain | `172` cov / `26` nc |
| `3bcda5fd` | Review 1 Fix 4: Non-overlap-inflating composition residual in `volume_oracle_doc.rs` | **355** cov / `17` nc / `2` nochain | `172` cov / `26` nc |
| **`4bfcfd70`** | **Review 2 Item 1:** Clamp `eps_n` in `coplanar_faces_overlap` to `<= 0.5 * TAU_MODEL` | **355** cov / `17` nc / `2` nochain | `172` cov / `26` nc |
| **`6c36ea45`** | **Review 2 Item 2:** Evaluate explicit `FeatureOutput` anchors across candidate lumps (`LumpPartition`, `Expr::Lump`) | **355** cov / `17` nc / `2` nochain | **173** cov / `25` nc |
| **`7f7ac116`** | **Review 2 Item 3:** Evaluate both cut reversals (`AltCut`, `Expr::AltLeaf`) before declining on disagreement | **355** cov / `17` nc / `2` nochain | **175** cov / `23` nc |
| **`b9e51663`** | **Review 2 Item 4:** Check `ThroughAll` `[d_occ, d_val]` uncertainty slab across multi-body `target_exprs` | **355** cov / `17` nc / `2` nochain | **175** cov / `23` nc |

### 1.1 Exact Corpus Swap vs `main` (`592c813f` -> `b9e51663`)

Although both `main` and this branch report `355` `COVERED` / `17` `NOTCOVERED` / `2` `NOCHAIN` (`C0100`, `F0074`), the covered sets differ by an exact **`2`-for-`2` swap**:
- **Gained (`NOTCOVERED -> COVERED` vs `main`):** `R0003` and `R0070` (surviving material measured via `expr_extent_along` + `tighten_outer_extent_along` instead of stale `Expr::Sub` `expr_bbox`).
- **Lost (`COVERED -> NOTCOVERED` vs `main`):** `P0016` and `P0027` (`main` had no multi-lump `OutputKey::Main` check and assumed union-bbox auto-reversal; Commit `7f7ac116` evaluates both reversals and proves that `reverse` vs `!reverse` produce distinct volumes on both cases: `297869 vs 340682` lattice cubes at `256` cells on `P0016`, and `148641 vs 70171` lattice cubes at `256` cells on `P0027`).

### 1.2 Per-Case Census Changes Across Review 2 Commits

- **After Commit `6c36ea45` (Review 2 Item 2 — Explicit `FeatureOutput` anchors across candidate lumps):**
  - **Corpus (`374` cases):** `0` per-case changes (`355 / 17 / 2`).
  - **Seed-6 (`198` cases, `172 / 26 -> 173 / 25`):**
    - `X00000006-00117`: `NOTCOVERED -> COVERED` (downstream volume invariant across candidate lumps; `prospect_judge` `exact_vs_mesh` `rel = -2.932e-8`, `bound_ratio = 2.015e-7`).
    - `X00000006-00043`: cleared explicit-anchor ambiguity, then `NOTCOVERED` at the existing coarseness gate (`lattice too coarse: surface cubes are 39 % of the volume at 256 cells (h = 4.685e-3)`).
    - `X00000006-00134`: cleared step-12 `BooleanCombine(Intersect)` explicit-anchor ambiguity, advancing to step-14 multi-component cut auto-reversal disagreement (`sketch_proj = 6.761175e-4`).
    - `X00000006-00008` (`3292536 vs 3415540`), `X00000006-00055` (`1697148 vs 1807208`), `X00000006-00073` (`1316253 vs 1316276`), `X00000006-00074` (`275073 vs 1012672`), `X00000006-00082` (`3050550 vs 2850222`), `X00000006-00162` (`1893296 vs 2632067`), `X00000006-00165` (`1734570 vs 1732402`), and `X00000006-00177` (`384464 vs 869334`): remain `NOTCOVERED` with exact `256`-cell cube-count witnesses proving genuinely distinct volumes across candidate `Main` lumps.
- **After Commit `7f7ac116` (Review 2 Item 3 — Both cut reversals evaluated via `AltCut` / `Expr::AltLeaf`):**
  - **Corpus (`374` cases, `355 / 17 / 2`):** `0` status flips; `P0016` (`297869 vs 340682 lattice cubes at 256 cells`) and `P0027` (`148641 vs 70171 lattice cubes at 256 cells`) now report explicit distinct-volume witnesses across `reverse` vs `!reverse`.
  - **Seed-6 (`198` cases, `173 / 25 -> 175 / 23`):**
    - `X00000006-00051`: `NOTCOVERED -> COVERED` (both reversals produce identical volume).
    - `X00000006-00057`: `NOTCOVERED -> COVERED` (both reversals produce identical volume; `prospect_judge` `exact_vs_mesh` `rel = +1.339e-9`, `bound_ratio = 6.072e-8`).
    - `X00000006-00134`: cleared step-14 multi-component cut auto-reversal disagreement, then `NOTCOVERED` at a subsequent `BooleanCombine` explicit anchor (`FeatureOutput(a25b6fb9-52b8-4440-914a-026511947f09, index=0)` on a 3-lump output: `239247 vs 689989 lattice cubes at 256 cells`).
    - `X00000006-00007` (`1176500 vs 1141893`), `X00000006-00024` (`165593 vs 184591`), `X00000006-00040` (`2563666 vs 2563450`), `X00000006-00042` (`1166600 vs 1058797`), `X00000006-00155` (`2265531 vs 2264674`), and `X00000006-00180` (`5802259 vs 5811884`): remain `NOTCOVERED` with explicit `256`-cell cube-count witnesses proving genuinely distinct volumes across `reverse` vs `!reverse`.
    - `X00000006-00022` (`min_d_lo`: `9.367997e0 -> 1.120723e1`) and `X00000006-00058` (`min_d_lo`: `1.773790e1 -> 1.803561e1`): remain `NOTCOVERED` with tighter per-lump `min_d_lo` bounds from `partition_lumps` + `Expr::Lump`.
- **After Commit `b9e51663` (Review 2 Item 4 — Multi-target `ThroughAll` `[d_occ, d_val]` slab check):**
  - `0` per-case changes on either corpus (`355 / 17 / 2`, keeping `P0012` covered via `solid_proj_bound` / `expr_proj_bound` frame clamping in `slab_must_miss_expr`) or seed-6 (`175 / 23`).

### 1.3 Final `NOTCOVERED` Breakdown (`b9e51663`, zero `Add` / `Union` contact declines)

- **Corpus (`17` `NOTCOVERED`):** `5` empty/sub-lattice at `256` cells (`C0034`, `C0117`, `P0023`, `R0007`, `R0027`), `5` surface cubes `> 25%` (`C0040`, `R0009`, `R0031`, `R0038`, `R0078`), `4` single-body cuts with `sketch_proj` inside `[mid_lo, mid_hi]` (`C0086`, `F0058`, `F0060`, `R0020`), `2` multi-component cuts with distinct volumes across `reverse` vs `!reverse` (`P0016`, `P0027`), and `1` cut target with zero occupied lattice samples (`R0088`).
- **Seed-6 (`23` `NOTCOVERED`):** `9` explicit `FeatureOutput` anchors whose candidate lumps yield distinct volumes (`00008`, `00055`, `00073`, `00074`, `00082`, `00134`, `00162`, `00165`, `00177`), `6` multi-component cuts whose opposing reversals yield distinct volumes (`00007`, `00024`, `00040`, `00042`, `00155`, `00180`), `2` multi-component `ThroughAll` cuts whose `[min_d_lo, d_val]` slab crosses target material (`00022`, `00058`), `2` single-body cuts inside `[mid_lo, mid_hi]` (`00067`, `00072`), `2` surface cubes `> 25%` (`00043` at `39%`, `00122` at `29%`), and `2` empty/annihilated chains (`00000`, `00095`).

---

## 2. Mechanisms Implemented (`crates/test-harness/`)

### 2.1 True Cut Extent, Sound Outer Bounds, and Multi-Target `ThroughAll` Slabs
- **Files & symbols:** `crates/test-harness/src/assay/exact_membership.rs` (`SampledExtent` L3378, `TargetExtent` L3397, `target_extent_for_bodies` L3423, `target_extent` L3540, `slab_must_miss_expr` L3616, `expr_extent_along` L4540, `expr_proj_bound` L4636, `tighten_outer_extent_along` L4727, `solid_proj_bound` L4907, `check_thru_slabs` L2816).
- **Mechanism:**
  1. `expr_extent_along` computes both the occupied-sample bracket `occupied = Some((occ_lo, occ_hi))` and a sound outer projection bound `outer_proj = (outer_lo, outer_hi)` via `tighten_outer_extent_along`, which prunes only sub-boxes certified empty by `box_must_miss`.
  2. For a single-component primary cut target, `outer_lo <= true_lo <= occ_lo` and `occ_hi <= true_hi <= outer_hi` imply that the true midpoint lies in `[mid_lo, mid_hi] = [0.5 * (outer_lo + occ_hi), 0.5 * (occ_lo + outer_hi)]`; `sketch_proj` inside `[mid_lo - fp_pad, mid_hi + fp_pad]` records `indeterminate`.
  3. `ThroughAll` resolves depth `d_val` from `outer_proj` plus `THROUGH_ALL_OVERSHOOT` (`1e-2`). When multiple targets are present in `target_exprs` (such as leftovers of a disjoint `Add`) or `is_sym` is active, `check_thru_slabs` applies `slab_must_miss_expr` across all `target_exprs` over `[d_occ, d_val]` (and `[-d_val, -d_occ]` when `is_sym`) so secondary targets cannot be silently cut deeper than the engine's `d_true`. `slab_must_miss_expr` clamps its frame box by `solid_proj_bound` and `expr_proj_bound` along `frame.x`, `frame.y`, `frame.n` (and `dir` on `ExactSolid::Extrude`) to avoid world-AABB rotation bloat on oblique sweeps (`P0012`).

### 2.2 Explicit `FeatureOutput` Anchors and Dual-Reversal Cut Evaluation
- **Files & symbols:** `crates/test-harness/src/assay/exact_membership.rs` (`AltCut` L564, `VoronoiNode` L588, `LumpPartition` L606, `LumpPartitionHandle` L620, `LumpPartitionOutcome` L631, `Expr::AltLeaf` L641, `Expr::Lump` L645, `leaf_solid` L996, `worlds_after` L1077, `stage_explicit_merge_anchors` L1912, `stage_explicit_op_anchors` L2003, `stage_fid_anchor` L2058, `check_worlds_volume_agreement` L2330, `partition_lumps` L3860).
- **Mechanism:**
  1. **`partition_lumps` & `Expr::Lump`:** `partition_lumps` flood-fills `256`-cell components of `Sub`/`Inter` bodies (bridging diagonal surface steps only when `t ∈ {0.25, 0.5, 0.75}` stay inside `expr`), builds a KD-tree (`VoronoiNode`) over surface cells, and tightens per-lump world boxes `lump_wbox`. `Expr::Lump { base, partition, lump }` evaluates exact analytical `expr_contains(base, p)` restricted to the nearest-surface-seed Voronoi cell `nearest_lump(p) == lump`, with sound sub-box pruning via `box_must_miss_voronoi` and `box_must_cover_voronoi`.
  2. **Explicit `FeatureOutput` anchors (`stage_explicit_merge_anchors`, `stage_explicit_op_anchors`, `stage_fid_anchor`):** When an explicit anchor references a subset of slots on a multi-lump `feature_id`, `stage_fid_anchor` splits unsplit `Sub`/`Inter` bodies into `Expr::Lump` bodies, fast-paths operands proven `TouchVerdict::Disjoint` from `other_expr`, and otherwise branches `CustodyWorld` over injective slot assignments (`MAX_WORLDS = 32`), declining via `check_worlds_volume_agreement` only when resulting world volumes differ.
  3. **Dual cut-reversal branching (`AltCut`, `Expr::AltLeaf`):** When candidate components in `target_extent_for_bodies` (each measured via `Expr::Lump`) disagree on direction-less cut auto-reversal, `from_waffle` builds both `reverse` (`Expr::Leaf(i)`) and `!reverse` (`AltCut` / `Expr::AltLeaf(i)`, scoping `ThroughAll` depth to components compatible with each reversal), branches `CustodyWorld` over reversals compatible with each world's primary target, and declines only when `check_worlds_volume_agreement` finds differing volumes.

### 2.3 Coplanar Contact Proof, Analytic `Disjoint`, and Consumer-Scoped Worlds
- **Files & symbols:** `crates/test-harness/src/assay/exact_membership.rs` (`CustodyWorld` L3336, `TouchVerdict` L3351, `exprs_touch` L4103, `collect_extrude_patches` L4309, `coplanar_faces_overlap` L4421, `box_must_miss` L4685, `box_must_cover` L4704, `solid_must_miss` L5108, `solid_must_cover` L5306).
- **Mechanism:**
  1. **`TouchVerdict::Touches`** requires either a shared analytical interior witness (`expr_contains(a, p) && expr_contains(b, p)`) or `coplanar_faces_overlap`, which lifts a 2D `±delta_uv` cross by `±eps_n` (`eps_n <= 0.5 * TAU_MODEL`) along the shared plane normal so witness pairs sit at distance `<= 2 * eps_n <= TAU_MODEL` across a positive-area 2D patch.
  2. **`TouchVerdict::Disjoint`** is certified strictly from analytic separation beyond `TAU_MODEL` via world/frame AABB clearance or a `512`-split KD-tree calling `box_must_miss` on `0.5 * sep_tol`-padded sub-boxes (`X00000006-00112`: `1.535e-6 m > TAU_MODEL`).
  3. **`TouchVerdict::Ambiguous`** branches `CustodyWorld` (`MAX_WORLDS = 32`) without declining pure `Add`/`Union` chains; `check_worlds_volume_agreement` declines only when a downstream consumer produces distinct volumes across worlds.

### 2.4 Non-Overlap-Inflating Composition Residual
- **Files & symbols:** `crates/test-harness/src/assay/volume_oracle_doc.rs` (`evaluate_composition` L258, `evaluate_composed_scans` L296).
- **Mechanism:** `expected_residual = expected.residual.max(fine_composed_residual.min(max_operand_residual))`, where `fine_composed_residual = composed_volume(&refs, &cuts, 2 * grid).residual` (`|V(4n) - V(2n)|`, evaluated lazily when `max_operand_residual > expected.residual`). Across all `150` union-only corpus cases, `139 / 150` keep `1.0000x` of `main`'s band, only `11 / 150` loosen (`> 1.0001x`), only `1` exceeds `2x` (`R0077 2.80x`), `C0089` stays `1.00x`, and `X00000006-00100` (`rel = 3.497e-4 <= band = 9.176e-4`) agrees with `0` corpus flags.

---

## 3. Tolerances & Numerical Margins (`exact_membership.rs`)

| Constant / Expression | Location | Value | Mathematical Reason |
|---|---|---|---|
| `eps_n` | `exact_membership.rs:4480-4482` | `max(1e-6 * min(step_a, step_b), 2 * dist_tol).min(0.5 * TAU_MODEL)` (`<= 5e-8 m`) | Clamping the normal lift to `TAU_MODEL / 2` guarantees `\|\|(q + sa*n) - (q + sb*n)\|\| = \|sa - sb\| <= 2 * eps_n <= TAU_MODEL` (`sa, sb ∈ {+eps_n, -eps_n}`). Across the `±delta_uv` cross, either `a ∩ b != ∅` or every normal segment of length `<= TAU_MODEL` crosses both `∂a` and `∂b`, making `coplanar_faces_overlap` a true proof that `dist(∂a, ∂b) <= TAU_MODEL` over positive 2D area even at large model scale. |
| `delta_uv` | `exact_membership.rs:4483` | `1e-5 * uv_min` | Requires all four points `(u ± delta_uv, v)`, `(u, v ± delta_uv)` around a cell center on the `32 × 32` overlap patch to satisfy lifted `expr_contains`, proving positive 2D contact area and rejecting 1D edge or 0D vertex kissing. |
| `ang_tol`, `dist_tol` | `exact_membership.rs:4429-4430` | `10.0 * TAU_WORK` (`1e-11`), `10.0 * TAU_WORK * scale_ref` | Pre-filters candidate `PlanarPatch` pairs whose normals or offsets differ solely by `Basis`/`norm`/`cross` working-precision drift (`TAU_WORK = 1e-12`, `100×` below `TAU_MODEL`); final `Touches` certification still comes strictly from `expr_contains` at `±eps_n <= TAU_MODEL / 2`. |
| `1e-11` skins (`t_eps`, `r_tol`, `tol`) | `exact_membership.rs:5179, 5321, 5336, 5351` | `1e-11 * scale` | In `solid_must_cover` (`L5321, L5336, L5351`), absorbs `~1e-15..1e-14` `Basis`/`embed` roundoff on flush `Sub(a, b)` cap/wall planes so `box_must_miss(Expr::Sub(a, b))` can prune subtracted flush faces while staying `10,000×` finer than `TAU_MODEL` (`1e-7`). At `L5179` (`solid_must_miss`), conservatively widens convex-polygon edge cross-product bounds against `f64` dot/cross roundoff so grazing boxes are never falsely pruned. |
| `c_pad`, `fp_pad` | `exact_membership.rs:1818, 1827, 1839, 2729, 2738, 2756, 2869` | `1e-9 * span` | Widens the indeterminate midpoint bracket `[mid_lo - pad, mid_hi + pad]` by `1e-9` of axial span (`~10^7` ULPs, negligible geometrically) so exact midpoint ties (e.g. `sketch_proj = 0` on `[-a, a]` in `F0060`) conservatively trigger dual-reversal / indeterminate evaluation instead of choosing a sign from `f64` roundoff. |
| `sep_tol` (`half_sep`) | `exact_membership.rs:4117, 4261` | `TAU_MODEL + TAU_WORK * world_scale.max(1.0)` | Sub-boxes in `exprs_touch` are padded by `0.5 * sep_tol` on both operands before `box_must_miss`, proving Euclidean clearance strictly `> TAU_MODEL` before returning `TouchVerdict::Disjoint`. |
| `1e-12` roundoff pads & guards | `exact_membership.rs:947, 976, 4321, 4357, 4933, 4982, 5046, 5118, 5121, 5149, 5264, 5318` | `1e-12 * scale` | Outward/inward IEEE-754 safety margins (`~4,000` ULPs) on `LumpPartition` Voronoi pruning (`L947, L976`), `solid_proj_bound` (`L4933, L4982`), `polygon_convex_sign` (`L5046`), and `solid_must_miss` (`L5121, L5149, L5264`), plus zero-thickness/zero-edge degeneracy guards (`L4321, L4357, L5118, L5318`). |

---

## 4. Unit Tests (`crates/test-harness/`)

| Test Symbol | Location | What It Verifies |
|---|---|---|
| `cut_direction_and_through_all_measure_membership_not_stale_bbox` | `exact_membership.rs:6593` | Stale `Expr::Sub` bbox (`[0, 4]` after `[1, 4]` removed -> surviving `[0, 1]`) auto-reverses a cut at `z = 1.2`, declines (`NotCovered`) at `z = 0.502` inside `[mid_lo, mid_hi]`, and resolves `ThroughAll` against `SampledExtent::outer_proj`. |
| `thin_spike_beyond_occupied_sample_uses_sound_outer_bounds` | `exact_membership.rs:6719` | Sub-lattice spike missed by `occupied` is still bracketed by `outer_proj`: `ThroughAll` clears the spike, and a cut inside `[mid_lo, mid_hi]` declines (`NotCovered`). |
| `add_touching_and_multi_lump_custody_use_membership_not_bbox` | `exact_membership.rs:6852` | Analytic `Disjoint` on overlapping AABBs; sub-`TAU_MODEL` gap (`5e-8 m`) stays `Agree` on pure `Add` and declines when targeted; `10 m`-scale coplanar contact proves `Touches` under `eps_n <= TAU_MODEL / 2` while a `2e-6 m` (`20 * TAU_MODEL`) gap proves `Disjoint`; multi-lump `FeatureOutput` anchors return `Agree` when candidate lumps yield equal volume (`2.5`) and `NotCovered` when volumes differ (`1.0` vs `2.0`). |
| `multi_component_cut_only_declines_when_components_disagree` | `exact_membership.rs:7295` | Agreeing multi-component blind (`1a`) and `ThroughAll` (`1b`) cuts stay `Agree`; opposing component reversals with equal resulting volume (`1c`, `z = 2.0` -> `2.875`) stay `Agree`; opposing reversals with differing volumes (`1d`, `z = 1.8` -> `2.825` vs `2.925`) decline (`NotCovered`); disjoint chained `BooleanCombine(Union)` (`2`) stays `Agree`. |
| `through_all_with_multiple_targets_checks_secondary_target_slab` | `exact_membership.rs:7493` | Multi-target `ThroughAll` cut (`b2` primary with un-sampled `z ∈ [1, 3]` fin + disjoint `Add` leftover `b1`) declines (`NotCovered`) when `b1` sits inside `[d_occ, d_val]` (`z ∈ [1.8, 2.4]`) and returns `Agree` when `b1` sits clear of `[d_occ, d_val]` (`z ∈ [4.0, 4.6]`). |
| `composition_band_carries_dominant_operand_residual` | `volume_oracle_doc.rs:492` | `synth-00100` returns `Agree`; duplicate overlapping operands keep identical band (`<= 1e-12`); buried `star6` inside a `40^3` box does not inflate the `1e-5` box band; dropped protruding spikes (`synth-dropped`) return `Flag`. |

---

## 5. Seed-6 Re-Judged Verdicts (`b9e51663`) — Agreement vs Decline

Command per candidate:
```bash
CARGO_TARGET_DIR=/home/claude/workspace/target/wt-agy-oracle \
  PROSPECT_CANDIDATE=/home/claude/workspace/target/prospect/seed-6/candidates/X00000006-<id> \
  cargo test --release -p test-harness --test prospect prospect_judge -- --ignored --nocapture
```

| Row | Judge Verdict | How Reached (`exact_volume` / `volume_composition`) | Mechanism & Verbatim `detail` |
|---|---|---|---|
| **`00021`** | **`SUPPORTED_WRONG`** *(engine side — expected pending `cut-true-extent`)* | **`exact_volume`: COVERED (`Flag`)** | Engine `fe-cut` reads `circle:boss` B-Rep seam vertices (`reverse = true` -> `kernel = 196.689`), while `exact_volume` uses true cylinder extent (`reverse = false` -> `exact = 272.466`, `band = 7.249e-2`).<br>`detail`: `exact_volume: kernel 1.966892e2 vs exact 2.724657e2 (rel -2.781e-1 outside band 7.249e-2); on_surface: max 8.260e-14 m, rel 2.370e-15, solids 2 at (7.544039e0, 1.379686e0, 5.879939e0); mesh_on_surface: plane rel 1.402e-16, chord ratio 9.275e-1 at (-1.277997e0, -7.554715e0, 2.806778e0), solids 2; exact_vs_mesh: exact 0.000000000e0 mesh 0.000000000e0 rel NaN bound_ratio NaN (+2 unmeasured); monotonicity: 0 checked, 0 not covered, 0 violation(s)` |
| **`00024`** | **`SUPPORTED_CORRECT`** | **By DECLINE** (`exact_volume`: `NotCovered`) | `AltCut` evaluates both reversals on the 2-lump primary cut target at `sketch_proj = -2.255443e-3` and proves genuinely distinct volumes (`165593 vs 184591 lattice cubes at 256 cells`), so `exact_volume` declines.<br>`detail`: `all checks passed; on_surface: max 4.337e-19 m, rel 8.278e-17, solids 1 at (1.862133e-3, 7.615723e-4, -2.087481e-3); mesh_on_surface: plane rel 3.622e-17, chord ratio 8.700e-1 at (1.608501e-3, 5.324891e-4, -2.143468e-3), solids 1; exact_vs_mesh: exact 0.000000000e0 mesh 0.000000000e0 rel NaN bound_ratio NaN (+1 unmeasured); monotonicity: 0 checked, 0 not covered, 0 violation(s)` |
| **`00100`** | **`SUPPORTED_CORRECT`** | **By AGREEMENT** (`exact_volume`: `Agree`, `volume_composition`: `Agree`) | Both `exact_volume` (`COVERED`) and `volume_composition` (`rel = 3.497e-4 <= band = 9.176e-4`) actively agree with the kernel union (`2211.598`), with `coplanar_faces_overlap` clamped to `eps_n <= TAU_MODEL / 2`.<br>`detail`: `all checks passed; on_surface: max 6.328e-15 m, rel 6.573e-16, solids 1 at (-1.915357e0, 4.461088e0, -5.167536e0); mesh_on_surface: plane rel 6.573e-16, chord ratio 8.913e-1 at (1.751485e0, 5.478077e0, -3.101482e0), solids 1; exact_vs_mesh: exact 0.000000000e0 mesh 0.000000000e0 rel NaN bound_ratio NaN (+1 unmeasured); monotonicity: 0 checked, 0 not covered, 0 violation(s)` |
| **`00112`** | **`SUPPORTED_CORRECT`** | **By AGREEMENT** (`exact_volume`: `Agree`, `exact_vs_mesh`: `Agree` on cylinder) | `exprs_touch` proves `dist(cylinder, hex_boss) > TAU_MODEL` (`1.535e-6 m > 1e-7 m`) -> `TouchVerdict::Disjoint`, keeping the cylinder as `Body { index: 1 }` untouched by `BooleanCombine(Intersect)` on `Main`, so `exact_volume` (`COVERED`) agrees with the 2-body output (`2.427805e-11`).<br>`detail`: `all checks passed; on_surface: max 1.084e-19 m, rel 1.947e-16, solids 2 at (3.172875e-4, 4.743469e-4, 1.399470e-4); mesh_on_surface: plane rel 4.928e-16, chord ratio 8.700e-1 at (4.720387e-5, -2.253529e-4, 4.793952e-5), solids 2; exact_vs_mesh: exact 1.199682318e-11 mesh 1.198117036e-11 rel -1.305e-3 bound_ratio 6.420e-3 (+1 unmeasured); monotonicity: 0 checked, 0 not covered, 0 violation(s)` |
| **`00122`** | **`SUPPORTED_CORRECT`** | **By DECLINE** (`exact_volume`: `NotCovered`) | `target_extent` computes `reverse = false` from surviving material + `outer_proj`, and `exact_volume_verdict` then declines at the existing coarseness gate (`lattice too coarse: surface cubes are 29 % of the volume at 256 cells (h = 1.974e-2)` > `25%`).<br>`detail`: `all checks passed; on_surface: max 5.829e-16 m, rel 8.324e-17, solids 1 at (1.699417e-1, 1.327799e0, -1.043244e0); mesh_on_surface: plane rel 1.070e-16, chord ratio 8.692e-1 at (-3.293232e-1, 1.596798e0, -1.003416e0), solids 1; exact_vs_mesh: exact 0.000000000e0 mesh 0.000000000e0 rel NaN bound_ratio NaN (+1 unmeasured); monotonicity: 0 checked, 0 not covered, 0 violation(s)` |
| **`00134`** | **`SUPPORTED_CORRECT`** | **By DECLINE** on `exact_volume` (`NotCovered`); **By AGREEMENT** on `exact_vs_mesh` (`rel = +4.218e-8`) | Step 12's anchor ambiguity (`6c36ea45`) and step 14's cut auto-reversal disagreement (`7f7ac116`) both resolve equal-volume, and `exact_vs_mesh` agrees across all 3 surviving solids (`9.279941e-9` vs `9.2799406e-9`), while `exact_volume` declines on a subsequent `BooleanCombine` explicit anchor (`FeatureOutput(a25b6fb9-52b8-4440-914a-026511947f09, index=0)` on a 3-lump output: `239247 vs 689989 lattice cubes at 256 cells`).<br>`detail`: `all checks passed; on_surface: max 6.370e-19 m, rel 7.556e-17, solids 3 at (2.436397e-4, -4.564932e-3, -9.204300e-5); mesh_on_surface: plane rel 4.943e-17, chord ratio 0.000e0 at (NaN, NaN, NaN), solids 3; exact_vs_mesh: exact 9.279940631e-9 mesh 9.279941022e-9 rel +4.218e-8 bound_ratio 6.727e-8; monotonicity: 0 checked, 0 not covered, 0 violation(s)` |

---

## 6. Verification Suites & Static Checks (`b9e51663`)

Executed in the foreground under `~/.claude/bin/reserve --here`:
- `cargo clippy --all-targets -p test-harness -- -D warnings`: **clean (`exit 0`)**
- `cargo fmt --check -p test-harness`: **clean (`exit 0`)**
- `cargo test --release -p test-harness --lib`: **193 passed; 0 failed; 1 ignored**
- `cargo test --release -p test-harness --test assay_exact_membership`: **12 passed; 0 failed; 5 ignored**
- `cargo test --release -p test-harness --test assay_generative_pairs`: **21 passed; 0 failed; 0 ignored**
- `cargo test --release -p test-harness --test assay_metamorphic`: **33 passed; 0 failed; 4 ignored**
