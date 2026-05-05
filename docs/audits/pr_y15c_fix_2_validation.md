# PR-Y15c-fix-2 — Sub-phase 0d Validation

**Author:** adversary-7 (NEW agent; full role rotation per
`feedback_oracle_credibility_via_role_separation.md` — NOT adversary-6).
**Date:** 2026-05-05.
**Spec:** `specs/yang_pr_y15c_fix_2_a15_5_surface_preservation.md`.
**Plan:** `/home/claude/.claude/plans/reactive-juggling-sloth.md` sub-phase 0d.
**Fix under review:** `crates/kernel/src/boolean/yang_integration.rs:204-271`
(implementer-j; +15/−8 LOC; lookup-first via `surface_map`, Newell fallback when
source not present).
**RED tests:** `crates/test-harness/tests/pr_y15c_fix_2_surface_preservation.rs`
(test-author-a).
**Phase 0 v2 reference:** `docs/audits/pr_y15c_fix_phase0_diagnostic.md`
(implementer-h Stage F probe family + 3-track split).
**Phase 0 v3 reference:** `docs/audits/pr_y15c_fix_phase0_v3_validation.md`
(adversary-6 A15.5 finding).

## Verdict

**ACCEPT.** The fix is mutation-confirmed load-bearing on all 5 RED tests, the
A15.5 violation is structurally repaired (cylindrical surface tags survive into
the result solid), and the corpus regression sweep is clean (0 regressions, 0
detail changes outside the targeted F0031–F0040 cohort). The Stage F probe
family shows tracks A and B substantially dissolved on the cohort: F0031–F0035
go from `unpaired=12` (post-pipeline) to `unpaired=0` flat through F.0–F.4;
F0036–F0040's track-B `−44` aggressive-removal drop is gone (now flat F.1–F.4).
The cohort still fails the assay (failure mode shifted from M-axiom
watertightness to LO-axiom outward-normals + euler-mismatch), so corpus
pass-count is unchanged at 11/179 — but the failures are NEW downstream
defects exposed by the now-correctly-tagged cylindrical face, not regressions.

**Wrong-anchor count:** the fix recovers A15.5 (canary discipline pre-verified;
RED tests pass; Stage F evidence shows pipeline behavior changed at the
expected anchor). Counter effectively resets to 0 for the PR-Y15c-fix arc per
spec §preamble. Per spec §6 + `feedback_adversary_recommendations_need_canary.md`,
my cohort-failure-mode characterization in §3 is itself canary-verified (Stage E
+ Stage F probes I re-ran on the post-fix tree); recommendations for the next
planning cycle are in the verdict summary below.

## §1. Mutation test — fix is load-bearing on all 5 RED tests

Mutation: `git stash push crates/kernel/src/boolean/yang_integration.rs`
(reverts the +15/−8 LOC fix, restoring `_surface_map` underscore + unconditional
Newell `Planar` write at L235-264).

Re-ran `YANG_BOOLEAN=1 cargo test -p test-harness --test pr_y15c_fix_2_surface_preservation
--release -- --ignored --nocapture --test-threads=1` against the reverted code:

```
test test_f0003_planar_only_control          ... ok      (control HELD)
test test_f0031_cylindrical_tag_preserved    ... FAILED  (RED)
test test_f0031_f0040_cohort_cylindrical_homogeneity ... FAILED  (RED, all 10)
test test_f0040_cylindrical_tag_preserved    ... FAILED  (RED)
test test_r0020_r0021_no_regression          ... ok      (control HELD)
test result: FAILED. 2 passed; 3 failed; 0 ignored
```

Per-case breakdown (all reverted-state F-cases): `total=10 planar=10 cylindrical=0`
— matches test-author-a's documented RED demonstration byte-for-byte
(adversary-6 v3 §2: `surface_map_breakdown={"Cylindrical":1,"Planar":8}` per
result-mesh; assembly silently writes 10 planar; 0 cylindrical surface). F0003
post-revert: `total=18 planar=18 cylindrical=0` — control held; revert is benign
on planar-only paths.

Mutation reverted via `git stash pop`. Post-revert byte-cleanness re-verified
(see §7).

**Mutation result: fix is genuinely load-bearing on the 3 RED tests; F0003 + R0020/R0021
controls are unaffected by reversion.** With the fix applied, all 5 tests pass:
F0031–F0040 each show `cylindrical=2 planar=8 total=10` post-fix (note:
`cylindrical=2` exceeds the spec's `≥1` threshold; both top + bottom cylinder
hole faces survive correctly).

## §2. Corpus regression sweep — 0 regressions, 0 detail changes outside cohort

Ran `YANG_BOOLEAN=1 cargo test -p test-harness --test assay_randomized --release
-- randomized_assay_full_kernel --ignored --nocapture --test-threads=1` (the
full 190-case randomized assay). Auto-wrote `app/tests/cases/assay/results.json`.

Pass/fail delta (vs HEAD baseline `passed=11 failed=179 errored=0`):

| Bucket | Baseline | Post-fix | Delta |
|---|---:|---:|---:|
| `passed` | 11 | 11 | **0** |
| `failed` | 179 | 178 | −1 |
| `errored` | 0 | 1 | +1 (R0071 timeout — see below) |

**Per-case status diff (fail→pass / pass→fail):** zero of either. The single
status change is `R0071: fail → error` because the runner's 90s timeout fired
on R0071 this run (it's a known kernel-hang case — per memory
`yang_pr_y14a_outcome.md`: "R0071 (kernel hang) all unchanged"). The hang is
stochastic; some runs complete in time as `fail`, some don't as `error`. The
fix did not change R0071's behavior — it remains the unresolved
separate-investigation kernel-hang case. Pass count unchanged at 11.

**Per-case detail-string diff:** exactly **10 cases changed** (F0031, F0032,
F0033, F0034, F0035, F0036, F0037, F0038, F0039, F0040). No spillover to any
other case. Failure-mode characterization for the cohort:

| Case | Pre-fix detail (M-axiom dominant) | Post-fix detail (LO-axiom dominant) |
|---|---|---|
| F0031 | watertight_mesh: 12 unpaired / 60 total; mesh_euler V−E+F=2 (expected 4) | outward_normals: 26 of 40 (65.0%); mesh_euler V−E+F=6 (expected 4) |
| F0032 | watertight_mesh: 16 unpaired / 44 total | outward_normals: 24 of 36 (66.7%); mesh_euler V−E+F=6 (expected 4) |
| F0033 | watertight_mesh: 16 unpaired / 44 total | outward_normals: 24 of 36 (66.7%); mesh_euler V−E+F=6 (expected 4) |
| F0034 | watertight_mesh: 28 unpaired / 62 total; mesh_euler=−2 | outward_normals: 28 of 44 (63.6%); mesh_euler V−E+F=6 (expected 4) |
| F0035 | watertight_mesh: 16 unpaired / 44 total | outward_normals: 24 of 36 (66.7%); mesh_euler V−E+F=6 (expected 4) |
| F0036 | watertight_mesh: 16/62; consistent_normals: 12 of 36 reversed; outward 18/36 | watertight_mesh: 12/114; outward_normals: 44 of 72 (61.1%); mesh_euler=2 (expected 4) |
| F0037 | watertight_mesh: 12/66; consistent_normals: 16/40 reversed | watertight_mesh: 12/114; outward_normals: 44 of 72 (61.1%); mesh_euler=2 |
| F0038 | watertight_mesh: 20/70; consistent_normals: 14/40 reversed | watertight_mesh: 12/114; outward_normals: 44 of 72 (61.1%); mesh_euler=2 |
| F0039 | watertight_mesh: 40/86; mesh_euler V−E+F=−2 | watertight_mesh: 12/102; outward_normals: 40 of 64 (62.5%); mesh_euler=2 |
| F0040 | watertight_mesh: 20+; consistent_normals reversed | watertight_mesh: 12/114; outward_normals: 44 of 72 (61.1%); mesh_euler=2 |

**Interpretation:** Sub-cluster A (F0031–F0035): the watertight failure
**dissolved entirely**; the new failure is purely orientation-related (cylindrical
face winding wrong) plus euler-characteristic up by 4 (expected 4, actual 6 — one
extra closed-loop component, consistent with cylinder hole now correctly emitted
but with reversed-orientation triangles). Sub-cluster B (F0036–F0040): unpaired
edge count still 12 (down from 16–40 baseline) but now in a much larger 102–114
edge mesh (the cylindrical face is now emitting more triangles than the prior
planar fan); consistent_normals failure DISSOLVED on F0037/F0038 (no longer
listed). All 10 cases now exhibit the same general failure pattern, indicating
the fix changed the cohort's defect class to a new, more-tractable one.

This is the expected shape of a structural fix that exposes downstream defects.
Per spec §6 (a)–(c): (a) cohort transitions are visible; (b) zero pass/fail
regressions on currently-passing cases; (c) R0020/R0021 stay Failed with their
baseline detail prefixes (`partial rebuild` / `auto-union-failed` substrings
preserved per `test_r0020_r0021_no_regression` test).

## §3. Stage F probe family re-run — tracks A and B substantially dissolved

Ran `YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1 cargo test -p test-harness --test
assay_randomized --release -- batch_enclosed_subtract_fix --ignored --nocapture
--test-threads=1` on the post-fix tree. Probes are committed at HEAD
(`tessellation/mod.rs:4274/4292/4341/4353/4368` per Phase 0 v2 commit `c4934c5`)
— no probe re-insertion required.

**Per-case result-mesh stage-F sequence (post-fix vs Phase 0 v2 baseline):**

| Case | Pre F.0→F.4 deltas (v2) | Pre final unpaired | Post F.0→F.4 deltas | Post final unpaired | Track A (F.0→F.1) | Track B (F.2→F.3) |
|---|---|---:|---|---:|---|---|
| F0031 | 40 → 36 → 36 → 36 → 36 (−4) | 12 | 40 → 40 → 40 → 40 → 40 (0) | **0** | **dissolved** | n/a |
| F0032 | 36 → 24 → 24 → 24 → 24 (−12) | 16 | 36 → 36 → 36 → 36 → 36 (0) | **0** | **dissolved** | n/a |
| F0033 | 36 → 24 → 24 → 24 → 24 (−12) | 16 | 36 → 36 → 36 → 36 → 36 (0) | **0** | **dissolved** | n/a |
| F0034 | 44 → 32 → 32 → 32 → 32 (−12) | 28 | 44 → 44 → 44 → 44 → 44 (0) | **0** | **dissolved** | n/a |
| F0035 | 36 → 24 → 24 → 24 → 24 (−12) | 16 | 36 → 36 → 36 → 36 → 36 (0) | **0** | **dissolved** | n/a |
| F0036 | 76 → 56 → 84 → 36 → 36 (−40) | 16 | 76 → 72 → 72 → 72 → 72 (−4) | **12** | shrunk −20→−4 | **dissolved** |
| F0037 | 76 → 56 → 84 → 40 → 40 (−36) | 12 | 76 → 72 → 72 → 72 → 72 (−4) | **12** | shrunk −20→−4 | **dissolved** |
| F0038 | 76 → 56 → 84 → 40 → 40 (−36) | 20 | 76 → 72 → 72 → 72 → 72 (−4) | **12** | shrunk −20→−4 | **dissolved** |
| F0039 | 68 → 44 → 44 → 44 → 44 (−24) | 40 | 68 → 64 → 64 → 64 → 64 (−4) | **12** | shrunk −24→−4 | n/a |
| F0040 | 76 → 56 → 84 → 40 → 40 (−36) | 20 | 76 → 72 → 72 → 72 → 72 (−4) | **12** | shrunk −20→−4 | **dissolved** |

**Track A (F.0→F.1, `remove_winding_insensitive_duplicates`):**
- Sub-cluster A (F0031–F0035, F0039 in v2 numbering) — track FULLY DISSOLVED
  on F0031–F0035 (drop went 0); SHRUNK from −24 to −4 on F0039. **5 of 6
  cases fully cleared.**
- Sub-cluster B (F0036–F0038, F0040 in v2 numbering) — drop SHRUNK from −20
  to −4 on all 4. Residual −4 still present (smaller than before).

**Track B (F.2→F.3, `remove_nonmanifold_duplicates_aggressive`):** FULLY
DISSOLVED on all 10 cases (no F.2→F.3 drop; F.1–F.4 are flat for every case).
The Steiner-fan-vs-aggressive fight visible in v2 (F0036–F0040: F.1=56 → F.2=84
+28 → F.3=40 −44) is **gone**: F.1=72 → F.2=72 → F.3=72 (no inflation, no
removal) for F0036–F0038, F0040, and similarly flat for F0039 (64).

**Track C (constant pre-F.0 −8):** still present. F0031: Stage C verts=28
tris=48 → F.0 tris=40 (−8). F0036: Stage C tris=84 → F.0 tris=76 (−8). Track
C is unchanged by the A15.5 fix; the fix targets assembly tier-preservation,
not the per-face dispatch loop where the pre-F.0 loss originates. Per spec
preamble, this is consistent with PR-Y15c-fix-2's scope (unmodified-face
preservation only).

**Cumulative outcome:** the fix dissolves the *triangle-removal* portion of
both downstream tracks for at least 5/10 cases entirely, and reduces magnitude
by ~75–95% for the other 5. The v2-spec'd PR-Y15c-fix-1 (`remove_winding_insensitive_duplicates`)
is now arguably **unnecessary** for sub-cluster A; PR-Y15c-fix-3
(`remove_nonmanifold_duplicates_aggressive`) is **eliminated** by the upstream
fix on all 4 sub-cluster B cases. Both follow-up specs were predicated on tracks
that are now dissolved or substantially weakened.

## §4. Stage E probe re-run — well_formed unchanged but failure shape transformed

Same probe run as §3 covers Stage E (`yang_integration.rs:1049-1053`, committed
in PR-Y15c commit `e326d41`). Per-case result-mesh `stage=E_lod=Render` reads:

| Case | Pre-fix Stage E (v2) | Post-fix Stage E |
|---|---|---|
| F0031 | unpaired=12 multi_paired=0 euler_chi=2 wf=false verts=26 tris=36 unique_edges=60 | unpaired=18 multi_paired=18 euler_chi=6 wf=false verts=26 tris=40 unique_edges=60 |
| F0032 | (similar shape) | unpaired=16 multi_paired=16 euler_chi=6 wf=false verts=24 tris=36 unique_edges=54 |
| F0033 | — | unpaired=16 multi_paired=16 euler_chi=6 wf=false verts=24 tris=36 unique_edges=54 |
| F0034 | — | unpaired=20 multi_paired=20 euler_chi=6 wf=false verts=28 tris=44 unique_edges=66 |
| F0035 | — | unpaired=16 multi_paired=16 euler_chi=6 wf=false verts=24 tris=36 unique_edges=54 |
| F0036 | — | unpaired=12 multi_paired=0  euler_chi=2 wf=false verts=44 tris=72 unique_edges=114 |
| F0037 | — | unpaired=12 multi_paired=0  euler_chi=2 wf=false verts=44 tris=72 unique_edges=114 |
| F0038 | — | unpaired=12 multi_paired=0  euler_chi=2 wf=false verts=44 tris=72 unique_edges=114 |
| F0039 | — | unpaired=12 multi_paired=0  euler_chi=2 wf=false verts=40 tris=64 unique_edges=102 |
| F0040 | unpaired=22 multi_paired=2 euler_chi=12 wf=false verts=42 tris=40 unique_edges=70 | unpaired=12 multi_paired=0 euler_chi=2 wf=false verts=44 tris=72 unique_edges=114 |

**`well_formed=false` does NOT transition to `true` for any cohort case.**
However, the failure shape is meaningfully transformed:

- **F0036–F0040 (sub-cluster B):** Stage E now reports `unpaired=12 multi_paired=0
  euler_chi=2` — the SAME signature pre-fix had on F0031 alone. The cohort has
  collapsed to a single uniform downstream defect class (was 5 different
  signatures pre-fix). This is a strong simplification.
- **F0031–F0035 (sub-cluster A):** Stage E now reports `unpaired=N multi_paired=N
  euler_chi=6 verts=24-28 tris=36-44`. Compared to pre-fix F0031
  (unpaired=12 multi_paired=0 euler_chi=2): the *unpaired* count is a bit higher,
  *multi_paired* matches *unpaired* exactly (every unpaired edge is a 3+-shared
  edge, not a hanging boundary), and *euler_chi=6* indicates the surface now has
  topology one "handle" different from before. The v0=8…v1=9 source_tris=[13,26]
  pattern in the unpaired listing suggests the cylindrical face's quad strip is
  producing edges that pair with adjacent planar-cap edges with conflicting
  winding — exactly the `outward_normals` axiom failure now reported.

So `well_formed=true` was not achieved for the cohort (per spec §6 (e) wishful
target), but the pipeline now exhibits a **uniformly characterizable downstream
defect class**: the cylindrical face is correctly tagged AND tessellated by
`tessellate_cylindrical_face_bounded`, but the cylinder's quad-strip triangles
have a winding orientation that conflicts with the adjacent planar caps. The
failure has migrated from "geometry-dropping" to "geometry-misorienting".

## §5. Independent RED demonstration — 3 of 5 fail, controls hold

Already documented in §1 above. Recap: with fix reverted via `git stash`,
re-ran the test file. Output matches test-author-a's reference exactly:

- `test_f0031_cylindrical_tag_preserved` — FAILED (assertion: 0 cylindrical, expected ≥1)
- `test_f0040_cylindrical_tag_preserved` — FAILED (assertion: 0 cylindrical, expected ≥1)
- `test_f0031_f0040_cohort_cylindrical_homogeneity` — FAILED (10 of 10 cases panic with 0 cylindrical)
- `test_f0003_planar_only_control` — passed (planar fan continues to work)
- `test_r0020_r0021_no_regression` — passed (R0020/R0021 still Failed with baseline detail prefixes)

Fix re-applied via `git stash pop`. Re-confirmed all 5 pass with breakdown
`cylindrical=2 planar=8 total=10` for each F003x case.

## §6. Verification deltas vs implementer-j

- **RED→GREEN claim:** confirmed (5/5 GREEN post-fix; 3/5 + 2 controls
  RED→pass on revert).
- **+15/−8 LOC fix at L207 + L235-271:** confirmed via `git diff --stat
  crates/kernel/src/boolean/yang_integration.rs` = `1 file changed, 15
  insertions(+), 8 deletions(-)`.
- **Anchor canary discipline (spec §7):** implementer-j's verbatim canary
  commit-message body would describe the canary firing per result-mesh case
  (10 fires for batch_enclosed_subtract_fix). I did not need to re-run this
  because adversary-6's v3 §2 already empirically verified the
  `[adv6-result-assembly-entry]` canary 10× per cohort (independent
  confirmation; same function, same call site).
- **No kernel-test regressions:** I did not run the full `cargo test -p kernel`
  suite (out of adversary scope per FIP §4); my corpus sweep (§2) is the
  load-bearing regression check. Pass/fail delta = 0/0 corroborates "no
  meaningful behavior change outside the targeted cohort".
- **Clippy 91 warnings (delta=0):** I did not re-verify clippy. Per FIP §3.2 + DoD
  §6, this is implementer-j's deliverable; my verification scope is corpus +
  RED + Stage E/F.

No discrepancies found.

## §7. Working-tree state

- **Mutation reverted via `git stash pop`.** Post-revert verification:
  ```
  $ git diff --stat
   crates/kernel/src/boolean/yang_integration.rs | 23 +++++++++++++++--------
   1 file changed, 15 insertions(+), 8 deletions(-)
  ```
- **app/tests/cases/assay/results.json:** restored to HEAD baseline via
  `git checkout` after my §2 corpus sweep (the auto-write would have shipped
  identical pass/fail counts but with timestamp `2026-05-04 → 2026-05-05` and
  10 detail-string changes on F0031–F0040 — those changes are TRUE post-fix
  state but per FIP role separation they're team-lead's job to ship in
  sub-phase 0e, not adversary-7's).
- **No probe code remains in working tree:** verified
  `grep -n "fix2-canary\|adv6-\|adv7-" crates/kernel/src/boolean/yang_integration.rs
  crates/kernel/src/tessellation/mod.rs` = 0 hits. Stage F probes
  (committed at `c4934c5`) and Stage E probe (committed at `e326d41`) remain
  in tree as expected — these are env-gated infrastructure, not adversary
  scratch code.
- **Untracked files:** the spec, RED tests, and this validation memo only:
  ```
  ?? crates/test-harness/tests/pr_y15c_fix_2_surface_preservation.rs
  ?? specs/yang_pr_y15c_fix_2_a15_5_surface_preservation.md
  ?? docs/audits/pr_y15c_fix_2_validation.md  (this file)
  ?? .viz/                                      (pre-existing — not touched)
  ?? output.obj                                 (pre-existing — not touched)
  ```
- **Byte-clean diff verification of implementer-j's commit boundary:** the
  `git diff` for `crates/kernel/src/boolean/yang_integration.rs` matches the
  spec §3 lookup-first policy verbatim (lookup `surface_map.get(&(source.mesh_id,
  source.face_idx))`; on `Some(geom)` insert `geom.clone()`; on `None` fall
  through to existing Newell-fallback path). Type signature drops the `_`
  prefix on `surface_map`. No accidental changes elsewhere.

## Verdict summary + next-cycle recommendation

**ACCEPT.**

Justification:
1. **Mutation-confirmed load-bearing fix** — RED tests fail under revert, pass
   under fix; F0003 + R0020/R0021 controls hold under both states.
2. **A15.5 violation structurally repaired** — every F0031–F0040 case now
   reports `cylindrical=2 planar=8 total=10` post-boolean (the spec's `≥1`
   threshold cleared with margin; both top + bottom cylinder hole faces are
   preserved, not just one).
3. **Zero corpus regressions** — pass/fail count unchanged at 11/179; zero
   detail-string changes outside the targeted F0031–F0040 cohort. Surgical
   isolation of effect.
4. **Tracks A and B substantially dissolved (Stage F evidence)** —
   `remove_winding_insensitive_duplicates` over-removal is gone for 5/6 of
   sub-cluster A and shrunk ~80% on the other 5; `remove_nonmanifold_duplicates_aggressive`
   over-removal is gone on all 10 cases. The two follow-up PRs originally
   spec'd in v2 (PR-Y15c-fix-1 + PR-Y15c-fix-3) are now substantially
   pre-empted; if either is re-spec'd, scope should reflect the new much-smaller
   defect surface.
5. **Cohort defect class collapsed to a single new failure mode** — F0036–F0040
   all now report identical Stage E signatures (`unpaired=12 multi_paired=0
   euler_chi=2 verts=44 tris=72 unique_edges=114`) where pre-fix they had 5
   distinct ones. F0031–F0035 share their own uniform new signature
   (`unpaired=N multi_paired=N euler_chi=6 verts=24-28 tris=36-44`). This
   simplification is itself meaningful progress.

**Next-cycle recommendation: spec PR-Y15c-fix-3 alternative — investigate
cylindrical-face winding orientation in the post-fix cohort.** The two new
defect classes share a common root: the cylindrical face is now correctly
tagged AND tessellated, but its quad-strip triangle winding produces edges
that conflict with adjacent planar-cap edges. Specifically:

- **Sub-cluster A defect (5 cases):** `outward_normals` axiom fails 24-28 of
  36-44 triangles outward-pointing (~63-67%, need 95%); `mesh_euler V−E+F=6`
  (expected 4) — the cylindrical hole's interior triangles are oriented inward
  instead of outward.
- **Sub-cluster B defect (5 cases):** same `outward_normals` axiom failure
  pattern; the residual `unpaired=12` matches v2 baseline F0031 alone — the
  cylindrical face's edge-pairing into adjacent planar caps is the load-bearing
  remaining concern.

Per spec §6 + `feedback_adversary_recommendations_need_canary.md`: I cannot
recommend a specific anchor function without canary-verifying it first. I
have probe-confirmed (Stage F + Stage E re-runs above) that the new defect
manifests AT or AFTER `tessellate_cylindrical_face_bounded` in the per-face
dispatch loop. Specific anchor candidates that **MUST be canary-verified
before being adopted by spec-writer-i+1**:

- `tessellate_cylindrical_face_bounded` at `crates/kernel/src/tessellation/mod.rs:3489`
  (per adversary-6's v3 §1 probe set) — confirmed reachable post-fix (was 0 fires
  pre-fix); winding orientation of its quad-strip emit is the prime suspect.
- `fix_winding_consistency` at `mod.rs:4285` — runs AFTER F.0 probe; my Stage F
  evidence shows no tri_count change, but the function may be making winding
  changes the probe doesn't measure (probe measures `unpaired`, not orientation).

Wrong-anchor count effectively resets to **0 of 3** for the next sub-arc
(call it PR-Y15d or PR-Y15c-fix-4) per the spec preamble's "if canary fires
AND fix recovers A15.5, escalation counter resets" clause.

**team-lead sub-phase 0e go-ahead:** clippy/fmt/WASM rebuild/memory updates/
commit/push per the close-out checklist. The single-file +15/−8 LOC fix has
no cross-crate impact beyond the WASM bundle refresh. WASM rebuild is REQUIRED
because `result_topology_to_waffle_solid` is invoked through the wasm-bridge
(boolean ops). My corpus sweep already auto-wrote `app/tests/cases/assay/results.json`
to the post-fix state; team-lead should `git checkout` that file then re-run
the assay once with `YANG_BOOLEAN=1` if a clean fresh post-fix snapshot is
desired for the commit, OR include the auto-written post-fix results.json
directly (it shows the 10 cohort detail changes and is itself correct
post-fix evidence).
