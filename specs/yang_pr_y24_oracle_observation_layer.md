# PR-Y24 — Oracle/validator NMM-classification keyed on construction-time `directed_he`

**Author:** spec-y24 · **Date:** 2026-05-08 · **Plan:** `/home/claude/.claude/plans/optimized-wandering-wind.md`
**Canary:** `docs/audits/pr_y24_anchor_canary.md` (canary-y24, commit `69c6c2b`) — **CONFIRMED**.
F0020 b#2 `[twin-oracle] unpaired_count`: **simulated=0 (vs actual=2)** under construction-time
keying. F0044/F0045/R0092 batch: 7/7 invocations `simulated=actual=0` (cohort guard MET).
**Scope:** **B1 (plumb construction-time directed-edge data via a new `TopoArena` field)**
— see §4 + §5 for reasoning. Canary §5 amended to recommend B2 as defensible-minimum;
this spec elevates to B1 to defend against the F0044 invocation #5 fragility surfaced
in canary §6 banked finding 3 and the I2 measurability requirement (§5).

**FIP §3.2 + §8 Bug Fix Variant.**

---

## §1 Goal

Drive F0020 Extrude 3 `[twin-oracle] unpaired_count` from **2 → 0** and resolve the
`validate_yang_result_topology` `(38,27)` panic by re-keying the NMM-vs-missing-edge
predicate at the **observation layer** from arena traversal `(he.origin,
he.next.origin)` to the **construction-time `directed_he` keys** populated at
`crates/kernel/src/boolean/topology_extract.rs:1149-1152`.

The observation layer comprises:

- **Site A:** `[twin-oracle]` at `topology_extract.rs:1445-1471` (and offender-trace at L1515).
- **Site B:** `validate_yang_result_topology` at `yang_integration.rs:1241-1308`.

Pairing logic (`topology_extract.rs:1219-1380`), arena structure
(`topology_extract.rs:1131-1146`), and chain emission upstream stay byte-identical.

**Per `feedback_no_last_bug.md`:** PR-Y24 addresses the **observation layer**. F0020
spotlight Status:Failed MAY persist post-PR at a *different* panic surface (e.g.,
mesh-quality / face-iteration / tessellation-render layer). That outcome is a
next-PR layer, not a PR-Y24 regression. Success metric is the layer-targeted
counter drop and the absence of the `(38,27)` validator panic, not case-status
flip.

---

## §2 Background

### §2.1 PR-Y23 ABORT post-mortem (the root reason for PR-Y24)

PR-Y23 ABORTed (`docs/audits/pr_y23_abort.md` §3) because fix-shape (a) — drop
open chains at `topology_extract.rs:961` — regressed the F0044/F0045/R0092 cohort
by 9 new MISSING-edge defects. Quoting the abort memo §3:

> The closure check at L961 treats each patch's open chain in isolation, but in
> the F0044/F0045/R0092 batch, an open chain's half-edges can legitimately pair
> with closed-loop HEs in OTHER patches that share the same canonical
> directed-edge pair. Dropping the open chain leaves those other patches' HEs
> stranded as new MISSING-edge defects.
>
> This is a structural property of the patch-segmentation graph: the
> chain-builder at L900-963 operates per-patch but the directed-edge canonical
> map (`directed_edge_to_tris` at Step 4) is global. An HE's pair-ability is
> determined globally; the closure check is local. The two views disagree on
> the F0044 cohort.

Per `feedback_local_fix_for_global_invariant.md`: the broken invariant is
global, but the PR-Y23 fix shape acted per-element in isolation. The H1' anchor
at L961 is **correctly identified**; the **fix shape** was wrong-scoped.

### §2.2 PR-Y24 reframing — observation layer vs. arena structure

PR-Y23's option (d) (Phase 1 explore in plan §"Pivot to option (e) reframed")
proposed setting `HE[n-1].next = HE[n-1]` for n=2 open chains as a self-loop.
**REJECTED** in Phase 1: face iteration starting at HE[0] walks
`HE[0] → HE[1] → HE[1] → ...` and infinite-loops at all 27 traversal sites
that use `if he == start_he { break; }` termination. The arena structure is
load-bearing for face traversal; modifying HE.next is not viable.

The remaining lever is option (e) reframed: leave the arena structure
untouched, and re-key the **observation layer** from arena traversal to
`directed_he` (the global ground truth populated by the construction loop at
L1149-1152). The arena traversal is polluted on open-chain wrap-backs at
L1131-1146 because `HE[i].next = HE[base + (i+1) % n]` cycles back to HE[0]
when i = n-1 even on patches whose chain is non-closed (canary §1 confirms 16
arena-only entries on F0020 b#2 with non-degenerate first 5
`(73,20)`, `(36,69)`, `(54,44)`, `(59,49)`, `(31,71)`). The
construction-time `directed_he` map sees only the directed edges actually
inserted by the construction loop (the ground-truth set per Yang §3 paper
input), unpolluted by the wrap-back artifact.

### §2.3 Canary §1 P4 readback — load-bearing evidence for HE 58 / HE 59

From `docs/audits/pr_y24_anchor_canary.md` §1, F0020 b#2 (169 HEs):

```
[y24-probe-p1] arena_only_count=16 constructed_only_count=16
[y24-probe-p1] arena_only_first5=[(73,20),(36,69),(54,44),(59,49),(31,71)]
[y24-probe-p1] constructed_only_first5=[(38,26),(74,10),(57,41),(78,70),(31,17)]
[y24-probe-p2] simulated_twin_oracle_unpaired_count=0 (vs actual=2) he_with_no_construct_dest=0
[twin-oracle] unpaired_count=2
[twin-oracle] offender he=58 twin=-3 twin.twin=-3 origin=v27 dest=v38
[twin-oracle] offender he=59 twin=-3 twin.twin=-3 origin=v38 dest=v27
```

Canary §1 reading:

- HE 58's construction-time directed edge is `(BV27, BV38)`. The reverse `(BV38, BV27)` is **NOT** in `directed_he` (it is in arena_dir_edges as a wrap-back). Under construction-time keying, rev-test fails → HE 58 classified as legitimate-NMM, not unpaired.
- HE 59's construction-time directed edge is `(BV38, BV26)` — verbatim match to `constructed_only_first5[0] = (38, 26)`. The reverse `(BV26, BV38)` is **NOT** in `directed_he`. Under construction-time keying, rev-test fails → HE 59 classified as legitimate-NMM, not unpaired.

The arena traversal sees `(BV38, BV27)` as the destination key for HE 58 because
HE 58's `next` field points back through the wrap-back at L1131-1146; this is the
**phantom reverse**. The construction-time map saw HE 58 inserted with key
`(BV27, BV38)` and never inserted the reverse — that is the **input ground
truth** per Yang §3.

### §2.4 Canary §2 — F0044 cohort guard MET (7/7 invocations)

All 7 `flood_fill_patches` invocations in the F0044+F0045+R0092 batch report
`simulated=actual=0`. Per canary §2 table:

| # | total HEs | actual | simulated | arena_only | constructed_only |
|---|---|---|---|---|---|
| 1 | 136 | 0 | 0 | 0 | 0 |
| 2 | 234 | 0 | 0 | 0 | 0 |
| 3 | 330 | 0 | 0 | 0 | 0 |
| 4 | 460 | 0 | 0 | 0 | 0 |
| 5 | 229 | 0 | 0 | **4** | **5** |
| 6 | 283 | 0 | 0 | 0 | 0 |
| 7 | 408 | 0 | 0 | 0 | 0 |

Both keying schemes agree on all 7 invocations: 0 unpaired. **Cohort guard
MET; mechanism is structurally invariant in this cohort under either keying
scheme.**

The 7th invocation's divergence in row 5 (229 HEs, 4 arena-only) is the
fragility signal cited in canary §6 banked finding 3 and informs the §4 scope
selection.

---

## §3 Parameters

None. PR-Y24 is a behavior-preserving correctness fix at the observation
layer; no new user-facing parameters, env vars, or feature flags. The probe
gate `Y24_PROBE` is canary-only and removed at close-out (worktree teardown
per canary §7).

---

## §4 Branch table — H1' wrap-back manufactures phantom arena-traversal reverse

The defect is single-mechanism, single-anchor. The branch table enumerates
the **fix-shape sub-options** considered for routing the construction-time
directed-edge data to the validator (Site B); the oracle (Site A) is fixed
in-place via inline replacement (e1).

### §4.1 Branch — Single mechanism

| Mechanism | Trigger | Symptom | Layer |
|---|---|---|---|
| H1' open-loop wrap-back | Patch's chain non-closed; L1131-1146 emits `HE[n-1].next = HE[base]` cycling to HE[0]; arena traversal sees phantom reverse not in `directed_he` | Observation layer reads `arena_dir_edges` ⊇ phantom reverses; mis-classifies legitimate-NMM as missing-defect | Observation (oracle + validator) |

### §4.2 Sub-options for fix shape

Per canary §5 the load-bearing prediction was MET under construction-time
keying for both Site A and Site B. The remaining design choice is **how**
construction-time directed-edge data reaches the validator (Site B), since
`directed_he` is local to `extract_topology` and currently not exported.

| Sub-option | Site A | Site B | LOC | Cohort risk |
|---|---|---|---|---|
| (e1) inline-replace | Replace `arena_dir_edges` build at L1445-1449 with `directed_he.keys()` collect; replace `v_dest = arena.half_edges[he.next.0].origin.0` at L1463+L1515 with `he_to_constructed_dest[i]` | (un-addressed; Site B unchanged → still uses polluted arena-traversal keys; partial fix) | ~25 (Site A only) | **REJECTED** — leaves `(38,27)` validator panic intact |
| (B2) move predicate upstream into `extract_topology` | Same as (e1) | Run the missing-edge predicate in `extract_topology` (where `directed_he` is in scope), surface the result via existing `Result<...>` channel; validator at L1298-1308 stops re-running classification | ~50 | LOW — but no defense against I2 measurability gap (validator independence lost; F0044 invocation #5 fragility — canary §6 finding 3 — has no second-layer guard) |
| **(B1) plumb directed_he via `TopoArena` field** | Same as (e1) | Add `pub constructed_directed_edge: Vec<Option<(VertexIdx, VertexIdx)>>` field on `TopoArena`, populated at Step 7 close in `extract_topology` (one entry per HE, from `directed_he`'s inverse); validator at L1241-1247 builds `arena_dir_edges` from `arena.constructed_directed_edge` instead of `arena.half_edges[he.next.0].origin`, and at L1298-1308 reads `v_dest` from the same field | ~80 | LOWEST — preserves validator's independent role; field is observable in tests; defends against future cohort divergence (canary §6 finding 3) |

### §4.3 Selection — B1 (plumb directed_he via arena field)

**Selected sub-option: B1 + (e1) at Site A.**

**Reasoning (deviation from canary §5 amended recommendation, with team-lead grant in brief):**

1. **I2 measurability requires the field on the arena.** Spec §5 I2 mandates that "pairing logic at L1219-1380 and arena structure at L1131-1146 are byte-identical pre/post PR; measurable: F0044 batch `[topo-extract] summary unpaired=N` is byte-identical." Under B2, the validator no longer runs an independent NMM-classification pass; the test cannot independently verify that the new construction-time-keyed predicate is consistent with the upstream extraction (the upstream IS the predicate). Under B1, the validator runs the predicate **independently** with input data sourced from a stable arena field — the existing test `pr_y22_f0044_b5_mode_a_missing_drops_by_2` continues to exercise the validator path with byte-identical L1219-1380 internals, providing an oracle-layer cross-check.
2. **F0044 invocation #5 fragility.** Canary §6 banked finding 3: invocation #5 (229 HEs) is the only F0044 cohort case with non-zero arena-only divergence (`arena_only_count=4`, `constructed_only_count=5`); the cohort's "happens to be 0" verdict in row 5 of canary §2 is fragile in exactly the way `feedback_local_fix_for_global_invariant.md` warns about — the local symmetry of `actual=0` and `simulated=0` could shift under future cohort additions where divergence yields *different* verdicts. B1 retains validator independence so a future cohort case where `simulated ≠ upstream-predicate` would surface a B-side error rather than silently inheriting the upstream verdict.
3. **Canary §5 acknowledges the upgrade path.** Quoting canary §5: "If spec-y24 finds future cohort risk during spec-writing or test-y24 produces a corpus-wide guard that catches it, the upgrade path B2→B1 is straightforward." This spec phase is precisely that future-cohort-risk evaluation, and the §5 reasoning above identifies risk that B1 defends against.
4. **B1's incremental cost is modest.** The `TopoArena` field addition is ~50 LOC (struct field + populate at Step 7 close + 27 traversal sites continue to use `he.next` for face iteration; nothing else changes). The new field is **observation-only**: face iteration, pairing, NMM-classification at Step 7 [] arm, collision detection, and Step 6 boundary collection all continue to read `arena.half_edges` via `he.next` as before. The field is consumed solely by the [twin-oracle] (Site A) and validator (Site B) when those sites need to read the construction-time directed edge.

**Anti-rule:** B1 must not introduce a new path that *replaces* arena traversal at face-iteration sites (the 27 sites cited in plan §"Pivot to option (e) reframed"). The arena's `next` chain remains the load-bearing structure for face walks; only the **observation predicate** (NMM-vs-missing-edge classification) reads the new field.

---

## §5 Invariants (paper-cited, formal, measurable)

### §5.1 I1 — Observation-layer NMM predicate reads from construction-time `directed_he`

**Statement:** The NMM-vs-missing-edge classification predicate at Site A (oracle, L1452-1469) and Site B (validator, L1298-1308) reads `v_dest` for each half-edge from the construction-time directed-edge data populated at L1149-1152, NOT from `arena.half_edges[he.next.0].origin.0`. The set used to test rev-presence is sourced from `directed_he.keys()`, NOT from arena traversal.

**Paper citations (verbatim):**

- **Cherchi 2022 §3** (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:251-254`):
  > "When exact methods are used, the arrangement is guaranteed to be a well formed simplicial complex and surface patches are bounded by closed loops of non-manifold edges, namely the intersection lines."

  This establishes that patch boundaries are closed loops of non-manifold edges. The observation predicate's purpose is to mechanize that contract: distinguish a half-edge whose canonical reverse exists in the patch-boundary set (manifold-edge candidate; missing pair = defect) from a half-edge whose canonical reverse does not exist (legitimate non-manifold edge per the §3 surface-patches definition).

  The construction-time `directed_he` map IS the arrangement's directed-edge set as inserted by the patch-boundary loop at L1119-1146 (the mechanization of "closed loops of non-manifold edges"). Arena traversal `(he.origin, he.next.origin)` is a derivative view, polluted on open-chain wrap-backs because `next` cycles back to HE[0] regardless of chain closure. Re-keying the predicate on `directed_he` aligns the observation with the arrangement's input ground truth.

- **Yang 2025 §3** (`refs/text/yang2025_hybrid_boolean.txt:248-249`):
  > "edges that form a continuous boundary, with each edge shared by two adjacent faces."

  This establishes the manifold-edge baseline: an edge is shared by exactly two adjacent faces (incidence == 2). The observation predicate's `rev-present` test is the mechanization of "shared by two adjacent faces" — both directions must appear in the directed-edge set for an edge to be manifold. The directed-edge set must be the construction-time set (input ground truth: what was inserted), not the arena-traversal projection (derivative: what `next`-walks point to).

**Citation hygiene (per `feedback_external_coherence.md`):** Yang §4.4.2 is cited separately for the boundary-curve algorithm (mesh-segmentation along boundary curves, `refs/text/yang2025_hybrid_boolean.txt:574-605`), NOT as a "directional-symmetry" mandate. The kernel-internal coinage "Yang §4.4.2 directional-symmetry" was refuted in PR-Y22 v2 §5 audit and banked in PR-Y23 spec §8.3; PR-Y24 spec continues that discipline. The `[the_one]`-arm pairing logic at L1232 implements per-edge pair-up by canonical-direction match; that mechanic is local to Waffle's implementation, not a paper-named theorem.

### §5.2 I2 — Pairing logic and arena structure byte-identical pre/post PR

**Statement:** The pairing-search loop at `topology_extract.rs:1219-1380` (which reads `directed_he.keys()` directly to build candidate match sets) and the arena population loop at `topology_extract.rs:1131-1146` (which sets `HE[i].twin`, `HE[i].next`, `HE[i].prev`, `HE[i].loop_`) emit identical state pre- and post-PR.

**Measurable:** Re-running the F0044+F0045+R0092 batch with `YANG_BOOLEAN=1 TWIN_DEBUG=1` produces identical `[topo-extract] summary: paired=P, unpaired=N, ambiguous=A` lines pre/post PR. Per canary §2 baseline, all 7 invocations have `unpaired=0` pre-PR; post-PR must remain `unpaired=0` per-invocation.

The new `TopoArena.constructed_directed_edge` field is populated **after** L1131-1146 (i.e., as a Step 7-close augmentation, sourced from `directed_he`); its presence does not modify the data feeding L1219-1380.

**Paper citation:** Cherchi 2022 §4 (preceding §5 in the canary's verbatim quote) "well formed simplicial complex" — the arrangement output contract is unchanged because the upstream construction is unchanged.

### §5.3 I3 — F0020 Extrude 3 `[twin-oracle] unpaired_count == 0` (load-bearing)

**Statement:** F0020 Extrude 3 (the b#2 boolean per canary §1 numbering) `[twin-oracle] unpaired_count` is 0 across all `flood_fill_patches` invocations.

**Pre-PR baseline (commit `3c749a3`):** 2 (canary §1 actual; offenders HE 58 + HE 59).
**Post-PR target:** 0 (canary §1 simulated; both HE 58 and HE 59 reclassify as legitimate-NMM under construction-time keying).

**Paper citation:**

- Yang §3 (verbatim, line 248-249): "each edge shared by two adjacent faces" — defines manifold-edge incidence.
- Cherchi 2022 §3 (verbatim, line 251-254): "surface patches are bounded by closed loops of non-manifold edges" — defines the patch-boundary structure that the observation predicate mechanizes.

Both HE 58's reverse `(BV38, BV27)` and HE 59's reverse `(BV26, BV38)` are absent from `directed_he` per canary §1 P4 readback. Under construction-time keying, neither half-edge's reverse is present → both classify as legitimate-NMM (Yang §3 "each edge shared by two adjacent faces" fails), so neither contributes to `unpaired_count`. Post-PR `unpaired_count = 0` for F0020 b#2; pre-PR `unpaired_count = 0` already for F0020 b#1 (canary §1 first block).

---

## §6 Oracles (measurable)

| # | Oracle | Pre-PR baseline | Post-PR target | How measured |
|---|---|---|---|---|
| 1 | F0020 spotlight Status | `Failed` at `validate_yang_result_topology` `(38,27)` panic | `Passed` OR `Failed` at *different* panic surface | `cargo test -p test-harness --test assay_randomized -- spotlight_f0020 --ignored --nocapture` — read `Status:` line + `detail` field |
| 2 | F0020 `[twin-oracle] unpaired_count` (MAX across all flood_fill_patches invocations) | 2 (b#2; canary §1) | **0** (load-bearing per I3) | `max_twin_oracle_field(stderr, "unpaired_count")` (helper from `pr_y22_mode_a_missing_regression.rs:155-166`) |
| 3 | F0044+F0045+R0092 batch `[topo-extract] summary unpaired` (MAX) | 0 (canary §2 + PR-Y22 baseline) | **0** (cohort guard, structural per I2) | `max_topo_extract_unpaired(stderr)` (helper from `pr_y22_mode_a_missing_regression.rs:182-199`) |
| 4 | F0044+F0045+R0092 batch `[twin-oracle] unpaired_count` (MAX) | 0 (canary §2 — all 7 invocations) | **0** (cohort guard) | `max_twin_oracle_field(stderr, "unpaired_count")` |
| 5 | F0030 sibling status | Failed (12 unpaired/66; Euler V-E+F=3 per plan §"Adversary phase" gate 2) | unchanged | `spotlight_f0030` |
| 6 | F0050 sibling status | Failed (39 unpaired/417 watertight per plan gate 3) | unchanged | `spotlight_f0050` |
| 7 | Yang fast subset | 10/157 (per plan gate 6) | ≥ 10 (expect ≥ 11 if F0020 returns) | `cargo test -p test-harness --test assay_randomized -- yang_fast --ignored --nocapture` — count `Passed:` line |
| 8 | Kernel baseline tests | 1250 pass / 29 ignored / 42 (per plan gate 7) | 1250/29/42 unchanged | `cargo test -p kernel 2>&1 \| tail -3` |
| 9 | `cargo clippy -p kernel` | clean | clean (no new warnings) | `cargo clippy -p kernel 2>&1` |
| 10 | `cargo fmt --check` | clean | clean | `cargo fmt --check` |

**Load-bearing gate:** Oracle #2 (the F0020 `[twin-oracle] unpaired_count == 0`). All other oracles are non-regression bounds.

---

## §7 Failure modes

### §7.1 Same `(38,27)` panic survives → ABORT

If post-PR `[twin-oracle] unpaired_count` for F0020 b#2 is still 2 (or any non-zero value) AND the validator still panics with `half_edge[*].twin = None but arena contains a HE for the reverse direction (38->27)`, the option (e) reframed mechanism is **empirically refuted**.

**Action:** ABORT per FIP §3 + §8 bug-fix variant. Write `docs/audits/pr_y24_abort.md` capturing the failure mode and revert via worktree. Per `feedback_anchor_before_fix.md`, this would be the 4th wrong fix-shape in the PR-Y22→PR-Y24 sub-arc at H1' anchor; strategic escalation to **reference-parity build** (Cherchi 2022 C++ sidecar comparison on F0020 b#2) becomes mandatory next phase.

### §7.2 Different panic surface → next-layer outcome (NOT a regression)

Per `feedback_no_last_bug.md`: PR-Y24 may resolve the `(38,27)` validator panic and surface a **different** panic at the next downstream layer (e.g., mesh-quality `(38,38)` self-loop check, face-iteration on open-chain faces, NMM-render layer banked from PR-Y21 ABORT, or Euler V-E+F validation downstream).

**Action:** Adversary classifies. If the new panic surfaces **post** the validator (i.e., further along the build path), it is a next-PR layer; this is consistent with PR-Y24's load-bearing oracle (#2) being GREEN. If the new panic surfaces **earlier** than the previous `(38,27)` (i.e., the fix introduced a new defect upstream of the fixed layer), that is a regression — fix before ship.

The canonical "next-layer" examples to watch for:

- `validate_yang_result_topology` reaches a different error string after the L1298-1308 missing-edge check passes (e.g., loop-closure check at L1314+ or face-outer-loop validation).
- `flood_fill_patches` Step 4 manifold-incidence check at L504+ fires on the same edge canonical key with a different message.
- Downstream tessellation-render NMM-handling layer (PR-Y21 ABORT residual; banked).

### §7.3 F0044+F0045+R0092 cohort regresses → ABORT

If any invocation in the F0044 batch reports `[topo-extract] summary unpaired > 0` OR `[twin-oracle] unpaired_count > 0` post-PR, I2 is violated.

**Action:** ABORT immediately. Write abort memo. Per `feedback_local_fix_for_global_invariant.md`: cohort regression means the global pair-ability invariant was disturbed despite the spec's "byte-identical" claim — debug the I2 measurability gap before any re-attempt. (B1's design specifically defends against this; a regression here would imply the field-population logic at Step 7 close has a bug.)

### §7.4 Kernel baseline regresses → fix before ship; never `--no-verify`

If `cargo test -p kernel` reports any test failure post-PR that was passing pre-PR, fix the underlying issue. Per CLAUDE.md "Fix It Right or Don't Fix It (P9-P10)": no `--no-verify`, no skipped tests. If the broken test is unrelated to the PR's scope, revert and investigate.

### §7.5 `clippy` / `fmt` regression → fix before ship

Standard CLAUDE.md "Before Committing" gate.

---

## §8 Research basis

### §8.1 Yang 2025

- **§3 Overview** (`refs/text/yang2025_hybrid_boolean.txt:240-330`) — input B-Rep model definition; closed-loop boundary structure with each edge shared by two adjacent faces. Verbatim phrase cited in I1 + I3.
- **§4.4.2 Mesh and B-Rep Booleans** (`refs/text/yang2025_hybrid_boolean.txt:574-605`) — mesh-segmentation algorithm: "starting from an inner triangle ... using it as a seed triangle for the patch, our algorithm expands the patch by including more neighboring inner triangles, until all the neighboring triangles of the patch are on the boundaries. The boundary curves can then be easily collected and mapped back to the parametric surfaces by fitting the curve in the parametric domain." This is the algorithm `flood_fill_patches` implements; the [twin-oracle] is its post-condition observation layer. **NOT** cited as "Yang §4.4.2 directional-symmetry" per PR-Y23 spec §8.3 banked imprecision.

### §8.2 Cherchi 2022

- **§3 Overview** (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:232-290`) — arrangement output contract: "the arrangement is guaranteed to be a well formed simplicial complex and surface patches are bounded by closed loops of non-manifold edges." Verbatim phrase cited in I1 + I3.
- **§5 Inside/Outside Classification** (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:385-470`) — per-patch ray-casting; not anchor-load-bearing for PR-Y24, cited for cohort context. The `edgeIsManifold` predicate referenced in canary §6 banked finding 5 mirrors a similar `edge_is_manifold` at `topology_extract.rs:513-516` used for Step 4 patch flooding; PR-Y24 leaves that predicate untouched.

### §8.3 Citation hygiene (carry-forward from PR-Y23 §8.3)

- "Yang §4.4.2 directional-symmetry" — **DO NOT USE.** Refuted in PR-Y22 v2 §5 audit. Yang §4.4.2 is the mesh-segmentation algorithm, not a directional-symmetry theorem. The 1:1 manifold-edge pairing is established in §3 (each edge shared by two adjacent faces; incidence == 2) and `topology_extract.rs:1232` `[the_one]` arm implements it.
- "Cherchi 2022 §5 manifold-edge barrier" — acceptable when describing the `edgeIsManifold` predicate at C++ `booleans.cpp:412` mirrored at `topology_extract.rs:513-516`; not load-bearing for PR-Y24.
- All paper citations in this spec quote the exact phrase from the verbatim text file with line numbers, per `feedback_external_coherence.md` "When porting from a published algorithm, build differential testing against the reference as the load-bearing oracle. Don't substitute one for the other."

---

## §9 Anti-scope (explicit OUT)

- **Face iteration on open-chain faces** at the 27 traversal sites listed in plan §"Pivot to option (e) reframed" — UNCHANGED. Open-chain incomplete-boundary face iteration is a banked Layer-4 concern (next-PR territory per `feedback_no_last_bug.md`); PR-Y24 fixes only the observation layer.
- **R3 ownership at L810-863** — UNCHANGED. PR-Y23 abort §4.1 banked option (b) (R3 strengthening) for a future PR; PR-Y24 does not modify R3 routing.
- **Closure check at L961** (PR-Y23 fix-shape (a)) — STAYS BANKED PR-Y23-style. The H1' anchor is correct; PR-Y23's local fix shape is wrong-scoped per `feedback_local_fix_for_global_invariant.md`. PR-Y24 fixes the same anchor at the **observation layer** instead of dropping the chain.
- **Self-loop next-pointer at L1131-1146** — REJECTED by Phase 1 (plan §"Pivot to option (e) reframed" Section D infinite-loop kill). 27 traversal sites use `if he == start_he { break; }` termination; setting `HE[n-1].next = HE[n-1]` infinite-loops at all of them.
- **Collision-detection at L1483-1494** (PR-Y19-MODE-B 0e amendment) — LEAVE UNCHANGED. `collision_count` is informational per the 0e amendment, not load-bearing. Canary §6 banked finding 3 noted invocation #5's `collision_count=1` co-occurs with the divergence; this is a flag for future cohort review, not a PR-Y24 anchor.
- **`stitch.rs` legacy path** — DEPRECATED per CLAUDE.md "Boolean Pipeline Strategic Direction." The S-H clipping + tolerance escalation pipeline is being replaced by Yang; PR-Y24 does not modify legacy code.
- **PR-Y20-MODE-A `Option<HalfEdgeIdx>` type system** — UNCHANGED. PR-Y22 + PR-Y20 stay in force; PR-Y24 builds on them.
- **PR-Y17-COPLANAR L264 panic** — different mechanism; banked PR-Y18 territory.
- **F0030 sibling Euler V-E+F=3 defect** — different defect class (PR-Y23+ candidate per plan adversary gate 2); UNCHANGED.
- **F0050 sibling watertight defect** — different defect class; UNCHANGED.
- **5 L264 panic cases (R0014/R0046/R0055/R0081/F0075)** — different mechanism; banked.
- **F0086, F0031-F0040, R0020/R0021, R0071** — out of scope.
- **Reference C++ sidecar comparison for PR-Y24** — internal correctness verifiable via I1+I2+I3 invariants + canary §1+§2 simulated-vs-actual mechanism confirmation. Sidecar parity is for cases where Cherchi output divergence is suspected; canary §2 verified the cohort is structurally invariant under both keying schemes. Reserved for §7.1 ABORT escalation only.
- **Fillet, chamfer, shell** — DEFERRED INDEFINITELY per CLAUDE.md.

---

## §10 NO fallback paths discipline (per `feedback_yang_only.md`)

- **No tolerance widening.** The new construction-time-keyed predicate uses exact `BrepVIdx` equality on directed-edge tuples — same equality semantics as the existing arena-traversal predicate. No `tau_*` tolerance is introduced.
- **No special-case branches.** The predicate fires uniformly across all half-edges with `twin == None`. The `he_with_no_construct_dest` count from canary P2 was `0` across all 9 measured invocations (F0020 ×2 + F0044 batch ×7); per canary §6 this is structurally guaranteed because every HE inserted at L1133 is registered into `directed_he` at L1149-1152 in the same loop iteration. If a future regression surfaces a HE with `he_to_constructed_dest[i] == usize::MAX`, that is a CONTRACT VIOLATION between L1133 and L1149-1152 — debug the upstream emission, do NOT add a fallback to arena-traversal lookup.
- **No alternative-validator path.** B1's plumbed `TopoArena.constructed_directed_edge` field is the **single** source of truth for the validator's NMM-classification predicate post-PR. The validator does not consult `arena.half_edges[he.next.0].origin` for this predicate. (Twin-symmetry check at L1254-1290 continues to read `he.twin` and `arena.half_edges[t.0].twin` — that is unchanged because twin-symmetry IS arena-structure invariant, distinct from NMM-classification which is patch-boundary-set invariant.)
- **No silent fallback on missing field data.** If `arena.constructed_directed_edge[i]` is `None` for any HE during validator consumption, that is a CONTRACT VIOLATION between Step 7-close population and validator consumption — panic per A15.5 hardening contract (per `yang_pr_y14a_outcome.md` PR-Y15c-fix-2.2 precedent: silent fallback → panic on miss). This guards against I2 measurability gaps in the field-population logic.

---

## §11 FIP role table

| Sub-phase | Agent | Reads required | Writes |
|---|---|---|---|
| 0a Canary | canary-y24 | Yang §3 + §4.4.2; Cherchi §3 + §5; PR-Y23 abort + canary memos; plan; `feedback_anchor_before_fix.md`; `feedback_adversary_no_destructive_git.md` | `docs/audits/pr_y24_anchor_canary.md` (commit `69c6c2b`) — DONE |
| 0b Spec | spec-y24 (this agent) | Same papers; canary memo from 0a; `feedback_external_coherence.md`; `feedback_no_last_bug.md`; `feedback_yang_only.md`; `feedback_local_fix_for_global_invariant.md`; FIP §3 + §8 | this spec |
| 0c Test | test-y24 | Same papers; this spec; `pr_y22_mode_a_missing_regression.rs` as helper template; `feedback_validate_against_corpus.md` | `crates/test-harness/tests/pr_y24_oracle_observation_layer_regression.rs` (RED-phase demonstrated) |
| 0d Implement | impl-y24 | Same papers; this spec; canary memo; failing test from 0c; CLAUDE.md P9-P10; `feedback_implementer_anti_fabrication_diff.md` | Code edits at Site A (`topology_extract.rs:1445-1471`, L1515) + Site B (`yang_integration.rs:1241-1308`) + `TopoArena` field add (`crates/kernel/src/topology/arena.rs`) + Step 7-close populate (`topology_extract.rs` after L1380) |
| 0e Adversary | adv-y24 | All 0a-0d; `feedback_adversary_recommendations_need_canary.md`; `feedback_local_fix_for_global_invariant.md` | `docs/audits/pr_y24_validation.md` with verdict + gates 1-7 results |
| 0f Close-out | lead-y24 | All 0a-0e; `governance/DEFINITION_OF_DONE.md`; CLAUDE.md WASM two-step; `feedback_per_plan_cycle_team.md` | clippy/fmt + WASM rebuild + memory updates + commit + push + TeamDelete |

Per `feedback_oracle_credibility_via_role_separation.md`: NO agent performs more than one sub-phase. Spec-y24 (this agent) does not implement, test, or adversary. Spec-y24's role ENDS at writing this document and committing it.

---

## §12 test-y24 recommendations

**File:** `crates/test-harness/tests/pr_y24_oracle_observation_layer_regression.rs` (NEW; do NOT modify `pr_y22_mode_a_missing_regression.rs` or `pr_y23_open_loop_emission_regression.rs` — both stay in tree as historical records).

### §12.1 Helper reuse

Reuse helpers from `pr_y22_mode_a_missing_regression.rs:128-207` (already imported pattern at line 116-122):
- `capture_stderr<F, R>(f: F) -> (R, String)` — process-global FD swap for stderr capture.
- `max_twin_oracle_field(stderr: &str, key: &str) -> Option<usize>` — for `unpaired_count`.
- `count_twin_oracle_lines(stderr: &str, key: &str) -> usize` — diagnostic.
- `max_topo_extract_unpaired(stderr: &str) -> Option<usize>` — for cohort guard.
- `count_topo_extract_summary_lines(stderr: &str) -> usize` — diagnostic.

These can be either (a) re-imported via path (preferred — declare them as `pub fn` in `pr_y22_mode_a_missing_regression.rs` and `use` them; risks coupling tests), or (b) duplicated verbatim (preferred — tests are independent; ~80 LOC duplication acceptable per `feedback_implementer_anti_fabrication_diff.md` "tests are permanent"). **Recommendation: duplicate verbatim** to keep PR-Y24 test self-contained against future PR-Y22 test churn.

### §12.2 Two tests required

#### Test 1 — `pr_y24_f0020_twin_oracle_zero` (load-bearing per spec I3)

```rust
#[test]
#[ignore]
fn pr_y24_f0020_twin_oracle_zero() {
    let dir = Path::new(ASSAY_DIR);
    assert!(dir.exists(), "Assay corpus not generated yet at {ASSAY_DIR}");

    std::env::set_var("YANG_BOOLEAN", "1");
    std::env::set_var("TWIN_DEBUG", "1");

    let dir_owned = dir.to_path_buf();
    let (result, stderr) = capture_stderr(move || run_single_case(&dir_owned, "F0020", true));
    let _r = result.expect("F0020 must exist in corpus");

    let max_twin_unpaired = max_twin_oracle_field(&stderr, "unpaired_count");
    eprintln!(
        "[pr-y24-test] F0020 max [twin-oracle] unpaired_count: {:?} \
         (pre-PR-Y24 baseline: 2; post-PR-Y24 expected: 0; LOAD-BEARING)",
        max_twin_unpaired
    );

    let twin_unpaired = max_twin_unpaired.unwrap_or_else(|| {
        panic!(
            "[pr-y24-test] no [twin-oracle] unpaired_count line in F0020 stderr. \
             TWIN_DEBUG=1 gate failed or pipeline aborted before flood_fill_patches."
        )
    });
    assert_eq!(
        twin_unpaired, 0,
        "[pr-y24-test] PR-Y24 spec §5 I3 violation: F0020 max [twin-oracle] \
         unpaired_count = {} (expected 0). Per Yang §3 / Cherchi 2022 §3, \
         construction-time directed_he keying must reclassify HE 58 + HE 59 \
         (origin/dest v27/v38) as legitimate-NMM. See spec §2.3 + canary §1.",
        twin_unpaired
    );
}
```

#### Test 2 — `pr_y24_f0044_topo_extract_no_regression` (cohort guard per spec I2 + §6 oracle 3+4)

```rust
#[test]
#[ignore]
fn pr_y24_f0044_topo_extract_no_regression() {
    let dir = Path::new(ASSAY_DIR);
    assert!(dir.exists(), "Assay corpus not generated yet at {ASSAY_DIR}");

    std::env::set_var("YANG_BOOLEAN", "1");
    std::env::set_var("TWIN_DEBUG", "1");

    let dir_owned = dir.to_path_buf();
    // F0044 spotlight runs the F0044+F0045+R0092 batch (canary §2). 7
    // flood_fill_patches invocations total.
    let (result, stderr) = capture_stderr(move || run_single_case(&dir_owned, "F0044", true));
    let _r = result.expect("F0044 must exist in corpus");

    let max_topo_unpaired = max_topo_extract_unpaired(&stderr);
    let max_twin_unpaired = max_twin_oracle_field(&stderr, "unpaired_count");
    eprintln!(
        "[pr-y24-test] F0044 batch max [topo-extract] summary unpaired: {:?} \
         (canary §2: pre-PR baseline 0 across 7 invocations; cohort guard, must stay 0)",
        max_topo_unpaired
    );
    eprintln!(
        "[pr-y24-test] F0044 batch max [twin-oracle] unpaired_count: {:?} \
         (canary §2: pre-PR baseline 0 across 7 invocations; cohort guard, must stay 0)",
        max_twin_unpaired
    );

    // Both counters must stay 0 post-PR per spec I2 + §6 oracles 3+4.
    let topo_unpaired = max_topo_unpaired.unwrap_or(0);  // 0 if no line; structural absence = no defect
    let twin_unpaired = max_twin_unpaired.unwrap_or(0);
    assert_eq!(
        topo_unpaired, 0,
        "[pr-y24-test] PR-Y24 spec I2 cohort regression: F0044 batch max \
         [topo-extract] summary unpaired = {} (expected 0). Pairing logic at \
         topology_extract.rs:1219-1380 must remain byte-identical pre/post PR-Y24.",
        topo_unpaired
    );
    assert_eq!(
        twin_unpaired, 0,
        "[pr-y24-test] PR-Y24 cohort regression: F0044 batch max \
         [twin-oracle] unpaired_count = {} (expected 0). Per canary §2 baseline.",
        twin_unpaired
    );
}
```

### §12.3 RED-phase requirements

Per FIP §4.4: tests must FAIL on commit `3c749a3` (the canary baseline) before implementation lands. Run order:

```bash
# Verify red phase on baseline
git worktree add /tmp/y24-redphase-wt 3c749a3
cd /tmp/y24-redphase-wt
# Copy this PR's test file into the worktree (it doesn't exist on 3c749a3)
cp /home/claude/workspace/crates/test-harness/tests/pr_y24_oracle_observation_layer_regression.rs \
   crates/test-harness/tests/

YANG_BOOLEAN=1 TWIN_DEBUG=1 cargo test -p test-harness \
    --test pr_y24_oracle_observation_layer_regression -- \
    --ignored --nocapture --test-threads=1
# Required outcome:
#   pr_y24_f0020_twin_oracle_zero: FAIL (max=2, expected 0)
#   pr_y24_f0044_topo_extract_no_regression: PASS (already 0)
```

Document red-run output in test-y24's commit message (verbatim stderr line `[twin-oracle] unpaired_count=2` from F0020 b#2, plus the `assert_eq!` failure message).

After red-run, remove worktree: `git worktree remove /tmp/y24-redphase-wt`.

### §12.4 Test gate flags

- `#[ignore]`-gated (must run with `--ignored`).
- `--test-threads=1` mandatory (FD redirection + `set_var` global state, per pr_y22_mode_a_missing_regression.rs:111-114).
- Requires `YANG_BOOLEAN=1 TWIN_DEBUG=1` env vars (set within test body via `std::env::set_var`).

### §12.5 PR-Y23 historical test side-effect

`crates/test-harness/tests/pr_y23_open_loop_emission_regression.rs` (commit `770c5a2`) currently `#[ignore]`-passes the load-bearing F0020 gate but fails the cohort guard on PR-Y23 fix-shape (a). Per plan §"Verification" §3:

> `cargo test -p test-harness --test pr_y23_open_loop_emission_regression`
> PR-Y24 fix should make this test PASS as a side-effect (cohort guard + F0020 gate both GREEN).

Adversary should verify this test passes post-PR-Y24 as an additional cross-validation. Do NOT modify the PR-Y23 test file.

---

## §13 Definition of Done checklist (mirrors plan §"Definition of Done")

`lead-y24` verifies before commit + push:

- [ ] Spec at `specs/yang_pr_y24_oracle_observation_layer.md` (this file) with all FIP §3.2 sections present
- [ ] Spec cites Yang §3 + Cherchi 2022 §3 verbatim (line-numbered) per `feedback_external_coherence.md`
- [ ] Spec carries forward PR-Y23 §8.3 citation hygiene (no "Yang §4.4.2 directional-symmetry" coinage)
- [ ] Test at `crates/test-harness/tests/pr_y24_oracle_observation_layer_regression.rs` with two tests per §12.2
- [ ] Tests demonstrably failed pre-fix on commit `3c749a3` (red-phase log captured per §12.3)
- [ ] Implementation did not modify spec or test files (FIP §5.1)
- [ ] All Verification commands in plan §"Verification" pass:
  - [ ] §1 PR-Y24 own gate (both tests pass)
  - [ ] §2 PR-Y22 regression preservation (still passes)
  - [ ] §3 PR-Y23 historical test now passes as side-effect
  - [ ] §4 F0020 spotlight (status flip OR different-panic per §7.2)
  - [ ] §5 Yang fast corpus ≥ 10/157 (expect ≥ 11 if F0020 returns)
  - [ ] §6 Kernel baseline 1250/29/42 unchanged
  - [ ] §7 Linters clean
  - [ ] §8 WASM rebuild green (per CLAUDE.md two-step)
- [ ] Adversary ACCEPT in `docs/audits/pr_y24_validation.md` covering gates 1-7
- [ ] CI green
- [ ] WASM rebuilt + bundled in same commit as Rust changes (per CLAUDE.md "WASM Rebuild Workflow")
- [ ] `git push` to origin/main per `feedback_always_push.md` (close-out task)
- [ ] TeamDelete + `/tmp/y24-*` scratch cleanup (canary worktree at `/tmp/y24-probe-wt`, any `/tmp/y24-*.log`, any `/tmp/y24-redphase-wt` from §12.3)

---

## §14 Critical files (anchor citations)

- `/home/claude/workspace/crates/kernel/src/boolean/topology_extract.rs:1101` — `directed_he` declaration (BTreeMap source)
- `:1131-1146` — arena half-edge population (I2 byte-identical scope)
- `:1149-1152` — `directed_he.entry().or_default().push(he_idx)` populate (ground truth source)
- `:1219-1380` — pairing-search loop (I2 byte-identical scope)
- `:1437-1471` — **Site A** [twin-oracle] guard + `arena_dir_edges` build (PR-Y24 oracle anchor)
- `:1452-1469` — [twin-oracle] unpaired-detection loop (consumer; reads via construction-time map post-PR)
- `:1483-1494` — [twin-oracle] collision_count (informational; UNCHANGED)
- `:1498-1540` — [twin-oracle] offender-trace (`v_dest` at L1515 also migrates)
- `/home/claude/workspace/crates/kernel/src/boolean/yang_integration.rs:1171` — `validate_yang_result_topology` signature
- `:1241-1247` — **Site B** validator analogous build (PR-Y24 validator anchor; B1 plumbing target)
- `:1254-1290` — twin-symmetry check (UNCHANGED — twin equality on `he.twin` is structural, not NMM-classification)
- `:1292-1310` — validator missing-edge defect Err emission (PR-Y24 reads `arena.constructed_directed_edge[i]` post-PR)
- `/home/claude/workspace/crates/kernel/src/topology/arena.rs` — `TopoArena` struct (B1 field-add target)
- `/home/claude/workspace/docs/audits/pr_y24_anchor_canary.md` — PR-Y24 canary (commit `69c6c2b`); §1 P1+P2 evidence; §2 cohort table; §5 amended scope recommendation; §6 banked findings 3+5
- `/home/claude/workspace/docs/audits/pr_y23_anchor_canary.md` — predecessor canary (commit `990571c`); §1 P3+P4 BV27/BV38/BV26 ground truth
- `/home/claude/workspace/docs/audits/pr_y23_abort.md` — option-banking memo; §3 cohort regression mechanism
- `/home/claude/workspace/refs/text/yang2025_hybrid_boolean.txt:248-249` — Yang §3 verbatim phrase ("each edge shared by two adjacent faces")
- `/home/claude/workspace/refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:251-254` — Cherchi §3 verbatim phrase ("well formed simplicial complex ... closed loops of non-manifold edges")
- `/home/claude/workspace/crates/test-harness/tests/pr_y22_mode_a_missing_regression.rs:128-207` — helper template (`capture_stderr`, `max_twin_oracle_field`, `max_topo_extract_unpaired`)
- `/home/claude/workspace/crates/test-harness/tests/pr_y23_open_loop_emission_regression.rs` — historical test (passes as side-effect post-PR-Y24 per plan §"Verification" §3)
