# PR-Y15c-fix Phase 0 (v2) — Stage F multi-probe across `tessellate_solid_bounded` repair pipeline

**Status:** INVESTIGATION SPEC (pre-FIP-§3.2). NOT a fix spec.
**Anchor evidence (carried over):** `docs/audits/pr_y15c_phase0_diagnostic.md` (Stage E_lod=Render row 1 fires 10/10 on F0031–F0040; tri deltas F0031 −12, F0040 −44).
**Aborted attempt (v1):** `specs/yang_pr_y15c_fix_phase0_weld_probe.md` (spec_e + implementer_g, 2026-05-04). Anchor canary at `tessellation/mod.rs:792` fired 0 times. Implementer-g's dispatch trace proved all 10 result-mesh calls early-return at `tessellation/mod.rs:231` into `tessellate_solid_bounded`. Aborted cleanly per `feedback_anchor_before_fix.md`. Tree byte-clean.
**Refuted directive (background):** `docs/audits/pr_y15c_validation.md` §4.4 — adversary-3's recommendation that the L791-792 `weld_shared_edge_vertices` boundary was the next cheaper-proxy site. Refuted by canary at v1.
**Reproducer pair:** F0031 + F0040.
**Plan:** `/home/claude/.claude/plans/reactive-juggling-sloth.md` sub-phase 0a (re-spec).
**Wrong-anchor count for PR-Y15c-fix:** 1 of 3 (weld site refuted). Strategic-escalation rule fires at 3.

## 1. Goal

Localize WHICH of the 4 repair stages in `tessellate_solid_bounded` (`crates/kernel/src/tessellation/mod.rs:4164-4346`) drops the 12-44 missing triangles per F-case. The 4 candidate stages are `remove_winding_insensitive_duplicates` (L4283), `remove_nonmanifold_topology_aware` (L4318), `remove_nonmanifold_duplicates_aggressive` (L4330), `weld_smooth_vertices` (L4338). Output: a Phase 0 diagnostic memo (`docs/audits/pr_y15c_fix_phase0_diagnostic.md`) naming the dropper. Investigation only; no fix code.

## 2. Why this is the new locus

The v1 spec targeted `weld_shared_edge_vertices` at `tessellation/mod.rs:792` per the refuted §4.4 directive. Implementer-g inserted the anchor pre-verification canary per `feedback_anchor_before_fix.md` and ran the F0031–F0040 batch under `YANG_CONFORMAL_PROBE=1`: **canary fired 0 times**. The L792 weld call is dead code for the result-mesh path; all 10 result-mesh calls early-return at `tessellation/mod.rs:231` into `tessellate_solid_bounded` (`*_params=None && !is_polygon_soup && !has_arcs`).

Re-exploration of `tessellate_solid_bounded` confirmed it builds a SHARED `EdgeDiscretization { positions: Vec<[f64;3]>, edge_verts: BTreeMap<EdgeIdx, Vec<usize>> }` ONCE at L4170 via `discretize_edges` (`tessellation/mod.rs:3128-3215`). All sub-helpers (`tessellate_cylindrical_face_bounded`, `tessellate_planar_face_bounded`, the fallback at L4224-4261) consume from this pool by EdgeIdx lookup, NOT by hash-of-position. **No independent re-discretization, no per-face byte-identity drift at shared edges by construction.** This invalidates the PR14 "per-face byte-identity" hypothesis as the LIKELY mechanism for this code path, and re-frames the hypothesis: per-face tessellation is correct; triangle loss happens in REPAIR stages. `remove_nonmanifold_duplicates_aggressive` is the most likely culprit per its docstring at L4327-4329 ("no fill triangles so all removals target real face overlaps from adjacent tessellations"; 10-pass iteration; "no safety checks").

**A15.6 cross-domain flag:** `tessellation::` is architecturally outside Yang Boolean scope per `governance/ARCHITECTURAL_INVARIANTS.md` A15.6 (pipeline ends at B-Rep assembly step 7; render LOD downstream). Phase 0 is observation-only inside `tessellation::`. PR-Y15c-fix-N WILL require cross-domain coordination (deferred).

## 3. What PR-Y15c Phase 0 + the v1 aborted attempt established

- 10/10 F0031–F0040 fire row 1 at Stage E_lod=Render (`well_formed=false`); operand Stage E_lod=Adaptive 20/20 well_formed=true; per-case tri loss spans −12 to −48; named anchor narrowed to `tessellation::tessellate_solid_ext_with_lod` at LOD=Render.
- v1's L792 canary fired 0 times → cheaper-proxy site refuted.
- Implementer-g's dispatch trace pinned the actual result-mesh path: all 10 calls early-return at L231 into `tessellate_solid_bounded`.
- Adversary-3's §4.4 recommendation was inferred from PR-Y15c Stage E data, not verified by their own probe runs (per `feedback_oracle_credibility_via_role_separation.md`).

## 4. Phase 0 instrumentation requirements

### 4.1 Reuse `YANG_CONFORMAL_PROBE=1`; tag `[stage-f]`

No new env var. Tag `[stage-f]` distinguishes from `[conformal-probe]` (Stage A/Bb/B/C/E) and from v1's `[weld-probe]` lineage.

### 4.2 Anchor pre-verification canary

Per `feedback_anchor_before_fix.md` strategic-escalation rule. Insert at `tessellation/mod.rs:4180` (after `disc` built, before the per-face dispatch loop) BEFORE writing real probes:

```rust
eprintln!("[stage-f-canary] reached tessellate_solid_bounded face_count={}", sorted_faces.len());
```

Run F0031 + F0040 with `YANG_CONFORMAL_PROBE=1`. Confirm canary fires for every result-mesh call (≈10 fires total for `batch_enclosed_subtract_fix`, F0031–F0040). **ABORT-if-zero-fires per ENGINEERING_CONSTITUTION P10.** Implementer-g's dispatch trace already proved the function IS reached, so this is a formality — but the discipline is non-negotiable. Remove canary BEFORE the real probes land.

### 4.3 Stage F probe family (5 sites; ~25 LOC additive)

Insert 5 probes inside `tessellate_solid_bounded`. Each probe is gated on `YANG_CONFORMAL_PROBE=1`:

```rust
if std::env::var("YANG_CONFORMAL_PROBE").as_deref() == Ok("1") {
    let unpaired = repair::count_unpaired_in_mesh(&vertices, &indices);
    let tri_count = indices.len() / 3;
    eprintln!("[stage-f] sub=N tri_count={tri_count} unpaired={unpaired}");
}
```

Probe sites and `sub=N` assignments:

| sub | Site | After call/state | Approx line |
|---|---|---|---|
| 0 | F.0 baseline | Per-face dispatch loop completes; `fix_winding_consistency` not yet called | ~L4271 (just after the `for &(kid, face_idx) in &sorted_faces` loop closes) |
| 1 | F.1 | After `remove_winding_insensitive_duplicates` | ~L4284 |
| 2 | F.2 | After `remove_nonmanifold_topology_aware` | ~L4326 |
| 3 | F.3 | After `remove_nonmanifold_duplicates_aggressive` (**most likely culprit**) | ~L4331 |
| 4 | F.4 | After `weld_smooth_vertices` (just before `Ok(RenderMesh { ... })`) | ~L4339 |

Reuses `repair::count_unpaired_in_mesh` at `tessellation/repair.rs:81-124` (already `pub(super)`; callable from parent module — no visibility change required).

**Risk #5 documentation requirement.** `count_unpaired_in_mesh` quantizes at `TAU_TESS_GRID_FACTOR`; `weld_smooth_vertices` may quantize at a different scale. The two scales may disagree on what counts as "shared", confounding the F.3 → F.4 reading. Diagnostic memo MUST document any counter-intuitive F.3 → F.4 deltas (e.g., tri_count dropping AND unpaired dropping simultaneously, or unpaired increasing post-weld).

### 4.4 Reproducer harness

`batch_enclosed_subtract_fix` at `crates/test-harness/tests/assay_randomized.rs:445`. F0031 + F0040 spot-check pair (operand-order coverage); full F0031–F0040 batch for cluster homogeneity. Expect 5 `[stage-f]` lines per result-mesh call (= 50 total for the batch). Filter to result-mesh calls only.

### 4.5 libtest `--nocapture` quirk

```
YANG_CONFORMAL_PROBE=1 YANG_BOOLEAN=1 \
  cargo test -p test-harness --test assay_randomized --release -- \
  batch_enclosed_subtract_fix --ignored --nocapture --test-threads=1 \
  2>stderr_capture 1>stdout_capture
```

`--test-threads=1` and separated streams MANDATORY (PR-Y15a/Y15c precedent).

## 5. Decision tree (5 rows)

| Stage where `tri_count` drops by ≥12 | Anchor | Next PR |
|---|---|---|
| F.0 → F.1 | `remove_winding_insensitive_duplicates` over-removes | PR-Y15c-fix-1: tri_key dedup logic at `repair.rs:502-574`. |
| F.1 → F.2 | `remove_nonmanifold_topology_aware` over-removes | PR-Y15c-fix-2: B-Rep topology-aware logic at `repair.rs:585-828`. |
| F.2 → F.3 | `remove_nonmanifold_duplicates_aggressive` over-removes (**most likely per docstring**) | PR-Y15c-fix-3: aggressive 10-pass removal at `repair.rs:1870-2154`. |
| F.3 → F.4 | `weld_smooth_vertices` collapses verts wrong | PR-Y15c-fix-4: weld_smooth normal-grid dedup at `tessellation/mod.rs:4096-4158`. |
| No stage drops by ≥12 (sum < 12) | Loss happens earlier (per-face dispatch is producing wrong tris from the start) OR PR14 hypothesis IS the mechanism after all (per-face emission does not honor shared `disc.positions`) | Hits wrong-anchor count #2. Spec PR-Y15c-fix-Phase0-v3 instrumenting per-face dispatch (5 sites in cylindrical/planar helpers). At wrong-anchor #3, escalate to reference comparison per `feedback_external_coherence.md`. |

## 6. FIP role assignments

Per `governance/FEATURE_IMPLEMENTATION_PROTOCOL.md` §3.2 (spec writer ≠ implementer ≠ adversary):

| Sub-phase | Agent | Writes |
|---|---|---|
| 0a Spec | spec-writer-f (NEW agent) | `specs/yang_pr_y15c_fix_phase0_stage_f_repair.md` (this file) |
| 0b Implement | implementer-h (NEW; NOT spec-writer-f; NOT implementer-g) | Probe code + canary + diagnostic memo |
| 0c Adversary | adversary-5 (NEW; **NOT adversary-3** per `feedback_oracle_credibility_via_role_separation.md`) | `docs/audits/pr_y15c_fix_phase0_validation.md` |
| 0d Commit | team-lead | Memory updates, git commit |

**adversary-5 role rationale (load-bearing).** Adversary-3 made the v1 cheaper-proxy recommendation that was empirically refuted by implementer-g's canary. Per `feedback_oracle_credibility_via_role_separation.md`, their judgment is compromised on this anchor lineage and they are not assigned to v2. **adversary-5 directive (verbatim, MUST be honored): DO NOT recommend a next-layer cheaper proxy without running it yourself FIRST.** That was the failure mode that cost PR-Y15c-fix-attempt-1.

## 7. Out of scope

- Fix code (PR-Y15c-fix-N follows ONLY after Phase 0 names the dropper).
- Probes inside the bodies of the 4 removal functions (deferred; v3 territory).
- Probes in per-face dispatch helpers (`tessellate_cylindrical_face_bounded` / `tessellate_planar_face_bounded`) — deferred to v3 if Stage F doesn't localize cleanly.
- R-class cases (separate cohort).
- PR-Y15b.1 follow-ups; TSV re-segmentation; R0071 kernel hang; S-H clipping removal.
- WASM rebuild (probe is env-gated, default-off).
- Cherchi 2022 reference comparison: NOT AVAILABLE for this stage (Cherchi has no render-LOD step; Waffle-specific). Phase 0 must rely on internal canary discipline + multi-stage probe (per `feedback_multi_stage_anchor_probe.md`) instead of cross-impl parity.
- Cross-domain A15.6 coordination for the eventual fix.
- Changing any of the 4 repair functions' bodies (read-only this Phase).
- Re-investigating PR14's per-face byte-identity hypothesis (now considered unlikely given shared `EdgeDiscretization` pool — would only revisit if Stage F shows F.0 baseline already broken per row 5).

## 8. Phase 0 deliverable checklist

Implementer-h SHALL produce:

1. **Anchor canary** at `tessellation/mod.rs:4180` (1 LOC). Run F0031 + F0040 with `YANG_CONFORMAL_PROBE=1`; verify canary fires per result-mesh call. **ABORT if 0 fires per P10.** Remove canary BEFORE the real probes land.
2. **Stage F probe family** at `tessellation/mod.rs` 5 sites (~25 LOC additive), env-gated on `YANG_CONFORMAL_PROBE=1`, tagged `[stage-f]`, each emitting `sub=N tri_count={} unpaired={}`. No visibility changes.
3. `docs/audits/pr_y15c_fix_phase0_diagnostic.md` (~150 LOC):
   - Verbatim Stage F probe output for F0031 + F0040 (5 lines per case).
   - Cluster-homogeneity table for full F0031–F0040 batch (per-stage tri_count delta + final unpaired).
   - Decision-tree row determination per spec §5.
   - Named anchor function (file + line range) — even if "anchor unknown — escalate to v3".
   - **Reconciliation (load-bearing):** `tri_drop` summed across F.0 → F.4 MUST match Stage E delta from PR-Y15c (F0031: −12; F0040: −44). If sum < 12: row 5 fires; loss starts BEFORE F.0; escalate to v3. If sum > 12: a stage drops more than necessary AND a later stage restores (unlikely — document if seen).
   - Risk #5 documentation per §4.3 if counter-intuitive F.3 → F.4 readings surface.
   - Spec ambiguities encountered.
4. **Production safety verification per DoD §6:**
   - `YANG_CONFORMAL_PROBE` unset → 0 `[stage-f]` lines, 0 `[stage-f-canary]` lines, F0002 `yang_trace_f0002` test passes byte-identical, results.json pass/fail counts unchanged at 11/179.
   - `cargo clippy -p kernel --no-deps`: net delta MUST be 0 (PR-Y15c baseline cited as 91; implementer-f observed 92 — flag any drift).
   - `rustfmt --check` only on `tessellation/mod.rs` (NO `cargo fmt -p kernel` per fmt-cascade lesson).
5. Anchor canary removed before final probe code lands (verify by re-grep on probe-on rerun: 0 `[stage-f-canary]` hits).
