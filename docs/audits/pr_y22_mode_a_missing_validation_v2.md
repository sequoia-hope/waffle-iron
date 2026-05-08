# PR-Y22-MODE-A-MISSING sub-phase R-d — adversary-23 validation v2

**Author:** adversary-23 · **Date:** 2026-05-08 · **Plan:** PR-Y22-RECOVERY R-d
**Verdict:** **ACCEPT** — production diff exists, both regression tests PASS,
zero corpus regression, F0020 next-layer error string IDENTICAL to PR-Y21
ABORT layer (no NEW layer surfaced by PR-Y22).

Per `feedback_oracle_credibility_via_role_separation.md`: NEW agent role
distinct from adversary-22 (R-d, not R-0/a/b/c re-attempt).
NO destructive git operations (stash/checkout/reset on live tree).
Comparisons against baseline `ab65baa` performed via `git worktree add` to
`/tmp/y22-baseline-wt`, removed cleanly post-measurement.

---

## §1 Pre-flight: production diff exists

```
$ git diff HEAD --stat -- crates/kernel/src/boolean/topology_extract.rs
 crates/kernel/src/boolean/topology_extract.rs | 42 +++++++++++++++++++++++++--
 1 file changed, 40 insertions(+), 2 deletions(-)
```

**+40/-2 EXACTLY as expected** per the brief. Direct read of the diff
confirms M2 at L468-491 (canon-degenerate filter, mirrors
`exact_mesh.rs:1771-1775` pattern) AND M1 at L1287-1311 (NMM predicate
extension to `undirected_count != 2`, citing R-0 §1 incidence-3 evidence).
Both implementations match spec §3 + §4.1 verbatim. Adversary-22's v1 §1
"no production code change exists" finding is REFUTED — the diff was
empirically present at the start of R-d (status snapshot at `git status`
time confirms).

---

## §2 Gate 1: independent regression test re-run

```
$ YANG_BOOLEAN=1 TWIN_DEBUG=1 cargo test -p test-harness \
    --test pr_y22_mode_a_missing_regression -- --ignored --nocapture --test-threads=1

running 2 tests
test pr_y22_f0020_mode_a_missing_zero ...
[pr-y22-test] F0020 max `[topo-extract] summary: unpaired=N`: Some(0)
                                  (pre-PR-Y22 baseline: 8; LOAD-BEARING GATE)
[pr-y22-test] F0020 max `[twin-oracle] unpaired_count`: Some(2)
                                  (regression guard: must stay <= 2)
[pr-y22-test] F0020 case status=Failed detail=auto-union-failed ...
                                  half_edge[58].twin = None ... (38->27) ...
ok
test pr_y22_f0044_b5_mode_a_missing_drops_by_2 ...
[pr-y22-test] F0044 batch max `[topo-extract] summary: unpaired=N`: Some(0)
                                  (pre-PR-Y22 baseline: 2; LOAD-BEARING GATE)
[pr-y22-test] F0044 batch max `[twin-oracle] unpaired_count`: Some(0)
                                  (regression guard: must stay <= 0)
ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

**Both gates GREEN.** F0020 [topo-extract]: 8 → 0 (load-bearing assertion 1).
F0020 [twin-oracle]: 2 ≤ 2 (regression-guard assertion 2). F0044 batch
[topo-extract]: 2 → 0. F0044 batch [twin-oracle]: 0 ≤ 0. Behavior matches
spec §6 gating criteria exactly.

---

## §3 Gate 3 + 4: corpus + cohort sweep

### Gate 3 — Yang fast corpus

```
Yang fast: 10/157 passed, 143 failed, 4 errored (skipped 33 known timeouts)
```

**10/157 — UNCHANGED from baseline.** No corpus progression but also no
corpus regression. F0051 did not return (banked per canary §5 + spec §6
informational); cohort siblings did not cross. This is consistent with
spec §6 informational ("F0020 spotlight Status:Failed MAY persist —
downstream tessellation NMM-render layer banked from PR-Y21 ABORT");
PR-Y22's targeted layer (`[topo-extract] summary unpaired=N`) is the
load-bearing gate, not status-flip count.

### Gate 4 — Cohort siblings (per-case)

| Case  | Status   | Notes                                                        |
|-------|----------|--------------------------------------------------------------|
| F0030 | Failed   | watertight_mesh: 12 unpaired / 66; Euler V-E+F=3 (unchanged) |
| F0044 | Failed   | watertight_mesh: 12 unpaired / 180; outward 51.7%            |
| F0045 | Failed   | watertight_mesh: 38 unpaired / 472                           |
| R0092 | Failed   | watertight_mesh: 43 unpaired / 280                           |
| F0050 | Failed   | watertight_mesh: 39 unpaired / 417 (different defect)        |

All status outcomes consistent with spec §6 expectation (different defect
classes; F0030 stays clean at the M1+M2 layer per canary §2; F0050 stays
Failed at Euler/normal layer; F0044 batch [topo-extract]=0 per §2 above).

---

## §4 Gate 5: F0020 layer-N characterization

F0020 spotlight Status:Failed with verbatim error string from
`spotlight_f0020 --ignored --nocapture` Boolean #2 (Extrude 3):

```
[topo-extract] summary: paired=65, unpaired=0, ambiguous=0   ← PR-Y22 GREEN
[yang-diag] flood_fill_patches: 39 unpaired HEs out of 169 total
[twin-oracle] total_directed_edges=169
[twin-oracle] unpaired_count=2
[twin-oracle] offender he=58 twin=-3 twin.twin=-3 origin=v27 dest=v38
[twin-oracle] offender he=59 twin=-3 twin.twin=-3 origin=v38 dest=v27
[A15.6] Yang boolean pipeline failed (not falling through):
  half_edge[58].twin = None but arena contains a HE for the reverse
  direction (38->27) — this is a missing-edge defect (Yang Step 6/7
  boundary-classification dropped the reverse), not a legitimate
  non-manifold edge. Banked PR-Y21+.
```

**Critical finding: this is THE SAME error string as the PR-Y21 ABORT
layer.** The validator-panic `(38, 27)` reverse-in-arena MISSING-defect
predates PR-Y22 (canary-runner-9 §1 documented the F0020 [twin-oracle]=2
baseline; spec §5 + R-b test amendment explicitly bank this 2-residual
post-pairing layer for PR-Y23+).

**PR-Y22 did NOT introduce a new layer.** The PR-Y22 layer (pre-pairing
[topo-extract] summary unpaired=N) is GREEN (8 → 0). The downstream
[twin-oracle] layer is at its pre-PR baseline (2; the documented
non-PR-Y22-fixable residual). PR-Y22's load-bearing gate fired and
succeeded; the validator panic now fires on a DIFFERENT 2-edge subset
that was already documented as separate-layer.

The error string also reveals the topo-extract output is valid (paired=65,
unpaired=0) but the in-process B-Rep arena builds 169 directed edges with
2 [twin-oracle] orphans — i.e. the post-topo-extract arena step
introduces the 2-edge orphans (or doesn't fix them). This is downstream
of PR-Y22's anchor and is the rightful PR-Y23+ banked layer.

---

## §5 Gate 6: Yang §4.4.2 paper audit

Yang 2025 §4.4.2 (line 988 of `pdftotext` extract) is titled "**Mesh and
B-Rep Booleans**" — describing inside/outside classification + patch
segmentation for B-Rep Boolean assembly:

> 4.4.2 Mesh and B-Rep Booleans.
> Mesh Booleans. After trimming the meshes using the intersection
> curves, we directly apply a standard inside/outside classification
> step [Cherchi et al. 2022] to identify the triangles that need to be
> retained, thus completing the mesh Boolean operation.

**§4.4.2 does NOT contain a verbatim "manifold ↔ incidence==2"
definition.** This is a PRECISION GAP in the spec's citation: implementer-aa
(per the comment at L1300-1308) and spec §4.0 (R-a) cite "Yang §4.4.2:
manifold ↔ incidence==2; otherwise NMM" — but §4.4.2 does not contain
this definition explicitly.

**However, the underlying claim IS paper-supported, just at a different
location.** Yang 2025 §3 (Background, line 369-371) defines the B-Rep
manifold-edge contract:

> The faces are surrounded by a set of closed loops that trim the
> surface, with each loop composed of edges that form a continuous
> boundary, **with each edge shared by two adjacent faces**.

This is the canonical incidence-2 manifold-edge definition — restated for
B-Rep faces rather than mesh tris, but topologically identical. Cherchi
2022 §3 (line 367-369 of its extract) reinforces it for the arrangement
output:

> the arrangement is guaranteed to be a well formed simplicial complex
> and surface patches are bounded by closed loops of non-manifold edges,
> namely the intersection lines.

(I.e., interior patch edges are manifold = incidence-2; boundary edges
between patches are non-manifold = incidence ≥ 3.)

**Verdict:** the spec's citation `Yang §4.4.2` is **section-imprecise**
but **claim-correct**. The manifold ↔ incidence==2 predicate is
established standard topology that Yang 2025 §3 + Cherchi 2022 §3 both
support. The implementation is paper-faithful per `feedback_yang_only.md`.
**AMEND-grade** non-blocking finding for spec hygiene: replace
"Yang §4.4.2" with "Yang 2025 §3 + Cherchi 2022 §3" in the production
code comment at L1300-1308 and spec §4 in a future PR's editorial pass.
Not blocking ACCEPT.

---

## §6 Gate 7: production safety

### Kernel test parity (worktree comparison)

```
$ git worktree add /tmp/y22-baseline-wt ab65baa
HEAD is now at ab65baa

$ cd /tmp/y22-baseline-wt && cargo test -p kernel 2>&1 | tail -5
test result: FAILED. 1250 passed; 29 failed; 42 ignored; 0 measured;
                     0 filtered out; finished in 13.93s

$ cargo test -p kernel 2>&1 | tail -5      # live tree (HEAD + PR-Y22)
test result: FAILED. 1250 passed; 29 failed; 42 ignored; 0 measured;
                     0 filtered out; finished in 14.85s

$ git worktree remove /tmp/y22-baseline-wt   # cleanup
```

**Live tree: 1250 / 29 / 42. Baseline: 1250 / 29 / 42. ZERO regression.**
The 29 pre-existing failures are unchanged (they predate PR-Y22 and are
unrelated to flood_fill_patches NMM logic).

### Clippy

```
$ cargo clippy -p kernel 2>&1 | grep -c '^warning\|^error'
95
```

**95 warnings** — matches implementer-aa's reported baseline. No new
warnings introduced.

### Fmt

```
$ cargo fmt --check -p kernel 2>&1 ; echo EXIT:$?
EXIT:0
```

**Clean.** No formatting drift.

---

## §7 Self-canaried recommendation for PR-Y23+

Per `feedback_adversary_recommendations_need_canary.md`: the
recommendation below is backed by the Gate 5 verbatim error string
(empirical observation in this audit), NOT inference.

**Recommendation: PR-Y23 anchor = post-`flood_fill_patches` arena
build, between [topo-extract] summary emission (L1346) and [twin-oracle]
oracle emission (L1458).**

**Empirical evidence (Gate 5 verbatim):** the F0020 Extrude 3 stderr
shows:
- `[topo-extract] summary: paired=65, unpaired=0, ambiguous=0` (PR-Y22
  layer GREEN — output of `flood_fill_patches`)
- followed immediately by:
  `[yang-diag] flood_fill_patches: 39 unpaired HEs out of 169 total`
  (after the in-process arena materializes edges from the patch-graph
  output, 39 of 169 directed edges are unpaired)
- `[twin-oracle] unpaired_count=2` (after orphan-resolution pass: 2
  remain)

The arena materialization and orphan-resolution pass between L1346 and
L1458 is the PR-Y23 layer surface. Specifically: directed edges
`he=58 (v27→v38)` and `he=59 (v38→v27)` exist in the arena AS A PAIR
(both directions present), but BOTH report `twin=-3` (unset) and
`twin.twin=-3`. They should pair to each other. The post-flood-fill
twin-resolution code is failing to bind them despite their arena
co-presence.

**This is a NEW layer surfacing now that PR-Y22 cleared the upstream
[topo-extract] noise.** Pre-PR-Y22, the topo-extract stage emitted
unpaired=8, masking visibility into the downstream 2-residual. Post-PR-Y22,
the 2-residual is the only thing left, and is now the rightful next anchor.

**Self-canary check:** I empirically observed the SAME `(38, 27)` error
string as the PR-Y21 ABORT layer (per Gate 5). The ABORT memo (banked at
`yang_pr_y22_mode_a_missing.md` §1 line 33-37) framed this as
"downstream tessellation NMM-render layer." That framing may be
imprecise — the empirical evidence here is that the panic comes from
**`A15.6` validator** (B-Rep arena post-build), not from tessellation/
render. R-c implementer for PR-Y23 should re-anchor based on direct
arena-build inspection between L1346 and L1458, NOT inherit the "NMM-
render" framing.

**Banked NOT recommended:** I did NOT run a probe to localize the exact
arena-build line that drops the (38, 27) twin pairing. That probe is
PR-Y23 R-0 work; this self-canary stops at "the layer is
post-topo-extract / pre-twin-oracle, with the arena seeing both
directions but failing to pair." Per
`feedback_adversary_recommendations_need_canary.md`: I declare this an
EMPIRICAL anchor candidate, not a definitive anchor.

---

## §8 Verdict: ACCEPT

**All gates GREEN:**
- Gate 1 (regression): both tests PASS; F0020 [topo-extract]=0,
  F0044 batch [topo-extract]=0; both [twin-oracle] within regression bounds.
- Gate 3 (corpus): 10/157 baseline preserved; no progression but no regression.
- Gate 4 (cohort): all expected outcomes per spec §6.
- Gate 5 (next-layer): same error string as PR-Y21 ABORT layer; PR-Y22
  did NOT introduce a new layer; [twin-oracle]=2 banked as documented.
- Gate 6 (paper): claim is paper-supported (Yang 2025 §3 + Cherchi 2022 §3);
  spec citation is section-imprecise but claim-correct.
- Gate 7 (production safety): kernel 1250/29 = baseline 1250/29 (zero
  regression); clippy 95 = baseline 95; fmt clean.

**No blocking issues. No required actions before close-out.** The PR
fully delivers the spec's load-bearing claims (M1 + M2 at the right
anchors with paper-supported semantics).

**AMEND-grade non-blocking finding** (spec hygiene only): the
"Yang §4.4.2" citation in production comments + spec §4.0/§4.1/§5 should
be edited in a future cosmetic pass to "Yang 2025 §3 + Cherchi 2022 §3"
to reflect where the manifold-edge incidence==2 definition is actually
located in the papers. NOT blocking ACCEPT.

**Routing:** team-lead may proceed to R-e close-out (clippy/fmt/WASM
rebuild/commit/push). PR-Y23+ candidate anchor banked at §7.

**Discipline-checks (per HARD CONSTRAINTS):**
- NO `git stash` used at any point
- NO `git checkout` on production files in the live working tree
- NO `git reset` on the live working tree
- Baseline comparison performed via `git worktree add /tmp/y22-baseline-wt`,
  cleaned up with `git worktree remove` post-measurement
- Live tree state preserved end-to-end (`git status` at start = `git
  status` at end except for this v2 memo)

Per `feedback_yang_only.md`: ACCEPT is paper-faithful (the predicate
extension follows the standard manifold-edge incidence==2 definition).
Per `feedback_no_last_bug.md`: this PR fixes ONE layer; the [twin-oracle]
2-residual is acknowledged as a SEPARATE layer banked for PR-Y23+, not
declared "the last bug."
