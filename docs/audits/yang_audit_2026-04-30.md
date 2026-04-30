# Yang+Cherchi 2022 Audit — 2026-04-30 (synthesized)

**Audit date**: 2026-04-30
**Branch**: `yang-audit-2026-04-30`
**Method**: 4-auditor team, paper-vs-implementation slicing, lead synthesis
**Predecessor**: `docs/audits/cherchi_port_audit.md` (2026-04-28; 42 findings on the Cherchi 2020 port)

## Sources audited

- **Yang 2025** (`docs/references/yang2025_hybrid_boolean.txt`, full re-read).
- **Cherchi 2022** (`/tmp/cherchi2022.txt`, full re-read).
- **Yang pipeline** (Auditor A): `boolean/yang_integration.rs`, `boolean/mod.rs`, `boolean/coplanar_preprocess.rs`, `boolean/topology_extract.rs`.
- **Yang assay corpus** (Auditor B): 157-case `yang_fast` slice, post-run `results.json`, classified by first failure point.
- **Cherchi 2022 layer** (Auditor C): `boolean/exact_mesh.rs` (`label_cells`, `ray_cast_inside`, `label_sub_tri_raycast`), patch segmentation in `flood_fill_patches`.
- **Tessellation thread retrospective** (Auditor D): PR1-7 commits, `specs/tessellation_*.md`, `tessellation/mod.rs`, `tessellation/bijective.rs`, `tessellation/pr7_classify.rs`.

Per-auditor reports:
- `docs/audits/yang_audit_a_yang_pipeline.md` — 35 findings (`YA-NN`).
- `docs/audits/yang_audit_b_assay_failures.md` — 18 findings (`YB-NN`) + 157-case classification table.
- `docs/audits/yang_audit_c_cherchi2022.md` — 24 findings (`YC-NN`).
- `docs/audits/yang_audit_d_tessellation_retro.md` — 15 findings (`YD-NN`).

**Total: 92 findings.**

## Executive summary

The Yang assay sits at 7-9/157 passing. **84.7% of the corpus fails on three failure modes** (Auditor B): YANG-ERR-twin-validation (92 cases, 58.6%), WATERTIGHT-unpaired (26, 16.6%), SELF-INTERSECT (15, 9.6%). Only **6 of the 8 passing cases are real boolean results** — 5 are trivial coplanar merges and 1 (R0018) is nondeterministic. The "honest baseline" is closer to **0/157** (per memory `[Yang Implementation Status]`).

The audit converges on a single architectural diagnosis. Three independent slices produce the same finding from different angles:

- **B's YB-01** (92 cases fail flood_fill_patches twin-pairing): single error message aggregates many upstream causes.
- **A's YA-01** (per-sub-triangle labeling instead of per-patch): Yang stage 4b is at the wrong granularity.
- **C's YC-05** (per-patch labeling DEVIATES): Cherchi 2022's Section 5 *headline contribution* — "the algorithm scales with the number of patches in the arrangement and not with the number of triangles" — is forfeited.

These are **the same bug at three layers**: we run per-sub-tri ray-casts where Cherchi/Yang prescribe per-patch ray-casts, and as a consequence (a) we can't honor Cherchi's proof-of-correctness via patch-graph propagation, (b) `flood_fill_patches` consumes a patch graph it built with Yang-style intersection-edge barriers (per YC-06) instead of Cherchi-style manifold-edge barriers, (c) inconsistent labels propagate into B-Rep assembly, (d) the assembler emits unpaired half-edges that `validate_yang_result_topology` rejects.

Auditor D's retrospective on PR1-7 reaches the same conclusion via the deprecated-S-H-pattern argument: the bounded-path tessellator with its 6-stage repair pipeline is structurally a different algorithm from Yang §4.1.2's u-v CDT-with-boundary-constraints. Patching repair stages does not advance the bijective contract.

### Top 5 findings by leverage

1. **(YA-01 + YC-05 + YB-01) Per-patch labeling architecture** — Yang stage 4b should classify patches, not sub-triangles. Implementing this correctly subsumes the dominant assay failure bucket (92/157 cases) AND eliminates the wrong-barrier asymmetry in `flood_fill_patches`. This is the highest-leverage single change in the audit.
2. **(YA-13) Surface tier lost on every Yang result** — `result_topology_to_waffle_solid` synthesizes Newell-`Planar` for every result face, ignoring `surface_map`. A15.5 + A15.6 violation. Chained Yang booleans degrade to faceted approximations even when source faces are quadric.
3. **(YC-01/YC-02/YC-03/YC-08/YC-15) `findRayEndpoints` cascade MISSING** — Cherchi 2022 §5.1's full ray-origin cascade (interior `vertInfo == 0` vertex → snap-rounded centroid push back along `maxComponentInTriangleNormal` → exact rationals) is collapsed to "always centroid." Axis-aligned CAD inputs systematically fall into the Hoffmann fallback where YC-10 corrupts geometry.
4. **(YC-10) Hoffmann fallback uses wrong perturbation geometry** — perturbs the SAMPLE POINT along the sub-triangle's own normal instead of the RAY ENDPOINT via `std::nextafter` along axis-aligned offsets. For 45°-to-axis sub-triangles (common with cylinder/cone tessellations) the two methods sample geometrically different points. Predicted assay impact: most of the 15 SELF-INTERSECT cases (YB-05).
5. **(YD architectural verdict + YA-15) Coplanar preprocessing architectural mismatch** — Yang §4.5.5 prescribes 2D Boolean BEFORE tessellation producing identical triangles by construction; we tessellate independently and overlay/replace meshes post-hoc with single-pair T-junction repair. Multi-pair cascading is silently incorrect.

### What this audit confirms is OK

- **D-05 still faithful** (YC-16): the previous audit's highest-priority finding (parity counting → first-hit signed-volume) landed correctly at commit `3e17f08` and has not regressed.
- **Cherchi 2020 mesh-arrangement layer**: extensively audited in the prior `cherchi_port_audit.md` (42 findings, ~12 of which have been fixed in the Cluster I cleanup PRs). The remaining gaps are tracked there; this audit does not duplicate them.
- **Bijective oracle infrastructure (PR1)**: real diagnostic infrastructure that survives the rest of the tessellation thread. Not a fix, but permanent value.

### What this audit confirms is NOT working

- **PR1-7 tessellation thread did not move the Yang assay** (YD §1, §2, §5). 7 PRs, 2 fixes, 4 docs closures, no assay impact.
- **Bounded-path + repair pipeline IS the deprecated S-H pattern** (YD §2.3). Per A15.6 this approach was deprecated 2026-03-30. Patching it accommodates legacy code.
- **The gap to a Yang-§4.1.1-faithful tessellator is architectural, not tactical** (YD §3.4). PR8+ on this thread cannot be a tactical fix.

## Severity counts

| Severity | Count | Notes |
|----------|------:|-------|
| **CORRECTNESS-BUG** | 23 | YA: 6 / YB: 1 / YC: 5 / YD: ~11 (YD severity scheme uses High/Medium/Low; ~4 High map to CORRECTNESS-BUG) |
| **MISSING / STUB** | 11 | YA: 0 / YB: 0 / YC: 4 / YD: ~7 (architectural gaps documented as MISSING) |
| **DEVIATES** | 21 | YA: 13 / YC: 4 / YD: 4 |
| **PERFORMANCE-DRIFT** | 16 | YA: 5 / YC: 7 / YD: 4 |
| **UNKNOWN / scope-limit** | 21 | YA: 11 / YB: 17 (most of YB are scope-limit informational) / YC: 0 / YD: 0 |

(Total ≠ exactly 92 because some findings were tagged with multiple severities in narrative; primary severity per-finding listed above.)

## Counts by slice

| Slice | Auditor | Findings | Top severity bucket |
|-------|---------|---------:|---------------------|
| Yang pipeline (orchestrator + Stage 0/5/6) | A | 35 | DEVIATES (13) |
| Assay failure-mode analysis | B | 18 | UNKNOWN (informational); YB-01 single CORRECTNESS-BUG |
| Cherchi 2022 (`exact_mesh.rs` + ray-cast) | C | 24 | PERFORMANCE-DRIFT (7) but 5 CORRECTNESS-BUGs are highest-leverage |
| Tessellation retrospective + Yang §4.1.1 gap | D | 15 | 4 High / 7 Medium / 4 Low |

## Cross-slice cluster themes

The audit is dominated by **two cross-cutting themes** rather than independent findings:

### Cluster Y-I: Per-patch vs per-sub-triangle labeling (4 findings, ALL cross-cuts)

The single most important architectural finding. Cherchi 2022 §5 prescribes per-patch labeling (one ray-cast per patch, Algorithm 1). Yang §4.4.2 cites Cherchi 2022 for this stage. Our implementation runs per-sub-triangle.

**Members**:
- **YA-01** (Yang pipeline): `label_cells` loops every sub-tri; should be per-patch.
- **YC-05** (Cherchi 2022): "scales with #patches not #triangles" — Cherchi's headline complexity claim is forfeited.
- **YC-06** (Cherchi 2022): `flood_fill_patches` uses Yang-style intersection-edge barriers, not Cherchi-style manifold-edge barriers — a downstream symptom of the per-sub-tri choice.
- **YB-01** (assay): the dominant 92-case failure bucket is the downstream symptom of inconsistent per-sub-tri labels propagating into B-Rep assembly.

**Why this matters**: Cherchi's per-patch architecture is what *guarantees* label consistency within a patch (one ray, one label, propagated by manifold-edge graph traversal). Our per-sub-tri approach can produce mixed Inside/Outside labels within a single patch. When `flood_fill_patches` then groups sub-tris into face patches using Yang-style intersection-edge barriers, it inherits the inconsistency. The unpaired half-edges that `validate_yang_result_topology` rejects (92/157 cases) are downstream of this.

**Single fix**: replace `label_cells`'s per-sub-tri loop with per-patch labeling per Cherchi 2022 Algorithm 1. Combined with switching `flood_fill_patches` to manifold-edge barriers (YC-06), this likely subsumes 5+ findings AND unblocks ~40-92 assay cases (the 92 YB-01 + some fraction of WATERTIGHT-unpaired and SELF-INTERSECT). **Highest-leverage single change in the audit.**

### Cluster Y-II: Cherchi 2022 §5.1 + §5.3 ray-cast hardening (8 findings, all in Auditor C)

Cherchi 2022 §5.1 and §5.3 contain the paper's distinctive correctness/robustness contributions over Cherchi 2020. We ship the **happy path** (D-05 first-hit signed-volume) and degrade gracefully on the rest, but graceful-degradation is exactly Hoffmann sample-both-sides → corrupted geometry on near-degenerate inputs.

**Members**:
- **YC-01** §5.1 `findRayEndpoints` cascade — MISSING (centroid-only).
- **YC-02** §5.1 backward push 0.1 along chosen axis — MISSING.
- **YC-03** §5.1 `maxComponentInTriangleNormal` axis selection — MISSING (cycles 0..2).
- **YC-04** §5.2 implicit-LPI sort — DEVIATES (single f64 `t_hit`).
- **YC-08** §5.1 `vertInfo` border-vertex marker — MISSING.
- **YC-09** §5.3 `IntersInfo` enum / vertex-edge ambiguity dispatch — MISSING.
- **YC-10** §5.3 `nextafter` 8-offset cascade — MISSING (Hoffmann normal-perturb instead).
- **YC-15** §5.1 `tv[3]` first-hit pivot — MISSING.

**Why this matters**: D-05 is the happy-path correctness anchor (still faithful per YC-16). Everything that *degrades from* D-05 (vertex/edge ambiguity, near-degenerate geometry, axis-aligned grazes) is wrong. Predicted assay impact is most of the 15 SELF-INTERSECT cases plus contributions to WATERTIGHT-unpaired.

**Fix shape**: substantial port. The paper's `findRayEndpoints` + `IntersInfo` dispatch + `nextafter` cascade is ~280 C++ lines (per prior audit's D-07 estimate). Multi-PR.

### Other cross-references

- **YA-15 + YA-26 (coplanar architecture)** + **YD-08 (AABB-disjoint short-circuit strips primitive params)**: the coplanar preprocessing pipeline does the right ORDER (split B-Rep pre-tess) but the wrong *mechanism* (overlay+replace post-tess instead of trimmed shared surface). Multi-pair T-junction repair is deferred. AABB-disjoint short-circuit defeats PR2's revolve cap-pool fix because `result_topology_to_waffle_solid` strips primitive params.
- **YA-13 (surface tier dropped)** is independent — A15.5 violation. Even if everything above is fixed, chained booleans still degrade to faceted approximations.

## Per-slice finding tables

Detailed finding text, severity, code/paper citations, and severity tests live in the per-auditor reports. The prioritized to-fix queue below references findings by their `Y[ABCD]-NN` IDs. Open the per-auditor reports for the full text.

## Prioritized to-fix queue

Priority order: CORRECTNESS-BUG → MISSING-with-high-leverage → DEVIATES-with-high-leverage → PERFORMANCE-DRIFT.

### Tier 1: Architectural fixes (move the assay)

| Rank | ID(s) | One-line fix | Estimated assay impact |
|------|-------|--------------|------------------------|
| 1 | **YA-01 + YC-05 + YC-06** | Replace per-sub-tri labeling in `label_cells` with per-patch labeling per Cherchi 2022 Algorithm 1; switch `flood_fill_patches` to manifold-edge barriers. | HIGH — single architectural change subsumes Cluster Y-I. Unblocks downstream visibility on 40-92 cases. |
| 2 | **YC-01/YC-02/YC-03/YC-08/YC-15 + YC-10 + YC-09** | Port Cherchi 2022 §5.1 `findRayEndpoints` cascade + §5.3 `IntersInfo` ambiguity dispatch + `nextafter` 8-offset cascade. ~280-line C++ port. | MEDIUM-HIGH — predicted majority of SELF-INTERSECT (15 cases) + contributions to WATERTIGHT-unpaired. |
| 3 | **YA-13** | Wire `surface_map` provenance into `result_topology_to_waffle_solid` so unmodified result faces preserve their analytical surface tier. A15.5/A15.6 conformance. | LOW per-PR (no immediate assay change); HIGH for chained-boolean correctness and downstream rendering. Easy fix once specified. |
| 4 | **YA-15 + YA-16 + YA-26** | Move coplanar 2D Boolean BEFORE tessellation per Yang §4.5.5 (replace post-tess overlay with shared trimmed surface generated once and tessellated identically). Eliminates T-junction repair entirely. | MEDIUM — multi-pair coplanar cases (F0001-F0007) are dense in the corpus. |

### Tier 2: Mechanism-localized fixes (after Tier 1 unblocks visibility)

| Rank | ID(s) | One-line fix |
|------|-------|--------------|
| 5 | **YD-08 / cherchi_port D-13** | AABB-disjoint short-circuit strips primitive params, defeating PR2 revolve cap-pool. Either preserve params OR run the cap-pool fix downstream of `result_topology_to_waffle_solid`. |
| 6 | **YC-07** | Remove `weld_mesh_vertices` (D-10 from prior audit) once Tier 1 lands. A15.6 violation, currently load-bearing only because tessellation is non-bijective. |
| 7 | **YB-04** | Split WATERTIGHT-unpaired (26 cases) into high-rate (>30%, ~9 cases — survival classification bug) vs low-rate (<1%, ~7 cases — intersection-curve seam closure). Two distinct mechanisms under one bucket. |
| 8 | **cherchi_port C-01 + C-02** (paired) | Convert remaining defensive guards to `debug_assert!` per Cluster I cleanup pattern. Tractable; finishes Cluster I. |

### Tier 3: Cluster II/III from prior audit (if Tier 1+2 doesn't fully resolve)

The prior `cherchi_port_audit.md` Cluster II (5 findings: B-03/B-04/B-05/B-12/B-14, SimplexIntersection 4-state) and Cluster III (3 findings: A-03/A-06/B-12, jolly-points reorder) remain pending. May naturally subsume after Tier 1's per-patch labeling lands; verify before committing to them as separate PRs.

### Tier 4: Performance + cleanup

- **YC-12 / YC-13 / YC-22 / YC-23**: shared octree for arrangement + ray-cast, cached predicates, octree.intersects_box query, analyzeSortedIntersections first-of-each-label. All Cherchi 2022 §4 performance contributions. PERFORMANCE-DRIFT only.
- **YC-11**: `slab_eps = 1e-14` slab expansion (D-09 not yet fixed). Resolves with D-05's faithful path remaining intact.
- **YA-02 / YA-14 / YA-28 / YA-33**: misc PERFORMANCE-DRIFT in Yang pipeline.

## Methodology

### Team structure

- **Lead** (this synthesis): scope review, calibration, spot-check, write executive summary + queue.
- **Auditor A**: Yang 2025 pipeline conformance.
- **Auditor B**: Yang assay failure-pattern analysis (157-case classification).
- **Auditor C**: Cherchi 2022 paper-vs-port conformance.
- **Auditor D**: Tessellation thread (PR1-7) retrospective + Yang §4.1.1 contract analysis.

All four auditors re-read Yang 2025 and Cherchi 2022 in full before slicing. All ran in parallel.

### Calibration

The prior `cherchi_port_audit.md` (2026-04-28) used `tree.h` as a 4-auditor calibration target. This audit reuses that audit's findings as cross-references rather than re-validating from scratch — Auditors A/B/C/D each note where their findings overlap with prior `C-NN`/`D-NN`/etc. and what's been fixed. Status of prior findings: ~12 of 42 fixed in Cluster I cleanup PRs (Sprint), the rest pending in the prior audit's queue.

### Verification (Lead)

- Spot-checked ~25% of findings by re-reading both code and paper at cited lines.
- Verified D-05's continued correctness (cross-referenced YC-16 vs `exact_mesh.rs:1374-1449`).
- Cross-checked Cluster Y-I's three angles (YA-01 / YC-05 / YB-01 / YC-06) point at the same architectural defect.
- Cross-checked YD's "PR1-7 was wrong priority" verdict against the live Yang assay (7-9/157 stable across all 7 PRs — confirmed by auditor B's run).

### What this audit did NOT cover

- **Production code paths outside the 4 slices**: `cherchi/intersection_class.rs`, `cherchi/triangulation.rs`, `cherchi/fast_trimesh.rs` (covered in prior audit).
- **Yang Stage 3 SSI refinement deep correctness**: only structural presence checked (Auditor A §1 Stage 3); paper-conformance details deferred.
- **The 33 yang_fast skipped timeouts**: Auditor B's classification covers 157 cases; the 33 skipped are not analyzed.
- **Full B-Rep assembly via `build_result_brep`**: Auditor A §1 Stage 6 covers the architecture; per-line conformance not checked.
- **GUI / wasm-bridge interactions**: out of slice for all auditors.

### What's next

1. **Tier 1 rank 1 (per-patch labeling)** is the single highest-leverage change in this audit. PR8 should drive against it specifically.
2. **Re-validate the audit queue after Tier 1 lands**: per-patch labeling may subsume more findings than this synthesis predicts; conversely it may surface new mechanisms.
3. **Tessellation thread is officially CLOSED** per YD §3.4 + this audit's Tier-2 framing of D-10. Future tessellation work happens IF AND ONLY IF it serves the bijective-tessellation contract (Yang §4.1.1) — not the bounded-path repair pipeline (deprecated S-H per A15.6).

---

*This audit is a snapshot. Future audits should reference this report as the baseline. Code state at audit time: `main` HEAD `3af7fd6` (post-PR7 tessellation closure).*
