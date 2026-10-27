# Yang §4.1 `d_ε` from Model Bounding Box & Stage-1 `d(T)` Certificate (Roadmap F5: N85, N93)

## 1. Scope & Paper Specification

Roadmap milestone **F5** ("Stage 1 as §4.1", `docs/yang_faithfulness_roadmap.md:169-178`) starts with two items:

1. **Ledger `N85` (matrix `U-1`, row `4.1-18`):** select one operand-level discretization bound `d_ε = 10⁻² · d` from the diagonal `d` of the Axis-Aligned Bounding Box (AABB) of the B-Rep model, replacing per-rim and per-surface-kind chord-bound formulas.
2. **Ledger `N93` (matrix `M-6`, rows `4.1-01`, `4.1-24`, `4.1-26`, `A-01`):** compute `d(T)` at Stage 1 and produce the per-triangle Stage-1 bound `d_ε(T)` there (loud on any certificate failure, never silently patched).

### 1.1 Verbatim Paper Prescriptions (`refs/text/yang2025_hybrid_boolean.txt`)

- **§4.1.1 — Model-AABB `d_ε` (`:391-395`):**
  > "In practice, we select `d_ε` as a value `10⁻²d` relative to the diagonal length `d` of the Axis-Aligned Bounding Box (AABB) of the B-Rep model as a balance of efficiency and accuracy."

- **§4.1.1 & Appendix A — Inner sub-patch certificate (`:313-321`, `:348-369`, `:1163-1209`):**
  > "For each sub-patch `p(u, v)` with control points `P = p_{i,j}` where `i = 0, ..., m`, `j = 0, ..., n`, we create two triangles from the four corner control points, i.e. `△p_{00} p_{0n} p_{m0}` and `△p_{0n} p_{m0} p_{mn}`, as an initial discretization of the surface. If the distance from the Bézier patch to these two triangles exceeds `d_ε`, it is subdivided into four sub-patches until the tolerance is fulfilled, while the mesh is updated throughout." (`:316-321`)
  >
  > "**Theorem A.1.** The surface-to-mesh distance from `p(u, v)` to `M` is within a tolerance `d_ε`, if for each sub-patch `p_i(u_i, v_i) ⊆ p(u, v)`, there exists a planar convex polygon `P_i ∈ M` such that the distances between the control points of `p_i(u, v)` and `P_i` are within `d_ε`." (`:1163-1171`)
  >
  > "**Theorem A.2.** The surface-to-mesh distance from a rational Bézier sub-patch whose four corners are `v_0, v_1, v_2` and `v_3` to the triangle mesh `M` consists of two triangles `△v_0 v_1 v_3` and `△v_1 v_2 v_3` is within a tolerance `d_ε`, if the distances between the control points of the rational Bézier sub-patch and the planar quadrangle `□w_0 w_1 w_2 w_3` is within `ω`, `□w_0 w_1 w_2 w_3` is convex, and `ω + d < d_ε`, where `□w_0 w_1 w_2 w_3`, `d` and `ω` are defined as in Fig. 3." (`:1193-1201`)

- **§4.1.2 & Fig. 6 — Boundary retriangulation and per-triangle `d_ε(T)` (`:340-347`, `:354-390`):**
  > "For a boundary triangle `T`, we find the minimal rectangular region in the parametric domain that covers its three vertices. Then using the NURBS subdivision technique, we can obtain a subpatch defined on the rectangular region. We compute the maximal distance `d(T)` between the control points of the patch and the triangle as shown in Fig. 6. Thus, we re-define the discretization error bound `d_ε(T)` of a NURBS patch defined on a parametric triangle corresponding to the mesh triangle `T` as follows:
  > `d_ε(T) = d_ε` if `T` is an inner triangle, `d(T)` otherwise.
  > An inner triangle is a triangle that does not contain sampling points on the boundaries. For convenience, `d_ε(T)` will be abbreviated as `d_ε` in the following text, with the understanding that a different value is used when dealing with boundary triangles. The distance bounds are then used in our octree search structure as explained in Sec. 4.2.1 for a conservative intersection detection." (`:358-390`)

---

## 2. Inventory of Existing `d_ε` & `d(T)` Code Sites

All paths below are relative to `crates/yang-rs/src/`.

### 2.1 Stage-1 `d_ε` Producers (`stage1_tessellate/normals_chord_bounds.rs`)

| Symbol | Line | Formula today | Consumers |
|---|---|---|---|
| `CHORD_BASE` / `chord_rel()` | `:124`, `:152` | `1e-2 / chord_refine_scale()` (`1e-2` in `--release`) | Every `*_chord_bound` helper below |
| `curved_chord_bound(edges)` | `:234` | `1e-2 · diag(AABB(Circle rims))`, `None` if no `Curve::Circle` | Stage-1 circle pre-pass, cylinder chart budget, `owner_stage1_chord_budget`, Stage-4 `input_curved_chord_bound`, Stage-6 `tol_for`, `rim_junction.rs:236`, `boolean.rs:2340` |
| `ellipse_chord_bound(major_r)` | `:163` | `1e-2 · major_radius` (or `max(a, b)` for `Hyperbola`) | Stage-1 ellipse (`:1211`) & hyperbola (`:1336`) pre-passes; `ellipse_rim_chord_bound` (`:173`) |
| `sphere_chord_bound(radius)` | `:276` | `1e-2 · 2r√3` | `tessellate_sphere_face` (`surface_face_tessellators.rs:91`), Stage-3 SSI (`stage3_ssi.rs:863-865`), Stage-4 `input_curved_chord_bound` (`stage4_correct.rs:690`), Stage-6 `tol_for` (`boolean.rs:2835`) |
| `torus_chord_bound(R, r)` | `:293` | `1e-2 · (R + r)` | Patch-path `tessellate_torus_band` (`patch_tessellators.rs:1893`, `stage1_tessellate.rs:2396`, `self_contact.rs:320`), Stage-4 `input_curved_chord_bound` (`stage4_correct.rs:705`), Stage-6 `tol_for` (`boolean.rs:2907`) |
| `cone_chord_bound(h, α)` | `:309` | `1e-2 · √((2 h tan α)² + h²)` | Min-folded into Stage-1 circle `d_eps` (`stage1_tessellate.rs:616`), `cone_band_chord_bound` (`:336` → `stage3_ssi.rs:701`), `stage4_relocate.rs:1872`, Stage-6 `tol_for` (`boolean.rs:2872`) |
| `surface_pair_chain_bound` / `surface_pair_chord_bound` | `:407`, `:436` | `1e-2 · min(local curvature radius at endpoints)` | Stage-1 `SurfacePair` pre-pass (`stage1_tessellate.rs:1534`), `owner_stage1_chord_budget` (`:215`), Stage-4 `input_curved_chord_bound` (`stage4_correct.rs:714`) |
| `owner_stage1_chord_budget(brep)` | `:212` | `curved_chord_bound` ∥ `ellipse_rim_chord_bound` ∥ `surface_pair_chord_bound` | `stage3_ssi.rs:561` (`chord_tol_for_curved_owner`), `stage4_relocate.rs:1879` (`cone_chord_budget_from_owner`) |

### 2.2 Stage-1 Discretization & `d(T)` Sites

- **Boundary curve pre-passes (`stage1_tessellate.rs`):**
  - Circle shared `n_seg` (`:586-634`): `curved_chord_bound(edges)` min-folded over `cone_chord_bound` of circle-rimmed cones, sized so `max_r · (1 - cos(π / n_seg)) ≤ d_eps`.
  - Ellipse chains (`:1211`): per-edge `ellipse_chord_bound(major_radius) / k²`.
  - Hyperbola chains (`:1336`): per-edge `ellipse_chord_bound(semi_transverse.max(semi_conjugate)) / k²`.
  - `SurfacePair` chains (`:1534`): per-edge `surface_pair_chain_bound(a, b, p0, p1) / k²`.
- **Curved face tessellators:**
  - `Surface::Cylinder` (`stage1_tessellate.rs:2111, 2299-2315`): structured two-ring strip uses shared circle `n_seg`; chart CDT path seeds interior rings using `operand_chord_budget = curved_chord_bound ∥ ellipse_rim_chord_bound` divided by `k²`.
  - `Surface::Cone` (`surface_face_tessellators.rs:238`): structured frustum/apex-fan uses shared circle `n_seg`; chart CDT path passes `ChartBudget::default()` (boundary-only CDT, `N61` OPEN).
  - `Surface::Sphere` (`surface_face_tessellators.rs:90-108`): lat/lon grid with `s_arc = d_eps / 2 = sphere_chord_bound(r) / (2 k²)`.
  - `Surface::Torus` (`patch_tessellators.rs:47, 1893`): structured path uses shared circle `n_seg`; UV-CDT patch path uses `torus_chord_bound(R, r) / k²`.
- **Theorem A.1 `d(T)` primitive (`stage4_dt.rs:101`):**
  - `stage4_dt::d_of_t(surface, uv: [Point2; 3]) -> Result<f64, DtError>` computes the exact rational quadratic / bi-quadratic Bézier control net on `[u_0, u_1] × [v_0, v_1]` covering `uv` and returns the maximum point-to-triangle distance to `△(eval_uv(uv[0]), eval_uv(uv[1]), eval_uv(uv[2]))`.
  - Today called **only** in Stage 4 (`stage4_construct.rs:656, 764`, `stage4_boundary_curve.rs:4537`), never in Stage 1 (`N93`).

---

## 3. Step-1 Corpus Census (`YANG_S1_DEPS_CENSUS`)

### 3.1 Probe & Command

Registered print-only / JSONL-append probe `YANG_S1_DEPS_CENSUS=<path>` in `docs/kernel_probe_registry.md`, implemented in `crates/yang-rs/src/stage1_tessellate/deps_census.rs` and wired into `BRep::from_topology_with_rim_overrides` (`crates/yang-rs/src/brep.rs`) on initial operand constructions (`min_n_seg.is_none()` and empty standing overrides/refinements).

`brep_model_aabb(verts, edges, faces)` computes the exact closed-form AABB over:
1. All `BRepVertex` points.
2. All `BRepEdge` curves (`LineSegment`, full and open-arc `Circle` axis tangents, full and open-arc `Ellipse` exact stationary angles `atan2(w_k, u_k)`, `Parabola` stationary parameter, `Hyperbola` exact stationary parameters `atanh(-u_k / v_k)`, and projected `SurfacePair` chord bisection).
3. Doubly-curved `BRepFace` interiors (`Surface::Sphere` exact radial extrema on closed spheres and spherical caps, and `Surface::Torus` exact stationary `(θ, φ)` extrema on closed tori, seam-bounded structured laterals, Slice F-4 window-holed closed tori, and oriented universal-cover bands/disks).

Canonical corpus run executed in this worktree:

```text
~/.claude/bin/reserve -c 8 -l "f5 step1 census corpus" -- \
  env YANG_S1_DEPS_CENSUS=/home/claude/wt/agy-f5-n85-n93-deps/target/agy-logs/deps_census.jsonl \
  ASSAY_JOBS=8 ASSAY_CASE_TIMEOUT_SECS=900 \
  cargo test -p test-harness --test assay_kv2 --release full_corpus_categorized -- --ignored --nocapture \
  > target/agy-logs/step1_corpus.log 2>&1
```

- **Corpus verdict:** `360C / 0W / 9E / 5EE / 0T` over `374` cases (`finished in 1018.13s`, 0 verdict changes vs baseline; 9 ERROR cases: `R0023, F0064, C0046, C0107, C0108, C0109, C0111, C0113, P0026`).
- **Patch-path torus AABB verification:** re-ran `R0026`, `R0085`, `P0045` (`single_case --release`) after aligning patch-path torus loop orientation with `patch_tessellators.rs:980-1289`. Across all `2,814` operand records, `diag_mesh > diag_brep · (1 + 1e-9)` is **`0 / 2,814`**, with `(diag_brep - diag_mesh) / diag_brep` quantiles `min = 0.0`, `p50 = 0.0`, `p90 = 0.008337`, `p95 = 0.010129`, `max = 0.229581` (mesh vertices sit strictly inside the analytic curved-surface AABB by chord sagitta).

### 3.2 N85 Census: Today's `d_ε` vs Paper's B-Rep Model AABB `d_ε`

- **Total initial `BRep::new` operand records:** `2,814` across `361` boolean-invoking `ASSAY_CASE` ids (the remaining 13 of 374 cases are expected-error or fail upstream before `yang_rs::BRep::new`).
- **Operand classes:**
  - `1,672` all-planar operands (`has_curved_face == false && has_curved_edge == false`): today `d_eps_today_primary = None`; paper `d_eps_bbox = 10⁻² · d > 0` (planes have zero chord error regardless of `d_ε`).
  - `0` planar-faces-only operands with curved edges.
  - `1,142` curved-face operands (`Stage-1 ok = 2,814 / 2,814`).
- **Intra-operand multi-`d_ε` inconsistency today:** **`224 / 1,142` (`19.6%`)** of curved operands simultaneously use **2 or 3 distinct `d_ε` values** across their own edges and faces:
  - `active_bound_count = 3`: e.g. `R0028` (`today_min = 3.755e-5`, `today_max = 3.944e-4`, `d_eps_bbox = 5.649e-4`) and `R0032` (`today_min = 0.1520`, `today_max = 4.4280`, `d_eps_bbox = 0.8580`), mixing shared circle `d_ε`, patch-torus `10⁻²(R + r)`, and `SurfacePair` local-radius `d_ε`.
  - `active_bound_count = 2`: e.g. `R0006`, `R0009`, `R0017`, mixing circle-rim `d_ε` with ellipse/hyperbola `10⁻² max(a, b)`.

#### Ratio `d_eps_bbox / d_eps_today_primary` over all `1,142` curved operands

| Metric | Value |
|---|---|
| Count (`n`) | `1,142` |
| Equal (`\|ratio - 1\| ≤ 1e-6`) | `645` (`56.5%`) |
| Bbox **coarser** (`ratio > 1 + 1e-6`, looser budget / fewer segments) | `395` (`34.6%`) |
| Bbox **tighter** (`ratio < 1 - 1e-6`, stricter budget / more segments) | `102` (`8.9%`) |
| `min` | `0.107905` |
| `p05` | `0.747778` |
| `p10` | `1.000000` |
| `p25` | `1.000000` |
| `p50` (median) | `1.000000` |
| `p75` | `1.913633` |
| `p90` | `5.250081` |
| `p95` | `8.286355` |
| `max` | `15.418116` |

#### Breakdown by curved operand surface signature

| Surface signature | Operands (`n`) | `min` | `p25` | `p50` | `p75` | `max` |
|---|---:|---:|---:|---:|---:|---:|
| `Cyl` | `928` | `0.1731` | `1.0000` | `1.0000` | `1.1032` | `14.2409` |
| `Cone` | `98` | `0.1079` | `2.5937` | `3.9692` | `8.4039` | `15.4181` |
| `Torus` | `80` | `0.4510` | `0.8709` | `1.0418` | `1.2524` | `3.0160` |
| `Cyl+Cone` | `13` | `0.5867` | `8.1111` | `9.8977` | `10.3184` | `11.5936` |
| `Cone+Torus` | `9` | `0.1938` | `0.1938` | `0.2459` | `0.7331` | `0.7331` |
| `Cyl+Torus` | `7` | `1.0000` | `1.0129` | `1.0719` | `1.0767` | `1.0815` |
| `Sphere` | `7` | `1.0000` | `1.0000` | `1.0000` | `1.0000` | `1.0000` |

#### Extreme tails

- **Tightest under model AABB (`ratio < 1`):**
  - `P0016`: `ratio = 0.107905` (`today = 6.580593e-3`, `bbox = 7.100774e-4`) — small conic-trimmed cone patch (`2` ellipses, `6` hyperbolas, `0` circles) whose hyperbola semi-axis `max(a, b)` is ~9.3× larger than the trimmed body's AABB diagonal.
  - `P0029`: `ratio = 0.173142` (`today = 6.139476e-1`, `bbox = 1.063002e-1`) — obliquely trimmed cylinder patch bounded by `3` ellipse arcs (`major_radius` >> patch extent).
  - `R0032`: `ratio = 0.193773` (`today = 4.427984e0`, `bbox = 8.580256e-1`) — trimmed cone+torus patch where `torus_chord_bound(R, r)` / full cone height exceeds the trimmed solid's AABB.
- **Coarsest under model AABB (`ratio > 1`):**
  - `R0035`: `ratio = 15.418116` (`today = 1.585710e-3`, `bbox = 2.444866e-2`) — `224` small cone bevel/tooth faces on a large wheel (`cone_chord_bound` min-fold discretizes the entire wheel at one tooth's height; model AABB discretizes `15.4×` coarser).
  - `F0083`: `ratio = 14.240898` (`today = 2.542166e-3`, `bbox = 3.620273e-2`) — `449` planar faces + `1` small cylindrical hole (`curved_chord_bound` discretizes at the hole's AABB; model AABB discretizes `14.2×` coarser).

---

### 3.3 N93 Census: Stage-1 `d(T)` and Inner vs Boundary Triangles

Across all `2,814` initial operands, Stage 1 emitted **`3,269,201` curved triangles** (`88,329` inner triangles with all 3 vertices `TessellationSource::BRepFace`, and `3,180,872` boundary triangles with `≥ 1` vertex `BRepVertex` or `BRepEdge`):

- **`stage4_dt::d_of_t` reliability at Stage 1:** **`0` `DtError`s** across all `3,269,201` curved triangles (`max |eval_uv(uv) - tri_pt| = 1.467082e-09`, coming from `1e-9` position-key welding on `Torus` UV-CDT patches; `≤ 1.76e-16` on analytic primitives).
- **Boundary triangles (`3,180,872` triangles across all `1,142` curved operands):**
  - `max_dt_bdy / d_eps_today_primary`: `min = 1.7677`, `p05 = 4.5499`, `p25 = 7.2112`, `p50 = 10.4046`, `p75 = 13.1052`, `p95 = 20.7996`, `max = 55.4146`.
  - `max_dt_bdy / d_eps_bbox`: `min = 0.5755`, `p05 = 1.3685`, `p25 = 4.7298`, `p50 = 8.7033`, `p75 = 12.4676`, `p95 = 14.4519`, `max = 22.4768`.
  - Per Yang §4.1.2 (`:375-384`), boundary triangles are **not** required to satisfy `d(T) ≤ d_ε`; rather, `d_ε(T) := d(T)` **is** their defined Stage-1 error bound. Computing `d(T)` via `stage4_dt::d_of_t` on every curved boundary triangle at Stage 1 (with a loud `YangError` if `d_of_t` ever fails) is **100% output-neutral**.
- **Inner triangles (`88,329` triangles across `277` operands in `72` distinct corpus cases — `180` cylinder operands, `90` torus operands, `7` sphere operands):**
  - `max_dt_inner > face_today_d_eps` on **`277 / 277` (`100%`)** of operands with curved inner triangles (`min = 1.5562`, `p05 = 1.6101`, `p25 = 4.7767`, `p50 = 5.3363`, `p75 = 6.0588`, `p90 = 11.8996`, `p95 = 11.9434`, `max = 18.4613`).
  - `max_dt_inner > d_eps_bbox` on **`226 / 277` (`81.6%`)** of operands with curved inner triangles (`min = 0.2978`, `p25 = 1.2468`, `p50 = 3.0087`, `p75 = 5.7040`, `p90 = 7.4574`, `p95 = 8.7693`, `max = 9.6662`).

#### Why single-triangle `d_of_t(T)` exceeds `d_ε` on 100% of inner-triangle operands (and why inner certification belongs to F5 item 3 / `N5` / `N61`)

1. **Fig. 6 `d_of_t(T)` (`:358-368`) vs Fig. 3 / Theorem A.2 `ω + d` (`:348-369`, `:1193-1209`):**
   - Yang §4.1.1 discretizes an untrimmed surface into **rectangular Bézier sub-patches** `[u_0, u_1] × [v_0, v_1]`, splitting each 4-corner sub-patch into **two triangles** `T_1 = △v_0 v_1 v_3` and `T_2 = △v_1 v_2 v_3` (`:316-321`), and certifies the **pair** against their shared 4-corner planar convex quadrangle `□w_0 w_1 w_2 w_3` (`ω + d < d_ε`, Theorem A.2 `:1193-1209`), where `ω + d = O(κ Δs²)` is quadratic in step size.
   - By contrast, §4.1.2's `d_of_t(T)` (Fig. 6, `:358-368`) is defined **specifically for boundary triangles** ("For a boundary triangle `T`, we find the minimal rectangular region in the parametric domain that covers its three vertices..."). Applied to a single grid triangle `T_1 = △v_0 v_1 v_3`, its minimal covering rectangle `[u_0, u_1] × [v_0, v_1]` includes the **fourth corner `v_2 ∈ T_2 \ T_1`**, whose Euclidean distance to the bounded triangle `T_1` (clamped to `T_1`'s hypotenuse `v_1 v_3`) is half the quad diagonal `O(r Δu) = O(√(r d_ε)) ≫ d_ε`! That is why even on a unit `Sphere` (`7` operands), `d_of_t(T_1) = 7.4574 · d_ε`, and on structured `Torus` grids, `d_of_t(T_1) ≈ 11.94–14.03 · d_ε`.
2. **Current Stage-1 interior tessellators (`N5`, `N61`) also lack Theorem A.2 sub-patch sizing:**
   - Cylinder charts (`180` operands) and torus UV-CDT patches emit **unstructured Spade CDT** inner triangles (`4.1-16`), not structured 4-corner `[u_0, u_1] × [v_0, v_1]` sub-patches, so Theorem A.2 cannot pair them into quadrangles until F5 item 3 replaces interior unstructured CDT with structured domain subdivision + boundary-only CDT (`:404-407`).
   - Even on structured `Sphere` (`s_arc ≤ d_ε / 2`) and structured `Torus` (`s_arc ≤ d_ε`) grids (`N5`), rational quadratic Bézier middle control points sit at radius `r / cos α`, giving control-net offset `r(sec α - cos α) = (1 + sec α) s_arc > 2 s_arc`: Theorem A.2's `ω + d ≈ 1.87 · d_ε > d_ε` on `Sphere` and `≈ 2–4 · d_ε > d_ε` on `Torus`, while `Cone` charts (`N61`) pass `ChartBudget::default()` with no interior chord bound at all.

---

## 4. Step-2 Landed Implementation (`N93` partial — Stage-1 `d(T)` Certificate)

Landed in commit `e6932757`:

1. Extended `Stage1Tess` (`stage1_tessellate.rs`) with `pub(crate) tri_dt: Vec<f64>` (1:1 with `tris`) and `BRep` (`brep.rs`) with `tri_dt: Vec<f64>` + `pub(crate) fn tri_dt(&self) -> &[f64]` + `pub(crate) fn is_stage1_inner_tri(&self, tri_idx: usize) -> bool`, matching Yang §4.1.2 (`:354-390`) and Appendix A Theorem A.1 (`:1163-1171`):
   - `stage1_tessellate_once` calls `stage4_dt::d_of_t(&eff_surf, uv)` uniformly on **every** emitted Stage-1 triangle (`0.0` on `Surface::Plane`, positive finite bound on curved faces; zero new matches on surface type in `stage1_tessellate.rs`), raising a loud `YangError::MalformedTopology` on any `DtError` or `!dt.is_finite() || dt < 0.0` (verified on a double-nappe cone crossing the apex → `DtError::NegativeConeAxialRange`).
   - On boundary triangles (`≥ 1` vertex `BRepVertex` or `BRepEdge`), `tri_dt[t]` is `d_ε(T) = d(T)` verbatim (`:375-382`). On inner triangles (all 3 vertices `BRepFace`), `tri_dt[t]` provides the certified conservative Theorem A.1 upper bound until F5's generic-surface CDT item (`N5`/`N61`, `docs/yang_faithfulness_roadmap.md`) subdivides 4-corner inner sub-patches to satisfy Theorem A.2 (`ω + d < d_ε`).
2. `N93` stays **OPEN** (matrix `M-6`, `4.1-24` `ORDER` → `FAITHFUL`, `4.1-26` `MISSING` → `PARTIAL`, `A-01` updated). Canonical release corpus: `360C / 0W / 9E / 5EE / 0T` over `374` cases in `976.99s` (`0` moved cases).

---

## 5. Step-3 Empirical Trial (`d_ε = 10⁻² · d` Switch, `N85`) and Stop Diagnosis

### 5.1 Trial Scope

To measure `N85` directly on the full corpus without touching off-limits files (`shell_contact.rs`, `gauss_map.rs`, `stage4_*.rs`, `stage4_correct/*`), we wired `brep_curved_chord_bound(verts, edges, faces) -> Option<f64>` (`None` on all-planar straight-edged solids where discretization sag is identically `0.0`, and `brep_model_chord_bound` = `10⁻² · d` of `brep_model_aabb` whenever any curved face or curved edge is present) into:

- **Stage 1 (`normals_chord_bounds.rs`, `stage1_tessellate.rs`, `surface_face_tessellators.rs`, `patch_tessellators.rs`, `self_contact.rs`):** `owner_stage1_chord_budget`, shared circle `n_seg` (retiring the `cone_chord_bound` cross-patch min-fold at `stage1_tessellate.rs:607-616`), ellipse/hyperbola/`SurfacePair` pre-passes, cylinder chart budget, sphere grid budget, and patch-torus UV-CDT + `self_contact` budgets.
- **Stage 3 (`stage3_ssi.rs`):** `cone_chord_tol_for_owner` and sphere-arm selection tolerances (`chord_tol_for_curved_owner`).
- **Stage 5 (`stage5_output_refine.rs`, `stage5_topology/emit.rs`, `stage5_topology.rs`):** `restore_carried_edge_curves_with_bounds` so §4.4.2 carried-edge restoration checks sag against the owner's Stage-1 `d_ε` rather than `surface_pair_chain_bound` (verified on `kernel-v2` `kv9_cyl_cyl_special::unequal_perpendicular_union_reenters_with_far_pocket`: `1 passed; 0 failed; finished in 2.29s`).
- **Stage 6 (`boolean/rim_junction.rs`, `boolean.rs`):** `rim_junction` input budget and `tol_for` attribution/curve-restoration bands (replacing the 4-arm per-surface `tol_for` switch with `owner_stage1_chord_budget(owner_brep)`).

Off-limits **Stage-4 readback sites** left untouched during this trial (must move in lockstep with Stage 1 when `N85` lands after Stage-4 restructuring):
1. `stage4_correct.rs:680` (`input_curved_chord_bound`, which reads `curved_chord_bound`, `sphere_chord_bound`, `torus_chord_bound`, `ellipse_rim_chord_bound`, `surface_pair_chord_bound`).
2. `stage4_relocate.rs:1856` (`cone_chord_budget_from_owner`, which checks `cone_band_chord_bound` before `owner_stage1_chord_budget`).
3. `stage4_correct/domain_postcondition.rs:12` (`face_chord_bound` / `domain_census.rs:864`).

### 5.2 Canonical Corpus Result & Moved Cases

Canonical release corpus run (`target/agy-logs/step3_corpus.log`, `ASSAY_JOBS=8`, `ASSAY_CASE_TIMEOUT_SECS=900`):

- **Score:** `356 SUPPORTED_CORRECT / 0 SUPPORTED_WRONG / 13 ERROR / 5 EXPECTED_ERROR / 0 TIMEOUT` over `374` cases (`finished in 1115.26s`), vs Step-2 baseline `360C / 0W / 9E / 5EE / 0T`.
- **Moved cases (`4` `SUPPORTED_CORRECT → ERROR` regressions, `0` `ERROR → SUPPORTED_CORRECT`, plus `1` `ERROR → ERROR` detail shift on `F0064`):**

| Case | Verdict move | Lineage (`.meta.json`) | Census `d_ε` shift (`deps_census_final.jsonl`) | Failure & structural blocker |
|---|---|---|---|---|
| `F0084` | `SUPPORTED_CORRECT → ERROR` (`24.1s → 5.9s`, `Extrude 14`) | `20 ops, off-axis chained extrudes (0-5° tilt/step) — seed 20009` | Accumulated body (`op#24..#36`): `719..796` `Plane` + `1` small `Cylinder`; `d_eps_today = 5.025380e-3` (rim AABB) vs `d_eps_bbox = 3.2577e-2 .. 4.0765e-2` (**`6.48×–8.11×` coarser**; `7.5569×` at `op#32` = `Extrude 14`) | `Stage-4 relocation region around vertex 4294967295 is invalid: LocalRefinementRequired` — `7.56×` coarser cylinder chords hit a T1/carrier failure requiring **F3**'s `§4.5.2` local refinement loop (`N84`/`N62`), compounded by untouched `stage4_correct.rs:685` keeping `stage4_chord_band = 5.025e-3`. |
| `P0007` | `SUPPORTED_CORRECT → ERROR` (`23.7s → 0.9s`, `Extrude` union) | `prospector seed 1 index 16 (already minimal, step 1 rounded): two circle bosses, a non-convex hexagon boss on an oblique plane, a square extrude-union` (`×1e3` identical) | Re-entering body `op#5` (`19 Plane + 2 Cylinder`, `16 Circle + 11 Ellipse + 41 SurfacePair`): today mixed `1.78e-6` (`SurfacePair`), `~1.0e-5` (`Ellipse`), `1.9317e-5` (`Circle`); `d_eps_bbox = 2.4295e-5` (**`13.65×` coarser** on `SurfacePair` chains, `1.26×` on circles) | `Stage-4 relocation region around vertex 89 is invalid: RelocationCrossedCarrierVertex` — `13.65×` coarser re-entry chains cause Stage-4 relocation to overtake neighbour vertex `89`; needs **F2** (`§4.4.1` `update_patch` merge, `N2`) + **F3** (`§4.5.2` local refinement, `N62`/`N84`). |
| `P0046` | `SUPPORTED_CORRECT → ERROR` (`2.2s → 0.9s`, `Extrude` union) | `prospector seed 4 index 108, minimized 6->4 ops (convex4:boss convex7:rev convex3:cut circle:boss)`; un-minimized 6-op lineage had identical failure class (`×1e3` preserved) | `op#0` (`2 Plane + 7 Cone` revolve): `N88` `cone_chord_bound` min-fold gave `0.139186` vs `d_eps_bbox = 0.861117` (**`6.1868×` coarser**); re-entering cone+hyperbola operands `op#3,4,5,7` (`30–58 Hyperbola`): `0.292676 → 0.608509..0.704084` (**`2.08×–2.41×` coarser**) | `TessellationFailed { face: FaceId(84), reason: "ring rejected by CDT (degenerate/self-intersecting)" }` — directly confirms roadmap **F5's ordering note**: retiring `N88`'s `cone_chord_bound` min-fold exposes a self-crossing output face loop before **F2** (`update_patch`) + **F3** (`N83` §4.5.4 curved-face loop-crossing detection + local refinement) absorb it. |
| `R0032` | `SUPPORTED_CORRECT → ERROR` (`8.6s → 20.0s`, `boolean_subtract`) | `3 ops, scale=2.33e2, revolve(circle,boss)+revolve(gear,cut)+extrude(circle,boss)` (torus boss − 588-cone gear revolve + cylinder boss) | `op#1` (`2 Plane + 588 Cone`, `96,428` tris today): `N88` `cone_chord_bound` min-fold gave `1.025376` vs `d_eps_bbox = 8.414580` (**`8.2063×` coarser**); re-entering torus+cone operands `op#2,4,5,7,8,12`: `SurfacePair` `0.151953 → 0.580042..0.858026` (**`3.82×–5.65×` coarser**) | `TessellationFailed { face: FaceId(691), reason: "patch triangulation folded (inverted triangle) — KV9-F2..." }` — same **F5 → F2/F3** dependency (`N88` cone min-fold + `surface_pair_chain_bound` over-discretization currently shielding a coarse gear-tooth patch from folding until **F2**/`N83` **F3** land). |
| `F0064` | `ERROR → ERROR` (error text shifted at `Extrude 4`) | `10 ops, off-axis chained extrudes` (existing open ERROR) | `op#4` (`611 Plane + 1 Cylinder`): `d_eps_today = 5.428232e-3` vs `d_eps_bbox = 1.390335e-2` (`2.5613×` coarser) | Moved from `§4.5.2 local refinement ended with 10 unresolved failure site(s)...` to `Stage-4 relocation region around vertex 1278 is invalid: OffCurveBeyondChordBand` because untouched `stage4_correct.rs:685` still computes `stage4_chord_band = 5.428e-3 < 1.390e-2`. |

Per the task stop rule and roadmap F5's ordering note (`docs/yang_faithfulness_roadmap.md:170-171`), the uncommitted Step-3 code switch was reverted cleanly back to Step 2 (`e6932757`) with zero ad-hoc arms, bands, or knobs added.

### 5.3 Classification of the 6 `yang-rs --lib` Unit-Test Failures Under Step 3 (FORMULA vs BEHAVIOUR)

In `target/agy-logs/step3_gate.log`, `cargo test -p yang-rs --lib` ran `1,089` tests (`1,081 passed; 6 failed; 2 ignored`). Every pure **BEHAVIOUR** test (including `n71_an_all_planar_owner_keeps_the_loud_producer_fault`, `the_driver_refines_the_torus_patch_until_its_chords_clear_the_fin`, and corpus `P0016` `SUPPORTED_CORRECT`) passed; all **6** failures pin pre-`N85` **FORMULAS** (or a formula-derived polygon-count constant):

1. `tests_unit::n72_cone_band_readback::n71_conic_bounded_cone_band_has_no_own_rim_but_the_owner_has_a_budget` (`tests_unit/n72_cone_band_readback.rs:288`): **FORMULA** — asserts `owner_stage1_chord_budget(&brep) == Some(ellipse_chord_bound(major_radius))` (`0.12247448713915893`), whereas the conic-bounded cone patch's model-AABB bound is `0.35898756664358805`. Update to `brep_curved_chord_bound` when `N85` lands.
2. `tests_unit::n72_cone_band_readback::n71_stage3_cone_selection_band_reads_the_owner_budget_back` (`tests_unit/n72_cone_band_readback.rs:306`): **FORMULA** — asserts `cone_chord_tol_for_owner(...) == ellipse_chord_bound(major_radius)` instead of `owner_stage1_chord_budget(&brep).unwrap()`. Update to the paper's model-AABB owner budget when `N85` lands.
3. `tests_unit::n72_cone_band_readback::n71_stage4_cone_relocation_budget_reads_the_owner_budget_back` (`tests_unit/n72_cone_band_readback.rs:328`): **FORMULA** — asserts `cone_chord_budget_from_owner(...) == ellipse_chord_bound(major_radius)` instead of `owner_stage1_chord_budget(&brep).unwrap()` (`stage4_relocate.rs:1879` already delegates to `owner_stage1_chord_budget`). Update to the model-AABB owner budget when `N85` lands.
4. `tests_unit::n72_cone_band_readback::n71_a_circle_rimmed_cone_band_still_takes_its_own_per_band_bound` (`tests_unit/n72_cone_band_readback.rs:416`): **FORMULA** — asserts `cone_chord_tol_for_owner` returns the N38 2D axial cross-section formula `cone_band_chord_bound` (`10⁻² · √((2R)² + H²) = 0.1527525231651947`) rather than the 3D cone model-AABB diagonal `10⁻² · √((2R)² + (2R)² + H²) = 0.19148542155126766`. Retiring per-band `N38`/`N88` cone bounds in favour of the model AABB is `N85`'s exact specification; update to `owner_stage1_chord_budget(&brep)` when `N85` lands.
5. `tests_unit::s1_self_contact::a_torus_skin_thinner_than_its_chord_sag_reports_the_fin_with_a_halved_bound` (`tests_unit/s1_self_contact.rs:527`): **FORMULA** — asserts `report.face_bounds.get(&0) == Some(torus_chord_bound(TR, TR_MINOR) / 2.0)` (`0.375`), whereas halving the fixture's model-AABB bound gives `0.23983356174283457` (while its companion **BEHAVIOUR** test `the_driver_refines_the_torus_patch_until_its_chords_clear_the_fin` at `:540` passes). Update `own` to the model-AABB bound when `N85` lands (before F5's final item retires `self_contact` altogether).
6. `tests_unit::boolean_functional::planar_full_ellipse_cap_reenters_stage1` (`tests_unit/boolean_functional.rs:2537`): **FORMULA-derived threshold** — on a standalone `2 × 1` planar ellipse disk (`AABB = [-2, 2] × [-1, 1] × [0, 0]`, analytic area `2π = 6.283185307179586`), `ellipse_chord_bound(a) = 10⁻² · 2 = 0.02` gave `n_seg = 23` (`area = 6.2054 > 0.985 · 2π`), whereas the paper's `d_ε = 10⁻² · √(4² + 2²) = 0.0447213595` selects `n_seg = 15` (`a(1 − cos(π/15)) = 0.0437096 ≤ d_ε`), whose exact inscribed 15-gon area is `15 · ½ · 2 · 1 · sin(2π/15) = 6.101049646137002 = 0.971012 · 2π`. The re-entry CDT succeeds, every chord satisfies sag `≤ d_ε = 10⁻² · d`, and only the hardcoded `0.985 * analytic` constant (assuming `n_seg ≥ 20` from `10⁻² · a`) fails; update the area bound to the inscribed polygon area implied by `d_ε = 10⁻² · d` when `N85` lands.

### 5.4 Note on Reopened `D14` Generic Parametric-Surface Interface (`main` F5 item 3)

On `main` (`docs/yang_faithfulness_roadmap.md` F5), `D14` was reopened on 2026-10-27 to introduce a single generic parametric-surface interface before replacing Stage 1's per-surface tessellators. Commit `e6932757` added zero surface-type matches in `stage1_tessellate.rs`; the two analytic dispatch helpers introduced in `stage1_tessellate/deps_census.rs` during Step 1 (`brep_model_aabb` over `Surface::Sphere`/`Torus` interiors and `stage1_tri_surface_and_uv` inverse `(u, v)` projection) should be included in F5 item 3's surface-match retirement inventory so both delegate to the generic surface trait (`domain_aabb` / closest-point `(u, v)` projection).
