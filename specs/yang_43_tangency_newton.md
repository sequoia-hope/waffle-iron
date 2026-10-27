# Yang 2025 §4.2.2–§4.4.1 — Tangency, Small-Loop Tracing, Gauss-Map Admission & Fig. 10(c) Seeding (F4 / N91 + N92)

**Status:** Design note & 374-case corpus census (Roadmap milestone F4, ledger `N91` + folded `N92`)
**Date:** 2026-10-27
**Paper authority:** Yang, Jia, Wang, Yang, Xin & Yan 2025, *Boolean Operation for CAD Models Using a Hybrid Representation*, ACM TOG 44(4), Art. 114 (`refs/text/yang2025_hybrid_boolean.txt`)
**Related specs:** `specs/yang_faithful_cleanup.md`, `specs/yang_433_tangent_point_mesh_update.md`, `specs/yang_tangency_pinch_split.md`
**Audit rows:** Findings `M-3`, `M-4`; rows `4.2-12`, `4.2-13`, `4.2-17..20`, `4.3-04`, `4.3-06..08`, `4.3-11..12`, `4.3-18..20`, `4.4-06`, `5.6-01`, `5.6-02`

---

## 1. Paper Mechanisms and Target Code Host Sites

Yang et al. 2025 resolve surface tangencies and sub-triangle intersection loops through a six-part pipeline spanning §4.2.1–§4.4.1 and Appendix C (`refs/text/yang2025_hybrid_boolean.txt` line numbers and `L`/`R` columns cited from `./scripts/extract-papers.sh`):

| # | Paper mechanism | Paper citation (`yang2025_hybrid_boolean.txt`) | Mathematical rule | Target `yang-rs` host site |
|---|---|---|---|---|
| **M1** | **Non-crossing `< 2d_ε` proximity detection** | §4.2.1 Eq. (1), `:433-461` L; Fig. 10(a) `:487-489` L | Triangles `△t_A ∈ M_A, △t_B ∈ M_B` with `Dis(△t_A, △t_B) < 2d_ε` (`d_ε` per-face / per-triangle chord sagitta bound) and no mesh arrangement crossing form candidate non-crossing regions (Case III: tangent point or small loop; Case IV: non-intersecting near-miss). | `crates/yang-rs/src/stage4_tangency.rs` (new module) fed by Stage-1 face meshes and Stage-2 arrangement crossing sets in `crates/yang-rs/src/boolean.rs` / `crates/yang-rs/src/boolean/tangency.rs` |
| **M2** | **§4.2.2 Gauss-map overlap admission filter (`N92`)** | §4.2.2 `:440-446` R, `:466-470` R; Fig. 10(b) `:489-491` L; §5.6 `:1026-1035` L | Within any closed small intersection loop (Sederberg et al. 1989) — and trivially at any tangency point — there exist `p_A ∈ S_A, p_B ∈ S_B` with `n_A(p_A) = ±n_B(p_B)`. Compute exact closed-form normal cones (`GaussMap`) over the `< 2d_ε` triangle sub-patch on each analytic surface and check `g_A.overlaps(&g_B)` (`n_A = +n_B` or `n_A = -n_B`). Non-overlap rules out both tangency points and closed small loops without invoking optimization. | `crates/yang-rs/src/gauss_map.rs` (closed-form `GaussMap` primitive for all 5 analytic `Surface` variants, extracted from `1519f6ed`) called by `stage4_tangency` as the candidate-region admission test |
| **M3** | **Fig. 10(c) seed construction & curve-redundancy removal** | §4.2.2 `:471-473` R, `:491-495` L, `:498-505` L | On each Gauss-map-admitted `< 2d_ε` candidate region, locate triangle `T ∈ M_A` nearest to `M_B`, compute centroid `p = barycenter(T)`, and find the closest point `q ∈ M_B` to `p`. Project `(p, q)` to surface parameters `(u_0, v_0), (s_0, t_0)`. Discard if the solved point falls within tolerance of an already-solved intersection curve (`:502-505` L). | `crates/yang-rs/src/stage4_tangency.rs` (`fig10c_seeds_for_region`, `is_redundant_with_solved_curves`) |
| **M4** | **§4.3.1 / Appendix C parametric Newton & tangency Newton** | §4.3 `:521-537` L; §4.3.1 `:479-492` R; §4.3.3 `:518-537` R; App. C `:1232-1250` R | Geometric two-plane projection (§4.3.2 `:493-516` R) requires `n_A × n_B ≠ 0` and is restricted to transversal mesh-intersection loops. Tangential points and `< 2d_ε` non-crossing seeds use Newton's method on parametric surfaces: `D(u,v,s,t) = s_1(u,v) - s_2(s,t) = 0` via minimum-norm step `∇D ∇D^T a_k = s_2 - s_1`, `x_{k+1} = x_k + ∇D^T a_k` (`:1240-1250` R), paired with coupled tangency polish `{s_1(u,v) - s_2(s,t) = 0, L · (n_1(u,v) - σ n_2(s,t)) = 0}` to resolve multiplicity-2 elliptic kisses, 1D grazing circles/rulings, and hyperbolic (`X`-pinch) saddle tangencies to machine precision inside pole-free local surface charts. Out-of-domain or `‖s_1 - s_2‖ > d_p` rules out Case IV (`:529-534` R). | `crates/yang-rs/src/stage4_tangency.rs` (`LocalSurfaceChart`, `solve_parametric_pair_newton`, `solve_tangent_point_newton`), called from `stage4_relocate.rs` (when `relocate_onto_implicit_pair` encounters near-parallel normals) and pre-arrangement tangency/loop discovery in `boolean/tangency.rs` / `boolean.rs` |
| **M5** | **§4.3.3 single-point tangency vs. small-loop classification + Bajaj et al. 1988 loop marching + §4.3.4 refinement** | §4.3.3 `:535-540` R, `:567-573` L (older extraction `:598-601`); §4.3.4 `:575-593` L; §5 `:743-745` R | At an isolated converged point `r = s_1(u,v) = s_2(s,t)`, compare `n_A(r)` and `n_B(r)` against the paper's angular tolerance `10^{-6}` rad (`:745` R). If `∠(n_A, ±n_B) ≤ 10^{-6}` rad, `r` is a **tangent point**. Otherwise (`∠(n_A, ±n_B) > 10^{-6}` rad), `r` lies on a **small closed loop**; trace the full closed loop from `r` along `t = (n_A × n_B) / ‖n_A × n_B‖` with orthogonal-plane surface-pair corrector (Bajaj et al. 1988, `:570-573` L) and subdivide segments until `h < 10^2 d_p`, `l < 10^3 d_p`, `α < π/18` (§4.3.4 `:588-590` L). | `crates/yang-rs/src/stage4_tangency.rs` (`classify_isolated_point`, `trace_small_loop_bajaj`, `refine_loop_s434`) |
| **M6** | **§4.4.1 mesh updating (`r_A = r_B = r`) & pinch-manifoldness** | Fig. 10(c) `:494-495` L; Fig. 11 & §4.4.1 `:547-570` R | Insert solved tangent point `r` or traced small-loop polyline vertices into `M_A` and `M_B` with bit-identical coordinates `r_A = r_B = r` so Cherchi's exact mesh arrangement (`Stage 2`) sees shared curve/pinch vertices; split figure-eight pinch vertices into manifold cycles (`split_pinch_vertices`). | `crates/yang-rs/src/stage1_tessellate.rs` (`face_overrides` / interior point & constraint insertion) + `crates/yang-rs/src/boolean/tangency.rs` + `crates/yang-rs/src/stage4_correct.rs` (`split_pinch_vertices`) |

---

## 2. Inventory of What Exists Today

### 2.1 Ambient two-plane pair Newton and its tangency refusal (`stage4_relocate.rs`)
- `relocate_onto_implicit_pair` (`crates/yang-rs/src/stage4_relocate.rs:285-351`) implements §4.3.2's **geometric tangent-plane intersection** in ambient `ℝ^3` (`[n_1 · δx = -f_1, n_2 · δx = -f_2]` minimum-norm step) rather than Appendix C's 4-parameter surface-chart Newton.
- Line `315-318` computes the Gram determinant `det = n1_sq * n2_sq - n1_n2 * n1_n2 = ‖n_1 × n_2‖^2` with `rank_eps = (MIN_FEATURE_SIZE^2) * n1_sq * n2_sq` (`MIN_FEATURE_SIZE = 1e-5`, so `sin θ ≤ 1e-5` rad) and **refuses**:
  ```rust
  if det <= rank_eps {
      return None;
  }
  ```
- As established in `specs/yang_433_tangent_point_mesh_update.md` §3, simply shrinking `rank_eps` inside the ambient `ℝ^3` solver is mathematically unsound: for a unit cylinder kissing `y = 0` at `(x_0, 0, 0)`, the two linearized tangent planes meet at `y = -x_0^2 / 2` (off the cylinder), whereas the true intersection is `(0, 0, 0)`, and near `x_0 → 0` the step divides `O(x_0^2)` by `O(x_0^2)` to jump to `O(1)`. Resolving tangency requires optimizing in surface parameter coordinates `(u, v, s, t)` so iterates stay on `S_A × S_B`.
- `relocate_onto_implicit_triple` (`stage4_relocate.rs:367-428`) similarly checks `|det(n_1, n_2, n_3)| <= MIN_FEATURE_SIZE * ‖n_1‖ ‖n_2‖ ‖n_3‖` (`:398-401`) and returns `None` when three surface normals are coplanar (e.g., two tangent cylinders meeting an endcap plane).

### 2.2 Stage-1 `Cylinder × Cylinder` closed-form tangent mint (`boolean/tangency.rs`)
- `tangent_point_face_overrides` (`crates/yang-rs/src/boolean/tangency.rs:634-652`, called pre-Stage-1 from `boolean/junction.rs:1348`) inspects only `Surface::Cylinder × Surface::Cylinder` pairs via `tangent_overrides` (`:659-838`), which contains three distinct mechanisms:
  1. **0D crossing-axis point tangency (`cyl_cyl_tangent_points`, `:925-1002`, wired at `:799-836`):** closed-form line-to-line closest points `c_a ± r_a n`, `c_b ∓ r_b n` along `n = ±(d_a × d_b)/‖d_a × d_b‖` when `|‖c_b - c_a‖_⊥ - (r_a + r_b)| ≤ 100·TAU_MODEL` or `|‖c_b - c_a‖_⊥ - |r_a - r_b|| ≤ 100·TAU_MODEL`. Inserts `p` as a `face_overrides` interior point on both cylinder meshes plus the ruling azimuth sample on canonical tube rims so `splice_lateral_interior_points` (`stage1_tessellate.rs:4243`) performs a conforming 2+2 edge split (`N82`). **This closed-form cylinder×cylinder 0D mint is a surface-pair-specific stand-in for §4.3's general Newton tangency solver and is slated for retirement in Step 2b once general Newton tangency discovery + mesh insertion replaces it.**
  2. **1D parallel-axis line tangency (`mint_generator`, `:715-755`):** closed-form shared generator ruling for parallel tangent cylinders (`|d - (r_a + r_b)|` or `|d - |r_a - r_b||`), which is a 1D degenerate SSI locus (outside 0D point-Newton scope; retained under `N82`).
  3. **1D near-parallel grazing ruling (`mint_crossing_rulings`, `:761-795`):** ruling sample alignment for near-parallel cylinders whose chord sagitta hides a narrow strip (`C0057`, `R0038`, etc.; retained).

### 2.3 `P0026` — Adjudicated minimizer-minted 1D line tangency
- In `P0026`, two cylinders whose axes are parallel in `XY` (`(0, 0, -50)` radius `3` vs `(-1, -1.73205, -50)` radius `1`, center distance `√(1 + 1.73205^2) = 1.9999985200`, `r_a - r_b = 2.0`, gap `1.48 × 10^{-6}`) were rounded to 5 decimal places by the prospection minimizer (`docs/yang_tail_triage.md` & `CLAUDE.md` open tail), minting an unintended `1.48 µm` overlap strip bounded by two nearly coincident generator lines on endcap planes `z = ±50`.
- In our census below, `P0026` hits `triple_newton_refusal` (`10×` on `Plane × Cylinder × Cylinder` with `det = 0`) followed by `surface_pair_endpoint_mix` (`5×` on `Cylinder × Cylinder`, vertices `9, 15, 27, 105`) → `Stage4InvalidReason::LocalRefinementRequired` (loud `ERROR`, never silent `SUPPORTED_WRONG`). Because `P0026` is a 1D parallel-axis sub-band strip rather than an isolated 0D tangent point or interior small loop, `P0026` is expected to remain a loud `ERROR` unless its two-cylinder strip is refined below `1.48 µm`.

### 2.4 Non-crossing shell-contact fallback (`shell_contact.rs` `T6`) and pinch split (`stage4_correct.rs`)
- `shell_contact::contact_pairs` (`crates/yang-rs/src/shell_contact.rs:295-470`, deviation `D13` / `T6`) runs only when Stage 2 produced **zero** intersection edges globally (`n_arrangement_int_edges == 0`) or inside `failures::missed_intersection_sites`. It filters face pairs with `d_a + d_b > 0` by exact surface crossing `pieces_cross_point` (`:908`), triggering whole-face sagitta halving in `refine_loop` (`boolean.rs:622`). Because it refines entire faces iteratively until triangles cross rather than solving the loop by Newton + Bajaj tracing, `C0116` currently takes **`158.9 s`** in release!
- `split_pinch_vertices` (`crates/yang-rs/src/stage4_correct.rs`, spec `specs/yang_tangency_pinch_split.md`) splits degree-`2k` (`k ≥ 2`) figure-eight pinch vertices into manifold cycles once the tangent point `r_A = r_B = r` is present in the mesh arrangement.
- 0D external point contacts between disjoint solid interiors (`C0107` `Cylinder × Sphere`, `C0108` `Sphere × Sphere`, `C0109` internal kissing spheres) have no 1D curve neighbourhood; a single-shell B-Rep union pinched at one vertex is non-2-manifold (`assay_kv2.rs:1317` pins `C0107` as `Category::Error`).

---

## 3. Full 374-Case Corpus Census (`YANG_S43_TANGENCY_CENSUS`)

Measured on branch `agy/f4-n91-tangency` at base commit `d0bd7be4` using the canonical release corpus command:

```sh
~/.claude/bin/reserve -c 8 -l "agy-f4-n91 step1 census corpus" -- \
  env ASSAY_JOBS=8 ASSAY_CASE_TIMEOUT_SECS=900 \
  YANG_S43_TANGENCY_CENSUS=/home/claude/wt/agy-f4-n91-tangency/target/agy-logs/tangency_census.tsv \
  cargo test -p test-harness --test assay_kv2 --release full_corpus_categorized -- --ignored --nocapture \
  > target/agy-logs/corpus_step1.log 2>&1
```

Baseline corpus score: **`360C / 0W / 9E / 5EE / 0T` over 374 cases** (wall `927.32 s`, `253` probe events logged across the corpus):

| Probe event | Code site | Events | Cases | Case IDs & details |
|---|---|---:|---:|---|
| `pair_newton_refusal` | `stage4_relocate.rs:315` (`det <= rank_eps`) | **0** | **0** | *(none)* — `relocate_onto_implicit_pair` is only invoked on mesh-crossing edges already found by Stage 2, and `stage4_correct.rs:4496` skips torus non-curve own-vertices where `tangent_to` holds. |
| `pair_newton_max_iter` / `pair_newton_undef_normal` | `stage4_relocate.rs:300, 350` | **0** | **0** | *(none)* |
| `triple_newton_refusal` | `stage4_relocate.rs:398` (`\|det\| <= MIN_FEATURE_SIZE`) | **17** | **3** | `P0026` (×10, `Plane×Cylinder×Cylinder`, `ERROR`), `P0039` (×6, `Cylinder×Plane×Plane`, `SUPPORTED_CORRECT`), `P0054` (×1, `Plane×Plane×Cylinder`, `SUPPORTED_CORRECT`) |
| `triple_newton_max_iter` | `stage4_relocate.rs:427` | **28** | **3** | `P0022` (×2, `Cylinder×Cylinder×Plane`, `SUPPORTED_CORRECT`), `P0026` (×2, `Plane×Cylinder×Cylinder`, `ERROR`), `R0074` (×24, `Torus×Plane×Plane`, `SUPPORTED_CORRECT`) |
| `surface_pair_endpoint_mix` | `stage4_correct.rs:2415` (`relocate_onto_implicit_pair` failed on one endpoint of an edge) | **6** | **2** | `P0022` (×1, `Cylinder×Cylinder`, recovered via refinement → `SUPPORTED_CORRECT`), `P0026` (×5, `Cylinder×Cylinder`, `v=9,15,27,105` → `LocalRefinementRequired` `ERROR`) |
| `cyl_cyl_point_mint` | `boolean/tangency.rs:812` (`cyl_cyl_tangent_points` 0D point mint) | **16** | **5** | `C0058` (×4, `[0, ±0.4, 1]`), `F0056` (×4, `[0, ±0.3, 0]`), `F0057` (×4, `[±0.25, 0, 0]`), `F0058` (×2, `[0, ±0.2, 0]`), `F0060` (×2, `[±0.3, 0, 0]`) — **all 5 are Steinmetz equal-radius crossing-cylinder saddle tangencies (`SUPPORTED_CORRECT`) and are the direct corpus customers of Step 2 + Step 2b!** |
| `cyl_cyl_generator_mint` | `boolean/tangency.rs:736` (1D parallel generator tangency) | **6** | **3** | `C0043` (×2), `C0055` (×2), `C0056` (×2) — 1D line tangencies (`SUPPORTED_CORRECT`) |
| `cyl_cyl_crossing_ruling_mint` | `boolean/tangency.rs:782` (1D near-parallel grazing ruling) | **41** | **7** | `C0057` (×8), `F0041` (×8), `F0042` (×4), `F0043` (×8), `F0045` (×8), `P0043` (×4), `R0038` (×1) (`SUPPORTED_CORRECT`) |
| `contact_pair_crossing` | `shell_contact.rs:423` (`pieces_cross_point == Some`) | **39** | **15** | `C0116` (×2, `Cylinder×Cylinder` interior small loop, currently `158.9 s`), `F0064` (×7), `F0067` (×8), `P0021` (×2, `Cylinder×Cylinder` trim-clipped open arc), `P0049` (×3, `Cylinder×Cone`), `P0050` (×1), `R0004` (×1), `R0011` (×2), `R0019` (×3), `R0023` (×2), `R0032` (×1), `R0044` (×2), `R0051` (×1), `R0074` (×2), `R0085` (×2) |
| `contact_pair_noncrossing_touch` | `shell_contact.rs` (`d > 0`, `pieces_cross_point == None`, `pieces_may_touch == true`) | **100** | **34** | Includes 0D external kisses `C0107` (`Cylinder×Sphere`), `C0108` (`Sphere×Sphere`), sub-band micro-graze `C0118` (`Cylinder×Cylinder`, `1e-8` deep → `EXPECTED_ERROR`), `C0104` (`Sphere×Plane`), `P0026` (`Cylinder×Plane`), `R0085` (`Plane×Torus`), plus 28 planar/curved boundary-sharing near-touches where `GaussMap` + Case-IV Newton cleanly rule out interior small loops. |

### Key Takeaways from the Census
1. **Why `pair_newton_refusal` has 0 corpus hits today:** `relocate_onto_implicit_pair` is never called unless Stage 2 already produced a mesh-crossing edge, and non-crossing tangent contacts either get pre-seeded by the cylinder×cylinder closed form (`C0058`, `F0056`, `F0057`, `F0058`, `F0060`) or never cross at all (`C0107`, `C0108`, `C0116` on pass 0).
2. **Direct corpus customers per mechanism:**
   - **Step 2 + Step 2b (Newton tangency point resolution + retiring `cyl_cyl_tangent_points`):** `C0058`, `F0056`, `F0057`, `F0058`, `F0060` (all 5 equal-radius Steinmetz saddle-tangency cases must pass through general Newton tangency resolution + Stage-1 interior splice with `cyl_cyl_tangent_points` deleted).
   - **Step 3 + Step 4 (Gauss-map admission + Fig. 10(c) seeding + Newton seed + Bajaj small-loop tracing):** `C0116` (perpendicular `R=0.5` cylinders at offset `0.99`, strictly interior `0.01`-deep closed loop around `(0, 0.495, 1.0)`), plus non-overlapping Gauss-map pruning of false non-crossing `< 2d_ε` curved candidate regions across the 34 `contact_pair_noncrossing_touch` cases.
   - **Loud-error boundary invariants (`smoke_corpus_boundary_categories`):** `C0107` (`Cylinder×Sphere` 0D point contact → `ERROR`), `C0108` (`Sphere×Sphere` 0D point contact → `ERROR`), `C0109` (`Sphere×Sphere` internal 0D kiss → `ERROR`), `C0118` (`1e-8` sub-`MIN_FEATURE_SIZE` micro-graze → `EXPECTED_ERROR`), `C0111`/`C0113` (`EXPECTED_ERROR` / `ERROR` sub-band planes), `P0026` (minimizer-minted `1.48 µm` 1D parallel cylinder strip → `ERROR`).

---

## 4. Mathematical & Architectural Design

### 4.1 Pole-Free Local Parametric Surface Charts (`LocalSurfaceChart`)
Global parameterizations (`stage4_dt::eval_uv`) have coordinate singularities (`∂s/∂u = 0`) at sphere north/south poles `v = ±π/2` — right where an axis-aligned sphere–plane kiss (`C0104`, `C0107`, `C0108`, unit test sphere–plane kiss) occurs! To make Newton uniform across all five analytic `Surface` variants (`Plane`, `Cylinder`, `Cone`, `Sphere`, `Torus`) with no surface-pair branching:

- Construct a `LocalSurfaceChart` centered at seed point `p_0`:
  - **`Plane { point, normal }`:** orthonormal tangent basis `(e_u, e_v)` orthogonal to `normal`, origin at `p_0`'s projection onto the plane: `s(u, v) = o + u e_u + v e_v`, `n(u, v) = normal`, `∂n/∂u = ∂n/∂v = 0`. Nonsingular everywhere (`σ_1 = σ_2 = 1`).
  - **`Cylinder { point, axis, radius }`:** standard `(u, v) = (θ, z)` around `θ_0` (unwrapped continuously so `u - θ_0 ∈ (-π, π]` never wraps across `±π`): `s(u, v) = c + v a + R (cos u e_1 + sin u e_2)`, `‖∂s/∂u‖ = R`, `‖∂s/∂v‖ = 1`. Nonsingular everywhere for `R > 0`.
  - **`Cone { apex, axis, half_angle }`:** `(u, v) = (θ, t)` unwrapped around `θ_0` (`t > 0` axial distance from apex): `s(u, v) = apex + v (a + tan α (cos u e_1 + sin u e_2))`. Nonsingular everywhere away from the singular apex `v = 0`.
  - **`Sphere { center, radius }`:** choose local equatorial frame `(e_1, e_2, e_3)` with `e_1 = normalize(p_0 - center)`, `e_2 ⊥ e_1`, `e_3 = e_1 × e_2`! Then `(u, v) = (0, 0)` puts the seed on the **equator** `v = 0` where `cos v = 1`:
    `s(u, v) = center + R (cos v (cos u e_1 + sin u e_2) + sin v e_3)`, `n(u, v) = ±(cos v (cos u e_1 + sin u e_2) + sin v e_3)`.
    `‖∂s/∂u(0,0)‖ = R`, `‖∂s/∂v(0,0)‖ = R` — **completely pole-free for every point on the sphere!**
  - **`Torus { center, axis, major_radius, minor_radius }`:** `(u, v)` unwrapped continuously around `(u_0, v_0)`: `s(u, v) = center + (R + r cos v)(cos u e_1 + sin u e_2) + r sin v a`, `n(u, v) = cos v (cos u e_1 + sin u e_2) + sin v a`. Nonsingular everywhere on any regular torus (`R > r > 0`).

Every `LocalSurfaceChart` provides analytic first and second geometry (`s`, `∂s/∂u`, `∂s/∂v`, `n`, `∂n/∂u`, `∂n/∂v`) and fast exact inverse `project(p) -> (u, v)` in closed form.

### 4.2 Appendix C Parametric Pair Newton + Coupled Tangency Newton
1. **Appendix C minimum-norm parametric Newton (`solve_parametric_pair_newton`, `:479-492` R, `:1232-1250` R):**
   - Solves `D(u, v, s, t) = s_1(u, v) - s_2(s, t) = 0` using the minimum-norm step `Δ_k = -∇D(x_k)^+ D(x_k)` computed via thin SVD of `∇D` (`3 × 4`), which implements `∇D ∇D^T a_k = s_2 - s_1`, `x_{k+1} = x_k + ∇D^T a_k` (`:1246-1250` R) without catastrophic cancellation when `‖n_1 × n_2‖ → 0` (`det(∇D ∇D^T) = O(θ^2)`).
   - Backtracking line search ensures `‖D(x_{k+1})‖` decreases monotonically and chart coordinates stay bounded near the seed.
2. **Coupled tangency Newton (`solve_tangent_point_newton`, `:489-492` R, `:518-540` R, `:567-573` L):**
   - At an elliptic kiss (`z_1 - z_2 = κ u^2 / 2`), `D = 0` has multiplicity 2, so `‖D‖ < d_p = 10^{-7}` is satisfied as soon as `|u| ≈ √(2 d_p / κ) ≈ 4.5 × 10^{-4}`, where the normal angle `∠(n_1, ±n_2) ≈ 4.5 × 10^{-4}` rad still exceeds `10^{-6}` rad (`:745` R)! And at a hyperbolic (`X`-pinch) saddle tangency (`C0058` Steinmetz cylinders or torus inner-throat plane graze), two 1D curve branches cross at the tangent point so `D = 0` alone is underdetermined along the branches.
   - Solving the `5 × 4` coupled tangency system
     `F_tan(u, v, s, t) = [ s_1(u, v) - s_2(s, t) ; L_0 · (n_1(u, v) - σ n_2(s, t)) ; L_1 · (n_1(u, v) - σ n_2(s, t)) ] = 0`
     (where `σ = sign(n_1 · n_2) ∈ {+1, -1}` and `(L_0, L_1)` is an orthonormal basis of `n_2(s_k, t_k)^⊥`) via `5 × 4` thin-SVD minimum-norm Gauss–Newton (`Δ = -J_tan^+ F_tan`) resolves:
     - **0D elliptic kisses** (sphere–plane kiss, torus outer-equator graze, externally/internally kissing cylinders or spheres) quadratically (`rank = 4`) to `‖s_1 - s_2‖ < 10^{-14}` and `∠(n_1, ±n_2) < 10^{-13}` rad;
     - **0D hyperbolic saddle tangencies** (`C0058`, `F0056`, `F0057`, `F0058`, `F0060` crossing cylinders; torus inner-saddle plane graze) quadratically (`rank = 4`) to `~10^{-15}`;
     - **1D tangent curves** (torus bottom-circle plane graze `z = -r`, where `rank = 3` along the circle nullspace) quadratically onto the nearest point on the tangent locus;
     - while cleanly rejecting **separated non-intersecting surfaces** (`‖s_1 - s_2‖ = gap > d_p`) and **transversal small loops** (`C0116`, where at `n_1 ∥ n_2`, `‖s_1 - s_2‖ = 0.01 >> d_p`).
3. **Combined §4.3.3 classification (`solve_and_classify_seed`, `:535-540` R, `:567-573` L, `:745` R):**
   - Given seed `(p, q)` on `(S_A, S_B)`:
     1. Run `solve_parametric_pair_newton` (`D = 0`). If `‖s_1 - s_2‖ ≤ d_p` inside both trimmed face domains:
        - Check `∠(n_1, ±n_2)`: if `≤ 10^{-6}` rad, or if `solve_tangent_point_newton` from the converged/initial seed finds a valid in-domain tangent point within the region (`‖s_1 - s_2‖ ≤ d_p` and `∠(n_1, ±n_2) ≤ 10^{-6}` rad) when `D = 0` landed at the multiplicity-2 root, classify as **`TangentPoint(r)`** (at the polished machine-precision tangent point).
        - Otherwise (`∠(n_1, ±n_2) > 10^{-6}` rad and `D = 0` converged on a genuine non-collinear curve locus), classify as **`SmallLoopSeed(r)`**.
     2. For saddle-tangency discovery (where 1D curve branches also pass through `r`), `solve_tangent_point_newton` directly locates in-domain isolated tangent points `r` where `‖s_1 - s_2‖ ≤ d_p`, `∠(n_1, ±n_2) ≤ 10^{-6}` rad, and `rank(J_tan) == 4` (isolated 0D tangency, distinguishing 0D point tangencies from 1D parallel cylinder generators whose `J_tan` has `rank == 3`).

### 4.3 Trim-Boundary Decision for `N92` (`GaussMap` Admission on `C0116` vs `P0021`)
On parked branch `1519f6ed`, `GaussMap::overlaps` was wired into `shell_contact::contact_pairs` (`T6`), which also handles **open trim-clipped arcs** (`P0021`, where a small cylinder pokes through another cylinder's trim boundary with surface normals `61.9°`/`51.7°` apart and no collinear normal pair). That mis-placement forced an ad-hoc `trim_exempt` bypass (`N92` `PROPOSED`).

Wired in its **paper role** (§4.2.2 `:440-446` R + §4.3.3 `:567-573` L):
- `GaussMap::overlaps` is the admission test specifically for **non-crossing `< 2d_ε` small-loop / tangency candidate regions**, while `shell_contact::contact_pairs` continues to guard open trim-boundary crossings (`P0021`).
- By Sederberg et al. 1989 (`:442-446` R), **every closed intersection loop strictly inside both surfaces encloses a collinear normal pair `n_A = ±n_B`**, regardless of whether the surrounding coarse mesh triangles happen to touch the face's outer trim boundary!
- **Concrete verification on `C0116`:** In `C0116` (`Cylinder` along `Z`, `R=0.5`, height `[0, 2]` vs `Cylinder` along `X`, `R=0.5`, `y_0 = 0.99`), every coarse lateral triangle spans the full cylinder height `[0, 2]` and therefore has vertices on the trim rims `z = 0, 2` (so a `trim_exempt` rule would have exempted 100% of triangles!). Yet `GaussMap::from_triangles` over the 4 `< 2d_ε` triangles computes exact analytical azimuthal arcs covering `+Y` (`(0, +1, 0)`) on `A` and `-Y` (`(0, -1, 0)`) on `B`, so `g_A.overlaps(&g_B) == true` **with zero trim-boundary exemption**!
- Conversely, if Bajaj loop tracing starting from a seed exits either face's trimmed domain (an open boundary-clipped arc rather than a closed interior loop), loop tracing rejects it as non-closed and leaves open-arc handling to boundary intersection / `T6`.
- **Decision:** **No trim-boundary exemption is needed or added.** `GaussMap::overlaps` runs unconditionally on non-crossing small-loop candidate patches, resolving `N92` cleanly without a boundary-exemption deviation.

---

## 5. Increment & Commit Sequence

1. **Step 1 (this commit, docs-only):** `specs/yang_43_tangency_newton.md` — paper-to-code mapping, inventory of existing mechanisms, 374-case `YANG_S43_TANGENCY_CENSUS` results, and `N92` trim-boundary decision.
2. **Step 2 (tangency-point Newton resolution, `:489-492` R):**
   - Add `crates/yang-rs/src/stage4_tangency.rs` (`LocalSurfaceChart`, Appendix C `solve_parametric_pair_newton`, coupled `solve_tangent_point_newton`, `10^{-6}` rad classification), wire Stage-4 tangential fallback in `stage4_relocate.rs` and general Newton tangent-point discovery, register `YANG_S43_TANGENCY_CENSUS` in `docs/kernel_probe_registry.md`, and add unit pins in `crates/yang-rs/src/tests_unit/s43_tangency_newton.rs` (tangent cylinder pair, sphere–plane kiss, torus–plane graze, separated near-miss, transversal loop seed).
   - Run full 374-case release corpus; update `docs/yang_deviations.md` and `docs/audits/yang_2025_audit.md` in the same commit.
3. **Step 2b (retire Stage-1 `Cylinder × Cylinder` 0D point-tangent mint):**
   - Delete `cyl_cyl_tangent_points` from `crates/yang-rs/src/boolean/tangency.rs` so `C0058`, `F0056`, `F0057`, `F0058`, `F0060` (and `steinmetz_union_exact_volume`) are resolved solely by general Newton tangency discovery + mesh insertion (retaining 1D `mint_generator` and `mint_crossing_rulings`).
   - Run full 374-case release corpus; update `docs/yang_deviations.md` (`N82`, `N91`) and `docs/audits/yang_2025_audit.md` (`4.3-19`, `4.4-06`) in the same commit.
4. **Step 3 (small-loop detection + Bajaj tracing with `GaussMap` admission, `:440-446` R, `:567-573` L):**
   - Bring in `crates/yang-rs/src/gauss_map.rs` and `crates/yang-rs/src/tests_unit/s422_gauss_map.rs` from `1519f6ed` (primitive + small-loop admission pins), implement `< 2d_ε` non-crossing candidate component grouping, `GaussMap::overlaps` admission (no trim exemption), Bajaj et al. 1988 closed-loop marching + §4.3.4 refinement, and mesh loop insertion (`C0116`).
   - Run full 374-case release corpus; resolve `N92` and update matrix rows `4.2-12`, `4.2-13`, `4.3-18`, `4.3-20`, `5.6-01`, `5.6-02`, `M-4` in the same commit.
5. **Step 4 (Fig. 10(c) seeding & curve-redundancy filter, `:471-473` R, `:491-505` L):**
   - Wire exact Fig. 10(c) seed selection (`T` nearest on `M_A` to `M_B`, `p = centroid(T)`, `q = closest_on_M_B(p)`) + redundancy removal against already-solved intersection curves (`:502-505` L), with unit pins.
   - Run full 374-case release corpus; update `N91` and matrix rows `4.2-17..20`, `4.3-08`, `4.3-11..12`, `M-3` in the same commit.
