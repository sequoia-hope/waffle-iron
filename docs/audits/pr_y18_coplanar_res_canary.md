# PR-Y18-COPLANAR-RES sub-phase 0a — F1 anchor empirical canary on F0030

**Author:** canary-runner-4
**Date:** 2026-05-06
**Scope:** READ-ONLY empirical probe of `chord_sample_count` (F1 hardcode-N=16). The verdict (PASS / PARTIAL / FAIL) gates whether PR-Y18 proceeds with F1 or aborts to F2/F3 / re-plan.

---

## §1 F1 canary results (per case)

Canary patch (REVERTED before reporting; git status clean):

```rust
fn chord_sample_count(_r: f64, _sweep: f64) -> usize {
    16  // CANARY F1: hardcode to test Layer 1 alignment to Boolean LOD
}
```

Three N variants tested. F0030 spotlight + regression test + cohort + parity per variant:

| Variant | F0030 status | Error string | partial_overlap | verts_dropped | tris_a + tris_b | Wall-clock |
|---|---|---|---|---|---|---|
| Baseline (pre-canary, N=1591) | Failed | `half_edge[4].twin = 0 but twin.twin = 32 (expected 4)` | 1 | 1591 | 36+40=76 | 39.8 ms |
| **F1 N=16** | **Failed** | `half_edge[4].twin = 0 but twin.twin = 32 (expected 4)` | 1 | 15 | **36+40=76** | 35.1 ms |
| F1 N=32 | Failed | `half_edge[4].twin = 0 but twin.twin = 32 (expected 4)` | 1 | 31 | 36+40=76 | 35.5 ms |
| F1 LOD direct (`TessellationLod::Boolean.circle_segments()` → 16) | Failed | `half_edge[4].twin = 0 but twin.twin = 32 (expected 4)` | 1 | 15 | 36+40=76 | 35.6 ms |

**All three F1 variants produce IDENTICAL final state** (`tris_a=36, tris_b=40, verts=35`, same twin-pairing failure shape). N changes only the FIRST-pass subdivide count (60+58 vs 60+58). Strategy 2 mesh refinement (`d_epsilon=3.46e-3 round 1`) erases the resolution mismatch and converges all variants to the same final mesh.

**Regression test** `pr_y17_coplanar_curve_sampling_red_phase` under N=16:

```
[pr-y17-test] [coplanar-tele] pairs=1 ... verts_dropped=15 ... partial_overlap=1
[pr-y17-test] [yang-diag] after subdivide: tris_a=36, tris_b=40, verts=35
[pr-y17-test] [topo-extract] summary: paired=23, unpaired=2, ambiguous=11
[pr-y17-test] [twin-oracle] collision_count=2
```

- Assertion 1 (`pairs=1`): GREEN ✓ (unchanged from PR-Y17-COPLANAR baseline)
- Assertion 2 (`partial_overlap=1`): GREEN ✓ (unchanged)
- **Assertion 3 (`tris_a + tris_b < 58`): RED** — got 76 (same as PR-Y17 baseline; N change does NOT advance this invariant)
- Assertion 4 (`ambiguous=0`): RED (got 11; unchanged from PR-Y17)
- Assertion 5 (`collision_count=0`): RED (got 2; unchanged from PR-Y17)

F1 alone moves zero invariants from RED to GREEN.

**Cherchi sidecar parity** `pr_y16_parity_f0030_cohort` under N=16: still **RED** with identical defect shape (`status=Failed twin.twin=32`). Yang case Failed; Cherchi sidecar produces 18 multi-paired edges (own ambiguity); lower-bar carve-out still applies. F1 does not affect parity.

---

## §2 PR11 cohort sibling probe

**F0020 spotlight (N=16, LOD variant)**: still **RED**, same defect shape (`half_edge[16].twin = 0 but twin.twin = 31`). F0020's coplanar pair is rectangle-rectangle (no circles to chord-sample), so F1 has no surface-area on F0020. RED expected, RED observed.

**F0050 spotlight (N=16, LOD variant)**: still **RED**, same defect shape (`watertight_mesh: 39 unpaired / 417, consistent_normals 162 of 265 reversed, mesh_euler V-E+F=106`). F1 has no observable effect — F0050's defect class is downstream of coplanar preprocessing. RED expected, RED observed.

**Other spotlights (N=16, LOD variant)**: F0044 Failed, F0061_gear_cut Failed (notable: F0061 also fires `[coplanar-tele] verts_dropped=15` indicating coplanar preprocessing kicks in; defect downstream). No NEW failure mode surfaced.

**Yang fast corpus sweep (release, N=16 LOD variant)**: `Yang fast: 10/157 passed, 145 failed, 2 errored (skipped 33 known timeouts)` in 112.75s.

| Bucket | Pre-PR baseline (PR-Y17 adversary-16 §2) | Post-canary F1 N=16 | Delta |
|---|---|---|---|
| Passed | 10 | **10** | **0** (F0030 NOT flipped GREEN; no previously-passing case regressed) |
| Failed | 142 | 145 | +3 |
| Errored | 5 | **2** | **−3** (3 panic-cases shifted Errored→Failed; see below) |
| Timeouts | 33 | 33 | 0 |

**Secondary observation (notable but not load-bearing for F0030):** The 5 L264 panics that PR-Y17-COPLANAR adversary-16 §5 banked (R0014, R0046, R0055, R0081, F0075) drop from 5→2 with F1. Mechanism: R0055's poly_b had 32803 chord verts on baseline (i_overlay edge case at scale); R0081 had 3373; R0046 had 26. With N=16, those become ~16 verts each and i_overlay no longer returns empty → no panic. **R0014 and F0075 still panic** (both same_dir=true, both-linear or near-linear; the panic mechanism for those is upstream of F1's reach — likely false-positive coplanar detection, not chord-sample density).

**No previously-passing case regressed** under F1.

---

## §3 Verdict: **FAIL**

F1 (with N=16, N=32, or `TessellationLod::Boolean.circle_segments()` direct call) does NOT make F0030 spotlight GREEN. F0030 fails at the byte-identical defect shape under all three F1 variants. The 3-layer resolution mismatch documented in PR-Y17-COPLANAR adversary-16 §4 is REAL but is NOT load-bearing for F0030's `half_edge[4].twin = 0 but twin.twin = 32` failure. Strategy 2 mesh refinement (`d_epsilon=3.46e-3 round 1`) converges Layer 1 / Layer 2 / Layer 3 to a common state regardless of Layer 1's initial chord count, then the SAME twin-pairing defect surfaces.

Per the plan's ABORT condition (sub-phase 0a step 7 + Risk #1): **PR-Y18 should HALT and re-plan.** The next-anchor candidates from the plan are:

- **F2** (raise tessellation LOD to TAU_MODEL) — plan flags catastrophic perf risk; rejected on its face.
- **F3** (B-Rep boundary extraction in Layer 3) — `extract_face_boundary_2d` modification; ~100-150 LOC. The `verts=35` final state across all F1 variants suggests Layer 3 is converging to ~16 boundary verts AND F2/F3 won't change that either, since Strategy 2 is the equalizer.
- **Different defect layer entirely** — given that all three F1 variants converge to byte-identical `tris_a=36, tris_b=40, verts=35` with the same twin-pairing failure, the F0030 defect is downstream of coplanar preprocessing AND downstream of Strategy 2 mesh refinement. The defect lives in topology extraction or twin pairing itself. PR-Y17-COPLANAR adversary-16 §4 also flagged this as a possible reframing.

---

## §4 Self-canaried recommendation for sub-phase 0d implementer

Per `feedback_adversary_recommendations_need_canary.md`: this section cites empirical observation, not inference.

**Empirically verified by THIS canary (4 variants run, all reverted, all logged):**
- F1 with N=16 leaves F0030 at byte-identical failure (`twin.twin=32`, `tris_a=36 tris_b=40 verts=35`) versus N=1591 baseline.
- F1 with N=32 produces same final state (only verts_dropped intermediate counter differs: 31 vs 15).
- F1 with `TessellationLod::Boolean.circle_segments()` direct call produces same final state as N=16 (LOD returns 16 by default; same outcome as hardcoded 16).
- Yang fast corpus baseline preserved (10/157; no regression).
- Strategy 2 mesh refinement (`d_epsilon=3.46e-3 round 1`) is the convergence layer that erases F1's effect.

**Empirically NOT verified (would require additional probes beyond this canary):**
- That the twin-pairing defect at `half_edge[4]` is in `topology_extract.rs` vs `flood_fill_patches` vs B-Rep assembly. The plan presumes coplanar preprocessing as the anchor; this canary refutes that for F0030.
- Whether F3 (Layer 3 B-Rep boundary extraction) would change the post-Strategy-2 state — given Strategy 2's role, unlikely.
- Whether bypassing Strategy 2 entirely (forcing the first-pass `tris_a=60 tris_b=58 verts=50` state into topology extraction) would surface a different defect.

**Recommendation for sub-phase 0d implementer (if PR-Y18 is re-scoped post-ABORT):**

1. **Do NOT proceed with F1 as the F0030 fix.** It changes intermediate state but not the final converged state under Strategy 2.
2. **Do NOT proceed with F2.** Plan flags catastrophic perf risk; the canary confirms F2 won't help anyway (Strategy 2 erases Layer-1 resolution differences).
3. **Probe Strategy 2's interaction with `inject_partial_overlap_mesh`.** Empirical observation: post-Strategy-2 `tris_a=36 tris_b=40 verts=35` is the SAME state regardless of Layer 1 chord count. This suggests Strategy 2 IS doing the inter-layer alignment that F1 was supposed to do — but the resulting topology STILL has the twin-pairing defect. The defect is downstream.
4. **Re-anchor on the actual twin-pairing failure point.** `half_edge[4].twin = 0 but twin.twin = 32` is in `topology_extract.rs` per the validation memo path. The 11 `ambiguous` and 2 `collision_count` reported by the regression test telemetry (consistent across N variants) point at the topology extraction phase, NOT at coplanar preprocessing. Sub-phase 0d (or its replacement) should instrument `topology_extract.rs` for F0030 and identify which directed half-edge pair produces the `twin=0 but twin.twin=32` invalid state.

**Bonus finding to bank for follow-up PR (NOT a recommendation for THIS PR's scope):** F1 reduces L264 panics from 5→2 across the corpus. R0046, R0055, R0081 stop panicking under N=16 (i_overlay no longer returns empty when poly_b is ≤16 verts instead of thousands). R0014 and F0075 still panic — those need a different fix mechanism. This is a SECONDARY benefit of F1 unrelated to F0030's anchor; if a future PR has separate justification to land F1, this is supporting evidence.

**Self-canary status of this recommendation:** I empirically observed F0030's converged state under all 4 N variants and Yang fast's 10/157 stability. I did NOT empirically verify the recommendation in §4(4) (re-anchor to topology extract); that's the next probe's job, exactly per `feedback_adversary_recommendations_need_canary.md`.

---

## §5 Wall-clock impact assessment

F0030 spotlight wall-clock per variant:

| Variant | Wall-clock | Δ vs baseline |
|---|---|---|
| Baseline (N=1591) | 39.8 ms | — |
| F1 N=16 | 35.1 ms | −4.7 ms (−12%) |
| F1 N=32 | 35.5 ms | −4.3 ms (−11%) |
| F1 LOD direct | 35.6 ms | −4.2 ms (−11%) |

Modest improvement (~10%). Most of F0030's time is in Strategy 2 mesh refinement + downstream topology extraction, NOT in Layer 1 chord sampling. This is consistent with §3's mechanistic finding: F1 trims a small slice of upstream cost but doesn't move the dominant downstream cost.

Yang fast corpus wall-clock: 112.75s (release build) under F1 N=16. PR-Y17 adversary-16 §6 reported 375.06s under N=1591 (debug build). Apples-to-oranges (release vs debug); cannot make a clean perf claim from this comparison alone. Banked: a clean apples-to-apples pre/post under release would quantify F1's corpus-scale perf benefit, but per §3 verdict FAIL it's moot for PR-Y18.

---

## Verification before reporting completion

- ✓ `git diff` clean post-revert (`git status --short` empty).
- ✓ `cargo build -p kernel` clean post-revert.
- ✓ Memo at `docs/audits/pr_y18_coplanar_res_canary.md` (this file) has §1-§5 with non-empty bodies.
- ✓ §3 picks ONE verdict: **FAIL** (not "see notes").
- ✓ §4 self-canaried per `feedback_adversary_recommendations_need_canary.md`: empirical observations cited from §1-§3; non-empirical claims explicitly flagged as "NOT verified".
- ✓ §5 quantifies wall-clock per variant.
- ✓ All 4 variant runs logged (`/tmp/canary4_f0030.log`, `/tmp/canary4_f0030_n32.log`, `/tmp/canary4_f0030_lod.log`, `/tmp/canary4_yang_fast2.log`, `/tmp/canary4_regression.log`, `/tmp/canary4_cohort.log`).
