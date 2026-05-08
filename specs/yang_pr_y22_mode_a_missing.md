# PR-Y22-MODE-A-MISSING — F0020 NONCONF same-patch retroactive pairing + DEGEN canon-induced filter

**Author:** spec-writer-v · **Date:** 2026-05-08 · **Plan:** sub-phase 0b
**Canary:** `docs/audits/pr_y22_mode_a_missing_canary.md` (canary-runner-9)
— SHIFTED. F0020 Ext.3 = 7 NONCONF + 1 DEGEN (matches Phase 1 prediction
exactly); F0044 b#5 = 0 NONCONF + 2 DEGEN (counts smaller than forecast,
no NEW subclass); F0050 = 0 MISSING residual; F0051 not canaried (banked).
**M2 anchor SHIFTED** vs original brief: degens are NOT in
`subdivide_mesh_pair` output (Cherchi 2022 §4 well-formed simplicial
complex guarantee holds); they are introduced by `canon_v` nanometer
quantization at `topology_extract.rs:425-475` inside `flood_fill_patches`.
**M1 anchor RESHAPED** vs original brief: NONCONF rev exists in
`directed_edge_to_tris` (rev_in_de2t=true, rev_in_same_patch=true), so it
is MANIFOLD-but-mis-segmented, NOT non-manifold; PR-Y20-MODE-A's NMM
treatment would mis-classify and silence M1 failure. Correct fix is
RETROACTIVE PAIRING per Yang §4.4.2 directional-symmetry. **FIP §3 + §8
Bug Fix Variant.**

## §1 Goal

Drive F0020 Extrude 3 [twin-oracle] `unpaired_count` from **8 → 0** via
two targeted fixes inside `flood_fill_patches`:

- **M1 (NONCONF, 7 cases on F0020 Ext.3 chain `(71,69)…(67,66)`):** rescue
  same-patch manifold edges by emitting BOTH directions as boundary HEs at
  Step 6 (Path A — preferred); they then pair naturally at Step 7 L1232
  `[the_one]` arm.
- **M2 (DEGEN, 1 case on F0020 Ext.3 + 2 on F0044 b#5):** filter
  quantization-induced degenerate sub-tris in the `all_tris` builder
  (post-canon_v, L468-474), mirroring the `welded_tris` filter at
  `exact_mesh.rs:1771-1775`.

Per `feedback_yang_only.md`: F0020 spotlight Status:Failed may persist
post-PR (downstream tessellation NMM-render layer still open from PR-Y21
ABORT). Success metric is the **layer-targeted** drop 8→0, not the
case-level Status flip. Per `feedback_no_last_bug.md`: PR-Y22 fixes ONE
layer; further layers will surface and are next-PR territory.

## §2 Background

Post-PR-Y20-MODE-A baseline (canary §1): F0020 Boolean #2 reports
`paired=66, unpaired=8, ambiguous=0`. The 23 NMM cases now correctly
leave `twin=None` without incrementing `unpaired_count`; only the 8
MISSING residual remain. Canary-runner-9 §1 classified all 8:

| canon edge   | class    | rev_tris | rev_patches |
|--------------|----------|----------|-------------|
| (71, 69)     | NONCONF  | [229]    | [27]        |
| (69, 70)     | NONCONF  | [219]    | [26]        |
| (70, 73)     | NONCONF  | [223]    | [26]        |
| (73, 72)     | NONCONF  | [207]    | [26]        |
| (72, 68)     | NONCONF  | [208]    | [26]        |
| (68, 66)     | NONCONF  | [209]    | [26]        |
| (66, 67)     | NONCONF  | [197]    | [25]        |
| (96, 26)     | DEGEN    | [89]     | [11]        |

Cohort sweep (canary §2): F0044 b#5 has 2 DEGEN (no NONCONF); F0050 has
0 MISSING; F0020 b#1 / F0044 b#1-4,6,7 are clean. **Total cohort
addressed by PR-Y22 = 10 MISSING residual (8 on F0020 + 2 on F0044
b#5).**

Reference parity context (canary §4): the brief's M2 anchor at
`face_survival_detect` L1823+L1842 was empirically refuted —
`subdivide_mesh_pair` output contains 0 degens across all 14 booleans
probed (F0020 ×2, F0044 batch ×7, F0050 ×3, R0092 ×2). The DEGEN
sub-tris are produced **inside** `flood_fill_patches` Step 1-2 by
`canon_v` quantization collapsing two upstream-distinct vertices to
the same canonical index. M2 must be re-anchored to the
`flood_fill_patches::all_tris` builder, post-canon_v.

## §3 M2 spec (DEGEN, simpler — sub-spec first)

**File:** `crates/kernel/src/boolean/topology_extract.rs`
**Site:** Step 2 `all_tris` builder, L460-475 (loop body L468-474).
**Pattern:** Mirror `exact_mesh.rs:1771-1775` welded_tris filter.

After computing `cv = [canon_v(raw[0]), canon_v(raw[1]), canon_v(raw[2])]`,
skip the `all_tris.push(...)` if `cv[0]==cv[1] || cv[1]==cv[2] || cv[0]==cv[2]`.

```rust
let cv = [canon_v(raw[0]), canon_v(raw[1]), canon_v(raw[2])];
if cv[0] == cv[1] || cv[1] == cv[2] || cv[0] == cv[2] {
    // Quantization-induced degenerate sub-tri: two upstream-distinct
    // vertices collapsed to the same canonical index by canon_v's
    // nanometer quantization. Cherchi 2022 §4's "well formed simplicial
    // complex" output contract is preserved at OUR consumer layer by
    // dropping these post-canon_v. Mirrors exact_mesh.rs:1771-1775
    // welded_tris filter.
    continue;
}
all_tris.push(FlatSubTri {
    verts: cv,
    source: *sf,
    cosurface_orientation: tri.cosurface_orientation,
    parent_tri: tri.parent_tri,
});
```

**Why L468-474 (vs L480-487 directed_edge builder):** anchor here keeps
`all_tris` and `directed_edge_to_tris` consistent. If filtered later (at
the directed-edge builder), the DEGEN tri remains ghosted in `all_tris`
and `tri_to_patch[ti]` indexing for downstream consumers (Step 4
manifold incidence, Step 5 patch flood, Step 6 boundary) sees stale
entries. Canary §5 edge case #2 confirms: filter must run BEFORE Step 4
manifold-incidence counting at L504+.

**Why NOT `face_survival_detect` L1823+L1842:** canary §4 verified
upstream Cherchi output is degen-free in 14/14 probed booleans; the
brief's anchor would have iterated over already-clean data and filtered
nothing.

**Reference contract:** Cherchi 2022 §4 "the arrangement is guaranteed
to be a well formed simplicial complex" — the upstream invariant is
intact; M2 is a Waffle-side defense against a quantization step Cherchi
does not perform. NOT a fallback per `feedback_yang_only.md`: this is a
structural barrier preserving the paper's contract through our canon_v
layer.

**LOC:** ~10-15.

## §4 M1 spec (NONCONF — NMM-classification refinement)

### §4.0 R-a EMPIRICAL REVISION (2026-05-08, post-PR-Y22 ABORT)

**This section was rewritten after `docs/audits/pr_y22_recovery_incidence_probe.md`
(R-0 incidence-prober-r) empirically refuted the original "manifold-
mis-segmented" framing.** The original §4 (preserved in git history
through commit predating R-a) prescribed *Path A retroactive emit at
L862* on the assumption that the 7 NONCONF target edges had
`undirected_incidence == 2` with both directions mis-routed into the
same patch. That assumption is **REFUTED**: R-0's direct
`n_fwd + n_rev` probe at L857-881 boundary collection shows
**`n_total = 3` for all 7 cases, uniformly** (R-0 §1 table). Probe
data: `(2,1)` from the side with two coplanar sub-tris emitting the
forward direction, `(1,2)` from the single sub-tri emitting the
reverse — undirected incidence is **3**, not 2.

Per Yang §4.4.2, a manifold edge has incidence exactly 2; edges with
incidence ≠ 2 are **non-manifold (NMM)**. Cherchi 2022 §5 encodes the
identical predicate in its manifold-edge barrier
(`edgeIsManifold` in reference C++ `booleans.cpp:412`; mirrored at
`topology_extract.rs:513-516` `edge_is_manifold` for Step 4 patch
flooding). **The 7 target edges are legitimately NMM by both Yang §4.4.2
and Cherchi 2022 §5 definitions.** They are NOT manifold edges
mis-segmented into one patch; they are genuinely 3-incident.

**Re-framing of M1:** instead of Path A retroactive emit (which would
symmetrize a non-existent manifold pair and force an arbitrary 2-of-3
HE selection violating Yang's 1:1 pairing mandate at the surviving
manifold pair), **M1 extends the existing PR-Y20-MODE-A NMM predicate
at L1270-1291 to also classify edges with `undirected_incidence != 2`
as NMM.** Currently the NMM predicate triggers only when the reverse
direction is entirely absent from `directed_edge_to_tris`; the
extension adds the case where the reverse is present but the
undirected incidence is ≠ 2 (i.e. 3+ for our 7 cases, or 1 in
hypothetical asymmetric cases).

This aligns with Yang §4.4.2 (manifold ↔ incidence == 2) and Cherchi
2022 §5 (`edgeIsManifold` barrier predicate). It is paper-faithful per
`feedback_yang_only.md` (no fallback paths; the paper's classification
IS the classification).

### §4.1 M1 specification

**File:** `crates/kernel/src/boolean/topology_extract.rs`
**Site:** L1270-1291 (the PR-Y20-MODE-A `[]` arm of Step 7 pair-search,
inside the `is_nmm` predicate computation).

**Current code (L1270-1291):**
```rust
let mut is_nmm = false;
if let Some(prov) = he_provenance.get(&he_fwd) {
    let (_, _, _, v0_canon, v1_canon) = *prov;
    is_nmm = !directed_edge_to_tris.contains_key(&(v1_canon, v0_canon));
} else if !twin_debug {
    // … fallback when provenance unavailable …
    is_nmm = true;
}
```

**M1 extension (replace the `is_nmm = !directed_edge_to_tris…` line):**
```rust
let mut is_nmm = false;
if let Some(prov) = he_provenance.get(&he_fwd) {
    let (_, _, _, v0_canon, v1_canon) = *prov;
    let n_fwd = directed_edge_to_tris
        .get(&(v0_canon, v1_canon))
        .map(|v| v.len())
        .unwrap_or(0);
    let n_rev = directed_edge_to_tris
        .get(&(v1_canon, v0_canon))
        .map(|v| v.len())
        .unwrap_or(0);
    let undirected_count = n_fwd + n_rev;
    // Yang §4.4.2: manifold edge ↔ undirected incidence == 2.
    // Cherchi 2022 §5: manifold-edge barrier uses the same predicate
    // (`edgeIsManifold` at booleans.cpp:412; mirrored at
    // topology_extract.rs:513-516 `edge_is_manifold`). Edges with
    // incidence ≠ 2 are non-manifold — twin=None is the correct
    // answer, no `unpaired_count` increment.
    is_nmm = !directed_edge_to_tris.contains_key(&(v1_canon, v0_canon))
        || undirected_count != 2;
} else if !twin_debug {
    // unchanged: fallback to is_nmm = true when provenance unavailable
    is_nmm = true;
}
```

**Why L1270-1291 (vs L862 Step 6 boundary):** the Step 6 boundary
predicate is *correct*: a 3-incident edge legitimately produces both
"forward boundary" and "reverse boundary" emissions, but Step 7 then
fails to pair them because pairing is undefined for incidence != 2.
The defect is at the *classification* layer (Step 7 [] arm
mis-counting these as `unpaired` defects rather than NMM), not at the
boundary-collection layer. R-0 §4 confirms.

**Why NOT Path A retroactive emit at L862:** R-0 §4 explicit refutation:
"there is no manifold edge to retroactively-emit — the edge is
genuinely NMM. Emitting both directions from a 3-incident undirected
edge would still leave one HE unpaired (or worse, force an arbitrary
2-of-3 selection that violates Yang's 1:1 mandate at the surviving
manifold pair)."

### §4.2 Reconciliation with PR-Y20-MODE-A NMM (revised)

PR-Y20-MODE-A handles `rev_in_de2t == false` (no reverse exists in
`directed_edge_to_tris` at all) — genuinely non-manifold, twin=None
is the right answer. PR-Y22 M1 EXTENDS the same NMM predicate to
cover the second NMM case: reverse exists but undirected incidence is
≠ 2 (incidence-3+ for the 7 F0020 cases per R-0 §1).

| condition                                                 | mechanism                  | fix         |
|-----------------------------------------------------------|----------------------------|-------------|
| `rev_in_de2t == false`                                    | NMM (Yang §4.4.2 absence)  | PR-Y20 NMM (twin=None, no unpaired) |
| `rev_in_de2t == true && undirected_count != 2`            | NMM (Yang §4.4.2 incidence)| PR-Y22 M1 — extend NMM predicate |
| `rev_in_de2t == true && undirected_count == 2`            | manifold (Yang §4.4.2)     | existing `[the_one]` pairing |

The PR-Y20 anti-scope rule "do not extend NMM to manifold edges"
remains in force — manifold edges are exactly those with
`undirected_count == 2`, which the new predicate explicitly preserves
in the third row above. M1 only extends NMM coverage to *additional
NMM cases*, never to manifold cases.

**LOC:** ~10-15 (a single-expression predicate extension plus the
two `let` bindings for `n_fwd`/`n_rev`/`undirected_count`).

## §5 Reference parity contract (3 invariants)

**I1 (DEGEN filter, M2):** post-M2, `all_tris` contains no
duplicate-vertex tris. The validator (added in this PR; see §6 test
plan) panics if any `FlatSubTri` with `verts[i] == verts[j]` for `i ≠ j`
leaks into `directed_edge_to_tris`.
- *Paper:* Cherchi 2022 §4 well-formed simplicial complex, preserved
  through canon_v.
- *Test:* `pr_y22_no_degen_in_all_tris` — F0020 Extrude 3 spotlight;
  iterate `all_tris` post-construction; assert no duplicate-vertex tri.

**I2 (NONCONF NMM-classification, M1 — REVISED post-R-a):** at Step 7
`[]` arm (L1270-1291), edges with `undirected_count != 2` are
classified `is_nmm = true` (twin=None, no `unpaired_count` increment).
Per R-0 §1, all 7 F0020 Extrude 3 target canonical edges have
`undirected_count == 3`. Post-M1, none of these contribute to the
`[topo-extract] summary unpaired=N` count.
- *Paper:* Yang §4.4.2 (manifold ↔ incidence == 2; otherwise NMM).
  Cherchi 2022 §5 (`edgeIsManifold` barrier predicate; mirrored at
  `topology_extract.rs:513-516`).
- *Test:* `pr_y22_nonconf_classified_as_nmm` — F0020 Extrude 3
  spotlight; for the 7 NONCONF canon edges (per R-0 §1 table), assert
  the corresponding HE has `twin == None` AND
  `[topo-extract] summary unpaired=N` is decremented to 0 (with M2's
  DEGEN reduction folded in).

**I3 (F0020 Mode A residual GREEN at topology layer):** F0020 Extrude
3 `[topo-extract] summary unpaired=N` drops 8 → 0 post-PR (load-bearing
gate). The 7 NONCONF cases get reclassified to NMM (twin=None, no
unpaired contribution). The 1 DEGEN case (canon edge `(96,26)`) gets
filtered upstream at M2: its sole reverse-emitter is the degenerate
ti=89 in pi=11; once filtered, `(26, 96)` falls into the PR-Y20-MODE-A
NMM branch (rev_in_de2t becomes false) and `twin=None` without
incrementing unpaired (canary §5 edge case #4).

**Note:** the [twin-oracle] `unpaired_count == 2` residual on F0020
documented at adversary-22 §6 is a SEPARATE downstream layer (banked
PR-Y23+); it is NOT load-bearing for PR-Y22 success and is decoupled
in §6 test plan per R-b's amendment.

- *Paper:* Yang §4.4.2 (incidence-based manifold classification);
  Cherchi 2022 §4 (well-formed simplicial complex) + §5
  (`edgeIsManifold`).
- *Test:* `pr_y22_mode_a_missing_invariant` — F0020 spotlight
  `[topo-extract] summary unpaired=N == 0`.

## §6 Test plan

### §6.0 R-a + R-b AMENDMENT (2026-05-08)

The original §6 specified `[twin-oracle] unpaired_count == 0` as the
load-bearing F0020 gate (assertion 2). Adversary-22 §6 + canary
re-baseline showed this value is **2 on plain HEAD** (NOT 0): the
[twin-oracle] residual originates from a separate downstream layer in
the in-process B-Rep build path, and is NOT fixable by PR-Y22's
topology-extract layer fix. R-b amended the regression test to use
`[topo-extract] summary unpaired=N == 0` as the load-bearing gate
(assertion 1) and `[twin-oracle] unpaired_count <= 2` for F0020 (or
`<= 0` for F0044 batch) as a non-regression bound (assertion 2). The
non-zero [twin-oracle] residual is banked as PR-Y23+ downstream layer.

**Required (gating):**
- F0020 Extrude 3 `[topo-extract] summary unpaired=N` drops 8 → 0
  (LOAD-BEARING; assertion 1 in regression test). The 7 NONCONF cases
  reclassify as NMM via M1 (no unpaired increment per L1305-1310
  predicate); the 1 DEGEN case is filtered upstream by M2.
- F0044 b#5 `[topo-extract] summary unpaired=N` drops by 2 (the DEGEN
  entries `(31,169)` + `(197,200)` filtered at M2; canary §2 + §5
  edge case #3).
- F0020 [twin-oracle] `unpaired_count <= 2` (non-regression bound;
  the 2 residual edges are a separate downstream layer banked PR-Y23+
  per adversary-22 §6 + R-b amendment).
- F0044 batch [twin-oracle] `unpaired_count <= 0` (non-regression
  bound; already at target on plain HEAD per adversary-22 §8).
- F0030 stays clean (no Mode A; canary §2).
- F0050 stays Failed at the same Euler/normal defect class (different
  defect; canary §2 confirms 0 MISSING).
- 1250+ existing kernel tests: NO regression.
- `cargo clippy -p kernel`: NO new warnings.
- `cargo fmt --check`: clean.

**Informational (non-gating):**
- F0020 spotlight Status:Failed MAY persist (downstream tessellation
  NMM-render layer banked from PR-Y21 ABORT). Per `feedback_yang_only.md`
  no-movement at status level → next-layer outcome.
- F0051 likely improves (canary §5 banked it; adversary-22 must verify
  via spot-test or extension of `spotlight_f0050`).
- Yang fast subset 10/157 → ≥11 if F0051 returns; ≥12 if cohort siblings
  cross threshold.

## §7 NO fallback paths (per `feedback_yang_only.md`)

- **M1 NMM-classification scope guard:** the extended NMM predicate
  fires only when `undirected_count != 2` (Yang §4.4.2 manifold
  definition). Manifold edges (`undirected_count == 2`) MUST continue
  to flow through the existing `[the_one]` pairing arm at Step 7
  L1232 — M1 must never silence a true-manifold edge by mis-classifying
  it as NMM. If a regression surfaces showing `undirected_count == 2`
  edges falling into the `[]` arm with `is_nmm == true`, that is a
  CONTRACT VIOLATION between the predicate and the pairing logic; debug
  the upstream emission, do NOT widen the predicate.
- **M2 filter overshoot:** the filter `cv[0]==cv[1] || ...` rejects
  exactly the canon-degenerate tris. Filter overshoot is structurally
  impossible: `cv` has duplicate iff `raw` is non-degenerate-but-
  quantization-collapses (the only case we WANT to filter) OR `raw` is
  itself degenerate (which canary §4 confirms doesn't happen at our
  upstream). If a non-degenerate raw tri with non-duplicate cv is
  rejected, that's a logic error in the filter itself — no overshoot
  recovery; debug the filter.
- No new tolerance widening, no special-case branches.

## §8 Anti-scope (explicit OUT)

- **Upstream `subdivide_mesh_pair` degenerate-tri origin investigation
  — REMOVED FROM SCOPE.** Canary §4 verified Cherchi output is clean
  across 14 booleans; this concern is closed (NOT banked PR-Y23+).
- **PR-Y20-MODE-A NMM `Option<HalfEdgeIdx>` type system** — already
  shipped; M1 must NOT extend NMM semantics to TRUE-manifold edges
  (those with `undirected_count == 2` per Yang §4.4.2). Per R-a
  amendment: M1 ONLY extends NMM coverage to additional NMM cases
  (`undirected_count != 2`); the load-bearing distinction is between
  manifold (incidence == 2) and non-manifold (incidence != 2), as
  defined by the paper. The original "manifold-mis-segmented" framing
  was a misnomer — R-0 §1+§2 confirmed the 7 NONCONF F0020 cases have
  `n_total = 3` and are genuinely NMM, NOT mis-segmented manifold.
- **Step 5a re-partition for NONCONF** — REJECTED per canary §5:
  classification refinement at Step 7 L1270-1291 (M1 NMM extension) is
  a lighter touch and is paper-faithful per Yang §4.4.2 (3-incident
  edges are NMM; the patch decomposition is correct).
- **Path A retroactive emit at Step 6 L862** — REJECTED post-R-a
  (originally PREFERRED in pre-R-a §4): R-0 §4 empirical finding
  (`undirected_count = 3` for all 7 cases) refutes the manifold-
  symmetric-emission premise. Emitting both directions of a 3-incident
  edge would force an arbitrary 2-of-3 HE selection violating Yang's
  1:1 manifold pairing mandate.
- **Path B retroactive HE allocation at Step 7 L1253** — REJECTED
  pre-R-a (arena post-hoc growth + missing topological pointers); also
  refuted post-R-a since there is no manifold pair to allocate.
- **F0050 normals + Euler** — different defect class; PR-Y23+ candidate.
- **F0020 downstream tessellation NMM-rendering** — PR-Y21 ABORTED;
  banked when right anchor identified.
- **PR-Y17-COPLANAR L264 panic, PR-Y19-MODE-B R3 routing, PR-Y20-MODE-A
  NMM type** — all stay in force; PR-Y22 builds on them.
- **5 L264 panic cases (R0014/R0046/R0055/R0081/F0075)** — different
  mechanism; PR-Y18 territory.
- **F0086, F0031-F0040, R0020/R0021, R0071** — out of scope.
- **Reference C++ sidecar comparison for PR-Y22** — internal correctness
  verifiable via `[twin-oracle]` invariants I1-I3 + Yang §4.4.2 paper
  audit. Sidecar parity is for cases where Cherchi output divergence is
  suspected; canary §4 verified Cherchi output IS correct here.
- Fillet/chamfer/shell (DEFERRED INDEFINITELY per CLAUDE.md).

## §9 FIP role table

| Sub-phase | Agent | Reads required | Writes |
|---|---|---|---|
| 0a Canary | canary-runner-9 | F0020 case + L862 + canon_v + face_survival_detect + Yang §4.4.2 + Cherchi §4 | empirical NONCONF/DEGEN probe + canary memo (DONE) |
| 0b Spec | spec-writer-v | canary memo + Yang §4.4.2 + Cherchi §4 + L468-474 + L862 + L1253 | this spec |
| 0c Tests | test-author-m | this spec + existing spotlight pattern | RED regression test asserting I1+I2+I3 |
| 0d Implement | implementer-z | spec + tests + canary §5 + L468-474 + L862 (NOT face_survival_detect) | M2 filter at L468-474 + M1 Path A at L862-866 |
| 0e Adversary | adversary-22 | all 0a-0d + cohort tests + paper audit + F0051 verification | independent runs + corpus sweep + F0051 status + verdict memo |
| 0f Close-out | team-lead | all 0a-0e | clippy/fmt + WASM rebuild + memory updates + commit |

Per `feedback_oracle_credibility_via_role_separation.md`: NO agent
performs more than one sub-phase. Spec-writer-v (this agent) does not
implement, test, or adversary. Spec-writer-v's role ENDS at writing
this document.
