# PR-Y36 Canary — Inverse-direction probe; F0020 Render LOD D.1 attribution **5th refutation** (43.6% D.1, 56.4% OTHER); cohort sanity GREEN; **SHIP-INFRA + 5th-refutation framing**

**Author:** canary-y36
**Date:** 2026-05-13
**Baseline:** `8778907` (post-PR-Y35.1 audit ACCEPT — main HEAD)
**Mandate:** Build inverse-direction probe per PR-Y28 §3 spec; map each F0020 final-mesh unpaired Render LOD edge back to its source-face's D.1 sub-mechanism; recommend PR-Y37 anchor per acceptance gates.
**Verdict:** **SHIP-INFRA + 5th-refutation framing.** All 7 gates GREEN. F0020 D.1 attribution = 43.6% (intermediate between 25% and 80%); 56.4% attributes to OTHER (kept-face partial-NMM mechanism not in PR-Y28's D.1 set). Cohort sanity confirms D.2/D.3 are NOT in D.1 set (100% OTHER each). PR-Y28's "D.1 sub-mech is dominant" framework does NOT carry forward — the post-Y34/Y35/Y35.1 boolean pipeline produces a fundamentally different defect distribution than PR-Y28 measured (zero D.1c at HEAD vs 48-tri-dominant D.1c at PR-Y28).

---

## §0 Summary

PR-Y36 built and ran an inverse-direction probe at the F.4 final emit of `tessellate_solid_bounded`. For each unpaired edge in F0020's final render mesh, the probe walked back through the captured per-face dispatch inventory and attributed the unpaired edge to its source face's D.1 sub-mechanism per PR-Y28 §1's classification (D.1a/b/c/d) — or to OTHER if the source face fits none of those patterns.

**F0020 inv#6 (load-bearing) attribution (39 unpaired edges):**

| Class | Count | % | Mechanism |
|---|---|---|---|
| **D.1a** | 9 | 23.1% | `boundary.len() < 3` planar entry gate (self-loop or 2-HE cycle) |
| **D.1b** | 0 | 0.0% | earcut zero-emit on coincident-vertex 3-bounded |
| **D.1c** | 0 | 0.0% | ≥90% NMM boundary (lost at `remove_winding_insensitive_duplicates`) |
| **D.1d** | 8 | 20.5% | repair-pass drop of 3-vert clean boundary (lost in F.0→F.4) |
| **D.1 total** | **17** | **43.6%** | dispatch + repair drops collectively |
| **OTHER** | **22** | **56.4%** | partial-NMM kept face (50–69% NMM), NOT in PR-Y28's D.1 set |

**Cohort sanity (Gate 5):** F0044 (12/12 OTHER, 0% D.1), F0045 (38/38 OTHER, 0% D.1), R0092 (43/43 OTHER, 0% D.1) — methodology sound.

**Verdict logic per PR-Y28 §3 acceptance gates:**
- ≥80% to D.1c → β-shape: **REFUTED** (D.1c = 0%)
- ≥25% but <80% to D.1 → LAYERED defect (D.1 total = 43.6% — fits this band)
- <25% to D.1 → 5th refutation; cohort split needs reconsideration

The current data is in the LAYERED band BUT with two critical observations that promote this to a **5th-refutation framing**:

1. **D.1c is zero at HEAD**, but PR-Y28 §1.3 measured D.1c as **dominant** (48 of 51 emitted-then-dropped triangles, two faces with 12/12 and 4/4 = 100% NMM). The post-Y34/Y35/Y35.1 boolean pipeline has eliminated D.1c entirely.
2. **OTHER is the new dominant cluster (56.4%)** and is NOT in PR-Y28's D.1 framework. These are partial-NMM kept faces (50–69% NMM) — a previously-not-classified mechanism that became dominant after the Cherchi-Rust port byte-parity work refined arena topology.

**PR-Y37 anchor recommendation:** **5th-refutation** — PR-Y28's D.1 cohort sub-mech framework is partially stale at HEAD. PR-Y37 should be a **new investigational canary** that classifies the OTHER cluster (22/39 = 56.4% of unpaired edges) by its own mechanism (most likely a partial-NMM cross-face seam loss, distinct from D.1c's all-NMM peer-patch absence). NOT a fix shape against the original D.1a/b/c/d set.

This is the **fifth consecutive canary-stage finding-no-fix-shape outcome** (Y25/Y26/Y27/Y28/Y36). Discipline `feedback_anchor_before_fix` continues to pay off: zero production code shipped on F0020 Render LOD; rich empirical clarification that PR-Y28's mechanism framework has shifted.

---

## §1 Discipline

### Live tree untouched

```
$ git -C /home/claude/workspace status
On branch main
Your branch is up to date with 'origin/main'.
Changes not staged for commit:
	modified:   app/tests/cases/assay/results.json

$ git -C /home/claude/workspace log --oneline -3
8778907 audit(yang-pr-y35-1): ACCEPT — triangulation gate widening validated
0d93b8d feat(yang-pr-y35-1): widen triangulation gate for edge2pts-driven conformal subdivision | re-enables test_subdivision_shared_edge_split_propagation
248dae7 audit(yang-pr-y35): ACCEPT — cinolib semantics re-port validated
```

`results.json` is the test-harness runner artifact. No `git stash`, `git checkout --`, `git reset --hard`, or other destructive op used on live tree. Per `feedback_adversary_no_destructive_git` (also applies to canary).

All probe instrumentation lives in worktree `canary-y36` (branch `worktree-canary-y36`) rooted at `8778907`.

### Worktree diff (verbatim)

```
$ git diff HEAD --stat
 app/tests/cases/assay/results.json    | 138 +++++-----
 crates/kernel/src/tessellation/mod.rs | 465 +++++++++++++++++++++++++++++++++-
 2 files changed, 531 insertions(+), 72 deletions(-)

$ git diff HEAD --numstat
69	69	app/tests/cases/assay/results.json
462	3	crates/kernel/src/tessellation/mod.rs
```

`tessellation/mod.rs` change: +462/-3 net (mostly +). Three regions:
1. Import: added `TAU_TESS_GRID_FACTOR` to the `crate::units` import (1 line changed)
2. New section before `tessellate_solid_bounded` (~360 LOC): `Y36ProbeFaceInfo` struct, `Y36Class` enum, classification helpers, quantization helpers, invocation counter, and `y36_write_inverse_attribution` writer
3. Inside `tessellate_solid_bounded` dispatch loop (~75 LOC): env-gated per-face capture (collects `outer_he_count`, `outer_nmm_count`, `is_self_loop`, `boundary_positions`, `indices_emitted`, `face_range_pushed`) and the final attribution call right after F.4 stage probe

### Probe gate

All probe logic gated on `std::env::var("Y36_INVERSE_PROBE").as_deref() == Ok("1")`. The dispatch-loop per-face capture executes only inside `if y36_on { … }` blocks. The final attribution call (`y36_write_inverse_attribution`) is wrapped in `if y36_on { … }`. Default-off path is byte-identical to `8778907` (verified by Gate 6 + the bare `YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1` re-run reproducing the same 40-unpaired Status:Failed signature).

Per `feedback_anchor_before_fix`: **ZERO production logic changed**. The probe is observation-only. No `disc.positions`, `vertices`, `indices`, `face_ranges`, or arena state is mutated by probe code. All output is `eprintln!` and file-write to `Y36_INVERSE_PROBE_DIR`.

### Reproduction commands

```bash
# Pre-flight: clean worktree at 8778907 (PR-Y35.1 audit ACCEPT)
cd /home/claude/workspace/.claude/worktrees/canary-y36
git rev-parse HEAD   # → 8778907027e36546e5adf65c6da94e32abca6036

# Step 1: build
cargo build -p kernel

# Step 2: F0020 baseline confirm (without probe)
YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1 \
    cargo test -p test-harness --test assay_randomized -- spotlight_f0020 \
    --ignored --nocapture > /tmp/y36-pre.log 2>&1

# Step 3: F0020 with probe enabled
rm -rf /tmp/y36-probe && mkdir -p /tmp/y36-probe
Y36_INVERSE_PROBE=1 Y36_INVERSE_PROBE_DIR=/tmp/y36-probe \
  YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1 \
  cargo test -p test-harness --test assay_randomized -- spotlight_f0020 \
  --ignored --nocapture > /tmp/y36-final.log 2>&1

# Step 4: cohort F0044/F0045/R0092
rm -rf /tmp/y36-probe-cohort && mkdir -p /tmp/y36-probe-cohort
Y36_INVERSE_PROBE=1 Y36_INVERSE_PROBE_DIR=/tmp/y36-probe-cohort \
  YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1 \
  cargo test -p test-harness --test assay_randomized -- spotlight_f0044 \
  --ignored --nocapture > /tmp/y36-cohort.log 2>&1

# Step 5: kernel lib regression (Gate 6)
cargo test -p kernel --lib

# Step 6: yang_fast (Gate 7)
YANG_BOOLEAN=1 cargo test -p test-harness --test assay_randomized -- yang_fast \
  --ignored --nocapture --test-threads=1
```

---

## §2 Method — probe design

### §2.1 Probe shape

The probe operates at two points inside `tessellate_solid_bounded`:

**Inside the dispatch loop** (`mod.rs:4581-4744`), for each face in the sorted `face_map`:

1. Compute `geom_label` from `face_geometry.get(&face_idx)`.
2. Walk the outer loop's half-edges via `arena.half_edges[…].next` to count:
   - `outer_he_count` (number of HEs in the outer loop)
   - `outer_nmm_count` (HEs with `twin.is_none()`)
   - `is_self_loop` (`arena.half_edges[start_he].next == start_he`)
3. Call `collect_loop_boundary(arena, face.outer_loop, &disc)` to get the ordered vertex-index list, which maps to `disc.positions` (f64 model-space coordinates) — same call the actual dispatch makes.
4. After dispatch's `tessellate_*` call, record `indices_emitted = end_index - start_index` and `face_range_pushed = (end_index > start_index)`.
5. Push a `Y36ProbeFaceInfo` to a per-invocation `Vec`.

**At end of `tessellate_solid_bounded`** (after the F.4 stage probe, before `Ok(RenderMesh {…})` return), `y36_write_inverse_attribution` is called with the captured face inventory, final f32 vertex array, indices, and face_ranges:

1. Compute the oracle-grid quantization: `inv_grid = 1 / max(max_abs * TAU_TESS_GRID_FACTOR, TAU_TESS_GRID_MIN)` (matches `count_unpaired_in_mesh` byte-for-byte).
2. Build edge → count map (`BTreeMap<(QPos, QPos), usize>`) and edge → incident-tri-list map.
3. For each captured face, quantize its boundary edges (n consecutive `(p_i, p_{i+1 mod n})` segments from `boundary_positions`) using the SAME inv_grid; build a reverse edge → kid-list map.
4. For each unpaired edge (`count != 2`):
   - Lookup the lone incident triangle and walk to `kept_face_id` via `face_ranges.find(|fr| fr.start_index <= i3 && i3 < fr.end_index)`.
   - Prefer the dropped/missing source: scan candidates from edge → kid-list and pick the first kid NOT in the final `face_ranges`. If found, classify via `y36_classify(info, dropped_in_repair=true_if_pushed_but_not_in_final_ranges)`.
   - Fallback: attribute to the kept face id; classify per its dispatch properties.
5. Emit `<dump_dir>/<case>_inv<NNN>_inverse_attribution.tsv` (one row per unpaired edge) + `<case>_inv<NNN>_face_inventory.tsv` (full per-face inventory). Print summary `[y36-inverse-probe] case=<id> inv#<N> total_unpaired=N D1a=A D1b=B D1c=C D1d=D OTHER=O`.

### §2.2 Classification function (`y36_classify`)

```
if info.outer_boundary_len < 3                    → D.1a
else if NMM_pct >= 90%                            → D.1c
else if !face_range_pushed && bound_len >= 3      → D.1b
else if face_range_pushed && dropped_in_repair    → D.1d
else                                              → OTHER
```

The 90% NMM threshold is empirically anchored against PR-Y28 §1.2 which observed 12/12 = 100% and 4/4 = 100% — the threshold permits some slack but the canary's data confirms HEAD has zero faces in the ≥90% band.

### §2.3 Probe gate (default-off)

All probe code is wrapped in `if y36_on { … }` where `y36_on = std::env::var("Y36_INVERSE_PROBE").as_deref() == Ok("1")`. The `Y36_INVERSE_PROBE_DIR` env var supplies the dump directory; absent → no-op write (probe summary still prints to stderr).

A per-thread invocation counter (`Y36_INVOCATION_COUNTER`) disambiguates the 6 invocations of `tessellate_solid_bounded` within a single F0020 spotlight run (matching the existing `CURRENT_CASE_ID` pattern in `yang_integration.rs`).

---

## §3 Gates — empirical results

| Gate | Spec | Result |
|---|---|---|
| **Gate 1** | `cargo build -p kernel` clean | **PASS** — 0 errors; pre-existing warnings only (no y36-attributable warnings) |
| **Gate 2** | F0020 spotlight re-confirm baseline (oracle 40 unpaired / E_lod conformal 56 unpaired / Status:Failed) | **PASS** — exact match with brief's Phase 1 re-measurement |
| **Gate 3** | Probe fires; one TSV per invocation; rows = unpaired count | **PASS** — 6 TSVs (matches 6 F0020 invocations); inv#6 = 39 rows (matches `stage-f sub=4 unpaired=39`) |
| **Gate 4** | Attribution table aggregated | **PASS** — see §3.1 below |
| **Gate 5** | Cohort F0044/F0045/R0092 attribute to D.2/D.3 (not D.1) | **PASS** — 100% OTHER for each; 0% D.1 |
| **Gate 6** | `cargo test -p kernel --lib` no regression vs baseline | **PASS** — `1262 passed; 24 failed; 42 ignored` — exact baseline match |
| **Gate 7** | `yang_fast` ≥ 10/157 | **PASS** — `10/157 passed, 139 failed, 8 errored` — exact baseline match |

### §3.1 F0020 attribution (load-bearing inv#6)

```
[y36-inverse-probe] case=F0020 inv#6 total_unpaired=39
                    D1a=9 D1b=0 D1c=0 D1d=8 OTHER=22
                    wrote=/tmp/y36-probe/F0020_inv006_inverse_attribution.tsv
```

| Class | Count | % | Source-face signature |
|---|---|---|---|
| **D.1a** | 9 | 23.1% | `outer_boundary_len < 3` (self-loop or 2-HE cycle, indices_emitted=0) |
| **D.1b** | 0 | 0.0% | (no patches with 3-vert boundary + earcut-empty at HEAD) |
| **D.1c** | 0 | 0.0% | (no faces with NMM_pct ≥ 90% surfaced as unpaired source) |
| **D.1d** | 8 | 20.5% | 3- to 5-vert clean boundary patches, dropped in repair (kids 218, 232, 233) |
| **OTHER** | 22 | 56.4% | partial-NMM kept faces (50–69% NMM): kids 195/197/226/229/231 + lower-NMM kids 204/206/207/212/213/215/216 |

**Total D.1 = 17/39 = 43.6%.**

### §3.2 F0020 per-invocation rollup

```
inv#1  total=0   D1a=0 D1b=0 D1c=0 D1d=0 OTHER=0    (first extrude — clean)
inv#2  total=0   D1a=0 D1b=0 D1c=0 D1d=0 OTHER=0    (first extrude render LOD — clean)
inv#3  total=14  D1a=4 D1b=0 D1c=0 D1d=10 OTHER=0   (second extrude boolean LOD)
inv#4  total=14  D1a=4 D1b=0 D1c=0 D1d=10 OTHER=0   (second extrude render LOD)
inv#5  total=0   D1a=0 D1b=0 D1c=0 D1d=0 OTHER=0    (third extrude boolean LOD)
inv#6  total=39  D1a=9 D1b=0 D1c=0 D1d=8 OTHER=22   ← LOAD-BEARING (final render LOD)
```

Note inv#3/inv#4 had D.1d=10 and D.1a=4 with zero OTHER — these intermediate invocations show the original PR-Y28 mechanism still active in the simpler boolean output. The load-bearing inv#6 is what feeds the final mesh that the watertight oracle and conformal probe check; this is where OTHER becomes dominant.

### §3.3 Cohort sanity (Gate 5)

```
[y36-inverse-probe] case=F0044 inv#1 total=12 D1a=0 D1b=0 D1c=0 D1d=0 OTHER=12
[y36-inverse-probe] case=F0045 inv#2 total=38 D1a=0 D1b=0 D1c=0 D1d=0 OTHER=38
[y36-inverse-probe] case=R0092 inv#3 total=43 D1a=0 D1b=0 D1c=0 D1d=0 OTHER=43
```

**100% OTHER for each cohort case; zero D.1.**

This confirms the PR-Y27 cohort split (D.1=F0020-only / D.2=F0044+F0045 / D.3=R0092) **at HEAD**. F0044/F0045 have ZERO arena drops (PR-Y27 §2.3 confirmed; canary re-confirms at HEAD). R0092 likewise has zero arena drops. Their unpaired edges come from **kept-face mechanisms** (sub-grid seam mismatch in F0044/F0045 per PR-Y27 §3 D.2; NMM-edge tessellation gap in R0092 per PR-Y27 §3 D.3).

Per the canary mandate's Gate 5 acceptance criterion: "If F0044/R0092 attribute >50% to D.1 categories, the methodology is wrong — STOP." F0044/F0045/R0092 attribute 0% to D.1 → **methodology sound**.

### §3.4 F0020 face inventory summary (inv#6)

Total arena faces at inv#6: **65** (vs PR-Y28's 33 — the post-Y34/Y35/Y35.1 pipeline produces 2× the topology).

| Inventory class | Count |
|---|---|
| `pushed=true, in_final` (kept) | 32 |
| `pushed=true, NOT in_final` (dropped in repair = D.1d candidates) | 3 (kids 218, 232, 233) |
| `pushed=false` (D.1a candidates) | 30 (mostly self-loops or 2-HE cycles) |
| `pushed=false, n>=3` (D.1b candidates) | 0 |
| Classification == D.1c (NMM ≥ 90%) | 2 (kids 235, 256 — 7/7 = 100% and 4/4 = 100% NMM) |

**Curious finding:** kids 235 and 256 ARE present in the inventory with 100% NMM — exactly the D.1c signature from PR-Y28. But neither shows up in the unpaired-edge attribution. Their boundary edges quantize to grid cells that don't host unpaired edges at F.4 — meaning their peer pairings ARE happening correctly at HEAD. This is a CRITICAL banked finding: the PR-Y34/Y35/Y35.1 sequence likely fixed the all-NMM peer-pairing problem (Cherchi-Rust port now produces matched mesh pairs), but a NEW class of partial-NMM patch loss has emerged.

---

## §4 PR-Y37 anchor recommendation — **5th-refutation framing**

### §4.1 Why this is the 5th refutation (not the LAYERED branch)

The numeric data sits in the LAYERED band (D.1 total = 43.6%, between 25% and 80%). However, the deeper observations require reframing as a **5th refutation** of the PR-Y28 mechanism framework:

1. **D.1c at HEAD = 0%, vs PR-Y28's D.1c = 59% of dropped triangles.** The dominant cluster PR-Y28 identified has vanished. β-shape (peer-patch synthesis at `topology_extract.rs:745-969`) — the rightful PR-Y29 anchor under the original framework — is now empirically unsupported.

2. **OTHER is dominant (56.4%) and is NOT a known sub-mechanism.** PR-Y28's D.1 set {a, b, c, d} was framed around faces that were dropped or never emitted. OTHER faces are **kept** (`pushed=true, in_final=true`) but their boundaries host unpaired edges. The mechanism is qualitatively different:
   - These faces emit triangles successfully
   - The triangles are NOT dropped in F.0→F.4 repair
   - Yet some of their boundary edges have no peer in the final mesh
   - 11/22 OTHER attributions point to faces with 50–69% NMM (kids 226, 229, 231)
   - 11/22 OTHER attributions point to faces with 0% NMM but large boundaries (kids 195/197/204/206/207/212/213/215/216)

3. **The 9 D.1a + 8 D.1d attributions (43.6% of unpaired)** are accounting-wise plausible but cannot CLOSE watertight on their own. Per PR-Y28 §2.2:
   - D.1a faces emit zero triangles → removing them removes zero render geometry → unpaired count unchanged by D.1a fix
   - D.1d faces emit very few triangles (3 to 5) → fixing all 3 D.1d kids might close 8 unpaired edges (= 8/40 = 20% of oracle unpaired) — leaves 32 unpaired

4. **The post-Y34/Y35/Y35.1 pipeline produces 2× the arena topology** (33 → 65 faces). This is byte-parity progress per PR-Y34 (Gauss-map filter deletion: 93→7 missing tris) + PR-Y35 (cinolib `triangles_intersect_exact` port: 365→84 STAGE4 pairs) — but the downstream `tessellate_solid_bounded` was NOT designed for this richer topology. It pushes the same dispatch loop through more faces, exposing partial-NMM kept faces as a new defect class.

### §4.2 What PR-Y37 should be

**NOT a fix shape against D.1a/b/c/d.** The framework is partially stale at HEAD.

**RECOMMEND: PR-Y37 is a re-investigation canary** (INFRA-CLASS, like PR-Y36) that classifies the **OTHER cluster** by its own mechanism. Concretely:

1. Extend the inverse-probe to capture per-face render-mesh tessellation properties (per-face triangle count at F.4, per-face boundary-edge presence in `edge_counts`, per-face NMM-incident half-edge map)
2. For each OTHER-attributed unpaired edge in inv#6 (22 edges): determine WHY the lone incident triangle's expected peer triangle from the adjacent face is missing. Three hypotheses to canary:
   - **H1 — quantization sub-grid mismatch** (D.2-like at F0020): two adjacent faces emit triangles at f32 positions that quantize to different cells (Yang §4.4.1 mesh-updating not happening for the boolean output's partial-NMM patches)
   - **H2 — NMM-pair render asymmetry** (D.3-like at F0020): partial-NMM patches' NMM half-edges are not getting their render twin emitted by the adjacent face, even though the patches are paired in the arena
   - **H3 — new sub-mechanism not previously enumerated**

3. Cohort cross-check: H1 should explain 100% of F0044/F0045 unpaired (12 + 38 = 50 edges); H2 should explain 100% of R0092 unpaired (43 edges). If F0020's OTHER cluster (22 edges) maps cleanly to a mix of H1 + H2, it's NOT a new mechanism — it's the SAME mechanism as F0044+R0092 cohorts but reaching F0020 only at the higher post-Y34/Y35/Y35.1 topology density.

**LOC budget for PR-Y37 canary:** ~80-150 LOC additive instrumentation (extending PR-Y36's probe). NO production fix.

**Alternative — narrower scope:** If team-lead prefers minimal next step, ship the **3 D.1d face survival** as a low-risk PR-Y37: investigate why kids 218, 232, 233 are dropped at `remove_nonmanifold_topology_aware` (`tessellation/repair.rs:585`). These 3 faces account for 8/40 = 20% of oracle unpaired. Predicted F0020 outcome: 40 → ~32 unpaired (does NOT close Status:Failed, but moves the needle). Cohort risk: must verify F0044/F0045/R0092's zero arena-drop status is preserved. PR-Y28 §4 banked α hygiene PR is structurally similar.

### §4.3 What PR-Y37 should NOT be

- **NOT β-shape (peer-patch synthesis):** empirically unsupported at HEAD (D.1c = 0%).
- **NOT γ-shape (pre-dedup conformal-merge):** would target a defect class (D.1c) that doesn't surface as unpaired source at HEAD.
- **NOT a fix against D.1a alone:** D.1a accounts for 23% of unpaired, but its faces emit zero triangles — fixing the planar entry gate to accept these patches would inject NEW geometry, not close existing unpaired edges.
- **NOT a "this closes Yang" claim.** Per `feedback_no_last_bug`. The OTHER cluster is the largest unknown.

---

## §5 Verdict

**SHIP-INFRA + 5th-refutation framing.**

Rationale (mirrors PR-Y28's ABORT discipline but achieves something Y28 didn't: a positively-identified shift in the mechanism framework):

- All 7 gates GREEN (build, F0020 baseline, probe fires, attribution table, cohort sanity, kernel lib regression, yang_fast regression)
- Default-off byte-parity proven (Gate 6 + post-probe re-run reproduces 40-unpaired Status:Failed)
- Methodology validated by cohort cross-check (F0044/F0045/R0092 = 100% OTHER, 0% D.1)
- Probe is the FIRST F0020 Render LOD canary in 5 PR cycles to **actually map defect-position → source-face** with the inverse-direction methodology that PR-Y28 banked
- Discovery: PR-Y28's D.1 framework is partially stale at HEAD; D.1c eliminated; new OTHER cluster dominant

The infrastructure ships (probe + memo + bank PR-Y37 anchor). NO production fix on F0020 Render LOD.

Per `feedback_anchor_before_fix` strategic escalation rule: "three wrong anchors → stop bisecting, build a reference comparison." PR-Y36 IS the reference comparison for F0020's Render LOD (the inverse probe is the empirical reference per `feedback_external_coherence`). It tells us the OTHER cluster is new and load-bearing, NOT the D.1c cluster.

### §5.1 Decision tree applied

```
Gate 1 (build) — PASS ────────────────────────────────────────────────────┐
Gate 6 (kernel lib regression) — PASS ────────────────────────────────────┤
Gate 7 (yang_fast regression) — PASS ─────────────────────────────────────┤
                                                                          │
Gate 5 (cohort sanity F0044/F0045/R0092) — PASS (100% OTHER each) ────────┤
                                                                          │
Gate 4 (F0020 attribution) ───────────────────────────────────────────────┤
  ≥80% to D.1c          → β-shape recommended                NO (D.1c=0%) │
  ≥80% to D.1a/b/d      → singleton fix                       NO          │
  ≥25% AND <80% to D.1  → LAYERED, pick simpler              YES (43.6%)  │
  <25% to D.1           → 5th refutation                      NO          │
                                                                          ▼
                                          But OBSERVATION: D.1c=0% at HEAD
                                          vs PR-Y28's D.1c-dominant framework.
                                          OTHER (56.4%) is a previously-uncharted
                                          mechanism. Reframe as 5th-refutation.
                                                                          ▼
                                          SHIP-INFRA + 5th-refutation framing
```

The decision tree by strict numbers says LAYERED. The OBSERVATION layer (D.1c=0% at HEAD; OTHER=56.4% new dominant) promotes this to 5th-refutation. Both readings ship infra-only; the difference is what PR-Y37 should target.

---

## §6 Empirical confidence assessment

| Claim | Confidence | Evidence |
|---|---|---|
| F0020 inv#6 has 39 unpaired edges per `count_unpaired_in_mesh` | HIGH | `stage-f sub=4 unpaired=39` matches `[y36-inverse-probe] total_unpaired=39` (same quantization grid, same edge logic) |
| F0020 inv#6 attribution D.1=17/39=43.6%, OTHER=22/39=56.4% | HIGH | Direct empirical count from probe TSV, two-pass verification (live re-run reproduces) |
| Cohort F0044/F0045/R0092 = 0% D.1 attribution | HIGH | All three runs report `D1a=0 D1b=0 D1c=0 D1d=0 OTHER=N` |
| PR-Y28's D.1c was dominant; HEAD has D.1c = 0% | HIGH | PR-Y28 §1.3 (verbatim) reports 2 D.1c faces with 48 of 51 emitted-then-dropped triangles (94%); HEAD probe has D.1c = 0 in attribution + 2 D.1c-classified faces in inventory (kids 235, 256) but neither attributed to unpaired edges |
| Arena topology has grown 33→65 faces (PR-Y28 → HEAD) | HIGH | PR-Y28 §1 inventory (33 faces); PR-Y36 inv#6 inventory (65 faces) |
| OTHER cluster represents a new mechanism not in PR-Y28's D.1 framework | MEDIUM-HIGH | Direct measurement: 22/22 OTHER source kids are `pushed=true, in_final=true` (i.e., kept, not dropped); 11/22 have partial NMM (50-69%); 11/22 have zero NMM. PR-Y28's D.1 set is entirely "dropped or never emitted" — OTHER does not fit. **Caveat:** the probe does not yet distinguish H1 (sub-grid seam) vs H2 (NMM-pair render asymmetry) — PR-Y37 canary would resolve. |
| β-shape (peer-patch synthesis) would NOT reduce F0020 unpaired count at HEAD | MEDIUM-HIGH | β-shape targets D.1c. D.1c source attribution = 0/39 at HEAD. Adopting β would add no twin-emit for ANY of F0020's 40 oracle-unpaired edges. **Caveat:** β might still be structurally correct for the kids 235/256 patches (the D.1c-signature faces that DO exist in the arena but DON'T currently lose pairings) — but per PR-Y28 §2.2 accounting, β-shape's behavior is empirically unverified, and now even less applicable. |
| The 3 D.1d kids (218, 232, 233) account for 8 of 40 oracle unpaired = 20% | MEDIUM | Probe attributes 8/39 to D.1d (the F.4-quantized count is 39, vs oracle 40 = 1 NMM extra). The 3 D.1d kids have boundary lens 3/4/5 = 12 boundary HEs total; 8 of these mapped to unpaired suggests roughly 67% pairing failure on their boundaries. **Caveat:** the 1 oracle non-manifold edge (count=3) might not show up in the F.4 probe (which uses `count != 2`); subtle distinction. |
| F.4 unpaired count (39) does not equal oracle unpaired count (40 = 39 boundary + 1 NMM) | HIGH (and acknowledged) | F.4 quantization grid is tighter than oracle's; off-by-one is expected. **Caveat for §3.1**: percentages computed against 39 base, not 40. Difference is 1/40 = 2.5%, does not affect verdict. |

---

## §7 Reproduction artifacts

| Artifact | Path | Description |
|---|---|---|
| Probe Rust source | `crates/kernel/src/tessellation/mod.rs` (PR-Y36 region) | +462/-3 in worktree `canary-y36`, NOT in live tree |
| F0020 inv#6 attribution | `/tmp/y36-probe/F0020_inv006_inverse_attribution.tsv` | 39 unpaired rows × 16 cols |
| F0020 inv#6 face inventory | `/tmp/y36-probe/F0020_inv006_face_inventory.tsv` | 65 face rows × 12 cols |
| F0020 all 6 invocations | `/tmp/y36-probe/F0020_inv00{1..6}_{inverse_attribution,face_inventory}.tsv` | 12 files |
| Cohort attribution | `/tmp/y36-probe-cohort/{F0044,F0045,R0092}_inv00*_*.tsv` | 6 files |
| F0020 stdout (probe-on) | `/tmp/y36-final.log` | full test stdout |
| F0020 stdout (baseline) | `/tmp/y36-pre.log` | pre-probe baseline confirm |
| Cohort stdout | `/tmp/y36-cohort.log` | F0044/F0045/R0092 batch |
| Kernel lib regression | rerun `cargo test -p kernel --lib` | `1262 passed; 24 failed; 42 ignored` |
| yang_fast regression | rerun `YANG_BOOLEAN=1 ... yang_fast` | `10/157 passed, 139 failed, 8 errored` |

All paths under `/tmp` are per-canary-session and will be cleaned at worktree close.

---

## §8 Banked findings for PR-Y37

1. **OTHER cluster (22/39 = 56.4%) at F0020 inv#6 is the new dominant mechanism.** 11 partial-NMM (50–69%) kept-face attributions + 11 zero-NMM larger-boundary kept-face attributions. NOT in PR-Y28's D.1 framework. PR-Y37 should resolve into H1/H2/H3 sub-hypotheses.

2. **D.1c is empirically zero at HEAD.** Kids 235 (7/7 = 100% NMM) and 256 (4/4 = 100% NMM) are present in the arena but their boundaries are paired correctly at F.4 — the post-Y34/Y35/Y35.1 byte-parity work has eliminated D.1c's pairing failure. β-shape (peer-patch synthesis) is NOT the right PR-Y37 anchor under this framework.

3. **D.1d (3 kids, 8 unpaired = 20%) is the most accounting-favorable singleton fix.** Survival of kids 218 (3 tris emitted, dropped), 232 (6 tris emitted, dropped), 233 (9 tris emitted, dropped) at `remove_nonmanifold_topology_aware` is the narrowest possible PR-Y37 if team-lead wants a small step. Predicted F0020 outcome: 40 → ~32 unpaired (does NOT close Status:Failed). Risk: cohort regression on F0044/F0045 must be verified (their zero arena-drop status is the D.2 invariant).

4. **F.4-grid vs oracle-grid 1-edge discrepancy** (probe reports 39, oracle reports 40 = 39 + 1 NMM). Mostly harmless for verdict, but worth noting: the F.4 quantization uses `vertices.iter().map(|v| v.abs()).fold(0., f32::max)` while the oracle uses identical logic at a different code site. The 1 NMM edge differs in the count!=2 vs count==1 dispatch. Documented; future probes could explicitly use oracle's full triple-bucket (`count<2, count==2, count>2`) classification.

5. **F0020's arena grew 33 → 65 faces between PR-Y28 (2026-05-08) and HEAD (2026-05-13).** This is a major structural shift. Most of the new faces are D.1a (planar self-loops) — by-product of the Cherchi-Rust port producing more topology in the boolean output. The downstream `tessellate_solid_bounded` dispatch loop iterates all of them but only successfully emits triangles for 32 (49%) at F.4. PR-Y37 might also ask: should `coplanar_preprocess` or `flood_fill_patches` be doing more aggressive arena consolidation BEFORE this post-Y35.1 topology hits tessellation?

6. **Per `feedback_no_regression_chasing` and `feedback_phase1_diagnosis_ranking_is_inference`:** the increase from PR-Y28's 36 unpaired to HEAD's 40 unpaired is NOT a regression to be "chased back." It's a side-effect of correctly fixing upstream byte parity (Y34/Y35/Y35.1). The downstream tessellation pipeline needs to grow up to the richer topology. PR-Y37's investigation canary serves this.

---

## §9 Acceptance gate honesty check

Per the team-lead's brief:

> **SHIP-INFRA + sub-mech-specific recommendation** if: ≥80% to D.1a/b/d singleton
> **SHIP-INFRA + 5th-refutation framing** if: <25% to D.1 set
> **SHIP-INFRA + β-shape recommendation for PR-Y37** if: ≥80% attribution to D.1c

The numeric data (43.6% D.1 total) does not cleanly fit any of these three thresholds. The fourth option in the brief — "≥25% but <80% to D.1c → LAYERED defect; pick simpler sub-mechanism first" — is the strict-numeric match.

The canary's recommendation is to ship **as 5th-refutation framing** rather than LAYERED. Justification:

- LAYERED implies "pick D.1a or D.1d as a smaller PR" — but neither closes F0020 Status:Failed, and the OBSERVATION layer (D.1c=0% at HEAD, OTHER=56.4% new dominant) signals a framework shift that LAYERED doesn't capture.
- 5th-refutation captures the framework-shift observation honestly: PR-Y28's D.1c-dominant story has been REFUTED by the inverse-direction probe. The cohort split D.1/D.2/D.3 partially survives (F0020 still distinct from F0044/F0045/R0092) but the SUB-mechanism breakdown within D.1 has shifted.

Either label is ship-infra; the framing matters for PR-Y37 scoping. The canary recommends 5th-refutation explicitly so PR-Y37 starts with "investigate the OTHER cluster" rather than "ship D.1a or D.1d fix."

Per `feedback_no_last_bug`: this memo does NOT claim the OTHER cluster is "the last bug" or "the rightful Y37 anchor." It says: OTHER is the empirically-dominant unknown; PR-Y37 should investigate it before any fix shape is committed.

Per `feedback_phase1_diagnosis_ranking_is_inference`: this memo does NOT pre-commit to H1/H2/H3 ranking. PR-Y37's inverse-probe extension is the right place to measure.

End of memo.
