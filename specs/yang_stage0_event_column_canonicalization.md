# SPEC — Stage-0 exact event-column canonicalization (compliant twin-mint fix)

Status: **DESIGN** (2026-07-16, #169/#170 compliant climb-back, increment 1).
Prerequisite context: deviation N53 (welds retired), N48 (diagnosis), N49 (two
refutations). Owner surface: `crates/yang-rs/src/coplanar_overlay.rs`.

## 0. Why this is the top-priority compliant fix

The four non-compliant welds were retired (N53): honest baseline `228C/0W/63E`.
Three of the four (`f32`, `coincident`, `subres`) share ONE upstream root — the
Stage-0 coplanar overlay mints **near-coincident twin vertices** — which is *also*
the root of the F0082/R0095 non-2-manifold bucket that #169 Phase B refuted. So a
single fix at this mint site is the highest-leverage compliant move available:

- Directly targets the render-collapse reverts: R0012, R0098, R0055, F0078, F0079
  (`f32`/`coincident`), and R0076 (`subres`).
- Plausibly unblocks F0082/R0095 (same near-duplicate-junction root, per the
  #169 §8e refutation) — to be confirmed per-case, not assumed.

It does NOT target the `subfeature` cases (→ wire §4.4.1 mesh-update, Phase B) nor
the grazing/near-tangency set (→ §4.5.2, Phase C). Those are separate increments.

## 1. The mint mechanism (grounded)

`coplanar_overlay_multi` (`coplanar_overlay.rs:386`) lifts each operand's projected
2D polygon corners to **exact rationals** (`exact_loops` → `ExactPoint2`), then runs
an exact plane-sweep arrangement:
- `split_all(&all_edges)` → `subs` (`:414`).
- `xs` = the sorted set of **distinct exact endpoint x's** (`BTreeSet<RBig>`, `:417`).
- Per event line `x=xi`, `line_pts[xi]` collects every sub-segment endpoint at `xi`
  plus the crossing `y_at(s, xi)` of every sub-segment whose open x-span contains
  `xi` (`:427-444`). Adjacent slabs share these verts exactly (conformity).

**The defect.** The 2D projection `u = p·e₁` is computed in f64 *before* the exact
lift. Two genuinely-distinct input corners whose 3D separation is (near-)orthogonal
to the sweep axis `e₁` therefore land at **near-equal but exactly-distinct** x — a
gap at the coordinate-resolution floor (~1e-6 relative), pure projection noise, not
a real feature. The exact sweep faithfully opens **two event columns** `xi ≈ xj`.
Any crossing edge of the *other* operand is lifted at BOTH columns (`y_at` fires for
each), minting two arrangement vertices ~1e-6 apart. They are exact-distinct (they
survive the f64 interner — B6 is working correctly) yet below model resolution, so
they render-collapse at f32 (the R0012/R0098 signature; N48 sidecar-certified this
is NOT a native-port bug — the C++ reference mints the identical pair from the same
noisy input).

**RED oracle (already written, `#[ignore]`d):**
`coplanar_overlay.rs:1534 near_coincident_event_columns_do_not_mint_twin` —
A = quad top edge `y=60−0.2x`; B ⊂ A with left corners `(50,10)` and
`(50.000001,30)` (Δx=1e-6, Δy=20). Asserts min pairwise output-vertex gap `> 1e-3`.
Un-ignore this when the fix lands.

## 2. What the two prior refutations teach (constraints)

- **N48 refutation — do not tear the shared boundary.** Snapping the input columns
  moved boundary-shared corners; the overlay's OUTER boundary is shared with the
  neighbouring (non-coplanar) mesh rims, which are NOT part of this overlay and do
  NOT move, so the seam desynced (`nary_tessellated_group_stage0_meshes` broke).
  ⟹ **The canonicalization must never move an outer-boundary vertex.**
- **N49 refutation — do not over-merge, and 2D can't predict 3D f32.** A
  model-tolerance × GLOBAL-scale twin weld over-merged legitimate near-origin rim
  samples in far-flung models and regressed F0063/F0090/R0014/R0088; a 2D overlay
  cannot see the 3D f32 render threshold. ⟹ **Merge only at the coordinate
  -resolution floor where the gap is provably noise, and merge COLUMNS (x-values),
  not arbitrary vertex pairs.**
- **Landscape shift (post-N53).** Two of N49's four regression cases (F0090, R0088)
  are now ERROR anyway (weld-retired), so the "must not regress" set for this fix is
  narrower. But F0063/R0014 remain CORRECT and must stay so.

## 3. The compliant approach — interior-only exact column canonicalization

Merge near-coincident event columns that arise **only from interior geometry**,
consistently and exactly, rewriting the sub-segment coordinates (not just `xs`):

1. **Cluster** the event x-values `xs` into groups whose consecutive gap is below a
   **coordinate-resolution floor** `x_eps` (relative to local magnitude — NOT model
   tolerance; a genuine small feature is ≥ `MIN_FEATURE_SIZE`, far above projection
   noise). Only 2+-member clusters matter.
2. **Interior guard (avoids N48).** A cluster is eligible ONLY if none of its member
   columns carries a vertex on the overlay's **outer boundary** (the shared seam with
   the rest of the mesh). Outer-boundary columns are frozen; only strictly-interior
   near-columns (e.g. B's corners inside A) may merge. Interior-only is exactly what
   N49 identified as the missing constraint.
3. **Canonicalize (avoids "merging xs breaks the slab filter", N48).** For each
   eligible cluster pick ONE canonical exact x (e.g. the member that is itself an
   input corner, else the min). Rewrite EVERY occurrence of a member x in the
   `Sub` endpoints (`s.a.x`, `s.b.x`) AND in `all_edges` before/at `split_all`, so
   `xs`, `line_pts`, and the slab active-set filter all see the single column. The
   two distinct corners keep their y — they become two points in one exact column,
   which the sweep handles natively (both enter `line_pts[x_canon]`).
4. The crossing edge is now lifted **once** at the merged column ⟹ no twin.

Because the merge is global+consistent (applied to all references of the x-value on
both operands) and interior-only, shared-boundary corners never move ⟹ no seam
tear; and because it fires only at the resolution floor on interior columns, it
cannot touch legitimate near-origin rim samples ⟹ no N49 over-merge.

## 4. De-risk plan (ordered; gated; each gates the next)

1. **Un-ignore the RED oracle** `near_coincident_event_columns_do_not_mint_twin`
   and make it pass with the canonicalization behind a gate `YANG_STAGE0_COLMERGE`
   (off ⟹ byte-identical). Add a fixture where the near-columns are on the OUTER
   boundary → assert they are NOT merged (interior guard) and the seam is intact.
2. **`nary_tessellated_group_stage0_meshes` and all `coplanar_overlay` unit tests
   stay green** with the gate ON (watertightness preserved).
3. **Full release assay, gate ON:** target `≥ 228C, 0 WRONG`, and specifically
   check R0012/R0098/R0055/F0078/F0079/R0076 → CORRECT and F0063/R0014 unchanged.
   Report the actual delta (no promised count).
4. **Note on the oracle:** sidecar parity is NOT the validator here — the C++
   reference receives the same f64-noisy projection and mints the identical twin
   (N48). The oracles are the RED test, watertightness, and the assay 0-WRONG
   invariant. Confirm F0082/R0095 per-case (may or may not clear).
5. **Flip the gate default ON** only after 1–4 hold; keep the env as an A/B knob.

## 5. Risks / open questions

- **`x_eps` calibration.** Must sit strictly between projection noise (~1e-6 rel)
  and the smallest legitimate feature (`MIN_FEATURE_SIZE`). Derive from local
  magnitude, not a tuned constant; if a case needs it tightened to stay 0-WRONG,
  that is a STOP-not-a-band decision (P9/P10).
- **Only-x is insufficient if the sweep axis differs.** The mint is along `e₁`; if
  a twin arises from near-coincident *y* within a column, the same clustering must
  apply to `line_pts[x]` y-values (interior-only). Handle if a case demands it;
  do not build speculatively.
- **Interior classification.** Need a cheap, exact test for "column carries an
  outer-boundary vertex." The overlay already knows each operand's outer loops
  (`polys_a`/`polys_b` outer rings); a column x is boundary-touching iff some outer
  -ring vertex has that exact x. Precompute the set of outer-ring x's.
- If canonicalization proves to interact badly with `split_all`'s crossing
  computation (a merged column changing which sub-segments cross), fall back to a
  loud STOP for that overlay rather than emitting a wrong arrangement — never a
  silent divergence.

## 6. Sequencing within the compliant climb-back

1. **THIS spec** — Stage-0 event-column canonicalization (`f32`/`coincident`/
   `subres` roots + F0082/R0095 probe).
2. Wire §4.4.1 mesh-update (`two_sided_conformal_update_lifted` + `SurfaceChart`)
   — subsumes the `subfeature` weld. (#169 Phase B, re-scoped.)
3. §4.5.2 local-refinement loop — grazing/near-tangency (C0067/R0038/C0065/R0074).
   (#169 Phase C; needs new cherchi local-arrangement machinery.)
4. Compliance ledger + grep-lint (the ratchet, `feedback_paper_compliance_north_star_weld_ratchet`).
