# PR-Y25 Anchor Canary — Option H1 NMM-aware retessellation REFUTED: F0020 b#2 has zero NMM-pairs in construction-time directed_he

**Author:** canary-y25
**Date:** 2026-05-08
**Plan:** `/home/claude/.claude/plans/optimized-wandering-wind.md` Phase 0 canary
**Verdict:** **REFUTED** — H1's mechanism (NMM-pair render-mesh vertex sharing keyed on `arena.constructed_directed_edge`) does not apply on F0020 b#2. The probe found `total_nmm_pairs=0` across every invocation in both F0020 and F0044 spotlights. All 39 NMM HEs in F0020 b#2 are directionally asymmetric in the construction-time `directed_he` map (`keys_with_reverse_anywhere=0`); all 44 NMM HEs in F0044 batch invocation #4 likewise. Stronger than acceptance-gate "0 pairs": the §D probe confirms all 39 NMM HEs of F0020 b#2 sit at *paired* render-mesh edges (count=2), so the 36 unpaired render-mesh edges are not at NMM-HE positions at all.
**Recommended scope:** **ABORT** PR-Y25 Option H1 as currently scoped. The 36-unpaired residual on F0020 has a different mechanism than diagnosis A in plan §"Phase 1 findings". Option (i) Yang §4.4.1 mesh-updating (banked PR-Y26) and/or a non-NMM-anchored mechanism are the next candidates; spec phase should not write H1 around an empirically vacuous antecedent.

This memo names the empirical evidence only. It does NOT propose code shape; per `feedback_anchor_before_fix.md`, scope decision is upstream.

---

## §0 Discipline — live tree untouched

### Live tree at session start

```
$ git status
On branch main
Your branch is up to date with 'origin/main'.

nothing to commit, working tree clean
```

### Live tree just before writing this memo

```
$ git status
On branch main
Your branch is up to date with 'origin/main'.

nothing to commit, working tree clean
```

All probe instrumentation was applied inside a separate worktree:

```
$ git worktree add /tmp/y25-probe-wt 5993954
Preparing worktree (detached HEAD 5993954)
HEAD is now at 5993954 build(wasm): rebuild WASM bundle for PR-Y24 (8b8297c)

$ cd /tmp/y25-probe-wt && git diff --stat
 app/tests/cases/assay/results.json    |   8 +-
 crates/kernel/src/tessellation/mod.rs | 312 ++++++++++++++++++++++++++++++++++
 2 files changed, 316 insertions(+), 4 deletions(-)
```

No `git stash`, `git checkout --`, `git reset --hard`, or any other destructive op was used on the live working tree. Per `feedback_adversary_no_destructive_git.md`. (`results.json` mutation is the assay test runner's normal artifact; it does not reflect probe-instrumentation logic and will be discarded with `git worktree remove`.)

### Probe gate

Every probe is gated on `std::env::var("Y25_PROBE").as_deref() == Ok("1")` — when unset the codepath is byte-identical to the `5993954` baseline. Probe blocks scoped inside `if y25_probe { ... }`. Output is `eprintln!`-only. No mutation of `disc.positions`, `vertices`, `indices`, or arena state.

### Reproduction commands

```
git worktree add /tmp/y25-probe-wt 5993954
cd /tmp/y25-probe-wt
# (probes injected into tessellate_solid_bounded per §3 below)

# F0020 b#2 (load-bearing prediction)
YANG_BOOLEAN=1 Y25_PROBE=1 TWIN_DEBUG=1 cargo test -p test-harness \
    --test assay_randomized -- spotlight_f0020 \
    --ignored --nocapture --test-threads=1 \
    > /tmp/y25-canary-f0020.txt 2>&1

# F0044 batch (cohort guard — also F0045 + R0092)
YANG_BOOLEAN=1 Y25_PROBE=1 TWIN_DEBUG=1 cargo test -p test-harness \
    --test assay_randomized -- spotlight_f0044 \
    --ignored --nocapture --test-threads=1 \
    > /tmp/y25-canary-f0044.txt 2>&1

cd /home/claude/workspace
git worktree remove /tmp/y25-probe-wt
```

Both tests reported `test result: ok. 1 passed; 0 failed`. Inner spotlight verdicts: F0020 `Status: Failed` (`watertight_mesh: 36 unpaired edges out of 130 total`), F0044 batch as expected at PR-Y22 baseline.

---

## §1 F0020 spotlight — mechanism REFUTED on load-bearing invocation

The F0020 spotlight produces SIX `tessellate_solid_bounded` invocations across the three sequential extrudes (each extrude tessellates pre-boolean operands at LOD=Boolean and the post-boolean result at LOD=Render, plus the validator's render-LOD repeat). The load-bearing one is the **final invocation**: 169 half-edges, 130 render-mesh edges, 36 unpaired — exactly matching the spotlight oracle output.

### Verbatim grep on F0020 final invocation (load-bearing)

```
[yang-diag] flood_fill_patches: 39 unpaired HEs out of 169 total
[y25-probe-p1-diag] nmm_he_total=39 nmm_he_with_construct=39 nmm_directed_keys=39 nmm_directed_total_bucket_size=39 keys_with_reverse_anywhere=0
[y25-probe-p1] total_nmm_pairs=0 ambiguous_pairs=0
[y25-probe-p2] actual_unpaired=36 actual_total=130 simulated_watertight_unpaired=36 simulated_total=130 snap_targets=0 key_remap_size=0
[y25-probe-p2-d] nmm_he_at_unpaired_render_edge=0 nmm_he_at_paired_render_edge=39 nmm_he_no_edge_verts=0
```

### What the lines mean

| Line | Reading |
|---|---|
| `[yang-diag] flood_fill_patches: 39 unpaired HEs out of 169 total` | Topology has 39 NMM HEs (twin=None) — Cherchi 2022 §3 / Yang 2025 §4.4.2 directional-symmetry; baseline-correct per PR-Y20-MODE-A |
| `nmm_he_total=39 nmm_he_with_construct=39` | All 39 NMM HEs have a construction-time directed-edge entry in `arena.constructed_directed_edge` — PR-Y24's plumbing covers the population |
| `nmm_directed_keys=39 nmm_directed_total_bucket_size=39` | 39 distinct (u,v) keys, each with bucket size 1; no construction-time multi-HE-per-direction exists |
| **`keys_with_reverse_anywhere=0`** | **No (u,v) key has its reverse (v,u) anywhere in the construction-time map.** All 39 NMM HEs are directionally asymmetric in the construction-time view |
| **`total_nmm_pairs=0 ambiguous_pairs=0`** | **Zero NMM-pairs detected.** No (u,v)+(v,u) co-existence; no pairing decision to make |
| `actual_unpaired=36 simulated_watertight_unpaired=36` | Position-key edge-counter on actual mesh: 36 unpaired (matches spotlight oracle). Simulated (after applying NMM-pair-snap remap): identical 36 — because `snap_targets=0` and `key_remap_size=0` |
| **`nmm_he_at_unpaired_render_edge=0 nmm_he_at_paired_render_edge=39`** | **All 39 NMM HEs sit at *paired* render-mesh edges (count=2).** None of them sit at the 36 unpaired render-mesh edges |

### What this empirically says

H1's diagnosis premise (plan lines 17-18, "the render-mesh has 36 unpaired edges; ~30 of them trace to those NMM HEs flowing through `tessellate_solid_bounded` without shared vertex IDs across NMM-pair seams") is contradicted by §D:
1. There are no NMM-pair seams in the construction-time map.
2. All 39 NMM HEs are at paired render-mesh edges, not unpaired ones.

The 36 unpaired render-mesh edges originate from a *different* mechanism — somewhere in the planar/cylindrical face tessellator's output where face-interior triangulations don't share boundary vertex positions across faces. Diagnosis A in plan §"Phase 1 findings" — "NMM-pair HEs (twin=None) on the same intersection-curve seam emit per-face distinct render-mesh vertex IDs" — has empirical antecedent count zero on F0020 b#2.

### Acceptance-gate verdict (plan §"Acceptance gate")

| F0020 P1 outcome | Verdict (per plan) |
|---|---|
| `~14-19 NMM pairs found` | Expected — proceed |
| `<10 NMM pairs` | Mechanism may not match; investigate |
| **`0 NMM pairs`** | **REFUTED — no pairs to share** |

Observed: **0 NMM pairs.** **REFUTED.**

| F0020 P2 outcome | Verdict (per plan) |
|---|---|
| `simulated ≤ 8` | CONFIRMED — full mechanism |
| `8 < simulated ≤ 20` | PARTIAL |
| **`simulated > 20`** | **REFUTED — H1 insufficient** |

Observed: **simulated=36** (= actual). **REFUTED.**

Both gates fire REFUTED. The §D supplementary probe further reinforces: the unpaired render-mesh edges are not at NMM-HE positions at all (`nmm_he_at_unpaired_render_edge=0`), so any NMM-anchored fix is structurally incapable of dropping the count.

---

## §2 F0044 cohort batch — H1 a structural no-op

The F0044 spotlight runs the F0044+F0045+R0092 batch, producing 7 `flood_fill_patches` invocations and a corresponding number of `tessellate_solid_bounded` invocations (3 are LOD=Render; the other 4 are intermediate LOD=Boolean tessellations of operands not exercising the final-render path). Verbatim grep over the 3 LOD=Render invocations:

```
[y25-probe-p1-diag] nmm_he_total=0 nmm_he_with_construct=0 nmm_directed_keys=0 nmm_directed_total_bucket_size=0 keys_with_reverse_anywhere=0
[y25-probe-p1] total_nmm_pairs=0 ambiguous_pairs=0
[y25-probe-p2] actual_unpaired=12 actual_total=180 simulated_watertight_unpaired=12 simulated_total=180 snap_targets=0 key_remap_size=0
[y25-probe-p2-d] nmm_he_at_unpaired_render_edge=0 nmm_he_at_paired_render_edge=0 nmm_he_no_edge_verts=0

[y25-probe-p1-diag] nmm_he_total=0 nmm_he_with_construct=0 nmm_directed_keys=0 nmm_directed_total_bucket_size=0 keys_with_reverse_anywhere=0
[y25-probe-p1] total_nmm_pairs=0 ambiguous_pairs=0
[y25-probe-p2] actual_unpaired=38 actual_total=472 simulated_watertight_unpaired=38 simulated_total=472 snap_targets=0 key_remap_size=0
[y25-probe-p2-d] nmm_he_at_unpaired_render_edge=0 nmm_he_at_paired_render_edge=0 nmm_he_no_edge_verts=0

[y25-probe-p1-diag] nmm_he_total=44 nmm_he_with_construct=44 nmm_directed_keys=44 nmm_directed_total_bucket_size=44 keys_with_reverse_anywhere=0
[y25-probe-p1] total_nmm_pairs=0 ambiguous_pairs=0
[y25-probe-p2] actual_unpaired=43 actual_total=281 simulated_watertight_unpaired=43 simulated_total=281 snap_targets=0 key_remap_size=0
[y25-probe-p2-d] nmm_he_at_unpaired_render_edge=0 nmm_he_at_paired_render_edge=44 nmm_he_no_edge_verts=0
```

### Per-invocation table

| Inv | total HEs | NMM HEs | NMM with construct | Keys w/ reverse | NMM-pairs | Render unpaired | Sim unpaired | NMM HEs at unpaired render | NMM HEs at paired render |
|---|---|---|---|---|---|---|---|---|---|
| 1 | 180 (edges) | 0 | 0 | 0 | 0 | 12 | 12 | 0 | 0 |
| 2 | 472 (edges) | 0 | 0 | 0 | 0 | 38 | 38 | 0 | 0 |
| 3 | 281 (edges) | 44 | 44 | 0 | 0 | 43 | 43 | 0 | 44 |

### Cohort guard verdict

In every invocation: `total_nmm_pairs=0`. **H1 is a structural no-op on F0044 batch** — its mechanism cannot fire because the antecedent (NMM-pair existence) is empty. This is the cohort guard the plan asked for; the result is consistent with H1 being safe-but-vacuous for F0044, *but* this safety derives from the same emptiness that REFUTES H1 on F0020.

Note: the F0044 batch is also failing watertight (12, 38, 43 unpaired edges respectively). This was not a baseline-passing cohort under the watertight oracle. The "cohort guard" structure in plan §"Acceptance gate" was framed around H1's *non-regression* on F0044; with H1 vacuous, both pass-mode (no NMM pairs to break) and fail-mode (the existing watertight failures remain unchanged) are inert. Plan gate 5/6 (`[topo-extract] unpaired=0` and `[twin-oracle] unpaired_count=0`) are topology-layer metrics; both 0 across F0044 batch (PR-Y22+Y24 contracts). Render-mesh watertight failures in F0044 are pre-existing and **not** gated by H1.

### Invocation 3's 44 NMM HEs — same pattern as F0020

The third F0044 batch LOD=Render invocation has 44 NMM HEs all populated in `constructed_directed_edge` with `keys_with_reverse_anywhere=0` — same directional-asymmetry pattern as F0020 b#2, just at higher cardinality. All 44 sit at *paired* render-mesh edges (count=2); the 43 unpaired render-mesh edges are at non-NMM-HE positions. Cross-confirms F0020's mechanism finding: the watertight failure is **not** an NMM-pair-vertex-sharing problem on this corpus.

---

## §3 Probe diff (worktree only — NOT committed)

Located at `/tmp/y25-probe-wt/crates/kernel/src/tessellation/mod.rs`. Three blocks inserted in `tessellate_solid_bounded`: P1 immediately after `discretize_edges` (L~4189), P2 just before `Ok(RenderMesh{...})` (L~4422). All gated on `Y25_PROBE=1` env var.

```
$ cd /tmp/y25-probe-wt && git diff --stat crates/kernel/src/tessellation/mod.rs
 crates/kernel/src/tessellation/mod.rs | 312 ++++++++++++++++++++++++++++++++++
 1 file changed, 312 insertions(+)
```

### Probe P1 (NMM-pair detection + diagnostic) — inserted after L4189

Walks `arena.constructed_directed_edge`, groups twin=None HEs by their (u,v) construction-time key, and counts pairs where exactly one HE in (u,v) has a counterpart in (v,u). Emits per-pair lines plus a summary count. The §A diagnostic supplements with NMM-HE population rates and `keys_with_reverse_anywhere` to distinguish "no construction-time entry" from "no reverse exists".

```rust
let y25_probe = std::env::var("Y25_PROBE").as_deref() == Ok("1");
let mut nmm_pair_endpoints: Vec<((usize, usize), (usize, usize))> = Vec::new();
if y25_probe {
    use std::collections::BTreeMap;
    let mut by_dir: BTreeMap<(usize, usize), Vec<usize>> = BTreeMap::new();
    for (i, he) in arena.half_edges.iter().enumerate() {
        if he.twin.is_some() { continue; }
        if let Some((u, v)) = arena.constructed_directed_edge.get(i).copied().flatten() {
            by_dir.entry((u.0, v.0)).or_default().push(i);
        }
    }
    // §A diagnostic counts (NMM HE total, NMM with construct, ...)
    // ...
    let mut pair_count = 0usize;
    for ((u, v), fwd_hes) in by_dir.iter() {
        if u >= v { continue; }
        if let Some(rev_hes) = by_dir.get(&(*v, *u)) {
            if fwd_hes.len() == 1 && rev_hes.len() == 1 {
                pair_count += 1;
                // emit per-pair info, capture endpoints into nmm_pair_endpoints
            }
        }
    }
}
```

### Probe P2 (predicted-outcome simulation) — inserted before L4422 `Ok(RenderMesh{...})`

For each NMM-pair captured by P1, snap the two sides' u-endpoint quantized positions to their midpoint key, and v-endpoint to its midpoint key. Apply the resulting `key_remap` (with transitive-closure resolution) to render-mesh vertex quantized positions, then run the same position-keyed edge-count edge-pair check that `check_watertight_mesh` uses. With `nmm_pair_endpoints` empty (the F0020/F0044 case), `key_remap_size=0` and `simulated == actual` by construction.

```rust
if y25_probe {
    // §A: actual unpaired count via TAU_TESS_GRID_FACTOR/TAU_TESS_GRID_MIN-keyed
    //     edge counter, mirroring oracle.rs:185-264
    // §B: build key_remap from nmm_pair_endpoints (NMM-pair midpoint snap)
    // §C: simulated unpaired count via remapped keys
    // §D: where do NMM HEs sit on the render mesh? At paired or unpaired
    //     position-edges? Distinguishes "NMM HEs cause the unpaired-ness" from
    //     "NMM HEs are paired; unpaired-ness is elsewhere"
}
```

The probe scaffolding will be discarded by `git worktree remove /tmp/y25-probe-wt` at close-out. Per plan §"Phase 0 canary": probes live ONLY in the worktree.

---

## §4 Hypothesis verdict — REFUTED

| Probe | Prediction (plan §"Phase 0 canary") | Observation | Verdict |
|---|---|---|---|
| P1 F0020 b#2 NMM-pair count | `~14-19 NMM pairs found` | **0 pairs** | ❌ REFUTED |
| P1 F0020 b#2 reverse-key existence (added §A diag) | (n/a; assumed nonzero) | `keys_with_reverse_anywhere=0` — no reverses anywhere | ❌ REFUTED at premise |
| P1 F0044 batch NMM-pair count | `nmm_pair_count == 0` for all 7 invocations | 0 pairs in all 3 LOD=Render invocations | ✅ MET (but H1 vacuous, see §2) |
| P2 F0020 b#2 simulated unpaired | `simulated ≤ 8` (CONFIRMED branch) | `simulated=36 (vs actual=36)` | ❌ REFUTED |
| §D F0020 b#2 NMM HEs at unpaired render edges | (added; would have informed) | `nmm_he_at_unpaired_render_edge=0`, `nmm_he_at_paired_render_edge=39` | Bonus refutation: unpaired-ness is not co-located with NMM HEs |

**Acceptance gate (plan lines 87-91):** "F0020: simulated > 20 → ABORT (single-layer Option H1 insufficient; need Option (i))." Observed: simulated = 36 > 20. **ABORT recommendation triggered.**

The §D probe additionally rules out a softer "NMM HEs are at the unpaired edges but not via HE↔HE pair-share" formulation: the unpaired render-mesh edges are not even at NMM-HE positions, so no NMM-anchored mechanism (pair-share, share-with-self, position-snap-of-NMM-endpoints) can drop the count.

---

## §5 Recommended scope — ABORT

### Why H1 is structurally inapplicable on F0020

1. The 39 NMM HEs in F0020 b#2 are all directionally asymmetric in `directed_he` (Yang §4.4.2 / Cherchi §3 — closed-loop intersection curves in this corpus are traversed once per side, not as anti-parallel HE pairs). H1's `BTreeMap<HalfEdgeIdx, HalfEdgeIdx>` pair-map would be empty.
2. Even granting H1 could be reformulated to act on single-direction NMM HEs (e.g., position-snapping origin/dest endpoints to neighboring face boundary positions), the §D probe shows the 39 NMM HEs are already at paired (count=2) render-mesh edges. There is no positional gap at NMM HE locations to close.
3. The 36 unpaired render-mesh edges are at non-NMM-HE positions. H1's mechanism, no matter how generously reframed, does not touch those positions.

### Where the watertight defect actually lives (banked for next anchor probe)

§D's `nmm_he_at_paired_render_edge=39` together with `actual_unpaired=36 actual_total=130` says:
- Of 130 quantized-position edges in the F0020 b#2 render mesh, 36 are non-paired (count != 2).
- All 39 NMM-HE-located edges are paired (count = 2).
- Therefore the 36 unpaired edges are at positions that are **face-interior** to one of the source faces, not on inter-face boundaries — likely:
  - **(a)** earcut/Steiner-fan diagonal mismatches across faces sharing an interior position (T-junctions from per-face triangulations of curved-face cylindrical strips, given F0020's oblique-extrude trapezoidal sides may produce degenerate slivers); or
  - **(b)** Yang §4.4.1 mesh-updating gap (banked PR-Y26 in plan §"Phase 1 findings") — the source-mesh A↔B vertex non-unification at refined-intersection seams produces face-interior unpaired quads when adjacent solid-A and solid-B faces tessellate independently along a shared intersection curve.

The plan's existing assignment of "B": "Yang §4.4.1 mesh-updating is also INCOMPLETE (vertex unification between source-meshes A+B at the seam); contributes to ~6 residual unpaired + the self-intersections + the χ deficit. Architectural follow-up; banked PR-Y26." This canary's evidence suggests B is dominant, **not** a residual — the entirety of the 36-unpaired count plausibly lives there, with 0 contribution from H1's diagnosis A. The plan's "~30 of them trace to those NMM HEs flowing through tessellate_solid_bounded" was an unverified estimate; this canary refutes it to "0 of them."

### Routing recommendation

- **Do NOT proceed to spec-y25 with H1 as currently scoped.** Spec writing on an empirically vacuous antecedent is a `feedback_validate_against_corpus.md` failure mode (unit-test-green / paper-citation-coherent ≠ corpus-effective).
- **Re-anchor probe candidate:** PR-Y25-vNext should canary the alternate hypothesis "what render-mesh structure produces the 36 unpaired edges". The §D supplementary probe (added to this canary) is the seed; extending it to enumerate the 36 unpaired position-keys, classify each as (face-interior-singleton vs. cross-face-3+), and locate them relative to face-rangess would be the next anchor probe. That probe should run FIRST, then any code shape decision.
- **PR-Y26 (Yang §4.4.1 mesh-updating) candidacy strengthened.** If the next canary confirms (b), the next PR's anchor is Yang §4.4.1 vertex unification at SSI refinement, with the explicit understanding that PR-Y25's "blast radius" concern (banked) was the right scope decision — but that the "smaller surgical change" PR-Y25 attempted (H1) is empirically not a smaller fix; it's a vacuous one.

---

## §6 Banked findings — observations not load-bearing for the verdict

1. **PR-Y24's `constructed_directed_edge` plumbing IS populated correctly on F0020 b#2.** All 39 NMM HEs have an entry (`nmm_he_with_construct=39 == nmm_he_total=39`). PR-Y24's contract holds; this canary's REFUTED verdict is not a PR-Y24 regression. PR-Y24's [twin-oracle] unpaired count remains 0 (verified earlier in /tmp/y25-canary-f0020.txt, not shown — same value as PR-Y24 commit).

2. **F0020 has six `tessellate_solid_bounded` invocations across three sequential extrudes.** Five of them have `nmm_he_total=0` (no NMM HEs — clean booleans or operand-only LOD passes); only the final invocation (Extrude 3 final render) has 39. This concentrates the load-bearing analysis on a single invocation.

3. **F0044 batch invocation 3** has 44 NMM HEs in the same directional-asymmetry pattern as F0020 b#2 — `keys_with_reverse_anywhere=0`. This is structurally consistent across the corpus and suggests Yang §4.4.2 directional-symmetry is a *robust* property of construction-time `directed_he`, not an F0020 oddity. Any future fix that depends on construction-time NMM-pair existence should expect this empirical zero rate.

4. **§D probe innovation: NMM HE position-co-location with render-mesh unpaired edges.** This was not originally in the plan's probe list; I added it after P1's surprise-zero result. The §D check is the strongest-form refutation: even if NMM-pair antecedent were generously stretched (e.g., "treat single-direction NMM HEs as their-own pair via reflection at a face plane"), `nmm_he_at_unpaired_render_edge=0` rules out *any* NMM-anchored mechanism. Worth banking as a reusable probe template for future canaries that distinguish "is the defect AT the named site or elsewhere" — the same shape catches Layer-routing errors PR-Y23 fell into.

5. **The 36 unpaired edges are at face-interior positions, not at NMM-HE seams.** This is the key positive finding for routing: the next anchor probe should focus on intra-face (per-face triangulator output) and inter-face cross-source-mesh (Yang §4.4.1) vertex-sharing, NOT on the NMM-vs-non-NMM HE distinction.

6. **Probe overhead bounded.** P1 + P2 + §D each iterate `arena.half_edges` at most twice and do one full pass over `indices`. Total probe output for F0020: 3 lines × 6 invocations = 18 `[y25-probe-*]` lines. For F0044 batch: 3 × ≈3 LOD=Render invocations = ~9 lines. Default-off codepath is byte-identical to baseline (verified by structure: every probe block is gated on `if y25_probe { ... }`).

---

## §7 Final-report block

### Probe diff (worktree only — NOT committed)

```
$ cd /tmp/y25-probe-wt && git diff --stat
 app/tests/cases/assay/results.json    |   8 +-
 crates/kernel/src/tessellation/mod.rs | 312 ++++++++++++++++++++++++++++++++++
 2 files changed, 316 insertions(+), 4 deletions(-)
```

### Live tree status at memo write

```
$ cd /home/claude/workspace && git status
On branch main
Your branch is up to date with 'origin/main'.

nothing to commit, working tree clean
```

Identical to start-of-session status. No live-tree mutation occurred during canary work. The memo at `docs/audits/pr_y25_anchor_canary.md` is the only live-tree change in this canary phase.

### Probe output artifacts

- `/tmp/y25-canary-f0020.txt` — F0020 spotlight log, retained for follow-on phases.
- `/tmp/y25-canary-f0044.txt` — F0044+F0045+R0092 batch log, retained.
- `/tmp/y25-probe-wt/` — probe worktree, retained until close-out (`lead-y25` removes via `git worktree remove`).

---

## §8 Routing

- **Hypothesis tested:** Option H1 (NMM-pair render-mesh vertex sharing keyed on `arena.constructed_directed_edge`).
- **Verdict:** **REFUTED.** F0020 b#2 has 0 NMM-pairs (vs ~14-19 predicted); simulated unpaired = actual unpaired = 36 (vs ≤ 8 in CONFIRMED branch); NMM HEs sit at paired render-mesh edges, not unpaired ones.
- **Recommended scope:** **ABORT** PR-Y25 as currently shaped. Spec phase should NOT write H1 around an empirically vacuous antecedent.
- **Cohort guard:** F0044+F0045+R0092 batch: 0 NMM-pairs across all invocations (H1 vacuous → no regression risk; but this is consistency-with-refutation, not confirmation).
- **Canary anchor sites for next iteration:**
  - For "where the 36 unpaired actually originate" probe: same site (`tessellation/mod.rs:4183-4427`); enumerate the 36 unpaired position-edges, label each by face-id range membership and origin-face-vs-cross-face status. Seed: §D probe in this canary.
  - For Yang §4.4.1 (mesh-updating) anchor candidacy: pre-tessellation step in `boolean/yang_integration.rs` where the source-mesh-A+B → result-mesh transition happens; needs a separate canary brief.
- **Next agent:** team-lead (do NOT spawn spec-y25 until team-lead confirms the ABORT and decides re-anchor strategy).
- **Citations remain valid for any successor PR:**
  - **Yang 2025 §4.4.3** (verbatim, `refs/text/yang2025_hybrid_boolean.txt:599-605`): "watertightness of our result is inherited from the mesh Boolean output, ensuring the mesh has no geometric gaps." This is the contract F0020 violates with `unpaired_count=36`. The contract is layer-agnostic; its violator on F0020 is now empirically known to be NOT NMM-pair seams.
  - **Cherchi 2022 §3** (verbatim, `refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:251-254`): "the arrangement is guaranteed to be a well formed simplicial complex and surface patches are bounded by closed loops of non-manifold edges, namely the intersection lines." F0020 satisfies this at the topology layer (39 NMM HEs forming closed loops along intersection lines, all directionally-asymmetric per Yang §4.4.2). The render-mesh-watertight gap lives downstream of arrangement, in the per-face triangulation / source-mesh unification step, not in the arrangement / NMM topology itself.

This memo names the empirical mechanism only. Per `feedback_anchor_before_fix.md`: canary names WHERE; team-lead/spec/impl decide WHAT — and in this case, decide whether to ABORT and re-anchor, or to prosecute Option (i) / PR-Y26 directly.
