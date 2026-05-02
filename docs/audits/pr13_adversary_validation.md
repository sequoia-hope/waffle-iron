# PR13 adversary validation — determinism + scope-down honesty

**Author**: adversary, team `yang-trim-loop-chaining-pr13`
**Date**: 2026-05-02
**Branch**: `yang-trim-loop-chaining-pr13`
**FIP role**: §6 (adversary). Lead reads.

## §1 Methodology

Per FIP §6, validation slices V1–V8 against the spec at `7c8303d`
(second-amendment scope-down) + the fix commit `59123b9`. Hard budget 3 h.
Mutation testing per §6.3 (revert load-bearing change, re-run determinism
test). All commands `YANG_BOOLEAN=1` where the test path requires it.

Environment: cargo 1.x default profile, kernel suite ~13s, full corpus pairing
~228s. Three commits on the chain since `main`:

```
7c8303d spec(yang-pr13): second amendment — scope down to determinism win; PR14 anchor preserved
59123b9 fix(yang-pr13): scope-down — determinism + structural cleanup of flood_fill_patches::Step 6
7ea59d3 test(yang-pr13): red-phase tests for flood_fill_patches::Step 6 fix (Approach A)
d13bcf0 spec(yang-pr13): finalize §8 — Approach A on flood_fill_patches::Step 6 (PR12 anchor was wrong)
a26c913 audit(yang-pr13): trim-loop chaining diagnostic for R0020/R0021 violations
9a43f8b spec(yang): PR13 trim-loop chaining bijective fix scope (TBD §8)
```

(Six commits — mission text said "7" but the 6 listed SHAs are exhaustive.)

## §2 Headline result

| Metric                     | PR12 baseline | PR13 measured | Δ        |
|---------------------------:|--------------:|--------------:|---------:|
| Stage1Bijective first-fail | 15            | 13            | −2       |
| Stage2Arrangement          | 25            | 26            | +1       |
| Stage4bClassification      | 0             | 0             | 0        |
| Stage5PatchSegment         | 0             | 0             | 0        |
| Stage6Assembly             | 29–31         | 32            | +1–+3    |
| AllPass                    | 84            | **83**        | **−1**   |
| Timeout                    | 4             | 3             | −1       |

R0021 NB-pair count: previously 6 vs 7 flap per T2 §6 (note: T2 audit at
`a26c913` says 6 vs 7; PR13 spec/commit message rewrites this as "5/6/7" —
modest over-claim, see §6). Post-PR13: stable at 7 across 3 in-process and
3 cross-process runs. **Determinism win is real and reproducible.**

R0020 stable at 2 NB pairs, R0021 stable at 7 NB pairs across all 6 runs in
this validation (3 in-process from T6, 3 cross-process from V1).

## §3 V1–V8 results

### V1 — Determinism stability (3 runs of T2 probe)

PASS. Three consecutive runs of `pr13_trim_loop_diagnostic` produce
byte-identical violation tables (R0020 2 pairs, R0021 7 pairs, identical face
indices, identical degree classifications). `diff` returns 0 between runs 1↔2
and 2↔3.

### V2 — Corpus regression (full pairing histogram)

PASS-with-noise. AllPass = 83 vs PR12 baseline 84. Single-case drop is within
the V2 criterion's "small noise tolerated" band but worth flagging: PR12
adversary explicitly noted F0018, F0076, R0046 as flap-prone cases in
PR12's 84 measurement. PR13 places R0034 + F0076 in S6 (vs PR12's S1) — a
cascade reordering consistent with the BTreeMap suite's growing reach into
PR12's flap-prone cluster. The single-case AllPass drop is plausibly one of
those cases drifting from "accidentally AllPass on PR12 sample run" to "S6
on PR13 sample run". Not a regression of the PR13 fix per se.

### V3 — Known-PASS spot-check

PASS. F0001 AllPass; R0018 AllPass; R0052 in the AllPass list of the
pairing run. The passcheck test panel itself does not include R0052 in
its fixed table, but the corpus run confirms it.

### V4 — R0020/R0021 still-failing (T3 red tests)

PASS as deferred. With `--ignored`:
- Test 1 (`synthetic_two_face_bijective_baseline`): pass
- Test 2 (`synthetic_same_direction_anti_fixture_caught`): pass
- Test 3 (`r0020_corpus_regression_red_phase`): FAIL (expected — deferred to PR14)
- Test 4 (`r0021_corpus_regression_red_phase`): FAIL (expected — deferred to PR14)
- Test 5 (`cluster_x_cascade_resolution_red_phase`): FAIL (expected — deferred to PR14)
- Test 6 (`r0021_determinism_stability_red_phase`): pass — headline win

Distribution exactly matches spec second amendment §470–477.

### V5 — Mutation testing on the determinism invariant

**FAIL — single-mutation T6 is not load-bearing.**

Mutation A (revert BTreeMap → HashMap on `topology_extract.rs:669`, the
exact load-bearing change of PR13 per spec §8 task 1): T6 STILL PASSES.
Cross-process runs (3 separate cargo invocations) all produce stable
"A 7 pair(s) of 23, B 0 pair(s) of 2".

Mutation B (revert dedup + sort + FIFO `remove(0)` while keeping BTreeMap):
T6 also still passes — stable verdict across 3 in-process runs, byte
identical Stage 1 message.

Implication: **T6 cannot distinguish PR13's changes from no-PR13.** The
underlying determinism is provided by an EARLIER fix —
`extract_trim_boundaries::adj` was converted to BTreeMap by PR12 commit
`7e119cc` (line ~1084 in topology_extract.rs). PR13's Step 6 BTreeMap
on line 669 is a defensive belt-and-suspenders change that is not
exercised by R0021's test path in any mutation tested here.

This is not a structural defect in the code — BTreeMap-on-line-669 is
strictly more deterministic than HashMap-on-line-669, and removing
non-determinism from a code path is desirable. But PR13's claim that
"BTreeMap conversion fixes R0021 NB pair count flap" is mechanistically
unsupported by V5 mutation testing. The real flap fix landed in PR12.

This downgrades PR13's "determinism win" from "load-bearing structural
fix" to "code hygiene + cosmetic determinism guard". The structural
cleanup (dedup + sort + FIFO) is also load-bearing-unproven by V5: it
makes outputs more canonical at branch points but T6 is insensitive to
it.

### V6 — PR14 anchor honesty assessment

PASS. The PR14 anchor cites:
- `crates/kernel/src/boolean/yang_integration.rs:978` — `tessellate_waffle_solid`
  call after Yang pipeline. Verified: this is the `cached_mesh = ...` line and
  the function exists at `:988`.
- `crates/kernel/src/tessellation/mod.rs:218` — `needs_fan_welding`. Verified:
  this exact identifier is at line 218 (a `let mut` declaration), with
  later set-to-true points at 237 and 272 and a use site at 791.

Both anchors are real, in-tree, and plausibly related to per-face Render-LOD
tessellation generating non-byte-identical reciprocal mesh edges across a
shared B-Rep edge. The agent-impl claim that `patch_boundaries` are
reciprocal but `cached_mesh` post-yang_inner has 2 NB pairs on R0020 is
internally consistent with the V1 measurement of 2 NB pairs on R0020 from
the production pipeline.

Honest assessment: this is now the THIRD wrong anchor in the chain
(PR12 → `extract_trim_boundaries`; T2 → `flood_fill_patches::Step 6`;
T4 → empirically rejected → `tessellate_waffle_solid` Render-LOD).
The discipline of running an anchor probe BEFORE writing fix code (per
`feedback_anchor_before_fix.md`) caught the third miss exactly as designed.
PR14 work on the new anchor must be guarded by another anchor probe before
agent-impl writes any production code.

### V7 — Sanity tests + git hygiene

PASS.
- `cargo test -p kernel`: 1239 passed / 29 failed / 42 ignored — exact match
  with commit message claim.
- `cargo test -p kernel pipeline_oracles`: 12/12 pass.
- `git status --short`: clean except `app/tests/cases/assay/results.json` (M)
  and `output.obj` (??) — both are test-run artifacts that the spec already
  accepts.
- `git log --oneline main..HEAD`: 6 commits as listed in §1.

### V8 — Spec compliance with second amendment

PASS. `git show 59123b9` modifies exactly one file
(`crates/kernel/src/boolean/topology_extract.rs`, +41 −6) and the diff
contains exactly the three approved changes:
1. `HashMap` → `BTreeMap` on the local `adj` map.
2. `BTreeSet seen` dedup before `boundary.push(...)`.
3. `outs.sort_unstable_by_key(|&(t, _)| t)` after `adj` build, plus
   `match outgoing { Some(v) if !v.is_empty() => v.remove(0), _ => break }`
   replacing `outgoing.and_then(|v| v.pop())`.

NO canonical-edge picker (the reverted Approach A core).
NO cross-patch alternating-flip rule (also reverted).
No additional production-code changes elsewhere.

## §4 Mutation testing detail

V5 ran two mutations; both produced T6 PASS:

**Mutation A** (line 669): `BTreeMap` → `HashMap`. T6 passes 3 runs in
`A 7 pair(s) of 23` stably. Cross-process: 3 separate cargo invocations,
all matching.

**Mutation B**: dedup `BTreeSet` removed; `sort_unstable_by_key` removed;
`remove(0)` reverted to `pop()`. BTreeMap kept. T6 passes 3 runs identically.

Together these establish that **neither sub-component of PR13 individually
gates T6**. The likely explanation: the upstream `extract_trim_boundaries`
adj BTreeMap from PR12's `7e119cc` already canonicalises the trim-loop
extraction order; downstream `flood_fill_patches::Step 6` consumes this
already-canonical input, so its own internal map type does not surface to
the bijective oracle's count.

The structural changes (dedup + sort + FIFO) are still defensible as
**code hygiene**: they remove redundancy and ambiguity in the Step 6
data path, even if T6 cannot witness their effect. They do NOT
incrementally fix R0020/R0021's bijective contract — and the spec
correctly admits this.

Recommendation: do NOT remove the PR13 changes. They are strictly
hygienic improvements to a path that future work (PR14+) may exercise
more aggressively. But do NOT credit them with the determinism win
either; that win predates PR13.

## §5 PR14 anchor honesty assessment

The empirical evidence claim from agent-impl:

> patch_boundaries reciprocal; cached_mesh post-yang_inner has 2 NB pairs on
> R0020.

is consistent with the violation table from V1 (R0020: 2 pairs, both
`byte_eq=true / reciprocal=false`). The data flow is plausible: if
`flood_fill_patches::patch_boundaries` are reciprocal but the rendered mesh
is not, the divergence must occur between flood-fill output and the
final mesh — `tessellate_waffle_solid` at `yang_integration.rs:978` is
exactly that boundary. Not proven (we did not instrument production code in
this audit), but plausible.

The named PR14 candidate paths exist:
- `tessellate_waffle_solid` is at `yang_integration.rs:988`.
- `needs_fan_welding` is at `tessellation/mod.rs:218`.
- Bounded-tessellation LOD=64 is the Render LOD value the call uses.

**Verdict on the PR14 anchor**: honest and grounded. PR14 must add an
anchor probe that instruments the production
`tessellate_waffle_solid → needs_fan_welding → bounded_tessellation` path
on R0020 BEFORE writing fix code, per `feedback_anchor_before_fix.md`.

## §6 Findings

### Spec deviations / drift

- **Flap range over-claim**: PR13 spec §8 second amendment (line ~474) and
  the `59123b9` commit message both write "R0021 NB count stable at 7 (was
  flapping 5/6/7)". T2 audit at `a26c913` documents the flap as "6 vs 7"
  (R0020=2 stable, R0021=6–7 flapping). Spec/commit say "5/6/7"; the T2
  primary record says "6 vs 7". Minor — does not change the headline.
  Worth correcting in the spec for honesty.

- **V5 mutation testing not load-bearing**: T6 is insensitive to single
  mutations of either the BTreeMap conversion or the dedup+sort+FIFO
  changes. The PR12 `7e119cc` BTreeMap on `extract_trim_boundaries:1084`
  appears to be the actual mechanism keeping R0021 stable at 7. PR13's
  changes are hygienic but not mechanistically load-bearing under V5
  mutation. This is documented honestly in §4 above; not a regression
  but a reporting gap.

### Regressions

- **AllPass 84 → 83**: single-case drop. Within V2 noise tolerance per
  the mission's "±2 small noise tolerated" criterion. Most likely a Cluster
  Z residual flap-prone case (F0018 / R0046 / F0076 territory) drifting
  off AllPass on this sample run. Not a structural regression of the
  PR13 fix.

- No new oracle violations on F0001, R0018, R0052 (V3).

### Missed deferrals

None. Tests 3, 4, 5 stay red as the spec second amendment commits to.

### Scope drift

None. `59123b9` touches only `topology_extract.rs` with the three approved
edits. No canonical-edge picker, no cross-patch flip rule.

## §7 Verdict

**BLESS with notes.**

Ship PR13. The structural cleanup is a strict improvement to the
`flood_fill_patches::Step 6` path. The spec second amendment is honest
about the scope-down: R0020/R0021 are deferred, the PR14 archaeological
anchor is recorded, and the third wrong-anchor episode is acknowledged
in writing.

Notes for lead to record / forward to PR14:

1. **V5 mutation testing showed PR13's BTreeMap is not load-bearing
   for T6** — the determinism mechanism predates PR13 (PR12's
   `7e119cc`). Update the spec/commit message language to "hygienic
   alignment with PR12's BTreeMap discipline" rather than "fixes flap"
   when summarising. The T6 headline (R0021 stable at 7) is real;
   the *attribution* to PR13 is not mechanistically established.

2. **Flap range mis-cited**: T2 says 6 vs 7; spec says 5/6/7. Fix wording.

3. **AllPass 84 → 83** is within noise but flag any further drop in
   future PRs as potential regression. PR12 adversary already noted
   the histogram is run-to-run unstable in the F0018/R0046/F0076
   cluster.

4. **PR14 anchor must run anchor probe before agent-impl touches code**.
   This is the third anchor miss in a row; the discipline of empirical
   anchor verification is the only thing that has prevented merging
   wrong fixes — preserve it.

## §8 Sanity rerun after mutations reverted

After V5 mutations were reverted, the working tree is clean (matches
HEAD on `crates/`). Synthetic tests (Test 1 + 2) re-run cleanly.
Production code identical to `59123b9`.

---

Per FIP §1 P5: this audit was performed by adversary, distinct from
agent-spec, agent-diagnose, agent-test, agent-impl. Lead reads.
