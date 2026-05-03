# PR-Y14c — Cherchi-internal LPI canonicalization for F0002/F0004 corner cluster

> ## ⚠️ SUPERSEDED — do not implement this spec
>
> **Superseded by:** PR-S3 (commits land 2026-05-03+):
> - `specs/yang_pr_y15b_pre_cherchi_input_validation.md` — F0002-class
>   tessellation/coplanar fix (covers the 13% `combined_failures` minority)
> - `specs/yang_pr_y15a_downstream_investigation.md` — Phase-0
>   investigation for the 78% downstream cohort
>
> **Empirical evidence for the supersession:**
> `docs/audits/pr_s2_inputcheck_corpus_findings.md` (PR-S2 commit `aee34ce`).
> The Cherchi 2022 sidecar's corpus-wide `mesh_booleans_inputcheck` sweep
> across all 190 assay cases (380 case-sides) reports:
> - 295 sides (78%) are Cherchi-VALID (manifold + watertight + intersection-free)
> - 51 sides (13%) `combined_failures` (the F0002-class minority this spec
>   targeted)
> - **284 rows in the "interesting cell" — Waffle=Failed × Cherchi=valid**
>
> The dominant defect (78% cohort) is DOWNSTREAM of
> `subdivide_mesh_pair_full_cherchi`, not at Cherchi-internal LPI
> canonicalization as this spec assumed. Cherchi happily accepts
> Waffle's input and produces a labeled patch set; Waffle's own
> post-Cherchi half-edge reconstruction (`flood_fill_patches` and twin
> pairing in `topology_extract.rs`) then breaks. PR12/PR13's
> `flood_fill_patches::Step 6` instinct was correct in spirit; just
> at the wrong concrete site, and the prior PRs failed because their
> reproducers (R0020/R0021) were F0002-class outliers, not the
> dominant cohort.
>
> **What was right about this spec:** the F0002-specific LPI cluster
> finding from PR-Y14a §11 is real — the corner cluster IS produced by
> Cherchi internals. PR-Y15b inherits that anchor for the F0002-class
> minority. **What was wrong:** assuming the F0002 finding generalized
> to the corpus.
>
> Body preserved below for audit trail. Do NOT implement.

---

**Original status (pre-supersession):** SPEC (FIP §3.2 — Phase 1).
**Original anchor empirical evidence:** `docs/audits/pr_y14a_conformal_findings.md` §11
(post-implementer-b correction).
**Reference parity required:** YES. Per CLAUDE.md commit `4808f2e` and
the strategic-escalation rule (3 wrong anchors before, see §7), this
PR's correctness criterion includes Cherchi 2022 C++ sidecar
differential-test confirmation. Build deferred to PR-Y14d if not
already landed at PR-Y14c spec time; see §10.

---

## 1. Goal

Eliminate the 8-vertex canonical cluster in F0002 and F0004's
post-Cherchi `subdivided.verts`. The proximate cause is a mix of:

- **6 vertices** from `ImplicitPoint::LPI` materializations in
  `intersection_class.rs:454/494/532` — three sites that emit
  symbolic edge×edge / edge×triangle intersection points which
  later get evaluated to `[f64;3]` via independent rational/float
  arithmetic, producing sub-picometer drift between
  geometrically-identical points.
- **2 vertices** from a non-dedup-canonicalized writer to
  `verts_a`/`verts_b` between `dedup_mesh_vertices` exit and
  `subdivide_mesh_pair` entry — most likely
  `inject_partial_overlap_mesh` partial-mutate before its counter-
  increment guard. The exact source is one of the PR-Y14c
  implementer's first investigation tasks (see §6.7).

User-visible outcome: F0002 and F0004 either pass under
`YANG_BOOLEAN=1`, OR fail at a strictly later pipeline stage with
the conformal probe reporting `well_formed=true` at Stage A.
(Spec mirror of PR-Y14b §1; the goal didn't change, only the
anchor.)

## 2. Parameters

This is a bug fix; no new user-facing parameters. Implementation
parameters:

| Parameter | Default | Source |
|---|---|---|
| Canonical-quantize scale | `crate::units::QUANT_NANOMETER_SCALE` (= `1e9`) | reuse |
| Per-call LPI canonical-key map | empty `BTreeMap<[i64;3], usize>` per `solve_intersections` invocation | new |

No `Cargo.toml` changes. No new env vars (the fix is unconditional).

## 3. Branch Table

PR-Y14c modifies vertex-creation logic at TWO sites. Each site adds
ONE new sub-branch.

### Site 1: Cherchi LPI dedup (primary, addresses 6 of 8 cluster members)

In `intersection_class.rs`, three functions emit `ImplicitPoint::LPI`:
`add_edge_cross_edge_inters` (~L437), `add_edge_cross_edge_inters_with_tri`
(~L484), `add_edge_cross_tri_inters` (~L523). Each currently calls
`aux.add_vertex_in_sorted_list(lpi.clone(), pos)` for symbolic dedup,
then `ts.add_impl_point(lpi)` if new.

| Branch | Condition | Behavior | Counter |
|---|---|---|---|
| **A — symbolic-equal LPI** (existing) | `add_vertex_in_sorted_list` returns `is_new=false` | reuse `existing_id` | (no counter) |
| **B — new symbolic LPI, NEW canonical key** (modified) | `is_new=true` AND `materialize(lpi).quantize()` not in per-call LPI canon-key map | call `ts.add_impl_point(lpi)`; record `quantize → returned_id` | new counter `CHERCHI_LPI_CREATED` |
| **C — new symbolic LPI, REPEAT canonical key** (NEW) | `is_new=true` AND canonical key was already inserted by an earlier LPI in the same `solve_intersections` call | reuse the earlier-recorded `id`; do NOT call `ts.add_impl_point`; ALSO update `aux.add_vertex_in_sorted_list` so future symbolic-equal queries also hit | new counter `CHERCHI_LPI_DEDUPED_BY_CANON_KEY` |

The dedup map lives in `solve_intersections` (one per call) and is
threaded through `classify_intersections` and
`triangulation_with_parents` into `intersection_class.rs`. Pass it
as a `&mut` reference; do NOT use thread-local state.

### Site 2: Pre-subdivide vert audit (secondary, addresses 2 of 8 members)

The exact source of the 2 Explicit cluster members is not yet pinned
(see §6.7). PR-Y14c implementer's first action is to add temporary
instrumentation in `inject_partial_overlap_mesh` (and
`inject_identical_footprint_mesh` as a safety check) to count `verts_a.push`
and `verts_b.push` calls per pair, then run F0002 to confirm whether
those functions are the source. If yes, the second branch addition
is:

| Branch | Condition | Behavior | Counter |
|---|---|---|---|
| **D — partial-overlap mid-mutation guard** (NEW, conditional on §6.7 finding) | A `verts_*.push` would produce a position whose canon key already exists in `verts_*` | reuse the existing index; do NOT push | new counter `INJECT_OVERLAP_VERT_DEDUPED` |

If §6.7 reveals a different source, branch D's location adapts. The
spec writer's job is to amend §3 with the actual location once the
investigation pinpoints it.

## 4. Invariants

**I1 — F0002 post-Cherchi canon-0 cluster size = 1.** After Cherchi's
`solve_intersections` returns, no two distinct entries in
`result.coords` share the same nanometer canonical key for F0002.
Test via the conformal-probe Stage A `multi_paired_edges` field —
the (v0=0, v1=0) self-loop entry must be absent.

**I2 — F0004 same.** Symmetric to I1 (F0002 ≡ F0004 byte-identical
defect class).

**I3 — Conformal-probe Stage A `well_formed=true` (or no (0,0)
self-loop)** for F0002, F0004. PR-Y14b's red-phase tests
`f0002_canon0_cluster_size_pinned_postfix` and
`f0004_canon0_cluster_size_pinned_postfix` turn green.

**I4 — `f0002_distinct_failure_after_dedup_or_passes` turns green.**
F0002 either passes OR fails at a stage strictly later than the
pre-fix `half_edge[4].twin = 0 but twin.twin = 28` anchor.

**I5 — No new `unpaired_directed_edges` introduced at Stage A.**
PR-Y14b's `f0002_no_new_unpaired_at_stage_a` continues to pass.

**I6 — Determinism preserved.** Two consecutive yang_fast sweeps
produce byte-identical `results.json`. PR-Y14b's
`f0002_determinism_two_runs_byte_identical` continues to pass.

**I7 — Architectural integrity (A15.6).** No new boundary-chaining
recovery, no tolerance escalation. Additive over existing exact
arithmetic.

**I8 — Cherchi reference parity.** The `mesh_booleans` CLI from
the Cherchi 2022 C++ sidecar (per
`docs/audits/cherchi2022_sidecar_feasibility.md`), fed F0002's
post-Stage-0 `verts_a + verts_b` mesh as OBJ, produces an
arrangement whose canonicalized vertex count for the F0002 corner
is 1 (matches our post-fix output). **This invariant is the
load-bearing external check**; if I8 is satisfied and I1–I7 are
satisfied, the fix is correct. If I1–I7 hold but I8 fails, the fix
silently disagrees with the reference and is wrong even if internal
oracles say green.

## 5. Oracles

| Oracle | Measures | Where |
|---|---|---|
| `check_conformal` Stage A on F0002 | `is_well_formed` field, multi_paired count, (0,0) self-loop presence | `crates/test-harness/tests/pr_y14b_coplanar_corner_dedup.rs::f0002_canon0_cluster_size_pinned_postfix` (already exists, currently RED; turns GREEN under PR-Y14c) |
| Same for F0004 | symmetric | `f0004_canon0_cluster_size_pinned_postfix` (already exists, RED) |
| F0002 distinct-failure-or-pass | F0002 either passes or fails at strictly later stage | `f0002_distinct_failure_after_dedup_or_passes` (already exists, RED) |
| Yang corpus regression guard | Post-PR pass count `≥` pre-PR baseline (= 9) | `app/tests/cases/assay/results.json` |
| `CHERCHI_LPI_DEDUPED_BY_CANON_KEY` counter | Non-zero on F0002 | New `[cherchi-tele]` line; PR-Y14c test author adds a counter-pinning test |
| Determinism across runs | Two consecutive sweeps produce byte-identical output | Existing `f0002_determinism_two_runs_byte_identical` |
| **Cherchi sidecar reference parity** | F0002 mesh-booleans CLI output's nanometer canonical vertex count for canon-0 = 1 | New harness test `crates/test-harness/tests/pr_y14c_cherchi_reference_parity.rs` (PR-Y14c test author writes; depends on PR-Y14d sidecar build) — see §10 |

## 6. Failure Modes

**6.1 LPI cluster collapses to fewer than expected.** I1 says "size
= 1." If we get 0 (the LPI was DROPPED entirely, not deduped), the
arrangement is missing a vertex and Cherchi's downstream
triangulation will produce malformed output. Distinguish via the
conformal probe Stage A: if Stage A reports `unpaired > 0` with
edges referencing canonical-0, the dedup over-collapsed and the
fix is wrong. Revert.

**6.2 Cross-call interference.** The dedup map MUST be per-call
(per `solve_intersections` invocation), not global. F0002 might run
multiple `subdivide_mesh_pair` calls (refinement loop in
`yang_integration.rs:794`) and each must independently re-build the
map. If global state is used, a Stage A `well_formed=true` on the
first call followed by `well_formed=false` on the second is the
diagnostic.

**6.3 Site 2 (inject_*) source not actually the culprit.** §6.7
investigation may reveal the 2 Explicit cluster members come from
elsewhere (e.g. a tessellation-level injector I missed). In that
case, branch D's location adapts; spec writer amends §3.

**6.4 Cherchi sidecar reference reports `well_formed=false` on
F0002 input too.** This would indicate the F0002 mesh itself is
ill-formed (not a Yang-port bug, but a Yang-port-input bug). Then
the defect is upstream of `subdivide_mesh_pair` and PR-Y14c becomes
"investigate why F0002 input mesh is ill-formed", which likely
returns to coplanar preprocess or tessellation. This outcome
re-opens the question of whether `dedup_mesh_vertices`'s `key as f64
/ scale` round-trip preserves bit-identity for the canonical
positions Cherchi expects.

**6.5 PR-Y14c lands without sidecar (I8 deferred to PR-Y14d).**
Acceptable per the team-lead's spec brief. The internal oracles
(I1–I7) prove the dedup behaves correctly; reference parity is a
correctness-vs-paper check, not a correctness-vs-spec check.
PR-Y14c's commit message must explicitly note "I8 deferred to
PR-Y14d sidecar build" so reviewers know the external check is
pending.

**6.6 Ship-as-is on Stage-B/C only with Stage-A still broken.** If
PR-Y14c lands LPI dedup but Site-2 source isn't fixed (or the
investigation shows there isn't one), Stage A's cluster size drops
from 8 to 2 (the 2 Explicit verts) — measurable improvement, but
I1's "= 1" target unmet. Ship as PR-Y14c partial; document
remaining 2-vert cluster anchor for PR-Y14d.

**6.7 INVESTIGATION TASK (Phase 0 of PR-Y14c).** Before writing
any production code, the implementer MUST run F0002 with temporary
instrumentation in `inject_partial_overlap_mesh` and
`inject_identical_footprint_mesh`, capturing per-pair `verts_a.push`
and `verts_b.push` event counts. The pre-fix Phase-3 evidence
strongly suggests `inject_partial_overlap_mesh` partial-mutates
before bailing out on a guard, but this is not yet PROVEN — it's
the most-likely candidate by elimination. The investigation either
confirms (defines branch D location) or refutes (re-localizes to a
new site) the hypothesis. **Do not write production code until
this investigation is done** — per
`feedback_anchor_before_fix.md`, "verify the fix anchor BEFORE
coding" is non-negotiable in this PR class.

## 7. Research Basis

**Yang et al. 2025 §4.4.2** — "After trimming the meshes using the
intersection curves, we directly apply a standard inside/outside
classification step [Cherchi et al. 2022]". Yang assumes the Cherchi
arrangement output is a well-formed simplicial complex; F0002's 8-way
canon-0 cluster violates that assumption, blocking everything
downstream.

**Cherchi et al. 2022 §4** — Cherchi's arrangement guarantees a
well-formed output from manifold-watertight input. The C++ reference
implementation (`gcherchi/InteractiveAndRobustMeshBooleans`,
`mesh_booleans` CLI) is the load-bearing external check per §I8.
LPI implicit points are introduced for symbolic robustness; the paper
notes (§5.1, §5.3) that materialization to floats is a "snap
rounding" problem with unsolved corner cases. Our LPI canonicalization
addresses this for the specific case where multiple LPIs collapse to
the same nanometer canonical key.

**Cherchi et al. 2020 §5.5** — symbolic-equality LPI dedup (via
`add_vertex_in_sorted_list`) catches the case where two LPI
expressions are exactly the same triple of (q1, q2, plane). It does
NOT catch the case where two DIFFERENT triples produce the same
materialized point — which is exactly the F0002 corner case. PR-Y14c
adds the canonical-key-based second dedup layer.

**Strategic escalation rule** (CLAUDE.md, MEMORY.md
`feedback_anchor_before_fix.md`): three wrong anchors in a row
(PR12, PR13, PR-Y14a/b) on the F0002 twin-pairing class triggers
mandatory reference-parity check. PR-Y14c is the first PR in this
class to require I8.

### 7a. Analytical vs. Approximate Method Justification

**Method:** Exact. LPI canonical key = nanometer integer-grid
quantization (no f64 comparison). Symbolic LPI equality is via
`add_vertex_in_sorted_list` (already exact). The new branch C uses
canonical-key equality, also exact (integer comparison).

**Surface-pair coverage:** N/A — this is a mesh-arrangement
canonicalization, not a SSI operation.

## 8. Implementation Sketch (informational, NOT spec)

The PR-Y14c spec writer is the authoritative source for §1–§7. This
§8 is a non-binding sketch.

```rust
// In cherchi/mod.rs::solve_intersections, near line 142
let mut lpi_canon_dedup: BTreeMap<[i64; 3], usize> = BTreeMap::new();

// Pass &mut lpi_canon_dedup through detect_intersections,
// classify_intersections, triangulation_with_parents into
// intersection_class.rs's three add_*_inters functions.

// In intersection_class.rs at each LPI emission site, after the
// existing add_vertex_in_sorted_list / add_impl_point block:
let new_v_id = if is_new {
    // Compute the LPI's materialized canonical key (cheap one-shot).
    let materialized = lpi.materialize().unwrap_or([0.0, 0.0, 0.0]);
    let scale = crate::units::QUANT_NANOMETER_SCALE;
    let canon = [
        (materialized[0] * scale).round() as i64,
        (materialized[1] * scale).round() as i64,
        (materialized[2] * scale).round() as i64,
    ];
    if let Some(&existing) = lpi_canon_dedup.get(&canon) {
        // Branch C: canonical collision with earlier LPI.
        CHERCHI_LPI_DEDUPED_BY_CANON_KEY.fetch_add(1, Ordering::Relaxed);
        existing
    } else {
        let id = ts.add_impl_point(lpi);
        debug_assert!(id == pos);
        lpi_canon_dedup.insert(canon, id);
        CHERCHI_LPI_CREATED.fetch_add(1, Ordering::Relaxed);
        id
    }
} else {
    existing_id
};
```

Counter declarations alongside `JOLLY_POINT_CREATIONS`. Telemetry
emission alongside `[cherchi-tele] jolly_creations:` line.

## 9. Out of Scope

- Building the Cherchi 2022 C++ sidecar — separate PR-Y14d. See
  `docs/audits/cherchi2022_sidecar_feasibility.md` for the GO
  verdict and the disk-space caveat (workspace volume at 99% used;
  build must use a different volume).
- F0005 (different probe signature; needs its own probe-driven
  anchor).
- R0020/R0021 (PR14 Render LOD anchor still candidate).
- Removing deprecated S-H clipping (blocked on Yang being operational).
- Refactoring `ImplicitPoint` materialization — the LPI implementation
  is upstream-faithful to the Cherchi C++ reference and should not be
  modified outside its narrow dedup-key role.

## 10. Verification (PR-Y14c pre-merge)

1. **Internal oracle suite** (PR-Y14b's red-phase tests) all pass:
   `f0002_canon0_cluster_size_pinned_postfix`,
   `f0004_canon0_cluster_size_pinned_postfix`,
   `f0002_distinct_failure_after_dedup_or_passes`.
2. **Internal oracle suite** (PR-Y14b's green tests) still pass:
   `coplanar_dedup_counter_nonzero_for_f0002`,
   `f0002_determinism_two_runs_byte_identical`,
   `f0002_no_new_unpaired_at_stage_a`.
3. **Yang corpus regression guard:** `results.json` post-PR pass
   count `≥` 9 (current baseline). Adversary refreshes and commits.
4. **`CHERCHI_LPI_DEDUPED_BY_CANON_KEY` counter** non-zero on F0002.
5. **`cargo clippy -p kernel --no-deps -- -D warnings`** clean.
6. **`cargo fmt --check`** clean.
7. **WASM rebuild** included in the same commit.
8. **Cherchi sidecar reference parity (I8)** — IF PR-Y14d sidecar
   build has landed, run differential test on F0002 and assert
   reference's canonical-vertex count for canon-0 = 1. IF NOT
   landed, document deferral in commit message and link to PR-Y14d
   in TODO. Ship may proceed without I8 verification on this
   condition; PR-Y14d's spec must include "verify I8 retroactively
   against PR-Y14c's HEAD" as part of its scope.

If verification 4 yields counter `= 0`, the dedup logic isn't being
exercised and the fix is dead code. Investigate before shipping.

If verification 1 yields some-pass-some-fail, that means the LPI
dedup helps but doesn't fully fix; either Site-2 (inject_*) is
contributing OR the LPI dedup needs broader application sites.
Recommend ship-as-partial with explicit anchor-recursion commitment
to PR-Y14e.

---

## 11. Notes for the PR-Y14c spec writer

This document is an adversary-drafted skeleton. The spec writer for
PR-Y14c (different agent per FIP §1) should:

- Verify §6.7 instrumentation produces the data the spec assumes —
  if not, amend §3 site 2's location.
- Verify the implementer can pass `&mut lpi_canon_dedup` through
  the call stack without major refactor — if not, propose a
  thread-local OR a `std::cell::RefCell` solution and document why.
- Clarify whether PR-Y14c's I8 deferral to PR-Y14d is acceptable or
  whether PR-Y14d should be merged into PR-Y14c's scope (the
  team-lead's call).
- Update §6.7's "investigation TASK" to a concrete data-collection
  phase 0 with a specific exit criterion.
- Re-confirm via Cherchi 2020 §5 / 2022 §4 that LPI canonical-key
  dedup does not violate the well-formed-simplicial-complex
  guarantee. This is non-obvious; the symbolic LPI dedup catches
  cases the canonical-key dedup does not (and vice versa), so they
  are complementary, but the spec writer must verify the union of
  branches A, B, C does not over-merge.
