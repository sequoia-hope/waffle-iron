# PR-Y15c-fix Phase 0 (v3) — Per-face dispatch probe at unequal-ring earcut silent-failure site

**Status:** INVESTIGATION SPEC (Phase 0 only, no fix code).
**Predecessor:** `specs/yang_pr_y15c_fix_phase0_stage_f_repair.md` (v2; commit `c4934c5`).
**Anchor evidence (carried over):** `docs/audits/pr_y15c_fix_phase0_diagnostic.md` + `docs/audits/pr_y15c_fix_phase0_validation.md` — uniform pre-F.0 loss of **−8 tris/case across all 10 F0031–F0040 cases**, independent of operand-order and Steiner-fan eligibility.
**Reproducer pair:** F0031 + F0040.
**Plan:** `/home/claude/.claude/plans/reactive-juggling-sloth.md` sub-phase 0a.
**Wrong-anchor count for PR-Y15c-fix arc:** currently 1 of 3 (v1 weld site refuted; v2 pinned the answer for tracks A/B). If v3 confirms L4053, count unchanged. If v3 refutes, count moves to 2 of 3.

## 1. Goal

Localize the constant **−8 tris/case** loss in per-face dispatch (Track C from v2's three-track decomposition). Primary suspect: silent-failure earcut at `crates/kernel/src/tessellation/mod.rs:4053` in the unequal-ring cylindrical patch branch. Output: a Phase 0 diagnostic memo confirming or refuting the L4053 hypothesis. Investigation only; no fix code.

## 2. Why this is the locus

v2's reconciliation table established that across all 10 F0031–F0040 cases, the pre-F.0 delta (Stage C tris → F.0 tris) is uniformly −8 — independent of cohort size (F.0 ranging 36→76), operand-order (box-minus-cyl AND cyl-minus-box), and Steiner-fan eligibility (sub-clusters A and B both affected). v2 §"Reconciliation" routes this to v3 with per-face dispatch probes.

Phase 1 exploration (this session) localized a strong specific suspect at `tessellation/mod.rs:4053-4056` — the unequal-ring cylindrical patch earcut path:

```rust
let tri_indices = earcutr::earcut(&coords_2d, &[], 2).unwrap_or_default();
if tri_indices.is_empty() {
    return;  // silent skip; no tris emitted
}
```

This is the **only** cylindrical branch that uses `unwrap_or_default()` on its earcut call — others use `.expect()` or `if let Ok(...)`. When earcut fails on the (θ, z) cylindrical-coordinate polygon, the function returns silently without emitting vertices or indices. The unequal-ring patch is hit once per result mesh per box-cyl boolean, which matches the observed properties of the −8 loss: constant magnitude, all 10 cases affected, operand-order-independent.

**A15.6 cross-domain flag:** still inside `tessellation::` per `governance/ARCHITECTURAL_INVARIANTS.md` A15.6. PR-Y15c-fix-2 (the eventual fix) will require WASM rebuild post-fix; Phase 0 is observation-only.

## 3. What v2 established

- v2 diagnostic + validation memos confirmed three concurrent anchors (tracks A/B/C). Tracks A (F.0→F.1) and B (F.2→F.3) point at repair-stage over-removal; Track C points BEFORE F.0 (per-face dispatch).
- Pre-F.0 −8 tri loss is uniform across all 10 cases (see v2 diagnostic §"Reconciliation").
- adversary-5 independently re-ran reconciliation arithmetic; matches byte-for-byte (v2 validation §5).

## 4. Phase 0 instrumentation requirements

### 4.1 Reuse `YANG_CONFORMAL_PROBE=1`; tag `[unequal-ring-probe]` + `[unequal-ring-canary]`

No new env var. Tags `[unequal-ring-probe]` and `[unequal-ring-canary]` distinguish from `[stage-f]` (v2), `[conformal-probe]` (PR-Y15c stage A/B/C/E), and `[weld-probe]` (v1 lineage).

### 4.2 Anchor pre-verification canary

Per `feedback_anchor_before_fix.md` strategic-escalation rule. Insert at `tessellation/mod.rs:4027` (entry to the `} else {` unequal-ring branch — verify exact line via Read first):

```rust
eprintln!("[unequal-ring-canary] reached unequal_ring branch boundary_len={}", boundary.len());
```

Run F0031 + F0040 with `YANG_CONFORMAL_PROBE=1`. Confirm canary fires per result-mesh case (≥1 per case). **ABORT-if-zero-fires per ENGINEERING_CONSTITUTION P10**: if 0 fires for ANY case, the unequal-ring path is NOT the locus and the suspect must be revised. Remove canary BEFORE the real probe lands.

### 4.3 Probe pair at L4051-4056 silent-failure site (~15 LOC additive)

Use `[unequal-ring-probe]` tag, gated on `YANG_CONFORMAL_PROBE=1`:

```rust
// Pre-earcut probe
if probe_on { eprintln!("[unequal-ring-probe] pre_earcut coords_2d_len={}", coords_2d.len()); }
let tri_indices = earcutr::earcut(&coords_2d, &[], 2).unwrap_or_default();
if probe_on { eprintln!("[unequal-ring-probe] post_earcut tri_indices_empty={} tri_indices_len={}", tri_indices.is_empty(), tri_indices.len()); }
if tri_indices.is_empty() {
    if probe_on { eprintln!("[unequal-ring-probe] SILENT_SKIP boundary_len={} ring1_len={} ring2_len={}", boundary.len(), ring1.len(), ring2.len()); }
    return;
}
```

Three emissions per call: pre-earcut (`coords_2d.len()`), post-earcut (`tri_indices.is_empty()`, `tri_indices.len()`), and SILENT_SKIP path (boundary, ring1, ring2 lengths). Implementer-i: verify `boundary`/`ring1`/`ring2` are the in-scope binding names at L4053 by reading the surrounding function.

### 4.4 Reproducer harness

`batch_enclosed_subtract_fix` at `crates/test-harness/tests/assay_randomized.rs:445`. F0031 + F0040 spot-check pair (operand-order coverage); full F0031–F0040 batch for cluster homogeneity.

### 4.5 libtest `--nocapture` quirk command

```
YANG_CONFORMAL_PROBE=1 YANG_BOOLEAN=1 \
  cargo test -p test-harness --test assay_randomized --release -- \
  batch_enclosed_subtract_fix --ignored --nocapture --test-threads=1 \
  2>stderr_capture 1>stdout_capture
```

`--test-threads=1` and separated streams MANDATORY (PR-Y15a/Y15c precedent).

## 5. Decision tree (3 rows)

| L4053 SILENT_SKIP fires per case | Anchor | Next PR |
|---|---|---|
| Yes, on all 10 cases | L4053 silent-failure CONFIRMED. Earcut fails on the (θ, z) polygon. | PR-Y15c-fix-2 (v3 follow-up): replace `unwrap_or_default()` with explicit failure handling; either re-tessellate via fallback strategy OR error out cleanly. |
| Canary fires but SILENT_SKIP doesn't | L4027 unequal-ring path is hit but earcut succeeds — loss is elsewhere in the per-face dispatch | Probe other candidates (planar earcut at L3425/L3463/L3704, fan emission, etc.); wrong-anchor count #3 → escalate to reference comparison per `feedback_external_coherence.md`. |
| Canary doesn't fire | Unequal-ring branch not hit on this cohort — primary suspect refuted | Re-investigate per-face dispatch from scratch; wrong-anchor count #3 → escalate. |

## 6. FIP role assignments

Per `governance/FEATURE_IMPLEMENTATION_PROTOCOL.md` §3.2 (spec writer ≠ implementer ≠ adversary):

| Sub-phase | Agent | Writes |
|---|---|---|
| 0a Spec | spec-writer-g (NEW agent) | `specs/yang_pr_y15c_fix_phase0_v3_per_face_dispatch.md` (this file) |
| 0b Implement | implementer-i (NEW; NOT spec-writer-g; NOT implementer-h) | Probe code + canary + diagnostic memo |
| 0c Adversary | adversary-6 (NEW; **NOT adversary-3, NOT adversary-5**) | `docs/audits/pr_y15c_fix_phase0_v3_validation.md` |
| 0d Commit | team-lead | Memory updates, git commit |

**adversary-6 role rationale (load-bearing).** Full role rotation away from adversary-5 per `feedback_oracle_credibility_via_role_separation.md`, even though adversary-5's v2 verdict is NOT compromised on this lineage (their reconciliation arithmetic was correct). Rotation is the safer pattern; adversary-5 stands by.

**adversary-6 directive (verbatim, MUST be honored, per `feedback_adversary_recommendations_need_canary.md`): DO NOT recommend a next-layer cheaper proxy without running it yourself FIRST.** This is the v1 lesson banked from PR-Y15c (adversary-3's L792 weld-site recommendation was inferred from another probe family's data, not measured — refuted by canary at v1). Any cheaper-proxy recommendation in adversary-6's validation memo MUST be backed by their own probe runs at the proposed site.

## 7. Out of scope

- Fix code (PR-Y15c-fix-2 follows ONLY after Phase 0 v3 confirms anchor).
- Probes inside `earcutr` itself (third-party crate; not modifiable).
- Probes at planar earcut paths (L3425, L3463, L3704) — deferred to v3-redirect if decision-tree row 2 fires.
- Fan emission probes — deferred to v3-redirect.
- R-class cases (separate cohort).
- PR-Y15b.1 follow-ups; TSV re-segmentation; R0071 kernel hang; S-H clipping removal.
- WASM rebuild (probe is env-gated, default-off).
- Cherchi 2022 reference comparison: NOT AVAILABLE for this stage. Cherchi 2022 §5 has no per-face emission concept (mesh arrangement operates globally on the merged mesh, not per-face); reference parity not buildable here. Phase 0 must rely on internal canary discipline + the L4053 hypothesis.
- Cross-domain A15.6 coordination for the eventual fix.
- Modifying L4053 itself (read-only this Phase).
- Spec'ing PR-Y15c-fix-1 and PR-Y15c-fix-3 (deferred per user's sequencing decision: gate on v3 outcome — Track C may dissolve A/B).

## 8. Phase 0 deliverable checklist

Implementer-i SHALL produce:

1. **Anchor canary** at `tessellation/mod.rs:4027` (1 LOC). Run F0031 + F0040 with `YANG_CONFORMAL_PROBE=1`; verify canary fires per result-mesh case. **ABORT if 0 fires per P10.** Remove canary BEFORE the real probes land.
2. **Probe pair** at L4051-4056 silent-failure site (~15 LOC additive), env-gated on `YANG_CONFORMAL_PROBE=1`, tagged `[unequal-ring-probe]`. Three emissions per call (pre-earcut, post-earcut, SILENT_SKIP).
3. `docs/audits/pr_y15c_fix_phase0_v3_diagnostic.md` (~120 LOC):
   - Verbatim probe output for F0031 + F0040 (per-call counts of canary + pre/post/SILENT_SKIP).
   - Cluster-homogeneity table for full F0031–F0040 batch (does L4053 SILENT_SKIP fire on all 10? Once per case or multiple times?).
   - Decision-tree row determination per spec §5.
   - **RECONCILIATION (load-bearing):** tri count saved by SILENT_SKIPs MUST equal the −8 pre-F.0 loss from v2. SILENT_SKIP fires once per case ⇒ each skip would have emitted ~8 tris. Document any discrepancy.
   - **Why earcut fails** analysis: from the SILENT_SKIP probe's boundary/ring data, infer the geometric reason (degenerate rings, theta unwrapping NaN, collinear coords, etc.).
   - Spec ambiguities encountered.
4. **Production safety verification per DoD §6:**
   - `YANG_CONFORMAL_PROBE` unset → 0 `[unequal-ring-probe]` lines, 0 `[unequal-ring-canary]` lines, F0002 `yang_trace_f0002` test passes byte-identical, results.json pass/fail counts unchanged at 11/179.
   - `cargo clippy -p kernel --no-deps`: net delta MUST be 0 (PR-Y15c baseline cited as 91; v2 observed 92 — flag any drift).
   - `rustfmt --check` only on `tessellation/mod.rs` (NO `cargo fmt -p kernel` per fmt-cascade lesson).
5. Anchor canary removed before final probe code lands (verify by re-grep on probe-on rerun: 0 `[unequal-ring-canary]` hits).
