# Yang PR-Y41 — Dispatch-loop emission probe at `tessellate_planar_face_bounded`

**Verdict in header: SHIP-INFRA + 7th-refutation framing + STRATEGIC PIVOT RECOMMENDED.**

| Field | Value |
|---|---|
| Authors | team-lead, canary-y41, spec-y41 |
| Date | 2026-05-14 |
| Parent commit | `7a3e4c3` |
| Class | INFRASTRUCTURE-CLASS (env-gated probe; 0 LOC production logic) |
| Probe LOC | ~317 LOC additive in `crates/kernel/src/tessellation/mod.rs` (env-gated, default-off) |
| Cumulative probe LOC across Y36/Y37/Y38/Y40/Y41 | ~1358 LOC |
| Target sites | `tessellate_planar_face_bounded` entry (`mod.rs:3311`) + exit; `tessellate_solid_bounded` parent driver |
| Verdict | **SHIP-INFRA + 7th-refutation framing** — PR-Y40 §6's "missing ~12 D.1d indices upstream of F.0" framing is empirically refuted by direct dispatch measurement. All 18 expected indices are emitted. **Strategic pivot to external-oracle diagnosis recommended.** |
| Cycle position | 10th investigational PR on F0020 Render LOD; 5 ABORTs (Y25/Y26/Y27/Y28/Y39) + 5 INFRA-SHIPs (Y36/Y37/Y38/Y40/Y41) |

---

## §1 Context

PR-Y40 (commit `57bfe32`, audit `7a3e4c3`) shipped an INFRA probe inside `remove_winding_insensitive_duplicates` and refuted PR-Y39's "16 D.1d-loser collisions at F.0→F.1" attribution with a measurement of 4 collisions. PR-Y40 §6 banked a follow-up claim: of 18 D.1d-emitted indices, 4 lose at F.0→F.1, 2 survive — *the other ~12 indices come from a DIFFERENT mechanism (likely degenerate-vert collapse at dispatch)*. PR-Y41 was scoped to canary the F.−1 → F.0 transition: instrument the per-face triangulator (`tessellate_planar_face_bounded`) and measure where the inferred ~12-index residual is lost.

**The PR-Y41 dispatch probe refutes the "missing 12" framing.** When `indices_emitted` is measured directly at the planar-dispatch exit site, F0020 kids 218 / 232 / 233 emit **exactly 18 indices** (3 + 6 + 9). There is no upstream loss between dispatch (F.−1) and the canonical-key dedup entry (F.0). PR-Y40's §3.3 row "`tris surviving F.1 (predicted: dispatched - lost) = 2`" already matched downstream observation; the "missing 12" residual was an over-interpretation in PR-Y40 §6, not a measurement. **The D.1d dispatch chain is verified correct end-to-end across Y36 inverse + Y40 canon-dedup + Y41 dispatch.**

This is the 10th investigational PR cycle on F0020 Render LOD. The plan (`snappy-humming-hejlsberg.md` §strategic-checkpoint) explicitly conditioned PR-Y42 strategy on PR-Y41's outcome: *"No anomaly (Gate 4 = 18) → 7th-refutation; ~1041+~317 LOC cumulative probe with no production code in 10 cycles. Strategic pivot recommended at this point — options (B) different diagnostic strategy or (C) pause F0020 Render LOD."* The checkpoint condition has fired.

PR-Y41 ships the probe as durable scaffolding (default-off byte-identical) and triggers the planned strategic pivot. **No production code is modified.**

## §2 Why infra-class + strategic checkpoint

`feedback_anchor_before_fix` ("3 wrong anchors in a row → stop bisecting, build a reference comparison") was the discipline that produced PR-Y29's Cherchi C++ differential-diff harness in early May. Since then, F0020 Render LOD has been worked under a strict empirical-anchor-before-production discipline:

- **PR-Y25 / Y26 / Y27 / Y28 / Y39** — five consecutive canary-stage ABORTs on D.1 fix candidates. Each eliminated one wrong attribution.
- **PR-Y36 / Y37 / Y38 / Y40 / Y41** — five INFRA-only SHIPs. Each added durable measurement scaffolding at successively-refined sites along the dispatch → topology → render pipeline.
- **PR-Y41 (this PR)** — the 7th refutation by a stricter probe of a prior PR's banked inference. The dispatch chain D.1d emits exactly 18 indices; PR-Y40 §6's "missing 12" framing is an inference artifact.

The discipline has paid off: 10 cycles, all wrong candidates eliminated, no production cycle wasted on inferred-but-unmeasured fix shapes. **But the diagnostic ROI has plateaued.** Each successive probe-refinement PR has produced no production code and no movement in F0020 unpaired count (40 → 40 across all 10 cycles). The pattern argues that probe-refinement at increasingly granular sites is no longer the right tool. The plan's PR-Y41 §strategic-checkpoint explicitly anticipated this: a Gate-4-equals-18 outcome is the trigger for pivoting to an external-oracle-led diagnosis strategy (option B.1) or to deprioritizing F0020 Render LOD (option C).

This is the disciplined response to the 10-cycle search: ship the load-bearing INFRA measurement (which closes the D.1d dispatch-side investigation cleanly), then pivot the next cycle's diagnostic strategy.

## §3 Probe design

The probe is contained within `crates/kernel/src/tessellation/mod.rs`. Production logic — the existing emission paths of `tessellate_planar_face_bounded` and the dispatch loop in `tessellate_solid_bounded` — is unchanged.

### §3.1 Probe sites

Three edit hunks (all in `tessellation/mod.rs`):

1. **`tessellate_planar_face_bounded` entry** (`mod.rs:3311-3340+`) — capture `y41_start_idx_count`, `y41_start_vert_count`, `y41_boundary_positions: Vec<[f64;3]>`, `y41_inner_count`, `y41_boundary_size`. If `boundary.len() < 3` push an empty record and return early.
2. **`tessellate_planar_face_bounded` exit** (`mod.rs:3519+`) — after all four emission branches complete, compute `indices_emitted = out_indices.len() - y41_start_idx_count`, then per-triangle quantize (`y41_quantize_f32_vert` with `y41_inv_grid_from_verts`) and classify into three buckets:
   - **distinct_quantized_tris**: all 3 quantized verts pairwise distinct
   - **degenerate_collapse_count**: all 3 quantize to the SAME i64 grid cell (fully-degenerate / zero-area at quantization)
   - **single_vert_collision_count**: exactly 2 of 3 quantize to the same cell (thin-sliver tri)

   Push `Y41DispatchRecord` into thread-local buffer.
3. **`tessellate_solid_bounded` parent driver** — pre-loop drain stale records (defensive); per-face snapshot buffer position before dispatch match and drain newly-pushed records after, attribute to `(kid, face_idx)`; post-loop call `y41_write_dispatch_tsv` to emit per-invocation TSVs.

Plus the Y41 helper module (~140 LOC at `mod.rs:4891-5031+`): `Y41DispatchRecord` struct, thread-local invocation counter and buffer, `y41_probe_enabled` env gate, `y41_inv_grid_from_verts`, `y41_quantize_f32_vert`, and `y41_write_dispatch_tsv`.

### §3.2 Env gate (verbatim, `mod.rs:4891-4893`)

```rust
fn y41_probe_enabled() -> bool {
    std::env::var("Y41_DISPATCH_PROBE").as_deref() == Ok("1")
}
```

`Y41_DISPATCH_PROBE_DIR` controls the output directory for TSVs (same pattern as Y36/Y37/Y38/Y40).

### §3.3 Per-invocation TSV schema

`$Y41_DISPATCH_PROBE_DIR/$CASE_inv$N_dispatch.tsv`:

```
kid  face_idx  dispatch_type  boundary_size  inner_count  indices_emitted  distinct_quantized_tris  degenerate_collapse_count  single_vert_collision_count
```

`$Y41_DISPATCH_PROBE_DIR/$CASE_inv$N_d1d_summary.tsv` — focused on kids 218 / 232 / 233 (and totals) for the load-bearing 18-index accounting.

### §3.4 Default-off invariant

When `Y41_DISPATCH_PROBE != "1"`:
- `y41_on = false`
- Entry block returns zero captures (all guarded by `if y41_on { … } else { 0 / Vec::new() }`)
- Exit block skipped entirely
- Parent driver pre-loop drain skipped; per-iteration snapshot/drain skipped; writer skipped

Empirically validated by canary Gate 2: F0020 spotlight default-off produces a byte-identical Status line, Detail line, stage-f progression, and conformal-probe output relative to the pre-PR-Y41 baseline.

### §3.5 Methodological note: probe-site rationale

Y36 inverse-probe already records `indices_emitted_dispatch` per face at the end of `tessellate_solid_bounded` (`mod.rs:4984`, `end_index - start_index`). The dispatch and parent-driver measurements MUST agree by construction (no intervening code path), so Y41's `indices_emitted` field is technically redundant relative to Y36. What Y41 adds:

1. **Per-triangle quantization classification** at dispatch (Y36 only sees aggregate `indices_emitted` per face; cannot see per-triangle structure).
2. **Direct attestation at the dispatch site itself** that the dispatch IS emitting N indices, not merely that the parent driver counts N at end-of-loop. Y39 §2.5's indices-vs-tris confusion + Y40 §6's missing-12 inference both occurred at Y36-derived inference steps; Y41 measures the per-triangle structure explicitly so subsequent inferences cannot conflate the two.

## §4 Empirical findings (LOAD-BEARING refutation)

### §4.1 F0020 inv006 D.1d 18-index accounting (the refutation table)

`inv006` is the load-bearing F.0 boolean-result repair pass. Y41 measures `total_tris = 138` byte-matching PR-Y40 `n_tris_input = 138` AND byte-matching stage-f `sub=0 n_tris=138`. Triple-anchored on the same load-bearing invocation.

| kid | face_idx | dispatch | boundary | indices_emitted | distinct_q_tris | degen | single_coll |
|---|---|---|---|---|---|---|---|
| 218 | 26 | planar | 3 | **3** | 1 | 0 | 0 |
| 232 | 40 | planar | 4 | **6** | 1 | 0 | 1 |
| 233 | 41 | planar | 5 | **9** | 1 | 0 | 2 |
| **TOTAL D.1d** | – | – | – | **18** | **3** | **0** | **3** |

| Quantity | PR-Y40 §6 banked inference | PR-Y41 measured | Refutation |
|---|---|---|---|
| D.1d indices emitted at dispatch | "missing ~12 upstream of F.0" → implies < 18 | **18 EXACT** | **REFUTED** |
| D.1d tris emitted at dispatch | 6 (consistent with §3.3) | **6** | confirmed |
| D.1d tris lost at F.0 → F.1 canon-dedup | 4 (consistent with §3.3) | (downstream — not measured by Y41) | confirmed by PR-Y40 |
| D.1d tris surviving F.1 | 2 (consistent with §3.3) | (downstream — not measured by Y41) | confirmed by PR-Y39 §2.3 + PR-Y40 §3.3 |
| Missing-indices residual | ~12 (banked in §6) | **0** | **REFUTED** |

**Gate 4 outcome = 18 EXACT.** The Y40 §6 "missing ~12 indices upstream of F.0" was an over-interpretation: PR-Y40 §3.3's own row "tris surviving F.1 = 2" with kid breakdowns (218→0, 232→1, 233→1) already fully accounts for the 18 emitted indices (4 lost × 3 = 12 indices lost; 2 surviving × 3 = 6 indices kept; 12 + 6 = 18 indices emitted). There is no missing-12 residual. PR-Y40's underlying §3.3 measurement was correct; the §6 inference was wrong.

### §4.2 Cross-reference: PR-Y40 §3.3 vs PR-Y41 dispatch measurement

| Quantity | PR-Y40 §3.3 (downstream count) | PR-Y41 (direct dispatch) | Agreement |
|---|---|---|---|
| Kid 218 dispatched tris | 1 (from `indices_emitted_dispatch / 3`) | **1** (direct triangle count) | ✓ |
| Kid 232 dispatched tris | 2 | **2** | ✓ |
| Kid 233 dispatched tris | 3 | **3** | ✓ |
| TOTAL D.1d tris dispatched | 6 | **6** | ✓ |
| TOTAL D.1d indices dispatched | 18 | **18** | ✓ |

The two probes are mutually consistent.

### §4.3 Per-triangle quantization signal (NEW from Y41)

PR-Y40 measured collision-loser face_ids but could not see per-triangle quantization. Y41 reveals the D.1d kids' triangle-level quantization structure:

- **Kid 218** (1 tri): 1 distinct ✓ (no quantization collisions)
- **Kid 232** (2 tris): 1 distinct + **1 single-collision** (2 of 3 verts coincide post-quantize → 50%)
- **Kid 233** (3 tris): 1 distinct + **2 single-collision** (67% of dispatched tris have a vertex coincidence)

Single-collision triangles ("thin slivers") present a canonical-key `[v0, v_dup, v_dup]` to `remove_winding_insensitive_duplicates`. When two different faces emit single-collision tris with the same dup-pair, they canonical-key-collide at F.0. This matches PR-Y40's 4 D.1d-loser collisions exactly (1 kid 232 single-collision + 2 kid 233 single-collisions + 1 kid 233 intra-kid self-collision = 4). The Y41 per-triangle signal corroborates PR-Y40's count from an independent measurement direction.

### §4.4 F0020 fully-degenerate cluster (CONFIRMED F0020-specific)

Y41 also captures non-D.1d dispatch records on F0020 inv006:

| kid | tris dispatched | distinct | degen | single |
|---|---|---|---|---|
| 198 | 3 | 1 | **1** | 1 |
| 231 | 3 | 1 | **1** | 1 |
| 235 | 7 | 0 | **7** | 0 |
| 256 | 4 | 0 | **4** | 0 |
| **degen total (F0020)** | – | – | **13** | – |

Kids 235 and 256 emit ALL their triangles as fully-degenerate (zero-area, all 3 verts to same quantized cell). This matches PR-Y40 §3.5's 10 fully-degenerate canonical-key collisions at key `(65051,-15817,-36086)`: dispatch emits 13 fully-degenerates → F.0 canon-dedup collapses 10 of them → 3 survive to contribute to F0020's "8 of 113 triangles are degenerate" final degenerate count.

F0020 total inv006 dispatch breakdown: 138 tris = 114 distinct + 13 fully-degenerate + 11 single-collision. ✓

### §4.5 Cohort confirms F0020-specificity

| Case | invocations captured | tot_indices | tot_tris | distinct | degen | single_coll |
|---|---|---|---|---|---|---|
| **F0044** | 1 | 180 | 60 | 60 | **0** | **0** |
| **F0045** | 1 | 19,890 | 6,630 | 6,629 | **0** | 1 |
| **R0045** | 1 | 1,824 | 608 | 608 | **0** | **0** |
| **R0092** | 1 | 40,863 | 13,621 | 13,571 | **0** | 50 |

Cohort cases dispatch >99% clean triangles with **zero fully-degenerate emissions**. The F0020 fully-degenerate cluster (kids 235, 256) is **F0020-specific** — confirmed at the dispatch site. The 50 single-collision tris in R0092 are <0.4% of 13,621 dispatched — not the dominant defect mechanism for R0092.

None of the cohort cases have kids in {218, 232, 233} (their kid ID-spaces differ). Cohort Render LOD defects are NOT in the D.1d mechanism — consistent with PR-Y27/Y28/Y40 cohort splits (D.1 F0020-specific; D.2 F0044/F0045; D.3 R0092).

### §4.6 Empirical-gates table

| Gate | Description | Status | Observed |
|---|---|---|---|
| **1** | Build with probe | **GREEN** | `cargo build -p kernel` clean (58 warnings; 1 new `boundary_positions never read` — reserved for future use, no functional issue) |
| **2** | F0020 default-off byte parity | **GREEN** | `Status:Failed; 40 unpaired (39 boundary, 1 NMM); 8 degen; 10 self-int` IDENTICAL to PR-Y40 baseline; stage-f progression byte-identical (138→119→119→113→113; unpaired 30→42→39→39→39) |
| **3** | Probe fires (F0020) | **GREEN** | 6 invocations × 2 files = 12 TSV files in `/tmp/y41-probe`; inv006 has 65 faces dispatched, 138 tris emitted |
| **4** | F0020 18-index accounting (LOAD-BEARING) | **= 18 EXACT** | Kids 218=3, 232=6, 233=9 indices emitted. **EXACTLY 18 — no upstream loss. 7th-refutation framing applies.** |
| **5** | F0020 D.1d degenerate-quantization | **NEW SIGNAL** | 0/6 fully-degenerate; 3/6 single-vert-collision. F0020-specific fully-degen cluster confirmed (kids 235, 256, 198, 231) |
| **6** | Cohort F0044/F0045/R0045/R0092 | **VERIFIED** | All cohort cases dispatch with ZERO fully-degenerate. No D.1d signature in cohort. |
| **7** | kernel lib regression | **GREEN** | `1262 passed; 24 failed; 42 ignored` — IDENTICAL to baseline |
| **8** | yang_fast corpus | **GREEN** | `10/157 passed, 139 failed, 8 errored (skipped 33)` — IDENTICAL to baseline |

## §5 PR-Y42 strategic-pivot recommendation

Per the plan §strategic-checkpoint, a Gate-4-equals-18 outcome triggers the strategic-pivot recommendation. Three options are banked; the canary §6 ranks them:

### §5.1 PRIMARY recommendation — option (B.1): extend PR-Y29 Cherchi differential harness to Render LOD vertex diff

**Lowest-risk, highest-information-yield pivot.** PR-Y29's Cherchi C++ sidecar exists at `external/InteractiveAndRobustMeshBooleans/` (~14s build) and the `cherchi_differential_diff` harness lives at `crates/test-harness/tests/cherchi_differential_diff.rs` (671 LOC, 8 helpers including `run_diff_for_case`). PR-Y31 already plumbs the per-case boolean operator through to Cherchi. PR-Y30 calibrated the harness to compare Stage B (post-`face_survival_detect`); PR-Y33 compared per-stage byte-diffs against C++ reference.

Extending the harness to compare **Render LOD output** vertex-by-vertex (take Cherchi's boolean result, tessellate identically — same LOD bounds — and diff against Waffle's Render LOD output) is incremental work (~150-250 LOC, reuses existing Cherchi build + sidecar + parse_obj + quantize_tri infrastructure).

**Why this is the load-bearing pivot:** every probe in PR-Y36..Y41 has measured Waffle's behavior in isolation. There has been no external ground-truth oracle for F0020 Render LOD. Without one, every probe-refinement step produces a "this is internally consistent with that downstream observation" finding but cannot tell us *whether the dispatch's 18 emitted indices are themselves the correct emission*. A Cherchi-C++ Render-LOD diff provides exactly that oracle: a known-good reference implementation's Render-LOD output that we can byte-diff against.

This is also what `feedback_external_coherence` directly prescribes: *"When the algorithm we're porting has a public reference implementation (Cherchi 2020/2022 C++), build differential testing against that reference as the load-bearing oracle. Internal stage oracles measure self-consistency; reference parity measures correctness."* The Y36..Y41 probe stack measures self-consistency; B.1 measures correctness.

### §5.2 SECONDARY recommendation — option (B.2): synthetic minimum-failing-case fixture for F0020 D.1d signature

Construct a synthetic test case that reproduces F0020 D.1d's signature (a planar face dispatch producing single-vert-collision tris with the F0020 single-collision boundary structure) at minimum complexity. Smaller, manually-bisectable. Less leveraged than B.1 because it still measures Waffle in isolation, but more focused than continuing F0020 itself (which has 65 faces dispatched at inv006 and many confounding signals).

### §5.3 TERTIARY recommendation — option (C): pause F0020 Render LOD; pivot PR-Y42 to other priorities

Park F0020 Render LOD explicitly. PR-Y42 picks up another CLAUDE.md priority area:
- Cohort cases F0044/F0045/R0092 (D.2/D.3 mechanisms, different from F0020 and not exhibiting F0020's fully-degen / D.1d signatures)
- SSI solvers (A15.4 matrix; feeds Yang stage 4)
- GUI test coverage (priority #3)
- Cross-crate integration tests (priority #4)

F0020 remains a known-failing case; future work on the broader Cherchi-port arrangement defect (post-Y35) may resolve it incidentally, or B.1 may be revisited later.

### §5.4 Recommendation

**B.1 is the recommended PR-Y42 anchor.** If B.1's Cherchi Render-LOD diff also fails to localize the F0020 defect, that's the rightful trigger for C — pause F0020 Render LOD. Either way, **a 10th probe-refinement PR on F0020 Render LOD D.1d at any finer granularity would be the empirically-wrong move.** Y41 closes the D.1d dispatch-side investigation cleanly; the dispatch IS emitting the expected indices; the defect (if any) is in DOWNSTREAM consumption by the watertight oracle relative to the correct reference, not in dispatch attribution.

## §6 Strategic context — honest reckoning

The F0020 Render LOD search has now spanned 10 PR cycles:

| PR | Outcome | What was refuted |
|---|---|---|
| Y25 | ABORT (canary) | Yang §4.4.1 mesh-updating Diagnosis B not empirically load-bearing |
| Y26 | ABORT (canary) | Defect is missing triangles cohort-wide, not Phase 1's 3 candidates |
| Y27 | ABORT (canary) | `flood_fill_patches` drops zero SourceFaces; D.1 splits into 3 sub-mechanisms |
| Y28 | ABORT (canary) | D.1d identified, but fix-shape refused commit |
| Y36 | INFRA SHIP | Y36 inverse-probe source-face attribution downstream |
| Y37 | INFRA SHIP | H1/H2/H3 classification refined |
| Y38 | INFRA SHIP | Grid-sensitivity oracle gate |
| Y39 | ABORT (canary) | F.1 → F.2 anchor (`remove_nonmanifold_topology_aware`) refuted |
| Y40 | INFRA SHIP — 6th-refutation | "16 D.1d-loser collisions" refuted (actual 4) |
| **Y41** | **INFRA SHIP — 7th-refutation** | **"missing 12 indices upstream of F.0" refuted (actual 0; 18 emitted exactly)** |

**Cumulative: 10 cycles. ~1358 LOC of probe instrumentation (Y36/Y37/Y38: ~711; Y40: ~151; Y41: ~317; Y38 oracle: ~179). 0 LOC of production fix. 0 movement in F0020 unpaired count (40 → 40).** Each cycle has eliminated a candidate (D.1c, H1/H2 detection, phantom-from-quantization, Y39 stage attribution, Y40 16-collision frame, Y41 missing-12 inference) but no fix anchor has emerged.

The pattern argues clearly for the planned strategic pivot. The probe scaffolding is durable and provides empirical reference layers for future cycles; that is the value Y36..Y41 have delivered. But continued probe-refinement at deeper sites within Waffle, without an external oracle, has reached the limit of its diagnostic ROI. **Option (B.1) — extend PR-Y29's Cherchi differential harness to Render LOD — is the empirically-supported next step.**

## §7 Out of scope (banked, NOT addressed by PR-Y41)

- **F0020 Status:Failed.** 40 unpaired (39 boundary, 1 NMM), 8 degenerate, 10 self-intersections — unchanged. PR-Y41 ships zero production logic.
- **F0045 / R0092 retess-pass 13K-collision outliers** (PR-Y40 §4.3). Different defect (fully-degenerate Render-LOD quantization on huge planar faces). Banked.
- **139 / 157 yang_fast failures.** Unchanged (Gate 8 confirms 10/157 baseline).
- **Cherchi TBB non-determinism.** Banked from PR-Y29/Y30/Y31. Use missing-count (deterministic) as gate; extras can drift.
- **Yang §4.4.1 mesh-updating (Diagnosis B from PR-Y25).** Banked as the long-term load-bearing layer; no infrastructure or production work here.
- **F0020 fully-degenerate cluster (kids 235, 256, 198, 231).** Y41 confirms it is F0020-specific and contributes 10/19 of F.0→F.1 collisions, but its connection to the 40-unpaired-edge defect is unproven. The 3 surviving fully-degenerate tris contribute to F0020's `8 of 113 degenerate` final count but the unpaired edges are reportedly elsewhere (PR-Y36 face_inventory). Banked.

**No "this closes Yang" or "this is the last bug" language** (`feedback_no_last_bug`). We do not know how many bugs remain. The dispatch chain D.1d is verified correct end-to-end; F0020 Render LOD as a whole is NOT closed.

## §8 Risk / mitigation

### §8.1 Default-off byte parity is load-bearing

The probe lives inside the production functions (`tessellate_planar_face_bounded`, `tessellate_solid_bounded`). If `Y41_DISPATCH_PROBE != "1"`, the path through both functions must be byte-identical to the pre-PR-Y41 baseline. Canary §1 verified this on F0020 spotlight (Gate 2: same Status line; same stage-f progression 138→119→119→113→113; same `unpaired` per-stage 30→42→39→39→39).

Implementation must re-run Gates 2 / 7 / 8 fresh on the live tree at commit time:
- Gate 2: `YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1 cargo test -p test-harness --test assay_randomized -- spotlight_f0020 --ignored --nocapture` → 40 unpaired, 8 degen, 10 self-int
- Gate 7: `cargo test -p kernel --lib` → 1262 pass, 24 fail, 42 ignored
- Gate 8: `YANG_BOOLEAN=1 cargo test -p test-harness --test assay_randomized -- yang_fast --ignored --nocapture --test-threads=1` → 10/157 pass

### §8.2 Probe-on overhead

The probe-on path allocates an additional `Vec<[f64;3]>` per face (for `boundary_positions`) and pushes a `Y41DispatchRecord` into a thread-local Vec. For F0020 inv006 (65 dispatched faces, 138 emitted tris), overhead is negligible. For the F0045 / R0092 cohort cases (6k–13k tris), the buffer grows proportionally; per-invocation TSV write is one-shot. Dev-only output; not a concern.

### §8.3 Cumulative probe complexity is at a limit

Across `tessellation/mod.rs` (Y36/Y37: ~711 + Y41: ~317), `repair.rs` (Y40: ~151), and `oracle.rs` (Y38: ~179), the kernel + test-harness now carry **~1358 LOC of env-gated probe code**. The cumulative scaffold is durable but represents real complexity. PR-Y41 is the cycle at which the plan's pre-baked strategic checkpoint fires: continued probe-refinement at finer Waffle-internal sites is no longer the right tool. The recommended PR-Y42 pivot (B.1) shifts the next cycle's investment to extending external-oracle scaffolding (PR-Y29 harness), which is also infra-class but pivots the diagnostic frame from self-consistency to reference-parity. If B.1 also fails to localize the F0020 defect, escalate to C (pause F0020 Render LOD).

### §8.4 The "missing 12" inference was an interpretation, not a measurement

PR-Y40 §6 framed its banked follow-up as *"the other ~12 indices come from a DIFFERENT mechanism (likely degenerate-vert collapse at dispatch)."* The phrasing reads as a hypothesis, not a measurement. PR-Y40's §3.3 measurements were correct; only the §6 over-interpretation was wrong. **Lesson reinforced: `feedback_phase1_diagnosis_ranking_is_inference` and `feedback_anchor_before_fix` both apply at PR-banked-claim level, not only at Phase 1 Explore-agent level.** A banked claim from one INFRA PR's §6 is itself inference until directly measured. Y41 measured it and refuted it cleanly; this is the discipline working as intended, but it also indicates the strategic checkpoint has fired and the next cycle needs an external anchor, not another internal probe.

## §9 Paper citations

- **Yang 2025 §4.4.1** (`refs/text/yang2025_hybrid_boolean.txt:605-610`): *"As the intersections on the surfaces are relocated and refined during the optimization, the bijectivity is essentially broken. Each intersection curve is no longer mapped to the corresponding intersection curve between the two meshes, thus causing gaps or self-intersections."* This conformality-preservation concern is the long-term load-bearing context for F0020 Render LOD as a whole. The F0020 fully-degenerate cluster (Y41 §4.4: kids 235, 256, 198, 231 dispatching 13 zero-area tris) is one plausible mechanism for bijectivity breaking at Render LOD, but the connection from Yang §4.4.1 to F0020's 40 unpaired edges is currently unverified. Banked as the long-term load-bearing layer (Diagnosis B from PR-Y25).

- **Cherchi 2022 §3** (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:240-320`): describes the pipeline as a two-step process (mesh arrangement + patch inside/outside classification). The `tessellate_planar_face_bounded` site is part of Waffle's Render LOD layer, *downstream* of Cherchi's arrangement stage. The dispatch loop is a Render-LOD-only operation and is outside the Cherchi paper's scope — this is consistent with `feedback_external_coherence`: the PR-Y41 probe IS the empirical reference at this layer; there is no upstream paper-cited oracle for per-face dispatch emission. The recommended PR-Y42 pivot (B.1, extending Cherchi differential diff to Render LOD) is what introduces the external oracle that this layer currently lacks.

The probe at `tessellate_planar_face_bounded` is empirical-only by necessity; no paper section governs its correctness beyond Cherchi 2022's well-formed-simplicial-complex output guarantee (which is upstream of Render LOD).

---

## Appendix A — Reproduction commands

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

# Gate 2 — default-off byte parity
YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1 \
  cargo test -p test-harness --test assay_randomized -- spotlight_f0020 --ignored --nocapture 2>&1 | tail -30
# expect: 40 unpaired (39 boundary, 1 NMM); 8 degen; 10 self-int — IDENTICAL to pre-PR-Y41

# Gate 6: cohort
rm -rf /tmp/y41-cohort && mkdir -p /tmp/y41-cohort
Y41_DISPATCH_PROBE=1 Y41_DISPATCH_PROBE_DIR=/tmp/y41-cohort \
  YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1 \
  cargo test -p test-harness --test assay_randomized -- spotlight_f0044 spotlight_r0045 --ignored --nocapture
# expect: ZERO fully-degenerate emissions in cohort dispatch.tsv files

# Gate 7: kernel lib
cargo test -p kernel --lib
# expect: 1262 passed, 24 failed, 42 ignored

# Gate 8: yang_fast
YANG_BOOLEAN=1 cargo test -p test-harness --test assay_randomized -- yang_fast --ignored --nocapture --test-threads=1
# expect: 10/157
```

## Appendix B — Critical files

- `crates/kernel/src/tessellation/mod.rs:3311-3340` — `tessellate_planar_face_bounded` entry probe site (Y41 capture: `y41_on`, `y41_start_idx_count`, `y41_start_vert_count`, `y41_boundary_positions`, `y41_inner_count`, `y41_boundary_size`; early-return empty record when `boundary.len() < 3`)
- `crates/kernel/src/tessellation/mod.rs:3519-3560` — `tessellate_planar_face_bounded` exit probe site (Y41 per-triangle quantization + `Y41DispatchRecord` push)
- `crates/kernel/src/tessellation/mod.rs:4805-4900` — `tessellate_solid_bounded` parent driver (Y41 pre-loop drain, per-iteration snapshot/drain, post-loop TSV writer call)
- `crates/kernel/src/tessellation/mod.rs:4891-5031+` — Y41 helper module: `y41_probe_enabled`, `y41_push_record`, `y41_take_records`, `y41_next_invocation`, `y41_inv_grid_from_verts`, `y41_quantize_f32_vert`, `Y41DispatchRecord`, `y41_write_dispatch_tsv`
- `docs/audits/pr_y41_canary.md` — load-bearing canary memo with 18-index refutation table (~480 lines)
- `docs/audits/pr_y40_canary.md` §3.3 / §6 — refuted "missing 12" inference (LOAD-BEARING context for §1)
- `crates/test-harness/tests/cherchi_differential_diff.rs` — PR-Y29/Y30/Y31 Cherchi differential-diff harness (671 LOC; B.1 pivot starting point)
- `crates/kernel/src/tessellation/mod.rs:4180-4570` — Y36 inverse probe (cross-validation reference for §4.2)
- `crates/kernel/src/tessellation/repair.rs:502-700` — Y40 collision probe at `remove_winding_insensitive_duplicates` (downstream cross-reference for §4.1)
- `crates/test-harness/src/oracle.rs:185-264` — Y38 grid-sensitivity probe (read-only reference)
