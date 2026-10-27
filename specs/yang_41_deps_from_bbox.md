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

## 4. Implementation Plan (Steps 2 & 3)

### Step 2 (`N93` partial — output-neutral Stage-1 `d(T)` & `d_ε(T)` table)

1. Extend `Stage1Tess` (`stage1_tessellate.rs`) with `pub(crate) tri_d_eps: Vec<f64>` (1:1 with `tris`) and `BRep` (`brep.rs`) with `pub(crate) tri_d_eps: Vec<f64>` + `pub(crate) fn tri_d_eps(&self) -> &[f64]`, matching Yang §4.1.2 (`:375-382`):
   - `Surface::Plane`: `0.0`.
   - Curved **boundary** triangle (`≥ 1` vertex is `BRepVertex` or `BRepEdge`): `stage4_dt::d_of_t(&eff_surf, uv)` — **loud `YangError::MalformedTopology`** if UV recovery fails, `d_of_t` returns `Err(DtError)`, or `!dt.is_finite() || dt < 0.0`.
   - Curved **inner** triangle (all 3 vertices `BRepFace`): the face's `d_ε` in force (`face_d_eps`).
2. Keep `N93` **OPEN** (amending `N93` and matrix `M-6`, `4.1-24`, `4.1-26`), recording that Stage-1 boundary `d(T)` is now computed and checked on every Stage-1 pass while inner Theorem A.2 `ω + d < d_ε` certification is gated on F5 item 3 (`N5` / `N61`). Never invent a slack multiplier (`2×` or `20×`) to fake an inner certificate.

### Step 3 (`N85` — switch `d_ε` to `10⁻² · d` of the B-Rep model AABB)

1. Wire `brep_model_chord_bound(verts, edges, faces)` (`10⁻² · d` of `brep_model_aabb`) as the operand's single `d_ε` across Stage 1 and non-Stage-4 readback helpers (`normals_chord_bounds.rs`, `stage1_tessellate.rs`, `surface_face_tessellators.rs`, `patch_tessellators.rs`, `self_contact.rs`, `stage3_ssi.rs`, `boolean/rim_junction.rs`, `boolean.rs`), preserving `None` on all-planar operands where downstream curved-band gates check whether an operand carries any curved surface/edge, while avoiding edits to `shell_contact.rs`, `gauss_map.rs`, and `stage4_*.rs` / `stage4_correct/*`.
2. Run unit/integration tests and the canonical 8-job release corpus.
3. **Stop rule (`docs/yang_faithfulness_roadmap.md:170-171`, P9/P10):** name and anchor every moved case. If any `SUPPORTED_CORRECT` case regresses (e.g. among the `395` operands where `d_eps_bbox` is up to `15.4×` coarser than today before F3's local refinement loop absorbs them, or from Stage-4 readback mismatch while `stage4_*.rs` is off-limits), **STOP** without compensating via any arm, band, floor, or knob: revert the uncommitted Step-3 code switch, commit the findings in this spec and `docs/yang_deviations.md` (`N85`), and report.
