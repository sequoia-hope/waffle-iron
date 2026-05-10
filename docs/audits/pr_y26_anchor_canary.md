# PR-Y26 Anchor Canary — UNCLASSIFIED + NEW HYPOTHESIS: dominant signature is `count=1` boundary edges (missing triangles), not (i)/(j)/(k) position-mismatch

**Author:** canary-y26
**Date:** 2026-05-08
**Plan:** `/home/claude/.claude/plans/optimized-wandering-wind.md` Phase 0 canary
**Verdict:** **UNCLASSIFIED + NEW HYPOTHESIS** — All three pre-selected candidates (i)/(j)/(k) are REFUTED by the empirical classification. The dominant unpaired-edge signature on F0020 b#2 final invocation is **`count=1` (boundary, only one incident triangle)** — 34/36 edges, with the remaining 2/36 being `count=3` zero-length-quantized degenerate edges (`qa==qb`). Zero of the 36 unpaired edges are `count=2` (which would be the position-mismatch signature predicted by candidates (i) and (j)). Zero endpoints sit in any face's inner-loop quant set (refutes (k)). Cohort F0044/F0045/R0092 reproduces the same shape: 12+38+43 unpaired edges, **all `count=1`** — the watertight defect is consistent with "missing triangle patches" across the cohort, not a per-face vertex-unification mismatch.
**Recommended next step:** **ABORT PR-Y26 as currently scoped.** The plan's three candidate fix shapes were structurally wrong — none of them explains a `count=1` signature. Re-investigate where the missing triangles' twins should have been emitted and were not. The cohort presence (F0044+F0045+R0092 each show `count=1` patterns at higher cardinality) means the underlying defect is wider than F0020 and is **not** a "single-extrude-final-invocation" oddity.

This memo names empirical evidence only. It does NOT propose fix shape; per `feedback_anchor_before_fix.md` and `feedback_phase1_diagnosis_ranking_is_inference.md`, scope decision is upstream.

---

## §0 Discipline — live tree untouched

### Live tree at session start and just before writing this memo

```
$ git -C /home/claude/workspace status
On branch main
Your branch is up to date with 'origin/main'.

nothing to commit, working tree clean
```

All probe instrumentation lives in a separate worktree at `/tmp/y26-probe-wt` rooted at `d1a301d`:

```
$ git worktree add /tmp/y26-probe-wt d1a301d
Preparing worktree (detached HEAD d1a301d)
HEAD is now at d1a301d audit(yang-pr-y25): ABORT at canary phase — H1 antecedent empirically vacuous; bank Yang §4.4.1 for PR-Y26

$ cd /tmp/y26-probe-wt && git diff --stat
 app/tests/cases/assay/results.json            |   6 +-
 crates/kernel/src/boolean/yang_integration.rs |  40 +++-
 crates/kernel/src/tessellation/mod.rs         | 302 ++++++++++++++++++++++++++
 3 files changed, 344 insertions(+), 4 deletions(-)
```

No `git stash`, `git checkout --`, `git reset --hard`, or other destructive op was used on the live working tree. Per `feedback_adversary_no_destructive_git.md`. (`results.json` mutation is the assay test runner's normal artifact; will be discarded with `git worktree remove`.)

### Probe gate

Every probe is gated on `std::env::var("Y26_PROBE").as_deref() == Ok("1")`. Default-off codepath is byte-identical to the `d1a301d` baseline. All output is `eprintln!`-only. No mutation of `vertices`, `indices`, `face_ranges`, or arena state.

### Reproduction commands

```
git worktree add /tmp/y26-probe-wt d1a301d
cd /tmp/y26-probe-wt
# (probes injected per §3 below)

# F0020 b#2 (load-bearing)
YANG_BOOLEAN=1 Y26_PROBE=1 TWIN_DEBUG=1 cargo test -p test-harness \
    --test assay_randomized -- spotlight_f0020 \
    --ignored --nocapture --test-threads=1 \
    > /tmp/y26-canary-f0020.txt 2>&1

# F0044/F0045/R0092 batch (cohort guard)
YANG_BOOLEAN=1 Y26_PROBE=1 TWIN_DEBUG=1 cargo test -p test-harness \
    --test assay_randomized -- spotlight_f0044 \
    --ignored --nocapture --test-threads=1 \
    > /tmp/y26-canary-f0044.txt 2>&1
```

Both tests reported `test result: ok. 1 passed; 0 failed`. Inner spotlight verdicts:
- **F0020:** `Status: Failed` — `watertight_mesh: 36 unpaired edges out of 130 total (34 boundary, 2 non-manifold)`
- **F0044 batch:** all three (`F0044`, `F0045`, `R0092`) `Failed` watertight (12, 38, 43 unpaired respectively)

---

## §1 F0020 spotlight — load-bearing invocation classification

The F0020 spotlight produces six `tessellate_solid_bounded` invocations across the three sequential extrudes. The load-bearing one is the **final** invocation: 169 half-edges, 130 render-mesh edges, 36 unpaired — exactly matching the spotlight oracle output and PR-Y25's measurement.

### Verbatim P1/P2 summary lines (final invocation only)

```
[y26-probe-p1-summary] total_unpaired=36 total_edges=130 tri_count=76
[y26-probe-p2-summary] total_unpaired=36 candidate_i_count=0 candidate_j_count=2 candidate_k_count=0 unclassified_count=34
[y26-probe-p3-summary] cand_i_count=0 p3_lines_emitted=0
```

### Count-multiplicity distribution of the 36 unpaired edges

| Count | # edges | Notes |
|---|---|---|
| `1` (boundary, only one incident triangle) | **34** | The dominant signature |
| `2` | **0** | Position-mismatch signature (i/j) — empty |
| `3` (non-manifold) | **2** | Both have `qa == qb` (zero-length quantized) — degenerate triangles |
| ≥`4` | 0 | — |

**Of the 34 `count=1` edges, NONE are `count=2`.** The premise of candidates (i) and (j) — that the same logical edge appears twice with slightly different positions because two faces emit it from different per-face tessellations — is empirically false on this case. There is no second emission to reconcile; the second triangle is missing entirely.

### Inner-loop classification (candidate (k))

Across all 36 unpaired edges:

```
inner_a=0  inner_b=0  inner_either=0  of 36
```

**Zero endpoints quantize into any incident face's inner-loop boundary set.** Candidate (k) (inner-loop re-emission divergence) has empirical incidence count zero on F0020 b#2.

### Source-face provenance distribution

For the 34 `count=1` edges, `face_a_id` (the face emitting the lone incident triangle) splits across the two source meshes:

```
count=1 face_a from MeshId::A: 16
count=1 face_a from MeshId::B: 18
```

Provenance coverage is 100% (no `prov_a=None` for any incident face). `face_b_id=0` for the count=1 edges is a structural property: only one triangle uses the edge, so there is no second face. **`face_b_id=0` does NOT mean "missing provenance"** — it means "no second triangle exists." The probe's per-edge sig label `boundary_only_one_tri` flags this case explicitly.

### Connected-component structure of the unpaired-edge graph (count=1 edges)

Excluding the 2 zero-length count=3 degenerate edges, the 33 non-degenerate count=1 edges form three connected components in the graph of unique quant endpoints:

```
Endpoint degree distribution: {2: 22, 4: 4, 5: 1, 1: 1}
Connected components: 3
  comp 0:  3 vertices,  3 edges  — closed 3-cycle (a triangular hole)
  comp 1: 16 vertices, 20 edges  — degree-{2:12, 4:4} (two cycles meeting at 4 nodes; figure-eight / merged-loop)
  comp 2:  9 vertices, 10 edges  — degree-{1:1, 2:7, 5:1} (open chain with one branch / star)
```

Comp 0 is a textbook "missing triangle" — three boundary edges that should have been the rim of a triangle that is not in the mesh. Comp 1 and comp 2 are larger missing-patch boundaries (with comp 2 having one open endpoint suggesting either a partial chain or a topology collision with the 2 degenerate count=3 zero-length edges).

### The 2 `count=3` zero-length-quantized edges

```
edge_idx=27  count=1  qa==qb=(50704,-18298,-19414)  face=203 prov=Some((0,10))
edge_idx=29  count=3  qa==qb=(50704,-18298,-19414)  face=203 prov=Some((0,10))
edge_idx=34  count=3  qa==qb=(65051,-15817,-36086)  face=198 prov=Some((0,4))
```

(The probe emits edges with `qa == qb` when two of a triangle's three vertices quantize into the same cell — i.e. a sub-quantum-thin / collapsed triangle.) F0020's spotlight oracle separately reports `no_degenerate_triangles: 4 of 76 triangles are degenerate`. The 2 count=3 zero-length quantized "edges" and the 4 degenerate triangles are the same defect surface.

### Verdict for plan's three candidates on F0020

| Candidate | Predicted signature on 36 unpaired | Observed | Verdict |
|---|---|---|---|
| **(i)** Yang §4.4.1 vertex non-unification (`update_mesh_along_refined_curves` per-face VertexIdx, A↔B seam crossing with f64→f32 / accumulation divergence) | `count=2` edges with prov_a/prov_b on different mesh_ids and small position delta | **0 of 36** are `count=2`; **0 of 36** are A↔B seam pairs of two emissions | ❌ **REFUTED** |
| **(j)** Earcut/Steiner-fan diagonal mismatch (within-face two emissions disagree on diagonal) | `count=2` edges with prov_a == prov_b (same source-face on both incidences) | **0 of 36** are `count=2`. The 2 `count=3` zero-length qa==qb edges have same prov but are degenerate-triangle artifacts, not earcut diagonal mismatches | ❌ **REFUTED** |
| **(k)** Inner-loop re-emission divergence | At least one endpoint quantizes into an incident face's inner-loop boundary set | **0 of 36** endpoints hit any inner-loop set | ❌ **REFUTED** |

All three are REFUTED. Per the plan's acceptance gate ("All candidates < 9/36 → UNCLASSIFIED → Investigate before specing; possibly REFUTE all three and start fresh"), **the dominant signature is something else entirely**.

### NEW HYPOTHESIS — missing-triangle (count=1 boundary)

The empirical signature of 34/36 edges is **`count=1` boundary** — a triangle that should exist on the "other side" of the edge does not exist in the render mesh at all. The 33 non-degenerate count=1 edges are not scattered: they form 3 connected loops/chains, consistent with **3 missing surface patches** in the final retessellated solid.

This is upstream of the per-face tessellator's vertex-unification logic (which only matters when both faces emit a triangle). The candidate fix shapes the plan ranked were all variations on "two emissions disagree on positions"; the actual mechanism is "one emission is absent."

Two structurally-different-but-coherent next-step hypotheses for spec-y26 to consider (NOT a fix proposal — anchor candidates for further investigation):

- **Candidate (m) — surviving-patch dropout in `flood_fill_patches` / topology layer.** Per Cherchi 2022 §3 (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:251-256`) "the arrangement is guaranteed to be a well formed simplicial complex and surface patches are bounded by closed loops of non-manifold edges". If a surface patch fails to survive Cherchi §5 inside/outside classification (or its Yang §4.4.2 NMM-aware adaptation), the resulting B-Rep face is missing entirely. The render mesh then has no triangles for that face, and the perimeter edges of that face's neighbours appear `count=1`.
- **Candidate (n) — face deletion / `face_provenance` shrinkage between topology extract and `result_topology_to_waffle_solid`.** If a face that was correctly classified inside the mesh-boolean output is dropped before the WaffleSolid is built, the render mesh has no triangles for it. The probe shows 100% provenance coverage among the *surviving* faces' edges, so any drop happened upstream of `result_topology_to_waffle_solid`'s face_map population.

The probe data does not distinguish (m) from (n); spec-y26 (or a re-investigation phase) must.

---

## §2 F0044 / F0045 / R0092 cohort batch — same `count=1` signature

The F0044 spotlight runs the F0044+F0045+R0092 batch and produces 3 `tessellate_solid_bounded` LOD=Render invocations. Verbatim P1/P2 summaries:

```
[y26-probe-p1-summary] total_unpaired=12 total_edges=180 tri_count=116
[y26-probe-p2-summary] total_unpaired=12 candidate_i_count=0 candidate_j_count=0 candidate_k_count=0 unclassified_count=12

[y26-probe-p1-summary] total_unpaired=38 total_edges=472 tri_count=302
[y26-probe-p2-summary] total_unpaired=38 candidate_i_count=0 candidate_j_count=0 candidate_k_count=0 unclassified_count=38

[y26-probe-p1-summary] total_unpaired=43 total_edges=281 tri_count=173
[y26-probe-p2-summary] total_unpaired=43 candidate_i_count=0 candidate_j_count=0 candidate_k_count=0 unclassified_count=43
```

### Per-invocation classification

| Inv | total_unpaired | total_edges | tri_count | count=1 | count=2 | count≥3 | zero-length | inner_either |
|---|---|---|---|---|---|---|---|---|
| 1 (F0044 case) | 12 | 180 | 116 | **12** | 0 | 0 | 0 | 0 |
| 2 (F0045 case) | 38 | 472 | 302 | **38** | 0 | 0 | 0 | 0 |
| 3 (R0092 case) | 43 | 281 | 173 | **43** | 0 | 0 | 0 | 0 |

**Every unpaired edge across all three cohort invocations is `count=1`. Zero are `count=2`. Zero hit inner-loop sets.** The cohort exhibits the same dominant mechanism as F0020 b#2.

### Cohort guard verdict — plan's expectation was wrong

The plan §"Cohort guard probe" expected: *"For F0044/F0045/R0092 batch: same P1+P2 enumeration. Expected: 0 unpaired edges (all batch invocations pass watertight today)."*

That expectation is **empirically wrong** and was already documented as wrong by the PR-Y25 canary (commit `42674e2`, §2 — F0044 batch's three LOD=Render invocations had 12, 38, 43 unpaired edges at baseline). PR-Y25's framing was that those failures were "pre-existing and not gated by H1"; PR-Y26's plan inherited the wrong expectation from the broader PR-Y22+PR-Y24 cohort guards (which assert `[topo-extract] unpaired=0` and `[twin-oracle] unpaired_count=0` — *topology-layer* metrics, not *render-mesh watertight*).

The cohort batch render-mesh watertight has been failing for at least the PR-Y25 baseline (`5993954`) and continues to fail at `d1a301d`. **This is a banked finding for spec-y26**: any PR-Y26 attempt that scopes itself only to F0020 will leave the cohort's identical-shape defect in place.

### Per-invocation source-mesh distribution of count=1 edges

| Inv | face_a ∈ MeshId::A | face_a ∈ MeshId::B |
|---|---|---|
| F0020 final | 16 | 18 |
| F0044-batch #1 | 12 | 0 |
| F0044-batch #2 | 15 | 23 |
| F0044-batch #3 | 34 | 9 |

The lopsided distribution in batch #1 (12/0) and #3 (34/9) suggests the missing-triangle defect is **not** a symmetric A↔B seam phenomenon. It can fall asymmetrically on one source mesh, indicating something more local than "intersection-curve seam mismatch."

---

## §3 Probe instrumentation (worktree-only, NOT committed)

Located under `/tmp/y26-probe-wt`. Three changes:

```
$ cd /tmp/y26-probe-wt && git diff --stat
 app/tests/cases/assay/results.json            |   6 +-
 crates/kernel/src/boolean/yang_integration.rs |  40 +++-
 crates/kernel/src/tessellation/mod.rs         | 302 ++++++++++++++++++++++++++
 3 files changed, 344 insertions(+), 4 deletions(-)
```

(`results.json` mutation is the runner's per-test artifact; not probe logic.)

### Block 1 — provenance plumbing in `yang_integration.rs`

A `Y26_PROVENANCE` thread-local (`BTreeMap<u64, (u8, usize)>` mapping kernel_face_id → (mesh_id_byte, face_idx_value)) populated by `result_topology_to_waffle_solid` (line 215). Recording is gated inside the helper on `Y26_PROBE=1`; non-probe runs do an env-var check only and return early.

```rust
// PR-Y26 canary thread-local: kernel_face_id -> (mesh_id_byte, face_idx_value).
std::thread_local! {
    pub(crate) static Y26_PROVENANCE: std::cell::RefCell<std::collections::BTreeMap<u64, (u8, usize)>> =
        const { std::cell::RefCell::new(std::collections::BTreeMap::new()) };
}

pub(crate) fn y26_record_provenance(kid: u64, mesh_id: MeshId, face_idx_value: usize) {
    if std::env::var("Y26_PROBE").as_deref() != Ok("1") { return; }
    /* insert into thread-local */
}
pub(crate) fn y26_clear_provenance() { /* clear */ }
pub(crate) fn y26_get_provenance(kid: u64) -> Option<(u8, usize)> { /* lookup */ }
```

And inside `result_topology_to_waffle_solid`:

```rust
let mut face_map = BTreeMap::new();
y26_clear_provenance();  // PR-Y26 canary
for &face_idx in result.face_provenance.keys() {
    let kid = id_alloc();
    face_map.insert(kid, face_idx);
    if let Some(src) = result.face_provenance.get(&face_idx) {
        y26_record_provenance(kid, src.mesh_id, src.face_idx.0);
    }
}
```

### Block 2 — canary probe in `tessellate_solid_bounded`

A `y26_emit_canary_probe(arena, face_map, &disc, &vertices, &indices, &face_ranges)` call at the very end of `tessellate_solid_bounded`, just before `Ok(RenderMesh{...})`. Gated:

```rust
if std::env::var("Y26_PROBE").as_deref() == Ok("1") {
    y26_emit_canary_probe(arena, face_map, &disc, &vertices, &indices, &face_ranges);
}
```

The probe (a) mirrors `oracle.rs:185-264` quantization (`max_abs * TAU_TESS_GRID_FACTOR`, floor `TAU_TESS_GRID_MIN`), (b) builds an `(edge → Vec<triangle_idx>)` map, (c) builds `triangle → kernel_face_id` via `face_ranges`, (d) builds per-face `inner-loop quant point set` via `arena.faces[*].inner_loops` + `disc.positions`, (e) for each unpaired (count != 2) edge, emits a `[y26-probe-p1]` line with `count`, both incident triangles' `face_id`s, both faces' SourceFace provenance, the inner-loop-hit booleans, and a candidate classification (`i`/`j`/`k`/`B` boundary-only-one-tri / `?` unclassified), (f) emits `[y26-probe-p2-summary]` rolling counts and `[y26-probe-p3-position]` lines for any cand-i edges (none observed).

The probe does NOT mutate any state. It is `eprintln!`-only. Lives only in worktree; not committed to the live tree.

---

## §4 Verdict against plan acceptance gate

Plan §"Phase 0 canary" → "Acceptance gate":

| Outcome | Plan-defined verdict | Observation | Result |
|---|---|---|---|
| One candidate ≥ 27/36 (75%) | DOMINANT — spec proceeds | (i)=0, (j)=2, (k)=0 | not satisfied |
| Two candidates each ≥ 9/36 (25%) | LAYERED — spec picks simpler, banks other | (i)=0, (j)=2, (k)=0 | not satisfied |
| All candidates < 9/36 | UNCLASSIFIED — investigate before specing | (i)=0, (j)=2, (k)=0 | **MET** |
| **Any new mechanism appears that doesn't fit (i)/(j)/(k)** | **NEW HYPOTHESIS — document and bring to user** | 34/36 are `count=1 sig=boundary_only_one_tri`; 2/36 are `count=3` qa==qb degenerate; the dominant mechanism is "missing triangles" not "two emissions disagree" | **MET** |

**UNCLASSIFIED + NEW HYPOTHESIS (both fire).** Per plan: investigate before specing AND bring the new mechanism to the user for scope decision.

### Cohort guard verdict

Plan §"Cohort guard probe" expected zero unpaired in F0044 batch. Observation: 12+38+43 unpaired, all `count=1`. The cohort guard's expected baseline was wrong (the cohort has been failing watertight at least since PR-Y25's `5993954` baseline). **The defect is wider than F0020.** Banked finding for whatever phase replaces PR-Y26 as scoped.

---

## §5 Recommendation to spec-y26

1. **ABORT PR-Y26 as currently scoped.** The plan's three candidate fix shapes (i)/(j)/(k) all assume `count=2` position-mismatch. The empirical dominant signature is `count=1` (no twin emission at all), with the cohort exhibiting the same shape. Specing any of the three banked fixes around an empirically vacuous antecedent would repeat PR-Y25's exact mistake — the failure mode that spawned `feedback_phase1_diagnosis_ranking_is_inference.md`.
2. **Re-investigate missing triangles.** The next investigational step is to identify *where* in the pipeline a triangle that should exist on the other side of these `count=1` edges fails to be emitted. Two seed candidates worth probing (NOT proposed as fix shapes; only as next-investigation anchors):
   - (m) — surface-patch dropout in `flood_fill_patches` / Yang §4.4.2 NMM-aware patch labeling (Cherchi 2022 §3 simplicial-complex closure)
   - (n) — face deletion between Yang topology-extract and `result_topology_to_waffle_solid` consumption
3. **Cohort framing.** Any next investigation must be cohort-scoped (F0020 + F0044/F0045/R0092 minimum), not F0020-only. The same-shape signature across 4 cases means this is a structural defect, not a per-case configuration.
4. **PR-Y26 plan's "expected: 0 unpaired in cohort"** was wrong on entry. Future plan acceptance gates should query the actual baseline cohort metric and not assume cohort passes.

This memo names empirical evidence; it does NOT propose a fix shape. Spec-y26 (or a replacement investigation phase) selects the next anchor.

---

## §6 Banked findings

- F0020 b#2 final invocation has `4 of 76 triangles degenerate` (oracle output) and `2 of 36 unpaired edges with qa == qb` (probe). These are the same defect surface — a small number of sub-quantum triangles. Independent of the missing-triangle phenomenon but worth fixing as a separate small concern (non-blocking on the watertight count).
- F0020 b#2 also has `consistent_normals: 2 of 76 triangles have reversed normals` and `no_self_intersection: 10 inter-face triangle penetrations` (oracle output). These are *additional* defects the spotlight reports beyond watertight; PR-Y26's plan §"Anti-scope" already flags self-intersection (10), normals (2), degenerate (4) for F0020 as banked, and that classification still stands — not in scope here.
- F0044 cohort's render-mesh watertight failures were silent in PR-Y22+PR-Y24 cohort guards because those guards measured topology-layer `[topo-extract]` and `[twin-oracle]` (per-half-edge directional-asymmetry), not render-mesh position-keyed pairing. The render-mesh layer has been silently failing for these cases since PR-Y25's `5993954` baseline at minimum. Worth a separate cohort-shaped investigation independent of PR-Y26.
- The probe shows 100% source-face provenance coverage among surviving faces (no `prov_a=None` for any incident face). This is consistent with the missing-triangle hypothesis: the *surviving* face set is intact and has correct provenance metadata; the question is whether some faces that should be in the surviving set were dropped.

---

## §7 Reproduction artifacts

- Probe stdout: `/tmp/y26-canary-f0020.txt` (1357 lines), `/tmp/y26-canary-f0044.txt` (5250 lines)
- Probe instrumentation diff: `/tmp/y26-probe-wt` worktree, 344 lines additive across 2 files (excluding `results.json`)
- Will be discarded by `git worktree remove /tmp/y26-probe-wt` at close-out

End of memo.
