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
