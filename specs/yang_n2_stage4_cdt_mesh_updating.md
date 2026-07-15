# SPEC — N2: Stage-4 §4.4.1 CDT mesh-updating (replace relocation-only)

**Status:** DESIGN (pre-code). Author: 2026-06-30. Roadmap: deviation **N2**
(`docs/yang_deviations.md`), M8 same-normal campaign **Mode 2**
(`crates/test-harness/tests/m8_samenormal_campaign.rs`). Reviewer sign-off
required before implementation (per "Fix It Right or Don't Fix It", P9/P10).

---

## 1. Paper requirement (the spec)

Yang 2025 §4.4.1 "Mesh updating" (`refs/text/yang2025_hybrid_boolean.txt:534-565`,
Fig. 11) + §4.1.2 (per-surface u-v CDT):

> "we trim and update the meshes using the intersection curves to maintain a
> correct topology, bijectivity with the corresponding surfaces, and the dₑ
> constraints. The intersection curves on the parametric surfaces are mapped to
> the meshes M_A and M_B … we set r_A = r_B = r, so that the two polylines in the
> meshes coincide with the intersection curve … through **CDT** we obtain valid
> discretizations of the trimmed meshes … The triangulation can be totally
> operated in the parametric domain, it maps boundary curves to boundary curves,
> and contains no flipping triangles since the intersection curves are regular."

Fig. 11 preprocessing before CDT: (a) locate the constraint edge containing an
intersection point q, split it at q; (b) if a split-edge endpoint p is too close
to q, **merge** p with q; (c) if an intersection loop has no interior mesh
vertex, **insert** one. §4.4.3: watertightness is *inherited* from the mesh
boolean and the topology after updating matches the intended B-Rep — it is NOT
re-derived per facet.

**The operative requirement:** each surface patch trimmed by an intersection
curve is **re-triangulated by CDT in the surface's parametric domain**, with the
intersection curve as a constraint and the patch boundary mapped to boundary —
*not* by moving the boolean mesh's vertices in place.

## 2. Current implementation & why it fails (N2)

`stage4_relocate_and_correct` (`crates/yang-rs/src/lib.rs:7839`) does **relocation
in place**: it moves each mesh intersection vertex onto the exact analytic curve
(`project_onto_circle` / cylinder param), runs the §4.5.3 reversed-point sweep
(`sweep_reversed_intersections:9372`), then a **validity gate**
(`validate_relocated_triangles:9713`) that loudly STOPs `DegenerateTriangle` when
any triangle incident to a moved vertex drops below `MIN_FEATURE_SIZE²`. It never
**re-triangulates**.

Mode-2 failures (instrumented, `YANG_RELOC_PROBE=1`):

| case | triangle | cause | 2·area |
|------|----------|-------|--------|
| R0021 | [133,75,131], all 3 moved | three vertices **monotonic-collinear** on the plane∩cylinder generator **line** (params 0, 1×, 2.94×) → a triangle that *spans* the constraint curve | 3.8e-20 |
| R0072 | [7,11,8], only v11 moved | the two **unmoved** verts v7,v8 are near-coincident (Δ≈1.3e-7 at scale 5.5e-4) → minted-duplicate junction on the cylinder | 1.5e-12 |

Neither is reachable by the §4.5.3 sweep: it is gated `all_conic` (Circle/Ellipse,
explicitly **excludes** `LineSegment`, `lib.rs:9418-9429`) and these intersection
curves are plane∩cylinder generator **lines**; and even if included, `is_reversed`
fires only on U-turns (degenerate `t̃`), not monotonic-collinear points. **The
harness note "§4.5.3 region repair" is therefore optimistic** — the faithful fix
is §4.4.1 CDT re-triangulation, of which the §4.5.3 sweep is only one
preprocessing piece (the reversal correction, Fig. 11 not-shown).

P9/P10 boundary: a tolerance-gated edge-collapse of the sliver is **prohibited** —
R0021's shortest edge is 0.0013 (a *real* edge at model scale ~0.2), so collapsing
it moves neighbor geometry ~0.6% and yields a watertight-but-wrong mesh (the exact
silent-wrong this campaign exists to catch).

## 3. Design: per-patch parametric-domain re-triangulation

Replace the relocate-in-place flow with **trim-and-remesh** per affected patch,
faithful to §4.4.1. Operate on the *combined* boolean-output mesh (where A- and
B-attributed patches already **share** intersection-curve vertices, so D5
`r_A=r_B=r` conformality is structural, not a separate weld).

### 3.1 Pipeline (replaces steps 3-4 of `stage4_relocate_and_correct`)

1. **Relocate boundary onto the exact curve** (unchanged): move each intersection
   vertex onto its analytic `Curve` (circle/ellipse/**line**) — already done in
   steps (1)-(2). This fixes the SHARED boundary vertices for all patches.
2. **Identify affected patches.** From `compute_phase_a` (`PatchInfo.cycles`),
   select every patch whose boundary cycle contains ≥1 relocated (intersection)
   vertex. Each patch has a single analytic `Surface` (its attribution).
3. **Project to the surface's parametric domain.**
   - `Surface::Plane` → `ortho_basis(normal)` 2D frame (reuse `project_loop_2d`,
     `lib.rs:1730` — the SAME frame Stage-1 CDT uses, so a re-meshed planar patch
     is frame-consistent with its un-remeshed neighbors).
   - `Surface::Cylinder` → `(θ, z)` in the `ortho_basis(axis)` frame (the same
     parameterization Stage-1 uses at `lib.rs:1413-1423`). **Increment-gated**
     (see §5): the planar case lands first.
4. **Build the CDT constraint set in 2D:**
   - the patch boundary cycle(s) (outer + holes) as hard constraints, **vertices
     fixed, never subdivided** (preserves conformality with un-remeshed
     neighbors and the other solid's patch across the shared curve);
   - any intersection-curve chain crossing the patch *interior* as **interior
     constraint edges** (its endpoints are the shared relocated vertices);
   - §4.4.1(c): if a resulting region would have **no interior vertex**, insert
     one strictly-interior Steiner point (safe — interior points are not shared,
     so they cannot break conformality).
5. **CDT → triangles → lift to 3D.** Run CDT; map each output 2D vertex back to
   3D via the surface's exact `eval` (planar: inverse `ortho_basis`; the boundary
   vertices map back to their existing 3D positions bit-for-bit). Replace the
   patch's triangles in `mesh.tris` + `attribution` in lockstep.
6. **Re-gate** `validate_relocated_triangles` + `check_watertight_2manifold`
   (§4.4.3). A patch that still yields a degenerate/non-watertight result after a
   faithful CDT is a genuine `LocalRefinementRequired` STOP (§4.5.2), not papered.

### 3.2 CDT capability gap & decision

`cdt_polygon_with_holes` (`crates/cherchi-rs/src/triangulation/mod.rs:102`) is
**boundary-only**: outer + holes as constraints, **no interior constraint edges,
no Steiner points** (`mod.rs:77,213`). §4.4.1 needs both: an interior constraint
chain (the curve cutting through the patch) and the (c) interior Steiner insert.

The backend is spade's `ConstrainedDelaunayTriangulation`, which **supports both**
(`add_constraint` on any edge; `insert` of interior points). So this is a
*wrapper* gap, not an engine gap.

**DECISION (recommend):** add a sibling entry point in cherchi-rs, e.g.
`cdt_polygon_with_interior_constraints(verts, outer, holes, interior_edges, steiner_pts)`,
that reuses the existing validation + deterministic-order machinery and adds
(a) interior constraint segments via the same `can_add_constraint`/`add_constraint`
guard, (b) optional interior Steiner points. Keep the existing boundary-only
function byte-identical (Stage-1 + the YR25 overlay depend on it). This is the
minimal, layering-clean extension (the new capability lives in cherchi-rs, the
CDT owner — not improvised in yang-rs).

### 3.3 Conformality invariant (D5 `r_A = r_B = r`, the watertight key)

The single combined mesh means A's and B's patches **share** the intersection-curve
vertices. The re-mesh MUST:
- never subdivide a SHARED boundary edge (no Steiner on the curve or any patch
  boundary), so both sides keep identical samples → watertight by construction;
- only add Steiner points STRICTLY interior to a single patch.

This is enforced structurally (boundary vertices passed fixed; Steiner flagged
interior) and verified by `check_watertight_2manifold` after each patch remesh.

## 4. Acceptance criteria

- `red_r0021_stage4_relocation` and `red_r0072_stage3_ambiguous_parallel_lines`
  reach **oracle-correct** (watertight, Euler χ=2, volume, bbox, single body) and
  are un-`#[ignore]`d.
- **Assay (`assay_kv2 -- --ignored`): 0 SUPPORTED_WRONG**, no SUPPORTED_CORRECT
  lost vs the current 80. (The silent-wrong gate — non-negotiable.)
- Campaign always-on tests stay green; `fuzz_boxes` 900/900 and the curved YR
  suites unregressed (an all-planar / no-intersection input must hit the **no-op**
  path — re-mesh only runs on patches with a relocated vertex).
- New cherchi-rs CDT entry point has its own unit tests (interior constraint
  honored; Steiner interior-only; boundary-only path byte-identical).

## 5. Decomposition (REVISED 2026-06-30 after grounding — ordered increments)

**Two findings from grounding the code/cases corrected the original plan:**

- **(A) The "interior-constraint CDT" increment is UNNECESSARY and was dropped.**
  Flood-fill patches are bounded by intersection edges (`reconstruct_topology`),
  so an intersection curve is **always a patch boundary, never interior** to a
  same-attribution patch — there is no interior constraint to add. The existing
  `cdt_polygon_with_holes` (boundary-only) + `cdt_polygon_with_holes_refined`
  (interior Steiner via spade area-refinement, already shipped for KV6d torus)
  cover every CDT need below. Adding an unused function would also violate
  cherchi-rs demand-driven rule #8.
- **(B) The two Mode-2 cases are on DIFFERENT surfaces** (`YANG_RELOC_PROBE`,
  attribution surface dump): **R0072 is on a `Plane`** (near-coincident pair,
  Δ=1.3e-7 < MIN_FEATURE_SIZE=1e-6) → a §4.4.1(b) **merge**, NOT a remesh;
  **R0021 is on a `Cylinder`** (r=0.040, monotonic-collinear sliver) → a curved
  `(θ,z)` re-CDT. So they are NOT the same increment.

Revised order:

1. **N2-1 — §4.4.1(b) sub-feature vertex merge (closes R0072).** When a relocated
   triangle is degenerate AND its shortest edge is `< MIN_FEATURE_SIZE` (the
   governance feature floor A14.2 — two points nearer than the smallest
   representable feature ARE the same point; principled, not a tuned tolerance),
   edge-collapse that pair via the watertight-preserving `collapse_vertex`,
   iterating to a fixed point, before `validate_relocated_triangles`. This is
   "Stage-4 owns junction-duplicate collapse" for curved inputs (the I6 near-weld
   is bit-exact-only for curved). A genuinely-spread degenerate (R0021, edge
   0.0013 ≫ floor) is untouched → still a loud STOP. *Small, low-risk, self-
   contained; the natural first increment.*
2. **N2-2 — CYLINDER `(θ,z)` patch re-CDT (closes R0021).** Re-triangulate a
   degenerate cylinder patch in its `(θ, z)` parametric domain (`ortho_basis(axis)`,
   the Stage-1 frame; seam-wrap aware) with `cdt_polygon_with_holes` (boundary
   fixed, conformal) — `_refined` if a curved patch needs chord-bounded Steiner.
   Curved-but-non-cylinder patches keep the loud STOP. *The larger curved piece;
   its own spec section / review before coding.*
3. **N2-3 — sphere/cone patch re-CDT** (same parametric-domain pattern).
4. **N2-4 — retire the `validate_relocated_triangles` STOP** → `LocalRefinementRequired`
   only for genuinely-unresolvable regions once remesh covers the surface types.
5. (NURBS parametric-domain CDT is the separate D14 milestone — out of scope.)
6. (PLANAR patch re-CDT, §3.1's general form, remains available for any future
   planar degeneracy that is NOT a sub-feature merge — none in the current corpus.)

## 5b. Detailed re-CDT design (increment N2-2, for review)

The patch re-CDT replaces the degenerate-after-relocation triangulation of one
affected patch with a fresh CDT in the surface's parametric domain. Below is the
**planar** form (closes R0072); §5b.7 gives the cylinder `(θ,z)` deltas.

### 5b.1 Data already in hand

- `Patch { attribution: TriangleAttribution, tri_indices: Vec<u32> }` (`lib.rs`
  `flood_fill_patches`) — **the patch's triangle set is `tri_indices`**, exactly
  what we replace.
- `patch_boundary_cycle(patch, mesh) -> Vec<Vec<(u32,u32)>>` — the patch's ordered
  boundary cycles (directed vertex-pair edges). A patch has one outer cycle + 0..n
  hole cycles.
- `ortho_basis(normal)` + the `project_loop_2d` projection (`lib.rs:1730`); the
  CDT `cherchi_rs::cdt_polygon_with_holes(verts2d, outer, holes)`; the
  local-pool / map-back / orient-to-normal pattern in `tessellate_planar_cdt_face`
  (`lib.rs:1755+`) — the **template** for this function.
- `Mesh { verts, tris }`; `attribution.attributions: Vec<Option<TriangleAttribution>>`
  is 1:1 with `mesh.tris`.

### 5b.2 Trigger (which patches, when)

Run AFTER relocation + the N2-1 merge + the §4.5.3 sweep, as a repair step that
replaces `validate_relocated_triangles`'s loud STOP for the handled surface types.
A patch is **affected** iff it contains ≥1 relocated (`moved`) vertex AND ≥1 of its
`tri_indices` triangles is degenerate (area `< MIN_FEATURE_SIZE²`). Only affected
patches are re-meshed; all others are byte-untouched (so all-planar / no-conic
inputs keep the no-op path — `fuzz_boxes` unaffected). Scope this increment to
`attribution`-surface `Surface::Plane`; other surface types with an affected
degenerate patch keep the current loud STOP.

### 5b.3 Algorithm (planar patch)

For each affected planar patch P (surface `Plane { normal, .. }`):

1. **Boundary cycles.** `cycles = patch_boundary_cycle(P, mesh)`.
2. **Local 2D pool.** Collect the unique boundary vertices across all cycles;
   project each to 2D via `ortho_basis(normal)` (the `project_loop_2d` formula),
   building `verts2d: Vec<Point2>` + `local_of_global: HashMap<u32,u32>` and its
   inverse `global_of_local: Vec<u32>`.
3. **Classify outer vs holes.** Compute each cycle's signed area in the 2D frame;
   the cycle of largest |area| is `outer`, the rest are `holes` (the same rule
   `emit_topology` uses at `lib.rs:10048`, reused — not re-invented). Convert each
   cycle's vertex sequence to local indices.
4. **CDT.** `tris_local = cdt_polygon_with_holes(&verts2d, &outer_local, &holes_local)?`.
   No Steiner, no boundary subdivision → the boundary vertex set is preserved
   bit-for-bit.
5. **Lift + orient.** Map each `tris_local[i]` triple back to global vertex
   indices via `global_of_local`. **Wind each new triangle to match P's existing
   winding** — derive P's reference normal from the average area-vector of its
   non-degenerate `tri_indices` triangles (NOT the bare `plane.normal`, which can
   be the opposite sense for a Subtract-reversed / opposite-normal patch), and
   flip any new triple whose area-vector opposes it. (§5b.5 explains why this is
   the conformality-critical step.)
6. **Splice.** Remove P's old triangles (`tri_indices`) from `mesh.tris` +
   `attribution` in lockstep; append the new triangles, each carrying P's
   `attribution`. Interior (non-boundary) vertices of P are simply not referenced
   by the new triangles — a **flat** patch carries no shape in its interior, so
   dropping them is exact (cleaned by `compact_unreferenced_verts`). *(Curved
   patches must KEEP interior vertices — §5b.7.)*
7. **Recompute** Phase A is NOT needed inside the loop (we mutate triangles, not
   the relocation); after all affected patches are re-meshed, re-run
   `validate_relocated_triangles` + `check_watertight_2manifold`.

### 5b.4 Splice mechanics (preserving the 1:1 attribution array)

`mesh.tris` and `attribution.attributions` are parallel `Vec`s. The splice must
keep them parallel. Cleanest: rebuild both in one pass — iterate the old
`(tri, attr)` pairs, copying through every triangle NOT in any affected patch's
`tri_indices` (a `HashSet`), then append the new `(tri, P.attribution)` pairs for
each affected patch. This avoids index-invalidation from in-place removal and is
the same shape as `collapse_vertex`'s rebuild.

### 5b.5 Conformality (the watertight key — D5 / §4.4.3)

The re-meshed patch shares its boundary vertices with its neighbors (one combined
mesh). CDT keeps that boundary unsubdivided, so each boundary edge still exists,
incident to exactly one re-meshed triangle on P's side and the unchanged neighbor
triangle on the other side. For the half-edge pairing to still cancel, P's
boundary **half-edges must keep their original direction** — guaranteed by §5b.3
step 5 (wind to P's existing reference normal, so the boundary is traversed the
same way). Interior re-triangulation cannot affect pairing (interior edges are
internal to P). `check_watertight_2manifold` after the splice is the proof gate;
any breach → loud STOP, never shipped.

### 5b.6 Loud STOPs (P9/P10)

- `cdt_polygon_with_holes` error (degenerate/duplicate/crossing boundary) →
  `Stage4RegionInvalid { LocalRefinementRequired }` (the boundary is malformed —
  genuine §4.5.2 territory, not papered).
- A re-meshed patch that still fails `validate_relocated_triangles` /
  `check_watertight_2manifold` → loud STOP. Never a silent accept.
- No tolerance widening anywhere; the only tolerances are the existing
  `MIN_FEATURE_SIZE` (degeneracy test) and `TAU_WORK` (the watertight pairing),
  both pre-existing.

### 5b.7 Cylinder `(θ,z)` deltas (increment N2-2b — separate review)

Same skeleton, three changes: (1) project boundary verts to `(θ, z)` in the
`ortho_basis(axis)` frame (the Stage-1 cylinder frame, `lib.rs:1413`), handling
the **θ seam wrap** (a patch crossing θ=0/2π must be unwrapped to a continuous θ
interval before CDT, then re-wrapped on lift-back); (2) **keep interior vertices**
(project them into the pool too) OR use `cdt_polygon_with_holes_refined` with a
`max_area` derived from the cylinder chord bound, because a curved patch's interior
carries shape; (3) lift-back maps `(θ,z)` to 3D via the exact cylinder `eval`, NOT
an inverse-planar projection. Boundary vertices lift back to their EXISTING 3D
positions bit-for-bit (they are already on the analytic cylinder from relocation).
This increment is gated behind N2-2 (planar) landing green.

### 5b.8 Test plan

- `red_r0072_…` reaches oracle-correct (planar re-CDT) → un-`#[ignore]`.
- A yang-rs lib unit test on the planar re-CDT helper: a synthetic patch with a
  collinear-monotonic boundary run re-meshes to all-positive-area triangles, same
  boundary vertex set, wound to the reference normal.
- Assay 0 SUPPORTED_WRONG, no CORRECT lost; campaign always-on green; `fuzz_boxes`
  900/900 (no-op path); curved YR suites unregressed.

## 5c. Finding (2026-06-30): both Mode-2 slivers are on CYLINDERS — planar re-CDT is moot, curved needs LOCAL repair

Implementing §5b and probing (`YANG_RECDT_PROBE`) revealed the planar scoping was
wrong. The earlier "R0072 = Plane" was the **pre-merge** triangle (the sub-feature
pair, now handled by N2-1). After N2-1, R0072's remaining degenerate triangle
`[111,109,50]` is on **input A, face 2, a Cylinder** (r=0.00021) — same surface
class as R0021. So:

- **Neither Mode-2 case exercises the planar re-CDT.** The §5b planar whole-patch
  re-CDT was written and reverted (uncommitted) — it closes no current corpus
  case, so per demand-driven discipline it is NOT shipped; §5b stays as the
  documented design for a future planar-sliver case.
- **Whole-patch re-CDT is WRONG for curved patches.** The affected cylinder patch
  has **142 triangles** (the whole lateral). Re-triangulating it from the boundary
  (dropping interior vertices) would collapse the cylinder's curvature → chord
  error ≫ d_ε → wrong geometry. Curved patches require **§4.5.2 LOCAL** repair
  (fix the sliver region; keep the rest of the patch tessellation), not a
  whole-patch CDT.

### 5c.1 The cylinder collinear sliver, precisely

`[111,109,50]` is three relocated points **monotonic-collinear** on the
intersection generator (a line on the cylinder), 2A≈5e-26. The middle point is a
**redundant collinear curve sample**: it lies on the segment between its two
neighbors, so removing it does not change the curve. Triangle longest edge
≈5.9e-6; cylinder d_ε at this scale ≈5.5e-6 (borderline).

### 5c.2 Two candidate local repairs (DECISION NEEDED)

- **Option A — redundant-collinear-point collapse.** Extend the §4.5.3-style
  collapse to a monotonic-collinear degenerate relocated triangle: edge-collapse
  its middle (redundant) vertex onto a neighbor along the curve via
  `collapse_vertex`. Surface-agnostic, ~15 lines, reuses proven machinery.
  *Faithful claim:* the removed point is provably on the line (the triangle is
  collinear), so the CURVE is unchanged; the only effect is off-curve neighbor
  triangles re-attach to the survivor, shifting one vertex along the curve by the
  collapsed-edge length. *P9 risk:* that shift must stay within d_ε to be a
  faithful re-sampling — and here the triangle span (5.9e-6) is ~d_ε
  (borderline), so the gate (collapse only if longest edge < d_ε) sits right at
  the edge. Clean if it holds with margin; a hack if forced.
- **Option B — local (θ,z) re-triangulation.** Re-triangulate ONLY the sliver +
  its one-ring in the cylinder `(θ,z)` frame, **keeping** all those vertices
  (needs a CDT that accepts caller interior points — the cherchi-rs extension the
  original increment-1 proposed, now genuinely demanded). No geometry shift
  (every vertex kept), faithful by construction. More code; the seam-wrap is
  avoided because the region is local.

**Recommendation:** Option A *iff* the d_ε gate holds with comfortable margin on
both cases (measure first); otherwise Option B. Option A is the smaller, lower-
risk step and is consistent with the existing §4.5.3 collapse philosophy — but the
borderline d_ε on R0072 means I must measure the margin before committing to it.

### 5c.3 Option A TRIED and RULED OUT (2026-06-30): relocation flattens a STRIP, not a point

Measured d_ε (7.97e-6 R0072 / 1.97e-3 R0021) and widened the N2-1 collapse gate to
d_ε. Result: each collapse removed one collinear sliver but **exposed a wider
one** — R0072 cascaded 111→35 (`[35,118,119]`, shortest edge 1.14e-5 **> d_ε
7.97e-6**), R0021 cascaded 133→127. **Root insight:** relocating the intersection
vertices onto the exact curve flattens an entire **band of triangles along the
curve** into collinear slivers, not a single redundant point. Point-by-point
collapse cascades through the band and stalls at the first sliver whose shortest
edge exceeds d_ε (collapsing it would distort off-curve neighbours beyond the mesh
resolution — correctly refused). So Option A is INSUFFICIENT and was reverted (it
also loosened N2-1's faithful merge for zero gain). The committed N2-1
(MIN_FEATURE_SIZE merge) stands; it handles genuine duplicates, which is real.

### 5c.4 The faithful fix is §4.4.1 LOCAL band re-triangulation (the real N2 core)

The whole curve-adjacent band must be re-triangulated **at once** (so the
relocated curve points become a boundary chain, not spanning triangles), KEEPING
every band vertex (a curved patch carries interior shape — they cannot be
dropped), in the surface's `(θ,z)` parametric domain, with the intersection curve
as a constraint edge. This requires:

- **The cherchi-rs CDT extension previously dropped as "unnecessary" is actually
  REQUIRED here.** §5's claim (a) — that the boundary-only CDT suffices — holds
  only for the planar *whole-patch* remesh (drop interior). The **curved local
  band remesh must keep caller-provided interior vertices**, which neither
  `cdt_polygon_with_holes` (boundary-only) nor `cdt_polygon_with_holes_refined`
  (adds *new* Steiner, not caller points) provides. So a new
  `cdt_points_with_constraints(verts, constraint_edges)` (triangulate a given
  vertex set with hard constraint edges; spade supports it) IS demanded — by this
  consumer. Increment 1 is **un-dropped**, now with a real caller.
- Local band identification (the connected set of degenerate collinear relocated
  triangles + their one-ring), `(θ,z)` projection with seam-wrap, conformal splice
  (the §5b.4/§5b.5 machinery), re-gate.

**This is a multi-component build, the genuine N2 core.** The shortcuts (merge,
d_ε collapse, whole-patch re-CDT) are now all ruled out by measurement. The
current Mode-2 loud STOP is CORRECT (never ships wrong geometry); closing it is
this build.

### 5c.5 BUILT: the keep-interior CDT foundation; FOUND: both cases are FULL-RING (not local bands)

- **Committed:** `cherchi_rs::cdt_polygon_with_holes_keep_interior` (triangulate a
  polygon-with-holes keeping caller interior vertices) — the §4.4.1 foundation,
  3 unit tests incl. the collinear-boundary no-degenerate property.
- **Built then reverted (P9 — unverified, doesn't handle the actual cases):** a
  LOCAL band re-CDT consumer (`replan_patch_band`: band = degenerate tris +
  one-ring within the patch, projected to the surface's parametric domain, CDT
  keep-interior, conformal splice). It works for a *local* band — but probing
  showed **neither Mode-2 case is local**: the same-normal boss cut produces a
  **full circumferential intersection ring** on the cylinder lateral. R0072's band
  normals **cancel** (symmetric perpendicular cut → ref-normal ≈ 0); R0021's band
  **straddles the θ seam** (40 tris, 34 seed verts). A full annular band in `(θ,z)`
  is **periodic in θ**, which planar CDT cannot triangulate directly.
- **Remaining (the real closer):** full-ring periodic-θ re-mesh — cut the annulus
  at one θ (seam line z_lo→z_hi), DUPLICATE the seam vertices, CDT the unwrapped
  rectangle `[0,2π]×[z_lo,z_hi]` with the seam as boundary (keep-interior for the
  off-curve vertices), then RE-IDENTIFY the duplicated seam vertices. The
  per-triangle outward normal must be taken locally (not a single band ref-normal,
  which cancels around the ring). This is a substantial, self-contained increment
  on top of the committed foundation; until it lands, Mode-2 stays a correct loud
  STOP.

### 5c.6 R0038 — the LOCAL cylinder-generator-band demand (task #167/#168, 2026-07-15)

**New corpus demand for the LOCAL band re-CDT** (the increment §5c.5 built then
reverted for lack of a demanding case — R0038 IS that case). Grounded via
`YANG_LRR_PROBE` / `YANG_LRR_DEGEN` / `YANG_LRR_DEGEN_SURF`:

- **Failure:** R0038 STOPs at `site=degenerate_no_longedge`, `ndeg=3`. The three
  degenerate triangles (83=[23,19,18] / 84=[18,19,14] / 85=[15,14,19]) are on a
  **Cylinder** (r=15.22, axis_dir [0.4034, 0.9150, 0] — HORIZONTAL axis). Their
  five distinct vertices (14,15,18,19,23) are **exactly collinear** at constant
  z=13.256, xy-direction ratio 2.269 == axis_dir ratio 0.9150/0.4034 = 2.268 →
  the points lie on a cylinder **GENERATOR** (plane‖axis × cylinder = generator
  lines, the N46/R0026 configuration). One generator = **constant θ** → the
  degenerate band is a **thin θ-strip, LOCAL** (NOT the full circumferential ring
  R0021/R0072 produce from an axis-perpendicular boss cut). So R0038 is the
  local-band case, and the θ-seam / periodic-θ closer (§5c.5) is NOT needed here.

- **Why it is safe to close (risk profile).** The keep-interior re-CDT **moves no
  geometry** — it re-connects the SAME vertices (every band vertex kept, none
  dropped, no Steiner). So it CANNOT produce the R0091 "moved neighbour geometry"
  silent-wrong the collapse (Option A, §5c.3) risks; its worst case is a `cdt_*`
  error or a failed watertight re-gate → a **loud STOP** (safe). The distortion
  budget that made Option A a P9 hazard is simply absent.

- **Consumer design (implementation increment, review-gated):**
  1. **Band.** `is_degen(ti)` = triangle has a `moved` vertex AND area < `MIN_FEATURE_SIZE²`.
     Seed = the degenerate triangles; `band` = seed ∪ {same-`attribution`
     triangles sharing a vertex with a seed triangle} (one attribution per band;
     cross-curve neighbours on the OTHER surface are excluded — they are the fixed
     conformal neighbours). Connected components → one band per cluster.
  2. **Perimeter.** `patch_boundary_cycle(&Patch{ attribution, tri_indices: band }, mesh)`
     — reuse the existing walk (it already excludes fold slivers, so the zero-area
     degenerate triangles do not corrupt the boundary). Outer = largest |signed
     area| in `(θ,z)`; the rest are holes.
  3. **`(θ,z)` projection.** `ortho_basis(axis_dir)` frame; θ = atan2 of the radial
     component, z = projection onto axis. Assert the band's θ span ≪ π (LOCAL) —
     if it straddles the seam, bail to the loud STOP (defer to the periodic-θ
     closer). Interior band vertices projected into the same pool.
  4. **CDT.** `cdt_polygon_with_holes_keep_interior(verts2d, outer, holes, interior)`
     (committed foundation) — boundary fixed, interior kept, no new points.
  5. **Wind + splice.** Wind each new triangle to the **local** cylinder outward
     normal at its centroid (radial from axis; NOT a band ref-normal). Rebuild
     `mesh.tris`+`attr_vec` in one pass (remove band tris, append new with the
     band attribution) — the §5b.4 splice shape. Vertices are all reused, so no
     `compact`/lift-back is needed.
  6. **Re-gate + loop.** `continue` the degenerate-resolution loop so it re-scans;
     if no degenerate triangle remains, fall through to the existing
     `validate_relocated_triangles` + `check_watertight_2manifold`. Any breach →
     the existing loud STOP.
  7. **Scope gate:** Cylinder only, LOCAL bands only (θ span < π). Full-ring /
     seam-straddling / non-cylinder degenerate bands keep the current loud STOP.

- **Certification (non-negotiable, P10 — topology-mutating):** RED oracle
  `red_r0038_cylinder_generator_band` → oracle-correct + un-`#[ignore]`d; full
  release assay **0 SUPPORTED_WRONG**, no CORRECT lost; **sidecar parity** on
  R0038's arrangement (the mutation is downstream of Stage-2, so the arrangement
  is unchanged — the check is that the re-meshed OUTPUT is watertight/χ=2 and its
  boundary curve set matches); `fuzz_boxes` 900/900 (no-op path — re-CDT only runs
  when a degenerate band exists). A yang-rs lib unit test on the band re-CDT helper
  (synthetic collinear generator strip → all-positive-area, same vertex set, wound
  outward).

**Status:** DESIGN complete, de-risked (foundation committed, R0038 confirmed
LOCAL, risk profile shown safe). Implementation is the next increment — held to
the certification bar above, not rushed.

## 6. Risks & guardrails

- **Conformality** (§3.3): the dominant risk; mitigated by fixed-boundary + the
  watertight re-gate after every patch. Any breach → loud STOP, never shipped.
- **No tolerance widening / no hack-to-green** (P9/P10): the only "merge" allowed
  is §4.4.1(b) for verts within the *curve resolution* of one another (genuine
  coincidence), derived, not tuned; everything else is CDT or a loud STOP.
- **Determinism** (A4.2): spade is deterministic given fixed insertion order;
  reuse the boundary-only fn's caller-order insertion + sorted emit.
- **Scope creep:** each increment is independently committable and assay-gated;
  if N2-cdt-2 cannot make R0021 green cleanly, STOP and report (do not improvise).

## 7. Open questions for the reviewer

1. OK to add the new cherchi-rs CDT entry point (interior constraints + Steiner)
   rather than extend the existing signature? (Recommend: new fn, existing
   untouched.)
2. Increment N2-cdt-2 scoped to **planar patches only** first — acceptable that
   curved-patch Mode-2 configs stay a loud STOP until N2-cdt-3? (Recommend: yes —
   the same-normal planar caps are the live Mode-2 cases.)
3. Parametric domain for the planar remesh = `ortho_basis(normal)` (matches
   Stage-1). Confirmed acceptable (vs. an independent plane frame)?
