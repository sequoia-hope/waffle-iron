# PR-S3 Spec Validation — adversary mutation tests

**Author:** adversary-2 (PR-S3 Phase 2)
**Date:** 2026-05-03
**Specs validated:**
- `specs/yang_pr_y15b_pre_cherchi_input_validation.md` (460 lines, full FIP §3.2)
- `specs/yang_pr_y15a_downstream_investigation.md` (243 lines, Phase-0 investigation)
- `specs/pr_s2_corpus_inputcheck_sweep.md` (amended, 2 single-block additions)

**Empirical baseline:** `docs/audits/cherchi_inputcheck_sweep_2026-05-03.tsv` (380 rows, md5 `361799057b3fe476ca2af73eb9fdff96`)

## Overall verdict

**SHIP WITH 2 SMALL AMENDMENTS.** spec-writer-2 produced specs that are EMPIRICALLY GROUNDED (the new `parse-error` sub-class and the 9-mask distribution are exactly correct vs. my TSV) and ROBUST TO MUTATION (Mutation 1 fails coherence cleanly, Mutation 2 routes through the spec's escape hatch correctly, Mutation 3 weakens strategic intent without breaking parse). One real spec defect found (F0064 §6.5 contradiction) and one missing diagnostic angle from my findings memo (Waffle-Passing reproducer dropped). Both are 5-minute amendments by spec-writer-2.

---

## §1. Mutation results

### Mutation 1 — PR-Y15b: replace F0002 reproducer with F0031 in §1

**Result: FAILS spec coherence (= spec is correctly TIGHT).**

F0031 is `cherchi_class=valid` on BOTH sides per the TSV. PR-Y15b's I8 (line 184) demands "all 40 unique cases in PR-S2's `combined_failures` bucket" migrate to `valid`. F0031 is not in that bucket — it's already at the goal state. Substituting it as a reproducer makes the I8 check vacuously true and the §3 branch table inapplicable (mask `(0,0,0,0,0)` has no fix-shape entry).

**Conclusion:** PR-Y15b's reproducer commitment is INHERENTLY tied to the `combined_failures` bucket via I8. The spec cannot be passed off as "generic enough to apply elsewhere" — it self-rejects the wrong cohort. ✓ Spec is appropriately scoped.

### Mutation 2 — PR-Y15a: replace F0031 reproducer with F0002 in §1

**Result: PASSES spec coherence — VIA THE SPEC'S OWN ESCAPE HATCH.**

PR-Y15a §4 decision tree row 3 explicitly handles this: "F0031 is misclassified as 78%-cohort — it actually has a pre-Cherchi defect that PR-S2's `mesh_booleans_inputcheck` happened to accept (false negative on Cherchi's loose input check vs. our stricter conformal probe) → Re-bucket F0031 + cluster as F0002-class; fold the cluster into PR-Y15b's reproducer set."

F0002 substitution would land in this row (Stage A `well_formed=false`), and the Phase-0 implementer would correctly route the case to PR-Y15b. The spec ANTICIPATES the wrong-reproducer mistake and provides explicit handling.

**Conclusion:** This is the OPPOSITE of "spec too vague" — it's "spec robust to operator error." The decision tree is genuinely diagnostic, not just reproducing the original hypothesis. ✓ Strong spec design.

### Mutation 3 — PR-Y15b: weaken I8 from "all 40" to "≥30 cases"

**Result: PASSES internal coherence; UNDERMINES strategic intent.**

§6.4 already documents F0005 as a known carve-out, and §10 verification step 1 explicitly handles partial fixes ("ship as PR-Y15b partial with explicit follow-up PR-Y15b.1 for the residual cases"). So weakening to "≥30" would compose with the existing failure-mode catalog without contradiction.

HOWEVER, I8 is justified by line 192-198: "PR-Y15b cannot ship without I8 holding on the full 40-case set" because "four prior anchors (PR12, PR13, PR-Y14a/b, PR-Y14c) all produced internally-coherent fixes that reference parity later invalidated." Weakening to "≥30" gives the strategic-escalation rule a 25% looseness budget — a previously-wrong-anchor pattern could lurk in the residual 10 cases and never get caught.

**Conclusion:** PARTIAL FAIL. The spec's strategic value depends on the strict "all 40" wording. The text would still parse if weakened, but the invariant would lose its load-bearing role. ✓ Spec is correctly tight; do NOT weaken I8.

---

## §2. Verdict on the 6 ambiguity items

**Heads up to team-lead:** I do not have the actual 6-item list spec-writer-2 surfaced — please forward it. Inferring from spec text, the most likely ambiguity items are below; my verdicts may not align with what spec-writer-2 actually flagged. Treat this section as preliminary pending that list.

| # | Inferred ambiguity | Source line(s) | Verdict |
|---|---|---|---|
| 1 | "Implementer's choice" between Option A and Option B for Phase-0 reproducer harness | PR-Y15a §3.3, line 108 | **Accept.** Both options are concrete; choice is implementation-cost only. No spec gap. |
| 2 | "Spec authority is §1, §4, §10" disclaimer on the parameters table | PR-Y15b §2, line 53 | **Accept.** Distinguishes contract from courtesy; helpful guard against scope creep. |
| 3 | "MAY shift to `valid` (welcome side effect) or stay" for single-axiom non_watertight cases | PR-Y15b §4 I6, line 173 | **Accept.** Explicitly out-of-scope per §9; the "MAY" language correctly leaves room without committing. |
| 4 | F0005 carve-out — "may belong to a distinct sub-defect class" | PR-Y15b §6.4, line 245 | **Accept** with caveat: per `feedback_no_last_bug.md`, the spec correctly avoids over-claiming. |
| 5 | "Implementer SHALL re-tally from the TSV at fix-time" | PR-Y15b §3, line 71 | **Accept.** TSV is the immutable artifact (md5 pinned in this memo); re-tally is mechanical. |
| 6 | "Or a new test file" alternative location for the Phase-0 reproducer | PR-Y15a §3.3, line 99 | **Accept.** Either location works; new file is cleaner long-term. |

**My own ambiguity flag (not from spec-writer-2's list):** PR-Y15b §1 line 41-42 says "F0005 has a different Stage-A signature ... see §6.4 for handling guidance if F0005 doesn't migrate to `valid`." The "if" leaves the F0005 outcome unresolved at spec-write time, but per §6.4 either outcome is acceptable. NOT a real ambiguity, but worth noting as a potential operator confusion point.

**Recommendation:** All 6 inferred items are acceptable; none need spec amendment. If spec-writer-2's actual list differs, please forward and I'll re-verdict.

---

## §3. Verdict on the new `parse-error` class finding

**VERIFIED — spec-writer-2's discovery is empirically correct.**

I re-grepped the TSV for `parse-error` in the `cherchi_detail` column:

```
$ awk -F'\t' '$5 ~ /parse-error/' cherchi_inputcheck_sweep_2026-05-03.tsv | wc -l
9
```

The 9 parse-error rows:
- R0007 A — has explicit "WARNING: adding duplicated poly!" in detail
- R0027 A — empty detail (warning truncated past 200-char cap or cleared)
- R0031 A — empty detail
- R0063 A — has 3 explicit "duplicated poly" warnings
- R0063 B — empty detail
- R0081 B — empty detail
- R0095 A — has 4+ explicit "duplicated poly" warnings (truncated mid-warning)
- F0083 A — empty detail
- F0084 A — empty detail

All 9 are correctly classified as `combined_failures` per the test runner's parse-error fallback (test §177-181: "parse error: didn't see all 5 expected lines. Bucket as combined_failures (spec §2 'catch-all') with the raw output prefixed for diagnostic"). spec-writer-2's claim that these 9 are an M-class symptom (Cherchi's loader collapses a duplicated triangle and the loaded mesh diverges from the OBJ file → manifestly non-manifold) is the right call; the duplicated-poly emission DOES happen at OBJ load time before the check phase, so the standard 5-line output never arrives.

**However:** 6 of the 9 parse-error rows have EMPTY parse-error details (just `parse-error: ` with nothing after). spec-writer-2 attributes these all to the "duplicated poly" class, but I cannot confirm that — the empty-detail rows might represent a different parse failure mode (e.g., Cherchi crashed on load with an error written to a file descriptor we didn't capture). PR-Y15b's I8 ("all 40 cases migrate to `valid`") would catch this either way (if the underlying issue is duplicated-poly OR a crash, both would resolve when the upstream tessellation defect is fixed), so this is not a blocker.

**Recommendation for spec-writer-2:** Add a single sentence to PR-Y15b §3's parse-error row noting that 3 of 9 cases have empirically-confirmed "duplicated poly" warnings; the other 6 have empty parse-error details and may represent an additional sub-defect class. This avoids over-claiming uniform causation. ~1 line edit.

### Mask distribution verification (PR-Y15b §3 branch table)

I tallied the 51 `combined_failures` rows from the TSV by exact 5-bit mask (parsing `cherchi_detail`):

| Spec table mask | Spec count | TSV count | Match |
|---|---:|---:|---|
| `(0,1,0,0,1)` W+I | 15 | 15 | ✓ |
| `(1,1,0,0,1)` M+W+I | 9 | 9 | ✓ |
| `parse-error` | 9 | 9 | ✓ |
| `(0,1,1,0,1)` W+LO+I | 6 | 6 | ✓ |
| `(1,1,1,0,1)` M+W+LO+I | 5 | 5 | ✓ |
| `(1,1,1,1,1)` all five | 3 | 3 | ✓ |
| `(1,1,0,0,0)` M+W only | 2 | 2 | ✓ |
| `(0,1,1,0,0)` W+LO only | 1 | 1 | ✓ |
| `(1,1,0,1,1)` M+W+GO+I | 1 | 1 | ✓ |
| **TOTAL** | **51** | **51** | ✓ |

Every cell matches exactly. Spec-writer-2's branch table is grounded in empirical data, not invented.

---

## §4. Reproducer alignment with my PR-S2 findings memo

**DIVERGENCE found, but defensible.** My memo §6 "Recommended PR-S3 deliverables" listed PR-S3a reproducers as: **F0002, F0004 (combined_failures), F0003 (non_watertight), F0008 (non_watertight, Waffle-Passing)**.

spec-writer-2 chose PR-Y15b reproducers: **F0002, F0004, F0005, F0006 (all combined_failures, F-clean) + R0014, R0015, R0017 (R-cases, all combined_failures)**.

**Differences:**
1. **My F0003 + F0008 dropped.** Both are `non_watertight` (single-axiom W failure), not `combined_failures`. PR-Y15b's I8 specifically targets the `combined_failures` bucket, so spec-writer-2's drop is consistent with the I8 scope. **My intent in including F0003/F0008** was to surface the surprising "Waffle-Passing case ships leaky pre-Cherchi mesh" diagnostic — it's a `pass-boss-only` case where Waffle considers the result OK because no boolean ran, but the underlying mesh is still bad. spec-writer-2 explicitly addresses this in §9 ("18 `non_watertight` single-axiom cases ... May shift to `valid` as a side effect (welcome) or stay (out of scope; PR-Y15c handles)"). LOSS: the diagnostic angle is now deferred to PR-Y15c, not used as an early-warning probe in PR-Y15b.
2. **F0005 added by spec-writer-2.** §1 line 47 explicitly notes F0005's distinct Stage-A signature (16 unpaired + 153 multi vs F0002's 0 + 50). My memo §1 also flagged F0005 as `combined_failures`, but I didn't explicitly call it out as a separate signature class. spec-writer-2's inclusion + §6.4 carve-out is more rigorous. ✓ Improvement.
3. **R-cases added** (R0014, R0015, R0017). My memo listed them but didn't elevate them to canonical reproducers. spec-writer-2 picks R-cases that are SIDE-ASYMMETRIC (one side `combined_failures`, one side `valid`) — those are the cleanest diagnostic for "the fix touched only one side, did the OTHER stay valid?" ✓ Improvement.

**Net assessment:** spec-writer-2's reproducer set is a STRICT IMPROVEMENT over my recommendation, except for the lost F0003/F0008 diagnostic angle. Recommend adding F0003 to PR-Y15b's "spot reproducers for development" list as a *control* (Waffle-Passing case with bad pre-Cherchi mesh — verifies the fix doesn't break Waffle-Passing cases). ~1 line edit.

---

## §5. Real spec defects found (require amendment)

### Defect 1: F0064 contradiction in PR-Y15b §6.5 (line 250)

§6.5 says: "The 4 `bad_orientation` cases (F0064, F0065, F0066, F0071) are NOT in the `combined_failures` bucket."

But the TSV says:

```
F0064  A  Failed  combined_failures  Manifold check: passed;Watertight check: failed;Local  Orientation check: passed;Global Orientation check: passed;Intersection check: failed
F0064  B  Failed  bad_orientation    Manifold check: passed;Watertight check: passed;Local  Orientation check: failed;Global Orientation check: passed;Intersection check: passed
```

**F0064-A IS in combined_failures (W+I).** Only F0064-B is `bad_orientation`. F0065/F0066/F0071 also have asymmetric sides (A=non_watertight, B=bad_orientation), and §6.5 misses this entirely.

The spec's own I6 (line 90) handles asymmetric cases correctly — "the side-asymmetric cases have one side `combined_failures` and the other in another bucket — implementer SHALL also fix the non-`combined_failures` side if it shifts during the fix" — so the §6.5 statement is internally inconsistent with I6.

**Recommended amendment:** §6.5 line 250-253 should say: "The 4 cases (F0064, F0065, F0066, F0071) include `bad_orientation` failures on side B; sides A are in `combined_failures` (F0064 W+I), `non_watertight` (F0065/F0066/F0071 W only), and PR-Y15b's I6 already commits to handling the asymmetry. Side B's `bad_orientation` failure is OUT OF SCOPE for PR-Y15b per §1's reproducer set, and remains for PR-Y15c if it doesn't resolve as a side effect."

This is the only defect that needs amendment to ship.

### Defect 2: §3 parse-error sub-claim over-attribution (per §3 above)

Recommended amendment: PR-Y15b §3 row "parse-error" (line 79). Add: "Empirically, 3 of 9 parse-error rows in the PR-S2 TSV contain an explicit `WARNING: adding duplicated poly!` from Cherchi's OBJ loader; the other 6 have empty parse-error details and may represent additional sub-defect modes (e.g., loader crash, output-format divergence). All 9 fall under the M+W+I fix-shape per the manifold-class symptom analysis above; the residual 6 may reveal additional sub-cases when re-tallied post-fix."

### Defect 3 (minor): missing F0003/F0008 diagnostic angle (per §4 above)

Recommended addition: PR-Y15b §1 line 45 (spot reproducers list). Add: "F0003 (`pass-boss-only`, both sides `non_watertight`) as a CONTROL case — verifies the fix doesn't break Waffle-Passing cases that nonetheless ship leaky pre-Cherchi meshes."

---

## Summary verdict

**SHIP WITH AMENDMENTS** — 2 required (Defect 1 F0064 fix, Defect 2 parse-error attribution), 1 nice-to-have (Defect 3 control reproducer). All three are 1-3 line spec edits by spec-writer-2; none change the spec's structure or scope.

The mutation tests confirm the specs are TIGHT, not vague:
- M1 fails coherence cleanly (PR-Y15b is bucket-bound via I8)
- M2 routes through the spec's own escape hatch (PR-Y15a §4 row 3 explicitly handles wrong-reproducer)
- M3 would parse but undermine strategic intent (I8's load-bearing role depends on "all 40")

The new parse-error class finding is empirically grounded (9 rows verified). The mask distribution table matches the TSV exactly (every cell). Reproducer alignment with my findings memo is a strict improvement except for one lost diagnostic angle.

**Recommended team-lead action:** Loop spec-writer-2 with the 3-item amendment list above, then proceed to Phase 3.
