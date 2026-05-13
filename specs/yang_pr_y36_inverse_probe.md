# PR-Y36 — Inverse-direction probe at `tessellate_solid_bounded`; F0020 Render LOD source-face attribution; **SHIP-INFRA + 5th-refutation framing**

| Field | Value |
|---|---|
| Authors | spec-y36, canary-y36 |
| Parent | `8778907` (post-PR-Y35.1 ACCEPT — main HEAD on 2026-05-13) |
| Date | 2026-05-13 |
| Class | **INFRASTRUCTURE-ONLY** — instrumentation + memo; **zero production logic changed** |
| Verdict | **SHIP-INFRA + 5th-refutation framing** — PR-Y28's D.1c-dominant hypothesis empirically refuted at HEAD (0% attribution); new dominant OTHER cluster (56.4%) banked for PR-Y37 investigation |
| Load-bearing artifact | `docs/audits/pr_y36_canary.md` — F0020 attribution table + cohort sanity + 7-gate verification |

---

## §1 Context

### §1.1 F0020 Render LOD trajectory

F0020's Render LOD layer has been the architectural blocker for `Status:Failed` across PR-Y22-RECOVERY (shipped, topo-extract 2→0) followed by **four consecutive canary-stage ABORTs** on production-fix candidates (Y23-Y28, all refuted before commit), then a pivot to **Cherchi-Rust port byte-parity** (Y29 diff harness → Y30 Stage B calibration → Y31 op-plumb → Y32 port-defect localization → Y33/Y34/Y35/Y35.1) which reached byte-parity with Cherchi C++ at STAGE4. Phase 1 Explore re-measurement at HEAD `8778907` shows F0020 Render LOD oracle unpaired **worsened** from PR-Y28's 36 → 40 (39 boundary + 1 NMM); conformal probe = 56; Status still Failed.

The aggregate worsening is **not a regression** in the `feedback_no_regression_chasing` sense. Upstream byte parity correctly enriched the boolean-output arena topology (33 → 65 faces at F0020 inv#6); the downstream `tessellate_solid_bounded` dispatch was not designed for that density and exposes a new defect class. Tracing the new defect requires source-attribution data that no prior PR cycle produced.

### §1.2 Strategic context

PR-Y28 banked an **inverse-direction probe** ("where do the missing-twin edges come from?") as the right next investigation, but the team pivoted to Cherchi differential diff (PR-Y29) to address the larger byte-parity gap. With Y29-Y35.1 byte parity now shipped, the inverse-direction probe is the right next step.

PR-Y28's banked sub-mechanism classification — `D.1a` (boundary<3 planar entry), `D.1b` (earcut empty on 3-vert coincident), `D.1c` (≥90% NMM boundary loss at `remove_winding_insensitive_duplicates`), `D.1d` (clean boundary dropped in `remove_nonmanifold_topology_aware`) — provides the inverse probe's classification taxonomy. PR-Y36 re-derives source-face attribution at HEAD against this taxonomy.

---

## §2 Why infrastructure-class

### §2.1 Strategic escalation rule (`feedback_anchor_before_fix`)

> "Three wrong anchors → stop bisecting, build a reference comparison."

PR-Y23-Y28 were four consecutive canary-stage ABORTs against Render LOD production fixes; PR-Y36 follows the strategic escalation. The inverse probe **is** the reference comparison for F0020's Render LOD — the empirical attribution chain that defect-position → source-face → sub-mechanism has not been observable through any prior probe. Per `feedback_external_coherence`, when no external paper reference covers the target layer (Cherchi 2022 §3 paper scope ends at the arrangement output; Render LOD is downstream and unreferenced), the in-situ probe becomes the empirical reference.

### §2.2 No fix shape without verified attribution (`feedback_phase1_diagnosis_ranking_is_inference`)

PR-Y28's framework ranking ("D.1c is dominant") was inference from forward-mechanism inspection. PR-Y36 measures the inverse direction. Until the attribution chain is verified end-to-end through `parent_tri` lineage, no fix shape is proposed. The 5th canary-stage finding-no-fix-shape outcome (Y25/Y26/Y27/Y28/Y36) is on-discipline, not a meta-failure: each ABORT/SHIP-INFRA materially shrunk the candidate space.

### §2.3 What ships

The PR-Y36 commit is `feat(yang-pr-y36): …` (or `infra(yang-pr-y36): …`), additive-only: ~360 LOC of `Y36ProbeFaceInfo` struct, `Y36Class` enum, classification + quantization helpers, invocation counter, and the `y36_write_inverse_attribution` writer in `crates/kernel/src/tessellation/mod.rs`, plus ~75 LOC of per-face capture inside the dispatch loop (all gated `if y36_on { … }`), the canary memo, and this spec. **Zero changes to `disc.positions`, `vertices`, `indices`, `face_ranges`, or arena state.** Per `feedback_no_regression_chasing`: this is infra-class — there is no production logic to revert. Default-off byte parity is the load-bearing invariant (Gate 6 + Gate 7 in §7).

---

## §3 Probe design

### §3.1 Where the probe runs

The probe operates at **two points** inside `crates/kernel/src/tessellation/mod.rs::tessellate_solid_bounded`:

1. **Inside the dispatch loop** (`mod.rs:4581-4744` in worktree canary-y36), for each face in the sorted `face_map`. Captures per-face properties at dispatch time.
2. **At the end of `tessellate_solid_bounded`**, after the F.4 stage probe and immediately before the `Ok(RenderMesh {…})` return. `y36_write_inverse_attribution` is called with the captured face inventory plus the final f32 vertex/index/face_range arrays.

The probe extends the existing `YANG_CONFORMAL_PROBE` stage-f probe pattern at `mod.rs:4295-4416` — same env-gating discipline, same per-invocation counter pattern, same TSV/log output format.

### §3.2 Per-face capture (inside dispatch loop)

For each face in `face_map`, while `y36_on`: compute `geom_label`; walk the outer loop's half-edges via `arena.half_edges[…].next` to count `outer_he_count`, `outer_nmm_count` (HEs with `twin.is_none()`), and `is_self_loop` (`next == start`); call `collect_loop_boundary(arena, face.outer_loop, &disc)` for ordered f64 vertex positions; after the dispatch's `tessellate_*` call, record `indices_emitted = end_index - start_index` and `face_range_pushed = (end_index > start_index)`; push a `Y36ProbeFaceInfo` to a per-invocation `Vec`.

### §3.3 Inverse attribution writer

At end of `tessellate_solid_bounded`, when `y36_on`, `y36_write_inverse_attribution`:

1. **Quantizes to the oracle's grid.** `inv_grid = 1 / max(max_abs * TAU_TESS_GRID_FACTOR, TAU_TESS_GRID_MIN)` — byte-for-byte match with `count_unpaired_in_mesh`.
2. **Builds edge → count + edge → incident-tri + reverse edge → kid-list maps** (`BTreeMap<(QPos, QPos), …>`), quantizing each captured face's boundary edges (n consecutive `(p_i, p_{i+1 mod n})` segments) using the SAME inv_grid.
3. **For each unpaired edge (`count != 2`):** look up the lone incident triangle and walk to `kept_face_id` via `face_ranges.find(|fr| fr.start <= i3 < fr.end)`. Prefer the dropped/missing source — scan candidates from edge → kid-list and pick the first kid NOT in final `face_ranges`; classify via `y36_classify(info, dropped_in_repair = pushed && !in_final)`. Fallback: attribute to the kept face id.
4. **Emits two TSVs** at `<dump_dir>/<case>_inv<NNN>_inverse_attribution.tsv` (one row per unpaired edge with columns `unpaired_edge_id, v0_*, v1_*, source_face_id, classification, boundary_size, nmm_pct, …`) and `<case>_inv<NNN>_face_inventory.tsv`. Stderr summary:

   ```
   [y36-inverse-probe] case=F0020 inv#6 total_unpaired=39 D1a=9 D1b=0 D1c=0 D1d=8 OTHER=22
   ```

### §3.4 Classification function (`y36_classify`)

```text
if  info.outer_boundary_len < 3                    → D.1a
elif NMM_pct >= 90%                                → D.1c
elif !face_range_pushed && bound_len >= 3          → D.1b
elif face_range_pushed && dropped_in_repair        → D.1d
else                                               → OTHER
```

The 90% NMM threshold is anchored against PR-Y28 §1.2's verbatim observation: D.1c-classified faces measured at 12/12 (100%) and 4/4 (100%) NMM. The threshold permits slack but the canary's data confirms zero faces sit in the ≥90% band at HEAD's unpaired-attribution.

### §3.5 Env gating + invocation counter

- `Y36_INVERSE_PROBE=1` — turns probe on. All per-face capture and writer code is wrapped `if y36_on { … }`. Default-off path is byte-identical (verified by Gate 6 + Gate 7 + post-probe baseline re-run reproducing 40-unpaired Status:Failed signature).
- `Y36_INVERSE_PROBE_DIR=<path>` — supplies the dump directory. Absent → stderr summary still prints, but no file write.
- Per-thread `Y36_INVOCATION_COUNTER` disambiguates the 6 invocations of `tessellate_solid_bounded` within a single F0020 spotlight run (matching the existing `CURRENT_CASE_ID` pattern in `yang_integration.rs`).

---

## §4 Empirical findings (load-bearing)

### §4.1 F0020 inv#6 attribution (load-bearing — final Render LOD)

The watertight oracle reports 40 unpaired edges (39 boundary + 1 NMM). The F.4-grid probe reports 39 unpaired (off-by-one is the 1 NMM edge under count!=2 vs count==1 dispatch — documented banked finding §8.4):

| Class | Count | % | Mechanism |
|---|---|---|---|
| **D.1a** | 9 | 23.1% | `outer_boundary_len < 3` planar entry gate (self-loop or 2-HE cycle, `indices_emitted=0`) |
| **D.1b** | 0 | 0.0% | earcut zero-emit on coincident-vertex 3-bounded |
| **D.1c** | 0 | 0.0% | ≥90% NMM boundary loss at `remove_winding_insensitive_duplicates` — **PR-Y28's dominant cluster; empirically zero at HEAD** |
| **D.1d** | 8 | 20.5% | repair-pass drop of 3- to 5-vert clean boundary (kids 218, 232, 233) |
| **D.1 total** | **17** | **43.6%** | dispatch + repair drops, collectively |
| **OTHER** | **22** | **56.4%** | partial-NMM kept face (50–69% NMM), **NOT in PR-Y28's D.1 set** |

**OTHER cluster composition (22/22):** 11 attributions to partial-NMM kept faces (50–69% NMM: kids 226, 229, 231) + 11 attributions to zero-NMM larger-boundary kept faces (kids 195/197/204/206/207/212/213/215/216). All 22 are `pushed=true, in_final=true` — i.e., kept, not dropped.

### §4.2 F0020 per-invocation rollup

inv#1/#2/#5 = 0 unpaired (clean). inv#3/#4 = 14 each (D.1a=4, D.1d=10, OTHER=0 — original PR-Y28 mechanism, simpler boolean output). **inv#6 (load-bearing, final render LOD feeding watertight oracle) = 39 (D.1a=9, D.1d=8, OTHER=22)** — this is where OTHER becomes dominant and the Status:Failed signal originates.

### §4.3 Cohort sanity (Gate 5 — METHODOLOGY VALIDATED)

| Case | total | D.1a | D.1b | D.1c | D.1d | OTHER | D.1 % | Verdict |
|---|---|---|---|---|---|---|---|---|
| F0044 | 12 | 0 | 0 | 0 | 0 | 12 | 0% | 100% OTHER → confirms PR-Y27 D.2 cohort attribution (sub-grid seam) |
| F0045 | 38 | 0 | 0 | 0 | 0 | 38 | 0% | 100% OTHER → confirms D.2 |
| R0092 | 43 | 0 | 0 | 0 | 0 | 43 | 0% | 100% OTHER → confirms PR-Y27 D.3 (NMM-edge tessellation gap) |

Per canary mandate Gate 5: "If F0044/R0092 attribute >50% to D.1 categories, the methodology is wrong — STOP." All three cohort cases attribute 0% to D.1 → **methodology sound for cross-cohort attribution**. The PR-Y27 cohort split (D.1 = F0020-only / D.2 = F0044+F0045 / D.3 = R0092) survives at HEAD.

### §4.4 Arena topology shift

PR-Y28 inventory: 33 arena faces at the equivalent of inv#6. PR-Y36 inventory: **65 arena faces** — nearly 2× the topology. This is the by-product of correct upstream byte-parity work (PR-Y34/Y35/Y35.1), not a regression. Most new faces are D.1a-classified (planar self-loops); the dispatch loop now iterates 65 faces but only successfully emits triangles for 32 (49%) at F.4.

### §4.5 Curious finding — D.1c-signature faces present, but not unpaired-source

Kids 235 (7/7 = 100% NMM) and 256 (4/4 = 100% NMM) are present in the inv#6 inventory with the D.1c signature, but neither shows up in the unpaired-edge attribution. Their boundary edges quantize to grid cells that don't host unpaired edges at F.4 — their peer pairings ARE happening correctly. The post-Y34/Y35/Y35.1 byte-parity sequence appears to have repaired the all-NMM peer-pairing problem; the partial-NMM kept-face problem is the new defect class.

---

## §5 PR-Y37 anchor recommendation — strict-numeric vs empirical-narrative

### §5.1 Strict-numeric reading (LAYERED)

Per PR-Y28 §3 acceptance gates (verbatim from the plan):

> - **≥80% map to D.1c** → β-shape (peer-patch synthesis) is the rightful PR-Y37 anchor
> - **≥25% but <80% map to D.1c** → LAYERED defect; pick simpler sub-mechanism first
> - **<25% map to D.1 set** → 5th refutation; re-canary the cohort split

The F0020 inv#6 numerics:

- D.1c = 0% (NOT ≥80%) → β-shape gate fails
- D.1 total = 43.6% (between 25% and 80%) → strict-numeric match is **LAYERED**

Under LAYERED, the next step would be "pick a simpler sub-mechanism." The accounting-favorable candidate is **D.1d kids (218, 232, 233)** at `crates/kernel/src/tessellation/repair.rs:585`: 3 source faces collectively account for 8 of 40 oracle unpaired (20%). Predicted F0020 outcome: 40 → ~32 unpaired (does **NOT** close Status:Failed; partial win). Cohort regression risk: F0044/F0045/R0092 have zero arena-drop status as their D.2/D.3 invariant — must be verified preserved by any D.1d fix.

### §5.2 Empirical-narrative reading (5th-refutation) — RECOMMENDED

Two observations promote the recommendation from LAYERED to **5th-refutation**:

1. **D.1c at HEAD = 0%**, vs PR-Y28's measurement of D.1c as the **dominant** sub-mechanism (PR-Y28 §1.3: 2 faces with 12/12 and 4/4 = 100% NMM, 48 of 51 emitted-then-dropped triangles, 94%). The post-Y34/Y35/Y35.1 boolean pipeline has empirically **eliminated** D.1c from the unpaired-edge source set. β-shape (peer-patch synthesis at `topology_extract.rs:745-969`), the rightful PR-Y29 anchor under the original framework, is now empirically **unsupported** as the right load-bearing PR-Y37 anchor.

2. **OTHER (56.4%) is dominant and is NOT in PR-Y28's D.1 framework.** PR-Y28's D.1 set was framed around faces that were dropped or never emitted; OTHER faces are kept (`pushed=true, in_final=true`) but their boundaries host unpaired edges. This is qualitatively a different defect class. PR-Y28's framework is partially **stale** at HEAD.

**Promotion reasoning:** the strict-numeric reading prescribes a partial fix (D.1d, ~20% reduction) that does not close F0020 Status:Failed and risks D.2/D.3 cohort regression. The empirical-narrative reading correctly identifies that the dominant 56.4% mass sits in an undiagnosed cluster. Per `feedback_phase1_diagnosis_ranking_is_inference`, picking a fix shape before measuring the dominant cluster is premature. Per `feedback_no_last_bug`, the 5th-refutation framing is honest about what we don't yet know.

### §5.3 PR-Y37 anchor (banked) — investigational canary on OTHER

**Shape:** NEW investigational canary, INFRA-CLASS (like PR-Y36), extending PR-Y36's probe to capture per-face render-mesh tessellation properties for OTHER-attributed unpaired edges. Specifically:

1. For each OTHER-attributed unpaired edge in inv#6 (22 edges), determine WHY the lone incident triangle's expected peer triangle from the adjacent face is missing.
2. Three hypotheses to canary:
   - **H1 — quantization sub-grid mismatch** (D.2-like at F0020): two adjacent faces emit triangles at f32 positions that quantize to different cells. (Yang §4.4.1 mesh-updating prescribes re-mesh-along-refined-curves to keep bijectivity across optimization-shifted intersection curves; this is the paper's principled fix for the relevant defect class, banked but not directly applied by PR-Y36.)
   - **H2 — NMM-pair render asymmetry** (D.3-like at F0020): partial-NMM patches' NMM half-edges are not getting their render twin emitted by the adjacent face, even though the patches are paired in the arena.
   - **H3 — new sub-mechanism not previously enumerated.**
3. **Cohort cross-check** is the load-bearing verification: H1 should explain 100% of F0044/F0045 unpaired (12 + 38 = 50 edges); H2 should explain 100% of R0092 unpaired (43 edges). If F0020's OTHER (22 edges) maps cleanly to a mix of H1 + H2, the OTHER cluster is **NOT** novel — it's the SAME defects as F0044/R0092 cohorts but reaching F0020 only at the higher post-Y34/Y35/Y35.1 topology density.

**LOC budget for PR-Y37 canary:** ~80-150 LOC additive instrumentation (extending PR-Y36's probe). No production fix.

### §5.4 Narrower banked alternative

If team-lead prefers a minimal next step instead of the OTHER investigation:

- **PR-Y37 (narrower):** D.1d kids survival fix at `crates/kernel/src/tessellation/repair.rs:585`. Investigate why kids 218, 232, 233 are dropped at `remove_nonmanifold_topology_aware`. ~30-80 LOC production change.
- **Predicted F0020 outcome:** 40 → ~32 oracle unpaired. Does **NOT** close Status:Failed.
- **Cohort regression risk:** must verify F0044/F0045/R0092 zero arena-drop status (their D.2/D.3 invariant) is preserved.
- Structural similar to PR-Y28 §4 banked α hygiene PR.

### §5.5 What PR-Y37 should NOT be

- **NOT β-shape (peer-patch synthesis):** empirically unsupported at HEAD (D.1c = 0%).
- **NOT γ-shape (pre-dedup conformal-merge):** would target a defect class that does not surface as unpaired source at HEAD.
- **NOT a fix against D.1a alone:** D.1a accounts for 23% but its faces emit zero triangles — fixing the planar entry gate to accept them would inject NEW geometry, not close existing unpaired edges.
- **NOT a "this closes Yang" claim.** Per `feedback_no_last_bug`. The OTHER cluster is the largest unknown.

---

## §6 Out of scope

PR-Y36 does **NOT** close, reduce, or address:

- **F0020 Status:Failed** — Render LOD oracle unpaired stays at 40 (no production logic touched). The probe is observation-only.
- **F0045 tessellation-grid divergence** — PR-Y27 D.2 sub-grid seam mismatch; F0045 inv#2 attributed 38/38 OTHER, deferred to PR-Y37 H1 investigation.
- **R0092 NMM-edge tessellation gap** — PR-Y27 D.3; R0092 inv#3 attributed 43/43 OTHER, deferred to PR-Y37 H2 investigation.
- **139 yang_fast failing cases** — full corpus remediation; PR-Y36 ships at 10/157 (no movement).
- **The yang_fast baseline correction** — PR-Y31 banked finding (10/157 not 11/157); orthogonal.
- **Cherchi C++ TBB non-determinism gating** — banked since PR-Y31; not in PR-Y36 scope.
- **Yang §4.4.1 mesh updating** — paper-prescribed re-mesh-along-refined-curves is RELEVANT background context for the OTHER cluster's H1 sub-hypothesis (PR-Y37) but is NOT directly applied by PR-Y36. Per `feedback_yang_only`, no Yang-§X claims are made on behalf of PR-Y36's own changes.

No "closes Yang" / "last gap" / "Status flips to Pass" language is made in this spec or in the canary memo.

---

## §7 Risk / mitigation

### §7.1 Probe attribution accuracy depends on `parent_tri` lineage

The probe's source-attribution chain walks from final-mesh unpaired edge → lone incident triangle → enclosing `face_range` → kept face id. For the dropped-source path, it walks from quantized edge → reverse edge-to-kid map → first kid NOT in final `face_ranges`. Both depend on the per-face dispatch-time capture being intact through dispatch and on the quantization grid being byte-identical to `count_unpaired_in_mesh`.

**Risk:** if `parent_tri` lineage breaks (e.g., due to future NMM-edge re-keying or a downstream re-tessellation step), attribution becomes noise.

**Mitigation:** **Gate 5 cohort sanity check (PASSED).** F0044/F0045/R0092 each attribute 100% OTHER and 0% D.1. If the methodology were broken, cohort cases would show spurious D.1 attributions (because PR-Y27's D.2/D.3 mechanism faces are kept-not-dropped; if the probe wrongly attributed them to D.1a/b/c/d, the cohort would show non-zero D.1 percentages). The 0% D.1 across all three cohort cases confirms the probe correctly distinguishes drop sources from kept-but-unpaired sources.

### §7.2 Default-off byte parity

The probe is additive and env-gated. Default-off behavior MUST be byte-identical to `8778907`.

**Risk:** instrumentation accidentally mutates state (e.g., `disc.positions`, `vertices`, `indices`, `face_ranges`, arena fields).

**Mitigation:**

- **Gate 6 (PASSED):** `cargo test -p kernel --lib` → `1262 passed; 24 failed; 42 ignored` — exact baseline match.
- **Gate 7 (PASSED):** `YANG_BOOLEAN=1 ... yang_fast` → `10/157 passed, 139 failed, 8 errored` — exact baseline match.
- Post-probe re-run without `Y36_INVERSE_PROBE=1` reproduces the 40-unpaired Status:Failed signature on F0020.

All probe writes are `eprintln!` and file-write to `Y36_INVERSE_PROBE_DIR`. The `Vec<Y36ProbeFaceInfo>` capture allocates only inside `if y36_on { … }` blocks.

### §7.3 F.4-grid vs oracle-grid 1-edge discrepancy

The F.4 probe quantization reports 39 unpaired edges for F0020 inv#6; the watertight oracle reports 40 (= 39 boundary + 1 NMM). The off-by-one is the 1 NMM edge under count!=2 vs count==1 dispatch. Documented in canary §8.4 as a banked future-probe refinement (triple-bucket: `count<2 / count==2 / count>2`).

**Risk:** percentages in §4.1 are computed against base = 39, not 40. Difference = 1/40 = 2.5%, does not affect verdict.

### §7.4 Reproduction is non-destructive

All probe instrumentation lives in worktree `canary-y36` (branch `worktree-canary-y36`) rooted at `8778907`. The implementer (`impl-y36`) applies it to live tree as a fresh commit in Phase 5; the canary worktree itself is read-only post-canary. Per `feedback_adversary_no_destructive_git` (extending to canary discipline): no `git stash`, `git checkout --`, `git reset --hard`, or other destructive op on live tree.

---

## §8 Reproduction (verification, end-to-end)

```bash
# 1. Probe fires when env set
cd /home/claude/workspace && rm -rf /tmp/y36-verify && mkdir -p /tmp/y36-verify
Y36_INVERSE_PROBE=1 Y36_INVERSE_PROBE_DIR=/tmp/y36-verify \
  YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1 \
  cargo test -p test-harness --test assay_randomized -- spotlight_f0020 --ignored --nocapture
# expect 6 inverse_attribution.tsv + 6 face_inventory.tsv in /tmp/y36-verify/

# 2. Probe default-off byte parity (no env var)
YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1 \
  cargo test -p test-harness --test assay_randomized -- spotlight_f0020 --ignored --nocapture 2>&1 | tail -30
# expect identical Status/oracle output (40 unpaired, Status:Failed)

# 3. kernel lib suite (Gate 6) — expect 1262/24/42
cargo test -p kernel --lib

# 4. yang_fast corpus (Gate 7) — expect 10/157
YANG_BOOLEAN=1 cargo test -p test-harness --test assay_randomized -- yang_fast --ignored --nocapture
```

The load-bearing summary line on F0020 spotlight (with probe enabled):

```
[y36-inverse-probe] case=F0020 inv#6 total_unpaired=39 D1a=9 D1b=0 D1c=0 D1d=8 OTHER=22
```

End of spec.
