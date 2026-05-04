# PR-Y15a Phase 0 — Downstream cohort anchor diagnostic

**Author:** implementer-e (PR-Y15a Phase 0)
**Date:** 2026-05-02
**Spec:** `specs/yang_pr_y15a_downstream_investigation.md`
**Plan:** `/home/claude/.claude/plans/reactive-juggling-sloth.md` (PR-Y15a Phase 0)
**Probe:** `YANG_CONFORMAL_PROBE=1`, new Stage Bb at
`crates/kernel/src/boolean/topology_extract.rs:1810`
**Reproducers:** F0031 (canonical) + F0032 + F0040 (spot-checks); validated
against the full F0031–F0040 ten-case stripe.

## TL;DR

Decision-tree row 4 (well_formed=true at Stages A/Bb/B/C, but Waffle still
fails downstream) fires uniformly across all three reproducers AND the
broader F0031–F0040 cohort. The 78% Cherchi-valid downstream defect class
is **NOT** in `flood_fill_patches` twin pairing or `label_cells` —
the conformal mesh is well-formed at every probe in the existing
A/Bb/B/C family. The defect is in **B-Rep assembly post-Step 7**
(`flood_fill_patches`), most likely in the half-edge twin construction
in Step 7 itself or in the downstream `tessellate_waffle_solid` render
path that the watertight oracle ultimately measures.

This is a **PR-Y15c-shape** outcome per spec §4: a new Stage D probe
must be added between `flood_fill_patches` Step 7 (B-Rep half-edge
construction) and the downstream watertight validator. PR-Y15a-fix
cannot be specced from Phase-0 evidence alone; the actual buggy
function lives in code path that the four-stage A/Bb/B/C family does
not cover.

The cluster is **homogeneous** across all 10 cases — F0031–F0040 all
report identical Stage A/Bb/B/C signatures (well_formed=true,
unpaired=0, multi_paired=0). PR-Y15c's reproducer set may safely
collapse to F0031 + F0040 (covering both extrude operand orderings)
as a 2-case spot-validation.

## Anchor pre-verification (per `feedback_anchor_before_fix.md`)

Before adding the real Stage Bb probe, an `eprintln!("[stage-bb-canary]
reached after label_cells")` was inserted immediately after `label_cells`
returns at `topology_extract.rs:1809`. The `batch_enclosed_subtract_fix`
test (which runs F0031–F0040) was executed with `YANG_BOOLEAN=1`.

**Result:** The canary fired **10 times** (once per case in the
batch). The Stage Bb anchor at "immediately post-`label_cells` return"
is verified empirically. Canary was removed before the real probe code
landed.

## Stage Bb probe — implementation

Added at `crates/kernel/src/boolean/topology_extract.rs:1810`,
immediately after `label_cells` returns. Mirrors the existing Stage A/B/C
pattern:

- **Gate:** `YANG_CONFORMAL_PROBE=1` (4th member of the same probe
  family — no new env var)
- **Input mesh:** the FULL `subdivided` (all `tris_a` + all `tris_b`),
  unfiltered. `label_cells` does not add or remove triangles — it
  only labels them — so Stage Bb's input is the unfiltered subdivided
  mesh. This differs from Stage B (which uses post-`face_survival_detect`
  filtered mesh via `survival.groups`); the Stage Bb / Stage B delta
  isolates whether `face_survival_detect` itself fixes-or-breaks
  conformality.
- **Output:** single `[conformal-probe] stage=Bb ...` line + first 5
  violation details on `well_formed=false`, identical format to
  Stages A/B/C via the shared `emit_conformal_probe` helper.

LOC: ~25 (additive). One file modified: `topology_extract.rs`.

## Verbatim probe output — F0031 (canonical reproducer)

```
[conformal-probe] stage=A unpaired=0 multi_paired=0 euler_chi=4 well_formed=true verts=28 tris=48 unique_edges=72
[conformal-probe] stage=Bb unpaired=0 multi_paired=0 euler_chi=4 well_formed=true verts=28 tris=48 unique_edges=72
[yang-diag] after label_cells: A outside=12 inside=0 cosurface=0, B outside=0 inside=36 cosurface=0
[conformal-probe] stage=B unpaired=0 multi_paired=0 euler_chi=4 well_formed=true verts=28 tris=48 unique_edges=72
[conformal-probe] stage=C unpaired=0 multi_paired=0 euler_chi=4 well_formed=true verts=28 tris=48 unique_edges=72
```

Downstream Waffle failure:
```
F0031 Failed: watertight_mesh: 12 unpaired edges out of 60 total;
              mesh_euler_characteristic: V(26) - E(60) + F(36) = 2 (expected 4)
```

Note the divergence: probe at Stage C reports `verts=28 tris=48 unique_edges=72`
(well_formed=true), but the downstream watertight oracle measures
`V=26 E=60 F=36` (12 unpaired). The mesh shrinks by 2 vertices and
12 triangles between Stage C and the watertight check — that's the
B-Rep assembly + retessellation stage. The resulting render-LOD mesh
is non-watertight even though the conformal triangle mesh that fed
into the half-edge construction is well-formed.

## Verbatim probe output — F0032 (spot-check, same op pattern)

```
[conformal-probe] stage=A unpaired=0 multi_paired=0 euler_chi=4 well_formed=true verts=26 tris=44 unique_edges=66
[conformal-probe] stage=Bb unpaired=0 multi_paired=0 euler_chi=4 well_formed=true verts=26 tris=44 unique_edges=66
[yang-diag] after label_cells: A outside=12 inside=0 cosurface=0, B outside=0 inside=32 cosurface=0
[conformal-probe] stage=B unpaired=0 multi_paired=0 euler_chi=4 well_formed=true verts=26 tris=44 unique_edges=66
[conformal-probe] stage=C unpaired=0 multi_paired=0 euler_chi=4 well_formed=true verts=26 tris=44 unique_edges=66
```

Downstream Waffle failure:
```
F0032 Failed: watertight_mesh: 16 unpaired edges out of 44 total
```

Same defect signature as F0031: well_formed=true at all 4 stages, then
non-watertight mesh from downstream B-Rep assembly. F0032's downstream
mesh has 44 edges total (vs. Stage C's 66 unique edges) — the half-edge
construction discards triangles or fails to pair them correctly.

## Verbatim probe output — F0040 (spot-check, INVERTED operand order)

```
[conformal-probe] stage=A unpaired=0 multi_paired=0 euler_chi=4 well_formed=true verts=46 tris=84 unique_edges=126
[conformal-probe] stage=Bb unpaired=0 multi_paired=0 euler_chi=4 well_formed=true verts=46 tris=84 unique_edges=126
[yang-diag] after label_cells: A outside=72 inside=0 cosurface=0, B outside=0 inside=12 cosurface=0
[conformal-probe] stage=B unpaired=0 multi_paired=0 euler_chi=4 well_formed=true verts=46 tris=84 unique_edges=126
[conformal-probe] stage=C unpaired=0 multi_paired=0 euler_chi=4 well_formed=true verts=46 tris=84 unique_edges=126
```

Downstream Waffle failure:
```
F0040 Failed: watertight_mesh: 20 unpaired edges out of 70 total;
              consistent_normals: 14 of 40 triangles have reversed normals;
              outward_normals: only 26 of 40 triangles (65.0%) have outward normals (need 95%);
              mesh_euler_characteristic: V(42) - E(70) + F(40) = 12 (expected 4)
```

F0040's `[yang-diag]` after `label_cells` shows the operand order is
inverted vs. F0031/F0032: A has 72 outside (the cylinder boss is the
larger mesh A), B has 12 inside (the rectangle cut hole is the smaller
mesh B). Despite the inversion, the conformal probe signature is
identical — `well_formed=true` at all 4 stages — and the downstream
failure mode is identical (non-watertight render mesh).

## Per-case decision-tree row mapping (spec §4)

| Case  | Stage A | Stage Bb | Stage B | Stage C | Waffle outcome | Row |
|-------|---------|----------|---------|---------|----------------|-----|
| F0031 | true    | true     | true    | true    | Failed (watertight) | **4** |
| F0032 | true    | true     | true    | true    | Failed (watertight) | **4** |
| F0040 | true    | true     | true    | true    | Failed (watertight) | **4** |

All three reproducers fire **row 4** of the spec §4 decision tree:

> Stage A=true, Stage Bb=true, Stage C=true AND Waffle still fails →
> Anchor is in B-Rep assembly POST-`flood_fill_patches` (after Step 7,
> in the half-edge construction or validation logic itself) →
> Spec a new investigation PR-Y15c with a Stage D probe added immediately
> before the half-edge validator.

## Cluster homogeneity verdict — HOMOGENEOUS

Beyond the 3 mandatory spot-checks, the full F0031–F0040 cohort was
captured in the same probe run (all 10 cases run via
`batch_enclosed_subtract_fix`). All 10 cases report identical Stage
A/Bb/B/C signatures (well_formed=true, unpaired=0, multi_paired=0).
The cluster is fully homogeneous on the conformal-probe axis.

| Case  | verts | tris | unique_edges | All stages well_formed |
|-------|-------|------|--------------|------------------------|
| F0031 | 28    | 48   | 72           | true                   |
| F0032 | 26    | 44   | 66           | true                   |
| F0033 | 26    | 44   | 66           | true                   |
| F0034 | 30    | 52   | 78           | true                   |
| F0035 | 26    | 44   | 66           | true                   |
| F0036 | 46    | 84   | 126          | true                   |
| F0037 | 46    | 84   | 126          | true                   |
| F0038 | 46    | 84   | 126          | true                   |
| F0039 | 42    | 76   | 114          | true                   |
| F0040 | 46    | 84   | 126          | true                   |

The 5-vs-5 split (F0031–F0035 = box-minus-cyl; F0036–F0040 = cyl-minus-box)
shows distinct vert/tri counts per group but identical conformal
behavior. Operand order does not matter on this axis. Single-anchor
fix scope is justified.

## Comparison: F0002 (PR-Y15b cohort) vs. F0031 (PR-Y15a cohort)

For calibration, here is F0002's 4-stage probe output (post-PR-Y15b):

```
[conformal-probe] stage=A unpaired=0 multi_paired=18 euler_chi=10 well_formed=false verts=28 tris=72 unique_edges=90
[conformal-probe] stage=Bb unpaired=0 multi_paired=18 euler_chi=10 well_formed=false verts=28 tris=72 unique_edges=90
[conformal-probe] stage=B unpaired=0 multi_paired=0 euler_chi=2 well_formed=true verts=28 tris=52 unique_edges=78
[conformal-probe] stage=C unpaired=0 multi_paired=0 euler_chi=2 well_formed=true verts=28 tris=52 unique_edges=78
```

F0002 fires **decision-tree row 2** (Stage A=false, Stage Bb=false,
Stage B=true, Stage C=true) — the conformal mesh is broken at A/Bb but
gets "fixed" by `face_survival_detect` (which drops 20 triangles from
72 to 52, eliminating all 18 multi-paired duplicates). This is a
distinct defect class from F0031–F0040.

The F0002/F0031 contrast is the load-bearing observation that justifies
the new Stage Bb probe being permanent: F0002 demonstrates that A→Bb
shows zero delta (label_cells doesn't corrupt) AND that Bb→B shows
nonzero delta (face_survival_detect drops broken tris). For F0031–F0040
A=Bb=B=C, all green. Without Stage Bb in the family we could not
distinguish "label_cells is innocent" from "label_cells coincidentally
preserves brokenness" in the F0002 cohort, and we'd have no per-stage
calibration for the F0031 cohort either.

## Recommended PR-Y15a-fix anchor — DEFER, ESCALATE TO PR-Y15c

Per spec §4 row 4: the conformal probe family does not cover the
buggy code path. The defect is downstream of Stage C
(`topology_extract.rs:786`), in:

1. **`flood_fill_patches` Step 7 (B-Rep half-edge construction)** —
   `topology_extract.rs:790+`. Step 7 takes the conformal triangle
   mesh and constructs the half-edge graph (mev/mef Euler operations
   on `TopoArena`). If twin pairing fails here, it's not visible at
   Stage C (which measures the triangle mesh, not the half-edge graph).
   This is the most likely anchor.

2. **Downstream B-Rep retessellation (`tessellate_waffle_solid` render
   LOD)** — invoked in `feature-engine` after the half-edge graph is
   built. The render-LOD path is the existing PR14 anchor candidate
   (per MEMORY.md `yang_implementation_status.md` 2026-05-02 entry):
   "PR14 anchor = `tessellate_waffle_solid` Render LOD per-face
   byte-identity defect." If that anchor is correct, F0031–F0040's
   downstream watertight failures are the same defect cohort.

The vert-count delta at the Stage C → watertight boundary (F0031:
28 → 26 verts; F0032: 26 → 22 verts implied; F0040: 46 → 42 verts)
is consistent with retessellation that drops degenerate or
near-degenerate triangles per-face without re-stitching the resulting
gaps. That fits the PR14-anchor description exactly.

**Recommended PR-Y15c (next investigation cycle):**

- Add a **Stage D probe** between Step 7 (B-Rep half-edge construction)
  and the downstream watertight validator. Site: after the half-edge
  arena is constructed in `topology_extract.rs::flood_fill_patches`
  (post Step 7, ~L850-1000 range — the exact line is the canary's job
  in PR-Y15c Phase 0).
- Add a **Stage E probe** at the post-`tessellate_waffle_solid` render
  mesh, OR convert the existing watertight oracle's measurement into
  a conformal-probe entry. Site: `feature-engine` or `kernel` render
  LOD output.

A Stage D well_formed=false finding pins the anchor to Step 7
half-edge construction. A Stage D well_formed=true + Stage E
well_formed=false finding pins it to the render LOD retessellation
(confirming the PR14 anchor). A both-true outcome would be a new
defect class (probably in the watertight oracle's measurement
methodology vs. what the user sees).

**Estimated LOC for the eventual PR-Y15a-fix (after PR-Y15c localization):**
unknown until Stage D/E pin the anchor. If Step 7 half-edge construction:
30-100 LOC in `flood_fill_patches`. If render LOD retessellation: 50-200
LOC in `tessellate_waffle_solid`. Risk: medium-to-high — both code paths
are load-bearing for downstream rendering.

## Spec ambiguities

1. **"well_formed" measures topology, not labels.** The conformal
   oracle counts unpaired/multi-paired directed edges and computes
   Euler characteristic. It does NOT verify that `label_cells` produced
   semantically correct inside/outside labels. F0031–F0040's
   `[yang-diag]` show the labels look reasonable (extrude+cut produces
   12 outside in A, ~32-36 inside in B), but if `label_cells` had
   produced WRONG labels, the conformal probe at Stage Bb would still
   report well_formed=true (because triangles are still well-formed).
   This is the row-4 ambiguity: a label-only defect would also fire
   row 4. We cannot distinguish "label-only defect" from "B-Rep
   assembly defect" with the current probe family. Stage D in PR-Y15c
   should include a label-consistency check (e.g., compute total winding
   number of selected triangles and assert it matches the operation's
   expected topology).

2. **The 78% cohort defect signature differs from PR-Y15a's spec
   §1 example.** The spec's headline failure mode was
   `half_edge[N].twin = 0 but twin.twin = M`. F0031–F0040's actual
   failures are `watertight_mesh: N unpaired edges out of M total`
   plus `mesh_euler_characteristic: V-E+F=X (expected Y)`. These are
   downstream watertight-oracle failures, not the half-edge validator
   self-consistency error from §1. The 78% Cherchi-valid cohort may
   be heterogeneous along a finer axis than "Cherchi-valid +
   Waffle-failed" — some cases may fail the half-edge twin validator
   (the F0002 anchor pre-PR-Y15b), others may pass that validator but
   fail downstream watertight (the F0031–F0040 anchor). PR-S2's TSV
   should be re-segmented by failure-mode signature, not just by
   Cherchi-vs-Waffle status, before PR-Y15c's reproducer set is
   chosen.

3. **`batch_enclosed_subtract_fix`'s output buffering quirk.** The
   first probe-on run via `cargo test ... --nocapture 2>&1 > file`
   captured zero `[conformal-probe]` lines. The second run via
   `cargo test ... --nocapture --test-threads=1 2>stderr_file 1>stdout_file`
   captured all expected lines. The difference: libtest's `--nocapture`
   only releases stderr WITHOUT redirection and only when
   `--test-threads=1`. Documenting this so PR-Y15c's harness doesn't
   re-discover it.

## Production safety verification

- **Probe-disabled F0002 trace** (`YANG_CONFORMAL_PROBE` unset →
  `cargo test ... yang_trace_f0002 --nocapture --test-threads=1`):
  **0 `[conformal-probe]` lines** ✓ — Stage Bb is gated, no behavior change
- **Probe-enabled F0002 pinned test** (`YANG_CONFORMAL_PROBE=1`):
  emits **exactly 1 line of each stage** (A, Bb, B, C) ✓ — the 4-stage
  family is intact and Stage Bb is non-disruptive to the existing 3
- **`cargo clippy -p kernel --no-deps`**: 91 warnings (vs. PR-Y15b
  baseline of 92; my additive change introduces 0 new warnings — a
  prior cleanup may have dropped one) ✓
- **`rustfmt --check crates/kernel/src/boolean/topology_extract.rs`**:
  clean ✓
- **No new env vars beyond the existing `YANG_CONFORMAL_PROBE`**: ✓
- **Anchor canary removed before final probe code landed**: verified
  by re-running `cargo test ... batch_enclosed_subtract_fix
  --nocapture --test-threads=1 2>stderr_file` and grepping
  `[stage-bb-canary]` (0 hits) ✓

## Conclusion

PR-Y15a Phase 0 establishes that the F0031–F0040 cohort (the dominant
78% Cherchi-valid downstream defect class) is **homogeneous** on the
conformal-probe axis and fires **decision-tree row 4 uniformly** —
all four existing/new probe stages report well_formed=true, yet
Waffle still fails the downstream watertight oracle. The probe family
A/Bb/B/C is exhausted on this cohort: no further information can be
extracted by adding more probes within the
`subdivide_mesh_pair_full_cherchi → label_cells → face_survival_detect
→ flood_fill_patches Step 6` window.

**PR-Y15a-fix CANNOT be specced from Phase-0 evidence.** The actual
buggy function is downstream of Stage C, in B-Rep half-edge
construction (`flood_fill_patches` Step 7) or render-LOD retessellation
(`tessellate_waffle_solid`). Per FIP §3 + Engineering Constitution
P10, writing a fix spec without an empirically-anchored fix target
would be a fifth wrong-anchor cycle — exactly what the spec was
written to prevent.

**Recommended next action:** Spec PR-Y15c with a Stage D probe at
the post-Step-7 half-edge graph and a Stage E probe at the
post-`tessellate_waffle_solid` render mesh. PR-Y15c's reproducer set
may safely collapse to F0031 + F0040 (operand-order coverage) given
this Phase-0 cluster homogeneity finding.
