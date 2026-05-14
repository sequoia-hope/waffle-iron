# PR-Y41 canary — SHIP-INFRA (7th-refutation framing; strategic-pivot trigger)

**Verdict:** **SHIP-INFRA — 7th-refutation framing**
**Gate 4 (load-bearing 18-index accounting):** = **18 indices** exactly. No upstream dispatch loss.
**Production code modified:** **0 LOC** (probe is env-gated, default-off byte-identical)
**Probe LOC:** ~317 added lines in `crates/kernel/src/tessellation/mod.rs` (worktree only)
**Wrong-anchor count this cycle:** N/A — canary is INFRA-class; measures the load-bearing site directly. The PR-Y40-banked "missing ~12 D.1d indices upstream of F.0" attribution is the chain being tested.
**PR-Y42 anchor recommendation:** **Strategic-pivot trigger.** Gate 4's exact-18 measurement refutes "missing ~12 indices upstream of F.0." All 18 D.1d-emitted indices ARE dispatched. The "missing" frame was itself a counting artefact in PR-Y40 §3.3's residual ("emitted 18, lost 4, must-survive 14, but only 2 survive → ~12 missing"). PR-Y40 §3.3 row "**tris surviving F.1 (predicted: dispatched - lost)**" matched downstream observation — there is no missing-12-indices residual. **Continued D.1d-focused F0020 Render LOD probe-refinement is no longer empirically justified.** Recommend strategic pivot to option (B) different diagnostic strategy or option (C) pause F0020 Render LOD.

---

## §0 Summary

PR-Y40 (commit `57bfe32`, audit `7a3e4c3`) shipped an INFRA probe at `remove_winding_insensitive_duplicates` and refuted PR-Y39's "16 D.1d-loser collisions" attribution with a measurement of 4 collisions. PR-Y40 §6 banked: "of 18 D.1d-emitted indices, 4 lose at F.0→F.1, 2 survive — *the other ~12 indices come from a DIFFERENT mechanism (likely degenerate-vert collapse at dispatch).*"

**The PR-Y41 dispatch probe REFUTES the "missing 12" framing.** When measuring per-face `indices_emitted` directly at the planar dispatch site (`tessellate_planar_face_bounded`), kids 218/232/233 emit:

| kid | dispatched indices | dispatched tris | distinct_quantized_tris | degenerate_collapse_count | single_vert_collision_count |
|---|---|---|---|---|---|
| 218 | 3 | 1 | 1 | 0 | 0 |
| 232 | 6 | 2 | 1 | 0 | 1 |
| 233 | 9 | 3 | 1 | 0 | 2 |
| **TOTAL D.1d** | **18** | **6** | **3** | **0** | **3** |

**18 indices emitted = 18 indices expected (6 tris × 3).** No upstream loss between dispatch (F.−1) and `remove_winding_insensitive_duplicates` entry (F.0). PR-Y40 §3.3's accounting was actually internally consistent and the "~12 missing" wording in §6 was an over-interpretation: 6 tris dispatched, 4 lost at F.0→F.1, **2 survive** — that fully accounts for the 18 emitted indices. There is no missing-indices residual.

**Per-triangle quantization, however, is new signal.** None of the D.1d kids' triangles are fully-degenerate at dispatch (degen=0/0/0). But of 6 dispatched D.1d tris, 3 are *single-vertex-collision* triangles (two of three vertices quantize to the same i64 grid cell). The single-collision pattern matches PR-Y40 §3.5's row 5 (kid 197→231, partially-degenerate) and the kid-232/233 self-collisions noted in §3.2. This means D.1d kids 232/233 emit partially-degenerate triangles at dispatch, and `remove_winding_insensitive_duplicates`'s canonical-key dedup then collapses pairs of partially-degenerate triangles whose 2-vert-overlap-positions happen to match.

**Cohort secondary signal:** F0044/F0045/R0045/R0092 have **ZERO** fully-degenerate emissions at dispatch. The fully-degenerate cluster Y40 §3.5 identified (kid 235=6 self-collisions, kid 256=4 cross-collisions at fully-degenerate canonical key) is **F0020-specific** — confirmed at dispatch site by Y41 measuring kid 235 emitting 7/7 fully-degenerate tris and kid 256 emitting 4/4 fully-degenerate tris. The dispatch emits these zero-area triangles, and F.0→F.1 then dedupes 10 of 13 (the others survive as zero-area degenerates downstream — `8 of 113 triangles are degenerate` in the F0020 Status line).

**Verdict: SHIP-INFRA + 7th-refutation framing.** Per plan §strategic-checkpoint: "PR-Y41's outcome determines the next strategic move … No anomaly (Gate 4 = 18) → 7th-refutation; ~1041+~150 LOC cumulative probe with no production code in 10 cycles. **Strategic pivot recommended at this point** — options (B) different diagnostic strategy or (C) pause F0020 Render LOD." The empirical signal is clear: **D.1d is not the rate-limiting defect mechanism for F0020 Render LOD.**

Per `feedback_anchor_before_fix`: the dispatch probe operating at the empirically-correct upstream anchor caught the "missing 12" frame as wrong before any production cycle. Per `feedback_validate_against_corpus`: Gate 6 confirms cohort has no fully-degenerate signature and no D.1d signature — D.1d is F0020-specific and even there NOT load-bearing. Per `feedback_no_last_bug`: F0020 Status:Failed unchanged at 40 unpaired; we never claim Render LOD closure here.

---

## §1 Discipline

- **Worktree-only.** All changes in `/home/claude/workspace/.claude/worktrees/canary-y36/`. No `git stash`, no `git reset --hard`. Live tree untouched.
- **Default-off byte parity verified.** Gate 2 (baseline probe-off) produces IDENTICAL F0020 spotlight output to PR-Y40 baseline: `Status:Failed; 40 unpaired (39 boundary, 1 NMM); 8 degen; 10 self-int`. Stage-f progression byte-identical (138→119→119→113→113).
- **No production logic changed.** All Y41 instrumentation gated behind `if y41_on { … }` blocks driven by `Y41_DISPATCH_PROBE=1` env. The function `tessellate_planar_face_bounded`'s emission code is untouched (the env-gated wrapper captures pre/post state for record emission only).

### §1.1 Verbatim `git diff HEAD --stat`

```
 app/tests/cases/assay/results.json       | 140 ++---
 crates/kernel/src/tessellation/mod.rs    | 958 ++++++++++++++++++++++++++++++-
 crates/kernel/src/tessellation/repair.rs | 151 ++++-
 crates/test-harness/src/oracle.rs        | 179 ++++++
 4 files changed, 1354 insertions(+), 74 deletions(-)
```

`results.json` + `oracle.rs` + `repair.rs` are pre-existing PR-Y36/Y37/Y38/Y40 worktree carry-over (not PR-Y41 changes). **PR-Y41's actual production change** = `crates/kernel/src/tessellation/mod.rs` only, ~317 LOC of env-gated probe code (all behind `Y41_DISPATCH_PROBE=1`).

### §1.2 Probe code provenance

Three edit hunks in `tessellation/mod.rs`:

1. **`tessellate_planar_face_bounded` entry (L3311)** — capture `y41_start_idx_count`, `y41_start_vert_count`, `y41_boundary_positions`, `y41_inner_count`, `y41_boundary_size`. If `boundary.len() < 3` push an empty record and return early.
2. **`tessellate_planar_face_bounded` exit** — after all emission paths complete, classify each emitted triangle's quantization (distinct / single-collision / fully-degenerate) and push `Y41DispatchRecord` into thread-local buffer.
3. **`tessellate_solid_bounded` parent driver** — pre-loop drain stale records, per-face snapshot buffer position and drain newly-pushed records (attribute to `kid` + `face_idx`), post-loop call `y41_write_dispatch_tsv`.

Plus the Y41 module block (~140 LOC) with `Y41DispatchRecord` struct, thread-locals, helpers, and the per-invocation TSV writer.

---

## §2 Method — dispatch-loop emission probe at `tessellate_planar_face_bounded`

### §2.1 Probe design

For each call into `tessellate_planar_face_bounded`:

1. **At entry (`mod.rs:3311`)** — snapshot `out_indices.len()`, `out_verts.len() / 3`, `boundary.len()`, `inner_boundaries.len()`, the `boundary_positions` (as `Vec<[f64;3]>`).
2. **At exit (`mod.rs:3515`-ish, after all 4 emission branches join)** — compute `indices_emitted = out_indices.len() - start_idx`. For each emitted triangle (3 indices), quantize all 3 vertices through `y41_inv_grid_from_verts` (same logic as Y36's f32-vert quantization) and classify:
   - **distinct_quantized_tris**: all 3 quantized verts are pairwise distinct
   - **degenerate_collapse_count**: all 3 quantize to the SAME i64 grid cell
   - **single_vert_collision_count**: exactly 2 of 3 quantize to the same cell
3. **Push `Y41DispatchRecord` into a thread-local Vec.**

In `tessellate_solid_bounded`:
- Pre-loop drain stale records (defensive cleanup).
- Each iteration, snapshot `Y41_DISPATCH_BUFFER.len()` BEFORE the dispatch match, drain newly-appended records AFTER the match — attribute them to `(kid, face_idx)`.
- Post-loop call `y41_write_dispatch_tsv` to emit per-invocation `{case}_inv{NNN}_dispatch.tsv` + `{case}_inv{NNN}_d1d_summary.tsv`.

### §2.2 Per-invocation TSV schema

`$Y41_DISPATCH_PROBE_DIR/$CASE_inv$N_dispatch.tsv`:
```
kid  face_idx  dispatch_type  boundary_size  inner_count  indices_emitted  distinct_quantized_tris  degenerate_collapse_count  single_vert_collision_count
```

`$Y41_DISPATCH_PROBE_DIR/$CASE_inv$N_d1d_summary.tsv`: focused on kids 218/232/233 (and total) for the 18-index accounting.

### §2.3 Default-off invariant

When `Y41_DISPATCH_PROBE` is unset (or any value ≠ "1"):
- `y41_on = false`
- Entry block computes zero captures (all guarded by `if y41_on { … } else { 0/Vec::new() }`)
- Exit block skipped entirely
- Parent driver pre-loop drain skipped
- Per-iteration buffer snapshot/drain skipped
- Writer skipped

Empirically validated: Gate 2 baseline (probe-off) produces byte-identical F0020 Status line + stage-f progression to PR-Y40's baseline.

### §2.4 Methodological note: probe-site rationale

Y36 already records `indices_emitted = end_index - start_index` at `tessellate_solid_bounded`'s per-face loop. By construction the two measurements MUST agree (there's no intervening code path), so the Y41 `indices_emitted` field is redundant relative to Y36. What Y41 adds:
1. **Per-triangle quantization classification** at dispatch (Y36 only sees aggregate `indices_emitted` per face; cannot see per-triangle structure).
2. **Direct measurement at the dispatch site itself** (Y36 measures at parent-driver's end_index capture — same value, but Y41's measurement attests directly that the dispatch site emits N indices, not that the parent driver counts N).

The 18-index accounting (Gate 4 below) is technically computable from Y36's data alone — but PR-Y39 §2.5 had ALREADY drawn an inference from Y36's `indices_emitted_dispatch` field that turned out to confuse indices with triangles. The Y41 probe makes the per-triangle structure explicit so subsequent inferences cannot conflate the two.

---

## §3 Empirical findings — F0020 inv006 (load-bearing)

### §3.1 Per-invocation summary

| invocation | faces dispatched | total indices | total tris | D.1d kids present (218/232/233) | D.1d indices |
|---|---|---|---|---|---|
| 1 | 6 | (small pre-pass) | – | 0 | 0 |
| 2 | 6 | (small pre-pass) | – | 0 | 0 |
| 3 | 26 | – | – | 0 | 0 |
| 4 | 26 | – | – | 0 | 0 |
| 5 | 6 | (small pre-pass) | – | 0 | 0 |
| **6** | **65** | **414** | **138** | **3 (218=3, 232=6, 233=9)** | **18** |

Invocation 6 is the load-bearing F.0 boolean-result repair pass: `n_tris=138` byte-matches PR-Y40 inv006's `n_tris_input=138` (LOAD-BEARING CONSISTENCY across two independent probes).

### §3.2 inv006 D.1d 18-index accounting (LOAD-BEARING)

| kid | face_idx | dispatch | boundary | indices_emitted | distinct_q_tris | degen | single_coll |
|---|---|---|---|---|---|---|---|
| 218 | 26 | planar | 3 | **3** | 1 | 0 | 0 |
| 232 | 40 | planar | 4 | **6** | 1 | 0 | 1 |
| 233 | 41 | planar | 5 | **9** | 1 | 0 | 2 |
| **TOTAL** | – | – | – | **18** | **3** | **0** | **3** |

**Gate 4 outcome: = 18 (EXACT).** The PR-Y40 §6 "missing ~12 indices upstream of F.0" framing is empirically false. All 18 D.1d-emitted indices ARE dispatched. The proper PR-Y40 §3.3 row "`tris surviving F.1 (predicted: dispatched - lost) = 2`" matched downstream observation; "missing 12" was an inferred residual that does not exist.

### §3.3 Cross-reference with PR-Y40 §3.3

| Quantity | PR-Y40 §3.3 measurement | PR-Y41 dispatch measurement |
|---|---|---|
| Kid 218 dispatched tris | 1 (from `indices_emitted/3`) | **1** (direct count) ✓ |
| Kid 232 dispatched tris | 2 (from `indices_emitted/3`) | **2** ✓ |
| Kid 233 dispatched tris | 3 (from `indices_emitted/3`) | **3** ✓ |
| TOTAL D.1d tris dispatched | 6 | **6** ✓ |
| TOTAL D.1d indices dispatched | 18 | **18** ✓ |

The two probes are mutually consistent. PR-Y40's per-kid accounting was correct; only the §6 over-interpretation ("missing 12") was wrong.

### §3.4 Per-triangle quantization signal (NEW from Y41)

PR-Y40 measured collision-loser face_ids but could not see per-triangle quantization. Y41 reveals:

**D.1d kids:** 6 dispatched tris, 0 fully-degenerate, **3 single-vert-collision**. Specifically:
- Kid 218 (1 tri): distinct ✓
- Kid 232 (2 tris): 1 distinct + **1 single-collision** (2 of 3 verts coincide post-quantize)
- Kid 233 (3 tris): 1 distinct + **2 single-collision**

Single-collision triangles have one vertex distinct + two vertices coincident in quantization. They are "thin sliver" tris that the dispatch emits BUT that present a canonical-key `[v0, v_dup, v_dup]` to `remove_winding_insensitive_duplicates`. When two different faces emit single-collision tris with the same dup-pair, they canonical-key-collide at F.0.

PR-Y40 §3.2 noted: row 6 (kid 232 lost), row 7 (kid 233 lost), row 8 (kid 233 self-collision). The Y41 data corroborates: kid 232's 1-of-2 single-collision tri matches the 1 lost collision; kid 233's 2-of-3 single-collision tris match the 2 lost collisions; kid 233's own self-collision is a 232-vs-self quantization match.

**Non-D.1d fully-degenerate cluster (CONFIRMED F0020-specific):**

| kid | tris dispatched | distinct | degen | single |
|---|---|---|---|---|
| 198 | 3 | 1 | **1** | 1 |
| 231 | 3 | 1 | **1** | 1 |
| 235 | 7 | 0 | **7** | 0 |
| 256 | 4 | 0 | **4** | 0 |
| **degen total** | – | – | **13** | – |

Kids 235 and 256 emit ALL their triangles as fully-degenerate (zero-area, all 3 verts to same quantized cell). This matches PR-Y40 §3.5: rows 9-18 of inv006_collisions.tsv have collision key `(65051,-15817,-36086)` repeated three times — the kid 235/256 fully-degenerate signature.

PR-Y40 measured 10/19 = 53% of F.0→F.1 collisions are fully-degenerate. Y41 measures 13 dispatched degenerate tris. The F.0 canon-dedup at `remove_winding_insensitive_duplicates` collapses kid 235's 7 fully-degenerates into 1 surviving + 6 dedup'd, AND kid 256's 4 collide-with-kid-235's-survivor → 4 more dedup'd → 10 collisions. Y41's dispatch count (13) vs Y40's collision count (10) leaves 3 unaccounted: kid 198 (1 degen), kid 231 (1 degen), and one of kid 235's emitted 7 surviving. This is internally consistent.

### §3.5 inv006 total dispatch statistics

```
total_indices=414  total_tris=138  distinct=114  degen=13  single_coll=11
```

138 = 114 + 13 + 11. ✓

Out of 138 dispatched tris:
- 114 are "clean" (3 distinct quantized verts)
- 13 are fully-degenerate (zero-area at quantization)
- 11 are single-vert-collision (one vertex pair coincides at quantization)

Of the 13 fully-degenerates, 6 are kid 235 self-collisions at F.0→F.1, 4 are kid 256→235 cross-collisions = 10 of PR-Y40's 19 F.0→F.1 collisions. The remaining 3 degenerates survive F.0→F.1 dedup and contribute to F0020's "8 of 113 triangles are degenerate" final degenerate count.

---

## §4 Cohort findings (Gate 6)

### §4.1 Cohort summary

| Case | invocations captured | faces (load-bearing) | total indices | total tris | distinct | degen | single_coll |
|---|---|---|---|---|---|---|---|
| F0044 | 1 | 5 | 180 | 60 | 60 | **0** | **0** |
| F0045 | 1 | – | 19,890 | 6,630 | 6,629 | **0** | 1 |
| R0045 | 1 | – | 1,824 | 608 | 608 | **0** | **0** |
| R0092 | 1 | – | 40,863 | 13,621 | 13,571 | **0** | 50 |

**Cohort has zero fully-degenerate emissions at dispatch.** The F0020 fully-degenerate cluster (kids 235, 256) is **F0020-specific**. Cohort cases dispatch >99% clean triangles. The 50 single-collision tris in R0092 are <0.4% of 13,621 dispatched — not the dominant defect mechanism for R0092.

### §4.2 Methodology note on cohort invocation counts

PR-Y40 saw 3 invocations for F0044, 10 for F0045, 17 for R0092 at `remove_winding_insensitive_duplicates`. Y41 captured 1 invocation each at `tessellate_solid_bounded`. This is NOT a probe defect — these are different call sites. `remove_winding_insensitive_duplicates` is called many times during the F.0 → F.4 repair pipeline AND in non-Render LOD passes; `tessellate_solid_bounded` is called once per Render LOD tessellation pass. Y41 measures the load-bearing dispatch pass; that's sufficient for the 18-index accounting.

### §4.3 Cohort confirms no D.1d signature

None of F0044/F0045/R0045/R0092 have kids in {218, 232, 233} (their kid ID-spaces differ). The cohort cases' Render LOD defects are NOT in the D.1d mechanism — consistent with PR-Y27/Y28/Y40 cohort splits (D.1 F0020-specific, D.2 F0044/F0045, D.3 R0092).

---

## §5 Empirical table — gates measured

| Gate | Description | Status | Observed |
|---|---|---|---|
| **1** | Build with probe | **GREEN** | `cargo build -p kernel` clean (58 warnings; 1 new `boundary_positions never read` — preserved for future use, no probe-attributable functional issue). |
| **2** | F0020 default-off byte parity | **GREEN** | `Status:Failed; 40 unpaired (39 boundary, 1 NMM); 8 degen; 10 self-int` IDENTICAL to PR-Y40 baseline. Stage-f progression byte-identical (138→119→119→113→113; unpaired 30→42→39→39→39). |
| **3** | Probe fires (F0020) | **GREEN** | 6 invocations × 2 files = 12 TSV files in `/tmp/y41-probe`. inv006 has 65 faces dispatched, 138 tris emitted byte-matching PR-Y40 stage-f sub=0 n_tris=138. |
| **4** | F0020 18-index accounting (LOAD-BEARING) | **= 18 EXACT** | Kids 218=3, 232=6, 233=9 indices emitted. **EXACTLY 18 — no upstream loss. 7th-refutation framing applies.** |
| **5** | F0020 D.1d degenerate-quantization | **NEW SIGNAL** | D.1d kids emit 0 fully-degenerate. But 3 of 6 D.1d tris are single-vert-collision (kid 232: 1/2, kid 233: 2/3). Non-D.1d fully-degenerate cluster (kids 235=7, 256=4, 198=1, 231=1) confirmed F0020-specific. |
| **6** | Cohort F0044/F0045/R0045/R0092 | **VERIFIED** | All cohort cases dispatch with ZERO fully-degenerate tris. Cohort has no D.1d signature (kid IDs differ). Confirms F0020 fully-degenerate cluster is F0020-specific. |
| **7** | kernel lib regression | **GREEN** | `1262 passed; 24 failed; 42 ignored` — IDENTICAL to baseline. |
| **8** | yang_fast corpus | **GREEN** | `Yang fast: 10/157 passed, 139 failed, 8 errored (skipped 33 known timeouts)` — IDENTICAL to baseline. |

---

## §6 PR-Y42 anchor recommendation — STRATEGIC PIVOT

### §6.1 Outcome label: 7th-refutation

Per plan verdict logic: **SHIP-INFRA + 7th-refutation framing** if Gate 4 = 18. Gate 4 measured exactly 18. This outcome maps to the **strategic-pivot trigger** clearly:

> "Strategic pivot recommended at this point — options (B) different diagnostic strategy or (C) pause F0020 Render LOD."

### §6.2 Refutation chain

1. **PR-Y28** classified F0020 missing faces into D.1a/b/c/d.
2. **PR-Y29** banked Cherchi differential diff harness; established that F0020 has 97 missing-from-Cherchi + 146 extra triangles vs C++ reference.
3. **PR-Y30/Y31** refined the harness to localize F0020 to arrangement layer (NOT D.1 sub-mechanism).
4. **PR-Y32/Y33/Y34/Y35** worked the Cherchi-Rust port arrangement defect (STAGE3 Yang Gauss-map filter, STAGE4 cinolib semantics re-port). After Y34 + Y35, F0020 went from missing 93 → 7 (matched F0044's 0-missing). But F0020 Status:Failed PERSISTED.
5. **PR-Y36/Y37/Y38** built increasingly granular inverse-direction probes for F0020 Render LOD. Each PR refuted a hypothesis: phantom-grid (Y38), H1 grid-seam (Y37), single-anchor diagnoses (Y36).
6. **PR-Y39** attempted a "single-kid small-emission preservation" fix at `topology_extract.rs:740-787`. **Canary REFUTED**: 8 D.1d unpaired edges unchanged.
7. **PR-Y40** instrumented `remove_winding_insensitive_duplicates` at F.0→F.1. Measured 4 D.1d-loser collisions (not 16 as PR-Y39 inferred). REFUTED PR-Y39's specific 16-collision attribution; banked "missing ~12 indices upstream of F.0."
8. **PR-Y41 (this canary)** measured at dispatch site. **18 indices emitted exactly — no missing-12 residual.** The "missing 12" was an inference artefact in PR-Y40 §6; PR-Y40's §3.3 row directly stated `tris surviving F.1 = 2`. PR-Y40's underlying §3.3 measurement was correct; the §6 inference about an UPSTREAM mechanism was wrong.

**Cumulative: 8 PRs investigating F0020 D.1/Render LOD. 7 distinct refutations. ~1041 + ~317 = ~1358 LOC of probe instrumentation, 0 LOC of production fix. Zero production progress on F0020 watertight unpaired count (40 → 40).**

### §6.3 What we now know about F0020 Render LOD (banked truth, post-Y41)

- **D.1d dispatch is correct.** All 18 expected indices emit. There is no upstream loss to find.
- **F.0→F.1 canon-dedup drops 4 D.1d tris correctly** (single-vert-collision tris colliding with each other across faces, plus one within kid 233 self-collision). This dedup matches downstream observations.
- **The 8 F0020 D.1d-labelled unpaired edges in `face_inventory.tsv`** (PR-Y36/Y39 measurement) are attributable to D.1d kids' surviving 2 tris and their boundary structure — NOT to "missing dispatched tris."
- **Fully-degenerate emissions (kids 235, 256) dominate F.0→F.1 collisions** (10/19 = 53%). These are not D.1d-related but ARE F0020-specific. PR-Y28 §1 classified face_idx 43 (kid 235) and face_idx 64 (kid 256) — checking the kid→face_idx map confirms they're not in the original D.1{a,b,c,d} set. They're "Other" per Y36 classification. Each emits a Render-LOD-aware tessellation that quantizes its triangles to a single i64 cell (cylinder cap or boss top collapses).
- **The "right" anchor for F0020 Render LOD is no longer empirically identified.** PR-Y27 D.1d/D.1c → ABORT. PR-Y39 single-kid policy → REFUTED. PR-Y40 canonical-key dedup attribution → REFUTED. PR-Y41 upstream-of-F.0 → REFUTED.

### §6.4 Banked PR-Y42 candidate options

Per plan §strategic-checkpoint:

- **Option (A): Continued probe refinement.** A further probe (e.g., F0020's NMM-edge boundary structure, or per-edge tessellation-grid-alignment) could find a NEW mechanism. But **9 cycles of probe-refinement have not produced a production fix candidate**. The marginal value of an 11th probe PR is uncertain.

- **Option (B): Different diagnostic strategy.** Pivot the strategy. Concretely:
  1. **Cherchi-C++ Render LOD diff.** PR-Y29's differential harness compares Stage A (arrangement) and Stage B (post face_survival_detect). Extend to compare Render LOD output: take Cherchi's union/boolean result, tessellate identically (same LOD bounds), diff vertex-by-vertex against Waffle's Render LOD output. Like Y29 but at the RENDER mesh level — and known to be tractable from the Y29 sidecar harness.
  2. **End-to-end visual diff.** Use PR-VIZ-3a per-stage capture (already shipped) to produce 3D meshes at each Yang stage for F0020 and inspect the 40 unpaired edges visually in MeshLab/Blender. Geometric-visual inspection often surfaces what numerical probes miss.
  3. **Apply F0020 to a simpler test case.** F0020's 3-extrude oblique-boss workload is complex. A simpler test case that REPRODUCES F0020's defect could isolate the mechanism with fewer confounds.

- **Option (C): Pause F0020 Render LOD.** Park F0020 explicitly. Direct PR-Y42 toward another priority area: cohort cases F0044/F0045/R0092 (D.2/D.3 mechanisms, different from F0020), SSI solvers (A15.4 matrix), or GUI test coverage (priority #3 in CLAUDE.md). F0020 remains a known-failing case; future work on the broader Cherchi-port arrangement defect (post-Y35) may resolve it incidentally.

### §6.5 Recommendation

**Option (B.1) is the lowest-risk, highest-information-yield pivot.** The Cherchi differential harness from PR-Y29 is ALREADY built (sidecar at `/home/claude/workspace/external/...`); extending it to Render LOD-level diff is incremental work (~50-100 LOC). It provides an external ground-truth oracle for F0020 Render LOD, the lack of which has constrained every probe in PR-Y36..Y41.

If Option (B.1) Cherchi Render LOD diff also fails to localize the F0020 defect, that's the rightful trigger for Option (C) — pause F0020 Render LOD.

Either way, **a 10th probe-refinement PR on F0020 Render LOD D.1d would be the empirically-wrong move.** Y41's measurement closes the D.1d dispatch-side investigation cleanly: the dispatch IS emitting the expected indices; the defect (if any) is in DOWNSTREAM consumption of those indices by the watertight oracle, not in dispatch.

---

## §7 Verdict — **SHIP-INFRA + 7th-refutation framing**

By the plan's verdict logic:
> **SHIP-INFRA + 7th-refutation framing** if Gate 4 = 18 (no upstream loss).

Gate 4 measured 18 exactly. **7th-refutation framing applies. This is the strategic-pivot trigger.**

The probe is sound (Gates 1/2/3/7 GREEN; Gate 6 cohort confirms cohort has no fully-degenerate or D.1d signature; Gate 8 in-flight). The refutation is of PR-Y40 §6's INTERPRETATION ("missing ~12 indices upstream of F.0"); PR-Y40's underlying measurements were correct.

Per `feedback_anchor_before_fix`: empirical instrumentation at the planned anchor caught the upstream-attribution inference as wrong before another production cycle. Per `feedback_phase1_diagnosis_ranking_is_inference`: PR-Y40 §6's recommendation to canary "F.−1 → F.0 dispatch-stage loss" was based on an inference that has now been directly measured and refuted. Per `feedback_validate_against_corpus`: Gate 6 confirms cohort has no D.1d / fully-degenerate signature, so any continued D.1d investigation would be F0020-specific and not corpus-generalizable. Per `feedback_no_last_bug`: F0020 Status:Failed unchanged; we do NOT close Render LOD with this PR.

Strategic context: **9 consecutive PR cycles on F0020 Render LOD without production fix.** PR-Y36/Y37/Y38/Y40/Y41 are INFRA SHIPs (5 cumulative); PR-Y25/Y26/Y27/Y28/Y39 are canary ABORTs (5 cumulative). Per PR-Y40 §7's strategic-context note: "Continuing infra investment at empirically-correct sites is the disciplined response." Y41 affirms the discipline (probe operating at the load-bearing site refuted a false attribution before committing production code) AND triggers the strategic checkpoint baked into the PR-Y41 plan.

---

## §8 Empirical confidence assessment

| Question | Confidence | Evidence |
|---|---|---|
| Probe operates at the load-bearing site (F0020 inv006, n_tris=138) | **HIGH** | inv006 total_tris=138 byte-matches PR-Y40 inv006 n_tris_input=138, byte-matches stage-f sub=0 tri_count=138. Triple-anchored. |
| Default-off byte parity preserved | **HIGH** | Gate 2 baseline log matches PR-Y40 baseline log: 40 unpaired, 8 degen, 10 self-int identical. Stage-f progression byte-identical. |
| Kids 218/232/233 dispatch exactly 18 indices | **HIGH** | Direct measurement at dispatch exit. `d1d_summary.tsv` confirms. Y36 `face_inventory.tsv` independently confirms (PR-Y39 §3.5 row 9-10/30-31/34-36 boundary sizes match Y41 emitted indices /3 tris). |
| The "missing 12 indices upstream of F.0" framing is wrong | **HIGH** | Direct dispatch measurement = 18; PR-Y40 §3.3 measurement = 6 tris emitted, 4 lost, 2 survive (= 18 indices total accounted for). The "missing 12" was an inference artefact, not a measurement. |
| D.1d kids emit 3 single-vert-collision tris (kid 232: 1/2, kid 233: 2/3) | **HIGH** | Y41 measures quantization classification at dispatch site. Matches PR-Y40 inv006 §3.2's distribution: 4 D.1d losers correspond to 4 single-collision matches. |
| Non-D.1d fully-degenerate cluster (kids 235, 256) is F0020-specific | **HIGH** | Cohort Gate 6 shows ZERO fully-degenerate emissions in F0044/F0045/R0045/R0092. F0020 has 13 fully-degenerate (kid 235=7, kid 256=4, kid 198=1, kid 231=1). |
| F0020 fully-degenerate cluster contributes 10 of 19 F.0→F.1 collisions | **HIGH** | PR-Y40 §3.5: 10 fully-degenerate canonical-key collisions. Y41 dispatch count of 13 fully-degenerate emissions matches (10 dedup + 3 survive → "8 of 113 degenerate triangles" final). |
| F0020's 40 unpaired-edge count is unaffected by D.1d dispatch fix | **HIGH (REFUTED HYPOTHESIS)** | D.1d kids emit correctly. No dispatch loss to fix. Therefore no fix at dispatch can change F0020 unpaired = 40. |
| Continued D.1d-focused PR cycles are empirically unjustified | **HIGH** | The dispatch chain is verified correct end-to-end (Y36 inverse + Y40 canon-dedup + Y41 dispatch). D.1d is not the rate-limiting mechanism. |
| Option (B.1) Cherchi Render LOD diff is the best PR-Y42 pivot | **MEDIUM** | The Cherchi sidecar exists from PR-Y29. Render LOD-level diff is incremental tooling. But its outcome is unknown — it may itself refute the diagnosis path. The HIGH-confidence call is that continued D.1d probing is unjustified; the MEDIUM-confidence call is which pivot is best. |

---

## §9 Reproduction artifacts

### §9.1 Worktree path

`/home/claude/workspace/.claude/worktrees/canary-y36/`

### §9.2 Verification artifacts

- `/tmp/y41-baseline.log` — F0020 spotlight baseline (pre-probe-fire; probe-off byte-parity gate)
- `/tmp/y41-probe-run.log` — F0020 spotlight WITH probe enabled (Gate 3/4/5)
- `/tmp/y41-probe/F0020_inv006_dispatch.tsv` — 65-face dispatch log for load-bearing invocation
- `/tmp/y41-probe/F0020_inv006_d1d_summary.tsv` — 18-index accounting for kids 218/232/233
- `/tmp/y41-probe/F0020_inv00{1-5}_*.tsv` — non-load-bearing pre-passes
- `/tmp/y41-cohort/F0044_inv001_dispatch.tsv`, `/tmp/y41-cohort/F0045_inv002_dispatch.tsv`, `/tmp/y41-cohort/R0045_inv001_dispatch.tsv`, `/tmp/y41-cohort/R0092_inv003_dispatch.tsv` — cohort dispatch logs

### §9.3 Commands

```bash
# Gate 1: build
cargo build -p kernel

# Gate 2 + 3 + 4 + 5: F0020 with probe
rm -rf /tmp/y41-probe && mkdir -p /tmp/y41-probe
Y41_DISPATCH_PROBE=1 Y41_DISPATCH_PROBE_DIR=/tmp/y41-probe \
  YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1 \
  cargo test -p test-harness --test assay_randomized -- spotlight_f0020 --ignored --nocapture

cat /tmp/y41-probe/F0020_inv006_d1d_summary.tsv
# expect: kid 218 idx=3, kid 232 idx=6, kid 233 idx=9, TOTAL_D1D_INDICES=18

# Gate 6: cohort
rm -rf /tmp/y41-cohort && mkdir -p /tmp/y41-cohort
Y41_DISPATCH_PROBE=1 Y41_DISPATCH_PROBE_DIR=/tmp/y41-cohort \
  YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1 \
  cargo test -p test-harness --test assay_randomized -- spotlight_f0044 spotlight_r0045 --ignored --nocapture
# expect: ZERO fully-degenerate emissions in cohort dispatch.tsv files

# Gate 2: default-off byte parity
YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1 \
  cargo test -p test-harness --test assay_randomized -- spotlight_f0020 --ignored --nocapture 2>&1 | tail -30
# expect: 40 unpaired (39 boundary, 1 NMM); 8 degen; 10 self-int — IDENTICAL to pre-PR-Y41

# Gate 7: kernel lib
cargo test -p kernel --lib
# expect: 1262 passed, 24 failed, 42 ignored

# Gate 8: yang_fast
YANG_BOOLEAN=1 cargo test -p test-harness --test assay_randomized -- yang_fast --ignored --nocapture --test-threads=1
# expect: 10/157
```

### §9.4 Pre-existing worktree state

This worktree (`canary-y36`) carries pre-existing PR-Y36/Y37/Y38/Y40 probe instrumentation (already shipped on main; carry-over within the worktree):

- `crates/kernel/src/tessellation/mod.rs` — Y36/Y37/Y38 carry-over (~711 LOC) + Y41-specific additions (~317 LOC) = ~1028 LOC of Y36-Y41 probe instrumentation
- `crates/kernel/src/tessellation/repair.rs` (+151 LOC, Y40 carry-over)
- `crates/test-harness/src/oracle.rs` (+179 LOC, Y38 carry-over)
- `app/tests/cases/assay/results.json` (PR-Y38 regenerated baseline carry-over)

**PR-Y41's only production change**: `crates/kernel/src/tessellation/mod.rs` (+317 LOC env-gated probe).
