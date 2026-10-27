# Spec: Yang 2025 §4.2.2 Closed-Form Gauss-Map Overlap Filter (F4 / N92 / M-4)

**Status:** Design note for roadmap item F4 (ledger **N92**, compliance matrix finding **M-4**, rows **4.2-12**, **4.2-13**, **5.6-02**).
**Scope:** Closed-form conservative Gauss-map estimation and overlap filtering for the five analytic surfaces in `yang-rs` (`Plane`, `Cylinder`, `Cone`, `Sphere`, `Torus`). The rational Bézier control-net / Theorem 4.1 / Appendix B normal-cone construction (`4.2-14..16`, `B-01..03`) remains out of scope under **D14**.

---

## 1. Paper Basis

- **§4.2.2 Redundant computation reduction** (`refs/text/yang2025_hybrid_boolean.txt:440-482` right col; `:498-503` left col after Fig. 10):
  - `:440-446` (R): *"We use the normal vectors of the areas that have potential intersections to improve the detection accuracy. Given that within a small intersection loop, there must exist two points $\mathbf{p}_A \in S_A$ and $\mathbf{p}_B \in S_B$ such that the normal vector of $S_A$ at $\mathbf{p}_A$ is collinear with that of $S_B$ at $\mathbf{p}_B$ [Sederberg et al. 1989], we conservatively estimate the Gauss map of the corresponding region on each surface and check if two Gauss maps overlap."*
  - `:466-470` (R): *"Our Gauss map overlapping check reduces the computation and optimization for nonexistent intersections, thus reducing the number of invalid intersections detected by 40.7% and accelerating the subsequence optimization of intersections by an average of 34.9% on our dataset."*
  - `:498-505` (L): Surviving potential-intersection areas feed seed selection (*"the barycenter of $\triangle t_A$ and its closest point on $M_B$"*) for *"the accurate small intersection loop or a single tangential point if the intersection exists"*, with redundant points on already-solved intersection curves removed.
- **§5.6 Detecting small loops** (`:1033-1035` right col): potential-intersection regions on non-intersecting meshes are identified *"based on the normal estimation and distance evaluation shown in Fig. 9"*.

### Collinearity (`n_A = ±n_B`), not oriented equality

Sederberg et al. 1989's criterion in `:442-444` requires **collinearity** ($\mathbf{n}_A \times \mathbf{n}_B = \mathbf{0}$, i.e. $\mathbf{n}_A = +\mathbf{n}_B$ or $\mathbf{n}_A = -\mathbf{n}_B$), not $\mathbf{n}_A = +\mathbf{n}_B$ alone. For example, two spheres kissing or forming a tiny external lens around $(0, 0, R)$ have antipodal outward normals $\mathbf{n}_A \approx +\hat{\mathbf{z}}$ and $\mathbf{n}_B \approx -\hat{\mathbf{z}}$. Testing $G_A \cap (G_B \cup (-G_B)) \neq \emptyset$ also makes the overlap predicate invariant to `BRepFace.reversed`.

---

## 2. Representation and Overlap Test (`crates/yang-rs/src/gauss_map.rs`)

A `GaussMap` conservatively bounds a subset of $S^2$ as a stack-allocated (`Copy`, zero-heap) union `GaussMap { count, items: [GaussRegion; 4] }` of up to 4 `GaussRegion`s, with whole-sphere coverage $S^2$ represented uniformly by `GaussMap::FULL` ($\theta_{\text{cap}} = \pi$, $\vartheta \in [0, \pi]$; emitted for degenerate / non-finite input, a triangle touching a sphere center, or a double-nappe cone straddle).

Each `GaussRegion` is the intersection on $S^2$ of two closed elementary primitives:

1. **Spherical cap (circular half-cone):**
   $$\mathcal{C}(\hat{\mathbf{d}}, \theta_{\text{cap}}) = \{\mathbf{n} \in S^2 : \angle(\mathbf{n}, \hat{\mathbf{d}}) \le \theta_{\text{cap}}\}, \quad \theta_{\text{cap}} \in [0, \pi].$$
2. **Axial spherical zone (polar-angle band):**
   $$Z(\hat{\mathbf{a}}, \vartheta_{\min}, \vartheta_{\max}) = \{\mathbf{n} \in S^2 : \vartheta_{\min} \le \angle(\mathbf{n}, \hat{\mathbf{a}}) \le \vartheta_{\max}\}, \quad 0 \le \vartheta_{\min} \le \vartheta_{\max} \le \pi.$$

### Conservative two-region overlap

For `sign` $\in \{+1, -1\}$, $R_A$ and $\text{sign} \cdot R_B$ can share a unit vector only if **both** their caps overlap and their zones overlap:

1. **Cap overlap:** With $\mathbf{d}'_B = \text{sign} \cdot \hat{\mathbf{d}}_B$, compute the great-circle angle via Kahan's formula
   $$\gamma = 2 \operatorname{atan2}(\|\hat{\mathbf{d}}_A - \mathbf{d}'_B\|,\; \|\hat{\mathbf{d}}_A + \mathbf{d}'_B\|) \in [0, \pi],$$
   which avoids `acos`'s $\sim \sqrt{\varepsilon_{\text{mach}}}$ loss near $0$ and $\pi$. Require $\gamma \le \theta_A + \theta_B + \delta_{\text{fp}}$.
2. **Zone overlap:** With $[\beta'_0, \beta'_1] = [\beta_0, \beta_1]$ for $\text{sign} = +1$ and $[\pi - \beta_1, \pi - \beta_0]$ for $\text{sign} = -1$, let $\psi = 2 \operatorname{atan2}(\|\hat{\mathbf{a}}_A - \hat{\mathbf{a}}_B\|, \|\hat{\mathbf{a}}_A + \hat{\mathbf{a}}_B\|) \in [0, \pi]$. By the spherical triangle inequality, $\angle(\mathbf{n}, \hat{\mathbf{a}}_B)$ over $\mathbf{n} \in Z(\hat{\mathbf{a}}_A, \alpha_0, \alpha_1)$ spans the exact closed interval
   $$\bigl[\operatorname{dist}(\psi, [\alpha_0, \alpha_1]),\; \pi - \operatorname{dist}(\pi - \psi, [\alpha_0, \alpha_1])\bigr].$$
   Require this interval to intersect $[\beta'_0, \beta'_1]$ within $\delta_{\text{fp}}$.

Here $\delta_{\text{fp}} = 64\,\varepsilon_{\text{mach}} \approx 1.42 \times 10^{-14}\text{ rad}$ is an IEEE-754 rounding guard (never a geometric tolerance) accounting for frame normalization, `atan2`/`asin`, and Kahan angle evaluation so boundary-tangent regions never produce a false negative.

---

## 3. Closed-Form Bounds per Analytic Surface

Both parameter-domain rectangles (`GaussMap::from_uv_rect`, in [`stage4_dt::eval_uv`](../crates/yang-rs/src/stage4_dt.rs)'s `ortho_basis` frame) and 3D mesh triangles / triangle sets (`GaussMap::from_triangle`, `GaussMap::from_triangles`) use closed-form bounds over `Surface`:

1. **`Plane { normal, .. }`:**
   Single normal $\hat{\mathbf{n}}$: $\mathcal{C}(\hat{\mathbf{n}}, 0) \cap Z(\hat{\mathbf{n}}, 0, 0)$.

2. **`Cylinder { axis_point, axis_dir, radius }`:**
   Normals lie on the equator $\perp \hat{\mathbf{a}}$: $\mathbf{n}(u) = \cos u\,\mathbf{e}_1 + \sin u\,\mathbf{e}_2$, so $Z(\hat{\mathbf{a}}, \frac{\pi}{2}, \frac{\pi}{2})$ is exact.
   - Over $[u_0, u_1]$: if $\Delta u \ge 2\pi$, $\theta_{\text{cap}} = \pi$; otherwise split into $\le 2$ sub-arcs of width $\le \pi$, each bounded tightly by $\mathcal{C}(\hat{\mathbf{r}}(u_m), \Delta u_{\text{sub}} / 2)$.
   - From 3D triangle $T$: project vertices onto $(\mathbf{e}_1, \mathbf{e}_2)$. If $\operatorname{dist}((0,0), T_{2\text{D}}) \le 10^{-12} \max(R, \rho_{\max})$ (axis touches/pierces $T$), $[u_0, u_1] = [-\pi, \pi]$; else $T_{2\text{D}}$ lies in an open half-plane through $(0,0)$, so unwrapping $\theta_1, \theta_2$ to the nearest $2\pi$-image of $\theta_0$ yields the unique exact wedge $[u_0, u_1]$ of span $< \pi$ (seam-invariant across $u = \pm\pi$).

3. **`Cone { apex, axis_dir, half_angle: α }`:**
   Away from the apex on the $+h$ nappe ($h = (\mathbf{x} - \mathbf{a}_0)\cdot\hat{\mathbf{a}} > 0$), $\mathbf{n}(u) = \cos\alpha\,\hat{\mathbf{r}}(u) - \sin\alpha\,\hat{\mathbf{a}}$ traces the small circle $\angle(\mathbf{n}, \hat{\mathbf{a}}) = \frac{\pi}{2} + \alpha$.
   - Over a sub-arc $[u_a, u_b]$ with $\Delta u = u_b - u_a \le \pi$: $\mathbf{n}(u)$ has constant chord distance to $\mathbf{n}(u_m)$, giving exact half-angle
     $$\theta_{\text{cap}} = 2\arcsin\!\bigl(\cos\alpha \sin(\Delta u / 4)\bigr), \quad Z\bigl(\hat{\mathbf{a}}, \tfrac{\pi}{2} + \alpha, \tfrac{\pi}{2} + \alpha\bigr).$$
     (Symmetric on the $-h$ nappe with $+\sin\alpha\,\hat{\mathbf{a}}$ and $\vartheta = \frac{\pi}{2} - \alpha$.)
   - **Apex straddle / contact:** For the $+h$ solid cone $K = \{(\mathbf{r}, h) : h \ge 0, \|\mathbf{r}\| \le h\tan\alpha\}$, convex duality gives the outward normal cone at the apex $(0,0)$ as the polar cone
     $$K^\circ = \bigl\{\mathbf{n} \in S^2 : \angle(\mathbf{n}, -\hat{\mathbf{a}}) \le \tfrac{\pi}{2} - \alpha\bigr\} = \mathcal{C}(-\hat{\mathbf{a}}, \tfrac{\pi}{2} - \alpha) \cap Z\bigl(\hat{\mathbf{a}}, \tfrac{\pi}{2} + \alpha, \pi\bigr),$$
     whose boundary circle *is* the smooth $+h$ Gauss map $\{\mathbf{n}(u) : u \in [-\pi, \pi]\}$. Whenever a $+h$ region touches the apex ($v_0 \le 0$ or $\rho_{\min}\sin\alpha + h_{\min}\cos\alpha \le 0$) or a 3D triangle bridges across the cone axis ($\rho_{\min} \approx 0$, so its cone-surface cap straddles the apex), it emits $K^\circ$. Regions spanning both nappes ($h_{\min} < 0 < h_{\max}$ or $v_0 < 0 < v_1$) emit `GaussMap::Full`.

4. **`Sphere { center, radius }`:**
   - **From 3D triangle $T$ (chart- and pole-free):** Stage 1 tessellates spheres either in canonical $\hat{\mathbf{z}}$ or in rotated revolve frames (`tessellate_sphere_patch_in_frame`). Under central projection $\pi_S(\mathbf{x}) = \frac{\mathbf{x} - \mathbf{c}}{\|\mathbf{x} - \mathbf{c}\|}$, adjacent 3D mesh triangles share straight 3D chords whose $\pi_S$-images are identical great-circle arcs, tiling the sphere without gaps. If $\operatorname{dist}(\mathbf{c}, T) \le 10^{-12} R$, emit `Full`. Otherwise let $\mathbf{n}_k = \frac{\mathbf{v}_k - \mathbf{c}}{\|\mathbf{v}_k - \mathbf{c}\|}$ and pick $\hat{\mathbf{d}}$ (spherical circumcenter of $\{\mathbf{n}_k\}$ when inside the open hemisphere, else $\operatorname{normalize}(\mathbf{n}_0 + \mathbf{n}_1 + \mathbf{n}_2)$). Whenever $\min_k (\mathbf{n}_k \cdot \hat{\mathbf{d}}) \ge \cos\theta > 0$, every $\mathbf{x} = \mathbf{c} + \sum_k \lambda_k (\mathbf{v}_k - \mathbf{c}) \in T$ satisfies $w_k = \lambda_k \|\mathbf{v}_k - \mathbf{c}\| \ge 0$ and
     $$\frac{\sum_k w_k \mathbf{n}_k}{\bigl\|\sum_k w_k \mathbf{n}_k\bigr\|} \cdot \hat{\mathbf{d}} \ge \frac{(\sum_k w_k)\cos\theta}{\sum_k w_k} = \cos\theta,$$
     proving $\pi_S(T) \subseteq \mathcal{C}(\hat{\mathbf{d}}, \theta)$ regardless of pole straddles or coordinate seams. If $\min_k (\mathbf{n}_k \cdot \hat{\mathbf{d}}) \le 0$, emit `Full`.
   - **From UV rectangle $[u_0, u_1] \times [v_0, v_1]$ ($v \in [-\frac{\pi}{2}, \frac{\pi}{2}]$):** Zone $Z(\hat{\mathbf{z}}, \frac{\pi}{2} - v_1, \frac{\pi}{2} - v_0)$ is exact. Splitting $[u_0, u_1]$ into $\le 2$ spans of width $\Delta u \le \pi$, the minimax cap center on the mid-meridian $u_m$ sits at latitude $v_c = \operatorname{clamp}\bigl(\operatorname{atan2}(\sin v_m \cos(\Delta u/2), \cos v_m),\; \min(\alpha_0, \alpha_1),\; \max(\alpha_0, \alpha_1)\bigr)$ with $\alpha_i = \operatorname{atan2}(\sin v_i, \cos v_i \cos(\Delta u/2))$, and $\max_{(u,v)} \angle(\mathbf{n}(u,v), \hat{\mathbf{d}})$ is attained at the four rectangle corners via the cancellation-free half-chord formula $1 - \mathbf{n}\cdot\hat{\mathbf{d}} = 2\sin^2\frac{v - v_c}{2} + 2\cos v\cos v_c\sin^2\frac{\Delta u}{4}$.

5. **`Torus { center, axis_dir, major_radius: R, minor_radius: r }`:**
   The outward normal $\mathbf{n}(u, v) = \cos v\,\hat{\mathbf{r}}(u) + \sin v\,\hat{\mathbf{a}}$ covers all of $S^2$, reversing radial sense across the inner-equator transitions $v = \pm\frac{\pi}{2}$.
   - **UV rectangle $[u_0, u_1] \times [v_0, v_1]$:** Split $[v_0, v_1]$ (clamped to width $2\pi$) along $v \equiv \pm\frac{\pi}{2} \pmod{2\pi}$ into outer-tube intervals ($\cos v \ge 0$) and inner-tube intervals ($\cos v \le 0$). On outer intervals, $\mathbf{n}(u, v)$ is identically a sphere lat-lon patch $[u_0, u_1] \times [v_a, v_b]$ about $\hat{\mathbf{a}}$. On inner intervals, substituting $w = \pi - v_{\text{wrapped}} \in [-\frac{\pi}{2}, \frac{\pi}{2}]$ gives $\mathbf{n}(u, v) = \cos w\,\hat{\mathbf{r}}(u + \pi) + \sin w\,\hat{\mathbf{a}}$, identically a sphere lat-lon patch on $[u_0 + \pi, u_1 + \pi] \times [w_a, w_b]$ (storing up to 4 exact sub-regions in `GaussMap`).
   - **From 3D triangle $T$:** Extract $[u_0, u_1]$ from $T_{2\text{D}}$ perpendicular to $\hat{\mathbf{a}}$ (full $[-\pi, \pi]$ if $\rho_{\min} = \operatorname{dist}((0,0), T_{2\text{D}}) \le 10^{-12} R$). In the meridian half-plane, $\rho_{\min}$ is exact point-to-2D-triangle distance and $\rho_{\max} = \max_k \rho_k$ is exact by convexity of $\|\cdot\|$, so every $\mathbf{x} \in T$ maps into the convex box $B_{\rho\tau} = [\rho_{\min} - R, \rho_{\max} - R] \times [\tau_{\min}, \tau_{\max}]$. If $\operatorname{dist}((0,0), B_{\rho\tau}) \le 10^{-12} r$ (touches torus spine), $[v_0, v_1] = [-\pi, \pi]$; otherwise $B_{\rho\tau}$ lies in an open half-plane of $\mathbb{R}^2 \setminus \{(0,0)\}$, and unwrapping $\operatorname{atan2}(\tau, d)$ at the four corners of $B_{\rho\tau}$ bounds $v(\mathbf{x})$ across $v = \pm\pi$ seams and inward chord sag.

---

## 4. Wiring in `shell_contact::contact_pairs` and Relationship to `pieces_cross_point`

### Honest decision on `pieces_cross_point`: **complemented, NOT redundant**

`pieces_cross_point` (`crates/yang-rs/src/shell_contact.rs:933`) **must be kept** behind the §4.2.2 Gauss-map filter for two independent, proven reasons:

1. **False positives on parallel / facing disjoint surfaces (`N91` / `N89` item 2):**
   In Yang 2025, §4.2.2 eliminates only 40.7% of non-intersecting $2d_\epsilon$ candidates (`:466-470`); parallel or facing non-intersecting surfaces within $2d_\epsilon$ (e.g. `R0015`'s parallel planes $2 \times 10^{-7}$ apart, `shell_contact.rs:423-426`, or concentric cylinders/spheres) have **identical/antipodal normals**, so their Gauss maps overlap (`true`). In the paper, §4.3's seeded Newton solver subsequently classifies those seeds as *"no intersection"* (`:524-530`). Under `N7`/`N91`, `yang-rs` resolves every emitted `ContactPair` by §4.5.2 face refinement (`boolean::refine_loop`) rather than a seeded §4.3 solve; deleting `pieces_cross_point` would drive every parallel near-miss into `YangError::RefinementExhausted`.

2. **Trim-boundary-clipped Case-III intersections violate Sederberg 1989's closed-loop precondition (measured on `P0021`):**
   Sederberg et al. 1989 guarantees a collinear normal pair $(\mathbf{p}_A, \mathbf{p}_B)$ inside a **closed** intersection loop or at an **interior** tangency. When a sub-sagitta grazing lens is clipped by operand trim boundaries $\partial F_A$ or $\partial F_B$, the intersection is an open arc between boundary edges and **need not contain any collinear normals**.
   - Measured on `P0021` (`YANG_T6_PROBE=1 cargo test -p kernel-v2 --test p0021_case_iii_finite_extent`): the $6.16 \times 10^{-6}$ lens is clipped by the base cap rims, producing two T6 contacts at `[0.0008981, 0.0005436, 0.0014872]`:
     - `Cap_A (Plane) × Cyl_B`: `Cap_A`'s normal is $28.1^\circ$ from `Cyl_B`'s axis, so their Gauss maps are separated by **$61.9^\circ$** everywhere;
     - `Cyl_A × Cyl_B`: their Gauss maps meet only along $\pm(\hat{\mathbf{a}}_A \times \hat{\mathbf{a}}_B)/\|\cdot\|$, **$51.7^\circ$** away from the rim-clipped lens.
   Pruning trim-boundary-incident near pairs by Gauss-map overlap would falsely drop `P0021`'s Case-III contact.

### Connected-area + trim-boundary wiring in `contact_pairs`

Notice also that on a multi-triangle **interior** closed loop $\Gamma$, the curve tangent $\mathbf{n}_A \times \mathbf{n}_B$ is nonzero along $\Gamma$ itself while Sederberg's collinear-normal pair $(\mathbf{p}^*, \mathbf{q}^*)$ lies in the **interior of the disk** $D_A, D_B$ bounded by $\Gamma$ — potentially in a central triangle pair $(t^*, s^*)$ adjacent to the perimeter triangle pair $(t_\Gamma, s_\Gamma)$ where the surfaces cross (`pieces_cross_point`). By the triangle inequality and IVT, when $M_A$ and $M_B$ do not intersect anywhere over $D_A, D_B$, every triangle overlapping $D_A$ or $D_B$ has a partner within `reach = dev_a + dev_b + band`, so $(t^*, s^*)$ and $(t_\Gamma, s_\Gamma)$ belong to the **same connected $2d_\epsilon$ potential-intersection area $C$** (matching `:440-441`, *"areas that have potential intersections"*).

Accordingly, `shell_contact::contact_pairs` runs in three stages per face pair `(fa, fb)`:

1. **§4.2.1 Distance query:** Collect all triangle pairs `(t, s)` on distinct surfaces with `d_coarse = triangle_distance(t, s) <= dev_a + dev_b + band`, recording whether `d_coarse == 0.0`.
2. **§4.2.2 Connected-area Gauss-map filter:** Group near pairs on `(fa, fb)` into connected components $C$ (where `(t, s)` and `(t', s')` connect when `t, t'` share a vertex in `mesh_a` and `s, s'` share a vertex in `mesh_b`). Component $C$ is **strictly interior** iff:
   - no pair in $C$ has `d_coarse == 0.0` (the meshes do not intersect anywhere in $C$), and
   - no pair `(t, s)` in $C$ has a face-trim boundary edge of `t` (an edge of `fa` not shared by two triangles of `fa`; periodic seams have `fa` on both sides and are interior) within `reach` of `s`, or vice versa.
   For each strictly interior $C$, compute `GaussMap::from_triangle` per participating triangle in $T_A(C)$ and $T_B(C)$ and test whether $G(T_A(C))$ overlaps $G(T_B(C))$ (`∃ t ∈ T_A(C), s ∈ T_B(C)` with overlapping `GaussMap`s, accelerated by a `GaussMap::union` pre-check). If disjoint, reject every pair in $C$ without running `pieces_cross_point`.
3. **Exact leaf-crossing test (`pieces_cross_point`):** Run on surviving non-intersecting, unskipped pairs `(t, s)` to filter parallel/facing non-intersecting false positives (`N91`) and confirm boundary-clipped crossings.

*( Note: `shell_contact_census` / `pieces_may_touch` is kernel-v2's `N33` whole-shell set-algebra pre-check before Stage 2, where transversal crossings with non-collinear normals must report `Contact`; it is not a §4.2.2 site and is untouched. )*

---

## 5. Compliance Status (`N92` `PROPOSED`, Matrix `4.2-12` / `4.2-13` / `5.6-02` `PARTIAL`) and Measured Liveness

- **Why `N92` is `PROPOSED` and rows `4.2-12` / `4.2-13` / `5.6-02` are `PARTIAL`:** Yang §4.2.2 (`:440-446`, `:466-470` R) applies the Gauss-map overlap check to every potential-intersection area without stating a boundary exemption. Restricting the overlap filter to strictly interior connected areas (exempting areas touching a `d_coarse == 0` mesh crossing or within `reach` of a face-trim boundary, where Sederberg 1989's closed-loop precondition fails — e.g. `P0021` at `61.9°` and `51.7°`) is strictly more conservative than the paper (never drops an intersection the paper keeps), but diverges from the literal text and therefore awaits prose sign-off from the user under governance A15.7.
- **Measured corpus liveness & cost (`YANG_S422_GAUSS_PROBE`, 2026-10-27):** Across all 374 corpus cases (`1,307` `contact_pairs_with_stats` calls across `315` cases; `304,994` near-candidate pairs in `3,213` components: `2,778` trim-exempt / `277,710` pairs, `389` crossing-exempt / `26,590` pairs, `46` strictly interior / `694` pairs across `16` cases), **`8` cases have `gauss_rejected > 0`** (`F0082`: 8, `P0045`: 4, `P0049`: 12, `R0019`: 2, `R0025`: 3, `R0032`: 2, `R0050`: 1, `R0085`: 82 — **`114` pairs across `26` of `46` interior components, `56.5%`**, preserving all `608` contacts). Total CPU spent in `filter_candidates_by_gauss_map` across all 374 cases is **`1.271 s`** (`0.37%` of `contact_pairs`'s `347.634 s`), broken down into `tri_trim_edge_mask = 0.606 s`, component grouping = `0.656 s`, `pair_touches_trim_within_reach = 0.007 s`, and `GaussMap` evaluation/overlap = `0.001 s`.
