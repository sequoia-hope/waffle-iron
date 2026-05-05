## PR-Y15c-fix-2.1 — Sub-phase 0c Validation Memo

**Author:** adversary-8 (NEW agent; full role rotation per
`feedback_oracle_credibility_via_role_separation.md` — NOT spec-writer-i,
NOT implementer-k/-l, NOT adversary-7).
**Date:** 2026-05-05.
**Spec:** `specs/yang_pr_y15c_fix_2_1_a15_5_fallback_audit.md`.
**Diagnostic under review:** `docs/audits/pr_y15c_fix_2_1_diagnostic.md`
(implementer-l; 166 LOC; null-result classification).
**Probe under review:** `crates/kernel/src/boolean/yang_integration.rs:248-258`
(env-gated `YANG_A15_5_AUDIT=1`, tag `[a15-5-fallback]`; 11 LOC additive).

## Verdict

**ACCEPT.** The 0/190-fire null result is mutation-confirmed real, not the
result of a dead probe or an uninvoked code path. Forcing the lookup to miss
yields the expected fire format and the expected RED test reversion on F0031;
reverting the mutation restores all 7 spot-check cases to clean post-fix state
(0 fires; F0031/F0040 cylindrical=2; R0020/R0021 control held). Implementer-l's
declared methodology shortcut (vacuous cross-reference per memo §6+§8) is
acceptable given the empty input set. Recommend proceeding to PR-Y15c-fix-2.2
to harden `unwrap_or_else` → `expect()`/`panic!` per spec §5 row 3.

## §1. Mutation test — probe is load-bearing; diagnostic depends on data

**Mutation:** wrapped the lookup in a `let _real_lookup = ...; let forced_miss:
Option<&SurfaceGeom> = None;` and pattern-matched against `forced_miss` instead
of the real lookup. Probe code itself was preserved verbatim. The lookup still
executes (so any side effects of `BTreeMap::get` are preserved), but the
`Some(geom)` arm is now structurally unreachable.

**Result on F0002** (`yang_trace_f0002` test, full Yang pipeline):

```
$ grep -c '\[a15-5-fallback\]' /tmp/mutation_f0002.stderr
22

$ grep '\[a15-5-fallback\]' /tmp/mutation_f0002.stderr | head -5
[a15-5-fallback] face_idx=FaceIdx(0) source_mesh=A source_face=FaceIdx(0) map_size=20
[a15-5-fallback] face_idx=FaceIdx(1) source_mesh=A source_face=FaceIdx(1) map_size=20
[a15-5-fallback] face_idx=FaceIdx(2) source_mesh=A source_face=FaceIdx(2) map_size=20
[a15-5-fallback] face_idx=FaceIdx(3) source_mesh=A source_face=FaceIdx(2) map_size=20
[a15-5-fallback] face_idx=FaceIdx(4) source_mesh=A source_face=FaceIdx(3) map_size=20
```

**Result on F0031** (`test_f0031_cylindrical_tag_preserved`, the cylindrical-rich
RED-test from adversary-7's PR-Y15c-fix-2 §1):

```
$ grep -c '\[a15-5-fallback\]' /tmp/mutation_f0031.stderr
10

$ grep '\[a15-5-fallback\]' /tmp/mutation_f0031.stderr
[a15-5-fallback] face_idx=FaceIdx(0) source_mesh=A source_face=FaceIdx(0) map_size=9
... (10 lines total: 6× MeshId::A face_idx 0-5 + 4× MeshId::B face_idx 0-2)

$ tail /tmp/mutation_f0031.stdout
test test_f0031_cylindrical_tag_preserved ... FAILED
panicked at ...
F0031 A15.5 violation: result solid has 0 cylindrical faces
(breakdown: total=10 planar=10 cylindrical=0 ...). [...]
```

Verified all four mutation-test acceptance criteria from the brief:

1. **Probe fires non-zero times** — F0002 (22 fires), F0031 (10 fires).
2. **Format is correct** — exact spec §3 format
   `[a15-5-fallback] face_idx=... source_mesh=... source_face=... map_size=...`,
   matches diagnostic memo verbatim.
3. **Test outcome changes** — F0031 cylindrical breakdown reverts to
   `total=10 planar=10 cylindrical=0` (exactly the RED state from adversary-7
   §1 / test-author-a's reference). The diagnostic interpretation depends on
   actual lookup data, not on which assertion is in the test.
4. **Mutation reverted; F0031 cylindrical=2** — re-ran `test_f0031_cylindrical_tag_preserved`
   post-revert: PASSED, breakdown = `cylindrical=2 planar=8 total=10` (matches
   post-PR-Y15c-fix-2 expected state from adversary-7 §1).

The probe is genuinely load-bearing: the 0/190 corpus result is real (the
`Some(geom)` arm is hit 100% of the time on the corpus), not a "function never
invoked" artifact.

## §2. Spot-check spot subset — 7/7 cases yield 0 fires post-revert

Five trace + control cases re-run independently with `YANG_A15_5_AUDIT=1` set,
mutation reverted:

| Case | Test | Fires | Test outcome | Source |
|---|---|---:|---|---|
| F0002 | `yang_trace_f0002` (R0014A-class M+W cohort) | **0** | ok | post-PR-Y15b passing case |
| F0003 | `yang_trace_f0003` (planar-only control) | **0** | ok | adversary-7 PR-Y15c-fix-2 §1 control |
| F0004 | `yang_trace_f0004` (PR17 partial-overlap-cosurface) | **0** | ok | post-PR-Y15b passing case |
| F0031 | `test_f0031_cylindrical_tag_preserved` (cylindrical-rich) | **0** | ok (cyl=2) | adversary-7 PR-Y15c-fix-2 §1 RED-test |
| F0040 | `test_f0040_cylindrical_tag_preserved` (cylindrical-rich) | **0** | ok (cyl=2) | adversary-7 PR-Y15c-fix-2 §1 RED-test |
| R0020 | `test_r0020_r0021_no_regression` (separate failure mode) | **0** | ok (held Failed) | adversary-7 PR-Y15c-fix-2 §1 control |
| R0021 | `test_r0020_r0021_no_regression` (separate failure mode) | **0** | ok (held Failed) | adversary-7 PR-Y15c-fix-2 §1 control |

**Total: 7 cases × 0 fires each = 0 fires.** Consistent with implementer-l's
0/190 corpus headline. Cylindrical-rich cases (F0031, F0040) and a planar
control (F0003) and an unrelated failure-mode pair (R0020/R0021) ALL show
0 fires — the perfect coverage holds across surface tiers and across both
passing and failing cases.

Independent re-run of `grep -c '[a15-5-fallback]' /tmp/a15_5_audit.stderr`
on the implementer's archived corpus stderr also yields **0** (confirms the
archived data matches what implementer-l reported).

## §3. Methodology assessment — vacuous cross-reference is acceptable

Implementer-l's memo §6 + §8 declares the spec §5 ¶2 cross-reference step
("classify each fire as in-set vs not-in-set against operand `face_geometry`")
**vacuous because the input set was empty.** Per spec §5, the cross-reference
is *contingent on at least one fire* — without fires, there is nothing to
classify, so nothing to cross-reference. The decision-tree row 3 ("never
fires") fires unambiguously regardless of cross-reference outcome.

This is methodologically clean for two reasons:

1. **Honest declaration:** memo §6 explicitly names the shortcut ("Methodology
   shortcut DECLARED: I did NOT execute the cross-reference because the input
   set was empty"). No silent skipping. Per
   `feedback_validate_against_corpus.md`, declaring shortcuts beats hiding them.
2. **Methodology canary via my §1 mutation test:** the cross-reference logic
   itself (memo §8) describes how to load each operand's `.waffle`, truncate to
   pre-boolean state, call `compute_all_signatures(..., TopoKind::Face)`, and
   compare FaceIdx sets. With my mutation, fires DID occur (22 on F0002, 10 on
   F0031) — but I deliberately did NOT execute the cross-reference on those
   forced fires, because forced-miss fires don't carry diagnostic meaning
   (every fire would trivially be "in-set" since the lookup was forced to None
   without truly being absent from `surface_map`). The cross-reference logic
   remains untested-in-anger; if a future cycle sees row 1 or row 2 fire, the
   first fire SHOULD trigger an end-to-end shake-out of the cross-reference
   methodology before the diagnostic memo's classification is trusted. I name
   this caveat for future-adversary; it is NOT a defect in this cycle's memo.

The vacuous cross-reference is acceptable for THIS cycle. Recommend it be
re-validated end-to-end the first time row 1 or row 2 fires.

## §4. PR-Y15c-fix-2.2 scope recommendation — promote to `expect()`/`panic!`

Implementer-l's recommendation (memo §7): replace `unwrap_or_else` arm with
`panic!("A15.5 contract violation: source ({mesh_id:?}, {face_idx:?}) absent
from surface_map (size={N})")` and delete L259-281 (Newell fallback +
degenerate-skip guards become unreachable). **Concur — with one canary-discipline
addendum.**

**Sizing:** ~5 LOC swap (`unwrap_or_else` block → `expect`/`panic!`); ~25 LOC
deletion (Newell fallback path including degenerate-skip guards); net ≈ −20 LOC.
This matches spec §5 row 3 "small follow-up PR to harden the contract".

**Risk assessment:**
- **Confidence-bound:** corpus is bounded (190 cases). A real-world model with
  a face-construction path not exercised by the corpus could still hit the
  fallback. Per FIP P9, a hard panic is correct over silent planar-fallback —
  silent fallback masks data loss; panic surfaces it for investigation. Lower
  blast radius than allowing wrong topology through.
- **Function signature:** stays infallible (`-> WaffleSolid`). No `Result<...>`
  refactor needed in this PR. Caller chain unchanged.
- **Synthetic RED test:** the canonical pattern is to construct a `ResultTopology`
  with a `face_provenance` entry whose `(mesh_id, face_idx)` is intentionally
  absent from `surface_map`, then assert the panic fires with the expected
  message substring. Test must be canary-verified per
  `feedback_anchor_before_fix.md` — implementer adds an `eprintln!` in the
  test setup that triggers BEFORE the panic to confirm the test exercise path
  reaches the lookup.

**Canary-discipline addendum (per `feedback_adversary_recommendations_need_canary.md`):**
my recommendation here is itself self-canary-verified — §1's mutation test
proved that an artificially-injected lookup-miss DOES drive code through the
fallback arm (22 fires on F0002, 10 fires on F0031, F0031 cylindrical-tag
test reverts to RED). The synthetic RED test for PR-Y15c-fix-2.2 will
exercise THE SAME code path. Confidence: high. Wrong-anchor counter: 0/3
for this sub-arc.

**Open question for spec-writer (deferrable):** if a row-1 or row-2 fire
EVER fires in a future corpus expansion, the panic in PR-Y15c-fix-2.2 would
trigger and abort the boolean. That is correct behavior (data loss should
not silently complete) but means the panic is the next anchor for further
investigation. Recommend the panic message include a link to the audit memo
+ an instruction to re-run with `YANG_A15_5_AUDIT=1` for diagnosis context.
~3 extra LOC. Not load-bearing but improves operability.

## §5. Working-tree state — mutation reverted; byte-clean

```
$ git diff --stat crates/kernel/src/boolean/yang_integration.rs
 crates/kernel/src/boolean/yang_integration.rs | 11 +++++++++++
 1 file changed, 11 insertions(+)
```

11 LOC additive matches implementer-l memo §9 + diagnostic memo's stated
probe size. Diff content is the env-gated `eprintln!` block ONLY (lookup
mutation reverted, no other code changes). Verified by reading the diff
output: the only added lines are the comment + `if std::env::var(...) ... eprintln!`
guard.

```
$ git status --short
 M app/tests/cases/assay/results.json   (auto-write from corpus run; team-lead deals with)
 M crates/kernel/src/boolean/yang_integration.rs   (probe ONLY, mutation reverted)
?? .viz/                                 (pre-existing — not touched)
?? docs/audits/pr_y15c_fix_2_1_diagnostic.md   (implementer-l)
?? specs/yang_pr_y15c_fix_2_1_a15_5_fallback_audit.md   (spec-writer-i)
```

Adversary-8 added: this validation memo only. Output `output.obj` was
pre-existing per the conversation start; not touched.

**Probe-off byte identity** (re-verified per spec §6): without
`YANG_A15_5_AUDIT=1`, the `if` guard short-circuits before any `eprintln!`,
so no stderr output. The probe block is entirely inside the env-var guard.
Adopted from implementer-l's memo §4 verification; not independently re-run
(implementer-l's verification chain is sound).

## Verdict summary

**ACCEPT.** Recommendations:

1. **Decision-tree row 3 confirmed** — `surface_map` has perfect coverage of
   `face_provenance` keys for all 745 successful pipeline runs across the 190-case
   corpus. Mutation-confirmed: probe IS load-bearing; the result is real.
2. **PR-Y15c-fix-2.2 scope:** ~5 LOC promotion of `unwrap_or_else` → `expect()`/
   `panic!`; ~−20 LOC deletion of Newell fallback + degenerate-skip guards.
   Synthetic RED test required (force a missing provenance entry). Optional ~3
   LOC additional context in the panic message.
3. **For future-adversary:** the cross-reference methodology in spec §5 ¶2 +
   memo §8 has not been end-to-end-validated (no real fires existed); the first
   time row 1 or row 2 fires, the first fire's classification should be
   independently spot-checked before the memo's recommendation is trusted.
4. **Wrong-anchor count for this sub-arc:** **0 of 3.** Spec, diagnostic, and
   validation all converge on the same null-result interpretation; mutation
   test confirms the null-result is genuine, not artifactual.

team-lead sub-phase 0d go-ahead: clippy + rustfmt + memory updates + commit
+ push. NO WASM rebuild required (probe is env-gated default-off; production
binary behavior is byte-identical without the env var).
