# Review/Cleanup Pass — 2026-04-05

**Branch**: `auto-waffle/2026-04-05T03-22-56`
**Commits reviewed**: 079e139, 198b588, 803b767, 0911511 (+ ae2f9e1, 74467f5)
**Governance docs referenced**: Constitution (P1, P9, P10), Invariants (A14.3, A15.6), DoD, FIP

---

## Verdict: PASS — No active governance violations

The auditor reviewed all recent kernel commits, grepped for hardcoded tolerances,
and searched for workaround patterns. The codebase is governance-compliant.

---

## Commit Review

| Hash | Message | Verdict | Notes |
|------|---------|---------|-------|
| `079e139` | feat(kernel): improve Yang pipeline correctness and performance (A15.6) | FLAG (corrected) | Introduced Yang→S-H timeout fallback. **A15.6 violation**. Fixed by 198b588. |
| `198b588` | audit(kernel): enforce A15.6 — Yang errors must not fall back to legacy S-H | PASS | Corrects 079e139's fallback. Only allows fallthrough for env-var gate ("not enabled"). |
| `803b767` | fix(kernel): resolve T-junction splitting after coplanar face group merge | PASS | Two root-cause fixes for T-junction bugs. Good test oracles (Euler formula V-E+F=2). |
| `0911511` | fix(kernel): pre-deduplicate per-face vertices before Yang mesh boolean | PASS | Targeted fix for per-face vertex duplication. Uses QUANT_NANOMETER_SCALE from units.rs. |
| `ae2f9e1` | refactor: tighten auto-waffle plan+execute for 60-minute focused sessions | PASS | Non-modeling change. |
| `74467f5` | fix: require TeamCreate for agent teams, not anonymous sub-agents | PASS | Non-modeling change. |

### Historical violation (already corrected)

Commit 079e139 added a Yang pipeline timeout (legitimate performance engineering) but
the dispatch in `waffle_kernel.rs` silently caught the timeout error and fell through
to the deprecated S-H pipeline. This violates A15.6 ("Yang errors must fail, not
silently degrade to the broken legacy path"). Commit 198b588 corrected this 5 hours
later by gating fallthrough strictly to the env-var check (`"not enabled"` only).

**No revert needed** — the violation was self-corrected in the same session.

---

## Tolerance Audit (A14.3)

**Result**: PASS — No violations found.

All `1e-` values in `crates/kernel/src/` (outside test files and units.rs) are either:
- Computed from units.rs constants (e.g., `TAU_WORK.sqrt()`)
- Documentation comments only
- Derived from `BooleanOptions` or `QUANT_NANOMETER_SCALE`

No hardcoded epsilons (`0.0001`, `0.001`, `EPSILON`) found outside the central policy.

---

## Workaround Detection (P9)

**Result**: PASS — No active P9 violations.

All "fallback" hits belong to:
- Deprecated S-H pipeline (correctly marked deprecated, not invested in)
- Legitimate incomplete-SSI polygon fallback (documented, not tolerance escalation)
- Internal tessellation repair (not boolean pipeline bypass)

No instances of: "workaround", "accept.*invalid", "bypass", "skip.*validation",
"tolerance.*widen" in active (non-deprecated) code.

---

## Test Oracle Quality (P1)

**Result**: PASS — All new tests use numeric/structural oracles.

Recent test additions verify:
- Vertex counts after deduplication (`verts.len() == 4`)
- Triangle count lower bounds (`result_tri_count >= 10`)
- Euler formula (V-E+F=2) for topological validity
- Normal vector unit length within f32 precision (`1e-5`)
- Ray-cast parity for interior point classification
- Error propagation on timeout (`result.is_err()`)

No "just check it doesn't panic" tests found.

---

## Actions Taken

- **Reverts**: None needed (no active violations)
- **Tolerance centralization**: None needed (already compliant)
- **Test strengthening**: None needed (already has numeric oracles)
- **Dead code removal**: None (deprecated S-H code retained per A15.6 migration plan)
- **Documentation updates**: None needed

---

## Recommendations for Future Sessions

1. **079e139 pattern to watch**: When adding timeout/performance mechanisms to the Yang
   pipeline, always verify the dispatch layer propagates errors correctly. The timeout
   itself was good engineering; the fallback dispatch was the governance violation.

2. **Assay score tracking**: Commits 803b767 and 0911511 improve Yang pipeline correctness
   but the assay pass rate isn't captured in commit messages. Consider adding assay
   deltas to commit messages for traceability.

3. **Phase 5 (B-Rep reassembly)**: Per CLAUDE.md, this produces invalid topology. The
   recent fixes (T-junction, vertex dedup) are incremental improvements but the core
   Phase 5 issue remains the top priority.
