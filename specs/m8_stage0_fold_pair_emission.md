# M8 — Stage-0 fold-pair emission (annulus angular-merge visibility)

**Status:** spec (FIP Phase 1) — §2 Measured mechanism COMPLETE (2026-07-04).
**Change class:** bug fix (modeling-related), M8 workstream — the fold-pair
residue named by `specs/m8_stage0_inputcheck_clean_emission.md` §2a and
quantified by both corpus operand sweeps. **Crates:** `yang-rs`
(`stage0.rs` `annulus_tris`), `cherchi-rs` (census fixtures — oracle only),
`test-harness` (diagnosis drivers, already landed).

## 1. Goal

Same conditional contract as the parent cycles: every operand Stage-0 hands
to the native boolean is five-axiom clean whenever its solid's Stage-1 mesh
is. This cycle closes the **fold-pair class** — the LAST introduced-defect
class in the 2026-07-04 sweep
(`docs/audits/stage0_operand_inputcheck_sweep_2026-07-04.tsv`): 8 dirty
operands (F0027/F0028/F0029-a, R0007-b, R0023-b ×2, R0038-a, R0089-a), all
with misoriented (same-traversal) manifold edge pairs + improper exact
contacts and ZERO boundary edges.

**No-regression sentinels:** F0027 and F0028 are currently
SUPPORTED_CORRECT *despite* their dirty operands (cherchi tolerates these
folds) — the fix must keep them CORRECT (assay I4). F0029/R0023/R0038/R0089
are ERROR-class; R0007 walls UNSUPPORTED(coplanar) on a different op.

## 2. Measured mechanism (2026-07-04, diagnosis drivers + geometric
## reconstruction on the dumped operands)

All dirty ops route through the **disc-pair direct builders**
(`build_disc_pair` / `build_disc_disc_containment` — confirmed by the
absence of an overlay dump for exactly the dirty ops) and the defect lives
in **`annulus_tris`**, the angular-merge triangulation of the region
between the nested convex rings (outer polygon/rim, inner rim):

1. F0027 op `000_union` operand-a face 0 (polygon face containing an
   11-segment disc, `opposite=true`): tris 0-10 = the shared overlap fan,
   tris 11-25 = the annulus. Census: 6 misoriented pairs + 2 improper, all
   face 0. Reconstruction: NO negative-area triangles — the misoriented
   pairs are **same-side overlaps**: annulus tris 13 `(11,12,corner 3)` and
   21 `(17,18,corner 1)` overlap the overlap fan (e.g. fan tri 2 and tri 13
   both traverse edge 11→12 in the same direction).
2. Root: the merge picks the advance purely by ANGLE
   (`ia[i+1] <= oa[j+1]` about the inner centroid O). Angular monotonicity
   does not imply visibility: a far outer corner can lie on the **center
   side of a distant inner chord's supporting line** even while far outside
   the circle (measured: corner 3 at azimuth −2.503, radius 0.31 vs chord
   (11,12) at azimuth [−1.571,−1.0], R=0.117 — exact side test +3e-4, same
   sign as O's +7.4e-3). Fanning that chord to that corner emits a
   positive-area sliver that pokes across the inner ring — a fold pair with
   the fan + improper contacts with its neighbors, and no boundary edges
   (the pleat double-covers; watertightness is preserved).
3. The `tri` closure's orientation normalization (swap when the exact cross
   is negative) is sound for VALID merges (the natural chord→outer order is
   CW and gets swapped) but also silently "repairs" genuinely invalid
   merges, which is why nothing failed loudly.
4. Same census signature on F0029 (6 misoriented + 4 improper), R0038-a op
   `001_subtract` (7 + 2), R0089-a op `003_union` (6 + 1) — two fold
   triangles each, clustered around two outer corners. R0007/R0023 share
   the sweep signature (misoriented+improper, no boundary edges) and the
   same builder route.

## 3. Parameters

None new. No tolerances (A14.3) — the guard is an exact rational half-plane
predicate; the certificate is exact shoelace arithmetic.

## 4. Branch table (annulus merge × validity)

| # | Path | Trigger | Contract row |
|---|------|---------|--------------|
| E-F1 | **[fix]** Inner-advance candidate `(inner_i, inner_i+1, outer_j)` with `outer_j` strictly on the OUTER side of the chord's supporting line (exact: `cross(c1,c2,outer_j)` nonzero and opposite in sign to `cross(c1,c2,O)`) | Emitted (orientation normalized as today); the angle preference is unchanged when valid — byte-identical for the entire currently-valid population (I3) |
| E-F2 | **[fix]** Inner-advance candidate whose outer vertex is on the chord's center side or exactly on its line | NOT emitted; the merge advances the OUTER chain instead (the chord fans to a later, visible outer vertex) |
| E-F3 | Outer-advance candidate `(outer_j, outer_j+1, inner_i)` | Emitted with `inner_i` required strictly on O's side of the outer edge (exact; guaranteed by convex nesting — a violation is a loud E-F5) |
| E-F4 | **[new certificate]** Merge completes | Exact coverage certificate: Σ (positive) triangle areas == area(outer) − area(inner), computed by rational shoelace over the same exact coords. Mismatch → `None` |
| E-F5 | Both advances unavailable/invalid at any step, or certificate mismatch | `None` → the existing loud `DiscPair::Wall("disc-annulus-tri")` typed residue (never a silent fold) |

## 5. Invariants

- **I1 (conditional cleanliness):** the 8 fold-pair operands emit five-axiom
  clean (or their op stops loudly pre-backend); sweep introduced-dirty
  count → 0.
- **I2 (coverage exactness):** every returned annulus triangulation
  satisfies the E-F4 certificate — no double-cover, no gap, exactly the
  annulus (P1/P9: correctness is measured, not assumed).
- **I3 (non-regression, byte-identical):** rings where the angle-preferred
  merge is everywhere valid produce byte-identical triangle lists (the
  guard only redirects invalid advances; the certificate only observes).
- **I4 (E2E acceptance):** F0027/F0028 stay SUPPORTED_CORRECT; full assay
  0 SUPPORTED_WRONG, zero CORRECT lost vs `baseline-m8mc`. ERROR-class
  members may keep downstream walls (their acceptance is operand-level).
- **I5 (determinism):** the merge remains deterministic (exact predicates
  on deterministic inputs; no iteration-order change).

## 6. Oracles

- **Census RED fixtures:** `cherchi-rs/tests/fixtures/f0027_foldpair_a.obj`
  (the CORRECT-despite-dirty sentinel) + `r0038_foldpair_a.obj`
  (subtract-side member), banked from the current defective emission; RED =
  census finds the measured misoriented/improper counts; GREEN = re-banked
  from the fixed emission, all-five clean + sidecar agreement (added to the
  `sidecar_inputcheck_agrees_on_banked_operands` list).
- **Unit tests (annulus_tris):** (RED) a square-outer × 11-gon-inner
  reproduction of F0027's configuration yields an overlap-free
  triangulation — exact pairwise no-overlap + the E-F4 certificate as the
  assertion; (GREEN adds) certificate holds on representative valid rings
  byte-identically to today's output (I3 guard).
- **E2E:** the full assay (I4) + the corpus operand sweep TSV regen
  (introduced-dirty → 0 expected).
- **Witnesses:** m8_disc_coplanar suite (the disc-pair builders' own
  suite), m8 campaigns, fuzz_boxes differential, patch-label parity.

## 7. Failure modes / P10 stop criteria

- **GREEN stop:** operands reach cleanliness but F0027/F0028 flip away from
  SUPPORTED_CORRECT → the fix regressed the sentinel population → STOP,
  amend §2 (do NOT band-aid downstream).
- **Deadlocked merge (E-F5):** a containment configuration where the
  guarded merge cannot complete becomes a loud typed
  `Wall("disc-annulus-tri")` — acceptable M8 residue, recorded here; never
  to be resolved by re-admitting invisible fans or re-enabling the silent
  orientation repair.
- **Fix-shape gate:** any tolerance deciding advance validity → STOP
  (P9/A14.3). The predicates are exact sign tests; the certificate is exact
  equality.

## 8. Research basis

- Yang 2025 §4.5.5 [#24] (`refs/text/yang2025_hybrid_boolean.txt:718-732`):
  the coplanar overlap segmentation whose A-only/B-only regions these
  builders emit; identical shared-boundary sampling is why the annulus must
  preserve both rings' vertices exactly (no Steiner, no keyhole — the
  existing design, unchanged).
- Standard monotone-strip triangulation between nested convex polygons:
  the angular merge is the established construction; the addition is an
  exact half-plane VISIBILITY guard (the strip triangle with an inner
  chord is valid iff the outer apex lies in the chord's outer half-plane)
  plus the house-style exact coverage certificate
  (`triangulate_ring`'s P9-gate precedent, spec `m8_nonstar_ring_earclip`).
- Prior records: `specs/m8_stage0_inputcheck_clean_emission.md` (named this
  class), `specs/m8_stage0_band_scale_crossing_verts.md` (the sibling M-C
  cycle; same census/dump tooling), PR-M8-disc
  (`kernel_v2_m8_disc_containment` — introduced `annulus_tris`).

### 8a. Analytical vs approximate

Not applicable — no SSI, no surface approximation; exact combinatorial
triangulation hygiene on the existing exact ring machinery.
