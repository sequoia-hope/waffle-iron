# PR-Y15c-fix Phase 0 — `weld_shared_edge_vertices` probe pair

**Status:** INVESTIGATION SPEC (pre-FIP-§3.2). NOT a fix spec.
**Anchor evidence:** `docs/audits/pr_y15c_phase0_diagnostic.md` (Stage
E_lod=Render row 1 fires 10/10 on F0031–F0040; tri deltas F0031 −12,
F0040 −44).
**Bound directive:** `docs/audits/pr_y15c_validation.md` §4.4.
**Reproducer pair:** F0031 + F0040.
**Plan:** `/home/claude/.claude/plans/reactive-juggling-sloth.md` 0a.

## 1. Goal

Split PR-Y15c's named anchor `tessellate_solid_ext_with_lod` into 3
mutually-exclusive sub-anchors via a probe pair pre/post the
`weld_shared_edge_vertices` call at `crates/kernel/src/tessellation/mod.rs:792`.
Output: diagnostic memo `docs/audits/pr_y15c_fix_phase0_diagnostic.md`
naming the sub-anchor for the eventual PR-Y15c-fix-N. Investigation only.

## 2. Why this is not yet a fix spec

Per `~/.claude/projects/-home-claude-workspace/memory/feedback_anchor_before_fix.md`
strategic-escalation rule (three wrong anchors → reference comparison),
the F0002 / twin-pairing class consumed five wrong-anchor cycles before
PR-S1's sidecar oracle pinned the actual defect. PR-Y15c named the
function but explicitly deferred file:line precision to this PR — three
sub-anchors remain candidate.

The long-banked PR14 hypothesis (per
`~/.claude/projects/-home-claude-workspace/memory/yang_implementation_status.md`
2026-05-02 entry, verbatim):

> "Two adjacent B-Rep faces' Render-LOD tessellations produce
> non-byte-identical reciprocal edges along shared B-Rep edges."

This is the **row-2** hypothesis below; rows 1 and 3 are the
alternatives. Writing fix code without splitting these would be a
sixth wrong-anchor cycle.

**A15.6 cross-domain flag:** `tessellation::` is architecturally
outside Yang Boolean scope per `governance/ARCHITECTURAL_INVARIANTS.md`
A15.6 (pipeline ends at B-Rep assembly step 7; render LOD downstream).
Phase 0 is observation-only inside `tessellation::`. PR-Y15c-fix-N WILL
require cross-domain coordination (deferred).

## 3. What PR-Y15c Phase 0 established

10/10 cases of F0031–F0040 fired decision-tree row 1 (Stage A/Bb/B/C
true; Stage E_lod=Render false). Operand-mesh Stage E_lod=Adaptive
20/20 true (refuting f32-round-trip false-positive risk). Per-case tri
loss spans −12 to −48. Named anchor: `tessellation::tessellate_solid_ext_with_lod`.

Adversary-3's PR-Y15c validation memo §4.4 prescribes the next-layer
probe site verbatim:

> "Spec PR-Y15c-fix Phase 0 SHOULD probe pre/post
> `weld_shared_edge_vertices` at `tessellation/mod.rs:791` using
> existing `repair::count_unpaired_in_mesh` primitive (already imported
> into `tessellate_solid_ext`, called 8 times during convergence loop —
> near-zero cost). This splits 'per-face fan emission produces
> non-byte-identical verts' from 'weld loses verts' — the natural
> anchor question."

(Actual call site is L792 in current main; surrounding `needs_fan_welding`
gate at L785-787. This spec uses L792.)

## 4. Phase 0 instrumentation requirements

### 4.1 Reuse `YANG_CONFORMAL_PROBE=1`; tag `[weld-probe]`

No new env var. Tag `[weld-probe]` distinguishes from `[conformal-probe]`.
**Scope: fan-path-only** — L792 sits inside `if needs_fan_welding { ... }`
at L785-787. F0031–F0040 use cylindrical fan paths; non-fan-path
tessellations (revolve, sphere) will not fire.

### 4.2 Anchor pre-verification canary (per `feedback_anchor_before_fix.md`)

Insert at L792 BEFORE the real probe:

```rust
eprintln!("[weld-canary] reached weld_shared_edge_vertices needs_fan_welding=true");
```

Run F0031 + F0040 with `YANG_CONFORMAL_PROBE=1`. Confirm canary fires
≥3 times per case (operand A + operand B + result). **CRITICAL: if
`needs_fan_welding=false` for the F0031–F0040 cohort, L792 is SKIPPED
ENTIRELY → adversary-3's anchor recommendation is invalid → ABORT,
report to team-lead, do NOT proceed (per ENGINEERING_CONSTITUTION P10).**
Remove canary BEFORE the real probe lands.

### 4.3 Probe pair at L792 (~12 LOC)

Capture `(pre_unpaired, pre_tris)` before the weld call;
`(post_unpaired, post_tris)` after. Single line per call:

```
[weld-probe] pre_unpaired={} post_unpaired={} pre_tris={} post_tris={} tri_delta={}
```

Reuses `count_unpaired_in_mesh` at `tessellation/repair.rs:81-124`
(`pub(super)` — callable from `tessellation/mod.rs` parent; no
visibility change).

**Risk #8 (quant-scale mismatch):** `count_unpaired_in_mesh` quantizes
at `TAU_TESS_GRID_FACTOR`; `weld_shared_edge_vertices`
(`tessellation/mod.rs:883-954`) quantizes at `TAU_MODEL_RECIP` (1e-7 m).
The two scales may disagree on what counts as "shared" — confounding
the signal. Diagnostic memo MUST document any counter-intuitive
readings (e.g., `pre_unpaired=0` BUT `post_tris < pre_tris`, or the
inverse).

### 4.4 Reproducer harness

`batch_enclosed_subtract_fix` at `crates/test-harness/tests/assay_randomized.rs:445`.
F0031 + F0040 spot-check pair (operand-order coverage); full F0031–F0040
batch for cluster homogeneity. Capture per-call breakdown (operand A,
B, result) since each F-case fires ≥3 times.

### 4.5 libtest `--nocapture` quirk

```
YANG_CONFORMAL_PROBE=1 YANG_BOOLEAN=1 \
  cargo test -p test-harness --test assay_randomized --release -- \
  batch_enclosed_subtract_fix --ignored --nocapture --test-threads=1 \
  2>stderr_capture 1>stdout_capture
```

`--test-threads=1` and separated streams MANDATORY (PR-Y15a/Y15c
precedent). `TWIN_DEBUG` NOT needed — `[topo-extract]` summary is
upstream of this site.

## 5. Decision tree (3 rows)

| `pre_tris - post_tris` | `post_unpaired` | Anchor | Next PR |
|---|---|---|---|
| > 0 | (any) | `weld_shared_edge_vertices` (`tessellation/mod.rs:883-954`) collapses valid triangles into degenerates due to position-collision misdetection. Diagnostic MUST report exact tri_delta per case. | PR-Y15c-fix-1: position-collision logic in `weld_shared_edge_vertices`. |
| == 0 | > 0 | Welding ran but per-face boundary verts are NOT byte-identical at shared B-Rep edges → no shared index emerges → unpaired persists. **Confirms PR14 long-banked hypothesis.** | PR-Y15c-fix-2: `tessellate_polygon_face` per-face byte-identity at shared edges. (A15.6; likely shares root cause with PR-Y15b.1's Yang §4.1.1 fan unification.) |
| == 0 | == 0 | Welding worked; triangle loss is downstream of L792 (`compact_unreferenced_vertices` at L796, post-weld convergence at L709-735, last-resort at L745-771). | PR-Y15c-fix-3: instrument downstream stages (Stage F probe). |

## 6. FIP role assignments

Per `governance/FEATURE_IMPLEMENTATION_PROTOCOL.md` §3.2 (spec writer
≠ implementer ≠ adversary):

| Sub-phase | Agent | Writes |
|---|---|---|
| 0a Spec | spec-writer-e | `specs/yang_pr_y15c_fix_phase0_weld_probe.md` |
| 0b Implement | implementer-g (NOT spec-writer-e) | Probe code + canary + diagnostic memo |
| 0c Adversary | adversary-4 (NOT spec-writer-e, NOT implementer-g) | `docs/audits/pr_y15c_fix_phase0_validation.md` |
| 0d Commit | team-lead | Memory updates, git commit |

## 7. Out of scope

- Fix code (PR-Y15c-fix-N follows ONLY after Phase 0 names sub-anchor).
- Probes inside `weld_shared_edge_vertices` body itself (deferred;
  adversary-4 may argue for one in alternative-probe-site refutation).
- Probes at downstream stages post-L792 (deferred to PR-Y15c-fix-3 if
  row 3 fires).
- R-class cases; PR-Y15b.1 follow-ups; R0071; S-H clipping removal.
- WASM rebuild (probe is env-gated, default-off).
- **Cherchi 2022 reference comparison: NOT AVAILABLE for this stage.**
  Cherchi outputs the conformal mesh directly with no analogous
  render-LOD step (per implementer-f's PR-Y15c diagnostic §"Reference
  comparison status"). Phase 0 must rely on internal canary discipline
  + multi-stage probe instead of cross-impl parity.
- Cross-domain A15.6 coordination for the eventual fix.
- Changing `weld_shared_edge_vertices` body itself (read-only).

## 8. Phase 0 deliverable checklist

Implementer-g SHALL produce:

1. `crates/kernel/src/tessellation/mod.rs` — probe pair around L792
   (~12 LOC additive), env-gated on `YANG_CONFORMAL_PROBE=1`, tagged
   `[weld-probe]`. No visibility changes.
2. `docs/audits/pr_y15c_fix_phase0_diagnostic.md` (~120 LOC):
   verbatim probe output F0031 + F0040; cluster-homogeneity table
   F0031–F0040; decision-tree row per spec §5; named anchor file:line;
   **reconciliation: weld-probe `tri_delta` MUST match (or sum to)
   PR-Y15c Phase 0's Stage C → Stage E delta (F0031: −12; F0040: −44)**;
   quant-scale mismatch documentation if counter-intuitive readings
   surface (per §4.3); spec ambiguities encountered.
3. Production safety per DoD §6: `YANG_CONFORMAL_PROBE` unset → 0
   `[weld-probe]`, 0 `[weld-canary]` lines, F0002 trace byte-identical;
   clippy net delta = 0; `rustfmt --check` clean on touched file only
   (NO `cargo fmt -p kernel` — fmt-cascade lesson).
4. Anchor canary removed before final probe code lands (verify by
   re-grep on probe-on rerun: 0 `[weld-canary]` hits).
