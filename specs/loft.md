# Loft (B-Series Modeling Feature)

**Status:** Draft — design specification for user sign-off prior to implementation

> **Reviewer amendment (2026-10-27, after this draft was written):** the user
> REOPENED D14 the same day (`docs/yang_deviations.md` D14 "Decision"; commit
> cc2d4f30). D14 is now OPEN and is resolved by roadmap **F5**, not F7: F5
> builds Stage 1 and every later stage on a generic parametric-surface
> interface, with the five analytic surfaces as its instances, and carries a
> first non-analytic (ruled / B-spline) instance end to end. So Option (b)
> below is no longer "a multi-week overhaul after F7" — it is F5's last item,
> and loft booleans (L7) ride on it. Option (a) remains the v1 plan until F5
> lands. Two consequences for this spec:
> 1. **One B-spline implementation, shared.** The P&T algorithms of L1 must
>    live where both kernel-v2 and yang-rs can use them — `crates/cad-primitives`
>    (shared geometry, below both in the layering) — not in
>    `kernel-v2/src/geom/bspline.rs`, so that F5's non-analytic instance and
>    loft's surfaces are the same code. `waffle_types::bspline` (the endpoint
>    clamp noted in §2.4) is retired into it, not kept beside it.
> 2. §3.3's stage table is the F5 inventory's starting point: its count of
>    45 non-test yang-rs files matching on `Surface::*` / `Curve::*` is what
>    F5's interface must absorb.
**Branch:** `agy/loft-spec` (code citations anchored at HEAD `b4bee54ee4104ef7cf23a29f6cf4c5845bfff0f4`)
**Context:** `specs/custom_features_and_modeling_roadmap.md` (§5.1 "What's left" — Loft is the remaining classic constructor after B2 `Pipe` and B6 `Sweep`), `specs/b6_general_sweep.md`, `specs/surface_type_taxonomy.md` (§Lofted Surface, §Ruled Surface), `docs/yang_deviations.md` (`D14`, `N94`), `docs/yang_faithfulness_roadmap.md` (Milestone F7).
**Primary literature:**
- Les Piegl & Wayne Tiller, *The NURBS Book*, 2nd ed., Springer, 1997 — `refs/text/piegl_tiller1997_nurbs_book.txt` (§4.5 `RatSurfaceDerivs`, §5.2–§5.5 knot refinement / Bézier decomposition / degree elevation, §8.2 bilinear patches, §8.4 ruled surfaces, §9.2.1 global curve interpolation, §10.3 skinned surfaces).
- Ian Stroud, *Boundary Representation Modelling Techniques*, Springer, 2006 — `refs/text/stroud2006_brep_modelling_techniques.txt` (§13.3 Lofting: topological assembly, normalized `[0, 1]` arc-length vertex matching, start-vertex & direction twist control).
- Yingyu Yang, Xiaohong Jia, Bolun Wang, Jieyin Yang, Shiqing Xin, Dong-Ming Yan, *Boolean Operation for CAD Models Using a Hybrid Representation*, ACM TOG 44(4), Art. 114, 2025 — `refs/text/yang2025_hybrid_boolean.txt` (§4.1–§4.5, Appendices A–C).

---

## 1. Goal, CAD Survey & User-Facing Scope

### 1.1 Goal

Give Waffle Iron a **solid Loft constructor** (`Operation::Loft` / `Kernel::loft`) that blends an ordered sequence of `m ≥ 2` closed planar cross-sections (`C_0, C_1, …, C_{m-1}`) lying on distinct sketch planes (parallel or non-parallel) into a single manifold B-Rep solid:

- When a lofted face is **algebraically exact** as a plane, right circular cylinder, or right circular cone (e.g. parallel homothetic polygons, frusta, wedges, coaxial circles), the constructor emits `Surface::Plane`, `Surface::Cylinder`, or `Surface::Cone` directly so existing analytic downstream operations (booleans, measurements, drawings, exact STEP elementary surfaces) apply immediately.
- When a lofted face is genuinely freeform or doubly-ruled non-planar (e.g. twisted prism, non-parallel skewed quad → hyperbolic paraboloid, oblique circular frustum, circle-to-rectangle transition, or smooth `m ≥ 3` cubic skinning), the constructor emits an exact tensor-product rational/polynomial B-spline surface (`Surface::BSpline`) bounded by exact isoparametric curves (`Curve::Line`, `Curve::CircleArc`, or `Curve::BSpline`) — **never** a silent planar faceting (`governance/ENGINEERING_CONSTITUTION.md` P8, P9, P10; `governance/ARCHITECTURAL_INVARIANTS.md` A15.1–A15.3).

### 1.2 Survey of Existing CAD Systems

| Capability | SolidWorks (`Lofted Boss/Base`) | Onshape (`Loft`) | FreeCAD (`PartDesign::AdditiveLoft` / `Part::Loft` via OCCT `BRepOffsetAPI_ThruSections`) | Proposed Waffle Iron **v1** |
|---|---|---|---|---|
| **Ordered sections (`m ≥ 2`)** | 2+ planar sketches, planar faces, or 3D curves (parallel or oblique planes) | 2+ sketch regions, faces, or points | 2+ closed wires/sketches on any planes | **Yes:** `m ≥ 2` closed profiles/regions from distinct sketches (standard, `DatumPlane`, or face-hosted local planes) |
| **Open vs. closed Solid** | Solid (closed profiles) or Surface (open wires); **Closed Loft** checkbox (`m ≥ 3`) wraps last→first into a ring | Solid / Surface tabs; **Make periodic** (`m ≥ 3` closed loop, no caps) | `Solid: bool` (adds planar end caps); `Closed: bool` (`m ≥ 3` periodic ring, no caps) | **Solid only** (closed profiles required; open-wire sheet lofts refused). **`closed: bool`**: `false` (`m ≥ 2`, two planar end caps) or `true` (`m ≥ 3`, periodic closed ring with zero end caps) |
| **Ruled vs. smooth** | Smooth cubic spline by default; `Centerline` / `Start-End None`; ruled via `Ruled Surface` or 2-section loft | Smooth cubic (`m ≥ 3`), linear ruled when `m = 2` (or polyline path) | Explicit **`Ruled: bool`** (`true` = piecewise ruled `C^0` across sections; `false` = smooth B-spline up to `MaxDegree`, default 5) | **Yes:** **`ruled: bool`** (default `false`: smooth cubic `q = min(3, m - 1)` for `m ≥ 3`, exact linear ruling `q = 1` for `m = 2`; `ruled: true` forces piecewise degree-1 ruling across every adjacent pair) |
| **Equal vs. unequal segment counts** | Equal counts map 1-to-1; unequal counts slice via synchronization points or merge into one smooth spline face | Equal counts map vertex-to-vertex; unequal counts split by normalized arc-length ratio unless manual `Connections` are added | OCCT splits all wires when segment counts match; when counts differ, re-approximates / splits by arc-length ratio | **Yes (staged `L2` → `L4`):** equal counts map edge-for-edge (`L2`–`L3`); unequal counts (`L4`) co-refine by normalized `[0, 1]` arc-length parameters (Stroud §13.3), splitting circles into 4 quadrant arcs when paired with a `4k`-gon or `N` equal arcs for an `N`-gon |
| **Twist / correspondence fix** | Draggable pink/green **Connector** dots on profile vertices + context-menu `Reset Connectors` / `Show Next Connector` + `Reverse Direction` | **Connections** sub-panel: pick one vertex/edge per profile to synchronize seam + flip loop orientation | **Sketch orientation** & first-vertex order (historically required manual sketch re-indexing; newer builds expose vertex mapping) | **Yes:** automatic minimum-chord-sum start-vertex & CCW alignment default, overridden per section by **`start_vertex: Option<usize>`** (shifts connector index) and **`reverse: bool`** (flips loop traversal for deliberate Mobius-free twists or untwisting) |
| **End conditions (`G^1` / `G^2`)** | `None`, `Direction Vector`, `Normal To Profile`, `Tangency To Face`, `Curvature To Face` + tangent length weight | `None`, `Normal to profile`, `Tangent to face`, `Match tangent`, `Match curvature` + magnitude | Not in `AdditiveLoft` (`Part::Loft` is unconstrained interpolation) | **Deferred (v2):** P&T §10.3 Eqs. `(10.9)`–`(10.13)` support prescribed end tangents, reserved for v2 |
| **Guide curves & centerline** | 1+ 3D guide rails (`Influence` modes) + centerline | `Guides` (pierce profiles) + `Path` spine (`Sections` count) | `Part::Sweep` with multisection / `Pipeshell` (separate from `AdditiveLoft`) | **Deferred (v2):** Gordon/Coons bi-directional skinning with `Sketch3d` rails |
| **Point / apex loft** | Sketch point allowed at start and/or end (cone / pyramid) | Vertex allowed at start/end | Single vertex allowed at first/last section | **Deferred (v2):** degenerate collapsed-edge fan (`P_{i,0} = P_{apex}`) |
| **Inner loops (hollow loft)** | Supported via thin-feature or nested sketch contours in single-loop pairs | Supported when every profile has matching outer + inner loop counts | Supported in `PartDesign::AdditiveLoft` when every section has the same number of inner wires (`1:1` wire order) | **Single-hole hollow in `L4`** (`holes.len() ≤ 1`, equal across all sections); multi-hole `holes.len() ≥ 2` refused typed in v1 |

### 1.3 Minimal Useful v1 Contract & Explicit Non-Goals

Following `specs/b6_general_sweep.md` §1–§2, v1 defines a crisp, honest capability boundary where every out-of-scope input fails with a loud typed `KernelError::NotSupported` or `OpError::InvalidParameter` (`governance/ENGINEERING_CONSTITUTION.md` §3.1 P10):

1. **Sections (`sections.len() >= 2` for open, `>= 3` for `closed: true`):**
   - Each section references a planar `Sketch` feature (`sketch_id`) and either a resolved `ProfileRegion` (`region`, supporting sub-regions and single-hole annular footprints just like `ExtrudeParams` / `SweepParams` at `crates/feature-engine/src/types.rs:942-960, 1340-1356`) or `(profile_index, profile_entity_ids)`.
   - Two consecutive sections on the **exact same plane** (coplanar with zero normal separation and overlapping footprints) or a section whose plane intersects the adjacent section curve so rulings fold back (`(P_{k+1}(u) - P_k(u)) · axis_avg <= 0` everywhere) are refused with `InvalidParameter` / `KernelV2Error::LoftSelfIntersecting`. Non-parallel planes (e.g. a duct bending `30°` across two `DatumPlane`s) are **supported** whenever consecutive sections do not cross along their rulings.
2. **Closed profiles only:**
   - Open chains (`Sketch3d` or unclosed 2D wires) are refused (`InvalidParameter`). Waffle Iron's `Kernel` trait models closed 2-manifold solids (`crates/waffle-types/src/kernel/traits.rs:21-25`).
3. **Vertex/segment correspondence & twist control:**
   - Every section loop has a canonical counter-clockwise orientation (`twice_area > 0` in `(plane_origin, plane_normal, plane_x_axis)`, matching `resolve_sweep_section` at `crates/feature-engine/src/rebuild.rs:1587-1596`).
   - By default (`start_vertex: None, reverse: false`), section `0` keeps its canonical vertex `0` and CCW sense; each subsequent section `k` aligns its orientation so its plane normal points consistently along the section-centroid progression `c_{k} - c_{k-1}` (preventing inside-out hourglasses when two sketch planes have opposite normals!) and selects the cyclic vertex shift `s_k ∈ 0..M` minimizing total squared chord length `\sum_{j=0}^{M-1} \|P_{k, (j + s_k) \bmod M} - P_{k-1, j}\|^2`.
   - As Stroud notes (`refs/text/stroud2006_brep_modelling_techniques.txt:14674-14681`):
     > *"Sometimes this can only be done with user assistance, maybe to adjust the topology of the sections manually and to help pick matching starting points and directions. This cannot really be done automatically. Suppose that a twisted object such as that in figure 13.24 is what is wanted... All sections are the same shape and size, and an automatic method would not be able to tell what the user really wants."*
   - Accordingly, each `LoftProfileRef` carries `start_vertex: Option<usize>` (an explicit connector vertex index in `0..vertex_count` overriding the auto-minimizer, allowing deliberate `90°`/`180°` twists or manual untwisting) and `reverse: bool` (toggling loop sense on top of normal alignment; when one section flips sense relative to its neighbour without compensation, rulings cross in an hourglass and self-intersection validation rejects the solid loudly).
4. **Open vs. closed loft (`closed: bool`):**
   - `closed: false` (default): builds lateral faces from section `0` through `m - 1`, capped by two planar end faces (`Role::EndCapNegative` at section `0` with outward normal opposing the inward loft tangent, `Role::EndCapPositive` at section `m - 1` with outward normal along the outward loft tangent — matching `assign_sweep_roles` at `crates/modeling-ops/src/sweep.rs:97-147`).
   - `closed: true` (requires `m ≥ 3`): wraps section `m - 1` back to section `0` periodically (`C_m = C_0`) with **zero end caps** (`G ≥ 1` torus-like ring), assigning `Role::SideFace { index }` to all lateral faces (`crates/modeling-ops/src/sweep.rs:66-74, 238-257`).
5. **Ruled vs. smooth (`ruled: bool`):**
   - `ruled: true`: constructs `m - 1` (or `m` when `closed`) independent ruled bands (`q = 1` in `v`) connected along the intermediate section loops as sharp `C^0` B-Rep edges.
   - `ruled: false` (default): for `m = 2`, degree `q = min(3, m - 1) = 1`, which coincides algebraically with the ruled surface; for `m = 3`, quadratic `q = 2` (`C^1` across section `1`); for `m ≥ 4`, cubic `q = 3` (`C^2` across interior sections `1..m-2`), with one continuous longitudinal face per lateral strip (no artificial seam splits at intermediate sections unless `ruled: true`).
6. **Explicit v1 non-goals (typed refusals):**
   - Guide curves / rails, centerline spine lofts, prescribed `G^1`/`G^2` end-tangent magnitudes, point/apex degeneracy, open sheet/surface lofts, profiles with `≥ 2` holes, and boolean operations that intersect a `Surface::BSpline` face prior to Roadmap Milestone F7 (§3).

---

## 2. Geometry, Analytic Downcasting & Tolerance Story

### 2.1 Section Curve Representation & Compatibility

Each planar section loop is lifted from sketch `(u_s, v_s)` coordinates into world meters via `P(u_s, v_s) = plane_origin + u_s · x_axis + v_s · (plane_normal × x_axis)` (`crates/kernel-v2/src/adapter.rs:721-741`) as a closed sequence of `M` oriented curve segments `{E_{k,0}, …, E_{k,M-1}}`, where each segment is initially one of:
- **Line segment** `A → B`: exact degree-1 polynomial Bézier curve `p = 1`, `U = [0, 0, 1, 1]`, `P = [A, B]`, `w = [1, 1]`.
- **Circular arc** `A → B` of sweep `Δθ ∈ (0, 2π)` about `(C, n, R)`: exact degree-2 rational NURBS curve (`Piegl & Tiller §7.3`, `ALGORITHM A7.1` at `refs/text/piegl_tiller1997_nurbs_book.txt:13351-13425`), split at `≤ π/2` spans with middle weight `w_1 = cos(Δθ / 2) > 0` (`w = [1, cos(Δθ / 2), 1]`).
- **Full circle** of radius `R`: exact 4-quadrant degree-2 rational NURBS curve (`p = 2`, 9 control points, `U = [0, 0, 0, 1/4, 1/4, 1/2, 1/2, 3/4, 3/4, 1, 1, 1]`, `w = [1, √2/2, 1, √2/2, 1, √2/2, 1, √2/2, 1]`).
- **Cubic B-spline sketch segment** (`SplineSegment` at `crates/waffle-types/src/profiles.rs`): exact polynomial B-spline curve (`p = 3`, unit weights).

#### Topology decomposition into lateral strips (preserving sharp edges)
Unlike academic skinning of a single smooth closed curve (`Piegl & Tiller §10.3`), mechanical CAD profiles contain sharp corners (e.g. squares, D-shafts, keyways). Merging four line segments of a square into one `C^0` knot-multiplicity-`p` surface patch would create a single `Face` with internal `G^0` creases, violating B-Rep face smoothness assumptions in tessellation normals, silhouette extraction, and `Role::SideFace` selection! Therefore:

1. **Equal segment count (`M_0 = M_1 = … = M_{m-1} = M`):** After start-vertex alignment, segment `j ∈ 0..M` of every section corresponds `1:1` across all `m` sections, producing **`M` distinct lateral faces** (or `1` periodic lateral face with a seam half-edge pair when every section is a single smooth full circle).
2. **Unequal segment count (`L4`, Stroud §13.3):** Stroud specifies normalized arc-length co-refinement (`refs/text/stroud2006_brep_modelling_techniques.txt:14640-14671`):
   > *"One way of doing this is to consider the boundary of each section as a continuous sequence parametrised from 0 to 1, say. The parameter values of the vertices are then matched, and new vertices are inserted to minimise the discrepancies."*
   Concretely: compute cumulative arc-length parameters `\tau_{k, j} = L_{k, j} / L_k ∈ [0, 1)` for every vertex of section `k` (with a full circle seeded at `\tau = 0` aligned to the neighbour's vertex `0` direction). Merge `\{\tau_{k, j}\}_{k,j}` within normalized merge band `\tau_{snap}` (matching closest unsolved vertices greedily when `|M_a - M_b|` is small so e.g. a `4`-gon to `3`-gon splits one triangle edge at its midpoint rather than minting near-zero slivers; refusing if any split segment is shorter than `MIN_FEATURE_SIZE = 1e-6 m` at `crates/waffle-types/src/kernel/units.rs:43`). Splitting a line or NURBS segment at parameter `u_*` uses exact knot insertion (`A5.4`) / de Casteljau subdivision (`A5.6`) — **exact point-set preservation, zero geometric approximation**.

### 2.2 Surface Construction: Two Sections (`m = 2`) vs. `m ≥ 3` Sections

#### Case A: Two sections (`m = 2`) — Ruled Surface (`Piegl & Tiller §8.2, §8.4`)
Between corresponding segment curves `C_{0,j}(u)` and `C_{1,j}(u)` on `u ∈ [0, 1]`, Piegl & Tiller define the ruled surface (`refs/text/piegl_tiller1997_nurbs_book.txt:14571-14601`):

> *"We want a surface which is ruled in the v direction, i.e., it is a linear interpolation between C1(u) and C2(u). Furthermore, we require that the interpolation be between points of equal parameter value, i.e., for fixed u, S(u, v) is a straight line segment connecting the points C1(u) and C2(u)."* (`Eq. 8.4`)
> *"1. ensure that the two curves are defined on the same parameter range...*
> *2. if p_1 = p_2, then set p = p_1; otherwise, set p to the larger of p_1, p_2, and raise the degree of the lower degree curve to p (Algorithm A5.9);*
> *3. if the knot vectors U_1 and U_2 are not identical, merge them to obtain the knot vector U; more precisely, u_j is in U if it is in either U_1 or U_2. The maximum multiplicity of u_j in U_1 or U_2 is also carried over to U...*
> *4. using U, apply knot refinement (Algorithm A5.4) to both curves, C_k(u); this step yields the final values of n, w_{i,j}, and P_{i,j}."*

- **`Line → Line` (`p = 1, q = 1`):** `U = [0, 0, 1, 1]`, `V = [0, 0, 1, 1]`, unit weights `w_{i,j} = 1`. This is the **bilinear patch** (`Piegl & Tiller §8.2 Eq. 8.1`, `refs/text/piegl_tiller1997_nurbs_book.txt:14382-14400`):
  $$S(u, v) = (1 - u)(1 - v) P_{0,0} + u(1 - v) P_{1,0} + (1 - u)v P_{0,1} + u v P_{1,1}.$$
  As Piegl & Tiller note (`:14399-14400`, Fig. 8.1a vs. Fig. 8.1b), `S(u, v)` is planar **if and only if** the twist vector `T = S_{uv} = P_{0,0} - P_{1,0} - P_{0,1} + P_{1,1}` has zero component normal to `(P_{1,0} - P_{0,0}) × (P_{0,1} - P_{0,0})` (equivalently, all four corners are coplanar). Whenever the four corners are non-coplanar — e.g. a square rotated by `15°` relative to the base square, or two non-homothetic edges on non-parallel planes — `S(u, v)` is an exact **hyperbolic paraboloid** (a doubly-ruled saddle surface with negative Gaussian curvature `K < 0` everywhere).
- **`Arc/Circle → Line` or `Arc → Arc` (`p = 2, q = 1`):** Step 2 elevates the line from `p_1 = 1` to `p = 2` (`P'_{0} = A, P'_{1} = \frac{1}{2}(A + B), P'_{2} = B`, `w' = [1, 1, 1]`); Step 4 refines knots to match the arc/circle's span structure. **Crucial rational nuance (`Piegl & Tiller §8.4 :14576-14583, :14717-14722`):** In homogeneous coordinates `S^w(u, v) = (1 - v) C_0^w(u) + v C_1^w(u)`, the 3D point is:
  $$S(u, v) = \frac{(1 - v) w_0(u) C_0(u) + v w_1(u) C_1(u)}{(1 - v) w_0(u) + v w_1(u)}.$$
  Notice that for fixed `u`, `S(u, v)` traces the **exact Euclidean straight line segment** from `C_0(u)` to `C_1(u)` for ANY positive weight functions `w_0(u), w_1(u) > 0` (the scalar ratio `\lambda(v) = \frac{v w_1(u)}{(1 - v) w_0(u) + v w_1(u)} \in [0, 1]` merely reparameterizes speed along the straight ruling, and `w(u, v) > 0` is strictly guaranteed for all `(u, v) ∈ [0, 1]^2` by convex combination of positive weights!).

#### Case B: `m = K + 1 ≥ 3` sections (`ruled: false`) — B-Spline / NURBS Skinning (`Piegl & Tiller §10.3`)
Piegl & Tiller define skinning across `K + 1` compatible section curves `C_k(u)` of degree `p` over common knot vector `U` (`refs/text/piegl_tiller1997_nurbs_book.txt:19910-19926, 20024-20038, 20190-20198`):

> *"Based on B-splines, we define skinning as follows. Let C_k^w(u) = \sum_{i=0}^n N_{i,p}(u) P_{i,k}^w, k = 0, \dots, K (10.7) be the rational or nonrational section curves. We assume that all C_k^w(u) are defined on the same knot vector, U, and have common degree p. If necessary, the curves can be brought to a common p and U, as described in Section 8.4 (in fact, the ruled surface is a special case of a skinned surface). Then, for the v direction a degree q is chosen, and parameters \{\bar{v}_k\}, k = 0, \dots, K, and a knot vector, V, are computed. These are then used to do n + 1 curve interpolations across the control points of the section curves, yielding the control points Q_{i,j}^w of the skinned surface."*

1. **Longitudinal degree:** `q = min(3, K)` (`q = 2` for 3 sections, `q = 3` for `≥ 4` sections; `piegl_tiller1997:20025`).
2. **Global shared longitudinal parameterization `(\bar{v}_k, V)` across ALL strips `j ∈ 0..M` (Watertightness Invariant):**
   - **Critical B-Rep invariant:** Strip `j - 1` ends at `u = 1` (`Q_{k, \text{end}}^{(j-1)} = V_{k, j}`) and strip `j` starts at `u = 0` (`Q_{k, 0}^{(j)} = V_{k, j}`). If strip `j - 1` and strip `j` computed independent parameter sets `\bar{v}_k^{(j-1)} \neq \bar{v}_k^{(j)}`, their boundary curves `S_{j-1}(1, v)` and `S_j(0, v)` would interpolate the same vertices `\{V_{k, j}\}_{k=0}^K` at *different* parameter values and knots, producing **two distinct 3D curves** and tearing the B-Rep shell open along every longitudinal edge!
   - Therefore, `(\bar{v}_k, V)` MUST be computed **once globally** over all unique control-point columns across the entire section loop (`Eq. 10.8`, `refs/text/piegl_tiller1997_nurbs_book.txt:20028-20038`), using 3D Euclidean distances `\|P_{i,k} - P_{i,k-1}\|` (with zero-chord columns skipped if a single vertex stays stationary across two sections, and refused if all columns have zero chord):
     $$\bar{v}_0 = 0, \quad \bar{v}_K = 1, \quad \bar{v}_k = \frac{1}{N_{\text{active}}} \sum_{i \in \text{active}} v_{k, i}, \qquad u\text{-knot averaging (`Eq. 9.8`)}: \; v_{j+q} = \frac{1}{q} \sum_{r=j}^{j+q-1} \bar{v}_r \;\; (j = 1, \dots, K - q).$$
   - Because `S_{j-1}(1, v)` and `S_j(0, v)` now solve the **exact same `(K + 1) × (K + 1)` linear system** (`N_{r,q}(\bar{v}_k)`) on the **exact same right-hand side** `\{V_{k, j}\}_{k=0}^K` (with end weights `w = 1` at clamped curve endpoints), `S_{j-1}(1, v) \equiv S_j(0, v)` holds **bit-for-bit identically** for all `v ∈ [0, 1]`, and that shared curve is stored once as the longitudinal edge's `Curve::BSpline`!
3. **Linear solver (`ALGORITHM A9.1 GlobalCurveInterp`, `piegl_tiller1997:15733-15737, 15961`):**
   > *"With this method of choosing parameters and knots, the (n + 1) x (n + 1) coefficient matrix in Eq. (9.2) is totally positive and banded with a semibandwidth less than p (i.e., a_{i,j} = 0 if |i - j| >= p) [DeBo78]. Hence, it can be solved by Gaussian elimination without pivoting."*
   For `q ≤ 3`, the `(K + 1) × (K + 1)` collocation matrix `A_{k,r} = N_{r,q}(\bar{v}_k)` is factored **once** per loft (`O(K q^2)`) and back-substituted across every control-point column of every lateral strip (`O(K q)` per column). For `closed: true` (`m = K + 1 ≥ 3`), periodic knot wrapping `v_{-r} = \bar{v}_{K+1-r} - 1`, `v_{K+1+r} = 1 + \bar{v}_r` produces a cyclic banded `(K + 1) × (K + 1)` system solved deterministically via Sherman-Morrison / dense LU so `S(u, v)` is `C^{q-1}` smooth right across section `0`.
4. **Rational weight positivity invariant (`piegl_tiller1997:20304-20402`):**
   Piegl & Tiller demonstrate a major hazard of naive 4D homogeneous skinning (`Eq. 10.14`, `:20312-20316`):
   > *"when the weights vary within sections and/or from section to section... interpolation in four-dimensional space can even produce zero or negative surface weights, even though all section curve weights are positive."* (In their worked example `:20389-20402`, interpolating positive section weights yields `w_{3,2} = -0.75`, creating a pole / singularity in `\mathbb{R}^3`!)
   - **Why pure polygons/splines and all-circle lofts are immune:**
     1. Pure polygon/spline lofts have `w_{i,k} = 1` everywhere → purely polynomial (`w_{i,j} = 1`).
     2. Lofts across `m ≥ 3` full circles (or aligned arcs of identical angular span `Δθ`) have `w_{i,k} = w_i > 0` **independent of `k`**. Since B-spline basis functions satisfy `\sum_{r=0}^K N_{r,q}(\bar{v}_k) = 1` for every `k` and `A` is nonsingular (`:15733`), the unique solution to `A w_col = w_i \mathbf{1}` is `w_{i,r} = w_i > 0` for all `r`!
   - **Guard for heterogeneous rational lofts (`m ≥ 3` mixing circles/arcs of unequal spans with lines):** After solving `Eq. 10.14` in `\mathbb{R}^4 = (w P, w)`, check `\min_{i,r} w_{i,r} > W_MIN` (`W_MIN = 1e-3`). If chord-length `\bar{v}_k` produces `w_{i,r} \le W_MIN`, retry once with centripetal parameterization (`Eq. 9.6`, `d_k^{1/2}`, `:15709-15718`); if still `\le W_MIN`, fail loudly with `KernelV2Error::LoftNonPositiveWeight` rather than emitting a singular surface or clamping weights (which would stop interpolating the input sections!).

### 2.3 Exact Analytic Downcasting Table

`kernel-v2` carries five exact analytic `Surface` variants (`Plane`, `Cylinder`, `Cone`, `Sphere`, `Torus` at `crates/kernel-v2/src/arena.rs:241-251`) and six `Curve` variants (`Line`, `CircleArc`, `Circle`, `EllipseArc`, `HyperbolaArc`, `SurfacePair` at `crates/kernel-v2/src/arena.rs:198-238`). Before allocating a `Surface::BSpline` or `Curve::BSpline`, `construct::loft` applies **exact algebraic downcasting** within `TAU_COINCIDENT` (`1e-9 m` at `crates/waffle-types/src/kernel/units.rs:28`, scaled by `(1 + max|coord|)` matching `planarity_band` at `crates/kernel-v2/src/validate.rs:85-88`):

| Loft Configuration | True Algebraic Surface / Curve | Representable by Existing `kernel-v2` Analytic Types? | Emitted `kernel-v2` Geometry |
|---|---|---|---|
| **End caps** (`!closed`, sections `0` and `m - 1`) | Planar polygon / circle / arc-spline face | **Yes — Exact** | `Surface::Plane` (`arena.rs:76-81`) |
| **Longitudinal edge** with `q = 1` (`m = 2`, or `ruled: true`, or collinear control points in `m ≥ 3`) | Straight line segment | **Yes — Exact** | `Curve::Line` |
| **`m = 2` (or `ruled: true` slice), `Line → Line` with coplanar 4 corners** (`|n · (P_{1,1} - P_{0,0})| ≤ band`) — includes **all parallel homothetic polygons** (pyramidal frusta, prisms, wedges, offset rectangles) & coplanar trapezoids | Plane | **Yes — Exact** | `Surface::Plane` |
| **`m = 2` (or `ruled: true` slice), `Line → Line` with non-coplanar corners** (twisted polygon, general non-homothetic polygon on tilted plane) | Hyperbolic paraboloid (`z = c x y`, doubly-ruled saddle quadric) | **No** (`Surface` has no hyperbolic paraboloid variant) | `Surface::BSpline` (`p = 1, q = 1`, polynomial, `2 × 2` control net) |
| **`m = 2` (or collinear coaxial `m ≥ 3`), coaxial circles/arcs perpendicular to axis with `|r_1 - r_0| ≤ TAU_COINCIDENT`** | Right circular cylinder | **Yes — Exact** | `Surface::Cylinder` (`axis = unit(O_1 - O_0)`, `radius = r_0`, matching `extrude.rs`) |
| **`m = 2` (or collinear linearly-tapered coaxial `m ≥ 3`), coaxial circles/arcs perpendicular to axis with `r_1 ≠ r_0` (`r_0, r_1 > 0`)** | Right circular cone frustum | **Yes — Exact** | `Surface::Cone` (`apex = (r_1 O_0 - r_0 O_1)/(r_1 - r_0)`, `half_angle = atan2(|r_1 - r_0|, \|O_1 - O_0\|)`) |
| **Offset (non-coaxial) parallel circles** (`(O_1 - O_0) \times n_0 \neq 0`) or **circles on non-parallel planes** | Oblique circular cylinder / oblique circular cone (normal cross-sections are ellipses!) | **No** (`CylinderSurface` & `ConeSurface` at `arena.rs:83-107` are strictly right-circular) | `Surface::BSpline` (`p = 2, q = 1`, rational NURBS, `9 × 2` control net) |
| **Circle / Arc → Line** (e.g. round-to-square HVAC transition, `L4`) | Rational degree-`(2, 1)` ruled conoid patch | **No** | `Surface::BSpline` (`p = 2, q = 1`, rational NURBS) |
| **`m ≥ 3` (`ruled: false`)** with non-collinear longitudinal control columns | Degree-`(p, q)` polynomial or rational NURBS skin (`q ∈ {2, 3}`) | **No** | `Surface::BSpline` + `Curve::BSpline` longitudinal edges |

> **Why exact downcasting matters:** A rectangular hopper/frustum, tapered prism, wedge, coaxial stepped shaft, or conical reducer built with `Loft` emits **100% `Plane` / `Cylinder` / `Cone` faces** — meaning it immediately participates in `yang-rs` booleans (`Add`, `Cut`, `Intersect`), `measure` analytic distances, D1 exact silhouettes, and STEP `PLANE` / `CYLINDRICAL_SURFACE` / `CONICAL_SURFACE` export in v1 with zero extra machinery!

### 2.4 `BrepArena` Data Structures & Tolerance Story

To preserve the `Copy` bound on `Surface` (`crates/kernel-v2/src/arena.rs:240`), `Curve` (`arena.rs:197`), and `HalfEdge` across `kernel-v2`, `BrepArena` stores variable-length knot vectors and control nets in dedicated append-only pools (`arena.bspline_curves: Vec<BSplineCurveData>`, `arena.bspline_surfaces: Vec<BSplineSurfaceData>`) indexed by lightweight `Copy` handles:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BSplineCurveId(pub(crate) u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BSplineSurfaceId(pub(crate) u32);

#[derive(Debug, Clone, PartialEq)]
pub struct BSplineCurveData {
    pub degree: usize,                 // p >= 1
    pub knots: Vec<f64>,               // non-decreasing, len = control_points.len() + p + 1, clamped [0, 1]
    pub control_points: Vec<Point3>,   // len = n + 1 >= p + 1 (meters)
    pub weights: Option<Vec<f64>>,     // None = polynomial (all 1.0); Some(w) with all w_i > 0
}

#[derive(Debug, Clone, PartialEq)]
pub struct BSplineSurfaceData {
    pub degree_u: usize,                       // p >= 1
    pub degree_v: usize,                       // q >= 1
    pub knots_u: Vec<f64>,                     // clamped [0, 1], len = n_u + p + 2
    pub knots_v: Vec<f64>,                     // clamped [0, 1], len = n_v + q + 2
    pub control_points: Vec<Vec<Point3>>,      // (n_u + 1) rows × (n_v + 1) cols (meters)
    pub weights: Option<Vec<Vec<f64>>>,        // None = polynomial; Some(w) with all w_{i,j} > 0
}
```

And `Curve` / `Surface` gain one `Copy` variant each:
- `Curve::BSpline { id: BSplineCurveId, t_range: (f64, f64), reversed: bool }` (when `reversed == false`, traverses `t_range.0 → t_range.1`; its twin half-edge flips `reversed: !reversed`, preserving `validate_solid` Check 5's `CurveTwinMismatch` pairing at `crates/kernel-v2/src/validate.rs:31-32`).
- `Surface::BSpline { id: BSplineSurfaceId, reversed: bool }` (outward normal is `±unit(S_u × S_v)` according to `reversed`).

**Why `waffle_types::bspline` is NOT reused in `kernel-v2`:** `crates/waffle-types/src/bspline.rs:53-55, 167-169` clamps `t = 1.0` to `t_max - 1e-10` instead of handling clamped right-endpoint knot spans per Piegl & Tiller `FindSpan` (`ALGORITHM A2.1`, `refs/text/piegl_tiller1997_nurbs_book.txt:3240-3268`), introducing an `O(1e-10 · \|C'\|)` boundary gap at `u = 1` and `v = 1`, and supports neither rational weights `w` nor partial derivatives (`A4.4`) nor knot insertion (`A5.4`) / Bézier decomposition (`A5.6`/`A5.7`). `kernel-v2` will implement clean, allocation-light Piegl & Tiller algorithms (`A2.1`, `A2.2`, `A2.3`, `A4.4`, `A5.4`, `A5.6`, `A5.7`, `A5.9`, `A9.1`) in `crates/kernel-v2/src/geom/bspline.rs`.

**Exactness & Tolerance Layering (`crates/waffle-types/src/kernel/units.rs`):**
1. **Boundary water-tightness (`~1e-15` relative, algebraic):** Clamped knot vectors (`p + 1` / `q + 1` endpoint multiplicity) guarantee `S(u, 0) = C_0(u)`, `S(u, 1) = C_K(u)` (`Piegl & Tiller P4.5`, `:5948-5965`), and global `(\bar{v}_k, V)` guarantees `S_{j-1}(1, v) = S_j(0, v)` along every longitudinal edge. Every B-Rep vertex is evaluated at `(u, v) ∈ \{0, 1\}^2`, which equals the corner control point `P_{0,0}, P_{n_u,0}, P_{0,n_v}, P_{n_u,n_v}` with zero basis rounding!
2. **Tessellation certified error (`chord_bound`):** Decompose `Surface::BSpline` into rational Bézier sub-patches via `ALGORITHM A5.7 DecomposeSurface` (`refs/text/piegl_tiller1997_nurbs_book.txt:7751-7820`). Subdivide `(u, v)` quad cells until the maximum distance from interior Bézier control points to the quad's bilinear/triangle approximation is `\le d_\epsilon` (Yang 2025 Theorem A.1, `refs/text/yang2025_hybrid_boolean.txt:308-321, 1160-1182`). Crucially, **boundary edges are discretized solely from each canonical half-edge's `Curve`** (`Curve::BSpline` / `Curve::Line` / `Curve::CircleArc`) so twin half-edges across adjacent faces emit **bit-identical vertex sequences** (watertight render mesh, zero T-junction cracks, matching `tessellate/mod.rs`'s boundary-stitching contract).

---

## 3. The Boolean Question (`yang-rs` Code Audit)

### 3.1 What Happens Today When a New Surface Type Enters `boolean_op`

Tracing the exact call chain at HEAD `b4bee54e`:
1. `KernelV2Adapter::boolean_union` / `boolean_subtract` / `boolean_intersect` (`crates/kernel-v2/src/adapter.rs:769-838`) calls `crate::boolean::boolean_op(&mut self.arena, ha, hb, op, Some(&mut self.journal))` (`crates/kernel-v2/src/boolean/mod.rs:146`).
2. `boolean_op` immediately converts **both** input solids into `yang_rs::brep::BRep` via `to_yang::to_yang_brep_indexed(arena, a)` and `to_yang::to_yang_brep_indexed(arena, b)` at `crates/kernel-v2/src/boolean/mod.rs:193-196` — **before** `yang_rs::boolean::boolean` checks `conservative_aabb` disjointness (`crates/yang-rs/src/boolean.rs:297-335`) or untouched-shell passthrough!
3. Inside `crates/kernel-v2/src/boolean/to_yang.rs`:
   - `convert_surface(s: &Surface)` (`to_yang.rs:48-96`) exhaustively matches `Surface::{Plane, Cylinder, Cone, Sphere, Torus}` into `yang_rs::geom::Surface`.
   - `convert_curve(...)` (`to_yang.rs:98-216`) exhaustively matches `Curve::{Line, Circle, CircleArc, EllipseArc, HyperbolaArc, SurfacePair}` into `yang_rs::geom::Curve` (`SurfacePair` itself converts its two `PairSurface`s via `convert_pair_surface` at `to_yang.rs:218-236`, which also only knows the 5 quadric/torus types).
4. Once inside `yang-rs`, `yang_rs::geom::Surface` (`crates/yang-rs/src/geom.rs:37-72`) is a `Copy` 5-variant enum (`Plane | Sphere | Cylinder | Cone | Torus`) with **45 non-test `.rs` files** in `crates/yang-rs/src/` matching on `Surface::*` or `Curve::*`.

We now evaluate the three architectural options requested.

---

### 3.2 Option (a): Standalone `NewBody` v1 Lofts + Loud Typed `NotSupported` on `BSpline` Booleans

Under Option (a):
- `LoftParams.combine` defaults `None => CombineMode::NewBody` via `normalize_loft_combine` (the exact pattern already established for `PipeParams` at `crates/feature-engine/src/types.rs:1255-1279` and `SweepParams` at `types.rs:1358-1374`). Every default loft builds a standalone manifold solid without invoking `yang-rs`.
- In `crates/kernel-v2/src/boolean/to_yang.rs:48, 98`, `Surface::BSpline { .. }` and `Curve::BSpline { .. }` return `Err(KernelV2Error::NotSupported { operation: "boolean on B-spline loft surface (D14: yang-rs is analytic-only until F7)".into() })`.
- `KernelV2Adapter::map_boolean_err` (`crates/kernel-v2/src/adapter.rs:820-830`) maps `KernelV2Error::NotSupported` to `KernelError::NotSupported`, which `feature-engine` surfaces as `ErrorKind::NotSupported` (`crates/feature-engine/src/types.rs:2278, 2340`) — a clean, structured capability refusal (classified `UNSUPPORTED`, never `ERROR` or silent corruption).
- **Bonus from §2.3 Analytic Downcasting:** Whenever a loft produces only `Plane`, `Cylinder`, and `Cone` faces (e.g. pyramidal frustum, tapered bracket boss, wedge, coaxial conical transition, stepped cylinder), `to_yang.rs` sees zero `BSpline` entities and **runs `Add` / `Cut` / `Intersect` booleans through `yang-rs` in v1 automatically**!

---

### 3.3 Option (b): Admitting General Parametric / NURBS Surfaces into `yang-rs`

Yang et al. 2025 designed the hybrid boolean algorithm *expressly* around trimmed NURBS patches:
> *"We convert the surface patches of the B-Rep models into NURBS surfaces as inputs of this step."* (`refs/text/yang2025_hybrid_boolean.txt:304-305`)

However, our `yang-rs` implementation took an explicit analytic shortcut recorded in `docs/yang_deviations.md:3767-3784` as **`D14`** (**status: `DELEGATED (C)` — open debt, NOT `PERMANENT` signed scope**):
- **`D14` (`docs/yang_deviations.md:3767-3784`):** `yang-rs` represents only the 5 analytic implicit/parametric surface kinds (`Plane`, `Sphere`, `Cylinder`, `Cone`, `Torus`) and uses algebraic quadric/torus SSI (`ssi-rs`) + 3D implicit Newton (`F(x) = 0`) instead of NURBS Bézier subdivision, Gauss-map loop detection, and 4-parameter `(u_1, v_1, u_2, v_2)` surface-pair Newton/tracing.
- **`N94` (`docs/yang_deviations.md:6895-6914`, status: `OPEN`):** Non-conic intersection curves (`Curve::SurfacePair` today; degree `> 2` `BSpline × Analytic` or `BSpline × BSpline` curves tomorrow) lack parameter-domain p-curve discretization in Stage 1 (`§4.1.1`) for chained re-entry booleans (`(A op B) op C`).
- **Roadmap sequencing (`docs/yang_faithfulness_roadmap.md`):** Standing priority (adopted 2026-10-27) locks boolean work to **Milestones F0 → F7 in strict order**, placing `D14` retirement in **Milestone F7** *after* F1–F6 retire the 99 non-faithful matrix rows on the existing analytic surface set. *(Superseded 2026-10-27: D14 reopened, resolved by F5 — see the amendment at the top.)*

Below is the **complete stage-by-stage `yang-rs` + `ssi-rs` audit** of every mechanism required to admit a general parametric surface `s(u, v)` (`eval(u, v)`, `RatSurfaceDerivs` `s_u, s_v, s_{uu}, s_{uv}, s_{vv}`, domain `[u_0, u_1] × [v_0, v_1]`, rational Bézier sub-patches):

| Pipeline Stage | Paper Mechanism (`yang2025_hybrid_boolean.txt`) | Current `yang-rs` / `ssi-rs` Code (`path:line`) | Changes Required for Parametric `Surface::BSpline` |
|---|---|---|---|
| **0. Core Geometry & Bijection** | NURBS surface `p(u, v)` on `[u_0, u_1] × [v_0, v_1]` + parametric trimming p-curves (`§4.1.1 :304-315`) | `crates/yang-rs/src/geom.rs:37-72` (`Surface: Copy`), `geom.rs:85-147` (`Curve: Copy`), `geom.rs:411-493` (`signed_distance_to_surface`), `geom.rs:495-560` (`surface_normal_at`), `crates/yang-rs/src/brep.rs:1442-1600` (`eval_source`), `crates/yang-rs/src/boolean.rs:243-282` (`conservative_aabb`) | 1. Add arena/Arc-backed `Surface::BSpline` & `Curve::BSpline` (plus `TrimPCurve` in `(u, v)` on half-edges).<br>2. Replace 3D-only `surface_normal_at(s, x)` / `signed_distance_to_surface(s, x)` call sites with `(u, v)`-seeded `s_u × s_v` evaluation or Newton point-inversion `min_{u,v} \|s(u,v) - x\|^2` (a general NURBS patch has **no** closed-form 3D implicit equation).<br>3. `conservative_aabb` (`boolean.rs:243`) uses positive-weight control-point min/max (convex hull property, P&T `P4.9`). |
| **Stage 0: Exact Coincident / Overlap Pre-Pass** | Coplanar / coincident patch overlap handling (`§4.5.2 :652-680`) | `crates/yang-rs/src/stage0/mod.rs:112-162`, `coplanar_scan.rs:102`, `cylinder.rs:76-152`, `plane_weld.rs:163`, `edge_in_plane.rs:94`, `rim_chords.rs:37`, `frame.rs:161` | Existing Stage-0 arms pass non-analytic faces through untouched; overlapping identical/sub-patch NURBS faces (e.g. two lofts sharing a ruled side face) need overlapping-surface detection (`\|s_1(u_1, v_1) - s_2(u_2, v_2)\| \le \tau` over shared Gauss-map / sample points, `§4.5.2`). |
| **Pre-Stage-1 Shell Contact** | Conservative `2 d_\epsilon` triangle proximity filter (`§4.2.2 Fig. 10a :479-490`) | `crates/yang-rs/src/shell_contact.rs:295` | Uses per-face/per-triangle `chord_bound` (`d(T)`); works unchanged once Stage 1 populates exact `d(T)` for `Surface::BSpline`. |
| **Stage 1: Error-Bounded Triangulation** | *"We discretize the parametric trimming curves, i.e., p-curves, on a patch into polyline segments within a distance threshold d_\epsilon... compute the maximum distance between the control points of s_{T'} and T', denoted by d(T')"* (`§4.1.1–§4.1.2 :286-408`, Theorems A.1 & A.2 `:1160-1209`) | `crates/yang-rs/src/stage1_tessellate.rs:511-800`, `normals_chord_bounds.rs:124-350`, `patch_tessellators.rs:25-171`, `surface_face_tessellators.rs:36-74`, `self_contact.rs:42` | Today Stage 1 uses per-quadric structured grids (`Cylinder`, `Cone`, `Sphere`, `Torus`) and closed-form chord-sag formulas. Admitting `BSpline` requires building the paper's actual §4.1 engine: `DecomposeSurface` (`A5.7`) into Bézier sub-patches, Theorem A.1/A.2 control-net `d(T)` bound, p-curve discretization in `(u, v)` (fixing **`N94`** simultaneously), CDT in `(u, v)`, and persistent `TessellationMap` `(tri_id → (u_0, v_0), (u_1, v_1), (u_2, v_2))`! |
| **Stage 2: Mesh Arrangement** | Exact triangle-triangle mesh arrangement (`[Cherchi et al. 2022]`, `§4.2 :410-432`) | `crates/cherchi-rs/` + `crates/yang-rs/src/boolean.rs` | **Zero changes.** `cherchi-rs` operates purely on `f64` 3D triangles and provenance IDs. |
| **Stage 3: Missing-Loop Detection & SSI** | *"Theorem 4.1. Let P_1 and P_2 be two rational C^1 continuous parametric patches... Suppose that P_1 and P_2 do not contain any intersection loops, and there does not exist vectors n_1 \in G_1 and n_2 \in G_2 such that n_1 \times n_2 = 0."* (`§4.2.2 :450-494`, Appendix B `:1211-1231`); Bajaj et al. 1988 marching (`§4.3.3 :567-573`) | `crates/yang-rs/src/stage3_ssi.rs:56-280, 768` (`surface_to_quadric` → `ssi_rs::intersect`), `crates/ssi-rs/src/lib.rs:99-255, 397` (quadrics only; even `Torus` returns `None` at `stage3_ssi.rs:94`) | **Largest new subsystem:** Implement Bézier control-net Gauss-map / normal-cone bounding (`G_1, G_2` on `S^2`), recursive `2 d_\epsilon` + Gauss-map overlap subdivision (`Fig. 10b`), closest-point seed optimization (`Fig. 10c`), collinear-normal tangency check (`§4.3.3 :567`), and differential curve marching (`s_1(u_1,v_1) = s_2(u_2,v_2)` along `(n_1 \times n_2)`) for small closed loops missed by the mesh! |
| **Stage 4: Vertex Relocation, Refinement & `(u,v)` Trim** | Parametric 4-var Newton `D(u,v,s,t) = s_1(u,v) - s_2(s,t) = 0` (`Appendix C :1233-1252`), tangent-plane step (`§4.3.2 :493-520`), 6-var 3-surface Newton (`§4.3.3 :573-576`), midpoint arc-height refinement (`§4.3.4 :575-594`), `(u,v)` CDT re-trimming & `d(T)` update (`§4.4.1 :546-572`) | `crates/yang-rs/src/stage4_relocate.rs:78-422` (3D implicit `F(x)=0` Newton), `stage4_project.rs:43-319` (`SurfaceChart`), `stage4_dt.rs:101-252` (`RevFrame`/`Profile` Bézier), `stage4_correct.rs:77-273`, `stage4_construct.rs:64`, `stage4_boundary_curve.rs:196`, `stage4_rehome.rs:48`, `stage4_slit.rs:78`, `stage4_transit.rs:76` | 1. Add `(u,v,s,t)` parametric tangent-plane + pseudo-inverse Newton solver (`∇D ∇D^T a_k = s_2 - s_1`, `Appendix C :1247-1250`), handling `BSpline × BSpline` and `BSpline × Analytic` (and curve-restricted 3-var Newton on boundary edges `:481-483`).<br>2. Add `SurfaceChart::BSpline` in `stage4_project.rs` using `(u,v)` directly from `TessellationMap` + 2-var Newton point inversion.<br>3. Generalize `stage4_dt.rs` from `RevFrame` surfaces of revolution to arbitrary tensor-product NURBS sub-patches (`Fig. 6c–d :340-365`). |
| **Stage 5 & Re-Entry** | Spline p-curve fitting in `(u,v)` (`§4.5.1 :615-651`), mesh segmentation + exact winding number (`§4.4.2, §4.5.5`), B-Rep emission | `crates/yang-rs/src/stage5_topology.rs:381`, `stage5_topology/emit.rs:285-531`, `stage5_output_refine.rs:329`, `stage5_seam_merge.rs:58`, `stage5_envelope.rs:648`, `crates/kernel-v2/src/boolean/from_yang.rs:45-230` | Emit intersection edges touching `Surface::BSpline` as `Curve::SurfacePair` (generalized `PairSurface` holding `BSplineSurfaceId` + sampled `(u_1,v_1,u_2,v_2)` polyline seed) or fitted `(u,v)` p-curves (`§4.5.1`), and convert `Surface::BSpline` / `Curve::BSpline` losslessly back in `from_yang.rs`. |

**Scope verdict on Option (b)** *(superseded in part 2026-10-27 — Option (b) is F5's last item; see the amendment at the top)*: Option (b) is **100% aligned with Yang 2025** and is the exact work needed to retire **`D14`** and **`N94`** at **Milestone F7** of `docs/yang_faithfulness_roadmap.md`. However, landing it *before* F0–F6 finish stabilizing the analytic pipeline would violate the repository's standing roadmap order (`GEMINI.md` §1.3) and couple a clean B-series constructor (`Loft`) to a multi-week boolean overhaul.

---

### 3.4 Option (c): Faceted Approximation into `Surface::Plane` Triangles

Under Option (c), `construct::loft` would tessellate curved loft walls at build time into hundreds of small `Surface::Plane` triangles so `to_yang.rs` sees a polyhedron.

**Why Option (c) is strictly prohibited (both by governance and by geometry):**
1. **Direct violation of Engineering Law:**
   - **Constitution P8 (`governance/ENGINEERING_CONSTITUTION.md` §3.1):** *"Geometric diversity is mandatory. Every benchmark, test suite, and validation target must include planar, cylindrical, conical, spherical, toroidal, and freeform geometry. Planes-only designs are prohibited as target architecture."*
   - **Constitution P9 & P10:** *"Precision beats speed. Exact or bounded-error methods are preferred over fast approximations. Silently degrading precision to avoid errors is prohibited."* / *"Fail loudly, never silently repair."*
   - **Architectural Invariant A15.1–A15.3 (`governance/ARCHITECTURAL_INVARIANTS.md`):** Constructors must preserve true surface geometry; baking a chordal mesh into `BrepArena` as "exact" planar B-Rep faces erases the distinction between exact CAD B-Rep and mesh import (`ImportedBody`).
2. **Concrete technical breakage across the stack:**
   - **Topological & persistent-ID explosion:** A single smooth loft between two 32-segment approximations produces `64+` `Face` entities instead of `1` or `4`, breaking `Role::SideFace { index }`, changing face PIDs whenever chord density changes, and turning `Sketch on Face` / `MateConnector` picks on a loft wall into arbitrary micro-triangle picks.
   - **Corrupted STEP export & drawings:** `step_export` would emit hundreds of `PLANE` entities instead of a single `B_SPLINE_SURFACE_WITH_KNOTS` / `RATIONAL_B_SPLINE_SURFACE`, and D1 drawings (`projection.rs`) would draw dense black wireframe spiderwebs across every smooth loft wall instead of clean silhouettes!
   - **Boolean fragility in `yang-rs`:** Feeding two faceted 200-face polyhedra with `178°` dihedral creases and near-coplanar grazing triangles into `yang-rs` Stage 0 (`coplanar_scan`) and Stage 2 (`cherchi-rs`) manufactures the exact near-degenerate sliver and multi-plane vertex configurations that stress `stage4_relocate` hardest.

---

### 3.5 Recommendation

**Adopt Option (a) + §2.3 Exact Analytic Downcasting for Loft v1 (`L1`–`L6`), and schedule Option (b) as Milestone F7 (`L7`, retiring `D14` + `N94`). Reject Option (c) unconditionally.**

This gives users:
1. **Exact freeform NURBS lofts immediately** as standalone parts/bodies (`NewBody` default), with exact mass properties, watertight error-bounded viewport meshes, persistent face/edge naming, rigid transforms/patterns, and exact `B_SPLINE_SURFACE` STEP export.
2. **Immediate `yang-rs` booleans (`Add`/`Cut`/`Intersect`)** whenever a loft's faces analytically downcast to `Plane` / `Cylinder` / `Cone` (all parallel-polygon frusta, wedges, coaxial circular cones/cylinders).
3. **Loud typed `NotSupported` citing `D14`** whenever a boolean touches a `Surface::BSpline` face, keeping zero technical debt in `yang-rs` while F0–F6 execute.

---

## 4. Every Layer Touched (with File Paths)

### 4.1 `crates/kernel-v2` — Geometry, Constructor, Validation, Mesh, Mass, STEP, Introspection

| File | Exact Changes |
|---|---|
| `crates/kernel-v2/src/arena.rs` (`:197-255, 360-520`) | Add `BSplineCurveId`, `BSplineSurfaceId`, `BSplineCurveData`, `BSplineSurfaceData`, `Curve::BSpline { id, t_range, reversed }`, `Surface::BSpline { id, reversed }`, and arena pool accessors (`alloc_bspline_curve`, `bspline_curve`, `alloc_bspline_surface`, `bspline_surface`). |
| `crates/kernel-v2/src/geom/bspline.rs` *(new, wired in `geom.rs`)* | Implement Piegl & Tiller `FindSpan` (`A2.1`), `BasisFuns` / `DersBasisFuns` (`A2.2`/`A2.3`), `CurveDerivsAlg1` / rational curve eval (`A3.2`/`A4.2`), `RatSurfaceDerivs` (`A4.4`), `RefineKnotVectCurve` (`A5.4`), `DecomposeCurve` (`A5.6`), `DecomposeSurface` (`A5.7`), `DegreeElevateCurve` (`A5.9`), `GlobalCurveInterp` (`A9.1`, open & periodic), and damped 2-var Newton closest-point `(u, v)` inversion on `BSplineSurfaceData`. |
| `crates/kernel-v2/src/construct/loft.rs` *(new, wired in `construct/mod.rs:1-35` & `lib.rs`)* | Implement `loft(arena, sections, opts, seed)`: input validation, normal-consistent orientation & minimum-chord `start_vertex` alignment (plus explicit user `start_vertex` / `reverse`), equal-count & (`L4`) Stroud §13.3 `[0,1]` arc-length co-refinement, analytic downcasting (§2.3), ruled (`m = 2` / `ruled: true`) & skinned (`m ≥ 3`, `ruled: false`, open & `closed: true`) surface synthesis, B-Rep shell assembly, `assign_face_pids(solid, seed)` (`arena.rs`), and `debug_assert!(validate_solid(...).is_ok())`. |
| `crates/kernel-v2/src/error.rs` | Add typed `LoftTooFewSections`, `LoftDegenerateSection`, `LoftSelfIntersecting`, `LoftUnsupportedSection`, `LoftNonPositiveWeight` variants on `KernelV2Error`. |
| `crates/kernel-v2/src/validate.rs` (`:18-35`) & `crates/kernel-v2/src/validate/faces.rs` (`:27-350`) | Extend Check 4 & Check 5 (`validate_face_geometry` / `check_twin_curves`) for `Surface::BSpline` and `Curve::BSpline`: verify positive weights, valid knot vectors, corner vertices on `S(u,v)` within `CURVED_SURFACE_DEBUG_TOLERANCE`, interior sample outward normal consistency with boundary traversal, and twin `Curve::BSpline` reversal symmetry. |
| `crates/kernel-v2/src/tessellate/mod.rs` (`:90-260`) & `tessellate/surfaces.rs` (`:1-380`) | Sample `Curve::BSpline` boundary half-edges at certified chord tolerance via `DecomposeCurve` (`A5.6`); tessellate `Surface::BSpline` faces via `DecomposeSurface` (`A5.7`) adaptive quad subdivision + boundary polygon stitching, recording exact `FaceRange`s and certified `chord_bound` (`Theorem A.1`). |
| `crates/kernel-v2/src/mass.rs` (`:35-145`) & `geom/flux.rs` | Integrate `Surface::BSpline` face contributions to `Moments` (`volume`, `centroid`, `inertia`, `area`) using tensor-product **Gauss-Legendre quadrature over each Bézier sub-patch** (`A5.7`): for polynomial `S(u, v)` of degree `(p, q)`, the divergence-theorem volume integrand `\frac{1}{3} S \cdot (S_u \times S_v)` has polynomial degree `(3p - 1, 3q - 1)` — so `\lceil 3p/2 \rceil \times \lceil 3q/2 \rceil` Gauss nodes (`2 × 2` for bilinear `p=q=1`, `5 × 5` for bicubic `p=q=3`) are **algebraically exact to machine precision (`~1e-15`)**; for rational patches (`weights.is_some()`), adaptive `8 × 8` sub-patch Gauss quadrature converges to `< 1e-12` relative error! |
| `crates/kernel-v2/src/step_export.rs` (`:160-430`) | Emit ISO 10303-42 AP214 entities for `Surface::BSpline` and `Curve::BSpline` (scaling control points by `1000.0` m → mm per `step_export.rs` convention):<br>• Polynomial: `B_SPLINE_SURFACE_WITH_KNOTS('', p, q, ((#cp...)), .UNSPECIFIED., .F., .F., .F., (mult_u...), (mult_v...), (knots_u...), (knots_v...), .UNSPECIFIED.)` and `B_SPLINE_CURVE_WITH_KNOTS(...)`.<br>• Rational: complex entity `(BOUNDED_SURFACE() B_SPLINE_SURFACE(p, q, ((#cp...)), .UNSPECIFIED., .F., .F., .F.) B_SPLINE_SURFACE_WITH_KNOTS((mult_u...), (mult_v...), (knots_u...), (knots_v...), .UNSPECIFIED.) GEOMETRIC_REPRESENTATION_ITEM() RATIONAL_B_SPLINE_SURFACE(((w...))) REPRESENTATION_ITEM('') SURFACE())` (and curve counterpart), both already parsed natively by `truck-stepio` (`crates/step-import/src/analytic.rs:601-602, 749`)! |
| `crates/kernel-v2/src/boolean/to_yang.rs` (`:48-236`) | Return `Err(KernelV2Error::NotSupported { operation: "boolean on B-spline loft surface (D14)".into() })` on `Surface::BSpline` and `Curve::BSpline` (§3.2). |
| `crates/kernel-v2/src/signature.rs` (`:75-300`), `introspect.rs` (`:68-305`), `transform.rs` (`:188-305`), `measure.rs` (`:209-264`), `projection.rs` (`:24-90`) & `projection/silhouette.rs` (`:228-295`), `adapter.rs` (`:716-768`) | • `signature.rs`: `surface_type_str` → `"bspline"`, `axis_of` → `None`, `outward_normal_at` / `place_centroid` via `(u_mid, v_mid)` + Newton point inversion.<br>• `introspect.rs`: `edge_polyline` for `Curve::BSpline`; `conservative_shell_aabb` using positive-weight NURBS control-point convex hull.<br>• `transform.rs`: **Affine invariance of NURBS** (`Piegl & Tiller P4.3`): `map_surface` / `map_curve` transform ONLY `control_points` (`P' = R·P + t`), keeping knots & weights untouched; `mirror_solid` flips `reversed` so `PatternLinear` / `PatternCircular` / `PatternMirror` work exactly on lofts!<br>• `measure.rs`: `face_is_exactly_triangulated` → `false`; closest-point Newton refinement on `Surface::BSpline` / `Curve::BSpline`.<br>• `projection.rs` / `silhouette.rs`: `Curve::BSpline` projects as render-density polyline (`projection.rs:24-30`); `silhouette.rs` traces `(S_u × S_v) · w = 0` contours on Bézier sub-patches (or records `ProjectionDeclines` until silhouette sub-step lands, see §6 Q5).<br>• `adapter.rs`: implement `Kernel::loft` on `KernelV2Adapter`, forwarding `self.take_construct_seed()` (`adapter.rs:485`). |

---

### 4.2 `crates/waffle-types` & `crates/modeling-ops`

| File | Exact Changes |
|---|---|
| `crates/waffle-types/src/kernel/types.rs` (`:433-455`) | Add kernel boundary types:<br>• `LoftEdge` (`Line { a, b }`, `Arc { a, b, center, radius, ccw }`, `Spline { control_points }`) in sketch `(u, v)` meters, plus `Circle { center, radius }` loop variant on `LoftLoop`.<br>• `LoftSection { plane_origin: [f64; 3], plane_normal: [f64; 3], plane_x_axis: [f64; 3], outer: LoftLoop, holes: Vec<LoftLoop>, start_vertex: Option<usize>, reverse: bool }`.<br>• `LoftOptions { closed: bool, ruled: bool }`. |
| `crates/waffle-types/src/kernel/traits.rs` (`:90-145`) | Add `fn loft(&mut self, sections: &[LoftSection], options: &LoftOptions) -> Result<KernelSolidHandle, KernelError>` (defaulting to `Err(KernelError::NotSupported { operation: "loft".into() })`, matching `Kernel::sweep` at `traits.rs:116-135`). |
| `crates/waffle-types/src/kernel/mock.rs` (`:675-727`) | Implement `MockKernel::loft` (Rule 14: mock before real): validate `sections.len() >= 2` (`>= 3` when `options.closed`), finite coordinates, hole-count consistency, and return a deterministic synthetic solid with planar end caps (when `!options.closed`) + `"bspline"` / `"planar"` lateral `MockFace`s oriented along `c_{m-1} - c_0`. |
| `crates/waffle-types/src/roles.rs` (`:8-38`) | **No new `Role` variant required** (avoiding a `Selector::Role` wire-format floor bump per `crates/file-format/src/save.rs:26-29`!): open lofts assign `Role::EndCapNegative` (section `0` cap), `Role::EndCapPositive` (section `m - 1` cap), and `Role::SideFace { index }` (lateral strips in canonical section-0 edge order); closed lofts assign `Role::SideFace { index }` to all lateral faces (`crates/modeling-ops/src/sweep.rs:97-147`). |
| `crates/modeling-ops/src/loft.rs` *(new, re-exported in `modeling-ops/src/lib.rs:1-27`)* | Implement `execute_loft(kb, sections, options, before_snapshot) -> Result<OpResult, OpError>`: argument checks, `kb.loft(sections, options)?`, `diff::snapshot` provenance, and `assign_loft_roles` (matching start/end planar caps by centroid + outward normal alignment with `±section.plane_normal`, remaining faces as `Role::SideFace { index }`). |

---

### 4.3 `crates/feature-engine`, `crates/file-format` & `crates/test-harness`

| File | Exact Changes |
|---|---|
| `crates/feature-engine/src/types.rs` (`:665-845, 1318-1375`) | • Add `LoftProfileRef { sketch_id: Uuid, profile_index: usize, profile_entity_ids: Option<Vec<u32>>, region: Option<ProfileRegion>, start_vertex: Option<usize>, reverse: bool }`.<br>• Add `LoftParams { profiles: Vec<LoftProfileRef>, closed: bool, ruled: bool, combine: Option<CombineMode>, targets: Option< Vec<GeomRef> > }`.<br>• Add `normalize_loft_combine(params: &LoftParams) -> EffectiveCombine` (`None => CombineMode::NewBody`, identical to `normalize_sweep_combine` at `types.rs:1358-1374`).<br>• Add `Operation::Loft { params: LoftParams }`, `KnownOperation::Loft { params: LoftParams }`, and `"Loft"` to `OPERATION_TAGS` (`types.rs:762-780`). |
| `crates/feature-engine/src/rebuild.rs` (`:490-531, 1324-1358, 1516-1599`) | • In `depends_on_tree_position` (`:490`): include `Operation::Loft`, checking `positional_targets` and whether *any* `p.sketch_id` in `params.profiles` has `!s.projected.is_empty()`.<br>• In `execute_feature` (`:1358`): resolve each `LoftProfileRef` via `current_sketch` (`:1493`), extract exact `LoftLoop` (polygon, `CircleProfile`, `arc_segments`, `spline_segments`, or `ProfileRegion`), call `execute_loft` with `ConstructSeed::from_u128(feature.id.as_u128())`, then run `dispatch_combine(kb, &eff, &combine_targets, loft_result, "loft")` + `carry_untargeted_siblings`. |
| `crates/feature-engine/src/rebuild/combine.rs` (`:137-162, 694-711`), `src/lib.rs` (`:566-576`), `src/params.rs` (`:592-630, 800-830, 1246`) | Wire `Operation::Loft` alongside `Operation::Sweep` in `find_consumed_feature_ids`, `named_custody_outputs`, `consume_target_body_id`, and parameter-expression walkers (`LoftParams` has no scalar length expressions in v1, so `params.rs` returns `false` / empty uses cleanly). |
| `crates/feature-engine/src/script/host.rs` (`:1153-1248`) & `script/interp.rs` (`:158-163`) | Expose `ctx.loft(regions_array)` and `ctx.loft(regions_array, opts_map)` (`closed`, `ruled`, `combine`, `targets`, per-section `start_vertex`/`reverse`) to Rhai custom features (`specs/custom_features_and_modeling_roadmap.md` Track A). |
| `crates/feature-engine/tests/opaque_operation.rs` (`:13-37, 162-200`) & `crates/file-format/tests/v4_document_tests.rs` (`:981-1038`) | **Fixture collision fix:** Both tests currently hardcode `"type": "Loft"` as their hypothetical *unimplemented* operation tag to test `Operation::Unknown(Value)` preservation! Rename that dummy tag in those two test files to `"Emboss"` (without deleting any test, Rule 12) and add a positive `"Loft"` round-trip test. |
| `crates/file-format/src/migrate.rs` (`:146-156`), `src/save.rs` (`:163-183`), `docs/schema/waffle-v5.schema.json` | • `migrate.rs`: add `Operation::Loft { .. } => {}` (postdates v15; all coordinates are meters by definition).<br>• **Version floor (`save.rs:163, 178`):** `FORMAT_VERSION` and `MIN_READER_VERSION` **remain `15`**. Per `docs/FILE_FORMAT.md:137, 413-422, 995` (v4 Phase 1b), unknown `Operation` tags deserialize into `Operation::Unknown(Value)` on older v15 readers, preserving the document and failing only that feature's rebuild cleanly (`UnsupportedOperation`) — the exact no-bump path used by `Pipe`, `Sweep`, `UnionAll`, and `Sketch3d`.<br>• Regenerate `docs/schema/waffle-v5.schema.json` via `UPDATE_SCHEMA=1 cargo test -p file-format --features json-schema --test schema_golden` (`crates/file-format/tests/schema_golden.rs:52`) and verify `tests/corpus_backcompat.rs`. |
| `crates/test-harness/src/assertions.rs` (`:117`) & `src/report.rs` (`:164, 405`) | Add `Operation::Loft { .. } => "Loft"` and `describe_operation` formatting. |

---

### 4.4 `crates/wasm-bridge`, MCP / Agent-Link Tools, `app` UI & Playwright GUI Tests

| File | Exact Changes |
|---|---|
| `crates/wasm-bridge/src/dispatch.rs` (`:4074`) & `src/tools/author.rs` (`:45-60`) | • `dispatch.rs`: add `Operation::Loft { .. } => "Loft".to_string()` in `operation_name` (`UiToEngine::AddFeature` / `EditFeature` already carry `Operation` polymorphically — no new wire message enum needed).<br>• `tools/author.rs`: add `"Loft"` to `AUTHORABLE` so MCP `feature_add` and `feature_edit` accept `Operation::Loft`. |
| `app/src/lib/agent/tools/authoring.js` (`:157-185`), `scripts.js` (`:21`), `engineSchemas.generated.js`, `relay/src/waffle_mcp_relay/agent-tools.manifest.json` | Document `Loft` (`profiles`, `closed`, `ruled`, `start_vertex`, `reverse`, `combine`) in `feature_add` tool prose and `ctx.loft` in `scripts.js`; regenerate `engineSchemas.generated.js` and the MCP relay manifest via `node app/scripts/gen-agent-manifest.mjs` (`app/scripts/gen-agent-manifest.mjs:7-31`). |
| `app/src/lib/engine/store.svelte.js` (`:1370-1550, 4822-5545, 6172, 10484`) & `app/src/lib/sketch/InactiveSketchRenderer.svelte` (`:685`) | • Add `loftDialogState` (`showLoftDialog`, `hideLoftDialog`, `showLoftDialogForEdit`, `applyLoft`, `addLoftProfile`, `removeLoftProfile`, `reorderLoftProfile`, `cycleLoftStartVertex`, `toggleLoftReverse`), plus `'Loft'` in body-producing ops filter (`:6172`) and double-click edit router (`:10484`).<br>• Arm multi-sketch profile picking (`getProfilePickMode()?.target === 'loft'`) so clicking closed profiles/regions across `2+` inactive sketches appends them in pick order, and draw **3D connector polyline preview** (`V_{0, s_0} → V_{1, s_1} → …`) in the viewport so twists are immediately visible before Apply! |
| `app/src/lib/ui/LoftDialog.svelte` *(new)*, `Toolbar.svelte` (`:19, 348`), `FeatureTree.svelte` (`:743`), `PropertyEditor.svelte` (`:95`), `app/src/routes/+page.svelte` (`:17, 257`) | • `LoftDialog.svelte` (mirroring `ExtrudeDialog.svelte` + `PipeDialog.svelte`): ordered `Profiles (2+)` list with Up/Down reorder, Remove, per-profile `Shift Connector (↻)` (`start_vertex`) and `Flip Direction (⇄)` (`reverse`) buttons, `Ruled` toggle, `Closed loop` toggle (enabled when `profiles.length >= 3`), and `Combine` (`New Body` default, `Add` / `Cut` / `Intersect` + `Targets`).<br>• Add Toolbar `Loft` button (`data-testid="toolbar-btn-loft"`), FeatureTree icon & context-menu Edit, PropertyEditor summary, and mount `<LoftDialog />` in `+page.svelte`. |
| `app/tests/gui/loft-dialog.spec.js` *(new)* & `app/tests/gui/helpers/toolbar.js` | Add `clickLoft(page)` helper and end-to-end Playwright suite obeying `CLAUDE.md:200, 360-373`:<br>1. **Both sketch drawing click styles (`CLAUDE.md:200, 364-365`):** author bottom and offset-plane top profiles using **click-click** (`drawLine` / `drawCircle`) in one workflow test AND **click-drag** (`dragLine` / `dragRect`) in a paired workflow test (never `__waffle.addSketchEntity` for drawing workflows; API setup allowed only in pure dialog-state unit tests per `pipe-dialog.spec.js:5-8`).<br>2. Dialog gates (`Apply` disabled with `< 2` profiles, `Closed` disabled with `< 3` profiles), profile reorder/remove, deliberate twist via `Shift Connector` vs untwisted default, `Ruled` vs smooth `3`-section loft, double-click feature edit round-trip, loud error toast when attempting `Cut`/`Add` on a `BSpline` loft (`D14`), Cancel/Escape, and `collectCrashErrors(page)` + `expectNoAnyCrash(crashes)`. |

---

## 5. Increments `L1…L7` & Verification Oracles

Every increment is independently landable, keeps the workspace green, and carries mathematical + topological + third-party (`truck`) oracles (no magic-number visual-only checks):

| Increment | Scope | Quantitative Oracles & Acceptance Tests |
|---|---|---|
| **`L1` — `kernel-v2` NURBS Foundation & Surface/Curve Plumbing** | `BSplineCurveData` / `BSplineSurfaceData` in `arena.rs`; P&T algorithms (`A2.1–A2.3`, `A4.4`, `A5.4`, `A5.6`, `A5.7`, `A5.9`, `A9.1`) in `geom/bspline.rs`; all non-constructor `kernel-v2` match sites (`validate`, `tessellate`, `mass`, `step_export`, `to_yang` `NotSupported`, `signature`, `introspect`, `transform`, `measure`, `projection`). | 1. **P&T textbook test vectors:** exact evaluation & `A4.4` 1st/2nd derivatives checked against finite differences (`< 1e-8`) and P&T worked examples (`§4.5`, `§5.2`, `§5.3`, `§5.5`, `§9.2`).<br>2. **Exact Gauss-Legendre volume oracle:** synthetic closed Bézier/NURBS test solids match closed-form integral volume to `< 1e-14` relative (`polynomial`) and `< 1e-11` (`rational` sphere/cylinder/cone written as NURBS).<br>3. **Affine invariance oracle:** `mass_properties(transform_solid(S, R, t)).volume == mass_properties(S).volume` to `< 1e-13` and `mirror_solid` preserves positive outward volume.<br>4. **`truck` STEP oracle (`step_export_roundtrip.rs`):** exported `B_SPLINE_SURFACE_WITH_KNOTS` & `RATIONAL_B_SPLINE_SURFACE` parse cleanly via `step_import::parse_step` (`truck-stepio`) with matching face count, bbox, and volume! |
| **`L2` — 2-Section Ruled Loft (`m = 2`) + Analytic Downcasting** | `construct::loft` for `m = 2` equal-count polygons, circles, and arc-bearing loops (parallel & tilted planes), `start_vertex` / `reverse` twist control, and exact downcasting to `Plane` / `Cylinder` / `Cone` (§2.3). | 1. **Prismatoid exact volume oracle (works on twisted hyperbolic-paraboloid lofts!):** For *any* 2-section ruled loft between parallel planes `z = 0` and `z = h` — **including a square rotated by `30°` or `45°` whose 4 lateral faces are non-planar hyperbolic paraboloids** — horizontal cross-sectional area `A(z)` is quadratic in `z`, so Simpson's prismatoid law $V = \frac{h}{6}(A_0 + 4 A_{h/2} + A_h)$ is **algebraically exact**! Verify `mass_properties` matches $\frac{h}{6}(A_0 + 4 A_{h/2} + A_h)$ to `< 1e-13` relative.<br>2. **Topology & mesh:** `validate_solid` passes (Checks 1–6, `V - E + F = 2`), every render-mesh edge shared by `== 2` triangles (`chord_bound <= d_eps`).<br>3. **Analytic downcast + Boolean gate:** untwisted square frustum & coaxial circular frustum emit `Surface::Plane` / `Surface::Cone` and **succeed** in `boolean_subtract` / `boolean_union`; `30°`-twisted square loft emits `Surface::BSpline` and returns `KernelError::NotSupported` on boolean; both pass `truck` STEP round-trip. |
| **`L3` — Multi-Section Smooth Skinning (`m ≥ 3`) & Closed Ring Lofts** | P&T §10.3 skinning (`q = min(3, K)`, global shared `(\bar{v}_k, V)` Eq. `10.8`, `ruled: false` smooth vs `ruled: true` piecewise, `closed: true` periodic rings, rational circle/arc skinning + positive-weight check). | 1. **Linear-reproduction oracle:** `m = 4` collinear homothetic squares or coaxial linearly-tapered circles produce the exact frustum/cone volume $V = \frac{h}{3}(A_0 + A_3 + \sqrt{A_0 A_3})$ to `< 1e-12`.<br>2. **Smooth-skinning calculus oracle:** 3-section barrel (`r_0 = 10, r_1 = 15, r_2 = 10 mm` at `z = 0, 15, 30 mm`, quadratic `q = 2` in `z, r`): exact analytical volume of revolution $V = \pi \int_0^1 r(v)^2 z'(v)\,dv$ matches `mass_properties` to `< 1e-11` and `truck` STEP round-trip mesh volume within `1e-2` (`step_export_roundtrip.rs:216`).<br>3. **Closed ring oracle (`closed: true`):** 4 squares arranged around a `Z`-axis ring give `0` planar caps, genus `G = 1` (`V - E + F = 0` in `validate_solid` Check 6), watertight longitudinal + periodic seams, and exact Pappus/quadrature volume. |
| **`L4` — Unequal Segment Counts & Single-Hole Hollow Lofts** | Stroud §13.3 `[0, 1]` arc-length vertex co-refinement (`n_a ≠ n_b`), 4-quadrant circle-to-polygon splitting (round-to-square duct!), spline section segments, and 1-hole hollow lofts (`holes.len() == 1`). | 1. **Square-to-triangle & rectangle-to-hexagon prismatoids:** exact volume $V = \frac{h}{6}(A_0 + 4 A_{h/2} + A_h)$ to `< 1e-12`, `V - E + F = 2`, watertight mesh.<br>2. **Round-to-square adapter (`Circle` at `z = 0` → `Square` at `z = h`):** emits 4 rational degree-`(2, 1)` `Surface::BSpline` faces + 2 `Plane` caps; verify positive weights `w_{i,j} \in [\sqrt{2}/2, 1]`, exact prismatoid volume ($A(h/2)$ of the rounded-square midsection), and `truck` STEP `RATIONAL_B_SPLINE_SURFACE` round-trip!<br>3. **Hollow loft oracle:** concentric outer + inner profiles satisfy $V_{\text{hollow}} = V_{\text{outer}} - V_{\text{inner}}$ to `< 1e-12` and Euler–Poincaré `V - E + F - R = 2(1 - 1) = 0` (`R = 2` cap inner rings, genus `G = 1`, `validate.rs:33`). |
| **`L5` — Trait, Ops, Engine, File Format, Script & MCP Tools** | `waffle-types::Kernel::loft` + `MockKernel::loft` → `modeling-ops::execute_loft` → `feature-engine` (`Operation::Loft`, `rebuild`, `combine`, Rhai `ctx.loft`, `opaque_operation.rs` fixture rename) → `file-format` (`schema_golden`, `v4_document_tests.rs`, `corpus_backcompat`) → `wasm-bridge` (`AUTHORABLE`, `operation_name`) → `gen-agent-manifest.mjs`. | 1. **`MockKernel` unit suite (`modeling-ops` & `feature-engine`):** open/closed role assignment (`EndCapNegative`, `EndCapPositive`, `SideFace`), `NewBody` default vs `Add`/`Cut` combine custody, `ConstructSeed` PID stability across unrelated upstream edits, `.waffle` JSON round-trip, `schema_golden`, and `corpus_backcompat`.<br>2. **`KernelV2Adapter` integration suite (`crates/test-harness/tests/loft_kv2.rs`):** sketch → datum plane → sketch → loft → sketch-on-cap → extrude workflow; parametric sketch dimension edit updates loft volume; Rhai `ctx.loft([r1, r2, r3])` custom feature test; MCP `feature_add` authoring + rollback-on-error test. |
| **`L6` — `LoftDialog.svelte` UI, Viewport Connector Preview & Playwright Suite** | `LoftDialog.svelte`, `store.svelte.js` multi-sketch profile picker + live 3D connector lines, `Toolbar`, `FeatureTree`, `PropertyEditor`, and `app/tests/gui/loft-dialog.spec.js`. | Playwright GUI tests passing with zero crashes (`expectNoAnyCrash`): both **click-click** and **click-drag** sketch-to-loft e2e workflows (`CLAUDE.md:200, 364`), 2-profile & 3-profile lofts, connector shift twist/untwist, `Ruled` & `Closed` toggles, double-click edit persistence, and loud UI error banner when combining a `BSpline` loft into another body. |
| **`L7` — `yang-rs` Parametric / NURBS Booleans *(rides on roadmap F5, which resolves D14)*** | Loft surfaces enter `yang-rs` through F5's generic parametric-surface interface (per the §3.3 stage table); N94's p-curves re-adjudicated there. | Remove `to_yang.rs` `NotSupported` gate; boolean union/subtract/intersect on twisted bilinear, rational round-to-square, and cubic skinned lofts pass `validate_solid`, `validate_boolean_output_self_intersection`, and exact Gauss-Legendre volume checks (`V(A \cup B) + V(A \cap B) = V(A) + V(B)`). |

---

## 6. Open Questions for the User

> **Decisions (user, 2026-10-27).** Put to the user in prose: start loft now
> alongside the faithfulness work; include L4 (unequal segment counts,
> round-to-square, single-hole) in v1, ordered L1 → L2 → L3 → L5 → L6 → L4;
> take this section's recommendations on degree (fixed `q = min(3, m − 1)`,
> no degree knob), parameterization (P&T averaged chord-length, Eq. 10.8),
> drawing silhouettes (`ProjectionDeclines` for `Surface::BSpline` interiors
> in L1) and boolean scope (Option (a) until F5 lands — loud `NotSupported`
> on a `Surface::BSpline` boolean operand, L7 rides on F5); and one change:
> **no automatic centripetal retry** — a non-positive weight or a failed
> chord-length interpolation is a loud typed refusal (P9/P10: one rule, no
> fallback path). The user answered, verbatim: *"agree"*. The B-spline
> algorithms of L1 live in `crates/cad-primitives` (reviewer amendment
> above), so F5's non-analytic instance reuses them.

Each decision point below is stated in prose with its literature and codebase grounding and a concrete engineering recommendation (`GEMINI.md` §3).

1. **Boolean scope for v1 (`D14` boundary and disjoint multi-body combines):**
   In `docs/yang_deviations.md:3767-3784`, `D14` records `yang-rs`'s analytic-only surface vocabulary as `DELEGATED` debt slated for Milestone F7 of `docs/yang_faithfulness_roadmap.md`, while Yang et al. 2025 (`refs/text/yang2025_hybrid_boolean.txt:304-305`) formulate the full pipeline over NURBS patches (§3.3). Under our recommended Option (a), `Loft` defaults to `CombineMode::NewBody`, lofts whose faces all analytically downcast to `Plane`/`Cylinder`/`Cone` (§2.3) work in booleans immediately, and any boolean where an operand carries `Surface::BSpline` returns a loud typed `KernelError::NotSupported` at `crates/kernel-v2/src/boolean/to_yang.rs:48`. One subtle edge case exists: if a user runs `UnionAll` (`specs/b4_balanced_union.md`) on a multi-body tab containing a `Surface::BSpline` loft that is **spatially disjoint** from all other bodies, `boolean_op` currently converts both solids via `to_yang_brep_indexed` *before* the AABB disjointness check (`crates/kernel-v2/src/boolean/mod.rs:193-196`). **Recommendation:** Approve Option (a) for v1 and keep the uniform `to_yang.rs` `NotSupported` refusal even for spatially disjoint `BSpline` bodies (so boolean support is a predictable property of the surface type rather than breaking suddenly when a parameter edit moves two parts into contact), deferring Option (b) to Milestone F7.

2. **Longitudinal parameterization in `m ≥ 3` skinning *(reviewer note: the automatic centripetal retry on a non-positive weight is a fallback path — P9/P10 prefer one rule and a loud refusal; decide at L3 with a measured case, default to refusing)* (Chord-Length vs. Centripetal):**
   Piegl & Tiller `§10.3` (`refs/text/piegl_tiller1997_nurbs_book.txt:20028-20038`, Eq. `10.8`) average **chord-length** parameters (`Eq. 9.5`, `:15699-15707`) across section control-point columns, while noting in `§9.2.1` (`:15709-15718, 15838-15839`, Eq. `9.6`, Fig. `9.7`) that **centripetal** parameterization (`\|Q_k - Q_{k-1}\|^{1/2}`) suppresses overshoot/looping when adjacent section spacings vary sharply (e.g. two closely spaced control sections followed by a long span). **Recommendation:** Default to Piegl & Tiller's averaged chord-length (`Eq. 10.8`) for standard CAD parity, automatically fall back to centripetal (`Eq. 9.6`) only if rational 4D interpolation produces a non-positive weight (`§2.2`), and omit a user-facing parameterization knob in v1 unless user testing reveals oscillation on unevenly spaced sections.

3. **Maximum longitudinal degree (`q = min(3, m - 1)`) vs. user-configurable degree:**
   Piegl & Tiller `§10.3` (`refs/text/piegl_tiller1997_nurbs_book.txt:20025`) require `q ≤ K = m - 1` (commonly cubic `q = 3`), whereas FreeCAD/OCCT expose `MaxDegree` (default `5`). Higher degrees (`q = 4, 5`) increase Runge-like wiggle on uneven sections and raise the exact Gauss-Legendre volume quadrature order (`\lceil 3q/2 \rceil`). **Recommendation:** Fix smooth loft degree at `q = min(3, m - 1)` (linear for `2` sections, quadratic `C^1` for `3`, cubic `C^2` for `≥ 4`) alongside the boolean `ruled` toggle (`q = 1` piecewise) for v1, without exposing raw degree integers in `LoftParams`.

4. **Inclusion of `L4` (unequal segment counts & round-to-square transitions) in v1 vs. v1.1:**
   Stroud `§13.3` (`refs/text/stroud2006_brep_modelling_techniques.txt:14638-14671`) highlights circle-to-square and unequal-polygon matching as canonical loft use cases (e.g. HVAC adapters, nozzles, ergonomic handles), yet B6 `Sweep` (`specs/b6_general_sweep.md` §2) shipped polygon-only sections first and deferred arc/hole sections to a follow-up increment (`S4`). Because our increment plan isolates equal-count ruled (`L2`) and smooth (`L3`) lofts from unequal-count / single-hole co-refinement (`L4`), `L5`–`L6` (engine + UI) can either ship right after `L3` (refusing unequal counts & holes typed at first) or wait for `L4`. **Recommendation:** Include `L4` inside the v1 milestone before declaring Loft feature-complete — round-to-square transitions are one of the primary reasons users choose Loft over Extrude/Sweep — while keeping `L2 → L3 → L5 → L6 → L4` (or `L1 → L2 → L3 → L4 → L5 → L6`) each independently mergeable.

5. **2D Technical Drawing silhouettes (`KernelProjection`) on `Surface::BSpline` faces:**
   In `crates/kernel-v2/src/projection.rs:24-30` and `crates/kernel-v2/src/projection/silhouette.rs:221-296`, `Curve::BSpline` boundary edges project cleanly as render-density polylines from day one (`L1`), whereas interior smooth horizon silhouettes on curved faces currently rely on closed-form analytic loci (`Ruling`, `Circle`, torus `Branch` at `silhouette.rs:304-327`) with an explicit `ProjectionDeclines` diagnostic channel (`silhouette.rs:226`). For a freeform `Surface::BSpline`, interior silhouettes are the zero-contour `f(u, v) = (S_u(u, v) \times S_v(u, v)) \cdot w = 0` marched across Bézier sub-patches. **Recommendation:** Record `Surface::BSpline` faces in `ProjectionDeclines` in `L1` (so boundary edges draw accurately and any missing interior horizon line is surfaced honestly rather than silently omitted), and add marching-squares + bisection contour extraction for `(S_u \times S_v) \cdot w = 0` as a small dedicated sub-increment (`L6b`) once `L1`–`L6` land.
