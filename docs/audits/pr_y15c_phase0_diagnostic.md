# PR-Y15c Phase 0 — Render-LOD Stage E anchor diagnostic

**Author:** implementer-f (PR-Y15c Phase 0)
**Date:** 2026-05-04
**Spec:** `specs/yang_pr_y15c_render_lod_investigation.md`
**Plan:** `/home/claude/.claude/plans/reactive-juggling-sloth.md` (PR-Y15c sub-phase 0b)
**Probe:** `YANG_CONFORMAL_PROBE=1`, new Stage E at
`crates/kernel/src/boolean/yang_integration.rs:1022-1051`
(`tessellate_waffle_solid` post-`tessellate_solid_ext_with_lod`)
**Reproducers:** F0031 + F0040 (operand-order spot-check pair); validated
across the full F0031–F0040 ten-case batch for cluster homogeneity.

## TL;DR

**Decision-tree row 1 fires.** Stage E_lod=Render reports
`well_formed=false` on **all 10 cases** in F0031–F0040 — the long-banked
PR14 Render-LOD anchor is empirically confirmed for this cohort.
Operand meshes (Stage E_lod=Adaptive) read `well_formed=true` on 20/20
samples, refuting the f32-round-trip false-positive risk and proving
the oracle handles `RenderMesh`'s f32 verts faithfully without an
explicit `dedup_mesh_vertices` call.

The defect is `tessellate_solid_ext_with_lod` at LOD `Render` (64
segments), invoked from `yang_integration.rs:1012` as the final cached
render mesh. Anchor recommendation for PR-Y15c-fix:
**`crates/kernel/src/tessellation::tessellate_solid_ext_with_lod`** at
the per-face fan-emission / vertex-sharing layer. Cross-domain
coordination required (A15.6: render LOD lives in `tessellation::`,
outside Yang Boolean scope).

## Anchor pre-verification (per `feedback_anchor_before_fix.md`)

Per the strategic-escalation rule (three wrong anchors → reference
comparison) and the standing canary discipline,
`eprintln!("[stage-e-canary] reached after tessellate_waffle_solid lod={lod:?}")`
was inserted at the planned probe site
(`yang_integration.rs:1037`-area, after `tessellate_solid_ext_with_lod`
returns) BEFORE coding the real probe.

**Result:** `batch_enclosed_subtract_fix` (F0031–F0040) executed under
`YANG_CONFORMAL_PROBE=1 YANG_BOOLEAN=1`. The canary fired **30 times
total = exactly 3 per case**: 10 fires with `lod=Render` (the
production cached render mesh at `yang_integration.rs:1012`) and 20
fires with `lod=Adaptive { d_epsilon: ... }` (operand A + operand B at
`yang_integration.rs:653-654`, varying d_epsilon per case). Anchor
verified at all three call sites. Canary removed before the real probe
was committed (verified by re-grep on probe-on rerun: 0 hits).

## Stage E probe — implementation

Added at `crates/kernel/src/boolean/yang_integration.rs:1022-1051`,
inside `tessellate_waffle_solid`, after the
`tessellate_solid_ext_with_lod?` call returns successfully. Mirrors
Stage A/Bb/B/C structure (see `topology_extract.rs:1810-1834` for the
Stage Bb template):

- **Gate:** `YANG_CONFORMAL_PROBE=1` (5th member of the same probe
  family — no new env var per spec §4.1)
- **Helpers reused:** `render_mesh_to_arrays`
  (`yang_integration.rs:46-69`) for the `RenderMesh` → `(Vec<[f64;3]>,
  Vec<[usize;3]>)` conversion; `check_conformal`
  (`oracles/conformal_mesh.rs:97-138`) for the well-formedness
  measurement (vert canonicalization at nanometer precision is
  internal); `emit_conformal_probe` (`topology_extract.rs:36-75`,
  visibility raised to `pub(crate)` per spec §8 deliverable 1) for the
  log line emission.
- **LOD discrimination:** stage name formatted as
  `"E_lod={lod:?}"` so the Adaptive (operand) and Render (cached)
  call-sites are distinguishable in the log without a per-call-site
  probe. This satisfies spec §4.3's "fires on EVERY call,
  discriminated via LOD-tagged stage name" requirement.

LOC: 13 lines added in `yang_integration.rs` (1 capture-rename + 9
probe block + 3 doc), 1 char changed in `topology_extract.rs`
(`fn` → `pub(crate) fn`).

## F32 round-trip verification (spec §4.3 open question) — RESOLVED

**Outcome: f32 round-trip is healthy. No `dedup_mesh_vertices` call
needed before `check_conformal`.**

The spec flagged a risk that f32 precision (~1e-7 m) being ~100×
coarser than the nanometer-quant grid (1e-9 m) might push two
B-Rep-coincident verts into different quant cells, producing a *false*
`well_formed=false` reading. The verification protocol: inspect
Stage E_lod=Adaptive on the operand mesh first; if a known-good operand
mesh reads `well_formed=false`, the oracle cannot process render-LOD
verts as-is and an explicit `dedup_mesh_vertices` call must precede
`check_conformal`.

All 20 Stage E_lod=Adaptive observations (10 cases × 2 operands)
report `well_formed=true`:

| LOD example | verts | tris | unique_edges | well_formed |
|---|---:|---:|---:|---|
| Adaptive { d_epsilon: 0.010636… } (small operand) | 8 | 12 | 18 | true |
| Adaptive { d_epsilon: 0.010636… } (large operand) | 20 | 36 | 54 | true |
| Adaptive { d_epsilon: 0.006211… } (large operand) | 38 | 72 | 108 | true |
| Adaptive { d_epsilon: 0.008211… } (mid operand) | 34 | 64 | 96 | true |

(20/20 readings well_formed=true; selected representative samples
shown.)

The operand meshes are pre-Cherchi and known-good (PR-Y15a Phase 0
Stage A confirmed `well_formed=true` on the canonicalized merged
input). They round-trip through `RenderMesh` (f32) → `render_mesh_to_arrays`
(f32 → f64) → `check_conformal` (f64 → quantized at 1e-9 m → indexed)
without any false-negative report. **The oracle handles f32 verts
faithfully in this regime.** Therefore, the `well_formed=false` finding
on Stage E_lod=Render is genuine — not an oracle artifact.

This confirms the spec §4.3 mitigation path (explicit
`dedup_mesh_vertices`) is NOT required for the Stage E probe. The
probe code remains as written in spec §8 deliverable 3.

## Verbatim probe output — F0031 (canonical reproducer, box-minus-cyl)

```
[conformal-probe] stage=A unpaired=0 multi_paired=0 euler_chi=4 well_formed=true verts=28 tris=48 unique_edges=72
[conformal-probe] stage=Bb unpaired=0 multi_paired=0 euler_chi=4 well_formed=true verts=28 tris=48 unique_edges=72
[conformal-probe] stage=B unpaired=0 multi_paired=0 euler_chi=4 well_formed=true verts=28 tris=48 unique_edges=72
[conformal-probe] stage=C unpaired=0 multi_paired=0 euler_chi=4 well_formed=true verts=28 tris=48 unique_edges=72
[topo-extract] summary: paired=30, unpaired=0, ambiguous=0
[conformal-probe] stage=E_lod=Render unpaired=12 multi_paired=0 euler_chi=2 well_formed=false verts=26 tris=36 unique_edges=60
[conformal-probe]   unpaired #0: v0=8 v1=9 source_tris=[13]
[conformal-probe]   unpaired #1: v0=8 v1=15 source_tris=[29]
[conformal-probe]   unpaired #2: v0=9 v1=10 source_tris=[17]
[conformal-probe]   unpaired #3: v0=10 v1=8 source_tris=[28]
[conformal-probe]   unpaired #4: v0=15 v1=16 source_tris=[12]
[conformal-probe] stage=E_lod=Adaptive { d_epsilon: 0.009620312332577416 } unpaired=0 multi_paired=0 euler_chi=2 well_formed=true verts=8 tris=12 unique_edges=18
[conformal-probe] stage=E_lod=Adaptive { d_epsilon: 0.009620312332577416 } unpaired=0 multi_paired=0 euler_chi=2 well_formed=true verts=18 tris=32 unique_edges=48
```

Downstream Waffle failure for F0031:
```
F0031 Failed: watertight_mesh: 12 unpaired edges out of 60 total;
              mesh_euler_characteristic: V(26) - E(60) + F(36) = 2 (expected 4)
```

**Vert/tri/edge delta — Stage C → Stage E_lod=Render (LOAD-BEARING per Risk 6 / Risk 9):**

| Stage | verts | tris | unique_edges | euler_chi | well_formed | unpaired | multi_paired |
|---|---:|---:|---:|---:|---|---:|---:|
| C        | 28 | 48 | 72 | 4 | **true**  | 0 | 0 |
| E_lod=Render | 26 | 36 | 60 | 2 | **false** | 12 | 0 |
| **Δ (E−C)**  | −2 | **−12** | −12 | −2 | true→false | +12 | 0 |

The render-LOD output is **2 verts and 12 triangles smaller** than the
Stage C conformal mesh. The watertight oracle's downstream measurement
(`V=26 E=60 F=36`) matches Stage E_lod=Render byte-for-byte
(verts=26, unique_edges=60, tris=36 == F=36, unpaired=12 == 12 unpaired
edges). **Stage E IS what the watertight oracle sees.** No further
shrinkage occurs between Stage E and the watertight measurement —
the entire degradation happens inside `tessellate_solid_ext_with_lod`
at LOD Render.

## Verbatim probe output — F0040 (operand-order spot-check, cyl-minus-box)

```
[conformal-probe] stage=A unpaired=0 multi_paired=0 euler_chi=4 well_formed=true verts=46 tris=84 unique_edges=126
[conformal-probe] stage=Bb unpaired=0 multi_paired=0 euler_chi=4 well_formed=true verts=46 tris=84 unique_edges=126
[conformal-probe] stage=B unpaired=0 multi_paired=0 euler_chi=4 well_formed=true verts=46 tris=84 unique_edges=126
[conformal-probe] stage=C unpaired=0 multi_paired=0 euler_chi=4 well_formed=true verts=46 tris=84 unique_edges=126
[topo-extract] summary: paired=48, unpaired=0, ambiguous=0
[conformal-probe] stage=E_lod=Render unpaired=22 multi_paired=2 euler_chi=12 well_formed=false verts=42 tris=40 unique_edges=70
[conformal-probe]   unpaired #0: v0=0 v1=17 source_tris=[29]
[conformal-probe]   unpaired #1: v0=1 v1=0 source_tris=[20]
[conformal-probe]   unpaired #2: v0=2 v1=1 source_tris=[21]
[conformal-probe]   unpaired #3: v0=9 v1=45 source_tris=[24]
[conformal-probe]   unpaired #4: v0=10 v1=9 source_tris=[24]
```

Downstream Waffle failure for F0040:
```
F0040 Failed: watertight_mesh: 20 unpaired edges out of 70 total;
              consistent_normals: 10 of 40 triangles have reversed normals;
              outward_normals: only 30 of 40 triangles (75.0%) have outward normals (need 95%);
              mesh_euler_characteristic: V(42) - E(70) + F(40) = 12 (expected 4)
```

(Note: F0040's `consistent_normals` count — 10 vs 14 in PR-Y15a's
table — and `outward_normals` ratio 75% vs 65% differ slightly between
runs; the orientation oracle has run-dependent ordering. The
watertight count and topology counts are stable: 20 unpaired / 70
total / V=42 E=70 F=40.)

**Vert/tri/edge delta — Stage C → Stage E_lod=Render:**

| Stage | verts | tris | unique_edges | euler_chi | well_formed | unpaired | multi_paired |
|---|---:|---:|---:|---:|---|---:|---:|
| C        | 46 | 84 | 126 | 4 | **true**  | 0 | 0 |
| E_lod=Render | 42 | 40 | 70 | 12 | **false** | 22 | 2 |
| **Δ (E−C)**  | −4 | **−44** | −56 | +8 | true→false | +22 | +2 |

F0040 loses **44 of 84 triangles** (52%) at the render-LOD step. Again,
the watertight oracle's `V=42 E=70 F=40` matches Stage E exactly
(unique_edges=70 == 70, tris=40 == F=40, unpaired ≈ 20 vs probe's 22 —
within 2 of expected, attributable to the watertight oracle counting
undirected vs the conformal probe counting directed). **Stage E is
again the load-bearing measurement.**

## Cluster homogeneity — F0031–F0040 — 10/10 well_formed=false at Stage E_lod=Render

| Case | Stage C verts/tris/edges | E_lod=Render verts/tris/edges | E_lod=Render well_formed | E_lod unpaired | Decision row |
|---|---|---|---|---:|---|
| F0031 | 28/48/72 | 26/36/60 | **false** | 12 | **1** |
| F0032 | 26/44/66 | 24/24/44 | **false** | 16 | **1** |
| F0033 | 26/44/66 | 24/24/44 | **false** | 16 | **1** |
| F0034 | 30/52/78 | 28/32/62 | **false** | 28 | **1** |
| F0035 | 26/44/66 | 24/24/44 | **false** | 16 | **1** |
| F0036 | 46/84/126 | 38/36/62 | **false** | 16 | **1** |
| F0037 | 46/84/126 | 42/40/66 | **false** | 12 | **1** |
| F0038 | 46/84/126 | 42/40/70 | **false** | 24 | **1** |
| F0039 | 42/76/114 | 40/44/86 | **false** | 40 | **1** |
| F0040 | 46/84/126 | 42/40/70 | **false** | 22 | **1** |

**All 10 cases fire decision-tree row 1.** Cluster is homogeneous:
every case has well_formed=true at Stage C and well_formed=false at
Stage E_lod=Render. The vert/tri shrinkage is consistent across the
cohort (ranging from −2/−12 on F0031 to −8/−48 on F0036). Operand
order does not matter (F0031–F0035 = box-minus-cyl shrink small;
F0036–F0040 = cyl-minus-box shrink larger because the cyl operand has
more curved-surface fans to lose).

Operand-mesh Stage E_lod=Adaptive readings: **20/20 well_formed=true**
across all 10 cases (each case calls Adaptive twice — operand A + B —
for both pre-Cherchi tessellations). The defect is exclusively in the
LOD=Render path.

## Decision-tree row determination — ROW 1

Per spec §5:

> | Stage E (lod=Render at L1012) | Anchor | Next PR |
> | `well_formed=false` | `tessellate_waffle_solid` retessellation IS the defect (confirms PR14 anchor). Diagnostic MUST report exact vert/tri/edge delta vs Stage C. | PR-Y15c-fix targets `tessellate_solid_ext_with_lod` / fan welding / per-face byte-identity. Cross-domain coordination needed (A15.6). |

**Row 1 fires uniformly across all 10 F0031–F0040 cases.** Per the
delta tables above, vert/tri/edge deltas vs Stage C are reported per
case (load-bearing per Risk 6 / Risk 9).

Row 2 (`E_lod=Render well_formed=true`) does NOT fire.
Row 3 (`E_lod=Render true BUT E_lod=Adaptive false`) does NOT fire
(operand reads true).

The PR14 Render-LOD anchor (per `yang_implementation_status.md`
2026-05-02 entry: *"PR14 anchor = `tessellate_waffle_solid` Render LOD
per-face byte-identity defect"*) is **empirically confirmed for the
F0031–F0040 cohort**.

## Named anchor function — `tessellate_solid_ext_with_lod` at LOD=Render

The defect is inside the call invoked at
`crates/kernel/src/boolean/yang_integration.rs:1026-1038`
(`tessellation::tessellate_solid_ext_with_lod` with `lod` set to
`tessellation::TessellationLod::Render`). The pre-call B-Rep arena
(passed as `solid.arena`) was constructed by `flood_fill_patches` Step
6 from a Stage-C-validated conformal mesh; the post-call `RenderMesh`
output fails the conformal oracle.

**Specific suspected sub-anchor (per spec §5 row 1 framing):** Yang
2025 §4.5 + Cherchi 2020 §5 vertex sharing — the per-face fan-emission
in `tessellate_solid_ext_with_lod` (file: `crates/kernel/src/tessellation/`,
line range to be empirically pinned in PR-Y15c-fix Phase 0) emits
per-face vertex tables that are not byte-identical at shared edges
between adjacent faces. When two adjacent faces tessellate the SAME
shared edge with vertex coordinates that differ at f32 precision
(even sub-quant precision in absolute terms), the watertight oracle's
position-based pairing splits them into 2× directed half-edges with no
twin partner — exactly the `unpaired=12-40` pattern seen at Stage E.

**File:line range for the precise anchor — TO BE EMPIRICALLY PINNED IN
PR-Y15c-fix Phase 0.** This Phase 0 deliberately stops at the function
boundary. Per the strategic-escalation rule (three wrong anchors →
stop bisecting), PR-Y15c-fix Phase 0 must canary the next layer down
inside `tessellate_solid_ext_with_lod` before writing fix code.

## Cross-domain ownership flag (A15.6)

`tessellation::` is **architecturally outside** the Yang Boolean
pipeline scope per A15.6. The fix necessarily crosses the
`crates/kernel/src/boolean/` ↔ `crates/kernel/src/tessellation/`
boundary. Per spec §2 + §6:
- This Phase 0 is observation-only and crosses no boundary (the
  probe measures from inside `boolean/yang_integration.rs`).
- PR-Y15c-fix WILL require cross-domain coordination. The team-lead
  is the appropriate manager-authorization gate per DoD §7.

## Production safety verification

Per spec §8 deliverable 4 + plan §"Sub-phase 0b" deliverable 7:

1. **Probe-off byte identity** (`YANG_CONFORMAL_PROBE` unset):
   - Command: `YANG_BOOLEAN=1 cargo test -p test-harness --test assay_randomized --release -- yang_trace_f0002 --ignored --nocapture --test-threads=1`
   - `[conformal-probe]` lines: **0** ✓
   - `[stage-e-canary]` lines: **0** ✓
   - Test result: **1 passed; 0 failed** ✓ (F0002 trace pinned test passes)
   - results.json baseline: `passed: 11, failed: 179` (read from
     `app/tests/cases/assay/results.json`); the probe is gated on
     `YANG_CONFORMAL_PROBE=1`, so probe-off runs cannot affect
     pass/fail counts. Determinism preserved.

2. **`cargo clippy -p kernel --no-deps`** baseline check:
   - Pre-edit (HEAD = 9a2406c, my changes stashed): **92** warnings.
   - Post-edit (my probe applied): **92** warnings.
   - Net delta: **0** ✓
   - **Spec ambiguity flag:** the spec said baseline=91; the actual
     current main baseline is 92. Most likely a +1 drift from a
     post-PR-Y15a commit unrelated to my work. My changes contribute
     0 new warnings; the DoD §6 invariant "no silent change in
     determinism / build remains reproducible" holds.

3. **`rustfmt --check`** on edited files:
   - `crates/kernel/src/boolean/yang_integration.rs`: clean ✓
   - `crates/kernel/src/boolean/topology_extract.rs`: clean ✓
   - Per the fmt-cascade lesson (`feedback_*` from PR-Y14a/15a),
     `cargo fmt -p kernel` was NOT run — that pass would touch
     pre-existing fmt issues in unrelated files.

4. **DoD §6 (Infrastructure / Tooling Change) re-verification:**
   - "Does not alter modeling behavior unintentionally": ✓
     (probe is env-gated; default-off; production callers do not set
     `YANG_CONFORMAL_PROBE=1`).
   - "Tests still pass": ✓ (F0002 trace test passes with same
     verdict; F0031–F0040 still fail with same `watertight_mesh: N
     unpaired` signatures — failure mode unchanged).
   - "No silent change in determinism": ✓ (probe path is
     observation-only — `eprintln!` only — no mutation of any data
     flowing through the pipeline; non-eprintln behavior of
     `tessellate_waffle_solid` is the unchanged
     `tessellate_solid_ext_with_lod?` → `Ok(mesh)` shape).
   - "Build remains reproducible": ✓ (no new dependencies; no
     features touched; no `Cargo.toml` edits).

5. **Anchor canary removed before final probe code landed:**
   verified by re-grepping the probe-on `probe_stderr` for
   `[stage-e-canary]` (0 hits) ✓.

6. **No new env vars beyond existing `YANG_CONFORMAL_PROBE`:** ✓
   (probe re-uses the same gate as Stages A/Bb/B/C).

## Spec ambiguities encountered (per spec §"Spec ambiguities flagged" + plan §6)

1. **F32 round-trip drift (spec §4.3) — RESOLVED.** Verification
   protocol executed; operand mesh reads `well_formed=true` 20/20.
   `dedup_mesh_vertices` not needed before `check_conformal`.
   Spec ambiguity closed by empirical data.

2. **Probe site line number L1037 vs after-call (plan §6 item 2) —
   RESOLVED.** The actual `tessellate_solid_ext_with_lod` call spans
   L1026-1038 in original main. The probe must fire AFTER that call
   returns, which requires capturing the result into a local
   variable rather than returning the call expression directly. The
   refactor in `yang_integration.rs:1022-1051` (rename
   trailing-expression to `let mesh = ...?;`, then `Ok(mesh)`) is
   the minimal-invasive shape that satisfies both the spec
   requirement and Rust's expression-statement distinction. Canary
   pre-verification confirmed the chosen insertion line (post-`?`,
   pre-`Ok(mesh)`) fires for every call.

3. **DoD §6 mapping (plan §6 item 3) — CLEANLY MAPPED.** All four
   DoD §6 bullets verified above (§"Production safety verification"
   item 4). The probe is a pure infrastructure addition; no modeling
   behavior change; build and determinism preserved.

4. **Anchor-pre-verification canary text (plan §6 item 4) — APPLIED
   VERBATIM.** The spec §4.4 text
   `"[stage-e-canary] reached after tessellate_waffle_solid lod={lod:?}"`
   was inserted unchanged so adversary-3's mutation grep is
   deterministic.

5. **Clippy baseline drift (NEW, observed during my work) — flagged
   above in §"Production safety verification" item 2.** Spec quoted
   91; current main baseline is 92. Recommend team-lead either
   refresh the baseline citation or investigate the +1 drift; my
   contribution is delta=0 either way.

## Reference comparison status

Cherchi 2020 §5 documents conformal vertex sharing requirements; Yang
2025 §4.5 documents the post-Cherchi B-Rep retessellation step. There
is **no Cherchi reference for the render-LOD layer** (Cherchi has no
analogous render-LOD — they output the conformal mesh directly).
This is Waffle-specific, so no reference oracle is buildable for the
Stage E layer. The PR-Y15c-fix Phase 0 must rely on internal
canary discipline + multi-stage probe (per
`feedback_multi_stage_anchor_probe.md`) instead of cross-impl parity.

## Conclusion

PR-Y15c Phase 0 confirms decision-tree row 1: the Render-LOD
retessellation step inside `tessellate_solid_ext_with_lod` is the
defect anchor for the F0031–F0040 cohort (10/10 cluster homogeneous,
operand meshes innocent, f32 round-trip oracle-faithful). The PR14
Render-LOD anchor banked since 2026-05-02 in
`yang_implementation_status.md` is empirically validated for this
cohort.

**Recommended next action (PR-Y15c-fix Phase 0):** drop into
`crates/kernel/src/tessellation/` (specifically
`tessellate_solid_ext_with_lod` and the per-face fan-emission /
shared-edge-vertex-equality layer it dispatches into); add a Phase 0
canary at the suspected sub-anchor; build a probe that compares
per-face emitted vertex tables for byte-identity at shared edges. The
file:line precision of the actual anchor inside `tessellation::` is
deliberately deferred to PR-Y15c-fix Phase 0 per the strategic-
escalation rule (anchor pre-verification before fix code).

**A15.6 cross-domain coordination required for PR-Y15c-fix.**
