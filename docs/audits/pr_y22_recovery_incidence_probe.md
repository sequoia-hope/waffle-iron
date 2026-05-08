# PR-Y22-RECOVERY R-0 — Incidence probe (incidence-prober-r)

**Author:** incidence-prober-r · **Date:** 2026-05-08 · **Plan:** PR-Y22-RECOVERY R-0
**Role:** probe + verdict ONLY (per `feedback_oracle_credibility_via_role_separation.md`).
NOT spec-edit (R-a) and NOT implement (R-c).

**Verdict:** **incidence-3+** — implementer-z's classification is empirically
correct; canary-runner-9 §1 + spec §4 "manifold-mis-segmented" framing is
**REFUTED** by direct n_fwd+n_rev probe at L857-881 boundary collection.

---

## §1 Per-canonical-edge incidence table

Probe site: `crates/kernel/src/boolean/topology_extract.rs` L857-881, inside the
`for (pi, patch) in patches.iter().enumerate()` → `for &ti in &patch.tris` →
`for ei in 0..3` loops, immediately after computing `v0`/`v1` and BEFORE the
`is_boundary` predicate. Probe printed `n_fwd = directed_edge_to_tris[(v0,v1)].len()`,
`n_rev = directed_edge_to_tris[(v1,v0)].len()`, `n_total = n_fwd + n_rev`,
`manif = (n_total == 2)`. 21 lines emitted across F0020 spotlight (Boolean #2,
i.e. Extrude 3, the only boolean with these target edges).

Aggregated per-canonical (lo, hi), deduped across the (pi, direction) cross-product:

| canon (lo, hi) | n_fwd+n_rev | manif | seen in patches `pi`     |
|----------------|-------------|-------|--------------------------|
| (69, 71)       | **3**       | false | 6, 27                    |
| (69, 70)       | **3**       | false | 6, 26                    |
| (70, 73)       | **3**       | false | 6, 26                    |
| (72, 73)       | **3**       | false | 6, 26                    |
| (68, 72)       | **3**       | false | 6, 26                    |
| (66, 68)       | **3**       | false | 6, 26                    |
| (66, 67)       | **3**       | false | 6, 25                    |

Per-edge `(n_fwd, n_rev)` always presents as `(2,1)` from one direction's tri
and `(1,2)` from the other — confirming the undirected incidence is **3**
(2 tris emit one direction, 1 tri emits the reverse). Each canonical edge is
seen in exactly **two** patches: `pi=6` (the first probe-line patch, where the
fwd tri lives in B's tessellation) plus one of `pi=25/26/27` (where the rev
single-tri lives in A's tessellation).

**All 7 target canonical edges show `n_total = 3` uniformly. Zero exceptions.**

## §2 Verdict: incidence-3+

**implementer-z was correct.** All 7 NONCONF cases are TRULY non-manifold by
the Yang §4.4.2 definition (incidence != 2). They are NOT
manifold-mis-segmented (which would require `n_total = 2` with both directions
simply mis-routed into one patch). The mechanism is incidence=3: two coplanar
sub-tris on one side share the directed edge (v0→v1) and a single sub-tri on
the other side carries the reverse (v1→v0).

**canary-runner-9 §1 + spec-writer-v §4 reconciliation table is REFUTED.**
Specifically, the spec's claim that `rev_in_de2t == true && rev_in_same_patch
== true` implies "manifold-mis-segmented" with `n_total = 2` is wrong: the
canary's rev_tris/rev_patches enumeration only proved a reverse exists, NOT
that undirected_count == 2. The probe shows undirected_count == 3 for every
target edge.

## §3 Yang §4.4.2 paper alignment

Yang §4.4.2 defines a manifold edge as one with incidence count exactly 2
(equivalently, exactly two incident triangles). Edges with incidence ≥ 3 are
**non-manifold (NMM)**. Per Yang's directional-symmetry mandate, the
1:1 twin-pairing requirement applies to **manifold edges only**. NMM edges are
explicitly NOT subject to 1:1 pairing — they may have arbitrarily many
incident half-edges that do not pair into single twins.

Cherchi 2022 §5 manifold-edge barriers also encode this: an edge is a flood
barrier iff `undirected_incidence != 2` (matches Step 4 `edge_is_manifold`
predicate at L513-516 of `topology_extract.rs`).

The 7 target edges have `n_total = 3`, so per Yang §4.4.2 they are
**legitimately NMM**. The correct treatment is NMM-classification (twin=None,
no `unpaired_count` increment) — exactly what PR-Y20-MODE-A's NMM branch does
for `rev_in_de2t == false`, EXTENDED to also cover `undirected_incidence != 2`.

## §4 M1-anchor recommendation (data-cited)

R-c implementer should NMM-classify these 7 edges via the existing
PR-Y20-MODE-A NMM branch at L1253-1314 `[]` arm (`is_nmm` predicate at
L1270-1291). Current predicate:

```rust
is_nmm = !directed_edge_to_tris.contains_key(&(v1_canon, v0_canon));
```

Recommended extension (cite §1 data: every target edge has
`undirected_incidence == 3 != 2`):

```rust
let undirected_count = directed_edge_to_tris.get(&(v0_canon, v1_canon))
    .map(|v| v.len()).unwrap_or(0)
    + directed_edge_to_tris.get(&(v1_canon, v0_canon))
    .map(|v| v.len()).unwrap_or(0);
is_nmm = !directed_edge_to_tris.contains_key(&(v1_canon, v0_canon))
    || undirected_count != 2;
```

This aligns with Yang §4.4.2 (manifold ↔ incidence == 2) and Cherchi 2022 §5
manifold-edge barrier semantics already in use at L513-516.

**Path A retroactive emit (spec §4 L126-169) is empirically WRONG:** there is
no manifold edge to retroactively-emit — the edge is genuinely NMM. Emitting
both directions from a 3-incident undirected edge would still leave one HE
unpaired (or worse, force an arbitrary 2-of-3 selection that violates Yang's
1:1 mandate at the surviving manifold pair).

NB: spec §8 anti-scope (L290) explicitly forbids extending NMM to "manifold
same-patch edges." That prohibition stands — these are **NOT** manifold;
they are incidence-3 NMM. The anti-scope rule is preserved; the spec's
factual classification of "manifold-mis-segmented" is what fails.

## §5 Verification

```
$ git checkout -- crates/kernel/src/boolean/topology_extract.rs
$ git status --short
 M app/tests/cases/assay/results.json                              (pre-existing drift)
?? crates/test-harness/tests/pr_y22_mode_a_missing_regression.rs   (test-author-m)
?? docs/audits/pr_y22_mode_a_missing_canary.md                     (canary-runner-9)
?? docs/audits/pr_y22_mode_a_missing_validation.md                 (adversary-22)
?? specs/yang_pr_y22_mode_a_missing.md                             (spec-writer-v)
?? docs/audits/pr_y22_recovery_incidence_probe.md                  (THIS MEMO — not yet added)
$ grep -n 'r0-incidence' crates/kernel/src/boolean/topology_extract.rs
(no output — probe fully reverted)
```

- Probe diff captured at `/tmp/r0_probe.patch` (29 LOC, includes 22-LOC `eprintln!`
  block) for posterity. NOT applied to repo.
- `git stash` was NOT used — `git checkout --` after diff capture, per brief.
- No production code changes. R-0 is empirical-only.

## §6 Discipline checks

- Per `feedback_yang_only.md`: probe data REPORTED honestly. The data REFUTES
  the spec; no fallback path or shortcut taken to preserve the spec's prior
  classification.
- Per `feedback_anchor_before_fix.md`: this IS the anchor verification step
  for R-c. R-c should now be unambiguously routed to NMM-classification at
  L1270-1291, NOT Path A retroactive emit at L862.
- Per `feedback_oracle_credibility_via_role_separation.md`: incidence-prober-r
  did NOT recommend the spec edit (R-a's job) and did NOT implement (R-c's
  job). §4 is a recommendation framed as data-cited input to R-a/R-c, not a
  unilateral spec rewrite or code change.

**Routing:** R-a (spec-rewriter) reads §1+§2+§3 and revises spec §4 to drop
the "manifold-mis-segmented" framing and adopt NMM-classification with
`undirected_count != 2`. R-c (implementer) reads §4 and lands the
two-line predicate extension at L1270-1291. R-0 sub-phase complete.
