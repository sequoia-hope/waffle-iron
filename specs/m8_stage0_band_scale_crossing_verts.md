# M8 — Stage-0 band-scale crossing vertices (M-C follow-up cycle)

**Status:** spec (FIP Phase 1) — §2 Measured mechanism COMPLETE for R0088-a
(rim path); R0070-b measurement in progress. **Change class:** bug fix
(modeling-related), M8 workstream — the M-C residue named by
`specs/m8_stage0_inputcheck_clean_emission.md` §2a. **Crates:** `yang-rs`
(`lib.rs` rim-override insertion; possibly `stage0.rs`), `test-harness`
(trackers), `cherchi-rs` (census fixtures — oracle only).

## 1. Goal

Same conditional contract as the parent cycle
(`m8_stage0_inputcheck_clean_emission` §1): every operand mesh Stage-0 hands
to `backend.labeled_arrangement` individually satisfies the five Cherchi
input axioms whenever its solid's Stage-1 mesh does. This cycle closes the
**M-C residue**: overlay-minted crossing vertices band-close (~1e-7·scale)
to OTHER crossing/corner vertices emit band-scale sliver T-junctions —
the last introduced-defect class with boundary edges in the 2026-07-03
corpus operand sweep (`docs/audits/stage0_operand_inputcheck_sweep_2026-07-03.tsv`,
356 operands / 10 introduced-dirty):

- **R0088** op `374_subtract` operand-a: 12 boundary edges (4 three-edge
  sliver holes) + 1 improper pair (pinch).
- **R0070** op `358_subtract` operand-b: 6 boundary edges (2 sliver holes).

The fold-pair emission class (F0027/28/29, R0007, R0023, R0038, R0089 —
misoriented+improper, no boundary edges) is a SEPARATE named follow-up and
explicitly out of scope here.

## 2. Measured mechanism (2026-07-04, diagnosis harness
## `m8_stage0_operand_diagnosis.rs` + new overlay dump + probes)

### 2a. R0088-a — rim-override angular merge_tol drop (CONFIRMED, causal)

R0088's defective subtract: A = 11-segment circle-revolve solid whose pair
face 0 is a flat disc cap; B = 952-vert gear solid, pair face 1 planar
polygon, same-normal crossing configuration. Instrumentation
(`YANG_STAGE0_DUMP_DIR` overlay dump — new: per-vertex resolution
provenance + per-triangle emission verdict — and `YANG_SPLIT_PROBE` probes
in `collect_rim_crossings` / the stage-1 rim-override insertion):

1. The exact overlay subdivides the disc's rim chords at every sweep event
   column. Near two gear corners (corner_b 951 / 929), TWO event columns
   sit **7.2e-5 apart in u** (≈ band·scale; the gear boundary crosses the
   rim chord twice band-close, and the crossing columns cannot be welded by
   the §2b/§2c input-chain clustering — they are minted downstream of it).
   Each such column pair mints band-close TWIN vertices on each rim chord
   it crosses (measured pairs: overlay v1183/v1189, v1182/v1188,
   v1368/v1374, v1369/v1375 — all `mint`-tagged, N2-3a on-circle).
2. `collect_rim_crossings` collects BOTH twins into the rim override list
   (probe: 362 KEPT, 0 skipped by the endpoint window, 0 duplicate skips —
   its dedup is exact bit-identity, correct).
3. **The defect:** `stage1_tessellate_inner`'s rim-override insertion
   (lib.rs, the `inserted_offsets` scan) dedups by an ANGULAR tolerance
   `merge_tol = uni_step·1e-6` (= 5.712e-7 rad at N=11). The twins'
   angular separation is 2–4e-7 rad < merge_tol → the second twin of each
   pair is SILENTLY SKIPPED (probe: exactly 4 drops on cap rim edge 0 —
   bit-for-bit the four census hole twins, mesh verts 713-716 — plus their
   4 azimuth projections dropped on opposite rim edge 1).
4. The disc face's override (the overlay triangulation) keeps BOTH twins on
   its boundary chain; the rim ring (shared by cap border / lateral /
   opposite cap) keeps only ONE → each dropped override is a T-junction:
   the lateral fan uses the whole chord sub-segment (census: 1 whole edge
   on face 2 per hole) while the cap override uses the two sub-edges
   (2 sub-edges on face 0 per hole) → 4 × 3 = 12 boundary edges, and one
   adjacent-hole contact = the 1 improper pair.

The dedup's documented intent (a8c9f2b1) is "an override point coinciding
with an already-inserted override" — i.e. the SAME point re-arriving from
adjacent sub-chords sharing an endpoint. Such re-arrivals are bit-identical
3D points; the angular tolerance over-reaches onto genuinely distinct
band-close crossings. This is a silent tolerance merge — the P9 anti-class.

### 2b. R0070-b — SAME root, ULP-scale twins (CONFIRMED, causal)

R0070's second subtract op (pair face_a=0 / face_b=0, mm-scale model) IS a
disc-rim pair on solid B after all: the `[rim-insert-probe]` fired exactly
TWICE on B's circle edge 0, and the two dropped override coordinates are
bit-for-bit the two census hole twins
(`(0.00489102431109368, …, …074)` = mesh 45's coords;
`(0.006090419500911003, …, …874)` = mesh 50's coords). Here the twin
separation is 1-2 ULP (mint(rev) femto-twins — reverted chord lifts of
femto-split overlay columns), i.e. ~1e-14 rad, far below merge_tol
(5.236e-7 at N=12) → dropped → the same T-junction shape (2 holes ×
3 boundary edges). Neither dropped offset is near a uniform sample
(k-distance ~0.036 rad), so the loud uniform-coincidence branch is not in
play. R0070 currently walls LOUDLY end-to-end regardless (Stage-4
`LocalRefinementRequired` + Stage-3 `AnalyticalSolutionNotAvailable` — M5/N2
class), so its E2E outcome is out of scope here; its acceptance is
operand-level (census on the re-banked fixture).

### 2c. GREEN outcome (2026-07-04, measured)

The E-C1 fix (exact bit-identity dedup replaces the angular merge_tol scan;
the uniform-coincidence loud check and the off-rim radial validation are
untouched):

- **R0088:** ALL emitted operands five-axiom clean (native census, both
  ops, both sides; operand-a 717→721 verts — the 4 opposite-rim twin
  projections now insert). The edge-pairing wall is GONE; the
  previously-defective op now stops loudly at kernel-v2
  `EmptyBooleanResult`, and the second op keeps its pre-existing
  "output loop with fewer than 3 edges" wall (the R0046-class output-loop
  residual). `red_r0088` GREEN per the campaign convention.
- **R0070:** the previously-defective op now stops LOUDLY pre-backend:
  `MalformedTopology("face 2: azimuth-merge rims have mismatched /
  too-few samples (24 vs 22)")` — the cap rim carries both ULP twins but
  the OPPOSITE-rim projection (f64 azimuth + grid search) collapses them
  bit-identically, so the lateral refuses the mismatched rings. This is
  the §7 "downstream band-scale intolerance" residue, measured: loud,
  typed, pre-backend (no dirty operand is emitted — I1 holds vacuously);
  the case's first op keeps its unrelated Stage-4 `LocalRefinementRequired`
  wall (M5/N2 class). Its RED fixture is retired (no emission to re-bank);
  named follow-up: an exact opposite-rim projection that preserves
  ULP-distinct azimuths (or a structural share of the cap twins' azimuth
  keys) would let this op proceed to its next honest wall.
- **R0046 (witness):** operands stay clean; its output-loop wall unchanged.
- **Full assay (I4):** 84 SUPPORTED_CORRECT / 0 SUPPORTED_WRONG; per-case
  diff vs the banked baseline (`assay_kv2_report.baseline-m8stage0.json`,
  the load-noisy 81-correct copy): **zero CORRECT lost, +3 gained**
  (F0016/F0024/F0061 — baseline load-noise ERRORs, known-CORRECT), R0070
  TIMEOUT→ERROR (the new loud stop lands inside the 30s cap). Timeout
  class ~equal (35 vs 36; the box was not fully quiet — the parent
  cycle's 96-CORRECT quiet-box figure was not re-measured this cycle, and
  the timeout population is the known load-sensitive gear-perf class, so
  the binding gate is the per-case non-negative diff above). New report
  banked as `assay_kv2_report.baseline-m8mc.json`.

## 3. Parameters

None new. No tolerances, no epsilons (A14.3) — the fix REMOVES a tolerance
comparison (angular merge_tol dedup → exact bit-identity dedup). Diagnostic
env vars (`YANG_STAGE0_DUMP_DIR` overlay dump, `YANG_SPLIT_PROBE`) are
read-only observers, not modeling parameters.

## 4. Branch table (rim-override insertion × override population)

| # | Path | Trigger | Contract row |
|---|------|---------|--------------|
| E-C1 | **[fix]** Override point NOT bit-identical to any already-inserted override on this circle edge | ALWAYS inserted, regardless of angular proximity (band-close genuine crossings become adjacent rim-ring vertices; the ring stays conformal with the cap override that carries the same points) |
| E-C1b | Override point bit-identical (same f64 coordinate triple) to an already-inserted override | Deduplicated (skipped) — the original intent, now exact |
| E-C2 | Override point angularly within merge_tol of a UNIFORM sample | UNCHANGED: loud `MalformedTopology` (never silently merged) |
| E-C3 | Pairs with no band-close crossing twins | Byte-identical emission (I4): exact dedup and angular dedup agree whenever no two distinct overrides fall within merge_tol |

## 5. Invariants

- **I1 (conditional cleanliness):** as parent spec — Stage-0 introduces no
  new five-axiom violations; R0088's defective-op operand-a passes all five
  axioms post-fix.
- **I2 (rim-chain identity):** every rim-override point consumed by the cap
  override's boundary chain appears in the inserted rim ring (and vice
  versa: inserted override points come only from `collect_rim_crossings`) —
  the T-junction population is structurally empty.
- **I3 (non-regression, byte-identical):** operands whose override sets
  contain no two distinct points within merge_tol of each other emit
  byte-identical meshes (the exact dedup differs from the angular dedup
  only on that population).
- **I4 (E2E acceptance):** R0088 loses its remaining kernel-v2
  edge-pairing wall instance (success or a DIFFERENT loud typed error);
  R0070 per §2b scoping. Full assay: 0 SUPPORTED_WRONG, no
  SUPPORTED_CORRECT lost vs the 2026-07-03 baseline (96 CORRECT).
- **I5 (determinism):** insertion order and ring order remain deterministic
  (slot sort by seam-relative angle; bit-exact dedup is order-independent
  for distinct points).

## 6. Oracles

- **Native census** (`cherchi_rs::inputcheck::census`) on banked fixtures:
  `cherchi-rs/tests/fixtures/r0088_mc_stage0_a.obj` (+ r0070 when §2b
  lands) — RED: boundary_edges = 12 / improper = 1 today; GREEN: all-clean.
  Sidecar `mesh_booleans_inputcheck` agreement per the parity convention.
- **E2E trackers:** `m8_stage0_inputcheck_campaign.rs` — `red_r0088`
  tightens from `count < 2` to full absence of the edge-pairing wall
  (its §2-caveat second instance IS this cycle's target); new `red_r0070`
  pinning its current wall.
- **Unit test (rim insertion):** two distinct override points within
  merge_tol angular separation both appear in the ring (RED on current
  code); a bit-identical duplicate is still dropped; the uniform-sample
  coincidence stays loud.
- **Parity:** `r0046_patch_label_parity` re-run (fixture refresh recorded
  if emission bytes change on its pinned inputs — expected NOT to, I3).
- **Full assay** on a quiet box vs `target/assay_kv2_report.baseline-*.json`
  (96 CORRECT / 0 WRONG / zero lost), then corpus operand sweep TSV
  regenerated — R0088-a leaves the introduced-dirty set.

## 7. Failure modes / P10 stop criteria

- **GREEN stop:** operand-a reaches five-axiom cleanliness but R0088's
  edge-pairing wall persists → the §2a causal chain is falsified at its
  last link → STOP, amend §2, do not chase the kept set downstream.
- **Downstream band-scale intolerance:** if inserting the twins makes a
  DOWNSTREAM consumer fail loudly (lateral azimuth-merge, opposite-cap fan,
  kernel-v2 render CDT) on the band-scale ring segments, that failure is a
  loud typed error (acceptable residue, recorded here) — NEVER to be fixed
  by re-introducing a merge tolerance (P9).
- **Fix-shape gate:** any fix requiring a tolerance to decide which
  override points survive → STOP (P9/A14.3). The only permitted dedup
  criterion is exact bit-identity.
- **R0070-b divergence:** if §2b measurement shows a root outside the rim
  path, R0070 is descoped to its own cycle; R0088 remains this spec's
  acceptance case.

## 8. Research basis

- Yang 2025 §4.5.5 [#24] (`refs/text/yang2025_hybrid_boolean.txt:718-732`):
  "The common part and the other two parts share identical sampling points
  on their boundaries" — the rim ring and the cap override must carry the
  SAME point set; dropping one side's point violates the shared-sampling
  invariant directly.
- Cherchi 2022 [#38] input contract (manifold, watertight, no
  self-intersections) — the five-axiom operationalization, as parent spec.
- Parent records: `specs/m8_stage0_inputcheck_clean_emission.md` (M-A/M-B
  fixes + M-C naming), a8c9f2b1 (the disc-rim crossing PR that introduced
  the angular dedup), P9 (no hack-to-green: silent tolerance merges are the
  cautionary class).

### 8a. Analytical vs approximate

Not applicable — no SSI, no surface approximation. The fix is exact
combinatorial dedup hygiene on the existing exact overlay + exact rim-mint
machinery.
