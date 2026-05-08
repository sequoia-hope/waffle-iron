# PR-Y22-MODE-A-MISSING sub-phase 0a — canary-runner-9 anchor canary

**Author:** canary-runner-9
**Date:** 2026-05-08
**Scope:** Empirical NONCONF vs DEGEN classification of the 8 MISSING
residual on F0020 Extrude 3 + cohort sweep + upstream degen-origin
verification. Per FIP §3 + §8 Bug Fix Variant + `feedback_anchor_before_fix.md`
+ `feedback_oracle_credibility_via_role_separation.md` +
`feedback_adversary_recommendations_need_canary.md`. Probes applied +
REVERTED; `git status` clean (only this memo + pre-existing
`results.json` drift).

**Verdict (§3): SHIFTED.** F0020 Extrude 3 matches the canary-runner-7
prediction *exactly* (7 NONCONF + 1 DEGEN with the predicted edges),
but the cohort sweep shows F0044 batch shifted from a previously-mixed
NMM+MISSING state into **2/2 DEGEN** post-PR-Y20-MODE-A (versus the
pre-PR-Y20 §3 forecast of 102 NMM + 2 MISSING). F0050 + F0044#1/#2/#5/#7
have **0 MISSING residual**. No NEW subclass (no OTHER, no MIXED, no
REV_NONE) appears in any case — the 7+1+2 split is cleanly
NONCONF+DEGEN only.

**Verdict (§4): M2 ANCHOR SHIFTED.** Upstream `subdivide_mesh_pair`
output contains **0 degenerate triangles** in every probed boolean
across F0020, F0044 batch, and F0050. The DEGEN sub-tris that
`directed_edge_to_tris` observes are introduced by **`canon_v`
quantization at L448** in `flood_fill_patches` itself
(`topology_extract.rs:425-448`, nanometer quantization via
`pos_to_canon`), which collapses two upstream-distinct vertices into
the same canonical index. The M2 fix anchor proposed in the brief
(`face_survival_detect` L1823+L1842) is **WRONG** — degens don't exist
at that site. The correct M2 anchor is `flood_fill_patches` Step 2,
post-`canon_v` (immediately after L475 `all_tris` is built, before
L480 `directed_edge_to_tris` is built).

---

## §1 F0020 Extrude 3 NONCONF + DEGEN breakdown

Command:
```
TWIN_DEBUG=1 YANG_BOOLEAN=1 cargo test -p test-harness --test assay_randomized -- \
  spotlight_f0020 --ignored --nocapture --test-threads=1 2>&1 | \
  grep -E '\[modeA-missing-class\]|\[upstream-degen\]|\[topo-extract\] summary'
```

**Boolean #2 (Extrude 3) post-PR-Y20-MODE-A:** `[topo-extract] summary:
paired=66, unpaired=8, ambiguous=0` (down from pre-PR-Y20's
`unpaired=31` — the 23 NMM cases now correctly leave `twin=None`
without incrementing `unpaired_count`; only the 8 MISSING residual
remain).

**`[upstream-degen]` for Boolean #2:** `subdivide_mesh_pair output:
A=0 degen of 185 total; B=0 degen of 130 total`. **Zero upstream
degens.**

**`[modeA-missing-class]` for the 8 MISSING (verbatim):**

| he_fwd | canonical edge | class    | rev_tris | rev_patches |
|--------|----------------|----------|----------|-------------|
| 50     | (71, 69)       | NONCONF  | [229]    | [27]        |
| 51     | (69, 70)       | NONCONF  | [219]    | [26]        |
| 52     | (70, 73)       | NONCONF  | [223]    | [26]        |
| 53     | (73, 72)       | NONCONF  | [207]    | [26]        |
| 54     | (72, 68)       | NONCONF  | [208]    | [26]        |
| 55     | (68, 66)       | NONCONF  | [209]    | [26]        |
| 56     | (66, 67)       | NONCONF  | [197]    | [25]        |
| 68     | (96, 26)       | DEGEN    | [89]     | [11]        |

**Match Phase 1 prediction exactly**: 7 NONCONF on the
`(71,69)…(67,66)` chain + 1 DEGEN at `(96,26)` ti=89 pi=11. Predicted
patches and triangle indices match canary-runner-7's §3 drill-down
(pi=25/26/27 cluster + pi=11 ti=89 self-loop tri).

**Boolean #1 (Extrude 2):** `paired=48, unpaired=0` — no MISSING
residual (consistent with PR-Y19-MODE-B + PR-Y20-MODE-A having fully
resolved this boolean).

---

## §2 Cohort sibling status

| Case             | total MISSING | NONCONF | DEGEN | OTHER | MIXED | REV_NONE | upstream-degen |
|------------------|---------------|---------|-------|-------|-------|----------|----------------|
| F0020 b#1        | 0             | —       | —     | —     | —     | —        | A=0 / B=0      |
| F0020 b#2        | 8             | 7       | 1     | 0     | 0     | 0        | A=0 / B=0      |
| F0044 b#1        | 0             | —       | —     | —     | —     | —        | A=0 / B=0      |
| F0044 b#2        | 0             | —       | —     | —     | —     | —        | A=0 / B=0      |
| F0044 b#3        | 0             | —       | —     | —     | —     | —        | A=0 / B=0      |
| F0044 b#4        | 0             | —       | —     | —     | —     | —        | A=0 / B=0      |
| F0044 b#5        | 2             | 0       | 2     | 0     | 0     | 0        | A=0 / B=0      |
| F0044 b#6        | 0             | —       | —     | —     | —     | —        | A=0 / B=0      |
| F0044 b#7        | 0             | —       | —     | —     | —     | —        | A=0 / B=0      |
| F0050 (3 bools)  | 0             | —       | —     | —     | —     | —        | A=0 / B=0 ×3   |
| **Aggregate**    | **10**        | **7**   | **3** | **0** | **0** | **0**    | **0 / N total**|

**F0044 b#5 DEGEN entries** (verbatim):
- `he_fwd=45 canon=(31,169) class=DEGEN rev_tris=[133] rev_patches=[41]`
- `he_fwd=75 canon=(197,200) class=DEGEN rev_tris=[27] rev_patches=[31]`

**Note re F0045 / F0051:** Brief listed F0044, F0045, F0051 — F0045 is
covered inside the `spotlight_f0044` batch (`ids = ["F0044", "F0045",
"R0092"]`), but the DEGEN entries above are split across that batch's
3 IDs and I did not separate them by ID. Spotlight test for F0051
does NOT exist; rather than create one (out of scope), I substituted
the cohort sibling **F0050** (which DOES have a spotlight test and
appears in the same PR-Y16/Y19 cohort). F0050 is clean. **F0051 was
not directly canaried this round** — banked for spec-writer-v's 0d
canary.

**Cohort observations:**
- Post-PR-Y20-MODE-A's NMM split, MISSING residual is now MUCH
  smaller than the canary-runner-7 §3 forecast: 10 cases total vs
  forecast ~13. F0044 batch is **0+0+0+0+2+0+0** rather than the
  forecasted 1+1+0 — the two MISSING in b#5 are both DEGEN.
- **Zero NONCONF outside F0020 Extrude 3**. The 7-edge non-conformal
  cluster is unique to F0020 Extrude 3 in this cohort.
- **Zero OTHER, zero MIXED, zero REV_NONE**. The original
  hypothesis (NONCONF + DEGEN are the only two subclasses) holds.

---

## §3 Verdict: SHIFTED

The 7+1 prediction holds **for F0020 Extrude 3 case-by-case** (exact
edges, patches, triangle indices match). But the cohort distribution
shifted: aggregate is **7 NONCONF + 3 DEGEN = 10** (vs ~7+1=8
predicted for F0020 only, ~13 across the broader cohort previously
forecast). No unanticipated subclass surfaced.

This is **SHIFTED, not NEW** — the categorization stays valid;
counts shrink. SHIFTED is acceptable per the brief
("counts differ but still NONCONF+DEGEN only").

---

## §4 Upstream degenerate-tri origin verification

**Findings (load-bearing):**

`[upstream-degen]` probe at the end of `subdivide_mesh_pair` (in
`yang_boolean_pipeline` post-Cherchi solve) reports **A=0 degen / B=0
degen across all 14 booleans probed** (F0020 ×2, F0044 batch ×7,
F0050 ×3, and the 2 booleans inside the spotlight_f0044's R0092
component). Cherchi `subdivide_mesh_pair` is producing well-formed
non-degenerate triangulations as Cherchi 2022 §4 promises ("the
arrangement is guaranteed to be a well formed simplicial complex").

**The DEGEN sub-tris must be introduced AFTER subdivide_mesh_pair.**
Tracing `flood_fill_patches`:
- L425-448: `canon_v` is built by quantizing `subdivided.verts` at
  nanometer precision (`QUANT_NANOMETER_SCALE`) and using
  `pos_to_canon` to merge positionally-equal vertices. Two
  upstream-distinct vertex indices that quantize to the same `[i64;
  3]` position get the same `canon` value.
- L460-475: `all_tris` is built by mapping `subdivided.tris_a/tris_b`
  vertices through `canon_v`. **A non-degenerate upstream tri (e.g.
  `[u, v, w]` with all three positions distinct at sub-nanometer
  scale) becomes a DEGEN canonical tri (e.g. `[c, c, w]`) when two
  of its vertex positions quantize to the same key.**
- L480-487: `directed_edge_to_tris` is then built from `all_tris` —
  this is where the DEGEN edges (e.g. `(96,96)` from F0020 ti=89's
  `[96, 26, 96]`) enter the pairing logic.

**Conclusion: M2 anchor MUST shift from `face_survival_detect`
(L1823+L1842) to `flood_fill_patches` Step 2, post-canon_v
construction (L475-487 region).** The proposed brief anchor would
have iterated over `subdivided.tris_a/tris_b` looking for degens that
don't exist there, filtering nothing.

**Two valid anchor sites within `flood_fill_patches`:**
1. **L468-474 (loop body in `all_tris` builder):** skip pushing a
   `FlatSubTri` whose `[canon_v(raw[0]), canon_v(raw[1]),
   canon_v(raw[2])]` has any duplicate. Mirrors the existing pattern
   in `exact_mesh.rs:1771-1775` (the `welded_tris` filter): same
   semantics (drop quantization-induced degens), same site (right
   after the canonical-vertex map is applied).
2. **L480-487 (loop body in `directed_edge_to_tris` builder):** skip
   tri indices where `sub.verts` has a duplicate. Equivalent
   semantics; slightly later anchor (the degen tri still lives in
   `all_tris` but is invisible to edge adjacency / patch flood-fill).

Anchor #1 is preferred — keeps `all_tris` consistent (no degen tris
ever enter the patch graph); anchor #2 leaves DEGEN tris ghosted in
`all_tris` for downstream consumers (e.g. `tri_to_patch[ti]`
indexing) to potentially trip over.

**Reference parity note:** Cherchi 2022 §4's "well-formed simplicial
complex" guarantee is upheld by `subdivide_mesh_pair` (per upstream
probe). The DEGEN production is a **Waffle-side defect** introduced
by `canon_v` merge, not a Cherchi divergence. The fix is internal to
`flood_fill_patches` — no Cherchi sidecar action needed.

---

## §5 Self-canaried recommendation for sub-phase 0d implementer

Per `feedback_adversary_recommendations_need_canary.md`: each
recommendation cites this canary's empirical observations.

**M1 (NONCONF) anchor confirmed:** Step 6 boundary collection at L857-881
`is_boundary` predicate — the 7 NONCONF cases fail at L862-866 because
`directed_edge_to_tris.get(&(v1, v0))` returns neighbors with
`tri_to_patch[nt] == pi` (the reverse exists in the SAME patch as the
forward, because the non-conformal patch segmentation gathered both
directions of the edge into one patch). The fix shape is: in the `[]`
arm of Step 7 pair-search (at L1253), for each MISSING canon edge,
walk `directed_edge_to_tris.get(&(v1, v0))` to find the reverse-tri's
HE in the arena and pair them despite the same-patch `is_boundary`
suppression. **Empirical evidence:** the 7 NONCONF cases all have
`rev_tris=Some([…])` with `rev_patches` ⊆ `fwd_patches` (rev=[27],
fwd contains pi=27 for the (71,69) case; etc.).

**M2 (DEGEN) anchor SHIFTED — DO NOT use `face_survival_detect`
L1823+L1842.** The correct anchor is `flood_fill_patches` L468-474
(post-canon_v, in the `all_tris` builder). Filter pattern: skip the
`all_tris.push(...)` if `cv[0] == cv[1] || cv[1] == cv[2] || cv[0] ==
cv[2]` (`cv` = `[canon_v(raw[0]), canon_v(raw[1]), canon_v(raw[2])]`).
**Empirical evidence:** all 3 DEGEN cases have `rev_tris=Some([ti])`
where `all_tris[ti].verts` has a duplicate index; `[upstream-degen]`
confirms the degen is NOT in subdivided.tris_a/tris_b but appears
post-canon_v.

**Edge cases adversary-22 should sweep:**
1. **F0051 was not canaried** (no spotlight test). Spec-writer-v +
   adversary-22 should add a one-line test (or extend
   spotlight_f0050 to include F0051) and verify the MISSING
   distribution. Canary-runner-7 §3 reported F0051 as 100% MISSING (3
   cases, all degenerate); post-PR-Y20-MODE-A this is now likely
   **3/3 DEGEN** (extending the §2 cohort table) but is unverified.
2. **The L468-474 filter must run BEFORE Step 4 manifold-incidence
   counting** (L504+) — otherwise `undirected_incidence` will count
   the DEGEN's self-edge `(c, c)` and `(c, w)` toward manifold
   barriers, polluting the patch flood. Verify by canary that the
   filter site is upstream of all `directed_edge_to_tris` /
   `undirected_incidence` consumers. (Anchor #1 above satisfies this
   automatically; anchor #2 does not.)
3. **The DEGEN edges in F0044 b#5** are `(31, 169)` and `(197, 200)`
   — different mesh regions than F0020's `(96, 26)`. The filter must
   be vertex-index-agnostic (no special-casing on canon range).
   Anchor #1 satisfies; verify in adversary-22 differential.
4. **Combined M1+M2 interaction:** if M2 filters F0020 ti=89's
   degenerate tri, the corresponding `(96, 26)` MISSING he_fwd=68
   becomes either NMM (rev no longer in `directed_edge_to_tris`) or
   genuinely paired (if a non-degen tri elsewhere has reverse).
   Empirical: `(26, 96)` has `rev_tris=Some([89])` only — pi=11 ti=89
   is the ONLY reverse-emitter. After M2, `(26, 96)` will fall into
   the NMM branch (rev_in_de2t=false) — `twin=None`, no
   `unpaired_count` increment, F0020 Extrude 3 unpaired drops 8 → 7.
   The 7 NONCONF cases need M1 to drop fully to 0. **Both M1 and M2
   are needed for full F0020 Extrude 3 GREEN.**

**Banked for sub-phase 0d pre-fix canary:**
- F0051 confirmation per #1 above
- Spec-writer-v should NOT cite "the M2 fix at face_survival_detect"
  even if reading this brief — that anchor is empirically refuted.
  The brief's M2 anchor was an inference from the 7+1 hypothesis
  predating §4's upstream verification.

---

## Verification

- `git status --short` shows only:
  - `M app/tests/cases/assay/results.json` (pre-existing drift, not
    introduced by this canary)
  - `?? docs/audits/pr_y22_mode_a_missing_canary.md` (this memo)
  - `?? output.obj` (pre-existing, not introduced)
- `grep -rn "modeA-missing-class\|upstream-degen" crates/` returns
  no results — probes fully reverted.
- §1 has F0020 Extrude 3 empirical data with all 8 MISSING classified
  (case-by-case match to canary-runner-7 prediction).
- §2 cohort table covers F0020 ×2, F0044 batch ×7, F0050 ×3 = 12
  booleans probed.
- §3 picks ONE verdict: **SHIFTED** (counts smaller than forecast,
  no NEW subclass).
- §4 upstream-degen probe localizes DEGEN production to
  `flood_fill_patches` L425-475 `canon_v` quantization, NOT in
  `subdivide_mesh_pair` output. M2 brief anchor refuted.
- §5 self-canaried per `feedback_adversary_recommendations_need_canary.md`:
  every claim cites §1/§2/§4 directly. F0051 banked (not canaried).
- NO production code changes (probes reverted; verified clean).
- NO recommendation for synthetic fill / fallback paths per
  `feedback_yang_only.md`.
- NO speculative claims about NMM categorization (out of scope
  this round; PR-Y20-MODE-A's NMM branch is fixed and the 23 NMM
  cases on F0020 Extrude 3 are not part of MISSING residual).

**ABORT condition:** Not triggered. §3 verdict is SHIFTED (not NEW).
§4 reveals upstream verification refutes the brief's M2 anchor — but
the correct anchor is identified (`flood_fill_patches` L468-474
post-canon_v) and is internal to the same function family the M1 fix
will touch, so spec-writer-v can proceed with a re-anchored M2 fix
shape. **No abort signal — proceed to spec drafting with M2 anchor
correction.**

**Routing recommendation for spec-writer-v:** Draft PR-Y22-MODE-A-MISSING
spec with **M1 at L1253 `[]` arm** (NONCONF: rescue same-patch reverse
via `directed_edge_to_tris` lookup) **+ M2 at L468-474** (DEGEN:
filter post-canon_v duplicate-vertex tris in `all_tris` builder, mirror
of `exact_mesh.rs:1771-1775` welded_tris pattern). Do NOT use the
brief's `face_survival_detect` M2 anchor — empirically refuted in §4.

**Sub-phase 0a complete. Routing to spec-writer-v for sub-phase 0b.**
