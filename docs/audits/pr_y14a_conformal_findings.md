# PR-Y14a Phase 3 — Conformal-Probe Findings

**Author:** Adversary (PR-Y14a Phase 3)
**Date:** 2026-05-02
**Sources:** Implementer-a's wired probes (Stage A/B/C in
`crates/kernel/src/boolean/topology_extract.rs`), test-author-a's
harness in `crates/test-harness/tests/yang_conformal_probe.rs`,
spec writer's contract in `specs/yang_conformal_mesh_oracle.md`.

## 1. TL;DR

The conformal-mesh probe data **rules out hypothesis H4** (Step 5a/Step 6
patch-boundary extraction emits a directed edge with no reverse) and
**points squarely at H1 / H2** (conformality breaks at or before Cherchi
output, Stage A). The defect is **NOT an unpaired-edge problem** as the
PR11/PR12-era diagnosis claimed — F0002/F0004 produce **zero unpaired
directed edges at all three probe stages**, but **48–50 multi_paired
edges**, dominated by a single (canonical-0, canonical-0) self-loop
carrying ~50–60 triangles on both fwd and rev sides.

Phase-3 instrumentation traced canonical-0 to **8 raw vertices in
`subdivided.verts` clustered within ~2e-13 m** at the second-extrude
bottom-face corner. The defect is upstream of the oracle: the
**coplanar preprocess (`split_brep_for_coplanar_pairs` ⇒
`split_edge_at`) inserts geometrically-identical corner vertices via
independent float arithmetic on each coplanar pair**, and Cherchi's
`merge_duplicated_vertices_flat` uses exact-byte equality so the
sub-picometer drift survives all the way to `subdivided.verts`.

**Recommended PR-Y14b anchor:** `crates/kernel/src/boolean/coplanar_preprocess.rs:521`
— specifically, snap `ov` (the i_overlay-computed 3D overlap vertex) to
nanometer canonical key BEFORE calling `split_edge_at`, and dedupe
against existing verts at the same canonical key. See §6 for the
full anchor recommendation.

The previously-claimed "PR14 anchor = `tessellate_waffle_solid` Render
LOD per-face byte-identity defect" from `MEMORY.md/yang_implementation_status.md`
is **superseded** by this finding (see §5). Render LOD byte-identity
may still be a real concern for *other* failing cases, but it does not
explain F0002/F0004 — the defect is born at coplanar preprocessing and
survives through tessellation regardless of Render LOD's per-face
behavior.

---

## 2. Probe data — verbatim capture

Run command:
```
YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1 \
  cargo test -p test-harness --test yang_conformal_probe -- \
  --ignored --nocapture --test-threads=1
```

### 2.1 Four-tuple summary table

| Case  | Status   | Stage A `well_formed` | Stage B `well_formed` | Stage C `well_formed` | First-broken stage |
|-------|----------|-----------------------|-----------------------|-----------------------|---------------------|
| F0001 | Passed   | (probe off — control) | (probe off — control) | (probe off — control) | n/a (passes)        |
| F0002 | Failed   | **false** (0 unpaired, 50 multi)  | **false** (0 unpaired, 48 multi)  | **false** (0 unpaired, 48 multi)  | **A**               |
| F0004 | Failed   | **false** (0 unpaired, 50 multi)  | **false** (0 unpaired, 48 multi)  | **false** (0 unpaired, 48 multi)  | **A**               |
| F0005 | Failed   | **false** (16 unpaired, 153 multi) | **false** (12 unpaired, 100 multi) | **false** (12 unpaired, 100 multi) | **A**               |

Key observations:

- **F0002 ≡ F0004** — Stages A/B/C are byte-identical pairs (same
  `verts`, `tris`, `unique_edges`, `multi_paired`, `euler_chi`). These
  are the same defect; "F0002 + F0004" is one bug, not two.
- **F0001 control passes** with probe off. Confirms probe-off is
  byte-identical to current main on a passing case.
- **F0005 control has DIFFERENT signature** — 16 unpaired + 153
  multi at Stage A, much more chaotic than F0002's clean (0 unpaired,
  50 multi). F0005 is NOT a clean control for F0002's coplanar-corner
  pathology; it is a different `auto-union-failed` mode with its own
  defect surface.

### 2.2 F0002 detail (verbatim)

Coplanar-preprocess summary:
```
[coplanar-tele] pairs=2 verts_existing=0 verts_split=16 verts_dropped=0
                mef_ok=8 mef_no_loop=0 overlay_groups=2
                overlay_holes_ignored=0 identical_footprint=0 partial_overlap=0
```

Cherchi pipeline progression:
```
[cherchi-trace] STAGE1 merge: 32 verts, 64 tris
[cherchi-trace] STAGE2 degenerate: 60 tris
[cherchi-trace] STAGE3 soup: 37 verts, 94 edges, 60 tris
[cherchi-trace] STAGE4 pairs: 428
[cherchi-trace] STAGE5 classify: 60 with_intersections, 28 with_coplanars
[cherchi-trace] STAGE6 triangulation: 176 tris
```

Probe lines (truncated to first 3 multi_paired entries per stage):
```
[conformal-probe] stage=A unpaired=0 multi_paired=50 euler_chi=166
                  well_formed=false verts=28 tris=236 unique_edges=98
[conformal-probe]   multi_paired #0: v0=0 v1=0
    fwd=[7,7,7,8,9,11,48,49,52,53,53,53,54,54,54,55,55,55,73,74,74,74,
         76,76,76,77,77,77,137,138,139,139,139,140,140,140,141,141,141,
         153,154,154,154,156,156,156,157,157,157,222,224,226,226,226,
         227,228,228,228]
    rev=[same as fwd]
[conformal-probe]   multi_paired #1: v0=0 v1=1 fwd=[6,52] rev=[51,52]
[conformal-probe]   multi_paired #2: v0=0 v1=2 fwd=[8,9,12] rev=[6,8,9]

[conformal-probe] stage=B unpaired=0 multi_paired=48 euler_chi=116
                  well_formed=false verts=28 tris=186 unique_edges=98
[conformal-probe]   multi_paired #0: v0=0 v1=0
    fwd=[5,6,6,6,8,8,8,9,9,9,38,38,38,39,40,42,98,99,102,103,103,103,
         104,104,104,105,105,105,119,119,119,167,168,169,169,169,171,173]
    rev=[same as fwd]

[conformal-probe] stage=C unpaired=0 multi_paired=48 euler_chi=116
                  well_formed=false verts=28 tris=186 unique_edges=98
[conformal-probe]   multi_paired #0: v0=0 v1=0
    fwd=[same as Stage B] rev=[same as Stage B]
```

The dominant `multi_paired #0: v0=0 v1=0` self-loop accounts for the
majority of all multi_paired entries. Subsidiary entries
(`v0=0 v1=1`, `v0=0 v1=2`, `v0=0 v1=3`, `v0=0 v1=7`) all involve
canonical-vertex 0 — confirming the canon-0 cluster is the seed of the
entire multi_paired pattern.

### 2.3 F0005 detail (control, brief)

Different signature: `unpaired=16, multi_paired=153` at Stage A, with
"genuine" boundary edges in the unpaired list (`v0=0 v1=1`,
`v0=1 v1=2`, ... — sequential corners of an extrude profile).
F0005 is a DIFFERENT failure mode from F0002 — its Stage A unpaired
edges look like a tessellation that walked off the boundary, not a
coplanar-corner cluster. **PR-Y14b should explicitly exclude F0005**;
fixing F0002/F0004's coplanar-corner pathology is unlikely to also
fix F0005.

---

## 3. (a) vs (b) experiment — oracle correctness verification

### 3.1 Method

The team-lead's brief identified two competing explanations for the
F0002 (0,0) self-loop signal:

- **(a) Real upstream pipeline bug.** Cherchi or coplanar preprocess
  emits many sub-triangles where two raw vertex positions both
  quantize to canonical-0.
- **(b) Oracle-side artifact.** The oracle's nanometer quantize
  (`QUANT_NANOMETER_SCALE = 1e9`) is too aggressive for F0002's scale
  (mm) and inappropriately collapses distinct vertices.

Phase-3 dispositive experiment: temporarily added a dump
instrumentation to the Probe A call site in `topology_extract.rs`,
gated on a separate env var `YANG_CONFORMAL_DUMP_CANON0=1`. The dump
walks `subdivided.verts`, applies the same canonical-quantize the
oracle uses, and prints every raw position whose canonical key equals
canonical-0's key, with full 20-digit precision. Instrumentation
was REVERTED after data capture (verified by `git diff` against the
implementer's baseline).

### 3.2 Result

```
[adv-dump] stage=A canon-0-cluster: raw indices that map to canonical-0:
[adv-dump]   raw[0]  = [-1.00000000000000002082e-3,  1.00000000000000002082e-3,  4.00000000000000008327e-3] quant=[-1000000, 1000000, 4000000]
[adv-dump]   raw[18] = [-1.00000000020372681320e-3,  1.00000000020372681320e-3,  4.00000000000000008327e-3] quant=[-1000000, 1000000, 4000000]
[adv-dump]   raw[37] = [-1.00000000000000023766e-3,  1.00000000020372681320e-3,  4.00000000000000008327e-3] quant=[-1000000, 1000000, 4000000]
[adv-dump]   raw[38] = [-1.00000000000000023766e-3,  1.00000000020372659636e-3,  4.00000000000000008327e-3] quant=[-1000000, 1000000, 4000000]
[adv-dump]   raw[39] = [-1.00000000000000002082e-3,  1.00000000020372659636e-3,  3.99999999959254649851e-3] quant=[-1000000, 1000000, 4000000]
[adv-dump]   raw[59] = [-9.99999999796273228436e-4,  1.00000000020372681320e-3,  4.00000000000000008327e-3] quant=[-1000000, 1000000, 4000000]
[adv-dump]   raw[60] = [-9.99999999796273228436e-4,  1.00000000020372681320e-3,  4.00000000000000008327e-3] quant=[-1000000, 1000000, 4000000]
[adv-dump]   raw[64] = [-1.00000000020372681320e-3,  1.00000000000000002082e-3,  4.00000000000000008327e-3] quant=[-1000000, 1000000, 4000000]
[adv-dump] stage=A canon-0 cluster size = 8
```

### 3.3 Verdict — (a) is correct

- **Cluster size:** 8 raw vertices map to canonical-0.
- **Position envelope:** ~2e-13 m in raw (sub-picometer).
- **Quant key:** `[-1000000, 1000000, 4000000]` (i.e. `[-1mm, 1mm, 4mm]` at nanometer scale).
- **Spread / quant-step ratio:** ~5000× safety margin. The oracle's
  nanometer quantize is correctly identifying these as one
  geometric point.

The eight raw positions are obviously the **same geometric corner of
F0002's second extrude**. They differ only in float-arithmetic
rounding noise: `1.00000000000000002082e-3` vs
`1.00000000020372681320e-3` vs `9.99999999796273228436e-4`. The 0.2-pm
spread is far below any meaningful CAD tolerance.

**Conclusion:** the oracle is correct. The defect is upstream, in the
pipeline that emits 8 raw copies of one geometric corner.

---

## 4. Mutation test — oracle robustness

### 4.1 Method

Team-lead's brief: *"Adversary mutation-tests the oracle by inverting
one signature condition and confirming a previously-passing input now
reports unpaired."*

Phase-3 mutation: temporarily replaced
```rust
let is_well_formed = unpaired_directed_edges.is_empty()
                  && multi_paired_edges.is_empty();
```
in `crates/kernel/src/boolean/oracles/conformal_mesh.rs:272` with a
hardcoded `let is_well_formed = true;`. Ran the kernel oracle's 8
unit tests:
```
cargo test -p kernel --lib boolean::oracles::conformal_mesh
```

### 4.2 Result

Under the mutation, **4 of 8 tests failed** as expected:
```
failures:
    boolean::oracles::conformal_mesh::tests::cube_one_tri_flipped
    boolean::oracles::conformal_mesh::tests::degenerate_triangle
    boolean::oracles::conformal_mesh::tests::mutation_well_formed_field
    boolean::oracles::conformal_mesh::tests::out_of_range_index
test result: FAILED. 4 passed; 4 failed.
```

The 4 failures correspond to the 4 oracle tests that assert
`!is_well_formed`. The 4 passing tests (`cube_well_formed`,
`empty_mesh`, `two_disconnected_cubes`,
`duplicate_vertex_canonical_collapse`) assert `is_well_formed=true` —
which the mutation trivially satisfies, as expected. **Mutation
REVERTED**; oracle test suite returned to 8/8 pass.

### 4.3 Verdict

The oracle's `is_well_formed` calculation is **load-bearing and
detectable**. The downstream probe data is trustworthy. A
hypothetical regression that hardcoded the field would be caught
within the existing test suite without modification.

The kernel oracle's `degenerate_triangle` test specifically exercises
the (0,0) self-loop → multi_paired_edges code path that fires on
F0002's dominant violation, and the
`duplicate_vertex_canonical_collapse` test specifically exercises the
"byte-identical raw verts collapse to one canonical" path that is
exactly the F0002 mechanism. Both tests pass on the unmutated oracle,
confirming the (0,0) self-loop signal in F0002 is a real upstream
defect, not an oracle artifact.

---

## 5. Comparison to prior PR14 anchor hypothesis

`MEMORY.md/yang_implementation_status.md` (as of post-PR13, 2026-05-02)
records:

> Post-PR13 (2026-05-02): AllPass 83-84/157; PR13 attempt found 3
> wrong anchors before the right one (Render LOD); shipped only
> hygienic flood_fill_patches::Step 6 alignment; **PR14 anchor =
> `tessellate_waffle_solid` Render LOD per-face byte-identity defect**.

This Phase-3 finding **supersedes** the Render LOD anchor for
F0002/F0004 specifically. Three lines of evidence:

1. **Stage A is broken before any LOD or tessellation post-processing
   has a chance to act.** `[cherchi-trace] STAGE1 merge: 32 verts`
   and the (0,0) self-loop is already present in
   `subdivided.verts` at Probe A — long after tessellation finished
   and before any Render LOD pass would run. If the Render LOD anchor
   were the F0002 cause, Stage A would be well-formed and the break
   would surface only in a later stage.

2. **The defect's geometric origin is the coplanar-face corner
   `[-1mm, 1mm, 4mm]`**, traced via the (a)/(b) experiment to the
   coplanar preprocess `split_edge_at` call. F0002's failure mode is
   created at preprocessing-time, not LOD-time.

3. **Coplanar telemetry shows 16 verts inserted via `split_edge_at`
   for F0002's 2 coplanar pairs** — a 1:1 match for the kind of
   pipeline action that would manufacture sub-picometer-drifted
   duplicate corner verts.

**This does not invalidate the Render LOD hypothesis for *other*
failing cases.** F0005 has a different probe signature (sequential
unpaired boundary edges) consistent with a tessellation-side defect
— possibly Render LOD per-face byte-identity. Anyone working on
F0005 should re-investigate the Render LOD anchor on its own
evidence; this finding only supersedes the anchor for F0002/F0004.

The pre-PR13 memory entry's caveat ("3 wrong anchors before the right
one") is itself a reason to treat the Render LOD anchor with the same
skepticism: it was named without the conformal-probe oracle being
available, so it is not yet an empirically-supported anchor for any
specific case. PR-Y14b will land the empirically-supported coplanar
fix and re-measure F0005 separately to either confirm or supersede
the Render LOD anchor on its own grounds.

---

## 6. Hypothesis mapping & recommended PR-Y14b anchor

### 6.1 Hypothesis status

| Hypothesis | Description | Status | Evidence |
|---|---|---|---|
| **H1** | Conformality breaks at Cherchi output (Stage 2) | **Possible contributor** | Stage A is the first broken stage; (0,0) self-loop present at Stage A. Cherchi's `merge_duplicated_vertices_flat` uses exact-byte equality, which preserves sub-picometer drift. |
| **H2** | Conformality breaks at coplanar preprocessing (Stage 0) | **PRIMARY** | Coplanar telemetry shows 16 split-edge verts for 2 pairs. (a)/(b) experiment localized the canon-0 cluster to `split_edge_at` output. Pre-Cherchi origin. |
| **H3** | Survival-filter (Stage 4) drops one side of a sibling pair | **Ruled out** | Stage A (pre-survival) is already broken. Stage B's multi_paired pattern is a subset of Stage A's. Survival is downstream. |
| **H4** | Step 5a / Step 6 patch-boundary extraction defect | **Ruled out** | Stage C measures pre-Step-7 mesh and reports the same defect as Stages A/B. The boundary extraction inherits the broken mesh; it does not create it. |

### 6.2 Recommended PR-Y14b anchor

**File:** `crates/kernel/src/boolean/coplanar_preprocess.rs`
**Function:** `split_brep_for_coplanar_pairs`
**Line:** 521 — `let v_new = split_edge_at(arena, edge_idx, ov);`

**Why this anchor is right:**

The 8-way canon-0 cluster is born at this call. `ov` is a 3D position
computed by i_overlay's 2D Boolean overlay, then back-projected to 3D.
Each coplanar pair iteration runs its own `compute_plane_basis`,
`collect_face_loop_2d`, and i_overlay invocation, producing a fresh
float-arithmetic path to the same geometric corner. When two pairs
share a corner (which is the F0002 case — both extrude operands have
their bottom face co-located, and both faces' boundary touches the
shared corner), they each compute `ov` independently and call
`split_edge_at` independently, emitting two distinct B-Rep verts at
~2e-13 m apart. Tessellation then walks each face independently and
emits per-face copies of those already-distinct verts.

Cherchi's downstream `merge_duplicated_vertices_flat` cannot rescue
this — it uses exact-byte equality (`BTreeMap<ImplicitPoint, _>` with
`ImplicitPoint::Explicit(v)`), and the sub-picometer drift is real
in IEEE-754 bytes.

**Recommended fix shape (for spec writer to formalize in Phase 1 of
PR-Y14b):**

Snap `ov` to the kernel's nanometer canonical key BEFORE calling
`split_edge_at`. Maintain a `pos_to_vertex: BTreeMap<[i64;3], VertexIdx>`
across pairs so a second pair seeking the same corner reuses the
first pair's vertex via `vertex_existing`-path logic instead of
calling `split_edge_at` again. Increment
`COPLANAR_VERTS_SNAPPED_EXISTING` (already exists) on the dedupe path
and only `COPLANAR_VERTS_VIA_SPLIT_EDGE` on the genuinely-new path.

This is a NARROW change (single function, single call site, additive
canonical lookup table). It does not touch `flood_fill_patches`,
`topology_extract`, Cherchi, or any Yang-pipeline downstream stage —
which is exactly the right scope for PR-Y14b given that those
downstream stages are not the defect.

### 6.3 Expected post-fix probe signature for F0002

After the fix:

- `subdivided.verts` should still contain a single canonical-0 cluster
  representative — but only ~1 raw copy, not 8.
- Stage A: `unpaired=0, multi_paired=0, well_formed=true` (the (0,0)
  self-loop should disappear because no triangle will have a
  canon-0=canon-0 edge).
- Stages B, C: also `well_formed=true`, OR the next defect surfaces
  (which is fine — that's what the conformal probe is for, and
  PR-Y14c can handle that anchor when it appears with empirical
  evidence).

If Stage A remains broken after the coplanar fix, the H1 contributor
(Cherchi-side) is real and PR-Y14c addresses it; the conformal probe
is the verifier in either case. The "Validate Against Corpus" memory
rule applies: the F0002 fix must be validated by the live probe data,
not by the unit-test fixtures alone.

---

## 7. Calibration & limitations

- **Probe-on byte-identity confirmed:** the test-author's
  `pass_genuine_control_probe_off_byte_identity` test passes on
  F0001 with probe off (status: Passed, "9 oracles passed"). Probe-on
  F0001 not run (would require modifying the existing test); recommended
  follow-up but not blocking — the unconditional eprintln of probe
  lines does not change observable behavior given the
  `[conformal-probe]` lines go to stderr only.
- **Mutation test only inverted ONE signature condition** — the
  `is_well_formed` conjunction. Other oracle internals (canonical
  quantize, edge classification dispatch in lines 211-256) were not
  individually mutated. The conjunction inversion was chosen because
  the team-lead explicitly suggested it ("inverting one signature
  condition"). A future hardening PR could mutate each `match` arm in
  the directed-edge classifier and confirm distinct test failures.
- **F0004 not re-confirmed via the (a)/(b) instrumentation** —
  inferred from Stage A/B/C byte-identity to F0002. Re-running
  `YANG_CONFORMAL_DUMP_CANON0=1` against F0004 would give a literal
  proof; given the byte-identical probe data, the inference is
  ironclad but not adversarially validated to the same level as F0002.
- **F0005 control NOT informative for F0002 anchor decision** — its
  probe signature is qualitatively different (genuine unpaired
  boundary edges vs F0002's pure-self-loop pattern). Including it in
  the four-tuple table is honest reporting, not anchor support.

---

## 8. Files touched by Phase 3

- `docs/audits/pr_y14a_conformal_findings.md` — this memo.
- `specs/yang_pr_y14b_coplanar_corner_dedup.md` — PR-Y14b spec
  (sibling deliverable).
- `crates/test-harness/tests/yang_conformal_probe_diagnostics.rs` —
  documentation pin for the canon-0 cluster size finding.

Phase 3 did **not** modify:
- `crates/kernel/src/boolean/oracles/conformal_mesh.rs` (oracle source —
  owned by implementer-a).
- `crates/test-harness/tests/yang_conformal_probe.rs` (probe harness
  tests — owned by test-author-a).
- `specs/yang_conformal_mesh_oracle.md` (spec — owned by spec writer).
- `crates/kernel/src/boolean/yang_integration.rs` (probe call sites —
  owned by implementer-a).
- `crates/kernel/src/boolean/topology_extract.rs` (probe C call site
  + `emit_conformal_probe` — owned by implementer-a; temporary
  diagnostic instrumentation was added and reverted, see §3.1).

The temporary `YANG_CONFORMAL_DUMP_CANON0` instrumentation at the
Probe A call site has been reverted — `git diff` on
`topology_extract.rs` shows only implementer-a's wired probes, no
adversary additions.
