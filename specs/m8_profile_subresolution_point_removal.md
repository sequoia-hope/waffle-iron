# M8 — Profile-ingestion sub-resolution point removal (§4.3 at the constructor gate)

**Task:** #130 (M8 coplanar residue campaign), mechanism (1) — the
`DegenerateLoop` duo R0007/R0071, the last two `UNSUPPORTED(coplanar-boolean)`
cases in the corpus.

**Paper grounding:** Yang 2025 §4.3 (refs/text/yang2025_hybrid_boolean.txt:535)
— "remove a point too close to another on the same loop". N56 (2026-07-16)
ratified this vocabulary as a genuine paper op with the scale-relative
criterion `TAU_MODEL·(1+scale)`; this increment applies the same op at the
**profile ingestion gate** (`Profile::new`), the earliest point in the whole
pipeline where a loop exists.

## 1. Measured mechanism (re-confirmed 2026-07-22 on today's tree)

- R0007 (`extrude(gear,cut)`, scale 1.21e-4): the extrude OUTPUT carries
  **96 sub-TAU_MODEL vertex pairs, all at 7.790e-8, all edge-connected**
  (`KV2_SUBFLOOR_TWIN_PROBE`); the smallest legitimate feature is 2.337e-7
  (3.0× the twin separation).
- R0071 (`revolve(gear,cut)`, scale 1.86e-4): the revolve OUTPUT carries
  **36 pairs, all at 9.460e-8, all edge-connected**; legit floor 1.089e-7.
- The pairs are **profile-congenital** (KV15b plan-correction 2026-07-10):
  the micro gear profiles themselves carry doubled corners; the constructors
  emit them faithfully. No boolean mints them — mint-site collapse (KV15b)
  cannot reach them.
- Downstream failure: the Stage-0 coplanar overlay's §2b in-frame clustering
  (band floor = TAU_MODEL) welds the twins to one 2D representative →
  bit-identical consecutive polygon vertices → loud
  `DegenerateLoop("zero-length edge (repeated consecutive vertex)")` →
  the generic `UNSUPPORTED(coplanar-boolean)` wall. Probed 2026-07-22:
  R0007 pair (0,1), R0071 pair (1,0).
- `Profile::new` validation today rejects only **bitwise-exact** consecutive
  duplicates (check 4); sub-resolution twins pass.

## 2. Why ingestion, not overlay/boolean (the desync objection, resolved)

The 2026-07-10 census rejected a local consecutive-dup collapse *inside the
overlay* because it would desync the pair face from neighbor faces still
carrying both corners. That objection is about WHERE, not WHETHER: collapsing
at `Profile::new` happens **before any face exists**, so every face of the
constructed solid — caps, laterals, and everything a later boolean sees —
agrees on the single corner by construction. There is nothing to desync.

Alternatives considered:
- **Loud typed reject (#178 pattern):** wrong class — #178's
  `SubResolutionCoplanarGap` rejects *two distinct parallel planes* posing as
  coplanar (out-of-contract ambiguity). Here the model is unambiguous; the
  doubled corner is below the pipeline's own representable resolution and the
  paper's op is *removal*, not rejection.
- **Absolute-floor weld (MIN_FEATURE_SIZE):** permanently dead — these models
  carry hundreds of legitimate features in 2e-7..9e-7 (the R0091 revert
  lesson).
- **Fixing the assay generator:** the dirty profile is realistic input; the
  kernel must canonicalize or reject, not the test author.

## 3. Design

New canonicalization pass in `Profile::new`, running **before**
`validate_and_normalize_loop`, on the outer loop and each hole loop
(`ProfileRegion::Polygon` only; `Circle` has no loop, `ArcPolygon` twin
hygiene is out of scope until a case demands it):

1. `scale` = max |coordinate| over ALL loop points of the profile (outer +
   holes, u and v) — one scale per profile, so all loops use one criterion.
2. `tau = TAU_MODEL · (1 + scale)` (the N55/N56 idiom; at these micro scales
   ≈ 1e-7, at unit scale 2e-7 — both below anything the pipeline can carry
   through Stage-0 clustering anyway).
3. Single sweep over ORIGINAL consecutive segments `(i, i+1 mod n)` with
   exact f64 distance `< tau` (strict): drop the higher-index endpoint;
   for the closing segment `(n−1, 0)` drop `n−1` (min-index survivor keeps
   its own bits — the KV15b rule). A segment fires only if BOTH endpoints
   are still alive → a chain of sub-tau steps collapses pairwise only, no
   chain-drift (a super-tau feature made of sub-tau steps cannot vanish).
4. If a loop drops below 3 points → loud `ProfileTooFewVertices` (the whole
   loop was sub-resolution; refuse to guess).
5. All existing validation (dup check, spike, simplicity, orientation,
   disjointness, containment) runs AFTER the pass, unchanged and loud.

Determinism: pure function of input bits; no iteration order beyond the loop
order itself.

**Oracle safety:** R0007/R0071 metas carry soft oracles only (χ=2,
watertight, bbox, volume monotonicity). Collapsing an isolated profile twin
pair changes an extrude by ΔV=−2, ΔE=−3, ΔF=−1 (Δχ=0) and the enclosed area
by O(tau·scale) — far inside every oracle band.

## 4. Increment plan

- inc-1: `collapse_subresolution_points` in `profile.rs` + unit tests
  (RED→GREEN micro-gear twin fixture with verbatim R0007 coordinates;
  keep-above-tau fixture at the R0071 margin 1.089e-7; chain-drift fixture;
  all-sub-resolution loop → `ProfileTooFewVertices`; unit-scale no-op
  canary). Mutation check: widening tau to MIN_FEATURE_SIZE must kill the
  keep-fixture; dropping the both-alive rule must kill the chain fixture.
- inc-2: single-case verification (R0007/R0071 with the twin probe: extrude/
  revolve OUTPUT sub-tau pair count → 0; case category movement recorded),
  then full release corpus + rewrite tier + clippy/fmt. Expected: the two
  cases leave `UNSUPPORTED(coplanar-boolean)`; whatever wall they hit next
  is a NEW characterized frontier, not a regression. Everything else
  byte-identical (no other corpus case has sub-tau profile twins — the
  probe census is the evidence).
- WASM rebuild in the same commit (kernel-v2 changed).

## 5. Second and third mechanisms found during inc-2 (2026-07-22)

Retiring the profile twins exposed two further defects, both the SAME
absolute-vs-scale-relative class, one in the harness and one in Stage 0:

### 5a. Harness oracle: absolute degenerate-triangle filter (R0007 WRONG)

With the twins gone, R0007 ran end-to-end and landed SUPPORTED_WRONG
(`outward_normals: no valid triangles`). Root cause was the ORACLE, not the
kernel: `check_outward_normals` (and, vacuously-passing, its sibling
`check_consistent_normals`) skipped triangles with `area_sq < 1e-20`
ABSOLUTE. The final mesh was healthy — 280 triangles, max area_sq 8.05e-21
(measured, `ASSAY_ORACLE_PROBE`) — every one below the absolute floor, so
`total == 0` → fail. R0007 is the first ~1e-4-scale case ever to REACH the
oracle (it was UNSUPPORTED before). Fix: sine-based per-triangle filter
`|cross| < 1e-6·e_max²` — scale-free, and safely above the f32 vertex noise
(~1e-7·e²) whose unreliability the skip exists to absorb.

### 5b. Stage-0 §2b/§2c clustering: detection-band floor welds legit
features (R0071 residual)

With the revolve twins gone, R0071's overlay STILL rejected
`DegenerateLoop`: the post-cluster polygon carried 16 bit-identical
consecutive pairs minted by the CLUSTERING itself. The §2b/§2c in-frame
clustering reuses the coplanar DETECTION band (`max(TAU_MODEL,
scale·TAU_WORK)` = 1e-7 here) though what it reconciles is frame-PROJECTION
rounding, O(scale·ε) ≈ 1e-19 (measured pair gaps 1e-19..1e-21). Its safety
assumption ("real features ≥ MIN_FEATURE_SIZE, orders above the band") is
false at micro scale: R0071's legitimate 1.089e-7 tooth features were welded
PER-AXIS whenever diagonal in the frame (components ~7.7e-8 each under the
band; effective diagonal weld radius √2·band), while axis-aligned instances
survived — measured: 16 welded (d=0) vs 12 surviving (d≈1.0–1.09e-7) around
the gear. Fix: clamp the clustering band to the scale-relative ceiling
`1e-9·scale` (the stage0 rim `snap_eps` shape) inside
`cluster_frame_coords_rim_aware` — ≫ ULP noise, ≪ representable features at
every scale; the (test-only) §2b wrapper and both production call sites
inherit it. Consistency: profile ingestion (§3) and the KV15b emission
collapse both remove sub-TAU_MODEL geometry BEFORE Stage 0, so no input
carries features between the clustering ceiling and TAU_MODEL.

## 6. Ledger

- 2026-07-22: spec written; census re-confirmed on today's tree (96×7.790e-8
  / 36×9.460e-8, all edge-connected, legit floors 2.337e-7 / 1.089e-7).
- 2026-07-22 inc-1: `Profile::new` sub-resolution point removal SHIPPED
  (8 unit tests incl. both mutation canaries; exact-repeat contract
  preserved via the open interval).
- 2026-07-22 inc-2: R0007 UNSUPPORTED→SUPPORTED_CORRECT (after the §5a
  oracle fix); R0071 UNSUPPORTED→SUPPORTED_CORRECT (after the §5b band
  clamp; RED→GREEN unit
  `micro_scale_diagonal_legit_feature_survives_detection_band`). The
  UNSUPPORTED(coplanar-boolean) corpus tail is EMPTY.
- 2026-07-22 (same day, follow-on characterization): **the F0082
  Extrude-12 "M8 coplanar-residue" layer (#188 spec §10.10 defect 1) is
  REFUTED as a Stage-0 class on the current tree** — probes
  `YANG_SCAN_NEARMISS_PROBE` (new, banked in `scan_near_coplanar`: dumps
  plane-test failures with gap<1e-3 and AABB-overlap failures) and
  `YANG_OPFACE_DUMP=x,z` (new, banked in `stage0_preprocess`: per-op face
  dump filtered to an (x,z) column). Measured: all 55 near-miss pairs in
  the F0082 chain sit ≥4e-5 gap (≥400× band) with misaligned normals;
  Extrude 12's tool is an 8-plane prism whose sketch-plane family is a
  THIRD orientation (≥0.019 rad off both A wall families), and NO tool
  plane contains the defect-cluster verts (seal column, axis point v935 —
  residuals 0.015–0.5). The live Extrude-12 STOP signature today =
  §10.10 defect 2 exactly: double-cover edges (930,931)/(930,934)/
  (931,971)/(932,994) at the seal cluster and the edge-connected
  sub-TAU_WORK twin v971/v972 at 5.5e-14, plus tool↔A shared-sketch-vert
  re-mints — ALL arrangement-level. The whole Extrude-12 residual routes
  to task #194; no Stage-0/M8 work is actionable on it. With all four
  census mechanisms retired and the UNSUPPORTED(coplanar) tail empty,
  the #130 campaign charter is COMPLETE.
- 2026-07-22 full release ledger: **255C / 0W / 54E / 1T** on 312 (was
  252C/0W/55E/2U/1T). Deltas: R0007+R0071 UNSUPPORTED→CORRECT (targets);
  **F0069 ERROR→CORRECT bonus** — the #153 off-plane planar-face emission
  wall (3e-8 @ 2m): the OLD 1e-7 clustering band could legally move
  in-frame coords by up to 1e-7 at any scale, and the §5b clamp (2e-9 at
  2m) removes exactly that lift, consistent with #153's measured 3e-8
  off-plane magnitude; R0081 stays ERROR with the failure point moved
  (engine-error → auto-union at Revolve 3, detail-only). F0072 stays the
  known budget-borderline TIMEOUT. Parity tier green; rewrite + fast tiers
  green; clippy/fmt clean; WASM rebuilt.
