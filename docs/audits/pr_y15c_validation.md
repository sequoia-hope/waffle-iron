# PR-Y15c — Phase 0 Validation

**Author:** adversary-3 (PR-Y15c Phase 0)
**Date:** 2026-05-04
**Spec:** `specs/yang_pr_y15c_render_lod_investigation.md`
**Phase 0 diagnostic:** `docs/audits/pr_y15c_phase0_diagnostic.md`
**Stage E probe site:** `crates/kernel/src/boolean/yang_integration.rs:1042-1046`
(post-`tessellate_solid_ext_with_lod`; LOD-tagged via `format!("E_lod={lod:?}")`)

## Verdict

**ACCEPT** — implementer-f's decision-tree row 1 attribution is empirically
airtight on the F0031–F0040 cohort. All 10 cases are homogeneous on the
Stage E_lod=Render axis (well_formed=false, 10/10), all 20 operand readings
on Stage E_lod=Adaptive are well_formed=true (refuting f32-round-trip
false-positive risk), and the Stage E probe is mutation-confirmed
load-bearing on the row-attribution decision. The PR14 Render-LOD anchor
(banked since 2026-05-02) is empirically validated for this cohort, and
the named anchor `tessellation::tessellate_solid_ext_with_lod` is the
correct PR-Y15c-fix entry point.

**One clarifying finding (memo §4):** no cheaper-proxy probe site exists
for this cohort. `REVOLVE_DEBUG=1` was a candidate analog of `TWIN_DEBUG`
(per PR-Y15a §6) but produces **0 lines** for F0031–F0040 (the cohort
does not exercise the revolve fan path). PR-Y15c-fix Phase 0 cannot skip
its own canary cycle the way PR-Y15c skipped Stage D. However, an
existing primitive (`repair::count_unpaired_in_mesh` already imported
inside `tessellate_solid_ext`) makes the next-layer-down probe nearly
free to add at the right boundary (memo §4.3).

## §1. Decision-tree verdict per case

I re-ran F0031–F0040 with `TWIN_DEBUG=1 YANG_CONFORMAL_PROBE=1
YANG_BOOLEAN=1`, capturing all 120 probe lines (40 stage-A/Bb/B/C +
10 Render + 20 Adaptive + ≤50 detail lines on violation):

| Case | A | Bb | B | C | E_lod=Render | Waffle outcome | Row |
|---|---|---|---|---|---|---|---|
| F0031 | true | true | true | true | **false** (unp=12) | Failed (12 unpaired/60) | **1** |
| F0032 | true | true | true | true | **false** (unp=16) | Failed (16 unpaired/44) | **1** |
| F0033 | true | true | true | true | **false** (unp=16) | Failed (16 unpaired/44) | **1** |
| F0034 | true | true | true | true | **false** (unp=28) | Failed (28 unpaired/62) | **1** |
| F0035 | true | true | true | true | **false** (unp=16) | Failed (16 unpaired/44) | **1** |
| F0036 | true | true | true | true | **false** (unp=16) | Failed (16 unpaired/62) | **1** |
| F0037 | true | true | true | true | **false** (unp=12) | Failed (12 unpaired/66) | **1** |
| F0038 | true | true | true | true | **false** (unp=24, mp=4) | Failed (20 unpaired/70) | **1** |
| F0039 | true | true | true | true | **false** (unp=40) | Failed (40 unpaired/86) | **1** |
| F0040 | true | true | true | true | **false** (unp=22, mp=2) | Failed (20 unpaired/70) | **1** |

**All 10 cases fire decision-tree row 1.** Stage A/Bb/B/C uniformly
well_formed=true (40/40), Stage E_lod=Render uniformly well_formed=false
(10/10). Per spec §5 row 1 mandate: `tessellate_waffle_solid`
retessellation IS the defect. PR14 anchor empirically confirmed.

**F0038/F0040 directed-vs-undirected reconciliation:** Stage E reports
directed-edge `unpaired` plus a separate `multi_paired` count. The
watertight oracle counts undirected edges. F0038: 24 directed unpaired −
4 multi_paired = 20 undirected unpaired = Waffle's 20. F0040: 22 − 2 =
20 = Waffle's 20. **Stage E predicts the watertight reading exactly,
including the directed/undirected accounting wrinkle.**

## §2. Cluster homogeneity expansion — F0031–F0040

| Case | C verts/tris/edges | E_lod=Render verts/tris/edges | Δ verts | Δ tris | Δ edges | Decision row |
|---|---|---|---:|---:|---:|---|
| F0031 | 28/48/72 | 26/36/60 | −2 | −12 | −12 | **1** |
| F0032 | 26/44/66 | 24/24/44 | −2 | −20 | −22 | **1** |
| F0033 | 26/44/66 | 24/24/44 | −2 | −20 | −22 | **1** |
| F0034 | 30/52/78 | 28/32/62 | −2 | −20 | −16 | **1** |
| F0035 | 26/44/66 | 24/24/44 | −2 | −20 | −22 | **1** |
| F0036 | 46/84/126 | 38/36/62 | −8 | −48 | −64 | **1** |
| F0037 | 46/84/126 | 42/40/66 | −4 | −44 | −60 | **1** |
| F0038 | 46/84/126 | 42/40/70 | −4 | −44 | −56 | **1** |
| F0039 | 42/76/114 | 40/44/86 | −2 | −32 | −28 | **1** |
| F0040 | 46/84/126 | 42/40/70 | −4 | −44 | −56 | **1** |

**10/10 homogeneous on Stage E axis.** Per-case verts/tris/edges match
implementer-f's diagnostic memo's homogeneity table byte-for-byte. The
shrinkage is real (−2 to −48 triangles per case) and operand-order shows
expected asymmetry (F0031–F0035 small/box-minus-cyl shrink; F0036–F0040
larger/cyl-minus-box shrink, because the cyl-operand has more curved
fans). Operand-mesh Stage E_lod=Adaptive readings: **20/20
well_formed=true** across all 10 cases.

## §3. Mutation test — Stage E probe IS load-bearing

**Mutation:** Inserted `report.is_well_formed = false;` immediately after
the `check_conformal` call at `yang_integration.rs:1044` (renamed
binding to `let mut report` to permit assignment), forcing every Stage E
emission to report `well_formed=false` regardless of underlying topology.

**Result with mutation applied** (verbatim — first Adaptive operand
sample, F0031 small operand, KNOWN-GOOD per §5 below):

```
[conformal-probe] stage=E_lod=Adaptive { d_epsilon: 0.010636327205198413 } unpaired=0 multi_paired=0 euler_chi=2 well_formed=false verts=8 tris=12 unique_edges=18  ← FORCED
[conformal-probe] stage=E_lod=Adaptive { d_epsilon: 0.010636327205198413 } unpaired=0 multi_paired=0 euler_chi=2 well_formed=false verts=20 tris=36 unique_edges=54  ← FORCED
```

The Adaptive (operand) lines now report `well_formed=false` while
underlying counts (`unpaired=0 multi_paired=0`) say true. **The probe is
faithfully reading and emitting the value of `is_well_formed` — the field
flows through to the printed output.** Stage A/Bb/B/C unchanged
(separate probe sites with their own `check_conformal` calls; 40/40
still well_formed=true post-mutation).

If this mutation were left in place, every case in F0031–F0040 would
exhibit Stage E_lod=Adaptive=false simultaneously with Stage
E_lod=Render=false, which would route the anchor decision to spec §5
**row 3** ("Stage E_lod=Render true BUT E_lod=Adaptive false ⇒
operand-mesh tessellation broken pre-Cherchi; re-examine Stage A").
**The Stage E probe (specifically the Adaptive sub-readings) is
load-bearing on distinguishing row 1 from row 3.** Without the Adaptive
data, implementer-f could not have ruled out row 3.

**Mutation reverted** — see §8.

## §4. Alternative-probe-site refutation

Could a probe **inside** `tessellate_solid_ext_with_lod` give cheaper
or more granular signal than Stage E at the function exit? Three
candidates considered:

### §4.1 `REVOLVE_DEBUG=1` — REFUTED (does not fire on this cohort)

Existing instrumentation in `tessellation/mod.rs:1283-1746` (gated on
`REVOLVE_DEBUG=1`) emits per-triangle traces during `tessellate_revolve_lateral`
and `tessellate_revolve_cap_polygon`. PR-Y15a §6 used the analogous
`TWIN_DEBUG=1` to skip Stage D. By analogy, `REVOLVE_DEBUG=1` could be
the cheaper Stage E proxy — IF F0031–F0040 exercised the revolve path.

I ran `REVOLVE_DEBUG=1 YANG_BOOLEAN=1 cargo test ... batch_enclosed_subtract_fix
--ignored --nocapture --test-threads=1`. Result: **0 `[revolve-tess]`
lines emitted**. The F0031–F0040 cohort (cylinder-operand booleans) does
NOT exercise the revolve fan path. `REVOLVE_DEBUG` is the wrong fan
tessellator for this cohort — it instruments revolve sweeps, not the
trimmed-cylindrical-face fan path that runs for cyl-arc geometry.

### §4.2 Probe BEFORE `weld_shared_edge_vertices` (mod.rs:791) — ACCEPTED as PR-Y15c-fix Phase 0 candidate

`tessellate_solid_ext` has a clear pre-/post-weld boundary at L791-797:

```rust
if needs_fan_welding {
    weld_shared_edge_vertices(&vertices, &mut indices, &mut face_ranges);
    compact_unreferenced_vertices(&mut vertices, &mut normals, &mut indices);
}
Ok(RenderMesh { vertices, normals, indices, face_ranges })
```

A probe at L790 (pre-weld) and L797 (post-weld, pre-Ok) would split
two distinct hypotheses:
- **Hypothesis A:** per-face fan emission produces non-byte-identical
  shared-edge verts (pre-weld well_formed=false; weld is innocent).
- **Hypothesis B:** `weld_shared_edge_vertices` is over-aggressive or
  loses verts (pre-weld well_formed=true; post-weld false).

This is the natural PR-Y15c-fix Phase 0 anchor question and is **NOT
covered by any existing instrumentation**. I did NOT add this probe in
this validation cycle (out of scope per spec §7 — investigation only)
but flag it as the recommended canary site for PR-Y15c-fix Phase 0.

### §4.3 `count_unpaired_in_mesh` is already an in-scope cheap primitive

`tessellation/repair.rs:81` exposes `pub(super) fn count_unpaired_in_mesh(vertices:
&[f32], indices: &[u32]) -> usize`, already imported into
`tessellate_solid_ext` and called eight times during the convergence
loop (L606, L613, L699, L715, L731, L746, L763). PR-Y15c-fix Phase 0
can wrap this in an `eprintln!` gated on a new env var (or reuse
`YANG_CONFORMAL_PROBE`) at the §4.2 boundary at near-zero cost. This is
the analog of "cheaper proxy" for PR-Y15c-fix.

### §4.4 Verdict

No cheaper proxy exists FOR THE STAGE E QUESTION ITSELF; implementer-f's
Stage E probe at the `tessellate_waffle_solid` exit is the right call.
A cheaper-proxy chain CAN be assembled for PR-Y15c-fix Phase 0 (use
existing `count_unpaired_in_mesh` at the L791 pre-/post-weld boundary).
This refutation is therefore **partial accept**: Stage E is correct for
PR-Y15c, and the next layer's canary site is mapped for PR-Y15c-fix.

## §5. F32 round-trip independent confirmation

I verified the operand-mesh known-good baseline directly. All 20 Stage
E_lod=Adaptive readings (10 cases × 2 operands) report
**well_formed=true** in my run:

```
$ grep "stage=E_lod=Adaptive" /tmp/adv3_stderr | grep -c "well_formed=true"
20
$ grep "stage=E_lod=Adaptive" /tmp/adv3_stderr | grep -c "well_formed=false"
0
```

This independently confirms implementer-f's diagnostic memo §"F32
round-trip verification — RESOLVED": the f32 round-trip through
`render_mesh_to_arrays` (f32 → f64) → `check_conformal` (f64 →
nanometer-quantized) does NOT push B-Rep-coincident verts into different
quant cells in this regime. The `well_formed=false` finding on Stage
E_lod=Render is GENUINE — not an oracle artifact. **Explicit
`dedup_mesh_vertices` is correctly NOT added** before `check_conformal`.

Sample d_epsilon spread across the cohort (operand A + operand B per
case): values seen include 0.010636327205198413, 0.009620312332577416,
0.008210528..., 0.006210528... (matches implementer-f's table sample).

## §6. Re-segmentation insight

Implementer-f did not re-segment the cohort; PR-Y15a validation §5
already flagged the watertight-vs-twin-pair sub-cohort split as work
for PR-Y15c's spec writer. PR-Y15c collapses the sub-cohort question
because it explicitly scopes to **sub-cohort A only** (watertight-oracle
violators: F0031–F0040). Sub-cohort B (half-edge twin-pair violators in
R-cases) and sub-cohort C (orientation/normals failures) are out of
scope per spec §7.

**Insight to bank for PR-Y15c-fix:** F0036–F0040 additionally fail
`consistent_normals` (8–18 reversed of 36–40) and `outward_normals`
(55–77.8% outward, need 95%). These could be (a) a separate
winding-orientation defect in the LOD=Render fan emission, or (b) a
direct downstream consequence of the watertight defect (badly-paired
edges create undefined adjacency, which `fix_global_orientation` cannot
resolve). PR-Y15c-fix Phase 0 should distinguish; if (a), the per-face
fan-emission winding logic is in scope; if (b), fixing the watertight
defect should resolve normals automatically.

## §7. Verification deltas vs implementer-f's diagnostic

Three discrepancies found, all minor and non-load-bearing:

1. **F0031 Adaptive d_epsilon mis-quote.** Implementer-f's diagnostic
   §"Verbatim probe output — F0031" lists `d_epsilon: 0.009620312332577416`
   for the F0031 Adaptive operand readings. My run shows F0031 Adaptive
   uses `d_epsilon: 0.010636327205198413` (verts=8 tris=12 + verts=20
   tris=36). The 0.009620 value belongs to a different case in the
   batch (the values come in pairs as the case index advances). This
   is a verbatim-block mis-attribution by implementer-f, NOT a
   substantive error — Stage E_lod=Render readings (which drive the
   anchor decision) are accurate per case.

2. **F0040 verbatim sample.** Implementer-f's diagnostic shows F0040
   Stage E `unpaired=22 multi_paired=2`; my run confirms exactly these
   values. No discrepancy on the load-bearing data.

3. **Clippy baseline.** Implementer-f reports "92 warnings" pre-edit
   and post-edit; my fresh run on the implementer-f tree reports
   **91 warnings** (matches PR-Y15a baseline). Probably a difference
   in `Cargo.lock` or a build-cache effect. Net delta from
   implementer-f's edits = 0 either way (verified on my tree). This
   is a spec-ambiguity flag for team-lead, not a defect.

All other implementer-f data points (per-case Stage E verts/tris/edges,
operand 20/20 well_formed=true, downstream watertight oracle V/E/F)
match my independent run byte-for-byte.

## §8. Working-tree state

- **Mutation reverted.** I re-edited `yang_integration.rs:1042-1046`
  back to the implementer-f-shipped form (removed `mut` binding +
  `report.is_well_formed = false;` line).
- `git diff crates/kernel/src/boolean/yang_integration.rs
  crates/kernel/src/boolean/topology_extract.rs` against the
  implementer-f commit: **byte-clean**. The current diff shows only
  the original 1-LOC visibility change in `topology_extract.rs:36`
  (`fn` → `pub(crate) fn`) plus the original 13-LOC additive Stage E
  block in `yang_integration.rs:1023-1047`.
- `git diff --numstat`: `1 1 topology_extract.rs` and `11 2
  yang_integration.rs` — matches implementer-f's reported deltas.
- New deliverable file: `docs/audits/pr_y15c_validation.md` (this memo).
- No untracked source files added; `app/tests/cases/assay/results.json`
  not modified by my runs (probe is observation-only; failure mode
  unchanged at 11 passed / 179 failed per PR-Y15b post-fix).
- `cargo clippy -p kernel --no-deps` → **91 warnings** (matches
  PR-Y15a baseline; net delta from implementer-f edits = 0).

## Verdict summary

**ACCEPT — proceed to PR-Y15c-fix.**

- All 10 cases (F0031–F0040) homogeneously fire decision-tree row 1
  (Stage A/Bb/B/C true, Stage E_lod=Render false).
- Stage E probe mutation-confirmed load-bearing on row 1 vs row 3
  attribution (Adaptive sub-readings rule out operand-mesh defect).
- F32 round-trip independently re-verified: 20/20 operand readings
  well_formed=true; the well_formed=false finding on Render is genuine.
- No cheaper proxy exists for the Stage E question itself
  (`REVOLVE_DEBUG` produces 0 lines on this cohort), but the next-layer
  canary site for PR-Y15c-fix Phase 0 is mapped: probe before/after
  `weld_shared_edge_vertices` at `tessellation/mod.rs:791`, using the
  existing `repair::count_unpaired_in_mesh` primitive.
- Diff is byte-clean against implementer-f's commit; mutation reverted.
- Three minor verification deltas flagged (§7): F0031 d_epsilon
  mis-quote in implementer-f's verbatim block, clippy baseline
  drift (91 vs 92), F0040 sample exact match. None are load-bearing.

**Recommendation for PR-Y15c-fix scope:** target
`crates/kernel/src/tessellation::tessellate_solid_ext_with_lod` (per
implementer-f's anchor naming). PR-Y15c-fix Phase 0 should canary the
pre-/post-`weld_shared_edge_vertices` boundary at L791 first to split
"per-face fan emission produces non-byte-identical verts" from "weld
loses verts" before writing fix code (per `feedback_anchor_before_fix.md`
strategic-escalation rule). A15.6 cross-domain coordination needed
(`tessellation::` is outside Yang Boolean scope per A15.6).
